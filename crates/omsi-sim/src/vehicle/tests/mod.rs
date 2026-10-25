//! Tests of the vehicle: couplings and consists, collisions, restored displays.

include!("../../../../../tools/test-support/original_root.rs");
use super::*;
use std::sync::Arc;

mod articulation;
mod fuel;
mod model_variables;
mod rail_power;
mod scripts_props;

#[test]
fn velocity_is_the_driven_wheels_speed() {
    // a front axle rolling at 10 m/s, the driven rear axle spinning at twice that
    // (wheels of 0.5 m): the speedometer shows the spin, the body's speed stays apart
    let wheels = [(20.0, 0.5, false), (20.0, 0.5, false), (40.0, 0.5, true), (40.0, 0.5, true)];
    assert_eq!(driven_wheels_speed(wheels.into_iter()), Some(20.0));
    // a locked driven axle reads 0, one on the other side of a differential half
    assert_eq!(driven_wheels_speed([(0.0, 0.5, true), (0.0, 0.5, true)].into_iter()), Some(0.0));
    assert_eq!(driven_wheels_speed([(0.0, 0.5, true), (20.0, 0.5, true)].into_iter()), Some(5.0));
    // a trailer without driven wheels: the body's speed is used
    assert_eq!(driven_wheels_speed([(20.0, 0.5, false)].into_iter()), None);
}

pub(super) fn coupling_test_type(boogies: Option<f32>) -> Arc<VehicleType> {
    Arc::new(VehicleType {
        def: Vehicle {
            boogies,
            coupling_front: Some(omsi_vehicle::vehicle::Coupling {
                pos: [0.0, 5.0, 0.3],
            }),
            coupling_back: Some(omsi_vehicle::vehicle::Coupling {
                pos: [0.0, -5.0, 0.3],
            }),
            axles: [-3.0, 3.0]
                .map(|long| omsi_vehicle::Axle {
                    long,
                    ..Default::default()
                })
                .to_vec(),
            ..Default::default()
        },
        model: Model::default(),
        model_dir: PathBuf::new(),
        program: Arc::new(Program::default()),
        meshes: Vec::new(),
        lod_meshes: Vec::new(),
        paint_schemes: Vec::new(),
        texchanges: Vec::new(),
        wheel_meshes: Vec::new(),
        suspension_axles: Vec::new(),
        missing_packs: Vec::new(),
        mesh_bounds: Vec::new(),
        // Deliberately includes a protruding coupler beyond the declared joint.
        mesh_boxes: vec![(Vec3::new(-1.0, -6.2, 0.0), Vec3::new(1.0, 6.2, 3.0))],
    })
}

#[test]
fn bogie_orientation_is_independent_of_consist_flags() {
    for bogies in [None, Some(5.0), Some(-5.0), Some(0.0)] {
        let ty = coupling_test_type(bogies);
        let intrinsic = bogies.is_some_and(|b| b < 0.0);
        let mut v = VehicleInstance::new(ty.clone(), VehicleHost::new(Default::default()));
        v.heading = 37.0;
        v.pitch = 5.0;
        v.bank = 2.0;
        let expected = body_rotation(37.0, 5.0, 2.0, false);
        let forward = expected.transform_vector3(Vec3::Y);
        assert!(
            (v.body_rotation().transform_vector3(Vec3::Y).dot(forward)
                - if intrinsic { -1.0 } else { 1.0 })
            .abs()
                < 1e-5
        );
        for reversed in [false, true] {
            let mut t =
                TrailerPart::new_ex(ty.clone(), &ty, false, reversed, &ty.program, 2, 0);
            t.heading = v.heading;
            t.pitch = v.pitch;
            t.bank = v.bank;
            let sign = if intrinsic ^ reversed { -1.0 } else { 1.0 };
            assert!(
                (t.body_rotation().transform_vector3(Vec3::Y).dot(forward) - sign).abs() < 1e-5
            );
            assert!(
                (t.body_rotation().transform_vector3(Vec3::Z)
                    - expected.transform_vector3(Vec3::Z))
                .length()
                    < 1e-5
            );
            assert!(
                (t.body_heading() - (37.0 + if sign < 0.0 { 180.0 } else { 0.0 })).abs() < 1e-5
            );
        }
    }
}

#[test]
fn simple_collision_uses_the_body_frame_for_current_and_previous_pose() {
    for bogies in [None, Some(5.0), Some(-5.0)] {
        for already_inside in [false, true] {
            let mut ty = coupling_test_type(bogies);
            let def = &mut Arc::get_mut(&mut ty).unwrap().def;
            def.bounding_box = Some([2.0, 2.0, 2.0, 0.0, 3.0, 1.0]);
            def.rolling_resistance = 0.0;
            let mut v = VehicleInstance::new(ty, VehicleHost::new(Default::default()));
            v.collision = Some(Arc::new(crate::collision::CollisionWorld::default()));
            let center = if bogies == Some(-5.0) { -3.0 } else { 3.0 };
            let y = center + if already_inside { 0.0 } else { 1.15 };
            v.dynamic_boxes.push(crate::collision::Obb::point(
                DVec3::new(0.0, y, 1.0),
                0.025,
            ));
            v.set_speed(2.0);
            v.step_physics(0.1);
            if already_inside {
                assert!((v.position.y - 0.2).abs() < 1e-6);
                assert_eq!(v.physics.speed, 2.0);
            } else {
                assert_eq!(v.position, DVec3::ZERO, "{bogies:?}");
                assert_eq!(v.physics.speed, 0.0);
                let sign = if bogies == Some(-5.0) { -1.0 } else { 1.0 };
                assert!((v.host.coll_pos[1] as f64 - sign * (y - 0.2)).abs() < 1e-5);
            }
        }
    }
}

#[test]
fn simple_physics_reports_a_moving_vehicle_striking_a_stationary_bus() {
    let mut ty = coupling_test_type(None);
    let def = &mut Arc::get_mut(&mut ty).unwrap().def;
    def.bounding_box = Some([2.0, 2.0, 2.0, 0.0, 0.0, 1.0]);
    def.rolling_resistance = 0.0;
    let mut bus = VehicleInstance::new(ty, VehicleHost::new(Default::default()));
    bus.physics.mass_kg = 10_000.0;
    bus.dynamic_boxes.push(
        crate::collision::Obb::from_box(
            [0.8, 0.4, 2.0, 0.0, 0.0, 1.0],
            DVec3::new(0.0, -1.1, 0.0),
            0.0,
        )
        .moving(glam::DVec2::new(0.0, 10.0), 1_000.0, 7),
    );

    // The bus is stationary; the AI car moves into it during this tick.
    bus.step_physics(0.1);

    let impacts = bus.take_dynamic_impacts();
    assert_eq!(impacts.len(), 1, "{impacts:?}");
    let impact = impacts[0];
    assert_eq!(impact.obstacle_id, -9);
    assert!(impact.speed > crate::rigid::CRASH_SPEED, "{impact:?}");
    assert!(impact.energy >= 1_000.0, "{impact:?}");
    assert!(impact.point.y < 0.0, "{impact:?}");
    assert!(impact.push.y < 0.0, "{impact:?}");
    assert!(bus.collided);
    assert_eq!(bus.physics.speed, 0.0);
}

/// A car creeping into the standing bus (below the crash speed) does not shove it along
/// at its own pace, and the crash of a car running into it counts the pair's energy.
#[test]
fn simple_physics_a_creeping_car_does_not_shove_the_standing_bus() {
    let mut ty = coupling_test_type(None);
    let def = &mut Arc::get_mut(&mut ty).unwrap().def;
    def.bounding_box = Some([2.0, 2.0, 2.0, 0.0, 0.0, 1.0]);
    def.rolling_resistance = 0.0;
    let mut bus = VehicleInstance::new(ty, VehicleHost::new(Default::default()));
    bus.physics.mass_kg = 10_000.0;
    let y0 = bus.position.y;
    let mut car = crate::collision::Obb::from_box([0.8, 0.4, 2.0, 0.0, 0.0, 1.0], DVec3::new(0.0, -1.19, 0.0), 0.0)
        .moving(glam::DVec2::new(0.0, 0.3), 1_000.0, 7);
    for _ in 0..60 {
        car.center += car.velocity / 30.0;
        bus.dynamic_boxes = vec![car];
        bus.step_physics(1.0 / 30.0);
    }
    assert!(bus.position.y - y0 < 0.1, "the bus was shoved {:.2} m", bus.position.y - y0);
    assert!(bus.take_dynamic_impacts().is_empty());

    let mut ty = coupling_test_type(None);
    let def = &mut Arc::get_mut(&mut ty).unwrap().def;
    def.bounding_box = Some([2.0, 2.0, 2.0, 0.0, 0.0, 1.0]);
    let mut bus = VehicleInstance::new(ty, VehicleHost::new(Default::default()));
    bus.physics.mass_kg = 10_000.0;
    bus.dynamic_boxes.push(
        crate::collision::Obb::from_box([0.8, 0.4, 2.0, 0.0, 0.0, 1.0], DVec3::new(0.0, -1.1, 0.0), 0.0)
            .moving(glam::DVec2::new(0.0, 10.0), 1_000.0, 7),
    );
    bus.step_physics(0.1);
    let impact = bus.take_dynamic_impacts()[0];
    // 0.5 v² (1 - e²) over the pair's inverse mass, and the bus's mass for the car's recoil
    assert!((impact.energy - 0.5 * 100.0 * 0.96 / (1.0 / 10_000.0 + 1.0 / 1_000.0)).abs() < 1.0, "{impact:?}");
    assert_eq!(impact.mass, 10_000.0);
}

#[test]
fn simple_physics_separates_persistent_overlap_with_a_moving_vehicle() {
    let mut ty = coupling_test_type(None);
    let def = &mut Arc::get_mut(&mut ty).unwrap().def;
    def.bounding_box = Some([2.0, 2.0, 2.0, 0.0, 0.0, 1.0]);
    def.rolling_resistance = 0.0;
    let mut bus = VehicleInstance::new(ty, VehicleHost::new(Default::default()));
    let car = crate::collision::Obb::from_box(
        [1.0, 2.0, 2.0, 0.0, 0.0, 1.0],
        DVec3::new(0.9, 0.0, 0.0),
        0.0,
    )
    .moving(glam::DVec2::ZERO, 1_000.0, 7);
    bus.dynamic_boxes.push(car);

    bus.step_physics(0.01);

    let body = crate::collision::Obb::from_box(
        [2.0, 2.0, 2.0, 0.0, 0.0, 1.0],
        bus.position,
        bus.body_heading(),
    );
    assert!(!body.overlaps(&car), "position {:?}, car {:?}", bus.position, car);
    assert!(bus.position.x < 0.0);
    assert_eq!(bus.take_dynamic_impacts().len(), 0);
}

#[test]
fn non_bogie_rail_markers_keep_asymmetric_body_end_placement() {
    for marker in 0..2 {
        let mut ty = coupling_test_type(None);
        let ty_mut = Arc::get_mut(&mut ty).unwrap();
        if marker == 0 {
            ty_mut.def.rail_body_osc = Some([0.0; 7]);
        } else {
            ty_mut.def.contact_shoes.push([0.0; 6]);
        }
        ty_mut.mesh_boxes = vec![(
            Vec3::new(-1.0, -7.0, 0.0),
            Vec3::new(1.0, 5.0, 3.0),
        )];
        // The declared joint is inset from the body's rear end.
        ty_mut.def.coupling_back.as_mut().unwrap().pos[1] = -4.4;
        for lead_reversed in [false, true] {
            for reversed in [false, true] {
                let (back, front) = coupling_points(&ty, lead_reversed, &ty, reversed);
                assert_eq!(back.y, if lead_reversed { 5.0 } else { -7.0 });
                assert_eq!(front.y, if reversed { -7.0 } else { 5.0 });
                let (position, heading) = coupling_placement(
                    DVec3::ZERO,
                    0.0,
                    lead_reversed,
                    back.y,
                    reversed,
                    front.y,
                );
                let expected_distance = if lead_reversed { 5.0 } else { 7.0 }
                    + if reversed { 7.0 } else { 5.0 };
                assert!((position.y + expected_distance).abs() < 1e-6);
                assert_eq!(heading, if reversed { 180.0 } else { 0.0 });
            }
        }
    }
}

#[test]
fn consecutive_reversed_parts_keep_absolute_orientation() {
    for bogies in [None, Some(5.0), Some(-5.0)] {
        let ty = coupling_test_type(bogies);
        let mut v = VehicleInstance::new(ty.clone(), VehicleHost::new(Default::default()));
        v.heading = 23.0;
        for reversed in [true, true, false] {
            v.attach_trailer_ex(ty.clone(), reversed);
        }
        for _ in 0..30 {
            v.update_trailers(0.0);
        }
        let h = v.heading.to_radians();
        let forward = DVec3::new(h.sin(), h.cos(), 0.0);
        for (i, t) in v.trailers.iter().enumerate() {
            assert!(
                (t.position + forward * (10.0 * (i + 1) as f64)).length() < 1e-4
            );
            let reversed = (i < 2) ^ (bogies == Some(-5.0));
            assert!(
                (t.body_heading() - (v.heading + if reversed { 180.0 } else { 0.0 })).abs()
                    < 1e-4
            );
        }
    }
}

#[test]
fn bogie_couplings_agree_with_spawn_placement_and_following() {
    for lead_bogies in [None, Some(5.0), Some(-5.0)] {
        for car_bogies in [None, Some(5.0), Some(-5.0)] {
            for lead_reversed in [false, true] {
                for reversed in [false, true] {
                    let lead = coupling_test_type(lead_bogies);
                    let car = coupling_test_type(car_bogies);
                    let (back, front) = coupling_points(&lead, lead_reversed, &car, reversed);
                    let heading = 23.0;
                    let (position, model_heading) = coupling_placement(
                        DVec3::ZERO,
                        heading,
                        body_reversed(&lead.def, lead_reversed),
                        back.y,
                        body_reversed(&car.def, reversed),
                        front.y,
                    );
                    let mut v = VehicleInstance::new(
                        lead.clone(),
                        VehicleHost::new(Default::default()),
                    );
                    v.heading = heading;
                    let lead_rot = body_rotation(
                        heading,
                        0.0,
                        0.0,
                        body_reversed(&lead.def, lead_reversed),
                    );
                    let mut t = TrailerPart::new_ex(
                        car.clone(),
                        &lead,
                        lead_reversed,
                        reversed,
                        &lead.program,
                        2,
                        0,
                    );
                    for _ in 0..30 {
                        t.update(&mut v, 0.0, Some((DVec3::ZERO, lead_rot, heading)));
                    }
                    assert!(
                        (t.position - position).length() < 1e-4,
                        "{lead_bogies:?}/{car_bogies:?}, {lead_reversed}/{reversed}"
                    );
                    assert!((t.body_heading() - model_heading).abs() < 1e-4);
                    if lead_bogies.is_some() && car_bogies.is_some() {
                        assert!((position.length() - 10.0).abs() < 1e-4);
                    }
                    let joint = lead_rot.transform_point3(back).as_dvec3();
                    assert!(
                        (joint
                            - t.position
                            - t.body_rotation().transform_point3(front).as_dvec3())
                        .length()
                            < 1e-4
                    );
                }
            }
        }
    }
}

#[test]
fn coupling_defaults_and_non_bogie_rail_bounds_are_preserved() {
    for bogies in [None, Some(5.0), Some(-5.0)] {
        let mut ty = coupling_test_type(bogies);
        let def = &mut Arc::get_mut(&mut ty).unwrap().def;
        def.coupling_front = None;
        def.coupling_back = None;
        let (back, front) = coupling_points(&ty, false, &ty, false);
        let sign = if bogies == Some(-5.0) { -1.0 } else { 1.0 };
        let z = if bogies.is_some() { 0.0 } else { 0.3 };
        assert_eq!(back, Vec3::new(0.0, -4.0 * sign, z));
        assert_eq!(front, Vec3::new(0.0, 4.0 * sign, z));
    }
    let mut ty = coupling_test_type(None);
    Arc::get_mut(&mut ty).unwrap().def.rail_body_osc = Some([0.0; 7]);
    let (back, front) = coupling_points(&ty, false, &ty, false);
    assert_eq!((back.y, front.y), (-6.2, 6.2));
}

#[test]
fn inset_rail_joints_and_unit_couplers_keep_their_declared_distances() {
    // Two three-part units: the outer ends have longer couplers than the
    // internal joints. Mesh bounds extending past a joint must not stretch it.
    for bogies in [5.0, -5.0] {
        for half_length in [6.6, 7.8] {
            let part = |front: f32, back: f32| {
                let mut ty = coupling_test_type(Some(bogies));
                let ty_mut = Arc::get_mut(&mut ty).unwrap();
                ty_mut.def.coupling_front.as_mut().unwrap().pos[1] = front;
                ty_mut.def.coupling_back.as_mut().unwrap().pos[1] = back;
                ty_mut.mesh_boxes = vec![(
                    Vec3::new(-1.0, -half_length, 0.0),
                    Vec3::new(1.0, half_length, 3.0),
                )];
                ty
            };
            let lead = part(6.3, -5.7);
            let middle = part(5.7, -5.7);
            let tail = part(5.7, -6.3);
            let mut v = VehicleInstance::new(
                lead.clone(),
                VehicleHost::new(Default::default()),
            );
            for ty in [&middle, &tail, &lead, &middle, &tail] {
                v.attach_trailer_ex(ty.clone(), false);
            }
            for heading in [0.0_f64, 180.0] {
                v.heading = heading;
                for t in &mut v.trailers {
                    t.realign();
                }
                for _ in 0..30 {
                    v.update_trailers(0.0);
                }
                let h = heading.to_radians();
                let forward = DVec3::new(h.sin(), h.cos(), 0.0);
                let mut previous = v.position;
                for (t, distance) in v.trailers.iter().zip([11.4, 11.4, 12.6, 11.4, 11.4]) {
                    assert!((previous - t.position - forward * distance).length() < 1e-4);
                    previous = t.position;
                }
            }
        }
    }
}

#[test]
fn a_joint_clear_of_the_body_is_kept_as_declared() {
    let frame = |front: f32, back: f32, half: f32| {
        let mut ty = coupling_test_type(Some(1.1));
        let ty_mut = Arc::get_mut(&mut ty).unwrap();
        ty_mut.def.coupling_front.as_mut().unwrap().pos[1] = front;
        ty_mut.def.coupling_back.as_mut().unwrap().pos[1] = back;
        ty_mut.mesh_boxes =
            vec![(Vec3::new(-1.1, -half, 0.0), Vec3::new(1.1, half, 3.0))];
        ty
    };
    // A bogie frame hung far ahead of its joint (the NF6D's `[coupling_front] 0 -17.71 1`,
    // the BR481's -34.35): along the track that is where it belongs, under its car.
    let wild = frame(-17.71, -5.49, 1.1);
    assert_eq!(
        coupling_points(&wild, false, &wild, false),
        (Vec3::new(0.0, -5.49, 0.0), Vec3::new(0.0, -17.71, 0.0))
    );
    // a coupler standing clear within the body's own length is the mod's, and kept
    let coupler = frame(2.0, -2.4, 1.1);
    assert_eq!(
        coupling_points(&coupler, false, &coupler, false),
        (Vec3::new(0.0, -2.4, 0.0), Vec3::new(0.0, 2.0, 0.0))
    );
    // an inset joint is kept too - the CR200J's overlap in vanilla
    let inset = frame(0.8, -0.9, 1.1);
    assert_eq!(
        coupling_points(&inset, false, &inset, false),
        (Vec3::new(0.0, -0.9, 0.0), Vec3::new(0.0, 0.8, 0.0))
    );
    // a fitting too
    let bellows = frame(-17.71, -5.49, 0.3);
    assert_eq!(
        coupling_points(&bellows, false, &bellows, false),
        (Vec3::new(0.0, -5.49, 0.0), Vec3::new(0.0, -17.71, 0.0))
    );
}

#[test]
fn asymmetric_rail_ends_distinguish_declared_joints_from_body_bounds() {
    for bogies in [None, Some(5.0), Some(-5.0)] {
        let part = |front: f32, back: f32, min_y: f32, max_y: f32| {
            let mut ty = coupling_test_type(bogies);
            let ty_mut = Arc::get_mut(&mut ty).unwrap();
            ty_mut.def.rail_body_osc = Some([0.0; 7]);
            ty_mut.def.coupling_front.as_mut().unwrap().pos[1] = front;
            ty_mut.def.coupling_back.as_mut().unwrap().pos[1] = back;
            ty_mut.mesh_boxes = vec![(
                Vec3::new(-1.0, min_y, 0.0),
                Vec3::new(1.0, max_y, 3.0),
            )];
            ty
        };
        // The first joint is inside the lead body; the tail's joints extend
        // beyond its body. Bogie-defined cars must retain these declared
        // distances even when they produce an overlap or a visible gap.
        let lead = part(15.0, -9.0, -12.0, 15.0);
        let middle = part(13.0, -13.0, -13.0, 13.0);
        let tail = part(14.0, -16.0, -15.0, 12.0);
        for middle_reversed in [false, true] {
            for tail_reversed in [false, true] {
                let mut v = VehicleInstance::new(
                    lead.clone(),
                    VehicleHost::new(Default::default()),
                );
                v.attach_trailer_ex(middle.clone(), middle_reversed);
                v.attach_trailer_ex(tail.clone(), tail_reversed);
                // Without bogies, rail cars still meet at their mesh bounds.
                let distances = if bogies.is_some() {
                    [22.0, if tail_reversed { 29.0 } else { 27.0 }]
                } else {
                    [25.0, if tail_reversed { 28.0 } else { 25.0 }]
                };
                for heading in [37.0_f64, 180.0] {
                    v.heading = heading;
                    for t in &mut v.trailers {
                        t.realign();
                    }
                    let h = heading.to_radians();
                    let forward = DVec3::new(h.sin(), h.cos(), 0.0);
                    for _ in 0..30 {
                        v.update_trailers(0.0);
                        let mut previous = v.position;
                        for (i, (previous_ty, previous_reversed, ty, reversed)) in [
                            (&lead, false, &middle, middle_reversed),
                            (&middle, middle_reversed, &tail, tail_reversed),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let expected = previous - forward * distances[i];
                            let (back, front) = coupling_points(
                                previous_ty,
                                previous_reversed,
                                ty,
                                reversed,
                            );
                            let (spawn_position, spawn_heading) = coupling_placement(
                                previous,
                                heading,
                                body_reversed(&previous_ty.def, previous_reversed),
                                back.y,
                                body_reversed(&ty.def, reversed),
                                front.y,
                            );
                            let t = &v.trailers[i];
                            assert!((spawn_position - expected).length() < 1e-4);
                            assert!((t.position - expected).length() < 1e-4);
                            assert!((t.body_heading() - spawn_heading).abs() < 1e-4);
                            previous = expected;
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn rail_coupling_distance_does_not_displace_cars_off_track() {
    for bogies in [None, Some(5.0), Some(-5.0)] {
        let mut lead = coupling_test_type(bogies);
        let mut car = coupling_test_type(bogies);
        Arc::get_mut(&mut lead)
            .unwrap()
            .def
            .coupling_back
            .as_mut()
            .unwrap()
            .pos = [0.6, -5.0, 0.8];
        Arc::get_mut(&mut car)
            .unwrap()
            .def
            .coupling_front
            .as_mut()
            .unwrap()
            .pos = [-0.2, 5.0, 3.6];
        let mut v = VehicleInstance::new(lead, VehicleHost::new(Default::default()));
        v.position.z = 2.0;
        v.attach_trailer_ex(car, false);
        for _ in 0..30 {
            v.update_trailers(0.0);
        }
        let offset = v.trailers[0].position - v.position;
        assert!((offset.y + 10.0).abs() < 1e-5);
        if bogies.is_some() {
            assert!(offset.x.abs() < 1e-5 && offset.z.abs() < 1e-5);
        } else {
            // Road sections still meet at their full three-dimensional joint.
            assert!((offset.x - 0.8).abs() < 1e-5);
            assert!((offset.z + 2.8).abs() < 1e-5);
        }
    }
}

fn synthetic_rail_consist() -> VehicleInstance {
    let ty = coupling_test_type(Some(-5.0));
    let mut v = VehicleInstance::new(ty.clone(), VehicleHost::new(Default::default()));
    for reversed in [false, true, true] {
        v.attach_trailer_ex(ty.clone(), reversed);
    }
    v
}

#[test]
fn negative_bogie_consist_has_outward_cabs_and_declared_spacing() {
    let mut v = synthetic_rail_consist();
    for heading in [0.0_f64, 180.0] {
        v.heading = heading;
        for t in &mut v.trailers {
            t.realign();
        }
        for _ in 0..30 {
            v.update_trailers(0.0);
        }
        let h = heading.to_radians();
        let forward = Vec3::new(h.sin() as f32, h.cos() as f32, 0.0);
        // Model the cab pointing towards the body's negative longitudinal end.
        let front_cab = v.body_rotation().transform_vector3(Vec3::NEG_Y);
        let rear_cab = v
            .trailers
            .last()
            .unwrap()
            .body_rotation()
            .transform_vector3(Vec3::NEG_Y);
        assert!(
            front_cab.dot(forward) > 0.99,
            "front cab faces inward: {front_cab:?}"
        );
        assert!(
            rear_cab.dot(forward) < -0.99,
            "rear cab faces inward: {rear_cab:?}"
        );
        let mut previous = v.position;
        for t in &v.trailers {
            let offset = previous - t.position;
            assert!(
                (offset.dot(forward.as_dvec3()) - 10.0).abs() < 1e-4,
                "car spacing {offset:?}"
            );
            assert!(
                offset.cross(forward.as_dvec3()).length() < 1e-4,
                "car off track: {offset:?}"
            );
            previous = t.position;
        }
    }
}

#[test]
fn rail_joints_stay_closed_on_curved_graded_track() {
    let mut v = synthetic_rail_consist();
    for direction in [1.0_f64, -1.0] {
        v.heading = if direction > 0.0 { 0.0 } else { 180.0 };
        v.pitch = 0.03_f32.atan().to_degrees();
        for t in &mut v.trailers {
            t.realign();
        }
        let track = |d: f64| {
            let a = d / 150.0;
            Some(DVec3::new(
                150.0 * (1.0 - a.cos()) * direction,
                -150.0 * a.sin() * direction,
                -d * 0.03,
            ))
        };
        for _ in 0..60 {
            v.retrail(1.0 / 30.0, &track);
            v.update_trailers(0.0);
            let (mut origin, mut rotation) = (v.position, v.body_rotation());
            for t in &v.trailers {
                let (back, front) = t.couplings();
                let lead_joint = origin + rotation.transform_point3(back).as_dvec3();
                let own_joint =
                    t.position + t.body_rotation().transform_point3(front).as_dvec3();
                // (each car stands on the track with its bogies, so on the bend two joints
                // meet within a few centimetres - here the lead, put beside the bend's
                // start rather than on it, sets the largest gap)
                assert!((lead_joint - own_joint).length() < 0.1, "{}", (lead_joint - own_joint).length());
                assert!(t.position.is_finite() && t.body_rotation().is_finite());
                let rail = t.track.expect("rail contact");
                assert!(rail.z < 0.0, "contact left the descending track");
                assert!((t.axle_z.unwrap() - rail.z).abs() < 1e-4);
                origin = t.position;
                rotation = t.body_rotation();
            }
        }
    }
}

#[test]
fn a_borrowed_part_is_judged_with_its_own_pack() {
    // the same machine from its own pack, whichever bus borrows it (#977)
    let a = winding_pack(Path::new("/omsi/Vehicles/Citelis/model/body.o3d"));
    let b = winding_pack(Path::new("/omsi/vehicles/Atron_AFR4/model/afr4.o3d"));
    let c = winding_pack(Path::new("/omsi/Vehicles/MAN_SD200/model/../../Atron_AFR4/model/afr4.o3d"));
    assert_eq!(a, "citelis");
    assert_eq!(b, "atron_afr4");
    assert_eq!(winding_pack(Path::new("/omsi/Vehicles/Atron_AFR4/model/sub/afr4.o3d")), b);
    assert_eq!(c, b);
    let elsewhere = winding_pack(Path::new("/omsi/Sceneryobjects/x/model/y.o3d"));
    assert_eq!(elsewhere, winding_pack(Path::new("/omsi/Sceneryobjects/x/model/z.o3d")));
    assert_ne!(elsewhere, winding_pack(Path::new("/omsi/Sceneryobjects/w/model/y.o3d")));
}

fn restored_display_vehicle() -> VehicleInstance {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "omsi_restore_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("display.osc");
    std::fs::write(
        &script,
        r#"
{frame}
(M.L.Matrix_frame)
(M.L.line_draw)
{end}
{macro:line_draw}
(L.$.line_request) (S.$.line_display)
{end}
{macro:Matrix_frame}
(L.L.power) (L.L.IBIS_Linie_Complex) (L.L.Matrix_Nr_Last) = ! &&
(L.L.power) (L.L.IBIS_TerminusIndex) (L.L.Matrix_TerminusIndex_Last) = ! && ||
{if}
0 (M.V.STNewTex)
0 (M.V.STLock)
0 255 255 120 0 (M.V.STSetColor)
0 0 0 4 2 (M.V.STDrawRect)
0 (M.V.STUnlock)
(L.L.IBIS_Linie_Complex) (S.L.Matrix_Nr_Last)
(L.L.IBIS_TerminusIndex) (S.L.Matrix_TerminusIndex_Last)
{endif}
{end}
"#,
    )
    .unwrap();
    let vars = dir.join("vars.txt");
    std::fs::write(&vars, "power\nIBIS_Linie_Complex\nIBIS_TerminusIndex\nMatrix_Nr_Last\nMatrix_TerminusIndex_Last\n").unwrap();
    let strings = dir.join("strings.txt");
    std::fs::write(&strings, "line_request\nline_display\n").unwrap();
    let program = omsi_script::compile(&omsi_script::CompileInput {
        scripts: vec![script],
        varlists: vec![vars],
        stringvarlists: vec![strings],
        ..Default::default()
    });
    assert!(program.errors.is_empty(), "{:?}", program.errors);
    let mut program = program;
    program.declare_str_var("destination");
    let mut model = Model::default();
    model.script_textures = vec![(4, 2)];
    model.text_textures.push(omsi_model::TextTexture {
        variable: "destination".into(),
        width: 4,
        height: 2,
        ..Default::default()
    });
    model.text_textures.push(omsi_model::TextTexture {
        variable: "line_display".into(),
        width: 900,
        height: 100,
        ..Default::default()
    });
    let ty = Arc::new(VehicleType {
        def: Default::default(),
        model,
        model_dir: dir.clone(),
        program: Arc::new(program),
        meshes: Vec::new(),
        lod_meshes: Vec::new(),
        paint_schemes: Vec::new(),
        texchanges: Vec::new(),
        wheel_meshes: Vec::new(),
        suspension_axles: Vec::new(),
        missing_packs: Vec::new(),
        mesh_bounds: Vec::new(),
        mesh_boxes: Vec::new(),
    });
    std::fs::remove_dir_all(dir).unwrap();
    VehicleInstance::new(ty, VehicleHost::new(Default::default()))
}

#[test]
fn restore_refreshes_unchanged_bitmap_without_retyping_or_powering_on() {
    for power in [0.0, 1.0] {
        let mut original = restored_display_vehicle();
        original.set_var("power", power);
        original.set_var("IBIS_Linie_Complex", 10900.0);
        original.set_var("IBIS_TerminusIndex", 9.0);
        original.update(0.02);
        let vars: Vec<_> = original
            .ty
            .program
            .var_names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), original.state.vars[i]))
            .collect();
        let mut resumed =
            VehicleInstance::new(original.ty.clone(), VehicleHost::new(Default::default()));
        resumed.restore_script_state(
            &vars,
            &[("destination".into(), "  Manual destination  ".into())],
        );
        assert_eq!(resumed.var("power"), Some(power));
        assert_eq!(resumed.var("IBIS_Linie_Complex"), Some(10900.0));
        assert_eq!(resumed.var("IBIS_TerminusIndex"), Some(9.0));
        assert_eq!(resumed.str_var("destination"), "  Manual destination  ");
        assert!(resumed.host.script_textures[0].rgba.iter().all(|p| *p == 0));
        resumed.update(0.02);
        assert_eq!(
            resumed.host.script_textures[0].rgba,
            original.host.script_textures[0].rgba
        );
        assert_eq!(
            resumed.host.script_textures[0].rgba.iter().any(|p| *p != 0),
            power > 0.0
        );
    }
}

#[test]
fn string_only_restore_preserves_and_reuploads_main_and_articulated_displays() {
    for power in [0.0, 1.0] {
        let mut v = restored_display_vehicle();
        v.set_var("power", power);
        v.attach_trailer(v.ty.clone());
        for def in &v.ty.model.text_textures {
            v.text_textures
                .push(crate::texttex::TextTextureState::new(def.clone(), None));
            v.trailers[0]
                .text_textures
                .push(crate::texttex::TextTextureState::new(def.clone(), None));
        }
        let strings = [
            ("line_request".into(), "X9                            ".into()),
            ("line_display".into(), "X9                            ".into()),
            ("destination".into(), "  Manual destination  ".into()),
        ];
        // Repeat with already uploaded, unchanged text: a newly bound texture still
        // needs its image, even when no numeric variables were saved.
        for _ in 0..2 {
            v.restore_script_state(&[], &strings);
            v.update(0.02);
            assert_eq!(v.var("power"), Some(power));
            assert_eq!(v.str_var("destination"), "  Manual destination  ");
            assert_eq!(v.str_var("line_request"), "X9                            ");
            assert_eq!(v.str_var("line_display"), "X9                            ");
            assert_eq!(v.update_text_textures(), vec![0, 1]);
            let mut part = v.trailers.pop().unwrap();
            assert_eq!(part.update_text_textures(&v), vec![0, 1]);
            assert_eq!(
                part.text_textures[1].last_text.as_deref(),
                Some("X9                            ")
            );
            v.trailers.push(part);
            for t in &mut v.text_textures {
                t.pending.take();
            }
            assert!(v.update_text_textures().is_empty());
        }
    }
}

#[test]
fn remote_strings_are_visible_to_scripts_and_restored_before_visuals() {
    let mut v = restored_display_vehicle();
    for def in &v.ty.model.text_textures {
        v.text_textures
            .push(crate::texttex::TextTextureState::new(def.clone(), None));
    }
    let request = v.ty.program.str_var("line_request").unwrap();
    let display = v.ty.program.str_var("line_display").unwrap();
    let power = v.ty.program.var("power").unwrap();
    // The frame copies request to display: it must see the owner request immediately.
    v.update_ai_with_strings(
        0.02,
        &AiFrame::default(),
        &[],
        &[(power, 0.0)],
        &[(request, "owner request")],
    );
    assert_eq!(v.str_var("line_display"), "owner request");
    // That script result must not overwrite an independently synchronized display.
    v.update_ai_with_strings(
        0.02,
        &AiFrame::default(),
        &[],
        &[(power, 0.0)],
        &[(request, "other request"), (display, "owner display")],
    );
    assert_eq!(v.str_var("line_display"), "owner display");
    assert_eq!(v.var("power"), Some(0.0));
    v.update_text_textures();
    assert!(
        v.text_textures
            .iter()
            .any(|t| t.last_text.as_deref() == Some("owner display"))
    );
}

/// A railcar running alone (no coupled parts) is put on its track without a panic (#2259).
#[test]
fn a_single_railcar_retrails_without_trailers() {
    let mut v = synthetic_rail_consist();
    v.trailers.clear();
    v.retrail(1.0 / 30.0, &|d| Some(DVec3::new(0.0, -d, 0.0)));
    assert!(v.trailers.is_empty());
}

/// A paint scheme's folder gives only what its `.cti` names, each by its full name; a
/// picture the folder has not got keeps its bare name and is found where the model's own
/// textures are. The folder is no search path for the rest: with it first in line, a
/// same-named file left there for another scheme (the IK-103's yellow sedista-diffuse.bmp
/// beside the grey bars schemes) painted the details of every scheme of the folder.
#[test]
fn a_scheme_folder_supplies_only_what_its_cti_names() {
    let dir = std::env::temp_dir().join(format!("omsi_vehicle_scheme_folder_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("yellow.bmp"), b"BM").unwrap();
    let ty = VehicleType {
        def: Default::default(),
        model: omsi_model::Model { ctc_textures: vec![("Body".into(), "body.bmp".into()), ("Seats".into(), "sedista.bmp".into())], ..Default::default() },
        model_dir: dir.clone(),
        program: Arc::new(omsi_script::compile(&omsi_script::CompileInput::default())),
        meshes: Vec::new(),
        lod_meshes: Vec::new(),
        paint_schemes: vec![PaintScheme {
            name: "Yellow".into(),
            dir: dir.clone(),
            textures: vec![("Body".into(), "yellow.bmp".into()), ("Seats".into(), "sedista-diffuse.bmp".into())],
            set_vars: Vec::new(),
        }],
        texchanges: Vec::new(),
        wheel_meshes: Vec::new(),
        suspension_axles: Vec::new(),
        missing_packs: Vec::new(),
        mesh_bounds: Vec::new(),
        mesh_boxes: Vec::new(),
    };
    let (map, folder) = ty.scheme_substitutions(0, omsi_texture::find_texture);
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(folder.as_deref(), Some(dir.as_path()));
    let body = std::path::Path::new(&map["body.bmp"]);
    assert_eq!(body.file_name().and_then(|n| n.to_str()), Some("yellow.bmp"), "a picture the folder has: its full name");
    assert!(body.is_absolute(), "{}", map["body.bmp"]);
    assert_eq!(map["sedista.bmp"], "sedista-diffuse.bmp", "a picture the folder has not got: the bare name, found with the model's textures");
}

/// A trolleybus on a wire: no current, no movement; with its poles raised, the drive moves it.
#[test]
fn a_trolleybus_moves_only_on_current() {
    use omsi_trolley::{DriveKind, PoleDef, RheostatDef, TrolleyDef, WireNet};
    let net = Arc::new(WireNet::parse(
        "[trolley_section]\nid 1\nlive 1\n[trolley_span]\nsection 1\np -400 0 5.5\np 400 0 5.5\n",
    ));
    let mut ty = coupling_test_type(None);
    {
        let t = Arc::get_mut(&mut ty).unwrap();
        t.def.axles[1].driven = true;
        t.def.mass = 11.0;
        let pole = |side: i8| PoleDef { side, base: Vec3::new(0.3 * side as f32, -0.5, 3.0), length: 6.5, ..Default::default() };
        t.def.trolley = TrolleyDef { poles: vec![pole(-1), pole(1)], drive: Some(DriveKind::Rheostat(RheostatDef::default())), ..Default::default() };
    }
    // heading east, on the wire's line
    let drive = |net: Option<Arc<WireNet>>, raise: bool| {
        let mut v = VehicleInstance::new(ty.clone(), VehicleHost::new(Default::default()));
        v.heading = 90.0;
        v.position = DVec3::new(0.0, 0.0, 0.0);
        v.attach_wires(net);
        v.set_controls(Controls { throttle: 1.0, ..Default::default() });
        if raise {
            v.trolley.as_deref_mut().unwrap().auto_raise_for = 5.0;
        }
        for _ in 0..(60 * 14) {
            v.update(1.0 / 60.0);
        }
        v
    };
    let bare = drive(None, true);
    assert!(bare.physics.speed.abs() < 0.01, "no wire: {}", bare.physics.speed);
    let down = drive(Some(net.clone()), false);
    assert!(down.physics.speed.abs() < 0.01, "poles down: {}", down.physics.speed);
    let up = drive(Some(net), true);
    assert!(up.physics.speed > 3.0, "poles up: {} m/s", up.physics.speed);
    assert!(up.trolley.as_ref().unwrap().rig.circuit);
    assert!(up.var("Trolley_Powered").is_some());
}
