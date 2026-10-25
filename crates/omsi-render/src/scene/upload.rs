//! The scene's per-frame upload: instance entries, the camera bind group, the draw list.

use crate::*;

/// The room the instance entries' buffers get for `n` bytes of them: a third more than
/// needed, so that the cars and people spawned over the next minutes are appended instead
/// of forcing new buffers each time.
fn entry_room(n: usize) -> u64 {
    (((n as f64 * 1.35) as u64 + 65536).max(256)).div_ceil(256) * 256
}

impl Renderer {
    /// Upload instance transforms (call after building or changing the scene).
    /// The per-draw entries of one instance: one model matrix and two parameter vectors
    /// per material slot.
    pub(crate) fn instance_entries(
        i: &Instance,
        ro: DVec3,
        mats: &mut Vec<[[f32; 4]; 4]>,
        params: &mut Vec<[f32; 4]>,
    ) {
        let m =
            (Mat4::from_translation((i.origin - ro).as_vec3()) * i.transform).to_cols_array_2d();
        let vis = if i.visible { 1.0 } else { 0.0 };
        for (k, a) in i.slot_alpha.iter().enumerate() {
            let mut m = m;
            m[0][3] = draw_order_level(i, k);
            mats.push(m);
            let uv = i.slot_uv.get(k).copied().unwrap_or([0.0; 2]);
            params.push([*a, vis, uv[0], uv[1]]);
            params.push([
                i.slot_light.get(k).copied().unwrap_or(1.0),
                i.slot_night.get(k).copied().unwrap_or(1.0),
                // (a lamp code is 1 or more, a plain brightness below 1)
                if i.interior_lamps != 0 {
                    i.interior_lamps as f32
                } else {
                    i.interior.min(0.99)
                },
                // the surface flag: 1 ground, 2 a vehicle's shadow blob (no snow on it),
                // 1.25 a legacy pulled decal, 1.4 a road's painted lines, 0.9 an OMSI-ordered surface (weather
                // classification without view-space pull), 0.75 a painted ground layer;
                // below -500 a vehicle part, -(5000 + the roof height relative to origin)
                if let Some(code) = Self::roof_code(i, ro) {
                    code
                } else {
                    surface_instance_code(
                        i.blob,
                        i.ground_layer,
                        i.decal,
                        i.surface,
                        i.surface_bias,
                        i.render_phase == RenderPhase::BeforeNormal,
                    )
                },
            ]);
        }
    }

    /// A vehicle part's roof height relative to the render origin, as its parameters
    /// carry it (see `instance_entries`).
    pub(crate) fn roof_code(i: &Instance, ro: DVec3) -> Option<f32> {
        let roof = i.roof.filter(|_| !i.blob && !i.surface)?;
        let z = (i.origin - ro).z as f32 + i.transform.transform_point3(Vec3::new(0.0, 0.0, roof)).z;
        Some(-(5000.0 + z.clamp(-4000.0, 4000.0)))
    }

    pub fn prepare(&self, scene: &mut Scene) {
        Self::prepare_bounds(scene);
        scene.bind_groups.clear();
        let moved_z = std::mem::take(&mut scene.origin_moved_z);
        let profile = omsi_cfg::flags::OMSI_PROFILE.is_set();
        let t0 = std::time::Instant::now();
        if std::mem::take(&mut scene.origin_moved) && !scene.dirty {
            let ro = scene.render_origin;
            let n = scene.uploaded_entries as usize;
            let params_known = n * 2 <= scene.cpu_params.len();
            for i in &scene.instances[..scene.uploaded_instances] {
                let t = (Mat4::from_translation((i.origin - ro).as_vec3()) * i.transform).w_axis.to_array();
                let (b, k) = (i.base as usize, i.slot_alpha.len());
                if b + k <= n.min(scene.cpu_models.len()) {
                    for m in &mut scene.cpu_models[b..b + k] {
                        m[3] = t;
                    }
                    if let (true, true, Some(code)) = (moved_z, params_known, Self::roof_code(i, ro)) {
                        for p in scene.cpu_params[b * 2..(b + k) * 2].iter_mut().skip(1).step_by(2) {
                            p[3] = code;
                        }
                    }
                }
            }
            match (scene.model_buf.as_ref(), scene.params_buf.as_ref()) {
                (Some(buf), Some(pbuf)) if (n * 64) as u64 <= buf.size() && n <= scene.cpu_models.len() && (!moved_z || (params_known && (n * 32) as u64 <= pbuf.size())) => {
                    buf.write(&self.queue, 0, bytemuck::cast_slice(&scene.cpu_models[..n]));
                    if moved_z {
                        pbuf.write(&self.queue, 0, bytemuck::cast_slice(&scene.cpu_params[..n * 2]));
                    }
                }
                _ => scene.dirty = true,
            }
            if profile {
                log::info!("render origin moved: {} entries of {} instances written again in {:.1} ms", n, scene.uploaded_instances, t0.elapsed().as_secs_f64() * 1000.0);
            }
        }
        if !scene.dirty
            && scene.instances.len() > scene.uploaded_instances
            && scene.model_buf.is_some()
        {
            // new instances: append their entries if the buffers have room, else rebuild
            let ro = scene.render_origin;
            let mut mats: Vec<[[f32; 4]; 4]> = Vec::new();
            let mut params: Vec<[f32; 4]> = Vec::new();
            let first = scene.uploaded_instances;
            let mut base = scene.uploaded_entries;
            for i in scene.instances[first..].iter_mut() {
                i.base = base;
                Self::instance_entries(i, ro, &mut mats, &mut params);
                base = scene.uploaded_entries + mats.len() as u32;
            }
            let (buf, pbuf) = (
                scene.model_buf.as_ref().unwrap(),
                scene.params_buf.as_ref().unwrap(),
            );
            let mo = scene.uploaded_entries as u64 * 64;
            let po = scene.uploaded_entries as u64 * 32;
            let mb: &[u8] = bytemuck::cast_slice(&mats);
            let pb: &[u8] = bytemuck::cast_slice(&params);
            if mo + mb.len() as u64 <= buf.size()
                && po + pb.len() as u64 <= pbuf.size()
                && scene.cpu_models.len() == scene.uploaded_entries as usize
            {
                if !mb.is_empty() {
                    buf.write(&self.queue, mo, mb);
                    pbuf.write(&self.queue, po, pb);
                }
                scene.cpu_models.extend_from_slice(&mats);
                scene.cpu_params.extend_from_slice(&params);
                scene.uploaded_instances = scene.instances.len();
                scene.uploaded_entries = base;
            } else if scene.cpu_models.len() == scene.uploaded_entries as usize
                && scene.cpu_params.len() == scene.uploaded_entries as usize * 2
            {
                // Out of room: bigger buffers, filled from the CPU copies with the new
                // entries after them. The instances' entries stay where they are (rebuilt
                // from the instances, all of them were computed again and the vectors made
                // anew, for the few new ones that did not fit).
                scene.cpu_models.extend_from_slice(&mats);
                scene.cpu_params.extend_from_slice(&params);
                scene.uploaded_instances = scene.instances.len();
                scene.uploaded_entries = base;
                let bytes: &[u8] = bytemuck::cast_slice(&scene.cpu_models);
                let pbytes: &[u8] = bytemuck::cast_slice(&scene.cpu_params);
                let model_buf = GpuArray::new(&self.device, "models", entry_room(bytes.len()), 16, true);
                let params_buf = GpuArray::new(&self.device, "params", entry_room(pbytes.len()), 16, true);
                model_buf.write(&self.queue, 0, bytes);
                params_buf.write(&self.queue, 0, pbytes);
                scene.model_buf = Some(model_buf);
                scene.params_buf = Some(params_buf);
                self.rebuild_camera_bind_group(scene);
                if profile {
                    log::info!("instance entries: {} new ones did not fit: {} in bigger buffers in {:.1} ms", mats.len(), scene.uploaded_entries, t0.elapsed().as_secs_f64() * 1000.0);
                }
            } else {
                if profile {
                    log::info!("instance entries: {} new ones do not fit the buffers ({} bytes): all built again", mats.len(), scene.model_buf.as_ref().map(|b| b.size()).unwrap_or(0));
                }
                scene.dirty = true;
            }
        }
        if !scene.dirty {
            // only what moved or changed since the last frame
            if !scene.changed.is_empty() {
                if omsi_cfg::flags::OMSI_DEBUG_DRAWS.is_set() {
                    log::info!(
                        "prepare: {} changed instances of {}",
                        scene.changed.len(),
                        scene.instances.len()
                    );
                }
                if let (Some(buf), Some(pbuf)) = (&scene.model_buf, &scene.params_buf) {
                    let ro = scene.render_origin;
                    // The changed entries go into the CPU copy; the ranges they span are
                    // uploaded merged - the meshes of one vehicle or person sit next to each
                    // other, and neighbouring vehicles are sent together with whatever lies
                    // between them when that is not much.
                    const MERGE_GAP: u32 = 4096;
                    scene.changed.sort_unstable_by_key(|&i| {
                        scene.instances.get(i).map(|x| x.base).unwrap_or(u32::MAX)
                    });
                    let mut mats: Vec<[[f32; 4]; 4]> = Vec::new();
                    let mut params: Vec<[f32; 4]> = Vec::new();
                    let mut ranges: Vec<(u32, u32)> = Vec::new();
                    for &idx in &scene.changed {
                        let Some(i) = scene.instances.get(idx) else {
                            continue;
                        };
                        mats.clear();
                        params.clear();
                        Self::instance_entries(i, ro, &mut mats, &mut params);
                        let (b, n) = (i.base as usize, mats.len());
                        if b + n > scene.cpu_models.len() || (b + n) * 2 > scene.cpu_params.len() {
                            continue;
                        }
                        scene.cpu_models[b..b + n].copy_from_slice(&mats);
                        scene.cpu_params[b * 2..(b + n) * 2].copy_from_slice(&params);
                        let (start, end) = (b as u32, (b + n) as u32);
                        match ranges.last_mut() {
                            Some(r) if start <= r.1 + MERGE_GAP => r.1 = r.1.max(end),
                            _ => ranges.push((start, end)),
                        }
                    }
                    for (start, end) in ranges {
                        let mb: &[u8] =
                            bytemuck::cast_slice(&scene.cpu_models[start as usize..end as usize]);
                        let pb: &[u8] = bytemuck::cast_slice(
                            &scene.cpu_params[start as usize * 2..end as usize * 2],
                        );
                        let (mo, po) = (start as u64 * 64, start as u64 * 32);
                        if mo + mb.len() as u64 <= buf.size() && po + pb.len() as u64 <= pbuf.size()
                        {
                            buf.write(&self.queue, mo, mb);
                            pbuf.write(&self.queue, po, pb);
                        }
                    }
                }
                for &idx in &scene.changed {
                    if let Some(m) = scene.changed_mark.get_mut(idx) {
                        *m = false;
                    }
                }
                scene.changed.clear();
            }
            return;
        }
        scene.changed.clear();
        scene.changed_mark.clear();
        // one entry per (instance, material slot) so every draw call has its own parameters
        let mut mats: Vec<[[f32; 4]; 4]> = Vec::new();
        let mut params: Vec<[f32; 4]> = Vec::new();
        let ro = scene.render_origin;
        for i in scene.instances.iter_mut() {
            i.base = mats.len() as u32;
            Self::instance_entries(i, ro, &mut mats, &mut params);
        }
        if mats.is_empty() {
            mats.push(Mat4::IDENTITY.to_cols_array_2d());
            params.push([1.0, 1.0, 0.0, 0.0]);
            params.push([1.0, 1.0, 0.0, 0.0]);
        }
        scene.uploaded_instances = scene.instances.len();
        scene.uploaded_entries = mats.len() as u32;
        scene.cpu_models = mats;
        scene.cpu_params = params;
        let bytes: &[u8] = bytemuck::cast_slice(&scene.cpu_models);
        let pbytes: &[u8] = bytemuck::cast_slice(&scene.cpu_params);
        if let (Some(buf), Some(pbuf), Some(_)) = (
            &scene.model_buf,
            &scene.params_buf,
            &scene.camera_bind_group,
        ) {
            if buf.size() >= bytes.len() as u64 && pbuf.size() >= pbytes.len() as u64 {
                buf.write(&self.queue, 0, bytes);
                pbuf.write(&self.queue, 0, pbytes);
                scene.dirty = false;
                if profile {
                    log::info!("instance entries: {} of {} instances written again in {:.1} ms", scene.uploaded_entries, scene.instances.len(), t0.elapsed().as_secs_f64() * 1000.0);
                }
                return;
            }
        }
        let model_buf = GpuArray::new(&self.device, "models", entry_room(bytes.len()), 16, true);
        let params_buf = GpuArray::new(&self.device, "params", entry_room(pbytes.len()), 16, true);
        model_buf.write(&self.queue, 0, bytes);
        params_buf.write(&self.queue, 0, pbytes);
        scene.model_buf = Some(model_buf);
        scene.params_buf = Some(params_buf);
        self.rebuild_camera_bind_group(scene);
        scene.dirty = false;
        if profile {
            log::info!("instance entries: {} of {} instances in new buffers in {:.1} ms", scene.uploaded_entries, scene.instances.len(), t0.elapsed().as_secs_f64() * 1000.0);
        }
    }

    pub(crate) fn rebuild_camera_bind_group(&self, scene: &mut Scene) {
        let (Some(model_buf), Some(params_buf), Some(light_buf), Some(grid_buf), Some(draw_buf)) = (
            &scene.model_buf,
            &scene.params_buf,
            &scene.light_buf,
            &scene.grid_buf,
            &scene.draw_buf,
        ) else {
            return;
        };
        let ground_bound = self.ground_should_bind();
        self.ground_bound.set(ground_bound);
        let mut entries = vec![
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.camera_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: model_buf.binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buf.binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&self.shadows.shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&self.shadows.shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&self.shadows.shadow_view_far),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    // (Enhanced+: the traced lighting, full size, in place of the SSAO)
                    // (while the AO is off the ground albedo picture is bound here, see `ground_should_bind`)
                    resource: wgpu::BindingResource::TextureView(
                        self.rt
                            .as_ref()
                            .and_then(|r| r.targets.as_ref())
                            .map(|t| &t.out)
                            .or(self.targets.ao.as_ref().map(|a| match self.targets.ground.as_ref().filter(|_| ground_bound) { Some((_, g)) => g, None => &a.blur_view }))
                            .unwrap_or(&self.white_texture.view),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::Sampler(&self.ao_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: draw_buf.binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: self.tone.enh_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: wgpu::BindingResource::Sampler(&self.tone.lin_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 18,
                    resource: wgpu::BindingResource::TextureView(&self.lights.lm_atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 19,
                    resource: self.lights.lm_uniform.as_entire_binding(),
                },
        ];
        // (the point lights where the device has storage buffers for them)
        if array_path() != ArrayPath::NoStorage {
            entries.push(wgpu::BindGroupEntry { binding: 3, resource: light_buf.binding() });
            entries.push(wgpu::BindGroupEntry { binding: 4, resource: grid_buf.binding() });
        }
        // (the enhanced path's textures: not on the sixteen-texture-unit path, nor in the
        // launcher's preview, which has no reflection probe - see `camera_layout_entries`)
        if let (false, Some(probe)) = (sixteen_texture_units(), self.tone.probe.as_ref()) {
            entries.extend([
                wgpu::BindGroupEntry { binding: 12, resource: wgpu::BindingResource::TextureView(&probe.view) },
                wgpu::BindGroupEntry { binding: 14, resource: wgpu::BindingResource::TextureView(&self.sky.sky_lut_view) },
                wgpu::BindGroupEntry { binding: 17, resource: wgpu::BindingResource::TextureView(&probe.cube_view) },
            ]);
        }
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera"),
            layout: &self.camera_layout,
            entries: &entries,
        });
        scene.ground_camera_bind_group = ground_bound.then(|| {
            let mut other = entries.clone();
            if let Some(e) = other.iter_mut().find(|e| e.binding == 8) {
                e.resource = wgpu::BindingResource::TextureView(&self.white_texture.view);
            }
            self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("camera, ground albedo pass"), layout: &self.camera_layout, entries: &other })
        });
        scene.camera_bind_group = Some(bg);
        let sbg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow camera"),
            layout: &self.shadows.shadow_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.camera_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: model_buf.binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buf.binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: draw_buf.binding(),
                },
            ],
        });
        scene.shadow_bind_group = Some(sbg);
        scene.precip_bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("precipitation roof camera"),
            layout: &self.shadows.shadow_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.shadows.precip.camera_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: model_buf.binding() },
                wgpu::BindGroupEntry { binding: 2, resource: params_buf.binding() },
                wgpu::BindGroupEntry { binding: 10, resource: draw_buf.binding() },
            ],
        }));
    }

    /// Upload the frame's draw list, growing its buffer (and the bind groups that hold it)
    /// when it no longer fits.
    pub(crate) fn upload_draw_list(&self, scene: &mut Scene, list: &[u32]) {
        let bytes: &[u8] = bytemuck::cast_slice(if list.is_empty() { &[0u32] } else { list });
        let fits = scene
            .draw_buf
            .as_ref()
            .map(|b| b.size() >= bytes.len() as u64)
            .unwrap_or(false);
        if !fits {
            let cap = (bytes.len() as u64 * 3 / 2).max(1 << 16).div_ceil(4) * 4;
            scene.draw_buf = Some(GpuArray::new(&self.device, "draw list", cap, 4, true));
            self.rebuild_camera_bind_group(scene);
        } else if scene.camera_bind_group.is_none() || scene.shadow_bind_group.is_none() {
            self.rebuild_camera_bind_group(scene);
        }
        if let Some(b) = &scene.draw_buf {
            b.write(&self.queue, 0, bytes);
        }
    }
}

/// The depth step of a draw (slot `slot` of `i`) for the vertex shader's `scene_clip`: of
/// two of an object's surfaces in one plane - a dashboard and the instruments printed on it,
/// a plate and the lines on it - the one later in the model is seen, as Omsi.exe draws a
/// model mesh by mesh and subset by subset (0x7c32c4) with LESSEQUAL. Only a slot lying on
/// an earlier surface of its object takes a step (`Instance::slot_levels`, at most 3), and no
/// more than any surface lying just off its plane takes, so that the step never decides
/// between surfaces that are apart (#2790); everything else, and the ground, the roads and
/// what lies on them, keep their depth -
/// but for a spline laid on an earlier spline of its tile at the same height (a red bus
/// lane over the road, `Renderer::set_spline_layer`): Omsi.exe draws a tile's splines in
/// order with LESSEQUAL, so it is seen over the road, where the two fought for every pixel.
/// `OMSI_ZFIGHT_PROBE` swaps the steps of the slots that have one for a hash (1) or its
/// reverse (2), `OMSI_NO_DRAW_ORDER_DEPTH` turns them off.
pub(crate) fn draw_order_level(i: &Instance, slot: usize) -> f32 {
    use std::sync::OnceLock;
    static MODE: OnceLock<u8> = OnceLock::new();
    let mode = *MODE.get_or_init(|| {
        if omsi_cfg::flags::OMSI_NO_DRAW_ORDER_DEPTH.is_set() {
            3
        } else {
            omsi_cfg::flags::OMSI_ZFIGHT_PROBE.parse::<f64>().map(|m| m.round() as u8).filter(|m| *m == 1 || *m == 2).unwrap_or(0)
        }
    });
    let level = i.slot_levels.get(slot).copied().unwrap_or(0);
    let spline = i.surface && i.render_phase == crate::scene::RenderPhase::Spline;
    if mode == 3 || level == 0 || (i.surface && !spline) || i.render_phase == crate::scene::RenderPhase::Terrain {
        return 0.0;
    }
    let max = crate::scene::overlay::MAX_LEVEL as u64;
    match mode {
        0 => level as f32,
        _ => {
            let h = 1 + (slot as u64 ^ i.draw_order).wrapping_mul(0x9e37_79b9_7f4a_7c15) % max;
            (if mode == 1 { h } else { max + 1 - h }) as f32
        }
    }
}
