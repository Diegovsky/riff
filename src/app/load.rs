//! Navigation epoch, and how requests are tagged with it.
//!
//! Every outbound read carries a [`Load`]: a priority plus the navigation epoch
//! at the time the request was issued. The api layer's queues order waiters by
//! epoch first, so a newer view preempts a previous view's leftovers.
//!
//! The epoch is captured here, at the call site, not looked up by the api
//! layer: requests are often created well before they reach the queue, and
//! reading the epoch late would promote the stale work this demotes.

use std::sync::atomic::{AtomicU64, Ordering};

use riff_api::{Load, LoadPriority, BACKGROUND_EPOCH};

/// Starts above `BACKGROUND_EPOCH` so the first view already outranks warming.
static NAV_EPOCH: AtomicU64 = AtomicU64::new(BACKGROUND_EPOCH + 1);

/// Mark a navigation, returning the new epoch. Called for any change of what the
/// user is looking at, including going back.
pub fn bump_epoch() -> u64 {
    NAV_EPOCH.fetch_add(1, Ordering::Relaxed) + 1
}

pub fn current_epoch() -> u64 {
    NAV_EPOCH.load(Ordering::Relaxed)
}

/// Tag a request with `priority` and the epoch that is current right now.
pub fn at(priority: LoadPriority) -> Load {
    Load::new(priority, current_epoch())
}

pub fn visible() -> Load {
    at(LoadPriority::Visible)
}

/// Artwork that is bound but scrolled out of view.
pub fn offscreen() -> Load {
    at(LoadPriority::Offscreen)
}

pub fn hero() -> Load {
    at(LoadPriority::Hero)
}

/// Playback transport. Rides the current epoch so navigation never demotes it.
pub fn transport() -> Load {
    at(LoadPriority::Transport)
}

pub fn background() -> Load {
    Load::background()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_starts_above_background_and_increases() {
        let first = current_epoch();
        assert!(first > BACKGROUND_EPOCH);
        assert_eq!(bump_epoch(), first + 1);
        assert_eq!(current_epoch(), first + 1);
    }

    #[test]
    fn background_is_below_every_view() {
        assert!(background().epoch < visible().epoch);
        assert_eq!(background().priority, LoadPriority::Background);
    }

    #[test]
    fn transport_outranks_artwork_in_the_same_epoch() {
        let transport = transport();
        let hero = hero();
        assert_eq!(transport.epoch, hero.epoch);
        assert!(transport.priority > hero.priority);
    }

    #[test]
    fn view_priorities_are_ordered() {
        assert!(hero().priority > visible().priority);
        assert!(visible().priority > offscreen().priority);
        assert!(offscreen().priority > background().priority);
    }
}
