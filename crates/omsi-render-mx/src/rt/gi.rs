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

use super::*;

/// One GI ray per this many pixels each way (rt_gi.wgsl and rt_gi_denoise.wgsl `GI_DIV`).
const GI_DIV: u32 = 4;
/// How far a GI ray looks (m); beyond it the sky.
const GI_RANGE: f32 = 40.0;
/// How far from the camera the light is traced (m); the main pass's own beyond.
const GI_REACH: f32 = 160.0;
/// Rays a pixel when there is no history to gather them over.
const GI_STILL_RAYS: f32 = 16.0;
/// Frames a pixel's rays are gathered over at most.
pub(super) const GI_HISTORY: f32 = 24.0;

/// The traced global illumination's textures (all rgba16float).
pub(super) struct GiTargets {
    /// The rays' grid (one per 4 x 4 pixels).
    half: (u32, u32),
    /// This frame's rays (rgb the bounced irradiance, a the share of the sky light that
    /// reaches the surface) and the surfaces they left (x view depth, zw normal).
    raw: mx::TextureView,
    meta: mx::TextureView,
    /// The history and its surfaces (x view depth, y frames, zw normal), this frame's and last
    /// frame's by turns.
    hist: [mx::TextureView; 2],
    histm: [mx::TextureView; 2],
    /// The a-trous passes' two textures by turns (the last leaves the result in `b`).
    a: mx::TextureView,
    b: mx::TextureView,
    /// The result at the picture's size: what the main pass reads (`FrameHeap::gi`).
    pub(super) full: mx::TextureView,
    /// Last frame's picture at a quarter size each way for the rays' hits (rt_gi.wgsl `cs_gi_capture`).
    seen: mx::TextureView,
}

/// The pipelines of the traced global illumination.
pub(super) struct GiPipes {
    trace: mx::ComputePipeline,
    temporal: mx::ComputePipeline,
    atrous: mx::ComputePipeline,
    upsample: mx::ComputePipeline,
    capture: mx::ComputePipeline,
}

impl GiPipes {
    /// Its pipelines from the reflections' module (the rays) and their denoiser's (the
    /// rest), made when the GI is first asked for; an error (a driver that refuses them)
    /// leaves the GI off and the rest of the ray tracing as it is.
    pub(super) fn new(device: &mx::Device) -> Result<GiPipes, String> {
        let module = |label: &str, source: String| {
            device
                .create_shader_module(&mx::ShaderModuleDescriptor {
                    label: Some(label),
                    source: &source,
                })
                .map_err(|e| format!("{label}: {e}"))
        };
        let refl = module("ray-traced reflections", super::RT_REFLECT.wgsl())?;
        let denoise = module("rt reflections denoiser", super::RT_DENOISE.wgsl())?;
        let div = [("REFL_DIV", refl_div() as f64)];
        let make = |module: &mx::ShaderModule, entry: &str| {
            device
                .create_compute_pipeline(&mx::ComputePipelineDescriptor {
                    label: Some(entry),
                    module,
                    entry_point: Some(entry),
                    constants: &div,
                })
                .map_err(|e| format!("{entry}: {e}"))
        };
        Ok(GiPipes {
            trace: make(&refl, "cs_gi")?,
            temporal: make(&denoise, "cs_gi_temporal")?,
            atrous: make(&denoise, "cs_gi_atrous")?,
            upsample: make(&denoise, "cs_gi_upsample")?,
            capture: make(&refl, "cs_gi_capture")?,
        })
    }
}

/// What the kept GI bind groups were made with: the textures, the lamps' buffers.
type GiBindKey = (RtBindKey, Option<mx::Buffer>, Option<mx::Buffer>);

/// The GI's kept bind groups: per parity the rays, the temporal pass, the four a-trous
/// passes and the upsampling.
pub(super) type GiGroups = (GiBindKey, [[HeapGroup; 7]; 2]);

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

impl RayTracer {
    /// The GI's textures for this picture size; true when they were made anew (the camera's
    /// heap places must then name the new result).
    pub(crate) fn ensure_gi(&mut self, device: &mx::Device, w: u32, h: u32) -> bool {
        if self.gi_pipes.is_none() {
            if self.gi_failed {
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
                .create_texture(&mx::TextureDescriptor {
                    label: Some(label),
                    size: mx::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: mx::TextureDimension::D2,
                    format: mx::TextureFormat::Rgba16Float,
                    usage: mx::TextureUsages::STORAGE_BINDING | mx::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .expect("mX: a texture")
                .create_view(&Default::default())
                .expect("mX: a texture view")
        };
        self.gi = Some(GiState {
            t: GiTargets {
                half,
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
            },
            full_size: (w, h),
            last: 0,
            captured: 0,
            capture_group: None,
        });
        self.gi_groups = None;
        true
    }

    /// The GI's result for the main pass, while it is traced.
    pub(crate) fn gi_view(&self) -> Option<&mx::TextureView> {
        self.gi.as_ref().map(|g| &g.t.full)
    }
}

/// The GI's textures and when it was last traced (its history is good the frame after).
pub(super) struct GiState {
    pub(super) t: GiTargets,
    full_size: (u32, u32),
    pub(super) last: u64,
    /// When the picture was last taken for the rays, and the capture's bind group (by the
    /// picture it reads).
    pub(super) captured: u64,
    capture_group: Option<((u64, mx::TextureView), HeapGroup)>,
}

impl std::ops::Deref for GiState {
    type Target = GiTargets;
    fn deref(&self) -> &GiTargets {
        &self.t
    }
}

impl Renderer {
    /// Trace this frame's GI into `GiTargets::full`, after the traced lighting (the depth
    /// prepass's surfaces) and before the main pass.
    pub(super) fn encode_rt_gi(
        &mut self,
        encoder: &mut mx::CommandEncoder,
        scene: &Scene,
        queries: Option<&mx::QuerySet>,
        timed: &mut Vec<&'static str>,
    ) {
        let (Some(rt), Some(ao)) = (self.rt.as_mut(), self.targets.ao.as_ref()) else {
            return;
        };
        if !rt.gi_on {
            return;
        }
        let Some(gi) = rt.gi.as_mut() else { return };
        gi.last = rt.frame;
        let g = &gi.t;
        let depth = &ao.depth_view;
        let lamps = |a: &Option<crate::gpu::GpuArray>| match a {
            Some(crate::gpu::GpuArray::Buffer(b)) => Some(b.clone()),
            _ => None,
        };
        let (lights, grid) = (lamps(&scene.light_buf), lamps(&scene.grid_buf));
        let probe = self
            .tone
            .probe
            .as_ref()
            .map(|p| p.view.clone())
            .unwrap_or_else(|| self.black_texture.view.clone());
        let key: GiBindKey = (
            (
                rt.bind_gen,
                vec![
                    depth.clone(),
                    g.raw.clone(),
                    g.hist[0].clone(),
                    g.full.clone(),
                    probe.clone(),
                ],
            ),
            lights.clone(),
            grid.clone(),
        );
        if rt.gi_groups.as_ref().is_none_or(|k| k.0 != key) {
            let trace = {
                let mut entries = vec![
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
                        resource: mx::BindingResource::TextureView(depth),
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
                        binding: 7,
                        resource: mx::BindingResource::TextureView(&g.raw),
                    },
                    mx::BindGroupEntry {
                        binding: 8,
                        resource: mx::BindingResource::Sampler(&self.tone.lin_sampler),
                    },
                    mx::BindGroupEntry {
                        binding: 10,
                        resource: mx::BindingResource::TextureView(&probe),
                    },
                    mx::BindGroupEntry {
                        binding: 11,
                        resource: self.tone.enh_buf.as_entire_binding(),
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
                        binding: 12,
                        resource: mx::BindingResource::TextureView(&g.seen),
                    },
                    mx::BindGroupEntry {
                        binding: 15,
                        resource: mx::BindingResource::TextureView(&g.meta),
                    },
                ];
                if let (Some(l), Some(gr)) = (lights.as_ref(), grid.as_ref()) {
                    entries.push(mx::BindGroupEntry {
                        binding: 17,
                        resource: l.as_entire_binding(),
                    });
                    entries.push(mx::BindGroupEntry {
                        binding: 18,
                        resource: gr.as_entire_binding(),
                    });
                }
                HeapGroup::new(super::RT_REFLECT.layout(), 0, &entries)
            };
            let dgroup = |meta: &mx::TextureView,
                          cur: &mx::TextureView,
                          prev: &mx::TextureView,
                          prev_m: &mx::TextureView,
                          out: &mx::TextureView,
                          out_m: &mx::TextureView,
                          step: u64| {
                let view = |binding, v| mx::BindGroupEntry {
                    binding,
                    resource: mx::BindingResource::TextureView(v),
                };
                HeapGroup::new(
                    super::RT_DENOISE.layout(),
                    0,
                    &[
                        mx::BindGroupEntry {
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
                        mx::BindGroupEntry {
                            binding: 11,
                            resource: mx::BindingResource::Buffer(mx::BufferBinding {
                                buffer: &rt.steps,
                                offset: step * rt.step_stride,
                                size: Some(16),
                            }),
                        },
                    ],
                )
            };
            let sp = &rt.spare[2];
            let per = |p: usize| {
                let (h, hm, ph, phm) = (&g.hist[p], &g.histm[p], &g.hist[1 - p], &g.histm[1 - p]);
                [
                    trace.clone(),
                    dgroup(&g.meta, &g.raw, ph, phm, h, hm, 0),
                    dgroup(hm, h, &g.raw, phm, &g.a, sp, 0),
                    dgroup(hm, &g.a, &g.raw, phm, &g.b, sp, 1),
                    dgroup(hm, &g.b, &g.raw, phm, &g.a, sp, 2),
                    dgroup(hm, &g.a, &g.raw, phm, &g.b, sp, 3),
                    dgroup(hm, &g.b, &g.raw, phm, &g.full, sp, 0),
                ]
            };
            rt.gi_groups = Some((key, [per(0), per(1)]));
        }
        let groups = &rt.gi_groups.as_ref().expect("made above").1[(rt.frame % 2) as usize];
        let Some(pipes) = rt.gi_pipes.as_ref() else {
            return;
        };
        let (hx, hy) = (g.half.0.div_ceil(8), g.half.1.div_ceil(8));
        let full = rt.targets.as_ref().map_or((0, 0), |t| t.size);
        {
            let mut pass = encoder.begin_compute_pass(&mx::ComputePassDescriptor {
                label: Some("rt gi"),
                timestamp_writes: pass_timer(queries, timed, "rt gi").map(|t| {
                    mx::PassTimestampWrites {
                        query_set: t.query_set,
                        beginning_of_pass_write_index: t.beginning_of_pass_write_index,
                        end_of_pass_write_index: t.end_of_pass_write_index,
                    }
                }),
            });
            pass.set_pipeline(&pipes.trace);
            groups[0].set_compute(&mut pass);
            pass.dispatch_workgroups(hx, hy, 1);
        }
        let mut pass = encoder.begin_compute_pass(&mx::ComputePassDescriptor {
            label: Some("rt gi denoiser"),
            timestamp_writes: pass_timer(queries, timed, "rt gi denoise").map(|t| {
                mx::PassTimestampWrites {
                    query_set: t.query_set,
                    beginning_of_pass_write_index: t.beginning_of_pass_write_index,
                    end_of_pass_write_index: t.end_of_pass_write_index,
                }
            }),
        });
        pass.set_pipeline(&pipes.temporal);
        groups[1].set_compute(&mut pass);
        pass.dispatch_workgroups(hx, hy, 1);
        pass.set_pipeline(&pipes.atrous);
        for bg in &groups[2..6] {
            bg.set_compute(&mut pass);
            pass.dispatch_workgroups(hx, hy, 1);
        }
        pass.set_pipeline(&pipes.upsample);
        groups[6].set_compute(&mut pass);
        pass.dispatch_workgroups(full.0.div_ceil(8), full.1.div_ceil(8), 1);
    }

    /// After the main pass: the picture at a quarter size each way for the next frame's GI rays
    /// (rt_gi.wgsl `cs_gi_capture`).
    pub(crate) fn encode_rt_gi_capture(
        &mut self,
        encoder: &mut mx::CommandEncoder,
        w: u32,
        h: u32,
        queries: Option<&mx::QuerySet>,
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
        let (Some(gi), Some(pipes)) = (rt.gi.as_mut(), rt.gi_pipes.as_ref()) else {
            return;
        };
        if gi.full_size != (w, h) {
            return;
        }
        let key = (rt.bind_gen, hdr.view.clone());
        if gi.capture_group.as_ref().is_none_or(|g| g.0 != key) {
            let group = HeapGroup::new(
                super::RT_REFLECT.layout(),
                0,
                &[
                    mx::BindGroupEntry {
                        binding: 0,
                        resource: rt.params.as_entire_binding(),
                    },
                    mx::BindGroupEntry {
                        binding: 2,
                        resource: mx::BindingResource::TextureView(&ao.depth_view),
                    },
                    mx::BindGroupEntry {
                        binding: 7,
                        resource: mx::BindingResource::TextureView(&gi.t.seen),
                    },
                    mx::BindGroupEntry {
                        binding: 8,
                        resource: mx::BindingResource::Sampler(&self.tone.lin_sampler),
                    },
                    mx::BindGroupEntry {
                        binding: 9,
                        resource: mx::BindingResource::TextureView(&hdr.view),
                    },
                    mx::BindGroupEntry {
                        binding: 11,
                        resource: self.tone.enh_buf.as_entire_binding(),
                    },
                ],
            );
            gi.capture_group = Some((key, group));
        }
        gi.captured = rt.frame;
        let half = gi.t.half;
        let mut pass = encoder.begin_compute_pass(&mx::ComputePassDescriptor {
            label: Some("rt gi capture"),
            timestamp_writes: pass_timer(queries, timed, "rt gi capture").map(|t| {
                mx::PassTimestampWrites {
                    query_set: t.query_set,
                    beginning_of_pass_write_index: t.beginning_of_pass_write_index,
                    end_of_pass_write_index: t.end_of_pass_write_index,
                }
            }),
        });
        pass.set_pipeline(&pipes.capture);
        gi.capture_group
            .as_ref()
            .expect("made above")
            .1
            .set_compute(&mut pass);
        pass.dispatch_workgroups(half.0.div_ceil(8), half.1.div_ceil(8), 1);
    }
}
