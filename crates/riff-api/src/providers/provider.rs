//! The provider contract.
//!
//! [`MusicProvider`] is a generic music vendor interface the
//! [`crate::service::ApiService`] depends on.

use async_trait::async_trait;

use crate::error::DomainError;
use crate::models::*;

#[async_trait]
pub trait MusicProvider: Send + Sync + 'static {
    async fn get_album(&self, id: &str) -> Result<Album, DomainError>;
    async fn get_album_tracks(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError>;
    async fn get_saved_albums(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Album>, DomainError>;
    async fn check_saved_albums(&self, ids: &str) -> Result<Vec<bool>, DomainError>;
    async fn save_albums(&self, ids: &str) -> Result<(), DomainError>;
    async fn remove_albums(&self, ids: &str) -> Result<(), DomainError>;

    async fn get_track(&self, id: &str) -> Result<Track, DomainError>;
    async fn get_saved_tracks(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError>;
    async fn save_tracks(&self, ids: Vec<String>) -> Result<(), DomainError>;
    async fn remove_tracks(&self, ids: Vec<String>) -> Result<(), DomainError>;

    async fn get_saved_playlists(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Playlist>, DomainError>;
    async fn get_playlist(&self, id: &str) -> Result<Playlist, DomainError>;
    async fn get_playlist_tracks(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError>;
    async fn add_to_playlist(
        &self,
        id: &str,
        uris: Vec<String>,
        position: Option<i32>,
    ) -> Result<(), DomainError>;
    async fn remove_from_playlist(
        &self,
        id: &str,
        uris: Vec<String>,
        snapshot_id: Option<&str>,
    ) -> Result<(), DomainError>;
    async fn create_playlist(
        &self,
        user_id: &str,
        name: &str,
        public: Option<bool>,
        collaborative: Option<bool>,
        description: Option<&str>,
    ) -> Result<Playlist, DomainError>;
    async fn follow_playlist(&self, id: &str) -> Result<(), DomainError>;
    async fn unfollow_playlist(&self, id: &str) -> Result<(), DomainError>;
    async fn update_playlist_details(
        &self,
        id: &str,
        name: Option<&str>,
        public: Option<bool>,
        collaborative: Option<bool>,
        description: Option<&str>,
    ) -> Result<(), DomainError>;

    async fn get_artist(&self, id: &str) -> Result<Artist, DomainError>;
    async fn get_artist_albums(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Album>, DomainError>;
    async fn get_artist_top_tracks(&self, id: &str) -> Result<Vec<Track>, DomainError>;
    async fn get_followed_artists(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<Artist>, Option<String>), DomainError>;
    async fn follow_artists(&self, ids: &str) -> Result<(), DomainError>;
    async fn unfollow_artists(&self, ids: &str) -> Result<(), DomainError>;
    async fn check_following_artists(&self, ids: &str) -> Result<Vec<bool>, DomainError>;

    async fn get_current_user(&self) -> Result<User, DomainError>;
    async fn get_user(&self, id: &str) -> Result<User, DomainError>;
    async fn get_user_playlists(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Playlist>, DomainError>;

    async fn search(
        &self,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResults, DomainError>;
    async fn search_scoped(
        &self,
        query: &str,
        kind: SearchType,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResults, DomainError>;

    async fn get_devices(&self) -> Result<Vec<Device>, DomainError>;
    async fn get_player_queue(&self) -> Result<Queue, DomainError>;
    async fn get_player_state(&self) -> Result<PlayerState, DomainError>;
    async fn player_resume(&self, device_id: &str) -> Result<(), DomainError>;
    async fn player_play_in_context(
        &self,
        device_id: &str,
        context_uri: &str,
        offset: usize,
    ) -> Result<(), DomainError>;
    async fn player_play_uris(
        &self,
        device_id: &str,
        uris: Vec<String>,
        offset: usize,
    ) -> Result<(), DomainError>;
    async fn player_pause(&self, device_id: &str) -> Result<(), DomainError>;
    async fn player_seek(&self, device_id: &str, position_ms: u32) -> Result<(), DomainError>;
    async fn player_repeat(&self, device_id: &str, mode: RepeatMode) -> Result<(), DomainError>;
    async fn player_shuffle(&self, device_id: &str, state: bool) -> Result<(), DomainError>;
    async fn player_volume(&self, device_id: &str, volume_percent: u8) -> Result<(), DomainError>;

    /// Drop any cached auth configuration (e.g. after a 401).
    fn invalidate_config(&self);
}
