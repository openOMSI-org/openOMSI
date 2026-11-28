//! Ambient occlusion, and the lamps in the fog: both read the depth prepass.

use super::common::*;
use crate::*;

pub(crate) struct Ssao {
    pub buf: mx::Buffer,
    pub sampler: mx::Sampler,
    pub ssao_pipeline: Option<mx::RenderPipeline>,
    pub blur_pipeline: Option<mx::RenderPipeline>,
    /// The same into two-channel targets (the occlusion and its depth), for every picture
    /// without the bounced light: half the memory and the bandwidth of the four-channel ones.
    pub plain_ssao_pipeline: Option<mx::RenderPipeline>,
    pub plain_blur_pipeline: Option<mx::RenderPipeline>,
}

/// ssao.wgsl, its group read from the heap.
pub(crate) static SSAO: HeapModule = HeapModule::new(|| {
    crate::shaders::hot_wgsl(include_str!("../ssao.wgsl"), "ssao.wgsl").to_string()
});

pub(crate) fn ssao(device: &mx::Device, gl: bool) -> Ssao {
    // ambient occlusion: a depth prepass with the camera projection, then the AO and a blur
    log::info!("renderer: compiling the SSAO shaders");
    let ssao_shader = compile_shader(
        device,
        mx::ShaderModuleDescriptor {
            label: Some("ssao"),
            source: &SSAO.wgsl(),
        },
    );
    let ao_buf = uniform_buffer(
        device,
        "ssao params",
        std::mem::size_of::<SsaoUniform>() as u64,
    );
    // (also the camera group's `s_ao`: the wet panes read the picture behind them through it
    // at their blur's level, see scene/08_rain_glass.wgsl `glass_blur`)
    let ao_sampler = clamped_trilinear_sampler(device);
    // (r the occlusion, g the depth it was worked out at, b and a the bounced light)
    let targets = [target(mx::TextureFormat::Rgba16Float, None)];
    let make_ao =
        |entry: &str| no_vertex_pipeline(entry, &ssao_shader, entry, &targets).create(device);
    let plain_targets = [target(mx::TextureFormat::Rg16Float, None)];
    let make_plain =
        |entry: &str| no_vertex_pipeline(entry, &ssao_shader, entry, &plain_targets).create(device);
    let ssao_pipeline = (!gl).then(|| make_ao("fs_ssao"));
    let blur_pipeline = (!gl).then(|| make_ao("fs_blur"));
    let plain_ssao_pipeline = (!gl).then(|| make_plain("fs_ssao"));
    let plain_blur_pipeline = (!gl).then(|| make_plain("fs_blur"));
    Ssao {
        buf: ao_buf,
        sampler: ao_sampler,
        ssao_pipeline,
        blur_pipeline,
        plain_ssao_pipeline,
        plain_blur_pipeline,
    }
}

pub(crate) struct FogLamps {
    pub buf: mx::Buffer,
    pub pipeline: Option<[mx::RenderPipeline; 2]>,
    /// the module's places in the heap (`groups_to_heap`)
    pub layout: crate::shaders::HeapLayout,
}

pub(crate) fn fog_lamps(device: &mx::Device, hdr_format: mx::TextureFormat, gl: bool) -> FogLamps {
    // the lamps in the fog: the camera group (lights, grid, the enhanced uniform) and the
    // prepass depth, added onto the high-range picture
    let fog_lamps_buf = uniform_buffer(
        device,
        "fog lamps params",
        std::mem::size_of::<FogLampUniform>() as u64,
    );
    let fog_lamps_pipeline = (!gl && !array_path().no_storage() && !basic_pipelines())
        .then(|| {
            optional_pipeline(device, "the lamps in the fog", || {
                log::info!("renderer: compiling the lamps in the fog shaders");
                let module = compile_shader(
                    device,
                    mx::ShaderModuleDescriptor {
                        label: Some("fog lamps"),
                        source: &crate::shaders::groups_to_heap(&fog_lamps_shader_source()),
                    },
                );
                let make = |entry: &str, blend: Option<mx::BlendState>| {
                    no_vertex_pipeline(entry, &module, entry, &[target(hdr_format, blend)])
                        .vs("vs_fog_lamps")
                        .create(device)
                };
                // (the second added onto what is there)
                let add = mx::BlendState {
                    color: mx::BlendComponent {
                        src_factor: mx::BlendFactor::One,
                        dst_factor: mx::BlendFactor::One,
                        operation: mx::BlendOperation::Add,
                    },
                    alpha: mx::BlendComponent {
                        src_factor: mx::BlendFactor::Zero,
                        dst_factor: mx::BlendFactor::One,
                        operation: mx::BlendOperation::Add,
                    },
                };
                [
                    make("fs_fog_lamps", None),
                    make("fs_fog_composite", Some(add)),
                ]
            })
        })
        .flatten();
    FogLamps {
        buf: fog_lamps_buf,
        pipeline: fog_lamps_pipeline,
        layout: crate::shaders::layout_of(&fog_lamps_shader_source()),
    }
}
