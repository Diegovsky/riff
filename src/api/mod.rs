mod api_models;
mod cached_client;
mod client;

pub mod cache;

pub use cached_client::{CachedSpotifyClient, SpotifyApiClient, SpotifyResult};
pub use client::SpotifyApiError;

#[cfg(debug_assertions)]
pub use client::{is_simulate_offline, set_simulate_offline};

#[cfg(debug_assertions)]
pub use client::set_injected_error;

use crate::auth::TokenStore;
use client::SpotifyClient;

pub async fn clear_user_cache() -> Option<()> {
    cache::CacheManager::for_dir("riff/net")?
        .clear_cache_pattern(&cached_client::USER_CACHE)
        .await
        .ok()
}

/// Result of checking the user's profile at login time.
pub struct UserProfileCheck {
    pub is_premium: bool,
    pub explicit_filter_enabled: bool,
    pub explicit_filter_locked: bool,
}

/// Check the user's profile via GET /v1/me.
/// Returns premium status and whether the account has the explicit content
/// filter enabled/locked.
///
/// Returns Ok with the profile info, or Err if the API call itself failed.
pub async fn check_user_profile(token: &str) -> Result<UserProfileCheck, SpotifyApiError> {
    debug!("Checking user profile via GET /v1/me");
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
        "GET /v1/me succeeded: id={}, display_name={}, product={:?}, explicit_filter={:?}",
        user.id,
        user.display_name,
        user.product,
        user.explicit_content.as_ref().map(|e| e.filter_enabled)
    );

    let is_premium = match user.product.as_deref() {
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
    };

    let (explicit_filter_enabled, explicit_filter_locked) = match &user.explicit_content {
        Some(ec) => (ec.filter_enabled, ec.filter_locked),
        None => (false, false),
    };

    Ok(UserProfileCheck {
        is_premium,
        explicit_filter_enabled,
        explicit_filter_locked,
    })
}
