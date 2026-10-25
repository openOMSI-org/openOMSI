//! The painted ground lit once: the terrain's base and its paint layers are laid into one
//! albedo picture before the main pass (`fs_ground`), and the main pass lights the base
//! from that picture (`from_ground` in `shade_enhanced`) while the layers it took in are
//! left out of its draws - where every layer of paint used to be lit in full and blended
//! over the base's lit colour.

use super::*;

/// The picture's draws: the base's batches and the paint layers', in the order the main pass
/// drew them (the layers over the base, each over the ones before it).
#[derive(Default)]
pub(crate) struct GroundPlan {
    pub base: Vec<Batch>,
    pub paint: Vec<Batch>,
}

impl GroundPlan {
    pub(crate) fn active(&self) -> bool {
        !self.base.is_empty()
    }
}

/// Whether a terrain material is one this picture can hold: nothing in its shading that the
/// albedo does not carry - no night coverage, glow, envmap, bump, moisture, unlit or emissive
/// colour, no snow or water picture, and an ambient colour that is its diffuse one (the
/// picture holds one colour for both). `paint`: a layer blended over the base (a brush mask
/// or the texture's own alpha), else the opaque base.
fn albedo_only(u: &MaterialUniform, paint: bool) -> bool {
    let mode = u.params[0];
    let blank = |v: &[f32]| v.iter().all(|x| x.abs() < 1e-6);
    let flags = (u.params2[3] + 0.5) as u32;
    let ambient_is_diffuse = u.ambient[3] < -0.5 || (0..3).all(|k| (u.ambient[k] - u.color[k]).abs() < 1e-3);
    u.extra[0] > 0.5
        && u.params[1] < 0.5
        && u.params2[1] <= 0.0
        && u.params2[2] <= 0.0
        && flags & (1 | 4 | 8 | 16) == 0
        && u.ambient[3] <= 0.5
        && u.ambient[3] > -1.5
        && ambient_is_diffuse
        && blank(&u.emissive)
        && u.glow[0] == 0.0
        && blank(&u.pbr)
        && u.bump[1] < 0.5
        && u.flags[0] < 0.5
        && u.wipe_bounds[2] == 0.0
        && u.sway[0] < 0.5
        && if paint { (1.5..2.5).contains(&mode) } else { mode < 0.5 && u.params[2] < 0.5 }
}

impl Renderer {
    /// Whether the camera group's slot 8 holds the ground albedo picture: while nothing reads
    /// the AO's there (the ambient occlusion is off, no traced lighting). The camera group is
    /// made again when this changes.
    pub(crate) fn ground_should_bind(&self) -> bool {
        self.ground_eligible()
            && self.targets.ground.as_ref().zip(self.targets.ao.as_ref()).is_some_and(|(g, a)| g.0 == a.size)
    }

    /// Whether the ground picture can be drawn at all with these settings (it is made, and
    /// takes its memory, only then): the Enhanced path's pipelines, the ambient occlusion
    /// off, no traced lighting, one sample.
    pub(crate) fn ground_eligible(&self) -> bool {
        self.ground_pipelines.is_some()
            && !self.options.ssao
            && self.rt.is_none()
            && self.options.msaa <= 1
            && !omsi_cfg::flags::OMSI_NO_GROUND_PASS.is_set()
    }

    /// The ground picture's draws out of the frame's main batches, or none when the frame
    /// cannot have it. The base's batches stay in the main pass (it lights them); the paint
    /// layers right after the terrain phase's opaque draws, in the picture, leave it.
    /// `terrain_blends`: where the terrain phase's blended draws lie in `main_batches`.
    pub(crate) fn plan_ground(&self, scene: &Scene, f: &FrameCtx, main_batches: &mut Vec<Batch>, terrain_blends: std::ops::Range<usize>) -> GroundPlan {
        if !f.ground_ok || !self.ground_bound.get() {
            return GroundPlan::default();
        }
        let mat = |b: &Batch| &scene.materials[b.material as usize].uniform;
        // the terrain's base as the shader takes it (`from_ground`): every draw of it has to
        // be one the picture holds, or the frame goes without
        let is_base = |b: &Batch| b.pipe / 4 == PIPE_OPAQUE && { let u = mat(b); u.extra[0] > 0.5 && u.params[0] < 0.5 && u.params[2] < 0.5 };
        let base: Vec<Batch> = main_batches.iter().filter(|b| is_base(b)).cloned().collect();
        if base.is_empty() || base.iter().any(|b| !albedo_only(mat(b), false)) {
            return GroundPlan::default();
        }
        // the paint layers that follow the base in the order, up to the first draw that is not one
        let start = terrain_blends.start.min(main_batches.len());
        let end = terrain_blends.end.min(main_batches.len());
        let layers = main_batches[start..end].iter().take_while(|b| b.pipe / 4 == PIPE_TERRAIN_PAINT && albedo_only(mat(b), true)).count();
        let paint: Vec<Batch> = main_batches.drain(start..start + layers).collect();
        GroundPlan { base, paint }
    }

    /// The ground picture, before the main pass: the base, then its layers over it.
    pub(crate) fn encode_ground(&self, encoder: &mut wgpu::CommandEncoder, scene: &Scene, plan: &GroundPlan, timers: &mut PassTimers) {
        let (Some(pipes), Some(ao), Some((_, ground))) = (self.ground_pipelines.as_ref(), self.targets.ao.as_ref(), self.targets.ground.as_ref()) else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ground albedo"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ground,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            // (the prepass's depth, only tested against: what the base and its layers
            // show is what the main pass would have drawn there)
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &ao.depth_view, depth_ops: None, stencil_ops: None }),
            timestamp_writes: pass_timer(timers.set.as_ref(), &mut timers.timed, "ground albedo"),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let Some(camera) = scene.ground_camera_bind_group.as_ref() else {
            return;
        };
        pass.set_bind_group(0, camera, &[]);
        encode_batches(&mut pass, scene, &plan.base, |pipe| &pipes[(pipe % 4) as usize]);
        encode_batches(&mut pass, scene, &plan.paint, |pipe| &pipes[4 + (pipe % 4) as usize]);
    }
}
