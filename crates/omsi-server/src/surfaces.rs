//! CPU-only road/crossing *spline* ground surfaces for the dedicated host.
//!
//! Use the same production CPU tessellation and wheel-probe raster as the
//! graphical host. Scenery [surface] objects, authored holes and aligned
//! terrain deformation are not yet placed here. Those must not be silently
//! replaced by a flat plane or synthetic road surface.

use crate::world::HostWorld;
use glam::{DVec3, Mat4};
use omsi_geometry::{MeshData, SplineCurve, TileSurface};
use omsi_map::tile_size;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

const RASTER_SIZE: usize = 512;
const MAX_SPLINE_LENGTH: f64 = 5_000.0;
const MAX_MESH_FACES: usize = 100_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SurfaceCounts {
    pub target_tiles: usize,
    pub road_meshes: usize,
    pub wheel_meshes: usize,
    pub placed_scenery_surfaces: usize,
    pub overlays: usize,
    pub overhead_only: usize,
    pub skipped_invalid: usize,
    pub missing_definitions: usize,
    pub pending_scenery_surfaces: usize,
}

impl SurfaceCounts {
    pub fn incomplete(&self) -> bool {
        self.skipped_invalid != 0
            || self.missing_definitions != 0
            || self.pending_scenery_surfaces != 0
    }
}

fn overlay_only(def: &omsi_scenery::Spline) -> bool {
    !def.profiles.is_empty()
        && def.profiles.iter().all(|p| {
            def.textures
                .get(p.texture)
                .is_some_and(|texture| texture.alpha == 2)
        })
}

fn overhead_only(def: &omsi_scenery::Spline) -> bool {
    !def.profiles.is_empty()
        && def
            .profiles
            .iter()
            .all(|p| !p.points.is_empty() && p.points.iter().all(|point| point.z >= 2.0))
}

/// A source spline can reach into a neighboring tile. Check actual world
/// geometry bounds before rasterizing into that neighbor's CPU wheel grid.
fn overlaps_tile(mesh: &MeshData, origin: DVec3, key: (i32, i32)) -> bool {
    if mesh.indices.is_empty() || mesh.positions.is_empty() {
        return false;
    }
    let mut lo = glam::Vec3::splat(f32::INFINITY);
    let mut hi = glam::Vec3::splat(f32::NEG_INFINITY);
    for p in &mesh.positions {
        if !p.is_finite() {
            return false;
        }
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let tile = tile_size();
    let x0 = key.0 as f64 * tile;
    let y0 = key.1 as f64 * tile;
    (origin.x + hi.x as f64) >= x0
        && (origin.x + lo.x as f64) <= x0 + tile
        && (origin.y + hi.y as f64) >= y0
        && (origin.y + lo.y as f64) <= y0 + tile
}

/// True only when the transformed scene surface reaches the active tile.
pub(crate) fn raster_overlaps_tile(
    mesh: &MeshData,
    rot: &Mat4,
    origin: DVec3,
    key: (i32, i32),
) -> bool {
    if mesh.indices.is_empty() || mesh.positions.is_empty() {
        return false;
    }
    let mut lo = glam::Vec2::splat(f32::INFINITY);
    let mut hi = glam::Vec2::splat(f32::NEG_INFINITY);
    for point in &mesh.positions {
        let p = rot.transform_point3(*point);
        if !p.is_finite() {
            return false;
        }
        lo = lo.min(p.truncate());
        hi = hi.max(p.truncate());
    }
    let edge = tile_size();
    let x0 = key.0 as f64 * edge;
    let y0 = key.1 as f64 * edge;
    origin.x + hi.x as f64 >= x0
        && origin.x + lo.x as f64 <= x0 + edge
        && origin.y + hi.y as f64 >= y0
        && origin.y + lo.y as f64 <= y0 + edge
}

impl HostWorld {
    /// Publish an immutable CPU road-surface snapshot for all *active* tiles.
    /// Includes source splines in their immediate neighbors so that road
    /// segments crossing tile seams don't end at the map file boundary.
    /// Does not unload terrain or collision geometry that in-world vehicles
    /// still need; streamed surface eviction needs a separate entity-aware
    /// policy. This is a partial physics migration, not readiness parity.
    pub fn install_spline_ground_surfaces(&mut self, active: &[(i32, i32)]) -> SurfaceCounts {
        let mut report = SurfaceCounts::default();
        let tiles: BTreeSet<_> = active.iter().copied().collect();
        let root = self.data.snapshot().map.root.clone();
        let mut rasters = HashMap::new();
        let mut scenery = crate::scenery_ground::ScenerySurfaces::new();
        for key in &tiles {
            if self.load_tile(*key).is_some() {
                rasters.insert(*key, TileSurface::new(RASTER_SIZE));
            }
        }
        report.target_tiles = rasters.len();
        let mut sources = BTreeSet::new();
        for &(x, y) in &tiles {
            for dx in -1..=1 {
                for dy in -1..=1 {
                    sources.insert((x + dx, y + dy));
                }
            }
        }

        for key in sources {
            let Some(tile) = self.load_tile(key) else {
                continue;
            };
            let origin = DVec3::new(key.0 as f64 * tile_size(), key.1 as f64 * tile_size(), 0.0);
            for spline in &tile.splines {
                if spline.deleted || spline.file.trim().is_empty() {
                    continue;
                }
                let Some(def) = self.spline(&root, &spline.file) else {
                    report.missing_definitions += 1;
                    continue;
                };
                if !spline.length.is_finite()
                    || !(0.0..=MAX_SPLINE_LENGTH).contains(&spline.length)
                    || !spline.pos.iter().all(|v| v.is_finite())
                {
                    report.skipped_invalid += 1;
                    continue;
                }
                let curve = SplineCurve::from_map(spline, origin.truncate());
                let drivable = def.paths.iter().any(|p| p.kind == 0 || p.kind == 1);
                let overlay = overlay_only(&def);
                let overhead = overhead_only(&def) && def.height_profiles.is_empty() && !drivable;
                if overlay {
                    report.overlays += 1;
                }
                if overhead {
                    report.overhead_only += 1;
                }
                let visible = omsi_geometry::build_spline_mesh(&def, &curve, spline.mirror, origin);
                let wheel = if omsi_cfg::flags::OMSI_HEIGHTPROFILE_GROUND.is_set() {
                    omsi_geometry::build_height_profile_mesh(&def, &curve, spline.mirror, origin)
                } else {
                    visible.clone()
                };
                if visible.indices.len() / 3 > MAX_MESH_FACES
                    || wheel.indices.len() / 3 > MAX_MESH_FACES
                {
                    report.skipped_invalid += 1;
                    continue;
                }
                for (&target, ts) in &mut rasters {
                    if !overlay && !overhead && overlaps_tile(&visible, origin, target) {
                        ts.rasterize_kind(
                            &visible,
                            &Mat4::IDENTITY,
                            origin,
                            target.0,
                            target.1,
                            drivable,
                        );
                        report.road_meshes += 1;
                    }
                    if overlaps_tile(&wheel, origin, target) {
                        ts.add_spline_drive(&wheel, None, origin, target.0, target.1);
                        report.wheel_meshes += 1;
                    }
                }
            }
            // Static [surface] objects use only their safe first mesh. Any
            // crossing requiring a deformation mesh or terrain-hole cut is
            // explicitly left pending rather than forming false wheel ground.
            self.install_scenery_ground(key, &tile, &mut rasters, &mut scenery, &mut report);
        }
        for raster in rasters.values_mut() {
            raster.finish();
        }
        self.data.update(|data| {
            let ground = Arc::make_mut(&mut data.ground);
            for (key, raster) in rasters {
                ground.surfaces.insert(key, Arc::new(raster));
            }
        });
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn neighboring_tile_surface_does_not_stop_at_source_boundary() {
        let mesh = MeshData {
            positions: vec![
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(40.0, 0.0, 1.0),
                Vec3::new(0.0, 40.0, 1.0),
            ],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        let origin = DVec3::new(tile_size() - 10.0, 0.0, 0.0);
        assert!(overlaps_tile(&mesh, origin, (0, 0)));
        assert!(overlaps_tile(&mesh, origin, (1, 0)));
        assert!(!overlaps_tile(&mesh, origin, (2, 0)));
    }

    #[test]
    fn wheel_height_uses_real_triangle_mesh_not_flat_terrain() {
        let shape = MeshData {
            positions: vec![
                Vec3::new(0.0, 0.0, 3.0),
                Vec3::new(100.0, 0.0, 3.0),
                Vec3::new(0.0, 100.0, 3.0),
            ],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        let mut tile = TileSurface::new(RASTER_SIZE);
        tile.rasterize(&shape, &Mat4::IDENTITY, DVec3::ZERO, 0, 0);
        tile.add_drive_mesh(&shape, &Mat4::IDENTITY, DVec3::ZERO, 0, 0);
        tile.finish();
        let height = tile.sample(20.0, 20.0).expect("road surface height");
        assert!((height - 3.0).abs() < 0.1);
    }
}
