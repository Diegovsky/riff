//! Authentication subsystem.
//!
//! Owns everything related to obtaining and persisting Spotify credentials:
//! the OAuth2 authorization-code (PKCE) flow ([`oauth2`]) and the secure
//! credential store backed by the Secret Service ([`token_store`]).
//!
//! This module is a peer to [`crate::player`] and [`crate::api`]; both depend
//! on [`TokenStore`] for authenticated requests, while the player also drives
//! the interactive login flow via [`RiffOauthClient`].

mod oauth2;
mod token_store;

pub use oauth2::{AuthcodeChallenge, OAuthError, RiffOauthClient, SESSION_CLIENT_ID};
pub use token_store::TokenStore;
