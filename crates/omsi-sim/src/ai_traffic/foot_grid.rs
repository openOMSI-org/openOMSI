//! The AI vehicles' footprints of one tick, with a coarse grid to find the ones near a
//! point. Every car sweeps its way ahead against the bodies near it (`body_in_way`), and it
//! looked through every footprint for them: with many cars, the number of cars squared each
//! tick. The grid hands out the footprints of the cells around the point - in the order of
//! the list, so what is found first is the same - and the callers keep their own exact
//! distance test. With few footprints the list is simply walked.

use super::*;

/// Edge of a grid cell (m).
const CELL: f64 = 32.0;
/// From how many footprints on the grid is made.
const GRID_FROM: usize = 64;
/// A footprint reaching over more cells than this each way (a broken one) is in every
/// answer instead.
const MAX_SPAN: i64 = 16;

pub struct Feet {
    list: Vec<Footprint>,
    grid: Option<Grid>,
}

#[derive(Default)]
struct Grid {
    cells: HashMap<(i64, i64), Vec<u32>>,
    /// Footprints too big or not finite for the cells, given to every query.
    always: Vec<u32>,
}

fn cell(v: f64) -> i64 {
    (v / CELL).floor() as i64
}

impl Feet {
    pub fn new(list: Vec<Footprint>) -> Feet {
        let grid = (list.len() >= GRID_FROM).then(|| {
            let mut g = Grid::default();
            for (k, f) in list.iter().enumerate() {
                // (a footprint is reached within its own half length and half width of its
                // centre, see `body_in_way`: it goes into every cell that square touches)
                let r = f.half_len + f.half_w;
                let (x0, x1, y0, y1) = (
                    cell(f.center.x - r),
                    cell(f.center.x + r),
                    cell(f.center.y - r),
                    cell(f.center.y + r),
                );
                if !(f.center.is_finite() && r.is_finite())
                    || x1 - x0 > MAX_SPAN
                    || y1 - y0 > MAX_SPAN
                {
                    g.always.push(k as u32);
                    continue;
                }
                for x in x0..=x1 {
                    for y in y0..=y1 {
                        g.cells.entry((x, y)).or_default().push(k as u32);
                    }
                }
            }
            g
        });
        Feet { list, grid }
    }

    /// The footprints whose centre may lie within `r` plus their own half length and half
    /// width of `pos` (and perhaps some more), in the order of the list.
    pub fn near(&self, pos: DVec2, r: f64) -> impl Iterator<Item = &Footprint> + '_ {
        let picked = self
            .grid
            .as_ref()
            .filter(|_| pos.is_finite() && r.is_finite() && r < CELL * MAX_SPAN as f64)
            .map(|g| {
                let mut ks: Vec<u32> = g.always.clone();
                for x in cell(pos.x - r)..=cell(pos.x + r) {
                    for y in cell(pos.y - r)..=cell(pos.y + r) {
                        if let Some(v) = g.cells.get(&(x, y)) {
                            ks.extend_from_slice(v);
                        }
                    }
                }
                ks.sort_unstable();
                ks.dedup();
                ks
            });
        let (all, some) = match picked {
            Some(ks) => (
                None,
                Some(ks.into_iter().map(move |k| &self.list[k as usize])),
            ),
            None => (Some(self.list.iter()), None),
        };
        all.into_iter().flatten().chain(some.into_iter().flatten())
    }
}

impl std::ops::Deref for Feet {
    type Target = [Footprint];

    fn deref(&self) -> &[Footprint] {
        &self.list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Over many footprints, the grid finds every one the exact test keeps, in the same
    /// order as walking the list.
    #[test]
    fn the_grid_finds_what_the_list_does() {
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut list = Vec::new();
        for car in 0..400 {
            let h = rnd() * std::f64::consts::TAU;
            list.push(Footprint {
                car,
                center: DVec2::new(rnd() * 900.0 - 450.0, rnd() * 900.0 - 450.0),
                fwd: DVec2::new(h.sin(), h.cos()),
                right: DVec2::new(h.cos(), -h.sin()),
                half_len: 1.5 + rnd() * 10.0,
                half_w: 0.8 + rnd() * 0.5,
                speed: 0.0,
                z: 0.0,
            });
        }
        // (one on a cell's edge, one huge, one broken)
        list[0].center = DVec2::new(CELL, -CELL);
        list[1].half_len = 5000.0;
        list[2].center = DVec2::new(f64::NAN, 3.0);
        let feet = Feet::new(list);
        assert!(feet.grid.is_some());
        for _ in 0..500 {
            let pos = DVec2::new(rnd() * 1000.0 - 500.0, rnd() * 1000.0 - 500.0);
            let r = rnd() * 90.0;
            let keep = |f: &&Footprint| (f.center - pos).length() < r + f.half_len + f.half_w;
            let by_list: Vec<usize> = feet.iter().filter(keep).map(|f| f.car).collect();
            let by_grid: Vec<usize> = feet.near(pos, r).filter(keep).map(|f| f.car).collect();
            assert_eq!(by_grid, by_list, "at {pos:?} within {r}");
        }
        let few = Feet::new(feet.list[..10].to_vec());
        assert!(few.grid.is_none());
        assert_eq!(few.near(DVec2::ZERO, 1.0).count(), 10);
    }
}
