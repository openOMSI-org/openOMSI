use super::*;
use std::hash::{Hash, Hasher};

pub(super) struct Cached {
    plan: Plan,
    entries: Vec<u32>,
    resources: [u64; 4],
    params: u64,
    built: std::time::Instant,
}

fn parameters(scene: &Scene, members: &[usize]) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    scene.instances.len().hash(&mut hash);
    for &i in members {
        let Some(inst) = scene.instances.get(i) else { continue; };
        (inst.visible, inst.mesh).hash(&mut hash);
        inst.materials.hash(&mut hash);
        for &id in &inst.materials {
            let mat = &scene.materials[id];
            (mat.look, mat.uniform.glow[2].to_bits(), mat.uniform.emissive[3].to_bits()).hash(&mut hash);
        }
        for &alpha in &inst.slot_alpha { (alpha >= 1.0 / 512.0).hash(&mut hash); }
    }
    hash.finish()
}

impl Pipelines {
    pub(crate) fn plan(&mut self, scene: &Scene, f: &FrameCtx, visible: &[(usize, f32, bool)], list: &mut Vec<u32>) -> Option<Plan> {
        // XR stays off until cabin captures are cached per eye.
        if !f.enhanced || f.lighting.reflection_strength <= 0.0 || !f.prepass_on || !f.with_overlays || f.xr_view
            || !f.lighting.inside.as_ref().is_some_and(|own| point_in_vehicle_box(f.camera.position, own)) {
            if f.with_overlays || f.xr_view { self.planned = None; }
            return None;
        }
        let resources = [scene.serial, scene.model_epoch, f.width as u64, f.height as u64];
        let keep = self.planned.as_ref().is_some_and(|c| c.built.elapsed().as_secs_f32() < 1.0 / 30.0
            && c.resources == resources && c.params == parameters(scene, &c.plan.members));
        if !keep {
            let mut entries = Vec::new();
            let Some(plan) = build_plan(scene, f, visible, &mut entries) else { self.planned = None; return None; };
            let params = parameters(scene, &plan.members);
            self.planned = Some(Cached { plan, entries, resources, params, built: std::time::Instant::now() });
        }
        let cached = self.planned.as_ref().unwrap();
        let mut plan = cached.plan.clone();
        plan.planes = plan.refresh.world_planes(f);
        let offset = list.len() as u32;
        list.extend_from_slice(&cached.entries);
        for batch in plan.panes.iter_mut().chain(plan.cabin.iter_mut().flatten()) {
            batch.instances = batch.instances.start + offset..batch.instances.end + offset;
        }
        Some(plan)
    }
}
