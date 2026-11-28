//! Decode OMSI [collision_mesh] as CPU triangle collision geometry.
//!
//! Collision meshes are not object bounding boxes: bridges, underpasses and
//! houses can have empty gaps that must remain driveable. Use the same .o3d
//! geometry and production MeshShape as the graphical host, with no GPU upload.

use omsi_scenery::SceneryObject;
use omsi_sim::collision::MeshShape;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Reject pathological custom content before expanding triangles into slabs.
const MAX_TRIANGLES: usize = 100_000;
const MIN_SOLID_TOP: f64 = 0.5;

pub struct MeshCache {
    data: HashMap<PathBuf, Option<Arc<MeshShape>>>,
}

impl MeshCache {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn load(&mut self, sco: &SceneryObject) -> Option<Arc<MeshShape>> {
        let path = collision_path(sco)?;
        if let Some(found) = self.data.get(&path) {
            return found.clone();
        }
        if self.data.len() >= 256 {
            self.data.clear();
        }
        let shape = omsi_o3d::load_mesh(&path)
            .map_err(|error| log::warn!("headless collision mesh {}: {error}", path.display()))
            .ok()
            .and_then(|m| mesh_shape(&m));
        self.data.insert(path, shape.clone());
        shape
    }
}

fn collision_path(sco: &SceneryObject) -> Option<PathBuf> {
    let name = sco.collision_mesh.as_deref()?;
    let parent = sco.path.parent().unwrap_or(Path::new(""));
    let model_dir = sco
        .model_file
        .as_ref()
        .map(|m| omsi_cfg::resolve_path(parent, m))
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| parent.to_path_buf());
    [
        omsi_cfg::resolve_path(&omsi_cfg::resolve_path(&model_dir, "model"), name),
        omsi_cfg::resolve_path(&model_dir, name),
        omsi_cfg::resolve_path(parent, name),
    ]
    .into_iter()
    .find(|path| omsi_cfg::vfs::is_file(path))
}

fn mesh_shape(mesh: &omsi_o3d::Mesh) -> Option<Arc<MeshShape>> {
    if mesh.triangles.len() > MAX_TRIANGLES {
        log::warn!(
            "headless collision mesh: too many triangles ({})",
            mesh.triangles.len()
        );
        return None;
    }
    if mesh.triangles.iter().any(|t| {
        t.indices
            .iter()
            .any(|&index| index as usize >= mesh.vertices.len())
    }) {
        log::warn!("headless collision mesh: invalid triangle vertex index");
        return None;
    }
    let data = omsi_geometry::mesh_from_o3d(mesh);
    let tris = data.indices.chunks_exact(3).filter_map(|i| {
        let (a, b, c) = (
            data.positions.get(i[0] as usize)?,
            data.positions.get(i[1] as usize)?,
            data.positions.get(i[2] as usize)?,
        );
        (a.is_finite() && b.is_finite() && c.is_finite()).then_some([
            a.as_dvec3(),
            b.as_dvec3(),
            c.as_dvec3(),
        ])
    });
    Some(Arc::new(MeshShape::from_triangles(tris, MIN_SOLID_TOP)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_geometry_is_sampled_without_gpu_and_keeps_open_space() {
        let p = |x: f32, y: f32, z: f32| omsi_o3d::Vertex {
            position: glam::Vec3::new(x, y, z),
            normal: glam::Vec3::Z,
            uv: glam::Vec2::ZERO,
        };
        let mesh = omsi_o3d::Mesh {
            vertices: vec![p(0.0, 0.0, 2.0), p(2.0, 0.0, 2.0), p(0.0, 2.0, 2.0)],
            triangles: vec![omsi_o3d::Triangle {
                indices: [0, 1, 2],
                material: 0,
            }],
            ..Default::default()
        };
        let shape = mesh_shape(&mesh).expect("CPU shape");
        assert!(!shape.parts.is_empty(), "the triangle must be kept");
        assert!(
            shape.parts.len() < 100,
            "must not fill an entire bounding box"
        );
    }

    #[test]
    fn too_complex_content_does_not_allocate_unbounded_collision_slabs() {
        let mesh = omsi_o3d::Mesh {
            triangles: vec![
                omsi_o3d::Triangle {
                    indices: [0, 1, 2],
                    material: 0
                };
                MAX_TRIANGLES + 1
            ],
            ..Default::default()
        };
        assert!(mesh_shape(&mesh).is_none());
    }

    #[test]
    fn corrupt_mesh_indices_cannot_panic_on_loading() {
        let m = omsi_o3d::Mesh {
            triangles: vec![omsi_o3d::Triangle {
                indices: [0, 1, 99],
                material: 0,
            }],
            ..Default::default()
        };
        assert!(mesh_shape(&m).is_none());
    }
}
