// Model for the playlist detail page.
// Handles playlist metadata, track pagination, like/unlike (follow/unfollow),
// playback, and editable playlist detection. On 400/404 from the API, navigates
// back (the playlist may have been deleted or is inaccessible).

use gettextrs::gettext;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::app::components::DetailsPageModel;
use crate::app::components::{
    dispatch_api_call, dispatch_api_call_many, dispatch_api_read, dispatch_api_read_with_fallback,
    labels, refresh_playlist_cover, upload_playlist_cover, HasHeaderBarModel, HeaderImageShape,
    PageModel, PinnedPageModel, PlaylistForm, Saved, TrackListModel,
};
use crate::app::models::*;
use crate::app::playlist_actions::unfollow_and_unpin_playlist;
use crate::app::state::Route;
use crate::app::state::SelectionContext;
use crate::app::state::{BrowserAction, BrowserEvent, SelectionState};
use crate::app::{AppAction, AppEvent, AppModel, Dispatcher, PaginationTarget, SongsSource};
use crate::settings;
use crate::{impl_source_page, impl_source_track_list, impl_track_list_model_base};
use riff_api::models::is_resource_url;
use riff_api::DomainError;
use riff_config::feature_flags::{is_enabled, FeatureFlag};

const TRACKS_PAGE: usize = 50;

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
        self.owns_playlist(&self.id)
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
            api.update_playlist_details(&id, &title, None, None)
                .await
                .map(|_| AppAction::UpdatePlaylistName(PlaylistSummary { id, title }))
        });
    }

    pub fn edit_playlist(&self, form: PlaylistForm) -> oneshot::Receiver<()> {
        let (saved, saved_rx) = Saved::channel();
        let api = self.app_model.api();
        let id = self.id.clone();
        let dispatcher = self.dispatcher.clone();
        let public = form.public_changed.then_some(form.public);
        let kept = self.get_playlist_info().and_then(|p| p.description.clone());
        let not_cleared = form.description.is_none() && kept.is_some();
        let description = form.description.clone().or(kept);
        dispatch_api_call_many(&self.dispatcher, move || async move {
            api.update_playlist_details(&id, &form.name, form.description.as_deref(), public)
                .await?;
            saved.tell();
            if let Some(cover) = form.cover {
                upload_playlist_cover(&dispatcher, api, id.clone(), cover);
            }
            let mut actions = vec![
                AppAction::UpdatePlaylistName(PlaylistSummary {
                    id: id.clone(),
                    title: form.name,
                }),
                BrowserAction::UpdatePlaylistInfo(id, description, public).into(),
            ];
            if not_cleared {
                actions.push(AppAction::ShowNotification(gettext(
                    // translators: This notification is shown when the user emptied the description of a playlist, which cannot be removed once set.
                    "A playlist's description can't be removed, so it was kept.",
                )));
            }
            Ok(actions)
        });
        saved_rx
    }

    pub fn delete_playlist(&self) {
        let api = self.app_model.api();
        let id = self.id.clone();
        let user_id = self.app_model.get_state().logged_user.user.clone();
        dispatch_api_call_many(&self.dispatcher, move || async move {
            let leave = BrowserAction::NavigationLeave(Route::Playlist(id.clone())).into();
            let removed = unfollow_and_unpin_playlist(&api, id, user_id).await?;
            Ok(vec![removed, leave])
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

    fn get_subtitle_detail(&self) -> Option<String> {
        let count = self.get_playlist_info()?.total_tracks?;
        let text = gettextrs::ngettext("{} Track", "{} Tracks", count);
        Some(labels::fill(text, &[&count.to_string()]))
    }

    fn get_artwork(&self) -> Option<ImageSet> {
        Some(self.get_playlist_info()?.art.clone())
    }

    fn get_caption(&self) -> Option<String> {
        let public = self.get_playlist_info().and_then(|p| p.public);
        Some(match public {
            Some(true) => labels::PUBLIC_PLAYLIST_CAPTION.clone(),
            Some(false) => labels::PRIVATE_PLAYLIST_CAPTION.clone(),
            None => gettext("Playlist"),
        })
    }

    fn header_image_shape(&self) -> HeaderImageShape {
        HeaderImageShape::Square
    }

    fn songs_source(&self) -> Option<SongsSource> {
        Some(SongsSource::Playlist(self.id.clone()))
    }

    fn load_page_info(&self) {
        let api = self.app_model.api();
        let id = self.id.clone();
        let dispatcher = self.dispatcher.clone();
        dispatch_api_read(&self.dispatcher, move |tag| async move {
            let (tracks_result, playlist_result) = futures::join!(
                api.get_playlist_tracks(&id, 0, TRACKS_PAGE, tag),
                api.get_playlist(&id, tag)
            );
            let playlist_tracks = tracks_result?;
            let cover = playlist_result
                .as_ref()
                .ok()
                .and_then(|playlist| playlist.art.images.first());
            if cover.is_none_or(|image| is_resource_url(&image.url)) {
                refresh_playlist_cover(&dispatcher, Arc::clone(&api), id.clone(), false);
            }
            match playlist_result {
                Ok(playlist) => Ok(BrowserAction::SetPlaylistDetails(
                    Box::new(playlist),
                    Box::new(playlist_tracks),
                )
                .into()),
                Err(DomainError::ClientError { status: 400, .. })
                | Err(DomainError::NotFound { .. }) => {
                    Ok(BrowserAction::NavigationLeave(Route::Playlist(id)).into())
                }
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

    impl_source_page!();

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
        let pin_enabled = is_enabled(FeatureFlag::PinnedObjects);
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
                        settings::unpin_object(&user_id, settings::PinnedKind::Playlist, &id);
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

    fn should_refresh_details(&self, event: &AppEvent) -> bool {
        matches!(event,
            AppEvent::BrowserEvent(BrowserEvent::PlaylistDetailsLoaded(id))
            | AppEvent::BrowserEvent(BrowserEvent::PlaylistTracksRemoved(id))
            if id == &self.id)
    }

    fn should_refresh_liked(&self, event: &AppEvent) -> bool {
        matches!(event,
            AppEvent::BrowserEvent(BrowserEvent::PlaylistSaved(id)) | AppEvent::BrowserEvent(BrowserEvent::PlaylistUnsaved(id))
            if id == &self.id
        )
    }
}

impl PinnedPageModel for PlaylistDetailsModel {
    fn supports_pin_button(&self) -> bool {
        true
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

    fn show_album_column(&self) -> bool {
        true
    }

    impl_track_list_model_base!();
    impl_source_track_list!();

    fn enable_selection(&self) -> bool {
        let context = if self.is_playlist_editable() {
            SelectionContext::EditablePlaylist(self.id.clone())
        } else {
            SelectionContext::Playlist
        };
        self.enable_selection_with_context(context)
    }
}

crate::impl_track_list_header_bar!(PlaylistDetailsModel);

impl crate::app::ProvidesApi for PlaylistDetailsModel {
    fn api_service(&self) -> std::sync::Arc<riff_api::ApiService> {
        self.app_model.api()
    }
}
