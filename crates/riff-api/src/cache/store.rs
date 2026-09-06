//! In-memory LRU cache for Riff domain types.
//!
//! No disk I/O, no TTL. Supports type-erased single values and paginated
//! lists keyed by offset.

use std::any::Any;
use std::collections::HashMap;
use std::sync::Mutex;

use riff_config::BYTES_PER_ITEM;

use super::lru::LruCache;
use super::CacheKey;

type Stored = Box<dyn Any + Send + Sync>;

enum EntryData {
    Single(Stored),
    Paginated(PaginatedList),
}

struct PaginatedList {
    pages: HashMap<usize, Vec<Stored>>,
    total: usize,
}

pub struct Store {
    store: Mutex<LruCache<CacheKey, EntryData>>,
}

impl Store {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            store: Mutex::new(LruCache::new(max_bytes)),
        }
    }

    pub fn get_single<T: Clone + Send + Sync + 'static>(&self, key: &CacheKey) -> Option<T> {
        let mut store = self.store.lock().unwrap();
        match store.get(key)? {
            EntryData::Single(boxed) => boxed.downcast_ref::<T>().cloned(),
            _ => None,
        }
    }

    pub fn insert_single<T: Clone + Send + Sync + 'static>(&self, key: &CacheKey, value: T) {
        let mut store = self.store.lock().unwrap();
        store.insert(
            key.clone(),
            EntryData::Single(Box::new(value)),
            BYTES_PER_ITEM,
        );
    }

    /// Return the page at `offset`, or `None` if not cached.
    ///
    /// Note: This method doesn't handle cases where the request page size changes.
    /// If the page size changes the cache has to be rebuilt.
    pub fn get_paginated<T: Clone + Send + Sync + 'static>(
        &self,
        key: &CacheKey,
        offset: usize,
    ) -> Option<Vec<T>> {
        let mut store = self.store.lock().unwrap();
        let Some(EntryData::Paginated(list)) = store.get(key) else {
            return None;
        };

        let Some(page) = list.pages.get(&offset) else {
            debug!("cache: miss {:?} offset {offset} (no page)", key);
            return None;
        };

        match downcast_page::<T>(page) {
            Some(items) => {
                debug!(
                    "cache: hit {:?} offset {offset} ({} items)",
                    key,
                    items.len()
                );
                Some(items)
            }
            None => {
                // Items are present but the wrong type then treat the item as a miss
                // so we fall through to disk/network instead of returning bad data.
                warn!(
                    "cache: downcast failure for {:?} offset {offset} (stored {} items, type {})",
                    key,
                    page.len(),
                    std::any::type_name::<T>()
                );
                None
            }
        }
    }

    /// Store the page at `offset`, replacing any previous page there.
    pub fn append_paginated<T: Clone + Send + Sync + 'static>(
        &self,
        key: &CacheKey,
        offset: usize,
        items: Vec<T>,
        total: usize,
    ) {
        debug!(
            "cache: store {:?} offset={} count={} total={} type={}",
            key,
            offset,
            items.len(),
            total,
            std::any::type_name::<T>()
        );
        let mut store = self.store.lock().unwrap();

        // Get or create the paginated entry, promoting it to most-recently-used.
        let entry = store.get_or_insert_mut(key.clone(), 0, || {
            EntryData::Paginated(PaginatedList {
                pages: HashMap::new(),
                total,
            })
        });

        // Only paginated entries accept pages. A key already holding a single
        // object is left untouched (matching the previous behaviour).
        let new_bytes = if let EntryData::Paginated(list) = entry {
            list.total = total;
            let page: Vec<Stored> = items.into_iter().map(|it| Box::new(it) as Stored).collect();
            list.pages.insert(offset, page);
            let item_count: usize = list.pages.values().map(|p| p.len()).sum();
            Some(item_count * BYTES_PER_ITEM)
        } else {
            None
        };

        // Re-account the grown entry and evict others down to the budget.
        if let Some(new_bytes) = new_bytes {
            store.set_size(key, new_bytes);
        }
    }

    pub fn get_total(&self, key: &CacheKey) -> Option<usize> {
        let store = self.store.lock().unwrap();
        match store.peek(key)? {
            EntryData::Paginated(l) => Some(l.total),
            _ => None,
        }
    }

    pub fn remove(&self, key: &CacheKey) {
        self.store.lock().unwrap().remove(key);
    }

    pub fn clear(&self) {
        self.store.lock().unwrap().clear();
    }
}

fn downcast_page<T: Clone + 'static>(page: &[Stored]) -> Option<Vec<T>> {
    let mut out = Vec::with_capacity(page.len());
    for boxed in page {
        out.push(boxed.downcast_ref::<T>()?.clone());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct FakeAlbum {
        id: String,
        title: String,
    }

    fn albums(range: std::ops::Range<usize>) -> Vec<FakeAlbum> {
        range
            .map(|i| FakeAlbum {
                id: format!("a{i}"),
                title: format!("Album {i}"),
            })
            .collect()
    }

    /// Invariant proof: delegates to the generic cache's structural check,
    /// which asserts `current_bytes` equals the sum of every entry's byte size
    /// and that the recency list and key map stay consistent.
    fn assert_bytes_consistent(store: &Store) {
        store.store.lock().unwrap().check_invariants();
    }

    #[test]
    fn std_any_roundtrip() {
        let val = FakeAlbum {
            id: "x".into(),
            title: "X".into(),
        };
        let boxed: Box<dyn std::any::Any> = Box::new(val.clone());
        assert!(boxed.downcast_ref::<FakeAlbum>().is_some());
    }

    #[test]
    fn any_downcast_roundtrip() {
        let val = FakeAlbum {
            id: "x".into(),
            title: "X".into(),
        };
        let boxed: Stored = Box::new(val.clone());
        assert_eq!(boxed.downcast_ref::<FakeAlbum>(), Some(&val));
        assert!(boxed.downcast_ref::<String>().is_none());
    }

    #[test]
    fn paginated_round_trip() {
        let store = Store::new(32 * 1024 * 1024);
        let key = CacheKey::ArtistAlbums("artist1".into());
        let items = albums(0..15);

        store.append_paginated(&key, 0, items.clone(), 15);

        assert_eq!(store.get_paginated::<FakeAlbum>(&key, 0), Some(items));
    }

    #[test]
    fn paginated_second_retrieval_same_result() {
        let store = Store::new(32 * 1024 * 1024);
        let key = CacheKey::ArtistAlbums("artist2".into());
        let items = albums(0..10);

        store.append_paginated(&key, 0, items.clone(), 10);

        let first = store.get_paginated::<FakeAlbum>(&key, 0);
        let second = store.get_paginated::<FakeAlbum>(&key, 0);

        assert_eq!(first, second);
        assert_eq!(first, Some(items));
    }

    #[test]
    fn paginated_wrong_type_returns_miss() {
        let store = Store::new(32 * 1024 * 1024);
        let key = CacheKey::ArtistAlbums("artist3".into());
        store.append_paginated(
            &key,
            0,
            vec![FakeAlbum {
                id: "x".into(),
                title: "X".into(),
            }],
            1,
        );

        // Downcast to the wrong type must be a miss, not bad data.
        assert_eq!(store.get_paginated::<String>(&key, 0), None);
    }

    #[test]
    fn paginated_empty_page_is_hit() {
        let store = Store::new(32 * 1024 * 1024);
        let key = CacheKey::UserPlaylists("user1".into());

        // A genuinely empty collection: an empty page is stored at offset 0.
        store.append_paginated::<FakeAlbum>(&key, 0, vec![], 0);

        assert_eq!(store.get_paginated::<FakeAlbum>(&key, 0), Some(vec![]));
    }

    #[test]
    fn missing_offset_is_miss() {
        let store = Store::new(32 * 1024 * 1024);
        let key = CacheKey::SavedAlbums;
        store.append_paginated(&key, 0, albums(0..5), 20);

        // Only offset 0 was stored; any other offset is a miss.
        assert!(store.get_paginated::<FakeAlbum>(&key, 5).is_none());
        assert!(store.get_paginated::<FakeAlbum>(&key, 100).is_none());
    }

    #[test]
    fn distinct_offsets_are_stored_separately() {
        let store = Store::new(32 * 1024 * 1024);
        let key = CacheKey::SavedTracks;
        store.append_paginated(&key, 0, albums(0..5), 10);
        store.append_paginated(&key, 5, albums(5..10), 10);

        assert_eq!(
            store.get_paginated::<FakeAlbum>(&key, 0),
            Some(albums(0..5))
        );
        assert_eq!(
            store.get_paginated::<FakeAlbum>(&key, 5),
            Some(albums(5..10))
        );
        assert_bytes_consistent(&store);
    }

    #[test]
    fn get_total_returns_correct_value() {
        let store = Store::new(32 * 1024 * 1024);
        let key = CacheKey::SavedAlbums;

        assert_eq!(store.get_total(&key), None);

        store.append_paginated(
            &key,
            0,
            vec![FakeAlbum {
                id: "1".into(),
                title: "T".into(),
            }],
            42,
        );
        assert_eq!(store.get_total(&key), Some(42));
    }

    #[test]
    fn reload_same_offset_is_idempotent() {
        let store = Store::new(32 * 1024 * 1024);
        let key = CacheKey::SavedTracks;
        store.append_paginated(&key, 0, albums(0..10), 10);
        // Re-storing the same offset replaces the page; accounting must not grow.
        store.append_paginated(&key, 0, albums(0..10), 10);

        assert_eq!(
            store.get_paginated::<FakeAlbum>(&key, 0),
            Some(albums(0..10))
        );
        assert_eq!(
            store.store.lock().unwrap().current_bytes(),
            10 * BYTES_PER_ITEM
        );
        assert_bytes_consistent(&store);
    }

    #[test]
    fn multiple_pages_accounting_sums_all_items() {
        let store = Store::new(32 * 1024 * 1024);
        let key = CacheKey::SavedTracks;
        store.append_paginated(&key, 0, albums(0..5), 15);
        store.append_paginated(&key, 5, albums(5..10), 15);
        store.append_paginated(&key, 10, albums(10..15), 15);

        assert_eq!(
            store.store.lock().unwrap().current_bytes(),
            15 * BYTES_PER_ITEM
        );
        assert_bytes_consistent(&store);
    }

    #[test]
    fn single_roundtrip_and_type_mismatch() {
        let store = Store::new(32 * 1024 * 1024);
        let key = CacheKey::Album("x".into());
        store.insert_single(
            &key,
            FakeAlbum {
                id: "x".into(),
                title: "X".into(),
            },
        );

        assert_eq!(
            store.get_single::<FakeAlbum>(&key),
            Some(FakeAlbum {
                id: "x".into(),
                title: "X".into()
            })
        );
        // Wrong type must not alias into a bogus value.
        assert_eq!(store.get_single::<String>(&key), None);
        assert_bytes_consistent(&store);
    }

    #[test]
    fn single_reinsert_does_not_double_count() {
        let store = Store::new(32 * 1024 * 1024);
        let key = CacheKey::Album("x".into());
        store.insert_single(
            &key,
            FakeAlbum {
                id: "1".into(),
                title: "one".into(),
            },
        );
        let after_first = store.store.lock().unwrap().current_bytes();
        store.insert_single(
            &key,
            FakeAlbum {
                id: "2".into(),
                title: "two".into(),
            },
        );
        let after_second = store.store.lock().unwrap().current_bytes();

        assert_eq!(
            after_first, after_second,
            "overwriting a key must not grow usage"
        );
        assert_eq!(
            store.get_single::<FakeAlbum>(&key),
            Some(FakeAlbum {
                id: "2".into(),
                title: "two".into()
            })
        );
        assert_bytes_consistent(&store);
    }

    #[test]
    fn remove_and_clear_update_accounting() {
        let store = Store::new(32 * 1024 * 1024);
        let key = CacheKey::Album("x".into());
        store.insert_single(
            &key,
            FakeAlbum {
                id: "x".into(),
                title: "X".into(),
            },
        );
        assert!(store.store.lock().unwrap().current_bytes() > 0);

        store.remove(&key);
        assert_eq!(store.get_single::<FakeAlbum>(&key), None);
        assert_eq!(store.store.lock().unwrap().current_bytes(), 0);

        store.insert_single(
            &CacheKey::Album("y".into()),
            FakeAlbum {
                id: "y".into(),
                title: "Y".into(),
            },
        );
        store.clear();
        assert_eq!(store.store.lock().unwrap().current_bytes(), 0);
    }

    #[test]
    fn single_eviction_respects_budget_invariant() {
        // Budget holds exactly three single objects.
        let store = Store::new(BYTES_PER_ITEM * 3);
        for i in 0..8 {
            store.insert_single(
                &CacheKey::Album(format!("a{i}")),
                FakeAlbum {
                    id: format!("a{i}"),
                    title: format!("A {i}"),
                },
            );
            assert!(
                store.store.lock().unwrap().current_bytes() <= BYTES_PER_ITEM * 3,
                "usage must never exceed the budget"
            );
            assert_bytes_consistent(&store);
        }
        assert_eq!(
            store.store.lock().unwrap().current_bytes(),
            BYTES_PER_ITEM * 3
        );
    }

    #[test]
    fn lru_touch_protects_recently_used_single() {
        let store = Store::new(BYTES_PER_ITEM * 3);
        let a = CacheKey::Album("1".into());
        let b = CacheKey::Album("2".into());
        let c = CacheKey::Album("3".into());
        let d = CacheKey::Album("4".into());
        for k in [&a, &b, &c] {
            store.insert_single(
                k,
                FakeAlbum {
                    id: "x".into(),
                    title: "X".into(),
                },
            );
        }

        // Touch `a`, making `b` the least-recently-used entry.
        assert!(store.get_single::<FakeAlbum>(&a).is_some());

        // Inserting `d` should evict `b`.
        store.insert_single(
            &d,
            FakeAlbum {
                id: "x".into(),
                title: "X".into(),
            },
        );
        assert!(
            store.get_single::<FakeAlbum>(&a).is_some(),
            "touched entry survives"
        );
        assert!(store.get_single::<FakeAlbum>(&c).is_some());
        assert!(
            store.get_single::<FakeAlbum>(&d).is_some(),
            "newest entry present"
        );
        assert!(
            store.get_single::<FakeAlbum>(&b).is_none(),
            "LRU entry evicted"
        );
        assert_bytes_consistent(&store);
    }
}
