//! A bounded single-producer single-consumer ring: one thread puts values in, one other
//! takes them out, neither ever waits for the other and nothing is allocated after it is
//! made. The game hands the mixer its commands through one (the mixer's thread must not
//! wait for a lock the game holds: a block mixed late is a click), the mixer hands back
//! through another what is to be freed, and the internet radio's reader feeds its frames
//! through a third.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The two counters on cache lines of their own (each is written by one side only).
#[repr(align(128))]
struct Padded(AtomicUsize);

struct Ring<T> {
    slots: Box<[UnsafeCell<MaybeUninit<T>>]>,
    mask: usize,
    /// Values ever taken out (written by the consumer).
    head: Padded,
    /// Values ever put in (written by the producer).
    tail: Padded,
}

// (each slot is touched by one side at a time: the producer writes a slot before it
// publishes it with `tail`, the consumer reads it before it gives it back with `head`)
unsafe impl<T: Send> Sync for Ring<T> {}
unsafe impl<T: Send> Send for Ring<T> {}

impl<T> Drop for Ring<T> {
    fn drop(&mut self) {
        let (head, tail) = (*self.head.0.get_mut(), *self.tail.0.get_mut());
        for i in head..tail {
            unsafe { (*self.slots[i & self.mask].get()).assume_init_drop() };
        }
    }
}

/// The putting-in end.
pub struct Producer<T> {
    ring: Arc<Ring<T>>,
    /// The producer's own copy of `tail`, and the last `head` it saw.
    tail: usize,
    head: usize,
}

/// The taking-out end.
pub struct Consumer<T> {
    ring: Arc<Ring<T>>,
    head: usize,
    tail: usize,
}

/// A ring for at least `capacity` values (rounded up to a power of two).
pub fn channel<T>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    let cap = capacity.max(2).next_power_of_two();
    let ring = Arc::new(Ring {
        slots: (0..cap)
            .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
            .collect(),
        mask: cap - 1,
        head: Padded(AtomicUsize::new(0)),
        tail: Padded(AtomicUsize::new(0)),
    });
    (
        Producer {
            ring: ring.clone(),
            tail: 0,
            head: 0,
        },
        Consumer {
            ring,
            head: 0,
            tail: 0,
        },
    )
}

impl<T> Producer<T> {
    /// Put `v` in, or give it back when the ring is full.
    pub fn push(&mut self, v: T) -> Result<(), T> {
        let cap = self.ring.mask + 1;
        if self.tail - self.head >= cap {
            self.head = self.ring.head.0.load(Ordering::Acquire);
            if self.tail - self.head >= cap {
                return Err(v);
            }
        }
        unsafe { (*self.ring.slots[self.tail & self.ring.mask].get()).write(v) };
        self.tail += 1;
        self.ring.tail.0.store(self.tail, Ordering::Release);
        Ok(())
    }

    /// How many more values fit now.
    pub fn slots(&mut self) -> usize {
        self.head = self.ring.head.0.load(Ordering::Acquire);
        self.ring.mask + 1 - (self.tail - self.head)
    }

    /// How many values are in, not yet taken out.
    pub fn len(&mut self) -> usize {
        self.ring.mask + 1 - self.slots()
    }

    pub fn is_empty(&mut self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        self.ring.mask + 1
    }
}

impl<T> Consumer<T> {
    /// The oldest value, taken out.
    pub fn pop(&mut self) -> Option<T> {
        if self.head == self.tail {
            self.tail = self.ring.tail.0.load(Ordering::Acquire);
            if self.head == self.tail {
                return None;
            }
        }
        let v = unsafe { (*self.ring.slots[self.head & self.ring.mask].get()).assume_init_read() };
        self.head += 1;
        self.ring.head.0.store(self.head, Ordering::Release);
        Some(v)
    }

    /// The oldest value, left in.
    pub fn peek(&mut self) -> Option<&T> {
        self.get(0)
    }

    /// The `i`-th oldest value, left in.
    pub fn get(&mut self, i: usize) -> Option<&T> {
        if self.tail - self.head <= i {
            self.tail = self.ring.tail.0.load(Ordering::Acquire);
            if self.tail - self.head <= i {
                return None;
            }
        }
        Some(unsafe {
            (*self.ring.slots[(self.head + i) & self.ring.mask].get()).assume_init_ref()
        })
    }

    /// How many values wait.
    pub fn len(&mut self) -> usize {
        self.tail = self.ring.tail.0.load(Ordering::Acquire);
        self.tail - self.head
    }

    pub fn is_empty(&mut self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_come_out_in_order_and_a_full_ring_says_so() {
        let (mut p, mut c) = channel::<u32>(3);
        assert_eq!(p.capacity(), 4);
        for i in 0..4 {
            p.push(i).unwrap();
        }
        assert_eq!(p.push(9), Err(9));
        assert_eq!((p.len(), c.len()), (4, 4));
        assert_eq!(c.peek(), Some(&0));
        assert_eq!(c.get(3), Some(&3));
        assert_eq!(c.get(4), None);
        assert_eq!(c.pop(), Some(0));
        p.push(4).unwrap();
        let got: Vec<u32> = std::iter::from_fn(|| c.pop()).collect();
        assert_eq!(got, [1, 2, 3, 4]);
        assert!(c.is_empty() && p.slots() == 4);
    }

    /// Values left in when both ends are gone are dropped (an `Arc` counts it).
    #[test]
    fn values_left_in_are_dropped() {
        let a = Arc::new(());
        let (mut p, c) = channel(8);
        p.push(a.clone()).unwrap();
        p.push(a.clone()).unwrap();
        drop((p, c));
        assert_eq!(Arc::strong_count(&a), 1);
    }

    #[test]
    fn two_threads_pass_a_million_values() {
        let (mut p, mut c) = channel::<u64>(64);
        let t = std::thread::spawn(move || {
            for i in 0..1_000_000u64 {
                let mut v = i;
                while let Err(back) = p.push(v) {
                    v = back;
                    std::hint::spin_loop();
                }
            }
        });
        let mut want = 0u64;
        while want < 1_000_000 {
            if let Some(v) = c.pop() {
                assert_eq!(v, want);
                want += 1;
            }
        }
        t.join().unwrap();
    }
}
