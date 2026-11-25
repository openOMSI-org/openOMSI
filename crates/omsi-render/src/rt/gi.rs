//! Enhanced+ (experimental): ray-traced global illumination, the settings' `rt_gi` (see
//! `RenderOptions::rt_gi`). One diffuse bounce: after the depth prepass a ray per 4 x 4 pixels
//! leaves each surface in a cosine-weighted direction, new every frame, and brings back the
//! light of what it meets - lit by the sun (a shadow ray), the sky and the street lamps, or
//! its own light - and whether it saw the sky (rt_gi.wgsl). The rays are gathered over the
//! frames and the pixels round each (rt_gi_denoise.wgsl) and brought up to the full picture;
//! the main pass takes the result in place of its sky occlusion's guess at the light the
//! surroundings send back (04_shade.wgsl, `camera.clouds.w` 3).
//!
//! Without a history (a picture on its own) a pixel traces `GI_STILL_RAYS` rays at once.
//! Its passes have layouts of their own (the reflections' and their denoiser's modules hold
//! its entry points, but bind more than these read).

use super::*;

/// One GI ray per this many pixels each way (rt_gi.wgsl and rt_gi_denoise.wgsl `GI_DIV`).
const GI_DIV: u32 = 4;
/// How far a GI ray looks (m); beyond it the sky.
const GI_RANGE: f32 = 40.0;
/// How far from the camera the light is traced (m); the main pass's own beyond.
const GI_REACH: f32 = 160.0;
/// Rays a pixel when there is no history to gather them over.
const GI_STILL_RAYS: f32 = 16.0;

/// The traced global illumination's textures (all rgba16float).
pub(super) struct GiTargets {
    /// The rays' grid (one per 4 x 4 pixels) and the picture's size.
    half: (u32, u32),
    full_size: (u32, u32),
    /// This frame's rays (rgb the bounced irradiance, a the share of the sky light that
    /// reaches the surface) and the surfaces they left (x view depth, zw normal).
    raw: wgpu::TextureView,
    meta: wgpu::TextureView,
    /// The history and its surfaces (x view depth, y frames, zw normal), this frame's and last
    /// frame's by turns.
    hist: [wgpu::TextureView; 2],
    histm: [wgpu::TextureView; 2],
    /// The a-trous passes' two textures by turns (the last leaves the result in `b`).
    a: wgpu::TextureView,
    b: wgpu::TextureView,
    /// The result at the picture's size: what the main pass reads (camera binding 22).
    pub(super) full: wgpu::TextureView,
    /// Last frame's picture at the rays' size for their hits (rt_gi.wgsl `cs_gi_capture`).
    seen: wgpu::TextureView,
    /// When it was last traced (its history is good the frame after), when the picture was
    /// last taken for it, and the capture's bind group (by the picture it reads).
    pub(super) last: u64,
    pub(super) captured: u64,
    capture_group: Option<((u64, wgpu::TextureView), wgpu::BindGroup)>,
}

/// The GI's layouts and pipelines.
pub(super) struct GiPipes {
    trace_layout: wgpu::BindGroupLayout,
    denoise_layout: wgpu::BindGroupLayout,
    trace: wgpu::ComputePipeline,
    temporal: wgpu::ComputePipeline,
    atrous: wgpu::ComputePipeline,
    upsample: wgpu::ComputePipeline,
    capture_layout: wgpu::BindGroupLayout,
    capture: wgpu::ComputePipeline,
    /// (a storage buffer for the lamps' bindings where the lamps are no buffer)
    no_lamps: wgpu::Buffer,
}

/// What the kept GI bind groups were made with: the textures, the lamps' buffers.
type GiBindKey = (RtBindKey, Option<wgpu::Buffer>, Option<wgpu::Buffer>);

/// The GI's kept bind groups: per parity the rays, the temporal pass, the four a-trous
/// passes and the upsampling.
pub(super) type GiGroups = (GiBindKey, [[wgpu::BindGroup; 7]; 2]);

/// The GI's parameters (`RtParams::gi`): on, whether its history may be used, and whether
/// last frame's picture is there for its rays (`GiTargets::seen`).
pub(super) fn gi_params(on: bool, history: bool, seen: bool) -> [f32; 4] {
    [
        GI_RANGE,
        GI_STILL_RAYS,
        GI_REACH,
        match (on, history, seen) {
            (false, _, _) => 0.0,
            (true, false, _) => 1.0,
            (true, true, false) => 2.0,
            (true, true, true) => 3.0,
        },
    ]
}

impl GiPipes {
    /// Made when the GI is first asked for (an error scope round it: a driver that refuses
    /// them leaves the GI off and the rest of the ray tracing as it is).
    pub(super) fn new(device: &wgpu::Device) -> Result<GiPipes, String> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let params = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Params>() as u64),
            },
            count: None,
        };
        let unfilt = wgpu::TextureSampleType::Float { filterable: false };
        let filt = wgpu::TextureSampleType::Float { filterable: true };
        let trace_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rt gi"),
            entries: &[
                params,
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::AccelerationStructure {
                        vertex_return: false,
                    },
                    count: None,
                },
                texture_entry(2, wgpu::TextureSampleType::Depth),
                storage_buffer(3, true, false, 48),
                storage_buffer(4, true, false, 256),
                storage_texture(7),
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 11,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage_buffer(13, true, false, 16),
                texture_entry(14, filt),
                texture_entry(12, unfilt),
                storage_texture(15),
                storage_buffer(17, true, false, 64),
                storage_buffer(18, true, false, 4),
            ],
        });
        let denoise_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rt gi denoiser"),
            entries: &[
                params,
                texture_entry(2, wgpu::TextureSampleType::Depth),
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
        let capture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rt gi capture"),
            entries: &[
                params,
                texture_entry(2, wgpu::TextureSampleType::Depth),
                storage_texture(7),
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture_entry(9, filt),
                wgpu::BindGroupLayoutEntry {
                    binding: 11,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let div = [("REFL_DIV", refl_div() as f64)];
        let pipes = |layout: &wgpu::BindGroupLayout, source: String, entries: &[&str]| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("rt gi"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("rt gi"),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            entries
                .iter()
                .map(|entry| {
                    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some(entry),
                        layout: Some(&pl),
                        module: &module,
                        entry_point: Some(entry),
                        compilation_options: wgpu::PipelineCompilationOptions {
                            constants: &div,
                            ..Default::default()
                        },
                        cache: crate::pipeline_cache::get(device).as_ref(),
                    })
                })
                .collect::<Vec<_>>()
        };
        let mut trace = pipes(&trace_layout, reflect_source(), &["cs_gi"]);
        let mut rest = pipes(
            &denoise_layout,
            denoise_source(),
            &["cs_gi_temporal", "cs_gi_atrous", "cs_gi_upsample"],
        );
        let mut capture = pipes(&capture_layout, reflect_source(), &["cs_gi_capture"]);
        let no_lamps = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rt gi no lamps"),
            size: 64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(crate::gpu_error_text(&e));
        }
        let upsample = rest.pop().expect("three");
        let atrous = rest.pop().expect("three");
        let temporal = rest.pop().expect("three");
        Ok(GiPipes {
            trace_layout,
            denoise_layout,
            trace: trace.pop().expect("one"),
            temporal,
            atrous,
            upsample,
            capture_layout,
            capture: capture.pop().expect("one"),
            no_lamps,
        })
    }
}

impl RayTracer {
    /// The GI's textures for this picture size; true when they were made anew (the camera
    /// bind group must then name the new result).
    pub(crate) fn ensure_gi(&mut self, device: &wgpu::Device, w: u32, h: u32) -> bool {
        if self.gi_pipes.is_none() {
            if self.gi_failed {
                return false;
            }
            if !crate::gi_binding() {
                log::warn!(
                    "ray-traced global illumination unavailable: the device takes 16 textures a shader stage"
                );
                self.gi_failed = true;
                return false;
            }
            match GiPipes::new(device) {
                Ok(p) => self.gi_pipes = Some(p),
                Err(e) => {
                    log::warn!("ray-traced global illumination unavailable: {e}");
                    self.gi_failed = true;
                    return false;
                }
            }
        }
        let half = (w.div_ceil(GI_DIV), h.div_ceil(GI_DIV));
        if self
            .gi
            .as_ref()
            .is_some_and(|g| g.half == half && g.full_size == (w, h))
        {
            return false;
        }
        let mk = |label: &str, (w, h): (u32, u32)| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: w,
                        height: h,
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
        self.gi = Some(GiTargets {
            half,
            full_size: (w, h),
            raw: mk("rt gi rays", half),
            meta: mk("rt gi surfaces", half),
            hist: [mk("rt gi history", half), mk("rt gi history", half)],
            histm: [
                mk("rt gi history surfaces", half),
                mk("rt gi history surfaces", half),
            ],
            a: mk("rt gi filter", half),
            b: mk("rt gi filter", half),
            full: mk("rt gi", (w, h)),
            seen: mk("rt gi last picture", half),
            last: 0,
            captured: 0,
            capture_group: None,
        });
        self.gi_groups = None;
        true
    }

    /// The GI's result for the main pass, while it is traced.
    pub(crate) fn gi_view(&self) -> Option<&wgpu::TextureView> {
        self.gi.as_ref().map(|g| &g.full)
    }
}

impl Renderer {
    /// Trace this frame's GI into `GiTargets::full`, after the traced lighting (the depth
    /// prepass's surfaces) and before the main pass.
    pub(super) fn encode_rt_gi(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
        queries: Option<&wgpu::QuerySet>,
        timed: &mut Vec<&'static str>,
    ) {
        let (Some(rt), Some(ao)) = (self.rt.as_mut(), self.targets.ao.as_ref()) else {
            return;
        };
        if !rt.gi_on {
            return;
        }
        let (Some(g), Some(pipes)) = (rt.gi.as_mut(), rt.gi_pipes.as_ref()) else {
            return;
        };
        g.last = rt.frame;
        let g = &*g;
        let depth = &ao.depth_view;
        let lamps = |a: &Option<crate::gpu::GpuArray>| match a {
            Some(crate::gpu::GpuArray::Buffer(b)) => Some(b.clone()),
            _ => None,
        };
        let (lights, grid) = (lamps(&scene.light_buf), lamps(&scene.grid_buf));
        let key: GiBindKey = (
            (
                rt.bind_gen,
                vec![
                    depth.clone(),
                    g.raw.clone(),
                    g.hist[0].clone(),
                    g.full.clone(),
                ],
            ),
            lights.clone(),
            grid.clone(),
        );
        if rt.gi_groups.as_ref().is_none_or(|k| k.0 != key) {
            let (lb, gb) = match (lights.as_ref(), grid.as_ref()) {
                (Some(l), Some(gr)) => (l, gr),
                _ => (&pipes.no_lamps, &pipes.no_lamps),
            };
            let trace = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rt gi"),
                layout: &pipes.trace_layout,
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
                        resource: wgpu::BindingResource::TextureView(depth),
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
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(&g.raw),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: wgpu::BindingResource::Sampler(&self.tone.lin_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 11,
                        resource: self.tone.enh_buf.as_entire_binding(),
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
                        binding: 12,
                        resource: wgpu::BindingResource::TextureView(&g.seen),
                    },
                    wgpu::BindGroupEntry {
                        binding: 15,
                        resource: wgpu::BindingResource::TextureView(&g.meta),
                    },
                    wgpu::BindGroupEntry {
                        binding: 17,
                        resource: lb.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 18,
                        resource: gb.as_entire_binding(),
                    },
                ],
            });
            let dgroup = |meta: &wgpu::TextureView,
                          cur: &wgpu::TextureView,
                          prev: &wgpu::TextureView,
                          prev_m: &wgpu::TextureView,
                          out: &wgpu::TextureView,
                          out_m: &wgpu::TextureView| {
                let view = |binding, v| wgpu::BindGroupEntry {
                    binding,
                    resource: wgpu::BindingResource::TextureView(v),
                };
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("rt gi denoiser"),
                    layout: &pipes.denoise_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: rt.params.as_entire_binding(),
                        },
                        view(2, depth),
                        view(5, meta),
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
            let sp = &rt.spare[2];
            let per = |p: usize| {
                let (h, hm, phm) = (&g.hist[p], &g.histm[p], &g.histm[1 - p]);
                [
                    trace.clone(),
                    dgroup(&g.meta, &g.raw, &g.hist[1 - p], phm, h, hm),
                    dgroup(hm, h, &g.raw, phm, &g.a, sp),
                    dgroup(hm, &g.a, &g.raw, phm, &g.b, sp),
                    dgroup(hm, &g.b, &g.raw, phm, &g.a, sp),
                    dgroup(hm, &g.a, &g.raw, phm, &g.b, sp),
                    dgroup(hm, &g.b, &g.raw, phm, &g.full, sp),
                ]
            };
            rt.gi_groups = Some((key, [per(0), per(1)]));
        }
        let groups = &rt.gi_groups.as_ref().expect("made above").1[(rt.frame % 2) as usize];
        let (hx, hy) = (g.half.0.div_ceil(8), g.half.1.div_ceil(8));
        let full = g.full_size;
        let stride = rt.step_stride as u32;
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rt gi"),
                timestamp_writes: pass_timer(queries, timed, "rt gi").map(|t| {
                    wgpu::ComputePassTimestampWrites {
                        query_set: t.query_set,
                        beginning_of_pass_write_index: t.beginning_of_pass_write_index,
                        end_of_pass_write_index: t.end_of_pass_write_index,
                    }
                }),
            });
            pass.set_pipeline(&pipes.trace);
            pass.set_bind_group(0, &groups[0], &[]);
            pass.dispatch_workgroups(hx, hy, 1);
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rt gi denoiser"),
            timestamp_writes: pass_timer(queries, timed, "rt gi denoise").map(|t| {
                wgpu::ComputePassTimestampWrites {
                    query_set: t.query_set,
                    beginning_of_pass_write_index: t.beginning_of_pass_write_index,
                    end_of_pass_write_index: t.end_of_pass_write_index,
                }
            }),
        });
        pass.set_pipeline(&pipes.temporal);
        pass.set_bind_group(0, &groups[1], &[0]);
        pass.dispatch_workgroups(hx, hy, 1);
        pass.set_pipeline(&pipes.atrous);
        for (bg, offset) in [
            (&groups[2], 0),
            (&groups[3], stride),
            (&groups[4], 2 * stride),
            (&groups[5], 3 * stride),
        ] {
            pass.set_bind_group(0, bg, &[offset]);
            pass.dispatch_workgroups(hx, hy, 1);
        }
        pass.set_pipeline(&pipes.upsample);
        pass.set_bind_group(0, &groups[6], &[0]);
        pass.dispatch_workgroups(full.0.div_ceil(8), full.1.div_ceil(8), 1);
    }

    /// After the main pass: the picture at the rays' size for the next frame's GI rays
    /// (rt_gi.wgsl `cs_gi_capture`).
    pub(crate) fn encode_rt_gi_capture(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        w: u32,
        h: u32,
        queries: Option<&wgpu::QuerySet>,
        timed: &mut Vec<&'static str>,
    ) {
        let (Some(rt), Some(ao), Some(hdr)) = (
            self.rt.as_mut(),
            self.targets.ao.as_ref(),
            self.targets.hdr_targets.get(&(w, h)),
        ) else {
            return;
        };
        if !rt.gi_on {
            return;
        }
        let (Some(g), Some(pipes)) = (rt.gi.as_mut(), rt.gi_pipes.as_ref()) else {
            return;
        };
        if g.full_size != (w, h) {
            return;
        }
        let key = (rt.bind_gen, hdr.view.clone());
        if g.capture_group.as_ref().is_none_or(|c| c.0 != key) {
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rt gi capture"),
                layout: &pipes.capture_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: rt.params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&ao.depth_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(&g.seen),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: wgpu::BindingResource::Sampler(&self.tone.lin_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 9,
                        resource: wgpu::BindingResource::TextureView(&hdr.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 11,
                        resource: self.tone.enh_buf.as_entire_binding(),
                    },
                ],
            });
            g.capture_group = Some((key, group));
        }
        g.captured = rt.frame;
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rt gi capture"),
            timestamp_writes: pass_timer(queries, timed, "rt gi capture").map(|t| {
                wgpu::ComputePassTimestampWrites {
                    query_set: t.query_set,
                    beginning_of_pass_write_index: t.beginning_of_pass_write_index,
                    end_of_pass_write_index: t.end_of_pass_write_index,
                }
            }),
        });
        pass.set_pipeline(&pipes.capture);
        pass.set_bind_group(0, &g.capture_group.as_ref().expect("made above").1, &[]);
        pass.dispatch_workgroups(g.half.0.div_ceil(8), g.half.1.div_ceil(8), 1);
    }
}
