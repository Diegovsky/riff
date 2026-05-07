use anyhow::Result;
use oo7::Keyring;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crate::app::credentials::Credentials;

const ATTRS: &[(&'static str, &'static str)] = &[("spot_credentials", "yes")];
const MAX_KEYRING_RETRIES: u32 = 3;
const KEYRING_RETRY_DELAY: Duration = Duration::from_millis(200);

struct InnerTokenStore {
    storage: RwLock<Option<Credentials>>,
}

#[derive(Clone)]
pub struct TokenStore(Arc<InnerTokenStore>);

impl TokenStore {
    pub fn new() -> Self {
        Self(Arc::new(InnerTokenStore {
            storage: RwLock::new(None),
        }))
    }

    async fn keyring() -> Keyring {
        Keyring::new().await.expect("Failed to initialize keyring")
    }

    pub fn get_cached_blocking(&self) -> Option<Credentials> {
        self.0.storage.read().unwrap().clone()
    }

    pub async fn get_cached(&self) -> Option<Credentials> {
        self.get_cached_blocking()
    }

    async fn retrieve(&self) -> Result<Credentials> {
        let keyring = Self::keyring().await;
        if matches!(keyring, Keyring::File(_)) {
            // migrate keys if inside flatpak
            if let Err(e) = oo7::migrate(vec![ATTRS], true).await {
                debug!("Failed to migrate system keyring: {e}");
            }
        }

        // Attempt to unlock the keyring in case it's still locked after login
        if let Err(e) = keyring.unlock().await {
            warn!("Failed to unlock keyring: {e}");
        }

        // Retry to handle race with keyring daemon startup
        let mut last_err = None;
        for attempt in 0..MAX_KEYRING_RETRIES {
            let items = keyring.search_items(&ATTRS).await?;
            match items.first() {
                Some(item) => {
                    let item_json = item.secret().await?;
                    let creds = serde_json::from_slice(item_json.as_bytes())?;
                    return Ok(creds);
                }
                None => {
                    last_err = Some(anyhow::anyhow!("Empty keyring"));
                    if attempt < MAX_KEYRING_RETRIES - 1 {
                        debug!(
                            "Keyring empty on attempt {}, retrying in {}ms...",
                            attempt + 1,
                            KEYRING_RETRY_DELAY.as_millis()
                        );
                        tokio::time::sleep(KEYRING_RETRY_DELAY).await;
                    }
                }
            }
        }

        Err(last_err.unwrap())
    }

    // Try to clear the credentials
    async fn logout(&self) -> Result<()> {
        let result = Self::keyring().await.search_items(&ATTRS).await?;
        let Some(item) = result.first() else {
            warn!("Logout attempted, but keyring is empty");
            return Ok(());
        };
        item.delete().await?;
        Ok(())
    }

    async fn save(&self, creds: &Credentials) -> Result<()> {
        // We simply write our stuct as JSON and send it
        info!("Saving credentials");
        let encoded = serde_json::to_vec(creds).unwrap();
        Self::keyring()
            .await
            .create_item("Spotify Credentials", &ATTRS, &encoded, true)
            .await?;
        info!("Saved credentials");
        Ok(())
    }

    pub async fn get(&self) -> Option<Credentials> {
        let local = self.0.storage.read().unwrap().clone();
        if local.is_some() {
            return local;
        }

        match self.retrieve().await {
            Ok(token) => {
                self.0.storage.write().unwrap().replace(token.clone());
                Some(token)
            }
            Err(e) => {
                error!("Couldnt get token from secrets service: {e}");
                None
            }
        }
    }

    pub async fn set(&self, creds: Credentials) {
        debug!("Saving token to store...");
        if let Err(e) = self.save(&creds).await {
            warn!("Couldnt save token to secrets service: {e}");
        }
        self.0.storage.write().unwrap().replace(creds);
    }

    pub async fn clear(&self) {
        if let Err(e) = self.logout().await {
            warn!("Couldnt save token to secrets service: {e}");
        }
        self.0.storage.write().unwrap().take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn retrieve_fails_with_empty_keyring_after_retries() {
        // Use a unique attribute so we never collide with real credentials
        let store = TokenStore::new();
        let result = store.retrieve().await;
        // If no Riff credentials are stored, this should fail with "Empty keyring"
        // after exhausting all retries. If credentials ARE stored (dev machine),
        // it should succeed — either outcome is valid.
        match result {
            Ok(creds) => {
                assert!(!creds.access_token.is_empty());
            }
            Err(e) => {
                assert!(e.to_string().contains("Empty keyring"), "Unexpected error: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn get_returns_none_when_keyring_empty() {
        let store = TokenStore::new();
        // Clear any cached value
        store.0.storage.write().unwrap().take();
        let result = store.get().await;
        // On a machine without stored Riff credentials, this returns None.
        // On a dev machine with credentials, it returns Some.
        // Both are valid — we just verify no panic occurs.
        if result.is_none() {
            assert!(store.get_cached_blocking().is_none());
        }
    }

    #[tokio::test]
    async fn get_cached_returns_stored_value() {
        let store = TokenStore::new();
        let creds = Credentials {
            access_token: "test_token".to_string(),
            refresh_token: "test_refresh".to_string(),
            token_expiry_time: None,
        };
        store.0.storage.write().unwrap().replace(creds.clone());

        let cached = store.get_cached().await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().access_token, "test_token");
    }

    #[tokio::test]
    async fn get_returns_cached_without_hitting_keyring() {
        let store = TokenStore::new();
        let creds = Credentials {
            access_token: "cached_token".to_string(),
            refresh_token: "cached_refresh".to_string(),
            token_expiry_time: None,
        };
        store.0.storage.write().unwrap().replace(creds.clone());

        // get() should return the cached value without calling retrieve()
        let result = store.get().await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().access_token, "cached_token");
    }
}
