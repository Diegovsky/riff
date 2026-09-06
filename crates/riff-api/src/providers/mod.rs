//! Music vendor implementations.
//!
//! Each submodule binds one streaming service and produces the provider-neutral
//! types from [`crate::models`].

mod provider;
pub use provider::MusicProvider;

pub mod spotify;

use crate::models::{AlbumType, ContentRating, Image, ImageSet, ReleaseDate};

/// Build an [`ImageSet`] from `(url, width, height)` tuples.
pub(crate) fn images_to_set<I>(images: I) -> Option<ImageSet>
where
    I: IntoIterator<Item = (String, Option<u32>, Option<u32>)>,
{
    let imgs: Vec<Image> = images
        .into_iter()
        .filter(|(url, _, _)| !url.is_empty())
        .map(|(url, width, height)| Image { url, width, height })
        .collect();
    if imgs.is_empty() {
        None
    } else {
        Some(ImageSet {
            images: imgs,
            template: None,
        })
    }
}

pub(crate) fn content_rating_from_explicit(explicit: Option<bool>) -> ContentRating {
    match explicit {
        Some(true) => ContentRating::Explicit,
        Some(false) => ContentRating::Clean,
        None => ContentRating::None,
    }
}

/// Parse "YYYY", "YYYY-MM", or "YYYY-MM-DD" into a [`ReleaseDate`].
pub(crate) fn parse_release_date(s: &str) -> Option<ReleaseDate> {
    let mut parts = s.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next().and_then(|m| m.parse::<u8>().ok());
    let day = parts.next().and_then(|d| d.parse::<u8>().ok());
    Some(ReleaseDate { year, month, day })
}

pub(crate) fn enum_to_string<T: serde::Serialize>(value: &T) -> Option<String> {
    match serde_json::to_value(value).ok()? {
        serde_json::Value::String(s) => Some(s),
        _ => None,
    }
}

pub(crate) fn album_type_from<T: serde::Serialize>(value: &T) -> Option<AlbumType> {
    enum_to_string(value).map(|s| match s.as_str() {
        "album" => AlbumType::Album,
        "single" => AlbumType::Single,
        "compilation" => AlbumType::Compilation,
        other => AlbumType::Other(other.to_string()),
    })
}
