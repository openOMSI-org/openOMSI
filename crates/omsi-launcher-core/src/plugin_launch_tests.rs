#[test]
fn disabling_plugins_is_opt_in_and_survives_settings_updates() {
    for old in [None, Some("discord_status=1\n"), Some("disable_plugins=0\n")] {
        assert_eq!(settings_from_text(old)["disable_plugins"], json!(false));
    }
    let mut settings = settings_from_text(Some("disable_plugins=1\n"));
    assert_eq!(settings["disable_plugins"], json!(true));
    // Changing an unrelated option must not silently enable plugins again.
    settings["discord_status"] = json!(false);
    let saved = settings_to_text(&settings, Some("disable_plugins=0\ndisable_plugins=1\n"));
    assert_eq!(saved.lines().filter(|s| s.starts_with("disable_plugins=")).count(), 1);
    assert_eq!(settings_from_text(Some(&saved))["disable_plugins"], json!(true));
    settings["disable_plugins"] = json!(false);
    let saved = settings_to_text(&settings, Some(&saved));
    assert_eq!(settings_from_text(Some(&saved))["disable_plugins"], json!(false));
}

#[test]
fn plugin_preference_reaches_new_saved_and_tutorial_sessions() {
    let root = std::path::Path::new("test-omsi");
    let driving = Duty {
        map: "maps/x/global.cfg".into(),
        bus: "Vehicles/x.bus".into(),
        time: "09:00".into(),
        ..Default::default()
    };
    for (duty, entry) in [
        (driving, "--map"),
        (Duty { situation: Some("saved.osn".into()), ..Default::default() }, "--situation"),
        (Duty { tutorial: Some(1), ..Default::default() }, "--tutorial"),
    ] {
        for disabled in [false, true] {
            let settings = json!({"disable_plugins": disabled});
            let args = duty_args_with_settings(&duty, root, &settings).unwrap();
            assert!(args.iter().any(|s| s == entry), "{entry}: {args:?}");
            assert_eq!(args.iter().filter(|s| *s == "--no-plugins").count(), usize::from(disabled), "{entry}: {args:?}");
            let root_arg = args.iter().position(|s| s == "--root").unwrap();
            assert_eq!(args[root_arg + 1], root.to_string_lossy());
        }
    }
}
