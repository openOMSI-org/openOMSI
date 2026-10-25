//! The traffic network's and the AI cars' tests (`super`: `crate::traffic`).

#[test]
fn platform_side_selects_the_correct_visit_on_a_two_way_route() {
    let out = LaneBuilder::polyline(
        vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(100.0, 0.0, 0.0)],
        LaneKind::Street,
        3.0,
    );
    let back = LaneBuilder::polyline(
        vec![DVec3::new(100.0, 6.0, 0.0), DVec3::new(0.0, 6.0, 0.0)],
        LaneKind::Street,
        3.0,
    );
    let mut net = Network {
        lanes: vec![out, back],
        ..Default::default()
    };
    // North of both lanes: right of the westbound lane, left of the eastbound.
    let stop = DVec3::new(50.0, 8.0, 0.0);
    let at = |net: &Network, side| {
        net.project_stop_on_route_side(&[0, 1], stop, Some(25.0), 0, side)
            .unwrap()
            .0
    };
    assert_eq!(at(&net, 0), 1);
    assert_eq!(at(&net, 1), 0);
    assert_eq!(at(&net, 2), 1);
    net.left_hand = true;
    assert_eq!(at(&net, 0), 0);
    assert_eq!(at(&net, 1), 1);
    assert_eq!(at(&net, 2), 1);
    assert_eq!(
        net.project_stop_on_route_side(&[0, 1], stop, Some(25.0), 2, 1)
            .unwrap()
            .0,
        1
    );
    // A central platform is left of both directions: choose the nearest lane.
    let central = DVec3::new(50.0, 2.0, 0.0);
    net.left_hand = false;
    for side in [1, 2] {
        assert_eq!(
            net.project_stop_on_route_side(&[0, 1], central, Some(25.0), 0, side)
                .unwrap()
                .0,
            0
        );
    }
}

#[test]
fn platform_side_uses_the_local_tangent_on_a_curve() {
    let lane = LaneBuilder::arc(DVec3::ZERO, 0.0, 30.0, 40.0, 0.0, LaneKind::Street, 3.0);
    let (point, heading) = lane.at(15.0);
    let h = (heading as f64).to_radians();
    let right = DVec3::new(h.cos(), -h.sin(), 0.0);
    let net = Network {
        lanes: vec![lane],
        ..Default::default()
    };
    let (_, s, lat) = net
        .project_stop_on_route_side(&[0], point - right * 4.0, Some(25.0), 0, 1)
        .unwrap();
    assert!((s - 15.0).abs() < 1.0);
    assert!((lat + 4.0).abs() < 0.2);
}

#[test]
fn a_stop_is_matched_to_the_lane_it_stands_beside() {
    // out along y = 0 (east), back along y = 6 (west); the stop stands north of the
    // way back: on its right, across the road from the way out
    let out = LaneBuilder::polyline(vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(100.0, 0.0, 0.0)], LaneKind::Street, 3.0);
    let back = LaneBuilder::polyline(vec![DVec3::new(100.0, 6.0, 0.0), DVec3::new(0.0, 6.0, 0.0)], LaneKind::Street, 3.0);
    let net = Network { lanes: vec![out, back], ..Default::default() };
    let stop = DVec3::new(50.0, 9.0, 0.0);
    // nearer to the way back anyway: matched there
    assert_eq!(net.project_stop_on_route(&[0, 1], stop, Some(25.0), 0).unwrap().0, 1);
    // a stop on the right of the way out, nearer the middle of the road
    let stop2 = DVec3::new(50.0, -2.0, 0.0);
    assert_eq!(net.project_stop_on_route(&[0, 1], stop2, Some(25.0), 0).unwrap().0, 0);
    // the route out, back and out again: a stop on the way out, once the trip is past
    // its first leg, is the one on the second way out
    assert_eq!(net.project_stop_on_route(&[0, 1, 0], stop2, Some(25.0), 1).unwrap().0, 2);
}

use super::*;

#[test]
fn arc_height_change_and_object_gradients_have_distinct_units() {
    let start = DVec3::new(3.0, 4.0, 5.0);
    let linear = LaneBuilder::arc(start, 20.0, 20.0, 30.0, 0.2, LaneKind::Street, 3.0);
    assert!((linear.points[5].z - 5.1).abs() < 1e-12);
    assert!((linear.end().z - 5.2).abs() < 1e-12);
    let graded = LaneBuilder::arc_with_gradients(start, 20.0, 20.0, 30.0, 0.06, -0.04, LaneKind::Street, 3.0);
    assert!((graded.points[5].z - 5.35).abs() < 1e-12);
    assert!((graded.end().z - 5.2).abs() < 1e-12);
    assert_eq!(linear.headings, graded.headings);
    assert_eq!(linear.curvature, graded.curvature);
    for (a, b) in linear.points.iter().zip(&graded.points) {
        assert_eq!(a.truncate(), b.truncate());
    }
}

/// A straight lane of 60 m running north, then a right-hand bend of radius 14 m over
/// 60°, then straight on again.
fn junction() -> Network {
    let a = LaneBuilder::arc(DVec3::ZERO, 0.0, 60.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let bend = LaneBuilder::arc(a.end(), 0.0, 14.0 * 60f64.to_radians(), 14.0, 0.0, LaneKind::Street, 3.0);
    let c = LaneBuilder::arc(bend.end(), 60.0, 80.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut net = Network { lanes: vec![a, bend, c], ..Default::default() };
    net.link(1.5);
    net
}

#[test]
fn path_rules_open_lanes_by_vehicle_type() {
    let mut l = LaneBuilder::polyline(vec![DVec3::ZERO, DVec3::new(0.0, 50.0, 0.0)], LaneKind::Street, 3.0);
    // no rules: cars and taxis, no AI buses or trucks (Omsi.exe 0x71d714)
    assert!(l.allows(0) && l.allows(1) && !l.allows(2) && !l.allows(3) && l.allows(-1));
    l.rule_trucks = true;
    assert!(l.allows(0) && l.allows(3) && !l.allows(2));
    l.no_cars = true;
    assert!(!l.allows(0) && l.allows(1) && l.allows(3));
    l.rule_trucks = false;
    assert!(!l.allows(0) && !l.allows(1));
    l.rule_bus = true;
    assert!(!l.allows(0) && l.allows(1) && l.allows(2) && !l.allows(3));
}

#[test]
fn no_legal_exit_does_not_fall_back_to_a_forbidden_lane() {
    let mut net = junction();
    let mut car = AiState::new(0, 0.0, 7);
    net.lanes[1].no_cars = true;
    assert_eq!(car.choose_after(&net, 0), None);
    net.lanes[1].no_cars = false;
    // (density 0 is no closure: with no other way on, the car drives on there, #2466)
    net.lanes[1].density = 0.0;
    assert_eq!(car.choose_after(&net, 0), Some(1));
    net.lanes[1].density = 1.0;
    car.veh_type = 3;
    assert_eq!(car.choose_after(&net, 0), None);
    net.lanes[1].rule_trucks = true;
    assert_eq!(car.choose_after(&net, 0), Some(1));
    // Explicit timetable paths retain their original OMSI semantics.
    car.veh_type = -1;
    car.route = vec![0, 1];
    net.lanes[1].density = 0.0;
    car.plan_next(&net);
    assert_eq!(car.planned_next, Some(1));
}

#[test]
fn pool_closed_exits_cannot_fall_back_to_general_traffic() {
    let mut net = junction();
    let mut car = AiState::new(0, 0.0, 7);
    car.traffic_pool = Some((2, vec![1, 1, 1].into()));
    net.lanes[1].group_density = vec![(2, 0.0)];
    assert_eq!(car.choose_after(&net, 0), None);
    net.lanes[1].group_density = vec![(2, 0.5)];
    assert_eq!(car.choose_after(&net, 0), Some(1));
}

#[test]
fn slows_down_for_a_bend() {
    let net = junction();
    assert_eq!(net.lanes[0].next, vec![1]);
    let mut car = AiState::new(0, 0.0, 7);
    car.speed = 13.9;
    car.plan_next(&net);
    assert_eq!(car.upcoming().collect::<Vec<_>>(), vec![1, 2]);
    // the bend allows sqrt(2.8 × 14) ≈ 6.3 m/s; 60 m before it the car may still go fast
    let far = car.curve_speed(&net);
    assert!(far > 13.0, "60 m before the bend: {far}");
    let dt = 1.0 / 30.0;
    let mut entered = None;
    for _ in 0..600 {
        if !car.advance(&net, dt, None, None) {
            break; // the end of the test road
        }
        if car.lane == 1 && entered.is_none() {
            entered = Some(car.speed);
        }
    }
    let v = entered.expect("reached the bend");
    assert!(v < 7.5, "entered the bend at {v} m/s");
}


/// A pull-out started a few metres before the car's lane ends (a road of short spline
/// pieces) carries on across the joint and ends on the lane beside's next piece, without
/// a jump: it used to be finished at the joint, the car moved sideways in one frame.
#[test]
fn a_lane_change_carries_on_across_a_joint() {
    let lane = |x: f64, y0: f64, y1: f64| LaneBuilder::polyline(vec![DVec3::new(x, y0, 0.0), DVec3::new(x, y1, 0.0)], LaneKind::Street, 3.0);
    let mut net = Network { lanes: vec![lane(0.0, 0.0, 20.0), lane(-3.5, 0.0, 20.0), lane(0.0, 20.0, 70.0), lane(-3.5, 20.0, 70.0)], ..Default::default() };
    net.link(1.5);
    net.lanes[0].left = Some(1);
    net.lanes[2].left = Some(3);
    let mut car = AiState::new(0, 16.0, 7);
    car.speed = 3.0;
    car.plan_next(&net);
    assert_eq!(car.planned_next, Some(2));
    car.start_bypass(&net, 1, 1);
    let dt = 1.0 / 30.0;
    let mut at = car.way_point(&net, 0.0);
    for _ in 0..300 {
        assert!(car.drive(&net, dt, None, None));
        let p = car.way_point(&net, 0.0);
        let step = (p - at).length();
        assert!(step < car.speed as f64 * dt as f64 + 0.05, "a jump of {step:.2} m on lane {} s {:.1} change {:?}", car.lane, car.s, car.change);
        at = p;
        if car.change.is_none() {
            break;
        }
    }
    assert!(car.change.is_none(), "the move is over");
    assert_eq!(car.lane, 3, "on the lane beside's next piece");
    assert!((at.x + 3.5).abs() < 0.05, "over in the lane beside: {at:?}");
}

#[test]
fn timetable_route_does_not_jump_across_an_unbridged_gap() {
    let lane = |y0, y1| LaneBuilder::polyline(
        vec![DVec3::new(0.0, y0, 0.0), DVec3::new(0.0, y1, 0.0)],
        LaneKind::Street,
        3.0,
    );
    let mut net = Network { lanes: vec![lane(0.0, 10.0), lane(30.0, 40.0)], ..Default::default() };
    net.link(1.5);
    let mut car = AiState::new(0, 10.0, 7);
    car.route = vec![0, 1];
    car.plan_next(&net);
    assert_eq!(car.planned_next, Some(1));
    assert!(!car.advance(&net, 0.0, None, None));
    assert_eq!(car.lane, 0);

    net.lanes[1] = lane(10.0, 20.0);
    net.link(1.5);
    assert!(car.advance(&net, 0.0, None, None));
    assert_eq!(car.lane, 1);
}

#[test]
fn timetable_change_stops_at_a_diverged_junction_branch() {
    let lane = |points| LaneBuilder::polyline(points, LaneKind::Street, 3.0);
    let mut main = lane(vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 60.0, 0.0)]);
    let mut branch = lane(vec![DVec3::new(3.5, 0.0, 0.0), DVec3::new(3.5, 30.0, 0.0), DVec3::new(18.0, 60.0, 0.0)]);
    main.key = Some(LaneKey { tile: (0, 0), id: 1, path: 0 });
    branch.key = Some(LaneKey { tile: (0, 0), id: 1, path: 1 });
    let net = Network { lanes: vec![main, branch], ..Default::default() };
    assert!(net.parallel(0, 1), "the authored route treats the fork as a lane change");
    assert!(net.route_change_locally_possible(0, 1, 10.0));
    assert!(
        !net.route_change_locally_possible(0, 1, 40.0),
        "do not cut across the divided junction"
    );
    let wait = net.route_change_wait_distance(0, 1, 10.0);
    assert!(wait > 0.0 && wait < 30.0, "wait before the branches part: {wait}");
}

#[test]
fn timetable_change_to_a_longer_branch_stays_beside_the_bus() {
    let lane = |points| LaneBuilder::polyline(points, LaneKind::Street, 3.0);
    let mut main = lane(vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 60.0, 0.0)]);
    let mut branch = lane(vec![DVec3::new(3.5, 0.0, 0.0), DVec3::new(3.5, 25.0, 0.0), DVec3::new(3.5, 30.0, 0.0), DVec3::new(100.0, 30.0, 0.0)]);
    main.key = Some(LaneKey { tile: (0, 0), id: 1, path: 0 });
    branch.key = Some(LaneKey { tile: (0, 0), id: 1, path: 1 });
    let net = Network { lanes: vec![main, branch], ..Default::default() };
    assert!(net.parallel(0, 1));
    assert!(net.route_change_locally_possible(0, 1, 10.0));
    assert!((net.beside_s(0, 1, 10.0) - 10.0).abs() < 0.01);

    let mut car = AiState::new(0, 10.0, 7);
    car.set_route(&net, vec![0, 1], 10.0);
    car.speed = 3.0;
    car.start_route_change(&net, 1, 2);
    let at = car.change.expect("lane change started");
    assert!((at.s_to - 10.0).abs() < 0.01);
    let mut prev = car.way_point(&net, 0.0);
    for _ in 0..300 {
        assert!(car.drive(&net, 1.0 / 30.0, None, None));
        let p = car.way_point(&net, 0.0);
        assert!((p - prev).length() < 0.3, "lane change jumped from {prev:?} to {p:?}");
        prev = p;
        if car.change.is_none() {
            break;
        }
    }
    assert_eq!(car.lane, 1);
    assert!(prev.x > 3.0 && prev.x < 4.0 && prev.y < 30.0, "bus crossed the branch's turn: {prev:?}");
}

#[test]
fn a_turning_lane_is_signalled_in_advance() {
    let mut net = junction();
    net.lanes[1].turn = 2;
    let mut car = AiState::new(0, 0.0, 7);
    car.speed = 10.0;
    car.plan_next(&net);
    let dt = 1.0 / 30.0;
    let mut first_on = None;
    for _ in 0..900 {
        car.advance(&net, dt, None, None);
        car.update_blinker(&net);
        if car.blinker == 2 && first_on.is_none() {
            first_on = Some((car.lane, net.lanes[0].length() - car.s));
        }
        if car.lane == 2 && car.s > 10.0 {
            assert_eq!(car.blinker, 0, "indicator off after the turn");
            break;
        }
    }
    let (lane, before) = first_on.expect("indicator on");
    assert_eq!(lane, 0);
    assert!(before >= 24.0, "indicator on only {before} m before the turn");
}

/// Einm_erzgebirgs.sco's "Main" light: red and yellow 2 s, green 11 s, yellow 3 s,
/// red for the rest of the 38 s cycle.
fn erzgebirgs() -> TrafficLightController {
    TrafficLightController::from_program(vec![(vec![(3, 2.0), (6, 11.0), (9, 3.0), (0, 0.0)], None)], Some(38.0), &[], &[])
}

#[test]
fn a_light_shows_every_aspect_for_its_time_at_any_frame_rate() {
    for dt in [1.0 / 144.0, 1.0 / 30.0, 0.1, 0.25] {
        let mut c = erzgebirgs();
        c.start(0.0);
        // (aspect, seconds) as the lamps show it over two cycles
        let mut seen: Vec<(Aspect, f32)> = Vec::new();
        let mut t = 0.0f32;
        while t < 76.0 {
            let a = TrafficLightController::aspect(c.state(0));
            match seen.last_mut() {
                Some(last) if last.0 == a => last.1 += dt,
                _ => seen.push((a, dt)),
            }
            c.advance(dt);
            t += dt;
        }
        let order: Vec<Aspect> = seen.iter().map(|s| s.0).collect();
        assert_eq!(&order[..5], &[Aspect::RedYellow, Aspect::Green, Aspect::Yellow, Aspect::Red, Aspect::RedYellow], "dt {dt}");
        let tol = dt + 1e-3;
        assert!((seen[0].1 - 2.0).abs() <= tol, "red-yellow {} at dt {dt}", seen[0].1);
        assert!((seen[1].1 - 11.0).abs() <= tol, "green {} at dt {dt}", seen[1].1);
        assert!((seen[2].1 - 3.0).abs() <= tol, "yellow {} at dt {dt}", seen[2].1);
        assert!((seen[3].1 - 22.0).abs() <= tol, "red {} at dt {dt}", seen[3].1);
    }
}

#[test]
fn stock_state_codes_mean_what_the_lamp_scripts_show() {
    assert_eq!(TrafficLightController::lamps(0), (true, false, false));
    assert_eq!(TrafficLightController::lamps(3), (true, true, false));
    assert_eq!(TrafficLightController::lamps(6), (false, false, true));
    assert_eq!(TrafficLightController::lamps(9), (false, true, false));
    assert_eq!(TrafficLightController::lamps(12), (false, false, false));
    assert!(TrafficLightController::allows_go(6) && !TrafficLightController::allows_go(9) && !TrafficLightController::allows_go(3));
    // the clock starts from the time of day: two crossings with one cycle run in step
    let (mut a, mut b) = (erzgebirgs(), erzgebirgs());
    a.start(8.0 * 3600.0 + 10.0);
    b.start(8.0 * 3600.0 + 10.0);
    assert_eq!(a.time, b.time);
    // 28 810 s = 758 cycles and 6 s: green
    assert!((a.time - 6.0).abs() < 1e-6);
    assert_eq!(a.state(0), 6);
}

#[test]
fn a_level_crossing_waits_for_its_train() {
    // bue_falks_ohe.sco: road green until a train asks at light 0 (stop at 1 s), barrier
    // down while it is still there (stop at 16 s)
    let mut c = TrafficLightController::from_program(
        vec![(vec![(0, 15.0), (6, 4.0), (0, 1.0)], None), (vec![(6, 2.0), (9, 12.0), (0, 5.0), (3, 1.0)], None)],
        Some(22.0),
        &[[0.0, 1.0, 1.0], [0.0, 16.0, 0.0]],
        &[],
    );
    c.start(0.0);
    for _ in 0..600 {
        c.advance(0.1);
    }
    assert!((c.time - 1.0).abs() < 1e-6 && c.held, "holding at {}", c.time);
    assert_eq!(c.state(1), 6, "road green while no train comes");
    c.request[0] = true;
    let mut t = 0.0;
    while t < 20.0 {
        c.advance(0.1);
        t += 0.1;
    }
    assert!((c.time - 16.0).abs() < 1e-6 && c.held, "the train is still there: holding at {}", c.time);
    assert_eq!(c.state(0), 6);
    assert_eq!(c.state(1), 0, "road red while the train passes");
    c.request[0] = false;
    c.advance(1.0);
    assert!((c.time - 17.0).abs() < 1e-3, "{}", c.time);
}

#[test]
fn a_bus_phase_is_skipped_when_no_bus_comes() {
    // Kreuz_Heerstr_Pillnitzer_Reimer.sco: the bus light 3 jumps from 51.5 to 61.5 s
    let mut c = TrafficLightController::from_program(vec![(vec![(0, 52.0), (3, 2.0), (6, 4.0), (9, 3.0), (0, 0.0)], Some(10.0))], Some(64.0), &[], &[[0.0, 51.5, 1.0, 61.5]]);
    c.start(50.0);
    c.advance(2.0);
    assert!((c.time - 62.0).abs() < 1e-3, "jumped: {}", c.time);
    c.start(0.0);
    c.time = 50.0;
    c.request[0] = true;
    c.advance(3.0);
    assert!((c.time - 53.0).abs() < 1e-3);
    assert_eq!(c.state(0), 3, "the bus gets its phase");
}

#[test]
fn a_rest_loop_holds_while_nobody_asks() {
    // win-4.sco (#1722): main green 2-12 s, jumping back from 12 to 2 while no bus
    // asks at the bus light; a bus sends it on through yellow.
    let mut c = TrafficLightController::from_program(
        vec![
            (vec![(3, 2.0), (6, 10.0), (9, 3.0), (0, 7.0), (9, 3.0), (0, 1.0)], None),
            (vec![(0, 16.0), (6, 6.0), (0, 1.0)], None),
        ],
        Some(37.0),
        &[],
        &[[1.0, 12.0, 1.0, 2.0], [1.0, 22.0, 1.0, 2.0], [1.0, 22.0, 0.0, 18.0], [1.0, 2.0, 0.0, 12.0], [1.0, 7.0, 0.0, 12.0]],
    );
    c.time = 3.0;
    for _ in 0..1200 {
        c.advance(0.1);
        assert_eq!(c.state(0), 6, "the main road left its green at {:.1} s with nobody asking", c.time);
    }
    c.request[1] = true;
    let mut saw_bus_green = false;
    for _ in 0..300 {
        c.advance(0.1);
        saw_bus_green |= c.state(1) == 6;
    }
    assert!(saw_bus_green, "a bus asking gets its green");
}

#[test]
fn a_backwards_jump_replays_its_phase_once() {
    // win-wit.sco and similar crossings use a small rewind to extend a green. Taking
    // that jump again on every pass locked the whole program in that stretch.
    let mut c = TrafficLightController::from_program(
        vec![(vec![(0, 2.0), (3, 2.0), (6, 4.0), (9, 2.0), (0, 0.0)], None)],
        Some(12.0),
        &[],
        &[[0.0, 8.0, 0.0, 4.0]],
    );
    c.time = 7.9;
    c.request[0] = true;
    for _ in 0..50 {
        c.advance(0.1);
    }
    assert_eq!(c.state(0), 9, "the clock left the replayed green");
}

#[test]
fn coincident_skips_wait_for_any_of_their_lights() {
    let p = |s: [i32; 9]| -> Vec<(i32, f32)> { s.into_iter().zip([3.0, 10.0, 3.0, 3.0, 10.0, 3.0, 3.0, 10.0, 3.0]).collect() };
    for asking in [3, 4] {
        let mut c = TrafficLightController::from_program(
            vec![
                (p([3, 6, 9, 0, 0, 0, 0, 0, 0]), Some(20.0)),
                (p([6, 6, 6, 6, 6, 9, 0, 0, 0]), None),
                (p([6, 6, 6, 6, 6, 9, 0, 0, 0]), None),
                (p([0, 0, 0, 0, 0, 0, 3, 6, 9]), Some(20.0)),
                (p([0, 0, 0, 0, 0, 0, 3, 6, 9]), None),
                (p([0, 0, 0, 3, 6, 9, 0, 0, 0]), Some(14.0)),
            ],
            Some(48.0),
            &[],
            &[[3.0, 32.0, 1.0, 48.0], [4.0, 32.0, 1.0, 48.0], [5.0, 16.0, 1.0, 32.0], [5.0, 0.0, 0.0, 16.0], [5.0, 5.0, 0.0, 16.0], [5.0, 10.0, 0.0, 16.0], [0.0, 13.0, 0.0, 8.0]],
        );
        c.start(0.0);
        c.request[asking] = true;
        let mut green = false;
        for _ in 0..600 {
            c.advance(0.1);
            green |= c.state(asking) == 6;
        }
        assert!(green, "light {asking} asking alone never got its green");
    }
}

#[test]
fn simultaneous_inactive_events_do_not_stall_the_cycle() {
    let mut c = TrafficLightController::from_program(
        vec![(vec![(0, 4.0), (6, 16.0)], None); 2],
        Some(20.0),
        &[[0.0, 4.0, 0.0]],
        &[[1.0, 4.0, 0.0, 12.0]],
    );
    c.start(0.0);
    c.advance(5.0);
    assert_eq!(c.time, 5.0);
    assert_eq!(c.state(0), 6);
    for _ in 0..40 {
        c.advance(1.0);
    }
    assert_eq!(c.time, 5.0, "the clock completes later cycles too");
    assert!(!c.held);
}

#[test]
fn jump_to_zero_at_cycle_boundary_does_not_freeze_the_program() {
    // Grand Paris-Moulon's TCSP17 ends its 84 s cycle with a request-conditioned jump
    // from 84 to 0, and also jumps from 64 to 0 when no light-4 request is present.
    // The jump at 64 lands on the cycle-boundary jump. That second jump makes no progress
    // and must only be processed once, so the bus light can reach its next green.
    let mut lights = vec![Vec::new(); 7];
    lights[2] = vec![(0, 29.0), (6, 12.0), (9, 2.0), (0, 25.0), (6, 12.0), (9, 2.0), (0, 1.0)];
    let mut c = TrafficLightController::from_program(
        lights.into_iter().map(|p| (p, None)).collect(),
        Some(84.0),
        &[],
        &[[4.0, 64.0, 1.0, 0.0], [4.0, 84.0, 1.0, 0.0]],
    );
    c.time = 63.9;
    c.advance(0.2);
    assert!((c.time - 0.1).abs() < 1e-5, "controller clock froze at {}", c.time);

    c.advance(29.0);
    assert_eq!(c.state(2), 6, "the bus light reaches its next green");
}

#[test]
fn simultaneous_inactive_events_do_not_hide_a_later_active_jump() {
    let mut c = TrafficLightController::from_program(
        vec![(vec![(0, 4.0), (6, 16.0)], None); 3],
        Some(20.0),
        &[[0.0, 4.0, 0.0]],
        &[[1.0, 4.0, 0.0, 9.0], [2.0, 4.0, 0.0, 12.0]],
    );
    c.request[2] = true;
    c.start(0.0);
    c.advance(5.0);
    assert_eq!(c.time, 13.0, "take the jump and consume the remaining second");
    assert!(!c.held);
}

#[test]
fn simultaneous_stops_release_and_are_checked_on_the_next_cycle() {
    let mut c = TrafficLightController::from_program(
        vec![(vec![(0, 4.0), (6, 16.0)], None); 3],
        Some(20.0),
        &[[0.0, 4.0, 0.0], [1.0, 4.0, 0.0], [2.0, 4.0, 0.0]],
        &[],
    );
    c.request[2] = true;
    c.start(0.0);
    c.advance(5.0);
    assert_eq!(c.time, 4.0);
    assert!(c.held);
    c.advance(1.0);
    assert_eq!(c.time, 4.0, "an active stop remains held next frame");
    c.request[2] = false;
    c.advance(1.0);
    assert_eq!(c.time, 5.0);
    assert!(!c.held);
    c.advance(16.0);
    assert_eq!(c.time, 1.0);
    c.request[2] = true;
    c.advance(4.0);
    assert_eq!(c.time, 4.0, "the released stop is evaluated after wrapping");
    assert!(c.held);
}

#[test]
fn many_simultaneous_inactive_events_consume_the_frame_time() {
    let mut c = TrafficLightController::new(vec![vec![(0, 4.0), (6, 16.0)]], 20.0);
    c.stops = vec![
        LightStop {
            light: 0,
            time: 4.0,
            if_request: false,
            jump_to: None,
        };
        33
    ];
    c.start(4.0);
    c.advance(0.25);
    assert_eq!(c.time, 4.25, "the inactive group must not exhaust the event budget");
    assert_eq!(c.state(0), 6);
    assert!(!c.held);
}

#[test]
fn a_car_stops_at_the_line_without_braking_hard() {
    let net = junction();
    let mut car = AiState::new(0, 0.0, 3);
    car.speed = 13.9;
    car.accel = 1.5;
    car.decel = 2.2;
    car.plan_next(&net);
    let dt = 1.0 / 60.0;
    let line = 55.0;
    let mut hardest = 0.0f32;
    for _ in 0..1200 {
        let stop = line - car.s;
        car.drive(&net, dt, None, Some(stop));
        hardest = hardest.min(car.acc);
    }
    let front = car.s + car.front;
    assert!(car.speed < 0.01, "stopped: {}", car.speed);
    assert!(front <= line && front > line - 1.5, "front at {front}, line at {line}");
    assert!(hardest > -3.5, "braked at {hardest} m/s²");
}

#[test]
fn a_follower_keeps_its_distance_when_the_leader_brakes() {
    let net = junction();
    let mut car = AiState::new(0, 0.0, 5);
    car.speed = 12.0;
    car.plan_next(&net);
    let (mut lead_s, mut lead_v) = (30.0f32, 12.0f32);
    let dt = 1.0 / 60.0;
    let mut closest = f32::MAX;
    for k in 0..900 {
        // the leader brakes hard after a second and stays standing
        if k > 60 {
            lead_v = (lead_v - 6.0 * dt).max(0.0);
        }
        lead_s += lead_v * dt;
        let gap = lead_s - 2.5 - (car.s + car.front);
        closest = closest.min(gap);
        car.drive(&net, dt, Some(Lead { gap, speed: lead_v, acc: if k > 60 && lead_v > 0.0 { -6.0 } else { 0.0 } }), None);
    }
    assert!(closest > 0.8, "came within {closest} m");
    assert!(car.speed < 0.05);
}

#[test]
fn right_of_way_between_paths() {
    // a crossing: a from the south going north, b from the east going west, c from the
    // north going south and turning left (east)
    let a = LaneBuilder::arc(DVec3::new(0.0, -10.0, 0.0), 0.0, 20.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let b = LaneBuilder::arc(DVec3::new(10.0, 0.0, 0.0), 270.0, 20.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut c = LaneBuilder::arc(DVec3::new(-1.5, 10.0, 0.0), 180.0, 10.0 * std::f64::consts::FRAC_PI_2, -10.0, 0.0, LaneKind::Street, 3.0);
    c.turn = 1;
    let mut net = Network { lanes: vec![a, b, c], ..Default::default() };
    net.link(1.5);
    // equal priority: neither gives way by where it comes from - the first there goes
    // first, as in Omsi.exe (no priority to the right, #2058)
    assert!(!net.must_yield(0, 1));
    assert!(!net.must_yield(1, 0));
    // the left turn waits for the oncoming car
    assert!(net.must_yield(2, 0));
    assert!(!net.must_yield(0, 2));
    // a [rule] priority beats the geometry
    net.lanes[0].priority = 192.0;
    net.lanes[1].priority = 64.0;
    assert!(!net.must_yield(0, 1));
    assert!(net.must_yield(1, 0));
}

#[test]
fn right_of_way_on_the_left() {
    // the same crossing on a left-hand-traffic map: a right turn across the oncoming traffic
    // waits, a left one not; crossing straight on, nobody gives way by the side
    let a = LaneBuilder::arc(DVec3::new(0.0, -10.0, 0.0), 0.0, 20.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let b = LaneBuilder::arc(DVec3::new(10.0, 0.0, 0.0), 270.0, 20.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut c = LaneBuilder::arc(DVec3::new(-1.5, 10.0, 0.0), 180.0, 10.0 * std::f64::consts::FRAC_PI_2, -10.0, 0.0, LaneKind::Street, 3.0);
    c.turn = 2;
    let mut net = Network { lanes: vec![a, b, c], left_hand: true, ..Default::default() };
    net.link(1.5);
    assert!(!net.must_yield(0, 1));
    assert!(!net.must_yield(1, 0));
    assert!(net.must_yield(2, 0));
    net.lanes[2].turn = 1;
    assert!(!net.must_yield(2, 0));
    assert_eq!(net.oncoming_sign(), 1.0);
}

#[test]
fn a_shallow_crossing_is_a_long_meeting_place() {
    // one junction object (source 2, same key): a straight lane and two lanes crossing
    // it, one square, one at 20°
    let key = |path: u16| Some(LaneKey { tile: (0, 0), id: 1, path });
    let mut a = LaneBuilder::arc(DVec3::new(0.0, -20.0, 0.0), 0.0, 40.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut b = LaneBuilder::arc(DVec3::new(20.0, 0.0, 0.0), 270.0, 40.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let h = 20f64.to_radians();
    let mut c = LaneBuilder::arc(DVec3::new(-20.0 * h.sin(), -20.0 * h.cos(), 0.0), 20.0, 40.0, 0.0, 0.0, LaneKind::Street, 3.0);
    for (l, k) in [(&mut a, 0), (&mut b, 1), (&mut c, 2)] {
        l.source = 2;
        l.key = key(k);
    }
    let mut net = Network { lanes: vec![a, b, c], ..Default::default() };
    net.link(1.5);
    let square = net.crossings[0].iter().find(|x| x.other == 1).expect("square crossing");
    let shallow = net.crossings[0].iter().find(|x| x.other == 2).expect("shallow crossing");
    assert!((square.at - 20.0).abs() < 0.5 && (shallow.at - 20.0).abs() < 0.5);
    // square: the bodies touch within a car's width or so of the point
    assert!(square.before <= 3.0 && square.after <= 3.0, "{square:?}");
    // at 20° the centre lines stay within 2.6 m for 2.6 / sin 20° ≈ 7.6 m either side
    assert!(shallow.before >= 7.0 && shallow.after >= 7.0, "{shallow:?}");
}

#[test]
fn a_driver_with_a_choice_keeps_out_of_a_dead_end() {
    // a lane that forks: one way ends after 50 m, the other runs round a long loop
    let a = LaneBuilder::arc(DVec3::ZERO, 0.0, 30.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let dead = LaneBuilder::arc(DVec3::new(0.0, 30.0, 0.0), 10.0, 50.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let on = LaneBuilder::arc(DVec3::new(0.0, 30.0, 0.0), 350.0, 700.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut net = Network { lanes: vec![a, dead, on], ..Default::default() };
    net.link(1.5);
    assert_eq!(net.lanes[0].next.len(), 2);
    assert!(net.reach[1] < DEAD_END && net.reach[0] >= DEAD_END && net.reach[2] >= DEAD_END);
    for seed in 1..40 {
        let mut car = AiState::new(0, 0.0, seed);
        car.plan_next(&net);
        assert_eq!(car.planned_next, Some(2), "seed {seed}");
    }
}

#[test]
fn rejecting_a_short_return_preserves_reach_and_existing_exit_choices() {
    // Filter the erroneous driving choice without removing graph links.
    let a = LaneBuilder::arc(DVec3::ZERO, 0.0, 30.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let finite = LaneBuilder::arc(a.end(), 10.0, 20.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let short = LaneBuilder::arc(finite.end(), 10.0, 0.47, 0.0, 0.0, LaneKind::Street, 3.0);
    let next = LaneBuilder::arc(short.end(), 10.0, 1.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let exit = LaneBuilder::arc(next.end(), 10.0, 280.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let on = LaneBuilder::arc(DVec3::new(0.0, 30.0, 0.0), 350.0, 700.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut net = Network { lanes: vec![a, finite, short, next, exit, on], ..Default::default() };
    net.link(1.5);
    assert_eq!(net.lanes[0].next.len(), 2);
    assert!(net.lanes[3].next.contains(&2));
    assert_eq!(net.reach[1], REACH_MAX);
    assert!(net.reach[5] >= DEAD_END);
    let mut at_joint = AiState::new(3, 0.0, 7);
    assert_eq!(at_joint.choose_after(&net, 3), Some(4));
    let mut selected = [0; 2];
    for seed in 1..128 {
        let mut car = AiState::new(0, 0.0, seed);
        car.plan_next(&net);
        match car.planned_next {
            Some(1) => selected[0] += 1,
            Some(5) => selected[1] += 1,
            other => panic!("unexpected exit {other:?}"),
        }
    }
    assert!(selected.iter().all(|&n| n > 0), "an open exit disappeared: {selected:?}");
}

#[test]
fn the_way_has_no_steps_at_a_lane_joint() {
    // two lanes that meet 1.2 m apart: the way bends over the joint instead of jumping
    let a = LaneBuilder::arc(DVec3::ZERO, 0.0, 30.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let b = LaneBuilder::arc(DVec3::new(1.2, 30.0, 0.0), 0.0, 30.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut net = Network { lanes: vec![a, b], ..Default::default() };
    net.link(1.5);
    let mut car = AiState::new(0, 20.0, 1);
    car.plan_next(&net);
    let mut last = car.way_point(&net, -10.0);
    let mut d = -10.0;
    while d < 25.0 {
        d += 0.25;
        let p = car.way_point(&net, d);
        assert!((p - last).length() < 0.3, "step of {} m at {d}", (p - last).length());
        last = p;
    }
}

/// The southbound half of a road through a junction: a lane B (60 m) into the junction's
/// straight path J1 (10 m), a left turn J2 from the east into the same exit, and the lane
/// A (50 m) after the junction.
fn oncoming_road() -> Network {
    let a = LaneBuilder::arc(DVec3::new(-3.0, 150.0, 0.0), 180.0, 50.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let j1 = LaneBuilder::arc(DVec3::new(-3.0, 160.0, 0.0), 180.0, 10.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let j2 = LaneBuilder::arc(DVec3::new(5.0, 158.0, 0.0), 270.0, 8.0 * std::f64::consts::FRAC_PI_2, -8.0, 0.0, LaneKind::Street, 3.0);
    let b = LaneBuilder::arc(DVec3::new(-3.0, 220.0, 0.0), 180.0, 60.0, 0.0, 0.0, LaneKind::Street, 3.0);
    assert!((j2.end() - DVec3::new(-3.0, 150.0, 0.0)).length() < 0.05, "{:?}", j2.end());
    let mut net = Network { lanes: vec![a, j1, j2, b], ..Default::default() };
    net.link(1.5);
    net
}

#[test]
fn upstream_walks_back_through_the_junction() {
    let net = oncoming_road();
    assert_eq!(net.prev[0].len(), 2, "{:?}", net.prev);
    // 20 m into A, looking 100 m back: A itself, both junction paths and the lane before
    let up = net.upstream(0, 20.0, 100.0, 16);
    let find = |l: usize| up.iter().find(|e| e.0 == l).copied();
    assert_eq!(up[0], (0, 0.0, None));
    let (_, off1, into1) = find(1).expect("J1");
    assert!((off1 + 10.0).abs() < 0.01 && into1 == Some(0), "{up:?}");
    let (_, off2, into2) = find(2).expect("J2");
    assert!((off2 + 8.0 * std::f32::consts::FRAC_PI_2).abs() < 0.05 && into2 == Some(0), "{up:?}");
    let (_, off3, into3) = find(3).expect("B");
    assert!((off3 + 70.0).abs() < 0.01 && into3 == Some(1), "{up:?}");
    // a car 15 m into B is at 15 - 70 = -55 in A's distances: 75 m before the place
    // looking only 25 m back, the lanes that end within reach are there, B is not
    let near = net.upstream(0, 20.0, 25.0, 16);
    assert!(near.iter().any(|e| e.0 == 1) && !near.iter().any(|e| e.0 == 3), "{near:?}");
    // and the list is capped
    assert_eq!(net.upstream(0, 20.0, 100.0, 2).len(), 2);
}

#[test]
fn an_acceleration_cap_holds_a_car_back() {
    let net = junction();
    let mut car = AiState::new(0, 0.0, 7);
    car.accel = 2.5;
    car.plan_next(&net);
    car.accel_cap = Some(1.0);
    for _ in 0..30 {
        car.drive(&net, 1.0 / 30.0, None, None);
    }
    assert!(car.speed > 0.9 && car.speed < 1.0 + 1e-3, "{}", car.speed);
    car.accel_cap = None;
    for _ in 0..30 {
        car.drive(&net, 1.0 / 30.0, None, None);
    }
    assert!(car.speed > 3.0, "{}", car.speed);
}

#[test]
fn arrival_times() {
    assert_eq!(arrival_time(-1.0, 5.0, 1.0, 10.0), 0.0);
    // at a steady 10 m/s
    assert!((arrival_time(100.0, 10.0, 1.0, 10.0) - 10.0).abs() < 1e-4);
    // from a standstill at 2 m/s² without reaching the limit: sqrt(2 d / a)
    assert!((arrival_time(50.0, 0.0, 2.0, 20.0) - 50f32.sqrt()).abs() < 1e-3);
    // 5 s up to 10 m/s over 25 m, then 125 m at 10 m/s
    assert!((arrival_time(150.0, 0.0, 2.0, 10.0) - 17.5).abs() < 1e-3);
    // a car faster than the limit keeps its speed
    assert!((arrival_time(60.0, 15.0, 2.0, 10.0) - 4.0).abs() < 1e-3);
}

#[test]
fn moving_back_in_clears_the_oncoming_lane_part_way() {
    // 3.3 m over, 2.05 m needed to the oncoming lane's middle: 62 % of the offset
    let t = ramp_progress_for(3.3, 2.05);
    assert!((smooth01(t) - 2.05 / 3.3).abs() < 1e-4, "{t}");
    assert!(t > 0.5 && t < 0.7, "{t}");
    assert_eq!(ramp_progress_for(3.3, 0.0), 0.0);
    assert_eq!(ramp_progress_for(2.0, 2.5), 1.0);
    assert!(ramp_progress_for(3.3, 1.5) < t);
}

/// Straight lanes north, one after the other, with these lengths and limits (km/h).
fn straight_road(pieces: &[(f64, f32)]) -> Network {
    let mut lanes = Vec::new();
    let mut y = 0.0;
    for &(len, kmh) in pieces {
        let mut l = LaneBuilder::arc(DVec3::new(0.0, y, 0.0), 0.0, len, 0.0, 0.0, LaneKind::Street, 3.0);
        l.speed_limit_kmh = kmh;
        lanes.push(l);
        y += len;
    }
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    net
}

/// A follower at its time gap behind a leader that brakes as a driver brakes for a light
/// (2.5 m/s²) to a standstill: the ordinary channel is enough all the way, nothing comes
/// near the ordinary maximum and the follower stops at a gap.
#[test]
fn a_leader_braking_comfortably_needs_no_emergency() {
    let net = straight_road(&[(900.0, 50.0)]);
    for seed in [3u64, 0x5151, 0xbeef] {
        let mut car = AiState::new(0, 0.0, seed);
        car.speed = 13.0;
        car.plan_next(&net);
        let mut lead_v = 13.0f32;
        let mut lead_s = car.front + car.min_gap + car.headway * 13.0 + 2.5;
        let dt = 0.02;
        let (mut closest, mut hardest, mut emergencies) = (f32::MAX, 0.0f32, 0);
        for k in 0..2000 {
            let braking = k > 100 && lead_v > 0.0;
            if braking {
                lead_v = (lead_v - 2.5 * dt).max(0.0);
            }
            lead_s += lead_v * dt;
            let gap = lead_s - 2.5 - (car.s + car.front);
            closest = closest.min(gap);
            car.drive(&net, dt, Some(Lead { gap, speed: lead_v, acc: if braking { -2.5 } else { 0.0 } }), None);
            hardest = hardest.min(car.acc);
            emergencies += car.emergency_began as u32;
        }
        assert_eq!(emergencies, 0, "seed {seed}: the collision-prevention channel took over");
        assert!(hardest > -car.ordinary_brake + 1.0, "seed {seed}: braked at {hardest:.2} m/s²");
        assert!(closest > car.min_gap * 0.6, "seed {seed}: came within {closest:.2} m");
        assert!(car.speed < 0.01);
    }
}

/// Something standing turns up 16 m ahead of a car at 50 km/h (it needs 6 m/s²): the
/// collision-prevention channel takes over, once, brakes harder than the ordinary maximum
/// and the car stops short of it.
#[test]
fn an_emergency_stop_is_its_own_channel_and_stays_clear() {
    let net = straight_road(&[(900.0, 50.0)]);
    let mut car = AiState::new(0, 0.0, 9);
    car.speed = 13.9;
    car.cmd = 0.0;
    car.plan_next(&net);
    let wall = car.s + car.front + 16.0;
    let (mut closest, mut hardest, mut began) = (f32::MAX, 0.0f32, 0);
    for _ in 0..600 {
        let gap = wall - (car.s + car.front);
        closest = closest.min(gap);
        car.drive(&net, 0.02, Some(Lead { gap, speed: 0.0, acc: 0.0 }), None);
        hardest = hardest.min(car.acc);
        began += car.emergency_began as u32;
    }
    assert_eq!(began, 1, "one emergency, counted once");
    assert!(hardest < -car.ordinary_brake, "it braked at {hardest:.2} m/s² only");
    assert!(hardest >= -car.emergency_brake - 1e-3);
    assert!(closest > 0.3, "came within {closest:.2} m");
    assert!(car.speed < 0.01 && !car.emergency);
}

/// A car coming to its line, or up to a car standing ahead, stops there and stays: no
/// creeping on by centimetres once it stands, the brake lights on all the while.
#[test]
fn a_stop_is_reached_without_creeping_or_flicker() {
    let net = straight_road(&[(900.0, 50.0)]);
    for (seed, v, behind) in [(3u64, 13.9f32, false), (77, 6.0, false), (0x1234, 2.0, false), (3, 13.9, true), (77, 6.0, true), (0x4321, 1.0, true)] {
        let mut car = AiState::new(0, 0.0, seed);
        car.speed = v;
        car.plan_next(&net);
        let mut line = 70.0f32;
        let mut stood: Option<f32> = None;
        let mut flips = 0;
        let mut last_braking = true;
        for k in 0..3000 {
            // (the car ahead rolls on by 40 cm once everybody stands: the one behind stays)
            if behind && k == 2000 {
                line += 0.4;
            }
            // a line, or the back of a car standing there
            let (lead, stop) = if behind { (Some(Lead { gap: line - car.s - car.front, speed: 0.0, acc: 0.0 }), None) } else { (None, Some(line - car.s)) };
            car.drive(&net, 0.02, lead, stop);
            match stood {
                None if car.speed < 0.01 => stood = Some(car.s),
                Some(_) => {
                    flips += (car.braking != last_braking) as u32;
                    last_braking = car.braking;
                }
                None => {}
            }
        }
        let at = stood.expect("it stopped");
        assert!((car.s - at).abs() < 0.01, "seed {seed}: crept {:.3} m on after stopping", car.s - at);
        assert_eq!(flips, 0, "seed {seed}: the brake lights went on and off");
        let front = car.s + car.front;
        let want = if behind { line - 0.4 - car.min_gap } else { line };
        assert!(front <= want + 0.3 && front > want - 1.5, "seed {seed}: front at {front:.2}, wanted at {want}");
    }
}

/// A 30 zone after a 50 one: the car is down to 30 km/h where it begins, braking gently,
/// and a car over a limit gets down to it within a couple of seconds rather than creeping
/// towards it.
#[test]
fn a_lower_limit_ahead_is_met_in_time_and_without_creeping() {
    let net = straight_road(&[(200.0, 50.0), (400.0, 30.0)]);
    let mut car = AiState::new(0, 20.0, 5);
    car.speed = 50.0 / 3.6;
    car.decel = 2.2;
    car.desire = 1.0;
    car.plan_next(&net);
    let limit = 30.0 / 3.6;
    let mut entered = None;
    let mut hardest = 0.0f32;
    for _ in 0..1500 {
        car.drive(&net, 0.02, None, None);
        hardest = hardest.min(car.acc);
        if car.lane == 1 && entered.is_none() {
            entered = Some(car.speed);
        }
    }
    let v = entered.expect("reached the 30 zone");
    assert!(v < limit + 0.7, "entered at {:.1} km/h", v * 3.6);
    assert!(hardest > -car.decel, "braked at {hardest:.2} m/s²");
    // put on a 30 lane at 40 km/h: within the limit after two seconds, not still above it
    let mut fast = AiState::new(1, 10.0, 6);
    fast.speed = 40.0 / 3.6;
    fast.desire = 1.0;
    fast.plan_next(&net);
    for _ in 0..100 {
        fast.drive(&net, 0.02, None, None);
    }
    assert!(fast.speed < limit + 0.1, "{:.2} km/h after 2 s", fast.speed * 3.6);
}

/// A junction object of two paths: a straight one north (path 0) and one from the east
/// crossing it square (path 1), with `[blockpath]`/`[crossingproblem]` as given.
fn two_paths(crossing_problem: bool, block: bool) -> Network {
    let key = |path: u16| Some(LaneKey { tile: (0, 0), id: 1, path });
    let mut a = LaneBuilder::arc(DVec3::new(0.0, -20.0, 0.0), 0.0, 40.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut b = LaneBuilder::arc(DVec3::new(20.0, 0.0, 0.0), 270.0, 40.0, 0.0, 0.0, LaneKind::Street, 3.0);
    for (l, k) in [(&mut a, 0), (&mut b, 1)] {
        l.source = 2;
        l.key = key(k);
    }
    a.crossing_problem = crossing_problem;
    if block {
        a.blocks = vec![1];
    }
    let mut net = Network { lanes: vec![a, b], ..Default::default() };
    net.link(1.5);
    net
}

#[test]
fn a_crossing_problem_makes_the_whole_of_both_lanes_the_meeting_place() {
    let plain = two_paths(false, false);
    let c = plain.crossings[1][0];
    assert!(c.other_before <= 3.0 && c.other_after <= 3.0, "{c:?}");
    // (either lane flagged: seen from both)
    let flagged = two_paths(true, false);
    for (l, len) in [(0usize, 40.0f32), (1, 40.0)] {
        let c = flagged.crossings[l][0];
        assert!((c.at - c.before).abs() < 0.01 && (c.at + c.after - len).abs() < 0.01, "{c:?}");
        assert!((c.other_at - c.other_before).abs() < 0.01 && (c.other_at + c.other_after - len).abs() < 0.01, "{c:?}");
    }
}

#[test]
fn a_blockpath_has_no_effect_as_in_omsi() {
    // two paths that do not cross: a blockpath between them adds no meeting place
    let key = |path: u16| Some(LaneKey { tile: (0, 0), id: 1, path });
    let mut a = LaneBuilder::arc(DVec3::new(0.0, -20.0, 0.0), 0.0, 40.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut b = LaneBuilder::arc(DVec3::new(10.0, -20.0, 0.0), 0.0, 40.0, 0.0, 0.0, LaneKind::Street, 3.0);
    for (l, k) in [(&mut a, 0), (&mut b, 1)] {
        l.source = 2;
        l.key = key(k);
    }
    a.blocks = vec![1];
    let mut net = Network { lanes: vec![a, b], ..Default::default() };
    net.link(1.5);
    assert!(net.crossings[0].is_empty() && net.crossings[1].is_empty());
    // and a crossing pair keeps its geometric meeting place
    let blocked = two_paths(false, true);
    assert_eq!(blocked.crossings[0], two_paths(false, false).crossings[0]);
}

#[test]
fn the_ring_goes_before_the_entry_where_the_map_sets_no_priorities() {
    // an entry from the south and a ring lane coming from its left
    let a = LaneBuilder::arc(DVec3::new(0.0, -10.0, 0.0), 0.0, 20.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let b = LaneBuilder::arc(DVec3::new(-10.0, 0.0, 0.0), 90.0, 20.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut net = Network { lanes: vec![a, b], ..Default::default() };
    net.link(1.5);
    // without the ring nobody gives way by the side
    assert!(!net.must_yield(0, 1) && !net.must_yield(1, 0));
    net.lanes[1].ring = true;
    assert!(net.must_yield(0, 1) && !net.must_yield(1, 0));
    // the map's priorities still decide where it sets them
    net.lanes[0].priority = 192.0;
    assert!(!net.must_yield(0, 1));
}

#[test]
fn two_turns_sweeping_past_each_other_meet_where_they_pass() {
    // two gentle bends of one junction object from opposite sides, bowing towards each
    // other: 1.5 m apart at their middles, never crossing
    let key = |path: u16| Some(LaneKey { tile: (0, 0), id: 1, path });
    let arc = |x: f64, y: f64, h: f64| LaneBuilder::arc(DVec3::new(x, y, 0.0), h, 25.0 * 0.824, 25.0, 0.0, LaneKind::Street, 3.0);
    let net_of = |dy: f64| {
        let mut a = arc(-10.0, -2.84, 66.4);
        let mut b = arc(10.0, 2.84 + dy, 246.4);
        for (l, k) in [(&mut a, 0), (&mut b, 1)] {
            l.source = 2;
            l.key = key(k);
        }
        let mut net = Network { lanes: vec![a, b], ..Default::default() };
        net.link(1.5);
        net
    };
    let net = net_of(0.0);
    let c = net.crossings[0].iter().find(|c| c.other == 1).expect("a meeting place where they pass");
    assert!(!c.merge && (c.at - net.lanes[0].length() * 0.5).abs() < 2.0, "{c:?}");
    // bends well apart do not meet
    assert!(net_of(5.0).crossings[0].is_empty());
}

/// #2466: of two ways on, the car does not take the one that leads only into lanes closed
/// to it (it came to that lane's end with nowhere to go and vanished in the road); with no
/// other choice it still takes it, and the map's edge (no lane on at all) stays a way.
#[test]
fn a_way_on_that_leads_only_into_closed_lanes_is_not_taken() {
    let a = LaneBuilder::arc(DVec3::ZERO, 0.0, 60.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let trap = LaneBuilder::arc(a.end(), 0.0, 30.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut closed = LaneBuilder::arc(trap.end(), 0.0, 30.0, 0.0, 0.0, LaneKind::Street, 3.0);
    closed.density = 0.0;
    let open = LaneBuilder::arc(a.end(), 0.0, 14.0 * 60f64.to_radians(), 14.0, 0.0, LaneKind::Street, 3.0);
    let on = LaneBuilder::arc(open.end(), 60.0, 30.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut net = Network { lanes: vec![a, trap, closed, open, on], ..Default::default() };
    net.link(1.5);
    assert!(net.lanes[0].next.contains(&1) && net.lanes[0].next.contains(&3));
    for seed in 1..40 {
        let mut car = AiState::new(0, 0.0, seed);
        assert_eq!(car.choose_after(&net, 0), Some(3), "seed {seed} took the lane into the closed one");
    }
    // the open way gone: the trapped one is still taken rather than nothing, and at its end
    // the lane of density 0 (no closure)
    net.lanes[3].density = 0.0;
    let mut car = AiState::new(0, 0.0, 5);
    assert_eq!(car.choose_after(&net, 0), Some(1));
    assert_eq!(car.choose_after(&net, 1), Some(2));
    // closed to cars: nothing
    net.lanes[2].no_cars = true;
    assert_eq!(car.choose_after(&net, 1), None);
    // the edge of the network is no trap
    let mut net = junction();
    net.lanes[2].next.clear();
    let mut car = AiState::new(0, 0.0, 5);
    assert_eq!(car.choose_after(&net, 0), Some(1));
}

/// A two-light program with stops in several kinds of phase: in the changeover (a light in
/// red-and-yellow) a request does not hold the clock; in the green, all red or a flashing
/// rest it does.
#[test]
fn stop_points_hold_only_where_traffic_goes() {
    // light 0: green 0-30 s, yellow 3 s, red; light 1: red to 36 s, red-and-yellow 36-38 s,
    // green 38-50 s, yellow, red
    let program = || {
        vec![
            (vec![(6, 30.0), (9, 3.0), (0, 0.0)], None),
            (vec![(0, 36.0), (3, 2.0), (6, 12.0), (9, 3.0), (0, 0.0)], None),
        ]
    };
    let run = |stop_at: f32, seconds: f32| {
        let mut ctl = TrafficLightController::from_program(program(), Some(60.0), &[[1.0, stop_at, 0.0]], &[]);
        ctl.time = 34.5;
        ctl.request[1] = true;
        let steps = (seconds / 0.05) as usize;
        for _ in 0..steps {
            ctl.advance(0.05);
        }
        ctl
    };
    let in_changeover = run(37.0, 6.0);
    assert!(!in_changeover.held, "held at {:.2} s", in_changeover.time);
    assert!(in_changeover.time > 40.0);
    assert_eq!(in_changeover.state(1), 6, "light 1 went on to green");
    let in_green = run(42.0, 12.0);
    assert!(in_green.held, "a request in the green holds it");
    assert!((in_green.time - 42.0).abs() < 1e-3, "{:.2}", in_green.time);
    // all red (both lights red at 34 s) and a dark rest hold too
    let all_red = run(34.8, 6.0);
    assert!(all_red.held && (all_red.time - 34.8).abs() < 1e-3, "{:.2}", all_red.time);
    let mut dark_rest = TrafficLightController::from_program(vec![(vec![(13, 20.0), (0, 0.0)], None)], Some(40.0), &[[0.0, 5.0, 0.0]], &[]);
    dark_rest.time = 4.0;
    dark_rest.request[0] = true;
    for _ in 0..80 {
        dark_rest.advance(0.05);
    }
    assert!(dark_rest.held && (dark_rest.time - 5.0).abs() < 1e-3, "{:.2}", dark_rest.time);
}

/// A jump to a time beyond the cycle goes on at the start of the next cycle.
#[test]
fn jumps_beyond_the_cycle_restart_it() {
    // a 90 s cycle that flashes yellow (13) for its first 15 s; at 80 s, unasked, it jumps
    // to 97 s. Wrapped round that would be 7 s, still flashing; it must be 0.
    let program = vec![(vec![(13, 15.0), (6, 60.0), (9, 3.0), (0, 0.0)], None)];
    let mut ctl = TrafficLightController::from_program(program, Some(90.0), &[], &[[0.0, 80.0, 1.0, 97.0]]);
    ctl.time = 79.0;
    for _ in 0..12 {
        ctl.advance(0.25);
    }
    assert!((ctl.time - 2.0).abs() < 1e-3, "clock at {:.2} s", ctl.time);
    assert_eq!(ctl.state(0), 13);
    // a jump inside the cycle still lands where it says
    let mut inside = TrafficLightController::from_program(vec![(vec![(6, 60.0), (0, 0.0)], None)], Some(90.0), &[], &[[0.0, 80.0, 1.0, 10.0]]);
    inside.time = 79.5;
    inside.advance(1.0);
    assert!((inside.time - 10.5).abs() < 1e-3, "{:.2}", inside.time);
}

#[test]
fn the_kept_bends_give_the_bend_speed_found_anew() {
    let net = junction();
    let mut car = AiState::new(0, 1.3, 7);
    car.speed = 13.9;
    car.plan_next(&net);
    let dt = 1.0 / 50.0;
    for k in 0..400 {
        let (cached, fresh) = (car.curve_speed_cached(&net), car.curve_speed(&net));
        assert!((cached - fresh).abs() < 0.05 || (cached > 1e30 && fresh > 1e30), "step {k}: {cached} vs {fresh}");
        if !car.advance(&net, dt, None, None) {
            break;
        }
    }
}

#[test]
fn two_quiet_jumps_at_the_cycle_start_let_the_clock_run_on() {
    // A 127 s cycle with two day jumps at 0 s, each taken while its detector light is
    // empty (to 15 s, past the night flashing), and one at 87 s past the cycle's end. At
    // night both detectors are occupied: neither jump fires and the clock runs through the
    // flashing from 0 s, frame after frame, instead of standing at 0.
    let lights = vec![(vec![(0, 127.0)], None), (vec![(0, 127.0)], None), (vec![(13, 15.0), (6, 112.0)], None)];
    let jumps = [[0.0, 0.0, 1.0, 15.0], [1.0, 0.0, 1.0, 15.0], [2.0, 87.0, 0.0, 130.0]];
    let mut night = TrafficLightController::from_program(lights.clone(), Some(127.0), &[], &jumps);
    night.request = vec![true, true, false];
    night.time = 126.0;
    for _ in 0..60 {
        night.advance(0.1);
    }
    assert!((night.time - 5.0).abs() < 1e-3, "the night clock stood at {:.2} s", night.time);
    assert_eq!(night.state(2), 13, "the flashing runs at night");
    // by day the empty detectors take their jump at 0 s straight past the flashing
    let mut day = TrafficLightController::from_program(lights, Some(127.0), &[], &jumps);
    day.time = 126.0;
    day.advance(2.0);
    assert!((day.time - 16.0).abs() < 1e-3, "the day clock went on at {:.2} s", day.time);
}

/// win-4.sco ("Hauptbahnhof 4", #3092): the program as the file has it, lights 0 Main,
/// 1 Bus, 2 Bus2, 3 Green, 4 Ped, and its thirteen jumps.
fn win4() -> TrafficLightController {
    TrafficLightController::from_program(
        vec![
            (vec![(3, 2.0), (6, 10.0), (9, 3.0), (0, 7.0), (9, 3.0), (0, 1.0)], None),
            (vec![(0, 16.0), (6, 6.0), (0, 1.0)], Some(5.0)),
            (vec![(0, 26.0), (6, 10.0)], Some(40.0)),
            (vec![(6, 1.0)], Some(6.0)),
            (vec![(6, 22.0), (0, 1.0)], Some(0.0)),
        ],
        Some(37.0),
        &[],
        &[
            [1.0, 12.0, 1.0, 2.0],
            [2.0, 12.0, 1.0, 2.0],
            [1.0, 22.0, 1.0, 2.0],
            [1.0, 22.0, 0.0, 18.0],
            [2.0, 12.0, 0.0, 22.0],
            [1.0, 2.0, 0.0, 12.0],
            [1.0, 7.0, 0.0, 12.0],
            [2.0, 2.0, 0.0, 22.0],
            [2.0, 7.0, 0.0, 22.0],
            [2.0, 12.0, 0.0, 22.0],
            [2.0, 17.0, 0.0, 22.0],
            [2.0, 31.0, 0.0, 27.0],
            [2.0, 35.0, 0.0, 27.0],
        ],
    )
}

#[test]
fn win4_gives_each_bus_light_its_green_when_it_asks_alone() {
    // (the second bus light's request jumps to 22 s, where the first one's "nobody asks:
    // back to 2 s" waits: landing on it is not a second visit, #3092)
    for asking in [1usize, 2] {
        let mut c = win4();
        c.time = 3.0;
        c.request[asking] = true;
        let mut green_at = None;
        for i in 0..400 {
            c.advance(0.1);
            if green_at.is_none() && c.state(asking) == 6 {
                green_at = Some(i as f32 * 0.1);
            }
        }
        let t = green_at.unwrap_or(f32::INFINITY);
        assert!(t < 40.0, "light {asking} asking alone waited {t} s for its green");
        assert_ne!(c.state(0), 6, "the main road stays green over a waiting bus (light {asking})");
    }
    // with nobody asking the main road rests on its green
    let mut c = win4();
    c.time = 3.0;
    for _ in 0..1800 {
        c.advance(0.1);
        assert_eq!(c.state(0), 6, "the main road left its green with nobody asking");
    }
}
