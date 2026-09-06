//! Caching layer: disk TTL, texture LRU, domain object LRU.

use gdk_pixbuf::prelude::PixbufLoaderExt;
use serde::{Deserialize, Serialize};

pub mod disk;
pub(crate) mod lru;
pub mod store;
pub mod texture;

pub use disk::DiskCache;
pub use store::Store;
pub use texture::TextureCache;

/// Key identifying a cached resource.
#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum CacheKey {
    Album(String),
    Playlist(String),
    Artist(String),
    ArtistTopTracks(String),
    User(String),
    SavedAlbums,
    SavedTracks,
    SavedPlaylists,
    AlbumTracks(String),
    PlaylistTracks(String),
    ArtistAlbums(String),
    UserPlaylists(String),
}

impl CacheKey {
    pub fn disk_key(&self) -> String {
        match self {
            Self::Album(id) => format!("album_{id}"),
            Self::Playlist(id) => format!("playlist_{id}"),
            Self::Artist(id) => format!("artist_{id}"),
            Self::ArtistTopTracks(id) => format!("artist_top_{id}"),
            Self::User(id) => format!("user_{id}"),
            Self::SavedAlbums => "pg_saved_albums".into(),
            Self::SavedTracks => "pg_saved_tracks".into(),
            Self::SavedPlaylists => "pg_saved_playlists".into(),
            Self::AlbumTracks(id) => format!("pg_album_tracks_{id}"),
            Self::PlaylistTracks(id) => format!("pg_playlist_tracks_{id}"),
            Self::ArtistAlbums(id) => format!("pg_artist_albums_{id}"),
            Self::UserPlaylists(id) => format!("pg_user_playlists_{id}"),
        }
    }
}

pub fn decode_texture(data: &[u8], width: i32, height: i32) -> Option<gdk::Texture> {
    let loader = gdk_pixbuf::PixbufLoader::new();
    loader.set_size(width, height);
    loader.write(data).ok()?;
    loader.close().ok()?;
    let pixbuf = loader.pixbuf()?;
    Some(gdk::Texture::for_pixbuf(&pixbuf))
}
