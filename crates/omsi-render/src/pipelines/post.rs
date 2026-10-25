//! The enhanced graphics' post passes (glow, metering, adaptation, tone curve, FXAA).

use super::common::*;
use crate::*;

pub(crate) struct Post {
    pub pipelines: PostPipelines,
    pub layout: wgpu::BindGroupLayout,
    pub buf: wgpu::Buffer,
    pub sampler: wgpu::Sampler,
    pub meter_view: wgpu::TextureView,
    pub adapt_views: [wgpu::TextureView; 2],
    pub adapt_bg: [wgpu::BindGroup; 2],
}

pub(crate) fn build(device: &wgpu::Device, format: wgpu::TextureFormat, hdr_format: wgpu::TextureFormat, white_texture: &GpuTexture) -> Post {
    // --- enhanced graphics: the post passes (glow, metering, adaptation, tone curve, FXAA)
    log::info!("renderer: compiling the post passes shaders");
    let post_shader = compile_shader(device, wgpu::ShaderModuleDescriptor {
        label: Some("post"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../post.wgsl").into()),
    });
    let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("post"),
        entries: &[
            uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
            float_texture_entry(1),
            sampler_entry(2),
            float_texture_entry(3),
            float_texture_entry(4),
            // (the screen mask for the tone mapping: the condensation on the bus's
            // panes, see post.wgsl `misted`)
            float_texture_entry(5),
        ],
    });
    let post_buf = uniform_buffer(device, "post params", std::mem::size_of::<PostUniform>() as u64);
    let post_sampler = clamped_linear_sampler(device);
    let post_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("post"),
        bind_group_layouts: &[Some(&post_layout)],
        immediate_size: 0,
    });
    let post_pipeline = |entry: &str, format: wgpu::TextureFormat| {
        no_vertex_pipeline(entry, &post_pl, &post_shader, entry, &[target(format, None)]).create(device)
    };
    let post = PostPipelines {
        down_first: post_pipeline("fs_down_first", hdr_format),
        down: post_pipeline("fs_down", hdr_format),
        up: post_pipeline("fs_up", hdr_format),
        meter: post_pipeline("fs_meter", hdr_format),
        adapt: post_pipeline("fs_adapt", hdr_format),
        tonemap: post_pipeline("fs_tonemap", format),
        tonemap_encoded: post_pipeline("fs_tonemap_encoded", wgpu::TextureFormat::Rgba8Unorm),
        fxaa: post_pipeline("fs_fxaa", format),
        history: post_pipeline("fs_history", wgpu::TextureFormat::Rgba16Float),
    };
    let one = wgpu::Extent3d {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };
    let tiny = |label: &str| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: one,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: hdr_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
            .create_view(&Default::default())
    };
    let meter_view = tiny("exposure meter");
    let adapt_views = [tiny("exposure a"), tiny("exposure b")];
    // adapt_bg[k] reads the meter and adapt[k] (and draws into the other one)
    let adapt_bg = [0usize, 1].map(|k| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("exposure"),
            layout: &post_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: post_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&meter_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&post_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&adapt_views[k]),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&white_texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&white_texture.view),
                },
            ],
        })
    });
    Post { pipelines: post, layout: post_layout, buf: post_buf, sampler: post_sampler, meter_view, adapt_views, adapt_bg }
}
