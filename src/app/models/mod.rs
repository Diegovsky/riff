// Domain models
mod main;
pub use main::*;

// Shared enums (used by UI, state, and settings)
mod card_enums;
pub use card_enums::*;

// UI models (GObject)
mod songs;
pub use songs::*;

mod card_model;
pub use card_model::*;

impl From<&AlbumDescription> for CardModel {
    fn from(album: &AlbumDescription) -> Self {
        let art = album.art.as_ref()
            .and_then(|s| s.best_for_width(200))
            .map(str::to_owned);
        let release_date = album.release_date.as_deref().unwrap_or("");
        glib::Object::builder()
            .property("id", &album.id)
            .property("image", &art)
            .property("title", &album.title)
            .property("subtitle", &album.artists_name())
            .property("release-date", release_date)
            .property("popularity", album.popularity)
            .build()
    }
}

impl From<AlbumDescription> for CardModel {
    fn from(album: AlbumDescription) -> Self {
        Self::from(&album)
    }
}

impl From<&PlaylistDescription> for CardModel {
    fn from(playlist: &PlaylistDescription) -> Self {
        let art = playlist.art.as_ref()
            .and_then(|s| s.best_for_width(200))
            .map(str::to_owned);
        CardModel::new(&playlist.id, art.as_ref(), &playlist.title, &playlist.owner.display_name)
    }
}

impl From<PlaylistDescription> for PlaylistSummary {
    fn from(PlaylistDescription { id, title, .. }: PlaylistDescription) -> Self {
        Self { id, title }
    }
}

impl From<PlaylistDescription> for CardModel {
    fn from(playlist: PlaylistDescription) -> Self {
        Self::from(&playlist)
    }
}

impl From<SongDescription> for SongModel {
    fn from(song: SongDescription) -> Self {
        SongModel::new(song)
    }
}

impl From<&SongDescription> for SongModel {
    fn from(song: &SongDescription) -> Self {
        SongModel::new(song.clone())
    }
}

impl From<&ArtistSummary> for CardModel {
    fn from(artist: &ArtistSummary) -> Self {
        let photo = artist.photo.as_ref()
            .and_then(|s| s.best_for_width(200))
            .map(str::to_owned);
        glib::Object::builder()
            .property("id", &artist.id)
            .property("image", &photo)
            .property("title", &artist.name)
            .property("subtitle", "")
            .property("popularity", artist.popularity)
            .build()
    }
}
