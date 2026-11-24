//! What loading hands to the simulation: an [`Inbox`] the loader's threads put things into
//! as they make them (a tile's lanes, a crossing's light program, a placed scenery object)
//! and the simulation takes out whole at the start of its step - on whichever thread it
//! steps. The simulation never reaches into the loader's lists, and the loader never into
//! the simulation's state.

use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Things handed over in the order they were put in, taken all at once.
///
/// Putting in takes a short lock; asking whether anything came does not (most steps find
/// nothing), and neither side holds the lock while it works on the things.
pub struct Inbox<T> {
    pending: AtomicBool,
    items: Mutex<Vec<T>>,
}

impl<T> Default for Inbox<T> {
    fn default() -> Inbox<T> {
        Inbox {
            pending: AtomicBool::new(false),
            items: Mutex::new(Vec::new()),
        }
    }
}

impl<T> Inbox<T> {
    pub fn new() -> Inbox<T> {
        Inbox::default()
    }

    /// Hand `item` over.
    pub fn push(&self, item: T) {
        let mut items = self.items.lock();
        items.push(item);
        self.pending.store(true, Ordering::Release);
    }

    /// Hand several over together: whoever takes them gets all of them or none.
    pub fn push_many(&self, list: impl IntoIterator<Item = T>) {
        let mut items = self.items.lock();
        items.extend(list);
        if !items.is_empty() {
            self.pending.store(true, Ordering::Release);
        }
    }

    /// Whether anything waits to be taken.
    pub fn is_pending(&self) -> bool {
        self.pending.load(Ordering::Acquire)
    }

    /// Everything handed over since the last take, in order (empty without a lock when
    /// nothing came).
    pub fn take(&self) -> Vec<T> {
        if !self.is_pending() {
            return Vec::new();
        }
        let mut items = self.items.lock();
        self.pending.store(false, Ordering::Release);
        std::mem::take(&mut *items)
    }

    /// A look at what waits, without taking it (for a reader that only lists it).
    pub fn peek<R>(&self, f: impl FnOnce(&[T]) -> R) -> R {
        f(&self.items.lock())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taken_in_order_once() {
        let inbox = Inbox::new();
        assert!(inbox.take().is_empty());
        inbox.push(1);
        inbox.push_many([2, 3]);
        assert!(inbox.is_pending());
        assert_eq!(inbox.peek(|l| l.len()), 3);
        assert_eq!(inbox.take(), vec![1, 2, 3]);
        assert!(!inbox.is_pending());
        assert!(inbox.take().is_empty());
        inbox.push_many(Vec::<i32>::new());
        assert!(!inbox.is_pending());
    }
}
