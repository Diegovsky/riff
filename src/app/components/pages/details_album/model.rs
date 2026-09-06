// Model for the album detail page.
// Handles album metadata, track pagination, save/unsave (like), playback,
// and artist navigation. On 400/404 from the API, navigates back.

use gio::prelude::*;
use gio::SimpleActionGroup;
use std::ops::Deref;
use std::rc::Rc;

use crate::app::components::DetailsPageModel;
use crate::app::components::SongActions;
use crate::app::components::{
    labels, HasHeaderBarModel, HeaderImageShape, PageModel, PlaylistModel, SimpleHeaderBarModel,
};
use crate::app::models::*;
use crate::app::state::SelectionContext;
use crate::app::state::CARD_BATCH_SIZE;
use crate::app::state::{
    BrowserAction, BrowserEvent, PlaybackAction, SelectionAction, SelectionState,
};
use crate::app::{ActionDispatcher, AppAction, AppEvent, AppModel, PaginationTarget, SongsSource};
use crate::feature_flags::{self, FeatureFlag};
use crate::{impl_playlist_model_base, impl_toggle_play};
use riff_api::DomainError;

/// Data model for the album detail page. Composes `DetailsPageModel` via Deref.
pub struct DetailsModel {
    base: DetailsPageModel,
}

impl Deref for DetailsModel {
    type Target = DetailsPageModel;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl HasHeaderBarModel for DetailsModel {}

impl DetailsModel {
    pub fn new(id: String, app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
        Self {
            base: DetailsPageModel::new(id, app_model, dispatcher),
        }
    }

    /// Returns the album metadata from browser state, if loaded.
    pub fn get_album_info(&self) -> Option<impl Deref<Target = Album> + '_> {
        self.app_model
            .map_state_opt(|s| s.browser.details_state(&self.id)?.album.as_ref())
    }
}

impl PageModel for DetailsModel {
    fn get_title(&self) -> Option<String> {
        Some(self.get_album_info()?.title.clone())
    }

    fn get_subtitle(&self) -> Option<String> {
        Some(self.get_album_info()?.artists_name())
    }

    fn get_artwork(&self) -> Option<ImageSet> {
        Some(self.get_album_info()?.art.clone())
    }

    fn get_caption(&self) -> Option<String> {
        Some("Album".to_string())
    }

    fn header_image_shape(&self) -> HeaderImageShape {
        HeaderImageShape::Square
    }

    fn load_page_info(&self) {
        let api = self.app_model.api();

        // Album metadata: drives the header and the release-details dialog.
        // A 400/404 means the album does not exist, so navigate back.
        let id = self.id.clone();
        let info_api = api.clone();
        self.dispatcher.call_api_and_dispatch(move || async move {
            match info_api.get_album(&id).await {
                Ok(album) => Ok(BrowserAction::SetAlbumInfo(Box::new(album)).into()),
                Err(DomainError::ClientError { status: 400, .. })
                | Err(DomainError::NotFound { .. }) => Ok(BrowserAction::NavigationPop.into()),
                Err(e) => Err(e),
            }
        });

        // Initial track page: populates the song list independently of the
        // album metadata.
        let id = self.id.clone();
        let tracks_api = api.clone();
        self.dispatcher.call_api_and_dispatch(move || async move {
            tracks_api
                .get_album_tracks(&id, 0, CARD_BATCH_SIZE)
                .await
                .map(|batch| BrowserAction::SetAlbumTracks(id, Box::new(batch)).into())
        });

        // Liked status: a failure here must not blank the page, so it is
        // swallowed to "not liked" rather than surfaced as an error.
        let id = self.id.clone();
        self.dispatcher.call_api_and_dispatch(move || async move {
            let is_liked = api.check_saved_album(&id).await.unwrap_or(false);
            Ok(BrowserAction::SetAlbumLikedStatus(id, is_liked).into())
        });
    }

    fn load_more(&self) {
        let api = self.app_model.api();
        let state = self.app_model.get_state();
        let Some(next_page) = state
            .browser
            .details_state(&self.id)
            .map(|s| s.next_tracks_page.clone())
        else {
            return;
        };
        drop(state);

        let Some(offset) = next_page.next_offset else {
            return;
        };
        let id = self.id.clone();
        let batch_size = next_page.batch_size;

        self.app_model.update_state(
            BrowserAction::ConsumeNextPage(PaginationTarget::AlbumTracks(id.clone())).into(),
        );

        self.dispatcher.call_api_and_dispatch(move || async move {
            api.get_album_tracks(&id, offset, batch_size)
                .await
                .map(|song_batch| BrowserAction::AppendAlbumTracks(id, Box::new(song_batch)).into())
        });
    }

    fn is_loaded(&self) -> bool {
        self.get_album_info().is_some()
    }

    fn has_play_button(&self) -> bool {
        true
    }

    fn source_is_playing(&self) -> bool {
        matches!(self.app_model.get_state().playback.current_source(), Some(SongsSource::Album(ref id)) if id == &self.id)
    }

    impl_toggle_play!();

    fn has_like_button(&self) -> bool {
        true
    }

    fn is_liked(&self) -> bool {
        self.app_model
            .get_state()
            .browser
            .details_state(&self.id)
            .map(|s| s.is_liked)
            .unwrap_or(false)
    }

    fn toggle_like(&self) {
        let album = {
            let Some(album) = self.get_album_info() else {
                return;
            };
            (*album).clone()
        };
        let id = album.rri.id.clone();
        let is_liked = self.is_liked();
        let api = self.app_model.api();
        self.dispatcher.call_api_and_dispatch(move || async move {
            if !is_liked {
                api.save_albums(&id)
                    .await
                    .map(|_| BrowserAction::SaveAlbum(Box::new(album)).into())
            } else {
                api.remove_albums(&id)
                    .await
                    .map(|_| BrowserAction::UnsaveAlbum(id).into())
            }
        });
    }

    fn has_info_button(&self) -> bool {
        true
    }

    fn has_share_button(&self) -> bool {
        true
    }

    fn on_share_clicked(&self) {
        self.share_link(&format!("https://open.spotify.com/album/{}", self.id));
    }

    fn get_subtitle_links(&self) -> Vec<ArtistRef> {
        self.get_album_info()
            .map(|album| album.artists.clone())
            .unwrap_or_default()
    }

    fn navigate_to_subtitle_link(&self, id: &str) {
        self.dispatcher
            .dispatch(AppAction::ViewArtist(id.to_string()));
    }

    fn should_refresh_details(&self, event: &AppEvent) -> bool {
        matches!(event, AppEvent::BrowserEvent(BrowserEvent::AlbumDetailsLoaded(id)) if id == &self.id)
    }

    fn should_refresh_liked(&self, event: &AppEvent) -> bool {
        matches!(event,
            AppEvent::BrowserEvent(BrowserEvent::AlbumSaved(id)) | AppEvent::BrowserEvent(BrowserEvent::AlbumUnsaved(id))
            if id == &self.id
        )
    }
}

impl PlaylistModel for DetailsModel {
    fn song_list_model(&self) -> SongListModel {
        self.app_model
            .get_state()
            .browser
            .details_state(&self.id)
            .expect("illegal attempt to read details_state")
            .songs
            .clone()
    }

    fn show_song_covers(&self) -> bool {
        false
    }

    impl_playlist_model_base!();

    fn enable_selection(&self) -> bool {
        self.enable_selection_with_context(SelectionContext::Default)
    }

    fn play_song_at(&self, pos: usize, id: &str) {
        let batch = PlaylistModel::song_list_model(self).song_batch_for(pos);
        if let Some(batch) = batch {
            self.dispatcher.dispatch(
                PlaybackAction::LoadPagedSongs(SongsSource::Album(self.id.clone()), batch).into(),
            );
            self.dispatcher
                .dispatch(PlaybackAction::Load(id.to_string()).into());
        }
    }

    fn actions_for(&self, song: &Track) -> Option<gio::ActionGroup> {
        let group = SimpleActionGroup::new();
        for a in song.make_artist_actions(self.dispatcher.box_clone(), None) {
            group.add_action(&a);
        }

        group.add_action(&song.make_link_action(None));
        group.add_action(&song.make_queue_action(self.dispatcher.box_clone(), None));
        Some(group.upcast())
    }

    fn menu_for(&self, song: &Track) -> Option<gio::MenuModel> {
        let menu = gio::Menu::new();
        for artist in song.artists.iter() {
            menu.append(
                Some(&labels::more_from_label(&artist.name)),
                Some(&format!("song.view_artist_{}", artist.rri.id)),
            );
        }
        menu.append(Some(&*labels::COPY_LINK), Some("song.copy_link"));
        menu.append(Some(&*labels::ADD_TO_QUEUE), Some("song.queue"));
        Some(menu.upcast())
    }
}

impl SimpleHeaderBarModel for DetailsModel {
    fn selection_context(&self) -> Option<SelectionContext> {
        if !feature_flags::is_enabled(FeatureFlag::SelectMode) {
            return None;
        }
        Some(SelectionContext::Default)
    }

    fn select_all(&self) {
        let songs: Vec<Track> = PlaylistModel::song_list_model(self).collect();
        self.dispatcher
            .dispatch(SelectionAction::Select(songs).into());
    }
}

impl crate::app::ProvidesApi for DetailsModel {
    fn api_service(&self) -> std::sync::Arc<riff_api::ApiService> {
        self.app_model.api()
    }
}
