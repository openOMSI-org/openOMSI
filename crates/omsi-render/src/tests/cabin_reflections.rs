use super::*;

fn panel(renderer: &Renderer, scene: &mut Scene, y: f32, x: [f32; 2], z: [f32; 2], material: usize, vehicle: bool) -> usize {
    let mesh = renderer.add_mesh(scene, &MeshData {
        positions: vec![Vec3::new(x[0], y, z[0]), Vec3::new(x[1], y, z[0]), Vec3::new(x[1], y, z[1]), Vec3::new(x[0], y, z[1])],
        normals: vec![if y < 0.0 { Vec3::Y } else { -Vec3::Y }; 4],
        uvs: vec![glam::Vec2::new(0.0, 1.0), glam::Vec2::ONE, glam::Vec2::new(1.0, 0.0), glam::Vec2::ZERO],
        indices: vec![0, 1, 2, 0, 2, 3], ranges: vec![(0, 6, 0)], one_sided: false,
    });
    let id = renderer.add_instance(scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
    if vehicle { scene.instances[id].roof = Some(3.0); }
    id
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn windscreen_reflects_lit_geometry_behind_the_camera() {
    reflection_case(1, false);
}

#[test]
#[ignore = "requires a ray-query graphics adapter; run with --ignored on a GPU host"]
fn windscreen_reflects_the_cabin_with_ray_tracing_and_msaa() {
    reflection_case(4, true);
}

fn reflection_case(msaa: u32, ray_tracing: bool) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(&instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa, ray_tracing, ssao: false, shadow_size: 256, fxaa: false, light_shadows: 0, ..Default::default() })).unwrap();
    if ray_tracing { assert!(renderer.rt.is_some(), "this test requires ray queries"); }
    let mut scene = renderer.new_scene();
    let glass = renderer.add_material_extra(&mut scene, None, AlphaMode::Blend, [0.0, 0.0, 0.0, 0.08], false, None, None, None, None, [0.0; 3],
        MaterialExtra { glass: true, daylight_glass: true, no_z_write: true, ..Default::default() });
    let pane = panel(&renderer, &mut scene, 2.0, [-2.0, 2.0], [0.0, 3.0], glass, true);
    // Stock glass fades its daylight environment at night. Its physical reflection
    // still exists, even though the ordinary blended draw is skipped altogether.
    scene.instances[pane].slot_alpha[0] = 0.0;
    let red = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 0.01, 0.01, 1.0], false);
    let blue = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.01, 0.01, 1.0, 1.0], false);
    let seat = panel(&renderer, &mut scene, -2.0, [-1.4, -0.4], [0.5, 1.5], red, true);
    let rail = panel(&renderer, &mut scene, -2.0, [0.5, 0.8], [0.2, 2.8], blue, true);
    // The driver's own figure is hidden in the direct view, but belongs in glass.
    scene.instances[seat].mirror_only = true;
    let light = renderer.alloc_interior_lights(&mut scene, 1);
    let lamp = PointLight { position: DVec3::new(0.0, 0.0, 2.0), radius: 8.0, core: 2.0, intensity: 1.0, color: [1.0; 3], ..Default::default() };
    renderer.set_interior_light(&mut scene, light, lamp.clone());
    for i in [seat, rail] { renderer.set_interior_lamps(&mut scene, i, light, 1); }
    let outside = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.03, 0.04, 0.03, 1.0], true);
    panel(&renderer, &mut scene, 20.0, [-50.0, 50.0], [-50.0, 50.0], outside, false);
    let camera = Camera { position: DVec3::new(0.0, 0.0, 1.5), yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 90.0, near: 0.1, far: 100.0 };
    let lighting = Lighting { enhanced: true, sun_dir: Vec3::new(0.0, 0.8, -0.6), sun_intensity: 0.0, ambient: Vec3::ZERO, secondary: Vec3::ZERO, night: 1.0, shadows: false, lamp_shadows: false, fog_density: 0.0,
        inside: Some((DVec3::ZERO, 0.0, [4.5, 10.0, 3.2, 0.0, 0.0, 1.5])), ..Default::default() };
    let on = renderer.render_to_image(&mut scene, 128, 128, &camera, &lighting).unwrap();
    renderer.set_interior_light(&mut scene, light, PointLight { intensity: 0.0, ..lamp });
    let off = renderer.render_to_image(&mut scene, 128, 128, &camera, &lighting).unwrap();
    let pixel = |rgba: &[u8], x: usize, y: usize| -> [i32; 3] { std::array::from_fn(|c| rgba[(y * 128 + x) * 4 + c] as i32) };
    let diff = |x, y| -> [i32; 3] { let a = pixel(&on, x, y); let b = pixel(&off, x, y); std::array::from_fn(|c| a[c] - b[c]) };
    // A reflected point is at y=6, not at the pane's y=2. The shapes therefore
    // occupy these different footprints: a low square seat and a long thin rail.
    let seat_diff = diff(54, 70);
    let rail_diff = diff(71, 56);
    assert!(seat_diff[0] > 10 && seat_diff[0] > seat_diff[2] * 2, "red seat: {seat_diff:?}");
    assert!(rail_diff[2] > 10 && rail_diff[2] > rail_diff[0] * 2, "blue rail: {rail_diff:?}");
    assert!(diff(54, 53).iter().all(|c| c.abs() <= 3), "no seat above its actual silhouette");
    assert!(diff(90, 64).iter().all(|c| c.abs() <= 3), "clear street between cabin objects");
    // Two coincident authored pane layers must not double the reflected light.
    renderer.set_interior_light(&mut scene, light, PointLight { position: DVec3::new(0.0, 0.0, 2.0), radius: 8.0, core: 2.0, intensity: 1.0, color: [1.0; 3], ..Default::default() });
    let duplicate = panel(&renderer, &mut scene, 2.0, [-2.0, 2.0], [0.0, 3.0], glass, true);
    scene.instances[duplicate].slot_alpha[0] = 0.0;
    let twice = renderer.render_to_image(&mut scene, 128, 128, &camera, &lighting).unwrap();
    for (x, y) in [(54, 70), (71, 56)] {
        let (a, b) = (pixel(&on, x, y), pixel(&twice, x, y));
        assert!((0..3).all(|c| (a[c] - b[c]).abs() <= 2), "a single reflection through layered glass: {a:?} vs {b:?}");
    }
    let mut disabled_lighting = lighting.clone();
    disabled_lighting.reflection_strength = 0.0;
    let disabled = renderer.render_to_image(&mut scene, 128, 128, &camera, &disabled_lighting).unwrap();
    assert!(pixel(&disabled, 54, 70)[0] < pixel(&on, 54, 70)[0] - 10, "the existing reflection control disables the cabin capture too");
    // Turning a pane off with an ordinary script must still hide its reflection.
    scene.materials[glass].uniform.glow[2] = 0.0;
    let hidden = renderer.render_to_image(&mut scene, 128, 128, &camera, &lighting).unwrap();
    assert!(pixel(&hidden, 54, 70)[0] <= pixel(&off, 54, 70)[0] + 2);
    // A transparent texture hole on the right transmits the street without a
    // reflection, while the left half still shows the seat's actual silhouette.
    scene.instances[pane].visible = false;
    scene.instances[duplicate].visible = false;
    scene.dirty = true;
    let texture = renderer.add_texture(&mut scene, &omsi_texture::Image { width: 32, height: 1, rgba: (0..32).flat_map(|x| [255, 255, 255, if x < 16 { 255 } else { 0 }]).collect(), has_alpha: true }, false);
    let cut_glass = renderer.add_material_extra(&mut scene, Some(texture), AlphaMode::Blend, [0.0, 0.0, 0.0, 0.08], false, None, None, None, None, [0.0; 3],
        MaterialExtra { glass: true, daylight_glass: true, no_z_write: true, ..Default::default() });
    let cut = panel(&renderer, &mut scene, 2.0, [-2.0, 2.0], [0.0, 3.0], cut_glass, true);
    scene.instances[cut].slot_alpha[0] = 0.0;
    let hole = renderer.render_to_image(&mut scene, 128, 128, &camera, &lighting).unwrap();
    assert!(pixel(&hole, 54, 70)[0] > pixel(&hidden, 54, 70)[0] + 10);
    assert!(pixel(&hole, 71, 56)[2] <= pixel(&hidden, 71, 56)[2] + 2, "no reflection in the texture hole");
    let black = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 0.0, 0.0, 1.0], true);
    panel(&renderer, &mut scene, 1.0, [-0.2, -0.1], [1.33, 1.46], black, true);
    let blocked = renderer.render_to_image(&mut scene, 128, 128, &camera, &lighting).unwrap();
    assert!(pixel(&blocked, 54, 70)[0] < pixel(&hole, 54, 70)[0] - 10, "a foreground dashboard part occludes the reflection");
}
