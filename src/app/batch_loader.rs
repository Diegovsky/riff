use gettextrs::gettext;
use std::sync::Arc;

use crate::api::{SpotifyApiClient, SpotifyApiError};
use crate::app::models::*;
use crate::app::AppAction;

// A wrapper around the Spotify API to load batches of songs from various sources (see below)
#[derive(Clone)]
pub struct BatchLoader {
    api: Arc<dyn SpotifyApiClient + Send + Sync>,
}

// The sources mentionned above
#[derive(Clone, Debug)]
pub enum SongsSource {
    Playlist(String),
    Album(String),
    Artist(String),
    SavedTracks,
}

impl PartialEq for SongsSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Playlist(l), Self::Playlist(r)) => l == r,
            (Self::Album(l), Self::Album(r)) => l == r,
            (Self::Artist(l), Self::Artist(r)) => l == r,
            (Self::SavedTracks, Self::SavedTracks) => true,
            _ => false,
        }
    }
}

impl Eq for SongsSource {}

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
}

// How to query for a batch: specify a source, and a batch to get (offset + number of elements to get)
#[derive(Debug)]
pub struct BatchQuery {
    pub source: SongsSource,
    pub batch: Batch,
}

impl BatchLoader {
    pub fn new(api: Arc<dyn SpotifyApiClient + Send + Sync>) -> Self {
        Self { api }
    }

    // Query a batch and create an action when it's been retrieved succesfully
    pub async fn query<ActionCreator>(
        &self,
        query: BatchQuery,
        create_action: ActionCreator,
    ) -> Option<AppAction>
    where
        ActionCreator: FnOnce(SongsSource, SongBatch) -> AppAction,
    {
        let api = Arc::clone(&self.api);

        let Batch {
            offset, batch_size, ..
        } = query.batch;
        if matches!(&query.source, SongsSource::Artist(_)) {
            error!("Artist top tracks are not paginated and should not be batch-loaded");
            return None;
        }

        let do_fetch = || match &query.source {
            SongsSource::Playlist(id) => api.get_playlist_tracks(id, offset, batch_size),
            SongsSource::SavedTracks => api.get_saved_tracks(offset, batch_size),
            SongsSource::Album(id) => api.get_album_tracks(id, offset, batch_size),
            SongsSource::Artist(_) => unreachable!(),
        };

        let result = match do_fetch().await {
            Err(SpotifyApiError::InvalidToken) => do_fetch().await,
            other => other,
        };

        match result {
            Ok(batch) => Some(create_action(query.source, batch)),
            Err(SpotifyApiError::NoToken | SpotifyApiError::InvalidToken) => None,
            Err(SpotifyApiError::TooManyRequests) => {
                error!("Spotify API error: rate limited");
                Some(AppAction::ShowNotification(gettext(
                    // translators: This notification is shown when Spotify throttles requests.
                    "Rate limited by Spotify. Please wait a moment and try again.",
                )))
            }
            Err(err) => {
                error!("Spotify API error: {}", err);
                Some(AppAction::ShowNotification(gettext(
                    // translators: This notification is the default message for unhandled errors. Logs refer to console output.
                    "An error occured. Check logs for details!",
                )))
            }
        }
    }
}
