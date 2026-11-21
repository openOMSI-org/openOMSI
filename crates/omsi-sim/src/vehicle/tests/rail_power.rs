//! Where a rail vehicle gets its current: the line under the pantograph, the rail beside the shoe.

use super::*;
use omsi_transit::Supply;
use omsi_vehicle::rail_spec::PowerKind;

fn vehicle(edit: impl FnOnce(&mut Vehicle)) -> VehicleInstance {
    let mut ty = (*coupling_test_type(Some(1.0))).clone();
    edit(&mut ty.def);
    VehicleInstance::new(Arc::new(ty), VehicleHost::new(Default::default()))
}

fn env(wire: Option<Supply>, third_rail: Option<Supply>) -> RailEnv {
    RailEnv {
        grade: 0.0,
        radius: f32::INFINITY,
        in_tunnel: false,
        wire,
        third_rail,
    }
}

const LIVE: Supply = Supply {
    volts: 600.0,
    nominal: 600.0,
};
const DEAD: Supply = Supply {
    volts: 0.0,
    nominal: 600.0,
};

/// On the rails a car's body meets scenery at its whole width from the top of a platform
/// up: a pole beside the track inside the body is hit, as are a buffer stop and another
/// vehicle; a platform's edge under the body, which the real car overhangs, is not.
#[test]
fn on_the_rails_the_body_hits_what_stands_beside_it_and_overhangs_a_platform() {
    let crashes = |on_rails: bool, vehicle_ahead: bool, obstacle: crate::collision::Obb| {
        let mut v = vehicle(|d| {
            d.bounding_box = Some([2.0, 2.0, 2.0, 0.0, 3.0, 1.0]);
            d.rolling_resistance = 0.0;
        });
        let mut world = crate::collision::CollisionWorld::default();
        if vehicle_ahead {
            v.dynamic_boxes.push(obstacle);
        } else {
            world.add(obstacle);
        }
        v.collision = Some(Arc::new(world));
        if on_rails {
            v.set_rail_env(Some(env(None, None)));
        }
        v.set_speed(2.0);
        v.step_physics(0.1);
        v.crashes
    };
    let pole =
        |across: f64, z: f64| crate::collision::Obb::point(DVec3::new(across, 4.15, z), 0.025);
    // a platform 30 cm high, its edge 0.8 m from the track's middle, just ahead of the car
    let platform = crate::collision::Obb::from_box(
        [2.2, 2.9, 0.3, 0.0, 0.0, 0.15],
        DVec3::new(1.9, 5.55, 0.0),
        0.0,
    );
    assert_eq!(
        crashes(true, false, pole(0.9, 1.0)),
        1,
        "a pole beside the track, inside the body"
    );
    assert_eq!(
        crashes(false, false, pole(0.9, 1.0)),
        1,
        "the same pole and a bus"
    );
    assert_eq!(
        crashes(true, false, platform),
        0,
        "a platform's edge under the body"
    );
    assert_eq!(
        crashes(false, false, platform),
        1,
        "the same platform and a bus"
    );
    assert_eq!(
        crashes(true, false, pole(0.0, 1.0)),
        1,
        "a buffer stop on the track"
    );
    assert_eq!(
        crashes(true, false, pole(0.0, 0.1)),
        1,
        "something low on the track"
    );
    assert_eq!(
        crashes(true, true, pole(0.0, 1.0)),
        1,
        "another vehicle on the track"
    );
    assert_eq!(
        crashes(true, true, pole(0.9, 1.0)),
        1,
        "another vehicle at the body's side"
    );
}

/// In a bend each car meets scenery where it stands: a pole beside the second car's body,
/// which the leading car's box does not reach, is in the way, and a platform's edge under
/// that car is not.
#[test]
fn in_a_bend_each_car_meets_what_stands_beside_its_own_body() {
    let bb = [2.4, 10.0, 3.0, 0.0, 0.0, 1.5];
    let ty = {
        let mut ty = (*coupling_test_type(Some(1.0))).clone();
        ty.def.bounding_box = Some(bb);
        Arc::new(ty)
    };
    let mut v = VehicleInstance::new(ty.clone(), VehicleHost::new(Default::default()));
    v.attach_trailer_ex(ty, false);
    v.set_rail_env(Some(env(None, None)));
    // the second car 20° round the bend behind the first's rear coupling
    let h = 340.0f64;
    let (fwd, right) = {
        let r = h.to_radians();
        (
            DVec3::new(r.sin(), r.cos(), 0.0),
            DVec3::new(r.cos(), -r.sin(), 0.0),
        )
    };
    let middle = DVec3::new(0.0, -5.0, 0.0) - fwd * 5.0;
    (v.trailers[0].position, v.trailers[0].heading) = (middle, h);
    let boxes = v.scenery_boxes(bb, (v.position, v.body_heading()), DVec3::ZERO);
    let lead = crate::collision::Obb::from_box(bb, v.position, v.body_heading());
    let beside = middle + right * 1.0;
    let pole = crate::collision::Obb::point(beside + DVec3::Z * 1.5, 0.025);
    assert!(
        !pole.overlaps(&lead),
        "the leading car's box does not reach it"
    );
    assert!(
        boxes.iter().any(|b| pole.overlaps(b)),
        "the second car's body does"
    );
    let platform = crate::collision::Obb::point(beside + DVec3::Z * 0.15, 0.15);
    assert!(
        !boxes.iter().any(|b| platform.overlaps(b)),
        "a platform's edge under the second car's body"
    );
}

/// A car that drives into the side of a standing tram hits it: against other vehicles a rail
/// vehicle keeps its whole width, not only the strip over the track.
#[test]
fn a_car_into_the_side_of_a_tram_hits_it() {
    let mut v = vehicle(|d| d.bounding_box = Some([2.4, 10.0, 3.0, 0.0, 0.0, 1.5]));
    v.set_rail_env(Some(env(None, None)));
    v.collision = Some(Arc::new(crate::collision::CollisionWorld::default()));
    let car = crate::collision::Obb::from_box(
        [1.8, 4.0, 1.5, 0.0, 0.0, 0.75],
        DVec3::new(3.1, 1.0, 0.0),
        90.0,
    )
    .moving(glam::DVec2::new(-4.0, 0.0), 1200.0, 1);
    v.dynamic_boxes.push(car);
    v.step_physics(0.1);
    assert_eq!(
        v.crashes, 1,
        "its nose 1.1 m from the middle: outside the 0.7 m strip, inside the body"
    );
}

#[test]
fn a_map_without_a_line_takes_the_current_for_granted() {
    let mut v = vehicle(|d| d.rail_power = PowerKind::Catenary);
    v.set_rail_env(Some(env(None, None)));
    assert!(v.rail_powered());
    assert_eq!(v.rail_supply(), None);
}

#[test]
fn a_catenary_vehicle_needs_the_wire_to_be_live_under_it() {
    let mut v = vehicle(|d| d.rail_power = PowerKind::Catenary);
    v.set_rail_env(Some(env(Some(LIVE), Some(DEAD))));
    assert!(v.rail_powered(), "the third rail does not matter to it");
    v.set_rail_env(Some(env(Some(DEAD), Some(LIVE))));
    assert!(
        !v.rail_powered(),
        "a dead section stops it, a live third rail does not help"
    );
}

#[test]
fn a_third_rail_vehicle_follows_the_rail() {
    let mut v = vehicle(|d| d.contact_shoes.push([1.0, 0.0, 0.0, 0.0, 0.0, 0.0]));
    assert_eq!(
        v.rail_power_kind(),
        PowerKind::ThirdRail,
        "a contact shoe says so"
    );
    v.set_rail_env(Some(env(Some(DEAD), Some(LIVE))));
    assert!(v.rail_powered());
    v.set_rail_env(Some(env(Some(LIVE), Some(DEAD))));
    assert!(!v.rail_powered());
}

#[test]
fn an_onboard_power_vehicle_needs_no_line() {
    let mut v = vehicle(|d| d.rail_power = PowerKind::OnBoard);
    v.set_rail_env(Some(env(Some(DEAD), Some(DEAD))));
    assert!(v.rail_powered());
}

#[test]
fn a_vehicle_that_says_nothing_is_as_it_was() {
    let mut v = vehicle(|_| {});
    assert_eq!(v.rail_power_kind(), PowerKind::OnBoard);
    v.set_rail_env(Some(env(Some(DEAD), Some(DEAD))));
    assert!(v.rail_powered());
}

#[test]
fn a_rail_wheel_turns_once_with_the_rails() {
    let mut v = vehicle(|d| d.kind = omsi_vehicle::VehicleKind::Other(2));
    v.set_rail_env(Some(env(None, None)));
    v.physics.speed = 10.0;
    let before: Vec<(f32, f32)> = v
        .physics
        .wheels
        .iter()
        .flatten()
        .map(|w| (w.turned, w.radius))
        .collect();
    assert!(!before.is_empty());
    v.step_physics(0.1);
    for (w, (t, _)) in v.physics.wheels.iter().flatten().zip(&before) {
        let expected = v.physics.speed / w.radius.max(0.05) * 0.1;
        assert!(
            (w.turned - t - expected).abs() < 1e-3,
            "turned {} expected {expected}",
            w.turned - t
        );
    }
}
