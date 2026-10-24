//! Dev-tools

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::error::DomainError;

static SIMULATE_OFFLINE: AtomicBool = AtomicBool::new(false);
static INJECTED_ERROR: AtomicU8 = AtomicU8::new(0);

pub fn set_simulate_offline(offline: bool) {
    SIMULATE_OFFLINE.store(offline, Ordering::Relaxed);
}

pub fn is_simulate_offline() -> bool {
    SIMULATE_OFFLINE.load(Ordering::Relaxed)
}

pub fn set_injected_error(code: u8) {
    INJECTED_ERROR.store(code, Ordering::Relaxed);
}

pub fn simulated_failure() -> Option<DomainError> {
    if is_simulate_offline() {
        return Some(DomainError::Network("simulated offline".to_string()));
    }
    match INJECTED_ERROR.load(Ordering::Relaxed) {
        1 => Some(DomainError::RateLimited {
            retry_after_ms: None,
        }),
        2 => Some(DomainError::AuthExpired),
        3 => Some(DomainError::ServerError {
            status: 500,
            message: "injected server error".to_string(),
        }),
        4 => Some(DomainError::ClientError {
            status: 204,
            message: "injected no content".to_string(),
        }),
        _ => None,
    }
}
