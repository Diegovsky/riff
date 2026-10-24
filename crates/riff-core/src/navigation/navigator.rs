use super::{History, HomePage, Route};

pub type EntryId = u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: EntryId,
    pub route: Route,
    pub title: Option<String>,
    pub scroll: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition {
    pub from_entry: EntryId,
    pub from: Route,
    pub to: Route,
    pub direction: Direction,
    pub restore_scroll: Option<u32>,
    pub evicted: Vec<Route>,
}

#[derive(Debug, Clone)]
pub struct Navigator {
    history: History<Entry>,
    live_limit: usize,
    next_id: EntryId,
}

impl Default for Navigator {
    fn default() -> Self {
        Self::new(Self::DEFAULT_CAPACITY, Self::DEFAULT_LIVE_LIMIT)
    }
}

impl Navigator {
    pub const DEFAULT_CAPACITY: usize = 50;
    pub const DEFAULT_LIVE_LIMIT: usize = 6;

    pub fn new(capacity: usize, live_limit: usize) -> Self {
        let start = Entry {
            id: 0,
            route: Route::Home(HomePage::default()),
            title: None,
            scroll: None,
        };
        Self {
            history: History::new(start, capacity),
            live_limit: live_limit.max(1),
            next_id: 1,
        }
    }

    pub fn current(&self) -> &Entry {
        self.history.current()
    }

    pub fn current_route(&self) -> &Route {
        &self.current().route
    }

    pub fn entries(&self) -> &[Entry] {
        self.history.entries()
    }

    pub fn can_go_back(&self) -> bool {
        self.history.can_go_back()
    }

    pub fn can_go_forward(&self) -> bool {
        self.history.can_go_forward()
    }

    pub fn back_entries(&self) -> impl Iterator<Item = &Entry> {
        self.history.back_entries().map(|(_, e)| e)
    }

    pub fn forward_entries(&self) -> impl Iterator<Item = &Entry> {
        self.history.forward_entries().map(|(_, e)| e)
    }

    pub fn navigate(&mut self, route: Route) -> Option<Transition> {
        if !route.is_valid() || route == *self.current_route() {
            return None;
        }
        let before = self.live_routes();
        let from = self.current().clone();
        let title = self
            .history
            .entries()
            .iter()
            .rev()
            .find(|e| e.route == route)
            .and_then(|e| e.title.clone());
        let entry = Entry {
            id: self.next_id,
            route,
            title,
            scroll: None,
        };
        self.next_id += 1;
        self.history.push(entry);
        Some(self.transition(before, from, Direction::Forward))
    }

    pub fn back(&mut self) -> Option<Transition> {
        let index = self.history.cursor().checked_sub(1)?;
        self.go_to_index(index)
    }

    pub fn forward(&mut self) -> Option<Transition> {
        self.go_to_index(self.history.cursor() + 1)
    }

    pub fn go_to(&mut self, id: EntryId) -> Option<Transition> {
        let index = self.index_of(id)?;
        self.go_to_index(index)
    }

    pub fn leave_if_current(&mut self, route: &Route) -> Option<Transition> {
        let before = self.live_routes();
        let from = self.history.remove_current_if(|e| e.route == *route)?;
        Some(self.transition(before, from, Direction::Backward))
    }

    pub fn set_title(&mut self, route: &Route, title: &str) -> bool {
        let mut changed = false;
        for entry in self.history.entries_mut() {
            if entry.route == *route && entry.title.as_deref() != Some(title) {
                entry.title = Some(title.to_string());
                changed = true;
            }
        }
        changed
    }

    pub fn save_scroll(&mut self, id: EntryId, scroll: u32) {
        if let Some(index) = self.index_of(id) {
            self.history.entries_mut()[index].scroll = Some(scroll);
        }
    }

    pub fn live_routes(&self) -> Vec<Route> {
        let entries = self.history.entries();
        let cursor = self.history.cursor();
        let behind = entries[..cursor].iter().rev();
        let ahead = entries[cursor + 1..].iter();
        let nearest = std::iter::once(&entries[cursor]).chain(interleave(behind, ahead));

        let mut live = vec![Route::Home(HomePage::default())];
        let mut others = 0;
        for entry in nearest {
            if others == self.live_limit {
                break;
            }
            if live.iter().any(|r| r.same_screen(&entry.route)) {
                continue;
            }
            live.push(entry.route.clone());
            others += 1;
        }
        live
    }

    fn index_of(&self, id: EntryId) -> Option<usize> {
        self.history.entries().iter().position(|e| e.id == id)
    }

    fn go_to_index(&mut self, index: usize) -> Option<Transition> {
        let cursor = self.history.cursor();
        if index == cursor {
            return None;
        }
        let before = self.live_routes();
        let from = self.current().clone();
        self.history.go_to(index)?;
        let direction = if index > cursor {
            Direction::Forward
        } else {
            Direction::Backward
        };
        Some(self.transition(before, from, direction))
    }

    fn transition(&self, before: Vec<Route>, from: Entry, direction: Direction) -> Transition {
        let after = self.live_routes();
        let evicted = before
            .into_iter()
            .filter(|r| !after.iter().any(|a| a.same_screen(r)))
            .collect();
        let to = self.current();
        Transition {
            from_entry: from.id,
            from: from.route,
            to: to.route.clone(),
            direction,
            restore_scroll: to.scroll,
            evicted,
        }
    }
}

fn interleave<'a, T: 'a>(
    a: impl Iterator<Item = &'a T>,
    b: impl Iterator<Item = &'a T>,
) -> impl Iterator<Item = &'a T> {
    let mut a = a.fuse();
    let mut b = b.fuse();
    let mut take_a = true;
    std::iter::from_fn(move || {
        let next = if take_a {
            a.next().or_else(|| b.next())
        } else {
            b.next().or_else(|| a.next())
        };
        take_a = !take_a;
        next
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn album(id: &str) -> Route {
        Route::Album(id.to_string())
    }

    fn artist(id: &str) -> Route {
        Route::Artist(id.to_string())
    }

    fn routes(nav: &Navigator) -> Vec<Route> {
        nav.entries().iter().map(|e| e.route.clone()).collect()
    }

    const HOME: Route = Route::Home(HomePage::Library);

    #[test]
    fn revisiting_a_screen_adds_an_entry() {
        let mut nav = Navigator::default();
        nav.navigate(album("a"));
        nav.navigate(artist("b"));
        nav.navigate(album("a"));
        assert_eq!(
            routes(&nav),
            vec![HOME, album("a"), artist("b"), album("a")]
        );

        assert_eq!(nav.back().unwrap().to, artist("b"));
        assert_eq!(nav.back().unwrap().to, album("a"));
        let forward = nav.forward().unwrap();
        assert_eq!(forward.to, artist("b"));
        assert_eq!(forward.direction, Direction::Forward);
    }

    #[test]
    fn navigating_to_the_current_route_does_nothing() {
        let mut nav = Navigator::default();
        assert!(nav.navigate(HOME).is_none());
        nav.navigate(album("a"));
        assert!(nav.navigate(album("a")).is_none());
        assert_eq!(nav.entries().len(), 2);
    }

    #[test]
    fn routes_without_an_id_are_ignored() {
        let mut nav = Navigator::default();
        assert!(nav.navigate(album("")).is_none());
        assert_eq!(nav.entries().len(), 1);
    }

    #[test]
    fn home_pages_are_entries() {
        let mut nav = Navigator::default();
        nav.navigate(album("a"));
        nav.navigate(Route::Home(HomePage::SavedArtists));
        let back = nav.back().unwrap();
        assert_eq!(back.from, Route::Home(HomePage::SavedArtists));
        assert_eq!(back.to, album("a"));
        assert_eq!(back.direction, Direction::Backward);
    }

    #[test]
    fn navigating_drops_forward_history() {
        let mut nav = Navigator::default();
        nav.navigate(album("a"));
        nav.navigate(album("b"));
        nav.back();
        nav.navigate(artist("c"));
        assert!(!nav.can_go_forward());
        assert_eq!(routes(&nav), vec![HOME, album("a"), artist("c")]);
    }

    #[test]
    fn nothing_to_go_back_or_forward_to() {
        let mut nav = Navigator::default();
        assert!(nav.back().is_none());
        assert!(nav.forward().is_none());
    }

    #[test]
    fn go_to_jumps_by_entry_id() {
        let mut nav = Navigator::default();
        nav.navigate(album("a"));
        let a = nav.current().id;
        nav.navigate(album("b"));
        nav.navigate(album("c"));
        assert_eq!(nav.go_to(a).unwrap().to, album("a"));
        assert!(nav.go_to(a).is_none(), "already there");
        assert!(nav.go_to(999).is_none());
    }

    #[test]
    fn leaving_only_acts_on_the_current_route() {
        let mut nav = Navigator::default();
        nav.navigate(album("missing"));
        nav.navigate(artist("b"));
        assert!(nav.leave_if_current(&album("missing")).is_none());
        assert_eq!(nav.current_route(), &artist("b"));

        nav.back();
        let left = nav.leave_if_current(&album("missing")).unwrap();
        assert_eq!(left.to, HOME);
        assert_eq!(routes(&nav), vec![HOME, artist("b")]);
        assert!(left.evicted.contains(&album("missing")));
    }

    #[test]
    fn scroll_is_restored_per_entry() {
        let mut nav = Navigator::default();
        nav.navigate(album("a"));
        let a = nav.current().id;
        let t = nav.navigate(album("b")).unwrap();
        assert_eq!(t.restore_scroll, None);
        nav.save_scroll(t.from_entry, 420);
        assert_eq!(t.from_entry, a);
        assert_eq!(nav.back().unwrap().restore_scroll, Some(420));
    }

    #[test]
    fn titles_apply_to_every_visit() {
        let mut nav = Navigator::default();
        nav.navigate(album("a"));
        nav.navigate(album("b"));
        nav.navigate(album("a"));
        assert!(nav.set_title(&album("a"), "Album A"));
        assert!(!nav.set_title(&album("a"), "Album A"));
        let titled = nav
            .entries()
            .iter()
            .filter(|e| e.title.as_deref() == Some("Album A"))
            .count();
        assert_eq!(titled, 2);
    }

    #[test]
    fn revisits_keep_the_known_title() {
        let mut nav = Navigator::default();
        nav.navigate(artist("a"));
        nav.set_title(&artist("a"), "Artist A");
        nav.navigate(album("b"));
        nav.navigate(artist("a"));
        assert_eq!(nav.current().title.as_deref(), Some("Artist A"));
        let back: Vec<_> = nav.back_entries().map(|e| e.title.as_deref()).collect();
        assert_eq!(back, vec![None, Some("Artist A"), None]);
    }

    #[test]
    fn capacity_bounds_the_history() {
        let mut nav = Navigator::new(3, 6);
        for i in 0..5 {
            nav.navigate(album(&i.to_string()));
        }
        assert_eq!(routes(&nav), vec![album("2"), album("3"), album("4")]);
    }

    #[test]
    fn home_is_always_live() {
        let mut nav = Navigator::new(50, 2);
        for i in 0..5 {
            nav.navigate(album(&i.to_string()));
        }
        let live = nav.live_routes();
        assert!(live.contains(&HOME));
        assert_eq!(live.len(), 3);
    }

    #[test]
    fn far_screens_are_evicted() {
        let mut nav = Navigator::new(50, 2);
        assert!(nav.navigate(album("a")).unwrap().evicted.is_empty());
        assert!(nav.navigate(album("b")).unwrap().evicted.is_empty());
        let t = nav.navigate(album("c")).unwrap();
        assert_eq!(t.evicted, vec![album("a")]);
        assert_eq!(nav.live_routes(), vec![HOME, album("c"), album("b")]);
    }

    #[test]
    fn live_screens_look_both_ways() {
        let mut nav = Navigator::new(50, 3);
        for id in ["a", "b", "c", "d", "e"] {
            nav.navigate(album(id));
        }
        nav.go_to(nav.entries()[3].id); // c
        assert_eq!(
            nav.live_routes(),
            vec![HOME, album("c"), album("b"), album("d")]
        );
    }

    #[test]
    fn a_screen_visited_twice_is_live_once() {
        let mut nav = Navigator::new(50, 6);
        nav.navigate(album("a"));
        nav.navigate(album("b"));
        nav.navigate(album("a"));
        nav.navigate(Route::Home(HomePage::SavedTracks));
        assert_eq!(nav.live_routes(), vec![HOME, album("a"), album("b")]);
    }
}
