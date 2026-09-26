// Playlist adapter for the search page's scoped track view (Songs filter).
// Reads the single `SearchState`: the query and scoped track results.

use gio::prelude::*;
use gio::SimpleActionGroup;
use std::cell::Ref;
use std::ops::Deref;
use std::rc::Rc;

use crate::impl_track_list_model_base;

use crate::app::components::DetailsPageModel;
use crate::app::components::SongActions;
use crate::app::components::{build_song_menu, QueueMenuEntry, TrackListModel};
use crate::app::models::*;
use crate::app::state::SelectionContext;
use crate::app::state::{PlaybackAction, SearchState, SelectionState, CARD_BATCH_SIZE};
use crate::app::{AppModel, Dispatcher, SongsSource};

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

    fn play_song_at(&self, _pos: usize, id: &str) {
        let tracks: Vec<Track> = TrackListModel::song_list_model(self).collect();
        let query = self.get_query().unwrap_or_default();
        self.dispatcher
            .dispatch(PlaybackAction::LoadContextSongs(SongsSource::Search(query), tracks).into());
        self.dispatcher
            .dispatch(PlaybackAction::Load(id.to_string()).into());
    }

    fn actions_for(&self, song: &Track) -> Option<SimpleActionGroup> {
        let group = SimpleActionGroup::new();
        for a in song.make_artist_actions(self.dispatcher.clone()) {
            group.add_action(&a);
        }
        group.add_action(&song.make_album_action(self.dispatcher.clone()));
        group.add_action(&song.make_link_action());
        Some(group)
    }

    fn menu_for(&self, song: &Track, liked: bool, pinned: Option<bool>) -> Option<gio::MenuModel> {
        Some(build_song_menu(
            song,
            true,
            None,
            QueueMenuEntry::None,
            Some(liked),
            pinned,
        ))
    }
}

impl crate::app::ProvidesApi for SearchScopeTracksModel {
    fn api_service(&self) -> std::sync::Arc<riff_api::ApiService> {
        self.app_model.api()
    }
}
