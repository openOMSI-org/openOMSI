/// The simulation goes beside the picture only when the player turns it on (opt-in, on
/// phones too); the launcher's save keeps the choice, and the game's own text writes it.
#[test]
fn the_sim_thread_is_off_until_turned_on() {
    assert!(!super::Settings::default().sim_thread);
    assert!(!super::Settings::phone().sim_thread);
    assert!(!super::Settings::from_text("version=4\n").sim_thread);
    assert!(super::Settings::from_text("sim_thread=1\n").sim_thread);
    let launcher = omsi_launcher_lib::settings_from_text(Some("sim_thread=1\n"));
    let saved = omsi_launcher_lib::settings_to_text(&launcher, Some("sim_thread=1\n"));
    assert!(super::Settings::from_text(&saved).sim_thread, "{saved}");
    let on = super::Settings { sim_thread: true, ..super::Settings::default() };
    assert!(super::Settings::from_text(&on.to_text()).sim_thread);
}

/// Switched in the game's menu, the value saved (through the launcher's writer, as the game
/// saves) is the one read back at the next start - with or without the key in the file.
#[test]
fn switched_in_the_game_it_stays_switched() {
    for old in ["", "sim_thread=1\n", "sim_thread=0\n"] {
        for on in [false, true] {
            let mut v = omsi_launcher_lib::settings_from_text(Some(old));
            crate::game_lists::apply_pending(&mut v, &[("sim_thread".to_string(), (on as u8).to_string())]);
            let saved = omsi_launcher_lib::settings_to_text(&v, Some(old));
            assert_eq!(super::Settings::from_text(&saved).sim_thread, on, "switched to {on} over {old:?}:\n{saved}");
            assert_eq!(saved.matches("sim_thread=").count(), 1, "{saved}");
        }
    }
}
