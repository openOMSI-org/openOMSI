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
        quick_actions: "default",
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
    let mut sh = Shell::new(); sh.pause_hidden = false;
    click(&mut sh, &view(&rail), Vec2::new(400.0, 225.0));
    assert_eq!(sh.actions, [Action::Choose(2)]);
}

#[test]
fn pause_navigation_category_click_is_not_a_game_action() {
    let rail = menu();
    let mut sh = Shell::new(); sh.pause_hidden = false;
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
    let mut sh = Shell::new(); sh.pause_hidden = false;
    click(&mut sh, &v, Vec2::new(400.0, 225.0));
    assert!(sh.actions.is_empty());
}

#[test]
fn pause_navigation_draws_each_category_at_small_and_large_sizes() {
    let rail = menu();
    let mut sh = Shell::new(); sh.pause_hidden = false;
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
        ("quick-actions-editor", Category::Home, 1440, 900, None),
        ("quick-actions-editor-laptop", Category::Home, 1366, 768, None),
        ("quick-actions-editor-narrow", Category::Home, 400, 720, None),
        ("quick-actions-editor-short", Category::Home, 400, 320, None),
        ("quick-actions-custom", Category::Home, 1440, 900, None),
    ] {
        let mut sh = Shell::new(); sh.pause_hidden = false;
        sh.pause_category = category;
        sh.quick_edit = name.starts_with("quick-actions-editor");
        sh.begin(w as f32, h as f32, 1.0, 1.0 / 60.0);
        let mut v = view(&rail);
        if name.starts_with("quick-actions-") { v.quick_actions = r#"["save","world","camera","map"]"#; }
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
    let mut sh = Shell::new(); sh.pause_hidden = false;
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
        let mut sh = Shell::new(); sh.pause_hidden = false;
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
    let mut sh = Shell::new(); sh.pause_hidden = false;
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    session_action(&mut sh, &v, "quit", Rect::new(16.0, 768.0, 204.0, 54.0), ButtonKind::Danger);
    assert!(sh.ui.p().verts.iter().any(|vertex| vertex.color == ACCENT.0));
}

#[test]
fn pause_navigation_enlarged_quit_dispatches_original_action() {
    let rail = menu();
    let mut sh = Shell::new(); sh.pause_hidden = false;
    click(&mut sh, &view(&rail), Vec2::new(110.0, 815.0));
    assert_eq!(sh.actions, [Action::Choose(rail.len() - 1)]);
}

#[test]
fn pause_navigation_open_list_preserves_origin_category() {
    let rail = menu();
    for origin in [Category::Home, Category::Settings, Category::Driving] {
        for target in ["camera", "duty"] {
            let mut sh = Shell::new(); sh.pause_hidden = false;
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

#[test]
fn quick_actions_custom_card_keeps_original_index() {
    let rail = menu();
    let mut v = view(&rail);
    v.quick_actions = r#"["save","camera"]"#;
    let mut sh = Shell::new(); sh.pause_hidden = false;
    click(&mut sh, &v, Vec2::new(400.0, 225.0));
    assert_eq!(sh.actions, [Action::Choose(5)]);
}

#[test]
fn quick_actions_editor_toggle_never_executes_action() {
    let rail = menu();
    let mut sh = Shell::new(); sh.pause_hidden = false;
    sh.quick_edit = true;
    click(&mut sh, &view(&rail), Vec2::new(400.0, 265.0));
    assert!(sh.actions.iter().any(|a| matches!(a, Action::QuickActions(_))));
    assert!(!sh.actions.iter().any(|a| matches!(a, Action::Choose(_) | Action::Rail(_))));
}

#[test]
fn quick_actions_empty_page_keeps_edit_entry_accessible() {
    let rail = menu();
    let mut v = view(&rail);
    v.quick_actions = "[]";
    let mut sh = Shell::new(); sh.pause_hidden = false;
    // Look up the edit button's actual bounds rather than assuming a fixed
    // position: the pause page changes width and alignment with the layout.
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    draw(&mut sh, &v);
    let edit = sh.ui.drawn.get(&omsi_ui::kit::id_of("pause-quick-edit"))
        .expect("edit button should be accessible with an empty quick-actions page")
        .center();
    sh.finish();
    click(&mut sh, &v, edit);
    assert!(sh.quick_edit);
    assert!(sh.actions.is_empty());
}

#[test]
fn quick_actions_editor_draws_checkbox_and_footer_at_all_sizes() {
    let rail = menu();
    for (w, h, scale) in [(320.0, 480.0, 1.0), (400.0, 320.0, 1.0), (1366.0, 768.0, 1.5), (1920.0, 1080.0, 1.0)] {
        let mut sh = Shell::new(); sh.pause_hidden = false;
        sh.quick_edit = true;
        sh.begin(w, h, scale, 0.016);
        draw(&mut sh, &view(&rail));
        let sidebar = sh.ui.size.x >= 700.0;
        let left = if sidebar { RAIL_W } else { 0.0 };
        let page_w = sh.ui.size.x - left;
        let pad = if page_w < 420.0 { 18.0 } else { 36.0 };
        let width = (page_w - pad * 2.0).min(980.0);
        let x = left + (page_w - width) * 0.5;
        let button_w = (width - 24.0) * 0.5;
        let done = Vec2::new(x + width - 8.0 - button_w * 0.5, sh.ui.size.y - if sidebar { 48.0 } else { 94.0 } - 27.0);
        assert!(done.x >= 0.0 && done.x <= sh.ui.size.x && done.y > 0.0);
        sh.finish();
        sh.begin(w, h, scale, 0.016);
        sh.pointer(done.x * scale, done.y * scale);
        sh.ui.input.pressed = true;
        sh.ui.input.released = true;
        draw(&mut sh, &view(&rail));
        assert!(!sh.quick_edit, "Done must remain clickable at {w}x{h}, scale {scale}");
        assert!(sh.actions.is_empty());
    }
}
