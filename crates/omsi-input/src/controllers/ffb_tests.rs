#[test]
fn the_wheel_is_pulled_to_the_middle_harder_at_speed() {
    let mut t = 0.0;
    let f = |kmh| super::FfInput { on: true, kmh, dt: 0.016, ..Default::default() };
    let slow = super::wheel_force(&f(10.0), 0.5, 0.5, &mut t, 1.0, 1.0);
    let fast = super::wheel_force(&f(60.0), 0.5, 0.5, &mut t, 1.0, 1.0);
    assert!(slow < 0.0 && fast < slow, "{slow} {fast}");
    // Moving to the right at a standstill, it is held back by tyre scrub.
    let turning = super::wheel_force(&f(0.0), 0.0, -0.05, &mut t, 1.0, 1.0);
    assert!(turning < 0.0);
}

#[test]
fn a_parked_wheel_has_no_self_centering_force() {
    let mut t = 0.0;
    for kmh in [-0.5, -0.1, 0.0, 0.1, 0.5] {
        // Residual lateral acceleration from the body's suspension is no force when parked.
        let f = super::FfInput { on: true, kmh, lateral_accel: 3.0, dt: 0.016, ..Default::default() };
        assert_eq!(super::wheel_force(&f, 0.0, 0.0, &mut t, 1.0, 0.0), 0.0, "kmh={kmh}");
        for x in [0.2, 0.5, 1.0] {
            let right = super::wheel_force(&f, x, x, &mut t, 1.0, 0.0);
            let left = super::wheel_force(&f, -x, -x, &mut t, 1.0, 0.0);
            assert_eq!(right, 0.0, "kmh={kmh}, steering={x}: {right}");
            assert_eq!(right, -left, "kmh={kmh}, steering={x}");
        }
    }
}

#[test]
fn parking_drag_opposes_turning_in_both_directions() {
    let mut t = 0.0;
    let f = super::FfInput { on: true, dt: 0.016, ..Default::default() };
    for x in [-0.5, 0.5] {
        let right = super::wheel_force(&f, x, x - 0.01, &mut t, 1.0, 0.0);
        let left = super::wheel_force(&f, x, x + 0.01, &mut t, 1.0, 0.0);
        // The drag opposes wheel motion in either direction.
        assert!(right < left, "{right} {left}");
    }
}

#[test]
fn self_aligning_torque_builds_up_as_the_bus_rolls_in_either_direction() {
    let mut t = 0.0;
    let force = |kmh, t: &mut f32| {
        let f = super::FfInput { on: true, kmh, dt: 0.016, ..Default::default() };
        super::wheel_force(&f, 0.5, 0.5, t, 1.0, 0.0)
    };
    let mut previous = 0.0;
    for kmh in [0.51, 1.0, 2.0, 3.0, 4.0, 5.0] {
        let forward = force(kmh, &mut t);
        let reverse = force(-kmh, &mut t);
        assert!(forward < previous, "kmh={kmh}: {forward} >= {previous}");
        assert_eq!(forward, reverse);
        previous = forward;
    }
}

/// A wheel let go at half lock in a turn comes back to the centre and settles there,
/// without swinging far past it (#2103) - a rim of 0.06 kg m^2 on an 8 Nm base, 900
/// degrees, the forces sent sixty times a second.
#[test]
fn a_released_wheel_returns_without_overshooting() {
    for kmh in [10.0, 30.0, 60.0] {
        let mut t = 0.0;
        let (mut x, mut x0) = (0.5f32, 0.5f32);
        let mut lowest = x;
        let dt = 1.0 / 60.0;
        let half_range = 450.0f32.to_radians();
        let accel_per_force = 8.0 / 0.06 / half_range;
        let mut v = 0.0f32;
        for _ in 0..(6.0 / dt) as usize {
            let f = super::FfInput { on: true, kmh, dt, ..Default::default() };
            let force = super::wheel_force(&f, x, x0, &mut t, 1.0, 0.0);
            v += force * accel_per_force * dt;
            x0 = x;
            x += v * dt;
            lowest = lowest.min(x);
        }
        assert!(x.abs() < 0.03, "kmh={kmh}: still at {x}");
        assert!(lowest > -0.06, "kmh={kmh}: swung past the centre to {lowest}");
    }
}

#[test]
fn steering_resistance_drops_as_the_bus_starts_rolling() {
    let mut t = 0.0;
    let f = |kmh| super::FfInput { on: true, kmh, dt: 0.016, ..Default::default() };
    let parked = super::wheel_force(&f(0.0), 0.2, 0.15, &mut t, 1.0, 0.0);
    let moving = super::wheel_force(&f(30.0), 0.2, 0.15, &mut t, 1.0, 0.0);
    assert!(parked < moving && moving < 0.0, "{parked} {moving}");
}

#[test]
fn a_returning_wheel_is_not_stopped_by_parking_drag() {
    let mut t = 0.0;
    let f = super::FfInput { on: true, kmh: 20.0, dt: 0.016, ..Default::default() };
    // (coming back at a third of the wheel's range a second, as a wheel let go does)
    let right = super::wheel_force(&f, 0.5, 0.505, &mut t, 1.0, 0.0);
    let left = super::wheel_force(&f, -0.5, -0.505, &mut t, 1.0, 0.0);
    assert!(right < 0.0 && left > 0.0, "{right} {left}");
}

#[test]
fn a_ninety_degree_turn_has_return_torque_across_wheel_ranges() {
    let mut t = 0.0;
    let f = super::FfInput { on: true, kmh: 30.0, dt: 0.016, ..Default::default() };
    for range in [900.0, 1080.0, 1800.0, 2880.0] {
        let x = 90.0 / (range * 0.5);
        let right = super::wheel_force(&f, x, x, &mut t, 1.0, 0.0);
        let left = super::wheel_force(&f, -x, -x, &mut t, 1.0, 0.0);
        assert!(right < -0.05, "range={range}: {right}");
        assert_eq!(right, -left);
    }
}

#[test]
fn small_remaining_angles_keep_returning_while_the_bus_rolls() {
    let mut t = 0.0;
    for kmh in [5.0, 20.0, 70.0] {
        let f = super::FfInput { on: true, kmh, dt: 1.0 / 60.0, ..Default::default() };
        for x in [0.005, 0.01, 0.05] {
            // Already returning slowly: do not let steering drag cancel
            // the last few degrees of return on a wheel with friction.
            let right = super::wheel_force(&f, x, x + 0.001, &mut t, 1.0, 0.0);
            let left = super::wheel_force(&f, -x, -x - 0.001, &mut t, 1.0, 0.0);
            assert!(right < -0.02 && left > 0.02, "kmh={kmh}, x={x}: {right} {left}");
            assert!((right + left).abs() < 1e-6);
        }
    }
}

#[test]
fn return_torque_fades_smoothly_at_the_physical_centre() {
    let mut t = 0.0;
    let f = super::FfInput { on: true, kmh: 30.0, dt: 0.016, ..Default::default() };
    let mut previous: f32 = 1.0;
    for x in [0.2, 0.1, 0.01, 0.001, 0.0001, 0.00001, 0.0] {
        let force = super::wheel_force(&f, x, x, &mut t, 1.0, 0.0);
        assert!(force.abs() < previous, "x={x}: {force}");
        if x == 0.0 {
            assert_eq!(force, 0.0);
        } else {
            assert!(force < 0.0);
        }
        if x <= 0.0001 {
            assert!(force.abs() < 0.002, "x={x}: {force}");
        }
        previous = force.abs();
    }
}

#[test]
fn a_moving_wheel_is_damped_when_it_crosses_the_centre() {
    let mut t = 0.0;
    for kmh in [5.0, 20.0, 70.0] {
        let f = super::FfInput { on: true, kmh, dt: 0.02, ..Default::default() };
        // At the centre there is no spring torque. The remaining force must
        // slow a crossing wheel in either direction rather than accelerate it.
        let right = super::wheel_force(&f, 0.0, -0.01, &mut t, 1.0, 0.0);
        let left = super::wheel_force(&f, 0.0, 0.01, &mut t, 1.0, 0.0);
        assert!(right < -0.015 && left > 0.015, "kmh={kmh}: {right} {left}");
        assert!((right + left).abs() < 1e-6);
        assert_eq!(super::wheel_force(&f, 0.0, 0.0, &mut t, 1.0, 0.0), 0.0);
        // Steering strength controls damping too, independently of vibration.
        assert_eq!(super::wheel_force(&f, 0.0, -0.01, &mut t, 0.0, 0.0), 0.0);
    }
}

#[test]
fn the_same_wheel_velocity_gives_the_same_force_at_different_frame_rates() {
    let mut t = 0.0;
    for velocity in [-0.5, 0.5] {
        let mut reference: Option<f32> = None;
        for fps in [20.0, 30.0, 60.0, 144.0] {
            let dt = 1.0 / fps;
            let f = super::FfInput { on: true, kmh: 30.0, dt, ..Default::default() };
            let force = super::wheel_force(&f, 0.05, 0.05 - velocity * dt, &mut t, 1.0, 0.0);
            if let Some(previous) = reference {
                assert!((force - previous).abs() < 1e-6, "fps={fps}: {force} {previous}");
            } else {
                reference = Some(force);
            }
        }
    }
}

#[test]
fn a_real_turn_adds_aligning_torque_but_a_parked_bus_does_not() {
    let mut t = 0.0;
    let mut f = super::FfInput { on: true, kmh: 30.0, dt: 0.016, ..Default::default() };
    let straight = super::wheel_force(&f, 0.2, 0.2, &mut t, 1.0, 0.0);
    f.lateral_accel = 3.0;
    let right_turn = super::wheel_force(&f, 0.2, 0.2, &mut t, 1.0, 0.0);
    assert!(right_turn < straight, "{straight} {right_turn}");
    f.kmh = 0.0;
    let parked = super::wheel_force(&f, 0.2, 0.2, &mut t, 1.0, 0.0);
    f.lateral_accel = 0.0;
    let parked_without_accel = super::wheel_force(&f, 0.2, 0.2, &mut t, 1.0, 0.0);
    assert_eq!(parked, parked_without_accel);
}

#[test]
fn steering_assist_lightens_turning_out() {
    let mut t = 0.0;
    let f = super::FfInput { on: true, kmh: 25.0, lateral_accel: 2.0, dt: 0.016, ..Default::default() };
    let held = super::wheel_force(&f, 0.5, 0.5, &mut t, 1.0, 0.0);
    let turning_out = super::wheel_force(&f, 0.5, 0.48, &mut t, 1.0, 0.0);
    // (coming back as a wheel let go does; a faster return meets the steering's damping)
    let returning = super::wheel_force(&f, 0.5, 0.505, &mut t, 1.0, 0.0);
    // Assistance still lightens turning out. A moving return also includes
    // damping, so its net torque need not exceed the outward-turning torque.
    assert!(held < turning_out && turning_out < 0.0, "{held} {turning_out}");
    assert!(returning < 0.0, "{returning}");
}

#[test]
fn centering_does_not_grow_linearly_to_full_lock() {
    let mut t = 0.0;
    let f = super::FfInput { on: true, kmh: 30.0, dt: 0.016, ..Default::default() };
    let quarter = super::wheel_force(&f, 0.25, 0.25, &mut t, 1.0, 0.0);
    let full = super::wheel_force(&f, 1.0, 1.0, &mut t, 1.0, 0.0);
    assert!(full < quarter && full > 2.5 * quarter, "{quarter} {full}");
}

#[test]
fn wheel_bumps_need_motion_and_a_suspension_or_impact_event() {
    assert_eq!(super::bump_strength(0.0, 0.0, 20.0), 0.0);
    assert_eq!(super::bump_strength(1.0, 0.0, 0.0), 0.0);
    assert_eq!(super::bump_strength(0.008, 0.08, 20.0), 0.0);
    assert!(super::bump_strength(0.015, 0.0, 20.0) < 0.05);
    assert!(super::bump_strength(0.05, 0.0, 20.0) > 0.4);
    assert!(super::bump_strength(0.1, 0.0, 20.0) > 0.9);
    assert!(super::bump_strength(0.0, 1.0, 20.0) > 0.5);
}

#[test]
fn settling_ignores_seams_and_keeps_slow_motion_as_it_was() {
    let dt = 1.0 / 60.0;
    let mut settled = Vec::new();
    // the first frame (or the first back in the bus) starts from the travel as it is
    assert_eq!(super::settle(&mut settled, &[0.09], dt), vec![0.0]);
    // a 1.5 cm seam for one step
    let seam = super::settle(&mut settled, &[0.105], dt)[0];
    assert!(super::bump_strength(seam, 0.0, 20.0) < 0.05);
    // a 6 cm step (a pothole's edge)
    let mut settled = vec![0.0];
    let step = super::settle(&mut settled, &[0.06], dt)[0];
    assert!(super::bump_strength(step, 0.0, 20.0) > 0.45);
    // brake dive at 0.3 m/s: about as strong as the old one-step rate made it
    let mut settled = vec![0.0];
    let mut travel = 0.0;
    let mut ripple = 0.0;
    for _ in 0..60 {
        travel += 0.3 * dt;
        ripple = super::settle(&mut settled, &[travel], dt)[0];
    }
    let old = (0.3 - 0.12) / 0.9;
    assert!((super::bump_strength(ripple, 0.0, 20.0) - old).abs() < 0.05, "{ripple}");
}

/// Where the wheel plays the shaking as its own periodic effect, the force set each
/// frame carries none of it.
#[test]
fn the_shaking_leaves_the_force_when_the_wheel_plays_it() {
    let f = super::FfInput { on: true, kmh: 0.0, vib_amp: 1.0, vib_period: 2.5, dt: 0.016, ..Default::default() };
    let mut t = 0.0;
    let soft = super::wheel_force(&f, 0.0, 0.0, &mut t, 0.0, 1.0);
    assert!(soft.abs() > 0.01, "{soft}");
    t = 0.0;
    let periodic = super::wheel_force(&super::FfInput { vib_amp: 0.0, ..f }, 0.0, 0.0, &mut t, 0.0, 1.0);
    assert_eq!(periodic, 0.0);
}

#[test]
fn a_wheel_bump_uses_the_device_vibration_strength() {
    let mut t = 0.0;
    let f = super::FfInput { on: true, kmh: 20.0, wheel_bump: 1.0, dt: 0.016, ..Default::default() };
    let off = super::wheel_force(&f, 0.0, 0.0, &mut t, 0.0, 0.0);
    t = 0.0;
    let on = super::wheel_force(&f, 0.0, 0.0, &mut t, 0.0, 1.0);
    assert_eq!(off, 0.0);
    assert!(on.abs() > 0.15, "{on}");
    assert!(on.abs() < 0.5, "{on}");
    let lighter = super::wheel_force(&super::FfInput { wheel_bump: 0.25, ..f }, 0.0, 0.0, &mut t, 0.0, 1.0);
    assert!(lighter.abs() > on.abs() * 0.45, "{lighter} {on}");
    t = 0.37;
    let at_impact = super::wheel_force(&f, 0.0, 0.0, &mut t, 0.0, 1.0);
    assert!((on - at_impact).abs() < 0.001, "{on} {at_impact}");
}

#[test]
fn active_turning_feels_firmer_at_road_speed_than_in_town() {
    let mut t = 0.0;
    let f = |kmh| super::FfInput { on: true, kmh, dt: 0.016, ..Default::default() };
    let town = super::wheel_force(&f(10.0), 0.5, 0.48, &mut t, 1.0, 0.0);
    let road = super::wheel_force(&f(70.0), 0.5, 0.48, &mut t, 1.0, 0.0);
    assert!(town < 0.0 && road < town * 1.4, "{town} {road}");
}

#[test]
fn steering_force_scale_changes_the_constant_force() {
    let mut t = 0.0;
    let f = super::FfInput { on: true, kmh: 30.0, dt: 0.016, ..Default::default() };
    let zero = super::wheel_force(&f, 0.4, 0.4, &mut t, 0.0, 0.0);
    let normal = super::wheel_force(&f, 0.4, 0.4, &mut t, 1.0, 0.0);
    assert_eq!(zero, 0.0);
    assert!(normal.abs() > 0.05, "{normal}");
}

/// The largest the trembling of the tarmac and the engine may become (of the wheel's
/// full lock) - a wheel's centring spring is several times this, and the scripts' own
/// shaking is 0.25, so the tremble stays felt as a buzz and not as a push.
const MICRO_MAX: f32 = 0.25;

/// Ten seconds of the road at `kmh`, as sampled by the game.
fn road(kmh: f32) -> Vec<f32> {
    let mut m = super::Micro::default();
    (0..600).map(|_| m.sample(1.0 / 60.0, kmh, 0.0, 0.0, 0.0, 1.0, 0.0)).collect()
}

/// The most the signal ever lines up with itself, at any lag up to two seconds. Sines
/// line themselves up; noise cannot, so this is what tells a road carrying a weight
/// from a road rattling.
fn self_similarity(v: &[f32]) -> f32 {
    let rms = (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
    let mut best = 0.0f32;
    for lag in 4..120.min(v.len()) {
        let mut num = 0.0f32;
        for i in lag..v.len() {
            num += v[i] * v[i - lag];
        }
        best = best.max((num / ((v.len() - lag) as f32) / rms / rms).abs());
    }
    best
}

#[test]
fn the_road_carries_a_weight_rather_than_a_rattle() {
    // A wheel is told one force at a time, and a force that repeats itself tells the
    // driver there is something solid underneath the bus, while one that never does
    // tells them the surface is broken up into loose pieces - which is what the
    // complaint was: a road made of nothing but noise reads as random however fast it
    // is felt. Noise scores about 0.2 here at any speed and the ride about 0.8, so
    // the grain left under it cannot pull this down where it matters.
    for kmh in [15.0, 30.0, 50.0, 80.0] {
        let alike = self_similarity(&road(kmh));
        assert!(alike > 0.5, "the road at {kmh} km/h carries nothing solid: {alike:.2}");
    }
}
#[test]
fn the_road_is_under_the_tyres_and_not_merely_slow() {
    // How often the road changes is what the tyre feels, and it has to outrun the
    // hands: the rim goes wherever it is told and the hands go with it, so a change a
    // few times a second is a push to be dragged along by rather than a texture. The
    // short layer is what keeps this true as the bus speeds up.
    let rate = |v: &[f32]| {
        let mut crossings = 0;
        for i in 1..v.len() {
            if (v[i] > 0.0) != (v[i - 1] > 0.0) {
                crossings += 1;
            }
        }
        crossings as f32 / (v.len() as f32 / 60.0) / 2.0
    };
    assert!(rate(&road(30.0)) > 6.0, "the road in town is too slow to feel: {}", rate(&road(30.0)));
    assert!(rate(&road(50.0)) > 10.0, "the road at 50 is too slow to feel: {}", rate(&road(50.0)));
    assert!(rate(&road(80.0)) > 13.0, "the road does not quicken with the speed: {}", rate(&road(80.0)));
}


#[test]
fn the_tremble_stays_silent_at_a_standstill_and_appears_with_the_speed() {
    // how much is felt is the average of the tremble, not its loudest frame: the
    // grain is noise, and its peaks fall where the pattern happens to be
    let felt = |m: &mut super::Micro, kmh: f32| {
        (0..240).map(|_| m.sample(1.0 / 60.0, kmh, 0.0, 0.0, 0.0, 1.0, 0.0).abs()).sum::<f32>() / 240.0
    };
    let mut m = super::Micro::default();
    assert_eq!(felt(&mut m, 0.0), 0.0, "a bus standing still feels no grain at all");
    let crawling = felt(&mut m, 5.0);
    let town = felt(&mut m, 30.0);
    let road = felt(&mut m, 50.0);
    let fast = felt(&mut m, 80.0);
    assert!(crawling > 0.0 && town > crawling * 2.0, "{crawling} {town}");
    // the finer layer is let go above 40 Hz and the coarser one carries the road
    // there, so the strength holds from town speed up rather than falling away
    assert!(road > town * 0.9, "the grain should be at its strongest by 30 km/h: {town} {road}");
    assert!(fast > road * 0.9, "the grain thins out at speed: {fast} against {road}");
    assert!(fast < MICRO_MAX, "the grain grows too strong: {fast}");
}

#[test]
fn the_tremble_is_read_along_the_road_not_along_the_clock() {
    let mut m = super::Micro::default();
    // two seconds at 50 km/h over one stretch of tarmac, then the same stretch again
    let over = |m: &mut super::Micro| (0..120).map(|_| m.sample(1.0 / 60.0, 50.0, 0.0, 0.0, 0.0, 1.0, 0.0)).collect::<Vec<_>>();
    let first = over(&mut m);
    assert!(first.iter().any(|v| v.abs() > 0.001));
    let start = m.roll;
    assert!(start > 20.0, "the distance the bus rolled was not kept: {start}");
    m.roll = 0.0;
    assert_eq!(over(&mut m), first, "the same stretch of road shook differently");
    // a long drive does not walk the pattern off into a constant
    m.roll = 2_000_000.0;
    assert!(over(&mut m).iter().any(|v| v.abs() > 0.001), "the grain died out after a long drive");
}

#[test]
fn a_wet_or_snowy_road_hums_more_than_dry_asphalt() {
    let felt = |cond: f32| {
        let mut m = super::Micro::default();
        (0..120).map(|_| m.sample(1.0 / 60.0, 50.0, cond, 0.0, 0.0, 1.0, 0.0).abs()).sum::<f32>() / 120.0
    };
    let (dry, wet, snow) = (felt(0.0), felt(1.0), felt(2.0));
    assert!(wet > dry && snow > wet, "{dry} {wet} {snow}");
}

#[test]
fn the_engine_is_felt_idling_and_grows_with_what_it_is_doing() {
    let mut m = super::Micro::default();
    let off = (0..60).map(|_| m.sample(1.0 / 60.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0)).fold(0.0f32, f32::max);
    assert_eq!(off, 0.0, "an engine that is not running is felt through nothing");
    let peak = |rpm: f32, load: f32| {
        let mut m = super::Micro::default();
        // two seconds, so the engine's speed is settled and its angle has turned
        (0..120).map(|_| m.sample(1.0 / 60.0, 0.0, 0.0, rpm, load, 0.0, 1.0)).fold(0.0f32, f32::max)
    };
    let idling = peak(700.0, 0.0);
    let working = peak(700.0, 1.0);
    let pulling = peak(2200.0, 1.0);
    assert!(idling > 0.001, "a running engine at idle is not felt at all: {idling}");
    assert!(working > idling, "a loaded engine is not felt harder than an idling one: {idling} {working}");
    assert!(pulling > working, "a fast engine is not felt harder than a slow one: {working} {pulling}");
    assert!(pulling < MICRO_MAX, "the engine shakes the wheel too hard: {pulling}");
}

#[test]
fn the_engine_leads_on_the_crankshaft_and_stays_under_the_aliasing_limit() {
    // six seconds of an engine at 700 rpm, which is where the buzz is at its most
    // characteristic; the whole of it is a sum of sines, so projecting the signal on
    // each frequency that is in it says how the sound is shared out between them
    let dt = 1.0f32 / 60.0;
    let mut m = super::Micro::default();
    let sig: Vec<f32> = (0..360).map(|_| m.sample(dt, 0.0, 0.0, 700.0, 0.5, 0.0, 1.0)).collect();
    let partial = |f: f32| {
        let (mut re, mut im) = (0.0f32, 0.0f32);
        for (i, &x) in sig.iter().enumerate() {
            let w = std::f32::consts::TAU * f * dt * i as f32;
            re += x * w.cos();
            im += x * w.sin();
        }
        (re * re + im * im).sqrt() / sig.len() as f32 * 2.0
    };
    // at 700 rpm the crankshaft turns at 11.7 Hz and a four-stroke four fires at
    // 23.3 Hz; the rumble under them is at 2.9 Hz. What leads is the crankshaft.
    let rumble = partial(700.0 / 240.0);
    let crank = partial(700.0 / 60.0);
    let firing = partial(700.0 / 30.0);
    assert!(crank > rumble, "the buzz leads on the low rumble instead of the engine: {rumble} {crank}");
    assert!(crank > firing, "the firing pulses lead instead of the engine turning: {crank} {firing}");
    // a wheel is told its force once a frame, so nothing the engine does may ask for
    // a frequency the frame cannot carry
    for f in [700.0 / 240.0, 700.0 / 60.0, 700.0 / 30.0] {
        assert!(f < 28.0, "the engine asks for {f} Hz, past what a wheel can be told");
    }
    // and at the top of the rev range, where the crank and the pulses have both been
    // let go, what is left is the rumble and not silence
    let mut m = super::Micro::default();
    let top: Vec<f32> = (0..360).map(|_| m.sample(dt, 0.0, 0.0, 4500.0, 1.0, 0.0, 1.0)).collect();
    let mut im = 0.0f32;
    for (i, &x) in top.iter().enumerate() {
        im += x * (std::f32::consts::TAU * (4500.0 / 240.0) * dt * i as f32).sin();
    }
    let mass = (im / top.len() as f32).abs();
    assert!(mass > 0.01, "a flat-out engine is felt as nothing: {mass}");
}

#[test]
fn the_tremble_is_turned_off_by_its_settings() {
    let run = |road: f32, engine: f32| {
        let mut m = super::Micro::default();
        (0..120).map(|_| m.sample(1.0 / 60.0, 60.0, 0.0, 900.0, 0.5, road, engine)).fold(0.0f32, f32::max)
    };
    assert_eq!(run(0.0, 0.0), 0.0);
    assert!(run(1.0, 0.0) > 0.0 && run(0.0, 1.0) > 0.0);
    assert!(run(2.0, 2.0) > run(1.0, 1.0), "the settings do not make it stronger");
    // a bigger engine buzz is not a road that got rougher
    let mut road_only = super::Micro::default();
    let mut both = super::Micro::default();
    for _ in 0..60 {
        road_only.sample(1.0 / 60.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0);
        both.sample(1.0 / 60.0, 0.0, 0.0, 900.0, 0.0, 1.0, 1.0);
    }
    assert_eq!(road_only.roll, both.roll, "the engine changed how the road is read");
}

#[test]
fn the_tremble_comes_out_of_the_wheel_through_the_vibration_strength() {
    let mut t = 0.0;
    let f = super::FfInput { on: true, kmh: 50.0, micro: 0.08, dt: 0.016, ..Default::default() };
    assert_eq!(super::wheel_force(&f, 0.0, 0.0, &mut t, 1.0, 0.0), 0.0, "it shook the wheel with the vibrations switched off");
    let felt = super::wheel_force(&f, 0.0, 0.0, &mut t, 1.0, 1.0);
    assert!((felt - 0.08).abs() < 0.0001, "{felt}");
    assert_eq!(super::wheel_force(&super::FfInput { micro: 0.5, ..f }, 0.0, 0.0, &mut t, 1.0, 1.0), 0.5);
}

#[test]
fn a_vibration_eases_away_once_it_stops() {
    assert_eq!(super::fade_gain(0.0, super::FF_FADE), 1.0);
    assert_eq!(super::fade_gain(super::FF_FADE * 0.5, super::FF_FADE), 0.5);
    assert_eq!(super::fade_gain(super::FF_FADE, super::FF_FADE), 0.0);
    assert_eq!(super::fade_gain(super::FF_FADE * 3.0, super::FF_FADE), 0.0);
    // it never jumps: no frame hands the wheel a step of its own
    let mut last = 1.0;
    for i in 0..40 {
        let g = super::fade_gain(i as f32 / 40.0 * super::FF_FADE * 1.2, super::FF_FADE);
        assert!(g <= last && last - g < 0.15, "{g} after {last}");
        last = g;
    }
    // a fade of nothing is the old way: it stops where it stands
    assert_eq!(super::fade_gain(0.5, 0.0), 1.0);
}

#[test]
fn the_settings_fade_is_how_long_a_jolt_takes_to_go() {
    // The jolt the physics hands over is not faded by a gain applied afterwards: by the
    // time such a gain would start, the jolt it was following is already spent. So the
    // jolt is held and eased on the settings' own time, and this is what says whether
    // the setting reaches the jolt at all.
    let dt = 1.0f32 / 60.0;
    let tail = |fade: f32| {
        let mut held = 0.6f32;
        let mut frames = 0;
        for _ in 0..600 {
            held = held * (-dt / (fade * 0.3)).exp();
            frames += 1;
            if held < 0.002 {
                break;
            }
        }
        frames
    };
    let fast = tail(0.05);
    let slow = tail(0.6);
    assert!(fast < slow * 3, "the setting barely changes the jolt: {fast} {slow}");
    assert!(fast >= 2, "the jolt was cut off in a single frame: {fast}");
    assert!(slow <= 120, "a long fade never ends: {slow}");
}

#[test]
fn a_scripts_shaking_eases_away_instead_of_stopping_dead() {
    // The scripts write the amplitude afresh every frame, so on the frame they stop it
    // is already zero and fading that fades nothing: the wheel drops the shake in a
    // single step, which is the jolt the fade is here to avoid. What the fade needs is
    // the last amplitude the scripts reached the wheel with, so this is what says
    // whether it is still there once they have gone quiet.
    let dt = 1.0f32 / 60.0;
    let mut vib = super::ScriptVib::default();
    let (amp, period) = vib.step(true, 0.8, 12.0, super::FF_FADE, dt);
    assert_eq!(amp, 0.8, "the shake the scripts asked for never reached the wheel");
    assert_eq!(period, 12.0, "the period the scripts asked for was not passed on");
    // the frame the scripts stop: kept, not cut off
    let (mut amp, period) = vib.step(true, 0.0, 0.0, super::FF_FADE, dt);
    assert!(amp > 0.5, "the shake stopped dead the frame the scripts did: {amp}");
    assert_eq!(period, 12.0, "the period went with the amplitude instead of being kept");
    // and it eases away to nothing rather than to a stop
    let mut frames = 0;
    while amp > 0.0 && frames < 600 {
        amp = vib.step(true, 0.0, 0.0, super::FF_FADE, dt).0;
        frames += 1;
    }
    assert_eq!(amp, 0.0, "the shake never finished fading: {amp}");
    // the setting is what decides how long that takes
    let mut fast = super::ScriptVib::default();
    fast.step(true, 0.8, 12.0, 0.05, dt);
    let mut quick = 0;
    while fast.step(true, 0.0, 0.0, 0.05, dt).0 > 0.0 && quick < 600 {
        quick += 1;
    }
    assert!(quick < frames, "the setting barely changes the shake: {quick} {frames}");
    // with the fade at zero it stops where it stands, as it always did
    let mut none = super::ScriptVib::default();
    none.step(true, 0.8, 12.0, 0.0, dt);
    assert_eq!(none.step(true, 0.0, 0.0, 0.0, dt).0, 0.0, "the fade at zero still hangs on");
    // and a bus whose force feedback is off is not shaking at all
    let mut off = super::ScriptVib::default();
    off.step(true, 0.8, 12.0, super::FF_FADE, dt);
    assert_eq!(off.step(false, 0.0, 0.0, super::FF_FADE, dt).0, 0.0);
}

/// `wheel_force` as it was in v0.3.42, before a wheel could be tuned: what the default
/// tuning must still give, bit for bit.
fn wheel_force_v0342(f: &super::FfInput, x: f32, x0: f32, t: &mut f32, k_springs: f32, k_effects: f32) -> f32 {
    let dt = f.dt.max(1e-3);
    let v = f.kmh.abs();
    let x = x.clamp(-1.0, 1.0);
    let rolling = ((v - 0.5) / 4.5).clamp(0.0, 1.0);
    let rolling = rolling * rolling * (3.0 - 2.0 * rolling);
    let road_speed = ((v - 20.0) / 50.0).clamp(0.0, 1.0);
    let spring_strength = (0.12 + 0.38 * v / (v + 20.0)) * (1.0 + 0.5 * road_speed);
    let moving_steering_gain = 1.0 + 0.18 * v / (v + 8.0);
    let centre_return = 0.025 * x / (x * x + 0.004 * 0.004).sqrt() / (1.0 + (x / 0.12).powi(4));
    let spring = -(spring_strength * x / (0.5 + 1.15 * x.abs()) + centre_return) * rolling;
    let road_align = -(f.lateral_accel / 9.81).clamp(-0.45, 0.45) * 0.25 * (v / 5.0).clamp(0.0, 1.0) * rolling;
    let lock_assist = 1.0 / (1.0 + 0.55 * x * x);
    let turning_speed = ((x - x0) / dt).clamp(-4.0, 4.0);
    let turning_out = (x * turning_speed * 2.0).clamp(0.0, 1.0);
    let assist = 1.0 - (0.4 - 0.16 * (v / 80.0).min(1.0)) * turning_out;
    let parking_drag = 0.018 + 0.12 / (1.0 + (v / 6.0).powi(2));
    let returning = x * turning_speed < 0.0;
    let drag = -turning_speed * parking_drag * if returning { 1.0 - 0.8 * rolling } else { 1.0 };
    let back = ((0.03 - x * turning_speed.signum()) / 0.06).clamp(0.0, 1.0);
    let fast_back = 0.35 * (turning_speed.abs() - 0.15).max(0.0) * turning_speed.signum() * back * back * (3.0 - 2.0 * back);
    let damping = -(0.03 * turning_speed + fast_back) * rolling;
    *t += dt;
    let period = (f.vib_period * 0.01).max(0.02);
    let shake = f.vib_amp.clamp(0.0, 1.0) * super::VIB_SHARE * (std::f32::consts::TAU * *t / period).sin();
    let bump = f.wheel_bump.clamp(0.0, 1.0).sqrt() * 0.46 * (std::f32::consts::TAU * f.wheel_bump_age * 6.5).cos();
    let steering = ((spring + road_align) * lock_assist * assist + drag) * moving_steering_gain + damping;
    let micro = f.micro.clamp(-1.0, 1.0);
    (steering * k_springs.clamp(0.0, 2.0) + (shake + bump + micro) * k_effects.clamp(0.0, 2.0)).clamp(-1.0, 1.0)
}

/// The steering force alone (no vibrations, `[FFScale]` 1) under `tune`, from a filter at rest.
fn tuned(f: &super::FfInput, x: f32, x0: f32, tune: super::FfTune) -> f32 {
    super::tuned_wheel_force(f, x, x0, &mut 0.0, &mut 0.0, 1.0, 0.0, &tune)
}

fn rolling(kmh: f32) -> super::FfInput {
    super::FfInput { on: true, kmh, dt: 1.0 / 60.0, ..Default::default() }
}

#[test]
fn the_default_tuning_gives_the_wheel_the_very_same_force_as_before() {
    let tune = super::FfTune::default();
    let mut checked = 0;
    for kmh in [-40.0, -3.0, -0.5, 0.0, 0.3, 0.5, 0.6, 2.0, 5.0, 12.0, 30.0, 70.0, 130.0] {
        for lateral_accel in [-8.0, -1.0, 0.0, 2.5] {
            for (wheel_bump, vib_amp, micro) in [(0.0, 0.0, 0.0), (0.6, 0.8, -0.07)] {
                for dt in [0.0, 1.0 / 144.0, 1.0 / 60.0, 0.05] {
                    let f = super::FfInput { on: true, kmh, lateral_accel, wheel_bump, wheel_bump_age: 0.05, vib_amp, vib_period: 3.0, micro, dt, ..Default::default() };
                    for x in [-1.2, -1.0, -0.5, -0.1, -0.01, -0.0005, 0.0, 0.0005, 0.02, 0.3, 0.9] {
                        for dx in [-0.05, -0.004, 0.0, 0.001, 0.03] {
                            for (k_s, k_e) in [(1.0, 1.0), (0.0, 1.0), (0.65, 1.25), (2.5, 0.0)] {
                                let (mut t_was, mut t_now, mut smoothed) = (0.37, 0.37, 0.2);
                                let was = wheel_force_v0342(&f, x, x - dx, &mut t_was, k_s, k_e);
                                let now = super::tuned_wheel_force(&f, x, x - dx, &mut t_now, &mut smoothed, k_s, k_e, &tune);
                                assert_eq!(now.to_bits(), was.to_bits(), "kmh={kmh} lateral={lateral_accel} dt={dt} x={x} dx={dx} k=({k_s}, {k_e}): {now} against {was}");
                                assert_eq!(t_now, t_was);
                                checked += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(checked > 10_000, "{checked}");
}

#[test]
fn without_centring_a_held_wheel_is_not_pulled_but_a_moving_one_is_still_slowed() {
    let none = super::FfTune { centring: 0.0, ..Default::default() };
    let normal = super::FfTune::default();
    let turning = super::FfInput { lateral_accel: 2.5, ..rolling(30.0) };
    // held in a turn: nothing brings it back, neither the tyres nor the turn's pull
    assert_eq!(tuned(&turning, 0.4, 0.4, none), 0.0);
    assert!(tuned(&turning, 0.4, 0.4, normal) < -0.05);
    assert!(tuned(&rolling(30.0), 0.4, 0.4, super::FfTune { centring: 2.0, ..normal }) < tuned(&rolling(30.0), 0.4, 0.4, normal) * 1.9);
    // crossing the centre at speed: the damping is all there is, and all of it is still there
    let crossing = tuned(&rolling(30.0), 0.0, -0.01, none);
    assert!(crossing < -0.015, "{crossing}");
    assert_eq!(crossing, tuned(&rolling(30.0), 0.0, -0.01, normal));
    // turning a parked wheel: the tyres' scrub is all there is, and all of it is still there
    let parked = tuned(&rolling(0.0), 0.2, 0.15, none);
    assert!(parked < -0.1, "{parked}");
    assert_eq!(parked, tuned(&rolling(0.0), 0.2, 0.15, normal));
}

#[test]
fn the_damping_setting_weights_the_damping_and_nothing_else() {
    let with = |damping| super::FfTune { damping, ..Default::default() };
    // a wheel held in a turn, or turned in a parked bus, has no damping to weight
    for (kmh, x, x0) in [(30.0, 0.4, 0.4), (0.0, 0.2, 0.15)] {
        assert_eq!(tuned(&rolling(kmh), x, x0, with(0.0)), tuned(&rolling(kmh), x, x0, with(1.0)), "kmh={kmh}");
    }
    // crossing the centre at speed it is most of what slows the wheel, and it goes with the setting
    let crossing = |d| tuned(&rolling(30.0), 0.0, -0.01, with(d));
    let (none, one, three) = (crossing(0.0), crossing(1.0), crossing(3.0));
    assert!(none < 0.0 && one < none * 3.0 && three < one, "{none} {one} {three}");
    assert!(((three - none) - 3.0 * (one - none)).abs() < 1e-6, "{none} {one} {three}");
}

#[test]
fn the_parking_setting_weights_the_tyres_scrub_of_a_parked_bus() {
    let with = |parking| super::FfTune { parking, ..Default::default() };
    // turning the wheel of a parked bus: nothing at all without it, twice as heavy at 2
    assert_eq!(tuned(&rolling(0.0), 0.2, 0.15, with(0.0)), 0.0);
    let normal = tuned(&rolling(0.0), 0.2, 0.15, with(1.0));
    assert!(normal < -0.1, "{normal}");
    assert!((tuned(&rolling(0.0), 0.2, 0.15, with(2.0)) - 2.0 * normal).abs() < 1e-6);
    // a wheel held in a turn while the bus rolls is centred just the same
    assert_eq!(tuned(&rolling(30.0), 0.4, 0.4, with(0.0)), tuned(&rolling(30.0), 0.4, 0.4, with(1.0)));
}

#[test]
fn the_minimum_force_lifts_a_small_pull_past_the_wheels_dead_zone_and_leaves_the_centre_alone() {
    let lift = super::FfTune { min_force: 0.1, ..Default::default() };
    let normal = super::FfTune::default();
    // a gentle pull in a turn comes out that much stronger, in its own direction
    for x in [0.1f32, -0.1, 0.4, -0.4] {
        let (was, now) = (tuned(&rolling(30.0), x, x, normal), tuned(&rolling(30.0), x, x, lift));
        assert!(was.abs() > 0.02, "x={x}: {was}");
        assert!((now - (was - 0.1 * x.signum())).abs() < 1e-6, "x={x}: {was} {now}");
    }
    // a wheel held still at the centre, or in a parked bus, is told nothing at all
    assert_eq!(tuned(&rolling(30.0), 0.0, 0.0, lift), 0.0);
    assert_eq!(tuned(&rolling(0.0), 0.3, 0.3, lift), 0.0);
    // and on the way out of the centre the lift grows from nothing, without a step for the
    // wheel to chatter on
    let mut last = 0.0f32;
    for i in 1..=300 {
        let x = i as f32 * 0.0001;
        let now = tuned(&rolling(30.0), x, x, lift);
        assert!(now <= last && last - now < 0.01, "x={x}: {now} after {last}");
        last = now;
    }
}

#[test]
fn smoothing_keeps_a_steady_force_and_eases_a_sudden_one_at_any_frame_rate() {
    let smooth = super::FfTune { smoothing: 0.03, ..Default::default() };
    let at = |fps: f32| super::FfInput { dt: 1.0 / fps, ..rolling(30.0) };
    let held = tuned(&at(60.0), 0.4, 0.4, super::FfTune::default());
    // a force that stays as it is comes out as it is
    let (mut t, mut smoothed) = (0.0, held);
    for _ in 0..30 {
        assert_eq!(super::tuned_wheel_force(&at(60.0), 0.4, 0.4, &mut t, &mut smoothed, 1.0, 0.0, &smooth), held);
    }
    // the same force arriving all at once is eased in: part of it the first frame, all of it
    // a quarter of a second later, and as far after the same time at any frame rate
    let after = |fps: f32, frames: usize| {
        let (mut t, mut smoothed, mut out) = (0.0, 0.0, 0.0);
        for _ in 0..frames {
            out = super::tuned_wheel_force(&at(fps), 0.4, 0.4, &mut t, &mut smoothed, 1.0, 0.0, &smooth);
        }
        out
    };
    let first = after(60.0, 1);
    assert!(first < 0.0 && first > held * 0.6, "{first} of {held}");
    assert!((after(60.0, 5) - after(144.0, 12)).abs() < 1e-5, "{} {}", after(60.0, 5), after(144.0, 12));
    assert!((after(60.0, 15) - held).abs() < 1e-3);
    // the vibrations are left as sharp as they are
    let tremble = super::FfInput { micro: 0.08, ..at(60.0) };
    assert_eq!(super::tuned_wheel_force(&tremble, 0.0, 0.0, &mut 0.0, &mut 0.0, 0.0, 1.0, &smooth), 0.08);
}

#[test]
fn a_tuning_out_of_range_is_brought_into_it() {
    let wild = super::FfTune { centring: 5.0, damping: -1.0, parking: f32::NAN, min_force: 1.0, smoothing: 2.0 };
    assert_eq!(wild.clamped(), super::FfTune { centring: 2.0, damping: 0.0, parking: 1.0, min_force: 0.2, smoothing: 0.05 });
    assert_eq!(super::FfTune::from_values(&[0.5]), super::FfTune { centring: 0.5, ..Default::default() });
    // and the force never leaves the wheel's range whatever it is told
    let f = super::FfInput { micro: 1.0, ..rolling(70.0) };
    let force = super::tuned_wheel_force(&f, 1.0, 0.9, &mut 0.0, &mut 0.0, 2.0, 2.0, &wild);
    assert!(force.abs() <= 1.0, "{force}");
}

mod file {
    use super::super::super::{cfg_text, parse_cfg, save_cfg_to, DeviceCfg};
    use super::super::FfTune;

    fn wheel(tune: Option<FfTune>) -> DeviceCfg {
        DeviceCfg { name: "Wheel".into(), second: "1".into(), ff_scale: Some((0.8, 1.2)), ff_tune: tune, ff_invert: Some(true), ..Default::default() }
    }

    #[test]
    fn the_tuning_is_kept_after_the_ffscale_and_read_back_as_it_was() {
        let tune = FfTune { centring: 1.25, damping: 2.0, parking: 0.5, min_force: 0.06, smoothing: 0.015 };
        let text = cfg_text(&[wheel(Some(tune))]);
        assert!(text.contains("[FFScale]\r\n0.800\r\n1.200\r\n\r\n[openOMSI.FFTuning]\r\n1.250\r\n2.000\r\n0.500\r\n0.060\r\n0.015\r\n\r\n[openOMSI.FFInvert]\r\n1\r\n"), "{text}");
        let back = parse_cfg(&text);
        assert_eq!(back[0].ff_tune, Some(tune));
        assert_eq!(cfg_text(&back), text);
        // and the save's own round-trip check lets it through
        let path = std::env::temp_dir().join(format!("openomsi-fftuning-{}.cfg", std::process::id()));
        save_cfg_to(&path, &back).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_wheel_at_the_default_tuning_writes_the_file_as_before() {
        let old = cfg_text(&[wheel(None)]);
        assert!(!old.contains("FFTuning"), "{old}");
        assert_eq!(cfg_text(&parse_cfg(&old)), old);
        assert_eq!(cfg_text(&[wheel(Some(FfTune::default()))]), old);
        // what the file would round to the defaults is the defaults, or the save would fail
        // its own round-trip check
        assert_eq!(cfg_text(&[wheel(Some(FfTune { centring: 1.0002, min_force: -0.0, ..Default::default() }))]), old);
        let path = std::env::temp_dir().join(format!("openomsi-fftuning-default-{}.cfg", std::process::id()));
        save_cfg_to(&path, &[wheel(Some(FfTune { damping: 1.0001, ..Default::default() }))]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), old);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_short_or_broken_tuning_section_falls_back_to_the_defaults() {
        let head = "\r\n[ctrl]\r\nWheel\r\n1\r\n\r\n[FFScale]\r\n1.000\r\n1.000\r\n\r\n";
        let read = |section: &str| parse_cfg(&format!("{head}[openOMSI.FFTuning]\r\n{section}"));
        assert_eq!(read("0.5\r\n2\r\n\r\n")[0].ff_tune, Some(FfTune { centring: 0.5, damping: 2.0, ..Default::default() }));
        assert_eq!(read("9\r\n-1\r\nNaN\r\n1\r\n0.5\r\n")[0].ff_tune, Some(FfTune { centring: 2.0, damping: 0.0, parking: 1.0, min_force: 0.2, smoothing: 0.05 }));
        // a section that ends at once leaves everything at its default, and what follows is
        // still read
        let empty = read("\r\n[openOMSI.FFInvert]\r\n1\r\n\r\n");
        assert_eq!(empty[0].ff_tune, Some(FfTune::default()));
        assert_eq!(empty[0].ff_invert, Some(true));
        let cut = read("0.7\r\n[openOMSI.FFInvert]\r\n0\r\n");
        assert_eq!(cut[0].ff_tune, Some(FfTune { centring: 0.7, ..Default::default() }));
        assert_eq!(cut[0].ff_invert, Some(false));
    }
}
