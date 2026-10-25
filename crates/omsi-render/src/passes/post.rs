//! The enhanced path's post passes (glow, metering and adaptation, tone map, FXAA) and
//! the scaled picture's way up to the window.

use super::*;

impl Renderer {
    // --- the post passes: glow, metering and adaptation, tone curve, FXAA
    pub(crate) fn encode_post(&mut self, encoder: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, puddles_on: bool, timers: &mut PassTimers) {
        let (width, height, lighting, dt) = (f.width, f.height, f.lighting, f.dt);
        let (with_overlays, lead_view, rt_frame) = (f.with_overlays, f.lead_view, f.rt_frame);
        let scene_view = f.scene_view();
        let overlays = &f.overlays;
        let secs = |tau: f32| {
            if dt > 0.0 {
                1.0 - (-dt / tau).exp()
            } else {
                1.0
            }
        };
        // (the brightening is for what the light model cannot know by day - a dark cab,
        // an underpass; by night the picture is dark because the night is, and the eye's
        // adaptation to it is the light model's already: lifted by the meter on top, a
        // lamp-lit street came out most of a stop brighter than any eye sees it)
        let mut m = meter_tuning();
        let after_sunset = self.sky.sky_state.as_ref().map_or(0.0, |st| ((0.052 - st.input.sun_dir.normalize_or_zero().z) / 0.157).clamp(0.0, 1.0));
        let (brighten, shortfall_gain) = shortfall_adaptation(self.tone.exposure.unwrap_or(0.0) / std::f32::consts::LN_2, m[3], m[0]);
        m[3] = brighten;
        let pu = PostUniform {
            // the metering may take a little off a bright picture and add a little to a
            // dark one: a night stays a night, snow stays white
            a: [
                // (the share of the light beyond the screen's white the eye scatters
                // further than a third of a degree, CIE 146 - see post.wgsl `fs_up`)
                0.2,
                m[2],
                m[3],
                if self.instant_exposure || dt <= 0.0 {
                    1.0
                } else {
                    0.0
                },
            ],
            // darker: the eye takes a few seconds; brighter: under one
            b: [secs(ADAPT_TO_DARK_S), secs(ADAPT_TO_LIGHT_S), m[1], m[4] + lighting.night_brightness * after_sunset],
            // (w: an LED panel's dots count for this much in the glow's source. The mix
            // the glow lands with is a few per cent - a lamp a hundred times brighter
            // than white spreads, a white wall does not - so the dots are multiplied up
            // there instead of being drawn burning: their halo shows, they don't bleach.
            // `Led glow`, 0 = not at all.)
            c: [m[0], m[5], self.tone.exposure.map(f32::exp).unwrap_or(1.0), lighting.led_glow * 10.0],
            // Enhanced+: its grade, the vignette and the sharpening; both: the tone
            // curve's contrast (post.wgsl `natural_tone`)
            d: {
                let contrast = tone_contrast(self.tone.exposure.unwrap_or(0.0));
                if rt_frame && !f.env.no_rt_grade { [1.0, 0.08, 0.0, contrast] } else { [0.0, 0.0, 0.0, contrast] }
            },
            e: [shortfall_gain, 0.0, 0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.tone.post_buf, 0, bytemuck::bytes_of(&pu));
        // (a mirror's small picture goes without FXAA)
        let fxaa = with_overlays
            && self.options.fxaa
            && self.options.msaa <= 1
            && !f.env.no_fxaa;
        if let Some(h) = self.targets.hdr_targets.get(&(width, height)) {
            let puddles = h.puddles.as_ref().filter(|_| puddles_on);
            let levels = h.down.len();
            // the picture for the next frame's bounced light (the window's only)
            if lead_view && crate::lighting::bounce_wanted(f.ao_on, f.enhanced_frame, self.street_light()) {
                if let Some(ao) = self.targets.ao.as_ref() {
                    post_pass(encoder, &ao.hist_view, None, &self.tone.post.history, puddles.map(|p| &p.down_bg).unwrap_or(&h.down_bg[0]));
                }
            }
            for i in 0..levels {
                post_pass(
                    encoder,
                    &h.down[i],
                    None,
                    if i == 0 {
                        &self.tone.post.down_first
                    } else {
                        &self.tone.post.down
                    },
                    if i == 0 { puddles.map(|p| &p.down_bg).unwrap_or(&h.down_bg[i]) } else { &h.down_bg[i] },
                );
            }
            // the exposure: meter the smallest level, move the adapted value towards it
            // (the window's picture only: a mirror is graded with the window's exposure,
            // as the eye that looks into it is adapted to the street)
            if lead_view {
                post_pass(
                    encoder,
                    &self.tone.meter_view,
                    None,
                    &self.tone.post.meter,
                    &h.meter_bg,
                );
                let front = self.tone.adapt_front;
                post_pass(
                    encoder,
                    &self.tone.adapt_views[1 - front],
                    None,
                    &self.tone.post.adapt,
                    &self.tone.adapt_bg[front],
                );
                self.tone.adapt_front = 1 - front;
            }
            // (a device lost since this frame began took the meter's buffer as well)
            let lost = self.device_lost().is_some();
            if let Some(log) = self.tone.exposure_log.as_mut().filter(|_| with_overlays && !lost) {
                let pre = self.tone.exposure.unwrap_or(0.0) / std::f32::consts::LN_2;
                log.sample(encoder, &self.tone.adapt_views[self.tone.adapt_front], pre, m);
            }
            // (timed on its last pass: the glow chain with the metering, see GpuTimers)
            for i in (0..levels).rev() {
                let timer = if i == 0 {
                    pass_timer(timers.set.as_ref(), &mut timers.timed, "glow+meter")
                } else {
                    None
                };
                post_pass(encoder, &h.up[i], timer, &self.tone.post.up, &h.up_bg[i]);
            }
            let final_view = if fxaa { &h.ldr } else { scene_view };
            let tonemap_bg = &puddles.map(|p| &p.tonemap_bg).unwrap_or(&h.tonemap_bg)[self.tone.adapt_front];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tone map"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: final_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: pass_timer(timers.set.as_ref(), &mut timers.timed, "tone map"),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(if fxaa {
                &self.tone.post.tonemap_encoded
            } else {
                &self.tone.post.tonemap
            });
            pass.set_bind_group(0, tonemap_bg, &[]);
            pass.draw(0..3, 0..1);
            if fxaa {
                drop(pass);
                pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("fxaa"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: scene_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: pass_timer(timers.set.as_ref(), &mut timers.timed, "fxaa"),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.tone.post.fxaa);
                pass.set_bind_group(0, &h.fxaa_bg, &[]);
                pass.draw(0..3, 0..1);
            }
            if !overlays.is_empty() && !f.scaled {
                pass.set_pipeline(&self.overlay_pipeline_1x);
                for (k, _) in overlays.iter().enumerate() {
                    if let Some((_, _, bg, _)) = scene.overlay_res.get(k) {
                        pass.set_bind_group(0, bg, &[]);
                        pass.draw(0..6, 0..1);
                    }
                }
            }
        }
    }

    /// FSR 1 scales this view's picture up: asked for and built, the window's picture or an
    /// eye's (not a mirror's, nor with vanilla FXAA), drawn smaller than its target.
    pub(crate) fn fsr_draws(&self, f: &FrameCtx) -> bool {
        self.options.fsr1 && (f.with_overlays || f.xr_view) && !f.vanilla_fxaa
            && f.scene_target.is_some() && f.width < f.full_w && f.height < f.target.texture().height()
            && self.scale.fsr.is_some()
    }

    /// The textures' mip bias for this view (`CameraUniform::lod`): log2 of the scale while
    /// FSR 1 scales its picture up (-0.58 at 67 %, -1 at 50 %), as AMD advises for FSR 1;
    /// else 0. (The standard upscaler keeps the textures as they were: its sharpening does
    /// its part, and the picture it gives is unchanged.)
    pub(crate) fn fsr_mip_bias(&self, f: &FrameCtx) -> f32 {
        let off = omsi_cfg::flags::OMSI_NO_FSR_MIP_BIAS.is_set() || self.scale.fsr_mip_bias_off.get();
        if !off && self.fsr_draws(f) { (f.width as f32 / f.full_w.max(1) as f32).log2().clamp(-1.0, 0.0) } else { 0.0 }
    }

    // --- the smaller picture scaled up to the window, the HUD on top at full size;
    // the smaller the picture, the more it is sharpened
    pub(crate) fn encode_upscale(&self, encoder: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, timers: &mut PassTimers) {
        let (width, height, full_w) = (f.width, f.height, f.full_w);
        let overlays = &f.overlays;
        if let Some((_, bg)) = &f.scene_target {
            // Opt-in FSR1 experiment. Use the existing scene scale setting for quality.
            // Leave mirrors, offscreen views, vanilla FXAA and the default path untouched.
            let full_h = f.target.texture().height();
            let fsr = self.fsr_draws(f);
            // (RCAS: the slider's whole range onto 0..0.8 of AMD's lobe, `fsr1_rcas_strength`;
            // a picture FSR does not draw - a mirror, vanilla FXAA - keeps the standard
            // upscaler's own sharpening: the slider is RCAS's, and at its default the
            // standard one's CAS ringed round the trees)
            let sharpen = if fsr {
                crate::fsr1_rcas_strength(self.options.fsr_sharpness)
            } else {
                ((1.0 - width as f32 / full_w as f32) * 2.0).clamp(0.0, 0.8)
            };
            self.queue.write_buffer(
                &self.scale.upscale_buf,
                0,
                bytemuck::cast_slice(&[width as f32, height as f32, sharpen, if f.vanilla_fxaa { 1.0 } else { 0.0 }]),
            );
            if fsr {
                // At sharpness 0 RCAS gives its input back (its lobe is 0): EASU then draws
                // the window's picture itself - a full-screen pass and its intermediate saved.
                let sharpened = sharpen > 0.0;
                let size = (full_w, full_h);
                let mut cached = self.scale.fsr_target.borrow_mut();
                // (the sizes no view drew at for two seconds go: a resized window's old one,
                // 33 MB at 4K, stayed for as long as two other sizes did not take its place)
                let now = std::time::Instant::now();
                cached.retain(|t| t.size == size || now.duration_since(t.used.get()).as_secs_f32() < 2.0);
                if sharpened && !cached.iter().any(|t| t.size == size) {
                    let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("FSR1 EASU full-resolution intermediate"),
                        size: wgpu::Extent3d {
                            width: full_w, height: full_h, depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: self.scale.fsr.as_ref().map_or(self.format, |p| p.mid_format),
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    });
                    let view = texture.create_view(&Default::default());
                    let bind_group = self.picture_group(&view);
                    // (three sizes at most: the window, a VR eye, a panel)
                    if cached.len() >= 3 {
                        cached.remove(0);
                    }
                    cached.push(FsrTarget { size, used: std::cell::Cell::new(now), _texture: texture, view, bind_group });
                }
                let intermediate = sharpened.then(|| cached.iter().find(|t| t.size == size).expect("FSR1 intermediate initialized"));
                if let Some(t) = intermediate {
                    t.used.set(now);
                }
                // (OMSI_FSR_RAW_INPUT: the scene through a view that is not sRGB)
                let raw_bg = self.scale.fsr.as_ref().filter(|p| p.raw_input).and_then(|_| f.scene_target.as_ref()).map(|(view, _)| {
                    let mut kept = self.scale.fsr_raw_bg.borrow_mut();
                    if kept.as_ref().is_none_or(|(t, _)| t != view.texture()) {
                        let raw = view.texture().create_view(&wgpu::TextureViewDescriptor { format: Some(self.format.remove_srgb_suffix()), ..Default::default() });
                        *kept = Some((view.texture().clone(), self.picture_group(&raw)));
                    }
                    kept.as_ref().expect("made above").1.clone()
                });
                // The HUD is composed on the finished picture, never upscaled or sharpened.
                let draw_overlays = |pass: &mut wgpu::RenderPass| {
                    if !overlays.is_empty() {
                        pass.set_pipeline(&self.overlay_pipeline_1x);
                        for (k, _) in overlays.iter().enumerate() {
                            if let Some((_, _, bg, _)) = scene.overlay_res.get(k) {
                                pass.set_bind_group(0, bg, &[]);
                                pass.draw(0..6, 0..1);
                            }
                        }
                    }
                };
                // EASU reconstructs edges from the low-resolution, tone-mapped image.
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("FSR1 EASU"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: intermediate.map_or(f.target, |t| &t.view),
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: pass_timer(timers.set.as_ref(), &mut timers.timed, "FSR1 EASU"),
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    let p = self.scale.fsr.as_ref().expect("FSR1 pipelines");
                    pass.set_pipeline(if intermediate.is_some() { &p.easu_mid } else { &p.easu });
                    pass.set_bind_group(0, raw_bg.as_ref().unwrap_or(bg), &[]);
                    pass.draw(0..3, 0..1);
                    if intermediate.is_none() {
                        draw_overlays(&mut pass);
                    }
                }
                // RCAS sharpens the reconstructed full-size picture.
                if let Some(intermediate) = intermediate {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("FSR1 RCAS"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: f.target,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: pass_timer(timers.set.as_ref(), &mut timers.timed, "FSR1 RCAS"),
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    pass.set_pipeline(&self.scale.fsr.as_ref().expect("FSR1 pipelines").rcas);
                    pass.set_bind_group(0, &intermediate.bind_group, &[]);
                    pass.draw(0..3, 0..1);
                    draw_overlays(&mut pass);
                }
                return;
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("upscale"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: f.target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: pass_timer(timers.set.as_ref(), &mut timers.timed, "upscale"),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.scale.upscale_pipeline);
            pass.set_bind_group(0, bg, &[]);
            pass.draw(0..3, 0..1);
            if !overlays.is_empty() {
                pass.set_pipeline(&self.overlay_pipeline_1x);
                for (k, _) in overlays.iter().enumerate() {
                    if let Some((_, _, bg, _)) = scene.overlay_res.get(k) {
                        pass.set_bind_group(0, bg, &[]);
                        pass.draw(0..6, 0..1);
                    }
                }
            }
        }
    }
}
