//! Conservative static-object collision for the renderer-free host.
//!
//! Publishes only proven solid .sco bounding boxes. Mesh-shaped obstacles,
//! attached objects, moving parked vehicles, road surface meshes and advanced
//! terrain deformation still require dedicated CPU placement before admitting
//! actual game clients. Do not substitute a large box for a collision mesh:
//! bridges and depots must not become invisible walls.

use crate::world::HostWorld;
use glam::{DVec2, DVec3};
use omsi_scenery::SceneryObject;
use omsi_sim::collision::{CollisionWorld, Obb};
use omsi_sim::traffic::{Lane, LaneKind};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

const ROAD_CELL: f64 = 64.0;
const LOW_OBSTACLE: f32 = 0.5;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CollisionCounts {
    pub loaded_tiles: usize,
    pub static_boxes: usize,
    pub static_meshes: usize,
    /// Meshes with missing, unsupported or oversized files.
    pub pending_tilted: usize,
    /// Collision meshes have different shapes and must be loaded as meshes.
    pub pending_meshes: usize,
    pub pending_attachments: usize,
    pub missing_definitions: usize,
    pub avoided_road_walls: usize,
}

impl CollisionCounts {
    pub fn incomplete(&self) -> bool {
        self.pending_meshes > 0
            || self.pending_tilted > 0
            || self.pending_attachments > 0
            || self.missing_definitions > 0
    }
}

/// A coarse immutable road-point lookup. A single large building can span
/// several cells; test only its nearby buckets to avoid O(objects × lanes).
#[derive(Default)]
struct RoadPoints {
    cells: HashMap<(i32, i32), Vec<DVec3>>,
}

impl RoadPoints {
    fn build(lanes: &[Lane]) -> Self {
        let mut result = Self::default();
        for lane in lanes
            .iter()
            .filter(|lane| matches!(lane.kind, LaneKind::Street))
        {
            for &p in &lane.points {
                if p.is_finite() {
                    result
                        .cells
                        .entry(Self::cell(p.truncate()))
                        .or_default()
                        .push(p);
                }
            }
        }
        result
    }

    fn cell(p: DVec2) -> (i32, i32) {
        (
            (p.x / ROAD_CELL).floor() as i32,
            (p.y / ROAD_CELL).floor() as i32,
        )
    }

    fn crosses(&self, box_: &Obb) -> bool {
        let r = box_.radius();
        let min = Self::cell(box_.center - DVec2::splat(r));
        let max = Self::cell(box_.center + DVec2::splat(r));
        let [right, forward] = box_.axes();
        for x in min.0..=max.0 {
            for y in min.1..=max.1 {
                let Some(points) = self.cells.get(&(x, y)) else {
                    continue;
                };
                for p in points {
                    let d = p.truncate() - box_.center;
                    // A road on the roof of a bridge (or far below it) must not
                    // suppress collision obstacles elsewhere on that structure.
                    if d.dot(right).abs() <= box_.half.x
                        && d.dot(forward).abs() <= box_.half.y
                        && p.z >= box_.z0 - 1.0
                        && p.z <= box_.z1 - 0.5
                    {
                        return true;
                    }
                }
            }
        }
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Exclusion {
    NotSolid,
    MissingBox,
    MeshRequired,
    UnsafeBox,
    RoadThrough,
}

fn static_box(
    def: &SceneryObject,
    id: i64,
    pos: DVec3,
    heading: f64,
    roads: &RoadPoints,
) -> Result<Obb, Exclusion> {
    if !(def.fixed || def.crash_mode_pole.is_some())
        || def.no_collision
        || def.surface
        || def.render_type.is_ground_layer()
    {
        return Err(Exclusion::NotSolid);
    }
    // The mesh wins over [boundingbox] in the graphical loader. Never
    // silently replace it with an oversized rectangle.
    if def.collision_mesh.is_some() {
        return Err(Exclusion::MeshRequired);
    }
    let Some(bb) = def.bounding_box else {
        return Err(Exclusion::MissingBox);
    };
    if !pos.is_finite()
        || !heading.is_finite()
        || !bb.iter().all(|v| v.is_finite())
        || !(0.05..400.0).contains(&bb[0])
        || !(0.05..400.0).contains(&bb[1])
        || bb[2] <= 0.4
        || bb[5] + bb[2] * 0.5 <= LOW_OBSTACLE
    {
        return Err(Exclusion::UnsafeBox);
    }
    let mut candidate = Obb::from_box(bb, pos, heading);
    if (bb[0] > 3.0 || bb[1] > 3.0) && roads.crosses(&candidate) {
        return Err(Exclusion::RoadThrough);
    }
    candidate.id = id;
    candidate.pole = def.crash_mode_pole;
    Ok(candidate)
}

/// MapIndex adds terrain height to ordinary scenery records. Objects
/// declaring [absheight] must retain their authored Z instead, even when an
/// indexed pose is available (bridge decks and raised collision barriers).
fn collision_position(
    base: DVec2,
    authored_z: f64,
    indexed: Option<DVec3>,
    absolute_height: bool,
    ground: impl FnOnce() -> f64,
) -> DVec3 {
    if absolute_height {
        DVec3::new(base.x, base.y, authored_z)
    } else {
        indexed.unwrap_or_else(|| DVec3::new(base.x, base.y, authored_z + ground()))
    }
}

impl HostWorld {
    /// Rebuild the complete active set of **box** obstacles and publish one
    /// WorldData snapshot. An LRU eviction of the tile parser does not evict
    /// live collision geometry. Call this after generating road lanes for the
    /// same complete tile set; partial calls replace the previous collision set.
    pub fn install_static_box_collisions(
        &mut self,
        keys: &[(i32, i32)],
        lanes: &[Lane],
    ) -> CollisionCounts {
        let mut stats = CollisionCounts::default();
        let road_points = RoadPoints::build(lanes);
        let root = self.data.snapshot().map.root.clone();
        let mut collision = CollisionWorld::default();
        let mut meshes = crate::collision_mesh::MeshCache::new();

        for key in keys.iter().copied().collect::<BTreeSet<_>>() {
            let Some(tile) = self.load_tile(key) else {
                continue;
            };
            stats.loaded_tiles += 1;
            for obj in &tile.objects {
                if obj.parent_id.is_some() {
                    stats.pending_attachments += 1;
                    continue;
                }
                if obj.file.trim().is_empty() {
                    continue;
                }
                let Some(def) = self.scenery(&root, &obj.file) else {
                    stats.missing_definitions += 1;
                    continue;
                };
                let base = DVec2::new(
                    key.0 as f64 * omsi_map::tile_size() + obj.pos[0],
                    key.1 as f64 * omsi_map::tile_size() + obj.pos[1],
                );
                // Index placements include Chrono patches and the original
                // terrain height. Resolve duplicate numeric IDs by tile.
                let indexed = self
                    .index
                    .duplicates
                    .get(&(key, obj.id))
                    .map(|(p, _)| *p)
                    .or_else(|| {
                        self.index
                            .objects
                            .get(&obj.id)
                            .and_then(|(owner, p, _)| (*owner == key).then_some(*p))
                    });
                let pos =
                    collision_position(base, obj.pos[2], indexed, def.absolute_height(), || {
                        self.terrain_height(base.x, base.y)
                            .map(f64::from)
                            .unwrap_or(0.0)
                    });
                // Decorative and terrain-render objects are not rigid
                // obstacles even when their map placement is tilted.
                if !(def.fixed || def.crash_mode_pole.is_some())
                    || def.no_collision
                    || def.surface
                    || def.render_type.is_ground_layer()
                {
                    continue;
                }
                // Tilted collision meshes must be transformed in full 3D:
                // a yaw-only approximation would make a sloping bridge solid.
                if obj.rot[1] != 0.0 || obj.rot[2] != 0.0 {
                    stats.pending_tilted += 1;
                    continue;
                }
                match static_box(&def, obj.id, pos, obj.rot[0], &road_points) {
                    Ok(b) => collision.add(b),
                    Err(Exclusion::MeshRequired) => {
                        if let Some(shape) = meshes.load(&def) {
                            if !shape.parts.is_empty() {
                                collision.add_mesh(omsi_sim::collision::MeshObstacle::new(
                                    shape, pos, obj.rot[0], obj.id,
                                ));
                            }
                        } else {
                            stats.pending_meshes += 1;
                        }
                    }
                    Err(Exclusion::RoadThrough) => stats.avoided_road_walls += 1,
                    Err(Exclusion::NotSolid | Exclusion::MissingBox | Exclusion::UnsafeBox) => {}
                }
            }
        }
        stats.static_boxes = collision.boxes.len();
        stats.static_meshes = collision.meshes.len();
        self.data.update(|data| {
            let lists = Arc::make_mut(&mut data.lists);
            lists.collision = Arc::new(collision);
            lists.generation = lists.generation.wrapping_add(1);
        });
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_def(bb: [f32; 6]) -> SceneryObject {
        SceneryObject {
            fixed: true,
            bounding_box: Some(bb),
            ..Default::default()
        }
    }

    #[test]
    fn publishes_real_boxes_with_ids_without_render_assets() {
        let box_ = static_box(
            &box_def([1.0, 2.0, 2.0, 0.0, 0.0, 1.0]),
            45,
            DVec3::new(10.0, 20.0, 0.0),
            90.0,
            &RoadPoints::default(),
        )
        .unwrap();
        assert_eq!(box_.id, 45);
        assert!((box_.center - DVec2::new(10.0, 20.0)).length() < 1e-6);
    }

    #[test]
    fn rejects_decals_nonfixed_and_mesh_fallbacks() {
        let mut def = box_def([1.0, 1.0, 0.2, 0.0, 0.0, 0.1]);
        let at = DVec3::ZERO;
        assert_eq!(
            static_box(&def, 1, at, 0.0, &RoadPoints::default()).unwrap_err(),
            Exclusion::UnsafeBox
        );
        def.bounding_box = Some([2.0, 2.0, 2.0, 0.0, 0.0, 1.0]);
        def.collision_mesh = Some("bridge.o3d".into());
        assert_eq!(
            static_box(&def, 1, at, 0.0, &RoadPoints::default()).unwrap_err(),
            Exclusion::MeshRequired
        );
        def.collision_mesh = None;
        def.fixed = false;
        assert_eq!(
            static_box(&def, 1, at, 0.0, &RoadPoints::default()).unwrap_err(),
            Exclusion::NotSolid
        );
    }

    #[test]
    fn absolute_collision_height_is_not_shifted_by_indexed_terrain() {
        let base = DVec2::new(100.0, 200.0);
        let indexed = Some(DVec3::new(100.0, 200.0, 35.0));
        assert_eq!(
            collision_position(base, 5.0, indexed, true, || panic!("terrain sampled")),
            DVec3::new(100.0, 200.0, 5.0),
        );
        assert_eq!(
            collision_position(base, 5.0, indexed, false, || panic!("terrain sampled")),
            DVec3::new(100.0, 200.0, 35.0),
        );
        assert_eq!(
            collision_position(base, 5.0, None, false, || 30.0),
            DVec3::new(100.0, 200.0, 35.0),
        );
    }

    #[test]
    fn road_passage_does_not_become_a_wall_or_hide_a_bridge_above_it() {
        let mut roads = RoadPoints::default();
        roads
            .cells
            .insert((0, 0), vec![DVec3::new(10.0, 10.0, 1.0)]);
        let def = box_def([8.0, 8.0, 3.0, 0.0, 0.0, 1.5]);
        let pos = DVec3::new(10.0, 10.0, 0.0);
        assert_eq!(
            static_box(&def, 7, pos, 0.0, &roads).unwrap_err(),
            Exclusion::RoadThrough
        );
        let high = DVec3::new(10.0, 10.0, 12.0);
        assert!(static_box(&def, 7, high, 0.0, &roads).is_ok());
    }

    #[test]
    fn incomplete_world_reports_unresolved_mesh_and_attachment_work() {
        assert!(
            !CollisionCounts {
                static_boxes: 8,
                ..Default::default()
            }
            .incomplete()
        );
        assert!(
            CollisionCounts {
                pending_meshes: 1,
                ..Default::default()
            }
            .incomplete()
        );
        assert!(
            CollisionCounts {
                pending_attachments: 1,
                ..Default::default()
            }
            .incomplete()
        );
    }
}
