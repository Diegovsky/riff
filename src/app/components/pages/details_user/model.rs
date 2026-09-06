// Model for the user profile detail page.
// Implements PageModel, CardListModel, and SimpleHeaderBarModel to display
// user info and their public playlists with pagination.

use gettextrs::gettext;
use std::ops::Deref;
use std::rc::Rc;

use crate::app::components::DetailsPageModel;
use crate::app::components::{
    CardListModel, HasHeaderBarModel, HeaderImageShape, ImageShape, PageModel, SimpleHeaderBarModel,
};
use crate::app::models::*;
use crate::app::state::{BrowserAction, BrowserEvent, SelectionContext, CARD_BATCH_SIZE};
use crate::app::{ActionDispatcher, AppAction, AppEvent, AppModel, ListStore, PaginationTarget};

/// Data model for the user profile page. Composes `DetailsPageModel` via Deref.
pub struct UserDetailsModel {
    base: DetailsPageModel,
}

impl Deref for UserDetailsModel {
    type Target = DetailsPageModel;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl HasHeaderBarModel for UserDetailsModel {}

impl UserDetailsModel {
    pub fn new(id: String, app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
        Self {
            base: DetailsPageModel::new(id, app_model, dispatcher),
        }
    }
}

impl PageModel for UserDetailsModel {
    fn get_title(&self) -> Option<String> {
        self.app_model
            .map_state_opt(|s| s.browser.user_state(&self.id)?.user.as_ref())
            .map(|name| name.clone())
    }

    fn get_caption(&self) -> Option<String> {
        Some(gettext("Profile"))
    }

    fn get_artwork(&self) -> Option<ImageSet> {
        self.app_model
            .map_state_opt(|s| s.browser.user_state(&self.id)?.photo.as_ref())
            .map(|photo| photo.clone())
    }

    fn header_image_shape(&self) -> HeaderImageShape {
        HeaderImageShape::Circle
    }

    fn load_page_info(&self) {
        let api = self.app_model.api();

        // User info: name and photo for the header.
        let id = self.id.clone();
        let info_api = api.clone();
        self.dispatcher.call_api_and_dispatch(move || async move {
            info_api
                .get_user(&id)
                .await
                .map(|user| BrowserAction::SetUserInfo(Box::new(user)).into())
        });

        // Initial page of the user's public playlists (the card list).
        let id = self.id.clone();
        self.dispatcher.call_api_and_dispatch(move || async move {
            api.get_user_playlists(&id, 0, CARD_BATCH_SIZE)
                .await
                .map(|page| BrowserAction::SetUserPlaylists(id, page.items).into())
        });
    }

    fn load_more(&self) {
        let api = self.app_model.api();

        let state = self.app_model.get_state();
        let Some(next_page) = state
            .browser
            .user_state(&self.id)
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
            BrowserAction::ConsumeNextPage(PaginationTarget::UserPlaylists(id.clone())).into(),
        );

        self.dispatcher.call_api_and_dispatch(move || async move {
            api.get_user_playlists(&id, offset, batch_size)
                .await
                .map(|page| BrowserAction::AppendUserPlaylists(id, page.items).into())
        });
    }

    fn is_loaded(&self) -> bool {
        self.app_model
            .map_state_opt(|s| s.browser.user_state(&self.id)?.user.as_ref())
            .is_some()
    }

    fn should_refresh_details(&self, event: &AppEvent) -> bool {
        matches!(event, AppEvent::BrowserEvent(BrowserEvent::UserDetailsUpdated(id)) if id == &self.id)
    }

    fn has_share_button(&self) -> bool {
        true
    }

    fn on_share_clicked(&self) {
        self.share_link(&format!("https://open.spotify.com/user/{}", self.id));
    }
}

impl CardListModel for UserDetailsModel {
    fn get_store(&self) -> Option<impl Deref<Target = ListStore<CardModel>> + '_> {
        self.app_model
            .map_state_opt(|s| Some(&s.browser.user_state(&self.id)?.playlists))
    }

    fn load_more(&self) {
        PageModel::load_more(self);
    }

    fn refresh(&self) {}

    fn has_items(&self) -> bool {
        self.app_model
            .map_state_opt(|s| Some(&s.browser.user_state(&self.id)?.playlists))
            .map(|s| s.len() > 0)
            .unwrap_or(false)
    }

    fn open_item(&self, id: String) {
        self.dispatcher.dispatch(AppAction::ViewPlaylist(id));
    }

    fn image_shape(&self) -> ImageShape {
        ImageShape::Square
    }
}

impl SimpleHeaderBarModel for UserDetailsModel {
    fn selection_context(&self) -> Option<SelectionContext> {
        None
    }

    fn select_all(&self) {}
}

impl crate::app::ProvidesApi for UserDetailsModel {
    fn api_service(&self) -> std::sync::Arc<riff_api::ApiService> {
        self.app_model.api()
    }
}
