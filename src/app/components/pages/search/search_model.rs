use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;
use std::time::Duration;

use crate::app::dispatch::ActionDispatcher;
use crate::app::models::*;
use crate::app::state::{AppAction, AppModel, BrowserAction, PlaybackAction};

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

    pub fn go_back(&self) {
        self.dispatcher
            .dispatch(BrowserAction::NavigationPop.into());
    }

    pub fn search(&self, query: String) {
        self.dispatcher
            .dispatch(BrowserAction::Search(query).into());
    }

    fn get_query(&self) -> Option<impl Deref<Target = String> + '_> {
        self.app_model
            .map_state_opt(|s| Some(&s.browser.search_state()?.query).filter(|s| !s.is_empty()))
    }

    pub fn fetch_results(&self) {
        let api = self.app_model.get_spotify();
        if let Some(query) = self.get_query() {
            let query = query.to_owned();
            self.dispatcher
                .call_spotify_and_dispatch(move || async move {
                    api.search(&query, 0, 5)
                        .await
                        .map(|results| BrowserAction::SetSearchResults(Box::new(results)).into())
                });
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
}
