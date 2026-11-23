//! Mip maps made on the GPU.

use super::common::*;

/// How a texture's smaller levels average the bigger (see `mip.wgsl`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MipAverage {
    /// Colour weighted by alpha (colour alone where all alpha is 0), for the content's
    /// textures: dark transparent texels stay out of the edges of trees and fences.
    ByCoverage,
    /// Colour and alpha apart, as OMSI does for text and script textures.
    Independent,
}

pub(crate) struct Mip {
    pub pipeline: mx::RenderPipeline,
    pub text_pipeline: mx::RenderPipeline,
    pub sampler: mx::Sampler,
}

pub(crate) fn build(device: &mx::Device) -> Mip {
    // --- mipmaps on the GPU: the CPU box average took up to a second per bus spawn
    log::info!("renderer: compiling the mip maps shaders");
    let mip_shader = compile_shader(
        device,
        mx::ShaderModuleDescriptor {
            label: Some("mip"),
            source: &*(crate::shaders::hot_wgsl(include_str!("../mip.wgsl"), "mip.wgsl")),
        },
    );
    let mip_pipeline = no_vertex_pipeline(
        "mip",
        &mip_shader,
        "fs_main",
        &[target(mx::TextureFormat::Rgba8UnormSrgb, None)],
    )
    .create(device);
    let text_pipeline = no_vertex_pipeline(
        "mip text",
        &mip_shader,
        "fs_text",
        &[target(mx::TextureFormat::Rgba8UnormSrgb, None)],
    )
    .create(device);
    let mip_sampler = clamped_linear_sampler(device);
    Mip {
        pipeline: mip_pipeline,
        text_pipeline,
        sampler: mip_sampler,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The source texels under destination texel `i` along one axis and their weights: the
    /// share of the destination texel's span each of them covers.
    fn footprint(i: u32, s: u32, d: u32) -> Vec<(u32, f32)> {
        let (lo, hi) = (
            i as f64 * s as f64 / d as f64,
            (i + 1) as f64 * s as f64 / d as f64,
        );
        (0..s)
            .filter_map(|j| {
                let overlap = hi.min(j as f64 + 1.0) - lo.max(j as f64);
                (overlap > 1e-9).then(|| (j, (overlap / (hi - lo)) as f32))
            })
            .collect()
    }

    /// `share` of `mip.wgsl` over the texels `mix_under` visits, on the CPU.
    fn shader_shares(i: u32, s: u32, d: u32) -> Vec<(u32, f32)> {
        let (first, last) = (i * s / d, (((i + 1) * s - 1) / d).min(s - 1));
        (first..=last)
            .map(|j| {
                let (from, to) = ((i * s).max(j * d), ((i + 1) * s).min((j + 1) * d));
                (
                    j,
                    if to > from {
                        (to - from) as f32 / s as f32
                    } else {
                        0.0
                    },
                )
            })
            .filter(|t| t.1 > 0.0)
            .collect()
    }

    fn close(a: &[(u32, f32)], b: &[(u32, f32)]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(x, y)| x.0 == y.0 && (x.1 - y.1).abs() < 1e-6)
    }

    #[test]
    fn footprints_cover_exactly_the_area_of_the_bigger_level() {
        assert!(close(&footprint(3, 16, 8), &[(6, 0.5), (7, 0.5)]));
        // 15 -> 7: a bit more than two texels, the outer one with a partial weight
        assert!(close(
            &footprint(0, 15, 7),
            &[(0, 7.0 / 15.0), (1, 7.0 / 15.0), (2, 1.0 / 15.0)]
        ));
        assert!(close(
            &footprint(3, 15, 7),
            &[(6, 4.0 / 15.0), (7, 7.0 / 15.0), (8, 4.0 / 15.0)]
        ));
        // the last row of the bigger level still counts
        assert_eq!(footprint(6, 15, 7).last().unwrap().0, 14);
        assert!(close(
            &footprint(0, 3, 1),
            &[(0, 1.0 / 3.0), (1, 1.0 / 3.0), (2, 1.0 / 3.0)]
        ));
        assert!(close(&footprint(0, 1, 1), &[(0, 1.0)]));
        for s in 1..70u32 {
            let d = (s / 2).max(1);
            let mut per_texel = vec![0.0f32; s as usize];
            for i in 0..d {
                let f = footprint(i, s, d);
                assert!(
                    (f.iter().map(|t| t.1).sum::<f32>() - 1.0).abs() < 1e-5,
                    "{s}->{d} texel {i}"
                );
                for (j, w) in &f {
                    per_texel[*j as usize] += w;
                }
                // the shader's whole-number shares give the same texels and weights, at most
                // four of them
                let shares = shader_shares(i, s, d);
                assert!(
                    shares.len() <= 4 && close(&shares, &f),
                    "{s}->{d} texel {i}: {shares:?} vs {f:?}"
                );
            }
            // every texel of the bigger level weighs the same in the smaller one
            for w in per_texel {
                assert!((w - d as f32 / s as f32).abs() < 1e-5, "{s}->{d}");
            }
        }
    }

    fn to_linear(c: u8) -> f32 {
        let c = c as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    fn to_srgb(l: f32) -> f32 {
        let c = if l <= 0.0031308 {
            l * 12.92
        } else {
            1.055 * l.powf(1.0 / 2.4) - 0.055
        };
        c * 255.0
    }

    /// The smaller level as `mip.wgsl` should make it, on the CPU (sRGB colour, linear alpha).
    fn reference(src: &[u8], (w, h): (u32, u32), average: MipAverage) -> Vec<[f32; 4]> {
        let (dw, dh) = ((w / 2).max(1), (h / 2).max(1));
        let mut out = Vec::new();
        for y in 0..dh {
            for x in 0..dw {
                let (mut weighted, mut plain, mut alpha) = ([0.0f32; 3], [0.0f32; 3], 0.0f32);
                for (sy, wy) in footprint(y, h, dh) {
                    for (sx, wx) in footprint(x, w, dw) {
                        let p = &src[((sy * w + sx) * 4) as usize..][..4];
                        let a = p[3] as f32 / 255.0;
                        for c in 0..3 {
                            weighted[c] += to_linear(p[c]) * a * wx * wy;
                            plain[c] += to_linear(p[c]) * wx * wy;
                        }
                        alpha += a * wx * wy;
                    }
                }
                let rgb = if average == MipAverage::ByCoverage && alpha > 2.5e-6 {
                    weighted.map(|c| c / alpha)
                } else {
                    plain
                };
                out.push([
                    to_srgb(rgb[0]),
                    to_srgb(rgb[1]),
                    to_srgb(rgb[2]),
                    alpha * 255.0,
                ]);
            }
        }
        out
    }

    /// A 15x15 display digit (white, its edge fading out in alpha, on transparent black)
    /// goes down to 7x7 on the GPU as the CPU reference says, for both filters: the text
    /// average keeps colour and alpha apart, the other weights colour by alpha.
    #[test]
    fn odd_text_textures_go_down_by_their_exact_footprint() {
        let instance = mx::Instance::new();
        let Ok(adapter) = instance.default_adapter() else {
            eprintln!("no graphics adapter: footprint render check skipped");
            return;
        };
        let (device, queue) = adapter
            .request_device(&mx::DeviceDescriptor::default())
            .unwrap();
        let mip = build(&device);
        let (w, h) = (15u32, 15u32);
        let mut src = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                // the digit "0": a ring 3 texels wide with a softer outer edge, and its
                // bottom row lit to the last row of the texture
                let ring = (x <= 2 || x >= 12 || y <= 2 || y >= 12) && x >= 1 && x <= 13;
                let a = if !ring {
                    0
                } else if x == 1 || x == 13 || y == 0 {
                    96
                } else {
                    255
                };
                if a > 0 {
                    src[((y * w + x) * 4) as usize..][..4].copy_from_slice(&[255, 255, 255, a]);
                }
            }
        }
        for average in [MipAverage::Independent, MipAverage::ByCoverage] {
            let texture = device
                .create_texture(&mx::TextureDescriptor {
                    label: None,
                    size: mx::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 2,
                    sample_count: 1,
                    dimension: mx::TextureDimension::D2,
                    format: mx::TextureFormat::Rgba8UnormSrgb,
                    usage: mx::TextureUsages::TEXTURE_BINDING
                        | mx::TextureUsages::COPY_DST
                        | mx::TextureUsages::COPY_SRC
                        | mx::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .expect("mX: a texture");
            queue
                .write_texture(
                    mx::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: mx::Origin3d::ZERO,
                        aspect: mx::TextureAspect::All,
                    },
                    &src,
                    mx::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(w * 4),
                        rows_per_image: Some(h),
                    },
                    mx::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                )
                .expect("mX: a queue write");
            let view = |level| {
                texture
                    .create_view(&mx::TextureViewDescriptor {
                        base_mip_level: level,
                        mip_level_count: Some(1),
                        ..Default::default()
                    })
                    .expect("mX: a texture view")
            };
            let (src_view, dst_view) = (view(0), view(1));
            let bg = device.create_bind_group(&mx::BindGroupDescriptor {
                label: None,
                entries: &[
                    mx::BindGroupEntry {
                        binding: 0,
                        resource: mx::BindingResource::TextureView(&src_view),
                    },
                    mx::BindGroupEntry {
                        binding: 1,
                        resource: mx::BindingResource::Sampler(&mip.sampler),
                    },
                ],
            });
            let (dw, dh) = (w / 2, h / 2);
            let buf = device
                .create_buffer(&mx::BufferDescriptor {
                    label: None,
                    size: (256 * dh) as u64,
                    usage: mx::BufferUsages::COPY_DST | mx::BufferUsages::MAP_READ,
                })
                .expect("mX: a buffer");
            let mut enc = device.create_command_encoder(None);
            {
                let mut pass = enc.begin_render_pass(&mx::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(mx::RenderPassColorAttachment {
                        view: &dst_view,
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
                pass.set_pipeline(if average == MipAverage::Independent {
                    &mip.text_pipeline
                } else {
                    &mip.pipeline
                });
                pass.set_bind_group(0, &bg);
                pass.draw(0..3, 0..1);
            }
            enc.copy_texture_to_buffer(
                mx::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 1,
                    origin: mx::Origin3d::ZERO,
                    aspect: mx::TextureAspect::All,
                },
                mx::TexelCopyBufferInfo {
                    buffer: &buf,
                    layout: mx::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(dh),
                    },
                },
                mx::Extent3d {
                    width: dw,
                    height: dh,
                    depth_or_array_layers: 1,
                },
            );
            let copied = queue.submit([enc.finish()]);
            device.wait(copied).unwrap();
            let got = buf.map_read_now(0..buf.size(), |b| b.to_vec()).unwrap();
            let want = reference(&src, (w, h), average);
            for y in 0..dh {
                for x in 0..dw {
                    let g = &got[(y * 256 + x * 4) as usize..][..4];
                    let e = want[(y * dw + x) as usize];
                    for c in 0..4 {
                        assert!(
                            (g[c] as f32 - e[c]).abs() <= 2.0,
                            "{average:?} texel {x},{y} channel {c}: {} vs {:.1}",
                            g[c],
                            e[c]
                        );
                    }
                }
            }
            if average == MipAverage::Independent {
                // the bottom row of the digit is still in the last row of the smaller level
                assert!(got[(6 * 256 + 3 * 4 + 3) as usize] > 60);
                // and colour is averaged on its own: a half-covered texel is grey, not white
                let corner = &got[(3 * 256) as usize..][..4];
                assert!(corner[0] < 250, "{corner:?}");
            }
        }
    }
}
