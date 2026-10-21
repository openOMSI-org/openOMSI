//! The sun's (and the street lamps') shadow maps and the pipelines drawing into them.

use super::common::*;
use super::scene::SceneBase;
use crate::*;

pub(crate) struct Shadows {
    pub view: wgpu::TextureView,
    pub view_far: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub clear_pipeline: wgpu::RenderPipeline,
    pub pipelines: Vec<wgpu::RenderPipeline>,
    pub precip_pipelines: [wgpu::RenderPipeline; 2],
}

pub(crate) fn build(device: &wgpu::Device, scene: &SceneBase, shadow_size: u32) -> Shadows {
    // sun shadow map: depth only, from the light's orthographic camera
    let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadow map"),
        size: wgpu::Extent3d {
            // near cascade on the left, close cascade on the right
            width: shadow_size * 2,
            height: shadow_size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let shadow_view = shadow_tex.create_view(&Default::default());
    let shadow_tex_far = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadow map far"),
        // (and the street lamps' tiles under it, see `LAMP_SHADOWS`)
        size: wgpu::Extent3d {
            width: shadow_size,
            height: (shadow_size as f32 * FAR_MAP_ASPECT) as u32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let shadow_view_far = shadow_tex_far.create_view(&Default::default());
    let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    });
    let shadow_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow"),
            bind_group_layouts: &[Some(&scene.shadow_layout), Some(&scene.material_layout)],
            immediate_size: 0,
        });
    let make_shadow = |kind: u8, cascade: u8, precip: bool| {
        let precip_target = [Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba8Unorm,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];
        compile_pipeline(device, &wgpu::RenderPipelineDescriptor {
            label: Some(if precip { "precipitation roof" } else { "shadow" }),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &scene.shader,
                entry_point: Some(match cascade {
                    0 => "vs_shadow",
                    1 => "vs_shadow_far",
                    2 => "vs_shadow_close",
                    3 => "vs_shadow_lamp0",
                    4 => "vs_shadow_lamp1",
                    5 => "vs_shadow_lamp2",
                    _ => "vs_shadow_lamp3",
                }),
                buffers: &[scene.vertex_layout.clone()],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                front_face: wgpu::FrontFace::Cw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: 4,
                    slope_scale: 3.0,
                    clamp: 0.0,
                },
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &scene.shader,
                entry_point: Some(match (precip, kind == PIPE_ALPHA_TEST) {
                    (true, true) => "fs_precip_test",
                    (true, false) => "fs_precip",
                    (false, true) => "fs_shadow_test",
                    (false, false) => "fs_shadow",
                }),
                targets: if precip { &precip_target } else { &[] },
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: crate::pipeline_cache::get(device).as_ref(),
        })
    };
    let shadow_clear_pipeline = clear_pipeline(device);
    let mut shadow_pipelines = vec![
        make_shadow(PIPE_OPAQUE, 0, false),
        make_shadow(PIPE_ALPHA_TEST, 0, false),
        make_shadow(PIPE_OPAQUE, 1, false),
        make_shadow(PIPE_ALPHA_TEST, 1, false),
        make_shadow(PIPE_OPAQUE, 2, false),
        make_shadow(PIPE_ALPHA_TEST, 2, false),
    ];
    let precip_pipelines = [make_shadow(PIPE_OPAQUE, 2, true), make_shadow(PIPE_ALPHA_TEST, 2, true)];
    if !basic_pipelines() {
        // (the street lamps' tiles, 6 + 2 k + kind)
        log::info!("renderer: compiling the street lamps' shadow shaders");
        // (left out alone where the driver fails on them: the lamps then cast no shadows,
        // `shadow_pipelines.len() > 6`)
        let lamps = optional_pipeline(device, "the street lamps' shadows", || {
            (3..=6).flat_map(|cascade| [make_shadow(PIPE_OPAQUE, cascade, false), make_shadow(PIPE_ALPHA_TEST, cascade, false)]).collect::<Vec<_>>()
        });
        shadow_pipelines.extend(lamps.into_iter().flatten());
    }
    Shadows {
        view: shadow_view,
        view_far: shadow_view_far,
        sampler: shadow_sampler,
        clear_pipeline: shadow_clear_pipeline,
        pipelines: shadow_pipelines,
        precip_pipelines,
    }
}

fn clear_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    log::info!("renderer: compiling the shadow clear shader");
    let module = compile_shader(device, wgpu::ShaderModuleDescriptor {
        label: Some("shadow clear"),
        source: wgpu::ShaderSource::Wgsl(
            "@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
                        let x = f32(i32(i & 1u) * 4 - 1);
                        let y = f32(i32(i >> 1u) * 4 - 1);
                        return vec4<f32>(x, y, 1.0, 1.0);
                    }"
            .into(),
        ),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("shadow clear"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    compile_pipeline(device, &wgpu::RenderPipelineDescriptor {
        label: Some("shadow clear"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: None,
        multiview_mask: None,
        cache: crate::pipeline_cache::get(device).as_ref(),
    })
}
