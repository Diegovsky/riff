use std::cell::Cell;
use std::rc::Rc;

use crate::app::components::sidebar::{Sidebar, SidebarModel};
use crate::app::components::*;
use crate::app::{AppModel, Dispatcher};
use crate::settings::StateTracker;

pub struct ScreenFactory {
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
    shared_layout: Rc<Cell<CardLayout>>,
    shared_size: Rc<Cell<CardSize>>,
    registrar: HeaderRegistrar,
}

impl ScreenFactory {
    pub fn new(
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
        registrar: HeaderRegistrar,
    ) -> Self {
        let tracker = StateTracker::new_from_gsettings();
        Self {
            app_model,
            dispatcher,
            shared_layout: Rc::new(Cell::new(tracker.load_card_layout())),
            shared_size: Rc::new(Cell::new(tracker.load_card_size())),
            registrar,
        }
    }

    /// Register a `CardListComponent`'s title and view button with the shared
    /// header (keyed by `name`) and return the page as-is (no local header).
    fn register_card_page<M: CardListPageModel + 'static>(
        &self,
        name: &str,
        title: &str,
        page: CardListComponent<M>,
    ) -> CardListComponent<M> {
        self.registrar.set_static_title(name, title);
        self.registrar.add_end(name, page.view_button());
        page
    }

    pub fn make_library(&self) -> impl ListenerComponent {
        let model = SavedAlbumsModel::new(Rc::clone(&self.app_model), self.dispatcher.clone());
        let page = make_saved_albums(
            model,
            Rc::clone(&self.shared_layout),
            Rc::clone(&self.shared_size),
            self.dispatcher.clone(),
        );
        self.register_card_page("library", &gettext("Library"), page)
    }

    pub fn make_sidebar(&self, listbox: gtk::ListBox) -> impl ListenerComponent {
        let model = SidebarModel::new(Rc::clone(&self.app_model), self.dispatcher.clone());
        Sidebar::new(listbox, Rc::new(model))
    }

    pub fn make_saved_playlists(&self) -> impl ListenerComponent {
        let model = SavedPlaylistsModel::new(Rc::clone(&self.app_model), self.dispatcher.clone());
        let page = make_saved_playlists(
            model,
            Rc::clone(&self.shared_layout),
            Rc::clone(&self.shared_size),
            self.dispatcher.clone(),
        );
        self.register_card_page("saved_playlists", &gettext("Playlists"), page)
    }

    pub fn make_saved_artists(&self) -> impl ListenerComponent {
        let model = SavedArtistsModel::new(Rc::clone(&self.app_model), self.dispatcher.clone());
        let page = make_saved_artists(
            model,
            Rc::clone(&self.shared_layout),
            Rc::clone(&self.shared_size),
            self.dispatcher.clone(),
        );
        self.register_card_page("saved_artists", &gettext("Artists"), page)
    }

    pub fn make_now_playing(&self) -> impl ListenerComponent {
        let model = Rc::new(NowPlayingModel::new(
            Rc::clone(&self.app_model),
            self.dispatcher.clone(),
        ));
        NowPlaying::new(model, self.registrar.clone(), "now_playing".to_string())
    }

    pub fn make_saved_tracks(&self) -> impl ListenerComponent {
        let model = Rc::new(SavedTracksModel::new(
            Rc::clone(&self.app_model),
            self.dispatcher.clone(),
        ));
        SavedTracks::new(model, self.registrar.clone(), "saved_tracks".to_string())
    }

    pub fn make_album_details(&self, id: String) -> impl ListenerComponent {
        let name = format!("album_{id}");
        let model = Rc::new(DetailsModel::new(
            id,
            Rc::clone(&self.app_model),
            self.dispatcher.clone(),
        ));
        Details::new(model, self.registrar.clone(), name)
    }

    pub fn make_search_results(&self) -> impl ListenerComponent {
        let model = SearchResultsModel::new(Rc::clone(&self.app_model), self.dispatcher.clone());
        SearchResults::new(
            model,
            Rc::clone(&self.shared_layout),
            Rc::clone(&self.shared_size),
            self.dispatcher.clone(),
            self.registrar.clone(),
        )
    }

    pub fn make_artist_details(&self, id: String) -> impl ListenerComponent {
        let name = format!("artist_{id}");
        let model = Rc::new(ArtistDetailsModel::new(
            id,
            Rc::clone(&self.app_model),
            self.dispatcher.clone(),
        ));
        ArtistDetails::new(
            model,
            Rc::clone(&self.shared_layout),
            Rc::clone(&self.shared_size),
            self.dispatcher.clone(),
            self.registrar.clone(),
            name,
        )
    }

    pub fn make_playlist_details(&self, id: String) -> impl ListenerComponent {
        let name = format!("playlist_{id}");
        let model = Rc::new(PlaylistDetailsModel::new(
            id,
            Rc::clone(&self.app_model),
            self.dispatcher.clone(),
        ));
        PlaylistDetails::new(model, self.registrar.clone(), name)
    }

    pub fn make_user_details(&self, id: String) -> impl ListenerComponent {
        let name = format!("user_{id}");
        let model = UserDetailsModel::new(id, Rc::clone(&self.app_model), self.dispatcher.clone());
        UserDetails::new(
            model,
            Rc::clone(&self.shared_layout),
            Rc::clone(&self.shared_size),
            self.dispatcher.clone(),
            self.registrar.clone(),
            name,
        )
    }
}
