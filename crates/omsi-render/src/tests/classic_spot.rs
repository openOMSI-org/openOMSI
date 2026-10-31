use super::*;

/// The declared source can lie inside the bus. Its ambient term must not light its
/// own cabin/body, even from an exterior camera or with the ray blocker disabled.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn classic_spot_does_not_light_its_own_vehicle() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    )).expect("test renderer");
    let mut scene = renderer.new_scene();
    let material = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0; 4], false);
    let mesh = renderer.add_mesh(&mut scene, &MeshData {
        positions: vec![Vec3::new(-0.6, 4.0, 0.5), Vec3::new(0.6, 4.0, 0.5), Vec3::new(0.6, 4.0, 1.5), Vec3::new(-0.6, 4.0, 1.5)],
        normals: vec![-Vec3::Y; 4], uvs: vec![glam::Vec2::ZERO; 4],
        ranges: vec![(0, 6, 0)], indices: vec![0, 1, 2, 0, 2, 3], one_sided: false,
    });
    let id = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
    renderer.set_roof(&mut scene, id, Some(3.0));
    scene.lights.push(PointLight {
        position: DVec3::new(0.0, 5.0, 1.0), radius: 150.0, direction: Vec3::Y,
        cone: [10f32.to_radians().cos(), 35f32.to_radians().cos()], mode: LightMode::Vanilla,
        d3d_spot: Some(D3dSpot { range: 100.0, source: DVec3::new(0.0, 0.0, 1.0), player: true }),
        ..Default::default()
    });
    let mut lighting = Lighting {
        enhanced: false, sun_intensity: 0.0, sun_color: Vec3::ZERO, secondary: Vec3::ZERO,
        ambient: Vec3::ZERO, shadows: false, fog_density: 0.0, detail: false,
        inside: Some((DVec3::ZERO, 0.0, [2.5, 12.0, 3.0, 0.0, 0.0, 1.5])),
        ..Default::default()
    };
    for classic in [true, false] {
        lighting.classic = classic;
        for (position, yaw) in [(DVec3::new(0.0, 2.0, 1.0), 0.0), (DVec3::new(0.0, 8.0, 1.0), 180.0)] {
            let camera = Camera { position, yaw, pitch: 0.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 100.0 };
            for vehicle in [true, false] {
                renderer.set_roof(&mut scene, id, vehicle.then_some(3.0));
                let mut pixels = Vec::new();
                for intensity in [0.0, 1.0] {
                    scene.lights[0].intensity = intensity;
                    let image = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
                    pixels.push(image[(32 * 64 + 32) * 4]);
                }
                if vehicle {
                    assert!(pixels[0].abs_diff(pixels[1]) <= 2,
                        "own vehicle lit by its headlight: {pixels:?}, classic={classic}, camera={position}");
                } else {
                    assert!(pixels[1] > pixels[0] + 20,
                        "non-vehicle receivers must still take the beam: {pixels:?}");
                }
            }
        }
    }
}

/// Preserve #3298's fixed-function parity: the pool is interpolated from lit vertices,
/// not evaluated per pixel. A beam missing every vertex must not light the face interior.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn classic_spot_preserves_vertex_lit_pool() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    )).expect("test renderer");
    let mut scene = renderer.new_scene();
    let material = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0; 4], false);
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 90.0, near: 0.1, far: 100.0 };
    let mut lighting = Lighting {
        enhanced: false, sun_intensity: 0.0, sun_color: Vec3::ZERO, secondary: Vec3::ZERO,
        ambient: Vec3::ZERO, shadows: false, fog_density: 0.0, detail: false,
        ..Default::default()
    };
    scene.lights.push(PointLight {
        position: DVec3::ZERO, radius: 150.0, direction: Vec3::Y,
        cone: [10f32.to_radians().cos(), 15f32.to_radians().cos()],
        mode: LightMode::Vanilla,
        d3d_spot: Some(D3dSpot { range: 100.0, source: DVec3::ZERO, player: true }),
        ..Default::default()
    });
    // The coarse mesh has no vertices inside the cone; subdividing adds a lit centre.
    // The resulting difference is intentional, matching the fixed-function road pool.
    for classic in [true, false] {
        lighting.classic = classic;
        let mut images = Vec::new();
        for cells in [1u32, 2] {
            let mut positions = Vec::new();
            let mut indices = Vec::new();
            for z in 0..=cells {
                for x in 0..=cells {
                    positions.push(Vec3::new(-8.0 + 16.0 * x as f32 / cells as f32, 10.0, -8.0 + 16.0 * z as f32 / cells as f32));
                }
            }
            for z in 0..cells {
                for x in 0..cells {
                    let a = z * (cells + 1) + x;
                    let b = a + cells + 1;
                    indices.extend_from_slice(&[a, a + 1, b + 1, a, b + 1, b]);
                }
            }
            let mesh = renderer.add_mesh(&mut scene, &MeshData {
                normals: vec![-Vec3::Y; positions.len()],
                uvs: vec![glam::Vec2::ZERO; positions.len()],
                ranges: vec![(0, indices.len() as u32, 0)],
                positions, indices, one_sided: false,
            });
            let id = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
            images.push(renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap());
            scene.instances[id].visible = false;
            scene.dirty = true;
        }
        let centre = (32 * 64 + 32) * 4;
        let outside_cone = (32 * 64 + 50) * 4;
        assert!(images[0][centre] < 5, "unlit vertices must give an unlit interior, classic={classic}");
        assert!(images[1][centre] > 30, "lit centre vertex must light the pool, classic={classic}");
        assert!(images[1][outside_cone] > 20, "vertex lighting must spread the pool, classic={classic}");
    }
}
