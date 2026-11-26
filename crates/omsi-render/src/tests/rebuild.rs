use super::*;

/// A renderer as the tests make it, with `shadow_size`.
fn renderer(shadow_size: u32) -> Renderer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: 1,
            ssao: false,
            shadow_size,
            fxaa: false,
            ..Default::default()
        },
    ))
    .expect("test renderer")
}

/// A scene of nothing but the classic sky: plain gradients and a cloud type of stripes.
fn sky_scene(r: &mut Renderer) -> Scene {
    let mut scene = r.new_scene();
    let blue = omsi_texture::Image::solid([90, 140, 220, 255]);
    let g = [0, 1, 2].map(|_| r.add_texture(&mut scene, &blue, false));
    let (w, h) = (64u32, 64u32);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for _y in 0..h {
        for x in 0..w {
            let a = if (x / 8) % 2 == 0 { 255 } else { 0 };
            rgba.extend([255, 255, 255, a]);
        }
    }
    let stripes = omsi_texture::Image {
        width: w,
        height: h,
        rgba,
        has_alpha: true,
    };
    let clouds = r.add_texture(&mut scene, &stripes, true);
    r.set_sky_textures_vanilla(&mut scene, g, None, Some(clouds), None);
    scene
}

fn lighting(offset: f32) -> Lighting {
    Lighting {
        shadows: false,
        fog_density: 0.0,
        vanilla_sky: VanillaSky {
            cloud_height: 1500.0,
            fog_range: 5000.0,
            visibility: 20000.0,
            cloud_size: 1000.0,
            overcast: false,
            cloud_offset: [offset, 0.0],
        },
        ..Default::default()
    }
}

fn camera() -> Camera {
    Camera {
        position: DVec3::new(0.0, 0.0, 2.0),
        yaw: 0.0,
        pitch: 50.0,
        roll: 0.0,
        fov_deg: 70.0,
        near: 0.1,
        far: 30000.0,
    }
}

fn difference(a: &[u8], b: &[u8]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (*x as f64 - *y as f64).abs())
        .sum::<f64>()
        / a.len() as f64
}

/// A rebuilt renderer (the fallbacks after a GPU error, a setting changed in the menu) draws
/// the sky as a new one does: the sky's bind group is made again with the new renderer's
/// buffers, so the clouds keep moving with the weather instead of standing where they were
/// at the rebuild.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_rebuilt_renderer_draws_the_sky_as_a_new_one() {
    let (w, h) = (160, 96);
    let mut fresh = renderer(512);
    let mut scene = sky_scene(&mut fresh);
    let a = fresh
        .render_to_image(&mut scene, w, h, &camera(), &lighting(0.0))
        .unwrap();
    let b = fresh
        .render_to_image(&mut scene, w, h, &camera(), &lighting(125.0))
        .unwrap();
    assert!(
        difference(&a, &b) > 2.0,
        "the clouds' drift shows in the picture: {}",
        difference(&a, &b)
    );

    let mut r = renderer(1024);
    let mut scene = sky_scene(&mut r);
    let before = r
        .render_to_image(&mut scene, w, h, &camera(), &lighting(0.0))
        .unwrap();
    assert!(difference(&before, &a) < 0.05);
    let options = RenderOptions {
        shadow_size: 512,
        ..r.options
    };
    assert!(r.rebuild_with(&mut scene, options));
    let after = r
        .render_to_image(&mut scene, w, h, &camera(), &lighting(125.0))
        .unwrap();
    let d = difference(&after, &b);
    assert!(d < 0.05, "the rebuilt renderer's sky is a new one's: {d}");
}

/// A rebuild that the card could not give all its memory to (here taken for failed) is built
/// once more after the old renderer is gone, and draws as a new renderer does: no resource of
/// its own is left invalid for the rest of the session.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_rebuild_with_the_card_full_is_built_once_more() {
    let (w, h) = (160, 96);
    let mut fresh = renderer(512);
    let mut scene = sky_scene(&mut fresh);
    let b = fresh
        .render_to_image(&mut scene, w, h, &camera(), &lighting(125.0))
        .unwrap();

    let mut r = renderer(1024);
    let mut scene = sky_scene(&mut r);
    r.render_to_image(&mut scene, w, h, &camera(), &lighting(0.0))
        .unwrap();
    crate::render::REBUILDS.set(0);
    crate::render::REBUILD_FAULTS.set(1);
    let options = RenderOptions {
        shadow_size: 512,
        ..r.options
    };
    assert!(r.rebuild_with(&mut scene, options));
    assert_eq!(crate::render::REBUILDS.get(), 2, "built twice");
    assert_eq!(crate::render::REBUILD_FAULTS.get(), 0);
    assert_eq!(r.options.shadow_size, 512);
    let errors = r.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let after = r
        .render_to_image(&mut scene, w, h, &camera(), &lighting(125.0))
        .unwrap();
    assert!(
        pollster::block_on(errors.pop()).is_none(),
        "no invalid resource drawn with"
    );
    let d = difference(&after, &b);
    assert!(
        d < 0.05,
        "the twice-built renderer's sky is a new one's: {d}"
    );
}
