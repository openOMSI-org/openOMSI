use super::*;

#[test]
fn blended_screen_materials_do_not_write_depth() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let instance = wgpu::Instance::new(descriptor);
    let renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, render_scale: 1.0, ..Default::default() },
    ))
    .expect("noop renderer");
    let mut scene = renderer.new_scene();
    let screen = renderer.add_material_extra(
        &mut scene,
        None,
        AlphaMode::Blend,
        [1.0; 4],
        true,
        None,
        None,
        None,
        None,
        [0.0; 3],
        MaterialExtra { screen: true, ..Default::default() },
    );

    assert!(scene.materials[screen].no_z_write);
    assert!(!scene.materials[screen].writes_depth);
    assert!(crate::frame_plan::depth_prepass_kind(
        crate::frame_plan::PIPE_BLEND,
        &scene.materials[screen],
        false,
    )
    .is_none());
}

#[test]
fn stacked_depth_caps_keep_the_two_step_floor() {
    let shader = include_str!("../shaders/scene/03_vertex.wgsl");
    assert!(shader.contains("let stack_floor = select(0.0, 2.0, (level_code & 3u) >= 2u);"));
    assert!(shader.contains("level = max(level, stack_floor);"));
}

#[test]
fn uploaded_depth_normal_comes_from_geometry_not_lighting_normals() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let adapter = wgpu::Instance::new(descriptor);
    let renderer = pollster::block_on(Renderer::new_with(
        &adapter,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, shadow_size: 1024, ..Default::default() },
    )).expect("no-op renderer");
    let mut scene = renderer.new_scene();
    let data = MeshData {
        positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
        normals: vec![Vec3::X; 3],
        uvs: vec![glam::Vec2::ZERO; 3],
        indices: vec![0, 1, 2],
        ranges: vec![(0, 3, 0)],
        ..Default::default()
    };
    let mesh = renderer.add_mesh(&mut scene, &data);
    renderer.begin_object(&mut scene);
    renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![0]);
    renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![0]);
    renderer.prepare(&mut scene);
    assert_eq!([scene.cpu_models[1][1][3], scene.cpu_models[1][2][3]], [0.0, 0.0]);
}

#[test]
fn deforming_a_mesh_invalidates_its_static_depth_plane_metadata() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let adapter = wgpu::Instance::new(descriptor);
    let renderer = pollster::block_on(Renderer::new_with(
        &adapter,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, shadow_size: 1024, ..Default::default() },
    )).expect("no-op renderer");
    let mut scene = renderer.new_scene();
    let data = MeshData {
        positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
        normals: vec![Vec3::Z; 3],
        uvs: vec![glam::Vec2::ZERO; 3],
        indices: vec![0, 1, 2],
        ranges: vec![(0, 3, 0)],
        ..Default::default()
    };
    let mesh = renderer.add_mesh(&mut scene, &data);
    let instance = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![0]);
    renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![0]);
    scene.instances[1].slot_levels[0] = 1;
    renderer.prepare(&mut scene);
    assert_eq!([scene.cpu_models[1][1][3], scene.cpu_models[1][2][3]], [0.0, 0.0]);

    let mut positions = data.positions.clone();
    positions[2].z = 0.2;
    renderer.update_mesh(&mut scene, mesh, &positions, &data.normals, &data.uvs);
    assert!(scene.meshes[mesh].depth_normals.is_empty());
    assert!(scene.changed.contains(&instance));
    renderer.prepare(&mut scene);
    assert_eq!(
        [scene.cpu_models[1][1][3], scene.cpu_models[1][2][3]],
        crate::scene::depth_plane::NO_PLANE,
    );
}
