//! The game's own actions a controller button or key can send, by name.

/// Global actions a controller button should send to the game instead of to the bus script.
pub fn is_game_action(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("view_")
        || matches!(
            name.as_str(),
            "sim_pause"
                | "open_menu"
                | "screenshot"
                | "photo_mode"
                | "quicksave"
                | "toggel_mouse_ctrl"
                | "toggel_ctrler"
                | "voice_radio"
                | "head_tracking_recenter"
                | "head_tracking_toggle"
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

#[cfg(test)]
mod tests {
    use super::*;

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
