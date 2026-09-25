use std::cell::Ref;
use std::ops::Deref;
use std::rc::Rc;

use crate::app::components::dispatch_api_call;
use crate::app::dispatch::Dispatcher;
use crate::app::models::SongListModel;
use crate::app::state::{
    BrowserAction, PlaybackAction, SelectionAction, SelectionContext, SelectionState,
};
use crate::app::{AppAction, AppModel, AppState};
use crate::feature_flags::{self, FeatureFlag};

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
        fn skip_explicit(&self) -> bool {
            self.base.skip_explicit()
        }
    };
}

/// Generates the standard `toggle_play` and `shuffle_play` methods for models
/// that implement both `PageModel` and `TrackListModel`.
#[macro_export]
macro_rules! impl_toggle_play {
    () => {
        fn start_play(&self, id: &str) {
            let songs = TrackListModel::song_list_model(self);
            match songs.find_index(id) {
                Some(index) => TrackListModel::play_song_at(self, index, id),
                None => error!("Failed to play track {id}"),
            }
        }
        fn toggle_play(&self) {
            let songs = TrackListModel::song_list_model(self);
            self.base
                .toggle_playback(self.source_is_playing(), &songs, |pos, id| {
                    TrackListModel::play_song_at(self, pos, id);
                });
        }

        fn shuffle_play(&self) {
            let songs = TrackListModel::song_list_model(self);
            self.base.shuffle_playback(&songs, |pos, id| {
                TrackListModel::play_song_at(self, pos, id);
            });
        }
    };
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

    pub fn is_shuffled(&self) -> bool {
        self.state().playback.is_shuffled()
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

    /// Toggle play/pause. If not currently playing this source, starts playback
    /// (with shuffle disabled). If already playing, toggles pause/resume.
    pub fn toggle_playback(
        &self,
        source_is_playing: bool,
        song_list: &SongListModel,
        play_song_at: impl FnOnce(usize, &str),
    ) {
        if !source_is_playing {
            self.start_playback(false, song_list, play_song_at);
        } else if self.is_playing() {
            self.dispatcher.dispatch(PlaybackAction::Pause.into());
        } else {
            self.dispatcher.dispatch(PlaybackAction::Play.into());
        }
    }

    /// Start playback in shuffle mode.
    pub fn shuffle_playback(
        &self,
        song_list: &SongListModel,
        play_song_at: impl FnOnce(usize, &str),
    ) {
        self.start_playback(true, song_list, play_song_at);
    }

    /// Start playback. When shuffle is enabled, picks a random track; otherwise starts from the first.
    pub fn start_playback(
        &self,
        shuffle: bool,
        song_list: &SongListModel,
        play_song_at: impl FnOnce(usize, &str),
    ) {
        if shuffle != self.is_shuffled() {
            self.dispatcher
                .dispatch(PlaybackAction::ToggleShuffle.into());
        }
        let len = song_list.partial_len();
        if len == 0 {
            return;
        }
        let index = if shuffle {
            rand::random::<usize>() % len
        } else {
            0
        };
        if let Some(song) = song_list.index(index) {
            play_song_at(index, &song.get_id());
        }
    }

    // Liked song helpers
    pub fn is_song_liked(&self, id: &str) -> bool {
        let state = self.app_model.get_state();
        if let Some(home) = state.browser.home_state() {
            return home.saved_tracks.get(id).is_some();
        }
        false
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
        assert!(!model.is_shuffled());
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

    // Tests: playback control helpers

    #[test]
    fn test_toggle_playback_starts_when_not_playing_source() {
        let (model, dispatcher) = make_model();
        let list = make_song_list(vec![song("first"), song("second")]);
        let called = Rc::new(RefCell::new(None));
        let called_clone = called.clone();

        model.toggle_playback(false, &list, move |pos, id| {
            *called_clone.borrow_mut() = Some((pos, id.to_string()));
        });

        assert_eq!(*called.borrow(), Some((0, "first".to_string())));
        assert!(dispatcher.dispatched().is_empty());
    }

    #[test]
    fn test_toggle_playback_pauses_when_playing() {
        let (model, dispatcher) = make_model_playing();
        let list = make_song_list(vec![song("x")]);

        model.toggle_playback(true, &list, |_, _| panic!("should not start playback"));

        let action = dispatcher.last_action().unwrap();
        assert!(matches!(
            action,
            AppAction::PlaybackAction(PlaybackAction::Pause)
        ));
    }

    #[test]
    fn test_toggle_playback_resumes_when_paused_but_source_playing() {
        let (model, dispatcher) = make_model();
        let list = make_song_list(vec![song("x")]);

        model.toggle_playback(true, &list, |_, _| panic!("should not start playback"));

        let action = dispatcher.last_action().unwrap();
        assert!(matches!(
            action,
            AppAction::PlaybackAction(PlaybackAction::Play)
        ));
    }

    #[test]
    fn test_shuffle_playback_enables_shuffle() {
        let (model, dispatcher) = make_model();
        let list = make_song_list(vec![song("a"), song("b")]);
        let called = Rc::new(RefCell::new(None));
        let called_clone = called.clone();

        model.shuffle_playback(&list, move |pos, id| {
            *called_clone.borrow_mut() = Some((pos, id.to_string()));
        });

        let actions = dispatcher.dispatched();
        assert!(actions
            .iter()
            .any(|a| matches!(a, AppAction::PlaybackAction(PlaybackAction::ToggleShuffle))));
        let (pos, id) = called
            .borrow()
            .clone()
            .expect("play_song_at should be called");
        assert!(pos < 2);
        assert!(id == "a" || id == "b");
    }

    #[test]
    fn test_start_playback_disables_shuffle_when_not_wanted() {
        let (model, dispatcher) = make_model_playing();
        model
            .app_model
            .update_state(PlaybackAction::ToggleShuffle.into());
        assert!(model.is_shuffled());
        dispatcher.clear();

        let list = make_song_list(vec![song("x")]);
        model.start_playback(false, &list, |_, _| {});

        let actions = dispatcher.dispatched();
        assert!(actions
            .iter()
            .any(|a| matches!(a, AppAction::PlaybackAction(PlaybackAction::ToggleShuffle))));
    }

    #[test]
    fn test_start_playback_empty_list() {
        let (model, dispatcher) = make_model();
        let list = make_song_list(vec![]);
        let called = Rc::new(RefCell::new(false));
        let called_clone = called.clone();

        model.start_playback(false, &list, move |_, _| {
            *called_clone.borrow_mut() = true;
        });

        assert!(!*called.borrow());
        assert!(dispatcher.dispatched().is_empty());
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
