use std::sync::{Arc, Mutex};

use tokio::task::JoinHandle;

use crate::credentials::Credentials;
use crate::oauth2::{OauthClient, RiffOauthClient, SESSION_CLIENT_ID};
use crate::token_store::TokenStore;

struct InnerSessionTokens {
    client: OauthClient,
    store: TokenStore,
    refresh_task: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Clone)]
pub struct SessionTokens(Arc<InnerSessionTokens>);

impl Default for SessionTokens {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionTokens {
    pub fn new() -> Self {
        Self(Arc::new(InnerSessionTokens {
            client: RiffOauthClient::build_client(SESSION_CLIENT_ID),
            store: TokenStore::for_session(),
            refresh_task: Mutex::new(None),
        }))
    }

    pub(crate) fn client(&self) -> &OauthClient {
        &self.0.client
    }

    pub fn access_token(&self) -> Option<String> {
        self.0
            .store
            .get_cached_blocking()
            .filter(|creds| !creds.token_expired())
            .map(|creds| creds.access_token)
    }

    pub(crate) async fn save(&self, creds: &Credentials) {
        self.0.store.set(creds.clone()).await;
        self.keep_valid();
    }

    pub(crate) async fn restore(&self) -> bool {
        let stored = self.0.store.get().await.is_some();
        if stored {
            self.keep_valid();
        }
        stored
    }

    pub(crate) async fn clear(&self) {
        if let Some(task) = self.0.refresh_task.lock().unwrap().take() {
            task.abort();
        }
        self.0.store.clear().await;
    }

    fn keep_valid(&self) {
        let client = self.0.client.clone();
        let store = self.0.store.clone();
        let task = tokio::task::spawn(async move {
            if let Err(e) = RiffOauthClient::valid_token(&client, &store).await {
                warn!("Session token is not valid yet: {e}");
            }
            loop {
                if let Err(e) = RiffOauthClient::refresh_at_expiry(&client, &store).await {
                    warn!("Session token refresh loop stopping: {e}");
                    break;
                }
            }
        });
        if let Some(previous) = self.0.refresh_task.lock().unwrap().replace(task) {
            previous.abort();
        }
    }
}
