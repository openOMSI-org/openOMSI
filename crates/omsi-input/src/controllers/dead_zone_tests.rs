//! #1344: an old pedal that trembles at rest gets a dead zone of its own, without taking the
//! wheel's away.

use super::{Func, Zones, axis_zone};

#[test]
fn an_axis_without_a_zone_of_its_own_follows_the_general_setting() {
    let z = Zones::parse("throttle=0.12");
    assert_eq!(z.throttle, Some(0.12));
    assert_eq!(z.steer, None, "the wheel was not mentioned");
    assert_eq!(z.of(Func::Throttle, 0.05), 0.12);
    assert_eq!(z.of(Func::Steering, 0.05), 0.05, "the settings' own");
    assert_eq!(z.of(Func::Brake, 0.07), 0.07);
    // nothing in the setting at all: the general zone everywhere, as before
    assert_eq!(Zones::default().of(Func::Clutch, 0.09), 0.09);
}

#[test]
fn the_setting_is_read_back_as_it_was_written() {
    let s = "steer=0.05;throttle=0.12;brake=0.075;throttlebrake=0.09;clutch=0";
    let z = Zones::parse(s);
    assert_eq!(
        z,
        Zones {
            steer: Some(0.05),
            throttle: Some(0.12),
            brake: Some(0.075),
            throttlebrake: Some(0.09),
            clutch: Some(0.0)
        }
    );
    assert_eq!(z.write(), s);
    assert_eq!(
        Zones::parse(&z.write()),
        z,
        "written and read again, the same"
    );
    // the other names, a decimal comma, spacing, and a value out of range
    let z = Zones::parse(" gas = 0,9 ; steering=bad ; brake=-1 ; throttlebrake=9 ; nonsense=0.1 ");
    assert_eq!(
        z.throttle,
        Some(Zones::MAX),
        "clamped to the widest the setting allows"
    );
    assert_eq!(z.steer, None, "unreadable");
    assert_eq!(z.brake, Some(0.0), "nothing narrower than nothing");
    assert_eq!(z.throttlebrake, Some(Zones::MAX));
    assert_eq!(Zones::parse("").write(), "", "an empty setting stays empty");
}

#[test]
fn a_pedal_that_trembles_at_rest_is_held_at_its_rest_by_its_own_zone() {
    // the axis a fifth of the way down (-0.6), read by the general zone and by a wide one
    let trembling = -0.6;
    let general = axis_zone(trembling, Func::Throttle, 0.1);
    let own = axis_zone(
        trembling,
        Func::Throttle,
        Zones::parse("throttle=0.3").of(Func::Throttle, 0.1),
    );
    // after the pedal's ends the general zone still lets a little throttle through...
    let through = |v: f32| crate::pedal::pedal_ends(((v + 1.0) * 0.5).clamp(0.0, 1.0));
    assert!(through(general) > 0.0, "{general}");
    assert_eq!(through(own), 0.0, "the wide zone puts it back at rest");
    // a pedal inside its zone is at rest
    assert_eq!(through(axis_zone(-0.9, Func::Throttle, 0.12)), 0.0);
    assert!(
        through(axis_zone(-0.9, Func::Throttle, 0.1)) == 0.0,
        "and is so with the general one too"
    );
    // a combined pedal is read about its centre, where the two halves meet
    assert_eq!(
        axis_zone(-0.05, Func::ThrottleBrake, 0.12),
        0.0,
        "the two halves' meeting point"
    );
    assert!(axis_zone(0.5, Func::ThrottleBrake, 0.12) > 0.0);
}

#[test]
fn the_combined_axis_has_its_own_override_apart_from_the_pedals() {
    let z = Zones::parse("throttlebrake=0.12;throttle=0.2;brake=0.08");
    assert_eq!(z.of(Func::ThrottleBrake, 0.04), 0.12);
    assert_eq!(z.of(Func::Throttle, 0.04), 0.2);
    assert_eq!(z.of(Func::Brake, 0.04), 0.08);
    for spec in ["throttle=0.12", "brake=0.08", "throttle=0.12;brake=0.08"] {
        assert_eq!(
            Zones::parse(spec).of(Func::ThrottleBrake, 0.04),
            0.04,
            "{spec} only moves its own pedal"
        );
    }
    assert_eq!(Zones::default().of(Func::ThrottleBrake, 0.04), 0.04);
    assert_eq!(Zones::parse("steer=0.2").of(Func::LookX, 0.0), 0.0);
}
