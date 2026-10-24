//! The shadow maps' passes: the sun's cascades and the street lamps' tiles.

use super::*;

/// What a lamp's shadow tile draws this frame, summed up (`lamp_item_hash` of each item
/// added: the same whatever order the worker threads find them in), and which tiles draw
/// something that moves every frame (foliage bending in the wind).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LampSigs {
    pub sums: [u64; LAMP_SHADOWS],
    pub volatile: u64,
}

impl Default for LampSigs {
    fn default() -> Self {
        LampSigs { sums: [0; LAMP_SHADOWS], volatile: 0 }
    }
}

impl LampSigs {
    pub(crate) fn add(&mut self, k: usize, h: u64) {
        self.sums[k] = self.sums[k].wrapping_add(h);
    }

    pub(crate) fn merge(&mut self, o: &LampSigs) {
        for k in 0..LAMP_SHADOWS {
            self.add(k, o.sums[k]);
        }
        self.volatile |= o.volatile;
    }
}

/// The lamps' shadow tiles as kept between frames. The lamps stand still, and so do
/// most things under them: every tile was cleared and drawn anew every night frame, and the
/// pass loaded and stored the whole far map with them (84 MB each way at the 4096 setting).
/// A tile is drawn again when what it draws has changed (an item, its place, a skinned
/// pose), its lamp's matrix has (another lamp in the tile, the render origin moved), or it
/// holds foliage bending in the wind; a frame without changed tiles has no lamp pass.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LampTiles {
    /// This frame's (set by `plan_shadow_casters`).
    pub frame: LampSigs,
    /// What each tile holds (the key of `lamp_tile_key`), `None`: nothing to keep.
    pub drawn: [Option<u64>; LAMP_SHADOWS],
    /// The window frames that have drawn lamp tiles (counted for the foliage's turns).
    pub frames: u32,
}

impl Default for LampTiles {
    fn default() -> Self {
        LampTiles { frame: LampSigs::default(), drawn: [None; LAMP_SHADOWS], frames: 0 }
    }
}

/// A tile with foliage bending in the wind in it is drawn anew every this many frames (each tile
/// on its own turn, so that the tiles of a row of lamps do not all fall on one frame): a tree's
/// sway changes its shadow by a little in a twentieth of a second.
const FOLIAGE_TILE_PERIOD: u32 = 3;

fn mix(h: u64, x: u64) -> u64 {
    (h.rotate_left(5) ^ x).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95)
}

/// One draw of a lamp tile, as `LampSigs` sums them: the mesh, its range and pose, the
/// material (and the slot's alpha, a script's fading part) and the instance's place. A saloon
/// lamp's tile (`frame`: the world's frame in the vehicle's, and the render origin) takes the
/// place in the vehicle's frame, to a coarser step: the numbers that come out of the vehicle's
/// own rotation are the same to a few millionths only.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lamp_item_hash(inst: &Instance, pose: u32, kind: u8, range: u32, material: u32, look: u32, alpha: f32, frame: Option<(&Mat4, DVec3)>) -> u64 {
    let mut h = mix(0x9e37_79b9_7f4a_7c15, inst.mesh as u64);
    h = mix(h, alpha.to_bits() as u64);
    h = mix(h, (range as u64) << 32 | pose as u64);
    h = mix(h, (material as u64) << 8 | kind as u64);
    h = mix(h, look as u64);
    if let Some((to_local, ro)) = frame {
        return mix(mix(h, cabin_place(inst, to_local, ro)), 0x2545_f491_4f6c_dd1d);
    }
    // (the place to a few millimetres and the turn to a tenth of a degree, a fraction of
    // a tile's texel: a bus standing with its engine running shakes by less, and taken
    // exactly, every tile it stood in was drawn anew every frame)
    let t = inst.transform.to_cols_array();
    for (k, v) in t.iter().enumerate() {
        let step = if k >= 12 { 0.004 } else { 0.0015 };
        h = mix(h, (v / step).round() as i64 as u64);
    }
    for v in inst.origin.to_array() {
        h = mix(h, (v / 0.004).round() as i64 as u64);
    }
    // (a final mix: the sums of plain products would let two changes cancel too easily)
    mix(h, 0x2545_f491_4f6c_dd1d)
}

/// An instance's place in the vehicle's frame (`to_local`: the world's in it; `ro`: the render
/// origin), taken to a few centimetres and a degree or so: a texel of a saloon's tile is three
/// centimetres, and what comes out of the vehicle's own rotation is the same to a few
/// millionths only.
fn cabin_place(inst: &Instance, to_local: &Mat4, ro: DVec3) -> u64 {
    let t = *to_local * Mat4::from_translation((inst.origin - ro).as_vec3()) * inst.transform;
    let mut h = 0x51ed_270b_1c3a_9f07;
    for (k, v) in t.to_cols_array().iter().enumerate() {
        let step = if k >= 12 { 0.05 } else { 0.02 };
        h = mix(h, (v / step).round() as i64 as u64);
    }
    h
}

/// How many frames a part of the saloon keeps its place in the vehicle before it counts as
/// standing still, and so as part of what its lamps' tiles hold.
const CABIN_SETTLE_FRAMES: u8 = 30;
/// A part not tracked (outside the vehicle's box, or never seen).
const CABIN_UNTRACKED: u8 = u8::MAX;

/// Which parts of the player's vehicle stand still in it. A saloon lamp's tile holds what stands
/// still under the lamp: the seats, the rails, the panels, a door that is shut. The wheels, the
/// steering wheel, a door that swings, the driver and the passengers move while the bus drives:
/// taken into a tile, each of them would draw all sixteen again every frame, so a part that
/// has not kept its place for `CABIN_SETTLE_FRAMES` frames (half a second: a needle creeping across
/// a dial pauses for less) is left out of them until it does
/// (the first frame a part is seen it counts as still: a picture of one frame has its saloon).
#[derive(Default)]
pub(crate) struct CabinMotion {
    /// Per instance: its place in the vehicle's frame last frame and how many frames it has kept it.
    seen: Vec<(u64, u8)>,
    /// Per instance: whether it stands still in the vehicle now.
    pub(crate) still: Vec<bool>,
}

impl CabinMotion {
    /// Note where the instances inside the vehicle's box (`cabin`) are this frame.
    pub(crate) fn update(&mut self, scene: &Scene, cabin: &Cabin) {
        let n = scene.instances.len();
        self.seen.resize(n, (0, CABIN_UNTRACKED));
        self.still.clear();
        self.still.resize(n, false);
        for (i, inst) in scene.instances.iter().enumerate() {
            let inside = inst.visible && cabin.vehicle.contains(Renderer::bounding_sphere(scene, inst).0, 0.3);
            if !inside {
                self.seen[i] = (0, CABIN_UNTRACKED);
                continue;
            }
            let key = mix(cabin_place(inst, &cabin.to_local, scene.render_origin), scene.meshes[inst.mesh].pose as u64);
            let (last, age) = self.seen[i];
            let age = if age == CABIN_UNTRACKED { CABIN_SETTLE_FRAMES } else if last == key { (age + 1).min(CABIN_SETTLE_FRAMES) } else { 0 };
            self.seen[i] = (key, age);
            self.still[i] = age >= CABIN_SETTLE_FRAMES;
        }
        if omsi_cfg::flags::OMSI_DEBUG_DRAWS.is_set() {
            let inside = self.seen.iter().filter(|s| s.1 != CABIN_UNTRACKED).count();
            let moving: Vec<usize> = (0..n).filter(|&i| self.seen[i].1 != CABIN_UNTRACKED && !self.still[i]).collect();
            log::info!("saloon: {inside} parts in the vehicle, {} moving: {:?}", moving.len(), moving.iter().take(12).map(|&i| (i, scene.meshes[scene.instances[i].mesh].pose)).collect::<Vec<_>>());
        }
    }
}

/// What tile `k` holds when drawn this frame for `lamp`.
pub(crate) fn lamp_tile_key(sums: &LampSigs, k: usize, view_proj: Mat4) -> u64 {
    let mut h = mix(sums.sums[k], k as u64);
    for v in view_proj.to_cols_array() {
        h = mix(h, v.to_bits() as u64);
    }
    h
}

impl Renderer {
    /// The near cascade (with the close one beside it in the atlas) and the far cascade,
    /// each when it is redrawn this frame.
    pub(crate) fn encode_shadow_maps(&self, shadow_encoder: &mut wgpu::CommandEncoder, scene: &Scene, sh: &ShadowPlan, shadow_batches: &[Vec<Batch>; SHADOW_LISTS], timers: &mut PassTimers) {
        let (draw_shadows, redraw_near, redraw_far) = (sh.draw_shadows, sh.redraw_near, sh.redraw_far);
        for cascade in [0usize, 1] {
            if !draw_shadows || (cascade == 1 && !redraw_far) {
                continue;
            }
            let view = if cascade == 0 {
                &self.shadows.shadow_view
            } else {
                &self.shadows.shadow_view_far
            };
            let keep_near = cascade == 0 && !redraw_near;
            // (the far map keeps the street lamps' tiles under it when they are kept)
            let keep_lamps = cascade == 1 && self.shadows.lamp_tiles.get().drawn.iter().any(|d| d.is_some());
            let mut pass = shadow_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations {
                        load: if keep_near || keep_lamps { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(1.0) },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: pass_timer(
                    timers.set.as_ref(),
                    &mut timers.timed,
                    if cascade == 0 {
                        "shadow near"
                    } else {
                        "shadow far"
                    },
                ),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, scene.shadow_bind_group.as_ref().unwrap(), &[]);
            if cascade == 0 {
                // the atlas: near cascade on the left half, close cascade on the right
                let sz = self.options.shadow_size as f32;
                pass.set_viewport(0.0, 0.0, sz, sz, 0.0, 1.0);
                encode_batches(&mut pass, scene, &shadow_batches[0], |pipe| {
                    &self.shadows.shadow_pipelines[pipe as usize]
                });
                let csz = self.options.shadow_size.min(SHADOW_CLOSE_MAX) as f32;
                pass.set_viewport(sz, 0.0, csz, csz, 0.0, 1.0);
                if keep_near {
                    // (the near half is last frame's: only the close part is cleared)
                    pass.set_pipeline(&self.shadows.shadow_clear_pipeline);
                    pass.draw(0..3, 0..1);
                    pass.set_bind_group(0, scene.shadow_bind_group.as_ref().unwrap(), &[]);
                }
                encode_batches(&mut pass, scene, &shadow_batches[2], |pipe| {
                    &self.shadows.shadow_pipelines[4 + pipe as usize]
                });
            } else {
                let sz = self.options.shadow_size as f32;
                pass.set_viewport(0.0, 0.0, sz, sz, 0.0, 1.0);
                if keep_lamps {
                    pass.set_pipeline(&self.shadows.shadow_clear_pipeline);
                    pass.draw(0..3, 0..1);
                    pass.set_bind_group(0, scene.shadow_bind_group.as_ref().unwrap(), &[]);
                }
                encode_batches(&mut pass, scene, &shadow_batches[cascade], |pipe| {
                    &self.shadows.shadow_pipelines[cascade * 2 + pipe as usize]
                });
            }
        }
    }

    // the lamps' maps: tiles under the far map (`LampAtlas`), each cleared and drawn when
    // what it shows has changed (see `LampTiles`)
    pub(crate) fn encode_lamp_shadows(&self, shadow_encoder: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, shadow_batches: &[Vec<Batch>; SHADOW_LISTS], timers: &mut PassTimers) {
        let lamp_shadows = &f.lamp_shadows;
        // (the frames that draw the lamps' maps keep the record: a mirror's has none)
        if !f.with_overlays || f.projection.is_some() {
            return;
        }
        let mut tiles = self.shadows.lamp_tiles.get();
        let mut redraw = [false; LAMP_SHADOWS];
        for (k, d) in tiles.drawn.iter_mut().enumerate() {
            let Some(l) = lamp_shadows.iter().find(|l| l.slot == k) else {
                *d = None;
                continue;
            };
            let key = lamp_tile_key(&tiles.frame, k, l.key_matrix());
            let foliage_turn = tiles.frame.volatile & (1u64 << k) != 0 && (tiles.frames.wrapping_add(k as u32)) % FOLIAGE_TILE_PERIOD == 0;
            if *d != Some(key) || foliage_turn || f.env.lamp_shadows_every_frame {
                if f.env.debug_draws {
                    log::info!("lamp shadow tile {k} drawn: {} (lamp at {:?}, {} batches)", if *d != Some(key) { "changed" } else { "moving foliage" }, l.position, shadow_batches[3 + k].len());
                }
                redraw[k] = true;
                *d = Some(key);
            }
        }
        if f.env.debug_draws && !lamp_shadows.is_empty() {
            let count = |run: std::ops::Range<usize>| (run.clone().filter(|k| redraw[*k]).count(), lamp_shadows.iter().filter(|l| run.contains(&l.slot)).count());
            let (w, v, c) = (count(WORLD_SLOTS), count(VEHICLE_SLOTS), count(CABIN_SLOTS));
            log::info!("lamp shadow tiles drawn: world {} of {}, vehicle {} of {}, saloon {} of {}", w.0, w.1, v.0, v.1, c.0, c.1);
        }
        tiles.frames = tiles.frames.wrapping_add(1);
        self.shadows.lamp_tiles.set(tiles);
        if f.env.debug_draws {
            *self.counts.borrow_mut().entry("lamp shadow tiles drawn").or_default() += redraw.iter().filter(|r| **r).count() as f64;
        }
        if redraw.iter().any(|r| *r) {
            let mut pass = shadow_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lamp shadows"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadows.shadow_view_far,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: pass_timer(timers.set.as_ref(), &mut timers.timed, "lamp shadows"),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let atlas = self.shadows.lamp_atlas;
            let tile = atlas.tile as f32;
            for k in (0..LAMP_SHADOWS).filter(|k| redraw[*k]) {
                let (x, y) = atlas.origin(k);
                pass.set_viewport(x as f32, y as f32, tile, tile, 0.0, 1.0);
                pass.set_pipeline(&self.shadows.shadow_clear_pipeline);
                pass.draw(0..3, 0..1);
                pass.set_bind_group(0, scene.shadow_bind_group.as_ref().unwrap(), &[]);
                encode_batches(&mut pass, scene, &shadow_batches[3 + k], |pipe| {
                    &self.shadows.shadow_pipelines[6 + pipe as usize]
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tiles' sums do not depend on the order the workers find the items in, and a
    /// lamp's tile key changes with its matrix and its place among the tiles.
    #[test]
    fn lamp_tile_keys() {
        let items = [3u64, 0xdead_beef, 77, 1 << 40];
        let (mut a, mut b, mut c) = (LampSigs::default(), LampSigs::default(), LampSigs::default());
        for &h in &items {
            a.add(1, h);
        }
        for &h in items.iter().rev().skip(2) {
            b.add(1, h);
        }
        for &h in items.iter().rev().take(2) {
            c.add(1, h);
        }
        b.merge(&c);
        assert_eq!(a, b);
        let m = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let k = lamp_tile_key(&a, 1, m);
        assert_eq!(k, lamp_tile_key(&b, 1, m));
        assert_ne!(k, lamp_tile_key(&a, 2, m));
        assert_ne!(k, lamp_tile_key(&a, 1, Mat4::IDENTITY));
        a.add(1, 5);
        assert_ne!(k, lamp_tile_key(&a, 1, m));
    }
}
