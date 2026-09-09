//! Byte-budgeted LRU cache for decoded textures.

use std::sync::Mutex;

use super::lru::LruCache;

pub struct TextureCache {
    inner: Mutex<LruCache<String, gdk::Texture>>,
}

impl TextureCache {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            inner: Mutex::new(LruCache::new(max_bytes)),
        }
    }

    pub fn get(&self, key: &str) -> Option<gdk::Texture> {
        self.inner.lock().unwrap().get(key).cloned()
    }

    pub fn insert(&self, key: String, texture: gdk::Texture, byte_size: usize) {
        self.inner.lock().unwrap().insert(key, texture, byte_size);
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a throwaway 1x1 texture. Uses an in-memory pixbuf, which needs no
    /// display connection, so it is safe in a headless test environment.
    fn dummy_texture() -> gdk::Texture {
        let pixbuf = gdk_pixbuf::Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, true, 8, 1, 1)
            .expect("1x1 pixbuf should allocate");
        gdk::Texture::for_pixbuf(&pixbuf)
    }

    #[test]
    fn empty_cache_reports_zero_and_misses() {
        let cache = TextureCache::new(1024);
        assert_eq!(cache.inner.lock().unwrap().current_bytes(), 0);
        assert!(cache.get("absent").is_none());
    }

    #[test]
    fn insert_then_get_roundtrip() {
        let cache = TextureCache::new(1024);
        cache.insert("a".into(), dummy_texture(), 100);
        assert!(cache.get("a").is_some());
        assert_eq!(cache.inner.lock().unwrap().current_bytes(), 100);
    }

    #[test]
    fn reinsert_same_key_does_not_double_count() {
        let cache = TextureCache::new(1024);
        cache.insert("a".into(), dummy_texture(), 100);
        cache.insert("a".into(), dummy_texture(), 150);
        assert_eq!(cache.inner.lock().unwrap().current_bytes(), 150);
        assert!(cache.get("a").is_some());
    }

    #[test]
    fn item_larger_than_budget_is_rejected() {
        let cache = TextureCache::new(100);
        cache.insert("big".into(), dummy_texture(), 200);
        assert!(cache.get("big").is_none());
        assert_eq!(cache.inner.lock().unwrap().current_bytes(), 0);
    }

    #[test]
    fn eviction_keeps_within_budget() {
        const MAX_BYTES: usize = 250;
        let cache = TextureCache::new(MAX_BYTES);
        for i in 0..10 {
            cache.insert(format!("k{i}"), dummy_texture(), 100);
            let used = cache.inner.lock().unwrap().current_bytes();
            assert!(
                used <= MAX_BYTES,
                "current_bytes {used} exceeded max {MAX_BYTES}"
            );
        }
        assert!(cache.inner.lock().unwrap().current_bytes() <= 200);
    }

    #[test]
    fn lru_touch_protects_recently_used_entry() {
        // Budget holds exactly two 100-byte entries.
        let cache = TextureCache::new(200);
        cache.insert("a".into(), dummy_texture(), 100);
        cache.insert("b".into(), dummy_texture(), 100);

        // Touch "a" so "b" becomes the least-recently-used entry.
        assert!(cache.get("a").is_some());

        // Inserting "c" must evict "b", not the just-touched "a".
        cache.insert("c".into(), dummy_texture(), 100);
        assert!(
            cache.get("a").is_some(),
            "recently used entry should survive"
        );
        assert!(cache.get("c").is_some(), "newest entry should be present");
        assert!(
            cache.get("b").is_none(),
            "LRU entry should have been evicted"
        );
    }
}
