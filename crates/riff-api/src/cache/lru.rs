//! Generic byte-budgeted LRU cache.
//!
//! Uses an arena-backed doubly-linked list for O(1) priority tracking and a
//! HashMap for O(1) lookup. Freed slots are recycled via a free list.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

enum Slot<K, V> {
    Occupied(Node<K, V>),
    Free,
}

struct Node<K, V> {
    key: K,
    value: V,
    byte_size: usize,
    prev: Option<usize>,
    next: Option<usize>,
}

/// Byte-budgeted LRU cache. `byte_size` is caller-supplied on insert.
pub(crate) struct LruCache<K, V> {
    nodes: Vec<Slot<K, V>>,
    free: Vec<usize>,
    map: HashMap<K, usize>,
    head: Option<usize>,
    tail: Option<usize>,
    current_bytes: usize,
    max_bytes: usize,
}

impl<K, V> LruCache<K, V>
where
    K: Eq + Hash + Clone,
{
    pub(crate) fn new(max_bytes: usize) -> Self {
        Self {
            nodes: Vec::new(),
            free: Vec::new(),
            map: HashMap::new(),
            head: None,
            tail: None,
            current_bytes: 0,
            max_bytes,
        }
    }

    /// Look up a value, promoting it to most-recently-used.
    pub(crate) fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        let idx = *self.map.get(key)?;
        self.move_to_front(idx);
        Some(&self.node(idx).value)
    }

    /// Insert a value with the given byte cost. Rejects items larger than the
    /// budget. Replaces existing entries and evicts LRU entries as needed.
    pub(crate) fn insert(&mut self, key: K, value: V, byte_size: usize) {
        if byte_size > self.max_bytes {
            return;
        }

        // Replace existing entry to avoid double-counting.
        if let Some(old_idx) = self.map.remove(&key) {
            self.unlink(old_idx);
            let old_bytes = self.node(old_idx).byte_size;
            self.current_bytes -= old_bytes;
            self.free(old_idx);
        }

        // Evict LRU entries until the new item fits.
        while self.current_bytes + byte_size > self.max_bytes {
            let Some(victim) = self.pop_back() else { break };
            let (victim_key, victim_bytes) = {
                let n = self.node(victim);
                (n.key.clone(), n.byte_size)
            };
            self.map.remove(&victim_key);
            self.current_bytes -= victim_bytes;
            self.free(victim);
        }

        let idx = self.alloc(Node {
            key: key.clone(),
            value,
            byte_size,
            prev: None,
            next: None,
        });
        self.push_front(idx);
        self.map.insert(key, idx);
        self.current_bytes += byte_size;
    }

    pub(crate) fn peek<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        let idx = *self.map.get(key)?;
        Some(&self.node(idx).value)
    }

    /// Get or insert a value, then promote value to MRU.
    pub(crate) fn get_or_insert_mut<F>(&mut self, key: K, byte_size: usize, default: F) -> &mut V
    where
        F: FnOnce() -> V,
    {
        if let Some(&idx) = self.map.get(&key) {
            self.move_to_front(idx);
            return &mut self.node_mut(idx).value;
        }
        let idx = self.alloc(Node {
            key: key.clone(),
            value: default(),
            byte_size,
            prev: None,
            next: None,
        });
        self.push_front(idx);
        self.map.insert(key, idx);
        self.current_bytes += byte_size;
        self.evict_to_budget(Some(idx));
        &mut self.node_mut(idx).value
    }

    /// Update the recorded byte size and re-enforce the budget. No-op if absent.
    pub(crate) fn set_size(&mut self, key: &K, new_size: usize) {
        let Some(&idx) = self.map.get(key) else {
            return;
        };
        let old = self.node(idx).byte_size;
        self.node_mut(idx).byte_size = new_size;
        self.current_bytes = self.current_bytes - old + new_size;
        self.evict_to_budget(Some(idx));
    }

    pub(crate) fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        let idx = self.map.remove(key)?;
        self.unlink(idx);
        let node = std::mem::replace(&mut self.nodes[idx], Slot::Free);
        self.free.push(idx);
        match node {
            Slot::Occupied(n) => {
                self.current_bytes -= n.byte_size;
                Some(n.value)
            }
            Slot::Free => None,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.nodes.clear();
        self.free.clear();
        self.map.clear();
        self.head = None;
        self.tail = None;
        self.current_bytes = 0;
    }

    fn node(&self, idx: usize) -> &Node<K, V> {
        match &self.nodes[idx] {
            Slot::Occupied(n) => n,
            Slot::Free => unreachable!("access to freed slot {idx}"),
        }
    }

    fn node_mut(&mut self, idx: usize) -> &mut Node<K, V> {
        match &mut self.nodes[idx] {
            Slot::Occupied(n) => n,
            Slot::Free => unreachable!("access to freed slot {idx}"),
        }
    }

    fn alloc(&mut self, node: Node<K, V>) -> usize {
        if let Some(idx) = self.free.pop() {
            self.nodes[idx] = Slot::Occupied(node);
            idx
        } else {
            self.nodes.push(Slot::Occupied(node));
            self.nodes.len() - 1
        }
    }

    fn free(&mut self, idx: usize) {
        self.nodes[idx] = Slot::Free;
        self.free.push(idx);
    }

    fn unlink(&mut self, idx: usize) {
        let (prev, next) = {
            let n = self.node(idx);
            (n.prev, n.next)
        };
        match prev {
            Some(p) => self.node_mut(p).next = next,
            None => self.head = next,
        }
        match next {
            Some(nx) => self.node_mut(nx).prev = prev,
            None => self.tail = prev,
        }
        let n = self.node_mut(idx);
        n.prev = None;
        n.next = None;
    }

    fn push_front(&mut self, idx: usize) {
        let old_head = self.head;
        {
            let n = self.node_mut(idx);
            n.prev = None;
            n.next = old_head;
        }
        match old_head {
            Some(h) => self.node_mut(h).prev = Some(idx),
            None => self.tail = Some(idx),
        }
        self.head = Some(idx);
    }

    fn move_to_front(&mut self, idx: usize) {
        if self.head == Some(idx) {
            return;
        }
        self.unlink(idx);
        self.push_front(idx);
    }

    fn pop_back(&mut self) -> Option<usize> {
        let idx = self.tail?;
        self.unlink(idx);
        Some(idx)
    }

    /// Evict LRU entries until within budget. `keep` is protected from eviction.
    fn evict_to_budget(&mut self, keep: Option<usize>) {
        while self.current_bytes > self.max_bytes {
            let Some(tail) = self.tail else { break };
            if Some(tail) == keep {
                break;
            }
            self.unlink(tail);
            let (victim_key, victim_bytes) = {
                let n = self.node(tail);
                (n.key.clone(), n.byte_size)
            };
            self.map.remove(&victim_key);
            self.current_bytes -= victim_bytes;
            self.free(tail);
        }
    }
}

#[cfg(test)]
impl<K, V> LruCache<K, V>
where
    K: Eq + Hash + Clone + std::fmt::Debug,
{
    /// Keys ordered from MRU (head) to LRU (tail).
    fn order(&self) -> Vec<K> {
        let mut out = Vec::new();
        let mut cur = self.head;
        while let Some(idx) = cur {
            let n = self.node(idx);
            out.push(n.key.clone());
            cur = n.next;
        }
        out
    }

    fn occupied(&self) -> usize {
        self.nodes
            .iter()
            .filter(|s| matches!(s, Slot::Occupied(_)))
            .count()
    }

    pub(crate) fn current_bytes(&self) -> usize {
        self.current_bytes
    }

    /// Validate structural invariants (list consistency, map size, byte accounting).
    pub(crate) fn check_invariants(&self) {
        let forward = self.order();
        assert_eq!(
            forward.len(),
            self.occupied(),
            "list length must equal occupied slots"
        );
        assert_eq!(
            self.map.len(),
            self.occupied(),
            "map size must equal occupied slots"
        );

        let mut back = Vec::new();
        let mut cur = self.tail;
        while let Some(idx) = cur {
            let n = self.node(idx);
            back.push(n.key.clone());
            cur = n.prev;
        }
        back.reverse();
        assert_eq!(forward, back, "forward and backward traversals must agree");

        let summed: usize = self
            .nodes
            .iter()
            .filter_map(|s| match s {
                Slot::Occupied(n) => Some(n.byte_size),
                Slot::Free => None,
            })
            .sum();
        assert_eq!(
            summed, self.current_bytes,
            "current_bytes must equal the sum of node sizes"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_reports_zero_and_misses() {
        let mut cache: LruCache<String, i32> = LruCache::new(1024);
        assert_eq!(cache.current_bytes(), 0);
        assert_eq!(cache.occupied(), 0);
        assert!(cache.get("absent").is_none());
    }

    #[test]
    fn insert_then_get_roundtrip() {
        let mut cache: LruCache<String, i32> = LruCache::new(1024);
        cache.insert("a".into(), 1, 100);
        assert_eq!(cache.get("a"), Some(&1));
        assert_eq!(cache.current_bytes(), 100);
        assert_eq!(cache.occupied(), 1);
    }

    #[test]
    fn insert_orders_newest_first() {
        let mut cache: LruCache<String, i32> = LruCache::new(1000);
        cache.insert("a".into(), 1, 100);
        cache.insert("b".into(), 2, 100);
        cache.insert("c".into(), 3, 100);
        assert_eq!(cache.order(), vec!["c", "b", "a"]);
        cache.check_invariants();
    }

    #[test]
    fn reinsert_same_key_does_not_double_count() {
        let mut cache: LruCache<String, i32> = LruCache::new(1024);
        cache.insert("a".into(), 1, 100);
        cache.insert("a".into(), 2, 150);
        assert_eq!(cache.current_bytes(), 150);
        assert_eq!(cache.get("a"), Some(&2));
        cache.check_invariants();
    }

    #[test]
    fn item_larger_than_budget_is_rejected() {
        let mut cache: LruCache<String, i32> = LruCache::new(100);
        cache.insert("big".into(), 1, 200);
        assert!(cache.get("big").is_none());
        assert_eq!(cache.current_bytes(), 0);
    }

    #[test]
    fn eviction_keeps_within_budget() {
        const MAX_BYTES: usize = 250;
        let mut cache: LruCache<String, i32> = LruCache::new(MAX_BYTES);
        for i in 0..10 {
            cache.insert(format!("k{i}"), i, 100);
            assert!(cache.current_bytes() <= MAX_BYTES);
        }
        assert!(cache.current_bytes() <= 200);
        cache.check_invariants();
    }

    #[test]
    fn get_moves_entry_to_front() {
        let mut cache: LruCache<String, i32> = LruCache::new(1000);
        cache.insert("a".into(), 1, 100);
        cache.insert("b".into(), 2, 100);
        cache.insert("c".into(), 3, 100);
        assert_eq!(cache.get("a"), Some(&1));
        assert_eq!(cache.order(), vec!["a", "c", "b"]);
        cache.check_invariants();
    }

    #[test]
    fn lru_touch_protects_recently_used_entry() {
        let mut cache: LruCache<String, i32> = LruCache::new(200);
        cache.insert("a".into(), 1, 100);
        cache.insert("b".into(), 2, 100);

        assert!(cache.get("a").is_some());

        cache.insert("c".into(), 3, 100);
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

    #[test]
    fn free_list_recycles_slots_under_churn() {
        let mut cache: LruCache<String, i32> = LruCache::new(200);
        cache.insert("a".into(), 0, 100);
        cache.insert("b".into(), 0, 100);
        for i in 0..50 {
            cache.insert(format!("k{i}"), i, 100);
        }
        assert!(
            cache.nodes.len() <= 2,
            "arena grew to {} slots; freed slots not recycled",
            cache.nodes.len()
        );
        cache.check_invariants();
    }

    #[test]
    fn clear_empties_everything() {
        let mut cache: LruCache<String, i32> = LruCache::new(1000);
        cache.insert("a".into(), 1, 100);
        cache.insert("b".into(), 2, 100);
        cache.clear();
        assert_eq!(cache.current_bytes(), 0);
        assert_eq!(cache.occupied(), 0);
        assert!(cache.get("a").is_none());
        cache.check_invariants();
        cache.insert("c".into(), 3, 100);
        assert_eq!(cache.get("c"), Some(&3));
    }
}
