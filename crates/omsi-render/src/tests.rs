use super::*;

mod freetex_reload;

/// The night meter takes the street the picture shows: from 30 m over a lot lit by a
/// grid of street lamps, looking down at it, the eye adapts to the lit lot - not, as
/// when it sampled a "ground" 1.6 m under the camera (in the air, out of every lamp's
/// reach), to a moonless night that lifted the lot's light to white.
#[test]
fn the_night_meter_sees_a_lamp_lit_lot_from_above() {
    let mut lights = Vec::new();
    for i in 0..4 {
        for j in 0..4 {
            lights.push(PointLight {
                position: DVec3::new(i as f64 * 12.0 - 18.0, j as f64 * 12.0 - 18.0, 8.0),
                radius: 24.0,
                color: [1.0, 0.8, 0.5],
                intensity: 1.0,
                core: 3.0,
                housed: true,
                mode: LightMode::Both,
                ..Default::default()
            });
        }
    }
    let cam = Camera { position: DVec3::new(0.0, -35.0, 30.0), yaw: 0.0, pitch: -40.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 1000.0 };
    let cam_rel = cam.position.as_vec3();
    let seen = view_lamp_light(&lights, DVec3::ZERO, cam_rel, &cam, 16.0 / 9.0, Some(0.0), &|_| 1.0);
    let in_air = view_lamp_light(&lights, DVec3::ZERO, cam_rel, &cam, 16.0 / 9.0, None, &|_| 1.0);
    // (the lot's ground: some 10-30 lux under the lamps, 1e-3 .. 3e-3, less between and
    // round them - a log-average of some 10 lux; a moonless night is 2e-6)
    assert!(seen > 5e-5, "{seen}");
    assert!(in_air < seen * 0.2, "{in_air} vs {seen}");
    // and the lamps seen from beside the lot, at eye height, veil the eye as well
    let eye = Camera { position: DVec3::new(0.0, -40.0, 1.7), pitch: 0.0, ..cam };
    assert!(glare_veil(&lights, DVec3::ZERO, eye.position.as_vec3(), eye.forward(), &|_| 1.0) > 1e-5);
    assert_eq!(glare_veil(&lights, DVec3::ZERO, eye.position.as_vec3(), eye.forward(), &|_| 0.0), 0.0);
}

/// A textured material can have black diffuse but white ambient (depot interiors).
/// Enhanced must not turn it into a black surface or silently replace its diffuse.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn enhanced_respects_material_ambient() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    )).expect("test renderer");
    let mut scene = renderer.new_scene();
    let mesh = renderer.add_mesh(&mut scene, &MeshData {
        positions: vec![Vec3::new(-8.0, 4.0, -8.0), Vec3::new(8.0, 4.0, -8.0), Vec3::new(8.0, 4.0, 8.0), Vec3::new(-8.0, 4.0, 8.0)],
        normals: vec![-Vec3::Y; 4],
        uvs: vec![glam::Vec2::ZERO; 4],
        ranges: vec![(0, 6, 0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        one_sided: false,
    });
    let texture = renderer.add_texture(&mut scene, &omsi_texture::Image {
        width: 1, height: 1, rgba: vec![180, 180, 180, 255], has_alpha: true,
    }, false);
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 90.0, near: 0.1, far: 100.0 };
    let lighting = Lighting { enhanced: true, shadows: false, fog_density: 0.0, sun_dir: -Vec3::Y, ..Default::default() };
    let mut pixels = Vec::new();
    for ambient in [[0.0; 3], [1.0; 3], [1.0, 0.0, 0.0]] {
        let material = renderer.add_material_extra(&mut scene, Some(texture), AlphaMode::Blend,
            [0.0, 0.0, 0.0, 1.0], false, None, None, None, None, [0.0; 3],
            MaterialExtra { ambient: Some(ambient), ..Default::default() });
        let id = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
        let rgba = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
        pixels.push(rgba[(32 * 64 + 32) * 4..][..3].to_vec());
        scene.instances[id].visible = false;
    }
    assert!(pixels[1].iter().map(|&v| v as u32).sum::<u32>() > pixels[0].iter().map(|&v| v as u32).sum::<u32>() + 60,
        "white ambient must illuminate black diffuse: {pixels:?}");
    assert!(pixels[2][0] > pixels[2][1].saturating_add(20) && pixels[2][0] > pixels[2][2].saturating_add(20),
        "the material's ambient tint must be preserved: {pixels:?}");
}

/// A road must cover flush terrain without covering a building floor above it.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn surface_bias_keeps_raised_floors_visible_at_a_distance() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    )).expect("test renderer");
    let mut scene = renderer.new_scene();
    let mesh = renderer.add_mesh(&mut scene, &MeshData {
        positions: vec![Vec3::new(-1000.0, -1000.0, 0.0), Vec3::new(1000.0, -1000.0, 0.0), Vec3::new(1000.0, 1000.0, 0.0), Vec3::new(-1000.0, 1000.0, 0.0)],
        normals: vec![Vec3::Z; 4], uvs: vec![glam::Vec2::ZERO; 4],
        ranges: vec![(0, 6, 0)], indices: vec![0, 1, 2, 0, 2, 3], one_sided: false,
    });
    let blue = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 0.0, 1.0, 1.0], true);
    let green = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 1.0, 0.0, 1.0], true);
    let red = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 0.0, 0.0, 1.0], true);
    let ground = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![blue]);
    scene.instances[ground].render_phase = RenderPhase::Terrain;
    let road = renderer.add_surface_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![green]);
    scene.instances[road].render_phase = RenderPhase::Spline;
    let floor = renderer.add_instance(&mut scene, mesh, DVec3::new(0.0, 0.0, 0.03), Mat4::IDENTITY, vec![red]);
    scene.instances[floor].render_phase = RenderPhase::AfterVehicles;
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    for (pitch, fov) in [(-5.0, 60.0), (-5.0, 20.0), (-30.0, 60.0)] {
        let camera = Camera { position: DVec3::new(0.0, 0.0, 5.0), yaw: 0.0, pitch, roll: 0.0, fov_deg: fov, near: 0.1, far: 2000.0 };
        for show_floor in [false, true] {
            scene.instances[floor].visible = show_floor;
            scene.dirty = true;
            let rgba = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
            let pixel = &rgba[(32 * 64 + 32) * 4..][..3];
            let wanted = if show_floor { 0 } else { 1 };
            assert!(pixel[wanted] > 200 && pixel[1 - wanted] < 20 && pixel[2] < 20,
                "pitch {pitch}, fov {fov}, floor {show_floor}: {pixel:?}");
        }
    }
}

/// An untextured slot (a Blender export's Base Color alone) keeps its colour in Enhanced:
/// the white ambient Omsi.exe gives every o3d slot turned it white under the sky (#1737).
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn enhanced_untextured_keeps_its_colour() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    )).expect("test renderer");
    let mut scene = renderer.new_scene();
    let mesh = renderer.add_mesh(&mut scene, &MeshData {
        positions: vec![Vec3::new(-8.0, 4.0, -8.0), Vec3::new(8.0, 4.0, -8.0), Vec3::new(8.0, 4.0, 8.0), Vec3::new(-8.0, 4.0, 8.0)],
        normals: vec![-Vec3::Y; 4],
        uvs: vec![glam::Vec2::ZERO; 4],
        ranges: vec![(0, 6, 0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        one_sided: false,
    });
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 90.0, near: 0.1, far: 100.0 };
    let lighting = Lighting { enhanced: true, shadows: false, fog_density: 0.0, sun_dir: -Vec3::Y, ..Default::default() };
    let material = renderer.add_material_extra(&mut scene, None, AlphaMode::Opaque,
        [0.8, 0.05, 0.05, 1.0], false, None, None, None, None, [0.0; 3],
        MaterialExtra { ambient: Some([1.0; 3]), ..Default::default() });
    renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
    let rgba = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
    let px = &rgba[(32 * 64 + 32) * 4..][..3];
    assert!(px[0] as u32 > px[1] as u32 * 2 + 10 && px[0] as u32 > px[2] as u32 * 2 + 10, "an untextured red slot must stay red: {px:?}");
}

/// Overlays drawn texel for pixel: onto whole pixels, their size kept.
#[test]
fn overlays_land_on_whole_pixels() {
    // (a 120 x 26 text a quarter and a half pixel off: moved, the same size)
    assert_eq!(snap_rect([25.25, 40.5, 145.25, 66.5]), [25.0, 41.0, 145.0, 67.0]);
    assert_eq!(snap_rect([10.0, 20.0, 30.0, 40.0]), [10.0, 20.0, 30.0, 40.0]);
    // (left of the window as well: the same size)
    assert_eq!(snap_rect([-0.5, -2.5, 19.5, 7.5]), [0.0, -2.0, 20.0, 8.0]);
    // (a separator 0.6 px high stays a line)
    let line = snap_rect([16.0, 100.3, 300.0, 100.9]);
    assert_eq!(line[3] - line[1], 1.0);
    // (an empty rectangle stays empty)
    assert_eq!(snap_rect([5.2, 5.2, 5.2, 5.2]), [5.0, 5.0, 5.0, 5.0]);
}

/// The enhanced street is lit by the sun that gets through the weather: none from
/// under an overcast deck, next to none through a dense fog (which glowed white round
/// the sun), nearly all of it on a clear or a hazy summer's day (#1106).
#[test]
fn enhanced_sun_does_not_shine_through_overcast_or_dense_fog() {
    // an evening sun 27 degrees high, a weather's visibility as the app turns it into
    // a density (`lights::lighting_from`), the cover and the sun `apply_weather` leaves
    let light = |visibility: f32, cloud_density: f32, sun_intensity: f32| Lighting {
        sun_dir: Vec3::new(-0.89, 0.0, 0.454),
        fog_density: (2.3 / visibility).max(0.00005),
        cloud_density,
        sun_intensity,
        ..Default::default()
    };
    let reaching = |l: &Lighting| enhanced_sky_input(l, 1.0).0.sun_visibility;
    // #CAVOK, a cumulus sky, Sommerlich's summer haze of 2 km
    assert!(reaching(&light(50_000.0, 0.0, 1.0)) > 0.99);
    assert!(reaching(&light(50_000.0, 0.75, 0.54)) > 0.53);
    assert!(reaching(&light(2_000.0, 0.35, 1.0)) > 0.4);
    // Bodennebel's 75 m, Schmuddelwetter's 200 m: the sun is gone
    assert!(reaching(&light(75.0, 0.0, 1.0)) < 1e-4);
    assert!(reaching(&light(200.0, 0.0, 1.0)) < 1e-3);
    // an Overcast cloud type (the 15 % of the sun `apply_weather` keeps): gone from the
    // street, while the clouds keep their light from above
    let (input, clouds) = enhanced_sky_input(&light(50_000.0, 1.0, 0.15), 1.0);
    assert_eq!(input.sun_visibility, 0.0);
    assert!((clouds - 0.15).abs() < 1e-6);
}

#[test]
#[ignore = "requires a graphics adapter; renders terrain and foliage lighting"]
fn enhanced_masked_and_uncut_ground_share_lighting() {
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
    let mut texture = |rgba: [u8; 4]| {
        renderer.add_texture(
            &mut scene,
            &omsi_texture::Image {
                width: 1,
                height: 1,
                rgba: rgba.to_vec(),
                has_alpha: true,
            },
            false,
        )
    };
    let grey = texture([100, 100, 100, 255]);
    let opaque = texture([255; 4]);
    let transparent = texture([100, 100, 100, 0]);
    let masked = renderer.add_terrain_material(
        &mut scene,
        Some(grey),
        Some(opaque),
        None,
        1.0,
        None,
        0.0,
    );
    let uncut =
        renderer.add_terrain_material(&mut scene, Some(grey), None, None, 1.0, None, 0.0);
    let cut = renderer.add_terrain_material(
        &mut scene,
        Some(grey),
        Some(transparent),
        None,
        1.0,
        None,
        0.0,
    );
    let foliage =
        renderer.add_material(&mut scene, Some(grey), AlphaMode::Test, [1.0; 4], false);
    let cut_foliage = renderer.add_material(
        &mut scene,
        Some(transparent),
        AlphaMode::Test,
        [1.0; 4],
        false,
    );
    let backdrop = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [1.0, 0.0, 0.0, 1.0],
        true,
    );
    let mut quad = |left: f32, right: f32, z: f32, material| {
        let mesh = renderer.add_mesh(
            &mut scene,
            &MeshData {
                positions: vec![
                    Vec3::new(left, -5.0, z),
                    Vec3::new(right, -5.0, z),
                    Vec3::new(right, 5.0, z),
                    Vec3::new(left, 5.0, z),
                ],
                normals: vec![Vec3::Z; 4],
                uvs: vec![glam::Vec2::splat(0.5); 4],
                indices: vec![0, 1, 2, 0, 2, 3],
                ranges: vec![(0, 6, 0)],
                one_sided: false,
            },
        );
        renderer.add_instance(
            &mut scene,
            mesh,
            DVec3::ZERO,
            Mat4::IDENTITY,
            vec![material],
        )
    };
    // Symmetric samples in the same image share exposure, view and lamp distances.
    quad(-6.0, 6.0, -1.0, backdrop);
    let ground = quad(-5.0, -0.5, 0.0, masked);
    let mapped = quad(0.5, 5.0, 0.0, uncut);
    scene.instances[ground].render_phase = RenderPhase::Terrain;
    scene.instances[mapped].render_phase = RenderPhase::Spline;
    scene.instances[mapped].surface = true;
    let camera = Camera {
        position: DVec3::new(0.0, -0.105, 6.0),
        yaw: 0.0,
        pitch: -89.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 100.0,
    };
    let day = Lighting {
        enhanced: true,
        sun_dir: Vec3::Z,
        sun_intensity: 1.0,
        shadows: false,
        detail: false,
        fog_density: 0.0,
        ..Default::default()
    };
    let night = Lighting {
        sun_dir: -Vec3::Z,
        sun_intensity: 0.0,
        night: 1.0,
        ..day.clone()
    };
    let pixel = |rgba: &[u8], x: usize| -> [u8; 3] {
        rgba[(32 * 64 + x) * 4..(32 * 64 + x) * 4 + 3]
            .try_into()
            .unwrap()
    };
    for (name, lighting) in [("sun", &day), ("lamp", &night)] {
        scene.lights = if name == "lamp" {
            vec![PointLight {
                position: DVec3::new(0.0, 0.0, 7.0),
                radius: 20.0,
                core: 10.0,
                intensity: 1.0,
                ..Default::default()
            }]
        } else {
            Vec::new()
        };
        let rgba = renderer
            .render_to_image(&mut scene, 64, 64, &camera, lighting)
            .unwrap();
        let (a, b) = (pixel(&rgba, 16), pixel(&rgba, 47));
        assert!(
            a.iter().all(|v| *v > 10 && *v < 245),
            "lit, unclipped {name}: {a:?}"
        );
        assert!(
            a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2),
            "masked terrain and uncut mapped ground differ under {name}: {a:?} / {b:?}"
        );
    }
    // A road cut still reveals the geometry beneath it.
    renderer.set_material(&mut scene, ground, 0, cut);
    let rgba = renderer
        .render_to_image(&mut scene, 64, 64, &camera, &night)
        .unwrap();
    let a = pixel(&rgba, 16);
    assert!(
        a[0] > a[1] + 30 && a[0] > a[2] + 30,
        "road cut must reveal red: {a:?}"
    );

    // Real foliage still scatters light arriving from behind its normal; ground does not.
    renderer.set_material(&mut scene, ground, 0, masked);
    renderer.set_material(&mut scene, mapped, 0, foliage);
    scene.lights[0].position.z = -4.0;
    renderer
        .render_to_image(&mut scene, 64, 64, &camera, &night)
        .unwrap();
    let rgba = renderer
        .render_to_image(&mut scene, 64, 64, &camera, &night)
        .unwrap();
    let (a, b) = (pixel(&rgba, 16), pixel(&rgba, 47));
    assert!(
        b[1] > a[1] + 8,
        "foliage retains backlighting: ground {a:?}, foliage {b:?}"
    );
    renderer.set_material(&mut scene, mapped, 0, cut_foliage);
    let rgba = renderer
        .render_to_image(&mut scene, 64, 64, &camera, &night)
        .unwrap();
    let b = pixel(&rgba, 47);
    assert!(
        b[0] > b[1] + 30 && b[0] > b[2] + 30,
        "foliage cutout must reveal red: {b:?}"
    );
}

#[test]
fn a_mesh_page_hands_freed_space_to_the_next_mesh_and_joins_its_holes() {
    let (mut free, mut top) = (Vec::new(), 0u64);
    let a = page_take(&mut free, &mut top, 1000, 100).unwrap();
    let b = page_take(&mut free, &mut top, 1000, 200).unwrap();
    let c = page_take(&mut free, &mut top, 1000, 300).unwrap();
    assert_eq!((a, b, c, top), (0, 100, 300, 600));
    page_give(&mut free, &mut top, b, 200);
    assert_eq!(free, vec![(100, 200)]);
    assert_eq!(page_take(&mut free, &mut top, 1000, 150), Some(100));
    assert_eq!(free, vec![(250, 50)]);
    page_give(&mut free, &mut top, a, 100);
    page_give(&mut free, &mut top, 100, 150);
    assert_eq!(free, vec![(0, 300)]);
    page_give(&mut free, &mut top, c, 300);
    assert_eq!((free.len(), top), (0, 0));
    assert_eq!(page_take(&mut free, &mut top, 1000, 1001), None);
}

/// A dedicated server's renderer (wgpu's no-op device): its meshes take their place in the
/// pages as on a GPU, with their bounds, but the pages hold no bytes of them.
#[test]
fn on_no_device_the_mesh_pages_hold_places_but_no_bytes() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let instance = wgpu::Instance::new(descriptor);
    let renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, shadow_size: 1024, ..Default::default() },
    )).expect("no-op renderer");
    let mut scene = renderer.new_scene();
    let mesh = |n: usize, x: f32| {
        let positions: Vec<Vec3> = (0..n).map(|i| Vec3::new(x + i as f32, 0.0, 0.0)).collect();
        let indices: Vec<u32> = (0..n as u32 / 3 * 3).collect();
        MeshData { normals: vec![Vec3::Z; n], uvs: vec![glam::Vec2::ZERO; n], ranges: vec![(0, indices.len() as u32, 0)], positions, indices, ..Default::default() }
    };
    let ids: Vec<MeshId> = (0..2000).map(|k| renderer.add_mesh(&mut scene, &mesh(300, k as f32))).collect();
    let big = renderer.add_mesh(&mut scene, &mesh(400_000, 0.0));
    let pages: Vec<PreparedMesh> = prepare_meshes(&renderer.device, &renderer.queue, &[&mesh(900, 0.0), &mesh(600, 0.0)], renderer.mesh_pages());
    for m in pages {
        renderer.add_prepared_mesh(&mut scene, m);
    }
    // every mesh in its own place, with its own bounds
    let m0 = &scene.meshes[ids[0]];
    let m1 = &scene.meshes[ids[1]];
    assert!(m0.page != m1.page || m0.vertex_offset + m0.vertex_bytes <= m1.vertex_offset);
    assert_eq!(m1.vertex_bytes, 300 * std::mem::size_of::<Vertex>() as u64);
    assert!((m1.bounds_center.x - (1.0 + 149.5)).abs() < 1e-3);
    // and the buffers a token: 2000 meshes of 300 vertices did not take 19 MB
    assert!(scene.mesh_pages.iter().all(|p| p.vertex.size() == 32 && p.index.size() == 32));
    assert_eq!(scene.meshes[big].vertex_buf.size(), 32);
    // a freed mesh's place is taken by the next
    let at = (scene.meshes[ids[5]].page, scene.meshes[ids[5]].vertex_offset);
    renderer.free_mesh(&mut scene, ids[5]);
    let again = renderer.add_mesh(&mut scene, &mesh(300, 0.0));
    assert_eq!((scene.meshes[again].page, scene.meshes[again].vertex_offset), at);
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn meshes_coming_and_going_tile_by_tile_keep_the_pages_from_growing() {
    let adapter = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let renderer = pollster::block_on(Renderer::new_with(
        &adapter, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, shadow_size: 1024, ..Default::default() },
    )).expect("test renderer");
    if !renderer.mesh_pages() {
        return;
    }
    let mut scene = renderer.new_scene();
    let mesh = |n: usize| {
        let positions: Vec<Vec3> = (0..n).map(|i| Vec3::new(i as f32, (i % 7) as f32, 0.0)).collect();
        let indices: Vec<u32> = (0..n as u32 / 3 * 3).collect();
        MeshData { normals: vec![Vec3::Z; n], uvs: vec![glam::Vec2::ZERO; n], ranges: vec![(0, indices.len() as u32, 0)], positions, indices, ..Default::default() }
    };
    // tiles of a few thousand meshes of every size, loaded and unloaded in no order, as
    // a long drive across a map brings them; the scene's pages must not keep growing
    let mut tiles: Vec<Vec<MeshId>> = Vec::new();
    let mut seed = 0x1234_5678u64;
    let mut rand = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut most = 0;
    for round in 0..300 {
        let tile: Vec<MeshId> = (0..400).map(|_| renderer.add_mesh(&mut scene, &mesh(3 + (rand() % 600) as usize))).collect();
        tiles.push(tile);
        if tiles.len() > 12 {
            let gone = tiles.remove((rand() % tiles.len() as u64) as usize);
            for id in gone {
                renderer.free_mesh(&mut scene, id);
            }
        }
        if round == 50 {
            most = scene.mesh_pages.len();
        }
    }
    assert!(scene.mesh_pages.len() <= most + 1, "the pages grew from {most} to {} over the drive", scene.mesh_pages.len());
    let held = scene.mesh_page_bytes();
    let used: u64 = scene.meshes.iter().filter(|m| m.page != u32::MAX).map(|m| m.vertex_bytes + m.index_bytes).sum();
    assert!(held < used * 3, "{held} bytes of pages held for {used} in use");
    for t in &tiles {
        for &id in t {
            let m = &scene.meshes[id];
            assert_eq!(m.vertex_offset % std::mem::size_of::<Vertex>() as u64, 0);
            assert!(m.vertex_offset + m.vertex_bytes <= m.vertex_buf.size());
            assert!(m.first_index as u64 * 4 + m.index_bytes <= m.index_buf.size());
        }
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn msaa_depth_prepass_preserves_layered_surfaces() {
    fn quad(renderer: &Renderer, scene: &mut Scene, z: f32, mat: MaterialId) -> usize {
        let mesh = renderer.add_mesh(
            scene,
            &MeshData {
                positions: vec![
                    Vec3::new(-4.0, -4.0, z),
                    Vec3::new(4.0, -4.0, z),
                    Vec3::new(4.0, 4.0, z),
                    Vec3::new(-4.0, 4.0, z),
                ],
                normals: vec![Vec3::Z; 4],
                uvs: vec![
                    glam::Vec2::ZERO,
                    glam::Vec2::X,
                    glam::Vec2::ONE,
                    glam::Vec2::Y,
                ],
                indices: vec![0, 1, 2, 0, 2, 3],
                ranges: vec![(0, 6, 0)],
                one_sided: false,
            },
        );
        renderer.add_surface_instance(scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![mat])
    }
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: 4,
            ssao: true,
            fxaa: false,
            render_scale: 1.0,
            ..Default::default()
        },
    ))
    .expect("test renderer");
    if renderer.options.msaa <= 1 {
        return; // This adapter cannot exercise a multisampled depth buffer.
    }
    assert!(renderer.prepass_msaa_pipelines.is_some());
    let mut scene = renderer.new_scene();
    let mask = renderer.add_texture(
        &mut scene,
        &omsi_texture::Image {
            width: 7,
            height: 1,
            rgba: [0, 0, 112, 128, 160, 255, 255]
                .into_iter()
                .flat_map(|a| [255, 255, 255, a])
                .collect(),
            has_alpha: true,
        },
        false,
    );
    let base = renderer.add_terrain_material(&mut scene, None, None, None, 1.0, None, 1.0);
    let ground = quad(&renderer, &mut scene, 0.0, base);
    scene.instances[ground].render_phase = RenderPhase::Terrain;
    let paint =
        renderer.add_terrain_layer_material(&mut scene, None, mask, None, 1.0, None, 1.0);
    let layer = quad(&renderer, &mut scene, 0.0, paint);
    scene.instances[layer].ground_layer = true;
    scene.instances[layer].render_phase = RenderPhase::Terrain;
    let road_mat = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 0.0, 1.0, 1.0],
        true,
    );
    let road = quad(&renderer, &mut scene, 0.02, road_mat);
    scene.instances[road].render_phase = RenderPhase::Spline;
    let cutout = renderer.add_material(
        &mut scene,
        Some(mask),
        AlphaMode::Test,
        [0.0, 1.0, 0.0, 1.0],
        false,
    );
    let fence = quad(&renderer, &mut scene, 0.3, cutout);
    scene.instances[fence].render_phase = RenderPhase::Normal;
    let blend = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Blend,
        [1.0, 0.0, 0.0, 0.25],
        false,
    );
    let pane = quad(&renderer, &mut scene, 0.5, blend);
    scene.instances[pane].render_phase = RenderPhase::Normal;
    // A small excavation in the corner activates the deferred prepass without
    // covering the road, cutout and glass samples checked below.
    let floor = quad(&renderer, &mut scene, -0.1, road_mat);
    scene.instances[floor].presurface = true;
    scene.instances[floor].transform = Mat4::from_translation(Vec3::new(7.0, 7.0, 0.0));
    let clear = renderer.add_texture(&mut scene, &omsi_texture::Image {
        width: 1, height: 1, rgba: vec![255, 255, 255, 0], has_alpha: true,
    }, false);
    let cover_material = renderer.add_material(&mut scene, Some(clear), AlphaMode::Blend, [1.0; 4], true);
    let cover = quad(&renderer, &mut scene, 0.1, cover_material);
    scene.instances[cover].presurface = true;
    scene.instances[cover].transform = Mat4::from_translation(Vec3::new(7.0, 7.0, 0.0));
    let camera = Camera {
        position: DVec3::new(0.0, -0.105, 6.0),
        yaw: 0.0,
        pitch: -89.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 100.0,
    };
    for (excavation, wetness) in [(false, 0.0), (false, 1.0), (true, 0.0), (true, 1.0)] {
        scene.instances[floor].visible = excavation;
        scene.instances[cover].visible = excavation;
        Renderer::mark_changed(&mut scene, floor);
        Renderer::mark_changed(&mut scene, cover);
        let lighting = Lighting {
            enhanced: true,
            shadows: false,
            fog_density: 0.0,
            wetness,
            ..Default::default()
        };
        let enabled = renderer
            .render_to_image(&mut scene, 64, 64, &camera, &lighting)
            .unwrap();
        let pipelines = renderer.prepass_msaa_pipelines.take().unwrap();
        let disabled = renderer
            .render_to_image(&mut scene, 64, 64, &camera, &lighting)
            .unwrap();
        renderer.prepass_msaa_pipelines = Some(pipelines);
        let delta = enabled
            .iter()
            .zip(&disabled)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(
            delta <= 2,
            "MSAA prepass changed layered surfaces by {delta}: excavation {excavation}, wetness {wetness}"
        );
        let pixel = |x: usize| &enabled[(32 * 64 + x) * 4..(32 * 64 + x) * 4 + 3];
        assert!(
            pixel(12)[2] > pixel(12)[1] + 30,
            "cutout hid the road: {:?}",
            pixel(12)
        );
        assert!(
            pixel(52)[1] > pixel(52)[2] + 30,
            "opaque cutout disappeared: {:?}",
            pixel(52)
        );
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_reshaped_mesh_moves_the_bounds_of_every_instance_drawing_it() {
    let adapter = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let renderer = pollster::block_on(Renderer::new_with(
        &adapter, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, shadow_size: 1024, ..Default::default() },
    )).expect("test renderer");
    let mut scene = renderer.new_scene();
    scene.cache_bounds = true;
    let material = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0; 4], true);
    let data = MeshData {
        positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
        normals: vec![Vec3::Z; 3], uvs: vec![glam::Vec2::ZERO; 3],
        indices: vec![0, 1, 2], ranges: vec![(0, 3, 0)], ..Default::default()
    };
    let (skinned, plain) = (renderer.add_mesh(&mut scene, &data), renderer.add_mesh(&mut scene, &data));
    let add = |scene: &mut Scene, mesh, x: f64| renderer.add_instance(scene, mesh, DVec3::new(x, 0.0, 0.0), Mat4::IDENTITY, vec![material]);
    let first = add(&mut scene, skinned, 0.0);
    let others: Vec<usize> = (0..300).map(|k| add(&mut scene, plain, k as f64 * 10.0)).collect();
    renderer.prepare(&mut scene);
    let pose = |scale: f32| -> Vec<Vec3> { data.positions.iter().map(|p| *p * scale).collect() };
    renderer.update_mesh(&mut scene, skinned, &pose(5.0), &data.normals, &data.uvs);
    renderer.prepare(&mut scene);
    // one added after the first pose, one switched to the skinned mesh
    let late = add(&mut scene, skinned, 50.0);
    renderer.set_instance_mesh(&mut scene, others[7], skinned);
    renderer.prepare(&mut scene);
    renderer.update_mesh(&mut scene, skinned, &pose(9.0), &data.normals, &data.uvs);
    renderer.prepare(&mut scene);
    let expected = InstanceBounds::new(&scene.meshes[skinned], Mat4::IDENTITY);
    for i in [first, late, others[7]] {
        assert_eq!(scene.instances[i].bounds.radius, expected.radius, "instance {i}");
    }
    assert_ne!(scene.instances[others[8]].bounds.radius, expected.radius);
    assert!(scene.bounds_users.len() <= 4, "{} instances rescanned", scene.bounds_users.len());
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn terrain_paint_fast_path_preserves_composition() {
    fn quad(
        renderer: &Renderer,
        scene: &mut Scene,
        rect: [f32; 4],
        z: f32,
        mat: MaterialId,
    ) -> usize {
        let [left, right, bottom, top] = rect;
        let mesh = renderer.add_mesh(
            scene,
            &MeshData {
                positions: vec![
                    Vec3::new(left, bottom, z),
                    Vec3::new(right, bottom, z),
                    Vec3::new(right, top, z),
                    Vec3::new(left, top, z),
                ],
                normals: vec![Vec3::Z; 4],
                uvs: vec![
                    glam::Vec2::ZERO,
                    glam::Vec2::X,
                    glam::Vec2::ONE,
                    glam::Vec2::Y,
                ],
                indices: vec![0, 1, 2, 0, 2, 3],
                ranges: vec![(0, 6, 0)],
                one_sided: false,
            },
        );
        renderer.add_surface_instance(scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![mat])
    }
    for msaa in [1, 4] {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let mut renderer = pollster::block_on(Renderer::new_with(
            &instance,
            None,
            Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            RenderOptions {
                msaa,
                ssao: false,
                shadow_size: 1024,
                fxaa: false,
                render_scale: 1.0,
                ..Default::default()
            },
        ))
        .expect("test renderer");
        let mut scene = renderer.new_scene();
        let base = renderer.add_terrain_material(&mut scene, None, None, None, 1.0, None, 1.0);
        let ground = quad(&renderer, &mut scene, [-6.0, 6.0, -6.0, 6.0], 0.0, base);
        scene.instances[ground].render_phase = RenderPhase::Terrain;
        let mut paint_materials = Vec::new();
        for (rgb, alphas) in [
            ([160, 40, 40], [0, 0, 112, 128, 160, 255, 255]),
            ([40, 160, 40], [255, 255, 160, 128, 112, 0, 0]),
        ] {
            let diffuse = renderer.add_texture(
                &mut scene,
                &omsi_texture::Image {
                    width: 4,
                    height: 4,
                    rgba: (0..16)
                        .flat_map(|i| {
                            let c = rgb.map(|v| {
                                if (i / 4 + i % 4) % 2 == 0 { v } else { v / 2 }
                            });
                            [c[0], c[1], c[2], 255]
                        })
                        .collect(),
                    has_alpha: false,
                },
                false,
            );
            let mask = renderer.add_texture(
                &mut scene,
                &omsi_texture::Image {
                    width: 7,
                    height: 1,
                    rgba: alphas
                        .into_iter()
                        .flat_map(|a| [255, 255, 255, a])
                        .collect(),
                    has_alpha: true,
                },
                false,
            );
            // Generated tiled detail exercises sampling beside empty mask pixels.
            let detail = renderer.add_texture(
                &mut scene,
                &omsi_texture::Image {
                    width: 4,
                    height: 4,
                    rgba: (0..16)
                        .flat_map(|i| {
                            let v = if (i / 4 + i % 4) % 2 == 0 { 160 } else { 255 };
                            [v, v, v, 255]
                        })
                        .collect(),
                    has_alpha: false,
                },
                false,
            );
            let mat = renderer.add_terrain_layer_material(
                &mut scene,
                Some(diffuse),
                mask,
                Some((detail, 12.0)),
                8.0,
                None,
                1.0,
            );
            paint_materials.push(mat);
            let layer = quad(&renderer, &mut scene, [-6.0, 6.0, -6.0, 6.0], 0.0, mat);
            scene.instances[layer].ground_layer = true;
            scene.instances[layer].render_phase = RenderPhase::Terrain;
        }
        let blue = renderer.add_material(
            &mut scene,
            None,
            AlphaMode::Opaque,
            [0.0, 0.0, 1.0, 1.0],
            true,
        );
        let road = quad(&renderer, &mut scene, [-6.0, 6.0, -0.7, 0.7], 0.02, blue);
        scene.instances[road].render_phase = RenderPhase::Spline;
        let camera = Camera {
            position: DVec3::new(0.0, -0.105, 6.0),
            yaw: 0.0,
            pitch: -89.0,
            roll: 0.0,
            fov_deg: 90.0,
            near: 0.1,
            far: 100.0,
        };
        // Compare identical scene data through the specialized and original no-write
        // pipelines, including water's opacity and wet-ground reflection coverage.
        for (wetness, water) in [(0.0, false), (1.0, false), (1.0, true)] {
            if water {
                for &mat in &paint_materials {
                    scene.materials[mat].uniform.ambient[3] = 2.0;
                    renderer.queue.write_buffer(
                        &scene.materials[mat].buf,
                        0,
                        bytemuck::bytes_of(&scene.materials[mat].uniform),
                    );
                }
            }
            let lighting = Lighting {
                enhanced: true,
                shadows: false,
                fog_density: 0.0,
                wetness,
                ..Default::default()
            };
            let actual = renderer
                .render_to_image(&mut scene, 64, 64, &camera, &lighting)
                .unwrap();
            let pp = renderer.hdr_pass.as_mut().unwrap();
            let mut saved = Vec::new();
            for variant in 0..4 {
                let original = pp.pipelines[PIPE_BLEND_NO_WRITE as usize * 4 + variant].clone();
                saved.push(std::mem::replace(
                    &mut pp.pipelines[PIPE_TERRAIN_PAINT as usize * 4 + variant],
                    original,
                ));
            }
            let expected = renderer
                .render_to_image(&mut scene, 64, 64, &camera, &lighting)
                .unwrap();
            for (variant, pipeline) in saved.into_iter().enumerate() {
                renderer.hdr_pass.as_mut().unwrap().pipelines
                    [PIPE_TERRAIN_PAINT as usize * 4 + variant] = pipeline;
            }
            let delta = actual
                .iter()
                .zip(&expected)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(
                delta <= 2,
                "paint composition changed by {delta}: MSAA {msaa}, wet {wetness}, water {water}"
            );
            let centre = &actual[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 3];
            if debug_view() <= 0.5 {
                assert!(
                    centre[2] > centre[0] + 30 && centre[2] > centre[1] + 30,
                    "paint hid the road: {centre:?}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_freed_and_recycled_mesh_slot_is_no_longer_counted_as_reshaped() {
    let adapter = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let renderer = pollster::block_on(Renderer::new_with(
        &adapter, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, shadow_size: 1024, ..Default::default() },
    )).expect("test renderer");
    let mut scene = renderer.new_scene();
    scene.cache_bounds = true;
    let material = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0; 4], true);
    let data = MeshData {
        positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
        normals: vec![Vec3::Z; 3], uvs: vec![glam::Vec2::ZERO; 3],
        indices: vec![0, 1, 2], ranges: vec![(0, 3, 0)], ..Default::default()
    };
    let pose = |scale: f32| -> Vec<Vec3> { data.positions.iter().map(|p| *p * scale).collect() };
    let (once_skinned, skinned) = (renderer.add_mesh(&mut scene, &data), renderer.add_mesh(&mut scene, &data));
    let first = renderer.add_instance(&mut scene, skinned, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
    renderer.update_mesh(&mut scene, once_skinned, &pose(2.0), &data.normals, &data.uvs);
    renderer.prepare(&mut scene);
    // the slot is freed and taken by a plain mesh, which a whole tile then draws
    renderer.free_mesh(&mut scene, once_skinned);
    renderer.prepare(&mut scene);
    let plain = renderer.add_mesh(&mut scene, &data);
    assert_eq!(renderer.recycle_mesh(&mut scene, plain, once_skinned), once_skinned);
    let tile: Vec<usize> = (0..200).map(|k| renderer.add_instance(&mut scene, once_skinned, DVec3::new(k as f64, 0.0, 0.0), Mat4::IDENTITY, vec![material])).collect();
    renderer.prepare(&mut scene);
    renderer.update_mesh(&mut scene, skinned, &pose(7.0), &data.normals, &data.uvs);
    renderer.prepare(&mut scene);
    assert!(scene.bounds_users.len() <= 2, "{} instances rescanned", scene.bounds_users.len());
    let expected = InstanceBounds::new(&scene.meshes[skinned], Mat4::IDENTITY);
    assert_eq!(scene.instances[first].bounds.radius, expected.radius);
    let plain_bounds = InstanceBounds::new(&scene.meshes[once_skinned], Mat4::IDENTITY);
    assert_eq!(scene.instances[tile[42]].bounds.radius, plain_bounds.radius);
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn materials_that_look_alike_are_batched_together_whenever_they_were_made() {
    let adapter = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let renderer = pollster::block_on(Renderer::new_with(
        &adapter, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, shadow_size: 1024, ..Default::default() },
    )).expect("test renderer");
    let mut scene = renderer.new_scene();
    let image = omsi_texture::Image { width: 1, height: 1, rgba: vec![200, 100, 50, 255], has_alpha: false };
    let texture = renderer.add_texture(&mut scene, &image, true);
    let a = renderer.add_material(&mut scene, Some(texture), AlphaMode::Opaque, [1.0; 4], false);
    renderer.prepare(&mut scene);
    let b = renderer.add_material(&mut scene, Some(texture), AlphaMode::Opaque, [1.0; 4], false);
    let c = renderer.add_material(&mut scene, Some(texture), AlphaMode::Opaque, [0.5, 0.5, 0.5, 1.0], false);
    let other = renderer.add_texture(&mut scene, &image, true);
    let d = renderer.add_material(&mut scene, Some(other), AlphaMode::Opaque, [1.0; 4], false);
    let look = |scene: &Scene, m: MaterialId| scene.materials[m].look;
    assert_ne!(scene.materials[a].bind_group, scene.materials[b].bind_group);
    assert_eq!(look(&scene, a), look(&scene, b));
    assert_ne!(look(&scene, a), look(&scene, c));
    assert_ne!(look(&scene, a), look(&scene, d));
    let data = MeshData {
        positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
        normals: vec![Vec3::Z; 3], uvs: vec![glam::Vec2::ZERO; 3],
        indices: vec![0, 1, 2], ranges: vec![(0, 3, 0)], ..Default::default()
    };
    let mesh = renderer.add_mesh(&mut scene, &data);
    let instances: Vec<usize> = [a, c, b, d]
        .iter()
        .map(|&m| renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![m]))
        .collect();
    renderer.prepare(&mut scene);
    let mut items: Vec<DrawItem> = instances
        .iter()
        .map(|&i| {
            let material = scene.instances[i].materials[0];
            DrawItem { pipe: 0, mesh: mesh as u32, range: 0, material: material as u32, look: look(&scene, material), entry: scene.instances[i].base }
        })
        .collect();
    let (mut list, mut batches) = (Vec::new(), Vec::new());
    batch_items(&scene, &mut items, true, &mut list, &mut batches);
    assert_eq!(batches.len(), 3);
    let shared = batches.iter().find(|b| b.instances.len() == 2).expect("a and b in one batch");
    assert!(shared.material == a as u32 || shared.material == b as u32);
    let mut entries: Vec<u32> = list[shared.instances.start as usize..shared.instances.end as usize].to_vec();
    entries.sort_unstable();
    assert_eq!(entries, vec![scene.instances[instances[0]].base, scene.instances[instances[2]].base]);
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn cached_bounds_follow_transforms_skinning_and_recycled_resources() {
    let adapter = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let renderer = pollster::block_on(Renderer::new_with(
        &adapter, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, shadow_size: 1024, ..Default::default() },
    )).expect("test renderer");
    let mut scene = renderer.new_scene();
    scene.cache_bounds = true;
    let material = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0; 4], true);
    let data = MeshData {
        positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
        normals: vec![Vec3::Z; 3], uvs: vec![glam::Vec2::ZERO; 3],
        indices: vec![0, 1, 2], ranges: vec![(0, 3, 0)], ..Default::default()
    };
    let mesh = renderer.add_mesh(&mut scene, &data);
    let origin = DVec3::new(1_000_000.000_001, 2_000_000.0, 12.0);
    let i = renderer.add_instance(&mut scene, mesh, origin, Mat4::IDENTITY, vec![material]);
    let check = |scene: &Scene, i: usize| {
        let inst = &scene.instances[i];
        let expected = InstanceBounds::new(&scene.meshes[inst.mesh], inst.transform);
        assert_eq!(Renderer::bounding_sphere(scene, inst),
            (expected.centre + (inst.origin - scene.render_origin).as_vec3(), expected.radius));
        assert_eq!(Renderer::instance_scale(scene, inst), expected.scale);
    };
    renderer.prepare(&mut scene);
    check(&scene, i);
    let transform = Mat4::from_scale_rotation_translation(
        Vec3::new(-2.0, 3.0, 4.0), glam::Quat::from_rotation_z(0.7), Vec3::new(10.0, 20.0, 30.0));
    renderer.set_transform(&mut scene, i, origin, transform);
    renderer.prepare(&mut scene);
    check(&scene, i);
    let posed: Vec<_> = data.positions.iter().map(|p| *p * 5.0 + Vec3::Z).collect();
    renderer.update_mesh(&mut scene, mesh, &posed, &data.normals, &data.uvs);
    renderer.prepare(&mut scene);
    check(&scene, i);
    renderer.set_render_origin(&mut scene, origin - DVec3::new(0.25, 0.5, 0.75));
    renderer.prepare(&mut scene);
    check(&scene, i);
    renderer.free_mesh(&mut scene, mesh);
    renderer.prepare(&mut scene);
    check(&scene, i);
    let new = renderer.add_mesh(&mut scene, &data);
    assert_eq!(renderer.recycle_mesh(&mut scene, new, mesh), mesh);
    renderer.prepare(&mut scene);
    check(&scene, i);
    let replacement = renderer.add_instance(&mut scene, mesh, origin, Mat4::IDENTITY, vec![material]);
    assert_eq!(renderer.recycle_instance(&mut scene, replacement, i), i);
    renderer.prepare(&mut scene);
    check(&scene, i);
    let other = renderer.add_mesh(&mut scene, &data);
    renderer.set_instance_mesh(&mut scene, i, other);
    renderer.prepare(&mut scene);
    check(&scene, i);
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn presurface_reveals_excavation_before_terrain_is_drawn() {
    fn quad(renderer: &Renderer, scene: &mut Scene, y: f32, half: f32) -> MeshId {
        renderer.add_mesh(
            scene,
            &MeshData {
                positions: vec![
                    Vec3::new(-half, y, -half),
                    Vec3::new(half, y, -half),
                    Vec3::new(half, y, half),
                    Vec3::new(-half, y, half),
                ],
                normals: vec![-Vec3::Y; 4],
                uvs: vec![glam::Vec2::ZERO; 4],
                ranges: vec![(0, 6, 0)],
                indices: vec![0, 1, 2, 0, 2, 3],
                one_sided: false,
            },
        )
    }
    let camera = Camera {
        position: DVec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 100.0,
    };
    for (msaa, ssao, enhanced) in [
        (1, false, false),
        (1, true, false),
        (1, false, true),
        (1, true, true),
        (4, true, true),
    ] {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let mut renderer = pollster::block_on(Renderer::new_with(
            &instance,
            None,
            Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            RenderOptions {
                msaa,
                ssao,
                shadow_size: 1024,
                fxaa: false,
                render_scale: 1.0,
                ..Default::default()
            },
        ))
        .expect("test renderer");
        renderer.profiling = true;
        let mut scene = renderer.new_scene();
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
        let red = renderer.add_material(
            &mut scene,
            None,
            AlphaMode::Opaque,
            [1.0, 0.0, 0.0, 1.0],
            true,
        );
        let texture = renderer.add_texture(
            &mut scene,
            &omsi_texture::Image {
                width: 1,
                height: 1,
                rgba: vec![255, 255, 255, 0],
                has_alpha: true,
            },
            false,
        );
        let transparent =
            renderer.add_material(&mut scene, Some(texture), AlphaMode::Blend, [1.0; 4], true);
        let cutout =
            renderer.add_material(&mut scene, Some(texture), AlphaMode::Test, [1.0; 4], true);
        // Put terrain first in the scene to catch reliance on insertion order. The
        // excavation floor is behind it, with a transparent cover in front of both.
        let terrain = quad(&renderer, &mut scene, 6.0, 10.0);
        renderer.add_instance(
            &mut scene,
            terrain,
            DVec3::ZERO,
            Mat4::IDENTITY,
            vec![green],
        );
        let floor = quad(&renderer, &mut scene, 8.0, 10.0);
        let floor = renderer.add_surface_instance(
            &mut scene,
            floor,
            DVec3::ZERO,
            Mat4::IDENTITY,
            vec![blue],
        );
        scene.instances[floor].presurface = true;
        let cover_mesh = quad(&renderer, &mut scene, 4.0, 1.5);
        let cover = renderer.add_surface_instance(
            &mut scene,
            cover_mesh,
            DVec3::ZERO,
            Mat4::IDENTITY,
            vec![transparent],
        );
        scene.instances[cover].presurface = true;
        // A normal opaque object lies between the excavation floor and its cover.
        // Prefilling its depth before the floor's colour erases the excavation, even
        // though the cover will reject that object's later colour draw.
        let background_mesh = quad(&renderer, &mut scene, 7.0, 2.0);
        renderer.add_instance(&mut scene, background_mesh, DVec3::ZERO, Mat4::IDENTITY, vec![red]);
        // Ordinary cutouts need shading to determine visibility on tile GPUs.
        // Keep a transparent one in the view to exercise mixed-scene prefilling.
        let foliage_mesh = quad(&renderer, &mut scene, 10.0, 0.25);
        renderer.add_instance(&mut scene, foliage_mesh, DVec3::ZERO, Mat4::IDENTITY, vec![cutout]);
        let foreground_mesh = quad(&renderer, &mut scene, 2.0, 0.25);
        let foreground = renderer.add_instance(
            &mut scene,
            foreground_mesh,
            DVec3::ZERO,
            Mat4::IDENTITY,
            vec![red],
        );
        scene.instances[foreground].visible = false;
        let lighting = Lighting {
            enhanced,
            shadows: false,
            fog_density: 0.0,
            ..Default::default()
        };
        let pixel = |rgba: &[u8], x: usize| -> [u8; 3] {
            rgba[(32 * 64 + x) * 4..(32 * 64 + x) * 4 + 3]
                .try_into()
                .unwrap()
        };
        let rgba = renderer
            .render_to_image(&mut scene, 64, 64, &camera, &lighting)
            .unwrap();
        if enhanced {
            let key = if renderer.options.msaa > 1 { "msaa prepass batches" } else { "presurface prepass batches" };
            assert!(renderer.counts.borrow().get(key).copied().unwrap_or(0.0) > 0.0,
                "an excavation must not disable depth prefilling for the rest of the view");
        }
        if enhanced {
            let saved = renderer.prepass_msaa_pipelines.take();
            let reference = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
            renderer.prepass_msaa_pipelines = saved;
            assert!(rgba.iter().zip(&reference).all(|(a, b)| a.abs_diff(*b) <= 2),
                "presurface depth optimization changed the image: {msaa}/{ssao}/{enhanced}");
        }
        let centre = pixel(&rgba, 32);
        assert!(
            centre[2] > centre[1] + 40,
            "floor must show through cover: {centre:?}; {msaa}/{ssao}/{enhanced}"
        );
        let outside = pixel(&rgba, 4);
        assert!(
            outside[1] > outside[2] + 40,
            "terrain outside cover: {outside:?}"
        );
        scene.instances[foreground].visible = true;
        Renderer::mark_changed(&mut scene, foreground);
        let rgba = renderer
            .render_to_image(&mut scene, 64, 64, &camera, &lighting)
            .unwrap();
        let centre = pixel(&rgba, 32);
        assert!(
            centre[0] > centre[2] + 40,
            "foreground stays visible: {centre:?}"
        );
        scene.instances[foreground].visible = false;
        Renderer::mark_changed(&mut scene, foreground);
        // Ordinary blended surfaces must keep showing the terrain, as must covers
        // whose material explicitly disables depth writes or uses alpha testing.
        for (presurface, alpha, no_z_write) in [
            (false, AlphaMode::Blend, false),
            (true, AlphaMode::Blend, true),
            (true, AlphaMode::Test, false),
        ] {
            scene.instances[cover].presurface = presurface;
            scene.materials[transparent].no_z_write = no_z_write;
            renderer.set_material(
                &mut scene,
                cover,
                0,
                if alpha == AlphaMode::Test {
                    cutout
                } else {
                    transparent
                },
            );
            let rgba = renderer
                .render_to_image(&mut scene, 64, 64, &camera, &lighting)
                .unwrap();
            let centre = pixel(&rgba, 32);
            assert!(
                centre[1] > centre[2] + 40,
                "terrain should show: {centre:?}; {presurface}/{alpha:?}/{no_z_write}"
            );
        }
    }
}

/// Two blended panes of one model, the near one listed first, both marked see-through
/// (`no_z_write`) for the shading: written into the depth buffer as Omsi.exe writes it
/// (`writes_depth`, no `[matl_noZwrite]` in the model), the far pane drawn after it is
/// hidden behind it; left out of it (`[matl_noZwrite]`), it is blended over it (#211).
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn stacked_panes_hide_each_other_in_model_order_where_they_write_depth() {
    let camera = Camera {
        position: DVec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 100.0,
    };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    ))
    .expect("test renderer");
    let mut scene = renderer.new_scene();
    let quad = |y: f32, half: f32| -> [Vec3; 4] {
        [Vec3::new(-half, y, -half), Vec3::new(half, y, -half), Vec3::new(half, y, half), Vec3::new(-half, y, half)]
    };
    let wall = renderer.add_mesh(
        &mut scene,
        &MeshData {
            positions: quad(8.0, 20.0).to_vec(),
            normals: vec![-Vec3::Y; 4],
            uvs: vec![glam::Vec2::ZERO; 4],
            ranges: vec![(0, 6, 0)],
            indices: vec![0, 1, 2, 0, 2, 3],
            one_sided: false,
        },
    );
    let green = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 1.0, 0.0, 1.0], true);
    renderer.add_instance(&mut scene, wall, DVec3::ZERO, Mat4::IDENTITY, vec![green]);
    // the near pane (slot 0) and the far one (slot 1), half transparent
    let panes = renderer.add_mesh(
        &mut scene,
        &MeshData {
            positions: [quad(2.0, 4.0), quad(4.0, 8.0)].concat(),
            normals: vec![-Vec3::Y; 8],
            uvs: vec![glam::Vec2::ZERO; 8],
            ranges: vec![(0, 6, 0), (6, 6, 1)],
            indices: vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7],
            one_sided: false,
        },
    );
    let half = renderer.add_texture(
        &mut scene,
        &omsi_texture::Image { width: 1, height: 1, rgba: vec![255, 255, 255, 128], has_alpha: true },
        false,
    );
    let near = renderer.add_material(&mut scene, Some(half), AlphaMode::Blend, [1.0, 0.0, 0.0, 1.0], true);
    let far = renderer.add_material(&mut scene, Some(half), AlphaMode::Blend, [0.0, 0.0, 1.0, 1.0], true);
    renderer.add_instance(&mut scene, panes, DVec3::ZERO, Mat4::IDENTITY, vec![near, far]);
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    for writes_depth in [true, false] {
        for m in [near, far] {
            scene.materials[m].no_z_write = true;
            scene.materials[m].writes_depth = writes_depth;
        }
        let rgba = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
        let c = &rgba[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 3];
        if writes_depth {
            assert!(c[2] < 20 && c[0] > 60 && c[1] > 60, "far pane hidden behind the near one: {c:?}");
        } else {
            assert!(c[2] > 40 && c[0] > 60, "far pane blended over the near one: {c:?}");
        }
    }
}

/// A `[matl_envmap]` reads its sphere map where Direct3D's D3DTSS_TCI_SPHEREMAP puts it
/// (u = Rx/m + 0.5, v = Ry/m + 0.5, m = 2|R - (0, 0, 1)| in camera space): a pane
/// turned 30 degrees from the view mirrors a ray 60 degrees off it, a quarter of the
/// way out from the map's middle (0.75), not near its rim (0.93) as the u = Rx/2 + 0.5
/// taken before had it; turned up, the same below the middle (the sky's half, #1193).
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn vanilla_sphere_map_is_read_where_direct3d_generates_it() {
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 100.0 };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    ))
    .expect("test renderer");
    let mut scene = renderer.new_scene();
    // the sphere map: red counts u, green v
    let size = 64usize;
    let mut rgba = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let at = |i: usize| ((i as f32 + 0.5) / size as f32 * 255.0).round() as u8;
            rgba.extend_from_slice(&[at(x), at(y), 0, 255]);
        }
    }
    let env = renderer.add_texture(&mut scene, &omsi_texture::Image { width: size as u32, height: size as u32, rgba, has_alpha: false }, false);
    let white = renderer.add_texture(&mut scene, &omsi_texture::Image { width: 1, height: 1, rgba: vec![255; 4], has_alpha: false }, false);
    let paint = renderer.add_material_env(&mut scene, Some(white), AlphaMode::Opaque, [1.0; 4], false, None, None, None, Some((env, 1.0)));
    // a pane 5 m ahead, its normal turned 30 degrees from the view: about z (a side
    // to the right) or about x (the pane tipped back, mirroring the sky)
    let pane = |renderer: &Renderer, scene: &mut Scene, n: Vec3, along: Vec3| {
        let c = Vec3::new(0.0, 5.0, 0.0);
        let across = n.cross(along);
        let mesh = renderer.add_mesh(
            scene,
            &MeshData {
                positions: vec![c - along - across, c + along - across, c + along + across, c - along + across],
                normals: vec![n; 4],
                uvs: vec![glam::Vec2::splat(0.5); 4],
                ranges: vec![(0, 6, 0)],
                indices: vec![0, 1, 2, 0, 2, 3],
                one_sided: false,
            },
        );
        renderer.add_instance(scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![paint])
    };
    let (s, c) = 30f32.to_radians().sin_cos();
    let lighting = Lighting { shadows: false, fog_density: 0.0, classic: true, ..Default::default() };
    let mut read = |n: Vec3, along: Vec3| {
        let id = pane(&renderer, &mut scene, n, along);
        let rgba = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
        scene.instances[id].visible = false;
        let p = &rgba[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 3];
        (p[0] as f32 / 255.0, p[1] as f32 / 255.0)
    };
    let (u, v) = read(Vec3::new(s, -c, 0.0), Vec3::Z);
    assert!((u - 0.75).abs() < 0.03 && (v - 0.5).abs() < 0.03, "turned to the side: ({u}, {v})");
    let (u, v) = read(Vec3::new(0.0, -c, s), Vec3::X);
    assert!((u - 0.5).abs() < 0.03 && (v - 0.75).abs() < 0.03, "tipped back: ({u}, {v})");
}

/// A lamp's flare behind a window that writes its depth: seen from inside the vehicle
/// it shows through the glass, which Omsi.exe draws after the flares (0x6f0430 after
/// 0x6f0400/0x6f0418); seen from outside the vehicle, drawn before the flares, the
/// window hides it as in the original.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn flares_show_through_the_glass_of_the_vehicle_the_camera_is_in() {
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 90.0, near: 0.1, far: 100.0 };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    ))
    .expect("test renderer");
    let mut scene = renderer.new_scene();
    let quad = |y: f32, half: f32| MeshData {
        positions: vec![Vec3::new(-half, y, -half), Vec3::new(half, y, -half), Vec3::new(half, y, half), Vec3::new(-half, y, half)],
        normals: vec![-Vec3::Y; 4],
        uvs: vec![glam::Vec2::ZERO; 4],
        ranges: vec![(0, 6, 0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        one_sided: false,
    };
    let wall = renderer.add_mesh(&mut scene, &quad(30.0, 60.0));
    let black = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 0.0, 0.0, 1.0], true);
    renderer.add_instance(&mut scene, wall, DVec3::new(0.0, 0.0, 0.0), Mat4::IDENTITY, vec![black]);
    let pane = renderer.add_mesh(&mut scene, &quad(2.0, 4.0));
    let half = renderer.add_texture(
        &mut scene,
        &omsi_texture::Image { width: 1, height: 1, rgba: vec![255, 255, 255, 64], has_alpha: true },
        false,
    );
    let glass = renderer.add_material(&mut scene, Some(half), AlphaMode::Blend, [0.2, 0.2, 0.2, 1.0], true);
    scene.materials[glass].no_z_write = true;
    scene.materials[glass].writes_depth = true;
    renderer.add_instance(&mut scene, pane, DVec3::ZERO, Mat4::IDENTITY, vec![glass]);
    scene.coronas = vec![Corona { position: DVec3::new(0.0, 10.0, 0.0), size: 3.0, color: [1.0; 3], brightness: 1.0, ..Default::default() }];
    for inside in [true, false] {
        let lighting = Lighting {
            shadows: false,
            fog_density: 0.0,
            night: 1.0,
            inside: inside.then_some((DVec3::ZERO, 0.0, [6.0, 6.0, 6.0, 0.0, 0.0, 0.0])),
            ..Default::default()
        };
        let rgba = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
        let c = &rgba[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 3];
        if inside {
            assert!(c[0] > 120, "the flare shows through the windscreen: {c:?}");
        } else {
            assert!(c[0] < 80, "the flare behind a bus's window seen from outside: {c:?}");
        }
    }
}

// (the rest of the renderer's tests, in a file of their own for the size budget)
mod bundles;
mod coplanar;
mod fsr;
mod instance_layout;
mod lamp_shadows;
mod light_grid;
mod light_maps;
mod long_road;
mod light;
mod more;
mod night_coverage;
mod prepass_reach;
mod transparency;
mod vanilla_lighting;
mod xr_culling;
