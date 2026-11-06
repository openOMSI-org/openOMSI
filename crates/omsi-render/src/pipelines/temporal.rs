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

pub(crate) fn build(device: &wgpu::Device, scene: &SceneBase, upscaler: Option<TemporalUpscaler>) -> Option<TemporalPipelines> {
    let upscaler = upscaler?;
    if array_path() != ArrayPath::Storage {
        log::warn!("temporal upscaling: this device reads the scene's arrays from textures; drawing without it");
        return None;
    }
    log::info!("renderer: compiling the temporal upscaling's motion shaders");
    let shader = compile_shader(device, wgpu::ShaderModuleDescriptor {
        label: Some("temporal motion"),
        source: wgpu::ShaderSource::Wgsl(format!("{}\n{}", scene_shader_source(false), include_str!("../temporal/motion.wgsl")).into()),
    });
    let motion_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("temporal motion"),
        entries: &[
            uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("temporal prepass"),
        bind_group_layouts: &[Some(&scene.camera_layout), Some(&scene.material_layout), None, Some(&motion_layout)],
        immediate_size: 0,
    });
    let motion_target = [Some(wgpu::ColorTargetState {
        format: TEMPORAL_MOTION_FORMAT,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
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
        compile_pipeline(device, &wgpu::RenderPipelineDescriptor {
            label: Some("temporal depth prepass"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_motion"),
                buffers: &[scene.vertex_layout.clone()],
                compilation_options: Default::default(),
            },
            primitive: one_sided_primitive(cull),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(write),
                depth_compare: Some(if write { wgpu::CompareFunction::GreaterEqual } else { wgpu::CompareFunction::Equal }),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(fragment),
                targets: &motion_target,
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: crate::pipeline_cache::get(device).as_ref(),
        })
    };
    let sky_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("temporal sky motion"),
        bind_group_layouts: &[None, None, None, Some(&motion_layout)],
        immediate_size: 0,
    });
    let sky = compile_pipeline(device, &wgpu::RenderPipelineDescriptor {
        label: Some("temporal sky motion"),
        layout: Some(&sky_pl),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_motion_sky"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_motion_sky"),
            targets: &motion_target,
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: crate::pipeline_cache::get(device).as_ref(),
    });
    let mask_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("temporal transparency mask"),
        bind_group_layouts: &[Some(&scene.camera_layout), Some(&scene.material_layout)],
        immediate_size: 0,
    });
    let max = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Max };
    let mask_target = [Some(wgpu::ColorTargetState {
        format: TEMPORAL_MASK_FORMAT,
        blend: Some(wgpu::BlendState { color: max, alpha: max }),
        write_mask: wgpu::ColorWrites::ALL,
    })];
    let mask = |fragment: &str, cull: bool| {
        compile_pipeline(device, &wgpu::RenderPipelineDescriptor {
            label: Some("temporal transparency mask"),
            layout: Some(&mask_pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_mask"),
                buffers: &[scene.vertex_layout.clone()],
                compilation_options: Default::default(),
            },
            primitive: one_sided_primitive(cull),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(fragment),
                targets: &mask_target,
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: crate::pipeline_cache::get(device).as_ref(),
        })
    };
    let glass = upscaler.glass_mask_entry();
    Some(TemporalPipelines {
        prepass: [make(0, false, true), make(0, true, true), make(1, false, true), make(1, true, true), make(2, false, true), make(2, true, true)],
        late_motion: [make(0, false, false), make(0, true, false), make(1, false, false), make(1, true, false), make(2, false, false), make(2, true, false)],
        sky,
        motion_layout,
        glass_mask: [mask(glass, false), mask(glass, true)],
        rain_mask: [mask("fs_rain_mask", false), mask("fs_rain_mask", true)],
        screen_mask: [mask("fs_screen_mask", false), mask("fs_screen_mask", true)],
    })
}
