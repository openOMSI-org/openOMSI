use glam::{DVec3, Vec3};
use omsi_render::{Camera, Corona, Lighting, RenderOptions, Renderer};
use omsi_texture::Image;

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn compact_vehicle_coronas_preserve_off_center_bitmap_details() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            light_glow: true,
            msaa: 1,
            ssao: false,
            fxaa: false,
            render_scale: 1.0,
            ..Default::default()
        },
    ))
    .expect("test renderer");
    let mut rgba = vec![0; 32 * 16 * 4];
    for y in 5..11 {
        for x in [7..11, 21..25].into_iter().flatten() {
            let i = (y * 32 + x) * 4;
            rgba[i..i + 4].copy_from_slice(&[255, 220, 100, 255]);
        }
    }
    renderer.set_corona_texture(
        7,
        &Image {
            width: 32,
            height: 16,
            rgba,
            has_alpha: false,
        },
    );
    let mut scene = renderer.new_scene();
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
    let baseline = renderer
        .render_to_image(&mut scene, 96, 96, &camera, &night)
        .unwrap();
    scene.coronas = vec![Corona {
        position: DVec3::new(0.0, 3.0, 0.0),
        size: 1.5,
        color: [1.0; 3],
        brightness: 2.0,
        vehicle_lamp: true,
        texture: 7,
        ..Default::default()
    }];
    let image = renderer
        .render_to_image(&mut scene, 96, 96, &camera, &night)
        .unwrap();
    let lit_pixels: Vec<_> = image
        .chunks_exact(4)
        .enumerate()
        .filter_map(|(i, px)| {
            let x = i % 96;
            let y = i / 96;
            let delta = px[0].saturating_sub(baseline[i * 4]);
            (delta > 20 && (30..66).contains(&y)).then_some((x, y, delta))
        })
        .collect();
    let count = |x_range: std::ops::Range<usize>| {
        lit_pixels
            .iter()
            .filter(|(x, y, _)| x_range.contains(x) && (32..64).contains(y))
            .count()
    };
    let left = count(30..44);
    let center = count(44..52);
    let right = count(52..66);
    assert!(left > 20, "left LED detail was lost or shifted");
    assert!(right > 20, "right LED detail was lost or shifted");
    assert!(
        center * 2 < left.min(right),
        "LED bitmap collapsed toward the center"
    );
}

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
