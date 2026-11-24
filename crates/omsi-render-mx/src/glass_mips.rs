//! The smaller levels of the picture behind the glass (`Targets::glass_picture`), for the
//! soft blur a wet pane lays over the street seen through it.
//!
//! A film of water on a window scatters what is behind it into a soft haze a degree or two
//! wide. The rain films read the clean picture of the frame (copied before they are drawn,
//! see `encode_rain_film`) at a mip level that grows with the water on the pane
//! (enhanced/07_glass.wgsl `glass_film`), so each level is made from the one above by a
//! small tent filter: four bilinear reads a texel apart, sixteen texels with tent weights.
//! Every picture's copy, the enhanced picture's HDR one and the classic pictures' own (their
//! wet panes blur the same way, scene/09_vanilla.wgsl `glass_film_classic`).

use crate::pipelines::common::*;

/// How many levels the copy keeps: the finest is the frame itself, the coarsest a sixteenth
/// of it - some 70 pixels a side at 1080p, more than a downpour's blur needs.
pub(crate) const GLASS_MIP_LEVELS: u32 = 5;

/// `SHADER`, its group read from the heap.
pub(crate) static GLASS_MIP: HeapModule = HeapModule::new(|| SHADER.to_string());

const SHADER: &str = r#"
@group(0) @binding(0) var t_src: texture_2d<f32>;
@group(0) @binding(1) var s_src: sampler;
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    var out: VsOut;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(t_src));
    var sum = vec4<f32>(0.0);
    for (var k = 0; k < 4; k = k + 1) {
        let o = vec2<f32>(f32(k & 1) * 2.0 - 1.0, f32(k >> 1u) * 2.0 - 1.0) * texel;
        sum = sum + textureSampleLevel(t_src, s_src, in.uv + o, 0.0);
    }
    // (a non-finite texel - a NaN off a broken surface - must not spread over the blur)
    let c = sum * 0.25;
    return select(vec4<f32>(0.0), c, (c == c) & (abs(c) < vec4<f32>(65000.0)));
}
"#;

/// The levels of one glass picture and what makes them.
pub(crate) struct GlassMips {
    pipeline: mx::RenderPipeline,
    /// For each level from the second on: where it is drawn, and the level above it to read.
    levels: Vec<(mx::TextureView, HeapGroup)>,
}

impl GlassMips {
    /// The levels of `texture` (made with `GLASS_MIP_LEVELS` levels or fewer, and
    /// `RENDER_ATTACHMENT`), drawn in its own format.
    pub(crate) fn new(device: &mx::Device, texture: &mx::Texture) -> GlassMips {
        let module = compile_shader(
            device,
            mx::ShaderModuleDescriptor {
                label: Some("glass mips"),
                source: &GLASS_MIP.wgsl(),
            },
        );
        let pipeline = no_vertex_pipeline(
            "glass mips",
            &module,
            "fs_main",
            &[target(texture.format(), None)],
        )
        .create(device);
        let sampler = clamped_linear_sampler(device);
        let level = |mip: u32| {
            texture
                .create_view(&mx::TextureViewDescriptor {
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
                .expect("mX: a texture view")
        };
        let levels = (1..texture.mip_level_count())
            .map(|mip| {
                let src = level(mip - 1);
                let group = HeapGroup::of(
                    GLASS_MIP.layout(),
                    0,
                    &mx::BindGroupDescriptor {
                        label: Some("glass mip"),
                        entries: &[
                            mx::BindGroupEntry {
                                binding: 0,
                                resource: mx::BindingResource::TextureView(&src),
                            },
                            mx::BindGroupEntry {
                                binding: 1,
                                resource: mx::BindingResource::Sampler(&sampler),
                            },
                        ],
                    },
                );
                (level(mip), group)
            })
            .collect();
        GlassMips { pipeline, levels }
    }

    /// Make every level below the first from the one above (after the frame was copied in).
    pub(crate) fn encode(&self, encoder: &mut mx::CommandEncoder) {
        for (dst, group) in &self.levels {
            let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
                label: Some("glass mip"),
                color_attachments: &[Some(mx::RenderPassColorAttachment {
                    view: dst,
                    resolve_target: None,
                    ops: mx::Operations {
                        load: mx::LoadOp::Clear(mx::Color::BLACK),
                        store: mx::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            group.set(&mut pass);
            pass.draw(0..3, 0..1);
        }
    }
}

/// The levels a glass picture of this size gets, in any format: the enhanced picture's HDR
/// copy and the classic pictures' own blur their wet panes alike.
pub(crate) fn glass_mip_levels(width: u32, height: u32, _format: mx::TextureFormat) -> u32 {
    let most = 32 - width.min(height).max(1).leading_zeros();
    GLASS_MIP_LEVELS.min(most)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The downsampling shader parses and validates (it is compiled only when it rains).
    #[test]
    fn glass_mip_shader_validates() {
        let module = naga::front::wgsl::parse_str(SHADER).expect("glass mip WGSL must parse");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("glass mip WGSL must validate");
    }

    /// Every copy gets its blur chain, the classic picture's as the enhanced one's, and
    /// small pictures as many levels as they have.
    #[test]
    fn every_copy_gets_its_levels() {
        assert_eq!(
            glass_mip_levels(1920, 1080, crate::HDR_FORMAT),
            GLASS_MIP_LEVELS
        );
        assert_eq!(glass_mip_levels(8, 6, crate::HDR_FORMAT), 3);
        assert_eq!(glass_mip_levels(1, 1, crate::HDR_FORMAT), 1);
        assert_eq!(
            glass_mip_levels(1920, 1080, mx::TextureFormat::Bgra8UnormSrgb),
            GLASS_MIP_LEVELS
        );
    }
}
