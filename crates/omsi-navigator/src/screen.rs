//! The small map on a screen of the player's bus: a mod's `[navigatortexture]` (see
//! `docs/NAVIGATORTEXTURE.md`). It is drawn independently of the panel - hiding the panel
//! leaves the screen on - and is black while the variable that powers it reads 0.

use super::panel::PanelLook;
use super::*;

/// The map and the place row under it fit this many layout points high.
const SCREEN_POINTS_HIGH: f32 = 300.0;

/// A texture made by `Renderer::add_render_texture`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Screen {
    pub texture: TextureId,
    pub size: (u32, u32),
    pub on: bool,
}

impl Screen {
    /// The model's first `[navigatortexture]`, its texture among a vehicle's script textures.
    pub fn find(model: &omsi_model::Model, textures: &[Option<TextureId>], var: impl Fn(&str) -> Option<f32>) -> Option<Screen> {
        let def = model.navigator_textures.first()?;
        let texture = textures.get(def.script_index).copied().flatten()?;
        Some(Screen { texture, size: (def.width.max(1) as u32, def.height.max(1) as u32), on: def.is_on(var) })
    }

    pub fn of_vehicle(vehicle: &omsi_sim::VehicleInstance, render: &omsi_scene::VehicleRender) -> Option<Screen> {
        Self::find(&vehicle.ty.model, &render.script_textures, |name| vehicle.var(name))
    }

    fn map_height(&self, below: f32) -> f32 {
        let h = self.size.1 as f32;
        (h - below).max(h * 0.5).round()
    }
}

#[derive(Default)]
pub(super) struct ScreenState {
    /// As last drawn or blacked out: a change is drawn at once.
    shown: Option<Screen>,
    frames_since_drawn: u32,
    /// Held only while a screen is on, so a bus without one costs no video memory.
    attachments: Option<omsi_ui::Attachments>,
}

impl Navigator {
    pub fn set_screen(&mut self, screen: Option<Screen>) {
        self.screen = screen;
    }

    pub(super) fn screen_wants_frame(&self) -> bool {
        self.screen.is_some_and(|s| s.on) || self.screen != self.screen_state.shown
    }

    pub(super) fn frame_screen(&mut self, renderer: &Renderer, scene: &Scene, f: &NavFrame) {
        let Some(screen) = self.screen.filter(|s| s.on) else {
            if let Some(off) = self.screen.filter(|_| self.screen != self.screen_state.shown) {
                renderer.clear_texture(scene, off.texture, wgpu::Color::BLACK);
            }
            self.screen_state = ScreenState { shown: self.screen, ..ScreenState::default() };
            return;
        };
        let changed = self.screen_state.shown != Some(screen);
        let state = &mut self.screen_state;
        state.frames_since_drawn = state.frames_since_drawn.saturating_add(1);
        // The city map covers the cockpit.
        if !changed && (self.city.open || state.frames_since_drawn < self.draw_every) {
            return;
        }
        // A GPU that cannot make the texture as large as declared made it smaller.
        let (Some(view), Some((w, h, _))) = (renderer.texture_view(scene, screen.texture), renderer.texture_levels(scene, screen.texture)) else { return };
        state.frames_since_drawn = 0;
        let mut attachments = state.attachments.take().unwrap_or_default();
        let drawn = Screen { size: (w, h), ..screen };
        let look = PanelLook::screen(drawn.size, SCREEN_POINTS_HIGH);
        let map_h = drawn.map_height(self.below_map(look.scale, f.stops.len()));
        self.ensure_gpu(renderer);
        self.draw(renderer, &view, drawn.size, map_h, &look, Some(&mut attachments), f);
        self.screen_state.attachments = Some(attachments);
        self.screen_state.shown = Some(screen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(text: &str) -> omsi_model::Model {
        omsi_model::Model::parse(&omsi_cfg::CfgFile::from_str("model.cfg", text))
    }

    #[test]
    fn a_vehicle_screen_is_its_first_navigator_texture() {
        let m = model("[scripttexture]\n64\n32\n[navigatortexture]\n800\n480\npower\n[navigatortexture]\n10\n10\n");
        let textures = [Some(7), Some(9), Some(11)];
        let on = Screen::find(&m, &textures, |_| Some(1.0)).unwrap();
        assert_eq!(on, Screen { texture: 9, size: (800, 480), on: true });
        assert!(!Screen::find(&m, &textures, |_| Some(0.0)).unwrap().on);
        assert!(Screen::find(&m, &[Some(7), None], |_| None).is_none(), "its texture is not there");
        assert!(Screen::find(&model("[scripttexture]\n64\n32\n"), &textures, |_| None).is_none());
    }

    #[test]
    fn the_map_leaves_room_under_it_but_keeps_half_the_screen() {
        let s = Screen { texture: 0, size: (800, 480), on: true };
        assert_eq!(s.map_height(40.0), 440.0);
        assert_eq!(s.map_height(400.0), 240.0);
    }

    #[test]
    fn a_screen_is_laid_out_for_its_shorter_way() {
        let wide = PanelLook::screen((800, 240), SCREEN_POINTS_HIGH);
        assert!((wide.scale - 0.8).abs() < 1e-6, "held by the height");
        let tall = PanelLook::screen((360, 1000), SCREEN_POINTS_HIGH);
        assert!((tall.scale - 1.0).abs() < 1e-6, "held by the width");
        assert_eq!((tall.radius, tall.fade, tall.backdrop, tall.grip), (0.0, 1.0, 1.0, false));
    }
}
