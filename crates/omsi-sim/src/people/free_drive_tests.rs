//! #2712: the stops of the map fill in free drive, where no trip of the timetable is due
//! anywhere - as OMSI's passengers turned up whatever the player was driving.

use super::free_drive::{FreeDriveFill, stop_served};

#[test]
fn the_setting_spells_the_three_ways() {
    assert_eq!(
        FreeDriveFill::default(),
        FreeDriveFill::Auto,
        "as OMSI had it"
    );
    for on in ["on", "1", "yes", "true", "always", " ON "] {
        assert_eq!(FreeDriveFill::from_setting(on), FreeDriveFill::On, "{on:?}");
    }
    for off in ["off", "0", "no", "false", "never"] {
        assert_eq!(
            FreeDriveFill::from_setting(off),
            FreeDriveFill::Off,
            "{off:?}"
        );
    }
    for auto in ["auto", "", "nonsense"] {
        assert_eq!(
            FreeDriveFill::from_setting(auto),
            FreeDriveFill::Auto,
            "{auto:?}"
        );
    }
    // and what it writes is read back as itself
    for f in [FreeDriveFill::Auto, FreeDriveFill::On, FreeDriveFill::Off] {
        assert_eq!(FreeDriveFill::from_setting(f.setting_name()), f);
    }
}

#[test]
fn free_drive_fills_the_stops_and_a_duty_keeps_to_the_timetable() {
    assert!(
        FreeDriveFill::Auto.fills(false),
        "no duty: the map fills as in OMSI"
    );
    assert!(
        !FreeDriveFill::Auto.fills(true),
        "on a duty: only the trips due"
    );
    assert!(FreeDriveFill::On.fills(true), "always");
    assert!(FreeDriveFill::On.fills(false));
    assert!(!FreeDriveFill::Off.fills(false), "never");
    assert!(!FreeDriveFill::Off.fills(true));
}

#[test]
fn a_stop_is_filled_unless_the_timetable_is_worked_and_has_nothing_due_there() {
    // free drive, a timetable in the map: filled by the stop's own numbers
    assert_eq!(
        stop_served(true, true, true, false),
        1.0,
        "nothing due anywhere in free drive"
    );
    assert_eq!(
        stop_served(true, true, false, false),
        1.0,
        "not even a line listed"
    );
    // on a duty: as before - a stop with no trip due, or no line, stays empty
    assert_eq!(stop_served(true, false, true, false), 0.0_f32);
    assert_eq!(
        stop_served(true, false, false, true),
        0.0,
        "no line listed there"
    );
    assert_eq!(stop_served(true, false, true, true), 1.0, "a trip is due");
    // a map with no timetable at all: everything fills, as it always did
    assert_eq!(stop_served(false, false, false, false), 1.0_f32);
}
