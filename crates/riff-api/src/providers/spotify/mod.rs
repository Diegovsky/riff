//! Spotify vendor implementation.

use std::sync::Arc;

use crate::service::ApiService;
use crate::token::TokenProvider;

pub mod domain;

mod converter;

use domain::SpotifyDomain;

pub fn spotify_service(
    token_provider: Arc<dyn TokenProvider>,
    memory_cache_bytes: usize,
    disk_cache_bytes: usize,
) -> ApiService {
    let provider = Arc::new(SpotifyDomain::new(Arc::clone(&token_provider)));
    ApiService::new(
        provider,
        token_provider,
        memory_cache_bytes,
        disk_cache_bytes,
    )
}
