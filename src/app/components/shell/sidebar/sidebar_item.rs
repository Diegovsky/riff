use gettextrs::gettext;
use glib::Properties;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::app::models::PlaylistSummary;

const LIBRARY: &str = "library";
const SAVED_TRACKS: &str = "saved_tracks";
const NOW_PLAYING: &str = "now_playing";
const SAVED_PLAYLISTS: &str = "saved_playlists";
const SAVED_ARTISTS: &str = "saved_artists";
const PLAYLIST: &str = "playlist";
pub const SAVED_PLAYLISTS_SECTION: &str = "saved_playlists_section";
pub const PINNED_PLAYLISTS_SECTION: &str = "pinned_playlists_section";
pub const LIBRARY_SECTION: &str = "library_section";
pub const CREATE_PLAYLIST_ITEM: &str = "create_playlist";

#[derive(Debug)]
pub enum SidebarDestination {
    Library,
    SavedTracks,
    NowPlaying,
    SavedPlaylists,
    SavedArtists,
    Playlist(PlaylistSummary),
}

impl SidebarDestination {
    pub fn id(&self) -> &'static str {
        match self {
            Self::Library => LIBRARY,
            Self::SavedTracks => SAVED_TRACKS,
            Self::NowPlaying => NOW_PLAYING,
            Self::SavedPlaylists => SAVED_PLAYLISTS,
            Self::SavedArtists => SAVED_ARTISTS,
            Self::Playlist(_) => PLAYLIST,
        }
    }

    pub fn title(&self) -> String {
        match self {
            // translators: This is a sidebar entry to browse to saved albums.
            Self::Library => gettext("Library"),
            // translators: This is a sidebar entry to browse to saved tracks.
            Self::SavedTracks => gettext("Saved Tracks"),
            // translators: This is a sidebar entry to browse to saved playlists.
            Self::NowPlaying => gettext("Now Playing"),
            // translators: This is a sidebar entry that marks that the entries below are playlists.
            Self::SavedPlaylists => gettext("Playlists"),
            // translators: This is a sidebar entry to browse to followed artists.
            Self::SavedArtists => gettext("Artists"),
            Self::Playlist(PlaylistSummary { title, .. }) => title.clone(),
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Library => "library-music-symbolic",
            Self::SavedTracks => "audio-x-generic-symbolic",
            Self::NowPlaying => "music-queue-symbolic",
            Self::SavedPlaylists => "playlist2-symbolic",
            Self::SavedArtists => "avatar-default-symbolic",
            Self::Playlist(_) => "playlist2-symbolic",
        }
    }
}

impl SidebarItem {
    pub fn from_destination(dest: SidebarDestination) -> Self {
        let (id, data, title) = match dest {
            SidebarDestination::Playlist(PlaylistSummary { id, title }) => {
                (PLAYLIST, Some(id), title)
            }
            _ => (dest.id(), None, dest.title()),
        };
        glib::Object::builder()
            .property("id", id)
            .property("data", data.unwrap_or_default())
            .property("title", &title)
            .property("navigatable", true)
            .build()
    }

    pub fn playlists_section() -> Self {
        glib::Object::builder()
            .property("id", SAVED_PLAYLISTS_SECTION)
            .property("data", String::new())
            .property("title", gettext("All Playlists"))
            .property("navigatable", false)
            .build()
    }

    pub fn pinned_playlists_section() -> Self {
        glib::Object::builder()
            .property("id", PINNED_PLAYLISTS_SECTION)
            .property("data", String::new())
            .property("title", gettext("Pinned Playlists"))
            .property("navigatable", false)
            .build()
    }

    pub fn library_section() -> Self {
        glib::Object::builder()
            .property("id", LIBRARY_SECTION)
            .property("data", String::new())
            .property("title", gettext("Library"))
            .property("navigatable", false)
            .build()
    }

    pub fn create_playlist_item() -> Self {
        glib::Object::builder()
            .property("id", CREATE_PLAYLIST_ITEM)
            .property("data", String::new())
            .property("title", gettext("New Playlist"))
            .property("navigatable", false)
            .build()
    }

    pub fn destination(&self) -> Option<SidebarDestination> {
        let navigatable = self.property::<bool>("navigatable");
        if navigatable {
            let id = self.id();
            let data = self.property::<String>("data");
            let title = self.title();
            match id.as_str() {
                LIBRARY => Some(SidebarDestination::Library),
                SAVED_TRACKS => Some(SidebarDestination::SavedTracks),
                NOW_PLAYING => Some(SidebarDestination::NowPlaying),
                SAVED_PLAYLISTS => Some(SidebarDestination::SavedPlaylists),
                SAVED_ARTISTS => Some(SidebarDestination::SavedArtists),
                PLAYLIST => Some(SidebarDestination::Playlist(PlaylistSummary {
                    id: data,
                    title,
                })),
                _ => None,
            }
        } else {
            None
        }
    }

    pub fn icon(&self) -> Option<&str> {
        match self.id().as_str() {
            CREATE_PLAYLIST_ITEM => Some("list-add-symbolic"),
            _ => self.destination().map(|d| d.icon()),
        }
    }
}

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Debug, Default, Properties)]
    #[properties(wrapper_type = super::SidebarItem)]
    pub struct SidebarItem {
        #[property(get, set)]
        pub id: RefCell<String>,
        #[property(get, set)]
        pub data: RefCell<String>,
        #[property(get, set)]
        pub title: RefCell<String>,
        #[property(get, set)]
        pub navigatable: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SidebarItem {
        const NAME: &'static str = "SideBarItem";
        type Type = super::SidebarItem;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for SidebarItem {}
}

glib::wrapper! {
    pub struct SidebarItem(ObjectSubclass<imp::SidebarItem>);
}
