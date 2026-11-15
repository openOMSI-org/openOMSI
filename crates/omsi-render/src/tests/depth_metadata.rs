use super::*;

#[test]
fn stacked_depth_slant_uses_the_vertex_normal() {
    let shader = include_str!("../shaders/scene/03_vertex.wgsl");
    assert!(shader.contains("let plane = max(abs(dot(safe_normal(nw), cp - camera.cam_pos.xyz))"));
    assert!(shader.contains("let level_code = u32(round(models[e * 4u].w));"));
    assert!(!shader.contains("let encoded = vec2<f32>(models[e * 4u + 1u].w"));
}

#[test]
fn deforming_a_mesh_does_not_encode_unused_depth_normals() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let adapter = wgpu::Instance::new(descriptor);
    let renderer = pollster::block_on(Renderer::new_with(
        &adapter, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, shadow_size: 1024, ..Default::default() },
    )).expect("no-op renderer");
    let mut scene = renderer.new_scene();
    let data = MeshData {
        positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
        normals: vec![Vec3::Z; 3], uvs: vec![glam::Vec2::ZERO; 3],
        indices: vec![0, 1, 2], ranges: vec![(0, 3, 0)], ..Default::default()
    };
    let mesh = renderer.add_mesh(&mut scene, &data);
    let instance = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![0]);
    scene.instances[instance].slot_levels[0] = 1;
    renderer.prepare(&mut scene);
    assert_eq!([scene.cpu_models[0][1][3], scene.cpu_models[0][2][3]], [0.0, 0.0]);
    let mut positions = data.positions.clone();
    positions[2].z = 0.2;
    renderer.update_mesh(&mut scene, mesh, &positions, &data.normals, &data.uvs);
    renderer.prepare(&mut scene);
    assert_eq!([scene.cpu_models[0][1][3], scene.cpu_models[0][2][3]], [0.0, 0.0]);
}
