//! Enhanced lighting: the uniform, the sky table, the reflection probe and the sky cube.

use super::common::*;
use super::sky::SkyBase;
use crate::*;

pub(crate) struct Enhanced {
    pub enh_buf: wgpu::Buffer,
    pub sky_lut: wgpu::Texture,
    pub sky_lut_view: wgpu::TextureView,
    pub lin_sampler: wgpu::Sampler,
    /// None when the enhanced passes are omitted (preview or the GL compatibility path).
    pub probe: Option<Probe>,
}

/// What the probe's passes read besides the sky.
struct Lighting<'a> {
    camera_buf: &'a wgpu::Buffer,
    enh_buf: &'a wgpu::Buffer,
    lin_sampler: &'a wgpu::Sampler,
    sky_lut_view: &'a wgpu::TextureView,
}

pub(crate) fn build(device: &wgpu::Device, queue: &wgpu::Queue, hdr_format: wgpu::TextureFormat, camera_buf: &wgpu::Buffer, sky: &SkyBase, with_probe: bool) -> Enhanced {
    // --- enhanced lighting: the uniform, the sky table, the reflection probe
    let enh_buf = uniform_buffer(device, "enhanced lighting", std::mem::size_of::<EnhancedUniform>() as u64);
    let sky_lut = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sky table"),
        size: wgpu::Extent3d {
            width: SKY_TEX_W,
            height: SKY_TEX_H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: hdr_format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let sky_lut_view = sky_lut.create_view(&Default::default());
    let lin_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let probe_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("probe"),
        entries: &[
            uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
            uniform_entry(11, wgpu::ShaderStages::VERTEX_FRAGMENT),
            sampler_entry(13),
            float_texture_entry(14),
            uniform_entry(15, wgpu::ShaderStages::VERTEX_FRAGMENT),
            texture_entry(16, wgpu::TextureSampleType::Float { filterable: true }, wgpu::TextureViewDimension::Cube),
        ],
    });
    let lighting = Lighting { camera_buf, enh_buf: &enh_buf, lin_sampler: &lin_sampler, sky_lut_view: &sky_lut_view };
    let probe = with_probe.then(|| probe(device, queue, hdr_format, &probe_layout, &lighting, sky));
    Enhanced { enh_buf, sky_lut, sky_lut_view, lin_sampler, probe }
}

/// A bind group of the probe's layout reading `src` with the pass parameters in `pass`.
fn probe_bind_group(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::BindGroupLayout,
    lighting: &Lighting,
    pass: &wgpu::Buffer,
    src: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: lighting.camera_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 11, resource: lighting.enh_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 13, resource: wgpu::BindingResource::Sampler(lighting.lin_sampler) },
            wgpu::BindGroupEntry { binding: 14, resource: wgpu::BindingResource::TextureView(lighting.sky_lut_view) },
            wgpu::BindGroupEntry { binding: 15, resource: pass.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 16, resource: wgpu::BindingResource::TextureView(src) },
        ],
    })
}

fn probe(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    hdr_format: wgpu::TextureFormat,
    probe_layout: &wgpu::BindGroupLayout,
    lighting: &Lighting,
    sky: &SkyBase,
) -> Probe {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("reflection probe"),
        size: wgpu::Extent3d {
            width: PROBE_SIZE,
            height: PROBE_SIZE,
            depth_or_array_layers: 6,
        },
        mip_level_count: PROBE_MIPS,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: hdr_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let cube = |base: u32, count: u32| {
        tex.create_view(&wgpu::TextureViewDescriptor {
            label: Some("probe cube"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            base_mip_level: base,
            mip_level_count: Some(count),
            base_array_layer: 0,
            array_layer_count: Some(6),
            ..Default::default()
        })
    };
    let view = cube(0, PROBE_MIPS);
    let faces: Vec<Vec<wgpu::TextureView>> = (0..PROBE_MIPS)
        .map(|m| {
            (0..6)
                .map(|f| {
                    tex.create_view(&wgpu::TextureViewDescriptor {
                        label: Some("probe face"),
                        dimension: Some(wgpu::TextureViewDimension::D2),
                        base_mip_level: m,
                        mip_level_count: Some(1),
                        base_array_layer: f,
                        array_layer_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect()
        })
        .collect();
    // level 0 is drawn from the sky and reads nothing: a black cube stands in
    let dummy = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("probe placeholder"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 6,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: hdr_format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let dummy_view = dummy.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    });
    let cube_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sky cube"),
        size: wgpu::Extent3d {
            width: SKY_CUBE_SIZE,
            height: SKY_CUBE_SIZE,
            depth_or_array_layers: 6,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: hdr_format,
        // (copied into: the cube for a new eye is filled beside it, see `SkySched`)
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let cube_view = cube_tex.create_view(&wgpu::TextureViewDescriptor {
        label: Some("sky cube"),
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    });
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
                let buf = buffer_init(device, queue, Some("probe pass"), bytemuck::cast_slice(&[
                        first as f32,
                        rough,
                        PROBE_SIZE as f32,
                        m as f32,
                    ]), wgpu::BufferUsages::UNIFORM);
                probe_bind_group(device, "probe", probe_layout, lighting, &buf, &src)
            })
        })
        .collect();
    let probe_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("probe"),
        bind_group_layouts: &[Some(probe_layout), Some(&sky.layout)],
        immediate_size: 0,
    });
    let plain = target(hdr_format, None);
    let targets = [plain.clone(), plain.clone(), plain.clone()];
    let probe_pipeline = |entry: &str| no_vertex_pipeline(entry, &probe_pl, &sky.shader, entry, &targets).vs("vs_probe").create(device);
    let cube_faces: Vec<wgpu::TextureView> = (0..6)
        .map(|f| {
            cube_tex.create_view(&wgpu::TextureViewDescriptor {
                label: Some("sky cube face"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_mip_level: 0,
                mip_level_count: Some(1),
                base_array_layer: f,
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    // per face and redraw round: the round picks where the clouds' steps start
    let cube_bind_groups: Vec<wgpu::BindGroup> = (0..6 * SKY_CUBE_ROUNDS)
        .map(|k| {
            let (f, round) = (k / SKY_CUBE_ROUNDS, k % SKY_CUBE_ROUNDS);
            let buf = buffer_init(device, queue, Some("sky cube face"), bytemuck::cast_slice(&[f as f32, round as f32, SKY_CUBE_SIZE as f32, 0.0]), wgpu::BufferUsages::UNIFORM);
            probe_bind_group(device, "sky cube", probe_layout, lighting, &buf, &dummy_view)
        })
        .collect();
    // a redraw is blended into what the face holds (the blend constant is
    // the old picture's share): the clouds' grain averages out
    let redraw_blend = wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::OneMinusConstant,
            dst_factor: wgpu::BlendFactor::Constant,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::OneMinusConstant,
            dst_factor: wgpu::BlendFactor::Constant,
            operation: wgpu::BlendOperation::Add,
        },
    };
    let cube_pipeline = no_vertex_pipeline("sky cube", &probe_pl, &sky.shader, "fs_sky_cube", &[target(hdr_format, Some(redraw_blend))])
        .vs("vs_probe")
        .create(device);
    Probe {
        view,
        faces,
        bind_groups,
        sky_pipeline: probe_pipeline("fs_probe_sky"),
        filter_pipeline: probe_pipeline("fs_probe_filter"),
        scale: 1.0,
        cube_view,
        cube_tex,
        cube_faces,
        cube_bind_groups,
        cube_pipeline,
        cube_next: 0,
        cube_filled: false,
        cube_round: 0,
        cube_eye: None,
        sched: SkySched::default(),
    }
}
