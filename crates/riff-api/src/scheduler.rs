//! Admission control for outbound requests.
//!
//! Reads take a slot from an [`AdmissionQueue`], which caps concurrency and
//! orders waiters by epoch, then [`LoadPriority`], then arrival. Mutations use
//! a [`Lane`], which only caps concurrency since reordering them would be
//! wrong.

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, OnceCell, OwnedSemaphorePermit, Semaphore, SemaphorePermit};

/// Epoch for work not tied to any view. Below every real epoch, so background
/// work never outranks a view.
pub const BACKGROUND_EPOCH: u64 = 0;

/// Ordered low to high so the derived `Ord` matches scheduling priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LoadPriority {
    /// Warming, speculative prefetch.
    Background = 0,
    /// Images not yet on screen.
    Offscreen = 1,
    /// Images and metadata for the current page.
    Visible = 2,
    /// Header / hero artwork.
    Hero = 3,
    /// Playback transport state. Not tied to a view, and a stale transport is
    /// noticed immediately, so it outranks artwork.
    Transport = 4,
}

impl LoadPriority {
    /// Nothing on screen is waiting on this, so it can be dropped under load.
    fn is_speculative(self) -> bool {
        self <= LoadPriority::Offscreen
    }
}

/// How important a request is, and how current the view that asked for it was
/// *when it asked*.
///
/// The epoch is a value, not something looked up at admission time: a request
/// is often created long before it reaches the queue, and reading the epoch
/// late would promote exactly the stale work this demotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Load {
    pub priority: LoadPriority,
    pub epoch: u64,
}

impl Load {
    pub const fn new(priority: LoadPriority, epoch: u64) -> Self {
        Self { priority, epoch }
    }

    /// Work with no view behind it, at [`BACKGROUND_EPOCH`].
    pub const fn background() -> Self {
        Self::new(LoadPriority::Background, BACKGROUND_EPOCH)
    }
}

/// Permission to make one request. Hold it for as long as the request runs.
pub struct Slot {
    #[allow(dead_code, reason = "held for its Drop, which frees the permit")]
    permit: OwnedSemaphorePermit,
}

/// Outcome of asking for a slot.
pub enum Admission {
    Granted(Slot),
    /// Evicted from the backlog, but a view is waiting: run it unslotted.
    Unslotted,
    /// Evicted speculative work: skip the request.
    Denied,
}

/// Declaration order is comparison order. Greater key wins.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    epoch: u64,
    class: LoadPriority,
    // Reverse so a smaller seq compares greater, i.e. FIFO within a tier.
    seq: Reverse<u64>,
}

struct Ticket {
    key: Key,
    grant: oneshot::Sender<Slot>,
}

/// Priority queue gating how many requests run concurrently.
pub struct AdmissionQueue {
    name: &'static str,
    max_concurrency: usize,
    cap: usize,
    seq: AtomicU64,
    tx: OnceCell<mpsc::UnboundedSender<Ticket>>,
}

impl AdmissionQueue {
    pub fn new(name: &'static str, max_concurrency: usize, cap: usize) -> Self {
        Self {
            name,
            max_concurrency,
            cap,
            seq: AtomicU64::new(0),
            tx: OnceCell::new(),
        }
    }

    async fn sender(&self) -> mpsc::UnboundedSender<Ticket> {
        self.tx
            .get_or_init(|| async {
                let (tx, rx) = mpsc::unbounded_channel();
                let sem = Arc::new(Semaphore::new(self.max_concurrency));
                tokio::spawn(run(self.name, rx, sem, self.cap));
                tx
            })
            .await
            .clone()
    }

    /// Waits for a slot. On eviction, speculative work is [`Admission::Denied`]
    /// and everything else runs [`Admission::Unslotted`].
    pub async fn admit(&self, load: Load) -> Admission {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let key = Key {
            epoch: load.epoch,
            class: load.priority,
            seq: Reverse(seq),
        };
        let (grant, wait) = oneshot::channel();
        let slot = match self.sender().await.send(Ticket { key, grant }) {
            Ok(()) => wait.await.ok(),
            Err(_) => None,
        };

        match slot {
            Some(slot) => Admission::Granted(slot),
            None if load.priority.is_speculative() => Admission::Denied,
            None => Admission::Unslotted,
        }
    }
}

async fn run(
    name: &'static str,
    mut rx: mpsc::UnboundedReceiver<Ticket>,
    sem: Arc<Semaphore>,
    cap: usize,
) {
    let mut q: BTreeMap<Key, Ticket> = BTreeMap::new();

    loop {
        if q.is_empty() {
            match rx.recv().await {
                Some(t) => insert_capped(name, &mut q, t, cap),
                None => return, // all senders dropped
            }
        }

        while let Ok(t) = rx.try_recv() {
            insert_capped(name, &mut q, t, cap);
        }

        let permit = tokio::select! {
            p = sem.clone().acquire_owned() => p.expect("semaphore open"),
            maybe = rx.recv() => match maybe {
                Some(t) => { insert_capped(name, &mut q, t, cap); continue; }
                None => return,
            }
        };

        if let Some((_key, ticket)) = q.pop_last() {
            // If the waiter is gone, the permit drops here and returns to the pool.
            let _ = ticket.grant.send(Slot { permit });
        }
    }
}

fn insert_capped(name: &'static str, q: &mut BTreeMap<Key, Ticket>, t: Ticket, cap: usize) {
    q.insert(t.key, t);
    while q.len() > cap {
        // Dropping the ticket closes its grant channel, which is how the waiter
        // learns it was evicted.
        if let Some((key, _)) = q.pop_first() {
            debug!(
                "{name}: backlog full at {cap}, evicting epoch {} {:?}",
                key.epoch, key.class
            );
        }
    }
}

/// A serialization lane for mutations. No priority: waiters are granted in
/// arrival order.
///
/// Lanes are per concern rather than global, so a slow playlist rename cannot
/// delay a volume change.
pub struct Lane {
    permits: Semaphore,
}

impl Lane {
    /// A lane admitting `width` concurrent mutations. Use 1 to serialize.
    pub fn new(width: usize) -> Self {
        Self {
            permits: Semaphore::new(width),
        }
    }

    /// Wait for the lane. The mutation runs while the returned guard is alive.
    pub async fn enter(&self) -> SemaphorePermit<'_> {
        self.permits
            .acquire()
            .await
            .expect("lane semaphore is never closed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    /// Time is paused, so this costs nothing and cannot flake.
    const SETTLE: Duration = Duration::from_millis(10);

    fn slot(admission: Admission) -> Option<Slot> {
        match admission {
            Admission::Granted(slot) => Some(slot),
            _ => None,
        }
    }

    fn visible(epoch: u64) -> Load {
        Load::new(LoadPriority::Visible, epoch)
    }

    fn queue(max_concurrency: usize, cap: usize) -> Arc<AdmissionQueue> {
        Arc::new(AdmissionQueue::new("test", max_concurrency, cap))
    }

    #[tokio::test(start_paused = true)]
    async fn priority_ordering_within_same_epoch() {
        let q = queue(1, 16);

        let held = slot(q.admit(visible(1)).await).unwrap();

        let q1 = Arc::clone(&q);
        let offscreen =
            tokio::spawn(
                async move { slot(q1.admit(Load::new(LoadPriority::Offscreen, 1)).await) },
            );
        // Enqueued first, so plain FIFO would let this win.
        tokio::time::sleep(SETTLE).await;
        let q2 = Arc::clone(&q);
        let hero =
            tokio::spawn(async move { slot(q2.admit(Load::new(LoadPriority::Hero, 1)).await) });
        tokio::time::sleep(SETTLE).await;

        drop(held);

        let hero_slot = hero.await.unwrap();
        assert!(hero_slot.is_some(), "hero should be granted the free slot");

        tokio::time::sleep(SETTLE).await;
        assert!(!offscreen.is_finished());

        drop(hero_slot);
        assert!(offscreen.await.unwrap().is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn fifo_within_a_tier() {
        let q = queue(1, 16);
        let order = Arc::new(Mutex::new(Vec::new()));

        let held = slot(q.admit(visible(1)).await).unwrap();

        let mut handles = Vec::new();
        for i in 0..4u64 {
            let q = Arc::clone(&q);
            let order = Arc::clone(&order);
            handles.push(tokio::spawn(async move {
                let granted = slot(q.admit(visible(1)).await);
                assert!(granted.is_some(), "waiter {i} was evicted");
                order.lock().unwrap().push(i);
            }));
            tokio::time::sleep(SETTLE).await;
        }

        drop(held);
        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(*order.lock().unwrap(), vec![0, 1, 2, 3]);
    }

    #[tokio::test(start_paused = true)]
    async fn navigation_demotes_older_epoch() {
        let q = queue(1, 16);
        let held = slot(q.admit(visible(1)).await).unwrap();

        let q1 = Arc::clone(&q);
        let old_view = tokio::spawn(async move { slot(q1.admit(visible(1)).await) });
        tokio::time::sleep(SETTLE).await;
        let q2 = Arc::clone(&q);
        let new_view = tokio::spawn(async move { slot(q2.admit(visible(2)).await) });
        tokio::time::sleep(SETTLE).await;

        drop(held);

        let new_slot = new_view.await.unwrap();
        assert!(new_slot.is_some(), "newer epoch should win the free slot");
        assert!(!old_view.is_finished());

        drop(new_slot);
        assert!(old_view.await.unwrap().is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn epoch_outranks_class() {
        let q = queue(1, 16);
        let held = slot(q.admit(visible(5)).await).unwrap();

        let q1 = Arc::clone(&q);
        let old_hero =
            tokio::spawn(async move { slot(q1.admit(Load::new(LoadPriority::Hero, 4)).await) });
        tokio::time::sleep(SETTLE).await;
        let q2 = Arc::clone(&q);
        let new_offscreen =
            tokio::spawn(
                async move { slot(q2.admit(Load::new(LoadPriority::Offscreen, 5)).await) },
            );
        tokio::time::sleep(SETTLE).await;

        drop(held);

        let new_slot = new_offscreen.await.unwrap();
        assert!(new_slot.is_some());
        assert!(
            !old_hero.is_finished(),
            "an older view's hero image should yield to the current view"
        );

        drop(new_slot);
        assert!(old_hero.await.unwrap().is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn background_never_outranks_a_view() {
        let q = queue(1, 16);
        let held = slot(q.admit(visible(1)).await).unwrap();

        let q1 = Arc::clone(&q);
        let warm = tokio::spawn(async move { slot(q1.admit(Load::background()).await) });
        tokio::time::sleep(SETTLE).await;
        let q2 = Arc::clone(&q);
        let view =
            tokio::spawn(
                async move { slot(q2.admit(Load::new(LoadPriority::Offscreen, 1)).await) },
            );
        tokio::time::sleep(SETTLE).await;

        drop(held);

        let view_slot = view.await.unwrap();
        assert!(view_slot.is_some());
        assert!(!warm.is_finished(), "warming should yield to any view");

        drop(view_slot);
        assert!(warm.await.unwrap().is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn backlog_cap_evicts_least_relevant() {
        let cap = 4;
        let q = queue(1, cap);
        let held = slot(q.admit(visible(10)).await).unwrap();

        let overflow = 2;
        let mut handles = Vec::new();
        for epoch in 0..(cap as u64 + overflow) {
            let q = Arc::clone(&q);
            // Drop the `Slot` in-task; returning it parks the permit unread.
            handles.push(tokio::spawn(async move {
                slot(q.admit(visible(epoch)).await).is_some()
            }));
            tokio::time::sleep(SETTLE).await;
        }

        drop(held);

        let mut granted = Vec::new();
        let mut evicted = Vec::new();
        for (epoch, h) in handles.into_iter().enumerate() {
            if h.await.unwrap() {
                granted.push(epoch as u64);
            } else {
                evicted.push(epoch as u64);
            }
        }

        assert_eq!(evicted, vec![0, 1]);
        assert_eq!(granted, vec![2, 3, 4, 5]);
    }

    #[tokio::test(start_paused = true)]
    async fn eviction_denies_speculative_work() {
        let q = queue(1, 1);
        let held = slot(q.admit(visible(10)).await).unwrap();

        let q1 = Arc::clone(&q);
        let warm = tokio::spawn(async move { q1.admit(Load::background()).await });
        tokio::time::sleep(SETTLE).await;
        // Overflows the backlog.
        let q2 = Arc::clone(&q);
        let view = tokio::spawn(async move { q2.admit(visible(11)).await });
        tokio::time::sleep(SETTLE).await;

        assert!(matches!(warm.await.unwrap(), Admission::Denied));
        drop(held);
        assert!(matches!(view.await.unwrap(), Admission::Granted(_)));
    }

    #[tokio::test(start_paused = true)]
    async fn eviction_lets_a_view_run_unslotted() {
        let q = queue(1, 1);
        let held = slot(q.admit(visible(10)).await).unwrap();

        let q1 = Arc::clone(&q);
        let old_view = tokio::spawn(async move { q1.admit(visible(1)).await });
        tokio::time::sleep(SETTLE).await;
        let q2 = Arc::clone(&q);
        let new_view = tokio::spawn(async move { q2.admit(visible(11)).await });
        tokio::time::sleep(SETTLE).await;

        assert!(matches!(old_view.await.unwrap(), Admission::Unslotted));
        drop(held);
        assert!(matches!(new_view.await.unwrap(), Admission::Granted(_)));
    }

    // Lane

    #[tokio::test(start_paused = true)]
    async fn lane_serializes_mutations() {
        let lane = Arc::new(Lane::new(1));
        let in_flight = Arc::new(AtomicU64::new(0));
        let max_seen = Arc::new(AtomicU64::new(0));

        let mut handles = Vec::new();
        for _ in 0..5 {
            let lane = Arc::clone(&lane);
            let in_flight = Arc::clone(&in_flight);
            let max_seen = Arc::clone(&max_seen);
            handles.push(tokio::spawn(async move {
                let _guard = lane.enter().await;
                let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                max_seen.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(SETTLE).await;
                in_flight.fetch_sub(1, Ordering::SeqCst);
            }));
        }

        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(max_seen.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn lane_grants_waiters_in_arrival_order() {
        let lane = Arc::new(Lane::new(1));
        let order = Arc::new(Mutex::new(Vec::new()));

        let held = lane.enter().await;

        let mut handles = Vec::new();
        for i in 0..4u64 {
            let lane = Arc::clone(&lane);
            let order = Arc::clone(&order);
            handles.push(tokio::spawn(async move {
                let _guard = lane.enter().await;
                order.lock().unwrap().push(i);
            }));
            tokio::time::sleep(SETTLE).await;
        }

        drop(held);
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(*order.lock().unwrap(), vec![0, 1, 2, 3]);
    }

    #[tokio::test(start_paused = true)]
    async fn separate_lanes_do_not_block_each_other() {
        let library = Arc::new(Lane::new(1));
        let player = Lane::new(1);

        let library_clone = Arc::clone(&library);
        let blocker = tokio::spawn(async move {
            let _guard = library_clone.enter().await;
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        tokio::time::sleep(SETTLE).await;

        let entered = tokio::time::timeout(Duration::from_millis(100), player.enter()).await;
        assert!(entered.is_ok(), "player lane blocked by a library mutation");

        blocker.abort();
    }
}
