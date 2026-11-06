use super::*;

fn session() -> View {
    View {
        speeds: crate::admin::SPEEDS.to_vec(),
        my_id: 1,
        rights: Some(Role::Admin),
        voice: true,
        time: 21600.0,
        speed: 1.0,
        traffic: Some(120),
        traffic_next: "traffic next".into(),
        weather: vec![
            ("Weather/sun.owt".into(), "Bestes Wetter".into()),
            ("Weather/rain.owt".into(), "Rain".into()),
        ],
        players: ["shamough1792", "Alex", "BusFan", "Chris"]
            .iter()
            .enumerate()
            .map(|(i, name)| Player {
                id: i as u32 + 1,
                conn: 100 + i as u64,
                name: name.to_string(),
                state: if i == 3 {
                    PlayerState::OnFoot
                } else {
                    PlayerState::Driving
                },
                vehicle: if i == 3 {
                    String::new()
                } else {
                    format!("Vehicles/MAN/MAN_D{}.bus", 86 + i)
                },
                line: String::new(),
                tour: String::new(),
                aboard: None,
                online_secs: 60,
                admin: (i == 0).then_some(Role::Admin),
            })
            .collect(),
        ..Default::default()
    }
}

fn view(rail: &[crate::app_impl::MenuLine]) -> PauseView<'_> {
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
        summary: Summary::default(),
        online: session(),
    }
}

fn render(sh: &mut Shell, tab: usize, v: &View) {
    sh.pause_online.tab = tab;
    sh.begin(1440.0, 900.0, 1.0, 0.016);
    match tab {
        0 => players(sh, v, Rect::new(280.0, 300.0, 1000.0, 550.0)),
        1 => world(sh, v, Rect::new(280.0, 300.0, 1000.0, 550.0)),
        _ => bans(sh, v, Rect::new(280.0, 300.0, 1000.0, 550.0)),
    }
}

fn click(sh: &mut Shell, tab: usize, v: &View, id: &str) {
    render(sh, tab, v);
    let r = *sh.ui.drawn.get(&id_of(id)).expect("visible control");
    sh.finish();
    sh.begin(1440.0, 900.0, 1.0, 0.016);
    sh.ui.input.mouse = r.center();
    sh.ui.input.pressed = true;
    sh.ui.input.released = true;
    match tab {
        0 => players(sh, v, Rect::new(280.0, 300.0, 1000.0, 550.0)),
        1 => world(sh, v, Rect::new(280.0, 300.0, 1000.0, 550.0)),
        _ => bans(sh, v, Rect::new(280.0, 300.0, 1000.0, 550.0)),
    }
}

#[test]
fn online_room_controls_dispatch_the_original_permission_filtered_menu_indices() {
    let rail = [
        ("resume", "Resume".into(), None),
        ("copycode", "Copy server code".into(), None),
        ("players", "Players".into(), None),
        ("multiplayer", "Multiplayer".into(), None),
        ("quit", "Quit".into(), None),
    ];
    for (width, height) in [(1440.0, 900.0), (400.0, 720.0)] {
        for (id, index) in [("copycode", 1), ("multiplayer", 3)] {
            let mut sh = Shell::new();
            sh.pause_category = navigation::Category::Online;
            sh.pause_hidden = false;
            sh.begin(width, height, 1.0, 0.016);
            super::super::draw(&mut sh, &view(&rail));
            let r = *sh
                .ui
                .drawn
                .get(&id_of(&format!("pause-online-room-{id}")))
                .expect("room control must remain accessible");
            assert!(r.x >= 0.0 && r.right() <= width && r.y >= 0.0 && r.bottom() <= height);
            sh.finish();
            sh.begin(width, height, 1.0, 0.016);
            sh.ui.input.mouse = r.center();
            sh.ui.input.pressed = true;
            sh.ui.input.released = true;
            super::super::draw(&mut sh, &view(&rail));
            assert_eq!(sh.actions, [Action::Choose(index)]);
        }
    }
}

#[test]
fn online_room_controls_do_not_create_unavailable_or_disabled_actions() {
    let rail = [
        ("resume", "Resume".into(), None),
        ("players", "Players".into(), None),
        ("multiplayer", "Multiplayer".into(), None),
    ];
    let mut v = view(&rail);
    v.disabled = &["multiplayer"];
    let mut sh = Shell::new();
    sh.pause_category = navigation::Category::Online;
    sh.pause_hidden = false;
    sh.begin(1440.0, 900.0, 1.0, 0.016);
    super::super::draw(&mut sh, &v);
    for id in ["copycode", "multiplayer"] {
        assert!(
            !sh.ui
                .drawn
                .contains_key(&id_of(&format!("pause-online-room-{id}")))
        );
    }
}

#[test]
fn online_tabs_display_live_controls_without_an_open_button() {
    let rail = [
        ("resume", "Resume".into(), None),
        ("players", "Players".into(), None),
        ("quit", "Quit".into(), None),
    ];
    for tab in 0..3 {
        let mut sh = Shell::new();
        sh.pause_hidden = false;
        sh.pause_category = navigation::Category::Online;
        sh.pause_online.tab = tab;
        sh.begin(1440.0, 900.0, 1.0, 0.016);
        super::super::draw(&mut sh, &view(&rail));
        assert!(
            !sh.ui
                .drawn
                .contains_key(&id_of(&format!("pause-online-action-{tab}")))
        );
        assert!(sh.ui.drawn.contains_key(&id_of("pause-online-tab-world")));
        if tab == 0 {
            assert!(
                sh.ui
                    .drawn
                    .contains_key(&id_of("pause-online-manage-2#101"))
            );
        }
        if tab == 1 {
            assert!(sh.ui.drawn.contains_key(&id_of("online-time-preset-6")));
        }
    }
}

#[test]
fn online_player_management_opens_a_modal_for_the_selected_connection() {
    let mut v = session();
    let mut sh = Shell::new();
    let target = v.players[1].target();
    click(
        &mut sh,
        0,
        &v,
        &format!("pause-online-manage-{}", target.tag()),
    );
    assert_eq!(sh.pause_online.management, Some((target, None)));
    assert_eq!(
        sh.actions,
        [Action::OnlineAction(0, format!("pick {}", target.tag()))]
    );
    sh.actions.clear();
    assert!(!sh.ui.drawn.contains_key(&id_of(&format!(
        "pause-online-manage-{}",
        v.players[0].target().tag()
    ))));

    v.management = Some((
        v.players[1].clone(),
        vec![("bring here".into(), "run bring 2#101".into())],
    ));
    sh.begin(1440.0, 900.0, 1.0, 0.016);
    management_modal(&mut sh, &v);
    assert!(
        sh.ui
            .drawn
            .contains_key(&id_of("pause-online-manage-action-0"))
    );
    let r = *sh
        .ui
        .drawn
        .get(&id_of("pause-online-manage-action-0"))
        .unwrap();
    sh.finish();
    sh.begin(1440.0, 900.0, 1.0, 0.016);
    sh.ui.input.mouse = r.center();
    sh.ui.input.pressed = true;
    sh.ui.input.released = true;
    management_modal(&mut sh, &v);
    assert_eq!(
        sh.actions,
        [Action::OnlineAction(0, "run bring 2#101".into())]
    );

    sh.actions.clear();
    sh.pause_online.management = Some((target, None));
    v.management = Some((
        v.players[1].clone(),
        vec![(
            "Ban this machine...".into(),
            format!("do ban {}", target.tag()),
        )],
    ));
    sh.begin(1440.0, 900.0, 1.0, 0.016);
    management_modal(&mut sh, &v);
    let r = *sh
        .ui
        .drawn
        .get(&id_of("pause-online-manage-action-0"))
        .unwrap();
    sh.finish();
    sh.begin(1440.0, 900.0, 1.0, 0.016);
    sh.ui.input.mouse = r.center();
    sh.ui.input.pressed = true;
    sh.ui.input.released = true;
    management_modal(&mut sh, &v);
    assert_eq!(sh.pause_online.management, Some((target, Some("ban".into()))));
    assert!(sh.actions.is_empty());
}

#[test]
fn online_management_modal_blocks_the_page_and_closes_after_disconnect() {
    let rail = [
        ("resume", "Resume".into(), None),
        ("players", "Players".into(), None),
    ];
    let mut sh = Shell::new();
    sh.pause_category = navigation::Category::Online;
    sh.pause_hidden = false;
    let mut v = view(&rail);
    let player = v.online.players[1].clone();
    sh.pause_online.management = Some((player.target(), None));
    v.online.management = Some((player, vec![]));
    sh.begin(1440.0, 900.0, 1.0, 0.016);
    super::super::draw(&mut sh, &v);
    let tab = *sh.ui.drawn.get(&id_of("pause-online-tab-world")).unwrap();
    sh.finish();
    sh.begin(1440.0, 900.0, 1.0, 0.016);
    sh.ui.input.mouse = tab.center();
    sh.ui.input.pressed = true;
    sh.ui.input.released = true;
    super::super::draw(&mut sh, &v);
    assert_eq!(sh.pause_online.tab, 0);
    assert!(sh.actions.is_empty());
    assert!(sh.pause_online.management.is_some());
    v.online.management = None;
    sh.begin(1440.0, 900.0, 1.0, 0.016);
    super::super::draw(&mut sh, &v);
    assert!(sh.pause_online.management.is_none());
}

#[test]
fn online_search_accepts_unicode_keyboard_input() {
    let mut sh = Shell::new();
    sh.ui.focus = Some(id_of("pause-online-search"));
    assert!(super::super::search::key(
        &mut sh,
        winit::keyboard::KeyCode::KeyA,
        Some("名稱")
    ));
    assert_eq!(sh.ui.input.text, "名稱");
}

#[test]
#[ignore = "Requires a graphics adapter and OPENOMSI_PAUSE_PREVIEW"]
fn online_write_preview() {
    omsi_ui::i18n::set_lookup(|lang, text| {
        crate::_rust_i18n_try_translate(lang, text).map(|value| value.into_owned())
    });
    omsi_ui::i18n::set_language("en");
    let out = std::path::PathBuf::from(
        std::env::var_os("OPENOMSI_PAUSE_PREVIEW").expect("preview directory"),
    );
    std::fs::create_dir_all(&out).unwrap();
    let instance = wgpu::Instance::default();
    let renderer = pollster::block_on(omsi_render::Renderer::new(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
    ))
    .unwrap();
    let rail = [
        ("resume", "Resume".into(), None),
        ("players", "Players".into(), None),
        ("copycode", "Copy server code".into(), None),
        ("multiplayer", "Multiplayer".into(), None),
        ("admin", "Administration...".into(), None),
        ("quit", "End the session".into(), None),
    ];
    for (name, tab, w, h, light) in [
        ("players", 0, 1440, 900, false),
        ("players-light", 0, 1440, 900, true),
        ("world", 1, 1440, 900, false),
        ("bans", 2, 1440, 900, false),
        ("world-laptop", 1, 1366, 768, false),
        ("players-narrow", 0, 400, 720, false),
        ("world-light", 1, 1440, 900, true),
    ] {
        omsi_ui::theme::with_palette(omsi_ui::theme::palette(light), || {
            let mut sh = Shell::new();
            sh.pause_hidden = false;
            sh.pause_category = navigation::Category::Online;
            sh.pause_online.tab = tab;
            sh.begin(w as f32, h as f32, 1.0, 0.016);
            super::super::draw(&mut sh, &view(&rail));
            sh.finish();
            let pixels = sh.picture_for_shot(&renderer, w, h).unwrap();
            image::save_buffer(
                out.join(format!("online-{name}.png")),
                &pixels,
                w,
                h,
                image::ColorType::Rgba8,
            )
            .unwrap();
        });
    }
    for (name, w, h, light) in [
        ("manage", 1440, 900, false),
        ("manage-narrow", 400, 720, false),
        ("manage-light", 1440, 900, true),
    ] {
        omsi_ui::theme::with_palette(omsi_ui::theme::palette(light), || {
            let mut sh = Shell::new();
            sh.pause_hidden = false;
            sh.pause_category = navigation::Category::Online;
            let v = session();
            let target = v.players[1].target();
            sh.pause_online.management = Some((target, None));
            let mut page = view(&rail);
            page.online.management = Some((
                v.players[1].clone(),
                vec![
                    (
                        "Voice chat: audible. Mute them".into(),
                        format!("vmute {}", target.tag()),
                    ),
                    ("go to".into(), format!("run goto {}", target.tag())),
                    ("bring here".into(), format!("run bring {}", target.tag())),
                    (
                        "repair and refuel their bus".into(),
                        format!("run service repair {}", target.tag()),
                    ),
                    (
                        "put their bus back on its wheels".into(),
                        format!("run unstick {}", target.tag()),
                    ),
                    ("Send away...".into(), format!("do kick {}", target.tag())),
                    (
                        "Ban this machine...".into(),
                        format!("do ban {}", target.tag()),
                    ),
                ],
            ));
            sh.begin(w as f32, h as f32, 1.0, 0.016);
            super::super::draw(&mut sh, &page);
            sh.finish();
            let pixels = sh.picture_for_shot(&renderer, w, h).unwrap();
            image::save_buffer(
                out.join(format!("online-{name}.png")),
                &pixels,
                w,
                h,
                image::ColorType::Rgba8,
            )
            .unwrap();
        });
    }
    for (name, w, h, light) in [
        ("confirm", 1440, 900, false),
        ("confirm-light", 1440, 900, true),
        ("confirm-narrow", 400, 720, false),
    ] {
        omsi_ui::theme::with_palette(omsi_ui::theme::palette(light), || {
            let mut sh = Shell::new();
            sh.pause_hidden = false;
            sh.pause_category = navigation::Category::Online;
            sh.pause_online.tab = 1;
            sh.pause_online.confirm = Some((1, "traffic clear".into()));
            sh.begin(w as f32, h as f32, 1.0, 0.016);
            super::super::draw(&mut sh, &view(&rail));
            sh.finish();
            let pixels = sh.picture_for_shot(&renderer, w, h).unwrap();
            image::save_buffer(
                out.join(format!("online-{name}.png")),
                &pixels,
                w,
                h,
                image::ColorType::Rgba8,
            )
            .unwrap();
        });
    }
}
