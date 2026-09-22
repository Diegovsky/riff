//! Tunables for the api layer (`riff-api`).
//!
//! Grouped by what you would be tuning: caching, how requests reach the
//! network, and how competing requests are ordered.
//!
//! Cache warming has nothing here on purpose. Every value it needs has to match
//! what the UI will later ask for, so the UI passes them in.

use std::time::Duration;

// Caching

/// Estimated bytes per cached item in the in-memory LRU. Budget accounting
/// only, so it need not be exact.
pub const BYTES_PER_ITEM: usize = 512;

/// Appended to disk cache entries that hold expiry metadata.
pub const EXPIRY_EXT: &str = ".expiry";

/// Part of every image cache key. The Spotify CDN serves JPEG for all artwork.
pub const IMAGE_EXT: &str = "jpg";

pub const IMAGE_TTL: Duration = Duration::from_secs(48 * 3600);

pub const API_TTL: Duration = Duration::from_secs(48 * 3600);

/// How long a stale-if-error entry stays valid before retrying the network, so
/// an outage doesn't get hammered.
pub const STALE_IF_ERROR_TTL: Duration = Duration::from_secs(60);

// Requests
//
// Transport-level limits. Two clients: an isahc pool for the image CDN and a
// reqwest client for the JSON API. Configured separately, but neither is
// allowed to go untimed.

/// Max simultaneous connections the shared HTTP pool opens to a single host.
/// Every concurrency cap under "Scheduling" is sized against this.
pub const CDN_MAX_CONNECTIONS_PER_HOST: usize = 32;

/// Long enough to reuse across a burst of image loads.
pub const POOL_TCP_KEEPALIVE: Duration = Duration::from_secs(30);

/// Connect deadline, as opposed to whole-request. Short: a failed connect is
/// usually a hard network problem. Shared by both clients.
pub const POOL_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Whole-request deadline for the image CDN. Generous, since cover art on a
/// slow link is worth waiting for.
pub const CDN_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Whole-request deadline for the JSON API. Tighter than
/// [`CDN_REQUEST_TIMEOUT`] since a read holds a read-queue slot while it runs.
pub const API_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Sized to `API_READ_CONCURRENCY`, since the API is a single host.
pub const API_MAX_IDLE_CONNECTIONS_PER_HOST: usize = 8;

pub const CDN_RETRY_MAX_ATTEMPTS: u32 = 3;

/// Grows by `CDN_RETRY_BACKOFF_MULTIPLIER` on each attempt.
pub const CDN_RETRY_INITIAL_BACKOFF: Duration = Duration::from_millis(200);

pub const CDN_RETRY_BACKOFF_MULTIPLIER: u32 = 2;

/// How long to reuse a cached Spotify config before re-reading the token.
pub const CONFIG_CACHE_TTL: Duration = Duration::from_secs(120);

// Scheduling
//
// All image work shares one cap, so warming cannot crowd out a card the user is
// looking at: priority decides who gets a slot, not a separate budget.

/// Cap on concurrent image fetches, interactive and background alike. Below
/// `CDN_MAX_CONNECTIONS_PER_HOST` so the pool is never the binding constraint.
pub const IMAGE_LOAD_CONCURRENCY: usize = 24;

/// How many image requests may wait for a slot. Beyond this the queue drops
/// its least relevant waiter; see `AdmissionQueue::admit`.
pub const IMAGE_QUEUE_CAP: usize = 256;

/// Cap on concurrent API reads, so a screen that fires several fetches at once
/// does not trip the provider's rate limiter.
pub const API_READ_CONCURRENCY: usize = 8;

/// Backlog cap for queued API reads, mirroring `IMAGE_QUEUE_CAP`. Smaller
/// because a screen queues a handful of reads, not hundreds of images.
pub const API_QUEUE_CAP: usize = 64;

/// How many warm image loads run at once. Not a second budget: these still
/// compete for `IMAGE_LOAD_CONCURRENCY`. It just bounds how many park in the
/// queue at once.
pub const WARM_IMAGE_BUFFER: usize = 8;
