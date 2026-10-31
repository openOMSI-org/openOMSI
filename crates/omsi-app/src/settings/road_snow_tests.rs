/// The built-up snow on the roads and roofs is on until the player turns it off, on a phone
/// as on the desktop, and an old settings file without the key reads as on.
#[test]
fn built_up_snow_is_on_until_turned_off() {
    assert!(super::Settings::default().road_snow);
    assert!(super::Settings::phone().road_snow);
    assert!(super::Settings::from_text("").road_snow);
    assert!(super::Settings::from_text("version=4\n").road_snow);
    assert!(!super::Settings::from_text("road_snow=0\n").road_snow);
    assert!(!super::Settings::from_text("road_snow=off\n").road_snow);
    // (the game's own writer, for the round trip: every key read is written back)
    let off = super::Settings { road_snow: false, ..Default::default() };
    assert_eq!(super::Settings::from_text(&off.to_text()), off);
    // (the launcher's save keeps it)
    let launcher = omsi_launcher_lib::settings_from_text(Some("road_snow=0\n"));
    let saved = omsi_launcher_lib::settings_to_text(&launcher, Some("road_snow=0\n"));
    assert!(!super::Settings::from_text(&saved).road_snow, "{saved}");
}

/// Switched in the game's Settings, the value saved (through the launcher's writer, as the
/// game saves) is the one read back after a restart - with or without the key in the file
/// before, and written once.
#[test]
fn switched_in_the_game_the_built_up_snow_stays_switched() {
    for old in ["", "road_snow=1\n", "road_snow=0\n"] {
        for on in [false, true] {
            let mut v = omsi_launcher_lib::settings_from_text(Some(old));
            crate::game_lists::apply_pending(&mut v, &[("road_snow".to_string(), (on as u8).to_string())]);
            let saved = omsi_launcher_lib::settings_to_text(&v, Some(old));
            assert_eq!(super::Settings::from_text(&saved).road_snow, on, "switched to {on} over {old:?}:\n{saved}");
            assert_eq!(saved.matches("road_snow=").count(), 1, "{saved}");
        }
    }
}

/// Switched off on the launcher's Graphics page, the game reads it off, and the launcher
/// reads its own file back the same.
#[test]
fn the_launchers_switch_reaches_the_game() {
    let mut page = omsi_launcher_lib::settings_from_text(None);
    assert_eq!(page["road_snow"], serde_json::json!(true));
    page["road_snow"] = serde_json::json!(false);
    let text = omsi_launcher_lib::settings_to_text(&page, Some("road_snow=1\n"));
    assert_eq!(text.matches("road_snow=0").count(), 1, "{text}");
    assert!(!super::Settings::from_text(&text).road_snow);
    assert_eq!(omsi_launcher_lib::settings_from_text(Some(&text))["road_snow"], serde_json::json!(false));
    // (a graphics profile holds it, as the rest of the Graphics page)
    assert!(omsi_launcher_lib::GRAPHICS_PROFILE_KEYS.contains(&"road_snow"));
}
