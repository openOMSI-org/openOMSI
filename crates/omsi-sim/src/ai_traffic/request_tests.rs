//! Who asks a light for green, and from how far: a car standing at the line, a train
//! coming to a level crossing, a bus waiting beside one.

use super::scenario_tests::{street, traffic, Fixture};
use super::*;
use crate::traffic::LaneBuilder;

/// A road of `before` m to a light path of 10 m (light 0 of program 0), and a road on.
fn road_to_light(before: f64) -> Network {
    let mut lanes = vec![street(DVec3::new(0.0, -before, 0.0), 0.0, before)];
    let mut lit = street(DVec3::ZERO, 0.0, 10.0);
    lit.traffic_light = Some((0, 0));
    lanes.push(lit);
    lanes.push(street(DVec3::new(0.0, 10.0, 0.0), 0.0, 200.0));
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    net
}

/// A track of `before` m to a crossing path of 8 m with light 0, and a track on.
fn track_to_crossing(before: f64) -> Network {
    let rail = |start: DVec3, length: f64| LaneBuilder::arc(start, 0.0, length, 0.0, 0.0, LaneKind::Rail, 1.435);
    let mut lit = rail(DVec3::ZERO, 8.0);
    lit.traffic_light = Some((0, 0));
    let lanes = vec![rail(DVec3::new(0.0, -before, 0.0), before), lit, rail(DVec3::new(0.0, 8.0, 0.0), 300.0)];
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    net
}

/// A track of `pieces` pieces of 25 m each to the crossing path of 8 m with light 0, and a
/// track on: the lanes in order, the crossing at `pieces`.
fn pieced_track(pieces: usize) -> Network {
    let rail = |start: DVec3, length: f64| LaneBuilder::arc(start, 0.0, length, 0.0, 0.0, LaneKind::Rail, 1.435);
    let before = 25.0 * pieces as f64;
    let mut lanes: Vec<crate::traffic::Lane> = (0..pieces).map(|k| rail(DVec3::new(0.0, 25.0 * k as f64 - before, 0.0), 25.0)).collect();
    let mut lit = rail(DVec3::ZERO, 8.0);
    lit.traffic_light = Some((0, 0));
    lanes.push(lit);
    lanes.push(rail(DVec3::new(0.0, 8.0, 0.0), 300.0));
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    net
}

/// One light, red for a long while, asking `approach` m before its line.
fn program(approach: Option<f32>) -> TrafficLightController {
    let mut c = TrafficLightController::new(vec![vec![(0, 400.0), (6, 20.0)]], 0.0);
    c.approach = vec![approach];
    c
}

/// A vehicle of its own type file: `head` is what the file says before its model.
fn vehicle_type(f: &Fixture, name: &str, head: &str) -> Arc<VehicleType> {
    let path = f.dir.join(name);
    std::fs::write(&path, format!("{head}[model]\nmodel.cfg\n")).unwrap();
    Arc::new(VehicleType::load_ai(&f.dir, &path).unwrap())
}

/// Put a vehicle of `ty` on `lane` with its front `gap` m before the end of that lane,
/// at `speed`.
fn place_front(t: &mut TrafficSim, ty: &Arc<VehicleType>, kind: LaneKind, lane: usize, gap: f32, speed: f32, seed: u64) -> u64 {
    let vehicle = VehicleInstance::new(ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
    let id = t.place_car(vehicle, kind, lane, 0.0, ty.clone(), seed, None, None, Some(speed), None);
    let len = t.net.lanes[lane].length();
    let c = t.cars.iter_mut().find(|c| c.id == id).unwrap();
    c.state.s = len - gap - c.state.front;
    // (its way ahead from there, as it would have planned it driving up)
    c.state.plan_next(&t.net);
    id
}

/// Whether light 0 is asked after one step with the cars as they are.
fn asked(t: &mut TrafficSim) -> bool {
    t.tick(0.02, None);
    t.lights[0].request[0]
}

#[test]
fn a_car_standing_at_the_line_asks_a_light_of_no_distance() {
    let f = Fixture::new();
    // the program says 0: only who is at the line asks
    let mut t = traffic(&f, road_to_light(120.0));
    t.lights = vec![program(Some(0.0))];
    let ty = f.ty.clone();
    place_front(&mut t, &ty, LaneKind::Street, 0, 0.6, 0.0, 0x31);
    assert!(asked(&mut t), "the car waiting just short of the line never asked");
    // driving up 30 m away it does not ask yet
    let mut t = traffic(&f, road_to_light(120.0));
    t.lights = vec![program(Some(0.0))];
    place_front(&mut t, &ty, LaneKind::Street, 0, 30.0, 8.0, 0x32);
    assert!(!asked(&mut t));
    // nor standing 30 m away in a queue
    let mut t = traffic(&f, road_to_light(120.0));
    t.lights = vec![program(Some(0.0))];
    place_front(&mut t, &ty, LaneKind::Street, 0, 30.0, 0.0, 0x33);
    assert!(!asked(&mut t));
}

#[test]
fn a_train_asks_a_crossing_without_a_distance_from_its_braking_distance() {
    let f = Fixture::new();
    let train = vehicle_type(&f, "railcar.ovh", "[type]\n2\n\n");
    let tram = vehicle_type(&f, "tram.ovh", "");
    for approach in [None, Some(0.0)] {
        // a train 450 m off asks; a tram there does not, nor a train 750 m off
        for (ty, gap, expect) in [(&train, 450.0, true), (&tram, 450.0, false), (&train, 750.0, false)] {
            let mut t = traffic(&f, track_to_crossing(900.0));
            t.lights = vec![program(approach)];
            place_front(&mut t, ty, LaneKind::Rail, 0, gap, 20.0, 0x77);
            assert_eq!(asked(&mut t), expect, "approach {approach:?}, gap {gap}, train {}", Arc::ptr_eq(ty, &train));
        }
    }
    // a distance the program sets counts for the train as for a car
    let mut t = traffic(&f, track_to_crossing(900.0));
    t.lights = vec![program(Some(120.0))];
    place_front(&mut t, &train, LaneKind::Rail, 0, 450.0, 20.0, 0x78);
    assert!(!asked(&mut t));
    let mut t = traffic(&f, track_to_crossing(900.0));
    t.lights = vec![program(Some(120.0))];
    place_front(&mut t, &train, LaneKind::Rail, 0, 100.0, 20.0, 0x79);
    assert!(asked(&mut t));
}

#[test]
fn a_bus_beside_a_level_crossing_does_not_ring_it() {
    let f = Fixture::new();
    let mut t = traffic(&f, track_to_crossing(200.0));
    t.lights = vec![program(None)];
    // standing off the roads (a yard, a bay) just before the crossing, facing along the
    // track 3 m beside it
    let bus: PlayerBox = (DVec3::new(3.0, -8.0, 0.0), 0.0, 6.0, 1.25, 0.0);
    t.tick(0.02, Some(bus));
    assert!(!t.lights[0].request[0], "the bus beside the track rang the crossing shut");
    // a depot gate on a road path ahead of it still opens for it
    let mut t = traffic(&f, road_to_light(200.0));
    t.net.lanes[0].kind = LaneKind::Sidewalk;
    t.lights = vec![program(None)];
    t.tick(0.02, Some((DVec3::new(3.0, -8.0, 0.0), 0.0, 6.0, 1.25, 0.0)));
    assert!(t.lights[0].request[0]);
}

#[test]
fn a_timetable_train_asks_past_its_planned_lanes() {
    // the route runs over many short pieces: the plan covers ten of them, the crossing is
    // some 480 m on
    let f = Fixture::new();
    let train = vehicle_type(&f, "railcar.ovh", "[type]\n2\n\n");
    let mut t = traffic(&f, pieced_track(24));
    t.lights = vec![program(None)];
    let vehicle = VehicleInstance::new(train.clone(), crate::VehicleHost::new(crate::SimClock::default()));
    let id = t.place_car(vehicle, LaneKind::Rail, 4, 0.0, train.clone(), 0x91, None, None, Some(20.0), None);
    let i = t.cars.iter().position(|c| c.id == id).unwrap();
    let mut st = t.cars[i].state.clone();
    st.set_route(&t.net, (4..26).collect(), 5.0);
    t.cars[i].state = st;
    assert!(asked(&mut t), "the train never asked the crossing beyond its plan");
}

/// A track of 700 m forking into two crossing paths, light 0 straight on and light 1 on
/// the branch to the right.
fn forked_track() -> Network {
    let rail = |start: DVec3, length: f64, radius: f64| LaneBuilder::arc(start, 0.0, length, radius, 0.0, LaneKind::Rail, 1.435);
    let mut straight = rail(DVec3::ZERO, 10.0, 0.0);
    straight.traffic_light = Some((0, 0));
    let mut branch = rail(DVec3::ZERO, 10.0, 60.0);
    branch.traffic_light = Some((0, 1));
    let mut net = Network { lanes: vec![rail(DVec3::new(0.0, -700.0, 0.0), 700.0, 0.0), straight, branch], ..Default::default() };
    net.link(1.5);
    net
}

#[test]
fn the_players_train_asks_the_lights_on_every_branch_ahead() {
    let f = Fixture::new();
    let run = |s: f32, along: bool, train: bool, speed: f32| {
        let mut t = traffic(&f, forked_track());
        let mut ctl = TrafficLightController::new(vec![vec![(0, 400.0), (6, 20.0)]; 2], 0.0);
        ctl.approach = vec![None, None];
        t.lights = vec![ctl];
        t.player_rail = Some(PlayerRail { lane: 0, s, along, train });
        let p = t.net.lanes[0].at(s).0;
        t.tick(0.02, Some((p, 0.0, 10.0, 1.4, speed)));
        (t.lights[0].request[0], t.lights[0].request[1])
    };
    // a train 450 m off asks both branches' lights
    assert_eq!(run(250.0, true, true, 15.0), (true, true));
    // a tram there does not, but does 30 m off (the default distance)
    assert_eq!(run(250.0, true, false, 8.0), (false, false));
    assert_eq!(run(660.0, true, false, 8.0), (true, true));
    // backing away from the fork it asks nothing
    assert_eq!(run(660.0, true, false, -2.0), (false, false));
}

fn junction_with_a_second_light(cross_traffic_joins: bool) -> Network {
    let path = |start: DVec3, n: u16, light: (usize, usize)| {
        let mut lane = street(start, 0.0, 10.0);
        lane.source = 2;
        lane.key = Some(crate::traffic::LaneKey { tile: (0, 0), id: 10, path: n });
        lane.traffic_light = Some(light);
        lane
    };
    let mut lanes = vec![
        street(DVec3::new(0.0, -100.0, 0.0), 0.0, 100.0),
        path(DVec3::ZERO, 0, (0, 0)),
        path(DVec3::new(0.0, 10.0, 0.0), 1, (0, 1)),
        street(DVec3::new(0.0, 20.0, 0.0), 0.0, 200.0),
    ];
    if cross_traffic_joins {
        lanes.push(street(DVec3::new(0.0, -30.0, 0.0), 0.0, 40.0));
    }
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    net
}

#[test]
fn a_car_turning_onto_the_cross_traffics_path_does_not_ask_its_light() {
    let f = Fixture::new();
    let ty = f.ty.clone();
    for (cross_traffic_joins, second_asked) in [(true, false), (false, true)] {
        let mut t = traffic(&f, junction_with_a_second_light(cross_traffic_joins));
        assert_eq!(t.net.prev[2].len(), if cross_traffic_joins { 2 } else { 1 });
        let mut ctl = TrafficLightController::new(vec![vec![(0, 400.0), (6, 20.0)]; 2], 0.0);
        ctl.approach = vec![Some(100.0), Some(100.0)];
        t.lights = vec![ctl];
        place_front(&mut t, &ty, LaneKind::Street, 0, 5.0, 8.0, 0x51);
        t.tick(0.02, None);
        assert!(t.lights[0].request[0], "the car's own light was not asked");
        assert_eq!(t.lights[0].request[1], second_asked, "cross traffic joins: {cross_traffic_joins}");
    }
}
