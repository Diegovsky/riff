//! Shared HTTP client for generated OpenAPI providers, with request logging.

use std::sync::OnceLock;
use std::time::Instant;

use http::Extensions;
use reqwest::{Request, Response, StatusCode, Url};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, Middleware, Next};
use riff_config::api::{
    API_MAX_IDLE_CONNECTIONS_PER_HOST, API_REQUEST_TIMEOUT, POOL_CONNECT_TIMEOUT,
    POOL_TCP_KEEPALIVE,
};

/// Per-provider trace-id header, keyed by request host.
const PROVIDER_TRACE_ID_HEADERS: &[(&str, &str)] = &[("api.spotify.com", "spotify-request-id")];

/// Generic trace-id header, checked if no provider-specific one matches.
const FALLBACK_TRACE_ID_HEADER: &str = "x-request-id";

/// Query params whose values may be logged. An allowlist, so a new endpoint
/// cannot quietly start logging user content.
const LOGGED_PARAMS: &[&str] = &["limit", "offset", "type", "market", "locale", "after"];

/// Process-wide provider API client; cloning shares one connection pool.
pub fn api_client() -> ClientWithMiddleware {
    static CLIENT: OnceLock<ClientWithMiddleware> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            let inner = reqwest::Client::builder()
                .timeout(API_REQUEST_TIMEOUT)
                .connect_timeout(POOL_CONNECT_TIMEOUT)
                .tcp_keepalive(POOL_TCP_KEEPALIVE)
                .pool_max_idle_per_host(API_MAX_IDLE_CONNECTIONS_PER_HOST)
                .build()
                .expect("failed to build the provider API client");
            ClientBuilder::new(inner).with(LoggingMiddleware).build()
        })
        .clone()
}

struct LoggingMiddleware;

#[async_trait::async_trait]
impl Middleware for LoggingMiddleware {
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<Response> {
        let method = req.method().clone();
        let host = req.url().host_str().unwrap_or("unknown-host").to_string();
        let endpoint = endpoint_for_log(req.url());
        trace!("{host}: {method} {endpoint} sending");

        let started = Instant::now();
        let result = next.run(req, extensions).await;
        let ms = started.elapsed().as_millis();

        match &result {
            Ok(response) => {
                let status = response.status();
                let line = response_log_line(&host, method.as_str(), &endpoint, ms, response);
                log::log!(level_for(status), "{line}");
            }
            Err(err) => {
                warn!("{host}: {method} {endpoint} failed after {ms}ms: {err}");
            }
        }

        result
    }
}

fn response_log_line(
    host: &str,
    method: &str,
    endpoint: &str,
    ms: u128,
    response: &Response,
) -> String {
    let status = response.status();
    let trace_id = trace_id(host, response).unwrap_or("none");
    let retry_after = header(response, "retry-after")
        .map(|v| format!(", retry-after {v}"))
        .unwrap_or_default();
    format!(
        "{host}: {method} {endpoint} -> {status} in {ms}ms \
         (request-id {trace_id}{retry_after})"
    )
}

fn level_for(status: StatusCode) -> log::Level {
    if status.is_server_error() {
        log::Level::Error
    } else if status == StatusCode::TOO_MANY_REQUESTS {
        // Already shown to the user as a toast.
        log::Level::Warn
    } else if status == StatusCode::UNAUTHORIZED {
        // Routine token expiry; caller refreshes and retries.
        log::Level::Debug
    } else if status.is_client_error() {
        log::Level::Warn
    } else {
        log::Level::Debug
    }
}

/// Path and query with user content redacted, e.g. `/v1/search?q=<redacted>&type=track`.
fn endpoint_for_log(url: &Url) -> String {
    let mut out = url.path().to_string();
    let mut pairs = url.query_pairs().peekable();
    if pairs.peek().is_none() {
        return out;
    }
    out.push('?');
    for (i, (key, value)) in pairs.enumerate() {
        if i > 0 {
            out.push('&');
        }
        out.push_str(&key);
        out.push('=');
        if LOGGED_PARAMS.contains(&key.as_ref()) {
            out.push_str(&value);
        } else {
            out.push_str("<redacted>");
        }
    }
    out
}

fn header<'a>(response: &'a Response, name: &str) -> Option<&'a str> {
    response.headers().get(name).and_then(|v| v.to_str().ok())
}

fn trace_id<'a>(host: &str, response: &'a Response) -> Option<&'a str> {
    let provider_header = PROVIDER_TRACE_ID_HEADERS
        .iter()
        .find(|(h, _)| *h == host)
        .map(|(_, header_name)| *header_name);

    provider_header
        .and_then(|name| header(response, name))
        .or_else(|| header(response, FALLBACK_TRACE_ID_HEADER))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(raw: &str) -> String {
        endpoint_for_log(&Url::parse(raw).unwrap())
    }

    fn response(status: u16, headers: &[(&str, &str)]) -> Response {
        let mut builder = http::Response::builder().status(status);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        Response::from(builder.body(Vec::<u8>::new()).unwrap())
    }

    #[test]
    fn path_only_request_logs_path() {
        assert_eq!(
            target("https://api.spotify.com/v1/albums/4aawyAB9vmqN3uQ7FjRGTy"),
            "/v1/albums/4aawyAB9vmqN3uQ7FjRGTy"
        );
    }

    #[test]
    fn pagination_params_are_kept_for_context() {
        assert_eq!(
            target("https://api.spotify.com/v1/me/albums?limit=20&offset=40"),
            "/v1/me/albums?limit=20&offset=40"
        );
    }

    #[test]
    fn search_text_is_redacted() {
        assert_eq!(
            target("https://api.spotify.com/v1/search?q=nick%20cave&type=artist"),
            "/v1/search?q=<redacted>&type=artist"
        );
    }

    #[test]
    fn unrecognised_params_are_redacted() {
        assert_eq!(
            target("https://api.spotify.com/v1/tracks?ids=abc,def&limit=2"),
            "/v1/tracks?ids=<redacted>&limit=2"
        );
    }

    #[test]
    fn log_line_reports_endpoint_status_and_request_id() {
        let response = response(502, &[("x-request-id", "test-req-id")]);
        let endpoint = target("https://music.example.com/v1/search?q=secret&type=track");
        let line = response_log_line("music.example.com", "GET", &endpoint, 12, &response);

        assert!(
            line.contains("/v1/search?q=<redacted>&type=track"),
            "{line}"
        );
        assert!(line.contains("502 Bad Gateway"), "{line}");
        assert!(line.contains("request-id test-req-id"), "{line}");
        assert!(line.contains("in 12ms"), "{line}");
        assert!(!line.contains("secret"), "{line}");
    }

    #[test]
    fn log_line_reports_retry_after_when_present() {
        let response = response(429, &[("retry-after", "30")]);
        let line = response_log_line("api.spotify.com", "GET", "/v1/me/albums", 5, &response);

        assert!(line.contains("retry-after 30"), "{line}");
        assert!(line.contains("request-id none"), "{line}");
    }

    #[test]
    fn status_log_levels() {
        assert_eq!(level_for(StatusCode::OK), log::Level::Debug);
        assert_eq!(level_for(StatusCode::UNAUTHORIZED), log::Level::Debug);
        assert_eq!(level_for(StatusCode::TOO_MANY_REQUESTS), log::Level::Warn);
        assert_eq!(level_for(StatusCode::NOT_FOUND), log::Level::Warn);
        assert_eq!(level_for(StatusCode::BAD_GATEWAY), log::Level::Error);
    }

    #[test]
    fn provider_header_does_not_leak_to_other_hosts() {
        let response = response(
            200,
            &[
                ("spotify-request-id", "should-not-be-used"),
                ("x-request-id", "generic-id"),
            ],
        );

        assert_eq!(trace_id("music.example.com", &response), Some("generic-id"));
    }

    #[test]
    fn provider_header_wins_on_its_own_host() {
        let response = response(
            200,
            &[
                ("spotify-request-id", "spotify-id"),
                ("x-request-id", "generic-id"),
            ],
        );

        assert_eq!(trace_id("api.spotify.com", &response), Some("spotify-id"));
    }
}
