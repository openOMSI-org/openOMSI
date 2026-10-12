//! RosterFlow's name, its mark and its opening. The mark is a scheduler's Gantt in small:
//! three trip chips on a time axis, staggered as a bus's day hands over to the next - blue,
//! amber and teal, the middle one pointing back. When RosterFlow opens, the chips slide in
//! along the axis and settle into the mark, the name and "Bus Scheduling & Crew Rostering"
//! come up beside it, and the studio fades in (about 1.5 s; a click or Escape skips it, and
//! the setting "animations" off leaves it out).

use super::super::super::ui::{ease_out_cubic, Ui};
use glam::Vec2;
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

/// The name and the line under it (the same in every language: a product's name, its
/// subtitle translated).
pub(super) const NAME: &str = "RosterFlow";
pub(super) const SUBTITLE: &str = "Bus Scheduling & Crew Rostering";

/// The mark's chips: where each lies in the tile (fractions), its colour, and which way it
/// points.
const CHIPS: [(f32, f32, f32, [u8; 3], bool); 3] = [(0.16, 0.24, 0.80, [63, 168, 255], true), (0.42, 0.20, 0.72, [255, 172, 51], false), (0.68, 0.34, 0.86, [38, 198, 170], true)];

/// An arrow chip: rounded on its flat end, pointed on the other (`right`: to the right).
pub(super) fn arrow_chip(ui: &mut Ui, r: Rect, fill: Color, right: bool) {
    if r.w <= 0.5 || r.h <= 0.5 {
        return;
    }
    let tip = (r.h * 0.45).min(r.w * 0.45);
    let rad = (r.h * 0.22).min(r.w * 0.25);
    if right {
        let body = Rect::new(r.x, r.y, (r.w - tip + rad).max(1.0), r.h);
        ui.p().rounded(body, rad, fill);
        ui.p().rect(Rect::new((r.right() - tip - rad).max(r.x), r.y, rad.min(r.w), r.h), fill);
        ui.p().convex(&[Vec2::new(r.right() - tip, r.y), Vec2::new(r.right(), r.center().y), Vec2::new(r.right() - tip, r.bottom())], fill);
    } else {
        let body = Rect::new(r.x + tip - rad, r.y, (r.w - tip + rad).max(1.0), r.h);
        ui.p().rounded(body, rad, fill);
        ui.p().rect(Rect::new(r.x + tip, r.y, rad.min(r.w), r.h), fill);
        ui.p().convex(&[Vec2::new(r.x + tip, r.y), Vec2::new(r.x + tip, r.bottom()), Vec2::new(r.x, r.center().y)], fill);
    }
}

/// The outline of an arrow chip (a slot where something may go).
pub(super) fn arrow_points(r: Rect, right: bool) -> Vec<Vec2> {
    let tip = (r.h * 0.45).min(r.w * 0.45);
    if right {
        vec![Vec2::new(r.x, r.y), Vec2::new(r.right() - tip, r.y), Vec2::new(r.right(), r.center().y), Vec2::new(r.right() - tip, r.bottom()), Vec2::new(r.x, r.bottom()), Vec2::new(r.x, r.y)]
    } else {
        vec![Vec2::new(r.x + tip, r.y), Vec2::new(r.right(), r.y), Vec2::new(r.right(), r.bottom()), Vec2::new(r.x + tip, r.bottom()), Vec2::new(r.x, r.center().y), Vec2::new(r.x + tip, r.y)]
    }
}

/// The mark in `r` (a square), `t` how far it has come together (1 together), `alpha` its
/// fade. Drawn as it is in either look.
pub(super) fn mark(ui: &mut Ui, r: Rect, t: f32, alpha: f32) {
    let s = r.w.min(r.h);
    let tile = Rect::new(r.center().x - s * 0.5, r.center().y - s * 0.5, s, s);
    ui.as_drawn();
    ui.p().rounded_gradient(tile, s * 0.22, Color::rgba(30, 41, 62, alpha), Color::rgba(18, 25, 40, alpha));
    ui.p().rounded_border(tile, s * 0.22, (s * 0.025).max(1.0), Color::rgba(255, 255, 255, 0.10 * alpha));
    // the time axis: a faint line, and the hour ticks
    let ax = tile.x + s * 0.30;
    ui.p().rect(Rect::new(ax, tile.y + s * 0.10, (s * 0.018).max(1.0), s * 0.80), Color::rgba(255, 255, 255, 0.18 * alpha));
    for k in 0..3 {
        let x = tile.x + s * (0.30 + 0.25 * k as f32);
        ui.p().rect(Rect::new(x, tile.y + s * 0.08, (s * 0.012).max(1.0), s * 0.05), Color::rgba(255, 255, 255, 0.28 * alpha));
    }
    for (k, (y, a, b, c, right)) in CHIPS.iter().enumerate() {
        // (each slides in along the axis, one after the other)
        let local = ((t - k as f32 * 0.14) / 0.72).clamp(0.0, 1.0);
        let e = ease_out_cubic(local);
        let from = if k % 2 == 0 { -0.9 } else { 0.9 };
        let shift = (1.0 - e) * from * s;
        let h = s * 0.16;
        let chip = Rect::new(tile.x + a * s + shift, tile.y + y * s - h * 0.5 + h * 0.5, (b - a) * s, h);
        ui.push_clip(tile, s * 0.22);
        arrow_chip(ui, chip, Color::rgba(c[0], c[1], c[2], alpha * e.max(0.15)), *right);
        ui.pop_clip();
    }
    ui.end_as_drawn();
}

/// The name in two colours ("Roster" in the ink, "Flow" in the blue) at `at` (its left and
/// middle), `px` its size. Returns its width.
pub(super) fn wordmark(ui: &mut Ui, at: Vec2, px: f32, ink: Color, blue: Color) -> f32 {
    ui.as_drawn();
    let w1 = ui.text_as_is("Roster", Vec2::new(at.x, at.y + px * 0.36), px, Weight::Black, ink, Align::Left);
    let w2 = ui.text_as_is("Flow", Vec2::new(at.x + w1, at.y + px * 0.36), px, Weight::Black, blue, Align::Left);
    ui.end_as_drawn();
    w1 + w2
}

/// The opening's length (seconds).
pub(super) const INTRO: f32 = 1.5;

/// The opening over `r` at `t` seconds since it began: the chips come together into the mark
/// (to 0.8 s), the name and the subtitle rise beside it (0.55 - 1.05 s), and the whole fades
/// to the studio under it (1.05 - 1.5 s). `bg`, `ink`, `dim`, `blue`: the look's colours.
pub(super) fn opening(ui: &mut Ui, r: Rect, t: f32, bg: Color, ink: Color, dim: Color, blue: Color) {
    let fade = 1.0 - ((t - 1.05) / 0.45).clamp(0.0, 1.0);
    if fade <= 0.0 {
        return;
    }
    ui.as_drawn();
    ui.p().rect(r, bg.alpha(fade));
    let s = (r.h * 0.22).clamp(64.0, 132.0);
    let name_px = s * 0.42;
    let name_w = ui.width(NAME, name_px, Weight::Black);
    let total = s + s * 0.28 + name_w;
    let mx = r.center().x - total * 0.5;
    let my = r.center().y - s * 0.5;
    // (a time axis across, its hours faint: what the chips run on)
    let ay = r.center().y + s * 0.78;
    ui.p().rect(Rect::new(r.x + r.w * 0.18, ay, r.w * 0.64, 1.0), dim.alpha(0.35 * fade));
    for k in 0..=8 {
        let x = r.x + r.w * (0.18 + 0.08 * k as f32);
        ui.p().rect(Rect::new(x, ay - 4.0, 1.0, 8.0), dim.alpha(0.35 * fade));
        let h = format!("{:02}:00", 5 + 2 * k);
        ui.text_as_is(&h, Vec2::new(x, ay + 8.0), 10.0, Weight::Bold, dim.alpha(0.6 * fade), Align::Center);
    }
    mark(ui, Rect::new(mx, my, s, s), (t / 0.8).clamp(0.0, 1.0), fade);
    let words = ((t - 0.55) / 0.5).clamp(0.0, 1.0);
    let rise = (1.0 - ease_out_cubic(words)) * 14.0;
    let nx = mx + s * 1.28;

    let a = words * fade;
    let w = ui.text_as_is("Roster", Vec2::new(nx, r.center().y + name_px * 0.12 + rise), name_px, Weight::Black, ink.alpha(a), Align::Left);
    ui.text_as_is("Flow", Vec2::new(nx + w, r.center().y + name_px * 0.12 + rise), name_px, Weight::Black, blue.alpha(a), Align::Left);
    let sub = omsi_ui::tr(SUBTITLE);
    ui.text(&sub, Vec2::new(nx + 2.0, r.center().y + name_px * 0.98 + rise * 1.4), (name_px * 0.36).max(12.0), Weight::Medium, dim.alpha(a), Align::Left);
    ui.end_as_drawn();
    let _ = Rect::new(0.0, 0.0, 0.0, 0.0);
    ui.keep_moving();
}

/// The opening's progress: whether it still plays at `t` seconds.
pub(super) fn playing(t: f32) -> bool {
    t < INTRO
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_opening_lasts_a_second_and_a_half_and_the_chips_point_both_ways() {
        assert!(playing(0.0) && playing(1.49) && !playing(1.5));
        assert!(CHIPS.iter().any(|c| c.4) && CHIPS.iter().any(|c| !c.4));
        let p = arrow_points(Rect::new(0.0, 0.0, 40.0, 10.0), true);
        assert_eq!(p[2], Vec2::new(40.0, 5.0));
        let q = arrow_points(Rect::new(0.0, 0.0, 40.0, 10.0), false);
        assert_eq!(q[4], Vec2::new(0.0, 5.0));
    }
}
