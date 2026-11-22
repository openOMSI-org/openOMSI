use super::*;

#[test]
fn pax_late_tolerance_min_round_trips_and_defaults() {
    assert_eq!(Settings::desktop().pax_late_tolerance_min, 5.0);
    assert_eq!(
        Settings::from_text("pax_late_tolerance_min=12.5\n").pax_late_tolerance_min,
        12.5
    );
    let mut s = Settings::desktop();
    s.pax_late_tolerance_min = 10.0;
    assert_eq!(
        Settings::from_text(&s.to_text()).pax_late_tolerance_min,
        10.0
    );
}
