//! History belongs to a scene and advances only for the desktop main view.

use crate::*;
use std::collections::HashMap;

#[derive(Clone)]
pub(super) struct Pose {
    generation: u64,
    revision: u32,
    pub positions: Vec<[f32; 4]>,
}

#[derive(Clone, Copy)]
struct Object {
    identity: u64,
    mesh: MeshId,
    origin: DVec3,
    transform: Mat4,
    visible: bool,
    mesh_revision: (u64, u32),
}

#[derive(Clone, Copy)]
struct View {
    camera: Camera,
    origin: DVec3,
    vp: Mat4,
    size: [u32; 4],
    time: f32,
    wind: [f32; 4],
    rendered: std::time::Instant,
}

#[derive(Default)]
pub(crate) struct History {
    view: Option<View>,
    objects: Vec<Object>,
    poses: HashMap<MeshId, Pose>,
    previous_poses: HashMap<MeshId, Pose>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct MotionUniform {
    pub previous_vp: [[f32; 4]; 4],
    pub size: [f32; 4],
    pub time: [f32; 4],
    pub tree_wind: [f32; 4],
    pub origin: [f32; 4],
}

impl History {
    /// A new renderer/pipeline invalidates old frames, but must preserve a CPU pose
    /// already supplied by this frame's simulation before lazy pipeline creation.
    pub(crate) fn restart_view(&mut self) {
        self.view = None;
        self.objects.clear();
        self.previous_poses.clear();
    }

    pub(crate) fn reset(&mut self) {
        self.restart_view();
        self.poses.clear();
    }

    /// update_mesh supplies CPU-skinned local positions before they reach the GPU.
    /// Keep positions only; previous normals/UVs are unnecessary for reprojection.
    pub(crate) fn record_pose(
        &mut self,
        mesh: MeshId,
        generation: u64,
        revision: u32,
        positions: &[Vec3],
    ) {
        let pose = self.poses.entry(mesh).or_insert_with(|| Pose {
            generation,
            revision,
            positions: Vec::new(),
        });
        pose.generation = generation;
        pose.revision = revision;
        pose.positions.clear();
        pose.positions
            .extend(positions.iter().map(|p| p.extend(1.0).to_array()));
    }

    pub(super) fn previous_pose(&self, mesh: MeshId, generation: u64) -> Option<&Pose> {
        self.previous_poses
            .get(&mesh)
            .filter(|p| p.generation == generation)
    }

    pub(super) fn uniform(
        &mut self,
        f: &passes::FrameCtx,
        vp: Mat4,
        cu: &CameraUniform,
        size: [u32; 4],
        cut: bool,
    ) -> MotionUniform {
        let valid = !cut
            && vp.is_finite()
            && self.view.is_some_and(|v| {
                v.size == size
                    && v.rendered.elapsed().as_secs_f32() < 0.5
                    && (v.camera.position - f.camera.position).length() < 50.0
                    && v.camera.forward().dot(f.camera.forward()) > 0.5
                    && (v.camera.fov_deg - f.camera.fov_deg).abs() < 0.01
                    && v.camera.near == f.camera.near
                    && v.camera.far == f.camera.far
            });
        if !valid {
            self.view = None;
            self.objects.clear();
        }
        let prev = self.view;
        // Old model positions are uploaded relative to THIS frame's render origin.
        // Convert them to the old camera's origin before applying the old VP.
        let previous_vp = prev.map_or(vp, |v| rebase_view(v.vp, v.origin, f.ro));
        MotionUniform {
            previous_vp: previous_vp.to_cols_array_2d(),
            size: [f.width as f32, f.height as f32, valid as u8 as f32, 0.0],
            time: [prev.map_or(cu.post[1], |v| v.time), 0.0, 0.0, 0.0],
            tree_wind: prev.map_or(cu.tree_wind, |v| v.wind),
            origin: cu.world_origin,
        }
    }

    /// Matrix .x.w is metadata in our private buffer: 1 = history valid. WGSL restores
    /// the affine zero before multiplication, just as model_matrix removes draw order.
    pub(super) fn model(
        &self,
        index: usize,
        instance: &Instance,
        ro: DVec3,
        mesh: &GpuMesh,
    ) -> [[f32; 4]; 4] {
        let previous = self.objects.get(index).filter(|p| {
            p.identity == instance.draw_order
                && p.mesh == instance.mesh
                && p.visible
                && p.mesh_revision.0 == mesh.generation
                && (p.mesh_revision.1 == mesh.pose
                    || self
                        .previous_pose(instance.mesh, mesh.generation)
                        .is_some_and(|pose| pose.revision == p.mesh_revision.1))
                && p.transform.is_finite()
                && instance.transform.is_finite()
                && p.origin.is_finite()
                && instance.origin.is_finite()
                && (p.origin - instance.origin).length() < 50.0
        });
        let (origin, transform) = previous.map_or((instance.origin, instance.transform), |p| {
            (p.origin, p.transform)
        });
        let mut m =
            (Mat4::from_translation((origin - ro).as_vec3()) * transform).to_cols_array_2d();
        m[0][3] = previous.is_some() as u8 as f32;
        m
    }

    pub(super) fn commit(
        &mut self,
        scene: &Scene,
        f: &passes::FrameCtx,
        vp: Mat4,
        cu: &CameraUniform,
        size: [u32; 4],
    ) {
        self.view = Some(View {
            camera: *f.camera,
            origin: f.ro,
            vp,
            size,
            time: cu.post[1],
            wind: cu.tree_wind,
            rendered: std::time::Instant::now(),
        });
        self.objects.clear();
        self.objects.extend(scene.instances.iter().map(|i| Object {
            identity: i.draw_order,
            mesh: i.mesh,
            origin: i.origin,
            transform: i.transform,
            visible: i.visible,
            mesh_revision: (scene.meshes[i.mesh].generation, scene.meshes[i.mesh].pose),
        }));
        self.poses.retain(|id, p| {
            scene
                .meshes
                .get(*id)
                .is_some_and(|m| m.generation == p.generation)
        });
        self.previous_poses.retain(|id, p| {
            scene
                .meshes
                .get(*id)
                .is_some_and(|m| m.generation == p.generation)
        });
        for (&mesh, pose) in &self.poses {
            match self.previous_poses.get_mut(&mesh) {
                Some(previous) => {
                    previous.generation = pose.generation;
                    previous.revision = pose.revision;
                    previous.positions.clone_from(&pose.positions);
                }
                None => {
                    self.previous_poses.insert(mesh, pose.clone());
                }
            }
        }
    }
}

fn rebase_view(previous: Mat4, old_origin: DVec3, current_origin: DVec3) -> Mat4 {
    previous * Mat4::from_translation((current_origin - old_origin).as_vec3())
}

/// Engine convention: previous pixel minus current pixel, top-left origin, no jitter.
/// A future NVIDIA adapter must explicitly convert to that SDK's verified convention.
#[cfg(test)]
fn motion(current: Vec4, previous: Vec4, size: glam::Vec2) -> Option<glam::Vec2> {
    if !current.is_finite() || !previous.is_finite() || current.w <= 1e-5 || previous.w <= 1e-5 {
        return None;
    }
    Some(
        (previous.truncate().truncate() / previous.w - current.truncate().truncate() / current.w)
            * size
            * glam::Vec2::new(0.5, -0.5),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_world_survives_large_floating_origin_changes() {
        let camera = Camera {
            position: DVec3::new(892_123.0, 700_000.0, 2.0),
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            fov_deg: 60.0,
            near: 0.1,
            far: 2000.0,
        };
        let old = DVec3::new(892_000.0, 700_000.0, 0.0);
        let new = DVec3::new(892_100.0, 700_100.0, 0.0);
        let point = camera.position + DVec3::new(3.0, 50.0, 0.0);
        let current = camera.view_proj(1.5, new) * (point - new).as_vec3().extend(1.0);
        let previous =
            rebase_view(camera.view_proj(1.5, old), old, new) * (point - new).as_vec3().extend(1.0);
        assert!(
            motion(current, previous, glam::Vec2::new(1920.0, 1280.0))
                .unwrap()
                .length()
                < 0.001
        );
    }

    #[test]
    fn motion_has_pixel_units_and_screen_y_points_down() {
        let size = glam::Vec2::new(200.0, 100.0);
        assert_eq!(
            motion(
                Vec4::new(0.2, 0.4, 0.5, 1.0),
                Vec4::new(0.0, 0.0, 0.5, 1.0),
                size
            ),
            Some(glam::Vec2::new(-20.0, 20.0))
        );
        assert!(motion(Vec4::ONE, Vec4::new(0.0, 0.0, 0.0, -1.0), size).is_none());
        assert!(motion(Vec4::splat(f32::NAN), Vec4::ONE, size).is_none());
    }

    #[test]
    fn reset_releases_history_and_poses() {
        let mut history = History::default();
        history.record_pose(2, 10, 1, &[Vec3::ZERO]);
        history.restart_view();
        assert_eq!(
            history.poses[&2].positions.len(),
            1,
            "lazy pipeline startup keeps this frame's pose"
        );
        history.reset();
        assert!(
            history.view.is_none()
                && history.objects.is_empty()
                && history.poses.is_empty()
                && history.previous_poses.is_empty()
        );
        assert_eq!(std::mem::size_of::<MotionUniform>(), 128);
    }
}
