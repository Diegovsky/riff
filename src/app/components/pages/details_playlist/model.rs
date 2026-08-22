// Model for the playlist detail page.
// Handles playlist metadata, track pagination, like/unlike (follow/unfollow),
// playback, and editable playlist detection. On 400/404 from the API, navigates
// back (the playlist may have been deleted or is inaccessible).

use gettextrs::gettext;
use gio::prelude::*;
use gio::SimpleActionGroup;
use std::ops::Deref;
use std::rc::Rc;

use crate::app::components::DetailsPageModel;
use crate::app::components::SongActions;
use crate::app::components::{
    build_song_menu, dispatch_api_call, dispatch_api_read, dispatch_api_read_with_fallback, labels,
    HasHeaderBarModel, HeaderImageShape, PageModel, QueueMenuEntry, SimpleHeaderBarModel,
    TrackListModel,
};
use crate::app::models::*;
use crate::app::state::SelectionContext;
use crate::app::state::{
    BrowserAction, BrowserEvent, PlaybackAction, SelectionAction, SelectionState,
};
use crate::app::{AppAction, AppEvent, AppModel, Dispatcher, PaginationTarget, SongsSource};
use crate::feature_flags::{is_enabled, FeatureFlag};
use crate::settings;
use crate::{impl_toggle_play, impl_track_list_model_base};
use riff_api::DomainError;

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
    pub fn new(id: String, app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
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
            .map(|p| p.owner.as_ref().map(|o| o.rri.id == *user).unwrap_or(false))
            .unwrap_or(false)
    }

    pub fn get_playlist_info(&self) -> Option<impl Deref<Target = Playlist> + '_> {
        self.app_model.map_state_opt(|s| {
            s.browser
                .playlist_details_state(&self.id)?
                .playlist
                .as_ref()
        })
    }

    /// Rename the playlist via the API and update local state.
    pub fn update_playlist_details(&self, title: String) {
        let api = self.app_model.api();
        let id = self.id.clone();
        dispatch_api_call(&self.dispatcher, move || async move {
            api.update_playlist_details(&id, &title)
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
        Some(
            self.get_playlist_info()?
                .owner
                .as_ref()
                .map(|o| o.display_name.clone())
                .unwrap_or_default(),
        )
    }

    fn get_artwork(&self) -> Option<ImageSet> {
        Some(self.get_playlist_info()?.art.clone())
    }

    fn get_caption(&self) -> Option<String> {
        Some(gettext("Playlist"))
    }

    fn header_image_shape(&self) -> HeaderImageShape {
        HeaderImageShape::Square
    }

    fn load_page_info(&self) {
        let api = self.app_model.api();
        let id = self.id.clone();
        dispatch_api_read(&self.dispatcher, move |tag| async move {
            let (tracks_result, playlist_result) = futures::join!(
                api.get_playlist_tracks(&id, 0, 50, tag),
                api.get_playlist(&id, tag)
            );
            let playlist_tracks = tracks_result?;
            match playlist_result {
                Ok(playlist) => Ok(BrowserAction::SetPlaylistDetails(
                    Box::new(playlist),
                    Box::new(playlist_tracks),
                )
                .into()),
                Err(DomainError::ClientError { status: 400, .. })
                | Err(DomainError::NotFound { .. }) => Ok(BrowserAction::NavigationPop.into()),
                Err(e) => Err(e),
            }
        });
    }

    fn load_more(&self) {
        let api = self.app_model.api();
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

        let restore_target = PaginationTarget::PlaylistTracks(id.clone());
        dispatch_api_read_with_fallback(
            &self.dispatcher,
            move |tag| {
                let id = id.clone();
                async move {
                    api.get_playlist_tracks(&id, offset, batch_size, tag)
                        .await
                        .map(|song_batch| {
                            BrowserAction::AppendPlaylistTracks(id, Box::new(song_batch)).into()
                        })
                }
            },
            move || BrowserAction::PageRequestFailed(restore_target.clone(), offset).into(),
        );
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

    fn like_tooltip(&self, is_liked: bool) -> Option<String> {
        Some(if is_liked {
            labels::UNLIKE_PLAYLIST.clone()
        } else {
            labels::LIKE_PLAYLIST.clone()
        })
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
        let api = self.app_model.api();
        let pin_enabled = is_enabled(FeatureFlag::PinnedPlaylists);
        let user_id = self.app_model.get_state().logged_user.user.clone();

        let description = {
            let state = self.app_model.get_state();
            state
                .browser
                .playlist_details_state(&id)
                .and_then(|s| s.playlist.clone())
        };

        dispatch_api_call(&self.dispatcher, move || async move {
            if is_saved {
                api.unfollow_playlist(&id).await?;
                if pin_enabled {
                    if let Some(user_id) = user_id {
                        settings::unpin_playlist(&user_id, &id);
                    }
                }
                Ok(BrowserAction::UnsavePlaylist(id).into())
            } else {
                api.follow_playlist(&id).await?;
                let description = description.unwrap_or_else(|| Playlist {
                    rri: ResourceId {
                        id: id.clone(),
                        ..Default::default()
                    },
                    ..Default::default()
                });
                Ok(BrowserAction::SavePlaylist(Box::new(description)).into())
            }
        });
    }

    fn like_visible(&self) -> bool {
        !self.is_playlist_editable()
    }

    fn get_subtitle_links(&self) -> Vec<ArtistRef> {
        self.get_playlist_info()
            .and_then(|playlist| {
                playlist.owner.as_ref().map(|o| {
                    vec![ArtistRef {
                        rri: o.rri.clone(),
                        name: o.display_name.clone(),
                    }]
                })
            })
            .unwrap_or_default()
    }

    fn navigate_to_subtitle_link(&self, id: &str) {
        self.dispatcher
            .dispatch(AppAction::ViewUser(id.to_string()));
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

    fn supports_pin_button(&self) -> bool {
        true
    }

    fn is_pinned(&self) -> bool {
        self.app_model
            .get_state()
            .logged_user
            .user
            .as_ref()
            .is_some_and(|user_id| settings::is_playlist_pinned(user_id, &self.id))
    }

    fn toggle_pin(&self) {
        let Some(user_id) = self.app_model.get_state().logged_user.user.clone() else {
            return;
        };
        let changed = if self.is_pinned() {
            settings::unpin_playlist(&user_id, &self.id)
        } else {
            settings::pin_playlist(&user_id, &self.id)
        };
        if changed {
            self.dispatcher
                .dispatch(BrowserAction::NotifyPinnedPlaylistsUpdated.into());
        }
    }
}

impl TrackListModel for PlaylistDetailsModel {
    fn song_list_model(&self) -> SongListModel {
        self.state()
            .browser
            .playlist_details_state(&self.id)
            .expect("illegal attempt to read playlist_details_state")
            .songs
            .clone()
    }

    fn load_more(&self) {
        PageModel::load_more(self);
    }

    fn show_album_column(&self) -> bool {
        true
    }

    impl_track_list_model_base!();

    fn enable_selection(&self) -> bool {
        if !is_enabled(FeatureFlag::SelectMode) {
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
        let batch = TrackListModel::song_list_model(self).song_batch_for(pos);
        if let Some(batch) = batch {
            self.dispatcher.dispatch(
                PlaybackAction::LoadPagedSongs(SongsSource::Playlist(self.id.clone()), batch)
                    .into(),
            );
            self.dispatcher
                .dispatch(PlaybackAction::Load(id.to_string()).into());
        }
    }

    fn actions_for(&self, song: &Track) -> Option<SimpleActionGroup> {
        let group = SimpleActionGroup::new();
        for a in song.make_artist_actions(self.dispatcher.clone()) {
            group.add_action(&a);
        }
        group.add_action(&song.make_album_action(self.dispatcher.clone()));
        group.add_action(&song.make_link_action());
        group.add_action(&song.make_queue_action(self.dispatcher.clone()));
        Some(group)
    }

    fn menu_for(&self, song: &Track, liked: bool) -> Option<gio::MenuModel> {
        Some(build_song_menu(
            song,
            true,
            None,
            QueueMenuEntry::Add,
            Some(liked),
        ))
    }
}

impl SimpleHeaderBarModel for PlaylistDetailsModel {
    fn selection_context(&self) -> Option<SelectionContext> {
        if !is_enabled(FeatureFlag::SelectMode) {
            return None;
        }
        if self.is_playlist_editable() {
            Some(SelectionContext::EditablePlaylist(self.id.clone()))
        } else {
            Some(SelectionContext::Playlist)
        }
    }

    fn select_all(&self) {
        let songs: Vec<Track> = TrackListModel::song_list_model(self).collect();
        self.dispatcher
            .dispatch(SelectionAction::Select(songs).into());
    }
}

impl crate::app::ProvidesApi for PlaylistDetailsModel {
    fn api_service(&self) -> std::sync::Arc<riff_api::ApiService> {
        self.app_model.api()
    }
}
