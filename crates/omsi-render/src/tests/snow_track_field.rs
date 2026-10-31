use super::*;

/// The snow track field (road_snow.wgsl) is 16 MB of the card: a token until the roads'
/// snow first sends it rows, made once then, and bound by a scene from its next frame.
#[test]
fn the_snow_track_field_is_made_only_when_the_roads_snow_first_needs_it() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let instance = wgpu::Instance::new(descriptor);
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb), RenderOptions::default(),
    )).expect("noop renderer");
    let mut scene = renderer.new_scene();
    let material = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0; 4], true);
    let mesh = renderer.add_mesh(&mut scene, &MeshData {
        positions: vec![Vec3::new(-2.0, 4.0, -2.0), Vec3::new(2.0, 4.0, -2.0), Vec3::new(2.0, 4.0, 2.0)],
        normals: vec![-Vec3::Y; 3], uvs: vec![glam::Vec2::ZERO; 3],
        indices: vec![0, 1, 2], ranges: vec![(0, 3, 0)],
        ..Default::default()
    });
    renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![material]);
    let camera = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 90.0, near: 0.1, far: 100.0 };
    let lighting = Lighting::default();
    renderer.render_to_image(&mut scene, 16, 16, &camera, &lighting).unwrap();
    // (a session without snow keeps the token)
    assert_eq!(renderer.lights.snow_track.borrow().size(), SNOW_TRACK_TOKEN);
    assert_eq!(renderer.lights.snow_track_made.get(), 0);
    if !road_snow_supported() {
        return;
    }
    renderer.set_snow_track_rows(3, 0, &vec![7u8; SNOW_TRACK_TILE as usize * 4 * 2]);
    assert_eq!(renderer.lights.snow_track.borrow().size(), (SNOW_TRACK_TEXELS as u64).pow(2) * 4);
    assert_eq!(renderer.lights.snow_track_made.get(), 1);
    renderer.render_to_image(&mut scene, 16, 16, &camera, &lighting).unwrap();
    assert_eq!(scene.snow_track_seen, 1, "the scene binds the field made");
    renderer.set_snow_track_rows(4, 0, &vec![7u8; SNOW_TRACK_TILE as usize * 4]);
    assert_eq!(renderer.lights.snow_track_made.get(), 1, "made once");
}
