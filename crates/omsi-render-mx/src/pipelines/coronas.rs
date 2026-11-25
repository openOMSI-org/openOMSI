//! The lamps' coronas and the smoke puffs: sprites drawn over the scene; and the snowfall.

use super::common::*;
use super::scene::SceneBase;
use crate::*;

pub(crate) const ADDITIVE: mx::BlendState = mx::BlendState {
    color: mx::BlendComponent {
        src_factor: mx::BlendFactor::One,
        dst_factor: mx::BlendFactor::One,
        operation: mx::BlendOperation::Add,
    },
    alpha: mx::BlendComponent::REPLACE,
};
// Omsi's lamp sprites: SRCBLEND ONE, DESTBLEND INVSRCCOLOR (src + dst * (1 - src)),
// which keeps a coloured sprite's hue over a lit background instead of washing it to white
pub(crate) const SCREEN: mx::BlendState = mx::BlendState {
    color: mx::BlendComponent {
        src_factor: mx::BlendFactor::One,
        dst_factor: mx::BlendFactor::OneMinusSrc,
        operation: mx::BlendOperation::Add,
    },
    alpha: mx::BlendComponent::REPLACE,
};
pub(crate) const ALPHA_BLEND: mx::BlendState = mx::BlendState {
    color: mx::BlendComponent {
        src_factor: mx::BlendFactor::SrcAlpha,
        dst_factor: mx::BlendFactor::OneMinusSrcAlpha,
        operation: mx::BlendOperation::Add,
    },
    alpha: mx::BlendComponent::REPLACE,
};

const CORONA_ATTRIBUTES: [mx::VertexAttribute; 6] = mx::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4];

#[derive(Clone)]
pub(crate) struct Coronas {
    pub sampler: mx::Sampler,
    pub bind_group: HeapGroup,
    pub smoke_bind_group: HeapGroup,
    /// corona.wgsl's places in the heap (`groups_to_heap`)
    pub layout: crate::shaders::HeapLayout,
    shader: mx::ShaderModule,
}

impl Coronas {
    /// The coronas' and the smoke's bind groups and shader, and the corona sprite (its
    /// bind group holds it).
    pub(crate) fn new(
        device: &mx::Device,
        queue: &mx::Queue,
        _scene: &SceneBase,
    ) -> (Coronas, GpuTexture) {
        // corona sprite: soft radial falloff
        let cs = 64u32;
        let mut corona_img = omsi_texture::Image {
            width: cs,
            height: cs,
            rgba: vec![0; (cs * cs * 4) as usize],
            has_alpha: false,
        };
        for y in 0..cs {
            for x in 0..cs {
                let dx = (x as f32 + 0.5) / cs as f32 * 2.0 - 1.0;
                let dy = (y as f32 + 0.5) / cs as f32 * 2.0 - 1.0;
                let r = (dx * dx + dy * dy).sqrt();
                let v = ((1.0 - r).max(0.0)).powf(1.6) * 255.0;
                let o = ((y * cs + x) * 4) as usize;
                corona_img.rgba[o..o + 4].copy_from_slice(&[v as u8, v as u8, v as u8, 255]);
            }
        }
        let corona_texture = upload_texture(device, queue, &corona_img, false);
        let corona_sampler = clamped_linear_sampler(device);
        let layout = crate::shaders::layout_of(&corona_shader_source());
        let corona_bind_group = HeapGroup::of(
            &layout,
            1,
            &mx::BindGroupDescriptor {
                label: Some("corona"),
                entries: &[
                    mx::BindGroupEntry {
                        binding: 0,
                        resource: mx::BindingResource::TextureView(&corona_texture.view),
                    },
                    mx::BindGroupEntry {
                        binding: 1,
                        resource: mx::BindingResource::Sampler(&corona_sampler),
                    },
                ],
            },
        );
        // a soft grey puff until the app hands over the game's own `Texture/rauch.tga`
        let mut puff = omsi_texture::Image {
            width: cs,
            height: cs,
            rgba: vec![0; (cs * cs * 4) as usize],
            has_alpha: true,
        };
        for y in 0..cs {
            for x in 0..cs {
                let dx = (x as f32 + 0.5) / cs as f32 * 2.0 - 1.0;
                let dy = (y as f32 + 0.5) / cs as f32 * 2.0 - 1.0;
                let a = (1.0 - (dx * dx + dy * dy).sqrt()).max(0.0).powf(1.2) * 255.0;
                let o = ((y * cs + x) * 4) as usize;
                puff.rgba[o..o + 4].copy_from_slice(&[255, 255, 255, a as u8]);
            }
        }
        let puff_texture = upload_texture(device, queue, &puff, false);
        let smoke_bind_group = HeapGroup::of(
            &layout,
            1,
            &mx::BindGroupDescriptor {
                label: Some("smoke"),
                entries: &[
                    mx::BindGroupEntry {
                        binding: 0,
                        resource: mx::BindingResource::TextureView(&puff_texture.view),
                    },
                    mx::BindGroupEntry {
                        binding: 1,
                        resource: mx::BindingResource::Sampler(&corona_sampler),
                    },
                ],
            },
        );
        drop(puff_texture);
        log::info!("renderer: compiling the coronas shaders");
        let corona_shader = compile_shader(
            device,
            mx::ShaderModuleDescriptor {
                label: Some("corona"),
                source: &crate::shaders::groups_to_heap(&corona_shader_source()),
            },
        );
        let coronas = Coronas {
            sampler: corona_sampler,
            bind_group: corona_bind_group,
            smoke_bind_group,
            shader: corona_shader,
            layout,
        };
        (coronas, corona_texture)
    }

    pub(crate) fn pipeline_for(
        &self,
        device: &mx::Device,
        f: mx::TextureFormat,
        fs: &str,
        blend: mx::BlendState,
        msaa: u32,
        surfaces: bool,
    ) -> mx::RenderPipeline {
        let corona_vertex = mx::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuCorona>() as u64,
            step_mode: mx::VertexStepMode::Instance,
            attributes: &CORONA_ATTRIBUTES,
        };
        compile_pipeline(
            device,
            &mx::RenderPipelineDescriptor {
                label: Some("corona"),
                vertex: mx::VertexState {
                    constants: &[],
                    module: &self.shader,
                    entry_point: Some(if fs == "fs_enhanced" {
                        "vs_enhanced"
                    } else {
                        "vs_main"
                    }),
                    buffers: &[corona_vertex],
                },
                primitive: mx::PrimitiveState {
                    topology: mx::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    front_face: mx::FrontFace::Ccw,
                    ..Default::default()
                },
                depth_stencil: Some(depth_test(mx::CompareFunction::GreaterEqual)),
                multisample: mx::MultisampleState {
                    count: msaa,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                fragment: Some(mx::FragmentState {
                    constants: &[],
                    module: &self.shader,
                    entry_point: Some(fs),
                    targets: &color_targets_for(
                        f,
                        Some(blend),
                        mx::ColorWrites::COLOR,
                        false,
                        false,
                        surfaces,
                    ),
                }),
            },
        )
    }
}

/// The snowfall (snow.wgsl) or the rainfall (rain.wgsl): the camera group and its own
/// parameters, no vertices.
#[derive(Clone)]
pub(crate) struct Precip {
    shader: mx::ShaderModule,
    pub buf: mx::Buffer,
    pub bind_group: HeapGroup,
    /// its places in the heap (`groups_to_heap`)
    pub layout: crate::shaders::HeapLayout,
    label: &'static str,
    vs: &'static str,
    /// The enhanced picture's vertex shader (with its own fragment shaders: the flakes'
    /// and the drops' shapes, the splashes, the far veil).
    vs_enhanced: &'static str,
    /// Its quads are strips of four vertices (the rain's), not lists of six.
    strip: bool,
}

const PREMULTIPLIED_SNOW: mx::BlendState = mx::BlendState {
    color: mx::BlendComponent {
        src_factor: mx::BlendFactor::One,
        dst_factor: mx::BlendFactor::OneMinusSrcAlpha,
        operation: mx::BlendOperation::Add,
    },
    alpha: mx::BlendComponent::REPLACE,
};

impl Precip {
    pub(crate) fn snow(
        device: &mx::Device,
        roof: &mx::TextureView,
        flakes: &crate::precip::FlakeAtlas,
    ) -> Precip {
        Precip::new(
            device,
            roof,
            flakes,
            "snow",
            ["vs_snow", "vs_snow_enhanced"],
            snow_shader_source(),
            std::mem::size_of::<SnowUniform>(),
            false,
        )
    }

    pub(crate) fn rain(
        device: &mx::Device,
        roof: &mx::TextureView,
        flakes: &crate::precip::FlakeAtlas,
    ) -> Precip {
        Precip::new(
            device,
            roof,
            flakes,
            "rain",
            ["vs_rain", "vs_rain_enhanced"],
            rain_shader_source(),
            std::mem::size_of::<RainUniform>(),
            true,
        )
    }

    /// (`roof`: the precipitation's roof map, `passes::precip_map`; `flakes`: the enhanced
    /// snowfall's crystals, `precip::FlakeAtlas`, in the rain's group as well)
    #[allow(clippy::too_many_arguments)]
    fn new(
        device: &mx::Device,
        roof: &mx::TextureView,
        flakes: &crate::precip::FlakeAtlas,
        label: &'static str,
        vs: [&'static str; 2],
        source: String,
        size: usize,
        strip: bool,
    ) -> Precip {
        let shader = compile_shader(
            device,
            mx::ShaderModuleDescriptor {
                label: Some(label),
                source: &crate::shaders::groups_to_heap(&source),
            },
        );
        let layout = crate::shaders::layout_of(&source);
        let buf = uniform_buffer(device, label, size as u64);
        let bind_group = HeapGroup::of(
            &layout,
            1,
            &mx::BindGroupDescriptor {
                label: Some(label),
                entries: &[
                    mx::BindGroupEntry {
                        binding: 0,
                        resource: buf.as_entire_binding(),
                    },
                    mx::BindGroupEntry {
                        binding: 1,
                        resource: mx::BindingResource::TextureView(roof),
                    },
                    mx::BindGroupEntry {
                        binding: 2,
                        resource: mx::BindingResource::TextureView(&flakes.view),
                    },
                    mx::BindGroupEntry {
                        binding: 3,
                        resource: mx::BindingResource::Sampler(&flakes.sampler),
                    },
                ],
            },
        );
        Precip {
            shader,
            buf,
            bind_group,
            layout,
            label,
            vs: vs[0],
            vs_enhanced: vs[1],
            strip,
        }
    }

    pub(crate) fn pipeline_for(
        &self,
        device: &mx::Device,
        f: mx::TextureFormat,
        fs: &str,
        msaa: u32,
        surfaces: bool,
    ) -> mx::RenderPipeline {
        log::info!("renderer: compiling the {} shader ({fs})", self.label);
        let targets = color_targets_for(
            f,
            Some(PREMULTIPLIED_SNOW),
            mx::ColorWrites::COLOR,
            false,
            false,
            surfaces,
        );
        // (the enhanced picture's fragment shaders take the enhanced vertex shader's output)
        let vs = if fs.ends_with("_enhanced") {
            self.vs_enhanced
        } else {
            self.vs
        };
        no_vertex_pipeline(self.label, &self.shader, fs, &targets)
            .vs(vs)
            .strip(self.strip)
            .depth(depth_test(mx::CompareFunction::GreaterEqual))
            .samples(msaa)
            .create(device)
    }
}
