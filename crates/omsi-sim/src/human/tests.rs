use super::*;

#[test]
fn standard_bone_names_have_engine_ids() {
    assert_eq!(bone_id_by_name("OS_L"), Some(BONE_OS_L));
    assert_eq!(bone_id_by_name("hand_r"), Some(BONE_HAND_R));
    assert_eq!(bone_id_by_name("unknown"), None);
}

/// A rig like the stock adults', without a mesh.
fn rig() -> Rig {
    let def = Human {
        height: 1.77,
        seat_height: 0.82,
        links: vec![
            0.09, 0.0, 0.92, 0.09, -0.03, 0.53, 0.02, 1.17, 0.18, -0.05, 1.43, 0.44, -0.04,
            1.41, -0.02, 1.55, 0.69, -0.03, 1.43, 0.9, -0.03, 1.43,
        ],
        walk_param: [1.4, 80.0, 1.0, 1.0, 0.0],
        ..Default::default()
    };
    let j = Joints::from_links(&def.links);
    Rig::measure(&def, &j, &[])
}

fn walk(
    rig: &Rig,
    pose: &mut Pose,
    speed: f64,
    secs: f32,
    dt: f32,
    pos: &mut DVec3,
    mut check: impl FnMut(&Pose, &Posed, DVec3),
) {
    let steps = (secs / dt) as usize;
    for _ in 0..steps {
        *pos += DVec3::new(0.0, speed * dt as f64, 0.0);
        let input = PoseInput {
            activity: Activity::Walk,
            origin: *pos,
            velocity: DVec2::new(0.0, speed),
            ..Default::default()
        };
        pose.advance(rig, &input, dt);
        let posed = pose.bones(rig);
        check(pose, &posed, *pos);
    }
}

#[test]
fn rig_is_sane() {
    let r = rig();
    assert!((r.thigh - 0.39).abs() < 0.01, "{}", r.thigh);
    assert!(r.shin > 0.35 && r.shin < 0.5, "{}", r.shin);
    assert!(r.heel < 0.0 && r.ball > 0.0 && r.toe > r.ball);
    assert!(r.seat_lift > 0.05 && r.seat_lift < 0.15);
    let c = r.cadence(1.35);
    assert!(c > 1.7 && c < 2.1, "cadence {c} steps/s at 1.35 m/s");
    // `[walk_param]` 1.4 (the stride) / 80 (an arm angle): 0.7 m steps
    assert!((r.walk_step - 0.7).abs() < 1e-6, "{}", r.walk_step);
    // slower than 1.2 m/s the stride shortens and the pace stays: 2.4 / 1.4 steps a second
    assert!((r.cadence(0.6) - 2.4 / 1.4).abs() < 1e-4, "{}", r.cadence(0.6));
}

#[test]
fn standing_is_rest_like_and_finite() {
    let r = rig();
    let mut p = Pose::new(7);
    // (the arms hanging: the other ways of standing are tried below)
    (p.idle_from, p.idle_to) = (Idle::Hang, Idle::Hang);
    let input = PoseInput::default();
    for _ in 0..200 {
        p.advance(&r, &input, 1.0 / 60.0);
    }
    let posed = p.bones(&r);
    for b in &posed.bones {
        assert!(b.is_finite());
    }
    for side in 0..2 {
        assert!(
            posed.sole[side].abs() < 0.01,
            "sole {} at {}",
            side,
            posed.sole[side]
        );
        assert!(
            posed.knee_flex[side] < 20.0,
            "knee {}",
            posed.knee_flex[side]
        );
        // the arms hang down beside the body
        assert!(posed.wrist[side].z < 1.0, "wrist z {}", posed.wrist[side].z);
        assert!(
            posed.wrist[side].x * SIDE[side] > 0.2,
            "wrist x {}",
            posed.wrist[side].x
        );
    }
}

#[test]
fn walking_plants_the_feet() {
    let r = rig();
    let mut p = Pose::new(3);
    let mut pos = DVec3::ZERO;
    let dt = 1.0 / 60.0;
    let speed = 1.35;
    // get going
    walk(&r, &mut p, speed, 2.0, dt, &mut pos, |_, _, _| {});
    let mut last: [Option<(bool, DVec3)>; 2] = [None, None];
    let mut max_slide = 0.0f64;
    let mut knee_max = [0f32; 2];
    let mut knee_min = [180f32; 2];
    let mut lowest = 0f32;
    let mut miss = 0f32;
    let mut lifts = 0;
    let mut max_drop = 0f32;
    let (catch_ups, shuffles) = (p.catch_ups, p.shuffles);
    walk(&r, &mut p, speed, 4.0, dt, &mut pos, |pose, posed, _| {
        max_drop = max_drop.max(pose.drop);
        for side in 0..2 {
            let f = pose.feet[side];
            if let Some((was, at)) = last[side] {
                if was && f.planted {
                    max_slide = max_slide.max((f.pos - at).length());
                }
                if was && !f.planted {
                    lifts += 1;
                }
            }
            last[side] = Some((f.planted, f.pos));
            knee_max[side] = knee_max[side].max(posed.knee_flex[side]);
            knee_min[side] = knee_min[side].min(posed.knee_flex[side]);
            lowest = lowest.min(posed.sole[side]);
            if f.planted {
                miss = miss.max(posed.leg_miss[side]);
            }
            assert!(posed.ok && posed.bones.iter().all(|b| b.is_finite()));
        }
    });
    assert!(max_slide < 1e-9, "a planted foot moved {max_slide} m");
    assert!(
        max_drop < 0.12,
        "the hips sank {max_drop} m in a steady walk"
    );
    assert_eq!(
        p.catch_ups, catch_ups,
        "steady walking needed catch-up steps"
    );
    assert_eq!(p.shuffles, shuffles, "a foot was pulled in while walking");
    // 1.35 m/s at ~1.8 steps/s: about 7 steps in 4 s
    assert!(lifts >= 6 && lifts <= 9, "{lifts} steps in 4 s");
    for side in 0..2 {
        assert!(
            knee_max[side] > 40.0 && knee_max[side] < 80.0,
            "knee flexes up to {}",
            knee_max[side]
        );
        assert!(
            knee_min[side] >= 0.0 && knee_min[side] < 15.0,
            "knee at least {}",
            knee_min[side]
        );
    }
    assert!(lowest > -0.015, "a sole went {lowest} m through the floor");
    assert!(miss < 0.03, "a planted foot was missed by {miss} m");
}

#[test]
fn stopping_and_turning_bring_the_feet_back() {
    let r = rig();
    let mut p = Pose::new(11);
    let mut pos = DVec3::ZERO;
    walk(&r, &mut p, 1.2, 3.0, 1.0 / 60.0, &mut pos, |_, _, _| {});
    // stop and turn round on the spot
    let mut heading = 0.0;
    for k in 0..240 {
        if k < 90 {
            heading += 2.0;
        }
        let input = PoseInput {
            origin: pos,
            heading,
            ..Default::default()
        };
        p.advance(&r, &input, 1.0 / 60.0);
    }
    let posed = p.bones(&r);
    assert!(p.feet.iter().all(|f| f.planted));
    for side in 0..2 {
        let (rest, yaw) = p.rest_target(&r, side, pos, heading, false);
        assert!(
            (p.feet[side].pos - rest).truncate().length() < 0.14,
            "foot {side} {:?} vs {:?}",
            p.feet[side].pos,
            rest
        );
        assert!(angle_diff(p.feet[side].yaw, yaw).abs() < 25.0);
        assert!(posed.sole[side].abs() < 0.02);
    }
}

#[test]
fn sitting_down_puts_the_hips_on_the_seat() {
    let r = rig();
    let mut p = Pose::new(5);
    let seat = Vec3::new(0.0, -r.seat_front(), 0.45);
    let mut settling_seen = false;
    p.advance(&r, &PoseInput::default(), 1.0 / 60.0);
    for k in 0..120 {
        let input = PoseInput {
            activity: Activity::Sit,
            seat: Some(seat),
            ..Default::default()
        };
        p.advance(&r, &input, 1.0 / 60.0);
        let posed = p.bones(&r);
        settling_seen |= p.settling();
        assert!(
            posed.ok && posed.bones.iter().all(|b| b.is_finite()),
            "frame {k}"
        );
    }
    assert!(settling_seen);
    let input = PoseInput {
        activity: Activity::Sit,
        seat: Some(seat),
        ..Default::default()
    };
    for _ in 0..30 {
        p.advance(&r, &input, 1.0 / 60.0);
    }
    let posed = p.bones(&r);
    let hips = (posed.hip[0] + posed.hip[1]) * 0.5;
    assert!(
        (hips.z - (seat.z + r.seat_lift)).abs() < 0.05,
        "hips at {:?}",
        hips
    );
    assert!((hips.y - seat.y).abs() < 0.08, "hips at {:?}", hips);
    for side in 0..2 {
        assert!(
            posed.knee_flex[side] > 60.0,
            "knee {}",
            posed.knee_flex[side]
        );
        assert!(
            posed.sole[side].abs() < 0.03,
            "feet on the floor: {}",
            posed.sole[side]
        );
        // hands in the lap, above the thighs
        assert!(
            posed.wrist[side].z > seat.z && posed.wrist[side].y > seat.y,
            "wrist {:?}",
            posed.wrist[side]
        );
    }
    // and up again
    let input = PoseInput::default();
    for _ in 0..90 {
        p.advance(&r, &input, 1.0 / 60.0);
    }
    let posed = p.bones(&r);
    assert!(p.sit_amount() == 0.0);
    let hips = (posed.hip[0] + posed.hip[1]) * 0.5;
    assert!(hips.z > r.pelvis.z - 0.05, "stood up: {:?}", hips);
}

#[test]
fn paying_reaches_the_desk_and_looks() {
    let r = rig();
    let mut p = Pose::new(9);
    let desk = Vec3::new(0.02, 0.5, 1.08);
    let input = PoseInput {
        activity: Activity::Pay,
        reach: Some(desk),
        look: Some(Vec3::new(-0.8, 0.9, 1.3)),
        ..Default::default()
    };
    for _ in 0..90 {
        p.advance(&r, &input, 1.0 / 60.0);
    }
    let posed = p.bones(&r);
    assert!(
        (posed.wrist[1] - desk).length() < 0.08,
        "wrist {:?}",
        posed.wrist[1]
    );
    assert!(p.head.x < -20.0, "head yaw {}", p.head.x);
}

#[test]
fn jerky_crowd_motion_does_not_crouch() {
    // the crowd starts, stops, turns and shoves people about
    let r = rig();
    let mut p = Pose::new(4);
    let mut pos = DVec3::ZERO;
    let dt = 1.0 / 60.0;
    let mut worst_drop = 0f32;
    let mut worst_miss = 0f32;
    let mut worst_knee = 0f32;
    let mut deep = 0;
    for k in 0..1200 {
        let t = k as f64 * dt as f64;
        let v = match (t * 1.3) as i64 % 5 {
            0 => DVec2::new(0.0, 1.6),
            1 => DVec2::new(0.9, 0.3),
            2 => DVec2::ZERO,
            3 => DVec2::new(-1.4, -0.6),
            _ => DVec2::new(0.0, 0.25 * (t * 9.0).sin()),
        };
        pos += (v * dt as f64).extend(0.0);
        let heading = if v.length() > 0.2 {
            v.x.atan2(v.y).to_degrees()
        } else {
            0.0
        };
        let input = PoseInput {
            activity: Activity::Walk,
            origin: pos,
            heading,
            velocity: v,
            ..Default::default()
        };
        p.advance(&r, &input, dt);
        let posed = p.bones(&r);
        worst_drop = worst_drop.max(p.drop);
        if p.drop > 0.11 {
            deep += 1;
        }
        for side in 0..2 {
            if p.feet[side].planted {
                worst_miss = worst_miss.max(posed.leg_miss[side]);
                worst_knee = worst_knee.max(posed.knee_flex[side]);
            }
        }
    }
    assert!(worst_drop <= 0.12, "the pelvis dropped {worst_drop} m");
    assert!(deep < 150, "the hips were low for {deep} of 1200 frames");
    assert!(
        worst_knee < 70.0,
        "a knee carrying weight bent {worst_knee} degrees"
    );
    assert!(
        worst_miss < 0.05,
        "a planted foot was missed by {worst_miss} m"
    );
}

#[test]
fn bad_input_and_empty_rigs_stay_finite() {
    // a .hum without [links] and without a mesh
    let empty = Rig::measure(&Human::default(), &Joints::from_links(&[]), &[]);
    for r in [rig(), empty] {
        let mut p = Pose::new(1);
        let inputs = [
            PoseInput {
                velocity: DVec2::new(f64::NAN, 1.0),
                ..Default::default()
            },
            PoseInput {
                origin: DVec3::new(1e7, -1e7, 30.0),
                heading: 1e9,
                velocity: DVec2::new(40.0, 0.0),
                ..Default::default()
            },
            PoseInput {
                activity: Activity::Sit,
                seat: Some(Vec3::new(0.0, 5.0, -3.0)),
                ..Default::default()
            },
            PoseInput {
                activity: Activity::Pay,
                reach: Some(Vec3::ZERO),
                look: Some(Vec3::new(0.0, -0.0, 1.55)),
                sway: Vec2::splat(1e6),
                hold: 7.0,
                ..Default::default()
            },
            PoseInput {
                origin: DVec3::new(f64::NAN, 0.0, 0.0),
                ..Default::default()
            },
        ];
        for (k, input) in inputs.iter().cycle().take(400).enumerate() {
            p.advance(&r, input, if k % 7 == 0 { 5.0 } else { 1.0 / 60.0 });
            let posed = p.bones(&r);
            assert!(
                posed.ok && posed.bones.iter().all(|b| b.is_finite()),
                "input {}",
                k % inputs.len()
            );
        }
    }
}

#[test]
fn a_step_up_is_climbed() {
    let r = rig();
    let mut p = Pose::new(2);
    let floor = |at: DVec2| -> Option<f64> { Some(if at.y > 1.0 { 0.3 } else { 0.0 }) };
    let mut pos = DVec3::ZERO;
    let dt = 1.0 / 60.0;
    for _ in 0..240 {
        pos.y += 0.9 * dt as f64;
        pos.z = if pos.y > 1.0 {
            (pos.z + 1.2 * dt as f64).min(0.3)
        } else {
            0.0
        };
        let input = PoseInput {
            activity: Activity::Walk,
            origin: pos,
            velocity: DVec2::new(0.0, 0.9),
            floor: Some(&floor),
            ..Default::default()
        };
        p.advance(&r, &input, dt);
        let posed = p.bones(&r);
        assert!(posed.ok && posed.bones.iter().all(|b| b.is_finite()));
    }
    for f in &p.feet {
        if f.planted {
            assert!(
                (f.pos.z - if f.pos.y > 1.0 { 0.3 } else { 0.0 }).abs() < 1e-6,
                "{:?}",
                f.pos
            );
        }
    }
}

/// Down a flight whose floor the feet only know in steps (a landing 0.7 m up, then the
/// aisle) while the body slides down it evenly, as on the SD202's stairs: a foot left on
/// the landing lifted the body's floor, the other one landing on the aisle below could
/// not be reached and stepped again and again, and the first was dragged along the
/// aisle a flight up. Once the body is down, both feet are down.
#[test]
fn a_foot_left_on_a_landing_comes_down() {
    let r = rig();
    let mut p = Pose::new(3);
    let floor = |at: DVec2| -> Option<f64> { Some(if at.x < -0.5 { 0.7 } else { 0.0 }) };
    let dt = 1.0 / 60.0;
    let mut pos = DVec3::new(-1.2, 0.0, 0.7);
    let mut worst: f64 = 0.0;
    for k in 0..400 {
        let v = if k < 30 { 0.0 } else { 0.8 };
        pos.x += v * dt as f64;
        pos.z = (0.7 * (-pos.x / 0.85)).clamp(0.0, 0.7);
        let input = PoseInput {
            activity: Activity::Walk,
            origin: pos,
            heading: 90.0,
            velocity: DVec2::new(v, 0.0),
            floor: Some(&floor),
            ..Default::default()
        };
        p.advance(&r, &input, dt);
        assert!(p.bones(&r).ok);
        if pos.x > 1.2 {
            for f in p.feet.iter().filter(|f| f.planted) {
                worst = worst.max(f.pos.z);
            }
        }
    }
    assert!(
        worst < 1e-6,
        "a foot stayed {worst:.2} m up ({})",
        p.describe()
    );
}

/// Stepping off a bus: the feet on its floor (0.45 m up) are carried into the ground's
/// frame where they are, and within a second of walking on both are on the pavement.
#[test]
fn feet_come_off_the_bus_floor() {
    let r = rig();
    let mut p = Pose::new(4);
    let bus_floor = |_: DVec2| -> Option<f64> { Some(0.45) };
    let ground = |_: DVec2| -> Option<f64> { Some(0.0) };
    let dt = 1.0 / 60.0;
    let mut pos = DVec3::new(0.0, 0.0, 0.45);
    for _ in 0..90 {
        pos.y += 0.6 * dt as f64;
        let input = PoseInput {
            activity: Activity::Walk,
            origin: pos,
            frame: 1,
            velocity: DVec2::new(0.0, 0.6),
            floor: Some(&bus_floor),
            ..Default::default()
        };
        p.advance(&r, &input, dt);
    }
    pos.z = 0.0;
    let mut worst: f64 = 0.0;
    for k in 0..120 {
        pos.y += 0.8 * dt as f64;
        let input = PoseInput {
            activity: Activity::Walk,
            origin: pos,
            frame: 0,
            velocity: DVec2::new(0.0, 0.8),
            floor: Some(&ground),
            ..Default::default()
        };
        p.advance(&r, &input, dt);
        assert!(p.bones(&r).ok);
        if k > 60 {
            for f in p.feet.iter().filter(|f| f.planted) {
                worst = worst.max(f.pos.z);
            }
        }
    }
    assert!(
        worst < 1e-6,
        "a foot stayed {worst:.2} m up a second after stepping off ({})",
        p.describe()
    );
}

/// A person standing whose heading turns at once by 90 degrees (the sim turns them round to
/// a door or a seat): the hips stay over the feet while the feet step round - turned at
/// once over feet that stayed, the legs ran diagonally and the person seemed to slide.
#[test]
fn a_sudden_turn_keeps_the_legs_under_the_hips() {
    let r = rig();
    let mut p = Pose::new(5);
    let stand = PoseInput::default();
    for _ in 0..120 {
        p.advance(&r, &stand, 1.0 / 60.0);
    }
    let turned = PoseInput { heading: 90.0, ..PoseInput::default() };
    let mut worst = 0.0f32;
    for _ in 0..90 {
        p.advance(&r, &turned, 1.0 / 60.0);
        let posed = p.bones(&r);
        let hips = posed.bones[HIP].transform_vector3(Vec3::Y).truncate().normalize();
        for side in 0..2 {
            let foot = posed.bones[FOOT[side]].transform_vector3(Vec3::Y).truncate().normalize();
            worst = worst.max(hips.dot(foot).clamp(-1.0, 1.0).acos().to_degrees());
        }
    }
    assert!(worst < 50.0, "a foot {worst:.0} degrees off the hips' way");
}

/// Every way of standing gives a finite pose with the hands where that way puts them: in
/// front for clasped hands, the crossed arms and the phone, behind for the hands behind
/// the back, out at the side for a hand on the hip - and the feet stay on the floor.
#[test]
fn every_way_of_standing_puts_the_hands_where_it_should() {
    let r = rig();
    for (way, check) in [
        (Idle::Clasped, (|w: Vec3, _s: f32| w.y > 0.08) as fn(Vec3, f32) -> bool),
        (Idle::Behind, |w, _| w.y < -0.05),
        (Idle::Crossed, |w, s| w.y > 0.1 && w.z > 1.1 && w.x * s > 0.0),
        (Idle::Phone, |w, _| w.y > 0.1 && w.z > 1.0),
    ] {
        let mut p = Pose::new(3);
        (p.idle_from, p.idle_to, p.idle_t) = (way, way, 1e9);
        for _ in 0..120 {
            p.advance(&r, &PoseInput::default(), 1.0 / 60.0);
        }
        let posed = p.bones(&r);
        assert!(posed.ok && posed.bones.iter().all(|b| b.is_finite()), "{way:?}");
        for side in 0..2 {
            assert!(check(posed.wrist[side], SIDE[side]), "{way:?}: wrist {side} at {:?}", posed.wrist[side]);
            assert!(posed.sole[side].abs() < 0.02, "{way:?}: sole {}", posed.sole[side]);
        }
    }
    let mut p = Pose::new(3);
    (p.idle_from, p.idle_to, p.idle_t) = (Idle::HandOnHip, Idle::HandOnHip, 1e9);
    for _ in 0..120 {
        p.advance(&r, &PoseInput::default(), 1.0 / 60.0);
    }
    let posed = p.bones(&r);
    let side = if p.style[2] > 0.0 { 1 } else { 0 };
    assert!(posed.wrist[side].x * SIDE[side] > 0.2 && posed.wrist[side].z > 0.85, "hand on the hip at {:?}", posed.wrist[side]);
}

#[test]
fn a_standing_rider_keeps_the_hands_down() {
    let r = rig();
    let mut p = Pose::new(3);
    (p.idle_from, p.idle_to) = (Idle::Hang, Idle::Hang);
    let input = PoseInput { hold: 1.0, ..Default::default() };
    for _ in 0..200 {
        p.advance(&r, &input, 1.0 / 60.0);
    }
    let posed = p.bones(&r);
    for side in 0..2 {
        assert!(posed.wrist[side].z < r.shoulder[side].z, "wrist {side} at {:?}", posed.wrist[side]);
    }
}

#[test]
fn changing_the_way_of_standing_keeps_the_hands_out_of_the_body() {
    let r = rig();
    for (from, to) in [(Idle::Behind, Idle::Clasped), (Idle::Behind, Idle::Phone), (Idle::Clasped, Idle::Behind), (Idle::Phone, Idle::Behind)] {
        let mut p = Pose::new(3);
        (p.idle_from, p.idle_to, p.idle_t) = (from, from, 1e9);
        for _ in 0..120 {
            p.advance(&r, &PoseInput::default(), 1.0 / 60.0);
        }
        (p.idle_from, p.idle_to, p.idle_mix) = (from, to, 0.0);
        for _ in 0..90 {
            p.advance(&r, &PoseInput::default(), 1.0 / 60.0);
            let posed = p.bones(&r);
            for side in 0..2 {
                let w = posed.wrist[side];
                let inside = w.x.abs() < 0.12 && w.y.abs() < 0.08 && w.z > r.hip[side].z - 0.05 && w.z < r.shoulder[side].z;
                assert!(!inside, "{from:?} to {to:?}: wrist {side} at {w:?}, mix {}", p.idle_mix);
            }
        }
    }
}

#[test]
fn setting_off_steps_left_and_right_in_turn() {
    let r = rig();
    for dt in [1.0 / 60.0, 0.0667] {
        for seed in 0..36 {
            let speed = [0.7, 1.05, 1.45][seed as usize % 3];
            let mut p = Pose::new(seed);
            let mut pos = DVec3::ZERO;
            let mut steps = Vec::new();
            let mut was = [true; 2];
            let mut last_ankle: Option<[Vec3; 2]> = None;
            for i in 0..(5.0 / dt) as usize {
                let v = if (i as f32) * dt < 1.0 { 0.0 } else { speed };
                pos.y += v * dt as f64;
                let input = PoseInput { activity: Activity::Walk, origin: pos, velocity: DVec2::new(0.0, v), ..Default::default() };
                p.advance(&r, &input, dt);
                let ankle = p.bones(&r).ankle;
                if let Some(last) = last_ankle {
                    for side in 0..2 {
                        let jump = (ankle[side] - last[side]).length() / dt;
                        assert!(jump < 8.0, "seed {seed} dt {dt}: foot {side} jumped at {jump} m/s at {}", i as f32 * dt);
                    }
                }
                last_ankle = Some(ankle);
                for side in 0..2 {
                    if p.planted(side) && !was[side] {
                        steps.push(side);
                    }
                    was[side] = p.planted(side);
                }
            }
            assert!(steps.len() >= 6, "seed {seed} dt {dt}: {steps:?}");
            assert!(steps.windows(2).all(|w| w[0] != w[1]), "seed {seed} dt {dt}: one foot stepped twice running: {steps:?}");
        }
    }
}

#[test]
fn walking_looks_where_it_goes() {
    let r = rig();
    for seed in 0..30 {
        let mut p = Pose::new(seed);
        let mut pos = DVec3::ZERO;
        let mut worst = 0.0f32;
        for i in 0..600 {
            let v = if i < 200 { 0.0 } else { 1.2 };
            pos.y += v / 60.0;
            let input = PoseInput { activity: Activity::Walk, origin: pos, velocity: DVec2::new(0.0, v), ..Default::default() };
            p.advance(&r, &input, 1.0 / 60.0);
            if i > 320 {
                worst = worst.max(p.head.x.abs());
            }
        }
        assert!(worst < 30.0, "seed {seed}: the head turned {worst} degrees while walking");
    }
}

#[test]
fn people_stand_and_walk_in_ways_of_their_own() {
    let r = rig();
    let poses: Vec<Pose> = (0..40).map(Pose::new).collect();
    let spread = |f: &dyn Fn(&Pose) -> f32| {
        let v: Vec<f32> = poses.iter().map(f).collect();
        v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
    };
    let width = |p: &Pose| {
        let (l, _) = p.rest_target(&r, 0, DVec3::ZERO, 0.0, false);
        let (rt, _) = p.rest_target(&r, 1, DVec3::ZERO, 0.0, false);
        (rt.x - l.x) as f32
    };
    assert!(spread(&width) > 0.1, "stance widths spread {}", spread(&width));
    assert!(spread(&|p: &Pose| p.pace) > 0.18, "paces spread {}", spread(&|p: &Pose| p.pace));
}
