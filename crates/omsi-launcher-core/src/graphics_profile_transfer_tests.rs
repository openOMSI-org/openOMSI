use super::graphics_profile_transfer::*;
use super::graphics_profiles::*;
use super::*;
use serde_json::json;
use std::path::PathBuf;

fn folder(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "openomsi-profile-transfer-{tag}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn graphics_profile_transfer_roundtrip_filters_machine_settings() {
    let dir = folder("roundtrip");
    let source = dir.join("source");
    let destination = dir.join("destination");
    let settings = json!({"msaa": 4, "graphics": "enhanced", "fullscreen": true, "graphics_api": "vulkan", "language": "CHT"});
    save_graphics_profile_in(&source, "Shared", &settings).unwrap();
    let path = dir.join("shared.json");
    export_graphics_profile_in(&source, "Shared", &path).unwrap();
    let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        file,
        json!({"name": "Shared", "graphics": {"msaa": 4, "graphics": "enhanced"}})
    );
    assert_eq!(
        import_graphics_profile_in(&destination, &path).unwrap(),
        "Shared"
    );
    assert_eq!(
        graphics_profiles_in(&destination)["Shared"],
        file["graphics"]
    );
    assert!(export_graphics_profile_in(&source, "Missing", &path).is_err());
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap(),
        file
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn graphics_profile_transfer_preserves_same_name_and_long_unicode_names() {
    let dir = folder("names");
    let source = dir.join("source");
    let destination = dir.join("destination");
    let name = "圖".repeat(40);
    save_graphics_profile_in(&source, &name, &json!({"msaa": 8})).unwrap();
    save_graphics_profile_in(&destination, &name, &json!({"msaa": 2})).unwrap();
    let path = graphics_profile_file(&source, &name);
    let first = import_graphics_profile_in(&destination, &path).unwrap();
    let second = import_graphics_profile_in(&destination, &path).unwrap();
    assert_eq!(first, format!("{} (2)", "圖".repeat(36)));
    assert_eq!(second, format!("{} (3)", "圖".repeat(36)));
    let profiles = graphics_profiles_in(&destination);
    assert_eq!(profiles.len(), 3);
    assert_eq!(profiles[&name]["msaa"], 2);
    assert_eq!(profiles[&first]["msaa"], 8);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn graphics_profile_transfer_rejects_invalid_files_without_writing() {
    let dir = folder("invalid");
    let destination = dir.join("destination");
    let path = dir.join("input.json");
    for text in [
        "not json",
        "[]",
        "{}",
        r#"{"name":" ","graphics":{"msaa":4}}"#,
        r#"{"name":"X","graphics":[]}"#,
        r#"{"name":"X","graphics":{}}"#,
        r#"{"name":"X","graphics":{"language":"ENG"}}"#,
        r#"{"name":"X","graphics":{"msaa":[]}}"#,
        r#"{"name":"X","graphics":{"msaa":"oops"}}"#,
        r#"{"name":"X","graphics":{"shadows":"yes"}}"#,
        r#"{"name":"X","graphics":{"lamp_shadows":"true"}}"#,
        r#"{"name":"X","graphics":{"lamp_shadows":1}}"#,
        r#"{"name":"X","graphics":{"graphics":42}}"#,
        r#"{"name":"X","graphics":{"upscaler":42}}"#,
        r#"{"name":"X","graphics":{"graphics":"enhanced\n[fullscreen]"}}"#,
    ] {
        std::fs::write(&path, text).unwrap();
        assert!(
            import_graphics_profile_in(&destination, &path).is_err(),
            "{text}"
        );
        assert!(!destination.exists());
    }
    std::fs::write(&path, " ".repeat(65537)).unwrap();
    assert!(import_graphics_profile_in(&destination, &path).is_err());
    assert!(!destination.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn graphics_profile_transfer_accepts_legacy_files_and_numeric_choices() {
    let dir = folder("legacy");
    let destination = dir.join("destination");
    let path = dir.join("friend.json");
    let file = json!({"name":"  Friend  ","graphics":{"msaa":"4","render_scale":1.0,"reflections":true,"fullscreen":true,"future_option":{}}});
    std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
    assert_eq!(
        import_graphics_profile_in(&destination, &path).unwrap(),
        "Friend"
    );
    assert_eq!(
        graphics_profiles_in(&destination)["Friend"],
        json!({"msaa":4,"render_scale":1.0,"reflections":true})
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn graphics_profile_transfer_accepts_every_current_preset_and_default() {
    let dir = folder("presets");
    let mut profiles = vec![
        ("Desktop default", settings_from_text_on(None, false)),
        ("Phone default", settings_from_text_on(None, true)),
    ];
    profiles.extend(graphics_presets());
    for (name, settings) in profiles {
        save_graphics_profile_in(&dir, name, &settings).unwrap();
        let path = dir.join("export.json");
        export_graphics_profile_in(&dir, name, &path).unwrap();
        let imported = import_graphics_profile_in(&dir, &path).unwrap();
        let all = graphics_profiles_in(&dir);
        assert_eq!(all[&imported], all[name]);
        for phone in [false, true] {
            let persisted = settings_from_text_on(Some(&settings_to_text(&settings, None)), phone);
            save_graphics_profile_in(&dir, name, &persisted).unwrap();
            export_graphics_profile_in(&dir, name, &path).unwrap();
            let imported = import_graphics_profile_in(&dir, &path).unwrap();
            let all = graphics_profiles_in(&dir);
            assert_eq!(all[&imported], all[name], "{name}, phone={phone}");
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn graphics_profile_transfer_normalizes_before_checking_for_collisions() {
    let dir = folder("normalized");
    let destination = dir.join("destination");
    let path = dir.join("friend.json");
    save_graphics_profile_in(&destination, "A", &json!({"msaa": 2})).unwrap();
    let file = json!({"name": format!("A{}B", " ".repeat(39)), "graphics": {"msaa": 8}});
    std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
    assert_eq!(
        import_graphics_profile_in(&destination, &path).unwrap(),
        "A (2)"
    );
    let all = graphics_profiles_in(&destination);
    assert_eq!(all["A"]["msaa"], 2);
    assert_eq!(all["A (2)"]["msaa"], 8);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn graphics_profile_transfer_normalizes_float_strings_before_applying() {
    let dir = folder("numeric-strings");
    let destination = dir.join("destination");
    let path = dir.join("friend.json");
    let file = json!({"name":"Sharpness", "graphics":{"fsr_sharpness":"0.25", "msaa":"8"}});
    std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
    import_graphics_profile_in(&destination, &path).unwrap();
    let all = graphics_profiles_in(&destination);
    assert_eq!(all["Sharpness"]["fsr_sharpness"], 0.25);
    assert_eq!(all["Sharpness"]["msaa"], 8);
    let mut settings = settings_from_text(None);
    apply_graphics_profile(&all["Sharpness"], &mut settings);
    let applied = settings_from_text(Some(&settings_to_text(&settings, None)));
    assert_eq!(applied["fsr_sharpness"], 0.25);
    assert_eq!(applied["msaa"], 8);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn graphics_profile_transfer_replaces_exports_and_cleans_up_failed_replacements() {
    let dir = folder("atomic-export");
    save_graphics_profile_in(&dir, "Shared", &json!({"msaa":4})).unwrap();
    let path = dir.join("shared.json");
    std::fs::write(&path, "old export").unwrap();
    export_graphics_profile_in(&dir, "Shared", &path).unwrap();
    let value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(value["graphics"]["msaa"], 4);
    let blocked = dir.join("blocked.json");
    std::fs::create_dir(&blocked).unwrap();
    std::fs::write(blocked.join("keep"), "unchanged").unwrap();
    assert!(export_graphics_profile_in(&dir, "Shared", &blocked).is_err());
    assert_eq!(
        std::fs::read_to_string(blocked.join("keep")).unwrap(),
        "unchanged"
    );
    assert!(!std::fs::read_dir(&dir).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn graphics_profile_transfer_accepts_settings_written_and_read_by_the_launcher() {
    let dir = folder("persisted-settings");
    let settings = settings_from_text(Some(&settings_to_text(&settings_from_text(None), None)));
    assert_eq!(settings["lamp_shadows"], true);
    for key in GRAPHICS_PROFILE_KEYS {
        assert!(!settings[key].is_null(), "missing persisted setting: {key}");
    }
    save_graphics_profile_in(&dir, "Persisted", &settings).unwrap();
    let path = dir.join("shared.json");
    export_graphics_profile_in(&dir, "Persisted", &path).unwrap();
    let imported = import_graphics_profile_in(&dir, &path).unwrap();
    assert_eq!(
        graphics_profiles_in(&dir)[&imported],
        graphics_profiles_in(&dir)["Persisted"]
    );
    let mut disabled = settings.clone();
    disabled["lamp_shadows"] = json!(false);
    save_graphics_profile_in(&dir, "Disabled", &disabled).unwrap();
    export_graphics_profile_in(&dir, "Disabled", &path).unwrap();
    let imported = import_graphics_profile_in(&dir, &path).unwrap();
    assert_eq!(graphics_profiles_in(&dir)[&imported]["lamp_shadows"], false);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn graphics_profile_transfer_checks_each_supported_field_independently() {
    let dir = folder("all-fields");
    let path = dir.join("shared.json");
    let raw = settings_from_text(None);
    let persisted = settings_from_text(Some(&settings_to_text(&raw, None)));
    for key in GRAPHICS_PROFILE_KEYS {
        if raw[key].is_null() {
            println!("Missing from unsaved defaults: {key}");
        } else {
            assert_eq!(
                std::mem::discriminant(&raw[key]),
                std::mem::discriminant(&persisted[key]),
                "type changed after saving: {key}"
            );
        }
        let mut value = persisted[key].clone();
        if let Some(on) = value.as_bool() {
            value = json!(!on);
        } else if value.is_number() {
            value = json!(value.to_string());
        }
        save_graphics_profile_in(&dir, key, &json!({key: value})).unwrap();
        export_graphics_profile_in(&dir, key, &path).unwrap_or_else(|err| panic!("{key}: {err}"));
        let imported = import_graphics_profile_in(&dir, &path).unwrap();
        assert_eq!(
            graphics_profiles_in(&dir)[&imported][key],
            if value.is_string() && persisted[key].is_number() {
                persisted[key].clone()
            } else {
                value
            }
        );
        for invalid in [Value::Null, json!([]), json!({})] {
            std::fs::write(
                &path,
                serde_json::to_vec(&json!({"name": "Invalid", "graphics": {key: invalid}}))
                    .unwrap(),
            )
            .unwrap();
            assert!(
                import_graphics_profile_in(&dir, &path).is_err(),
                "invalid value accepted: {key}"
            );
        }
    }
    println!("Audited {} supported fields", GRAPHICS_PROFILE_KEYS.len());
    std::fs::remove_dir_all(dir).unwrap();
}
