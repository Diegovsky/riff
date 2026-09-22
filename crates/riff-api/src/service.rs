//! ApiService - orchestrates caching around domain calls.
//!
//! Tier order: memory LRU -> disk cache -> network.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use serde::{de::DeserializeOwned, Serialize};

use riff_config::api::{
    API_QUEUE_CAP, API_READ_CONCURRENCY, API_TTL, IMAGE_EXT, IMAGE_LOAD_CONCURRENCY,
    IMAGE_QUEUE_CAP, IMAGE_TTL, STALE_IF_ERROR_TTL,
};

use gdk::prelude::TextureExt;

use crate::cache::{CacheKey, DiskCache, Store, TextureCache};
use crate::error::DomainError;
use crate::http;
use crate::models::*;
use crate::providers::MusicProvider;
use crate::scheduler::{Admission, AdmissionQueue, Lane, Load, Slot};

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
    inflight: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    image_queue: AdmissionQueue,
    read_queue: AdmissionQueue,
    write_lane: Lane,
    player_lane: Lane,
}

impl ApiService {
    /// Build a service around a provider implementation. Crate-private: services
    /// are constructed through a vendor factory (e.g. `spotify_service`).
    pub(crate) fn new(
        provider: Arc<dyn MusicProvider>,
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
            inflight: Mutex::new(HashMap::new()),
            image_queue: AdmissionQueue::new(
                "image queue",
                IMAGE_LOAD_CONCURRENCY,
                IMAGE_QUEUE_CAP,
            ),
            read_queue: AdmissionQueue::new("api read queue", API_READ_CONCURRENCY, API_QUEUE_CAP),
            // A dropped caller leaves the cache stale and the change unsent,
            // but no caller cancels today.
            write_lane: Lane::new(1),
            player_lane: Lane::new(1),
        }
    }

    async fn admit_read(&self, load: Load) -> Result<Option<Slot>, DomainError> {
        match self.read_queue.admit(load).await {
            Admission::Granted(slot) => Ok(Some(slot)),
            Admission::Unslotted => Ok(None),
            Admission::Denied => Err(DomainError::Shed),
        }
    }

    /// Evict both disk caches to their budgets. Call on shutdown.
    pub async fn run_cache_maintenance(&self) {
        self.api_disk.evict_to_budget().await;
        self.image_disk.evict_to_budget().await;
    }

    /// Clear all cached data, on disk (API responses + images) and in memory
    /// (API response cache + decoded textures).
    pub async fn clear_user_cache(&self) {
        self.json_cache.clear();
        self.texture_cache.clear();
        self.api_disk.clear().await;
        self.image_disk.clear().await;
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

    /// Single-resource fetch: memory -> disk -> queue -> network, with
    /// single-flight.
    async fn cached_or_fetch<T, F>(
        &self,
        cache_key: CacheKey,
        load: Load,
        fetch: F,
    ) -> Result<T, DomainError>
    where
        T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
        F: std::future::Future<Output = Result<T, DomainError>> + Send,
    {
        // Tier 1: Memory
        if let Some(val) = self.json_cache.get_single::<T>(&cache_key) {
            return Ok(val);
        }

        let disk_key = cache_key.disk_key();
        let flight = self.inflight_lock(&disk_key);

        let outcome: Result<T, DomainError> = 'resolved: {
            // Tier 2: Disk, one caller at a time per key.
            let mut stale: Option<(T, Option<String>)> = None;
            {
                let _guard = flight.lock().await;

                if let Some(val) = self.json_cache.get_single::<T>(&cache_key) {
                    break 'resolved Ok(val);
                }

                if let Some(entry) = self.api_disk.read(&disk_key).await {
                    if let Ok(val) = serde_json::from_slice::<T>(&entry.data) {
                        match entry.state {
                            crate::cache::disk::EntryState::Fresh => {
                                debug!("api: {disk_key} served from disk, no request made");
                                self.json_cache.insert_single(&cache_key, val.clone());
                                break 'resolved Ok(val);
                            }
                            crate::cache::disk::EntryState::Stale { etag } => {
                                debug!("api: {disk_key} disk copy is stale, refetching");
                                stale = Some((val, etag));
                            }
                        }
                    }
                }
            }

            // Tier 3: Network.
            let mut slot = match self.admit_read(load).await {
                Ok(slot) => slot,
                Err(err) => break 'resolved Err(err),
            };
            let _guard = match flight.try_lock() {
                Ok(guard) => guard,
                Err(_) => {
                    drop(slot);
                    let guard = flight.lock().await;
                    slot = match self.admit_read(load).await {
                        Ok(slot) => slot,
                        Err(err) => break 'resolved Err(err),
                    };
                    guard
                }
            };
            let _slot = slot;

            // Another caller may have fetched this while we waited.
            if let Some(val) = self.json_cache.get_single::<T>(&cache_key) {
                break 'resolved Ok(val);
            }

            match fetch.await {
                Ok(val) => {
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
                Err(err) => {
                    self.handle_auth_error(&err);
                    // Stale-if-error: serve stale copy on transient failure.
                    let serving_stale = stale.is_some() && err.is_transient();
                    log_fetch_failure(&disk_key, &err, serving_stale);
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
            }
        };

        self.release_inflight(&disk_key, flight);
        outcome
    }

    /// One page of `lim` items at `off`, if memory already holds it.
    fn cached_page<T>(&self, cache_key: &CacheKey, off: usize, lim: usize) -> Option<Page<T>>
    where
        T: Clone + Send + Sync + 'static,
    {
        let mut items = self.json_cache.get_paginated::<T>(cache_key, off)?;
        items.truncate(lim);
        Some(Page {
            items,
            offset: Some(off),
            total: Some(self.json_cache.get_total(cache_key).unwrap_or(0)),
            next_cursor: None,
        })
    }

    /// Paginated fetch: memory -> disk -> network, with single-flight.
    async fn cached_paginated<T, F>(
        &self,
        cache_key: CacheKey,
        off: usize,
        lim: usize,
        load: Load,
        fetch: F,
    ) -> Result<Page<T>, DomainError>
    where
        T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
        F: std::future::Future<Output = Result<Page<T>, DomainError>> + Send,
    {
        // Tier 1: Memory.
        if let Some(page) = self.cached_page::<T>(&cache_key, off, lim) {
            return Ok(page);
        }

        let disk_key = cache_key.disk_key();
        let flight_id = format!("{disk_key}#{off}#{lim}");
        let flight = self.inflight_lock(&flight_id);

        let outcome: Result<Page<T>, DomainError> = 'resolved: {
            // Tier 2: Disk. See `cached_or_fetch` for the lock dance.
            {
                let _guard = flight.lock().await;

                if let Some(page) = self.cached_page::<T>(&cache_key, off, lim) {
                    break 'resolved Ok(page);
                }

                if let Some(entry) = self.api_disk.read(&disk_key).await {
                    if matches!(entry.state, crate::cache::disk::EntryState::Fresh) {
                        if let Ok(persisted) = serde_json::from_slice::<PersistedPage>(&entry.data)
                        {
                            let total = persisted.total;
                            for (page_offset, values) in persisted.pages {
                                let items: Vec<T> = values
                                    .iter()
                                    .filter_map(|v| serde_json::from_value::<T>(v.clone()).ok())
                                    .collect();
                                if items.len() == values.len() {
                                    self.json_cache.append_paginated(
                                        &cache_key,
                                        page_offset,
                                        items,
                                        total,
                                    );
                                }
                            }
                            if let Some(page) = self.cached_page::<T>(&cache_key, off, lim) {
                                debug!("api: {disk_key} served from disk, no request made");
                                break 'resolved Ok(page);
                            }
                        }
                    }
                }
            }

            // Tier 3: Network.
            let mut slot = match self.admit_read(load).await {
                Ok(slot) => slot,
                Err(err) => break 'resolved Err(err),
            };
            let _guard = match flight.try_lock() {
                Ok(guard) => guard,
                Err(_) => {
                    drop(slot);
                    let guard = flight.lock().await;
                    slot = match self.admit_read(load).await {
                        Ok(slot) => slot,
                        Err(err) => break 'resolved Err(err),
                    };
                    guard
                }
            };
            let _slot = slot;

            // Another caller may have filled this page while we waited.
            if let Some(page) = self.cached_page::<T>(&cache_key, off, lim) {
                break 'resolved Ok(page);
            }

            let page = match fetch.await {
                Ok(p) => p,
                Err(err) => {
                    self.handle_auth_error(&err);
                    log_fetch_failure(&flight_id, &err, false);
                    break 'resolved Err(err);
                }
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

        self.release_inflight(&flight_id, flight);
        outcome
    }

    pub async fn get_album(&self, id: &str, load: Load) -> Result<Album, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_or_fetch(CacheKey::Album(id.clone()), load, async move {
            d.get_album(&id).await
        })
        .await
    }

    pub async fn get_album_tracks(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
        load: Load,
    ) -> Result<Page<Track>, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        let id2 = id.clone();
        let mut result = self
            .cached_paginated(
                CacheKey::AlbumTracks(id.clone()),
                offset,
                limit,
                load,
                async move { d.get_album_tracks(&id, offset, limit).await },
            )
            .await?;

        if result.items.iter().any(|t| t.art.is_resource()) {
            let art = match self
                .json_cache
                .get_single::<Album>(&CacheKey::Album(id2.clone()))
            {
                Some(album) if !album.art.is_resource() => Some(album.art),
                Some(_) => None,
                None => match self.get_album(&id2, load).await {
                    Ok(album) if !album.art.is_resource() => Some(album.art),
                    _ => None,
                },
            };
            if let Some(art) = art {
                for track in result.items.iter_mut() {
                    if track.art.is_resource() {
                        track.art = art.clone();
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
        load: Load,
    ) -> Result<Page<Album>, DomainError> {
        let d = Arc::clone(&self.provider);
        self.cached_paginated(CacheKey::SavedAlbums, offset, limit, load, async move {
            d.get_saved_albums(offset, limit).await
        })
        .await
    }

    pub async fn get_saved_tracks(
        &self,
        offset: usize,
        limit: usize,
        load: Load,
    ) -> Result<Page<Track>, DomainError> {
        let d = Arc::clone(&self.provider);
        self.cached_paginated(CacheKey::SavedTracks, offset, limit, load, async move {
            d.get_saved_tracks(offset, limit).await
        })
        .await
    }

    pub async fn get_saved_playlists(
        &self,
        offset: usize,
        limit: usize,
        load: Load,
    ) -> Result<Page<Playlist>, DomainError> {
        let d = Arc::clone(&self.provider);
        self.cached_paginated(CacheKey::SavedPlaylists, offset, limit, load, async move {
            d.get_saved_playlists(offset, limit).await
        })
        .await
    }

    pub async fn get_playlist(&self, id: &str, load: Load) -> Result<Playlist, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_or_fetch(CacheKey::Playlist(id.clone()), load, async move {
            d.get_playlist(&id).await
        })
        .await
    }

    pub async fn get_playlist_tracks(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
        load: Load,
    ) -> Result<Page<Track>, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_paginated(
            CacheKey::PlaylistTracks(id.clone()),
            offset,
            limit,
            load,
            async move { d.get_playlist_tracks(&id, offset, limit).await },
        )
        .await
    }

    pub async fn get_artist(&self, id: &str, load: Load) -> Result<Artist, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_or_fetch(CacheKey::Artist(id.clone()), load, async move {
            d.get_artist(&id).await
        })
        .await
    }

    pub async fn get_artist_albums(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
        load: Load,
    ) -> Result<Page<Album>, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_paginated(
            CacheKey::ArtistAlbums(id.clone()),
            offset,
            limit,
            load,
            async move { d.get_artist_albums(&id, offset, limit).await },
        )
        .await
    }

    pub async fn get_artist_top_tracks(
        &self,
        id: &str,
        load: Load,
    ) -> Result<Vec<Track>, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_or_fetch(CacheKey::ArtistTopTracks(id.clone()), load, async move {
            d.get_artist_top_tracks(&id).await
        })
        .await
    }

    pub async fn search(
        &self,
        query: &str,
        offset: usize,
        limit: usize,
        load: Load,
    ) -> Result<SearchResults, DomainError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(SearchResults::default());
        }
        let _slot = self.admit_read(load).await?;
        self.provider.search(query, offset, limit).await
    }

    pub async fn search_scoped(
        &self,
        query: &str,
        kind: SearchType,
        offset: usize,
        limit: usize,
        load: Load,
    ) -> Result<SearchResults, DomainError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(SearchResults::default());
        }
        let _slot = self.admit_read(load).await?;
        self.provider
            .search_scoped(query, kind, offset, limit)
            .await
    }

    pub async fn get_user(&self, id: &str, load: Load) -> Result<User, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_or_fetch(CacheKey::User(id.clone()), load, async move {
            d.get_user(&id).await
        })
        .await
    }

    pub async fn get_current_user(&self, load: Load) -> Result<User, DomainError> {
        let _slot = self.admit_read(load).await?;
        self.provider.get_current_user().await
    }

    pub async fn get_track(&self, id: &str, load: Load) -> Result<Track, DomainError> {
        let _slot = self.admit_read(load).await?;
        self.provider.get_track(id).await
    }

    pub async fn get_user_playlists(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
        load: Load,
    ) -> Result<Page<Playlist>, DomainError> {
        let d = Arc::clone(&self.provider);
        let id = id.to_string();
        self.cached_paginated(
            CacheKey::UserPlaylists(id.clone()),
            offset,
            limit,
            load,
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
        let _lane = self.write_lane.enter().await;
        self.provider.save_albums(ids).await
    }

    pub async fn remove_albums(&self, ids: &str) -> Result<(), DomainError> {
        self.json_cache.remove(&CacheKey::SavedAlbums);
        self.api_disk
            .invalidate(&CacheKey::SavedAlbums.disk_key())
            .await;
        self.invalidate_albums(ids).await;
        let _lane = self.write_lane.enter().await;
        self.provider.remove_albums(ids).await
    }

    pub async fn save_tracks(&self, ids: Vec<String>) -> Result<(), DomainError> {
        self.json_cache.remove(&CacheKey::SavedTracks);
        self.api_disk
            .invalidate(&CacheKey::SavedTracks.disk_key())
            .await;
        let _lane = self.write_lane.enter().await;
        self.provider.save_tracks(ids).await
    }

    pub async fn remove_tracks(&self, ids: Vec<String>) -> Result<(), DomainError> {
        self.json_cache.remove(&CacheKey::SavedTracks);
        self.api_disk
            .invalidate(&CacheKey::SavedTracks.disk_key())
            .await;
        let _lane = self.write_lane.enter().await;
        self.provider.remove_tracks(ids).await
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
        let _lane = self.write_lane.enter().await;
        self.provider.add_to_playlist(id, uris, None).await
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
        let _lane = self.write_lane.enter().await;
        self.provider.remove_from_playlist(id, uris, None).await
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
        let _lane = self.write_lane.enter().await;
        self.provider
            .create_playlist(user_id, name, None, None, None)
            .await
    }

    pub async fn follow_playlist(&self, id: &str) -> Result<(), DomainError> {
        self.json_cache.remove(&CacheKey::SavedPlaylists);
        self.api_disk
            .invalidate(&CacheKey::SavedPlaylists.disk_key())
            .await;
        let _lane = self.write_lane.enter().await;
        self.provider.follow_playlist(id).await
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
        let _lane = self.write_lane.enter().await;
        self.provider.unfollow_playlist(id).await
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
        let _lane = self.write_lane.enter().await;
        self.provider
            .update_playlist_details(id, Some(name), None, None, None)
            .await
    }

    pub async fn get_devices(&self, load: Load) -> Result<Vec<Device>, DomainError> {
        let _slot = self.admit_read(load).await?;
        self.provider.get_devices().await
    }

    pub async fn get_player_queue(&self, load: Load) -> Result<Queue, DomainError> {
        let _slot = self.admit_read(load).await?;
        self.provider.get_player_queue().await
    }

    pub async fn get_player_state(&self, load: Load) -> Result<PlayerState, DomainError> {
        let _slot = self.admit_read(load).await?;
        self.provider.get_player_state().await
    }

    pub async fn player_resume(&self, device_id: &str) -> Result<(), DomainError> {
        let _lane = self.player_lane.enter().await;
        self.provider.player_resume(device_id).await
    }

    pub async fn player_play_in_context(
        &self,
        device_id: &str,
        context_uri: &str,
        offset: usize,
    ) -> Result<(), DomainError> {
        let _lane = self.player_lane.enter().await;
        self.provider
            .player_play_in_context(device_id, context_uri, offset)
            .await
    }

    pub async fn player_play_uris(
        &self,
        device_id: &str,
        uris: Vec<String>,
        offset: usize,
    ) -> Result<(), DomainError> {
        let _lane = self.player_lane.enter().await;
        self.provider
            .player_play_uris(device_id, uris, offset)
            .await
    }

    pub async fn player_pause(&self, device_id: &str) -> Result<(), DomainError> {
        let _lane = self.player_lane.enter().await;
        self.provider.player_pause(device_id).await
    }

    pub async fn player_seek(&self, device_id: &str, position_ms: u32) -> Result<(), DomainError> {
        let _lane = self.player_lane.enter().await;
        self.provider.player_seek(device_id, position_ms).await
    }

    pub async fn player_repeat(
        &self,
        device_id: &str,
        mode: RepeatMode,
    ) -> Result<(), DomainError> {
        let _lane = self.player_lane.enter().await;
        self.provider.player_repeat(device_id, mode).await
    }

    pub async fn player_shuffle(&self, device_id: &str, state: bool) -> Result<(), DomainError> {
        let _lane = self.player_lane.enter().await;
        self.provider.player_shuffle(device_id, state).await
    }

    pub async fn player_volume(
        &self,
        device_id: &str,
        volume_percent: u8,
    ) -> Result<(), DomainError> {
        let _lane = self.player_lane.enter().await;
        self.provider.player_volume(device_id, volume_percent).await
    }

    pub async fn get_followed_artists(
        &self,
        after: Option<&str>,
        limit: usize,
        load: Load,
    ) -> Result<(Vec<Artist>, Option<String>), DomainError> {
        let _slot = self.admit_read(load).await?;
        self.provider.get_followed_artists(after, limit).await
    }

    pub async fn follow_artists(&self, ids: &str) -> Result<(), DomainError> {
        let _lane = self.write_lane.enter().await;
        self.provider.follow_artists(ids).await
    }

    pub async fn unfollow_artists(&self, ids: &str) -> Result<(), DomainError> {
        let _lane = self.write_lane.enter().await;
        self.provider.unfollow_artists(ids).await
    }

    pub async fn check_following_artists(
        &self,
        ids: &str,
        load: Load,
    ) -> Result<Vec<bool>, DomainError> {
        let _slot = self.admit_read(load).await?;
        self.provider.check_following_artists(ids).await
    }

    pub async fn check_saved_albums(
        &self,
        ids: &str,
        load: Load,
    ) -> Result<Vec<bool>, DomainError> {
        let _slot = self.admit_read(load).await?;
        self.provider.check_saved_albums(ids).await
    }

    pub async fn check_following_artist(&self, id: &str, load: Load) -> Result<bool, DomainError> {
        Ok(self
            .check_following_artists(id, load)
            .await?
            .first()
            .copied()
            .unwrap_or(false))
    }

    pub async fn check_saved_album(&self, id: &str, load: Load) -> Result<bool, DomainError> {
        Ok(self
            .check_saved_albums(id, load)
            .await?
            .first()
            .copied()
            .unwrap_or(false))
    }

    fn image_key(url: &str) -> String {
        format!("{url}.{IMAGE_EXT}")
    }

    /// Load an image: memory LRU -> disk -> CDN.
    ///
    /// `load` carries the epoch from when the caller decided it needed this
    /// image, since reading it later would stamp a deferred off-screen cover
    /// with whatever view is current by then.
    pub async fn load_image(
        &self,
        url: &str,
        width: i32,
        height: i32,
        load: Load,
    ) -> Option<gdk::Texture> {
        // Bundled placeholder: load from GResource, bypass caches.
        if is_resource_url(url) {
            return load_resource_texture(url, width, height);
        }

        let disk_key = Self::image_key(url);
        let tex_key = format!("{disk_key}:{width}x{height}");

        // Tier 1: Memory LRU
        if let Some(texture) = self.texture_cache.get(&tex_key) {
            return Some(texture);
        }

        // Tier 2: Disk
        let bytes = if let Some(entry) = self.image_disk.read(&disk_key).await {
            entry.data
        } else {
            // Tier 3: Network, gated so interactive loads preempt background ones.
            let _slot = match self.image_queue.admit(load).await {
                Admission::Granted(slot) => Some(slot),
                Admission::Unslotted => None,
                Admission::Denied => {
                    debug!("cdn: image queue is full, skipping {url}");
                    return None;
                }
            };

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

        // Decode on a blocking thread so it does not stall the frame clock.
        let byte_len = bytes.len();
        let decoded = tokio::task::spawn_blocking(move || {
            crate::cache::decode_texture(&bytes, width, height)
        })
        .await;

        let texture = match decoded {
            Ok(Some(texture)) => texture,
            Ok(None) => {
                warn!(
                    "cdn: failed to decode texture for {url} from {byte_len} bytes \
                     ({width}x{height}); no image"
                );
                return None;
            }
            Err(e) => {
                warn!("cdn: decode task for {url} failed: {e}");
                return None;
            }
        };

        let tex_w = texture.width().max(0) as usize;
        let tex_h = texture.height().max(0) as usize;
        let byte_size = tex_w * tex_h * 4;
        self.texture_cache
            .insert(tex_key, texture.clone(), byte_size);
        Some(texture)
    }
}

fn is_resource_url(url: &str) -> bool {
    url.starts_with("resource://")
}

/// Log a network fetch that failed.
fn log_fetch_failure(key: &str, err: &DomainError, serving_stale: bool) {
    if matches!(
        err,
        DomainError::NoToken | DomainError::AuthExpired | DomainError::Shed
    ) {
        return;
    }
    #[cfg(debug_assertions)]
    if crate::dev::is_simulate_offline() {
        return;
    }
    if serving_stale {
        warn!(
            "api: {key} fetch failed, serving the stale cached copy for {}s: {err}",
            STALE_IF_ERROR_TTL.as_secs()
        );
    } else if err.is_transient() {
        warn!("api: {key} fetch failed with no cached copy to fall back on: {err}");
    } else {
        error!("api: {key} fetch failed with no cached copy to fall back on: {err}");
    }
}

fn load_resource_texture(url: &str, width: i32, height: i32) -> Option<gdk::Texture> {
    let path = url.strip_prefix("resource://")?;
    let pixbuf = gdk_pixbuf::Pixbuf::from_resource_at_scale(path, width, height, true).ok()?;
    Some(gdk::Texture::for_pixbuf(&pixbuf))
}
