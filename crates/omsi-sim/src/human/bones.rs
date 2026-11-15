//! The bone transforms for a person's pose this frame.

use super::*;

/// What `Pose::foot_targets` finds for the two feet (see there).
type FootTargets = ([Vec3; 2], [Mat3A; 2], [f32; 2], [Vec3; 2], [f32; 2], [Option<(f32, f32, f32)>; 2], [f32; 2]);

impl Pose {
    /// Bone transforms for the current state.
    pub fn bones(&mut self, rig: &Rig) -> Posed {
        let d = |deg: f32| deg.to_radians();
        let walk = self.walk;
        let speed = self.speed.max(0.08);
        let beta = stance_fraction(speed);
        let ph = self.phase * TAU;
        let sit = self.sit;
        let s_ease = smoothstep(0.0, 1.0, sit);
        // hips move back first and come down later (and the other way round getting up)
        let s_back = smoothstep(0.0, 0.75, sit);
        let s_down = smoothstep(0.2, 1.0, sit);
        let bump = (PI * sit).sin().max(0.0);
        let breath = self.breath.sin();
        let intensity = ((speed - 0.15) / 1.1).clamp(0.0, 1.0) * walk;
        let still = 1.0 - walk;

        // --- feet targets (model frame): ankle positions, foot yaw and pitch ---
        let (mut ankle_t, mut foot_rot, toe_bend, foot_fwd, foot_yaw, swing, swing_floor) = self.foot_targets(rig, walk, beta);

        // --- pelvis ---
        let sway = rig.hip_sway.sqrt();
        // towards the leg that carries the weight, dipping on the swinging side
        let lat_walk = -0.022 * rig.scale * sway.min(1.3) * (ph - 0.25).sin() * intensity;
        let lat_idle = self.shift * still * (1.0 - s_ease);
        let run = run_factor(self.speed);
        // (each person bounces, swings the arms and leans walking in a way of their own)
        let bob = -0.018 * rig.scale * intensity * (1.0 + 1.8 * run) * (1.0 + 0.3 * self.style[1]) * (0.5 + 0.5 * (2.0 * ph - 0.35).cos());
        let lean_acc = (self.accel * 1.8).clamp(-6.0, 7.0) * walk;
        let pelvis_roll = d(3.5) * sway * intensity * (ph - 0.35).sin()
            - d(2.2) * (self.shift / (0.03 * rig.scale)) * still * (1.0 - s_ease);
        // the hip of the leg in front leads (about z, counter-clockwise positive)
        let pelvis_yaw = -d(5.0) * intensity * ph.cos();
        // The body turns over its feet: the person's heading turns at once (a corner of the
        // way, the door, the seat), the feet only as they step. Standing, the hips point
        // between the two feet (a swinging one where it is now), as in a pivot turn, and
        // come round with the heading as the feet step - turned straight to the new heading
        // over feet that stayed, the legs ran diagonally and the person seemed to slide.
        // (The whole body turns with them: the trunk's counter-turn is the gait's alone.)
        // (half as much while walking, whose steps follow the heading by themselves; none
        // seated)
        let body_lag = -d(((foot_yaw[0] + foot_yaw[1]) * 0.5).clamp(-80.0, 80.0)) * (1.0 - 0.5 * walk) * (1.0 - s_ease);
        // reaching for something far: bend at the hips and bring the right shoulder round
        let (reach_lean, reach_twist) = if self.reach > 0.0 {
            let v = self.reach_at - rig.shoulder[1];
            let excess = (v.length() - 0.75 * (rig.upper_arm + rig.forearm)).max(0.0);
            let r = smoothstep(0.0, 1.0, self.reach);
            let yaw = v.x.atan2(v.y.max(0.05)).to_degrees();
            (
                d(32.0) * (excess / 0.35).min(1.0) * r * if v.y > 0.0 { 1.0 } else { 0.3 },
                d((-yaw * 0.5).clamp(-30.0, 12.0)) * r,
            )
        } else {
            (0.0, 0.0)
        };
        // a driver sits up, leaning towards the wheel
        let grip_lean = d(16.0 + self.grip_extra.clamp(0.0, 30.0)) * smoothstep(0.0, 1.0, self.grip);
        // forward tilt walking, backwards on a seat
        let pelvis_tilt = d(2.0) * walk + d(11.0) * run - d(12.0) * s_ease + reach_lean * 0.5 + grip_lean * 0.7;
        // swaying with the bus: the body goes with the pull (forwards when it brakes, outwards
        // in a bend), less when holding on or sitting
        let lean_bus = self.lean * (1.0 - 0.4 * self.hold) * (1.0 - 0.4 * s_ease);
        let bus_shift = Vec3::new(lean_bus.x * 0.012, lean_bus.y * 0.012, 0.0) * (1.0 - s_ease);
        let stand_z = rig.pelvis.z - 0.004 * rig.scale - 0.012 * walk - 0.035 * rig.scale * run + 0.002 * breath * still;
        let stand_c = Vec3::new(
            lat_walk + lat_idle,
            rig.pelvis.y + 0.02 * walk,
            stand_z + bob + self.body_floor,
        ) + bus_shift;
        // `self.seat` puts the pelvis a fixed distance (`SEAT_FRONT` in humans.rs) in
        // front of the hip point, the same for every human type because the shared cabin
        // that computed it has no rig to ask. Redone here with this rig's own
        // `seat_front()` (from its actual thigh length), the knee lands where this body's
        // legs naturally put it instead of at the average distance - on a long bench with
        // no footwell to hide a mismatch in, a longer-legged rig forced to the average
        // distance bent its knee enough to poke through the seat ahead.
        let seated_c = self.seat_pelvis.unwrap_or(Vec3::new(
            self.seat.x,
            -rig.seat_front() + 0.03,
            self.seat.z + rig.seat_lift,
        ));
        let mut pc = Vec3::new(
            stand_c.x + (seated_c.x - stand_c.x) * s_back,
            stand_c.y + (seated_c.y - stand_c.y) * s_back,
            stand_c.z + (seated_c.z - stand_c.z) * s_down,
        );
        // over the feet while getting up or down: the hips go a little lower and further
        pc.z -= 0.05 * bump * rig.scale;
        // the pelvis never higher than the legs can reach
        let pelvis_q = Quat::from_rotation_z(pelvis_yaw + body_lag)
            * Quat::from_rotation_y(pelvis_roll)
            * Quat::from_rotation_x(-pelvis_tilt);
        let reach_max = (rig.thigh + rig.shin) * 0.997;
        let mut need = 0.0f32;
        if sit < 0.5 {
            // (a foot in the air need not be reached)
            for side in (0..2).filter(|&k| self.feet[k].planted) {
                let hip_at = pc + pelvis_q * (rig.hip[side] - rig.pelvis);
                let dv = ankle_t[side] - hip_at;
                let horiz = dv.truncate().length();
                if horiz < reach_max {
                    let max_up = (reach_max * reach_max - horiz * horiz).sqrt();
                    let over = -dv.z - max_up;
                    need = need.max(over);
                }
            }
        }
        let drop = need.clamp(0.0, MAX_DROP).max(self.drop);
        self.drop = drop;
        pc.z -= drop * (1.0 - s_ease);
        let pelvis_m = Affine3A::from_translation(pc - rig.pelvis) * about(rig.pelvis, pelvis_q);

        // --- trunk and head ---
        // shoulders against the hips, and a little towards what the head looks at
        // (the shoulders take a good part of a look to the side - up to 30 degrees - so that
        // the head turns no further on the trunk than a neck can: the people of OMSI have
        // no neck bone, and the skin between collar and head, stretched by a head turned
        // 60 degrees on still shoulders, made a twisted, broken neck of every passenger
        // who looked at the driver)
        let trunk_yaw = -pelvis_yaw * 1.7
            - d((self.head.x.clamp(-90.0, 90.0) * 0.42).clamp(-30.0, 30.0)) * (1.0 - walk) * (1.0 - self.reach)
            + reach_twist;
        let trunk_lean = d((3.0 + 2.0 * self.style[3]) * walk + 1.5 * self.style[3] + lean_acc)
            + d(34.0) * bump
            + d(12.0 - 4.0) * s_ease
            + d(0.7) * breath
            + d(lean_bus.y * 2.4)
            + reach_lean * 0.6
            + grip_lean * 0.5;
        let trunk_side = -pelvis_roll * 0.8 + d(lean_bus.x * 2.4);
        let trunk_q = Quat::from_rotation_z(trunk_yaw)
            * Quat::from_rotation_y(trunk_side)
            * Quat::from_rotation_x(-trunk_lean);
        let trunk_m = pelvis_m * about(rig.waist, trunk_q);
        let trunk_rot = pelvis_q * trunk_q;
        // the head: level, turned towards what it looks at, within its limits
        // (it follows half of a deep forward lean)
        let head_pitch = self.head.y.clamp(-38.0, 28.0) + 4.0 * self.style[1]
            - 0.5 * trunk_lean.to_degrees().max(0.0);
        let head_world =
            yaw_quat(self.head.x.clamp(-72.0, 72.0)) * Quat::from_rotation_x(d(head_pitch));
        let head_rel = limit_quat(trunk_rot.inverse() * head_world, d(45.0));
        let head_m = trunk_m * about(rig.head_pivot, head_rel);

        // --- legs ---
        let mut out_bones = [Affine3A::IDENTITY; SLOTS];
        out_bones[HIP] = pelvis_m;
        out_bones[MAIN] = trunk_m;
        out_bones[HEAD] = head_m;
        let mut posed = Posed {
            bones: [Affine3A::IDENTITY; SLOTS],
            hip: [Vec3::ZERO; 2],
            knee: [Vec3::ZERO; 2],
            ankle: [Vec3::ZERO; 2],
            elbow: [Vec3::ZERO; 2],
            wrist: [Vec3::ZERO; 2],
            sole: [0.0; 2],
            knee_flex: [0.0; 2],
            ankle_flex: [0.0; 2],
            leg_miss: [0.0; 2],
            neck: head_m.transform_point3(rig.neck),
            ok: true,
        };
        let pelvis_fwd = pelvis_q * Vec3::Y;
        for side in 0..2 {
            let hip_at = pelvis_m.transform_point3(rig.hip[side]);
            let fwd = (foot_fwd[side] + pelvis_fwd).normalize_or(Vec3::Y);
            let pole = fwd + Vec3::Z * 0.25;
            // Seated, the feet go where a sitting body puts them: the thigh along the seat,
            // the shin hanging down. A floor further down than that (a seat on a podium or
            // over a wheel arch) is not reached by stretching the leg straight at it - the
            // leg ran diagonally through the seat's front - the feet hang above it instead.
            if sit > 0.5 {
                let flat = Vec3::new(pelvis_fwd.x, pelvis_fwd.y, 0.0).normalize_or(Vec3::Y);
                let knee_n = hip_at + flat * rig.thigh * 0.95;
                let hang = knee_n - Vec3::Z * rig.shin * 0.97;
                // (always, not only where the standing place is out of reach: a bench's
                // hip point has the feet that stood up to it right under the pelvis, and the
                // legs went straight down from the seat as if kneeling)
                let blend = ((sit - 0.5) * 2.0).clamp(0.0, 1.0);
                // (a floor within reach of a hanging shin is stood on: the knee bends a little more)
                let floor_z = if ankle_t[side].z >= hang.z - 0.12 { ankle_t[side].z } else { hang.z };
                let target = Vec3::new(hang.x, hang.y, floor_z);
                ankle_t[side] = ankle_t[side] + (target - ankle_t[side]) * blend;
            }
            let (knee_at, ankle_at, hinge) =
                two_bone(hip_at, rig.thigh, rig.shin, ankle_t[side], pole);
            let side_v = hinge;
            if let Some((t, from, land)) = swing[side] {
                let pitch = Self::swing_pitch(rig, t, from, land, knee_at, ankle_at, side_v, swing_floor, side);
                foot_rot[side] =
                    Mat3A::from_quat(yaw_quat(foot_yaw[side]) * Quat::from_rotation_x(d(pitch)));
            }
            let r_thigh = bone_rot(
                rig.knee[side] - rig.hip[side],
                Vec3::X,
                knee_at - hip_at,
                side_v,
            );
            let r_shin = bone_rot(
                rig.ankle[side] - rig.knee[side],
                Vec3::X,
                ankle_at - knee_at,
                side_v,
            );
            out_bones[THIGH[side]] = joint_xf(rig.hip[side], hip_at, r_thigh);
            out_bones[SHIN[side]] = joint_xf(rig.knee[side], knee_at, r_shin);
            out_bones[FOOT[side]] = joint_xf(rig.ankle[side], ankle_at, foot_rot[side]);
            let ball_rest = rig.ball_joint(side);
            let ball_at = out_bones[FOOT[side]].transform_point3(ball_rest);
            let toe_rot = foot_rot[side] * Mat3A::from_rotation_x(d(toe_bend[side]));
            out_bones[TOE[side]] = joint_xf(ball_rest, ball_at, toe_rot);
            let t_dir = (knee_at - hip_at).normalize_or(-Vec3::Z);
            let s_dir = (ankle_at - knee_at).normalize_or(-Vec3::Z);
            posed.hip[side] = hip_at;
            posed.knee[side] = knee_at;
            posed.ankle[side] = ankle_at;
            posed.knee_flex[side] = t_dir.dot(s_dir).clamp(-1.0, 1.0).acos().to_degrees();
            let foot_dir = Vec3::from(foot_rot[side] * Vec3A::Y);
            // toes towards the shin positive
            posed.ankle_flex[side] =
                90.0 - foot_dir.dot(-s_dir).clamp(-1.0, 1.0).acos().to_degrees();
            posed.leg_miss[side] = (ankle_at - ankle_t[side]).length();
            let heel =
                ankle_at + Vec3::from(foot_rot[side] * Vec3A::new(0.0, rig.heel, -rig.ankle_h));
            let ball = ball_at + Vec3::from(foot_rot[side] * Vec3A::new(0.0, 0.0, -rig.ball_h));
            let toe =
                ball_at + Vec3::from(toe_rot * Vec3A::new(0.0, rig.toe - rig.ball, -rig.ball_h));
            posed.sole[side] = heel.z.min(toe.z).min(ball.z);
        }

        // --- arms ---
        let run = run_factor(self.speed);
        let arm_amp = d(17.0) * rig.arm_swing.powf(0.7) * intensity * (1.0 + 1.1 * run) * (1.0 + 0.5 * self.style[0]);
        for side in 0..2 {
            let s = SIDE[side];
            let sh_at = trunk_m.transform_point3(rig.shoulder[side]);
            // the hanging arm: swings against the legs (left arm forward when the right
            // leg is), a little out from the body, the elbow bending more going forward
            let arm_fwd = if side == 0 { -ph.cos() } else { ph.cos() };
            let swing = arm_amp * arm_fwd - d(3.0) * walk
                + d(20.0) * bump * (1.0 - s_down)
                + d(2.0 + 1.5 * self.style[3]);
            let abd = d(9.0 + 1.5 * self.style[0] + 2.0 * walk) + d(0.6) * breath * still;
            // (running: the elbows held bent near a right angle)
            let flex = d(13.0 + 3.0 * self.style[3])
                + d(16.0) * intensity * (0.5 + 0.5 * arm_fwd)
                + d(8.0) * bump
                + d(62.0) * run;
            let inward = d(16.0);
            let a = Quat::from_rotation_x(swing) * Vec3::new(s * abd.sin(), 0.0, -abd.cos());
            let front0 =
                Quat::from_rotation_x(swing) * Vec3::new(-s * inward.sin(), inward.cos(), 0.0);
            let front = (front0 - a * a.dot(front0)).normalize_or(Vec3::Y);
            let c = a * flex.cos() + front * flex.sin();
            let elbow_l = a * rig.upper_arm;
            let wrist_l = elbow_l + c * rig.forearm;
            let hang_wrist = sh_at + trunk_rot * wrist_l;
            let hang_elbow = sh_at + trunk_rot * elbow_l;
            let hang_pole = hang_elbow - (sh_at + hang_wrist) * 0.5;
            let (target, pole) = self.arm_target(rig, &posed, side, s, sh_at, hang_wrist, hang_pole, pelvis_q, sit);
            let (el_at, wr_at, _) = two_bone(sh_at, rig.upper_arm, rig.forearm, target, pole);
            let a1 = (el_at - sh_at).normalize_or(-Vec3::Z);
            let c1 = (wr_at - el_at).normalize_or(-Vec3::Z);
            let mut f1 = c1 - a1 * a1.dot(c1);
            if f1.length_squared() < 1e-6 {
                f1 = pole.cross(a1).cross(a1) * -1.0;
            }
            let f1 = f1.normalize_or(Vec3::Y);
            let bend = a1.dot(c1).clamp(-1.0, 1.0).acos();
            let f1_fore = f1 * bend.cos() - a1 * bend.sin();
            let a0 = rig.elbow[side] - rig.shoulder[side];
            let c0 = rig.wrist[side] - rig.elbow[side];
            let r_upper = bone_rot(a0, Vec3::Y, a1, f1);
            let r_fore = bone_rot(c0, Vec3::Y, c1, f1_fore);
            let r_hand = self.hand_rot(side, c0, r_fore);
            out_bones[UPPER[side]] = joint_xf(rig.shoulder[side], sh_at, r_upper);
            out_bones[FORE[side]] = joint_xf(rig.elbow[side], el_at, r_fore);
            out_bones[HAND[side]] = joint_xf(rig.wrist[side], wr_at, r_hand);
            posed.elbow[side] = el_at;
            posed.wrist[side] = wr_at;
        }
        posed.ok = out_bones.iter().all(|b| b.is_finite());
        posed.bones = if posed.ok {
            out_bones
        } else {
            [Affine3A::IDENTITY; SLOTS]
        };
        if !posed.ok {
            // start over from standing rather than carry the bad state on
            let seed = self.rng;
            *self = Pose::new(seed);
        }
        posed
    }

    /// Where the feet go this frame (model frame): the ankles, the feet's turn and pitch, the
    /// toes' bend, and for a foot in the air its swing and the floor under it.
    fn foot_targets(&self, rig: &Rig, walk: f32, beta: f32) -> FootTargets {
        let d = |deg: f32| deg.to_radians();
        let mut ankle_t = [Vec3::ZERO; 2];
        let mut foot_rot = [Mat3A::IDENTITY; 2];
        // the toes bend up against the foot while the heel is up, and straighten in the air
        let mut toe_bend = [0f32; 2];
        let mut foot_fwd = [Vec3::Y; 2];
        let mut foot_yaw = [0f32; 2];
        // a foot in the air: (progress, pitch at lift-off, pitch to land with); its pitch
        // follows the shin once the leg is solved
        let mut swing: [Option<(f32, f32, f32)>; 2] = [None; 2];
        let mut swing_floor = [0f32; 2];
        for side in 0..2 {
            let f = self.feet[side];
            let u = (self.phase + if side == 0 { 0.0 } else { 0.5 }) % 1.0;
            let (ankle, yaw, pitch) = if f.planted {
                // (a lift-off held back while the other foot is still in the air keeps the
                // push-off pitch: drawn flat meanwhile, the ankle leapt 7 cm up and forward
                // the moment the foot left)
                let pitch = if f.walk && self.gait && u < beta + 0.3 {
                    stance_pitch(u.min(beta), beta, walk)
                } else {
                    0.0
                };
                toe_bend[side] = (-pitch).max(0.0);
                (Self::stance_ankle(rig, &f, pitch), f.yaw, pitch)
            } else {
                let t = f.t.clamp(0.0, 1.0);
                let land_pitch = if f.walk { heel_pitch(walk) } else { 0.0 };
                let land = {
                    let g = Foot {
                        pos: DVec3::new(f.to.x, f.to.y, f.land_z),
                        yaw: f.to_yaw,
                        ..f
                    };
                    Self::stance_ankle(rig, &g, land_pitch)
                };
                let s = smoothstep(0.0, 1.0, t) as f64;
                let mut a = f.from_ankle + (land - f.from_ankle) * s;
                // up a step the foot rises early, down one it drops late
                // (blended by the height, not switched: the curve changing mid-swing moved
                // the foot as much as the floor did)
                let rise = (land.z - f.from_ankle.z) as f32;
                let up = smoothstep(0.0, 0.06, rise);
                let down = smoothstep(0.0, 0.06, -rise);
                let zt = t * (1.0 - up - down) + smoothstep(0.0, 0.55, t) * up + smoothstep(0.35, 1.0, t) * down;
                let bumpf = (PI * t.powf(0.75)).sin().max(0.0);
                a.z = f.from_ankle.z
                    + (land.z - f.from_ankle.z) * zt as f64
                    + (f.lift * bumpf) as f64;
                let yaw = f.from_yaw + angle_diff(f.from_yaw, f.to_yaw) * s;
                toe_bend[side] = (-f.from_pitch).max(0.0) * (1.0 - smoothstep(0.0, 0.35, t));
                if f.walk {
                    swing[side] = Some((t, f.from_pitch, land_pitch));
                }
                swing_floor[side] =
                    (f.from_floor + (f.land_z - f.from_floor) * zt as f64 - self.origin.z) as f32;
                (a, yaw, 0.0)
            };
            let local = self.to_local(ankle);
            let yaw_l = angle_diff(self.heading, yaw) as f32;
            ankle_t[side] = local;
            foot_rot[side] = Mat3A::from_quat(yaw_quat(yaw_l) * Quat::from_rotation_x(d(pitch)));
            foot_fwd[side] = yaw_quat(yaw_l) * Vec3::Y;
            foot_yaw[side] = yaw_l;
        }
        (ankle_t, foot_rot, toe_bend, foot_fwd, foot_yaw, swing, swing_floor)
    }

    /// The pitch of a foot in the air (degrees): from the push-off through neutral to the heel
    /// strike, against the shin, with toes and heel clear of the floor.
    #[allow(clippy::too_many_arguments)]
    fn swing_pitch(rig: &Rig, t: f32, from: f32, land: f32, knee_at: Vec3, ankle_at: Vec3, side_v: Vec3, swing_floor: [f32; 2], side: usize) -> f32 {
        // in the air the ankle goes from pointed (push-off) through neutral to a
        // little flexed; the pitch is that against the shin, blended from the
        // push-off and into the heel strike
        let s_dir = (ankle_at - knee_at).normalize_or(-Vec3::Z);
        let neutral = side_v.cross(s_dir).normalize_or(Vec3::Y);
        let shin_pitch = neutral.z.clamp(-1.0, 1.0).asin().to_degrees();
        let ankle = -14.0 + 14.0 * smoothstep(0.0, 0.5, t) + 3.0 * smoothstep(0.5, 0.85, t);
        let rel = shin_pitch + ankle;
        let from_w = 1.0 - smoothstep(0.0, 0.3, t);
        let land_w = smoothstep(0.65, 1.0, t);
        let mut pitch = (rel + (from - rel) * from_w) * (1.0 - land_w) + land * land_w;
        // the toes clear the floor, the heel too
        let ah = rig.ankle_h;
        // At lift-off/landing the sole is still touching the floor. Requiring
        // 12 mm of clearance there gives mutually impossible heel/toe limits:
        // the last clamp tips the shoe through the floor. Spend only the
        // clearance the ankle has actually gained above its resting height.
        let height = ankle_at.z - swing_floor[side];
        let above = height - (height - ah).clamp(0.0, 0.012);
        let (rt, pt) = ((rig.toe * rig.toe + ah * ah).sqrt(), ah.atan2(rig.toe));
        if -above / rt > -1.0 {
            pitch = pitch.max((pt + (-above / rt).min(1.0).asin()).to_degrees());
        }
        let (rh, phh) = ((rig.heel * rig.heel + ah * ah).sqrt(), ah.atan2(-rig.heel));
        if -above / rh > -1.0 {
            pitch = pitch.min(-(phh + (-above / rh).min(1.0).asin()).to_degrees());
        }
        pitch
    }

    /// Where a hand goes and how its elbow points: hanging, on the lap, reaching for the desk
    /// or on the steering wheel.
    #[allow(clippy::too_many_arguments)]
    fn arm_target(&self, rig: &Rig, posed: &Posed, side: usize, s: f32, sh_at: Vec3, hang_wrist: Vec3, hang_pole: Vec3, pelvis_q: Quat, sit: f32) -> (Vec3, Vec3) {
        // seated: hands on the thighs
        let hip_at = posed.hip[side];
        let knee_at = posed.knee[side];
        let lap =
            hip_at + (knee_at - hip_at) * 0.55 + Vec3::new(-s * 0.02, 0.0, 0.11 * rig.scale);
        let lap_pole = pelvis_q * Vec3::new(s * 0.5, -1.0, -0.2);
        let sit_w = smoothstep(0.35, 1.0, sit);
        let mut target = hang_wrist.lerp(lap, sit_w);
        let mut pole = hang_pole.normalize_or(-Vec3::Y).lerp(lap_pole, sit_w);
        // standing still: the person's way of standing (see `Idle`), round the turned hips
        let still = (1.0 - smoothstep(0.05, 0.4, self.walk)) * (1.0 - sit_w);
        if still > 0.0 {
            let mid = (posed.hip[0] + posed.hip[1]) * 0.5;
            let up = rig.shoulder[side].z - rig.hip[side].z;
            let way = |w: Idle| -> Option<(Vec3, Vec3)> {
                let sc = rig.scale;
                let (at, pole) = match w {
                    Idle::Hang => return None,
                    Idle::Clasped => (Vec3::new(s * 0.035, 0.17, -0.07) * sc, Vec3::new(s, -0.3, -0.6)),
                    Idle::Behind => (Vec3::new(s * 0.04, -0.17, -0.02) * sc, Vec3::new(s, 0.3, -0.5)),
                    Idle::Crossed => (Vec3::new(s * 0.05 * sc, 0.21 * sc, up - 0.24 * sc + if side == 1 { 0.035 } else { 0.0 }), Vec3::new(s, -0.25, -0.35)),
                    Idle::HandOnHip if (side == 1) == (self.style[2] > 0.0) => (Vec3::new(s * (rig.hip[1].x.abs() + 0.13 * sc), -0.02 * sc, 0.07 * sc), Vec3::new(s, -0.6, 0.1)),
                    Idle::HandOnHip => return None,
                    Idle::Phone if side == 1 => (Vec3::new(0.06 * sc, 0.27 * sc, up - 0.31 * sc), Vec3::new(s, -0.5, -1.0)),
                    Idle::Phone => (Vec3::new(0.0, 0.25 * sc, up - 0.36 * sc), Vec3::new(s, -0.5, -1.0)),
                };
                Some((mid + pelvis_q * at, pelvis_q * pole))
            };
            let blend = |w: Idle| way(w).unwrap_or((target, pole));
            let (fa, fp) = blend(self.idle_from);
            let (ta, tp) = blend(self.idle_to);
            let through_body = (self.idle_from == Idle::Behind) != (self.idle_to == Idle::Behind) && way(self.idle_from).is_some() && way(self.idle_to).is_some();
            let (it, ip) = if !through_body {
                let m = smoothstep(0.0, 1.0, self.idle_mix);
                (fa.lerp(ta, m), fp.lerp(tp, m))
            } else if self.idle_mix < 0.5 {
                let m = smoothstep(0.0, 0.5, self.idle_mix);
                (fa.lerp(target, m), fp.lerp(pole, m))
            } else {
                let m = smoothstep(0.5, 1.0, self.idle_mix);
                (target.lerp(ta, m), pole.lerp(tp, m))
            };
            target = target.lerp(it, still);
            pole = pole.lerp(ip, still);
        }
        if side == 1 && self.reach > 0.0 {
            let r = smoothstep(0.0, 1.0, self.reach);
            let reach_to = self.reach_at;
            // not further than the arm, and not through the body
            let v = reach_to - sh_at;
            let reach_to = sh_at
                + v.normalize_or(Vec3::Y)
                    * v.length().min((rig.upper_arm + rig.forearm) * 0.96);
            target = target.lerp(reach_to, r);
            pole = pole.lerp(Vec3::new(0.6, -0.3, -1.0), r);
        }
        if self.grip > 0.0 {
            // on the steering wheel: the elbows down and a little out
            let g = smoothstep(0.0, 1.0, self.grip);
            let v = self.grip_at[side] - sh_at;
            let to = sh_at
                + v.normalize_or(Vec3::Y) * v.length().min((rig.upper_arm + rig.forearm) * 0.97);
            target = target.lerp(to, g);
            pole = pole.lerp(Vec3::new(s * 0.7, -0.2, -1.0), g);
        }
        (target, pole)
    }

    /// The hand's rotation: the relaxed wrist, or round the steering wheel's rim.
    fn hand_rot(&self, side: usize, c0: Vec3, r_fore: Mat3A) -> Mat3A {
        let d = |deg: f32| deg.to_radians();
        // the wrist: relaxed, a little flexed towards the palm (down in the T-pose);
        // flat for the desk
        let hinge0 = c0
            .normalize_or(Vec3::X)
            .cross(-Vec3::Z)
            .normalize_or(Vec3::Y);
        let wrist_flex = d(10.0) * (1.0 - if side == 1 { self.reach } else { 0.0 });
        let mut r_hand = r_fore * Mat3A::from_axis_angle(hinge0, wrist_flex);
        if let (true, Some(frames)) = (self.grip > 0.0, self.grip_frame) {
            // round the rim: the knuckles along it, the palm against it (the rest hand
            // lies palm down along the forearm); the wrist bends 80 degrees at most
            let (dir, palm) = frames[side];
            if dir.length_squared() > 1e-6 && palm.length_squared() > 1e-6 {
                let want = bone_rot(c0, -Vec3::Z, dir.normalize(), palm.normalize());
                let q_fore = Quat::from_mat3a(&r_hand);
                let rel = limit_quat(q_fore.inverse() * Quat::from_mat3a(&want), d(80.0));
                let q = q_fore.slerp(q_fore * rel, smoothstep(0.0, 1.0, self.grip));
                r_hand = Mat3A::from_quat(q);
            }
        }
        r_hand
    }
}
