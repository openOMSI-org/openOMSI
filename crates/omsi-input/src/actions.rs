//! The game's own actions a controller button or key can send, by name.

/// Global actions a controller button should send to the game instead of to the bus script.
pub fn is_game_action(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("view_")
        || name.starts_with("radio_")
        || matches!(
            name.as_str(),
            "sim_pause"
                | "open_menu"
                | "screenshot"
                | "photo_mode"
                | "flashlight"
                | "clean_screen"
                | "get_up"
                | "quicksave"
                | "toggel_mouse_ctrl"
                | "toggel_ctrler"
                | "voice_radio"
                | "voice_nearby"
                | "pax_door_request"
                | "head_tracking_recenter"
        )
}

/// The game's own door actions for a controller button (or a key of `keyboard.cfg`) that
/// work on every bus: `door_<n>` is the n-th door front to back, as Shift+n on the
/// keyboard, and `doors_all` every door at once (#916).
pub fn door_action(name: &str) -> Option<usize> {
    let n = name.to_ascii_lowercase();
    if n == "doors_all" {
        return Some(0);
    }
    n.strip_prefix("door_").and_then(|d| d.parse::<usize>().ok()).filter(|d| (1..=9).contains(d))
}

/// The door release on any bus, `doors_release` (see `Player::door_release`).
pub fn is_door_release(name: &str) -> bool {
    name.eq_ignore_ascii_case("doors_release")
}

/// Whether a bus whose scripts are `program` does something with a vehicle key bound to
/// `name`, as `Player::action` handles it: the game's own indicator, door, ticket and change
/// keys, a gate of an H-pattern gearbox, a trigger of the scripts, one of the stock names an
/// action stands for (`aliases`, `ACTION_ALIASES`) or a toggle the scripts take as up and
/// down steps. (The engine's axes - throttle, brake, steering - are the caller's to add.)
/// keyboard.cfg keeps the keys of every bus ever installed: a key only another add-on uses
/// must not take a game shortcut such as Ctrl+Shift+M from this bus (#2577), while one this
/// bus answers - Ctrl+Left on `blinker_left_set` - must.
pub fn bus_uses_action(program: &omsi_script::Program, name: &str, aliases: &[(&str, &[&str])]) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(n.as_str(), "blinker_left_toggle" | "blinker_right_toggle" | "ticket_give" | "change_give" | "change_take")
        || door_action(&n).is_some()
        || is_door_release(&n)
        || crate::hpattern::resolve(program, name).is_some()
        || program.trigger(name).is_some()
        || aliases.iter().find(|(a, _)| a.eq_ignore_ascii_case(name)).is_some_and(|(_, al)| al.iter().any(|a| program.trigger(a).is_some()))
        || n.strip_suffix("_toggle").is_some_and(|b| program.trigger(&format!("{b}_up")).is_some() && program.trigger(&format!("{b}_down")).is_some())
}

/// A `keyboard.cfg` action of the VR headset (`vr_recenter`, `vr_position_navigator`, ...).
pub fn is_vr_action(action: &str) -> bool {
    action.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("vr_"))
}

/// Whether a [game] key of `keys` takes the chord (`scan` with the modifier bits `chord`): the
/// headset's keys only while a headset runs (`vr`), as their actions do nothing without one.
/// Everybody's list has `vr_position_navigator` on Ctrl+Shift+M (`with_vr_defaults`), and
/// outside VR it held the mirror editor's shortcut dead since 0.2.46 (#2577).
pub fn game_chord_bound(keys: &[omsi_content::KeyBinding], scan: i32, chord: i32, vr: bool) -> bool {
    keys.iter().any(|b| b.scan_code == scan && b.matches(chord) && (vr || !is_vr_action(&b.action)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With the default lists, the only [game] key on Ctrl+Shift+M is the headset's: outside
    /// VR the mirror editor has the chord, in VR the key (#2577).
    #[test]
    fn the_mirror_editor_chord_is_only_a_vr_key() {
        let keys = omsi_content::KeyboardCfg::default().with_game_defaults().with_vr_defaults().game;
        let scan = crate::keys::dik_code(winit::keyboard::KeyCode::KeyM).unwrap();
        let chord = omsi_content::input::chord(true, true, false);
        assert!(!game_chord_bound(&keys, scan, chord, false));
        assert!(game_chord_bound(&keys, scan, chord, true));
    }

    /// A controller button on head tracking's recentre goes to the game, not to the bus.
    #[test]
    fn the_head_tracking_recentre_is_a_game_action() {
        assert!(is_game_action("head_tracking_recenter"));
        assert!(is_game_action("Head_Tracking_Recenter"));
    }

    /// The game's own vehicle keys count for any bus; a script's trigger, an alias and a
    /// toggle by steps only where the scripts have them (#2577).
    #[test]
    fn a_key_counts_where_the_bus_uses_it() {
        let src = "{trigger:kw_m_enginestart}\n{end}\n{trigger:lights_up}\n{end}\n{trigger:lights_down}\n{end}\n{trigger:bremse_halte_on}\n{end}\n";
        let file = std::env::temp_dir().join(format!("omsi-input-actions-{}.osc", std::process::id()));
        std::fs::write(&file, src).unwrap();
        let program = omsi_script::compile(&omsi_script::CompileInput { scripts: vec![file.clone()], ..Default::default() });
        std::fs::remove_file(&file).ok();
        let aliases: &[(&str, &[&str])] = &[("haltestellenbremse", &["bremse_halte_on"]), ("unused_alias", &["no_such_trigger"])];
        for used in ["blinker_left_toggle", "door_2", "doors_all", "ticket_give", "kw_m_enginestart", "haltestellenbremse", "lights_toggle"] {
            assert!(bus_uses_action(&program, used, aliases), "{used}");
        }
        for unused in ["mirror_editor_of_another_bus", "unused_alias", "wipers_toggle"] {
            assert!(!bus_uses_action(&program, unused, aliases), "{unused}");
        }
    }

    /// The door actions that work on every bus (#916).
    #[test]
    fn door_actions_name_a_door_or_all_of_them() {
        assert_eq!(door_action("door_1"), Some(1));
        assert_eq!(door_action("Door_3"), Some(3));
        assert_eq!(door_action("doors_all"), Some(0));
        assert_eq!(door_action("door_0"), None);
        assert_eq!(door_action("bus_doorfront0"), None);
        assert_eq!(door_action("door_x"), None);
    }
}
