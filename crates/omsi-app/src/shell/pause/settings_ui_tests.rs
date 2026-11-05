use super::*;
use crate::app_impl::MenuLine;

fn fixture(world: bool) -> Vec<(&'static str, String)> {
    let row = |name: &str, kind: char, value: &str, desc: &str| format!("{name}\u{1f}{kind}\u{1f}{value}\u{1f}{desc}\u{1f}0.5");
    if world {
        vec![
            ("#", row("Live weather", 'i', "", "")),
            ("metar_sync", row("METAR sync", 's', "off", "The weather follows the real METAR report")),
            ("metar_src", row("METAR source", 'o', "VHHH", "The airport used for real weather.")),
            ("metar_icao_edit", row("ICAO", 'e', "VHHH", "Enter any 4-letter ICAO station.")),
            ("metar_once", row("METAR report", 'a', "Load once", "Load the selected station once without enabling continuous METAR sync.")),
            ("#", row("Weather preset and clouds", 'i', "", "")),
            ("weather", row("Preset", 'o', "Clear sky", "A ready-made weather")),
            ("cloud_cover", row("Cloud cover", 'v', "50 %", "How much of the sky the clouds cover.")),
            ("#", row("Temperature and humidity", 'i', "", "")),
            ("temp", row("Temperature", 'v', "25 °C", "The air temperature.")),
            ("humidity", row("Humidity", 'v', "70 %", "Relative humidity of the air.")),
            ("#", row("Wind", 'i', "", "")),
            ("wind_speed", row("Wind speed", 'v', "5 m/s", "How fast the wind blows; it drives the clouds.")),
            ("wind_dir", row("Wind direction", 'v', "180°", "The direction of the wind in degrees (0 is north).")),
        ]
    } else {
        vec![
            ("#", row("Destination and depot", 'i', "", "")),
            ("dest", row("Destination display", 'o', "", "Change the current destination")),
            ("hof", row("Depot file (HOF)", 'o', "", "Change the current depot file (used for the timetable)")),
            ("#", row("Vehicle identification", 'i', "", "")),
            ("number", row("Fleet number", 'o', "", "Change the vehicle's current fleet number")),
            ("custom_number_edit", row("Custom fleet number", 'e', "1234", "Enter custom fleet number (not from the list)")),
            ("custom_ident_edit", row("Custom plate", 'e', "AB 1234", "Enter custom plate (leave free for automatic)")),
            ("#", row("Driver", 'i', "", "")),
            ("driver", row("Driver", 'o', "", "Change the current driver profile")),
        ]
    }
}

fn nested_view<'a>(rail: &'a [MenuLine], rows: &'a [(&'static str, String)], world: bool) -> PauseView<'a> {
    let titles: Vec<String> = if world { vec!["Time", "Weather", "Traffic and people", "Tools"] } else { vec!["Display and driver", "Vehicles", "Service"] }
        .into_iter()
        .map(|text| omsi_ui::tr(text).into_owned())
        .collect();
    PauseView {
        paused: true,
        quick_actions: "default",
        rail,
        rail_sel: Some(1),
        disabled: &[],
        rail_open: Some(if world { "world" } else { "vehicle" }),
        kbd: true,
        keys: true,
        summary: Summary { bus: "Test vehicle".into(), driver: "Driver".into() },
        list: Some(ListView {
            kind: MenuKind::Options,
            head: Some((omsi_ui::tr(if world { "World options..." } else { "Vehicle options..." }).into_owned(), String::new())),
            tabs: Some((titles, usize::from(world))),
            items: rows.iter().map(|(id, label)| (*id, label.as_str())).collect(),
            sel: 1,
            preview: None,
            pane_first: None,
            dropdown: None,
            key: if world { "World1".into() } else { "Vehicle0".into() },
        }),
    }
}

#[test]
fn pause_nested_groups_preserve_original_indices_and_hide_back() {
    let items = [("#", "First"), ("a", "A"), ("b", "B"), ("#", "Second"), ("c", "C"), ("back", "Back")];
    assert_eq!(settings_groups(&items, true), vec![vec![0, 1, 2], vec![3, 4]]);
    assert_eq!(settings_groups(&items, false), vec![vec![0, 1, 2], vec![3, 4, 5]]);
}

#[test]
fn pause_nested_rows_keep_original_mouse_action_indices() {
    let rows = fixture(false);
    let rail = Vec::new();
    let v = nested_view(&rail, &rows, false);
    let mut sh = Shell::new();
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    sh.ui.input.mouse = Vec2::new(350.0, 245.0);
    sh.ui.input.pressed = true;
    sh.ui.input.released = true;
    settings_rows(&mut sh, &v, v.list.as_ref().unwrap(), Rect::new(300.0, 180.0, 800.0, 600.0), &mut None);
    assert_eq!(sh.actions, [Action::Choose(1)]);
}

#[test]
fn pause_nested_controls_draw_on_narrow_windows() {
    let label = "Refuel\u{1f}a\u{1f}Refuel\u{1f}Fills the tank of the current vehicle";
    for width in [120.0, 200.0, 440.0] {
        let mut sh = Shell::new();
        sh.begin(400.0, 600.0, 1.0, 1.0 / 60.0);
        settings_row(&mut sh.ui, 7, label, Rect::new(0.0, 0.0, width, 80.0), true, false, &mut None);
    }
}

#[test]
fn pause_nested_vehicle_and_world_draw_at_all_window_sizes() {
    for world in [false, true] {
        let rows = fixture(world);
        let rail = vec![
            ("resume", "Resume".into(), None),
            (if world { "world" } else { "vehicle" }, "Options".into(), None),
            ("quit", "End the session".into(), None),
        ];
        for (w, h) in [(400.0, 600.0), (1440.0, 900.0), (1920.0, 1080.0)] {
            let mut sh = Shell::new();
            sh.begin(w, h, 1.0, 1.0 / 60.0);
            draw(&mut sh, &nested_view(&rail, &rows, world));
            sh.finish();
            assert!(sh.showing());
        }
    }
}

#[test]
#[ignore = "Requires a graphics adapter and OPENOMSI_PAUSE_PREVIEW output directory"]
fn pause_nested_write_preview() {
    omsi_ui::i18n::set_lookup(|lang, text| crate::_rust_i18n_try_translate(lang, text).map(|value| value.into_owned()));
    omsi_ui::i18n::set_language("zh-tw");
    let out = std::path::PathBuf::from(std::env::var_os("OPENOMSI_PAUSE_PREVIEW").expect("set preview directory"));
    std::fs::create_dir_all(&out).unwrap();
    let instance = wgpu::Instance::default();
    let renderer = pollster::block_on(omsi_render::Renderer::new(&instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb))).unwrap();
    for (name, world, w, h) in [
        ("pause-vehicle-options", false, 1440, 900),
        ("pause-world-options", true, 1440, 900),
        ("pause-options-narrow", false, 400, 720),
    ] {
        let rows = fixture(world);
        let rail = vec![
            ("resume", "Resume".into(), None),
            (if world { "world" } else { "vehicle" }, "Options".into(), None),
            ("quit", "End the session".into(), None),
        ];
        let mut sh = Shell::new();
        sh.begin(w as f32, h as f32, 1.0, 1.0 / 60.0);
        draw(&mut sh, &nested_view(&rail, &rows, world));
        sh.finish();
        let pixels = sh.picture_for_shot(&renderer, w, h).expect("drawn settings");
        assert_eq!(pixels[(((h / 2) * w + w - 2) * 4 + 3) as usize], 255);
        image::save_buffer(out.join(format!("{name}.png")), &pixels, w, h, image::ColorType::Rgba8).unwrap();
        println!("preview: {}", out.join(format!("{name}.png")).display());
    }
}

#[test]
fn pause_settings_keep_main_row_density() {
    assert_eq!(LIST_ROW, 40.0);
    assert_eq!(row_h("item", "Name\u{1f}a"), ROW + 4.0);
    assert_eq!(row_h("item", "Name\u{1f}a\u{1f}Value\u{1f}Description"), 52.0);
    assert_eq!(row_h(crate::game_lists::HEADING, "Group"), 34.0);
}

#[test]
fn pause_settings_mouse_hover_suppresses_keyboard_highlight() {
    let render = |kbd, mouse| {
        let rows = fixture(false);
        let rail = Vec::new();
        let mut v = nested_view(&rail, &rows, false);
        v.kbd = kbd;
        let mut sh = Shell::new();
        sh.begin(1366.0, 768.0, 1.0, 1.0 / 60.0);
        sh.ui.input.mouse = mouse;
        settings_rows(&mut sh, &v, v.list.as_ref().unwrap(), Rect::new(300.0, 180.0, 800.0, 500.0), &mut None);
        sh.finish();
        sh.drawn.unwrap().verts
    };
    let hovered = Vec2::new(350.0, 330.0);
    assert!(render(true, hovered) == render(false, hovered), "mouse hover must suppress keyboard focus");
    assert!(render(true, Vec2::ZERO) != render(false, Vec2::ZERO), "keyboard focus must remain visible without hover");
}

#[test]
fn pause_settings_headings_draw_accent_markers_without_actions() {
    for scale in [1.0, 1.5] {
        let rows = vec![
            ("#", "Clock and time".to_owned()),
            ("#", "Quick time adjustments".to_owned()),
            ("#", "Time presets".to_owned()),
        ];
        let rail = Vec::new();
        let v = nested_view(&rail, &rows, true);
        let mut sh = Shell::new();
        sh.begin(1366.0, 768.0, scale, 1.0 / 60.0);
        sh.ui.input.mouse = Vec2::new(319.0, 210.0);
        sh.ui.input.pressed = true;
        sh.ui.input.released = true;
        settings_rows(
            &mut sh,
            &v,
            v.list.as_ref().unwrap(),
            Rect::new(300.0, 180.0, 800.0, 500.0),
            &mut None,
        );
        sh.finish();
        assert!(
            sh.actions.is_empty(),
            "section headings must not dispatch actions"
        );
        let verts = &sh.drawn.as_ref().unwrap().verts;
        for n in 0..3 {
            let top = 201.0 + n as f32 * 74.0;
            let marker: Vec<_> = verts
                .iter()
                .filter(|v| {
                    v.color == th().accent.0
                        && (318.0..=321.0).contains(&v.pos[0])
                        && (top..=top + 20.0).contains(&v.pos[1])
                })
                .collect();
            assert!(
                !marker.is_empty(),
                "heading {n} must have an accent marker at scale {scale}"
            );
            assert!(marker.iter().any(|v| v.pos[0] == 318.0));
            assert!(marker.iter().any(|v| v.pos[0] == 321.0));
            assert!(marker.iter().any(|v| v.pos[1] == top));
            assert!(marker.iter().any(|v| v.pos[1] == top + 20.0));
        }
    }
}

#[test]
#[ignore = "Requires a graphics adapter and OPENOMSI_PAUSE_PREVIEW output directory"]
fn pause_section_markers_write_preview() {
    omsi_ui::i18n::set_lookup(|lang, text| {
        crate::_rust_i18n_try_translate(lang, text).map(|value| value.into_owned())
    });
    omsi_ui::i18n::set_language("zh-tw");
    let row = |name: &str, kind: char, value: &str, desc: &str| {
        format!(
            "{}\u{1f}{kind}\u{1f}{value}\u{1f}{}\u{1f}0.5",
            omsi_ui::tr(name),
            omsi_ui::tr(desc)
        )
    };
    let mut rows = vec![
        ("#", row("Clock and time speed", 'i', "", "")),
        (
            "time_sync",
            row(
                "Real-time sync",
                's',
                "off",
                "The game follows your device's date and time",
            ),
        ),
        (
            "time_edit",
            row(
                "Exact time",
                'e',
                "00:39:11",
                "Change the current time (Press Enter to change)",
            ),
        ),
        (
            "hour",
            row("Hour", 'v', "00", "Set the hour of the day directly"),
        ),
        (
            "minute",
            row("Minute", 'v', "39", "Set the minute directly"),
        ),
        ("#", row("Quick time changes", 'i', "", "")),
    ];
    for (name, value) in [
        ("Clock -1 hour", "-1 h"),
        ("Clock -10 minutes", "-10 min"),
        ("Clock +10 minutes", "+10 min"),
        ("Clock +1 hour", "+1 h"),
    ] {
        rows.push(("clock_shift", row(name, 'a', value, "Move the clock.")));
    }
    rows.push(("#", row("Time presets", 'i', "", "")));
    for (name, value) in [
        ("Morning", "06:00"),
        ("Noon", "12:00"),
        ("Evening", "18:00"),
        ("Night", "23:00"),
    ] {
        rows.push((
            "clock_set",
            row(name, 'a', value, "Jump to this time of day."),
        ));
    }
    rows.push(("#", row("Date", 'i', "", "")));
    rows.push((
        "date",
        row(
            "Date",
            'd',
            "2026-10-24",
            "The day the timetable, the season and the map's changes follow",
        ),
    ));
    for (name, value) in [
        ("Date -1 month", "-1 month"),
        ("Date -1 week", "-1 week"),
        ("Date -1 day", "-1 day"),
        ("Date +1 day", "+1 day"),
        ("Date +1 week", "+1 week"),
        ("Date +1 month", "+1 month"),
    ] {
        rows.push(("clock_shift", row(name, 'a', value, "Move the date.")));
    }
    rows.push(("#", row("Time speed", 'i', "", "")));
    rows.push((
        "speed",
        row("Time speed", 'v', "x1", "How fast the world's clock runs"),
    ));
    let out = std::path::PathBuf::from(
        std::env::var_os("OPENOMSI_PAUSE_PREVIEW").expect("set preview directory"),
    );
    std::fs::create_dir_all(&out).unwrap();
    let instance = wgpu::Instance::default();
    let renderer = pollster::block_on(omsi_render::Renderer::new(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
    ))
    .unwrap();
    let rail = vec![
        ("resume", "Resume".into(), None),
        ("world", "World options...".into(), None),
    ];
    for (name, w, h, scale) in [
        ("time-desktop", 1920, 1080, 1.0),
        ("time-laptop", 1366, 768, 1.0),
        ("time-laptop-150", 1366, 768, 1.5),
        ("time-narrow", 400, 720, 1.0),
    ] {
        let mut v = nested_view(&rail, &rows, true);
        v.list.as_mut().unwrap().tabs.as_mut().unwrap().1 = 0;
        let mut sh = Shell::new();
        sh.begin(w as f32, h as f32, scale, 1.0 / 60.0);
        draw(&mut sh, &v);
        sh.finish();
        let pixels = sh
            .picture_for_shot(&renderer, w, h)
            .expect("drawn time settings");
        image::save_buffer(
            out.join(format!("{name}.png")),
            &pixels,
            w,
            h,
            image::ColorType::Rgba8,
        )
        .unwrap();
        println!("preview: {}", out.join(format!("{name}.png")).display());
    }
}

#[test]
fn pause_settings_wheel_scrolls_instead_of_adjusting_slider() {
    let mut sh = Shell::new();
    sh.begin(800.0, 600.0, 1.0, 0.016);
    sh.ui.input.mouse = Vec2::new(270.0, 100.0);
    sh.ui.input.wheel = Vec2::new(0.0, -1.0);
    let mut action = None;
    sh.ui.scroll_area(
        "slider-page",
        Rect::new(0.0, 0.0, 500.0, 300.0),
        &mut |ui, _| {
            action = settings_row(
                ui,
                1,
                "Range\u{1f}v\u{1f}50\u{1f}\u{1f}0.5",
                Rect::new(0.0, 80.0, 500.0, 40.0),
                false,
                false,
                &mut None,
            );
            900.0
        },
    );
    assert!(action.is_none(), "wheel must not change a slider");
    assert!(
        sh.ui.scroll[&(id_of("slider-page") ^ 0xabc)] > 0.0,
        "wheel must scroll the settings page"
    );
}

#[test]
fn pause_settings_dropdown_wheel_does_not_reach_underlying_page() {
    let rows = vec![
        ("language", "Language\u{1f}o\u{1f}English".to_owned()),
        ("range", "Range\u{1f}v\u{1f}50\u{1f}\u{1f}0.5".to_owned()),
    ];
    let rail = Vec::new();
    let mut v = nested_view(&rail, &rows, false);
    v.kbd = false;
    v.list.as_mut().unwrap().dropdown = Some(DropdownView {
        row: 0,
        items: vec!["Language"; 30],
        sel: 0,
        top: 0,
        current: Some(0),
    });
    let mut sh = Shell::new();
    let page = Rect::new(0.0, 0.0, 1000.0, 800.0);
    sh.begin(1000.0, 800.0, 1.0, 0.016);
    list_page(&mut sh, &v, v.list.as_ref().unwrap(), page);
    let anchor = sh.dd_anchor.unwrap();
    sh.finish();
    sh.actions.clear();
    sh.begin(1000.0, 800.0, 1.0, 0.016);
    sh.ui.input.mouse = Vec2::new(anchor.x + 30.0, anchor.bottom() + 15.0);
    sh.ui.input.wheel = Vec2::new(0.0, -1.0);
    list_page(&mut sh, &v, v.list.as_ref().unwrap(), page);
    assert!(
        sh.actions.is_empty(),
        "dropdown wheel must not change the covered slider"
    );
    assert_eq!(
        sh.ui.scroll[&id_of("pause-dropdown")],
        40.0,
        "only the dropdown must scroll"
    );
    assert_eq!(sh.ui.input.wheel, Vec2::ZERO);
}

#[test]
fn pause_settings_slider_still_accepts_pointer_drag() {
    let mut sh = Shell::new();
    sh.begin(800.0, 600.0, 1.0, 0.016);
    sh.ui.input.mouse = Vec2::new(270.0, 100.0);
    sh.ui.input.pressed = true;
    sh.ui.input.down = true;
    let action = settings_row(
        &mut sh.ui,
        1,
        "Range\u{1f}v\u{1f}50\u{1f}\u{1f}0.5",
        Rect::new(0.0, 80.0, 500.0, 40.0),
        false,
        false,
        &mut None,
    );
    assert!(matches!(action, Some(Action::Control(1, _))));
}

#[test]
fn pause_settings_scale_stays_stable_until_slider_release() {
    let mut sh = Shell::new();
    sh.begin(800.0, 600.0, 1.0, 0.016);
    sh.pointer(270.0, 100.0);
    sh.button(true);
    let action = settings_row(
        &mut sh.ui, 1, "Range\u{1f}v\u{1f}50\u{1f}\u{1f}0.5",
        Rect::new(0.0, 80.0, 500.0, 40.0), false, false, &mut None,
    );
    assert!(matches!(action, Some(Action::Control(1, _))));
    sh.finish();
    sh.begin(800.0, 600.0, 1.5, 0.016);
    assert_eq!(sh.scale, 1.0, "drag geometry must not move with the selected scale");
    sh.button(false);
    sh.begin(800.0, 600.0, 1.5, 0.016);
    assert_eq!(sh.scale, 1.0, "release must use the same slider geometry");
    settings_row(
        &mut sh.ui, 1, "Range\u{1f}v\u{1f}50\u{1f}\u{1f}0.5",
        Rect::new(0.0, 80.0, 500.0, 40.0), false, false, &mut None,
    );
    sh.finish();
    sh.begin(800.0, 600.0, 1.5, 0.016);
    assert_eq!(sh.scale, 1.5, "apply the pending scale after release");
    assert_eq!(sh.ui.input.mouse, Vec2::new(180.0, 100.0 / 1.5));
}

#[test]
fn pause_settings_scale_keeps_stationary_pointer_in_physical_pixels() {
    let mut sh = Shell::new();
    sh.begin(800.0, 600.0, 1.0, 0.016);
    sh.pointer(300.0, 150.0);
    sh.begin(800.0, 600.0, 1.5, 0.016);
    assert_eq!(sh.ui.input.mouse, Vec2::new(200.0, 100.0));
}

#[test]
fn pause_date_field_opens_calendar_without_changing_clock() {
    let mut sh = Shell::new();
    sh.begin(800.0, 600.0, 1.0, 0.016);
    sh.ui.input.mouse = Vec2::new(300.0, 100.0);
    sh.ui.input.pressed = true;
    sh.ui.input.released = true;
    let action = settings_row(
        &mut sh.ui,
        7,
        "Date\u{1f}d\u{1f}2026-11-03\u{1f}Choose a date",
        Rect::new(0.0, 80.0, 500.0, 40.0),
        false,
        false,
        &mut None,
    );
    assert!(action.is_none(), "opening the calendar must not execute a row action");
    sh.finish();
    sh.ui.input.mouse = Vec2::new(260.0, 140.0);
    assert!(
        !sh.ui.hover(Rect::new(250.0, 130.0, 40.0, 40.0)),
        "the calendar must capture pointer input over the page"
    );
}


fn calendar_row(sh: &mut Shell) -> Option<Action> {
    settings_row(&mut sh.ui, 7, "Date\u{1f}d\u{1f}2026-11-03", Rect::new(0.0, 80.0, 500.0, 40.0), false, false, &mut None)
}

#[test]
fn pause_date_field_commits_only_the_selected_day() {
    let mut sh = Shell::new();
    for mouse in [Vec2::new(300.0, 100.0), Vec2::new(310.0, 259.0)] {
        sh.begin(800.0, 600.0, 1.0, 0.016);
        sh.ui.input.mouse = mouse;
        sh.ui.input.pressed = true;
        sh.ui.input.released = true;
        assert!(calendar_row(&mut sh).is_none());
        sh.finish();
    }
    sh.begin(800.0, 600.0, 1.0, 0.016);
    assert_eq!(calendar_row(&mut sh), Some(Action::Date("2026-11-10".into())));
    assert!(!sh.ui.dismiss_date_popup(), "selection must close the calendar");
}

#[test]
fn pause_date_field_can_be_cancelled_without_a_clock_action() {
    let mut sh = Shell::new();
    sh.begin(800.0, 600.0, 1.0, 0.016);
    sh.ui.input.mouse = Vec2::new(300.0, 100.0);
    sh.ui.input.pressed = true;
    sh.ui.input.released = true;
    assert!(calendar_row(&mut sh).is_none());
    sh.finish();
    assert!(sh.ui.dismiss_date_popup());
    sh.begin(800.0, 600.0, 1.0, 0.016);
    assert!(calendar_row(&mut sh).is_none());
    assert!(!sh.ui.dismiss_date_popup());
}

#[test]
fn pause_read_only_date_does_not_open_a_calendar() {
    let mut sh = Shell::new();
    sh.begin(800.0, 600.0, 1.0, 0.016);
    sh.ui.input.mouse = Vec2::new(300.0, 100.0);
    sh.ui.input.pressed = true;
    sh.ui.input.released = true;
    assert!(settings_row(&mut sh.ui, 7, "Date and time\u{1f}i\u{1f}2026-11-03 13:24:56", Rect::new(0.0, 80.0, 500.0, 40.0), false, false, &mut None).is_none());
    assert!(!sh.ui.dismiss_date_popup());
}


#[test]
#[ignore = "Requires a graphics adapter and OPENOMSI_PAUSE_PREVIEW output directory"]
fn render_pause_date_calendar_preview() {
    omsi_ui::i18n::set_lookup(|lang, text| crate::_rust_i18n_try_translate(lang, text).map(|value| value.into_owned()));
    omsi_ui::i18n::set_language("zh-tw");
    let out = std::path::PathBuf::from(std::env::var_os("OPENOMSI_PAUSE_PREVIEW").expect("set preview directory"));
    std::fs::create_dir_all(&out).unwrap();
    let instance = wgpu::Instance::default();
    let renderer = pollster::block_on(omsi_render::Renderer::new(&instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb))).unwrap();
    let rows = vec![
        ("#", "Date\u{1f}i".to_owned()),
        ("date", "Date\u{1f}d\u{1f}2026-11-03\u{1f}The day the timetable, the season and the map's changes follow".to_owned()),
        ("clock_shift -86400", "Date -1 day\u{1f}a\u{1f}-1 day\u{1f}Move the date.".to_owned()),
        ("clock_shift 86400", "Date +1 day\u{1f}a\u{1f}+1 day\u{1f}Move the date.".to_owned()),
    ];
    let rail = vec![("resume", "Resume".into(), None), ("world", "World options...".into(), None)];
    for (name, w, h) in [("pause-date-desktop", 1440, 900), ("pause-date-narrow", 400, 720)] {
        let mut view = nested_view(&rail, &rows, true);
        view.kbd = false;
        let list = view.list.as_mut().unwrap();
        list.tabs.as_mut().unwrap().1 = 0;
        list.key = "World0".into();
        let mut sh = Shell::new();
        sh.begin(w as f32, h as f32, 1.0, 1.0 / 60.0);
        draw(&mut sh, &view);
        let field = sh.ui.drawn[&id_of("pause-date-1-Date")];
        sh.finish();
        for frame in 0..12 {
            sh.begin(w as f32, h as f32, 1.0, 1.0 / 60.0);
            sh.ui.input.mouse = field.center();
            sh.ui.input.pressed = frame == 0;
            sh.ui.input.released = frame == 0;
            draw(&mut sh, &view);
            sh.finish();
        }
        let pixels = sh.picture_for_shot(&renderer, w, h).expect("drawn calendar");
        image::save_buffer(out.join(format!("{name}.png")), &pixels, w, h, image::ColorType::Rgba8).unwrap();
        println!("preview: {}", out.join(format!("{name}.png")).display());
    }
}

#[test]
fn settings_search_keyboard_input_does_not_dispatch_game_actions() {
    use winit::keyboard::KeyCode as K;
    let mut sh = Shell::new();
    assert!(!search::key(&mut sh, K::KeyW, Some("w")));
    sh.ui.focus = Some(id_of("pause-settings-search"));
    assert!(search::key(&mut sh, K::KeyW, Some("w")));
    assert_eq!(sh.ui.input.text, "w");
    assert!(sh.actions.is_empty());
    assert!(search::key(&mut sh, K::Escape, None));
    assert!(!search::focused(&sh));
}

#[test]
fn settings_search_filtered_click_keeps_original_action_index() {
    let rows = fixture(false);
    let rail = Vec::new();
    let v = nested_view(&rail, &rows, false);
    let mut sh = Shell::new();
    sh.settings_search.query = "custom plate".into();
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    sh.ui.input.mouse = Vec2::new(350.0, 245.0);
    sh.ui.input.pressed = true;
    sh.ui.input.released = true;
    settings_rows(&mut sh, &v, v.list.as_ref().unwrap(), Rect::new(300.0, 180.0, 800.0, 600.0), &mut None);
    assert_eq!(sh.actions, [Action::Choose(6)]);
    assert_eq!(sh.settings_search.visible, [6]);
}

#[test]
fn settings_search_toolbar_types_and_resets_when_page_changes() {
    let rows = fixture(false);
    let rail = Vec::new();
    let v = nested_view(&rail, &rows, false);
    let list = v.list.as_ref().unwrap();
    let mut sh = Shell::new();
    sh.settings_search.page = list.key.clone();
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    sh.ui.focus = Some(id_of("pause-settings-search"));
    sh.ui.input.text = "Fleet".into();
    let mut body = Rect::new(300.0, 180.0, 800.0, 600.0);
    search::toolbar(&mut sh, list, Rect::new(300.0, 100.0, 700.0, 650.0), &mut body);
    assert_eq!(sh.settings_search.query, "Fleet");
    assert_eq!(body.y, 228.0);
    sh.ui.input.text.clear();
    sh.settings_search.page = "Other".into();
    search::toolbar(&mut sh, list, Rect::new(300.0, 100.0, 700.0, 650.0), &mut body);
    assert!(sh.settings_search.query.is_empty());
    assert!(!search::focused(&sh));
}

#[test]
fn settings_search_closes_phone_keyboard_and_keeps_desktop_text() {
    use clap::Parser;
    use winit::keyboard::KeyCode as K;
    let mut app = crate::assemble_app(crate::cli::Args::parse_from(["openomsi"]), crate::settings::Settings::default());
    app.menus.game_menu = Some(0);
    app.open_list(crate::game_lists::ListKind::Options(0));
    app.shell.ui.focus = Some(id_of("pause-settings-search"));
    crate::platform::TEXT_INPUT_ON.with(|flag| flag.set(false));
    crate::platform::phone_keyboard(None, true);
    assert_eq!(crate::platform::TEXT_INPUT_ON.with(|flag| flag.get()), crate::platform::MOBILE);
    assert!(app.settings_search_key(K::KeyE, Some("é")));
    assert_eq!(app.shell.ui.input.text, "é");
    assert!(app.settings_search_key(K::Escape, None));
    assert!(!crate::platform::PHONE_KEYBOARD_ON.with(|flag| flag.get()));
    assert!(!crate::platform::TEXT_INPUT_ON.with(|flag| flag.get()));
}

#[test]
fn pause_unheaded_long_page_uses_right_column_with_original_action_index() {
    let rows: Vec<_> = (0..8).map(|_| ("keybind", "Binding\u{1f}a\u{1f}Key\u{1f}Change binding".to_string())).collect();
    let rail = Vec::new();
    let mut v = nested_view(&rail, &rows, false);
    v.list.as_mut().unwrap().sel = 0;
    let mut sh = Shell::new();
    sh.begin(1600.0, 900.0, 1.0, 1.0 / 60.0);
    sh.ui.input.mouse = Vec2::new(1200.0, 215.0);
    sh.ui.input.pressed = true;
    sh.ui.input.released = true;
    settings_rows(&mut sh, &v, v.list.as_ref().unwrap(), Rect::new(300.0, 180.0, 1100.0, 600.0), &mut None);
    assert_eq!(sh.actions, [Action::Choose(4)]);
}
