//! The main pass: the multisampled depth prepass, the parts of a split main pass and the
//! picture itself with the sky, the smoke, the snowfall, the coronas and the cab.

use super::*;

/// Where the main pass draws, and how its targets start.
struct MainTargets<'t> {
    draw_view: &'t wgpu::TextureView,
    resolve_view: Option<&'t wgpu::TextureView>,
    depth_view: &'t wgpu::TextureView,
    hdr: Option<&'t HdrTargets>,
    pp: &'t PassPipelines,
    sky_pipe: &'t wgpu::RenderPipeline,
    sky_clear: wgpu::LoadOp<wgpu::Color>,
    depth_first: wgpu::LoadOp<f32>,
    depth_store: wgpu::StoreOp,
    parts: usize,
    presurface_prefill: bool,
    /// the ordinary scenery's depth, laid into the multisampled buffer at the start of
    /// the pass (of its first part) before the colour draws (see `encode_prefill`)
    prefill: Option<(&'t [Batch], &'t [wgpu::RenderPipeline; 6])>,
    /// the bundles of the last part (all of them when the pass is not split)
    bundles: &'t [wgpu::RenderBundle],
}

impl Renderer {
    pub(crate) fn shares_prepass_depth(&self, f: &FrameCtx, plan: &DrawPlan) -> bool {
        f.prepass_on && self.options.msaa <= 1 && self.targets.ao.is_some() && !plan.has_presurface && !f.puddles_wanted
    }

    /// The main pass into `encoder`, the parts before its last one (when it is split) into
    /// encoders of their own, which are returned.
    pub(crate) fn encode_main_pass(&mut self, encoder: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, plan: &DrawPlan, main_bundles: &[wgpu::RenderBundle], timers: &mut PassTimers) -> Vec<wgpu::CommandEncoder> {
        let (width, height, enhanced, with_overlays, lighting) = (f.width, f.height, f.enhanced, f.with_overlays, f.lighting);
        // Without multisampling the main pass tests against the depth the prepass left
        // (when there was one): the costly shading - lighting, the shadow filter - is then
        // done once per pixel for the surface that is seen, not for every tree and wall
        // hidden behind it.
        let single = self.options.msaa <= 1;
        // A presurface must colour its below-ground faces before its invisible cover
        // seals them. Reusing prepass depth would reject those faces (or let terrain
        // reject them first). The prepass still supplies AO; colour rebuilds its depth.
        // A depth-writing window in the colour pass must not replace the road receiver.
        let share_depth = self.shares_prepass_depth(f, plan);
        // (the colour only where the picture is drawn into it: not into the HDR targets)
        let msaa_color = !single && !(f.masked_frame && self.targets.hdr_targets.contains_key(&(width, height)));
        let targets = if share_depth {
            None
        } else {
            Some(self.msaa_targets(width, height, msaa_color))
        };
        // With multisampling the prepass above (single-sampled, for the ambient occlusion)
        // cannot be the main pass's depth: the enhanced picture lays its depth again into
        // the multisampled buffer first. Without it every wall, tree and car hidden behind
        // the one in front ran the whole enhanced shading (11 ms of a 1080p frame in
        // central Spandau with 4x MSAA).
        // Apple's hidden-surface removal already handles ordinary opaque draws.
        // Preserve that fast path. In the measured mixed views, prefilling MSAA
        // depth before cutouts/blends saved more hidden shading than the pass cost.
        let msaa_prepass = plan.msaa_prepass(self, f);
        let presurface_prefill = plan.presurface_prefill(self, f);
        let parts = if !cfg!(any(target_os = "macos", target_os = "ios")) && main_bundles.len() >= 2 && !f.env.no_main_split {
            main_bundles.len().min(2)
        } else { 1 };
        let mut lead = (parts > 1).then(|| self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("main part") }));
        // The prefill is drawn inside the main pass, before its colour draws (as with an
        // excavation, `presurface_prefill`): as a render pass of its own it stored the
        // multisampled depth (133 MB at 4K) and the main pass loaded it straight back.
        let prefill = (msaa_prepass && !presurface_prefill && targets.is_some())
            .then_some(())
            .and(self.prefill_pipelines(f.eye_color))
            .map(|pipes| (plan.prefill_batches(), pipes));
        let mut main_parts: Vec<wgpu::CommandEncoder> = Vec::new();
        {
            // The enhanced sky dome was meant to cover everything, so this used to clear to
            // black on that assumption - but the dome is a hemisphere, not a full sphere, and
            // wherever the ground does not quite reach (a streamed tile not loaded yet, a gap
            // right at the horizon) that showed as a stark black void, where vanilla's plain
            // sky colour clear made the very same gap invisible. Using that same colour here
            // (unscaled - multiplying it by the enhanced exposure blew a night sky's dim clear
            // colour out to white instead) keeps a real gap from ever reading as a rendering
            // bug of its own.
            let sky = if lighting.classic && !enhanced { lighting.sky_color.map(|v| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }) } else { lighting.sky_color };
            let msaa_color = targets.as_ref().and_then(|t| t.0.as_ref());
            let depth_view: &wgpu::TextureView = match &targets {
                Some(t) => &t.1,
                None => &self.targets.ao.as_ref().unwrap().depth_view,
            };
            // the enhanced path draws into a high-range picture the post pass then grades
            let hdr = if f.masked_frame {
                self.targets.hdr_targets.get(&(width, height))
            } else {
                None
            };
            let scene_view = f.scene_view();
            let (draw_view, resolve_view): (&wgpu::TextureView, Option<&wgpu::TextureView>) =
                match hdr {
                    Some(h) => match &h.msaa_view {
                        Some(m) => (m, Some(&h.view)),
                        None => (&h.view, None),
                    },
                    None => {
                        if single {
                            (scene_view, None)
                        } else {
                            (msaa_color.expect("multisampled target"), Some(scene_view))
                        }
                    }
                };
            let pp = self.main_pass(enhanced, f.reflection_frame, f.eye_color);
            // (a mirror of an Enhanced picture, plainly shaded: the window's sky, see
            // `sky_mirror_pipeline`)
            let sky_pipe = match &self.sky.sky_mirror_pipeline {
                Some(p) if lighting.enhanced && !enhanced && !with_overlays && self.sky.sky_state.is_some() && !f.env.no_enhanced =>
                {
                    p
                }
                _ => &pp.sky_pipeline,
            };
            let per_part = main_bundles.len().div_ceil(parts.max(1));
            let sky_clear = wgpu::LoadOp::Clear(wgpu::Color { r: sky.x as f64, g: sky.y as f64, b: sky.z as f64, a: 1.0 });
            let depth_first = if share_depth { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(0.0) };
            let tail = (parts - 1) * per_part;
            let mt = MainTargets {
                draw_view,
                resolve_view,
                depth_view,
                hdr,
                pp,
                sky_pipe,
                sky_clear,
                depth_first,
                // (nothing reads the picture's own depth buffer after the pass - the
                // ambient occlusion, the fog and the rain films read the prepass's
                // texture - so unless that is the buffer drawn into, a tile-based GPU
                // need not flush a full-size (multisampled) depth buffer back)
                depth_store: if share_depth {
                    wgpu::StoreOp::Store
                } else {
                    wgpu::StoreOp::Discard
                },
                parts,
                presurface_prefill,
                prefill,
                bundles: &main_bundles[tail..],
            };
            for g in 0..parts.saturating_sub(1) {
                let first = g == 0;
                let mut part = lead.take().unwrap_or_else(|| self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("main part") }));
                // (OMSI_GPU_TIMERS: the first part timed too - the sky and the first bundles;
                // ahead of every other timed pass, it was counted nowhere)
                let stamp = if first { pass_timer(timers.set.as_ref(), &mut timers.timed, if f.with_overlays { "main part" } else { "mirror part" }) } else { None };
                self.encode_main_part(&mut part, scene, f, plan, &mt, &main_bundles[g * per_part..((g + 1) * per_part).min(main_bundles.len())], first, stamp);
                main_parts.push(part);
            }
            self.encode_main(encoder, scene, f, plan, &mt, timers);
        }
        main_parts
    }

    /// The ordinary scenery's depth laid into the main pass before the colour
    /// draws: every wall, tree and car hidden behind the one in front then fails the depth
    /// test before the enhanced shading runs for it.
    fn encode_prefill<'p>(&self, pass: &mut wgpu::RenderPass<'p>, scene: &'p Scene, f: &FrameCtx, batches: &[Batch], pipes: &'p [wgpu::RenderPipeline; 6]) {
        pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
        // Alpha-tested colour draws use alpha-to-coverage, which covers a cut-out's
        // edge in part: those samples must keep the depth of what lies behind (or the
        // sky shows through the houses behind the foliage), so a cut-out lays only the
        // depth of its fully covered inside here (`fs_depth_cover`) - the trees' crowns
        // then hide the houses and the other leaves behind them before their shading.
        if self.profiling && f.with_overlays {
            let key = if self.options.msaa > 1 { "msaa prepass batches" } else { "presurface prepass batches" };
            *self.counts.borrow_mut().entry(key).or_default() += batches.len() as f64;
        }
        // (the pipelines: plain, cut-out and transmap depth, each without and with culling)
        encode_batches(pass, scene, batches, |pipe| &pipes[pipe as usize]);
    }

    /// The sky dome (see sky.wgsl `vs_main`: it lies on the far plane and is tested against
    /// the depth there is).
    fn encode_sky<'p>(&self, pass: &mut wgpu::RenderPass<'p>, scene: &'p Scene, sky_pipe: &'p wgpu::RenderPipeline) {
        if let Some(sky) = &scene.sky_bind_group {
            pass.set_pipeline(sky_pipe);
            pass.set_bind_group(1, sky, &[]);
            pass.set_vertex_buffer(0, self.sky.sky_mesh.0.slice(..));
            pass.set_index_buffer(self.sky.sky_mesh.1.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.sky.sky_mesh.2, 0, 0..1);
        }
    }

    /// One part of a split main pass (the first one with the sky), its bundles executed.
    fn encode_main_part(&self, part: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, plan: &DrawPlan, mt: &MainTargets, bundles: &[wgpu::RenderBundle], first: bool, stamp: Option<wgpu::RenderPassTimestampWrites<'_>>) {
        let MainTargets { draw_view, depth_view, hdr, pp, sky_pipe, sky_clear, depth_first, prefill, presurface_prefill, .. } = *mt;
        let part_colors = [
            Some(wgpu::RenderPassColorAttachment {
                view: draw_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: if first { sky_clear } else { wgpu::LoadOp::Load }, store: wgpu::StoreOp::Store },
            }),
            hdr.map(|h| wgpu::RenderPassColorAttachment {
                view: h.mask_msaa.as_ref().unwrap_or(&h.mask),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: if first { wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT) } else { wgpu::LoadOp::Load }, store: wgpu::StoreOp::Store },
            }),
            hdr.filter(|_| !f.eye_color)
                .and_then(|h| h.gbuf.as_ref()).map(|g| wgpu::RenderPassColorAttachment {
                view: g[0].0.as_ref().unwrap_or(&g[0].1),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: if first { wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT) } else { wgpu::LoadOp::Load }, store: wgpu::StoreOp::Store },
            }),
            hdr.filter(|_| !f.eye_color)
                .and_then(|h| h.gbuf.as_ref()).map(|g| wgpu::RenderPassColorAttachment {
                view: g[1].0.as_ref().unwrap_or(&g[1].1),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: if first { wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT) } else { wgpu::LoadOp::Load }, store: wgpu::StoreOp::Store },
            }),
        ];
        let mut pass = part.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("main part"),
            color_attachments: if part_colors[2].is_some() { &part_colors[..] } else if part_colors[1].is_some() { &part_colors[..2] } else { &part_colors[..1] },
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations { load: if first { depth_first } else { wgpu::LoadOp::Load }, store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: stamp,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
        if first {
            let sky_late = prefill.is_some() && f.exclude_texture.is_none();
            if !sky_late {
                self.encode_sky(&mut pass, scene, sky_pipe);
            }
            // (an excavation coloured and sealed first, as `encode_main` does for an unsplit pass)
            if presurface_prefill {
                encode_batches(&mut pass, scene, &plan.main_batches[..plan.presurface_batch_end], |pipe| main_pipeline(pp, pipe));
                self.encode_prefill(&mut pass, scene, f, &plan.msaa_safe_batches, self.prepass_msaa_pipelines.as_ref().unwrap());
            }
            if let Some((batches, pipes)) = prefill {
                self.encode_prefill(&mut pass, scene, f, batches, pipes);
            }
            if sky_late {
                self.encode_sky(&mut pass, scene, sky_pipe);
            }
        }
        pass.execute_bundles(bundles.iter());
    }

    /// The main pass (the last part of a split one).
    fn encode_main(&self, encoder: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, plan: &DrawPlan, mt: &MainTargets, timers: &mut PassTimers) {
        let MainTargets { draw_view, resolve_view, depth_view, hdr, pp, sky_pipe, sky_clear, depth_first, depth_store, parts, presurface_prefill, prefill, bundles } = *mt;
        // the enhanced pass's screen mask beside the picture (see `MASK_FORMAT`)
        let mask_attachment = hdr.map(|h| wgpu::RenderPassColorAttachment {
            view: h.mask_msaa.as_ref().unwrap_or(&h.mask),
            depth_slice: None,
            resolve_target: h.mask_msaa.as_ref().map(|_| &h.mask),
            ops: wgpu::Operations {
                load: if parts > 1 { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT) },
                store: if h.mask_msaa.is_some() { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store },
            },
        });
        let main_attachment = Some(wgpu::RenderPassColorAttachment {
                view: draw_view,
                depth_slice: None,
                resolve_target: resolve_view,
                ops: wgpu::Operations {
                    load: if parts > 1 { wgpu::LoadOp::Load } else { sky_clear },
                    store: if resolve_view.is_none() {
                        wgpu::StoreOp::Store
                    } else {
                        wgpu::StoreOp::Discard
                    },
                },
            });
        let gbuf_attachments = hdr.filter(|_| !f.eye_color)
            .and_then(|h| h.gbuf.as_ref()).map(|g| {
            g.each_ref().map(|(msaa, view)| {
                Some(wgpu::RenderPassColorAttachment {
                    view: msaa.as_ref().unwrap_or(view),
                    depth_slice: None,
                    resolve_target: msaa.as_ref().map(|_| view),
                    ops: wgpu::Operations {
                        load: if parts > 1 { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT) },
                        store: if msaa.is_some() { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store },
                    },
                })
            })
        });
        let [g0, g1] = gbuf_attachments.unwrap_or([None, None]);
        let colors = [main_attachment, mask_attachment, g0, g1];
        let colors = if colors[2].is_some() { &colors[..] } else if colors[1].is_some() { &colors[..2] } else { &colors[..1] };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("main"),
            // drawn with MSAA samples and resolved into the real target at the end
            // (without multisampling straight into the target)
            color_attachments: colors,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: if parts > 1 { wgpu::LoadOp::Load } else { depth_first },
                    store: depth_store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: pass_timer(
                timers.set.as_ref(),
                &mut timers.timed,
                if f.with_overlays { "main" } else { "mirror" },
            ),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
        // The sky after the prefill: over the laid depth it shades only the pixels no
        // scenery covers (an excavation's floor is coloured before its prefill, so there it
        // goes first, over the cleared depth, as it always did; and where a texture is left
        // out of the picture its depth is still in the prefill, with the sky to show there).
        let prefill = prefill.filter(|_| parts == 1);
        let sky_late = prefill.is_some() && f.exclude_texture.is_none();
        if parts == 1 && !sky_late {
            self.encode_sky(&mut pass, scene, sky_pipe);
        }
        if presurface_prefill && parts == 1 {
            // Colour and seal the excavation first; prefill ordinary scenery without
            // storing and reloading the colour attachments in a separate pass.
            encode_batches(&mut pass, scene, &plan.main_batches[..plan.presurface_batch_end], |pipe| main_pipeline(pp, pipe));
            // At 1x an excavation still prevents sharing the earlier prepass depth.
            // Refill ordinary scenery here, after its floor and cover have been coloured.
            let pipes = self.prefill_pipelines(f.eye_color).unwrap();
            self.encode_prefill(&mut pass, scene, f, &plan.msaa_safe_batches, pipes);
        } else if let Some((batches, pipes)) = prefill {
            self.encode_prefill(&mut pass, scene, f, batches, pipes);
            if sky_late {
                self.encode_sky(&mut pass, scene, sky_pipe);
            }
        }
        if bundles.is_empty() {
            pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
            encode_batches(&mut pass, scene, if presurface_prefill { &plan.main_batches[plan.presurface_batch_end..] } else { &plan.main_batches }, |pipe| {
                main_pipeline(pp, pipe)
            });
        } else {
            // the batches, recorded as bundles on several threads (see `record_bundles`)
            pass.execute_bundles(bundles.iter());
            // a bundle leaves the pass without bind groups
            pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
        }
        self.encode_main_late(&mut pass, scene, f, plan, pp);
    }

    /// What the main pass draws after the batches: the smoke, the snowfall, the coronas
    /// with the cab, the HUD.
    fn encode_main_late<'p>(&self, pass: &mut wgpu::RenderPass<'p>, scene: &'p Scene, f: &FrameCtx, plan: &DrawPlan, pp: &'p PassPipelines) {
        let lighting = f.lighting;
        let cab_batches = &plan.cab_batches;
        let overlays = &f.overlays;
        // (a mirror's small picture leaves the smoke and the snowfall out: 150 000 flakes
        // in each of a frame's mirrors were the costliest part of a mirror in a snowfall,
        // for a few pixels of them; see also `prepare_smoke` in `begin_frame`)
        let particles = f.exclude_texture.is_none();
        // (the smoke a mirror is given is the window's of the frame before: the mirrors are drawn
        // ahead of the picture, and `prepare_smoke` is not run for them)
        let smoke = particles || self.options.mirror_particles;
        // smoke, blended over the scene
        if smoke && scene.smoke_count > 0 && !f.env.no_smoke {
            if let Some(sb) = &scene.smoke_buf {
                pass.set_pipeline(&pp.smoke_pipeline);
                pass.set_bind_group(1, &self.lights.smoke_bind_group, &[]);
                pass.set_vertex_buffer(0, sb.slice(..));
                pass.draw(0..6, 0..scene.smoke_count);
            }
        }
        // the snowfall, every flake worked out on the GPU (snow.wgsl), over the world and
        // under the cab of the vehicle the camera is in
        if let Some(snow) = pp.snow_pipeline.as_ref().filter(|_| particles && lighting.snowfall > 0.01 && !f.env.no_snowfall) {
            let (counts, mean) = snowfall_flakes(lighting.snowfall);
            let u = SnowUniform {
                wind: lighting.wind.extend(self.started.elapsed().as_secs_f32() % 20000.0).to_array(),
                fall: [lighting.snowfall, mean, counts[0] as f32, counts[1] as f32],
                roof: self.shadows.precip.uniform().0,
                roof_on: [self.shadows.precip.uniform().1, 0.0, 0.0, 0.0],
            };
            self.queue.write_buffer(&self.lights.snow_buf, 0, bytemuck::bytes_of(&u));
            pass.set_pipeline(snow);
            pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
            pass.set_bind_group(1, &self.lights.snow_bind_group, &[]);
            pass.draw(0..6, 0..counts.iter().sum::<u32>());
        }
        // the rainfall, as the snowfall (rain.wgsl)
        let rf = scene.rainfall;
        if let Some(rain) = pp.rain_fall_pipeline.as_ref().filter(|_| particles && rf.rate > 0.001 && lighting.snowfall <= 0.01 && !f.env.no_rain) {
            let counts = rainfall_drops(rf.rate, rf.quality);
            let u = RainUniform {
                wind: rf.wind.extend(self.started.elapsed().as_secs_f32() % 20000.0).to_array(),
                // (the eye's time: a seventieth of a second's way of the drop, a streak of
                // some ten centimetres in a downpour)
                eye: rf.eye_velocity.extend(1.0 / 70.0).to_array(),
                fall: [rf.rate, rf.drop_mm * 0.001, counts[0] as f32, counts[1] as f32],
                view: [f.width as f32, f.height as f32, 0.85, (counts[0] + counts[1] + counts[2]) as f32],
                roof: self.shadows.precip.uniform().0,
                roof_on: [self.shadows.precip.uniform().1, 0.0, 0.0, 0.0],
            };
            self.queue.write_buffer(&self.lights.rain_buf, 0, bytemuck::bytes_of(&u));
            pass.set_pipeline(rain);
            pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
            pass.set_bind_group(1, &self.lights.rain_bind_group, &[]);
            pass.draw(0..4, 0..counts.iter().sum::<u32>());
        }
        // light coronas, additive: the world's, then the vehicle the camera is in -
        // drawn over them as Omsi.exe draws it last (see `cab_items`) - then its own
        let coronas_on = scene.corona_count > 0 && !f.env.no_coronas;
        for late in [false, true] {
            if late && !cab_batches.is_empty() {
                pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
                encode_batches(pass, scene, cab_batches, |pipe| main_pipeline(pp, pipe));
            }
            if let Some(cb) = scene.corona_buf.as_ref().filter(|_| coronas_on) {
                pass.set_pipeline(&pp.corona_pipeline);
                pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
                pass.set_vertex_buffer(0, cb.slice(..));
                // in runs by picture (the standard glow, the lights' own bitmaps, the cone)
                for &(tex, first, count, _) in scene.corona_runs.iter().filter(|r| r.3 == late) {
                    let bg = self.lights.corona_textures.get(tex as usize).and_then(|b| b.as_ref()).unwrap_or(&self.lights.corona_bind_group);
                    pass.set_bind_group(1, bg, &[]);
                    pass.draw(0..6, first..first + count);
                }
            }
        }
        // HUD overlays (on the vanilla path at full size; the enhanced path draws them
        // after grading, a scaled picture after scaling it up)
        if !overlays.is_empty() && !f.masked_frame && !f.scaled {
            pass.set_pipeline(&self.overlay_pipeline);
            for (k, _) in overlays.iter().enumerate() {
                if let Some((_, _, bg, _)) = scene.overlay_res.get(k) {
                    pass.set_bind_group(0, bg, &[]);
                    pass.draw(0..6, 0..1);
                }
            }
        }
    }
}
