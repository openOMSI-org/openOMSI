//! The outside camera's arm: how far it may swing out from the vehicle before scenery is
//! in the way, and how it pulls in and eases back out again.
//!
//! OMSI's own outside camera only keeps above the ground. Here the arm is a probe from the
//! point the camera orbits to where it wants to be - five parallel rays (the middle and the
//! four sides of the near plane) against the ground and the solid meshes of the buildings,
//! walls, shelters and canopies around - and the camera stands short of the first hit, the
//! way a modern third-person camera does: it snaps in at once (the inside of a wall is never
//! shown), waits a moment once the way is clear and then eases back out, so that an edge
//! swept past does not make it pump in and out.
//!
//! What blocks is decided once per object type ([`BlockerShape`]): only the opaque
//! triangles count (foliage, fences and wire are alpha-tested cut-outs the camera may pass
//! through), and only a type that is at least 2 m tall and wide and whose surfaces are a fair
//! part of its bounding shell - a house, a wall, a bus shelter or a petrol station canopy,
//! not a lamp post, a sign on a mast, a traffic light's boom, a tree's trunk or a parked car.
//! `OMSI_DEBUG_CAMERA=1` logs the decision per type and what the arm hits.

use crate::{ObjectType, World};
use glam::{DVec3, Mat4, Vec3};

/// How far the camera keeps from what it hits (m): a little more than the near plane.
const MARGIN: f64 = 0.4;
/// How far apart the side rays run from the middle one (m).
const PROBE: f64 = 0.3;
/// The arm is never shorter than this (m): inside the bus rather than inside the wall.
pub const ARM_MIN: f64 = 1.0;
/// Seconds the arm stays pulled in after the way has become clear.
const HOLD: f32 = 0.35;
/// Rate (1/s) at which it eases back out, and the fastest it may do so (m/s).
const EASE: f32 = 3.0;
const EASE_MAX: f32 = 14.0;
/// Rate (1/s) at which it is pulled in towards a blocked length.
const PULL_IN: f32 = 18.0;
/// How far above the ground the camera stays (m).
const GROUND_CLEARANCE: f64 = 0.6;

/// Types smaller or lower than this never block the camera (m).
const MIN_HEIGHT: f32 = 2.0;
const MIN_WIDTH: f32 = 2.0;
/// Opaque surface area over the area of the bounding shell below which a type is mostly
/// air (a mast with a boom, a tree's trunk under its crown).
const MIN_FILL: f32 = 0.25;

/// `OMSI_DEBUG_CAMERA`: 1 logs the types and what stops the arm, 2 also every frame's arm.
pub fn debug_level() -> u32 {
    static ON: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        omsi_cfg::flags::OMSI_DEBUG_CAMERA
            .var()
            .map(|v| v.trim().parse().unwrap_or(1))
            .unwrap_or(0)
    })
}

pub fn debug() -> bool {
    debug_level() > 0
}

/// Whether and where an object type stops the outside camera.
#[derive(Debug, Clone, Default)]
pub struct BlockerShape {
    pub blocks: bool,
    /// Bounds of the solid triangles in the object's own frame.
    pub lo: Vec3,
    pub hi: Vec3,
    /// Per LOD-0 mesh (parallel to `ObjectType::meshes`): the material slots that count.
    solid: Vec<Vec<bool>>,
    /// The solid triangles in a bounding volume hierarchy, made when the camera first
    /// comes near the type (a detailed mod building has tens of thousands of them).
    bvh: std::sync::OnceLock<TriBvh>,
}

/// Triangles in a binary tree of boxes: a ray looks only at the few whose boxes it passes.
#[derive(Debug, Clone, Default)]
struct TriBvh {
    tris: Vec<[Vec3; 3]>,
    nodes: Vec<BvhNode>,
}

#[derive(Debug, Clone, Copy)]
struct BvhNode {
    lo: Vec3,
    hi: Vec3,
    /// A leaf: `count` triangles from `first`; an inner node: its children are `first` and
    /// `first + 1`.
    first: u32,
    count: u32,
}

const BVH_LEAF: usize = 6;

impl TriBvh {
    fn build(mut tris: Vec<[Vec3; 3]>) -> TriBvh {
        let mut nodes = Vec::with_capacity(tris.len() / 2 + 1);
        nodes.push(BvhNode {
            lo: Vec3::ZERO,
            hi: Vec3::ZERO,
            first: 0,
            count: 0,
        });
        let n = tris.len();
        Self::split(&mut tris, &mut nodes, 0, 0, n);
        TriBvh { tris, nodes }
    }

    /// Make node `at` hold the triangles `start..end`, splitting at the median of the
    /// longest axis of their centres until few are left.
    fn split(
        tris: &mut [[Vec3; 3]],
        nodes: &mut Vec<BvhNode>,
        at: usize,
        start: usize,
        end: usize,
    ) {
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let (mut clo, mut chi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for t in &tris[start..end] {
            for p in t {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
            let c = (t[0] + t[1] + t[2]) / 3.0;
            clo = clo.min(c);
            chi = chi.max(c);
        }
        if end - start <= BVH_LEAF {
            nodes[at] = BvhNode {
                lo,
                hi,
                first: start as u32,
                count: (end - start) as u32,
            };
            return;
        }
        let ext = chi - clo;
        let axis = if ext.x >= ext.y && ext.x >= ext.z {
            0
        } else if ext.y >= ext.z {
            1
        } else {
            2
        };
        let mid = (start + end) / 2;
        let centre = |t: &[Vec3; 3]| t[0][axis] + t[1][axis] + t[2][axis];
        tris[start..end]
            .select_nth_unstable_by(mid - start, |a, b| centre(a).total_cmp(&centre(b)));
        let left = nodes.len();
        nodes.push(BvhNode {
            lo: Vec3::ZERO,
            hi: Vec3::ZERO,
            first: 0,
            count: 0,
        });
        nodes.push(BvhNode {
            lo: Vec3::ZERO,
            hi: Vec3::ZERO,
            first: 0,
            count: 0,
        });
        nodes[at] = BvhNode {
            lo,
            hi,
            first: left as u32,
            count: 0,
        };
        Self::split(tris, nodes, left, start, mid);
        Self::split(tris, nodes, left + 1, mid, end);
    }

    /// The nearest hit of the ray (object frame) closer than `max`.
    fn ray(&self, o: Vec3, d: Vec3, max: f32) -> Option<f32> {
        if self.tris.is_empty() {
            return None;
        }
        let mut best = max;
        let mut hit = false;
        let mut stack = vec![0usize];
        while let Some(i) = stack.pop() {
            let node = &self.nodes[i];
            match slab(o, d, node.lo, node.hi) {
                Some((t0, t1)) if t0 <= best && t1 >= 0.0 => {}
                _ => continue,
            }
            if node.count > 0 {
                for t in &self.tris[node.first as usize..(node.first + node.count) as usize] {
                    if let Some(s) = ray_triangle(o, d, t[0], t[1], t[2]) {
                        if s < best {
                            best = s;
                            hit = true;
                        }
                    }
                }
            } else {
                stack.push(node.first as usize);
                stack.push(node.first as usize + 1);
            }
        }
        hit.then_some(best)
    }
}

impl BlockerShape {
    /// The farthest a solid point lies from the object's origin in the ground plane.
    pub fn radius(&self) -> f64 {
        let x = self.lo.x.abs().max(self.hi.x.abs()) as f64;
        let y = self.lo.y.abs().max(self.hi.y.abs()) as f64;
        (x * x + y * y).sqrt()
    }
}

/// Decide once per type what of it stops the camera.
pub fn classify(ot: &ObjectType) -> BlockerShape {
    let mut shape = BlockerShape {
        lo: Vec3::splat(f32::MAX),
        hi: Vec3::splat(f32::MIN),
        ..Default::default()
    };
    if ot.sco.tree.is_some() || ot.sco.only_editor || ot.sco.is_help_arrow {
        return BlockerShape::default();
    }
    let (mut area, mut tris) = (0.0f32, 0usize);
    for (mi, (mesh, o3d_mats, overrides)) in ot.meshes.iter().enumerate() {
        if ot.mesh_visible.get(mi).and_then(|v| v.as_ref()).is_some() {
            continue;
        }
        let shadow = ot.mesh_shadow.get(mi).copied().unwrap_or(false);
        let base: Vec<omsi_model::MaterialDef> =
            overrides.iter().filter(|o| !o.item).cloned().collect();
        let slots: Vec<bool> = (0..o3d_mats.len().max(1))
            .map(|slot| {
                let texture = o3d_mats.get(slot).map(|m| m.texture.as_str()).unwrap_or("");
                !shadow
                    && !crate::is_null_texture(texture)
                    && crate::material_alpha(o3d_mats, slot, &base)
                        == omsi_render::AlphaMode::Opaque
            })
            .collect();
        for &(first, count, slot) in &mesh.ranges {
            if !slots.get(slot as usize).copied().unwrap_or(false) {
                continue;
            }
            let Some(ix) = mesh.indices.get(first as usize..(first + count) as usize) else {
                continue;
            };
            for t in ix.chunks_exact(3) {
                let (Some(a), Some(b), Some(c)) = (
                    mesh.positions.get(t[0] as usize),
                    mesh.positions.get(t[1] as usize),
                    mesh.positions.get(t[2] as usize),
                ) else {
                    continue;
                };
                area += (*b - *a).cross(*c - *a).length() * 0.5;
                tris += 1;
                for p in [a, b, c] {
                    shape.lo = shape.lo.min(*p);
                    shape.hi = shape.hi.max(*p);
                }
            }
        }
        shape.solid.push(slots);
    }
    if tris == 0 {
        return BlockerShape::default();
    }
    let size = shape.hi - shape.lo;
    let shell = 2.0 * (size.x * size.z + size.y * size.z + size.x * size.y);
    let fill = area / shell.max(1e-3);
    shape.blocks = size.z >= MIN_HEIGHT
        && size.x.max(size.y) >= MIN_WIDTH
        && size.x.min(size.y) >= 0.1
        && fill >= MIN_FILL;
    if debug() {
        log::info!(
            "camera: {} {} ({tris} solid triangles, {:.1} x {:.1} x {:.1} m, fill {fill:.2})",
            ot.sco.path.display(),
            if shape.blocks {
                "blocks"
            } else {
                "lets through"
            },
            size.x,
            size.y,
            size.z
        );
    }
    shape
}

/// A placed object that stops the camera (kept per tile, see `TileState::blockers`).
#[derive(Clone)]
pub struct Blocker {
    pub ty: std::sync::Weak<ObjectType>,
    pub pos: DVec3,
    pub xf: Mat4,
    pub radius: f64,
}

/// Where along the ray (origin, unit dir) up to `max` it first meets a solid triangle of
/// the placed object, if it does.
fn ray_object(
    ot: &ObjectType,
    shape: &BlockerShape,
    pos: DVec3,
    xf: &Mat4,
    origin: DVec3,
    dir: DVec3,
    max: f64,
) -> Option<f64> {
    // into the object's frame (an affine map keeps the ray parameter)
    let inv = xf.inverse();
    let o = inv.transform_point3((origin - pos).as_vec3());
    let d = inv.transform_vector3(dir.as_vec3());
    let (t0, t1) = slab(
        o,
        d,
        shape.lo - Vec3::splat(0.05),
        shape.hi + Vec3::splat(0.05),
    )?;
    if t0 > max as f32 || t1 < 0.0 {
        return None;
    }
    let bvh = shape.bvh.get_or_init(|| {
        let mut tris = Vec::new();
        for ((mesh, _, _), slots) in ot.meshes.iter().zip(&shape.solid) {
            for &(first, count, slot) in &mesh.ranges {
                if !slots.get(slot as usize).copied().unwrap_or(false) {
                    continue;
                }
                let Some(ix) = mesh.indices.get(first as usize..(first + count) as usize) else {
                    continue;
                };
                for t in ix.chunks_exact(3) {
                    if let (Some(a), Some(b), Some(c)) = (
                        mesh.positions.get(t[0] as usize),
                        mesh.positions.get(t[1] as usize),
                        mesh.positions.get(t[2] as usize),
                    ) {
                        tris.push([*a, *b, *c]);
                    }
                }
            }
        }
        TriBvh::build(tris)
    });
    bvh.ray(o, d, max as f32).map(|t| t as f64)
}

/// Entry and exit parameter of a ray through an axis-aligned box.
fn slab(o: Vec3, d: Vec3, lo: Vec3, hi: Vec3) -> Option<(f32, f32)> {
    let (mut t0, mut t1) = (f32::MIN, f32::MAX);
    for k in 0..3 {
        if d[k].abs() < 1e-8 {
            if o[k] < lo[k] || o[k] > hi[k] {
                return None;
            }
            continue;
        }
        let (a, b) = ((lo[k] - o[k]) / d[k], (hi[k] - o[k]) / d[k]);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
        if t0 > t1 {
            return None;
        }
    }
    Some((t0, t1))
}

/// Möller-Trumbore, both faces: a one-sided wall seen from behind is still a wall.
fn ray_triangle(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 0.0).then_some(t)
}

/// The outside camera's orbit centre, raised to [`GROUND_CLEARANCE`] over the ground where a
/// bus's `[camera_outside_center]` lies lower than that: from under it every ray of
/// [`free_length`] met the ground at once and the camera stayed trapped under the bus (#702).
pub fn lift_pivot(world: &World, pivot: DVec3) -> DVec3 {
    match world.camera_ground(pivot.x, pivot.y, pivot.z + GROUND_CLEARANCE) {
        Some(g) if pivot.z < g + GROUND_CLEARANCE => DVec3::new(pivot.x, pivot.y, g + GROUND_CLEARANCE),
        _ => pivot,
    }
}

/// How far the camera may go from `pivot` along the unit vector `dir` (at most `want`):
/// the nearest hit of the five rays with scenery, less the margin, and the point where the
/// middle ray comes within [`GROUND_CLEARANCE`] of the ground. `right`/`up` span the plane
/// the side rays are spread in.
pub fn free_length(
    world: &World,
    pivot: DVec3,
    dir: DVec3,
    right: DVec3,
    up: DVec3,
    want: f64,
) -> f64 {
    let reach = want + MARGIN;
    let offsets = [
        DVec3::ZERO,
        right * PROBE,
        -right * PROBE,
        up * PROBE,
        -up * PROBE,
    ];
    let ends: Vec<DVec3> = offsets
        .iter()
        .flat_map(|o| [pivot + *o, pivot + *o + dir * reach])
        .collect();
    let (mut lo, mut hi) = (ends[0], ends[0]);
    for e in &ends {
        lo = lo.min(*e);
        hi = hi.max(*e);
    }
    let candidates = world.camera_blockers(lo.truncate(), hi.truncate());
    let mut free = reach;
    let mut what: Option<String> = None;
    for (ot, b) in &candidates {
        let Some(shape) = ot.camera_shape() else {
            continue;
        };
        // the object's height range against the ray's (a metre spare for a tilted object)
        let (z0, z1) = (
            b.pos.z + shape.lo.z as f64 - 1.0,
            b.pos.z + shape.hi.z as f64 + 1.0,
        );
        if hi.z < z0 || lo.z > z1 {
            continue;
        }
        for o in &offsets {
            if let Some(t) = ray_object(ot, shape, b.pos, &b.xf, pivot + *o, dir, free) {
                if t < free {
                    free = t;
                    if debug() {
                        what = Some(
                            ot.sco
                                .path
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_default(),
                        );
                    }
                }
            }
        }
    }
    let mut free = free - MARGIN;
    // The ground under the middle ray, in quarter-metre steps and then narrowed down. A face
    // counts up to the clearance above the point: in one step the ray and a slope cannot
    // move by more than that band, so a hillside is never stepped over, while a roof well
    // above the ray is not taken for the ground.
    let below = |t: f64| -> bool {
        let p = pivot + dir * t;
        world
            .camera_ground(p.x, p.y, p.z + GROUND_CLEARANCE)
            .map(|g| p.z < g + GROUND_CLEARANCE)
            .unwrap_or(false)
    };
    let step = 0.25;
    let mut t = step;
    let mut prev = 0.0;
    while t <= free.max(0.0) + 1e-9 {
        if below(t) {
            let (mut a, mut b) = (prev, t);
            for _ in 0..6 {
                let m = (a + b) * 0.5;
                if below(m) {
                    b = m;
                } else {
                    a = m;
                }
            }
            if a < free {
                free = a;
                if debug() {
                    let q = pivot + dir * b;
                    what = Some(format!(
                        "the ground at ({:.1}, {:.1}, {:.2}): {:.2} m",
                        q.x,
                        q.y,
                        q.z,
                        world
                            .camera_ground(q.x, q.y, q.z + GROUND_CLEARANCE)
                            .unwrap_or(f64::NAN)
                    ));
                }
            }
            break;
        }
        prev = t;
        t = if t < free && t + step > free {
            free
        } else {
            t + step
        };
    }
    let free = free.clamp(ARM_MIN.min(want), want);
    if debug() {
        static LAST: std::sync::Mutex<Option<(i64, String)>> = std::sync::Mutex::new(None);
        let key = (
            (free * 4.0).round() as i64,
            what.clone().unwrap_or_default(),
        );
        let mut last = LAST.lock().unwrap();
        if last.as_ref() != Some(&key) {
            log::info!(
                "camera: arm free to {free:.2} of {want:.2} m ({} objects near){}",
                candidates.len(),
                what.map(|w| format!(", stopped by {w}"))
                    .unwrap_or_default()
            );
            *last = Some(key);
        }
    }
    free
}

/// The length of the arm from frame to frame.
#[derive(Debug, Clone, Default)]
pub struct SpringArm {
    len: f32,
    /// What the player asked for last frame.
    want: f32,
    hold: f32,
    pivot: Option<DVec3>,
}

impl SpringArm {
    /// The arm length for this frame: `want` is what the player asks for, `free` how far
    /// the way is clear (≤ `want`), `pivot` the point the camera turns about.
    pub fn update(&mut self, want: f32, free: f32, pivot: DVec3, dt: f32) -> f32 {
        let target = free.min(want);
        // the first frame, or the vehicle was put somewhere else: no easing
        let jumped = self
            .pivot
            .map(|p| (p - pivot).length() > 25.0)
            .unwrap_or(true);
        self.pivot = Some(pivot);
        if jumped {
            self.len = target;
            self.want = want;
            self.hold = 0.0;
            return self.len;
        }
        if target < self.len - 1e-4 {
            // in quickly, but not in one frame: a sudden jump of the camera read as a zoom
            // whenever something passed behind it (a tenth of a second, the rest of the way
            // at once when it is under 5 cm)
            let quick = self.len + (target - self.len) * (1.0 - (-PULL_IN * dt).exp());
            self.len = if quick - target < 0.05 { target } else { quick };
            self.hold = HOLD;
        } else if self.len >= self.want - 1e-3 {
            // not pulled in: the player's own zoom goes straight through
            self.len = target;
        } else if target <= self.len + 1e-4 {
            // still blocked right here: the wait starts once the way is clear
            self.hold = HOLD;
        } else if self.hold > 0.0 {
            // clear, but only just: an edge swept past must not pump the arm
            self.hold -= dt;
        } else {
            let step = ((target - self.len) * (1.0 - (-EASE * dt).exp())).min(EASE_MAX * dt);
            self.len = (self.len + step.max(0.02 * dt)).min(target);
        }
        self.want = want;
        self.len
    }

    /// Forget the state (another view was chosen).
    pub fn reset(&mut self) {
        self.pivot = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(arm: &mut SpringArm, want: f32, free: f32, secs: f32) -> f32 {
        let mut l = 0.0;
        for _ in 0..(secs * 60.0) as usize {
            l = arm.update(want, free, DVec3::ZERO, 1.0 / 60.0);
        }
        l
    }

    #[test]
    fn snaps_in_and_eases_out() {
        let mut arm = SpringArm::default();
        assert_eq!(run(&mut arm, 10.0, 10.0, 0.5), 10.0);
        // a wall at 4 m: in within a moment, not in one frame
        let first = arm.update(10.0, 4.0, DVec3::ZERO, 1.0 / 60.0);
        assert!(first > 4.0 && first < 10.0, "{first}");
        assert_eq!(run(&mut arm, 10.0, 4.0, 0.5), 4.0);
        // clear again: still in during the hold, then out gradually, never past the target
        let held = run(&mut arm, 10.0, 10.0, 0.2);
        assert!((held - 4.0).abs() < 1e-4, "{held}");
        let a = run(&mut arm, 10.0, 10.0, 0.4);
        assert!(a > 4.5 && a < 9.5, "{a}");
        let b = run(&mut arm, 10.0, 10.0, 2.0);
        assert!(b > 9.9 && b <= 10.0, "{b}");
        // and it moves out monotonically, without overshoot
        let mut arm = SpringArm::default();
        run(&mut arm, 10.0, 3.0, 0.5);
        let mut last = 3.0;
        for _ in 0..180 {
            let l = arm.update(10.0, 10.0, DVec3::ZERO, 1.0 / 60.0);
            assert!(l >= last && l <= 10.0);
            last = l;
        }
    }

    #[test]
    fn a_flickering_gap_does_not_pump() {
        // the way is clear every other frame (an edge swept past): the arm stays in
        let mut arm = SpringArm::default();
        run(&mut arm, 10.0, 10.0, 0.2);
        let mut max = 0.0f32;
        for k in 0..120 {
            let free = if k % 2 == 0 { 5.0 } else { 10.0 };
            let l = arm.update(10.0, free, DVec3::ZERO, 1.0 / 60.0);
            if k > 40 {
                max = max.max(l);
            }
        }
        assert!(max <= 5.0 + 1e-4, "{max}");
    }

    #[test]
    fn zoom_is_direct_and_a_jump_resets() {
        let mut arm = SpringArm::default();
        run(&mut arm, 10.0, 10.0, 0.2);
        assert_eq!(arm.update(12.0, 12.0, DVec3::ZERO, 1.0 / 60.0), 12.0);
        run(&mut arm, 12.0, 3.0, 0.2);
        // the bus was put 100 m away with the way clear: no easing out from 3 m
        assert_eq!(
            arm.update(12.0, 12.0, DVec3::new(100.0, 0.0, 0.0), 1.0 / 60.0),
            12.0
        );
    }

    #[test]
    fn the_tree_finds_the_nearest_triangle() {
        // a wall of quads along y, every metre from 2 to 40, each 2 m wide and high
        let mut tris = Vec::new();
        for k in 0..39 {
            let y = 2.0 + k as f32;
            let (a, b, c, d) = (
                Vec3::new(-1.0, y, -1.0),
                Vec3::new(1.0, y, -1.0),
                Vec3::new(1.0, y, 1.0),
                Vec3::new(-1.0, y, 1.0),
            );
            tris.push([a, b, c]);
            tris.push([a, c, d]);
        }
        let bvh = TriBvh::build(tris.clone());
        assert!(bvh.nodes.len() > 1);
        for (o, dir) in [
            (Vec3::ZERO, Vec3::Y),
            (Vec3::new(0.3, 10.5, 0.2), Vec3::Y),
            (Vec3::new(0.5, 50.0, 0.0), -Vec3::Y),
            (
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.05, 1.0, 0.02).normalize(),
            ),
        ] {
            let brute = tris
                .iter()
                .filter_map(|t| ray_triangle(o, dir, t[0], t[1], t[2]))
                .fold(f32::MAX, f32::min);
            let found = bvh.ray(o, dir, 100.0).unwrap();
            assert!(
                (found - brute).abs() < 1e-5,
                "{o:?} {dir:?}: {found} vs {brute}"
            );
        }
        // nothing nearer than the limit, nothing off to the side
        assert_eq!(bvh.ray(Vec3::ZERO, Vec3::Y, 1.5), None);
        assert_eq!(bvh.ray(Vec3::new(3.0, 0.0, 0.0), Vec3::Y, 100.0), None);
    }

    #[test]
    fn rays_meet_triangles_from_both_sides() {
        let (a, b, c) = (
            Vec3::new(-1.0, 5.0, -1.0),
            Vec3::new(1.0, 5.0, -1.0),
            Vec3::new(0.0, 5.0, 1.0),
        );
        let t = ray_triangle(Vec3::ZERO, Vec3::Y, a, b, c).unwrap();
        assert!((t - 5.0).abs() < 1e-5);
        assert!(ray_triangle(Vec3::new(0.0, 10.0, 0.0), -Vec3::Y, a, b, c).is_some());
        assert!(ray_triangle(Vec3::new(3.0, 0.0, 0.0), Vec3::Y, a, b, c).is_none());
        assert!(ray_triangle(Vec3::ZERO, -Vec3::Y, a, b, c).is_none());
        assert_eq!(
            slab(
                Vec3::ZERO,
                Vec3::X,
                Vec3::new(2.0, -1.0, -1.0),
                Vec3::new(3.0, 1.0, 1.0)
            ),
            Some((2.0, 3.0))
        );
        assert_eq!(
            slab(
                Vec3::ZERO,
                Vec3::X,
                Vec3::new(2.0, 1.5, -1.0),
                Vec3::new(3.0, 2.0, 1.0)
            ),
            None
        );
    }
}
