//! Which parking spaces of a tile get their parked car under the options' cap.
//!
//! Omsi.exe has no count of parked cars: as a tile comes in it fills each car park space
//! (an object of type 7) with a chance of `[AIMaxCountParked]` percent (FUN_0079c8b8,
//! 0x79c909: `Random(100) < AIMaxCountParked`), so the cars stand evenly over the map,
//! wherever the driver gets to. openOMSI's "at most N" kept the first N it placed: the
//! tiles of the start took them all, and a tile that came in while they stood there stayed
//! empty after the start's tiles had gone again (#4098, #2808). The cap is now turned into
//! such a chance per space - N over the spaces the loaded area has (the tiles in range
//! times the spaces a tile has had so far) - and the count stays a last bound.

use std::sync::atomic::Ordering::Relaxed;

/// The chance of a space getting its car when `max` cars may stand in an area of `tiles`
/// tiles, tiles having had `spaces` spaces in `seen` tiles (1 = every space).
pub fn space_chance(max: i64, tiles: usize, spaces: u64, seen: u64) -> f64 {
    if max <= 0 || tiles == 0 || seen == 0 || spaces == 0 {
        return 1.0;
    }
    let area = spaces as f64 / seen as f64 * tiles as f64;
    (max as f64 / area).min(1.0)
}

/// Does space `key` get its car at `chance` (the same draw every time the tile comes in)?
pub fn space_taken(key: i64, chance: f64) -> bool {
    if chance >= 1.0 {
        return true;
    }
    let h = (key as u64 ^ 0x5bd1_e995_7f4a_7c15)
        .wrapping_mul(0xd6e8_feb8_6659_fd93)
        .rotate_left(29)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15);
    ((h >> 11) as f64 / (1u64 << 53) as f64) < chance
}

impl crate::World {
    /// Tile streaming: the map's tiles within the load radius now.
    pub fn set_parked_area(&self, tiles: usize) {
        self.parked_area.store(tiles, Relaxed);
    }

    /// The chance of each of the `spaces` cars of a tile coming in (`first`: the first time
    /// it does, which counts for the spaces a tile has).
    pub(crate) fn parked_chance(&self, spaces: usize, first: bool) -> f64 {
        if first {
            self.parked_spaces.fetch_add(spaces as u64, Relaxed);
            self.parked_tiles_seen.fetch_add(1, Relaxed);
        }
        space_chance(
            self.parked_max,
            self.parked_area.load(Relaxed),
            self.parked_spaces.load(Relaxed),
            self.parked_tiles_seen.load(Relaxed),
        )
    }
}

/// The cars the cap held back: a tile that comes in while the cap is full still gets the
/// spaces its chance chose, their cars placed but hidden, and they come out as unloading
/// tiles give room (`Streamer::update` places before it unloads, and the tiles beyond the
/// load radius stay until the unload radius: without this a tile come in at the cap kept
/// its spaces empty for as long as it stayed).
#[derive(Default)]
pub struct ParkedHold {
    held: std::collections::HashMap<i64, Held>,
}

pub struct Held {
    pub tile: (i32, i32),
    /// Its collision box (an obstacle and a parked car's box once it is out).
    pub boxes: Vec<omsi_sim::collision::Obb>,
    /// The parked car, once its instances are on the GPU.
    pub object: Option<crate::ParkedObject>,
}

impl ParkedHold {
    pub fn hold(&mut self, key: i64, tile: (i32, i32)) {
        self.held.insert(
            key,
            Held {
                tile,
                boxes: Vec::new(),
                object: None,
            },
        );
    }

    pub fn is_held(&self, key: i64) -> bool {
        self.held.contains_key(&key)
    }

    pub fn add_box(&mut self, key: i64, b: omsi_sim::collision::Obb) {
        if let Some(h) = self.held.get_mut(&key) {
            h.boxes.push(b);
        }
    }

    /// Keeps the car of held space `key`; one not held is handed back (placed as usual).
    pub fn set_object(&mut self, key: i64, p: crate::ParkedObject) -> Option<crate::ParkedObject> {
        match self.held.get_mut(&key) {
            Some(h) => {
                h.object = Some(p);
                None
            }
            None => Some(p),
        }
    }

    pub fn drop_tile(&mut self, tile: (i32, i32)) {
        self.held.retain(|_, h| h.tile != tile);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.held.len()
    }

    /// As many held cars as `room` lets out (those placed on the GPU), the lowest keys first
    /// so that every run lets out the same ones.
    pub fn release(&mut self, room: usize) -> Vec<(i64, Held)> {
        let mut keys: Vec<i64> = self
            .held
            .iter()
            .filter(|(_, h)| h.object.is_some())
            .map(|(k, _)| *k)
            .collect();
        keys.sort_unstable();
        keys.truncate(room);
        keys.into_iter()
            .filter_map(|k| self.held.remove(&k).map(|h| (k, h)))
            .collect()
    }
}

/// How many more parked cars the cap allows with `live` standing (`max` 0 or less: no cap).
pub fn parked_room(max: i64, live: usize) -> usize {
    if max <= 0 {
        usize::MAX
    } else {
        (max as usize).saturating_sub(live)
    }
}

impl crate::World {
    /// Unloading gave room under the cap: the cars held back come out (shown, obstacles, a
    /// parked car the traffic keeps clear of).
    pub fn release_held_parked(
        &self,
        renderer: &omsi_render::Renderer,
        scene: &mut omsi_render::Scene,
    ) {
        let out = self.take_released_parked();
        for instances in &out {
            crate::props::show_parked_instances(renderer, scene, instances, true);
        }
    }

    /// The world's side of [`crate::World::release_held_parked`]: the instances to show.
    pub(crate) fn take_released_parked(&self) -> Vec<Vec<usize>> {
        let room = parked_room(self.parked_max, self.parked_live.load(Relaxed));
        if room == 0 {
            return Vec::new();
        }
        let released = self.parked_held.lock().release(room);
        if released.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        {
            let mut states = self.tile_state.lock();
            let mut parked = self.parked_objects.lock();
            for (key, h) in released {
                let (Some(st), Some(p)) = (states.get_mut(&h.tile), h.object) else {
                    continue;
                };
                st.obstacles.extend(h.boxes.iter().copied());
                st.parked_boxes.extend(h.boxes);
                st.parked_count += 1;
                self.parked_live.fetch_add(1, Relaxed);
                out.push(p.instances.clone());
                parked.insert(key, p);
            }
        }
        self.refresh_tile_lists();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(tile: (i32, i32)) -> crate::ParkedObject {
        crate::ParkedObject {
            tile,
            pos: Default::default(),
            heading: 0.0,
            sco: Default::default(),
            instances: vec![1],
        }
    }

    /// A tile comes in while the cap is full, then the old tiles go: its chosen spaces get
    /// their cars as the room comes, never more than the cap.
    #[test]
    fn a_tile_come_in_at_the_cap_gets_its_cars_as_old_tiles_go() {
        let max: i64 = 4;
        let mut live: usize = 4; // the old tiles hold the cap
        let mut hold = ParkedHold::default();
        for key in 10..13 {
            hold.hold(key, (1, 0));
            assert!(hold.set_object(key, object((1, 0))).is_none());
        }
        assert!(
            hold.set_object(99, object((1, 0))).is_some(),
            "a car not held is placed as usual"
        );
        assert!(hold.release(parked_room(max, live)).is_empty());
        // an old tile with two cars goes
        live -= 2;
        let out = hold.release(parked_room(max, live));
        assert_eq!(
            out.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            vec![10, 11]
        );
        live += out.len();
        assert_eq!(live, max as usize);
        assert_eq!(hold.len(), 1);
        // the rest when the next goes; the new tile leaving drops what it still holds
        live -= 1;
        let out = hold.release(parked_room(max, live));
        live += out.len();
        assert_eq!((live, hold.len()), (max as usize, 0));
        hold.hold(20, (2, 0));
        hold.drop_tile((2, 0));
        assert_eq!(hold.len(), 0);
        // without a cap nothing is held back
        assert_eq!(parked_room(0, 1000), usize::MAX);
    }

    #[test]
    fn the_cap_becomes_a_chance_per_space() {
        // every space where there is no cap, or nothing known yet
        assert_eq!(space_chance(0, 50, 1000, 10), 1.0);
        assert_eq!(space_chance(250, 0, 1000, 10), 1.0);
        assert_eq!(space_chance(250, 50, 0, 0), 1.0);
        // 100 spaces a tile, 50 tiles in range: 250 of 5000
        assert!((space_chance(250, 50, 1000, 10) - 0.05).abs() < 1e-9);
        // more room than spaces: all of them
        assert_eq!(space_chance(250, 2, 100, 10), 1.0);
    }

    /// The cars of a capped area stand over all its tiles (#4098): the tiles that come in
    /// after the first ones get their share instead of nothing.
    #[test]
    fn every_tile_gets_its_share_of_the_parked_cars() {
        let chance = space_chance(250, 50, 1000, 10);
        for tile in 0..50i64 {
            let n = (0..100)
                .filter(|s| space_taken(tile * 1000 + s, chance))
                .count();
            assert!(n <= 20, "tile {tile}: {n} of 100");
        }
        let all = (0..50i64 * 100)
            .filter(|&k| space_taken(k * 7 + 3, chance))
            .count();
        assert!((180..330).contains(&all), "{all} of a cap of 250");
        // the same spaces every time the tile comes in
        assert_eq!(space_taken(1234, 0.5), space_taken(1234, 0.5));
        assert!(space_taken(99, 1.0));
    }
}
