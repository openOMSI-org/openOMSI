use super::*;
use omsi_platform::updater::Status;

/// Every clickable thing of the settings page by the tab it is on (switches are named
/// `set-<key>`). Taken from the page as it was before the tabs: nothing may go missing.
fn by_tab() -> Vec<Vec<&'static str>> {
    let mut graphics = vec![
        "s-gp-sel", "s-gp-load", "s-gp-del", "s-gp-name", "s-gp-save",
        "s-preset", "s-graphics", "s-msaa", "s-scale", "s-af", "s-shadow", "set-ssao", "set-shadows", "s-casters", "set-detail_textures", "s-night", "s-led", "s-led-mip", "set-shadow_blobs", "set-reflections", "set-clouds", "s-cloud-quality", "s-rain-quality", "set-windy_trees",
        "set-fullscreen", "s-res", "set-vsync", "s-fps", "s-view", "s-maxobj", "s-minobj", "s-mirror", "s-mirror-refresh", "set-mirror_hud_every_frame", "s-texmem", "set-texture_compression", "set-gpu_texture_compression",
    ];
    if !cfg!(target_os = "macos") {
        graphics.push("s-api");
    }
    let driving = vec![
        "s-keys", "set-steering_linear", "set-old_steering", "set-red_steer_spd", "s-mouse", "s-mouse-pedal", "set-mouse_smooth", "set-mouse_hold", "set-mouse_right_off", "set-blinker_cancel", "set-brake_hold", "set-auto_clutch", "set-auto_shift", "set-momentary_gears", "s-go-keys",
        "s-wrange", "s-wlock", "s-pad-steer-smooth", "s-pad-steer-speed", "s-pad-deadzone", "set-pad_steer_linear", "s-pad-type", "set-pad_buttons", "set-arrows_switch_cams", "s-pedt", "s-pedb", "set-ff_enabled", "set-ff_invert", "s-ffroad", "s-ffeng", "s-fffade", "s-wreset", "s-go-pads",
    ];
    let mut camera = vec![
        "s-seaty",
        "s-seatz",
        "s-seatx",
        "s-seat-pitch",
        "s-seatreset",
        "s-fov",
        "s-look-sens",
        "set-right_stick_look",
        "s-look-smoothing",
        "s-head-idle",
        "s-head-idle-pace",
        "set-steer_look",
        "s-steer-look-angle",
        "s-steer-look-response",
        "set-head_movement",
        "set-driverview_smooth",
        "set-hands_in_cab",
        "set-alt_view",
        "set-precision_zoom",
        "set-camera_collision",
        "set-driver",
        "set-head_tracking",
        "s-trackir-yaw", "s-trackir-pitch", "s-trackir-roll",
        "set-head_tracking_invert_yaw", "set-head_tracking_invert_pitch", "set-head_tracking_invert_roll",
        "s-trackir-x", "s-trackir-y", "s-trackir-z",
        "set-head_tracking_invert_x", "set-head_tracking_invert_y", "set-head_tracking_invert_z",
        "s-trackir-reset",
        "set-triple_screen",
        "set-triple_span",
        "set-triple_hud_center",
        "s-triple-width_mm",
        "s-triple-distance_mm",
        "s-triple-bezel_mm",
        "s-triple-left_angle_deg",
        "s-triple-right_angle_deg",
        "s-triple-eye_height_mm",
    ];
    if cfg!(windows) {
        camera.extend(["set-vr", "s-vr-scale", "s-vr-head-smoothing", "s-vr-mirror-rate", "set-vr_desktop_mirror", "s-go-vr-keys"]);
    }
    // (the radio stations: one, see `frame`)
    let sound = vec!["s-vol", "s-volai", "s-volsc", "set-doppler", "set-ambient", "s-volamb", "s-voices", "s-radspk", "s-radhiss", "s-raddrop", "radio-name-0", "radio-url-0", "radio-del-0", "radio-add"];
    let gameplay = vec![
        "s-board", "s-stand", "s-pax-animation", "set-exact_fare", "s-pax", "set-get_up", "s-unsched", "s-maxsched", "s-maxpark", "set-ai_wait_timed_stops_only",
        "s-maint", "set-collision_vehicles", "set-collision_objects", "set-collision_pedestrians", "set-use_real_time", "set-use_real_date", "set-time_sync", "set-metar_sync", "s-timespeed",
    ];
    let general = vec![
        "s-lang", "set-machine_translation", "set-launcher_rest", "set-discord_status", "set-voice_chat", "s-uiscale", "set-ui_scale_window", "s-uiop", "set-tooltips", "set-show_fps", "s-perf", "set-notes", "set-chat", "s-chatsize", "set-name_tags",
        "set-navigator", "set-nav_arrows", "set-nav_ai", "corner-top-left", "corner-top-right", "corner-bottom-left", "corner-bottom-right",
        "set-update_check", "set-update_auto", "set-update_notify", "set-presence", "s-upd-check", "s-upd-github", "s-reset",
    ];
    vec![graphics, driving, camera, sound, gameplay, general]
}

/// Settings that show every row: Enhanced (Vanilla hides the shadows and effects), VR on.
fn all_rows() -> Value {
    let mut s = core::settings_from_text(None);
    s["triple_screen"] = json!(true);
    s["graphics"] = json!("enhanced");
    s["vr"] = json!(true);
    s
}

fn outside() -> Outside {
    Outside { update: Status::Idle, check_updates: false, reset: false, controls: None }
}

/// One frame of tab `tab`, its two columns tall enough that nothing is cut off. The Sound
/// tab lists one radio station (not the radio.cfg of whoever runs the tests).
fn frame(ui: &mut Ui, tab: usize, s: &mut Value, out: &mut Outside) {
    RADIO.with(|r| {
        r.borrow_mut().get_or_insert_with(|| vec![("One".into(), "https://example.org/one.mp3".into())]);
    });
    ui.begin(Vec2::new(1200.0, 2000.0), 1.0, 1.0 / 60.0);
    let mut dirty = 0.0;
    settings_tab(ui, tab, s, &mut dirty, out, [Rect::new(0.0, 0.0, 580.0, 2000.0), Rect::new(620.0, 0.0, 580.0, 2000.0)]);
}

/// Click the widget `name` on tab `tab`: the mouse goes down over it and comes up again.
fn click(tab: usize, name: &str, s: &mut Value) -> Outside {
    let mut ui = Ui::new();
    let mut out = outside();
    frame(&mut ui, tab, s, &mut out);
    let r = *ui.drawn.get(&id_of(name)).unwrap_or_else(|| panic!("{name} is not on the {} tab", SETTINGS_TABS[tab]));
    ui.input.mouse = r.center();
    ui.input.pressed = true;
    ui.input.down = true;
    frame(&mut ui, tab, s, &mut out);
    ui.input.pressed = false;
    ui.input.down = false;
    ui.input.released = true;
    frame(&mut ui, tab, s, &mut out);
    out
}

#[test]
fn every_setting_is_on_exactly_one_tab() {
    let tabs = by_tab();
    assert_eq!(tabs.len(), SETTINGS_TABS.len());
    let mut seen = std::collections::HashSet::new();
    for name in tabs.iter().flatten() {
        assert!(seen.insert(*name), "{name} is listed on two tabs");
    }
    for (tab, names) in tabs.iter().enumerate() {
        let mut ui = Ui::new();
        frame(&mut ui, tab, &mut all_rows(), &mut outside());
        for name in names {
            assert!(ui.drawn.contains_key(&id_of(name)), "{name} is not on the {} tab", SETTINGS_TABS[tab]);
        }
        assert_eq!(ui.drawn.len(), names.len(), "the {} tab has a clickable thing more than the list names", SETTINGS_TABS[tab]);
    }
}

/// The phone's stacked Graphics tab shows the cloud quality with Enhanced, and the
/// choice reaches the game's settings.
#[test]
fn stacked_graphics_tab_shows_and_saves_the_cloud_quality() {
    let mut ui = Ui::new();
    let mut s = all_rows();
    let mut out = outside();
    ui.begin(Vec2::new(430.0, 1600.0), 1.0, 1.0 / 60.0);
    let mut dirty = 0.0;
    settings_tab(&mut ui, 0, &mut s, &mut dirty, &mut out, [Rect::new(12.0, 0.0, 406.0, 760.0), Rect::new(12.0, 780.0, 406.0, 760.0)]);
    assert!(ui.drawn.contains_key(&id_of("s-cloud-quality")));
    s["cloud_quality"] = json!("low");
    let saved = core::settings_to_text(&s, None);
    // (that the game reads it so: omsi-app's launcher tests)
    assert_eq!(core::settings_from_text(Some(&saved))["cloud_quality"], json!("low"));
}

#[test]
fn right_stick_look_switch_toggles_and_saves_from_the_camera_tab() {
    let mut s = all_rows();
    assert_eq!(s["right_stick_look"], json!(true));

    click(2, "set-right_stick_look", &mut s);
    assert_eq!(s["right_stick_look"], json!(false));
    let saved = core::settings_to_text(&s, None);
    assert_eq!(
        core::settings_from_text(Some(&saved))["right_stick_look"],
        json!(false)
    );

    click(2, "set-right_stick_look", &mut s);
    assert_eq!(s["right_stick_look"], json!(true));
}

#[test]
fn the_driving_tab_leads_to_the_keys_and_the_controllers() {
    let mut s = all_rows();
    assert_eq!(click(1, "s-go-keys", &mut s).controls, Some(0));
    assert_eq!(click(1, "s-go-pads", &mut s).controls, Some(1));
}

/// The mouse steering's smoothing is on unless switched off, and the switch is kept (#1092).
#[test]
fn smooth_mouse_steering_switches_off_and_is_saved() {
    let mut s = all_rows();
    assert_eq!(s["mouse_smooth"], json!(true));
    click(1, "set-mouse_smooth", &mut s);
    assert_eq!(s["mouse_smooth"], json!(false));
    let saved = core::settings_to_text(&s, None);
    assert!(saved.contains("mouse_smooth=0\n"), "{saved}");
    assert_eq!(core::settings_from_text(Some(&saved))["mouse_smooth"], json!(false));
    // (that the game reads it so: omsi-app's launcher tests)
}

#[test]
fn reset_asks_first_and_changes_nothing() {
    let mut s = all_rows();
    let before = s.clone();
    let out = click(5, "s-reset", &mut s);
    assert!(out.reset);
    assert_eq!(s, before);
}
