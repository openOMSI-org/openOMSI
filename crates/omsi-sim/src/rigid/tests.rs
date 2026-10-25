use super::*;

/// The body's pitch and roll are damped as much a second at any frame rate: as OMSI
/// damps them at 30 frames a second (a 12 t bus on 240-280 kN/m springs).
#[test]
fn body_damping_does_not_depend_on_the_frame_rate() {
    let f = ((2.0 * 240.0 + 2.0 * 280.0) / 12.0f32).sqrt();
    let second = |fps: f32| (0..fps as usize).fold(1.0f32, |k, _| k * super::body_damping(f, 1.0 / fps));
    let (a, b, c) = (second(30.0), second(60.0), second(144.0));
    assert!((a - b).abs() < 1e-3 && (a - c).abs() < 1e-3, "{a} {b} {c}");
    assert!(a > 0.2 && a < 0.5, "{a}");
}
use omsi_vehicle::vehicle::Axle;

fn bus() -> Vehicle {
    let axle = |long: f32, spring: f32, max_force: f32, driven: bool| Axle { long, max_width: 2.4, min_width: 1.76, wheel_diameter: 0.94, spring, max_force, damper: 20.0, driven, inertia_inv: 0.0 };
    Vehicle { mass: 10.2, moment_of_inertia: [600.0, 90.0, 400.0], cog_height: 1.2, rolling_resistance: 1000.0, rot_pnt_long: -2.7, inv_min_turn_radius: 0.13, axles: vec![axle(3.238, 240.0, 90.0, false), axle(-2.637, 280.0, 116.0, true)], bounding_box: Some([2.472, 11.663, 2.491, 0.0, 0.001, 1.622]), ..Default::default() }
}

/// Flat road at height 0 with an optional step of `step` metres from y = `at` on.
fn road(at: f64, step: f64) -> impl Fn(f64, f64, f64) -> GroundProbe {
    move |_x, y, z_top| {
        let z = if y >= at { step } else { 0.0 };
        if z <= z_top {
            GroundProbe { below: Some(z), above: None }
        } else {
            GroundProbe { below: Some(0.0), above: Some(z) }
        }
    }
}

fn run(rb: &mut RigidBody, secs: f32, torque: f32, brake: f32, g: &dyn Fn(f64, f64, f64) -> GroundProbe) {
    let brakes = vec![brake; rb.wheels.len()];
    for _ in 0..(secs * 60.0) as usize {
        rb.step(1.0 / 60.0, torque, &brakes, 0.0, g);
    }
}

#[test]
fn driving_past_a_thin_roadside_triangle_has_no_false_wall() {
    let mut grid = omsi_geometry::DriveGrid::default();
    grid.push([Vec3::ZERO, Vec3::new(30.0, 0.0, 0.0), Vec3::new(0.0, 50.0, 0.0)]);
    grid.push([Vec3::new(30.0, 0.0, 0.0), Vec3::new(30.0, 50.0, 0.0), Vec3::new(0.0, 50.0, 0.0)]);
    // Entirely beside the bus's wheel track. A tolerance on the infinite
    // edge lines extended this face into the road and extrapolated its height.
    grid.push([Vec3::new(10.0, 10.0, 0.0), Vec3::new(12.0, 9.6, 0.8), Vec3::new(10.0, 9.999, 0.0)]);
    grid.build(300.0);
    let ground = |x: f64, y: f64, top: f64| {
        let p = grid.probe(x as f32, y as f32, top as f32);
        GroundProbe { below: p.below.map(f64::from), above: p.above.map(f64::from) }
    };
    for fps in [30, 60] {
        for kmh in [5.0, 40.0, 45.0, 60.0] {
            let mut rb = RigidBody::from_definition(&bus(), &[]);
            rb.place(DVec3::new(14.0, 4.0, 0.0), 0.0);
            run(&mut rb, 2.0, 0.0, 0.0, &ground);
            let speed = kmh / 3.6;
            rb.velocity = Vec3::Y * speed;
            for w in &mut rb.wheels { w.spin = speed / w.radius; }
            let radius = rb.wheels.iter().find(|w| w.driven).unwrap().radius;
            let dt = 1.0 / fps as f32;
            // Hold speed with drive torque so the slow case crosses too.
            for _ in 0..(18.0 / speed / dt).ceil() as usize {
                let torque = ((speed - rb.forward_speed()) * rb.mass * 2.0 + rb.rolling_resistance) * radius;
                rb.step(dt, torque, &[0.0; 4], 0.0, &ground);
                assert!(rb.wheel_impacts.is_empty(), "{kmh} km/h at {fps} fps: {:?}", rb.wheel_impacts);
                assert!(rb.wheels.iter().all(|w| w.walls.is_empty()), "{kmh} km/h at {fps} fps: false wheel wall");
            }
            assert!(rb.position.y > 20.0, "{kmh} km/h at {fps} fps: stopped at {:?}", rb.position);
            assert!(rb.forward_speed() > speed * 0.9, "{kmh} km/h at {fps} fps: lost speed");
        }
    }
}

/// `Axle_Steering_*` is the axle's angle, one for both sides, the same to the left as to
/// the right (#953): the tyres keep their own Ackermann angles for the physics.
#[test]
fn the_axle_steers_as_far_left_as_right() {
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    rb.place(DVec3::ZERO, 0.0);
    let g = road(1e9, 0.0);
    let mut at = |steer: f32| {
        for _ in 0..5 {
            rb.step(1.0 / 60.0, 0.0, &[0.0; 4], steer, &g);
        }
        (rb.axle_steer(0), rb.axle_steer(1), rb.wheels[0].steer, rb.wheels[1].steer)
    };
    let (l0, r0, tl0, tr0) = at(-1.0);
    let (l1, r1, tl1, tr1) = at(1.0);
    assert_eq!(l0, r0);
    assert_eq!(l1, r1);
    assert!((l0 + l1).abs() < 1e-6, "left {l0} right {l1}");
    // (the full lock of 0.13 1/m, 5.94 m ahead of the turning line)
    assert!((l1 - (5.938f32 * 0.13).atan()).abs() < 1e-3, "{l1}");
    // the inner tyre turns further than the outer
    assert!(tl0.abs() > tr0.abs() && tr1.abs() > tl1.abs(), "{tl0} {tr0} {tl1} {tr1}");
}

/// A brake stronger than the road takes locks the wheels (the ABS scripts see the
/// wheel stand while the bus still moves); a brake the road takes keeps them turning.
#[test]
fn a_brake_beyond_the_grip_locks_the_wheels() {
    for (brake, locks) in [(60_000.0, true), (8_000.0, false)] {
        let mut rb = RigidBody::from_definition(&bus(), &[]);
        rb.place(DVec3::ZERO, 0.0);
        let g = road(1e9, 0.0);
        run(&mut rb, 2.0, 0.0, 0.0, &g);
        rb.velocity = rb.orientation.mul_vec3(Vec3::Y) * 12.0;
        for w in rb.wheels.iter_mut() {
            w.spin = 12.0 / w.radius;
        }
        run(&mut rb, 0.6, 0.0, brake, &g);
        let v = rb.velocity.length();
        let locked = rb.wheels.iter().all(|w| w.spin.abs() * w.radius < 0.5);
        assert!(v > 5.0, "still moving: {v}");
        assert_eq!(locked, locks, "brake {brake}: spins {:?} at {v} m/s", rb.wheels.iter().map(|w| w.spin).collect::<Vec<_>>());
    }
}

/// The NL202 sits about 0.10 m below its unloaded height - its own `ai_deltaheight` -
/// with the wheels exactly on the road.
#[test]
fn sags_by_its_own_springs() {
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    rb.place(DVec3::ZERO, 0.0);
    let g = road(1e9, 0.0);
    run(&mut rb, 3.0, 0.0, 5000.0, &g);
    let origin = rb.origin();
    assert!((origin.z + 0.097).abs() < 0.01, "origin {origin:?}");
    for w in &rb.wheels {
        assert!((w.compression - 0.097).abs() < 0.015, "compression {}", w.compression);
        assert!(w.ground_z.abs() < 1e-6);
    }
    // a softer air spring lowers it further
    let mut soft = RigidBody::from_definition(&bus(), &[]);
    soft.wheels.iter_mut().for_each(|w| w.spring_factor = 0.5);
    soft.place(DVec3::ZERO, 0.0);
    run(&mut soft, 3.0, 0.0, 5000.0, &g);
    assert!(soft.origin().z < origin.z - 0.08, "{:?}", soft.origin());
}

/// A 6 cm hump 0.6 m long (a manhole cover's rim, a speed cushion's edge) crossed at
/// 40 km/h: the wheels ride over it without leaving the road for long, the body
/// hardly lifts, and a wheel's travel changes smoothly from frame to frame instead of
/// jumping into the arch and out to full droop.
#[test]
fn a_long_frame_does_not_bounce_the_body() {
    // driving on flat road, one frame held up for 60 ms (loading): the body keeps still
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    rb.place(DVec3::ZERO, 0.0);
    let g = road(1e9, 0.0);
    run(&mut rb, 3.0, 3000.0, 5000.0, &g);
    let before: Vec<f32> = rb.wheels.iter().map(|w| w.compression).collect();
    rb.step(0.1, 3000.0, &[0.0; 4], 0.0, &g);
    let mut worst = 0.0f32;
    for _ in 0..30 {
        rb.step(1.0 / 60.0, 3000.0, &[0.0; 4], 0.0, &g);
        for (w, b) in rb.wheels.iter().zip(&before) {
            worst = worst.max((w.compression - b).abs());
        }
    }
    assert!(worst < 0.02, "the suspension moved {worst} m after a long frame");
}

#[test]
fn rides_over_a_hump() {
    let hump = |_x: f64, y: f64, z_top: f64| {
        let z = if (20.0..20.6).contains(&y) { 0.06 } else { 0.0 };
        if z <= z_top {
            GroundProbe { below: Some(z), above: None }
        } else {
            GroundProbe { below: Some(0.0), above: Some(z) }
        }
    };
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    rb.place(DVec3::ZERO, 0.0);
    run(&mut rb, 2.0, 0.0, 5000.0, &hump);
    rb.velocity = rb.orientation.mul_vec3(Vec3::Y) * (40.0 / 3.6);
    let z0 = rb.position.z;
    let (mut lift, mut jump, mut air) = (0.0f64, 0.0f32, 0usize);
    let mut last: Vec<f32> = rb.wheels.iter().map(|w| w.compression).collect();
    for _ in 0..(3.0 * 60.0) as usize {
        rb.step(1.0 / 60.0, 0.0, &[0.0; 4], 0.0, &hump);
        lift = lift.max(rb.position.z - z0);
        for (k, w) in rb.wheels.iter().enumerate() {
            jump = jump.max((w.compression - last[k]).abs());
            last[k] = w.compression;
            if !w.on_ground {
                air += 1;
            }
        }
    }
    assert!(rb.origin().y > 30.0, "{:?}", rb.origin());
    assert!(lift < 0.05, "the body lifted {lift:.3} m");
    // (Omsi.exe's wheel stands on the point under it: at the hump's edge its travel
    // takes the hump's height at once, no more)
    assert!(jump < 0.065, "a wheel's travel jumped {jump:.3} m in a frame");
    assert!(air < 16, "wheels off the road for {air} wheel-frames");
}

/// Drive at `torque` (N m) for `secs` from rest against the ground `g`; where the front
/// axle ends up.
fn climb(g: &dyn Fn(f64, f64, f64) -> GroundProbe, torque: f32, secs: f32) -> (RigidBody, f64) {
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    rb.place(DVec3::ZERO, 0.0);
    run(&mut rb, 1.0, 0.0, 5000.0, g);
    run(&mut rb, secs, torque, 0.0, g);
    let front = rb.origin().y + 3.238;
    (rb, front)
}

/// A 15 cm kerb takes a firm push from a standstill (full torque: about 20 kN at the
/// rear wheels) but not the idle creep of 3 kN; a lowered kerb of 3 cm the creep takes;
/// a 60 cm wall stops the bus whatever it does.
#[test]
fn climbs_a_kerb_but_not_a_wall() {
    let kerb = road(6.0, 0.15);
    let (rb, front) = climb(&kerb, 9500.0, 10.0);
    assert!(front > 9.0, "front axle at {front}");
    assert!((rb.wheels[0].ground_z - 0.15).abs() < 0.01, "front wheel on the kerb: {}", rb.wheels[0].ground_z);
    let (_, front) = climb(&kerb, 1400.0, 10.0);
    assert!(front < 6.0, "idle creep climbed the kerb: front axle at {front}");
    let lowered = road(6.0, 0.03);
    let (_, front) = climb(&lowered, 1400.0, 15.0);
    assert!(front > 7.0, "idle creep stuck at a lowered kerb: front axle at {front}");

    let mut rb = RigidBody::from_definition(&bus(), &[]);
    rb.place(DVec3::ZERO, 0.0);
    let wall = road(6.0, 0.6);
    run(&mut rb, 1.0, 0.0, 5000.0, &wall);
    run(&mut rb, 12.0, 6000.0, 0.0, &wall);
    let front = rb.origin().y + 3.238;
    assert!(front < 6.0 && front > 5.3, "front axle at {front}");
    // and it backs off at once
    let y0 = rb.origin().y;
    run(&mut rb, 3.0, -6000.0, 0.0, &wall);
    assert!(rb.origin().y < y0 - 1.0, "{} -> {}", y0, rb.origin().y);
}

/// A pusher's front section has no driven wheels: the push of the rear section's axle
/// comes through the joint and moves it like the same torque on its own axle, the towed
/// mass slows the start, and the brakes of either part hold the train.
#[test]
fn pusher_front_section_is_pushed_through_the_joint() {
    let mut def = bus();
    def.axles.iter_mut().for_each(|a| a.driven = false);
    let rear = CoupledPart { point: Vec3::new(0.0, -4.456, 0.503), dir: Vec3::Y, mass: 0.0, driven_wheels: 2, radius: 0.47, driven_load: 70_000.0, brake: 0.0, load: 70_000.0, joint_load: 0.0 };
    let g = road(1e9, 0.0);
    let start = |def: &Vehicle, parts: Vec<CoupledPart>| {
        let mut rb = RigidBody::from_definition(def, &[]);
        rb.coupled = parts;
        rb.place(DVec3::ZERO, 0.0);
        run(&mut rb, 1.0, 0.0, 5000.0, &g);
        rb
    };
    // held by the front section's brakes, then by the rear section's alone
    let mut rb = start(&def, vec![rear]);
    run(&mut rb, 2.0, 9000.0, 20_000.0, &g);
    assert!(rb.origin().y.abs() < 0.05, "crept to {:?}", rb.origin());
    rb.coupled[0].brake = 80_000.0;
    run(&mut rb, 2.0, 9000.0, 0.0, &g);
    assert!(rb.origin().y.abs() < 0.05, "crept to {:?}", rb.origin());
    rb.coupled[0].brake = 0.0;
    run(&mut rb, 5.0, 9000.0, 0.0, &g);
    let pushed = rb.origin().y;
    // the same torque on its own rear axle
    let mut own = start(&bus(), Vec::new());
    run(&mut own, 5.0, 9000.0, 0.0, &g);
    let driven = own.origin().y;
    assert!(pushed > 5.0 && (pushed - driven).abs() < 0.1 * driven, "pushed {pushed} m, driven {driven} m");
    let (heading, _, _) = rb.heading_pitch_bank();
    assert!(rb.origin().x.abs() < 0.05 && heading.min(360.0 - heading) < 0.5, "{:?} {heading}", rb.origin());
    // the rear section's brakes stop the train
    let v = rb.forward_speed();
    rb.coupled[0].brake = 30_000.0;
    run(&mut rb, 1.0, 0.0, 0.0, &g);
    assert!(rb.forward_speed() < v - 2.0, "{v} -> {}", rb.forward_speed());
    // and backwards
    rb.coupled[0].brake = 0.0;
    run(&mut rb, 1.0, 0.0, 20_000.0, &g);
    let y0 = rb.origin().y;
    run(&mut rb, 6.0, -9000.0, 0.0, &g);
    assert!(rb.origin().y < y0 - 1.0, "{} -> {}", y0, rb.origin().y);
    // a towed mass as heavy as the body halves the acceleration
    let mut heavy = start(&def, vec![CoupledPart { mass: rb.mass, ..rear }]);
    run(&mut heavy, 5.0, 9000.0, 0.0, &g);
    let ratio = heavy.origin().y / pushed;
    assert!((ratio - 0.5).abs() < 0.08, "heavy train went {} m, light {pushed} m", heavy.origin().y);
    // a joint bent by 30 degrees pushes with its share along the body and does not turn
    // it, standing against the brakes or driving
    let bent_dir = Vec3::new(0.5, 0.866, 0.0);
    let mut bent = start(&def, vec![CoupledPart { dir: bent_dir, brake: 30_000.0, ..rear }]);
    run(&mut bent, 5.0, 9000.0, 20_000.0, &g);
    let (heading, _, _) = bent.heading_pitch_bank();
    assert!(bent.origin().truncate().length() < 0.05 && heading.min(360.0 - heading) < 0.2, "{:?} {heading}", bent.origin());
    bent.coupled[0].brake = 0.0;
    run(&mut bent, 5.0, 9000.0, 0.0, &g);
    let (heading, _, _) = bent.heading_pitch_bank();
    let ratio = bent.origin().y / pushed;
    assert!((ratio - 0.866).abs() < 0.08 && heading.min(360.0 - heading) < 0.5, "{:?} {heading} ({ratio})", bent.origin());
}

/// A towed part on a grade pulls the body down it, and its brakes hold it there.
#[test]
fn towed_weight_on_a_grade() {
    let mut def = bus();
    def.axles.iter_mut().for_each(|a| a.driven = false);
    let g = ramp(0.08);
    let part = CoupledPart { point: Vec3::new(0.0, -4.456, 0.503), dir: Vec3::Y, mass: 7000.0, driven_wheels: 0, radius: 0.47, driven_load: 0.0, brake: 0.0, load: 70_000.0, joint_load: 0.0 };
    let mut rb = RigidBody::from_definition(&def, &[]);
    rb.coupled = vec![CoupledPart { brake: 40_000.0, ..part }];
    rb.place(DVec3::new(0.0, 50.0, 4.0), 0.0);
    run(&mut rb, 3.0, 0.0, 0.0, &g);
    let y0 = rb.origin().y;
    run(&mut rb, 3.0, 0.0, 0.0, &g);
    assert!((rb.origin().y - y0).abs() < 0.02, "rolled from {y0} to {}", rb.origin().y);
    rb.coupled[0].brake = 0.0;
    run(&mut rb, 3.0, 0.0, 0.0, &g);
    assert!(rb.origin().y < y0 - 1.0, "stayed at {}", rb.origin().y);
}

/// A ramp of grade `s` along y (from y = 0), steps over `CLIMB` radii read as faces.
fn ramp(s: f64) -> impl Fn(f64, f64, f64) -> GroundProbe {
    move |_x, y, top| {
        let z = y * s;
        if z <= top { GroundProbe { below: Some(z), above: None } } else { GroundProbe { below: None, above: Some(z) } }
    }
}

/// A kerb running along under the tread lifts the hub by the share of the tread over
/// it, smoothly as the edge passes under the tyre - not all at once as the edge passes
/// the middle - and lets it down the same way from the other side.
#[test]
fn kerb_along_the_tread_lifts_smoothly() {
    let r = 0.47f32;
    let h = 0.15;
    let mut last: Option<f64> = None;
    let mut max_jump = 0.0f64;
    for k in 0..=200 {
        // the kerb's edge from 0.3 m right of the hub to 0.3 m left of it
        let e = 0.3 - k as f64 * 0.003;
        let probe = move |x: f64, _y: f64, top: f64| {
            let z = if x > e { h } else { 0.0 };
            if z <= top { GroundProbe { below: Some(z), above: None } } else { GroundProbe { below: None, above: Some(z) } }
        };
        let hub = DVec3::new(0.0, 0.0, r as f64);
        let z_top = (CLIMB * r) as f64;
        let line = tyre_line(&probe, hub, Vec3::Y, r, z_top, hub.z + WALL_HEIGHT, 0.0);
        let (need, _, _) = line.ground.expect("ground");
        let need = need + tread_step(&probe, hub, Vec3::Y, r, z_top);
        if let Some(l) = last {
            max_jump = max_jump.max((need - l).abs());
        }
        last = Some(need);
    }
    assert!((last.unwrap() - (r as f64 + h)).abs() < 1e-3, "ends on the kerb: {:?}", last);
    assert!(max_jump < 0.02, "a jump of {max_jump:.3} m");
}

/// On a uniform grade of any steepness the tyre reports the grade itself and stands where
/// the envelope touches the ramp, with no more probes than on the flat. The old comb read
/// every grade over 6.4 % as a run of kerbs: 0.043 for 6.5-9 %, 0.13 for 10-15 %, and 73
/// probes a wheel.
#[test]
fn tyre_reads_any_grade_as_a_grade() {
    let r = 0.47f32;
    let a = (r * ENVELOPE) as f64;
    for pct in [0.0, 2.0, 5.0, 6.3, 6.5, 8.0, 9.0, 10.0, 12.0, 15.0, 20.0, 25.0] {
        let s = pct / 100.0;
        let count = std::cell::Cell::new(0);
        let g = ramp(s);
        let probe = |x: f64, y: f64, top: f64| {
            count.set(count.get() + 1);
            g(x, y, top)
        };
        for k in 0..40 {
            let y = 100.0 + k as f64 * 0.037;
            let hub = DVec3::new(0.0, y, y * s + r as f64);
            let z_top = y * s + (CLIMB * r) as f64 + 0.05;
            count.set(0);
            let line = tyre_line(&probe, hub, Vec3::Y, r, z_top, hub.z + WALL_HEIGHT, 0.0);
            let (need, slope, x) = line.ground.expect("ground");
            // the envelope touches where its own slope is the grade
            let t = s * a / r as f64;
            let xs = a * t / (1.0 + t * t).sqrt();
            let want = s * (y + xs) + r as f64 * (1.0 - (xs / a).powi(2)).sqrt();
            assert!((slope as f64 - s).abs() < 1e-4, "{pct} %: slope {slope} at {y}");
            assert!((need - want).abs() < 1e-5, "{pct} %: need {need}, want {want}");
            assert!((x as f64 - xs).abs() < 1e-3, "{pct} %: contact at {x}, want {xs}");
            assert!(line.faces == [None, None] && line.clear);
            assert!(count.get() <= LATTICE, "{pct} %: {} probes", count.get());
        }
    }
}

/// Walked over a step or a chamfered face (on the level or on a grade), the push the
/// slope gives takes exactly what the hub is lifted by (∫ slope dx = Δneed), and the hub
/// never jumps as the samples move on: the old corner slope gave back only 0.43 of the
/// load for 0.6 of it on a steep face and pumped the body up.
#[test]
fn climbing_a_face_costs_what_it_lifts() {
    let r = 0.47f32;
    for (grade, height, angle) in [(0.0, 0.15, 90.0f64), (0.0, 0.2, 72.0), (0.0, 0.2, 60.0), (0.0, 0.1, 45.0), (0.0, 0.3, 80.0), (0.0, 0.05, 20.0), (0.0, 0.37, 90.0), (0.0, 0.37, 70.0), (0.1, 0.15, 90.0), (-0.08, 0.2, 75.0)] {
        let run = height / angle.to_radians().tan();
        let face = move |_x: f64, y: f64, top: f64| {
            let z = grade * y + ((y - 5.0) / run.max(1e-9)).clamp(0.0, 1.0) * height;
            if z <= top { GroundProbe { below: Some(z), above: None } } else { GroundProbe { below: Some(0.0), above: Some(z) } }
        };
        let need_at = |y: f64| {
            let hub = DVec3::new(0.3, y, r as f64 + grade * y);
            let top = hub.z - r as f64 + (CLIMB * r) as f64;
            tyre_line(&face, hub, Vec3::Y, r, top.max(grade * 5.0 + height), hub.z + WALL_HEIGHT, 0.0).ground.expect("ground")
        };
        let (y0, y1, dy) = (2.5, 7.0, 0.0005);
        let (mut work, mut y, mut need) = (0.0, y0, need_at(y0).0);
        while y < y1 {
            let slope = need_at(y + dy / 2.0).1 as f64;
            work += slope * dy;
            y += dy;
            let next = need_at(y).0;
            assert!((next - need - slope * dy).abs() < 1e-3, "{height} m at {angle}° on {grade}: the hub jumped by {} at {y}", next - need - slope * dy);
            need = next;
        }
        let lift = need_at(y).0 - need_at(y0).0;
        let want = height + grade * (y - y0);
        assert!((lift - want).abs() < 1e-3, "{height} m at {angle}° on {grade}: lifted {lift}, want {want}");
        assert!((work - lift).abs() < 0.003 * height.max(0.1), "{height} m at {angle}° on {grade}: pushed back {work} for {lift}");
    }
}

/// A step asks the ground for a few points per tyre - the lattice and a look to either
/// side - on the flat and on any grade alike (the comb read every grade over 6.4 % as a
/// run of kerbs and asked for 73 per tyre).
#[test]
fn a_step_asks_for_few_points() {
    for s in [0.0, 0.05, 0.08, 0.12, 0.2] {
        let count = std::cell::Cell::new(0);
        let g = ramp(s);
        let probe = |x: f64, y: f64, top: f64| {
            count.set(count.get() + 1);
            g(x, y, top)
        };
        let mut rb = RigidBody::from_definition(&bus(), &[]);
        rb.place(DVec3::new(0.0, 10.0, 10.0 * s), 0.0);
        run(&mut rb, 1.0, 0.0, 30_000.0, &probe);
        count.set(0);
        rb.step(1.0 / 60.0, 0.0, &[30_000.0; 4], 0.0, &probe);
        // (and three across the tread, `tread_step`, and the point under the wheel
        // Omsi.exe's suspension stands on)
        assert!(count.get() <= 4 * rb.wheels.len() * (LATTICE + 6), "{s}: {} probes", count.get());
    }
}

/// Released on grades from 4 % to 16 %, the bus rolls down with gravity's full pull along
/// the slope less the rolling resistance.
#[test]
fn rolls_down_a_grade_at_its_own_pace() {
    for pct in [4.0f64, 8.0, 12.0, 16.0] {
        let s = pct / 100.0;
        let g = ramp(s);
        let mut rb = RigidBody::from_definition(&bus(), &[]);
        rb.place(DVec3::ZERO, 0.0);
        run(&mut rb, 3.0, 0.0, 30_000.0, &g);
        run(&mut rb, 0.5, 0.0, 0.0, &g);
        let v0 = rb.forward_speed();
        run(&mut rb, 2.0, 0.0, 0.0, &g);
        let accel = (rb.forward_speed() - v0) / 2.0;
        let want = -(9.81 * s / (1.0 + s * s).sqrt()) as f32 + 1000.0 / rb.mass;
        assert!((accel - want).abs() < 0.04 * want.abs(), "{pct} %: {accel} m/s², want {want}");
    }
}

/// A step close to the tyre's radius (0.45 m on 0.47 m, which the hub-and-5-cm limit
/// still climbed) is a wall, whatever the push or the run-up; one of 0.3 m is climbed
/// with a run-up.
#[test]
fn a_step_near_the_radius_is_a_wall() {
    let step = road(6.0, 0.45);
    let (rb, front) = climb(&step, 9500.0, 10.0);
    assert!(front < 6.0 - 0.4, "front axle at {front}");
    assert!(rb.wheels[0].ground_z.abs() < 0.01, "on the step: {}", rb.wheels[0].ground_z);
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    rb.place(DVec3::ZERO, 0.0);
    run(&mut rb, 1.0, 0.0, 5000.0, &step);
    rb.velocity = Vec3::new(0.0, 15.0 / 3.6, 0.0);
    run(&mut rb, 5.0, 9500.0, 0.0, &step);
    let front = rb.origin().y + 3.238;
    assert!(front < 6.0 - 0.4, "front axle at {front} after a run-up");
    let low = road(6.0, 0.3);
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    rb.place(DVec3::ZERO, 0.0);
    run(&mut rb, 1.0, 0.0, 5000.0, &low);
    rb.velocity = Vec3::new(0.0, 15.0 / 3.6, 0.0);
    run(&mut rb, 5.0, 9500.0, 0.0, &low);
    assert!(rb.origin().y > 6.0, "stuck at {} before a 0.3 m step", rb.origin().y);
    assert!(rb.wheels.iter().all(|w| (w.ground_z - 0.3).abs() < 0.01), "{:?}", rb.wheels.iter().map(|w| w.ground_z).collect::<Vec<_>>());
}

/// A platform's face stops the front wheels at any speed and frame rate (the body has no
/// collision box here: a spline's faces are not in the collision world), without
/// throwing the bus about.
#[test]
fn a_face_stops_the_wheels_at_any_speed() {
    let wall = road(6.0, 0.6);
    for fps in [60.0f32, 30.0, 20.0] {
        for kmh in [10.0f32, 30.0, 50.0, 80.0] {
            let mut rb = RigidBody::from_definition(&bus(), &[]);
            rb.place(DVec3::new(0.0, -10.0, 0.0), 0.0);
            run(&mut rb, 1.0, 0.0, 5000.0, &wall);
            rb.velocity = Vec3::new(0.0, kmh / 3.6, 0.0);
            let kinetic = rb.kinetic_energy();
            let (mut front, mut top, mut crash) = (f64::MIN, f64::MIN, 0.0);
            for _ in 0..(fps * 6.0) as usize {
                rb.step(1.0 / fps, 0.0, &[0.0; 4], 0.0, &wall);
                front = front.max(rb.origin().y + 3.238);
                top = top.max(rb.origin().z);
                for hit in rb.wheel_impacts.iter().filter(|h| h.speed > CRASH_SPEED && h.energy > 1000.0) {
                    assert!(hit.obstacle < 2 && hit.point.y > 3.238 && hit.point.z < 1.1, "{hit:?}");
                    crash += hit.energy;
                }
            }
            // the crash is the wheels' (the bus has no box here), and it took most of the
            // bus's motion
            assert!(crash > 0.5 * kinetic && crash < 1.05 * kinetic, "{kmh} km/h at {fps} fps: crash of {crash} J for {kinetic} J");
            assert!(front < 6.0 - 0.47 + 0.03, "{kmh} km/h at {fps} fps: front axle reached {front}");
            assert!(top < 0.3, "{kmh} km/h at {fps} fps: thrown up to {top}");
            // (a rebound of a metre or so: with the moments of inertia on Omsi.exe's axes
            // the body's lower yaw inertia takes a little more of the blow back)
            assert!(rb.forward_speed().abs() < 1.5, "{kmh} km/h at {fps} fps: still at {}", rb.forward_speed());
        }
    }
}

/// Sliding sideways into a platform along the bus, the tyres stop at its face instead of
/// running under it.
#[test]
fn a_face_beside_the_tyres_stops_a_slide() {
    let platform = |x: f64, _y: f64, top: f64| {
        let z = if x >= 1.5 { 0.6 } else { 0.0 };
        if z <= top { GroundProbe { below: Some(z), above: None } } else { GroundProbe { below: Some(0.0), above: Some(z) } }
    };
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    rb.place(DVec3::ZERO, 0.0);
    run(&mut rb, 1.0, 0.0, 0.0, &platform);
    rb.velocity = Vec3::new(8.0, 0.0, 0.0);
    let mut right = f64::MIN;
    for _ in 0..120 {
        rb.step(1.0 / 30.0, 0.0, &[0.0; 4], 0.0, &platform);
        for w in rb.wheels.iter().filter(|w| w.attach.x > 0.0) {
            let hub = rb.position + rb.orientation.mul_vec3(w.attach - rb.cog).as_dvec3();
            right = right.max(hub.x);
        }
    }
    let limit = 1.5 - (HALF_WIDTH * 0.47) as f64;
    assert!(right < limit + 0.02, "a right hub reached {right} (face at 1.5)");
    assert!(rb.wheels.iter().filter(|w| w.attach.x > 0.0).all(|w| !w.walls.is_empty()), "{:?}", rb.wheels.iter().map(|w| &w.walls).collect::<Vec<_>>());
    let (_, _, bank) = rb.heading_pitch_bank();
    assert!(bank.abs() < 5.0, "banked {bank}");
}

/// On an 8 % slope the parking brake (rear wheels only) holds the bus; released, it rolls
/// back down; and a driven bus brakes to a stand on the flat.
#[test]
fn holds_on_a_slope_and_brakes_to_a_stand() {
    let slope = |_x: f64, y: f64, top: f64| {
        let z = y * 0.08;
        if z <= top { GroundProbe { below: Some(z), above: None } } else { GroundProbe { below: None, above: Some(z) } }
    };
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    rb.place(DVec3::ZERO, 0.0);
    let park = [0.0, 0.0, 20_000.0, 20_000.0];
    for _ in 0..180 {
        rb.step(1.0 / 60.0, 0.0, &park, 0.0, &slope);
    }
    let y0 = rb.origin().y;
    for _ in 0..600 {
        rb.step(1.0 / 60.0, 0.0, &park, 0.0, &slope);
    }
    let y1 = rb.origin().y;
    for _ in 0..600 {
        rb.step(1.0 / 60.0, 0.0, &park, 0.0, &slope);
    }
    eprintln!("parked on 8 %: {:+.4} m in the first 10 s, {:+.4} m in the next", y1 - y0, rb.origin().y - y1);
    assert!((rb.origin().y - y1).abs() < 0.005, "crept {} m", rb.origin().y - y1);
    for _ in 0..180 {
        rb.step(1.0 / 60.0, 0.0, &[0.0; 4], 0.0, &slope);
    }
    assert!(rb.origin().y < y0 - 1.0 && rb.forward_speed() < -0.5, "rolled back to {} at {}", rb.origin().y, rb.forward_speed());

    let flat = road(1e9, 0.0);
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    rb.place(DVec3::ZERO, 0.0);
    run(&mut rb, 0.5, 0.0, 0.0, &flat);
    rb.velocity = Vec3::new(0.0, 50.0 / 3.6, 0.0);
    let mut t = 0.0;
    while rb.forward_speed() > 0.01 && t < 10.0 {
        rb.step(1.0 / 60.0, 0.0, &[15_000.0; 4], 0.0, &flat);
        t += 1.0 / 60.0;
    }
    // 60 kN on 10.2 t: about 5.9 m/s², so 50 km/h are gone in some 2.4 s
    assert!(t > 2.0 && t < 3.0, "stopped after {t} s");
    let y = rb.origin().y;
    run(&mut rb, 3.0, 0.0, 15_000.0, &flat);
    assert!((rb.origin().y - y).abs() < 0.02, "moved {} m after stopping", rb.origin().y - y);
}

/// The same full braking from 50 km/h on a dry road and on black ice: the tyres cannot
/// pass the brakes' 5.9 m/s² on to the ice, and the bus slides about five times as far.
#[test]
fn brakes_slide_on_ice() {
    let stop = |mu: f32| {
        let flat = road(1e9, 0.0);
        let mut rb = RigidBody::from_definition(&bus(), &[]);
        rb.place(DVec3::ZERO, 0.0);
        run(&mut rb, 0.5, 0.0, 0.0, &flat);
        rb.friction = mu;
        rb.velocity = Vec3::new(0.0, 50.0 / 3.6, 0.0);
        let y0 = rb.origin().y;
        let mut t = 0.0;
        while rb.forward_speed() > 0.01 && t < 30.0 {
            rb.step(1.0 / 60.0, 0.0, &[15_000.0; 4], 0.0, &flat);
            t += 1.0 / 60.0;
        }
        rb.origin().y - y0
    };
    let (dry, ice) = (stop(crate::vehicle::road_grip(0.0, 15.0)), stop(crate::vehicle::road_grip(0.4, -6.0)));
    eprintln!("50 km/h to a stand: {dry:.1} m dry, {ice:.1} m on ice");
    assert!(dry > 12.0 && dry < 20.0, "dry {dry}");
    assert!(ice > dry * 4.0, "ice {ice} dry {dry}");
}

/// Straight into a wall at 40 km/h: the bus stops at the wall, bounces off a little, and
/// drives away backwards straight after.
#[test]
fn crash_bounces_and_releases() {
    let def = bus();
    let bb = def.bounding_box.unwrap();
    let mut rb = RigidBody::from_definition(&def, &[]);
    rb.place(DVec3::ZERO, 0.0);
    let g = road(1e9, 0.0);
    run(&mut rb, 1.0, 0.0, 5000.0, &g);
    rb.velocity = Vec3::new(0.0, 40.0 / 3.6, 0.0);
    let wall = [Obb::from_box([20.0, 1.0, 3.0, 0.0, 0.0, 1.5], DVec3::new(0.0, 12.0, 0.0), 0.0)];
    let mut impacts = Vec::new();
    for _ in 0..90 {
        rb.step(1.0 / 30.0, 0.0, &[0.0; 4], 0.0, &g);
        impacts.extend(rb.collide(bb, &wall, &|_| false, 1.0 / 30.0));
    }
    assert_eq!(impacts.iter().filter(|i| i.speed > CRASH_SPEED).count(), 1, "{impacts:?}");
    let hit = impacts[0];
    assert!(hit.speed > 10.0 && hit.energy > 400_000.0, "{hit:?}");
    assert!(hit.point.y > 5.5, "{hit:?}");
    assert!(rb.forward_speed() < 0.0, "bounced: {}", rb.forward_speed());
    let front = rb.origin().y + 5.83;
    assert!(front < 11.5 + 0.01, "{front}");
    // reverse away at once
    let y0 = rb.origin().y;
    for _ in 0..60 {
        rb.step(1.0 / 30.0, -8000.0, &[0.0; 4], 0.0, &g);
        rb.collide(bb, &wall, &|_| false, 1.0 / 30.0);
    }
    assert!(rb.origin().y < y0 - 1.0, "{} -> {}", y0, rb.origin().y);
}

/// A glancing blow on the front corner turns the bus away from the wall and lets it slide.
#[test]
fn glancing_blow_turns_the_bus() {
    let def = bus();
    let bb = def.bounding_box.unwrap();
    let mut rb = RigidBody::from_definition(&def, &[]);
    rb.place(DVec3::ZERO, 0.0);
    let g = road(1e9, 0.0);
    run(&mut rb, 1.0, 0.0, 5000.0, &g);
    // heading 20° to the right towards a wall along x = 4
    rb.place(DVec3::ZERO, 20.0);
    run(&mut rb, 0.5, 0.0, 0.0, &g);
    rb.velocity = rb.orientation.mul_vec3(Vec3::Y) * 30.0 / 3.6;
    let wall = [Obb::from_box([1.0, 60.0, 3.0, 0.0, 0.0, 1.5], DVec3::new(4.5, 10.0, 0.0), 0.0)];
    let mut hits = 0;
    for _ in 0..60 {
        rb.step(1.0 / 30.0, 0.0, &[0.0; 4], 0.0, &g);
        hits += rb.collide(bb, &wall, &|_| false, 1.0 / 30.0).len();
    }
    let (heading, _, _) = rb.heading_pitch_bank();
    assert!(hits > 0);
    assert!(heading < 15.0 || heading > 300.0, "turned away from the wall: {heading}");
    assert!(rb.velocity.y > 1.0, "still sliding along: {:?}", rb.velocity);
}

/// A car (1 t, kinematic: it keeps its speed) drives into the side of the standing bus
/// at 50 km/h: the bus gets a nudge and a crash, not a shove across the road.
#[test]
fn a_car_does_not_bulldoze_the_bus() {
    let def = bus();
    let bb = def.bounding_box.unwrap();
    let mut rb = RigidBody::from_definition(&def, &[]);
    rb.place(DVec3::ZERO, 0.0);
    let g = road(1e9, 0.0);
    run(&mut rb, 1.0, 0.0, 5000.0, &g);
    let (x0, mut hits, mut energy) = (rb.origin().x, 0, 0.0f32);
    for k in 0..30 {
        // from the left, heading east
        let car = Obb::from_box([1.7, 4.2, 1.4, 0.0, 0.0, 0.7], DVec3::new(-4.0 + 50.0 / 3.6 * k as f64 / 30.0, 0.0, 0.0), 90.0).moving(glam::DVec2::new(50.0 / 3.6, 0.0), 1000.0, 7);
        rb.step(1.0 / 30.0, 0.0, &[5000.0; 4], 0.0, &g);
        for hit in rb.collide(bb, &[car], &|_| false, 1.0 / 30.0) {
            hits += 1;
            energy += hit.energy;
        }
    }
    run(&mut rb, 2.0, 0.0, 5000.0, &g);
    let (_, _, bank) = rb.heading_pitch_bank();
    assert!(hits > 0 && energy > 20_000.0, "{hits} hits, {energy} J");
    assert!((rb.origin().x - x0).abs() < 1.0, "shoved {} m", rb.origin().x - x0);
    assert!(bank.abs() < 3.0, "banked {bank}");
}

/// A car driving into the standing bus is one crash per call, however long it stays
/// inside the bus: 0.5 v² (1 - e²) over the pair's inverse mass, not four times that.
#[test]
fn a_car_strike_counts_once() {
    let def = bus();
    let bb = def.bounding_box.unwrap();
    let mut rb = RigidBody::from_definition(&def, &[]);
    rb.place(DVec3::ZERO, 0.0);
    let g = road(1e9, 0.0);
    run(&mut rb, 1.0, 0.0, 5000.0, &g);
    let v = 50.0f32 / 3.6;
    let car = Obb::from_box([1.7, 4.2, 1.4, 0.0, 0.0, 0.7], DVec3::new(-2.0, 0.0, 0.0), 90.0).moving(glam::DVec2::new(v as f64, 0.0), 1000.0, 7);
    let hits = rb.collide(bb, &[car], &|_| false, 1.0 / 30.0);
    assert_eq!(hits.len(), 1, "{hits:?}");
    let n = Vec3::X;
    let r = Vec3::new(-1.236, 0.0, 0.0);
    let k = rb.inv_mass_at(r, n) + 1.0 / 1000.0;
    let want = 0.5 * v * v * (1.0 - RESTITUTION * RESTITUTION) / k;
    assert!((hits[0].energy - want).abs() < 0.15 * want, "{} J, want about {want}", hits[0].energy);
    assert!(hits[0].energy < 100_000.0, "{hits:?}");
}

#[test]
fn moving_car_overlap_does_not_bulldoze_bus_and_clears_when_receding() {
    let def = bus();
    let bb = def.bounding_box.unwrap();
    let mut rb = RigidBody::from_definition(&def, &[]);
    rb.place(DVec3::ZERO, 0.0);
    let g = road(1e9, 0.0);
    run(&mut rb, 1.0, 0.0, 5000.0, &g);
    let car = Obb::from_box(
        [1.7, 4.2, 1.4, 0.0, 0.0, 0.7],
        DVec3::new(-3.2, 0.0, 0.0),
        90.0,
    )
    .moving(glam::DVec2::new(8.0, 0.0), 1000.0, 7);
    assert!(rb.body_box(bb).overlaps(&car));

    let impacts = rb.collide(bb, &[car], &|_| false, 1.0 / 30.0);

    assert_eq!(impacts.len(), 1);
    assert!(rb.body_box(bb).overlaps(&car));
    assert!(rb.origin().x > 0.0 && rb.origin().x < 0.02, "{}", rb.origin().x);
    let receding = Obb {
        center: car.center - glam::DVec2::new(0.5, 0.0),
        velocity: glam::DVec2::new(-8.0, 0.0),
        ..car
    };
    rb.collide(bb, &[receding], &|_| false, 1.0 / 30.0);
    assert!(!rb.body_box(bb).overlaps(&receding));
}

/// An AI car that creeps into the standing bus (below the crash speed, its planner blind
/// to the bus) does not shove it along at its own pace: the bus only leaves an overlap
/// as far as it moved into it itself, as it did before the moving-car change.
#[test]
fn a_creeping_car_does_not_shove_the_standing_bus() {
    let def = bus();
    let bb = def.bounding_box.unwrap();
    let mut rb = RigidBody::from_definition(&def, &[]);
    rb.place(DVec3::ZERO, 0.0);
    let g = road(1e9, 0.0);
    run(&mut rb, 1.0, 0.0, 5000.0, &g);
    let x0 = rb.origin().x;
    let mut car = Obb::from_box([1.7, 4.2, 1.4, 0.0, 0.0, 0.7], DVec3::new(-3.3, 0.0, 0.0), 90.0)
        .moving(glam::DVec2::new(0.3, 0.0), 1000.0, 7);
    for _ in 0..60 {
        car.center += car.velocity / 30.0;
        rb.collide(bb, &[car], &|_| false, 1.0 / 30.0);
    }
    assert!(rb.origin().x - x0 < 0.1, "the bus was shoved {:.2} m", rb.origin().x - x0);
}

/// The scripts hear where the bodies meet - low down at the bumpers - not the height of
/// the centre of gravity: reversing into a wall or a car is below 1.10 m and behind
/// -4.70 m, where the stock buses damage their engine; a high beam is hit high.
#[test]
fn impacts_are_reported_at_bumper_height() {
    let def = bus();
    let bb = def.bounding_box.unwrap();
    let hit = |obstacle: Obb, v: Vec3| {
        let mut rb = RigidBody::from_definition(&def, &[]);
        rb.place(DVec3::ZERO, 0.0);
        let g = road(1e9, 0.0);
        run(&mut rb, 1.0, 0.0, 5000.0, &g);
        rb.velocity = v;
        for _ in 0..60 {
            rb.step(1.0 / 30.0, 0.0, &[0.0; 4], 0.0, &g);
            if let Some(h) = rb.collide(bb, &[obstacle], &|_| false, 1.0 / 30.0).into_iter().find(|h| h.speed > CRASH_SPEED) {
                return h;
            }
        }
        panic!("no hit on {obstacle:?}");
    };
    let back = Vec3::new(0.0, -15.0 / 3.6, 0.0);
    let wall = hit(Obb::from_box([20.0, 1.0, 5.0, 0.0, 0.0, 2.5], DVec3::new(0.0, -8.0, 0.0), 0.0), back);
    assert!(wall.point.z < 1.1 && wall.point.z > 0.5 && wall.point.y < -4.7, "{wall:?}");
    let car = hit(Obb::from_box([1.7, 4.2, 1.4, 0.0, 0.0, 0.7], DVec3::new(0.0, -9.0, 0.0), 0.0).moving(glam::DVec2::ZERO, 1000.0, 3), back);
    assert!(car.point.z < 1.1 && car.point.y < -4.7, "{car:?}");
    let front = hit(Obb::from_box([20.0, 1.0, 5.0, 0.0, 0.0, 2.5], DVec3::new(0.0, 8.0, 0.0), 0.0), -back);
    assert!(front.point.z < 1.1 && front.point.y > 4.7, "{front:?}");
    let beam = hit(Obb::from_box([20.0, 1.0, 1.0, 0.0, 0.0, 2.5], DVec3::new(0.0, 8.0, 0.0), 0.0), -back);
    assert!(beam.point.z > 2.0, "{beam:?}");
}

/// Into the back of a standing car at 30 km/h: the car (1 t, which the AI holds where it
/// is) stops the bus within a few metres instead of being driven through.
#[test]
fn rear_ends_a_standing_car() {
    let def = bus();
    let bb = def.bounding_box.unwrap();
    let mut rb = RigidBody::from_definition(&def, &[]);
    rb.place(DVec3::ZERO, 0.0);
    let g = road(1e9, 0.0);
    run(&mut rb, 1.0, 0.0, 5000.0, &g);
    rb.velocity = Vec3::new(0.0, 30.0 / 3.6, 0.0);
    let car = [Obb::from_box([1.7, 4.2, 1.4, 0.0, 0.0, 0.7], DVec3::new(0.0, 12.0, 0.0), 0.0).moving(glam::DVec2::ZERO, 1000.0, 3)];
    let mut first = None;
    for k in 0..90 {
        rb.step(1.0 / 30.0, 0.0, &[0.0; 4], 0.0, &g);
        if !rb.collide(bb, &car, &|_| false, 1.0 / 30.0).is_empty() && first.is_none() {
            first = Some((k, rb.origin().y));
        }
    }
    let (_, y_hit) = first.expect("hit the car");
    let front = rb.origin().y + 5.83;
    assert!(rb.forward_speed().abs() < 0.3, "still at {} km/h", rb.forward_speed() * 3.6);
    assert!(front < 12.0 - 2.1 + 0.3, "drove into the car: front at {front}");
    assert!(rb.origin().y - y_hit < 3.0, "pushed on {} m", rb.origin().y - y_hit);
}

/// A lamp post breaks off: the bus hardly slows down.
#[test]
fn a_post_breaks_off() {
    let def = bus();
    let bb = def.bounding_box.unwrap();
    let mut rb = RigidBody::from_definition(&def, &[]);
    rb.place(DVec3::ZERO, 0.0);
    let g = road(1e9, 0.0);
    run(&mut rb, 1.0, 0.0, 5000.0, &g);
    rb.velocity = Vec3::new(0.0, 30.0 / 3.6, 0.0);
    let mut post = Obb::from_box([0.2, 0.2, 6.0, 0.0, 0.0, 3.0], DVec3::new(0.5, 10.0, 0.0), 0.0);
    post.pole = Some((0.05, 0.5));
    let mut broke = false;
    for _ in 0..30 {
        rb.step(1.0 / 30.0, 0.0, &[0.0; 4], 0.0, &g);
        let hits = rb.collide(bb, &[post], &|_| broke, 1.0 / 30.0);
        broke |= hits.iter().any(|h| h.broke);
    }
    assert!(broke);
    assert!(rb.forward_speed() > 25.0 / 3.6, "{}", rb.forward_speed() * 3.6);
}


/// A drive torque, steering or brake force that is no number (a script's 0/0) is taken
/// as none: the body keeps its place and speed instead of turning to NaN (#1045).
#[test]
fn inputs_that_are_no_number_leave_the_body_whole() {
    for (torque, brake, steer) in [(f32::NAN, 0.0, 0.0), (3000.0, f32::INFINITY, 0.0), (3000.0, f32::NAN, 0.0), (3000.0, 0.0, f32::NAN), (f32::NEG_INFINITY, 0.0, 0.0)] {
        let mut rb = RigidBody::from_definition(&bus(), &[]);
        rb.place(DVec3::ZERO, 0.0);
        let g = road(1e9, 0.0);
        run(&mut rb, 1.0, 3000.0, 0.0, &g);
        for _ in 0..60 {
            rb.step(1.0 / 60.0, torque, &[brake; 4], steer, &g);
        }
        assert!(rb.position.is_finite() && rb.velocity.is_finite(), "{torque} {brake} {steer}: {:?} {:?}", rb.position, rb.velocity);
        assert!(rb.forward_speed() > 0.1, "{torque} {brake} {steer}: {}", rb.forward_speed());
        assert!(rb.wheels.iter().all(|w| w.compression.is_finite() && w.spin.is_finite()));
    }
    // a body whose own values are no number stays put
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    rb.place(DVec3::ZERO, 0.0);
    let g = road(1e9, 0.0);
    run(&mut rb, 0.5, 0.0, 0.0, &g);
    let at = rb.position;
    rb.mass = f32::NAN;
    rb.step(1.0 / 60.0, 3000.0, &[0.0; 4], 0.0, &g);
    assert!(rb.position.is_finite() && (rb.position - at).length() < 1e-9, "{:?}", rb.position);
    assert_eq!(rb.velocity, Vec3::ZERO);
}

/// The body moves alike at 60 and at 240 frames a second (one substep a frame there, four
/// at 60): it sags as far, pulls away as far, rides a hump as calmly, brakes to a stand on
/// the same spot, and at 240 frames stands still on a slope with its parking brake.
#[test]
fn moves_alike_at_any_frame_rate() {
    let hump = |_x: f64, y: f64, z_top: f64| {
        let z = if (30.0..30.6).contains(&y) { 0.06 } else { 0.0 };
        if z <= z_top { GroundProbe { below: Some(z), above: None } } else { GroundProbe { below: Some(0.0), above: Some(z) } }
    };
    let drive = |fps: f32| {
        let dt = 1.0 / fps;
        let n = |secs: f32| (secs * fps).round() as usize;
        let mut rb = RigidBody::from_definition(&bus(), &[]);
        rb.place(DVec3::ZERO, 0.0);
        for _ in 0..n(2.0) {
            rb.step(dt, 0.0, &[5000.0; 4], 0.0, &hump);
        }
        let sag = rb.origin().z;
        let (z0, mut lift) = (rb.position.z, 0.0f64);
        for _ in 0..n(8.0) {
            rb.step(dt, 12_000.0, &[0.0; 4], 0.0, &hump);
            lift = lift.max(rb.position.z - z0);
        }
        let (pulled, speed) = (rb.origin().y, rb.forward_speed());
        let mut t = 0.0;
        while rb.forward_speed() > 0.01 && t < 10.0 {
            rb.step(dt, 0.0, &[15_000.0; 4], 0.0, &hump);
            t += dt;
        }
        (sag, pulled, speed, lift, rb.origin().y, t)
    };
    let (a, b) = (drive(60.0), drive(240.0));
    eprintln!("60 fps {a:?}\n240 fps {b:?}");
    assert!((a.0 - b.0).abs() < 0.003, "sag {} / {}", a.0, b.0);
    assert!(a.1 > 30.6 && (a.1 - b.1).abs() < 0.01 * a.1, "pulled away {} / {} m", a.1, b.1);
    assert!((a.2 - b.2).abs() < 0.01 * a.2, "speed {} / {}", a.2, b.2);
    assert!(b.3 < 0.05 && b.3 < a.3 + 0.01, "the body lifted {} / {} m", a.3, b.3);
    assert!((a.4 - b.4).abs() < 0.25, "stood at {} / {} m", a.4, b.4);
    assert!((a.5 - b.5).abs() < 0.1, "stopped after {} / {} s", a.5, b.5);

    let slope = |_x: f64, y: f64, top: f64| {
        let z = y * 0.08;
        if z <= top { GroundProbe { below: Some(z), above: None } } else { GroundProbe { below: None, above: Some(z) } }
    };
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    rb.place(DVec3::ZERO, 0.0);
    let park = [0.0, 0.0, 20_000.0, 20_000.0];
    for _ in 0..240 * 5 {
        rb.step(1.0 / 240.0, 0.0, &park, 0.0, &slope);
    }
    let y = rb.origin().y;
    for _ in 0..240 * 5 {
        rb.step(1.0 / 240.0, 0.0, &park, 0.0, &slope);
    }
    assert!((rb.origin().y - y).abs() < 0.005, "crept {} m at 240 fps", rb.origin().y - y);
}

/// A body of the realistic road model (the setting on).
fn realistic_body() -> RigidBody {
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    rb.realistic = true;
    rb
}

/// A bus at `kmh` over a flat road of `surface` for `secs`: the RMS of the body's vertical
/// acceleration (m/s²) once it has settled.
fn vertical_shake(surface: u8, kmh: f32, secs: f32) -> f32 {
    let mut rb = realistic_body();
    rb.place(DVec3::ZERO, 0.0);
    let g = road(1e9, 0.0);
    run(&mut rb, 2.0, 0.0, 0.0, &g);
    let v = kmh / 3.6;
    rb.velocity = rb.orientation.mul_vec3(Vec3::Y) * v;
    for w in rb.wheels.iter_mut() {
        w.spin = v / w.radius;
        w.surface = surface;
        w.surface_now = surface;
        w.surface_was = surface;
    }
    let radius = rb.wheels.iter().find(|w| w.driven).unwrap().radius;
    let mut samples = Vec::new();
    for i in 0..(secs * 120.0) as usize {
        // hold the speed so the roughness is driven over at the same pace throughout
        let torque = ((v - rb.forward_speed()) * rb.mass * 2.0 + rb.rolling_resistance) * radius;
        rb.step(1.0 / 120.0, torque, &[0.0; 4], 0.0, &g);
        if i > 120 {
            // (the body's acceleration reads 9.81 standing: what shakes is the deviation)
            samples.push(rb.accel_body.z as f64);
        }
    }
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    (samples.iter().map(|a| (a - mean).powi(2)).sum::<f64>() / samples.len() as f64).sqrt() as f32
}

/// Cobbles, a gravel road and a slab road shake a bus - the "road damage has no impact"
/// of #3069 - where asphalt hardly does, and a faster bus is shaken harder.
#[test]
fn a_rough_road_shakes_the_bus() {
    let asphalt = vertical_shake(0, 40.0, 8.0);
    let slabs = vertical_shake(1, 40.0, 8.0);
    let cobbles = vertical_shake(2, 40.0, 8.0);
    let gravel = vertical_shake(5, 40.0, 8.0);
    eprintln!("vertical shake at 40 km/h (m/s² rms): asphalt {asphalt:.3}, slabs {slabs:.3}, cobbles {cobbles:.3}, gravel {gravel:.3}");
    assert!(asphalt < 0.15, "asphalt {asphalt}");
    assert!(slabs > asphalt * 1.5, "slabs {slabs} asphalt {asphalt}");
    assert!(cobbles > asphalt * 2.5, "cobbles {cobbles} asphalt {asphalt}");
    assert!(gravel > cobbles, "gravel {gravel} cobbles {cobbles}");
    // the bus's own springs keep a dirt road within what a passenger bears (ISO 2631: a
    // bus on a poor road is "a little uncomfortable" to "uncomfortable": 0.5-1.6 m/s²)
    assert!(gravel < 3.0, "gravel {gravel}");
    let fast = vertical_shake(2, 60.0, 8.0);
    assert!(fast > cobbles, "the faster, the harder: {cobbles} at 40 km/h, {fast} at 60 (asphalt {asphalt}, slabs {slabs}, gravel {gravel})");
}

/// A bus brakes harder on dry asphalt than on grass or in the snow, with the same brakes.
#[test]
fn the_surface_decides_how_far_a_braking_bus_goes() {
    let stop = |surface: u8, wet: f32| {
        let mut rb = realistic_body();
        rb.place(DVec3::ZERO, 0.0);
        // the road's grip in the weather, as `road_grip` gives it
        rb.friction = crate::vehicle::weather::road_grip(wet, 15.0);
        rb.wetness = wet;
        let g = road(1e9, 0.0);
        run(&mut rb, 2.0, 0.0, 0.0, &g);
        rb.velocity = rb.orientation.mul_vec3(Vec3::Y) * 16.0;
        for w in rb.wheels.iter_mut() {
            w.spin = 16.0 / w.radius;
            w.surface = surface;
            w.surface_now = surface;
            w.surface_was = surface;
        }
        let y0 = rb.position.y;
        // a brake the dry road takes, held
        let brakes = vec![30_000.0; rb.wheels.len()];
        for _ in 0..(12.0 * 120.0) as usize {
            rb.step(1.0 / 120.0, 0.0, &brakes, 0.0, &g);
        }
        (rb.position.y - y0) as f32
    };
    let dry = stop(0, 0.0);
    let wet = stop(0, 1.0);
    let cobbles_wet = stop(2, 1.0);
    let grass = stop(4, 0.0);
    assert!(dry < wet && wet < cobbles_wet, "dry {dry}, wet {wet}, wet cobbles {cobbles_wet}, grass {grass}");
    // 16 m/s at 0.85 g would stop in 15 m, on grass (0.5 x 0.85) in 30 m or more
    assert!(dry > 14.0 && dry < 22.0, "dry {dry}");
    assert!(grass > dry * 1.5, "grass {grass}");
}

/// The vehicle's scripts already put the still-air drag on its brakes (bremse.osc), so the body
/// adds only what the weather changes: a headwind slows a coasting bus more, a tailwind less, a
/// denser air more, and the air of the scripts nothing at all.
#[test]
fn the_weather_changes_the_drag_the_scripts_give() {
    let coast = |density: f32, wind: Vec3| {
        let mut rb = realistic_body();
        rb.place(DVec3::ZERO, 0.0);
        rb.air_density = density;
        rb.wind = wind;
        let g = road(1e9, 0.0);
        run(&mut rb, 2.0, 0.0, 0.0, &g);
        rb.velocity = rb.orientation.mul_vec3(Vec3::Y) * 25.0;
        for w in rb.wheels.iter_mut() {
            w.spin = 25.0 / w.radius;
        }
        run(&mut rb, 10.0, 0.0, 0.0, &g);
        rb.forward_speed()
    };
    let calm = coast(realism::SCRIPT_DENSITY, Vec3::ZERO);
    // (only the rolling resistance of the file slows it: 1 kN on 10 t for 10 s)
    assert!(calm > 23.5 && calm < 25.0, "{calm}");
    assert!(coast(1.4, Vec3::ZERO) < calm, "a denser air takes more");
    assert!(coast(realism::SCRIPT_DENSITY, Vec3::new(0.0, -12.0, 0.0)) < calm - 0.5, "a headwind takes more");
    assert!(coast(realism::SCRIPT_DENSITY, Vec3::new(0.0, 12.0, 0.0)) > calm, "a tailwind takes less");
}

/// The OMSI 2 model (the setting off) is as it was: a bus follows its wheels at any speed
/// short of the grip, asphalt is smooth, and the old drag (the mass-guessed area) acts.
#[test]
fn without_the_setting_the_body_is_omsi_s() {
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    assert!(!rb.realistic, "off by default");
    rb.place(DVec3::ZERO, 0.0);
    for w in rb.wheels.iter_mut() {
        w.surface = 2;
    }
    let g = road(1e9, 0.0);
    run(&mut rb, 2.0, 0.0, 0.0, &g);
    assert!(rb.wheels.iter().all(|w| w.rough == 0.0 && w.mu_scale == 1.0), "no road unevenness or surface grip");
    rb.wind = Vec3::new(-25.0, 0.0, 0.0);
    let before = rb.velocity;
    run(&mut rb, 1.0, 0.0, 0.0, &g);
    assert!((rb.velocity - before).length() < 0.05, "no wind");
    // a bend follows the wheels at 50 km/h, instantly
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    rb.place(DVec3::ZERO, 0.0);
    run(&mut rb, 2.0, 0.0, 0.0, &g);
    rb.velocity = rb.orientation.mul_vec3(Vec3::Y) * 13.9;
    for w in rb.wheels.iter_mut() {
        w.spin = 13.9 / w.radius;
    }
    let radius = rb.wheels.iter().find(|w| w.driven).unwrap().radius;
    let mut yaw = 0.0;
    for i in 0..480 {
        let torque = ((13.9 - rb.forward_speed()) * rb.mass * 2.0 + rb.rolling_resistance) * radius;
        rb.step(1.0 / 60.0, torque, &[0.0; 4], 0.15, &g);
        if i >= 420 {
            yaw += rb.omega.z.abs() / 60.0;
        }
    }
    let ratio = yaw / (rb.forward_speed() * 0.15 * rb.inv_min_turn_radius);
    assert!((ratio - 1.0).abs() < 0.03, "{ratio}");
}

/// A tyre that leaves the ground or is put down anew starts without a lagging contact patch.
#[test]
fn a_tyre_in_the_air_forgets_its_patch() {
    let mut rb = realistic_body();
    rb.place(DVec3::ZERO, 0.0);
    rb.wheels[0].lat_defl = 0.05;
    rb.place(DVec3::new(10.0, 0.0, 0.0), 0.0);
    assert_eq!(rb.wheels[0].lat_defl, 0.0, "put down anew");
    let g = road(1e9, 0.0);
    run(&mut rb, 1.0, 0.0, 0.0, &g);
    rb.wheels[1].lat_defl = 0.04;
    // (no ground at all: every tyre is in the air)
    let air = |_x: f64, _y: f64, _t: f64| GroundProbe { below: None, above: None };
    rb.step(1.0 / 60.0, 0.0, &[0.0; 4], 0.0, &air);
    assert_eq!(rb.wheels[1].lat_defl, 0.0);
}

/// A gale across the road pushes a standing bus along the wind and leans it.
#[test]
fn a_crosswind_leans_and_pushes_the_bus() {
    let mut rb = realistic_body();
    rb.place(DVec3::ZERO, 0.0);
    // blowing towards -x (to the bus's left)
    rb.wind = Vec3::new(-25.0, 0.0, 0.0);
    let g = road(1e9, 0.0);
    run(&mut rb, 3.0, 0.0, 0.0, &g);
    let (_, _, bank) = rb.heading_pitch_bank();
    let mut calm = realistic_body();
    calm.place(DVec3::ZERO, 0.0);
    run(&mut calm, 3.0, 0.0, 0.0, &g);
    let (_, _, calm_bank) = calm.heading_pitch_bank();
    // a bus on its tyres does not blow away, but its body leans from the gale
    assert!((bank - calm_bank).abs() > 0.05, "no lean: {bank} against {calm_bank}");
    assert!(rb.velocity.length() < 0.5, "a standing bus holds its place: {:?}", rb.velocity);
}

/// The yaw rate the bus settles to in a steady bend of `steer` at `kmh`, against the
/// geometry's own (speed times the curvature the steering asks for): 1 is a bus that
/// follows its wheels.
fn yaw_ratio(kmh: f32, steer: f32, fps: f32) -> f32 {
    let mut rb = realistic_body();
    rb.place(DVec3::ZERO, 0.0);
    let g = road(1e9, 0.0);
    run(&mut rb, 2.0, 0.0, 0.0, &g);
    let v = kmh / 3.6;
    rb.velocity = rb.orientation.mul_vec3(Vec3::Y) * v;
    for w in rb.wheels.iter_mut() {
        w.spin = v / w.radius;
    }
    let radius = rb.wheels.iter().find(|w| w.driven).unwrap().radius;
    let dt = 1.0 / fps;
    let mut yaw = 0.0;
    let n = (8.0 / dt) as usize;
    for i in 0..n {
        let torque = ((v - rb.forward_speed()) * rb.mass * 2.0 + rb.rolling_resistance) * radius;
        rb.step(dt, torque, &[0.0; 4], steer, &g);
        if i + (fps as usize) >= n {
            yaw += rb.omega.z.abs() / fps;
        }
    }
    yaw / (rb.forward_speed() * steer * rb.inv_min_turn_radius).max(1e-6)
}

/// A slip-angle tyre needs slip to make a side force: a bus follows its wheels at a walking
/// pace and understeers as it goes faster, and the same at any frame rate.
#[test]
fn a_bus_understeers_as_it_speeds_up() {
    let slow = yaw_ratio(10.0, 0.15, 60.0);
    let mid = yaw_ratio(40.0, 0.15, 60.0);
    let fast = yaw_ratio(70.0, 0.15, 60.0);
    assert!(slow > 0.97 && slow < 1.03, "walking pace: {slow}");
    assert!(mid < slow && fast < mid, "{slow} {mid} {fast}");
    assert!(mid > 0.8 && fast > 0.5, "{mid} {fast}");
    for fps in [30.0, 120.0] {
        let r = yaw_ratio(40.0, 0.15, fps);
        assert!((r - mid).abs() < 0.04, "{fps} fps: {r} against {mid}");
    }
}

/// A bus on a road that falls away sideways stays where it is: the tyres hold it across.
#[test]
fn the_tyres_hold_a_parked_bus_on_a_cross_slope() {
    let slope = |grade: f64| move |x: f64, _y: f64, top: f64| {
        let z = -grade * x;
        if z <= top { GroundProbe { below: Some(z), above: None } } else { GroundProbe { below: Some(z - 1.0), above: Some(z) } }
    };
    for (grade, holds) in [(0.08, true), (0.3, true)] {
        let mut rb = realistic_body();
        rb.place(DVec3::ZERO, 0.0);
        let g = slope(grade);
        let brakes = vec![3000.0; rb.wheels.len()];
        for _ in 0..(6.0 * 60.0) as usize {
            rb.step(1.0 / 60.0, 0.0, &brakes, 0.0, &g);
        }
        let moved = rb.position.x.abs();
        if holds {
            assert!(moved < 0.5, "slid {moved} m on {grade}");
        } else {
            assert!(moved > 1.0, "held on {grade}");
        }
    }
}

/// What the realistic road model costs a step: `cargo test -p omsi-sim --release physics_cost -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement, not a check"]
fn physics_cost() {
    let g = road(1e9, 0.0);
    for realistic in [false, true] {
        let mut rb = RigidBody::from_definition(&bus(), &[]);
        rb.realistic = realistic;
        rb.place(DVec3::ZERO, 0.0);
        run(&mut rb, 2.0, 0.0, 0.0, &g);
        rb.velocity = rb.orientation.mul_vec3(Vec3::Y) * 14.0;
        let steps = 20_000;
        let t = std::time::Instant::now();
        for i in 0..steps {
            rb.step(1.0 / 120.0, 3000.0, &[0.0; 4], (i as f32 * 0.01).sin() * 0.2, &g);
        }
        let us = t.elapsed().as_secs_f64() * 1e6 / steps as f64;
        eprintln!("{} road model: {us:.1} us per step of 1/120 s", if realistic { "realistic" } else { "OMSI 2   " });
    }
}

fn with_strut(kind: omsi_vehicle::vehicle::SuspensionKind) -> Vehicle {
    let mut v = bus();
    v.suspensions.push(omsi_vehicle::vehicle::Suspension::parse(&[&format!("type {}", match kind {
        omsi_vehicle::vehicle::SuspensionKind::Coil => "coil",
        omsi_vehicle::vehicle::SuspensionKind::Leaf => "leaf",
        omsi_vehicle::vehicle::SuspensionKind::Air => "air",
        omsi_vehicle::vehicle::SuspensionKind::Progressive => "progressive",
    }), "level_time 2"]));
    v
}

/// An axle without a `[suspension]` block is OMSI 2's coil strut, step for step.
#[test]
fn a_coil_strut_is_the_unchanged_strut() {
    use omsi_vehicle::vehicle::SuspensionKind::Coil;
    let g = road(1e9, 0.0);
    let mut a = RigidBody::from_definition(&bus(), &[]);
    let mut b = RigidBody::from_definition(&with_strut(Coil), &[]);
    a.place(DVec3::ZERO, 0.0);
    b.place(DVec3::ZERO, 0.0);
    run(&mut a, 2.0, 0.0, 0.0, &g);
    run(&mut b, 2.0, 0.0, 0.0, &g);
    assert_eq!(a.origin(), b.origin());
    assert_eq!(a.wheels[0].compression, b.wheels[0].compression);
}

/// Leaf springs rub: after a drop of the body the swing that is left a second later is no
/// larger than on a hydraulic damper alone.
#[test]
fn leaf_springs_damp_a_drop_with_their_friction() {
    use omsi_vehicle::vehicle::SuspensionKind::{Coil, Leaf};
    let late_swing = |kind| {
        let mut rb = RigidBody::from_definition(&with_strut(kind), &[]);
        rb.place(DVec3::ZERO, 0.0);
        let g = road(1e9, 0.0);
        run(&mut rb, 3.0, 0.0, 5000.0, &g);
        let before = rb.origin().z;
        rb.position.z += 0.06;
        let mut late = 0.0f64;
        for i in 0..240 {
            rb.step(1.0 / 60.0, 0.0, &[5000.0; 4], 0.0, &g);
            if i >= 60 {
                late = late.max((rb.origin().z - before).abs());
            }
        }
        late
    };
    let (coil, leaf) = (late_swing(Coil), late_swing(Leaf));
    assert!(coil < 0.06 && leaf < 0.06, "coil {coil}, leaf {leaf}");
    assert!(leaf <= coil + 1e-4, "friction does not add swing: coil {coil}, leaf {leaf}");
}

/// Air bellows hold the ride height when the bus is loaded; a coil spring sags.
#[test]
fn air_bellows_hold_the_ride_height_under_load() {
    use omsi_vehicle::vehicle::SuspensionKind::{Air, Coil};
    let height_after_load = |kind| {
        let mut rb = RigidBody::from_definition(&with_strut(kind), &[]);
        rb.place(DVec3::ZERO, 0.0);
        let g = road(1e9, 0.0);
        run(&mut rb, 3.0, 0.0, 5000.0, &g);
        let empty = rb.origin().z;
        rb.mass += 4000.0;
        run(&mut rb, 40.0, 0.0, 5000.0, &g);
        (empty, rb.origin().z)
    };
    let (coil_empty, coil_loaded) = height_after_load(Coil);
    let (air_empty, air_loaded) = height_after_load(Air);
    assert!(coil_empty - coil_loaded > 0.03, "a coil spring sags with 4 t: {coil_empty} -> {coil_loaded}");
    assert!((air_empty - air_loaded).abs() < 0.02, "air holds the height: {air_empty} -> {air_loaded}");
}
