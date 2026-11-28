//! The window size read from the settings, and the fullscreen scene cap.

#[test]
fn the_window_size_is_read_from_the_settings() {
    assert_eq!(
        super::resolution_of("msaa=4\nresolution=1280x800\n"),
        Some((1280, 800))
    );
    assert_eq!(super::resolution_of("resolution=auto\n"), None);
    assert_eq!(
        super::resolution_of("window_size=1920*1080\n"),
        Some((1920, 1080))
    );
    assert_eq!(super::resolution_of("vsync=1\n"), None);
}

#[test]
fn the_scene_cap_follows_fullscreen_both_ways() {
    let set = Some((1280, 720));
    assert_eq!(super::fullscreen_scene_cap(true, false, set), set);
    assert_eq!(super::fullscreen_scene_cap(false, false, set), None);
    assert_eq!(super::fullscreen_scene_cap(true, true, set), None);
    assert_eq!(super::fullscreen_scene_cap(true, false, None), None);
}
