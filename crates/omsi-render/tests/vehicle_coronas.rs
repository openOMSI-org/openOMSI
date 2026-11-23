use glam::{DVec3, Vec3};
use omsi_render::{Camera, Corona, Lighting, RenderOptions, Renderer};

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn compact_vehicle_coronas_only_change_the_picture_with_glow_enabled() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let camera = Camera {
        position: DVec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 100.0,
    };
    let night = Lighting {
        enhanced: true,
        shadows: false,
        sun_dir: -Vec3::Z,
        sun_intensity: 0.0,
        fog_density: 0.0,
        night: 1.0,
        ..Default::default()
    };

    for glow in [false, true] {
        let mut renderer = pollster::block_on(Renderer::new_with(
            &instance,
            None,
            Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            RenderOptions {
                light_glow: glow,
                msaa: 1,
                ssao: false,
                fxaa: false,
                render_scale: 1.0,
                ..Default::default()
            },
        ))
        .expect("test renderer");
        let mut picture = |vehicle_lamp| {
            let mut scene = renderer.new_scene();
            scene.coronas = vec![Corona {
                position: DVec3::new(0.0, 10.0, 0.0),
                size: 1.0,
                color: [1.0; 3],
                brightness: 2.0,
                vehicle_lamp,
                ..Default::default()
            }];
            renderer
                .render_to_image(&mut scene, 64, 64, &camera, &night)
                .unwrap()
        };
        let original = picture(false);
        let vehicle = picture(true);
        assert_eq!(original == vehicle, !glow, "Glow of lights: {glow}");
    }
}
