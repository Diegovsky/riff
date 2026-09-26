// Model for the now-playing/queue page.
// Provides current song info, queue track list, device selection,
// like/unlike for the current track, and artist navigation.

use gettextrs::gettext;
use gio::prelude::*;
use gio::SimpleActionGroup;
use std::ops::Deref;
use std::rc::Rc;

use crate::app::components::SongActions;
use crate::app::components::{
    build_song_menu, dispatch_api_call, dispatch_api_read, labels, DetailsPageModel,
    DeviceSelectorModel, HasHeaderBarModel, HeaderImageShape, PageModel, PinnedPageModel,
    QueueMenuEntry, SimpleHeaderBarModel, TrackListModel,
};
use crate::app::models::{ArtistRef, ImageSet, SongListModel, SongsSource, Track, TrackExt};
use crate::app::state::Device;
use crate::app::state::{
    PlaybackAction, PlaybackEvent, PlaybackState, SelectionAction, SelectionContext, SelectionState,
};
use crate::app::{AppAction, AppEvent, AppModel, BrowserAction, BrowserEvent, Dispatcher};
use crate::feature_flags::{self, FeatureFlag};
use crate::impl_toggle_play;
use crate::settings;

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
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        Self {
            base: DetailsPageModel::new_without_id(app_model, dispatcher),
        }
    }

    fn queue(&self) -> impl Deref<Target = PlaybackState> + '_ {
        self.app_model.map_state(|s| &s.playback)
    }

    fn current_song(&self) -> Option<Track> {
        self.app_model.get_state().playback.current_song()
    }

    fn current_selection_context(&self) -> SelectionContext {
        match self.app_model.get_state().playback.current_device() {
            Device::Local => SelectionContext::Queue,
            Device::Connect(_) => SelectionContext::ReadOnlyQueue,
        }
    }

    pub fn device_selector_model(&self) -> DeviceSelectorModel {
        DeviceSelectorModel::new(self.app_model.clone(), self.dispatcher.clone())
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
        Some(self.current_song()?.art.clone())
    }

    fn header_image_shape(&self) -> HeaderImageShape {
        HeaderImageShape::Square
    }

    fn load_more(&self) {
        let queue = self.queue();
        let Some((source, batch)) = queue.next_query() else {
            return;
        };
        let api = self.app_model.api();
        let offset = batch.offset;
        let batch_size = batch.batch_size;
        debug!(
            "next_query source={:?} offset={} size={}",
            &source, offset, batch_size
        );

        if matches!(&source, SongsSource::Artist(_) | SongsSource::Search(_)) {
            error!("non-paginated source in load_more, ignoring");
            return;
        }

        dispatch_api_read(&self.dispatcher, move |tag| async move {
            let song_batch = match &source {
                SongsSource::Playlist(id) => {
                    api.get_playlist_tracks(id, offset, batch_size, tag).await?
                }
                SongsSource::Album(id) => api.get_album_tracks(id, offset, batch_size, tag).await?,
                SongsSource::SavedTracks => api.get_saved_tracks(offset, batch_size, tag).await?,
                SongsSource::Artist(_) | SongsSource::Search(_) => unreachable!(),
            };
            Ok(PlaybackAction::LoadPagedSongs(source, song_batch).into())
        });
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

    fn like_tooltip(&self, is_liked: bool) -> Option<String> {
        Some(if is_liked {
            labels::UNLIKE.clone()
        } else {
            labels::LIKE.clone()
        })
    }

    fn is_liked(&self) -> bool {
        if let Some(song) = self.current_song() {
            let state = self.app_model.get_state();
            if let Some(home) = state.browser.home_state() {
                return home.saved_tracks.get(&song.rri.id).is_some();
            }
        }
        false
    }

    fn toggle_like(&self) {
        let Some(song) = self.current_song() else {
            return;
        };
        let id = song.rri.id.clone();
        let api = self.app_model.api();
        let is_liked = self.is_liked();

        if is_liked {
            dispatch_api_call(&self.dispatcher, move || async move {
                api.remove_tracks(vec![id.clone()]).await?;
                Ok(BrowserAction::RemoveSavedTracks(vec![id]).into())
            });
        } else {
            let song_desc = song.clone();
            dispatch_api_call(&self.dispatcher, move || async move {
                api.save_tracks(vec![id]).await?;
                Ok(BrowserAction::SaveTracks(vec![song_desc]).into())
            });
        }
    }

    fn get_subtitle_links(&self) -> Vec<ArtistRef> {
        self.current_song()
            .map(|song| song.artists.clone())
            .unwrap_or_default()
    }

    fn navigate_to_subtitle_link(&self, id: &str) {
        self.dispatcher
            .dispatch(AppAction::ViewArtist(id.to_string()));
    }

    fn has_share_button(&self) -> bool {
        true
    }

    fn on_share_clicked(&self) {
        if let Some(song) = self.current_song() {
            self.base
                .share_link(&format!("https://open.spotify.com/track/{}", song.rri.id));
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

impl PinnedPageModel for NowPlayingModel {
    fn pin_kind(&self) -> settings::PinnedKind {
        settings::PinnedKind::Track
    }

    fn pinned_object_id(&self) -> Option<String> {
        self.current_song().map(|song| song.rri.id)
    }

    fn supports_pin_button(&self) -> bool {
        true
    }
}

impl TrackListModel for NowPlayingModel {
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

    fn show_album_column(&self) -> bool {
        true
    }

    fn show_loading_skeleton(&self) -> bool {
        false
    }
    fn deselect_song(&self, id: &str) {
        self.base.deselect_song(id);
    }
    fn selection(&self) -> Option<Box<dyn Deref<Target = SelectionState> + '_>> {
        self.base.selection()
    }

    fn load_more(&self) {
        PageModel::load_more(self);
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

    fn actions_for(&self, song: &Track) -> Option<SimpleActionGroup> {
        let group = SimpleActionGroup::new();
        for a in song.make_artist_actions(self.dispatcher.clone()) {
            group.add_action(&a);
        }
        group.add_action(&song.make_album_action(self.dispatcher.clone()));
        group.add_action(&song.make_link_action());
        group.add_action(&song.make_dequeue_action(self.dispatcher.clone()));
        Some(group)
    }

    fn menu_for(&self, song: &Track, liked: bool, pinned: Option<bool>) -> Option<gio::MenuModel> {
        Some(build_song_menu(
            song,
            true,
            None,
            QueueMenuEntry::Remove,
            Some(liked),
            pinned,
        ))
    }
}

impl SimpleHeaderBarModel for NowPlayingModel {
    fn selection_context(&self) -> Option<SelectionContext> {
        if !feature_flags::is_enabled(FeatureFlag::SelectMode) {
            return None;
        }
        Some(self.current_selection_context())
    }

    fn select_all(&self) {
        let songs: Vec<Track> = self.queue().songs().collect();
        self.dispatcher
            .dispatch(SelectionAction::Select(songs).into());
    }
}

impl crate::app::ProvidesApi for NowPlayingModel {
    fn api_service(&self) -> std::sync::Arc<riff_api::ApiService> {
        self.app_model.api()
    }
}
