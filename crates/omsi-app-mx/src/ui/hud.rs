use super::{Renderer, Scene, Ui};
use omsi_mapdraw::style;
use omsi_ui::Color;

#[derive(Clone, Copy, Debug)]
pub(super) struct Hud {
    pub card: [u8; 4],
    pub ink: [u8; 4],
    pub dim: [u8; 4],
    pub muted: [u8; 4],
    pub accent: [u8; 4],
}

pub(super) const RADIUS: f32 = 14.0;
pub(crate) const SIZE_KEYS: [&str; 3] = ["info_size", "timetable_size", "perf_size"];

pub(crate) fn window_size(size: f32) -> f32 {
    if size.is_finite() { size.clamp(0.6, 2.0) } else { 1.0 }
}

pub(super) fn fit_cap(used: f32, rect: [f32; 4], room: [f32; 4]) -> f32 {
    let (w, h) = ((rect[2] - rect[0]).max(1.0), (rect[3] - rect[1]).max(1.0));
    let ratio = ((room[3] - rect[1]) / h).min((room[2] - room[0]) / w);
    (used * ratio).max(0.3)
}

fn grip_pixels(n: u32, s: f32, c: [u8; 4]) -> Vec<u8> {
    let mut rgba = vec![0u8; (n * n * 4) as usize];
    for py in 0..n {
        for px in 0..n {
            let along = (n as f32 - px as f32 - 0.5) + (n as f32 - py as f32 - 0.5);
            let cover = [3.0, 7.0, 11.0]
                .iter()
                .map(|o| (0.7 * s + 0.5 - (along - o * s).abs() / std::f32::consts::SQRT_2).clamp(0.0, 1.0))
                .fold(0.0, f32::max);
            let i = ((py * n + px) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], (c[3] as f32 * 0.7 * cover).round() as u8]);
        }
    }
    rgba
}
pub(super) const LINE: [u8; 4] = [226, 58, 52, 255];
pub(super) const WARN: [u8; 4] = [235, 170, 60, 255];
pub(super) const ON_TIME: [u8; 4] = [47, 168, 79, 255];
pub(super) const LATE: [u8; 4] = rgba(style::TRAFFIC[2]);
pub(super) const EARLY: [u8; 4] = rgba(style::TRAFFIC[0]);

const fn rgba(c: Color) -> [u8; 4] {
    let [r, g, b, a] = c.0;
    [(r * 255.0 + 0.5) as u8, (g * 255.0 + 0.5) as u8, (b * 255.0 + 0.5) as u8, (a * 255.0 + 0.5) as u8]
}

impl Hud {
    pub fn new(light: bool) -> Self {
        let pal = style::palette(light);
        Self {
            card: rgba(pal.card),
            ink: rgba(pal.card_text),
            dim: rgba(pal.card_dim),
            muted: if light { [142, 142, 147, 255] } else { [118, 118, 124, 255] },
            accent: rgba(pal.route),
        }
    }

    pub fn shade(&self, toward: [u8; 4], k: f32, opacity: f32) -> [u8; 4] {
        let mix = |i: usize| (self.card[i] as f32 + (toward[i] as f32 - self.card[i] as f32) * k).round() as u8;
        [mix(0), mix(1), mix(2), (self.card[3] as f32 * opacity.clamp(0.0, 1.0)).round() as u8]
    }
}

pub(super) fn text(c: [u8; 4]) -> [u8; 4] {
    [c[0], c[1], c[2], 0]
}

pub(super) fn tint(c: [u8; 4], alpha: u8) -> [u8; 4] {
    [c[0], c[1], c[2], alpha]
}

pub(super) fn delay_color(seconds: f32) -> [u8; 4] {
    if seconds > 59.0 {
        LATE
    } else if seconds < -59.0 {
        EARLY
    } else {
        ON_TIME
    }
}

impl Ui {
    pub(super) fn shown_size(&self, k: usize, size: f32) -> f32 {
        size.min(self.caps[k])
    }

    pub(super) fn hud_grip(&mut self, r: &Renderer, scene: &mut Scene, k: usize, rect: [f32; 4], used: f32, f: &super::Frame, s: f32) {
        let cap = fit_cap(used, rect, [0.0, 0.0, f.width, f.height - 8.0 * s]);
        if (cap - self.caps[k]).abs() > cap.min(self.caps[k]) * 0.02 {
            self.caps[k] = cap;
        }
        if f.vr || crate::platform::touch_controls() {
            return;
        }
        let n = (13.0 * s).round().max(8.0) as u32;
        let dim = self.hud.dim;
        let tex = self.text.cached(r, scene, ("\u{0}grip".to_string(), n, dim), (n, n), || grip_pixels(n, n as f32 / 13.0, dim));
        let (x1, y1) = ((rect[2] - 4.0 * s).round(), (rect[3] - 4.0 * s).round());
        scene.overlays.push((tex, [x1 - n as f32, y1 - n as f32, x1, y1]));
        self.grips[k] = Some((rect, 24.0 * s, used));
    }

    pub fn grip_at(&self, x: f32, y: f32) -> Option<(usize, f32, f32)> {
        self.grips.iter().enumerate().find_map(|(k, g)| {
            let (r, zone, used) = (*g)?;
            (x > r[2] - zone && x <= r[2] && y > r[3] - zone && y <= r[3]).then_some((k, (r[2] - r[0]).max(100.0), used))
        })
    }

    pub(super) fn below_info(&self, x0: f32, x1: f32, top: f32, s: f32) -> f32 {
        match self.info_rect {
            Some(bar) if bar[0] < x1 && bar[2] > x0 => top.max(bar[3] + 10.0 * s),
            _ => top,
        }
    }

    pub(super) fn hud_card(&mut self, r: &Renderer, scene: &mut Scene, rect: [f32; 4], radius: f32, opacity: f32, s: f32) {
        let k = opacity.clamp(0.0, 1.0);
        let card = self.hud.card;
        self.text.shadow(r, scene, rect, radius, 8.0 * s, 2.0 * s, (42.0 * k).round() as u8);
        self.text.rounded(r, scene, rect, radius, tint(card, (card[3] as f32 * k).round() as u8));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cards_follow_the_navigator_look() {
        let dark = Hud::new(false);
        let light = Hud::new(true);
        assert_eq!(dark.ink, [242, 242, 247, 255]);
        assert_eq!(light.ink, [28, 28, 30, 255]);
        assert_eq!(dark.card, [30, 30, 32, 240]);
        assert_eq!(light.card[..3], [255, 255, 255]);
        assert_eq!(LATE, [255, 69, 58, 255]);
        assert_eq!(dark.shade(dark.ink, 0.0, 1.0), dark.card);
        assert_eq!(light.shade([0, 0, 0, 255], 0.5, 0.5)[..], [128, 128, 128, 120]);
        assert_eq!(delay_color(120.0), LATE);
        assert_eq!(delay_color(-120.0), EARLY);
        assert_eq!(delay_color(30.0), ON_TIME);
    }

    #[test]
    fn a_window_keeps_to_its_sizes_and_its_grip_draws_three_strokes() {
        assert_eq!(window_size(5.0), 2.0);
        assert_eq!(window_size(0.1), 0.6);
        assert_eq!(window_size(f32::NAN), 1.0);
        let px = grip_pixels(13, 1.0, [255, 255, 255, 255]);
        let alpha = |x: u32, y: u32| px[((y * 13 + x) * 4 + 3) as usize];
        assert!(alpha(12, 12) == 0 && alpha(0, 0) == 0);
        assert!(alpha(12, 10) > 100 && alpha(12, 6) > 100 && alpha(12, 2) > 100);
        assert_eq!(alpha(12, 8), 0);
    }

    #[test]
    fn a_window_too_tall_for_a_short_screen_is_shrunk_until_its_grip_is_on_it() {
        let screen = [0.0, 0.0, 1280.0, 712.0];
        let below_large_bar = 170.0;
        let tall = [600.0, below_large_bar, 1400.0, below_large_bar + 714.0];
        let cap = fit_cap(2.0, tall, screen);
        assert!(cap < 2.0 && cap >= 0.6);
        let h = 714.0 * cap / 2.0;
        let w = 800.0 * cap / 2.0;
        assert!(below_large_bar + h <= screen[3] + 0.5 && w <= screen[2]);
        let roomy = [600.0, 60.0, 900.0, 300.0];
        assert!(fit_cap(1.0, roomy, screen) > 2.0);
        assert!(fit_cap(2.0, [0.0, 700.0, 100.0, 900.0], screen) >= 0.3);
    }
}
