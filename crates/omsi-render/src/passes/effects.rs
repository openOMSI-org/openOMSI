//! The passes over the finished main picture: the reflections (traced or the puddles'),
//! the lamps' light in the fog, the rain films on the panes and the classic graphics'
//! reflections presented.

use super::*;

impl Renderer {
    /// The traced reflections (Enhanced+) or the puddles'; true when the puddles' picture
    /// was drawn this frame.
    pub(crate) fn encode_scene_reflections(&mut self, encoder: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, plan: &DrawPlan, cu: &CameraUniform, timers: &mut PassTimers) -> bool {
        let (width, height, camera, lighting, rt_frame) = (f.width, f.height, f.camera, f.lighting, f.rt_frame);
        let (main_batches, cab_batches) = (&plan.main_batches, &plan.cab_batches);
        // Weather alone is insufficient: leave the allocation and reflection passes out when
        // the visible batches contain no moisture-tagged surface (a showroom, bare terrain).
        // Enhanced+: the traced reflections (wet roads' too) in place of the puddles' rays
        if rt_frame {
            self.encode_rt_reflections(encoder, width, height, timers.set.as_ref(), &mut timers.timed);
        }
        let puddles_on = f.puddles_wanted
            && !rt_frame
            && main_batches.iter().any(|b| scene.materials[b.material as usize].uniform.params2[2] > 0.0)
            && self.prepare_puddle_reflections(width, height, camera, f.aspect, f.projection, cu, lighting, f.ro);
        if puddles_on {
            // (the vehicle the camera is in is drawn into the puddles' picture as well)
            let all: Vec<Batch>;
            let batches = if cab_batches.is_empty() {
                main_batches
            } else {
                all = main_batches.iter().chain(cab_batches).cloned().collect();
                &all
            };
            self.encode_puddle_reflections(encoder, width, height, scene, batches, &plan.list, lighting, camera, timers.set.as_ref(), &mut timers.timed);
        }
        puddles_on
    }

    // the lamps' light in the weather's fog and the sun's shafts between the shadows
    // (before the rain on the panes, which writes the prepass depth),
    // over the picture before its glow and metering (the window's picture; the mirrors
    // go without)
    pub(crate) fn encode_fog_lamps(&self, encoder: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, vp_mat: Mat4, puddles_on: bool, timers: &mut PassTimers) {
        let (width, height, lighting) = (f.width, f.height, f.lighting);
        let (enhanced, prepass_on, with_overlays, masked_frame) = (f.enhanced, f.prepass_on, f.with_overlays, f.masked_frame);
        let fog_lamps = enhanced_weather_fog(lighting) >= 2e-4 && !scene.lights.is_empty();
        let shafts = lighting.shadows && lighting.sun_dir.z > 0.0 && self.shafts_seen(vp_mat, lighting.sun_dir);
        // (the glare goes by how much of the sun is seen, which is none with the sun behind
        // the camera - fog_lamps.wgsl `sun_seen`: then there is no glare to add)
        // (the veil reaches 100 degrees round the sun - `sun_glare` - so the pass runs while
        // any of the picture is within that, not only while the sun is in front of the camera)
        let sun_ahead = nearest_ray_cos(vp_mat, lighting.sun_dir) > (100.0f32).to_radians().cos();
        let glare = lighting.sun_dir.z > -0.01 && lighting.sun_intensity > 0.0 && !f.env.no_glare && sun_ahead;
        if f.env.debug_fog_lamps {
            log::info!("fog lamps: enhanced {enhanced} prepass {prepass_on} overlays {with_overlays} masked {masked_frame} fog {:.5} lights {} shafts {shafts} glare {glare}", enhanced_weather_fog(lighting), scene.lights.len());
        }
        if enhanced && prepass_on && with_overlays && masked_frame && (fog_lamps || shafts || glare) {
            if let (Some(pipes), Some(ao), Some(cam_bg)) = (self.lights.fog_lamps_pipeline.as_ref(), self.targets.ao.as_ref(), scene.camera_bind_group.as_ref()) {
                if let Some(fog_bg) = ao.fog_bg.as_ref() {
                    // (how much of the weather's extinction is mist and fog - droplets of some
                    // ten micrometres, which scatter a lamp's light into a halo and its beam
                    // into a cone - rather than rain: a raindrop of a millimetre sends what it
                    // scatters on within a few hundredths of a degree, no halo round a lamp,
                    // no cone of a headlight in a drizzle (the rain's share as lights.rs
                    // `apply_weather` lays it on))
                    let fog_all = enhanced_weather_fog(lighting);
                    let rain_part = if lighting.rain > 0.0 && lighting.snowfall <= 0.0 { 2.3 / (2500.0 - 1800.0 * lighting.rain.clamp(0.0, 1.0)) } else { 0.0 };
                    let droplets = if fog_all > 0.0 { ((fog_all - rain_part) / fog_all).clamp(0.0, 1.0) } else { 0.0 };
                    // (z: 1 the sun's glare is added, 2 it is all there is to add - no lamps in
                    // the fog, no shafts to see - and the half-size pass works out only the
                    // row with how much of the sun is seen)
                    let glare_only = !fog_lamps && !shafts;
                    let u = FogLampUniform { inv_view_proj: vp_mat.inverse().to_cols_array_2d(), size: [width as f32, height as f32, if glare_only { 2.0 } else if glare { 1.0 } else { 0.0 }, droplets] };
                    self.queue.write_buffer(&self.lights.fog_lamps_buf, 0, bytemuck::bytes_of(&u));
                    let h = &self.targets.hdr_targets[&(width, height)];
                    let view = h.puddles.as_ref().filter(|_| puddles_on).map_or(&h.view, |p| &p.view);
                    for (k, (target, load)) in [(&ao.fog_view, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)), (view, wgpu::LoadOp::Load)].into_iter().enumerate() {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("fog lamps"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: target, depth_slice: None, resolve_target: None,
                                ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: if k == 1 { pass_timer(timers.set.as_ref(), &mut timers.timed, "fog lamps") } else { None },
                            occlusion_query_set: None,
                            multiview_mask: None,
                        });
                        if k == 0 && glare_only {
                            let size = ao.fog_view.texture().size();
                            pass.set_scissor_rect(0, size.height - 1, 1, 1);
                        }
                        pass.set_pipeline(&pipes[k]);
                        pass.set_bind_group(0, cam_bg, &[]);
                        pass.set_bind_group(1, &fog_bg[k], &[]);
                        pass.draw(0..3, 0..1);
                    }
                }
            }
        }
    }

    /// Whether the sun's shafts can show anywhere in the picture: the shader's own test
    /// (fog_lamps.wgsl `sun_shafts`, `most`) for the ray nearest the sun. Looking away from
    /// the sun, in clear air, they never can, and the half-size pass and the full-size
    /// gather over its result are left out (the glare alone needs neither).
    fn shafts_seen(&self, vp_mat: Mat4, sun_dir: Vec3) -> bool {
        let u = &self.tone.enh_last;
        let e_sun = u.sun[0].max(u.sun[1]);
        // (the uniform of the frame: zero before the first enhanced frame)
        if u.exposure[0] <= 0.0 {
            return true;
        }
        if e_sun < 1e-4 {
            return false;
        }
        let cos_t = nearest_ray_cos(vp_mat, sun_dir);
        let hg = |c: f32, g: f32| (1.0 - g * g) / (4.0 * std::f32::consts::PI * (1.0 + g * g - 2.0 * g * c).max(1e-4).powf(1.5));
        let most = (u.fog[0] * hg(cos_t, 0.55) * 0.9 + u.fog[3] * hg(cos_t, 0.7)) * e_sun * 180.0 * u.exposure[0];
        // (180 m: the shader's SHAFT_REACH; with a margin under its 0.004)
        most >= 0.002
    }

    /// The rain films on the panes, over a copy of the clean current picture.
    pub(crate) fn encode_rain_film(&self, encoder: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, rain_batches: &[Batch], puddles_on: bool) {
        let (width, height, masked_frame) = (f.width, f.height, f.masked_frame);
        let scene_view = f.scene_view();
        if f.glass_on {
            let hdr = masked_frame.then(|| &self.targets.hdr_targets[&(width, height)]);
            let view = hdr.map_or(scene_view, |h| h.puddles.as_ref().filter(|_| puddles_on).map_or(&h.view, |p| &p.view));
            let behind = self.targets.glass_picture.as_ref().unwrap();
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo { texture: view.texture(), mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyTextureInfo { texture: behind.texture(), mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            );
            let colours = [
                Some(wgpu::RenderPassColorAttachment {
                    view, depth_slice: None, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                }),
                hdr.map(|h| wgpu::RenderPassColorAttachment {
                    view: &h.mask, depth_slice: None, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                }),
                hdr.filter(|_| !f.eye_color).and_then(|h| h.gbuf.as_ref()).map(|g| wgpu::RenderPassColorAttachment {
                    view: &g[0].1, depth_slice: None, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                }),
                hdr.filter(|_| !f.eye_color).and_then(|h| h.gbuf.as_ref()).map(|g| wgpu::RenderPassColorAttachment {
                    view: &g[1].1, depth_slice: None, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                }),
            ];
            let pipes = &self.main_pass(f.enhanced, f.reflection_frame, f.eye_color).rain_pipelines;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rain on current scene"),
                color_attachments: &colours[..if colours[2].is_some() { 4 } else if hdr.is_some() { 2 } else { 1 }],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.ao.as_ref().unwrap().depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
            });
            pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
            encode_batches(&mut pass, scene, rain_batches, |pipe| &pipes[pipe as usize]);
        }
    }

    /// The classic graphics' picture with the puddles' reflections, presented (and the
    /// HUD over it at full size).
    pub(crate) fn encode_classic_present(&self, encoder: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, puddles_on: bool) {
        let (width, height) = (f.width, f.height);
        let overlays = &f.overlays;
        if f.reflection_frame {
            let h = &self.targets.hdr_targets[&(width, height)];
            let bg = h.puddles.as_ref().filter(|_| puddles_on).map_or(&h.classic_bg, |p| &p.classic_bg);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("classic reflections present"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: f.scene_view(), depth_slice: None, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None, timestamp_writes: None,
                occlusion_query_set: None, multiview_mask: None,
            });
            pass.set_pipeline(&self.scale.copy_pipeline);
            pass.set_bind_group(0, bg, &[]);
            pass.draw(0..3, 0..1);
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
}

/// The largest cosine between `dir` and any of the picture's rays (at most): the
/// picture's rays lie in a cone round its middle one, as wide as its corners are off it.
pub(crate) fn nearest_ray_cos(vp_mat: Mat4, dir: Vec3) -> f32 {
    let inv = vp_mat.inverse();
    let ray = |x: f32, y: f32| {
        let near = inv.project_point3(Vec3::new(x, y, 1.0));
        let mid = inv.project_point3(Vec3::new(x, y, 0.5));
        (mid - near).normalize_or_zero()
    };
    let centre = ray(0.0, 0.0);
    let spread = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .iter()
        .map(|&(x, y)| centre.dot(ray(x, y)).clamp(-1.0, 1.0).acos())
        .fold(0.0f32, f32::max);
    let off = centre.dot(dir.normalize_or_zero()).clamp(-1.0, 1.0).acos();
    (off - spread).max(0.0).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view_proj() -> Mat4 {
        // looking along +y from 2 m up, 60 degrees high, 16:9, reversed Z as the game has it
        let proj = Mat4::perspective_rh(60f32.to_radians(), 16.0 / 9.0, 3000.0, 0.1);
        proj * Mat4::look_at_rh(Vec3::new(0.0, 0.0, 2.0), Vec3::new(0.0, 1.0, 2.0), Vec3::Z)
    }

    #[test]
    fn the_glare_is_drawn_while_any_of_the_picture_is_within_its_reach() {
        // (100 degrees round the sun: a sun just behind the camera's plane still veils the
        // picture's edges; one straight behind it does not)
        let vp = view_proj();
        let reach = (100.0f32).to_radians().cos();
        assert!(nearest_ray_cos(vp, Vec3::new(0.0, 1.0, 0.3)) > reach);
        assert!(nearest_ray_cos(vp, Vec3::new(1.0, -0.2, 0.2)) > reach);
        assert!(nearest_ray_cos(vp, Vec3::new(0.0, -1.0, 0.1)) <= reach);
    }

    #[test]
    fn the_nearest_ray_bounds_every_pixel_of_the_picture() {
        let vp = view_proj();
        let inv = vp.inverse();
        for sun in [Vec3::new(0.0, 1.0, 0.2), Vec3::new(1.0, 0.3, 0.5), Vec3::new(0.2, -1.0, 0.4), Vec3::new(-0.6, 0.5, 0.9)] {
            let bound = nearest_ray_cos(vp, sun);
            let s = sun.normalize();
            for i in 0..=20 {
                for j in 0..=20 {
                    let (x, y) = (i as f32 / 10.0 - 1.0, j as f32 / 10.0 - 1.0);
                    let d = (inv.project_point3(Vec3::new(x, y, 0.5)) - inv.project_point3(Vec3::new(x, y, 1.0))).normalize();
                    assert!(d.dot(s) <= bound + 1e-4, "{sun:?} at {x},{y}: {} > {bound}", d.dot(s));
                }
            }
        }
        // straight into the sun: a ray meets it
        assert!(nearest_ray_cos(vp, Vec3::Y) > 0.9999);
    }
}
