mod api_models;
mod cached_client;
pub mod client;

pub mod cache;

use std::time::Instant;

pub use cached_client::{CachedSpotifyClient, SpotifyApiClient, SpotifyResult};
pub use client::SpotifyApiError;

pub async fn clear_user_cache() -> Option<()> {
    cache::CacheManager::for_dir("riff/net")?
        .clear_cache_pattern(&cached_client::USER_CACHE)
        .await
        .ok()
}

use http::Method;
use librespot::core::spclient::SpClient;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct History {
    #[serde(rename = "playContexts")]
    items: Vec<Item>,
}

#[derive(Deserialize, Debug)]
struct Item {
    uri: String,
    #[serde(rename = "lastPlayedTime")]
    last_played: u64,
    #[serde(rename = "lastPlayedTrackUri")]
    track_uri: String,
}

pub async fn recently_listened_albums(
    sp: &SpClient,
    username: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("/recently-played/v3/user/{username}/recently-played?format=json&offset=0&limit=50&filter=default,collection-new-episodes");

    let res = sp.request_as_json(&Method::GET, &url, None, None).await?;
    let values = serde_json::from_slice::<History>(&*res)?;
    println!("RES: {values:#?}");
    Ok(())
}
