// The app consumes the provider-neutral data-layer models directly. The leaf
// catalog entities (`Track`, `Album`, `Playlist`, `Artist`, ...) are
// re-exported from `riff_api::models` so the rest of the app can refer to them
// as `crate::app::models::*`.
pub use riff_api::models::{
    Album, AlbumType, Artist, ArtistRef, ContentRating, Device, DeviceKind, ImageSet, Page,
    PlayerState, Playlist, RepeatMode, ResourceId, SearchResults, Track, User,
};

// Only the `#[cfg(test)]` fixtures below name `Provider`; keep its import
// test-scoped so non-test builds don't see an unused re-export.
#[cfg(test)]
use riff_api::models::Provider;

/// UI helper methods for [`Track`].
pub trait TrackExt {
    fn artists_name(&self) -> String;
    fn is_explicit(&self) -> bool;
}

impl TrackExt for Track {
    fn artists_name(&self) -> String {
        self.artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn is_explicit(&self) -> bool {
        matches!(self.content_rating, ContentRating::Explicit)
    }
}

/// UI helper methods for [`Album`].
pub trait AlbumExt {
    fn artists_name(&self) -> String;
    fn release_date_string(&self) -> Option<String>;
    fn album_type_string(&self) -> Option<String>;
}

impl AlbumExt for Album {
    fn artists_name(&self) -> String {
        self.artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn release_date_string(&self) -> Option<String> {
        self.release_date.map(|d| match (d.month, d.day) {
            (Some(m), Some(day)) => format!("{:04}-{:02}-{:02}", d.year, m, day),
            (Some(m), None) => format!("{:04}-{:02}", d.year, m),
            _ => format!("{:04}", d.year),
        })
    }

    fn album_type_string(&self) -> Option<String> {
        Some(match &self.album_type {
            AlbumType::Album => "album".to_string(),
            AlbumType::Single => "single".to_string(),
            AlbumType::Ep => "ep".to_string(),
            AlbumType::Compilation => "compilation".to_string(),
            AlbumType::Live => "live".to_string(),
            AlbumType::Soundtrack => "soundtrack".to_string(),
            AlbumType::Other(s) => s.clone(),
        })
    }
}

/// A request descriptor for the next page to fetch: the offset to load and the
/// page size to request. This is the "fetch intent" counterpart to a loaded
/// `Page<Track>`. It intentionally carries no `total`;
/// the app paginates until a short/empty page is returned (see
/// [`crate::app::state::pagination::Pagination`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageRequest {
    /// Offset of the first element to load.
    pub offset: usize,
    /// Number of elements to request.
    pub batch_size: usize,
}

// "Something"Ref models (UserRef, ArtistRef, AlbumRef) are re-exported from
// the data layer above.

/// The category a scoped ("sub") search page searches within.
///
/// Selecting a filter on the main search page opens a dedicated page scoped to
/// one of these categories, using the Spotify search API restricted to the
/// matching `type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchType {
    Artists,
    Albums,
    Playlists,
    Tracks,
}

impl From<SearchType> for riff_api::models::SearchType {
    fn from(kind: SearchType) -> Self {
        match kind {
            SearchType::Artists => Self::Artist,
            SearchType::Albums => Self::Album,
            SearchType::Playlists => Self::Playlist,
            SearchType::Tracks => Self::Track,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlaylistSummary {
    pub id: String,
    pub title: String,
}

#[derive(Copy, Clone, Default)]
pub struct SongState {
    pub is_playing: bool,
    pub is_selected: bool,
    pub is_liked: bool,
    pub is_explicit_filtered: bool,
}

/// Identifies the source of a song list (playlist, album, etc.) for playback
/// and pagination purposes.
#[derive(Clone, Debug)]
pub enum SongsSource {
    Playlist(String),
    Album(String),
    Artist(String),
    SavedTracks,
    /// Songs shown on a scoped track search page, keyed by the search query.
    Search(String),
}

impl PartialEq for SongsSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Playlist(l), Self::Playlist(r)) => l == r,
            (Self::Album(l), Self::Album(r)) => l == r,
            (Self::Artist(l), Self::Artist(r)) => l == r,
            (Self::SavedTracks, Self::SavedTracks) => true,
            (Self::Search(l), Self::Search(r)) => l == r,
            _ => false,
        }
    }
}

impl Eq for SongsSource {}

impl SongsSource {
    pub fn has_spotify_uri(&self) -> bool {
        matches!(self, Self::Playlist(_) | Self::Album(_))
    }

    pub fn spotify_uri(&self) -> Option<String> {
        match self {
            Self::Playlist(id) => Some(format!("spotify:playlist:{}", id)),
            Self::Album(id) => Some(format!("spotify:album:{}", id)),
            _ => None,
        }
    }
}

/// Test-only constructor for a minimal [`Track`]. Shared across the crate's
/// unit tests so they do not each have to spell out every field.
#[cfg(test)]
pub fn make_track(id: &str) -> Track {
    Track {
        rri: ResourceId {
            provider: Provider::Spotify,
            id: id.to_string(),
            uri: None,
        },
        title: "Title".to_string(),
        artists: vec![],
        album: None,
        duration_ms: 1000,
        track_number: None,
        disc_number: None,
        content_rating: ContentRating::None,
        isrc: None,
        art: ImageSet::default(),
        playable: true,
        popularity: None,
        saved: None,
        preview_url: None,
        url: None,
    }
}

#[cfg(test)]
pub fn make_album(id: &str) -> Album {
    Album {
        rri: ResourceId {
            provider: Provider::Spotify,
            id: id.to_string(),
            uri: None,
        },
        title: String::new(),
        artists: vec![],
        art: ImageSet::default(),
        album_type: AlbumType::Album,
        release_date: None,
        total_tracks: None,
        label: None,
        copyright: None,
        upc: None,
        genres: vec![],
        popularity: None,
        content_rating: ContentRating::None,
        saved: None,
        tracks: None,
        url: None,
    }
}

#[cfg(test)]
pub fn make_artist(id: &str, name: &str) -> Artist {
    Artist {
        rri: ResourceId {
            provider: Provider::Spotify,
            id: id.to_string(),
            uri: None,
        },
        name: name.to_string(),
        art: ImageSet::default(),
        genres: vec![],
        popularity: None,
        follower_count: None,
        bio: None,
        following: None,
        url: None,
    }
}

#[cfg(test)]
pub fn make_playlist(id: &str, title: &str) -> Playlist {
    Playlist {
        rri: ResourceId {
            provider: Provider::Spotify,
            id: id.to_string(),
            uri: None,
        },
        title: title.to_string(),
        description: None,
        owner: None,
        art: ImageSet::default(),
        total_tracks: None,
        collaborative: None,
        public: None,
        version: None,
        tracks: None,
        following: None,
        url: None,
    }
}
