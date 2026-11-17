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

/// Scenery beside the track does not stop a rail vehicle, whose box only guesses at the body
/// in a bend; a buffer stop on the track and another vehicle do, and scenery still stops a bus.
#[test]
fn on_the_rails_only_what_stands_on_the_track_is_in_the_way() {
    let crashes = |on_rails: bool, vehicle_ahead: bool, across: f64| {
        let mut v = vehicle(|d| {
            d.bounding_box = Some([2.0, 2.0, 2.0, 0.0, 3.0, 1.0]);
            d.rolling_resistance = 0.0;
        });
        let ahead = crate::collision::Obb::point(DVec3::new(across, 4.15, 1.0), 0.025);
        let mut world = crate::collision::CollisionWorld::default();
        if vehicle_ahead {
            v.dynamic_boxes.push(ahead);
        } else {
            world.add(ahead);
        }
        v.collision = Some(Arc::new(world));
        if on_rails {
            v.set_rail_env(Some(env(None, None)));
        }
        v.set_speed(2.0);
        v.step_physics(0.1);
        v.crashes
    };
    assert_eq!(
        crashes(true, false, 0.9),
        0,
        "a wall beside the track, inside the body's box"
    );
    assert_eq!(crashes(false, false, 0.9), 1, "the same wall and a bus");
    assert_eq!(crashes(true, false, 0.0), 1, "a buffer stop on the track");
    assert_eq!(crashes(true, true, 0.0), 1, "another vehicle on the track");
    assert_eq!(
        crashes(true, true, 0.9),
        1,
        "another vehicle at the body's side, outside the strip over the track"
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
