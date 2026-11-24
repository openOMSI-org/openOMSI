//! The sky dome and its clouds.

use super::common::*;
use crate::*;

const SKY_ATTRIBUTES: [mx::VertexAttribute; 1] = mx::vertex_attr_array![0 => Float32x3];

pub(crate) struct SkyBase {
    pub cloud_shape_view: mx::TextureView,
    pub cloud_detail_view: mx::TextureView,
    /// The clouds' base volume and curl map: read by the passes that march the clouds (the
    /// sky cube and the screen's clouds, cloud_pass.rs), not by the dome.
    pub cloud_base_view: mx::TextureView,
    pub cloud_curl_view: mx::TextureView,
    pub cloud_sampler: mx::Sampler,
    pub cloud_shape_cpu: crate::lighting::CloudCpu,
    /// Where the screen's clouds were traced from (`CloudView`, binding 12).
    pub cloud_view_buf: mx::Buffer,
    /// The classic sky's weather (`VanillaSkyUniform`, binding 10).
    pub vanilla_buf: mx::Buffer,
    pub shader: mx::ShaderModule,
    /// the module's places in the heap (`groups_to_heap`): the dome's, the probe's and the
    /// clouds' groups are made at them
    pub layout: crate::shaders::HeapLayout,
}

impl SkyBase {
    /// With no enhanced pass, use the vanilla sky module and placeholder noise textures.
    /// This includes ANGLE's texture-unit fallback, not only the launcher's preview.
    pub(crate) fn new(device: &mx::Device, queue: &mx::Queue, enhanced: bool) -> SkyBase {
        // sky dome
        let clouds = cloud_noise_textures(device, queue, enhanced);
        log::info!("renderer: compiling the sky and clouds shaders");
        let sky_shader = compile_shader(
            device,
            mx::ShaderModuleDescriptor {
                label: Some("sky"),
                source: &crate::shaders::groups_to_heap(&shader_source(enhanced)),
            },
        );
        let vanilla_buf = uniform_buffer(
            device,
            "vanilla sky",
            std::mem::size_of::<VanillaSkyUniform>() as u64,
        );
        let cloud_view_buf = uniform_buffer(
            device,
            "cloud view",
            std::mem::size_of::<crate::cloud_pass::CloudView>() as u64,
        );
        SkyBase {
            cloud_shape_view: clouds.weather,
            cloud_detail_view: clouds.detail,
            cloud_base_view: clouds.base,
            cloud_curl_view: clouds.curl,
            cloud_sampler: clouds.sampler,
            cloud_shape_cpu: clouds.cpu,
            cloud_view_buf,
            vanilla_buf,
            shader: sky_shader,
            layout: crate::shaders::layout_of(&shader_source(enhanced)),
        }
    }

    pub(crate) fn pipeline_base(&self) -> SkyPipelineBase {
        SkyPipelineBase {
            shader: self.shader.clone(),
        }
    }
}

/// Only shader/layout handles are retained for deferred headset pipelines.
#[derive(Clone)]
pub(crate) struct SkyPipelineBase {
    shader: mx::ShaderModule,
}

impl SkyPipelineBase {
    pub(crate) fn pipeline(
        &self,
        device: &mx::Device,
        f: mx::TextureFormat,
        fs: &str,
        msaa: u32,
    ) -> mx::RenderPipeline {
        self.pipeline_for(device, f, fs, msaa, rt_gbuf())
    }

    pub(crate) fn pipeline_for(
        &self,
        device: &mx::Device,
        f: mx::TextureFormat,
        fs: &str,
        msaa: u32,
        surfaces: bool,
    ) -> mx::RenderPipeline {
        let sky_vertex = mx::VertexBufferLayout {
            array_stride: 12,
            step_mode: mx::VertexStepMode::Vertex,
            attributes: &SKY_ATTRIBUTES,
        };
        compile_pipeline(
            device,
            &mx::RenderPipelineDescriptor {
                label: Some("sky"),
                vertex: mx::VertexState {
                    constants: &[],
                    module: &self.shader,
                    entry_point: Some("vs_main"),
                    buffers: &[sky_vertex],
                },
                primitive: mx::PrimitiveState {
                    topology: mx::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    front_face: mx::FrontFace::Ccw,
                    ..Default::default()
                },
                // (the dome lies on the far plane: over a cleared depth it covers everything,
                // over laid depth only what is still empty)
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
                        None,
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

pub(crate) fn shader_source(enhanced: bool) -> String {
    if enhanced {
        sky_shader_source()
    } else {
        [
            crate::shaders::hot_wgsl(include_str!("../colour.wgsl"), "colour.wgsl"),
            crate::shaders::hot_wgsl(include_str!("../sky.wgsl"), "sky.wgsl"),
        ]
        .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::shader_source;
    use naga;

    /// The fallback sky must actually omit the raymarch/probe entry points and remain
    /// valid on GLES, not merely refrain from drawing its enhanced pipelines.
    #[test]
    fn fallback_sky_translates_without_enhanced_entry_points() {
        let source = shader_source(false);
        let module = naga::front::wgsl::parse_str(&crate::shaders::with_heap(&source))
            .expect("vanilla sky WGSL");
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
pub(crate) fn dome(
    device: &mx::Device,
    queue: &mx::Queue,
) -> (mx::Sampler, (mx::Buffer, mx::Buffer, u32)) {
    let sky_sampler = device
        .create_sampler(&mx::SamplerDescriptor {
            address_mode_u: mx::AddressMode::Repeat,
            address_mode_v: mx::AddressMode::ClampToEdge,
            mag_filter: mx::FilterMode::Linear,
            min_filter: mx::FilterMode::Linear,
            ..Default::default()
        })
        .expect("mX: a sampler");
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
    let sky_vb = buffer_init(
        device,
        queue,
        Some("sky vb"),
        bytemuck::cast_slice(&sv),
        mx::BufferUsages::VERTEX,
    );
    let sky_ib = buffer_init(
        device,
        queue,
        Some("sky ib"),
        bytemuck::cast_slice(&si),
        mx::BufferUsages::INDEX,
    );
    (sky_sampler, (sky_vb, sky_ib, si.len() as u32))
}
