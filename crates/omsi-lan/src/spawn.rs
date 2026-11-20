//! Where a joining player's bus may stand: the footprints of what stands at a spawn, the
//! overlap test between them (with the gaps kept), and points along the lanes from a place.

use crate::host::LanHost;
use glam::DVec3;
use omsi_net::Footprint;
use omsi_sim::traffic::{Lane, LaneKind, Network};
use omsi_sim::world_data::WorldData;

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

pub fn clear_spawn(
    p: &mut omsi_sim::VehicleInstance,
    occupied: &[Footprint],
    world: &WorldData,
    host: &dyn LanHost,
    net: Option<&Network>,
) -> Option<f64> {
    let me = footprint_of(p, BUS_BOX);
    if !blocked(&me, occupied) {
        log::info!(
            "LAN: our spawn at ({:.1}, {:.1}) is free ({} vehicle(s) nearby)",
            p.position.x,
            p.position.y,
            occupied.len()
        );
        return Some(0.0);
    }
    // the origin sits this far from the box centre (along, across)
    let v = &*p;
    let h0 = v.heading.to_radians();
    let dc = DVec3::new(me.x, me.y, me.z) - v.position;
    let (c_along, c_across) = (
        dc.x * h0.sin() + dc.y * h0.cos(),
        dc.x * h0.cos() - dc.y * h0.sin(),
    );
    let statics = v.collision.clone();
    let free_at = |centre: DVec3, heading: f64| -> Option<DVec3> {
        let fp = Footprint {
            x: centre.x,
            y: centre.y,
            z: centre.z,
            heading: heading as f32,
            length: me.length,
            width: me.width,
        };
        if blocked(&fp, occupied) {
            return None;
        }
        let z = world.ground_height(centre.x, centre.y)?;
        if let Some(cw) = statics.as_ref() {
            let mut b = obb(&fp, false);
            b.z0 = z + 0.3;
            b.z1 = z + 3.0;
            if cw.hit(&b).is_some() {
                return None;
            }
        }
        let h = heading.to_radians();
        Some(DVec3::new(
            centre.x - c_along * h.sin() - c_across * h.cos(),
            centre.y - c_along * h.cos() + c_across * h.sin(),
            z,
        ))
    };
    // the lanes: the traffic network (linked), or the world's own list
    let world_lanes;
    let (lanes, prev): (&[Lane], &[Vec<usize>]) = match net {
        Some(n) if !n.lanes.is_empty() => (&n.lanes, &n.prev),
        _ => {
            world_lanes = host.world_lanes();
            (&world_lanes, &[])
        }
    };
    let centre = DVec3::new(me.x, me.y, me.z);
    let mut best_lane: Option<(usize, f32, f64, bool)> = None;
    for (i, l) in lanes.iter().enumerate() {
        if l.kind != LaneKind::Street || l.points.len() < 2 {
            continue;
        }
        // a lane cannot reach further from its start than its own length
        if (l.points[0] - centre).truncate().length() > l.length() as f64 + 10.0 {
            continue;
        }
        if let Some((s, d)) = l.nearest_point(centre) {
            if d > 6.0 {
                continue;
            }
            let same_way = heading_gap(l.at(s).1, v.heading as f32) < 60.0;
            // a lane running our way beats a closer one running the other way
            let score = d + if same_way { 0.0 } else { 4.0 };
            if best_lane.map(|b| score < b.2).unwrap_or(true) {
                best_lane = Some((i, s, score, same_way));
            }
        }
    }
    let mut placed: Option<(DVec3, f64, f64)> = None;
    // the lane and the place on it where the bus went (for its rear sections)
    let mut on_lane: Option<(usize, f32, f32)> = None;
    if let Some((li, s, _, same_way)) = best_lane {
        let sign = if same_way { 1.0 } else { -1.0 };
        // how much the road turns under the whole bus there: a straight box on a bend
        // stands with its ends off the road (an articulated bus with its rear section on
        // the verge), so a straight stretch a little further off is taken first
        let reach = me.length * 0.5 + 1.0;
        let bend = |c: f32| -> f32 {
            match (
                along(lanes, prev, li, s, c + reach),
                along(lanes, prev, li, s, c - reach),
            ) {
                (Some(a), Some(b)) => heading_gap(a.1, b.1),
                _ => 90.0,
            }
        };
        let mut fallback: Option<(DVec3, f64, f64, f32)> = None;
        'search: for k in 1..=90 {
            let d = k as f32 * 1.5;
            for dd in [-d, d] {
                if let Some((pos, lh)) = along(lanes, prev, li, s, dd * sign) {
                    let heading = if same_way {
                        lh as f64
                    } else {
                        lh as f64 + 180.0
                    }
                    .rem_euclid(360.0);
                    if let Some(origin) = free_at(pos, heading) {
                        if bend(dd * sign) < 12.0 {
                            placed = Some((origin, heading, dd as f64));
                            on_lane = Some((li, s, dd * sign));
                            break 'search;
                        }
                        if fallback.is_none() {
                            fallback = Some((origin, heading, dd as f64, dd * sign));
                        }
                    }
                }
            }
            // a bend-free place more than 60 m away: the nearest free one on the bend
            if d > 60.0 && fallback.is_some() {
                break;
            }
        }
        if placed.is_none() {
            if let Some((o, h, m, at)) = fallback {
                placed = Some((o, h, m));
                on_lane = Some((li, s, at));
            }
        }
    }
    if placed.is_none() {
        // off the road (a depot yard): along our heading, then rows beside it
        let (fwd, right) = (
            DVec3::new(h0.sin(), h0.cos(), 0.0),
            DVec3::new(h0.cos(), -h0.sin(), 0.0),
        );
        'rows: for row in [0.0, 1.0, -1.0, 2.0, -2.0] {
            let side = right * row * (me.width as f64 + GAP_ACROSS + 0.5);
            for k in 0..=40 {
                let d = k as f64 * 1.5;
                for dd in [-d, d] {
                    if let Some(origin) = free_at(centre + side + fwd * dd, v.heading) {
                        placed = Some((origin, v.heading, dd));
                        break 'rows;
                    }
                }
            }
        }
    }
    let Some((origin, heading, moved)) = placed else {
        log::warn!(
            "LAN: no free place found near our spawn at ({:.1}, {:.1}); staying there",
            v.position.x,
            v.position.y
        );
        return None;
    };
    let from = p.position;
    p.position = origin;
    p.heading = heading;
    if let Some(rb) = p.rigid.as_mut() {
        rb.place(origin, heading);
    }
    for t in p.trailers.iter_mut() {
        t.realign();
    }
    // on the road: every rear section's turning axle on the lane behind its coupling, so
    // that it follows the bend instead of standing straight out onto the verge
    if let (Some((li, s, at)), true) = (on_lane, !p.trailers.is_empty()) {
        let back = if heading_gap(lanes[li].at(s).1, heading as f32) < 90.0 {
            -1.0
        } else {
            1.0
        };
        let (lead_pos, lead_rot) = (p.position, p.body_rotation());
        let fwd = DVec3::new(heading.to_radians().sin(), heading.to_radians().cos(), 0.0);
        // (the first rear section; a second one lines up behind it as it moves)
        if let Some(t) = p.trailers.first_mut() {
            let c = t.coupling_point(lead_pos, lead_rot);
            let len = t.pivot_length() as f64;
            // walk back along the lane until the axle is the part's length from the coupling
            let mut k = 0.0f32;
            while k < 60.0 {
                if let Some((q, _)) = along(lanes, prev, li, s, at + back * k) {
                    let q = DVec3::new(q.x, q.y, c.z);
                    if (q - c).truncate().length() >= len && (q - c).dot(fwd) < 0.0 {
                        let dir = (q - c).truncate().normalize_or_zero();
                        t.place_pivot(c + DVec3::new(dir.x, dir.y, 0.0) * len);
                        break;
                    }
                }
                k += 0.25;
            }
        }
    }
    for _ in 0..3 {
        p.update(1.0 / 30.0);
    }
    log::info!(
        "LAN: {} vehicle(s) stand at our spawn; moved our bus {:.1} m {} to ({:.1}, {:.1}, {:.1}) heading {:.0} (from ({:.1}, {:.1}))",
        occupied
            .iter()
            .filter(|o| (DVec3::new(o.x, o.y, o.z) - from).truncate().length() < 30.0)
            .count(),
        moved.abs(),
        if moved < 0.0 { "back" } else { "forward" },
        origin.x,
        origin.y,
        origin.z,
        heading,
        from.x,
        from.y
    );
    Some(moved)
}
