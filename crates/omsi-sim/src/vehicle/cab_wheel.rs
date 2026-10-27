//! The cab's steering wheel turned as far as the player's own wheel (the `cab_wheel_rotation`
//! setting): the stock model.cfg turns it with an `anim_rot` on the steered axle's angle
//! (`Axle_Steering_0_L`, radians) times a factor the modder chose - 1450 on the SD200, 1680
//! on the SD202 and most add-on buses, 922 to 1680 elsewhere - so its rotation lock to lock
//! is that factor times twice the axle's full lock, and differs from bus to bus. With the
//! setting on, every such animation of the player's bus is scaled by one multiplier (the
//! wheel, its hub and the buttons on it keep turning together) so that the largest of them
//! turns the wheel as far as the setting says from lock to lock.

use super::VehicleInstance;
use omsi_model::{AnimKind, Animation};

/// The smallest factor (magnitude) of an `anim_rot` on `Axle_Steering_*` taken for a steering
/// wheel: a road wheel turns by the angle itself (57.3, radians to degrees), a steering
/// wheel by 15 to 30 times that (922 to 1680 in the add-ons seen).
pub const CAB_WHEEL_MIN_FACTOR: f32 = 300.0;

/// Whether `a` turns a steering wheel: a rotation on an axle's steering angle by a large
/// factor.
pub fn is_cab_wheel_anim(a: &Animation) -> bool {
    a.kind == Some(AnimKind::Rot) && a.factor.abs() >= CAB_WHEEL_MIN_FACTOR && a.variable.trim().to_ascii_lowercase().starts_with("axle_steering")
}

/// The multiplier for a steering wheel's animation factor `factor` (degrees per radian of the
/// axle) so that the wheel turns `lock_to_lock` degrees while the axle goes from one full
/// lock (`max_steer_rad`) to the other; `None` when either is not usable (the bus's own
/// animation stays).
pub fn cab_wheel_scale(factor: f32, max_steer_rad: f32, lock_to_lock: f32) -> Option<f32> {
    let own = factor.abs() * 2.0 * max_steer_rad;
    (own.is_finite() && own > 1.0 && lock_to_lock > 0.0).then(|| lock_to_lock / own)
}

impl VehicleInstance {
    /// Turn the cab's steering wheel `deg` degrees lock to lock (0: as the bus's model.cfg
    /// says). Only does something when the value changed: call it when the player's bus is
    /// set up and when the setting changes (AI buses keep their own animation).
    pub fn set_cab_wheel_rotation(&mut self, deg: u32) {
        if self.cab_wheel_rotation == deg {
            return;
        }
        self.cab_wheel_rotation = deg;
        self.apply_cab_wheel_rotation();
    }

    /// The cab wheel's rotation the animations were last set to (`set_cab_wheel_rotation`).
    pub fn cab_wheel_rotation(&self) -> u32 {
        self.cab_wheel_rotation
    }

    /// Scale the steering wheel's animations for `self.cab_wheel_rotation` (again after the
    /// animators were made anew), from the factors of the model.cfg each time, so that the
    /// scaling never builds up.
    pub(super) fn apply_cab_wheel_rotation(&mut self) {
        let deg = self.cab_wheel_rotation;
        let ty = self.ty.clone();
        let mut found: Vec<(usize, usize, f32)> = Vec::new();
        for (i, vm) in ty.meshes.iter().enumerate() {
            let def = &ty.model.meshes[vm.def_index];
            for (j, a) in def.animations.iter().enumerate() {
                if is_cab_wheel_anim(a) {
                    found.push((i, j, a.factor));
                }
            }
        }
        let max_steer_rad = self.physics.max_steer_deg.to_radians();
        let Some(&(wi, wj, factor)) = found.iter().max_by(|a, b| a.2.abs().total_cmp(&b.2.abs())) else {
            if deg != 0 {
                log::info!("cab wheel rotation {deg}°: {} has no steering wheel turned by Axle_Steering_* (factor >= {CAB_WHEEL_MIN_FACTOR}); its own animation stays", ty.def.path.display());
            }
            return;
        };
        let scale = if deg == 0 { Some(1.0) } else { cab_wheel_scale(factor, max_steer_rad, deg as f32) };
        let Some(scale) = scale else {
            log::info!("cab wheel rotation {deg}°: no usable steering lock ({:.1}°): the bus's own animation stays", self.physics.max_steer_deg);
            return;
        };
        for &(i, j, f) in &found {
            if let Some(anim) = self.animators.get_mut(i).and_then(|m| m.anims.get_mut(j)) {
                anim.0.factor = f * scale;
            }
        }
        let def = &ty.model.meshes[ty.meshes[wi].def_index];
        let own = factor.abs() * 2.0 * max_steer_rad;
        log::info!(
            "cab wheel rotation: mesh {wi} {} anim {wj} ({} x {factor}), axle lock {:.1}°: {own:.0}° lock to lock in the model, {} ({} animation(s) scaled x{scale:.3})",
            def.file,
            def.animations[wj].variable,
            self.physics.max_steer_deg,
            if deg == 0 { "kept".to_string() } else { format!("{deg}° now") },
            found.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scale_makes_the_wheel_turn_the_setting_lock_to_lock() {
        // SD200: 1450 per radian, 0.75 rad of lock each way = 2175° lock to lock; 1080° wanted
        let s = cab_wheel_scale(1450.0, 0.75, 1080.0).unwrap();
        assert!((s - 1080.0 / 2175.0).abs() < 1e-6, "{s}");
        assert!((1450.0 * s * 2.0 * 0.75 - 1080.0).abs() < 1e-3);
        // (the sign of the factor is the model's turning sense, kept by the multiplier)
        assert_eq!(cab_wheel_scale(-1450.0, 0.75, 1080.0), Some(s));
        assert_eq!(cab_wheel_scale(1450.0, 0.0, 1080.0), None);
        assert_eq!(cab_wheel_scale(1450.0, 0.75, 0.0), None);
    }

    #[test]
    fn only_large_rotations_on_the_steering_angle_are_a_steering_wheel() {
        let anim = |kind, variable: &str, factor| Animation { kind: Some(kind), variable: variable.into(), factor, ..Default::default() };
        assert!(is_cab_wheel_anim(&anim(AnimKind::Rot, "Axle_Steering_0_L", 1450.0)));
        assert!(is_cab_wheel_anim(&anim(AnimKind::Rot, "axle_steering_0_l", -1680.0)));
        // a road wheel, a sliding part, another variable
        assert!(!is_cab_wheel_anim(&anim(AnimKind::Rot, "Axle_Steering_0_L", 57.29578)));
        assert!(!is_cab_wheel_anim(&anim(AnimKind::Trans, "Axle_Steering_0_L", 1450.0)));
        assert!(!is_cab_wheel_anim(&anim(AnimKind::Rot, "Lenkwinkel", -1.0)));
        assert!(!is_cab_wheel_anim(&anim(AnimKind::Rot, "ventilator_rot", 800.0)));
    }

    /// A bus whose cab wheel (and the horn button on it) turns with `Axle_Steering_0_L` by
    /// 1450 and whose road wheel turns by the angle itself.
    fn bus() -> VehicleInstance {
        let seed = crate::timetable_run::tests::script_test_vehicle("", "Axle_Steering_0_L\n", "");
        let mut ty = (*seed.ty).clone();
        let cfg = "[mesh]\nlenkrad.o3d\n[newanim]\nanim_rot\nAxle_Steering_0_L\n1450\n\n[mesh]\nhupe.o3d\n[newanim]\nanim_rot\nAxle_Steering_0_L\n-1450\n\n[mesh]\nrad.o3d\n[newanim]\nanim_rot\nAxle_Steering_0_L\n57.29578\n";
        ty.model = omsi_model::Model::parse(&omsi_cfg::CfgFile::from_str("model.cfg", cfg));
        ty.meshes = (0..ty.model.meshes.len())
            .map(|def_index| super::super::VehicleMesh {
                def_index,
                data: Default::default(),
                file: Default::default(),
                materials: Vec::new(),
                overrides: Vec::new(),
                pivot: glam::Mat4::IDENTITY,
                viewpoint: 0,
                skin: Vec::new(),
                keep_winding: false,
            })
            .collect();
        VehicleInstance::new(std::sync::Arc::new(ty), crate::host::VehicleHost::new(Default::default()))
    }

    #[test]
    fn the_players_wheel_turns_as_far_as_the_setting_and_back() {
        let mut v = bus();
        let factor = |v: &VehicleInstance, i: usize| v.animators[i].anims[0].0.factor;
        let lock = v.physics.max_steer_deg.to_radians();
        assert_eq!(v.cab_wheel_rotation(), 0);
        assert_eq!(factor(&v, 0), 1450.0);
        v.set_cab_wheel_rotation(1080);
        assert!((factor(&v, 0) * 2.0 * lock - 1080.0).abs() < 0.01, "{}", factor(&v, 0));
        // the button on the wheel turns with it, the road wheel keeps the axle's angle
        assert_eq!(factor(&v, 1), -factor(&v, 0));
        assert_eq!(factor(&v, 2), 57.29578);
        // (set again, or to another value: from the model's factors, never built up)
        v.set_cab_wheel_rotation(1080);
        v.set_cab_wheel_rotation(900);
        assert!((factor(&v, 0) * 2.0 * lock - 900.0).abs() < 0.01);
        v.set_cab_wheel_rotation(0);
        assert_eq!((factor(&v, 0), factor(&v, 1)), (1450.0, -1450.0));
    }

    #[test]
    fn a_model_read_again_keeps_the_setting() {
        let mut v = bus();
        v.set_cab_wheel_rotation(1080);
        let model = v.ty.model.clone();
        v.reload_model(model).unwrap();
        let lock = v.physics.max_steer_deg.to_radians();
        assert!((v.animators[0].anims[0].0.factor * 2.0 * lock - 1080.0).abs() < 0.01);
    }
}
