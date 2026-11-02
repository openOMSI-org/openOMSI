use super::*;

struct AllocationLimit;

impl AllocationLimit {
    fn set(bytes: u64) -> Self {
        TEXTURE_TEST_MAX_BYTES.set(Some(bytes));
        Self
    }
}

impl Drop for AllocationLimit {
    fn drop(&mut self) {
        TEXTURE_TEST_MAX_BYTES.set(None);
    }
}

fn renderer(noop: bool) -> Renderer {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    if noop {
        descriptor.backends = wgpu::Backends::NOOP;
        descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    }
    let instance = wgpu::Instance::new(descriptor);
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: 1,
            ssao: false,
            shadow_size: 1024,
            fxaa: false,
            render_scale: 1.0,
            ..Default::default()
        },
    ))
    .expect("test renderer")
}

fn picture(width: u32, height: u32) -> omsi_texture::TextureData {
    let rgba = (0..height)
        .flat_map(|_| {
            (0..width).flat_map(|x| {
                if x < width / 2 {
                    [230, 20, 20, 255]
                } else {
                    [0, 0, 0, 0]
                }
            })
        })
        .collect();
    omsi_texture::TextureData::from_image(omsi_texture::Image {
        width,
        height,
        rgba,
        has_alpha: true,
    })
}

#[test]
fn memory_pressure_retries_rgba_and_preserves_a_working_texture() {
    let renderer = renderer(true);
    let mut scene = renderer.new_scene();
    let data = picture(720, 405);
    let limit = AllocationLimit::set(100_000);
    let id = renderer.add_texture_data(&mut scene, &data);
    assert_eq!(scene.texture_size_of(id), Some((180, 101)));
    assert!(scene.texture_bytes_of(id) > 0 && scene.texture_bytes_of(id) <= 100_000);
    let generation = scene.textures[id].generation;
    TEXTURE_TEST_MAX_BYTES.set(Some(1_000));
    assert!(!renderer.replace_texture(&mut scene, id, &data));
    assert_eq!(
        scene.textures[id].generation, generation,
        "a failed upgrade must not degrade a working picture"
    );
    TEXTURE_TEST_MAX_BYTES.set(Some(0));
    assert!(!renderer.replace_texture(&mut scene, id, &data));
    assert_eq!(scene.textures[id].generation, generation);
    assert_eq!(scene.texture_size_of(id), Some((180, 101)));
    drop(limit);
    assert!(renderer.replace_texture(&mut scene, id, &data));
    assert_eq!(scene.texture_size_of(id), Some((720, 405)));
}

#[test]
fn memory_pressure_uses_authored_bc_levels_and_handles_the_smallest_blocks() {
    let renderer = renderer(true);
    assert!(renderer.supports_bc());
    let data = omsi_texture::TextureData {
        width: 64,
        height: 32,
        format: omsi_texture::PixelFormat::Bc1,
        levels: (0..7)
            .map(|level| {
                [0x00, 0xf8, 0, 0, 0, 0, 0, 0].repeat(
                    omsi_texture::PixelFormat::Bc1.level_bytes(64 >> level, 32 >> level) / 8,
                )
            })
            .collect(),
        has_alpha: true,
        gpu_mips: false,
    };
    let small = smaller_upload(&data).unwrap();
    assert_eq!(small.format, data.format);
    assert_eq!(small.levels, data.levels[1..]);
    let limit = AllocationLimit::set(120);
    let prepared = prepare_texture(&renderer.device, &renderer.queue, &data).unwrap();
    assert_eq!(prepared.0.size, (16, 8));
    assert!(prepared.bytes() > 0 && prepared.bytes() <= 120);
    assert_eq!(
        prepared.0.texture.format(),
        wgpu::TextureFormat::Bc1RgbaUnormSrgb
    );
    TEXTURE_TEST_MAX_BYTES.set(Some(4));
    let last = prepare_texture(&renderer.device, &renderer.queue, &data).unwrap();
    assert_eq!(last.0.size, (1, 1));
    assert_eq!(last.bytes(), 4);
    drop(limit);
}

#[test]
fn malformed_mips_are_not_mistaken_for_memory_pressure() {
    let renderer = renderer(true);
    let mut data = picture(8, 8);
    data.gpu_mips = false;
    data.levels[0].truncate(1);
    data.levels.push(vec![255; 4 * 4 * 4]);
    let prepared = prepare_texture(&renderer.device, &renderer.queue, &data).unwrap();
    assert_eq!(prepared.bytes(), 0);
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn memory_pressure_keeps_colour_and_cutout_alpha_on_the_gpu() {
    let mut renderer = renderer(false);
    let mut scene = renderer.new_scene();
    let data = picture(128, 128);
    let texture = renderer.add_texture_data(&mut scene, &data);
    let material =
        renderer.add_material(&mut scene, Some(texture), AlphaMode::Test, [1.0; 4], true);
    let mesh = renderer.add_mesh(
        &mut scene,
        &MeshData {
            positions: vec![
                Vec3::new(-4.0, 4.0, -4.0),
                Vec3::new(4.0, 4.0, -4.0),
                Vec3::new(4.0, 4.0, 4.0),
                Vec3::new(-4.0, 4.0, 4.0),
            ],
            normals: vec![-Vec3::Y; 4],
            uvs: vec![
                glam::Vec2::ZERO,
                glam::Vec2::X,
                glam::Vec2::ONE,
                glam::Vec2::Y,
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            ranges: vec![(0, 6, 0)],
            one_sided: false,
        },
    );
    renderer.add_instance(
        &mut scene,
        mesh,
        DVec3::ZERO,
        Mat4::IDENTITY,
        vec![material],
    );
    let camera = Camera {
        position: DVec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 100.0,
    };
    let lighting = Lighting {
        shadows: false,
        fog_density: 0.0,
        ..Default::default()
    };
    let before = renderer
        .render_to_image(&mut scene, 64, 64, &camera, &lighting)
        .unwrap();
    renderer.free_texture(&mut scene, texture);
    let limit = AllocationLimit::set(1_400);
    assert!(renderer.replace_texture(&mut scene, texture, &data));
    assert_eq!(scene.texture_size_of(texture), Some((16, 16)));
    drop(limit);
    assert_eq!(renderer.rebind_textures(&mut scene, &[texture]), 1);
    let after = renderer
        .render_to_image(&mut scene, 64, 64, &camera, &lighting)
        .unwrap();
    let pixel = |rgba: &[u8], x: usize| rgba[(32 * 64 + x) * 4..][..3].to_vec();
    assert!(pixel(&after, 16)[0] > 180 && pixel(&after, 16)[1] < 80);
    for x in [16, 48] {
        for (a, b) in pixel(&before, x).iter().zip(pixel(&after, x)) {
            assert!(
                a.abs_diff(b) <= 3,
                "colour/alpha changed at {x}: {:?} vs {:?}",
                pixel(&before, x),
                pixel(&after, x)
            );
        }
    }
}
