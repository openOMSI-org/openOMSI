//! Whole traffic scenes run headless, for the ways traffic used to lock up: four cars at
//! a junction without priorities, a queue beyond a junction, a side road at a busy main
//! road, a queue moving off at a green light.

use super::*;
use crate::ai_traffic::setup::RandomTypes;
use crate::traffic::{LaneBuilder, LaneKey};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A vehicle type with no model and no scripts, in a folder of its own.
pub(super) struct Fixture {
    pub(super) dir: std::path::PathBuf,
    pub(super) ty: Arc<VehicleType>,
}

impl Fixture {
    pub(super) fn new() -> Fixture {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "omsi-traffic-scenario-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("car.bus"), "[model]\nmodel.cfg\n").unwrap();
        std::fs::write(dir.join("model.cfg"), "").unwrap();
        let ty = Arc::new(VehicleType::load_ai(&dir, &dir.join("car.bus")).unwrap());
        Fixture { dir, ty }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub(super) fn traffic(f: &Fixture, mut net: Network) -> TrafficSim {
    net.build_grid();
    net.compute_reach();
    let random = RandomTypes { types: Vec::new(), groups: Vec::new(), group_curves: false, group_uvg: Vec::new(), uvg_defaults: Vec::new() };
    TrafficSim::assemble(&f.dir, net, random, Vec::new(), HashMap::new(), (Vec::new(), Vec::new()), Vec::new(), (1.0, 0), 0)
}

pub(super) fn add_car(t: &mut TrafficSim, f: &Fixture, lane: usize, s: f32, seed: u64, speed: Option<f32>) -> u64 {
    let vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
    t.place_car(vehicle, LaneKind::Street, lane, s, f.ty.clone(), seed, None, None, speed, None)
}

pub(super) fn street(start: DVec3, heading: f64, length: f64) -> crate::traffic::Lane {
    LaneBuilder::arc(start, heading, length, 0.0, 0.0, LaneKind::Street, 3.0)
}

/// A junction object (one key, a path per direction) where four straight roads meet,
/// 150 m of road before each of its paths and `exit` m after: lanes 0..4 the roads in
/// (from the south, west, north, east), 4..8 the junction's paths, 8..12 the roads out.
/// Without priorities: whoever comes first goes first.
pub(super) fn crossroads(exit: f64) -> Network {
    let half = 12.0;
    let o = 1.75;
    // (start of the junction path, heading) north, east, south, west bound
    let dirs = [
        (DVec3::new(o, -half, 0.0), 0.0),
        (DVec3::new(-half, -o, 0.0), 90.0),
        (DVec3::new(-o, half, 0.0), 180.0),
        (DVec3::new(half, o, 0.0), 270.0),
    ];
    let back = |p: DVec3, h: f64, d: f64| {
        let r = h.to_radians();
        p - DVec3::new(r.sin() * d, r.cos() * d, 0.0)
    };
    let mut lanes = Vec::new();
    for &(p, h) in &dirs {
        lanes.push(street(back(p, h, 150.0), h, 150.0));
    }
    for (k, &(p, h)) in dirs.iter().enumerate() {
        let mut l = street(p, h, 2.0 * half);
        l.source = 2;
        l.key = Some(LaneKey { tile: (0, 0), id: 1, path: k as u16 });
        lanes.push(l);
    }
    for k in 0..4 {
        let end = lanes[4 + k].end();
        lanes.push(street(end, dirs[k].1, exit));
    }
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    net
}

pub(super) fn run(t: &mut TrafficSim, secs: f32) {
    for _ in 0..(secs / 0.05) as usize {
        t.tick(0.05, None);
    }
}

/// The car's front, metres past the junction's middle along its own road (+ = through).
pub(super) fn past_middle(t: &TrafficSim, id: u64) -> Option<f64> {
    let c = t.cars.iter().find(|c| c.id == id)?;
    let h = c.vehicle.heading.to_radians();
    let fwd = DVec2::new(h.sin(), h.cos());
    Some(c.vehicle.position.truncate().dot(fwd) + c.state.front as f64)
}

#[test]
fn four_cars_at_a_junction_without_priorities_all_get_through() {
    // everybody has somebody on the right: by the rules alone nobody would ever go
    let f = Fixture::new();
    let mut t = traffic(&f, crossroads(150.0));
    let ids: Vec<u64> = (0..4).map(|k| add_car(&mut t, &f, k, 135.0, 0x51 + k as u64 * 0x1000, Some(4.0))).collect();
    run(&mut t, 40.0);
    for id in ids {
        if let Some(p) = past_middle(&t, id) {
            assert!(p > 15.0, "car {id} has got only {p:.1} m past the junction's middle");
        }
    }
}

#[test]
fn a_car_does_not_drive_into_a_junction_it_cannot_leave() {
    // a queue crawls along the road beyond the junction with no room for another car:
    // the car coming up stops at the junction's line, not in its middle
    let f = Fixture::new();
    let mut t = traffic(&f, crossroads(14.0));
    // (the road out ends in 14 m: the queue on it stands at its end)
    let lane_out = 8;
    let front = add_car(&mut t, &f, lane_out, 10.0, 0x77, Some(0.0));
    let second = add_car(&mut t, &f, lane_out, 3.0, 0x78, Some(0.0));
    // the car that comes up from the south
    let me = add_car(&mut t, &f, 0, 90.0, 0x79, Some(8.0));
    for _ in 0..(30.0 / 0.05) as usize {
        // the two ahead stay where they are (a queue that does not move on)
        for c in t.cars.iter_mut().filter(|c| c.id == front || c.id == second) {
            c.state.speed = 0.0;
            c.state.max_speed_kmh = 0.0;
        }
        t.tick(0.05, None);
    }
    let c = t.cars.iter().find(|c| c.id == me).expect("still there");
    assert!(c.state.lane == 0, "the car drove into the junction (lane {} s {:.1})", c.state.lane, c.state.s);
    assert!(c.state.speed < 0.1, "it stands at the line ({:.1} m/s)", c.state.speed);
}

#[test]
fn the_exit_counts_a_crawling_queue_where_it_will_stop() {
    let way = [(10, -20.0), (20, 5.0), (21, 15.0)];
    // the last car of the queue 4 m beyond the exit's start, crawling at 1.8 m/s (it will
    // be 0.8 m further on when it has stopped): no room for 7 m
    assert!(queued_exit_vehicle(&way, (20, 5.0), 7.0, [(20, 4.0, 1.8)]).is_some());
    // driving off at 8 m/s: room
    assert!(queued_exit_vehicle(&way, (20, 5.0), 7.0, [(20, 4.0, 8.0)]).is_none());
}

/// A main road west to east (lane 0 in, the junction's path 1, lane 2 out) and a side road
/// from the south (lane 3) whose path through the junction (4) crosses it, `[rule]`
/// priorities as the stock maps set them.
pub(super) fn side_road() -> Network {
    let key = |path: u16| Some(LaneKey { tile: (0, 0), id: 2, path });
    let main_in = street(DVec3::new(-200.0, 0.0, 0.0), 90.0, 190.0);
    let mut main_j = street(DVec3::new(-10.0, 0.0, 0.0), 90.0, 20.0);
    let main_out = street(DVec3::new(10.0, 0.0, 0.0), 90.0, 300.0);
    let side_in = street(DVec3::new(0.0, -110.0, 0.0), 0.0, 100.0);
    let mut side_j = street(DVec3::new(0.0, -10.0, 0.0), 0.0, 20.0);
    let side_out = street(DVec3::new(0.0, 10.0, 0.0), 0.0, 300.0);
    main_j.source = 2;
    main_j.key = key(0);
    main_j.priority = 192.0;
    side_j.source = 2;
    side_j.key = key(1);
    side_j.priority = 64.0;
    let mut net = Network { lanes: vec![main_in, main_j, main_out, side_in, side_j, side_out], ..Default::default() };
    net.link(1.5);
    net
}

/// When the side road's car (waiting at its line) gets across a main road whose queue
/// crawls past at `v` m/s, a new car put on whenever the last has moved `spacing` m on
/// (None: not within `secs`).
fn side_road_crossing(v: f32, spacing: f32, secs: f32) -> Option<f32> {
    let f = Fixture::new();
    let mut t = traffic(&f, side_road());
    let me = add_car(&mut t, &f, 3, 80.0, 0x99, Some(0.0));
    let mut seed = 0x1234;
    // the queue along the whole main road already
    for (lane, len) in [(0usize, 190.0f32), (1, 20.0), (2, 300.0)] {
        let mut s = 3.0;
        while s < len - 3.0 {
            seed += 0x101;
            add_car(&mut t, &f, lane, s, seed, Some(v));
            s += spacing;
        }
    }
    let mut time = 0.0f32;
    while time < secs {
        if !t.cars.iter().any(|c| c.state.lane == 0 && c.state.s < spacing) {
            seed += 0x101;
            add_car(&mut t, &f, 0, 2.0, seed, Some(v));
        }
        for c in t.cars.iter_mut().filter(|c| c.id != me) {
            c.state.max_speed_kmh = v * 3.6;
        }
        t.tick(0.05, None);
        time += 0.05;
        if t.cars.iter().find(|c| c.id == me).is_none_or(|c| c.state.lane == 5 || (c.state.lane == 4 && c.state.s > 15.0)) {
            return Some(time);
        }
    }
    None
}

#[test]
fn a_side_road_car_gets_into_a_main_road_that_never_leaves_a_gap() {
    // a queue rolling past at 3 or 5 m/s, a car every 9 m: never a gap any driver takes.
    // After its long wait the queue's drivers let the side road's car in (`lets_in`; it
    // used to stand there for good)
    for v in [3.0, 5.0] {
        let at = side_road_crossing(v, 9.0, 150.0);
        assert!(at.is_some_and(|t| t < COURTESY_WAIT + 40.0), "queue at {v} m/s: across after {at:?} s");
    }
    // a queue that crawls (1-1.5 m/s) leaves room enough between its cars
    assert!(side_road_crossing(1.5, 7.5, 150.0).is_some_and(|t| t < 40.0));
}

#[test]
fn a_queue_moves_off_without_closing_up_or_braking_hard() {
    // ten cars standing nose to tail move off together (the Intelligent Driver Model):
    // nobody runs into the one ahead, nobody brakes hard, and the queue is on its way
    let f = Fixture::new();
    let mut lanes = vec![street(DVec3::ZERO, 0.0, 900.0)];
    lanes[0].speed_limit_kmh = 50.0;
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    let mut t = traffic(&f, net);
    let ids: Vec<u64> = (0..10).map(|k| add_car(&mut t, &f, 0, 200.0 - k as f32 * 7.0, 0x300 + k as u64 * 0x77, Some(0.0))).collect();
    let mut min_gap = f32::MAX;
    let mut hardest = 0.0f32;
    for _ in 0..(40.0 / 0.05) as usize {
        t.tick(0.05, None);
        for w in ids.windows(2) {
            let (Some(a), Some(b)) = (t.cars.iter().find(|c| c.id == w[0]), t.cars.iter().find(|c| c.id == w[1])) else { continue };
            min_gap = min_gap.min(a.state.s - a.state.rear - (b.state.s + b.state.front));
        }
        hardest = hardest.min(t.cars.iter().map(|c| c.state.acc).fold(0.0, f32::min));
    }
    assert!(min_gap > 1.0, "two cars came within {min_gap:.2} m");
    assert!(hardest > -3.0, "a car braked at {hardest:.1} m/s²");
    for id in ids {
        let c = t.cars.iter().find(|c| c.id == id).unwrap();
        assert!(c.state.speed > 8.0, "car {id} at {:.1} m/s after 40 s", c.state.speed);
    }
}

#[test]
fn a_car_does_not_change_lanes_into_the_players_bus() {
    // two lanes north, the car on the right one at 100 m; the player's bus on the left one
    let f = Fixture::new();
    let lanes = vec![street(DVec3::ZERO, 0.0, 300.0), street(DVec3::new(-3.5, 0.0, 0.0), 0.0, 300.0)];
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    let mut t = traffic(&f, net);
    let id = add_car(&mut t, &f, 0, 100.0, 0x42, Some(10.0));
    let i = t.cars.iter().position(|c| c.id == id).unwrap();
    let bus_at = |y: f64, v: f32| -> PlayerBox { (DVec3::new(-3.5, y, 0.0), 0.0, 6.0, 1.25, v) };
    // beside it, or coming up fast just behind: not into that lane
    for (y, v) in [(100.0, 10.0), (88.0, 14.0)] {
        t.player = Some(bus_at(y, v));
        assert!(!t.players_let_in(i, 1, 100.0), "bus at {y} m, {v} m/s");
    }
    // far behind, or well ahead and faster: room
    for (y, v) in [(30.0, 10.0), (140.0, 12.0)] {
        t.player = Some(bus_at(y, v));
        assert!(t.players_let_in(i, 1, 100.0), "bus at {y} m, {v} m/s");
    }
}

/// When the car on a lane that runs into another (lane 1 joining lane 0, both going on as
/// lane 2) gets onto the lane beyond the joint while lane 0's queue rolls past at `v` m/s,
/// a car every `spacing` m (None: not within `secs`).
fn zip_merge(v: f32, spacing: f32, secs: f32) -> Option<f32> {
    let f = Fixture::new();
    let main = LaneBuilder::polyline(vec![DVec3::ZERO, DVec3::new(0.0, 150.0, 0.0)], LaneKind::Street, 3.0);
    let side = LaneBuilder::polyline(vec![DVec3::new(17.32, 120.0, 0.0), DVec3::new(0.0, 150.0, 0.0)], LaneKind::Street, 3.0);
    let on = LaneBuilder::polyline(vec![DVec3::new(0.0, 150.0, 0.0), DVec3::new(0.0, 450.0, 0.0)], LaneKind::Street, 3.0);
    let mut net = Network { lanes: vec![main, side, on], ..Default::default() };
    net.link(1.5);
    let mut t = traffic(&f, net);
    let side_len = t.net.lanes[1].length();
    let me = add_car(&mut t, &f, 1, side_len - 12.0, 0x5a, Some(0.0));
    let mut seed = 0x700;
    for (lane, len) in [(0usize, 150.0f32), (2, 300.0)] {
        let mut s = 3.0;
        while s < len - 3.0 {
            seed += 0x101;
            add_car(&mut t, &f, lane, s, seed, Some(v));
            s += spacing;
        }
    }
    let mut time = 0.0f32;
    while time < secs {
        if !t.cars.iter().any(|c| c.state.lane == 0 && c.state.s < spacing) {
            seed += 0x101;
            add_car(&mut t, &f, 0, 2.0, seed, Some(v));
        }
        for c in t.cars.iter_mut().filter(|c| c.id != me) {
            c.state.max_speed_kmh = v * 3.6;
        }
        t.tick(0.05, None);
        time += 0.05;
        if t.cars.iter().find(|c| c.id == me).is_none_or(|c| c.state.lane == 2) {
            return Some(time);
        }
    }
    None
}

#[test]
fn a_car_at_a_merge_gets_its_turn_in_a_rolling_queue() {
    // the zip: after a few seconds at the joint the car goes next, and the queue on the
    // other lane lets it in
    for (v, spacing) in [(1.5, 7.0), (3.0, 7.0), (4.0, 9.0)] {
        let at = zip_merge(v, spacing, 120.0);
        assert!(at.is_some_and(|t| t < 30.0), "queue at {v} m/s: merged after {at:?} s");
    }
}

#[test]
fn short_consecutive_paths_do_not_send_cars_backwards() {
    let f = Fixture::new();
    // Artificial straight road with two pieces shorter than the linking tolerance.
    // A backward link makes the planned way jump back and leaves the body following
    // a folded path, even though there is no obstacle on the road.
    let mut lanes = Vec::new();
    let mut y = 0.0;
    for len in [30.0, 0.47, 1.0, 25.0, 600.0] {
        lanes.push(street(DVec3::new(0.0, y, 0.0), 0.0, len));
        y += len;
    }
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    for seed in 1..=16 {
        let mut copy = Network { lanes: net.lanes.clone(), ..Default::default() };
        copy.link(1.5);
        let mut t = traffic(&f, copy);
        let id = add_car(&mut t, &f, 0, 26.0, seed, Some(5.0));
        let mut previous = 26.0;
        for _ in 0..200 {
            t.tick(0.05, None);
            let car = t.cars.iter().find(|c| c.id == id).expect("continuous road");
            let y = car.state.way_point(&t.net, 0.0).y;
            assert!(y >= previous - 0.01, "seed {seed}: {previous} -> {y}");
            previous = y;
        }
        assert!(previous > 60.0, "seed {seed} stuck at {previous}");
        let car = t.cars.iter().find(|c| c.id == id).unwrap();
        assert!(car.vehicle.position.y > 60.0, "seed {seed}: body stuck at {:?}", car.vehicle.position);
    }
}

#[test]
fn cars_still_cross_a_finite_exit_when_its_short_return_is_not_chosen() {
    let f = Fixture::new();
    let a = street(DVec3::ZERO, 0.0, 30.0);
    let finite = street(a.end(), 10.0, 20.0);
    let short = street(finite.end(), 10.0, 0.47);
    let next = street(short.end(), 10.0, 1.0);
    let exit = street(next.end(), 10.0, 280.0);
    let long = street(a.end(), 350.0, 700.0);
    let lanes = vec![a, finite, short, next, exit, long];
    let mut crossed = [0; 2];
    for seed in 1..=32 {
        let mut net = Network { lanes: lanes.clone(), ..Default::default() };
        net.link(1.5);
        let mut t = traffic(&f, net);
        let id = add_car(&mut t, &f, 0, 26.0, seed, Some(5.0));
        run(&mut t, 15.0);
        let car = t.cars.iter().find(|c| c.id == id).expect("ample road beyond the join");
        match car.state.lane {
            4 => {
                crossed[0] += 1;
                assert!(car.state.s > 30.0, "car did not drive past the short join");
                assert!(car.vehicle.position.y > 70.0, "body did not cross the join");
            }
            5 => crossed[1] += 1,
            other => panic!("car is still on approach lane {other}"),
        }
    }
    assert!(crossed.iter().all(|&n| n > 0), "a usable exit lost its traffic: {crossed:?}");
}

/// `way_lanes` as it was: the plan copied whole, then walked.
fn way_lanes_reference(t: &TrafficSim, st: &crate::traffic::AiState, within: f32) -> Vec<(usize, f32)> {
    let mut out = vec![(st.lane, -st.s)];
    let mut d = t.net.lanes[st.lane].length() - st.s;
    let plan: Vec<usize> = match st.change {
        Some(c) => std::iter::once(c.to).chain(st.change_plan.iter().copied()).collect(),
        None => st.upcoming().collect(),
    };
    if let Some(c) = st.change {
        out.clear();
        out.push((c.to, -c.s_to));
        d = t.net.lanes[c.to].length() - c.s_to;
        for &l in plan.iter().skip(1) {
            if d > within {
                break;
            }
            out.push((l, d));
            d += t.net.lanes[l].length();
        }
        return out;
    }
    for l in plan {
        if d > within {
            break;
        }
        out.push((l, d));
        d += t.net.lanes[l].length();
    }
    out
}

/// The way's lanes come out as before, for cars on their way through a junction and for
/// a car halfway over into another lane.
#[test]
fn way_lanes_are_what_they_were() {
    let f = Fixture::new();
    let mut t = traffic(&f, crossroads(150.0));
    for k in 0..4 {
        add_car(&mut t, &f, k, 100.0 + 10.0 * k as f32, 0x77 + k as u64, Some(8.0));
    }
    let mut checked = 0;
    for _ in 0..200 {
        t.tick(0.05, None);
        for c in &t.cars {
            let mut changing = c.state.clone();
            changing.change = Some(crate::traffic::LaneChange { to: (c.state.lane + 4) % t.net.lanes.len(), t: 0.3, length: 20.0, s_to: 3.0, dir: 1, wait: 0.0, bypass: false });
            changing.change_plan = vec![8, 9, 10];
            for st in [&c.state, &changing] {
                for within in [0.0, 30.0, 60.0, 100.0, 200.0, 1000.0] {
                    assert_eq!(t.way_lanes(st, within), way_lanes_reference(&t, st, within));
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 1000);
}

/// Every car of the scene as it stands: id, lane, distance along it and speed.
pub(super) fn snapshot(t: &TrafficSim) -> Vec<(u64, usize, f32, f32)> {
    let mut v: Vec<_> = t.cars.iter().map(|c| (c.id, c.state.lane, c.state.s, c.state.speed)).collect();
    v.sort_by_key(|x| x.0);
    v
}

/// Four cars at a crossroads without priorities and one queued behind them, placed in
/// `order` (with fixed ids, so that only their places in the list differ).
fn busy_crossroads(f: &Fixture, order: &[usize]) -> TrafficSim {
    let mut t = traffic(f, crossroads(150.0));
    let cars = [(0, 135.0), (1, 135.0), (2, 135.0), (3, 135.0), (0, 120.0)];
    for &k in order {
        let (lane, s) = cars[k];
        let vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
        t.place_car(vehicle, LaneKind::Street, lane, s, f.ty.clone(), 0x51 + k as u64 * 0x1000, None, Some(100 + k as u64), Some(4.0), None);
    }
    t
}

#[test]
fn a_lane_heading_alone_is_the_heading_of_its_point() {
    for (radius, heading) in [(0.0, 33.0), (25.0, 0.0), (40.0, 90.0), (12.0, 271.0)] {
        let lane = LaneBuilder::arc(DVec3::new(5.0, -3.0, 1.0), heading, 120.0, radius, 0.0, LaneKind::Street, 3.0);
        let mut s = -15.0f32;
        while s < lane.length() + 15.0 {
            assert_eq!(lane.heading_ext(s).to_bits(), lane.at_ext(s).1.to_bits(), "radius {radius} at {s}");
            s += 0.37;
        }
    }
}

#[test]
fn the_same_scene_drives_the_same_at_any_frame_rate() {
    let f = Fixture::new();
    let run_at = |fps: f32| {
        let mut t = busy_crossroads(&f, &[0, 1, 2, 3, 4]);
        for _ in 0..(20.0 * fps).round() as usize {
            t.advance(1.0 / fps, None);
        }
        snapshot(&t)
    };
    let slow = run_at(30.0);
    assert_eq!(slow, run_at(60.0));
    assert_eq!(slow, run_at(144.0));
    // (and the scene did move)
    assert!(slow.iter().any(|c| c.1 >= 8), "{slow:?}");
}

#[test]
fn the_order_the_cars_stand_in_the_list_decides_nothing() {
    let f = Fixture::new();
    let run = |order: &[usize]| {
        let mut t = busy_crossroads(&f, order);
        for _ in 0..1000 {
            t.advance(SIM_DT_TEST, None);
        }
        snapshot(&t)
    };
    let a = run(&[0, 1, 2, 3, 4]);
    assert_eq!(a, run(&[4, 3, 2, 1, 0]));
    assert_eq!(a, run(&[2, 4, 0, 3, 1]));
}

/// The cars' bodies and scripts come out the same whether the frame's thread runs them
/// alone or the workers help (`step_bodies`): told each car took a millisecond the step
/// before, the step is shared out among the workers.
#[test]
fn the_cars_step_alike_with_and_without_the_workers() {
    let f = Fixture::new();
    let run = |helped: bool| {
        let mut t = busy_crossroads(&f, &[0, 1, 2, 3, 4]);
        for _ in 0..1000 {
            if helped {
                t.cars.iter_mut().for_each(|c| c.ai_secs = 1e-3);
                assert!(omsi_cfg::par::helpers_for(5e-3, super::tick::BODY_SECS_PER_HELPER) > 0);
            }
            t.advance(SIM_DT_TEST, None);
        }
        snapshot(&t)
    };
    assert_eq!(run(false), run(true));
}

const SIM_DT_TEST: f32 = super::fixed_step::SIM_DT;

/// A car waiting for a gap to move over into the next lane while the car there, abreast of
/// it, stands behind it for good (it takes the one moving over for its lead, at a green
/// light too): the standing one drives on, and the waiting one gets its gap.
#[test]
fn a_car_that_waits_to_move_over_does_not_hold_up_the_lane_it_waits_for() {
    let f = Fixture::new();
    let net = Network {
        lanes: vec![street(DVec3::new(0.0, 0.0, 0.0), 0.0, 100.0), street(DVec3::new(3.5, 0.0, 0.0), 0.0, 100.0)],
        ..Default::default()
    };
    let mut t = traffic(&f, net);
    let waiting = add_car(&mut t, &f, 0, 60.0, 1, Some(0.0));
    let standing = add_car(&mut t, &f, 1, 58.0, 2, Some(0.0));
    let (a, b) = (t.cars.iter().position(|c| c.id == waiting).unwrap(), t.cars.iter().position(|c| c.id == standing).unwrap());
    t.time = 100.0;
    // the one behind takes the other for its lead and has stood for a while
    t.cars[b].lead_car = Some(waiting);
    t.cars[b].stopped = 20.0;
    t.cars[a].sideways.signalled = 12.0;
    t.cars[a].sideways.waiting = Some((1, t.time));
    t.break_lead_pairs();
    assert_eq!(t.cars[b].ignore_lead.map(|x| x.0), Some(waiting));
    // not when it waited for another lane, or only just began to wait
    t.cars[b].ignore_lead = None;
    t.cars[a].sideways.waiting = Some((0, t.time));
    t.break_lead_pairs();
    assert_eq!(t.cars[b].ignore_lead, None);
    t.cars[a].sideways.waiting = Some((1, t.time));
    t.cars[a].sideways.signalled = 2.0;
    t.break_lead_pairs();
    assert_eq!(t.cars[b].ignore_lead, None);
}
