use std::borrow::Cow;

use gettextrs::gettext;

use crate::app::components::labels;
use crate::app::models::{Playlist, PlaylistSummary, Track};
use crate::app::state::{
    browser_state::{BrowserAction, BrowserEvent, BrowserState},
    login_state::{LoginAction, LoginEvent, LoginState},
    playback_state::{Device, PlaybackAction, PlaybackEvent, PlaybackState},
    selection_state::{SelectionAction, SelectionContext, SelectionEvent, SelectionState},
    settings_state::{SettingsAction, SettingsEvent, SettingsState},
    EntryKey, ScreenName, SpotifyLink, UpdatableState,
};

// It's a big one...
// All possible actions!
// It's probably a VERY poor way to layout such a big enum, just look at the size, I'm so sorry I am not a sytems programmer
// Could use a few more Boxes maybe?
#[derive(Clone, Debug)]
pub enum AppAction {
    // With sub categories :)
    PlaybackAction(PlaybackAction),
    BrowserAction(BrowserAction),
    SelectionAction(SelectionAction),
    LoginAction(LoginAction),
    SettingsAction(SettingsAction),
    Start,
    Raise,
    ShowNotification(String),
    // Account is blocked by PlayPlay DRM; triggers an explanatory dialog.
    ShowDrmBlockedDialog,
    SetConnectionLost(bool),
    // Cross-state actions
    QueueSelection,
    QueueTracks { tracks: Vec<Track> },
    DequeueSelection,
    SaveSelection,
    UnsaveSelection,
    EnableSelection(SelectionContext),
    CancelSelection,
    CreatePlaylist(Playlist),
    UpdatePlaylistName(PlaylistSummary),
    RemovePlaylist(String),
}

// Not actual actions, just neat wrappers
impl AppAction {
    // An action to open a Spotify URI or an open.spotify.com URL.
    // Track links are not resolvable synchronously (they need an API lookup to
    // find the containing album), so they are handled separately by the caller.
    #[allow(non_snake_case)]
    pub fn OpenURI(uri: String) -> Option<Self> {
        debug!("parsing {}", &uri);
        match SpotifyLink::parse(&uri)? {
            SpotifyLink::Album(id) => Some(Self::ViewAlbum(id)),
            SpotifyLink::Artist(id) => Some(Self::ViewArtist(id)),
            SpotifyLink::Playlist(id) => Some(Self::ViewPlaylist(id)),
            SpotifyLink::User(id) => Some(Self::ViewUser(id)),
            // Tracks require an async album lookup; not handled here.
            SpotifyLink::Track(_) => None,
        }
    }

    #[allow(non_snake_case)]
    pub fn ViewAlbum(id: String) -> Self {
        BrowserAction::NavigationPush(ScreenName::AlbumDetails(id)).into()
    }

    #[allow(non_snake_case)]
    pub fn ViewArtist(id: String) -> Self {
        BrowserAction::NavigationPush(ScreenName::Artist(id)).into()
    }

    #[allow(non_snake_case)]
    pub fn ViewPlaylist(id: String) -> Self {
        BrowserAction::NavigationPush(ScreenName::PlaylistDetails(id)).into()
    }

    #[allow(non_snake_case)]
    pub fn ViewUser(id: String) -> Self {
        BrowserAction::NavigationPush(ScreenName::User(id)).into()
    }

    #[allow(non_snake_case)]
    pub fn ViewSearch() -> Self {
        BrowserAction::NavigationPush(ScreenName::Search).into()
    }
}

// Actions mutate stuff, and we know what changed thanks to these events
#[derive(Clone, Debug)]
pub enum AppEvent {
    // Also subcategorized
    PlaybackEvent(PlaybackEvent),
    BrowserEvent(BrowserEvent),
    SelectionEvent(SelectionEvent),
    LoginEvent(LoginEvent),
    Started,
    Raised,
    NotificationShown(String),
    DrmBlockedDialogShown,
    ConnectionLostChanged(bool),
    PlaylistCreatedNotificationShown(String),
    SettingsEvent(SettingsEvent),
}

// The actual state, split five-ways
pub struct AppState {
    started: bool,
    // Whether the app has lost its connection to Spotify. Driven by the
    // player's session-health path; kept here as a single source of truth so
    // the transient banner event is only emitted on an actual change.
    connection_lost: bool,
    pub playback: PlaybackState,
    pub browser: BrowserState,
    pub selection: SelectionState,
    pub logged_user: LoginState,
    pub settings: SettingsState,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            started: false,
            connection_lost: false,
            playback: Default::default(),
            browser: BrowserState::new(),
            selection: Default::default(),
            logged_user: Default::default(),
            settings: Default::default(),
        }
    }

    pub fn update_state(&mut self, message: AppAction) -> Vec<AppEvent> {
        match message {
            AppAction::Start if !self.started => {
                self.started = true;
                vec![AppEvent::Started]
            }
            // Couple of actions that don't mutate the state (not intested in keeping track of what they change)
            // they're here just to have a consistent way of doing things (always an Action)
            AppAction::ShowNotification(c) => vec![AppEvent::NotificationShown(c)],
            AppAction::ShowDrmBlockedDialog => vec![AppEvent::DrmBlockedDialogShown],
            AppAction::SetConnectionLost(lost) => {
                // Real state, deduplicated: only emit the event (and move the
                // banner) when the connection status actually changes, so
                // repeated raise/clear calls from the reconnect path are no-ops.
                if self.connection_lost == lost {
                    vec![]
                } else {
                    self.connection_lost = lost;
                    vec![AppEvent::ConnectionLostChanged(lost)]
                }
            }
            AppAction::Raise => vec![AppEvent::Raised],
            // Cross-state actions: multiple "substates" are affected by these actions, that's why they're handled here
            // Might need some clean-up
            AppAction::QueueSelection => {
                let tracks = self.selection.take_selection();
                let mut events = self.queue_tracks(tracks);
                events.push(SelectionEvent::SelectionModeChanged(false).into());
                events
            }
            AppAction::QueueTracks { tracks } => self.queue_tracks(tracks),
            AppAction::DequeueSelection => {
                let keys: Vec<EntryKey> = self
                    .selection
                    .take_keyed_selection()
                    .iter()
                    .filter_map(|(key, _)| key.parse().ok())
                    .collect();
                self.playback.dequeue(&keys);

                vec![
                    SelectionEvent::SelectionModeChanged(false).into(),
                    PlaybackEvent::PlaylistChanged.into(),
                ]
            }
            AppAction::SaveSelection => {
                let tracks = self.selection.take_selection();
                let mut events: Vec<AppEvent> = forward_action(
                    BrowserAction::SaveTracks(tracks),
                    self.browser.home_state_mut().unwrap(),
                );
                events.push(SelectionEvent::SelectionModeChanged(false).into());
                events
            }
            AppAction::UnsaveSelection => {
                let tracks: Vec<String> = self
                    .selection
                    .take_selection()
                    .into_iter()
                    .map(|s| s.rri.id)
                    .collect();
                let mut events: Vec<AppEvent> = forward_action(
                    BrowserAction::RemoveSavedTracks(tracks),
                    self.browser.home_state_mut().unwrap(),
                );
                events.push(SelectionEvent::SelectionModeChanged(false).into());
                events
            }
            AppAction::EnableSelection(context) => {
                if let Some(active) = self.selection.set_mode(Some(context)) {
                    vec![SelectionEvent::SelectionModeChanged(active).into()]
                } else {
                    vec![]
                }
            }
            AppAction::CancelSelection => {
                if let Some(active) = self.selection.set_mode(None) {
                    vec![SelectionEvent::SelectionModeChanged(active).into()]
                } else {
                    vec![]
                }
            }
            AppAction::CreatePlaylist(playlist) => {
                let id = playlist.rri.id.clone();
                let mut events = forward_action(
                    LoginAction::PrependUserPlaylist(vec![playlist.clone().into()]),
                    &mut self.logged_user,
                );
                let mut more_events = forward_action(
                    BrowserAction::PrependPlaylistsContent(vec![playlist]),
                    &mut self.browser,
                );
                events.append(&mut more_events);
                events.push(AppEvent::PlaylistCreatedNotificationShown(id));
                events
            }
            AppAction::UpdatePlaylistName(s) => {
                let mut events = forward_action(
                    LoginAction::UpdateUserPlaylist(s.clone()),
                    &mut self.logged_user,
                );
                let mut more_events =
                    forward_action(BrowserAction::UpdatePlaylistName(s), &mut self.browser);
                events.append(&mut more_events);
                events
            }
            AppAction::RemovePlaylist(id) => {
                let mut events = forward_action(
                    LoginAction::RemoveUserPlaylist(id.clone()),
                    &mut self.logged_user,
                );
                let mut more_events =
                    forward_action(BrowserAction::RemovePlaylist(id), &mut self.browser);
                events.append(&mut more_events);
                events
            }
            AppAction::BrowserAction(BrowserAction::SavePlaylist(playlist)) => {
                let mut events = forward_action(
                    LoginAction::PrependUserPlaylist(vec![PlaylistSummary {
                        id: playlist.rri.id.clone(),
                        title: playlist.title.clone(),
                    }]),
                    &mut self.logged_user,
                );
                let mut more_events =
                    forward_action(BrowserAction::SavePlaylist(playlist), &mut self.browser);
                events.append(&mut more_events);
                events
            }
            AppAction::BrowserAction(BrowserAction::UnsavePlaylist(id)) => {
                let mut events = forward_action(
                    LoginAction::RemoveUserPlaylist(id.clone()),
                    &mut self.logged_user,
                );
                let mut more_events =
                    forward_action(BrowserAction::UnsavePlaylist(id), &mut self.browser);
                events.append(&mut more_events);
                events
            }
            // As for all other actions, we forward them to the substates :)
            AppAction::PlaybackAction(a) => forward_action(a, &mut self.playback),
            AppAction::BrowserAction(a) => forward_action(a, &mut self.browser),
            AppAction::SelectionAction(a) => forward_action(a, &mut self.selection),
            AppAction::LoginAction(a) => forward_action(a, &mut self.logged_user),
            AppAction::SettingsAction(a) => forward_action(a, &mut self.settings),
            _ => vec![],
        }
    }
}

impl AppState {
    fn queue_tracks(&mut self, tracks: Vec<Track>) -> Vec<AppEvent> {
        if tracks.is_empty() {
            return vec![];
        }
        if matches!(self.playback.current_device(), Device::Connect(_)) {
            return vec![AppEvent::NotificationShown(gettext(
                "Queueing isn't available when playing on another device",
            ))];
        }
        self.playback.queue(tracks);
        vec![
            PlaybackEvent::PlaylistChanged.into(),
            AppEvent::NotificationShown(labels::ADDED_TO_QUEUE.clone()),
        ]
    }
}

fn forward_action<A, E>(
    action: A,
    target: &mut impl UpdatableState<Action = A, Event = E>,
) -> Vec<AppEvent>
where
    A: Clone,
    E: Into<AppEvent>,
{
    target
        .update_with(Cow::Owned(action))
        .into_iter()
        .map(|e| e.into())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::models::make_track;
    use crate::app::state::{load_context, start_actions};

    #[test]
    fn test_queue_and_dequeue_selection() {
        let mut state = AppState::new();
        let tracks = ["1", "2"].iter().map(|id| make_track(id)).collect();
        state.update_state(
            PlaybackAction::LoadContextSongs(crate::app::SongsSource::Album("a".into()), tracks)
                .into(),
        );
        state.update_state(PlaybackAction::Load("1".to_string()).into());
        state.update_state(AppAction::EnableSelection(SelectionContext::Default));
        state.update_state(
            SelectionAction::Select(vec![make_track("a"), make_track("a"), make_track("b")]).into(),
        );

        let events = state.update_state(AppAction::QueueSelection);
        assert!(events
            .iter()
            .any(|e| matches!(e, AppEvent::NotificationShown(_))));
        assert!(!state.selection.is_selection_enabled());
        state.update_state(AppAction::QueueTracks {
            tracks: vec![make_track("a")],
        });
        assert_eq!(state.playback.upcoming_ids(), ["1", "a", "b", "a", "2"]);

        state.update_state(AppAction::EnableSelection(SelectionContext::Queue));
        let key = state.playback.upcoming_keys()[2];
        let track = state.playback.view_track(key).unwrap();
        state.update_state(SelectionAction::SelectKeyed(vec![(key.to_string(), track)]).into());
        assert_eq!(state.selection.count(), 1);
        state.update_state(AppAction::DequeueSelection);
        assert_eq!(state.playback.upcoming_ids(), ["1", "a", "b", "2"]);
    }

    #[test]
    fn test_start_actions_replace_the_queue_and_play_the_first_track() {
        let mut state = AppState::new();
        let album = crate::app::SongsSource::Album("a".into());
        let ids: Vec<String> = (0..20).map(|i| i.to_string()).collect();
        let start = |shuffle| {
            let tracks = ids.iter().map(|id| make_track(id)).collect();
            let load = PlaybackAction::LoadContextSongs(album.clone(), tracks);
            start_actions(shuffle, load_context(album.clone(), Some("A".into()), load))
        };
        state.update_state(AppAction::QueueTracks {
            tracks: vec![make_track("q")],
        });

        for action in start(false) {
            state.update_state(action);
        }
        assert_eq!(state.playback.upcoming_ids(), ids);
        assert!(state.playback.is_playing());

        for action in start(true) {
            state.update_state(action);
        }
        let upcoming = state.playback.upcoming_ids();
        assert!(state.playback.is_shuffled());
        assert_eq!(upcoming.len(), 20);
        assert_ne!(upcoming, ids);
    }
}
