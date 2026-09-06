//! Provider-neutral error type for the data layer.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("No token available")]
    NoToken,

    #[error("Token expired or invalid (401)")]
    AuthExpired,

    #[error("Rate limited (429), retry after {retry_after_ms:?}ms")]
    RateLimited { retry_after_ms: Option<u64> },

    #[error("Not found (404): {resource}")]
    NotFound { resource: String },

    #[error("Client error ({status}): {message}")]
    ClientError { status: u16, message: String },

    #[error("Server error ({status}): {message}")]
    ServerError { status: u16, message: String },

    #[error("Network error: {0}")]
    Network(String),

    #[error("Parse error: {0}")]
    Parse(String),
}

impl DomainError {
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            DomainError::Network(_)
                | DomainError::RateLimited { .. }
                | DomainError::ServerError { .. }
        )
    }
}
