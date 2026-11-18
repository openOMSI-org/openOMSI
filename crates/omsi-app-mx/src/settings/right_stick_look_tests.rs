#[test]
fn existing_settings_keep_automatic_look_and_the_switch_round_trips() {
    assert!(super::Settings::from_text("").right_stick_look);
    for enabled in [false, true] {
        let settings = super::Settings::from_text(&format!("right_stick_look={}\n", enabled as u8));
        assert_eq!(settings.right_stick_look, enabled);
        assert_eq!(super::Settings::from_text(&settings.to_text()).right_stick_look, enabled);
    }
}
