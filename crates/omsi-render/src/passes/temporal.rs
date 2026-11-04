//! The two passes temporal upscaling adds after the main pass (see `temporal/frame.rs`).

use super::*;

impl Renderer {
    /// The motion of the parts the shared prefill left out of the prepass (a model drawn in its
    /// own order, after its first pane that writes depth: most buses' cabs). They would have only
    /// the street's motion behind the windscreen, and the dashboard would smear while driving.
    /// Drawn after the main pass, over the depth it left, without writing depth (#2059).
    pub(crate) fn encode_temporal_late_motion(&self, encoder: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, plan: &DrawPlan) {
        if !f.temporal_frame || plan.cut_batches.is_empty() || plan.prefill.is_none() || !self.shares_prepass_depth(f, plan) {
            return;
        }
        let (Some(d), Some(ao)) = (self.temporal.as_ref(), self.targets.ao.as_ref()) else { return };
        let (Some(p), Some(t), Some((_, bg))) = (self.temporal_pipes.as_ref(), d.targets.as_ref(), d.prev_models.as_ref()) else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("temporal late motion"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &t.motion_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &ao.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
        pass.set_bind_group(3, bg, &[]);
        // (plain opaque parts only, prepass codes 0 and 1: a cut-out or transmap layer, the dirt
        // on a windscreen, left its depth in scattered texels, and the motion through the glass
        // would come out speckled)
        encode_batches_filtered(&mut pass, scene, &plan.cut_batches, |b: &Batch| b.pipe < 2, |pipe| &p.late_motion[pipe as usize]);
    }

    /// The transparency mask of this frame (see `vs_mask` in `temporal/motion.wgsl`): the blended
    /// draws of the main pass and the cab, the rain films and the bus's screens, over the
    /// prepass's depth.
    pub(crate) fn encode_temporal_mask(&self, encoder: &mut wgpu::CommandEncoder, scene: &Scene, f: &FrameCtx, plan: &DrawPlan) {
        if !f.temporal_frame {
            return;
        }
        let (Some(d), Some(ao)) = (self.temporal.as_ref(), self.targets.ao.as_ref()) else { return };
        let (Some(p), Some(t)) = (self.temporal_pipes.as_ref(), d.targets.as_ref()) else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("temporal transparency mask"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &t.mask_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &ao.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, scene.camera_bind_group.as_ref().unwrap(), &[]);
        let blended = |b: &Batch| matches!(b.pipe / 4, PIPE_BLEND | PIPE_BLEND_NO_WRITE);
        let cull = |pipe: u8| ((pipe >> 1) & 1) as usize;
        for batches in [&plan.main_batches, &plan.cab_batches] {
            encode_batches_filtered(&mut pass, scene, batches, blended, |pipe| &p.glass_mask[cull(pipe)]);
        }
        encode_batches(&mut pass, scene, &plan.rain_batches, |pipe| &p.rain_mask[cull(pipe)]);
        // (not for FSR 3: without the history the jittered frames doubled the edges of their digits)
        let dlss = self.options.temporal.is_some_and(|t| matches!(t.upscaler, TemporalUpscaler::Dlss));
        let screen = |b: &Batch| dlss && scene.materials[b.material as usize].uniform.flags[0] > 0.5;
        for batches in [&plan.main_batches, &plan.cab_batches] {
            encode_batches_filtered(&mut pass, scene, batches, screen, |pipe| &p.screen_mask[cull(pipe)]);
        }
    }
}
