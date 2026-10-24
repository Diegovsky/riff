//! Default values for display-critical fields (artwork, titles, artist names).
//!
//! Every provider's conversion boundary resolves missing fields here so domain
//! models always carry usable values.

use gettextrs::gettext;

use crate::models::ImageSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKind {
    Album,
    Artist,
    Playlist,
    Track,
    User,
}

impl EntityKind {
    fn resource_url(self) -> &'static str {
        match self {
            EntityKind::Album => "resource:///dev/diegovsky/Riff/defaults/album-default.svg",
            EntityKind::Artist => "resource:///dev/diegovsky/Riff/defaults/artist-default.svg",
            EntityKind::Playlist => "resource:///dev/diegovsky/Riff/defaults/playlist-default.svg",
            EntityKind::Track => "resource:///dev/diegovsky/Riff/defaults/track-default.svg",
            EntityKind::User => "resource:///dev/diegovsky/Riff/defaults/user-default.svg",
        }
    }
}

pub fn image_set_for(kind: EntityKind) -> ImageSet {
    ImageSet::from_resource(kind.resource_url())
}

pub fn album_title() -> String {
    gettext("Untitled Album")
}

pub fn playlist_title() -> String {
    gettext("Untitled Playlist")
}

pub fn track_title() -> String {
    gettext("Untitled")
}

pub fn artist_name() -> String {
    gettext("Unknown Artist")
}

pub fn user_name() -> String {
    gettext("Unknown User")
}
