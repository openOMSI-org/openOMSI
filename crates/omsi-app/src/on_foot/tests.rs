//! Tests of walking: steps, slopes, kneeling and the cab's sound.

use super::*;

fn walker() -> OnFoot {
    OnFoot {
        pos: DVec3::ZERO,
        heading: 0.0,
        vel: DVec2::ZERO,
        lift: 0.0,
        vz: 0.0,
        seat: None,
        inside: None,
        cam: FootCam::First,
        yaw: 0.0,
        pitch: 0.0,
        eye: None,
        eye_yaw: 0.0,
        lag: DVec3::ZERO,
        settle: 0.0,
        view_before: "driver".into(),
        kind: 0,
        face_seat: false,
        transit: None,
        arrive: None,
        kneel: false,
        crouch: 0.0,
        door_request: None,
        door_request_send: 0.0,
        footing: None,
        bus_turn: None,
    }
}

/// The bus's sound fades in as the eyes come in through its wall, inside its box (and
/// a coupled part's), and not at all from outside.
#[test]
fn the_cab_is_heard_by_how_far_in_the_eyes_are() {
    let mut def = omsi_vehicle::Vehicle::default();
    def.bounding_box = Some([2.5, 12.0, 3.0, 0.0, -0.5, 1.5]);
    let ty = Arc::new(omsi_sim::VehicleType {
        def,
        model: Default::default(),
        model_dir: Default::default(),
        program: Arc::new(omsi_script::compile(&Default::default())),
        meshes: Vec::new(),
        lod_meshes: Vec::new(),
        paint_schemes: Vec::new(),
        texchanges: Vec::new(),
        wheel_meshes: Vec::new(),
        suspension_axles: Vec::new(),
        missing_packs: Vec::new(),
        mesh_bounds: Vec::new(),
        mesh_boxes: Vec::new(),
    });
    let mut v = omsi_sim::VehicleInstance::new(ty, omsi_sim::VehicleHost::new(Default::default()));
    v.position = DVec3::new(1200.0, -300.0, 40.0);
    v.heading = 30.0;
    let bb = v.ty.def.bounding_box.expect("a bounding box");
    let xf = v.world_transform();
    let at = |x: f32, y: f32| xf.transform_point3(glam::Vec3::new(x, y, bb[5])).as_dvec3();
    let wall = bb[3] + bb[0] * 0.5;
    assert_eq!(super::inside_amount(&v, at(wall + 1.0, bb[4])), 0.0, "outside");
    assert_eq!(super::inside_amount(&v, at(bb[3], bb[4])), 1.0, "the aisle");
    let half = super::inside_amount(&v, at(wall - super::CAB_SOUND_DEPTH * 0.5, bb[4]));
    assert!((half - 0.5).abs() < 0.02, "{half}");
}

/// Walking north up made-up ground: a 12 cm kerb is a step; a 55 degree embankment is
/// climbed one step high however finely it is walked; a 20 degree path all the way.
#[test]
fn embankments_stop_a_step_above_the_footing() {
    let kerb = |_: f64, y: f64| Some(if y >= 0.0 { 0.12 } else { 0.0 });
    assert!(ground_gradient(kerb, DVec2::new(3.0, 0.02), 0.12) <= MAX_WALK_GRADIENT, "a kerb is no embankment");
    let slope = |deg: f64| move |_: f64, y: f64| Some(y.max(0.0) * deg.to_radians().tan());
    let walk_north = |ground: &dyn Fn(f64, f64) -> Option<f64>| {
        let (mut y, mut z) = (-0.8, 0.0);
        let mut footing = 0.0;
        for _ in 0..1000 {
            let next = DVec2::new(3.0, y + 0.005);
            let g = ground(next.x, next.y).unwrap();
            let gradient = ground_gradient(ground, next, g);
            if g - z > STEP || too_steep_to_climb(g, gradient, footing) {
                return z;
            }
            if gradient <= MAX_WALK_GRADIENT {
                footing = g;
            }
            (y, z) = (next.y, g);
        }
        z
    };
    let embankment = walk_north(&slope(55.0));
    // (its foot, within the half span the gradient is taken over, still gives footing)
    let foot_allowance = GRADIENT_HALF_SPAN * 55f64.to_radians().tan();
    assert!(embankment > STEP && embankment < STEP + foot_allowance, "climbed to {embankment:.2} m");
    assert!(walk_north(&slope(20.0)) > 1.5, "the path");
}

/// C kneels: the eyes go down to about a metre over a moment (a picture from below,
/// #1148), the walk becomes a shuffle, and C again stands up; not on a seat.
#[test]
fn kneeling_lowers_the_eyes_and_slows_the_walk() {
    let mut f = walker();
    assert_eq!(f.eye_drop(), DVec3::ZERO);
    assert_eq!(f.pace(false), WALK);
    assert!(f.toggle_kneel() && f.kneel);
    f.ease_crouch(0.05);
    assert!(f.crouch > 0.2 && f.crouch < 0.9, "{}", f.crouch);
    for _ in 0..60 {
        f.ease_crouch(1.0 / 60.0);
    }
    assert_eq!(f.crouch, 1.0);
    assert_eq!(f.eye_drop().z, -KNEEL_DROP);
    assert_eq!((f.pace(false), f.pace(true)), (KNEEL_WALK, KNEEL_WALK));
    assert!(f.toggle_kneel() && !f.kneel);
    for _ in 0..60 {
        f.ease_crouch(1.0 / 60.0);
    }
    assert_eq!(f.eye_drop(), DVec3::ZERO);
    assert_eq!(f.pace(true), RUN);
    // (seated: nothing to kneel on)
    f.seat = Some((BusId::Player, 3));
    assert!(!f.toggle_kneel() && !f.kneel);
}

#[test]
fn the_walker_gets_up_to_pace_and_stops_over_steps_not_at_once() {
    let mut v = DVec2::ZERO;
    let want = DVec2::new(0.0, 1.4);
    // not at pace after a tenth of a second, at pace within half a second
    v = approach_velocity(v, want, 0.1, true);
    assert!(v.y > 0.2 && v.y < 0.5, "{v:?}");
    for _ in 0..40 {
        v = approach_velocity(v, want, 0.01, true);
    }
    assert!((v - want).length() < 1e-9);
    // stopping from a run takes more than a frame and less than a second
    let mut v = DVec2::new(0.0, 3.8);
    v = approach_velocity(v, DVec2::ZERO, 0.016, true);
    assert!(v.y > 3.5);
    for _ in 0..60 {
        v = approach_velocity(v, DVec2::ZERO, 1.0 / 60.0, true);
    }
    assert_eq!(v, DVec2::ZERO);
}

#[test]
fn a_turn_is_a_curve_with_no_speed_lost_to_a_snap() {
    // from walking north to walking east: the speed dips but never jumps
    let mut v = DVec2::new(0.0, 1.4);
    let want = DVec2::new(1.4, 0.0);
    let mut last = v;
    for _ in 0..200 {
        v = approach_velocity(v, want, 1.0 / 120.0, true);
        assert!((v - last).length() <= 6.5 / 120.0 + 1e-9);
        last = v;
    }
    assert!((v - want).length() < 1e-9);
}
