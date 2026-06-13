use std::cell::{Cell, Ref};
use std::ops::Deref;
use std::rc::Rc;

use gettextrs::gettext;

use crate::app::components::{CardLayout, CardListComponent, CardListModel, CardListPageModel, CardSize, ImageShape, SortOrder};
use crate::app::dispatch::Worker;
use crate::app::models::*;
use crate::app::state::HomeState;
use crate::app::{ActionDispatcher, AppAction, AppEvent, AppModel, BrowserAction, BrowserEvent, ListStore, PaginationTarget};

pub struct SavedPlaylistsModel {
    app_model: Rc<AppModel>,
    dispatcher: Box<dyn ActionDispatcher>,
}

impl SavedPlaylistsModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
        Self { app_model, dispatcher }
    }

    fn state(&self) -> Option<Ref<'_, HomeState>> {
        self.app_model.map_state_opt(|s| s.browser.home_state())
    }
}

impl CardListModel for SavedPlaylistsModel {
    fn get_store(&self) -> Option<impl Deref<Target = ListStore<CardModel>> + '_> {
        Some(Ref::map(self.state()?, |s| &s.playlists))
    }

    fn refresh(&self) {
        let api = self.app_model.get_spotify();
        if let Some(state) = self.state() {
            let batch_size = state.next_playlists_page.batch_size;
            drop(state);
            self.dispatcher
                .call_spotify_and_dispatch(move || async move {
                    api.get_saved_playlists(0, batch_size)
                        .await
                        .map(|playlists| BrowserAction::SetPlaylistsContent(playlists).into())
                });
        }
    }

    fn has_items(&self) -> bool {
        self.get_store().map(|s| s.len() > 0).unwrap_or(false)
    }

    fn load_more(&self) {
        let api = self.app_model.get_spotify();
        let state = match self.state() {
            Some(s) => s,
            None => return,
        };
        let batch_size = state.next_playlists_page.batch_size;
        let offset = match state.next_playlists_page.next_offset {
            Some(o) => o,
            None => return,
        };
        drop(state);

        self.app_model
            .update_state(BrowserAction::ConsumeNextPage(PaginationTarget::SavedPlaylists).into());

        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                api.get_saved_playlists(offset, batch_size)
                    .await
                    .map(|playlists| BrowserAction::AppendPlaylistsContent(playlists).into())
            });
    }

    fn open_item(&self, id: String) {
        self.dispatcher.dispatch(AppAction::ViewPlaylist(id));
    }

    fn image_shape(&self) -> ImageShape {
        ImageShape::Square
    }
}

impl CardListPageModel for SavedPlaylistsModel {
    fn page_id(&self) -> &str { "playlists" }
    fn empty_title(&self) -> String { gettext("You have no saved playlists.") }
    fn empty_description(&self) -> String { gettext("Your playlists will be shown here.") }

    fn available_sort_orders(&self) -> &[SortOrder] {
        &[SortOrder::RecentlyAdded, SortOrder::Alphabetic, SortOrder::Creator]
    }

    fn should_refresh(&self, event: &AppEvent) -> bool {
        matches!(event, AppEvent::BrowserEvent(BrowserEvent::SavedPlaylistsUpdated))
    }
}

pub type SavedPlaylists = CardListComponent<SavedPlaylistsModel>;

pub fn make_saved_playlists(
    worker: Worker,
    model: SavedPlaylistsModel,
    shared_layout: Rc<Cell<CardLayout>>,
    shared_size: Rc<Cell<CardSize>>,
    dispatcher: Box<dyn ActionDispatcher>,
) -> SavedPlaylists {
    CardListComponent::new(Rc::new(model), worker, shared_layout, shared_size, dispatcher)
}
