// Domain models
mod main;
pub use main::*;

// Playback queue conversions
mod queue;
pub use queue::*;

// Shared enums (used by UI, state, and settings)
mod card_enums;
pub use card_enums::*;

// UI models (GObject)
mod songs;
pub use songs::*;

mod card_model;
pub use card_model::*;

use crate::app::components::card::IMAGE_SIZE;

impl From<&Album> for CardModel {
    fn from(album: &Album) -> Self {
        let art = album.art.best_for_width(IMAGE_SIZE).map(str::to_owned);
        CardModel::new(
            &album.rri.id,
            art.as_ref(),
            &album.title,
            &album.artists_name(),
            album.release_date_string().as_deref(),
            Some(album.popularity.unwrap_or(0)),
            None,
            album.album_type_string().as_deref(),
        )
    }
}

impl From<Album> for CardModel {
    fn from(album: Album) -> Self {
        Self::from(&album)
    }
}

impl From<&Playlist> for CardModel {
    fn from(playlist: &Playlist) -> Self {
        let art = playlist.art.best_for_width(IMAGE_SIZE).map(str::to_owned);
        let owner = playlist
            .owner
            .as_ref()
            .map(|o| o.display_name.as_str())
            .unwrap_or("");
        CardModel::new(
            &playlist.rri.id,
            art.as_ref(),
            &playlist.title,
            owner,
            None,
            None,
            None,
            None,
        )
    }
}

impl From<Playlist> for PlaylistSummary {
    fn from(playlist: Playlist) -> Self {
        Self {
            id: playlist.rri.id,
            title: playlist.title,
        }
    }
}

impl From<Playlist> for CardModel {
    fn from(playlist: Playlist) -> Self {
        Self::from(&playlist)
    }
}

impl From<Track> for SongModel {
    fn from(song: Track) -> Self {
        SongModel::new(song)
    }
}

impl From<&Track> for SongModel {
    fn from(song: &Track) -> Self {
        SongModel::new(song.clone())
    }
}

impl From<&Artist> for CardModel {
    fn from(artist: &Artist) -> Self {
        let photo = artist.art.best_for_width(IMAGE_SIZE).map(str::to_owned);
        CardModel::new(
            &artist.rri.id,
            photo.as_ref(),
            &artist.name,
            "",
            None,
            Some(artist.popularity.unwrap_or(0)),
            None,
            None,
        )
    }
}

impl From<&Track> for CardModel {
    fn from(desc: &Track) -> Self {
        let photo = desc.art.best_for_width(IMAGE_SIZE).map(str::to_owned);
        CardModel::new(
            &desc.rri.id,
            photo.as_ref(),
            &desc.title,
            &desc.artists_name(),
            None,
            None,
            None,
            None,
        )
    }
}
