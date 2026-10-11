//! The pavements as one network of paths for the pedestrians, and their crossings.

use super::*;

/// Whether the straight way from `a` to `b` goes over a carriageway: across the centre
/// line of a street lane (walking along the kerb on the carriageway's edge does not).
pub fn crosses_street(net: &Network, a: DVec2, b: DVec2) -> bool {
    let mut cells: Vec<(i32, i32)> = Vec::new();
    for p in [a, b, (a + b) * 0.5] {
        let c = Network::grid_cell(p.extend(0.0));
        if !cells.contains(&c) {
            cells.push(c);
        }
    }
    let mut seen: Vec<usize> = Vec::new();
    for c in cells {
        for &i in net.grid.get(&c).map(|v| v.as_slice()).unwrap_or(&[]) {
            if seen.contains(&i) {
                continue;
            }
            seen.push(i);
            let l = &net.lanes[i];
            if l.kind != LaneKind::Street {
                continue;
            }
            if l
                .points
                .windows(2)
                .any(|w| segments_cross(a, b, w[0].truncate(), w[1].truncate()))
            {
                return true;
            }
        }
    }
    false
}

/// Whether the segments `a`-`b` and `c`-`d` cross.
pub fn segments_cross(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let side = |p: DVec2, q: DVec2, r: DVec2| (q - p).perp_dot(r - p);
    let (d1, d2) = (side(c, d, a), side(c, d, b));
    let (d3, d4) = (side(a, b, c), side(a, b, d));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

/// One piece of a walk along a pavement path: lane `lane` from distance `a` to `b`.
#[derive(Debug, Clone, Copy)]
pub struct Leg {
    pub lane: usize,
    pub a: f32,
    pub b: f32,
}

impl Leg {
    pub fn len(&self) -> f32 {
        (self.b - self.a).abs()
    }
    pub fn dist(&self, p: f32) -> f32 {
        if self.b >= self.a {
            self.a + p
        } else {
            self.a - p
        }
    }
    /// Point and walking heading `p` metres into the leg.
    pub fn at(&self, net: &Network, p: f32) -> (DVec3, f64) {
        let (q, h) = net.lanes[self.lane].at(self.dist(p.clamp(0.0, self.len())));
        (
            q,
            if self.b >= self.a {
                h as f64
            } else {
                h as f64 + 180.0
            },
        )
    }
    /// How far into the leg the point nearest `pos` lies, looking around `hint`.
    pub fn project(&self, net: &Network, pos: DVec3, hint: f32) -> f32 {
        let (lo, hi) = ((hint - 1.5).max(0.0), (hint + 3.0).min(self.len()));
        let mut best = (hint, f64::MAX);
        let mut p = lo;
        while p <= hi + 1e-3 {
            let d = (self.at(net, p).0 - pos).truncate().length_squared();
            if d < best.1 {
                best = (p, d);
            }
            p += 0.2;
        }
        best.0
    }
    /// Whether the leg starts at an end of its lane (at a kerb or a junction).
    pub fn from_end(&self, net: &Network) -> bool {
        self.a < 0.05 || self.a > net.lanes[self.lane].length() - 0.05
    }
}

/// A walk along the pavement network.
#[derive(Debug, Clone)]
pub struct PedWalk {
    pub legs: Vec<Leg>,
    pub leg: usize,
    /// Metres walked into the current leg.
    pub s: f32,
    /// A stroll: goes on at random when the legs run out.
    pub roam: bool,
    /// Keep-right offset (m).
    pub side: f32,
    /// Seconds spent waiting at the kerb before the current leg.
    pub held: f32,
}

impl PedWalk {
    pub fn new(legs: Vec<Leg>, roam: bool, side: f32) -> PedWalk {
        PedWalk {
            legs,
            leg: 0,
            s: 0.0,
            roam,
            side,
            held: 0.0,
        }
    }
}

/// The pavement paths as a walking network: path ends closer than a metre are one
/// junction, whatever their heading (the road network joins lane ends only when they
/// continue in the same direction, which leaves every pavement corner open).
pub struct PedNet {
    /// Per pavement lane: its start and end junction.
    pub ends: HashMap<usize, (usize, usize)>,
    /// Per junction: (lane, walked forwards) leaving it.
    pub out: Vec<Vec<(usize, bool)>>,
    /// Where each pavement lane crosses a carriageway (lazily), and the carriageway lanes
    /// it crosses.
    pub crossings: HashMap<usize, Vec<DVec2>>,
    pub crossed: HashMap<usize, Vec<usize>>,
    /// Pavement lanes by 50 m cell.
    pub grid: HashMap<(i32, i32), Vec<usize>>,
    /// The junctions and a 1.5 m grid of them, for joining the paths of tiles loaded later.
    pub nodes: Vec<DVec3>,
    pub cells: HashMap<(i64, i64), Vec<usize>>,
    /// How many lanes of the traffic network are in (the network only grows: tiles bring
    /// their lanes and the indices stay).
    pub built: usize,
    pub taps: HashMap<usize, Vec<(f32, usize)>>,
    pub through: Vec<Vec<(usize, f32)>>,
    pub bounds: HashMap<usize, (DVec2, DVec2)>,
    pub along: HashMap<usize, Vec<f32>>,
    pub tracks: HashMap<usize, Vec<f32>>,
}

impl PedNet {
    pub fn build(net: &Network) -> PedNet {
        let mut p = PedNet {
            ends: HashMap::new(),
            out: Vec::new(),
            crossings: HashMap::new(),
            crossed: HashMap::new(),
            grid: HashMap::new(),
            nodes: Vec::new(),
            cells: HashMap::new(),
            built: 0,
            taps: HashMap::new(),
            through: Vec::new(),
            bounds: HashMap::new(),
            along: HashMap::new(),
            tracks: HashMap::new(),
        };
        p.extend(net);
        log::info!(
            "pavement network: {} paths, {} junctions",
            p.ends.len(),
            p.nodes.len()
        );
        p
    }

    /// Take in the lanes the network gained since the last call (tiles streamed in).
    pub fn extend(&mut self, net: &Network) -> usize {
        let from = self.built.min(net.lanes.len());
        let before = self.ends.len();
        let mut added = Vec::new();
        for i in from..net.lanes.len() {
            let l = &net.lanes[i];
            if matches!(l.kind, LaneKind::Street | LaneKind::Rail) && from > 0 {
                // a new carriageway may cross pavement paths that are in already
                self.crossings.clear();
                self.along.clear();
                self.tracks.clear();
            }
            if l.kind != LaneKind::Sidewalk || l.points.len() < 2 || l.length() < 0.3 {
                continue;
            }
            let a = self.node_of(l.start());
            let b = self.node_of(l.end());
            if a == b && l.length() < 3.0 {
                continue;
            }
            self.ends.insert(i, (a, b));
            self.out[a].push((i, true));
            self.out[b].push((i, false));
            let lo = l.points.iter().fold(DVec2::splat(f64::MAX), |m, p| m.min(p.truncate()));
            let hi = l.points.iter().fold(DVec2::splat(f64::MIN), |m, p| m.max(p.truncate()));
            self.bounds.insert(i, (lo, hi));
            added.push(i);
            let mut seen: Vec<(i32, i32)> = Vec::new();
            for p in &l.points {
                let c = ((p.x / 50.0).floor() as i32, (p.y / 50.0).floor() as i32);
                if !seen.contains(&c) {
                    seen.push(c);
                    self.grid.entry(c).or_default().push(i);
                }
            }
        }
        self.built = net.lanes.len();
        self.join_midway(net, &added, from);
        self.ends.len() - before
    }

    fn lanes_near(&self, p: DVec2) -> Vec<usize> {
        let (cx, cy) = ((p.x / 50.0).floor() as i32, (p.y / 50.0).floor() as i32);
        let mut lanes = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                lanes.extend_from_slice(self.grid.get(&(cx + dx, cy + dy)).map(|v| v.as_slice()).unwrap_or(&[]));
            }
        }
        lanes.sort_unstable();
        lanes.dedup();
        lanes
    }

    fn join_midway(&mut self, net: &Network, added: &[usize], from: usize) {
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for &i in added {
            let (a, b) = self.ends[&i];
            for n in [a, b] {
                for l in self.lanes_near(self.nodes[n].truncate()) {
                    pairs.push((l, n));
                }
            }
            if from > 0 {
                let mut cells: Vec<(i32, i32)> = net.lanes[i].points.iter().map(|p| ((p.x / 50.0).floor() as i32, (p.y / 50.0).floor() as i32)).collect();
                cells.sort_unstable();
                cells.dedup();
                let mut old: Vec<usize> = Vec::new();
                for (cx, cy) in cells {
                    let c = DVec2::new((cx as f64 + 0.5) * 50.0, (cy as f64 + 0.5) * 50.0);
                    old.extend(self.lanes_near(c).into_iter().filter(|&l| l < from));
                }
                old.sort_unstable();
                old.dedup();
                for l in old {
                    let (a, b) = self.ends[&l];
                    pairs.push((i, a));
                    pairs.push((i, b));
                }
            }
        }
        for (l, n) in pairs {
            self.tap(net, l, n);
        }
    }

    fn tap(&mut self, net: &Network, l: usize, n: usize) {
        const JOIN: f64 = 1.2;
        let Some(&(a, b)) = self.ends.get(&l) else { return };
        let p = self.nodes[n];
        let (lo, hi) = self.bounds[&l];
        if a == n || b == n || p.x < lo.x - JOIN || p.y < lo.y - JOIN || p.x > hi.x + JOIN || p.y > hi.y + JOIN {
            return;
        }
        let lane = &net.lanes[l];
        let Some((s, _)) = lane.nearest_point(p) else { return };
        if s < 0.3 || s > lane.length() - 0.3 {
            return;
        }
        let (q, _) = lane.at(s);
        if (q - p).truncate().length() >= JOIN || (q.z - p.z).abs() >= 2.5 {
            return;
        }
        let taps = self.taps.entry(l).or_default();
        if taps.iter().any(|&(_, m)| m == n) {
            return;
        }
        taps.push((s, n));
        taps.sort_by(|x, y| x.0.total_cmp(&y.0));
        self.through[n].push((l, s));
    }

    fn stop(&self, net: &Network, lane: usize, from: f32, fwd: bool) -> f32 {
        let taps = self.taps.get(&lane).map(|t| t.as_slice()).unwrap_or(&[]);
        if fwd {
            taps.iter().map(|t| t.0).find(|&s| s > from + 0.05).unwrap_or(net.lanes[lane].length())
        } else {
            taps.iter().rev().map(|t| t.0).find(|&s| s < from - 0.05).unwrap_or(0.0)
        }
    }

    fn leg_from(&self, net: &Network, lane: usize, a: f32, fwd: bool) -> Leg {
        Leg { lane, a, b: self.stop(net, lane, a, fwd) }
    }

    fn together(&self, node: usize) -> Vec<usize> {
        let mut nodes = vec![node];
        for &(l, s) in &self.through[node] {
            for &(at, n) in self.taps.get(&l).map(|t| t.as_slice()).unwrap_or(&[]) {
                if (at - s).abs() < 0.05 && !nodes.contains(&n) {
                    nodes.push(n);
                }
            }
        }
        nodes
    }

    pub fn is_tap(&self, lane: usize, s: f32) -> bool {
        self.taps.get(&lane).is_some_and(|t| t.iter().any(|&(at, _)| (at - s).abs() < 0.05))
    }

    /// The junction at `p`, a new one when there is none within a metre.
    pub fn node_of(&mut self, p: DVec3) -> usize {
        let (cx, cy) = ((p.x / 1.5).floor() as i64, (p.y / 1.5).floor() as i64);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(list) = self.cells.get(&(cx + dx, cy + dy)) {
                    for &n in list {
                        if (self.nodes[n] - p).truncate().length() < 1.2
                            && (self.nodes[n].z - p.z).abs() < 2.5
                        {
                            return n;
                        }
                    }
                }
            }
        }
        self.nodes.push(p);
        self.out.push(Vec::new());
        self.through.push(Vec::new());
        self.cells
            .entry((cx, cy))
            .or_default()
            .push(self.nodes.len() - 1);
        self.nodes.len() - 1
    }

    /// The pavement lane nearest `p` within `reach` that can be reached without going over
    /// a carriageway: (lane, distance along it, distance to it). The plain nearest one was
    /// often the pavement across the road - a passenger off a bus then walked straight
    /// over the carriageway through the traffic to it, or joined a crossing in the middle.
    pub fn nearest(&self, net: &Network, p: DVec3, reach: f64) -> Option<(usize, f32, f64)> {
        let (cx, cy) = ((p.x / 50.0).floor() as i32, (p.y / 50.0).floor() as i32);
        let mut cands: Vec<(usize, f32, f64)> = Vec::new();
        let mut seen = HashSet::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &i in self
                    .grid
                    .get(&(cx + dx, cy + dy))
                    .map(|v| v.as_slice())
                    .unwrap_or(&[])
                {
                    if !seen.insert(i) {
                        continue;
                    }
                    if let Some((s, d)) = net.lanes[i].nearest_point(p) {
                        if d < reach {
                            cands.push((i, s, d));
                        }
                    }
                }
            }
        }
        cands.sort_by(|a, b| a.2.total_cmp(&b.2));
        let first = cands.first().copied();
        cands
            .into_iter()
            .find(|&(i, s, _)| {
                let (q, _) = net.lanes[i].at(s);
                !crosses_street(net, p.truncate(), q.truncate())
                    && !self.crossings.get(&i).map(|x| !x.is_empty()).unwrap_or(false)
            })
            // on an island between carriageways: the nearest after all
            .or(first)
    }

    /// The junction a leg ends at, when it ends at one.
    pub fn end_node(&self, net: &Network, leg: &Leg) -> Option<usize> {
        let (a, b) = *self.ends.get(&leg.lane)?;
        let len = net.lanes[leg.lane].length();
        if leg.b < 0.05 {
            Some(a)
        } else if leg.b > len - 0.05 {
            Some(b)
        } else {
            self.taps.get(&leg.lane)?.iter().find(|t| (t.0 - leg.b).abs() < 0.05).map(|t| t.1)
        }
    }

    /// A leg leaving junction `node`, not back along `came` (unless it is a dead end).
    pub fn next_leg(&self, net: &Network, node: usize, came: &Leg, pick: u64) -> Option<Leg> {
        let back = self.ends.get(&came.lane).copied();
        let ahead = if came.len() < 0.05 { came.b > 0.05 } else { came.b > came.a };
        let twin = |l: usize| -> bool {
            matches!((self.ends.get(&l), back), (Some(&(a, b)), Some((c, d))) if a == d && b == c && (net.lanes[l].length() - net.lanes[came.lane].length()).abs() < 1.0)
        };
        let reverse = |l: usize, a: f32, fwd: bool| l == came.lane && fwd != ahead && (a - came.b).abs() < 0.05;
        let mut list: Vec<Leg> = Vec::new();
        self.out.get(node)?;
        for n in self.together(node) {
            for &(l, fwd) in &self.out[n] {
                let a = if fwd { 0.0 } else { net.lanes[l].length() };
                if (l != came.lane && twin(l)) || (l == came.lane && (!self.taps.contains_key(&l) || reverse(l, a, fwd))) {
                    continue;
                }
                list.push(self.leg_from(net, l, a, fwd));
            }
            for &(l, s) in &self.through[n] {
                for fwd in [true, false] {
                    let leg = self.leg_from(net, l, s, fwd);
                    if !reverse(l, s, fwd) && !list.iter().any(|o| o.lane == leg.lane && (o.b - leg.b).abs() < 0.05 && (o.b > o.a) == fwd) {
                        list.push(leg);
                    }
                }
            }
        }
        // the way on rather than back: a path leaving the junction within 110° of the way
        // the walker came (there usually is one - a pavement goes on past a side street),
        // else any. Picked from all, a stroller would turn round at every corner and walk
        // back the way they came, which looked like a change of mind for no reason.
        let heading_in = {
            let h = net.lanes[came.lane].at(came.b).1 as f64;
            if ahead { h } else { wrap_heading(h + 180.0) }
        };
        let onward: Vec<Leg> = list
            .iter()
            .copied()
            .filter(|o| angle_between(heading_in, o.at(net, 0.0).1) <= 110.0)
            .collect();
        let list = if onward.is_empty() { list } else { onward };
        if list.is_empty() {
            // a dead end: turn round
            let (a, _) = back?;
            let fwd = a == node;
            let start = if fwd { 0.0 } else { net.lanes[came.lane].length() };
            return Some(self.leg_from(net, came.lane, start, fwd));
        }
        Some(list[(pick as usize) % list.len()])
    }

    pub fn crosses_on(&mut self, net: &Network, leg: &Leg) -> bool {
        let (lo, hi) = (leg.a.min(leg.b) - 0.5, leg.a.max(leg.b) + 0.5);
        !self.crossings_on(net, leg).is_empty() || self.tracks[&leg.lane].iter().any(|s| (lo..=hi).contains(s))
    }

    pub fn crossings_on(&mut self, net: &Network, leg: &Leg) -> Vec<DVec2> {
        self.crossings(net, leg.lane);
        let (lo, hi) = (leg.a.min(leg.b) - 0.5, leg.a.max(leg.b) + 0.5);
        self.crossings[&leg.lane]
            .iter()
            .zip(&self.along[&leg.lane])
            .filter(|(_, s)| (lo..=hi).contains(*s))
            .map(|(x, _)| *x)
            .collect()
    }

    /// Where pavement lane `lane` crosses a carriageway.
    pub fn crossings(&mut self, net: &Network, lane: usize) -> &[DVec2] {
        if !self.crossings.contains_key(&lane) {
            let l = &net.lanes[lane];
            let mut cand: Vec<usize> = Vec::new();
            for p in &l.points {
                if let Some(list) = net
                    .grid
                    .get(&((p.x / 50.0).floor() as i32, (p.y / 50.0).floor() as i32))
                {
                    for &i in list {
                        if matches!(net.lanes[i].kind, LaneKind::Street | LaneKind::Rail) && !cand.contains(&i) {
                            cand.push(i);
                        }
                    }
                }
            }
            let mut out = Vec::new();
            let mut along = Vec::new();
            let mut tracks = Vec::new();
            let mut crossed = Vec::new();
            for i in cand {
                let o = &net.lanes[i];
                for (k, w) in l.points.windows(2).enumerate() {
                    for v in o.points.windows(2) {
                        if (w[0].z - v[0].z).abs() > 3.0 {
                            continue;
                        }
                        if let Some(x) = seg_cross(
                            w[0].truncate(),
                            w[1].truncate(),
                            v[0].truncate(),
                            v[1].truncate(),
                        ) {
                            let s = l.dist[k] + (x - w[0].truncate()).length() as f32;
                            if o.kind == LaneKind::Rail {
                                tracks.push(s);
                                continue;
                            }
                            if !crossed.contains(&i) {
                                crossed.push(i);
                            }
                            if !out.iter().any(|q: &DVec2| (*q - x).length() < 1.5) {
                                out.push(x);
                                along.push(s);
                            }
                        }
                    }
                }
            }
            self.crossings.insert(lane, out);
            self.along.insert(lane, along);
            self.tracks.insert(lane, tracks);
            self.crossed.insert(lane, crossed);
        }
        &self.crossings[&lane]
    }

    /// The carriageway lanes a pavement lane crosses.
    pub fn crossed_lanes(&mut self, net: &Network, lane: usize) -> &[usize] {
        self.crossings(net, lane);
        &self.crossed[&lane]
    }
}

/// Seconds a pedestrian starting across `path` now has before a vehicle may drive over it:
/// until the first light of a carriageway lane it crosses (or of a lane leading into one)
/// turns green once the pedestrian green is over. Lanes that have green now, or get it
/// while the pedestrians still have theirs, are turning traffic that gives way. Without
/// such a light, the pedestrian green `green_left` and two seconds.
pub fn pedestrian_window(
    ped: Option<&mut PedNet>,
    net: &Network,
    traffic: &TrafficSim,
    path: usize,
    green_left: f32,
) -> f32 {
    let mut window = f32::MAX;
    if let Some(ped) = ped {
        for &s in ped.crossed_lanes(net, path) {
            let feeding = net.prev.get(s).map(|p| p.as_slice()).unwrap_or(&[]);
            for &l in std::iter::once(&s).chain(feeding) {
                let Some((c, li)) = net.lanes[l].traffic_light else {
                    continue;
                };
                if let Some(g) = traffic
                    .light_until_go(c, li)
                    .filter(|g| *g > 0.0 && *g >= green_left)
                {
                    window = window.min(g);
                }
            }
        }
    }
    if window == f32::MAX {
        green_left + 2.0
    } else {
        window
    }
}

pub fn seg_cross(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> Option<DVec2> {
    let r = b - a;
    let s = d - c;
    let den = r.perp_dot(s);
    if den.abs() < 1e-9 {
        return None;
    }
    let t = (c - a).perp_dot(s) / den;
    let u = (c - a).perp_dot(r) / den;
    (t >= 0.0 && t <= 1.0 && u >= 0.0 && u <= 1.0).then(|| a + r * t)
}
