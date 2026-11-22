//! The scene shader, its bind group layouts and the main pass's mesh pipelines; the depth
//! prepass drawn with the same vertex shader.

use super::common::*;
use crate::*;

/// The scene shader and what every pipeline drawing meshes with it shares.
#[derive(Clone)]
pub(crate) struct SceneBase {
    pub shader: mx::ShaderModule,
    pub lm_atlas: mx::Texture,
    pub lm_uniform: mx::Buffer,
    pub snow_track: mx::Buffer,
    pub snow_track_uniform: mx::Buffer,
    pub vertex_layout: mx::VertexBufferLayout<'static>,
    bias: i32,
    pub(crate) trees: &'static [(&'static str, f64)],
}

/// The ground albedo picture's format (see `fs_ground`): sRGB, so that the layers blend as light.
pub(crate) const GROUND_FORMAT: mx::TextureFormat = mx::TextureFormat::Rgba8UnormSrgb;

const VERTEX_ATTRIBUTES: [mx::VertexAttribute; 3] =
    mx::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];

impl SceneBase {
    /// `omit_enhanced`: the camera group without the enhanced path's textures (see
    /// `camera_layout_entries`).
    pub(crate) fn new(device: &mx::Device, omit_enhanced: bool, detailed_trees: bool) -> SceneBase {
        // One module for both paths: the enhanced fragment shader shares the vertex shader,
        // which the depth prepass relies on to the last bit (see `VsOut::clip`).
        log::info!("renderer: compiling the scene shaders");
        let shader = compile_shader(
            device,
            mx::ShaderModuleDescriptor {
                label: Some("omsi"),
                source: &*(scene_shader_source(
                    GL_BACKEND.load(std::sync::atomic::Ordering::Relaxed),
                )),
            },
        );
        let _ = omit_enhanced;
        let lm_atlas = device
            .create_texture(&mx::TextureDescriptor {
                label: Some("light map atlas"),
                size: mx::Extent3d {
                    width: LM_ATLAS_TILES * LM_TILE_PX,
                    height: LM_ATLAS_TILES * LM_TILE_PX,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: mx::TextureDimension::D2,
                format: mx::TextureFormat::Rgba8UnormSrgb,
                usage: mx::TextureUsages::TEXTURE_BINDING | mx::TextureUsages::COPY_DST,
                view_formats: &[],
            })
            .expect("mX: a texture");
        let lm_uniform = uniform_buffer(device, "light map atlas place", 16);
        let snow_track = device
            .create_buffer(&mx::BufferDescriptor {
                label: Some("snow track field"),
                // (a token until the roads' snow first sends it ruts or tracks: the 16 MB field is
                // made then, see `set_snow_track_rows` - not in a session without snow, nor where
                // the device does not draw it)
                size: SNOW_TRACK_TOKEN,
                usage: mx::BufferUsages::STORAGE | mx::BufferUsages::COPY_DST,
            })
            .expect("mX: a buffer");
        let snow_track_uniform = uniform_buffer(device, "snow track field place", 32);
        // coronas: camera group + a corona texture group
        let vertex_layout = mx::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: mx::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRIBUTES,
        };
        let bias: i32 = omsi_cfg::flags::OMSI_SURFACE_BIAS.parse().unwrap_or(-24);
        SceneBase {
            shader,
            lm_atlas,
            lm_uniform,
            snow_track,
            snow_track_uniform,
            vertex_layout,
            bias,
            trees: if detailed_trees && !GL_BACKEND.load(std::sync::atomic::Ordering::Relaxed) {
                &[("DETAILED_TREES", 1.0)]
            } else {
                &[]
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn pipeline(
        &self,
        device: &mx::Device,
        format: mx::TextureFormat,
        fs: &str,
        blend: Option<mx::BlendState>,
        depth_write: bool,
        cull: bool,
        bias: i32,
        alpha_to_coverage: bool,
        terrain_paint: bool,
        samples: u32,
        surfaces: bool,
    ) -> mx::RenderPipeline {
        let use_alpha_to_coverage = alpha_to_coverage && samples > 1;
        let constants: Vec<(&str, f64)> = [
            ("ALPHA_TEST", if alpha_to_coverage { 1.0 } else { 0.0 }),
            (
                "ALPHA_TO_COVERAGE",
                if use_alpha_to_coverage { 1.0 } else { 0.0 },
            ),
            ("TERRAIN_PAINT", if terrain_paint { 1.0 } else { 0.0 }),
        ]
        .into_iter()
        .chain(self.trees.iter().copied())
        .collect();
        compile_pipeline(
            device,
            &mx::RenderPipelineDescriptor {
                label: Some("omsi"),
                vertex: mx::VertexState {
                    constants: self.trees,
                    module: &self.shader,
                    entry_point: Some("vs_main"),
                    buffers: &[self.vertex_layout.clone()],
                },
                primitive: one_sided_primitive(cull),
                // Reversed Z: the near plane is 1 and the far plane 0, so nearer means
                // greater. The depth bias keeps its meaning (negative = towards the viewer)
                // only if its sign is turned around with the axis.
                depth_stencil: Some(mx::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: depth_write,
                    depth_compare: mx::CompareFunction::GreaterEqual,
                    stencil: Default::default(),
                    bias: mx::DepthBiasState {
                        constant: -bias,
                        slope_scale: if bias != 0 {
                            -bias.signum() as f32 * 2.0
                        } else {
                            0.0
                        },
                        clamp: 0.0,
                    },
                }),
                multisample: mx::MultisampleState {
                    count: samples,
                    mask: !0,
                    alpha_to_coverage_enabled: use_alpha_to_coverage,
                },
                fragment: Some(mx::FragmentState {
                    module: &self.shader,
                    entry_point: Some(fs),
                    targets: &color_targets_for(
                        format,
                        blend,
                        if fs == "fs_surface_depth" {
                            mx::ColorWrites::empty()
                        } else {
                            mx::ColorWrites::ALL
                        },
                        fs != "fs_surface_depth",
                        fs == "fs_enhanced",
                        surfaces,
                    ),
                    // Only cutouts and painted terrain keep their discard; opaque and blended
                    // materials retain early depth testing. (A blend that writes depth writes it
                    // where it is invisible too: Omsi.exe draws `[matl_alpha] 2` with the alpha
                    // test off, and content hides things under invisible covers, #2059.)
                    constants: &constants,
                }),
            },
        )
    }

    pub(crate) const PIPELINES: usize = PIPE_KINDS as usize * 4;

    /// The ground albedo pass's pipelines (`fs_ground`): the terrain's opaque base (0 to 3:
    /// two-sided or culled, without or with the surface bias) and its paint layers blended
    /// over it (4 to 7). Depth is only tested, against the depth prepass's.
    pub(crate) fn ground_pipeline_at(
        &self,
        device: &mx::Device,
        index: usize,
    ) -> mx::RenderPipeline {
        let paint = index >= 4;
        let (cull, surface) = (index & 2 != 0, index & 1 != 0);
        let bias = if surface { self.bias } else { 0 };
        let blend = paint.then_some(mx::BlendState {
            color: mx::BlendComponent {
                src_factor: mx::BlendFactor::SrcAlpha,
                dst_factor: mx::BlendFactor::OneMinusSrcAlpha,
                operation: mx::BlendOperation::Add,
            },
            // (the alpha channel stays as the base wrote it)
            alpha: mx::BlendComponent {
                src_factor: mx::BlendFactor::Zero,
                dst_factor: mx::BlendFactor::One,
                operation: mx::BlendOperation::Add,
            },
        });
        compile_pipeline(
            device,
            &mx::RenderPipelineDescriptor {
                label: Some("ground albedo"),
                vertex: mx::VertexState {
                    constants: &[],
                    module: &self.shader,
                    entry_point: Some("vs_main"),
                    buffers: &[self.vertex_layout.clone()],
                },
                primitive: one_sided_primitive(cull),
                depth_stencil: Some(mx::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: false,
                    depth_compare: mx::CompareFunction::GreaterEqual,
                    stencil: Default::default(),
                    bias: mx::DepthBiasState {
                        constant: -bias,
                        slope_scale: if bias != 0 {
                            -bias.signum() as f32 * 2.0
                        } else {
                            0.0
                        },
                        clamp: 0.0,
                    },
                }),
                multisample: mx::MultisampleState::default(),
                fragment: Some(mx::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs_ground"),
                    targets: &[Some(mx::ColorTargetState {
                        format: GROUND_FORMAT,
                        blend,
                        write_mask: mx::ColorWrites::ALL,
                    })],
                    constants: &[("TERRAIN_PAINT", if paint { 1.0 } else { 0.0 })],
                }),
            },
        )
    }

    /// The pipeline at `index` of the ones per `pipe_code` (kind, then culled, then surface
    /// bias): the kind decides blending and the depth write.
    pub(crate) fn pipeline_at_for(
        &self,
        device: &mx::Device,
        f: mx::TextureFormat,
        fs: &str,
        samples: u32,
        index: usize,
        surfaces: bool,
    ) -> mx::RenderPipeline {
        let kind = (index / 4) as u8;
        let (cull, surface) = (index & 2 != 0, index & 1 != 0);
        let blend = matches!(kind, PIPE_BLEND | PIPE_BLEND_NO_WRITE | PIPE_TERRAIN_PAINT)
            .then_some(mx::BlendState::ALPHA_BLENDING);
        let depth_write = kind != PIPE_BLEND_NO_WRITE && kind != PIPE_TERRAIN_PAINT;
        self.pipeline(
            device,
            f,
            if kind == PIPE_SURFACE_DEPTH {
                "fs_surface_depth"
            } else {
                fs
            },
            blend,
            depth_write,
            cull,
            if surface { self.bias } else { 0 },
            kind == PIPE_ALPHA_TEST,
            kind == PIPE_TERRAIN_PAINT && fs.starts_with("fs_enhanced"),
            samples,
            surfaces,
        )
    }
}

/// The depth-only pass's pipelines and depth prefilling inside the main pass.
pub(crate) struct Prepass {
    pub pipelines: [mx::RenderPipeline; 6],
    /// (drawn in the enhanced main pass, see `encode_prefill`)
    pub msaa_pipelines: Option<[mx::RenderPipeline; 6]>,
    pub eye_pipelines: Option<[mx::RenderPipeline; 6]>,
    pub pane_mask: Option<PaneMaskPipelines>,
}

pub(crate) const PANE_MASK_TEXTURES: u32 = 17;

pub(crate) struct PaneMaskPipelines {
    pub panes: [mx::RenderPipeline; 4],
    pub prefill: [mx::RenderPipeline; 2],
}

pub(crate) fn prepass(
    device: &mx::Device,
    scene: &SceneBase,
    msaa: u32,
    enhanced: bool,
) -> Prepass {
    let make_prepass_samples = prepass_builder(device, scene);
    let make_prepass = |kind: u8, cull: bool| make_prepass_samples(kind, cull, 1, false, false);
    let pipelines = [
        make_prepass(0, false),
        make_prepass(0, true),
        make_prepass(1, false),
        make_prepass(1, true),
        make_prepass(2, false),
        make_prepass(2, true),
    ];
    // Apple views with cutout/blended draws also need these pipelines: their
    // visibility depends on shading, unlike opaque hidden-surface removal.
    // Purely opaque Apple views still skip the prefill to avoid its cost.
    // (Only these: the prefill is drawn inside the main pass, with its colour targets.)
    // Excavations need a depth refill even at 1x, after their floors and covers.
    // The depth-only pass's pipelines cannot bind the main pass's colour targets.
    let msaa_pipelines = (msaa > 1 || enhanced).then(|| {
        [
            (0, false),
            (0, true),
            (1, false),
            (1, true),
            (2, false),
            (2, true),
        ]
        .map(|(kind, cull)| make_prepass_samples(kind, cull, msaa, true, rt_gbuf()))
    });
    let eye_pipelines = None;
    let pane_mask =
        (msaa > 1 && enhanced && MULTISAMPLED_SHADING.load(std::sync::atomic::Ordering::Relaxed))
            .then(|| pane_mask(device, scene, msaa));
    Prepass {
        pipelines,
        msaa_pipelines,
        eye_pipelines,
        pane_mask,
    }
}

fn pane_mask(device: &mx::Device, scene: &SceneBase, msaa: u32) -> PaneMaskPipelines {
    let panes = [
        (false, 0),
        (true, 0),
        (false, scene.bias),
        (true, scene.bias),
    ]
    .map(|(cull, bias)| depth_pipeline(device, scene, "fs_depth", cull, msaa, false, false, bias));
    let prefill = [false, true].map(|cull| {
        depth_pipeline(
            device,
            scene,
            "fs_depth_masked",
            cull,
            msaa,
            true,
            rt_gbuf(),
            0,
        )
    });
    PaneMaskPipelines { panes, prefill }
}

pub(crate) fn eye_prepass(
    device: &mx::Device,
    scene: &SceneBase,
    msaa: u32,
) -> [mx::RenderPipeline; 6] {
    let make = prepass_builder(device, scene);
    [
        (0, false),
        (0, true),
        (1, false),
        (1, true),
        (2, false),
        (2, true),
    ]
    .map(|(kind, cull)| make(kind, cull, msaa, true, false))
}

fn prepass_builder<'a>(
    device: &'a mx::Device,
    scene: &'a SceneBase,
) -> impl Fn(u8, bool, u32, bool, bool) -> mx::RenderPipeline + 'a {
    // the prepass culls exactly as the main pass does: a back face that wrote depth
    // here would hide what the main pass then draws behind it
    move |kind: u8, cull: bool, samples: u32, in_main: bool, surfaces: bool| {
        let fragment = match kind {
            0 => "fs_depth",
            // (the multisampled prefill in the main pass: only what alpha-to-coverage will
            // cover in full, see `fs_depth_cover`)
            1 if in_main && samples > 1 => "fs_depth_cover",
            1 => "fs_depth_test",
            2 => "fs_depth_transmap",
            _ => unreachable!(),
        };
        depth_pipeline(device, scene, fragment, cull, samples, in_main, surfaces, 0)
    }
}

#[allow(clippy::too_many_arguments)]
fn depth_pipeline(
    device: &mx::Device,
    scene: &SceneBase,
    fragment: &str,
    cull: bool,
    samples: u32,
    in_main: bool,
    surfaces: bool,
    bias: i32,
) -> mx::RenderPipeline {
    compile_pipeline(
        device,
        &mx::RenderPipelineDescriptor {
            label: Some("depth prepass"),
            vertex: mx::VertexState {
                constants: &[],
                module: &scene.shader,
                // (the place and the cut-out's coverage only: see `vs_depth`)
                entry_point: Some("vs_depth"),
                buffers: &[scene.vertex_layout.clone()],
            },
            primitive: one_sided_primitive(cull),
            depth_stencil: Some(mx::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: mx::CompareFunction::GreaterEqual,
                stencil: Default::default(),
                bias: mx::DepthBiasState {
                    constant: -bias,
                    slope_scale: if bias != 0 {
                        -bias.signum() as f32 * 2.0
                    } else {
                        0.0
                    },
                    clamp: 0.0,
                },
            }),
            multisample: mx::MultisampleState {
                count: samples,
                ..Default::default()
            },
            fragment: Some(mx::FragmentState {
                constants: scene.trees,
                module: &scene.shader,
                entry_point: Some(fragment),
                targets: &if in_main {
                    color_targets_for(
                        HDR_FORMAT,
                        None,
                        mx::ColorWrites::empty(),
                        false,
                        false,
                        surfaces,
                    )
                } else {
                    Vec::new()
                },
            }),
        },
    )
}
