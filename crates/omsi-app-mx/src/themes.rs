//! Interface themes: a look of its own for the game's screens - the colours of the menus, the
//! panels as frosted glass over the picture, the loading screen's logo and how it shows the
//! progress, the logo at the top of the pause menu - from a theme file (docs/THEMES.md).
//!
//! A theme is a folder with a `theme.json`: `Themes/<id>/` of a content root, or a plugin's own
//! folder (`plugins/<name>/theme.json`, its id the plugin's name). The player chooses one in
//! Options › Interface (`Settings::ui_theme_pack`); a plugin may switch to one (`omsi.ui.set_theme`).
//! A content root another machine sent (a LAN host's mods) brings none.

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use omsi_ui::icons::{Mark, Stage};
use omsi_ui::theme::{self, Glass, Palette};
use omsi_ui::Color;

/// openOMSI's own look as the settings file writes it (the setting reads it as "").
pub(crate) const OWN: &str = "openomsi";
/// The longest theme file, SVG logo or picture that is read (bytes).
const MAX_FILE: u64 = 4 << 20;
/// The widest and highest picture logo (pixels).
const MAX_PICTURE: u32 = 4096;
/// The longest name and title of a theme (characters).
const MAX_NAME: usize = 40;
/// The track under a logo drawn along its path: the whole path, this faint.
const TRACK: f32 = 0.18;

/// How the loading screen shows how far the loading is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Progress {
    /// openOMSI's striped bar along the bottom, in the theme's accent.
    #[default]
    Bar,
    /// The logo's path `progress` drawn along as far as the loading is (an SVG logo).
    Stroke,
    /// The logo filled from the left as far as the loading is, faint beyond.
    Fill,
}

/// A logo of a theme: an SVG (its source, its size, the length of its path `progress` when it
/// has one, and its marks: what comes in as the loading reaches it) or a picture.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Logo {
    Svg { src: String, size: (f32, f32), progress_len: Option<f32>, marks: Vec<Mark> },
    Picture { w: u32, h: u32, rgba: Arc<Vec<u8>> },
}

impl Logo {
    /// Width over height.
    pub fn aspect(&self) -> f32 {
        let (w, h) = match self {
            Logo::Svg { size, .. } => *size,
            Logo::Picture { w, h, .. } => (*w as f32, *h as f32),
        };
        (w / h.max(1e-3)).clamp(0.05, 20.0)
    }

    /// Its pixels fitted into a square of `n` (the pause menu's logo), centred.
    pub fn square_pixels(&self, n: u32) -> Option<Vec<u8>> {
        let a = self.aspect();
        let (w, h) = if a >= 1.0 { (n, ((n as f32 / a).round() as u32).max(1)) } else { (((n as f32 * a).round() as u32).max(1), n) };
        let px = self.pixels(w, h, None, &|_| Stage::Shown)?;
        let (x0, y0) = ((n - w) / 2, (n - h) / 2);
        let mut out = vec![0u8; (n * n * 4) as usize];
        for y in 0..h {
            let from = (y * w * 4) as usize;
            let to = (((y0 + y) * n + x0) * 4) as usize;
            out[to..to + (w * 4) as usize].copy_from_slice(&px[from..from + (w * 4) as usize]);
        }
        Some(out)
    }

    /// The marks of an SVG logo (`data-at`: what comes in as the loading reaches it).
    pub fn marks(&self) -> &[Mark] {
        match self {
            Logo::Svg { marks, .. } => marks,
            Logo::Picture { .. } => &[],
        }
    }

    /// Its pixels at `w` x `h` (straight RGBA) - with the loading's progress `frac` shown as
    /// `progress` has it (none: the whole logo), its marks as `stage` has them.
    pub fn pixels(&self, w: u32, h: u32, progress: Option<(Progress, f32)>, stage: &dyn Fn(&Mark) -> Stage) -> Option<Vec<u8>> {
        let staged = |src: &str, marks: &[Mark]| if marks.is_empty() { src.to_string() } else { omsi_ui::icons::staged_svg(src, marks, stage) };
        if let (Logo::Svg { src, progress_len: Some(len), marks, .. }, Some((Progress::Stroke, frac))) = (self, progress) {
            return omsi_ui::icons::rasterize_rgba(&omsi_ui::icons::progress_svg(&staged(src, marks), "progress", *len, frac, TRACK), w, h);
        }
        let mut px = match self {
            Logo::Svg { src, marks, .. } => omsi_ui::icons::rasterize_rgba(&staged(src, marks), w, h)?,
            Logo::Picture { w: pw, h: ph, rgba } => {
                let img = image::RgbaImage::from_raw(*pw, *ph, rgba.to_vec())?;
                image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle).into_raw()
            }
        };
        if let Some((_, frac)) = progress {
            // (filled from the left: beyond the progress grey and faint)
            let cut = (w as f32 * frac.clamp(0.0, 1.0)).round() as usize;
            for (i, p) in px.chunks_mut(4).enumerate() {
                if i % w as usize >= cut {
                    let grey = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) as u8;
                    p[..3].fill(grey);
                    p[3] = (p[3] as f32 * TRACK) as u8;
                }
            }
        }
        Some(px)
    }
}

/// The loading screen of a theme.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct Loading {
    pub background: Option<Color>,
    /// The map's name and the tips.
    pub text: Option<Color>,
    /// In the middle, in place of openOMSI's wordmark.
    pub logo: Option<Logo>,
    pub progress: Progress,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Theme {
    pub id: String,
    pub name: String,
    /// The colours it gives (by name), over openOMSI's light or dark look (`palette`).
    pub colors: Vec<(String, Color)>,
    /// The corners of its panels (none: openOMSI's).
    pub radius: Option<f32>,
    pub glass: Option<Glass>,
    pub loading: Loading,
    /// The logo at the top of the pause menu, and the name beside it (openOMSI's version
    /// stays under it).
    pub logo: Option<Logo>,
    pub title: Option<String>,
}

impl Theme {
    /// Its palette over openOMSI's light or dark look, as the player chose (`ui_theme`): its
    /// colours, what it leaves out from that look, its corners and its glass.
    pub fn palette(&self, light: bool) -> Palette {
        let (mut p, _) = Palette::with_colors(theme::palette(light), self.colors.iter().map(|(k, c)| (k.as_str(), *c)));
        p.radius = self.radius.unwrap_or(p.radius);
        p.glass = self.glass;
        p
    }
}

/// A theme file read: `text` of the folder `dir`, the theme `id`. What it gets wrong it says;
/// what it does not know is logged and left out.
pub(crate) fn parse(id: &str, dir: &Path, text: &str) -> Result<Theme, String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("theme.json: {e}"))?;
    let o = v.as_object().ok_or("theme.json: an object is expected")?;
    let known = ["name", "title", "colors", "radius", "glass", "loading", "logo", "author", "version", "description"];
    for k in o.keys().filter(|k| !known.contains(&k.as_str())) {
        log::warn!("interface theme \"{id}\": \"{k}\" is not known, left out");
    }
    let name = match o.get("name") {
        Some(n) => text_of(n, "name")?,
        None => id.to_string(),
    };
    let mut colors: Vec<(String, Color)> = Vec::new();
    let mut known = Palette::OPENOMSI;
    if let Some(c) = o.get("colors") {
        for (k, val) in c.as_object().ok_or("colors: an object of colours is expected")? {
            let c = color_of(val).ok_or_else(|| format!("colors.{k}: \"#rrggbb\", \"#rrggbbaa\" or [r, g, b, a] is expected"))?;
            if known.color_mut(k).is_some() {
                colors.push((k.clone(), c));
            } else {
                log::warn!("interface theme \"{id}\": colors.{k} is not known, left out");
            }
        }
    }
    let radius = match o.get("radius") {
        Some(r) => Some(r.as_f64().filter(|r| (0.0..=24.0).contains(r)).ok_or("radius: 0 to 24 is expected")? as f32),
        None => None,
    };
    let glass = match o.get("glass") {
        None | Some(serde_json::Value::Bool(false)) | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Bool(true)) => Some(Glass::DEFAULT),
        Some(serde_json::Value::Object(g)) => {
            let num = |k: &str, max: f64, default: f32| -> Result<f32, String> {
                match g.get(k) {
                    None => Ok(default),
                    Some(x) => x.as_f64().filter(|x| (0.0..=max).contains(x)).map(|x| x as f32).ok_or(format!("glass.{k}: 0 to {max} is expected")),
                }
            };
            let d = Glass::DEFAULT;
            Some(Glass { opacity: num("opacity", 1.0, d.opacity)?, blur: num("blur", 64.0, d.blur)?, edge: num("edge", 1.0, d.edge)?, highlight: num("highlight", 1.0, d.highlight)? })
        }
        Some(_) => return Err("glass: true, false or an object is expected".into()),
    };
    let mut loading = Loading::default();
    if let Some(l) = o.get("loading") {
        let l = l.as_object().ok_or("loading: an object is expected")?;
        if let Some(c) = l.get("background") {
            loading.background = Some(color_of(c).ok_or("loading.background: a colour is expected")?);
        }
        if let Some(c) = l.get("text") {
            loading.text = Some(color_of(c).ok_or("loading.text: a colour is expected")?);
        }
        if let Some(f) = l.get("logo") {
            loading.logo = Some(read_logo(dir, &text_of(f, "loading.logo")?)?);
        }
        loading.progress = match l.get("progress").and_then(|p| p.as_str()).unwrap_or("bar") {
            "bar" => Progress::Bar,
            "stroke" => Progress::Stroke,
            "fill" => Progress::Fill,
            other => return Err(format!("loading.progress: \"bar\", \"stroke\" or \"fill\" is expected, not \"{other}\"")),
        };
        match (&loading.logo, loading.progress) {
            (None, Progress::Stroke | Progress::Fill) => return Err("loading.progress: a logo (loading.logo) is needed to show the progress".into()),
            // (a logo without the path to draw along: filled instead)
            (Some(Logo::Picture { .. } | Logo::Svg { progress_len: None, .. }), Progress::Stroke) => {
                log::warn!("interface theme \"{id}\": the loading logo has no path with id=\"progress\": it is filled from the left instead");
                loading.progress = Progress::Fill;
            }
            _ => {}
        }
    }
    let logo = o.get("logo").map(|f| text_of(f, "logo").and_then(|f| read_logo(dir, &f))).transpose()?;
    let title = o.get("title").map(|t| text_of(t, "title")).transpose()?;
    Ok(Theme { id: id.to_string(), name, colors, radius, glass, loading, logo, title })
}

/// A name or a file of the theme file: a text, on one line, not too long.
fn text_of(v: &serde_json::Value, what: &str) -> Result<String, String> {
    let t = v.as_str().ok_or(format!("{what}: a text is expected"))?.trim();
    if t.is_empty() || t.chars().count() > 200 || t.chars().any(char::is_control) {
        return Err(format!("{what}: one line of text is expected"));
    }
    Ok(if what == "name" || what == "title" { t.chars().take(MAX_NAME).collect() } else { t.to_string() })
}

/// A colour of the theme file: `"#rrggbb"`, `"#rrggbbaa"` or `[r, g, b]` / `[r, g, b, a]`
/// (0 to 255, the opacity 0 to 1).
fn color_of(v: &serde_json::Value) -> Option<Color> {
    match v {
        serde_json::Value::String(s) => theme::parse_hex(s),
        serde_json::Value::Array(a) if a.len() == 3 || a.len() == 4 => {
            let n: Vec<f64> = a.iter().map(|x| x.as_f64()).collect::<Option<_>>()?;
            let byte = |x: f64| (0.0..=255.0).contains(&x).then_some(x.round() as u8);
            let alpha = n.get(3).copied().unwrap_or(1.0);
            (0.0..=1.0).contains(&alpha).then_some(())?;
            Some(Color::rgba(byte(n[0])?, byte(n[1])?, byte(n[2])?, alpha as f32))
        }
        _ => None,
    }
}

/// The logo `file` of the theme's folder `dir`: an SVG or a picture (PNG, TGA, BMP, JPEG,
/// DDS). Only a file in the folder (or below it) is read.
fn read_logo(dir: &Path, file: &str) -> Result<Logo, String> {
    let rel = Path::new(file);
    if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(format!("{file}: a file in the theme's folder is expected"));
    }
    let path = dir.join(rel);
    let len = std::fs::metadata(&path).map_err(|e| format!("{file}: {e}"))?.len();
    if len > MAX_FILE {
        return Err(format!("{file}: larger than {} MB", MAX_FILE >> 20));
    }
    if file.to_ascii_lowercase().ends_with(".svg") {
        let src = std::fs::read_to_string(&path).map_err(|e| format!("{file}: {e}"))?;
        let size = omsi_ui::icons::svg_size(&src).filter(|s| s.0 > 0.0 && s.1 > 0.0).ok_or(format!("{file}: not an SVG that can be drawn"))?;
        let progress_len = omsi_ui::icons::path_length(&src, "progress");
        let marks = omsi_ui::icons::marks(&src);
        return Ok(Logo::Svg { src, size, progress_len, marks });
    }
    let fits = |w: u32, h: u32| (1..=MAX_PICTURE).contains(&w) && (1..=MAX_PICTURE).contains(&h);
    let too_large = || format!("{file}: 1 to {MAX_PICTURE} pixels wide and high is expected");
    // (its size from its header first: a small file can hold a picture far larger than that)
    let header = image::ImageReader::open(&path).ok().and_then(|r| r.with_guessed_format().ok()).and_then(|r| r.into_dimensions().ok());
    if header.is_some_and(|(w, h)| !fits(w, h)) {
        return Err(too_large());
    }
    let img = omsi_texture::decode_file(&path).map_err(|e| format!("{file}: {e}"))?;
    if !fits(img.width, img.height) {
        return Err(too_large());
    }
    Ok(Logo::Picture { w: img.width, h: img.height, rgba: Arc::new(img.rgba) })
}

/// A theme's id is the name of its folder: letters, digits, `-`, `_` and `.`.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_NAME && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) && !id.eq_ignore_ascii_case("openomsi")
}

/// Every theme there is, (id, folder): the content roots' `Themes` folders first, then the
/// plugins'; of two of one id (in any case) the first.
pub(crate) fn found() -> Vec<(String, PathBuf)> {
    let roots: Vec<PathBuf> = omsi_cfg::content_roots().iter().filter(|r| !omsi_cfg::is_sandbox(r)).cloned().collect();
    found_in(&roots)
}

fn found_in(roots: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for folder in ["themes", "plugins"] {
        for root in roots {
            let Some(dir) = omsi_plugin::resolve_path(root, folder).filter(|d| d.is_dir()) else { continue };
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.join("theme.json").is_file()).collect();
            dirs.sort();
            for d in dirs {
                let Some(id) = d.file_name().and_then(|n| n.to_str()).filter(|id| valid_id(id)).map(str::to_string) else { continue };
                if !out.iter().any(|(o, _)| o.eq_ignore_ascii_case(&id)) {
                    out.push((id, d));
                }
            }
        }
    }
    out
}

/// The theme `id` of `themes`, read.
fn load_from(themes: &[(String, PathBuf)], id: &str) -> Result<Theme, String> {
    let (id, dir) = themes.iter().find(|(t, _)| t.eq_ignore_ascii_case(id)).ok_or("there is no theme of that id")?;
    let file = dir.join("theme.json");
    let len = std::fs::metadata(&file).map_err(|e| e.to_string())?.len();
    if len > MAX_FILE {
        return Err("theme.json is too large".into());
    }
    let text = std::fs::read_to_string(&file).map_err(|e| format!("theme.json: {e}"))?;
    parse(id, dir, &text)
}

/// The value of the setting `ui_theme_pack` as read: one line, "" for openOMSI's own look.
pub(crate) fn setting(v: &str) -> String {
    let v = v.trim();
    if v.eq_ignore_ascii_case("openomsi") || v.eq_ignore_ascii_case("default") || !valid_id(v) {
        return String::new();
    }
    v.to_string()
}

struct State {
    wanted: String,
    light: bool,
    theme: Option<Arc<Theme>>,
}

static STATE: Mutex<State> = Mutex::new(State { wanted: String::new(), light: false, theme: None });

/// Puts the theme `id` (the setting `ui_theme_pack`) in force over openOMSI's light or dark
/// look (`ui_theme`), when they are not already: the theme read anew when it is another, the
/// palette for the screens. A theme that cannot be read leaves openOMSI's look, with a line in
/// the log why. Cheap to call every frame.
pub(crate) fn sync(id: &str, light: bool) {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if s.wanted == id && s.light == light {
        return;
    }
    if s.wanted != id {
        s.wanted = id.to_string();
        s.theme = None;
        if !id.is_empty() {
            match load_from(&found(), id) {
                Ok(t) => {
                    log::info!("interface theme \"{}\" ({}): glass {}, loading {:?}", t.id, t.name, t.glass.is_some(), t.loading.progress);
                    s.theme = Some(Arc::new(t));
                }
                Err(e) => log::warn!("interface theme \"{id}\": {e} - openOMSI's own look"),
            }
        }
    }
    s.light = light;
    let palette = s.theme.as_ref().map_or(theme::palette(light), |t| t.palette(light));
    theme::set(palette);
}

/// The palette of the theme in force over openOMSI's light or dark look: what `sync` puts in
/// force, for what is built from the settings rather than drawn (the game's own panels).
pub(crate) fn palette(light: bool) -> Palette {
    current().map_or(theme::palette(light), |t| t.palette(light))
}

/// The theme in force (none: openOMSI's own look).
pub(crate) fn current() -> Option<Arc<Theme>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).theme.clone()
}

/// The themes the player can choose, (id, name): openOMSI's own look first. Looked for once.
pub(crate) fn choices() -> &'static [(String, String)] {
    static LIST: OnceLock<Vec<(String, String)>> = OnceLock::new();
    LIST.get_or_init(|| {
        let mut out = vec![(String::new(), "openOMSI".to_string())];
        for (id, dir) in found() {
            // (its name, without reading its pictures)
            let name = std::fs::read_to_string(dir.join("theme.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| v.get("name").and_then(|n| n.as_str()).map(|n| n.trim().chars().take(MAX_NAME).collect::<String>())).filter(|n| !n.is_empty()).unwrap_or_else(|| id.clone());
            out.push((id, name));
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("openomsi-themes-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const LOGO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 200"><path id="progress" d="M100 20 A80 80 0 1 0 180 100 L380 100" fill="none" stroke="#2a75f7" stroke-width="16"/></svg>"##;

    /// A theme of a few colours, glass and a logo that is the loading bar.
    #[test]
    fn a_theme_is_read_whole() {
        let dir = folder("whole");
        std::fs::write(dir.join("loading.svg"), LOGO).unwrap();
        std::fs::write(dir.join("mark.svg"), LOGO).unwrap();
        let text = r##"{
            "name": "Omsi-Hub", "title": "Omsi-Hub",
            "colors": { "accent": "#f58620", "background": "#0a142e", "surface": [16, 30, 62], "ink": "#ffffff" },
            "radius": 14,
            "glass": { "opacity": 0.55, "blur": 28 },
            "loading": { "background": "#07101f", "text": "#c8d2e6", "logo": "loading.svg", "progress": "stroke" },
            "logo": "mark.svg",
            "sparkle": true
        }"##;
        let t = parse("omsihub", &dir, text).unwrap();
        assert_eq!((t.name.as_str(), t.title.as_deref()), ("Omsi-Hub", Some("Omsi-Hub")));
        assert_eq!(t.palette(false).accent, Color::rgba(0xf5, 0x86, 0x20, 1.0));
        assert_eq!((t.palette(false).surface, t.palette(false).panel), (Color::rgba(16, 30, 62, 1.0), Color::rgba(16, 30, 62, 1.0)));
        assert_eq!(t.palette(false).radius, 14.0);
        assert_eq!(t.palette(false).glass, Some(Glass { opacity: 0.55, blur: 28.0, ..Glass::DEFAULT }));
        assert_eq!(t.loading.progress, Progress::Stroke);
        assert_eq!(t.loading.background, Some(Color::rgba(7, 16, 31, 1.0)));
        let logo = t.loading.logo.as_ref().unwrap();
        assert_eq!(logo.aspect(), 2.0);
        // the loading bar: drawn along the path as far as the loading is
        let half = logo.pixels(200, 100, Some((Progress::Stroke, 0.5)), &|_| Stage::Shown).unwrap();
        let whole = logo.pixels(200, 100, None, &|_| Stage::Shown).unwrap();
        let ink = |px: &[u8]| px.chunks(4).filter(|p| p[3] > 128).count();
        assert!(ink(&half) > ink(&whole) / 4 && ink(&half) < ink(&whole), "{} of {}", ink(&half), ink(&whole));
        // filled from the left: the right half faint
        let fill = logo.pixels(200, 100, Some((Progress::Fill, 0.5)), &|_| Stage::Shown).unwrap();
        assert!(fill.chunks(4).enumerate().filter(|(i, _)| i % 200 >= 100).all(|(_, p)| p[3] <= 50));
        assert!(t.logo.is_some());
    }

    /// What a theme file gets wrong is said, and nothing outside its folder is read.
    #[test]
    fn a_wrong_theme_says_why() {
        let dir = folder("wrong");
        let bad = |text: &str| parse("x", &dir, text).unwrap_err();
        assert!(bad("[]").contains("object"));
        assert!(bad(r#"{"colors": {"accent": "orange"}}"#).contains("colors.accent"));
        assert!(bad(r#"{"radius": 80}"#).contains("radius"));
        assert!(bad(r#"{"glass": {"opacity": 2}}"#).contains("glass.opacity"));
        assert!(bad(r#"{"loading": {"progress": "stroke"}}"#).contains("logo"));
        assert!(bad(r#"{"loading": {"progress": "spin"}}"#).contains("spin"));
        assert!(bad(r#"{"logo": "../../secret.svg"}"#).contains("theme's folder"));
        assert!(bad(r#"{"logo": "/etc/x.svg"}"#).contains("theme's folder"));
        assert!(bad(r#"{"logo": "missing.svg"}"#).contains("missing.svg"));
        // (a picture too large is told by its header, before it is decoded)
        image::RgbaImage::new(5000, 1).save(dir.join("wide.png")).unwrap();
        assert!(bad(r#"{"logo": "wide.png"}"#).contains("4096"));
        // (a stroke without the path to draw along: filled; no name: the id)
        std::fs::write(dir.join("plain.svg"), r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10"/></svg>"#).unwrap();
        let t = parse("x", &dir, r#"{"loading": {"logo": "plain.svg", "progress": "stroke"}, "glass": true}"#).unwrap();
        assert_eq!((t.name.as_str(), t.loading.progress, t.palette(false).glass), ("x", Progress::Fill, Some(Glass::DEFAULT)));
        assert_eq!(t.palette(false).accent, Palette::OPENOMSI.accent);
    }

    /// Themes are found in the content roots' `Themes` folders and in the plugins' folders;
    /// the first of an id wins.
    #[test]
    fn themes_are_found_in_themes_and_plugins() {
        let root = folder("roots");
        let a = root.join("a");
        let b = root.join("b");
        for (dir, json) in [(a.join("Themes/Night"), r#"{"name": "Night"}"#), (b.join("Themes/night"), r#"{"name": "Other"}"#), (b.join("plugins/omsihub"), r##"{"name": "Omsi-Hub", "colors": {"accent": "#f58620"}}"##), (b.join("plugins/plain"), "")] {
            std::fs::create_dir_all(&dir).unwrap();
            if !json.is_empty() {
                std::fs::write(dir.join("theme.json"), json).unwrap();
            }
        }
        std::fs::create_dir_all(b.join("Themes/bad id!")).unwrap();
        std::fs::write(b.join("Themes/bad id!/theme.json"), "{}").unwrap();
        let found = found_in(&[a.clone(), b.clone()]);
        let ids: Vec<&str> = found.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["Night", "omsihub"]);
        let t = load_from(&found, "OMSIHUB").unwrap();
        assert_eq!((t.id.as_str(), t.palette(false).accent), ("omsihub", Color::rgba(0xf5, 0x86, 0x20, 1.0)));
        assert!(load_from(&found, "plain").is_err());
        // the setting: one id, openOMSI's own look by any of its names
        assert_eq!(setting(" omsihub "), "omsihub");
        for own in ["", "openOMSI", "default", "a b", "../x"] {
            assert_eq!(setting(own), "", "{own}");
        }
    }

    /// The options' row for the theme is in the player's language.
    #[test]
    fn the_theme_row_is_translated() {
        let desc = "The look of the menus, the panels and the loading screen: openOMSI's own, or one a mod or plugin brings";
        crate::with_test_language("nl", || {
            assert_eq!(omsi_ui::tr("Theme pack"), "Themapakket");
            assert!(omsi_ui::tr(desc).starts_with("Hoe de menu's"));
        });
        crate::with_test_language("de", || assert_eq!(omsi_ui::tr("Theme pack"), "Designpaket"));
    }
}
