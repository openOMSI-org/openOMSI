//! Growing the network as the map streams in.

use super::*;

/// Growing the network while the map streams in: the lanes of newly loaded tiles are
/// appended (existing indices stay valid for the cars and routes that hold them) and linked
/// to what is there, with the same rules as [`Network::link`].
/// How near (m) a lane's end and the next one's start must lie to be linked: what the game
/// links its road network with, and what a driving choice measures short paths against.
pub const LINK_TOLERANCE: f64 = 1.5;

impl Network {
    /// Append `new` lanes and link them. Returns the range of their indices.
    pub fn extend(&mut self, new: Vec<Lane>, tol: f64) -> std::ops::Range<usize> {
        let first = self.lanes.len();
        if new.is_empty() {
            return first..first;
        }
        if first == 0 {
            self.lanes = new;
            self.link(tol);
            return 0..self.lanes.len();
        }
        self.lanes.extend(new);
        let end = self.lanes.len();
        for i in first..end {
            if let Some(k) = self.lanes[i].key {
                self.by_key.entry(k).or_default().push(i);
            }
            self.start_grid.entry(Self::grid_cell(self.lanes[i].start())).or_default().push(i);
            for c in Self::lane_cells(&self.lanes[i]) {
                self.grid.entry(c).or_default().push(i);
            }
        }
        // neighbouring lanes of the new splines
        let mut by_spline: HashMap<((i32, i32), i64, bool), Vec<usize>> = HashMap::new();
        for i in first..end {
            let l = &self.lanes[i];
            if let (Some(k), 1, LaneKind::Street) = (l.key, l.source, l.kind) {
                by_spline.entry((k.tile, k.id, l.reversed)).or_default().push(i);
            }
        }
        for (_, mut list) in by_spline {
            list.sort_by(|a, b| self.lanes[*a].offset.total_cmp(&self.lanes[*b].offset));
            for w in list.windows(2) {
                let (a, b) = (w[0], w[1]);
                let gap = self.lanes[b].offset - self.lanes[a].offset;
                if gap > 1.5 && gap < 6.0 {
                    if self.lanes[a].reversed {
                        self.lanes[a].left = Some(b);
                        self.lanes[b].right = Some(a);
                    } else {
                        self.lanes[a].right = Some(b);
                        self.lanes[b].left = Some(a);
                    }
                }
            }
        }
        // links: ends of new lanes to any start, ends of old lanes to new starts
        let joins = |net: &Network, from: usize, to: usize| -> bool {
            let (a, b) = (&net.lanes[from], &net.lanes[to]);
            if from == to || a.kind != b.kind {
                return false;
            }
            let (e, s) = (a.end(), b.start());
            let dh = ((b.start_heading() - a.end_heading() + 540.0) % 360.0 - 180.0).abs();
            (s - e).truncate().length() <= tol && dh < 40.0 && (s.z - e.z).abs() < 3.0
        };
        let near = |net: &Network, p: DVec3| -> Vec<usize> {
            let c = ((p.x / GRID_CELL).floor() as i32, (p.y / GRID_CELL).floor() as i32);
            let mut out: Vec<usize> = Vec::new();
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(list) = net.grid.get(&(c.0 + dx, c.1 + dy)) {
                        out.extend(list.iter().copied());
                    }
                }
            }
            out.sort_unstable();
            out.dedup();
            out
        };
        let mut added: Vec<(usize, usize)> = Vec::new();
        for i in first..end {
            let e = self.lanes[i].end();
            let found: Vec<usize> = near(self, e).into_iter().filter(|&j| joins(self, i, j)).collect();
            for &j in &found {
                added.push((i, j));
            }
            self.lanes[i].next = found;
            let s = self.lanes[i].start();
            for j in near(self, s) {
                if j < first && joins(self, j, i) && !self.lanes[j].next.contains(&i) {
                    self.lanes[j].next.push(i);
                    added.push((j, i));
                }
            }
        }
        self.prev.resize(end, Vec::new());
        for (from, to) in added {
            if !self.prev[to].contains(&from) {
                self.prev[to].push(from);
            }
        }
        // meeting places and footpath crossings inside the new junction objects, and how far
        // every lane now leads (a dead end may go on into the new tiles)
        self.conflicts_from(first);
        self.update_reach_from(first);
        first..end
    }

    /// Appending lanes can only change the reach of those lanes and their predecessors.
    /// An outgoing edge into the rest of the network uses its already settled reach. This
    /// avoids rewalking every loaded tile whenever the streamer adds a small batch.
    fn update_reach_from(&mut self, first: usize) {
        if self.reach.len() != first {
            self.update_reach();
            return;
        }
        let mut affected: hashbrown::HashSet<usize> = (first..self.lanes.len()).collect();
        let mut pending: Vec<usize> = affected.iter().copied().collect();
        while let Some(i) = pending.pop() {
            // The vector-based full pass is cheaper once much of the network leads
            // into this tile (for example a strongly connected city grid).
            if affected.len() > self.lanes.len() / 4 {
                self.update_reach();
                return;
            }
            for &p in &self.prev[i] {
                if affected.insert(p) {
                    pending.push(p);
                }
            }
        }
        self.reach.resize(self.lanes.len(), REACH_MAX);
        let counts = |i: usize, j: usize| !self.lanes[j].no_cars || self.lanes[i].no_cars;
        let mut waiting: HashMap<usize, usize> = affected.iter().map(|&i| {
            (i, self.lanes[i].next.iter().filter(|&&j| affected.contains(&j) && counts(i, j)).count())
        }).collect();
        let mut ready: Vec<usize> = waiting.iter().filter_map(|(&i, &n)| (n == 0).then_some(i)).collect();
        for &i in &affected {
            self.reach[i] = REACH_MAX;
        }
        while let Some(i) = ready.pop() {
            let l = &self.lanes[i];
            let best = l.next.iter().filter(|&&j| counts(i, j)).map(|&j| self.reach[j]).fold(0.0f32, f32::max);
            self.reach[i] = (l.length() + best).min(REACH_MAX);
            for &p in &self.prev[i] {
                if counts(p, i) {
                    if let Some(n) = waiting.get_mut(&p) {
                        *n = n.saturating_sub(1);
                        if *n == 0 {
                            ready.push(p);
                        }
                    }
                }
            }
        }
    }

    /// Like [`nearest_lane`](Self::nearest_lane), but only among the lanes in the grid cells
    /// around `p` (a cell is [`GRID_CELL`]): None when there are none there, never a search
    /// of the whole network.
    pub fn nearest_lane_near(&self, p: DVec3, kind: LaneKind) -> Option<(usize, f32, f64)> {
        let (cx, cy) = Self::grid_cell(p);
        let mut best: Option<(usize, f32, f64)> = None;
        let mut seen: Vec<usize> = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(list) = self.grid.get(&(cx + dx, cy + dy)) else { continue };
                for &i in list {
                    if seen.contains(&i) {
                        continue;
                    }
                    seen.push(i);
                    let l = &self.lanes[i];
                    if l.kind != kind {
                        continue;
                    }
                    if let Some((s, d)) = l.nearest_point(p) {
                        if best.map(|b| d < b.2).unwrap_or(true) {
                            best = Some((i, s, d));
                        }
                    }
                }
            }
        }
        best
    }

    /// The nearest lane of `kind` within `max_dist` of `p` that runs within `max_turn`
    /// degrees of `heading` there (a vehicle's own lane, not the one beside it going the
    /// other way): (lane, distance along it, distance to it).
    pub fn lane_along(&self, p: DVec3, heading: f64, kind: LaneKind, max_dist: f64, max_turn: f64) -> Option<(usize, f32, f64)> {
        let (cx, cy) = Self::grid_cell(p);
        let mut best: Option<(usize, f32, f64)> = None;
        let mut seen: Vec<usize> = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(list) = self.grid.get(&(cx + dx, cy + dy)) else { continue };
                for &i in list {
                    if seen.contains(&i) {
                        continue;
                    }
                    seen.push(i);
                    let l = &self.lanes[i];
                    if l.kind != kind {
                        continue;
                    }
                    let Some((s, d)) = l.nearest_point(p) else { continue };
                    if d > max_dist || best.map(|b| d >= b.2).unwrap_or(false) {
                        continue;
                    }
                    let turn = (l.at(s).1 as f64 - heading + 540.0).rem_euclid(360.0) - 180.0;
                    if turn.abs() <= max_turn {
                        best = Some((i, s, d));
                    }
                }
            }
        }
        best
    }

    /// The grid cell `p` lies in.
    pub fn grid_cell(p: DVec3) -> (i32, i32) {
        ((p.x / GRID_CELL).floor() as i32, (p.y / GRID_CELL).floor() as i32)
    }

    /// The grid cells a lane's points touch (see [`Network::build_grid`]).
    pub fn lane_cells(l: &Lane) -> Vec<(i32, i32)> {
        let mut cells: Vec<(i32, i32)> = Vec::new();
        // (a set beside the list: GPM's TCSP8.sco has a path 968 km long, 19 613 cells searched one by one)
        let mut seen: hashbrown::HashSet<(i32, i32)> = hashbrown::HashSet::new();
        for w in l.points.windows(2) {
            let n = ((w[1] - w[0]).truncate().length() / (GRID_CELL * 0.5)).ceil().max(1.0) as usize;
            for k in 0..=n {
                let q = w[0].lerp(w[1], k as f64 / n as f64);
                let c = ((q.x / GRID_CELL).floor() as i32, (q.y / GRID_CELL).floor() as i32);
                if seen.insert(c) {
                    cells.push(c);
                }
            }
        }
        if l.points.len() == 1 {
            cells.push(((l.points[0].x / GRID_CELL).floor() as i32, (l.points[0].y / GRID_CELL).floor() as i32));
        }
        cells
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lane_along_a_heading_is_the_one_going_that_way() {
        let north = LaneBuilder::arc(DVec3::new(0.0, 0.0, 0.0), 0.0, 100.0, 0.0, 0.0, LaneKind::Street, 3.0);
        let south = LaneBuilder::arc(DVec3::new(3.5, 100.0, 0.0), 180.0, 100.0, 0.0, 0.0, LaneKind::Street, 3.0);
        let mut net = Network { lanes: vec![north, south], ..Default::default() };
        net.build_grid();
        let p = DVec3::new(3.0, 50.0, 0.0);
        assert_eq!(net.nearest_lane(p, LaneKind::Street).map(|f| f.0), Some(1), "the nearest is the southbound");
        assert_eq!(net.lane_along(p, 0.0, LaneKind::Street, 25.0, 75.0).map(|f| f.0), Some(0), "dragged north: the northbound");
        assert_eq!(net.lane_along(p, 180.0, LaneKind::Street, 25.0, 75.0).map(|f| f.0), Some(1));
        assert_eq!(net.lane_along(p, 90.0, LaneKind::Street, 25.0, 75.0), None, "across the road: none");
        assert_eq!(net.lane_along(DVec3::new(60.0, 50.0, 0.0), 0.0, LaneKind::Street, 25.0, 75.0), None, "too far");
    }
}
