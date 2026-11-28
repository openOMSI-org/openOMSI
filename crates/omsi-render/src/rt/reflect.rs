//! The ray-traced reflections' own passes (see `rt.rs`): their encoding after the main
//! pass, the denoiser's pipelines and its a-trous steps, and the texture means the records
//! read. (Out of `rt.rs`, which outgrew its size budget.)

use super::*;

/// The reflections' denoiser: its bind group layout and its temporal and a-trous passes.
pub(super) fn denoise_pipelines(
    device: &wgpu::Device,
    div: &[(&str, f64)],
) -> (
    wgpu::BindGroupLayout,
    wgpu::ComputePipeline,
    wgpu::ComputePipeline,
) {
    let unfilt = wgpu::TextureSampleType::Float { filterable: false };
    let denoise_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("rt reflections denoiser"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Params>() as u64),
                },
                count: None,
            },
            texture_entry(5, unfilt),
            texture_entry(6, unfilt),
            texture_entry(7, unfilt),
            texture_entry(8, unfilt),
            storage_texture(9),
            storage_texture(10),
            wgpu::BindGroupLayoutEntry {
                binding: 11,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            },
        ],
    });
    let denoise_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("rt reflections denoiser"),
        source: wgpu::ShaderSource::Wgsl(denoise_source().into()),
    });
    let denoise_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("rt reflections denoiser"),
        bind_group_layouts: &[Some(&denoise_layout)],
        immediate_size: 0,
    });
    let denoise_pass = |entry: &str| {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(entry),
            layout: Some(&denoise_pl),
            module: &denoise_module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: div,
                ..Default::default()
            },
            cache: crate::pipeline_cache::get(device).as_ref(),
        })
    };
    let temporal_refl = denoise_pass("cs_temporal");
    let atrous = denoise_pass("cs_atrous");
    (denoise_layout, temporal_refl, atrous)
}

/// The pass that takes each texture's mean colour for the records (`encode_texture_means`).
pub(super) fn tex_mean_pipeline(
    device: &wgpu::Device,
) -> (wgpu::BindGroupLayout, wgpu::ComputePipeline) {
    let atlas_entries = rt_tex::TexStore::avg_entries();
    let avg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("texture means"),
        entries: &[
            texture_entry(0, wgpu::TextureSampleType::Float { filterable: true }),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            storage_buffer(2, false, true, 32),
            atlas_entries[0],
            atlas_entries[1],
        ],
    });
    let avg_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("texture means"),
        source: wgpu::ShaderSource::Wgsl(
            crate::shaders::hot_wgsl(include_str!("../rt_avg.wgsl"), "rt_avg.wgsl").into(),
        ),
    });
    let avg_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("texture means"),
        bind_group_layouts: &[Some(&avg_layout)],
        immediate_size: 0,
    });
    let avg_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("texture means"),
        layout: Some(&avg_pl),
        module: &avg_module,
        entry_point: Some("cs_tex_avg"),
        compilation_options: Default::default(),
        cache: crate::pipeline_cache::get(device).as_ref(),
    });
    (avg_layout, avg_pipeline)
}

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
        encoder: &mut wgpu::CommandEncoder,
        w: u32,
        h: u32,
        queries: Option<&wgpu::QuerySet>,
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
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some(label),
                        size: wgpu::Extent3d {
                            width: half.0,
                            height: half.1,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba16Float,
                        usage: wgpu::TextureUsages::STORAGE_BINDING
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
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
        let group = |out: &wgpu::TextureView,
                     scene: &wgpu::TextureView,
                     h12: &wgpu::TextureView,
                     meta_out: &wgpu::TextureView,
                     meta_in: &wgpu::TextureView| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ray-traced reflections"),
                layout: &rt.refl_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: rt.params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: rt.tlas.as_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&ao.depth_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: rt.records_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: rt.tex_avg.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&gbuf[0].1),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&gbuf[1].1),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(out),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: wgpu::BindingResource::Sampler(&self.tone.lin_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 9,
                        resource: wgpu::BindingResource::TextureView(scene),
                    },
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: wgpu::BindingResource::TextureView(&probe.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 11,
                        resource: self.tone.enh_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 12,
                        resource: wgpu::BindingResource::TextureView(h12),
                    },
                    wgpu::BindGroupEntry {
                        binding: 13,
                        resource: rt.tex.uvs.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 14,
                        resource: wgpu::BindingResource::TextureView(&rt.tex.atlas_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 15,
                        resource: wgpu::BindingResource::TextureView(meta_out),
                    },
                    wgpu::BindGroupEntry {
                        binding: 16,
                        resource: wgpu::BindingResource::TextureView(meta_in),
                    },
                ],
            })
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
        // (last frame's history in, this frame's out), then the three a-trous ones.
        let dgroup = |cur: &wgpu::TextureView,
                      prev: &wgpu::TextureView,
                      prev_m: &wgpu::TextureView,
                      out: &wgpu::TextureView,
                      out_m: &wgpu::TextureView| {
            let view = |binding, v| wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::TextureView(v),
            };
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rt reflections denoiser"),
                layout: &rt.denoise_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: rt.params.as_entire_binding(),
                    },
                    view(5, &rtg.meta),
                    view(6, cur),
                    view(7, prev),
                    view(8, prev_m),
                    view(9, out),
                    view(10, out_m),
                    wgpu::BindGroupEntry {
                        binding: 11,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &rt.steps,
                            offset: 0,
                            size: wgpu::BufferSize::new(16),
                        }),
                    },
                ],
            })
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
                    dgroup(raw, &h[1], &m[1], &h[0], &m[0]),
                    dgroup(raw, &h[0], &m[0], &h[1], &m[1]),
                    dgroup(&h[0], raw, &m[0], &rtg.a, sp),
                    dgroup(&h[1], raw, &m[1], &rtg.a, sp),
                    dgroup(&rtg.a, raw, &m[0], &rtg.b, sp),
                    dgroup(&rtg.b, raw, &m[0], &rtg.a, sp),
                ],
            ));
        }
        let dg = &rt.denoise_groups.as_ref().expect("made above").1;
        let parity = (rt.frame % 2) as usize;
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rt reflections"),
                timestamp_writes: pass_timer(queries, timed, "rt reflections").map(|t| {
                    wgpu::ComputePassTimestampWrites {
                        query_set: t.query_set,
                        beginning_of_pass_write_index: t.beginning_of_pass_write_index,
                        end_of_pass_write_index: t.end_of_pass_write_index,
                    }
                }),
            });
            let (gx, gy) = (half.0.div_ceil(8), half.1.div_ceil(8));
            pass.set_pipeline(&rt.reflect);
            pass.set_bind_group(0, trace_bg, &[]);
            pass.dispatch_workgroups(gx, gy, 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rt reflections denoiser"),
                timestamp_writes: pass_timer(queries, timed, "rt reflections denoise").map(|t| {
                    wgpu::ComputePassTimestampWrites {
                        query_set: t.query_set,
                        beginning_of_pass_write_index: t.beginning_of_pass_write_index,
                        end_of_pass_write_index: t.end_of_pass_write_index,
                    }
                }),
            });
            let (gx, gy) = (half.0.div_ceil(8), half.1.div_ceil(8));
            let stride = rt.step_stride as u32;
            pass.set_pipeline(&rt.temporal_refl);
            pass.set_bind_group(0, &dg[parity], &[0]);
            pass.dispatch_workgroups(gx, gy, 1);
            pass.set_pipeline(&rt.atrous);
            for (bg, offset) in [(&dg[2 + parity], 0), (&dg[4], stride), (&dg[5], 2 * stride)] {
                pass.set_bind_group(0, bg, &[offset]);
                pass.dispatch_workgroups(gx, gy, 1);
            }
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rt reflections composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &hdr.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: pass_timer(queries, timed, "rt composite"),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&rt.composite);
        pass.set_bind_group(0, composite_bg, &[]);
        pass.draw(0..3, 0..1);
    }
}
