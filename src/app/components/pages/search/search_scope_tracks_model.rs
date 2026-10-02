// Playlist adapter for the search page's scoped track view (Songs filter).
// Reads the single `SearchState`: the query and scoped track results.

use gio::SimpleActionGroup;
use std::cell::Ref;
use std::ops::Deref;
use std::rc::Rc;

use crate::impl_track_list_model_base;

use crate::app::components::DetailsPageModel;
use crate::app::components::{source_context_actions, track_menu, TrackListModel};
use crate::app::models::*;
use crate::app::state::SelectionContext;
use crate::app::state::{SearchState, SelectionState, CARD_BATCH_SIZE};
use crate::app::{AppAction, AppModel, Dispatcher, SongsSource};

use super::load_more_scope;

pub struct SearchScopeTracksModel {
    base: DetailsPageModel,
}

impl Deref for SearchScopeTracksModel {
    type Target = DetailsPageModel;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl SearchScopeTracksModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        Self {
            base: DetailsPageModel::new_without_id(app_model, dispatcher),
        }
    }

    fn search_state(&self) -> Option<Ref<'_, SearchState>> {
        self.app_model.map_state_opt(|s| s.browser.search_state())
    }

    pub fn get_query(&self) -> Option<String> {
        self.search_state().map(|s| s.query.clone())
    }

    pub fn load_more(&self) {
        load_more_scope(&self.app_model, &self.dispatcher, SearchType::Tracks);
    }
}

impl TrackListModel for SearchScopeTracksModel {
    fn song_list_model(&self) -> SongListModel {
        self.search_state()
            .map(|s| s.scope_tracks.clone())
            .unwrap_or_else(|| SongListModel::new(CARD_BATCH_SIZE as u32))
    }

    fn autoscroll_to_playing(&self) -> bool {
        false
    }

    fn show_album_column(&self) -> bool {
        true
    }

    fn load_more(&self) {
        SearchScopeTracksModel::load_more(self);
    }

    impl_track_list_model_base!();

    fn enable_selection(&self) -> bool {
        self.enable_selection_with_context(SelectionContext::Default)
    }

    fn play_song_at(&self, pos: usize, id: &str) {
        self.play_in_context(TrackListModel::context_actions(self, pos), id);
    }

    fn context_actions(&self, pos: usize) -> Option<Vec<AppAction>> {
        let query = self.get_query().unwrap_or_default();
        let songs = TrackListModel::song_list_model(self);
        source_context_actions(SongsSource::Search(query.clone()), Some(query), &songs, pos)
    }

    fn actions_for(&self, _row: &SongModel, song: &Track) -> Option<SimpleActionGroup> {
        Some(self.base.track_actions(song, None))
    }

    fn menu_for(
        &self,
        _row: &SongModel,
        song: &Track,
        liked: bool,
        pinned: Option<bool>,
    ) -> Option<gio::MenuModel> {
        Some(track_menu(song, None, liked, pinned))
    }
}

impl crate::app::ProvidesApi for SearchScopeTracksModel {
    fn api_service(&self) -> std::sync::Arc<riff_api::ApiService> {
        self.app_model.api()
    }
}
