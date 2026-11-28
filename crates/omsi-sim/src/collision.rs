//! Collision world: oriented boxes (scenery objects, parked and moving vehicles) tested
//! against a vehicle's bounding box - separating axes in the ground plane plus a height
//! range, which is what a bus body against walls, posts and cars needs. The contact query
//! gives the depth, the direction to push the vehicle out and where the two touch, for the
//! impulse response in [`crate::rigid`].

use glam::{DVec2, DVec3};
use hashbrown::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct Obb {
    pub center: DVec2,
    /// Half extents: x across, y along the heading.
    pub half: DVec2,
    /// Heading in radians (clockwise from north, like vehicles).
    pub heading: f64,
    pub z0: f64,
    pub z1: f64,
    /// Ground velocity (m/s) of a moving obstacle (an AI vehicle); zero for scenery.
    pub velocity: DVec2,
    /// Mass (kg) of a moving obstacle; zero for scenery, which does not give way.
    pub mass: f32,
    /// `[crashmode_pole]` (mass in t, break load in kN): a sign or a lamp post that
    /// breaks off at its foot when a vehicle runs into it, instead of standing like a wall.
    pub pole: Option<(f32, f32)>,
    /// Map object id of a placed obstacle; a moving one has `-2 - key` of its own key, and
    /// -1 is anything else.
    pub id: i64,
}

/// How far over the bottom of the height range two bodies share their bumpers meet (m).
const BUMPER: f64 = 0.5;

/// The height (world) at which a hit between two bodies sharing the heights `z0..z1` is
/// reported to the scripts as `coll_pos_z`: near the bottom of the range, where a bumper
/// or a frame meets the other body, not in its middle - the stock collision scripts of the
/// SD200, SD202 and NL202 only damage the engine and the drivetrain below 1.10 m.
pub fn impact_height(z0: f64, z1: f64) -> f64 {
    z0 + (0.5 * (z1 - z0)).clamp(0.0, BUMPER)
}

/// Where two boxes touch: push the tested box by `normal * depth` to separate them.
#[derive(Debug, Clone, Copy)]
pub struct Contact {
    /// Unit vector in the ground plane from the obstacle towards the tested box.
    pub normal: DVec2,
    pub depth: f64,
    /// Middle of the overlap in the ground plane.
    pub point: DVec2,
    /// Height range the two share.
    pub z0: f64,
    pub z1: f64,
}

impl Obb {
    /// From an OMSI `[boundingbox] w l h cx cy cz` in a frame at `pos` turned by `heading_deg`.
    pub fn from_box(bb: [f32; 6], pos: DVec3, heading_deg: f64) -> Obb {
        let h = heading_deg.to_radians();
        let (sh, ch) = (h.sin(), h.cos());
        let (cx, cy) = (bb[3] as f64, bb[4] as f64);
        // local (x right, y forward) → world for a clockwise heading
        let world = DVec2::new(pos.x + cx * ch + cy * sh, pos.y - cx * sh + cy * ch);
        Obb {
            center: world,
            half: DVec2::new(bb[0] as f64 * 0.5, bb[1] as f64 * 0.5),
            heading: h,
            z0: pos.z + bb[5] as f64 - bb[2] as f64 * 0.5,
            z1: pos.z + bb[5] as f64 + bb[2] as f64 * 0.5,
            velocity: DVec2::ZERO,
            mass: 0.0,
            pole: None,
            id: -1,
        }
    }

    /// A small upright box (the camera's probe).
    pub fn point(p: DVec3, half: f64) -> Obb {
        Obb {
            center: p.truncate(),
            half: DVec2::splat(half),
            heading: 0.0,
            z0: p.z - half,
            z1: p.z + half,
            velocity: DVec2::ZERO,
            mass: 0.0,
            pole: None,
            id: -1,
        }
    }

    /// A moving obstacle: its ground velocity (m/s), its mass (kg) and a key that stays the
    /// same from frame to frame (the AI car's id).
    pub fn moving(mut self, v: DVec2, mass: f32, key: u64) -> Obb {
        self.velocity = v;
        self.mass = mass;
        self.id = -2 - key as i64;
        self
    }

    pub fn axes(&self) -> [DVec2; 2] {
        let (s, c) = (self.heading.sin(), self.heading.cos());
        // "right" and "forward" of the box
        [DVec2::new(c, -s), DVec2::new(s, c)]
    }

    pub fn corners(&self) -> [DVec2; 4] {
        let [r, f] = self.axes();
        let (hx, hy) = (self.half.x, self.half.y);
        [
            self.center + r * hx + f * hy,
            self.center - r * hx + f * hy,
            self.center - r * hx - f * hy,
            self.center + r * hx - f * hy,
        ]
    }

    fn interval(&self, axis: DVec2) -> (f64, f64) {
        let [r, f] = self.axes();
        let c = self.center.dot(axis);
        let e = self.half.x * r.dot(axis).abs() + self.half.y * f.dot(axis).abs();
        (c - e, c + e)
    }

    /// Separating axis test in 2D and a height overlap.
    pub fn overlaps(&self, o: &Obb) -> bool {
        if self.z1 < o.z0 || o.z1 < self.z0 {
            return false;
        }
        for axis in self.axes().into_iter().chain(o.axes()) {
            let (min_a, max_a) = self.interval(axis);
            let (min_b, max_b) = o.interval(axis);
            if max_a < min_b || max_b < min_a {
                return false;
            }
        }
        true
    }

    /// How `self` touches the obstacle `o`, if it does. The push-out direction is the axis
    /// that needs the shortest way *out* - measured to whichever side is nearer, not by the
    /// width of the overlap, or a thin post in front of the bumper would be "separated"
    /// sideways by its own width and the bus would slip past it.
    pub fn contact(&self, o: &Obb) -> Option<Contact> {
        if self.z1 <= o.z0 || o.z1 <= self.z0 {
            return None;
        }
        let mut best: Option<(f64, DVec2)> = None;
        for axis in self.axes().into_iter().chain(o.axes()) {
            let (min_a, max_a) = self.interval(axis);
            let (min_b, max_b) = o.interval(axis);
            if max_a <= min_b || max_b <= min_a {
                return None;
            }
            // move self along +axis by (max_b - min_a), or along -axis by (max_a - min_b)
            let (d, n) = if max_b - min_a < max_a - min_b {
                (max_b - min_a, axis)
            } else {
                (max_a - min_b, -axis)
            };
            if best.map(|b| d < b.0).unwrap_or(true) {
                best = Some((d, n));
            }
        }
        let (depth, normal) = best?;
        // clipped around our own centre: products of map coordinates (four million metres
        // north on Spandau) drown a small overlap's area in rounding noise, and the contact
        // came out kilometres away
        let local = |c: [DVec2; 4]| c.map(|p| p - self.center);
        let point = centroid(&clip(&local(self.corners()), &local(o.corners())))
            .map(|p| p + self.center)
            .unwrap_or((self.center + o.center) * 0.5);
        Some(Contact {
            normal,
            depth,
            point,
            z0: self.z0.max(o.z0),
            z1: self.z1.min(o.z1),
        })
    }

    /// Separating axis test in plan view only.
    pub fn overlaps_plan(&self, o: &Obb) -> bool {
        self.separation(o) < 0.0
    }

    pub fn radius(&self) -> f64 {
        self.half.length()
    }

    /// A box on the ground from a vehicle's origin, heading (deg), the distances from the
    /// origin to its front and rear bumpers and its half width.
    pub fn vehicle(origin: DVec2, heading_deg: f64, front: f64, rear: f64, half_width: f64) -> Obb {
        let h = heading_deg.to_radians();
        let fwd = DVec2::new(h.sin(), h.cos());
        Obb {
            center: origin + fwd * ((front - rear) * 0.5),
            half: DVec2::new(half_width, (front + rear) * 0.5),
            heading: h,
            z0: f64::MIN,
            z1: f64::MAX,
            velocity: DVec2::ZERO,
            mass: 0.0,
            pole: None,
            id: -1,
        }
    }

    /// How far apart two boxes are in plan view (heights ignored): the widest gap along
    /// one of the four separating axes (m), or, when they overlap, minus the shallowest
    /// depth. A positive value is a lower bound of the true distance (exact where a face
    /// is nearest), which is what a clearance check wants.
    pub fn separation(&self, o: &Obb) -> f64 {
        let d = o.center - self.center;
        let (ra, fa) = (self.axes()[0], self.axes()[1]);
        let (rb, fb) = (o.axes()[0], o.axes()[1]);
        let extent = |r: DVec2, f: DVec2, half: DVec2, axis: DVec2| {
            r.dot(axis).abs() * half.x + f.dot(axis).abs() * half.y
        };
        let mut best = f64::MIN;
        for axis in [ra, fa, rb, fb] {
            let gap =
                d.dot(axis).abs() - extent(ra, fa, self.half, axis) - extent(rb, fb, o.half, axis);
            best = best.max(gap);
        }
        best
    }
}

/// A box in space: a vehicle's body as it is turned, pitched and banked.
#[derive(Debug, Clone, Copy)]
pub struct Box3 {
    pub center: DVec3,
    /// Unit axes (right, forward, up of the body).
    pub axes: [DVec3; 3],
    pub half: DVec3,
}

impl Box3 {
    /// The upright box of a plan-view box and its height range (None when the range is
    /// open, as a vehicle's outline for the traffic).
    pub fn from_obb(b: &Obb) -> Option<Box3> {
        if !(b.z1 - b.z0).is_finite() || b.z1 - b.z0 > 1e5 {
            return None;
        }
        let [r, f] = b.axes();
        Some(Box3 {
            center: DVec3::new(b.center.x, b.center.y, (b.z0 + b.z1) * 0.5),
            axes: [r.extend(0.0), f.extend(0.0), DVec3::Z],
            half: DVec3::new(b.half.x, b.half.y, (b.z1 - b.z0) * 0.5),
        })
    }

    /// Does the convex polygon (a face) pass through the box? Separating axes: the box's
    /// three, the face's normal and the cross products of their edges.
    pub fn meets(&self, poly: &[DVec3]) -> bool {
        // round the box's centre: products of map coordinates lose the centimetres
        let pts: Vec<DVec3> = poly.iter().map(|p| *p - self.center).collect();
        let n = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
        let separated = |axis: DVec3| {
            if axis.length_squared() < 1e-12 {
                return false;
            }
            let r = self.half.x * self.axes[0].dot(axis).abs()
                + self.half.y * self.axes[1].dot(axis).abs()
                + self.half.z * self.axes[2].dot(axis).abs();
            let (mut lo, mut hi) = (f64::MAX, f64::MIN);
            for p in &pts {
                let d = p.dot(axis);
                lo = lo.min(d);
                hi = hi.max(d);
            }
            lo > r || hi < -r
        };
        if self.axes.iter().any(|a| separated(*a)) || separated(n) {
            return false;
        }
        for i in 0..pts.len() {
            let e = pts[(i + 1) % pts.len()] - pts[i];
            if self.axes.iter().any(|a| separated(a.cross(e))) {
                return false;
            }
        }
        true
    }
}

/// Area centroid of a polygon (the vertex average when it has no area).
fn centroid(poly: &[DVec2]) -> Option<DVec2> {
    if poly.is_empty() {
        return None;
    }
    let (mut area, mut c) = (0.0, DVec2::ZERO);
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let a = p.perp_dot(q);
        area += a;
        c += (p + q) * a;
    }
    if area.abs() < 1e-6 {
        return Some(poly.iter().copied().sum::<DVec2>() / poly.len() as f64);
    }
    Some(c / (3.0 * area))
}

/// The part of the convex polygon `subject` inside the convex polygon `clipper`
/// (Sutherland-Hodgman); both wound the same way as [`Obb::corners`].
fn clip(subject: &[DVec2], clipper: &[DVec2]) -> Vec<DVec2> {
    let mut out: Vec<DVec2> = subject.to_vec();
    // the corners run clockwise seen from above: "inside" is to the right of each edge
    let n = clipper.len();
    let centre = clipper.iter().copied().sum::<DVec2>() / n as f64;
    for i in 0..n {
        let (a, b) = (clipper[i], clipper[(i + 1) % n]);
        let edge = b - a;
        let side = |p: DVec2| edge.perp_dot(p - a);
        let inside_sign = side(centre).signum();
        let input = std::mem::take(&mut out);
        if input.is_empty() {
            break;
        }
        for j in 0..input.len() {
            let (p, q) = (input[j], input[(j + 1) % input.len()]);
            let (sp, sq) = (side(p) * inside_sign, side(q) * inside_sign);
            if sp >= 0.0 {
                out.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                let t = sp / (sp - sq);
                out.push(p + (q - p) * t);
            }
        }
    }
    out
}

const CELL: f64 = 50.0;

/// Cell of a mesh shape's own grid (m).
const PART_CELL: f64 = 8.0;
/// A part no thinner than this (m): a wall's triangle has no depth of its own.
const PART_HALF_MIN: f64 = 0.05;
/// A sloping triangle whose box would hold more than this much air (m³ of footprint times
/// height) is cut into bands: a roof pitched over a 10 m house would otherwise be a solid
/// block from its eaves up to its ridge.
const PART_SLACK: f64 = 0.25;
/// How high a band of a sloping triangle is (m): a bus's floor clears a ramp it drives on
/// by a good deal more.
const PART_BAND: f64 = 0.1;

/// A `[collision_mesh]` in its object's own frame (x right, y forward, z up), as what OMSI
/// hands to ODE: a triangle mesh, not its extents. Each triangle becomes a thin box (a
/// wall's is a slab along it, a floor's a flat plate), so a vehicle meets the faces it
/// really runs into - one box around the whole mesh stood as a wall over every road
/// between the buildings of a housing estate, and around the whole Heerstraße bridge.
#[derive(Debug, Default)]
pub struct MeshShape {
    /// Boxes in the object's frame: `center` and `heading` relative to its origin and
    /// heading, `z0..z1` over its origin.
    pub parts: Vec<Obb>,
    /// The triangle each part stands for (the part's band of it, `z0..z1`, is the face).
    part_tri: Vec<u32>,
    tris: Vec<[glam::Vec3; 3]>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    /// Extents in the object's frame.
    pub lo: DVec3,
    pub hi: DVec3,
}

impl MeshShape {
    /// From triangles in the object's frame. Triangles whose top stays below `min_top` over
    /// the origin (a kerb, a floor at ground level) are no obstacle.
    pub fn from_triangles(tris: impl Iterator<Item = [DVec3; 3]>, min_top: f64) -> MeshShape {
        let mut shape = MeshShape {
            lo: DVec3::splat(f64::MAX),
            hi: DVec3::splat(f64::MIN),
            ..Default::default()
        };
        let mut seen: hashbrown::HashSet<[i64; 6]> = hashbrown::HashSet::new();
        for t in tris {
            let mut out = Vec::new();
            triangle_parts(t, &mut out);
            let ti = shape.tris.len() as u32;
            let mut used = false;
            for part in out {
                if part.z1 <= min_top {
                    continue;
                }
                // the two halves of a flat wall make the same slab
                let q = |v: f64| (v * 100.0).round() as i64;
                if !seen.insert([
                    q(part.center.x),
                    q(part.center.y),
                    q(part.half.x),
                    q(part.half.y),
                    q(part.z0),
                    q(part.z1),
                ]) {
                    continue;
                }
                shape.add(part);
                shape.part_tri.push(ti);
                used = true;
            }
            if used {
                shape.tris.push(t.map(|p| p.as_vec3()));
            }
        }
        shape
    }

    /// Part `i`'s piece of its triangle, in the object's frame.
    fn face(&self, i: usize) -> Vec<DVec3> {
        let p = &self.parts[i];
        let t = self.tris[self.part_tri[i] as usize].map(|v| v.as_dvec3());
        // a hair beyond the band, or the two bands either side of a vertex both lose it
        clip_z(&clip_z(&t, p.z0 - 1e-4, true), p.z1 + 1e-4, false)
    }

    fn add(&mut self, b: Obb) {
        let i = self.parts.len() as u32;
        self.parts.push(b);
        let r = b.radius();
        self.lo = self
            .lo
            .min(DVec3::new(b.center.x - r, b.center.y - r, b.z0));
        self.hi = self
            .hi
            .max(DVec3::new(b.center.x + r, b.center.y + r, b.z1));
        for y in ((b.center.y - r) / PART_CELL).floor() as i32
            ..=((b.center.y + r) / PART_CELL).floor() as i32
        {
            for x in ((b.center.x - r) / PART_CELL).floor() as i32
                ..=((b.center.x + r) / PART_CELL).floor() as i32
            {
                self.grid.entry((x, y)).or_default().push(i);
            }
        }
    }

    /// Indices of the parts within `r` of the local point `c` (each once).
    fn near(&self, c: DVec2, r: f64) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        let (x0, y0) = (
            ((c.x - r) / PART_CELL).floor() as i32,
            ((c.y - r) / PART_CELL).floor() as i32,
        );
        for y in y0..=((c.y + r) / PART_CELL).floor() as i32 {
            for x in x0..=((c.x + r) / PART_CELL).floor() as i32 {
                for &i in self.grid.get(&(x, y)).map(|v| v.as_slice()).unwrap_or(&[]) {
                    let p = &self.parts[i as usize];
                    let pr = p.radius();
                    // listed in every cell it covers: taken in the first of them met here
                    let first = (((p.center.x - pr) / PART_CELL).floor() as i32).max(x0) == x
                        && (((p.center.y - pr) / PART_CELL).floor() as i32).max(y0) == y;
                    if first && (p.center - c).length() <= r + pr {
                        out.push(i);
                    }
                }
            }
        }
        out
    }
}

/// The boxes standing in for one triangle: one when it is upright (a wall) or flat (a
/// floor, a roof), else a slab per band of its height, each laid along the band - a road
/// ramping up a bridge, cut into one box, stood 0.4 m over its own foot, where the bus
/// that drives up it runs into the box.
fn triangle_parts(t: [DVec3; 3], out: &mut Vec<Obb>) {
    let q = t.map(|p| p.truncate());
    let area = 0.5 * (q[1] - q[0]).perp_dot(q[2] - q[0]).abs();
    let (z0, z1) = (
        t[0].z.min(t[1].z).min(t[2].z),
        t[0].z.max(t[1].z).max(t[2].z),
    );
    let mut part = |poly: &[DVec3], along: DVec2| {
        let heading = if along.length() < 1e-6 {
            0.0
        } else {
            along.x.atan2(along.y)
        };
        let mut b = Obb {
            center: DVec2::ZERO,
            half: DVec2::ZERO,
            heading,
            z0: f64::MAX,
            z1: f64::MIN,
            velocity: DVec2::ZERO,
            mass: 0.0,
            pole: None,
            id: -1,
        };
        let [r, f] = b.axes();
        let (mut u0, mut u1, mut v0, mut v1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for p in poly {
            let p2 = p.truncate();
            u0 = u0.min(p2.dot(r));
            u1 = u1.max(p2.dot(r));
            v0 = v0.min(p2.dot(f));
            v1 = v1.max(p2.dot(f));
            b.z0 = b.z0.min(p.z);
            b.z1 = b.z1.max(p.z);
        }
        b.center = r * ((u0 + u1) * 0.5) + f * ((v0 + v1) * 0.5);
        b.half = DVec2::new(
            ((u1 - u0) * 0.5).max(PART_HALF_MIN),
            ((v1 - v0) * 0.5).max(PART_HALF_MIN),
        );
        out.push(b);
    };
    if area * (z1 - z0) <= PART_SLACK {
        // along the longest edge in plan view
        let edges = [q[1] - q[0], q[2] - q[1], q[0] - q[2]];
        let d = edges
            .into_iter()
            .max_by(|a, b| a.length_squared().total_cmp(&b.length_squared()))
            .unwrap();
        part(&t, d);
        return;
    }
    // bands between contour lines, laid along them
    let n = (t[1] - t[0]).cross(t[2] - t[0]);
    let contour = DVec2::new(-n.y, n.x);
    let bands = ((z1 - z0) / PART_BAND).ceil().clamp(1.0, 64.0) as usize;
    let dz = (z1 - z0) / bands as f64;
    for k in 0..bands {
        let (lo, hi) = (z0 + dz * k as f64, z0 + dz * (k + 1) as f64);
        let poly = clip_z(&clip_z(&t, lo, true), hi, false);
        if poly.len() >= 3 {
            part(&poly, contour);
        }
    }
}

/// The part of a convex polygon above (`keep_above`) or below the height `z`.
fn clip_z(poly: &[DVec3], z: f64, keep_above: bool) -> Vec<DVec3> {
    let inside = |p: &DVec3| if keep_above { p.z >= z } else { p.z <= z };
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        if inside(&p) {
            out.push(p);
        }
        if inside(&p) != inside(&q) {
            let t = (z - p.z) / (q.z - p.z);
            out.push(p + (q - p) * t);
        }
    }
    out
}

/// A placed object's collision mesh.
#[derive(Debug, Clone)]
pub struct MeshObstacle {
    pub shape: std::sync::Arc<MeshShape>,
    pub pos: DVec3,
    /// Radians, clockwise from north.
    pub heading: f64,
    pub id: i64,
    /// Around the whole mesh, in the world.
    pub bounds: Obb,
}

impl MeshObstacle {
    pub fn new(
        shape: std::sync::Arc<MeshShape>,
        pos: DVec3,
        heading_deg: f64,
        id: i64,
    ) -> MeshObstacle {
        let (lo, hi) = (shape.lo, shape.hi);
        let size = hi - lo;
        let c = (lo + hi) * 0.5;
        let mut bounds = Obb::from_box(
            [
                size.x as f32,
                size.y as f32,
                size.z as f32,
                c.x as f32,
                c.y as f32,
                c.z as f32,
            ],
            pos,
            heading_deg,
        );
        // the f32 of a box's centre offset is fine, not so its height over a map at 4000 km
        bounds.z0 = pos.z + lo.z;
        bounds.z1 = pos.z + hi.z;
        bounds.id = id;
        MeshObstacle {
            shape,
            pos,
            heading: heading_deg.to_radians(),
            id,
            bounds,
        }
    }

    /// Object frame → world.
    fn to_world(&self, p: &Obb) -> Obb {
        let (s, c) = self.heading.sin_cos();
        let l = p.center;
        Obb {
            center: DVec2::new(
                self.pos.x + l.x * c + l.y * s,
                self.pos.y - l.x * s + l.y * c,
            ),
            heading: p.heading + self.heading,
            z0: p.z0 + self.pos.z,
            z1: p.z1 + self.pos.z,
            id: self.id,
            ..*p
        }
    }

    /// World point → object frame (plan view).
    fn to_local(&self, p: DVec2) -> DVec2 {
        let (s, c) = self.heading.sin_cos();
        let d = p - self.pos.truncate();
        DVec2::new(d.x * c - d.y * s, d.x * s + d.y * c)
    }

    /// The parts that may touch `b`, in the world; with `solid`, only those whose face
    /// really passes through that box (a pitched bus's own, which the plan-view box with the
    /// height range of all its corners overstates on a ramp).
    pub fn parts_near<'a>(
        &'a self,
        b: &Obb,
        solid: Option<&'a Box3>,
    ) -> impl Iterator<Item = Obb> + 'a {
        let (z0, z1) = (b.z0 - self.pos.z, b.z1 - self.pos.z);
        let list = if self.bounds.overlaps_plan(b) {
            self.shape.near(self.to_local(b.center), b.radius())
        } else {
            Vec::new()
        };
        list.into_iter()
            .filter(move |&i| {
                let p = &self.shape.parts[i as usize];
                p.z1 > z0 && p.z0 < z1
            })
            .filter(move |&i| {
                let Some(solid) = solid else { return true };
                let (s, c) = self.heading.sin_cos();
                let face: Vec<DVec3> = self
                    .shape
                    .face(i as usize)
                    .into_iter()
                    .map(|l| {
                        DVec3::new(
                            self.pos.x + l.x * c + l.y * s,
                            self.pos.y - l.x * s + l.y * c,
                            self.pos.z + l.z,
                        )
                    })
                    .collect();
                face.len() >= 3 && solid.meets(&face)
            })
            .map(|i| self.to_world(&self.shape.parts[i as usize]))
    }
}

#[derive(Default)]
pub struct CollisionWorld {
    pub boxes: Vec<Obb>,
    grid: HashMap<(i32, i32), Vec<usize>>,
    /// Objects that collide with their `[collision_mesh]`.
    pub meshes: Vec<MeshObstacle>,
    mesh_grid: HashMap<(i32, i32), Vec<usize>>,
}

impl CollisionWorld {
    pub fn add(&mut self, b: Obb) {
        let i = self.boxes.len();
        self.boxes.push(b);
        let r = b.radius();
        let (x0, x1) = (
            ((b.center.x - r) / CELL).floor() as i32,
            ((b.center.x + r) / CELL).floor() as i32,
        );
        let (y0, y1) = (
            ((b.center.y - r) / CELL).floor() as i32,
            ((b.center.y + r) / CELL).floor() as i32,
        );
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.grid.entry((x, y)).or_default().push(i);
            }
        }
    }

    pub fn add_mesh(&mut self, m: MeshObstacle) {
        let i = self.meshes.len();
        let b = m.bounds;
        self.meshes.push(m);
        let r = b.radius();
        for y in ((b.center.y - r) / CELL).floor() as i32..=((b.center.y + r) / CELL).floor() as i32
        {
            for x in
                ((b.center.x - r) / CELL).floor() as i32..=((b.center.x + r) / CELL).floor() as i32
            {
                self.mesh_grid.entry((x, y)).or_default().push(i);
            }
        }
    }

    /// The meshes whose bounds may touch `b`.
    fn meshes_near(&self, b: &Obb) -> Vec<usize> {
        let r = b.radius();
        let mut out = Vec::new();
        for y in ((b.center.y - r) / CELL).floor() as i32..=((b.center.y + r) / CELL).floor() as i32
        {
            for x in
                ((b.center.x - r) / CELL).floor() as i32..=((b.center.x + r) / CELL).floor() as i32
            {
                for &i in self
                    .mesh_grid
                    .get(&(x, y))
                    .map(|v| v.as_slice())
                    .unwrap_or(&[])
                {
                    let o = &self.meshes[i].bounds;
                    if (o.center - b.center).length() <= r + o.radius() && !out.contains(&i) {
                        out.push(i);
                    }
                }
            }
        }
        out
    }

    /// Everything that may touch `b` as plain boxes in the world: the placed boxes and the
    /// parts of the collision meshes near it.
    pub fn obstacles_near(&self, b: &Obb) -> Vec<Obb> {
        self.obstacles_near_solid(b, Box3::from_obb(b).as_ref())
    }

    /// As [`Self::obstacles_near`], the collision meshes' faces only where they pass
    /// through `solid` (the vehicle's body as it is turned, pitched and banked).
    pub fn obstacles_near_solid(&self, b: &Obb, solid: Option<&Box3>) -> Vec<Obb> {
        let mut out: Vec<Obb> = self.near(b).into_iter().map(|i| self.boxes[i]).collect();
        for i in self.meshes_near(b) {
            out.extend(self.meshes[i].parts_near(b, solid));
        }
        out
    }

    /// How many boxes the collision meshes are made of.
    pub fn mesh_parts(&self) -> usize {
        self.meshes.iter().map(|m| m.shape.parts.len()).sum()
    }

    /// Does something building-sized (at least `min_half` m across on both axes and
    /// `min_height` m tall) stand between `a` and `b`? A box that holds `a` itself does not
    /// count (a camera in a depot hall or under a bridge).
    pub fn ray_blocked(&self, a: DVec3, b: DVec3, min_half: f64, min_height: f64) -> bool {
        self.ray_blocker(a, b, min_half, min_height).is_some()
    }

    /// The box `ray_blocked` finds in the way.
    pub fn ray_blocker(&self, a: DVec3, b: DVec3, min_half: f64, min_height: f64) -> Option<Obb> {
        let d = b - a;
        let len = d.truncate().length();
        let steps = (len / (CELL * 0.5)).ceil().max(1.0) as usize;
        let mut seen: Vec<usize> = Vec::new();
        for k in 0..=steps {
            let q = a + d * (k as f64 / steps as f64);
            let cell = ((q.x / CELL).floor() as i32, (q.y / CELL).floor() as i32);
            for &i in self.grid.get(&cell).map(|v| v.as_slice()).unwrap_or(&[]) {
                if seen.contains(&i) {
                    continue;
                }
                seen.push(i);
                let o = &self.boxes[i];
                if o.half.x < min_half || o.half.y < min_half || o.z1 - o.z0 < min_height {
                    continue;
                }
                let [r, f] = o.axes();
                let local = |p: DVec3| {
                    let rel = p.truncate() - o.center;
                    DVec2::new(rel.dot(r), rel.dot(f))
                };
                let (la, lb) = (local(a), local(b));
                if la.x.abs() <= o.half.x && la.y.abs() <= o.half.y {
                    continue;
                }
                // slab clip of the segment against the box in its own frame
                let (mut t0, mut t1) = (0.0f64, 1.0f64);
                let dl = lb - la;
                let mut hit = true;
                for (p, dd, h) in [(la.x, dl.x, o.half.x), (la.y, dl.y, o.half.y)] {
                    if dd.abs() < 1e-9 {
                        if p.abs() > h {
                            hit = false;
                            break;
                        }
                    } else {
                        let (u0, u1) = ((-h - p) / dd, (h - p) / dd);
                        t0 = t0.max(u0.min(u1));
                        t1 = t1.min(u0.max(u1));
                        if t0 > t1 {
                            hit = false;
                            break;
                        }
                    }
                }
                // and the line of sight passes through the box's height there (not over a
                // roof, not under a bridge deck)
                let (za, zb) = (a.z + d.z * t0, a.z + d.z * t1);
                if hit && za.min(zb) < o.z1 && za.max(zb) > o.z0 {
                    return Some(*o);
                }
            }
        }
        // a collision mesh counts by its size as a whole and blocks where the segment meets
        // one of its faces
        let span = Obb {
            center: (a.truncate() + b.truncate()) * 0.5,
            half: DVec2::new(0.1, len * 0.5 + 0.1),
            heading: d.x.atan2(d.y),
            z0: a.z.min(b.z),
            z1: a.z.max(b.z),
            velocity: DVec2::ZERO,
            mass: 0.0,
            pole: None,
            id: -1,
        };
        for i in self.meshes_near(&span) {
            let m = &self.meshes[i];
            let o = &m.bounds;
            if o.half.x < min_half || o.half.y < min_half || o.z1 - o.z0 < min_height {
                continue;
            }
            let steps = (len / PART_CELL).ceil().max(1.0) as usize;
            for k in 0..steps {
                let (p0, p1) = (
                    a + d * (k as f64 / steps as f64),
                    a + d * ((k + 1) as f64 / steps as f64),
                );
                let piece = Obb {
                    center: (p0.truncate() + p1.truncate()) * 0.5,
                    half: DVec2::new(0.05, (len / steps as f64) * 0.5 + 0.05),
                    heading: span.heading,
                    z0: p0.z.min(p1.z),
                    z1: p0.z.max(p1.z),
                    ..span
                };
                if let Some(p) = m
                    .parts_near(&piece, None)
                    .find(|p| segment_through(p0, p1, p))
                {
                    return Some(p);
                }
            }
        }
        None
    }

    /// Indices of the boxes that may touch `b` (a box listed in several cells once).
    pub fn near(&self, b: &Obb) -> Vec<usize> {
        let r = b.radius();
        let (x0, x1) = (
            ((b.center.x - r) / CELL).floor() as i32,
            ((b.center.x + r) / CELL).floor() as i32,
        );
        let (y0, y1) = (
            ((b.center.y - r) / CELL).floor() as i32,
            ((b.center.y + r) / CELL).floor() as i32,
        );
        let mut out = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                if let Some(list) = self.grid.get(&(x, y)) {
                    for &i in list {
                        let o = &self.boxes[i];
                        if (o.center - b.center).length() <= r + o.radius() && !out.contains(&i) {
                            out.push(i);
                        }
                    }
                }
            }
        }
        out
    }

    /// First box (or collision mesh face) overlapping `b`.
    pub fn hit(&self, b: &Obb) -> Option<Obb> {
        self.obstacles_near(b).into_iter().find(|o| o.overlaps(b))
    }
}

/// Does the segment `a`-`b` pass through the box (in plan view within its footprint, and
/// within its heights there)?
fn segment_through(a: DVec3, b: DVec3, o: &Obb) -> bool {
    let [r, f] = o.axes();
    let local = |p: DVec3| {
        let rel = p.truncate() - o.center;
        DVec2::new(rel.dot(r), rel.dot(f))
    };
    let (la, lb) = (local(a), local(b));
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    let dl = lb - la;
    for (p, dd, h) in [(la.x, dl.x, o.half.x), (la.y, dl.y, o.half.y)] {
        if dd.abs() < 1e-9 {
            if p.abs() > h {
                return false;
            }
        } else {
            let (u0, u1) = ((-h - p) / dd, (h - p) / dd);
            t0 = t0.max(u0.min(u1));
            t1 = t1.min(u0.max(u1));
            if t0 > t1 {
                return false;
            }
        }
    }
    let (za, zb) = (a.z + (b.z - a.z) * t0, a.z + (b.z - a.z) * t1);
    za.min(zb) < o.z1 && za.max(zb) > o.z0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bus_at(y: f64) -> Obb {
        // 2.5 m wide, 12 m long, heading north, centred at (0, y)
        Obb::from_box(
            [2.5, 12.0, 3.0, 0.0, 0.0, 1.5],
            DVec3::new(0.0, y, 0.0),
            0.0,
        )
    }

    #[test]
    fn a_post_ahead_pushes_the_bus_back_not_sideways() {
        let post = Obb::from_box(
            [0.1, 0.1, 3.0, 0.0, 0.0, 1.5],
            DVec3::new(0.3, 6.0, 0.0),
            0.0,
        );
        // the bumper is 0.25 m past the post's near face
        let bus = bus_at(0.2);
        let c = bus.contact(&post).expect("touching");
        assert!((c.normal - DVec2::new(0.0, -1.0)).length() < 1e-9, "{c:?}");
        assert!((c.depth - 0.25).abs() < 1e-9, "{c:?}");
        assert!((c.point - DVec2::new(0.3, 6.0)).length() < 1e-6, "{c:?}");
    }

    #[test]
    fn contact_point_far_from_the_map_origin() {
        let base = DVec3::new(892248.2, 4196461.4, 33.2);
        let post = Obb::from_box(
            [0.1, 0.1, 3.0, 0.0, 0.0, 1.5],
            base + DVec3::new(0.3, 6.0, 0.0),
            0.0,
        );
        let bus = Obb::from_box(
            [2.5, 12.0, 3.0, 0.0, 0.0, 1.5],
            base + DVec3::new(0.0, 0.2, 0.0),
            0.0,
        );
        let c = bus.contact(&post).expect("touching");
        assert!(
            (c.point - DVec2::new(base.x + 0.3, base.y + 6.0)).length() < 1e-3,
            "{c:?}"
        );
    }

    #[test]
    fn glancing_contact_with_a_turned_wall() {
        // a wall along the east side, turned 10° so it meets the bus's front right corner
        let wall = Obb::from_box(
            [1.0, 20.0, 3.0, 0.0, 0.0, 1.5],
            DVec3::new(2.0, 0.0, 0.0),
            -10.0,
        );
        let bus = bus_at(0.0);
        let c = bus.contact(&wall).expect("touching");
        // pushed out to the west, a little to the north (the wall's own normal)
        assert!(c.normal.x < -0.9 && c.depth > 0.0 && c.depth < 1.0, "{c:?}");
        // the overlap is at the front right of the bus
        assert!(c.point.x > 0.5 && c.point.y > 2.0, "{c:?}");
    }

    #[test]
    fn no_contact_under_a_high_or_low_box() {
        let sign = Obb::from_box(
            [2.0, 2.0, 1.0, 0.0, 0.0, 4.0],
            DVec3::new(0.0, 0.0, 0.0),
            0.0,
        );
        let low = Obb::from_box(
            [2.0, 2.0, 0.2, 0.0, 0.0, 0.1],
            DVec3::new(0.0, 0.0, 0.0),
            0.0,
        );
        let bus = Obb::from_box([2.5, 12.0, 3.0, 0.0, 0.0, 1.8], DVec3::ZERO, 0.0);
        assert!(bus.contact(&sign).is_none() && bus.contact(&low).is_none());
        assert!(bus.overlaps(&bus_at(1.0)));
    }
}

#[cfg(test)]
mod ray_tests {
    use super::*;

    fn square(x: f64, y: f64, heading_deg: f64) -> Obb {
        Obb {
            center: DVec2::new(x, y),
            half: DVec2::new(1.0, 1.0),
            heading: heading_deg.to_radians(),
            z0: 0.0,
            z1: 1.0,
            velocity: DVec2::ZERO,
            mass: 0.0,
            pole: None,
            id: -1,
        }
    }

    #[test]
    fn separation_of_boxes() {
        // side by side, a metre apart; overlapping by half a metre
        assert!((square(0.0, 0.0, 0.0).separation(&square(3.0, 0.0, 0.0)) - 1.0).abs() < 1e-9);
        assert!((square(0.0, 0.0, 0.0).separation(&square(1.5, 0.0, 0.0)) + 0.5).abs() < 1e-9);
        // a box turned by 45° points a corner at the other: the gap is from that corner
        let s = square(0.0, 0.0, 0.0).separation(&square(3.0, 0.0, 45.0));
        assert!((s - (2.0 - 2f64.sqrt())).abs() < 1e-9, "{s}");
        assert!((s - square(3.0, 0.0, 45.0).separation(&square(0.0, 0.0, 0.0))).abs() < 1e-12);
    }

    #[test]
    fn vehicle_box_from_its_origin() {
        // a car facing east whose origin is 1 m behind its middle
        let b = Obb::vehicle(DVec2::new(10.0, 5.0), 90.0, 3.0, 1.0, 0.9);
        assert!((b.center - DVec2::new(11.0, 5.0)).length() < 1e-9);
        assert!((b.half - DVec2::new(0.9, 2.0)).length() < 1e-9);
        // its front bumper is at x = 13
        let wall = Obb {
            center: DVec2::new(14.0, 5.0),
            half: DVec2::new(5.0, 0.5),
            heading: 90f64.to_radians(),
            z0: 0.0,
            z1: 1.0,
            velocity: DVec2::ZERO,
            mass: 0.0,
            pole: None,
            id: -1,
        };
        assert!((b.separation(&wall) - 0.5).abs() < 1e-9);
    }
}

#[cfg(test)]
mod mesh_tests {
    use super::*;
    use std::sync::Arc;

    /// The twelve triangles of a closed box from `lo` to `hi`.
    fn cube(lo: DVec3, hi: DVec3) -> Vec<[DVec3; 3]> {
        let c = |i: u32| {
            DVec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            )
        };
        let quads = [
            [0, 1, 3, 2],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 3, 7, 6],
            [0, 2, 6, 4],
            [1, 3, 7, 5],
        ];
        quads
            .iter()
            .flat_map(|q| [[c(q[0]), c(q[1]), c(q[2])], [c(q[0]), c(q[2]), c(q[3])]])
            .collect()
    }

    fn bus(x: f64, y: f64, z: f64) -> Obb {
        Obb::from_box([2.5, 12.0, 2.9, 0.0, 0.0, 1.75], DVec3::new(x, y, z), 0.0)
    }

    #[test]
    fn near_lists_each_part_once_in_the_order_met() {
        let mut seed = 7u64;
        let mut rnd = |a: f64, b: f64| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            a + (b - a) * ((seed >> 11) as f64 / (1u64 << 53) as f64)
        };
        let mut tris = Vec::new();
        for _ in 0..400 {
            let (x, y, w, d) = (
                rnd(-30.0, 30.0),
                rnd(-30.0, 30.0),
                rnd(0.2, 14.0),
                rnd(0.2, 14.0),
            );
            tris.extend(cube(
                DVec3::new(x, y, 0.0),
                DVec3::new(x + w, y + d, rnd(1.0, 6.0)),
            ));
        }
        let shape = MeshShape::from_triangles(tris.into_iter(), 0.3);
        for _ in 0..300 {
            let (c, r) = (
                DVec2::new(rnd(-40.0, 40.0), rnd(-40.0, 40.0)),
                rnd(0.5, 25.0),
            );
            let mut want: Vec<u32> = Vec::new();
            for y in ((c.y - r) / PART_CELL).floor() as i32..=((c.y + r) / PART_CELL).floor() as i32
            {
                for x in
                    ((c.x - r) / PART_CELL).floor() as i32..=((c.x + r) / PART_CELL).floor() as i32
                {
                    for &i in shape.grid.get(&(x, y)).map(|v| v.as_slice()).unwrap_or(&[]) {
                        let p = &shape.parts[i as usize];
                        if (p.center - c).length() <= r + p.radius() && !want.contains(&i) {
                            want.push(i);
                        }
                    }
                }
            }
            assert_eq!(shape.near(c, r), want);
        }
    }

    #[test]
    fn two_houses_leave_the_street_between_them_open() {
        // two houses 10 m apart in one mesh: its extents cover the street between them
        let mut tris = cube(DVec3::new(-20.0, -10.0, 0.0), DVec3::new(-5.0, 10.0, 12.0));
        tris.extend(cube(
            DVec3::new(5.0, -10.0, 0.0),
            DVec3::new(20.0, 10.0, 12.0),
        ));
        let shape = Arc::new(MeshShape::from_triangles(tris.into_iter(), 0.3));
        let base = DVec3::new(892248.2, 4196461.4, 33.2);
        let mut w = CollisionWorld::default();
        w.add_mesh(MeshObstacle::new(shape, base, 30.0, 7));
        let turn = |x: f64, y: f64| {
            let h = 30f64.to_radians();
            DVec3::new(
                base.x + x * h.cos() + y * h.sin(),
                base.y - x * h.sin() + y * h.cos(),
                base.z,
            )
        };
        let street = turn(0.0, 0.0);
        let mut b = bus(street.x, street.y, street.z);
        b.heading = 30f64.to_radians();
        assert!(w.hit(&b).is_none(), "the street between the houses is open");
        let wall = turn(4.0, 0.0);
        let mut b = bus(wall.x, wall.y, wall.z);
        b.heading = 30f64.to_radians();
        let hit = w.hit(&b).expect("the bus's right side is in the house");
        assert_eq!(hit.id, 7);
        let c = b.contact(&hit).expect("touching");
        // pushed back to the left, out of the east house
        let left = DVec2::new(-(30f64.to_radians().cos()), 30f64.to_radians().sin());
        assert!(c.normal.dot(left) > 0.99 && c.depth < 0.5, "{c:?}");
    }

    #[test]
    fn a_bridge_deck_passes_over_the_bus_and_carries_it() {
        // a deck 5.5..6.5 m up, 20 m wide across the road, with a railing along each edge
        let mut tris = cube(DVec3::new(-50.0, -10.0, 5.5), DVec3::new(50.0, 10.0, 6.5));
        tris.extend(cube(
            DVec3::new(-50.0, -10.0, 6.5),
            DVec3::new(50.0, -9.8, 7.5),
        ));
        tris.extend(cube(
            DVec3::new(-50.0, 9.8, 6.5),
            DVec3::new(50.0, 10.0, 7.5),
        ));
        let shape = Arc::new(MeshShape::from_triangles(tris.into_iter(), 0.3));
        let mut w = CollisionWorld::default();
        w.add_mesh(MeshObstacle::new(shape, DVec3::ZERO, 0.0, 1));
        // the road below runs across the deck
        let mut under = bus(0.0, 0.0, 0.0);
        under.heading = 90f64.to_radians();
        assert!(w.hit(&under).is_none());
        // a bus on the deck, its floor clear of the plate, between the railings
        let mut on = bus(0.0, 0.0, 6.5);
        on.heading = 90f64.to_radians();
        assert!(w.hit(&on).is_none());
        // and one driving into the railing
        let mut into = bus(0.0, 8.8, 6.5);
        into.heading = 90f64.to_radians();
        assert!(w.hit(&into).is_some());
    }

    #[test]
    fn a_pitched_roof_is_not_a_block() {
        // a roof ridge 8 m up over eaves at 4 m, the house 10 m deep: a bus may not stand in
        // the house but may stand next to it where the roof's extents reach
        let ridge = [DVec3::new(0.0, -10.0, 8.0), DVec3::new(0.0, 10.0, 8.0)];
        let eave = [DVec3::new(5.0, -10.0, 4.0), DVec3::new(5.0, 10.0, 4.0)];
        let tris = vec![[ridge[0], ridge[1], eave[1]], [ridge[0], eave[1], eave[0]]];
        let shape = MeshShape::from_triangles(tris.into_iter(), 0.3);
        assert!(shape.parts.len() > 2);
        let mut w = CollisionWorld::default();
        w.add_mesh(MeshObstacle::new(Arc::new(shape), DVec3::ZERO, 0.0, 1));
        // a 3 m tall bus 1 m over the ground under the eave's edge
        let b = Obb::from_box(
            [2.5, 12.0, 3.0, 0.0, 0.0, 2.5],
            DVec3::new(4.5, 0.0, 0.0),
            0.0,
        );
        assert!(w.hit(&b).is_none());
    }

    #[test]
    fn camera_ray_meets_a_mesh_wall() {
        let shape = Arc::new(MeshShape::from_triangles(
            cube(DVec3::new(-10.0, -10.0, 0.0), DVec3::new(10.0, 10.0, 12.0)).into_iter(),
            0.3,
        ));
        let mut w = CollisionWorld::default();
        w.add_mesh(MeshObstacle::new(
            shape,
            DVec3::new(100.0, 0.0, 0.0),
            0.0,
            1,
        ));
        assert!(w.ray_blocked(
            DVec3::new(50.0, 0.0, 2.0),
            DVec3::new(150.0, 0.0, 2.0),
            2.5,
            3.0
        ));
        assert!(!w.ray_blocked(
            DVec3::new(50.0, 0.0, 20.0),
            DVec3::new(150.0, 0.0, 20.0),
            2.5,
            3.0
        ));
        assert!(!w.ray_blocked(
            DVec3::new(50.0, 20.0, 2.0),
            DVec3::new(150.0, 20.0, 2.0),
            2.5,
            3.0
        ));
    }
}

#[cfg(test)]
mod ramp_tests {
    use super::*;

    #[test]
    fn a_bus_drives_up_a_mesh_ramp() {
        // the Heerstraße bridge's ramp: 70 m long, rising 6.95 m, 3.6 m wide
        let (a, b, c, d) = (
            DVec3::new(-1.8, 0.0, 0.0),
            DVec3::new(1.8, 0.0, 0.0),
            DVec3::new(1.8, 70.0, 6.95),
            DVec3::new(-1.8, 70.0, 6.95),
        );
        let shape = MeshShape::from_triangles([[a, b, c], [a, c, d]].into_iter(), 0.3);
        let mut w = CollisionWorld::default();
        w.add_mesh(MeshObstacle::new(
            std::sync::Arc::new(shape),
            DVec3::ZERO,
            0.0,
            1,
        ));
        // a 12 m bus standing on the ramp (pitched with it), its floor 0.35 m over it
        for y in [8.0, 20.0, 40.0, 63.0] {
            let z = y * 6.95 / 70.0;
            let pitch = (6.95f64 / 70.0).atan();
            let mut bus = Obb::from_box(
                [2.5, 12.0, 2.85, 0.0, 0.0, 1.775],
                DVec3::new(0.0, y, z),
                0.0,
            );
            // the pitched box's lowest corner is at the rear: 6 m back, 0.35 m up
            bus.z0 = z - 6.0 * pitch.sin() + 0.35 * pitch.cos();
            // which is all the plan-view box knows: it is in the ramp ahead
            assert!(w.hit(&bus).is_some());
            let (up, fwd) = (
                DVec3::new(0.0, -pitch.sin(), pitch.cos()),
                DVec3::new(0.0, pitch.cos(), pitch.sin()),
            );
            let solid = Box3 {
                center: DVec3::new(0.0, y, z) + up * 1.775,
                axes: [DVec3::X, fwd, up],
                half: DVec3::new(1.25, 6.0, 1.425),
            };
            let faces = w.obstacles_near_solid(&bus, Some(&solid));
            assert!(faces.iter().all(|f| !f.overlaps(&bus)), "at {y}: {faces:?}");
        }
        // but a bus level with the foot of the ramp, 10 m up it, is in it
        let bus = Obb::from_box(
            [2.5, 12.0, 2.85, 0.0, 0.0, 1.775],
            DVec3::new(0.0, 10.0, 0.0),
            0.0,
        );
        assert!(w.hit(&bus).is_some());
    }
}
