//! The jitter buffer: frames come off the network late, early, twice or not at all; the
//! player takes one every 20 ms. The buffer holds back a short while at the start of a
//! transmission (`TARGET` frames), puts frames in order, drops the ones that came too late,
//! asks for a repair of a lost one (from the error correction the next frame carries, else
//! concealment), and keeps the delay from growing (`MAX_DEPTH`).

use std::collections::BTreeMap;

/// Frames gathered before a transmission starts to play (40 ms).
pub const TARGET: usize = 2;
/// Frames waiting, at most: more means the sender is ahead of the clock (a burst after a
/// stall), and the oldest are dropped to keep the delay short.
pub const MAX_DEPTH: usize = 8;
/// Lost frames in a row that are made up before the buffer gives the transmission up.
pub const MAX_CONCEAL: u32 = 5;
/// A frame number this far from the expected one is a new transmission, not a late or
/// early frame.
const RESYNC: i64 = 200;

#[derive(Debug, PartialEq, Eq)]
pub enum Playout {
    /// Nothing to play yet (filling up, or nothing came).
    Wait,
    /// The next frame.
    Frame(Vec<u8>),
    /// The next frame was lost, and this one (the one after it) carries its repair.
    Recover(Vec<u8>),
    /// The next frame was lost and nothing is there to repair it: make something up.
    Conceal,
    /// The sender is gone (too many lost in a row): the transmission is over.
    Dead,
}

#[derive(Default)]
pub struct Jitter {
    queue: BTreeMap<i64, Vec<u8>>,
    /// The frame number (extended past 16 bits) played next; None while filling up.
    next: Option<i64>,
    /// The highest number seen, as the base to unwrap the 16-bit numbers against.
    top: Option<i64>,
    lost_run: u32,
    /// Frames that came after their time, and frames dropped to keep the delay down.
    pub late: u32,
    pub dropped: u32,
}

impl Jitter {
    pub fn new() -> Jitter {
        Jitter::default()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    fn unwrap(&self, seq: u16) -> i64 {
        match self.top {
            None => seq as i64,
            Some(top) => top + (seq.wrapping_sub(top as u16) as i16) as i64,
        }
    }

    pub fn push(&mut self, seq: u16, payload: Vec<u8>) {
        let ext = self.unwrap(seq);
        let expected = self.next.or(self.top);
        if let Some(e) = expected {
            if (ext - e).abs() > RESYNC {
                // a new transmission (or the sender restarted its numbers)
                self.queue.clear();
                self.next = None;
                self.top = None;
                self.lost_run = 0;
                return self.push(seq, payload);
            }
        }
        if self.next.is_some_and(|n| ext < n) {
            self.late += 1;
            return;
        }
        self.top = Some(self.top.map_or(ext, |t| t.max(ext)));
        self.queue.insert(ext, payload);
        while self.queue.len() > MAX_DEPTH {
            let first = *self.queue.keys().next().unwrap();
            self.queue.remove(&first);
            self.dropped += 1;
            if self.next.is_some_and(|n| n <= first) {
                self.next = Some(first + 1);
            }
        }
    }

    /// The frame to play now (called every 20 ms while the speaker is heard).
    pub fn pop(&mut self) -> Playout {
        let next = match self.next {
            Some(n) => n,
            None => {
                if self.queue.len() < TARGET {
                    return Playout::Wait;
                }
                let first = *self.queue.keys().next().unwrap();
                self.next = Some(first);
                first
            }
        };
        if let Some(p) = self.queue.remove(&next) {
            self.next = Some(next + 1);
            self.lost_run = 0;
            return Playout::Frame(p);
        }
        // the frame is missing
        if self.queue.is_empty() {
            // nothing later either: the sender stopped, or the network did
            self.lost_run += 1;
            if self.lost_run > MAX_CONCEAL {
                *self = Jitter {
                    late: self.late,
                    dropped: self.dropped,
                    ..Jitter::default()
                };
                return Playout::Dead;
            }
            self.next = Some(next + 1);
            return Playout::Conceal;
        }
        self.lost_run = 0;
        self.next = Some(next + 1);
        match self.queue.get(&(next + 1)) {
            Some(after) => Playout::Recover(after.clone()),
            None => Playout::Conceal,
        }
    }

    /// Start over (the speaker stopped).
    pub fn reset(&mut self) {
        *self = Jitter {
            late: self.late,
            dropped: self.dropped,
            ..Jitter::default()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(n: u8) -> Vec<u8> {
        vec![n]
    }

    #[test]
    fn frames_come_out_in_order_after_the_first_are_gathered() {
        let mut j = Jitter::new();
        j.push(10, p(10));
        assert_eq!(j.pop(), Playout::Wait);
        // out of order, and a duplicate
        j.push(12, p(12));
        j.push(11, p(11));
        j.push(11, p(11));
        assert_eq!(j.pop(), Playout::Frame(p(10)));
        assert_eq!(j.pop(), Playout::Frame(p(11)));
        assert_eq!(j.pop(), Playout::Frame(p(12)));
        // a frame that came after its time is dropped
        j.push(11, p(11));
        assert_eq!(j.late, 1);
        assert!(j.is_empty());
    }

    #[test]
    fn numbers_wrap_around() {
        let mut j = Jitter::new();
        for s in [65534u16, 65535, 0, 1] {
            j.push(s, p(s as u8));
        }
        let got: Vec<_> = (0..4).map(|_| j.pop()).collect();
        assert_eq!(
            got,
            vec![
                Playout::Frame(p(254)),
                Playout::Frame(p(255)),
                Playout::Frame(p(0)),
                Playout::Frame(p(1))
            ]
        );
    }

    #[test]
    fn a_lost_frame_is_repaired_from_the_next_or_concealed() {
        let mut j = Jitter::new();
        for s in [1u16, 2, 4, 5] {
            j.push(s, p(s as u8));
        }
        assert_eq!(j.pop(), Playout::Frame(p(1)));
        assert_eq!(j.pop(), Playout::Frame(p(2)));
        // 3 is lost: 4 carries its repair, and 4 itself is still played after
        assert_eq!(j.pop(), Playout::Recover(p(4)));
        assert_eq!(j.pop(), Playout::Frame(p(4)));
        assert_eq!(j.pop(), Playout::Frame(p(5)));
        // two lost in a row: the first has no repair at hand
        let mut j = Jitter::new();
        for s in [1u16, 2, 5, 6] {
            j.push(s, p(s as u8));
        }
        j.pop();
        j.pop();
        assert_eq!(j.pop(), Playout::Conceal);
        assert_eq!(j.pop(), Playout::Recover(p(5)));
        assert_eq!(j.pop(), Playout::Frame(p(5)));
    }

    #[test]
    fn a_silent_sender_is_given_up_after_a_few_frames() {
        let mut j = Jitter::new();
        j.push(1, p(1));
        j.push(2, p(2));
        j.pop();
        j.pop();
        for _ in 0..MAX_CONCEAL {
            assert_eq!(j.pop(), Playout::Conceal);
        }
        assert_eq!(j.pop(), Playout::Dead);
        // and a new transmission starts afresh, whatever its numbers
        j.push(40_000, p(1));
        j.push(40_001, p(2));
        assert_eq!(j.pop(), Playout::Frame(p(1)));
    }

    #[test]
    fn the_delay_does_not_grow() {
        let mut j = Jitter::new();
        for s in 0..40u16 {
            j.push(s, p(s as u8));
        }
        assert_eq!(j.len(), MAX_DEPTH);
        assert_eq!(j.dropped, 32);
        // what is played is the newest frames, in order
        assert_eq!(j.pop(), Playout::Frame(p(32)));
        assert_eq!(j.pop(), Playout::Frame(p(33)));
    }

    #[test]
    fn a_jump_in_the_numbers_is_a_new_transmission() {
        let mut j = Jitter::new();
        j.push(10, p(1));
        j.push(11, p(2));
        j.pop();
        j.push(30_000, p(9));
        j.push(30_001, p(10));
        assert_eq!(j.pop(), Playout::Frame(p(9)));
    }
}
