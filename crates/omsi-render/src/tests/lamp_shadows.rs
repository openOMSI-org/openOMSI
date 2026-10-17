use super::*;

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

fn shadowed(renderer: &Renderer, scene: &mut Scene, y: f32) -> Vec<i32> {
    let (_, lamps) = renderer.prepare_lights(scene, Vec3::new(0.0, y, 1.5), true, true, None);
    let mut ys: Vec<i32> = lamps.iter().map(|l| l.position.y.round() as i32).collect();
    ys.sort();
    ys
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_mirror_picture_leaves_the_street_lamps_their_shadow_maps() {
    let renderer = renderer();
    let mut scene = renderer.new_scene();
    scene.lights = (0..5)
        .map(|k| PointLight {
            position: DVec3::new(0.0, k as f64 * 10.0, 6.0),
            radius: 20.0,
            color: [1.0, 0.8, 0.5],
            core: 3.0,
            housed: true,
            mode: LightMode::Both,
            ..Default::default()
        })
        .collect();
    assert_eq!(shadowed(&renderer, &mut scene, 15.0), vec![0, 10, 20, 30]);
    assert_eq!(shadowed(&renderer, &mut scene, 21.0), vec![0, 10, 20, 30]);
    let _ = renderer.prepare_lights(&mut scene, Vec3::new(0.0, 21.0, 1.5), false, false, None);
    assert_eq!(shadowed(&renderer, &mut scene, 21.0), vec![0, 10, 20, 30]);
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn the_players_vehicle_keeps_a_lamp_behind_it_off_the_street_ahead() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let grey = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.8, 0.8, 0.8, 1.0],
        false,
    );
    let mesh = renderer.add_mesh(
        &mut scene,
        &MeshData {
            positions: vec![
                Vec3::new(-4.0, 4.0, -4.0),
                Vec3::new(4.0, 4.0, -4.0),
                Vec3::new(4.0, 4.0, 4.0),
                Vec3::new(-4.0, 4.0, 4.0),
            ],
            normals: vec![-Vec3::Y; 4],
            uvs: vec![glam::Vec2::ZERO; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
            ranges: vec![(0, 6, 0)],
            one_sided: false,
        },
    );
    renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    scene.lights = vec![PointLight {
        position: DVec3::new(0.0, -6.0, 0.0),
        radius: 30.0,
        intensity: 1.0,
        core: 4.0,
        ..Default::default()
    }];
    let camera = Camera {
        position: DVec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 100.0,
    };
    let bus = Some((
        DVec3::new(-2.0, 1.0, 0.0),
        0.0,
        [4.5, 3.5, 3.0, 0.0, 0.0, 0.0],
    ));
    for (classic, enhanced) in [(true, false), (false, false), (false, true)] {
        let lighting = Lighting {
            classic,
            enhanced,
            sun_dir: -Vec3::Z,
            sun_intensity: 0.0,
            ambient: Vec3::ZERO,
            secondary: Vec3::ZERO,
            shadows: false,
            fog_density: 0.0,
            night: 1.0,
            inside: bus,
            ..Default::default()
        };
        let rgba = renderer
            .render_to_image(&mut scene, 64, 64, &camera, &lighting)
            .unwrap();
        let at = |x: usize| {
            rgba[(32 * 64 + x) * 4..][..3]
                .iter()
                .map(|&c| c as u32)
                .sum::<u32>()
        };
        let (behind, open) = (at(16), at(47));
        assert!(
            open > 60 && behind * 4 < open,
            "classic {classic}, enhanced {enhanced}: behind the vehicle {behind}, beside it {open}"
        );
    }
}
