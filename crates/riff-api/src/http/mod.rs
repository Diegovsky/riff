//! HTTP client layer with shared connection pool and retry.

mod api_client;
mod pool;
mod retry;

pub use api_client::api_client;
pub use pool::{build_shared_pool, cdn_service};
pub use retry::ServiceClient;
