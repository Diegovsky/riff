// Model for the artist detail page.
// Provides artist info, top tracks (as a playlist), album releases (as a card
// list), follow/unfollow, and playback. On 400/404 from the API, navigates
// back (the artist may not exist or be inaccessible).

use gio::prelude::*;
use gio::SimpleActionGroup;
use std::ops::Deref;
use std::rc::Rc;

use crate::api::SpotifyApiError;
use crate::app::components::DetailsPageModel;
use crate::app::components::SimpleHeaderBarModel;
use crate::app::components::{
    labels, CardListModel, HasHeaderBarModel, HeaderImageShape, ImageShape, PageModel,
    PlaylistModel,
};
use crate::app::models::*;
use crate::app::state::SelectionContext;
use crate::app::state::{
    BrowserAction, BrowserEvent, PaginationTarget, PlaybackAction, SelectionState,
};
use crate::app::{ActionDispatcher, AppAction, AppEvent, AppModel, ListStore, SongsSource};
use crate::{impl_playlist_model_base, impl_toggle_play};

/// Data model for the artist detail page. Composes `DetailsPageModel` via Deref.
pub struct ArtistDetailsModel {
    base: DetailsPageModel,
}

impl Deref for ArtistDetailsModel {
    type Target = DetailsPageModel;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl HasHeaderBarModel for ArtistDetailsModel {}

impl ArtistDetailsModel {
    pub fn new(id: String, app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
        Self {
            base: DetailsPageModel::new(id, app_model, dispatcher),
        }
    }
}

impl PageModel for ArtistDetailsModel {
    fn get_title(&self) -> Option<String> {
        self.app_model
            .map_state_opt(|s| s.browser.artist_state(&self.id)?.artist.as_ref())
            .map(|n| n.clone())
    }

    fn get_artwork(&self) -> Option<ImageSet> {
        self.app_model
            .map_state_opt(|s| s.browser.artist_state(&self.id)?.photo.as_ref())
            .map(|p| p.clone())
    }

    fn get_caption(&self) -> Option<String> {
        Some("Artist".to_string())
    }

    fn header_image_shape(&self) -> HeaderImageShape {
        HeaderImageShape::Circle
    }

    fn load_page_info(&self) {
        let api = self.app_model.get_spotify();
        let id = self.id.clone();
        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                match api.get_artist(&id).await {
                    Ok(artist) => Ok(BrowserAction::SetArtistDetails(Box::new(artist)).into()),
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
            .artist_state(&self.id)
            .map(|s| s.next_page.clone())
        else {
            return;
        };
        drop(state);

        let Some(offset) = next_page.next_offset else {
            return;
        };
        let id = next_page.data;
        let batch_size = next_page.batch_size;

        self.app_model.update_state(
            BrowserAction::ConsumeNextPage(PaginationTarget::ArtistReleases(id.clone())).into(),
        );

        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                api.get_artist_albums(&id, offset, batch_size)
                    .await
                    .map(|albums| BrowserAction::AppendArtistReleases(id, albums).into())
            });
    }

    fn is_loaded(&self) -> bool {
        self.app_model
            .map_state_opt(|s| s.browser.artist_state(&self.id)?.artist.as_ref())
            .is_some()
    }

    fn has_play_button(&self) -> bool {
        true
    }

    fn source_is_playing(&self) -> bool {
        matches!(self.app_model.get_state().playback.current_source(), Some(SongsSource::Artist(ref id)) if id == &self.id)
    }

    impl_toggle_play!();

    fn has_like_button(&self) -> bool {
        true
    }

    fn is_liked(&self) -> bool {
        self.app_model
            .get_state()
            .browser
            .artist_state(&self.id)
            .map(|s| s.is_followed)
            .unwrap_or(false)
    }

    fn toggle_like(&self) {
        let id = self.id.clone();
        let is_followed = self.is_liked();
        let api = self.app_model.get_spotify();
        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                if is_followed {
                    api.unfollow_artist(&id).await?;
                    Ok(BrowserAction::UnfollowArtist(id).into())
                } else {
                    api.follow_artist(&id).await?;
                    Ok(BrowserAction::FollowArtist(id).into())
                }
            });
    }

    fn should_refresh_details(&self, event: &AppEvent) -> bool {
        matches!(event, AppEvent::BrowserEvent(BrowserEvent::ArtistDetailsUpdated(id)) if id == &self.id)
    }
}

impl CardListModel for ArtistDetailsModel {
    fn get_store(&self) -> Option<impl Deref<Target = ListStore<CardModel>> + '_> {
        self.app_model
            .map_state_opt(|s| Some(&s.browser.artist_state(&self.id)?.albums))
    }

    fn load_more(&self) {
        PageModel::load_more(self);
    }
    fn refresh(&self) {}

    fn has_items(&self) -> bool {
        self.app_model
            .map_state_opt(|s| Some(&s.browser.artist_state(&self.id)?.albums))
            .map(|s| s.len() > 0)
            .unwrap_or(false)
    }

    fn open_item(&self, id: String) {
        self.dispatcher.dispatch(AppAction::ViewAlbum(id));
    }

    fn image_shape(&self) -> ImageShape {
        ImageShape::Square
    }
}

impl PlaylistModel for ArtistDetailsModel {
    fn song_list_model(&self) -> SongListModel {
        self.app_model
            .get_state()
            .browser
            .artist_state(&self.id)
            .expect("illegal attempt to read artist_state")
            .top_tracks
            .clone()
    }

    impl_playlist_model_base!();

    fn enable_selection(&self) -> bool {
        self.enable_selection_with_context(SelectionContext::Default)
    }

    fn play_song_at(&self, _pos: usize, id: &str) {
        let tracks: Vec<SongDescription> = PlaylistModel::song_list_model(self).collect();
        let total = tracks.len();
        let batch = SongBatch {
            songs: tracks,
            batch: Batch {
                offset: 0,
                batch_size: total,
                total,
            },
        };
        self.dispatcher.dispatch(
            PlaybackAction::LoadPagedSongs(SongsSource::Artist(self.id.clone()), batch).into(),
        );
        self.dispatcher
            .dispatch(PlaybackAction::Load(id.to_string()).into());
    }

    fn actions_for(&self, id: &str) -> Option<gio::ActionGroup> {
        let song = PlaylistModel::song_list_model(self).get(id)?;
        let song = song.description();
        let group = SimpleActionGroup::new();
        for a in song.make_artist_actions(self.dispatcher.box_clone(), None) {
            group.add_action(&a);
        }
        group.add_action(&song.make_album_action(self.dispatcher.box_clone(), None));
        group.add_action(&song.make_link_action(None));
        group.add_action(&song.make_queue_action(self.dispatcher.box_clone(), None));
        Some(group.upcast())
    }

    fn menu_for(&self, id: &str) -> Option<gio::MenuModel> {
        let song = PlaylistModel::song_list_model(self).get(id)?;
        let song = song.description();
        let menu = gio::Menu::new();
        menu.append(Some(&*labels::VIEW_ALBUM), Some("song.view_album"));
        for artist in song.artists.iter().filter(|a| self.id != a.id) {
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

impl SimpleHeaderBarModel for ArtistDetailsModel {
    fn title(&self) -> Option<String> {
        PageModel::get_title(self)
    }
    fn title_updated(&self, event: &AppEvent) -> bool {
        PageModel::should_refresh_details(self, event)
    }
    fn selection_context(&self) -> Option<SelectionContext> {
        None
    }
    fn select_all(&self) {}
}
