use super::*;

fn example() -> ProcessPrompt {
    ProcessPrompt {
        id: 7,
        plugin: "Example plugin".into(),
        entry: "C:/Games/openOMSI/Plugins/Example/main.lua".into(),
        file: "C:/Games/openOMSI/Plugins/Example/RenderHelper.exe".into(),
        args: vec!["--render".into(), "dashboard".into()],
        cwd: Some("C:/Games/openOMSI".into()),
    }
}

#[test]
fn showing_a_request_never_implicitly_answers_it() {
    let mut shell = crate::shell::Shell::new();
    for (w, h) in [(1280.0, 720.0), (640.0, 480.0), (320.0, 240.0)] {
        shell.begin(w, h, 1.0, 0.016);
        assert_eq!(
            draw(&mut shell.ui, &example(), &mut shell.process_dialog),
            None
        );
        shell.finish();
        assert!(shell.showing());
        assert!(shell.actions.is_empty());
    }
}

#[test]
fn permission_text_is_localized_without_translating_plugin_data() {
    let prompt = example();
    for language in crate::_rust_i18n_available_locales() {
        // The translator accepts interface keys only; plugin data must never reach it.
        let translate = |key: &str| {
            assert!(!key.contains("Example") && !key.contains("dashboard"));
            crate::_rust_i18n_try_translate(&language, key)
                .expect(key)
                .into_owned()
        };
        let text = dialog::content(&prompt, true, translate);
        assert!(text.contains(&prompt.file.to_string_lossy().to_string()));
        assert!(text.contains(&prompt.plugin));
        assert!(text.contains("dashboard"));
        for key in [
            "Plugin process permission",
            "Allow once",
            "Always allow",
            "Deny",
            "Details",
            "Hide details",
            "Inherited from the game",
        ] {
            assert!(!translate(key).trim().is_empty(), "{language}: {key}");
        }
        let summary = dialog::summary(&prompt, translate);
        assert!(
            summary.contains("Example plugin")
                && summary.contains("RenderHelper.exe")
                && summary.contains('2'),
            "{language}: {summary}"
        );
        assert!(
            !summary.contains("{count}")
                && !summary.contains("{app}")
                && !summary.contains("{plugin}"),
            "{language}: {summary}"
        );
    }
}

#[test]
fn each_permission_button_requires_its_own_click() {
    for (index, expected) in [
        ProcessDecision::AllowOnce,
        ProcessDecision::AlwaysAllow,
        ProcessDecision::Deny,
    ]
    .into_iter()
    .enumerate()
    {
        let mut shell = crate::shell::Shell::new();
        // 680 x 440 dialog centred in a 1000 x 700 surface.
        shell.pointer(180.0 + (208.0 + 8.0) * index as f32 + 100.0, 534.0);
        shell.button(true);
        shell.begin(1000.0, 700.0, 1.0, 0.016);
        assert_eq!(
            draw(&mut shell.ui, &example(), &mut shell.process_dialog),
            None
        );
        shell.finish();
        shell.button(false);
        shell.begin(1000.0, 700.0, 1.0, 0.016);
        assert_eq!(
            draw(&mut shell.ui, &example(), &mut shell.process_dialog),
            Some(expected)
        );
        shell.finish();
    }
}

#[test]
#[ignore = "requires GPU; exports the actual game permission dialog for visual review"]
fn export_process_permission_dialog() {
    omsi_ui::i18n::set_lookup(|language, key| {
        crate::_rust_i18n_try_translate(language, key).map(|v| v.into_owned())
    });
    omsi_ui::i18n::set_language("hu");
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let renderer = pollster::block_on(omsi_render::Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        omsi_render::RenderOptions {
            msaa: 1,
            ssao: false,
            shadow_size: 1024,
            fxaa: false,
            ..Default::default()
        },
    ))
    .expect("test renderer");
    for (language, expanded, width, height) in [
        ("hu", false, 1000, 700),
        ("hu", true, 1000, 700),
        ("pt", false, 1000, 700),
        ("hu", true, 480, 480),
    ] {
        omsi_ui::i18n::set_language(language);
        let mut shell = crate::shell::Shell::new();
        // Establish the request before selecting its expanded state.
        shell.begin(width as f32, height as f32, 1.0, 0.016);
        assert_eq!(
            draw(&mut shell.ui, &example(), &mut shell.process_dialog),
            None
        );
        shell.finish();
        shell.process_dialog.expanded = expanded;
        shell.begin(width as f32, height as f32, 1.0, 0.016);
        assert_eq!(
            draw(&mut shell.ui, &example(), &mut shell.process_dialog),
            None
        );
        shell.finish();
        let rgba = shell.picture_for_shot(&renderer, width, height).unwrap();
        let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/review-validation/process-permission-{language}-{}-{width}.png",
            if expanded { "details" } else { "summary" }
        ));
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        image::save_buffer(&output, &rgba, width, height, image::ColorType::Rgba8).unwrap();
        println!("Permission dialog preview: {}", output.display());
    }
}

#[test]
fn collapsed_summary_hides_paths_and_arguments_but_keeps_the_warning() {
    let mut prompt = example();
    for args in [
        vec![],
        vec!["one".into()],
        vec!["--render".into(), "dashboard".into()],
    ] {
        prompt.args = args;
        let text = dialog::content(&prompt, false, str::to_owned);
        assert!(text.contains(&format!("with {} argument(s).", prompt.args.len())));
        assert!(text.contains("Example plugin") && text.contains("RenderHelper.exe"));
        assert!(!text.contains("C:/") && !text.contains("dashboard") && !text.contains("--render"));
        assert!(
            text.contains("Only allow programs you trust.")
                && text.contains("Always allow remembers")
        );
        let details = dialog::content(&prompt, true, str::to_owned);
        assert!(details.contains("C:/Games/openOMSI/Plugins/Example/RenderHelper.exe"));
        assert!(details.contains(&format!("{:?}", prompt.args)));
    }
    prompt.plugin = "Literal {app} {count}\n\u{202e}".into();
    let text = dialog::summary(&prompt, str::to_owned);
    assert!(text.contains("Literal {app} {count}\\n\\u{202e}"));
    assert!(!text.contains('\n') && !text.contains('\u{202e}'));
}

#[test]
fn details_toggle_never_approves_and_each_request_starts_collapsed() {
    let mut shell = crate::shell::Shell::new();
    let mut prompt = example();
    // Wide dialog: details button at x=180..820, y=478..510.
    for expanded in [true, false, true] {
        shell.pointer(500.0, 492.0);
        shell.button(true);
        shell.begin(1000.0, 700.0, 1.0, 0.016);
        assert_eq!(
            draw(&mut shell.ui, &prompt, &mut shell.process_dialog),
            None
        );
        shell.finish();
        shell.button(false);
        shell.begin(1000.0, 700.0, 1.0, 0.016);
        assert_eq!(
            draw(&mut shell.ui, &prompt, &mut shell.process_dialog),
            None
        );
        shell.finish();
        assert_eq!(shell.process_dialog.expanded, expanded);
        assert!(shell.actions.is_empty());
    }
    prompt.id += 1;
    shell.begin(1000.0, 700.0, 1.0, 0.016);
    assert_eq!(
        draw(&mut shell.ui, &prompt, &mut shell.process_dialog),
        None
    );
    assert!(!shell.process_dialog.expanded);
    shell.finish();
}
