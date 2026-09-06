//! Conversions from generated Spotify types into provider-neutral [`crate::models`].

use spotify_api::models as sp;

use crate::defaults::{self, EntityKind};
use crate::models::*;
use crate::providers::{album_type_from, content_rating_from_explicit, parse_release_date};

pub(super) fn rid(id: impl Into<String>) -> ResourceId {
    ResourceId {
        provider: Provider::Spotify,
        id: id.into(),
        uri: None,
    }
}

pub(super) fn ensure_artists(mut artists: Vec<ArtistRef>) -> Vec<ArtistRef> {
    if artists.is_empty() {
        artists.push(ArtistRef::unknown());
    }
    artists
}

pub(super) fn text_or(value: String, fallback: impl FnOnce() -> String) -> String {
    if value.trim().is_empty() {
        fallback()
    } else {
        value
    }
}

fn images_to_set(images: &[sp::ImageObject]) -> Option<ImageSet> {
    crate::providers::images_to_set(images.iter().map(|i| {
        (
            i.url.clone(),
            i.width.map(|w| w as u32),
            i.height.map(|h| h as u32),
        )
    }))
}

pub(super) fn to_artist_ref(a: &sp::SimplifiedArtistObject) -> ArtistRef {
    ArtistRef {
        rri: rid(a.id.clone().unwrap_or_default()),
        name: a.name.clone().unwrap_or_default(),
    }
}

pub(super) fn album_from_object(a: &sp::AlbumObject) -> Album {
    let copyright = a
        .copyrights
        .iter()
        .map(|c| c.text.clone().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("; ");
    Album {
        rri: rid(a.id.clone()),
        title: text_or(a.name.clone(), defaults::album_title),
        artists: ensure_artists(a.artists.iter().map(to_artist_ref).collect()),
        art: images_to_set(&a.images).unwrap_or_else(|| defaults::image_set_for(EntityKind::Album)),
        album_type: album_type_from(&a.album_type).unwrap_or(AlbumType::Album),
        release_date: parse_release_date(&a.release_date),
        total_tracks: Some(a.total_tracks as u32),
        label: Some(a.label.clone()),
        copyright: Some(copyright),
        upc: None,
        genres: Vec::new(),
        popularity: Some(a.popularity as u32),
        content_rating: ContentRating::None,
        saved: None,
        tracks: None,
        url: None,
    }
}

pub(super) fn album_from_discography(a: &sp::ArtistDiscographyAlbumObject) -> Album {
    Album {
        rri: rid(a.id.clone()),
        title: text_or(a.name.clone(), defaults::album_title),
        artists: ensure_artists(a.artists.iter().map(to_artist_ref).collect()),
        art: images_to_set(&a.images).unwrap_or_else(|| defaults::image_set_for(EntityKind::Album)),
        album_type: album_type_from(&a.album_type).unwrap_or(AlbumType::Album),
        release_date: parse_release_date(&a.release_date),
        total_tracks: None,
        label: None,
        copyright: None,
        upc: None,
        genres: Vec::new(),
        popularity: None,
        content_rating: ContentRating::None,
        saved: None,
        tracks: None,
        url: None,
    }
}

pub(super) fn album_from_simplified(a: &sp::SimplifiedAlbumObject) -> Album {
    Album {
        rri: rid(a.id.clone()),
        title: text_or(a.name.clone(), defaults::album_title),
        artists: ensure_artists(a.artists.iter().map(to_artist_ref).collect()),
        art: images_to_set(&a.images).unwrap_or_else(|| defaults::image_set_for(EntityKind::Album)),
        album_type: album_type_from(&a.album_type).unwrap_or(AlbumType::Album),
        release_date: parse_release_date(&a.release_date),
        total_tracks: None,
        label: None,
        copyright: None,
        upc: None,
        genres: Vec::new(),
        popularity: None,
        content_rating: ContentRating::None,
        saved: None,
        tracks: None,
        url: None,
    }
}

pub(super) fn album_from_saved(sa: &sp::SavedAlbumObject) -> Option<Album> {
    sa.album.as_ref().map(|a| album_from_object(a.as_ref()))
}

pub(super) fn track_from_object(t: &sp::TrackObject) -> Option<Track> {
    let id = t.id.clone()?;
    Some(Track {
        rri: ResourceId {
            provider: Provider::Spotify,
            id,
            uri: t.uri.clone(),
        },
        title: text_or(t.name.clone().unwrap_or_default(), defaults::track_title),
        artists: ensure_artists(
            t.artists
                .as_ref()
                .map(|a| a.iter().map(to_artist_ref).collect())
                .unwrap_or_default(),
        ),
        album: t.album.as_ref().map(|a| AlbumRef {
            rri: rid(a.id.clone()),
            name: a.name.clone(),
        }),
        duration_ms: t.duration_ms.unwrap_or(0) as u32,
        track_number: t.track_number.map(|n| n as u32),
        disc_number: None,
        content_rating: content_rating_from_explicit(t.explicit),
        isrc: None,
        art: t
            .album
            .as_ref()
            .and_then(|a| images_to_set(&a.images))
            .unwrap_or_else(|| defaults::image_set_for(EntityKind::Track)),
        playable: t.is_playable.unwrap_or(true),
        popularity: None,
        saved: None,
        preview_url: None,
        url: None,
    })
}

pub(super) fn playlist_from_object(p: &sp::PlaylistObject) -> Playlist {
    Playlist {
        rri: rid(p.id.clone().unwrap_or_default()),
        title: text_or(p.name.clone().unwrap_or_default(), defaults::playlist_title),
        description: None,
        owner: p.owner.as_ref().map(|o| UserRef {
            rri: rid(o.id.clone().unwrap_or_default()),
            display_name: o.display_name.clone().flatten().unwrap_or_default(),
        }),
        art: p
            .images
            .as_ref()
            .and_then(|imgs| images_to_set(imgs))
            .unwrap_or_else(|| defaults::image_set_for(EntityKind::Playlist)),
        total_tracks: Some(p.tracks.as_ref().map(|t| t.total as u32).unwrap_or(0)),
        collaborative: None,
        public: None,
        version: None,
        tracks: None,
        following: None,
        url: None,
    }
}

pub(super) fn playlist_from_simplified(p: &sp::SimplifiedPlaylistObject) -> Playlist {
    Playlist {
        rri: rid(p.id.clone().unwrap_or_default()),
        title: text_or(p.name.clone().unwrap_or_default(), defaults::playlist_title),
        description: None,
        owner: p.owner.as_ref().map(|o| UserRef {
            rri: rid(o.id.clone().unwrap_or_default()),
            display_name: o.display_name.clone().flatten().unwrap_or_default(),
        }),
        art: p
            .images
            .as_ref()
            .and_then(|imgs| images_to_set(imgs))
            .unwrap_or_else(|| defaults::image_set_for(EntityKind::Playlist)),
        total_tracks: Some(p.tracks.as_ref().and_then(|t| t.total).unwrap_or(0) as u32),
        collaborative: None,
        public: None,
        version: None,
        tracks: None,
        following: None,
        url: None,
    }
}

pub(super) fn artist_from_object(a: &sp::ArtistObject) -> Artist {
    Artist {
        rri: rid(a.id.clone().unwrap_or_default()),
        name: text_or(a.name.clone().unwrap_or_default(), defaults::artist_name),
        art: a
            .images
            .as_ref()
            .and_then(|imgs| images_to_set(imgs))
            .unwrap_or_else(|| defaults::image_set_for(EntityKind::Artist)),
        genres: Vec::new(),
        popularity: a.popularity.map(|p| p as u32),
        follower_count: None,
        bio: None,
        following: None,
        url: None,
    }
}

pub(super) fn user_from_object(u: &sp::PublicUserObject) -> User {
    User {
        rri: rid(u.id.clone().unwrap_or_default()),
        display_name: text_or(
            u.display_name.clone().flatten().unwrap_or_default(),
            defaults::user_name,
        ),
        art: u
            .images
            .as_ref()
            .and_then(|imgs| images_to_set(imgs))
            .unwrap_or_else(|| defaults::image_set_for(EntityKind::User)),
        follower_count: None,
        product: None,
        country: None,
        explicit_filter: None,
        url: None,
    }
}

pub(super) fn current_user_from_private(u: &sp::PrivateUserObject) -> User {
    User {
        rri: rid(u.id.clone().unwrap_or_default()),
        display_name: text_or(
            u.display_name.clone().unwrap_or_default(),
            defaults::user_name,
        ),
        art: u
            .images
            .as_ref()
            .and_then(|imgs| images_to_set(imgs))
            .unwrap_or_else(|| defaults::image_set_for(EntityKind::User)),
        follower_count: None,
        product: u.product.clone(),
        country: None,
        explicit_filter: u
            .explicit_content
            .as_ref()
            .map(|e| ExplicitContentSettings {
                filter_enabled: e.filter_enabled.unwrap_or(false),
                filter_locked: e.filter_locked.unwrap_or(false),
            }),
        url: None,
    }
}

pub(super) fn device_from_object(d: &sp::DeviceObject) -> Option<Device> {
    if d.is_restricted.unwrap_or(false) {
        return None;
    }
    let kind = match d.r#type.as_deref() {
        Some("Smartphone") => DeviceKind::Phone,
        Some("Computer") => DeviceKind::Computer,
        Some("Speaker") => DeviceKind::Speaker,
        Some("TV") | Some("CastVideo") => DeviceKind::Tv,
        _ => DeviceKind::Other,
    };
    Some(Device {
        id: d.id.clone().flatten().unwrap_or_default(),
        label: d.name.clone().unwrap_or_default(),
        kind,
        is_active: d.is_active.unwrap_or(false),
        volume_percent: None,
    })
}

pub(super) fn player_state_from_object(s: &sp::CurrentlyPlayingContextObject) -> PlayerState {
    let current_track_id = s
        .item
        .as_ref()
        .and_then(|item| match item.as_ref() {
            sp::QueueObjectCurrentlyPlaying::Track(t) => t.id.clone(),
            _ => None,
        })
        .map(rid);
    let repeat = match s.repeat_state.as_deref() {
        Some("track") => RepeatMode::Track,
        Some("context") => RepeatMode::Context,
        _ => RepeatMode::Off,
    };
    PlayerState {
        is_playing: s.is_playing.unwrap_or(false),
        current_track_id,
        progress_ms: s.progress_ms.unwrap_or(0) as u32,
        repeat,
        shuffle: s.shuffle_state.unwrap_or(false),
        device: None,
    }
}

pub(super) fn repeat_mode_str(mode: RepeatMode) -> &'static str {
    match mode {
        RepeatMode::Track => "track",
        RepeatMode::Context => "context",
        RepeatMode::Off => "off",
    }
}

pub(super) fn search_type_str(kind: SearchType) -> &'static str {
    match kind {
        SearchType::Artist => "artist",
        SearchType::Album => "album",
        SearchType::Playlist => "playlist",
        SearchType::Track => "track",
    }
}

pub(super) fn queue_from_object(q: &sp::QueueObject) -> Queue {
    let currently_playing = q
        .currently_playing
        .as_ref()
        .and_then(|cp| match cp.as_ref() {
            sp::QueueObjectCurrentlyPlaying::Track(t) => track_from_object(t),
            _ => None,
        });
    let items = q
        .queue
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter_map(|item| match item {
            sp::QueueObjectQueueInner::Track(t) => track_from_object(t),
            _ => None,
        })
        .collect();
    Queue {
        currently_playing,
        items,
    }
}

pub(super) fn search_results_from_response(s: &sp::Search200Response) -> SearchResults {
    let albums = s
        .albums
        .as_ref()
        .map(|p| Page {
            items: p.items.iter().map(album_from_simplified).collect(),
            offset: Some(p.offset as usize),
            total: Some(p.total as usize),
            next_cursor: None,
        })
        .unwrap_or_default();
    let artists = s
        .artists
        .as_ref()
        .map(|p| Page {
            items: p.items.iter().map(artist_from_object).collect(),
            offset: Some(p.offset as usize),
            total: Some(p.total as usize),
            next_cursor: None,
        })
        .unwrap_or_default();
    let tracks = s
        .tracks
        .as_ref()
        .map(|p| Page {
            items: p.items.iter().filter_map(track_from_object).collect(),
            offset: Some(p.offset as usize),
            total: Some(p.total as usize),
            next_cursor: None,
        })
        .unwrap_or_default();
    let playlists = s
        .playlists
        .as_ref()
        .map(|p| Page {
            items: p.items.iter().map(playlist_from_simplified).collect(),
            offset: Some(p.offset as usize),
            total: Some(p.total as usize),
            next_cursor: None,
        })
        .unwrap_or_default();
    SearchResults {
        albums,
        artists,
        tracks,
        playlists,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_queue_converts_to_empty() {
        let q = queue_from_object(&sp::QueueObject {
            currently_playing: None,
            queue: None,
        });
        assert!(q.currently_playing.is_none());
        assert!(q.items.is_empty());
    }

    #[test]
    fn empty_album_resolves_to_defaults() {
        let album = album_from_object(&sp::AlbumObject::default());

        assert_eq!(album.title, "Untitled Album");
        assert!(album.art.is_resource());
        assert_eq!(
            album.art.largest(),
            Some("resource:///dev/diegovsky/Riff/defaults/album-default.svg")
        );
        assert_eq!(album.artists.len(), 1);
        assert_eq!(album.artists[0].name, "Unknown Artist");
    }
}
