use omsi_launcher_lib::Instance;

fn game(pid: u32, running: bool) -> Instance {
    Instance {
        pid,
        running,
        ..Default::default()
    }
}

/// The wait after Start ends with the first list of games that has the one started,
/// running or ended already - a game that died at once left the launcher blank for the
/// whole 15 seconds.
#[test]
fn the_wait_after_start_ends_once_the_game_is_listed() {
    // (a list asked for before the game was started)
    assert!(!super::game_listed(Some(7), &[game(3, true)]));
    assert!(super::game_listed(Some(7), &[game(3, true), game(7, true)]));
    assert!(super::game_listed(Some(7), &[game(7, false)]));
    // (Start pressed, the game not started yet)
    assert!(!super::game_listed(None, &[game(7, true)]));
}
