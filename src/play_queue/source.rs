/// Where a list of tracks comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SongsSource {
    Playlist(String),
    Album(String),
    Artist(String),
    SavedTracks,
    /// Search results, by query.
    Search(String),
}

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

    pub fn spotify_url(&self) -> Option<String> {
        let (kind, id) = match self {
            Self::Playlist(id) => ("playlist", id),
            Self::Album(id) => ("album", id),
            Self::Artist(id) => ("artist", id),
            Self::SavedTracks | Self::Search(_) => return None,
        };
        Some(format!("https://open.spotify.com/{}/{}", kind, id))
    }

    pub fn is_paginated(&self) -> bool {
        matches!(self, Self::Playlist(_) | Self::Album(_) | Self::SavedTracks)
    }
}

/// The next page to fetch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageRequest {
    pub offset: usize,
    pub batch_size: usize,
}
