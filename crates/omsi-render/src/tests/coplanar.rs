use super::*;

// The draw order's depth step (`draw_order_level`, `scene_clip`): of an object's surfaces in
// one plane the one later in the model is seen at every pixel, and nothing changes between
// objects. Without the step (`OMSI_NO_DRAW_ORDER_DEPTH=1`) the first two are a coin toss per
// pixel, about half of each.

fn renderer() -> Renderer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: 1,
            ssao: false,
            shadow_size: 1024,
            fxaa: false,
            render_scale: 1.0,
            ..Default::default()
        },
    ))
    .expect("test renderer")
}

/// A flat grid of `n` x `n` cells over `half` metres each way from (cx, cy) at height `z`,
/// turned by `turn` degrees, its cells cut along one diagonal or the other.
fn grid(cx: f32, cy: f32, z: f32, half: f32, n: u32, turn: f32, flip: bool) -> MeshData {
    let (s, c) = turn.to_radians().sin_cos();
    let mut m = MeshData {
        wire_wind: Vec::new(),
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
        ranges: Vec::new(),
        indices: Vec::new(),
        one_sided: false,
    };
    for j in 0..=n {
        for i in 0..=n {
            let (x, y) = (
                -half + 2.0 * half * i as f32 / n as f32,
                -half + 2.0 * half * j as f32 / n as f32,
            );
            m.positions
                .push(Vec3::new(cx + x * c - y * s, cy + x * s + y * c, z));
            m.normals.push(Vec3::Z);
            m.uvs.push(glam::Vec2::ZERO);
        }
    }
    for j in 0..n {
        for i in 0..n {
            let k = j * (n + 1) + i;
            let (a, b, c2, d) = (k, k + 1, k + n + 2, k + n + 1);
            if flip {
                m.indices.extend([a, b, d, b, c2, d])
            } else {
                m.indices.extend([a, b, c2, a, c2, d])
            }
        }
    }
    m.ranges = vec![(0, m.indices.len() as u32, 0)];
    m
}

/// `grids` as one mesh, a material slot each, in that order.
fn slotted(grids: Vec<MeshData>) -> MeshData {
    let mut m = MeshData {
        wire_wind: Vec::new(),
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
        ranges: Vec::new(),
        indices: Vec::new(),
        one_sided: false,
    };
    for (slot, g) in grids.into_iter().enumerate() {
        let (base, first) = (m.positions.len() as u32, m.indices.len() as u32);
        m.positions.extend(&g.positions);
        m.normals.extend(&g.normals);
        m.uvs.extend(&g.uvs);
        m.indices.extend(g.indices.iter().map(|i| i + base));
        m.ranges.push((first, g.indices.len() as u32, slot as u32));
    }
    m
}

/// Pixels of each colour (red, green, blue) from a low, a grazing, a steep and a high camera.
fn count(renderer: &mut Renderer, scene: &mut Scene) -> [usize; 3] {
    let lighting = Lighting {
        shadows: false,
        fog_density: 0.0,
        ..Default::default()
    };
    let mut n = [0; 3];
    for (pitch, height) in [(-3.0, 1.7), (-12.0, 1.7), (-40.0, 6.0), (-6.0, 25.0)] {
        let camera = Camera {
            position: DVec3::new(3.3, -2.1, height),
            yaw: 20.0,
            pitch,
            roll: 0.0,
            fov_deg: 60.0,
            near: 0.1,
            far: 3000.0,
        };
        scene.dirty = true;
        let rgba = renderer
            .render_to_image(scene, 128, 128, &camera, &lighting)
            .unwrap();
        for p in rgba.chunks(4) {
            for (k, n) in n.iter_mut().enumerate() {
                let others = (0..3).filter(|&o| o != k).all(|o| p[o] < 60);
                if p[k] > 150 && others {
                    *n += 1;
                }
            }
        }
    }
    n
}

/// Two meshes of one object in one plane after four others of it (the second from the same
/// place, created right after it, cut into cells of 8 m along the other diagonal and
/// turned, all over the first's 5 m cells): the second is seen at every pixel.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn an_objects_later_mesh_in_one_plane_is_seen() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let red = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [1.0, 0.0, 0.0, 1.0],
        true,
    );
    let green = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 1.0, 0.0, 1.0],
        true,
    );
    let a = renderer.add_mesh(&mut scene, &grid(0.0, 0.0, 0.0, 150.0, 60, 0.0, false));
    let b = renderer.add_mesh(&mut scene, &grid(0.0, 0.0, 0.0, 400.0, 100, 33.0, true));
    let origin = DVec3::new(-61.0, 12.5, 0.0);
    // (as in a bus: its meshes all from one place, the two far down the list)
    let blue = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 0.0, 1.0, 1.0],
        true,
    );
    renderer.begin_object(&mut scene);
    for k in 0..4 {
        let other = renderer.add_mesh(
            &mut scene,
            &grid(0.0, 0.0, -3.0 - k as f32, 2.0, 1, 0.0, false),
        );
        renderer.add_instance(&mut scene, other, origin, Mat4::IDENTITY, vec![blue]);
    }
    renderer.add_instance(&mut scene, a, origin, Mat4::IDENTITY, vec![red]);
    renderer.add_instance(&mut scene, b, origin, Mat4::IDENTITY, vec![green]);
    let [r, g, _] = count(&mut renderer, &mut scene);
    // (a few pixels in a thousand far out at a grazing angle are left, where the step that
    // would hold them would also bury a centimetre between two objects)
    assert!(
        g > 15_000 && r <= g / 200,
        "{r} pixels of the first mesh, {g} of the second"
    );
}

/// One mesh, three slots in one plane: the third is drawn over the other two.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_meshs_last_slot_in_one_plane_is_seen() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let red = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [1.0, 0.0, 0.0, 1.0],
        true,
    );
    let blue = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 0.0, 1.0, 1.0],
        true,
    );
    let green = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 1.0, 0.0, 1.0],
        true,
    );
    let mesh = renderer.add_mesh(
        &mut scene,
        &slotted(vec![
            grid(0.0, 0.0, 0.0, 150.0, 60, 0.0, false),
            grid(10.0, 5.0, 0.0, 300.0, 70, 17.0, true),
            grid(0.0, 0.0, 0.0, 400.0, 100, 33.0, true),
        ]),
    );
    renderer.begin_object(&mut scene);
    renderer.add_instance(
        &mut scene,
        mesh,
        DVec3::new(-61.0, 12.5, 0.0),
        Mat4::IDENTITY,
        vec![red, blue, green],
    );
    let [r, g, b] = count(&mut renderer, &mut scene);
    // (as above, and the more so where the plane is seen flatter than 1 in 20, whose step
    // stays at what 1 in 20 gives, so that the edge of a curved mesh is not pulled forward:
    // under one in a hundred)
    assert!(
        g > 15_000 && r + b <= g / 100,
        "first slot {r}, second {b}, last {g} pixels"
    );
}

/// Between objects nothing moves: a plate a centimetre over another object's ground, whose
/// slot there is two steps up, stays over it near and far.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_plate_a_centimetre_over_another_objects_ground_stays_over_it() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let red = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [1.0, 0.0, 0.0, 1.0],
        true,
    );
    let blue = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 0.0, 1.0, 1.0],
        true,
    );
    let green = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 1.0, 0.0, 1.0],
        true,
    );
    // (80 m each way: further out, at a grazing angle, a centimetre is below what the depth
    // resolves, with or without the step)
    let raised = renderer.add_mesh(&mut scene, &grid(0.0, 0.0, 0.01, 80.0, 32, 0.0, false));
    renderer.begin_object(&mut scene);
    renderer.add_instance(&mut scene, raised, DVec3::ZERO, Mat4::IDENTITY, vec![green]);
    // (the ground under it, inside it: an object of three slots in one plane, the last
    // lying on the other two and so two steps up)
    let ground = renderer.add_mesh(
        &mut scene,
        &slotted(vec![
            grid(0.0, 0.0, 0.0, 10.0, 2, 0.0, false),
            grid(0.0, 0.0, 0.0, 8.0, 2, 10.0, false),
            grid(0.0, 0.0, 0.0, 55.0, 14, 33.0, true),
        ]),
    );
    renderer.begin_object(&mut scene);
    renderer.add_instance(
        &mut scene,
        ground,
        DVec3::new(3.0, -2.0, 0.0),
        Mat4::IDENTITY,
        vec![blue, blue, red],
    );
    let [r, g, _] = count(&mut renderer, &mut scene);
    assert!(
        g > 3_000 && r == 0,
        "{r} pixels of the ground through the plate a centimetre over it, {g} of the plate"
    );
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_text_a_tenth_of_a_millimetre_over_a_later_board_stays_seen_from_far() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let red = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [1.0, 0.0, 0.0, 1.0],
        true,
    );
    let green = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 1.0, 0.0, 1.0],
        true,
    );
    let text = renderer.add_mesh(&mut scene, &grid(0.0, 0.0, 0.0001, 2.0, 4, 0.0, false));
    let board = renderer.add_mesh(&mut scene, &grid(0.0, 0.0, 0.0, 4.0, 4, 0.0, true));
    renderer.begin_object(&mut scene);
    renderer.add_instance(&mut scene, text, DVec3::ZERO, Mat4::IDENTITY, vec![green]);
    renderer.add_instance(&mut scene, board, DVec3::ZERO, Mat4::IDENTITY, vec![red]);
    let lighting = Lighting {
        shadows: false,
        fog_density: 0.0,
        ..Default::default()
    };
    for d in [3.0, 15.0, 30.0] {
        for pitch in [-90.0f64, -45.0] {
            let (s, c) = pitch.to_radians().sin_cos();
            let camera = Camera {
                position: DVec3::new(0.0, -d * c, -d * s),
                yaw: 0.0,
                pitch: pitch as f32,
                roll: 0.0,
                fov_deg: 60.0,
                near: 0.1,
                far: 3000.0,
            };
            scene.dirty = true;
            let rgba = renderer
                .render_to_image(&mut scene, 256, 256, &camera, &lighting)
                .unwrap();
            let green = rgba
                .chunks(4)
                .filter(|p| p[1] > 150 && p[0] < 60 && p[2] < 60)
                .count();
            let red = rgba
                .chunks(4)
                .filter(|p| p[0] > 150 && p[1] < 60 && p[2] < 60)
                .count();
            assert!(
                green * 4 > red,
                "at {d} m, pitch {pitch}: {green} pixels of the text, {red} of the board"
            );
        }
    }
}

/// The order-sensitive case from the review: a pane 0.2 mm in front of a base, added first,
/// then the base and its coplanar print. The print keeps the step that orders it over its
/// base, so the base shows through it nowhere - without that step the two tie and it does,
/// at every distance.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_print_behind_a_pane_added_first_stays_over_its_base() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let red = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [1.0, 0.0, 0.0, 1.0],
        true,
    );
    let green = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 1.0, 0.0, 1.0],
        true,
    );
    let blue = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 0.0, 1.0, 1.0],
        true,
    );
    // (the pane 0.2 mm in front, over the left half of what follows)
    let pane = renderer.add_mesh(&mut scene, &grid(-0.5, 0.0, 0.0002, 0.5, 4, 0.0, false));
    // (the base, and its print in the same plane cut another way and larger, so that the print
    // covers the base whole: no corner of the base is meant to show)
    let base = renderer.add_mesh(
        &mut scene,
        &slotted(vec![
            grid(0.0, 0.0, 0.0, 0.8, 4, 0.0, false),
            grid(0.0, 0.0, 0.0, 1.3, 4, 33.0, true),
        ]),
    );
    renderer.begin_object(&mut scene);
    renderer.add_instance(&mut scene, pane, DVec3::ZERO, Mat4::IDENTITY, vec![blue]);
    renderer.add_instance(
        &mut scene,
        base,
        DVec3::ZERO,
        Mat4::IDENTITY,
        vec![red, green],
    );
    let lighting = Lighting {
        shadows: false,
        fog_density: 0.0,
        ..Default::default()
    };
    for d in [2.0, 7.0, 15.0, 30.0] {
        for pitch in [-90.0f64, -45.0] {
            let (s, c) = pitch.to_radians().sin_cos();
            let camera = Camera {
                position: DVec3::new(0.0, -d * c, -d * s),
                yaw: 0.0,
                pitch: pitch as f32,
                roll: 0.0,
                fov_deg: 60.0,
                near: 0.1,
                far: 3000.0,
            };
            scene.dirty = true;
            let rgba = renderer
                .render_to_image(&mut scene, 256, 256, &camera, &lighting)
                .unwrap();
            let green = rgba
                .chunks(4)
                .filter(|p| p[1] > 150 && p[0] < 60 && p[2] < 60)
                .count();
            let red = rgba
                .chunks(4)
                .filter(|p| p[0] > 150 && p[1] < 60 && p[2] < 60)
                .count();
            let blue = rgba
                .chunks(4)
                .filter(|p| p[2] > 150 && p[0] < 60 && p[1] < 60)
                .count();
            // (the print covers its base whole where the pane does not: the base shows through
            // it nowhere)
            assert!(
                red == 0 && green > 0,
                "at {d} m, pitch {pitch}: {green} pixels of the print, {red} of the base through it"
            );
            assert!(
                blue > 0,
                "at {d} m, pitch {pitch}: the print passed the pane completely"
            );
        }
    }
}

/// The same, with the print blended (`[matl_alpha] 2`, as prints on a dashboard are): blending
/// does not bypass the depth test, so a blended print needs its step over its base just as an
/// opaque one does, and keeps it beside a pane 0.2 mm off its plane.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_blended_print_behind_a_pane_added_first_stays_over_its_base() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let red = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [1.0, 0.0, 0.0, 1.0],
        true,
    );
    let green = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Blend,
        [0.0, 1.0, 0.0, 1.0],
        true,
    );
    let blue = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 0.0, 1.0, 1.0],
        true,
    );
    let pane = renderer.add_mesh(&mut scene, &grid(-0.5, 0.0, 0.0002, 0.5, 4, 0.0, false));
    let base = renderer.add_mesh(
        &mut scene,
        &slotted(vec![
            grid(0.0, 0.0, 0.0, 0.8, 4, 0.0, false),
            grid(0.0, 0.0, 0.0, 1.3, 4, 33.0, true),
        ]),
    );
    renderer.begin_object(&mut scene);
    renderer.add_instance(&mut scene, pane, DVec3::ZERO, Mat4::IDENTITY, vec![blue]);
    renderer.add_instance(
        &mut scene,
        base,
        DVec3::ZERO,
        Mat4::IDENTITY,
        vec![red, green],
    );
    let lighting = Lighting {
        shadows: false,
        fog_density: 0.0,
        ..Default::default()
    };
    for d in [2.0, 7.0, 15.0, 30.0] {
        for pitch in [-90.0f64, -45.0] {
            let (s, c) = pitch.to_radians().sin_cos();
            let camera = Camera {
                position: DVec3::new(0.0, -d * c, -d * s),
                yaw: 0.0,
                pitch: pitch as f32,
                roll: 0.0,
                fov_deg: 60.0,
                near: 0.1,
                far: 3000.0,
            };
            scene.dirty = true;
            let rgba = renderer
                .render_to_image(&mut scene, 256, 256, &camera, &lighting)
                .unwrap();
            let green = rgba
                .chunks(4)
                .filter(|p| p[1] > 150 && p[0] < 60 && p[2] < 60)
                .count();
            let red = rgba
                .chunks(4)
                .filter(|p| p[0] > 150 && p[1] < 60 && p[2] < 60)
                .count();
            let blue = rgba
                .chunks(4)
                .filter(|p| p[2] > 150 && p[0] < 60 && p[1] < 60)
                .count();
            // (the print covers its base whole where the pane does not: the base shows through
            // it nowhere)
            assert!(
                red == 0 && green > 0,
                "at {d} m, pitch {pitch}: {green} pixels of the print, {red} of the base through it"
            );
            assert!(
                blue > 0,
                "at {d} m, pitch {pitch}: the print passed the pane completely"
            );
        }
    }
}

/// A label a fraction of a millimetre in front of a glass three steps up - a windscreen's
/// fleet number and its prints on the pane, each its own subset, the pane's own patches piling
/// steps on each other (#2790) - stays seen at every distance: a stack of steps takes no more
/// than the surface in front of it, so its lift cannot carry it past that surface.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_label_a_fraction_of_a_millimetre_in_front_of_a_stack_of_steps_stays_seen() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let red = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [1.0, 0.0, 0.0, 1.0],
        true,
    );
    let green = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 1.0, 0.0, 1.0],
        true,
    );
    let blue = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 0.0, 1.0, 1.0],
        true,
    );
    // (a base and three surfaces lying on it, each in its plane cut another way or smaller:
    // the last, the glass, is three steps up)
    let glass = renderer.add_mesh(
        &mut scene,
        &slotted(vec![
            grid(0.0, 0.0, 0.0, 1.0, 4, 0.0, false),
            grid(0.0, 0.0, 0.0, 1.0, 5, 33.0, true),
            grid(0.0, 0.0, 0.0, 0.8, 6, 12.0, false),
            grid(0.0, 0.0, 0.0, 1.0, 4, 47.0, true),
        ]),
    );
    // (the label 0.2 mm in front of the glass, drawn before it - two subsets of one mesh, as a
    // fleet number and the print beside it)
    let label = renderer.add_mesh(
        &mut scene,
        &slotted(vec![
            grid(0.0, 0.0, 0.0002, 0.5, 2, 0.0, false),
            grid(0.0, 0.0, 0.0002, 0.4, 3, 21.0, true),
        ]),
    );
    renderer.begin_object(&mut scene);
    renderer.add_instance(
        &mut scene,
        label,
        DVec3::ZERO,
        Mat4::IDENTITY,
        vec![green, green],
    );
    renderer.add_instance(
        &mut scene,
        glass,
        DVec3::ZERO,
        Mat4::IDENTITY,
        vec![blue, blue, blue, red],
    );
    let lighting = Lighting {
        shadows: false,
        fog_density: 0.0,
        ..Default::default()
    };
    for d in [2.0, 7.0, 15.0, 30.0] {
        for pitch in [-90.0f64, -45.0] {
            let (s, c) = pitch.to_radians().sin_cos();
            let camera = Camera {
                position: DVec3::new(0.0, -d * c, -d * s),
                yaw: 0.0,
                pitch: pitch as f32,
                roll: 0.0,
                fov_deg: 60.0,
                near: 0.1,
                far: 3000.0,
            };
            scene.dirty = true;
            let rgba = renderer
                .render_to_image(&mut scene, 256, 256, &camera, &lighting)
                .unwrap();
            let green = rgba
                .chunks(4)
                .filter(|p| p[1] > 150 && p[0] < 60 && p[2] < 60)
                .count();
            let red = rgba
                .chunks(4)
                .filter(|p| p[0] > 150 && p[1] < 60 && p[2] < 60)
                .count();
            assert!(
                green > 0 && red > 0,
                "at {d} m, pitch {pitch}: {green} pixels of the label, {red} of the top glass layer"
            );
        }
    }
}

/// A blended screen slot in a stacked display keeps the measured cap: its full step must not
/// carry it past a label 0.2 mm in front of the stack (#2790).
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_label_a_fraction_of_a_millimetre_in_front_of_a_stacked_screen_stays_seen() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let red = renderer.add_material_extra(
        &mut scene,
        None,
        AlphaMode::Blend,
        [1.0, 0.0, 0.0, 1.0],
        true,
        None,
        None,
        None,
        None,
        [0.0; 3],
        MaterialExtra {
            screen: true,
            ..Default::default()
        },
    );
    let green = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 1.0, 0.0, 1.0],
        true,
    );
    let blue = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 0.0, 1.0, 1.0],
        true,
    );
    let glass = renderer.add_mesh(
        &mut scene,
        &slotted(vec![
            grid(0.0, 0.0, 0.0, 1.0, 4, 0.0, false),
            grid(0.0, 0.0, 0.0, 1.0, 5, 33.0, true),
            grid(0.0, 0.0, 0.0, 0.8, 6, 12.0, false),
            grid(0.0, 0.0, 0.0, 1.0, 4, 47.0, true),
        ]),
    );
    let label = renderer.add_mesh(
        &mut scene,
        &slotted(vec![
            grid(0.0, 0.0, 0.0002, 0.5, 2, 0.0, false),
            grid(0.0, 0.0, 0.0002, 0.4, 3, 21.0, true),
        ]),
    );
    renderer.begin_object(&mut scene);
    renderer.add_instance(
        &mut scene,
        label,
        DVec3::ZERO,
        Mat4::IDENTITY,
        vec![green, green],
    );
    renderer.add_instance(
        &mut scene,
        glass,
        DVec3::ZERO,
        Mat4::IDENTITY,
        vec![blue, blue, blue, red],
    );
    let lighting = Lighting {
        shadows: false,
        fog_density: 0.0,
        ..Default::default()
    };
    for d in [2.0, 7.0, 15.0, 30.0] {
        for pitch in [-90.0f64, -45.0] {
            let (s, c) = pitch.to_radians().sin_cos();
            let camera = Camera {
                position: DVec3::new(0.0, -d * c, -d * s),
                yaw: 0.0,
                pitch: pitch as f32,
                roll: 0.0,
                fov_deg: 60.0,
                near: 0.1,
                far: 3000.0,
            };
            scene.dirty = true;
            let rgba = renderer
                .render_to_image(&mut scene, 256, 256, &camera, &lighting)
                .unwrap();
            let green = rgba
                .chunks(4)
                .filter(|p| p[1] > 150 && p[0] < 60 && p[2] < 60)
                .count();
            let red = rgba
                .chunks(4)
                .filter(|p| p[0] > 150 && p[1] < 60 && p[2] < 60)
                .count();
            let centre = &rgba[(128 * 256 + 128) * 4..(128 * 256 + 128) * 4 + 3];
            assert!(
                green > 0 && red > 0,
                "at {d} m, pitch {pitch}: {green} pixels of the label, {red} of the stacked screen"
            );
            assert!(
                centre[1] > 150 && centre[0] < 60 && centre[2] < 60,
                "the front label must remain visible over the screen at {d} m, pitch {pitch}: {centre:?}"
            );
        }
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_coplanar_display_frame_and_text_stay_visible_under_a_front_label() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let base = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.3; 4], true);
    let frame = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Test,
        [1.0, 0.0, 0.0, 1.0],
        true,
    );
    let label = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 0.0, 1.0, 1.0],
        true,
    );
    let mut rgba = Vec::new();
    for y in 0..32 {
        for x in 0..32 {
            let alpha = if (12..20).contains(&x) && (8..24).contains(&y) {
                255
            } else {
                0
            };
            rgba.extend([0, 255, 0, alpha]);
        }
    }
    let glyph = renderer.add_texture(
        &mut scene,
        &omsi_texture::Image {
            width: 32,
            height: 32,
            rgba,
            has_alpha: true,
        },
        false,
    );
    let text = renderer.add_material(&mut scene, Some(glyph), AlphaMode::Blend, [1.0; 4], true);
    let row = grid(0.0, 0.0, 0.0, 0.45, 1, 0.0, false);
    let tilt = |mut row: MeshData| {
        for point in &mut row.positions {
            point.z += 0.006 * point.y;
        }
        row
    };
    let adjacent = grid(1.0, 0.0, 0.0, 0.35, 1, 0.0, false);
    let independent = grid(-0.9, 0.0, 0.0, 0.3, 1, 0.0, false);
    let with_uvs = |mut grid: MeshData| {
        use glam::Vec2;
        grid.uvs = [Vec2::ZERO, Vec2::X, Vec2::Y, Vec2::ONE].to_vec();
        grid
    };
    let mut display = slotted(vec![
        row.clone(),
        adjacent.clone(),
        row.clone(),
        with_uvs(row),
        with_uvs(adjacent),
        with_uvs(independent.clone()),
        with_uvs(tilt(grid(-2.1, 0.0, 0.0, 0.55, 1, 0.0, false))),
        with_uvs(independent),
        with_uvs(grid(-0.9, 0.0, 0.0, 0.3, 1, 0.0, false)),
    ]);
    for (range, slot) in display.ranges.iter_mut().zip([0, 0, 1, 2, 2, 2, 2, 3, 4]) {
        range.2 = slot;
    }
    let display = renderer.add_mesh(&mut scene, &display);
    let support = renderer.add_mesh(
        &mut scene,
        &slotted(vec![
            grid(0.0, 0.0, 0.0, 1.5, 1, 0.0, false),
            grid(0.0, 0.0, 0.0, 1.5, 1, 0.0, false),
            grid(0.0, 0.0, 0.0, 1.5, 1, 0.0, false),
        ]),
    );
    let front = renderer.add_mesh(&mut scene, &grid(1.0, 0.0, 0.0002, 0.25, 1, 0.0, false));
    renderer.begin_object(&mut scene);
    for (mesh, materials) in [
        (front, vec![label]),
        (support, vec![base; 3]),
        (display, vec![base, frame, text, text, text]),
    ] {
        renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, materials);
    }
    let display = scene.instances.last().unwrap();
    let caps = display.slot_depth_caps.clone();
    let levels = display.slot_levels.clone();
    assert!(caps[1].shared && caps[2].shared);
    assert!(
        caps[2].positive.is_finite(),
        "the multi-row print needs the front label's cap"
    );
    for slot in [3, 4] {
        assert!(
            !caps[slot].shared && caps[slot].positive.is_infinite(),
            "unrelated text slot {slot} inherited a cap: {:?}",
            caps[slot]
        );
    }
    let camera = Camera {
        position: DVec3::new(0.0, -12.0, 9.0),
        yaw: 0.0,
        pitch: -(0.75f32).atan().to_degrees(),
        roll: 0.0,
        fov_deg: 14.0,
        near: 0.1,
        far: 3000.0,
    };
    let lighting = Lighting {
        shadows: false,
        fog_density: 0.0,
        ..Default::default()
    };
    let image = renderer
        .render_to_image(&mut scene, 256, 256, &camera, &lighting)
        .unwrap();
    let vp = camera.view_proj(1.0, camera.position);
    for (point, colour, name) in [
        (Vec3::new(-0.26, 0.0, 0.0), 0, "frame"),
        (Vec3::new(1.0, 0.0, 0.0002), 2, "front label"),
        (Vec3::ZERO, 1, "text"),
    ] {
        let ndc = vp.project_point3(point - camera.position.as_vec3());
        let x = ((ndc.x * 0.5 + 0.5) * 256.0) as usize;
        let y = ((0.5 - ndc.y * 0.5) * 256.0) as usize;
        let pixel = &image[(y * 256 + x) * 4..][..3];
        assert!(
            pixel[colour] > 150 && pixel[(colour + 1) % 3] < 60 && pixel[(colour + 2) % 3] < 60,
            "{name} at x={x}: {pixel:?}, levels={levels:?}, caps={caps:?}"
        );
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn vehicle_duplicate_support_keeps_its_depth_above_earlier_meshes() {
    let renderer = renderer();
    let mut scene = renderer.new_scene();
    let material = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0; 4], true);
    let support = renderer.add_mesh(
        &mut scene,
        &slotted(vec![
            grid(0.0, 0.0, 0.0, 0.2, 1, 0.0, false),
            grid(0.0, 0.0, 0.0, 0.2, 1, 0.0, true),
        ]),
    );
    assert_eq!(scene.meshes[support].duplicate_surface_slots, [0]);
    renderer.begin_object(&mut scene);
    for half in [1.0, 0.8, 0.6] {
        let mesh = renderer.add_mesh(&mut scene, &grid(0.0, 0.0, 0.0, half, 1, 0.0, false));
        renderer.add_instance(
            &mut scene,
            mesh,
            DVec3::ZERO,
            Mat4::IDENTITY,
            vec![material],
        );
    }
    let instance = renderer.add_instance(
        &mut scene,
        support,
        DVec3::ZERO,
        Mat4::IDENTITY,
        vec![material; 2],
    );
    assert_eq!(scene.instances[instance].slot_depth_floors[0], 2);
    assert_eq!(scene.instances[instance].slot_levels[0], 3);
    renderer.limit_duplicate_support_depth(&mut scene, instance);
    assert_eq!(scene.instances[instance].slot_levels[0], 3);
}

/// Shading normals may be authored independently of a display's flat geometry.
/// They must not bend the depth offset and lift an earlier layer through its print.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn shading_normals_do_not_bend_a_flat_layers_depth() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let red = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [1.0, 0.0, 0.0, 1.0],
        true,
    );
    let green = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 1.0, 0.0, 1.0],
        true,
    );
    let mut layer = grid(0.0, 0.0, 0.0, 0.3, 1, 0.0, true);
    layer.normals[0] = Vec3::new(1.0, 0.0, -0.3).normalize();
    let mesh = renderer.add_mesh(
        &mut scene,
        &slotted(vec![
            grid(0.0, 0.0, 0.0, 0.4, 1, 0.0, false),
            layer,
            grid(0.0, 0.0, 0.000025, 0.5, 1, 0.0, false),
        ]),
    );
    renderer.begin_object(&mut scene);
    renderer.add_instance(
        &mut scene,
        mesh,
        DVec3::ZERO,
        Mat4::IDENTITY,
        vec![red, red, green],
    );
    let lighting = Lighting {
        shadows: false,
        fog_density: 0.0,
        ..Default::default()
    };
    for pitch in [-90.0f64, -45.0, -15.0] {
        let (s, c) = pitch.to_radians().sin_cos();
        let camera = Camera {
            position: DVec3::new(0.0, -c, -s),
            yaw: 0.0,
            pitch: pitch as f32,
            roll: 0.0,
            fov_deg: 60.0,
            near: 0.1,
            far: 3000.0,
        };
        scene.dirty = true;
        let rgba = renderer
            .render_to_image(&mut scene, 256, 256, &camera, &lighting)
            .unwrap();
        let red = rgba
            .chunks_exact(4)
            .filter(|p| p[0] > 150 && p[1] < 60 && p[2] < 60)
            .count();
        let green = rgba
            .chunks_exact(4)
            .filter(|p| p[1] > 150 && p[0] < 60 && p[2] < 60)
            .count();
        assert!(
            red == 0 && green > 0,
            "pitch {pitch}: {red} base pixels, {green} print pixels"
        );
    }
}
