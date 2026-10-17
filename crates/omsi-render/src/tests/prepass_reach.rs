use super::*;

fn quad(renderer: &mut Renderer, scene: &mut Scene, y: f32, half: f32) -> MeshId {
    renderer.add_mesh(scene, &MeshData {
        positions: vec![Vec3::new(-half, y, -half), Vec3::new(half, y, -half),
            Vec3::new(half, y, half), Vec3::new(-half, y, half)],
        normals: vec![-Vec3::Y; 4], uvs: vec![glam::Vec2::ZERO; 4],
        indices: vec![0, 1, 2, 0, 2, 3], ranges: vec![(0, 6, 0)],
        ..Default::default()
    })
}

fn batches(far_at: f32) -> (f64, f64) {
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
    let near = quad(&mut renderer, &mut scene, 4.0, 2.0);
    let far = quad(&mut renderer, &mut scene, far_at, 50.0);
    renderer.add_instance(&mut scene, near, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
    renderer.add_instance(&mut scene, far, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0,
        fov_deg: 90.0, near: 0.1, far: 1000.0 };
    let lighting = Lighting { enhanced: false, shadows: false, fog_density: 0.0, ..Default::default() };
    renderer.render_to_image(&mut scene, 16, 16, &camera, &lighting).unwrap();
    let counts = renderer.counts.borrow();
    (counts.get("prepass batches").copied().unwrap_or(0.0), counts.get("main batches").copied().unwrap_or(0.0))
}

#[test]
fn the_ambient_occlusions_own_prepass_leaves_out_what_lies_past_its_reach() {
    assert_eq!(batches(200.0), (2.0, 2.0));
    assert_eq!(batches(400.0), (1.0, 2.0));
}
