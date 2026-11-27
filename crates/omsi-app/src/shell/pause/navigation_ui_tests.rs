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
        list: None,
        kbd: false,
        keys: true,
        summary: Summary {
            bus: "Test vehicle".into(),
            driver: "Driver".into(),
        },
        online: Default::default(),
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

fn click_drawn(sh: &mut Shell, v: &PauseView, id: &str) {
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    draw(sh, v);
    let at = sh
        .ui
        .drawn
        .get(&id_of(id))
        .unwrap_or_else(|| panic!("missing clickable {id}"))
        .center();
    sh.finish();
    click(sh, v, at);
}

fn click_drawn_control(sh: &mut Shell, v: &PauseView, id: &str) {
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    draw(sh, v);
    let at = sh
        .ui
        .drawn
        .get(&id_of(id))
        .unwrap_or_else(|| panic!("missing control {id}"))
        .center();
    sh.finish();
    click(sh, v, at);
}

#[test]
fn pause_navigation_card_keeps_the_original_action_index() {
    let rail = menu();
    let mut sh = Shell::new();
    sh.pause_hidden = false;
    click_drawn(&mut sh, &view(&rail), "pause-card-2-camera");
    assert_eq!(sh.actions, [Action::Choose(2)]);
}

#[test]
fn quick_actions_use_separate_two_column_pill_cards_on_wide_pages() {
    let rail = menu();
    let mut sh = Shell::new();
    sh.pause_hidden = false;
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    draw(&mut sh, &view(&rail));
    let camera = sh.ui.drawn[&id_of("pause-card-2-camera")];
    let map = sh.ui.drawn[&id_of("pause-card-3-map")];
    assert_eq!(camera.h, 120.0);
    assert_eq!(camera.y, map.y);
    assert!(map.x > camera.x);
}

#[test]
fn pause_draws_a_full_height_divider_between_rail_and_page() {
    let rail = menu();
    let mut sh = Shell::new();
    sh.pause_hidden = false;
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    draw(&mut sh, &view(&rail));
    let panel = super::frame_rect(sh.ui.size);
    let divider_x = panel.x + RAIL_W - 0.5;
    let divider_color = omsi_ui::theme::th().edge.alpha(2.0).0;
    sh.finish();
    let vertices = &sh.drawn.as_ref().unwrap().verts;
    assert!(vertices.iter().any(|vertex| {
        vertex.color == divider_color && vertex.pos[0] == divider_x && vertex.pos[1] == panel.y
    }));
    assert!(vertices.iter().any(|vertex| {
        vertex.color == divider_color
            && vertex.pos[0] == divider_x
            && vertex.pos[1] == panel.bottom()
    }));
}

#[test]
fn pause_outer_frame_is_drawn_after_the_sidebar_fill() {
    let rail = menu();
    let mut sh = Shell::new();
    sh.pause_hidden = false;
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    draw(&mut sh, &view(&rail));
    let color = crate::shell::pause::visible_edge().0;
    sh.finish();
    let vertices = &sh.drawn.as_ref().unwrap().verts;
    assert!(vertices.len() >= 6);
    assert!(
        vertices[vertices.len() - 6..]
            .iter()
            .all(|vertex| { vertex.mode[0] == 4.0 && vertex.color == color })
    );
}

#[test]
fn quick_action_pill_cards_have_a_visible_outline() {
    let rail = menu();
    let v = view(&rail);
    let mut sh = Shell::new();
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    action_card(
        &mut sh.ui,
        &v,
        2,
        Rect::new(300.0, 200.0, 320.0, 120.0),
        false,
        true,
    );
    assert!(
        sh.ui
            .p()
            .verts
            .iter()
            .any(|v| { v.color == omsi_ui::Color::WHITE.alpha(0.08).0 })
    );
}

#[test]
fn pause_navigation_category_click_is_not_a_game_action() {
    let rail = menu();
    let mut sh = Shell::new();
    sh.pause_hidden = false;
    click_drawn(&mut sh, &view(&rail), "pause-category-Saves");
    assert_eq!(sh.actions, [Action::PauseCategory(Category::Saves)]);
}

#[test]
fn pause_navigation_places_categories_under_their_left_rail_groups() {
    let ids = [
        "resume",
        "options",
        "controls",
        "camera",
        "vehicle",
        "world",
        "map",
        "duty",
        "save",
        "shot",
        "players",
        "multiplayer",
        "plugin",
        "quit",
    ];
    assert_eq!(
        super::rail_entries(&Category::available(&ids)),
        [
            super::RailEntry::Category(Category::Home),
            super::RailEntry::Group("Simulation"),
            super::RailEntry::Category(Category::Driving),
            super::RailEntry::Category(Category::World),
            super::RailEntry::Category(Category::Saves),
            super::RailEntry::Group("Tools"),
            super::RailEntry::Category(Category::Capture),
            super::RailEntry::Group("Preferences"),
            super::RailEntry::Category(Category::Settings),
            super::RailEntry::Group("Community"),
            super::RailEntry::Category(Category::Online),
            super::RailEntry::Category(Category::Plugins),
        ]
    );
}

#[test]
fn pause_navigation_rail_does_not_add_a_second_frame() {
    let rail = menu();
    let v = view(&rail);
    let mut sh = Shell::new();
    sh.begin(800.0, 720.0, 1.0, 1.0 / 60.0);
    super::rail(&mut sh, &v, Rect::new(24.0, 20.0, RAIL_W, 680.0));
    let frame_color = omsi_ui::theme::th().edge.alpha(2.0).0;
    sh.finish();
    assert!(
        !sh.drawn
            .as_ref()
            .unwrap()
            .verts
            .iter()
            .any(|vertex| vertex.color == frame_color)
    );
}

#[test]
fn pause_quit_action_has_no_idle_outline() {
    let rail = menu();
    let v = view(&rail);
    let mut sh = Shell::new();
    sh.begin(800.0, 720.0, 1.0, 1.0 / 60.0);
    super::session_action(
        &mut sh,
        &v,
        "quit",
        Rect::new(20.0, 640.0, 220.0, 44.0),
        ButtonKind::Danger,
    );
    let edge = super::super::visible_edge().0;
    sh.finish();
    assert!(
        !sh.drawn
            .as_ref()
            .unwrap()
            .verts
            .iter()
            .any(|vertex| vertex.color == edge)
    );
}

#[test]
fn pause_navigation_online_page_uses_tabs_instead_of_category_cards() {
    let rail = [
        ("resume", "Resume".into(), None),
        ("players", "Players...".into(), None),
        ("world", "World options...".into(), None),
        ("admin", "Administration...".into(), None),
        ("quit", "End the session".into(), None),
    ];
    let v = view(&rail);
    let mut sh = Shell::new();
    sh.pause_hidden = false;
    sh.pause_category = Category::Online;
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    draw(&mut sh, &v);
    assert!(
        sh.ui
            .drawn
            .contains_key(&omsi_ui::kit::id_of("pause-online-tab-players"))
    );
    assert!(
        !sh.ui
            .drawn
            .contains_key(&omsi_ui::kit::id_of("pause-card-1-players"))
    );
    sh.finish();
}

#[test]
fn pause_options_side_navigation_keeps_the_existing_side_action() {
    let rail = menu();
    let rows = vec![
        ("first", "One\u{1f}s\u{1f}off"),
        ("second", "Two\u{1f}s\u{1f}off"),
        ("third", "Three\u{1f}s\u{1f}off"),
    ];
    let titles = vec![
        "Display and driver".into(),
        "Vehicles".into(),
        "Service".into(),
    ];
    let mut v = view(&rail);
    v.list = Some(ListView {
        kind: MenuKind::Options,
        head: Some(("Vehicle options...".into(), String::new())),
        badge: None,
        tabs: Some((titles, 0)),
        items: rows,
        sel: 0,
        preview: None,
        pane_first: None,
        dropdown: None,
        key: "Vehicle0".into(),
    });
    let mut sh = Shell::new();
    sh.pause_hidden = false;
    click_drawn_control(&mut sh, &v, "pause-tab-1");
    assert_eq!(sh.actions, [Action::Side(1)]);
}

#[test]
fn pause_navigation_disabled_card_does_not_dispatch() {
    let rail = menu();
    let mut v = view(&rail);
    v.disabled = &["camera"];
    let mut sh = Shell::new();
    sh.pause_hidden = false;
    click_drawn(&mut sh, &v, "pause-card-2-camera");
    assert!(sh.actions.is_empty());
}

#[test]
fn pause_navigation_draws_each_category_at_small_and_large_sizes() {
    let rail = menu();
    let mut sh = Shell::new();
    sh.pause_hidden = false;
    for (w, h) in [
        (320.0, 480.0),
        (480.0, 800.0),
        (1024.0, 600.0),
        (1920.0, 1080.0),
    ] {
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
fn pause_frame_is_centered_and_capped_on_large_screens() {
    assert_eq!(
        frame_rect(Vec2::new(1440.0, 900.0)),
        Rect::new(120.0, 40.0, 1200.0, 820.0),
    );
}

#[test]
fn pause_frame_fills_small_screens_with_equal_margins() {
    let size = Vec2::new(400.0, 640.0);
    let frame = frame_rect(size);
    assert_eq!(frame.x, 12.0);
    assert_eq!(frame.y, 12.0);
    assert_eq!(frame.right(), size.x - 12.0);
    assert_eq!(frame.bottom(), size.y - 12.0);
}

#[test]
fn pause_navigation_category_icons_exist_in_the_shared_icon_set() {
    for category in Category::ALL {
        assert!(
            omsi_ui::icons::names().any(|name| name == category.icon()),
            "{:?}",
            category
        );
    }
}

#[test]
#[ignore = "Requires a graphics adapter and OPENOMSI_PAUSE_PREVIEW output directory"]
fn pause_navigation_write_preview() {
    omsi_ui::i18n::set_lookup(|lang, text| {
        crate::_rust_i18n_try_translate(lang, text).map(|value| value.into_owned())
    });
    omsi_ui::i18n::set_language("en");
    let out = std::path::PathBuf::from(
        std::env::var_os("OPENOMSI_PAUSE_PREVIEW").expect("set OPENOMSI_PAUSE_PREVIEW"),
    );
    std::fs::create_dir_all(&out).unwrap();
    let instance = wgpu::Instance::default();
    let renderer = pollster::block_on(omsi_render::Renderer::new(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
    ))
    .unwrap();
    let mut rail = menu();
    for (id, label) in [
        ("vehicle", "Vehicle options..."),
        ("world", "World options..."),
        ("controls", "Controls..."),
        ("saveslot", "Save to a slot..."),
        ("load", "Load a situation..."),
        ("photo", "Photo mode"),
        ("skipstop", "Skip the next stop"),
        ("endduty", "End the tour"),
    ] {
        rail.insert(rail.len() - 1, (id, label.into(), None));
    }
    rail.insert(1, ("multiplayer", "Multiplayer...".into(), None));
    for (name, category, w, h, selected) in [
        ("pause-home", Category::Home, 1440, 900, None),
        ("pause-home-focus", Category::Home, 1440, 900, Some(2)),
        (
            "pause-quit-focus",
            Category::Home,
            1440,
            900,
            Some(rail.len() - 1),
        ),
        ("pause-saves", Category::Saves, 1440, 900, None),
        ("pause-narrow", Category::Home, 400, 720, None),
        ("quick-actions-editor", Category::Home, 1440, 900, None),
        (
            "quick-actions-editor-laptop",
            Category::Home,
            1366,
            768,
            None,
        ),
        (
            "quick-actions-editor-narrow",
            Category::Home,
            400,
            720,
            None,
        ),
        ("quick-actions-editor-short", Category::Home, 400, 320, None),
        ("quick-actions-custom", Category::Home, 1440, 900, None),
    ] {
        let mut sh = Shell::new();
        sh.pause_hidden = false;
        sh.pause_category = category;
        sh.quick_edit = name.starts_with("quick-actions-editor");
        sh.begin(w as f32, h as f32, 1.0, 1.0 / 60.0);
        let mut v = view(&rail);
        if name.starts_with("quick-actions-") {
            v.quick_actions = r#"["save","world","camera","map"]"#;
        }
        if let Some(index) = selected {
            v.kbd = true;
            v.rail_sel = Some(index);
        }
        draw(&mut sh, &v);
        sh.finish();
        let pixels = sh.picture_for_shot(&renderer, w, h).expect("drawn screens");
        assert!(
            pixels[(((h / 2) * w + w - 2) * 4 + 3) as usize] < 255,
            "{name}: home background must retain the veil"
        );
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
fn pause_navigation_short_window_keeps_cards_clickable() {
    let rail = menu();
    let mut sh = Shell::new();
    sh.pause_hidden = false;
    sh.begin(400.0, 320.0, 1.0, 1.0 / 60.0);
    sh.ui.input.mouse = Vec2::new(100.0, 230.0);
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
        sh.pause_hidden = false;
        sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
        assert!(sh.ui.flat);
        action_card(
            &mut sh.ui,
            &v,
            2,
            Rect::new(350.0, 184.0, 480.0, 120.0),
            selected,
            false,
        );
        counts.push(
            sh.ui
                .p()
                .verts
                .iter()
                .filter(|vertex| vertex.color == th().accent.0)
                .count(),
        );
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
    sh.pause_hidden = false;
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    session_action(
        &mut sh,
        &v,
        "quit",
        Rect::new(16.0, 768.0, 204.0, 54.0),
        ButtonKind::Danger,
    );
    assert!(
        sh.ui
            .p()
            .verts
            .iter()
            .any(|vertex| vertex.color == th().accent.0)
    );
}

#[test]
fn pause_navigation_enlarged_quit_dispatches_original_action() {
    let rail = menu();
    let mut sh = Shell::new();
    sh.pause_hidden = false;
    click_drawn(&mut sh, &view(&rail), "pause-session-quit");
    assert_eq!(sh.actions, [Action::Choose(rail.len() - 1)]);
}

#[test]
fn pause_navigation_open_list_preserves_origin_category() {
    let rail = menu();
    for origin in [Category::Home, Category::Settings, Category::Driving] {
        for target in ["camera", "duty"] {
            let mut sh = Shell::new();
            sh.pause_hidden = false;
            sh.pause_category = origin;
            let mut v = view(&rail);
            v.list = Some(ListView {
                kind: MenuKind::Options,
                head: None,
                badge: None,
                tabs: None,
                items: vec![("back", "Back")],
                sel: 0,
                preview: None,
                pane_first: None,
                dropdown: None,
                key: target.into(),
            });
            sh.begin(1366.0, 768.0, 1.0, 1.0 / 60.0);
            draw(&mut sh, &v);
            assert_eq!(sh.pause_category, origin, "opening {target}");
            v.list = None;
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
    let mut sh = Shell::new();
    sh.pause_hidden = false;
    click_drawn(&mut sh, &v, "pause-card-5-save");
    assert_eq!(sh.actions, [Action::Choose(5)]);
}

#[test]
fn quick_actions_editor_toggle_never_executes_action() {
    let rail = menu();
    let mut sh = Shell::new();
    sh.pause_hidden = false;
    sh.quick_edit = true;
    click_drawn(&mut sh, &view(&rail), "pause-quick-toggle-map");
    assert!(
        sh.actions
            .iter()
            .any(|a| matches!(a, Action::QuickActions(_)))
    );
    assert!(
        !sh.actions
            .iter()
            .any(|a| matches!(a, Action::Choose(_) | Action::Rail(_)))
    );
}

#[test]
fn quick_actions_empty_page_keeps_edit_entry_accessible() {
    let rail = menu();
    let mut v = view(&rail);
    v.quick_actions = "[]";
    let mut sh = Shell::new();
    sh.pause_hidden = false;
    // Look up the edit button's actual bounds rather than assuming a fixed
    // position: the pause page changes width and alignment with the layout.
    sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    draw(&mut sh, &v);
    let edit = sh
        .ui
        .drawn
        .get(&omsi_ui::kit::id_of("pause-quick-edit"))
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
    for (w, h, scale) in [
        (320.0, 480.0, 1.0),
        (400.0, 320.0, 1.0),
        (1366.0, 768.0, 1.5),
        (1920.0, 1080.0, 1.0),
    ] {
        let mut sh = Shell::new();
        sh.pause_hidden = false;
        sh.quick_edit = true;
        sh.begin(w, h, scale, 0.016);
        draw(&mut sh, &view(&rail));
        let done = sh
            .ui
            .drawn
            .get(&id_of("pause-quick-done"))
            .expect("Done button should be drawn")
            .center();
        assert!(done.x >= 0.0 && done.x <= sh.ui.size.x && done.y > 0.0);
        sh.finish();
        sh.begin(w, h, scale, 0.016);
        sh.pointer(done.x * scale, done.y * scale);
        sh.ui.input.pressed = true;
        sh.ui.input.released = true;
        draw(&mut sh, &view(&rail));
        assert!(
            !sh.quick_edit,
            "Done must remain clickable at {w}x{h}, scale {scale}"
        );
        assert!(sh.actions.is_empty());
    }
}

#[test]
fn pause_navigation_uses_runtime_theme_palette() {
    let rail = menu();
    let v = view(&rail);
    let mut dark = Shell::new();
    dark.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
    draw(&mut dark, &v);
    dark.finish();

    let mut light = Shell::new();
    omsi_ui::theme::with_palette(Palette::LIGHT, || {
        light.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
        draw(&mut light, &v);
        light.finish();
    });

    assert!(
        dark.drawn
            .as_ref()
            .unwrap()
            .verts
            .iter()
            .any(|v| v.color == Palette::OPENOMSI.sidebar.0)
    );
    assert!(
        light
            .drawn
            .as_ref()
            .unwrap()
            .verts
            .iter()
            .any(|v| v.color == Palette::LIGHT.sidebar.0)
    );
}

#[test]
fn quick_actions_editor_uses_runtime_theme_palette() {
    let rail = menu();
    for (light_theme, surface) in [
        (false, Palette::OPENOMSI.surface),
        (true, Palette::LIGHT.surface),
    ] {
        let mut sh = Shell::new();
        omsi_ui::theme::with_palette(omsi_ui::theme::palette(light_theme), || {
            sh.begin(1440.0, 900.0, 1.0, 1.0 / 60.0);
            super::super::quick_actions_ui::editor(
                &mut sh,
                &view(&rail),
                Rect::new(300.0, 120.0, 800.0, 650.0),
            );
            sh.finish();
        });

        assert!(
            sh.drawn
                .as_ref()
                .unwrap()
                .verts
                .iter()
                .any(|v| v.color == surface.0),
            "quick actions editor should draw its themed surface in light={light_theme}"
        );
    }
}
