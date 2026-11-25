//! Small authored shoe meshes: test actual skinning, not just virtual ankle markers.
use super::*;

fn character(scale: f32) -> (Rig, HumanMesh) {
    character_with_knee(scale, 0.53)
}

fn character_with_knee(scale: f32, knee_height: f32) -> (Rig, HumanMesh) {
    let mut def = Human {
        height: 1.75 * scale,
        seat_height: 0.82 * scale,
        links: vec![
            0.09, 0.0, 0.92, 0.09, -0.03, 0.53, 0.02, 1.17, 0.18, -0.05, 1.43, 0.44, -0.04, 1.41,
            -0.02, 1.55, 0.69, -0.03, 1.43, 0.9, -0.03, 1.43,
        ],
        ..Human::default()
    };
    def.links[5] = knee_height;
    for v in &mut def.links {
        *v *= scale;
    }
    let mut mesh = HumanMesh {
        data: MeshData::default(),
        materials: vec![],
        bones: vec![],
        skin: vec![],
        alpha: vec![],
        morphs: vec![],
    };
    for side in 0..2 {
        for x in [-0.035, 0.035] {
            for y in [-0.10, 0.15] {
                for z in [0.0, 0.025] {
                    mesh.data
                        .positions
                        .push(Vec3::new(SIDE[side] * 0.09 + x, y, z) * scale);
                    mesh.data.normals.push(Vec3::Z);
                    mesh.skin.push(Influence {
                        n: 1,
                        slot: [SHIN[side] as u8, 0, 0, 0],
                        weight: [1.0, 0.0, 0.0, 0.0],
                    });
                }
            }
        }
    }
    let rig = Rig::measure(
        &def,
        &Joints::from_links(&def.links),
        std::slice::from_ref(&mesh),
    );
    split_feet(&mut mesh, &rig);
    (rig, mesh)
}

#[test]
fn stationary_feet_follow_a_corrected_floor_without_lowering_the_body() {
    let (rig, mesh) = character(1.0);
    let mut pose = Pose::new(1);
    let mut positions = vec![];
    let mut normals = vec![];
    pose.advance(&rig, &PoseInput::default(), 1.0 / 60.0);
    let floor = |_: DVec2| Some(0.16);
    for _ in 0..120 {
        pose.advance(
            &rig,
            &PoseInput {
                origin: DVec3::new(0.0, 0.0, 0.16),
                floor: Some(&floor),
                ..Default::default()
            },
            1.0 / 60.0,
        );
    }
    let posed = pose.bones(&rig);
    skin(&mesh, &posed.bones, &mut positions, &mut normals);
    let min = positions.iter().map(|v| v.z).fold(f32::INFINITY, f32::min);
    assert!(min.abs() < 0.005, "shoe mesh at {min}, floor at local zero");
    assert!((posed.hip[1].z - rig.hip[1].z).abs() < 0.025);
}

#[test]
fn planted_shoe_vertices_match_ik_for_adults_and_children() {
    for (scale, knee) in [
        (0.6, 0.53),
        (0.8, 0.53),
        (1.0, 0.53),
        (1.25, 0.53),
        (1.0, 0.4),
        (1.0, 0.65),
    ] {
        let (rig, mesh) = character_with_knee(scale, knee);
        let mut pose = Pose::new(7);
        let mut positions = vec![];
        let mut normals = vec![];
        let mut origin = DVec3::new(10000.0, -20000.0, 0.45);
        for k in 0..600 {
            let speed = if k < 90 || k > 450 {
                0.0
            } else {
                0.8 * scale as f64
            };
            origin.y += speed / 60.0;
            pose.advance(
                &rig,
                &PoseInput {
                    origin,
                    velocity: DVec2::new(0.0, speed),
                    ..Default::default()
                },
                1.0 / 60.0,
            );
            let posed = pose.bones(&rig);
            skin(&mesh, &posed.bones, &mut positions, &mut normals);
            assert!(posed.ok && posed.bones.iter().all(|b| b.is_finite()));
            assert!(positions.iter().all(|v| v.is_finite()));
            for side in 0..2 {
                if pose.feet[side].planted {
                    let min = positions[side * 8..side * 8 + 8]
                        .iter()
                        .map(|v| v.z)
                        .fold(f32::INFINITY, f32::min);
                    assert!(min > -0.015, "scale {scale}, frame {k}, side {side}: {min}");
                    assert!(
                        (min - posed.sole[side]).abs() < 0.015,
                        "virtual/mesh mismatch {min} / {}",
                        posed.sole[side]
                    );
                }
            }
        }
    }
}

#[test]
fn seat_anchor_is_converted_once_for_low_and_high_seats() {
    for scale in [0.6, 1.0, 1.25] {
        let (rig, mesh) = character(scale);
        let model_height = 0.82 * scale;
        for physical_height in [0.3, 0.45, 0.85] {
            let lift = model_height - physical_height;
            let mut pose = Pose::new(3);
            let input = PoseInput {
                activity: Activity::Sit,
                origin: DVec3::new(0.0, 0.0, 0.4),
                seat: Some(Vec3::new(
                    0.0,
                    rig.seat_front() - 0.03,
                    physical_height - rig.seat_lift,
                )),
                ..Default::default()
            };
            for _ in 0..120 {
                pose.advance(&rig, &input, 1.0 / 60.0);
            }
            let posed = pose.bones(&rig);
            // #203's conversion back to the renderer anchor (hip - model seatheight).
            let shift = Affine3A::from_translation(Vec3::Z * lift);
            let bones = posed.bones.map(|b| shift * b);
            let mut vertices = vec![];
            skin(&mesh, &bones, &mut vertices, &mut vec![]);
            assert!((posed.hip[1].z + lift - model_height).abs() < 0.005);
            assert!(
                vertices
                    .iter()
                    .all(|v| v.is_finite() && v.z >= lift - 0.015)
            );
        }
    }
}

#[test]
fn floor_queries_are_bounded_for_a_stationary_crowd() {
    use std::cell::Cell;
    let (rig, _) = character(1.0);
    let queries = Cell::new(0);
    let floor = |_: DVec2| {
        queries.set(queries.get() + 1);
        Some(0.45)
    };
    let input = PoseInput {
        origin: DVec3::new(0.0, 0.0, 0.45),
        floor: Some(&floor),
        ..Default::default()
    };
    let mut crowd: Vec<_> = (0..1000).map(Pose::new).collect();
    for _ in 0..120 {
        for pose in &mut crowd {
            pose.advance(&rig, &input, 1.0 / 60.0);
            assert!(pose.bones(&rig).ok);
        }
    }
    // Two initial supports per person; standing feet do not rescan a map/cabin/mesh
    // every frame. Fidgeting is not due during this two-second interval.
    assert_eq!(queries.get(), 2000);
}

#[test]
fn splitting_feet_preserves_the_original_skinning() {
    let (_, mesh) = character(1.0);
    let legacy = [Affine3A::from_quat(Quat::from_rotation_x(0.17)); crate::human_omsi::BONES];
    let bones = slots_from_omsi(&legacy);
    let mut pos = vec![];
    skin(&mesh, &bones, &mut pos, &mut vec![]);
    for (rest, posed) in mesh.data.positions.iter().zip(pos) {
        assert!(legacy[2].transform_point3(*rest).abs_diff_eq(posed, 1e-6));
    }
}

#[test]
fn mixed_states_board_and_alight_without_stale_floor_support() {
    let (rig, mesh) = character(0.8);
    let mut pose = Pose::new(11);
    let mut origin = DVec3::ZERO;
    for (frame, height, activity) in [
        (0, 0.0, Activity::Stand),
        (0, 0.16, Activity::Walk),
        (1, 0.45, Activity::Walk),
        (1, 0.45, Activity::Pay),
        (1, 0.45, Activity::Sit),
        (1, 0.45, Activity::Stand),
        (0, 0.16, Activity::Walk),
        (0, 0.16, Activity::Stand),
    ] {
        origin.z = height;
        let floor = |_: DVec2| Some(height);
        for k in 0..120 {
            let speed = if activity == Activity::Walk { 0.6 } else { 0.0 };
            origin.y += speed / 60.0;
            pose.advance(
                &rig,
                &PoseInput {
                    frame,
                    origin,
                    activity,
                    floor: Some(&floor),
                    velocity: DVec2::new(0.0, speed),
                    seat: (activity == Activity::Sit).then_some(Vec3::new(0.0, 0.0, 0.4)),
                    ..Default::default()
                },
                1.0 / 60.0,
            );
            let posed = pose.bones(&rig);
            let mut vertices = vec![];
            skin(&mesh, &posed.bones, &mut vertices, &mut vec![]);
            assert!(posed.ok && vertices.iter().all(|v| v.is_finite()));
            assert!(
                vertices.iter().all(|v| v.z > -0.025),
                "frame {frame}, tick {k}, height {height}, {activity:?}, min {}, virtual {:?}, {}",
                vertices.iter().map(|v| v.z).fold(f32::INFINITY, f32::min),
                posed.sole,
                pose.describe()
            );
        }
    }
}
