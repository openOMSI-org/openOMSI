//! The passes of the prepass command buffer: the depth prepass with the ambient
//! occlusion, and the enhanced path's sky cube and reflection probe.

use super::*;

impl Renderer {
    // --- depth prepass + ambient occlusion (single-sampled, camera projection)
    pub(crate) fn encode_depth_prepass(&self, prepass_encoder: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, prepass_batches: &[Batch], timers: &mut PassTimers) {
        let (camera, aspect, width, height, ao_on) = (f.camera, f.aspect, f.width, f.height, f.ao_on);
        if f.prepass_on {
            let proj = f.projection.unwrap_or_else(|| {
                Mat4::perspective_rh(camera.fov_deg.to_radians(), aspect, camera.far, camera.near)
            });
            let u = SsaoUniform {
                inv_proj: proj.inverse().to_cols_array_2d(),
                params: [
                    1.0,
                    1.4,
                    width.div_ceil(2) as f32,
                    height.div_ceil(2) as f32,
                ],
                shift: [proj.z_axis.x, proj.z_axis.y, 0.0, 0.0],
            };
            self.queue
                .write_buffer(&self.ao_buf, 0, bytemuck::bytes_of(&u));
            let ao = self.targets.ao.as_ref().unwrap();
            {
                let mut pass = prepass_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("depth prepass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &ao.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: pass_timer(timers.set.as_ref(), &mut timers.timed, "depth prepass"),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
                encode_batches(&mut pass, scene, prepass_batches, |pipe| {
                    &self.prepass_pipelines[pipe as usize]
                });
            }
            for (pipe, bg, target, pass_label) in [
                (&self.ssao_pipeline, &ao.ssao_bg, &ao.ao_view, "ssao"),
                (&self.blur_pipeline, &ao.blur_bg, &ao.blur_view, "ssao blur"),
            ] {
                let Some(pipe) = pipe.as_ref().filter(|_| ao_on) else {
                    break;
                };
                let mut pass = prepass_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ssao"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: pass_timer(timers.set.as_ref(), &mut timers.timed, pass_label),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(pipe);
                pass.set_bind_group(0, bg, &[]);
                pass.draw(0..3, 0..1);
            }
        }
    }

    // --- the enhanced sky cube: a slice of a face a frame (all six faces the first time and
    // for a new sky), drawn in the window's frame only (see `SkySched`)
    pub(crate) fn encode_sky_cube(&mut self, prepass_encoder: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, probe_redraw: bool, timers: &mut PassTimers) {
        let (enhanced, lead_view) = (f.enhanced, f.lead_view);
        if !(enhanced && (lead_view || probe_redraw)) {
            return;
        }
        let (Some(probe), Some(sky_bg)) = (self.tone.probe.as_mut(), scene.sky_bind_group.as_ref()) else {
            return;
        };
        // (face, round, the old picture's share, rows, into the cube for the next eye): a
        // whole new cube is every round of every face averaged; afterwards a slice or two
        // a frame, blended in
        let full = !probe.cube_filled || self.instant_exposure;
        let mut draws: Vec<(u32, u32, f64, (u32, u32), bool)> = Vec::new();
        if full {
            probe.sched.cancel_staging();
            draws.extend((0..6).flat_map(|f| (0..SKY_CUBE_ROUNDS).map(move |r| (f, r, r as f64 / (r as f64 + 1.0), (0, SKY_CUBE_SIZE), false))));
        } else {
            if std::mem::take(&mut probe.sched.swap) {
                // the cube filled for the eye the frame's uniform has: over the one in use
                if let Some((staging, _)) = probe.sched.staging.as_ref() {
                    prepass_encoder.copy_texture_to_texture(
                        staging.as_image_copy(),
                        probe.cube_tex.as_image_copy(),
                        wgpu::Extent3d { width: SKY_CUBE_SIZE, height: SKY_CUBE_SIZE, depth_or_array_layers: 6 },
                    );
                }
            }
            if lead_view {
                let round = (probe.cube_round / 6) % SKY_CUBE_ROUNDS;
                let (mut next, mut rounds) = (probe.cube_next, probe.cube_round);
                for s in probe.sched.plan(&mut next, &mut rounds, if f.env.sky_cube_every_frame { SLICE_EVERY } else { f.dt }).into_iter().flatten() {
                    let history = if s.staging { 0.0 } else { SKY_CUBE_HISTORY };
                    draws.push((s.face, round, history, slice_rows(s.first, s.count), s.staging));
                }
                (probe.cube_next, probe.cube_round) = (next, rounds);
            } else {
                // (a view of its own before the window's first frame: a whole face)
                let round = (probe.cube_round / 6) % SKY_CUBE_ROUNDS;
                draws.push((probe.cube_next, round, SKY_CUBE_HISTORY, (0, SKY_CUBE_SIZE), false));
                probe.cube_next = (probe.cube_next + 1) % 6;
                probe.cube_round = probe.cube_round.wrapping_add(1);
            }
        }
        probe.cube_filled = true;
        if draws.iter().any(|d| d.4) && probe.sched.staging.is_none() {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("sky cube, next eye"),
                size: wgpu::Extent3d { width: SKY_CUBE_SIZE, height: SKY_CUBE_SIZE, depth_or_array_layers: 6 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: probe.cube_tex.format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let faces = (0..6)
                .map(|f| {
                    tex.create_view(&wgpu::TextureViewDescriptor {
                        label: Some("sky cube face, next eye"),
                        dimension: Some(wgpu::TextureViewDimension::D2),
                        base_array_layer: f,
                        array_layer_count: Some(1),
                        mip_level_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect();
            probe.sched.staging = Some((tex, faces));
        }
        let single = draws.len() == 1;
        for (f, round, history, (y0, rows), staging) in draws {
            let whole = rows == SKY_CUBE_SIZE;
            let view = match (staging, probe.sched.staging.as_ref()) {
                (true, Some((_, faces))) => &faces[f as usize],
                _ => &probe.cube_faces[f as usize],
            };
            let mut pass = prepass_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sky cube"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // (a slice keeps the rest of its face)
                        load: if history > 0.0 || !whole { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::BLACK) },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: if single { pass_timer(timers.set.as_ref(), &mut timers.timed, "sky cube") } else { None },
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&probe.cube_pipeline);
            pass.set_blend_constant(wgpu::Color { r: history, g: history, b: history, a: history });
            if !whole {
                pass.set_scissor_rect(0, y0, SKY_CUBE_SIZE, rows);
            }
            pass.set_bind_group(0, &probe.cube_bind_groups[(f * SKY_CUBE_ROUNDS + round) as usize], &[]);
            pass.set_bind_group(1, sky_bg, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    // --- the reflection probe of the enhanced path: the sky into the six faces, then
    // each blurrier level from the sharper ones (three faces a pass)
    pub(crate) fn encode_probe(&self, prepass_encoder: &mut wgpu::CommandEncoder, scene: &Scene, probe_redraw: bool, timers: &mut PassTimers) {
        if probe_redraw {
            if let (Some(probe), Some(sky_bg)) =
                (self.tone.probe.as_ref(), scene.sky_bind_group.as_ref())
            {
                for (m, faces) in probe.faces.iter().enumerate() {
                    for (half, bg) in probe.bind_groups[m].iter().enumerate() {
                        let attachments: Vec<Option<wgpu::RenderPassColorAttachment>> = faces
                            [half * 3..half * 3 + 3]
                            .iter()
                            .map(|v| {
                                Some(wgpu::RenderPassColorAttachment {
                                    view: v,
                                    depth_slice: None,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                        store: wgpu::StoreOp::Store,
                                    },
                                })
                            })
                            .collect();
                        let mut pass =
                            prepass_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("reflection probe"),
                                color_attachments: &attachments,
                                depth_stencil_attachment: None,
                                timestamp_writes: if m == 0 && half == 0 {
                                    pass_timer(timers.set.as_ref(), &mut timers.timed, "probe")
                                } else {
                                    None
                                },
                                occlusion_query_set: None,
                                multiview_mask: None,
                            });
                        pass.set_pipeline(if m == 0 {
                            &probe.sky_pipeline
                        } else {
                            &probe.filter_pipeline
                        });
                        pass.set_bind_group(0, bg, &[]);
                        pass.set_bind_group(1, sky_bg, &[]);
                        pass.draw(0..3, 0..1);
                    }
                }
            }
        }
    }
}
