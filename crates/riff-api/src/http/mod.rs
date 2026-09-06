//! HTTP client layer with shared connection pool and retry.

mod pool;
mod retry;

pub use pool::{build_shared_pool, cdn_service};
pub use retry::ServiceClient;
