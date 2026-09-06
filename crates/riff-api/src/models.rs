//! Provider-neutral music API models.
//!
//! Common-denominator types shared across Spotify, Apple Music, TIDAL, etc.
//! Intentionally decoupled from any single provider's schema.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    #[default]
    Unknown,
    Spotify,
    AppleMusic,
    Tidal,
    YouTubeMusic,
    Other(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceId {
    pub provider: Provider,
    pub id: String,
    pub uri: Option<String>,
}

impl ResourceId {
    pub fn new(provider: Provider, id: impl Into<String>) -> Self {
        Self {
            provider,
            id: id.into(),
            uri: None,
        }
    }

    pub fn with_uri(provider: Provider, id: impl Into<String>, uri: impl Into<String>) -> Self {
        Self {
            provider,
            id: id.into(),
            uri: Some(uri.into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    pub url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageSet {
    pub images: Vec<Image>,
    pub template: Option<String>,
}

impl ImageSet {
    pub fn best_for_width(&self, width: u32) -> Option<&str> {
        self.images
            .iter()
            .filter(|i| i.width.is_some_and(|w| w >= width))
            .min_by_key(|i| i.width.unwrap_or(u32::MAX))
            .or_else(|| self.images.iter().max_by_key(|i| i.width.unwrap_or(0)))
            .map(|i| i.url.as_str())
    }

    pub fn largest(&self) -> Option<&str> {
        self.images
            .iter()
            .max_by_key(|i| i.width.unwrap_or(0))
            .map(|i| i.url.as_str())
    }

    pub fn from_resource(resource_url: impl Into<String>) -> Self {
        ImageSet {
            images: vec![Image {
                url: resource_url.into(),
                width: None,
                height: None,
            }],
            template: None,
        }
    }

    pub fn is_resource(&self) -> bool {
        self.images
            .first()
            .is_some_and(|i| i.url.starts_with("resource://"))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentRating {
    #[default]
    None,
    Clean,
    Explicit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseDate {
    pub year: i32,
    pub month: Option<u8>,
    pub day: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub offset: Option<usize>,
    pub total: Option<usize>,
    pub next_cursor: Option<String>,
}

impl<T> Default for Page<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            offset: None,
            total: None,
            next_cursor: None,
        }
    }
}

impl<T> Page<T> {
    pub fn offset_paged(items: Vec<T>, offset: usize, total: usize) -> Self {
        Self {
            items,
            offset: Some(offset),
            total: Some(total),
            next_cursor: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtistRef {
    pub rri: ResourceId,
    pub name: String,
}

impl ArtistRef {
    pub fn unknown() -> Self {
        ArtistRef {
            rri: ResourceId::new(Provider::Other(String::new()), String::new()),
            name: gettextrs::gettext("Unknown Artist"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumRef {
    pub rri: ResourceId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserRef {
    pub rri: ResourceId,
    pub display_name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artist {
    pub rri: ResourceId,
    pub name: String,
    pub art: ImageSet,
    pub genres: Vec<String>,
    pub popularity: Option<u32>,
    pub follower_count: Option<u64>,
    pub bio: Option<String>,
    pub following: Option<bool>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlbumType {
    Album,
    Single,
    Ep,
    Compilation,
    Live,
    Soundtrack,
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Album {
    pub rri: ResourceId,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub art: ImageSet,
    pub album_type: AlbumType,
    pub release_date: Option<ReleaseDate>,
    pub total_tracks: Option<u32>,
    pub label: Option<String>,
    pub copyright: Option<String>,
    pub upc: Option<String>,
    pub genres: Vec<String>,
    pub popularity: Option<u32>,
    pub content_rating: ContentRating,
    pub saved: Option<bool>,
    pub tracks: Option<Page<Track>>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub rri: ResourceId,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub album: Option<AlbumRef>,
    pub duration_ms: u32,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub content_rating: ContentRating,
    pub isrc: Option<String>,
    pub art: ImageSet,
    pub playable: bool,
    pub popularity: Option<u32>,
    pub saved: Option<bool>,
    pub preview_url: Option<String>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub rri: ResourceId,
    pub title: String,
    pub description: Option<String>,
    pub owner: Option<UserRef>,
    pub art: ImageSet,
    pub total_tracks: Option<u32>,
    pub collaborative: Option<bool>,
    pub public: Option<bool>,
    pub version: Option<String>,
    pub tracks: Option<Page<PlaylistItem>>,
    pub following: Option<bool>,
    pub url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplicitContentSettings {
    pub filter_enabled: bool,
    pub filter_locked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub rri: ResourceId,
    pub display_name: String,
    pub art: ImageSet,
    pub follower_count: Option<u64>,
    pub product: Option<String>,
    pub country: Option<String>,
    pub explicit_filter: Option<ExplicitContentSettings>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistItem {
    pub track: Track,
    pub added_at: Option<String>,
    pub added_by: Option<UserRef>,
    pub position: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedItem<T> {
    pub item: T,
    pub added_at: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchType {
    Track,
    Album,
    Artist,
    Playlist,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResults {
    pub tracks: Page<Track>,
    pub albums: Page<Album>,
    pub artists: Page<Artist>,
    pub playlists: Page<Playlist>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceKind {
    Phone,
    Computer,
    Speaker,
    Tv,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub label: String,
    pub kind: DeviceKind,
    pub is_active: bool,
    pub volume_percent: Option<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepeatMode {
    #[default]
    Off,
    Track,
    Context,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerState {
    pub is_playing: bool,
    pub current_track_id: Option<ResourceId>,
    pub progress_ms: u32,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    pub device: Option<Device>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queue {
    pub currently_playing: Option<Track>,
    pub items: Vec<Track>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32) -> Image {
        Image {
            url: format!("img{w}"),
            width: Some(w),
            height: Some(w),
        }
    }

    #[test]
    fn best_for_width_prefers_smallest_at_least_requested() {
        let set = ImageSet {
            images: vec![img(64), img(300), img(640)],
            template: None,
        };
        assert_eq!(set.best_for_width(100), Some("img300"));
        assert_eq!(set.best_for_width(64), Some("img64"));
        // Nothing large enough falls back to the largest.
        assert_eq!(set.best_for_width(5000), Some("img640"));
        assert_eq!(set.largest(), Some("img640"));
    }

    #[test]
    fn empty_image_set_returns_none() {
        let set = ImageSet::default();
        assert_eq!(set.best_for_width(100), None);
        assert_eq!(set.largest(), None);
    }

    #[test]
    fn content_rating_defaults_to_none() {
        assert_eq!(ContentRating::default(), ContentRating::None);
    }

    #[test]
    fn page_default_is_empty() {
        let page: Page<Track> = Page::default();
        assert!(page.items.is_empty());
        assert!(page.offset.is_none());
        assert!(page.total.is_none());
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn resource_image_set_is_flagged_and_readable() {
        let set =
            ImageSet::from_resource("resource:///dev/diegovsky/Riff/defaults/album-default.svg");
        assert!(set.is_resource());
        // A resource set has a single, width-less image, so the size helpers
        // still return its URL.
        assert_eq!(
            set.largest(),
            Some("resource:///dev/diegovsky/Riff/defaults/album-default.svg")
        );
        assert_eq!(
            set.best_for_width(180),
            Some("resource:///dev/diegovsky/Riff/defaults/album-default.svg")
        );
    }

    #[test]
    fn remote_image_set_is_not_a_resource() {
        let set = ImageSet {
            images: vec![img(300)],
            template: None,
        };
        assert!(!set.is_resource());
        assert!(!ImageSet::default().is_resource());
    }

    #[test]
    fn unknown_artist_ref_has_empty_id_and_fallback_name() {
        let a = ArtistRef::unknown();
        assert!(a.rri.id.is_empty());
        // gettext with no bound catalog returns the source string.
        assert_eq!(a.name, "Unknown Artist");
    }
}
