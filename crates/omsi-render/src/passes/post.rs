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

    // --- the smaller picture scaled up to the window, the HUD on top at full size;
    // the smaller the picture, the more it is sharpened
    pub(crate) fn encode_upscale(&self, encoder: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, timers: &mut PassTimers) {
        let (width, height, full_w) = (f.width, f.height, f.full_w);
        let overlays = &f.overlays;
        if let Some((_, bg)) = &f.scene_target {
            let sharpen = (1.0 - width as f32 / full_w as f32) * 2.0;
            self.queue.write_buffer(
                &self.scale.upscale_buf,
                0,
                bytemuck::cast_slice(&[width as f32, height as f32, sharpen.clamp(0.0, 0.8), if f.vanilla_fxaa { 1.0 } else { 0.0 }]),
            );
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
