use riff_config::api::{
    CDN_MAX_CONNECTIONS_PER_HOST, CDN_REQUEST_TIMEOUT, CDN_RETRY_BACKOFF_MULTIPLIER,
    CDN_RETRY_INITIAL_BACKOFF, CDN_RETRY_MAX_ATTEMPTS, POOL_CONNECT_TIMEOUT, POOL_TCP_KEEPALIVE,
};

use super::retry::{RetryPolicy, ServiceClient};

/// Build a shared connection pool used by all service clients.
pub fn build_shared_pool() -> reqwest::Client {
    #[allow(unused_mut)]
    let mut builder = reqwest::Client::builder()
        .pool_max_idle_per_host(CDN_MAX_CONNECTIONS_PER_HOST)
        .tcp_keepalive(POOL_TCP_KEEPALIVE)
        .connect_timeout(POOL_CONNECT_TIMEOUT);

    // Needs `--features riff-api/insecure-tls`, so release builds cannot reach
    // it. Revoked certs are still rejected; this just tolerates a local
    // self-signed proxy.
    #[cfg(all(debug_assertions, feature = "insecure-tls"))]
    if std::env::var_os("RIFF_ACCEPT_INVALID_CERTS").is_some() {
        warn!(
            "RIFF_ACCEPT_INVALID_CERTS is set: TLS certificate validation is \
             DISABLED. Never use this outside local debugging."
        );
        builder = builder.danger_accept_invalid_certs(true);
    }

    builder.build().expect("failed to build shared HTTP pool")
}

/// Image CDN service: no auth, simple retry.
pub fn cdn_service(pool: reqwest::Client) -> ServiceClient {
    ServiceClient::new(
        pool,
        RetryPolicy {
            max_attempts: CDN_RETRY_MAX_ATTEMPTS,
            initial_backoff: CDN_RETRY_INITIAL_BACKOFF,
            backoff_multiplier: CDN_RETRY_BACKOFF_MULTIPLIER,
            respect_retry_after: false,
        },
        CDN_REQUEST_TIMEOUT,
    )
}
