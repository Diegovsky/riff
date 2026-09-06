//! Domain layer: auth-injecting Spotify API client.
//!
//! Caching is handled one layer up in `service.rs`.

use std::sync::{Arc, RwLock};
use std::time::Instant;

use spotify_api::apis::{self, configuration::Configuration};
use spotify_api::models as sp;

use async_trait::async_trait;

use super::converter::*;
use crate::defaults::{self, EntityKind};
use crate::error::DomainError;
use crate::models::*;
use crate::providers::{content_rating_from_explicit, MusicProvider};
use crate::token::TokenProvider;

pub struct UserProfileCheck {
    pub user_id: String,
    pub is_premium: bool,
    pub explicit_filter_enabled: bool,
    pub explicit_filter_locked: bool,
}

pub async fn check_user_profile(token: &str) -> Result<UserProfileCheck, DomainError> {
    let mut config = Configuration::new();
    config.oauth_access_token = Some(token.to_string());
    let user = apis::users_api::get_current_users_profile(&config).await?;

    let is_premium = !matches!(user.product.as_deref(), Some(p) if p != "premium");

    let (explicit_filter_enabled, explicit_filter_locked) = user
        .explicit_content
        .as_ref()
        .map(|e| {
            (
                e.filter_enabled.unwrap_or(false),
                e.filter_locked.unwrap_or(false),
            )
        })
        .unwrap_or((false, false));

    Ok(UserProfileCheck {
        user_id: user.id.unwrap_or_default(),
        is_premium,
        explicit_filter_enabled,
        explicit_filter_locked,
    })
}

impl<T: std::fmt::Debug> From<apis::Error<T>> for DomainError {
    fn from(e: apis::Error<T>) -> Self {
        match e {
            apis::Error::ResponseError(ref resp) => {
                let status = resp.status.as_u16();
                let message = resp.content.clone();
                match status {
                    401 => DomainError::AuthExpired,
                    429 => DomainError::RateLimited {
                        retry_after_ms: None,
                    },
                    404 => DomainError::NotFound { resource: message },
                    400..=499 => DomainError::ClientError { status, message },
                    500..=599 => DomainError::ServerError { status, message },
                    _ => DomainError::ClientError { status, message },
                }
            }
            apis::Error::Reqwest(e) => DomainError::Network(e.to_string()),
            apis::Error::ReqwestMiddleware(e) => DomainError::Network(e.to_string()),
            apis::Error::Serde(e) => DomainError::Parse(e.to_string()),
            apis::Error::Io(e) => DomainError::Network(e.to_string()),
        }
    }
}

struct CachedConfig {
    config: Configuration,
    created_at: Instant,
}

pub struct SpotifyDomain {
    token_provider: Arc<dyn TokenProvider>,
    cached_config: RwLock<Option<CachedConfig>>,
}

use riff_config::CONFIG_CACHE_TTL_SECS;

impl SpotifyDomain {
    pub fn new(token_provider: Arc<dyn TokenProvider>) -> Self {
        Self {
            token_provider,
            cached_config: RwLock::new(None),
        }
    }

    fn config_or_err(&self) -> Result<Configuration, DomainError> {
        #[cfg(debug_assertions)]
        if let Some(err) = crate::dev::simulated_failure() {
            return Err(err);
        }

        {
            let guard = self.cached_config.read().unwrap();
            if let Some(ref cached) = *guard {
                if cached.created_at.elapsed().as_secs() < CONFIG_CACHE_TTL_SECS {
                    return Ok(cached.config.clone());
                }
            }
        }

        let access_token = self
            .token_provider
            .access_token()
            .ok_or(DomainError::NoToken)?;

        let mut config = Configuration::new();
        config.oauth_access_token = Some(access_token);

        let mut guard = self.cached_config.write().unwrap();
        *guard = Some(CachedConfig {
            config: config.clone(),
            created_at: Instant::now(),
        });

        Ok(config)
    }
}

#[async_trait]
impl MusicProvider for SpotifyDomain {
    fn invalidate_config(&self) {
        let mut guard = self.cached_config.write().unwrap();
        *guard = None;
    }

    async fn get_album(&self, id: &str) -> Result<Album, DomainError> {
        let config = self.config_or_err()?;
        let raw = apis::albums_api::get_an_album(&config, id, None).await?;
        Ok(album_from_object(&raw))
    }

    async fn get_album_tracks(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError> {
        let config = self.config_or_err()?;
        let page = apis::albums_api::get_an_albums_tracks(
            &config,
            id,
            None,
            Some(limit as i32),
            Some(offset as i32),
        )
        .await?;
        let album_ref = AlbumRef {
            rri: rid(id.to_string()),
            name: String::new(),
        };
        let items = page
            .items
            .iter()
            .map(|t| Track {
                rri: ResourceId {
                    provider: Provider::Spotify,
                    id: t.id.clone().unwrap_or_default(),
                    uri: t.uri.clone(),
                },
                title: text_or(t.name.clone().unwrap_or_default(), defaults::track_title),
                artists: ensure_artists(
                    t.artists
                        .as_ref()
                        .map(|a| a.iter().map(to_artist_ref).collect())
                        .unwrap_or_default(),
                ),
                album: Some(album_ref.clone()),
                duration_ms: t.duration_ms.unwrap_or(0) as u32,
                track_number: t.track_number.map(|n| n as u32),
                disc_number: None,
                content_rating: content_rating_from_explicit(t.explicit),
                isrc: None,
                art: defaults::image_set_for(EntityKind::Track),
                playable: t.is_playable.unwrap_or(true),
                popularity: None,
                saved: None,
                preview_url: None,
                url: None,
            })
            .collect();
        Ok(Page::offset_paged(
            items,
            page.offset as usize,
            page.total as usize,
        ))
    }

    async fn get_saved_albums(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Album>, DomainError> {
        let config = self.config_or_err()?;
        let page = apis::library_api::get_users_saved_albums(
            &config,
            Some(limit as i32),
            Some(offset as i32),
            None,
        )
        .await?;
        let items = page.items.iter().filter_map(album_from_saved).collect();
        Ok(Page::offset_paged(
            items,
            page.offset as usize,
            page.total as usize,
        ))
    }

    async fn check_saved_albums(&self, ids: &str) -> Result<Vec<bool>, DomainError> {
        let config = self.config_or_err()?;
        Ok(apis::library_api::check_users_saved_albums(&config, ids).await?)
    }

    async fn save_albums(&self, ids: &str) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        Ok(apis::library_api::save_albums_user(&config, ids, None).await?)
    }

    async fn remove_albums(&self, ids: &str) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        Ok(apis::library_api::remove_albums_user(&config, ids, None).await?)
    }

    async fn get_track(&self, id: &str) -> Result<Track, DomainError> {
        let config = self.config_or_err()?;
        let raw = apis::tracks_api::get_track(&config, id, None).await?;
        track_from_object(&raw).ok_or_else(|| DomainError::Parse("track missing id".into()))
    }

    async fn get_saved_tracks(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError> {
        let config = self.config_or_err()?;
        let page = apis::library_api::get_users_saved_tracks(
            &config,
            None,
            Some(limit as i32),
            Some(offset as i32),
        )
        .await?;
        let items = page
            .items
            .iter()
            .filter_map(|st| st.track.as_ref().and_then(|t| track_from_object(t)))
            .collect();
        Ok(Page::offset_paged(
            items,
            page.offset as usize,
            page.total as usize,
        ))
    }

    async fn save_tracks(&self, ids: Vec<String>) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        // Save via PUT /me/tracks with the IDs in the request body. This mirrors
        // remove_tracks (DELETE /me/tracks) and takes bare Spotify track IDs.
        // The generic save_library_items endpoint expects full Spotify URIs, not
        // the bare IDs the app passes, so it silently rejects likes.
        let body = sp::SaveTracksUserRequest {
            ids: Some(ids),
            timestamped_ids: None,
        };
        Ok(apis::library_api::save_tracks_user(&config, Some(body)).await?)
    }

    async fn remove_tracks(&self, ids: Vec<String>) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        let ids = ids.join(",");
        Ok(apis::library_api::remove_tracks_user(&config, &ids, None).await?)
    }

    async fn get_saved_playlists(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Playlist>, DomainError> {
        let config = self.config_or_err()?;
        let page = apis::playlists_api::get_a_list_of_current_users_playlists(
            &config,
            Some(limit as i32),
            Some(offset as i32),
        )
        .await?;
        let items = page.items.iter().map(playlist_from_simplified).collect();
        Ok(Page::offset_paged(
            items,
            page.offset as usize,
            page.total as usize,
        ))
    }

    async fn get_playlist(&self, id: &str) -> Result<Playlist, DomainError> {
        let config = self.config_or_err()?;
        let raw = apis::playlists_api::get_playlist(&config, id, None, None, None).await?;
        Ok(playlist_from_object(&raw))
    }

    async fn get_playlist_tracks(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError> {
        let config = self.config_or_err()?;
        let page = apis::playlists_api::get_playlists_tracks(
            &config,
            id,
            None,
            None,
            Some(limit as i32),
            Some(offset as i32),
            None,
        )
        .await?;
        let items = page
            .items
            .iter()
            .filter_map(|pt| {
                pt.track.as_ref().and_then(|t| match t.as_ref() {
                    sp::PlaylistTrackObjectTrack::Track(track) => track_from_object(track),
                    _ => None,
                })
            })
            .collect();
        Ok(Page::offset_paged(
            items,
            page.offset as usize,
            page.total as usize,
        ))
    }

    async fn add_to_playlist(
        &self,
        id: &str,
        uris: Vec<String>,
        position: Option<i32>,
    ) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        let body = sp::AddTracksToPlaylistRequest {
            uris: Some(uris),
            position,
        };
        apis::playlists_api::add_tracks_to_playlist(&config, id, None, None, Some(body)).await?;
        Ok(())
    }

    async fn remove_from_playlist(
        &self,
        id: &str,
        uris: Vec<String>,
        snapshot_id: Option<&str>,
    ) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        let tracks: Vec<sp::RemoveTracksPlaylistRequestTracksInner> = uris
            .into_iter()
            .map(|uri| sp::RemoveTracksPlaylistRequestTracksInner { uri: Some(uri) })
            .collect();
        let body = sp::RemoveTracksPlaylistRequest {
            tracks,
            snapshot_id: snapshot_id.map(|s| s.to_string()),
        };
        apis::playlists_api::remove_tracks_playlist(&config, id, Some(body)).await?;
        Ok(())
    }

    async fn create_playlist(
        &self,
        user_id: &str,
        name: &str,
        public: Option<bool>,
        collaborative: Option<bool>,
        description: Option<&str>,
    ) -> Result<Playlist, DomainError> {
        let config = self.config_or_err()?;
        let body = sp::CreatePlaylistRequest {
            name: name.to_string(),
            public,
            collaborative,
            description: description.map(|s| s.to_string()),
        };
        let raw =
            apis::playlists_api::create_playlist_for_user(&config, user_id, Some(body)).await?;
        Ok(playlist_from_object(&raw))
    }

    async fn follow_playlist(&self, id: &str) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        Ok(apis::playlists_api::follow_playlist(&config, id, None).await?)
    }

    async fn unfollow_playlist(&self, id: &str) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        Ok(apis::playlists_api::unfollow_playlist(&config, id).await?)
    }

    async fn update_playlist_details(
        &self,
        id: &str,
        name: Option<&str>,
        public: Option<bool>,
        collaborative: Option<bool>,
        description: Option<&str>,
    ) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        let body = sp::ChangePlaylistDetailsRequest {
            name: name.map(|s| s.to_string()),
            public,
            collaborative,
            description: description.map(|s| s.to_string()),
        };
        Ok(apis::playlists_api::change_playlist_details(&config, id, Some(body)).await?)
    }

    async fn get_artist(&self, id: &str) -> Result<Artist, DomainError> {
        let config = self.config_or_err()?;
        let raw = apis::artists_api::get_an_artist(&config, id).await?;
        Ok(artist_from_object(&raw))
    }

    async fn get_artist_albums(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Album>, DomainError> {
        let config = self.config_or_err()?;
        let page = apis::artists_api::get_an_artists_albums(
            &config,
            id,
            Some("album,single"),
            None,
            Some(limit as i32),
            Some(offset as i32),
        )
        .await?;
        let items = page.items.iter().map(album_from_discography).collect();
        Ok(Page::offset_paged(
            items,
            page.offset as usize,
            page.total as usize,
        ))
    }

    async fn get_artist_top_tracks(&self, id: &str) -> Result<Vec<Track>, DomainError> {
        let config = self.config_or_err()?;
        let resp = apis::artists_api::get_an_artists_top_tracks(&config, id, None).await?;
        Ok(resp.tracks.iter().filter_map(track_from_object).collect())
    }

    async fn get_followed_artists(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<Artist>, Option<String>), DomainError> {
        let config = self.config_or_err()?;
        let resp =
            apis::library_api::get_followed(&config, "artist", after, Some(limit as i32)).await?;
        let artists = resp
            .artists
            .items
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(artist_from_object)
            .collect();
        let next = resp.artists.cursors.as_ref().and_then(|c| c.after.clone());
        Ok((artists, next))
    }

    async fn follow_artists(&self, ids: &str) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        Ok(apis::library_api::follow_artists_users(&config, "artist", ids, None).await?)
    }

    async fn unfollow_artists(&self, ids: &str) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        Ok(apis::library_api::unfollow_artists_users(&config, "artist", ids, None).await?)
    }

    async fn check_following_artists(&self, ids: &str) -> Result<Vec<bool>, DomainError> {
        let config = self.config_or_err()?;
        Ok(apis::library_api::check_current_user_follows(&config, "artist", ids).await?)
    }

    async fn get_current_user(&self) -> Result<User, DomainError> {
        let config = self.config_or_err()?;
        let raw = apis::users_api::get_current_users_profile(&config).await?;
        Ok(current_user_from_private(&raw))
    }

    async fn get_user(&self, id: &str) -> Result<User, DomainError> {
        let config = self.config_or_err()?;
        let raw = apis::users_api::get_users_profile(&config, id).await?;
        Ok(user_from_object(&raw))
    }

    async fn get_user_playlists(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Playlist>, DomainError> {
        let config = self.config_or_err()?;
        let page = apis::playlists_api::get_list_users_playlists(
            &config,
            id,
            Some(limit as i32),
            Some(offset as i32),
        )
        .await?;
        let items = page.items.iter().map(playlist_from_simplified).collect();
        Ok(Page::offset_paged(
            items,
            page.offset as usize,
            page.total as usize,
        ))
    }

    async fn search(
        &self,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResults, DomainError> {
        let config = self.config_or_err()?;
        let types = vec![
            "album".to_string(),
            "artist".to_string(),
            "track".to_string(),
            "playlist".to_string(),
        ];
        let resp = apis::search_api::search(
            &config,
            query,
            types,
            None,
            Some(limit as i32),
            Some(offset as i32),
            None,
        )
        .await?;
        Ok(search_results_from_response(&resp))
    }

    async fn search_scoped(
        &self,
        query: &str,
        kind: SearchType,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResults, DomainError> {
        let config = self.config_or_err()?;
        let types = vec![search_type_str(kind).to_string()];
        let resp = apis::search_api::search(
            &config,
            query,
            types,
            None,
            Some(limit as i32),
            Some(offset as i32),
            None,
        )
        .await?;
        Ok(search_results_from_response(&resp))
    }

    async fn get_devices(&self) -> Result<Vec<Device>, DomainError> {
        let config = self.config_or_err()?;
        let resp = apis::player_api::get_a_users_available_devices(&config).await?;
        Ok(resp.devices.iter().filter_map(device_from_object).collect())
    }

    async fn get_player_queue(&self) -> Result<Queue, DomainError> {
        let config = self.config_or_err()?;
        Ok(queue_from_object(
            &apis::player_api::get_queue(&config).await?,
        ))
    }

    async fn get_player_state(&self) -> Result<PlayerState, DomainError> {
        let config = self.config_or_err()?;
        let raw =
            apis::player_api::get_information_about_the_users_current_playback(&config, None, None)
                .await?;
        Ok(player_state_from_object(&raw))
    }

    async fn player_resume(&self, device_id: &str) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        Ok(apis::player_api::start_a_users_playback(&config, Some(device_id), None).await?)
    }

    async fn player_play_in_context(
        &self,
        device_id: &str,
        context_uri: &str,
        offset: usize,
    ) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        let mut offset_map = std::collections::HashMap::new();
        offset_map.insert("position".to_string(), serde_json::json!(offset));
        let body = sp::StartAUsersPlaybackRequest {
            context_uri: Some(context_uri.to_string()),
            uris: None,
            offset: Some(offset_map),
            position_ms: None,
        };
        Ok(apis::player_api::start_a_users_playback(&config, Some(device_id), Some(body)).await?)
    }

    async fn player_play_uris(
        &self,
        device_id: &str,
        uris: Vec<String>,
        offset: usize,
    ) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        let mut offset_map = std::collections::HashMap::new();
        offset_map.insert("position".to_string(), serde_json::json!(offset));
        let body = sp::StartAUsersPlaybackRequest {
            context_uri: None,
            uris: Some(uris),
            offset: Some(offset_map),
            position_ms: None,
        };
        Ok(apis::player_api::start_a_users_playback(&config, Some(device_id), Some(body)).await?)
    }

    async fn player_pause(&self, device_id: &str) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        Ok(apis::player_api::pause_a_users_playback(&config, Some(device_id)).await?)
    }

    async fn player_seek(&self, device_id: &str, position_ms: u32) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        Ok(
            apis::player_api::seek_to_position_in_currently_playing_track(
                &config,
                position_ms as i32,
                Some(device_id),
            )
            .await?,
        )
    }

    async fn player_repeat(&self, device_id: &str, mode: RepeatMode) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        let state = repeat_mode_str(mode);
        Ok(
            apis::player_api::set_repeat_mode_on_users_playback(&config, state, Some(device_id))
                .await?,
        )
    }

    async fn player_shuffle(&self, device_id: &str, state: bool) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        Ok(
            apis::player_api::toggle_shuffle_for_users_playback(&config, state, Some(device_id))
                .await?,
        )
    }

    async fn player_volume(&self, device_id: &str, volume_percent: u8) -> Result<(), DomainError> {
        let config = self.config_or_err()?;
        Ok(apis::player_api::set_volume_for_users_playback(
            &config,
            volume_percent as i32,
            Some(device_id),
        )
        .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_errors_are_classified() {
        assert!(DomainError::Network("x".into()).is_transient());
        assert!(DomainError::RateLimited {
            retry_after_ms: None
        }
        .is_transient());
        assert!(DomainError::ServerError {
            status: 500,
            message: "x".into()
        }
        .is_transient());
        assert!(!DomainError::AuthExpired.is_transient());
        assert!(!DomainError::NotFound {
            resource: "x".into()
        }
        .is_transient());
    }
}
