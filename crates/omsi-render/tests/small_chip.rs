//! A graphics chip that takes textures of at most 2048 texels a side (`OMSI_GPU_LIMITS=
//! downlevel`, as older OpenGL chips): pictures of the game's own larger than that - a
//! script's or a sign's text, an HTML display - are made to fit, and writing them every
//! frame is no device error. (A test binary of its own: the limits come from the process's
//! environment.)

#[test]
fn pictures_larger_than_the_chip_takes_are_made_to_fit() {
    unsafe { std::env::set_var("OMSI_GPU_LIMITS", "downlevel") };
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let instance = wgpu::Instance::new(descriptor);
    let renderer = pollster::block_on(omsi_render::Renderer::new_with(&instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb), Default::default()))
        .expect("renderer on the no-op device");
    assert_eq!(renderer.device.limits().max_texture_dimension_2d, 2048);
    let mut scene = renderer.new_scene();
    let scope = renderer.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let big = omsi_texture::Image { width: 4096, height: 64, rgba: vec![200; 4096 * 64 * 4], has_alpha: true };
    let a = renderer.add_texture(&mut scene, &big, false);
    let b = renderer.add_texture(&mut scene, &big, true);
    let c = renderer.add_blank_texture(&mut scene, 4096, 64, false);
    let d = renderer.add_render_texture(&mut scene, 3000, 3000);
    for id in [a, b, c] {
        renderer.update_texture(&scene, id, &big);
    }
    renderer.update_texture_mips(&mut scene, b, &big);
    for id in [a, b, c, d] {
        let (w, h) = scene.texture_size_of(id).expect("texture");
        assert!(w <= 2048 && h <= 2048, "{w}x{h}");
    }
    assert_eq!(scene.texture_size_of(a), Some((2048, 32)));
    let error = pollster::block_on(scope.pop());
    assert!(error.is_none(), "{error:?}");
}
