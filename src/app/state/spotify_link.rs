//! Parsing of Spotify links into a small, typed representation.
//!
//! Riff needs to understand two flavours of Spotify link:
//!  - `spotify:` URIs, e.g. `spotify:album:6akEvsycLGftJxYudPjmqK`, used for
//!    deep links / the desktop file's `HANDLES_OPEN`.
//!  - `https://open.spotify.com/...` URLs, which is what people copy out of the
//!    Spotify apps and web player (and what Riff itself copies via "Copy link").
//!
//! Real-world web URLs often carry a locale prefix (`/intl-de/track/...`) and a
//! tracking query (`?si=...`); both are normalised away here.

/// A resolved reference to something on Spotify that Riff knows how to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpotifyLink {
    Album(String),
    Artist(String),
    Playlist(String),
    User(String),
    Track(String),
}

impl SpotifyLink {
    /// Parse a `spotify:` URI or an `open.spotify.com` URL into a [`SpotifyLink`].
    ///
    /// Returns `None` if the input is not a Spotify link or references a type
    /// Riff does not know how to open.
    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }

        if let Some(rest) = input.strip_prefix("spotify:") {
            Self::parse_uri(rest)
        } else {
            Self::parse_url(input)
        }
    }

    /// Parse the part of a `spotify:` URI following the `spotify:` scheme, e.g.
    /// `album:6akEvsycLGftJxYudPjmqK`.
    fn parse_uri(rest: &str) -> Option<Self> {
        // glib may hand us URIs with a leading `///` because of
        // https://gitlab.gnome.org/GNOME/glib/-/issues/1886/
        let rest = rest.trim_start_matches('/');
        let mut parts = rest.split(':');
        let kind = parts.next()?;
        let id = parts.next()?;
        Self::from_kind_and_id(kind, id)
    }

    /// Parse an `https://open.spotify.com/...` URL.
    fn parse_url(input: &str) -> Option<Self> {
        // Strip the scheme; accept both http and https.
        let rest = input
            .strip_prefix("https://")
            .or_else(|| input.strip_prefix("http://"))?;

        // Only `open.spotify.com` links are supported.
        let rest = rest.strip_prefix("open.spotify.com/")?;

        // Drop any query string or fragment (e.g. `?si=...`).
        let path = rest
            .split(['?', '#'])
            .next()
            .unwrap_or(rest)
            .trim_matches('/');

        let mut segments = path.split('/').filter(|s| !s.is_empty());
        let mut kind = segments.next()?;

        // Skip a locale prefix such as `intl-de`.
        if kind.starts_with("intl-") {
            kind = segments.next()?;
        }

        let id = segments.next()?;
        Self::from_kind_and_id(kind, id)
    }

    fn from_kind_and_id(kind: &str, id: &str) -> Option<Self> {
        if id.is_empty() {
            return None;
        }
        let id = id.to_string();
        match kind {
            "album" => Some(Self::Album(id)),
            "artist" => Some(Self::Artist(id)),
            "playlist" => Some(Self::Playlist(id)),
            "user" => Some(Self::User(id)),
            "track" => Some(Self::Track(id)),
            _ => None,
        }
    }

    /// A short, human-readable label for the kind of link, used in the
    /// "open this link?" prompt.
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Album(_) => "album",
            Self::Artist(_) => "artist",
            Self::Playlist(_) => "playlist",
            Self::User(_) => "profile",
            Self::Track(_) => "track",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_uri_album() {
        assert_eq!(
            SpotifyLink::parse("spotify:album:6akEvsycLGftJxYudPjmqK"),
            Some(SpotifyLink::Album("6akEvsycLGftJxYudPjmqK".to_string()))
        );
    }

    #[test]
    fn test_parse_uri_all_types() {
        assert_eq!(
            SpotifyLink::parse("spotify:artist:abc"),
            Some(SpotifyLink::Artist("abc".to_string()))
        );
        assert_eq!(
            SpotifyLink::parse("spotify:playlist:abc"),
            Some(SpotifyLink::Playlist("abc".to_string()))
        );
        assert_eq!(
            SpotifyLink::parse("spotify:user:abc"),
            Some(SpotifyLink::User("abc".to_string()))
        );
        assert_eq!(
            SpotifyLink::parse("spotify:track:abc"),
            Some(SpotifyLink::Track("abc".to_string()))
        );
    }

    #[test]
    fn test_parse_uri_with_glib_slashes() {
        // glib sometimes prepends `///` to the first URI component.
        assert_eq!(
            SpotifyLink::parse("spotify:///playlist:xyz"),
            Some(SpotifyLink::Playlist("xyz".to_string()))
        );
    }

    #[test]
    fn test_parse_url_track() {
        assert_eq!(
            SpotifyLink::parse("https://open.spotify.com/track/6rqhFgbbKwnb9MLmUQDhG6"),
            Some(SpotifyLink::Track("6rqhFgbbKwnb9MLmUQDhG6".to_string()))
        );
    }

    #[test]
    fn test_parse_url_with_query() {
        assert_eq!(
            SpotifyLink::parse(
                "https://open.spotify.com/album/6akEvsycLGftJxYudPjmqK?si=abcdef123456"
            ),
            Some(SpotifyLink::Album("6akEvsycLGftJxYudPjmqK".to_string()))
        );
    }

    #[test]
    fn test_parse_url_with_locale_prefix() {
        assert_eq!(
            SpotifyLink::parse("https://open.spotify.com/intl-de/track/abc?si=xyz"),
            Some(SpotifyLink::Track("abc".to_string()))
        );
    }

    #[test]
    fn test_parse_url_http_scheme() {
        assert_eq!(
            SpotifyLink::parse("http://open.spotify.com/artist/abc"),
            Some(SpotifyLink::Artist("abc".to_string()))
        );
    }

    #[test]
    fn test_parse_url_trailing_slash() {
        assert_eq!(
            SpotifyLink::parse("https://open.spotify.com/playlist/abc/"),
            Some(SpotifyLink::Playlist("abc".to_string()))
        );
    }

    #[test]
    fn test_parse_with_surrounding_whitespace() {
        assert_eq!(
            SpotifyLink::parse("  spotify:album:abc\n"),
            Some(SpotifyLink::Album("abc".to_string()))
        );
    }

    #[test]
    fn test_reject_non_spotify() {
        assert_eq!(SpotifyLink::parse(""), None);
        assert_eq!(SpotifyLink::parse("hello world"), None);
        assert_eq!(SpotifyLink::parse("https://example.com/track/abc"), None);
        assert_eq!(
            SpotifyLink::parse("https://open.spotify.com/unknown/abc"),
            None
        );
        assert_eq!(SpotifyLink::parse("spotify:album:"), None);
        assert_eq!(SpotifyLink::parse("spotify:album"), None);
        assert_eq!(SpotifyLink::parse("https://open.spotify.com/track"), None);
    }
}
