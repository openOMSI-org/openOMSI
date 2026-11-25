//! Tests of articulated buses: the joint's angles, the rear section and the bellows.

use super::*;

/// The articulated GN92 turning right: the angle goes to `articulation_0_alpha` (the
/// jackknife protection's), and the joint's last dummy - the bone the bellows' rear end
/// hangs on - turns with it onto the rear section's line.
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn articulation_alpha_turns_the_joint_onto_the_rear_section() {
    let root = original_root();
    let bus = root.join("Vehicles/MAN_NL_NG/MAN_GN92_main.bus");
    let trail = root.join("Vehicles/MAN_NL_NG/MAN_GN92_trail.bus");
    require_content(&[&bus, &trail]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("GN92"));
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    v.attach_trailer_ex(
        Arc::new(VehicleType::load(&root, &trail).expect("GN92 trail")),
        false,
    );
    v.heading = 10.0;
    v.update_visuals(0.02);
    assert!(v.var("articulation_0_alpha").unwrap().abs() < 1e-3);
    // the front section turns right (clockwise) by 20 degrees about its rear coupling's
    // neighbourhood: the rear section still points the old way
    v.heading = 30.0;
    v.update_visuals(0.02);
    let t = &v.trailers[0];
    let alpha = v.var("articulation_0_alpha").unwrap();
    let beta = v.var("articulation_0_beta").unwrap();
    let geometric = ((v.heading - t.heading + 540.0) % 360.0) - 180.0;
    assert!(geometric > 5.0, "{geometric}");
    assert!(
        (alpha.abs() - geometric as f32).abs() < 1e-3,
        "alpha {alpha} for {geometric}"
    );
    assert!(beta.abs() < 1.0, "beta {beta}");
    // where the rear dummy points, in the front section's frame, against where the rear
    // section lies: (−sin a, cos a) forwards for the angle a
    let d =
        v.ty.meshes
            .iter()
            .position(|m| {
                v.ty.model.meshes[m.def_index]
                    .file
                    .to_ascii_lowercase()
                    .ends_with("gelenk_d.o3d")
            })
            .expect("GN_Dum_Gelenk_D");
    let back = v.mesh_transforms[d].transform_vector3(-Vec3::Y);
    let a = (geometric as f32).to_radians();
    let want = Vec3::new(a.sin(), -a.cos(), 0.0);
    assert!(
        (back - want).length() < 0.02,
        "dummy D points {back:?}, the rear section {want:?} (alpha {alpha})"
    );
    // the bellows ([smoothskin] on the dummies) bend with it: the rear ring swings
    // towards the rear section's side, the front ring stays
    let skinned = v.skinned_meshes();
    assert!(!skinned.is_empty(), "no skinned mesh on the GN92");
    for i in skinned {
        let rest = v.ty.meshes[i].data.positions.clone();
        let (pos, nrm) = v.skinned(i).expect("skinned vertices");
        assert_eq!((pos.len(), nrm.len()), (rest.len(), rest.len()));
        let mean_dx = |f: &dyn Fn(f32) -> bool| {
            let d: Vec<f32> = pos
                .iter()
                .zip(&rest)
                .filter(|(_, r)| f(r.y))
                .map(|(p, r)| p.x - r.x)
                .collect();
            d.iter().sum::<f32>() / d.len().max(1) as f32
        };
        let (front, rear) = (mean_dx(&|y| y > -3.8), mean_dx(&|y| y < -4.5));
        assert!(
            front.abs() < 0.05,
            "{}: front ring moved {front}",
            v.ty.model.meshes[v.ty.meshes[i].def_index].file
        );
        assert!(
            rear > 0.1,
            "{}: rear ring moved {rear}",
            v.ty.model.meshes[v.ty.meshes[i].def_index].file
        );
    }
    // straight: the modelled shape
    let mut s = VehicleInstance::new(v.ty.clone(), VehicleHost::new(crate::SimClock::default()));
    s.attach_trailer_ex(v.trailers[0].ty.clone(), false);
    s.update_visuals(0.02);
    let i = s.skinned_meshes()[0];
    let rest = s.ty.meshes[i].data.positions.clone();
    let (pos, _) = s.skinned(i).unwrap();
    let worst = pos
        .iter()
        .zip(&rest)
        .map(|(p, r)| (*p - *r).length())
        .fold(0.0f32, f32::max);
    assert!(worst < 1e-3, "straight bellows off by {worst}");
}

#[test]
fn restored_odometer_survives_engine_updates_and_continues_driving() {
    for driven in [0.0, 2.75] {
        let mut v = VehicleInstance::new(
            coupling_test_type(None),
            VehicleHost::new(Default::default()),
        );
        v.driven_km = driven;
        v.set_odometer_km(75556.639);
        assert_eq!(v.odometer_km(), 75556.639);
        v.update_engine_vars(0.02);
        assert_eq!(v.odometer_km(), 75556.639);
        assert_eq!(v.var("kmcounter_km"), Some(75556.0));
        assert!((v.var("kmcounter_m").unwrap() - 639.0).abs() < 0.001);
        v.driven_km += 0.5;
        v.update_engine_vars(0.02);
        assert!((v.odometer_km() - 75557.139).abs() < 1e-8);
        assert_eq!(v.var("kmcounter_km"), Some(75557.0));
        assert!((v.var("kmcounter_m").unwrap() - 139.0).abs() < 0.001);
    }
    // A restored zero is deliberate, rather than the sentinel for a random start.
    let mut v = VehicleInstance::new(
        coupling_test_type(None),
        VehicleHost::new(Default::default()),
    );
    v.set_odometer_km(0.0);
    v.update_engine_vars(0.02);
    assert_eq!(v.odometer_km(), 0.0);
}

/// A bus without `[kmcounter_init]` starts with Omsi.exe's defaults (in service since
/// 1980, 60000 km a year, +-20 %), not at 0 km; reversing takes the counter back.
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn odometer_starts_at_the_default_service_life() {
    let root = original_root();
    let bus = root.join("Vehicles/MAN_SD200/MAN_SD77.bus");
    require_content(&[&bus]);
    let mut ty = VehicleType::load(&root, &bus).expect("SD77");
    ty.def.km_counter_init = None;
    let mut v = VehicleInstance::new(Arc::new(ty), VehicleHost::new(crate::SimClock::default()));
    v.host.clock.year = 2011;
    v.host.clock.day_of_year = 0;
    v.update_engine_vars(0.02);
    let base = v.host.km_base;
    assert!(
        (31.0 * 60000.0 * 0.8 + 8.0..=31.0 * 60000.0 * 1.2 + 18.0).contains(&base),
        "{base}"
    );
    v.driven_km = -0.5;
    v.update_engine_vars(0.02);
    let km = v.var("kmcounter_km").unwrap() as f64 + v.var("kmcounter_m").unwrap() as f64 / 1000.0;
    assert!((km - (base - 0.5)).abs() < 1.0, "{km} for {base}");
}

/// The GN92's joint stops at `[coupling_front_character]`'s 52.5 degrees: the front
/// section swinging round 90 degrees drags the rear section's axle with it.
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn articulation_stops_at_the_coupling_max_alpha() {
    let root = original_root();
    let bus = root.join("Vehicles/MAN_NL_NG/MAN_GN92_main.bus");
    let trail = root.join("Vehicles/MAN_NL_NG/MAN_GN92_trail.bus");
    require_content(&[&bus, &trail]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("GN92"));
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    v.attach_trailer_ex(
        Arc::new(VehicleType::load(&root, &trail).expect("GN92 trail")),
        false,
    );
    v.heading = 10.0;
    v.update_visuals(0.02);
    for h in [100.0, -80.0] {
        v.heading = h;
        v.update_visuals(0.02);
        let alpha = v.var("articulation_0_alpha").unwrap();
        assert!(
            (alpha.abs() - 52.5).abs() < 1e-3,
            "alpha {alpha} at heading {h}"
        );
    }
}

/// The stock rattle (`klappern.osc`) as loud at 144 frames a second as at OMSI's 30.
#[test]
fn a_jolt_rattles_alike_at_any_frame_rate() {
    fn rattle(fps: f32) -> f32 {
        let dt = 1.0 / fps;
        let (mut frames, mut last, mut vol, mut peak) =
            (super::OmsiFrames::default(), 0.0f32, 0.0f32, 0.0f32);
        for i in 0..(fps as usize) {
            let t = i as f32 * dt;
            // a 6 Hz pitching after a bump, 0.5 m/s² along the bus
            let a = Vec3::new(
                0.0,
                0.5 * (t * 6.0 * std::f32::consts::TAU).sin() * (-t * 3.0).exp(),
                0.0,
            );
            let a = frames.push(a, dt);
            let m = (a.x * a.x + a.y * a.y + 0.01 * a.z * a.z).sqrt();
            vol = ((m - last) * 1.0).max(vol * (-dt).exp()).min(1.0);
            last = m;
            peak = peak.max(vol);
        }
        peak
    }
    let (omsi, fast) = (rattle(30.0), rattle(144.0));
    // (taken frame by frame, 144 a second rattled at 0.3 of OMSI's)
    assert!(omsi > 0.2 && omsi < 0.9, "{omsi}");
    assert!(
        fast > 0.6 * omsi && fast < 1.4 * omsi,
        "30 fps {omsi}, 144 fps {fast}"
    );
}

/// `A_Trans_*` are the body's acceleration without gravity, as in Omsi.exe: 0 for a bus
/// standing still, on the level or on a grade, and the braking's deceleration alone.
#[test]
fn scripts_acceleration_leaves_gravity_out() {
    let level = super::scripts_acceleration(Vec3::new(0.0, 0.0, 9.81), Quat::IDENTITY);
    assert!(level.length() < 1e-4, "{level}");
    // standing nose up on a 10 % grade: the accelerometer reads gravity's share along it
    let rot = Quat::from_rotation_x(0.1f32.atan());
    let reading = rot.inverse().mul_vec3(Vec3::new(0.0, 0.0, 9.81));
    let grade = super::scripts_acceleration(reading, rot);
    assert!(grade.length() < 1e-4, "{grade}");
    // braking at 3 m/s² on the level
    let braking = super::scripts_acceleration(Vec3::new(0.0, -3.0, 9.81), Quat::IDENTITY);
    assert!(
        (braking - Vec3::new(0.0, -3.0, 0.0)).length() < 1e-4,
        "{braking}"
    );
}

/// A rear section turns about its own `[rot_pnt_long]` line: the stock GN92's is its
/// axle; one set ahead of the axle (a steered rear axle, #322) is where it turns.
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn rear_section_turns_about_its_rot_pnt_long() {
    let root = original_root();
    let bus = root.join("Vehicles/MAN_NL_NG/MAN_GN92_main.bus");
    let trail = root.join("Vehicles/MAN_NL_NG/MAN_GN92_trail.bus");
    require_content(&[&bus, &trail]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("GN92"));
    let mut tt = VehicleType::load(&root, &trail).expect("GN92 trail");
    let stock = TrailerPart::new(
        Arc::new(VehicleType::load(&root, &trail).unwrap()),
        &ty,
        &ty.program,
        2,
    );
    assert!(
        (stock.pivot_length() - (4.169 + 0.387)).abs() < 1e-3,
        "{}",
        stock.pivot_length()
    );
    tt.def.rot_pnt_long = 1.0;
    let steered = TrailerPart::new(Arc::new(tt), &ty, &ty.program, 2);
    assert!(
        (steered.pivot_length() - (4.169 - 1.0)).abs() < 1e-3,
        "{}",
        steered.pivot_length()
    );
}

/// A rear section's axle behind its `[rot_pnt_long]` line turns against the bend, as far
/// as points it at the bend's centre on that line (Omsi.exe 0x7e303d); the stock GN92's,
/// on its line, stays straight (#322).
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn rear_section_axle_behind_its_turning_line_steers() {
    let root = original_root();
    let bus = root.join("Vehicles/MAN_NL_NG/MAN_GN92_main.bus");
    let trail = root.join("Vehicles/MAN_NL_NG/MAN_GN92_trail.bus");
    require_content(&[&bus, &trail]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("GN92"));
    // the front section driving a right-hand bend: a degree every 0.2 m
    let steer = |line: Option<f32>| -> (f32, f32) {
        let mut tt = VehicleType::load(&root, &trail).expect("GN92 trail");
        if let Some(l) = line {
            tt.def.rot_pnt_long = l;
        }
        let mut v = VehicleInstance::new(ty.clone(), VehicleHost::new(crate::SimClock::default()));
        v.attach_trailer_ex(Arc::new(tt), false);
        for _ in 0..200 {
            let h = v.heading.to_radians();
            v.position += DVec3::new(h.sin(), h.cos(), 0.0) * 0.2;
            v.heading += 1.0;
            v.update_visuals(0.02);
        }
        (v.var("Axle_Steering_2_L").unwrap(), v.trailers[0].kappa)
    };
    let (stock, kappa) = steer(None);
    assert!(stock.abs() < 1e-4, "the GN92's axle steered {stock}");
    assert!(kappa > 0.01, "a right-hand bend is {kappa}");
    let (steered, kappa) = steer(Some(1.0));
    let want = (-(1.0 + 0.387) * kappa).atan();
    assert!(
        steered < -0.05 && (steered - want).abs() < 1e-4,
        "{steered} for {want}"
    );
}

/// A rear section whose turning line lies ahead of its axle, coupled behind a front section
/// driving a steady right-hand bend: its curvature, worked out of the front's and the joint
/// angle as Omsi.exe 0x7e0f44 does, comes to the bend its turning line really runs on, and
/// its axle turns against the bend; put somewhere else, its wheels stand straight (#322).
#[test]
fn a_rear_sections_bend_follows_from_the_joint() {
    let front = coupling_test_type(None);
    let mut rear = (*coupling_test_type(None)).clone();
    rear.def.axles = vec![omsi_vehicle::Axle {
        long: -3.0,
        ..Default::default()
    }];
    rear.def.rot_pnt_long = -1.0;
    let mut v = VehicleInstance::new(front, VehicleHost::new(Default::default()));
    v.attach_trailer(Arc::new(rear));
    // the front turns about its origin's line (axles at -3 and 3): a bend of 15 m for its
    // origin, the wheel angle of a 3 m wheelbase
    let r = 15.0f64;
    v.physics.steer_deg = (3.0 / r).atan().to_degrees() as f32;
    let at = |h: f64| DVec3::new(r - r * h.cos(), r * h.sin(), 0.0);
    for i in 0..3000 {
        let h = i as f64 * 0.1 / r;
        v.position = at(h);
        v.heading = h.to_degrees();
        v.update_trailers(1.0 / 60.0);
    }
    let t = &v.trailers[0];
    // the joint 5 m behind the front's line, 6 m ahead of the rear's
    let want = 1.0 / (r * r + 25.0 - 36.0).sqrt() as f32;
    assert!(
        (t.kappa - want).abs() < 2e-3,
        "curvature {} for {want}",
        t.kappa
    );
    let steer = t.axle_steer(0).unwrap();
    assert!(
        (steer - (-2.0 * t.kappa).atan()).abs() < 1e-6 && steer < -0.1,
        "{steer}"
    );
    // straight on: straight wheels
    assert_eq!(
        super::super::trailer::coupled_curvature(0.0, -5.0, 0.0, 6.0),
        0.0
    );
    // a joint on the turning line turns nothing
    assert_eq!(
        super::super::trailer::coupled_curvature(0.1, -5.0, 0.3, 0.0),
        0.0
    );
    let t = &mut v.trailers[0];
    t.realign();
    assert_eq!(t.axle_steer(0), Some(0.0));
}

/// The rear section of an articulated bus on a viaduct stays on the deck: one frame with
/// no deck under its axle (a gap at a joint) does not drop it onto the road below, and
/// one that had sunk under the deck finds it again (#135).
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn rear_section_stays_on_a_viaduct_deck() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let root = original_root();
    let bus = root.join("Vehicles/MAN_NL_NG/MAN_GN92_main.bus");
    let trail = root.join("Vehicles/MAN_NL_NG/MAN_GN92_trail.bus");
    require_content(&[&bus, &trail]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("GN92"));
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    v.attach_trailer_ex(
        Arc::new(VehicleType::load(&root, &trail).expect("GN92 trail")),
        false,
    );
    // 0: a deck at 10 m over a road at 0; 1: a gap in the deck
    let mode = Arc::new(AtomicU8::new(0));
    let m = mode.clone();
    let ground = move |_x: f64, _y: f64, top: f64| {
        let deck = m.load(Ordering::Relaxed) == 0;
        if deck && top >= 10.0 {
            crate::rigid::GroundProbe {
                below: Some(10.0),
                above: None,
            }
        } else if deck {
            crate::rigid::GroundProbe {
                below: Some(0.0),
                above: Some(10.0),
            }
        } else {
            crate::rigid::GroundProbe {
                below: Some(0.0),
                above: None,
            }
        }
    };
    v.contact = Some(Arc::new(ground));
    v.position = DVec3::new(0.0, 0.0, 10.0);
    for _ in 0..50 {
        v.update_visuals(0.02);
    }
    let on_deck = v.trailers[0].position.z;
    assert!((on_deck - 10.0).abs() < 0.5, "rear section at {on_deck}");
    mode.store(1, Ordering::Relaxed);
    v.update_visuals(0.02);
    assert!(
        v.trailers[0].position.z > 9.0,
        "dropped through the gap to {}",
        v.trailers[0].position.z
    );
    // sunk under the deck: it comes back up
    mode.store(0, Ordering::Relaxed);
    v.trailers[0].position.z = 0.2;
    v.trailers[0].axle_z = Some(0.0);
    for _ in 0..5 {
        v.update_visuals(0.02);
    }
    assert!(
        v.trailers[0].position.z > 9.0,
        "stayed under the deck at {}",
        v.trailers[0].position.z
    );
}

/// #901: the rear section's wheels take the road under them - a kerb-high step under
/// its left wheel pushes that wheel up into its arch and leaves the right one.
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn rear_section_wheels_spring_on_their_own() {
    let root = original_root();
    let bus = root.join("Vehicles/MAN_NL_NG/MAN_GN92_main.bus");
    let trail = root.join("Vehicles/MAN_NL_NG/MAN_GN92_trail.bus");
    require_content(&[&bus, &trail]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("GN92"));
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    v.attach_trailer_ex(
        Arc::new(VehicleType::load(&root, &trail).expect("GN92 trail")),
        false,
    );
    let t = &v.trailers[0];
    let axle = t.first_axle;
    assert!(
        t.ty.suspension_axles.contains(&axle),
        "the GN92's rear axle is drawn sprung"
    );
    // a step 6 cm high under the left wheels behind the joint
    let ground = move |x: f64, y: f64, _top: f64| crate::rigid::GroundProbe {
        below: Some(if x < 0.0 && y < -4.0 { 0.06 } else { 0.0 }),
        above: None,
    };
    v.contact = Some(Arc::new(ground));
    v.position = DVec3::new(0.0, 0.0, 0.0);
    for _ in 0..50 {
        v.update_visuals(0.02);
    }
    let l = -v.var(&format!("Axle_Suspension_{axle}_L")).unwrap();
    let r = -v.var(&format!("Axle_Suspension_{axle}_R")).unwrap();
    assert!(l - r > 0.04, "left wheel up {l:.3}, right {r:.3}");
}

/// A timetable duty and a random traffic car load their bus with `VehicleType::load_ai`,
/// which lets the vertices of every mesh go to save memory - except a `[smoothskin]`
/// mesh (the bellows) has to keep its own, or there is nothing left to bend it from and
/// every AI articulated bus (and the rear-section trailer of a `.zug` train) drove with
/// its bellows frozen in the rest pose through every corner.
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn ai_loaded_bellows_keep_their_skin_and_still_bend() {
    let root = original_root();
    let bus = root.join("Vehicles/MAN_NL_NG/MAN_GN92_main.bus");
    let trail = root.join("Vehicles/MAN_NL_NG/MAN_GN92_trail.bus");
    require_content(&[&bus, &trail]);
    let ai = VehicleType::load_ai(&root, &bus).expect("GN92 (AI)");
    let skinned: Vec<usize> = (0..ai.meshes.len())
        .filter(|&i| !ai.meshes[i].skin.is_empty())
        .collect();
    assert!(
        !skinned.is_empty(),
        "the GN92 has no [smoothskin] mesh to check"
    );
    for &i in &skinned {
        assert!(
            !ai.meshes[i].data.positions.is_empty(),
            "{}: an AI copy dropped the bellows' vertices",
            ai.model.meshes[ai.meshes[i].def_index].file
        );
        assert!(
            !ai.mesh_dropped(i),
            "{}: an AI copy still marks the bellows as needing a re-read",
            ai.model.meshes[ai.meshes[i].def_index].file
        );
    }
    // every other mesh still lets its vertices go, exactly as before this fix
    assert!(
        (0..ai.meshes.len()).any(|i| ai.mesh_dropped(i)),
        "the AI type keeps meshes it should have let go"
    );

    let ty = Arc::new(ai);
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    v.attach_trailer_ex(
        Arc::new(VehicleType::load_ai(&root, &trail).expect("GN92 trail (AI)")),
        false,
    );
    v.heading = 10.0;
    v.update_visuals(0.02);
    v.heading = 30.0;
    v.update_visuals(0.02);
    let i = skinned[0];
    let rest = v.ty.meshes[i].data.positions.clone();
    let (pos, nrm) = v.skinned(i).expect("skinned vertices of an AI copy");
    assert_eq!((pos.len(), nrm.len()), (rest.len(), rest.len()));
    let moved = pos
        .iter()
        .zip(&rest)
        .map(|(p, r)| (*p - *r).length())
        .fold(0.0f32, f32::max);
    assert!(
        moved > 0.1,
        "the AI copy's bellows did not bend with the joint (moved {moved})"
    );
}

/// The same check on the O530G Facelift / Citaro LE mod pack (`OMSI_CONTENT`, or the
/// user's overlay by default): a third-party model with its own bone names
/// (`Armature_Gelenk_A-D` on `18m_main\bellows_out.o3d` / `bellows_in.o3d`, against the
/// stock GN92's `Gelenk_A-D` on its own files) still bends, both loaded normally and
/// loaded as an AI copy - the fix is generic over `[setbone]` names, not tied to MAN's.
#[test]
#[ignore = "needs OMSI_CONTENT with the MB_O530_Facelift mod"]
fn o530g_mod_bellows_bend_and_survive_an_ai_load() {
    let content = omsi_cfg::flags::OMSI_CONTENT
        .os()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from("/Users/savva/OMSI 2 Source Code/openOMSI/target/release")
        });
    let bus = content.join("Vehicles/MB_O530_Facelift/MB_O530GFL EL 3D Main.bus");
    let trail = content.join("Vehicles/MB_O530_Facelift/MB_O530GFL EL 3D Trail.bus");
    require_content(&[&bus, &trail]);
    // loaded fully (the player's own bus): bends like the GN92
    let ty = Arc::new(VehicleType::load(&content, &bus).expect("O530G"));
    let skinned = ty
        .meshes
        .iter()
        .enumerate()
        .filter(|(_, m)| !m.skin.is_empty())
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert!(
        !skinned.is_empty(),
        "the O530G has no [smoothskin] mesh to check (bellows_out/bellows_in)"
    );
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    v.attach_trailer_ex(
        Arc::new(VehicleType::load(&content, &trail).expect("O530G trail")),
        false,
    );
    v.heading = 10.0;
    v.update_visuals(0.02);
    v.heading = 30.0;
    v.update_visuals(0.02);
    for &i in &skinned {
        let rest = v.ty.meshes[i].data.positions.clone();
        let (pos, nrm) = v.skinned(i).expect("skinned vertices");
        assert_eq!((pos.len(), nrm.len()), (rest.len(), rest.len()));
        let moved = pos
            .iter()
            .zip(&rest)
            .map(|(p, r)| (*p - *r).length())
            .fold(0.0f32, f32::max);
        assert!(
            moved > 0.1,
            "{}: did not bend with the joint (moved {moved})",
            v.ty.model.meshes[v.ty.meshes[i].def_index].file
        );
    }
    // loaded as an AI copy (the timetable's or a random traffic bus): same bones kept
    let ai = VehicleType::load_ai(&content, &bus).expect("O530G (AI)");
    for &i in &skinned {
        assert!(
            !ai.meshes[i].data.positions.is_empty(),
            "an AI copy dropped the O530G's bellows vertices"
        );
    }
}

/// `OMSI_DEBUG_WHEELS` reports the wheels of an AI type, whose vertices were let go after
/// loading, exactly as it reports those of a fully loaded one.
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn wheel_pivot_report_reads_dropped_meshes() {
    let root = original_root();
    let bus = root.join("Vehicles/MAN_F90/AI_MAN_F90_Wechselbruecke.bus");
    require_content(&[&bus]);
    let report = |ty: VehicleType| {
        VehicleInstance::new(Arc::new(ty), VehicleHost::new(crate::SimClock::default()))
            .wheel_pivot_report()
    };
    let ai = VehicleType::load_ai(&root, &bus).expect("F90");
    assert!(
        (0..ai.meshes.len()).any(|i| ai.mesh_dropped(i)),
        "the AI type keeps its vertices"
    );
    let full = report(VehicleType::load(&root, &bus).expect("F90"));
    assert!(!full.is_empty(), "the F90 has no animated wheels");
    assert_eq!(report(ai), full);
}

/// A `[smoothskin]` vertex hung partly on an o3d bone no `[setbone]` names keeps that
/// share where it was modelled: the AA-FR Agora's retarder lever (`retarderhebel_2.o3d`)
/// splits its vertices between `Bone` (unbound) and `Bone.001` (the animated dummy), and
/// with the unbound share dropped the lever bent out of shape as it moved.
#[test]
#[ignore = "needs OMSI_ROOT with AA-FR_BusBundle (not part of the stock install)"]
fn unbound_bones_keep_their_share_of_a_vertex() {
    use super::*;
    use std::sync::Arc;
    let root = original_root();
    let bus = root.join("Vehicles/AA-FR_BusBundle/2002_Agora_S_2d.bus");
    require_content(&[&bus]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("Agora S"));
    let i = (0..ty.meshes.len())
        .find(|&i| {
            ty.model.meshes[ty.meshes[i].def_index]
                .file
                .to_ascii_lowercase()
                .contains("retarderhebel_2")
        })
        .expect("the Agora's retarder lever");
    let vm = &ty.meshes[i];
    let bound = vm
        .skin
        .iter()
        .find(|b| b.def_index.is_some())
        .expect("Bone.001");
    assert!(
        vm.skin.iter().any(|b| b.def_index.is_none()),
        "the unbound Bone is kept"
    );
    let k = ty
        .meshes
        .iter()
        .position(|m| Some(m.def_index) == bound.def_index)
        .unwrap();
    let mut v = VehicleInstance::new(ty.clone(), VehicleHost::new(crate::SimClock::default()));
    v.update_visuals(0.02);
    let rest = v.mesh_transforms.clone();
    assert!(v.set_var("cp_retarder_hebel", 4.0));
    for _ in 0..200 {
        v.update_visuals(0.05);
    }
    let bone = v.mesh_transforms[k] * rest[k].inverse();
    assert!(
        bone.abs_diff_eq(Mat4::IDENTITY, 1e-3) == false,
        "the lever's dummy did not move"
    );
    let (pos, _) = v.skinned(i).expect("skinned lever");
    let mut checked = 0;
    for &(vi, w) in &bound.weights {
        let vi = vi as usize;
        if !(0.2..0.8).contains(&w) {
            continue;
        }
        let p = vm.data.positions[vi];
        // (1 - w) where it was modelled, w with the bone, in the mesh's own frame
        let want = rest[i].inverse() * (Mat4::IDENTITY * (1.0 - w) + bone * w);
        let want = want.transform_point3(p);
        assert!(
            (pos[vi] - want).length() < 1e-4,
            "vertex {vi} (weight {w}): {:?} for {:?}",
            pos[vi],
            want
        );
        checked += 1;
    }
    assert!(checked > 0, "no vertex shared between the two bones");
}

/// The joint's vertical angle turns the Agora L's arch (`anim_rot articulation_0_beta
/// 0.5`) and the bone its bellows' far ring hangs on half-way towards the rear section,
/// whichever way the front section pitches.
#[test]
#[ignore = "needs OMSI_ROOT with AA-FR_BusBundle (not part of the stock install)"]
fn articulation_beta_tilts_the_joint_towards_the_rear_section() {
    use super::*;
    use std::sync::Arc;
    let root = original_root();
    let bus = root.join("Vehicles/AA-FR_BusBundle/2002_Agora_L_3d_main.bus");
    let trail = root.join("Vehicles/AA-FR_BusBundle/2002_Agora_L_3d_trail.bus");
    require_content(&[&bus, &trail]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("Agora L"));
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    v.attach_trailer_ex(
        Arc::new(VehicleType::load(&root, &trail).expect("Agora L trail")),
        false,
    );
    v.update_visuals(0.02);
    let find = |v: &VehicleInstance, n: &str| {
        v.ty.meshes
            .iter()
            .position(|m| {
                v.ty.model.meshes[m.def_index]
                    .file
                    .to_ascii_lowercase()
                    .ends_with(n)
            })
            .expect(n)
    };
    let (arch, bone_b) = (find(&v, "gelenk_arch.o3d"), find(&v, "bone_b.o3d"));
    for pitch in [4.0f32, -4.0] {
        v.pitch = pitch;
        for _ in 0..50 {
            v.update_visuals(0.05);
        }
        // where the rear section lies, in the front section's frame
        let rel = v.body_rotation().inverse() * v.trailers[0].body_rotation();
        let back = rel.transform_vector3(-Vec3::Y);
        assert!(
            back.z.abs() > 0.03,
            "the sections are not pitched apart ({back:?})"
        );
        for k in [arch, bone_b] {
            let d = v.mesh_transforms[k].transform_vector3(-Vec3::Y);
            assert!(
                d.z * back.z > 0.0 && d.z.abs() < back.z.abs(),
                "pitch {pitch}: {} points {d:?}, the rear section {back:?}",
                v.ty.model.meshes[v.ty.meshes[k].def_index].file
            );
        }
    }
}

/// The ids the vehicle and its rear section keep for the variables they use every frame
/// are those the names give: the program's own (driver's seat, text refresh, cabin air,
/// radio) and the per-axle wheel, suspension and spring variables the rear section
/// writes, also after the vehicle gains an engine variable on the way.
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn cached_variable_ids_answer_as_the_names() {
    let root = original_root();
    let bus = root.join("Vehicles/MAN_NL_NG/MAN_GN92_main.bus");
    let trail = root.join("Vehicles/MAN_NL_NG/MAN_GN92_trail.bus");
    require_content(&[&bus, &trail]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("GN92"));
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    v.attach_trailer_ex(
        Arc::new(VehicleType::load(&root, &trail).expect("GN92 trail")),
        false,
    );
    for round in 0..2 {
        v.update(0.02);
        let p = v.ty.program.clone();
        let ids = *v.program_ids();
        assert_eq!(ids.refresh_strings, p.var("Refresh_Strings"));
        assert_eq!(ids.driver_seat, p.var("Driver_Seat_VertTransl"));
        assert!(ids.driver_seat.is_some(), "the GN92 has a sprung seat");
        assert_eq!(
            ids.owns_cabin_temp,
            p.var("Cabinair_Temp").is_some_and(|i| p.stores(i))
        );
        assert_eq!(
            ids.owns_cabin_rel_hum,
            p.var("Cabinair_relHum").is_some_and(|i| p.stores(i))
        );
        assert_eq!(ids.snd_radio_text, p.str_var("Snd_Radio_Text"));
        assert_eq!(ids.magnitola, p.str_var("magnitola_1"));
        assert_eq!(ids.mp3_freq, p.str_var("mp3_freq"));
        assert_eq!(ids.mp3_track, p.str_var("mp3_display_track_name"));
        let t = &v.trailers[0];
        let n = t.axle_count.max(t.rest.len());
        assert_eq!(t.axle_ids.rotation.len(), n);
        let mut found = 0;
        for a in 0..n {
            let k = t.first_axle + a;
            for (s, side) in ["L", "R"].into_iter().enumerate() {
                assert_eq!(
                    t.axle_ids.rotation[a][s],
                    p.var(&format!("Wheel_Rotation_{k}_{side}"))
                );
                assert_eq!(
                    t.axle_ids.rotation_speed[a][s],
                    p.var(&format!("Wheel_RotationSpeed_{k}_{side}"))
                );
                assert_eq!(
                    t.axle_ids.suspension[a][s],
                    p.var(&format!("Axle_Suspension_{k}_{side}"))
                );
                found += t.axle_ids.rotation[a][s].is_some() as usize;
            }
            let factor = v
                .var_slot(&format!("Axle_Springfactor_{k}_L"))
                .map(|i| i as omsi_script::VarId);
            assert_eq!(t.axle_ids.spring_factor[a], factor);
        }
        assert!(
            found > 0,
            "the rear section's wheels turn by script variables"
        );
        // the rear section's lamps read the vehicle's variables through its plan as by name
        let lamps: Vec<&str> =
            t.ty.model
                .meshes
                .iter()
                .flat_map(|md| &md.light_enh_2)
                .map(|l| l.variable.as_str())
                .collect();
        assert!(!lamps.is_empty(), "the rear section has lamps");
        assert_eq!(t.props_plan.lights.len(), lamps.len());
        for (src, name) in t.props_plan.lights.iter().zip(&lamps) {
            let by_name = name
                .trim()
                .parse::<f32>()
                .ok()
                .or_else(|| v.var(name.trim()))
                .unwrap_or(0.0);
            assert_eq!(
                src.value(&v.state.vars, 0.0).to_bits(),
                by_name.to_bits(),
                "{name}"
            );
        }
        // (round 1: with a variable the vehicle gained, the ids are made again)
        if round == 0 {
            v.set_engine_var("Axle_Springfactor_2_L", 1.0);
        }
    }
    assert!(
        v.trailers[0]
            .axle_ids
            .spring_factor
            .iter()
            .any(|i| i.is_some())
    );
}

/// Another player's GN92 standing on a slope: its rear section keeps the pose and the lean
/// their game sends, and its wheels stand still with the bus (#1177: re-walked behind our
/// copy of the front, the rear section lost its lean and its axle turned for ever).
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn a_remote_rear_section_keeps_its_pose_and_its_wheels_stand() {
    let root = original_root();
    let bus = root.join("Vehicles/MAN_NL_NG/MAN_GN92_main.bus");
    let trail = root.join("Vehicles/MAN_NL_NG/MAN_GN92_trail.bus");
    require_content(&[&bus, &trail]);
    let ty = Arc::new(VehicleType::load(&root, &bus).expect("GN92"));
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    v.attach_trailer_ex(
        Arc::new(VehicleType::load(&root, &trail).expect("GN92 trail")),
        false,
    );
    // the front climbs a 6 % grade heading north; the rear section stands 0.5 m lower
    v.pitch = 3.4;
    v.update_visuals(0.02);
    let c = v.trailers[0].coupling_point(v.position, v.body_rotation());
    let at = v.trailers[0].position + glam::DVec3::new(0.3, 0.0, -0.5);
    let rot_before = v.trailers[0].odometer;
    for _ in 0..50 {
        v.trailers[0].set_remote_pose(at, 2.0, c);
        v.update_visuals(0.02);
    }
    let t = &v.trailers[0];
    assert!(
        (t.position - at).length() < 1e-6,
        "moved to {:?}",
        t.position
    );
    assert!((t.heading - 2.0).abs() < 1e-9, "heading {}", t.heading);
    assert!(
        t.pitch > 1.0,
        "nose up towards the front section: {}",
        t.pitch
    );
    assert_eq!(
        t.odometer, rot_before,
        "its wheels turned on a standing bus"
    );
}

/// The skin plan moves the vertices as the per-vertex sum of weighted bones did.
#[test]
fn a_skin_plan_moves_the_vertices_as_the_weighted_bones_do() {
    use std::path::PathBuf;
    let mut ty = coupling_test_type(None);
    let t = Arc::get_mut(&mut ty).unwrap();
    let mesh = |def_index: usize, skin: Vec<SkinBone>, n: usize| VehicleMesh {
        def_index,
        data: MeshData {
            positions: (0..n)
                .map(|k| Vec3::new(k as f32, 0.5 * k as f32, 1.0 - 0.2 * k as f32))
                .collect(),
            normals: (0..n)
                .map(|k| Vec3::new(0.1 * k as f32, 1.0, 0.3).normalize())
                .collect(),
            ..Default::default()
        },
        file: PathBuf::new(),
        materials: Vec::new(),
        overrides: Vec::new(),
        pivot: Mat4::IDENTITY,
        viewpoint: 0,
        skin,
        keep_winding: false,
    };
    let bone = |def_index: Option<usize>, weights: &[(u32, f32)]| SkinBone {
        def_index,
        weights: weights.to_vec(),
    };
    t.meshes.push(mesh(10, Vec::new(), 1));
    t.meshes.push(mesh(11, Vec::new(), 1));
    t.meshes.push(mesh(
        12,
        vec![
            bone(
                Some(10),
                &[(0, 1.0), (1, 0.5), (2, 0.25), (5, 1e-5), (9, 1.0)],
            ),
            bone(Some(11), &[(1, 0.5), (2, 0.25), (3, f32::NAN), (3, 0.7)]),
            bone(None, &[(2, 0.5), (3, -1.0)]),
            bone(Some(99), &[(4, 1.0)]),
        ],
        7,
    ));
    let xf = |a: f32, x: f32| {
        Mat4::from_rotation_translation(
            glam::Quat::from_euler(glam::EulerRot::ZXY, a, 0.3 * a, -0.2 * a),
            Vec3::new(x, 2.0 * x, -x),
        )
    };
    let transforms = [xf(0.4, 1.0), xf(-0.7, 0.5), xf(0.2, -0.3)];
    let rest = [xf(0.1, 0.2), xf(0.05, -0.1), xf(-0.15, 0.4)];
    // the sum as it was made before the plan: a matrix per vertex, divided by the weight
    let vm = &ty.meshes[2];
    let mut sum = vec![Mat4::ZERO; 7];
    let mut total = vec![0.0f32; 7];
    for b in &vm.skin {
        let m = match b.def_index {
            None => Mat4::IDENTITY,
            Some(d) => match ty.meshes.iter().position(|m| m.def_index == d) {
                Some(k) => transforms[k] * rest[k].inverse(),
                None => continue,
            },
        };
        for &(v, w) in &b.weights {
            if (v as usize) < 7 && w.is_finite() && w > 0.0 {
                sum[v as usize] += m * w;
                total[v as usize] += w;
            }
        }
    }
    let plan = SkinPlan::new(&ty, 2).expect("a skinned mesh");
    let (pos, nrm) = skin_vertices(&ty, 2, &transforms, &rest, &plan).expect("skinned vertices");
    for v in 0..7 {
        let (p, q) = (vm.data.positions[v], vm.data.normals[v]);
        let (want_p, want_n) = if total[v] < 1e-4 {
            (p, q)
        } else {
            let m = rest[2].inverse() * (sum[v] * (1.0 / total[v]));
            (
                m.transform_point3(p),
                m.transform_vector3(q).normalize_or_zero(),
            )
        };
        assert!(
            (pos[v] - want_p).length() < 1e-4,
            "vertex {v}: {:?} for {:?}",
            pos[v],
            want_p
        );
        assert!(
            (nrm[v] - want_n).length() < 1e-4,
            "normal {v}: {:?} for {:?}",
            nrm[v],
            want_n
        );
    }
    // (vertices 4, 5 and 6: held by a missing bone, too little, by nothing - where they were)
    assert_eq!(&pos[4..], &vm.data.positions[4..]);
}

/// A road part's wheels turn at the speed it rolls along its own line - the vehicle's times
/// the cosine of the joint's angle (Omsi.exe 0x7de798) - over their own axle's size
/// (0x7debb0), not at the vehicle's speed.
#[test]
fn a_road_parts_wheels_turn_at_its_own_speed_over_their_size() {
    let base = crate::timetable_run::tests::script_test_vehicle(
        "{frame}\n{end}\n",
        "Wheel_RotationSpeed_0_L\nWheel_RotationSpeed_1_L\nWheel_RotationSpeed_1_R\narticulation_0_alpha\n",
        "",
    );
    let mut front = (*base.ty).clone();
    front.def.axles = vec![omsi_vehicle::Axle {
        long: 2.0,
        wheel_diameter: 1.0,
        ..Default::default()
    }];
    let mut rear = front.clone();
    rear.def.axles = vec![omsi_vehicle::Axle {
        long: -4.0,
        wheel_diameter: 0.8,
        ..Default::default()
    }];
    let mut v = VehicleInstance::new(Arc::new(front), VehicleHost::new(Default::default()));
    v.attach_trailer_ex(Arc::new(rear), false);
    v.update_visuals(0.02);
    // the front swings 50 degrees round while the rear part still lies the old way
    v.heading = 50.0;
    v.physics.speed = 10.0;
    v.update_visuals(0.02);
    let alpha = v.var("articulation_0_alpha").unwrap();
    assert!(alpha.abs() > 20.0, "joint at {alpha}");
    let pi = std::f32::consts::PI;
    let want = 600.0 * alpha.to_radians().cos() / (pi * 0.8);
    for side in ["L", "R"] {
        let rpm = v.var(&format!("Wheel_RotationSpeed_1_{side}")).unwrap();
        assert!((rpm - want).abs() < 0.05, "{side}: {rpm} rpm, want {want}");
    }
}

/// A part turned round turns its wheels the other way: the angle as well as the speed
/// (0x7debb0 with the part's +0x4e0), so the animation and the scripts' rpm agree.
#[test]
fn a_turned_round_parts_wheels_roll_backwards_in_angle_and_speed() {
    let base = crate::timetable_run::tests::script_test_vehicle(
        "{frame}\n{end}\n",
        "Wheel_RotationSpeed_1_L\nWheel_Rotation_1_L\n",
        "",
    );
    let mut ty = (*base.ty).clone();
    ty.def.axles = vec![omsi_vehicle::Axle {
        long: 2.0,
        wheel_diameter: 1.0,
        ..Default::default()
    }];
    let ty = Arc::new(ty);
    let mut v = VehicleInstance::new(ty.clone(), VehicleHost::new(Default::default()));
    v.attach_trailer_ex(ty, true);
    v.update_visuals(0.02);
    let start = v.var("Wheel_Rotation_1_L").unwrap();
    v.physics.speed = 10.0;
    // (the bus drives on: the part's wheels roll as its pivot moves)
    for _ in 0..5 {
        v.position.y += 0.2;
        v.update_visuals(0.02);
    }
    let rpm = v.var("Wheel_RotationSpeed_1_L").unwrap();
    let turned = v.var("Wheel_Rotation_1_L").unwrap() - start;
    assert!(
        rpm.abs() > 1.0 && turned.abs() > 1e-3,
        "rpm {rpm}, turned {turned}"
    );
    assert_eq!(
        rpm.signum(),
        turned.signum(),
        "the wheel turns one way and its rpm says the other: rpm {rpm}, turned {turned}"
    );
}
