use anyhow::{Context, Result};
use oo7::Keyring;
use std::sync::{Arc, OnceLock, RwLock, RwLockReadGuard};

use crate::app::credentials::Credentials;

const ATTRS: &[(&'static str, &'static str)] = &[("spot_credentials", "yes")];

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
        let items = keyring.search_items(&ATTRS).await?;
        let item_json = items.first().context("Empty keyring")?.secret().await?;
        let item = serde_json::from_slice(item_json.as_bytes())?;
        Ok(item)
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
