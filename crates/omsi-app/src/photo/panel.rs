//! The photo mode's panel, in the launcher's toolkit: the photo across the window, the
//! composition guides over it, and a panel at the left with five pages - the camera, the
//! lens, the colour, the effects and the scene - the progress of the shot and the buttons
//! that take it and leave.

use glam::Vec2;
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

use super::develop::Filter;
use super::{lens, Photo, Request, ASPECTS, BLADES, FSTOPS, GRIDS, QUALITY, RANGES, SHUTTERS, SIZES};
use crate::launcher::theme::*;
use crate::launcher::ui::{ButtonKind, Ui};
use crate::shell::{Action, Shell};

pub(crate) const TABS: [&str; 5] = ["Camera", "Lens", "Colour", "Effects", "Scene"];

const PANEL_W: f32 = 372.0;

/// What the panel shows of the game.
pub(crate) struct Info {
    /// The clock (HH:MM).
    pub time: String,
    /// The bus moves (a rolling shot can blur the street).
    pub bus_moving: bool,
}

/// A slider over a list of `n` values, by their place: its text from `label`.
fn stepped(ui: &mut Ui, name: &str, r: Rect, at: &mut usize, n: usize, label: &str, text: &dyn Fn(usize) -> String) -> bool {
    let mut v = *at as f32;
    let changed = ui.slider(name, r, &mut v, 0.0, (n.max(1) - 1) as f32, 1.0, label, &|x| text(x.round() as usize));
    let k = (v.round() as usize).min(n.saturating_sub(1));
    if k != *at {
        *at = k;
        return true;
    }
    changed && false
}

/// A select of `labels` in a row, its name on the left as the launcher's settings have it.
fn select_row(ui: &mut Ui, name: &str, r: Rect, label: &str, at: &mut usize, labels: &[&str]) {
    ui.label(Rect::new(r.x, r.y, r.w * 0.38, r.h), label);
    let options: Vec<String> = labels.iter().map(|l| omsi_ui::tr(l).into_owned()).collect();
    let mut k = (*at).min(options.len().saturating_sub(1));
    if ui.select(name, Rect::new(r.x + r.w * 0.38, r.y, r.w * 0.62, r.h), &mut k, &options) {
        *at = k;
    }
}

/// A signed setting (-1..1) as -100..100.
fn signed(ui: &mut Ui, name: &str, r: Rect, v: &mut f32, label: &str) {
    let mut x = *v * 100.0;
    if ui.slider(name, r, &mut x, -100.0, 100.0, 1.0, label, &|x| if x.abs() < 0.5 { "0".into() } else { format!("{x:+.0}") }) {
        *v = x / 100.0;
    }
}

/// A setting from 0 to 1 as 0..100.
fn amount(ui: &mut Ui, name: &str, r: Rect, v: &mut f32, label: &str) {
    let mut x = *v * 100.0;
    if ui.slider(name, r, &mut x, 0.0, 100.0, 1.0, label, &|x| format!("{x:.0}")) {
        *v = x / 100.0;
    }
}

/// The rows of the open page, from `y` down; returns where they end.
fn page(ui: &mut Ui, ph: &mut Photo, info: &Info, x: f32, mut y: f32, w: f32, acts: &mut Vec<Request>) -> f32 {
    let row = |y: &mut f32| {
        let r = Rect::new(x, *y, w, ROW - 2.0);
        *y += ROW + 4.0;
        r
    };
    let section = |ui: &mut Ui, y: &mut f32, title: &str| {
        *y += 6.0;
        ui.heading(Rect::new(x, *y, w, 26.0), title, None);
        *y += 30.0;
    };
    let s = &mut ph.settings;
    match ph.tab {
        0 => {
            section(ui, &mut y, "Camera");
            let mut mode = s.orbit as usize;
            ui.label(Rect::new(x, y, w * 0.38, ROW - 2.0), "Movement");
            if ui.segmented("ph-mode", Rect::new(x + w * 0.38, y, w * 0.62, ROW - 2.0), &mut mode, &["Free", "Orbit"]) {
                s.orbit = mode == 1;
            }
            y += ROW + 4.0;
            // the lens's focal length, on a log scale from 10 to 400 mm
            let mm = lens::focal_mm(s.fov);
            let mut t = (mm / 10.0).ln() / 40f32.ln();
            let fov = s.fov;
            if ui.slider("ph-focal", row(&mut y), &mut t, 0.0, 1.0, 0.0, "Focal length", &|t| format!("{:.0} mm", 10.0 * 40f32.powf(t))) {
                let mm = 10.0 * 40f32.powf(t.clamp(0.0, 1.0));
                s.fov = lens::fov_for(mm);
                ph.fov_target = s.fov;
            }
            let _ = fov;
            ui.slider("ph-roll", row(&mut y), &mut s.roll, -45.0, 45.0, 0.5, "Roll", &|v| format!("{v:.1}°"));
            ui.slider("ph-speed", row(&mut y), &mut s.speed, 0.1, 4.0, 0.05, "Speed", &|v| format!("{v:.2}x"));
            let labels: Vec<&str> = RANGES.iter().map(|r| r.1).collect();
            select_row(ui, "ph-range", row(&mut y), "Distance from the bus", &mut s.range, &labels);
            section(ui, &mut y, "Frame");
            let labels: Vec<&str> = ASPECTS.iter().map(|a| a.1).collect();
            select_row(ui, "ph-aspect", row(&mut y), "Aspect ratio", &mut s.aspect, &labels);
            select_row(ui, "ph-grid", row(&mut y), "Guides", &mut s.grid, &GRIDS);
        }
        1 => {
            section(ui, &mut y, "Depth of field");
            ui.toggle("ph-dof", row(&mut y), &mut s.dof, "Depth of field");
            let mut auto = (!s.auto_focus) as usize;
            ui.label(Rect::new(x, y, w * 0.38, ROW - 2.0), "Focus");
            if ui.segmented("ph-af", Rect::new(x + w * 0.38, y, w * 0.62, ROW - 2.0), &mut auto, &["On the bus", "Manual"]) {
                s.auto_focus = auto == 0;
            }
            y += ROW + 4.0;
            if s.auto_focus {
                let r = row(&mut y);
                ui.label(Rect::new(r.x, r.y, r.w * 0.5, r.h), "Focus distance");
                let d = ph.focus_distance();
                let s = &mut ph.settings;
                let _ = s;
                ui.text_in(&format!("{d:.1} m"), Rect::new(r.x + r.w * 0.5, r.y, r.w * 0.5, r.h), 12.5, Weight::Medium, TEXT, Align::Right);
            } else {
                // 0.3 .. 300 m on a log scale
                let mut t = (s.focus / 0.3).ln() / 1000f32.ln();
                if ui.slider("ph-focus", row(&mut y), &mut t, 0.0, 1.0, 0.0, "Focus distance", &|t| {
                    let m = 0.3 * 1000f32.powf(t);
                    if m < 10.0 { format!("{m:.1} m") } else { format!("{m:.0} m") }
                }) {
                    s.focus = 0.3 * 1000f32.powf(t.clamp(0.0, 1.0));
                }
            }
            let s = &mut ph.settings;
            stepped(ui, "ph-fstop", row(&mut y), &mut s.fstop, FSTOPS.len(), "Aperture", &|k| format!("f/{}", FSTOPS[k]));
            let labels: Vec<&str> = BLADES.iter().map(|b| b.1).collect();
            select_row(ui, "ph-blades", row(&mut y), "Bokeh", &mut s.blades, &labels);
            section(ui, &mut y, "Shutter");
            ui.toggle("ph-motion", row(&mut y), &mut s.motion, "Motion blur (rolling shot)");
            stepped(ui, "ph-shutter", row(&mut y), &mut s.shutter, SHUTTERS.len(), "Shutter speed", &|k| SHUTTERS[k].1.to_string());
            if s.motion && !info.bus_moving {
                let r = row(&mut y);
                ui.text_in("The bus stands still: nothing to blur.", r, 12.0, Weight::Regular, WARN, Align::Left);
            }
            section(ui, &mut y, "Quality");
            let labels: Vec<&str> = QUALITY.iter().map(|q| q.1).collect();
            select_row(ui, "ph-quality", row(&mut y), "Samples", &mut s.quality, &labels);
        }
        2 => {
            section(ui, &mut y, "Light");
            let g = &mut s.grade;
            ui.slider("ph-ev", row(&mut y), &mut g.exposure, -3.0, 3.0, 0.05, "Exposure", &|v| if v.abs() < 0.025 { "0 EV".into() } else { format!("{v:+.1} EV") });
            signed(ui, "ph-contrast", row(&mut y), &mut g.contrast, "Contrast");
            signed(ui, "ph-high", row(&mut y), &mut g.highlights, "Highlights");
            signed(ui, "ph-shadow", row(&mut y), &mut g.shadows, "Shadows");
            section(ui, &mut y, "Colour");
            signed(ui, "ph-sat", row(&mut y), &mut g.saturation, "Saturation");
            signed(ui, "ph-vib", row(&mut y), &mut g.vibrance, "Vibrance");
            signed(ui, "ph-temp", row(&mut y), &mut g.temperature, "Temperature");
            signed(ui, "ph-tint", row(&mut y), &mut g.tint, "Tint");
        }
        3 => {
            section(ui, &mut y, "Film");
            let g = &mut s.grade;
            let mut k = Filter::ALL.iter().position(|f| *f == g.filter).unwrap_or(0);
            let labels: Vec<&str> = Filter::ALL.iter().map(|f| f.name()).collect();
            select_row(ui, "ph-filter", row(&mut y), "Filter", &mut k, &labels);
            g.filter = Filter::ALL[k.min(Filter::ALL.len() - 1)];
            amount(ui, "ph-fstrength", row(&mut y), &mut g.filter_strength, "Strength");
            amount(ui, "ph-grain", row(&mut y), &mut g.grain, "Film grain");
            section(ui, &mut y, "Lens");
            amount(ui, "ph-vignette", row(&mut y), &mut g.vignette, "Vignette");
            amount(ui, "ph-ca", row(&mut y), &mut g.aberration, "Colour fringes");
            amount(ui, "ph-sharp", row(&mut y), &mut g.sharpen, "Sharpen");
        }
        _ => {
            section(ui, &mut y, "Time of day");
            let r = row(&mut y);
            ui.label(Rect::new(r.x, r.y, r.w * 0.38, r.h), "Clock");
            ui.text_in(&info.time, Rect::new(r.x + r.w * 0.38, r.y, r.w * 0.62, r.h), 15.0, Weight::Bold, accent(), Align::Center);
            let r = row(&mut y);
            let bw = (r.w - 18.0) / 4.0;
            for (k, (text, secs)) in [("-1 h", -3600.0), ("-15 min", -900.0), ("+15 min", 900.0), ("+1 h", 3600.0)].into_iter().enumerate() {
                if ui.button(&format!("ph-clock-{k}"), Rect::new(r.x + k as f32 * (bw + 6.0), r.y, bw, r.h), text, None, ButtonKind::Normal) {
                    acts.push(Request::Clock(secs));
                }
            }
            section(ui, &mut y, "Saving");
            let labels: Vec<&str> = SIZES.iter().map(|z| z.1).collect();
            select_row(ui, "ph-size", row(&mut y), "Photo size", &mut s.size, &labels);
            let r = Rect::new(x, y, w, 40.0);
            ui.paragraph("Photos are saved as PNG into the Screenshots folder of the game.", Vec2::new(r.x, r.y), r.w, 12.0, Weight::Regular, TEXT_DIM);
            y += 44.0;
        }
    }
    y
}

/// The guides over the frame `f` (window points).
fn guides(ui: &mut Ui, f: Rect, kind: usize) {
    let c = Color::WHITE.alpha(0.45);
    let line_v = |ui: &mut Ui, x: f32| ui.p().rect(Rect::new(x - 0.5, f.y, 1.0, f.h), c);
    let line_h = |ui: &mut Ui, y: f32| ui.p().rect(Rect::new(f.x, y - 0.5, f.w, 1.0), c);
    match kind {
        1 => {
            for k in [1.0 / 3.0, 2.0 / 3.0] {
                line_v(ui, f.x + f.w * k);
                line_h(ui, f.y + f.h * k);
            }
        }
        2 => {
            let g = 0.381_966;
            for k in [g, 1.0 - g] {
                line_v(ui, f.x + f.w * k);
                line_h(ui, f.y + f.h * k);
            }
        }
        3 => {
            let m = f.center();
            ui.p().rect(Rect::new(m.x - 14.0, m.y - 0.5, 28.0, 1.0), c);
            ui.p().rect(Rect::new(m.x - 0.5, m.y - 14.0, 1.0, 28.0), c);
            line_h(ui, m.y);
        }
        _ => {}
    }
}

pub(crate) fn draw(sh: &mut Shell, ph: &mut Photo, info: &Info) {
    let mut acts: Vec<Request> = Vec::new();
    let size = sh.ui.size;
    let full = Rect::new(0.0, 0.0, size.x, size.y);
    // the photo itself (while the camera moves the window's live picture is under the panel)
    if let (Some(tex), true) = (sh.picture_tex, ph.render.showing_photo()) {
        sh.ui.image(full, tex, 0.0);
    }
    // the frame the photo is cut to, and its guides
    let (fx, fy, fw, fh) = super::develop::crop_rect((size.x * 100.0) as u32, (size.y * 100.0) as u32, ph.aspect());
    let frame = Rect::new(fx as f32 / 100.0, fy as f32 / 100.0, fw as f32 / 100.0, fh as f32 / 100.0);
    guides(&mut sh.ui, frame, ph.settings.grid);
    let ui = &mut sh.ui;
    // what was done, top right: where the photo went - a click shows it in the file browser
    if let Some((text, left)) = ph.note.clone() {
        let path = ph.saved.clone().filter(|_| !crate::platform::MOBILE);
        let place = path.as_ref().map(|p| shown_path(p));
        let mut w = ui.width(&text, 13.0, Weight::Medium) + 48.0;
        if let Some(place) = place.as_ref() {
            w = w.max(ui.width(place, 11.5, Weight::Regular) + 76.0);
        }
        let w = w.min(size.x - 40.0);
        let r = Rect::new(size.x - w - 20.0, 20.0, w, if place.is_some() { 58.0 } else { 40.0 });
        let (hover, _, clicked) = if path.is_some() { ui.interact(crate::launcher::ui::id_of("ph-saved"), r) } else { (false, false, false) };
        ui.solid(r);
        // (it stays while the mouse is on it)
        let left = if hover { left.max(1.0) } else { left };
        if let Some(n) = ph.note.as_mut() {
            n.1 = left;
        }
        let a = left.clamp(0.0, 1.0);
        // (flat, as the launcher's toasts: lighter under the mouse, no edge)
        let bg = if hover { SURFACE_3.alpha(a) } else { SURFACE_2.alpha(a) };
        ui.p().rounded(r, 8.0, bg);
        let failed = text.starts_with(omsi_ui::tr("Not saved:").as_ref());
        let (icon, tint) = if ph.saved.is_some() { ("check_circle", OK) } else if failed { ("error", DANGER) } else { ("info", TEXT_DIM) };
        ui.icon(icon, Vec2::new(r.x + 20.0, r.y + 20.0), 18.0, tint.alpha(a));
        ui.text_in(&text, Rect::new(r.x + 36.0, r.y, r.w - 44.0, 40.0), 13.0, Weight::Medium, TEXT.alpha(a), Align::Left);
        if let (Some(place), Some(path)) = (place, path) {
            ui.text_in(&place, Rect::new(r.x + 36.0, r.y + 30.0, r.w - 72.0, 18.0), 11.5, Weight::Regular, TEXT_DIM.alpha(a), Align::Left);
            ui.icon("folder_open", Vec2::new(r.right() - 22.0, r.y + 39.0), 16.0, if hover { accent().alpha(a) } else { TEXT_DIM.alpha(a) });
            if clicked {
                acts.push(Request::Reveal(path));
            }
        }
    }
    let (n, target) = ph.render.progress();
    let busy = ph.render.capture;
    if ph.hide_ui {
        if busy {
            ui.progress(Rect::new(size.x * 0.5 - 120.0, size.y - 28.0, 240.0, 4.0), n as f32 / target.max(1) as f32, true);
        }
        sh.actions.extend(acts.into_iter().map(Action::Photo));
        return;
    }
    // the panel
    let r = Rect::new(20.0, 20.0, PANEL_W.min(size.x - 40.0), size.y - 40.0);
    ui.panel(r);
    let inner = Rect::new(r.x + 18.0, r.y + 16.0, r.w - 36.0, r.h - 32.0);
    ui.icon("photo_camera", Vec2::new(inner.x + 12.0, inner.y + 14.0), 20.0, accent());
    ui.text("Photo mode", Vec2::new(inner.x + 32.0, inner.y + 21.0), 18.0, Weight::Bold, TEXT, Align::Left);
    let mm = lens::focal_mm(ph.settings.fov);
    let fnum = FSTOPS[ph.settings.fstop.min(FSTOPS.len() - 1)];
    let exif = if ph.settings.dof { format!("{mm:.0} mm  |  f/{fnum}  |  {:.1} m", ph.focus_distance()) } else { format!("{mm:.0} mm  |  {:.0}°", ph.settings.fov) };
    ui.text_in(&exif, Rect::new(inner.x, inner.y + 32.0, inner.w, 18.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
    let mut tab = ph.tab;
    if ui.segmented("ph-tabs", Rect::new(inner.x, inner.y + 58.0, inner.w, 34.0), &mut tab, &TABS) {
        ph.tab = tab;
    }
    // the pages scroll above the buttons
    let foot = 44.0 + 10.0 + 38.0 + 8.0 + 40.0;
    let body = Rect::new(inner.x, inner.y + 100.0, inner.w, (inner.bottom() - foot - inner.y - 100.0).max(40.0));
    let key = format!("ph-page-{}", ph.tab);
    ui.scroll_area(&key, body, &mut |ui, v| {
        let end = page(ui, ph, info, v.x, v.y, v.w - 8.0, &mut acts);
        end - v.y + 6.0
    });
    // the keys, over the buttons (going down was nowhere to be found)
    let keys = Rect::new(inner.x, inner.bottom() - 44.0 - 46.0 - 40.0, inner.w, 34.0);
    ui.paragraph(&omsi_ui::tr("W A S D: move, Space: up, Ctrl: down, Shift: faster, drag or arrows: look"), Vec2::new(keys.x, keys.y), keys.w, 11.5, Weight::Regular, TEXT_DIM);
    let b = Rect::new(inner.x, inner.bottom() - 44.0 - 46.0, inner.w, 44.0);
    if ui.button("ph-take", b, &if busy { format!("{}  {n} / {target}", omsi_ui::tr("Taking...")) } else { omsi_ui::tr("Take photo").into_owned() }, Some("photo_camera"), ButtonKind::Primary) && !busy {
        acts.push(Request::Take);
    }
    let half = (inner.w - 8.0) / 2.0;
    if ui.button("ph-reset", Rect::new(inner.x, inner.bottom() - 38.0, half, 38.0), &omsi_ui::tr("Reset"), Some("restart_alt"), ButtonKind::Ghost) {
        acts.push(Request::Reset);
    }
    if ui.button("ph-exit", Rect::new(inner.x + half + 8.0, inner.bottom() - 38.0, half, 38.0), &omsi_ui::tr("Exit"), Some("close"), ButtonKind::Normal) {
        acts.push(Request::Exit);
    }
    sh.actions.extend(acts.into_iter().map(Action::Photo));
}

impl crate::App {
    /// The photo mode's panel for this frame.
    pub(crate) fn frame_photo_panel(&mut self, dt: f32) {
        let Some(s) = self.gfx.surface.as_ref() else { return };
        let (w, h) = (s.config.width as f32, s.config.height as f32);
        let dpi = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0);
        let scale = dpi * crate::ui::size_factor(h, dpi, self.settings.ui_scale, self.settings.ui_scale_window);
        let t = self.clock.time;
        let info = Info {
            time: format!("{:02}:{:02}", ((t / 3600.0) as i64).rem_euclid(24), ((t % 3600.0) / 60.0) as i64),
            bus_moving: self.photo.as_ref().is_some_and(|p| p.bus_velocity.length() > 0.2),
        };
        let Some(ph) = self.photo.as_mut() else { return };
        self.shell.opaque = ph.render.showing_photo() && self.shell.picture_tex.is_some();
        self.shell.begin(w, h, scale, dt);
        draw(&mut self.shell, ph, &info);
        self.shell.finish();
    }
}

/// A saved file's place as the panel shows it: the home folder as `~`.
fn shown_path(p: &std::path::Path) -> String {
    let full = p.display().to_string();
    match std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        Some(home) if !home.is_empty() => {
            let home = std::path::PathBuf::from(home).display().to_string();
            match full.strip_prefix(&home) {
                Some(rest) => format!("~{rest}"),
                None => full,
            }
        }
        _ => full,
    }
}
