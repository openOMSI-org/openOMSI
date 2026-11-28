//! Enhanced+'s ray tracing on whatever adapter this machine has (on Vulkan and
//! Direct3D 12 as well as on Metal): a small scene drawn for several frames must raise no
//! device error. CI runs it on Mesa's lavapipe (Vulkan, VK_KHR_ray_query) and on WARP
//! (Direct3D 12), where no graphics card is needed. (A test binary of its own: the switch
//! comes from the process's environment.)
extern crate omsi_render_mx as omsi_render;

use glam::{DVec3, Mat4, Vec2, Vec3};
use omsi_geometry::MeshData;
use omsi_render::{AlphaMode, Camera, Lighting, RenderOptions, Renderer};

fn box_mesh(c: Vec3, h: Vec3) -> MeshData {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let mut n = Vec3::ZERO;
            n[axis] = sign;
            let (u, v) = match axis {
                0 => (Vec3::Y, Vec3::Z),
                1 => (Vec3::Z, Vec3::X),
                _ => (Vec3::X, Vec3::Y),
            };
            let base = positions.len() as u32;
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                positions.push(c + n * h + u * h * a + v * h * b);
                normals.push(n);
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    let count = indices.len() as u32;
    MeshData {
        wire_wind: Vec::new(),
        uvs: vec![Vec2::ZERO; positions.len()],
        positions,
        normals,
        ranges: vec![(0, count, 0)],
        indices,
        one_sided: false,
    }
}

#[test]
#[ignore = "needs a GPU adapter (rt_check.yml)"]
fn ray_traced_frames_raise_no_device_error() {
    let _ = env_logger::builder()
        .is_test(false)
        .filter_level(log::LevelFilter::Warn)
        .try_init();
    println!("creating the renderer");
    let instance = mx::Instance::new();
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(mx::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: 1,
            ssao: true,
            shadow_size: 1024,
            fxaa: true,
            render_scale: 1.0,
            ray_tracing: true,
            no_enhanced: false,
            ..Default::default()
        },
    ))
    .expect("renderer");
    let ray_query = renderer.adapter().is_some_and(|a| a.supports_ray_query());
    println!(
        "adapter: {} (ray queries {ray_query})",
        renderer.adapter_name
    );
    let traced = ray_query && omsi_cfg::flags::OMSI_NO_RT.live_os().is_none();
    println!("ray queries: {traced}");
    let mut scene = renderer.new_scene();
    let ground = renderer.add_mesh(
        &mut scene,
        &box_mesh(Vec3::new(0.0, 0.0, -0.5), Vec3::new(60.0, 60.0, 0.5)),
    );
    let wall = renderer.add_mesh(
        &mut scene,
        &box_mesh(Vec3::new(4.0, 12.0, 2.0), Vec3::new(3.0, 0.5, 2.0)),
    );
    let pole = renderer.add_mesh(
        &mut scene,
        &box_mesh(Vec3::new(-3.0, 8.0, 3.0), Vec3::new(0.2, 0.2, 3.0)),
    );
    let grey = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.5, 0.5, 0.5, 1.0],
        false,
    );
    let red = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.8, 0.2, 0.1, 1.0],
        false,
    );
    renderer.add_instance(&mut scene, ground, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    renderer.add_instance(&mut scene, wall, DVec3::ZERO, Mat4::IDENTITY, vec![red]);
    renderer.add_instance(&mut scene, pole, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    let camera = Camera {
        position: DVec3::new(0.0, 0.0, 2.0),
        yaw: 0.0,
        pitch: -5.0,
        roll: 0.0,
        fov_deg: 70.0,
        near: 0.1,
        far: 1000.0,
    };
    let lighting = Lighting {
        enhanced: true,
        shadows: true,
        wetness: 0.6,
        sun_dir: Vec3::new(0.4, -0.6, 0.7).normalize(),
        ..Default::default()
    };
    let scopes = [renderer.watch_errors()];
    let mut last = Vec::new();
    for frame in 0..6 {
        let t = std::time::Instant::now();
        last = renderer
            .render_to_image(&mut scene, 320, 180, &camera, &lighting)
            .expect("frame");
        println!(
            "frame {frame}: {:.0} ms",
            t.elapsed().as_secs_f64() * 1000.0
        );
    }
    let mut errors = Vec::new();
    for s in scopes.into_iter().rev() {
        if let Some(e) = s.pop() {
            errors.push(e.to_string());
        }
    }
    // the picture is drawn at all (the wall and the ground, not one flat colour)
    let lum: Vec<u32> = last
        .chunks(4)
        .map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32)
        .collect();
    let (lo, hi) = (
        lum.iter().min().copied().unwrap_or(0),
        lum.iter().max().copied().unwrap_or(0),
    );
    println!("picture luminance {lo}..{hi}");
    save_picture("rt_smoke", 320, 180, &last);
    assert!(
        errors.is_empty(),
        "device errors:\n{}",
        errors.join("\n---\n")
    );
    assert!(hi > lo + 30, "a flat picture: {lo}..{hi}");
}

/// A scene shaped like a map's: thousands of instances (more than the first top-level
/// structure holds), hundreds of textures, cut-out and blended slots, a mesh of many slots,
/// multisampling, a moving camera, a mesh whose vertices change.
#[test]
#[ignore = "needs a GPU adapter (rt_check.yml)"]
fn a_map_sized_scene_traces_without_device_errors() {
    let _ = env_logger::builder()
        .is_test(false)
        .filter_level(log::LevelFilter::Warn)
        .try_init();
    let instance = mx::Instance::new();
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(mx::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: 4,
            ssao: true,
            shadow_size: 2048,
            fxaa: true,
            render_scale: 1.0,
            ray_tracing: true,
            no_enhanced: false,
            ..Default::default()
        },
    ))
    .expect("renderer");
    println!("adapter: {}", renderer.adapter_name);
    let mut scene = renderer.new_scene();
    let mut textures = Vec::new();
    for t in 0..300u32 {
        let mut rgba = vec![0u8; 32 * 32 * 4];
        for (i, p) in rgba.chunks_mut(4).enumerate() {
            let v = ((i as u32 * 7 + t * 13) % 255) as u8;
            p.copy_from_slice(&[
                v,
                255 - v,
                (t % 255) as u8,
                if (i / 4) % 3 == 0 { 0 } else { 255 },
            ]);
        }
        textures.push(renderer.add_texture(
            &mut scene,
            &omsi_texture::Image {
                width: 32,
                height: 32,
                rgba,
                has_alpha: true,
            },
            true,
        ));
    }
    let mut materials = Vec::new();
    for (k, t) in textures.iter().enumerate() {
        let alpha = match k % 3 {
            0 => AlphaMode::Opaque,
            1 => AlphaMode::Test,
            _ => AlphaMode::Blend,
        };
        materials.push(renderer.add_material(&mut scene, Some(*t), alpha, [1.0; 4], false));
    }
    // meshes of one to three slots, and one of 70
    let mut meshes = Vec::new();
    for k in 0..400usize {
        let mut m = box_mesh(
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.5 + (k % 7) as f32 * 0.3, 0.5, 1.0 + (k % 5) as f32 * 0.4),
        );
        let slots = 1 + k % 3;
        let per = (m.indices.len() / 3 / slots) as u32 * 3;
        m.ranges = (0..slots)
            .map(|s| {
                (
                    s as u32 * per,
                    if s + 1 == slots {
                        m.indices.len() as u32 - s as u32 * per
                    } else {
                        per
                    },
                    s as u32,
                )
            })
            .collect();
        meshes.push((renderer.add_mesh(&mut scene, &m), slots));
    }
    let mut many = box_mesh(Vec3::ZERO, Vec3::splat(1.0));
    let base: Vec<u32> = many.indices.clone();
    let n0 = many.positions.len() as u32;
    let (p0, nn) = (many.positions.clone(), many.normals.clone());
    for s in 1..70u32 {
        many.positions
            .extend(p0.iter().map(|p| *p + Vec3::new(s as f32 * 0.1, 0.0, 0.0)));
        many.normals.extend(nn.iter());
        many.indices.extend(base.iter().map(|i| i + s * n0));
    }
    many.uvs = vec![Vec2::ZERO; many.positions.len()];
    many.ranges = (0..70u32).map(|s| (s * 36, 36, s)).collect();
    let many_id = renderer.add_mesh(&mut scene, &many);
    renderer.add_instance(
        &mut scene,
        many_id,
        DVec3::new(5.0, 20.0, 0.0),
        Mat4::IDENTITY,
        (0..70).map(|s| materials[s % materials.len()]).collect(),
    );
    let ground = renderer.add_mesh(
        &mut scene,
        &box_mesh(Vec3::new(0.0, 0.0, -0.5), Vec3::new(400.0, 400.0, 0.5)),
    );
    renderer.add_instance(
        &mut scene,
        ground,
        DVec3::ZERO,
        Mat4::IDENTITY,
        vec![materials[0]],
    );
    let mut count = 0;
    for x in -50i32..50 {
        for y in -50..50 {
            let k = ((x * 31 + y * 17).rem_euclid(400)) as usize;
            let (mesh, slots) = meshes[k];
            let mats = (0..slots)
                .map(|s| materials[(k * 3 + s) % materials.len()])
                .collect();
            renderer.add_instance(
                &mut scene,
                mesh,
                DVec3::new(x as f64 * 6.0, y as f64 * 6.0, 0.0),
                Mat4::from_rotation_z((k as f32) * 0.3),
                mats,
            );
            count += 1;
        }
    }
    println!("{count} instances");
    let lighting = Lighting {
        enhanced: true,
        shadows: true,
        wetness: 0.5,
        sun_dir: Vec3::new(0.4, -0.6, 0.7).normalize(),
        ..Default::default()
    };
    let scopes = [renderer.watch_errors()];
    for frame in 0..8 {
        let camera = Camera {
            position: DVec3::new(frame as f64 * 20.0, -30.0 + frame as f64 * 5.0, 3.0),
            yaw: frame as f32 * 20.0,
            pitch: -5.0,
            roll: 0.0,
            fov_deg: 70.0,
            near: 0.1,
            far: 1500.0,
        };
        if frame == 4 {
            let moved: Vec<Vec3> = many.positions.iter().map(|p| *p + Vec3::Z * 0.5).collect();
            renderer.update_mesh(&mut scene, many_id, &moved, &many.normals, &many.uvs);
        }
        let t = std::time::Instant::now();
        renderer
            .render_to_image(&mut scene, 480, 270, &camera, &lighting)
            .expect("frame");
        println!(
            "frame {frame}: {:.0} ms",
            t.elapsed().as_secs_f64() * 1000.0
        );
    }
    let mut errors = Vec::new();
    for s in scopes.into_iter().rev() {
        if let Some(e) = s.pop() {
            errors.push(e.to_string());
        }
    }
    assert!(
        errors.is_empty(),
        "device errors:\n{}",
        errors.join("\n---\n")
    );
}

/// RENDER_SMOKE_PNG=<folder>: the last frame written there as PNG (CI keeps it, to compare
/// the backends and the two renderers by eye)
fn save_picture(name: &str, w: u32, h: u32, rgba: &[u8]) {
    let Some(dir) = std::env::var_os("RENDER_SMOKE_PNG") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{name}.png"));
    match image::RgbaImage::from_raw(w, h, rgba.to_vec()).map(|img| img.save(&path)) {
        Some(Ok(())) => println!("picture written to {}", path.display()),
        other => println!("no picture written to {}: {other:?}", path.display()),
    }
}
