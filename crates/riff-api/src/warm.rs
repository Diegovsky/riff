//! Startup cache warming.
//!
//! Populates cache tiers ahead of first use. What to warm is not decided here:
//! the application passes in [`WarmSource`]s, so this module needs no knowledge
//! of which pages exist.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;

use futures::stream::{self, StreamExt};
use riff_config::api::WARM_IMAGE_BUFFER;

use crate::models::ImageSet;
use crate::scheduler::Load;
use crate::service::ApiService;
use crate::DomainError;

/// What a warm run should fetch. Every field has to match what the UI will later
/// request, since warming only helps if it lands under the same cache keys.
#[derive(Debug, Clone, Copy)]
pub struct WarmRequest {
    pub list_limit: usize,
    /// Logical width used to pick a CDN image URL.
    pub image_width: i32,
    /// Decode size in device pixels.
    pub decode_size: i32,
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Runs the request and reduces the result to image URLs to warm.
type WarmFetch = Box<
    dyn for<'a> Fn(
            &'a ApiService,
            &'a WarmRequest,
        ) -> BoxFuture<'a, Result<HashSet<String>, DomainError>>
        + Send
        + Sync,
>;

/// One page to warm. The fetch is just a call to an existing `ApiService`
/// method, so it decides which cache key the result lands under.
pub struct WarmSource {
    pub name: &'static str,
    fetch: WarmFetch,
}

impl WarmSource {
    /// `fetch` calls a paginated getter (e.g. `get_saved_albums`) with
    /// [`Load::background`], boxed with `Box::pin`. `art` picks the images to
    /// warm out of one fetched item.
    pub fn new<T, F, A>(name: &'static str, fetch: F, art: A) -> Self
    where
        T: 'static,
        F: for<'a> Fn(
                &'a ApiService,
                &'a WarmRequest,
            ) -> BoxFuture<'a, Result<crate::models::Page<T>, DomainError>>
            + Send
            + Sync
            + 'static,
        A: Fn(&T) -> Option<&ImageSet> + Send + Sync + 'static,
    {
        let art = std::sync::Arc::new(art);
        Self {
            name,
            fetch: Box::new(move |api, request| {
                let fut = fetch(api, request);
                let width = request.image_width;
                let art = std::sync::Arc::clone(&art);
                Box::pin(async move {
                    let page = fut.await?;
                    let mut urls = HashSet::new();
                    for item in &page.items {
                        if let Some(set) = art(item) {
                            push_art(&mut urls, set, width);
                        }
                    }
                    Ok(urls)
                })
            }),
        }
    }
}

fn push_art(urls: &mut HashSet<String>, art: &ImageSet, width: i32) {
    if art.is_resource() {
        return;
    }
    if let Some(url) = art.best_for_width(width.max(0) as u32) {
        urls.insert(url.to_string());
    }
}

impl ApiService {
    /// Run every source's fetch, then load every cover it turned up.
    ///
    /// No token required: each fetch checks memory then disk first, so a run
    /// before login still warms from the last session.
    pub async fn warm_cache(&self, request: WarmRequest, sources: &[WarmSource]) {
        debug!("warm: starting cache warm ({} source(s))", sources.len());

        let mut urls: HashSet<String> = HashSet::new();
        for source in sources {
            match (source.fetch)(self, &request).await {
                Ok(found) => urls.extend(found),
                Err(DomainError::NoToken) => {
                    debug!("warm: {} skipped, not cached and no token yet", source.name)
                }
                Err(DomainError::Shed) => debug!("warm: {} shed, queue is busy", source.name),
                Err(e) => warn!("warm: {} failed: {e}", source.name),
            }
        }

        let image_count = urls.len();
        // Still competes for the shared image budget at background priority.
        stream::iter(urls)
            .for_each_concurrent(WARM_IMAGE_BUFFER, |url| async move {
                self.load_image(
                    &url,
                    request.decode_size,
                    request.decode_size,
                    Load::background(),
                )
                .await;
            })
            .await;

        debug!("warm: cache warm complete ({image_count} covers)");
    }
}
