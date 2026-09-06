//! Startup cache warmer.

use std::sync::Arc;

use riff_api::ApiService;

use crate::app::components::EventListener;
use crate::app::state::LoginEvent;
use crate::app::AppEvent;

pub struct CacheWarmer {
    api: Arc<ApiService>,
}

impl CacheWarmer {
    pub fn new(api: Arc<ApiService>) -> Self {
        Self { api }
    }

    fn warm(&self) {
        let api = Arc::clone(&self.api);
        tokio::spawn(async move {
            api.warm_cache().await;
        });
    }
}

impl EventListener for CacheWarmer {
    fn on_event(&mut self, event: &AppEvent) {
        if let AppEvent::LoginEvent(LoginEvent::LoginCompleted) = event {
            self.warm();
        }
    }
}
