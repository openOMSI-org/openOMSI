#[test]
fn plugins_default_off_and_explicit_preferences_survive_settings_updates() {
    // Only a truly new installation defaults optional plugins off; legacy files
    // without the key keep their old plugins and vehicle extensions enabled.
    assert_eq!(settings_from_text(None)["disable_plugins"], json!(true));
    for old in ["discord_status=1\n", "version=4\n", ""] {
        assert_eq!(settings_from_text(Some(old))["disable_plugins"], json!(false));
    }
    // Preserve existing saved choices in both directions.
    for (saved_value, disabled) in [("0", false), ("1", true)] {
        let old = format!("disable_plugins={saved_value}\n");
        let mut settings = settings_from_text(Some(&old));
        assert_eq!(settings["disable_plugins"], json!(disabled));
        settings["discord_status"] = json!(false);
        let updated = settings_to_text(&settings, Some(&format!("disable_plugins=1\n{old}")));
        assert_eq!(updated.lines().filter(|line| line.starts_with("disable_plugins=")).count(), 1);
        assert_eq!(settings_from_text(Some(&updated))["disable_plugins"], json!(disabled));
    }
    // Explicitly enabling plugins remains possible and survives a later save.
    let mut settings = settings_from_text(None);
    settings["disable_plugins"] = json!(false);
    let saved = settings_to_text(&settings, None);
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
        // The default and even incomplete settings should use the safe path.
        for (settings, disabled) in [
            (settings_from_text(None), true),
            (json!({}), true),
            (json!({"disable_plugins": true}), true),
            (json!({"disable_plugins": false}), false),
        ] {
            let args = duty_args_with_settings(&duty, root, &settings).unwrap();
            assert!(args.iter().any(|s| s == entry), "{entry}: {args:?}");
            assert_eq!(args.iter().filter(|s| *s == "--no-plugins").count(), usize::from(disabled), "{entry}: {args:?}");
            let root_arg = args.iter().position(|s| s == "--root").unwrap();
            assert_eq!(args[root_arg + 1], root.to_string_lossy());
        }
    }
}
