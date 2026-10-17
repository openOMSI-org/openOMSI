//! The relief of the city map (when the map's package carries contour lines): the terrain button, and the heights
//! written on the lines.

use super::*;

impl Navigator {
    /// The Terrain button, only where the package has contour lines: `bs` high at `y`, its right edge at `right`;
    /// returns the new `right`. A click turns the terrain view on or off (see `show_contours`).
    pub(super) fn draw_terrain_button(&mut self, ui: &mut Painter, y: f32, bs: f32, right: f32, s: f32) -> f32 {
        if !self.surface.as_ref().is_some_and(|sf| sf.has_contours()) {
            return right;
        }
        let pal = super::style::palette(self.light);
        let label = omsi_ui::tr("Terrain");
        let bw = (self.fonts.width(&label, 13.5 * s, Weight::Bold) + 44.0 * s).max(96.0 * s);
        let r = Rect::new(right - bw, y, bw, bs);
        ui.shadow(r, bs * 0.5, 8.0 * s, Color::rgba(0, 0, 0, 0.14));
        ui.rounded(r, bs * 0.5, if self.show_contours { pal.route } else { pal.card });
        let ink = if self.show_contours { Color::WHITE } else { pal.card_text };
        // (a mountain: two peaks)
        let (c, k) = (Vec2::new(r.x + 20.0 * s, r.center().y + 2.0 * s), 6.5 * s);
        ui.tri(c + Vec2::new(-k * 1.3, k), c + Vec2::new(-k * 0.2, -k), c + Vec2::new(k * 0.9, k), ink, ink, ink);
        ui.tri(c + Vec2::new(-k * 0.1, k), c + Vec2::new(k * 0.7, -k * 0.5), c + Vec2::new(k * 1.5, k), ink, ink, ink);
        ui.text_in(&mut self.atlas, &self.fonts, &label, 13.5 * s, Weight::Bold, Rect::new(r.x + 32.0 * s, r.y, r.w - 34.0 * s, r.h), Align::Center, ink);
        self.city.buttons.push((r, 6));
        r.x - 8.0 * s
    }

    /// The strength of the contour lines at this zoom (up close full, from far a faint one, where the streets are to
    /// stand out) and the band it belongs to (the ground is built again when the band changes); `None` with the
    /// terrain view off.
    pub(super) fn contour_band(&self) -> Option<(f32, u64)> {
        if !self.show_contours || !self.surface.as_ref().is_some_and(|sf| sf.has_contours()) {
            return None;
        }
        let band = if self.city.mpp < 4.0 {
            0
        } else if self.city.mpp < 12.0 {
            1
        } else {
            2
        };
        Some(([1.0, 0.5, 0.2][band], band as u64))
    }

    /// The height written on the contour lines, in metres: on every mark up close, from further away on the index
    /// lines only; each with a halo, none over another.
    pub(super) fn terrain_labels(&mut self, ui: &mut Painter, on_screen: &dyn Fn(DVec3) -> Vec2, win: Rect, s: f32) {
        let (Some(sf), true) = (self.surface.clone(), self.city.mpp < 40.0) else { return };
        if self.contour_band().is_none() {
            return;
        }
        let pal = super::style::palette(self.light);
        let (px, close) = (11.5 * s, self.city.mpp < 6.0);
        let col = pal.contour.alpha(0.9);
        let mut shown: Vec<Vec2> = Vec::new();
        for (at, height, index) in sf.contour_marks() {
            if !close && !*index {
                continue;
            }
            let p = on_screen(DVec3::new(at.0, at.1, 0.0));
            if !win.pad(30.0 * s, 30.0 * s).contains(p) || shown.iter().any(|q| (*q - p).length() < if close { 64.0 * s } else { 120.0 * s }) {
                continue;
            }
            shown.push(p);
            let (text, at) = (format!("{height} m"), p + Vec2::new(0.0, px * 0.35));
            for o in [Vec2::new(1.0, 0.0), Vec2::new(-1.0, 0.0), Vec2::new(0.0, 1.0), Vec2::new(0.0, -1.0)] {
                ui.text(&mut self.atlas, &self.fonts, &text, px, Weight::Medium, at + o * s, Align::Center, pal.label_halo);
            }
            ui.text(&mut self.atlas, &self.fonts, &text, px, Weight::Medium, at, Align::Center, col);
        }
    }
}
