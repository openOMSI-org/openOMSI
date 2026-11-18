//! The texture coordinates of a window's film of rain and snow, laid out in metres.
//!
//! Every pane of a bus carries a film mesh textured with `regen.tga`, and in a snow weather
//! that texture is the snow settled on the glass (`rain::snow_on_glass`). The modellers lay
//! its coordinates out as it suited one pane's own texture: on a double-deck's rear window
//! they run a dozen times further along the glass than across it, so the snow crystals
//! come out as long streaks, and on a small vent window (a tilt window) the whole picture
//! is squeezed into a few centimetres. The drops of the rain film are not affected (their
//! shader works out the metres from the screen's derivatives), but a textured film is.
//!
//! The film's own triangles get coordinates from the glass itself: metres along the pane
//! (horizontally) and up it, so one texture tile is [`TILE`] metres of any window.

use omsi_geometry::MeshData;
use std::borrow::Cow;
use std::collections::HashMap;

/// The side of one tile of the film's texture on the glass (m).
pub const TILE: f32 = 1.0;

/// Mesh `i` of `vt` as the GPU takes it: the film slots' texture coordinates in metres
/// (a film vertex that other slots share becomes a copy of its own).
pub(crate) fn gpu_mesh(vt: &omsi_sim::VehicleType, i: usize) -> Option<Cow<'_, MeshData>> {
    let data = vt.mesh_data(i)?;
    let vm = vt.meshes.get(i)?;
    let film: Vec<u32> = (0..vm.materials.len() as u32)
        .filter(|slot| {
            vm.overrides.iter().any(|m| {
                omsi_sim::vehicle::override_slot(&vm.materials, m) == Some(*slot as usize)
                    && m.alphascale.as_deref().is_some_and(|v| v.trim().to_ascii_lowercase().starts_with("rain_window"))
            })
        })
        .collect();
    if film.is_empty() {
        return Some(data);
    }
    Some(match metric_film_uvs(&data, &film) {
        Some(m) => Cow::Owned(m),
        None => data,
    })
}

/// A copy of `data` whose triangles of the `film` slots have their coordinates in metres on
/// the pane (None: no such triangles). Each such triangle gets vertices of its own (the
/// slots of one mesh share vertices), laid out by a normal worked out from the film's own
/// triangles: a film mesh's vertex normals are not to be relied on (many have none). The
/// normal is the mean of the triangles that meet at a point, so that a slightly curved or
/// raked pane unrolls without a seam: a normal a degree off between two triangles would
/// shift the pattern by a few centimetres at the far end of a twelve-metre bus.
pub fn metric_film_uvs(data: &MeshData, film: &[u32]) -> Option<MeshData> {
    if data.indices.is_empty() || !film.iter().any(|s| data.ranges.iter().any(|r| r.2 == *s && r.1 > 0)) {
        return None;
    }
    // (the point's key: its position to a millimetre)
    let key = |p: glam::Vec3| [(p.x * 1000.0).round() as i32, (p.y * 1000.0).round() as i32, (p.z * 1000.0).round() as i32];
    let triangles = |first: u32, count: u32| (first as usize..(first + count) as usize).step_by(3).filter_map(|k| {
        let tri = data.indices.get(k..k + 3)?;
        let p = [*data.positions.get(tri[0] as usize)?, *data.positions.get(tri[1] as usize)?, *data.positions.get(tri[2] as usize)?];
        Some((k, p))
    });
    // One pane = the film triangles that meet at their corners (a windscreen of two curved
    // halves is one pane; a rear window and the side windows of the same slot are not).
    // Each pane gets ONE normal, the area-weighted mean of its triangles, so that its glass
    // unrolls on a single plane: a normal worked out per corner turned the plane a little at
    // every vertex of a curved windscreen and the snow came out in swirls and bands.
    let mut ids: HashMap<[i32; 3], usize> = HashMap::new();
    let mut parent: Vec<usize> = Vec::new();
    fn find(parent: &mut Vec<usize>, mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut tri_list: Vec<([usize; 3], glam::Vec3)> = Vec::new();
    for &(first, count, slot) in data.ranges.iter().filter(|r| film.contains(&r.2)) {
        let _ = slot;
        for (_, p) in triangles(first, count) {
            let n = oriented((p[1] - p[0]).cross(p[2] - p[0]));
            let mut c = [0usize; 3];
            for (k, q) in p.iter().enumerate() {
                c[k] = *ids.entry(key(*q)).or_insert_with(|| {
                    parent.push(parent.len());
                    parent.len() - 1
                });
            }
            let (r0, r1, r2) = (find(&mut parent, c[0]), find(&mut parent, c[1]), find(&mut parent, c[2]));
            parent[r1] = r0;
            let r2 = find(&mut parent, r2);
            parent[r2] = r0;
            tri_list.push((c, n));
        }
    }
    let mut pane_normal: HashMap<usize, glam::Vec3> = HashMap::new();
    for (c, n) in &tri_list {
        let root = find(&mut parent, c[0]);
        *pane_normal.entry(root).or_insert(glam::Vec3::ZERO) += *n;
    }
    // (the normal of the pane each corner belongs to, by the corner's key)
    let corner_normal: HashMap<[i32; 3], glam::Vec3> = ids
        .iter()
        .filter_map(|(k, &id)| {
            let root = find(&mut parent, id);
            pane_normal.get(&root).map(|n| (*k, *n))
        })
        .collect();
    let mean = |q: glam::Vec3| -> Option<glam::Vec3> { corner_normal.get(&key(q)).copied() };
    let mut out = data.clone();
    for &(first, count, slot) in &data.ranges {
        if !film.contains(&slot) {
            continue;
        }
        for (k, p) in triangles(first, count) {
            if (p[1] - p[0]).cross(p[2] - p[0]).length_squared() < 1e-12 {
                continue;
            }
            for (c, q) in p.iter().enumerate() {
                let n = out.positions.len() as u32;
                let normal = mean(*q).unwrap_or(glam::Vec3::Z);
                out.positions.push(*q);
                out.normals.push(data.normals.get(data.indices[k + c] as usize).copied().unwrap_or(normal.normalize_or_zero()));
                out.uvs.push(pane_coordinate(*q, normal) / TILE);
                out.indices[k + c] = n;
            }
        }
    }
    Some(out)
}

/// `n` turned into the half of space where a pane's normal is taken (a pane's two sides, and
/// triangles wound either way, then share their coordinates).
fn oriented(n: glam::Vec3) -> glam::Vec3 {
    if n.dot(glam::Vec3::new(0.4, 0.8, 0.45)) < 0.0 {
        -n
    } else {
        n
    }
}

/// The position `p` on a pane whose normal is `normal`, in metres: along the glass
/// horizontally and up it (the vehicle's z is up). A roof light, which has no up the glass,
/// takes the vehicle's own x and y. The normal's sign does not matter: a pane's two sides
/// (and triangles wound either way) share the coordinates.
pub fn pane_coordinate(p: glam::Vec3, normal: glam::Vec3) -> glam::Vec2 {
    let n = oriented(normal).normalize_or_zero();
    let across = glam::Vec3::Z.cross(n);
    let t = if across.length() < 0.2 { glam::Vec3::X } else { across.normalize() };
    let b = n.cross(t);
    glam::Vec2::new(p.dot(t), p.dot(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Vec2, Vec3};

    /// A flat pane `w` wide and `h` high facing back (normal -y), its texture coordinates
    /// squeezed to a hundredth of the width, with a second slot sharing two of its vertices.
    fn pane(w: f32, h: f32) -> MeshData {
        let positions = vec![Vec3::new(0.0, 0.0, 1.0), Vec3::new(w, 0.0, 1.0), Vec3::new(w, 0.0, 1.0 + h), Vec3::new(0.0, 0.0, 1.0 + h)];
        MeshData {
            normals: vec![-Vec3::Y; 4],
            uvs: vec![Vec2::new(0.0, 0.0), Vec2::new(0.01, 0.0), Vec2::new(0.01, 1.0), Vec2::new(0.0, 1.0)],
            positions,
            indices: vec![0, 1, 2, 0, 2, 3, 0, 1, 2],
            ranges: vec![(0, 6, 0), (6, 3, 1)],
            one_sided: true,
        }
    }

    /// The film's coordinates grow by a metre per metre of glass in both directions, on a
    /// pane that is long and low and on one that is small, and the other slot is untouched.
    #[test]
    fn film_coordinates_are_metres_on_the_glass() {
        for (w, h) in [(2.4f32, 0.6f32), (0.3, 0.25)] {
            let d = pane(w, h);
            let m = metric_film_uvs(&d, &[0]).expect("film");
            let idx = |k: usize| m.indices[k] as usize;
            let (a, b, c) = (m.uvs[idx(0)], m.uvs[idx(1)], m.uvs[idx(2)]);
            assert!(((a - b).length() - w / TILE).abs() < 1e-4, "{w}x{h}: {a} {b}");
            assert!(((b - c).length() - h / TILE).abs() < 1e-4, "{w}x{h}: {b} {c}");
            // (up the glass is the second coordinate: it grows with z)
            assert!((c.y - b.y).abs() > 0.0 && (c.x - b.x).abs() < 1e-5);
            // the second slot keeps its vertices and its coordinates
            assert_eq!(&m.indices[6..], &d.indices[6..]);
            assert_eq!(m.uvs[..4], d.uvs[..]);
        }
    }

    #[test]
    fn a_mesh_without_a_film_is_left_alone() {
        assert!(metric_film_uvs(&pane(1.0, 1.0), &[5]).is_none());
    }

    /// A roof light (its normal up) takes the vehicle's x and y.
    #[test]
    fn a_roof_light_takes_the_vehicles_axes() {
        let c = pane_coordinate(Vec3::new(1.5, 2.5, 3.0), Vec3::Z);
        assert!((c - Vec2::new(1.5, 2.5)).length() < 1e-5, "{c}");
    }

    /// A curved windscreen (two halves bent a few degrees) is one plane: the corner they
    /// share has the same coordinates from both halves, and no corner is turned by its own
    /// neighbours' normals (the snow came out in swirls).
    #[test]
    fn a_curved_pane_unrolls_on_one_plane() {
        let a = 0.2f32;
        let positions = vec![
            Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 2.0), Vec3::new(0.0, 0.0, 2.0),
            Vec3::new(1.0 + a.cos(), -a.sin(), 1.0), Vec3::new(1.0 + a.cos(), -a.sin(), 2.0),
        ];
        let d = MeshData {
            normals: vec![-Vec3::Y; 6],
            uvs: vec![Vec2::ZERO; 6],
            positions,
            indices: vec![0, 1, 2, 0, 2, 3, 1, 4, 5, 1, 5, 2],
            ranges: vec![(0, 12, 0)],
            one_sided: true,
        };
        let m = metric_film_uvs(&d, &[0]).expect("film");
        // the shared corner (1, 0, 1) is index 1 in the first half's triangle 0 and in the
        // second half's triangle (k = 6)
        let (first, second) = (m.uvs[m.indices[1] as usize], m.uvs[m.indices[6] as usize]);
        assert!((first - second).length() < 1e-5, "{first} {second}");
        // (and the third corner of a triangle further along the seam too)
        let (c1, c2) = (m.uvs[m.indices[2] as usize], m.uvs[m.indices[11] as usize]);
        assert!((c1 - c2).length() < 1e-5, "{c1} {c2}");
    }
}
