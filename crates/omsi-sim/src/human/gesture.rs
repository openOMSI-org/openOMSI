//! Local visual arm IK for optional passenger accessories (no simulation state).
// Adapted from Mayconrib808/OpenOmsi-BBS; copyright (c) 2026 Mayconrib808.
// Original MIT notice: docs/optional-passengers/MAYCON-LICENSE.txt.
use super::*;

/// A Brazilian-style boarding request: extend the free arm in front of the
/// posed torso, around shoulder height. The gaze is intentionally independent.
/// Conservative procedural gesture; no external photographic reference is bundled.
pub fn accessory_hail_arm(
    rig: &Rig,
    bones: &mut [Affine3A; SLOTS],
    side: usize,
    waiting_forward: Vec3,
    amount: f32,
    wave: f32,
) {
    let amount = if amount.is_finite() {
        amount.clamp(0., 1.)
    } else {
        0.
    };
    if amount <= 0.0001 {
        return;
    }
    // The IK owns the phase, with zero velocity at either end. This is a rig
    // choice, not a measured duration from the photographic references.
    let amount = amount * amount * (3.0 - 2.0 * amount);
    let side = side.min(1);
    // The waiting pose supplies a stable streetward direction BEFORE the gaze
    // turns the torso. The reference photos show the arm staying out towards
    // the street while the person looks along the approaching traffic.
    let body_forward = bones[MAIN].transform_vector3(Vec3::Y);
    let forward = Vec3::new(body_forward.x, body_forward.y, 0.);
    if !forward.is_finite() || forward.length_squared() < 1e-6 {
        return;
    }
    let forward = forward.normalize();
    let right = forward.cross(Vec3::Z);
    let wanted = Vec3::new(waiting_forward.x, waiting_forward.y, 0.);
    if !wanted.is_finite() || wanted.length_squared() < 1e-6 {
        return;
    }
    let wanted = wanted.normalize();
    let angle = wanted.dot(right).atan2(wanted.dot(forward));
    // Stay in the free shoulder's forward/outward cone. If the bus passes
    // behind, stop following with this arm instead of reaching backwards or
    // sweeping it through the chest. These bounds are rig-fitting choices.
    let angle = if side == 1 {
        angle.clamp(-15f32.to_radians(), 75f32.to_radians())
    } else {
        angle.clamp(-75f32.to_radians(), 15f32.to_radians())
    };
    let out = forward * angle.cos() + right * angle.sin();
    let reach = rig.upper_arm + rig.forearm;
    if !reach.is_finite() || rig.upper_arm <= 0. || rig.forearm <= 0. {
        return;
    }
    let wave = if wave.is_finite() {
        wave.clamp(-0.05, 0.38)
    } else {
        0.
    };
    let sh = bones[MAIN].transform_point3(rig.shoulder[side]);
    let old_el = bones[FORE[side]].transform_point3(rig.elbow[side]);
    let old_wr = bones[HAND[side]].transform_point3(rig.wrist[side]);
    let old_fore = bones[FORE[side]].to_scale_rotation_translation().1;
    let old_upper = bones[UPPER[side]].to_scale_rotation_translation().1;
    let old_hand = bones[HAND[side]].to_scale_rotation_translation().1;

    // Almost extended, with an unlocked elbow and a small outward allowance so
    // the arm does not cross the chest. The old pose held the wrist above the head.
    let raised = (out + Vec3::Z * 0.06).normalize();
    let target = old_wr.lerp(sh + raised * (reach * 0.96), amount);
    // Carry the elbow plane with the reach direction. A raw pole blend could
    // pass close to that axis and make the elbow/wrist turn abruptly.
    let axis = (target - sh).normalize_or(-Vec3::Z);
    let old_axis = (old_wr - sh).normalize_or(-Vec3::Z);
    let carried = Quat::from_rotation_arc(old_axis, axis) * (old_el - (sh + old_wr) * 0.5);
    let outward = right * SIDE[side];
    let project = |v: Vec3| (v - axis * v.dot(axis)).normalize_or(outward);
    let from_pole = project(carried);
    let to_pole = project(-Vec3::Z + outward * 0.15);
    let turn = axis
        .dot(from_pole.cross(to_pole))
        .atan2(from_pole.dot(to_pole));
    let pole = Quat::from_axis_angle(axis, turn * amount) * from_pole;
    let (el, wr, _) = two_bone(sh, rig.upper_arm, rig.forearm, target, pole);
    let upper_axis = (el - sh).normalize_or(-Vec3::Z);
    let fore_axis = (wr - el).normalize_or(out);
    // Keep the palm down as the forearm extends. No wrist "aim" at a bus moving
    // past the stop, no full-arm pivot around the passenger's head.
    let upper_rest = rig.elbow[side] - rig.shoulder[side];
    let fore_rest = rig.wrist[side] - rig.elbow[side];
    // Transport native idle-pose roll with each bone before easing the palm
    // down. Replacing roll at the first nonzero frame snapped the forearm.
    let upper_native =
        Quat::from_rotation_arc((old_upper * upper_rest).normalize(), upper_axis) * old_upper;
    let fore_native =
        Quat::from_rotation_arc((old_fore * fore_rest).normalize(), fore_axis) * old_fore;
    let upper_want = Quat::from_mat3a(&bone_rot(upper_rest, -Vec3::Z, upper_axis, -Vec3::Z));
    let fore_want = Quat::from_mat3a(&bone_rot(fore_rest, -Vec3::Z, fore_axis, -Vec3::Z));
    let upper = Mat3A::from_quat(upper_native.slerp(upper_want, amount));
    let fore = Mat3A::from_quat(fore_native.slerp(fore_want, amount));
    // Flex the hand at its wrist; the shoulder and elbow target do not follow
    // this movement. A bounded angle stays visible without bending the neck.
    let tilt = -0.06 + wave;
    let fingers = out * tilt.cos() - Vec3::Z * tilt.sin();
    let desired = Quat::from_mat3a(&bone_rot(
        rig.wrist[side] - rig.elbow[side],
        -Vec3::Z,
        fingers,
        -Vec3::Z,
    ));
    let fore_q = Quat::from_mat3a(&fore);
    let natural = fore_q * old_fore.conjugate() * old_hand;
    let hand = natural.slerp(desired, amount);
    // Also bound the blend from imported/native idle poses, not only the final pose.
    let hand = fore_q * limit_quat(fore_q.conjugate() * hand, 35f32.to_radians());
    bones[UPPER[side]] = joint_xf(rig.shoulder[side], sh, upper);
    bones[FORE[side]] = joint_xf(rig.elbow[side], el, fore);
    bones[HAND[side]] = joint_xf(rig.wrist[side], wr, Mat3A::from_quat(hand));
}

#[cfg(test)]
mod brazilian_hail_tests {
    use super::*;

    fn rig(height: f32) -> Rig {
        let mut def = Human {
            height: 1.77,
            seat_height: 0.82,
            links: vec![
                0.09, 0., 0.92, 0.09, -0.03, 0.53, 0.02, 1.17, 0.18, -0.05, 1.43, 0.44, -0.04,
                1.41, -0.02, 1.55, 0.69, -0.03, 1.43, 0.9, -0.03, 1.43,
            ],
            walk_param: [1.4, 80., 1., 1., 0.],
            ..Default::default()
        };
        let scale = height / def.height;
        for v in &mut def.links {
            *v *= scale;
        }
        def.height = height;
        Rig::measure(&def, &Joints::from_links(&def.links), &[])
    }

    #[test]
    fn hail_extends_from_shoulder_in_body_frame_at_different_sizes_and_headings() {
        for height in [1.10, 1.45, 1.77, 2.05] {
            let r = rig(height);
            for degrees in [-150f32, -90., -35., 0., 35., 90., 150.] {
                let q = Quat::from_rotation_z(degrees.to_radians());
                let forward = q * Vec3::Y;
                for side in 0..2 {
                    for wave in [-0.04, 0., 0.04] {
                        let mut bones = [Affine3A::from_quat(q); SLOTS];
                        let before = bones;
                        accessory_hail_arm(&r, &mut bones, side, forward, 1., wave);
                        let sh = bones[UPPER[side]].transform_point3(r.shoulder[side]);
                        let el = bones[FORE[side]].transform_point3(r.elbow[side]);
                        let wr = bones[HAND[side]].transform_point3(r.wrist[side]);
                        let reach = r.upper_arm + r.forearm;
                        assert!((sh.distance(el) - r.upper_arm).abs() < 1e-4);
                        assert!((el.distance(wr) - r.forearm).abs() < 1e-4);
                        assert!((sh.distance(wr) / reach - 0.96).abs() < 1e-4);
                        assert!((wr - sh).normalize().dot(forward) > 0.98);
                        assert!((wr.z - sh.z).abs() < reach * 0.08);
                        assert!(
                            bones[UPPER[side]]
                                .transform_point3(r.elbow[side])
                                .distance(el)
                                < 1e-4
                        );
                        assert!(
                            bones[FORE[side]]
                                .transform_point3(r.wrist[side])
                                .distance(wr)
                                < 1e-4
                        );
                        let fingers = bones[HAND[side]]
                            .transform_vector3(r.wrist[side] - r.elbow[side])
                            .normalize();
                        let palm = bones[HAND[side]].transform_vector3(-Vec3::Z).normalize();
                        assert!(fingers.dot(forward) > 0.97);
                        assert!(palm.dot(-Vec3::Z) > 0.97);
                        for slot in 0..SLOTS {
                            if ![UPPER[side], FORE[side], HAND[side]].contains(&slot) {
                                assert!(bones[slot].abs_diff_eq(before[slot], 1e-6));
                            }
                        }
                        assert!(bones.iter().all(|b| b.is_finite()));
                    }
                }
            }
        }
    }

    #[test]
    fn hailing_wrist_flexes_visibly_without_swinging_the_arm_or_twisting_the_neck() {
        for height in [1.10, 1.77, 2.05] {
            let r = rig(height);
            for side in 0..2 {
                let mut straight = [Affine3A::IDENTITY; SLOTS];
                accessory_hail_arm(&r, &mut straight, side, Vec3::Y, 1., 0.);
                let mut flexed = [Affine3A::IDENTITY; SLOTS];
                accessory_hail_arm(&r, &mut flexed, side, Vec3::Y, 1., 0.30);
                for slot in [UPPER[side], FORE[side], HEAD, MAIN] {
                    assert!(straight[slot].abs_diff_eq(flexed[slot], 1e-6));
                }
                let fingers = |b: &[Affine3A; SLOTS]| {
                    b[HAND[side]]
                        .transform_vector3(r.wrist[side] - r.elbow[side])
                        .normalize()
                };
                assert!(fingers(&straight).angle_between(fingers(&flexed)) > 0.20);
                assert!(
                    straight[HAND[side]]
                        .transform_point3(r.wrist[side])
                        .distance(flexed[HAND[side]].transform_point3(r.wrist[side]))
                        < 1e-5
                );
                let fore = flexed[FORE[side]].to_scale_rotation_translation().1;
                let hand = flexed[HAND[side]].to_scale_rotation_translation().1;
                assert!(fore.angle_between(hand) <= 35f32.to_radians() + 1e-3);
            }
        }
    }

    #[test]
    fn hail_hand_stays_streetward_while_gaze_tracks_the_bus() {
        let r = rig(1.77);
        for degrees in [-150f32, -90., -35., 0., 35., 90., 150.] {
            let bus = Quat::from_rotation_z(degrees.to_radians()) * Vec3::Y;
            let mut look = HailLook::default();
            let mut bones = [Affine3A::IDENTITY; SLOTS];
            for _ in 0..240 {
                bones = [Affine3A::IDENTITY; SLOTS];
                look.advance(
                    &r,
                    &mut bones,
                    Some(bus * 40. + Vec3::Z * 1.7),
                    1.,
                    1. / 60.,
                );
            }
            let head = bones[HEAD];
            let torso = bones[MAIN];
            let right = torso.transform_vector3(Vec3::X).normalize();
            let side = if Vec3::Y.dot(right) >= 0. { 1 } else { 0 };
            accessory_hail_arm(&r, &mut bones, side, Vec3::Y, 1., 0.);
            let forward = torso.transform_vector3(Vec3::Y).normalize();
            let hand = bones[HAND[side]]
                .transform_vector3(r.wrist[side] - r.elbow[side])
                .normalize();
            assert!(
                hand.dot(forward) > 0.24,
                "arm behind its shoulder at {degrees}"
            );
            if degrees.abs() <= 90. {
                assert!(hand.dot(Vec3::Y) > 0.97, "hand chased bus at {degrees}");
            }
            assert!(bones[HEAD].abs_diff_eq(head, 1e-6));
            let neck = torso.to_scale_rotation_translation().1.conjugate()
                * head.to_scale_rotation_translation().1;
            assert!(neck.to_euler(glam::EulerRot::ZXY).0.abs() <= 27f32.to_radians() + 1e-4);
        }
        // Same torso, entirely different head rotations: identical arm pose.
        let mut a = [Affine3A::IDENTITY; SLOTS];
        let mut b = a;
        b[HEAD] = Affine3A::from_quat(Quat::from_rotation_z(1.2));
        accessory_hail_arm(&r, &mut a, 1, Vec3::Y, 1., 0.);
        accessory_hail_arm(&r, &mut b, 1, Vec3::Y, 1., 0.);
        for slot in [UPPER[1], FORE[1], HAND[1]] {
            assert!(a[slot].abs_diff_eq(b[slot], 1e-6));
        }
    }

    #[test]
    fn hail_blends_from_native_waiting_poses_without_stretching() {
        let r = rig(1.77);
        for side in 0..2 {
            let mut pose = Pose::new(12);
            let input = PoseInput::default();
            for _ in 0..180 {
                pose.advance(&r, &input, 1. / 60.);
            }
            let native = pose.bones(&r);
            let mut previous = native.bones;
            for frame in 0..=100 {
                let mut bones = native.bones;
                accessory_hail_arm(&r, &mut bones, side, Vec3::Y, frame as f32 / 100., 0.);
                let wrist = bones[HAND[side]].transform_point3(r.wrist[side]);
                let old = previous[HAND[side]].transform_point3(r.wrist[side]);
                assert!(wrist.distance(old) < 0.025);
                for slot in [UPPER[side], FORE[side], HAND[side]] {
                    let a = previous[slot].to_scale_rotation_translation().1;
                    let b = bones[slot].to_scale_rotation_translation().1;
                    assert!(
                        a.angle_between(b) < 0.09,
                        "joint snapped at frame {frame}, slot {slot}"
                    );
                }
                let fore = bones[FORE[side]].to_scale_rotation_translation().1;
                let hand = bones[HAND[side]].to_scale_rotation_translation().1;
                // Matrix decomposition can choose q or -q for the same pose.
                // Compare the physical shortest rotation, not quaternion sign.
                let wrist_angle = fore.angle_between(hand);
                assert!(
                    wrist_angle <= 35f32.to_radians() + 1e-3,
                    "wrist {wrist_angle} at frame {frame}, side {side}"
                );
                assert!(bones.iter().all(|b| b.is_finite()));
                previous = bones;
            }
        }
    }

    #[test]
    fn disabled_and_invalid_hail_inputs_preserve_pose_or_remain_finite() {
        let r = rig(1.77);
        for amount in [0., -1., f32::NAN] {
            let mut bones = [Affine3A::IDENTITY; SLOTS];
            accessory_hail_arm(&r, &mut bones, 0, Vec3::Y, amount, f32::NAN);
            assert!(
                bones
                    .iter()
                    .all(|b| b.abs_diff_eq(Affine3A::IDENTITY, 1e-6))
            );
        }
        let mut bones = [Affine3A::IDENTITY; SLOTS];
        bones[MAIN] = Affine3A::from_quat(Quat::from_rotation_x(PI / 2.));
        let before = bones;
        accessory_hail_arm(&r, &mut bones, 0, Vec3::Y, 1., 0.);
        assert!(
            bones
                .iter()
                .zip(before)
                .all(|(a, b)| a.abs_diff_eq(b, 1e-6))
        );
        let mut bones = [Affine3A::IDENTITY; SLOTS];
        accessory_hail_arm(&r, &mut bones, 99, Vec3::Y, 1., f32::NAN);
        assert!(bones.iter().all(|b| b.is_finite()));
    }
}
