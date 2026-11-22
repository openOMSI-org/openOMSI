//! A separate process keeps the forced GL texture/array layout out of other renderer tests.
extern crate omsi_render_mx as omsi_render;

#[test]
fn texture_unit_fallback_builds_and_draws_without_enhanced() {
    unsafe { std::env::set_var("OMSI_GPU_ARRAYS", "nostorage") };
    unsafe { std::env::set_var("OMSI_GL_TEXTURE_UNITS", "1") };
    let instance = mx::Instance::with_backends(mx::Backends::NOOP);
    let mut renderer = pollster::block_on(omsi_render::Renderer::new_with(
        &instance,
        None,
        Some(mx::TextureFormat::Rgba8UnormSrgb),
        omsi_render::RenderOptions {
            msaa: 1,
            shadow_size: 256,
            no_enhanced: false,
            ..Default::default()
        },
    ))
    .expect("renderer on the limited GL layout");
    let mut scene = renderer.new_scene();
    let camera = omsi_render::Camera {
        position: glam::DVec3::new(0.0, -35.0, 30.0),
        yaw: 0.0,
        pitch: -40.0,
        roll: 0.0,
        fov_deg: 60.0,
        near: 0.1,
        far: 1000.0,
    };
    let scope = renderer.watch_errors();
    renderer
        .render_to_image(
            &mut scene,
            16,
            16,
            &camera,
            &omsi_render::Lighting::default(),
        )
        .expect("first frame on the fallback layout");
    assert!(scope.pop().is_none());
}
