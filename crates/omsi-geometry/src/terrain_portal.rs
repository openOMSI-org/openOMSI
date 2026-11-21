//! A `[terrainhole]` that stands upright, the mouth of a tunnel: the ground behind it is cut,
//! the width of the opening, up to where the hill is over it, and no wall stands across it.

use super::{DVec2, DVec3, Mat4, MeshData};

/// How much of an upright sheet's area shows from above, at the most.
const UPRIGHT: f64 = 0.5;
/// How far behind the opening the ground is cut when no hill rises over it (m).
const MAX_DEPTH: f64 = 30.0;
/// The ground counts as over the opening this far above its top (m).
const CLEARANCE: f64 = 0.2;
/// The cut starts this far in front of the opening's foot (m): a mesh's edge is not the ground's.
const FORE: f64 = 0.2;
/// How far in front of the opening no wall may stand (m).
const CLEARING: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Portal {
    /// The ends of its foot.
    pub foot: [DVec3; 2],
    /// The height of its top.
    pub top: f64,
    /// Which way it leans, in plan: from its foot to its top.
    pub lean: DVec2,
}

/// The upright opening a hole mesh is, if it is one: every face of it mostly upright. It runs
/// as a map's tiles load, so every step is linear in the mesh: each vertex is taken once
/// however many triangles share it, and the ends of the foot are found in two sweeps along its bottom edge.
pub fn hole_portal(mesh: &MeshData, transform: &Mat4, origin: DVec3) -> Option<Portal> {
    let world = |i: u32| {
        origin
            + transform
                .transform_point3(mesh.positions[i as usize])
                .as_dvec3()
    };
    let (mut area, mut plan) = (0.0, 0.0);
    let mut used = vec![false; mesh.positions.len()];
    for t in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [world(t[0]), world(t[1]), world(t[2])];
        let n = (b - a).cross(c - a);
        area += n.length();
        plan += n.z.abs();
        for &i in t {
            used[i as usize] = true;
        }
    }
    if area <= 0.0 || plan > area * UPRIGHT {
        return None;
    }
    let points: Vec<DVec3> = (0..mesh.positions.len() as u32)
        .filter(|&i| used[i as usize])
        .map(world)
        .collect();
    let (low, high) = points
        .iter()
        .fold((f64::MAX, f64::MIN), |(l, h), p| (l.min(p.z), h.max(p.z)));
    if high - low < 1.0 {
        return None;
    }
    let band = (high - low) * 0.25;
    let mean = |it: Vec<&DVec3>| it.iter().copied().sum::<DVec3>() / it.len().max(1) as f64;
    let feet: Vec<&DVec3> = points.iter().filter(|p| p.z <= low + band).collect();
    let tops: Vec<&DVec3> = points.iter().filter(|p| p.z >= high - band).collect();
    // (the foot is the sheet's bottom edge; where the mesh shows none, its lowest quarter)
    let bottom = bottom_edge(mesh, &world);
    let ([a, b], wide) = farthest_pair(&bottom.iter().collect::<Vec<_>>())
        .filter(|(_, wide)| *wide >= 1.0)
        .or_else(|| farthest_pair(&feet))?;
    if wide < 1.0 {
        return None;
    }
    let lean = (mean(tops) - mean(feet)).truncate();
    Some(Portal {
        foot: [a, b],
        top: high,
        lean,
    })
}

/// The ends of points that lie along one line in plan, and how far apart they are: the point
/// farthest from any of them is one end, and the point farthest from that one is the other.
fn farthest_pair(points: &[&DVec3]) -> Option<([DVec3; 2], f64)> {
    let farthest = |from: DVec3| {
        points.iter().copied().fold((from, 0.0), |(best, wide), p| {
            let d = (*p - from).truncate().length();
            if d > wide { (*p, d) } else { (best, wide) }
        })
    };
    let a = farthest(**points.first()?).0;
    let (b, wide) = farthest(a);
    Some(([a, b], wide))
}

/// The points of the sheet's bottom edge: the ends of each edge on the mesh's rim (in one
/// triangle only) whose triangle rises from it, not the sides or the top. A leaning sheet's
/// lowest quarter is a strip in plan, and its farthest points lie on opposite sides of the
/// strip; its bottom edge is a line. Edges are matched by where their ends are, so a vertex
/// split for its texture does not make a rim inside the sheet.
fn bottom_edge(mesh: &MeshData, world: &dyn Fn(u32) -> DVec3) -> Vec<DVec3> {
    let key = |i: u32| mesh.positions[i as usize].to_array().map(f32::to_bits);
    let mut edges: std::collections::HashMap<_, (u32, u32, u32, u32)> =
        std::collections::HashMap::with_capacity(mesh.indices.len());
    for t in mesh.indices.chunks_exact(3) {
        for (a, b, c) in [(t[0], t[1], t[2]), (t[1], t[2], t[0]), (t[2], t[0], t[1])] {
            let (ka, kb) = (key(a), key(b));
            let k = if ka < kb { (ka, kb) } else { (kb, ka) };
            edges.entry(k).or_insert((a, b, c, 0)).3 += 1;
        }
    }
    let mut out = Vec::new();
    for &(a, b, c, n) in edges.values() {
        let [pa, pb, pc] = [world(a), world(b), world(c)];
        let e = pb - pa;
        if n != 1 || e.length_squared() <= 0.0 {
            continue;
        }
        // (the way into the triangle from its edge, within its plane)
        let inward = (pc - pa) - e * ((pc - pa).dot(e) / e.length_squared());
        if inward.normalize_or_zero().z > 0.5 {
            out.extend([pa, pb]);
        }
    }
    out
}

/// The ends of the opening's foot and its middle (plan), and the way into the hill: the way it
/// leans, else towards the higher ground.
fn plan(portal: &Portal, ground: &dyn Fn(DVec2) -> Option<f64>) -> ([DVec2; 3], DVec2) {
    let [a, b] = portal.foot.map(|p| p.truncate());
    let across = (b - a).normalize();
    let behind = DVec2::new(-across.y, across.x);
    let middle = (a + b) * 0.5;
    let leaning = portal.lean.dot(behind);
    let flip = if leaning.abs() > 1e-3 {
        leaning < 0.0
    } else {
        ground(middle - behind * 3.0).unwrap_or(f64::MIN)
            > ground(middle + behind * 3.0).unwrap_or(f64::MIN)
    };
    ([a, middle, b], if flip { -behind } else { behind })
}

/// Whether the whole opening lies under the ground: in front of it the ground stands over its
/// top all across (where the ground is known), so it opens onto nothing.
pub fn portal_is_buried(portal: &Portal, ground: &dyn Fn(DVec2) -> Option<f64>) -> bool {
    let (feet, behind) = plan(portal, ground);
    [1.0, 2.0, 3.0].iter().all(|d| {
        feet.iter()
            .all(|p| ground(*p - behind * *d).is_some_and(|z| z > portal.top))
    })
}

/// The outline (world x, y) of the ground to cut behind the opening. `ground`: the height of the
/// ground at a place, None where it is not known.
pub fn portal_outline(portal: &Portal, ground: &dyn Fn(DVec2) -> Option<f64>) -> Vec<DVec2> {
    let ([a, middle, b], behind) = plan(portal, ground);
    let over = |d: f64| {
        [a, middle, b]
            .iter()
            .all(|p| ground(*p + behind * d).is_some_and(|z| z >= portal.top + CLEARANCE))
    };
    let known = |d: f64| {
        [a, middle, b]
            .iter()
            .all(|p| ground(*p + behind * d).is_some())
    };
    let mut depth = 1.0;
    while depth < MAX_DEPTH && known(depth) && !over(depth) {
        depth += 1.0;
    }
    vec![
        a - behind * FORE,
        b - behind * FORE,
        b + behind * depth,
        a + behind * depth,
    ]
}

/// Where no wall may stand in front of an opening, from a cut outline of `portal_outline`: the
/// track laid into a cutting ends at the mouth, and the wall along the end of its hole, up to
/// the slopes of the cutting, would stand across the opening.
pub fn portal_clearing(outline: &[DVec2]) -> Vec<DVec2> {
    let [a, b, c, d] = [outline[0], outline[1], outline[2], outline[3]];
    let out = (a - d).normalize() * CLEARING;
    vec![a + out, b + out, c, d]
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    /// A sheet 7 m wide and 4.75 m high leaning 0.7 m, as `Tramtunnel_Loch.o3d`.
    fn sheet() -> MeshData {
        MeshData {
            positions: vec![
                Vec3::new(-3.5, 0.2, 0.0),
                Vec3::new(3.5, 0.2, 0.0),
                Vec3::new(3.5, 0.9, 4.75),
                Vec3::new(-3.5, 0.9, 4.75),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            ..Default::default()
        }
    }

    #[test]
    fn a_sheet_standing_up_is_an_opening_and_a_hole_in_the_ground_is_not() {
        let p =
            hole_portal(&sheet(), &Mat4::IDENTITY, DVec3::new(100.0, 50.0, -1.0)).expect("upright");
        assert!((p.top - 3.75).abs() < 1e-5);
        assert!(
            p.foot
                .iter()
                .all(|f| (f.z + 1.0).abs() < 1e-5 && (f.y - 50.2).abs() < 1e-5)
        );
        assert!(p.lean.y > 0.0);
        let flat = MeshData {
            positions: vec![
                Vec3::new(-3.0, 0.0, 0.0),
                Vec3::new(3.0, 0.0, 0.0),
                Vec3::new(3.0, 8.0, 0.3),
                Vec3::new(-3.0, 8.0, 0.3),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            ..Default::default()
        };
        assert_eq!(hole_portal(&flat, &Mat4::IDENTITY, DVec3::ZERO), None);
    }

    #[test]
    fn an_opening_wholly_under_the_ground_is_buried_and_a_tunnel_mouth_is_not() {
        let p = hole_portal(&sheet(), &Mat4::IDENTITY, DVec3::ZERO).unwrap();
        assert!(portal_is_buried(&p, &|_| Some(30.0)));
        assert!(
            !portal_is_buried(&p, &|q| Some(if q.y > 0.2 { 20.0 } else { 0.0 })),
            "a cutting in front, a high hill behind"
        );
        assert!(!portal_is_buried(&p, &|q| Some((q.y - 0.2) * 0.5)));
        assert!(!portal_is_buried(&p, &|_| None), "the ground not known");
    }

    #[test]
    fn the_cut_goes_in_behind_the_opening_until_the_hill_is_over_it() {
        let p = hole_portal(&sheet(), &Mat4::IDENTITY, DVec3::ZERO).unwrap();
        let ring = portal_outline(&p, &|q| Some((q.y - 0.2) * 0.5));
        let ys: Vec<f64> = ring.iter().map(|q| q.y).collect();
        assert!(
            (ys[0] - 0.0).abs() < 1e-5,
            "from just in front of it: {ys:?}"
        );
        assert!(
            (ys[2] - 10.2).abs() < 1e-5,
            "to where the ground is 4.95 m up: {ys:?}"
        );
    }

    #[test]
    fn no_hill_leaves_a_short_cut_of_limited_depth() {
        let p = hole_portal(&sheet(), &Mat4::IDENTITY, DVec3::ZERO).unwrap();
        assert!(
            (portal_outline(&p, &|_| Some(9.0))[2].y - 1.2).abs() < 1e-5,
            "the hill is over it from the start"
        );
        assert!(
            (portal_outline(&p, &|_| Some(0.0))[2].y - 30.2).abs() < 1e-5,
            "flat ground: the limit"
        );
        assert!(
            (portal_outline(&p, &|q| (q.y < 5.0).then_some(0.0))[2].y - 5.2).abs() < 1e-5,
            "the tile not loaded"
        );
    }

    #[test]
    fn the_clearing_reaches_out_of_the_mouth() {
        let p = hole_portal(&sheet(), &Mat4::IDENTITY, DVec3::ZERO).unwrap();
        let ring = portal_outline(&p, &|q| Some((q.y - 0.2) * 0.5));
        let clearing = portal_clearing(&ring);
        assert!(
            (clearing[0].y + 3.0).abs() < 1e-5 && (clearing[1].y + 3.0).abs() < 1e-5,
            "{clearing:?}"
        );
        assert_eq!((clearing[2], clearing[3]), (ring[2], ring[3]));
    }

    /// A hole mesh made of many small faces, as a detailed portal is: the foot's ends are its
    /// outer corners, each vertex shared by up to six triangles counted once, found in linear
    /// work (a grid of 100 000 vertices; comparing every pair of the foot's would take some
    /// 10^9 steps).
    /// A sheet of `across` × `up` vertices, each at `at(x, z)` for x and z from 0 to 1.
    fn grid(across: usize, up: usize, at: impl Fn(f32, f32) -> Vec3) -> MeshData {
        let positions = (0..up)
            .flat_map(|j| {
                (0..across)
                    .map(move |i| (i as f32 / (across - 1) as f32, j as f32 / (up - 1) as f32))
            })
            .map(|(x, z)| at(x, z))
            .collect();
        let at = |i: usize, j: usize| (j * across + i) as u32;
        let indices = (0..up - 1)
            .flat_map(|j| {
                (0..across - 1).flat_map(move |i| {
                    [
                        at(i, j),
                        at(i + 1, j),
                        at(i + 1, j + 1),
                        at(i, j),
                        at(i + 1, j + 1),
                        at(i, j + 1),
                    ]
                })
            })
            .collect();
        MeshData {
            positions,
            indices,
            ..Default::default()
        }
    }

    /// Whether the foot's two ends are `a` and `b` (in plan, either way round).
    fn foot_is(p: &Portal, a: DVec2, b: DVec2) -> bool {
        let [f, g] = p.foot.map(|f| f.truncate());
        let near = |u: DVec2, v: DVec2| u.distance(v) < 1e-4;
        (near(f, a) && near(g, b)) || (near(f, b) && near(g, a))
    }

    #[test]
    fn a_subdivided_sheet_finds_the_ends_of_its_foot() {
        let mesh = grid(2000, 50, |x, z| {
            Vec3::new(-3.5 + 7.0 * x, 0.2 + 0.7 * z, 4.75 * z)
        });
        let p = hole_portal(&mesh, &Mat4::IDENTITY, DVec3::ZERO).expect("upright");
        assert!(
            foot_is(&p, DVec2::new(-3.5, 0.2), DVec2::new(3.5, 0.2)),
            "{:?}",
            p.foot
        );
        assert!(p.foot.iter().all(|f| f.z.abs() < 1e-5), "{:?}", p.foot);
        assert!((p.top - 4.75).abs() < 1e-5 && p.lean.y > 0.0);
    }

    #[test]
    fn a_leaning_sheet_has_its_foot_on_its_bottom_edge() {
        // (leaning 2 m over its 4.75 m, turned 30° in plan: its lowest quarter is a strip
        // 0.5 m deep, and the corners of that strip lie 7.02 m apart, wider than the foot)
        let turn = Mat4::from_rotation_z(30f32.to_radians());
        let mesh = grid(40, 20, |x, z| Vec3::new(-3.5 + 7.0 * x, 2.0 * z, 4.75 * z));
        let p = hole_portal(&mesh, &turn, DVec3::new(100.0, 50.0, 0.0)).expect("upright");
        let corner = |x: f32| {
            DVec2::new(100.0, 50.0)
                + turn
                    .transform_point3(Vec3::new(x, 0.0, 0.0))
                    .truncate()
                    .as_dvec2()
        };
        assert!(foot_is(&p, corner(-3.5), corner(3.5)), "{:?}", p.foot);
        let outline = portal_outline(&p, &|_| Some(0.0));
        assert!(
            ((outline[0] - outline[1]).length() - 7.0).abs() < 1e-3,
            "the cut is as wide as the mouth: {outline:?}"
        );
    }

    #[test]
    fn a_sheet_with_a_sloping_bottom_has_its_foot_along_its_bottom_edge() {
        // (its bottom falls 1 m from left to right, so only one corner is at the lowest height)
        let mesh = grid(30, 20, |x, z| {
            Vec3::new(-3.5 + 7.0 * x, 0.2 + 1.0 * z, 5.0 * z - x * (1.0 - z))
        });
        let p = hole_portal(&mesh, &Mat4::IDENTITY, DVec3::ZERO).expect("upright");
        assert!(
            foot_is(&p, DVec2::new(-3.5, 0.2), DVec2::new(3.5, 0.2)),
            "{:?}",
            p.foot
        );
    }

    #[test]
    fn a_plumb_sheet_opens_towards_the_hill() {
        let mesh = MeshData {
            positions: vec![
                Vec3::new(-3.5, 0.0, 0.0),
                Vec3::new(3.5, 0.0, 0.0),
                Vec3::new(3.5, 0.0, 4.75),
                Vec3::new(-3.5, 0.0, 4.75),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            ..Default::default()
        };
        let p = hole_portal(&mesh, &Mat4::IDENTITY, DVec3::ZERO).unwrap();
        assert!(
            portal_outline(&p, &|q| Some(-q.y))[2].y < 0.0,
            "the hill to the south"
        );
        assert!(
            portal_outline(&p, &|q| Some(q.y))[2].y > 0.0,
            "the hill to the north"
        );
    }
}
