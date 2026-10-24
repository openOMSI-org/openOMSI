//! One lane of the network: its kind, its points and the curve through them, what may use it,
//! and the builders that lay lanes along arcs and polylines.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneKind {
    Street,
    Sidewalk,
    Rail,
    /// `[path]` type 3: flight paths of AI aircraft.
    Air,
}

impl LaneKind {
    pub fn from_code(c: i32) -> LaneKind {
        match c {
            1 => LaneKind::Sidewalk,
            2 => LaneKind::Rail,
            3 => LaneKind::Air,
            _ => LaneKind::Street,
        }
    }
}

/// Identity of a lane in map terms: the tile, the spline/object id and the `[path]` index.
/// Timetable tracks reference lanes this way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LaneKey {
    pub tile: (i32, i32),
    pub id: i64,
    pub path: u16,
}

/// A sampled lane: points with headings, in world coordinates.
#[derive(Debug, Clone)]
pub struct Lane {
    pub key: Option<LaneKey>,
    /// True for the backwards lane of a two-way path.
    pub reversed: bool,
    pub kind: LaneKind,
    pub width: f32,
    /// World positions along the lane (in travel direction).
    pub points: Vec<DVec3>,
    /// Heading (degrees, clockwise from north) at each point: the tangent of the spline or
    /// arc the lane was sampled from, not the direction of the chord to the next point.
    pub headings: Vec<f32>,
    /// Signed curvature at each point (1/m, positive = turning right).
    pub curvature: Vec<f32>,
    /// Cumulative distance at each point.
    pub dist: Vec<f32>,
    pub speed_limit_kmh: f32,
    /// Lanes reachable from the end of this one.
    pub next: Vec<usize>,
    /// Traffic light controlling entry into this lane: (object instance, light index).
    pub traffic_light: Option<(usize, usize)>,
    /// Turn indicator for AI: 0 none, 1 left, 2 right.
    pub turn: i32,
    /// Source for debugging.
    pub source: u32,
    /// Lateral offset of the spline path (m, positive = right of the spline direction).
    pub offset: f32,
    /// Spline/object file the lane came from (debugging).
    pub name: String,
    /// `[rule] trafficdensity`: how much of the road traffic uses this lane (0 = none), of
    /// any random traffic group.
    pub density: f32,
    /// `[rule] trafficdensity <value> <group>` of the path, the last per group: the group
    /// is the random traffic group's place in the map's `unsched_vehgroups.txt`. A group
    /// without one takes its default there (Berlin-Spandau's GDR cars only drive where
    /// the Falkensee paths ask for them).
    pub group_density: Vec<(u16, f32)>,
    /// `[rule] no_cars`: cars keep off this lane.
    pub no_cars: bool,
    /// `[rule] bus` / `[rule] trucks` on the path: it is open to the AI vehicles of
    /// `[ai_veh_type]` 2 / 3 (see [`Lane::allows`]). The rules are switches: the value
    /// after them is not read (Grundorf's 110 `trucks` rules all say 0).
    pub rule_bus: bool,
    pub rule_trucks: bool,
    /// `[rule] overtaking_prohib` on the path (a solid centre line; Omsi.exe keeps it as a
    /// switch, `TPathRule_Road`+0x18): nobody goes over to the oncoming half to pass a
    /// vehicle that will move on (see `TrafficSim::plan_pass`).
    pub no_overtaking: bool,
    /// The spline this lane belongs to is editor-only: OMSI's invisible service roads at
    /// the edge of a map, where AI traffic drives with no road drawn under it.
    pub invisible: bool,
    /// Parallel lanes of the same road in the same direction (for lane changes).
    pub left: Option<usize>,
    pub right: Option<usize>,
    /// `[rule] priority` (`TPRIPriority`): who goes first where two paths of a junction
    /// meet. The stock maps put 192 on the straight paths of the main road and 64 on the
    /// paths coming out of a side road and leave the rest alone, so an unmarked path sits
    /// in between at `DEFAULT_PRIORITY`.
    pub priority: f32,
    /// Other `[path]`s of its object named by `[blockpath]`. Kept as the map has them, but
    /// without effect: in Omsi.exe 2.3.004 the loader builds the entries (FUN_007a0e54,
    /// 0x7a2ba4) and the only reader (FUN_00710ee0, 0x71100f) walks a list of vehicles that
    /// nothing ever fills (`TPathGroup+0x14`, set to length 0 in its constructor
    /// FUN_00710e98 and never written again), so a blockpath never holds anybody in the
    /// original either. No stock map places an object that has one.
    pub blocks: Vec<u16>,
    /// `[crossingproblem]` (segment +0x1a2 in Omsi.exe): where this path and one crossing it
    /// meet, a vehicle holding either of them blocks the other for as long as it holds it,
    /// not only until it has passed the crossing point (the look-ahead FUN_00716970 skips its
    /// "already past the crossing" test for such a pair, 0x718013). Spandau sets it on T
    /// junctions whose paths run side by side rather than cleanly across each other.
    pub crossing_problem: bool,
    /// Part of a roundabout's ring (see `rings::find_rings`).
    pub ring: bool,
    /// What a rail lane's track is: tram or railway, third rail, tunnel (see `rail::track`).
    pub track: Option<crate::rail::TrackInfo>,
}

/// Priority of a path without a `[rule] priority`.
pub const DEFAULT_PRIORITY: f32 = 128.0;

/// How much of `unsched_vehgroups.txt` group `pool`'s traffic a path carries: its `[rule]
/// trafficdensity` for the group (`rules`, see `Lane::group_density`: the last rule of a
/// group counts), else the group's default there (`defaults`): 0 none, for the first group 1
/// its medium density, for any other k that of the k-th group on the same path.
pub fn pool_density(rules: &[(u16, f32)], defaults: &[i32], pool: usize) -> f32 {
    let mut u = pool;
    // (a default naming another group that names this one again would go round for ever)
    for _ in 0..=defaults.len() {
        if let Some(&(_, v)) = rules.iter().rev().find(|(k, _)| *k as usize == u) {
            return v;
        }
        match defaults.get(u).copied().unwrap_or(0) {
            d if d <= 0 => return 0.0,
            _ if u == 0 => return 1.0,
            d => u = d as usize - 1,
        }
    }
    0.0
}

impl Lane {
    /// May an AI vehicle of `[ai_veh_type]` `veh_type` drive here (Omsi.exe 0x71d714, by
    /// the path's rules): a car (0) where there is no `no_cars`, a taxi (1) also where
    /// `bus` or `trucks` opens a no_cars path, a bus (2) only where `bus` and a truck (3)
    /// only where `trucks` is set. Other values (and the timetable's buses, -1) anywhere.
    pub fn allows(&self, veh_type: i32) -> bool {
        match veh_type {
            0 => !self.no_cars,
            1 => !self.no_cars || self.rule_bus || self.rule_trucks,
            2 => self.rule_bus,
            3 => self.rule_trucks,
            _ => true,
        }
    }

    /// How much of `unsched_vehgroups.txt` group `pool`'s traffic the lane carries (see
    /// [`pool_density`]).
    pub fn pool_density(&self, defaults: &[i32], pool: usize) -> f32 {
        pool_density(&self.group_density, defaults, pool)
    }

    /// Closest point of the lane's polyline to `p`: (distance along the lane, distance to it).
    pub fn nearest_point(&self, p: DVec3) -> Option<(f32, f64)> {
        let mut best: Option<(f32, f64)> = None;
        for k in 0..self.points.len().saturating_sub(1) {
            let a = self.points[k];
            let b = self.points[k + 1];
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
            let d = (a + ab * t - p).length();
            if best.map(|b| d < b.1).unwrap_or(true) {
                best = Some((self.dist[k] + (self.dist[k + 1] - self.dist[k]) * t as f32, d));
            }
        }
        best
    }

    pub fn length(&self) -> f32 {
        *self.dist.last().unwrap_or(&0.0)
    }

    /// A random road driver's forward choice across two short, reciprocally linked
    /// paths. A return whose entry and exit have already been passed folds the planned
    /// way back on itself. This checks the driving choice, not the network topology.
    pub(super) fn continues_from(&self, from: &Lane, tol: f64) -> bool {
        if self.kind != LaneKind::Street || self.length() as f64 > tol || from.length() as f64 > tol {
            return true;
        }
        let end = from.end();
        let backwards = (self.start() - end).truncate().dot(heading_dir(self.start_heading() as f64)) < 0.0
            && (self.end() - end).truncate().dot(heading_dir(self.end_heading() as f64)) <= 0.0;
        let forward_gap = (from.start() - self.end()).truncate().length_squared();
        let return_gap = (self.start() - from.end()).truncate().length_squared();
        !(backwards && forward_gap < return_gap)
    }

    /// Measure the lane again after its points were moved (an object's tilt).
    pub fn refresh(&mut self) {
        self.dist = cumulative(&self.points);
    }

    /// Segment and fraction along it at distance `s` (clamped to the lane).
    fn locate(&self, s: f32) -> (usize, f32) {
        // (a position gone NaN takes the lane's start rather than stopping the game)
        let s = if s.is_nan() { 0.0 } else { s.clamp(0.0, self.length()) };
        let i = match self.dist.binary_search_by(|d| d.total_cmp(&s)) {
            Ok(i) => i.min(self.points.len() - 2),
            Err(i) => i.saturating_sub(1).min(self.points.len() - 2),
        };
        let (d0, d1) = (self.dist[i], self.dist[i + 1]);
        (i, if d1 > d0 { (s - d0) / (d1 - d0) } else { 0.0 })
    }

    /// Position and heading at distance `s`. Between two samples the lane is a cubic
    /// Hermite curve whose tangents are the sampled headings, so a bend stays round
    /// instead of becoming a chain of chords: a car following the chords turned in small
    /// jerks at every sample and at every lane joint.
    pub fn at(&self, s: f32) -> (DVec3, f32) {
        if self.points.len() < 2 {
            return (self.points.first().copied().unwrap_or(DVec3::ZERO), self.headings.first().copied().unwrap_or(0.0));
        }
        let (i, t) = self.locate(s);
        let (h0, h1) = (self.headings[i], self.headings[i + 1]);
        (self.hermite(i, t as f64), h0 + wrap_deg(h1 - h0) * t)
    }

    fn hermite(&self, i: usize, t: f64) -> DVec3 {
        let (a, b) = (self.points[i], self.points[i + 1]);
        let lin = a.lerp(b, t);
        let chord = (b - a).truncate();
        let len = chord.length();
        if len < 1e-6 {
            return lin;
        }
        let d0 = heading_dir(self.headings[i] as f64);
        let d1 = heading_dir(self.headings[i + 1] as f64);
        // tangents far off the chord (a kink in hand-made samples) would make the curve loop
        let c = chord / len;
        if d0.dot(c) < 0.8 || d1.dot(c) < 0.8 {
            return lin;
        }
        let (t2, t3) = (t * t, t * t * t);
        let xy = a.truncate() * (2.0 * t3 - 3.0 * t2 + 1.0) + d0 * len * (t3 - 2.0 * t2 + t) + b.truncate() * (3.0 * t2 - 2.0 * t3) + d1 * len * (t3 - t2);
        DVec3::new(xy.x, xy.y, lin.z)
    }

    /// Position and heading at `s`, continued straight along the end tangents beyond either
    /// end of the lane.
    pub fn at_ext(&self, s: f32) -> (DVec3, f32) {
        if s < 0.0 {
            let h = self.start_heading();
            let d = heading_dir(h as f64) * s as f64;
            (self.start() + DVec3::new(d.x, d.y, 0.0), h)
        } else if s > self.length() {
            let h = self.end_heading();
            let d = heading_dir(h as f64) * (s - self.length()) as f64;
            (self.end() + DVec3::new(d.x, d.y, 0.0), h)
        } else {
            self.at(s)
        }
    }

    pub fn heading_ext(&self, s: f32) -> f32 {
        if s < 0.0 {
            self.start_heading()
        } else if s > self.length() {
            self.end_heading()
        } else if self.points.len() < 2 {
            self.headings.first().copied().unwrap_or(0.0)
        } else {
            let (i, t) = self.locate(s);
            let (h0, h1) = (self.headings[i], self.headings[i + 1]);
            h0 + wrap_deg(h1 - h0) * t
        }
    }

    /// Curvature at `s` (1/m, positive = right).
    pub fn curvature_at(&self, s: f32) -> f32 {
        if self.points.len() < 2 || self.curvature.len() != self.points.len() {
            return 0.0;
        }
        let (i, t) = self.locate(s);
        self.curvature[i] + (self.curvature[i + 1] - self.curvature[i]) * t
    }

    pub fn start(&self) -> DVec3 {
        self.points[0]
    }
    pub fn end(&self) -> DVec3 {
        *self.points.last().unwrap()
    }
    pub fn start_heading(&self) -> f32 {
        self.headings[0]
    }
    pub fn end_heading(&self) -> f32 {
        *self.headings.last().unwrap()
    }
}

/// Speed limit of a flight path without a `[rule] speedlimit` (km/h): none, the aircraft
/// flies at its own speed.
pub const AIR_NO_LIMIT_KMH: f32 = 1000.0;

/// Builds lanes from analytic descriptions.
pub struct LaneBuilder;

impl LaneBuilder {
    /// Sample an arc/straight lane: start position, heading (deg), length, radius (0 = straight,
    /// > 0 right turn), height change over the length.
    pub fn arc(start: DVec3, heading_deg: f64, length: f64, radius: f64, dz: f64, kind: LaneKind, width: f32) -> Lane {
        Self::sample_arc(start, heading_deg, length, radius, kind, width, |s| dz * s / length.max(1e-6))
    }

    /// Object-path gradients are rise per metre, not a total height change. Integrate
    /// the gradient between the ends so the lane meets the next path on a slope.
    pub fn arc_with_gradients(start: DVec3, heading_deg: f64, length: f64, radius: f64, grad_start: f64, grad_end: f64, kind: LaneKind, width: f32) -> Lane {
        Self::sample_arc(start, heading_deg, length, radius, kind, width, |s| {
            grad_start * s + (grad_end - grad_start) * s * s / (2.0 * length.max(1e-6))
        })
    }

    fn sample_arc(start: DVec3, heading_deg: f64, length: f64, radius: f64, kind: LaneKind, width: f32, height: impl Fn(f64) -> f64) -> Lane {
        let n = ((length / 2.0).ceil() as usize).clamp(1, 400);
        let mut points = Vec::with_capacity(n + 1);
        let mut headings = Vec::with_capacity(n + 1);
        for i in 0..=n {
            let s = length * i as f64 / n as f64;
            let (p, h) = arc_point(start, heading_deg, s, radius);
            points.push(DVec3::new(p.x, p.y, start.z + height(s)));
            headings.push(h as f32);
        }
        let k = if radius.abs() < 1e-6 { 0.0 } else { (1.0 / radius) as f32 };
        Self::curve(points, headings, vec![k; n + 1], kind, width)
    }

    /// Lane from sampled points with their tangent headings and curvatures.
    /// A flight path has no speed limit of its own unless the map gives it one with a
    /// `[rule] speedlimit`; the street default of 50 km/h had the Tegel approach flown at
    /// walking pace for an airliner.
    pub fn curve(points: Vec<DVec3>, headings: Vec<f32>, curvature: Vec<f32>, kind: LaneKind, width: f32) -> Lane {
        let dist = cumulative(&points);
        let speed_limit_kmh = if kind == LaneKind::Air { AIR_NO_LIMIT_KMH } else { 50.0 };
        Lane { key: None, reversed: false, kind, width, points, headings, curvature, dist, speed_limit_kmh, next: Vec::new(), traffic_light: None, turn: 0, source: 0, offset: 0.0, name: String::new(), invisible: false, density: 1.0, group_density: Vec::new(), no_cars: false, rule_bus: false, rule_trucks: false, no_overtaking: false, left: None, right: None, priority: DEFAULT_PRIORITY, blocks: Vec::new(), crossing_problem: false, ring: false, track: None }
    }

    /// Lane from bare points: headings from the neighbouring points on both sides (the
    /// tangent there, not the chord ahead), curvature from how the heading changes.
    pub fn polyline(points: Vec<DVec3>, kind: LaneKind, width: f32) -> Lane {
        let n = points.len();
        let mut headings = Vec::with_capacity(n);
        for i in 0..n {
            let (a, b) = match (i.checked_sub(1), (i + 1 < n).then_some(i + 1)) {
                (Some(p), Some(q)) => (points[p], points[q]),
                (None, Some(q)) => (points[i], points[q]),
                (Some(p), None) => (points[p], points[i]),
                (None, None) => (points[i], points[i] + DVec3::Y),
            };
            let d = b - a;
            headings.push((d.x.atan2(d.y)).to_degrees() as f32);
        }
        let dist = cumulative(&points);
        let curvature = (0..n)
            .map(|i| {
                let (p, q) = (i.saturating_sub(1), (i + 1).min(n.saturating_sub(1)));
                let ds = dist.get(q).copied().unwrap_or(0.0) - dist.get(p).copied().unwrap_or(0.0);
                if ds > 1e-3 { wrap_deg(headings[q] - headings[p]).to_radians() / ds } else { 0.0 }
            })
            .collect();
        Self::curve(points, headings, curvature, kind, width)
    }
}

/// Cumulative distance along a sequence of points.
fn cumulative(points: &[DVec3]) -> Vec<f32> {
    let mut acc = 0.0f32;
    points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i > 0 {
                acc += (*p - points[i - 1]).length() as f32;
            }
            acc
        })
        .collect()
}

/// An angle difference in degrees brought into -180..180.
pub fn wrap_deg(d: f32) -> f32 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

/// Unit vector (east, north) of a heading in degrees.
pub(super) fn heading_dir(h: f64) -> DVec2 {
    let r = h.to_radians();
    DVec2::new(r.sin(), r.cos())
}

/// Point and heading after travelling `s` metres on an arc starting at `start`.
pub fn arc_point(start: DVec3, heading_deg: f64, s: f64, radius: f64) -> (DVec2, f64) {
    let h = heading_deg.to_radians();
    let d = DVec2::new(h.sin(), h.cos());
    if radius.abs() < 1e-6 {
        return (start.truncate() + d * s, heading_deg);
    }
    let r = radius.abs();
    let turn = radius.signum();
    let right = DVec2::new(d.y, -d.x);
    let centre = start.truncate() + right * r * turn;
    let ang = -turn * s / r;
    let p = start.truncate() - centre;
    let (sa, ca) = ang.sin_cos();
    (centre + DVec2::new(p.x * ca - p.y * sa, p.x * sa + p.y * ca), heading_deg + turn * (s / r).to_degrees())
}
