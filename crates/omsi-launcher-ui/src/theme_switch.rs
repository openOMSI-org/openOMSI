//! The light and the dark look, and the switch between them (Luc: "There must also be a dark
//! and a light mode, with a satisfying transition animation", and "the light/dark transition
//! may be animated for the whole app anyway").
//!
//! The setting "dark mode" says which look; the sun and moon in the bar and in the bus
//! company's bar, and the switch in the bar's palette, change it. The look changes for the
//! whole launcher between two frames: the frame the setting changed in - still the old look -
//! is drawn once more into a texture that stays on the graphics card (nothing is read back,
//! nothing waits), and over the frames after it that picture lies over the new look outside a
//! circle that grows from where the click was, easing out over two thirds of a second: the new
//! look spreads from the button over the whole window, with a soft shade along its edge. A
//! switch during the spread starts a new one from the picture on the screen. With the setting
//! "animations" off the new look is there at once.

use super::theme::{light_ground, TEXT};
use super::ui::{ease_out_cubic, id_of, ButtonKind, Feel, Ui};
use super::Launcher;
use glam::Vec2;

use omsi_ui::{Color, Draw, Layer, Painter, Rect, Vertex};
use serde_json::json;

/// How long the new look takes to fill the window (seconds).
pub const SPREAD_S: f32 = 0.65;
/// The shade along the circle's edge, on the old look: how wide (points) and how dark at the
/// edge.
const SHADE_W: f32 = 22.0;
const SHADE: f32 = 0.16;

/// A picture of the old look on the graphics card: its texture, the number the interface
/// draws it by, its size in pixels, and where it spreads from (points).
struct Picture {
    _texture: wgpu::Texture,
    slot: usize,
    size: (u32, u32),
    from: Vec2,
}

/// The look and its switch.
#[derive(Default)]
pub struct ThemeSwitch {
    /// The look the frames are drawn in: dark or not (None before the first frame).
    dark: Option<bool>,
    /// The old look's picture, taken at the end of the frame the setting changed in.
    taken: Option<Picture>,
    /// The new look on its way: the old one's picture, and how far (0 to 1).
    spread: Option<(Picture, f32)>,
    /// The interface's two numbers for the pictures (one for the spread on its way, the other
    /// for the picture of a switch during it).
    slots: [Option<usize>; 2],
}

impl ThemeSwitch {
    /// The graphics device is gone: the pictures and their numbers with it.
    pub fn drop_gpu(&mut self) {
        self.taken = None;
        self.spread = None;
        self.slots = [None, None];
    }
}

/// The setting: the dark look or not.
fn wanted(l: &Launcher) -> bool {
    l.state.settings.get("dark_mode").and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Turn the look over: the setting (saved a moment later); the frame drawn now is the old
/// look's last (`after_frame`).
pub fn flip(l: &mut Launcher) {
    let dark = !wanted(l);
    l.state.settings["dark_mode"] = json!(dark);
    l.state.settings_dirty = 0.3;
}

/// At the frame's start (after `Ui::begin`, the setting "animations" read): the look as the
/// setting has it - the spread from the old look's picture when one was taken - and how far
/// the spread is.
pub fn begin_frame(l: &mut Launcher) {
    let want = wanted(l);
    let sw = &mut l.theme_switch;
    match sw.dark {
        Some(d) if d != want => {
            sw.dark = Some(want);
            if let Some(p) = sw.taken.take() {
                if l.ui.motion {
                    sw.spread = Some((p, 0.0));
                }
            }
        }
        Some(_) => sw.taken = None,
        None => sw.dark = Some(want),
    }
    if let Some((_, t)) = sw.spread.as_mut() {
        *t += l.ui.dt / SPREAD_S;
        if *t >= 1.0 || !l.ui.motion {
            sw.spread = None;
        } else {
            l.ui.keep_moving();
        }
    }
    let dark = sw.dark.unwrap_or(want);
    crate::accent::set_dark(dark);
    l.ui.light = !dark;
    l.ui.lighten.set_ground(light_ground(crate::accent::chosen()));
}

/// Whether the frame just drawn is the old look's last: the setting changed in it (and the
/// change is to be seen spreading).
pub fn wants_picture(l: &Launcher) -> bool {
    l.ui.motion && l.theme_switch.dark.is_some_and(|d| d != wanted(l)) && l.theme_switch.taken.is_none()
}

/// The frame just drawn (`layers`, `draws`, `w` x `h` pixels) as the old look's picture,
/// drawn once more into a texture of its own (`Launcher::look_picture`); it spreads from the
/// mouse, where the switch was clicked.
pub fn take_picture(l: &mut Launcher, w: u32, h: u32, layers: &[Layer], draws: &[Draw], bg: wgpu::Color) {
    // (the number the spread on its way does not use)
    let k = match l.theme_switch.spread.as_ref() {
        Some((p, _)) if l.theme_switch.slots[0] == Some(p.slot) => 1,
        _ => 0,
    };
    let Some((texture, view)) = l.look_picture(w, h, layers, draws, bg) else { return };
    let Some((device, _, _)) = l.dev.clone() else { return };
    let Some(gpu) = l.gpu.as_mut() else { return };
    let slot = match l.theme_switch.slots[k] {
        Some(id) => {
            gpu.set_view(&device, id, &view, (w, h));
            id
        }
        None => {
            let id = gpu.add_view(&device, &view, (w, h));
            l.theme_switch.slots[k] = Some(id);
            id
        }
    };
    l.theme_switch.taken = Some(Picture { _texture: texture, slot, size: (w, h), from: l.ui.input.mouse });
}

/// The spread on its way, over the frame just finished (`Ui::finish`'s layers, vertices and
/// ranges): the old look's picture outside the circle, and the shade along its edge. The
/// window `w` x `h` pixels; a window resized meanwhile ends it.
pub fn overlay(l: &mut Launcher, w: u32, h: u32, layers: &mut Vec<Layer>, verts: &mut Vec<Vertex>, ranges: &mut Vec<(std::ops::Range<u32>, usize)>) {
    let Some((p, t)) = l.theme_switch.spread.as_ref() else { return };
    if p.size != (w, h) {
        l.theme_switch.spread = None;
        return;
    }
    let size = l.ui.size;
    let layer = Layer { view_proj: glam::Mat4::IDENTITY, viewport: [0.0, 0.0, size.x, size.y], clip: [0.0, 0.0, w as f32, h as f32], radius: 0.0, opacity: 1.0, px_scale: 0.0 };
    let r = radius(size, p.from, *t);
    let a = verts.len() as u32;
    verts.extend(outside(size, p.from, r));
    layers.push(layer);
    ranges.push((a..verts.len() as u32, p.slot));
    if r > 0.5 {
        let b = verts.len() as u32;
        let mut shade = Painter::new();
        edge_shade(&mut shade, p.from, r);
        verts.extend(shade.verts);
        layers.push(layer);
        ranges.push((b..verts.len() as u32, 0));
    }
}

/// How far the new look has spread at `t` (0 to 1) from `from` in a window `size` points: an
/// ease out from nothing to past the farthest corner (and the shade beyond it).
pub fn radius(size: Vec2, from: Vec2, t: f32) -> f32 {
    let far = [Vec2::ZERO, Vec2::new(size.x, 0.0), Vec2::new(0.0, size.y), size].iter().map(|c| c.distance(from)).fold(0.0f32, f32::max);
    ease_out_cubic(t) * (far + SHADE_W + 2.0)
}

/// How many sides the circle of `r` points has: within a third of a point of round.
fn sides(r: f32) -> usize {
    let step = (1.0 - 0.3 / r.max(1.0)).clamp(-1.0, 1.0).acos();
    ((std::f32::consts::TAU / step.max(1e-3)).ceil() as usize).clamp(48, 720)
}

/// The window (`size` points) outside the circle of `r` round `from`, as triangles of the
/// picture: a ring from the circle out past the window's corners, each point's place in the
/// picture its place in the window.
pub fn outside(size: Vec2, from: Vec2, r: f32) -> Vec<Vertex> {
    let far = [Vec2::ZERO, Vec2::new(size.x, 0.0), Vec2::new(0.0, size.y), size].iter().map(|c| c.distance(from)).fold(0.0f32, f32::max);
    let n = sides(far.max(r));
    // (the outer ring's sides cut inside its circle: it reaches out far enough that they do
    // not cut a corner off)
    let out = (far + 4.0) / (std::f32::consts::PI / n as f32).cos() + 2.0;
    let v = |q: Vec2| Vertex { pos: [q.x, q.y, 0.0], uv: [q.x / size.x.max(1.0), q.y / size.y.max(1.0)], color: Color::WHITE.0, mode: [0.0, 1.0], ..Default::default() };
    let at = |k: usize, rad: f32| from + Vec2::from_angle(std::f32::consts::TAU * (k % n) as f32 / n as f32) * rad;
    let mut verts = Vec::with_capacity(n * 6);
    for k in 0..n {
        let (i0, i1, o0, o1) = (at(k, r), at(k + 1, r), at(k, out), at(k + 1, out));
        verts.extend([v(i0), v(o0), v(o1), v(i0), v(o1), v(i1)]);
    }
    verts
}

/// The shade along the circle's edge, on the old look outside it: dark at the edge, gone
/// `SHADE_W` points out - the new look lies a little above the old one.
fn edge_shade(p: &mut Painter, from: Vec2, r: f32) {
    let n = sides(r + SHADE_W);
    let at = |k: usize, rad: f32| from + Vec2::from_angle(std::f32::consts::TAU * (k % n) as f32 / n as f32) * rad;
    let (dark, none) = (Color::rgba(0, 0, 0, SHADE), Color::rgba(0, 0, 0, 0.0));
    for k in 0..n {
        let (i0, i1, o0, o1) = (at(k, r), at(k + 1, r), at(k, r + SHADE_W), at(k + 1, r + SHADE_W));
        p.tri(i0, o0, o1, dark, none, none);
        p.tri(i0, o1, i1, dark, none, dark);
    }
}

// --- the sun and the moon ----------------------------------------------------------------

/// The sun turning into the moon at `moon` (0 the sun, 1 the moon), in a box of `size` points
/// round `c`, in `ink`: the sun's rays draw in and turn away as its disc grows, and a second
/// disc slides over it until a crescent is left.
pub fn glyph(p: &mut Painter, c: Vec2, size: f32, moon: f32, ink: Color) {
    let m = moon.clamp(0.0, 1.0);
    let half = size * 0.5;
    let r = half * (0.40 + 0.22 * m);
    // the rays: eight, shorter and fainter as the moon comes, turning a little
    let sun = 1.0 - m;
    if sun > 0.01 {
        let len = half * 0.26 * sun;
        let inner = r + half * 0.16;
        for k in 0..8 {
            let a = std::f32::consts::FRAC_PI_4 * k as f32 + 0.6 * m;
            let d = Vec2::from_angle(a);
            p.line(c + d * inner, c + d * (inner + len.max(0.01)), half * 0.15, ink.alpha(sun));
        }
    }
    // the bite: a disc from the upper right, over the sun's disc as the moon comes
    let rb = r * 0.8;
    let towards = Vec2::new(1.0, -1.0).normalize();
    let ease = m * m * (3.0 - 2.0 * m);
    let d = (r + rb + 0.5) + (r * 0.55 - (r + rb + 0.5)) * ease;
    crescent(p, c, r, c + towards * d, rb, ink);
}

/// The disc of `r` round `c` without what the disc of `rb` round `b` covers of it.
fn crescent(p: &mut Painter, c: Vec2, r: f32, b: Vec2, rb: f32, ink: Color) {
    let d = c.distance(b);
    if d >= r + rb - 0.01 {
        p.circle(c, r, ink);
        return;
    }
    // where the two circles meet: on the disc's, `alpha` either side of the way to the bite
    let phi = (b - c).y.atan2((b - c).x);
    let alpha = ((d * d + r * r - rb * rb) / (2.0 * d * r)).clamp(-1.0, 1.0).acos();
    let p1 = c + Vec2::from_angle(phi + alpha) * r;
    // the bite's edge inside the disc, from the first meeting point to the second, through
    // the way to the disc's middle
    let psi = phi + std::f32::consts::PI;
    let beta = ((d * d + rb * rb - r * r) / (2.0 * d * rb)).clamp(-1.0, 1.0).acos();
    let s = if (b + Vec2::from_angle(psi - beta) * rb).distance(p1) < (b + Vec2::from_angle(psi + beta) * rb).distance(p1) { 1.0 } else { -1.0 };
    let n = 28;
    for k in 0..n {
        let (t0, t1) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
        let outer = |t: f32| c + Vec2::from_angle(phi + alpha + (std::f32::consts::TAU - 2.0 * alpha) * t) * r;
        let inner = |t: f32| b + Vec2::from_angle(psi - s * beta + s * 2.0 * beta * t) * rb;
        let (o0, o1, i0, i1) = (outer(t0), outer(t1), inner(t0), inner(t1));
        p.tri(o0, o1, i1, ink, ink, ink);
        p.tri(o0, i1, i0, ink, ink, ink);
    }
}

/// The glyph's moon as the look is drawn now, sprung (a click turns it as the new look
/// spreads from it).
fn moon_of(ui: &mut Ui, name: &str) -> f32 {
    let to = if crate::accent::dark() { 1.0 } else { 0.0 };
    ui.spring(id_of(name) ^ 0x5017_3007, to, Feel::KNOB)
}

/// What a click does, for the tooltip.
fn tip() -> &'static str {
    if crate::accent::dark() {
        "Switch to the light mode"
    } else {
        "Switch to the dark mode"
    }
}

/// The sun and moon in the launcher's bar at `hit`, in the glass's ink (`ink`: at rest, under
/// the mouse).
pub fn bar_button(l: &mut Launcher, hit: Rect, ink: (Color, Color)) {
    let (h, _, clicked) = l.ui.interact(id_of("bar-theme"), hit);
    let moon = moon_of(&mut l.ui, "bar-theme");
    let lit = l.ui.anim(id_of("bar-theme") ^ 0x11, if h { 1.0 } else { 0.0 }, 0.08);
    glyph(l.ui.p(), hit.center(), 17.0, moon, ink.0.mix(ink.1, lit));
    l.ui.tooltip(hit, tip());
    if clicked {
        flip(l);
    }
}

/// The sun and moon in the bus company's bar, a button as its settings' and help's beside it,
/// its right edge at `right`: returns where the next one to the left ends.
pub fn company_button(l: &mut Launcher, right: f32) -> f32 {
    let r = Rect::new(right - 40.0, 16.0, 40.0, 40.0);
    let clicked = l.ui.button("company-theme", r, "", None, ButtonKind::Normal);
    let moon = moon_of(&mut l.ui, "company-theme");
    glyph(l.ui.p(), r.center(), 18.0, moon, TEXT);
    l.ui.tooltip(r, tip());
    if clicked {
        flip(l);
    }
    r.x - 8.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(v: &[Vertex]) -> f32 {
        v.chunks(3)
            .map(|t| {
                let (a, b, c) = (Vec2::new(t[0].pos[0], t[0].pos[1]), Vec2::new(t[1].pos[0], t[1].pos[1]), Vec2::new(t[2].pos[0], t[2].pos[1]));
                (b - a).perp_dot(c - a).abs() * 0.5
            })
            .sum()
    }

    #[test]
    fn the_new_look_spreads_easing_out_past_every_corner() {
        let size = Vec2::new(1440.0, 900.0);
        let from = Vec2::new(1200.0, 40.0);
        // nothing at first, all of the window at the end
        assert_eq!(radius(size, from, 0.0), 0.0);
        let end = radius(size, from, 1.0);
        let corner = from.distance(Vec2::new(0.0, 900.0));
        assert!(end > corner + SHADE_W, "{end} {corner}");
        // easing out: fast at first, slow into its end, never back
        let at = |t: f32| radius(size, from, t);
        assert!(at(0.25) > 0.5 * end);
        assert!(at(0.1) - at(0.0) > at(1.0) - at(0.9));
        let mut last = 0.0;
        for k in 1..=60 {
            let r = at(k as f32 / 60.0);
            assert!(r >= last);
            // (no frame jumps more than a tenth of the way at 60 frames a second)
            assert!(r - last <= 0.1 * end, "{k}: {}", r - last);
            last = r;
        }
    }

    #[test]
    fn the_old_look_covers_the_window_outside_the_circle_only() {
        let size = Vec2::new(1440.0, 900.0);
        let from = Vec2::new(700.0, 450.0);
        // at the start the old picture covers all of the window (the circle has no size)
        let v = outside(size, from, 0.0);
        assert_eq!(v.len() % 3, 0);
        let covered = |v: &[Vertex], q: Vec2| {
            v.chunks(3).any(|t| {
                let p: Vec<Vec2> = t.iter().map(|x| Vec2::new(x.pos[0], x.pos[1])).collect();
                super::super::ui::inside_convex(&p, q) || super::super::ui::inside_convex(&[p[0], p[2], p[1]], q)
            })
        };
        for q in [Vec2::ZERO, Vec2::new(1440.0, 0.0), Vec2::new(0.0, 900.0), Vec2::new(1440.0, 900.0), Vec2::new(720.0, 10.0)] {
            assert!(covered(&v, q), "{q}");
        }
        // half way: the middle is the new look's, the corners still the old one's
        let v = outside(size, from, 300.0);
        assert!(!covered(&v, from + Vec2::new(150.0, 100.0)));
        assert!(covered(&v, Vec2::new(5.0, 5.0)) && covered(&v, Vec2::new(1435.0, 895.0)));
        // each point shows the picture where it is
        for x in &v {
            assert!((x.uv[0] * size.x - x.pos[0]).abs() < 1e-2 && (x.uv[1] * size.y - x.pos[1]).abs() < 1e-2);
            assert_eq!(x.mode, [0.0, 1.0]);
        }
        // the circle is round: its hole within a third of a point of a true circle's area
        let ring_outer = area(&outside(size, from, 0.0));
        let hole = ring_outer - area(&v);
        let true_hole = std::f32::consts::PI * 300.0 * 300.0;
        assert!((hole / true_hole - 1.0).abs() < 0.003, "{hole} {true_hole}");
    }

    #[test]
    fn the_sun_turns_into_a_crescent() {
        let c = Vec2::new(50.0, 50.0);
        let mut sun = Painter::new();
        glyph(&mut sun, c, 20.0, 0.0, Color::WHITE);
        let mut moon = Painter::new();
        glyph(&mut moon, c, 20.0, 1.0, Color::WHITE);
        assert_eq!(sun.verts.len() % 3, 0);
        assert_eq!(moon.verts.len() % 3, 0);
        // the sun has its rays; the moon is a crescent: less than its disc, more than nothing
        let r = 10.0 * 0.62;
        let disc = std::f32::consts::PI * r * r;
        let a = area(&moon.verts);
        assert!(a > 0.2 * disc && a < 0.7 * disc, "{a} of {disc}");
        // (its bite is towards the upper right: the moon's light lies lower left)
        let lower_left = moon.verts.iter().filter(|v| v.pos[0] < c.x && v.pos[1] > c.y).count();
        let upper_right = moon.verts.iter().filter(|v| v.pos[0] > c.x + 2.0 && v.pos[1] < c.y - 2.0).count();
        assert!(lower_left > upper_right);
        // on the way, a thing of whole triangles
        for k in 0..=10 {
            let mut p = Painter::new();
            glyph(&mut p, c, 20.0, k as f32 / 10.0, Color::WHITE);
            assert_eq!(p.verts.len() % 3, 0);
            assert!(p.verts.iter().all(|v| v.pos[0].is_finite() && v.pos[1].is_finite()));
        }
    }
}
