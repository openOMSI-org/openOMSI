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

/// The upright opening a hole mesh is, if it is one: every face of it mostly upright.
pub fn hole_portal(mesh: &MeshData, transform: &Mat4, origin: DVec3) -> Option<Portal> {
    let world = |i: u32| {
        origin
            + transform
                .transform_point3(mesh.positions[i as usize])
                .as_dvec3()
    };
    let (mut area, mut plan) = (0.0, 0.0);
    let mut points: Vec<DVec3> = Vec::new();
    for t in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [world(t[0]), world(t[1]), world(t[2])];
        let n = (b - a).cross(c - a);
        area += n.length();
        plan += n.z.abs();
        points.extend([a, b, c]);
    }
    if area <= 0.0 || plan > area * UPRIGHT {
        return None;
    }
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
    let (mut a, mut b, mut wide) = (*feet[0], *feet[0], 0.0);
    for p in &feet {
        for q in &feet {
            let d = (**p - **q).truncate().length();
            if d > wide {
                (a, b, wide) = (**p, **q, d);
            }
        }
    }
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
