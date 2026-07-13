mod api_models;
mod cached_client;
mod client;

pub mod cache;

pub use cached_client::{CachedSpotifyClient, SpotifyApiClient, SpotifyResult};
pub use client::SpotifyApiError;

use crate::auth::TokenStore;
use client::SpotifyClient;

pub async fn clear_user_cache() -> Option<()> {
    cache::CacheManager::for_dir("riff/net")?
        .clear_cache_pattern(&cached_client::USER_CACHE)
        .await
        .ok()
}

/// Check whether the given access token belongs to a Spotify Premium account.
/// This uses the standard API client infrastructure but authenticates with an
/// explicit token (for use during login before credentials are persisted).
pub async fn check_premium(token: &str) -> Result<bool, SpotifyApiError> {
    let client = SpotifyClient::new(TokenStore::new());
    let response = client.get_me().send_with_token(token).await?;
    let user: api_models::User = response.deserialize().ok_or(SpotifyApiError::NoContent)?;
    Ok(user.product.as_deref() == Some("premium"))
}
