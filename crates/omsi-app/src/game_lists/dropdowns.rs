//! The dropdowns of the settings windows, the settings file they read and write.

use super::*;

/// The name of the weather in force (the file's name without its ending).
/// The weather files (`Weather/*.owt`) of every content root, by name.
pub(super) fn weather_files() -> Vec<String> {
    let mut files: Vec<String> = omsi_cfg::read_dir_merged("Weather")
        .into_iter()
        .filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("owt")).unwrap_or(false))
        .filter_map(|p| p.file_name().map(|n| format!("Weather/{}", n.to_string_lossy())))
        .collect();
    files.sort_by(|a, b| bus_cmp(a.trim_start_matches("Weather/").trim_start_matches('#'), b.trim_start_matches("Weather/").trim_start_matches('#')));
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
                    let stem = f.rsplit('/').next().unwrap_or(&f).rsplit_once('.').map(|x| x.0).unwrap_or(&f).trim_start_matches('#').to_string();
                    if stem.eq_ignore_ascii_case(&now) {
                        current = Some(i);
                    }
                    // (shown by its name in the interface's language, not its German file name)
                    let name = if f == "tomorrow" { "Tomorrow.io".into() }
                        else { omsi_launcher_lib::content_path(&f).map(|p| omsi_launcher_lib::weather_name(&p)).unwrap_or(stem) };
                    (omsi_ui::tr(&name).into_owned(), format!("wx {f}"))
                })
                .collect()
        }
        "metar_src" => {
            let mut v = vec![(tr("Automatic (nearest the map)"), "metar_src ".to_string())];
            let own = &app.settings.metar_station;
            current = Some(0);
            for (i, (code, label)) in crate::weather_setup::metar_airports(&app.args.root).into_iter().enumerate() {
                if code.eq_ignore_ascii_case(own) {
                    current = Some(i + 1);
                }
                v.push((label, format!("metar_src {code}")));
            }
            v
        }
        "cloudkind" => {
            current = app.session.weather.as_ref().and_then(|w| cloud_index(&w.clouds.0));
            CLOUD_KINDS.iter().enumerate().map(|(i, k)| (tr(k.label), format!("cloud {i}"))).collect()
        }
        "precipkind" => {
            current = app.session.weather.as_ref().map(|w| (w.precip.first().copied().unwrap_or(0.0).max(0.0) as usize).min(PRECIP_KINDS.len() - 1));
            PRECIP_KINDS.iter().enumerate().map(|(i, n)| (tr(*n), format!("precip {i}"))).collect()
        }
        "preset" => {
            let file = settings_file();
            current = preset_now(&file);
            presets(&file).iter().enumerate().map(|(i, p)| (tr(p.0), format!("preset {i}"))).collect()
        }
        "gfxprofile" => omsi_launcher_lib::graphics_profiles().into_keys().map(|n| (n.clone(), format!("gfxprofile {n}"))).collect(),
        "reset" => vec![(tr("Cancel"), "noop".to_string()), (tr("Reset all settings"), "reset_all".to_string())],
        key if key.starts_with("sel ") => {
            let key = &key[4..];
            let (options, at, _) = select_state(&settings_file(), key);
            current = at;
            options.iter().map(|o| (tr(o.1), format!("pick {key} {}", o.0))).collect()
        }
        _ => return None,
    };
    if items.is_empty() {
        return None;
    }
    let sel = current.unwrap_or(0);
    Some(Dropdown { row, items, sel, top: 0, current })
}

/// Do an entry of a drop-down.
pub(crate) fn dropdown_apply(app: &mut App, action: &str) {
    let (verb, arg) = action.split_once(' ').unwrap_or((action, ""));
    if app.metar_locked() && matches!(verb, "wx" | "cloud" | "precip") {
        app.service_msg = Some(("The weather cannot be changed while the METAR sync is on".into(), 3.0));
        return;
    }
    match verb {
        "wx" => {
            if app.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client) {
                app.service_msg = Some(("In a LAN session the host sets the weather".into(), 3.0));
            } else {
                app.change_weather(Some(arg.to_string()), true, 1.0);
            }
        }
        "metar_src" => {
            let code: String = arg.trim().chars().filter(|c| c.is_ascii_alphabetic()).take(4).collect::<String>().to_ascii_uppercase();
            app.settings.metar_station = code.clone();
            remember_setting("metar_station", &code);
            app.session.metar_rx = None;
            app.session.metar_once = false;
            // With sync on, the new station is fetched at once. With it off this simply
            // selects the station for "Load current METAR once".
            app.session.metar_next = 0.0;
        }
        "cloud" => {
            if let Some(i) = arg.trim().parse::<usize>().ok().filter(|i| *i < CLOUD_KINDS.len()) {
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
            if let Some(i) = arg.trim().parse::<usize>().ok().filter(|i| *i < PRECIP_KINDS.len()) {
                set_precip(app, i);
            }
        }
        "pick" => {
            if let Some((key, value)) = arg.split_once(' ') {
                let before = app.settings.mirror_hud;
                remember_setting(key, value);
                reload_settings(app);
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
            if let Some(i) = arg.trim().parse::<usize>().ok().filter(|i| *i < 4) {
                store_with(app, |v| {
                    // (the preset of the mode the file has: it keeps the mode)
                    let Some(p) = presets(v).into_iter().nth(i) else { return };
                    if let Some(o) = p.1.as_object() {
                        for (k, x) in o {
                            v[k.as_str()] = x.clone();
                        }
                    }
                });
                app.service_msg = Some((omsi_ui::tr("Takes effect when the game starts the next time").into_owned(), 5.0));
            }
        }
        "gfxprofile" => {
            let name = arg.trim();
            match omsi_launcher_lib::graphics_profiles().get(name) {
                Some(p) => {
                    store_with(app, |v| omsi_launcher_lib::apply_graphics_profile(p, v));
                    sync_live(app);
                    app.menus.list_dirty = true;
                    app.service_msg = Some((format!("Graphics profile \"{name}\" loaded: graphics settings apply when the game starts the next time"), 5.0));
                }
                None => app.service_msg = Some((format!("Graphics profile \"{name}\" not found"), 4.0)),
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
    if crate::weather_tomorrow::selected(app.args.weather.as_deref()) { return "Tomorrow.io".into(); }
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
    if app.session.weather.as_ref().is_some_and(|w| w.name == CUSTOM_WEATHER) {
        return CUSTOM_WEATHER.to_string();
    }
    // a METAR report's weather
    if app.args.weather.as_deref().is_some_and(|p| p.starts_with(crate::weather_setup::REPORT)) {
        if let Some(n) = app.session.weather.as_ref().map(|w| w.name.trim().to_string()).filter(|n| !n.is_empty()) {
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
        None => app.session.weather.as_ref().map(|w| w.name.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Map default".to_string()),
    }
}

/// The launcher's settings file as the lists show it: read once (until something is
/// written), with the keys still waiting to be written on top.
pub(super) fn settings_file() -> serde_json::Value {
    let mut v = SETTINGS_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(|| {
            let text = std::fs::read_to_string(omsi_launcher_lib::data_dir().join("settings.cfg")).ok();
            omsi_launcher_lib::settings_from_text(text.as_deref())
        })
        .clone();
    let pending = PENDING_SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
    apply_pending(&mut v, &pending.0);
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
    a == b || a.parse::<f64>().ok().zip(b.parse::<f64>().ok()).is_some_and(|(x, y)| (x - y).abs() < 1e-6)
}

pub(super) fn select_options(key: &str) -> Vec<(&'static str, &'static str)> {
    match key {
        "cloud_quality" => vec![("high", "High"), ("low", "Low")],
        "street_light" => vec![("auto", "Auto"), ("on", "On"), ("off", "Off")],
        "rain_quality" => vec![("high", "High"), ("medium", "Medium"), ("low", "Low")],
        "vehicle_smoke" => vec![("all", "All vehicles"), ("player", "Player only"), ("off", "Off")],
        "puddles" => vec![("high", "High"), ("low", "Low"), ("off", "Off")],
        "graphics" => vec![("vanilla", "Vanilla (as OMSI 2)"), ("vanilla_plus", "Vanilla+"), ("enhanced", "Enhanced"), ("enhanced_plus", "Enhanced+")],
        "msaa" => vec![("1", "Off"), ("2", "2x MSAA"), ("4", "4x MSAA"), ("8", "8x MSAA")],
        "render_scale" => vec![("auto", "Auto"), ("1", "100%"), ("0.85", "85%"), ("0.75", "75%"), ("0.67", "67%"), ("0.5", "50%")],
        "upscaler" => vec![("default", "Standard (Catmull-Rom)"), ("fsr1", "AMD FSR 1 (experimental)")],
        "fsr_quality" => vec![("ultra", "Ultra Quality (77%)"), ("quality", "Quality (67%)"), ("balanced", "Balanced (59%)"), ("performance", "Performance (50%)"), ("auto", "Automatic (keeps the frame rate)"), ("custom", "Use render scale")],
        "anisotropy" => vec![("1", "Off"), ("2", "2x"), ("4", "4x"), ("8", "8x"), ("16", "16x")],
        "shadow_size" => vec![("1024", "1024"), ("2048", "2048"), ("4096", "4096")],
        "shadow_casters" => vec![("all", "Every solid mesh"), ("omsi", "[shadow] meshes, as OMSI")],
        "max_fps" => vec![("0", "Screen refresh rate"), ("30", "30 fps"), ("45", "45 fps"), ("60", "60 fps"), ("120", "120 fps"), ("144", "144 fps"), ("1000", "Unlimited")],
        "view_distance" => vec![("auto", "Default (1200 m)"), ("600", "600 m - fastest"), ("900", "900 m"), ("1200", "1200 m"), ("1500", "1500 m"), ("2000", "2000 m"), ("2500", "2500 m")],
        "max_obj_dist" => vec![("auto", "Automatic"), ("500", "500 m"), ("750", "750 m"), ("900", "900 m"), ("1500", "1500 m"), ("3000", "3000 m")],
        "min_obj_size" => vec![("0.005", "All"), ("0.013", "Normal"), ("0.02", "Fewer (faster)"), ("0.03", "Few (fastest)")],
        "mirror_size" => vec![("0", "Off"), ("128", "Low (128)"), ("256", "Normal (256)"), ("512", "High (512)"), ("1024", "Very high (1024)")],
        "mirror_refresh" => vec![("off", "None (frozen picture)"), ("eco", "Economical"), ("full", "Full"), ("every", "Every frame (smoothest)")],
        "mirror_hud" => vec![("0", "Off"), ("2", "Left mirror"), ("1", "Right mirror"), ("3", "Left and right"), ("7", "Left, right and front")],
        "mirror_quality" => vec![("standard", "Standard"), ("enhanced", "As the picture (Enhanced)")],
        "video_memory" => vec![("0", "Automatic"), ("1000", "1 GB"), ("1500", "1.5 GB"), ("2000", "2 GB"), ("3000", "3 GB"), ("4000", "4 GB"), ("6000", "6 GB"), ("8000", "8 GB"), ("12000", "12 GB"), ("16000", "16 GB")],
        "drive_keys" => vec![("omsi", "Custom controls (Controls page)"), ("simple", "W A S D + arrows"), ("wasd", "W A S D only"), ("arrows", "Arrow keys only")],
        "resolution" => vec![("auto", "Automatic"), ("1280x720", "1280 x 720"), ("1280x800", "1280 x 800 (Steam Deck)"), ("1366x768", "1366 x 768"), ("1600x900", "1600 x 900"), ("1920x1080", "1920 x 1080"), ("1920x1200", "1920 x 1200"), ("2560x1440", "2560 x 1440"), ("3840x2160", "3840 x 2160")],
        "navigator_corner" => vec![("top-left", "Top left"), ("top-right", "Top right"), ("bottom-left", "Bottom left"), ("bottom-right", "Bottom right")],
        "boarding" => vec![("auto", "Pay and take the ticket"), ("pay", "The driver sells the ticket"), ("walk", "Just walk in")],
        "standing_chance" => vec![("0", "Never"), ("5", "5%"), ("10", "10%"), ("25", "25%"), ("50", "50%"), ("100", "Always")],
        "units" => vec![("metric", "Metric (km/h, km, °C)"), ("uk", "British (mph, miles, yards, °C)"), ("us", "American (mph, miles, feet, °F)")],
        "head_tracking_source" => vec![("auto", "Automatic"), ("tobii", "Tobii eye tracker"), ("opentrack", "opentrack (UDP 4242)")],
        "pax_voices" => vec![("all", "Greetings and tickets"), ("tickets", "Only the ticket asked for"), ("off", "Silent")],
        "perf_overlay" => vec![("off", "Off"), ("compact", "Compact"), ("detailed", "Detailed")],
        "maintenance" => vec![("0", "Infinite (no wear)"), ("1", "Very bad"), ("2", "Bad"), ("3", "Normal"), ("4", "Good")],
        "ai_unsched_factor" => vec![("25", "25%"), ("50", "50%"), ("75", "75%"), ("100", "100%"), ("150", "150%"), ("200", "200%")],
        "ai_max_scheduled" => vec![("0", "All"), ("10", "At most 10"), ("25", "At most 25"), ("50", "At most 50")],
        "ai_max_parked" => vec![("-1", "None"), ("0", "Every space"), ("35", "At most 35"), ("100", "At most 100"), ("250", "At most 250")],
        // (OMSI's AIMaxHumans, #2087)
        "ai_max_humans" => vec![("50", "50"), ("100", "100"), ("200", "200"), ("400", "400"), ("720", "720"), ("1000", "1000")],
        "language" => omsi_launcher_lib::LANGUAGES.iter().map(|l| (l.0, l.1)).collect(),
        "vr_scale" => vec![("0.5", "50%"), ("0.65", "65%"), ("0.8", "80%"), ("1", "100%")],
        "vr_head_smoothing_ms" => vec![("0", "Off"), ("5", "5 ms"), ("10", "10 ms"), ("20", "20 ms"), ("30", "30 ms")],
        "vr_mirror_rate" => vec![("0", "Off"), ("8", "8/s"), ("16", "16/s"), ("24", "24/s"), ("32", "32/s"), ("48", "48/s"), ("60", "60/s"), ("90", "90/s"), ("120", "120/s"), ("180", "180/s"), ("240", "240/s"), ("360", "360/s"), ("-1", "Every frame")],
        _ => Vec::new(),
    }
}

pub(super) fn select_state(file: &serde_json::Value, key: &str) -> (Vec<(&'static str, &'static str)>, Option<usize>, String) {
    let options = select_options(key);
    let cur = value_text(file.get(key).unwrap_or(&serde_json::Value::Null));
    let at = options.iter().position(|o| same_value(o.0, &cur));
    (options, at, cur)
}

pub(super) fn select_row(file: &serde_json::Value, key: &str, name: &str, desc: &str) -> Option<(String, String)> {
    let (options, at, cur) = select_state(file, key);
    if options.is_empty() {
        return None;
    }
    let label = at.map(|i| omsi_ui::tr(options[i].1).into_owned()).unwrap_or(cur);
    Some((row(name, 'o', &label, desc, None), format!("sel {key}")))
}

/// The quality presets for the graphics mode of `file` (`graphics_presets_for`, as the
/// launcher's): they keep the mode.
pub(super) fn presets(file: &serde_json::Value) -> [(&'static str, serde_json::Value); 4] {
    omsi_launcher_lib::graphics_presets_for(file.get("graphics").and_then(|v| v.as_str()).unwrap_or("vanilla_plus"))
}

pub(super) fn preset_now(file: &serde_json::Value) -> Option<usize> {
    presets(file).iter().position(|p| {
        p.1.as_object().is_some_and(|o| o.iter().all(|(k, v)| same_value(&value_text(v), &value_text(file.get(k).unwrap_or(&serde_json::Value::Null)))))
    })
}

pub(super) fn preset_row(file: &serde_json::Value, name: &str, desc: &str) -> Option<(String, String)> {
    let label = omsi_ui::tr(preset_now(file).map(|i| presets(file)[i].0).unwrap_or("Custom")).into_owned();
    Some((row(name, 'o', &label, desc, None), "preset".to_string()))
}

pub(crate) fn saved_settings() -> Option<serde_json::Value> {
    flush_settings(true);
    omsi_launcher_lib::get_settings().ok()
}

pub(crate) fn store_with(app: &mut App, change: impl FnOnce(&mut serde_json::Value)) {
    flush_settings(true);
    let Ok(mut v) = omsi_launcher_lib::get_settings() else { return };
    change(&mut v);
    *SETTINGS_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    match omsi_launcher_lib::save_settings(&v) {
        Ok(()) => reload_settings(app),
        Err(e) => log::warn!("settings not saved: {e:#}"),
    }
}

pub(super) fn reload_settings(app: &mut App) {
    flush_settings(true);
    // (the seat is the bus's, see `bus_seats`: it is read again for the bus)
    app.settings = settings_in_mode(session_mode(app));
    app.session.seat_bus.clear();
    crate::ui_language(&app.settings.language);
    crate::units::Units::from_setting(&app.settings.units).set();
    sync_live(app);
}

pub(crate) fn session_mode(app: &App) -> &'static str {
    mode_of_session(&app.args.session_settings, &app.settings.graphics)
}

pub(super) fn mode_of_session(session: &crate::startup::SessionSettings, graphics: &str) -> &'static str {
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
    let s = crate::settings::Settings::load();
    if s.graphics == mode {
        return s;
    }
    let text = crate::settings::Settings::path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let mut s = text_in_mode(&text, mode);
    if let Some(n) = omsi_cfg::flags::OMSI_SAFE_GPU.parse::<u32>().filter(|n| *n > 0) {
        s.apply_safe_gpu(n);
    }
    s
}

pub(super) fn text_in_mode(text: &str, mode: &str) -> crate::settings::Settings {
    crate::settings::Settings::from_text(&format!("{text}\ngraphics={mode}\n"))
}

pub(super) fn sync_live(app: &mut App) {
    // (the upscaler and its sharpening at once, FSR 1 built when first asked for; and the
    // render scale with them, FSR's quality being one. Only when one of them changed and FSR
    // was or is asked for: without it the render scale stays for the next start, as its row
    // says)
    let o = app.settings.render_options();
    if let Some(r) = app.renderer.as_mut() {
        let asked = r.upscaler_asked();
        if (o.fsr1 || asked.0) && (o.fsr1, o.fsr_sharpness, o.render_scale) != asked && !r.set_upscaler(o.fsr1, o.fsr_sharpness, o.render_scale) {
            app.service_msg = Some((omsi_ui::tr("FSR 1 does not run on this graphics driver: the standard upscaler stays").into_owned(), 6.0));
        }
    }
    // (the street light takes effect at once: the renderer reads it every frame)
    let street_light = app.settings.render_options().street_light;
    if let Some(r) = app.renderer.as_mut() {
        r.options.street_light = street_light;
    }
    let s = &app.settings;
    if let Some(w) = app.world.as_ref() {
        w.settings.set_sound_scenery(s.vol_scenery);
    }
    if let Some(a) = app.sound.audio.as_ref() {
        a.set_doppler(s.doppler);
    }
    if let Some(n) = app.menus.navigator.as_mut() {
        n.arrows = s.nav_arrows;
    }
    if let Some(h) = app.session.humans.as_mut() {
        h.exact_fare = s.exact_fare;
        h.bad_driving = s.pax_bad_driving;
        h.boarding = s.boarding.clone();
        h.stand_chance = s.standing_chance;
        h.voices = match s.pax_voices.as_str() {
            "off" => 2,
            "tickets" => 1,
            _ => 0,
        };
    }
}
