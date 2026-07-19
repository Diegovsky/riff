// Playlist adapter for the search page's scoped track view (Songs filter).
// Reads the single `SearchState`: the query and scoped track results.

use gio::prelude::*;
use gio::SimpleActionGroup;
use std::cell::Ref;
use std::ops::Deref;
use std::rc::Rc;

use crate::impl_playlist_model_base;

use crate::app::components::DetailsPageModel;
use crate::app::components::{labels, PlaylistModel};
use crate::app::models::*;
use crate::app::state::SelectionContext;
use crate::app::state::{PlaybackAction, SearchState, SelectionState, CARD_BATCH_SIZE};
use crate::app::{ActionDispatcher, AppModel, SongsSource};

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
    pub fn new(app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
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
        load_more_scope(
            &self.app_model,
            self.dispatcher.as_ref(),
            SearchType::Tracks,
        );
    }
}

impl PlaylistModel for SearchScopeTracksModel {
    fn song_list_model(&self) -> SongListModel {
        self.search_state()
            .map(|s| s.scope_tracks.clone())
            .unwrap_or_else(|| SongListModel::new(CARD_BATCH_SIZE as u32))
    }

    fn autoscroll_to_playing(&self) -> bool {
        false
    }

    impl_playlist_model_base!();

    fn enable_selection(&self) -> bool {
        self.enable_selection_with_context(SelectionContext::Default)
    }

    fn play_song_at(&self, _pos: usize, id: &str) {
        let tracks: Vec<SongDescription> = PlaylistModel::song_list_model(self).collect();
        let total = tracks.len();
        let query = self.get_query().unwrap_or_default();
        let batch = SongBatch {
            songs: tracks,
            batch: Batch {
                offset: 0,
                batch_size: total,
                total,
            },
        };
        self.dispatcher
            .dispatch(PlaybackAction::LoadPagedSongs(SongsSource::Search(query), batch).into());
        self.dispatcher
            .dispatch(PlaybackAction::Load(id.to_string()).into());
    }

    fn actions_for(&self, song: &SongDescription) -> Option<gio::ActionGroup> {
        let group = SimpleActionGroup::new();
        for a in song.make_artist_actions(self.dispatcher.box_clone(), None) {
            group.add_action(&a);
        }
        group.add_action(&song.make_album_action(self.dispatcher.box_clone(), None));
        group.add_action(&song.make_link_action(None));
        Some(group.upcast())
    }

    fn menu_for(&self, song: &SongDescription) -> Option<gio::MenuModel> {
        let menu = gio::Menu::new();
        menu.append(Some(&*labels::VIEW_ALBUM), Some("song.view_album"));
        for artist in song.artists.iter() {
            menu.append(
                Some(&labels::more_from_label(&artist.name)),
                Some(&format!("song.view_artist_{}", artist.id)),
            );
        }
        menu.append(Some(&*labels::COPY_LINK), Some("song.copy_link"));
        Some(menu.upcast())
    }
}
