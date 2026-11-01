//! Emergency vehicles run headless (`emergency`): the junction kept for one and its red
//! light, who waits for it and who does not, how long the junction is kept, and the cars on
//! its way making way - on its lanes only, within their own, mirrored on a left-hand map,
//! not where the scenery is in the way, not on a ring - and its speed.

use super::bus_service::AiTimetable;
use super::emergency::*;
use super::scenario_tests::{crossroads, snapshot, street, Fixture};
use super::*;
use crate::ai_traffic::setup::RandomTypes;
use crate::traffic::{Lane, LaneBuilder, LaneKey, TrafficLightController};

/// Traffic on `net` with `lights` light programs of one light each (all green at first).
fn sim(f: &Fixture, mut net: Network, lights: usize) -> TrafficSim {
    net.build_grid();
    net.compute_reach();
    let random = RandomTypes { types: Vec::new(), groups: Vec::new(), group_curves: false, group_uvg: Vec::new(), uvg_defaults: Vec::new() };
    let ctls = (0..lights).map(|_| TrafficLightController::new(vec![vec![(6, 1000.0)]], 0.0)).collect();
    TrafficSim::assemble(&f.dir, net, random, ctls, HashMap::new(), (Vec::new(), Vec::new()), Vec::new(), (1.0, 0), 0)
}

/// Light `c` shows `state` from now on (6 green, 0 red).
fn show(t: &mut TrafficSim, c: usize, state: i32) {
    t.lights[c].lights[0] = vec![(state, 1000.0)];
}

/// A car on `lane` at `s` driving at `v` (not slowed down for being placed), with id `id`.
fn car(t: &mut TrafficSim, f: &Fixture, lane: usize, s: f32, id: u64, v: f32) -> u64 {
    let vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
    t.place_car(vehicle, LaneKind::Street, lane, s, f.ty.clone(), 0x9e37 * id + 1, None, Some(id), Some(v), None);
    let c = t.cars.iter_mut().find(|c| c.id == id).unwrap();
    c.fresh = 0.0;
    c.state.speed = v;
    id
}

/// The same, its script claiming priority (`TrafficPriority`).
fn siren(t: &mut TrafficSim, f: &Fixture, lane: usize, s: f32, id: u64, v: f32) -> u64 {
    car(t, f, lane, s, id, v);
    set_priority(t, id, true);
    id
}

fn set_priority(t: &mut TrafficSim, id: u64, on: bool) {
    let c = t.cars.iter_mut().find(|c| c.id == id).unwrap();
    c.vehicle.set_engine_var("TrafficPriority", if on { 1.0 } else { 0.0 });
    // (a script that reads the warning has it among its variables)
    if c.vehicle.var("TrafficPriorityWarningNeeded").is_none() {
        c.vehicle.set_engine_var("TrafficPriorityWarningNeeded", 0.0);
    }
}

fn get(t: &TrafficSim, id: u64) -> &AiCar {
    t.cars.iter().find(|c| c.id == id).expect("the car is still there")
}

fn find(t: &TrafficSim, id: u64) -> Option<&AiCar> {
    t.cars.iter().find(|c| c.id == id)
}

fn step(t: &mut TrafficSim) {
    t.tick(super::fixed_step::SIM_DT, None);
}

fn ticks(t: &mut TrafficSim, secs: f32) {
    for _ in 0..(secs / super::fixed_step::SIM_DT).round() as usize {
        step(t);
    }
}

/// Holds car `id` where it is (a queue that does not move on).
fn hold_still(t: &mut TrafficSim, id: u64) {
    if let Some(c) = t.cars.iter_mut().find(|c| c.id == id) {
        c.state.speed = 0.0;
        c.state.max_speed_kmh = 0.0;
    }
}

/// Do two vehicles' bodies overlap?
fn any_overlap(t: &TrafficSim) -> Option<(u64, u64)> {
    let feet = t.footprints();
    for (a, fa) in feet.iter().enumerate() {
        for fb in feet.iter().skip(a + 1) {
            if fa.car != fb.car && fa.overlaps(fb, 0.05) {
                return Some((t.cars[fa.car].id, t.cars[fb.car].id));
            }
        }
    }
    None
}

/// The crossroads (`crossroads`: lanes 0..4 in from the south, west, north and east, 4..8
/// the junction's paths, 8..12 out) with lights: light 0 for the north-south paths, light 1
/// for the east-west ones.
fn signalled_crossroads(exit: f64) -> Network {
    let mut net = crossroads(exit);
    for (l, light) in [(4, 0), (6, 0), (5, 1), (7, 1)] {
        net.lanes[l].traffic_light = Some((light, 0));
    }
    net
}

/// Is car `c` in the middle of the crossroads (its body over the paths' middle 8 m)?
fn in_middle(c: &AiCar) -> bool {
    (4..8).contains(&c.state.lane) && c.state.s + c.state.front > 8.0 && c.state.s - c.tail < 16.0
}

/// Of the cars `ids`, never two on crossing paths in the middle at once.
fn one_movement(t: &TrafficSim) -> bool {
    let middle: Vec<usize> = t.cars.iter().filter(|c| in_middle(c)).map(|c| c.state.lane - 4).collect();
    middle.iter().all(|a| middle.iter().all(|b| a == b || (a + 2) % 4 == *b))
}

#[test]
fn only_the_emergency_vehicle_crosses_its_red_and_it_crawls() {
    let f = Fixture::new();
    let mut t = sim(&f, signalled_crossroads(150.0), 2);
    show(&mut t, 0, 0);
    let e = siren(&mut t, &f, 0, 70.0, 1, 10.0);
    // a car following it, and one on the other side, both at the same red
    let follower = car(&mut t, &f, 0, 40.0, 2, 10.0);
    let other = car(&mut t, &f, 2, 100.0, 3, 8.0);
    let mut fastest_inside = 0.0f32;
    let mut through = None;
    let mut warned = false;
    for k in 0..(30.0 / super::fixed_step::SIM_DT) as usize {
        step(&mut t);
        let Some(c) = find(&t, e) else { break };
        if c.state.lane == 4 {
            fastest_inside = fastest_inside.max(c.state.speed);
        }
        warned |= c.vehicle.var("TrafficPriorityWarningNeeded") == Some(1.0);
        if through.is_none() && c.state.lane == 8 {
            through = Some(k);
        }
        assert!(any_overlap(&t).is_none());
    }
    assert!(through.is_some(), "the emergency vehicle crossed its red");
    assert!(fastest_inside <= CRAWL + 0.2, "it crossed the red at {fastest_inside:.1} m/s");
    assert!(fastest_inside > 2.0, "it did cross ({fastest_inside:.1} m/s)");
    for id in [follower, other] {
        let c = get(&t, id);
        assert!(matches!(c.state.lane, 0 | 2) && c.state.speed < 0.1, "car {id} kept its red (lane {} at {:.1} m/s)", c.state.lane, c.state.speed);
    }
    assert!(t.emergency.reservations.is_empty(), "the junction is let go once it is through");
    // the warning for its script: on while it crossed against the light, gone now
    assert!(warned);
    assert!(find(&t, e).is_none_or(|c| c.vehicle.var("TrafficPriorityWarningNeeded") == Some(0.0)));
}

#[test]
fn on_green_it_crosses_at_its_own_speed() {
    let f = Fixture::new();
    let mut t = sim(&f, signalled_crossroads(150.0), 2);
    let e = siren(&mut t, &f, 0, 70.0, 1, 10.0);
    let mut fastest_inside = 0.0f32;
    for _ in 0..(15.0 / super::fixed_step::SIM_DT) as usize {
        step(&mut t);
        let Some(c) = find(&t, e) else { break };
        if c.state.lane == 4 {
            fastest_inside = fastest_inside.max(c.state.speed);
            assert!(!t.emergency.crawling(e));
        }
    }
    assert!(fastest_inside > CRAWL + 2.0, "it crept over a green light ({fastest_inside:.1} m/s)");
}

#[test]
fn an_unsignalled_junction_kept_for_it_is_crossed_at_normal_speed_while_the_cross_traffic_waits() {
    let f = Fixture::new();
    let mut t = sim(&f, crossroads(150.0), 0);
    let e = siren(&mut t, &f, 0, 105.0, 1, 10.0);
    // from the east, there as early: without the emergency vehicle it could go first
    let cross = car(&mut t, &f, 3, 100.0, 2, 8.0);
    let mut fastest_inside = 0.0f32;
    let mut cross_waited = false;
    let mut cross_out = false;
    for _ in 0..(40.0 / super::fixed_step::SIM_DT) as usize {
        step(&mut t);
        assert!(one_movement(&t), "two crossing movements in the middle at {:.1} s", t.time);
        if let Some(c) = find(&t, e) {
            if c.state.lane == 4 {
                fastest_inside = fastest_inside.max(c.state.speed);
            }
        }
        let Some(c) = find(&t, cross) else { break };
        cross_waited |= c.state.lane == 3 && c.yielding && c.yield_to == Some(e);
        cross_out |= c.state.lane == 11;
    }
    assert!(fastest_inside > CRAWL + 2.0, "it crept over a junction without lights ({fastest_inside:.1} m/s)");
    assert!(cross_waited, "the cross traffic gave way to it at its entry");
    assert!(cross_out, "and went on once it was through");
}

#[test]
fn crossing_traffic_on_green_waits_for_it_and_moves_off_after() {
    let f = Fixture::new();
    let mut t = sim(&f, signalled_crossroads(150.0), 2);
    show(&mut t, 0, 0);
    let e = siren(&mut t, &f, 0, 80.0, 1, 10.0);
    let cross = car(&mut t, &f, 1, 80.0, 2, 8.0);
    let mut held = false;
    let mut e_out_at = None;
    let mut cross_in_at = None;
    for k in 0..(50.0 / super::fixed_step::SIM_DT) as usize {
        step(&mut t);
        assert!(one_movement(&t), "two crossing movements in the middle at {:.1} s", t.time);
        assert!(any_overlap(&t).is_none());
        let Some(c) = find(&t, cross) else { break };
        held |= c.state.lane == 1 && c.state.speed < 0.3 && c.yielding;
        if cross_in_at.is_none() && c.state.lane == 5 {
            cross_in_at = Some(k);
        }
        if e_out_at.is_none() && find(&t, e).is_some_and(|c| c.state.lane == 8 && c.state.s > c.tail + 0.5) {
            e_out_at = Some(k);
        }
    }
    assert!(held, "the cross traffic waited at its green");
    let (e_out, cross_in) = (e_out_at.expect("the emergency vehicle got through"), cross_in_at.expect("the cross traffic moved off again"));
    assert!(cross_in >= e_out, "the cross traffic went in before the emergency vehicle was out");
}

#[test]
fn a_car_already_in_the_junction_clears_it_first() {
    let f = Fixture::new();
    let mut t = sim(&f, signalled_crossroads(150.0), 2);
    show(&mut t, 0, 0);
    // a car standing in the middle on the east-west path, across the emergency vehicle's
    let occupant = car(&mut t, &f, 5, 11.0, 2, 0.0);
    let e = siren(&mut t, &f, 0, 118.0, 1, 8.0);
    let mut e_waited = false;
    let hold = (12.0 / super::fixed_step::SIM_DT) as usize;
    for k in 0..(40.0 / super::fixed_step::SIM_DT) as usize {
        if k < hold {
            hold_still(&mut t, occupant);
        } else if k == hold {
            t.cars.iter_mut().find(|c| c.id == occupant).unwrap().state.max_speed_kmh = 50.0;
        }
        step(&mut t);
        assert!(any_overlap(&t).is_none(), "bodies met at {:.1} s", t.time);
        e_waited |= k < hold && get(&t, e).state.speed < 0.2 && t.emergency.reservations.iter().any(|r| r.owner == e);
    }
    assert!(e_waited, "the emergency vehicle waited for the car in the junction");
    assert!(find(&t, occupant).is_none_or(|c| c.state.lane == 9), "the car in the junction drove out");
    assert!(find(&t, e).is_none_or(|c| c.state.lane == 8), "the emergency vehicle got through after it");
}

#[test]
fn of_two_emergency_vehicles_the_lower_id_keeps_the_junction() {
    let f = Fixture::new();
    for (low_lane, high_lane) in [(0, 1), (1, 0)] {
        let mut t = sim(&f, crossroads(150.0), 0);
        siren(&mut t, &f, high_lane, 112.0, 7, 8.0);
        siren(&mut t, &f, low_lane, 112.0, 4, 8.0);
        let mut owners: Vec<u64> = Vec::new();
        for _ in 0..(40.0 / super::fixed_step::SIM_DT) as usize {
            step(&mut t);
            assert!(one_movement(&t), "two crossing movements in the middle at {:.1} s", t.time);
            assert!(t.emergency.reservations.len() <= 1, "two kept one junction");
            if let Some(r) = t.emergency.reservations.first() {
                if owners.last() != Some(&r.owner) {
                    owners.push(r.owner);
                }
            }
        }
        assert_eq!(owners.first(), Some(&4), "the lower id first (owners {owners:?})");
        assert!(owners.len() <= 2, "the junction changed hands back and forth: {owners:?}");
        for id in [4, 7] {
            assert!(find(&t, id).is_none_or(|c| c.state.lane >= 8), "{id} got through");
        }
    }
}

#[test]
fn a_timetable_bus_with_traffic_priority_has_no_active_drive() {
    let f = Fixture::new();
    let mut t = sim(&f, signalled_crossroads(150.0), 2);
    show(&mut t, 0, 0);
    let ahead = car(&mut t, &f, 0, 110.0, 2, 6.0);
    let setup = BusSetup {
        route: vec![0, 4, 8],
        stops: Vec::new(),
        number: None,
        hof: None,
        timetable: AiTimetable { line: String::new(), terminus: String::new(), stops: Vec::new() },
    };
    let vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
    let bus = t.place_car(vehicle, LaneKind::Street, 0, 80.0, f.ty.clone(), 0x77, None, Some(1), Some(8.0), Some(setup));
    set_priority(&mut t, bus, true);
    // a car of the random traffic with the same variable is on an active drive
    let e = siren(&mut t, &f, 2, 20.0, 3, 8.0);
    assert!(!active(get(&t, bus)) && active(get(&t, e)));
    for _ in 0..(20.0 / super::fixed_step::SIM_DT) as usize {
        step(&mut t);
        assert!(get(&t, ahead).make_way.is_none(), "a car made way for the timetable bus");
        assert!(t.emergency.reservations.iter().all(|r| r.owner != bus));
        assert!(!get(&t, bus).state.priority_drive);
    }
    assert_eq!(get(&t, bus).state.lane, 0, "the timetable bus kept to its red");
}

#[test]
fn it_cannot_force_its_way_into_a_full_exit() {
    let f = Fixture::new();
    let mut t = sim(&f, crossroads(14.0), 0);
    let front = car(&mut t, &f, 8, 10.0, 2, 0.0);
    let second = car(&mut t, &f, 8, 3.0, 3, 0.0);
    let e = siren(&mut t, &f, 0, 90.0, 1, 8.0);
    for _ in 0..(180.0 / super::fixed_step::SIM_DT) as usize {
        hold_still(&mut t, front);
        hold_still(&mut t, second);
        step(&mut t);
        assert!(any_overlap(&t).is_none());
    }
    let c = get(&t, e);
    assert!(c.state.lane == 0 && c.state.speed < 0.1, "it went into the junction it could not leave (lane {} s {:.1})", c.state.lane, c.state.s);
}

#[test]
fn the_junction_is_kept_until_its_rear_is_out() {
    let f = Fixture::new();
    let mut t = sim(&f, crossroads(150.0), 0);
    let e = siren(&mut t, &f, 0, 118.0, 1, 7.0);
    {
        // a long rear (an articulated bus's last section reaches as far)
        let c = t.cars.iter_mut().find(|c| c.id == e).unwrap();
        c.state.rear = 20.0;
        c.state.length = c.state.front + 20.0;
    }
    // from the west, standing just short of its line
    let cross = car(&mut t, &f, 1, 140.0, 2, 0.0);
    let mut kept_while_in = true;
    let mut released = false;
    for _ in 0..(40.0 / super::fixed_step::SIM_DT) as usize {
        step(&mut t);
        let Some(c) = find(&t, e) else { break };
        let origin_y = match c.state.lane {
            0 => c.state.s - 162.0,
            4 => c.state.s - 12.0,
            8 => c.state.s + 12.0,
            _ => f32::MAX,
        };
        let rear_y = origin_y - c.state.rear;
        let kept = t.emergency.reservations.iter().any(|r| r.owner == e);
        // (its rear still over the paths: they are kept)
        if c.state.lane != 0 && rear_y < 12.0 {
            kept_while_in &= kept;
        }
        released |= !kept && rear_y > 12.0;
        // the car from the west reaches the emergency vehicle's path only once its rear has
        // crossed the east-west path
        let x = get(&t, cross);
        let front_x = match x.state.lane {
            1 => x.state.s + x.state.front - 162.0,
            5 => x.state.s + x.state.front - 12.0,
            9 => x.state.s + x.state.front + 12.0,
            _ => f32::MIN,
        };
        assert!(front_x < 0.5 || rear_y > -1.75 + 1.5, "the car from the west went into the long rear (front x {front_x:.1}, rear y {rear_y:.1})");
    }
    assert!(kept_while_in, "the junction was let go with the rear still in it");
    assert!(released, "and let go once the rear was out");
    assert!(get(&t, cross).state.lane != 1, "the car from the west went on after it");
}

#[test]
fn it_never_crosses_an_occupied_pedestrian_crossing() {
    let f = Fixture::new();
    let mut net = signalled_crossroads(150.0);
    // a footpath of the junction object across the northbound path, 4 m past the middle
    let mut walk = LaneBuilder::arc(DVec3::new(-10.0, 4.0, 0.0), 90.0, 20.0, 0.0, 0.0, LaneKind::Sidewalk, 2.0);
    walk.source = 2;
    walk.key = Some(LaneKey { tile: (0, 0), id: 1, path: 20 });
    net.lanes.push(walk);
    let walk = net.lanes.len() - 1;
    net.link(1.5);
    let mut t = sim(&f, net, 2);
    show(&mut t, 0, 0);
    assert!(t.net.walks[4].iter().any(|w| w.0 == walk), "the footpath crosses the path");
    t.walkers = vec![(walk, 11.75)];
    let e = siren(&mut t, &f, 0, 90.0, 1, 10.0);
    for _ in 0..(30.0 / super::fixed_step::SIM_DT) as usize {
        step(&mut t);
        let c = get(&t, e);
        assert!(c.state.lane == 0 || (c.state.lane == 4 && c.state.s + c.state.front < 16.0 - 2.0), "it drove onto the crossing (lane {} s {:.1})", c.state.lane, c.state.s);
    }
    t.walkers.clear();
    ticks(&mut t, 15.0);
    assert!(find(&t, e).is_none_or(|c| c.state.lane == 8), "it went on once the crossing was clear");
}

#[test]
fn no_junction_stays_kept_over_switches_removals_and_network_changes() {
    let f = Fixture::new();
    let mut t = sim(&f, signalled_crossroads(150.0), 2);
    show(&mut t, 0, 0);
    let a = siren(&mut t, &f, 0, 100.0, 1, 9.0);
    let b = siren(&mut t, &f, 1, 95.0, 2, 9.0);
    let c = siren(&mut t, &f, 2, 100.0, 3, 9.0);
    let normal = car(&mut t, &f, 3, 100.0, 4, 9.0);
    for k in 0..(60.0 / super::fixed_step::SIM_DT) as usize {
        let secs = k as f32 * super::fixed_step::SIM_DT;
        // the siren switched off and on, a car taken off holding a junction, the network
        // changing under them
        if k % 150 == 0 && find(&t, a).is_some() {
            let on = (k / 150) % 2 == 1;
            set_priority(&mut t, a, on);
        }
        if (secs - 4.0).abs() < 1e-3 {
            if let Some(i) = t.cars.iter().position(|x| x.id == b) {
                t.take_off(i, super::control::Removal::Finished);
            }
        }
        if k % 400 == 0 {
            t.lanes_generation += 1;
        }
        step(&mut t);
        let sirens: Vec<u64> = t.emergency.sirens.iter().map(|s| s.id).collect();
        for r in &t.emergency.reservations {
            assert!(sirens.contains(&r.owner), "a junction kept for {} that is no emergency vehicle now", r.owner);
        }
        assert!(t.emergency.reservations.len() <= sirens.len());
        assert!(t.emergency.requests.values().all(|r| sirens.contains(&r.from)));
    }
    for id in [a, c] {
        if find(&t, id).is_some() {
            set_priority(&mut t, id, false);
        }
    }
    step(&mut t);
    assert!(t.emergency.reservations.is_empty() && t.emergency.requests.is_empty(), "something was left kept");
    show(&mut t, 0, 6);
    ticks(&mut t, 40.0);
    for id in [a, c, normal] {
        assert!(find(&t, id).is_none_or(|x| x.state.lane >= 8), "{id} never got through");
    }
    assert!(t.cars.iter().all(|x| x.make_way.is_none()));
}

/// A road north of `lanes` lanes side by side (3.5 m apart and wide; lane 0 the leftmost),
/// 600 m long, and a road of its own 14 m to the east of it (the last lane).
fn carriageway(lanes: usize) -> Network {
    let mut ls: Vec<Lane> = (0..lanes).map(|k| {
        let mut l = LaneBuilder::arc(DVec3::new(-3.5 * (lanes - 1 - k) as f64, 0.0, 0.0), 0.0, 600.0, 0.0, 0.0, LaneKind::Street, 3.5);
        l.width = 3.5;
        l
    }).collect();
    let mut parallel = street(DVec3::new(14.0, 0.0, 0.0), 0.0, 600.0);
    parallel.width = 3.5;
    ls.push(parallel);
    let mut net = Network { lanes: ls, ..Default::default() };
    net.link(1.5);
    for k in 1..lanes {
        net.lanes[k - 1].right = Some(k);
        net.lanes[k].left = Some(k - 1);
    }
    net
}

#[test]
fn the_cars_on_its_way_make_way_a_road_beside_does_not() {
    let f = Fixture::new();
    let mut t = sim(&f, carriageway(2), 0);
    let e = siren(&mut t, &f, 1, 40.0, 1, 12.0);
    let outer = car(&mut t, &f, 1, 95.0, 2, 9.0);
    let inner = car(&mut t, &f, 0, 90.0, 3, 9.0);
    let beside = car(&mut t, &f, 2, 95.0, 4, 9.0);
    let (mut outer_out, mut inner_in, mut corridor) = (0.0f32, 0.0f32, 0.0f32);
    let mut passed = false;
    for _ in 0..(30.0 / super::fixed_step::SIM_DT) as usize {
        step(&mut t);
        assert!(any_overlap(&t).is_none(), "bodies met at {:.1} s", t.time);
        let (o, i, b) = (get(&t, outer), get(&t, inner), get(&t, beside));
        outer_out = outer_out.max(o.state.lateral);
        inner_in = inner_in.min(i.state.lateral);
        corridor = corridor.min(get(&t, e).state.lateral);
        // within their lanes (3.5 m)
        for c in [o, i] {
            assert!(c.state.lateral.abs() + c.half_width <= 1.75 + 1e-3, "car {} left its lane ({:.2})", c.id, c.state.lateral);
        }
        assert!(b.make_way.is_none() && b.state.lateral.abs() < 1e-3 && b.state.speed > 7.0, "the road beside reacted");
        passed |= get(&t, e).state.s > o.state.s + 20.0;
    }
    assert!(outer_out > 0.3, "the outer lane moved to the kerb ({outer_out:.2})");
    assert!(inner_in < -0.3, "the inner lane moved inward ({inner_in:.2})");
    assert!(corridor < -1.0, "the emergency vehicle took the corridor ({corridor:.2})");
    assert!(passed, "it got past them");
    // let go once it has passed: back to the middle and on their way
    for id in [outer, inner] {
        let c = get(&t, id);
        assert!(c.make_way.is_none() && c.state.lateral.abs() < 0.1 && c.state.speed > 5.0, "car {id} did not go on (lateral {:.2}, {:.1} m/s)", c.state.lateral, c.state.speed);
    }
}

/// How far across its lane the car on lane `lane` of `lanes` moves for an emergency vehicle
/// behind it in its lane (the largest offset either way, signed).
fn moved_aside(lanes: usize, lane: usize, left_hand: bool) -> f32 {
    let f = Fixture::new();
    let mut net = carriageway(lanes);
    net.left_hand = left_hand;
    let mut t = sim(&f, net, 0);
    siren(&mut t, &f, lane, 40.0, 1, 10.0);
    let c = car(&mut t, &f, lane, 90.0, 2, 8.0);
    let mut most = 0.0f32;
    for _ in 0..(8.0 / super::fixed_step::SIM_DT) as usize {
        step(&mut t);
        let x = get(&t, c);
        assert!(x.state.lateral.abs() + x.half_width <= 1.75 + 1e-3, "it left its lane ({:.2})", x.state.lateral);
        if x.state.lateral.abs() > most.abs() {
            most = x.state.lateral;
        }
    }
    most
}

#[test]
fn making_way_is_mirrored_on_a_left_hand_map() {
    // one lane: to the kerb (the right, the left on a left-hand map)
    assert!(moved_aside(1, 0, false) > 0.3);
    assert!(moved_aside(1, 0, true) < -0.3);
    // three lanes: the middle one moves outward - and the inner lane inward
    assert!(moved_aside(3, 1, false) > 0.3);
    assert!(moved_aside(3, 1, true) < -0.3);
    assert!(moved_aside(3, 0, false) < -0.3);
    assert!(moved_aside(3, 2, true) > 0.3);
}

#[test]
fn the_scenery_can_forbid_the_move_aside_but_not_a_stop() {
    let f = Fixture::new();
    // lane 0 north to a signal (the light's path 1, 20 m), lane 2 on
    let approach = street(DVec3::ZERO, 0.0, 150.0);
    let mut path = street(DVec3::new(0.0, 150.0, 0.0), 0.0, 20.0);
    path.source = 2;
    path.key = Some(LaneKey { tile: (0, 0), id: 5, path: 0 });
    path.traffic_light = Some((0, 0));
    let on = street(DVec3::new(0.0, 170.0, 0.0), 0.0, 300.0);
    for wall in [false, true] {
        let mut net = Network { lanes: vec![approach.clone(), path.clone(), on.clone()], ..Default::default() };
        net.link(1.5);
        let mut t = sim(&f, net, 1);
        show(&mut t, 0, 0);
        if wall {
            // a wall along the kerb on the right
            t.scenery_probe = Some(Arc::new(|b: &Obb| {
                let (s, c) = b.heading.sin_cos();
                let (fwd, right) = (DVec2::new(s, c), DVec2::new(c, -s));
                [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)].iter().any(|&(a, r)| (b.center + right * (a * b.half.x) + fwd * (r * b.half.y)).x > 1.2)
            }));
        }
        let me = car(&mut t, &f, 0, 100.0, 2, 8.0);
        siren(&mut t, &f, 0, 50.0, 1, 10.0);
        let mut most = 0.0f32;
        for _ in 0..(20.0 / super::fixed_step::SIM_DT) as usize {
            step(&mut t);
            let c = get(&t, me);
            most = most.max(c.state.lateral);
            let over = if c.state.lane == 0 { c.state.s + c.state.front - 150.0 } else { 1.0 };
            assert!(over <= 0.0, "the red light no longer held it");
        }
        if wall {
            assert!(most < 0.05, "it moved into the wall ({most:.2})");
            assert!(get(&t, me).make_way.is_some(), "it was asked all the same");
        } else {
            assert!(most > 0.3, "without the wall it moves aside ({most:.2})");
        }
    }
}

/// A roundabout of radius 15 m driven anticlockwise (junction object 9): lanes 0..4 its
/// quarters, 4 an entry from the south-west running into it, 5 the road to the entry.
fn roundabout() -> Network {
    let r: f64 = 15.0;
    let mut lanes = Vec::new();
    let object = |mut l: Lane, path: u16| {
        l.source = 2;
        l.key = Some(LaneKey { tile: (0, 0), id: 9, path });
        l
    };
    for k in 0..4 {
        let a = -(k as f64) * 90.0;
        let p = DVec3::new(r * a.to_radians().sin(), r * a.to_radians().cos(), 0.0);
        lanes.push(object(LaneBuilder::arc(p, a - 90.0, r * std::f64::consts::FRAC_PI_2, -r, 0.0, LaneKind::Street, 3.0), k as u16));
    }
    lanes.push(object(LaneBuilder::arc(DVec3::new(-12.0, -27.0, 0.0), 0.0, 12.0 * std::f64::consts::FRAC_PI_2, 12.0, 0.0, LaneKind::Street, 3.0), 4));
    lanes.push(street(DVec3::new(-12.0, -127.0, 0.0), 0.0, 100.0));
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    net
}

#[test]
fn a_car_on_a_ring_drives_on() {
    let f = Fixture::new();
    let mut t = sim(&f, roundabout(), 0);
    assert!(t.net.lanes[..4].iter().all(|l| l.ring));
    let ring = car(&mut t, &f, 1, 8.0, 2, 5.0);
    siren(&mut t, &f, 0, 4.0, 1, 5.0);
    for k in 0..(10.0 / super::fixed_step::SIM_DT) as usize {
        step(&mut t);
        let c = get(&t, ring);
        assert!(c.make_way.is_none(), "the ring car stopped to make way");
        if k as f32 * super::fixed_step::SIM_DT > 2.0 {
            assert!(c.state.speed > 2.0, "the ring car stood in the way ({:.1} m/s)", c.state.speed);
        }
        assert!(any_overlap(&t).is_none());
    }
}

#[test]
fn the_speed_bonus_holds_on_an_active_drive_only_and_within_the_vehicle() {
    // (1.5 times the limit, at most 30 km/h over it, never past the vehicle's own top speed)
    assert!((bonus_limit(50.0, 200.0) - 75.0).abs() < 1e-3);
    assert!((bonus_limit(100.0, 200.0) - 130.0).abs() < 1e-3);
    assert!((bonus_limit(50.0, 58.0) - 58.0).abs() < 1e-3);
    let f = Fixture::new();
    let mut road = street(DVec3::ZERO, 0.0, 3000.0);
    road.speed_limit_kmh = 50.0;
    let mut net = Network { lanes: vec![road], ..Default::default() };
    net.link(1.5);
    let run = |on: bool, max: f32| {
        let mut n = Network { lanes: net.lanes.clone(), ..Default::default() };
        n.link(1.5);
        let mut t = sim(&f, n, 0);
        let e = car(&mut t, &f, 0, 10.0, 1, 13.0);
        set_priority(&mut t, e, on);
        t.cars[0].state.max_speed_kmh = max;
        ticks(&mut t, 40.0);
        get(&t, e).state.speed * 3.6
    };
    let fast = run(true, 200.0);
    assert!((fast - 75.0).abs() < 1.5, "on a drive at {fast:.1} km/h");
    let capped = run(true, 58.0);
    assert!(capped < 58.5 && capped > 56.0, "its top speed at {capped:.1} km/h");
    let plain = run(false, 200.0);
    assert!(plain < 50.0 * 1.15 + 0.5, "without the siren at {plain:.1} km/h");
}

/// An emergency vehicle and four cars at the crossroads, placed in `order` (with fixed ids).
fn emergency_crossroads(f: &Fixture, order: &[usize]) -> TrafficSim {
    let mut t = sim(f, signalled_crossroads(150.0), 2);
    show(&mut t, 0, 0);
    let cars = [(0, 80.0, true), (0, 120.0, false), (1, 110.0, false), (2, 115.0, false), (3, 100.0, false)];
    for &k in order {
        let (lane, s, e) = cars[k];
        if e {
            siren(&mut t, f, lane, s, 100 + k as u64, 9.0);
        } else {
            car(&mut t, f, lane, s, 100 + k as u64, 7.0);
        }
    }
    t
}

#[test]
fn an_emergency_scene_drives_the_same_at_any_frame_rate_and_in_any_order() {
    let f = Fixture::new();
    let run_at = |fps: f32, order: &[usize]| {
        let mut t = emergency_crossroads(&f, order);
        for _ in 0..(25.0 * fps).round() as usize {
            t.advance(1.0 / fps, None);
        }
        snapshot(&t)
    };
    let a = run_at(60.0, &[0, 1, 2, 3, 4]);
    assert_eq!(a, run_at(30.0, &[0, 1, 2, 3, 4]));
    assert_eq!(a, run_at(144.0, &[0, 1, 2, 3, 4]));
    assert_eq!(a, run_at(60.0, &[4, 2, 0, 3, 1]));
    // (and the emergency vehicle did get through)
    assert!(a.iter().any(|c| c.0 == 100 && c.1 == 8), "{a:?}");
}

#[test]
fn the_warning_is_for_a_vehicle_close_ahead_not_one_far_off() {
    let f = Fixture::new();
    let mut road = street(DVec3::ZERO, 0.0, 1000.0);
    road.speed_limit_kmh = 50.0;
    let mut net = Network { lanes: vec![road], ..Default::default() };
    net.link(1.5);
    let mut t = sim(&f, net, 0);
    let e = siren(&mut t, &f, 0, 100.0, 1, 0.0);
    let ahead = car(&mut t, &f, 0, 200.0, 2, 0.0);
    for _ in 0..10 {
        hold_still(&mut t, e);
        hold_still(&mut t, ahead);
        step(&mut t);
    }
    assert_eq!(get(&t, e).vehicle.var("TrafficPriorityWarningNeeded"), Some(0.0), "a car 95 m ahead");
    t.cars.iter_mut().find(|c| c.id == ahead).unwrap().state.s = 140.0;
    for _ in 0..10 {
        hold_still(&mut t, e);
        hold_still(&mut t, ahead);
        step(&mut t);
    }
    assert_eq!(get(&t, e).vehicle.var("TrafficPriorityWarningNeeded"), Some(1.0), "a car 35 m ahead");
}

/// A road with a lane each way: lane 0 north along x = 0, lane 1 south 3.5 m to the west.
fn two_way() -> Network {
    let mut net = Network { lanes: vec![street(DVec3::ZERO, 0.0, 1500.0), street(DVec3::new(-3.5, 1500.0, 0.0), 180.0, 1500.0)], ..Default::default() };
    net.link(1.5);
    net
}

#[test]
fn it_goes_round_a_car_making_way_but_waits_for_the_oncoming_traffic() {
    let f = Fixture::new();
    for oncoming in [false, true] {
        let mut t = sim(&f, two_way(), 0);
        let slow = car(&mut t, &f, 0, 150.0, 2, 8.0);
        let e = siren(&mut t, &f, 0, 90.0, 1, 12.0);
        let mut next_id = 10;
        let mut made_way = false;
        let mut past_at = None;
        for k in 0..(60.0 / super::fixed_step::SIM_DT) as usize {
            let secs = k as f32 * super::fixed_step::SIM_DT;
            // a stream of cars coming the other way for the first 25 s
            if oncoming && secs < 25.0 && !t.cars.iter().any(|c| c.state.lane == 1 && c.state.s < 1270.0) {
                car(&mut t, &f, 1, 1240.0, next_id, 11.0);
                next_id += 1;
            }
            step(&mut t);
            assert!(any_overlap(&t).is_none(), "bodies met at {secs:.1} s");
            let (Some(c), Some(x)) = (find(&t, e), find(&t, slow)) else { break };
            made_way |= x.make_way.is_some_and(|w| w.from == e);
            if past_at.is_none() && c.state.s - c.state.rear > x.state.s + x.state.front {
                past_at = Some(secs);
            }
        }
        assert!(made_way, "the slow car made way");
        let past = past_at.expect("the emergency vehicle got past");
        if oncoming {
            assert!(past > 25.0, "it went round while the other lane's traffic came ({past:.1} s)");
        } else {
            assert!(past < 25.0, "it took {past:.1} s to get past");
        }
    }
}

#[test]
fn the_players_and_a_lan_players_emergency_vehicles_are_made_way_for() {
    let f = Fixture::new();
    for lan in [false, true] {
        let mut t = sim(&f, two_way(), 0);
        let c = car(&mut t, &f, 0, 120.0, 2, 8.0);
        let bus: PlayerBox = (DVec3::new(0.0, 80.0, 0.0), 0.0, 6.0, 1.25, 8.0);
        let mut moved = 0.0f32;
        for _ in 0..(4.0 / super::fixed_step::SIM_DT) as usize {
            if lan {
                t.others = vec![(5, bus)];
                t.other_priority = [5].into_iter().collect();
                t.tick(super::fixed_step::SIM_DT, None);
            } else {
                t.player_priority = true;
                t.tick(super::fixed_step::SIM_DT, Some(bus));
            }
            moved = moved.max(get(&t, c).state.lateral);
        }
        let from = if lan { lan_siren_id(5) } else { super::stats::WAITS_ON_PLAYER };
        assert_eq!(get(&t, c).make_way.map(|w| w.from), Some(from));
        assert!(moved > 0.3, "it moved to the kerb ({moved:.2})");
    }
}

/// #2431: an emergency vehicle that stands before a junction (at a red light, at a stop, in
/// a queue - or a player's bus whose script sets `TrafficPriority`) does not keep it: the
/// cross traffic goes on its green after `STANDING_LETS_GO`, and the junction is kept again
/// once it moves.
#[test]
fn a_standing_emergency_vehicle_does_not_keep_the_junction() {
    let f = Fixture::new();
    let mut t = sim(&f, signalled_crossroads(150.0), 2);
    // north-south red, east-west green
    show(&mut t, 0, 0);
    let e = siren(&mut t, &f, 0, 110.0, 1, 0.0);
    let cross = car(&mut t, &f, 1, 60.0, 2, 8.0);
    let mut kept = false;
    let mut through_at = None;
    for _ in 0..(40.0 / super::fixed_step::SIM_DT) as usize {
        hold_still(&mut t, e);
        step(&mut t);
        kept |= t.emergency.reservations.iter().any(|r| r.owner == e);
        if through_at.is_none() && find(&t, cross).is_none_or(|c| c.state.lane == 9) {
            through_at = Some(t.time);
        }
    }
    assert!(kept, "the junction was kept for it while it came");
    let at = through_at.expect("the cross traffic went on its green");
    assert!(at < STANDING_LETS_GO + 15.0, "the cross traffic waited for a standing emergency vehicle until {at:.1} s");
    assert!(t.emergency.reservations.is_empty(), "a junction is still kept for one that stands");
    // it moves again: the junction is kept for it again
    t.cars.iter_mut().find(|c| c.id == e).unwrap().state.max_speed_kmh = 50.0;
    show(&mut t, 0, 6);
    let mut again = false;
    for _ in 0..(10.0 / super::fixed_step::SIM_DT) as usize {
        step(&mut t);
        again |= t.emergency.reservations.iter().any(|r| r.owner == e);
    }
    assert!(again, "the junction was not kept again once it moved");
}

/// A car whose way through the junction meets nothing of the emergency vehicle's (the
/// oncoming traffic going straight on beside it) is not held at its entry.
#[test]
fn a_car_whose_way_meets_nothing_of_its_way_is_not_held() {
    let f = Fixture::new();
    let mut t = sim(&f, crossroads(150.0), 0);
    let e = siren(&mut t, &f, 0, 110.0, 1, 6.0);
    // from the north, straight on south (lane 6 runs beside lane 4 the other way)
    let oncoming = car(&mut t, &f, 2, 120.0, 2, 6.0);
    assert!(!t.net.crossings[4].iter().any(|c| c.other == 6), "the scene's two ways do not meet");
    let mut held = false;
    for _ in 0..(15.0 / super::fixed_step::SIM_DT) as usize {
        step(&mut t);
        if t.emergency.reservations.iter().any(|r| r.owner == e) {
            held |= find(&t, oncoming).is_some_and(|c| c.yield_to == Some(e));
        }
    }
    assert!(!held, "the oncoming car waited for the emergency vehicle's junction");
}
