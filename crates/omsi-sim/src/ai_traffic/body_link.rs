//! The body owns the pose: after every step the planner looks for where its car's body
//! really stands on the way it laid out, and keeps to it. The planner integrates its own
//! place along the lanes and the body (`ai_motion::AiBody`) follows that place; nothing
//! told the planner when the body could not follow - held back by a wall it was not let
//! into, pushed back by a blow, standing off its way after a recovery - and the plan ran on
//! along the lanes, deciding everything for a place the car was not at, until the body was
//! eight metres behind and was put back onto the way by force.
//!
//! The body's rotation point is projected onto a short window of the planner's way (the
//! same curve the steering follows, with its joints smoothed, a lane change blended and a
//! bay's offset in it: during a lane change both lanes' places are reconciled together, as
//! the way blends them). The projection is refused where the body is too far beside the
//! way, turned too far from it, or on another level - a road beside or under the car's own
//! does not take the body over. What it finds goes to the planner as `AiState::body`: the
//! command starts from the body's speed while the body lags, and the plan waits for the
//! body `BODY_MAX_LEAD` metres ahead of it (`AiState::drive`).

use super::*;
use crate::traffic::{BodyLink, Network};

/// The window of the way the body is looked for in, behind and ahead of the planner's
/// place (m). A body further off than this is put back onto its way by `AiBody::step`.
pub const WINDOW_BACK: f32 = 10.0;
pub const WINDOW_AHEAD: f32 = 4.0;
/// Spacing of the way's samples the window is searched by (m).
const SAMPLE: f32 = 0.5;
/// The body is refused as off its way further than this beyond the edge of its lane (m),
/// turned more than `MAX_TURN` degrees from the way there, or more than `MAX_RISE` metres
/// above or below it.
pub const BEYOND_EDGE: f32 = 1.0;
pub const MAX_TURN: f64 = 60.0;
pub const MAX_RISE: f64 = 2.0;

/// Where a body stands on its planner's way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    /// Distance along the way from the planner's place to the body's (m, negative behind).
    pub along: f32,
    /// How far beside the way (m), how far the body is turned from it (deg) and how far
    /// above or below it its origin stands (m).
    pub side: f32,
    pub turn: f64,
    pub rise: f64,
    /// Within the limits: the body stands on its way.
    pub on_way: bool,
}

/// Where `body` stands on the way `state` lays out over `net` (None: nowhere within the
/// window). `half_lane` is half the width of the car's lane.
pub fn project(
    state: &AiState,
    net: &Network,
    body: &AiBody,
    half_lane: f32,
) -> Option<Projection> {
    let rot = body.rotation_long();
    let at = body.rotation_at();
    let point = |d: f32| state.way_point(net, rot + d);
    // the nearest sample, then the nearest point of the pieces on either side of it
    let n = ((WINDOW_BACK + WINDOW_AHEAD) / SAMPLE).round() as usize;
    let mut best = (f64::MAX, 0usize);
    let samples: Vec<DVec3> = (0..=n)
        .map(|k| point(-WINDOW_BACK + k as f32 * SAMPLE))
        .collect();
    for (k, p) in samples.iter().enumerate() {
        let d = (p.truncate() - at).length_squared();
        if d < best.0 {
            best = (d, k);
        }
    }
    let k = best.1;
    if k == 0 || k == n {
        // (at the window's end: the nearest point of the way lies beyond it)
        return None;
    }
    let mut found: Option<(f64, f32, DVec3, DVec2)> = None;
    for (a, b) in [(k - 1, k), (k, k + 1)] {
        let (pa, pb) = (samples[a], samples[b]);
        let seg = (pb - pa).truncate();
        let len2 = seg.length_squared();
        let u = if len2 > 1e-9 {
            ((at - pa.truncate()).dot(seg) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let q = pa.lerp(pb, u);
        let dist = (q.truncate() - at).length();
        if found.is_none_or(|f| dist < f.0) {
            let along = -WINDOW_BACK + (a as f32 + u as f32) * SAMPLE;
            found = Some((dist, along, q, seg));
        }
    }
    let (_, along, q, seg) = found?;
    let dir = seg.normalize_or_zero();
    let right = DVec2::new(dir.y, -dir.x);
    let side = (at - q.truncate()).dot(right) as f32;
    let way_heading = dir.x.atan2(dir.y).to_degrees();
    let turn = ((body.heading - way_heading + 540.0).rem_euclid(360.0) - 180.0).abs();
    // (the way's height at the rotation point against the body's origin, both on the road)
    let rise = body.position.z - q.z;
    let on_way =
        side.abs() <= half_lane + BEYOND_EDGE && turn <= MAX_TURN && rise.abs() <= MAX_RISE;
    Some(Projection {
        along,
        side,
        turn,
        rise,
        on_way,
    })
}

/// What the planner learns of its body after a step (see the module).
pub fn link(state: &AiState, net: &Network, body: &AiBody) -> BodyLink {
    if body.kind != MotionKind::Road {
        return BodyLink::default();
    }
    let half_lane = net
        .lanes
        .get(state.lane)
        .map(|l| 0.5 * l.width.max(2.5))
        .unwrap_or(1.5);
    match project(state, net, body, half_lane) {
        Some(p) => BodyLink {
            known: true,
            on_way: p.on_way,
            lead: -p.along,
            speed: body.moved,
        },
        None => BodyLink::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::{LaneBuilder, LaneKind};

    fn road() -> Network {
        let mut net = Network {
            lanes: vec![LaneBuilder::arc(
                DVec3::ZERO,
                0.0,
                300.0,
                0.0,
                0.0,
                LaneKind::Street,
                3.0,
            )],
            ..Default::default()
        };
        net.link(1.5);
        net
    }

    fn golf() -> omsi_vehicle::Vehicle {
        use omsi_vehicle::vehicle::Axle;
        let mut v = omsi_vehicle::Vehicle {
            mass: 1.0,
            rot_pnt_long: -1.3,
            inv_min_turn_radius: 0.19,
            ..Default::default()
        };
        v.axles = vec![
            Axle {
                long: 1.2,
                max_width: 1.6,
                ..Default::default()
            },
            Axle {
                long: -1.3,
                max_width: 1.6,
                ..Default::default()
            },
        ];
        v
    }

    /// A body put where its planner is stands on its way with no lead; one held three
    /// metres back is found three metres behind; one beside a parallel road's lane, or
    /// turned across the way, is refused.
    #[test]
    fn a_body_is_found_on_its_way_or_refused() {
        let net = road();
        let mut st = AiState::new(0, 100.0, 1);
        st.plan_next(&net);
        let mut body = AiBody::new(&golf(), MotionKind::Road);
        body.place(&|d| st.way_point(&net, d), None, None, 0.0);
        let p = project(&st, &net, &body, 1.5).unwrap();
        assert!(
            p.on_way && p.along.abs() < 0.01 && p.side.abs() < 0.01,
            "{p:?}"
        );
        let mut back = st.clone();
        back.s = 97.0;
        body.place(&|d| back.way_point(&net, d), None, None, 0.0);
        let p = project(&st, &net, &body, 1.5).unwrap();
        assert!(p.on_way && (p.along + 3.0).abs() < 0.01, "{p:?}");
        // the next road over, 3.5 m to the left
        body.place(
            &|d| back.way_point(&net, d) + DVec3::new(-3.5, 0.0, 0.0),
            None,
            None,
            0.0,
        );
        let p = project(&st, &net, &body, 1.5).unwrap();
        assert!(!p.on_way && (p.side + 3.5).abs() < 0.01, "{p:?}");
        // turned across the road
        body.place(
            &|d| back.way_point(&net, 0.0) + DVec3::new(d as f64, 0.0, 0.0),
            None,
            None,
            0.0,
        );
        let p = project(&st, &net, &body, 1.5).unwrap();
        assert!(!p.on_way && p.turn > 60.0, "{p:?}");
        // a deck 6 m over the road
        body.place(
            &|d| back.way_point(&net, d) + DVec3::new(0.0, 0.0, 6.0),
            None,
            None,
            0.0,
        );
        let p = project(&st, &net, &body, 1.5).unwrap();
        assert!(!p.on_way, "{p:?}");
        // far behind the window: not found
        let mut far = st.clone();
        far.s = 60.0;
        body.place(&|d| far.way_point(&net, d), None, None, 0.0);
        assert!(project(&st, &net, &body, 1.5).is_none());
    }

    /// A planner whose body is held back two metres behind it waits for it: it takes the
    /// body's speed and moves no further ahead of it.
    #[test]
    fn a_plan_waits_for_its_body() {
        let net = road();
        let mut st = AiState::new(0, 100.0, 1);
        st.speed = 10.0;
        st.plan_next(&net);
        st.body = BodyLink {
            known: true,
            on_way: true,
            lead: 1.9,
            speed: 0.0,
        };
        let s0 = st.s;
        st.drive(&net, 0.02, None, None);
        assert!(st.s - s0 <= 0.1 + 1e-4, "ran {} m on", st.s - s0);
        assert!(st.speed <= 0.02 * st.accel + 1e-4, "kept {} m/s", st.speed);
        // and moves on again with its body
        st.body = BodyLink {
            known: true,
            on_way: true,
            lead: 0.0,
            speed: 0.0,
        };
        for _ in 0..50 {
            st.drive(&net, 0.02, None, None);
        }
        assert!(st.speed > 0.3 && st.s > s0 + 0.2);
    }
}
