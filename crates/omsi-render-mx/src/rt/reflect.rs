//! The ray-traced reflections' own pass (see `rt.rs`): its encoding after the main pass and
//! its a-trous steps. (Out of `rt.rs`, which outgrew its size budget.)

use super::*;

/// The reflections' a-trous passes' steps, 1, 2 and 4 rays apart, each at its own uniform
/// offset `stride` (the first takes the variance from the moments; the temporal pass reads
/// the first and ignores it); and 8 for the traced global illumination's fourth pass.
pub(super) fn refl_steps(stride: u64) -> Vec<u8> {
    let mut steps = vec![0u8; 4 * stride as usize];
    for (k, step) in [
        [1.0f32, 1.0, 0.0, 0.0],
        [2.0, 0.0, 0.0, 0.0],
        [4.0, 0.0, 0.0, 0.0],
        [8.0, 0.0, 0.0, 0.0],
    ]
    .iter()
    .enumerate()
    {
        let at = k * stride as usize;
        steps[at..at + 16].copy_from_slice(bytemuck::cast_slice(step));
    }
    steps
}

impl Renderer {
    /// The ray-traced reflections of the window's picture, added to `HdrTargets::view` after
    /// the main pass (and before the rain on the glass, which looks through it).
    pub(crate) fn encode_rt_reflections(
        &mut self,
        encoder: &mut mx::CommandEncoder,
        w: u32,
        h: u32,
        queries: Option<&mx::QuerySet>,
        timed: &mut Vec<&'static str>,
    ) {
        let (Some(rt), Some(hdr), Some(ao), Some(probe)) = (
            self.rt.as_mut(),
            self.targets.hdr_targets.get(&(w, h)),
            self.targets.ao.as_ref(),
            self.tone.probe.as_ref(),
        ) else {
            return;
        };
        let Some(gbuf) = hdr.gbuf.as_ref() else {
            return;
        };
        let half = (w.div_ceil(refl_div()), h.div_ceil(refl_div()));
        if rt.refl.as_ref().is_none_or(|r| r.0 != half) {
            let mk = |label: &str| {
                self.device
                    .create_texture(&mx::TextureDescriptor {
                        label: Some(label),
                        size: mx::Extent3d {
                            width: half.0,
                            height: half.1,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: mx::TextureDimension::D2,
                        format: mx::TextureFormat::Rgba16Float,
                        usage: mx::TextureUsages::STORAGE_BINDING
                            | mx::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .expect("mX: a texture")
                    .create_view(&Default::default())
                    .expect("mX: a texture view")
            };
            rt.refl = Some((
                half,
                ReflTargets {
                    raw: mk("rt reflection rays"),
                    meta: mk("rt reflection surfaces"),
                    hist: [mk("rt reflection history"), mk("rt reflection history")],
                    moments: [mk("rt reflection moments"), mk("rt reflection moments")],
                    a: mk("rt reflection filter"),
                    b: mk("rt reflection filter"),
                },
            ));
        }
        let (_, rtg) = rt.refl.as_ref().unwrap();
        let raw = &rtg.raw;
        let dummy = &self.black_texture.view;
        let group = |out: &mx::TextureView,
                     scene: &mx::TextureView,
                     h12: &mx::TextureView,
                     meta_out: &mx::TextureView,
                     meta_in: &mx::TextureView| {
            HeapGroup::of(
                super::RT_REFLECT.layout(),
                0,
                &mx::BindGroupDescriptor {
                    label: Some("ray-traced reflections"),
                    entries: &[
                        mx::BindGroupEntry {
                            binding: 0,
                            resource: rt.params.as_entire_binding(),
                        },
                        mx::BindGroupEntry {
                            binding: 1,
                            resource: mx::BindingResource::AccelerationStructure(&rt.tlas),
                        },
                        mx::BindGroupEntry {
                            binding: 2,
                            resource: mx::BindingResource::TextureView(&ao.depth_view),
                        },
                        mx::BindGroupEntry {
                            binding: 3,
                            resource: rt.records_buf.as_entire_binding(),
                        },
                        mx::BindGroupEntry {
                            binding: 4,
                            resource: rt.tex_avg.as_entire_binding(),
                        },
                        mx::BindGroupEntry {
                            binding: 5,
                            resource: mx::BindingResource::TextureView(&gbuf[0].1),
                        },
                        mx::BindGroupEntry {
                            binding: 6,
                            resource: mx::BindingResource::TextureView(&gbuf[1].1),
                        },
                        mx::BindGroupEntry {
                            binding: 7,
                            resource: mx::BindingResource::TextureView(out),
                        },
                        mx::BindGroupEntry {
                            binding: 8,
                            resource: mx::BindingResource::Sampler(&self.tone.lin_sampler),
                        },
                        mx::BindGroupEntry {
                            binding: 9,
                            resource: mx::BindingResource::TextureView(scene),
                        },
                        mx::BindGroupEntry {
                            binding: 10,
                            resource: mx::BindingResource::TextureView(&probe.view),
                        },
                        mx::BindGroupEntry {
                            binding: 11,
                            resource: self.tone.enh_buf.as_entire_binding(),
                        },
                        mx::BindGroupEntry {
                            binding: 12,
                            resource: mx::BindingResource::TextureView(h12),
                        },
                        mx::BindGroupEntry {
                            binding: 13,
                            resource: rt.tex.uvs.as_entire_binding(),
                        },
                        mx::BindGroupEntry {
                            binding: 14,
                            resource: mx::BindingResource::TextureView(&rt.tex.atlas_view),
                        },
                        mx::BindGroupEntry {
                            binding: 15,
                            resource: mx::BindingResource::TextureView(meta_out),
                        },
                        mx::BindGroupEntry {
                            binding: 16,
                            resource: mx::BindingResource::TextureView(meta_in),
                        },
                    ],
                },
            )
        };
        // (no pass's output is among its inputs)
        let key: RtBindKey = (
            rt.bind_gen,
            vec![
                ao.depth_view.clone(),
                gbuf[0].1.clone(),
                gbuf[1].1.clone(),
                raw.clone(),
                rtg.meta.clone(),
                rtg.a.clone(),
                hdr.view.clone(),
                probe.view.clone(),
                dummy.clone(),
            ],
        );
        if rt.refl_groups.as_ref().is_none_or(|g| g.0 != key) {
            rt.refl_groups = Some((
                key,
                [
                    group(raw, &hdr.view, dummy, &rtg.meta, dummy),
                    group(&rt.spare[0], dummy, &rtg.a, &rt.spare[1], &rtg.meta),
                ],
            ));
        }
        let [trace_bg, composite_bg] = &rt.refl_groups.as_ref().expect("made above").1;
        // The denoiser's passes (rt_denoise.wgsl): the temporal one by this frame's parity
        // (last frame's history in, this frame's out), then the three a-trous ones, each with
        // its step's uniform.
        let dgroup = |cur: &mx::TextureView,
                      prev: &mx::TextureView,
                      prev_m: &mx::TextureView,
                      out: &mx::TextureView,
                      out_m: &mx::TextureView,
                      step: u64| {
            let view = |binding, v| mx::BindGroupEntry {
                binding,
                resource: mx::BindingResource::TextureView(v),
            };
            HeapGroup::of(
                super::RT_DENOISE.layout(),
                0,
                &mx::BindGroupDescriptor {
                    label: Some("rt reflections denoiser"),
                    entries: &[
                        mx::BindGroupEntry {
                            binding: 0,
                            resource: rt.params.as_entire_binding(),
                        },
                        view(5, &rtg.meta),
                        view(6, cur),
                        view(7, prev),
                        view(8, prev_m),
                        view(9, out),
                        view(10, out_m),
                        mx::BindGroupEntry {
                            binding: 11,
                            resource: mx::BindingResource::Buffer(mx::BufferBinding {
                                buffer: &rt.steps,
                                offset: step * rt.step_stride,
                                size: Some(16),
                            }),
                        },
                    ],
                },
            )
        };
        let dkey: RtBindKey = (
            rt.bind_gen,
            vec![
                raw.clone(),
                rtg.meta.clone(),
                rtg.hist[0].clone(),
                rtg.moments[0].clone(),
                rtg.a.clone(),
            ],
        );
        if rt.denoise_groups.as_ref().is_none_or(|g| g.0 != dkey) {
            let (h, m, sp) = (&rtg.hist, &rtg.moments, &rt.spare[2]);
            rt.denoise_groups = Some((
                dkey,
                [
                    dgroup(raw, &h[1], &m[1], &h[0], &m[0], 0),
                    dgroup(raw, &h[0], &m[0], &h[1], &m[1], 0),
                    dgroup(&h[0], raw, &m[0], &rtg.a, sp, 0),
                    dgroup(&h[1], raw, &m[1], &rtg.a, sp, 0),
                    dgroup(&rtg.a, raw, &m[0], &rtg.b, sp, 1),
                    dgroup(&rtg.b, raw, &m[0], &rtg.a, sp, 2),
                    dgroup(&rtg.a, raw, &m[1], &rtg.b, sp, 1),
                    dgroup(&rtg.b, raw, &m[1], &rtg.a, sp, 2),
                ],
            ));
        }
        let dg = &rt.denoise_groups.as_ref().expect("made above").1;
        let parity = (rt.frame % 2) as usize;
        {
            let mut pass = encoder.begin_compute_pass(&mx::ComputePassDescriptor {
                label: Some("rt reflections"),
                timestamp_writes: pass_timer(queries, timed, "rt reflections").map(|t| {
                    mx::PassTimestampWrites {
                        query_set: t.query_set,
                        beginning_of_pass_write_index: t.beginning_of_pass_write_index,
                        end_of_pass_write_index: t.end_of_pass_write_index,
                    }
                }),
            });
            let (gx, gy) = (half.0.div_ceil(8), half.1.div_ceil(8));
            pass.set_pipeline(&rt.reflect);
            trace_bg.set_compute(&mut pass);
            pass.dispatch_workgroups(gx, gy, 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&mx::ComputePassDescriptor {
                label: Some("rt reflections denoiser"),
                timestamp_writes: pass_timer(queries, timed, "rt reflections denoise").map(|t| {
                    mx::PassTimestampWrites {
                        query_set: t.query_set,
                        beginning_of_pass_write_index: t.beginning_of_pass_write_index,
                        end_of_pass_write_index: t.end_of_pass_write_index,
                    }
                }),
            });
            let (gx, gy) = (half.0.div_ceil(8), half.1.div_ceil(8));
            pass.set_pipeline(&rt.temporal_refl);
            dg[parity].set_compute(&mut pass);
            pass.dispatch_workgroups(gx, gy, 1);
            pass.set_pipeline(&rt.atrous);
            for bg in [&dg[2 + parity], &dg[4 + 2 * parity], &dg[5 + 2 * parity]] {
                bg.set_compute(&mut pass);
                pass.dispatch_workgroups(gx, gy, 1);
            }
        }
        let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
            label: Some("rt reflections composite"),
            color_attachments: &[Some(mx::RenderPassColorAttachment {
                view: &hdr.view,
                resolve_target: None,
                ops: mx::Operations {
                    load: mx::LoadOp::Load,
                    store: mx::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: pass_timer(queries, timed, "rt composite"),
            occlusion_query_set: None,
        });
        pass.set_pipeline(&rt.composite);
        composite_bg.set(&mut pass);
        pass.draw(0..3, 0..1);
    }
}
