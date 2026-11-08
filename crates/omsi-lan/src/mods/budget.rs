//! The memory a server's work for its players may hold at once, in two budgets kept apart:
//!
//! - **transfers** (`transfers`): what one request holds while it is served - a list read in,
//!   a pieces plan, a file sent whole, a list answered (`MANIFEST`), a file put together from
//!   its pieces. One that would pass the budget is told `ERR busy` (or waits for room), and one
//!   larger than all of it (no size is refused) comes when no other transfer holds anything.
//! - **kept lists** (`kept_lists`): the offer each connection keeps for its life (its list,
//!   its lease, the files it still wants). Kept apart so that a connection's own list never
//!   stands in the way of its transfers: a file of a GB sent whole by a connection holding
//!   its offer came alone before, and never could.
//!
//! A hundred connections each announcing a list of 256 MB, or a file of a GB, took the
//! server's memory before there were budgets.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// What the transfers of every connection may hold at once.
pub(crate) const TRANSFER_MEMORY: u64 = 512 << 20;

/// What the lists every connection keeps may hold at once.
pub(crate) const KEPT_LIST_MEMORY: u64 = 256 << 20;

/// Bytes a budget may hold, and what it holds now.
pub(crate) struct Budget {
    held: AtomicU64,
    max: u64,
}

/// Bytes of a budget held by one request, given back when it is done.
pub(crate) struct Held<'a>(&'a AtomicU64, u64);

impl Drop for Held<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(self.1, Ordering::AcqRel);
    }
}

impl Budget {
    pub(crate) const fn new(max: u64) -> Budget {
        Budget { held: AtomicU64::new(0), max }
    }

    /// `n` bytes of it, if they are free - or if nothing is held: a request larger than all
    /// of it comes alone.
    pub(crate) fn hold(&self, n: u64) -> Option<Held<'_>> {
        self.held.try_update(Ordering::AcqRel, Ordering::Acquire, |h| (h + n <= self.max || h == 0).then_some(h + n)).ok().map(|_| Held(&self.held, n))
    }

    /// `hold`, waiting up to `wait` for the room (for work that has no `busy` to answer).
    pub(crate) fn hold_waiting(&self, n: u64, wait: Duration) -> Option<Held<'_>> {
        let until = Instant::now() + wait;
        loop {
            if let Some(h) = self.hold(n) {
                return Some(h);
            }
            if Instant::now() >= until {
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Bytes held now.
    #[cfg(test)]
    pub(crate) fn held(&self) -> u64 {
        self.held.load(Ordering::Acquire)
    }
}

static TRANSFERS: Budget = Budget::new(TRANSFER_MEMORY);
static KEPT_LISTS: Budget = Budget::new(KEPT_LIST_MEMORY);

/// The game's budget for what its requests hold while they are served.
pub(crate) fn transfers() -> &'static Budget {
    &TRANSFERS
}

/// The game's budget for the offers its connections keep.
pub(crate) fn kept_lists() -> &'static Budget {
    &KEPT_LISTS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_budget_holds_so_much_at_most_and_a_larger_request_comes_alone() {
        let b = Budget::new(100);
        let all = b.hold(100).unwrap();
        assert!(b.hold(1).is_none(), "one more is told: busy");
        drop(all);
        assert!(b.hold(1).is_some());
        // larger than all of it: alone
        let big = b.hold(250).unwrap();
        assert!(b.hold(1).is_none());
        // and one waiting for room gets it once the other is done
        std::thread::scope(|s| {
            s.spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                drop(big);
            });
            assert!(b.hold_waiting(10, Duration::from_secs(5)).is_some());
        });
        assert_eq!(b.held(), 0);
        assert!(b.hold_waiting(1, Duration::ZERO).is_some());
    }
}
