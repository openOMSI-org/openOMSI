use super::*;
use crate::app_impl::MenuLine;
use crate::shell::pause::{ListView, Summary, draw};
use crate::ui::MenuKind;

fn menu() -> Vec<MenuLine> {
    [
        ("resume", "Resume"),
        ("options", "Options..."),
        ("camera", "Camera..."),
        ("map", "City map"),
        ("duty", "Line and tour..."),
        ("save", "Save the situation"),
        ("shot", "Screenshot"),
        ("quit", "End the session"),
    ]
    .into_iter()
    .map(|(id, label)| (id, label.into(), None))
    .collect()
}

fn view(rail: &[MenuLine]) -> PauseView<'_> {
    PauseView {
        paused: true,
        rail,
        rail_sel: Some(0),
        disabled: &[],
        rail_open: None,
        list: None,
        kbd: false,
        keys: true,
        summary: Summary { bus: "Test vehicle".into(), driver: "Driver".into() },
    }
}

fn click(sh: &mut Shell, v: &PauseView, at: Vec2) {
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    sh.ui.input.mouse = at;
    sh.ui.input.pressed = true;
    sh.ui.input.released = true;
    draw(sh, v);
    sh.finish();
}

#[test]
fn pause_navigation_card_keeps_the_original_action_index() {
    let rail = menu();
    let mut sh = Shell::new();
    click(&mut sh, &view(&rail), Vec2::new(400.0, 225.0));
    assert_eq!(sh.actions, [Action::Choose(2)]);
}

#[test]
fn pause_navigation_category_click_is_not_a_game_action() {
    let rail = menu();
    let mut sh = Shell::new();
    let categories = Category::available(&ids(&view(&rail)));
    let row = categories.iter().position(|c| *c == Category::Saves).unwrap();
    click(&mut sh, &view(&rail), Vec2::new(60.0, 202.0 + row as f32 * 60.0));
    assert_eq!(sh.actions, [Action::PauseCategory(Category::Saves)]);
}

#[test]
fn pause_navigation_disabled_card_does_not_dispatch() {
    let rail = menu();
    let mut v = view(&rail);
    v.disabled = &["camera"];
    let mut sh = Shell::new();
    click(&mut sh, &v, Vec2::new(400.0, 225.0));
    assert!(sh.actions.is_empty());
}

#[test]
fn pause_navigation_draws_each_category_at_small_and_large_sizes() {
    let rail = menu();
    let mut sh = Shell::new();
    for (w, h) in [(320.0, 480.0), (480.0, 800.0), (1024.0, 600.0), (1920.0, 1080.0)] {
        for category in Category::available(&ids(&view(&rail))) {
            sh.pause_category = category;
            sh.begin(w, h, 1.0, 1.0 / 60.0);
            draw(&mut sh, &view(&rail));
            sh.finish();
            assert!(sh.showing());
        }
    }
}

#[test]
fn pause_navigation_category_icons_exist_in_the_shared_icon_set() {
    for category in Category::ALL {
        assert!(omsi_ui::icons::names().any(|name| name == category.icon()), "{:?}", category);
    }
}

#[test]
#[ignore = "Requires a graphics adapter and OPENOMSI_PAUSE_PREVIEW output directory"]
fn pause_navigation_write_preview() {
    omsi_ui::i18n::set_lookup(|lang, text| crate::_rust_i18n_try_translate(lang, text).map(|value| value.into_owned()));
    omsi_ui::i18n::set_language("zh-tw");
    let out = std::path::PathBuf::from(std::env::var_os("OPENOMSI_PAUSE_PREVIEW").expect("set OPENOMSI_PAUSE_PREVIEW"));
    std::fs::create_dir_all(&out).unwrap();
    let instance = wgpu::Instance::default();
    let renderer = pollster::block_on(omsi_render::Renderer::new(&instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb))).unwrap();
    let mut rail = menu();
    for (id, label) in [
        ("vehicle", "Vehicle options..."),
        ("world", "World options..."),
        ("controls", "Controls..."),
        ("saveslot", "Save to a slot..."),
        ("load", "Load the quicksave"),
        ("photo", "Photo mode"),
        ("skipstop", "Skip the next stop"),
        ("endduty", "End the tour"),
    ] {
        rail.insert(rail.len() - 1, (id, label.into(), None));
    }
    for (name, category, w, h, selected) in [
        ("pause-home", Category::Home, 1440, 900, None),
        ("pause-home-focus", Category::Home, 1440, 900, Some(2)),
        ("pause-quit-focus", Category::Home, 1440, 900, Some(rail.len() - 1)),
        ("pause-saves", Category::Saves, 1440, 900, None),
        ("pause-narrow", Category::Home, 400, 720, None),
    ] {
        let mut sh = Shell::new();
        sh.pause_category = category;
        sh.begin(w as f32, h as f32, 1.0, 1.0 / 60.0);
        let mut v = view(&rail);
        if let Some(index) = selected {
            v.kbd = true;
            v.rail_sel = Some(index);
        }
        draw(&mut sh, &v);
        sh.finish();
        let pixels = sh.picture_for_shot(&renderer, w, h).expect("drawn screens");
        assert!(pixels[(((h / 2) * w + w - 2) * 4 + 3) as usize] < 255, "{name}: home background must retain the veil");
        image::save_buffer(out.join(format!("{name}.png")), &pixels, w, h, image::ColorType::Rgba8).unwrap();
        println!("preview: {}", out.join(format!("{name}.png")).display());
    }
}

#[test]
fn pause_navigation_short_window_keeps_cards_clickable() {
    let rail = menu();
    let mut sh = Shell::new();
    sh.begin(400.0, 320.0, 1.0, 1.0 / 60.0);
    sh.ui.input.mouse = Vec2::new(100.0, 190.0);
    sh.ui.input.pressed = true;
    sh.ui.input.released = true;
    draw(&mut sh, &view(&rail));
    sh.finish();
    assert_eq!(sh.actions, [Action::Choose(2)]);
}

#[test]
fn pause_navigation_flat_cards_keep_keyboard_focus_visible() {
    let rail = menu();
    let v = view(&rail);
    let mut counts = Vec::new();
    for selected in [false, true] {
        let mut sh = Shell::new();
        sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
        assert!(sh.ui.flat);
        action_card(&mut sh.ui, &v, 2, Rect::new(350.0, 184.0, 480.0, 120.0), selected, false);
        counts.push(sh.ui.p().verts.iter().filter(|vertex| vertex.color == ACCENT.0).count());
    }
    assert_eq!(counts[0], 0);
    assert!(counts[1] > 0, "Keyboard focus must survive the flat theme");
}

#[test]
fn pause_navigation_quit_keeps_keyboard_focus_visible() {
    let rail = menu();
    let mut v = view(&rail);
    v.kbd = true;
    v.rail_sel = rail.iter().position(|line| line.0 == "quit");
    let mut sh = Shell::new();
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    session_action(&mut sh, &v, "quit", Rect::new(16.0, 768.0, 204.0, 54.0), ButtonKind::Danger);
    assert!(sh.ui.p().verts.iter().any(|vertex| vertex.color == ACCENT.0));
}

#[test]
fn pause_navigation_enlarged_quit_dispatches_original_action() {
    let rail = menu();
    let mut sh = Shell::new();
    click(&mut sh, &view(&rail), Vec2::new(110.0, 815.0));
    assert_eq!(sh.actions, [Action::Choose(rail.len() - 1)]);
}

#[test]
fn pause_navigation_open_list_preserves_origin_category() {
    let rail = menu();
    for origin in [Category::Home, Category::Settings, Category::Driving] {
        for target in ["camera", "duty"] {
            let mut sh = Shell::new();
            sh.pause_category = origin;
            let mut v = view(&rail);
            v.rail_open = Some(target);
            v.list = Some(ListView {
                kind: MenuKind::Options, head: None, tabs: None,
                items: vec![("back", "Back")], sel: 0, preview: None,
                pane_first: None, dropdown: None, key: target.into(),
            });
            sh.begin(1366.0, 768.0, 1.0, 1.0 / 60.0);
            draw(&mut sh, &v);
            assert_eq!(sh.pause_category, origin, "opening {target}");
            v.list = None;
            v.rail_open = None;
            draw(&mut sh, &v);
            assert_eq!(sh.pause_category, origin, "returning from {target}");
        }
    }
}
