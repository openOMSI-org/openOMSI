//! Traffic path network and AI road vehicles (original units `mc_path`, `mc_pathrule`,
//! the AI part of `mc_roadvehicle`).
//!
//! Lanes come from `[path]` entries of splines (running along the spline at a lateral
//! offset) and of scenery objects (arcs in the object's frame). Lane ends are linked by
//! proximity, which reproduces the original's spline `prev`/`next` chains and the object
//! `[splinehelper]` connectors without needing their ids.

use glam::{DVec2, DVec3};
use hashbrown::HashMap;

mod ai_state;
mod chain_load;
mod extend;
mod lane;
mod lights;
mod network;
mod rail_points;
pub use rail_points::{next_branch, settable_branches};
#[cfg(test)]
mod extend_tests;
mod walk_path;
pub mod way;

#[cfg(test)]
use ai_state::smooth01;
pub use ai_state::{
    AiState, BODY_MAX_LEAD, BODY_SLACK, BodyLink, EMERGENCY_LIMIT_FACTOR, EMERGENCY_LIMIT_PLUS,
    Envelope, JERK, LaneChange, Lead, MAX_BRAKE, ORDINARY_BRAKE, SIGNAL_BEFORE_CHANGE,
    arrival_time, ramp_progress_for,
};
pub use chain_load::{LaneLoad, NO_DENSITY_FILL, load_group};
pub use extend::LINK_TOLERANCE;
#[cfg(test)]
use lane::heading_dir;
pub use lane::{
    AIR_NO_LIMIT_KMH, DEFAULT_PRIORITY, Lane, LaneBuilder, LaneKey, LaneKind, arc_point,
    pool_density, wrap_deg,
};
pub use lights::{
    Aspect, DEFAULT_APPROACH, LightProgram, LightStop, PLAYER_SEARCH_RANGE, TimedProgram,
    TrafficLightController, UNLINKED_PHASE, WAITING_AT_LINE,
};
pub use walk_path::PathFooting;

/// The whole network.
#[derive(Default)]
pub struct Network {
    pub lanes: Vec<Lane>,
    /// Lanes by map identity (both directions of a two-way path share a key).
    pub by_key: HashMap<LaneKey, Vec<usize>>,
    /// Per lane: lanes of the same crossing object whose geometry crosses or merges into it.
    pub conflicts: Vec<Vec<usize>>,
    /// Per lane: the lanes that lead into it (the reverse of `next`), for walking a
    /// one-way path backwards.
    pub prev: Vec<Vec<usize>>,
    /// Lanes by 50 m grid cell (every cell a lane's points touch), so that a nearest-lane
    /// query looks at a handful of lanes instead of all 17 000 of Spandau.
    pub grid: HashMap<(i32, i32), Vec<usize>>,
    /// Lanes by the cell containing their start. Population only needs lane starts near
    /// a viewer; the geometry grid above can contain the same long lane in many cells.
    pub start_grid: HashMap<(i32, i32), Vec<usize>>,
    /// Per street lane: where the other street and rail lanes of its junction object cross
    /// it or run into its end (`conflicts` with the places).
    pub crossings: Vec<Vec<Crossing>>,
    /// Per street lane: the footpaths of its junction object that cross it (zebra and
    /// signalled crossings): (footpath lane, distance along the street lane, along the path).
    pub walks: Vec<Vec<(usize, f32, f32)>>,
    /// Per lane: how far a vehicle can drive from its start before the network ends (m, at
    /// most `REACH_MAX`; lanes closed to cars do not count as a way on).
    pub reach: Vec<f32>,
    /// The map drives on the left (`global.cfg` `[lht]`): priority to the left, the
    /// oncoming lane on the right, turning right across the oncoming traffic.
    pub left_hand: bool,
    /// The AI cars of each traffic group on each lane this step (see `chain_load`): how
    /// full the ways on from a fork are.
    pub load: LaneLoad,
}

/// `Network::reach` is counted up to this far (m).
pub const REACH_MAX: f32 = 600.0;
/// A way on that ends within this distance is a dead end to a driver who has a choice (m).
pub const DEAD_END: f32 = 500.0;

/// Where two lanes of a junction meet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crossing {
    pub other: usize,
    /// Distance along this lane and along the other one to the meeting point.
    pub at: f32,
    pub other_at: f32,
    /// Both lanes end in the same place (one road runs into the other) rather than crossing.
    pub merge: bool,
    /// The meeting place: how far before and after `at` this lane's centre line stays
    /// closer than `MEET_DIST` to the other's (where two vehicles on them would touch),
    /// and the same along the other lane. Two paths that cross at a shallow angle, or two
    /// turns that bend towards each other, meet over metres, not at a point.
    pub before: f32,
    pub after: f32,
    pub other_before: f32,
    pub other_after: f32,
}

/// Two vehicles whose centre lines come closer than this (m) touch: the half widths of two
/// buses.
pub const MEET_DIST: f64 = 2.6;
/// A meeting place reaches at most this far either side of the crossing point (m).
const MEET_MAX: f32 = 14.0;

/// How far before and after distance `at` of lane `a` its centre line stays within
/// `MEET_DIST` of lane `b`'s (in half-metre steps, at least the half metre around the point).
fn meeting_extent(a: &Lane, at: f32, b: &Lane) -> (f32, f32) {
    let near = |s: f32| {
        b.nearest_point(a.at(s).0)
            .map(|(_, d)| d < MEET_DIST)
            .unwrap_or(false)
    };
    let mut before = 0.5f32;
    while before < MEET_MAX && at - before >= 0.0 && near(at - before) {
        before += 0.5;
    }
    let mut after = 0.5f32;
    while after < MEET_MAX && at + after <= a.length() && near(at + after) {
        after += 0.5;
    }
    (before, after)
}

/// Where two polylines cross: the distance along each (ends excluded).
fn polyline_crossing(a: &Lane, b: &Lane) -> Option<(f32, f32)> {
    for (i, pa) in a.points.windows(2).enumerate() {
        for (j, pb) in b.points.windows(2).enumerate() {
            let (p, r) = (pa[0].truncate(), (pa[1] - pa[0]).truncate());
            let (q, s) = (pb[0].truncate(), (pb[1] - pb[0]).truncate());
            let den = r.perp_dot(s);
            if den.abs() < 1e-9 {
                continue;
            }
            let t = (q - p).perp_dot(s) / den;
            let u = (q - p).perp_dot(r) / den;
            if !(0.0..=1.0).contains(&t) || !(0.0..=1.0).contains(&u) {
                continue;
            }
            let sa = a.dist[i] + (a.dist[i + 1] - a.dist[i]) * t as f32;
            let sb = b.dist[j] + (b.dist[j + 1] - b.dist[j]) * u as f32;
            // meeting at an end is a joint (or a fork), not a crossing
            let inner = |s: f32, l: &Lane| s > 0.3 && s < l.length() - 0.3;
            if inner(sa, a) && inner(sb, b) {
                return Some((sa, sb));
            }
        }
    }
    None
}

/// Where two lanes of a junction that do not cross pass so close that two vehicles on
/// them would touch (centre lines nearer than `NEAR_PASS`): two turns from opposite sides
/// sweeping past each other in the middle of a junction. The distance along each at their
/// closest, away from the ends (a fork or a joint is not such a place). Without it the two
/// cars met there with nothing to sort out who goes first.
fn near_pass(a: &Lane, b: &Lane) -> Option<(f32, f32)> {
    if (a.start() - b.start()).truncate().length() < 3.0
        || (a.end() - b.end()).truncate().length() < 3.0
    {
        return None;
    }
    let mut best: Option<(f64, f32, f32)> = None;
    for (k, p) in a.points.iter().enumerate() {
        let sa = a.dist[k];
        if sa < 1.0 || sa > a.length() - 1.0 {
            continue;
        }
        let Some((sb, d)) = b.nearest_point(*p) else {
            continue;
        };
        if sb < 1.0 || sb > b.length() - 1.0 {
            continue;
        }
        if best.is_none_or(|x| d < x.0) {
            best = Some((d, sa, sb));
        }
    }
    best.filter(|x| x.0 < NEAR_PASS).map(|x| (x.1, x.2))
}

/// Centre lines of two lanes closer than this pass too close for two cars (m).
pub const NEAR_PASS: f64 = 2.0;

/// Grid cell size of `Network::grid` (m).
pub const GRID_CELL: f64 = 50.0;

mod rings;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod light_tests {
    use super::TrafficLightController;

    #[test]
    fn remaining_green_of_a_pedestrian_light() {
        let at = |mut c: TrafficLightController, t: f64| {
            c.time = t;
            c
        };
        // Einm_Stresow_Obermeier "Main_Ped": green 8 s, then red to the end of a 38 s cycle
        let ped = TrafficLightController::new(vec![vec![(6, 8.0), (0, 0.0)]], 38.0);
        let c = at(ped.clone(), 3.0);
        assert_eq!(c.state(0), 6);
        assert!((c.remaining(0) - 5.0).abs() < 1e-4);
        let c = at(ped, 20.0);
        assert_eq!(c.state(0), 0);
        assert!((c.remaining(0) - 18.0).abs() < 1e-4);
        // red running over the end of the cycle into a red start
        let d = at(
            TrafficLightController::new(
                vec![vec![(0, 19.0), (3, 2.0), (6, 11.0), (9, 3.0), (0, 0.0)]],
                38.0,
            ),
            36.0,
        );
        assert!((d.remaining(0) - 21.0).abs() < 1e-4, "{}", d.remaining(0));
        assert!(
            at(TrafficLightController::new(vec![vec![(6, 5.0)]], 0.0), 1.0)
                .remaining(0)
                .is_infinite()
        );
    }
}

#[cfg(test)]
mod aurora_pool_tests {
    use super::*;
    #[test]
    fn independent_rules_and_default_references() {
        let mut lane = LaneBuilder::arc(DVec3::ZERO, 0.0, 100.0, 0.0, 0.0, LaneKind::Street, 3.0);
        lane.group_density = vec![(0, 1.0), (1, 0.1), (3, 0.001), (5, 0.0)];
        let defaults = [1, 0, 1, 0, 0, 0];
        assert_eq!(lane.pool_density(&defaults, 1), 0.1);
        assert_eq!(lane.pool_density(&defaults, 2), 1.0);
        assert_eq!(lane.pool_density(&defaults, 3), 0.001);
        assert_eq!(lane.pool_density(&defaults, 4), 0.0);
        assert_eq!(lane.pool_density(&defaults, 5), 0.0);
        lane.group_density.push((1, 0.5));
        assert_eq!(lane.pool_density(&defaults, 1), 0.5);
    }
}
