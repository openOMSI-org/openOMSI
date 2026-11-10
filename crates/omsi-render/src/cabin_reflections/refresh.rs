use super::*;
use std::hash::{Hash, Hasher};

#[derive(Clone)]
pub(super) struct State {
    stamp: u64,
    eye: Vec3,
    forward: Vec3,
    up: Vec3,
    projection: Mat4,
    planes: Vec<glam::Vec4>,
}

impl State {
    pub(super) fn new(scene: &Scene, f: &FrameCtx, planes: &[glam::Vec4], batches: &[Vec<Batch>], list: &[u32]) -> Self {
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
        Self {
            stamp: hash.finish(),
            eye: local.transform_vector3((f.camera.position - origin).as_vec3()),
            forward: local.transform_vector3(f.camera.forward()),
            up: local.transform_vector3(f.camera.up()),
            projection: f.projection.unwrap_or_else(|| Mat4::perspective_rh(f.camera.fov_deg.to_radians(), f.aspect, f.camera.depth_far(), f.camera.near)),
            planes: planes.iter().map(|p| local.transform_vector3(p.truncate()).extend(p.w - p.truncate().dot((origin - f.ro).as_vec3()))).collect(),
        }
    }

    pub(super) fn matches(&self, other: &Self) -> bool {
        self.stamp == other.stamp && self.eye.abs_diff_eq(other.eye, 0.0001)
            && self.forward.abs_diff_eq(other.forward, 0.00001) && self.up.abs_diff_eq(other.up, 0.00001)
            && self.projection.abs_diff_eq(other.projection, 0.00001) && self.planes.len() == other.planes.len()
            && self.planes.iter().zip(&other.planes).all(|(a, b)| a.abs_diff_eq(*b, 0.0001))
    }
}
