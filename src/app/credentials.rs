use serde::{Deserialize, Serialize};
use std::time::SystemTime;

// A (statically accessed) wrapper around the DBUS Secret Service
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: String,
    pub token_expiry_time: Option<SystemTime>,
}

impl Credentials {
    pub fn token_expired(&self) -> bool {
        match self.token_expiry_time {
            Some(v) => SystemTime::now() > v,
            None => true,
        }
    }
}
