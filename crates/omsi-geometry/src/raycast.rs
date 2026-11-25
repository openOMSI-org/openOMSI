//! Rays against meshes, triangles and spheres (picking, sight lines), and the vertical probe
//! a wheel's point asks the faces with.

use super::*;

/// Möller-Trumbore ray/triangle test. Returns the distance along the ray.
pub fn ray_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    ray_triangle_bary(origin, dir, a, b, c).map(|(t, _, _)| t)
}

/// The same test, with the barycentric coordinates of the hit: `(t, u, v)`, the point being
/// `a + u * (b - a) + v * (c - a)`.
pub fn ray_triangle_bary(
    origin: Vec3,
    dir: Vec3,
    a: Vec3,
    b: Vec3,
    c: Vec3,
) -> Option<(f32, f32, f32)> {
    let e1 = b - a;
    let e2 = c - a;
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-8 {
        return None;
    }
    let inv = 1.0 / det;
    let t = origin - a;
    let u = t.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let d = e2.dot(q) * inv;
    if d > 1e-4 { Some((d, u, v)) } else { None }
}

/// Where a ray met a mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshHit {
    /// Distance along the ray.
    pub t: f32,
    /// Position of the triangle's first index in `MeshData::indices` (its material slot is the
    /// one of the `ranges` entry that holds it).
    pub index: usize,
    /// The texture coordinates at the hit.
    pub uv: Vec2,
}

impl MeshData {
    /// The material slot of the triangle whose first index is `index`.
    pub fn slot_of(&self, index: usize) -> u32 {
        self.ranges
            .iter()
            .find(|(first, count, _)| {
                index >= *first as usize && index < (*first + *count) as usize
            })
            .map(|r| r.2)
            .unwrap_or(0)
    }
}

/// Closest hit of a world-space ray against a mesh under `transform`, with the triangle it
/// hit and the texture coordinates there (what a click on a screen texture needs).
pub fn ray_mesh_hit(origin: Vec3, dir: Vec3, mesh: &MeshData, transform: &Mat4) -> Option<MeshHit> {
    let inv = transform.inverse();
    let o = inv.transform_point3(origin);
    let d = inv.transform_vector3(dir);
    let scale = d.length();
    if scale < 1e-9 {
        return None;
    }
    let dn = d / scale;
    let mut best: Option<MeshHit> = None;
    for (k, tri) in mesh.indices.chunks_exact(3).enumerate() {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let (a, b, c) = (mesh.positions[i0], mesh.positions[i1], mesh.positions[i2]);
        if let Some((t, u, v)) = ray_triangle_bary(o, dn, a, b, c) {
            let t = t / scale;
            if best.map(|h| t < h.t).unwrap_or(true) {
                let uv = match (mesh.uvs.get(i0), mesh.uvs.get(i1), mesh.uvs.get(i2)) {
                    (Some(&x), Some(&y), Some(&z)) => x * (1.0 - u - v) + y * u + z * v,
                    _ => Vec2::ZERO,
                };
                best = Some(MeshHit {
                    t,
                    index: k * 3,
                    uv,
                });
            }
        }
    }
    best
}

/// Closest hit of a world-space ray against a mesh under `transform`.
/// A sphere around the points: the middle of their box and the farthest point from it.
pub fn bounding_sphere(points: &[Vec3]) -> (Vec3, f32) {
    if points.is_empty() {
        return (Vec3::ZERO, 0.0);
    }
    let (lo, hi) = points.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let c = (lo + hi) * 0.5;
    let r = points
        .iter()
        .map(|p| p.distance_squared(c))
        .fold(0.0f32, f32::max)
        .sqrt();
    (c, r)
}

/// Does the ray (`dir` of unit length) pass within `r` of `c`, ahead of its origin or with
/// the origin inside?
pub fn ray_near_sphere(origin: Vec3, dir: Vec3, c: Vec3, r: f32) -> bool {
    let to = c - origin;
    let along = to.dot(dir);
    let d2 = to.length_squared() - along * along;
    d2 <= r * r && (along >= 0.0 || to.length_squared() <= r * r)
}

pub fn ray_mesh(origin: Vec3, dir: Vec3, mesh: &MeshData, transform: &Mat4) -> Option<f32> {
    // transform the ray into mesh space (cheaper than transforming every vertex)
    let inv = transform.inverse();
    let o = inv.transform_point3(origin);
    let d = inv.transform_vector3(dir);
    let scale = d.length();
    if scale < 1e-9 {
        return None;
    }
    let dn = d / scale;
    let mut best: Option<f32> = None;
    for tri in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (
            mesh.positions[tri[0] as usize],
            mesh.positions[tri[1] as usize],
            mesh.positions[tri[2] as usize],
        );
        if let Some(t) = ray_triangle(o, dn, a, b, c) {
            let t = t / scale;
            if best.map(|bt| t < bt).unwrap_or(true) {
                best = Some(t);
            }
        }
    }
    best
}

pub fn cone_triangles(
    origin: Vec3,
    axis: Vec3,
    tan_half: f32,
    mesh: &MeshData,
    transform: &Mat4,
) -> Vec<u32> {
    let inv = transform.inverse();
    let o = inv.transform_point3(origin);
    let a = inv.transform_vector3(axis).normalize_or_zero();
    let mut out = Vec::new();
    for (k, tri) in mesh.indices.chunks_exact(3).enumerate() {
        let (p0, p1, p2) = (
            mesh.positions[tri[0] as usize],
            mesh.positions[tri[1] as usize],
            mesh.positions[tri[2] as usize],
        );
        let c = (p0 + p1 + p2) / 3.0;
        let r = (p0 - c)
            .length()
            .max((p1 - c).length())
            .max((p2 - c).length());
        let w = c - o;
        let along = w.dot(a);
        if along < -r {
            continue;
        }
        let across = (w - a * along).length();
        if across > (along + r).max(0.0) * tan_half + r + 1e-3 {
            continue;
        }
        out.push((k * 3) as u32);
    }
    out
}

pub fn ray_triangles(
    origin: Vec3,
    dir: Vec3,
    mesh: &MeshData,
    transform: &Mat4,
    tris: &[u32],
) -> Option<f32> {
    let inv = transform.inverse();
    let o = inv.transform_point3(origin);
    let d = inv.transform_vector3(dir);
    let scale = d.length();
    if scale < 1e-9 {
        return None;
    }
    let dn = d / scale;
    let mut best: Option<f32> = None;
    for &k in tris {
        let k = k as usize;
        let (a, b, c) = (
            mesh.positions[mesh.indices[k] as usize],
            mesh.positions[mesh.indices[k + 1] as usize],
            mesh.positions[mesh.indices[k + 2] as usize],
        );
        if let Some(t) = ray_triangle(o, dn, a, b, c) {
            let t = t / scale;
            if best.map(|bt| t < bt).unwrap_or(true) {
                best = Some(t);
            }
        }
    }
    best
}

/// What a vertical probe found at one point: the highest face at or below the probe's top,
/// and the lowest face above it (a step the wheel cannot be under, or a bridge deck).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Probe {
    pub below: Option<f32>,
    pub above: Option<f32>,
}

impl Probe {
    /// Combine two probes of the same point (surfaces and terrain).
    pub fn merge(self, o: Probe) -> Probe {
        let pick = |a: Option<f32>, b: Option<f32>, f: fn(f32, f32) -> f32| match (a, b) {
            (Some(x), Some(y)) => Some(f(x, y)),
            (x, y) => x.or(y),
        };
        Probe {
            below: pick(self.below, o.below, f32::max),
            above: pick(self.above, o.above, f32::min),
        }
    }

    /// Put a single height into a probe with its top at `z_top`.
    pub fn of(z: f32, z_top: f32) -> Probe {
        if z <= z_top {
            Probe {
                below: Some(z),
                above: None,
            }
        } else {
            Probe {
                below: None,
                above: Some(z),
            }
        }
    }
}
