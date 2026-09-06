//! Shared constants and configuration values for the Riff workspace.
//!
//! This crate is the single source of truth for tunable values that are
//! referenced across multiple crates. Centralizing them here makes it easy to
//! find, adjust, and keep consistent.

use std::time::Duration;

/// File extension appended to cache entries for storing expiry metadata.
pub const EXPIRY_EXT: &str = ".expiry";

/// Default maximum total disk cache size in bytes (enforced on shutdown).
pub const DEFAULT_MAX_BYTES: u64 = 128 * 1024 * 1024; // 128 MB

/// Estimated bytes per cached item in the in-memory LRU store.
pub const BYTES_PER_ITEM: usize = 512;

/// TTL for cached images (covers, artwork).
pub const IMAGE_TTL: Duration = Duration::from_secs(48 * 3600);

/// TTL for cached API responses (metadata, lists).
pub const API_TTL: Duration = Duration::from_secs(48 * 3600);

/// How long a stale entry served via stale-if-error stays valid before we try
/// the network again, so a persistent outage doesn't hammer the API.
pub const STALE_IF_ERROR_TTL: Duration = Duration::from_secs(60);

/// Number of albums to warm on startup. Matches the homepage's first page
/// (`CARD_BATCH_SIZE`).
pub const WARM_LIST_LIMIT: usize = 50;

/// Width the home/library cards request. Used only to pick which CDN image URL
/// to prefetch, so the warmed bytes match exactly what the cards ask for.
pub const WARM_IMAGE_WIDTH: u32 = 180;

/// File extension for cover art (the Spotify CDN serves JPEG).
pub const WARM_IMAGE_EXT: &str = "jpg";

/// Cap on concurrent cover downloads. Sized to the CDN pool's
/// `max_connections_per_host`, so warming keeps every connection busy without
/// queuing a large backlog of parked requests.
pub const WARM_IMAGE_CONCURRENCY: usize = 16;

/// How long (in seconds) to reuse a cached Spotify Configuration before
/// re-reading from the token provider.
pub const CONFIG_CACHE_TTL_SECS: u64 = 120;
