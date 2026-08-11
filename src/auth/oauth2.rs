//! Provides a Spotify access token using the OAuth authorization code flow
//! with PKCE.
//!
//! Assuming sufficient scopes, the returned access token may be used with Spotify's
//! Web API, and/or to establish a new Session with [`librespot_core`].
//!
//! The authorization code flow is an interactive process which requires a web browser
//! to complete. The resulting code must then be provided back from the browser to this
//! library for exchange into an access token. Providing the code can be automatic via
//! a spawned http server (mimicking Spotify's client), or manually via stdin. The latter
//! is appropriate for headless systems.

use crate::app::credentials::Credentials;

use log::{error, info, trace, warn};
use oauth2::reqwest::async_http_client;
use oauth2::{
    basic::BasicClient, basic::BasicErrorResponseType, AuthUrl, AuthorizationCode, ClientId,
    CsrfToken, PkceCodeChallenge, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use oauth2::{PkceCodeVerifier, RefreshToken, RequestTokenError};
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::task::JoinHandle;
use url::Url;

use super::TokenStore;

pub const CLIENT_ID: &str = "782ae96ea60f4cdf986a766049607005";
pub const REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
pub const SCOPES: &str = "user-read-private,\
playlist-read-private,\
playlist-read-collaborative,\
user-library-read,\
user-library-modify,\
user-follow-read,\
user-follow-modify,\
user-top-read,\
user-read-recently-played,\
user-read-playback-state,\
playlist-modify-public,\
playlist-modify-private,\
user-modify-playback-state,\
streaming,\
playlist-modify-public";

pub const SESSION_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
pub const SESSION_SCOPES: &str = "streaming";

// Interval between background refresh retries. The refresh token stays valid
// far longer than the access token, so on a transient failure we keep polling
// rather than giving up. A 10s cadence is frequent enough to recover quickly
// yet light enough to poll indefinitely through a longer outage.
const REFRESH_POLL_INTERVAL: Duration = Duration::from_secs(10);

// How long before expiry we start trying to refresh. This must be larger than
// REFRESH_POLL_INTERVAL so that transient failures get retried while the
// current token is still valid, leaving no window where playback runs on an
// expired token. With a 60s lead and a 10s interval we get roughly six
// attempts before the token actually expires.
const REFRESH_LEAD_TIME: Duration = Duration::from_secs(60);

pub struct RiffOauthClient {
    client: BasicClient,
    session_client: BasicClient,
    token_store: TokenStore,
}

pub struct AuthcodeChallenge {
    pkce_verifier: PkceCodeVerifier,
    pub auth_url: Url,
    listener: JoinHandle<Result<AuthorizationCode, OAuthError>>,
}

async fn build_authcode_challenge(
    client: &BasicClient,
    scopes: &str,
    notify_complete: impl FnOnce() + Send + 'static,
) -> Result<AuthcodeChallenge, OAuthError> {
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    // Generate the full authorization URL.
    // Some of these scopes are unavailable for custom client IDs. Which?
    let request_scopes: Vec<oauth2::Scope> =
        scopes.split(",").map(|s| Scope::new(s.into())).collect();

    let (auth_url, csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scopes(request_scopes)
        .set_pkce_challenge(pkce_challenge)
        .url();

    Ok(AuthcodeChallenge {
        pkce_verifier,
        auth_url,
        listener: tokio::task::spawn(async move {
            let result = wait_for_authcode(csrf_token).await;
            notify_complete();
            result
        }),
    })
}

impl RiffOauthClient {
    pub fn new(token_store: TokenStore) -> Self {
        Self {
            client: Self::build_client(CLIENT_ID),
            session_client: Self::build_client(SESSION_CLIENT_ID),
            token_store,
        }
    }

    fn build_client(client_id: &str) -> BasicClient {
        let auth_url = AuthUrl::new("https://accounts.spotify.com/authorize".to_string())
            .expect("Malformed URL");
        let token_url = TokenUrl::new("https://accounts.spotify.com/api/token".to_string())
            .expect("Malformed URL");
        let redirect_url = RedirectUrl::new(REDIRECT_URI.to_string()).expect("Malformed URL");
        BasicClient::new(
            ClientId::new(client_id.to_string()),
            None,
            auth_url,
            Some(token_url),
        )
        .set_redirect_uri(redirect_url)
    }

    pub async fn spawn_authcode_listener(
        &self,
        notify_complete: impl FnOnce() + Send + 'static,
    ) -> Result<AuthcodeChallenge, OAuthError> {
        build_authcode_challenge(&self.client, SCOPES, notify_complete).await
    }

    pub async fn spawn_session_authcode_listener(
        &self,
        notify_complete: impl FnOnce() + Send + 'static,
    ) -> Result<AuthcodeChallenge, OAuthError> {
        build_authcode_challenge(&self.session_client, SESSION_SCOPES, notify_complete).await
    }

    /// Obtain a Spotify access token using the authorization code with PKCE OAuth flow.
    /// The redirect_uri must match what is registered to the client ID.
    pub async fn exchange_authcode(
        &self,
        challenge: AuthcodeChallenge,
    ) -> Result<Credentials, OAuthError> {
        let code = challenge
            .listener
            .await
            .map_err(|_| OAuthError::AuthCodeListenerTerminated)??;

        let token = self
            .client
            .exchange_code(code)
            .set_pkce_verifier(challenge.pkce_verifier)
            .request_async(async_http_client)
            .await
            .map_err(|e| match e {
                RequestTokenError::ServerResponse(res) => {
                    error!(
                        "An error occured while exchange a code: {}",
                        res.to_string()
                    );
                    OAuthError::ExchangeCode { e: res.to_string() }
                }
                e => OAuthError::ExchangeCode { e: e.to_string() },
            })?;

        trace!("Obtained new access token: {token:?}");

        let refresh_token = token
            .refresh_token()
            .ok_or(OAuthError::NoRefreshToken)?
            .secret()
            .to_string();

        let token = Credentials {
            access_token: token.access_token().secret().to_string(),
            refresh_token,
            token_expiry_time: Some(
                SystemTime::now()
                    + token
                        .expires_in()
                        .unwrap_or_else(|| Duration::from_secs(3600)),
            ),
        };

        Ok(token)
    }

    pub async fn exchange_session_authcode(
        &self,
        challenge: AuthcodeChallenge,
    ) -> Result<String, OAuthError> {
        let code = challenge
            .listener
            .await
            .map_err(|_| OAuthError::AuthCodeListenerTerminated)??;

        let token = self
            .session_client
            .exchange_code(code)
            .set_pkce_verifier(challenge.pkce_verifier)
            .request_async(async_http_client)
            .await
            .map_err(|e| match e {
                RequestTokenError::ServerResponse(res) => {
                    error!(
                        "An error occured while exchanging a session code: {}",
                        res.to_string()
                    );
                    OAuthError::ExchangeCode { e: res.to_string() }
                }
                e => OAuthError::ExchangeCode { e: e.to_string() },
            })?;

        Ok(token.access_token().secret().to_string())
    }

    pub async fn clear_credentials(&self) {
        self.token_store.clear().await;
    }

    pub async fn save_credentials(&self, creds: &Credentials) {
        self.token_store.set(creds.clone()).await;
    }

    pub async fn get_valid_token(&self) -> Result<Credentials, OAuthError> {
        let token = self.token_store.get().await.ok_or(OAuthError::LoggedOut)?;
        if token.token_expired() {
            self.refresh_token(token).await
        } else {
            Ok(token)
        }
    }

    /// Dev tools only: backdate the cached token so it looks expired, then run
    /// the normal valid-token path, which will refresh it against Spotify.
    /// Exercises the full token-refresh flow on demand.
    #[cfg(debug_assertions)]
    pub async fn dev_expire_and_refresh(&self) -> Result<Credentials, OAuthError> {
        self.token_store.dev_expire_cached_token();
        self.get_valid_token().await
    }

    /// Perform a single refresh-token exchange.
    ///
    /// This does not retry. A rejected refresh token (`invalid_grant`) is
    /// fatal: the stored credentials are cleared and [`OAuthError::RefreshFailed`]
    /// is returned. Any other failure is treated as transient
    /// ([`OAuthError::RefreshTransient`]) and the credentials are left intact so
    /// a caller can poll and try again later.
    pub async fn refresh_token(&self, old_token: Credentials) -> Result<Credentials, OAuthError> {
        let refresh_token = old_token.refresh_token.clone();

        match self
            .client
            .exchange_refresh_token(&RefreshToken::new(refresh_token.clone()))
            .request_async(async_http_client)
            .await
        {
            Ok(token) => {
                // Spotify does not always return a new refresh token. When it
                // is omitted, reuse the old one so we stay logged in.
                let new_refresh_token = token
                    .refresh_token()
                    .map(|t| t.secret().to_string())
                    .unwrap_or_else(|| refresh_token.clone());

                let new_token = Credentials {
                    access_token: token.access_token().secret().to_string(),
                    refresh_token: new_refresh_token,
                    token_expiry_time: Some(
                        SystemTime::now()
                            + token
                                .expires_in()
                                .unwrap_or_else(|| Duration::from_secs(3600)),
                    ),
                };

                self.token_store.set(new_token.clone()).await;
                Ok(new_token)
            }
            Err(e) => {
                // A rejected refresh token will never succeed on retry, so
                // treat it as fatal and clear the stored credentials. Anything
                // else (network errors, 5xx, parse failures) is transient and
                // must not discard the still-valid refresh token.
                let fatal = matches!(
                    &e,
                    RequestTokenError::ServerResponse(res)
                        if matches!(res.error(), BasicErrorResponseType::InvalidGrant)
                );

                if fatal {
                    error!("Refresh token rejected by Spotify: {e}");
                    self.token_store.clear().await;
                    Err(OAuthError::RefreshFailed { e: e.to_string() })
                } else {
                    warn!("Transient error while refreshing token: {e}");
                    Err(OAuthError::RefreshTransient { e: e.to_string() })
                }
            }
        }
    }

    pub async fn refresh_token_at_expiry(&self) -> Result<Credentials, OAuthError> {
        let Some(old_token) = self.token_store.get_cached().await.take() else {
            return Err(OAuthError::NoRefreshToken);
        };

        let duration = old_token
            .token_expiry_time
            .and_then(|d| d.duration_since(SystemTime::now()).ok())
            .unwrap_or(Duration::from_secs(120));

        info!(
            "Refreshing token in approx {}min",
            duration.as_secs().div_euclid(60)
        );
        tokio::time::sleep(duration.saturating_sub(REFRESH_LEAD_TIME)).await;

        // Long-poll the refresh: retry transient failures at a generous
        // interval until we succeed. Only a fatal error (rejected refresh
        // token) stops the poll and propagates to the caller.
        loop {
            info!("Refreshing token...");
            match self.refresh_token(old_token.clone()).await {
                Ok(new_token) => return Ok(new_token),
                Err(OAuthError::RefreshTransient { e }) => {
                    warn!(
                        "Token refresh failed transiently ({e}); retrying in {}s",
                        REFRESH_POLL_INTERVAL.as_secs()
                    );
                    tokio::time::sleep(REFRESH_POLL_INTERVAL).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("Auth code param not found in URI")]
    AuthCodeNotFound,

    #[error("CSRF token param not found in URI")]
    CsrfTokenNotFound,

    #[error("Failed to bind server to {addr} ({e})")]
    AuthCodeListenerBind { addr: SocketAddr, e: io::Error },

    #[error("Listener terminated without accepting a connection")]
    AuthCodeListenerTerminated,

    #[error("Failed to parse redirect URI from HTTP request")]
    AuthCodeListenerParse,

    #[error("Failed to write HTTP response")]
    AuthCodeListenerWrite,

    #[error("Failed to exchange code for access token ({e})")]
    ExchangeCode { e: String },

    #[error("Spotify did not provide a refresh token")]
    NoRefreshToken,

    #[error("Failed to refresh access token ({e})")]
    RefreshFailed { e: String },

    #[error("Transient error while refreshing access token ({e})")]
    RefreshTransient { e: String },

    #[error("No saved token")]
    LoggedOut,

    #[error("Mismatched state during auth code exchange")]
    InvalidState,
}

/// Spawn HTTP server at provided socket address to accept OAuth callback and return auth code.
async fn wait_for_authcode(expected_state: CsrfToken) -> Result<AuthorizationCode, OAuthError> {
    let addr = get_socket_address(REDIRECT_URI).expect("Invalid redirect uri");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| OAuthError::AuthCodeListenerBind { addr, e })?;

    let (mut stream, _) = listener
        .accept()
        .await
        .map_err(|_| OAuthError::AuthCodeListenerTerminated)?;

    let mut request_line = String::new();
    let mut reader = BufReader::new(&mut stream);
    reader
        .read_line(&mut request_line)
        .await
        .map_err(|_| OAuthError::AuthCodeListenerParse)?;

    let (state, code) = parse_query(&request_line)?;
    if *expected_state.secret() != *state.secret() {
        return Err(OAuthError::InvalidState);
    }

    let message = include_str!("./login.html");
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{}",
        message.len(),
        message
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|_| OAuthError::AuthCodeListenerWrite)?;

    Ok(code)
}

fn parse_query(request_line: &str) -> Result<(CsrfToken, AuthorizationCode), OAuthError> {
    let query = request_line
        .split_whitespace()
        .nth(1)
        .ok_or(OAuthError::AuthCodeListenerParse)?
        .split("?")
        .nth(1)
        .ok_or(OAuthError::AuthCodeListenerParse)?;

    let mut query_params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();

    let csrf_token = query_params
        .remove("state")
        .map(CsrfToken::new)
        .ok_or(OAuthError::CsrfTokenNotFound)?;
    let code = query_params
        .remove("code")
        .map(AuthorizationCode::new)
        .ok_or(OAuthError::AuthCodeNotFound)?;

    Ok((csrf_token, code))
}

// If the specified `redirect_uri` is HTTP, loopback, and contains a port,
// then the corresponding socket address is returned.
fn get_socket_address(redirect_uri: &str) -> Option<SocketAddr> {
    let url = match Url::parse(redirect_uri) {
        Ok(u) if u.scheme() == "http" && u.port().is_some() => u,
        _ => return None,
    };
    let socket_addr = match url.socket_addrs(|| None) {
        Ok(mut addrs) => addrs.pop(),
        _ => None,
    };
    if let Some(s) = socket_addr {
        if s.ip().is_loopback() {
            return socket_addr;
        }
    }
    None
}

#[cfg(test)]
mod test {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    use super::*;

    #[test]
    fn get_socket_address_none() {
        // No port
        assert_eq!(get_socket_address("http://127.0.0.1/foo"), None);
        assert_eq!(get_socket_address("http://127.0.0.1:/foo"), None);
        assert_eq!(get_socket_address("http://[::1]/foo"), None);
        // Not localhost
        assert_eq!(get_socket_address("http://56.0.0.1:1234/foo"), None);
        assert_eq!(
            get_socket_address("http://[3ffe:2a00:100:7031::1]:1234/foo"),
            None
        );
        // Not http
        assert_eq!(get_socket_address("https://127.0.0.1/foo"), None);
    }

    #[test]
    fn get_socket_address_localhost() {
        let localhost_v4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1234);
        let localhost_v6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)), 8888);

        assert_eq!(
            get_socket_address("http://127.0.0.1:1234/foo"),
            Some(localhost_v4)
        );
        assert_eq!(
            get_socket_address("http://[0:0:0:0:0:0:0:1]:8888/foo"),
            Some(localhost_v6)
        );
        assert_eq!(
            get_socket_address("http://[::1]:8888/foo"),
            Some(localhost_v6)
        );
    }
}
