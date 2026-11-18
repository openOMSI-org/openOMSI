//! Personal cockpit navigator placement in VR, stored per vehicle outside the repository.

use glam::{DVec3, Mat4, Quat, Vec3, Vec4};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Placement {
    pub enabled: bool,
    /// Metres relative to the bus's standard driver camera, in vehicle axes.
    pub offset: [f32; 3],
    pub width: f32,
    pub yaw: f32,
    pub tilt: f32,
    pub roll: f32,
    pub opacity: f32,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            enabled: false,
            offset: [0.28, 0.65, -0.35],
            width: 0.28,
            yaw: -23.0,
            tilt: 26.0,
            roll: 0.0,
            opacity: 0.95,
        }
    }
}

impl Placement {
    pub fn value(&self, field: &str) -> Option<f32> {
        Some(match field {
            "x" => self.offset[0], "y" => self.offset[1], "z" => self.offset[2],
            "width" => self.width, "yaw" => self.yaw, "tilt" => self.tilt,
            "roll" => self.roll, "opacity" => self.opacity,
            _ => return None,
        })
    }

    pub fn set_value(&mut self, field: &str, value: f32) {
        match field {
            "x" => self.offset[0] = value, "y" => self.offset[1] = value,
            "z" => self.offset[2] = value, "width" => self.width = value,
            "yaw" => self.yaw = value, "tilt" => self.tilt = value,
            "roll" => self.roll = value, "opacity" => self.opacity = value,
            _ => return,
        }
        *self = self.sanitize();
    }

    pub fn scroll(&mut self, amount: f32, resize: bool, eye: Vec3, driver: Vec3) {
        if !amount.is_finite() {
            return;
        }
        if resize {
            self.width *= 1.08_f32.powf(amount.clamp(-20.0, 20.0));
        } else {
            let ray = driver + Vec3::from(self.offset) - eye;
            let distance = ray.length();
            if distance > 0.001 {
                let new_distance = (distance - amount * 0.04).clamp(0.25, 2.5);
                self.offset = (eye + ray / distance * new_distance - driver).to_array();
            }
        }
        *self = self.sanitize();
    }

    pub fn sanitize(mut self) -> Self {
        let default = Self::default();
        let clamp = |v: f32, fallback: f32, lo: f32, hi: f32| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                fallback
            }
        };
        for i in 0..3 {
            self.offset[i] = clamp(self.offset[i], default.offset[i], -2.0, 2.0);
        }
        self.width = clamp(self.width, default.width, 0.12, 0.65);
        self.yaw = clamp(self.yaw, default.yaw, -180.0, 180.0);
        self.tilt = clamp(self.tilt, default.tilt, -80.0, 80.0);
        self.roll = clamp(self.roll, default.roll, -180.0, 180.0);
        self.opacity = clamp(self.opacity, default.opacity, 0.3, 1.0);
        self
    }

    pub fn adjust(&mut self, field: &str, direction: f32) {
        match field {
            "x" => self.offset[0] += direction * 0.02,
            "y" => self.offset[1] += direction * 0.02,
            "z" => self.offset[2] += direction * 0.02,
            "width" => self.width += direction * 0.02,
            "yaw" => self.yaw += direction * 2.0,
            "tilt" => self.tilt += direction * 2.0,
            "roll" => self.roll += direction * 2.0,
            "opacity" => self.opacity += direction * 0.05,
            "enabled" => self.enabled = !self.enabled,
            "reset" => *self = Self { enabled: self.enabled, ..Self::default() },
            _ => return,
        }
        *self = self.sanitize();
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Profiles {
    pub buses: BTreeMap<String, Placement>,
}

impl Profiles {
    /// Read the profiles from `path` (the app's `vr-navigator.json`; None: no settings folder).
    pub fn load(path: Option<&Path>) -> Self {
        let Some(path) = path else {
            return Self::default();
        };
        match std::fs::read(&path) {
            Ok(data) => match serde_json::from_slice::<Self>(&data) {
                Ok(mut profiles) => {
                    for value in profiles.buses.values_mut() {
                        *value = value.sanitize();
                    }
                    profiles
                }
                Err(e) => {
                    log::warn!("VR navigator: cannot read {}: {e}", path.display());
                    Self::default()
                }
            },
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("VR navigator: {}: {e}", path.display());
                }
                Self::default()
            }
        }
    }

    pub fn get(&self, key: &str) -> Placement {
        self.buses.get(key).copied().unwrap_or_default()
    }

    /// Write the profiles to `path` (as `load` takes it).
    pub fn save(&self, path: Option<&Path>) -> anyhow::Result<()> {
        let path = path.ok_or_else(|| anyhow::anyhow!("no user settings directory"))?;
        self.save_to(path)
    }

    fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(path.parent().unwrap())?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(temp, path)?;
        Ok(())
    }
}

/// The key of a bus in the profiles: its path below the installation, in lower case.
pub fn bus_key(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase()
}

/// A display is anchored to the vehicle, independently of view, seat and HMD recentering.
#[derive(Clone, Copy)]
pub struct Display {
    pub placement: Placement,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub local_center: Vec3,
}

impl Display {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn transform(
        &self,
        bus: DVec3,
        body: Mat4,
        eye: &omsi_render::Camera,
        projection: Mat4,
        aspect: f32,
    ) -> Mat4 {
        let rotation = Quat::from_rotation_z(self.placement.yaw.to_radians())
            * Quat::from_rotation_x(-self.placement.tilt.to_radians())
            * Quat::from_rotation_y(self.placement.roll.to_radians());
        let right = body.transform_vector3(rotation * Vec3::X) * self.placement.width * 0.5;
        let up = body.transform_vector3(rotation * Vec3::Z) * self.placement.width / aspect * 0.5;
        let origin = bus + body.transform_point3(self.local_center).as_dvec3();
        let plane = Mat4::from_cols(
            right.extend(0.0),
            up.extend(0.0),
            Vec4::Z,
            (origin - eye.position).as_vec3().extend(1.0),
        );
        projection * Mat4::look_to_rh(Vec3::ZERO, eye.forward(), eye.up()) * plane
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_values_keep_visibility_and_clamp_to_placement_limits() {
        let mut p = Placement::default();
        p.set_value("x", 0.42);
        p.set_value("opacity", 0.7);
        assert_eq!(p.value("x"), Some(0.42));
        assert_eq!(p.value("opacity"), Some(0.7));
        assert!(!p.enabled);
        p.set_value("width", 20.0);
        p.set_value("tilt", -100.0);
        assert_eq!(p.width, 0.65);
        assert_eq!(p.tilt, -80.0);
        let saved = p;
        p.set_value("unknown", 1.0);
        assert_eq!(p, saved);
    }

    #[test]
    fn navigator_starts_hidden_and_reset_preserves_visibility() {
        let mut p = Placement::default();
        assert!(!p.enabled);
        p.adjust("enabled", 1.0);
        p.adjust("x", 1.0);
        p.adjust("reset", 1.0);
        assert!(p.enabled);
        assert_eq!(p.offset, Placement::default().offset);
    }

    fn camera(position: DVec3, yaw: f32) -> omsi_render::Camera {
        omsi_render::Camera {
            position,
            yaw,
            pitch: 0.0,
            roll: 0.0,
            fov_deg: 90.0,
            near: 0.1,
            far: 1000.0,
        }
    }

    #[test]
    fn cockpit_panel_follows_the_bus_without_drifting() {
        let display = Display {
            placement: Placement::default(),
            local_center: Vec3::new(0.3, 1.0, -0.3),
        };
        let projection = Mat4::perspective_rh(90.0_f32.to_radians(), 1.0, 1000.0, 0.1);
        let first = display.transform(
            DVec3::ZERO,
            Mat4::IDENTITY,
            &camera(DVec3::ZERO, 0.0),
            projection,
            1.2,
        );
        // Heading is clockwise, whereas the vehicle rotation uses mathematical angles.
        let position = DVec3::new(1_000_000.0, -2_000_000.0, 15.0);
        let moved = display.transform(
            position,
            Mat4::from_rotation_z(-0.7),
            &camera(position, 0.7_f32.to_degrees()),
            projection,
            1.2,
        );
        assert!(first.abs_diff_eq(moved, 0.00001));
        let looked = display.transform(
            DVec3::ZERO,
            Mat4::IDENTITY,
            &camera(DVec3::ZERO, 25.0),
            projection,
            1.2,
        );
        assert!(
            !first.abs_diff_eq(looked, 0.01),
            "the panel must not follow head rotation"
        );
    }

    #[test]
    fn stereo_panel_keeps_aspect_and_has_parallax() {
        let placement = Placement {
            yaw: 0.0,
            tilt: 0.0,
            roll: 0.0,
            ..Placement::default()
        };
        let display = Display {
            placement,
            local_center: Vec3::Y,
        };
        let projection = Mat4::perspective_rh(90.0_f32.to_radians(), 1.0, 1000.0, 0.1);
        let left = display.transform(
            DVec3::ZERO,
            Mat4::IDENTITY,
            &camera(DVec3::new(-0.032, 0.0, 0.0), 0.0),
            projection,
            1.4,
        );
        let right = display.transform(
            DVec3::ZERO,
            Mat4::IDENTITY,
            &camera(DVec3::new(0.032, 0.0, 0.0), 0.0),
            projection,
            1.4,
        );
        assert!(left.project_point3(Vec3::ZERO).x > right.project_point3(Vec3::ZERO).x);
        let width = (left.project_point3(Vec3::X) - left.project_point3(-Vec3::X)).length();
        let height = (left.project_point3(Vec3::Y) - left.project_point3(-Vec3::Y)).length();
        assert!((width / height - 1.4).abs() < 0.0001);
    }

    #[test]
    fn wheel_changes_depth_and_ctrl_wheel_only_changes_size() {
        let mut p = Placement::default();
        let original = p;
        let direction = Vec3::from(p.offset).normalize();
        p.scroll(2.0, false, Vec3::ZERO, Vec3::ZERO);
        assert!(
            (Vec3::from(original.offset).length() - Vec3::from(p.offset).length() - 0.08).abs()
                < 0.0001
        );
        assert!(direction.abs_diff_eq(Vec3::from(p.offset).normalize(), 0.0001));
        assert_eq!(p.width, original.width);
        let moved = p;
        p.scroll(2.0, true, Vec3::ZERO, Vec3::ZERO);
        assert_eq!(p.offset, moved.offset);
        assert!(p.width > moved.width);
        p.scroll(1000.0, false, Vec3::ZERO, Vec3::ZERO);
        assert!((Vec3::from(p.offset).length() - 0.25).abs() < 0.0001);
        p.scroll(f32::NAN, true, Vec3::ZERO, Vec3::ZERO);
        assert!(p.width.is_finite());
    }

    #[test]
    fn profiles_survive_replacing_the_file_and_stay_separate_per_bus() {
        let dir = std::env::temp_dir().join(format!(
            "openomsi-vr-nav-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("profiles.json");
        let mut profiles = Profiles::default();
        let a = bus_key(Path::new("OMSI/Vehicles/Mod/Bus.bus"), Path::new("OMSI"));
        let b = bus_key(
            Path::new("OMSI/Vehicles/Payware/Bus.bus"),
            Path::new("OMSI"),
        );
        assert_ne!(a, b);
        let mut placement = Placement::default();
        placement.adjust("x", -1.0);
        profiles.buses.insert(a.clone(), placement);
        profiles.save_to(&path).unwrap();
        profiles.buses.insert(
            b.clone(),
            Placement {
                enabled: false,
                ..Placement::default()
            },
        );
        profiles.save_to(&path).unwrap();
        let restored: Profiles = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(restored.get(&a), placement);
        assert!(!restored.get(&b).enabled);
        assert_eq!(restored.get("another bus"), Placement::default());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
