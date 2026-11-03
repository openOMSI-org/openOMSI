//! What the frame of the people keeps from one frame to the next for speed alone: the
//! working lists of the step (cleared and filled again, not made anew: on macOS every big
//! list grown in a frame costs the system calls of a fresh allocation), the grids that
//! spare the looks of everybody at everybody, and where the time of the people's frames
//! went (OMSI_PROFILE).

use super::*;

/// Working buffers of `tick_inner` and `pax_frame`, reused frame after frame. Nothing in
/// here outlives the frame that fills it.
#[derive(Default)]
pub struct Scratch {
    /// Per person: taken away at the end of this frame (the `remove` list as a mask).
    pub gone: Vec<bool>,
    /// Per person: moved by the crowd this frame.
    pub moved: Vec<bool>,
    pub wants: Vec<Want>,
    pub walkers: Vec<Walker>,
    pub who: Vec<usize>,
    pub ground: Vec<(usize, Walker)>,
    pub cars: Vec<(DVec2, DVec2, f64)>,
    pub blocks: Vec<Block>,
    pub near_blocks: Vec<Block>,
    /// Per vehicle of `blocks`: within reach of somebody of the crowd.
    pub block_near: Vec<bool>,
    /// `blocks` by cell of `BLOCK_CELL` metres: each in every cell within `BLOCK_REACH` of
    /// its box.
    block_grid: HashMap<(i32, i32), Vec<u32>>,
    block_cells: Vec<(i32, i32)>,
    /// The people who may stand in a passenger's way (`pax_blockers`) by cell of
    /// `BLOCKER_CELL` metres, as they stood when the passengers' frame began.
    blocker_grid: HashMap<(i32, i32), Vec<u32>>,
    blocker_cells: Vec<(i32, i32)>,
    /// How many people were filed in `blocker_grid` (None: not filed, outside the
    /// passengers' frame).
    blockers_filed: Option<usize>,
    door_at: Vec<DVec3>,
    door_aboard: HashMap<BusId, Vec<u32>>,
    door_buses: Vec<BusId>,
    door_grid: HashMap<(i32, i32), Vec<u32>>,
    door_cells: Vec<(i32, i32)>,
}

/// The cell size of the vehicle grid of the crowd (m).
const BLOCK_CELL: f64 = 32.0;
/// How far round its box a vehicle is filed in the grid (m): the crowd takes the vehicles
/// within 25 m of anybody (the other looks are closer still).
const BLOCK_REACH: f64 = 25.0;
/// The cell size of the blocker grid (m).
const BLOCKER_CELL: f64 = 4.0;
const DOOR_CELL: f64 = 50.0;

fn cell(p: DVec2, size: f64) -> (i32, i32) {
    ((p.x / size).floor() as i32, (p.y / size).floor() as i32)
}

/// Empty the cells used last time (their lists kept for the next filling); the whole map
/// goes when it has grown big over a long drive.
fn reset<K: Copy + Eq + std::hash::Hash>(grid: &mut HashMap<K, Vec<u32>>, cells: &mut Vec<K>) {
    if grid.len() > 4096 {
        grid.clear();
        cells.clear();
        return;
    }
    for c in cells.drain(..) {
        if let Some(v) = grid.get_mut(&c) {
            v.clear();
        }
    }
}

fn file<K: Copy + Eq + std::hash::Hash>(grid: &mut HashMap<K, Vec<u32>>, cells: &mut Vec<K>, at: K, k: usize) {
    let v = grid.entry(at).or_default();
    if v.is_empty() {
        cells.push(at);
    }
    v.push(k as u32);
}

impl Scratch {
    /// File the vehicles of `blocks` in the grid: each in every cell that a point within
    /// `BLOCK_REACH` of its box can lie in (the box's corners out by the reach on both axes
    /// lie on a circle that holds all of them).
    pub fn file_blocks(&mut self) {
        reset(&mut self.block_grid, &mut self.block_cells);
        for (k, b) in self.blocks.iter().enumerate() {
            let r = (b.half + DVec2::splat(BLOCK_REACH)).length();
            let (lo, hi) = (cell(b.center - DVec2::splat(r), BLOCK_CELL), cell(b.center + DVec2::splat(r), BLOCK_CELL));
            for x in lo.0..=hi.0 {
                for y in lo.1..=hi.1 {
                    file(&mut self.block_grid, &mut self.block_cells, (x, y), k);
                }
            }
        }
    }

    /// The vehicles of `blocks` that may lie within `BLOCK_REACH` of `p` (a superset: each
    /// still to be looked at).
    pub fn blocks_at(&self, p: DVec2) -> &[u32] {
        self.block_grid.get(&cell(p, BLOCK_CELL)).map(|v| &v[..]).unwrap_or(&[])
    }

    /// The vehicles of `blocks` within `BLOCK_REACH` of somebody of `walkers`, in the order
    /// of `blocks` (the crowd adds up their pushes in it), into `out`.
    pub fn blocks_near_crowd(&mut self, walkers: &[Walker], out: &mut Vec<Block>) {
        out.clear();
        self.block_near.clear();
        self.block_near.resize(self.blocks.len(), false);
        for w in walkers {
            let Some(v) = self.block_grid.get(&cell(w.pos, BLOCK_CELL)) else { continue };
            for &k in v {
                let k = k as usize;
                if !self.block_near[k] && self.blocks[k].near(w.pos, BLOCK_REACH) {
                    self.block_near[k] = true;
                }
            }
        }
        out.extend(self.blocks.iter().zip(&self.block_near).filter(|(_, n)| **n).map(|(b, _)| *b));
        debug_assert_eq!(
            out.len(),
            self.blocks.iter().filter(|b| walkers.iter().any(|w| b.near(w.pos, BLOCK_REACH))).count(),
            "the vehicle grid missed one near the crowd"
        );
    }

    /// File everybody by where they stand now, for the looks of `pax_blockers` (all of
    /// them: who counts there may change in the frame, where they stand is what matters).
    pub fn file_blockers(&mut self, people: &[Person]) {
        reset(&mut self.blocker_grid, &mut self.blocker_cells);
        for (k, p) in people.iter().enumerate() {
            file(&mut self.blocker_grid, &mut self.blocker_cells, cell(p.position.truncate(), BLOCKER_CELL), k);
        }
        self.blockers_filed = Some(people.len());
    }

    /// The passengers' frame is over: `pax_blockers` looks at everybody again.
    pub fn drop_blockers(&mut self) {
        self.blockers_filed = None;
    }

    /// File person `k` again where they stand now (moved in the passengers' frame); they
    /// may be filed twice then, which the look at them does not mind.
    pub fn refile_blocker(&mut self, k: usize, was: DVec3, now: DVec3) {
        let c = cell(now.truncate(), BLOCKER_CELL);
        if c != cell(was.truncate(), BLOCKER_CELL) {
            file(&mut self.blocker_grid, &mut self.blocker_cells, c, k);
        }
    }

    /// Everybody filed within one cell of `p`: all who may stand within `BLOCKER_CELL` of it.
    /// False when the filing does not hold the `people` there are now (none made, or
    /// somebody came or went since): then everybody has to be looked at.
    pub fn blockers_near(&self, p: DVec3, people: usize, out: &mut Vec<u32>) -> bool {
        out.clear();
        if self.blockers_filed != Some(people) {
            return false;
        }
        let (cx, cy) = cell(p.truncate(), BLOCKER_CELL);
        for x in cx - 1..=cx + 1 {
            for y in cy - 1..=cy + 1 {
                if let Some(v) = self.blocker_grid.get(&(x, y)) {
                    out.extend_from_slice(v);
                }
            }
        }
        true
    }

    pub fn file_doorways(&mut self, at: impl Iterator<Item = (Option<BusId>, DVec3)>) {
        reset(&mut self.door_aboard, &mut self.door_buses);
        reset(&mut self.door_grid, &mut self.door_cells);
        self.door_at.clear();
        for (inside, pos) in at {
            let k = self.door_at.len();
            self.door_at.push(pos);
            match inside {
                Some(b) => file(&mut self.door_aboard, &mut self.door_buses, b, k),
                None => file(&mut self.door_grid, &mut self.door_cells, cell(pos.truncate(), DOOR_CELL), k),
            }
        }
    }

    pub fn doorways(&self, bn: &BusNow) -> (Vec<bool>, Vec<bool>) {
        let r = DVec2::splat(door_reach(bn));
        let (lo, hi) = (cell(bn.pos.truncate() - r, DOOR_CELL), cell(bn.pos.truncate() + r, DOOR_CELL));
        let aboard = self.door_aboard.get(&bn.id).into_iter().flatten();
        let outside = (lo.0..=hi.0).flat_map(|x| (lo.1..=hi.1).filter_map(move |y| self.door_grid.get(&(x, y)))).flatten();
        doorways_taken_partitioned(bn, aboard.map(|&k| &self.door_at[k as usize]), outside.map(|&k| &self.door_at[k as usize]))
    }
}

/// Each stage of the people's frame: frames, total and longest ms.
#[derive(Default)]
pub struct StageStats(pub Vec<(&'static str, u32, f64, f64)>);

impl StageStats {
    pub fn add(&mut self, name: &'static str, ms: f64) {
        match self.0.iter_mut().find(|s| s.0 == name) {
            Some(s) => {
                s.1 += 1;
                s.2 += ms;
                s.3 = s.3.max(ms);
            }
            None => self.0.push((name, 1, ms, ms)),
        }
    }

    /// `stage avg/longest ms` of every stage, per frame of the people (`frames`).
    pub fn line(&self, frames: u32) -> String {
        let n = frames.max(1) as f64;
        self.0.iter().map(|(k, _, sum, worst)| format!("{k} {:.3}/{worst:.1}", sum / n)).collect::<Vec<_>>().join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A little xorshift for the scattered boxes and walkers.
    struct Rng(u64);
    impl Rng {
        fn f(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    fn walker(pos: DVec2) -> Walker {
        Walker { pos, vel: DVec2::ZERO, radius: 0.28, want: DVec2::ZERO, give: 1.0, space: 0, fixed: false, ghost: false, corridor: None }
    }

    #[test]
    fn the_vehicle_grid_finds_what_a_look_at_every_vehicle_finds() {
        let mut r = Rng(0x9e37_79b9_7f4a_7c15);
        for round in 0..20 {
            let mut s = Scratch::default();
            let spread = 50.0 + round as f64 * 40.0;
            for _ in 0..120 {
                s.blocks.push(Block {
                    center: DVec2::new(r.f() * spread - spread / 2.0, r.f() * spread - spread / 2.0),
                    half: DVec2::new(0.5 + r.f() * 2.0, 1.0 + r.f() * 9.0),
                    heading: r.f() * std::f64::consts::TAU,
                    vel: DVec2::ZERO,
                });
            }
            s.file_blocks();
            let walkers: Vec<Walker> = (0..60).map(|_| walker(DVec2::new(r.f() * spread * 1.4 - spread * 0.7, r.f() * spread * 1.4 - spread * 0.7))).collect();
            let mut near = Vec::new();
            s.blocks_near_crowd(&walkers, &mut near);
            let all: Vec<Block> = s.blocks.iter().filter(|b| walkers.iter().any(|w| b.near(w.pos, BLOCK_REACH))).copied().collect();
            assert_eq!(near.len(), all.len());
            assert!(near.iter().zip(&all).all(|(a, b)| a.center == b.center && a.half == b.half));
            // every vehicle a point lies near (closer than the reach) is in that point's cell
            for _ in 0..200 {
                let p = DVec2::new(r.f() * spread - spread / 2.0, r.f() * spread - spread / 2.0);
                let at: Vec<u32> = s.blocks_at(p).to_vec();
                for (k, b) in s.blocks.iter().enumerate() {
                    if b.near(p, BLOCK_REACH - 1e-9) {
                        assert!(at.contains(&(k as u32)), "vehicle {k} missing at {p:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_blocker_grid_holds_everybody_within_a_cell() {
        let mut r = Rng(0x2545_f491_4f6c_dd1d);
        let mut s = Scratch::default();
        let points: Vec<DVec3> = (0..300).map(|_| DVec3::new(r.f() * 60.0 - 30.0, r.f() * 60.0 - 30.0, 0.0)).collect();
        // (filed by hand: `file_blockers` takes people)
        reset(&mut s.blocker_grid, &mut s.blocker_cells);
        for (k, p) in points.iter().enumerate() {
            file(&mut s.blocker_grid, &mut s.blocker_cells, cell(p.truncate(), BLOCKER_CELL), k);
        }
        s.blockers_filed = Some(points.len());
        let mut out = Vec::new();
        for (i, me) in points.iter().enumerate() {
            assert!(s.blockers_near(*me, points.len(), &mut out));
            for (j, o) in points.iter().enumerate() {
                if (*o - *me).truncate().length() < BLOCKER_CELL {
                    assert!(out.contains(&(j as u32)), "{j} missing round {i}");
                }
            }
        }
        // somebody came or went: no answer from the grid
        assert!(!s.blockers_near(points[0], points.len() + 1, &mut out));
        s.drop_blockers();
        assert!(!s.blockers_near(points[0], points.len(), &mut out));
    }
}
