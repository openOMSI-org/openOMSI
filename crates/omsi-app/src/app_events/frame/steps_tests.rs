use super::*;
use glam::{DVec3, Vec3};

/// The enhanced shader's `pane_local` (03_surface.wgsl) on the values `dress_lighting` gives it
/// (`inside_a`, `inside_b`, `inside_c` as passes/setup.rs fills them, and `pane_tilt`).
fn shader_pane_local(l: &omsi_render::Lighting, world: DVec3) -> Vec3 {
    let (o, h, bb) = l.inside.unwrap();
    let d = (world - o).as_vec3();
    let (sh, ch) = (h as f32).to_radians().sin_cos();
    let v = Vec3::new(d.x * ch - d.y * sh, d.x * sh + d.y * ch, d.z);
    let t = l.pane_tilt;
    let p = Vec3::new(v.x, v.y * t[1] + v.z * t[0], v.z * t[1] - v.y * t[0]);
    let b = Vec3::new(p.x * t[3] - p.z * t[2], p.y, p.z * t[3] + p.x * t[2]);
    b - Vec3::new(bb[3], bb[4], bb[5])
}

fn tram(boogies: Option<f32>, bb: [f32; 6]) -> omsi_sim::VehicleInstance {
    let ty = std::sync::Arc::new(omsi_sim::VehicleType {
        def: omsi_vehicle::Vehicle {
            boogies,
            bounding_box: Some(bb),
            ..Default::default()
        },
        model: Default::default(),
        model_dir: Default::default(),
        program: Default::default(),
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
    let mut v = omsi_sim::VehicleInstance::new(ty, omsi_sim::VehicleHost::new(Default::default()));
    v.position = DVec3::new(120.0, -45.0, 12.0);
    v.heading = 35.0;
    v.pitch = 7.0;
    v.bank = -4.0;
    v
}

#[test]
fn the_panes_frame_is_the_drawn_body_of_a_reversed_tram() {
    let bb = [2.4, 14.0, 3.2, 0.3, 1.2, 1.5];
    for boogies in [Some(-6.0), Some(6.0), None] {
        let v = tram(boogies, bb);
        let mut l = omsi_render::Lighting::default();
        dress_lighting(
            &mut l,
            None,
            Some(&v),
            &crate::settings::Settings::default(),
        );
        assert_eq!(l.inside_rotation, Some(v.body_rotation()));
        let rot = v.body_rotation();
        let c = Vec3::new(bb[3], bb[4], bb[5]);
        let (hx, hy, hz) = (bb[0] * 0.5, bb[1] * 0.5, bb[2] * 0.5);
        // (the windscreen's top, the rear window's foot, a side window and the box's centre)
        for m in [
            c + Vec3::new(0.2, hy, hz * 0.8),
            c + Vec3::new(-0.5, -hy, -hz * 0.5),
            c + Vec3::new(hx, 2.0, 0.4),
            c,
        ] {
            let world = v.position + rot.transform_vector3(m).as_dvec3();
            let got = shader_pane_local(&l, world);
            assert!(
                got.abs_diff_eq(m - c, 1e-3),
                "boogies {boogies:?}: model point {m} reaches the shader as {got}, not {}",
                m - c
            );
        }
    }
}

#[test]
fn ticketless_fares_do_not_count_as_tickets_sold_in_career() {
    let mut career = omsi_career::career::Career::default();
    let mut humans = crate::humans::Humans {
        sim: omsi_sim::people::PeopleSim::new(std::path::Path::new("/nonexistent"), 200),
    };
    humans.sim.ticket_cash = 3.5;
    humans.sim.fares_no_ticket = 1;
    assert_eq!(humans.tickets_sold, 0);

    career_from_humans(&mut career, &humans);
    assert_eq!(career.tickets.0, 0, "tickets sold in career remains 0");
    assert_eq!(
        career.tickets.1, 3.5,
        "cash collected from ticketless fare is recorded"
    );
}
