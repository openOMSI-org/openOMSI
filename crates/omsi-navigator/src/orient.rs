//! How the city map is turned: the compass, and what it says.

use super::*;

impl Navigator {
    /// The turn of the view for this frame (radians, see `CityMap::phi`): as the map is drawn, real north up (where the
    /// package says which way it lies) or the heading up, eased so that the view turns smoothly (`dt` seconds).
    pub(super) fn ease_turn(&mut self, heading: f64, dt: f64) -> f64 {
        let want = match self.orient {
            1 => self.surface.as_ref().map_or(0.0, |sf| sf.north() as f64),
            2 => heading,
            _ => 0.0,
        }
        .to_radians();
        let gap = (want - self.city.phi + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
        self.city.phi = if gap.abs() < 1e-3 { want } else { self.city.phi + gap * (1.0 - (-dt * 8.0).exp()) };
        self.city.phi
    }

    /// The compass, bottom right: where real north lies on the screen (red tip), and how the map is turned now (as it is
    /// drawn, real north up, or the bus's heading up). A click on it changes how (see `orient`).
    pub(super) fn draw_compass(&mut self, ui: &mut Painter, w: f32, h: f32, s: f32) {
        let pal = super::style::palette(self.light);
        let r = 24.0 * s;
        let c = Vec2::new(w - 36.0 * s - r, h - 62.0 * s - r);
        let north = self.surface.as_ref().map_or(0.0, |sf| sf.north() as f64);
        let disc = Rect::new(c.x - r, c.y - r, 2.0 * r, 2.0 * r);
        ui.shadow(disc, r, 8.0 * s, Color::rgba(0, 0, 0, 0.14));
        ui.rounded(disc, r, pal.card);
        // where north lies on the screen: clockwise from the screen's up
        let ang = (north.to_radians() - self.city.phi) as f32;
        let dir = Vec2::new(ang.sin(), -ang.cos());
        let side = Vec2::new(-dir.y, dir.x);
        let red = Color::rgba(226, 70, 62, 1.0);
        let grey = pal.card_dim;
        ui.tri(c + dir * (0.72 * r), c + side * (0.24 * r), c - side * (0.24 * r), red, red, red);
        ui.tri(c - dir * (0.72 * r), c + side * (0.24 * r), c - side * (0.24 * r), grey, grey, grey);
        ui.text(&mut self.atlas, &self.fonts, "N", 10.5 * s, Weight::Bold, c + dir * (1.2 * r) + Vec2::new(0.0, 4.0 * s), Align::Center, pal.card_text);
        let name = match self.orient {
            1 => omsi_ui::tr("North"),
            2 => omsi_ui::tr("Heading"),
            _ => omsi_ui::tr("Map"),
        };
        ui.text(&mut self.atlas, &self.fonts, &name, 10.5 * s, Weight::Medium, Vec2::new(c.x, c.y + r + 13.0 * s), Align::Center, pal.card_text);
        self.city.buttons.push((Rect::new(disc.x, disc.y, disc.w, disc.h + 16.0 * s), 7));
    }
}
