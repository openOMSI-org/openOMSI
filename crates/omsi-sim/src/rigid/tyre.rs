//! What a tyre finds on the ground ahead of and beside it: the height to stand at, the
//! tread's steps and the faces it cannot climb.

use super::*;

/// A truck tyre's cornering stiffness per newton of load (1/rad): 8-10 for radial bus and
/// truck tyres (0.15 rad of slip angle peaks the force at 1.2 times the load x grip).
pub(super) const CORNERING: f32 = 14.0;
/// The tyre's relaxation length in tyre radii: about 0.5-1 m on a truck tyre.
pub(super) const RELAXATION: f32 = 0.9;

/// What a tyre finds along its length (see [`tyre_line`]).
pub(super) struct TyreLine {
    /// The world height the hub has to stand at, how fast that rises as the hub rolls
    /// forward (m per m) and where along the tyre (m ahead of the hub) the ground carries
    /// it. None when there is nothing below the tyre at all.
    pub(super) ground: Option<(f64, f32, f32)>,
    /// A face the tyre cannot climb ahead of the hub and one behind it, each as the last
    /// clear sample and the first one in the face (m ahead of the hub), when it is near
    /// enough to be touched within the substep.
    pub(super) faces: [Option<(f32, f32)>; 2],
    /// The samples either side of the hub are clear of faces: it is not sunk into a step.
    pub(super) clear: bool,
}

/// The ground under and around a tyre of radius `r` rolling along `fwd_h`.
///
/// The tyre is an envelope [`ENVELOPE`] radii long (a flattened circle) laid over the ground,
/// and the hub stands where the envelope first touches it: `need` = the highest ground
/// height plus the envelope's height over it. The ground is a polyline through samples
/// that stand still on the ground while the hub rolls past them (a lattice along the tyre's
/// direction), with an edge between two of them placed by halving. Anchored so, the
/// polyline is the same at every step, `need` changes smoothly as the hub rolls, and the
/// slope reported is exactly how fast it changes: the ground's own slope where the envelope
/// rests on a face of it, the envelope's where it slides over a corner. Only then does the
/// push that slope gives (see `step`) take from the body what lifting it over a step gains;
/// a slope from the samples that differed from it pumped energy into the body on steep
/// faces, and one from a comb carried with the hub read every grade over 6 % as a run of
/// kerbs.
///
/// A sample whose ground is higher than `z_top` (the most the tyre climbs) is no ground,
/// and within a radius, a sample and `look` metres of the hub it is a face the tyre runs
/// against if that face is lower than `wall_top`.
pub(super) fn tyre_line(probe: &dyn Fn(f64, f64, f64) -> GroundProbe, hub: DVec3, fwd_h: Vec3, r: f32, z_top: f64, wall_top: f64, look: f32) -> TyreLine {
    const N: usize = LATTICE;
    let a = r * ENVELOPE;
    let lift = |x: f32| r * (1.0 - (x / a).powi(2)).max(0.0).sqrt();
    // d need / d hub while the envelope slides over a corner x ahead of the hub
    let corner_slope = |x: f32| r * x / (a * a * (1.0 - (x / a).powi(2)).max(1e-3).sqrt());
    let spacing = 2.0 * a / (TYRE_SAMPLES - 1) as f32;
    let fwd = fwd_h.as_dvec3();
    let along = hub.x * fwd.x + hub.y * fwd.y;
    let first = ((along - a as f64) / spacing as f64).floor() - 2.0;
    // metres ahead of the hub of lattice point k: x(2) <= -a < x(3), x(N - 4) <= a < x(N - 3)
    let x_of = |k: usize| (first + k as f64) * spacing as f64 - along;
    let at = |dx: f64| {
        let p = hub + fwd * dx;
        probe(p.x, p.y, z_top)
    };
    let xs: [f32; N] = std::array::from_fn(|k| x_of(k) as f32);
    let mut below = [None; N];
    let mut wall = [false; N];
    for k in 0..N {
        let g = at(x_of(k));
        below[k] = g.below;
        wall[k] = xs[k].abs() <= r + spacing + look && g.above.map(|z| z < wall_top).unwrap_or(false);
    }
    let rise = |k: usize| match (below.get(k).copied().flatten(), below.get(k + 1).copied().flatten()) {
        (Some(g0), Some(g1)) => Some(g1 - g0),
        _ => None,
    };
    // An edge: a rise unlike those either side of it. A grade, however steep, rises alike
    // from sample to sample; a curve changes its rise by far less than EDGE over one
    // spacing. Every rise that decides the pieces over the envelope has both neighbours
    // (and they theirs), so a step is told the same way wherever the hub stands; told from
    // one neighbour, the level ground after a step at the end of the samples was an edge
    // too, the step no longer a lone one, and the hub jumped a centimetre as the lattice
    // moved on.
    let edge: [bool; N] = std::array::from_fn(|k| match (rise(k), k.checked_sub(1).and_then(rise), rise(k + 1)) {
        (Some(d), Some(l), Some(h)) => (d - l).abs() > EDGE && (d - h).abs() > EDGE,
        _ => false,
    });

    let mut best: Option<(f64, f32, f32)> = None;
    let mut offer = |need: f64, slope: f32, x: f32| {
        if best.map_or(true, |b| need > b.0) {
            best = Some((need, slope, x));
        }
    };
    // the highest the envelope has to stand over the piece of ground from (x0, z0) to (x1, z1)
    let mut piece = |x0: f32, z0: f64, x1: f32, z1: f64| {
        if x1 < -a || x0 > a {
            return;
        }
        if x1 - x0 < 1e-6 {
            offer(z0.max(z1) + lift(x0) as f64, corner_slope(x0), x0);
            return;
        }
        let m = ((z1 - z0) / (x1 - x0) as f64) as f32;
        let (lo, hi) = (x0.max(-a), x1.min(a));
        // where the envelope's own slope matches the ground's
        let t = m * a / r;
        let free = a * t / (1.0 + t * t).sqrt();
        let x = free.clamp(lo, hi);
        let need = z0 + (m * (x - x0)) as f64 + lift(x) as f64;
        // resting on the face, or at the end of the tyre (which rolls on with the hub), the
        // need rises with the ground; at a corner of the ground the envelope slides over it
        let slope = if x == free || x <= -a || x >= a { m } else { corner_slope(x) };
        offer(need, slope, x);
    };
    for k in 0..N {
        let Some(g0) = below[k] else { continue };
        let left = k > 0 && below[k - 1].is_some();
        let Some(g1) = below.get(k + 1).copied().flatten() else {
            if !left && xs[k].abs() <= a {
                piece(xs[k], g0, xs[k], g0);
            }
            continue;
        };
        let lone_edge = edge[k] && !(k > 0 && edge[k - 1]) && !edge.get(k + 1).copied().unwrap_or(false);
        if !lone_edge || xs[k + 1] < -a || xs[k] > a {
            // no step, one beyond the envelope, or a run of them: a steep face the samples
            // outline well enough
            piece(xs[k], g0, xs[k + 1], g1);
            continue;
        }
        // A step between the two. The ground either side runs on at its own grade (that of
        // the rise beyond it) up to where the step is: the point of the gap whose height is
        // nearer the far side's.
        let grade = |j: usize| rise(j).unwrap_or(0.0) / spacing as f64;
        let (m_lo, m_hi) = (grade(k - 1), grade(k + 1));
        let (x0, x1) = (x_of(k), x_of(k + 1));
        let near = |x: f64| g0 + m_lo * (x - x0);
        let far = |x: f64| g1 + m_hi * (x - x1);
        let (mut lo, mut hi) = (x0, x1);
        for _ in 0..EDGE_HALVINGS {
            let m = 0.5 * (lo + hi);
            match at(m).below {
                Some(z) if (z - far(m)).abs() < (z - near(m)).abs() => hi = m,
                Some(_) => lo = m,
                None => break,
            }
        }
        let c = 0.5 * (lo + hi);
        piece(xs[k], g0, c as f32, near(c));
        piece(c as f32, far(c), xs[k + 1], g1);
    }

    // the faces nearest the hub ahead and behind, bracketed by the clear sample before them
    let ahead = (0..N).find(|&k| xs[k] > 0.0 && wall[k]).filter(|&k| k > 0 && !wall[k - 1]).map(|k| (xs[k - 1], xs[k]));
    let behind = (0..N).rev().find(|&k| xs[k] <= 0.0 && wall[k]).filter(|&k| k + 1 < N && !wall[k + 1]).map(|k| (xs[k + 1], xs[k]));
    let clear = (0..N - 1).find(|&k| xs[k] <= 0.0 && xs[k + 1] > 0.0).map_or(true, |k| !wall[k] && !wall[k + 1]);
    TyreLine { ground: best, faces: [ahead, behind], clear }
}

/// How much higher (or lower) the hub stands than the ground under the tread's middle line
/// says, for a step running along under the tread (the tyre driving onto a kerb at a
/// shallow angle, or with one shoulder over its edge). The lengthwise envelope of
/// [`tyre_line`] sees only the middle line: the tyre stood wholly down or wholly up on the
/// kerb and jumped its full height as the edge passed under the hub. Here the tread's width
/// is looked at: with a share of it over the higher ground the hub rises by the step times
/// that share (all of it once half the tread is up, as a tyre carries the load on the part
/// that touches), and over lower ground at one shoulder it sinks likewise.
pub(super) fn tread_step(probe: &dyn Fn(f64, f64, f64) -> GroundProbe, hub: DVec3, fwd_h: Vec3, r: f32, z_top: f64) -> f64 {
    let w = (HALF_WIDTH * r) as f64;
    let across = DVec3::new(fwd_h.y as f64, -fwd_h.x as f64, 0.0);
    let g = |d: f64| {
        let p = hub + across * d;
        probe(p.x, p.y, z_top).below
    };
    let Some(gc) = g(0.0) else { return 0.0 };
    let mut best = 0.0f64;
    for side in [-1.0f64, 1.0] {
        let Some(gs) = g(side * w) else { continue };
        let h = gs - gc;
        if h.abs() < 0.02 {
            continue;
        }
        // where across the tread the ground changes: halving between the middle and the
        // shoulder
        let (mut lo, mut hi) = (0.0, w);
        for _ in 0..5 {
            let m = 0.5 * (lo + hi);
            match g(side * m) {
                Some(z) if (z - gs).abs() < (z - gc).abs() => hi = m,
                Some(_) => lo = m,
                None => break,
            }
        }
        let e = 0.5 * (lo + hi);
        // share of the tread on the other ground (from its edge to this shoulder)
        let share = ((w - e) / (2.0 * w)).clamp(0.0, 0.5);
        let t = share / 0.5;
        let k = t * t * (3.0 - 2.0 * t);
        // up: the shoulder over the kerb takes the load in proportion. (Down, with the
        // middle still on the step and a shoulder hanging over its edge, the hub stays: seen
        // from the other side once the edge passes the middle, it is the same share going
        // the other way, so the tyre comes down from the kerb as smoothly as it went up.)
        let delta = if h > 0.0 { h * k } else { 0.0 };
        if delta.abs() > best.abs() {
            best = delta;
        }
    }
    best
}

/// The faces a tyre is up against after this step: those it was against that are still
/// there and near, and new ones its samples (and a look to either side, `look` metres
/// further than the tyre reaches) found.
#[allow(clippy::too_many_arguments)]
pub(super) fn track_walls(probe: &dyn Fn(f64, f64, f64) -> GroundProbe, old: &[WallFace], line: &TyreLine, hub: DVec3, fwd_h: Vec3, r: f32, z_top: f64, wall_top: f64, look: f32) -> Vec<WallFace> {
    let is_wall = |p: DVec2| probe(p.x, p.y, z_top).above.map(|z| z < wall_top).unwrap_or(false);
    let hub2 = hub.truncate();
    // A face stays while the hub is near it (as near as the samples find one) and has not
    // gone through by a whole radius, and while the ground just behind the line is still
    // the face and the ground just before it still clear (the face ends, or the line
    // drifted off a curved one).
    let mut out: Vec<WallFace> = old
        .iter()
        .copied()
        .filter(|f| {
            let d = f.distance(hub);
            let n = f.normal.as_dvec2();
            let foot = hub2 - n * d as f64;
            d > -r && d < 2.5 * r && is_wall(foot - n * 0.05) && !is_wall(foot + n * 0.03)
        })
        .collect();
    let facing = |out: &[WallFace], dir: Vec2| out.iter().any(|f| f.normal.dot(-dir) > 0.5);
    let fwd2 = fwd_h.truncate().normalize_or(Vec2::Y);
    for (clear, wall) in line.faces.iter().flatten().copied() {
        let dir = fwd2 * wall.signum();
        if !facing(&out, dir) {
            let d = halve(&is_wall, hub2, dir, clear * wall.signum(), wall.abs());
            out.push(find_face(&is_wall, hub2, dir, d));
        }
    }
    if line.clear {
        // beside the tyre (a bus sliding or turning into a platform)
        let reach = SIDE_REACH * HALF_WIDTH * r + look;
        for dir in [fwd2.perp(), -fwd2.perp()] {
            if !facing(&out, dir) && is_wall(hub2 + (dir * reach).as_dvec2()) {
                let d = halve(&is_wall, hub2, dir, 0.0, reach);
                out.push(find_face(&is_wall, hub2, dir, d));
            }
        }
    }
    out
}

/// Where between `clear` and `wall` metres from `from` along `dir` a face begins.
fn halve(is_wall: &dyn Fn(DVec2) -> bool, from: DVec2, dir: Vec2, clear: f32, wall: f32) -> f32 {
    let (mut a, mut b) = (clear, wall);
    for _ in 0..FACE_HALVINGS {
        let m = 0.5 * (a + b);
        if is_wall(from + (dir * m).as_dvec2()) {
            b = m;
        } else {
            a = m;
        }
    }
    0.5 * (a + b)
}

/// A face found `d` metres from the hub along `dir`, with its direction taken from where the
/// lines [`FACE_SPAN`] either side of that one meet it (square to `dir` where they miss it).
fn find_face(is_wall: &dyn Fn(DVec2) -> bool, hub2: DVec2, dir: Vec2, d: f32) -> WallFace {
    let across = dir.perp();
    let point = hub2 + (dir * d).as_dvec2();
    let side = |s: f32| {
        let from = hub2 + (across * s * FACE_SPAN).as_dvec2();
        let (a, b) = (d - 2.0 * FACE_SPAN, d + 2.0 * FACE_SPAN);
        let hit = !is_wall(from + (dir * a).as_dvec2()) && is_wall(from + (dir * b).as_dvec2());
        hit.then(|| from + (dir * halve(is_wall, from, dir, a, b)).as_dvec2())
    };
    let tangent = match (side(-1.0), side(1.0)) {
        (Some(p), Some(q)) => q - p,
        (Some(p), None) => point - p,
        (None, Some(q)) => q - point,
        (None, None) => across.as_dvec2(),
    };
    let mut normal = tangent.perp().normalize_or_zero().as_vec2();
    if normal.dot(dir) > 0.0 {
        normal = -normal;
    }
    if normal.dot(dir) > -0.1 {
        // a face the lines graze is no face to push the tyre along
        normal = -dir;
    }
    WallFace { point, normal }
}
