use std::cell::Ref;
use std::ops::Deref;
use std::rc::Rc;

use gettextrs::gettext;

use crate::app::components::{CardListModel, CardListPage, CardListPageConfig, ImageShape};
use crate::app::dispatch::Worker;
use crate::app::models::*;
use crate::app::state::HomeState;
use crate::app::{ActionDispatcher, AppAction, AppEvent, AppModel, BrowserAction, BrowserEvent, ListStore, PaginationTarget};

pub struct SavedArtistsModel {
    app_model: Rc<AppModel>,
    dispatcher: Box<dyn ActionDispatcher>,
}

impl SavedArtistsModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
        Self { app_model, dispatcher }
    }

    fn state(&self) -> Option<Ref<'_, HomeState>> {
        self.app_model.map_state_opt(|s| s.browser.home_state())
    }
}

impl CardListModel for SavedArtistsModel {
    fn get_store(&self) -> Option<impl Deref<Target = ListStore<CardModel>> + '_> {
        Some(Ref::map(self.state()?, |s| &s.artists))
    }

    fn refresh(&self) {
        let api = self.app_model.get_spotify();
        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                let (artists, cursor) = api.get_followed_artists(None, 30).await?;
                Ok(BrowserAction::SetSavedArtists(artists, cursor).into())
            });
    }

    fn has_items(&self) -> bool {
        self.state().map(|s| !s.artists.is_empty()).unwrap_or(false)
    }

    fn load_more(&self) {
        let state = match self.state() {
            Some(s) => s,
            None => return,
        };
        let cursor = state.artists_cursor.clone();
        drop(state);

        let after = match cursor {
            Some(ref c) if c.is_empty() => return,
            Some(c) => Some(c),
            None => return,
        };

        self.app_model
            .update_state(BrowserAction::ConsumeNextPage(PaginationTarget::SavedArtists).into());

        let api = self.app_model.get_spotify();
        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                let (artists, cursor) = api.get_followed_artists(after, 30).await?;
                Ok(BrowserAction::AppendSavedArtists(artists, cursor).into())
            });
    }

    fn open_item(&self, id: String) {
        self.dispatcher.dispatch(AppAction::ViewArtist(id));
    }

    fn image_shape(&self) -> ImageShape {
        ImageShape::Round
    }
}

pub type SavedArtists = CardListPage<SavedArtistsModel>;

pub fn make_saved_artists(worker: Worker, model: SavedArtistsModel) -> SavedArtists {
    CardListPage::new(
        model,
        worker,
        CardListPageConfig {
            empty_title: gettext("You have no saved artists."),
            empty_description: gettext("Your followed artists will be shown here."),
            empty_icon: "avatar-default-symbolic".to_string(),
        },
        |event| matches!(event, AppEvent::BrowserEvent(BrowserEvent::SavedArtistsUpdated)),
    )
}
