//! Junction scenes run headless: lights (the amber memory, the lights of one crossing
//! object), the room on a junction's exit, priority signs against early claims,
//! roundabouts, the waits-for graph.

#[allow(unused_imports)]
use super::scenario_tests::{add_car, crossroads, street, Fixture};
use super::*;
use crate::ai_traffic::setup::RandomTypes;
use crate::traffic::{LaneBuilder, LaneKey, TrafficLightController};

/// Traffic on `net` with `lights` light programs of one light each (all green at first).
fn with_lights(f: &Fixture, mut net: Network, lights: usize) -> TrafficSim {
    net.build_grid();
    net.compute_reach();
    let random = RandomTypes { types: Vec::new(), groups: Vec::new(), group_curves: false, group_uvg: Vec::new(), uvg_defaults: Vec::new() };
    let ctls = (0..lights).map(|_| TrafficLightController::new(vec![vec![(6, 1000.0)]], 0.0)).collect();
    TrafficSim::assemble(&f.dir, net, random, ctls, HashMap::new(), (Vec::new(), Vec::new()), Vec::new(), (1.0, 0), 0)
}

/// Light `c` shows `state` from now on (6 green, 9 yellow, 0 red).
fn show(t: &mut TrafficSim, c: usize, state: i32) {
    t.lights[c].lights[0] = vec![(state, 1000.0)];
}

/// A car put on `lane` at `s` driving at `v` (not slowed down for being placed).
fn car(t: &mut TrafficSim, f: &Fixture, lane: usize, s: f32, seed: u64, v: f32) -> u64 {
    let id = add_car(t, f, lane, s, seed, Some(v));
    let c = t.cars.iter_mut().find(|c| c.id == id).unwrap();
    c.fresh = 0.0;
    c.state.speed = v;
    id
}

fn get(t: &TrafficSim, id: u64) -> &AiCar {
    t.cars.iter().find(|c| c.id == id).expect("the car is still there")
}

fn ticks(t: &mut TrafficSim, secs: f32) {
    for _ in 0..(secs / super::fixed_step::SIM_DT).round() as usize {
        t.tick(super::fixed_step::SIM_DT, None);
    }
}

/// Lanes 0 (150 m north to the line), 1 (the crossing object's path, 20 m, light 0), 2
/// (300 m on). With `second`: lane 1 leads into lane 3 of the same object (10 m, light 1,
/// the stop line after a footpath), and on into lane 2; with `other_approach` as well,
/// lane 4 from the east runs into lane 3 too (its light is that approach's).
fn signalled(second: bool, other_approach: bool) -> Network {
    let key = |path: u16| Some(LaneKey { tile: (0, 0), id: 5, path });
    let approach = street(DVec3::ZERO, 0.0, 150.0);
    let mut path = street(DVec3::new(0.0, 150.0, 0.0), 0.0, 20.0);
    path.source = 2;
    path.key = key(0);
    path.traffic_light = Some((0, 0));
    let mut lanes = vec![approach, path];
    if second {
        lanes.push(street(DVec3::new(0.0, 180.0, 0.0), 0.0, 300.0));
        let mut inner = street(DVec3::new(0.0, 170.0, 0.0), 0.0, 10.0);
        inner.source = 2;
        inner.key = key(1);
        inner.traffic_light = Some((1, 0));
        lanes.push(inner);
        if other_approach {
            // (a road of its own from the east bending north into lane 3)
            lanes.push(LaneBuilder::arc(DVec3::new(12.0, 158.0, 0.0), 270.0, 12.0 * std::f64::consts::FRAC_PI_2, 12.0, 0.0, LaneKind::Street, 3.0));
        }
    } else {
        lanes.push(street(DVec3::new(0.0, 170.0, 0.0), 0.0, 300.0));
    }
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    net
}

/// The car's front, metres past the line of lane 1 (+ = over it).
fn over_line(t: &TrafficSim, id: u64) -> f32 {
    let c = get(t, id);
    let base = match c.state.lane {
        0 => 0.0,
        1 => 150.0,
        3 => 170.0,
        2 if t.net.lanes.len() > 3 => 180.0,
        _ => 170.0,
    };
    base + c.state.s + c.state.front - 150.0
}

#[test]
fn a_car_that_crept_up_to_the_line_stops_at_red() {
    let f = Fixture::new();
    let mut t = with_lights(&f, signalled(false, false), 1);
    // creeping up behind a queue that has gone: 1.5 m before the line at 1 m/s on green
    let id = car(&mut t, &f, 0, 0.0, 0x31, 1.0);
    let front = get(&t, id).state.front;
    t.cars.iter_mut().find(|c| c.id == id).unwrap().state.s = 150.0 - 1.5 - front;
    ticks(&mut t, 0.1);
    show(&mut t, 0, 9);
    ticks(&mut t, 1.0);
    show(&mut t, 0, 0);
    ticks(&mut t, 6.0);
    assert!(over_line(&t, id) < 0.3, "it crept over the line: {:.2} m", over_line(&t, id));
    assert!(get(&t, id).state.speed < 0.1);
}

#[test]
fn a_moving_car_clears_the_junction_on_amber() {
    let f = Fixture::new();
    let mut t = with_lights(&f, signalled(false, false), 1);
    // 12 m/s, 10 m before the line as the light goes yellow, red a second later
    let id = car(&mut t, &f, 0, 130.0, 0x32, 12.0);
    let front = get(&t, id).state.front;
    t.cars.iter_mut().find(|c| c.id == id).unwrap().state.s = 140.0 - front;
    show(&mut t, 0, 9);
    ticks(&mut t, 1.0);
    show(&mut t, 0, 0);
    ticks(&mut t, 4.0);
    assert!(get(&t, id).state.lane == 2, "it stopped in the junction: lane {}", get(&t, id).state.lane);
}

#[test]
fn a_car_standing_with_its_nose_over_the_line_does_not_set_off_at_red() {
    let f = Fixture::new();
    let mut t = with_lights(&f, signalled(false, false), 1);
    show(&mut t, 0, 0);
    let id = car(&mut t, &f, 0, 0.0, 0x33, 0.0);
    let front = get(&t, id).state.front;
    t.cars.iter_mut().find(|c| c.id == id).unwrap().state.s = 150.0 + 0.6 - front;
    ticks(&mut t, 8.0);
    assert!(over_line(&t, id) < 0.7, "it moved off at red: {:.2} m over", over_line(&t, id));
    // and it goes on green
    show(&mut t, 0, 6);
    ticks(&mut t, 6.0);
    assert!(over_line(&t, id) > 5.0);
}

#[test]
fn the_first_car_at_the_line_gets_through_a_short_green() {
    let f = Fixture::new();
    let mut t = with_lights(&f, signalled(false, false), 1);
    show(&mut t, 0, 0);
    let id = car(&mut t, &f, 0, 140.0, 0x34, 6.0);
    ticks(&mut t, 8.0);
    assert!(over_line(&t, id) < 0.0 && get(&t, id).state.speed < 0.1, "it waits at red");
    show(&mut t, 0, 6);
    ticks(&mut t, 2.0);
    show(&mut t, 0, 9);
    ticks(&mut t, 1.0);
    show(&mut t, 0, 0);
    ticks(&mut t, 8.0);
    assert!(over_line(&t, id) > 10.0, "the first car stopped again: {:.1} m", over_line(&t, id));
}

#[test]
fn another_light_of_the_same_junction_holds() {
    let f = Fixture::new();
    let mut t = with_lights(&f, signalled(true, false), 2);
    show(&mut t, 1, 0);
    let id = car(&mut t, &f, 0, 100.0, 0x35, 8.0);
    ticks(&mut t, 15.0);
    let c = get(&t, id);
    assert_eq!(c.state.lane, 1, "it went through the second red (lane {})", c.state.lane);
    assert!(c.state.speed < 0.1);
}

#[test]
fn a_light_passed_does_not_hold_again_further_in() {
    // lane 3's light belongs to the approach from the east: the car from the south went
    // through its own green and is not stopped by it in the middle of the junction
    let f = Fixture::new();
    let mut t = with_lights(&f, signalled(true, true), 2);
    assert!(t.net.prev[3].contains(&4), "the east approach runs into lane 3");
    show(&mut t, 1, 0);
    let id = car(&mut t, &f, 0, 100.0, 0x36, 8.0);
    ticks(&mut t, 15.0);
    assert_eq!(get(&t, id).state.lane, 2);
}

#[test]
fn amber_decision_holds() {
    let f = Fixture::new();
    let mut t = with_lights(&f, signalled(false, false), 1);
    show(&mut t, 0, 9);
    let first = car(&mut t, &f, 0, 100.0, 0x37, 15.0);
    let second = car(&mut t, &f, 0, 50.0, 0x38, 15.0);
    ticks(&mut t, 1.0);
    t.cars.iter_mut().find(|c| c.id == first).unwrap().amber_decision = 0.0;
    t.cars.iter_mut().find(|c| c.id == second).unwrap().amber_decision = 1.0;
    ticks(&mut t, 15.0);
    let c1 = get(&t, first);
    let c2 = get(&t, second);
    assert_eq!(c1.amber_decision, 0.0, "amber decision of car 1 was not overridden, {} != 0", c1.amber_decision);
    assert_eq!(c2.amber_decision, 1.0, "amber decision of car 2 was not overridden, {} != 1", c2.amber_decision);
    assert!(over_line(&t, first) > 10.0, "car 1 did not pass the yellow light");
    assert!(over_line(&t, second) < 0.0, "car 2 did pass the yellow light");
}

#[test]
fn amber_decision_holds_on_red() {
    let f = Fixture::new();
    let mut t = with_lights(&f, signalled(false, false), 1);
    show(&mut t, 0, 0);
    let id = car(&mut t, &f, 0, 100.0, 0x39, 15.0);
    ticks(&mut t, 1.0);
    t.cars.iter_mut().find(|c| c.id == id).unwrap().amber_decision = 0.0;
    ticks(&mut t, 15.0);
    let c = get(&t, id);
    assert_eq!(c.amber_decision, 0.0, "amber decision of car was not overridden, {} != 0", c.amber_decision);
    assert!(over_line(&t, id) > 10.0, "car did not pass the red light");
}

#[test]
fn amber_decision_holds_with_multiple_lights() {
    let f = Fixture::new();
    let mut t = with_lights(&f, signalled(true, false), 2);
    show(&mut t, 1, 0);
    let id = car(&mut t, &f, 0, 100.0, 0x3a, 15.0);
    ticks(&mut t, 1.0);
    t.cars.iter_mut().find(|c| c.id == id).unwrap().amber_decision = 0.0;
    ticks(&mut t, 15.0);
    let c = get(&t, id);
    assert_eq!(c.current_light, Some((1, 0)), "car's current light was not updated to (1, 0): {:?}", c.current_light);
    assert_eq!(c.state.lane, 1, "car did pass the crossing to lane {} (must have stopped on line 1)", c.state.lane);
    assert!(over_line(&t, id) > 10.0);
}

#[test]
fn stopped_car_does_not_sets_off_on_red() {
    let f = Fixture::new();
    let mut t = with_lights(&f, signalled(false, false), 1);
    show(&mut t, 0, 0);
    let id = car(&mut t, &f, 0, 0.0, 0x3b, 0.0);
    let front = get(&t, id).state.front;
    // stopped at 2 m before the line
    t.cars.iter_mut().find(|c| c.id == id).unwrap().state.s = 150.0 - 2.0 - front;
    ticks(&mut t, 0.1);
    t.cars.iter_mut().find(|c| c.id == id).unwrap().amber_decision = 0.0;
    ticks(&mut t, 15.0);
    let c = get(&t, id);
    assert!(over_line(&t, id) < 0.3, "car set off on red: {}", over_line(&t, id));
    assert_eq!(c.state.lane, 0, "car moved onto lane {}", c.state.lane);
    assert!(c.state.speed < 0.1, "car is moving at {} m/s", c.state.speed);
}

#[test]
fn slow_moving_car_does_not_run_the_red() {
    let f = Fixture::new();
    let mut t = with_lights(&f, signalled(false, false), 1);
    show(&mut t, 0, 0);
    let id = car(&mut t, &f, 0, 0.0, 0x3c, 2.0);
    let front = get(&t, id).state.front;
    // at 5 m before the line
    t.cars.iter_mut().find(|c| c.id == id).unwrap().state.s = 150.0 - 5.0 - front;
    ticks(&mut t, 0.1);
    t.cars.iter_mut().find(|c| c.id == id).unwrap().amber_decision = 0.0;
    ticks(&mut t, 15.0);
    let c = get(&t, id);
    assert!(over_line(&t, id) < 0.3, "car passed on red: {}", over_line(&t, id));
    assert_eq!(c.state.lane, 0, "car moved onto lane {}", c.state.lane);
    assert!(c.state.speed < 0.1, "car is moving at {} m/s", c.state.speed);
}

/// `lane` as path `path` of junction object `id`.
fn object(mut lane: crate::traffic::Lane, id: i64, path: u16) -> crate::traffic::Lane {
    lane.source = 2;
    lane.key = Some(LaneKey { tile: (0, 0), id, path });
    lane
}

/// Holds car `id` where it is (a queue that does not move on).
fn hold_still(t: &mut TrafficSim, id: u64) {
    if let Some(c) = t.cars.iter_mut().find(|c| c.id == id) {
        c.state.speed = 0.0;
        c.state.max_speed_kmh = 0.0;
    }
}

fn plain(f: &Fixture, mut net: Network) -> TrafficSim {
    net.build_grid();
    with_lights(f, net, 0)
}

/// Lane of the crossroads (`crossroads`): 0..4 in, 4..8 the junction's paths (24 m), 8..12
/// out (`exit` m).
#[test]
fn nobody_follows_a_standing_queue_into_a_junction() {
    // the car ahead stands in the junction's path just past the middle, 20 m in: a car
    // following it would stand with its rear across the east-west paths
    let f = Fixture::new();
    let mut t = plain(&f, crossroads(150.0));
    let ahead = car(&mut t, &f, 4, 20.0, 0x41, 0.0);
    let me = car(&mut t, &f, 0, 100.0, 0x42, 8.0);
    for _ in 0..(20.0 / super::fixed_step::SIM_DT) as usize {
        hold_still(&mut t, ahead);
        t.tick(super::fixed_step::SIM_DT, None);
    }
    let c = get(&t, me);
    assert_eq!(c.state.lane, 0, "it followed into the junction (s {:.1})", c.state.s);
    assert!(c.state.speed < 0.1);
}

#[test]
fn a_queue_with_room_beyond_the_junction_is_followed() {
    // the car ahead stands 30 m along the road out: room enough behind it
    let f = Fixture::new();
    let mut t = plain(&f, crossroads(150.0));
    let ahead = car(&mut t, &f, 8, 30.0, 0x43, 0.0);
    let me = car(&mut t, &f, 0, 100.0, 0x44, 8.0);
    for _ in 0..(20.0 / super::fixed_step::SIM_DT) as usize {
        hold_still(&mut t, ahead);
        t.tick(super::fixed_step::SIM_DT, None);
    }
    let c = get(&t, me);
    assert_eq!(c.state.lane, 8, "it stayed at the line (lane {} s {:.1})", c.state.lane, c.state.s);
}

#[test]
fn a_blocked_exit_is_never_granted_by_waiting_and_traffic_resumes_after_the_blocker_leaves() {
    let f = Fixture::new();
    let mut t = plain(&f, crossroads(14.0));
    let front = car(&mut t, &f, 8, 10.0, 0x77, 0.0);
    let second = car(&mut t, &f, 8, 3.0, 0x78, 0.0);
    let me = car(&mut t, &f, 0, 90.0, 0x79, 8.0);
    // (two minutes: long past the 45 s after which the car used to squeeze in)
    for _ in 0..(120.0 / super::fixed_step::SIM_DT) as usize {
        hold_still(&mut t, front);
        hold_still(&mut t, second);
        t.tick(super::fixed_step::SIM_DT, None);
        let c = get(&t, me);
        assert_eq!(c.state.lane, 0, "it drove into the junction after {:.0} s", t.time);
    }
    t.cars.retain(|c| c.id == me);
    ticks(&mut t, 20.0);
    assert!(t.cars.iter().find(|c| c.id == me).is_none_or(|c| c.state.lane == 8), "it did not go on once the exit was free");
}

/// A junction where a straight path (west to east, lane 2) and a right turn from the south
/// (lane 3) both lead onto one exit (lane 4), with roads in (0 from the west, 1 from the
/// south); priorities as a main road and its side road.
fn shared_exit() -> Network {
    let west = street(DVec3::new(-110.0, 0.0, 0.0), 90.0, 100.0);
    let south = street(DVec3::new(10.0, -110.0, 0.0), 0.0, 100.0);
    let mut main = object(street(DVec3::new(-10.0, 0.0, 0.0), 90.0, 20.0), 7, 0);
    main.priority = 192.0;
    // from (10, -10) heading north, a right turn of radius 10 ends at (20, 0) heading east:
    // that is not where the exit starts - turn into it at its start, 10 m east of the line
    let mut turn = object(LaneBuilder::arc(DVec3::new(0.0, -10.0, 0.0), 0.0, 10.0 * std::f64::consts::FRAC_PI_2, 10.0, 0.0, LaneKind::Street, 3.0), 7, 1);
    turn.priority = 64.0;
    turn.turn = 2;
    let mut lanes = vec![west, south, main, turn];
    // (the side road's approach ends where the turn starts)
    lanes[1] = street(DVec3::new(0.0, -110.0, 0.0), 0.0, 100.0);
    lanes.push(street(DVec3::new(10.0, 0.0, 0.0), 90.0, 200.0));
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    net
}

#[test]
fn the_room_on_an_exit_is_promised_once() {
    // a queue stands on the exit with room for one car; two cars come along the main road
    // one behind the other: the first is let in, the second - its leader not on the exit
    // yet, the exit's room looking free - is not promised the same room and waits at the
    // line, not in the junction
    let f = Fixture::new();
    let mut t = plain(&f, shared_exit());
    assert!(t.net.crossings[2].iter().any(|c| c.other == 3 && c.merge), "the two paths run into one exit");
    let blocker = car(&mut t, &f, 4, 0.0, 0x51, 0.0);
    let room = get(&t, blocker).state.rear + 9.0;
    t.cars.iter_mut().find(|c| c.id == blocker).unwrap().state.s = room;
    let a = car(&mut t, &f, 0, 75.0, 0x52, 6.0);
    let b = car(&mut t, &f, 0, 64.0, 0x53, 6.0);
    for _ in 0..(30.0 / super::fixed_step::SIM_DT) as usize {
        hold_still(&mut t, blocker);
        t.tick(super::fixed_step::SIM_DT, None);
    }
    assert!(matches!(get(&t, a).state.lane, 2 | 4), "the first car is let in (lane {})", get(&t, a).state.lane);
    let c = get(&t, b);
    assert_eq!(c.state.lane, 0, "the second car went into the junction for the first one's room (s {:.1})", c.state.s);
}

#[test]
fn the_priority_road_goes_first() {
    let f = Fixture::new();
    let mut t = plain(&f, super::scenario_tests::side_road());
    let side = car(&mut t, &f, 3, 85.0, 0x61, 0.0);
    let main = car(&mut t, &f, 0, 120.0, 0x62, 12.0);
    let mut main_passed_at = None;
    let mut side_in_at = None;
    for k in 0..(20.0 / super::fixed_step::SIM_DT) as usize {
        t.tick(super::fixed_step::SIM_DT, None);
        let time = k as f32 * super::fixed_step::SIM_DT;
        if main_passed_at.is_none() && t.cars.iter().find(|c| c.id == main).is_none_or(|c| c.state.lane == 2 || (c.state.lane == 1 && c.state.s - c.state.rear > 12.0)) {
            main_passed_at = Some(time);
        }
        if side_in_at.is_none() && get(&t, side).state.lane == 4 {
            side_in_at = Some(time);
        }
    }
    let (m, s) = (main_passed_at.expect("the main road's car got through"), side_in_at.expect("the side road's car went after it"));
    assert!(s >= m - 0.5, "the side road's car went in at {s:.1} s, before the main road's was past ({m:.1} s)");
}

#[test]
fn an_early_claim_from_the_side_road_does_not_take_the_priority() {
    // the side road's car has claimed its way 25 m before its line (early, the main road
    // empty then); the main road's car comes: it does not brake for the claim, the side
    // road's car, still able to stop, gives way
    let f = Fixture::new();
    let mut t = plain(&f, super::scenario_tests::side_road());
    let side = car(&mut t, &f, 3, 72.0, 0x63, 8.0);
    let main = car(&mut t, &f, 0, 140.0, 0x64, 13.0);
    {
        let c = t.cars.iter_mut().find(|c| c.id == side).unwrap();
        c.reserved = vec![4];
    }
    let mut hardest = 0.0f32;
    let mut side_first = false;
    for _ in 0..(15.0 / super::fixed_step::SIM_DT) as usize {
        t.tick(super::fixed_step::SIM_DT, None);
        if let Some(c) = t.cars.iter().find(|c| c.id == main) {
            hardest = hardest.min(c.state.acc);
            let main_in = c.state.lane == 1 || c.state.lane == 2;
            if !main_in && get(&t, side).state.lane == 4 {
                side_first = true;
            }
        }
    }
    assert!(hardest > -1.5, "the main road's car braked at {hardest:.1} m/s² for the early claim");
    assert!(!side_first, "the side road's car went first");
}

/// Crossroads whose north-south path is cut in two (path 0 up to 4 m past the middle,
/// path 4 on to the edge), the east-west path crossing the first: lanes 0..4 in, 4..8 the
/// paths (4 the first piece), 8..12 out, 12 the second piece.
fn split_crossroads() -> Network {
    let mut net = crossroads(150.0);
    // replace lane 4 (y -12..12 at x 1.75) by two pieces
    let mut first = object(street(DVec3::new(1.75, -12.0, 0.0), 0.0, 16.0), 1, 0);
    let mut second = object(street(DVec3::new(1.75, 4.0, 0.0), 0.0, 8.0), 1, 4);
    // (the north-south road has the right of way)
    first.priority = 192.0;
    second.priority = 192.0;
    net.lanes[7].priority = 64.0;
    net.lanes[4] = first;
    net.lanes.push(second);
    for l in net.lanes.iter_mut() {
        l.next.clear();
    }
    net.link(1.5);
    net
}

#[test]
fn a_long_vehicles_rear_holds_the_junction_until_it_is_out() {
    // a 22 m long vehicle (its rear section) goes north; a car from the east waits at
    // its line until the rear has crossed its path, two pieces of path back
    let f = Fixture::new();
    let mut t = plain(&f, split_crossroads());
    let bus = car(&mut t, &f, 0, 120.0, 0x71, 8.0);
    {
        let c = t.cars.iter_mut().find(|c| c.id == bus).unwrap();
        c.state.rear = 20.0;
        c.state.length = c.state.front + 20.0;
    }
    let cross = car(&mut t, &f, 3, 146.0, 0x72, 0.0);
    let mut clash = None;
    for _ in 0..(25.0 / super::fixed_step::SIM_DT) as usize {
        t.tick(super::fixed_step::SIM_DT, None);
        let (Some(b), Some(c)) = (t.cars.iter().find(|c| c.id == bus), t.cars.iter().find(|c| c.id == cross)) else { break };
        // the bus's rear (y) and the crossing car's front (x, heading west from x = 12)
        let rear_y = match b.state.lane {
            0 => b.state.s - 150.0 - 12.0,
            4 => b.state.s - 12.0,
            12 => b.state.s + 4.0,
            8 => b.state.s + 12.0,
            _ => f32::MAX,
        } - b.state.rear;
        let front_x = match c.state.lane {
            3 => 12.0 + (150.0 - c.state.s - c.state.front),
            7 => 12.0 - c.state.s - c.state.front,
            _ => f32::MIN,
        };
        // the bus fills x 0.5..3 up to its front: a car's front past x = 3 with the bus's
        // rear still south of the east-west path (y 1.75 + 1.5)
        if front_x < 3.0 && rear_y < 3.25 && b.state.lane != 0 {
            clash = Some((t.time, rear_y, front_x));
            break;
        }
    }
    assert!(clash.is_none(), "the car from the east went into the bus's rear: {clash:?}");
    assert!(t.cars.iter().find(|c| c.id == cross).is_none_or(|c| c.state.lane != 3), "the car from the east went on afterwards");
}

#[test]
fn a_four_way_crossing_admits_one_movement_at_a_time() {
    // five cars, no priorities: never two on crossing paths in the middle at once
    let f = Fixture::new();
    let mut t = plain(&f, crossroads(150.0));
    let ids: Vec<u64> = (0..4).map(|k| car(&mut t, &f, k, 125.0, 0x81 + k as u64 * 0x1000, 6.0)).collect();
    car(&mut t, &f, 0, 110.0, 0x91, 6.0);
    for _ in 0..(40.0 / super::fixed_step::SIM_DT) as usize {
        t.tick(super::fixed_step::SIM_DT, None);
        let middle: Vec<usize> = t
            .cars
            .iter()
            .filter(|c| (4..8).contains(&c.state.lane) && c.state.s + c.state.front > 8.0 && c.state.s - c.state.rear < 16.0)
            .map(|c| c.state.lane - 4)
            .collect();
        for a in &middle {
            for b in &middle {
                assert!(a == b || (a + 2) % 4 == *b, "paths {a} and {b} in the middle at once at {:.1} s", t.time);
            }
        }
    }
    for id in ids {
        assert!(t.cars.iter().find(|c| c.id == id).is_none_or(|c| c.state.lane >= 8), "car {id} still waits");
    }
}

/// A crossing for a left turn across the oncoming traffic: lanes 0 (from the south to the
/// line at y -12, x 1.75), 1 (the left turn, path 0, round to the west, `turn` 1), 2 (the
/// road out west), 3 (from the north to the line at y 12, x -1.75), 4 (straight on south,
/// path 1), 5 (the road out south). With `signal` both paths have light 0.
fn left_turn(signal: bool) -> Network {
    let r = 13.75;
    let mut turn = object(LaneBuilder::arc(DVec3::new(1.75, -12.0, 0.0), 0.0, r * std::f64::consts::FRAC_PI_2, -r, 0.0, LaneKind::Street, 3.0), 3, 0);
    turn.turn = 1;
    let west_start = turn.end();
    let straight = object(street(DVec3::new(-1.75, 12.0, 0.0), 180.0, 24.0), 3, 1);
    let mut lanes = vec![
        street(DVec3::new(1.75, -162.0, 0.0), 0.0, 150.0),
        turn,
        street(west_start, 270.0, 200.0),
        street(DVec3::new(-1.75, 212.0, 0.0), 180.0, 200.0),
        straight,
        street(DVec3::new(-1.75, -12.0, 0.0), 180.0, 300.0),
    ];
    if signal {
        lanes[1].traffic_light = Some((0, 0));
        lanes[4].traffic_light = Some((0, 0));
    }
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    net
}

/// Runs the left-turn scene for `secs` with an oncoming car put on every 20 m at 8 m/s:
/// how far past the line each turner (by id) got at most, in metres along its turn (100
/// on the road out). After that, with `then` seconds more and the oncoming traffic gone.
fn turners(signal: bool, cars_in: &[(f32, u64)], secs: f32, then: f32) -> Vec<f32> {
    let f = Fixture::new();
    let mut t = with_lights(&f, left_turn(signal), signal as usize);
    assert!(t.net.crossings[1].iter().any(|c| c.other == 4), "the turn crosses the oncoming path");
    let ids: Vec<u64> = cars_in.iter().map(|&(s, seed)| car(&mut t, &f, 0, s, seed, 0.0)).collect();
    let mut seed = 0x900;
    let mut s = 40.0;
    while s < 200.0 {
        seed += 0x11;
        car(&mut t, &f, 3, s, seed, 8.0);
        s += 20.0;
    }
    let mut best = vec![f32::MIN; ids.len()];
    for _ in 0..(secs / super::fixed_step::SIM_DT) as usize {
        if !t.cars.iter().any(|c| c.state.lane == 3 && c.state.s < 20.0) {
            seed += 0x11;
            car(&mut t, &f, 3, 2.0, seed, 8.0);
        }
        t.tick(super::fixed_step::SIM_DT, None);
        for (k, id) in ids.iter().enumerate() {
            let c = get(&t, *id);
            let past = match c.state.lane {
                0 => c.state.s + c.state.front - 150.0,
                1 => c.state.s + c.state.front,
                _ => 100.0,
            };
            best[k] = best[k].max(past);
        }
    }
    if then > 0.0 {
        t.cars.retain(|c| ids.contains(&c.id));
        ticks(&mut t, then);
        for (k, id) in ids.iter().enumerate() {
            if t.cars.iter().find(|c| c.id == *id).is_none_or(|c| c.state.lane == 2) {
                best[k] = 100.0;
            }
        }
    }
    best
}

#[test]
fn a_left_turn_at_a_signal_pulls_into_the_junction() {
    let best = turners(true, &[(140.0, 0xa1)], 20.0, 0.0);
    assert!(best[0] >= 3.0 && best[0] < 12.0, "it got {:.1} m past the line", best[0]);
    // and it turns once the oncoming traffic has gone
    assert_eq!(turners(true, &[(140.0, 0xa1)], 20.0, 15.0)[0], 100.0);
}

#[test]
fn a_left_turn_without_a_signal_waits_at_the_line() {
    let best = turners(false, &[(140.0, 0xa2)], 20.0, 0.0);
    assert!(best[0] < 1.0, "it got {:.1} m past the line", best[0]);
}

#[test]
fn a_second_left_turner_waits_at_the_line() {
    let best = turners(true, &[(140.0, 0xa3), (128.0, 0xa4)], 25.0, 0.0);
    assert!(best[0] >= 3.0, "the first got {:.1} m past the line", best[0]);
    assert!(best[1] < 1.0, "the second got {:.1} m past the line", best[1]);
}

/// A roundabout of radius 15 m driven anticlockwise (junction object 9): lanes 0..4 the
/// ring's quarters from the north point round (west, south, east), 4 an entry from the
/// south-west running into the ring at its south point, 5 the road to the entry.
fn roundabout() -> Network {
    let r: f64 = 15.0;
    let mut lanes = Vec::new();
    for k in 0..4 {
        let a = -(k as f64) * 90.0;
        let p = DVec3::new(r * a.to_radians().sin(), r * a.to_radians().cos(), 0.0);
        lanes.push(object(LaneBuilder::arc(p, a - 90.0, r * std::f64::consts::FRAC_PI_2, -r, 0.0, LaneKind::Street, 3.0), 9, k as u16));
    }
    lanes.push(object(LaneBuilder::arc(DVec3::new(-12.0, -27.0, 0.0), 0.0, 12.0 * std::f64::consts::FRAC_PI_2, 12.0, 0.0, LaneKind::Street, 3.0), 9, 4));
    lanes.push(street(DVec3::new(-12.0, -127.0, 0.0), 0.0, 100.0));
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    net
}

/// The roundabout with a car waiting at the entry's line (lane 5's end) and a ring car on
/// quarter `ring_lane` at `ring_s` doing `ring_v` (held there when 0): (traffic, the
/// entry's car, the ring car).
fn at_the_ring(f: &Fixture, ring_lane: usize, ring_s: f32, ring_v: f32) -> (TrafficSim, u64, u64) {
    let mut t = plain(f, roundabout());
    assert!(t.net.lanes[..4].iter().all(|l| l.ring) && !t.net.lanes[4].ring, "the ring is found");
    assert!(t.net.crossings[4].iter().any(|c| c.other == 1 && c.merge), "the entry runs into the ring");
    let entry = car(&mut t, f, 5, 92.0, 0xb1, 0.0);
    let ring = car(&mut t, f, ring_lane, ring_s, 0xb2, ring_v);
    (t, entry, ring)
}

/// Seconds until car `id` is on lane 2 (past the entry's joint), within `secs`.
fn reaches_lane_2(t: &mut TrafficSim, id: u64, hold: Option<u64>, secs: f32) -> Option<f32> {
    for k in 0..(secs / super::fixed_step::SIM_DT) as usize {
        if let Some(h) = hold {
            hold_still(t, h);
        }
        t.tick(super::fixed_step::SIM_DT, None);
        if t.cars.iter().find(|c| c.id == id).is_none_or(|c| matches!(c.state.lane, 2 | 3)) {
            return Some(k as f32 * super::fixed_step::SIM_DT);
        }
    }
    None
}

#[test]
fn the_ring_goes_before_the_entry_without_priorities() {
    let f = Fixture::new();
    let (mut t, entry, ring) = at_the_ring(&f, 1, 5.0, 7.0);
    let mut ring_at = None;
    let mut entry_at = None;
    for k in 0..(20.0 / super::fixed_step::SIM_DT) as usize {
        t.tick(super::fixed_step::SIM_DT, None);
        let time = k as f32 * super::fixed_step::SIM_DT;
        for (id, at) in [(ring, &mut ring_at), (entry, &mut entry_at)] {
            if at.is_none() && t.cars.iter().find(|c| c.id == id).is_none_or(|c| matches!(c.state.lane, 2 | 3)) {
                *at = Some(time);
            }
        }
    }
    let (r, e) = (ring_at.expect("the ring car went on"), entry_at.expect("the entry's car went in after it"));
    assert!(e > r, "the entry's car went in at {e:.1} s, before the ring car ({r:.1} s)");
}

#[test]
fn the_ring_does_not_give_way_to_an_entry_that_can_still_stop() {
    // the entry's car has claimed its way early, 30 m off at 8 m/s
    let f = Fixture::new();
    // (the ring car at the ring's own bend speed)
    let (mut t, entry, ring) = at_the_ring(&f, 1, 5.0, 4.3);
    {
        let c = t.cars.iter_mut().find(|c| c.id == entry).unwrap();
        c.state.s = 75.0;
        c.state.speed = 8.0;
        c.reserved = vec![4, 2];
    }
    let mut hardest = 0.0f32;
    let mut entry_first = false;
    for _ in 0..(12.0 / super::fixed_step::SIM_DT) as usize {
        t.tick(super::fixed_step::SIM_DT, None);
        let r = get(&t, ring);
        hardest = hardest.min(r.state.acc);
        if matches!(r.state.lane, 0 | 1) && t.cars.iter().find(|c| c.id == entry).is_none_or(|c| matches!(c.state.lane, 2 | 3)) {
            entry_first = true;
        }
    }
    assert!(hardest > -1.5, "the ring car braked at {hardest:.1} m/s² for the entry's claim");
    assert!(!entry_first, "the entry's car went in ahead of the ring car");
}

#[test]
fn an_entry_gives_way_to_a_ring_car_standing_close_however_long_it_waited() {
    // a ring car stands 6 m before the joint (briefly stopped, the entry's car has waited a
    // minute): the entry does not squeeze in ahead of it
    let f = Fixture::new();
    let len = roundabout().lanes[1].length();
    let (mut t, entry, ring) = at_the_ring(&f, 1, 0.0, 0.0);
    let front = get(&t, ring).state.front;
    t.cars.iter_mut().find(|c| c.id == ring).unwrap().state.s = len - 6.0 - front;
    ticks(&mut t, 1.0);
    t.cars.iter_mut().find(|c| c.id == entry).unwrap().state.yield_time = 60.0;
    assert_eq!(reaches_lane_2(&mut t, entry, Some(ring), 20.0), None, "the entry's car went in ahead of the ring car");
}

#[test]
fn an_entry_may_go_past_a_stalled_ring_queue() {
    // the ring car stands far back on the quarter before (a stalled queue, not coming)
    let f = Fixture::new();
    let (mut t, entry, ring) = at_the_ring(&f, 0, 8.0, 0.0);
    let at = reaches_lane_2(&mut t, entry, Some(ring), 20.0);
    assert!(at.is_some(), "the entry's car waited for a ring car that stood 37 m back");
}

/// A junction object of a path north (path 0, lane 1, `[crossingproblem]` as given) and
/// a path west crossing it (path 1, lane 3), each with its road in (lanes 0, 2) and out
/// (4, 5).
fn crossing_problem(flag: bool) -> Network {
    let mut north = object(street(DVec3::new(0.0, -20.0, 0.0), 0.0, 40.0), 11, 0);
    north.crossing_problem = flag;
    let west = object(street(DVec3::new(20.0, 0.0, 0.0), 270.0, 40.0), 11, 1);
    let lanes = vec![
        street(DVec3::new(0.0, -120.0, 0.0), 0.0, 100.0),
        north,
        street(DVec3::new(120.0, 0.0, 0.0), 270.0, 100.0),
        west,
        street(DVec3::new(0.0, 20.0, 0.0), 0.0, 200.0),
        street(DVec3::new(-20.0, 0.0, 0.0), 270.0, 200.0),
    ];
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    net
}

#[test]
fn a_crossing_problem_holds_the_crossing_path_while_the_lane_is_taken() {
    for flag in [false, true] {
        let f = Fixture::new();
        let mut t = plain(&f, crossing_problem(flag));
        // a car on the flagged path, standing past the crossing point (its rear 3 m past)
        let on = car(&mut t, &f, 1, 0.0, 0xc1, 0.0);
        let rear = get(&t, on).state.rear;
        t.cars.iter_mut().find(|c| c.id == on).unwrap().state.s = 20.0 + 3.0 + 1.5 + rear;
        let me = car(&mut t, &f, 2, 92.0, 0xc2, 0.0);
        for _ in 0..(15.0 / super::fixed_step::SIM_DT) as usize {
            hold_still(&mut t, on);
            t.tick(super::fixed_step::SIM_DT, None);
        }
        let c = get(&t, me);
        if flag {
            assert_eq!(c.state.lane, 2, "it went while the flagged lane was taken");
        } else {
            assert!(c.state.lane == 5 || (c.state.lane == 3 && c.state.s > 20.0), "without the flag it goes behind the car (lane {} s {:.1})", c.state.lane, c.state.s);
        }
    }
}

#[test]
fn somebody_on_the_crossing_holds_the_car() {
    let f = Fixture::new();
    let mut net = crossroads(150.0);
    // a footpath of the junction object across the northbound path, 4 m past the middle
    net.lanes.push(object(LaneBuilder::arc(DVec3::new(-10.0, 4.0, 0.0), 90.0, 20.0, 0.0, 0.0, LaneKind::Sidewalk, 2.0), 1, 20));
    let walk = net.lanes.len() - 1;
    net.link(1.5);
    let mut t = plain(&f, net);
    assert!(t.net.walks[4].iter().any(|w| w.0 == walk), "the footpath crosses the path");
    t.walkers = vec![(walk, 11.75)];
    let me = car(&mut t, &f, 0, 100.0, 0xd1, 8.0);
    ticks(&mut t, 15.0);
    let c = get(&t, me);
    assert!((c.state.lane == 0 || (c.state.lane == 4 && c.state.s + c.state.front < 16.0 - 2.0)) && c.state.speed < 0.1, "it stops before the crossing (lane {} s {:.1})", c.state.lane, c.state.s);
    t.walkers.clear();
    ticks(&mut t, 10.0);
    assert!(t.cars.iter().find(|c| c.id == me).is_none_or(|c| c.state.lane == 8));
}

#[test]
fn a_train_holds_the_road_traffic() {
    let f = Fixture::new();
    let mut net = crossroads(150.0);
    // a track of the junction object across the middle, west to east
    net.lanes.push(object(LaneBuilder::arc(DVec3::new(-80.0, 0.0, 0.0), 90.0, 160.0, 0.0, 0.0, LaneKind::Rail, 3.0), 1, 21));
    let track = net.lanes.len() - 1;
    net.link(1.5);
    let mut t = plain(&f, net);
    assert!(t.net.crossings[4].iter().any(|c| c.other == track));
    let me = car(&mut t, &f, 0, 138.0, 0xd2, 0.0);
    let vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
    let train = t.place_car(vehicle, LaneKind::Rail, track, 40.0, f.ty.clone(), 0xd3, None, None, Some(8.0), None);
    let mut crossed_before = false;
    for _ in 0..(15.0 / super::fixed_step::SIM_DT) as usize {
        t.tick(super::fixed_step::SIM_DT, None);
        let train_x = t.cars.iter().find(|c| c.id == train).map(|c| c.state.s - 80.0 - c.state.rear).unwrap_or(f32::MAX);
        let c = get(&t, me);
        if c.state.lane != 0 && train_x < 3.0 {
            crossed_before = true;
        }
    }
    assert!(!crossed_before, "the car went across in front of the train");
}

#[test]
fn a_queue_at_a_red_light_is_no_circle() {
    let f = Fixture::new();
    let mut t = with_lights(&f, signalled(false, false), 1);
    show(&mut t, 0, 0);
    let ids: Vec<u64> = (0..4).map(|k| car(&mut t, &f, 0, 140.0 - 8.0 * k as f32, 0xe1 + k, 5.0)).collect();
    ticks(&mut t, 30.0);
    for id in ids {
        let c = get(&t, id);
        assert_eq!(c.yield_pass, f32::MIN, "car {id} was let go");
        assert_eq!(c.state.lane, 0);
    }
}

/// Four cars standing at a crossroads, each holding (`kind`) the next, waited `waits` s.
fn circle_of_four(f: &Fixture, kind: super::deadlock::Hold, waits: [f32; 4]) -> TrafficSim {
    let mut t = plain(f, crossroads(150.0));
    let ids: Vec<u64> = (0..4).map(|k| car(&mut t, f, k, 140.0, 0xf1 + k as u64, 0.0)).collect();
    t.index_of = t.cars.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
    for k in 0..4 {
        let c = t.cars.iter_mut().find(|c| c.id == ids[k]).unwrap();
        c.holds = vec![(ids[(k + 1) % 4], kind)];
        c.state.yield_time = waits[k];
        c.reserved = vec![4 + k];
    }
    t
}

#[test]
fn a_circle_of_cars_giving_way_lets_the_longest_waiter_go_after_eight_seconds() {
    use super::deadlock::Hold;
    let f = Fixture::new();
    let mut t = circle_of_four(&f, Hold::Rule, [6.0, 7.5, 5.0, 7.0]);
    t.resolve_waits();
    assert!(t.cars.iter().all(|c| c.yield_pass == f32::MIN), "let go before eight seconds");
    let mut t = circle_of_four(&f, Hold::Rule, [9.0, 12.5, 8.5, 10.0]);
    t.resolve_waits();
    let going: Vec<usize> = (0..4).filter(|&k| t.cars[k].yield_pass > t.time).collect();
    assert_eq!(going, vec![1], "the longest waiter goes, alone");
    // a body in the circle: nobody is forced
    let mut t = circle_of_four(&f, Hold::Rule, [20.0; 4]);
    t.cars[2].holds[0].1 = Hold::Body;
    t.resolve_waits();
    assert!(t.cars.iter().all(|c| c.yield_pass == f32::MIN));
    // a red light in it: congestion as the law wants it
    let mut t = circle_of_four(&f, Hold::Rule, [20.0; 4]);
    t.cars[0].light_hold = true;
    t.resolve_waits();
    assert!(t.cars.iter().all(|c| c.yield_pass == f32::MIN));
}

#[test]
fn stale_claims_are_let_go_but_the_longest_waiters() {
    use super::deadlock::Hold;
    let f = Fixture::new();
    let mut t = circle_of_four(&f, Hold::Claim, [3.0, 4.0, 9.0, 1.0]);
    t.resolve_waits();
    let claims: Vec<bool> = t.cars.iter().map(|c| !c.reserved.is_empty()).collect();
    assert_eq!(claims, vec![false, false, true, false]);
}

#[test]
fn a_yield_pass_does_not_go_through_a_body() {
    // the four-car crossroads with every car given a pass: the junction still admits one
    // movement at a time (the bodies and claims in the way hold)
    let f = Fixture::new();
    let mut t = plain(&f, crossroads(150.0));
    for k in 0..4 {
        car(&mut t, &f, k, 130.0, 0x181 + k as u64, 5.0);
    }
    for _ in 0..(30.0 / super::fixed_step::SIM_DT) as usize {
        for c in t.cars.iter_mut() {
            c.yield_pass = t.time + 100.0;
        }
        t.tick(super::fixed_step::SIM_DT, None);
        let middle: Vec<usize> = t
            .cars
            .iter()
            .filter(|c| (4..8).contains(&c.state.lane) && c.state.s + c.state.front > 9.0 && c.state.s - c.state.rear < 15.0)
            .map(|c| c.state.lane - 4)
            .collect();
        for a in &middle {
            for b in &middle {
                assert!(a == b || (a + 2) % 4 == *b, "paths {a} and {b} in the middle at once at {:.1} s", t.time);
            }
        }
    }
}

/// Of two decided cars a second apart, the one that can no longer stop firmly goes first,
/// whatever their numbers; close in time and both able to stop (or neither), the first
/// there, a tie to the lower number. A car that rolled into its junction gives way to one
/// with the right of way coming too fast to stop for it, not to one far off, slow or
/// giving way itself.
#[test]
fn the_car_that_cannot_stop_goes_first() {
    // the main road's car at 13.7 m/s 12 m off (id 354) and one rolling out at 4 m/s 5 m
    // off (id 254): the main road's goes, from either side
    assert!(!goes_first((1.1, 5.3, 4.1, 254), (0.8, 11.8, 13.7, 354)));
    assert!(goes_first((0.8, 11.8, 13.7, 354), (1.1, 5.3, 4.1, 254)));
    // both able to stop: the tie to the lower number
    assert!(goes_first((1.0, 20.0, 3.0, 1), (1.1, 20.0, 3.0, 2)));
    assert!(!goes_first((1.1, 20.0, 3.0, 2), (1.0, 20.0, 3.0, 1)));
    let mut o = crate::traffic::AiState::new(0, 0.0, 1);
    o.speed = 14.0;
    o.decel = 2.4;
    assert!(too_close_to_pull_out(1.7, 2.6, &o, false, 50.0, 3.0, 4.0));
    assert!(!too_close_to_pull_out(1.7, 2.6, &o, true, 50.0, 3.0, 4.0), "it gives way itself");
    assert!(!too_close_to_pull_out(1.7, 2.6, &o, false, 120.0, 8.0, 4.0), "far off");
    assert!(!too_close_to_pull_out(5.0, 1.0, &o, false, 50.0, 3.0, 4.0), "this one cannot stop any more");
    o.speed = 2.0;
    assert!(!too_close_to_pull_out(1.7, 2.6, &o, false, 3.0, 1.0, 4.0), "slow enough to stop");
}

#[test]
fn a_driver_letting_in_a_car_that_waits_for_his_queue_takes_his_right_of_way_back() {
    use super::deadlock::Hold;
    // car 0 lets car 1 in, car 1 waits for room on its exit held by car 2, car 2 and
    // car 3 queue behind car 0: the one letting in goes, nobody else is pushed
    let f = Fixture::new();
    let mut t = circle_of_four(&f, Hold::Lead, [3.0; 4]);
    t.cars[0].holds[0].1 = Hold::Courtesy;
    t.cars[1].holds[0].1 = Hold::Exit;
    t.cars[2].holds = vec![(t.cars[3].id, Hold::Lead)];
    t.cars[3].holds = vec![(t.cars[0].id, Hold::Lead)];
    t.resolve_waits();
    let going: Vec<usize> = (0..4).filter(|&k| t.cars[k].yield_pass > t.time).collect();
    assert_eq!(going, vec![0]);
}

#[test]
fn a_circle_held_by_the_rules_and_the_cars_that_follow_is_sorted_out() {
    use super::deadlock::Hold;
    // car 0 gives way to car 1 by the rules, car 1 follows car 2, car 2 waits for car 3's
    // claim, car 3 follows car 0: no body in anybody's way, nobody ever moved on its own
    let f = Fixture::new();
    let tangled = |stood: f32| {
        let mut t = circle_of_four(&f, Hold::Lead, [9.0, 0.0, 6.0, 0.0]);
        t.cars[0].holds[0].1 = Hold::Rule;
        t.cars[2].holds[0].1 = Hold::Claim;
        for c in t.cars.iter_mut() {
            c.stopped = stood;
        }
        t
    };
    // not before they have all stood eight seconds
    let mut t = tangled(5.0);
    t.resolve_waits();
    assert!(t.cars.iter().all(|c| c.yield_pass == f32::MIN));
    let mut t = tangled(10.0);
    t.resolve_waits();
    let going: Vec<usize> = (0..4).filter(|&k| t.cars[k].yield_pass > t.time).collect();
    assert_eq!(going, vec![0], "the longest waiter held by the rules goes, alone");
    let claims: Vec<bool> = t.cars.iter().map(|c| !c.reserved.is_empty()).collect();
    assert_eq!(claims, vec![true, false, false, false], "the others let their claims go");
    // one of them waits for a body: nobody is forced
    let mut t = tangled(10.0);
    t.cars[1].holds[0].1 = Hold::Body;
    t.resolve_waits();
    assert!(t.cars.iter().all(|c| c.yield_pass == f32::MIN));
}

#[test]
fn a_driver_standing_at_his_line_can_still_stop() {
    // standing 60 cm short of the meeting place: he can (his early claim does not take
    // the right of way), rolling at 3 m/s as close he cannot
    assert!(stops_before(0.0, 0.6));
    assert!(!stops_before(0.0, 0.2));
    assert!(!stops_before(3.0, 0.6));
    assert!(stops_before(3.0, 10.0));
    // of two close in time, the one standing at its line does not go first for it
    assert!(goes_first((1.0, 8.0, 4.0, 9), (1.4, 0.6, 0.0, 1)));
}
