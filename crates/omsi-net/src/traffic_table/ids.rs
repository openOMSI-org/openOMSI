//! Row ids of the traffic table: a slot (which row) and a generation (which life of that row).
//!
//! A slot freed by a vehicle that went is kept out of use for [`REUSE_DELAY`] seconds, and
//! comes back with the next generation: a datagram that was delayed on the way, or a client
//! that has not heard of the vehicle's end yet, can never mistake the next vehicle in that
//! slot for the old one.

use std::collections::VecDeque;

pub const SLOT_BITS: u32 = 12;
pub const GEN_BITS: u32 = 8;
/// Bits of an id on the wire.
pub const ID_BITS: u32 = SLOT_BITS + GEN_BITS;
/// The most rows a table holds.
pub const MAX_ROWS: usize = 1 << SLOT_BITS;
/// Seconds a freed slot rests before it is given out again.
pub const REUSE_DELAY: f64 = 30.0;

/// One vehicle of the table, for as long as it lives: slot in the low bits, generation
/// (1 to 255, never 0) above.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TrafficId(u32);

impl TrafficId {
    pub fn new(slot: u16, generation: u8) -> TrafficId {
        TrafficId((generation as u32) << SLOT_BITS | (slot as u32 & (MAX_ROWS as u32 - 1)))
    }

    /// An id read off the wire: its generation is not 0 and it fits `ID_BITS`.
    pub fn from_raw(raw: u32) -> Option<TrafficId> {
        (raw < 1 << ID_BITS && raw >> SLOT_BITS != 0).then_some(TrafficId(raw))
    }

    pub fn raw(self) -> u32 {
        self.0
    }

    pub fn slot(self) -> usize {
        (self.0 & (MAX_ROWS as u32 - 1)) as usize
    }

    pub fn generation(self) -> u8 {
        (self.0 >> SLOT_BITS) as u8
    }
}

impl std::fmt::Display for TrafficId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.slot(), self.generation())
    }
}

/// Gives out ids and takes them back (see the module's comment).
#[derive(Debug, Default)]
pub struct IdPool {
    /// The generation each slot is on (0: never used).
    generations: Vec<u8>,
    live: Vec<bool>,
    /// Freed slots and when, oldest first.
    resting: VecDeque<(u16, f64)>,
    live_count: usize,
}

fn next_generation(g: u8) -> u8 {
    if g == u8::MAX { 1 } else { g + 1 }
}

impl IdPool {
    /// A new id at time `now` (s), or None when every slot is taken or resting.
    pub fn alloc(&mut self, now: f64) -> Option<TrafficId> {
        let slot = match self.resting.front() {
            Some(&(slot, since)) if now - since >= REUSE_DELAY => {
                self.resting.pop_front();
                slot as usize
            }
            _ if self.generations.len() < MAX_ROWS => {
                self.generations.push(0);
                self.live.push(false);
                self.generations.len() - 1
            }
            _ => return None,
        };
        self.generations[slot] = next_generation(self.generations[slot]);
        self.live[slot] = true;
        self.live_count += 1;
        Some(TrafficId::new(slot as u16, self.generations[slot]))
    }

    /// `id` is over at time `now`; false when it was not alive.
    pub fn release(&mut self, id: TrafficId, now: f64) -> bool {
        if !self.is_live(id) {
            return false;
        }
        self.live[id.slot()] = false;
        self.live_count -= 1;
        self.resting.push_back((id.slot() as u16, now));
        true
    }

    pub fn is_live(&self, id: TrafficId) -> bool {
        self.live.get(id.slot()).copied().unwrap_or(false)
            && self.generations[id.slot()] == id.generation()
    }

    pub fn live(&self) -> usize {
        self.live_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_packs_slot_and_generation() {
        let id = TrafficId::new(4095, 255);
        assert_eq!((id.slot(), id.generation()), (4095, 255));
        assert_eq!(TrafficId::from_raw(id.raw()), Some(id));
        // generation 0 and anything beyond 20 bits are not ids
        assert_eq!(TrafficId::from_raw(7), None);
        assert_eq!(TrafficId::from_raw(1 << ID_BITS), None);
    }

    #[test]
    fn a_slot_rests_before_it_comes_back_with_the_next_generation() {
        let mut p = IdPool::default();
        let a = p.alloc(0.0).unwrap();
        let b = p.alloc(0.0).unwrap();
        assert_ne!(a.slot(), b.slot());
        assert!(p.release(a, 10.0));
        assert!(!p.release(a, 11.0), "released twice");
        assert!(!p.is_live(a));
        // too soon: a fresh slot, not a's
        let c = p.alloc(10.5).unwrap();
        assert_ne!(c.slot(), a.slot());
        // after the delay: a's slot, a newer generation, which is not a
        let d = p.alloc(10.0 + REUSE_DELAY).unwrap();
        assert_eq!(d.slot(), a.slot());
        assert_eq!(d.generation(), a.generation() + 1);
        assert_ne!(d, a);
        assert!(p.is_live(d) && !p.is_live(a));
        assert_eq!(p.live(), 3);
    }

    #[test]
    fn generations_wrap_past_zero_and_the_table_can_be_full() {
        let mut p = IdPool::default();
        let mut t = 0.0;
        let mut id = p.alloc(t).unwrap();
        for _ in 0..300 {
            p.release(id, t);
            t += REUSE_DELAY;
            id = p.alloc(t).unwrap();
            assert_ne!(id.generation(), 0);
        }
        let mut full = IdPool::default();
        for _ in 0..MAX_ROWS {
            full.alloc(0.0).unwrap();
        }
        assert!(full.alloc(0.0).is_none());
    }
}
