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
///
/// Returns Ok(true) for premium, Ok(false) for confirmed non-premium, or an
/// Err if the API call itself failed (network, auth, rate-limit, etc.).
pub async fn check_premium(token: &str) -> Result<bool, SpotifyApiError> {
    debug!("Checking premium status via GET /v1/me");
    let client = SpotifyClient::new(TokenStore::new());
    let response = match client.get_me().send_with_token(token).await {
        Ok(r) => r,
        Err(e) => {
            error!("GET /v1/me request failed: {e:?}");
            return Err(e);
        }
    };
    let user: api_models::User = match response.deserialize() {
        Some(u) => u,
        None => {
            error!("GET /v1/me returned success but body could not be deserialized");
            return Err(SpotifyApiError::NoContent);
        }
    };
    debug!(
        "GET /v1/me succeeded: id={}, display_name={}, product={:?}",
        user.id, user.display_name, user.product
    );
    Ok(match user.product.as_deref() {
        Some("premium") => true,
        // The product field may be absent if the Spotify app is in Development
        // Mode (removed in the Feb 2026 API changes). Treat absent as premium
        // and let librespot enforce the actual restriction at session creation.
        None => true,
        // Explicitly non-premium (e.g. "free", "open")
        Some(other) => {
            error!("Account product type is {:?}, not premium", other);
            false
        }
    })
}
