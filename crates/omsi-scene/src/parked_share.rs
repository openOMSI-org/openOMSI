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

#[cfg(test)]
mod tests {
    use super::*;

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
