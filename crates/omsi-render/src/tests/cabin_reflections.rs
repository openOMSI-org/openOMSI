use super::*;

fn picture(renderer: &mut Renderer, scene: &mut Scene, camera: &Camera, lighting: &Lighting) -> Vec<u8> {
    // Let the capture cadence elapse between scripted scene changes.
    std::thread::sleep(std::time::Duration::from_millis(35));
    renderer.render_to_image(scene, 128, 128, camera, lighting).unwrap()
}

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

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn cabin_reflection_follows_brightness_and_distance() {
    falloff_case(1, false);
}

#[test]
#[ignore = "requires a ray-query graphics adapter; run with --ignored on a GPU host"]
fn cabin_reflection_follows_brightness_and_distance_with_ray_tracing() {
    falloff_case(4, true);
}

fn falloff_case(msaa: u32, ray_tracing: bool) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(&instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa, ray_tracing, ssao: false, shadow_size: 256, fxaa: false, light_shadows: 0, ..Default::default() })).unwrap();
    if ray_tracing { assert!(renderer.rt.is_some(), "this test requires ray queries"); }
    let camera = Camera { position: DVec3::new(0.0, 0.0, 1.5), yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 90.0, near: 0.1, far: 100.0 };
    let lighting = Lighting { enhanced: true, sun_dir: Vec3::new(0.0, 0.8, -0.6), sun_intensity: 0.0, ambient: Vec3::ZERO, secondary: Vec3::ZERO, night: 1.0, shadows: false, lamp_shadows: false, fog_density: 0.0,
        inside: Some((DVec3::ZERO, 0.0, [4.5, 30.0, 3.2, 0.0, 0.0, 1.5])), ..Default::default() };
    let centre = |rgba: &[u8]| -> f32 {
        (63..=65).flat_map(|y| (63..=65).map(move |x| rgba[(y * 128 + x) * 4] as f32)).sum::<f32>() / 9.0
    };
    let mut gains = Vec::new();
    for distance in [2.5, 6.0, 10.0, 14.0] {
        let mut scene = renderer.new_scene();
        let glass = renderer.add_material_extra(&mut scene, None, AlphaMode::Blend, [0.0, 0.0, 0.0, 0.08], false, None, None, None, None, [0.0; 3],
            MaterialExtra { glass: true, daylight_glass: true, no_z_write: true, ..Default::default() });
        let pane = panel(&renderer, &mut scene, 2.0, [-2.0, 2.0], [0.0, 3.0], glass, true);
        scene.instances[pane].slot_alpha[0] = 0.0;
        let outside = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 0.0, 0.0, 1.0], true);
        panel(&renderer, &mut scene, 20.0, [-50.0, 50.0], [-50.0, 50.0], outside, false);
        let red = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 0.01, 0.01, 1.0], false);
        // Change optical distance while keeping illumination and projected area constant.
        let y = 2.0 - distance;
        let half = (distance + 2.0) * 0.06;
        let seat = panel(&renderer, &mut scene, y, [-half, half], [1.5 - half, 1.5 + half], red, true);
        let light = renderer.alloc_interior_lights(&mut scene, 1);
        renderer.set_interior_lamps(&mut scene, seat, light, 1);
        let mut samples = Vec::new();
        for intensity in [0.0, 0.25, 0.5, 1.0] {
            renderer.set_interior_light(&mut scene, light, PointLight { position: DVec3::new(0.0, y as f64 + 1.0, 1.5), radius: 4.0, core: 1.0, intensity, color: [1.0; 3], ..Default::default() });
            samples.push(centre(&picture(&mut renderer, &mut scene, &camera, &lighting)));
        }
        if distance <= 6.0 {
            assert!(samples.windows(2).all(|p| p[1] > p[0] + 2.0), "brightness at {distance} m: {samples:?}");
        }
        gains.push(samples[3] - samples[0]);
        if distance == 2.5 {
            // Lamp attenuation is independent of the reflected image's distance fade.
            let mut lamp_gains = Vec::new();
            for lamp_distance in [3.0, 6.0] {
                renderer.set_interior_light(&mut scene, light, PointLight { position: DVec3::new(0.0, y as f64 + lamp_distance, 1.5), radius: 4.0, core: 1.0, intensity: 1.0, color: [1.0; 3], ..Default::default() });
                lamp_gains.push(centre(&picture(&mut renderer, &mut scene, &camera, &lighting)) - samples[0]);
            }
            assert!(lamp_gains[0] > 1.0 && lamp_gains[0] < gains[0] * 0.6 && lamp_gains[1] <= 1.0, "lamp distance: {lamp_gains:?}, nearby: {}", gains[0]);
        }
        // Emissive sources also fade with distance.
        scene.instances[seat].visible = false;
        scene.dirty = true;
        let emitter = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 0.01, 0.01, 1.0], true);
        panel(&renderer, &mut scene, y, [-half, half], [1.5 - half, 1.5 + half], emitter, true);
        let emitted = centre(&picture(&mut renderer, &mut scene, &camera, &lighting)) - samples[0];
        if distance == 2.5 { assert!(emitted > 5.0, "near emissive source: {emitted}"); }
        if distance == 14.0 { assert!(emitted <= 1.0, "distant emissive source: {emitted}"); }
    }
    eprintln!("cabin reflection gains at 2.5/6/10/14 m (RT={ray_tracing}): {gains:?}");
    assert!(gains[0] > gains[1] * 1.3 && gains[1] > gains[2] * 1.5, "front/middle/rear falloff: {gains:?}");
    assert!(gains[2] > 1.0 && gains[3] <= 1.0, "a faint rear reflection fades to none: {gains:?}");
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
    // Stock glass hides its environment map at night without removing the glass.
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
    let on = picture(&mut renderer, &mut scene, &camera, &lighting);
    renderer.set_interior_light(&mut scene, light, PointLight { intensity: 0.0, ..lamp });
    let off = picture(&mut renderer, &mut scene, &camera, &lighting);
    let pixel = |rgba: &[u8], x: usize, y: usize| -> [i32; 3] { std::array::from_fn(|c| rgba[(y * 128 + x) * 4 + c] as i32) };
    let diff = |x, y| -> [i32; 3] { let a = pixel(&on, x, y); let b = pixel(&off, x, y); std::array::from_fn(|c| a[c] - b[c]) };
    // Reflection places the seat and rail at y=6, beyond the pane at y=2.
    let seat_diff = diff(54, 70);
    let rail_diff = diff(71, 56);
    assert!(seat_diff[0] >= 8 && seat_diff[0] > seat_diff[2] * 2, "red seat: {seat_diff:?}");
    assert!(rail_diff[2] >= 8 && rail_diff[2] > rail_diff[0] * 2, "blue rail: {rail_diff:?}");
    assert!(diff(54, 53).iter().all(|c| c.abs() <= 3), "no seat above its actual silhouette");
    assert!(diff(90, 64).iter().all(|c| c.abs() <= 3), "clear street between cabin objects");
    // Two coincident authored pane layers must not double the reflected light.
    renderer.set_interior_light(&mut scene, light, PointLight { position: DVec3::new(0.0, 0.0, 2.0), radius: 8.0, core: 2.0, intensity: 1.0, color: [1.0; 3], ..Default::default() });
    let duplicate = panel(&renderer, &mut scene, 2.0, [-2.0, 2.0], [0.0, 3.0], glass, true);
    scene.instances[duplicate].slot_alpha[0] = 0.0;
    let twice = picture(&mut renderer, &mut scene, &camera, &lighting);
    for (x, y) in [(54, 70), (71, 56)] {
        let (a, b) = (pixel(&on, x, y), pixel(&twice, x, y));
        assert!((0..3).all(|c| (a[c] - b[c]).abs() <= 2), "a single reflection through layered glass: {a:?} vs {b:?}");
    }
    let mut disabled_lighting = lighting.clone();
    disabled_lighting.reflection_strength = 0.0;
    let depth_on = renderer.read_texture(&renderer.targets.ao.as_ref().unwrap().depth_tex, wgpu::TextureAspect::DepthOnly).unwrap();
    let disabled = picture(&mut renderer, &mut scene, &camera, &disabled_lighting);
    let depth_off = renderer.read_texture(&renderer.targets.ao.as_ref().unwrap().depth_tex, wgpu::TextureAspect::DepthOnly).unwrap();
    assert_eq!(depth_on, depth_off, "cabin composition must preserve scene depth for rain, puddles and temporal masks");
    assert!(pixel(&disabled, 54, 70)[0] <= pixel(&off, 54, 70)[0] + 2, "the existing reflection control disables the cabin capture too");
    // Turning a pane off with an ordinary script must still hide its reflection.
    scene.materials[glass].uniform.glow[2] = 0.0;
    let hidden = picture(&mut renderer, &mut scene, &camera, &lighting);
    assert!(pixel(&hidden, 54, 70)[0] <= pixel(&off, 54, 70)[0] + 2);
    // Texture holes transmit the street without reflecting the cabin.
    scene.instances[pane].visible = false;
    scene.instances[duplicate].visible = false;
    scene.dirty = true;
    let texture = renderer.add_texture(&mut scene, &omsi_texture::Image { width: 32, height: 1, rgba: (0..32).flat_map(|x| [255, 255, 255, if x < 16 { 255 } else { 0 }]).collect(), has_alpha: true }, false);
    let cut_glass = renderer.add_material_extra(&mut scene, Some(texture), AlphaMode::Blend, [0.0, 0.0, 0.0, 0.08], false, None, None, None, None, [0.0; 3],
        MaterialExtra { glass: true, daylight_glass: true, no_z_write: true, ..Default::default() });
    let cut = panel(&renderer, &mut scene, 2.0, [-2.0, 2.0], [0.0, 3.0], cut_glass, true);
    scene.instances[cut].slot_alpha[0] = 0.0;
    let hole = picture(&mut renderer, &mut scene, &camera, &lighting);
    assert!(pixel(&hole, 54, 70)[0] >= pixel(&hidden, 54, 70)[0] + seat_diff[0] - 2);
    assert!(pixel(&hole, 71, 56)[2] <= pixel(&hidden, 71, 56)[2] + 2, "no reflection in the texture hole");
    let black = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 0.0, 0.0, 1.0], true);
    panel(&renderer, &mut scene, 1.0, [-0.2, -0.1], [1.33, 1.46], black, true);
    let blocked = picture(&mut renderer, &mut scene, &camera, &lighting);
    assert!(pixel(&blocked, 54, 70)[0] <= pixel(&hole, 54, 70)[0] - seat_diff[0] + 2, "a foreground dashboard part occludes the reflection");
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn adjacent_panes_keep_separate_cabin_images() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(&instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ray_tracing: false, ssao: false, shadow_size: 256, fxaa: false, light_shadows: 0, ..Default::default() })).unwrap();
    let mut scene = renderer.new_scene();
    let glass = renderer.add_material_extra(&mut scene, None, AlphaMode::Blend, [0.0, 0.0, 0.0, 0.08], false, None, None, None, None, [0.0; 3],
        MaterialExtra { glass: true, daylight_glass: true, no_z_write: true, ..Default::default() });
    let left = panel(&renderer, &mut scene, 2.0, [-2.0, 0.0], [0.0, 3.0], glass, true);
    let right = panel(&renderer, &mut scene, 3.0, [0.0, 3.0], [0.0, 3.0], glass, true);
    let source = |rgba| omsi_texture::Image { width: 1, height: 1, rgba: Vec::from(rgba), has_alpha: false };
    let screen = renderer.add_texture(&mut scene, &source([255, 0, 0, 255]), false);
    let red = renderer.add_material(&mut scene, Some(screen), AlphaMode::Opaque, [1.0; 4], false);
    let blue = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 0.0, 1.0, 1.0], false);
    let red_panel = panel(&renderer, &mut scene, -2.0, [-1.6, -0.8], [1.1, 1.9], red, true);
    let blue_panel = panel(&renderer, &mut scene, -2.0, [0.8, 1.6], [1.1, 1.9], blue, true);
    let lamp = renderer.alloc_interior_lights(&mut scene, 1);
    renderer.set_interior_light(&mut scene, lamp, PointLight { position: DVec3::new(0.0, 0.0, 2.0), radius: 8.0, core: 2.0, intensity: 1.0, color: [1.0; 3], ..Default::default() });
    for i in [red_panel, blue_panel] { renderer.set_interior_lamps(&mut scene, i, lamp, 1); }
    let outside = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 0.0, 0.0, 1.0], true);
    panel(&renderer, &mut scene, 20.0, [-50.0, 50.0], [-50.0, 50.0], outside, false);
    let camera = Camera { position: DVec3::new(0.0, 0.0, 1.5), yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 90.0, near: 0.1, far: 100.0 };
    let lighting = Lighting { enhanced: true, sun_dir: Vec3::new(0.0, 0.8, -0.6), sun_intensity: 0.0, ambient: Vec3::ZERO, secondary: Vec3::ZERO, night: 1.0, shadows: false, lamp_shadows: false, fog_density: 0.0,
        inside: Some((DVec3::ZERO, 0.0, [6.0, 10.0, 3.2, 0.0, 0.0, 1.5])), ..Default::default() };
    let mut images = Vec::new();
    for (a, b) in [(true, false), (false, true), (true, true)] {
        renderer.set_params(&mut scene, left, &[0.0], a, &[]);
        renderer.set_params(&mut scene, right, &[0.0], b, &[]);
        images.push(renderer.render_to_image(&mut scene, 128, 128, &camera, &lighting).unwrap());
    }
    for (x, channel, isolated) in [(51, 0, 0), (74, 2, 1)] {
        let at = (64 * 128 + x) * 4 + channel;
        assert!(images[isolated][at] > 10, "the isolated pane reflects its colored source: {}", images[isolated][at]);
        assert!((images[2][at] as i32 - images[isolated][at] as i32).abs() <= 2, "array layers must match separate captures: {} vs {}", images[2][at], images[isolated][at]);
    }
    renderer.update_texture(&scene, screen, &source([0, 255, 0, 255]));
    std::thread::sleep(std::time::Duration::from_millis(35));
    let staggered = renderer.render_to_image(&mut scene, 128, 128, &camera, &lighting).unwrap();
    let blue_at = (64 * 128 + 74) * 4 + 2;
    assert!((staggered[blue_at] as i32 - images[2][blue_at] as i32).abs() <= 2, "updating a layer must preserve the other pane");
    let changed = renderer.render_to_image(&mut scene, 128, 128, &camera, &lighting).unwrap();
    let at = (64 * 128 + 51) * 4;
    assert!(changed[at] < images[2][at] / 2 && changed[at + 1] > changed[at] * 2, "a changed cabin display invalidates the cached picture: {:?} -> {:?}", &images[2][at..at+3], &changed[at..at+3]);
}
