//! Whole scenes for the body's side of the traffic, run headless: the planner keeping to
//! its body (`body_link`) and the bodies keeping out of the scenery (`scenery`).

use super::*;
use crate::ai_traffic::setup::RandomTypes;
use crate::traffic::{LaneBuilder, BODY_MAX_LEAD, BODY_SLACK};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A vehicle type with no model and no scripts, in a folder of its own.
pub(super) struct Fixture {
    dir: std::path::PathBuf,
    pub ty: Arc<VehicleType>,
}

impl Fixture {
    pub fn new() -> Fixture {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("omsi-traffic-body-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
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
    net.link(1.5);
    net.build_grid();
    net.compute_reach();
    let random = RandomTypes { types: Vec::new(), groups: Vec::new(), group_curves: false, group_uvg: Vec::new(), uvg_defaults: Vec::new() };
    TrafficSim::assemble(&f.dir, net, random, Vec::new(), HashMap::new(), (Vec::new(), Vec::new()), Vec::new(), (1.0, 0), 0)
}

pub(super) fn add_car(t: &mut TrafficSim, f: &Fixture, lane: usize, s: f32, seed: u64, speed: Option<f32>) -> u64 {
    let vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
    t.place_car(vehicle, LaneKind::Street, lane, s, f.ty.clone(), seed, None, None, speed, None)
}

pub(super) fn car(t: &TrafficSim, id: u64) -> &AiCar {
    t.cars.iter().find(|c| c.id == id).expect("the car is there")
}

#[test]
fn traffic_pedals_drive_script_engine_load_and_braking() {
    let mut f = Fixture::new();
    let scripted = crate::timetable_run::tests::script_test_vehicle(
        "{frame}\n(L.L.Throttle) (S.L.engine_throttle_injection)\n(L.L.Brake) (S.L.brake_seen)\n{end}\n",
        "Throttle\nBrake\nengine_throttle_injection\nbrake_seen\n",
        "",
    );
    Arc::get_mut(&mut f.ty).unwrap().program = scripted.ty.program.clone();
    let lane = LaneBuilder::arc(DVec3::ZERO, 0.0, 1000.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut t = traffic(&f, Network { lanes: vec![lane], ..Default::default() });
    let id = add_car(&mut t, &f, 0, 10.0, 42, Some(0.0));
    for _ in 0..250 {
        t.advance(0.02, None);
    }
    let c = car(&t, id);
    assert!(c.state.speed > 0.5, "speed {} cmd {} held {} why {:?}", c.state.speed, c.state.cmd, c.state.held, c.why);
    assert!(c.vehicle.var("engine_throttle_injection").unwrap() > 0.0);
    assert_eq!(c.vehicle.var("brake_seen"), Some(0.0));

    t.net.lanes[0].speed_limit_kmh = 3.0;
    for _ in 0..10 {
        t.advance(0.02, None);
    }
    let c = car(&t, id);
    assert!(c.state.cmd < 0.0);
    assert_eq!(c.vehicle.var("engine_throttle_injection"), Some(0.0));
    assert!(c.vehicle.var("brake_seen").unwrap() > 0.0);
    assert!((0.0..=1.0).contains(&c.vehicle.physics.controls.brake));
}

/// 120 m north, a right turn of radius `r` over 90°, 200 m east.
pub(super) fn turn(r: f64) -> Network {
    let a = LaneBuilder::arc(DVec3::ZERO, 0.0, 120.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let bend = LaneBuilder::arc(a.end(), 0.0, r * std::f64::consts::FRAC_PI_2, r, 0.0, LaneKind::Street, 3.0);
    let c = LaneBuilder::arc(bend.end(), 90.0, 200.0, 0.0, 0.0, LaneKind::Street, 3.0);
    Network { lanes: vec![a, bend, c], ..Default::default() }
}

/// Driving round a tight corner the body is where the plan is: found on its way all the
/// time, never more than half a metre behind it, and the plan never waits for it.
#[test]
fn a_body_keeps_to_its_plan_round_a_corner() {
    let f = Fixture::new();
    for r in [8.0, 15.0] {
        let mut t = traffic(&f, turn(r));
        let id = add_car(&mut t, &f, 0, 60.0, 0x42, Some(12.0));
        let (mut worst, mut off, mut steps) = (f32::MIN, 0, 0);
        for _ in 0..1000 {
            t.advance(0.02, None);
            let c = car(&t, id);
            let b = c.state.body;
            assert!(b.known, "radius {r}: the body is lost at lane {} s {:.1}", c.state.lane, c.state.s);
            worst = worst.max(b.lead);
            off += (!b.on_way) as u32;
            steps += 1;
        }
        let c = car(&t, id);
        assert!(c.state.lane == 2 && c.state.s > 30.0, "radius {r}: only at lane {} s {:.1}", c.state.lane, c.state.s);
        assert!(worst < BODY_SLACK, "radius {r}: the plan ran {worst:.2} m ahead of its body");
        assert_eq!(off, 0, "radius {r}: off its way {off} of {steps} steps");
    }
}

/// A body that cannot follow (held where it is, here by hand every step) is not run ahead
/// of: within a step or two the plan stands two metres ahead of it and waits, whether the
/// body stands on its way or three metres beside it.
#[test]
fn a_body_off_its_route_is_not_run_ahead_of() {
    let f = Fixture::new();
    for beside in [0.0, 3.0] {
        let mut t = traffic(&f, turn(15.0));
        let id = add_car(&mut t, &f, 0, 40.0, 0x77, Some(10.0));
        // drive for a moment, then the body stops where it is
        for _ in 0..50 {
            t.advance(0.02, None);
        }
        let i = t.cars.iter().position(|c| c.id == id).unwrap();
        let mut held = t.cars[i].body.clone();
        if beside > 0.0 {
            let st = t.cars[i].state.clone();
            let net = &t.net;
            held.place(&|d| st.way_point(net, d) + DVec3::new(beside, 0.0, 0.0), None, None, 0.0);
        }
        let s0 = t.cars[i].state.s;
        let mut worst = f32::MIN;
        for _ in 0..250 {
            t.cars[i].body = held.clone();
            // (the body stands: its own speed is nothing)
            t.cars[i].body.moved = 0.0;
            t.cars[i].state.body = body_link::link(&t.cars[i].state, &t.net, &t.cars[i].body);
            t.advance(0.02, None);
            let lead = body_link::project(&t.cars[i].state, &t.net, &held, 1.5).map(|p| -p.along).unwrap_or(f32::MAX);
            worst = worst.max(lead);
        }
        let st = &t.cars[i].state;
        assert!(worst <= BODY_MAX_LEAD + 0.25, "{beside} m beside: the plan ran {worst:.2} m ahead of its body");
        assert!(st.speed < 0.5, "{beside} m beside: the plan still goes {:.1} m/s", st.speed);
        assert!(st.s - s0 < BODY_MAX_LEAD + 0.5, "{beside} m beside: the plan ran {:.1} m on", st.s - s0);
    }
}

/// A box of the scenery from the road up to 3 m, `half` its half extents, turned `heading`
/// degrees.
fn block(center: DVec2, half: DVec2, heading: f64) -> crate::collision::Obb {
    crate::collision::Obb { center, half, heading: heading.to_radians(), z0: -0.5, z1: 3.0, ..crate::collision::Obb::point(DVec3::ZERO, 0.0) }
}

fn with_scenery(t: &mut TrafficSim, boxes: &[crate::collision::Obb]) {
    let mut world = crate::collision::CollisionWorld::default();
    for b in boxes {
        world.add(*b);
    }
    t.occluders = Some(Arc::new(world));
}

/// How deep car `id`'s body is in `o` (m, 0 clear).
fn depth_in(t: &TrafficSim, id: u64, o: &crate::collision::Obb) -> f64 {
    let c = car(t, id);
    let me = crate::collision::Obb::vehicle(c.vehicle.position.truncate(), c.vehicle.heading, c.state.front as f64, c.state.rear as f64, c.half_width as f64);
    let mut o = *o;
    (o.z0, o.z1) = (f64::MIN, f64::MAX);
    me.contact(&o).map(|c| c.depth).unwrap_or(0.0)
}

/// A wall across the road (a map's object laid over it): the car sees it coming along its
/// way, brakes as for a line and stands short of it - it does not touch it, and it brakes
/// within the ordinary envelope. Only once it has stood there for `PATIENCE` seconds does
/// it give the wall up and drive through it as Omsi.exe's AI would.
#[test]
fn a_car_brakes_before_a_wall_rather_than_into_it() {
    let f = Fixture::new();
    let mut t = traffic(&f, turn(15.0));
    let wall = block(DVec2::new(0.0, 100.0), DVec2::new(3.0, 0.3), 0.0);
    with_scenery(&mut t, &[wall]);
    let id = add_car(&mut t, &f, 0, 30.0, 0x31, Some(12.0));
    let (mut worst, mut hardest, mut emergencies, mut stood) = (0.0f64, 0.0f32, 0, 0.0f32);
    for _ in 0..(25.0 / 0.02) as usize {
        t.advance(0.02, None);
        let c = car(&t, id);
        worst = worst.max(depth_in(&t, id, &wall));
        hardest = hardest.min(c.state.acc);
        emergencies += c.state.emergency_began as u32;
        if c.state.speed < 0.05 {
            stood += 0.02;
        }
        if stood > scenery::PATIENCE - 1.5 {
            break;
        }
    }
    let c = car(&t, id);
    assert_eq!(worst, 0.0, "the body got {worst:.2} m into the wall");
    assert!(stood > 4.0, "it stood {stood:.1} s only");
    let gap = 100.0 - 0.3 - (c.vehicle.position.y + c.state.front as f64);
    assert!(gap > 0.0 && gap < 2.5, "it stopped {gap:.2} m short of the wall");
    assert_eq!(c.why.0, "scenery");
    assert_eq!(emergencies, 0, "it had to stop in an emergency");
    assert!(hardest > -c.state.ordinary_brake, "it braked at {hardest:.1} m/s²");
    // given up after its patience: the way runs through it
    for _ in 0..(10.0 / 0.02) as usize {
        t.advance(0.02, None);
    }
    assert!(car(&t, id).vehicle.position.y > 100.0, "it never drove on");
    assert_eq!(t.scenery_through.len(), 1);
}

/// A post standing 30 cm into the lane at its edge: the car is nudged round it within the
/// lane and drives past without touching it or stopping, and back to the middle of the lane
/// afterwards.
#[test]
fn a_car_is_nudged_round_a_post_at_its_side() {
    let f = Fixture::new();
    let mut t = traffic(&f, turn(15.0));
    let post = block(DVec2::new(0.9 - 0.3 + 0.1, 80.0), DVec2::new(0.1, 0.1), 0.0);
    with_scenery(&mut t, &[post]);
    let id = add_car(&mut t, &f, 0, 20.0, 0x51, Some(10.0));
    let (mut worst, mut slowest, mut widest) = (0.0f64, f32::MAX, 0.0f32);
    for _ in 0..(9.0 / 0.02) as usize {
        t.advance(0.02, None);
        let c = car(&t, id);
        worst = worst.max(depth_in(&t, id, &post));
        slowest = slowest.min(c.state.speed);
        widest = widest.min(c.state.nudge_at(c.state.odometer));
    }
    let c = car(&t, id);
    assert!(c.vehicle.position.y > 95.0, "it got only to {:.1}", c.vehicle.position.y);
    assert_eq!(worst, 0.0, "the body got {worst:.2} m into the post");
    assert!(slowest > 5.0, "it slowed to {slowest:.1} m/s");
    assert!((-0.61..-0.3).contains(&widest), "nudged {widest:.2} m");
    // and back in the middle once past
    for _ in 0..(4.0 / 0.02) as usize {
        t.advance(0.02, None);
    }
    let c = car(&t, id);
    assert!(c.state.nudge_at(c.state.odometer).abs() < 0.01, "still nudged {:.2} m", c.state.nudge_at(c.state.odometer));
}

/// A wall that turns up right in front of a moving car, too close for any look ahead (put
/// there between two steps): the steps that would take the body into it are taken back, the
/// body stands at it, and the plan does not run on without it.
#[test]
fn a_step_into_the_scenery_is_taken_back() {
    let f = Fixture::new();
    let mut t = traffic(&f, turn(15.0));
    let id = add_car(&mut t, &f, 0, 30.0, 0x61, Some(8.0));
    // (past its first second and a half, when a car just put on the road still slows
    // down at once for what it finds ahead)
    for _ in 0..100 {
        t.advance(0.02, None);
    }
    let c = car(&t, id);
    let front = c.vehicle.position.y + c.state.front as f64;
    let wall = block(DVec2::new(0.0, front + 0.6 + 0.3), DVec2::new(3.0, 0.3), 0.0);
    with_scenery(&mut t, &[wall]);
    let (mut worst, mut rejected, mut lead) = (0.0f64, 0, f32::MIN);
    for _ in 0..(3.0 / 0.02) as usize {
        t.advance(0.02, None);
        let c = car(&t, id);
        worst = worst.max(depth_in(&t, id, &wall));
        rejected += c.guard.rejected as u32;
        lead = lead.max(c.state.body.lead);
    }
    assert!(worst < 0.05, "the body got {worst:.2} m into the wall");
    assert!(rejected > 0, "no step was taken back");
    assert!(lead <= BODY_MAX_LEAD + 0.25, "the plan ran {lead:.2} m ahead of its body");
    assert!(car(&t, id).state.speed < 0.5);
}

/// The looks along the way and the steps taken back run on the traffic's own steps: a
/// scene with scenery in it drives the same at any frame rate.
#[test]
fn the_scenery_watch_drives_the_same_at_any_frame_rate() {
    let f = Fixture::new();
    let run_at = |fps: f32| {
        let mut t = traffic(&f, turn(10.0));
        with_scenery(&mut t, &[block(DVec2::new(0.0, 100.0), DVec2::new(3.0, 0.3), 0.0), block(DVec2::new(0.75, 70.0), DVec2::new(0.1, 0.1), 0.0)]);
        add_car(&mut t, &f, 0, 20.0, 0x31, Some(10.0));
        add_car(&mut t, &f, 0, 5.0, 0x32, Some(10.0));
        for _ in 0..(12.0 * fps).round() as usize {
            t.advance(1.0 / fps, None);
        }
        let mut v: Vec<_> = t.cars.iter().map(|c| (c.id, c.state.lane, c.state.s, c.state.speed, c.state.nudge_at(c.state.odometer))).collect();
        v.sort_by_key(|x| x.0);
        v
    };
    let slow = run_at(30.0);
    assert_eq!(slow, run_at(60.0));
    assert_eq!(slow, run_at(144.0));
}

#[test]
fn a_stop_request_fires_the_trigger_on_the_timetable_bus_only() {
    let mut f = Fixture::new();
    let scripted = crate::timetable_run::tests::script_test_vehicle("{trigger:int_haltewunsch}\n1 (S.L.stop_seen)\n{end}\n", "stop_seen\n", "");
    Arc::get_mut(&mut f.ty).unwrap().program = scripted.ty.program.clone();
    let lane = LaneBuilder::arc(DVec3::ZERO, 0.0, 1000.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut t = traffic(&f, Network { lanes: vec![lane], ..Default::default() });
    let bus = add_car(&mut t, &f, 0, 10.0, 42, Some(0.0));
    let other = add_car(&mut t, &f, 0, 50.0, 43, Some(0.0));
    t.request_stop(bus);
    assert_eq!(car(&t, bus).vehicle.var("stop_seen"), Some(1.0), "the bus's script hears its stop request");
    assert_ne!(car(&t, other).vehicle.var("stop_seen"), Some(1.0), "another car's script does not");
}
