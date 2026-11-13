//! Questions to the whole network and its linking: neighbouring and opposite lanes, the
//! conflicts and who yields, reach, the start grid and the shortest path.

use super::*;

impl Network {
    /// The sign of a lateral offset (positive to the right) towards the oncoming lane:
    /// -1 on the right-hand side of the road, +1 on a left-hand-traffic map.
    pub fn oncoming_sign(&self) -> f32 {
        if self.left_hand {
            1.0
        } else {
            -1.0
        }
    }

    /// Lane with the given key, preferring the direction flag.
    pub fn find(&self, key: LaneKey, reversed: Option<bool>) -> Option<usize> {
        let list = self.by_key.get(&key)?;
        match reversed {
            Some(r) => list.iter().copied().find(|&i| self.lanes[i].reversed == r).or_else(|| list.first().copied()),
            None => list.first().copied(),
        }
    }

    /// Does `b` run beside `a` from about where `a` starts - another lane of the same spline,
    /// or the other branch of a fork in the same junction - without `a` leading into it? A
    /// timetable track that lists two such paths one after the other changes lanes there
    /// (Spandau's line 92 moves from lane 5 to lane 6 of a six-lane Falkenseer Chaussee
    /// piece that way); driving `a` to its end first and then starting `b` sent the bus
    /// 30 m back and sideways.
    ///
    /// Or `b` runs beside a stretch of `a` without starting where it does: a bus lane or
    /// stop lane of a junction object that begins before or after the lane the route came
    /// in on (Spandau's Klosterstraße, Heerstraße and Altstädter Ring junctions: the station
    /// link reaches the stop on the through lane, the next one leaves from the stop lane
    /// beside it). Driven one after the other the bus jumped up to 58 m back.
    pub fn parallel(&self, a: usize, b: usize) -> bool {
        let (Some(la), Some(lb)) = (self.lanes.get(a), self.lanes.get(b)) else { return false };
        if a == b || la.next.contains(&b) {
            return false;
        }
        let same = match (la.key, lb.key) {
            (Some(x), Some(y)) => x.tile == y.tile && x.id == y.id && x.path != y.path,
            _ => false,
        };
        if same && (lb.start() - la.start()).truncate().length() < 8.0 && (lb.start().z - la.start().z).abs() < 1.0 && wrap_deg(lb.start_heading() - la.start_heading()).abs() < 45.0 {
            return true;
        }
        // (a lane of another spline beside it counts as well: mod maps lay a bus lane or a
        // second carriageway as splines of their own, and Novi Sad's tracks change onto
        // them 30 m before the lane they come in on ends)
        self.beside(la, lb)
    }

    /// A route can name two branches of the same junction as a lane change, even after
    /// the branches have parted. Only begin the manoeuvre while their centres are still
    /// beside one another at the vehicle and a little way ahead.
    pub fn route_change_locally_possible(&self, a: usize, b: usize, s: f32) -> bool {
        let (Some(la), Some(lb)) = (self.lanes.get(a), self.lanes.get(b)) else {
            return false;
        };
        let end = la.length();
        if s >= end - 3.0 {
            return false;
        }
        for along in [s, (s + 6.0).min(end)] {
            let (p, h) = la.at(along);
            let Some((sb, distance)) = lb.nearest_point(p) else {
                return false;
            };
            let (q, hb) = lb.at(sb);
            if distance > 5.5 || (p.z - q.z).abs() > 1.0
                || wrap_deg(hb - h).abs() > 30.0
                || sb >= lb.length() - 1.0
            {
                return false;
            }
        }
        true
    }

    /// If the target lane is occupied, wait before the two lanes part so the change
    /// remains possible when a gap opens. Waiting at the end of the source lane is too
    /// late for a junction fork or a bus lane that peels away before that end.
    pub fn route_change_wait_distance(&self, a: usize, b: usize, s: f32) -> f32 {
        let end = self.lanes[a].length();
        let mut probe = s + 2.0;
        while probe < end && probe < s + 100.0 {
            if !self.route_change_locally_possible(a, b, probe) {
                return (probe - s - 8.0).max(0.0);
            }
            probe += 2.0;
        }
        (end - s - 1.0).max(0.0)
    }

    /// Does lane `b` run beside `a` (a lane's width or so to the side, the same way) along
    /// at least 8 m of it?
    fn beside(&self, la: &Lane, lb: &Lane) -> bool {
        let n = 8;
        let (mut along, step) = (0.0f32, la.length() / n as f32);
        for i in 0..=n {
            let s = la.length() * i as f32 / n as f32;
            let (p, h) = la.at(s);
            let Some((sb, d)) = lb.nearest_point(p) else { continue };
            let inside = sb > 0.5 && sb < lb.length() - 0.5;
            // (on the same level: a track or road under a bridge lies a few metres off in 3D
            // too, and a train "changed lanes" down onto the line under its viaduct)
            let (q, hb) = lb.at(sb);
            let level = (q.z - p.z).abs() < 1.0;
            if inside && level && d > 0.8 && d < 5.5 && wrap_deg(hb - h).abs() < 30.0 {
                along += step;
            }
        }
        along >= 8.0 - 1e-3
    }

    /// Where on lane `b`, beside `a` (see `parallel`), the car at distance `s` along `a` is.
    /// Even lanes that start together can have very different lengths after a fork: mapping
    /// by their length ratio would aim a lane change far ahead on the longer branch.
    pub fn beside_s(&self, a: usize, b: usize, s: f32) -> f32 {
        let (la, lb) = (&self.lanes[a], &self.lanes[b]);
        if (lb.start() - la.start()).truncate().length() < 8.0 {
            if let Some((at, _)) = lb.nearest_point(la.at(s.clamp(0.0, la.length())).0) {
                return at;
            }
        }
        (s + self.beside_delta(a, b)).clamp(0.0, lb.length())
    }

    /// How much further along `b` than along `a` a point beside both lies (see `parallel`):
    /// 0 for lanes that start together, the distance `b` starts before `a` (positive) or
    /// after it (negative) otherwise.
    pub fn beside_delta(&self, a: usize, b: usize) -> f32 {
        let (la, lb) = (&self.lanes[a], &self.lanes[b]);
        if (lb.start() - la.start()).truncate().length() < 8.0 {
            return 0.0;
        }
        match la.nearest_point(lb.start()) {
            Some((sa, _)) if sa > 0.5 => -sa,
            _ => lb.nearest_point(la.start()).map(|(sb, _)| sb).unwrap_or(0.0),
        }
    }

    /// Point on a lane sequence closest to `p`: (index into `route`, distance along that lane).
    pub fn project_on_route(&self, route: &[usize], p: DVec3) -> Option<(usize, f32)> {
        self.project_on_route_lateral(route, p).map(|(ri, s, _)| (ri, s))
    }

    /// Like `project_on_route`, plus the lateral offset of `p` from the lane (m, right of
    /// the driving direction positive).
    pub fn project_on_route_lateral(&self, route: &[usize], p: DVec3) -> Option<(usize, f32, f32)> {
        let mut best: Option<(usize, f32, f64, f32)> = None;
        for (ri, &li) in route.iter().enumerate() {
            let l = &self.lanes[li];
            for k in 0..l.points.len().saturating_sub(1) {
                let a = l.points[k];
                let b = l.points[k + 1];
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
                let q = a + ab * t;
                let d = (q - p).truncate().length();
                if best.map(|b| d < b.2).unwrap_or(true) {
                    let dir = ab.truncate().normalize_or_zero();
                    let rel = (p - q).truncate();
                    // right vector of (dir.x, dir.y) is (dir.y, -dir.x)
                    let lateral = (rel.x * dir.y - rel.y * dir.x) as f32;
                    best = Some((ri, l.dist[k] + (l.dist[k + 1] - l.dist[k]) * t as f32, d, lateral));
                }
            }
        }
        best.map(|(ri, s, _, lat)| (ri, s, lat))
    }

    /// Where a bus stop at `p` lies on `route`: like [`Network::project_on_route_lateral`],
    /// but among the points within `reach` (when given) and not before route index
    /// `from`, a lane with the stop on its kerb side (right, or left with left-hand traffic)
    /// goes before a nearer one with the stop across it - a route back along the same
    /// street passes each stop twice, once from the other side, and the bus stopped at the
    /// stop across the road on its way out. Falls back to the nearest point.
    pub fn project_stop_on_route(&self, route: &[usize], p: DVec3, reach: Option<f64>, from: usize) -> Option<(usize, f32, f32)> {
        self.project_stop_on_route_side(route, p, reach, from, 0)
    }

    /// The same for a platform opposite the usual kerb (1), or either side (2).
    /// The side is the stop object's OMSI value, relative to the map's traffic hand.
    pub fn project_stop_on_route_side(&self, route: &[usize], p: DVec3, reach: Option<f64>, from: usize, side: u8) -> Option<(usize, f32, f32)> {
        // (index, s, distance, lateral) of the best on the platform side, and of any
        let mut kerb: Option<(usize, f32, f64, f32)> = None;
        let mut any: Option<(usize, f32, f64, f32)> = None;
        for (ri, &li) in route.iter().enumerate().skip(from.min(route.len())) {
            let l = &self.lanes[li];
            for k in 0..l.points.len().saturating_sub(1) {
                let a = l.points[k];
                let b = l.points[k + 1];
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
                let q = a + ab * t;
                let d = (q - p).truncate().length();
                if reach.is_some_and(|r| d > r) {
                    continue;
                }
                let dir = ab.truncate().normalize_or_zero();
                let rel = (p - q).truncate();
                let lateral = (rel.x * dir.y - rel.y * dir.x) as f32;
                let cand = (ri, l.dist[k] + (l.dist[k + 1] - l.dist[k]) * t as f32, d, lateral);
                let left = self.left_hand != (side == 1);
                let kerb_side = side == 2 || if left { lateral < -0.3 } else { lateral > 0.3 };
                if kerb_side && kerb.map(|b| d < b.2).unwrap_or(true) {
                    kerb = Some(cand);
                }
                if any.map(|b| d < b.2).unwrap_or(true) {
                    any = Some(cand);
                }
            }
        }
        match kerb.or(any) {
            Some((ri, s, _, lat)) => Some((ri, s, lat)),
            None if from > 0 => self.project_stop_on_route_side(route, p, reach, 0, side),
            None => None,
        }
    }

    /// Geometric conflicts between the lanes of each crossing object (paths that intersect
    /// or end at the same point, with where they meet), and the footpaths crossing its
    /// street lanes. AI cars sort out who goes first from these (`Network::must_yield`).
    pub fn compute_conflicts(&mut self) {
        self.conflicts = vec![Vec::new(); self.lanes.len()];
        self.crossings = vec![Vec::new(); self.lanes.len()];
        self.walks = vec![Vec::new(); self.lanes.len()];
        let (n, walks) = self.conflicts_from(0);
        log::info!("path network: {n} conflicting lane pairs at crossings, {walks} footpath crossings");
    }

    /// `conflicts`, `crossings` and `walks` of the junction objects whose lanes start at
    /// index `first` (all lanes of an object come with its tile, so a grown network only
    /// needs its new objects looked at). Returns the pairs and footpath crossings found.
    pub(super) fn conflicts_from(&mut self, first: usize) -> (usize, usize) {
        let len = self.lanes.len();
        self.conflicts.resize(len, Vec::new());
        self.crossings.resize(len, Vec::new());
        self.walks.resize(len, Vec::new());
        let mut by_object: HashMap<((i32, i32), i64), Vec<usize>> = HashMap::new();
        let mut walks_by_object: HashMap<((i32, i32), i64), Vec<usize>> = HashMap::new();
        for (i, l) in self.lanes.iter().enumerate().skip(first) {
            if l.source != 2 {
                continue;
            }
            let Some(k) = l.key else { continue };
            match l.kind {
                // a level crossing is a junction of a road and a railway
                LaneKind::Street | LaneKind::Rail => by_object.entry((k.tile, k.id)).or_default().push(i),
                LaneKind::Sidewalk => walks_by_object.entry((k.tile, k.id)).or_default().push(i),
                LaneKind::Air => {}
            }
        }
        let mut n = 0;
        for (key, lanes) in &by_object {
            for (x, &i) in lanes.iter().enumerate() {
                for &j in &lanes[x + 1..] {
                    let (a, b) = (&self.lanes[i], &self.lanes[j]);
                    let same_path = a.key.map(|k| k.path) == b.key.map(|k| k.path);
                    if same_path || (a.kind == LaneKind::Rail && b.kind == LaneKind::Rail) {
                        continue;
                    }
                    // one runs into the other: the same end, or ends a car's width apart that
                    // lead into the same lane (two turns converging on one exit)
                    let ends = (a.end() - b.end()).truncate().length();
                    let joint = ends < 1.5 || (ends < MEET_DIST && a.next.iter().any(|n| b.next.contains(n)));
                    let merge = joint && (a.start() - b.start()).truncate().length() > 3.0;
                    let place = if merge { Some((a.length(), b.length())) } else { polyline_crossing(a, b).or_else(|| near_pass(a, b)) };
                    // (`[blockpath]` adds no meeting place: it never had an effect in Omsi.exe,
                    // see `Lane::blocks`)
                    if let Some((sa, sb)) = place {
                        // (after a joint the two are in one lane, and follow each other)
                        let (mut ab, mut aa) = if merge { (meeting_extent(a, sa, b).0, 0.5) } else { meeting_extent(a, sa, b) };
                        let (mut bb, mut ba) = if merge { (meeting_extent(b, sb, a).0, 0.5) } else { meeting_extent(b, sb, a) };
                        // Keep the conflict envelope tied to the actual meeting geometry.
                        // Expanding a [crossingproblem] pair to the entire remaining lanes
                        // made every vehicle on a long approach hold crossing traffic until
                        // its tail left that whole lane, even when the paths only overlap
                        // near the junction. That produced unnecessary stops and queues.
                        self.conflicts[i].push(j);
                        self.conflicts[j].push(i);
                        self.crossings[i].push(Crossing { other: j, at: sa, other_at: sb, merge, before: ab, after: aa, other_before: bb, other_after: ba });
                        self.crossings[j].push(Crossing { other: i, at: sb, other_at: sa, merge, before: bb, after: ba, other_before: ab, other_after: aa });
                        n += 1;
                    }
                }
            }
            if let Some(walks) = walks_by_object.get(key) {
                for &i in lanes.iter().filter(|&&i| self.lanes[i].kind == LaneKind::Street) {
                    for &w in walks {
                        if let Some((sa, sw)) = polyline_crossing(&self.lanes[i], &self.lanes[w]) {
                            self.walks[i].push((w, sa, sw));
                        }
                    }
                }
            }
        }
        let walks: usize = self.walks[first..].iter().map(|w| w.len()).sum();
        let rings = rings::find_rings(self, first);
        if rings > 0 {
            log::info!("path network: {rings} roundabout rings");
        }
        (n, walks)
    }

    /// Do signs decide between lanes `a` and `b` (different `[rule] priority`, or a ring
    /// and its entry), rather than who came first or who turns across whom?
    pub fn signed(&self, a: usize, b: usize) -> bool {
        let (la, lb) = (&self.lanes[a], &self.lanes[b]);
        (la.priority - lb.priority).abs() > 0.5 || la.ring != lb.ring
    }

    /// Must a vehicle on lane `a` let one on lane `b` go first where they meet? The path with
    /// the higher `[rule] priority` goes first; between equals a turn across the oncoming
    /// traffic (left, or right when driving on the left) waits for it, and otherwise nobody
    /// has to: who comes first goes first, by the claims the cars make (`junction_stop`).
    /// That is Omsi.exe's rule (FUN_00716970: TrafficPriority, then a path reserved or taken
    /// already, then `[rule] priority`, then the vehicle going straight before the one
    /// turning left - no priority to the right). A right-before-left of our own made the
    /// main road's cars stop for every car out of a side road on their right where a map
    /// sets no priorities, the player's bus included (#2058, #1452). A train always goes
    /// first.
    pub fn must_yield(&self, a: usize, b: usize) -> bool {
        let (la, lb) = (&self.lanes[a], &self.lanes[b]);
        if la.kind == LaneKind::Rail {
            return false;
        }
        if lb.kind == LaneKind::Rail {
            return true;
        }
        if (la.priority - lb.priority).abs() > 0.5 {
            return la.priority < lb.priority;
        }
        // Where the map sets no priorities on a roundabout the ring goes before the entry
        // (StVO §8(1a), and how drivers everywhere take a ring). Omsi.exe has no ring rule:
        // there it is first come, first served unless the map gives the ring a higher
        // `[rule] priority`.
        if la.ring != lb.ring {
            return lb.ring;
        }
        let rel = wrap_deg(lb.start_heading() - la.start_heading());
        // (the turn across the oncoming traffic: left, or right when driving on the left)
        let across = if self.left_hand { 2 } else { 1 };
        if rel.abs() > 135.0 {
            // oncoming
            return la.turn == across && lb.turn != across;
        }
        // side by side from the same direction: the one turning across the other waits
        rel.abs() <= 30.0 && la.turn != 0 && lb.turn == 0
    }

    /// The lane beside `lane` at `s` that carries the traffic the other way (the other half
    /// of a two-way street): (lane, distance along it at the same place, how far its middle
    /// lies over to the oncoming side - the left, or the right on a left-hand-traffic map;
    /// `oncoming_sign` turns it into a lateral offset).
    pub fn opposite(&self, lane: usize, s: f32) -> Option<(usize, f32, f32)> {
        let l = self.lanes.get(lane)?;
        let (p, h) = l.at(s);
        let hr = (h as f64).to_radians();
        let left = DVec3::new(-hr.cos(), hr.sin(), 0.0);
        let left = if self.left_hand { -left } else { left };
        let cx = (p.x / GRID_CELL).floor() as i32;
        let cy = (p.y / GRID_CELL).floor() as i32;
        let mut best: Option<(usize, f32, f32)> = None;
        let mut seen: Vec<usize> = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &i in self.grid.get(&(cx + dx, cy + dy)).map(|v| v.as_slice()).unwrap_or(&[]) {
                    if i == lane || seen.contains(&i) {
                        continue;
                    }
                    seen.push(i);
                    let o = &self.lanes[i];
                    if o.kind != LaneKind::Street || o.no_cars {
                        continue;
                    }
                    let Some((os, d)) = o.nearest_point(p) else { continue };
                    if d > 7.0 || d < 1.5 {
                        continue;
                    }
                    let (q, oh) = o.at(os);
                    let side = (q - p).dot(left) as f32;
                    if side < 1.5 || wrap_deg(oh - h).abs() < 150.0 {
                        continue;
                    }
                    if best.map(|b| side < b.2).unwrap_or(true) {
                        best = Some((i, os, side));
                    }
                }
            }
        }
        best
    }

    /// The lanes traffic comes from towards distance `s` of `lane`, walked backwards over
    /// joints and through junctions for up to `within` metres: (lane, offset, the lane it
    /// leads into on the way to `lane`). A vehicle at distance `x` along such a lane is at
    /// `x + offset` in `lane`'s own distances (negative before its start); the first entry
    /// is `lane` itself with offset 0. Where several ways lead to one lane, the shortest
    /// counts. At most `max` lanes.
    pub fn upstream(&self, lane: usize, s: f32, within: f32, max: usize) -> Vec<(usize, f32, Option<usize>)> {
        let mut out: Vec<(usize, f32, Option<usize>)> = vec![(lane, 0.0, None)];
        if lane >= self.lanes.len() {
            return out;
        }
        // nearest first (a junction's lanes can be reached two ways)
        let mut done = vec![false];
        while let Some(k) = (0..out.len()).filter(|&k| !done[k]).max_by(|&a, &b| out[a].1.total_cmp(&out[b].1)) {
            done[k] = true;
            let (l, off, _) = out[k];
            // this lane starts `s - off` metres before the place: nothing before it is in reach
            if s - off > within {
                continue;
            }
            for &p in self.prev.get(l).map(|v| v.as_slice()).unwrap_or(&[]) {
                if p == lane || self.lanes[p].kind != self.lanes[lane].kind {
                    continue;
                }
                let o = off - self.lanes[p].length();
                match out.iter().position(|e| e.0 == p) {
                    Some(e) => {
                        if o > out[e].1 && !done[e] {
                            out[e] = (p, o, Some(l));
                        }
                    }
                    None => {
                        if out.len() < max {
                            out.push((p, o, Some(l)));
                            done.push(false);
                        }
                    }
                }
            }
        }
        out
    }

    /// Connect lane ends to lane starts that lie within `tol` metres with a compatible heading.
    pub fn link(&mut self, tol: f64) {
        self.by_key.clear();
        for (i, l) in self.lanes.iter().enumerate() {
            if let Some(k) = l.key {
                self.by_key.entry(k).or_default().push(i);
            }
        }
        self.build_grid();
        // neighbouring lanes: paths of one spline running the same way, next to each other
        let mut by_spline: HashMap<((i32, i32), i64, bool), Vec<usize>> = HashMap::new();
        for (i, l) in self.lanes.iter().enumerate() {
            if let Some(k) = l.key {
                if l.source == 1 && matches!(l.kind, LaneKind::Street) {
                    by_spline.entry((k.tile, k.id as i64, l.reversed)).or_default().push(i);
                }
            }
        }
        let mut neighbours = 0usize;
        for (_, mut list) in by_spline {
            list.sort_by(|a, b| self.lanes[*a].offset.total_cmp(&self.lanes[*b].offset));
            for w in list.windows(2) {
                let (a, b) = (w[0], w[1]);
                let gap = self.lanes[b].offset - self.lanes[a].offset;
                if gap > 1.5 && gap < 6.0 {
                    // b lies at the larger offset: to the right when driving along the spline
                    if neighbours == 0 {
                        let p = self.lanes[a].start();
                        log::info!("first neighbouring lane pair at ({:.1}, {:.1}), heading {:.0}", p.x, p.y, self.lanes[a].start_heading());
                    }
                    neighbours += 1;
                    let reversed = self.lanes[a].reversed;
                    if reversed {
                        self.lanes[a].left = Some(b);
                        self.lanes[b].right = Some(a);
                    } else {
                        self.lanes[a].right = Some(b);
                        self.lanes[b].left = Some(a);
                    }
                }
            }
        }
        log::info!("lanes: {} neighbouring lane pairs (lane changes)", neighbours);
        let cell = 4.0;
        let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, l) in self.lanes.iter().enumerate() {
            let s = l.start();
            grid.entry(((s.x / cell).floor() as i64, (s.y / cell).floor() as i64)).or_default().push(i);
        }
        let mut links = 0;
        for i in 0..self.lanes.len() {
            let e = self.lanes[i].end();
            let eh = self.lanes[i].end_heading();
            let kind = self.lanes[i].kind;
            let cx = (e.x / cell).floor() as i64;
            let cy = (e.y / cell).floor() as i64;
            let mut found = Vec::new();
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(list) = grid.get(&(cx + dx, cy + dy)) {
                        for &j in list {
                            if j == i {
                                continue;
                            }
                            let l = &self.lanes[j];
                            if l.kind != kind {
                                continue;
                            }
                            let d = (l.start() - e).truncate().length();
                            let dh = wrap_deg(l.start_heading() - eh).abs();
                            if d <= tol && dh < 40.0 && (l.start().z - e.z).abs() < 3.0 {
                                found.push(j);
                            }
                        }
                    }
                }
            }
            if found.is_empty() && omsi_cfg::flags::OMSI_DEBUG_UNLINKED.is_set() {
                // a lane end with a start of its kind near it that was not taken
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        for &j in grid.get(&(cx + dx, cy + dy)).map(|v| v.as_slice()).unwrap_or(&[]) {
                            let l = &self.lanes[j];
                            if j != i && l.kind == kind {
                                let d = (l.start() - e).truncate().length();
                                let dh = wrap_deg(l.start_heading() - eh).abs();
                                if d < 4.0 && dh < 40.0 {
                                    log::info!("unlinked: lane {i} {:?} end ({:.2}, {:.2}, {:.2}) -> lane {j} {:?} start {:.2} m away, dz {:.2}, dh {:.1}", self.lanes[i].key, e.x, e.y, e.z, l.key, d, l.start().z - e.z, dh);
                                }
                            }
                        }
                    }
                }
            }
            links += found.len();
            self.lanes[i].next = found;
        }
        log::info!("path network: {} lanes, {} links", self.lanes.len(), links);
        self.prev = vec![Vec::new(); self.lanes.len()];
        for i in 0..self.lanes.len() {
            for &n in &self.lanes[i].next {
                if n < self.lanes.len() {
                    self.prev[n].push(i);
                }
            }
        }
        self.compute_conflicts();
        self.compute_reach();
    }

    /// `reach` of every lane: its length plus the best reach of the lanes after it. The lanes
    /// are settled from the dead ends backwards (a lane once all the lanes after it are); a
    /// lane that never settles has a loop ahead of it and can be driven on for ever.
    pub fn compute_reach(&mut self) {
        let dead = self.update_reach();
        log::info!("path network: {dead} street lanes lead into a dead end within {DEAD_END} m");
    }

    /// `compute_reach` without the log line; returns the street lanes that lead into a dead
    /// end.
    pub(super) fn update_reach(&mut self) -> usize {
        let n = self.lanes.len();
        // a lane closed to cars is no way on for one that is open
        let counts = |i: usize, j: usize| !self.lanes[j].no_cars || self.lanes[i].no_cars;
        let mut waiting: Vec<usize> = (0..n).map(|i| self.lanes[i].next.iter().filter(|&&j| counts(i, j)).count()).collect();
        let mut reach = vec![REACH_MAX; n];
        let mut settled = vec![false; n];
        let mut ready: Vec<usize> = (0..n).filter(|&i| waiting[i] == 0).collect();
        while let Some(j) = ready.pop() {
            let l = &self.lanes[j];
            let best = l.next.iter().filter(|&&k| counts(j, k)).map(|&k| reach[k]).fold(0.0f32, f32::max);
            reach[j] = (l.length() + best).min(REACH_MAX);
            settled[j] = true;
            for &p in self.prev.get(j).map(|v| v.as_slice()).unwrap_or(&[]) {
                if counts(p, j) && !settled[p] {
                    waiting[p] = waiting[p].saturating_sub(1);
                    if waiting[p] == 0 {
                        ready.push(p);
                    }
                }
            }
        }
        let dead = self.lanes.iter().zip(&reach).filter(|(l, r)| l.kind == LaneKind::Street && **r < DEAD_END).count();
        self.reach = reach;
        dead
    }

    /// Nearest street lane and distance along it to a world point.
    /// Sort the lanes into the grid (called by `link`; call again after adding lanes).
    pub fn build_grid(&mut self) {
        self.grid.clear();
        self.start_grid.clear();
        for (i, l) in self.lanes.iter().enumerate() {
            self.start_grid.entry(Self::grid_cell(l.start())).or_default().push(i);
            let mut cells: Vec<(i32, i32)> = Vec::new();
            for w in l.points.windows(2) {
                // every cell along the segment, sampled finer than a cell
                let n = ((w[1] - w[0]).truncate().length() / (GRID_CELL * 0.5)).ceil().max(1.0) as usize;
                for k in 0..=n {
                    let q = w[0].lerp(w[1], k as f64 / n as f64);
                    let c = ((q.x / GRID_CELL).floor() as i32, (q.y / GRID_CELL).floor() as i32);
                    if !cells.contains(&c) {
                        cells.push(c);
                    }
                }
            }
            if l.points.len() == 1 {
                cells.push(((l.points[0].x / GRID_CELL).floor() as i32, (l.points[0].y / GRID_CELL).floor() as i32));
            }
            for c in cells {
                self.grid.entry(c).or_default().push(i);
            }
        }
    }

    /// Lane indices whose starts lie in cells intersecting a circle around `p`.
    /// Callers still apply their own exact distance and lane-kind tests. Sorting keeps
    /// their traversal (and seeded traffic selection) in map order.
    pub fn lanes_starting_near(&self, p: DVec3, radius: f64) -> Vec<usize> {
        let min_x = ((p.x - radius) / GRID_CELL).floor() as i32;
        let max_x = ((p.x + radius) / GRID_CELL).floor() as i32;
        let min_y = ((p.y - radius) / GRID_CELL).floor() as i32;
        let max_y = ((p.y + radius) / GRID_CELL).floor() as i32;
        let cells = (max_x as i64 - min_x as i64 + 1) * (max_y as i64 - min_y as i64 + 1);
        if self.start_grid.is_empty() || cells > self.start_grid.len() as i64 * 2 {
            return (0..self.lanes.len()).collect();
        }
        let mut out = Vec::new();
        for x in min_x..=max_x {
            for y in min_y..=max_y {
                if let Some(lanes) = self.start_grid.get(&(x, y)) {
                    out.extend_from_slice(lanes);
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// The nearest lane of `kind` to `p`: (lane, distance along it, distance to it). With
    /// the grid built, only lanes within about a cell of `p` are considered - enough for
    /// everything that asks where a vehicle or a person stands.
    pub fn nearest_lane(&self, p: DVec3, kind: LaneKind) -> Option<(usize, f32, f64)> {
        if !self.grid.is_empty() {
            if let Some(best) = self.nearest_lane_near(p, kind) {
                return Some(best);
            }
        }
        let mut best: Option<(usize, f32, f64)> = None;
        for (i, l) in self.lanes.iter().enumerate() {
            if l.kind != kind {
                continue;
            }
            for k in 0..l.points.len().saturating_sub(1) {
                let a = l.points[k];
                let b = l.points[k + 1];
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
                let q = a + ab * t;
                let d = (q - p).length();
                if best.map(|b| d < b.2).unwrap_or(true) {
                    let s = l.dist[k] + (l.dist[k + 1] - l.dist[k]) * t as f32;
                    best = Some((i, s, d));
                }
            }
        }
        best
    }

    /// Find shortest lane path from `start` to `target` using Dijkstra's algorithm.
    pub fn shortest_path(&self, start: usize, target: usize) -> Option<Vec<usize>> {
        if start >= self.lanes.len() || target >= self.lanes.len() {
            return None;
        }
        if start == target {
            return Some(vec![start]);
        }
        use std::cmp::Ordering;
        use std::collections::BinaryHeap;

        #[derive(Copy, Clone, PartialEq)]
        struct State {
            cost: f32,
            position: usize,
        }
        impl Eq for State {}
        impl Ord for State {
            fn cmp(&self, other: &Self) -> Ordering {
                other.cost.total_cmp(&self.cost)
            }
        }
        impl PartialOrd for State {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        let mut dist = vec![f32::INFINITY; self.lanes.len()];
        let mut parent = vec![None; self.lanes.len()];
        let mut heap = BinaryHeap::new();

        dist[start] = 0.0;
        heap.push(State { cost: 0.0, position: start });

        while let Some(State { cost, position }) = heap.pop() {
            if position == target {
                let mut path = Vec::new();
                let mut curr = Some(target);
                while let Some(node) = curr {
                    path.push(node);
                    curr = parent[node];
                }
                path.reverse();
                return Some(path);
            }

            if cost > dist[position] {
                continue;
            }

            for &next_idx in &self.lanes[position].next {
                if next_idx >= self.lanes.len() {
                    continue;
                }
                let next_len = self.lanes[next_idx].length();
                let next_cost = cost + next_len;
                if next_cost < dist[next_idx] {
                    dist[next_idx] = next_cost;
                    parent[next_idx] = Some(position);
                    heap.push(State { cost: next_cost, position: next_idx });
                }
            }
        }
        None
    }
}
