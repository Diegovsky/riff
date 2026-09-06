//! Startup cache warming.
//!
//! Populates cache tiers for the homepage (saved albums + cover art) ahead of
//! first use, so the opening screen renders from cache.

use std::collections::HashSet;

use futures::stream::{self, StreamExt};
use riff_config::{WARM_IMAGE_CONCURRENCY, WARM_IMAGE_EXT, WARM_IMAGE_WIDTH, WARM_LIST_LIMIT};

use crate::models::ImageSet;
use crate::service::ApiService;

fn push_art(urls: &mut HashSet<String>, art: &ImageSet) {
    if art.is_resource() {
        return;
    }
    if let Some(url) = art.best_for_width(WARM_IMAGE_WIDTH) {
        urls.insert(url.to_string());
    }
}

impl ApiService {
    pub async fn warm_cache(&self) {
        if !self.has_token() {
            debug!("warm: no access token, skipping cache warm");
            return;
        }
        debug!("warm: starting homepage cache warm");

        let page = match self.get_saved_albums(0, WARM_LIST_LIMIT).await {
            Ok(page) => page,
            Err(e) => {
                warn!("warm: saved albums failed: {e}");
                return;
            }
        };

        let mut urls: HashSet<String> = HashSet::new();
        for album in &page.items {
            push_art(&mut urls, &album.art);
        }

        let image_count = urls.len();
        stream::iter(urls)
            .for_each_concurrent(WARM_IMAGE_CONCURRENCY, |url| async move {
                self.prefetch_image(&url, WARM_IMAGE_EXT).await;
            })
            .await;

        debug!("warm: homepage cache warm complete ({image_count} covers)");
    }
}
