use super::*;

fn fixture(sel: usize) -> PauseView<'static> {
    PauseView {
        paused: true,
        quick_actions: "default",
        rail: &[],
        rail_sel: Some(0),
        disabled: &[],
        kbd: false,
        keys: true,
        summary: Default::default(),
        online: Default::default(),
        list: Some(ListView {
            kind: MenuKind::Vehicles,
            head: Some(("Place a vehicle".into(), String::new())),
            badge: None,
            tabs: None,
            items: vec![
                ("search", "Search..."),
                ("maker test", "Manufacturer"),
                ("bus Vehicles/Test/Bus.bus", "Double decker"),
                ("back", "Back"),
            ],
            sel,
            preview: None,
            pane_first: None,
            dropdown: None,
            key: "PlaceTypeTest".into(),
        }),
    }
}

#[test]
fn in_game_bus_preview_layout_keeps_both_panes_and_continue_inside_the_page() {
    for (width, height) in [
        (1100.0, 760.0),
        (760.0, 600.0),
        (540.0, 700.0),
        (320.0, 580.0),
        (760.0, 260.0),
    ] {
        let body = Rect::new(0.0, 0.0, width, height);
        let (left, right) = panes(body);
        for rect in [left, right] {
            assert!(rect.x >= body.x && rect.y >= body.y);
            assert!(
                rect.right() <= body.right() + 0.1 && rect.bottom() <= body.bottom() + 0.1,
                "{rect:?}"
            );
        }
        assert!(left.right() <= right.x || left.bottom() <= right.y);
    }
}

#[test]
fn in_game_bus_preview_click_selects_without_spawning_and_continue_keeps_the_index() {
    for action in ["bus Vehicles/Test/Bus.bus", "livery 1", "livery -1"] {
        let mut sh = Shell::new();
        let mut v = fixture(2);
        v.list.as_mut().unwrap().items[2].0 = action;
        sh.begin(1440.0, 900.0, 1.0, 0.016);
        super::super::draw(&mut sh, &v);
        let row = sh.ui.drawn[&id_of("pause-lr-PlaceTypeTest-2")];
        let button = sh.ui.drawn[&id_of("pause-bus-continue")];
        sh.finish();
        sh.begin(1440.0, 900.0, 1.0, 0.016);
        sh.ui.input.mouse = row.center();
        sh.ui.input.pressed = true;
        sh.ui.input.released = true;
        super::super::draw(&mut sh, &v);
        assert_eq!(sh.actions, [Action::BusSelect(2)]);
        sh.finish();
        sh.actions.clear();
        sh.begin(1440.0, 900.0, 1.0, 0.016);
        sh.ui.input.mouse = button.center();
        sh.ui.input.pressed = true;
        sh.ui.input.released = true;
        super::super::draw(&mut sh, &v);
        assert_eq!(sh.actions, [Action::Choose(2)]);
    }
}

#[test]
fn in_game_bus_preview_keeps_search_and_manufacturer_navigation() {
    for k in [0, 1] {
        let mut sh = Shell::new();
        let v = fixture(k);
        sh.begin(1440.0, 900.0, 1.0, 0.016);
        super::super::draw(&mut sh, &v);
        assert!(!sh.ui.drawn.contains_key(&id_of("pause-bus-continue")));
        let row = sh.ui.drawn[&id_of(&format!("pause-lr-PlaceTypeTest-{k}"))];
        sh.finish();
        sh.begin(1440.0, 900.0, 1.0, 0.016);
        sh.ui.input.mouse = row.center();
        sh.ui.input.pressed = true;
        sh.ui.input.released = true;
        super::super::draw(&mut sh, &v);
        assert_eq!(sh.actions, [Action::Choose(k)]);
    }
}

#[test]
#[ignore = "Requires OMSI_ROOT, OPENOMSI_PREVIEW_BUS and OPENOMSI_PAUSE_PREVIEW"]
fn in_game_bus_preview_render_real_vehicle() {
    use crate::launcher::showroom::Look;
    use std::time::{Duration, Instant};
    omsi_ui::i18n::set_lookup(|lang, text| {
        crate::_rust_i18n_try_translate(lang, text).map(|value| value.into_owned())
    });
    omsi_ui::i18n::set_language("zh-tw");
    let root: std::path::PathBuf = std::env::var_os("OMSI_ROOT").expect("OMSI_ROOT").into();
    let owns_root = !omsi_cfg::content_roots().contains(&root);
    if owns_root {
        omsi_cfg::add_content_root(root.clone());
    }
    let bus = std::env::var("OPENOMSI_PREVIEW_BUS").expect("OPENOMSI_PREVIEW_BUS");
    let map = std::env::var("OPENOMSI_PREVIEW_MAP").unwrap_or_default();
    let def = omsi_vehicle::Vehicle::load(&omsi_cfg::resolve_path(&root, &bus)).unwrap();
    let title = format!("{} {}", def.manufacturer, def.type_name);
    let out = std::path::PathBuf::from(
        std::env::var_os("OPENOMSI_PAUSE_PREVIEW").expect("preview directory"),
    );
    std::fs::create_dir_all(&out).unwrap();
    let settings = crate::settings::Settings::load();
    let options = crate::launcher::showroom_options(&settings);
    let instance = crate::graphics_instance();
    let renderer = pollster::block_on(omsi_render::Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        options,
    ))
    .unwrap();
    let mut sh = Shell::new();
    let ambient = omsi_sim::host::ambient_weather();
    sh.bus_preview.select(
        Some(Look {
            root: root.clone(),
            bus: bus.clone(),
            map: map.clone(),
            ..Default::default()
        }),
        title.clone(),
        "Original paint".into(),
    );
    let started = Instant::now();
    while !sh.bus_preview.ready {
        sh.bus_preview.update(options);
        assert!(sh.bus_preview.error.is_none(), "{:?}", sh.bus_preview.error);
        assert!(
            started.elapsed() < Duration::from_secs(180),
            "preview timeout"
        );
        std::thread::sleep(Duration::from_millis(16));
    }
    assert_eq!(
        omsi_sim::host::ambient_weather(),
        ambient,
        "preview must not change the session weather"
    );
    sh.upload_bus_preview(&renderer);
    assert!(
        sh.bus_preview.ready && sh.bus_preview.texture.is_some(),
        "uploaded preview must be visible"
    );
    for (name, w, h, light) in [
        ("bus-preview-desktop", 1440, 900, false),
        ("bus-preview-light", 1440, 900, true),
        ("bus-preview-narrow", 540, 860, false),
        ("bus-preview-short", 960, 540, false),
        ("bus-preview-repaint-menu", 1440, 900, false),
    ] {
        crate::themes::sync("", light);
        sh.begin(w as f32, h as f32, 1.0, 0.016);
        let mut v = fixture(2);
        v.list.as_mut().unwrap().head = Some((
            omsi_ui::tr("Place a vehicle...").into_owned(),
            String::new(),
        ));
        if name == "bus-preview-repaint-menu" {
            let ty =
                omsi_sim::VehicleType::load(&root, &omsi_cfg::resolve_path(&root, &bus)).unwrap();
            let paint = ty.paint_schemes.first().expect("paint scheme").name.clone();
            sh.bus_preview.select(
                Some(Look {
                    root: root.clone(),
                    bus: bus.clone(),
                    map: map.clone(),
                    paint_index: Some(0),
                    ..Default::default()
                }),
                title.clone(),
                paint.clone(),
            );
            let started = Instant::now();
            while !sh.bus_preview.ready {
                sh.bus_preview.update(options);
                assert!(sh.bus_preview.error.is_none(), "{:?}", sh.bus_preview.error);
                assert!(started.elapsed() < Duration::from_secs(180));
                std::thread::sleep(Duration::from_millis(16));
            }
            sh.upload_bus_preview(&renderer);
            let list = v.list.as_mut().unwrap();
            list.head = Some((omsi_ui::tr("Livery...").into_owned(), String::new()));
            list.items = vec![
                ("livery -1", "Default paint"),
                ("livery 0", &paint),
                ("back", "Back"),
            ];
            list.sel = 1;
            super::super::draw(&mut sh, &v);
        } else {
            super::super::draw(&mut sh, &v);
        }
        sh.finish();
        let target = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bus chooser test"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: renderer.format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        sh.render(&renderer, &target.create_view(&Default::default()), w, h);
        let encoder = renderer.device.create_command_encoder(&Default::default());
        let pixels = crate::shell::read_back(&renderer, encoder, &target, w, h);
        image::save_buffer(
            out.join(format!("{name}.png")),
            &pixels,
            w,
            h,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    sh.clear_bus_preview();
    assert!(sh.bus_preview.texture.is_none());
    crate::themes::sync("", false);
    if owns_root {
        omsi_cfg::remove_content_root(&root);
    }
}
