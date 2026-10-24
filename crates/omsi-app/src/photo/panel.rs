//! The photo mode's panel, in the launcher's toolkit: the photo across the window, the
//! composition guides over it, and a panel at the left with five pages - the camera, the
//! lens, the colour, the effects and the scene - the progress of the shot and the buttons
//! that take it and leave.

use glam::Vec2;
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

use super::develop::{Border, Filter};
use super::{exposure, lens, Photo, Request, ASPECTS, BLADES, BRACKETS, FSTOPS, GRIDS, LENS_MAX, MOTIONS, QUALITY, RANGES, SHUTTERS, SIZES};
use omsi_ui::theme::*;
use omsi_ui::kit::{ButtonKind, Ui};
use crate::shell::{Action, Shell};

pub(crate) const TABS: [&str; 5] = ["Camera", "Lens", "Colour", "Effects", "Scene"];
/// The advanced mode's pages.
pub(crate) const PRO_TABS: [&str; 6] = ["Body", "Exposure", "Lens", "Colour", "Develop", "Scene"];

/// The pages of the mode the photo mode is in.
pub(crate) fn tabs(pro: bool) -> &'static [&'static str] {
    if pro { &PRO_TABS } else { &TABS }
}

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

/// How the bus's motion is taken: frozen, panned with or from a tripod.
fn motion_row(ui: &mut Ui, r: Rect, s: &mut super::Settings) {
    let mut k = if !s.motion { 0 } else if s.tripod { 2 } else { 1 };
    ui.label(Rect::new(r.x, r.y, r.w * 0.30, r.h), "Motion");
    if ui.segmented("ph-motion", Rect::new(r.x + r.w * 0.30, r.y, r.w * 0.70, r.h), &mut k, &MOTIONS) {
        s.motion = k > 0;
        s.tripod = k == 2;
    }
}

fn border_row(ui: &mut Ui, r: Rect, g: &mut super::Grade) {
    let mut k = Border::ALL.iter().position(|b| *b == g.border).unwrap_or(0);
    let labels: Vec<&str> = Border::ALL.iter().map(|b| b.name()).collect();
    select_row(ui, "ph-border", r, "Border", &mut k, &labels);
    g.border = Border::ALL[k.min(Border::ALL.len() - 1)];
}

/// A value as a camera shows it: within `[lo, hi]` of `list`, by its place.
fn stepped_in(ui: &mut Ui, name: &str, r: Rect, at: &mut usize, lo: usize, hi: usize, label: &str, text: &dyn Fn(usize) -> String) {
    let mut k = (*at).clamp(lo, hi) - lo;
    stepped(ui, name, r, &mut k, hi - lo + 1, label, &|i| text(i + lo));
    *at = k + lo;
}

/// A setting the camera chose by itself, shown as a camera's display shows it.
fn auto_row(ui: &mut Ui, r: Rect, label: &str, value: &str) {
    ui.label(Rect::new(r.x, r.y, r.w * 0.5, r.h), label);
    ui.text_in(value, Rect::new(r.x + r.w * 0.5, r.y, r.w * 0.5, r.h), 13.0, Weight::Bold, ACCENT, Align::Right);
}

/// The rows of the open page, from `y` down; returns where they end.
fn page(ui: &mut Ui, ph: &mut Photo, info: &Info, x: f32, y: f32, w: f32, acts: &mut Vec<Request>) -> f32 {
    if ph.pro {
        pro_page(ui, ph, info, x, y, w, acts)
    } else {
        simple_page(ui, ph, info, x, y, w, acts)
    }
}

/// The advanced mode's pages: a camera's body, the exposure, the lens, the colour, the
/// darkroom and the scene.
fn pro_page(ui: &mut Ui, ph: &mut Photo, info: &Info, x: f32, mut y: f32, w: f32, acts: &mut Vec<Request>) -> f32 {
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
    let note = |ui: &mut Ui, y: &mut f32, text: &str, c: Color| {
        let r = Rect::new(x, *y, w, 34.0);
        ui.paragraph(&omsi_ui::tr(text), Vec2::new(r.x, r.y), r.w, 11.5, Weight::Regular, c);
        *y += 26.0;
    };
    let metered = ph.metered;
    let fstop_now = ph.fstop();
    let focus_now = ph.focus_distance();
    let s = &mut ph.settings;
    let lens_hi = exposure::nearest_aperture(if LENS_MAX[s.lens_max.min(LENS_MAX.len() - 1)] >= 2.8 { 32.0 } else { 22.0 });
    let lens_lo = exposure::nearest_aperture(LENS_MAX[s.lens_max.min(LENS_MAX.len() - 1)]);
    match ph.tab {
        0 => {
            section(ui, &mut y, "Camera");
            let names: Vec<&str> = exposure::SENSORS.iter().map(|x| x.0).collect();
            let before = s.sensor;
            let focal = s.focal(true);
            select_row(ui, "ph-sensor", row(&mut y), "Sensor", &mut s.sensor, &names);
            if s.sensor != before {
                // (the lens stays: the same focal length on another sensor frames otherwise)
                s.fov = (2.0 * (s.sensor_h(true) * 0.5 / focal).atan()).to_degrees();
                ph.fov_target = s.fov;
            }
            let sh = s.sensor_h(true);
            let real = s.focal(true);
            let mut t = (real / 8.0).ln() / 75f32.ln();
            if ui.slider("ph-focal", row(&mut y), &mut t, 0.0, 1.0, 0.0, "Focal length", &|t| {
                format!("{:.0} mm", 8.0 * 75f32.powf(t))
            }) {
                let mm = 8.0 * 75f32.powf(t.clamp(0.0, 1.0));
                s.fov = (2.0 * (sh * 0.5 / mm).atan()).to_degrees();
                ph.fov_target = s.fov;
            }
            let s = &mut ph.settings;
            if s.sensor != 0 {
                auto_row(ui, row(&mut y), "Full-frame equivalent", &format!("{:.0} mm", lens::focal_mm(s.fov)));
            }
            let mut mode = s.orbit as usize;
            ui.label(Rect::new(x, y, w * 0.38, ROW - 2.0), "Movement");
            if ui.segmented("ph-mode", Rect::new(x + w * 0.38, y, w * 0.62, ROW - 2.0), &mut mode, &["Free", "Orbit"]) {
                s.orbit = mode == 1;
            }
            y += ROW + 4.0;
            ui.slider("ph-roll", row(&mut y), &mut s.roll, -45.0, 45.0, 0.1, "Roll", &|v| format!("{v:.1}°"));
            ui.slider("ph-speed", row(&mut y), &mut s.speed, 0.1, 4.0, 0.05, "Speed", &|v| format!("{v:.2}x"));
            let labels: Vec<&str> = RANGES.iter().map(|r| r.1).collect();
            select_row(ui, "ph-range", row(&mut y), "Distance from the bus", &mut s.range, &labels);
            section(ui, &mut y, "Frame");
            let labels: Vec<&str> = ASPECTS.iter().map(|a| a.1).collect();
            select_row(ui, "ph-aspect", row(&mut y), "Aspect ratio", &mut s.aspect, &labels);
            select_row(ui, "ph-grid", row(&mut y), "Guides", &mut s.grid, &GRIDS);
            ui.toggle("ph-level", row(&mut y), &mut s.level, "Spirit level");
            section(ui, &mut y, "Live view");
            ui.toggle("ph-hist", row(&mut y), &mut s.histogram, "Histogram");
            ui.toggle("ph-zebra", row(&mut y), &mut s.zebra, "Clipping warning (zebra)");
            ui.toggle("ph-peak", row(&mut y), &mut s.peaking, "Focus peaking");
        }
        1 => {
            section(ui, &mut y, "Exposure");
            let e = &mut s.exposure;
            ui.label(Rect::new(x, y, w * 0.30, ROW - 2.0), "Mode");
            ui.segmented("ph-xmode", Rect::new(x + w * 0.30, y, w * 0.70, ROW - 2.0), &mut e.mode, &exposure::MODES);
            y += ROW + 4.0;
            note(ui, &mut y, ["Program: the camera sets the aperture and the time.", "Aperture priority: you set the aperture, the camera the time.", "Shutter priority: you set the time, the camera the aperture.", "Manual: you set everything; the meter shows how far off it is."][e.mode.min(3)], TEXT_DIM);
            if e.mode == 1 || e.mode == 3 {
                stepped_in(ui, "ph-xap", row(&mut y), &mut e.aperture, lens_lo, lens_hi, "Aperture", &|k| format!("f/{}", exposure::APERTURES[k]));
            } else {
                auto_row(ui, row(&mut y), "Aperture", &format!("f/{}", fstop_now));
            }
            if e.mode == 2 || e.mode == 3 {
                stepped(ui, "ph-xsh", row(&mut y), &mut e.shutter, exposure::SHUTTERS.len(), "Shutter speed", &|k| exposure::SHUTTERS[k].1.to_string());
            } else {
                auto_row(ui, row(&mut y), "Shutter speed", metered.map(|m| exposure::SHUTTERS[m.shutter_ix].1).unwrap_or("-"));
            }
            ui.toggle("ph-autoiso", row(&mut y), &mut e.auto_iso, "Auto ISO");
            if e.auto_iso {
                auto_row(ui, row(&mut y), "ISO", &metered.map(|m| format!("{:.0}", m.iso)).unwrap_or_default());
            } else {
                stepped(ui, "ph-iso", row(&mut y), &mut e.iso, exposure::ISOS.len(), "ISO", &|k| format!("{:.0}", exposure::ISOS[k]));
            }
            if e.mode != 3 {
                ui.slider("ph-comp", row(&mut y), &mut e.compensation, -3.0, 3.0, 1.0 / 3.0, "Compensation", &|v| if v.abs() < 0.05 { "0 EV".into() } else { format!("{v:+.1} EV") });
            }
            let names: Vec<&str> = exposure::METERING.to_vec();
            select_row(ui, "ph-meter", row(&mut y), "Metering", &mut e.metering, &names);
            if let Some(m) = metered {
                let r = row(&mut y);
                meter_scale(ui, r, m.offset);
                if m.offset.abs() > 0.5 && e.mode != 3 {
                    note(ui, &mut y, "The lens or the shutter ran out of range: the photo comes out off the meter.", WARN);
                }
            }
            section(ui, &mut y, "Shutter");
            motion_row(ui, row(&mut y), s);
            if s.motion && !info.bus_moving {
                note(ui, &mut y, "The bus stands still: nothing to blur.", WARN);
            }
            let labels: Vec<&str> = BRACKETS.iter().map(|b| b.1).collect();
            select_row(ui, "ph-bracket", row(&mut y), "Exposure bracketing", &mut s.bracket, &labels);
            if s.bracket > 0 {
                ui.slider("ph-bstep", row(&mut y), &mut s.bracket_step, 1.0 / 3.0, 3.0, 1.0 / 3.0, "Bracketing step", &|v| format!("{v:.1} EV"));
            }
        }
        2 => {
            section(ui, &mut y, "Lens");
            let labels: Vec<String> = LENS_MAX.iter().map(|f| format!("f/{f}")).collect();
            let refs: Vec<&str> = labels.iter().map(|x| x.as_str()).collect();
            select_row(ui, "ph-lensmax", row(&mut y), "Brightest aperture", &mut s.lens_max, &refs);
            let mut corrected = !s.lens_raw;
            ui.toggle("ph-lcorr", row(&mut y), &mut corrected, "Lens corrections (distortion, vignetting)");
            s.lens_raw = !corrected;
            section(ui, &mut y, "Focus");
            ui.toggle("ph-dof", row(&mut y), &mut s.dof, "Render the depth of field");
            let mut auto = (!s.auto_focus) as usize;
            ui.label(Rect::new(x, y, w * 0.38, ROW - 2.0), "Focus");
            if ui.segmented("ph-af", Rect::new(x + w * 0.38, y, w * 0.62, ROW - 2.0), &mut auto, &["On the bus", "Manual"]) {
                s.auto_focus = auto == 0;
            }
            y += ROW + 4.0;
            if !s.auto_focus {
                let mut t = (s.focus / 0.3).ln() / 1000f32.ln();
                if ui.slider("ph-focus", row(&mut y), &mut t, 0.0, 1.0, 0.0, "Focus distance", &|t| {
                    let m = 0.3 * 1000f32.powf(t);
                    if m < 10.0 { format!("{m:.2} m") } else { format!("{m:.0} m") }
                }) {
                    s.focus = 0.3 * 1000f32.powf(t.clamp(0.0, 1.0));
                }
            }
            let (near, far, hyper) = exposure::dof_limits(s.focal(true), fstop_now, focus_now, s.sensor_h(true));
            let fmt = |m: f32| if !m.is_finite() { "∞".to_string() } else if m < 10.0 { format!("{m:.2} m") } else { format!("{m:.0} m") };
            auto_row(ui, row(&mut y), "Sharp from - to", &format!("{} - {}", fmt(near), fmt(far)));
            auto_row(ui, row(&mut y), "Hyperfocal distance", &fmt(hyper));
            if ui.button("ph-hyper", row(&mut y), &omsi_ui::tr("Focus at the hyperfocal distance"), Some("landscape"), ButtonKind::Normal) {
                acts.push(Request::Hyperfocal);
            }
            note(ui, &mut y, "Click on the picture to focus on what is there.", TEXT_DIM);
            section(ui, &mut y, "Bokeh");
            let labels: Vec<&str> = BLADES.iter().map(|b| b.1).collect();
            select_row(ui, "ph-blades", row(&mut y), "Aperture blades", &mut s.blades, &labels);
            amount(ui, "ph-hl", row(&mut y), &mut s.highlights, "Bright bokeh of lights");
            section(ui, &mut y, "Tilt and shift");
            ui.slider("ph-tilt", row(&mut y), &mut s.tilt[0], -8.0, 8.0, 0.05, "Tilt", &|v| format!("{v:+.2}°"));
            ui.slider("ph-swing", row(&mut y), &mut s.tilt[1], -8.0, 8.0, 0.05, "Swing", &|v| format!("{v:+.2}°"));
            ui.slider("ph-shiftv", row(&mut y), &mut s.shift[1], -12.0, 12.0, 0.5, "Shift up / down", &|v| format!("{v:+.1} mm"));
            ui.slider("ph-shifth", row(&mut y), &mut s.shift[0], -12.0, 12.0, 0.5, "Shift right / left", &|v| format!("{v:+.1} mm"));
            note(ui, &mut y, "Tilt leans the plane of focus (a miniature look, or the whole street sharp); shift keeps the verticals upright.", TEXT_DIM);
            section(ui, &mut y, "Quality");
            let labels: Vec<&str> = QUALITY.iter().map(|q| q.1).collect();
            select_row(ui, "ph-quality", row(&mut y), "Samples", &mut s.quality, &labels);
        }
        3 => {
            section(ui, &mut y, "White balance");
            let names: Vec<&str> = exposure::WHITE_BALANCE.iter().map(|x| x.0).collect();
            select_row(ui, "ph-wb", row(&mut y), "White balance", &mut s.wb, &names);
            if exposure::WHITE_BALANCE[s.wb.min(exposure::WHITE_BALANCE.len() - 1)].1 < 0.0 {
                ui.slider("ph-kelvin", row(&mut y), &mut s.kelvin, 2000.0, 12000.0, 50.0, "Colour temperature", &|v| format!("{v:.0} K"));
            }
            signed(ui, "ph-tint", row(&mut y), &mut s.grade.tint, "Tint");
            section(ui, &mut y, "Picture profile");
            let names: Vec<&str> = exposure::PROFILES.iter().map(|x| x.0).collect();
            select_row(ui, "ph-profile", row(&mut y), "Profile", &mut s.profile, &names);
            section(ui, &mut y, "Tone");
            let g = &mut s.grade;
            ui.slider("ph-ev", row(&mut y), &mut g.exposure, -3.0, 3.0, 0.05, "Exposure (darkroom)", &|v| if v.abs() < 0.025 { "0 EV".into() } else { format!("{v:+.1} EV") });
            signed(ui, "ph-contrast", row(&mut y), &mut g.contrast, "Contrast");
            signed(ui, "ph-high", row(&mut y), &mut g.highlights, "Highlights");
            signed(ui, "ph-shadow", row(&mut y), &mut g.shadows, "Shadows");
            signed(ui, "ph-clarity", row(&mut y), &mut g.clarity, "Clarity");
            section(ui, &mut y, "Colour");
            signed(ui, "ph-sat", row(&mut y), &mut g.saturation, "Saturation");
            signed(ui, "ph-vib", row(&mut y), &mut g.vibrance, "Vibrance");
        }
        4 => {
            section(ui, &mut y, "Film");
            let g = &mut s.grade;
            let mut k = Filter::ALL.iter().position(|f| *f == g.filter).unwrap_or(0);
            let labels: Vec<&str> = Filter::ALL.iter().map(|f| f.name()).collect();
            select_row(ui, "ph-filter", row(&mut y), "Filter", &mut k, &labels);
            g.filter = Filter::ALL[k.min(Filter::ALL.len() - 1)];
            amount(ui, "ph-fstrength", row(&mut y), &mut g.filter_strength, "Strength");
            amount(ui, "ph-grain", row(&mut y), &mut g.grain, "Film grain");
            section(ui, &mut y, "Split toning");
            let hue = |v: f32| format!("{:.0}°", v * 360.0);
            ui.slider("ph-sph", row(&mut y), &mut g.split_shadows, 0.0, 1.0, 0.005, "Shadows hue", &hue);
            ui.slider("ph-shh", row(&mut y), &mut g.split_highlights, 0.0, 1.0, 0.005, "Highlights hue", &hue);
            amount(ui, "ph-sam", row(&mut y), &mut g.split_amount, "Amount");
            signed(ui, "ph-sbal", row(&mut y), &mut g.split_balance, "Balance");
            section(ui, &mut y, "Lens and sensor");
            ui.toggle("ph-noise", row(&mut y), &mut s.noise, "Sensor noise (grows with the ISO)");
            let g = &mut s.grade;
            amount(ui, "ph-vignette", row(&mut y), &mut g.vignette, "Vignette");
            amount(ui, "ph-ca", row(&mut y), &mut g.aberration, "Colour fringes");
            amount(ui, "ph-sharp", row(&mut y), &mut g.sharpen, "Sharpen");
            section(ui, &mut y, "Frame");
            border_row(ui, row(&mut y), g);
        }
        _ => {
            section(ui, &mut y, "Time of day");
            let r = row(&mut y);
            ui.label(Rect::new(r.x, r.y, r.w * 0.38, r.h), "Clock");
            ui.text_in(&info.time, Rect::new(r.x + r.w * 0.38, r.y, r.w * 0.62, r.h), 15.0, Weight::Bold, ACCENT, Align::Center);
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
            ui.toggle("ph-png16", row(&mut y), &mut s.png16, "16 bits a channel (for editing)");
            note(ui, &mut y, "Photos are saved as PNG into the Screenshots folder of the game, with the camera's settings (aperture, time, ISO, focal length) in their metadata.", TEXT_DIM);
            y += 18.0;
        }
    }
    y
}

/// The exposure meter: a scale from -3 to +3 stops, the needle where the exposure is.
fn meter_scale(ui: &mut Ui, r: Rect, offset: f32) {
    ui.label(Rect::new(r.x, r.y, r.w * 0.22, r.h), "Meter");
    let s = Rect::new(r.x + r.w * 0.24, r.y + r.h * 0.5 - 1.0, r.w * 0.62, 2.0);
    ui.p().rect(s, TEXT_DIM.alpha(0.5));
    for k in -3..=3 {
        let tx = s.x + s.w * (k as f32 + 3.0) / 6.0;
        let hgt = if k == 0 { 12.0 } else { 7.0 };
        ui.p().rect(Rect::new(tx - 0.5, s.y - hgt * 0.5, 1.0, hgt), TEXT_DIM);
    }
    let v = offset.clamp(-3.2, 3.2);
    let nx = s.x + s.w * (v + 3.0) / 6.0;
    let c = if offset.abs() < 0.35 { OK } else if offset.abs() < 1.5 { WARN } else { DANGER };
    ui.p().rounded(Rect::new(nx - 3.0, s.y - 8.0, 6.0, 16.0), 3.0, c);
    ui.text_in(&format!("{offset:+.1}"), Rect::new(r.x + r.w * 0.88, r.y, r.w * 0.12, r.h), 12.0, Weight::Medium, c, Align::Right);
}

/// The simple mode's rows of the open page, from `y` down; returns where they end.
fn simple_page(ui: &mut Ui, ph: &mut Photo, info: &Info, x: f32, mut y: f32, w: f32, acts: &mut Vec<Request>) -> f32 {
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
            ui.toggle("ph-level", row(&mut y), &mut s.level, "Spirit level");
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
            let r = Rect::new(x, y, w, 34.0);
            ui.paragraph(&omsi_ui::tr("Click on the picture to focus on what is there."), Vec2::new(r.x, r.y), r.w, 11.5, Weight::Regular, TEXT_DIM);
            y += 24.0;
            let s = &mut ph.settings;
            stepped(ui, "ph-fstop", row(&mut y), &mut s.fstop, FSTOPS.len(), "Aperture", &|k| format!("f/{}", FSTOPS[k]));
            let labels: Vec<&str> = BLADES.iter().map(|b| b.1).collect();
            select_row(ui, "ph-blades", row(&mut y), "Bokeh", &mut s.blades, &labels);
            amount(ui, "ph-hl", row(&mut y), &mut s.highlights, "Bright bokeh of lights");
            if ui.button("ph-hyper", row(&mut y), &omsi_ui::tr("Everything sharp (hyperfocal)"), Some("landscape"), ButtonKind::Normal) {
                acts.push(Request::Hyperfocal);
            }
            section(ui, &mut y, "Shutter");
            motion_row(ui, row(&mut y), s);
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
            signed(ui, "ph-clarity", row(&mut y), &mut g.clarity, "Clarity");
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
            section(ui, &mut y, "Frame");
            border_row(ui, row(&mut y), g);
        }
        _ => {
            section(ui, &mut y, "Time of day");
            let r = row(&mut y);
            ui.label(Rect::new(r.x, r.y, r.w * 0.38, r.h), "Clock");
            ui.text_in(&info.time, Rect::new(r.x + r.w * 0.38, r.y, r.w * 0.62, r.h), 15.0, Weight::Bold, ACCENT, Align::Center);
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

/// The spirit level: a line through the middle of the frame along the horizon as the camera
/// rolls, green when level; the pitch beside it.
fn level(ui: &mut Ui, f: Rect, roll: f32, pitch: f32) {
    let m = f.center();
    let ok = roll.abs() < 0.3;
    let c = if ok { OK } else { Color::WHITE.alpha(0.8) };
    let (s, co) = (-roll.to_radians()).sin_cos();
    for k in -40i32..=40 {
        if k.abs() < 6 {
            continue;
        }
        let d = k as f32 * 2.5;
        ui.p().rect(Rect::new(m.x + d * co - 1.0, m.y + d * s - 1.0, 2.0, 2.0), c);
    }
    ui.p().rect(Rect::new(m.x - 140.0, m.y - 0.5, 30.0, 1.0), Color::WHITE.alpha(0.35));
    ui.p().rect(Rect::new(m.x + 110.0, m.y - 0.5, 30.0, 1.0), Color::WHITE.alpha(0.35));
    ui.text_in(&format!("{roll:+.1}°  {pitch:+.1}°"), Rect::new(m.x - 60.0, m.y + 14.0, 120.0, 16.0), 11.5, Weight::Medium, c, Align::Center);
}

/// Where a click focused: the corners of a small square.
fn focus_mark(ui: &mut Ui, at: Vec2, a: f32) {
    let c = OK.alpha(a);
    let (r, l) = (22.0, 8.0);
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let (cx, cy) = (at.x + sx * r, at.y + sy * r);
        ui.p().rect(Rect::new(cx.min(cx - sx * l), cy - 1.0, l, 2.0), c);
        ui.p().rect(Rect::new(cx - 1.0, cy.min(cy - sy * l), 2.0, l), c);
    }
}

/// The developed photo's histogram: red, green and blue over each other, the luma behind.
fn histogram(ui: &mut Ui, r: Rect, h: &[[u32; 64]; 4]) {
    ui.p().rounded(r, 8.0, Color::BLACK.alpha(0.55));
    let inner = Rect::new(r.x + 8.0, r.y + 8.0, r.w - 16.0, r.h - 16.0);
    let max = h.iter().flat_map(|c| c.iter().skip(1).take(62)).copied().max().unwrap_or(1).max(1) as f32;
    let bw = inner.w / 64.0;
    let colours = [Color::WHITE.alpha(0.22), Color::rgba(255, 64, 64, 1.0).alpha(0.55), Color::rgba(77, 255, 90, 1.0).alpha(0.45), Color::rgba(90, 128, 255, 1.0).alpha(0.55)];
    for (k, ch) in [3usize, 0, 1, 2].into_iter().enumerate() {
        for (i, v) in h[ch].iter().enumerate() {
            let hh = ((*v as f32 / max).sqrt() * inner.h).min(inner.h);
            ui.p().rect(Rect::new(inner.x + i as f32 * bw, inner.bottom() - hh, bw.max(1.0), hh), colours[k]);
        }
    }
    // (burnt out or black: a mark at the edge)
    let tot: u32 = h[3].iter().sum::<u32>().max(1);
    if h[3][63] as f32 / tot as f32 > 0.01 {
        ui.p().rect(Rect::new(inner.right() - 3.0, inner.y, 3.0, inner.h), DANGER);
    }
    if h[3][0] as f32 / tot as f32 > 0.01 {
        ui.p().rect(Rect::new(inner.x, inner.y, 3.0, inner.h), Color::rgba(77, 128, 255, 1.0));
    }
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
    let sh_scale = sh.scale;
    let full = Rect::new(0.0, 0.0, size.x, size.y);
    // the photo itself (while the camera moves the window's live picture is under the panel)
    if let (Some(tex), true) = (sh.picture_tex, ph.render.showing_photo()) {
        sh.ui.image(full, tex, 0.0);
    }
    // the frame the photo is cut to, and its guides
    let (fx, fy, fw, fh) = super::develop::crop_rect((size.x * 100.0) as u32, (size.y * 100.0) as u32, ph.aspect());
    let frame = Rect::new(fx as f32 / 100.0, fy as f32 / 100.0, fw as f32 / 100.0, fh as f32 / 100.0);
    guides(&mut sh.ui, frame, ph.settings.grid);
    if ph.settings.level {
        level(&mut sh.ui, frame, ph.settings.roll, ph.cam.pitch);
    }
    if let Some((fx, fy, left)) = ph.focus_mark {
        focus_mark(&mut sh.ui, Vec2::new(fx * size.x, fy * size.y), left.min(1.0));
    }
    if ph.pro && !ph.hide_ui {
        if let (true, Some(h)) = (ph.settings.histogram, ph.render.hist.as_ref()) {
            histogram(&mut sh.ui, Rect::new(size.x - 276.0, size.y - 156.0, 256.0, 120.0), h);
        }
    }
    let touch = crate::platform::touch_controls();
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
        let r = Rect::new(size.x - w - 20.0, if touch { 84.0 } else { 20.0 }, w, if place.is_some() { 58.0 } else { 40.0 });
        let (hover, _, clicked) = if path.is_some() { ui.interact(omsi_ui::kit::id_of("ph-saved"), r) } else { (false, false, false) };
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
            ui.icon("folder_open", Vec2::new(r.right() - 22.0, r.y + 39.0), 16.0, if hover { ACCENT.alpha(a) } else { TEXT_DIM.alpha(a) });
            if clicked {
                acts.push(Request::Reveal(path));
            }
        }
    }
    let (n, target) = ph.render.progress();
    let busy = ph.render.capture;
    if ph.hide_ui || (touch && !ph.touch_panel) {
        ph.panel_rect = None;
        if busy {
            ui.progress(Rect::new(size.x * 0.5 - 120.0, size.y - 28.0, 240.0, 4.0), n as f32 / target.max(1) as f32, true);
        }
        sh.actions.extend(acts.into_iter().map(Action::Photo));
        return;
    }
    // the panel
    let r = if touch {
        // a finger's panel: at the right of a wide screen, a sheet from the bottom of a tall
        // one, inside what the system covers (rounded corners, the island)
        let (il, it, ir, ib) = crate::platform::safe_insets();
        let k = 1.0 / sh_scale.max(0.25);
        let avail = Rect::new(il * k + 12.0, it * k + 12.0, size.x - (il + ir) * k - 24.0, size.y - (it + ib) * k - 24.0);
        if size.x > size.y * 1.1 {
            let w = PANEL_W.min(avail.w * 0.6);
            Rect::new(avail.right() - w, avail.y, w, avail.h)
        } else {
            let h = (avail.h * 0.64).min(avail.h);
            Rect::new(avail.x, avail.bottom() - h, avail.w, h)
        }
    } else {
        Rect::new(20.0, 20.0, PANEL_W.min(size.x - 40.0), size.y - 40.0)
    };
    ph.panel_rect = touch.then(|| [r.x * sh_scale, r.y * sh_scale, r.w * sh_scale, r.h * sh_scale]);
    ui.panel(r);
    let inner = Rect::new(r.x + 18.0, r.y + 16.0, r.w - 36.0, r.h - 32.0);
    ui.icon("photo_camera", Vec2::new(inner.x + 12.0, inner.y + 14.0), 20.0, ACCENT);
    ui.text("Photo mode", Vec2::new(inner.x + 32.0, inner.y + 21.0), 18.0, Weight::Bold, TEXT, Align::Left);
    let mm = ph.settings.focal(ph.pro);
    let fnum = ph.fstop();
    let exif = if ph.pro {
        let t = ph.shutter_time();
        let ts = exposure::SHUTTERS[exposure::nearest_shutter(t)].1;
        format!("{}  {ts}  f/{fnum}  ISO {:.0}  {mm:.0} mm", exposure::MODES[ph.settings.exposure.mode.min(3)], ph.metered.map(|m| m.iso).unwrap_or(100.0))
    } else if ph.settings.dof {
        format!("{mm:.0} mm  |  f/{fnum}  |  {:.1} m", ph.focus_distance())
    } else {
        format!("{mm:.0} mm  |  {:.0}°", ph.settings.fov)
    };
    ui.text_in(&exif, Rect::new(inner.x, inner.y + 32.0, inner.w, 18.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
    let mut tab = ph.tab.min(tabs(ph.pro).len() - 1);
    if ui.segmented("ph-tabs", Rect::new(inner.x, inner.y + 58.0, inner.w, 34.0), &mut tab, tabs(ph.pro)) {
        ph.tab = tab;
    }
    // the pages scroll above the buttons
    let foot = if touch { 38.0 + 10.0 } else { 44.0 + 10.0 + 38.0 + 8.0 + 40.0 };
    let body = Rect::new(inner.x, inner.y + 100.0, inner.w, (inner.bottom() - foot - inner.y - 100.0).max(40.0));
    let key = format!("ph-page-{}", ph.tab);
    ui.scroll_area(&key, body, &mut |ui, v| {
        let end = page(ui, ph, info, v.x, v.y, v.w - 8.0, &mut acts);
        end - v.y + 6.0
    });
    if touch {
        // (the shutter, the stick and the rest are the fingers' own buttons, see `touch.rs`)
        let half = (inner.w - 8.0) / 2.0;
        if ui.button("ph-reset", Rect::new(inner.x, inner.bottom() - 38.0, half, 38.0), &omsi_ui::tr("Reset"), Some("restart_alt"), ButtonKind::Ghost) {
            acts.push(Request::Reset);
        }
        if ui.button("ph-done", Rect::new(inner.x + half + 8.0, inner.bottom() - 38.0, half, 38.0), &omsi_ui::tr("Done"), Some("check"), ButtonKind::Primary) {
            ph.touch_panel = false;
        }
        sh.actions.extend(acts.into_iter().map(Action::Photo));
        return;
    }
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

/// The photo mode's panel for this frame.
pub(super) fn photo_panel(cx: &mut super::PhotoCx, dt: f32) {
    if cx.menus.shell.process_prompt.is_some() { return; }
    let Some(s) = cx.gfx.gfx.surface.as_ref() else { return };
    let (w, h) = (s.config.width as f32, s.config.height as f32);
    let dpi = cx.gfx.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0);
    let scale = dpi * crate::ui::size_factor(h, dpi, cx.common.settings.ui_scale, cx.common.settings.ui_scale_window);
    let t = cx.clock.clock.time;
    let info = Info {
        time: format!("{:02}:{:02}", ((t / 3600.0) as i64).rem_euclid(24), ((t % 3600.0) / 60.0) as i64),
        bus_moving: cx.ui.photo.as_ref().is_some_and(|p| p.bus_velocity.length() > 0.2),
    };
    let Some(ph) = cx.ui.photo.as_mut() else { return };
    cx.menus.shell.opaque = ph.render.showing_photo() && cx.menus.shell.picture_tex.is_some();
    cx.menus.shell.begin(w, h, scale, dt);
    draw(&mut *cx.menus.shell, ph, &info);
    cx.menus.shell.finish();
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
