//! Authentication for Riff.
//!
//! Owns everything related to obtaining and persisting credentials:
//! the OAuth2 authorization-code (PKCE) flow ([`oauth2`]) and the secure
//! credential store backed by the Secret Service ([`token_store`]).

#[macro_use]
extern crate log;

mod credentials;
mod oauth2;
mod token_store;

pub use credentials::Credentials;
pub use oauth2::{AuthcodeChallenge, OAuthError, RiffOauthClient, SESSION_CLIENT_ID};
pub use token_store::TokenStore;
