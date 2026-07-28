use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;

use crate::app::dispatch::ActionDispatcher;
use crate::app::models::*;
use crate::app::state::{AppAction, AppModel, BrowserAction, PaginationTarget, CARD_BATCH_SIZE};

/// Items fetched per category for the combined ("all") preview view. Enough to
/// fill the couple of rows each section shows before "Show All".
const COMBINED_RESULTS_LIMIT: usize = 24;

pub struct SearchResultsModel {
    app_model: Rc<AppModel>,
    dispatcher: Box<dyn ActionDispatcher>,
    queued_song: RefCell<Option<SongDescription>>,
}

impl SearchResultsModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
        Self {
            queued_song: Default::default(),
            app_model,
            dispatcher,
        }
    }

    /// Shared app model, used to build the scoped view adapters.
    pub fn app_model(&self) -> Rc<AppModel> {
        Rc::clone(&self.app_model)
    }

    pub fn search(&self, query: String) {
        self.dispatcher
            .dispatch(BrowserAction::Search(query).into());
    }

    /// Set the active filter (`None` = combined results). Re-runs the search.
    pub fn set_filter(&self, filter: Option<SearchType>) {
        self.dispatcher
            .dispatch(BrowserAction::SetSearchFilter(filter).into());
    }

    pub fn get_filter(&self) -> Option<SearchType> {
        self.app_model
            .map_state_opt(|s| Some(&s.browser.search_state()?.filter))
            .and_then(|f| *f)
    }

    fn get_query(&self) -> Option<impl Deref<Target = String> + '_> {
        self.app_model
            .map_state_opt(|s| Some(&s.browser.search_state()?.query).filter(|s| !s.is_empty()))
    }

    pub fn has_query(&self) -> bool {
        self.get_query().is_some()
    }

    /// Run the appropriate search for the current query and filter.
    pub fn fetch_results(&self) {
        let Some(query) = self.get_query() else {
            return;
        };
        let query = query.to_owned();
        let api = self.app_model.get_spotify();
        match self.get_filter() {
            None => {
                self.dispatcher
                    .call_spotify_and_dispatch(move || async move {
                        api.search(&query, 0, COMBINED_RESULTS_LIMIT)
                            .await
                            .map(|results| {
                                BrowserAction::SetSearchResults(Box::new(results)).into()
                            })
                    });
            }
            Some(search_type) => {
                self.dispatcher
                    .call_spotify_and_dispatch(move || async move {
                        api.search_scoped(&query, search_type, 0, CARD_BATCH_SIZE)
                            .await
                            .map(|results| {
                                BrowserAction::SetSearchScopeResults(
                                    search_type,
                                    query.clone(),
                                    Box::new(results),
                                )
                                .into()
                            })
                    });
            }
        }
    }

    pub fn get_results(&self) -> Option<impl Deref<Target = SearchResults> + '_> {
        self.app_model
            .map_state_opt(|s| Some(&s.browser.search_state()?.results))
    }
    pub fn open_track(&self, song: SongDescription) {
        self.queued_song.borrow_mut().replace(song.clone());
        self.dispatcher
            .dispatch(AppAction::ViewAlbum(song.album.id.clone()));
    }
    pub fn on_album_loaded(&self, id: &str) {
        if let Some(song) = self.queued_song.borrow_mut().take() {
            if song.album.id == id {
                self.dispatcher
                    .dispatch(BrowserAction::PlaySong(song.id.clone()).into())
            }
        }
    }

    pub fn open_album(&self, id: String) {
        self.dispatcher.dispatch(AppAction::ViewAlbum(id));
    }

    pub fn open_artist(&self, id: String) {
        self.dispatcher.dispatch(AppAction::ViewArtist(id));
    }

    pub fn open_playlist(&self, id: String) {
        self.dispatcher.dispatch(AppAction::ViewPlaylist(id));
    }
}

/// Load the next page of a scoped search view and append it to the state.
///
/// Shared by the scoped card and track adapters. Reads the current query and
/// next page offset from `SearchState`, marks the page consumed, and dispatches
/// the follow-up scoped search. No-op when the state's filter no longer matches
/// `search_type`, there is no further page, or the query is blank.
pub(super) fn load_more_scope(
    app_model: &Rc<AppModel>,
    dispatcher: &(dyn ActionDispatcher + 'static),
    search_type: SearchType,
) {
    let api = app_model.get_spotify();
    let Some(state) = app_model.map_state_opt(|s| s.browser.search_state()) else {
        return;
    };
    if state.filter != Some(search_type) {
        return;
    }
    let query = state.query.clone();
    let batch_size = state.scope_page.batch_size;
    let Some(offset) = state.scope_page.next_offset else {
        return;
    };
    drop(state);

    if query.trim().is_empty() {
        return;
    }

    app_model.update_state(BrowserAction::ConsumeNextPage(PaginationTarget::SearchScope).into());

    dispatcher.call_spotify_and_dispatch(move || async move {
        api.search_scoped(&query, search_type, offset, batch_size)
            .await
            .map(|results| {
                BrowserAction::AppendSearchScopeResults(
                    search_type,
                    query.clone(),
                    Box::new(results),
                )
                .into()
            })
    });
}
