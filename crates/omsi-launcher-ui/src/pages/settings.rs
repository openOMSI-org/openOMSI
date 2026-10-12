//! The Settings page and its tabs.

use crate::host::host;
use super::*;
use omsi_launcher_lib as core;

// --- settings ---------------------------------------------------------------------------------

/// A value of the settings as the pages show it.
fn get<'a>(v: &'a Value, k: &str) -> &'a Value {
    v.get(k).unwrap_or(&Value::Null)
}

/// The window sizes the settings offer (`resolution`).
pub const RESOLUTIONS: &[(&str, &str)] = &[("auto", "Automatic"), ("1280x720", "1280 x 720"), ("1280x800", "1280 x 800 (Steam Deck)"), ("1366x768", "1366 x 768"), ("1600x900", "1600 x 900"), ("1920x1080", "1920 x 1080"), ("1920x1200", "1920 x 1200"), ("2560x1440", "2560 x 1440"), ("3840x2160", "3840 x 2160")];

fn sel_setting(ui: &mut Ui, s: &mut Value, dirty: &mut f32, name: &str, r: Rect, label: &str, key: &str, options: &[(&str, &str)]) {
    ui.label(Rect::new(r.x, r.y, r.w * 0.45, r.h), label);
    let cur = match get(s, key) {
        Value::String(x) => x.clone(),
        Value::Bool(b) => (*b as u8).to_string(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    };
    let mut labels: Vec<String> = options.iter().map(|o| o.1.to_string()).collect();
    let mut values: Vec<String> = options.iter().map(|o| o.0.to_string()).collect();
    let mut sel = values.iter().position(|v| *v == cur || v.parse::<f64>().ok().zip(cur.parse::<f64>().ok()).map(|(a, b)| (a - b).abs() < 1e-6).unwrap_or(false));
    if sel.is_none() && !cur.is_empty() {
        // a value written by hand gets an entry of its own
        labels.push(cur.clone());
        values.push(cur.clone());
        sel = Some(values.len() - 1);
    }
    let mut sel = sel.unwrap_or(0);
    if ui.select(name, Rect::new(r.x + r.w * 0.45, r.y, r.w * 0.55, r.h), &mut sel, &labels) {
        let v = &values[sel];
        s[key] = match get(s, key) {
            Value::Bool(_) => json!(v == "1"),
            Value::Number(_) => v.parse::<f64>().map(|f| if f.fract() == 0.0 { json!(f as i64) } else { json!(f) }).unwrap_or(json!(v)),
            _ => json!(v),
        };
        *dirty = 0.3;
    }
}

fn toggle_setting(ui: &mut Ui, s: &mut Value, dirty: &mut f32, r: Rect, label: &str, key: &str) {
    let mut v = get(s, key).as_bool().unwrap_or(false);
    if ui.toggle(&format!("set-{key}"), r, &mut v, label) {
        s[key] = json!(v);
        *dirty = 0.3;
    }
}

/// The settings page's tabs: what one has come to change.
pub const SETTINGS_TABS: [&str; 6] = ["Graphics", "Driving", "Camera", "Sound", "Gameplay", "General"];

/// What the tabs show of the launcher and ask of it (they see only the settings): the
/// updater's state, "Check now", "Reset all settings", the Controls page at one of its tabs.
struct Outside {
    update: omsi_platform::updater::Status,
    check_updates: bool,
    reset: bool,
    controls: Option<usize>,
}

thread_local! {
    /// How high each tab's two columns came out (the stacked layout's heights, and how far
    /// the page scrolls).
    static SETTINGS_COL_H: std::cell::Cell<[[f32; 2]; SETTINGS_TABS.len()]> = const { std::cell::Cell::new([[0.0; 2]; SETTINGS_TABS.len()]) };
}

pub fn settings(l: &mut Launcher, area: Rect) {
    let body = l.page_title(area, "Settings", "Every change is saved at once; the game reads them when it starts.");
    // (one tab at a time: the whole page in three columns was taller than two screens, and a
    // setting was found by reading all of it)
    let bar = Rect::new(body.x, body.y, body.w.min(660.0), 36.0);
    let mut tab = l.pages.settings_tab;
    if l.ui.segmented("settings-tab", bar, &mut tab, &SETTINGS_TABS) {
        l.pages.settings_tab = tab;
    }
    let body = Rect::new(body.x, bar.bottom() + 18.0, body.w, (body.bottom() - bar.bottom() - 18.0).max(0.0));
    let s = &mut l.state.settings;
    let dirty = &mut l.state.settings_dirty;
    let mut out = Outside { update: l.update.status(), check_updates: false, reset: false, controls: None };
    // (two columns side by side; where they would be too narrow to read - a phone - one
    // under the other, each as high as it was the frame before)
    let stacked = body.w < 900.0;
    // (each tab scrolled where it was left, not where another one was)
    l.ui.scroll_area(&format!("settings-page-{tab}"), body, &mut |ui, v| {
        let hs = SETTINGS_COL_H.with(|c| c.get())[tab];
        let w = v.w - 8.0;
        let (cols, h) = if stacked {
            ([Rect::new(v.x, v.y, w, hs[0]), Rect::new(v.x, v.y + hs[0] + GAP * 2.0, w, hs[1])], hs[0] + hs[1] + GAP * 2.0)
        } else {
            let cw = (w - GAP * 2.0) / 2.0;
            let h = v.h.max(hs[0]).max(hs[1]);
            ([Rect::new(v.x, v.y, cw, h), Rect::new(v.x + cw + GAP * 2.0, v.y, cw, h)], h)
        };
        let used = settings_tab(ui, tab, s, dirty, &mut out, cols);
        SETTINGS_COL_H.with(|c| {
            let mut all = c.get();
            all[tab] = used;
            c.set(all);
        });
        h
    });
    if out.check_updates {
        l.update.check();
    }
    if out.reset {
        l.pages.confirm_reset = true;
    }
    if let Some(t) = out.controls {
        l.pages.controls_tab = t;
        l.go(Page::Controls);
    }
}

/// The dialog that asks before every setting goes back to how it came.
pub fn reset_dialog(l: &mut Launcher) {
    let size = l.ui.size;
    let full = Rect::new(0.0, 0.0, size.x, size.y);
    l.ui.solid(full);
    l.ui.p().rect(full, omsi_ui::Color::rgba(0, 0, 0, 0.62));
    let w = (size.x - 48.0).min(520.0);
    let h = 190.0;
    let r = Rect::new((size.x - w) * 0.5, (size.y - h) * 0.5, w, h);
    l.ui.panel(r);
    let inner = Rect::new(r.x + 24.0, r.y + 20.0, r.w - 48.0, r.h - 40.0);
    l.ui.icon("restart_alt", Vec2::new(inner.x + 14.0, inner.y + 14.0), 26.0, DANGER);
    l.ui.text_in("Reset every setting?", Rect::new(inner.x + 38.0, inner.y, inner.w - 38.0, 28.0), 18.0, Weight::Bold, TEXT, Align::Left);
    l.ui.paragraph("Graphics, sound, controllers and game settings go back to how they came. The language, the drivers, the key bindings and the game folder stay.", Vec2::new(inner.x, inner.y + 40.0), inner.w, 13.0, Weight::Regular, TEXT_DIM);
    let by = inner.bottom() - 38.0;
    if l.ui.button("reset-no", Rect::new(inner.right() - 250.0, by, 110.0, 38.0), "Cancel", None, ButtonKind::Normal) {
        l.pages.confirm_reset = false;
    }
    if l.ui.button("reset-yes", Rect::new(inner.right() - 130.0, by, 130.0, 38.0), "Reset", Some("restart_alt"), ButtonKind::Danger) {
        let language = l.state.settings.get("language").cloned();
        l.state.settings = core::settings_from_text(None);
        if let Some(lang) = language {
            l.state.settings["language"] = lang;
        }
        l.state.settings_dirty = 0.3;
        l.pages.confirm_reset = false;
        l.state.set_status("Every setting is back to how it came.", false);
    }
}

/// A column of a settings tab: its panel, and the rows and sections laid down it.
struct Col {
    top: f32,
    inner: Rect,
    y: f32,
}

impl Col {
    /// The column in `r`, under the title of its first section.
    fn new(ui: &mut Ui, r: Rect, title: &str) -> Col {
        ui.panel(r);
        let inner = ui.heading(Rect::new(r.x + 18.0, r.y + 14.0, r.w - 36.0, r.h - 28.0), title, None);
        Col { top: r.y, inner, y: inner.y }
    }

    /// The next row.
    fn row(&mut self) -> Rect {
        let r = Rect::new(self.inner.x, self.y, self.inner.w, ROW - 2.0);
        self.y += ROW + 4.0;
        r
    }

    /// Another section, under its title.
    fn section(&mut self, ui: &mut Ui, title: &str) {
        self.y += 6.0;
        ui.heading(Rect::new(self.inner.x, self.y, self.inner.w, 28.0), title, None);
        self.y += 32.0;
    }

    /// How high the column came out.
    fn used(&self) -> f32 {
        self.y - self.top + 14.0
    }
}

/// Tab `tab` of the settings in its two columns. Returns the height each column needed (0
/// for one the tab leaves empty).
fn settings_tab(ui: &mut Ui, tab: usize, s: &mut Value, dirty: &mut f32, out: &mut Outside, cols: [Rect; 2]) -> [f32; 2] {
    match tab {
        0 => graphics_tab(ui, s, dirty, cols),
        1 => driving_tab(ui, s, dirty, out, cols),
        2 => camera_tab(ui, s, dirty, out, cols),
        3 => sound_tab(ui, s, dirty, cols),
        4 => gameplay_tab(ui, s, dirty, cols),
        _ => general_tab(ui, s, dirty, out, cols),
    }
}

/// The saved graphics profiles' part of the Graphics tab: the list, the name being typed.
#[derive(Default)]
struct GfxProfileUi {
    name: String,
    sel: usize,
    list: Option<Vec<String>>,
    msg: String,
}

thread_local! {
    static GFX_PROFILES: std::cell::RefCell<GfxProfileUi> = std::cell::RefCell::new(GfxProfileUi::default());
}

/// Save, load and delete the graphics settings as named profiles.
fn graphics_profiles_block(ui: &mut Ui, s: &mut Value, dirty: &mut f32, c: &mut Col) {
    GFX_PROFILES.with(|g| {
        let mut g = g.borrow_mut();
        let g = &mut *g;
        let names: Vec<String> = g.list.get_or_insert_with(|| core::graphics_profiles().into_keys().collect()).clone();
        g.sel = g.sel.min(names.len().saturating_sub(1));
        let labels: Vec<String> = if names.is_empty() { vec!["No saved profiles".to_string()] } else { names.clone() };
        let r = c.row();
        ui.label(Rect::new(r.x, r.y, r.w * 0.45, r.h), "Saved profile");
        if ui.select("s-gp-sel", Rect::new(r.x + r.w * 0.45, r.y, r.w * 0.55, r.h), &mut g.sel, &labels) && !names.is_empty() {
            g.name = names[g.sel].clone();
        }
        let r = c.row();
        let half = (r.w - GAP) * 0.5;
        if ui.button("s-gp-load", Rect::new(r.x, r.y, half, r.h), "Load", Some("download"), ButtonKind::Normal) && !names.is_empty() {
            let name = names[g.sel].clone();
            match core::graphics_profiles().get(&name) {
                Some(p) => {
                    core::apply_graphics_profile(p, s);
                    *dirty = 0.3;
                    g.msg = format!("Loaded \"{name}\".");
                }
                None => g.msg = format!("\"{name}\" is gone."),
            }
        }
        if ui.button("s-gp-del", Rect::new(r.x + half + GAP, r.y, half, r.h), "Delete", Some("delete"), ButtonKind::Danger) && !names.is_empty() {
            let name = names[g.sel].clone();
            g.msg = match core::delete_graphics_profile(&name) {
                Ok(()) => format!("Deleted \"{name}\"."),
                Err(e) => format!("{e:#}"),
            };
            g.list = None;
        }
        let r = c.row();
        ui.text_input("s-gp-name", r, &mut g.name, "Profile name", None);
        let r = c.row();
        if ui.button("s-gp-save", r, "Save current graphics as profile", Some("save"), ButtonKind::Primary) {
            g.msg = match core::save_graphics_profile(&g.name, s) {
                Ok(name) => {
                    g.name = name.clone();
                    g.list = None;
                    if let Some(i) = core::graphics_profiles().keys().position(|k| *k == name) {
                        g.sel = i;
                    }
                    format!("Saved \"{name}\".")
                }
                Err(e) => format!("{e:#}"),
            };
        }
        if !g.msg.is_empty() {
            c.y += ui.paragraph(&g.msg, Vec2::new(c.inner.x, c.y), c.inner.w, 12.5, Weight::Regular, TEXT_DIM) + 8.0;
        }
    });
}

/// How the game looks and how fast it runs.
fn graphics_tab(ui: &mut Ui, s: &mut Value, dirty: &mut f32, cols: [Rect; 2]) -> [f32; 2] {
    let mut c = Col::new(ui, cols[0], "Graphics");
    // The graphics mode first, then the quality presets for it: they set most of what
    // follows and keep the mode (`graphics_presets_for`). (OMSI's own option_presets/*.oop
    // are named after the PCs of their day - "PC 2006", "X10 high", "Chicago Recommended" -
    // which read as random words here.)
    sel_setting(ui, s, dirty, "s-graphics", c.row(), "Graphics", "graphics", &[("vanilla", "Vanilla (as OMSI 2)"), ("vanilla_plus", "Vanilla+"), ("enhanced", "Enhanced"), ("enhanced_plus", "Enhanced+")]);
    let presets = core::graphics_presets_for(get(s, "graphics").as_str().unwrap_or("vanilla_plus"));
    {
        // the preset the settings match now, else "Custom"
        let matches = |p: &serde_json::Value| p.as_object().map(|o| o.iter().all(|(k, v)| {
            let cur = get(s, k);
            cur == v || cur.as_f64().zip(v.as_f64()).map(|(a, b)| (a - b).abs() < 1e-6).unwrap_or(false) || cur.as_str().zip(v.as_f64()).map(|(a, b)| a.parse::<f64>().map(|a| (a - b).abs() < 1e-6).unwrap_or(false)).unwrap_or(false) || cur.as_f64().zip(v.as_str()).map(|(a, b)| b.parse::<f64>().map(|b| (a - b).abs() < 1e-6).unwrap_or(false)).unwrap_or(false)
        })).unwrap_or(false);
        let mut labels: Vec<String> = presets.iter().map(|p| p.0.to_string()).collect();
        labels.push("Custom".to_string());
        let mut sel = presets.iter().position(|p| matches(&p.1)).unwrap_or(presets.len());
        let r = c.row();
        ui.label(Rect::new(r.x, r.y, r.w * 0.45, r.h), "Quality preset");
        if ui.select("s-preset", Rect::new(r.x + r.w * 0.45, r.y, r.w * 0.55, r.h), &mut sel, &labels) && sel < presets.len() {
            if let Some(obj) = presets[sel].1.as_object() {
                for (k, v) in obj {
                    s[k.as_str()] = v.clone();
                }
                *dirty = 0.3;
            }
        }
    }
    // Vanilla draws what OMSI 2 draws: no sun shadows, ambient occlusion or detail grain
    let classic = get(s, "graphics").as_str() == Some("vanilla");
    let traced = get(s, "graphics").as_str() == Some("enhanced_plus");
    sel_setting(ui, s, dirty, "s-msaa", c.row(), "Anti-aliasing", "msaa", &[("1", "Off"), ("2", "2x MSAA"), ("4", "4x MSAA"), ("8", "8x MSAA")]);
    sel_setting(ui, s, dirty, "s-scale", c.row(), "Render scale", "render_scale", &[("auto", "Auto"), ("1", "100%"), ("0.85", "85%"), ("0.75", "75%"), ("0.67", "67%"), ("0.5", "50%")]);
    sel_setting(ui, s, dirty, "s-af", c.row(), "Anisotropic", "anisotropy", &[("1", "Off"), ("2", "2x"), ("4", "4x"), ("8", "8x"), ("16", "16x")]);
    if !classic {
        sel_setting(ui, s, dirty, "s-shadow", c.row(), "Shadow map", "shadow_size", &[("1024", "1024"), ("2048", "2048"), ("4096", "4096")]);
        // (Enhanced+ traces its shadows, occlusion and reflections: always on there)
        if !traced {
            toggle_setting(ui, s, dirty, c.row(), "Ambient occlusion", "ssao");
            toggle_setting(ui, s, dirty, c.row(), "Sun shadows", "shadows");
        }
        sel_setting(ui, s, dirty, "s-casters", c.row(), "Shadows cast by", "shadow_casters", &[("all", "Every solid mesh"), ("omsi", "[shadow] meshes, as OMSI")]);
        toggle_setting(ui, s, dirty, c.row(), "Detail texturing up close", "detail_textures");
        let mut night = get(s, "night_brightness").as_f64().unwrap_or(0.0) as f32;
        if ui.slider("s-night", c.row(), &mut night, 0.0, 3.0, 0.25, "Night brightness", &|v| if v < 0.01 { "Off".to_string() } else { format!("+{v:.2}") }) {
            s["night_brightness"] = json!((night / 0.25).round() * 0.25);
            *dirty = 0.3;
        }
        // (an LED panel's dots are its own light: how bright they burn, and how much of the
        // mip chain the panel's picture and its mask are held at - 0 point-samples them,
        // the sharpest dots and the worst shimmer; higher holds them at the level the
        // screen footprint asks for at most)
        let mut led = get(s, "led_glow").as_i64().unwrap_or(6) as f32;
        if ui.slider("s-led", c.row(), &mut led, 0.0, 15.0, 1.0, "LED glow", &|v| if v < 0.5 { "Off".to_string() } else { format!("{}", v as i64) }) {
            s["led_glow"] = json!(led.round() as i64);
            *dirty = 0.3;
        }
        let mut mip = get(s, "led_mips").as_f64().unwrap_or(1.3) as f32;
        if ui.slider("s-led-mip", c.row(), &mut mip, 0.0, 4.0, 0.05, "LED mip strength", &|v| if v < 0.005 { "Off".to_string() } else { format!("{v:.2}") }) {
            s["led_mips"] = json!((mip / 0.05).round() * 0.05);
            *dirty = 0.3;
        }
    }
    // (the models' `[isshadow]` blob is what OMSI draws under a vehicle in every graphics
    // mode, the vanilla one included, so its switch is not part of the extras above)
    toggle_setting(ui, s, dirty, c.row(), "OMSI's shadow meshes (under vehicles)", "shadow_blobs");
    if !traced {
        toggle_setting(ui, s, dirty, c.row(), "Reflection maps (paint, chrome, glass)", "reflections");
    }
    toggle_setting(ui, s, dirty, c.row(), "Clouds", "clouds");
    // (the enhanced graphics' volumetric clouds marched in fewer steps: see
    // `Lighting::low_clouds`; the vanilla ones are a texture whatever this says)
    if matches!(get(s, "graphics").as_str(), Some("enhanced" | "enhanced_plus")) && get(s, "clouds").as_bool() != Some(false) {
        sel_setting(ui, s, dirty, "s-cloud-quality", c.row(), "Cloud quality", "cloud_quality", &[("high", "High"), ("low", "Low")]);
    }
    sel_setting(ui, s, dirty, "s-rain-quality", c.row(), "Rain quality", "rain_quality", &[("high", "High"), ("medium", "Medium"), ("low", "Low")]);
    toggle_setting(ui, s, dirty, c.row(), "Windy trees", "windy_trees");
    let left = c.used();
    let mut c = Col::new(ui, cols[1], "Display");
    toggle_setting(ui, s, dirty, c.row(), "Fullscreen", "fullscreen");
    // (the window's own size in pixels; a Steam Deck's Gaming Mode and other odd screens,
    // #904 - "Automatic" fits the screen, and fills it under gamescope)
    sel_setting(ui, s, dirty, "s-res", c.row(), "Window size", "resolution", RESOLUTIONS);
    toggle_setting(ui, s, dirty, c.row(), "V-sync", "vsync");
    sel_setting(ui, s, dirty, "s-fps", c.row(), "Frame limit", "max_fps", &[("0", "Screen refresh rate"), ("30", "30 fps"), ("45", "45 fps"), ("60", "60 fps"), ("120", "120 fps"), ("144", "144 fps"), ("1000", "Unlimited")]);
    // (a Mac has Metal only; elsewhere a driver's Vulkan that misbehaves, or a card without
    // it, is got round here; on Windows also OpenGL ES on ANGLE over DirectX 11, for a chip
    // with none of the others - it is tried only when the package has ANGLE's DLLs)
    if cfg!(windows) {
        sel_setting(ui, s, dirty, "s-api", c.row(), "Graphics API", "graphics_api", &[("auto", "Automatic"), ("vulkan", "Vulkan"), ("dx12", "DirectX 12"), ("gl", "OpenGL"), ("angle", "ANGLE (DirectX 11)")]);
    } else if !cfg!(target_os = "macos") {
        sel_setting(ui, s, dirty, "s-api", c.row(), "Graphics API", "graphics_api", &[("auto", "Automatic"), ("vulkan", "Vulkan"), ("gl", "OpenGL")]);
    }
    c.section(ui, "World & memory");
    sel_setting(ui, s, dirty, "s-view", c.row(), "View distance", "view_distance", &[("auto", "Default (1200 m)"), ("600", "600 m - fastest"), ("900", "900 m"), ("1200", "1200 m"), ("1500", "1500 m"), ("2000", "2000 m"), ("2500", "2500 m")]);
    sel_setting(ui, s, dirty, "s-maxobj", c.row(), "Object distance", "max_obj_dist", &[("auto", "Automatic"), ("500", "500 m"), ("750", "750 m"), ("900", "900 m"), ("1500", "1500 m"), ("3000", "3000 m")]);
    sel_setting(ui, s, dirty, "s-minobj", c.row(), "Small objects", "min_obj_size", &[("0.005", "All"), ("0.013", "Normal"), ("0.02", "Fewer (faster)"), ("0.03", "Few (fastest)")]);
    sel_setting(ui, s, dirty, "s-mirror", c.row(), "Mirrors", "mirror_size", &[("0", "Off"), ("128", "Low (128)"), ("256", "Normal (256)"), ("512", "High (512)"), ("1024", "Very high (1024)")]);
    sel_setting(ui, s, dirty, "s-mirror-refresh", c.row(), "Real-time reflections", "mirror_refresh", &[("off", "None (frozen picture)"), ("eco", "Economical"), ("full", "Full")]);
    toggle_setting(ui, s, dirty, c.row(), "Mirror panels every frame", "mirror_hud_every_frame");
    // (the game takes the smaller of an eighth of the memory and what the graphics
    // adapter is taken to hold, see `memory::texture_budget`)
    let adapter_mb = omsi_render::ADAPTER_TEXTURE_MB.load(std::sync::atomic::Ordering::Relaxed) as i64;
    let auto_mb = match (get(s, "texture_memory_auto").as_i64().unwrap_or(0), adapter_mb) {
        (m, 0) => m,
        (0, a) => a,
        (m, a) => m.min(a),
    };
    let auto_label = if auto_mb > 0 { format!("Automatic ({} here)", mb(auto_mb)) } else { "Automatic".to_string() };
    let opts: Vec<(&str, &str)> = vec![("0", auto_label.as_str()), ("500", "500 MB"), ("1000", "1 GB"), ("1500", "1.5 GB"), ("2000", "2 GB"), ("3000", "3 GB"), ("4000", "4 GB"), ("6000", "6 GB")];
    sel_setting(ui, s, dirty, "s-texmem", c.row(), "Texture memory", "texture_memory", &opts);
    toggle_setting(ui, s, dirty, c.row(), "Compress textures on loading", "texture_compression");
    // (off: DXT/BC textures decoded to RGBA on loading, for a driver that mishandles block
    // formats - see `Settings::gpu_texture_compression`)
    toggle_setting(ui, s, dirty, c.row(), "DXT/BC textures stay compressed on the GPU", "gpu_texture_compression");
    c.section(ui, "Profiles");
    graphics_profiles_block(ui, s, dirty, &mut c);
    [left, c.used()]
}

/// How the bus answers the keys, the mouse, a wheel and pedals (which key does what: the
/// Controls page).
fn driving_tab(ui: &mut Ui, s: &mut Value, dirty: &mut f32, out: &mut Outside, cols: [Rect; 2]) -> [f32; 2] {
    let mut c = Col::new(ui, cols[0], "Keyboard & mouse");
    if super::mobile::mobile() {
        c.section(ui, "Touch controls");
        sel_setting(ui, s, dirty, "s-touch-doors", c.row(), "Door buttons", "touch_door_mode", &[("standard", "Standard (physical doors)"), ("direct", "Direct OMSI triggers")]);
        c.section(ui, "Keyboard & mouse");
    }
    sel_setting(ui, s, dirty, "s-keys", c.row(), "Driving keys", "drive_keys", &[("omsi", "Custom controls (Controls page)"), ("simple", "W A S D + arrows"), ("wasd", "W A S D only"), ("arrows", "Arrow keys only")]);
    toggle_setting(ui, s, dirty, c.row(), "Steering linearity (keys at OMSI's steady pace)", "steering_linear");
    toggle_setting(ui, s, dirty, c.row(), "Old Steering (the wheel stays, turn it back yourself)", "old_steering");
    toggle_setting(ui, s, dirty, c.row(), "Dynamic steering (slower keys at speed, OMSI's redSteerSpd)", "red_steer_spd");
    let mut ms = get(s, "mouse_sens").as_f64().unwrap_or(1.0) as f32;
    if ui.slider("s-mouse", c.row(), &mut ms, 0.1, 3.0, 0.05, "Mouse steering sensitivity (O)", &|v| if (v - 1.0).abs() < 0.01 { "OMSI".to_string() } else { format!("{:.0}%", v * 100.0) }) {
        s["mouse_sens"] = json!((ms * 100.0).round() / 100.0);
        *dirty = 0.3;
    }
    let mut mouse_pedal = get(s, "mouse_pedal_strength")
        .as_f64()
        .unwrap_or(1.0) as f32;
    if ui.slider(
        "s-mouse-pedal",
        c.row(),
        &mut mouse_pedal,
        0.5,
        2.0,
        0.05,
        "Mouse pedal strength",
        &|v| {
            if (v - 1.0).abs() < 0.01 {
                "OMSI".to_string()
            } else {
                format!("{:.0}%", v * 100.0)
            }
        },
    ) {
        s["mouse_pedal_strength"] = json!((mouse_pedal * 100.0).round() / 100.0);
        *dirty = 0.3;
    }
    toggle_setting(ui, s, dirty, c.row(), "Smooth mouse steering (off: the wheel follows the cursor at once, as in OMSI)", "mouse_smooth");
    toggle_setting(ui, s, dirty, c.row(), "Hold the cursor while the mouse steers (off: the crosshair stays free, the window's edges are the lock)", "mouse_hold");
    toggle_setting(ui, s, dirty, c.row(), "A right click ends the mouse steering (as in OMSI)", "mouse_right_off");
    toggle_setting(ui, s, dirty, c.row(), "Indicators cancel themselves (as the bus's script does)", "blinker_cancel");
    toggle_setting(ui, s, dirty, c.row(), "The keyboard brake stays on until the throttle (as in OMSI)", "brake_hold");
    toggle_setting(ui, s, dirty, c.row(), "Automatic clutch (manual gearboxes)", "auto_clutch");
    // (it was in the game's menu only, #1780)
    toggle_setting(ui, s, dirty, c.row(), "Automated manual gearbox (shifts a manual gearbox for you by the engine speed)", "auto_shift");
    if ui.button("s-go-keys", c.row(), "Change the keys", Some("keyboard"), ButtonKind::Normal) {
        out.controls = Some(0);
    }
    let left = c.used();
    let mut c = Col::new(ui, cols[1], "Game controllers");
    toggle_setting(ui, s, dirty, c.row(), "H-pattern shifter: return to neutral when the gear is released", "momentary_gears");
    // steering wheels: the wheel's own rotation and how much of it is the bus's full lock
    // (a real bus: about two and a half turns), force feedback the other way round
    let mut range = get(s, "wheel_range").as_f64().unwrap_or(900.0) as f32;
    if ui.slider("s-wrange", c.row(), &mut range, 180.0, 1800.0, 30.0, "Wheel rotation", &|v| format!("{v:.0}°")) {
        s["wheel_range"] = json!(range.round());
        *dirty = 0.3;
    }
    let mut lock = get(s, "wheel_lock").as_f64().unwrap_or(0.0) as f32;
    if ui.slider("s-wlock", c.row(), &mut lock, 0.0, 1800.0, 30.0, "Full lock at", &|v| if v < 45.0 { "OMSI".to_string() } else { format!("{v:.0}°") }) {
        s["wheel_lock"] = json!(if lock < 45.0 { 0.0 } else { lock.round() });
        *dirty = 0.3;
    }
    // a gamepad stick's steering, eased onto where the stick points (0: as it reads)
    let mut pad_smooth = get(s, "pad_steer_smooth").as_f64().unwrap_or(120.0) as f32;
    if ui.slider("s-pad-steer-smooth", c.row(), &mut pad_smooth, 0.0, 300.0, 10.0, "Stick steering smoothing", &|v| if v <= 0.0 { "Off".to_string() } else { format!("{v:.0} ms") }) {
        s["pad_steer_smooth"] = json!(pad_smooth.round());
        *dirty = 0.3;
    }
    // how fast the stick turns the wheel (middle to full lock), and its dead zone
    let mut pad_speed = get(s, "pad_steer_speed").as_f64().unwrap_or(2.0) as f32;
    if ui.slider("s-pad-steer-speed", c.row(), &mut pad_speed, 0.8, 5.0, 0.1, "Stick steering speed", &|v| format!("{v:.1} s")) {
        s["pad_steer_speed"] = json!((pad_speed * 10.0).round() / 10.0);
        *dirty = 0.3;
    }
    let mut pad_dead = get(s, "pad_deadzone").as_f64().unwrap_or(0.08) as f32;
    if ui.slider("s-pad-deadzone", c.row(), &mut pad_dead, 0.0, 0.4, 0.01, "Stick dead zone", &|v| format!("{:.0} %", v * 100.0)) {
        s["pad_deadzone"] = json!((pad_dead * 100.0).round() / 100.0);
        *dirty = 0.3;
    }
    toggle_setting(ui, s, dirty, c.row(), "Stick steers like a wheel (a wheel seen as a gamepad)", "pad_steer_linear");
    // Xbox, PlayStation 4 or 5: how the buttons are named, and the default buttons of a
    // gamepad nobody set up
    sel_setting(ui, s, dirty, "s-pad-type", c.row(), "Gamepad type", "pad_type", &[("auto", "Automatic"), ("xbox", "Xbox"), ("ps4", "PlayStation 4"), ("ps5", "PlayStation 5")]);
    toggle_setting(ui, s, dirty, c.row(), "Default gamepad buttons (indicators, doors, views, gears, menu, pause)", "pad_buttons");
    toggle_setting(ui, s, dirty, c.row(), "Arrow keys switch the cameras with a wheel too (no glance)", "arrows_switch_cams");
    // the pedals' response: softer (below 1) or stronger (above 1) than the pedal reads
    for (key, label, id) in [("pedal_throttle", "Throttle pedal strength", "s-pedt"), ("pedal_brake", "Brake pedal strength", "s-pedb")] {
        let mut v = get(s, key).as_f64().unwrap_or(1.0) as f32;
        if ui.slider(id, c.row(), &mut v, 0.5, 2.0, 0.05, label, &|v| if (v - 1.0).abs() < 0.01 { "Normal".to_string() } else if v < 1.0 { format!("Softer x{v:.2}") } else { format!("Stronger x{v:.2}") }) {
            s[key] = json!((v * 100.0).round() / 100.0);
            *dirty = 0.3;
        }
    }
    toggle_setting(ui, s, dirty, c.row(), "Force feedback and vibration", "ff_enabled");
    toggle_setting(ui, s, dirty, c.row(), "Invert force feedback by default", "ff_invert");
    c.y += ui.paragraph("Wheels with a saved direction use their own setting under Controls → Game controllers.", Vec2::new(c.inner.x, c.y), c.inner.w, 12.5, Weight::Regular, TEXT_DIM) + 8.0;
    toggle_setting(ui, s, dirty, c.row(), "Invert force feedback", "ff_invert");
    // what the wheel feels all the time while the bus runs: the road's grain and the
    // engine's buzz, and how long a jolt eases away once it is over
    for (key, label, id) in [("ff_road_vib", "Road texture vibration", "s-ffroad"), ("ff_engine_vib", "Engine vibration", "s-ffeng")] {
        let mut v = get(s, key).as_f64().unwrap_or(1.0) as f32;
        if ui.slider(id, c.row(), &mut v, 0.0, 4.0, 0.05, label, &|v| if v < 0.01 { "Off".to_string() } else if (v - 1.0).abs() < 0.01 { "Normal".to_string() } else { format!("{:.0}%", v * 100.0) }) {
            s[key] = json!((v * 20.0).round() / 20.0);
            *dirty = 0.3;
        }
    }
    let mut fade = get(s, "ff_fade").as_f64().unwrap_or(0.28) as f32;
    if ui.slider("s-fffade", c.row(), &mut fade, 0.0, 1.5, 0.05, "Vibration fade-out", &|v| if v < 0.01 { "Off".to_string() } else { format!("{:.0} ms", (v * 1000.0).round() as i32) }) {
        s["ff_fade"] = json!((fade * 100.0).round() / 100.0);
        *dirty = 0.3;
    }
    if ui.button("s-wreset", c.row(), "Reset wheel settings", Some("restart_alt"), ButtonKind::Normal) {
        s["wheel_range"] = json!(900.0);
        s["wheel_lock"] = json!(0.0);
        s["ff_invert"] = json!(false);
        s["ff_enabled"] = json!(true);
        s["ff_road_vib"] = json!(1.0);
        s["ff_engine_vib"] = json!(1.0);
        s["ff_fade"] = json!(0.28);
        *dirty = 0.3;
    }
    if ui.button("s-go-pads", c.row(), "Set up a wheel or pedals", Some("sports_esports"), ButtonKind::Normal) {
        out.controls = Some(1);
    }
    [left, c.used()]
}

/// What the driver sees: the seat, the views from it and around the bus, head tracking, VR.
fn camera_tab(ui: &mut Ui, s: &mut Value, dirty: &mut f32, out: &mut Outside, cols: [Rect; 2]) -> [f32; 2] {
    let mut c = Col::new(ui, cols[0], "Driver's view");
    // the driver's eye, moved from the bus's own camera
    c.section(ui, "Seat position");
    for (key, label, id) in [("seat_y", "Seat forward / back", "s-seaty"), ("seat_z", "Seat up / down", "s-seatz"), ("seat_x", "Seat right / left", "s-seatx")] {
        let mut v = get(s, key).as_f64().unwrap_or(0.0) as f32;
        if ui.slider(id, c.row(), &mut v, -0.6, 0.6, 0.01, label, &|v| format!("{:+.0} cm", v * 100.0)) {
            s[key] = json!((v * 100.0).round() / 100.0);
            *dirty = 0.3;
        }
    }
    let mut seat_pitch = get(s, "seat_pitch_deg").as_f64().unwrap_or(0.0) as f32;
    if ui.slider("s-seat-pitch", c.row(), &mut seat_pitch, -45.0, 45.0, 1.0, "Head pitch", &|v| format!("{v:+.0}°")) {
        s["seat_pitch_deg"] = json!(seat_pitch.round());
        *dirty = 0.3;
    }
    if ui.button("s-seatreset", c.row(), "Reset the seat position", Some("restart_alt"), ButtonKind::Normal) {
        for k in ["seat_x", "seat_y", "seat_z"] {
            s[k] = json!(0.0);
        }
        s["seat_pitch_deg"] = json!(0.0);
        *dirty = 0.3;
    }
    let fov_key = if get(s, "triple_screen").as_bool().unwrap_or(false)
        && !get(s, "vr").as_bool().unwrap_or(false)
    {
        "triple_fov_deg"
    } else {
        "fov"
    };
    let mut fov = get(s, fov_key).as_f64().unwrap_or(0.0) as f32;
    if ui.slider("s-fov", c.row(), &mut fov, 0.0, 120.0, 1.0, "Field of view", &|v| if v < 20.0 { "Default".to_string() } else { format!("{v:.0}°") }) {
        s[fov_key] = json!(if fov < 20.0 { 0.0 } else { fov.round() });
        *dirty = 0.3;
    }
    let mut look = get(s, "look_sens").as_f64().unwrap_or(1.0) as f32;
    if ui.slider("s-look-sens", c.row(), &mut look, 0.1, 2.0, 0.05, "Mouse look sensitivity", &|v| if (v - 1.0).abs() < 0.01 { "OMSI".to_string() } else { format!("{:.0}%", v * 100.0) }) {
        s["look_sens"] = json!((look * 100.0).round() / 100.0);
        *dirty = 0.3;
    }
    toggle_setting(ui, s, dirty, c.row(), "Right stick turns the view", "right_stick_look");
    let mut smooth = get(s, "look_smoothing_ms").as_f64().unwrap_or(0.0) as f32;
    if ui.slider("s-look-smoothing", c.row(), &mut smooth, 0.0, 200.0, 10.0, "Smooth the mouse look", &|v| if v <= 0.0 { "Off".to_string() } else { format!("{v:.0} ms") }) {
        s["look_smoothing_ms"] = json!(smooth.round());
        *dirty = 0.3;
    }
    c.section(ui, "A head at rest");
    let mut idle = get(s, "head_idle").as_f64().unwrap_or(0.0) as f32;
    if ui.slider("s-head-idle", c.row(), &mut idle, 0.0, 1.0, 0.05, "Head sway at a standstill", &|v| if v <= 0.0 { "Off".to_string() } else { format!("{:.0}%", v * 100.0) }) {
        s["head_idle"] = json!((idle * 100.0).round() / 100.0);
        *dirty = 0.3;
    }
    let mut pace = get(s, "head_idle_pace").as_f64().unwrap_or(1.0) as f32;
    if ui.slider("s-head-idle-pace", c.row(), &mut pace, 0.5, 2.0, 0.05, "Sway pace", &|v| format!("{:.0}%", v * 100.0)) {
        s["head_idle_pace"] = json!((pace * 100.0).round() / 100.0);
        *dirty = 0.3;
    }
    toggle_setting(ui, s, dirty, c.row(), "Driver's view turns with the steering", "steer_look");
    let mut angle = get(s, "steer_look_angle").as_f64().unwrap_or(30.0) as f32;
    if ui.slider("s-steer-look-angle", c.row(), &mut angle, 0.0, 60.0, 1.0, "Steering view angle", &|v| format!("{v:.0}°")) {
        s["steer_look_angle"] = json!(angle);
        *dirty = 0.3;
    }
    let mut response = get(s, "steer_look_response").as_f64().unwrap_or(0.25) as f32;
    if ui.slider("s-steer-look-response", c.row(), &mut response, 0.05, 1.0, 0.05, "Steering view response", &|v| format!("{:.0} ms", v * 1000.0)) {
        s["steer_look_response"] = json!(response);
        *dirty = 0.3;
    }
    toggle_setting(ui, s, dirty, c.row(), "Head moves with the bus", "head_movement");
    toggle_setting(ui, s, dirty, c.row(), "Camera glides between viewpoints", "driverview_smooth");
    toggle_setting(ui, s, dirty, c.row(), "Driver's hands in the cab view", "hands_in_cab");
    toggle_setting(ui, s, dirty, c.row(), "Right mouse button turns the view, Shift+right zooms (off: right zooms as in OMSI, the wheel button turns)", "alt_view");
    toggle_setting(ui, s, dirty, c.row(), "Precision mouse zoom (FOV curve instead of the linear way)", "precision_zoom");
    let left = c.used();
    let mut c = Col::new(ui, cols[1], "Outside views");
    toggle_setting(ui, s, dirty, c.row(), "Camera collisions (outside view)", "camera_collision");
    toggle_setting(ui, s, dirty, c.row(), "Driver at the wheel (outside views)", "driver");
    c.section(ui, "Head tracking");
    toggle_setting(ui, s, dirty, c.row(), "Head tracking (native TrackIR / OpenTrack)", "head_tracking");

    // TrackIR axis controls. Values are stored as user-facing percentages:
    // 0% disables an axis; 100% (the default) is 1:1.
    c.section(ui, "Rotation");
    for (key, id, label) in [
        ("head_tracking_yaw_sens", "s-trackir-yaw", "Yaw sensitivity (left / right)"),
        ("head_tracking_pitch_sens", "s-trackir-pitch", "Pitch sensitivity (up / down)"),
        ("head_tracking_roll_sens", "s-trackir-roll", "Roll sensitivity"),
    ] {
        let mut v = get(s, key).as_f64().unwrap_or(100.0) as f32;
        if ui.slider(id, c.row(), &mut v, 0.0, 100.0, 1.0, label, &|v| {
            if v <= 0.0 { "Off".to_string() } else { format!("{v:.0}%") }
        }) {
            s[key] = json!(v.round().clamp(0.0, 100.0));
            *dirty = 0.3;
        }
    }
    toggle_setting(ui, s, dirty, c.row(), "Invert yaw", "head_tracking_invert_yaw");
    toggle_setting(ui, s, dirty, c.row(), "Invert pitch", "head_tracking_invert_pitch");
    toggle_setting(ui, s, dirty, c.row(), "Invert roll", "head_tracking_invert_roll");

    c.section(ui, "Position");
    for (key, id, label) in [
        ("head_tracking_x_sens", "s-trackir-x", "X sensitivity (left / right)"),
        ("head_tracking_y_sens", "s-trackir-y", "Y sensitivity (up / down)"),
        ("head_tracking_z_sens", "s-trackir-z", "Z sensitivity (forward / back)"),
    ] {
        let mut v = get(s, key).as_f64().unwrap_or(100.0) as f32;
        if ui.slider(id, c.row(), &mut v, 0.0, 100.0, 1.0, label, &|v| {
            if v <= 0.0 { "Off".to_string() } else { format!("{v:.0}%") }
        }) {
            s[key] = json!(v.round().clamp(0.0, 100.0));
            *dirty = 0.3;
        }
    }
    toggle_setting(ui, s, dirty, c.row(), "Invert X", "head_tracking_invert_x");
    toggle_setting(ui, s, dirty, c.row(), "Invert Y", "head_tracking_invert_y");
    toggle_setting(ui, s, dirty, c.row(), "Invert Z", "head_tracking_invert_z");

    if ui.button("s-trackir-reset", c.row(), "Reset head-tracking axes", Some("restart_alt"), ButtonKind::Normal) {
        for key in [
            "head_tracking_yaw_sens", "head_tracking_pitch_sens", "head_tracking_roll_sens",
            "head_tracking_x_sens", "head_tracking_y_sens", "head_tracking_z_sens",
        ] {
            s[key] = json!(15.0);
        }
        for key in [
            "head_tracking_invert_yaw", "head_tracking_invert_pitch", "head_tracking_invert_roll",
            "head_tracking_invert_x", "head_tracking_invert_y", "head_tracking_invert_z",
        ] {
            s[key] = json!(false);
        }
        *dirty = 0.3;
    }
    c.section(ui, "Triple screen");
    toggle_setting(
        ui,
        s,
        dirty,
        c.row(),
        "Three screen projections",
        "triple_screen",
    );
    toggle_setting(
        ui,
        s,
        dirty,
        c.row(),
        "Span three monitors at startup",
        "triple_span",
    );
    toggle_setting(
        ui,
        s,
        dirty,
        c.row(),
        "HUD on centre screen",
        "triple_hud_center",
    );
    if get(s, "triple_screen").as_bool().unwrap_or(false) {
        ui.label(
            c.row(),
            "Three equal screens in a horizontal row. OpenXR takes priority.",
        );
        let mut value = get(s, "triple_width_mm").as_f64().unwrap_or(600.0) as f32;
        if ui.slider(
            "s-triple-width_mm",
            c.row(),
            &mut value,
            200.0,
            2000.0,
            10.0,
            "Visible width of one panel",
            &|v| format!("{v:.0} mm"),
        ) {
            s["triple_width_mm"] = json!(value);
            *dirty = 0.3;
        }
        let mut value = get(s, "triple_distance_mm").as_f64().unwrap_or(650.0) as f32;
        if ui.slider(
            "s-triple-distance_mm",
            c.row(),
            &mut value,
            200.0,
            3000.0,
            10.0,
            "Eye to centre screen",
            &|v| format!("{v:.0} mm"),
        ) {
            s["triple_distance_mm"] = json!(value);
            s["triple_fov_deg"] = json!(0.0);
            *dirty = 0.3;
        }
        let mut value = get(s, "triple_bezel_mm").as_f64().unwrap_or(0.0) as f32;
        if ui.slider(
            "s-triple-bezel_mm",
            c.row(),
            &mut value,
            0.0,
            100.0,
            1.0,
            "Both frames at each join",
            &|v| format!("{v:.0} mm"),
        ) {
            s["triple_bezel_mm"] = json!(value);
            *dirty = 0.3;
        }
        let mut value = get(s, "triple_left_angle_deg").as_f64().unwrap_or(45.0) as f32;
        if ui.slider(
            "s-triple-left_angle_deg",
            c.row(),
            &mut value,
            0.0,
            90.0,
            1.0,
            "Left screen inward angle",
            &|v| format!("{v:.0}°"),
        ) {
            s["triple_left_angle_deg"] = json!(value);
            *dirty = 0.3;
        }
        let mut value = get(s, "triple_right_angle_deg").as_f64().unwrap_or(45.0) as f32;
        if ui.slider(
            "s-triple-right_angle_deg",
            c.row(),
            &mut value,
            0.0,
            90.0,
            1.0,
            "Right screen inward angle",
            &|v| format!("{v:.0}°"),
        ) {
            s["triple_right_angle_deg"] = json!(value);
            *dirty = 0.3;
        }
        let mut value = get(s, "triple_eye_height_mm").as_f64().unwrap_or(0.0) as f32;
        if ui.slider(
            "s-triple-eye_height_mm",
            c.row(),
            &mut value,
            -500.0,
            500.0,
            1.0,
            "Eye above screen centre",
            &|v| format!("{v:.0} mm"),
        ) {
            s["triple_eye_height_mm"] = json!(value);
            *dirty = 0.3;
        }
    }
    if cfg!(windows) {
        c.section(ui, "Virtual reality");
        toggle_setting(ui, s, dirty, c.row(), "Use OpenXR headset", "vr");
        if get(s, "vr").as_bool().unwrap_or(false) {
            sel_setting(ui, s, dirty, "s-vr-scale", c.row(), "Eye resolution", "vr_scale", &[("0.5", "50%"), ("0.65", "65%"), ("0.8", "80%"), ("1", "100%")]);
            sel_setting(ui, s, dirty, "s-vr-head-smoothing", c.row(), "Head tracking smoothing", "vr_head_smoothing_ms", &[("0", "Off"), ("5", "5 ms"), ("10", "10 ms"), ("20", "20 ms"), ("30", "30 ms")]);
            sel_setting(ui, s, dirty, "s-vr-mirror-rate", c.row(), "Bus mirror refresh", "vr_mirror_rate", &[("0", "Off"), ("8", "8/s"), ("16", "16/s"), ("24", "24/s"), ("32", "32/s"), ("48", "48/s"), ("60", "60/s"), ("90", "90/s"), ("120", "120/s"), ("180", "180/s"), ("240", "240/s"), ("360", "360/s"), ("-1", "Every frame")]);
            c.y += ui.paragraph("The rate is shared by all bus mirrors. Higher rates can reduce game FPS.", Vec2::new(c.inner.x, c.y), c.inner.w, 12.0, Weight::Regular, TEXT_DIM) + 8.0;
            toggle_setting(ui, s, dirty, c.row(), "Show headset picture on monitor", "vr_desktop_mirror");
            // (the VR keys head the Controls page's game list)
            if ui.button("s-go-vr-keys", c.row(), "Change the VR keys", Some("keyboard"), ButtonKind::Normal) {
                out.controls = Some(0);
            }
        }
    }
    [left, c.used()]
}

/// How loud the bus, the traffic and the surroundings are, and what passengers say.
fn sound_tab(ui: &mut Ui, s: &mut Value, dirty: &mut f32, cols: [Rect; 2]) -> [f32; 2] {
    let mut c = Col::new(ui, cols[0], "Volume");
    let mut vol = get(s, "volume").as_f64().unwrap_or(0.6) as f32;
    if ui.slider("s-vol", c.row(), &mut vol, 0.0, 1.0, 0.05, "Volume", &|v| format!("{:.0}%", v * 100.0)) {
        s["volume"] = json!((vol * 100.0).round() / 100.0);
        *dirty = 0.3;
    }
    for (key, label, id) in [("vol_ai", "Traffic", "s-volai"), ("vol_scenery", "Surroundings", "s-volsc")] {
        let mut v = get(s, key).as_f64().unwrap_or(1.0) as f32;
        if ui.slider(id, c.row(), &mut v, 0.0, 1.0, 0.05, label, &|v| format!("{:.0}%", v * 100.0)) {
            s[key] = json!((v * 100.0).round() / 100.0);
            *dirty = 0.3;
        }
    }
    toggle_setting(ui, s, dirty, c.row(), "Doppler effect", "doppler");
    toggle_setting(ui, s, dirty, c.row(), "Ambience (places, weather, nature, town)", "ambient");
    let mut amb = get(s, "vol_ambient").as_f64().unwrap_or(0.8) as f32;
    if ui.slider("s-volamb", c.row(), &mut amb, 0.0, 1.0, 0.05, "Ambience volume", &|v| format!("{:.0}%", v * 100.0)) {
        s["vol_ambient"] = json!((amb * 100.0).round() / 100.0);
        *dirty = 0.3;
    }
    sel_setting(ui, s, dirty, "s-voices", c.row(), "Passenger voices", "pax_voices", &[("all", "Greetings and tickets"), ("tickets", "Only the ticket asked for"), ("off", "Silent")]);
    // the bus radio's own sound (all off at first)
    for (key, label, id) in [("radio_speaker", "Radio: small speaker", "s-radspk"), ("radio_hiss", "Radio: FM hiss", "s-radhiss"), ("radio_dropouts", "Radio: signal dropouts", "s-raddrop")] {
        let mut v = get(s, key).as_f64().unwrap_or(0.0) as f32;
        if ui.slider(id, c.row(), &mut v, 0.0, 1.0, 0.05, label, &|v| format!("{:.0}%", v * 100.0)) {
            s[key] = json!((v * 100.0).round() / 100.0);
            *dirty = 0.3;
        }
    }
    [c.used(), radio_stations(ui, cols[1])]
}

thread_local! {
    /// The radio stations as the Sound settings edit them (`radio.cfg`, read the first time).
    static RADIO: std::cell::RefCell<Option<Vec<(String, String)>>> = const { std::cell::RefCell::new(None) };
}

/// The bus radios' internet stations (`radio.cfg`, see `radio`): each with its name and
/// address, removed or added here, saved at once (#857). Returns the column's height.
fn radio_stations(ui: &mut Ui, r: Rect) -> f32 {
    let mut c = Col::new(ui, r, "Radio stations");
    c.y += ui.paragraph("A radio's station button n plays the n-th station, a cassette player the first; Shift+R steps through them. An address is an MP3, AAC or Ogg stream or an .m3u/.pls playlist.", Vec2::new(c.inner.x, c.y), c.inner.w, 12.5, Weight::Regular, TEXT_DIM) + 8.0;
    RADIO.with(|cell| {
        let mut cell = cell.borrow_mut();
        let list = cell.get_or_insert_with(|| host().radio_stations());
        let mut changed = false;
        let mut remove = None;
        for (k, (name, address)) in list.iter_mut().enumerate() {
            let row = c.row();
            let nw = (row.w * 0.3).round();
            changed |= ui.text_input(&format!("radio-name-{k}"), Rect::new(row.x, row.y, nw, row.h), name, "Name", None);
            changed |= ui.text_input(&format!("radio-url-{k}"), Rect::new(row.x + nw + 8.0, row.y, row.w - nw - 8.0 - 36.0, row.h), address, "https://...", None);
            if ui.icon_button(&format!("radio-del-{k}"), Vec2::new(row.right() - 16.0, row.center().y), 14.0, "delete", "Remove this station") {
                remove = Some(k);
            }
        }
        if let Some(k) = remove {
            list.remove(k);
            changed = true;
        }
        if ui.button("radio-add", c.row(), "Add a station", Some("add"), ButtonKind::Normal) {
            list.push((String::new(), String::new()));
        }
        if changed {
            if let Err(e) = host().save_radio_stations(list) {
                log::warn!("radio.cfg: {e}");
            }
        }
    });
    c.used()
}

/// How the world behaves: passengers, traffic, collisions, wear, the clock.
fn gameplay_tab(ui: &mut Ui, s: &mut Value, dirty: &mut f32, cols: [Rect; 2]) -> [f32; 2] {
    let mut c = Col::new(ui, cols[0], "Passengers");
    sel_setting(ui, s, dirty, "s-board", c.row(), "Boarding", "boarding", &[("auto", "Pay and take the ticket"), ("pay", "The driver sells the ticket"), ("walk", "Just walk in")]);
    sel_setting(ui, s, dirty, "s-stand", c.row(), "Standing passengers", "standing_chance", &[("0", "Never"), ("5", "5%"), ("10", "10%"), ("25", "25%"), ("50", "50%"), ("100", "Always")]);
    sel_setting(ui, s, dirty, "s-pax-animation", c.row(), "Passenger animations", "passenger_animation", &[("original", "Original OMSI"), ("enhanced", "Procedural (experimental)")]);
    toggle_setting(ui, s, dirty, c.row(), "Passengers pay the exact fare", "exact_fare");
    let mut pd = get(s, "pax_density").as_f64().unwrap_or(1.0) as f32;
    if ui.slider("s-pax", c.row(), &mut pd, 0.0, 2.0, 0.1, "How many passengers", &|v| format!("{:.0}%", v * 100.0)) {
        s["pax_density"] = json!((pd * 100.0).round() / 100.0);
        *dirty = 0.3;
    }
    toggle_setting(ui, s, dirty, c.row(), "Ability to get up (Ctrl+Shift+G)", "get_up");
    c.section(ui, "Traffic");
    sel_setting(ui, s, dirty, "s-unsched", c.row(), "Random traffic", "ai_unsched_factor", &[("25", "25%"), ("50", "50%"), ("75", "75%"), ("100", "100%"), ("150", "150%"), ("200", "200%")]);
    sel_setting(ui, s, dirty, "s-maxsched", c.row(), "Timetable vehicles", "ai_max_scheduled", &[("0", "All"), ("10", "At most 10"), ("25", "At most 25"), ("50", "At most 50")]);
    sel_setting(ui, s, dirty, "s-maxpark", c.row(), "Parked cars", "ai_max_parked", &[("-1", "None"), ("0", "Every space"), ("35", "At most 35"), ("100", "At most 100"), ("250", "At most 250")]);
    // (off: an early bus waits at every stop, as in OMSI)
    toggle_setting(ui, s, dirty, c.row(), "Timetable buses ahead of time wait only at timed stops", "ai_wait_timed_stops_only");
    let left = c.used();
    // OMSI's own options (options.cfg)
    let mut c = Col::new(ui, cols[1], "Simulation");
    sel_setting(ui, s, dirty, "s-maint", c.row(), "Maintenance", "maintenance", &[("0", "Infinite (no wear)"), ("1", "Very bad"), ("2", "Bad"), ("3", "Normal"), ("4", "Good")]);
    toggle_setting(ui, s, dirty, c.row(), "Collisions with vehicles", "collision_vehicles");
    toggle_setting(ui, s, dirty, c.row(), "Collisions with objects (walls, poles)", "collision_objects");
    toggle_setting(ui, s, dirty, c.row(), "Collisions with people", "collision_pedestrians");
    toggle_setting(ui, s, dirty, c.row(), "Start at the real time", "use_real_time");
    toggle_setting(ui, s, dirty, c.row(), "Start on today's date", "use_real_date");
    // the game's clock follows this device's (the host's in multiplayer); the time cannot be set
    toggle_setting(ui, s, dirty, c.row(), "Sync the clock with the real time (locks the time)", "time_sync");
    // the weather follows the METAR report of the airport nearest the map; it cannot be changed then
    toggle_setting(ui, s, dirty, c.row(), "Sync the weather with METAR (locks the weather)", "metar_sync");
    // (in multiplayer the host's or the server's speed counts)
    sel_setting(ui, s, dirty, "s-timespeed", c.row(), "Time speed (not in multiplayer or with the real-time sync)", "time_speed", &[("1", "Real time"), ("2", "x2"), ("4", "x4"), ("8", "x8"), ("15", "x15"), ("30", "x30")]);
    [left, c.used()]
}

/// The language, what the screen shows besides the bus, online, the navigator, updates.
fn general_tab(ui: &mut Ui, s: &mut Value, dirty: &mut f32, out: &mut Outside, cols: [Rect; 2]) -> [f32; 2] {
    let mut c = Col::new(ui, cols[0], "Interface & online");
    {
        let langs: Vec<(&str, &str)> = core::LANGUAGES.iter().map(|l| (l.0, l.1)).collect();
        sel_setting(ui, s, dirty, "s-lang", c.row(), "Language", "language", &langs);
    }
    // (the launcher speaks the chosen language at once)
    host().ui_language(get(s, "language").as_str().unwrap_or("ENG"));
    // (texts nobody has translated: translated on this machine, see `mt`)
    let was = get(s, "machine_translation").as_bool().unwrap_or(false);
    toggle_setting(ui, s, dirty, c.row(), "Translate the remaining texts automatically (offline, downloads 620 MB once)", "machine_translation");
    let now = get(s, "machine_translation").as_bool().unwrap_or(false);
    if now != was {
        host().mt_enable(now);
    }
    let st = host().mt_status();
    if now && !st.is_empty() && st != "Ready" {
        ui.text_in(&st, Rect::new(c.inner.x + 12.0, c.y - 6.0, c.inner.w - 24.0, 16.0), 11.5, omsi_ui::Weight::Regular, TEXT_FAINT, omsi_ui::paint::Align::Left);
        c.y += 14.0;
    }
    toggle_setting(ui, s, dirty, c.row(), "The launcher rests while a game runs (gives the graphics card to the game)", "launcher_rest");
    toggle_setting(ui, s, dirty, c.row(), "Discord Rich Presence", "discord_status");
    let help_height = ui.paragraph(
        "Shows the launcher or your map, bus, line and multiplayer status in Discord.",
        Vec2::new(c.inner.x + 12.0, c.y - 5.0),
        c.inner.w - 24.0,
        11.5,
        omsi_ui::Weight::Regular,
        TEXT_FAINT,
    );
    c.y += help_height + 3.0;
    toggle_setting(ui, s, dirty, c.row(), "Voice chat through GreenTeaSpeak (multiplayer)", "voice_chat");
    let help_height = ui.paragraph(
        "Players near you are heard from where they stand, when GreenTeaSpeak runs with the openOMSI plugin and the server names a voice server.",
        Vec2::new(c.inner.x + 12.0, c.y - 5.0),
        c.inner.w - 24.0,
        11.5,
        omsi_ui::Weight::Regular,
        TEXT_FAINT,
    );
    c.y += help_height + 3.0;
    // (the texts over the picture, the menu, the timetable and the navigator: larger for
    // those who find them hard to read, smaller for more of the picture; on a window taller
    // than 1080p they grow with it as well, and the launcher grows with its window anyway)
    let mut size = get(s, "ui_scale").as_f64().unwrap_or(1.0) as f32;
    if ui.slider("s-uiscale", c.row(), &mut size, 0.5, 2.0, 0.05, "Game interface size", &|v| format!("{:.0}%", v * 100.0)) {
        s["ui_scale"] = json!((size * 100.0).round() / 100.0);
        *dirty = 0.3;
    }
    toggle_setting(ui, s, dirty, c.row(), "Interface grows with the window", "ui_scale_window");
    // (the backgrounds of the whole interface - the navigator, the menu, the timetable, the
    // notes' plates - the texts staying solid; 85 % as designed)
    let mut op = get(s, "ui_opacity").as_f64().unwrap_or(0.85) as f32;
    if ui.slider("s-uiop", c.row(), &mut op, 0.2, 1.0, 0.05, "Interface opacity", &|v| format!("{:.0}%", v * 100.0)) {
        s["ui_opacity"] = json!((op * 100.0).round() / 100.0);
        *dirty = 0.3;
    }
    toggle_setting(ui, s, dirty, c.row(), "Name of the button under the mouse", "tooltips");
    toggle_setting(ui, s, dirty, c.row(), "Frame rate in the corner", "show_fps");
    sel_setting(ui, s, dirty, "s-perf", c.row(), "Performance details", "perf_overlay", &[("off", "Off"), ("compact", "Compact"), ("detailed", "Detailed")]);
    toggle_setting(ui, s, dirty, c.row(), "Notes in the top-left corner", "notes");
    toggle_setting(ui, s, dirty, c.row(), "Chat in online games", "chat");
    // (the chat's own size on top of the interface's; Ctrl + the wheel over it in the game)
    let mut chat = get(s, "chat_size").as_f64().unwrap_or(1.0) as f32;
    if ui.slider("s-chatsize", c.row(), &mut chat, 0.5, 3.0, 0.1, "Chat size", &|v| format!("{:.0}%", v * 100.0)) {
        s["chat_size"] = json!((chat * 10.0).round() / 10.0);
        *dirty = 0.3;
    }
    toggle_setting(ui, s, dirty, c.row(), "Other players' names above their buses", "name_tags");
    c.section(ui, "Navigator");
    toggle_setting(ui, s, dirty, c.row(), "Navigator (Shift+N: map, schedule, off)", "navigator");
    toggle_setting(ui, s, dirty, c.row(), "Route arrows (as in OMSI 2)", "nav_arrows");
    toggle_setting(ui, s, dirty, c.row(), "AI vehicles on the map", "nav_ai");
    // the corner: a little screen with four corners to click
    let r = Rect::new(c.inner.x, c.y, c.inner.w, 70.0);
    ui.label(Rect::new(r.x, r.y, r.w * 0.45, 24.0), "Corner");
    let screen = Rect::new(r.x + r.w * 0.45, r.y, 110.0, 64.0);
    ui.p().rounded(screen, 6.0, Color::WHITE.alpha(0.05));
    ui.p().rounded_border(screen, 6.0, 1.0, Color::WHITE.alpha(0.12));
    let cur = get(s, "navigator_corner").as_str().unwrap_or("bottom-left").to_string();
    for (name, x, yy) in [("top-left", 0.0, 0.0), ("top-right", 1.0, 0.0), ("bottom-left", 0.0, 1.0), ("bottom-right", 1.0, 1.0)] {
        let cell = Rect::new(screen.x + 5.0 + x * (screen.w * 0.5), screen.y + 5.0 + yy * (screen.h * 0.5), screen.w * 0.5 - 10.0, screen.h * 0.5 - 10.0);
        let (h, _, clicked) = ui.interact(id_of(&format!("corner-{name}")), cell);
        if clicked {
            s["navigator_corner"] = json!(name);
            *dirty = 0.3;
        }
        let on = cur == name;
        ui.p().rounded(cell, 3.0, if on { ACCENT } else { Color::WHITE.alpha(if h { 0.2 } else { 0.08 }) });
    }
    // (dragged somewhere else in the game, #940: that place, until a corner is chosen)
    if let Some(a) = host().navigator_placed_at(&cur) {
        let (cw, ch) = (screen.w * 0.5 - 10.0, screen.h * 0.5 - 10.0);
        let cell = Rect::new(screen.x + 5.0 + a[0] * (screen.w - 10.0 - cw), screen.y + 5.0 + a[1] * (screen.h - 10.0 - ch), cw, ch);
        ui.p().rounded(cell, 3.0, ACCENT);
    }
    c.y += 74.0;
    let left = c.used();
    // updates from the GitHub releases (see `omsi_platform::updater`)
    let mut c = Col::new(ui, cols[1], "Updates");
    toggle_setting(ui, s, dirty, c.row(), "Look for updates when the launcher starts", "update_check");
    toggle_setting(ui, s, dirty, c.row(), "Install updates without asking", "update_auto");
    toggle_setting(ui, s, dirty, c.row(), "Tell me about a new version during a session", "update_notify");
    toggle_setting(ui, s, dirty, c.row(), "Count me in the website's \"playing now\" (anonymous)", "presence");
    {
        use omsi_platform::updater::Status;
        let r = c.row();
        let busy = matches!(out.update, Status::Checking | Status::Downloading { .. } | Status::Installing(_) | Status::WaitingForInstaller(_) | Status::Restarting(_));
        if ui.button("s-upd-check", Rect::new(r.x, r.y, 150.0, r.h), if busy { "Checking..." } else { "Check now" }, Some("refresh"), ButtonKind::Normal) && !busy {
            out.check_updates = true;
        }
        let text = match &out.update {
            Status::UpToDate if omsi_platform::updater::is_test_build(omsi_platform::updater::current_version()) => format!("{} is a test build: it is not updated", omsi_platform::updater::current_version()),
            Status::UpToDate => format!("{} is the latest version", omsi_platform::updater::current_version()),
            Status::Available(rel) => format!("{} is available", rel.version),
            Status::Failed(_) => "The last check failed".to_string(),
            _ => format!("This is openOMSI {}", omsi_platform::updater::current_version()),
        };
        ui.text_in(&text, Rect::new(r.x + 162.0, r.y, r.w - 162.0, r.h), 12.5, omsi_ui::Weight::Regular, TEXT_DIM, omsi_ui::paint::Align::Left);
    }
    if ui.button("s-upd-github", c.row(), "github.com/openOmsi-project/openOMSI", Some("open_in_new"), ButtonKind::Ghost) {
        omsi_platform::updater::open_url(omsi_platform::updater::REPO_URL);
    }
    // every setting at once: here at the end, not first on the page where it was the
    // control one saw before any other
    c.section(ui, "Reset");
    if ui.button("s-reset", c.row(), "Reset all settings...", Some("restart_alt"), ButtonKind::Danger) {
        out.reset = true;
    }
    [left, c.used()]
}

fn mb(v: i64) -> String {
    if v >= 1000 {
        format!("{:.1} GB", v as f64 / 1000.0)
    } else {
        format!("{v} MB")
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod settings_tests;
