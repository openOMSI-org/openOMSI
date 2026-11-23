//! Temporal upscaling: the depth prepass that writes each pixel's motion beside its depth, and
//! the sky's motion laid first, so that the upscaler has a history to work from (`TemporalState`,
//! see `temporal/frame.rs`). Made only when temporal upscaling is asked for.
//!
//! Its shader is the scene module with `temporal/motion.wgsl` appended: a module of its own, so
//! that the scene module the other pipelines compile (and the tests hold to its hash) stays as it
//! is. The motion is its bind group 3, beside the vehicle reflection's group 2.

use super::common::*;
use super::scene::SceneBase;
use crate::*;

pub(crate) fn build(
    device: &mx::Device,
    scene: &SceneBase,
    upscaler: Option<TemporalUpscaler>,
) -> Option<TemporalPipelines> {
    let upscaler = upscaler?;
    if array_path() != ArrayPath::Storage {
        log::warn!(
            "temporal upscaling: this device reads the scene's arrays from textures; drawing without it"
        );
        return None;
    }
    log::info!("renderer: compiling the temporal upscaling's motion shaders");
    let shader = compile_shader(
        device,
        mx::ShaderModuleDescriptor {
            label: Some("temporal motion"),
            source: &*(scene_shader_with(
                false,
                crate::shaders::hot_wgsl(
                    include_str!("../temporal/motion.wgsl"),
                    "temporal/motion.wgsl",
                ),
            )),
        },
    );
    let motion_target = [Some(mx::ColorTargetState {
        format: TEMPORAL_MOTION_FORMAT,
        blend: None,
        write_mask: mx::ColorWrites::ALL,
    })];
    // (as the ordinary prepass: plain, cut out, a transmap's opaque part; two-sided, culled.
    // write false: the late ones, see `DrawPlan::masked`, only where the main pass left their own
    // depth, so just the parts that are seen: with GreaterEqual, parts the picture does not show
    // there put their motion over what lies in front)
    let make = |kind: u8, cull: bool, write: bool| {
        let fragment = match kind {
            0 => "fs_motion",
            1 => "fs_motion_test",
            2 => "fs_motion_transmap",
            _ => unreachable!(),
        };
        compile_pipeline(
            device,
            &mx::RenderPipelineDescriptor {
                label: Some("temporal depth prepass"),
                vertex: mx::VertexState {
                    constants: &[],
                    module: &shader,
                    entry_point: Some("vs_motion"),
                    buffers: &[scene.vertex_layout.clone()],
                },
                primitive: one_sided_primitive(cull),
                depth_stencil: Some(mx::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: write,
                    depth_compare: if write {
                        mx::CompareFunction::GreaterEqual
                    } else {
                        mx::CompareFunction::Equal
                    },
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(mx::FragmentState {
                    constants: scene.trees,
                    module: &shader,
                    entry_point: Some(fragment),
                    targets: &motion_target,
                }),
            },
        )
    };
    let sky = compile_pipeline(
        device,
        &mx::RenderPipelineDescriptor {
            label: Some("temporal sky motion"),
            vertex: mx::VertexState {
                constants: &[],
                module: &shader,
                entry_point: Some("vs_motion_sky"),
                buffers: &[],
            },
            primitive: mx::PrimitiveState::default(),
            depth_stencil: Some(mx::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: mx::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(mx::FragmentState {
                constants: &[],
                module: &shader,
                entry_point: Some("fs_motion_sky"),
                targets: &motion_target,
            }),
        },
    );
    let max = mx::BlendComponent {
        src_factor: mx::BlendFactor::One,
        dst_factor: mx::BlendFactor::One,
        operation: mx::BlendOperation::Max,
    };
    let mask_target = [Some(mx::ColorTargetState {
        format: TEMPORAL_MASK_FORMAT,
        blend: Some(mx::BlendState {
            color: max,
            alpha: max,
        }),
        write_mask: mx::ColorWrites::ALL,
    })];
    let mask = |fragment: &str, cull: bool| {
        compile_pipeline(
            device,
            &mx::RenderPipelineDescriptor {
                label: Some("temporal transparency mask"),
                vertex: mx::VertexState {
                    constants: &[],
                    module: &shader,
                    entry_point: Some("vs_mask"),
                    buffers: &[scene.vertex_layout.clone()],
                },
                primitive: one_sided_primitive(cull),
                depth_stencil: Some(mx::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: false,
                    depth_compare: mx::CompareFunction::GreaterEqual,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(mx::FragmentState {
                    constants: &[],
                    module: &shader,
                    entry_point: Some(fragment),
                    targets: &mask_target,
                }),
            },
        )
    };
    let glass = upscaler.glass_mask_entry();
    Some(TemporalPipelines {
        prepass: [
            make(0, false, true),
            make(0, true, true),
            make(1, false, true),
            make(1, true, true),
            make(2, false, true),
            make(2, true, true),
        ],
        late_motion: [
            make(0, false, false),
            make(0, true, false),
            make(1, false, false),
            make(1, true, false),
            make(2, false, false),
            make(2, true, false),
        ],
        sky,
        glass_mask: [mask(glass, false), mask(glass, true)],
        rain_mask: [mask("fs_rain_mask", false), mask("fs_rain_mask", true)],
        screen_mask: [mask("fs_screen_mask", false), mask("fs_screen_mask", true)],
    })
}
