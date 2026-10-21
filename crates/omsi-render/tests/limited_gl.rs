//! A separate process keeps the forced GL texture/array layout out of other renderer tests.

#[test]
fn texture_unit_fallback_builds_and_draws_without_enhanced() {
    std::env::set_var("OMSI_GPU_ARRAYS", "nostorage");
    std::env::set_var("OMSI_GL_TEXTURE_UNITS", "1");
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let instance = wgpu::Instance::new(descriptor);
    let mut renderer = pollster::block_on(omsi_render::Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        omsi_render::RenderOptions {
            msaa: 1,
            shadow_size: 256,
            no_enhanced: false,
            ..Default::default()
        },
    ))
    .expect("renderer on the limited GL layout");
    let mut scene = renderer.new_scene();
    scene.rainfall.rate = 0.5;
    scene.rainfall.drop_mm = 2.0;
    let camera = omsi_render::Camera {
        position: glam::DVec3::new(0.0, -35.0, 30.0),
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
        .render_to_image(
            &mut scene,
            16,
            16,
            &camera,
            &omsi_render::Lighting::default(),
        )
        .expect("first frame on the fallback layout");
    assert!(pollster::block_on(scope.pop()).is_none());
}
