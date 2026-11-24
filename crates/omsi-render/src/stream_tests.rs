//! The streaming's GPU side: textures shrunk by the budget, instance buffers that grow.

use super::*;

/// The texture budget shrinks far textures together (one command buffer for all): each
/// keeps its coarser levels, and the materials drawing it, rebound, show the same picture.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn textures_shrunk_together_keep_their_picture() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
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
    .expect("test renderer");
    let mut scene = renderer.new_scene();
    let mesh = renderer.add_mesh(
        &mut scene,
        &MeshData {
            positions: vec![
                Vec3::new(-8.0, 4.0, -8.0),
                Vec3::new(8.0, 4.0, -8.0),
                Vec3::new(8.0, 4.0, 8.0),
                Vec3::new(-8.0, 4.0, 8.0),
            ],
            normals: vec![-Vec3::Y; 4],
            uvs: vec![
                glam::Vec2::ZERO,
                glam::Vec2::X,
                glam::Vec2::ONE,
                glam::Vec2::Y,
            ],
            ranges: vec![(0, 6, 0)],
            indices: vec![0, 1, 2, 0, 2, 3],
            one_sided: false,
        },
    );
    let image = |side: u32, rgba: [u8; 4]| omsi_texture::Image {
        width: side,
        height: side,
        rgba: rgba
            .iter()
            .copied()
            .cycle()
            .take((side * side * 4) as usize)
            .collect(),
        has_alpha: false,
    };
    let red = renderer.add_texture(&mut scene, &image(256, [220, 20, 20, 255]), true);
    let green = renderer.add_texture(&mut scene, &image(128, [20, 200, 20, 255]), true);
    let flat = renderer.add_texture(&mut scene, &image(64, [20, 20, 200, 255]), false);
    let material = renderer.add_material(&mut scene, Some(red), AlphaMode::Opaque, [1.0; 4], true);
    renderer.add_instance(
        &mut scene,
        mesh,
        DVec3::ZERO,
        Mat4::IDENTITY,
        vec![material],
    );
    let camera = Camera {
        position: DVec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 100.0,
    };
    let lighting = Lighting {
        shadows: false,
        fog_density: 0.0,
        sun_dir: -Vec3::Y,
        ..Default::default()
    };
    let before = renderer
        .render_to_image(&mut scene, 64, 64, &camera, &lighting)
        .unwrap();
    let (w, h, levels) = renderer.texture_levels(&scene, red).unwrap();
    assert_eq!((w, h), (256, 256));
    let bytes = scene.texture_bytes_of(red);
    let mut drops = renderer.begin_level_drops();
    assert!(renderer.drop_top_levels_in(&mut drops, &mut scene, red, 2));
    assert!(renderer.drop_top_levels_in(&mut drops, &mut scene, green, 1));
    // (no levels to give)
    assert!(!renderer.drop_top_levels_in(&mut drops, &mut scene, flat, 1));
    assert_eq!(renderer.finish_level_drops(drops), 2);
    assert_eq!(
        renderer.texture_levels(&scene, red),
        Some((64, 64, levels - 2))
    );
    assert_eq!(
        renderer.texture_levels(&scene, green).map(|t| (t.0, t.1)),
        Some((64, 64))
    );
    assert!(
        scene.texture_bytes_of(red) * 8 < bytes,
        "{} of {bytes}",
        scene.texture_bytes_of(red)
    );
    assert_eq!(
        renderer.rebind_textures(&mut scene, &[red, green]),
        1,
        "the one material drawing them"
    );
    let after = renderer
        .render_to_image(&mut scene, 64, 64, &camera, &lighting)
        .unwrap();
    let px = |rgba: &[u8]| rgba[(32 * 64 + 32) * 4..][..3].to_vec();
    assert!(
        px(&before)[0] > 150 && px(&before)[1] < 80,
        "{:?}",
        px(&before)
    );
    for (a, b) in px(&before).iter().zip(px(&after)) {
        assert!(
            (*a as i32 - b as i32).abs() <= 3,
            "{:?} then {:?}",
            px(&before),
            px(&after)
        );
    }
}

/// Instances added after the first frame beyond the room of the entries' buffers go into
/// bigger buffers with the entries already there kept: the old ones and the new ones are
/// drawn where they stand.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn instances_beyond_the_buffers_room_are_drawn_with_the_old_ones() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
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
    .expect("test renderer");
    let mut scene = renderer.new_scene();
    let quad = |renderer: &Renderer, scene: &mut Scene, x0: f32| {
        renderer.add_mesh(
            scene,
            &MeshData {
                positions: vec![
                    Vec3::new(x0, 4.0, -2.0),
                    Vec3::new(x0 + 4.0, 4.0, -2.0),
                    Vec3::new(x0 + 4.0, 4.0, 2.0),
                    Vec3::new(x0, 4.0, 2.0),
                ],
                normals: vec![-Vec3::Y; 4],
                uvs: vec![glam::Vec2::ZERO; 4],
                ranges: vec![(0, 6, 0)],
                indices: vec![0, 1, 2, 0, 2, 3],
                one_sided: false,
            },
        )
    };
    let red = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.9, 0.1, 0.1, 1.0],
        true,
    );
    let green = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.1, 0.9, 0.1, 1.0],
        true,
    );
    let left = quad(&renderer, &mut scene, -4.5);
    let right = quad(&renderer, &mut scene, 0.5);
    renderer.add_instance(&mut scene, left, DVec3::ZERO, Mat4::IDENTITY, vec![red]);
    let camera = Camera {
        position: DVec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 100.0,
    };
    let lighting = Lighting {
        shadows: false,
        fog_density: 0.0,
        sun_dir: -Vec3::Y,
        ..Default::default()
    };
    renderer
        .render_to_image(&mut scene, 64, 64, &camera, &lighting)
        .unwrap();
    let room = scene.model_buf.as_ref().unwrap().size();
    // far more than the buffers hold (hidden behind the camera), the last one in view
    for _ in 0..3000 {
        renderer.add_instance(
            &mut scene,
            right,
            DVec3::new(0.0, -50.0, 0.0),
            Mat4::IDENTITY,
            vec![green],
        );
    }
    renderer.add_instance(&mut scene, right, DVec3::ZERO, Mat4::IDENTITY, vec![green]);
    let rgba = renderer
        .render_to_image(&mut scene, 64, 64, &camera, &lighting)
        .unwrap();
    assert!(
        scene.model_buf.as_ref().unwrap().size() > room,
        "the buffers grew"
    );
    let px = |x: usize| rgba[(32 * 64 + x) * 4..][..3].to_vec();
    // (0.1 linear is 89 in sRGB)
    assert!(
        px(16)[0] > 200 && px(16)[1] < 120,
        "the old instance: {:?}",
        px(16)
    );
    assert!(
        px(48)[1] > 200 && px(48)[0] < 120,
        "the new one: {:?}",
        px(48)
    );
}
