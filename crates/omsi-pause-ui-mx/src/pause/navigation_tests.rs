use super::*;

#[test]
fn pause_navigation_groups_every_existing_action() {
    for (id, category) in [
        ("map", Category::Driving),
        ("streets", Category::Driving),
        ("duty", Category::Driving),
        ("skipstop", Category::Driving),
        ("endduty", Category::Driving),
        ("vehicle", Category::World),
        ("world", Category::World),
        ("save", Category::Saves),
        ("saveslot", Category::Saves),
        ("load", Category::Saves),
        ("shot", Category::Capture),
        ("photo", Category::Capture),
        ("options", Category::Settings),
        ("controls", Category::Settings),
        ("camera", Category::Settings),
        ("multiplayer", Category::Online),
        ("players", Category::Online),
        ("copycode", Category::Online),
        ("admin", Category::Online),
        ("plugin", Category::Plugins),
        ("tobus", Category::Driving),
    ] {
        assert_eq!(Category::of(id), Some(category), "{id}");
    }
    assert_eq!(Category::of("resume"), None);
    assert_eq!(Category::of("quit"), None);
    assert_eq!(Category::of("future-action"), Some(Category::Settings));
}

#[test]
fn pause_navigation_shortcuts_use_original_indices_and_only_available_actions() {
    let ids = ["resume", "options", "camera", "map", "duty", "save", "quit"];
    assert_eq!(Category::Home.indices(&ids), [2, 3, 4, 5]);
    let online = ["resume", "camera", "world", "players", "quit"];
    assert_eq!(Category::Home.indices(&online), [1, 3]);
    assert!(!Category::available(&online).contains(&Category::Saves));
    assert!(Category::available(&online).contains(&Category::Online));
}

#[test]
fn pause_navigation_keeps_all_plugin_entries_distinct() {
    let ids = ["resume", "plugin", "save", "plugin", "plugin", "quit"];
    assert_eq!(Category::Plugins.indices(&ids), [1, 3, 4]);
    assert!(!Category::available(&["resume", "quit"]).contains(&Category::Plugins));
}

#[test]
fn pause_navigation_keyboard_stays_in_visible_actions_and_wraps() {
    let ids = ["resume", "options", "map", "save", "load", "quit"];
    let focus = Category::Saves.focus_indices(&ids);
    assert_eq!(focus, [0, 3, 4, 5]);
    assert_eq!(step(&focus, 0, true), Some(3));
    assert_eq!(step(&focus, 0, false), Some(5));
    assert_eq!(step(&focus, 5, true), Some(0));
    assert_eq!(step(&focus, 2, true), Some(0));
    assert_eq!(step(&[], 0, true), None);
}
