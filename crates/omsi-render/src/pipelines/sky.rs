//! The sky dome and its clouds.

use super::common::*;
use crate::*;

const SKY_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];

pub(crate) struct SkyBase {
    pub layout: wgpu::BindGroupLayout,
    pub cloud_shape_view: wgpu::TextureView,
    pub cloud_detail_view: wgpu::TextureView,
    pub cloud_sampler: wgpu::Sampler,
    pub cloud_shape_cpu: Vec<u8>,
    /// The classic sky's weather (`VanillaSkyUniform`, binding 10).
    pub vanilla_buf: wgpu::Buffer,
    pub shader: wgpu::ShaderModule,
    pl: wgpu::PipelineLayout,
}

impl SkyBase {
    /// With no enhanced pass, use the vanilla sky module and placeholder noise textures.
    /// This includes ANGLE's texture-unit fallback, not only the launcher's preview.
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, camera_layout: &wgpu::BindGroupLayout, enhanced: bool) -> SkyBase {
        // sky dome
        let float_2d = wgpu::TextureSampleType::Float { filterable: true };
        let sky_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky"),
            entries: &[
                float_texture_entry(0),
                float_texture_entry(1),
                float_texture_entry(2),
                sampler_entry(3),
                float_texture_entry(4),
                sampler_entry(5),
                float_texture_entry(6),
                texture_entry(7, float_2d, wgpu::TextureViewDimension::D3),
                sampler_entry(8),
                float_texture_entry(9),
                uniform_entry(10, wgpu::ShaderStages::FRAGMENT),
                float_texture_entry(11),
            ],
        });
        let (cloud_shape_view, cloud_detail_view, cloud_sampler, cloud_shape_cpu) = cloud_noise_textures(device, queue, enhanced);
        log::info!("renderer: compiling the sky and clouds shaders");
        let sky_shader = compile_shader(device, wgpu::ShaderModuleDescriptor {
            label: Some("sky"),
            source: wgpu::ShaderSource::Wgsl(shader_source(enhanced).into()),
        });
        let sky_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky"),
            bind_group_layouts: &[Some(camera_layout), Some(&sky_layout)],
            immediate_size: 0,
        });
        let vanilla_buf = uniform_buffer(device, "vanilla sky", std::mem::size_of::<VanillaSkyUniform>() as u64);
        SkyBase { layout: sky_layout, cloud_shape_view, cloud_detail_view, cloud_sampler, cloud_shape_cpu, vanilla_buf, shader: sky_shader, pl: sky_pl }
    }

    pub(crate) fn pipeline_base(&self) -> SkyPipelineBase {
        SkyPipelineBase { shader: self.shader.clone(), pl: self.pl.clone() }
    }
}

/// Only shader/layout handles are retained for deferred headset pipelines.
#[derive(Clone)]
pub(crate) struct SkyPipelineBase {
    shader: wgpu::ShaderModule,
    pl: wgpu::PipelineLayout,
}

impl SkyPipelineBase {
    pub(crate) fn pipeline(&self, device: &wgpu::Device, f: wgpu::TextureFormat, fs: &str, msaa: u32) -> wgpu::RenderPipeline {
        self.pipeline_for(device, f, fs, msaa, rt_gbuf())
    }

    pub(crate) fn pipeline_for(
        &self,
        device: &wgpu::Device,
        f: wgpu::TextureFormat,
        fs: &str,
        msaa: u32,
        surfaces: bool,
    ) -> wgpu::RenderPipeline {
        let sky_vertex = wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &SKY_ATTRIBUTES,
        };
        compile_pipeline(device, &wgpu::RenderPipelineDescriptor {
            label: Some("sky"),
            layout: Some(&self.pl),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs_main"),
                buffers: &[sky_vertex],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            // (the dome lies on the far plane: over a cleared depth it covers everything,
            // over laid depth only what is still empty)
            depth_stencil: Some(depth_test(wgpu::CompareFunction::GreaterEqual)),
            multisample: wgpu::MultisampleState {
                count: msaa,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
                entry_point: Some(fs),
                targets: &color_targets_for(f, None, wgpu::ColorWrites::COLOR, false, false,
                        surfaces,
                    ),
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: crate::pipeline_cache::get(device).as_ref(),
        })
    }
}

fn shader_source(enhanced: bool) -> String {
    if enhanced {
        sky_shader_source()
    } else {
        [include_str!("../colour.wgsl"), include_str!("../sky.wgsl")].join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::shader_source;
    use wgpu::naga;

    /// The fallback sky must actually omit the raymarch/probe entry points and remain
    /// valid on GLES, not merely refrain from drawing its enhanced pipelines.
    #[test]
    fn fallback_sky_translates_without_enhanced_entry_points() {
        let source = shader_source(false);
        let module = naga::front::wgsl::parse_str(&source).expect("vanilla sky WGSL");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("vanilla sky validation");
        let names: Vec<_> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, ["vs_main", "fs_main"]);
        assert!(shader_source(true).contains("fn fs_sky_cube"));
        for version in [300, 310] {
            let options = naga::back::glsl::Options {
                version: naga::back::glsl::Version::Embedded {
                    version,
                    is_webgl: false,
                },
                ..Default::default()
            };
            for entry in &module.entry_points {
                let pipeline = naga::back::glsl::PipelineOptions {
                    shader_stage: entry.stage,
                    entry_point: entry.name.clone(),
                    multiview: None,
                };
                let mut output = String::new();
                naga::back::glsl::Writer::new(
                    &mut output,
                    &module,
                    &info,
                    &options,
                    &pipeline,
                    Default::default(),
                )
                .and_then(|mut writer| writer.write())
                .expect("vanilla sky GLSL ES");
            }
        }
    }
}

/// The sky's sampler and the dome's vertex and index buffers (and index count).
pub(crate) fn dome(device: &wgpu::Device, queue: &wgpu::Queue) -> (wgpu::Sampler, (wgpu::Buffer, wgpu::Buffer, u32)) {
    let sky_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    // dome: latitude rings from a little below the horizon to the zenith
    let (mut sv, mut si): (Vec<[f32; 3]>, Vec<u32>) = (Vec::new(), Vec::new());
    let (rings, segs) = (12u32, 32u32);
    for r in 0..=rings {
        let elev = -0.15 + (std::f32::consts::FRAC_PI_2 + 0.15) * r as f32 / rings as f32;
        for sgm in 0..=segs {
            let az = sgm as f32 / segs as f32 * std::f32::consts::TAU;
            sv.push([elev.cos() * az.sin(), elev.cos() * az.cos(), elev.sin()]);
        }
    }
    for r in 0..rings {
        for sgm in 0..segs {
            let a = r * (segs + 1) + sgm;
            let b = a + segs + 1;
            si.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    let sky_vb = buffer_init(device, queue, Some("sky vb"), bytemuck::cast_slice(&sv), wgpu::BufferUsages::VERTEX);
    let sky_ib = buffer_init(device, queue, Some("sky ib"), bytemuck::cast_slice(&si), wgpu::BufferUsages::INDEX);
    (sky_sampler, (sky_vb, sky_ib, si.len() as u32))
}
