//! Spotify vendor implementation.

use std::sync::Arc;

use crate::service::ApiService;
use crate::token::TokenProvider;

mod converter;
pub mod domain;
mod internal_api;
mod request;

use domain::SpotifyDomain;

pub fn spotify_service(
    token_provider: Arc<dyn TokenProvider>,
    session_token_provider: Arc<dyn TokenProvider>,
    memory_cache_bytes: usize,
    disk_cache_bytes: usize,
) -> ApiService {
    let provider = Arc::new(SpotifyDomain::new(token_provider, session_token_provider));
    ApiService::new(provider, memory_cache_bytes, disk_cache_bytes)
}
