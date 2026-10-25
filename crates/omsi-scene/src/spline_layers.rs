//! Which of a tile's splines lie on an earlier one at the same height: a red bus lane laid
//! over the road at the road's own height (London's `iomex Treatment_Red_*.sli`, `btv
//! tarmac_red_*.sli`). Omsi.exe draws the splines of a tile in their order with LESSEQUAL,
//! so where two of them are one surface the later one is seen. Here their depths differ by
//! rounding alone and the draws are grouped by material and distance, not by the map's
//! order: the winner of each pixel changed as the camera moved and the red lanes came and
//! went (#2853). Such a spline takes the draw order's depth step (`draw_order_level`), one
//! level over the highest of those it lies on.
//!
//! A spline can lie on a `[surface]` object too: at Crystal Palace the whole carriageway is
//! the junction object and the red lanes and the lines are splines on it. The surface
//! objects are pulled towards the eye a little more than the splines (`scene_clip`: they are
//! meant to lie on the roads), so they covered what lay on them from a few metres away or
//! from above. A spline lying on one is pulled as the object is, so that their own heights
//! decide again, and takes a step for the ties.

use glam::{Vec2, Vec3};
use std::collections::HashMap;

use omsi_geometry::MeshData;

/// How close (m) to an earlier spline's surface a triangle's middle has to be to lie on it.
/// Road-hog123's markings, 5 mm over the road, are paint already (`spline_paint_slots`).
const SAME_HEIGHT: f32 = 0.004;
/// How far (m) over a `[surface]` object a triangle's middle may lie and still lie on it:
/// a layer laid over it, not a deck above it.
const OVER_OBJECT: f32 = 0.03;
/// The share of a spline's triangles that have to lie on earlier splines: a road segment
/// meeting the one before at its end, or two roads overlapping at a junction, are not
/// layers on each other.
const COVERED: f32 = 0.8;
/// The grid's cell (m).
const CELL: f32 = 4.0;
/// The most triangles of one spline tested: enough to tell a layer from a neighbour.
const SAMPLES: usize = 96;
/// The highest step (`omsi_render`'s `MAX_LEVEL`).
const MAX_LEVEL: u8 = 3;

/// A triangle of an earlier spline or of a `[surface]` object, flat enough to carry
/// another one: its corners and the level of the spline it belongs to.
struct Face {
    a: Vec3,
    b: Vec3,
    c: Vec3,
    level: u8,
    object: bool,
}

/// What a spline of a tile is drawn with (see the module).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct SplineLayer {
    /// The draw order's depth step, 0 to `MAX_LEVEL`.
    pub(crate) level: u8,
    /// It lies on a `[surface]` object: pulled towards the eye as the object is.
    pub(crate) on_object: bool,
}

/// The height of `f` at `p` (x, y), if `p` lies inside it (seen from above).
fn height_at(f: &Face, p: Vec2) -> Option<f32> {
    let (a, b, c) = (f.a.truncate(), f.b.truncate(), f.c.truncate());
    let d = (b - a).perp_dot(c - a);
    if d.abs() < 1e-9 {
        return None;
    }
    let u = (p - a).perp_dot(c - a) / d;
    let v = (b - a).perp_dot(p - a) / d;
    const EDGE: f32 = -1e-4;
    if u < EDGE || v < EDGE || u + v > 1.0 - EDGE {
        return None;
    }
    Some(f.a.z + (f.b.z - f.a.z) * u + (f.c.z - f.a.z) * v)
}

fn cell(p: Vec2) -> (i32, i32) {
    ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
}

/// The triangles of `m` that face up, as corner positions.
fn up_faces(m: &MeshData) -> impl Iterator<Item = [Vec3; 3]> + '_ {
    m.indices.chunks_exact(3).filter_map(move |t| {
        let p = |i: u32| m.positions.get(i as usize).copied();
        let (a, b, c) = (p(t[0])?, p(t[1])?, p(t[2])?);
        let n = (b - a).cross(c - a);
        let len = n.length();
        (len > 1e-8 && (n.z / len).abs() > 0.5).then_some([a, b, c])
    })
}

fn add_face(faces: &mut Vec<Face>, grid: &mut HashMap<(i32, i32), Vec<usize>>, f: Face) {
    let i = faces.len();
    let lo = f.a.min(f.b).min(f.c).truncate();
    let hi = f.a.max(f.b).max(f.c).truncate();
    faces.push(f);
    let (c0, c1) = (cell(lo), cell(hi));
    // (a huge triangle - a broken mesh - is left out rather than filling the grid)
    if (c1.0 - c0.0) as i64 * (c1.1 - c0.1) as i64 > 4096 {
        return;
    }
    for x in c0.0..=c1.0 {
        for y in c0.1..=c1.1 {
            grid.entry((x, y)).or_default().push(i);
        }
    }
}

/// How each spline of a tile is drawn (in the tile's order; the splines' and the `[surface]`
/// objects' triangles `ground` in one frame): `COVERED` of its upward triangles lying
/// within `SAME_HEIGHT` on earlier splines, one step more than the highest of those, at
/// most `MAX_LEVEL`; lying on `[surface]` objects (up to `OVER_OBJECT` over them), pulled
/// as they are and at least one step.
pub(crate) fn spline_layers(meshes: &[&MeshData], ground: &[[Vec3; 3]]) -> Vec<SplineLayer> {
    let mut faces: Vec<Face> = Vec::new();
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for t in ground {
        let n = (t[1] - t[0]).cross(t[2] - t[0]);
        let len = n.length();
        if len > 1e-8 && (n.z / len).abs() > 0.5 {
            add_face(&mut faces, &mut grid, Face { a: t[0], b: t[1], c: t[2], level: 0, object: true });
        }
    }
    let mut out = Vec::with_capacity(meshes.len());
    for m in meshes {
        let tris: Vec<[Vec3; 3]> = up_faces(m).collect();
        let mut layer = SplineLayer::default();
        if !tris.is_empty() && !faces.is_empty() {
            let stride = tris.len().div_ceil(SAMPLES).max(1);
            let (mut tested, mut on, mut on_object, mut under) = (0usize, 0usize, 0usize, 0u8);
            for t in tris.iter().step_by(stride) {
                let mid = (t[0] + t[1] + t[2]) / 3.0;
                let p = mid.truncate();
                let (mut spline_hit, mut object_hit, mut buried) = (None::<u8>, false, false);
                for &i in grid.get(&cell(p)).into_iter().flatten() {
                    let f = &faces[i];
                    let Some(z) = height_at(f, p) else { continue };
                    let over = mid.z - z;
                    buried |= over < -SAME_HEIGHT;
                    if f.object {
                        object_hit |= (-SAME_HEIGHT..=OVER_OBJECT).contains(&over);
                    } else if over.abs() <= SAME_HEIGHT {
                        spline_hit = spline_hit.max(Some(f.level));
                    }
                }
                // A triangle under the surface is never seen: the flanks of Road-hog123's
                // warning lines, a tent whose sides run down to 0 under a 10 cm road, do not
                // keep its top - the line - from lying on the road.
                if buried && spline_hit.is_none() && !object_hit {
                    continue;
                }
                tested += 1;
                if let Some(l) = spline_hit {
                    on += 1;
                    under = under.max(l);
                }
                on_object += object_hit as usize;
            }
            let covered = |n: usize| tested > 0 && n as f32 >= COVERED * tested as f32;
            if covered(on) {
                layer.level = (under + 1).min(MAX_LEVEL);
            }
            if covered(on_object) {
                layer.on_object = true;
                layer.level = layer.level.max(1);
            }
        }
        for t in tris {
            add_face(&mut faces, &mut grid, Face { a: t[0], b: t[1], c: t[2], level: layer.level, object: false });
        }
        out.push(layer);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A strip from x0 to x1 across, 0 to `len` along y, at height z, cut into `n` quads.
    fn strip(x0: f32, x1: f32, len: f32, z: f32, n: u32) -> MeshData {
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        for i in 0..=n {
            let y = len * i as f32 / n as f32;
            positions.push(Vec3::new(x0, y, z));
            positions.push(Vec3::new(x1, y, z));
        }
        for i in 0..n {
            let a = i * 2;
            indices.extend_from_slice(&[a, a + 1, a + 2, a + 1, a + 3, a + 2]);
        }
        let count = indices.len() as u32;
        MeshData { normals: vec![Vec3::Z; positions.len()], uvs: vec![Vec2::ZERO; positions.len()], positions, indices, ranges: vec![(0, count, 0)], one_sided: true }
    }

    fn spline_layer_levels(meshes: &[&MeshData]) -> Vec<u8> {
        spline_layers(meshes, &[]).iter().map(|l| l.level).collect()
    }

    fn faces(m: &MeshData) -> Vec<[Vec3; 3]> {
        m.indices.chunks_exact(3).map(|t| [m.positions[t[0] as usize], m.positions[t[1] as usize], m.positions[t[2] as usize]]).collect()
    }

    /// Crystal Palace: the carriageway is a `[surface]` object, the red lanes are splines a
    /// little over it.
    #[test]
    fn a_lane_on_a_surface_object_is_pulled_as_the_object_is() {
        let carriageway = faces(&strip(-6.0, 6.0, 40.0, 0.0, 4));
        let red = strip(1.0, 4.0, 20.0, 0.008, 7);
        let on_red = strip(2.0, 2.2, 10.0, 0.008, 3);
        // a footway's kerb beside it, and a bridge deck a metre over it, are not on it
        let footway = strip(6.0, 8.0, 40.0, 0.15, 4);
        let deck = strip(-2.0, 2.0, 30.0, 1.0, 4);
        // a warning line: a tent whose top lies on the carriageway and whose flanks run down
        // under it
        let tent = {
            let mut m = strip(-0.18, 0.18, 12.0, 0.0, 3);
            let mut positions = Vec::new();
            let mut indices = Vec::new();
            for i in 0..=3u32 {
                let y = 12.0 * i as f32 / 3.0;
                for (x, z) in [(-3.18, -0.1), (-3.075, 0.005), (-2.925, 0.005), (-2.82, -0.1)] {
                    positions.push(Vec3::new(x, y, z));
                }
            }
            for i in 0..3u32 {
                for j in 0..3u32 {
                    let a = i * 4 + j;
                    indices.extend_from_slice(&[a, a + 1, a + 4, a + 1, a + 5, a + 4]);
                }
            }
            m.normals = vec![Vec3::Z; positions.len()];
            m.uvs = vec![Vec2::ZERO; positions.len()];
            m.ranges = vec![(0, indices.len() as u32, 0)];
            m.positions = positions;
            m.indices = indices;
            m
        };
        let layers = spline_layers(&[&red, &on_red, &footway, &deck, &tent], &carriageway);
        assert_eq!(layers[0], SplineLayer { level: 1, on_object: true });
        assert_eq!(layers[1], SplineLayer { level: 2, on_object: true });
        assert_eq!(layers[2], SplineLayer::default());
        assert_eq!(layers[3], SplineLayer::default());
        assert_eq!(layers[4], SplineLayer { level: 1, on_object: true });
    }

    #[test]
    fn a_lane_laid_on_the_road_at_its_height_takes_a_step() {
        let road = strip(-4.5, 4.5, 30.0, 0.1, 6);
        // the red lane: other cuts, other width, the same height
        let red = strip(1.0, 3.5, 20.0, 0.1, 11);
        // a marking on the red lane (a level more), and the road's next segment
        let on_red = strip(2.0, 2.2, 10.0, 0.1, 3);
        let mut next = strip(-4.5, 4.5, 30.0, 0.1, 6);
        for p in &mut next.positions {
            p.y += 30.0;
        }
        let levels = spline_layer_levels(&[&road, &red, &on_red, &next]);
        assert_eq!(levels, vec![0, 1, 2, 0]);
    }

    #[test]
    fn neighbours_raised_layers_and_junctions_keep_their_depth() {
        let road = strip(-4.5, 4.5, 30.0, 0.1, 6);
        // a footway beside the road, a kerb's height over it
        let footway = strip(4.5, 7.0, 30.0, 0.25, 6);
        // a layer over the road but a few centimetres up
        let raised = strip(1.0, 3.5, 20.0, 0.15, 4);
        // a crossing road overlapping only a part of this one
        let mut crossing = strip(-4.5, 4.5, 60.0, 0.1, 12);
        for p in &mut crossing.positions {
            *p = Vec3::new(p.y - 30.0, p.x + 15.0, p.z);
        }
        let levels = spline_layer_levels(&[&road, &footway, &raised, &crossing]);
        assert_eq!(levels, vec![0, 0, 0, 0]);
        // and the first spline of a tile never has one
        assert_eq!(spline_layer_levels(&[&road]), vec![0]);
    }
}
