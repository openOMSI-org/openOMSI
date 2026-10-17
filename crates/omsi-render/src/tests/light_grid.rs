use super::*;

#[test]
fn a_light_takes_only_the_grid_cells_its_range_reaches() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let instance = wgpu::Instance::new(descriptor);
    let renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, render_scale: 1.0, ..Default::default() },
    )).expect("noop renderer");
    let mut scene = renderer.new_scene();
    let (grid, _) = renderer.prepare_lights(&mut scene, Vec3::ZERO, false, false, None);
    let at = DVec3::new(grid[0] as f64 + 10.5 * 25.0, grid[1] as f64 + 10.5 * 25.0, 5.0);
    scene.lights = vec![PointLight { position: at, radius: 40.0, color: [1.0; 3], mode: LightMode::Both, ..Default::default() }];
    let (grid, _) = renderer.prepare_lights(&mut scene, Vec3::ZERO, false, false, None);
    let side = grid[3] as usize;
    let lit: Vec<(usize, usize)> = (0..side * side).filter(|&c| scene.grid_counts[c] > 0).map(|c| (c % side, c / side)).collect();
    assert_eq!(lit.len(), 21);
    assert!(lit.contains(&(12, 10)) && lit.contains(&(11, 12)) && !lit.contains(&(12, 12)) && !lit.contains(&(8, 8)));
}
