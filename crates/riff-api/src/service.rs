//! ApiService - orchestrates caching around domain calls.
//!
//! Tier order: memory LRU -> disk cache -> network.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use serde::{de::DeserializeOwned, Serialize};

use riff_config::{API_TTL, IMAGE_TTL, STALE_IF_ERROR_TTL};

use crate::cache::{CacheKey, DiskCache, Store, TextureCache};
use crate::error::DomainError;
use crate::http;
use crate::models::*;
use crate::providers::MusicProvider;
use crate::token::TokenProvider;

#[derive(Serialize, serde::Deserialize)]
struct PersistedPage {
    pages: Vec<(usize, Vec<serde_json::Value>)>,
    total: usize,
}

pub struct ApiService {
    provider: Arc<dyn MusicProvider>,
    texture_cache: Arc<TextureCache>,
    json_cache: Arc<Store>,
    cdn_client: http::ServiceClient,
    image_disk: DiskCache,
    api_disk: DiskCache,
    token_provider: Arc<dyn TokenProvider>,
    inflight: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl ApiService {
    /// Build a service around a provider implementation. Crate-private: services
    /// are constructed through a vendor factory (e.g. `spotify_service`).
    pub(crate) fn new(
        provider: Arc<dyn MusicProvider>,
        token_provider: Arc<dyn TokenProvider>,
        memory_cache_bytes: usize,
        disk_cache_bytes: usize,
    ) -> Self {
        let pool = http::build_shared_pool();
        Self {
            provider,
            texture_cache: Arc::new(TextureCache::new(memory_cache_bytes)),
            json_cache: Arc::new(Store::new(memory_cache_bytes)),
            cdn_client: http::cdn_service(pool),
            image_disk: DiskCache::new("riff/img", disk_cache_bytes, IMAGE_TTL),
            api_disk: DiskCache::new("riff/net", disk_cache_bytes, API_TTL),
            token_provider,
            inflight: Mutex::new(HashMap::new()),
        }
    }

    pub fn has_token(&self) -> bool {
        self.token_provider.access_token().is_some()
    }

    /// Evict both disk caches to their budgets. Call on shutdown.
    pub async fn run_cache_maintenance(&self) {
        self.api_disk.evict_to_budget().await;
        self.image_disk.evict_to_budget().await;
    }

    /// Clear all user-specific cached data (memory + API disk). The image
    /// cache is content-addressed and safe to keep.
    pub async fn clear_user_cache(&self) {
        self.json_cache.clear();
        self.api_disk.clear().await;
    }

    fn handle_auth_error(&self, err: &DomainError) {
        if matches!(err, DomainError::AuthExpired) {
            self.provider.invalidate_config();
        }
    }

    fn inflight_lock(&self, id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut map = self.inflight.lock().unwrap();
        map.entry(id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    fn release_inflight(&self, id: &str, lock: Arc<tokio::sync::Mutex<()>>) {
        let mut map = self.inflight.lock().unwrap();
        if Arc::strong_count(&lock) <= 2 {
            map.remove(id);
        }
    }

    /// Single-resource fetch: memory -> disk -> network, with single-flight.
    async fn cached_or_fetch<T, F>(&self, cache_key: CacheKey, fetch: F) -> Result<T, DomainError>
    where
        T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
        F: std::future::Future<Output = Result<T, DomainError>> + Send + 'static,
    {
        // Tier 1: Memory
        if let Some(val) = self.json_cache.get_single::<T>(&cache_key) {
            return Ok(val);
        }

        let disk_key = cache_key.disk_key();
        let flight = self.inflight_lock(&disk_key);
        let guard = flight.lock().await;

        let outcome: Result<T, DomainError> = 'resolved: {
            // Re-check memory after acquiring the lock.
            if let Some(val) = self.json_cache.get_single::<T>(&cache_key) {
                break 'resolved Ok(val);
            }

            // Tier 2: Disk. Fresh entries are served directly; stale entries
            // are kept for stale-if-error fallback.
            let mut stale: Option<(T, Option<String>)> = None;
            if let Some(entry) = self.api_disk.read(&disk_key).await {
                if let Ok(val) = serde_json::from_slice::<T>(&entry.data) {
                    match entry.state {
                        crate::cache::disk::EntryState::Fresh => {
                            self.json_cache.insert_single(&cache_key, val.clone());
                            break 'resolved Ok(val);
                        }
                        crate::cache::disk::EntryState::Stale { etag } => {
                            stale = Some((val, etag));
                        }
                    }
                }
            }

            // Tier 3: Network.
            match tokio::spawn(fetch).await {
                Ok(Ok(val)) => {
                    self.json_cache.insert_single(&cache_key, val.clone());
                    if let Ok(bytes) = serde_json::to_vec(&val) {
                        let disk = self.api_disk.clone();
                        let dk = disk_key.clone();
                        tokio::spawn(async move {
                            disk.write_default(&dk, &bytes, None).await;
                        });
                    }
                    Ok(val)
                }
                Ok(Err(err)) => {
                    self.handle_auth_error(&err);
                    // Stale-if-error: serve stale copy on transient failure.
                    if let (Some((val, etag)), true) = (stale, err.is_transient()) {
                        self.json_cache.insert_single(&cache_key, val.clone());
                        let disk = self.api_disk.clone();
                        let dk = disk_key.clone();
                        tokio::spawn(async move {
                            disk.refresh_ttl(&dk, STALE_IF_ERROR_TTL, etag.as_deref())
                                .await;
                        });
                        Ok(val)
                    } else {
                        Err(err)
                    }
                }
                Err(e) => Err(DomainError::Network(e.to_string())),
            }
        };

        drop(guard);
        self.release_inflight(&disk_key, flight);
        outcome
    }

    /// Paginated fetch: memory -> disk -> network, with single-flight.
    async fn cached_paginated<T, F>(
        &self,
        cache_key: CacheKey,
        off: usize,
        lim: usize,
        fetch: F,
    ) -> Result<Page<T>, DomainError>
    where
        T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
        F: std::future::Future<Output = Result<Page<T>, DomainError>> + Send + 'static,
    {
        // Tier 1: Memory.
        if let Some(mut items) = self.json_cache.get_paginated::<T>(&cache_key, off) {
            items.truncate(lim);
            let total = self.json_cache.get_total(&cache_key).unwrap_or(0);
            return Ok(Page {
                items,
                offset: Some(off),
                total: Some(total),
                next_cursor: None,
            });
        }

        let disk_key = cache_key.disk_key();
        let flight_id = format!("{disk_key}#{off}#{lim}");
        let flight = self.inflight_lock(&flight_id);
        let guard = flight.lock().await;

        let outcome: Result<Page<T>, DomainError> = 'resolved: {
            // Re-check memory after acquiring the lock.
            if let Some(mut items) = self.json_cache.get_paginated::<T>(&cache_key, off) {
                items.truncate(lim);
                let total = self.json_cache.get_total(&cache_key).unwrap_or(0);
                break 'resolved Ok(Page {
                    items,
                    offset: Some(off),
                    total: Some(total),
                    next_cursor: None,
                });
            }

            // Tier 2: Disk - restore persisted pages into memory.
            if let Some(entry) = self.api_disk.read(&disk_key).await {
                if matches!(entry.state, crate::cache::disk::EntryState::Fresh) {
                    if let Ok(persisted) = serde_json::from_slice::<PersistedPage>(&entry.data) {
                        let total = persisted.total;
                        for (page_offset, values) in persisted.pages {
                            let items: Vec<T> = values
                                .iter()
                                .filter_map(|v| serde_json::from_value::<T>(v.clone()).ok())
                                .collect();
                            // Only restore pages that deserialized fully.
                            if items.len() == values.len() {
                                self.json_cache.append_paginated(
                                    &cache_key,
                                    page_offset,
                                    items,
                                    total,
                                );
                            }
                        }
                        if let Some(mut items) = self.json_cache.get_paginated::<T>(&cache_key, off)
                        {
                            items.truncate(lim);
                            break 'resolved Ok(Page {
                                items,
                                offset: Some(off),
                                total: Some(total),
                                next_cursor: None,
                            });
                        }
                    }
                }
            }

            // Tier 3: Network.
            let page = match tokio::spawn(fetch).await {
                Ok(Ok(p)) => p,
                Ok(Err(err)) => {
                    self.handle_auth_error(&err);
                    break 'resolved Err(err);
                }
                Err(e) => break 'resolved Err(DomainError::Network(e.to_string())),
            };

            let result = Page {
                items: page.items.clone(),
                offset: Some(off),
                total: Some(page.total.unwrap_or(0)),
                next_cursor: None,
            };

            // Deferred: update memory.
            self.json_cache.append_paginated(
                &cache_key,
                page.offset.unwrap_or(0),
                page.items.clone(),
                page.total.unwrap_or(0),
            );

            // Deferred: merge this page into the on-disk record.
            let disk = self.api_disk.clone();
            let dk = disk_key.clone();
            let page_items = page.items;
            let page_offset = page.offset.unwrap_or(0);
            let page_total = page.total.unwrap_or(0);
            tokio::spawn(async move {
                let mut pages: BTreeMap<usize, Vec<serde_json::Value>> = BTreeMap::new();
                if let Some(raw) = disk.read_raw(&dk).await {
                    if let Ok(prev) = serde_json::from_slice::<PersistedPage>(&raw) {
                        pages.extend(prev.pages);
                    }
                }
                let values: Vec<serde_json::Value> = page_items
                    .iter()
                    .filter_map(|item| serde_json::to_value(item).ok())
                    .collect();
                pages.insert(page_offset, values);
                let persisted = PersistedPage {
                    pages: pages.into_iter().collect(),
                    total: page_total,
                };
                if let Ok(bytes) = serde_json::to_vec(&persisted) {
                    disk.write(&dk, &bytes, API_TTL, None).await;
                }
            });

            Ok(result)
        };

        drop(guard);
        self.release_inflight(&flight_id, flight);
        outcome
    }

    pub async fn get_album(&self, id: &str) -> Result<Album, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_or_fetch(CacheKey::Album(id.clone()), async move {
            d.get_album(&id).await
        })
        .await
    }

    pub async fn get_album_tracks(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        let id2 = id.clone();
        let mut result = self
            .cached_paginated(
                CacheKey::AlbumTracks(id.clone()),
                offset,
                limit,
                async move { d.get_album_tracks(&id, offset, limit).await },
            )
            .await?;
        // Backfill album art into tracks that only have the placeholder.
        if result.items.iter().any(|t| t.art.is_resource()) {
            if let Ok(album) = self.get_album(&id2).await {
                if !album.art.is_resource() {
                    let art = album.art;
                    for track in result.items.iter_mut() {
                        if track.art.is_resource() {
                            track.art = art.clone();
                        }
                    }
                }
            }
        }
        Ok(result)
    }

    pub async fn get_saved_albums(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Album>, DomainError> {
        let d = Arc::clone(&self.provider);
        self.cached_paginated(CacheKey::SavedAlbums, offset, limit, async move {
            d.get_saved_albums(offset, limit).await
        })
        .await
    }

    pub async fn get_saved_tracks(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError> {
        let d = Arc::clone(&self.provider);
        self.cached_paginated(CacheKey::SavedTracks, offset, limit, async move {
            d.get_saved_tracks(offset, limit).await
        })
        .await
    }

    pub async fn get_saved_playlists(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Playlist>, DomainError> {
        let d = Arc::clone(&self.provider);
        self.cached_paginated(CacheKey::SavedPlaylists, offset, limit, async move {
            d.get_saved_playlists(offset, limit).await
        })
        .await
    }

    pub async fn get_playlist(&self, id: &str) -> Result<Playlist, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_or_fetch(CacheKey::Playlist(id.clone()), async move {
            d.get_playlist(&id).await
        })
        .await
    }

    pub async fn get_playlist_tracks(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_paginated(
            CacheKey::PlaylistTracks(id.clone()),
            offset,
            limit,
            async move { d.get_playlist_tracks(&id, offset, limit).await },
        )
        .await
    }

    pub async fn get_artist(&self, id: &str) -> Result<Artist, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_or_fetch(CacheKey::Artist(id.clone()), async move {
            d.get_artist(&id).await
        })
        .await
    }

    pub async fn get_artist_albums(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Album>, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_paginated(
            CacheKey::ArtistAlbums(id.clone()),
            offset,
            limit,
            async move { d.get_artist_albums(&id, offset, limit).await },
        )
        .await
    }

    pub async fn get_artist_top_tracks(&self, id: &str) -> Result<Vec<Track>, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_or_fetch(CacheKey::ArtistTopTracks(id.clone()), async move {
            d.get_artist_top_tracks(&id).await
        })
        .await
    }

    pub async fn search(
        &self,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResults, DomainError> {
        let d = Arc::clone(&self.provider);
        let query = query.to_string();
        tokio::spawn(async move { d.search(&query, offset, limit).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn search_scoped(
        &self,
        query: &str,
        kind: SearchType,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResults, DomainError> {
        let d = Arc::clone(&self.provider);
        let query = query.to_string();
        tokio::spawn(async move { d.search_scoped(&query, kind, offset, limit).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn get_user(&self, id: &str) -> Result<User, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_or_fetch(
            CacheKey::User(id.clone()),
            async move { d.get_user(&id).await },
        )
        .await
    }

    pub async fn get_current_user(&self) -> Result<User, DomainError> {
        let d = Arc::clone(&self.provider);
        tokio::spawn(async move { d.get_current_user().await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn get_track(&self, id: &str) -> Result<Track, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        tokio::spawn(async move { d.get_track(&id).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn get_user_playlists(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Playlist>, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_paginated(
            CacheKey::UserPlaylists(id.clone()),
            offset,
            limit,
            async move { d.get_user_playlists(&id, offset, limit).await },
        )
        .await
    }

    async fn invalidate_albums(&self, ids: &str) {
        for id in ids.split(',').filter(|s| !s.is_empty()) {
            self.json_cache.remove(&CacheKey::Album(id.to_string()));
            self.api_disk
                .invalidate(&CacheKey::Album(id.to_string()).disk_key())
                .await;
        }
    }

    pub async fn save_albums(&self, ids: &str) -> Result<(), DomainError> {
        self.json_cache.remove(&CacheKey::SavedAlbums);
        self.api_disk
            .invalidate(&CacheKey::SavedAlbums.disk_key())
            .await;
        self.invalidate_albums(ids).await;
        let d = Arc::clone(&self.provider);
        let ids = ids.to_string();
        tokio::spawn(async move { d.save_albums(&ids).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn remove_albums(&self, ids: &str) -> Result<(), DomainError> {
        self.json_cache.remove(&CacheKey::SavedAlbums);
        self.api_disk
            .invalidate(&CacheKey::SavedAlbums.disk_key())
            .await;
        self.invalidate_albums(ids).await;
        let d = Arc::clone(&self.provider);
        let ids = ids.to_string();
        tokio::spawn(async move { d.remove_albums(&ids).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn save_tracks(&self, ids: Vec<String>) -> Result<(), DomainError> {
        self.json_cache.remove(&CacheKey::SavedTracks);
        self.api_disk
            .invalidate(&CacheKey::SavedTracks.disk_key())
            .await;
        let d = Arc::clone(&self.provider);
        tokio::spawn(async move { d.save_tracks(ids).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn remove_tracks(&self, ids: Vec<String>) -> Result<(), DomainError> {
        self.json_cache.remove(&CacheKey::SavedTracks);
        self.api_disk
            .invalidate(&CacheKey::SavedTracks.disk_key())
            .await;
        let d = Arc::clone(&self.provider);
        tokio::spawn(async move { d.remove_tracks(ids).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn add_to_playlist(&self, id: &str, uris: Vec<String>) -> Result<(), DomainError> {
        self.json_cache
            .remove(&CacheKey::PlaylistTracks(id.to_string()));
        self.api_disk
            .invalidate(&CacheKey::Playlist(id.to_string()).disk_key())
            .await;
        self.api_disk
            .invalidate(&CacheKey::PlaylistTracks(id.to_string()).disk_key())
            .await;
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        tokio::spawn(async move { d.add_to_playlist(&id, uris, None).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn remove_from_playlist(
        &self,
        id: &str,
        uris: Vec<String>,
    ) -> Result<(), DomainError> {
        self.json_cache
            .remove(&CacheKey::PlaylistTracks(id.to_string()));
        self.api_disk
            .invalidate(&CacheKey::Playlist(id.to_string()).disk_key())
            .await;
        self.api_disk
            .invalidate(&CacheKey::PlaylistTracks(id.to_string()).disk_key())
            .await;
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        tokio::spawn(async move { d.remove_from_playlist(&id, uris, None).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn create_playlist(
        &self,
        user_id: &str,
        name: &str,
    ) -> Result<Playlist, DomainError> {
        self.json_cache.remove(&CacheKey::SavedPlaylists);
        self.api_disk
            .invalidate(&CacheKey::SavedPlaylists.disk_key())
            .await;
        let d = Arc::clone(&self.provider);
        let user_id = user_id.to_string();
        let name = name.to_string();
        tokio::spawn(async move { d.create_playlist(&user_id, &name, None, None, None).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn follow_playlist(&self, id: &str) -> Result<(), DomainError> {
        self.json_cache.remove(&CacheKey::SavedPlaylists);
        self.api_disk
            .invalidate(&CacheKey::SavedPlaylists.disk_key())
            .await;
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        tokio::spawn(async move { d.follow_playlist(&id).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn unfollow_playlist(&self, id: &str) -> Result<(), DomainError> {
        self.json_cache.remove(&CacheKey::SavedPlaylists);
        self.json_cache
            .remove(&CacheKey::PlaylistTracks(id.to_string()));
        self.api_disk
            .invalidate(&CacheKey::SavedPlaylists.disk_key())
            .await;
        self.api_disk
            .invalidate(&CacheKey::Playlist(id.to_string()).disk_key())
            .await;
        self.api_disk
            .invalidate(&CacheKey::PlaylistTracks(id.to_string()).disk_key())
            .await;
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        tokio::spawn(async move { d.unfollow_playlist(&id).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn update_playlist_details(&self, id: &str, name: &str) -> Result<(), DomainError> {
        self.json_cache
            .remove(&CacheKey::PlaylistTracks(id.to_string()));
        self.api_disk
            .invalidate(&CacheKey::Playlist(id.to_string()).disk_key())
            .await;
        self.api_disk
            .invalidate(&CacheKey::PlaylistTracks(id.to_string()).disk_key())
            .await;
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        let name = name.to_string();
        tokio::spawn(async move {
            d.update_playlist_details(&id, Some(&name), None, None, None)
                .await
        })
        .await
        .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn get_devices(&self) -> Result<Vec<Device>, DomainError> {
        let d = Arc::clone(&self.provider);
        tokio::spawn(async move { d.get_devices().await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn get_player_queue(&self) -> Result<Queue, DomainError> {
        let d = Arc::clone(&self.provider);
        tokio::spawn(async move { d.get_player_queue().await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn get_player_state(&self) -> Result<PlayerState, DomainError> {
        let d = Arc::clone(&self.provider);
        tokio::spawn(async move { d.get_player_state().await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn player_resume(&self, device_id: &str) -> Result<(), DomainError> {
        let d = Arc::clone(&self.provider);
        let device_id = device_id.to_string();
        tokio::spawn(async move { d.player_resume(&device_id).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn player_play_in_context(
        &self,
        device_id: &str,
        context_uri: &str,
        offset: usize,
    ) -> Result<(), DomainError> {
        let d = Arc::clone(&self.provider);
        let device_id = device_id.to_string();
        let context_uri = context_uri.to_string();
        tokio::spawn(async move {
            d.player_play_in_context(&device_id, &context_uri, offset)
                .await
        })
        .await
        .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn player_play_uris(
        &self,
        device_id: &str,
        uris: Vec<String>,
        offset: usize,
    ) -> Result<(), DomainError> {
        let d = Arc::clone(&self.provider);
        let device_id = device_id.to_string();
        tokio::spawn(async move { d.player_play_uris(&device_id, uris, offset).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn player_pause(&self, device_id: &str) -> Result<(), DomainError> {
        let d = Arc::clone(&self.provider);
        let device_id = device_id.to_string();
        tokio::spawn(async move { d.player_pause(&device_id).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn player_seek(&self, device_id: &str, position_ms: u32) -> Result<(), DomainError> {
        let d = Arc::clone(&self.provider);
        let device_id = device_id.to_string();
        tokio::spawn(async move { d.player_seek(&device_id, position_ms).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn player_repeat(
        &self,
        device_id: &str,
        mode: RepeatMode,
    ) -> Result<(), DomainError> {
        let d = Arc::clone(&self.provider);
        let device_id = device_id.to_string();
        tokio::spawn(async move { d.player_repeat(&device_id, mode).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn player_shuffle(&self, device_id: &str, state: bool) -> Result<(), DomainError> {
        let d = Arc::clone(&self.provider);
        let device_id = device_id.to_string();
        tokio::spawn(async move { d.player_shuffle(&device_id, state).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn player_volume(
        &self,
        device_id: &str,
        volume_percent: u8,
    ) -> Result<(), DomainError> {
        let d = Arc::clone(&self.provider);
        let device_id = device_id.to_string();
        tokio::spawn(async move { d.player_volume(&device_id, volume_percent).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn get_followed_artists(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<Artist>, Option<String>), DomainError> {
        let d = Arc::clone(&self.provider);
        let after = after.map(|s| s.to_string());
        tokio::spawn(async move { d.get_followed_artists(after.as_deref(), limit).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn follow_artists(&self, ids: &str) -> Result<(), DomainError> {
        let d = Arc::clone(&self.provider);
        let ids = ids.to_string();
        tokio::spawn(async move { d.follow_artists(&ids).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn unfollow_artists(&self, ids: &str) -> Result<(), DomainError> {
        let d = Arc::clone(&self.provider);
        let ids = ids.to_string();
        tokio::spawn(async move { d.unfollow_artists(&ids).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn check_following_artists(&self, ids: &str) -> Result<Vec<bool>, DomainError> {
        let d = Arc::clone(&self.provider);
        let ids = ids.to_string();
        tokio::spawn(async move { d.check_following_artists(&ids).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn check_saved_albums(&self, ids: &str) -> Result<Vec<bool>, DomainError> {
        let d = Arc::clone(&self.provider);
        let ids = ids.to_string();
        tokio::spawn(async move { d.check_saved_albums(&ids).await })
            .await
            .map_err(|e| DomainError::Network(e.to_string()))?
    }

    pub async fn check_following_artist(&self, id: &str) -> Result<bool, DomainError> {
        Ok(self
            .check_following_artists(id)
            .await?
            .first()
            .copied()
            .unwrap_or(false))
    }

    pub async fn check_saved_album(&self, id: &str) -> Result<bool, DomainError> {
        Ok(self
            .check_saved_albums(id)
            .await?
            .first()
            .copied()
            .unwrap_or(false))
    }

    fn image_key(url: &str, ext: &str) -> String {
        format!("{url}.{ext}")
    }

    /// Load an image: memory LRU -> disk -> CDN.
    pub async fn load_image(
        &self,
        url: &str,
        ext: &str,
        width: i32,
        height: i32,
    ) -> Option<gdk::Texture> {
        // Bundled placeholder: load from GResource, bypass caches.
        if is_resource_url(url) {
            return load_resource_texture(url, width, height);
        }

        let disk_key = Self::image_key(url, ext);
        let tex_key = format!("{disk_key}:{width}x{height}");

        // Tier 1: Memory LRU
        if let Some(texture) = self.texture_cache.get(&tex_key) {
            return Some(texture);
        }

        // Tier 2: Disk
        let bytes = if let Some(entry) = self.image_disk.read(&disk_key).await {
            entry.data
        } else {
            // Tier 3: Network
            let request = match isahc::http::Request::builder()
                .method("GET")
                .uri(url)
                .body(Vec::new())
            {
                Ok(request) => request,
                Err(e) => {
                    warn!("cdn: failed to build request for {url}: {e}");
                    return None;
                }
            };
            let response = match self.cdn_client.execute(request).await {
                Ok(response) => response,
                Err(e) => {
                    warn!("cdn: request for {url} failed: {e}");
                    return None;
                }
            };
            if !response.status.is_success() {
                warn!(
                    "cdn: {url} returned non-success status {} ({} bytes); no image",
                    response.status,
                    response.body.len()
                );
                return None;
            }
            let buf = response.body;
            // Empty body is a no-image, not retried.
            if buf.is_empty() {
                warn!(
                    "cdn: {url} returned success status {} but an EMPTY body; \
                     no image will be produced",
                    response.status
                );
                return None;
            }
            // Deferred: write to disk.
            let disk = self.image_disk.clone();
            let disk_key = disk_key.clone();
            let disk_buf = buf.clone();
            tokio::spawn(async move {
                disk.write(&disk_key, &disk_buf, IMAGE_TTL, None).await;
            });
            buf.into_boxed_slice()
        };

        let Some(texture) = crate::cache::decode_texture(&bytes, width, height) else {
            warn!(
                "cdn: failed to decode texture for {url} from {} bytes ({width}x{height}); no image",
                bytes.len()
            );
            return None;
        };
        let byte_size = (width as usize) * (height as usize) * 4;
        self.texture_cache
            .insert(tex_key, texture.clone(), byte_size);
        Some(texture)
    }

    /// Prefetch image bytes to disk without decoding a texture. Used by warming.
    pub async fn prefetch_image(&self, url: &str, ext: &str) {
        if is_resource_url(url) {
            return;
        }

        let key = Self::image_key(url, ext);

        // Already on disk - skip.
        if self.image_disk.read_raw(&key).await.is_some() {
            return;
        }

        let Ok(request) = isahc::http::Request::builder()
            .method("GET")
            .uri(url)
            .body(Vec::new())
        else {
            warn!("cdn: prefetch failed to build request for {url}");
            return;
        };
        let response = match self.cdn_client.execute(request).await {
            Ok(response) => response,
            Err(e) => {
                warn!("cdn: prefetch request for {url} failed: {e}");
                return;
            }
        };
        if !response.status.is_success() {
            warn!(
                "cdn: prefetch {url} returned non-success status {} ({} bytes)",
                response.status,
                response.body.len()
            );
            return;
        }
        if response.body.is_empty() {
            warn!(
                "cdn: prefetch {url} returned success status {} but an EMPTY body",
                response.status
            );
            return;
        }
        self.image_disk
            .write(&key, &response.body, IMAGE_TTL, None)
            .await;
    }
}

fn is_resource_url(url: &str) -> bool {
    url.starts_with("resource://")
}

fn load_resource_texture(url: &str, width: i32, height: i32) -> Option<gdk::Texture> {
    let path = url.strip_prefix("resource://")?;
    let pixbuf = gdk_pixbuf::Pixbuf::from_resource_at_scale(path, width, height, true).ok()?;
    Some(gdk::Texture::for_pixbuf(&pixbuf))
}
