use super::*;

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn changed_freetex_reuses_cached_materials_and_rebinds_only_after_resize() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let renderer = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: 1,
            shadow_size: 1024,
            ..Default::default()
        },
    ))
    .expect("test renderer");
    let mut scene = renderer.new_scene();
    let picture = |width: u32, colour: &[u8]| {
        omsi_texture::TextureData::from_image(omsi_texture::Image {
            width,
            height: 4,
            rgba: colour.repeat(width as usize * 4),
            has_alpha: false,
        })
    };
    let zero = renderer.add_texture_data(&mut scene, &picture(4, &[255, 0, 0, 255]));
    let one = renderer.add_texture_data(&mut scene, &picture(4, &[0, 0, 255, 255]));
    let cached_zero =
        renderer.add_material(&mut scene, Some(zero), AlphaMode::Opaque, [1.0; 4], true);
    let cached_one =
        renderer.add_material(&mut scene, Some(one), AlphaMode::Opaque, [1.0; 4], true);
    let old_group = scene.materials[cached_zero].bind_group.clone();
    let old_generation = scene.textures[zero].generation;
    // Page one is selected while page zero's file is regenerated.
    assert_eq!(scene.materials[cached_one].texture, Some(one));
    assert!(!renderer.refresh_texture(&mut scene, zero, &picture(4, &[0, 255, 0, 255])));
    assert_eq!(scene.textures[zero].generation, old_generation);
    assert_eq!(scene.materials[cached_zero].bind_group, old_group);
    assert_eq!(
        read_first_texture_block(&renderer, &scene.textures[zero]),
        [0, 255, 0, 255]
    );
    // Selecting the cached material again sees those new pixels, without rebuilding it.
    assert_eq!(scene.materials[cached_zero].texture, Some(zero));
    // A different owner's private texture remains unchanged by a remote update.
    assert_eq!(read_first_texture_block(&renderer, &scene.textures[one]), [0, 0, 255, 255]);
    assert!(renderer.refresh_texture(&mut scene, zero, &picture(8, &[255, 255, 0, 255])));
    assert_ne!(scene.textures[zero].generation, old_generation);
    assert_eq!(renderer.rebind_textures(&mut scene, &[zero]), 1);
    assert_ne!(scene.materials[cached_zero].bind_group, old_group);
    assert_eq!(
        read_first_texture_block(&renderer, &scene.textures[zero]),
        [255, 255, 0, 255]
    );

    // Also report submission cost at the user's TFT size; no timing assertion on
    // shared CI machines. Waiting for GPU completion is outside the frame's path.
    let mut large = omsi_texture::TextureData::from_image(omsi_texture::Image {
        width: 1095,
        height: 878,
        rgba: [0, 255, 0, 255].repeat(1095 * 878),
        has_alpha: false,
    });
    let large_id = renderer.add_texture_data(&mut scene, &large);
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let mut times = Vec::new();
    for value in 0..12 {
        large.levels[0][0] = value;
        let start = std::time::Instant::now();
        assert!(!renderer.refresh_texture(&mut scene, large_id, &large));
        times.push(start.elapsed());
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
    }
    times.sort();
    eprintln!(
        "1095x878 freetex upload submission on {}: median {:.3} ms, max {:.3} ms",
        renderer.adapter_name,
        times[6].as_secs_f64() * 1000.0,
        times[11].as_secs_f64() * 1000.0
    );

    if renderer.supports_bc() {
        let red = vec![0x00, 0xf8, 0x00, 0x00, 0, 0, 0, 0];
        let green = vec![0xe0, 0x07, 0x00, 0x00, 0, 0, 0, 0];
        let mut data = omsi_texture::TextureData {
            width: 4,
            height: 4,
            format: omsi_texture::PixelFormat::Bc1,
            levels: vec![red],
            has_alpha: false,
            gpu_mips: false,
        };
        let id = renderer.add_texture_data(&mut scene, &data);
        let generation = scene.textures[id].generation;
        data.levels[0] = green.clone();
        assert!(!renderer.refresh_texture(&mut scene, id, &data));
        assert_eq!(scene.textures[id].generation, generation);
        assert_eq!(
            read_first_texture_block(&renderer, &scene.textures[id]),
            green
        );
    }
}

fn read_first_texture_block(renderer: &Renderer, texture: &GpuTexture) -> Vec<u8> {
    let format = texture.texture.format();
    let (width, height) = format.block_dimensions();
    let bytes = format.block_copy_size(None).unwrap() as usize;
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("freetex test readback"),
        size: 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(1),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue.submit([encoder.finish()]);
    let (send, receive) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        send.send(r).unwrap();
    });
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    receive.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range();
    mapped[..bytes].to_vec()
}
