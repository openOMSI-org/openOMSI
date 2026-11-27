use super::*;
use winit::keyboard::ModifiersState as M;

#[test]
fn screenshot_modifier_release_without_focus_loss_restores_wasd() {
    // Captured on macOS: Shift down, Cmd down, screenshot, ModifiersChanged(empty),
    // W down. There was no Focused(false) or modifier KeyboardInput release.
    let mut keys: hashbrown::HashSet<KeyCode> = [KeyCode::ShiftLeft, KeyCode::SuperLeft].into();
    assert!(keys.contains(&KeyCode::ShiftLeft)); // W would reach the wipers.
    let released = release_inactive_modifiers(&mut keys, M::empty());
    assert_eq!(released, [KeyCode::ShiftLeft, KeyCode::SuperLeft]);
    keys.insert(KeyCode::KeyW);
    assert!(!shift_held_now(&keys));
    assert_eq!(
        fallback_action(KeyCode::KeyW, "wasd"),
        Some(omsi_sim::input::EngineAction::Throttle)
    );
    assert!(release_inactive_modifiers(&mut keys, M::empty()).is_empty());
    assert!(keys.contains(&KeyCode::KeyW));
}

#[test]
fn modifier_state_preserves_held_chords_and_releases_both_sides() {
    let mut keys = [
        KeyCode::ShiftLeft,
        KeyCode::ShiftRight,
        KeyCode::ControlRight,
        KeyCode::AltRight,
        KeyCode::SuperRight,
        KeyCode::KeyW,
    ]
    .into();
    assert_eq!(
        release_inactive_modifiers(&mut keys, M::SHIFT | M::CONTROL),
        [KeyCode::AltRight, KeyCode::SuperRight]
    );
    assert!(shift_held_now(&keys));
    assert!(keys.contains(&KeyCode::ControlRight));
    assert_eq!(
        release_inactive_modifiers(&mut keys, M::empty()),
        [
            KeyCode::ShiftLeft,
            KeyCode::ShiftRight,
            KeyCode::ControlRight
        ]
    );
    assert_eq!(keys, [KeyCode::KeyW].into());
    // A modifier held while focus returns must not fabricate a new key press.
    keys.clear();
    assert!(release_inactive_modifiers(&mut keys, M::SHIFT).is_empty());
    assert!(keys.is_empty());
}

/// A lone Shift, Ctrl or Alt bound in keyboard.cfg: its own code with no modifier (#1477),
/// OMSI's "both sides" code, and with its own bit as OMSI's files write it (#2003, #2392).
#[test]
fn a_modifier_key_is_a_key_of_its_own() {
    use omsi_content::KeyBinding;
    let kb = |k: i32, m: i32| KeyBinding {
        action: "a".into(),
        scan_code: k,
        modifier: m,
    };
    assert!(binding_is_key(&kb(42, 0), KeyCode::ShiftLeft, 42, 0));
    assert!(binding_is_key(&kb(90, 0), KeyCode::ShiftRight, 54, 0));
    assert!(binding_is_key(&kb(90, 3), KeyCode::ShiftLeft, 42, 0));
    assert!(binding_is_key(&kb(92, 0), KeyCode::AltRight, 184, 0));
    assert!(!binding_is_key(&kb(92, 0), KeyCode::ControlLeft, 29, 0));
    assert!(!binding_is_key(&kb(56, 0), KeyCode::AltRight, 184, 0));
    // Ctrl held with a lone Shift binding: not that binding
    assert!(!binding_is_key(
        &kb(42, 0),
        KeyCode::ShiftLeft,
        42,
        omsi_content::input::KEY_CTRL
    ));
    // an ordinary key: only its own code, only its chord
    assert!(binding_is_key(&kb(22, 6), KeyCode::KeyU, 22, 6));
    assert!(!binding_is_key(&kb(22, 6), KeyCode::KeyU, 22, 2));
    assert!(!binding_is_key(&kb(22, 2), KeyCode::KeyU, 22, 6));
}

/// F1 given to a door and the driver's view moved to 1 (#701): F1 is not the view any more.
#[test]
fn a_built_in_view_key_steps_aside_for_the_players_own() {
    use omsi_content::KeyBinding;
    let kb = |a: &str, k: i32| KeyBinding {
        action: a.into(),
        scan_code: k,
        modifier: 0,
    };
    let none = std::collections::HashSet::new();
    assert!(super::key_left_free(
        Some(59),
        "view_set_driver",
        &none,
        &[]
    ));
    let own: std::collections::HashSet<i32> = [59].into();
    assert!(!super::key_left_free(
        Some(59),
        "view_set_driver",
        &own,
        &[]
    ));
    assert!(!super::key_left_free(
        Some(59),
        "view_set_driver",
        &none,
        &[kb("view_set_driver", 2)]
    ));
    assert!(super::key_left_free(
        Some(59),
        "view_set_driver",
        &none,
        &[kb("view_set_passenger", 60)]
    ));
}

#[test]
fn shared_pause_and_vehicle_binding_triggers_both_once_without_double_toggle() {
    let mut app = crate::assemble_app(
        <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]),
        Default::default(),
    );
    let (p_scan, f6_scan) = (
        keys::dik_code(KeyCode::KeyP).unwrap(),
        keys::dik_code(KeyCode::F6).unwrap(),
    );
    let vehicle = crate::schedule::tests::script_test_vehicle(
        "{trigger:test_honk}\n(L.L.hits) 1 + (S.L.hits)\n{end}\n",
        "hits\n",
        "",
    );
    app.player = Some(crate::spawn::test_player(
        vehicle,
        vec![omsi_content::KeyBinding {
            action: "test_honk".into(),
            scan_code: p_scan,
            modifier: 0,
        }],
    ));
    app.input.own_keys.insert(p_scan);
    app.input
        .game_keys
        .retain(|b| !b.action.eq_ignore_ascii_case("sim_pause"));
    app.input.game_keys.push(omsi_content::KeyBinding {
        action: "sim_pause".into(),
        scan_code: p_scan,
        modifier: 0,
    });
    app.dispatch_driving_key(None, KeyCode::KeyP, true, false, false, false, false);
    assert!(app.paused && app.player.as_ref().unwrap().vehicle.var("hits") == Some(1.0));
    for scan in [f6_scan, 0] {
        app.paused = false;
        app.input
            .game_keys
            .iter_mut()
            .filter(|b| b.action.eq_ignore_ascii_case("sim_pause"))
            .for_each(|b| b.scan_code = scan);
        app.dispatch_driving_key(None, KeyCode::KeyP, true, false, false, false, false);
        assert!(!app.paused);
    }
    assert_eq!(app.player.as_ref().unwrap().vehicle.var("hits"), Some(3.0));
    app.dispatch_driving_key(None, KeyCode::F6, true, false, false, false, false);
    assert!(!app.paused);
    app.input
        .game_keys
        .iter_mut()
        .filter(|b| b.action.eq_ignore_ascii_case("sim_pause"))
        .for_each(|b| b.scan_code = f6_scan);
    app.dispatch_driving_key(None, KeyCode::F6, true, false, false, false, false);
    assert!(app.paused);
}
