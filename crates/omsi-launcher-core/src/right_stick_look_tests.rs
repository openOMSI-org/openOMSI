#[test]
fn launcher_saves_the_switch_as_a_boolean_without_duplicate_keys() {
    assert_eq!(
        super::settings_from_text(None)["right_stick_look"],
        serde_json::json!(true)
    );
    for enabled in [false, true] {
        let mut settings = super::settings_from_text(None);
        settings["right_stick_look"] = serde_json::json!(enabled);
        let saved = super::settings_to_text(&settings, Some("right_stick_look=1\n"));
        assert_eq!(
            saved
                .lines()
                .filter(|line| line.starts_with("right_stick_look="))
                .count(),
            1
        );
        assert_eq!(
            super::settings_from_text(Some(&saved))["right_stick_look"],
            serde_json::json!(enabled)
        );
    }
}
