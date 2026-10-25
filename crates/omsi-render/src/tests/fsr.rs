use super::*;

/// A road with thin red rails on it seen from the cab: fine lines and hard edges, what an
/// upscaler shows its difference on.
fn rails_scene(renderer: &mut Renderer) -> Scene {
    let mut scene = renderer.new_scene();
    let strip = |half: f32, z: f32, stations: usize| {
        let mut m = MeshData { ranges: vec![(0, 6 * stations as u32, 0)], ..Default::default() };
        for i in 0..=stations {
            let y = 2.0 + 30.0 * i as f32 / stations as f32;
            m.positions.extend([Vec3::new(-half, y, z), Vec3::new(half, y, z)]);
            m.normals.extend([Vec3::Z; 2]);
            m.uvs.extend([glam::Vec2::ZERO; 2]);
        }
        for i in 0..stations as u32 {
            let a = 2 * i;
            m.indices.extend([a, a + 1, a + 2, a + 1, a + 3, a + 2]);
        }
        m
    };
    let road = renderer.add_mesh(&mut scene, &strip(3.0, 0.0, 1));
    let rails = renderer.add_mesh(&mut scene, &strip(0.15, 0.001, 10));
    let grey = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.3, 0.3, 0.3, 1.0], true);
    let red = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 0.0, 0.0, 1.0], true);
    for (mesh, mat) in [(road, grey), (rails, red)] {
        let i = renderer.add_surface_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![mat]);
        scene.instances[i].render_phase = RenderPhase::Spline;
    }
    scene
}

fn renderer(render_scale: f32, fsr1: bool) -> Renderer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale, fsr1, ..Default::default() },
    ))
    .expect("test renderer")
}

fn camera() -> Camera {
    Camera { position: DVec3::new(0.0, 0.0, 2.5), yaw: 0.0, pitch: -15.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 200.0 }
}

/// How far apart two pictures are, on average per channel.
fn difference(a: &[u8], b: &[u8]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (*x as f64 - *y as f64).abs()).sum::<f64>() / a.len() as f64
}

/// FSR 1 turned on while the game runs (the menu's way): it is built then, upscales the
/// picture - close to the standard upscaler's, not the same - and turned off again gives
/// the standard picture back.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn fsr1_turned_on_while_running_upscales_the_picture() {
    let (w, h) = (192, 128);
    let mut r = renderer(0.5, false);
    let mut scene = rails_scene(&mut r);
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    assert!(!r.fsr_active(w, h));
    let standard = r.render_to_image(&mut scene, w, h, &camera(), &lighting).unwrap();
    assert!(r.set_upscaler(true, 0.3, 0.5), "FSR 1 built");
    assert!(r.fsr_active(w, h));
    let fsr = r.render_to_image(&mut scene, w, h, &camera(), &lighting).unwrap();
    let d = difference(&standard, &fsr);
    assert!(d > 0.05 && d < 12.0, "FSR 1's picture differs from the standard one a little: {d}");
    // the rails are still red in it
    let reds = fsr.chunks(4).filter(|p| p[0] > 120 && p[1] < 60 && p[2] < 60).count();
    assert!(reds > 50, "{reds} red pixels");
    assert!(r.set_upscaler(false, 0.3, 0.5));
    assert!(!r.fsr_active(w, h));
    assert!(r.scale.fsr_target.borrow().is_empty(), "turning FSR off releases its intermediate");
    let again = r.render_to_image(&mut scene, w, h, &camera(), &lighting).unwrap();
    assert!(difference(&standard, &again) < 0.01, "the standard picture back");
}

/// FSR 1 asked for at the start, and the automatic render scale (0): at full size nothing
/// is upscaled, so FSR stays out of the way; the governor's smaller step goes through it.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn fsr1_follows_the_automatic_render_scale() {
    let (w, h) = (192, 128);
    let mut r = renderer(0.0, true);
    assert!(!r.fsr_active(w, h), "full size: nothing to upscale");
    r.set_dynamic_scale(0.7);
    assert!(r.fsr_active(w, h), "the governor's step is upscaled by FSR 1");
    let mut scene = rails_scene(&mut r);
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    let picture = r.render_to_image(&mut scene, w, h, &camera(), &lighting).unwrap();
    assert!(picture.chunks(4).filter(|p| p[0] > 120 && p[1] < 60).count() > 50);
}

/// The three panels of a triple screen each go through FSR 1, as the window does.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn fsr1_upscales_each_triple_screen_panel() {
    let (w, h) = (288, 96);
    let rig = TripleScreen { enabled: true, ..Default::default() };
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    let mut standard = renderer(0.5, false);
    let mut scene = rails_scene(&mut standard);
    let a = standard.render_triple_to_image(&mut scene, w, h, &camera(), &lighting, &rig).unwrap();
    let mut fsr = renderer(0.5, true);
    let mut scene = rails_scene(&mut fsr);
    let b = fsr.render_triple_to_image(&mut scene, w, h, &camera(), &lighting, &rig).unwrap();
    // (the centre panel looks down the road: its rails are there in both)
    let reds = |img: &[u8]| (0..h as usize).flat_map(|y| (w as usize / 3..2 * w as usize / 3).map(move |x| (y, x))).filter(|&(y, x)| { let p = &img[(y * w as usize + x) * 4..]; p[0] > 120 && p[1] < 60 }).count();
    assert!(reds(&a) > 20 && reds(&b) > 20, "{} {}", reds(&a), reds(&b));
    let d = difference(&a, &b);
    assert!(d > 0.05 && d < 12.0, "the panels went through FSR 1: {d}");
}

/// VR with FSR 1: the headset image is full-sized, each eye is drawn at the eye resolution
/// and FSR 1 scales it up to the image; without FSR 1 the image itself is that small, as
/// before.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_vr_eye_with_fsr1_is_drawn_smaller_and_scaled_up() {
    let (w, h) = (192, 128);
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    let cam = camera();
    let projection = Mat4::perspective_rh(cam.fov_deg.to_radians(), w as f32 / h as f32, cam.far, cam.near);
    let eye = |r: &mut Renderer| {
        let mut scene = rails_scene(r);
        let texture = r.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("test eye"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: r.format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        r.render_xr_eye(&mut scene, &texture.create_view(&Default::default()), w, h, &cam, &lighting, projection, false);
        r.read_texture(&texture, wgpu::TextureAspect::All).unwrap()
    };
    let mut plain = renderer(1.0, false);
    assert_eq!(plain.start_xr(0.65), 0.65, "without FSR 1 the headset image is the eye resolution");
    let full = eye(&mut plain);
    let mut fsr = renderer(1.0, true);
    assert_eq!(fsr.start_xr(0.65), 1.0, "with FSR 1 the headset image is full-sized");
    assert_eq!(fsr.xr_fsr_scale(), Some(0.65));
    let scaled = eye(&mut fsr);
    assert!(scaled.chunks(4).filter(|p| p[0] > 120 && p[1] < 60).count() > 50, "the rails in the eye's picture");
    let d = difference(&full, &scaled);
    assert!(d > 0.05 && d < 12.0, "drawn at 65 % and scaled up by FSR 1: {d}");
    // FSR 1 turned off later: the eye stays small, scaled up by the standard upscaler
    assert!(fsr.set_upscaler(false, 0.3, 1.0));
    assert_eq!(fsr.xr_fsr_scale(), Some(0.65));
    let standard = eye(&mut fsr);
    assert!(difference(&scaled, &standard) > 0.05);
}

/// At sharpness 0 RCAS would give its input back: EASU draws the picture itself, without
/// the intermediate, and the picture is the one RCAS barely sharpening gives.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn fsr1_without_sharpening_skips_rcas() {
    let (w, h) = (192, 128);
    let mut r = renderer(0.5, true);
    let mut scene = rails_scene(&mut r);
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    assert!(r.set_upscaler(true, 0.0, 0.5));
    let unsharpened = r.render_to_image(&mut scene, w, h, &camera(), &lighting).unwrap();
    assert!(r.scale.fsr_target.borrow().is_empty(), "no intermediate without RCAS");
    assert!(unsharpened.chunks(4).filter(|p| p[0] > 120 && p[1] < 60).count() > 50, "the rails upscaled");
    assert!(r.set_upscaler(true, 1e-6, 0.5));
    let barely = r.render_to_image(&mut scene, w, h, &camera(), &lighting).unwrap();
    assert_eq!(r.scale.fsr_target.borrow().len(), 1, "RCAS ran through the intermediate");
    let d = difference(&unsharpened, &barely);
    assert!(d < 0.5, "the same picture, one 8-bit step aside: {d}");
    assert!(r.set_upscaler(true, 1.0, 0.5));
    let sharp = r.render_to_image(&mut scene, w, h, &camera(), &lighting).unwrap();
    assert!(difference(&unsharpened, &sharp) > d, "sharpening still changes it");
    assert!(r.set_upscaler(true, 0.0, 0.5));
    assert!(r.scale.fsr_target.borrow().is_empty(), "zero sharpness releases the old intermediate");
    let zero_again = r.render_to_image(&mut scene, w, h, &camera(), &lighting).unwrap();
    assert!(difference(&unsharpened, &zero_again) < 0.01, "EASU still draws after releasing RCAS");
    assert!(r.set_upscaler(true, 1.0, 0.5));
    let sharp_again = r.render_to_image(&mut scene, w, h, &camera(), &lighting).unwrap();
    assert_eq!(r.scale.fsr_target.borrow().len(), 1, "RCAS recreates its intermediate");
    assert!(difference(&sharp, &sharp_again) < 0.01, "sharpening survives recreation");
}

/// The FSR sharpening slider's whole range reaches RCAS: 100 % sharpens more than 80 %
/// (the standard upscaler's limit, which RCAS used to be held at too).
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn fsr1_sharpening_uses_the_whole_slider() {
    let (w, h) = (192, 128);
    let mut r = renderer(0.5, true);
    let mut scene = rails_scene(&mut r);
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    let shot = |r: &mut Renderer, scene: &mut Scene, sharpness: f32| {
        assert!(r.set_upscaler(true, sharpness, 0.5));
        r.render_to_image(scene, w, h, &camera(), &lighting).unwrap()
    };
    let (at80, at100, again80) = (shot(&mut r, &mut scene, 0.8), shot(&mut r, &mut scene, 1.0), shot(&mut r, &mut scene, 0.8));
    assert!(difference(&at80, &again80) < 0.01, "the same picture for the same setting");
    assert!(difference(&at80, &at100) > 0.01, "100 % is not held at 80 %: {}", difference(&at80, &at100));
}


/// The sharpening slider's whole range onto 0..0.8 of RCAS's lobe: 100 % is the strongest that
/// looked clean (AMD's 1.0 grained the gauges), the ends hold and nothing goes past them.
#[test]
fn fsr1_sharpening_slider_maps_onto_the_clean_range() {
    assert_eq!(crate::fsr1_rcas_strength(0.0), 0.0);
    assert!((crate::fsr1_rcas_strength(1.0) - 0.8).abs() < 1e-6);
    assert!((crate::fsr1_rcas_strength(0.5) - 0.4).abs() < 1e-6);
    assert_eq!(crate::fsr1_rcas_strength(2.0), crate::fsr1_rcas_strength(1.0));
    assert_eq!(crate::fsr1_rcas_strength(-1.0), 0.0);
}

/// FSR 1's three passes make GLSL (OpenGL 3.3, OpenGL ES 3.0) on both window formats: it is
/// kept to `textureLoad` and plain arithmetic for the old GL drivers it is meant for.
#[test]
fn fsr1_translates_to_glsl() {
    use wgpu::naga;
    use wgpu::naga::back::glsl;
    // (and the raw-input variant, OMSI_FSR_RAW_INPUT: not used on OpenGL, but it must make
    // the other backends' code, which the same translation checks)
    for (format, raw) in [(wgpu::TextureFormat::Bgra8UnormSrgb, false), (wgpu::TextureFormat::Rgba8Unorm, false), (wgpu::TextureFormat::Bgra8UnormSrgb, true)] {
        let src = crate::shaders::fsr1_source_for(format, raw);
        assert_eq!(src.contains("const RAW_INPUT: bool = true;"), raw);
        let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
        let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
            .validate(&module)
            .expect("validate");
        let names: Vec<&str> = module.entry_points.iter().map(|e| e.name.as_str()).collect();
        for entry in ["vs_main", "fs_easu", "fs_easu_mid", "fs_rcas"] {
            assert!(names.contains(&entry), "{entry} in {names:?}");
        }
        for version in [glsl::Version::Desktop(330), glsl::Version::Embedded { version: 300, is_webgl: false }] {
            let options = glsl::Options { version, ..Default::default() };
            for entry in &module.entry_points {
                let (module, info) = naga::back::pipeline_constants::process_overrides(&module, &info, Some((entry.stage, &entry.name)), &Default::default())
                    .expect("overrides");
                let pipeline = glsl::PipelineOptions { shader_stage: entry.stage, entry_point: entry.name.clone(), multiview: None };
                let mut out = String::new();
                glsl::Writer::new(&mut out, &module, &info, &options, &pipeline, Default::default())
                    .and_then(|mut w| w.write())
                    .unwrap_or_else(|e| panic!("{format:?} {version:?} {}: {e:?}", entry.name));
            }
        }
    }
}

/// FSR 1 draws the scene smaller, and the textures with a mip bias of log2 of the scale (AMD's
/// advice): a checkered wall facing the camera then keeps its checks - not the grey of the
/// smaller picture's mip - and shows no more of them than the picture drawn at the window's
/// own size (the bias stops at the window's size). (Not as much as it: the half-size picture
/// scaled up cannot hold checks of a pixel or so.)
/// The wall is square on to the camera and its texels as wide as they are high, a little
/// over one to the pixel at the window's size: with texels much longer one way than the
/// other, anisotropic filtering (Metal's) chose its mip from the short side, the top one with
/// the bias or without it, and the test saw no difference.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn fsr1_textures_keep_their_detail_with_the_mip_bias() {
    let (w, h) = (384, 256);
    let side = 64u32;
    let rgba: Vec<u8> = (0..side * side).flat_map(|i| if (i % side + i / side) % 2 == 0 { [20u8, 20, 20, 255] } else { [235, 235, 235, 255] }).collect();
    let data = omsi_texture::TextureData { width: side, height: side, format: omsi_texture::PixelFormat::Rgba8, levels: vec![rgba], has_alpha: false, gpu_mips: true };
    let wall = MeshData {
        positions: vec![Vec3::new(-10.0, 20.0, -2.0), Vec3::new(10.0, 20.0, -2.0), Vec3::new(10.0, 20.0, 12.0), Vec3::new(-10.0, 20.0, 12.0)],
        normals: vec![-Vec3::Y; 4],
        // (20 x 14 m, 11 pixels to the metre at 20 m: 1.3 texels to the pixel at the window's
        // size, 2.6 in the half-size picture - its mips' grey without the bias)
        uvs: vec![glam::Vec2::new(0.0, 0.0), glam::Vec2::new(4.5, 0.0), glam::Vec2::new(4.5, 3.15), glam::Vec2::new(0.0, 3.15)],
        ranges: vec![(0, 6, 0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        one_sided: false,
    };
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    let shot = |scale: f32, fsr: bool, bias: bool| {
        let mut r = renderer(scale, fsr);
        let mut scene = r.new_scene();
        let tex = r.add_texture_data(&mut scene, &data);
        let mesh = r.add_mesh(&mut scene, &wall);
        let mat = r.add_material(&mut scene, Some(tex), AlphaMode::Opaque, [1.0; 4], true);
        r.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![mat]);
        if fsr {
            assert!(r.set_upscaler(true, 0.0, scale));
        }
        r.scale.fsr_mip_bias_off.set(!bias);
        let square_on = Camera { pitch: 0.0, ..camera() };
        r.render_to_image(&mut scene, w, h, &square_on, &lighting).unwrap()
    };
    // how much the picture changes from one pixel to the next across: the checks' contrast
    let detail = |img: &[u8]| -> f64 {
        (0..h as usize).flat_map(|y| (1..w as usize).map(move |x| (x, y)))
            .map(|(x, y)| (img[(y * w as usize + x) * 4 + 1] as f64 - img[(y * w as usize + x - 1) * 4 + 1] as f64).abs())
            .sum()
    };
    let native = detail(&shot(1.0, false, false));
    let with = detail(&shot(0.5, true, true));
    let without = detail(&shot(0.5, true, false));
    eprintln!("detail: native {native}, FSR 50 % with the bias {with}, without {without}");
    assert!(with > without * 4.0, "the checks kept with the bias, grey without: native {native}, with {with}, without {without}");
    assert!(with < native * 1.3, "not sharper than the full-size picture: native {native}, with {with}");
}
