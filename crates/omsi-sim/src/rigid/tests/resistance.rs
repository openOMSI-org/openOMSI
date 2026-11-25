use super::*;

/// Omsi.exe's body knows no air drag (0x7e2574): the buses' brake scripts put it on their
/// brakes themselves. A bus coasting on the flat without rolling resistance or brakes keeps
/// its speed in the OMSI 2 model.
#[test]
fn omsi_s_body_adds_no_air_drag_of_its_own() {
    let mut rb = RigidBody::from_definition(&bus(), &[]);
    assert!(!rb.realistic);
    rb.rolling_resistance = 0.0;
    rb.place(DVec3::ZERO, 0.0);
    let g = road(1e9, 0.0);
    run(&mut rb, 2.0, 0.0, 0.0, &g);
    rb.velocity = rb.orientation.mul_vec3(Vec3::Y) * 25.0;
    for w in rb.wheels.iter_mut() {
        w.spin = 25.0 / w.radius;
    }
    run(&mut rb, 10.0, 0.0, 0.0, &g);
    // (a drag of 0.36 x area x v2 on top took a whole metre a second off in these 10 s)
    let v = rb.forward_speed();
    assert!(v > 24.7, "{v}");
}

/// A vehicle file without `[rollwiderstand]` rolls without any (Omsi.exe leaves it at 0,
/// 0x7cb3f4); one with it keeps its own figure.
#[test]
fn rolling_resistance_is_the_file_s_or_none() {
    let mut def = bus();
    assert_eq!(
        RigidBody::from_definition(&def, &[]).rolling_resistance,
        1000.0
    );
    def.rolling_resistance = 0.0;
    assert_eq!(
        RigidBody::from_definition(&def, &[]).rolling_resistance,
        0.0
    );
}

/// The OMSI 2 model (the setting off) is as it was: a bus follows its wheels at any speed
/// short of the grip, asphalt is smooth, and no wind acts.
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
    assert!(
        rb.wheels
            .iter()
            .all(|w| w.rough == 0.0 && w.mu_scale == 1.0),
        "no road unevenness or surface grip"
    );
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
