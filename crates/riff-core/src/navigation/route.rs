#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HomePage {
    #[default]
    Library,
    SavedTracks,
    SavedPlaylists,
    SavedArtists,
    NowPlaying,
}

impl HomePage {
    pub const ALL: [HomePage; 5] = [
        Self::Library,
        Self::SavedTracks,
        Self::SavedPlaylists,
        Self::SavedArtists,
        Self::NowPlaying,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::SavedTracks => "saved_tracks",
            Self::SavedPlaylists => "saved_playlists",
            Self::SavedArtists => "saved_artists",
            Self::NowPlaying => "now_playing",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|page| page.id() == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Route {
    Home(HomePage),
    Album(String),
    Artist(String),
    Playlist(String),
    User(String),
    Search,
}

impl Route {
    pub fn screen_key(&self) -> String {
        match self {
            Self::Home(_) => "home".to_string(),
            Self::Album(id) => format!("album_{id}"),
            Self::Artist(id) => format!("artist_{id}"),
            Self::Playlist(id) => format!("playlist_{id}"),
            Self::User(id) => format!("user_{id}"),
            Self::Search => "search".to_string(),
        }
    }

    pub fn same_screen(&self, other: &Route) -> bool {
        match (self, other) {
            (Self::Home(_), Self::Home(_)) => true,
            _ => self == other,
        }
    }

    pub fn is_valid(&self) -> bool {
        match self {
            Self::Album(id) | Self::Artist(id) | Self::Playlist(id) | Self::User(id) => {
                !id.is_empty()
            }
            Self::Home(_) | Self::Search => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_pages_share_a_screen() {
        let library = Route::Home(HomePage::Library);
        let tracks = Route::Home(HomePage::SavedTracks);
        assert_ne!(library, tracks);
        assert!(library.same_screen(&tracks));
        assert_eq!(library.screen_key(), tracks.screen_key());
    }

    #[test]
    fn item_routes_need_an_id() {
        assert!(!Route::Album(String::new()).is_valid());
        assert!(Route::Album("a".into()).is_valid());
        assert!(Route::Search.is_valid());
    }

    #[test]
    fn home_page_ids_round_trip() {
        for page in HomePage::ALL {
            assert_eq!(HomePage::from_id(page.id()), Some(page));
        }
        assert_eq!(HomePage::from_id("nope"), None);
    }
}
