//! The sun's (and the street lamps') shadow maps and the pipelines drawing into them.

use super::common::*;
use super::scene::SceneBase;
use crate::*;

pub(crate) struct Shadows {
    pub size: u32,
    pub view: wgpu::TextureView,
    pub view_far: wgpu::TextureView,
    /// Where the lamps' tiles lie under the far cascade.
    pub lamp_atlas: LampAtlas,
    pub sampler: wgpu::Sampler,
    pub clear_pipeline: wgpu::RenderPipeline,
    pub pipelines: Vec<wgpu::RenderPipeline>,
}

/// The next size tried when the graphics card has no memory left for shadow maps of `size`
/// texels: half of it, down to 512 - None for the last size tried.
fn smaller_shadow_size(size: u32) -> Option<u32> {
    (size > 512).then_some(size / 2)
}

/// `lamps`: the picture is an Enhanced one, which looks the lamps' shadows up (the plain ones
/// have no tiles under the far map, nor pipelines to draw them).
pub(crate) fn build(device: &wgpu::Device, scene: &SceneBase, shadow_size: u32, lamps: bool, cube: bool) -> Shadows {
    let depth_map = |label: &str, width: u32, height: u32| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    };
    let mut size = shadow_size;
    let (lamp_atlas, shadow_tex, shadow_tex_far) = loop {
        // (the lamps' tiles are left out where their pipelines are: see `basic_pipelines`)
        let lamp_atlas = LampAtlas::new(size, lamps && !basic_pipelines(), device.limits().max_texture_dimension_2d, cube);
        // (the smallest size is made outside the scope: a card with no room even for it fails
        // the renderer's build in the caller's scope - see `build_with_fallbacks` - instead of
        // drawing every frame with invalid shadow maps)
        let smaller = smaller_shadow_size(size);
        let scope = smaller.map(|_| device.push_error_scope(wgpu::ErrorFilter::OutOfMemory));
        // near cascade on the left, close cascade on the right
        let near = depth_map("shadow map", size * 2, size);
        // (and the lamps' tiles under it, see `LampAtlas`)
        let far = depth_map("shadow map far", size, lamp_atlas.height());
        match (scope.map(|scope| pollster::block_on(scope.pop())), smaller) {
            (Some(Some(_)), Some(smaller)) => {
                log::warn!("renderer: no graphics memory left for a shadow map of {size} texels: trying {smaller}");
                size = smaller;
            }
            _ => break (lamp_atlas, near, far),
        }
    };
    if lamp_atlas.rows == 0 && lamps && !basic_pipelines() {
        log::warn!("renderer: no room for the lamps' shadow tiles under a far shadow map of {size} texels: the lamps cast no shadows");
    }
    let shadow_view = shadow_tex.create_view(&Default::default());
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
    let make_shadow = |kind: u8, cascade: u8| {
        compile_pipeline(device, &wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &scene.shader,
                entry_point: Some(match cascade {
                    0 => "vs_shadow",
                    1 => "vs_shadow_far",
                    2 => "vs_shadow_close",
                    _ => "vs_shadow_lamp",
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
                entry_point: Some(if kind == PIPE_ALPHA_TEST { "fs_shadow_test" } else { "fs_shadow" }),
                targets: &[],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: crate::pipeline_cache::get(device).as_ref(),
        })
    };
    let shadow_clear_pipeline = clear_pipeline(device);
    let mut shadow_pipelines = vec![
        make_shadow(PIPE_OPAQUE, 0),
        make_shadow(PIPE_ALPHA_TEST, 0),
        make_shadow(PIPE_OPAQUE, 1),
        make_shadow(PIPE_ALPHA_TEST, 1),
        make_shadow(PIPE_OPAQUE, 2),
        make_shadow(PIPE_ALPHA_TEST, 2),
    ];
    if lamp_atlas.rows > 0 {
        // (the lamps' tiles, 6 + kind: one vertex shader draws every tile, reading its matrix
        // by the tile named in the draw list's entries; only where there are tiles)
        log::info!("renderer: compiling the lamps' shadow shaders");
        // (left out alone where the driver fails on them: the lamps then cast no shadows,
        // `shadow_pipelines.len() > 6`)
        let tile_pipelines = optional_pipeline(device, "the lamps' shadows", || {
            vec![make_shadow(PIPE_OPAQUE, 3), make_shadow(PIPE_ALPHA_TEST, 3)]
        });
        shadow_pipelines.extend(tile_pipelines.into_iter().flatten());
    }
    Shadows {
        size,
        view: shadow_view,
        view_far: shadow_view_far,
        lamp_atlas,
        sampler: shadow_sampler,
        clear_pipeline: shadow_clear_pipeline,
        pipelines: shadow_pipelines,
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

#[cfg(test)]
mod fallback_tests {
    use super::smaller_shadow_size;

    #[test]
    fn shadow_maps_halve_down_to_512_then_stop() {
        let mut tried = vec![4096];
        while let Some(next) = smaller_shadow_size(*tried.last().unwrap()) {
            tried.push(next);
        }
        assert_eq!(tried, [4096, 2048, 1024, 512]);
        // (the last size has no fallback, so it is made outside the out-of-memory scope and a
        // failure there reaches the renderer's build instead of leaving invalid textures)
        assert_eq!(smaller_shadow_size(512), None);
        assert_eq!(smaller_shadow_size(256), None);
        assert_eq!(smaller_shadow_size(768), Some(384));
    }
}
