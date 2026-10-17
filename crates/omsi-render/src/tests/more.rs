use super::*;

/// Rails laid a millimetre over a road as a spline of their own, seen from the cab: the
/// road's 10 m stretch between two cross-sections and the rails' metre-long ones are
/// pulled towards the eye alike, so the rails lie on the road right up to the bus. With a
/// fixed 2 cm pull per vertex the road's long triangles bowed over them near the eye and
/// the rails went under the road there (#1196).
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_spline_a_millimetre_over_a_road_stays_over_it_near_the_eye() {
    let camera = Camera { position: DVec3::new(0.0, 0.0, 2.5), yaw: 0.0, pitch: -25.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 200.0 };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    ))
    .expect("test renderer");
    let mut scene = renderer.new_scene();
    let strip = |half: f32, z: f32, stations: usize| {
        let mut m = MeshData { ranges: vec![(0, 6 * stations as u32, 0)], ..Default::default() };
        for i in 0..=stations {
            let y = 2.0 + 10.0 * i as f32 / stations as f32;
            m.positions.extend([Vec3::new(-half, y, z), Vec3::new(half, y, z)]);
            m.normals.extend([Vec3::Z; 2]);
            m.uvs.extend([glam::Vec2::ZERO; 2]);
        }
        for i in 0..stations as u32 {
            let a = 2 * i;
            m.indices.extend([a, a + 1, a + 2, a + 1, a + 3, a + 2]);
        }
        m
    };
    let road = renderer.add_mesh(&mut scene, &strip(3.0, 0.0, 1));
    let rails = renderer.add_mesh(&mut scene, &strip(0.2, 0.001, 10));
    let grey = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.3, 0.3, 0.3, 1.0], true);
    let red = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 0.0, 0.0, 1.0], true);
    for (mesh, mat) in [(road, grey), (rails, red)] {
        let i = renderer.add_surface_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![mat]);
        scene.instances[i].render_phase = RenderPhase::Spline;
    }
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    let rgba = renderer.render_to_image(&mut scene, 128, 128, &camera, &lighting).unwrap();
    // the middle column from 3 m (row 93) to 11 m (row 40) in front of the camera
    for row in 42..92 {
        let c = &rgba[(row * 128 + 64) * 4..(row * 128 + 64) * 4 + 3];
        assert!(c[0] > c[1] + 60, "row {row}: the road over the rails: {c:?}");
    }
}

/// A `[smoke]` puff (an exhaust's, a wheel's spray) is drawn where it is, in its colour,
/// in every graphics mode (#949, #948).
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn smoke_puffs_are_drawn() {
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 100.0 };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    ))
    .expect("test renderer");
    renderer.set_smoke_texture(&omsi_texture::Image { width: 4, height: 4, rgba: vec![255; 64], has_alpha: true });
    let mut scene = renderer.new_scene();
    for (classic, enhanced) in [(true, false), (false, false), (false, true)] {
        scene.smoke = vec![SmokeParticle { position: DVec3::new(0.0, 5.0, 0.0), size: 1.0, color: [1.0, 0.0, 0.0], alpha: 1.0, ..Default::default() }];
        let lighting = Lighting { shadows: false, fog_density: 0.0, classic, enhanced, ..Default::default() };
        let rgba = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
        let c = &rgba[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 3];
        assert!(c[0] > 120 && c[1] < 60 && c[2] < 60, "a red puff 5 m ahead (classic {classic}, enhanced {enhanced}): {c:?}");
    }
}

fn smoke_test_renderer() -> Renderer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    ))
    .expect("test renderer")
}

/// A puff that knows the ground under it fades out into it over its lowest part - no
/// line where the road would cut it off - and keeps its colour higher up, in every
/// graphics mode; one wholly under its ground is not drawn.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_smoke_puff_fades_into_its_ground() {
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 100.0 };
    let mut renderer = smoke_test_renderer();
    renderer.set_smoke_texture(&omsi_texture::Image { width: 4, height: 4, rgba: vec![255; 64], has_alpha: true });
    let mut scene = renderer.new_scene();
    let red = |row: usize, rgba: &[u8]| rgba[(row * 64 + 32) * 4] as i32;
    for (classic, enhanced) in [(true, false), (false, false), (false, true)] {
        let lighting = Lighting { shadows: false, fog_density: 0.0, classic, enhanced, ..Default::default() };
        // 5 m ahead with the ground at its middle: drawn 0.9 m round, a pixel row 9 cm
        // there (60 degrees over 64 rows); the fade reaches 0.18 m up
        let profile = |scene: &mut Scene, renderer: &mut Renderer| -> Vec<i32> {
            let rgba = renderer.render_to_image(scene, 64, 64, &camera, &lighting).unwrap();
            let bg = red(56, &rgba);
            (24..=40).map(|row| red(row, &rgba) - bg).collect()
        };
        scene.smoke = vec![SmokeParticle { position: DVec3::new(0.0, 5.0, 0.0), size: 1.0, color: [1.0, 0.0, 0.0], alpha: 1.0, ..Default::default() }];
        let plain = profile(&mut scene, &mut renderer);
        scene.smoke[0].ground = Some(0.0);
        let faded = profile(&mut scene, &mut renderer);
        let (at, full) = (|row: usize| faded[row - 24], |row: usize| plain[row - 24]);
        let mode = format!("classic {classic}, enhanced {enhanced}: {faded:?} against {plain:?}");
        assert!(full(25) > 30 && full(33) > 30, "the plain puff is drawn ({mode})");
        assert!(at(34).abs() < 4 && at(38).abs() < 4, "under its ground ({mode})");
        assert!(at(31) * 2 < full(31), "a few centimetres over the ground it is all but gone ({mode})");
        assert!((at(28) - full(28)).abs() <= 3 && (at(25) - full(25)).abs() <= 3, "30 cm up it is as it was ({mode})");
        for row in 28..31 {
            assert!(at(row) >= at(row + 1) - 2, "fading down into the ground, no step ({mode})");
        }
        // and one sunk wholly into its ground is left out
        scene.smoke[0].ground = Some(0.95);
        let rgba = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
        assert!((red(30, &rgba) - red(56, &rgba)).abs() < 6, "a puff under its ground drawn (classic {classic}, enhanced {enhanced})");
    }
}

/// A puff's picture is turned by its spin about the line of sight (Omsi.exe turns every
/// one its own way): the top half of rauch.tga comes to the bottom at half a turn.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_smoke_puffs_picture_turns_with_its_spin() {
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 100.0 };
    let mut renderer = smoke_test_renderer();
    let mut rgba = vec![255u8; 64];
    for px in 8..16 {
        rgba[px * 4 + 3] = 0;
    }
    renderer.set_smoke_texture(&omsi_texture::Image { width: 4, height: 4, rgba, has_alpha: true });
    let mut scene = renderer.new_scene();
    let red = |row: usize, rgba: &[u8]| rgba[(row * 64 + 32) * 4] as i32;
    let lighting = Lighting { shadows: false, fog_density: 0.0, classic: true, ..Default::default() };
    for (spin, top) in [(0.0, true), (std::f32::consts::PI, false)] {
        scene.smoke = vec![SmokeParticle { position: DVec3::new(0.0, 5.0, 0.0), size: 1.0, color: [1.0, 0.0, 0.0], alpha: 1.0, spin, ..Default::default() }];
        let rgba = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
        let bg = red(56, &rgba);
        let (up, down) = (red(25, &rgba) - bg, red(39, &rgba) - bg);
        assert_eq!((up > 60, down > 60), (top, !top), "spin {spin}: above {up}, below {down}");
    }
}

/// A smoke particle's sprite: its spin, its ground relative to the render origin and its
/// z offset where the smoke branch of the shader reads them; none when it is too faint or
/// wholly under its ground.
#[test]
fn a_smoke_sprite_carries_its_spin_ground_and_z_offset() {
    let ro = DVec3::new(1000.0, 2000.0, 30.0);
    let p = SmokeParticle { position: ro + DVec3::new(1.0, 2.0, 3.5), size: 1.0, color: [0.5; 3], alpha: 0.8, spin: std::f32::consts::FRAC_PI_2, z_offset: 0.1, ground: Some(33.0), ..Default::default() };
    let g = smoke_sprite(&p, ro).unwrap();
    assert_eq!(g.pos, [1.0, 2.0, 3.5]);
    assert_eq!(g.extra[3], 3.0);
    assert!(g.dir[0].abs() < 1e-6 && (g.dir[1] - 1.0).abs() < 1e-6, "{:?}", g.dir);
    assert_eq!(g.up[..3], [3.0, 1.0, 0.1]);
    assert!((g.up[3] - 0.18).abs() < 1e-6, "fade {}", g.up[3]);
    // the fade into the ground: a fifth of the drawn radius, 6 to 25 cm
    assert_eq!((smoke_ground_fade(0.1), smoke_ground_fade(5.0)), (0.06, 0.25));
    // no ground: not faded
    let free = smoke_sprite(&SmokeParticle { ground: None, ..p }, ro).unwrap();
    assert_eq!(free.up[1], 0.0);
    // its top (drawn at 0.9 of its half width) under the ground: left out
    assert!(smoke_sprite(&SmokeParticle { ground: Some(30.0 + 3.5 + 0.91), ..p }, ro).is_none());
    assert!(smoke_sprite(&SmokeParticle { ground: Some(30.0 + 3.5 + 0.89), ..p }, ro).is_some());
    assert!(smoke_sprite(&SmokeParticle { alpha: 0.0, ..p }, ro).is_none());
}

/// The launcher's preview (`RenderOptions::preview_only`) makes no enhanced pipeline,
/// reflection probe, ray tracer or cloud noise, its camera group leaves the enhanced
/// textures out, and it draws a frame with that group.
#[test]
fn launcher_preview_initializes_without_enhanced_or_ray_tracing_resources() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let instance = wgpu::Instance::new(descriptor);
    let mut renderer = pollster::block_on(Renderer::new_with(&instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb), RenderOptions { msaa: 1, preview_only: true, no_enhanced: true, ray_tracing: false, ..Default::default() })).unwrap();
    assert!(renderer.hdr_pass.is_none());
    assert!(renderer.tone.probe.is_none());
    assert!(renderer.rt.is_none());
    assert!(renderer.sky.cloud_shape_cpu.is_empty());
    let bindings: Vec<u32> = camera_layout_entries(ArrayPath::Storage, true).into_iter().map(|e| e.binding).collect();
    for binding in ENHANCED_CAMERA_TEXTURES {
        assert!(!bindings.contains(&binding), "preview camera layout must omit enhanced binding {binding}");
    }
    let mut scene = renderer.new_scene();
    let camera = Camera { position: DVec3::new(0.0, -35.0, 30.0), yaw: 0.0, pitch: -40.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 1000.0 };
    renderer.render_to_image(&mut scene, 16, 16, &camera, &Lighting::default()).unwrap();
}

#[test]
fn noop_backend_initializes_renderer() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let instance = wgpu::Instance::new(descriptor);
    let res = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: 2,
            shadow_size: 1024,
            ..Default::default()
        },
    ));
    assert!(res.is_ok(), "renderer should initialize on noop backend: {:?}", res.err());
}

/// The GL fallback must omit probe resources and match its camera bind group to the
/// vanilla pipelines. Exercise the full build and a frame, rather than the policy alone.
#[test]
fn vanilla_gl_build_omits_unused_enhanced_resources() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let instance = wgpu::Instance::new(descriptor);
    let base = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: 1,
            shadow_size: 256,
            ..Default::default()
        },
    ))
    .unwrap();
    let mut renderer = Renderer::build(
        base.device.clone(),
        base.queue.clone(),
        "OpenGL regression adapter".into(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
        RenderOptions {
            msaa: 1,
            shadow_size: 256,
            no_enhanced: true,
            ..Default::default()
        },
    );
    assert!(renderer.hdr_pass.is_none());
    assert!(renderer.tone.probe.is_none());
    assert!(renderer.sky.cloud_shape_cpu.is_empty());
    let mut scene = renderer.new_scene();
    let camera = Camera {
        position: DVec3::new(0.0, -35.0, 30.0),
        yaw: 0.0,
        pitch: -40.0,
        roll: 0.0,
        fov_deg: 60.0,
        near: 0.1,
        far: 1000.0,
    };
    let scope = renderer
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    renderer
        .render_to_image(&mut scene, 16, 16, &camera, &Lighting::default())
        .unwrap();
    assert!(pollster::block_on(scope.pop()).is_none());
    // A desktop's native backend still prepares Enhanced for changing modes in-game.
    assert!(base.hdr_pass.is_some());
    assert!(base.tone.probe.is_some());
}

#[test]
fn excavation_keeps_ordinary_depth_prefilling_without_a_gpu() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let instance = wgpu::Instance::new(descriptor);
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 4, ssao: true, render_scale: 1.0, ..Default::default() },
    )).expect("noop renderer");
    renderer.profiling = true;
    let mut scene = renderer.new_scene();
    let material = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0; 4], true);
    let mesh = renderer.add_mesh(&mut scene, &MeshData {
        positions: vec![Vec3::new(-2.0, 4.0, -2.0), Vec3::new(2.0, 4.0, -2.0),
            Vec3::new(2.0, 4.0, 2.0), Vec3::new(-2.0, 4.0, 2.0)],
        normals: vec![-Vec3::Y; 4], uvs: vec![glam::Vec2::ZERO; 4],
        indices: vec![0, 1, 2, 0, 2, 3], ranges: vec![(0, 6, 0)],
        ..Default::default()
    });
    let floor = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
    scene.instances[floor].presurface = true;
    let transparent = renderer.add_material(&mut scene, None, AlphaMode::Blend, [1.0, 1.0, 1.0, 0.0], true);
    let cover = renderer.add_surface_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![transparent]);
    scene.instances[cover].presurface = true;
    let glass_material = renderer.add_material(&mut scene, None, AlphaMode::Blend, [1.0, 1.0, 1.0, 0.25], true);
    let glass = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![glass_material]);
    let scenery = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0,
        fov_deg: 90.0, near: 0.1, far: 100.0 };
    let lighting = Lighting { enhanced: true, shadows: false, fog_density: 0.0, ..Default::default() };
    // Exercise the actual render planner on CI's CPU-only backend. A presurface
    // must not turn off prefilling for ordinary scenery, but ground must stay out:
    // its authored blends have to finish before later surface coverage writes depth.
    for phase in RenderPhase::DRAW_ORDER {
        scene.instances[scenery].render_phase = phase;
        renderer.counts.borrow_mut().clear();
        renderer.render_to_image(&mut scene, 16, 16, &camera, &lighting).unwrap();
        let batches = renderer.counts.borrow().get("msaa prepass batches").copied().unwrap_or(0.0);
        let ordinary = matches!(phase, RenderPhase::BeforeNormal | RenderPhase::Normal
            | RenderPhase::AfterNormal | RenderPhase::AfterVehicles);
        assert_eq!(batches > 0.0, ordinary, "phase {phase:?}");
    }
    // The explicit presurface flag overrides even an ordinary authored phase.
    scene.instances[scenery].render_phase = RenderPhase::Normal;
    scene.instances[scenery].presurface = true;
    renderer.counts.borrow_mut().clear();
    renderer.render_to_image(&mut scene, 16, 16, &camera, &lighting).unwrap();
    assert_eq!(renderer.counts.borrow().get("msaa prepass batches"), None);
    if cfg!(target_vendor = "apple") {
        scene.instances[scenery].presurface = false;
        scene.instances[glass].visible = false;
        renderer.counts.borrow_mut().clear();
        renderer.render_to_image(&mut scene, 16, 16, &camera, &lighting).unwrap();
        assert_eq!(renderer.counts.borrow().get("msaa prepass batches"), None,
            "an opaque-only view keeps native hidden-surface removal even with an excavation");
    }
}

#[test]
fn an_excavation_is_prefilled_at_1x_only_on_apple_graphics() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let instance = wgpu::Instance::new(descriptor);
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: true, render_scale: 1.0, ..Default::default() },
    )).expect("noop renderer");
    renderer.profiling = true;
    let mut scene = renderer.new_scene();
    let material = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0; 4], true);
    let mesh = renderer.add_mesh(&mut scene, &MeshData {
        positions: vec![Vec3::new(-2.0, 4.0, -2.0), Vec3::new(2.0, 4.0, -2.0),
            Vec3::new(2.0, 4.0, 2.0), Vec3::new(-2.0, 4.0, 2.0)],
        normals: vec![-Vec3::Y; 4], uvs: vec![glam::Vec2::ZERO; 4],
        indices: vec![0, 1, 2, 0, 2, 3], ranges: vec![(0, 6, 0)],
        ..Default::default()
    });
    let floor = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
    scene.instances[floor].presurface = true;
    let glass_material = renderer.add_material(&mut scene, None, AlphaMode::Blend, [1.0, 1.0, 1.0, 0.25], true);
    renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![glass_material]);
    renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0,
        fov_deg: 90.0, near: 0.1, far: 100.0 };
    let lighting = Lighting { enhanced: true, shadows: false, fog_density: 0.0, ..Default::default() };
    renderer.counts.borrow_mut().clear();
    renderer.render_to_image(&mut scene, 16, 16, &camera, &lighting).unwrap();
    let prefilled = renderer.counts.borrow().get("presurface prepass batches").is_some_and(|n| *n > 0.0);
    assert_eq!(prefilled, cfg!(target_vendor = "apple"));
}

#[test]
fn declared_transmap_ignores_slot_alpha() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let instance = wgpu::Instance::new(descriptor);
    let renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: 1,
            shadow_size: 1024,
            ..Default::default()
        },
    ))
    .expect("noop renderer");
    let mut scene = renderer.new_scene();
    let blended =
        |scene: &mut Scene, transmap: Option<(TextureId, bool)>, extra: MaterialExtra| {
            renderer.add_material_extra(
                scene,
                None,
                AlphaMode::Blend,
                [1.0; 4],
                true,
                transmap,
                None,
                None,
                None,
                [0.0; 3],
                extra,
            )
        };
    // declared, its file missing: no transmap texture bound
    let declared = blended(
        &mut scene,
        None,
        MaterialExtra {
            transmap_declared: true,
            ..Default::default()
        },
    );
    let map = renderer.add_blank_texture(&mut scene, 1, 1);
    let bound = blended(&mut scene, Some((map, true)), MaterialExtra::default());
    // another bit of the same flags, no transmap
    let metal = blended(
        &mut scene,
        None,
        MaterialExtra {
            metal_ok: true,
            ..Default::default()
        },
    );
    let plain = blended(&mut scene, None, MaterialExtra::default());
    assert_eq!(scene.materials[metal].uniform.params2[3], 4.0);
    let flags: Vec<bool> = [declared, bound, metal, plain]
        .iter()
        .map(|&m| scene.materials[m].transmap_declared())
        .collect();
    assert_eq!(flags, [true, true, false, false]);
    let corner = [Vec3::ZERO, Vec3::X, Vec3::Y];
    let data = MeshData {
        positions: corner.repeat(4),
        normals: vec![Vec3::Z; 12],
        uvs: vec![glam::Vec2::ZERO; 12],
        indices: (0..12).collect(),
        ranges: (0..4).map(|s| (s * 3, 3, s)).collect(),
        ..Default::default()
    };
    let mesh = renderer.add_mesh(&mut scene, &data);
    let i = renderer.add_instance(
        &mut scene,
        mesh,
        DVec3::ZERO,
        Mat4::IDENTITY,
        vec![declared, bound, metal, plain],
    );
    renderer.set_params(&mut scene, i, &[0.0, 0.0, 0.35, 0.35], true, &[]);
    assert_eq!(scene.instances[i].slot_alpha, vec![1.0, 1.0, 0.35, 0.35]);
}

#[test]
fn omsi_render_phases_are_monotonic_and_complete() {
    assert_eq!(
        RenderPhase::DRAW_ORDER.map(|phase| phase as usize),
        [0, 1, 2, 3, 4, 5, 6, 7, 8]
    );
    assert_eq!(RenderPhase::default(), RenderPhase::Normal);
}

#[test]
fn metric_lifted_surfaces_keep_shading_class_without_view_space_pull() {
    assert_eq!(surface_instance_code(false, false, false, true, false, false), 0.9);
    assert_eq!(surface_instance_code(false, false, false, true, true, false), 1.0);
    assert_eq!(surface_instance_code(false, false, true, false, false, false), 0.9);
    assert_eq!(surface_instance_code(false, true, false, true, false, false), 0.75);
    // a road's painted lines go over the crossings laid on the road (#1694)
    assert_eq!(surface_instance_code(false, false, false, true, true, true), 1.4);
    assert_eq!(surface_instance_code(false, false, true, true, true, true), 1.25);
}

#[test]
fn spline_blend_sort_ignores_height_and_camera_pitch() {
    let origin = DVec3::new(120.0, 45.0, 0.0);
    let render_origin = DVec3::new(100.0, 40.0, 0.0);
    let level_camera = Vec3::new(3.0, 1.0, 4.0);
    let high_camera = Vec3::new(3.0, 1.0, 80.0);
    assert_eq!(
        horizontal_sort_distance(origin, render_origin, level_camera),
        horizontal_sort_distance(origin + DVec3::Z * 60.0, render_origin, high_camera)
    );
}

#[test]
fn render_scale_auto_keeps_ordinary_windows_sharp() {
    // the default window and a 2560x1080 screen are drawn at full size
    assert_eq!(scene_scale_for(0.0, 1600, 900), 1.0);
    assert_eq!(scene_scale_for(0.0, 2560, 1080), 1.0);
    // Mac/Android cap this at 2.8 million pixels; desktops keep it at full size.
    let s = scene_scale_for(0.0, 3200, 1800);
    if cfg!(target_os = "macos") || cfg!(target_os = "android") {
        assert!((s - 0.697).abs() < 0.01, "{s}");
        assert!((3200.0 * s * 1800.0 * s - AUTO_SCALE_PIXELS).abs() < 1.0);
    } else {
        assert_eq!(s, 1.0);
    }
    // never below half size
    assert_eq!(scene_scale_for(0.0, 16384, 16384), 0.5);
    // what is asked for, within 0.5..1 - on a Mac or a phone a scale over the pixel
    // budget is capped like the automatic one
    let s = scene_scale_for(0.75, 3200, 1800);
    if cfg!(target_os = "macos") || cfg!(target_os = "android") {
        assert!((s - 0.697).abs() < 0.01, "{s}");
        assert!((3200.0 * s * 1800.0 * s - AUTO_SCALE_PIXELS).abs() < 1.0);
    } else {
        assert_eq!(s, 0.75);
    }
    // a small window under the budget keeps what it asked for
    assert_eq!(scene_scale_for(0.3, 1600, 900), 0.5);
    assert_eq!(scene_scale_for(1.4, 1600, 900), 1.0);
}

#[test]
fn surface_depth_coverage_excludes_glass_and_terrain_masks() {
    assert!(surface_depth_coverage(RenderPhase::Spline, AlphaMode::Blend, false, false));
    for phase in RenderPhase::DRAW_ORDER {
        if !world_surface_phase(phase) {
            assert!(!surface_depth_coverage(phase, AlphaMode::Blend, false, false));
        } else {
            assert!(surface_depth_coverage(phase, AlphaMode::Blend, false, false));
        }
    }
    assert!(!surface_depth_coverage(RenderPhase::Spline, AlphaMode::Blend, true, false));
    assert!(!surface_depth_coverage(RenderPhase::Spline, AlphaMode::Blend, false, true));
    assert!(!surface_depth_coverage(RenderPhase::Spline, AlphaMode::Opaque, false, false));
    assert!(!surface_depth_coverage(RenderPhase::Spline, AlphaMode::Test, false, false));
    let order = RenderPhase::DRAW_ORDER;
    assert!(order.iter().position(|p| *p == RenderPhase::OnSurface).unwrap()
        < order.iter().position(|p| *p == RenderPhase::BeforeNormal).unwrap());
}

#[test]
fn pipeline_codes_cover_the_table() {
    // every kind x culling x depth bias has its own pipeline, and the opaque and
    // alpha-tested codes sort before the blended ones
    let mut seen = std::collections::HashSet::new();
    for kind in 0..PIPE_KINDS {
        for cull in [false, true] {
            for surface in [false, true] {
                let code = pipe_code(kind, cull, surface);
                assert!((code as usize) < PIPE_KINDS as usize * 4);
                assert!(seen.insert(code));
                assert_eq!(
                    code < pipe_code(PIPE_BLEND, false, false),
                    kind < PIPE_BLEND
                );
            }
        }
    }
    assert_eq!(seen.len(), PIPE_KINDS as usize * 4);
    assert_eq!(pipe_code(PIPE_OPAQUE, false, false), 0);
    assert!(
        pipe_code(PIPE_ALPHA_TEST, true, true) < pipe_code(PIPE_BLEND_NO_WRITE, false, false)
    );
}

#[test]
fn nearest_by_origin_picks_the_closest_mesh_of_each_object() {
    // A long bus (one origin) has a window 3 m from the camera and another 14 m away;
    // ranking the whole object by its shared origin (here 8 m) instead of its nearest
    // blended mesh is exactly what let a car beside the near window sort as farther
    // away than the bus and disappear behind it.
    let bus = DVec3::new(0.0, 0.0, 0.0);
    let car = DVec3::new(1.0, 0.0, 0.0);
    let by_origin = nearest_by_origin([(bus, 14.0), (bus, 3.0), (car, 8.0)]);
    let bus_dist = by_origin[&origin_key(bus)];
    let car_dist = by_origin[&origin_key(car)];
    assert_eq!(
        bus_dist, 3.0,
        "the object's distance is its nearest mesh, not the first or an average"
    );
    assert_eq!(car_dist, 8.0);
    // the car (8 m) is nearer than the bus's near window (3 m) is far: with the old
    // origin-only distance (bus origin 8.5 m, say) the two could tie or invert
    assert!(car_dist > bus_dist);
    assert_eq!(
        by_origin.len(),
        2,
        "one entry per distinct origin, not per mesh"
    );
}

#[test]
fn origin_lookup_keeps_float_equality_and_large_coordinates() {
    let a = DVec3::new(-0.0, 1_000_000.000_001, 2.0);
    let b = DVec3::new(0.0, a.y, 2.0);
    let c = DVec3::new(0.0, a.y + 0.000_001, 2.0);
    let distances = nearest_by_origin([(a, 8.0), (b, 3.0), (c, 5.0), (DVec3::NAN, 1.0)]);
    assert_eq!(distances.len(), 2);
    assert_eq!(distances[&origin_key(a)], 3.0);
    assert_eq!(distances[&origin_key(c)], 5.0);
}

/// Every shader module parses and validates as the device will see it, translates to
/// Metal and Vulkan SPIR-V, and its uniform structs are laid out as the Rust side writes them.
#[test]
fn shaders_validate_and_match_the_uniforms() {
    use wgpu::naga;
    let modules = [
        ("scene", scene_shader_source(false)),
        ("scene opaque", scene_shader_source(false)),
        ("scene terrain paint", scene_shader_source(false)),
        ("sky", sky_shader_source()),
        ("corona", corona_shader_source()),
        ("fog lamps", fog_lamps_shader_source()),
        ("snow", snow_shader_source()),
        ("rain", rain_shader_source()),
        ("rain basic", lean_rain(precip_shader_source(include_str!("../rain.wgsl")))),
        ("post", include_str!("../post.wgsl").to_string()),
        ("ssao", include_str!("../ssao.wgsl").to_string()),
        ("puddles", puddles::shader_source()),
        ("upscale", include_str!("../upscale.wgsl").to_string()),
        ("mip", include_str!("../mip.wgsl").to_string()),
        ("xr_ui", include_str!("../xr_ui.wgsl").to_string()),
        ("lamp sight", include_str!("../lamp_sight.wgsl").to_string()),
        // Enhanced+: the scene's pass with its reflection targets, the ray tracing
        ("scene (Enhanced+)", scene_shader_source(false).replace("//RT ", "")),
        ("ray-traced lighting", rt::lighting_source()),
        ("ray-traced reflections", rt::reflect_source()),
        ("texture means", include_str!("../rt_avg.wgsl").to_string()),
        ("rt texture coordinates", include_str!("../rt_uv.wgsl").to_string()),
    ];
    let sizes: &[(&str, usize)] = &[
        ("Enhanced", std::mem::size_of::<EnhancedUniform>()),
        ("PostParams", std::mem::size_of::<PostUniform>()),
        ("FogLampParams", std::mem::size_of::<FogLampUniform>()),
        ("SnowParams", std::mem::size_of::<SnowUniform>()),
        ("RainParams", std::mem::size_of::<RainUniform>()),
        ("PuddleParams", std::mem::size_of::<puddles::Uniform>()),
        ("Sight", std::mem::size_of::<SightUniform>()),
        ("VehicleReflection", std::mem::size_of::<puddles::VehicleUniform>()),
        ("PointLight", std::mem::size_of::<GpuPointLight>()),
        ("Camera", std::mem::size_of::<CameraUniform>()),
        ("MaterialParams", std::mem::size_of::<MaterialUniform>()),
        ("RtParams", rt::PARAMS_SIZE),
        ("Record", rt::RECORD_SIZE),
    ];
    let mut checked = std::collections::HashSet::new();
    for (name, src) in &modules {
        let module = naga::front::wgsl::parse_str(src)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        // Exercise the painted-ground specialization as well as the default shader.
        let mut constants = naga::back::PipelineConstants::default();
        if name.starts_with("scene ") {
            constants.insert("ALPHA_TEST".into(), 0.0);
        }
        if *name == "scene terrain paint" {
            constants.insert("TERRAIN_PAINT".into(), 1.0);
        }
        let (module, info) = naga::back::pipeline_constants::process_overrides(
            &module,
            &info,
            None,
            &constants,
        )
        .unwrap_or_else(|e| panic!("{name}: overrides: {e:?}"));
        let (module, info) = (module.into_owned(), info.into_owned());
        #[cfg(target_os = "macos")]
        let options = naga::back::msl::Options {
            lang_version: (2, 4),
            ..Default::default()
        };
        #[cfg(target_os = "macos")]
        naga::back::msl::write_string(
            &module,
            &info,
            &options,
            &naga::back::msl::PipelineOptions::default(),
        )
        .unwrap_or_else(|e| panic!("{name}: Metal: {e:?}"));
        for entry in &module.entry_points {
            let pipeline = naga::back::spv::PipelineOptions {
                shader_stage: entry.stage,
                entry_point: entry.name.clone(),
            };
            naga::back::spv::write_vec(&module, &info, &Default::default(), Some(&pipeline))
                .unwrap_or_else(|e| panic!("{name}/{}: Vulkan: {e:?}", entry.name));
        }
        let mut layouter = naga::proc::Layouter::default();
        layouter.update(module.to_ctx()).expect("layout");
        for (ty_name, rust) in sizes {
            if *ty_name == "Camera" && *name == "corona" {
                // Corona owns just the prefix through `cam_up`.  It must still include
                // every member in that prefix: a missing `world_origin`, for example,
                // makes a subsequently-added lighting use read the wrong vec4.
                if let Some((h, _)) = module
                    .types
                    .iter()
                    .find(|(_, t)| t.name.as_deref() == Some(*ty_name))
                {
                    assert_eq!(
                        layouter[h].size as usize,
                        std::mem::offset_of!(CameraUniform, clouds),
                        "corona: Camera prefix"
                    );
                }
                continue;
            }
            if let Some((h, _)) = module
                .types
                .iter()
                .find(|(_, t)| t.name.as_deref() == Some(*ty_name))
            {
                assert_eq!(layouter[h].size as usize, *rust, "{name}: {ty_name}");
                checked.insert(*ty_name);
            }
        }
    }
    assert_eq!(checked.len(), sizes.len(), "structs checked: {checked:?}");
}

/// The scene module as the OpenGL backend gets it translates to GLSL ES 3.10 and 3.20 and
/// desktop GLSL 4.30 for every entry point, the reduced module of the basic pipelines too,
/// with no `invariant gl_FragCoord` (rejected by AMD's desktop GL and by GLES), no texture
/// read through two samplers and no constant array indexed by an element of another
/// (#617, #610, #2009).
#[test]
fn the_scene_shader_translates_to_glsl() {
    use wgpu::naga;
    use wgpu::naga::back::glsl;
    for (basic, src) in [(false, scene_shader_source(true)), (true, lean_scene(scene_shader_source(true)))] {
        let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
        let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
            .validate(&module)
            .expect("validate");
        let constants = [("ALPHA_TEST".into(), 0.0), ("TERRAIN_PAINT".into(), 1.0)]
            .into_iter().collect();
        let versions = [
            glsl::Version::Embedded { version: 310, is_webgl: false },
            glsl::Version::Embedded { version: 320, is_webgl: false },
            glsl::Version::Desktop(430),
        ];
        for version in versions {
            let options = glsl::Options { version, ..Default::default() };
            for entry in &module.entry_points {
                // (as wgpu's OpenGL backend does it: the module cut down to the one entry point)
                let (module, info) = naga::back::pipeline_constants::process_overrides(
                    &module, &info, Some((entry.stage, &entry.name)), &constants,
                )
                .expect("overrides");
                let pipeline = glsl::PipelineOptions {
                    shader_stage: entry.stage,
                    entry_point: entry.name.clone(),
                    multiview: None,
                };
                let mut out = String::new();
                glsl::Writer::new(&mut out, &module, &info, &options, &pipeline, Default::default())
                    .and_then(|mut w| w.write())
                    .unwrap_or_else(|e| panic!("basic {basic} {version:?} {}: {e:?}", entry.name));
                assert!(!out.contains("invariant gl_FragCoord"), "basic {basic} {version:?} {}", entry.name);
                assert_eq!(const_array_in_const_array(&out), None, "basic {basic} {version:?} {}", entry.name);
            }
        }
    }
    // the check finds the lookup Adreno rejected (0.2.20's sun shadow kernel)
    let old = "const int SHADOW_CORNERS[5] = int[5](5, 1, 7, 12, 15);\n\
        const vec2 SHADOW_OFFSETS[16] = vec2[16](vec2(0.0));\n\
        vec2 o_2 = ((SHADOW_OFFSETS[SHADOW_CORNERS[_e17]] * texel) * spread);";
    assert_eq!(const_array_in_const_array(old).as_deref(), Some("SHADOW_OFFSETS[SHADOW_CORNERS["));
    assert_eq!(const_array_in_const_array("const int A[2] = int[2](0, 1);\nfloat x = b[A[i]];"), None);
}

/// A GLSL constant array indexed by an element of another (`A[B[i]]`, both declared
/// `const`), if any. Adreno's GLES compiler (Adreno 710 among them) takes the inner lookup
/// for a constant it cannot fold and rejects the shader ("array field selection out of
/// range '-1'"): 0.2.20's sun shadow kernel did this and the scene pipelines failed on
/// OpenGL there (#2009).
fn const_array_in_const_array(glsl: &str) -> Option<String> {
    let names: Vec<&str> = glsl
        .lines()
        .filter_map(|l| {
            let rest = l.trim_start().strip_prefix("const ")?;
            let decl = rest.split('=').next()?;
            let name = decl.split_whitespace().nth(1)?;
            name.split_once('[').map(|(n, _)| n)
        })
        .collect();
    names.iter().flat_map(|a| names.iter().map(move |b| format!("{a}[{b}["))).find(|p| {
        // (the whole word: not `MY_A[` for `A[`)
        glsl.match_indices(p.as_str()).any(|(at, _)| {
            !glsl[..at].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        })
    })
}

/// The corona and smoke module (its smoke branch with the fade into the ground among
/// them) translates to the GLSL of the oldest OpenGL chips the renderer runs on, GLES 3.0
/// and GL 3.3, as to GLES 3.1 and GL 4.3.
#[test]
fn the_corona_shader_translates_to_glsl() {
    use wgpu::naga;
    use wgpu::naga::back::glsl;
    let src = corona_shader_source();
    let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(&module)
        .expect("validate");
    let (module, info) =
        naga::back::pipeline_constants::process_overrides(&module, &info, None, &Default::default()).expect("overrides");
    let versions = [
        glsl::Version::Embedded { version: 300, is_webgl: false },
        glsl::Version::Desktop(330),
        glsl::Version::Embedded { version: 310, is_webgl: false },
        glsl::Version::Desktop(430),
    ];
    for version in versions {
        let options = glsl::Options { version, ..Default::default() };
        let mut done = 0;
        for entry in module.entry_points.iter().filter(|e| ["vs_main", "fs_main", "fs_smoke", "fs_smoke_enhanced"].contains(&e.name.as_str())) {
            let pipeline = glsl::PipelineOptions { shader_stage: entry.stage, entry_point: entry.name.clone(), multiview: None };
            let mut out = String::new();
            glsl::Writer::new(&mut out, &module, &info, &options, &pipeline, Default::default())
                .and_then(|mut w| w.write())
                .unwrap_or_else(|e| panic!("{version:?} {}: {e:?}", entry.name));
            done += 1;
        }
        assert_eq!(done, 4, "{version:?}");
    }
}

/// Where the device cannot read storage buffers in a vertex shader, or none at all, the
/// scene module reads its arrays from textures: it validates and translates to the GLSL
/// of such chips (GLES 3.1 / GL 4.3 with storage for the lights, GLES 3.0 / GL 3.3
/// without), and nothing of it is a storage buffer where it must not be (#770, #316).
#[test]
fn the_scene_shader_reads_its_arrays_from_textures_without_vertex_storage() {
    use wgpu::naga;
    use wgpu::naga::back::glsl;
    for (path, versions) in [
        (ArrayPath::VertexTextures, [glsl::Version::Embedded { version: 310, is_webgl: false }, glsl::Version::Desktop(430)]),
        (ArrayPath::NoStorage, [glsl::Version::Embedded { version: 300, is_webgl: false }, glsl::Version::Desktop(330)]),
        (ArrayPath::LightTextures, [glsl::Version::Embedded { version: 300, is_webgl: false }, glsl::Version::Desktop(330)]),
    ] {
        let src = scene_shader_for(true, path);
        assert!(!src.contains("models[") && !src.contains("inst_params[") && !src.contains("draw_list["));
        assert_eq!(src.contains("var<storage"), path == ArrayPath::VertexTextures, "{path:?}");
        let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{path:?}: {}", e.emit_to_string(&src)));
        let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{path:?}: {e:?}"));
        let (module, info) =
            naga::back::pipeline_constants::process_overrides(&module, &info, None, &Default::default()).expect("overrides");
        for version in versions {
            let options = glsl::Options { version, ..Default::default() };
            for entry in &module.entry_points {
                let pipeline = glsl::PipelineOptions { shader_stage: entry.stage, entry_point: entry.name.clone(), multiview: None };
                let mut out = String::new();
                glsl::Writer::new(&mut out, &module, &info, &options, &pipeline, Default::default())
                    .and_then(|mut w| w.write())
                    .unwrap_or_else(|e| panic!("{path:?} {version:?} {}: {e:?}", entry.name));
                let vertex = entry.stage == naga::ShaderStage::Vertex;
                if vertex || path.no_storage() {
                    assert!(!out.contains(" buffer "), "{path:?} {version:?} {}: a storage block", entry.name);
                }
                if path.no_storage() {
                    assert!(!out.contains("textureGather"), "{path:?} {version:?} {}: textureGather", entry.name);
                }
                assert_eq!(const_array_in_const_array(&out), None, "{path:?} {version:?} {}", entry.name);
            }
        }
    }
    // the rewrite leaves names that only end alike alone, and nests
    assert_eq!(indexing_as_calls("a = my_models[1]; b = models[models[i + 1u] .x];", "models", "f"), "a = my_models[1]; b = f(f(i + 1u) .x);");
}

/// wgpu's OpenGL backend numbers the textures of all a pipeline's groups into sixteen
/// units: the scene's camera and material groups stay within them on every array path
/// there, and what is left out of the camera group for it no pipeline made there reads
/// (#1133, #1154, #1162, #1176).
#[test]
fn the_scene_groups_fit_the_sixteen_texture_units_of_opengl() {
    use wgpu::naga;
    let textures = |e: &[wgpu::BindGroupLayoutEntry]| e.iter().filter(|e| matches!(e.ty, wgpu::BindingType::Texture { .. })).count();
    let material = textures(&material_layout_entries());
    for path in [ArrayPath::Storage, ArrayPath::VertexTextures, ArrayPath::NoStorage] {
        let sixteen = path != ArrayPath::Storage;
        let n = textures(&camera_layout_entries(path, sixteen)) + material;
        assert!(n <= 16, "{path:?}: {n} textures");
    }
    let lamps = textures(&camera_layout_entries(ArrayPath::LightTextures, true)) + material;
    assert_eq!(lamps, 18);
    let sixteen = wgpu::DownlevelLimits { max_sampled_textures_per_pipeline_layout: 16, ..Default::default() };
    let angle = wgpu::DownlevelLimits { max_sampled_textures_per_pipeline_layout: 32, ..Default::default() };
    let gles = wgpu::Limits::downlevel_webgl2_defaults();
    assert!(!crate::device_selection::lamps_fit(&sixteen, &gles));
    assert!(crate::device_selection::lamps_fit(&angle, &gles));
    // (and nothing is left out where storage buffers carry the arrays)
    assert_eq!(textures(&camera_layout_entries(ArrayPath::Storage, false)) + material, 16);
    // the entry points that read a texture left out are the enhanced path's, the
    // probe's, the puddles' and the reflection pass's - none made there
    let left_out = |src: &str| -> Vec<String> {
        let module = naga::front::wgsl::parse_str(src).unwrap();
        let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all()).validate(&module).unwrap();
        let mut out = Vec::new();
        for (i, ep) in module.entry_points.iter().enumerate() {
            let used = info.get_entry_point(i);
            let reads = module.global_variables.iter().any(|(h, g)| {
                !used[h].is_empty() && g.binding.as_ref().is_some_and(|b| b.group == 0 && ENHANCED_CAMERA_TEXTURES.contains(&b.binding))
            });
            if reads {
                out.push(ep.name.clone());
            }
        }
        out
    };
    let only_enhanced = |name: &str| name.contains("enhanced") || name.contains("probe") || name.contains("puddle") || name.contains("reflections") || name == "fs_sky_cube";
    for path in [ArrayPath::VertexTextures, ArrayPath::NoStorage, ArrayPath::LightTextures] {
        let scene = arrays_as_textures(&scene_shader_text(true), path);
        for src in [scene, sky_shader_source(), corona_shader_source()] {
            for name in left_out(&src) {
                assert!(only_enhanced(&name), "{path:?}: {name} reads an enhanced texture");
            }
        }
    }
}

/// An array texture is written in the rest of a row, whole rows and the start of the
/// last row, and never past its end.
#[test]
fn array_texture_writes_cover_the_range_once() {
    let w = ARRAY_TEX_WIDTH;
    assert_eq!(array_tex_spans(0, 10, 4 * w as u64), vec![(0, 0, 10, 1)]);
    assert_eq!(array_tex_spans(w as u64 - 2, 5, 4 * w as u64), vec![(w - 2, 0, 2, 1), (0, 1, 3, 1)]);
    assert_eq!(array_tex_spans(5, 3 * w as u64, 4 * w as u64), vec![(5, 0, w - 5, 1), (0, 1, w, 2), (0, 3, 5, 1)]);
    assert_eq!(array_tex_spans(0, 2 * w as u64, 4 * w as u64), vec![(0, 0, w, 2)]);
    // past the end: cut
    assert_eq!(array_tex_spans(2 * w as u64 - 1, 10, 2 * w as u64), vec![(w - 1, 1, 1, 1)]);
    assert!(array_tex_spans(3 * w as u64, 10, 2 * w as u64).is_empty());
    for (at, n) in [(0u64, 1u64), (7, 9000), (2047, 2049), (4096, 4096)] {
        let covered: u64 = array_tex_spans(at, n, 64 * w as u64).iter().map(|s| s.2 as u64 * s.3 as u64).sum();
        assert_eq!(covered, n);
    }
}

#[test]
fn the_sky_is_recomputed_only_when_it_has_moved_on() {
    let a = atmosphere::SkyInput::default();
    assert!(!sky_input_differs(&a, &a));
    // the sun a hundredth of a degree on, a tint that drifts by a thousandth: the same sky
    let turn = |deg: f32| {
        glam::Quat::from_axis_angle(a.sun_dir.any_orthonormal_vector(), deg.to_radians())
            * a.sun_dir
    };
    assert!(!sky_input_differs(
        &a,
        &atmosphere::SkyInput {
            sun_dir: turn(0.01),
            tint: [Vec3::splat(1.001); 3],
            ..a
        }
    ));
    // a quarter of a degree, some rain, another envir.cfg: a new sky
    assert!(sky_input_differs(
        &a,
        &atmosphere::SkyInput {
            sun_dir: turn(0.25),
            ..a
        }
    ));
    assert!(sky_input_differs(
        &a,
        &atmosphere::SkyInput { rain: 0.3, ..a }
    ));
    assert!(sky_input_differs(
        &a,
        &atmosphere::SkyInput {
            tint: [Vec3::new(1.2, 1.0, 0.9), Vec3::ONE, Vec3::ONE],
            ..a
        }
    ));
}

#[test]
fn each_path_gets_its_own_headlights() {
    let lamp = PointLight {
        radius: 30.0,
        color: [1.0, 0.78, 0.46],
        core: 5.0,
        ..Default::default()
    };
    let stand_in = PointLight {
        radius: 18.0,
        intensity: 0.8,
        mode: LightMode::Vanilla,
        ..Default::default()
    };
    let spot = PointLight {
        radius: 60.0,
        intensity: 20.0,
        direction: Vec3::new(0.0, 2.0, -0.6),
        cone: [0.97, 0.82],
        core: 1.0,
        beam: 24.0,
        mode: LightMode::Enhanced,
        ..Default::default()
    };
    assert!(drawn_by(&lamp, false) && drawn_by(&lamp, true));
    assert!(drawn_by(&stand_in, false) && !drawn_by(&stand_in, true));
    assert!(!drawn_by(&spot, false) && drawn_by(&spot, true));
    assert!(!drawn_by(
        &PointLight {
            intensity: 0.0,
            ..lamp
        },
        true
    ));
    // a map light reads as before in the vanilla shader (position, radius, colour)
    let g = gpu_light(&lamp, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(g.pos, [1.0, 2.0, 3.0, 30.0]);
    assert_eq!(g.color, [1.0, 0.78, 0.46, 1.0]);
    assert_eq!(g.dir[3], -2.0);
    assert_eq!(g.extra[1..], [5.0, 0.0, 30.0]);
    // a spot: no radius for the vanilla shader, the cone and the range for the enhanced
    let g = gpu_light(&spot, Vec3::ZERO);
    assert_eq!(g.pos[3], 0.0);
    assert_eq!(g.extra[3], 60.0);
    assert!(
        (Vec3::from_slice(&g.dir[..3]).length() - 1.0).abs() < 1e-5
            && g.dir[3] == 0.82
            && g.extra[0] == 0.97
    );
    // a headlight's beam gain rides along; a plain lamp has none
    assert_eq!(g.extra[2], 24.0);
    assert_eq!(gpu_light(&lamp, Vec3::ZERO).extra[2], 0.0);
}

#[test]
fn metering_defaults_are_gentle() {
    // the light model's exposure leads: the metering corrects well under a stop either
    // way, so snow stays white and a night stays dark
    let m = meter_tuning();
    assert!(m[0] > 0.0 && m[0] < 0.6, "{m:?}");
    assert!(m[2] <= 2.5 && m[3] <= 2.0, "{m:?}");
    // a snow field metered 1.5 EV over the target is darkened by well under a stop
    let ev = ((m[1] - (m[1] + 1.5)) * m[0]).clamp(-m[2], m[3]);
    assert!(ev < 0.0 && ev >= -0.75, "{ev}");
}

#[test]
fn half_floats_read_back() {
    for v in [0.0f32, 1.0, -2.5, 0.5, 1e-3, -3.46] {
        assert!(
            (half_to_f32(atmosphere::f16_bits(v)) - v).abs() <= v.abs() * 1e-3 + 1e-6,
            "{v}"
        );
    }
}

#[test]
fn vehicle_box_contains_the_driver() {
    // an NL202: 2.5 x 12 x 3 m, the box centre 0.4 m ahead of the origin and 1.5 m up,
    // heading east; the driver sits 4.6 m ahead of the origin, 1.8 m up
    let bus = (
        DVec3::new(100.0, 200.0, 30.0),
        90.0,
        [2.5, 12.0, 3.0, 0.0, 0.4, 1.5],
    );
    assert!(point_in_vehicle_box(DVec3::new(104.6, 200.7, 31.8), &bus));
    // beside the bus, above it, behind it
    assert!(!point_in_vehicle_box(DVec3::new(104.6, 197.0, 31.8), &bus));
    assert!(!point_in_vehicle_box(DVec3::new(104.6, 200.0, 34.0), &bus));
    assert!(!point_in_vehicle_box(DVec3::new(93.0, 200.0, 31.0), &bus));
}

#[test]
fn opaque_and_transmapped_materials_ignore_dynamic_alpha() {
    assert_eq!(
        Renderer::clamp_slot_alpha(0.0, AlphaMode::Opaque, false),
        1.0
    );
    assert_eq!(
        Renderer::clamp_slot_alpha(0.35, AlphaMode::Opaque, false),
        1.0
    );
    assert_eq!(Renderer::clamp_slot_alpha(0.0, AlphaMode::Test, false), 1.0);
    assert_eq!(
        Renderer::clamp_slot_alpha(0.35, AlphaMode::Test, false),
        1.0
    );
    assert_eq!(
        Renderer::clamp_slot_alpha(0.85, AlphaMode::Blend, false),
        0.85
    );
    assert_eq!(Renderer::clamp_slot_alpha(0.0, AlphaMode::Blend, true), 1.0);
    assert_eq!(
        Renderer::clamp_slot_alpha(0.0, AlphaMode::Opaque, true),
        1.0
    );
    assert_eq!(Renderer::clamp_slot_alpha(0.0, AlphaMode::Test, true), 1.0);
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn transparent_led_layer_preserves_the_glow_mask() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    )).expect("test renderer");
    let mut scene = renderer.new_scene();
    let mesh = renderer.add_mesh(&mut scene, &MeshData {
        positions: vec![Vec3::new(-2.0, 4.0, -0.5), Vec3::new(2.0, 4.1, -0.5), Vec3::new(2.0, 4.1, 0.5), Vec3::new(-2.0, 4.0, 0.5)],
        normals: vec![-Vec3::Y; 4],
        uvs: vec![glam::Vec2::ZERO, glam::Vec2::X, glam::Vec2::ONE, glam::Vec2::Y],
        ranges: vec![(0, 6, 0)], indices: vec![0, 1, 2, 0, 2, 3], one_sided: false,
    });
    let background = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.2, 0.2, 0.2, 1.0], true);
    renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![background]);
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 90.0, near: 0.1, far: 100.0 };
    let lighting = Lighting { enhanced: true, shadows: false, fog_density: 0.0, ..Default::default() };
    let bare = renderer.render_to_image(&mut scene, 128, 128, &camera, &lighting).unwrap();
    let mask = renderer.add_texture(&mut scene, &omsi_texture::Image { width: 1, height: 1, rgba: vec![255, 255, 255, 0], has_alpha: true }, false);
    let material = renderer.add_material_extra(&mut scene, None, AlphaMode::Blend,
        [1.0; 4], false, Some((mask, true)), None, None, None, [0.0; 3],
        MaterialExtra { screen: true, led: true, ..Default::default() });
    renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
    let covered = renderer.render_to_image(&mut scene, 128, 128, &camera, &lighting).unwrap();
    let mut max_error = 0;
    for y in 60..68 {
        for x in 40..88 {
            let i = (y * 128 + x) * 4;
            max_error = max_error.max(bare[i].abs_diff(covered[i]));
        }
    }
    assert!(max_error <= 1, "invisible LED layer makes its background glow: max error {max_error}");
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn display_behind_a_pane_keeps_its_pixel_contrast() {
    let mut renderer = smoke_test_renderer();
    let mut scene = renderer.new_scene();
    let mesh = renderer.add_mesh(&mut scene, &MeshData {
        positions: vec![Vec3::new(-2.0, 4.0, -1.0), Vec3::new(2.0, 4.0, -1.0), Vec3::new(2.0, 4.0, 1.0), Vec3::new(-2.0, 4.0, 1.0)],
        normals: vec![-Vec3::Y; 4],
        uvs: vec![glam::Vec2::ZERO, glam::Vec2::X, glam::Vec2::ONE, glam::Vec2::Y],
        ranges: vec![(0, 6, 0)], indices: vec![0, 1, 2, 0, 2, 3], one_sided: false,
    });
    let picture = renderer.add_texture(&mut scene, &omsi_texture::Image {
        width: 64, height: 32, rgba: (0..32).flat_map(|y| (0..64).flat_map(move |x| {
            let c = if x % 4 == 0 || y % 7 == 0 { 220 } else { 0 };
            [c, c, c, 255]
        })).collect(), has_alpha: true,
    }, false);
    let pane = renderer.add_texture(&mut scene, &omsi_texture::Image {
        width: 1, height: 1, rgba: vec![0, 0, 0, 184], has_alpha: true,
    }, false);
    let glass = renderer.add_material_extra(&mut scene, Some(pane), AlphaMode::Blend,
        [1.0; 4], true, None, None, None, None, [0.0; 3],
        MaterialExtra { glass: true, no_z_write: true, ..Default::default() });
    let protected_glass = renderer.add_material_extra(&mut scene, Some(pane), AlphaMode::Blend,
        [1.0; 4], true, None, None, None, None, [0.0; 3],
        MaterialExtra { screen: true, glass: true, no_z_write: true, ..Default::default() });
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 90.0, near: 0.1, far: 100.0 };
    let lighting = Lighting { enhanced: true, shadows: false, fog_density: 0.0, ..Default::default() };
    for led in [false, true] {
        scene.instances.clear();
        let material = renderer.add_material_extra(&mut scene, Some(picture), AlphaMode::Blend,
            [1.0; 4], true, None, None, None, None, [0.0; 3],
            MaterialExtra { screen: true, led, ..Default::default() });
        renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
        let pane_instance = renderer.add_instance(&mut scene, mesh, DVec3::new(0.0, -0.1, 0.0), Mat4::IDENTITY, vec![glass]);
        renderer.options.fxaa = false;
        let plain = renderer.render_to_image(&mut scene, 128, 128, &camera, &lighting).unwrap();
        renderer.options.fxaa = true;
        let antialiased = renderer.render_to_image(&mut scene, 128, 128, &camera, &lighting).unwrap();
        let mut error = 0;
        for y in 55..73 {
            for x in 38..90 {
                let i = (y * 128 + x) * 4;
                for channel in 0..3 { error = error.max(plain[i + channel].abs_diff(antialiased[i + channel])); }
            }
        }
        assert!(error <= 1, "display blurred through its pane (LED={led}): {error}");
        // Identical clear-pane colour, with the display marker held above the mist
        // range: changing coverage of the marker must not add condensation blur.
        renderer.set_material(&mut scene, pane_instance, 0, protected_glass);
        let protected = renderer.render_to_image(&mut scene, 128, 128, &camera, &lighting).unwrap();
        let mut mist_error = 0;
        for y in 55..73 {
            for x in 38..90 {
                let i = (y * 128 + x) * 4;
                for channel in 0..3 { mist_error = mist_error.max(antialiased[i + channel].abs_diff(protected[i + channel])); }
            }
        }
        assert!(mist_error <= 1, "clear pane was mistaken for condensation (LED={led}): {mist_error}");
    }
}

#[test]
fn a_texture_the_card_refuses_is_drawn_plain_without_cutting_the_texture_budget() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let instance = wgpu::Instance::new(descriptor);
    let renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, render_scale: 1.0, ..Default::default() },
    )).expect("noop renderer");
    let _ = renderer.take_out_of_memory();
    let too_big = renderer.device.limits().max_texture_dimension_2d + 1;
    let desc = wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: too_big, height: 4, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    };
    let scope = renderer.device.push_error_scope(wgpu::ErrorFilter::Validation);
    assert!(crate::resources::try_create_texture(&renderer.device, &desc).is_none());
    let empty = wgpu::TextureDescriptor { size: wgpu::Extent3d { width: 1, height: 0, depth_or_array_layers: 1 }, ..desc.clone() };
    assert!(crate::resources::try_create_texture(&renderer.device, &empty).is_none());
    // the stand-in is a valid texture: a material made with it is no device error
    let mut scene = renderer.new_scene();
    let t = crate::resources::stand_in_texture(&renderer.device, &renderer.queue);
    assert_eq!((t.size, t.bytes), ((1, 1), 0));
    scene.textures.push(t);
    let id = scene.textures.len() - 1;
    let _ = renderer.add_material(&mut scene, Some(id), AlphaMode::Opaque, [1.0; 4], true);
    assert!(pollster::block_on(scope.pop()).is_none(), "nothing invalid reached the device");
    assert!(!renderer.take_out_of_memory(), "a refused texture is no shortage of graphics memory");
}

#[test]
fn the_lamps_light_the_scene_from_textures_where_storage_buffers_carry_nothing() {
    let storage = scene_shader_for(false, ArrayPath::Storage);
    assert_eq!(storage, scene_shader_text(false));
    assert_eq!(scene_shader_for(true, ArrayPath::Storage), scene_shader_text(true));
    let lamps = scene_shader_for(true, ArrayPath::LightTextures);
    let none = scene_shader_for(true, ArrayPath::NoStorage);
    assert!(lamps.contains("@group(0) @binding(3) var lights_tex: texture_2d<f32>;"));
    assert!(lamps.contains("@group(0) @binding(4) var grid_tex: texture_2d<u32>;"));
    assert!(!lamps.contains("lights[") && !lamps.contains("grid[") && !lamps.contains("var<storage"));
    assert!(none.contains("return 0xffffffffu;") && !none.contains("lights_tex"));
    assert_eq!(lamps.matches("lights_at(").count(), none.matches("lights_at(").count());
    assert_eq!(lamps.matches("grid_at(").count(), none.matches("grid_at(").count());
    let entries = camera_layout_entries(ArrayPath::LightTextures, true);
    for (binding, sample_type) in [(3, wgpu::TextureSampleType::Float { filterable: false }), (4, wgpu::TextureSampleType::Uint)] {
        let e = entries.iter().find(|e| e.binding == binding).expect("lights binding");
        assert_eq!(e.visibility, wgpu::ShaderStages::FRAGMENT);
        assert!(matches!(e.ty, wgpu::BindingType::Texture { sample_type: t, .. } if t == sample_type));
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_lamp_behind_a_wall_does_not_dazzle_and_one_in_view_still_does() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    ))
    .expect("test renderer");
    let mut scene = renderer.new_scene();
    let wall = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.5, 0.5, 0.5, 1.0], true);
    let mesh = renderer.add_mesh(
        &mut scene,
        &MeshData {
            positions: vec![Vec3::new(-6.0, -5.0, -1.0), Vec3::new(6.0, -5.0, -1.0), Vec3::new(6.0, 5.0, -1.0), Vec3::new(-6.0, 5.0, -1.0)],
            normals: vec![Vec3::Z; 4],
            uvs: vec![glam::Vec2::splat(0.5); 4],
            indices: vec![0, 1, 2, 0, 2, 3],
            ranges: vec![(0, 6, 0)],
            one_sided: false,
        },
    );
    renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![wall]);
    let (in_view, behind) = (DVec3::new(2.0, 0.0, 3.0), DVec3::new(-2.0, 0.0, -4.0));
    scene.lights = [in_view, behind]
        .map(|position| PointLight { position, radius: 20.0, core: 10.0, intensity: 1.0, ..Default::default() })
        .to_vec();
    let camera = Camera { position: DVec3::new(0.0, -0.105, 6.0), yaw: 0.0, pitch: -89.0, roll: 0.0, fov_deg: 90.0, near: 0.1, far: 100.0 };
    let night = Lighting { enhanced: true, sun_dir: -Vec3::Z, sun_intensity: 0.0, night: 1.0, shadows: false, detail: false, fog_density: 0.0, ..Default::default() };
    for _ in 0..2 {
        renderer.render_to_image(&mut scene, 64, 64, &camera, &night).unwrap();
    }
    let sight = renderer.tone.lamp_sight.as_ref().expect("lamp sight");
    assert!(sight.seen(in_view) > 0.9, "a lamp in plain view: {}", sight.seen(in_view));
    assert!(sight.seen(behind) < 0.1, "a lamp behind the wall: {}", sight.seen(behind));
}

#[test]
fn a_mesh_page_the_card_refuses_is_made_mesh_by_mesh_or_left_out() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let instance = wgpu::Instance::new(descriptor);
    let renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, render_scale: 1.0, ..Default::default() },
    )).expect("noop renderer");
    let mesh = |n: usize| {
        let positions: Vec<Vec3> = (0..n).map(|i| Vec3::new(i as f32, 0.0, 0.0)).collect();
        let indices: Vec<u32> = (0..n as u32 / 3 * 3).collect();
        MeshData { normals: vec![Vec3::Z; n], uvs: vec![glam::Vec2::ZERO; n], ranges: vec![(0, indices.len() as u32, 0)], positions, indices, ..Default::default() }
    };
    let _ = renderer.take_out_of_memory();
    let mut scene = renderer.new_scene();
    let scope = renderer.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let one = mesh_page_bytes(&mesh(300)).0;
    crate::resources::REFUSE_MESH_PAGES_OVER.with(|r| r.set(one));
    let alone = renderer.add_mesh(&mut scene, &mesh(300));
    let pair = prepare_meshes(&renderer.device, &renderer.queue, &[&mesh(300), &mesh(300)], renderer.mesh_pages());
    crate::resources::REFUSE_MESH_PAGES_OVER.with(|r| r.set(0));
    let refused = renderer.add_mesh(&mut scene, &mesh(300));
    let refused_pair = prepare_meshes(&renderer.device, &renderer.queue, &[&mesh(300), &mesh(300)], renderer.mesh_pages());
    crate::resources::REFUSE_MESH_PAGES_OVER.with(|r| r.set(u64::MAX));
    let pair: Vec<MeshId> = pair.into_iter().map(|m| renderer.add_prepared_mesh(&mut scene, m)).collect();
    let refused_pair: Vec<MeshId> = refused_pair.into_iter().map(|m| renderer.add_prepared_mesh(&mut scene, m)).collect();
    assert!(scene.mesh_pages.is_empty());
    assert!(scene.meshes[pair[0]].vertex_buf != scene.meshes[pair[1]].vertex_buf, "the refused pair split into pages of their own");
    for id in [alone, pair[0], pair[1]] {
        let m = &scene.meshes[id];
        assert_eq!((m.page, m.vertex_bytes, m.ranges.len()), (u32::MAX, one, 1), "made in a page of its own");
    }
    for id in [refused, refused_pair[0], refused_pair[1]] {
        let m = &scene.meshes[id];
        assert!(m.ranges.is_empty() && m.vertex_bytes == 0 && m.bounds_radius == 0.0, "left out");
        let mat = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0; 4], true);
        renderer.add_instance(&mut scene, id, DVec3::ZERO, Mat4::IDENTITY, vec![mat]);
        renderer.free_mesh(&mut scene, id);
    }
    assert!(pollster::block_on(scope.pop()).is_none(), "nothing invalid reached the device");
    assert!(!renderer.take_out_of_memory(), "a refused buffer is no shortage of graphics memory");
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_mesh_left_out_for_want_of_a_page_draws_nothing_and_the_rest_of_the_picture_is_drawn() {
    let camera = Camera { position: DVec3::new(0.0, 0.0, 5.0), yaw: 0.0, pitch: -90.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 100.0 };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, fxaa: false, shadow_size: 1024, render_scale: 1.0, ..Default::default() },
    )).expect("test renderer");
    let mut scene = renderer.new_scene();
    let quad = |z: f32| MeshData {
        positions: vec![Vec3::new(-4.0, -4.0, z), Vec3::new(4.0, -4.0, z), Vec3::new(4.0, 4.0, z), Vec3::new(-4.0, 4.0, z)],
        normals: vec![Vec3::Z; 4],
        uvs: vec![glam::Vec2::ZERO; 4],
        indices: vec![0, 1, 2, 0, 2, 3],
        ranges: vec![(0, 6, 0)],
        one_sided: false,
    };
    let red = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 0.0, 0.0, 1.0], true);
    let blue = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 0.0, 1.0, 1.0], true);
    crate::resources::REFUSE_MESH_PAGES_OVER.with(|r| r.set(0));
    let left_out = renderer.add_mesh(&mut scene, &quad(1.0));
    crate::resources::REFUSE_MESH_PAGES_OVER.with(|r| r.set(u64::MAX));
    renderer.add_instance(&mut scene, left_out, DVec3::ZERO, Mat4::IDENTITY, vec![blue]);
    let drawn = renderer.add_mesh(&mut scene, &quad(0.0));
    renderer.add_instance(&mut scene, drawn, DVec3::ZERO, Mat4::IDENTITY, vec![red]);
    let lighting = Lighting { fog_density: 0.0, ..Default::default() };
    let scope = renderer.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let image = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
    assert!(pollster::block_on(scope.pop()).is_none(), "the frame reached the device whole");
    let centre = (32 * 64 + 32) * 4;
    assert!(image[centre] > 2 * image[centre + 1].max(image[centre + 2]).max(8), "the red quad is drawn, the left out one is not: {:?}", &image[centre..centre + 4]);
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn the_reflection_strength_scales_what_an_envmapped_surface_mirrors() {
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 100.0 };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    )).expect("test renderer");
    let mut scene = renderer.new_scene();
    let red = renderer.add_texture(&mut scene, &omsi_texture::Image { width: 4, height: 4, rgba: [255, 0, 0, 255].repeat(16), has_alpha: false }, false);
    let grey = renderer.add_texture(&mut scene, &omsi_texture::Image { width: 1, height: 1, rgba: vec![60, 60, 60, 255], has_alpha: false }, false);
    let paint = renderer.add_material_env(&mut scene, Some(grey), AlphaMode::Opaque, [1.0; 4], false, None, None, None, Some((red, 1.0)));
    let mesh = renderer.add_mesh(&mut scene, &MeshData {
        positions: vec![Vec3::new(-3.0, 5.0, -3.0), Vec3::new(3.0, 5.0, -3.0), Vec3::new(3.0, 5.0, 3.0), Vec3::new(-3.0, 5.0, 3.0)],
        normals: vec![-Vec3::Y; 4],
        uvs: vec![glam::Vec2::splat(0.5); 4],
        ranges: vec![(0, 6, 0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        one_sided: false,
    });
    renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![paint]);
    for (mode, classic, enhanced) in [("vanilla", true, false), ("vanilla+", false, false), ("enhanced", false, true)] {
        let mut centre = |strength: f32| {
            let lighting = Lighting { shadows: false, fog_density: 0.0, classic, enhanced, reflection_strength: strength, ..Default::default() };
            let rgba = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
            [0, 1, 2].map(|c| rgba[(32 * 64 + 32) * 4 + c] as i32)
        };
        let off = centre(0.0);
        let away = |p: [i32; 3]| (0..3).map(|c| (p[c] - off[c]).abs()).sum::<i32>();
        let (full, half) = (away(centre(1.0)), away(centre(0.5)));
        assert!(full > half + 4 && half > 2, "{mode}: change from 0 % at 100 / 50 %: {full} {half} (0 %: {off:?})");
        if !enhanced {
            assert!((off[0] - off[1]).abs() <= 2, "{mode}: nothing mirrored at 0 %: {off:?}");
        }
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn envmapped_paint_with_normals_off_its_faces_mirrors_no_more_than_with_sound_ones() {
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 100.0 };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    )).expect("test renderer");
    let mut scene = renderer.new_scene();
    let blue = renderer.add_texture(&mut scene, &omsi_texture::Image { width: 4, height: 4, rgba: [20, 40, 160, 255].repeat(16), has_alpha: false }, false);
    let photo = renderer.add_texture(&mut scene, &omsi_texture::Image { width: 4, height: 4, rgba: [200, 200, 200, 255].repeat(16), has_alpha: false }, false);
    let paint = renderer.add_material_env(&mut scene, Some(blue), AlphaMode::Opaque, [1.0; 4], false, None, None, None, Some((photo, 0.1)));
    let mut mirrored = |normal: Vec3| {
        let mesh = renderer.add_mesh(&mut scene, &MeshData {
            positions: vec![Vec3::new(-3.0, 5.0, -3.0), Vec3::new(3.0, 5.0, -3.0), Vec3::new(3.0, 5.0, 3.0), Vec3::new(-3.0, 5.0, 3.0)],
            normals: vec![normal.normalize(); 4],
            uvs: vec![glam::Vec2::splat(0.5); 4],
            ranges: vec![(0, 6, 0)],
            indices: vec![0, 1, 2, 0, 2, 3],
            one_sided: false,
        });
        let i = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![paint]);
        let [none, full] = [0.0, 1.0].map(|strength| {
            let lighting = Lighting { shadows: false, fog_density: 0.0, enhanced: true, reflection_strength: strength, ..Default::default() };
            let rgba = renderer.render_to_image(&mut scene, 64, 64, &camera, &lighting).unwrap();
            [0, 1, 2].map(|c| rgba[(32 * 64 + 32) * 4 + c] as i32)
        });
        renderer.remove_instance(&mut scene, i);
        (0..3).map(|c| (full[c] - none[c]).abs()).sum::<i32>()
    };
    let sound = mirrored(-Vec3::Y);
    let off = mirrored(Vec3::new(1.0, -0.15, 0.0));
    assert!(sound >= 4, "the paint mirrors nothing to compare with: {sound}");
    assert!(off <= 2 * sound + 6, "normals along the face mirrored {off} where sound ones mirror {sound}");
}
