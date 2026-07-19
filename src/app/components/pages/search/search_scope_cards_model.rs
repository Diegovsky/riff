// Card-list adapter for the search page's scoped card view (artists/albums/playlists).
// Reads the single `SearchState`: the active filter, query, and paginated results.

use std::cell::Ref;
use std::ops::Deref;
use std::rc::Rc;

use crate::app::components::{CardListModel, ImageShape};
use crate::app::models::*;
use crate::app::state::SearchState;
use crate::app::{ActionDispatcher, AppAction, AppModel, ListStore};

use super::load_more_scope;

pub struct SearchScopeCardsModel {
    app_model: Rc<AppModel>,
    dispatcher: Box<dyn ActionDispatcher>,
}

impl SearchScopeCardsModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
        Self {
            app_model,
            dispatcher,
        }
    }

    fn state(&self) -> Option<Ref<'_, SearchState>> {
        self.app_model.map_state_opt(|s| s.browser.search_state())
    }

    fn filter(&self) -> Option<SearchType> {
        self.state().and_then(|s| s.filter)
    }
}

impl CardListModel for SearchScopeCardsModel {
    fn get_store(&self) -> Option<impl Deref<Target = ListStore<CardModel>> + '_> {
        Some(Ref::map(self.state()?, |s| &s.scope_cards))
    }

    fn refresh(&self) {}

    fn has_items(&self) -> bool {
        self.get_store().map(|s| s.len() > 0).unwrap_or(false)
    }

    fn has_more(&self) -> bool {
        self.state()
            .map(|s| s.scope_page.next_offset.is_some())
            .unwrap_or(false)
    }

    fn load_more(&self) {
        let Some(search_type) = self.filter() else {
            return;
        };
        if search_type == SearchType::Tracks {
            return;
        }
        load_more_scope(&self.app_model, self.dispatcher.as_ref(), search_type);
    }

    fn open_item(&self, id: String) {
        match self.filter() {
            Some(SearchType::Albums) => self.dispatcher.dispatch(AppAction::ViewAlbum(id)),
            Some(SearchType::Artists) => self.dispatcher.dispatch(AppAction::ViewArtist(id)),
            Some(SearchType::Playlists) => self.dispatcher.dispatch(AppAction::ViewPlaylist(id)),
            _ => {}
        }
    }

    fn image_shape(&self) -> ImageShape {
        match self.filter() {
            Some(SearchType::Artists) => ImageShape::Round,
            _ => ImageShape::Square,
        }
    }
}
