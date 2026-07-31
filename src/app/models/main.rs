use std::{
    hash::{Hash, Hasher},
    str::FromStr,
};

/// A set of image URLs at different sizes from Spotify.
///
/// An `ImageSet` is guaranteed to contain at least one image. Construction
/// via [`ImageSet::from_images`] returns `None` when no valid (non-empty URL)
/// images are provided, so any existing `ImageSet` value is always non-empty.
#[derive(Clone, Debug, Default)]
pub struct ImageSet {
    images: Vec<(u32, String)>, // (width, url) sorted by width ascending
}

impl ImageSet {
    /// Construct an `ImageSet` from an iterator of (width, url) pairs.
    ///
    /// Returns `None` if no valid images are provided (all URLs empty or
    /// iterator is empty). A `Some` value is guaranteed to contain at least
    /// one image, so `best_for_width` will always return `Some`.
    pub fn from_images(images: impl IntoIterator<Item = (Option<u32>, String)>) -> Option<Self> {
        let mut imgs: Vec<(u32, String)> = images
            .into_iter()
            .filter(|(_, url)| !url.is_empty())
            .map(|(w, url)| (w.unwrap_or(0), url))
            .collect();
        if imgs.is_empty() {
            return None;
        }
        imgs.sort_by_key(|(w, _)| *w);
        Some(Self { images: imgs })
    }

    /// Pick the image URL closest to (but preferring >=) the given width.
    ///
    /// Always returns `Some` on a properly constructed `ImageSet` (i.e. one
    /// obtained from `from_images`). Returns `None` only on a `Default` instance.
    pub fn best_for_width(&self, width: u32) -> Option<&str> {
        self.images
            .iter()
            .find(|(w, _)| *w >= width)
            .or_else(|| self.images.last())
            .map(|(_, url)| url.as_str())
    }

    /// Get the largest available image URL.
    pub fn largest(&self) -> Option<&str> {
        self.images.last().map(|(_, url)| url.as_str())
    }
}

// A batch of whatever
#[derive(Clone, Copy, Debug, Default)]
pub struct Batch {
    // What offset does the batch start at
    pub offset: usize,
    // How many elements
    pub batch_size: usize,
    // Total number of elements if we had all batches
    pub total: usize,
}

impl Batch {
    pub fn first_of_size(batch_size: usize) -> Self {
        Self {
            offset: 0,
            batch_size,
            total: 0,
        }
    }

    pub fn next(self) -> Option<Self> {
        let Self {
            offset,
            batch_size,
            total,
        } = self;

        Some(Self {
            offset: offset + batch_size,
            batch_size,
            total,
        })
        .filter(|b| b.offset < total)
    }
}

// "Something"Ref models usually boil down to an ID/url + a display name

#[derive(Clone, Debug)]
pub struct UserRef {
    pub id: String,
    pub display_name: String,
}

#[derive(Clone, Debug)]
pub struct ArtistRef {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct AlbumRef {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct SearchResults {
    pub albums: Vec<AlbumDescription>,
    pub artists: Vec<ArtistSummary>,
    pub playlists: Vec<PlaylistDescription>,
    pub tracks: SongBatch,
}

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

impl SearchType {
    /// The Spotify search API `type` query value.
    pub fn spotify_type(self) -> &'static str {
        match self {
            Self::Artists => "artist",
            Self::Albums => "album",
            Self::Playlists => "playlist",
            Self::Tracks => "track",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AlbumDescription {
    pub id: String,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub release_date: Option<String>,
    pub art: Option<ImageSet>,
    pub songs: SongBatch,
    pub is_liked: bool,
    pub popularity: u32,
    pub album_type: Option<String>,
}

impl AlbumDescription {
    pub fn artists_name(&self) -> String {
        self.artists
            .iter()
            .map(|a| a.name.to_string())
            .collect::<Vec<String>>()
            .join(", ")
    }

    pub fn year(&self) -> Option<u32> {
        self.release_date
            .as_ref()
            .and_then(|date| date.split('-').next())
            .and_then(|y| u32::from_str(y).ok())
    }
}

#[derive(Clone, Debug)]
pub struct AlbumFullDescription {
    pub description: AlbumDescription,
    pub release_details: AlbumReleaseDetails,
}

#[derive(Clone, Debug)]
pub struct AlbumReleaseDetails {
    pub label: String,
    pub copyright_text: String,
    pub total_tracks: usize,
}

#[derive(Clone, Debug)]
pub struct PlaylistDescription {
    pub id: String,
    pub title: String,
    pub art: Option<ImageSet>,
    pub songs: SongBatch,
    pub owner: UserRef,
}

#[derive(Clone, Copy, Debug)]
pub enum ConnectDeviceKind {
    Phone,
    Computer,
    Speaker,
    Other,
}

#[derive(Clone, Debug)]
pub struct ConnectDevice {
    pub id: String,
    pub label: String,
    pub kind: ConnectDeviceKind,
}

#[derive(Clone, Debug)]
pub struct PlaylistSummary {
    pub id: String,
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct SongDescription {
    pub id: String,
    pub track_number: Option<u32>,
    pub uri: String,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub album: AlbumRef,
    pub duration_ms: u32,
    pub art: Option<ImageSet>,
    pub explicit: bool,
    pub playable: bool,
}

impl SongDescription {
    pub fn artists_name(&self) -> String {
        self.artists
            .iter()
            .map(|a| a.name.to_string())
            .collect::<Vec<String>>()
            .join(", ")
    }
}

impl Hash for SongDescription {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[derive(Copy, Clone, Default)]
pub struct SongState {
    pub is_playing: bool,
    pub is_selected: bool,
    pub is_liked: bool,
    pub is_explicit_filtered: bool,
}

// A batch of SONGS
#[derive(Debug, Clone, Default)]
pub struct SongBatch {
    pub songs: Vec<SongDescription>,
    pub batch: Batch,
}

impl SongBatch {
    pub fn empty() -> Self {
        Self {
            songs: vec![],
            batch: Batch::first_of_size(1),
        }
    }

    pub fn resize(self, batch_size: usize) -> Vec<Self> {
        let SongBatch { mut songs, batch } = self;
        // Growing a batch is easy...
        if batch_size > batch.batch_size {
            let new_batch = Batch {
                batch_size,
                ..batch
            };
            vec![Self {
                songs,
                batch: new_batch,
            }]
        // Shrinking is not!
        // We have to split the batch in multiple batches
        } else {
            let n = songs.len();
            let iter_count = n.div_ceil(batch_size);
            (0..iter_count)
                .map(|i| {
                    let offset = batch.offset + i * batch_size;
                    let new_batch = Batch {
                        offset,
                        total: batch.total,
                        batch_size,
                    };
                    let drain_upper = usize::min(batch_size, songs.len());
                    let new_songs = songs.drain(0..drain_upper).collect();
                    Self {
                        songs: new_songs,
                        batch: new_batch,
                    }
                })
                .collect()
        }
    }
}

#[derive(Clone, Debug)]
pub struct ArtistDescription {
    pub id: String,
    pub name: String,
    pub photo: Option<ImageSet>,
    pub albums: Vec<AlbumDescription>,
    pub top_tracks: Vec<SongDescription>,
    pub is_followed: bool,
}

#[derive(Clone, Debug)]
pub struct ArtistSummary {
    pub id: String,
    pub name: String,
    pub photo: Option<ImageSet>,
    pub popularity: u32,
}

#[derive(Clone, Debug)]
pub struct UserDescription {
    pub id: String,
    pub name: String,
    pub photo: Option<ImageSet>,
    pub playlists: Vec<PlaylistDescription>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatMode {
    Song,
    Playlist,
    None,
}

#[derive(Clone, Debug)]
pub struct ConnectPlayerState {
    pub is_playing: bool,
    pub current_song_id: Option<String>,
    pub progress_ms: u32,
    pub repeat: RepeatMode,
    pub shuffle: bool,
}

impl Default for ConnectPlayerState {
    fn default() -> Self {
        Self {
            is_playing: false,
            current_song_id: None,
            progress_ms: 0,
            repeat: RepeatMode::None,
            shuffle: false,
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn song(id: &str) -> SongDescription {
        SongDescription {
            id: id.to_string(),
            uri: "".to_string(),
            title: "Title".to_string(),
            artists: vec![],
            album: AlbumRef {
                id: "".to_string(),
                name: "".to_string(),
            },
            duration_ms: 1000,
            art: None,
            track_number: None,
            explicit: false,
            playable: true,
        }
    }

    #[test]
    fn resize_batch() {
        let batch = SongBatch {
            songs: vec![song("1"), song("2"), song("3"), song("4")],
            batch: Batch::first_of_size(4),
        };

        let batches = batch.resize(2);
        assert_eq!(batches.len(), 2);
        assert_eq!(&batches.get(0).unwrap().songs.get(0).unwrap().id, "1");
        assert_eq!(&batches.get(1).unwrap().songs.get(0).unwrap().id, "3");
    }
}
