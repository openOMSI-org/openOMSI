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
