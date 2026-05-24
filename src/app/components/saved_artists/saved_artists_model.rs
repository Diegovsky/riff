use std::cell::Ref;
use std::rc::Rc;

use crate::app::models::*;
use crate::app::state::HomeState;
use crate::app::{ActionDispatcher, AppAction, AppModel, BrowserAction, ListStore, PaginationTarget};

pub struct SavedArtistsModel {
    app_model: Rc<AppModel>,
    dispatcher: Box<dyn ActionDispatcher>,
}

impl SavedArtistsModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
        Self {
            app_model,
            dispatcher,
        }
    }

    fn state(&self) -> Option<Ref<'_, HomeState>> {
        self.app_model.map_state_opt(|s| s.browser.home_state())
    }

    pub fn get_list_store(&self) -> Option<impl std::ops::Deref<Target = ListStore<ArtistModel>> + '_> {
        Some(Ref::map(self.state()?, |s| &s.artists))
    }

    pub fn has_artists(&self) -> bool {
        self.state().map(|s| !s.artists.is_empty()).unwrap_or(false)
    }

    pub fn refresh_saved_artists(&self) -> Option<()> {
        let api = self.app_model.get_spotify();
        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                let (artists, cursor) = api.get_followed_artists(None, 30).await?;
                Ok(BrowserAction::SetSavedArtists(artists, cursor).into())
            });
        Some(())
    }

    pub fn load_more_artists(&self) -> Option<()> {
        let state = self.state()?;
        let cursor = state.artists_cursor.clone();
        drop(state);

        // Empty string means first page hasn't loaded yet; None means exhausted
        let after = match cursor {
            Some(ref c) if c.is_empty() => return None,
            Some(c) => Some(c),
            None => return None,
        };

        self.app_model
            .update_state(BrowserAction::ConsumeNextPage(PaginationTarget::SavedArtists).into());

        let api = self.app_model.get_spotify();
        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                let (artists, cursor) = api.get_followed_artists(after, 30).await?;
                Ok(BrowserAction::AppendSavedArtists(artists, cursor).into())
            });

        Some(())
    }

    pub fn open_artist(&self, id: String) {
        self.dispatcher.dispatch(AppAction::ViewArtist(id));
    }
}
