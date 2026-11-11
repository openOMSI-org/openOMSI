use super::*;
use std::hash::{Hash, Hasher};

#[derive(Clone)]
pub(super) struct State {
    resources: [u64; 4],
    stamp: u64,
    motion: u64,
    eye: Vec3,
    forward: Vec3,
    up: Vec3,
    projection: Mat4,
    planes: Vec<glam::Vec4>,
}

impl State {
    pub(super) fn new(scene: &Scene, f: &FrameCtx, planes: &[glam::Vec4], batches: &[Vec<Batch>], list: &[u32], members: &[usize]) -> Self {
        let (origin, heading, _) = f.lighting.inside.unwrap();
        let local = f.lighting.inside_rotation.unwrap_or_else(|| Mat4::from_rotation_z(-(heading as f32).to_radians())).inverse();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        scene.serial.hash(&mut hash);
        scene.model_epoch.hash(&mut hash);
        (f.width, f.height).hash(&mut hash);
        for batch in batches.iter().flatten() {
            (batch.pipe, batch.mesh, batch.first, batch.count, batch.material).hash(&mut hash);
            list[batch.instances.start as usize..batch.instances.end as usize].hash(&mut hash);
            bytemuck::bytes_of(&scene.materials[batch.material as usize].uniform).hash(&mut hash);
        }
        for light in &scene.interior_lights {
            if light.position.distance_squared(origin) > 900.0 { continue; }
            local.transform_vector3((light.position - origin).as_vec3()).to_array().map(|v| (v * 1000.0).round() as i32).hash(&mut hash);
            [light.radius, light.core, light.intensity, light.color[0], light.color[1], light.color[2]].map(f32::to_bits).hash(&mut hash);
        }
        let mut motion = std::collections::hash_map::DefaultHasher::new();
        scene.texture_epoch.load(std::sync::atomic::Ordering::Relaxed).hash(&mut motion);
        for &i in members {
            let inst = &scene.instances[i];
            inst.origin.to_array().map(f64::to_bits).hash(&mut motion);
            inst.transform.to_cols_array().map(f32::to_bits).hash(&mut motion);
            for values in [&inst.slot_alpha, &inst.slot_light, &inst.slot_night] {
                for &v in values { v.to_bits().hash(&mut motion); }
            }
            for uv in &inst.slot_uv { uv.map(f32::to_bits).hash(&mut motion); }
            (inst.visible, inst.interior.to_bits(), inst.interior_lamps).hash(&mut motion);
        }
        for v in [f.lighting.sun_dir, f.lighting.sun_color, f.lighting.ambient, f.lighting.secondary] {
            v.to_array().map(f32::to_bits).hash(&mut motion);
        }
        [f.lighting.sun_intensity, f.lighting.night, f.lighting.night_brightness].map(f32::to_bits).hash(&mut motion);
        Self {
            resources: [scene.serial, scene.model_epoch, f.width as u64, f.height as u64],
            stamp: hash.finish(),
            motion: motion.finish(),
            eye: local.transform_vector3((f.camera.position - origin).as_vec3()),
            forward: local.transform_vector3(f.camera.forward()),
            up: local.transform_vector3(f.camera.up()),
            projection: f.projection.unwrap_or_else(|| Mat4::perspective_rh(f.camera.fov_deg.to_radians(), f.aspect, f.camera.depth_far(), f.camera.near)),
            planes: planes.iter().map(|p| local.transform_vector3(p.truncate()).extend(p.w - p.truncate().dot((origin - f.ro).as_vec3()))).collect(),
        }
    }

    pub(super) fn world_planes(&self, f: &FrameCtx) -> Vec<glam::Vec4> {
        let (origin, heading, _) = f.lighting.inside.unwrap();
        let rotation = f.lighting.inside_rotation.unwrap_or_else(|| Mat4::from_rotation_z(-(heading as f32).to_radians()));
        self.planes.iter().map(|p| {
            let n = rotation.transform_vector3(p.truncate());
            n.extend(p.w + n.dot((origin - f.ro).as_vec3()))
        }).collect()
    }

    pub(super) fn urgent(&self, other: &Self) -> bool {
        self.resources != other.resources || self.planes.len() != other.planes.len()
            || self.planes.iter().zip(&other.planes).any(|(a, b)| !a.abs_diff_eq(*b, 0.0001))
    }

    pub(super) fn matches(&self, other: &Self) -> bool {
        self.stamp == other.stamp && self.motion == other.motion && self.eye.abs_diff_eq(other.eye, 0.0001)
            && self.forward.abs_diff_eq(other.forward, 0.00001) && self.up.abs_diff_eq(other.up, 0.00001)
            && self.projection.abs_diff_eq(other.projection, 0.00001) && self.planes.len() == other.planes.len()
            && self.planes.iter().zip(&other.planes).all(|(a, b)| a.abs_diff_eq(*b, 0.0001))
    }
}

fn due(elapsed: f32, changed: bool, urgent: bool) -> bool {
    urgent || (changed && elapsed >= 1.0 / 30.0)
}

pub(super) fn needs_capture(previous: Option<&(std::time::Instant, State)>, next: &State, xr: bool) -> bool {
    xr || previous.is_none_or(|(at, old)| due(at.elapsed().as_secs_f32(), !old.matches(next), old.urgent(next)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn capture_caps_motion_and_lighting_but_refreshes_new_targets() {
        assert!(!super::due(10.0, false, false), "a static cabin is reused indefinitely");
        assert!(!super::due(0.01, true, false), "motion and lighting cannot bypass the capture interval");
        assert!(super::due(1.0 / 30.0, true, false));
        assert!(super::due(0.01, true, true), "new resources cannot reuse the previous picture");
    }
}
