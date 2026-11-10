/// A bus with a long wait at its stop switches its engine off after six seconds and on
/// twenty seconds before it leaves; a short stop keeps it running (#1404).
#[test]
fn the_engine_rests_through_a_long_wait() {
    let mut b = super::BusService::new(Vec::new());
    b.phase = super::Phase::Waiting;
    b.arrived_at = 1000.0;
    b.leave_at = 1300.0;
    assert!(b.engine_running(1003.0), "just arrived");
    assert!(!b.engine_running(1010.0), "resting");
    assert!(b.engine_running(1285.0), "about to leave");
    b.leave_at = 1040.0;
    assert!(b.engine_running(1010.0), "a short stop");
    b.leave_at = 1300.0;
    b.phase = super::Phase::Running;
    assert!(b.engine_running(1010.0), "driving");
}

use super::*;

/// Somebody coming from another stop than the one the bus serves does not keep it there
/// (#767); somebody on the way out does, wherever.
#[test]
fn a_hold_counts_at_the_stop_served() {
    let stop = Stop { ri: 0, s: 0.0, bay: 0.0, depart: 0.0, id: 42, side: 0.0 };
    let mut s = BusService::new(vec![stop]);
    s.phase = Phase::Boarding;
    s.boarding = 0.0;
    s.hold(Some(41), 2.5);
    assert_eq!(s.boarding, 0.0);
    s.hold(Some(42), 2.5);
    assert_eq!(s.boarding, 2.5);
    s.boarding = 0.0;
    s.hold(None, 2.5);
    assert_eq!(s.boarding, 2.5);
}

#[test]
fn an_early_bus_waits_for_its_time() {
    // five minutes early: until 20 s before its departure (a train: 2 min)
    assert_eq!(early_wait(400.0, 100.0, false, false), 280.0);
    assert_eq!(early_wait(400.0, 100.0, false, true), 180.0);
    // late, or nearly on time: off at once
    assert_eq!(early_wait(400.0, 390.0, false, false), 0.0);
    assert_eq!(early_wait(400.0, 500.0, false, false), 0.0);
    // a layover: to the departure itself
    assert_eq!(early_wait(400.0, 100.0, true, false), 300.0);
}

#[test]
fn standing_time() {
    let mut s = BusService::new(vec![]);
    assert_eq!(s.standing_for(0.0), 0.0);
    s.phase = Phase::Waiting;
    s.leave_at = 100.0;
    assert!((s.standing_for(40.0) - 62.0).abs() < 1e-3);
    s.phase = Phase::TripDone;
    assert!(s.standing_for(0.0) > 100.0);
}

#[test]
fn only_the_ends_of_the_trip_and_an_early_bus_stop_for_nobody() {
    let stop = |id: i64, depart: f64| Stop::from_tuple((0, 0.0, 0.0, depart, id, 0.0));
    let mut s = BusService::new(vec![stop(1, 100.0), stop(2, 200.0), stop(3, 300.0)]);
    s.last_stop = Some(3);
    // on time at a stop in the middle: only if somebody wants it
    assert!(!s.must_serve(&stop(2, 200.0), 150.0));
    // over two minutes early: it stops and waits
    assert!(s.must_serve(&stop(2, 200.0), 70.0));
    // the trip's terminus, and the last stop it knows of with the route complete
    assert!(s.must_serve(&stop(3, 300.0), 300.0));
    s.stops = vec![stop(2, 200.0)].into();
    assert!(s.must_serve(&stop(2, 200.0), 200.0));
    s.route_open = true;
    assert!(!s.must_serve(&stop(2, 200.0), 200.0));
    // its first stop, where it stands out its layover
    s.layover = true;
    assert!(s.must_serve(&stop(2, 200.0), 200.0));
    s.layover = false;
    // `[profile_otherstopping]` 1/4 (always) and 3 (when early)
    s.always = vec![2];
    assert!(s.must_serve(&stop(2, 200.0), 200.0));
    s.always.clear();
    s.serve_early = vec![2];
    assert!(!s.must_serve(&stop(2, 200.0), 190.0));
    assert!(s.must_serve(&stop(2, 200.0), 170.0));
}

#[test]
fn only_a_stop_the_timetable_times_holds_the_bus() {
    let stop = |id: i64, depart: f64| Stop::from_tuple((0, 0.0, 0.0, depart, id, 0.0));
    // an ordinary stop whose time is only the running time shared out: served and left
    let mut s = BusService::new(vec![stop(2, 200.0), stop(3, 300.0)]);
    s.last_stop = Some(3);
    assert!(!s.waits_here(false, false, 100.0));
    // a stop the map gave a time of its own: the bus waits for it
    s.holds = vec![2];
    assert!(s.waits_here(false, false, 100.0));
    // a stop marked to be served when early (kind 3) also holds the bus when it is early
    s.holds.clear();
    s.serve_early = vec![2];
    assert!(!s.waits_here(false, false, 5.0));
    assert!(s.waits_here(false, false, 100.0));
    s.serve_early.clear();
    // a stop the timetable only says it stops at in any case (kind 1/4) is served, not
    // waited at: stopping and waiting are different settings of the editor
    s.always = vec![2];
    assert!(!s.waits_here(false, false, 100.0));
    s.always.clear();
    // the trip's last stop always waits, and so do a layover and a railway station
    s.last_stop = Some(2);
    assert!(s.waits_here(false, false, 100.0));
    assert!(s.waits_here(true, false, 100.0));
    assert!(s.waits_here(false, true, 100.0));
}

/// The `ai_wait_timed_stops_only` setting off (the default): an early bus waits for its
/// departure at every stop it serves, as Omsi.exe does, timed by the map or not.
#[test]
fn without_the_setting_an_early_bus_waits_at_every_stop_as_in_omsi() {
    let stop = |id: i64, depart: f64| Stop::from_tuple((0, 0.0, 0.0, depart, id, 0.0));
    let mut s = BusService::new(vec![stop(2, 200.0), stop(3, 300.0)]);
    s.last_stop = Some(3);
    assert!(s.holds_for_departure(false, false, false, 100.0));
    s.always = vec![2];
    assert!(s.holds_for_departure(false, false, false, 100.0));
    // and with it only where the timetable times the stop
    assert!(!s.holds_for_departure(true, false, false, 100.0));
    s.holds = vec![2];
    assert!(s.holds_for_departure(true, false, false, 100.0));
    // the wait itself is OMSI's: until 20 s before the departure
    assert_eq!(early_wait(200.0, 100.0, false, false), 80.0);
}

#[test]
fn interpolated_intermediate_stop_cannot_hold_a_bus_for_many_minutes() {
    let stop = |id: i64| Stop::from_tuple((0, 0.0, 0.0, 1200.0, id, 0.0));
    let mut service = BusService::new(vec![stop(2), stop(3)]);
    service.last_stop = Some(3);
    assert!(!service.waits_here(false, false, 1200.0));
    assert_eq!(service.stop_wait(1200.0, 0.0, false, false, false), 20.0);
    assert_eq!(service.stop_wait(1200.0, 0.0, true, false, false), 0.0);
    service.holds.push(2);
    assert!(service.waits_here(false, false, 1200.0));
    assert_eq!(service.stop_wait(1200.0, 0.0, false, false, false), 1180.0);
    service.holds.clear();
    service.stops.pop_front();
    assert!(service.waits_here(false, false, 1200.0), "the terminus still keeps its layover");
    assert_eq!(service.stop_wait(1200.0, 0.0, false, false, false), 1180.0);
}

#[test]
fn station_side_comes_from_the_stop_it_boards_at() {
    let stop = |side: f32| Stop::from_tuple((0, 0.0, 0.0, 0.0, 1, side));
    let mut s = BusService::new(vec![stop(1.0)]);
    // off a stop: nothing to open
    s.phase = Phase::Running;
    assert_eq!(s.at_station_side(), 0.0);
    // boarding: the front stop's side
    s.phase = Phase::Boarding;
    assert_eq!(s.at_station_side(), 1.0);
    // waiting to pull out (doors shut): the side is not asked for any more
    s.phase = Phase::Waiting;
    assert_eq!(s.at_station_side(), 0.0);
    // an empty queue answers 0, not a panic
    s.phase = Phase::Boarding;
    s.stops.clear();
    assert_eq!(s.at_station_side(), 0.0);
}

fn service_context(net: &Network, day_time: f64, dt: f32) -> Ctx<'_> {
    Ctx { net, way: &[], day_time, dt, id: 1, stopped: 0.0, passing: false,
        kerb_swerve: None, wanted: Some(false), debug: false, timed_waits_only: false,
        time: day_time as f32, berth: BerthScene::default() }
}

fn service_network() -> Network {
    Network { lanes: vec![crate::traffic::LaneBuilder::arc(
        glam::DVec3::ZERO, 0.0, 100.0, 0.0, 0.0, LaneKind::Street, 3.0)],
        ..Default::default() }
}

fn service_vehicle() -> VehicleInstance {
    crate::timetable_run::tests::script_test_vehicle("{frame_ai}\n{end}\n", "schedule_active\n", "")
}

#[test]
fn three_stop_service_closes_each_stop_and_completes_only_at_the_end() {
    let net = service_network();
    let mut vehicle = service_vehicle();
    let mut state = AiState::new(0, 0.0, 1);
    let stops: Vec<_> = (1..=3).map(|id| Stop::from_tuple((0, 10.0, 0.0, id as f64 * 100.0, id, 0.0))).collect();
    let mut service = BusService::new(stops);
    for visit in 1..=3 {
        let depart = visit as f64 * 100.0;
        service.arrive(&service_context(&net, depart, 0.0), depart, (0, 10.0));
        assert_eq!(service.phase, Phase::Boarding);
        service.step(&mut state, &mut vehicle, &service_context(&net, depart + 8.0, 8.0), &mut Berths::default());
        assert_eq!(service.phase, Phase::Closing);
        service.step(&mut state, &mut vehicle, &service_context(&net, depart + 9.5, CLOSE_MIN), &mut Berths::default());
        assert_eq!(service.stops.len(), 3 - visit as usize);
        assert_eq!(service.trip_done(), visit == 3);
        if visit != 3 {
            assert_eq!(service.phase, Phase::Running);
        }
    }
    assert_eq!(state.signal, 0);
    // A next trip must not inherit a previous passenger hold or closing phase.
    service.boarding = 999.0;
    service.restart(vec![Stop::from_tuple((0, 10.0, 0.0, 400.0, 1, 1.0))], true);
    assert_eq!(service.phase, Phase::Running);
    assert_eq!(service.boarding, 0.0);
    assert_eq!(service.phase_t, 0.0);
    assert!(service.layover && !service.boarded);
}

/// A bus that served its stop from behind the one at the berth pulls up onto the berth
/// once that bus has gone and nobody is at its doors (#2591): it shuts its doors first
/// (`Phase::Closing`, the script's answer), moves with them shut and opens them again at
/// once at the berth. While the berth is held it stays where it is; 5 m short or less it
/// does not move at all.
#[test]
fn a_bus_served_from_behind_pulls_up_when_the_berth_comes_free() {
    let net = service_network();
    let mut vehicle = service_vehicle();
    let behind = || {
        let mut s = BusService::new(vec![Stop::from_tuple((0, 30.0, 0.0, 400.0, 7, 0.0))]);
        s.arrive(&service_context(&net, 100.0, 0.0), 400.0, (0, 10.0));
        s.behind = true;
        s.boarding = 0.0;
        s
    };
    let mut ctx = service_context(&net, 200.0, 0.1);
    ctx.berth = BerthScene { applies: true, free: true, there: false, pull_out_blocked: false };
    let key = BerthKey { stop: 7, lane: 0 };
    // the berth still the other bus's: it stays, doors open
    let mut state = AiState::new(0, 0.0, 1);
    let mut berths = Berths::default();
    berths.ask(key, 2, true, 0.0);
    assert!(berths.grant(key, 2, true));
    let mut service = behind();
    assert_eq!(service.step(&mut state, &mut vehicle, &ctx, &mut berths), Some(state.front));
    assert_eq!(service.phase, Phase::Boarding);
    // gone: the doors close first, it does not move yet
    berths.release(key, 2);
    let mut service = behind();
    assert_eq!(service.step(&mut state, &mut vehicle, &ctx, &mut berths), Some(state.front));
    assert_eq!(service.phase, Phase::Closing);
    assert_eq!(state.signal, 0, "no blinker for pulling up");
    // shut: it moves up
    let mut later = service_context(&net, 201.0, CLOSE_MIN);
    later.berth = ctx.berth;
    let at = service.step(&mut state, &mut vehicle, &later, &mut berths);
    assert_eq!(service.phase, Phase::Waiting);
    assert!(at.is_some_and(|d| d > state.front + 20.0), "{at:?}");
    assert!(!service.at_station());
    // at the berth: the doors open again at once, until its departure
    state.s = 30.0;
    later.day_time = 220.0;
    service.step(&mut state, &mut vehicle, &later, &mut berths);
    assert_eq!(service.phase, Phase::Boarding);
    assert!(service.at_station() && service.boarding > 150.0, "{}", service.boarding);
    // somebody still at its doors: it waits for them first
    let mut state = AiState::new(0, 0.0, 1);
    let mut berths = Berths::default();
    let mut service = behind();
    service.boarding = 5.0;
    service.step(&mut state, &mut vehicle, &ctx, &mut berths);
    assert_eq!(service.phase, Phase::Boarding);
    // 3 m short: it stays, the berth its own
    let mut state = AiState::new(0, 27.0, 1);
    let mut service = behind();
    service.step(&mut state, &mut vehicle, &ctx, &mut berths);
    assert_eq!(service.phase, Phase::Boarding);
    assert!(!service.behind && service.held_berth() == Some(key));
}

/// A bus that served its stop from behind keeps its place in the queue for the berth:
/// a bus that came after it does not get the berth before it.
#[test]
fn a_bus_served_from_behind_keeps_its_place_in_the_queue() {
    let net = service_network();
    let mut vehicle = service_vehicle();
    let mut state = AiState::new(0, 0.0, 1);
    let mut service = BusService::new(vec![Stop::from_tuple((0, 30.0, 0.0, 400.0, 7, 0.0))]);
    service.arrive(&service_context(&net, 100.0, 0.0), 400.0, (0, 10.0));
    service.behind = true;
    service.boarding = 5.0;
    let mut ctx = service_context(&net, 200.0, 0.1);
    ctx.berth = BerthScene { applies: true, free: false, there: false, pull_out_blocked: false };
    let key = BerthKey { stop: 7, lane: 0 };
    let mut berths = Berths::default();
    berths.ask(key, 2, true, 0.0);
    assert!(berths.grant(key, 2, true));
    service.step(&mut state, &mut vehicle, &ctx, &mut berths);
    berths.begin_step(0, |_| true);
    service.step(&mut state, &mut vehicle, &ctx, &mut berths);
    berths.ask(key, 4, false, 10.0);
    berths.release(key, 2);
    assert!(!berths.grant(key, 4, true), "the third bus went first");
    assert!(berths.grant(key, 1, true));
}

/// A bus moving up into a berth something else came to stand in gives the move up at
/// its departure time and leaves from where it is, doors shut (#2591).
#[test]
fn a_bus_moving_up_gives_up_at_its_departure() {
    let net = service_network();
    let mut vehicle = service_vehicle();
    let mut state = AiState::new(0, 0.0, 1);
    let mut service = BusService::new(vec![Stop::from_tuple((0, 30.0, 0.0, 400.0, 7, 0.0))]);
    service.arrive(&service_context(&net, 100.0, 0.0), 400.0, (0, 10.0));
    service.behind = true;
    service.reboard = true;
    service.boarded = false;
    service.phase = Phase::Waiting;
    let mut ctx = service_context(&net, 300.0, 0.1);
    ctx.berth = BerthScene { applies: true, free: true, there: false, pull_out_blocked: false };
    let mut berths = Berths::default();
    assert!(service.step(&mut state, &mut vehicle, &ctx, &mut berths).is_some_and(|d| d > state.front + 20.0));
    assert_eq!(service.phase, Phase::Waiting);
    ctx.day_time = 400.0;
    assert_eq!(service.step(&mut state, &mut vehicle, &ctx, &mut berths), Some(state.front));
    assert_eq!(service.phase, Phase::Closing);
    assert!(!service.behind && !service.at_station());
}

#[test]
fn early_late_and_layover_boundaries_keep_the_omsi_departure_rule() {
    assert_eq!(early_wait(400.0, 379.0, false, false), 1.0);
    assert_eq!(early_wait(400.0, 380.0, false, false), 0.0);
    assert_eq!(early_wait(400.0, 381.0, false, false), 0.0);
    assert_eq!(early_wait(400.0, 401.0, false, false), 0.0);
    assert_eq!(early_wait(400.0, 380.0, true, false), 20.0);
    let net = service_network();
    let mut vehicle = service_vehicle();
    let mut state = AiState::new(0, 0.0, 1);
    // a tour's bus come to the first stop of its next trip: the doors open at once and stay
    // open until it is time to go (#2433)
    let mut service = BusService::new(vec![Stop::from_tuple((0, 10.0, 0.0, 400.0, 1, 0.0))]);
    service.layover = true;
    service.arrive(&service_context(&net, 100.0, 0.0), 400.0, (0, 10.0));
    assert_eq!(service.phase, Phase::Boarding);
    // one put out at its stand: doors shut, open for the last minute
    let mut service = BusService::new(vec![Stop::from_tuple((0, 10.0, 0.0, 400.0, 1, 0.0))]);
    service.layover = true;
    service.put_out = true;
    service.arrive(&service_context(&net, 100.0, 0.0), 400.0, (0, 10.0));
    assert_eq!(service.phase, Phase::Waiting);
    assert!(!service.at_station());
    service.step(&mut state, &mut vehicle, &service_context(&net, 354.0, 254.0), &mut Berths::default());
    assert_eq!(service.phase, Phase::Waiting);
    service.step(&mut state, &mut vehicle, &service_context(&net, 355.0, 1.0), &mut Berths::default());
    assert_eq!(service.phase, Phase::Boarding);
    service.step(&mut state, &mut vehicle, &service_context(&net, 396.0, 41.0), &mut Berths::default());
    assert_eq!(service.phase, Phase::Boarding);
    service.step(&mut state, &mut vehicle, &service_context(&net, 400.0, 4.0), &mut Berths::default());
    assert_eq!(service.phase, Phase::Closing);
}

/// Somebody at a door the bus never opens holds it a minute past its departure at
/// most, then it leaves (#1801: buses stood at their stops for good).
#[test]
fn people_at_the_doors_hold_a_bus_a_minute_past_its_time_at_most() {
    let net = service_network();
    let mut vehicle = service_vehicle();
    let mut state = AiState::new(0, 0.0, 1);
    let mut service = BusService::new(vec![Stop::from_tuple((0, 10.0, 0.0, 100.0, 1, 0.0))]);
    service.arrive(&service_context(&net, 100.0, 0.0), 100.0, (0, 10.0));
    let mut t = 100.0;
    while t < 400.0 && service.phase == Phase::Boarding {
        service.hold(Some(1), 2.5);
        t += 0.5;
        service.step(&mut state, &mut vehicle, &service_context(&net, t, 0.5), &mut Berths::default());
    }
    assert_ne!(service.phase, Phase::Boarding, "still boarding at {t}");
    assert!(t < 100.0 + 90.0, "held until {t}");
}

#[test]
fn streamed_route_tail_is_not_trip_completion_and_platform_side_survives_restart() {
    let net = service_network();
    let mut vehicle = service_vehicle();
    let mut state = AiState::new(0, 0.0, 1);
    for side in [0.0, 1.0, 2.0] {
        let mut service = BusService::new(vec![Stop::from_tuple((0, 10.0, 0.0, 100.0, 42, side))]);
        service.route_open = true;
        service.phase = Phase::Boarding;
        assert_eq!(service.at_station_side(), side);
        service.boarding = 1.0;
        service.hold(Some(999), 10.0);
        assert_eq!(service.boarding, 1.0);
        service.hold(Some(42), 10.0);
        assert_eq!(service.boarding, 10.0);
        service.depart(&mut state, &mut vehicle, &service_context(&net, 110.0, 0.0));
        assert_eq!(service.phase, Phase::Running);
        assert!(!service.trip_done());
        // Geometry returns later: preserve the new visit's side and departure.
        service.restart(vec![Stop::from_tuple((0, 10.0, 0.0, 200.0, 42, side))], false);
        service.arrive(&service_context(&net, 200.0, 0.0), 200.0, (0, 10.0));
        assert_eq!(service.at_station_side(), side);
        assert_eq!(service.stops.front().unwrap().depart, 200.0);
    }
}
/// The doors closing at a stop: how long until the bus pulls out (None: still there
/// after a minute), and the fault it notes.
fn close_doors(vehicle: &mut VehicleInstance, at_station: f32, door: Option<f32>) -> (Option<f32>, Option<ServiceFault>) {
    let net = service_network();
    let mut state = AiState::new(0, 0.0, 1);
    let stop = |id: i64| Stop::from_tuple((0, 10.0 * id as f32, 0.0, 100.0, id, 0.0));
    let mut service = BusService::new(vec![stop(1), stop(2)]);
    service.set_phase(Phase::Closing);
    vehicle.set_var("AI_Scheduled_AtStation", at_station);
    if let Some(d) = door {
        vehicle.set_var("door_0", d);
    }
    let mut t = 0.0;
    while t < 60.0 {
        t += 0.1;
        service.step(&mut state, vehicle, &service_context(&net, 100.0 + t as f64, 0.1), &mut Berths::default());
        if service.phase != Phase::Closing {
            return (Some(t), service.fault);
        }
    }
    (None, service.fault)
}

#[test]
fn the_bus_pulls_out_when_its_script_says_the_doors_are_shut() {
    let answering = "{frame_ai}\n0 (S.L.AI_Scheduled_AtStation)\n{end}\n";
    let vars = "AI_Scheduled_AtStation\ndoor_0\n";
    // the script has answered: after the short close
    let mut v = crate::timetable_run::tests::script_test_vehicle(answering, vars, "");
    let (left, fault) = close_doors(&mut v, 0.0, Some(0.0));
    assert!(left.is_some_and(|t| (CLOSE_MIN..CLOSE_MIN + 0.3).contains(&t)), "{left:?}");
    assert_eq!(fault, None);
    // it has not, and its doors stand open: the bus waits for them, but not for good
    let mut v = crate::timetable_run::tests::script_test_vehicle(answering, vars, "");
    let (left, fault) = close_doors(&mut v, -1.0, Some(1.0));
    assert!(left.is_some_and(|t| (DOORS_GIVE_UP..DOORS_GIVE_UP + 0.3).contains(&t)), "{left:?}");
    assert_eq!(fault, Some(ServiceFault::DoorsOpenUnanswered));
    // it has not, and nobody knows about the doors: off after the longest wait
    let mut v = crate::timetable_run::tests::script_test_vehicle(answering, "AI_Scheduled_AtStation\n", "");
    let (left, fault) = close_doors(&mut v, -1.0, None);
    assert!(left.is_some_and(|t| (CLOSE_MAX..CLOSE_MAX + 0.3).contains(&t)), "{left:?}");
    assert_eq!(fault, Some(ServiceFault::DoorsUnanswered));
    // a script that never answers at all has nothing to wait for: the short close
    let mut v = crate::timetable_run::tests::script_test_vehicle("{frame_ai}\n{end}\n", "AI_Scheduled_AtStation\n", "");
    let (left, fault) = close_doors(&mut v, -1.0, None);
    assert!(left.is_some_and(|t| (CLOSE_MIN..CLOSE_MIN + 0.3).contains(&t)), "{left:?}");
    assert_eq!(fault, None);
}

/// A trip whose track runs on past its last stop ends at the end of the track: the bus
/// drives on from the stop instead of ending its trip there.
#[test]
fn the_trip_ends_where_the_track_ends() {
    let net = service_network();
    let mut vehicle = service_vehicle();
    for (route_on, done) in [(false, true), (true, false)] {
        let mut state = AiState::new(0, 10.0, 1);
        if route_on {
            state.route = vec![0];
        }
        let mut service = BusService::new(vec![Stop::from_tuple((0, 10.0, 0.0, 100.0, 1, 0.0))]);
        service.set_phase(Phase::Closing);
        service.phase_t = CLOSE_MIN;
        service.step(&mut state, &mut vehicle, &service_context(&net, 100.0, 0.1), &mut Berths::default());
        assert_eq!(service.trip_done(), done, "the track runs on: {route_on}");
        assert!(service.stops.is_empty());
    }
}
/// A route that moves over to the lane beside at its stop (a stop lane of a junction
/// object): the bus on the lane beside is as far past the stop as it is along, not a
/// whole lane further - it keeps its berth until its rear has left it.
#[test]
fn the_way_past_a_stop_counts_a_lane_beside_as_beside() {
    let key = |path: u16| Some(crate::traffic::LaneKey { tile: (0, 0), id: 9, path });
    let mut a = crate::traffic::LaneBuilder::arc(glam::DVec3::ZERO, 0.0, 100.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut b = crate::traffic::LaneBuilder::arc(glam::DVec3::new(3.5, 0.0, 0.0), 0.0, 100.0, 0.0, 0.0, LaneKind::Street, 3.0);
    a.key = key(0);
    b.key = key(1);
    let net = Network { lanes: vec![a, b], ..Default::default() };
    assert!(net.parallel(0, 1));
    let mut st = AiState::new(1, 21.5, 1);
    st.route = vec![0, 1];
    st.route_index = 1;
    let past = past_by(&st, &net, 0, 21.4);
    assert!((past - (0.1 - st.rear)).abs() < 1e-3, "{past}");
    st.s = 21.4 + st.rear + 2.0;
    assert!(past_by(&st, &net, 0, 21.4) > super::super::berths::CLEAR_PAST);
}

use crate::VehicleInstance;

#[test]
fn scheduled_ai_exposes_timetable_during_init_and_frame_ai() {
    let template = crate::timetable_run::tests::script_test_vehicle(
        "{init}\n(L.L.schedule_active) (S.L.init_active)\n(M.V.GetTTBusstopCount) (S.L.init_count)\n(M.V.GetTTLineString) (S.$.init_line)\n{end}\n{frame_ai}\n(L.L.schedule_active) (S.L.frame_active)\n(M.V.GetTTBusstopIndex) (S.L.frame_index)\n{end}\n",
        "schedule_active\ninit_active\ninit_count\nframe_active\nframe_index\n",
        "init_line\n",
    );
    let stop = Stop::from_tuple((0, 10.0, 0.0, 120.0, 7, 0.0));
    let timetable = AiTimetable {
        line: "042".into(),
        terminus: "terminus".into(),
        stops: vec![
            (7, "First".into(), 100.0, 120.0),
            (8, "Last".into(), 200.0, 220.0),
        ],
    };
    let mut host = crate::VehicleHost::new(Default::default());
    timetable.install(&mut host, Some(&stop));
    let mut vehicle = VehicleInstance::new(template.ty.clone(), host);
    assert_eq!(vehicle.var("init_active"), Some(1.0));
    assert_eq!(vehicle.var("init_count"), Some(2.0));
    assert_eq!(vehicle.str_var("init_line"), "042");
    vehicle.set_var("schedule_active", 0.0);
    vehicle.update_ai(0.01, &Default::default());
    assert_eq!(vehicle.var("frame_active"), Some(1.0));
    assert_eq!(vehicle.var("frame_index"), Some(0.0));
    // Ordinary traffic has no assigned timetable; it must remain inactive.
    let mut ordinary = VehicleInstance::new(
        template.ty.clone(),
        crate::VehicleHost::new(Default::default()),
    );
    ordinary.update_ai(0.01, &Default::default());
    assert_eq!(ordinary.var("init_active"), Some(0.0));
    assert_eq!(ordinary.var("frame_active"), Some(0.0));
}

#[test]
fn timetable_tracks_repeated_stops_streaming_and_trip_changes() {
    let stop = |id, depart| Stop::from_tuple((0, 10.0, 0.0, depart, id, 0.0));
    let first = stop(7, 120.0);
    let second = stop(8, 220.0);
    let repeated = stop(7, 320.0);
    let timetable = AiTimetable {
        line: "42".into(),
        terminus: "terminal".into(),
        stops: vec![
            (7, "A".into(), 100.0, 120.0),
            (8, "B".into(), 200.0, 220.0),
            (7, "A".into(), 300.0, 320.0),
        ],
    };
    let mut v =
        crate::timetable_run::tests::script_test_vehicle("{frame_ai}\n{end}\n", "schedule_active\n", "");
    v.host.hof = Some(std::sync::Arc::new(omsi_vehicle::Hof {
        termini: vec![omsi_vehicle::hof::Terminus {
            texture_id: "terminal".into(),
            ..Default::default()
        }],
        ..Default::default()
    }));
    timetable.install(&mut v.host, Some(&first));
    assert_eq!(v.host.tt_terminus_index, 0);
    let mut service = BusService::new(vec![first, second, repeated]);
    service.phase = Phase::Boarding;
    service.feed_timetable(&mut v, 110.0);
    assert_eq!(v.host.tt_delay, -10.0);
    service.stops.pop_front();
    service.phase = Phase::Running;
    service.delay = 5.0;
    service.feed_timetable(&mut v, 210.0);
    assert_eq!(v.host.tt_busstop_index, 1);
    assert_eq!(v.host.tt_delay, 10.0);
    service.stops.pop_front();
    service.feed_timetable(&mut v, 310.0);
    assert_eq!(v.host.tt_busstop_index, 2);
    // Exhausting the loaded geometry does not exhaust the script's timetable.
    service.stops.clear();
    service.route_open = true;
    service.feed_timetable(&mut v, 315.0);
    assert_eq!(v.host.tt_stops.len(), 3);
    assert_eq!(v.host.tt_busstop_index, 2);
    assert_eq!(v.host.schedule_active, 1.0);
    let next = AiTimetable {
        line: "43".into(),
        terminus: "missing".into(),
        stops: vec![(9, "C".into(), 400.0, 420.0)],
    };
    next.install(&mut v.host, Some(&stop(9, 420.0)));
    assert_eq!(v.host.tt_line, "43");
    assert_eq!(v.host.tt_stops.len(), 1);
    assert_eq!(v.host.tt_busstop_index, 0);
    assert_eq!(v.host.tt_terminus_index, -1);
    assert_eq!(v.host.tt_delay, 0.0);
}

#[test]
fn a_bus_stands_at_its_trips_last_stop_until_its_time_however_long() {
    // in at 21:10, its time at the last stop 05:40 the next morning: it stands the night
    let stop = |id: i64, depart: f64| Stop::from_tuple((0, 0.0, 0.0, depart, id, 0.0));
    let (now, depart) = (76_200.0, 106_800.0);
    let mut service = BusService::new(vec![stop(5, 75_000.0), stop(6, depart)]);
    service.last_stop = Some(6);
    // at an earlier stop the timetable holds it at, the early wait stays capped
    service.holds = vec![5];
    assert_eq!(service.stop_wait(depart, now, false, false, false), LAYOVER_WAIT);
    service.stops.pop_front();
    assert_eq!(service.stop_wait(depart, now, false, false, false), depart - EARLY_LEAVE - now);
}
