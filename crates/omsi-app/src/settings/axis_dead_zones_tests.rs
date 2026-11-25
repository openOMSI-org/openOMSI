use super::*;

#[test]
fn the_dead_zone_of_each_axis_round_trips_through_settings() {
    let mut s = Settings::desktop();
    s.ctrl_deadzone = 0.05;
    s.ctrl_deadzone_axes = "throttle=0.12;brake=0.08;throttlebrake=0.09".into();
    let back = Settings::from_text(&s.to_text());
    assert!((back.ctrl_deadzone - 0.05).abs() < 1e-4);
    assert_eq!(
        back.ctrl_deadzone_axes,
        "throttle=0.12;brake=0.08;throttlebrake=0.09"
    );
}
