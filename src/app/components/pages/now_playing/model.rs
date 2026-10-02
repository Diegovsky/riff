use gettextrs::gettext;
use std::ops::Deref;
use std::rc::Rc;

use crate::app::components::{
    dispatch_api_call, labels, DetailsPageModel, DeviceSelectorModel, HasHeaderBarModel,
    HeaderImageShape, PageModel, PinnedPageModel, QueueListModel, SimpleHeaderBarModel,
    TrackListModel,
};
use crate::app::models::{ArtistRef, ImageSet, Track, TrackExt};
use crate::app::state::{PlaybackAction, PlaybackEvent, SelectionContext};
use crate::app::{AppAction, AppEvent, AppModel, BrowserAction, BrowserEvent, Dispatcher};
use crate::feature_flags::{self, FeatureFlag};
use crate::settings;

const VIEW_ALBUM: &str = "view_album";

/// Data model for the now-playing page. Composes `DetailsPageModel` via Deref.
pub struct NowPlayingModel {
    base: DetailsPageModel,
    queue_list: Rc<QueueListModel>,
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
            queue_list: Rc::new(QueueListModel::new(
                app_model.clone(),
                dispatcher.clone(),
                false,
            )),
            base: DetailsPageModel::new_without_id(app_model, dispatcher),
        }
    }

    pub fn queue_list_model(&self) -> Rc<QueueListModel> {
        self.queue_list.clone()
    }

    fn current_song(&self) -> Option<Track> {
        self.app_model.get_state().playback.header_track()
    }

    pub fn has_queue(&self) -> bool {
        self.queue_list.has_queue()
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
        self.queue_list.load_more();
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

    fn start_play(&self, id: &str) {
        self.queue_list.play_id(id);
    }

    fn toggle_play(&self) {
        self.queue_list.toggle_play();
    }

    // Unlike other pages, this doesn't replace the queue
    fn shuffle_play(&self) {
        self.dispatcher
            .dispatch(PlaybackAction::ToggleShuffle.into());
    }

    fn header_menu_entries(&self) -> Vec<(String, String)> {
        vec![(VIEW_ALBUM.to_string(), labels::VIEW_ALBUM.clone())]
    }

    fn on_header_menu(&self, id: &str) {
        if id != VIEW_ALBUM {
            return;
        }
        let album = self
            .current_song()
            .and_then(|song| song.album)
            .map(|album| album.rri.id)
            .filter(|id| !id.is_empty());
        if let Some(album) = album {
            self.dispatcher.dispatch(AppAction::ViewAlbum(album));
        }
    }

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

    // The header shows the next queued track when nothing plays
    fn should_refresh_details(&self, event: &AppEvent) -> bool {
        matches!(
            event,
            AppEvent::PlaybackEvent(
                PlaybackEvent::TrackChanged(_)
                    | PlaybackEvent::PlaybackStopped
                    | PlaybackEvent::PlaylistChanged
            )
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

impl SimpleHeaderBarModel for NowPlayingModel {
    fn selection_context(&self) -> Option<SelectionContext> {
        if !feature_flags::is_enabled(FeatureFlag::SelectMode) {
            return None;
        }
        Some(self.queue_list.selection_context())
    }

    fn select_all(&self) {
        self.queue_list.select_all();
    }
}

impl crate::app::ProvidesApi for NowPlayingModel {
    fn api_service(&self) -> std::sync::Arc<riff_api::ApiService> {
        self.app_model.api()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::models::make_track;
    use crate::app::state::AppState;
    use crate::app::SongsSource;
    use std::sync::Arc;

    #[test]
    fn test_header_follows_playback() {
        let token_provider: Arc<dyn riff_api::TokenProvider> = Arc::new(|| None::<String>);
        let api = Arc::new(riff_api::spotify_service(
            token_provider,
            1024 * 1024,
            1024 * 1024,
        ));
        let (sender, mut receiver) = futures::channel::mpsc::unbounded();
        let app_model = Rc::new(AppModel::new(AppState::new(), api));
        let model = NowPlayingModel::new(app_model.clone(), Dispatcher::new(sender));
        assert_eq!(
            model.header_menu_entries(),
            [(VIEW_ALBUM.to_string(), labels::VIEW_ALBUM.clone())]
        );

        let mut track = make_track("1");
        track.album = Some(riff_api::models::AlbumRef {
            rri: riff_api::models::ResourceId {
                provider: riff_api::models::Provider::Spotify,
                id: "album1".to_string(),
                uri: None,
            },
            name: "An Album".to_string(),
        });
        app_model.update_state(
            PlaybackAction::LoadContextSongs(
                SongsSource::Album("album1".into()),
                vec![track, make_track("2")],
            )
            .into(),
        );
        app_model.update_state(PlaybackAction::Load("1".to_string()).into());
        assert!(model.source_is_playing() && model.is_playing());

        model.on_header_menu(VIEW_ALBUM);
        let action = receiver.try_next().ok().flatten().expect("an action");
        assert!(matches!(
            action,
            AppAction::BrowserAction(BrowserAction::NavigationPush(
                crate::app::state::ScreenName::AlbumDetails(ref id)
            )) if id == "album1"
        ));

        let events = app_model.update_state(PlaybackAction::Stop.into());
        assert!(events
            .iter()
            .any(|e| matches!(crate::app::components::is_playback_event(e), Some(false))));
        assert!(model.source_is_playing());
        assert!(!model.is_playing());
    }
}
