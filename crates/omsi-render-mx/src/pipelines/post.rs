//! The enhanced graphics' post passes (glow, metering, adaptation, tone curve, FXAA).

use super::common::*;
use crate::*;

/// post.wgsl, its groups read from the heap.
pub(crate) static POST: HeapModule = HeapModule::new(|| {
    crate::shaders::hot_wgsl(include_str!("../post.wgsl"), "post.wgsl").to_string()
});

pub(crate) struct Post {
    pub pipelines: PostPipelines,
    pub buf: mx::Buffer,
    pub sampler: mx::Sampler,
    pub meter_view: mx::TextureView,
    pub adapt_views: [mx::TextureView; 2],
    pub adapt_bg: [HeapGroup; 2],
}

pub(crate) fn build(
    device: &mx::Device,
    format: mx::TextureFormat,
    hdr_format: mx::TextureFormat,
    white_texture: &GpuTexture,
) -> Post {
    // --- enhanced graphics: the post passes (glow, metering, adaptation, tone curve, FXAA)
    log::info!("renderer: compiling the post passes shaders");
    let post_shader = compile_shader(
        device,
        mx::ShaderModuleDescriptor {
            label: Some("post"),
            source: &POST.wgsl(),
        },
    );
    let post_buf = uniform_buffer(
        device,
        "post params",
        std::mem::size_of::<PostUniform>() as u64,
    );
    let post_sampler = clamped_linear_sampler(device);
    let post_pipeline = |entry: &str, format: mx::TextureFormat| {
        no_vertex_pipeline(entry, &post_shader, entry, &[target(format, None)]).create(device)
    };
    let post = PostPipelines {
        down_first: post_pipeline("fs_down_first", hdr_format),
        down: post_pipeline("fs_down", hdr_format),
        up: post_pipeline("fs_up", hdr_format),
        meter: post_pipeline("fs_meter", hdr_format),
        adapt: post_pipeline("fs_adapt", hdr_format),
        tonemap: post_pipeline("fs_tonemap", format),
        tonemap_encoded: post_pipeline("fs_tonemap_encoded", mx::TextureFormat::Rgba8Unorm),
        fxaa: post_pipeline("fs_fxaa", format),
        history: post_pipeline("fs_history", mx::TextureFormat::Rgba16Float),
    };
    let one = mx::Extent3d {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };
    let tiny = |label: &str| {
        device
            .create_texture(&mx::TextureDescriptor {
                label: Some(label),
                size: one,
                mip_level_count: 1,
                sample_count: 1,
                dimension: mx::TextureDimension::D2,
                format: hdr_format,
                usage: mx::TextureUsages::RENDER_ATTACHMENT
                    | mx::TextureUsages::TEXTURE_BINDING
                    | mx::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
            .expect("mX: a texture")
            .create_view(&Default::default())
            .expect("mX: a texture view")
    };
    let meter_view = tiny("exposure meter");
    let adapt_views = [tiny("exposure a"), tiny("exposure b")];
    // adapt_bg[k] reads the meter and adapt[k] (and draws into the other one)
    let adapt_bg = [0usize, 1].map(|k| {
        HeapGroup::of(
            POST.layout(),
            0,
            &mx::BindGroupDescriptor {
                label: Some("exposure"),
                entries: &[
                    mx::BindGroupEntry {
                        binding: 0,
                        resource: post_buf.as_entire_binding(),
                    },
                    mx::BindGroupEntry {
                        binding: 1,
                        resource: mx::BindingResource::TextureView(&meter_view),
                    },
                    mx::BindGroupEntry {
                        binding: 2,
                        resource: mx::BindingResource::Sampler(&post_sampler),
                    },
                    mx::BindGroupEntry {
                        binding: 3,
                        resource: mx::BindingResource::TextureView(&adapt_views[k]),
                    },
                    mx::BindGroupEntry {
                        binding: 4,
                        resource: mx::BindingResource::TextureView(&white_texture.view),
                    },
                    mx::BindGroupEntry {
                        binding: 5,
                        resource: mx::BindingResource::TextureView(&white_texture.view),
                    },
                ],
            },
        )
    });
    Post {
        pipelines: post,
        buf: post_buf,
        sampler: post_sampler,
        meter_view,
        adapt_views,
        adapt_bg,
    }
}
