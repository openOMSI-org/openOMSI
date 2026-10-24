//! The size-keyed targets a picture is drawn into: depth, multisampled colour and depth,
//! ambient occlusion, the enhanced HDR chain, the render scale's smaller targets and the
//! glass's copy of the picture behind it - made on first use and let go of when unused.

use crate::*;

/// High-range colour targets of the enhanced path for one size: the multisampled one the
/// scene is drawn into, the single-sampled one it resolves into, the glow's levels, the
/// tone-mapped picture FXAA reads, and the bind groups of the passes between them.
pub(crate) struct HdrTargets {
    pub(crate) msaa_view: Option<wgpu::TextureView>,
    pub(crate) view: wgpu::TextureView,
    /// The screen mask (`MASK_FORMAT`), multisampled and resolved like the picture.
    pub(crate) mask_msaa: Option<wgpu::TextureView>,
    pub(crate) mask: wgpu::TextureView,
    /// Enhanced+: the reflections' surfaces (`GBUF_FORMAT`, `AUX_FORMAT`): (multisampled,
    /// resolved) each.
    pub(crate) gbuf: Option<[(Option<wgpu::TextureView>, wgpu::TextureView); 2]>,
    /// Glow levels at 1/2, 1/4, ... of the size, and the upsampled sums per level.
    pub(crate) down: Vec<wgpu::TextureView>,
    pub(crate) up: Vec<wgpu::TextureView>,
    pub(crate) ldr: wgpu::TextureView,
    /// down[i] reads the picture (i = 0) or down[i - 1]; up[i] reads up[i + 1] (or the
    /// last down level) and down[i].
    pub(crate) down_bg: Vec<wgpu::BindGroup>,
    pub(crate) up_bg: Vec<wgpu::BindGroup>,
    pub(crate) meter_bg: wgpu::BindGroup,
    /// Tone mapping with the adapted exposure in `adapt[k]`.
    pub(crate) tonemap_bg: [wgpu::BindGroup; 2],
    pub(crate) fxaa_bg: wgpu::BindGroup,
    /// Classic shading is presented directly, without the Enhanced tone curve.
    pub(crate) classic_bg: wgpu::BindGroup,
    /// Allocated only when wet roads need scene reflections in the main view.
    pub(crate) puddles: Option<puddles::Targets>,
}

/// The enhanced path's reflection probe: a cube map of the sky around the camera with a
/// GGX-blurred mip chain, drawn now and then.
pub(crate) struct Probe {
    pub(crate) view: wgpu::TextureView,
    /// Per mip level, per face: the view the level is drawn into.
    pub(crate) faces: Vec<Vec<wgpu::TextureView>>,
    /// Per mip level, the two passes' bind groups (faces 0-2 and 3-5).
    pub(crate) bind_groups: Vec<[wgpu::BindGroup; 2]>,
    pub(crate) sky_pipeline: wgpu::RenderPipeline,
    pub(crate) filter_pipeline: wgpu::RenderPipeline,
    /// The sky table scale it was drawn with (when, see `SkySched`).
    pub(crate) scale: f32,
    /// The enhanced sky itself at `SKY_CUBE_SIZE` a face (clouds and all), drawn a face a
    /// frame; the dome reads it (camera binding 17). The volumetric clouds cost 6-18 ms a
    /// frame drawn for every pixel of a 1440p sky; a face a frame is well under one.
    pub(crate) cube_view: wgpu::TextureView,
    pub(crate) cube_tex: wgpu::Texture,
    pub(crate) cube_faces: Vec<wgpu::TextureView>,
    pub(crate) cube_bind_groups: Vec<wgpu::BindGroup>,
    pub(crate) cube_pipeline: wgpu::RenderPipeline,
    pub(crate) cube_next: u32,
    pub(crate) cube_filled: bool,
    /// Redraws so far (picks the round of the next).
    pub(crate) cube_round: u32,
    /// Where every face of the cube is drawn from (world). A face drawn from wherever the
    /// camera stood at the time left six faces from six places: flying up towards the
    /// clouds, the faces disagreed at their seams and the blended history dragged each one
    /// behind the camera, so the clouds shook and lagged. The cube keeps one eye; the dome
    /// takes the parallax out, and the eye moves on (the whole cube redrawn) once the
    /// camera has gone far enough for the correction to show.
    pub(crate) cube_eye: Option<DVec3>,
    /// Which slices of the cube are drawn when, the cube for the next eye and the probe's
    /// turn.
    pub(crate) sched: SkySched,
}

/// The textures of the ambient-occlusion pass for one target size.
pub(crate) struct AoTargets {
    pub(crate) size: (u32, u32),
    pub(crate) depth_view: wgpu::TextureView,
    /// (the texture itself, to read the depth back for the photo mode)
    pub(crate) depth_tex: wgpu::Texture,
    pub(crate) ao_view: wgpu::TextureView,
    pub(crate) blur_view: wgpu::TextureView,
    /// The last frame's picture (absolute radiance) for the bounced light.
    pub(crate) hist_view: wgpu::TextureView,
    /// Whether `hist_view` is a picture (not the single texel of the other pipelines).
    pub(crate) bounce: bool,
    pub(crate) ssao_bg: wgpu::BindGroup,
    pub(crate) blur_bg: wgpu::BindGroup,
    /// The lamps' light in the fog (`fog_lamps.wgsl`): worked out at half size into
    /// `fog_view`, then added onto the picture (bind groups of the two passes).
    pub(crate) fog_view: wgpu::TextureView,
    pub(crate) fog_bg: Option<[wgpu::BindGroup; 2]>,
}

impl Renderer {
    /// The smaller colour target of the render scale for this size, with its bind group.
    pub(crate) fn scale_target(&mut self, w: u32, h: u32) -> (wgpu::TextureView, wgpu::BindGroup) {
        self.targets.target_use.insert((w, h), std::time::Instant::now());
        if let Some(t) = self.targets.scale_targets.get(&(w, h)) {
            return t.clone();
        }
        self.evict_targets();
        let raw_formats = [self.format.remove_srgb_suffix()];
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scaled scene"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            // (FSR 1 may read it as stored, `shaders::fsr1_raw_input`)
            view_formats: if crate::shaders::fsr1_raw_input(self.format) { &raw_formats } else { &[] },
        });
        let view = tex.create_view(&Default::default());
        let bg = self.picture_group(&view);
        self.targets.scale_targets
            .insert((w, h), (view.clone(), bg.clone()));
        (view, bg)
    }

    pub(crate) fn picture_group(&self, view: &wgpu::TextureView) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene picture"),
            layout: &self.scale.upscale_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.scale.upscale_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.tone.post_sampler) },
            ],
        })
    }

    /// The scene's slot for the picture behind the glass (black until a frame is drawn).
    pub(crate) fn glass_slot(&self, scene: &mut Scene) -> TextureId {
        if let Some(id) = scene.glass_slot {
            return id;
        }
        scene.textures.push(GpuTexture::showing(self.black_texture.texture.clone(), self.black_texture.view.clone(), (1, 1)));
        let id = scene.textures.len() - 1;
        scene.glass_slot = Some(id);
        id
    }

    /// Refraction reads a copy made before drawing films, avoiding rain/wiper feedback.
    pub(crate) fn prepare_glass_behind(&mut self, scene: &mut Scene, width: u32, height: u32, format: wgpu::TextureFormat) {
        let Some(id) = scene.glass_slot else { return };
        if self.targets.glass_picture.as_ref().is_none_or(|v| v.texture().width() != width || v.texture().height() != height || v.texture().format() != format) {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("picture behind glass"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1, sample_count: 1,
                dimension: wgpu::TextureDimension::D2, format,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.targets.glass_picture = Some(tex.create_view(&Default::default()));
        }
        let view = self.targets.glass_picture.as_ref().unwrap();
        if scene.textures[id].view != *view {
            scene.textures[id] = GpuTexture::showing(view.texture().clone(), view.clone(), (width, height));
            self.rebind_textures(scene, &[id]);
        }
    }

    /// The ambient-occlusion textures for a target of this size (rebuilt on resize).
    pub(crate) fn ensure_ao(&mut self, w: u32, h: u32, bounce: bool) -> bool {
        if self.targets.ao.as_ref().map(|a| a.size == (w, h) && (a.bounce || !bounce)).unwrap_or(false) {
            return false;
        }
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("prepass depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            // (copied from: by the puddles, and read back for the photo mode's depth of field)
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        // the AO itself at half size: four times fewer pixels, and the blur hides the rest
        let half = wgpu::Extent3d {
            width: w.div_ceil(2),
            height: h.div_ceil(2),
            depth_or_array_layers: 1,
        };
        let mk = |label: &str| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: half,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // r the occlusion, g the view depth (m) it was worked out at: the main
                // pass upsamples it by depth, not bilinearly (see `ao_at` in shader.wgsl);
                // b and a the light the surfaces round the point send back to it
                // (ssao.wgsl `bounce_pack`)
                // (two channels without the bounced light)
                format: if bounce { wgpu::TextureFormat::Rgba16Float } else { wgpu::TextureFormat::Rg16Float },
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let ao = mk("ssao");
        let blur = mk("ssao blur");
        // the last frame's picture at this size as absolute radiance, written after the
        // frame's main pass (post.wgsl `fs_history`) and read by the next frame's AO
        // (only the Enhanced picture with bounced light has one: another pipeline binds a
        // single texel)
        let hist = if bounce {
            mk("ssao history")
        } else {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("ssao history (unused)"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let hist_view = hist.create_view(&Default::default());
        let depth_view = depth.create_view(&Default::default());
        let ao_view = ao.create_view(&Default::default());
        let blur_view = blur.create_view(&Default::default());

        let bg = |label: &str, tex: &wgpu::TextureView| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &self.ao_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.ao_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&depth_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(tex),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&hist_view),
                    },
                ],
            })
        };
        // the SSAO pass reads no AO texture, but the layout wants one: bind the blur target
        let ssao_bg = bg("ssao", &blur_view);
        let blur_bg = bg("ssao blur", &ao_view);
        let fog_view = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fog lamps"),
            // (one row more: the sun's visibility, see fog_lamps.wgsl)
            size: wgpu::Extent3d { height: half.height + 1, ..half },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }).create_view(&Default::default());
        let fog_bg = self.lights.fog_lamps_pipeline.is_some().then(|| {
            // (the half-size pass reads no result: the layout wants one, the AO's is bound)
            [&ao_view, &fog_view].map(|read| {
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("fog lamps"),
                    layout: &self.lights.fog_lamps_layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: self.lights.fog_lamps_buf.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&depth_view) },
                        wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(read) },
                    ],
                })
            })
        });
        self.targets.ao = Some(AoTargets {
            size: (w, h),
            depth_view,
            depth_tex: depth.clone(),
            ao_view,
            blur_view,
            hist_view,
            bounce,
            ssao_bg,
            blur_bg,
            fog_view,
            fog_bg,
        });
        true
    }

    /// Before a new size's targets are made: let go of the sizes nobody asked for lately
    /// (dragging the window's edge made a full set per frame, and the old caches, cleared
    /// only past a dozen entries, could hold gigabytes meanwhile), and past a handful of
    /// sizes the least recently used (the window, the scaled picture and the mirrors are
    /// asked for every frame they are drawn).
    pub(crate) fn evict_targets(&mut self) {
        self.evict_targets_unused_for(std::time::Duration::from_millis(250));
    }

    fn evict_targets_unused_for(&mut self, stale: std::time::Duration) {
        const KEEP: usize = 10;
        let now = std::time::Instant::now();
        self.targets.evicted_at = now;
        let mut uses: Vec<((u32, u32), std::time::Instant)> = self.targets.target_use.iter().map(|(k, t)| (*k, *t)).collect();
        uses.sort_by_key(|(_, t)| std::cmp::Reverse(*t));
        let keep: std::collections::HashSet<(u32, u32)> = uses.iter().filter(|(_, t)| now.duration_since(*t) < stale).take(KEEP).map(|(k, _)| *k).collect();
        self.targets.target_use.retain(|k, _| keep.contains(k));
        self.targets.scale_targets.retain(|k, _| keep.contains(k));
        self.targets.msaa_targets.retain(|k, _| keep.contains(k));
        self.targets.hdr_targets.retain(|k, _| keep.contains(k));
    }

    /// The multisampled colour (`color`: where the pass draws into it) and depth attachments
    /// for a target of this size. The colour is made the first time it is asked for: the
    /// Enhanced picture draws into a multisampled target of its own and only the depth is
    /// used, and a colour of the window's size was made for nothing (130 MB at 4K with 4x).
    pub(crate) fn msaa_targets(&mut self, w: u32, h: u32, color: bool) -> (Option<wgpu::TextureView>, wgpu::TextureView) {
        self.targets.target_use.insert((w, h), std::time::Instant::now());
        if !self.targets.msaa_targets.contains_key(&(w, h)) {
            self.evict_targets();
        } else if self.targets.evicted_at.elapsed() > std::time::Duration::from_secs(1) {
            self.evict_targets_unused_for(std::time::Duration::from_secs(5));
        }
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let make_color = || self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("msaa colour"),
            size,
            mip_level_count: 1,
            sample_count: self.options.msaa,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        }).create_view(&Default::default());
        if let Some(t) = self.targets.msaa_targets.get_mut(&(w, h)) {
            if color && t.0.is_none() {
                t.0 = Some(make_color());
            }
            return t.clone();
        }
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("msaa depth"),
            size,
            mip_level_count: 1,
            sample_count: self.options.msaa,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let t = (color.then(make_color), depth.create_view(&Default::default()));
        self.targets.msaa_targets.insert((w, h), t.clone());
        t
    }

    /// The HDR colour targets of the enhanced path for this size, with the glow's levels,
    /// the tone-mapped picture FXAA reads and the post passes' bind groups.
    pub(crate) fn hdr_targets(&mut self, w: u32, h: u32) -> bool {
        self.targets.target_use.insert((w, h), std::time::Instant::now());
        if self.targets.hdr_targets.contains_key(&(w, h)) {
            return false;
        }
        self.evict_targets();
        let fmt = wgpu::TextureFormat::Rgba16Float;
        let target = |label: &str, tw: u32, th: u32, format: wgpu::TextureFormat, samples: u32| {
            let usage = if samples > 1 {
                wgpu::TextureUsages::RENDER_ATTACHMENT
            } else {
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC
            };
            self.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: tw.max(1),
                        height: th.max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let msaa_view =
            (self.options.msaa > 1).then(|| target("hdr msaa", w, h, fmt, self.options.msaa));
        let view = target("hdr", w, h, fmt, 1);
        let mask_msaa = (self.options.msaa > 1).then(|| target("screen mask msaa", w, h, MASK_FORMAT, self.options.msaa));
        let mask = target("screen mask", w, h, MASK_FORMAT, 1);
        let gbuf = rt_gbuf().then(|| {
            [GBUF_FORMAT, AUX_FORMAT].map(|f| ((self.options.msaa > 1).then(|| target("rt surfaces msaa", w, h, f, self.options.msaa)), target("rt surfaces", w, h, f, 1)))
        });
        // the glow: halving until the smallest level is a few dozen pixels across
        let levels = GLOW_LEVELS
            .min((w.min(h).max(16) as f32).log2() as usize - 3)
            .max(1);
        let down: Vec<wgpu::TextureView> = (1..=levels)
            .map(|k| target("glow down", w >> k, h >> k, fmt, 1))
            .collect();
        let up: Vec<wgpu::TextureView> = (1..=levels)
            .map(|k| target("glow up", w >> k, h >> k, fmt, 1))
            .collect();
        let ldr = target("tone mapped", w, h, wgpu::TextureFormat::Rgba8Unorm, 1);
        let bg_with = |src: &wgpu::TextureView, base: &wgpu::TextureView, adapt: &wgpu::TextureView, screen_mask: &wgpu::TextureView| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post"),
                layout: &self.tone.post_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.tone.post_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.tone.post_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(base),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(adapt),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(screen_mask),
                    },
                ],
            })
        };
        let bg = |src: &wgpu::TextureView, base: &wgpu::TextureView, adapt: &wgpu::TextureView| bg_with(src, base, adapt, &self.black_texture.view);
        let none = &self.white_texture.view;
        // (the first level of the glow reads the screen mask as its `t_base`: no light from
        // the screens)
        let down_bg: Vec<wgpu::BindGroup> = (0..levels)
            .map(|i| bg(if i == 0 { &view } else { &down[i - 1] }, if i == 0 { &mask } else { none }, none))
            .collect();
        let up_bg: Vec<wgpu::BindGroup> = (0..levels)
            .map(|i| {
                bg(
                    if i + 1 == levels {
                        &down[i]
                    } else {
                        &up[i + 1]
                    },
                    &down[i],
                    none,
                )
            })
            .collect();
        let meter_bg = bg(&down[levels - 1], none, none);
        let tonemap_bg = [
            bg_with(&view, &up[0], &self.tone.adapt_views[0], &mask),
            bg_with(&view, &up[0], &self.tone.adapt_views[1], &mask),
        ];
        // (FXAA reads the screen mask as its `t_base` and leaves the screens as they are)
        let fxaa_bg = bg(&ldr, &mask, none);
        let classic_bg = self.picture_group(&view);
        self.targets.hdr_targets.insert(
            (w, h),
            HdrTargets {
                msaa_view,
                view,
                mask_msaa,
                mask,
                gbuf,
                down,
                up,
                ldr,
                down_bg,
                up_bg,
                meter_bg,
                tonemap_bg,
                fxaa_bg,
                classic_bg,
                puddles: None,
            },
        );
        true
    }

    #[allow(dead_code)]
    pub(crate) fn ensure_depth(&mut self, w: u32, h: u32) {
        if let Some((_, _, dw, dh)) = &self.targets.depth {
            if *dw == w && *dh == h {
                return;
            }
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        self.targets.depth = Some((tex, view, w, h));
    }
}

/// The size-keyed targets (see targets.rs): made on first use, let go of when unused.
pub(crate) struct Targets {
    pub(crate) depth: Option<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
    /// Multisampled colour and depth targets per size (the window, each mirror).
    pub(crate) msaa_targets: HashMap<(u32, u32), (Option<wgpu::TextureView>, wgpu::TextureView)>,
    /// Screen-space ambient occlusion: depth prepass and AO textures of the window's size.
    pub(crate) ao: Option<AoTargets>,
    /// Enhanced path: the HDR targets per size, the post pipelines and their resources.
    pub(crate) hdr_targets: HashMap<(u32, u32), HdrTargets>,
    pub(crate) scale_targets: HashMap<(u32, u32), (wgpu::TextureView, wgpu::BindGroup)>,
    pub(crate) triple_targets: Option<((u32, u32), Vec<(wgpu::TextureView, wgpu::BindGroup)>)>,
    /// Full-resolution current scene before rain films, in its original colour format.
    pub(crate) glass_picture: Option<wgpu::TextureView>,
    /// When each size of the size-keyed targets (scale, MSAA, HDR) was last asked for.
    pub(crate) target_use: HashMap<(u32, u32), std::time::Instant>,
    pub(crate) evicted_at: std::time::Instant,
}

/// Full-sized EASU output kept between frames, recreated when the window size changes.
/// Reuse matters on low-memory cards: allocating this each frame causes VRAM churn.
pub(crate) struct FsrTarget {
    pub(crate) size: (u32, u32),
    /// When a picture of this size last went through it (a size not seen for a while - the
    /// window's before a resize - is let go).
    pub(crate) used: std::cell::Cell<std::time::Instant>,
    pub(crate) _texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    pub(crate) bind_group: wgpu::BindGroup,
}

/// The render scale: the pipelines that scale the 3D picture up to the window or copy a panel, and the governor's factor.
pub(crate) struct Upscale {
    /// Render scale: the pipeline that scales the 3D picture up to the window, its
    /// parameters, and the smaller targets per size (with their bind groups).
    pub(crate) upscale_pipeline: wgpu::RenderPipeline,
    pub(crate) fsr: Option<crate::pipelines::upscale::Fsr1>,
    /// FSR 1's full-sized intermediate pictures by size (the window's, a VR eye's, a
    /// triple-screen panel's: a few, kept, not made anew each frame).
    pub(crate) fsr_target: std::cell::RefCell<Vec<FsrTarget>>,
    /// FSR 1's passes were refused by the driver once (`set_upscaler`): not tried again.
    pub(crate) fsr_refused: bool,
    /// What `set_upscaler` was last asked for (at the start: the settings'), FSR 1, its
    /// sharpening and the render scale: see `Renderer::upscaler_asked`.
    pub(crate) upscaler_asked: (bool, f32, f32),
    /// OMSI_FSR_RAW_INPUT: the scene picture it was made for and the bind group reading it
    /// through a view that is not sRGB (made again only for a new picture, not each frame).
    pub(crate) fsr_raw_bg: std::cell::RefCell<Option<(wgpu::Texture, wgpu::BindGroup)>>,
    /// FSR 1 without its mip bias (OMSI_NO_FSR_MIP_BIAS; the tests compare with it).
    pub(crate) fsr_mip_bias_off: std::cell::Cell<bool>,
    pub(crate) copy_pipeline: wgpu::RenderPipeline,
    /// Triple screen: a panel's picture copied 1:1 to its place in the window.
    pub(crate) panel_pipeline: wgpu::RenderPipeline,
    pub(crate) upscale_layout: wgpu::BindGroupLayout,
    pub(crate) upscale_buf: wgpu::Buffer,
    /// The game's frame-rate governor on top of the render scale (1 = none; see
    /// `set_dynamic_scale`).
    pub(crate) dynamic_scale: std::cell::Cell<f32>,
}
