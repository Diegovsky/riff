use std::cell::{Cell, Ref};
use std::ops::Deref;
use std::rc::Rc;

use gettextrs::gettext;

use crate::app::components::{CardLayout, CardListComponent, CardListModel, CardListPageModel, CardSize, ImageShape, SortOrder};
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

    fn has_more(&self) -> bool {
        self.state().map(|s| s.artists_cursor.as_ref().map_or(false, |c| !c.is_empty())).unwrap_or(false)
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

impl CardListPageModel for SavedArtistsModel {
    fn page_id(&self) -> &str { "artists" }
    fn empty_title(&self) -> String { gettext("You have no saved artists.") }
    fn empty_description(&self) -> String { gettext("Your followed artists will be shown here.") }
    fn empty_icon(&self) -> &str { "avatar-default-symbolic" }

    fn available_sort_orders(&self) -> &[SortOrder] {
        &[SortOrder::RecentlyAdded, SortOrder::Alphabetic, SortOrder::Popularity]
    }

    fn should_refresh(&self, event: &AppEvent) -> bool {
        matches!(event, AppEvent::BrowserEvent(BrowserEvent::SavedArtistsUpdated))
    }
}

pub type SavedArtists = CardListComponent<SavedArtistsModel>;

pub fn make_saved_artists(
    worker: Worker,
    model: SavedArtistsModel,
    shared_layout: Rc<Cell<CardLayout>>,
    shared_size: Rc<Cell<CardSize>>,
    dispatcher: Rc<dyn ActionDispatcher>,
) -> SavedArtists {
    CardListComponent::new(Rc::new(model), worker, shared_layout, shared_size, dispatcher)
}
