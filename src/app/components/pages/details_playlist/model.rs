// Model for the playlist detail page.
// Handles playlist metadata, track pagination, like/unlike (follow/unfollow),
// playback, and editable playlist detection. On 400/404 from the API, navigates
// back (the playlist may have been deleted or is inaccessible).

use gettextrs::gettext;
use gio::prelude::*;
use gio::SimpleActionGroup;
use std::ops::Deref;
use std::rc::Rc;

use crate::api::SpotifyApiError;
use crate::app::components::DetailsPageModel;
use crate::app::components::{
    labels, HasHeaderBarModel, HeaderImageShape, PageModel, PlaylistModel, SimpleHeaderBarModel,
};
use crate::app::models::*;
use crate::app::state::SelectionContext;
use crate::app::state::{
    BrowserAction, BrowserEvent, PlaybackAction, SelectionAction, SelectionState,
};
use crate::app::{ActionDispatcher, AppAction, AppEvent, AppModel, PaginationTarget, SongsSource};
use crate::feature_flags::{self, FeatureFlag};
use crate::{impl_playlist_model_base, impl_toggle_play};

/// Data model for the playlist detail page. Composes `DetailsPageModel` via Deref.
pub struct PlaylistDetailsModel {
    base: DetailsPageModel,
}

impl Deref for PlaylistDetailsModel {
    type Target = DetailsPageModel;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl HasHeaderBarModel for PlaylistDetailsModel {}

impl PlaylistDetailsModel {
    pub fn new(id: String, app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
        Self {
            base: DetailsPageModel::new(id, app_model, dispatcher),
        }
    }

    /// Returns true if the logged-in user owns this playlist.
    pub fn is_playlist_editable(&self) -> bool {
        let state = self.app_model.get_state();
        let Some(user) = state.logged_user.user.as_ref() else {
            return false;
        };
        state
            .browser
            .playlist_details_state(&self.id)
            .and_then(|s| s.playlist.as_ref())
            .map(|p| p.owner.id == *user)
            .unwrap_or(false)
    }

    pub fn get_playlist_info(&self) -> Option<impl Deref<Target = PlaylistDescription> + '_> {
        self.app_model.map_state_opt(|s| {
            s.browser
                .playlist_details_state(&self.id)?
                .playlist
                .as_ref()
        })
    }

    /// Rename the playlist via the API and update local state.
    pub fn update_playlist_details(&self, title: String) {
        let api = self.app_model.get_spotify();
        let id = self.id.clone();
        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                api.update_playlist_details(&id, title.clone())
                    .await
                    .map(|_| AppAction::UpdatePlaylistName(PlaylistSummary { id, title }))
            });
    }
}

impl PageModel for PlaylistDetailsModel {
    fn get_title(&self) -> Option<String> {
        Some(self.get_playlist_info()?.title.clone())
    }

    fn get_subtitle(&self) -> Option<String> {
        Some(self.get_playlist_info()?.owner.display_name.clone())
    }

    fn get_artwork(&self) -> Option<ImageSet> {
        self.get_playlist_info()?.art.clone()
    }

    fn get_caption(&self) -> Option<String> {
        Some(gettext("Playlist"))
    }

    fn header_image_shape(&self) -> HeaderImageShape {
        HeaderImageShape::Square
    }

    fn load_page_info(&self) {
        let api = self.app_model.get_spotify();
        let id = self.id.clone();
        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                let (tracks_result, playlist_result) =
                    futures::join!(api.get_playlist_tracks(&id, 0, 100), api.get_playlist(&id));
                let playlist_tracks = tracks_result?;
                match playlist_result {
                    Ok(playlist) => Ok(BrowserAction::SetPlaylistDetails(
                        Box::new(playlist),
                        Box::new(playlist_tracks),
                    )
                    .into()),
                    Err(SpotifyApiError::BadStatus(400, _))
                    | Err(SpotifyApiError::BadStatus(404, _)) => {
                        Ok(BrowserAction::NavigationPop.into())
                    }
                    Err(e) => Err(e),
                }
            });
    }

    fn load_more(&self) {
        let api = self.app_model.get_spotify();
        let state = self.app_model.get_state();
        let Some(next_page) = state
            .browser
            .playlist_details_state(&self.id)
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
            BrowserAction::ConsumeNextPage(PaginationTarget::PlaylistTracks(id.clone())).into(),
        );

        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                api.get_playlist_tracks(&id, offset, batch_size)
                    .await
                    .map(|song_batch| {
                        BrowserAction::AppendPlaylistTracks(id, Box::new(song_batch)).into()
                    })
            });
    }

    fn is_loaded(&self) -> bool {
        self.get_playlist_info().is_some()
    }

    fn has_play_button(&self) -> bool {
        true
    }

    fn source_is_playing(&self) -> bool {
        matches!(self.app_model.get_state().playback.current_source(), Some(SongsSource::Playlist(ref id)) if id == &self.id)
    }

    impl_toggle_play!();

    fn has_like_button(&self) -> bool {
        true
    }

    fn is_liked(&self) -> bool {
        self.app_model
            .get_state()
            .logged_user
            .playlist_ids
            .contains(&self.id)
    }

    fn toggle_like(&self) {
        let id = self.id.clone();
        let is_saved = self.is_liked();
        let api = self.app_model.get_spotify();
        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                if is_saved {
                    api.unfollow_playlist(&id).await?;
                    Ok(BrowserAction::UnsavePlaylist(id).into())
                } else {
                    api.follow_playlist(&id).await?;
                    Ok(BrowserAction::SavePlaylist(id).into())
                }
            });
    }

    fn like_visible(&self) -> bool {
        !self.is_playlist_editable()
    }

    fn has_subtitle_link(&self) -> bool {
        true
    }

    fn on_subtitle_clicked(&self) {
        if let Some(playlist) = self.get_playlist_info() {
            self.dispatcher
                .dispatch(AppAction::ViewUser(playlist.owner.id.clone()));
        }
    }

    fn has_share_button(&self) -> bool {
        true
    }

    fn on_share_clicked(&self) {
        self.share_link(&format!("https://open.spotify.com/playlist/{}", self.id));
    }

    fn should_refresh_details(&self, event: &AppEvent) -> bool {
        matches!(event, AppEvent::BrowserEvent(BrowserEvent::PlaylistDetailsLoaded(id)) if id == &self.id)
    }

    fn should_refresh_liked(&self, event: &AppEvent) -> bool {
        matches!(event,
            AppEvent::BrowserEvent(BrowserEvent::PlaylistSaved(id)) | AppEvent::BrowserEvent(BrowserEvent::PlaylistUnsaved(id))
            if id == &self.id
        )
    }
}

impl PlaylistModel for PlaylistDetailsModel {
    fn song_list_model(&self) -> SongListModel {
        self.state()
            .browser
            .playlist_details_state(&self.id)
            .expect("illegal attempt to read playlist_details_state")
            .songs
            .clone()
    }

    impl_playlist_model_base!();

    fn enable_selection(&self) -> bool {
        if !feature_flags::is_enabled(FeatureFlag::SelectMode) {
            return false;
        }
        let context = if self.is_playlist_editable() {
            SelectionContext::EditablePlaylist(self.id.clone())
        } else {
            SelectionContext::Playlist
        };
        self.enable_selection_with_context(context)
    }

    fn play_song_at(&self, pos: usize, id: &str) {
        let batch = PlaylistModel::song_list_model(self).song_batch_for(pos);
        if let Some(batch) = batch {
            self.dispatcher.dispatch(
                PlaybackAction::LoadPagedSongs(SongsSource::Playlist(self.id.clone()), batch)
                    .into(),
            );
            self.dispatcher
                .dispatch(PlaybackAction::Load(id.to_string()).into());
        }
    }

    fn actions_for(&self, song: &SongDescription) -> Option<gio::ActionGroup> {
        let group = SimpleActionGroup::new();
        for a in song.make_artist_actions(self.dispatcher.box_clone(), None) {
            group.add_action(&a);
        }
        group.add_action(&song.make_album_action(self.dispatcher.box_clone(), None));
        group.add_action(&song.make_link_action(None));
        group.add_action(&song.make_queue_action(self.dispatcher.box_clone(), None));
        Some(group.upcast())
    }

    fn menu_for(&self, song: &SongDescription) -> Option<gio::MenuModel> {
        let menu = gio::Menu::new();
        menu.append(Some(&*labels::VIEW_ALBUM), Some("song.view_album"));
        for artist in song.artists.iter() {
            menu.append(
                Some(&labels::more_from_label(&artist.name)),
                Some(&format!("song.view_artist_{}", artist.id)),
            );
        }
        menu.append(Some(&*labels::COPY_LINK), Some("song.copy_link"));
        menu.append(Some(&*labels::ADD_TO_QUEUE), Some("song.queue"));
        Some(menu.upcast())
    }
}

impl SimpleHeaderBarModel for PlaylistDetailsModel {
    fn title(&self) -> Option<String> {
        PageModel::get_title(self)
    }
    fn title_updated(&self, event: &AppEvent) -> bool {
        PageModel::should_refresh_details(self, event)
    }

    fn selection_context(&self) -> Option<SelectionContext> {
        if !feature_flags::is_enabled(FeatureFlag::SelectMode) {
            return None;
        }
        if self.is_playlist_editable() {
            Some(SelectionContext::EditablePlaylist(self.id.clone()))
        } else {
            Some(SelectionContext::Playlist)
        }
    }

    fn select_all(&self) {
        let songs: Vec<SongDescription> = PlaylistModel::song_list_model(self).collect();
        self.dispatcher
            .dispatch(SelectionAction::Select(songs).into());
    }
}
