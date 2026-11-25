use super::*;

#[test]
fn pax_stop_numbers_exact_round_trips_and_saves() {
    let mut s = Settings::desktop();
    assert!(!s.pax_stop_numbers_exact);
    assert!(!Settings::from_text(&s.to_text()).pax_stop_numbers_exact);

    s.pax_stop_numbers_exact = true;
    let serialized = s.to_text();
    let reloaded = Settings::from_text(&serialized);
    assert!(reloaded.pax_stop_numbers_exact);

    let session = crate::startup::SessionSettings {
        pax_stop_numbers_exact: reloaded.pax_stop_numbers_exact,
        ..Default::default()
    };
    assert!(session.pax_stop_numbers_exact);
}
