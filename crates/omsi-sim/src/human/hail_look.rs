//! Cosmetic gaze for hailing. Never changes simulation heading, position or boarding.
// Adapted from Mayconrib808/OpenOmsi-BBS; copyright (c) 2026 Mayconrib808.
// Original MIT notice: docs/optional-passengers/MAYCON-LICENSE.txt.
use super::*;

/// Convert an absolute bus position into the person's model frame.
/// OMSI heading is clockwise from +Y; subtract world coordinates in f64 first.
pub fn hail_look_target(person: DVec3, heading: f64, bus: DVec3) -> Option<Vec3> {
    let delta = bus - person;
    if !delta.is_finite() || !heading.is_finite() || delta.truncate().length_squared() < 0.01 {
        return None;
    }
    let (s, c) = heading.to_radians().sin_cos();
    Some(Vec3::new(
        (c * delta.x - s * delta.y) as f32,
        (s * delta.x + c * delta.y) as f32,
        delta.z as f32 + 1.7,
    ))
}

#[derive(Default)]
pub struct HailLook {
    yaw: f32,
    pitch: f32,
}
impl HailLook {
    /// Smoothed model-space direction shared by the gaze and the hailing hand.
    pub fn direction(&self) -> Vec3 {
        Vec3::new(-self.yaw.sin(), self.yaw.cos(), 0.)
    }
    fn step(&mut self, target: Option<Vec3>, dt: f32) {
        let (yaw, pitch) = target.filter(|v| v.is_finite()).map_or((0., 0.), |v| {
            let mut yaw = -v.x.atan2(v.y);
            // Keep the previous turn side when a bus crosses directly behind.
            if v.y < 0. && v.x.abs() < v.y.abs() * 0.12 && self.yaw.abs() > 0.1 {
                yaw = self.yaw.signum() * PI;
            }
            (
                yaw.clamp(-150f32.to_radians(), 150f32.to_radians()),
                v.z.atan2(v.truncate().length().max(0.1))
                    .clamp(-25f32.to_radians(), 25f32.to_radians()),
            )
        });
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.1)
        } else {
            0.
        };
        let k = 1. - (-dt / 0.22).exp();
        self.yaw += ((yaw - self.yaw) * k).clamp(-dt * 3.5, dt * 3.5);
        self.pitch += (pitch - self.pitch) * k;
    }
    /// Cosmetic turn in place: feet/pelvis follow large turns, then torso and neck.
    /// Never change the simulated heading, position, route or boarding state.
    pub fn advance(
        &mut self,
        rig: &Rig,
        bones: &mut [Affine3A; SLOTS],
        target: Option<Vec3>,
        amount: f32,
        dt: f32,
    ) {
        let head = bones[HEAD].transform_point3(rig.head_pivot);
        self.step(target.map(|v| v - head), dt);
        let feet = (bones[FOOT[0]].transform_point3(rig.rest_foot(0))
            + bones[FOOT[1]].transform_point3(rig.rest_foot(1)))
            * 0.5;
        self.pose(bones, rig.waist, rig.head_pivot, feet, amount);
    }
    fn pose(
        &self,
        bones: &mut [Affine3A; SLOTS],
        waist: Vec3,
        head_pivot: Vec3,
        feet: Vec3,
        amount: f32,
    ) {
        let amount = if amount.is_finite() {
            amount.clamp(0., 1.)
        } else {
            0.
        };
        if amount <= 0.0001 {
            return;
        }
        // At most 45 degrees remain above the pelvis: 18 in the torso and
        // 27 in the neck. A bus behind makes the whole person turn instead
        // of stretching the neck against a fixed collar and planted legs.
        let upper = self.yaw.clamp(-45f32.to_radians(), 45f32.to_radians());
        let body = self.yaw - upper;
        let torso = upper * 0.4;
        let neck = upper - torso;
        let turn = about(feet, Quat::from_rotation_z(body * amount));
        for bone in bones.iter_mut() {
            *bone = turn * *bone;
        }
        let waist = bones[MAIN].transform_point3(waist);
        let turn = about(waist, Quat::from_rotation_z(torso * amount));
        for slot in [
            MAIN, HEAD, UPPER[0], UPPER[1], FORE[0], FORE[1], HAND[0], HAND[1],
        ] {
            bones[slot] = turn * bones[slot];
        }
        let pivot = bones[HEAD].transform_point3(head_pivot);
        let (scale, old, _) = bones[HEAD].to_scale_rotation_translation();
        let (_, parent, _) = bones[MAIN].to_scale_rotation_translation();
        let (_, _, roll) = (parent.conjugate() * old).to_euler(glam::EulerRot::ZXY);
        // Aim RELATIVE to the inherited torso, retaining native head roll,
        // scale and attachment point. Absolute model yaw broke the neck.
        let desired = parent
            * Quat::from_rotation_z(neck)
            * Quat::from_rotation_x(self.pitch)
            * Quat::from_rotation_y(roll);
        bones[HEAD] = Affine3A::from_translation(pivot)
            * Affine3A::from_scale_rotation_translation(
                scale,
                old.slerp(desired, amount),
                Vec3::ZERO,
            )
            * Affine3A::from_translation(-head_pivot);
    }
}

#[cfg(test)]
mod hail_look_2212_tests {
    use super::*;
    #[test]
    fn omsi_headings_face_the_same_bus_direction() {
        let at = DVec3::new(1e9, -1e9, 25.);
        for heading in [0f64, 90., 180., 270.] {
            let h = heading.to_radians();
            let ahead = DVec3::new(h.sin() * 40., h.cos() * 40., 0.);
            let right = DVec3::new(h.cos() * 40., -h.sin() * 40., 0.);
            let f = hail_look_target(at, heading, at + ahead).unwrap();
            let r = hail_look_target(at, heading, at + right).unwrap();
            assert!(f.x.abs() < 0.001 && f.y > 39.9);
            assert!(r.x > 39.9 && r.y.abs() < 0.001);
            let mut look = HailLook::default();
            for _ in 0..120 {
                look.step(Some(r), 1. / 60.);
            }
            assert!(look.yaw < -1.5 && look.yaw > -1.6);
        }
    }
    #[test]
    fn behind_is_bounded_stable_and_returns_smoothly() {
        let mut look = HailLook::default();
        for _ in 0..200 {
            look.step(Some(Vec3::new(-0.01, -40., 0.)), 1. / 60.);
        }
        let before = look.yaw;
        look.step(Some(Vec3::new(0.01, -40., 0.)), 1. / 60.);
        assert!(look.yaw > 0. && look.yaw <= 150f32.to_radians() + 0.0001);
        assert!((look.yaw - before).abs() < 0.01);
        look.step(None, 1. / 60.);
        assert!(look.yaw > 0. && look.yaw < before);
        for _ in 0..240 {
            look.step(None, 1. / 60.);
        }
        assert!(look.yaw.abs() < 0.001);
    }
    #[test]
    fn invalid_target_never_poison_pose() {
        assert!(hail_look_target(DVec3::ZERO, 0., DVec3::ZERO).is_none());
        assert!(hail_look_target(DVec3::ZERO, f64::NAN, DVec3::X).is_none());
        let mut look = HailLook::default();
        look.step(Some(Vec3::splat(f32::NAN)), f32::NAN);
        assert!(look.yaw.is_finite() && look.pitch.is_finite());
    }
}

#[cfg(test)]
mod hail_neck_body_tests {
    use super::*;
    const WAIST: Vec3 = Vec3::new(0., 0.02, 1.);
    const PIVOT: Vec3 = Vec3::new(0., 0.03, 1.55);
    fn yaw(q: Quat) -> f32 {
        let f = q * Vec3::Y;
        (-f.x).atan2(f.y)
    }
    fn rotation(b: Affine3A) -> Quat {
        b.to_scale_rotation_translation().1
    }
    #[test]
    fn rear_targets_turn_feet_and_keep_neck_relative_to_torso() {
        for degrees in [-150f32, -90., 0., 90., 150.] {
            let look = HailLook {
                yaw: degrees.to_radians(),
                pitch: 0.,
            };
            let mut bones = [Affine3A::IDENTITY; SLOTS];
            look.pose(&mut bones, WAIST, PIVOT, Vec3::ZERO, 1.);
            let pelvis = rotation(bones[HIP]);
            let torso = rotation(bones[MAIN]);
            let head = rotation(bones[HEAD]);
            assert!((yaw(head) - look.yaw).abs() < 0.0001);
            assert!(yaw(torso.conjugate() * head).abs() <= 27f32.to_radians() + 0.0001);
            assert!(yaw(pelvis.conjugate() * torso).abs() <= 18f32.to_radians() + 0.0001);
            for slot in [
                THIGH[0], THIGH[1], SHIN[0], SHIN[1], FOOT[0], FOOT[1], TOE[0], TOE[1],
            ] {
                assert!((yaw(rotation(bones[slot])) - yaw(pelvis)).abs() < 0.0001);
            }
            let left = bones[FOOT[0]].transform_point3(Vec3::new(-0.1, 0., 0.));
            let right = bones[FOOT[1]].transform_point3(Vec3::new(0.1, 0., 0.));
            assert!((left.distance(right) - 0.2).abs() < 0.0001);
            assert!(((left + right) * 0.5).length() < 0.0001);
            assert!(
                bones[HEAD]
                    .transform_point3(PIVOT)
                    .distance(bones[MAIN].transform_point3(PIVOT))
                    < 0.0001
            );
        }
    }
    #[test]
    fn native_tilt_roll_scale_and_head_attachment_survive() {
        let base = Affine3A::from_scale_rotation_translation(
            Vec3::splat(0.8),
            Quat::from_rotation_z(0.15) * Quat::from_rotation_x(0.1),
            Vec3::new(0.01, 0.02, 0.03),
        );
        let mut bones = [base; SLOTS];
        bones[HEAD] = base
            * about(
                PIVOT,
                Quat::from_rotation_z(1.1) * Quat::from_rotation_y(0.08),
            );
        let original = bones;
        let look = HailLook {
            yaw: 150f32.to_radians(),
            pitch: 0.12,
        };
        look.pose(&mut bones, WAIST, PIVOT, Vec3::ZERO, 0.);
        for slot in 0..SLOTS {
            assert!(bones[slot].abs_diff_eq(original[slot], 1e-6));
        }
        look.pose(&mut bones, WAIST, PIVOT, Vec3::ZERO, 1.);
        let relative = rotation(bones[MAIN]).conjugate() * rotation(bones[HEAD]);
        let (neck, pitch, roll) = relative.to_euler(glam::EulerRot::ZXY);
        assert!((neck - 27f32.to_radians()).abs() < 0.0001);
        assert!((pitch - 0.12).abs() < 0.0001);
        assert!((roll - 0.08).abs() < 0.0001);
        assert!(
            bones[HEAD]
                .to_scale_rotation_translation()
                .0
                .distance(Vec3::splat(0.8))
                < 0.0001
        );
        assert!(
            bones[HEAD]
                .transform_point3(PIVOT)
                .distance(bones[MAIN].transform_point3(PIVOT))
                < 0.0001
        );
    }
    #[test]
    fn turn_blends_in_without_sudden_changes() {
        let look = HailLook {
            yaw: -150f32.to_radians(),
            pitch: 0.,
        };
        let mut previous = [Affine3A::IDENTITY; SLOTS];
        for frame in 0..=100 {
            let mut bones = [Affine3A::IDENTITY; SLOTS];
            look.pose(&mut bones, WAIST, PIVOT, Vec3::ZERO, frame as f32 / 100.);
            for slot in 0..SLOTS {
                let delta = rotation(previous[slot]).conjugate() * rotation(bones[slot]);
                let angle = delta.to_axis_angle().1.abs();
                assert!(angle.min((TAU - angle).abs()) < 0.04);
                assert!(bones[slot].is_finite());
            }
            previous = bones;
        }
        let mut bones = [Affine3A::IDENTITY; SLOTS];
        look.pose(&mut bones, WAIST, PIVOT, Vec3::ZERO, f32::NAN);
        assert!(
            bones
                .iter()
                .all(|b| b.abs_diff_eq(Affine3A::IDENTITY, 1e-6))
        );
    }
}
