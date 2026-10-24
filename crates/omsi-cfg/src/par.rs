//! Parallel loops for the frame's own thread.
//!
//! `par_iter` from the frame's thread hands the whole loop to the worker pool and then
//! waits on a latch until the last job is done: when the workers are busy elsewhere (tile
//! loading, texture compression, another process) or the system has not scheduled them yet,
//! the frame waits for work it could have done itself. Standing still on a city map that
//! was most of the frame's 20-70 ms hitches. [`frame_each`] runs the loop on the calling
//! thread and lets workers join in: every item goes to whoever is free first, the caller
//! too, and the caller only ever waits for items a worker has already started.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// The items' address, shared with the helpers (each index is claimed by one thread only).
struct SendPtr<T>(*mut T);
unsafe impl<T: Send> Send for SendPtr<T> {}
unsafe impl<T: Send> Sync for SendPtr<T> {}
impl<T> SendPtr<T> {
    /// (a method, so a closure captures the whole wrapper, not its bare pointer)
    fn at(&self, i: usize) -> *mut T {
        self.0.wrapping_add(i)
    }
}

/// What the caller and its helpers share. The helpers outlive the call (a worker may start
/// after the loop is done): they only touch this, never the items, once `next` has run out.
struct Shared {
    next: AtomicUsize,
    done: AtomicUsize,
    len: usize,
    /// The loop body (it holds the items), valid while the call runs. A helper reads it
    /// only after claiming an index below `len`, and the call does not return before every
    /// claimed index is done, so it is never read after the call.
    body: usize,
    /// The calling thread, woken by whoever finishes the last item (it sleeps while it waits
    /// for items a helper has started: spinning there kept a core busy for nothing when the
    /// helper had been descheduled).
    caller: std::thread::Thread,
}

/// Run `f` on every item, on the calling thread and on up to `helpers` workers of the
/// global pool that join in when they are free. Order of the calls is unspecified; each
/// item is visited exactly once. With `helpers == 0` (or one item) it is a plain loop.
///
/// Give helpers only for loops worth it: about one helper per half millisecond of work
/// (waking a worker costs a few microseconds, and a tiny loop is faster alone).
pub fn frame_each<T: Send, F: Fn(&mut T) + Sync>(items: &mut [T], helpers: usize, f: F) {
    let len = items.len();
    if helpers == 0 || len < 2 {
        items.iter_mut().for_each(f);
        return;
    }
    // The body behind a pointer of a fixed type, so the helpers need not know `F`.
    let base = SendPtr(items.as_mut_ptr());
    let body: &(dyn Fn(usize) + Sync) = &|i: usize| {
        // SAFETY: `i < len` was claimed exactly once (fetch_add), so no other thread holds
        // this item; `items` lives until the call returns, which waits for claimed items.
        let item = unsafe { &mut *base.at(i) };
        f(item);
    };
    let shared = Arc::new(Shared {
        next: AtomicUsize::new(0),
        done: AtomicUsize::new(0),
        len,
        body: &body as *const &(dyn Fn(usize) + Sync) as usize,
        caller: std::thread::current(),
    });
    fn work(s: &Shared) {
        loop {
            let i = s.next.fetch_add(1, Ordering::AcqRel);
            if i >= s.len {
                return;
            }
            // SAFETY: see `Shared::body`: a claimed index keeps the caller (and so the
            // body it points to) waiting until `done` counts it.
            let body = unsafe { &*(s.body as *const &(dyn Fn(usize) + Sync)) };
            body(i);
            if s.done.fetch_add(1, Ordering::AcqRel) + 1 == s.len {
                s.caller.unpark();
            }
        }
    }
    for _ in 0..helpers.min(len - 1) {
        let s = shared.clone();
        rayon::spawn(move || work(&s));
    }
    work(&shared);
    // Only items a helper is working on right now are left: wait for those, briefly.
    // (a short spin first: most waits are a few microseconds; then asleep, woken by the last
    // item, with a timeout as a net)
    let spin_until = std::time::Instant::now() + std::time::Duration::from_micros(40);
    while shared.done.load(Ordering::Acquire) < len {
        if std::time::Instant::now() < spin_until {
            std::hint::spin_loop();
        } else {
            std::thread::park_timeout(std::time::Duration::from_micros(300));
        }
    }
}

/// How many helpers a loop of `total` seconds of work deserves: one per `per_helper`
/// seconds, at most the pool's threads.
pub fn helpers_for(total: f64, per_helper: f64) -> usize {
    if !(total > 0.0) || !(per_helper > 0.0) {
        return 0;
    }
    ((total / per_helper) as usize).min(rayon::current_num_threads())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_once_with_and_without_helpers() {
        for helpers in [0, 1, 3, 16] {
            let mut v: Vec<u64> = (0..1000).collect();
            frame_each(&mut v, helpers, |x| *x = *x * 2 + 1);
            assert!(v.iter().enumerate().all(|(i, &x)| x == i as u64 * 2 + 1), "helpers {helpers}");
        }
        let mut empty: Vec<u8> = Vec::new();
        frame_each(&mut empty, 4, |_| unreachable!());
    }

    #[test]
    fn late_helpers_find_nothing_left() {
        // (the pool busy with long jobs: the caller does it all and returns; the helpers
        // that start later see the loop finished)
        let gate = Arc::new(std::sync::Barrier::new(rayon::current_num_threads() + 1));
        for _ in 0..rayon::current_num_threads() {
            let g = gate.clone();
            rayon::spawn(move || {
                g.wait();
            });
        }
        let mut v = vec![0u32; 64];
        frame_each(&mut v, 4, |x| *x += 1);
        assert!(v.iter().all(|&x| x == 1));
        gate.wait();
    }

    #[test]
    fn helper_counts() {
        assert_eq!(helpers_for(0.0, 0.0005), 0);
        assert_eq!(helpers_for(0.0004, 0.0005), 0);
        assert!(helpers_for(1.0, 0.0005) <= rayon::current_num_threads());
    }
}
