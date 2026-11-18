/// A new bus starts with a full tank unless the player turned it off; the launcher's own
/// save keeps the choice (it does not know the key).
#[test]
fn the_full_tank_is_on_until_turned_off() {
    assert!(super::Settings::default().full_tank);
    assert!(super::Settings::from_text("version=4\n").full_tank);
    assert!(!super::Settings::from_text("full_tank=0\n").full_tank);
    let launcher = omsi_launcher_lib::settings_from_text(Some("full_tank=0\n"));
    let saved = omsi_launcher_lib::settings_to_text(&launcher, Some("full_tank=0\n"));
    assert!(!super::Settings::from_text(&saved).full_tank, "{saved}");
}

/// Switched in the game's menu, the value saved (through the launcher's writer, as the game
/// saves) is the one read back - with or without the key in the file before.
#[test]
fn switched_in_the_game_it_stays_switched() {
    let read: [(&str, fn(&super::Settings) -> bool); 2] = [("full_tank", |s| s.full_tank), ("trip_summary", |s| s.trip_summary)];
    for (key, value) in read {
        for old in ["", "full_tank=1\ntrip_summary=1\n", "full_tank=0\ntrip_summary=0\n"] {
            for on in [false, true] {
                let mut v = omsi_launcher_lib::settings_from_text(Some(old));
                crate::game_lists::apply_pending(&mut v, &[(key.to_string(), (on as u8).to_string())]);
                let saved = omsi_launcher_lib::settings_to_text(&v, Some(old));
                assert_eq!(value(&super::Settings::from_text(&saved)), on, "{key} switched to {on} over {old:?}:\n{saved}");
                assert_eq!(saved.matches(&format!("{key}=")).count(), 1, "{saved}");
            }
        }
    }
}
