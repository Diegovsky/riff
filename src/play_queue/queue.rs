use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use riff_api::models::{Page, RepeatMode, Track};

use super::context::{Context, CONTEXT_PAGE_SIZE};
use super::{EntryKey, PageRequest, SongsSource};

#[derive(Clone, Debug)]
struct Queued {
    uid: u64,
    track: Track,
}

#[derive(Clone, Debug)]
enum Now {
    Nothing,
    Context,
    Queued(Box<Queued>),
}

const DEFAULT_SEPARATION: usize = 2;

fn next_shuffled_loop(loop_order: &[usize], separation: usize, rng: &mut SmallRng) -> Vec<usize> {
    let n = loop_order.len();
    let separation = separation.min(n.saturating_sub(1));
    let min_position = |track: usize| {
        loop_order
            .iter()
            .rev()
            .take(separation)
            .position(|&t| t == track)
            .map_or(0, |e| separation - e)
    };
    let mut rest = loop_order.to_vec();
    let mut next = Vec::with_capacity(n);
    for j in 0..separation {
        let allowed: Vec<usize> = (0..rest.len())
            .filter(|&i| min_position(rest[i]) <= j)
            .collect();
        next.push(rest.swap_remove(*allowed.choose(rng).unwrap()));
    }
    rest.shuffle(rng);
    next.extend(rest);
    next
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Current,
    Queued(u64),
    Context(usize),
    Wrap(usize),
}

#[derive(Debug)]
pub struct QueueView {
    pub queued: Vec<(EntryKey, Track)>,
    pub context: Vec<(EntryKey, Track)>,
}

#[derive(Debug)]
pub struct PlayQueue {
    context: Context,
    order: Vec<usize>,
    cursor: Option<usize>,
    next_order: Vec<usize>,
    separation: usize,
    queued: Vec<Queued>,
    now: Now,
    shuffled: bool,
    next_uid: u64,
    rng: SmallRng,
}

impl Default for PlayQueue {
    fn default() -> Self {
        Self::with_rng(SmallRng::from_entropy())
    }
}

impl PlayQueue {
    fn with_rng(rng: SmallRng) -> Self {
        Self {
            context: Context::default(),
            order: vec![],
            cursor: None,
            next_order: vec![],
            separation: DEFAULT_SEPARATION,
            queued: vec![],
            now: Now::Nothing,
            shuffled: false,
            next_uid: 0,
            rng,
        }
    }

    pub fn source(&self) -> Option<&SongsSource> {
        self.context.source.as_ref()
    }

    pub fn is_shuffled(&self) -> bool {
        self.shuffled
    }

    fn cursor_index(&self) -> Option<usize> {
        self.cursor.map(|c| self.order[c])
    }

    pub fn current_context_index(&self) -> Option<usize> {
        matches!(self.now, Now::Context)
            .then(|| self.cursor_index())
            .flatten()
    }

    pub fn current(&self) -> Option<&Track> {
        match &self.now {
            Now::Queued(entry) => Some(&entry.track),
            _ => self.context.get(self.current_context_index()?),
        }
    }

    pub fn current_key(&self) -> Option<EntryKey> {
        match &self.now {
            Now::Queued(entry) => Some(EntryKey::Queued(entry.uid)),
            _ => self.current_context_index().map(EntryKey::Context),
        }
    }

    pub fn header_track(&self) -> Option<&Track> {
        self.current()
            .or_else(|| self.queued.first().map(|e| &e.track))
    }

    fn upcoming_start(&self) -> usize {
        self.cursor.map_or(0, |c| c + 1)
    }

    fn upcoming_order(&self) -> &[usize] {
        &self.order[self.upcoming_start()..]
    }

    fn next_loop(&self) -> &[usize] {
        if self.shuffled {
            &self.next_order
        } else {
            &self.order
        }
    }

    fn plan_next_loop(&mut self) {
        self.next_order = if self.shuffled {
            next_shuffled_loop(&self.order, self.separation, &mut self.rng)
        } else {
            vec![]
        };
    }

    pub fn set_separation(&mut self, separation: usize) {
        if self.separation != separation {
            self.separation = separation;
            self.plan_next_loop();
        }
    }

    fn track_at(&self, step: Step) -> Option<&Track> {
        match step {
            Step::Current => self.current(),
            Step::Queued(uid) => Some(&self.queued[self.queued_index(uid)?].track),
            Step::Context(k) => self.context.get(*self.order.get(k)?),
            Step::Wrap(k) => self.context.get(*self.next_loop().get(k)?),
        }
    }

    pub fn set_context(&mut self, source: Option<SongsSource>, tracks: Vec<Track>) {
        self.reset_context(source);
        self.context.set_all(tracks);
        self.rebuild_order(None);
    }

    pub fn add_page(&mut self, source: SongsSource, page: Page<Track>) -> bool {
        if self.source() == Some(&source) {
            let added = self.context.add_page(page);
            self.integrate(added);
            return false;
        }
        self.reset_context(Some(source));
        self.context.add_page(page);
        self.rebuild_order(None);
        true
    }

    pub fn clear(&mut self) {
        self.reset_context(None);
        self.clear_queued();
    }

    pub fn clear_queued(&mut self) -> bool {
        let changed = !self.queued.is_empty();
        self.queued.clear();
        changed
    }

    fn reset_context(&mut self, source: Option<SongsSource>) {
        self.context = Context::new(source);
        self.order.clear();
        self.cursor = None;
        self.now = Now::Nothing;
    }

    fn rebuild_order(&mut self, first: Option<usize>) {
        let mut order: Vec<usize> = self.context.loaded().collect();
        if self.shuffled {
            order.shuffle(&mut self.rng);
            if let Some(pos) = first.and_then(|f| order.iter().position(|&i| i == f)) {
                order.swap(0, pos);
            }
        }
        self.cursor = first.and_then(|f| order.iter().position(|&i| i == f));
        self.order = order;
        self.plan_next_loop();
    }

    fn integrate(&mut self, added: Vec<usize>) {
        for i in added {
            let pos = if self.shuffled {
                let pos = self.rng.gen_range(self.upcoming_start()..=self.order.len());
                let from = self.separation.min(self.next_order.len());
                let at = self.rng.gen_range(from..=self.next_order.len());
                self.next_order.insert(at, i);
                pos
            } else {
                self.order.partition_point(|&j| j < i)
            };
            self.order.insert(pos, i);
            if let Some(c) = self.cursor.as_mut().filter(|c| pos <= **c) {
                *c += 1;
            }
        }
    }

    pub fn set_shuffled(&mut self, shuffled: bool) {
        if self.shuffled == shuffled {
            return;
        }
        self.shuffled = shuffled;
        self.rebuild_order(self.cursor_index());
    }

    pub fn next_page_request(&self, repeat: RepeatMode, for_view: bool) -> Option<PageRequest> {
        if self.shuffled {
            return self.context.missing_page(0, None);
        }
        let Some(from) = self.cursor_index() else {
            return for_view
                .then(|| self.context.missing_page(0, None))
                .flatten();
        };
        let (request, ahead) = match self.context.missing_page(from, None) {
            Some(request) => (request, request.offset.saturating_sub(from + 1)),
            None if repeat == RepeatMode::Context => {
                let request = self.context.missing_page(0, Some(from))?;
                (request, self.upcoming_order().len() + request.offset)
            }
            None => return None,
        };
        (for_view || ahead < CONTEXT_PAGE_SIZE / 2).then_some(request)
    }

    fn next_steps(&self, auto: bool, repeat: RepeatMode) -> Vec<Step> {
        let mut steps = vec![];
        if auto && repeat == RepeatMode::Track {
            steps.push(Step::Current);
        }
        steps.extend(self.queued.iter().map(|e| Step::Queued(e.uid)));
        steps.extend((self.upcoming_start()..self.order.len()).map(Step::Context));
        if repeat == RepeatMode::Context {
            steps.extend((0..self.next_loop().len()).map(Step::Wrap));
        }
        steps
    }

    fn prev_steps(&self, repeat: RepeatMode) -> Vec<Step> {
        let Some(cursor) = self.cursor else {
            return vec![];
        };
        let last = match self.now {
            Now::Context => cursor,
            _ => cursor + 1,
        };
        let mut steps: Vec<Step> = (0..last).rev().map(Step::Context).collect();
        if repeat == RepeatMode::Context {
            steps.extend(
                (last..self.order.len())
                    .rev()
                    .filter(|&k| k != cursor)
                    .map(Step::Context),
            );
        }
        steps
    }

    fn first_playable(&self, steps: &[Step], skip: &impl Fn(&Track) -> bool) -> Option<Step> {
        steps
            .iter()
            .copied()
            .find(|&step| self.track_at(step).is_some_and(|t| !skip(t)))
    }

    fn queued_index(&self, uid: u64) -> Option<usize> {
        self.queued.iter().position(|e| e.uid == uid)
    }

    fn take_queued(&mut self, uid: u64) -> Option<Queued> {
        let i = self.queued_index(uid)?;
        Some(self.queued.remove(i))
    }

    fn go_to(&mut self, step: Step) {
        match step {
            Step::Current => {}
            Step::Queued(uid) => {
                if let Some(entry) = self.take_queued(uid) {
                    self.now = Now::Queued(Box::new(entry));
                }
            }
            Step::Context(k) => {
                self.cursor = Some(k);
                self.now = Now::Context;
            }
            Step::Wrap(k) => {
                if self.shuffled {
                    self.order = std::mem::take(&mut self.next_order);
                    self.plan_next_loop();
                }
                self.cursor = Some(k);
                self.now = Now::Context;
            }
        }
    }

    pub fn peek_next(
        &self,
        auto: bool,
        repeat: RepeatMode,
        skip: impl Fn(&Track) -> bool,
    ) -> Option<Track> {
        let step = self.first_playable(&self.next_steps(auto, repeat), &skip)?;
        self.track_at(step).cloned()
    }

    pub fn has_next(&self, repeat: RepeatMode) -> bool {
        !self.next_steps(false, repeat).is_empty()
    }

    pub fn advance(
        &mut self,
        auto: bool,
        repeat: RepeatMode,
        skip: impl Fn(&Track) -> bool,
    ) -> Option<Track> {
        let steps = self.next_steps(auto, repeat);
        let Some(step) = self.first_playable(&steps, &skip) else {
            self.now = Now::Nothing;
            return None;
        };
        for skipped in steps.iter().take_while(|s| **s != step) {
            if let Step::Queued(uid) = skipped {
                self.take_queued(*uid);
            }
        }
        self.go_to(step);
        self.current().cloned()
    }

    pub fn go_back(&mut self, repeat: RepeatMode, skip: impl Fn(&Track) -> bool) -> Option<Track> {
        let step = self.first_playable(&self.prev_steps(repeat), &skip)?;
        self.go_to(step);
        self.current().cloned()
    }

    pub fn play_id(&mut self, id: &str) -> bool {
        let context = &self.context;
        let Some(i) = context
            .loaded()
            .find(|&i| context.get(i).is_some_and(|t| t.rri.id == id))
        else {
            return false;
        };
        if self.shuffled {
            self.rebuild_order(Some(i));
        } else {
            self.cursor = self.order.iter().position(|&j| j == i);
        }
        self.now = Now::Context;
        self.cursor.is_some()
    }

    pub fn play_key(&mut self, key: EntryKey) -> bool {
        if self.current_key() == Some(key) {
            return false;
        }
        let position = |order: &[usize], i| order.iter().position(|&j| j == i);
        let step = match key {
            EntryKey::Queued(uid) => Some(Step::Queued(uid)),
            EntryKey::Context(i) => position(&self.order, i).map(Step::Context),
            EntryKey::NextLoop(i) => position(self.next_loop(), i).map(Step::Wrap),
        };
        let Some(step) = step.filter(|&step| self.track_at(step).is_some()) else {
            return false;
        };
        self.go_to(step);
        true
    }

    pub fn stop(&mut self) {
        self.now = Now::Nothing;
    }

    pub fn enqueue(&mut self, tracks: Vec<Track>) {
        for track in tracks {
            self.next_uid += 1;
            self.queued.push(Queued {
                uid: self.next_uid,
                track,
            });
        }
    }

    pub fn remove(&mut self, keys: &[EntryKey]) -> bool {
        let removed = keys
            .iter()
            .filter_map(|key| match key {
                EntryKey::Queued(uid) => self.take_queued(*uid),
                EntryKey::Context(_) | EntryKey::NextLoop(_) => None,
            })
            .count();
        removed > 0
    }

    pub fn move_queued(&mut self, key: EntryKey, to: usize) -> bool {
        let EntryKey::Queued(uid) = key else {
            return false;
        };
        let Some(from) = self.queued_index(uid) else {
            return false;
        };
        let to = if to > from { to - 1 } else { to };
        let to = to.min(self.queued.len() - 1);
        if to == from {
            return false;
        }
        let entry = self.queued.remove(from);
        self.queued.insert(to, entry);
        true
    }

    pub fn upcoming(&self) -> Vec<&Track> {
        let queued = self.queued.iter().map(|e| &e.track);
        let context = self
            .upcoming_order()
            .iter()
            .filter_map(|&i| self.context.get(i));
        self.current()
            .into_iter()
            .chain(queued)
            .chain(context)
            .collect()
    }

    pub fn view(&self, repeat: RepeatMode) -> QueueView {
        let queued = self
            .queued
            .iter()
            .map(|e| (EntryKey::Queued(e.uid), e.track.clone()))
            .collect();
        let upcoming = self.upcoming_order();
        let next_loop_shown = if repeat == RepeatMode::Context {
            let gone = self.order.len() - upcoming.len();
            let current = matches!(self.now, Now::Context) as usize;
            gone.saturating_sub(current).min(self.next_loop().len())
        } else {
            0
        };
        let this_loop = upcoming.iter().map(|&i| (EntryKey::Context(i), i));
        let next_loop = self.next_loop()[..next_loop_shown]
            .iter()
            .map(|&i| (EntryKey::NextLoop(i), i));
        let context = this_loop
            .chain(next_loop)
            .filter_map(|(key, i)| Some((key, self.context.get(i)?.clone())))
            .collect();
        QueueView { queued, context }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::models::make_track;

    fn queue() -> PlayQueue {
        PlayQueue::with_rng(SmallRng::seed_from_u64(0))
    }

    fn tracks<S: AsRef<str>>(ids: &[S]) -> Vec<Track> {
        ids.iter().map(|id| make_track(id.as_ref())).collect()
    }

    fn numbered(n: usize) -> Vec<String> {
        (0..n).map(|i| i.to_string()).collect()
    }

    fn album<S: AsRef<str>>(q: &mut PlayQueue, ids: &[S]) {
        q.set_context(Some(SongsSource::Album("a".into())), tracks(ids));
    }

    fn playing<S: AsRef<str>>(ids: &[S], current: &str) -> PlayQueue {
        let mut q = queue();
        album(&mut q, ids);
        assert!(q.play_id(current));
        q
    }

    fn shuffled<S: AsRef<str>>(ids: &[S], current: &str) -> PlayQueue {
        let mut q = queue();
        album(&mut q, ids);
        q.set_shuffled(true);
        assert!(q.play_id(current));
        q
    }

    fn page(offset: usize, len: usize, total: Option<usize>) -> Page<Track> {
        Page {
            items: tracks(&numbered(offset + len)[offset..]),
            offset: Some(offset),
            total,
            next_cursor: None,
        }
    }

    fn no_skip(_: &Track) -> bool {
        false
    }

    fn id(track: Option<Track>) -> Option<String> {
        track.map(|t| t.rri.id)
    }

    fn ids(rows: &[(EntryKey, Track)]) -> Vec<String> {
        rows.iter().map(|(_, t)| t.rri.id.clone()).collect()
    }

    fn current_id(q: &PlayQueue) -> Option<String> {
        q.current().map(|t| t.rri.id.clone())
    }

    fn queued_ids(q: &PlayQueue) -> Vec<String> {
        ids(&q.view(RepeatMode::Off).queued)
    }

    fn queued_keys(q: &PlayQueue) -> Vec<EntryKey> {
        q.view(RepeatMode::Off)
            .queued
            .iter()
            .map(|(k, _)| *k)
            .collect()
    }

    fn context_ids(q: &PlayQueue, repeat: RepeatMode) -> Vec<String> {
        ids(&q.view(repeat).context)
    }

    fn order_ids(q: &PlayQueue) -> Vec<String> {
        q.order
            .iter()
            .map(|&i| q.context.get(i).unwrap().rri.id.clone())
            .collect()
    }

    fn upcoming_ids(q: &PlayQueue) -> Vec<String> {
        q.upcoming().iter().map(|t| t.rri.id.clone()).collect()
    }

    fn is_empty(q: &PlayQueue) -> bool {
        q.current().is_none()
            && queued_ids(q).is_empty()
            && context_ids(q, RepeatMode::Off).is_empty()
    }

    fn next(q: &mut PlayQueue) -> Option<String> {
        id(q.advance(false, RepeatMode::Off, no_skip))
    }

    fn ended(q: &mut PlayQueue, repeat: RepeatMode) -> Option<String> {
        id(q.advance(true, repeat, no_skip))
    }

    fn play_all(q: &mut PlayQueue) -> Vec<String> {
        std::iter::from_fn(|| next(q)).collect()
    }

    fn ended_times(q: &mut PlayQueue, repeat: RepeatMode, n: usize) -> Vec<String> {
        (0..n).filter_map(|_| ended(q, repeat)).collect()
    }

    fn request(q: &PlayQueue, repeat: RepeatMode, for_view: bool) -> Option<usize> {
        q.next_page_request(repeat, for_view).map(|r| r.offset)
    }

    #[test]
    fn test_plays_context_in_order() {
        let mut q = playing(&["1", "2", "3"], "1");
        assert_eq!(play_all(&mut q), ["2", "3"]);
        assert_eq!(current_id(&q), None);
    }

    #[test]
    fn test_queued_tracks_play_first_and_are_consumed() {
        let mut q = playing(&["1", "2", "3"], "1");
        q.enqueue(tracks(&["a", "b"]));
        q.enqueue(tracks(&["c"]));
        assert_eq!(queued_ids(&q), ["a", "b", "c"]);

        next(&mut q);
        assert_eq!(current_id(&q).as_deref(), Some("a"));
        assert_eq!(queued_ids(&q), ["b", "c"]);
        assert_eq!(play_all(&mut q), ["b", "c", "2", "3"]);
    }

    #[test]
    fn test_enqueue_with_nothing_playing() {
        let mut q = queue();
        q.enqueue(tracks(&["a", "b"]));
        assert_eq!(id(q.header_track().cloned()).as_deref(), Some("a"));
        assert_eq!(ended_times(&mut q, RepeatMode::Context, 3), ["a", "b"]);
    }

    #[test]
    fn test_new_context_keeps_user_queue() {
        let mut q = playing(&["1", "2"], "1");
        q.enqueue(tracks(&["a"]));
        q.set_context(Some(SongsSource::Album("b".into())), tracks(&["x", "y"]));
        q.play_id("x");
        assert_eq!(queued_ids(&q), ["a"]);
        assert_eq!(order_ids(&q), ["x", "y"]);
    }

    #[test]
    fn test_remove_only_takes_out_that_queued_track() {
        let mut q = playing(&["1", "2"], "1");
        q.enqueue(tracks(&["a", "a", "a"]));
        let keys = queued_keys(&q);
        assert!(q.remove(&[keys[1]]));
        assert_eq!(queued_keys(&q), [keys[0], keys[2]]);
        assert!(!q.remove(&[EntryKey::Context(1)]));
        assert_eq!(order_ids(&q), ["1", "2"]);
    }

    #[test]
    fn test_move_queued() {
        let mut q = playing(&["1"], "1");
        q.enqueue(tracks(&["a", "b", "c", "d"]));
        let keys = queued_keys(&q);

        assert!(q.move_queued(keys[0], 3));
        assert_eq!(queued_ids(&q), ["b", "c", "a", "d"]);
        assert!(q.move_queued(keys[3], 0));
        assert_eq!(queued_ids(&q), ["d", "b", "c", "a"]);
        assert!(q.move_queued(keys[1], 9));
        assert_eq!(queued_ids(&q), ["d", "c", "a", "b"]);

        assert!(!q.move_queued(keys[2], 1));
        assert!(!q.move_queued(keys[2], 2));
        assert!(!q.move_queued(EntryKey::Context(0), 0));
        assert!(!q.move_queued(EntryKey::Queued(999), 0));
        assert_eq!(queued_ids(&q), ["d", "c", "a", "b"]);
    }

    #[test]
    fn test_clear() {
        assert!(is_empty(&queue()));
        assert!(queue().header_track().is_none());

        let mut q = playing(&["1", "2"], "1");
        q.enqueue(tracks(&["a", "b"]));
        assert!(q.clear_queued());
        assert!(queued_ids(&q).is_empty());
        assert_eq!(current_id(&q).as_deref(), Some("1"));
        assert_eq!(order_ids(&q), ["1", "2"]);

        q.enqueue(tracks(&["a"]));
        q.clear();
        assert!(is_empty(&q));
        assert!(q.source().is_none());
    }

    #[test]
    fn test_view_and_upcoming_leave_out_played_tracks() {
        let mut q = playing(&["1", "2", "3", "4"], "2");
        q.enqueue(tracks(&["a"]));
        assert_eq!(q.current_key(), Some(EntryKey::Context(1)));
        assert_eq!(queued_ids(&q), ["a"]);
        assert_eq!(context_ids(&q, RepeatMode::Off), ["3", "4"]);
        assert_eq!(upcoming_ids(&q), ["2", "a", "3", "4"]);

        next(&mut q);
        assert_eq!(upcoming_ids(&q), ["a", "3", "4"]);
        next(&mut q);
        assert_eq!(context_ids(&q, RepeatMode::Off), ["4"]);
        next(&mut q);
        assert!(context_ids(&q, RepeatMode::Off).is_empty());
    }

    #[test]
    fn test_play_key() {
        let mut q = playing(&["1", "2", "3"], "3");
        q.enqueue(tracks(&["a", "b", "c"]));
        assert!(q.play_key(queued_keys(&q)[1]));
        assert_eq!(current_id(&q).as_deref(), Some("b"));
        assert_eq!(queued_ids(&q), ["a", "c"]);

        assert!(q.play_key(EntryKey::Context(0)));
        assert_eq!(current_id(&q).as_deref(), Some("1"));
        assert_eq!(play_all(&mut q), ["a", "c", "2", "3"]);
    }

    #[test]
    fn test_repeat_all_view_rolls_one_track_at_a_time() {
        let mut q = playing(&["1", "2", "3", "4"], "1");
        assert_eq!(context_ids(&q, RepeatMode::Context), ["2", "3", "4"]);
        ended(&mut q, RepeatMode::Context);
        assert_eq!(context_ids(&q, RepeatMode::Context), ["3", "4", "1"]);
        ended(&mut q, RepeatMode::Context);
        assert_eq!(context_ids(&q, RepeatMode::Context), ["4", "1", "2"]);
        ended(&mut q, RepeatMode::Context);
        assert_eq!(context_ids(&q, RepeatMode::Context), ["1", "2", "3"]);
        assert!(context_ids(&q, RepeatMode::Off).is_empty());

        assert_eq!(ended(&mut q, RepeatMode::Context).as_deref(), Some("1"));
        assert_eq!(context_ids(&q, RepeatMode::Context), ["2", "3", "4"]);
        assert_eq!(context_ids(&q, RepeatMode::Off), ["2", "3", "4"]);
    }

    #[test]
    fn test_repeat_all_started_mid_context_shows_the_start_behind() {
        let q = playing(&["1", "2", "3", "4", "5"], "3");
        let view = q.view(RepeatMode::Context);
        assert_eq!(ids(&view.context), ["4", "5", "1", "2"]);
        assert_eq!(view.context[2].0, EntryKey::NextLoop(0));
        assert_eq!(context_ids(&q, RepeatMode::Off), ["4", "5"]);
    }

    #[test]
    fn test_repeat_off_stops_at_the_end() {
        let mut q = playing(&["1", "2"], "2");
        assert_eq!(ended(&mut q, RepeatMode::Off), None);
        assert!(!q.has_next(RepeatMode::Off));
    }

    #[test]
    fn test_repeat_all_wraps() {
        let mut q = playing(&["1", "2", "3"], "2");
        q.enqueue(tracks(&["a"]));
        assert_eq!(
            ended_times(&mut q, RepeatMode::Context, 5),
            ["a", "3", "1", "2", "3"]
        );

        let mut q = playing(&["1", "2"], "2");
        assert_eq!(
            id(q.advance(false, RepeatMode::Context, no_skip)).as_deref(),
            Some("1")
        );

        let mut q = playing(&["1"], "1");
        assert_eq!(ended(&mut q, RepeatMode::Context).as_deref(), Some("1"));
    }

    #[test]
    fn test_repeat_one() {
        let mut q = playing(&["1", "2"], "1");
        assert_eq!(ended(&mut q, RepeatMode::Track).as_deref(), Some("1"));
        assert_eq!(
            id(q.advance(false, RepeatMode::Track, no_skip)).as_deref(),
            Some("2")
        );

        let mut q = playing(&["1", "2"], "1");
        q.enqueue(tracks(&["a", "b"]));
        next(&mut q);
        assert_eq!(ended(&mut q, RepeatMode::Track).as_deref(), Some("a"));
        assert_eq!(ended(&mut q, RepeatMode::Track).as_deref(), Some("a"));
        assert_eq!(queued_ids(&q), ["b"]);
    }

    #[test]
    fn test_skips_unplayable_tracks() {
        let mut q = playing(&["1", "2", "3"], "1");
        q.enqueue(tracks(&["bad"]));
        let t = q.advance(false, RepeatMode::Off, |t| {
            t.rri.id == "bad" || t.rri.id == "2"
        });
        assert_eq!(id(t).as_deref(), Some("3"));
        assert!(queued_ids(&q).is_empty());

        let mut q = playing(&["1", "2"], "1");
        let t = q.advance(true, RepeatMode::Track, |t| t.rri.id == "1");
        assert_eq!(id(t).as_deref(), Some("2"));
        assert!(q.advance(true, RepeatMode::Context, |_| true).is_none());
    }

    #[test]
    fn test_peek_next_doesnt_move() {
        let mut q = playing(&["1", "2"], "1");
        q.enqueue(tracks(&["a"]));
        assert_eq!(
            id(q.peek_next(true, RepeatMode::Off, no_skip)).as_deref(),
            Some("a")
        );
        assert_eq!(current_id(&q).as_deref(), Some("1"));
        assert_eq!(queued_ids(&q), ["a"]);
    }

    #[test]
    fn test_previous() {
        let mut q = playing(&["1", "2", "3"], "2");
        q.enqueue(tracks(&["a"]));
        next(&mut q);
        assert_eq!(current_id(&q).as_deref(), Some("a"));
        assert_eq!(
            id(q.go_back(RepeatMode::Off, no_skip)).as_deref(),
            Some("2")
        );
        assert_eq!(
            id(q.go_back(RepeatMode::Off, no_skip)).as_deref(),
            Some("1")
        );
        assert!(q.go_back(RepeatMode::Off, no_skip).is_none());
        assert_eq!(
            id(q.go_back(RepeatMode::Context, no_skip)).as_deref(),
            Some("3")
        );
    }

    #[test]
    fn test_shuffle_keeps_current_and_follows_play_order() {
        let all = numbered(20);
        let mut q = playing(&all, "5");
        q.set_shuffled(true);
        assert_eq!(current_id(&q).as_deref(), Some("5"));
        let order = order_ids(&q);
        assert_eq!(order[0], "5");
        assert_eq!(order.len(), 20);
        assert_ne!(order, all);
        assert_eq!(play_all(&mut q), order[1..]);

        assert!(q.play_id("12"));
        assert_eq!(order_ids(&q)[0], "12");
        assert_eq!(order_ids(&q).len(), 20);
    }

    #[test]
    fn test_unshuffle_continues_in_source_order() {
        let mut q = shuffled(&["1", "2", "3", "4", "5"], "3");
        q.set_shuffled(false);
        assert_eq!(order_ids(&q), ["1", "2", "3", "4", "5"]);
        assert_eq!(next(&mut q).as_deref(), Some("4"));
    }

    #[test]
    fn test_shuffle_leaves_user_queue_alone() {
        let mut q = playing(&numbered(12), "0");
        q.enqueue(tracks(&numbered(12)));
        q.set_shuffled(true);
        assert_eq!(queued_ids(&q), numbered(12));
        q.enqueue(tracks(&["x"]));
        assert_eq!(queued_ids(&q).last().map(String::as_str), Some("x"));
    }

    #[test]
    fn test_shuffled_repeat_all_plays_the_next_loop_as_shown() {
        let mut q = shuffled(&numbered(20), "0");
        let first_loop = order_ids(&q);
        ended_times(&mut q, RepeatMode::Context, 19);
        assert_eq!(current_id(&q).as_deref(), Some(first_loop[19].as_str()));

        let shown = context_ids(&q, RepeatMode::Context);
        assert_eq!(shown.len(), 19);
        assert_eq!(ended_times(&mut q, RepeatMode::Context, 19), shown);
        let second_loop = order_ids(&q);
        assert_ne!(second_loop, first_loop);
        assert_eq!(second_loop[..19], shown[..]);
    }

    #[test]
    fn test_playing_a_next_loop_row_continues_as_shown() {
        let mut q = shuffled(&numbered(10), "0");
        ended_times(&mut q, RepeatMode::Context, 5);
        let view = q.view(RepeatMode::Context);
        let (key, track) = view.context[4].clone();
        assert!(matches!(key, EntryKey::NextLoop(_)));
        assert!(q.play_key(key));
        assert_eq!(current_id(&q), Some(track.rri.id));
        let shown = ids(&view.context[5..]);
        assert_eq!(ended_times(&mut q, RepeatMode::Context, shown.len()), shown);
    }

    fn min_gap_across(this_loop: &[usize], next_loop: &[usize]) -> Option<usize> {
        this_loop
            .iter()
            .rev()
            .enumerate()
            .map(|(after, track)| after + next_loop.iter().position(|t| t == track).unwrap())
            .min()
    }

    #[test]
    fn test_next_shuffled_loop_keeps_songs_apart() {
        let mut rng = SmallRng::seed_from_u64(1);
        for n in 1..12 {
            for separation in 0..5 {
                for _ in 0..200 {
                    let mut this_loop: Vec<usize> = (0..n).collect();
                    this_loop.shuffle(&mut rng);
                    let next = next_shuffled_loop(&this_loop, separation, &mut rng);
                    let mut sorted = next.clone();
                    sorted.sort();
                    assert_eq!(sorted, (0..n).collect::<Vec<_>>());
                    assert!(
                        min_gap_across(&this_loop, &next).unwrap() >= separation.min(n - 1),
                        "n={} separation={} {:?} then {:?}",
                        n,
                        separation,
                        this_loop,
                        next
                    );
                }
            }
        }
    }

    #[test]
    fn test_shuffle_repeat_never_plays_a_song_twice_in_a_row() {
        let mut q = queue();
        q.set_separation(2);
        album(&mut q, &["1", "2", "3", "4", "5", "6"]);
        q.set_shuffled(true);
        q.play_id("1");
        let mut played = vec!["1".to_string()];
        played.extend(ended_times(&mut q, RepeatMode::Context, 60));
        for window in played.chunks_exact(6) {
            let mut sorted = window.to_vec();
            sorted.sort();
            assert_eq!(sorted, ["1", "2", "3", "4", "5", "6"]);
        }
        for (i, id) in played.iter().enumerate() {
            if let Some(between) = played[i + 1..].iter().position(|other| other == id) {
                assert!(
                    between >= 2,
                    "{} again after {} in {:?}",
                    id,
                    between,
                    played
                );
            }
        }
    }

    #[test]
    fn test_changing_separation_replans_next_loop() {
        let mut q = shuffled(&numbered(8), "0");
        q.set_separation(7);
        assert!(min_gap_across(&q.order, &q.next_order).unwrap() >= 7);
    }

    #[test]
    fn test_pages_loaded_later_keep_source_order() {
        let mut q = queue();
        let source = SongsSource::Playlist("p".into());
        q.add_page(source.clone(), page(50, 50, Some(150)));
        q.play_id("98");
        q.add_page(source.clone(), page(100, 50, Some(150)));
        q.add_page(source, page(0, 50, Some(150)));
        assert_eq!(order_ids(&q), numbered(150));
        assert_eq!(current_id(&q).as_deref(), Some("98"));
        assert_eq!(next(&mut q).as_deref(), Some("99"));
    }

    #[test]
    fn test_page_requests_follow_playback() {
        let mut q = queue();
        let source = SongsSource::Playlist("p".into());
        q.add_page(source.clone(), page(50, 50, Some(150)));
        assert_eq!(request(&q, RepeatMode::Off, false), None);
        assert_eq!(request(&q, RepeatMode::Off, true), Some(0));

        q.play_id("51");
        assert_eq!(request(&q, RepeatMode::Off, false), None);
        q.play_id("90");
        assert_eq!(request(&q, RepeatMode::Off, false), Some(100));

        q.add_page(source, page(100, 50, Some(150)));
        q.play_id("140");
        assert_eq!(request(&q, RepeatMode::Off, false), None);
        assert_eq!(request(&q, RepeatMode::Context, false), Some(0));
    }

    #[test]
    fn test_page_request_fills_a_gap_before_later_pages() {
        let mut q = queue();
        let source = SongsSource::Playlist("p".into());
        q.add_page(source.clone(), page(200, 50, Some(300)));
        q.add_page(source.clone(), page(250, 50, Some(300)));
        q.add_page(source, page(0, 50, Some(300)));
        q.play_id("40");
        assert_eq!(request(&q, RepeatMode::Off, false), Some(50));
    }

    #[test]
    fn test_page_request_unknown_total_stops_at_short_page() {
        let mut q = queue();
        let source = SongsSource::Album("a".into());
        q.add_page(source.clone(), page(0, 50, None));
        q.play_id("40");
        assert_eq!(request(&q, RepeatMode::Off, false), Some(50));
        q.add_page(source, page(50, 10, None));
        q.play_id("55");
        assert_eq!(request(&q, RepeatMode::Off, false), None);
    }

    #[test]
    fn test_shuffle_mixes_new_pages_into_upcoming() {
        let mut q = queue();
        let source = SongsSource::Playlist("p".into());
        q.set_shuffled(true);
        q.add_page(source.clone(), page(0, 50, Some(100)));
        q.play_id("10");
        next(&mut q);
        next(&mut q);
        let played = order_ids(&q)[..3].to_vec();
        assert_eq!(request(&q, RepeatMode::Off, false), Some(50));
        q.add_page(source, page(50, 50, Some(100)));
        let order = order_ids(&q);
        assert_eq!(order[..3], played[..]);
        assert_eq!(order.len(), 100);
        assert_eq!(request(&q, RepeatMode::Off, false), None);
    }
}
