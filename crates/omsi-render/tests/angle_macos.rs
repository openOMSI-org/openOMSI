#![cfg(target_os = "macos")]

#[test]
#[ignore = "requires the bundled angle/cgl dylibs"]
fn angle_cgl_renders_a_frame() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("wgpu_core=debug,wgpu_hal::gles=debug"),
    )
    .is_test(true)
    .try_init();
    let directory = std::env::var("ANGLE_TEST_LIBRARY_DIRECTORY").expect("ANGLE library directory");
    let directory = std::fs::canonicalize(directory).expect("ANGLE library directory exists");
    for name in ["libEGL.dylib", "libGLESv2.dylib"] {
        assert!(directory.join(name).is_file(), "missing {name}");
    }
    assert!(
        std::env::var_os("ANGLE_DEFAULT_PLATFORM").is_none(),
        "ANGLE_DEFAULT_PLATFORM must be unset so ANGLE selects OpenGL/CGL on macOS"
    );
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::GL;
    let instance = omsi_render::angle::instance(descriptor);
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::GL));
    let adapter = adapters.first().unwrap_or_else(|| {
        panic!(
            "no ANGLE/CGL adapter, check the wgpu/EGL initialization errors above (libraries: {}, DYLD_LIBRARY_PATH: {:?})",
            directory.display(),
            std::env::var_os("DYLD_LIBRARY_PATH")
        )
    });
    let info = adapter.get_info();
    eprintln!("ANGLE/CGL adapter: {info:?}");
    assert_eq!(info.backend, wgpu::Backend::Gl);
    let mut renderer = pollster::block_on(omsi_render::Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        omsi_render::RenderOptions { msaa: 1, shadow_size: 256, ..Default::default() },
    ))
    .expect("game renderer on ANGLE/CGL");
    let mut scene = renderer.new_scene();
    scene.rainfall.rate = 0.1;
    scene.rainfall.drop_mm = 2.0;
    let pixels = renderer
        .render_to_image(
            &mut scene,
            16,
            16,
            &omsi_render::Camera {
                position: glam::DVec3::new(0.0, -35.0, 30.0),
                yaw: 0.0,
                pitch: -40.0,
                roll: 0.0,
                fov_deg: 60.0,
                near: 0.1,
                far: 1000.0,
            },
            &omsi_render::Lighting::default(),
        )
        .expect("first frame on ANGLE/CGL");
    assert_eq!(pixels.len(), 16 * 16 * 4);
    assert!(renderer.device_lost().is_none());
}
