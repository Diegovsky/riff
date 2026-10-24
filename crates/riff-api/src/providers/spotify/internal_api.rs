//! Spotify APIs that are not part of the public Web API.
//!
//! These are the endpoints Spotify's own clients call. They are undocumented,
//! can change without notice, and only accept a token minted for Spotify's own
//! client id (`riff_auth::SESSION_CLIENT_ID`): a Web API token is refused with
//! a 403. Only what the Web API cannot do belongs here.

use std::sync::{Arc, RwLock};

use prost::Message;
use reqwest::header::CONTENT_TYPE;
use reqwest::Method;
use spotify_api::apis;

use crate::error::DomainError;
use crate::token::TokenProvider;

/// Lists the hosts serving the internal API closest to the user.
const RESOLVE_URL: &str = "https://apresolve.spotify.com/?type=spclient";

/// Used when no host could be resolved.
const FALLBACK_HOST: &str = "spclient.wg.spotify.com";

fn is_public(level: proto::PermissionLevel) -> Option<bool> {
    match level {
        proto::PermissionLevel::Blocked => Some(false),
        proto::PermissionLevel::Viewer | proto::PermissionLevel::Contributor => Some(true),
        proto::PermissionLevel::Unknown => None,
    }
}

fn level_for(public: bool) -> proto::PermissionLevel {
    if public {
        proto::PermissionLevel::Viewer
    } else {
        proto::PermissionLevel::Blocked
    }
}

fn public_from(permission: Option<proto::Permission>) -> Result<bool, DomainError> {
    permission
        .and_then(|permission| permission.permission_level)
        .and_then(|level| proto::PermissionLevel::try_from(level).ok())
        .and_then(is_public)
        .ok_or_else(|| {
            error!("spotify: internal API answered without a permission level");
            DomainError::Parse("no permission level".to_string())
        })
}

pub struct SpotifyInternalApi {
    token_provider: Arc<dyn TokenProvider>,
    host: RwLock<Option<String>>,
}

impl SpotifyInternalApi {
    pub fn new(token_provider: Arc<dyn TokenProvider>) -> Self {
        Self {
            token_provider,
            host: RwLock::new(None),
        }
    }

    pub async fn set_playlist_public(&self, id: &str, public: bool) -> Result<bool, DomainError> {
        let request = proto::SetPermissionLevelRequest {
            permission_level: Some(level_for(public).into()),
        };
        let path = format!("{}/level", permission_path(id));
        let body = self
            .request(Method::POST, &path, request.encode_to_vec())
            .await?;
        let response: proto::SetPermissionResponse = decode(&body)?;
        public_from(response.resulting_permission)
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, DomainError> {
        #[cfg(debug_assertions)]
        if let Some(err) = crate::dev::simulated_failure() {
            return Err(err);
        }

        let token = self
            .token_provider
            .access_token()
            .ok_or(DomainError::NoToken)?;
        let url = format!("https://{}{path}", self.host().await);
        let request = crate::http::api_client()
            .request(method, url)
            .bearer_auth(token)
            .header(CONTENT_TYPE, "application/x-protobuf")
            .body(body);
        super::request::send(request).await
    }

    async fn host(&self) -> String {
        if let Some(host) = self.host.read().unwrap().clone() {
            return host;
        }
        match resolve_host().await {
            Some(host) => {
                *self.host.write().unwrap() = Some(host.clone());
                host
            }
            None => FALLBACK_HOST.to_string(),
        }
    }
}

fn permission_path(id: &str) -> String {
    format!(
        "/playlist-permission/v1/playlist/{}/permission/base",
        apis::urlencode(id)
    )
}

async fn resolve_host() -> Option<String> {
    let response = crate::http::api_client()
        .get(RESOLVE_URL)
        .send()
        .await
        .inspect_err(|e| warn!("spotify: could not resolve an internal API host: {e}"))
        .ok()?;
    let listed: serde_json::Value = serde_json::from_slice(&response.bytes().await.ok()?).ok()?;
    let host = listed.get("spclient")?.get(0)?.as_str()?;
    Some(host.strip_suffix(":443").unwrap_or(host).to_string())
}

fn decode<M: Message + Default>(body: &[u8]) -> Result<M, DomainError> {
    M::decode(body).map_err(|e| {
        error!("spotify: internal API response did not match the expected message: {e}");
        DomainError::Parse(e.to_string())
    })
}

mod proto {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
    #[repr(i32)]
    pub enum PermissionLevel {
        Unknown = 0,
        Blocked = 1,
        Viewer = 2,
        Contributor = 3,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Permission {
        #[prost(enumeration = "PermissionLevel", optional, tag = "2")]
        pub permission_level: Option<i32>,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SetPermissionLevelRequest {
        #[prost(enumeration = "PermissionLevel", optional, tag = "1")]
        pub permission_level: Option<i32>,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SetPermissionResponse {
        #[prost(message, optional, tag = "1")]
        pub resulting_permission: Option<Permission>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_for_public() {
        assert_eq!(level_for(false), proto::PermissionLevel::Blocked);
        assert_eq!(level_for(true), proto::PermissionLevel::Viewer);
    }

    fn public_of(permission: &[u8]) -> Result<bool, DomainError> {
        public_from(Some(decode(permission)?))
    }

    #[test]
    fn asks_for_a_level_as_spotify_clients_do() {
        let request = |public| proto::SetPermissionLevelRequest {
            permission_level: Some(level_for(public).into()),
        };
        assert_eq!(request(false).encode_to_vec(), [0x08, 0x01]);
        assert_eq!(request(true).encode_to_vec(), [0x08, 0x02]);
    }

    #[test]
    fn reads_the_level_of_a_permission() {
        let permission = [
            0x0a, 0x08, 0x34, 0xc5, 0x32, 0x50, 0x01, 0x89, 0x38, 0x17, 0x10, 0x02,
        ];
        assert!(public_of(&permission).unwrap());
        assert!(!public_of(&[0x10, 0x01]).unwrap());
        assert!(public_of(&[0x10, 0x03]).unwrap());
    }

    #[test]
    fn rejects_a_permission_without_a_known_level() {
        let unset = [0x0a, 0x07, b'd', b'e', b'f', b'a', b'u', b'l', b't'];
        assert!(public_of(&unset).is_err());
        assert!(public_of(&[0x10, 0x09]).is_err());
        assert!(public_of(&[0x10, 0x00]).is_err());
        assert!(public_of(&[]).is_err());
        assert!(public_of(&[0x0a, 0x08, 0x34]).is_err());
    }

    #[test]
    fn reads_the_permission_a_change_results_in() {
        let response: proto::SetPermissionResponse =
            decode(&[0x0a, 0x04, 0x0a, 0x00, 0x10, 0x01]).unwrap();
        assert!(!public_from(response.resulting_permission).unwrap());
    }

    #[test]
    fn permission_path_escapes_the_id() {
        assert_eq!(
            permission_path("a/b"),
            "/playlist-permission/v1/playlist/a%2Fb/permission/base"
        );
    }
}
