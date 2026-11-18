//! Personal cockpit navigator placement in VR: the editor and the app's side of the
//! placements (their data and drawing transform are `omsi_xr::navigator`).

use glam::Vec3;
pub(crate) use omsi_xr::navigator::{bus_key, Display, Placement, Profiles};

/// Where the placements are kept: beside the settings.
pub(crate) fn profiles_path() -> Option<std::path::PathBuf> {
    crate::settings::Settings::path().map(|p| p.with_file_name("vr-navigator.json"))
}

fn driver_origin(camera: &omsi_vehicle::Camera) -> Vec3 {
    let (sy, cy) = camera.yaw.to_radians().sin_cos();
    let (sp, cp) = camera.pitch.to_radians().sin_cos();
    Vec3::from(camera.pos) - Vec3::new(sy * cp, cy * cp, sp) * camera.dist.max(0.0)
}

pub(crate) struct Editing {
    pub moving: bool,
    pub rotating: bool,
    paused_before: bool,
    mouse_drive_before: bool,
}

impl crate::App {
    pub(crate) fn start_vr_nav_edit(&mut self) {
        if !self.vr_active() || self.player.is_none() || self.xr.vr_nav_edit.is_some() {
            return;
        }
        self.on_left(false);
        // The editor consumes key-up events; release existing driving and script
        // inputs before entering it so none remain held when driving resumes.
        if let Some(p) = self.player.as_mut() {
            let held: Vec<_> = p.held_keys.keys().copied().collect();
            for scan in held {
                p.key(scan, 0, false);
            }
            p.axes.release_all();
            for name in self.input.door_key_triggers.drain().flat_map(|(_, names)| names) {
                let off = format!("{name}_off");
                if p.vehicle.ty.program.trigger(&off).is_some() {
                    p.vehicle.trigger(&off);
                }
            }
        }
        self.input.buttons_held = (false, false);
        if self.menus.game_menu.is_some() {
            self.close_game_menu();
        }
        self.menus.chooser = None;
        self.menus.admin_list = None;
        self.menus.list_kind = None;
        self.menus.dropdown = None;
        self.menus.menu_drag = None;
        self.menus.menu_edit = None;
        self.view = "driver".into();
        self.xr.vr_nav_edit = Some(Editing {
            moving: false,
            rotating: false,
            paused_before: self.paused,
            mouse_drive_before: self.input.mouse_drive,
        });
        if self.net.lan.is_none() {
            self.paused = true;
        }
        self.input.mouse_drive = false;
        self.input.mouse_look = false;
        self.input.both_drag = None;
        self.input.keys.clear();
        self.menus.hover_key = None;
        #[cfg(windows)]
        {
            self.xr.vr_zoom_active = false;
            self.reset_vr_pointer();
        }
        if let Some(n) = self.menus.navigator.as_mut() {
            if n.map_open() {
                n.toggle_map();
            }
        }
        if let Some(p) = self.player.as_ref() {
            let key = bus_key(&p.vehicle.ty.def.path, &self.args.root);
            self.xr.vr_nav_profiles.buses.entry(key).or_default().enabled = true;
        }
        if let Some(window) = self.window.as_ref() {
            let _ = window.set_cursor_grab(winit::window::CursorGrabMode::Confined);
            window.set_cursor_visible(false);
        }
        self.input.cursor_hidden = None;
    }

    pub(crate) fn finish_vr_nav_edit(&mut self) {
        let Some(edit) = self.xr.vr_nav_edit.take() else {
            return;
        };
        self.paused = edit.paused_before;
        self.input.mouse_drive = edit.mouse_drive_before;
        self.input.mouse_look = false;
        self.menus.hover_key = None;
        self.input.keys.clear();
        self.input.cursor_hidden = None;
        #[cfg(windows)]
        self.reset_vr_pointer();
        if let Some(window) = self.window.as_ref() {
            let _ = window.set_cursor_grab(winit::window::CursorGrabMode::None);
            window.set_cursor_visible(true);
            if edit.mouse_drive_before {
                let _ = window.set_cursor_position(winit::dpi::PhysicalPosition::new(
                    self.input.cursor.0 as f64,
                    self.input.cursor.1 as f64,
                ));
            }
        }
        self.save_vr_nav_profiles();
    }

    /// The headset's axes in vehicle space, so a drag follows the direction the
    /// driver is looking while the resulting placement stays attached to the bus.
    fn vr_nav_edit_geometry(&self) -> Option<(Vec3, Vec3, Vec3, Vec3)> {
        let p = self.player.as_ref()?;
        let def = &p.vehicle.ty.def;
        let driver = def
            .cameras_driver
            .get(def.camera_std)
            .or(def.cameras_driver.first())?;
        let eye = *self.camera.as_ref()?;
        #[cfg(windows)]
        let eye = self
            .xr
            .vr
            .as_ref()
            .and_then(|vr| vr.navigator_edit_camera())
            .unwrap_or(eye);
        let inverse = p.vehicle.body_rotation().inverse();
        Some((
            inverse.transform_vector3(eye.right()),
            inverse.transform_vector3(eye.up()),
            inverse.transform_point3((eye.position - p.vehicle.position).as_vec3()),
            driver_origin(driver),
        ))
    }

    pub(crate) fn vr_nav_drag(&mut self, dx: f32, dy: f32) {
        let Some(edit) = self.xr.vr_nav_edit.as_ref() else {
            return;
        };
        if !edit.moving && !edit.rotating {
            return;
        }
        let (moving, rotating) = (edit.moving, edit.rotating);
        let Some((right, up, eye, driver)) = self.vr_nav_edit_geometry() else {
            return;
        };
        let key = bus_key(
            &self.player.as_ref().unwrap().vehicle.ty.def.path,
            &self.args.root,
        );
        let p = self.xr.vr_nav_profiles.buses.entry(key).or_default();
        if moving {
            let distance = (driver + Vec3::from(p.offset) - eye)
                .length()
                .clamp(0.25, 2.5);
            p.offset =
                (Vec3::from(p.offset) + (right * dx - up * dy) * distance * 0.0015).to_array();
        }
        if rotating {
            use winit::keyboard::KeyCode;
            if self.input.keys.contains(&KeyCode::ShiftLeft) || self.input.keys.contains(&KeyCode::ShiftRight) {
                p.roll += dx * 0.25;
            } else {
                p.yaw -= dx * 0.25;
                p.tilt -= dy * 0.25;
            }
        }
        *p = p.sanitize();
    }

    pub(crate) fn vr_nav_scroll(&mut self, amount: f32) {
        if !amount.is_finite() {
            return;
        }
        let Some((_, _, eye, driver)) = self.vr_nav_edit_geometry() else {
            return;
        };
        let key = bus_key(
            &self.player.as_ref().unwrap().vehicle.ty.def.path,
            &self.args.root,
        );
        let p = self.xr.vr_nav_profiles.buses.entry(key).or_default();
        use winit::keyboard::KeyCode;
        let resize =
            self.input.keys.contains(&KeyCode::ControlLeft) || self.input.keys.contains(&KeyCode::ControlRight);
        p.scroll(amount, resize, eye, driver);
    }

    fn save_vr_nav_profiles(&mut self) {
        if let Err(e) = self.xr.vr_nav_profiles.save(profiles_path().as_deref()) {
            log::warn!("VR navigator: saving placement: {e}");
            self.service_msg = Some((
                format!("{}: {e}", omsi_ui::tr("Could not save navigator position")),
                5.0,
            ));
        }
    }
    pub(crate) fn vr_nav_profile(&self) -> Placement {
        self.player
            .as_ref()
            .map(|p| {
                self.xr.vr_nav_profiles
                    .get(&bus_key(&p.vehicle.ty.def.path, &self.args.root))
            })
            .unwrap_or_default()
    }

    pub(crate) fn vr_nav_display(&self) -> Option<Display> {
        if !self.vr_active() || self.view != "driver" {
            return None;
        }
        let p = self.player.as_ref()?;
        let def = &p.vehicle.ty.def;
        let eye = def
            .cameras_driver
            .get(def.camera_std)
            .or(def.cameras_driver.first())?;
        let placement = self.vr_nav_profile();
        Some(Display {
            placement,
            local_center: driver_origin(eye) + Vec3::from(placement.offset),
        })
    }

    pub(crate) fn vr_nav_adjust(&mut self, field: &str, direction: f32) {
        self.update_vr_nav_profile(|p| p.adjust(field, direction));
    }

    pub(crate) fn vr_nav_set(&mut self, field: &str, value: f32) {
        self.update_vr_nav_profile(|p| p.set_value(field, value));
    }

    fn update_vr_nav_profile(&mut self, update: impl FnOnce(&mut Placement)) {
        if !self.vr_active() {
            return;
        }
        let Some(player) = self.player.as_ref() else {
            return;
        };
        let key = bus_key(&player.vehicle.ty.def.path, &self.args.root);
        update(self.xr.vr_nav_profiles.buses.entry(key).or_default());
        if self.xr.vr_nav_edit.is_none() {
            self.save_vr_nav_profiles();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigator_translations_are_loaded_for_all_languages() {
        let keys = [
            "Navigator position (this bus)",
            "Position right / left",
            "Position forward / back",
            "Position up / down",
            "Display width",
            "Display rotation",
            "Display tilt",
            "Display roll",
            "Move and rotate with the mouse...",
            "Positioning navigator - changes apply to this bus",
            "Hold left mouse: move | Hold right mouse: rotate",
            "Wheel: distance | Ctrl+wheel: size | Shift+right drag: roll",
            "Esc / Enter: save and finish | R: reset position",
            "Reset navigator position",
            "VR: Toggle navigator",
            "VR: Position navigator",
            "Could not save navigator position",
        ];
        for &(_, _, language, _, _) in omsi_launcher_lib::LANGUAGES.iter().filter(|l| l.2 != "en") {
            for key in keys {
                let translated = crate::_rust_i18n_try_translate(language, key);
                assert!(
                    translated.as_ref().is_some_and(|text| !text.trim().is_empty() && text.as_ref() != key),
                    "Missing navigator translation: {language} / {key}"
                );
            }
        }
    }

    #[test]
    fn standard_driver_camera_distance_is_respected() {
        let camera = omsi_vehicle::Camera {
            pos: [1.0, 2.0, 3.0],
            yaw: 90.0,
            pitch: 0.0,
            dist: 0.4,
            ..Default::default()
        };
        assert!(driver_origin(&camera).abs_diff_eq(Vec3::new(0.6, 2.0, 3.0), 0.0001));
    }

}
