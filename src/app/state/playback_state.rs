use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::app::models::*;
// The data-layer device type. Aliased here because this module also defines its
// own `Device` enum (local vs. Connect target), which would clash with the
// glob-imported name.
use crate::app::models::Device as ConnectDevice;

use crate::app::components::labels;
use crate::app::state::{AppAction, AppEvent, UpdatableState};
use crate::play_queue::{EntryKey, PlayQueue, CONTEXT_PAGE_SIZE};

#[derive(Debug)]
pub struct PlaybackState {
    available_devices: Vec<ConnectDevice>,
    current_device: Device,
    queue: PlayQueue,
    view: SongListModel,
    view_keys: Vec<EntryKey>,
    view_models: HashMap<EntryKey, SongModel>,
    context_name: Option<(SongsSource, String)>,
    seek_position: PositionMillis,
    repeat: RepeatMode,
    is_playing: bool,
    // Whether to skip explicit tracks
    skip_explicit: bool,
    // Whether the Spotify account has locked the explicit filter (e.g. via a
    // family plan parental control). When locked, skipping cannot be disabled.
    explicit_filter_locked: bool,
    // Last volume that was applied (0.0..=1.0). Initialised to a sentinel
    // outside the valid range so the first `SetVolume` always propagates.
    volume: f64,
}

// Most mutatings methods shouldn't be pub
// If they are, they probably are only used by the app state
impl PlaybackState {
    pub fn queue_view(&self) -> &SongListModel {
        &self.view
    }

    pub fn view_keys(&self) -> &[EntryKey] {
        &self.view_keys
    }

    pub fn view_key(&self, index: usize) -> Option<EntryKey> {
        self.view_keys.get(index).copied()
    }

    pub fn upcoming_keys(&self) -> &[EntryKey] {
        let current = self.current_key();
        let skip = (current.is_some() && self.view_keys.first() == current.as_ref()) as usize;
        &self.view_keys[skip..]
    }

    pub fn view_track(&self, key: EntryKey) -> Option<Track> {
        let index = self.view_keys.iter().position(|k| *k == key)?;
        Some(self.view.index_continuous(index)?.into_description())
    }

    pub fn header_track(&self) -> Option<Track> {
        self.queue.header_track().cloned()
    }

    pub fn queue_is_empty(&self) -> bool {
        self.view_keys.is_empty()
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing && self.queue.current().is_some()
    }

    pub fn is_shuffled(&self) -> bool {
        self.queue.is_shuffled()
    }

    pub fn explicit_filter_locked(&self) -> bool {
        self.explicit_filter_locked
    }

    pub fn skip_explicit(&self) -> bool {
        self.skip_explicit
    }

    pub fn repeat_mode(&self) -> RepeatMode {
        self.repeat
    }

    pub fn next_query(&self, for_view: bool) -> Option<(SongsSource, PageRequest)> {
        let source = self.queue.source().filter(|s| s.is_paginated())?;
        let request = self.queue.next_page_request(self.repeat, for_view)?;
        Some((source.clone(), request))
    }

    pub fn current_source(&self) -> Option<&SongsSource> {
        self.queue.source()
    }

    pub fn current_context_index(&self) -> Option<usize> {
        self.queue.current_context_index()
    }

    pub fn current_key(&self) -> Option<EntryKey> {
        self.queue.current_key()
    }

    pub fn current_song_id(&self) -> Option<String> {
        Some(self.queue.current()?.rri.id.clone())
    }

    pub fn current_song(&self) -> Option<Track> {
        self.queue.current().cloned()
    }

    pub fn upcoming_ids(&self) -> Vec<String> {
        self.queue
            .upcoming()
            .into_iter()
            .map(|t| t.rri.id.clone())
            .collect()
    }

    pub fn has_next(&self) -> bool {
        self.queue.has_next(self.repeat)
    }

    pub fn next_song(&self) -> Option<Track> {
        self.queue.peek_next(true, self.repeat, self.skipper())
    }

    pub fn has_prev(&self) -> bool {
        self.queue.current().is_some()
    }

    fn view_model(&mut self, key: EntryKey, track: Track, role: QueueRole) -> SongModel {
        match self.view_models.get(&key) {
            Some(model) if model.get_id() == track.rri.id && model.queue_role() == Some(role) => {
                model.clone()
            }
            _ => {
                let model = SongModel::new_keyed(track, key.to_string());
                model.set_queue_role(Some(role));
                self.view_models.insert(key, model.clone());
                model
            }
        }
    }

    fn refresh_view(&mut self) {
        let local = matches!(self.current_device, Device::Local);
        let view = self.queue.view(self.repeat);
        let queue_group = labels::QUEUE_NEXT_IN_QUEUE.clone();
        let context_group = self
            .context_name
            .as_ref()
            .filter(|(named, _)| Some(named) == self.queue.source())
            .map(|(_, name)| labels::queue_context_label(name));

        let current = self
            .queue
            .current_key()
            .zip(self.queue.current().cloned())
            .map(|row| {
                let group = Some(labels::QUEUE_NOW_PLAYING.clone());
                (row, group, QueueRole::Current)
            });

        let mut keys = Vec::with_capacity(view.queued.len() + view.context.len() + 1);
        let mut songs = Vec::with_capacity(keys.capacity());
        let rows = current
            .into_iter()
            .chain(
                view.queued
                    .into_iter()
                    .map(|row| (row, Some(queue_group.clone()), QueueRole::Queued)),
            )
            .chain(
                view.context
                    .into_iter()
                    .map(|row| (row, context_group.clone(), QueueRole::Context)),
            );
        for ((key, track), group, role) in rows {
            let role = if local { role } else { QueueRole::Fixed };
            let model = self.view_model(key, track, role);
            model.set_group(group);
            keys.push(key);
            songs.push(model);
        }
        let listed: HashSet<EntryKey> = keys.iter().copied().collect();
        self.view_models.retain(|key, _| listed.contains(key));
        self.view_keys = keys;
        self.view.replace_models(songs).commit();
    }

    fn set_queue_with_source(&mut self, source: Option<SongsSource>, tracks: Vec<Track>) {
        self.queue.set_context(source, tracks);
        self.refresh_view();
    }

    pub fn queue(&mut self, tracks: Vec<Track>) {
        self.queue.enqueue(tracks);
        self.refresh_view();
    }

    pub fn dequeue(&mut self, keys: &[EntryKey]) -> bool {
        let changed = self.queue.remove(keys);
        self.refresh_view();
        changed
    }

    pub fn move_queued(&mut self, key: EntryKey, to: usize) -> bool {
        let changed = self.queue.move_queued(key, to);
        if changed {
            self.refresh_view();
        }
        changed
    }

    fn skipper(&self) -> impl Fn(&Track) -> bool {
        let skip_explicit = self.skip_explicit;
        move |track: &Track| !track.playable || (skip_explicit && track.is_explicit())
    }

    fn started_playing(&mut self) -> Option<String> {
        self.is_playing = true;
        self.seek_position.set(0, true);
        self.refresh_view();
        self.current_song_id()
    }

    fn select_id(&mut self, id: &str) -> bool {
        if self.current_song_id().as_deref() == Some(id) {
            return false;
        }
        debug!("Playing {id}");
        let found = self.queue.play_id(id);
        if !found {
            debug!("Song not found");
        }
        found
    }

    fn jumped(&mut self) -> Vec<PlaybackEvent> {
        if self.current_song_should_skip() {
            debug!("Track must be skipped (unplayable or explicit-filtered)");
            return self.play_next_events(false);
        }
        self.started_playing()
            .map(PlaybackEvent::TrackChanged)
            .into_iter()
            .collect()
    }

    fn set_repeat(&mut self, mode: RepeatMode) -> Vec<PlaybackEvent> {
        self.repeat = mode;
        self.refresh_view();
        vec![
            PlaybackEvent::RepeatModeChanged(mode),
            PlaybackEvent::PlaylistChanged,
        ]
    }

    fn stop(&mut self) {
        self.queue.stop();
        self.is_playing = false;
        self.seek_position.set(0, false);
        self.refresh_view();
    }

    /// Advance to the next playable track. Unplayable tracks are always
    /// skipped; explicit tracks only when skip_explicit is on. Stops before
    /// returning None. `auto` means the current track ended by itself.
    fn play_next_skippable(&mut self, auto: bool) -> Option<String> {
        let skip = self.skipper();
        if self.queue.advance(auto, self.repeat, skip).is_some() {
            self.started_playing()
        } else {
            self.stop();
            None
        }
    }

    fn play_next_events(&mut self, auto: bool) -> Vec<PlaybackEvent> {
        if let Some(id) = self.play_next_skippable(auto) {
            vec![PlaybackEvent::TrackChanged(id)]
        } else {
            vec![PlaybackEvent::PlaybackStopped]
        }
    }

    fn current_song_should_skip(&self) -> bool {
        self.current_song().is_some_and(|s| self.skipper()(&s))
    }

    /// If the current track must be skipped, advance to the next playable one.
    /// Returns the playback events to emit, if any.
    fn skip_current_if_needed(&mut self) -> Vec<PlaybackEvent> {
        if self.current_song_should_skip() {
            debug!("Current track must be skipped (unplayable or explicit-filtered)");
            self.play_next_events(false)
        } else {
            vec![]
        }
    }

    fn play_prev_events(&mut self) -> Vec<PlaybackEvent> {
        if self.queue.current().is_none() {
            return vec![];
        }
        if self.seek_position.current() <= 2000 {
            let skip = self.skipper();
            if self.queue.go_back(self.repeat, skip).is_some() {
                if let Some(id) = self.started_playing() {
                    return vec![PlaybackEvent::TrackChanged(id)];
                }
            }
        }
        self.seek_position.set(0, self.is_playing);
        vec![PlaybackEvent::TrackSeeked(0)]
    }

    fn toggle_play(&mut self) -> Option<bool> {
        if self.queue.current().is_some() {
            self.is_playing = !self.is_playing;

            match self.is_playing {
                false => self.seek_position.pause(),
                true => self.seek_position.resume(),
            };

            Some(self.is_playing)
        } else {
            None
        }
    }

    fn start_from_queue(&mut self) -> Vec<PlaybackEvent> {
        if self.queue.current().is_none() && self.has_next() {
            self.play_next_events(false)
        } else {
            vec![]
        }
    }

    fn set_shuffled(&mut self, shuffled: bool) -> Vec<PlaybackEvent> {
        self.queue.set_shuffled(shuffled);
        self.refresh_view();
        vec![
            PlaybackEvent::ShuffleChanged(shuffled),
            PlaybackEvent::PlaylistChanged,
        ]
    }

    pub fn available_devices(&self) -> &Vec<ConnectDevice> {
        &self.available_devices
    }

    pub fn current_device(&self) -> &Device {
        &self.current_device
    }
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            available_devices: vec![],
            current_device: Device::Local,
            queue: PlayQueue::default(),
            view: SongListModel::new(CONTEXT_PAGE_SIZE as u32),
            view_keys: vec![],
            view_models: HashMap::new(),
            context_name: None,
            seek_position: PositionMillis::new(1.0),
            repeat: RepeatMode::Off,
            is_playing: false,
            skip_explicit: false,
            explicit_filter_locked: false,
            volume: -1.0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum PlaybackAction {
    TogglePlay,
    Play,
    Pause,
    Stop,
    SetRepeatMode(RepeatMode),
    SetShuffled(bool),
    SetSkipExplicit(bool),
    // Sync the explicit content filter lock from the Spotify account profile.
    // A locked filter (e.g. a family plan parental control) forces skipping on
    // and prevents the user from disabling it. An unlocked account setting does
    // not change the local preference, which defaults to off.
    SetExplicitFilterLocked(bool),
    ToggleRepeat,
    ToggleShuffle,
    Seek(u32),
    SyncSeek(u32),
    Load(String),
    #[deprecated]
    LoadSongs(Vec<Track>),
    LoadPagedSongs(SongsSource, Page<Track>),
    LoadContextSongs(SongsSource, Vec<Track>),
    SetVolume(f64),
    Next,
    TrackEnded,
    Previous,
    PreloadNext,
    ReplaceQueue,
    ClearQueued,
    SetContextName(SongsSource, String),
    SetShuffleSeparation(u32),
    PlayEntry(EntryKey),
    RemoveEntries(Vec<EntryKey>),
    MoveQueued {
        key: EntryKey,
        to: usize,
    },
    SwitchDevice(Device),
    SetAvailableDevices(Vec<ConnectDevice>),
}

pub fn load_context(
    source: SongsSource,
    name: Option<String>,
    load: PlaybackAction,
) -> Vec<AppAction> {
    let name = name.map(|name| PlaybackAction::SetContextName(source, name).into());
    name.into_iter().chain([load.into()]).collect()
}

pub fn start_actions(shuffle: bool, context: Vec<AppAction>) -> Vec<AppAction> {
    let mut actions = vec![
        PlaybackAction::SetShuffled(shuffle).into(),
        PlaybackAction::ReplaceQueue.into(),
    ];
    actions.extend(context);
    actions.push(PlaybackAction::Play.into());
    actions
}

impl From<PlaybackAction> for AppAction {
    fn from(playback_action: PlaybackAction) -> Self {
        Self::PlaybackAction(playback_action)
    }
}

#[derive(Clone, Debug)]
pub enum Device {
    Local,
    Connect(ConnectDevice),
}

#[derive(Clone, Debug)]
pub enum PlaybackEvent {
    PlaybackPaused,
    PlaybackResumed,
    RepeatModeChanged(RepeatMode),
    TrackSeeked(u32),
    SeekSynced(u32),
    VolumeSet(f64),
    TrackChanged(String),
    SourceChanged,
    Preload(String),
    ShuffleChanged(bool),
    PlaylistChanged,
    PlaybackStopped,
    SwitchedDevice(Device),
    AvailableDevicesChanged,
    /// Emitted when the skip_explicit preference changes (either locally or
    /// forced by the account's explicit content filter). Used to sync GSettings
    /// with the internal state.
    SkipExplicitChanged(bool),
}

impl From<PlaybackEvent> for AppEvent {
    fn from(playback_event: PlaybackEvent) -> Self {
        Self::PlaybackEvent(playback_event)
    }
}

fn changed_events(changed: bool) -> Vec<PlaybackEvent> {
    if changed {
        vec![PlaybackEvent::PlaylistChanged]
    } else {
        vec![]
    }
}

impl UpdatableState for PlaybackState {
    type Action = PlaybackAction;
    type Event = PlaybackEvent;

    // Main "reducer" :)
    fn update_with(&mut self, action: Cow<Self::Action>) -> Vec<Self::Event> {
        match action.into_owned() {
            PlaybackAction::TogglePlay => match self.toggle_play() {
                Some(true) => vec![PlaybackEvent::PlaybackResumed],
                Some(false) => vec![PlaybackEvent::PlaybackPaused],
                None => self.start_from_queue(),
            },
            PlaybackAction::Play => {
                if self.queue.current().is_none() {
                    self.start_from_queue()
                } else if !self.is_playing() && self.toggle_play() == Some(true) {
                    vec![PlaybackEvent::PlaybackResumed]
                } else {
                    vec![]
                }
            }
            PlaybackAction::Pause => {
                if self.is_playing() && self.toggle_play() == Some(false) {
                    vec![PlaybackEvent::PlaybackPaused]
                } else {
                    vec![]
                }
            }
            PlaybackAction::ToggleRepeat => self.set_repeat(match self.repeat {
                RepeatMode::Track => RepeatMode::Off,
                RepeatMode::Context => RepeatMode::Track,
                RepeatMode::Off => RepeatMode::Context,
            }),
            PlaybackAction::SetRepeatMode(mode) if self.repeat != mode => self.set_repeat(mode),
            PlaybackAction::SetShuffled(shuffled) if self.is_shuffled() != shuffled => {
                self.set_shuffled(shuffled)
            }
            PlaybackAction::SetSkipExplicit(skip) => {
                let changed = self.skip_explicit != skip;
                self.skip_explicit = skip;
                let mut events = Vec::new();
                if changed {
                    events.push(PlaybackEvent::SkipExplicitChanged(skip));
                }
                // If enabling while an explicit track is playing, skip it now.
                events.extend(self.skip_current_if_needed());
                events
            }
            PlaybackAction::SetExplicitFilterLocked(locked) => {
                self.explicit_filter_locked = locked;
                let mut events = Vec::new();
                // A locked account filter forces skipping on. An unlocked
                // account leaves the local preference untouched (default off).
                if locked && !self.skip_explicit {
                    self.skip_explicit = true;
                    events.push(PlaybackEvent::SkipExplicitChanged(true));
                }
                // If the filter just turned on while an explicit track is
                // playing, skip it now.
                events.extend(self.skip_current_if_needed());
                events
            }
            PlaybackAction::ToggleShuffle => self.set_shuffled(!self.is_shuffled()),
            PlaybackAction::Next => self.play_next_events(false),
            PlaybackAction::TrackEnded => self.play_next_events(true),
            PlaybackAction::Stop => {
                self.stop();
                vec![PlaybackEvent::PlaybackStopped]
            }
            PlaybackAction::Previous => self.play_prev_events(),
            PlaybackAction::Load(id) => {
                if self.select_id(&id) {
                    self.jumped()
                } else {
                    vec![]
                }
            }
            PlaybackAction::PlayEntry(key) => {
                if self.queue.play_key(key) {
                    self.jumped()
                } else {
                    vec![]
                }
            }
            PlaybackAction::PreloadNext => {
                let skip = self.skipper();
                match self.queue.peek_next(true, self.repeat, skip) {
                    Some(track) => vec![PlaybackEvent::Preload(track.rri.id)],
                    None => vec![],
                }
            }
            PlaybackAction::LoadPagedSongs(source, batch) => {
                let new_source = self.queue.add_page(source, batch);
                self.refresh_view();
                if new_source {
                    vec![PlaybackEvent::PlaylistChanged, PlaybackEvent::SourceChanged]
                } else {
                    vec![PlaybackEvent::PlaylistChanged]
                }
            }
            #[allow(deprecated)]
            PlaybackAction::LoadSongs(tracks) => {
                self.queue.clear();
                self.set_queue_with_source(None, tracks);
                vec![PlaybackEvent::PlaylistChanged, PlaybackEvent::SourceChanged]
            }
            PlaybackAction::LoadContextSongs(source, tracks) => {
                self.set_queue_with_source(Some(source), tracks);
                vec![PlaybackEvent::PlaylistChanged, PlaybackEvent::SourceChanged]
            }
            PlaybackAction::ReplaceQueue => {
                self.queue.clear();
                self.refresh_view();
                vec![PlaybackEvent::PlaylistChanged]
            }
            PlaybackAction::SetContextName(source, name) => {
                let playing = self.queue.source() == Some(&source);
                self.context_name = Some((source, name));
                if playing {
                    self.refresh_view();
                    vec![PlaybackEvent::PlaylistChanged]
                } else {
                    vec![]
                }
            }
            PlaybackAction::SetShuffleSeparation(separation) => {
                self.queue.set_separation(separation as usize);
                self.refresh_view();
                vec![PlaybackEvent::PlaylistChanged]
            }
            PlaybackAction::ClearQueued => {
                let changed = self.queue.clear_queued();
                self.refresh_view();
                changed_events(changed)
            }
            PlaybackAction::RemoveEntries(keys) => changed_events(self.dequeue(&keys)),
            PlaybackAction::MoveQueued { key, to } => changed_events(self.move_queued(key, to)),
            PlaybackAction::Seek(pos) => {
                self.seek_position.set(pos as u64 * 1000, true);
                vec![PlaybackEvent::TrackSeeked(pos)]
            }
            PlaybackAction::SyncSeek(pos) => {
                self.seek_position.set(pos as u64 * 1000, true);
                vec![PlaybackEvent::SeekSynced(pos)]
            }
            PlaybackAction::SetVolume(volume) => {
                // Idempotency guard: only emit (and thus touch dconf, the
                // mixer, the Web API and MPRIS/D-Bus) when the volume actually
                // changes. Rapid volume input (e.g. mouse-wheel scrolling the
                // slider) would otherwise fan out a storm of `VolumeSet` events
                // and MPRIS `PropertiesChanged` signals.
                if self.volume == volume {
                    vec![]
                } else {
                    self.volume = volume;
                    vec![PlaybackEvent::VolumeSet(volume)]
                }
            }

            PlaybackAction::SetAvailableDevices(list) => {
                self.available_devices = list;
                vec![PlaybackEvent::AvailableDevicesChanged]
            }
            PlaybackAction::SwitchDevice(new_device) => {
                self.current_device = new_device.clone();
                self.refresh_view();
                vec![PlaybackEvent::SwitchedDevice(new_device)]
            }
            _ => vec![],
        }
    }
}

// A struct to keep track of the playback position
// Caller must call pause/play at the right time
#[derive(Debug)]
struct PositionMillis {
    // Last recorded position in the track (in milliseconds)
    last_known_position: u64,
    // Last time we resumed playback
    last_resume_instant: Option<Instant>,
    // Playback rate (1)
    rate: f32,
}

impl PositionMillis {
    fn new(rate: f32) -> Self {
        Self {
            last_known_position: 0,
            last_resume_instant: None,
            rate,
        }
    }

    // Read the current pos by adding elapsed time since the last time we resumed playback to the last know position
    fn current(&self) -> u64 {
        let current_progress = self.last_resume_instant.map(|ri| {
            let elapsed = ri.elapsed().as_millis() as f32;
            let real_elapsed = self.rate * elapsed;
            real_elapsed.ceil() as u64
        });
        self.last_known_position + current_progress.unwrap_or(0)
    }

    fn set(&mut self, position: u64, playing: bool) {
        self.last_known_position = position;
        self.last_resume_instant = if playing { Some(Instant::now()) } else { None }
    }

    fn pause(&mut self) {
        self.last_known_position = self.current();
        self.last_resume_instant = None;
    }

    fn resume(&mut self) {
        self.last_resume_instant = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::app::models::make_track;

    fn song(id: &str) -> Track {
        make_track(id)
    }

    fn load(state: &mut PlaybackState, tracks: Vec<Track>) {
        state.set_queue_with_source(None, tracks);
    }

    impl PlaybackState {
        fn song_ids(&self) -> Vec<String> {
            self.upcoming_ids()
        }

        fn act(&mut self, action: PlaybackAction) -> Vec<PlaybackEvent> {
            self.update_with(Cow::Owned(action))
        }

        fn play(&mut self, id: &str) {
            self.act(PlaybackAction::Load(id.to_string()));
        }
    }

    fn playing(ids: &[&str], current: &str) -> PlaybackState {
        let mut state = PlaybackState::default();
        load(&mut state, ids.iter().map(|id| song(id)).collect());
        state.play(current);
        state
    }

    fn track_changes(
        state: &mut PlaybackState,
        action: PlaybackAction,
        times: usize,
    ) -> Vec<String> {
        (0..times)
            .flat_map(|_| state.act(action.clone()))
            .filter_map(|e| match e {
                PlaybackEvent::TrackChanged(id) => Some(id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn test_user_queue() {
        let mut state = PlaybackState::default();
        state.queue(vec![song("a"), song("b")]);
        assert_eq!(state.song_ids(), ["a", "b"]);
        assert!(state.current_song().is_none());
        assert_eq!(
            track_changes(&mut state, PlaybackAction::TogglePlay, 1),
            ["a"]
        );
        assert!(state.is_playing());

        let mut state = playing(&["1", "2", "3"], "1");
        state.queue(vec![song("a"), song("a"), song("b"), song("c")]);
        assert_eq!(state.song_ids(), ["1", "a", "a", "b", "c", "2", "3"]);
        let keys = state.upcoming_keys().to_vec();

        state.act(PlaybackAction::RemoveEntries(vec![keys[1]]));
        assert_eq!(state.song_ids(), ["1", "a", "b", "c", "2", "3"]);
        assert!(state.view_track(keys[1]).is_none());
        assert!(!state.dequeue(&[state.current_key().unwrap()]));

        let events = state.act(PlaybackAction::MoveQueued {
            key: keys[3],
            to: 0,
        });
        assert!(matches!(&events[..], [PlaybackEvent::PlaylistChanged]));
        assert_eq!(state.song_ids(), ["1", "c", "a", "b", "2", "3"]);
        assert!(state
            .act(PlaybackAction::MoveQueued {
                key: keys[3],
                to: 1
            })
            .is_empty());

        assert_eq!(
            track_changes(&mut state, PlaybackAction::PlayEntry(keys[2]), 1),
            ["b"]
        );
        assert_eq!(state.song_ids(), ["b", "c", "a", "2", "3"]);
        assert!(!state
            .upcoming_keys()
            .contains(&state.current_key().unwrap()));

        assert_eq!(
            track_changes(&mut state, PlaybackAction::Next, 4),
            ["c", "a", "2", "3"]
        );
        let events = state.act(PlaybackAction::Next);
        assert!(matches!(&events[..], [PlaybackEvent::PlaybackStopped]));
        assert!(!state.is_playing());
    }

    #[test]
    fn test_queue_view() {
        let mut state = PlaybackState::default();
        let album = SongsSource::Album("a".to_string());
        state.act(PlaybackAction::SetContextName(
            album.clone(),
            "Blue Train".to_string(),
        ));
        state.act(PlaybackAction::LoadContextSongs(
            album,
            vec![song("1"), song("2"), song("3")],
        ));
        state.act(PlaybackAction::Load("1".to_string()));
        state.queue(vec![song("q")]);
        let rows = |view: &SongListModel| -> Vec<(String, Option<String>, Option<QueueRole>)> {
            (0..view.partial_len())
                .filter_map(|i| view.index_continuous(i))
                .map(|m| (m.get_id(), m.group(), m.queue_role()))
                .collect()
        };

        let all = rows(state.queue_view());
        assert_eq!(all.len(), 4);
        assert_eq!(
            all[0],
            (
                "1".to_string(),
                Some(labels::QUEUE_NOW_PLAYING.clone()),
                Some(QueueRole::Current)
            )
        );
        assert_eq!(all[1].0, "q");
        assert_eq!(all[3].1.as_deref(), Some("Next from: Blue Train"));
        assert_eq!(state.view_key(0), state.current_key());
        assert_eq!(state.view_keys()[1..], *state.upcoming_keys());

        state.act(PlaybackAction::Next);
        let all = rows(state.queue_view());
        assert_eq!(
            (all[0].0.as_str(), all[0].2),
            ("q", Some(QueueRole::Current))
        );
        assert_eq!(all.len(), 3);

        state.act(PlaybackAction::Stop);
        assert_eq!(state.view_keys(), state.upcoming_keys());

        state.act(PlaybackAction::SetContextName(
            SongsSource::Album("b".to_string()),
            "Kind of Blue".to_string(),
        ));
        state.act(PlaybackAction::LoadContextSongs(
            SongsSource::Album("c".to_string()),
            vec![song("4"), song("5")],
        ));
        state.act(PlaybackAction::Load("4".to_string()));
        assert_eq!(rows(state.queue_view())[1].1, None);
    }

    #[test]
    fn test_context_source() {
        let mut state = PlaybackState::default();
        let source = SongsSource::Album("a".to_string());
        state.act(PlaybackAction::LoadPagedSongs(
            source.clone(),
            Page {
                items: (0..50).map(|i| song(&i.to_string())).collect(),
                offset: Some(0),
                total: Some(60),
                next_cursor: None,
            },
        ));
        state.act(PlaybackAction::Load("45".to_string()));
        state.queue(vec![song("x")]);
        assert_eq!(state.current_source(), Some(&source));
        let (query_source, request) = state.next_query(false).unwrap();
        assert_eq!((query_source, request.offset), (source.clone(), 50));

        state.act(PlaybackAction::ReplaceQueue);
        state.act(PlaybackAction::LoadContextSongs(
            source,
            vec![song("1"), song("2")],
        ));
        state.act(PlaybackAction::Load("1".to_string()));
        assert_eq!(state.song_ids(), ["1", "2"]);
    }

    #[test]
    fn test_repeat_modes() {
        let mut state = playing(&["1", "2"], "1");
        state.act(PlaybackAction::SetRepeatMode(RepeatMode::Track));
        assert_eq!(
            track_changes(&mut state, PlaybackAction::TrackEnded, 1),
            ["1"]
        );
        let events = state.act(PlaybackAction::PreloadNext);
        assert!(matches!(&events[..], [PlaybackEvent::Preload(id)] if id == "1"));
        assert_eq!(track_changes(&mut state, PlaybackAction::Next, 1), ["2"]);

        state.act(PlaybackAction::SetRepeatMode(RepeatMode::Context));
        state.queue(vec![song("a")]);
        assert_eq!(state.song_ids(), ["2", "a"]);
        assert_eq!(
            track_changes(&mut state, PlaybackAction::TrackEnded, 5),
            ["a", "1", "2", "1", "2"]
        );
    }

    #[test]
    fn test_shuffle_keeps_queue_order_and_preloads_play_order() {
        let ids: Vec<String> = (0..20).map(|i| i.to_string()).collect();
        let mut state = playing(&ids.iter().map(String::as_str).collect::<Vec<_>>(), "0");
        let queued: Vec<String> = (0..10).map(|i| format!("q{}", i)).collect();
        state.queue(queued.iter().map(|id| song(id)).collect());

        state.act(PlaybackAction::ToggleShuffle);
        assert!(state.is_shuffled());
        assert_eq!(state.song_ids()[1..11], queued[..]);

        state.act(PlaybackAction::ClearQueued);
        let next = state.song_ids()[1].clone();
        let events = state.act(PlaybackAction::PreloadNext);
        assert!(matches!(&events[..], [PlaybackEvent::Preload(id)] if *id == next));
        assert_eq!(track_changes(&mut state, PlaybackAction::Next, 1), [next]);
    }

    #[test]
    fn test_initial_state() {
        let state = PlaybackState::default();
        assert!(!state.is_playing());
        assert!(!state.is_shuffled());
        assert!(state.current_song().is_none());
        assert!(!state.has_prev());
        assert!(!state.has_next());
    }

    #[test]
    fn test_play_one() {
        let mut state = PlaybackState::default();
        load(&mut state, vec![song("foo")]);

        state.play("foo");
        assert!(state.is_playing());

        assert_eq!(state.current_song_id(), Some("foo".to_string()));
        assert!(!state.has_next());

        state.toggle_play();
        assert!(!state.is_playing());
    }

    #[test]
    fn test_play_multiple() {
        let mut state = PlaybackState::default();
        load(&mut state, vec![song("1"), song("2"), song("3")]);

        state.play("2");
        assert!(state.is_playing());
        assert!(state.has_prev());
        assert!(state.has_next());
        assert_eq!(state.current_song_id(), Some("2".to_string()));

        state.toggle_play();
        assert!(!state.is_playing());

        state.act(PlaybackAction::Next);
        assert!(state.is_playing());
        assert_eq!(state.current_song_id(), Some("3".to_string()));
        assert!(!state.has_next());

        state.act(PlaybackAction::Previous);
        state.act(PlaybackAction::Previous);
        assert!(state.is_playing());
        assert_eq!(state.current_song_id(), Some("1".to_string()));

        let events = state.act(PlaybackAction::Previous);
        assert!(matches!(&events[..], [PlaybackEvent::TrackSeeked(0)]));
        assert_eq!(state.current_song_id(), Some("1".to_string()));
    }

    #[test]
    fn test_shuffle() {
        let mut state = PlaybackState::default();
        load(&mut state, vec![song("1"), song("2"), song("3"), song("4")]);

        state.play("2");
        state.set_shuffled(true);
        assert!(state.is_shuffled());
        assert_eq!(state.current_song_id(), Some("2".to_string()));
        let order = state.song_ids();
        assert_eq!(order[0], "2");
        assert_eq!(order.len(), 4);

        state.act(PlaybackAction::Next);
        assert_eq!(state.current_song_id(), Some(order[1].clone()));

        state.set_shuffled(false);
        assert!(!state.is_shuffled());
        let current: usize = order[1].parse().unwrap();
        let expected: Vec<String> = (current..=4).map(|i| i.to_string()).collect();
        assert_eq!(state.song_ids(), expected);
    }

    #[test]
    fn test_dequeue_a_few_songs() {
        let mut state = PlaybackState::default();
        load(&mut state, vec![song("1"), song("2")]);
        state.play("1");
        state.queue(vec![song("a"), song("b"), song("c"), song("d")]);

        let keys: Vec<EntryKey> = state.upcoming_keys()[1..].to_vec();
        state.dequeue(&keys);
        assert_eq!(state.current_song_id(), Some("1".to_string()));
        assert_eq!(state.song_ids(), ["1", "a", "2"]);
    }

    #[test]
    fn test_details_page_shuffle_play() {
        let mut state = PlaybackState::default();
        let songs = vec![song("1"), song("2"), song("3"), song("4"), song("5")];
        let batch = Page {
            items: songs.clone(),
            offset: Some(0),
            total: None,
            next_cursor: None,
        };

        // Step 1: ToggleShuffle (no songs loaded yet)
        state.update_with(Cow::Owned(PlaybackAction::ToggleShuffle));
        assert!(state.is_shuffled());

        // Step 2: LoadPagedSongs (new source)
        state.update_with(Cow::Owned(PlaybackAction::LoadPagedSongs(
            SongsSource::Album("album1".to_string()),
            batch,
        )));
        assert_eq!(state.song_ids().len(), 5);

        // Step 3: Load first song
        state.update_with(Cow::Owned(PlaybackAction::Load("1".to_string())));
        assert!(state.is_playing());
        assert!(state.current_song_id().is_some());

        // Now press Next, this should NOT stop playback
        let events = state.update_with(Cow::Owned(PlaybackAction::Next));
        assert!(
            state.is_playing(),
            "Playback stopped after Next! current_song_id={:?}, current_key={:?}",
            state.current_song_id(),
            state.current_key(),
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, PlaybackEvent::TrackChanged(_))),
            "Expected TrackChanged event, got: {:?}",
            events,
        );
        assert!(state.current_song_id().is_some());

        // Press Next again — should still work
        let events = state.update_with(Cow::Owned(PlaybackAction::Next));
        assert!(state.is_playing());
        assert!(
            events
                .iter()
                .any(|e| matches!(e, PlaybackEvent::TrackChanged(_))),
            "Second Next failed, got: {:?}",
            events,
        );
    }

    #[test]
    fn test_details_page_shuffle_play_artist() {
        let mut state = PlaybackState::default();
        let songs = vec![song("1"), song("2"), song("3"), song("4"), song("5")];

        // Step 1: ToggleShuffle
        state.update_with(Cow::Owned(PlaybackAction::ToggleShuffle));
        assert!(state.is_shuffled());

        // Step 2: LoadContextSongs (non-paginated artist source)
        state.update_with(Cow::Owned(PlaybackAction::LoadContextSongs(
            SongsSource::Artist("artist1".to_string()),
            songs.clone(),
        )));
        assert_eq!(state.song_ids().len(), 5);

        // Step 3: Load first song
        state.update_with(Cow::Owned(PlaybackAction::Load("1".to_string())));
        assert!(state.is_playing(), "Song should be playing after Load");
        assert_eq!(state.current_song_id(), Some("1".to_string()));

        // Now press Next — this SHOULD work
        let events = state.update_with(Cow::Owned(PlaybackAction::Next));
        assert!(
            state.is_playing(),
            "Playback stopped after Next! current_song_id={:?}, current_key={:?}",
            state.current_song_id(),
            state.current_key(),
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, PlaybackEvent::TrackChanged(_))),
            "Expected TrackChanged event, got: {:?}",
            events,
        );
    }

    #[test]
    fn test_details_page_shuffle_play_playlist() {
        let mut state = PlaybackState::default();
        // A single aligned page of 50 songs (the playback list's batch size).
        let songs: Vec<_> = (1..=50).map(|i| song(&i.to_string())).collect();
        let batch = Page {
            items: songs,
            offset: Some(0),
            total: None,
            next_cursor: None,
        };

        // Step 1: ToggleShuffle
        state.update_with(Cow::Owned(PlaybackAction::ToggleShuffle));

        // Step 2: LoadPagedSongs
        state.update_with(Cow::Owned(PlaybackAction::LoadPagedSongs(
            SongsSource::Playlist("pl1".to_string()),
            batch,
        )));
        assert_eq!(state.song_ids().len(), 50);

        // Step 3: Load first song
        state.update_with(Cow::Owned(PlaybackAction::Load("1".to_string())));
        assert!(state.is_playing());

        // Press Next across the whole page — playback must never stop.
        let mut failed = false;
        for _ in 0..49 {
            state.update_with(Cow::Owned(PlaybackAction::Next));
            if !state.is_playing() || state.current_song_id().is_none() {
                failed = true;
                break;
            }
        }
        assert!(
            !failed,
            "Playback stopped because shuffle picked an unloaded song index"
        );
    }

    fn explicit_song(id: &str) -> Track {
        let mut track = make_track(id);
        track.title = "Explicit Title".to_string();
        track.content_rating = ContentRating::Explicit;
        track
    }

    #[test]
    fn test_skip_explicit_next() {
        let mut state = PlaybackState::default();
        state.update_with(Cow::Owned(PlaybackAction::SetSkipExplicit(true)));
        load(
            &mut state,
            vec![song("1"), explicit_song("2"), explicit_song("3"), song("4")],
        );

        state.play("1");
        assert!(state.is_playing());
        assert_eq!(state.current_song_id(), Some("1".to_string()));

        // Next should skip the two explicit tracks and land on "4"
        let events = state.update_with(Cow::Owned(PlaybackAction::Next));
        assert!(state.is_playing());
        assert_eq!(state.current_song_id(), Some("4".to_string()));
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::TrackChanged(id) if id == "4")));
    }

    #[test]
    fn test_skip_explicit_load() {
        let mut state = PlaybackState::default();
        state.update_with(Cow::Owned(PlaybackAction::SetSkipExplicit(true)));
        load(&mut state, vec![song("1"), explicit_song("2"), song("3")]);

        // Trying to load an explicit track should skip forward to "3"
        let events = state.update_with(Cow::Owned(PlaybackAction::Load("2".to_string())));
        assert!(state.is_playing());
        assert_eq!(state.current_song_id(), Some("3".to_string()));
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::TrackChanged(id) if id == "3")));
    }

    #[test]
    fn test_skip_explicit_all_explicit_stops_playback() {
        let mut state = PlaybackState::default();
        state.update_with(Cow::Owned(PlaybackAction::SetSkipExplicit(true)));
        load(
            &mut state,
            vec![song("1"), explicit_song("2"), explicit_song("3")],
        );

        state.play("1");
        assert!(state.is_playing());

        // Next should try to advance but all remaining are explicit - stop
        let events = state.update_with(Cow::Owned(PlaybackAction::Next));
        assert!(!state.is_playing());
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::PlaybackStopped)));
    }

    #[test]
    fn test_skip_explicit_disabled_plays_explicit() {
        let mut state = PlaybackState::default();
        // skip_explicit is false by default
        load(&mut state, vec![song("1"), explicit_song("2"), song("3")]);

        state.play("1");
        let events = state.update_with(Cow::Owned(PlaybackAction::Next));
        assert!(state.is_playing());
        // Should play the explicit track normally
        assert_eq!(state.current_song_id(), Some("2".to_string()));
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::TrackChanged(id) if id == "2")));
    }

    #[test]
    fn test_skip_explicit_previous() {
        let mut state = PlaybackState::default();
        state.update_with(Cow::Owned(PlaybackAction::SetSkipExplicit(true)));
        load(
            &mut state,
            vec![song("1"), explicit_song("2"), explicit_song("3"), song("4")],
        );

        state.play("4");
        assert!(state.is_playing());
        assert_eq!(state.current_song_id(), Some("4".to_string()));

        // Previous should skip the two explicit tracks and land on "1"
        let events = state.update_with(Cow::Owned(PlaybackAction::Previous));
        assert!(state.is_playing());
        assert_eq!(state.current_song_id(), Some("1".to_string()));
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::TrackChanged(id) if id == "1")));
    }

    #[test]
    fn test_account_explicit_filter_locked_forces_skip() {
        let mut state = PlaybackState::default();
        // Account has the filter locked (e.g. family plan control)
        state.update_with(Cow::Owned(PlaybackAction::SetExplicitFilterLocked(true)));
        assert!(state.skip_explicit);
        assert!(state.explicit_filter_locked());

        load(&mut state, vec![song("1"), explicit_song("2"), song("3")]);
        state.play("1");
        let events = state.update_with(Cow::Owned(PlaybackAction::Next));
        assert_eq!(state.current_song_id(), Some("3".to_string()));
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::TrackChanged(id) if id == "3")));
    }

    #[test]
    fn test_account_explicit_filter_unlocked_defaults_off() {
        let mut state = PlaybackState::default();
        // An unlocked account must not auto-enable skipping: default is off.
        state.update_with(Cow::Owned(PlaybackAction::SetExplicitFilterLocked(false)));
        assert!(!state.skip_explicit);
        assert!(!state.explicit_filter_locked());
    }

    #[test]
    fn test_account_explicit_filter_unlocked_preserves_local_pref() {
        let mut state = PlaybackState::default();
        // User locally enabled skipping
        state.update_with(Cow::Owned(PlaybackAction::SetSkipExplicit(true)));
        assert!(state.skip_explicit);

        // An unlocked account should not disable the user's local preference.
        state.update_with(Cow::Owned(PlaybackAction::SetExplicitFilterLocked(false)));
        assert!(state.skip_explicit);
        assert!(!state.explicit_filter_locked());
    }

    #[test]
    fn test_enable_filter_skips_current_explicit_track() {
        let mut state = PlaybackState::default();
        load(&mut state, vec![song("1"), explicit_song("2"), song("3")]);
        state.play("2");
        assert_eq!(state.current_song_id(), Some("2".to_string()));
        assert!(state.is_playing());

        // Enabling the filter while an explicit track is playing skips it now.
        let events = state.update_with(Cow::Owned(PlaybackAction::SetSkipExplicit(true)));
        assert_eq!(state.current_song_id(), Some("3".to_string()));
        assert!(state.is_playing());
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::TrackChanged(id) if id == "3")));
    }

    #[test]
    fn test_lock_filter_skips_current_explicit_track() {
        let mut state = PlaybackState::default();
        load(&mut state, vec![song("1"), explicit_song("2"), song("3")]);
        state.play("2");
        assert_eq!(state.current_song_id(), Some("2".to_string()));

        // Account lock arriving mid-playback skips the current explicit track.
        let events = state.update_with(Cow::Owned(PlaybackAction::SetExplicitFilterLocked(true)));
        assert_eq!(state.current_song_id(), Some("3".to_string()));
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::TrackChanged(id) if id == "3")));
    }

    #[test]
    fn test_enable_filter_leaves_non_explicit_current_track() {
        let mut state = PlaybackState::default();
        load(&mut state, vec![song("1"), explicit_song("2")]);
        state.play("1");

        // Current track is not explicit: enabling the filter does not skip.
        let events = state.update_with(Cow::Owned(PlaybackAction::SetSkipExplicit(true)));
        assert_eq!(state.current_song_id(), Some("1".to_string()));
        // Only the state-change notification is emitted, no track change.
        assert!(events
            .iter()
            .all(|e| matches!(e, PlaybackEvent::SkipExplicitChanged(true))));
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_enable_filter_all_explicit_stops_playback() {
        let mut state = PlaybackState::default();
        load(&mut state, vec![explicit_song("1"), explicit_song("2")]);
        state.play("1");
        assert!(state.is_playing());

        // Enabling the filter with only explicit tracks left stops playback.
        let events = state.update_with(Cow::Owned(PlaybackAction::SetSkipExplicit(true)));
        assert!(!state.is_playing());
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::PlaybackStopped)));
    }

    #[test]
    fn test_previous_while_paused_seeks_to_start() {
        let mut state = PlaybackState::default();
        load(&mut state, vec![song("1"), song("2")]);
        state.play("2");
        // Simulate being more than 2s into the track so Previous seeks to start.
        state.seek_position.set(5000, true);
        // Pause: is_playing becomes false, but list_position stays Some.
        state.toggle_play();
        assert!(!state.is_playing());

        // Previous should seek to start, NOT stop playback.
        let events = state.update_with(Cow::Owned(PlaybackAction::Previous));
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::TrackSeeked(0))));
        assert!(!events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::PlaybackStopped)));
        assert_eq!(state.current_song_id(), Some("2".to_string()));
    }

    #[test]
    fn test_skip_explicit_previous_all_explicit_restarts_track() {
        let mut state = PlaybackState::default();
        state.update_with(Cow::Owned(PlaybackAction::SetSkipExplicit(true)));
        load(
            &mut state,
            vec![explicit_song("1"), explicit_song("2"), song("3")],
        );

        state.play("3");
        assert!(state.is_playing());

        // Previous should try to go back but all previous tracks are explicit,
        // so the current track restarts instead.
        let events = state.update_with(Cow::Owned(PlaybackAction::Previous));
        assert!(state.is_playing());
        assert_eq!(state.current_song_id(), Some("3".to_string()));
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::TrackSeeked(0))));
    }

    fn unplayable_song(id: &str) -> Track {
        let mut track = make_track(id);
        track.title = "Unplayable Title".to_string();
        track.playable = false;
        track
    }

    #[test]
    fn test_unplayable_track_skipped_on_next_without_filter() {
        let mut state = PlaybackState::default();
        // skip_explicit is off by default; unplayable tracks must still skip.
        load(
            &mut state,
            vec![
                song("1"),
                unplayable_song("2"),
                unplayable_song("3"),
                song("4"),
            ],
        );

        state.play("1");
        assert_eq!(state.current_song_id(), Some("1".to_string()));

        let events = state.update_with(Cow::Owned(PlaybackAction::Next));
        assert!(state.is_playing());
        assert_eq!(state.current_song_id(), Some("4".to_string()));
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::TrackChanged(id) if id == "4")));
    }

    #[test]
    fn test_unplayable_track_skipped_on_load() {
        let mut state = PlaybackState::default();
        load(&mut state, vec![song("1"), unplayable_song("2"), song("3")]);

        // Trying to load an unplayable track skips forward to "3".
        let events = state.update_with(Cow::Owned(PlaybackAction::Load("2".to_string())));
        assert!(state.is_playing());
        assert_eq!(state.current_song_id(), Some("3".to_string()));
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::TrackChanged(id) if id == "3")));
    }

    #[test]
    fn test_unplayable_track_skipped_on_previous() {
        let mut state = PlaybackState::default();
        load(
            &mut state,
            vec![
                song("1"),
                unplayable_song("2"),
                unplayable_song("3"),
                song("4"),
            ],
        );

        state.play("4");
        assert_eq!(state.current_song_id(), Some("4".to_string()));

        let events = state.update_with(Cow::Owned(PlaybackAction::Previous));
        assert!(state.is_playing());
        assert_eq!(state.current_song_id(), Some("1".to_string()));
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::TrackChanged(id) if id == "1")));
    }

    #[test]
    fn test_all_unplayable_stops_playback() {
        let mut state = PlaybackState::default();
        load(
            &mut state,
            vec![song("1"), unplayable_song("2"), unplayable_song("3")],
        );

        state.play("1");
        assert!(state.is_playing());

        // Next has only unplayable tracks left, so playback stops.
        let events = state.update_with(Cow::Owned(PlaybackAction::Next));
        assert!(!state.is_playing());
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::PlaybackStopped)));
    }

    #[test]
    fn test_unplayable_skipped_together_with_explicit() {
        let mut state = PlaybackState::default();
        state.update_with(Cow::Owned(PlaybackAction::SetSkipExplicit(true)));
        load(
            &mut state,
            vec![
                song("1"),
                unplayable_song("2"),
                explicit_song("3"),
                song("4"),
            ],
        );

        state.play("1");

        // Next skips the unplayable "2" and the explicit "3", landing on "4".
        let events = state.update_with(Cow::Owned(PlaybackAction::Next));
        assert!(state.is_playing());
        assert_eq!(state.current_song_id(), Some("4".to_string()));
        assert!(events
            .iter()
            .any(|e| matches!(e, PlaybackEvent::TrackChanged(id) if id == "4")));
    }
}
