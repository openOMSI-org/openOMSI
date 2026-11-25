//! The dropdowns of the settings windows, the settings file they read and write.

use super::*;

/// The name of the weather in force (the file's name without its ending).
/// The weather files (`Weather/*.owt`) of every content root, by name.
pub(super) fn weather_files() -> Vec<String> {
    let mut files: Vec<String> = omsi_cfg::read_dir_merged("Weather")
        .into_iter()
        .filter(|p| {
            p.extension()
                .map(|e| e.eq_ignore_ascii_case("owt"))
                .unwrap_or(false)
        })
        .filter_map(|p| {
            p.file_name()
                .map(|n| format!("Weather/{}", n.to_string_lossy()))
        })
        .collect();
    files.sort_by(|a, b| {
        bus_cmp(
            a.trim_start_matches("Weather/").trim_start_matches('#'),
            b.trim_start_matches("Weather/").trim_start_matches('#'),
        )
    });
    files.dedup();
    files.insert(0, "tomorrow".into());
    files
}

/// A drop-down over a row of a settings window (the weather preset, the clouds), as a
/// select in a page: the entries drop down under the row's value.
pub(crate) struct Dropdown {
    /// The row of the window it belongs to.
    pub row: usize,
    /// (label, action) of its entries.
    pub items: Vec<(String, String)>,
    /// The entry the keyboard is on, the first one shown, and the one in force now.
    pub sel: usize,
    pub top: usize,
    pub current: Option<usize>,
}

/// The drop-down of the row `row` whose action is `id`, if that row has one.
pub(crate) fn dropdown_for(app: &App, row: usize, id: &str) -> Option<Dropdown> {
    let tr = |t: &str| omsi_ui::tr(t).into_owned();
    if app.metar_locked() && matches!(id, "weather" | "cloudkind" | "precipkind") {
        return None;
    }
    let mut current: Option<usize> = None;
    let items: Vec<(String, String)> = match id {
        "weather" => {
            let now = weather_name(app);
            weather_files()
                .into_iter()
                .enumerate()
                .map(|(i, f)| {
                    let stem = f
                        .rsplit('/')
                        .next()
                        .unwrap_or(&f)
                        .rsplit_once('.')
                        .map(|x| x.0)
                        .unwrap_or(&f)
                        .trim_start_matches('#')
                        .to_string();
                    if stem.eq_ignore_ascii_case(&now) {
                        current = Some(i);
                    }
                    // (shown by its name in the interface's language, not its German file name)
                    let name = if f == "tomorrow" {
                        "Tomorrow.io".into()
                    } else {
                        omsi_launcher_lib::content_path(&f)
                            .map(|p| omsi_launcher_lib::weather_name(&p))
                            .unwrap_or(stem)
                    };
                    (omsi_ui::tr(&name).into_owned(), format!("wx {f}"))
                })
                .collect()
        }
        "metar_src" => {
            let mut v = vec![(tr("Automatic (nearest the map)"), "metar_src ".to_string())];
            let own = &app.settings.metar_station;
            current = Some(0);
            for (i, (code, label)) in crate::weather_setup::metar_airports(&app.args.root)
                .into_iter()
                .enumerate()
            {
                if code.eq_ignore_ascii_case(own) {
                    current = Some(i + 1);
                }
                v.push((label, format!("metar_src {code}")));
            }
            v
        }
        "cloudkind" => {
            current = app
                .session
                .weather
                .as_ref()
                .and_then(|w| cloud_index(&w.clouds.0));
            CLOUD_KINDS
                .iter()
                .enumerate()
                .map(|(i, k)| (tr(k.label), format!("cloud {i}")))
                .collect()
        }
        "precipkind" => {
            current = app.session.weather.as_ref().map(|w| {
                (w.precip.first().copied().unwrap_or(0.0).max(0.0) as usize)
                    .min(PRECIP_KINDS.len() - 1)
            });
            PRECIP_KINDS
                .iter()
                .enumerate()
                .map(|(i, n)| (tr(*n), format!("precip {i}")))
                .collect()
        }
        "preset" => {
            let file = settings_file();
            current = preset_now(&file);
            presets(&file)
                .iter()
                .enumerate()
                .map(|(i, p)| (tr(p.0), format!("preset {i}")))
                .collect()
        }
        "gfxprofile" => omsi_launcher_lib::graphics_profiles()
            .into_keys()
            .map(|n| (n.clone(), format!("gfxprofile {n}")))
            .collect(),
        "reset" => vec![
            (tr("Cancel"), "noop".to_string()),
            (tr("Reset all settings"), "reset_all".to_string()),
        ],
        key if key.starts_with("sel ") => {
            let key = &key[4..];
            let (options, at, _) = select_state(&settings_file(), key);
            current = at;
            options
                .iter()
                .map(|o| (tr(o.1), format!("pick {key} {}", o.0)))
                .collect()
        }
        _ => return None,
    };
    if items.is_empty() {
        return None;
    }
    let sel = current.unwrap_or(0);
    Some(Dropdown {
        row,
        items,
        sel,
        top: 0,
        current,
    })
}

/// Do an entry of a drop-down.
pub(crate) fn dropdown_apply(app: &mut App, action: &str) {
    let (verb, arg) = action.split_once(' ').unwrap_or((action, ""));
    if app.metar_locked() && matches!(verb, "wx" | "cloud" | "precip") {
        app.service_msg = Some((
            "The weather cannot be changed while the METAR sync is on".into(),
            3.0,
        ));
        return;
    }
    match verb {
        "wx" => {
            if app
                .net
                .lan
                .as_ref()
                .is_some_and(|l| l.role == omsi_net::Role::Client)
            {
                app.service_msg = Some(("In a LAN session the host sets the weather".into(), 3.0));
            } else {
                app.change_weather(Some(arg.to_string()), true, 1.0);
            }
        }
        "metar_src" => {
            let code: String = arg
                .trim()
                .chars()
                .filter(|c| c.is_ascii_alphabetic())
                .take(4)
                .collect::<String>()
                .to_ascii_uppercase();
            app.settings.metar_station = code.clone();
            remember_setting("metar_station", &code);
            app.session.metar_rx = None;
            app.session.metar_once = false;
            // With sync on, the new station is fetched at once. With it off this simply
            // selects the station for "Load current METAR once".
            app.session.metar_next = 0.0;
        }
        "cloud" => {
            if let Some(i) = arg
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|i| *i < CLOUD_KINDS.len())
            {
                app.edit_weather(|w| {
                    w.clouds.0 = CLOUD_KINDS[i].id.to_string();
                    // (the kinds are points on the cover's scale: picking one sets the cover)
                    w.cloud_cover = Some(CLOUD_KINDS[i].cover);
                    if i == 0 {
                        w.clouds.1 = 0.0;
                    }
                });
            }
        }
        "precip" => {
            if let Some(i) = arg
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|i| *i < PRECIP_KINDS.len())
            {
                set_precip(app, i);
            }
        }
        "pick" => {
            if let Some((key, value)) = arg.split_once(' ') {
                let before = app.settings.mirror_hud;
                if is_graphics_setting(key) {
                    begin_graphics_change(app);
                }
                remember_setting(key, value);
                reload_settings(app);
                if key == "light_shadows"
                    && value == "all"
                    && app.renderer.as_ref().is_some_and(|r| !r.lamp_cube_maps())
                {
                    app.service_msg = Some((
                        omsi_ui::tr("Takes effect when the game starts the next time").into_owned(),
                        5.0,
                    ));
                }
                if key == "mirror_hud" && app.settings.mirror_hud != before {
                    if let Some(p) = app.player.as_ref() {
                        app.gfx.mirror_hud.apply_setting(p, app.settings.mirror_hud);
                    }
                }
                // (another source: the tracker starts anew with it, a few seconds on - the
                // old one's UDP port is let go first)
                if key == "head_tracking_source" {
                    app.input.headtrack = None;
                    app.input.headtrack_failed = Some(std::time::Instant::now());
                }
            }
        }
        "preset" => {
            if let Some(i) = arg.trim().parse::<usize>().ok().filter(|i| *i < 6) {
                begin_graphics_change(app);
                store_with(app, |v| {
                    // (a preset sets the mode as well)
                    let Some(p) = presets(v).into_iter().nth(i) else {
                        return;
                    };
                    if let Some(o) = p.1.as_object() {
                        for (k, x) in o {
                            v[k.as_str()] = x.clone();
                        }
                    }
                });
                app.service_msg = Some((
                    omsi_ui::tr("Graphics preset selected. Confirm to apply.").into_owned(),
                    5.0,
                ));
            }
        }
        "gfxprofile" => {
            let name = arg.trim();
            match omsi_launcher_lib::graphics_profiles().get(name) {
                Some(p) => {
                    begin_graphics_change(app);
                    store_with(app, |v| omsi_launcher_lib::apply_graphics_profile(p, v));
                    sync_live(app);
                    app.menus.list_dirty = true;
                    app.service_msg = Some((
                        format!(
                            "Graphics profile \"{name}\" loaded: graphics settings apply when the game starts the next time"
                        ),
                        5.0,
                    ));
                }
                None => {
                    app.service_msg = Some((format!("Graphics profile \"{name}\" not found"), 4.0))
                }
            }
        }
        "reset_all" => store_with(app, |v| {
            let language = v.get("language").cloned();
            *v = omsi_launcher_lib::settings_from_text(None);
            if let Some(l) = language {
                v["language"] = l;
            }
        }),
        _ => {}
    }
}

/// The session's weather as the player reads it (its name in the interface's language).
pub(super) fn weather_shown(app: &App) -> String {
    if crate::weather_tomorrow::selected(app.args.weather.as_deref()) {
        return "Tomorrow.io".into();
    }
    match app.args.weather.as_deref() {
        Some(p) => {
            let p = p.replace('\\', "/");
            match omsi_launcher_lib::content_path(&p) {
                Ok(f) => omsi_ui::tr(&omsi_launcher_lib::weather_name(&f)).into_owned(),
                Err(_) => weather_name(app),
            }
        }
        None => weather_name(app),
    }
}

pub(super) fn weather_name(app: &App) -> String {
    if app
        .session
        .weather
        .as_ref()
        .is_some_and(|w| w.name == CUSTOM_WEATHER)
    {
        return CUSTOM_WEATHER.to_string();
    }
    // a METAR report's weather
    if app
        .args
        .weather
        .as_deref()
        .is_some_and(|p| p.starts_with(crate::weather_setup::REPORT))
    {
        if let Some(n) = app
            .session
            .weather
            .as_ref()
            .map(|w| w.name.trim().to_string())
            .filter(|n| !n.is_empty())
        {
            return n;
        }
    }
    match app.args.weather.as_deref() {
        Some(p) => {
            let p = p.replace('\\', "/");
            let file = p.rsplit('/').next().unwrap_or("");
            let stem = file.rsplit_once('.').map(|x| x.0).unwrap_or(file);
            stem.trim_start_matches('#').to_string()
        }
        None => app
            .session
            .weather
            .as_ref()
            .map(|w| w.name.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Map default".to_string()),
    }
}

/// The launcher's settings file as the lists show it: read once (until something is
/// written), with the keys still waiting to be written on top.
pub(super) fn settings_file() -> serde_json::Value {
    let mut v = SETTINGS_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(|| {
            let text =
                std::fs::read_to_string(omsi_launcher_lib::data_dir().join("settings.cfg")).ok();
            omsi_launcher_lib::settings_from_text(text.as_deref())
        })
        .clone();
    let pending = PENDING_SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
    apply_pending(&mut v, &pending.0);
    if let Some(draft) = GRAPHICS_DRAFT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        graphics::overlay(&mut v, draft);
    }
    v
}

pub(super) fn value_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(x) => x.clone(),
        serde_json::Value::Bool(b) => (*b as u8).to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

pub(super) fn same_value(a: &str, b: &str) -> bool {
    a == b
        || a.parse::<f64>()
            .ok()
            .zip(b.parse::<f64>().ok())
            .is_some_and(|(x, y)| (x - y).abs() < 1e-6)
}

pub(super) fn select_options(key: &str) -> Vec<(&'static str, &'static str)> {
    match key {
        "ui_theme_pack" => crate::themes::choices()
            .iter()
            .map(|(id, name)| {
                (
                    if id.is_empty() {
                        crate::themes::OWN
                    } else {
                        id.as_str()
                    },
                    name.as_str(),
                )
            })
            .collect(),
        _ => omsi_launcher_lib::game_setting_options(key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn interface_theme_dropdown_uses_the_shared_dark_and_light_values() {
        assert_eq!(
            select_options("ui_theme"),
            [("dark", "Dark"), ("light", "Light theme")]
        );
    }

    #[test]
    fn selecting_custom_controls_releases_the_arrow_driving_preset() {
        use winit::keyboard::KeyCode;
        let args = crate::cli::Args::parse_from(["openomsi", "--drive-keys", "arrows"]);
        let settings = crate::settings::Settings {
            drive_keys: "arrows".into(),
            ..Default::default()
        };
        let mut app = crate::assemble_app(args, settings);
        app.input.keys.insert(KeyCode::ArrowLeft);
        assert!(crate::player::fallback_action(KeyCode::ArrowLeft, &app.args.drive_keys).is_some());

        app.settings.drive_keys = "omsi".into();
        sync_driving_keys(&mut app, "arrows");
        assert_eq!(app.args.drive_keys, "omsi");
        assert!(app.input.keys.is_empty());
        for (key, scan, action) in [
            (KeyCode::ArrowLeft, 203, "view_interiorcam_minus"),
            (KeyCode::ArrowRight, 205, "view_interiorcam_plus"),
        ] {
            assert_eq!(
                crate::player::fallback_action(key, &app.args.drive_keys),
                None
            );
            let binding = omsi_content::KeyBinding {
                action: action.into(),
                scan_code: scan,
                modifier: 0,
            };
            assert!(
                crate::input_script::binding_is_key(&binding, key, scan, 0),
                "no Alt needed"
            );
        }

        app.settings.drive_keys = "arrows".into();
        sync_driving_keys(&mut app, "omsi");
        assert!(
            crate::player::fallback_action(KeyCode::ArrowRight, &app.args.drive_keys).is_some()
        );
    }

    #[test]
    fn reloading_other_settings_keeps_the_command_line_driving_layout() {
        use winit::keyboard::KeyCode;
        let args = crate::cli::Args::parse_from(["openomsi", "--drive-keys", "arrows"]);
        let mut app = crate::assemble_app(args, crate::settings::Settings::default());
        app.input.keys.insert(KeyCode::ArrowRight);

        sync_driving_keys(&mut app, "simple");
        assert_eq!(app.args.drive_keys, "arrows");
        assert!(app.input.keys.contains(&KeyCode::ArrowRight));
    }

    #[test]
    fn live_interface_theme_updates_the_navigator_map() {
        let args = crate::cli::Args::parse_from(["openomsi"]);
        let mut app = crate::assemble_app(args, crate::settings::Settings::default());
        app.menus.navigator = Some(omsi_navigator::Navigator::new(true, 0.85, "bottom-left"));

        app.settings.ui_theme = "light".into();
        sync_live(&mut app);
        assert!(app.menus.navigator.as_ref().is_some_and(|n| n.light));

        app.settings.ui_theme = "dark".into();
        sync_live(&mut app);
        assert!(app.menus.navigator.as_ref().is_some_and(|n| !n.light));
    }
}

pub(super) fn select_state(
    file: &serde_json::Value,
    key: &str,
) -> (Vec<(&'static str, &'static str)>, Option<usize>, String) {
    let options = select_options(key);
    let cur = value_text(file.get(key).unwrap_or(&serde_json::Value::Null));
    let at = options.iter().position(|o| same_value(o.0, &cur));
    (options, at, cur)
}

pub(super) fn select_row(
    file: &serde_json::Value,
    key: &str,
    name: &str,
    desc: &str,
) -> Option<(String, String)> {
    let (options, at, cur) = select_state(file, key);
    if options.is_empty() {
        return None;
    }
    // (DLSS or FSR 3 whose DLLs are gone, MetalFX on another machine: the standard upscaler)
    let at = at.or((key == "upscaler").then_some(0));
    let label = at
        .map(|i| omsi_ui::tr(options[i].1).into_owned())
        .unwrap_or(cur);
    Some((row(name, 'o', &label, desc, None), format!("sel {key}")))
}

/// The quality presets (`graphics_presets`, as the launcher's): Low to Ultra, each with its
/// graphics mode.
pub(super) fn presets(_file: &serde_json::Value) -> [(&'static str, serde_json::Value); 6] {
    omsi_launcher_lib::graphics_presets()
}

pub(super) fn preset_now(file: &serde_json::Value) -> Option<usize> {
    presets(file).iter().position(|p| {
        p.1.as_object().is_some_and(|o| {
            o.iter().all(|(k, v)| {
                same_value(
                    &value_text(v),
                    &value_text(file.get(k).unwrap_or(&serde_json::Value::Null)),
                )
            })
        })
    })
}

pub(super) fn preset_row(
    file: &serde_json::Value,
    name: &str,
    desc: &str,
) -> Option<(String, String)> {
    let label = omsi_ui::tr(
        preset_now(file)
            .map(|i| presets(file)[i].0)
            .unwrap_or("Custom"),
    )
    .into_owned();
    Some((row(name, 'o', &label, desc, None), "preset".to_string()))
}

pub(crate) fn saved_settings() -> Option<serde_json::Value> {
    flush_settings(true);
    omsi_launcher_lib::get_settings().ok()
}

pub(crate) fn store_with(app: &mut App, change: impl FnOnce(&mut serde_json::Value)) {
    flush_settings(true);
    let Ok(mut v) = omsi_launcher_lib::get_settings() else {
        return;
    };
    let before = v.clone();
    if let Some(draft) = GRAPHICS_DRAFT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        graphics::overlay(&mut v, draft);
    }
    change(&mut v);
    {
        let mut draft = GRAPHICS_DRAFT.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(draft) = draft.as_mut() {
            // Only graphics enter the transaction; unrelated settings still save normally.
            for (key, value) in v.as_object().into_iter().flatten() {
                if is_graphics_setting(key) && before.get(key) != Some(value) {
                    draft[key] = value.clone();
                }
            }
            graphics::overlay(&mut v, &before);
        }
    }
    *SETTINGS_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    match omsi_launcher_lib::save_settings(&v) {
        Ok(()) => reload_settings(app),
        Err(e) => log::warn!("settings not saved: {e:#}"),
    }
}

pub(super) fn reload_settings(app: &mut App) {
    flush_settings(true);
    // (the seat is the bus's, see `bus_seats`: it is read again for the bus)
    let previous_drive_keys = app.settings.drive_keys.clone();
    app.settings = settings_in_mode(session_mode(app));
    sync_driving_keys(app, &previous_drive_keys);
    app.session.seat_bus.clear();
    crate::ui_language(&app.settings.language);
    crate::units::Units::from_setting(&app.settings.units).set();
    sync_live(app);
}

/// A layout picked during the drive must reach the key handlers too. Keeping the old
/// `args.drive_keys` left plain arrows steering after selecting Custom controls; Alt
/// happened to bypass that preset and let the interior-camera bindings through.
fn sync_driving_keys(app: &mut App, previous: &str) {
    if app.settings.drive_keys == previous {
        return;
    }
    // The key-up must not be interpreted through a different layout: release the old
    // vehicle actions first, then wait for a fresh press with the new layout.
    app.release_vehicle_keys();
    app.input.keys.clear();
    app.args.drive_keys.clone_from(&app.settings.drive_keys);
}

pub(crate) fn session_mode(app: &App) -> &'static str {
    mode_of_session(&app.args.session_settings, app.settings.mode())
}

pub(super) fn mode_of_session(
    session: &crate::startup::SessionSettings,
    graphics: &str,
) -> &'static str {
    let traced = crate::settings::graphics_mode(graphics) == "enhanced_plus";
    if session.enhanced_plus || (session.enhanced && traced) {
        "enhanced_plus"
    } else if session.enhanced {
        "enhanced"
    } else if session.classic {
        "vanilla"
    } else {
        "vanilla_plus"
    }
}

fn settings_in_mode(mode: &str) -> crate::settings::Settings {
    let file = settings_file();
    let text = omsi_launcher_lib::settings_to_text(&file, None);
    let mut s = text_in_mode(&text, mode);
    if let Some(n) = omsi_cfg::flags::OMSI_SAFE_GPU
        .parse::<u32>()
        .filter(|n| *n > 0)
    {
        s.apply_safe_gpu(n);
    }
    s
}

pub(super) fn text_in_mode(text: &str, mode: &str) -> crate::settings::Settings {
    // (the mode as the file spells it: Enhanced and ray tracing apart)
    let (graphics, rt) = match mode {
        "enhanced_plus" => ("enhanced", "1"),
        "enhanced" => ("enhanced", "0"),
        other => (other, "0"),
    };
    crate::settings::Settings::from_text(&format!(
        "{text}\ngraphics={graphics}\nray_tracing={rt}\n"
    ))
}

pub(super) fn sync_live(app: &mut App) {
    // (the upscaler and its sharpening at once, FSR 1 built when first asked for; and the
    // render scale with them, FSR's quality being one. Only when one of them changed and FSR
    // was or is asked for: without it the render scale stays for the next start, as its row
    // says)
    let o = app.settings.render_options();
    if let Some(r) = app.renderer.as_mut() {
        let asked = r.upscaler_asked();
        if (o.fsr1 || asked.0)
            && (o.fsr1, o.fsr_sharpness, o.render_scale) != asked
            && !r.set_upscaler(o.fsr1, o.fsr_sharpness, o.render_scale)
        {
            app.service_msg = Some((
                omsi_ui::tr(
                    "FSR 1 does not run on this graphics driver: the standard upscaler stays",
                )
                .into_owned(),
                6.0,
            ));
        }
    }
    // (the street light takes effect at once: the renderer reads it every frame)
    let live = app.settings.render_options();
    if let Some(r) = app.renderer.as_mut() {
        r.options.street_light = live.street_light;
        // (so do the lights' shadows, the clouds', the glow and the fog: read every frame)
        r.options.light_shadows = live.light_shadows;
        r.options.cloud_shadows = live.cloud_shadows;
        r.options.light_glow = live.light_glow;
        r.options.volumetric_fog = live.volumetric_fog;
        // (and the traced global illumination, while the ray tracing runs)
        r.options.rt_gi = live.rt_gi;
    }
    let s = &app.settings;
    if let Some(w) = app.world.as_ref() {
        w.settings.set_sound_scenery(s.vol_scenery);
        // The world reads these atomically during drawing, no map or bus reload required.
        w.settings.set_mirror_size(s.mirror_size);
        w.settings
            .set_rain_quality(crate::rain::quality_of(&s.rain_quality));
    }
    if let Some(a) = app.sound.audio.as_ref() {
        a.set_doppler(s.doppler);
    }
    if let Some(n) = app.menus.navigator.as_mut() {
        n.arrows = s.nav_dynamic_arrows;
        n.light = s.is_light_theme();
    }
    if let Some(h) = app.session.humans.as_mut() {
        h.exact_fare = s.exact_fare;
        h.transfer_percent = if app
            .args
            .passenger_connections_allowed(app.net.lan.is_some())
            && s.passenger_connections
        {
            s.passenger_transfer_percent
        } else {
            0.0
        };
        h.visual_hailing = s.passenger_hailing && s.passenger_animation == "enhanced";
        h.visual_full_bus_reaction =
            s.passenger_full_bus_reaction && s.passenger_animation == "enhanced";
        h.bad_driving = s.pax_bad_driving;
        h.boarding = s.boarding.clone();
        h.rear_door_boarding = s.rear_door_boarding;
        h.stand_chance = s.standing_chance;
        h.short_rides_stand = s.pax_short_rides_stand;
        h.voices = match s.pax_voices.as_str() {
            "off" => 2,
            "tickets" => 1,
            _ => 0,
        };
    }
}
