#[macro_use]
extern crate log;

pub mod models;

mod defaults;
mod error;
mod token;

pub(crate) mod cache;
pub(crate) mod http;
pub(crate) mod providers;
pub(crate) mod service;
mod warm;

#[cfg(debug_assertions)]
pub(crate) mod dev;

pub use error::DomainError;
pub use providers::spotify::domain::{check_user_profile, UserProfileCheck};
pub use providers::spotify::spotify_service;
pub use service::ApiService;

#[cfg(debug_assertions)]
pub use dev::{is_simulate_offline, set_injected_error, set_simulate_offline};

pub use models::{Device, DeviceKind, PlayerState, Queue, RepeatMode};
pub use token::TokenProvider;
