//! CPU mesh placement for static scenery ground surfaces.
//!
//! The first mesh of a [surface] object is part of OMSI's wheel ground.
//! Render-only ground layers do not become wheel collision. Static crossings
//! use their authored deformation field and explicit terrain-hole cutter mesh;
//! animated and attached objects remain outside this CPU-only path.

use crate::surfaces::{SurfaceCounts, raster_overlaps_tile};
use crate::world::HostWorld;
use glam::{DVec2, DVec3, Mat4};
use omsi_geometry::{MeshData, TileSurface};
use omsi_map::{tile::Tile, tile_size};
use omsi_model::Model;
use omsi_scenery::SceneryObject;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_SURFACE_TRIANGLES: usize = 80_000;
const MAX_OBJECT_MESH_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone)]
enum SurfaceDef {
    None,
    Unsupported,
    Geometry {
        mesh: Option<Arc<MeshData>>,
        drivable: bool,
        absolute_height: bool,
        holes: Vec<Arc<MeshData>>,
    },
}

pub(crate) struct ScenerySurfaces {
    definitions: HashMap<PathBuf, SurfaceDef>,
}

impl ScenerySurfaces {
    pub(crate) fn new() -> Self {
        Self {
            definitions: HashMap::new(),
        }
    }

    fn load(&mut self, def: &SceneryObject) -> SurfaceDef {
        let key = def.path.clone();
        if let Some(previous) = self.definitions.get(&key) {
            return previous.clone();
        }
        if self.definitions.len() >= 256 {
            self.definitions.clear();
        }
        let result = Self::parse(def);
        self.definitions.insert(key, result.clone());
        result
    }

    fn parse(def: &SceneryObject) -> SurfaceDef {
        if def.only_editor || def.tree.is_some() {
            return SurfaceDef::None;
        }
        let Some((sco, model, directory)) = resolved_model(def) else {
            return SurfaceDef::Unsupported;
        };
        // The visual host honors terrainhole cutters even on objects that
        // have no renderable ground surface (tunnel mouths, crossing helpers).
        let mut holes = Vec::new();
        for (hole_dir, name) in sco.terrain_hole_sources(&model) {
            let Some(hole) = load_checked_mesh(hole_dir, name, false) else {
                return SurfaceDef::Unsupported;
            };
            holes.push(hole);
        }
        let surface = sco.surface || sco.render_type.is_ground_layer();
        if !surface && holes.is_empty() {
            return SurfaceDef::None;
        }
        let mesh = if surface {
            let first = if model.lods.is_empty() {
                model.meshes.first()
            } else {
                model.lod_meshes(0).first()
            };
            let Some(first) = first else {
                return SurfaceDef::Unsupported;
            };
            if !first.animations.is_empty() {
                return SurfaceDef::Unsupported;
            }
            let Some(original) = load_checked_mesh(&directory, &first.file, true) else {
                return SurfaceDef::Unsupported;
            };
            let mut mesh = (*original).clone();
            if let Some(name) = &sco.crossing_height_deformation {
                let Some(field) = load_deformation(&directory, name) else {
                    return SurfaceDef::Unsupported;
                };
                warp_surface_mesh(&mut mesh, &field);
            }
            Some(Arc::new(mesh))
        } else {
            None
        };
        SurfaceDef::Geometry {
            mesh,
            drivable: sco.surface,
            absolute_height: sco.absolute_height(),
            holes,
        }
    }
}

/// Read model tags from the same file and directory as the visual host.
fn resolved_model(def: &SceneryObject) -> Option<(SceneryObject, Model, PathBuf)> {
    let parent = def.path.parent().unwrap_or(Path::new(""));
    let (model, directory) = match def.model_file.as_deref() {
        Some(file) => {
            let path = omsi_cfg::resolve_path(parent, file);
            let model = Model::load(&path).ok()?;
            (model, path.parent()?.to_path_buf())
        }
        None => (def.model.clone(), parent.to_path_buf()),
    };
    let mut sco = def.clone();
    sco.inherit_model_tags(&model);
    Some((sco, model, directory))
}

/// Reject oversized, missing or malformed map meshes before triangle indexing.
fn load_checked_mesh(directory: &Path, name: &str, model_first: bool) -> Option<Arc<MeshData>> {
    let model = omsi_cfg::resolve_path(&omsi_cfg::resolve_path(directory, "model"), name);
    let direct = omsi_cfg::resolve_path(directory, name);
    let paths = if model_first {
        [model, direct]
    } else {
        [direct, model]
    };
    let path = paths.into_iter().find(|p| omsi_cfg::vfs::is_file(p))?;
    if std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() > MAX_OBJECT_MESH_BYTES) {
        return None;
    }
    let loaded = omsi_o3d::load_mesh(&path).ok()?;
    if loaded.triangles.len() > MAX_SURFACE_TRIANGLES
        || loaded.triangles.iter().any(|triangle| {
            triangle
                .indices
                .iter()
                .any(|&i| i as usize >= loaded.vertices.len())
        })
    {
        return None;
    }
    let mesh = omsi_geometry::mesh_from_o3d(&loaded);
    if mesh.indices.is_empty() || mesh.positions.iter().any(|point| !point.is_finite()) {
        return None;
    }
    Some(Arc::new(mesh))
}

/// Shared with the server's AI lane placement. The deformation field is in
/// the object's local XY; it is NOT sampled from the terrain under a bridge.
const MAX_DEFORMATION_FACES: usize = 4_096;

fn load_deformation(directory: &Path, name: &str) -> Option<Arc<MeshData>> {
    let field = load_checked_mesh(directory, name, true)?;
    (field.indices.len() / 3 <= MAX_DEFORMATION_FACES).then_some(field)
}

/// None means the object has no field, Err means an authored field is missing.
pub(crate) fn crossing_field(def: &SceneryObject) -> Result<Option<Arc<MeshData>>, ()> {
    let (sco, _, directory) = resolved_model(def).ok_or(())?;
    match sco.crossing_height_deformation {
        Some(name) => load_deformation(&directory, &name).map(Some).ok_or(()),
        None => Ok(None),
    }
}

/// Use the barycentric crossing-height lookup of the graphical host.
pub(crate) fn field_height(field: &MeshData, x: f32, y: f32) -> Option<f32> {
    let mut best: Option<f32> = None;
    for triangle in field.indices.chunks_exact(3) {
        let (Some(&a), Some(&b), Some(&c)) = (
            field.positions.get(triangle[0] as usize),
            field.positions.get(triangle[1] as usize),
            field.positions.get(triangle[2] as usize),
        ) else {
            continue;
        };
        let det = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
        if det.abs() < 1e-9 {
            continue;
        }
        let l1 = ((b.x - a.x) * (y - a.y) - (x - a.x) * (b.y - a.y)) / det;
        let l2 = ((x - a.x) * (c.y - a.y) - (c.x - a.x) * (y - a.y)) / det;
        let l0 = 1.0 - l1 - l2;
        if l0 >= -1e-4 && l1 >= -1e-4 && l2 >= -1e-4 {
            let height = l0 * a.z + l2 * b.z + l1 * c.z;
            best = Some(best.map_or(height, |old| old.max(height)));
        }
    }
    best
}

fn warp_surface_mesh(mesh: &mut MeshData, field: &MeshData) {
    for vertex in &mut mesh.positions {
        if let Some(height) = field_height(field, vertex.x, vertex.y) {
            vertex.z += height;
        }
    }
}

/// A stale crossing placed many metres under the terrain cannot punch a pit.
fn hole_is_buried(
    hole: &MeshData,
    rotation: &Mat4,
    pos: DVec3,
    ground_at: &dyn Fn(f64, f64) -> Option<f64>,
) -> bool {
    if hole.positions.is_empty() {
        return false;
    }
    let (mut top, mut ground) = (f64::NEG_INFINITY, f64::INFINITY);
    for vertex in &hole.positions {
        let world = rotation.transform_point3(*vertex).as_dvec3() + pos;
        let Some(height) = ground_at(world.x, world.y) else {
            return false;
        };
        top = top.max(world.z);
        ground = ground.min(height);
    }
    top + 10.0 < ground
}

impl HostWorld {
    /// Publish authored static and height-field-deformed surfaces into the
    /// active tile rasters. Existing map-index coordinates preserve Chrono and
    /// geographic map placement; no GPU meshes or texture files are loaded.
    pub(crate) fn install_scenery_ground(
        &mut self,
        key: (i32, i32),
        tile: &Tile,
        rasters: &mut HashMap<(i32, i32), TileSurface>,
        cache: &mut ScenerySurfaces,
        counts: &mut SurfaceCounts,
    ) {
        let root = self.data.snapshot().map.root.clone();
        for obj in &tile.objects {
            if obj.file.trim().is_empty() {
                continue;
            }
            let Some(def) = self.scenery(&root, &obj.file) else {
                counts.missing_definitions += 1;
                continue;
            };
            let loaded = cache.load(&def);
            let SurfaceDef::Geometry {
                mesh,
                drivable,
                absolute_height,
                holes,
            } = loaded
            else {
                if matches!(loaded, SurfaceDef::Unsupported) {
                    counts.pending_scenery_surfaces += 1;
                }
                continue;
            };
            if obj.parent_id.is_some() {
                counts.pending_scenery_surfaces += 1;
                continue;
            }
            let indexed = self
                .index
                .duplicates
                .get(&(key, obj.id))
                .copied()
                .or_else(|| {
                    self.index
                        .objects
                        .get(&obj.id)
                        .and_then(|(owner, pos, rot)| (*owner == key).then_some((*pos, *rot)))
                });
            // MapIndex records ordinary ground-relative objects with terrain
            // added, even when the .sco declares [absheight]. For an absolute
            // bridge or tunnel plate the recorded Z must win over the index.
            let (pos, raw_rot) = if absolute_height {
                (
                    DVec3::new(
                        key.0 as f64 * tile_size() + obj.pos[0],
                        key.1 as f64 * tile_size() + obj.pos[1],
                        obj.pos[2],
                    ),
                    obj.rot,
                )
            } else {
                match indexed {
                    Some(p) => p,
                    None => {
                        let base = DVec2::new(
                            key.0 as f64 * tile_size() + obj.pos[0],
                            key.1 as f64 * tile_size() + obj.pos[1],
                        );
                        let height = self
                            .terrain_height(base.x, base.y)
                            .map(f64::from)
                            .unwrap_or(0.0);
                        (DVec3::new(base.x, base.y, obj.pos[2] + height), obj.rot)
                    }
                }
            };
            if !pos.is_finite() || !raw_rot.iter().all(|v| v.is_finite()) {
                counts.pending_scenery_surfaces += 1;
                continue;
            }
            let rotation = omsi_geometry::object_rotation(omsi_geometry::map_rotation(raw_rot));
            let ground = Arc::clone(&self.data);
            for (&target, surface) in rasters.iter_mut() {
                if let Some(mesh) = &mesh {
                    if raster_overlaps_tile(mesh, &rotation, pos, target) {
                        surface.rasterize_kind(mesh, &rotation, pos, target.0, target.1, drivable);
                        if drivable {
                            surface.add_drive_mesh(mesh, &rotation, pos, target.0, target.1);
                        }
                        counts.placed_scenery_surfaces += 1;
                    }
                }
                for hole in &holes {
                    if !raster_overlaps_tile(hole, &rotation, pos, target)
                        || hole_is_buried(hole, &rotation, pos, &|x, y| {
                            let data = ground.peek();
                            let (tile, (local_x, local_y)) = omsi_map::world_to_tile_local(x, y);
                            data.ground.terrains.get(&tile).map(|terrain| {
                                f64::from(terrain.sample(local_x as f32, local_y as f32))
                            })
                        })
                    {
                        continue;
                    }
                    surface.rasterize_hole(hole, &rotation, pos, target.0, target.1);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn crossing_field_deforms_only_covered_vertices() {
        let field = MeshData {
            positions: vec![
                Vec3::new(0.0, 0.0, 2.0),
                Vec3::new(10.0, 0.0, 2.0),
                Vec3::new(0.0, 10.0, 2.0),
            ],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        let mut crossing = MeshData {
            positions: vec![Vec3::new(2.0, 2.0, 1.0), Vec3::new(20.0, 20.0, 1.0)],
            indices: vec![0, 1, 1],
            ..Default::default()
        };
        warp_surface_mesh(&mut crossing, &field);
        assert!((crossing.positions[0].z - 3.0).abs() < 1e-5);
        assert!((crossing.positions[1].z - 1.0).abs() < 1e-5);
    }

    #[test]
    fn explicit_terrain_hole_cuts_even_without_a_surface_object() {
        let hole = MeshData {
            positions: vec![
                Vec3::new(4.0, 4.0, 0.0),
                Vec3::new(16.0, 4.0, 0.0),
                Vec3::new(16.0, 16.0, 0.0),
                Vec3::new(4.0, 16.0, 0.0),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            ..Default::default()
        };
        let mut surface = TileSurface::new(128);
        surface.rasterize_hole(&hole, &Mat4::IDENTITY, DVec3::ZERO, 0, 0);
        surface.finish();
        assert!(surface.cut_at(10.0, 10.0, 0.0, 0.12));
        assert!(!surface.cut_at(30.0, 30.0, 0.0, 0.12));
    }

    #[test]
    fn deeply_buried_hole_is_not_applied_to_road_terrain() {
        let hole = MeshData {
            positions: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(4.0, 0.0, 0.0),
                Vec3::new(0.0, 4.0, 0.0),
            ],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        assert!(hole_is_buried(
            &hole,
            &Mat4::IDENTITY,
            DVec3::ZERO,
            &|_, _| Some(30.0),
        ));
        assert!(!hole_is_buried(
            &hole,
            &Mat4::IDENTITY,
            DVec3::ZERO,
            &|_, _| Some(0.0),
        ));
    }

    #[test]
    fn rotated_surface_geometry_can_reach_a_neighbor() {
        let mesh = MeshData {
            positions: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(20.0, 0.0, 0.0),
                Vec3::new(0.0, 20.0, 0.0),
            ],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        let pos = DVec3::new(tile_size() - 5.0, 5.0, 0.0);
        assert!(raster_overlaps_tile(&mesh, &Mat4::IDENTITY, pos, (1, 0)));
        assert!(!raster_overlaps_tile(&mesh, &Mat4::IDENTITY, pos, (2, 0)));
    }
}
