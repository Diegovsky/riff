use std::time::Duration;

use isahc::config::Configurable;
use isahc::HttpClient;

use super::retry::{RetryPolicy, ServiceClient};

/// Build a shared connection pool used by all service clients.
pub fn build_shared_pool() -> HttpClient {
    let mut builder = HttpClient::builder()
        .max_connections_per_host(16)
        .tcp_keepalive(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10));

    // Allow skipping cert validation only with explicit env var opt-in.
    if std::env::var_os("RIFF_ACCEPT_INVALID_CERTS").is_some() {
        warn!(
            "RIFF_ACCEPT_INVALID_CERTS is set: TLS certificate validation is \
             DISABLED. Never use this outside local debugging."
        );
        builder = builder.ssl_options(isahc::config::SslOption::DANGER_ACCEPT_INVALID_CERTS);
    }

    builder.build().expect("failed to build shared HTTP pool")
}

/// Image CDN service: no auth, simple retry.
pub fn cdn_service(pool: HttpClient) -> ServiceClient {
    ServiceClient::new(
        pool,
        RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(200),
            backoff_multiplier: 2,
            respect_retry_after: false,
        },
        Duration::from_secs(30),
    )
}
