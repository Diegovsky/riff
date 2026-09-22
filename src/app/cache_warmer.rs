//! Startup cache warmer.

use std::sync::Arc;

use riff_api::{ApiService, WarmRequest, WarmSource};
use tokio::sync::Mutex;

use crate::app::components::card::IMAGE_SIZE;
use crate::app::components::utils::decode_px;
use crate::app::components::EventListener;
use crate::app::load;
use crate::app::state::{LoginEvent, CARD_BATCH_SIZE};
use crate::app::AppEvent;
use crate::settings::StateTracker;

pub struct CacheWarmer {
    api: Arc<ApiService>,
    /// Serializes warm runs, since overlapping ones would pull the same covers
    /// twice.
    running: Arc<Mutex<()>>,
}

/// What the warmer fetches. `warm_cache` itself has no opinion on this.
fn warm_sources() -> Vec<WarmSource> {
    vec![WarmSource::new(
        "saved albums",
        |api, request| Box::pin(api.get_saved_albums(0, request.list_limit, load::background())),
        |album| Some(&album.art),
    )]
}

impl CacheWarmer {
    pub fn new(api: Arc<ApiService>) -> Self {
        Self {
            api,
            running: Arc::new(Mutex::new(())),
        }
    }

    fn warm(&self) {
        // Must match what the cards will ask for, or the warmed textures land
        // under different cache keys.
        let card_size = StateTracker::new_from_gsettings().load_card_size();
        let request = WarmRequest {
            list_limit: CARD_BATCH_SIZE,
            image_width: IMAGE_SIZE as i32,
            decode_size: decode_px(card_size.pixel_size()),
        };
        let api = Arc::clone(&self.api);
        let running = Arc::clone(&self.running);
        tokio::spawn(async move {
            let _guard = running.lock().await;
            api.warm_cache(request, &warm_sources()).await;
        });
    }
}

impl EventListener for CacheWarmer {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            // Fires before login, so this pass only has disk-cached data. That
            // is the point: the opening screen renders from the last session.
            AppEvent::Started => self.warm(),
            // Once a token exists, retry anything that missed the disk cache.
            AppEvent::LoginEvent(LoginEvent::LoginCompleted) => self.warm(),
            _ => {}
        }
    }
}
