//! People on foot as a crowd: local avoidance between walkers and round vehicles, and
//! routes over a vehicle's cabin path network (`paths.cfg`).
//!
//! Avoidance is the anticipatory time-to-collision force of Karamouzas, Skinner and Guy
//! ("Universal power law governing pedestrian interactions", 2014): every walker looks at
//! when it would touch each neighbour if both kept their velocities and turns away from
//! that future contact the harder the sooner it comes. That is what makes two people give
//! way to each other a few metres early instead of bumping and sliding round each other
//! (which a plain repulsion does). A contact pass afterwards guarantees that nobody ends up
//! inside anybody else, and people standing still (waiting, queueing) are pushed less than
//! people walking - the one who walks goes round, the one who stands makes a little room.

use glam::{DVec2, Vec3};
use hashbrown::HashMap;

/// One person on foot in a plane (the ground, or the floor of one bus).
#[derive(Debug, Clone, Copy)]
pub struct Walker {
    pub pos: DVec2,
    pub vel: DVec2,
    pub radius: f64,
    /// Preferred velocity this frame (where the person wants to go, how fast).
    pub want: DVec2,
    /// How far the person yields to others and to contact (0 = not at all, 1 = fully).
    /// People standing in a queue or at a stop give little; walkers give fully.
    pub give: f64,
    /// Walkers only see walkers of the same space (0 = the ground, else one bus floor).
    pub space: u64,
    /// Seen by the others but not moved (a pedestrian held on a pavement lane, somebody
    /// seated, a person carried through a door by the bus).
    pub fixed: bool,
    /// Passes through other walkers for a moment: the way out of a deadlock in a doorway
    /// or a narrow aisle (two people who have pushed against each other for seconds).
    pub ghost: bool,
    /// Keeps to within `.2` of the segment `.0`-`.1` (an aisle): avoidance may move a walker
    /// sideways, never into the seats.
    pub corridor: Option<(DVec2, DVec2, f64)>,
}

impl Walker {
    pub fn new(pos: DVec2, radius: f64, space: u64) -> Walker {
        Walker {
            pos,
            vel: DVec2::ZERO,
            radius,
            want: DVec2::ZERO,
            give: 1.0,
            space,
            fixed: false,
            ghost: false,
            corridor: None,
        }
    }
}

/// A box nobody walks through (a vehicle), on the ground.
#[derive(Debug, Clone, Copy)]
pub struct Block {
    pub center: DVec2,
    /// Half extents: x across, y along the heading.
    pub half: DVec2,
    /// Radians, clockwise from north (like vehicle headings).
    pub heading: f64,
    /// Velocity of the vehicle (m/s): people keep out of the way of a moving bus earlier.
    pub vel: DVec2,
}

impl Block {
    fn axes(&self) -> (DVec2, DVec2) {
        let (s, c) = (self.heading.sin(), self.heading.cos());
        (DVec2::new(c, -s), DVec2::new(s, c))
    }

    /// Closest point of the box's outline to `p` and whether `p` lies inside it.
    pub fn closest(&self, p: DVec2) -> (DVec2, bool) {
        let (r, f) = self.axes();
        let d = p - self.center;
        let (x, y) = (d.dot(r), d.dot(f));
        let inside = x.abs() < self.half.x && y.abs() < self.half.y;
        if inside {
            // out through the nearest side
            let (dx, dy) = (self.half.x - x.abs(), self.half.y - y.abs());
            let q = if dx < dy {
                DVec2::new(self.half.x * sign(x), y)
            } else {
                DVec2::new(x, self.half.y * sign(y))
            };
            return (self.center + r * q.x + f * q.y, true);
        }
        let q = DVec2::new(
            x.clamp(-self.half.x, self.half.x),
            y.clamp(-self.half.y, self.half.y),
        );
        (self.center + r * q.x + f * q.y, false)
    }

    /// Whether `p` lies within `margin` of the box.
    pub fn near(&self, p: DVec2, margin: f64) -> bool {
        let (r, f) = self.axes();
        let d = p - self.center;
        d.dot(r).abs() < self.half.x + margin && d.dot(f).abs() < self.half.y + margin
    }
}

fn sign(v: f64) -> f64 {
    if v < 0.0 { -1.0 } else { 1.0 }
}

/// Tuning of the crowd model.
#[derive(Debug, Clone, Copy)]
pub struct CrowdParams {
    /// Scale of the anticipatory force.
    pub k: f64,
    /// Time horizon (s): collisions further away than about this matter little.
    pub tau0: f64,
    /// Relaxation time towards the preferred velocity (s).
    pub relax: f64,
    /// Largest acceleration (m/s²): nobody starts, stops or swerves like a robot.
    pub max_accel: f64,
    /// How far a walker looks for neighbours (m).
    pub sense: f64,
}

impl Default for CrowdParams {
    fn default() -> Self {
        CrowdParams {
            k: 1.5,
            tau0: 3.0,
            relax: 0.45,
            max_accel: 2.2,
            sense: 4.0,
        }
    }
}

impl CrowdParams {
    /// Inside a bus people are close by necessity: they look less far ahead and brush past
    /// each other instead of stopping a metre early.
    pub fn cabin() -> CrowdParams {
        CrowdParams {
            k: 0.6,
            tau0: 1.2,
            relax: 0.35,
            max_accel: 2.0,
            sense: 2.0,
        }
    }
}

const CELL: f64 = 2.0;

fn cell_of(space: u64, p: DVec2) -> (u64, i32, i32) {
    (
        space,
        (p.x / CELL).floor() as i32,
        (p.y / CELL).floor() as i32,
    )
}

/// Anticipatory avoidance force on a walker at `x` with velocity `v` from a body at
/// `xo` moving with `vo` (combined radius `r`). Zero when they would never touch.
fn ttc_force(p: &CrowdParams, x: DVec2, v: DVec2, xo: DVec2, vo: DVec2, r: f64) -> DVec2 {
    let w = xo - x;
    let dist = w.length();
    // already touching: count the contact at the current distance, so that the force
    // stays finite and still pushes apart
    let r = if dist < r { dist.max(1e-3) * 0.99 } else { r };
    let rv = v - vo;
    let a = rv.dot(rv);
    let b = w.dot(rv);
    let c = w.dot(w) - r * r;
    let discr = b * b - a * c;
    if discr <= 0.0 || a.abs() < 1e-6 {
        return DVec2::ZERO;
    }
    let sq = discr.sqrt();
    let t = (b - sq) / a;
    if !(t > 0.0) || t > p.tau0 * 3.0 {
        return DVec2::ZERO;
    }
    let m = 2.0;
    let f = -p.k * (-t / p.tau0).exp() * (rv - (rv * b - w * a) / sq) / (a * t.powf(m))
        * (m / t + 1.0 / p.tau0);
    let len = f.length();
    if len > 12.0 { f * (12.0 / len) } else { f }
}

/// Advance all walkers by `dt`: velocities follow the wishes with avoidance, positions
/// integrate, contacts are resolved. `blocks` only concern the ground (space 0).
pub fn step(walkers: &mut [Walker], blocks: &[Block], params: &CrowdParams, dt: f64) {
    if dt <= 0.0 || walkers.is_empty() {
        return;
    }
    let mut grid: HashMap<(u64, i32, i32), Vec<usize>> = HashMap::new();
    for (i, w) in walkers.iter().enumerate() {
        grid.entry(cell_of(w.space, w.pos)).or_default().push(i);
    }
    let reach = (params.sense / CELL).ceil() as i32;
    let mut new_vel = vec![DVec2::ZERO; walkers.len()];
    for (i, me) in walkers.iter().enumerate() {
        if me.fixed {
            continue;
        }
        let mut force = (me.want - me.vel) / params.relax;
        let mut avoid = DVec2::ZERO;
        let (sp, cx, cy) = cell_of(me.space, me.pos);
        if !me.ghost {
            for gy in cy - reach..=cy + reach {
                for gx in cx - reach..=cx + reach {
                    let Some(list) = grid.get(&(sp, gx, gy)) else {
                        continue;
                    };
                    for &j in list {
                        if j == i {
                            continue;
                        }
                        let o = &walkers[j];
                        let d = o.pos - me.pos;
                        if d.length_squared() > params.sense * params.sense {
                            continue;
                        }
                        // nobody minds the people behind them
                        if d.dot(me.want) < 0.0 && d.length() > me.radius + o.radius + 0.1 {
                            continue;
                        }
                        avoid +=
                            ttc_force(params, me.pos, me.vel, o.pos, o.vel, me.radius + o.radius);
                    }
                }
            }
        }
        if me.space == 0 {
            for b in blocks {
                if !b.near(me.pos, params.sense) {
                    continue;
                }
                let (q, inside) = b.closest(me.pos);
                if inside {
                    continue;
                }
                avoid += ttc_force(params, me.pos, me.vel, q, b.vel, me.radius + 0.05);
            }
        }
        // Straight at somebody the force only brakes, and two people stop nose to nose:
        // a walker then steps to the right, as people do.
        let wl = me.want.length();
        let al = avoid.length();
        if wl > 0.1 && al > 1e-3 && avoid.dot(me.want) < -0.9 * wl * al {
            avoid += DVec2::new(me.want.y, -me.want.x) / wl * al * 0.5;
        }
        // somebody who stands still only makes a little room
        force += avoid * me.give.clamp(0.0, 1.0);
        let len = force.length();
        if len > params.max_accel {
            force *= params.max_accel / len;
        }
        let mut v = me.vel + force * dt;
        // a walker never goes much faster than it wants to; a standing person only shuffles
        let cap = (me.want.length() * 1.2).max(0.3);
        let vl = v.length();
        if vl > cap {
            v *= cap / vl;
        }
        new_vel[i] = v;
    }
    for (i, w) in walkers.iter_mut().enumerate() {
        if w.fixed {
            continue;
        }
        w.vel = new_vel[i];
        w.pos += w.vel * dt;
        if let Some((a, b, dev)) = w.corridor {
            w.pos = ease_into_corridor(w.pos, a, b, dev, dt);
        }
    }
    // contacts: nobody inside anybody else or inside a vehicle
    for _ in 0..2 {
        for i in 0..walkers.len() {
            let (sp, cx, cy) = cell_of(walkers[i].space, walkers[i].pos);
            for gy in cy - 1..=cy + 1 {
                for gx in cx - 1..=cx + 1 {
                    let Some(list) = grid.get(&(sp, gx, gy)) else {
                        continue;
                    };
                    for &j in list {
                        if j <= i {
                            continue;
                        }
                        let (a, b) = (walkers[i], walkers[j]);
                        if a.ghost || b.ghost || a.space != b.space {
                            continue;
                        }
                        let d = b.pos - a.pos;
                        let dist = d.length();
                        let min = a.radius + b.radius;
                        if dist >= min {
                            continue;
                        }
                        let ga = if a.fixed { 0.0 } else { a.give.max(0.05) };
                        let gb = if b.fixed { 0.0 } else { b.give.max(0.05) };
                        if ga + gb <= 0.0 {
                            continue;
                        }
                        let n = if dist > 1e-6 {
                            d / dist
                        } else {
                            DVec2::new(((i * 7 + j * 3) % 5) as f64 - 2.0, 1.0).normalize()
                        };
                        let push = min - dist;
                        walkers[i].pos -= n * push * ga / (ga + gb);
                        walkers[j].pos += n * push * gb / (ga + gb);
                    }
                }
            }
        }
        for w in walkers.iter_mut() {
            if w.fixed || w.space != 0 {
                continue;
            }
            for b in blocks {
                if !b.near(w.pos, w.radius + 0.1) {
                    continue;
                }
                let (q, inside) = b.closest(w.pos);
                let d = w.pos - q;
                let dist = d.length();
                if inside {
                    let n = if dist > 1e-6 { -d / dist } else { DVec2::X };
                    w.pos = q + n * w.radius;
                } else if dist < w.radius {
                    let n = if dist > 1e-6 { d / dist } else { DVec2::X };
                    w.pos = q + n * w.radius;
                }
            }
        }
    }
    for w in walkers.iter_mut() {
        if let (false, Some((a, b, dev))) = (w.fixed, w.corridor) {
            w.pos = ease_into_corridor(w.pos, a, b, dev, dt);
        }
    }
}

/// [`clamp_to_corridor`] a little at a time: a walker outside the corridor is brought back
/// at up to 0.6 m/s. Clamped outright, a stroller jumped 5-6 cm in one frame wherever one
/// pavement path took over from the next (their lines do not meet exactly) - the "micro
/// teleports" - and whenever a corridor changed under somebody standing off its line.
pub fn ease_into_corridor(p: DVec2, a: DVec2, b: DVec2, max_dev: f64, dt: f64) -> DVec2 {
    let q = clamp_to_corridor(p, a, b, max_dev);
    let d = q - p;
    let step = 0.6 * dt.max(0.0) + 0.002;
    if d.length() <= step {
        q
    } else {
        p + d * (step / d.length())
    }
}

/// Turn `from` towards `to` (degrees) at most `rate` degrees per second, easing in at the
/// end so that a turn does not stop with a jolt.
pub fn turn_towards(from: f64, to: f64, rate: f64, dt: f64) -> f64 {
    let diff = angle_diff(from, to);
    // proportional near the target (time constant 0.25 s), capped at `rate`
    let step = (diff * (dt / 0.25).min(1.0)).clamp(-rate * dt, rate * dt);
    let out = from + step;
    out.rem_euclid(360.0)
}

/// Signed difference `to - from` in degrees, the short way round (-180..180).
pub fn angle_diff(from: f64, to: f64) -> f64 {
    let mut diff = (to - from) % 360.0;
    if diff > 180.0 {
        diff -= 360.0;
    } else if diff < -180.0 {
        diff += 360.0;
    }
    diff
}

/// Heading (degrees clockwise from north) of a ground direction.
pub fn heading_of(d: DVec2) -> f64 {
    d.x.atan2(d.y).to_degrees()
}

/// Nearest point to `p` on the segment `a`-`b` and the parameter along it (0..1).
pub fn project_on_segment(p: DVec2, a: DVec2, b: DVec2) -> (DVec2, f64) {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
    (a + ab * t, t)
}

/// Keep `p` within `max_dev` of the segment `a`-`b` (a walker in a narrow aisle).
pub fn clamp_to_corridor(p: DVec2, a: DVec2, b: DVec2, max_dev: f64) -> DVec2 {
    let (q, _) = project_on_segment(p, a, b);
    let d = p - q;
    let len = d.length();
    if len > max_dev {
        q + d * (max_dev / len)
    } else {
        p
    }
}

/// The walking network of a vehicle's cabin: `[pathpnt]`s and `[pathlink]`s.
#[derive(Debug, Clone, Default)]
pub struct PathGraph {
    pub points: Vec<Vec3>,
    adj: Vec<Vec<(usize, f32)>>,
}

impl PathGraph {
    /// `links`: (a, b, one-way) as in `paths.cfg`.
    pub fn new(points: Vec<Vec3>, links: &[(i32, i32, bool)]) -> PathGraph {
        let n = points.len();
        let mut adj = vec![Vec::new(); n];
        for &(a, b, oneway) in links {
            if a < 0 || b < 0 {
                continue;
            }
            let (a, b) = (a as usize, b as usize);
            if a >= n || b >= n || a == b {
                continue;
            }
            let d = (points[a] - points[b]).length();
            adj[a].push((b, d));
            if !oneway {
                adj[b].push((a, d));
            }
        }
        PathGraph { points, adj }
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Path point nearest `p` (height counts three times: the upper deck is not "near").
    pub fn nearest(&self, p: Vec3) -> Option<usize> {
        let d = |q: Vec3| {
            let v = q - p;
            v.x * v.x + v.y * v.y + (v.z * 3.0) * (v.z * 3.0)
        };
        self.points
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.adj[*i].is_empty())
            .min_by(|a, b| d(*a.1).total_cmp(&d(*b.1)))
            .map(|(i, _)| i)
    }

    /// Shortest walk from path point `a` to path point `b` (point indices, `a` first).
    pub fn route_points(&self, a: usize, b: usize) -> Option<Vec<usize>> {
        let n = self.points.len();
        if a >= n || b >= n {
            return None;
        }
        let mut dist = vec![f32::INFINITY; n];
        let mut prev = vec![usize::MAX; n];
        let mut done = vec![false; n];
        dist[a] = 0.0;
        loop {
            let mut u = usize::MAX;
            let mut best = f32::INFINITY;
            for i in 0..n {
                if !done[i] && dist[i] < best {
                    best = dist[i];
                    u = i;
                }
            }
            if u == usize::MAX || u == b {
                break;
            }
            done[u] = true;
            for &(v, w) in &self.adj[u] {
                if dist[u] + w < dist[v] {
                    dist[v] = dist[u] + w;
                    prev[v] = u;
                }
            }
        }
        if !dist[b].is_finite() {
            return None;
        }
        let mut out = vec![b];
        let mut x = b;
        while x != a {
            x = prev[x];
            out.push(x);
        }
        out.reverse();
        Some(out)
    }

    /// Walking distance from path point `a` to every path point (infinite when unreachable).
    pub fn distances_from(&self, a: usize) -> Vec<f32> {
        let n = self.points.len();
        let mut dist = vec![f32::INFINITY; n];
        if a >= n {
            return dist;
        }
        let mut done = vec![false; n];
        dist[a] = 0.0;
        loop {
            let mut u = usize::MAX;
            let mut best = f32::INFINITY;
            for i in 0..n {
                if !done[i] && dist[i] < best {
                    best = dist[i];
                    u = i;
                }
            }
            if u == usize::MAX {
                break;
            }
            done[u] = true;
            for &(v, w) in &self.adj[u] {
                if dist[u] + w < dist[v] {
                    dist[v] = dist[u] + w;
                }
            }
        }
        dist
    }

    /// Path points linked to `i` (either way).
    pub fn neighbours(&self, i: usize) -> Vec<usize> {
        let mut out: Vec<usize> = self
            .adj
            .get(i)
            .map(|a| a.iter().map(|x| x.0).collect())
            .unwrap_or_default();
        for (j, a) in self.adj.iter().enumerate() {
            if a.iter().any(|x| x.0 == i) && !out.contains(&j) {
                out.push(j);
            }
        }
        out
    }

    /// Length of the shortest walk between two path points (infinite when unconnected).
    pub fn distance(&self, a: usize, b: usize) -> f32 {
        match self.route_points(a, b) {
            Some(pts) => pts
                .windows(2)
                .map(|w| (self.points[w[0]] - self.points[w[1]]).length())
                .sum(),
            None => f32::INFINITY,
        }
    }

    /// Walk from `from` to `to` (bus frame): onto the network at the point nearest `from`,
    /// along it to the point nearest `to`, then to `to`. Points the walker is already
    /// standing on or has half passed are left out. A cabin without a network is crossed
    /// in a straight line.
    pub fn route(&self, from: Vec3, to: Vec3) -> Vec<Vec3> {
        // Onto the network where it passes closest - on a link, not at its nearest point.
        // The nearest *point* of a seat was often the one of the row behind (or across a
        // partition), and the walk to it went diagonally through the seat backs and the
        // wall; the nearest link is the aisle beside the seat.
        if let (Some(l1), Some(l2)) = (self.nearest_link(from), self.nearest_link(to)) {
            if let Some(r) = self.route_links(from, l1, l2, to) {
                return r;
            }
        }
        let (Some(a), Some(b)) = (self.nearest(from), self.nearest(to)) else {
            return vec![to];
        };
        self.route_via(from, a, b, to)
    }

    /// The link passing closest to `p` (height counts three times, as in `nearest`): its
    /// two points and the closest point on it.
    fn nearest_link(&self, p: Vec3) -> Option<(usize, usize, Vec3)> {
        let mut best: Option<(f32, usize, usize, Vec3)> = None;
        for (a, adj) in self.adj.iter().enumerate() {
            for &(b, _) in adj {
                let (pa, pb) = (self.points[a], self.points[b]);
                let ab = pb - pa;
                let len2 = ab.length_squared();
                let t = if len2 > 1e-6 {
                    ((p - pa).dot(ab) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let q = pa + ab * t;
                let v = q - p;
                let d = v.x * v.x + v.y * v.y + (v.z * 3.0) * (v.z * 3.0);
                if best.map(|b| d < b.0).unwrap_or(true) {
                    best = Some((d, a, b, q));
                }
            }
        }
        best.map(|(_, a, b, q)| (a, b, q))
    }

    /// The walk from `from` onto link `l1`, along the network and off link `l2` to `to`
    /// (None when the two links are not connected).
    fn route_links(
        &self,
        from: Vec3,
        l1: (usize, usize, Vec3),
        l2: (usize, usize, Vec3),
        to: Vec3,
    ) -> Option<Vec<Vec3>> {
        let (a1, b1, q1) = l1;
        let (a2, b2, q2) = l2;
        let mut out: Vec<Vec3> = Vec::new();
        let push = |out: &mut Vec<Vec3>, p: Vec3| {
            let last = out.last().copied().unwrap_or(from);
            if (p - last).length() > 0.05 {
                out.push(p);
            }
        };
        // (stepping onto the network only from off it: a walker on the link goes on)
        if (q1 - from).truncate().length() > 0.25 {
            push(&mut out, q1);
        }
        let same = (a1 == a2 && b1 == b2) || (a1 == b2 && b1 == a2);
        if !same {
            let mut best: Option<(f32, Vec<usize>)> = None;
            for e1 in [a1, b1] {
                for e2 in [a2, b2] {
                    let Some(pts) = self.route_points(e1, e2) else {
                        continue;
                    };
                    let along: f32 = pts
                        .windows(2)
                        .map(|w| (self.points[w[0]] - self.points[w[1]]).length())
                        .sum();
                    let total =
                        (self.points[e1] - q1).length() + along + (self.points[e2] - q2).length();
                    if best.as_ref().map(|b| total < b.0).unwrap_or(true) {
                        best = Some((total, pts));
                    }
                }
            }
            let (_, pts) = best?;
            for i in pts {
                push(&mut out, self.points[i]);
            }
        }
        if (q2 - to).truncate().length() > 0.25 {
            push(&mut out, q2);
        }
        push(&mut out, to);
        if out.is_empty() {
            out.push(to);
        }
        Some(out)
    }

    /// Like `route`, between known path points.
    pub fn route_via(&self, from: Vec3, a: usize, b: usize, to: Vec3) -> Vec<Vec3> {
        let mut out: Vec<Vec3> = match self.route_points(a, b) {
            Some(pts) => pts.into_iter().map(|i| self.points[i]).collect(),
            None => Vec::new(),
        };
        while !out.is_empty() && (out[0] - from).truncate().length() < 0.05 {
            out.remove(0);
        }
        // the walker already stands on the first leg: go straight for the second point
        if out.len() >= 2 {
            let (p0, p1) = (out[0], out[1]);
            let (_, t) = project_on_segment(
                from.truncate().as_dvec2(),
                p0.truncate().as_dvec2(),
                p1.truncate().as_dvec2(),
            );
            let q = p0 + (p1 - p0) * t as f32;
            if t > 0.0 && t < 1.0 && (q - from).truncate().length() < 0.3 {
                out.remove(0);
            }
        }
        if out
            .last()
            .map(|l| (*l - to).length() > 0.05)
            .unwrap_or(true)
        {
            out.push(to);
        }
        out
    }

    /// Length of a route starting at `from`.
    pub fn length(from: Vec3, route: &[Vec3]) -> f32 {
        let mut last = from;
        let mut total = 0.0;
        for p in route {
            total += (*p - last).truncate().length();
            last = *p;
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walker(x: f64, y: f64, want: DVec2) -> Walker {
        Walker {
            pos: DVec2::new(x, y),
            vel: want,
            radius: 0.25,
            want,
            give: 1.0,
            space: 0,
            fixed: false,
            ghost: false,
            corridor: None,
        }
    }

    #[test]
    fn head_on_walkers_pass_without_touching() {
        let mut w = vec![
            walker(0.0, -5.0, DVec2::new(0.0, 1.3)),
            walker(0.05, 5.0, DVec2::new(0.0, -1.3)),
        ];
        let p = CrowdParams::default();
        let mut closest = f64::MAX;
        for _ in 0..300 {
            w[0].want = DVec2::new(0.0, 1.3);
            w[1].want = DVec2::new(0.0, -1.3);
            step(&mut w, &[], &p, 1.0 / 30.0);
            closest = closest.min((w[0].pos - w[1].pos).length());
        }
        assert!(closest >= 0.5 - 1e-6, "closest approach {closest}");
        // both got past each other and kept going
        assert!(
            w[0].pos.y > 3.0 && w[1].pos.y < -3.0,
            "{:?} {:?}",
            w[0].pos,
            w[1].pos
        );
        // and they did not swerve wildly
        assert!(w[0].pos.x.abs() < 1.5 && w[1].pos.x.abs() < 1.5);
    }

    #[test]
    fn walker_goes_round_a_standing_person() {
        let mut w = vec![
            walker(0.0, -4.0, DVec2::new(0.0, 1.2)),
            Walker {
                give: 0.3,
                ..walker(0.0, 0.0, DVec2::ZERO)
            },
        ];
        let p = CrowdParams::default();
        let mut closest = f64::MAX;
        for _ in 0..300 {
            w[0].want = DVec2::new(0.0, 1.2);
            w[1].want = -w[1].pos * 1.5; // keeps to its spot
            step(&mut w, &[], &p, 1.0 / 30.0);
            closest = closest.min((w[0].pos - w[1].pos).length());
        }
        assert!(closest >= 0.5 - 1e-6);
        assert!(w[0].pos.y > 3.0);
        assert!(
            w[1].pos.length() < 0.4,
            "the standing person drifted to {:?}",
            w[1].pos
        );
    }

    #[test]
    fn crowd_never_interpenetrates() {
        // twelve people walking to the same point from a ring
        let mut w: Vec<Walker> = (0..12)
            .map(|k| {
                let a = k as f64 / 12.0 * std::f64::consts::TAU;
                walker(a.cos() * 5.0, a.sin() * 5.0, DVec2::ZERO)
            })
            .collect();
        let p = CrowdParams::default();
        for _ in 0..600 {
            for x in w.iter_mut() {
                let d = -x.pos;
                x.want = if d.length() > 0.1 {
                    d.normalize() * 1.2
                } else {
                    DVec2::ZERO
                };
            }
            step(&mut w, &[], &p, 1.0 / 30.0);
        }
        for i in 0..w.len() {
            for j in i + 1..w.len() {
                let d = (w[i].pos - w[j].pos).length();
                assert!(d > 0.45, "{i} and {j} overlap: {d}");
            }
        }
    }

    #[test]
    fn velocity_changes_smoothly() {
        // a walker told to reverse at once does not flip its velocity in one frame
        let mut w = vec![walker(0.0, 0.0, DVec2::new(0.0, 1.3))];
        let p = CrowdParams::default();
        w[0].want = DVec2::new(0.0, -1.3);
        let dt = 1.0 / 30.0;
        let before = w[0].vel;
        step(&mut w, &[], &p, dt);
        assert!(
            (w[0].vel - before).length() <= p.max_accel * dt + 1e-9,
            "{:?}",
            w[0].vel
        );
    }

    #[test]
    fn nobody_walks_through_a_bus() {
        let bus = Block {
            center: DVec2::ZERO,
            half: DVec2::new(1.25, 6.0),
            heading: 0.0,
            vel: DVec2::ZERO,
        };
        let mut w = vec![walker(-4.0, 0.3, DVec2::new(1.3, 0.0))];
        let p = CrowdParams::default();
        for _ in 0..400 {
            let d = DVec2::new(4.0, 0.0) - w[0].pos;
            w[0].want = if d.length() > 0.1 {
                d.normalize() * 1.3
            } else {
                DVec2::ZERO
            };
            step(&mut w, &[bus], &p, 1.0 / 30.0);
            let (_, inside) = bus.closest(w[0].pos);
            assert!(!inside, "inside the bus at {:?}", w[0].pos);
        }
    }

    #[test]
    fn ghosts_pass_through_each_other() {
        // two people pressed against each other in a doorway: as ghosts they get past
        let mut w = vec![
            Walker {
                ghost: true,
                ..walker(0.0, -0.2, DVec2::new(0.0, 1.0))
            },
            Walker {
                ghost: true,
                ..walker(0.0, 0.2, DVec2::new(0.0, -1.0))
            },
        ];
        let p = CrowdParams::cabin();
        for _ in 0..60 {
            w[0].want = DVec2::new(0.0, 1.0);
            w[1].want = DVec2::new(0.0, -1.0);
            step(&mut w, &[], &p, 1.0 / 30.0);
        }
        assert!(
            w[0].pos.y > 1.0 && w[1].pos.y < -1.0,
            "{:?} {:?}",
            w[0].pos,
            w[1].pos
        );
    }

    #[test]
    fn corridor_clamp_keeps_people_in_the_aisle() {
        let p = clamp_to_corridor(
            DVec2::new(1.0, 2.0),
            DVec2::new(0.0, 0.0),
            DVec2::new(0.0, 5.0),
            0.3,
        );
        assert!((p - DVec2::new(0.3, 2.0)).length() < 1e-9, "{p:?}");
        let q = clamp_to_corridor(
            DVec2::new(0.1, 2.0),
            DVec2::new(0.0, 0.0),
            DVec2::new(0.0, 5.0),
            0.3,
        );
        assert!((q - DVec2::new(0.1, 2.0)).length() < 1e-9);
    }

    #[test]
    fn cabin_route_follows_one_way_links() {
        // 0-1-2 two-way, 2->3 one-way, 3->0 one-way
        let pts = vec![
            Vec3::new(0.0, 0.0, 0.5),
            Vec3::new(0.0, -2.0, 0.5),
            Vec3::new(0.0, -4.0, 0.5),
            Vec3::new(0.0, -4.0, 2.5),
        ];
        let g = PathGraph::new(
            pts,
            &[(0, 1, false), (1, 2, false), (2, 3, true), (3, 0, true)],
        );
        assert_eq!(g.route_points(0, 3), Some(vec![0, 1, 2, 3]));
        assert_eq!(g.route_points(3, 2), Some(vec![3, 0, 1, 2]));
        let r = g.route(Vec3::new(0.0, 0.1, 0.5), Vec3::new(0.5, -2.0, 0.9));
        assert_eq!(r.last().copied(), Some(Vec3::new(0.5, -2.0, 0.9)));
        assert!(r.contains(&Vec3::new(0.0, -2.0, 0.5)));
        assert!((g.distance(0, 2) - 4.0).abs() < 1e-5);
        assert!(g.distance(0, 3) > 5.9 && g.distance(0, 3) < 6.1);
    }

    #[test]
    fn heading_turns_smoothly_the_short_way() {
        let mut h = 350.0;
        for _ in 0..30 {
            h = turn_towards(h, 20.0, 180.0, 1.0 / 30.0);
        }
        assert!((h - 20.0).abs() < 3.0 || (h - 20.0).abs() > 357.0, "{h}");
        let h1 = turn_towards(0.0, 180.0, 90.0, 0.1);
        assert!((h1 - 9.0).abs() < 1e-6 || (h1 - 351.0).abs() < 1e-6, "{h1}");
        assert!((angle_diff(350.0, 10.0) - 20.0).abs() < 1e-9);
        assert!((heading_of(DVec2::new(1.0, 0.0)) - 90.0).abs() < 1e-9);
    }
}
