use std::cell::Ref;
use std::collections::HashSet;
use std::ops::Deref;
use std::rc::Rc;

use gio::prelude::*;
use gio::SimpleActionGroup;

use crate::app::components::{
    build_song_menu, dispatch_api_call, queue_source_tracks, QueueMenuEntry, SongActions,
};
use crate::app::dispatch::Dispatcher;
use crate::app::models::{SongListModel, Track};
use crate::app::state::{
    load_context, start_actions, BrowserAction, PlaybackAction, SelectionAction, SelectionContext,
    SelectionState,
};
use crate::app::{AppAction, AppModel, AppState, SongsSource};
use crate::feature_flags::{self, FeatureFlag};
use crate::settings;

/// Generates the boilerplate TrackListModel methods that delegate to `self.base`.
/// Use inside an `impl TrackListModel for X { ... }` block.
#[macro_export]
macro_rules! impl_track_list_model_base {
    () => {
        fn is_paused(&self) -> bool {
            self.base.is_paused()
        }
        fn current_song_id(&self) -> Option<String> {
            self.base.current_song_id()
        }
        fn select_song(&self, id: &str) {
            self.select_song_from_list(&TrackListModel::song_list_model(self), id);
        }
        fn deselect_song(&self, id: &str) {
            self.base.deselect_song(id);
        }
        fn selection(&self) -> Option<Box<dyn Deref<Target = SelectionState> + '_>> {
            self.base.selection()
        }
        fn is_song_liked(&self, id: &str) -> bool {
            self.base.is_song_liked(id)
        }
        fn toggle_song_like(&self, id: &str) {
            let songs = TrackListModel::song_list_model(self);
            self.base.toggle_song_like(&songs, id);
        }
        fn pinned_song_ids(&self) -> Option<std::collections::HashSet<String>> {
            self.base.pinned_song_ids()
        }
        fn toggle_song_pin(&self, song: &Track) {
            self.base.toggle_song_pin(song);
        }
        fn skip_explicit(&self) -> bool {
            self.base.skip_explicit()
        }
    };
}

/// The `PageModel` methods of a page with a source (see
/// `PageModel::songs_source`).
#[macro_export]
macro_rules! impl_source_page {
    () => {
        fn source_is_playing(&self) -> bool {
            self.base
                .is_playing_source(PageModel::songs_source(self).as_ref())
        }

        fn start_play(&self, id: &str) {
            let songs = TrackListModel::song_list_model(self);
            match songs.find_index(id) {
                Some(index) => TrackListModel::play_song_at(self, index, id),
                None => error!("Failed to play track {id}"),
            }
        }

        fn toggle_play(&self) {
            self.base.toggle_playback(self.source_is_playing(), || {
                TrackListModel::context_actions(self, 0)
            });
        }

        fn shuffle_play(&self) {
            self.base
                .start_playback(true, TrackListModel::context_actions(self, 0));
        }

        fn queue_all(&self) {
            if let Some(source) = PageModel::songs_source(self) {
                self.base
                    .queue_source(source, &TrackListModel::song_list_model(self));
            }
        }

        fn on_share_clicked(&self) {
            if let Some(url) = PageModel::songs_source(self).and_then(|s| s.spotify_url()) {
                self.base.share_link(&url);
            }
        }
    };
}

/// The `TrackListModel` methods of a page with a source.
#[macro_export]
macro_rules! impl_source_track_list {
    () => {
        fn load_more(&self) {
            PageModel::load_more(self);
        }

        fn play_song_at(&self, pos: usize, id: &str) {
            self.play_in_context(TrackListModel::context_actions(self, pos), id);
        }

        fn context_actions(&self, pos: usize) -> Option<Vec<$crate::app::AppAction>> {
            $crate::app::components::source_context_actions(
                PageModel::songs_source(self)?,
                PageModel::context_name(self),
                &TrackListModel::song_list_model(self),
                pos,
            )
        }

        fn actions_for(
            &self,
            _row: &$crate::app::models::SongModel,
            song: &$crate::app::models::Track,
        ) -> Option<gio::SimpleActionGroup> {
            Some(
                self.base
                    .track_actions(song, PageModel::songs_source(self).as_ref()),
            )
        }

        fn menu_for(
            &self,
            _row: &$crate::app::models::SongModel,
            song: &$crate::app::models::Track,
            liked: bool,
            pinned: Option<bool>,
        ) -> Option<gio::MenuModel> {
            Some($crate::app::components::track_menu(
                song,
                PageModel::songs_source(self).as_ref(),
                liked,
                pinned,
            ))
        }
    };
}

/// Loads the page holding `pos` for a paginated source, all of `songs`
/// otherwise.
pub fn source_context_actions(
    source: SongsSource,
    name: Option<String>,
    songs: &SongListModel,
    pos: usize,
) -> Option<Vec<AppAction>> {
    let load = if source.is_paginated() {
        PlaybackAction::LoadPagedSongs(source.clone(), songs.song_batch_for(pos)?)
    } else {
        let tracks: Vec<Track> = songs.collect();
        if tracks.is_empty() {
            return None;
        }
        PlaybackAction::LoadContextSongs(source.clone(), tracks)
    };
    Some(load_context(source, name, load))
}

/// No "View Album" on its album's page, nor "More from" on its artist's.
pub fn track_menu(
    song: &Track,
    source: Option<&SongsSource>,
    liked: bool,
    pinned: Option<bool>,
) -> gio::MenuModel {
    let show_view_album = !matches!(source, Some(SongsSource::Album(_)));
    let artist = match source {
        Some(SongsSource::Artist(id)) => Some(id.as_str()),
        _ => None,
    };
    build_song_menu(
        song,
        show_view_album,
        artist,
        QueueMenuEntry::Add,
        Some(liked),
        pinned,
    )
}

/// Base struct shared by all detail page models.
///
/// Holds the common fields (`id`, `app_model`, `dispatcher`) and provides
/// methods that every detail page model needs. Concrete models compose this
/// via `Deref` to inherit these methods automatically.
pub struct DetailsPageModel {
    pub id: String,
    pub app_model: Rc<AppModel>,
    pub dispatcher: Dispatcher,
}

impl DetailsPageModel {
    pub fn new(id: String, app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        Self {
            id,
            app_model,
            dispatcher,
        }
    }

    pub fn new_without_id(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        Self::new(String::new(), app_model, dispatcher)
    }

    pub fn state(&self) -> Ref<'_, AppState> {
        self.app_model.get_state()
    }

    #[allow(dead_code)]
    pub fn dispatcher(&self) -> &Dispatcher {
        &self.dispatcher
    }

    /// Copy a shareable link to the clipboard and show a confirmation toast.
    pub fn is_playing_source(&self, source: Option<&SongsSource>) -> bool {
        source.is_some() && self.state().playback.current_source() == source
    }

    /// Every page of a paginated source, else `songs` as listed.
    pub fn queue_source(&self, source: SongsSource, songs: &SongListModel) {
        if source.is_paginated() {
            queue_source_tracks(&self.app_model, &self.dispatcher, source);
        } else {
            let tracks: Vec<Track> = songs.collect();
            let tracks = tracks.into_iter().filter(|t| t.playable).collect();
            self.dispatcher.dispatch(AppAction::QueueTracks { tracks });
        }
    }

    pub fn track_actions(&self, song: &Track, source: Option<&SongsSource>) -> SimpleActionGroup {
        let group = SimpleActionGroup::new();
        for action in song.make_artist_actions(self.dispatcher.clone()) {
            group.add_action(&action);
        }
        if !matches!(source, Some(SongsSource::Album(_))) {
            group.add_action(&song.make_album_action(self.dispatcher.clone()));
        }
        group.add_action(&song.make_link_action());
        for action in song.make_queue_actions(self.dispatcher.clone()) {
            group.add_action(&action);
        }
        group
    }

    pub fn share_link(&self, link: &str) {
        crate::app::components::copy_link_to_clipboard(link);
        self.dispatcher
            .dispatch(AppAction::ShowNotification(gettextrs::gettext(
                "Link copied to clipboard",
            )));
    }

    // Playback state helpers

    pub fn is_paused(&self) -> bool {
        !self.state().playback.is_playing()
    }

    pub fn is_playing(&self) -> bool {
        self.state().playback.is_playing()
    }

    pub fn current_song_id(&self) -> Option<String> {
        self.state().playback.current_song_id()
    }

    pub fn skip_explicit(&self) -> bool {
        self.state().playback.skip_explicit()
    }

    // Selection helpers

    pub fn deselect_song(&self, id: &str) {
        self.dispatcher
            .dispatch(SelectionAction::Deselect(vec![id.to_string()]).into());
    }

    pub fn selection(&self) -> Option<Box<dyn Deref<Target = SelectionState> + '_>> {
        Some(Box::new(self.app_model.map_state(|s| &s.selection)))
    }

    pub fn select_song_from_list(&self, song_list: &SongListModel, id: &str) {
        if let Some(song) = song_list.get(id) {
            self.dispatcher
                .dispatch(SelectionAction::Select(vec![song.description().clone()]).into());
        }
    }

    pub fn enable_selection_with_context(&self, context: SelectionContext) -> bool {
        if !feature_flags::is_enabled(FeatureFlag::SelectMode) {
            return false;
        }
        self.dispatcher
            .dispatch(AppAction::EnableSelection(context));
        true
    }

    // Playback control helpers

    /// Toggle play/pause, or start the source over with `context`.
    pub fn toggle_playback(
        &self,
        source_is_playing: bool,
        context: impl FnOnce() -> Option<Vec<AppAction>>,
    ) {
        if !source_is_playing {
            self.start_playback(false, context());
        } else if self.is_playing() {
            self.dispatcher.dispatch(PlaybackAction::Pause.into());
        } else {
            self.dispatcher.dispatch(PlaybackAction::Play.into());
        }
    }

    pub fn start_playback(&self, shuffle: bool, context: Option<Vec<AppAction>>) {
        if let Some(context) = context {
            self.dispatcher
                .dispatch_many(start_actions(shuffle, context));
        }
    }

    pub fn play_in_context(&self, context: Option<Vec<AppAction>>, id: &str) {
        if let Some(mut actions) = context {
            actions.push(PlaybackAction::Load(id.to_string()).into());
            self.dispatcher.dispatch_many(actions);
        }
    }

    // Liked song helpers
    pub fn is_song_liked(&self, id: &str) -> bool {
        let state = self.app_model.get_state();
        if let Some(home) = state.browser.home_state() {
            return home.is_track_liked(id);
        }
        false
    }

    /// IDs of the tracks pinned to the navigation panel, or `None` when pinning is
    /// disabled or no user is logged in.
    pub fn pinned_song_ids(&self) -> Option<HashSet<String>> {
        if !feature_flags::is_enabled(FeatureFlag::PinnedObjects) {
            return None;
        }
        let user_id = self.app_model.get_state().logged_user.user.clone()?;
        Some(
            settings::get_pinned_objects(&user_id)
                .into_iter()
                .filter(|o| o.kind == settings::PinnedKind::Track)
                .map(|o| o.id)
                .collect(),
        )
    }

    pub fn toggle_song_pin(&self, song: &Track) {
        let Some(user_id) = self.app_model.get_state().logged_user.user.clone() else {
            return;
        };
        let kind = settings::PinnedKind::Track;
        let id = &song.rri.id;
        let changed = if settings::is_object_pinned(&user_id, id, kind) {
            settings::unpin_object(&user_id, kind, id)
        } else if !self.is_song_liked(id) {
            // Only saved tracks can be pinned.
            return;
        } else {
            settings::pin_object(&user_id, kind, id, Some(song.title.clone()))
        };
        if changed {
            self.dispatcher
                .dispatch(BrowserAction::NotifyPinnedPlaylistsUpdated.into());
        }
    }

    pub fn toggle_song_like(&self, song_list: &SongListModel, id: &str) {
        let Some(song) = song_list.get(id) else {
            return;
        };
        let song_desc = song.into_description();
        let song_id = song_desc.rri.id.clone();
        let api = self.app_model.api();
        let is_liked = self.is_song_liked(id);

        if is_liked {
            dispatch_api_call(&self.dispatcher, move || async move {
                api.remove_tracks(vec![song_id.clone()]).await?;
                Ok(BrowserAction::RemoveSavedTracks(vec![song_id]).into())
            });
        } else {
            dispatch_api_call(&self.dispatcher, move || async move {
                api.save_tracks(vec![song_id]).await?;
                Ok(BrowserAction::SaveTracks(vec![song_desc]).into())
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::sync::Arc;

    use crate::app::components::details_page::is_playback_event;
    use crate::app::models::*;
    use crate::app::state::{BrowserEvent, PlaybackEvent};
    use crate::app::AppEvent;

    // Dispatcher harness
    struct TestDispatcher {
        dispatcher: Dispatcher,
        receiver: RefCell<futures::channel::mpsc::UnboundedReceiver<AppAction>>,
        received: RefCell<Vec<AppAction>>,
    }

    impl TestDispatcher {
        fn new() -> Self {
            let (sender, receiver) = futures::channel::mpsc::unbounded();
            Self {
                dispatcher: Dispatcher::new(sender),
                receiver: RefCell::new(receiver),
                received: RefCell::new(Vec::new()),
            }
        }

        fn dispatcher(&self) -> Dispatcher {
            self.dispatcher.clone()
        }

        fn pump(&self) {
            while let Ok(Some(action)) = self.receiver.borrow_mut().try_next() {
                self.received.borrow_mut().push(action);
            }
        }

        fn dispatched(&self) -> Vec<AppAction> {
            self.pump();
            self.received.borrow().clone()
        }

        fn last_action(&self) -> Option<AppAction> {
            self.pump();
            self.received.borrow().last().cloned()
        }

        fn clear(&self) {
            self.pump();
            self.received.borrow_mut().clear();
        }
    }

    // Api service fixture

    /// An `ApiService` that is never called; `AppModel` just needs one to exist.
    /// No request is issued and no disk is touched, since cache directories are
    /// created on first write. Shared, to avoid an HTTP client per test.
    fn test_api_service() -> Arc<riff_api::ApiService> {
        thread_local! {
            static API: Arc<riff_api::ApiService> = {
                let token_provider: Arc<dyn riff_api::TokenProvider> =
                    Arc::new(|| None::<String>);
                Arc::new(riff_api::spotify_service(
                    token_provider,
                    1024 * 1024,
                    1024 * 1024,
                ))
            };
        }
        API.with(Arc::clone)
    }

    // Test helpers

    fn make_model() -> (DetailsPageModel, TestDispatcher) {
        let dispatcher = TestDispatcher::new();
        let app_model = Rc::new(AppModel::new(AppState::new(), test_api_service()));
        let model =
            DetailsPageModel::new("test-id".to_string(), app_model, dispatcher.dispatcher());
        (model, dispatcher)
    }

    fn make_model_playing() -> (DetailsPageModel, TestDispatcher) {
        let dispatcher = TestDispatcher::new();
        let app_model = Rc::new(AppModel::new(AppState::new(), test_api_service()));
        #[allow(deprecated)]
        app_model.update_state(PlaybackAction::LoadSongs(vec![song("s1"), song("s2")]).into());
        app_model.update_state(PlaybackAction::Load("s1".to_string()).into());
        let model =
            DetailsPageModel::new("test-id".to_string(), app_model, dispatcher.dispatcher());
        (model, dispatcher)
    }

    fn song(id: &str) -> Track {
        make_track(id)
    }

    fn make_song_list(songs: Vec<Track>) -> SongListModel {
        let mut list = SongListModel::new(50);
        let _ = list.add(Page {
            items: songs,
            offset: Some(0),
            total: None,
            next_cursor: None,
        });
        list
    }

    // Tests: is_playback_event

    #[test]
    fn test_is_playback_event_paused() {
        let event = AppEvent::PlaybackEvent(PlaybackEvent::PlaybackPaused);
        assert_eq!(is_playback_event(&event), Some(false));
    }

    #[test]
    fn test_is_playback_event_resumed() {
        let event = AppEvent::PlaybackEvent(PlaybackEvent::PlaybackResumed);
        assert_eq!(is_playback_event(&event), Some(true));
    }

    #[test]
    fn test_is_playback_event_track_changed() {
        let event = AppEvent::PlaybackEvent(PlaybackEvent::TrackChanged("x".to_string()));
        assert_eq!(is_playback_event(&event), Some(true));
    }

    #[test]
    fn test_is_playback_event_non_playback() {
        let event = AppEvent::BrowserEvent(BrowserEvent::SavedTracksUpdated);
        assert_eq!(is_playback_event(&event), None);
    }

    // Tests: playback state helpers

    #[test]
    fn test_initial_state_is_paused() {
        let (model, _) = make_model();
        assert!(model.is_paused());
        assert!(!model.is_playing());
        assert_eq!(model.current_song_id(), None);
    }

    #[test]
    fn test_playing_state() {
        let (model, _) = make_model_playing();
        assert!(model.is_playing());
        assert!(!model.is_paused());
        assert_eq!(model.current_song_id(), Some("s1".to_string()));
    }

    // Tests: selection helpers

    #[test]
    fn test_deselect_song_dispatches() {
        let (model, dispatcher) = make_model();
        model.deselect_song("song-1");
        let action = dispatcher.last_action().unwrap();
        assert!(
            matches!(action, AppAction::SelectionAction(SelectionAction::Deselect(ids)) if ids == vec!["song-1".to_string()])
        );
    }

    #[test]
    fn test_selection_returns_state() {
        let (model, _) = make_model();
        let sel = model.selection();
        assert!(sel.is_some());
        assert!(!sel.unwrap().is_selection_enabled());
    }

    #[test]
    fn test_select_song_from_list_dispatches() {
        let (model, dispatcher) = make_model();
        let list = make_song_list(vec![song("a"), song("b")]);
        model.select_song_from_list(&list, "b");
        let action = dispatcher.last_action().unwrap();
        assert!(
            matches!(action, AppAction::SelectionAction(SelectionAction::Select(songs)) if songs.len() == 1 && songs[0].rri.id == "b")
        );
    }

    #[test]
    fn test_select_song_from_list_nonexistent() {
        let (model, dispatcher) = make_model();
        let list = make_song_list(vec![song("a")]);
        model.select_song_from_list(&list, "nonexistent");
        assert!(dispatcher.last_action().is_none());
    }

    #[test]
    fn test_enable_selection_with_context() {
        let (model, dispatcher) = make_model();
        let result = model.enable_selection_with_context(SelectionContext::Default);
        if result {
            let action = dispatcher.last_action().unwrap();
            assert!(matches!(
                action,
                AppAction::EnableSelection(SelectionContext::Default)
            ));
        }
    }

    #[test]
    fn test_source_helpers() {
        use crate::app::models::ArtistRef;
        let album = SongsSource::Album("al".into());
        let artist = SongsSource::Artist("ar".into());
        assert_eq!(
            album.spotify_url().as_deref(),
            Some("https://open.spotify.com/album/al")
        );
        assert_eq!(SongsSource::SavedTracks.spotify_url(), None);

        let mut track = song("t");
        track.artists = vec![ArtistRef {
            rri: ResourceId {
                id: "ar".into(),
                ..Default::default()
            },
            name: "Artist".into(),
        }];
        let entries = |source: Option<&SongsSource>| -> Vec<String> {
            let menu = track_menu(&track, source, false, None);
            (0..menu.n_items())
                .filter_map(|i| menu.item_link(i, gio::MENU_LINK_SECTION))
                .flat_map(|section| {
                    (0..section.n_items())
                        .filter_map(|j| {
                            section
                                .item_attribute_value(j, gio::MENU_ATTRIBUTE_ACTION, None)
                                .and_then(|v| v.get::<String>())
                        })
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        let has = |source, action: &str| entries(source).iter().any(|a| a == action);
        assert!(has(None, "song.view_album") && has(None, "song.view_artist_ar"));
        assert!(!has(Some(&album), "song.view_album"));
        assert!(!has(Some(&artist), "song.view_artist_ar"));
        assert!(has(Some(&album), "song.queue"));

        let (model, dispatcher) = make_model();
        assert!(model
            .track_actions(&track, Some(&album))
            .lookup_action("view_album")
            .is_none());
        assert!(model
            .track_actions(&track, None)
            .lookup_action("view_album")
            .is_some());

        let mut unplayable = song("u");
        unplayable.playable = false;
        let list = make_song_list(vec![song("a"), unplayable]);
        let paged = source_context_actions(album.clone(), Some("A".into()), &list, 0).unwrap();
        assert!(matches!(
            paged[..],
            [
                AppAction::PlaybackAction(PlaybackAction::SetContextName(..)),
                AppAction::PlaybackAction(PlaybackAction::LoadPagedSongs(..))
            ]
        ));
        let whole = source_context_actions(artist.clone(), None, &list, 0).unwrap();
        assert!(matches!(
            whole[..],
            [AppAction::PlaybackAction(PlaybackAction::LoadContextSongs(_, ref tracks))] if tracks.len() == 2
        ));
        assert!(source_context_actions(artist.clone(), None, &make_song_list(vec![]), 0).is_none());

        model.queue_source(artist, &list);
        assert!(matches!(
            dispatcher.last_action(),
            Some(AppAction::QueueTracks { ref tracks }) if tracks.len() == 1
        ));
        assert!(!model.is_playing_source(Some(&album)));
        assert!(!model.is_playing_source(None));
    }

    fn kinds(actions: &[AppAction]) -> Vec<&'static str> {
        actions
            .iter()
            .map(|a| match a {
                AppAction::PlaybackAction(PlaybackAction::SetShuffled(_)) => "SetShuffled",
                AppAction::PlaybackAction(PlaybackAction::ReplaceQueue) => "ReplaceQueue",
                AppAction::PlaybackAction(PlaybackAction::LoadContextSongs(..)) => {
                    "LoadContextSongs"
                }
                AppAction::PlaybackAction(PlaybackAction::Load(_)) => "Load",
                AppAction::PlaybackAction(PlaybackAction::Play) => "Play",
                _ => "other",
            })
            .collect()
    }

    #[test]
    fn test_playback_helpers() {
        let context = || {
            Some(vec![PlaybackAction::LoadContextSongs(
                crate::app::SongsSource::Artist("a".into()),
                vec![song("x")],
            )
            .into()])
        };

        let (model, dispatcher) = make_model();
        model.toggle_playback(false, context);
        let actions = dispatcher.dispatched();
        assert!(matches!(
            actions[0],
            AppAction::PlaybackAction(PlaybackAction::SetShuffled(false))
        ));
        assert_eq!(
            kinds(&actions),
            ["SetShuffled", "ReplaceQueue", "LoadContextSongs", "Play"]
        );

        dispatcher.clear();
        model.start_playback(true, context());
        assert!(matches!(
            dispatcher.dispatched()[0],
            AppAction::PlaybackAction(PlaybackAction::SetShuffled(true))
        ));

        dispatcher.clear();
        model.toggle_playback(false, || None);
        model.play_in_context(None, "x");
        assert!(dispatcher.dispatched().is_empty());
        model.play_in_context(context(), "x");
        assert_eq!(
            kinds(&dispatcher.dispatched()),
            ["LoadContextSongs", "Load"]
        );

        dispatcher.clear();
        model.toggle_playback(true, || panic!("should not start playback"));
        assert!(matches!(
            dispatcher.last_action(),
            Some(AppAction::PlaybackAction(PlaybackAction::Play))
        ));
        let (model, dispatcher) = make_model_playing();
        model.toggle_playback(true, || panic!("should not start playback"));
        assert!(matches!(
            dispatcher.last_action(),
            Some(AppAction::PlaybackAction(PlaybackAction::Pause))
        ));
    }

    // Tests: construction

    #[test]
    fn test_new_stores_id() {
        let (model, _) = make_model();
        assert_eq!(model.id, "test-id");
    }

    #[test]
    fn test_new_without_id() {
        let dispatcher = TestDispatcher::new();
        let app_model = Rc::new(AppModel::new(AppState::new(), test_api_service()));
        let model = DetailsPageModel::new_without_id(app_model, dispatcher.dispatcher());
        assert_eq!(model.id, "");
    }
}
