//! Enhanced lighting: the uniform, the sky table, the reflection probe and the sky cube.

use super::common::*;
use super::sky::SkyBase;
use crate::*;

pub(crate) struct Enhanced {
    pub enh_buf: mx::Buffer,
    pub sky_lut: mx::Texture,
    pub sky_lut_view: mx::TextureView,
    pub lin_sampler: mx::Sampler,
    /// None when the enhanced passes are omitted (preview or the GL compatibility path).
    pub probe: Option<Probe>,
}

/// What the probe's passes read besides the sky.
struct Lighting<'a> {
    camera_buf: &'a mx::Buffer,
    enh_buf: &'a mx::Buffer,
    lin_sampler: &'a mx::Sampler,
    sky_lut_view: &'a mx::TextureView,
    /// The clouds' base volume and curl map (the passes that march them).
    cloud_base: &'a mx::TextureView,
    cloud_curl: &'a mx::TextureView,
}

pub(crate) fn build(
    device: &mx::Device,
    queue: &mx::Queue,
    hdr_format: mx::TextureFormat,
    camera_buf: &mx::Buffer,
    sky: &SkyBase,
    with_probe: bool,
) -> Enhanced {
    // --- enhanced lighting: the uniform, the sky table, the reflection probe
    let enh_buf = uniform_buffer(
        device,
        "enhanced lighting",
        std::mem::size_of::<EnhancedUniform>() as u64,
    );
    let sky_lut = device
        .create_texture(&mx::TextureDescriptor {
            label: Some("sky table"),
            size: mx::Extent3d {
                width: SKY_TEX_W,
                height: SKY_TEX_H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: mx::TextureDimension::D2,
            format: hdr_format,
            usage: mx::TextureUsages::TEXTURE_BINDING | mx::TextureUsages::COPY_DST,
            view_formats: &[],
        })
        .expect("mX: a texture");
    let sky_lut_view = sky_lut
        .create_view(&Default::default())
        .expect("mX: a texture view");
    let lin_sampler = device
        .create_sampler(&mx::SamplerDescriptor {
            address_mode_u: mx::AddressMode::ClampToEdge,
            address_mode_v: mx::AddressMode::ClampToEdge,
            address_mode_w: mx::AddressMode::ClampToEdge,
            mag_filter: mx::FilterMode::Linear,
            min_filter: mx::FilterMode::Linear,
            mipmap_filter: mx::MipmapFilterMode::Linear,
            ..Default::default()
        })
        .expect("mX: a sampler");
    let lighting = Lighting {
        camera_buf,
        enh_buf: &enh_buf,
        lin_sampler: &lin_sampler,
        sky_lut_view: &sky_lut_view,
        cloud_base: &sky.cloud_base_view,
        cloud_curl: &sky.cloud_curl_view,
    };
    let probe = with_probe.then(|| probe(device, queue, hdr_format, &lighting, sky));
    Enhanced {
        enh_buf,
        sky_lut,
        sky_lut_view,
        lin_sampler,
        probe,
    }
}

/// A bind group of the probe's layout reading `src` with the pass parameters in `pass`.
fn probe_bind_group(
    device: &mx::Device,
    label: &str,
    lighting: &Lighting,
    pass: &mx::Buffer,
    src: &mx::TextureView,
) -> mx::BindGroup {
    device.create_bind_group(&mx::BindGroupDescriptor {
        label: Some(label),
        entries: &[
            mx::BindGroupEntry {
                binding: 0,
                resource: lighting.camera_buf.as_entire_binding(),
            },
            mx::BindGroupEntry {
                binding: 11,
                resource: lighting.enh_buf.as_entire_binding(),
            },
            mx::BindGroupEntry {
                binding: 13,
                resource: mx::BindingResource::Sampler(lighting.lin_sampler),
            },
            mx::BindGroupEntry {
                binding: 14,
                resource: mx::BindingResource::TextureView(lighting.sky_lut_view),
            },
            mx::BindGroupEntry {
                binding: 15,
                resource: pass.as_entire_binding(),
            },
            mx::BindGroupEntry {
                binding: 16,
                resource: mx::BindingResource::TextureView(src),
            },
            mx::BindGroupEntry {
                binding: 20,
                resource: mx::BindingResource::TextureView(lighting.cloud_base),
            },
            mx::BindGroupEntry {
                binding: 21,
                resource: mx::BindingResource::TextureView(lighting.cloud_curl),
            },
        ],
    })
}

fn probe(
    device: &mx::Device,
    queue: &mx::Queue,
    hdr_format: mx::TextureFormat,
    lighting: &Lighting,
    sky: &SkyBase,
) -> Probe {
    let tex = device
        .create_texture(&mx::TextureDescriptor {
            label: Some("reflection probe"),
            size: mx::Extent3d {
                width: PROBE_SIZE,
                height: PROBE_SIZE,
                depth_or_array_layers: 6,
            },
            mip_level_count: PROBE_MIPS,
            sample_count: 1,
            dimension: mx::TextureDimension::D2,
            format: hdr_format,
            usage: mx::TextureUsages::RENDER_ATTACHMENT | mx::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .expect("mX: a texture");
    let cube = |base: u32, count: u32| {
        tex.create_view(&mx::TextureViewDescriptor {
            label: Some("probe cube"),
            dimension: Some(mx::TextureViewDimension::Cube),
            base_mip_level: base,
            mip_level_count: Some(count),
            base_array_layer: 0,
            array_layer_count: Some(6),
            ..Default::default()
        })
        .expect("mX: a texture view")
    };
    let view = cube(0, PROBE_MIPS);
    let faces: Vec<Vec<mx::TextureView>> = (0..PROBE_MIPS)
        .map(|m| {
            (0..6)
                .map(|f| {
                    tex.create_view(&mx::TextureViewDescriptor {
                        label: Some("probe face"),
                        dimension: Some(mx::TextureViewDimension::D2),
                        base_mip_level: m,
                        mip_level_count: Some(1),
                        base_array_layer: f,
                        array_layer_count: Some(1),
                        ..Default::default()
                    })
                    .expect("mX: a texture view")
                })
                .collect()
        })
        .collect();
    // level 0 is drawn from the sky and reads nothing: a black cube stands in
    let dummy = device
        .create_texture(&mx::TextureDescriptor {
            label: Some("probe placeholder"),
            size: mx::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: mx::TextureDimension::D2,
            format: hdr_format,
            usage: mx::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .expect("mX: a texture");
    let dummy_view = dummy
        .create_view(&mx::TextureViewDescriptor {
            dimension: Some(mx::TextureViewDimension::Cube),
            ..Default::default()
        })
        .expect("mX: a texture view");
    let cube_tex = device
        .create_texture(&mx::TextureDescriptor {
            label: Some("sky cube"),
            size: mx::Extent3d {
                width: SKY_CUBE_SIZE,
                height: SKY_CUBE_SIZE,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: mx::TextureDimension::D2,
            format: hdr_format,
            usage: mx::TextureUsages::RENDER_ATTACHMENT | mx::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .expect("mX: a texture");
    let cube_view = cube_tex
        .create_view(&mx::TextureViewDescriptor {
            label: Some("sky cube"),
            dimension: Some(mx::TextureViewDimension::Cube),
            ..Default::default()
        })
        .expect("mX: a texture view");
    let bind_groups = (0..PROBE_MIPS)
        .map(|m| {
            // level 0 is the sky cube (clouds and all) looked up, the others the
            // level above blurred
            let src = if m == 0 {
                cube_view.clone()
            } else {
                cube(0, m)
            };
            [0u32, 3].map(|first| {
                let rough = m as f32 / (PROBE_MIPS - 1) as f32;
                let buf = buffer_init(
                    device,
                    queue,
                    Some("probe pass"),
                    bytemuck::cast_slice(&[first as f32, rough, PROBE_SIZE as f32, m as f32]),
                    mx::BufferUsages::UNIFORM,
                );
                probe_bind_group(device, "probe", lighting, &buf, &src)
            })
        })
        .collect();
    let plain = target(hdr_format, None);
    let targets = [plain.clone(), plain.clone(), plain.clone()];
    let probe_pipeline = |entry: &str| {
        no_vertex_pipeline(entry, &sky.shader, entry, &targets)
            .vs("vs_probe")
            .create(device)
    };
    let cube_faces: Vec<mx::TextureView> = (0..6)
        .map(|f| {
            cube_tex
                .create_view(&mx::TextureViewDescriptor {
                    label: Some("sky cube face"),
                    dimension: Some(mx::TextureViewDimension::D2),
                    base_mip_level: 0,
                    mip_level_count: Some(1),
                    base_array_layer: f,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
                .expect("mX: a texture view")
        })
        .collect();
    // per face and redraw round: the round picks where the clouds' steps start
    let cube_bind_groups: Vec<mx::BindGroup> = (0..6 * SKY_CUBE_ROUNDS)
        .map(|k| {
            let (f, round) = (k / SKY_CUBE_ROUNDS, k % SKY_CUBE_ROUNDS);
            let buf = buffer_init(
                device,
                queue,
                Some("sky cube face"),
                bytemuck::cast_slice(&[f as f32, round as f32, SKY_CUBE_SIZE as f32, 0.0]),
                mx::BufferUsages::UNIFORM,
            );
            probe_bind_group(device, "sky cube", lighting, &buf, &dummy_view)
        })
        .collect();
    // a redraw is blended into what the face holds (the blend constant is
    // the old picture's share): the clouds' grain averages out
    let redraw_blend = mx::BlendState {
        color: mx::BlendComponent {
            src_factor: mx::BlendFactor::OneMinusConstant,
            dst_factor: mx::BlendFactor::Constant,
            operation: mx::BlendOperation::Add,
        },
        alpha: mx::BlendComponent {
            src_factor: mx::BlendFactor::OneMinusConstant,
            dst_factor: mx::BlendFactor::Constant,
            operation: mx::BlendOperation::Add,
        },
    };
    let cube_pipeline = no_vertex_pipeline(
        "sky cube",
        &sky.shader,
        "fs_sky_cube",
        &[target(hdr_format, Some(redraw_blend))],
    )
    .vs("vs_probe")
    .create(device);
    let clouds = crate::cloud_pass::CloudPass::new(
        device,
        queue,
        &sky.shader,
        lighting.lin_sampler,
        &|round| probe_bind_group(device, "cloud trace", lighting, round, &dummy_view),
    );
    Probe {
        view,
        faces,
        bind_groups,
        sky_pipeline: probe_pipeline("fs_probe_sky"),
        filter_pipeline: probe_pipeline("fs_probe_filter"),
        scale: 1.0,
        cube_faces,
        cube_bind_groups,
        cube_pipeline,
        cube_next: 0,
        cube_filled: false,
        cube_round: 0,
        sched: SkySched::default(),
        clouds,
    }
}
