//! How full the stretches of road behind a fork are, for an AI car's choice there.
//!
//! Omsi.exe joins its paths into chains: the runs of road between two forks or merges
//! (FUN_007210dc / FUN_00720a4c build them; a chain record holds its paths, per traffic
//! group the sum of their `[rule] trafficdensity`, the cars of the group in it and the
//! ratio of the two, FUN_007238c8). Where a random car's path forks (FUN_0071ee40,
//! 0x71f584..0x71f6dd) it takes, of the ways on that carry its group (`density > 0`) and
//! are open to its vehicle type (FUN_0071d714), the one whose chain has the fewest cars for
//! its density; on a tie the first of them. So the cars spread over a junction's ways in
//! proportion to the density the map gives them, and an empty branch is the next car's.
//! A chain counts the cars of both directions of its two-way paths.

use super::*;

/// The chain fill of a chain without density for the group (Omsi.exe 0x7238c8: 10000.0).
pub const NO_DENSITY_FILL: f32 = 10_000.0;
/// A chain is followed at most this many lanes each way (a ring road without forks would
/// otherwise be walked all round).
const CHAIN_MAX: usize = 256;

/// The traffic group a car is counted under: its `unsched_vehgroups.txt` pool + 1, or 0
/// for a car without one (which goes by the paths' plain density).
pub fn load_group(pool: Option<usize>) -> u32 {
    pool.map(|p| p as u32 + 1).unwrap_or(0)
}

/// The cars of each group on each lane, filled once a step (`Network::refresh_load`).
#[derive(Default, Debug, Clone)]
pub struct LaneLoad {
    counts: HashMap<(usize, u32), u32>,
}

impl LaneLoad {
    pub fn get(&self, lane: usize, group: u32) -> u32 {
        self.counts.get(&(lane, group)).copied().unwrap_or(0)
    }
}

impl Network {
    /// Does lane `a` run on into `b` without a fork or a merge between (both in one chain)?
    fn joined(&self, a: usize, b: usize) -> bool {
        self.lanes[a].next.len() == 1
            && self.lanes[a].next[0] == b
            && self.prev.get(b).is_some_and(|p| p.len() == 1)
    }

    /// Does a chain begin with `lane` (a way on from a fork, or after a merge)?
    pub fn chain_head(&self, lane: usize) -> bool {
        match self.prev.get(lane).map(|p| p.as_slice()) {
            Some([p]) => !self.joined(*p, lane),
            _ => true,
        }
    }

    /// The lanes of `lane`'s chain, with the lanes of the same paths in the other
    /// direction.
    pub fn chain_of(&self, lane: usize) -> Vec<usize> {
        let mut run = vec![lane];
        let mut at = lane;
        while run.len() < CHAIN_MAX {
            match self.lanes[at].next.as_slice() {
                [n] if self.joined(at, *n) && !run.contains(n) => {
                    run.push(*n);
                    at = *n;
                }
                _ => break,
            }
        }
        at = lane;
        while run.len() < 2 * CHAIN_MAX {
            match self.prev.get(at).map(|p| p.as_slice()) {
                Some([p]) if self.joined(*p, at) && !run.contains(p) => {
                    run.push(*p);
                    at = *p;
                }
                _ => break,
            }
        }
        let mut all = run.clone();
        for &l in &run {
            if let Some(twins) = self.lanes[l].key.as_ref().and_then(|k| self.by_key.get(k)) {
                for &t in twins {
                    if !all.contains(&t) {
                        all.push(t);
                    }
                }
            }
        }
        all
    }

    /// Count the cars on the lanes: each `(lane it is on, group, lanes it has planned)`.
    /// A car is counted where it is, and also at the head of each chain its plan takes it
    /// into: openOMSI decides the way up to 90 m ahead where Omsi.exe decides it on the
    /// path before (the slots at `+0x94` are re-decided whenever the car's path changes),
    /// and a car that has decided on a branch has to count there, or the cars of a queue
    /// all chose the empty branch before the first of them got there (#2909).
    pub fn refresh_load<P: IntoIterator<Item = usize>>(
        &mut self,
        cars: impl IntoIterator<Item = (usize, u32, P)>,
    ) {
        self.load.counts.clear();
        for (lane, group, plan) in cars {
            self.add_load(lane, group, plan);
        }
    }

    /// Count one more car (see `refresh_load`): one put on the road during the step.
    pub fn add_load(&mut self, lane: usize, group: u32, plan: impl IntoIterator<Item = usize>) {
        if lane < self.lanes.len() {
            *self.load.counts.entry((lane, group)).or_insert(0) += 1;
        }
        for p in plan {
            if p < self.lanes.len() && self.chain_head(p) {
                *self.load.counts.entry((p, group)).or_insert(0) += 1;
            }
        }
    }

    /// The fill of `lane`'s chain for `group` (Omsi.exe FUN_007238c8): its cars over the
    /// sum of its paths' densities (`density`, a path counted once for both directions);
    /// `NO_DENSITY_FILL` where that sum is not above 0.
    pub fn chain_fill(&self, lane: usize, group: u32, density: &dyn Fn(&Lane) -> f32) -> f32 {
        let lanes = self.chain_of(lane);
        let mut paths: Vec<&LaneKey> = Vec::new();
        let mut sum = 0.0;
        let mut cars = 0;
        for &l in &lanes {
            cars += self.load.get(l, group);
            let ln = &self.lanes[l];
            if let Some(k) = ln.key.as_ref() {
                if paths.contains(&k) {
                    continue;
                }
                paths.push(k);
            }
            sum += density(ln);
        }
        if sum <= 0.0 {
            NO_DENSITY_FILL
        } else {
            cars as f32 / sum
        }
    }
}
