use super::*;

#[test]
fn a_light_takes_only_the_grid_cells_its_range_reaches() {
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
            render_scale: 1.0,
            ..Default::default()
        },
    ))
    .expect("noop renderer");
    let mut scene = renderer.new_scene();
    let (grid, _) = renderer.prepare_lights(&mut scene, &LightEye::at(Vec3::ZERO), false, false);
    let at = DVec3::new(
        grid[0] as f64 + 10.5 * 25.0,
        grid[1] as f64 + 10.5 * 25.0,
        5.0,
    );
    scene.lights = vec![PointLight {
        position: at,
        radius: 40.0,
        color: [1.0; 3],
        mode: LightMode::Both,
        ..Default::default()
    }];
    let (grid, _) = renderer.prepare_lights(&mut scene, &LightEye::at(Vec3::ZERO), false, false);
    let side = grid[3] as usize;
    let lit: Vec<(usize, usize)> = (0..side * side)
        .filter(|&c| scene.grid_counts[c] > 0)
        .map(|c| (c % side, c / side))
        .collect();
    assert_eq!(lit.len(), 21);
    assert!(
        lit.contains(&(12, 10))
            && lit.contains(&(11, 12))
            && !lit.contains(&(12, 12))
            && !lit.contains(&(8, 8))
    );
}

/// A light-mapped road takes no light from the map's lamps, so the classic shader stops at the
/// first one in a cell. Spots and headlight stand-ins must be ahead of them, even when the
/// scene lists them last (#3723).
#[test]
fn lights_that_shine_on_light_mapped_roads_are_listed_first() {
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
            render_scale: 1.0,
            ..Default::default()
        },
    ))
    .expect("noop renderer");
    let mut scene = renderer.new_scene();
    let (grid, _) = renderer.prepare_lights(&mut scene, &LightEye::at(Vec3::ZERO), false, false);
    let at = DVec3::new(
        grid[0] as f64 + 10.5 * 25.0,
        grid[1] as f64 + 10.5 * 25.0,
        5.0,
    );
    // Distinguishable colours tell which light a cell's GPU index actually names: the
    // indices are renumbered as the lights are uploaded, in the order they are walked.
    scene.lights = vec![
        PointLight {
            position: at,
            radius: 40.0,
            color: [0.1, 0.0, 0.0], // a map lamp (housed)
            housed: true,
            mode: LightMode::Vanilla,
            ..Default::default()
        },
        PointLight {
            position: at,
            radius: 40.0,
            color: [0.2, 0.0, 0.0], // a vehicle spot
            direction: Vec3::Y,
            d3d_spot: Some(D3dSpot {
                range: 100.0,
                source: at,
                player: false,
            }),
            ..Default::default()
        },
        PointLight {
            position: at,
            radius: 40.0,
            color: [0.3, 0.0, 0.0], // a classic headlight stand-in
            mode: LightMode::Vanilla,
            ..Default::default()
        },
    ];
    let (grid, _) = renderer.prepare_lights(&mut scene, &LightEye::at(Vec3::ZERO), false, false);
    let side = grid[3] as usize;
    let cell = 10 * side + 10;
    let base = cell * LIGHT_CELL_CAP;
    assert_eq!(scene.grid_counts[cell], 3);
    let stride = std::mem::size_of::<GpuPointLight>();
    let color_at = std::mem::offset_of!(GpuPointLight, color);
    let red = |idx: usize| {
        let at = idx * stride + color_at;
        f32::from_le_bytes(scene.last_lights[at..at + 4].try_into().unwrap())
    };
    // the spot and the headlight stand-in were listed after the map lamp in the scene, but
    // the cell walks them first, in their camera-distance order
    assert_eq!(red(scene.grid_scratch[base] as usize), 0.2);
    assert_eq!(red(scene.grid_scratch[base + 1] as usize), 0.3);
    assert_eq!(red(scene.grid_scratch[base + 2] as usize), 0.1);
}

#[test]
fn full_cell_eviction_keeps_classic_road_lights_ahead_of_map_lamps() {
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
            render_scale: 1.0,
            ..Default::default()
        },
    ))
    .expect("noop renderer");
    let mut scene = renderer.new_scene();
    let (grid, _) = renderer.prepare_lights(&mut scene, &LightEye::at(Vec3::ZERO), false, false);
    let at = DVec3::new(
        grid[0] as f64 + 10.5 * 25.0,
        grid[1] as f64 + 10.5 * 25.0,
        5.0,
    );
    // Fill a vanilla cell (cap 8): first 4 classic_road_lights get f32::MAX, the next 4 get
    // normal strength. Give the 5th (index 4) the lowest intensity so a bright map lamp
    // evicts index 4 in place while indices 5..8 survive.
    scene.lights = (0..VANILLA_CELL_CAP)
        .map(|i| PointLight {
            position: at,
            radius: 40.0,
            intensity: if i == VANILLA_CELL_CAP / 2 { 0.1 } else { 1.0 },
            color: [0.2, 0.0, 0.0],
            mode: LightMode::Vanilla,
            ..Default::default()
        })
        .collect();
    scene.lights.push(PointLight {
        position: at,
        radius: 40.0,
        intensity: 100.0,
        color: [0.9, 0.0, 0.0],
        housed: true,
        mode: LightMode::Vanilla,
        ..Default::default()
    });
    let (grid, _) = renderer.prepare_lights(&mut scene, &LightEye::at(Vec3::ZERO), false, false);
    let side = grid[3] as usize;
    let cell = 10 * side + 10;
    let base = cell * LIGHT_CELL_CAP;
    assert_eq!(scene.grid_counts[cell], VANILLA_CELL_CAP as u8);
    let stride = std::mem::size_of::<GpuPointLight>();
    let color_at = std::mem::offset_of!(GpuPointLight, color);
    let red = |idx: usize| {
        let at = idx * stride + color_at;
        f32::from_le_bytes(scene.last_lights[at..at + 4].try_into().unwrap())
    };
    let reds: Vec<f32> = (0..VANILLA_CELL_CAP)
        .map(|k| red(scene.grid_scratch[base + k] as usize))
        .collect();
    assert_eq!(reds[VANILLA_CELL_CAP - 1], 0.9);
    assert!(reds[..VANILLA_CELL_CAP - 1].iter().all(|&r| r == 0.2));
}
