//! Cables the plugins lay in the world (`omsi.cable`): overhead wires, ropes, lines - each a
//! row of points drawn as a thin round tube, lit and shaded as the scene round it. A plugin
//! sets a cable again whenever it moves (wires swaying, a rope going slack); the game draws
//! what is there each frame.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

/// What the points are measured from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CableFrame {
    /// The map's own coordinates (x east, y north, z up, metres).
    World,
    /// The player's bus: its model's frame (x right, y forward, z up), so that a rope tied
    /// to it moves with the bus as drawn.
    Bus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cable {
    pub points: Vec<[f64; 3]>,
    /// The tube's radius (m).
    pub radius: f32,
    /// Linear RGB and alpha, 0 to 1.
    pub color: [f32; 4],
    pub frame: CableFrame,
}

/// At most this many points a cable, and cables a plugin.
pub const MAX_POINTS: usize = 4096;
pub const MAX_CABLES: usize = 4096;
/// At most this many points in all of a plugin's cables together: each point is a ring of the
/// tube's vertices and its faces, so this bounds a plugin's geometry (about 22 MB) where the
/// two limits above alone would let it reach gigabytes.
pub const MAX_PLUGIN_POINTS: usize = 65_536;

/// Every plugin's cables, by owner and the plugin's own id for each.
#[derive(Debug, Default)]
pub struct Cables {
    by: BTreeMap<(u64, String), Cable>,
    /// Goes up with every change, so the game can tell when nothing moved.
    pub generation: u64,
    /// The cables laid, moved or taken away since the game last looked (`changes`): a game
    /// drawing a few thousand of them redraws only those.
    changed: BTreeSet<(u64, String)>,
    /// How many cables each plugin has, and their points in all.
    per_owner: HashMap<u64, (usize, usize)>,
}

pub type SharedCables = Rc<RefCell<Cables>>;

impl Cables {
    /// Lays cable `id` of `owner`, or changes it; false (and nothing changes) when the plugin
    /// would have more than `MAX_CABLES` cables or `MAX_PLUGIN_POINTS` points in all.
    pub fn set(&mut self, owner: u64, id: &str, cable: Cable) -> bool {
        let key = (owner, id.to_string());
        let (count, points) = self.per_owner.get(&owner).copied().unwrap_or_default();
        if let Some(old) = self.by.get_mut(&key) {
            if *old != cable {
                let points = points - old.points.len() + cable.points.len();
                if points > MAX_PLUGIN_POINTS {
                    return false;
                }
                self.per_owner.insert(owner, (count, points));
                *old = cable;
                self.generation += 1;
                self.changed.insert(key);
            }
            return true;
        }
        if count >= MAX_CABLES || points + cable.points.len() > MAX_PLUGIN_POINTS {
            return false;
        }
        self.per_owner.insert(owner, (count + 1, points + cable.points.len()));
        self.changed.insert(key.clone());
        self.by.insert(key, cable);
        self.generation += 1;
        true
    }

    pub fn remove(&mut self, owner: u64, id: &str) -> bool {
        let key = (owner, id.to_string());
        let Some(old) = self.by.remove(&key) else { return false };
        self.generation += 1;
        if let Some((count, points)) = self.per_owner.get_mut(&owner) {
            *count = count.saturating_sub(1);
            *points = points.saturating_sub(old.points.len());
        }
        self.changed.insert(key);
        true
    }

    /// Every cable of `owner` goes (it stopped, or cleared them).
    pub fn remove_owner(&mut self, owner: u64) -> usize {
        let keys: Vec<(u64, String)> = self.by.keys().filter(|(o, _)| *o == owner).cloned().collect();
        for k in &keys {
            self.by.remove(k);
        }
        let gone = keys.len();
        if gone > 0 {
            self.generation += 1;
            self.per_owner.remove(&owner);
            self.changed.extend(keys);
        }
        gone
    }

    /// The cables changed since the last call, each with what it is now (`None`: taken away);
    /// with `all`, every cable there is (for a game that lost what it drew).
    pub fn changes(&mut self, all: bool) -> Vec<((u64, String), Option<Cable>)> {
        let changed = std::mem::take(&mut self.changed);
        if all {
            return self.by.iter().map(|(k, c)| (k.clone(), Some(c.clone()))).collect();
        }
        changed.into_iter().map(|k| {
            let c = self.by.get(&k).cloned();
            (k, c)
        }).collect()
    }

    /// The cables, each with a key unique among all plugins'.
    pub fn iter(&self) -> impl Iterator<Item = (&(u64, String), &Cable)> {
        self.by.iter()
    }

    pub fn len(&self) -> usize {
        self.by.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cable(x: f64) -> Cable {
        Cable { points: vec![[0.0, 0.0, 6.0], [x, 0.0, 6.0]], radius: 0.006, color: [0.1, 0.1, 0.1, 1.0], frame: CableFrame::World }
    }

    fn cable_of(points: usize) -> Cable {
        Cable { points: vec![[0.0, 0.0, 6.0]; points], ..cable(1.0) }
    }

    #[test]
    fn a_plugin_cannot_lay_more_points_than_its_budget_however_it_splits_them() {
        let mut c = Cables::default();
        // sixteen cables of the most points each: every one valid, together the whole budget
        let each = MAX_POINTS;
        for i in 0..MAX_PLUGIN_POINTS / each {
            assert!(c.set(1, &format!("w{i}"), cable_of(each)));
        }
        assert!(!c.set(1, "one more", cable_of(2)), "over the budget");
        assert!(!c.set(1, "w0", cable_of(each + 1)), "grown over the budget");
        assert_eq!(c.by[&(1, "w0".into())].points.len(), each, "a refused change keeps the cable as it was");
        // another plugin has a budget of its own
        assert!(c.set(2, "w", cable_of(each)));
        // what is taken away frees its points, and a cable made shorter makes room
        assert!(c.remove(1, "w0"));
        assert!(c.set(1, "one more", cable_of(2)));
        assert!(c.set(1, "w1", cable_of(10)));
        let free = (each - 2) + (each - 10);
        assert!(c.set(1, "two more", cable_of(free)));
        assert!(!c.set(1, "three more", cable_of(2)));
        // cleared, the budget is whole again
        c.remove_owner(1);
        assert!(c.set(1, "w", cable_of(each)));
    }

    #[test]
    fn a_cable_set_again_unchanged_changes_nothing() {
        let mut c = Cables::default();
        assert!(c.set(1, "a", cable(10.0)));
        let g = c.generation;
        assert!(c.set(1, "a", cable(10.0)));
        assert_eq!(c.generation, g);
        assert!(c.set(1, "a", cable(11.0)));
        assert!(c.generation > g);
    }

    #[test]
    fn each_plugin_has_its_own_ids_and_takes_only_its_own_away() {
        let mut c = Cables::default();
        c.set(1, "w", cable(1.0));
        c.set(2, "w", cable(2.0));
        assert_eq!(c.len(), 2);
        assert_eq!(c.remove_owner(1), 1);
        assert_eq!(c.len(), 1);
        assert!(!c.remove(1, "w"));
        assert!(c.remove(2, "w"));
        assert!(c.is_empty());
    }

    #[test]
    fn the_game_gets_only_what_changed_since_it_last_asked() {
        let mut c = Cables::default();
        c.set(1, "a", cable(1.0));
        c.set(1, "b", cable(2.0));
        assert_eq!(c.changes(false).len(), 2);
        assert!(c.changes(false).is_empty());
        c.set(1, "a", cable(1.0));
        assert!(c.changes(false).is_empty(), "set again unchanged");
        c.set(1, "a", cable(3.0));
        c.remove(1, "b");
        let ch = c.changes(false);
        assert_eq!(ch, vec![((1, "a".into()), Some(cable(3.0))), ((1, "b".into()), None)]);
        // a game that lost its scene asks for all of them
        assert_eq!(c.changes(true), vec![((1, "a".into()), Some(cable(3.0)))]);
    }
}
