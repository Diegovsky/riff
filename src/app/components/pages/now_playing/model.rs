// Model for the now-playing/queue page.
// Provides current song info, queue track list, device selection,
// like/unlike for the current track, and artist navigation.

use gettextrs::gettext;
use gio::prelude::*;
use gio::SimpleActionGroup;
use std::ops::Deref;
use std::rc::Rc;

use crate::app::components::{
    labels, DetailsPageModel, DeviceSelectorModel, HasHeaderBarModel, HeaderImageShape, PageModel,
    PlaylistModel, SimpleHeaderBarModel,
};
use crate::app::models::{ImageSet, SongDescription, SongListModel};
use crate::app::state::Device;
use crate::app::state::{
    PlaybackAction, PlaybackEvent, PlaybackState, SelectionAction, SelectionContext, SelectionState,
};
use crate::app::{ActionDispatcher, AppAction, AppEvent, AppModel, BrowserAction, BrowserEvent};
use crate::feature_flags::{self, FeatureFlag};
use crate::impl_toggle_play;

/// Data model for the now-playing page. Composes `DetailsPageModel` via Deref.
pub struct NowPlayingModel {
    base: DetailsPageModel,
}

impl Deref for NowPlayingModel {
    type Target = DetailsPageModel;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl HasHeaderBarModel for NowPlayingModel {}

impl NowPlayingModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
        Self {
            base: DetailsPageModel::new_without_id(app_model, dispatcher),
        }
    }

    fn queue(&self) -> impl Deref<Target = PlaybackState> + '_ {
        self.app_model.map_state(|s| &s.playback)
    }

    fn current_song(&self) -> Option<SongDescription> {
        self.app_model.get_state().playback.current_song()
    }

    fn current_selection_context(&self) -> SelectionContext {
        match self.app_model.get_state().playback.current_device() {
            Device::Local => SelectionContext::Queue,
            Device::Connect(_) => SelectionContext::ReadOnlyQueue,
        }
    }

    pub fn device_selector_model(&self) -> DeviceSelectorModel {
        DeviceSelectorModel::new(self.app_model.clone(), self.dispatcher.box_clone())
    }
}

impl PageModel for NowPlayingModel {
    fn get_title(&self) -> Option<String> {
        Some(self.current_song()?.title.clone())
    }

    fn get_subtitle(&self) -> Option<String> {
        Some(self.current_song()?.artists_name())
    }

    fn get_caption(&self) -> Option<String> {
        Some(gettext("Now Playing"))
    }

    fn get_artwork(&self) -> Option<ImageSet> {
        self.current_song()?.art.clone()
    }

    fn header_image_shape(&self) -> HeaderImageShape {
        HeaderImageShape::Square
    }

    fn load_more(&self) {
        let queue = self.queue();
        let loader = self.app_model.get_batch_loader();
        let Some(query) = queue.next_query() else {
            return;
        };
        debug!("next_query = {:?}", &query);
        self.dispatcher.dispatch_async(Box::pin(async move {
            loader
                .query(query, |source, song_batch| {
                    PlaybackAction::LoadPagedSongs(source, song_batch).into()
                })
                .await
        }));
    }

    fn is_loaded(&self) -> bool {
        true
    }

    fn has_play_button(&self) -> bool {
        true
    }
    fn source_is_playing(&self) -> bool {
        true
    }

    impl_toggle_play!();

    fn has_like_button(&self) -> bool {
        true
    }

    fn is_liked(&self) -> bool {
        if let Some(song) = self.current_song() {
            let state = self.app_model.get_state();
            if let Some(home) = state.browser.home_state() {
                return home.saved_tracks.get(&song.id).is_some();
            }
        }
        false
    }

    fn toggle_like(&self) {
        let Some(song) = self.current_song() else {
            return;
        };
        let id = song.id.clone();
        let api = self.app_model.get_spotify();
        let is_liked = self.is_liked();

        if is_liked {
            self.dispatcher
                .call_spotify_and_dispatch(move || async move {
                    api.remove_saved_tracks(vec![id.clone()]).await?;
                    Ok(BrowserAction::RemoveSavedTracks(vec![id]).into())
                });
        } else {
            let song_desc = song.clone();
            self.dispatcher
                .call_spotify_and_dispatch(move || async move {
                    api.save_tracks(vec![id]).await?;
                    Ok(BrowserAction::SaveTracks(vec![song_desc]).into())
                });
        }
    }

    fn has_subtitle_link(&self) -> bool {
        true
    }

    fn on_subtitle_clicked(&self) {
        if let Some(song) = self.current_song() {
            if let Some(artist) = song.artists.first() {
                self.dispatcher
                    .dispatch(AppAction::ViewArtist(artist.id.clone()));
            }
        }
    }

    fn should_refresh_details(&self, event: &AppEvent) -> bool {
        matches!(
            event,
            AppEvent::PlaybackEvent(PlaybackEvent::TrackChanged(_))
        )
    }

    fn should_refresh_liked(&self, event: &AppEvent) -> bool {
        matches!(
            event,
            AppEvent::BrowserEvent(BrowserEvent::SavedTracksUpdated)
        )
    }
}

impl PlaylistModel for NowPlayingModel {
    fn song_list_model(&self) -> SongListModel {
        self.queue().songs().clone()
    }

    fn is_paused(&self) -> bool {
        self.base.is_paused()
    }
    fn current_song_id(&self) -> Option<String> {
        self.queue().current_song_id()
    }
    fn autoscroll_to_playing(&self) -> bool {
        false
    }
    fn deselect_song(&self, id: &str) {
        self.base.deselect_song(id);
    }
    fn selection(&self) -> Option<Box<dyn Deref<Target = SelectionState> + '_>> {
        self.base.selection()
    }

    fn play_song_at(&self, _pos: usize, id: &str) {
        self.dispatcher
            .dispatch(PlaybackAction::Load(id.to_string()).into());
    }

    fn select_song(&self, id: &str) {
        let queue = self.queue();
        if let Some(song) = queue.songs().get(id) {
            self.dispatcher
                .dispatch(SelectionAction::Select(vec![song.description().clone()]).into());
        }
    }

    fn enable_selection(&self) -> bool {
        if !feature_flags::is_enabled(FeatureFlag::SelectMode) {
            return false;
        }
        self.enable_selection_with_context(self.current_selection_context())
    }

    fn actions_for(&self, id: &str) -> Option<gio::ActionGroup> {
        let queue = self.queue();
        let song = queue.songs().get(id)?;
        let song = song.description();
        let group = SimpleActionGroup::new();
        for a in song.make_artist_actions(self.dispatcher.box_clone(), None) {
            group.add_action(&a);
        }
        group.add_action(&song.make_album_action(self.dispatcher.box_clone(), None));
        group.add_action(&song.make_link_action(None));
        group.add_action(&song.make_dequeue_action(self.dispatcher.box_clone(), None));
        Some(group.upcast())
    }

    fn menu_for(&self, id: &str) -> Option<gio::MenuModel> {
        let queue = self.queue();
        let song = queue.songs().get(id)?;
        let song = song.description();
        let menu = gio::Menu::new();
        menu.append(Some(&*labels::VIEW_ALBUM), Some("song.view_album"));
        for artist in song.artists.iter() {
            menu.append(
                Some(&labels::more_from_label(&artist.name)),
                Some(&format!("song.view_artist_{}", artist.id)),
            );
        }
        menu.append(Some(&*labels::COPY_LINK), Some("song.copy_link"));
        menu.append(Some(&*labels::REMOVE_FROM_QUEUE), Some("song.dequeue"));
        Some(menu.upcast())
    }
}

impl SimpleHeaderBarModel for NowPlayingModel {
    fn title(&self) -> Option<String> {
        Some(gettext("Now Playing"))
    }
    fn title_updated(&self, _: &AppEvent) -> bool {
        false
    }

    fn selection_context(&self) -> Option<SelectionContext> {
        if !feature_flags::is_enabled(FeatureFlag::SelectMode) {
            return None;
        }
        Some(self.current_selection_context())
    }

    fn select_all(&self) {
        let songs: Vec<SongDescription> = self.queue().songs().collect();
        self.dispatcher
            .dispatch(SelectionAction::Select(songs).into());
    }
}
