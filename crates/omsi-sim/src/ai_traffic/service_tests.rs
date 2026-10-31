//! Timetable buses at their stops, whole scenes run headless: who gets the berth, who
//! waits for it and where, what is served from behind and what is missed.

use super::bus_service::{AiTimetable, Stop};
use super::*;
use crate::ai_traffic::setup::RandomTypes;
use crate::traffic::LaneBuilder;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A vehicle type with no model and no scripts, in a folder of its own (12 m as a bus,
/// 4.5 m as a car).
pub(crate) struct Fixture {
    dir: std::path::PathBuf,
    pub(crate) ty: Arc<VehicleType>,
}

impl Fixture {
    pub(crate) fn new() -> Fixture {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("omsi-traffic-service-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
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

/// The stop of every scene: on the road's lane 0, its point 300 m along.
const STOP_S: f32 = 300.0;
const STOP_ID: i64 = 77;

/// One straight road north, 1.5 km long (lane 0), and the road back beside it (lane 1).
pub(crate) fn road(f: &Fixture) -> TrafficSim {
    let north = LaneBuilder::arc(DVec3::ZERO, 0.0, 1500.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let south = LaneBuilder::arc(DVec3::new(-3.5, 1500.0, 0.0), 180.0, 1500.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut net = Network { lanes: vec![north, south], ..Default::default() };
    net.link(1.5);
    net.build_grid();
    net.compute_reach();
    let random = RandomTypes { types: Vec::new(), groups: Vec::new(), group_curves: false, group_uvg: Vec::new(), uvg_defaults: Vec::new() };
    let mut t = TrafficSim::assemble(&f.dir, net, random, Vec::new(), HashMap::new(), (Vec::new(), Vec::new()), Vec::new(), (1.0, 0), 0);
    t.day_time = 8.0 * 3600.0;
    t
}

/// A timetable bus on lane 0 at `s` going `speed`, its trip's one stop leaving at `depart`
/// seconds from now (a stop the trip ends at, served whoever wants it).
fn add_bus(t: &mut TrafficSim, f: &Fixture, id: u64, s: f32, speed: f32, depart: f64) -> u64 {
    let stops = vec![Stop { ri: 0, s: STOP_S, bay: 0.0, depart: t.day_time + depart, id: STOP_ID, side: 0.0 }];
    let setup = BusSetup {
        route: vec![0],
        stops,
        number: None,
        hof: None,
        timetable: AiTimetable { line: String::new(), terminus: String::new(), stops: Vec::new() },
    };
    let vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
    t.place_car(vehicle, LaneKind::Street, 0, s, f.ty.clone(), id * 0x101, None, Some(id), Some(speed), Some(setup))
}

pub(super) fn add_car(t: &mut TrafficSim, f: &Fixture, id: u64, s: f32, speed: f32) -> u64 {
    let vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
    t.place_car(vehicle, LaneKind::Street, 0, s, f.ty.clone(), id * 0x77, None, Some(id), Some(speed), None)
}

fn car(t: &TrafficSim, id: u64) -> &AiCar {
    t.cars.iter().find(|c| c.id == id).expect("on the road")
}

/// What happened to each bus at the stop: where its origin stood each time it opened its
/// doors (m before the stop point), whether it ever held the berth.
#[derive(Debug, Default, Clone)]
struct Log {
    opened_at: Vec<f32>,
    held: bool,
    was_boarding: bool,
}

/// Run the scene for `secs` in steps, noting each bus's door openings; `each` sees the
/// scene before every step.
fn run(t: &mut TrafficSim, ids: &[u64], secs: f32, mut each: impl FnMut(&mut TrafficSim, f32)) -> Vec<Log> {
    let mut logs = vec![Log::default(); ids.len()];
    let mut time = 0.0;
    let dt = super::fixed_step::SIM_DT;
    while time < secs {
        each(t, time);
        t.tick(dt, None);
        time += dt;
        let boarding = t.cars.iter().filter(|c| c.at_station()).count();
        // (the bus at the berth and the one right behind it, as in OMSI 2 - never a third)
        assert!(boarding <= 2, "{boarding} buses with their doors open at one stop at {time:.1} s");
        for (k, &id) in ids.iter().enumerate() {
            let Some(c) = t.cars.iter().find(|c| c.id == id) else { continue };
            let b = c.bus.as_ref().unwrap();
            logs[k].held |= b.held_berth().is_some();
            let open = c.at_station();
            if open && !logs[k].was_boarding {
                logs[k].opened_at.push(STOP_S - c.state.s);
            }
            logs[k].was_boarding = open;
        }
    }
    logs
}

/// Three buses come to one stop close behind each other: the first docks at the berth, the
/// one right behind it serves the stop from where it stands at the same time (as two buses at
/// a stop do in OMSI 2, #2429), the third waits and docks once the berth is free. Each serves
/// the stop once, and none docks while another is still in the berth.
#[test]
fn three_buses_at_one_berth_serve_it_once_each_in_turn() {
    let f = Fixture::new();
    let mut t = road(&f);
    let ids = [add_bus(&mut t, &f, 1, 220.0, 8.0, 0.0), add_bus(&mut t, &f, 2, 195.0, 8.0, 0.0), add_bus(&mut t, &f, 3, 170.0, 8.0, 0.0)];
    let mut docked_while_occupied = false;
    let mut together = false;
    let logs = run(&mut t, &ids, 150.0, |t, _| {
        together |= t.cars.iter().filter(|c| c.at_station()).count() == 2;
        // the one with the doors open at the berth is in it alone: nobody else's body in it
        for c in t.cars.iter().filter(|c| c.at_station() && c.bus.as_ref().is_some_and(|b| b.held_berth().is_some())) {
            for o in t.cars.iter().filter(|o| o.id != c.id) {
                let (a, b) = (o.state.s - o.state.rear, o.state.s + o.state.front);
                docked_while_occupied |= b > STOP_S - c.state.rear && a < STOP_S + c.state.front;
            }
        }
    });
    for (k, l) in logs.iter().enumerate() {
        assert_eq!(l.opened_at.len(), 1, "bus {} opened its doors {} times: {l:?}", ids[k], l.opened_at.len());
    }
    assert!(logs[0].opened_at[0].abs() < 2.5 && logs[0].held, "{:?}", logs[0]);
    assert!(logs[1].opened_at[0] > 2.5 && logs[1].opened_at[0] < berths::SERVE_QUEUED_REACH, "the second served from {:.1} m", logs[1].opened_at[0]);
    assert!(logs[2].opened_at[0].abs() < 2.5 && logs[2].held, "{:?}", logs[2]);
    assert!(together, "the first two served the stop together");
    assert!(!docked_while_occupied, "a bus docked into another one");
    assert_eq!(t.berths.holder(berths::BerthKey { stop: STOP_ID, lane: 0 }), None);
    assert_eq!(t.berths.granted, t.berths.released);
}

/// A car broken down in the berth: the bus waits behind it without serving the stop, and
/// serves it at the berth once the car has gone.
#[test]
fn an_occupied_stop_is_waited_for() {
    let f = Fixture::new();
    let mut t = road(&f);
    let bus = add_bus(&mut t, &f, 1, 200.0, 8.0, 0.0);
    let broken = add_car(&mut t, &f, 50, STOP_S - 3.0, 0.0);
    let mut opened_while_there = false;
    let logs = run(&mut t, &[bus], 90.0, |t, time| {
        let here = t.cars.iter().any(|c| c.id == broken);
        if let Some(c) = t.cars.iter_mut().find(|c| c.id == broken) {
            if time < 15.0 {
                c.state.speed = 0.0;
                c.state.max_speed_kmh = 0.0;
            } else {
                c.state.max_speed_kmh = 50.0;
            }
        }
        opened_while_there |= here && time < 15.0 && t.cars.iter().any(|c| c.id == bus && c.at_station());
    });
    assert!(!opened_while_there, "the bus served the stop while the car stood in it");
    assert_eq!(logs[0].opened_at.len(), 1, "{:?}", logs[0]);
    assert!(logs[0].opened_at[0].abs() < 2.5, "served {:.1} m before the point", logs[0].opened_at[0]);
}

/// A bus on its layover holds the berth for a quarter of an hour: the bus behind serves the
/// stop from where it waits, holding no berth, and goes on.
#[test]
fn a_berth_held_for_long_is_served_from_behind() {
    let f = Fixture::new();
    let mut t = road(&f);
    let resting = add_bus(&mut t, &f, 1, STOP_S, 0.0, 900.0);
    t.cars.iter_mut().find(|c| c.id == resting).unwrap().bus.as_mut().unwrap().layover = true;
    t.cars.iter_mut().find(|c| c.id == resting).unwrap().bus.as_mut().unwrap().put_out = true;
    let coming = add_bus(&mut t, &f, 2, 200.0, 8.0, 0.0);
    let logs = run(&mut t, &[resting, coming], 90.0, |_, _| {});
    assert!(logs[0].held && logs[0].opened_at.is_empty(), "the layover bus: {:?}", logs[0]);
    assert_eq!(logs[1].opened_at.len(), 1, "{:?}", logs[1]);
    assert!(logs[1].opened_at[0] > 2.5 && logs[1].opened_at[0] < berths::SERVE_BEHIND_REACH, "served from {:.1} m", logs[1].opened_at[0]);
    assert!(!logs[1].held, "it held the berth");
    let c = car(&t, coming);
    assert!(c.bus.as_ref().unwrap().stops.is_empty() && !c.at_stop(), "it went on: {:?}", c.bus.as_ref().map(|b| b.phase));
}

/// A bus on a long stand at a stop several lines share, and three buses of other lines
/// coming one after the other (#1954): none of them waits the layover out - each serves the
/// stop behind it and goes round it, and all are on their way within minutes.
#[test]
fn buses_behind_a_long_stand_serve_and_go_round_it() {
    let f = Fixture::new();
    let mut t = road(&f);
    let resting = add_bus(&mut t, &f, 1, STOP_S, 0.0, 1800.0);
    t.cars.iter_mut().find(|c| c.id == resting).unwrap().bus.as_mut().unwrap().layover = true;
    t.cars.iter_mut().find(|c| c.id == resting).unwrap().bus.as_mut().unwrap().put_out = true;
    let coming: Vec<u64> = [(2, 200.0), (3, 140.0), (4, 80.0)].iter().map(|&(id, s)| add_bus(&mut t, &f, id, s, 8.0, 0.0)).collect();
    let ids: Vec<u64> = std::iter::once(resting).chain(coming.iter().copied()).collect();
    let logs = run(&mut t, &ids, 300.0, |_, _| {});
    assert!(logs[0].opened_at.is_empty(), "the layover bus: {:?}", logs[0]);
    for (k, id) in coming.iter().enumerate() {
        assert_eq!(logs[k + 1].opened_at.len(), 1, "bus {id}: {:?}", logs[k + 1]);
        let c = car(&t, *id);
        assert!(c.state.s > STOP_S + 20.0, "bus {id} is still at {:.0} m (the stop at {STOP_S})", c.state.s);
    }
}

/// Two buses on their layovers: the second serves from behind the first, and once the first
/// has left it moves up onto the berth, doors shut, and stands its layover out there.
#[test]
fn a_layover_behind_moves_up_once_the_berth_is_free() {
    let f = Fixture::new();
    let mut t = road(&f);
    let first = add_bus(&mut t, &f, 1, STOP_S, 0.0, 100.0);
    t.cars.iter_mut().find(|c| c.id == first).unwrap().bus.as_mut().unwrap().layover = true;
    t.cars.iter_mut().find(|c| c.id == first).unwrap().bus.as_mut().unwrap().put_out = true;
    let second = add_bus(&mut t, &f, 2, 230.0, 6.0, 900.0);
    t.cars.iter_mut().find(|c| c.id == second).unwrap().bus.as_mut().unwrap().layover = true;
    t.cars.iter_mut().find(|c| c.id == second).unwrap().bus.as_mut().unwrap().put_out = true;
    let mut moved_open = false;
    run(&mut t, &[first, second], 200.0, |t, _| {
        moved_open |= t.cars.iter().any(|c| c.id == second && c.at_station() && c.state.speed > 0.1);
    });
    let c = car(&t, second);
    let b = c.bus.as_ref().unwrap();
    assert!(!moved_open, "it moved with its doors open");
    assert!((STOP_S - c.state.s).abs() < 2.5, "it stands {:.1} m before the point", STOP_S - c.state.s);
    assert!(b.at_stop() && !b.behind, "{:?} behind {}", b.phase, b.behind);
    assert!(b.held_berth().is_some());
}

/// Three buses set down at the stop their trips end at, close behind each other, while the
/// first stands out its layover there (a tour's bus waiting to drive up to its next trip's
/// first stop, #3142): it tells the berth so, and the third, too far back to serve beside
/// it, serves the stop from where it waits instead of waiting in the road for the berth.
#[test]
fn buses_setting_down_behind_one_at_the_end_of_its_trip_serve_from_behind() {
    const END_STOP: f32 = 1470.0;
    let f = Fixture::new();
    let mut t = road(&f);
    let ids: Vec<u64> = [(1, 1390.0), (2, 1365.0), (3, 1340.0)]
        .iter()
        .map(|&(id, s)| {
            let stops = vec![Stop { ri: 0, s: END_STOP, bay: 0.0, depart: t.day_time, id: STOP_ID, side: 0.0 }];
            let setup = BusSetup { route: vec![0], stops, number: None, hof: None, timetable: AiTimetable { line: String::new(), terminus: String::new(), stops: Vec::new() } };
            let vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
            t.place_car(vehicle, LaneKind::Street, 0, s, f.ty.clone(), id * 0x101, None, Some(id), Some(8.0), Some(setup))
        })
        .collect();
    let mut opened: Vec<Vec<f32>> = vec![Vec::new(); 3];
    let mut was_open = [false; 3];
    let dt = super::fixed_step::SIM_DT;
    let mut time = 0.0;
    while time < 120.0 {
        t.tick(dt, None);
        time += dt;
        for (k, &id) in ids.iter().enumerate() {
            let c = car(&t, id);
            if c.at_station() && !was_open[k] {
                opened[k].push(END_STOP - c.state.s);
            }
            was_open[k] = c.at_station();
        }
    }
    let first = car(&t, ids[0]);
    assert!(first.trip_done() && first.bus.as_ref().unwrap().held_berth().is_some(), "the first stands at the berth at the end of its trip");
    for (k, o) in opened.iter().enumerate() {
        assert_eq!(o.len(), 1, "bus {} opened its doors {} times", ids[k], o.len());
    }
    assert!(opened[2][0] > berths::SERVE_QUEUED_REACH && opened[2][0] < berths::SERVE_BEHIND_REACH, "the third served from {:.1} m", opened[2][0]);
}

/// A bus too fast to stop at its stop runs past it: the stop is missed - the doors stay
/// shut, it does not back up, and it goes on.
#[test]
fn an_overshot_stop_is_missed_without_opening_the_doors() {
    let f = Fixture::new();
    let mut t = road(&f);
    let bus = add_bus(&mut t, &f, 1, 20.0, 12.0, 0.0);
    let mut moved = false;
    let logs = run(&mut t, &[bus], 40.0, |t, _| {
        // under way, the stop turns out to lie just behind the bus's origin: its front
        // has run past the stop point (no brakes stop a bus in no distance)
        let c = &mut t.cars[0];
        if !moved && c.state.speed > 10.0 {
            moved = true;
            let s = c.state.s - 4.0;
            c.bus.as_mut().unwrap().stops[0].s = s;
        }
    });
    assert!(moved);
    assert!(logs[0].opened_at.is_empty(), "{:?}", logs[0]);
    let c = car(&t, bus);
    assert_eq!(c.bus.as_ref().unwrap().missed, vec![STOP_ID]);
    assert!(c.state.s > 300.0, "it went on: at {:.1}", c.state.s);
    assert_eq!(t.berths.holder(berths::BerthKey { stop: STOP_ID, lane: 0 }), None);
}

/// Nobody wants an ordinary stop on the way: the bus drives past it. A stop its timetable
/// serves in any case is served.
#[test]
fn an_unwanted_stop_is_passed_and_a_timing_point_served() {
    let f = Fixture::new();
    for always in [false, true] {
        let mut t = road(&f);
        t.stop_wishes = Some((Default::default(), Default::default()));
        let bus = add_bus(&mut t, &f, 1, 200.0, 8.0, 0.0);
        {
            let b = t.cars[0].bus.as_mut().unwrap();
            // a stop further on is the trip's last
            b.stops.push_back(Stop { ri: 0, s: 900.0, bay: 0.0, depart: 8.0 * 3600.0 + 120.0, id: 99, side: 0.0 });
            b.last_stop = Some(99);
            if always {
                b.always = vec![STOP_ID];
            }
        }
        let logs = run(&mut t, &[bus], 40.0, |_, _| {});
        assert_eq!(logs[0].opened_at.len(), always as usize, "always {always}: {:?}", logs[0]);
        assert!(car(&t, bus).bus.as_ref().unwrap().missed.is_empty());
    }
}

/// The bus taken off the road gives its berth back, once.
#[test]
fn a_bus_taken_off_gives_its_berth_back() {
    let f = Fixture::new();
    let mut t = road(&f);
    let bus = add_bus(&mut t, &f, 1, STOP_S, 0.0, 300.0);
    run(&mut t, &[bus], 2.0, |_, _| {});
    let key = berths::BerthKey { stop: STOP_ID, lane: 0 };
    assert_eq!(t.berths.holder(key), Some(bus));
    let i = t.cars.iter().position(|c| c.id == bus).unwrap();
    t.take_off(i, Removal::TakenOver);
    assert_eq!(t.berths.holder(key), None);
    assert_eq!(t.berths.release_car(bus), 0);
    assert_eq!(t.removals[Removal::TakenOver as usize], 1);
}

/// A bus that cannot get across into its bay (it stands at the stop point, out in the lane,
/// too far from the kerb for anybody to get on) gives the stop up after a while: missed,
/// doors shut, and it drives on rather than stand there for good.
#[test]
fn a_stop_it_cannot_dock_at_is_missed_after_a_while() {
    let f = Fixture::new();
    let mut t = road(&f);
    let bus = add_bus(&mut t, &f, 1, 200.0, 8.0, 0.0);
    t.cars[0].bus.as_mut().unwrap().stops[0].bay = 3.0;
    let mut stood_still = 0.0f32;
    let logs = run(&mut t, &[bus], 60.0, |t, _| {
        let st = &mut t.cars[0].state;
        // (it does not get across: the S-curve into the bay takes for ever)
        st.lateral_ramp = (0.0, st.lateral_target, st.odometer, 1e6);
        if st.speed < 0.1 {
            stood_still += super::fixed_step::SIM_DT;
        }
    });
    assert!(logs[0].opened_at.is_empty(), "{:?}", logs[0]);
    let c = car(&t, bus);
    assert_eq!(c.bus.as_ref().unwrap().missed, vec![STOP_ID]);
    assert!(stood_still < 12.0, "stood {stood_still:.1} s");
    assert!(c.state.s > STOP_S + 50.0, "it went on: at {:.1}", c.state.s);
}

/// A bus in a bay does not pull out in front of a car coming up close behind: it waits for
/// it to go by. Standing in line with the lane, it just drives.
#[test]
fn pulling_out_of_a_bay_waits_for_a_car_close_behind() {
    let f = Fixture::new();
    for bay in [3.0f32, 0.0] {
        let mut t = road(&f);
        let bus = add_bus(&mut t, &f, 1, STOP_S, 0.0, 0.0);
        {
            let c = &mut t.cars[0];
            c.bus.as_mut().unwrap().stops[0].bay = bay;
            c.state.lateral = bay;
            c.state.lateral_target = bay;
            c.state.lateral_ramp = (bay, bay, 0.0, 1.0);
        }
        let (mut coming, mut left_with_car_behind, mut waited) = (None, None, 0.0f32);
        run(&mut t, &[bus], 40.0, |t, _| {
            let phase = car(t, bus).bus.as_ref().unwrap().phase;
            if phase == bus_service::Phase::Closing && coming.is_none() {
                let s = car(t, bus).state.s - 6.0 - 18.0;
                coming = Some(add_car(t, &f, 9, s, 9.0));
            }
            if phase == bus_service::Phase::Closing && coming.is_some() {
                waited += super::fixed_step::SIM_DT;
            }
            if let (Some(id), None) = (coming, left_with_car_behind) {
                if phase == bus_service::Phase::Running {
                    let b = car(t, bus);
                    let o = car(t, id);
                    let gap = b.state.s - b.state.rear - (o.state.s + o.state.front);
                    left_with_car_behind = Some(gap > 0.0 && gap < berths::PULL_OUT_BEHIND && o.state.speed > berths::PULL_OUT_COMING);
                }
            }
        });
        if bay > 0.0 {
            assert_eq!(left_with_car_behind, Some(false), "bay {bay}: pulled out in front of the car");
        } else {
            assert!(waited < 2.0, "in line with the lane it waited {waited:.1} s");
        }
    }
}

/// Something comes to stand in the berth after the bus was handed it (a car stopping there,
/// a queue at the lights beyond): the bus waits behind it, and after a long wait serves the
/// stop where it stands - nobody waits ahead of it - rather than miss it.
#[test]
fn a_holder_kept_short_of_its_point_serves_from_there_after_a_long_wait() {
    let f = Fixture::new();
    let mut t = road(&f);
    let bus = add_bus(&mut t, &f, 1, 200.0, 8.0, 0.0);
    let mut put = None;
    let logs = run(&mut t, &[bus], 80.0, |t, time| {
        let held = car(t, bus).bus.as_ref().unwrap().held_berth().is_some();
        if held && put.is_none() {
            put = Some((add_car(t, &f, 60, STOP_S + 1.0, 0.0), time));
        }
        if let Some((id, _)) = put {
            if let Some(c) = t.cars.iter_mut().find(|c| c.id == id) {
                c.state.speed = 0.0;
                c.state.max_speed_kmh = 0.0;
            }
        }
    });
    assert!(put.is_some());
    assert_eq!(logs[0].opened_at.len(), 1, "{:?}", logs[0]);
    assert!((SERVE_HERE_LONG_TEST..12.0).contains(&logs[0].opened_at[0]), "served {:.1} m before the point", logs[0].opened_at[0]);
    assert!(car(&t, bus).bus.as_ref().unwrap().missed.is_empty());
}

const SERVE_HERE_LONG_TEST: f32 = 2.5;
