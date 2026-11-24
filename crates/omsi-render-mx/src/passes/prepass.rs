//! The passes of the prepass command buffer: the depth prepass with the ambient
//! occlusion, and the enhanced path's sky cube and reflection probe.

use super::*;

impl Renderer {
    // --- depth prepass + ambient occlusion (single-sampled, camera projection)
    pub(crate) fn encode_depth_prepass(
        &self,
        prepass_encoder: &mut mx::CommandEncoder,
        scene: &Scene,
        f: &FrameCtx,
        prepass_batches: &[Batch],
        timers: &mut PassTimers,
    ) {
        let (camera, aspect, width, height, ao_on) =
            (f.camera, f.aspect, f.width, f.height, f.ao_on);
        if f.prepass_on {
            let proj = f.projection.unwrap_or_else(|| {
                Mat4::perspective_rh(
                    camera.fov_deg.to_radians(),
                    aspect,
                    camera.depth_far(),
                    camera.near,
                )
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
                wide: if f.enhanced_frame {
                    occlusion_tuning(self.street_light())
                } else {
                    [0.0; 4]
                },
                // (the window's own view only: another eye or panel is not where it was)
                reproj: self.tone.bounce_reproj.0,
                history: [
                    if f.lead_view {
                        self.tone.bounce_reproj.1
                    } else {
                        0.0
                    },
                    0.0,
                    0.0,
                    0.0,
                ],
            };
            self.queue
                .write_buffer(&self.ao_buf, 0, bytemuck::bytes_of(&u))
                .expect("mX: a queue write");
            let ao = self.targets.ao.as_ref().unwrap();
            // temporal upscaling: the same depth, and each pixel's motion written beside it
            let motion = if f.temporal_frame {
                self.temporal_pipes
                    .as_ref()
                    .zip(self.temporal.as_ref())
                    .and_then(|(p, d)| Some((p, d.targets.as_ref()?, &d.prev_models.as_ref()?.1)))
            } else {
                None
            };
            let motion_target = [motion.map(|(_, t, _)| mx::RenderPassColorAttachment {
                view: &t.motion_view,
                resolve_target: None,
                ops: mx::Operations {
                    load: mx::LoadOp::Clear(mx::Color::TRANSPARENT),
                    store: mx::StoreOp::Store,
                },
            })];
            {
                let mut pass = prepass_encoder.begin_render_pass(&mx::RenderPassDescriptor {
                    label: Some("depth prepass"),
                    color_attachments: if motion.is_some() {
                        &motion_target[..]
                    } else {
                        &[]
                    },
                    depth_stencil_attachment: Some(mx::RenderPassDepthStencilAttachment {
                        view: &ao.depth_view,
                        depth_ops: Some(mx::Operations {
                            load: mx::LoadOp::Clear(0.0),
                            store: mx::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: pass_timer(
                        timers.set.as_ref(),
                        &mut timers.timed,
                        "depth prepass",
                    ),
                    occlusion_query_set: None,
                });
                if let Some((p, _, bg)) = motion {
                    // (the sky's motion first, where nothing covers it)
                    pass.set_bind_group(3, bg);
                    pass.set_pipeline(&p.sky);
                    pass.draw(0..3, 0..1);
                }
                encode_batches(&mut pass, scene, prepass_batches, |pipe| match motion {
                    Some((p, _, _)) => &p.prepass[pipe as usize],
                    None => &self.prepass_pipelines[pipe as usize],
                });
            }
            for (pipe, bg, target, pass_label) in [
                (
                    if ao.bounce {
                        &self.ssao_pipeline
                    } else {
                        &self.plain_ssao_pipeline
                    },
                    &ao.ssao_bg,
                    &ao.ao_view,
                    "ssao",
                ),
                (
                    if ao.bounce {
                        &self.blur_pipeline
                    } else {
                        &self.plain_blur_pipeline
                    },
                    &ao.blur_bg,
                    &ao.blur_view,
                    "ssao blur",
                ),
            ] {
                let Some(pipe) = pipe.as_ref().filter(|_| ao_on) else {
                    break;
                };
                let mut pass = prepass_encoder.begin_render_pass(&mx::RenderPassDescriptor {
                    label: Some("ssao"),
                    color_attachments: &[Some(mx::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        ops: mx::Operations {
                            load: mx::LoadOp::Clear(mx::Color::WHITE),
                            store: mx::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: pass_timer(
                        timers.set.as_ref(),
                        &mut timers.timed,
                        pass_label,
                    ),
                    occlusion_query_set: None,
                });
                pass.set_pipeline(pipe);
                bg.set(&mut pass);
                pass.draw(0..3, 0..1);
            }
        }
    }

    // --- the enhanced sky cube: a slice of a face a frame (all six faces the first time and
    // for a new sky), drawn in the window's frame only (see `SkySched`)
    pub(crate) fn encode_sky_cube(
        &mut self,
        prepass_encoder: &mut mx::CommandEncoder,
        scene: &Scene,
        f: &FrameCtx,
        probe_redraw: bool,
        timers: &mut PassTimers,
    ) {
        let (enhanced, lead_view) = (f.enhanced, f.lead_view);
        if !(enhanced && (lead_view || probe_redraw)) {
            return;
        }
        let (Some(probe), Some(sky_bg)) = (self.tone.probe.as_mut(), scene.sky_bind_group.as_ref())
        else {
            return;
        };
        // (face, round, the old picture's share, rows): a whole new cube is every round of
        // every face averaged; afterwards a slice or two a frame, blended in
        let full = !probe.cube_filled || self.instant_exposure;
        let mut draws: Vec<(u32, u32, f64, (u32, u32))> = Vec::new();
        if full {
            draws.extend((0..6).flat_map(|f| {
                (0..SKY_CUBE_ROUNDS)
                    .map(move |r| (f, r, r as f64 / (r as f64 + 1.0), (0, SKY_CUBE_SIZE)))
            }));
        } else if lead_view {
            let round = (probe.cube_round / 6) % SKY_CUBE_ROUNDS;
            let (mut next, mut rounds) = (probe.cube_next, probe.cube_round);
            for s in probe
                .sched
                .plan(
                    &mut next,
                    &mut rounds,
                    if f.env.sky_cube_every_frame {
                        SLICE_EVERY
                    } else {
                        f.dt
                    },
                )
                .into_iter()
                .flatten()
            {
                draws.push((
                    s.face,
                    round,
                    SKY_CUBE_HISTORY,
                    slice_rows(s.first, s.count),
                ));
            }
            (probe.cube_next, probe.cube_round) = (next, rounds);
        } else {
            // (a view of its own before the window's first frame: a whole face)
            let round = (probe.cube_round / 6) % SKY_CUBE_ROUNDS;
            draws.push((probe.cube_next, round, SKY_CUBE_HISTORY, (0, SKY_CUBE_SIZE)));
            probe.cube_next = (probe.cube_next + 1) % 6;
            probe.cube_round = probe.cube_round.wrapping_add(1);
        }
        probe.cube_filled = true;
        let single = draws.len() == 1;
        for (f, round, history, (y0, rows)) in draws {
            let whole = rows == SKY_CUBE_SIZE;
            let view = &probe.cube_faces[f as usize];
            let mut pass = prepass_encoder.begin_render_pass(&mx::RenderPassDescriptor {
                label: Some("sky cube"),
                color_attachments: &[Some(mx::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: mx::Operations {
                        // (a slice keeps the rest of its face)
                        load: if history > 0.0 || !whole {
                            mx::LoadOp::Load
                        } else {
                            mx::LoadOp::Clear(mx::Color::BLACK)
                        },
                        store: mx::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: if single {
                    pass_timer(timers.set.as_ref(), &mut timers.timed, "sky cube")
                } else {
                    None
                },
                occlusion_query_set: None,
            });
            pass.set_pipeline(&probe.cube_pipeline);
            pass.set_blend_constant(mx::Color {
                r: history,
                g: history,
                b: history,
                a: history,
            });
            if !whole {
                pass.set_scissor_rect(0, y0, SKY_CUBE_SIZE, rows);
            }
            probe.cube_bind_groups[(f * SKY_CUBE_ROUNDS + round) as usize].set(&mut pass);
            sky_bg.set(&mut pass);
            pass.draw(0..3, 0..1);
        }
    }

    // --- the reflection probe of the enhanced path: the sky into the six faces, then
    // each blurrier level from the sharper ones (three faces a pass)
    pub(crate) fn encode_probe(
        &self,
        prepass_encoder: &mut mx::CommandEncoder,
        scene: &Scene,
        probe_redraw: bool,
        timers: &mut PassTimers,
    ) {
        if probe_redraw {
            if let (Some(probe), Some(sky_bg)) =
                (self.tone.probe.as_ref(), scene.sky_bind_group.as_ref())
            {
                for (m, faces) in probe.faces.iter().enumerate() {
                    for (half, bg) in probe.bind_groups[m].iter().enumerate() {
                        let attachments: Vec<Option<mx::RenderPassColorAttachment>> = faces
                            [half * 3..half * 3 + 3]
                            .iter()
                            .map(|v| {
                                Some(mx::RenderPassColorAttachment {
                                    view: v,
                                    resolve_target: None,
                                    ops: mx::Operations {
                                        load: mx::LoadOp::Clear(mx::Color::BLACK),
                                        store: mx::StoreOp::Store,
                                    },
                                })
                            })
                            .collect();
                        let mut pass =
                            prepass_encoder.begin_render_pass(&mx::RenderPassDescriptor {
                                label: Some("reflection probe"),
                                color_attachments: &attachments,
                                depth_stencil_attachment: None,
                                timestamp_writes: if m == 0 && half == 0 {
                                    pass_timer(timers.set.as_ref(), &mut timers.timed, "probe")
                                } else {
                                    None
                                },
                                occlusion_query_set: None,
                            });
                        pass.set_pipeline(if m == 0 {
                            &probe.sky_pipeline
                        } else {
                            &probe.filter_pipeline
                        });
                        bg.set(&mut pass);
                        sky_bg.set(&mut pass);
                        pass.draw(0..3, 0..1);
                    }
                }
            }
        }
    }
}
