//! The host's traffic table: every AI vehicle of the world, one row each, under an id of its
//! own. The simulation writes it after each of its steps; what shows or sounds the traffic
//! (the game's own pictures, the LAN clients) reads it.

use super::ids::{IdPool, TrafficId, MAX_ROWS};
use super::row::{Kind, Look, Meta, Row};
use std::collections::HashMap;

/// The most distinct looks (vehicle file and repaint) a table holds.
pub const MAX_LOOKS: usize = 1 << 12;

/// A row with what does not change as fast as its state.
#[derive(Debug, Clone)]
pub struct Slot {
    pub id: TrafficId,
    /// The simulation's own number of the vehicle.
    pub key: u64,
    pub kind: Kind,
    /// Index into the table's looks.
    pub look: u16,
    pub meta: Meta,
    /// Raised whenever `meta` changes.
    pub meta_rev: u32,
    pub row: Row,
    /// When the row appeared (s on the table's clock).
    pub born: f64,
    seen: u64,
}

#[derive(Debug, Default)]
pub struct TrafficTable {
    pool: IdPool,
    slots: Vec<Option<Slot>>,
    by_key: HashMap<u64, TrafficId>,
    looks: Vec<Look>,
    look_ids: HashMap<Look, u16>,
    now: f64,
    pass: u64,
    /// Bumped by every [`TrafficTable::begin`].
    pub generation: u64,
}

impl TrafficTable {
    /// Start a write at time `now` (s): the vehicles `upsert` from here on are the ones
    /// that exist; [`TrafficTable::finish`] takes the others away.
    pub fn begin(&mut self, now: f64) {
        self.now = now.max(self.now);
        self.pass += 1;
        self.generation += 1;
    }

    pub fn now(&self) -> f64 {
        self.now
    }

    /// Whether the vehicle `key` has a row (so that what is only needed for a new row is
    /// not made for an old one).
    pub fn has(&self, key: u64) -> bool {
        self.by_key.contains_key(&key)
    }

    pub fn id_of(&self, key: u64) -> Option<TrafficId> {
        self.by_key.get(&key).copied()
    }

    fn intern(&mut self, look: Look) -> Option<u16> {
        if let Some(i) = self.look_ids.get(&look) {
            return Some(*i);
        }
        if self.looks.len() >= MAX_LOOKS {
            return None;
        }
        let i = self.looks.len() as u16;
        self.look_ids.insert(look.clone(), i);
        self.looks.push(look);
        Some(i)
    }

    /// The row of vehicle `key`, made if it has none (`look` and `meta` are asked for then
    /// only). None when the table or its looks are full.
    pub fn upsert(&mut self, key: u64, kind: Kind, look: impl FnOnce() -> Look, meta: impl FnOnce() -> Meta, row: Row) -> Option<TrafficId> {
        if let Some(id) = self.by_key.get(&key).copied() {
            if let Some(s) = self.slots[id.slot()].as_mut() {
                s.row = row;
                s.seen = self.pass;
            }
            return Some(id);
        }
        let look = self.intern(look())?;
        let id = self.pool.alloc(self.now)?;
        if self.slots.len() <= id.slot() {
            self.slots.resize_with(id.slot() + 1, || None);
        }
        self.slots[id.slot()] = Some(Slot { id, key, kind, look, meta: meta(), meta_rev: 1, row, born: self.now, seen: self.pass });
        self.by_key.insert(key, id);
        Some(id)
    }

    /// Show a new line and destination of a vehicle; true when they differ from the last.
    pub fn set_meta(&mut self, id: TrafficId, meta: &Meta) -> bool {
        match self.slots.get_mut(id.slot()).and_then(|s| s.as_mut()).filter(|s| s.id == id) {
            Some(s) if s.meta != *meta => {
                s.meta = meta.clone();
                s.meta_rev += 1;
                true
            }
            _ => false,
        }
    }

    /// The end of a write: the vehicles not written since [`TrafficTable::begin`] are
    /// over. Their ids.
    pub fn finish(&mut self) -> Vec<TrafficId> {
        let mut gone = Vec::new();
        for s in self.slots.iter_mut() {
            if s.as_ref().is_some_and(|x| x.seen != self.pass) {
                if let Some(x) = s.take() {
                    self.by_key.remove(&x.key);
                    self.pool.release(x.id, self.now);
                    gone.push(x.id);
                }
            }
        }
        gone
    }

    /// Take a vehicle away at once (the simulation says it went).
    pub fn remove(&mut self, key: u64) -> Option<TrafficId> {
        let id = self.by_key.remove(&key)?;
        self.slots[id.slot()] = None;
        self.pool.release(id, self.now);
        Some(id)
    }

    pub fn clear(&mut self) {
        let keys: Vec<u64> = self.by_key.keys().copied().collect();
        for k in keys {
            self.remove(k);
        }
    }

    pub fn get(&self, id: TrafficId) -> Option<&Slot> {
        self.slots.get(id.slot()).and_then(|s| s.as_ref()).filter(|s| s.id == id)
    }

    pub fn look(&self, index: u16) -> Option<&Look> {
        self.looks.get(index as usize)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Slot> {
        self.slots.iter().flatten()
    }

    pub fn len(&self) -> usize {
        self.pool.live()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub const CAPACITY: usize = MAX_ROWS;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(n: &str) -> Look {
        Look { file: format!("Vehicles/{n}/{n}.bus"), scheme: Some(1), paint: "red".into() }
    }

    fn row(x: i32) -> Row {
        Row { x, ..Default::default() }
    }

    #[test]
    fn the_simulation_writes_and_rows_that_stay_unwritten_go() {
        let mut t = TrafficTable::default();
        t.begin(1.0);
        let a = t.upsert(10, Kind::Car, || look("golf"), Meta::default, row(1)).unwrap();
        let b = t.upsert(11, Kind::Bus, || look("man"), || Meta { line: "37".into(), destination: "#2".into(), ..Default::default() }, row(2)).unwrap();
        assert!(t.finish().is_empty());
        assert_eq!((t.len(), t.id_of(10), t.id_of(11)), (2, Some(a), Some(b)));
        // the same look is one entry
        t.begin(1.1);
        t.upsert(10, Kind::Car, || unreachable!("an old row asks for no look"), Meta::default, row(5)).unwrap();
        t.upsert(12, Kind::Car, || look("golf"), Meta::default, row(3)).unwrap();
        let gone = t.finish();
        assert_eq!(gone, vec![b]);
        assert_eq!(t.get(a).unwrap().row.x, 5);
        assert!(t.get(b).is_none() && !t.has(11));
        assert_eq!(t.look(t.get(t.id_of(12).unwrap()).unwrap().look), Some(&look("golf")));
        assert_eq!(t.iter().count(), 2);
    }

    #[test]
    fn a_new_vehicle_never_gets_the_id_of_one_that_just_went() {
        let mut t = TrafficTable::default();
        t.begin(0.0);
        let old = t.upsert(1, Kind::Car, || look("a"), Meta::default, row(0)).unwrap();
        t.begin(5.0);
        assert_eq!(t.finish(), vec![old]);
        t.begin(6.0);
        let new = t.upsert(2, Kind::Car, || look("a"), Meta::default, row(0)).unwrap();
        assert_ne!(new, old);
        assert_ne!(new.slot(), old.slot(), "the slot rests");
        assert!(t.get(old).is_none());
        t.begin(5.0 + super::super::ids::REUSE_DELAY);
        let again = t.upsert(3, Kind::Car, || look("a"), Meta::default, row(0)).unwrap();
        assert_eq!(again.slot(), old.slot());
        assert!(again.generation() != old.generation() && t.get(old).is_none());
    }

    #[test]
    fn a_new_line_or_destination_is_a_change_of_meta() {
        let mut t = TrafficTable::default();
        t.begin(0.0);
        let id = t.upsert(1, Kind::Bus, || look("m"), Meta::default, row(0)).unwrap();
        let m = Meta { line: "A".into(), destination: "#1".into(), ..Default::default() };
        assert!(t.set_meta(id, &m));
        assert!(!t.set_meta(id, &m));
        assert_eq!(t.get(id).unwrap().meta_rev, 2);
    }
}
