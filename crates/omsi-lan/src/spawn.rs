//! Where a joining player's bus may stand: the footprints of what stands at a spawn, the
//! overlap test between them (with the gaps kept), and points along the lanes from a place.

use glam::DVec3;
use omsi_net::Footprint;
use omsi_sim::traffic::Lane;

/// Room kept free between two buses placed one behind the other (m).
pub const GAP_ALONG: f64 = 3.0;
/// ... and side by side (m).
pub const GAP_ACROSS: f64 = 1.0;

/// The box of a vehicle: centre, heading, length, width.
pub fn footprint_of(v: &omsi_sim::VehicleInstance, fallback: [f32; 6]) -> Footprint {
    let bb = v.ty.def.bounding_box.unwrap_or(fallback);
    let h = v.heading.to_radians();
    let (cx, cy) = (bb[3] as f64, bb[4] as f64);
    let c = v.position
        + DVec3::new(
            cx * h.cos() + cy * h.sin(),
            -cx * h.sin() + cy * h.cos(),
            0.0,
        );
    // an articulated bus reaches back over its rear sections
    let mut length = bb[1];
    let mut centre = c;
    for t in &v.trailers {
        if let Some(tb) = t.ty.def.bounding_box {
            let back = (t.position - v.position).truncate().length() as f32 + tb[1] * 0.5;
            let extra = (back - (bb[1] * 0.5 - bb[4])).max(0.0);
            length += extra;
            centre -= DVec3::new(h.sin(), h.cos(), 0.0) * (extra as f64 * 0.5);
        }
    }
    Footprint {
        x: centre.x,
        y: centre.y,
        z: v.position.z,
        heading: v.heading as f32,
        length,
        width: bb[0],
    }
}

pub const BUS_BOX: [f32; 6] = [2.5, 11.5, 3.0, 0.0, 0.0, 1.5];
pub const CAR_BOX: [f32; 6] = [2.0, 4.5, 1.6, 0.0, 0.0, 0.8];

/// A box for the overlap test, grown by the gaps we want to keep.
pub fn obb(f: &Footprint, grow: bool) -> omsi_sim::collision::Obb {
    let (ga, gc) = if grow {
        (GAP_ALONG * 0.5, GAP_ACROSS * 0.5)
    } else {
        (0.0, 0.0)
    };
    omsi_sim::collision::Obb {
        center: glam::DVec2::new(f.x, f.y),
        half: glam::DVec2::new(f.width as f64 * 0.5 + gc, f.length as f64 * 0.5 + ga),
        heading: (f.heading as f64).to_radians(),
        z0: f.z - 3.0,
        z1: f.z + 4.0,
        velocity: glam::DVec2::ZERO,
        mass: 0.0,
        pole: None,
        id: -1,
    }
}

pub fn blocked(me: &Footprint, occupied: &[Footprint]) -> bool {
    let a = obb(me, true);
    occupied.iter().any(|o| {
        let b = obb(o, true);
        (a.center - b.center).length() <= a.radius() + b.radius() && a.overlaps(&b)
    })
}

/// A point `d` metres along the lanes from (`lane`, `s`), following the links (or the
/// straight continuation where there are none), with the heading there.
pub fn along(
    lanes: &[Lane],
    prev: &[Vec<usize>],
    lane: usize,
    s: f32,
    d: f32,
) -> Option<(DVec3, f32)> {
    let mut li = lane;
    let mut s = s + d;
    for _ in 0..16 {
        let l = &lanes[li];
        let len = l.length();
        if s < 0.0 {
            match prev.get(li).and_then(|p| {
                p.iter()
                    .copied()
                    .filter(|&i| lanes[i].kind == l.kind)
                    .min_by(|&a, &b| {
                        heading_gap(lanes[a].end_heading(), l.start_heading())
                            .total_cmp(&heading_gap(lanes[b].end_heading(), l.start_heading()))
                    })
            }) {
                Some(p) => {
                    s += lanes[p].length();
                    li = p;
                    continue;
                }
                None => {
                    let h = (l.start_heading() as f64).to_radians();
                    return Some((
                        l.start() + DVec3::new(h.sin(), h.cos(), 0.0) * s as f64,
                        l.start_heading(),
                    ));
                }
            }
        }
        if s > len {
            match l
                .next
                .iter()
                .copied()
                .filter(|&i| lanes[i].kind == l.kind)
                .min_by(|&a, &b| {
                    heading_gap(lanes[a].start_heading(), l.end_heading())
                        .total_cmp(&heading_gap(lanes[b].start_heading(), l.end_heading()))
                }) {
                Some(n) => {
                    s -= len;
                    li = n;
                    continue;
                }
                None => {
                    let h = (l.end_heading() as f64).to_radians();
                    return Some((
                        l.end() + DVec3::new(h.sin(), h.cos(), 0.0) * (s - len) as f64,
                        l.end_heading(),
                    ));
                }
            }
        }
        return Some(l.at(s));
    }
    None
}

pub fn heading_gap(a: f32, b: f32) -> f32 {
    ((a - b + 540.0).rem_euclid(360.0) - 180.0).abs()
}
