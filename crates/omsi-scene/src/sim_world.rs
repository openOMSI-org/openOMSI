//! What the simulation asks of the loaded world: omsi-sim's traits for the people
//! (`omsi_sim::people::World`) and the timetable (`TimetableWorld`), answered by [`World`].

use crate::World;
use glam::DVec3;
use hashbrown::HashMap;
use omsi_sim::timetable_run::TimetableWorld;
use parking_lot::MutexGuard;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What the people need of the loaded world (see `omsi_sim::people::World`).
impl omsi_sim::people::World for World {
    fn root(&self) -> &Path {
        &self.root
    }
    fn map_dir(&self) -> &Path {
        &self.map_dir
    }
    fn walk_height(&self, x: f64, y: f64) -> Option<f64> {
        World::walk_height(self, x, y)
    }
    fn walk_height_near(&self, x: f64, y: f64, near: f64) -> Option<f64> {
        World::walk_height_near(self, x, y, near)
    }
    fn has_ground(&self, x: f64, y: f64) -> bool {
        World::has_ground(self, x, y)
    }
    fn stop_exit_weight(&self, id: i64) -> f32 {
        World::stop_exit_weight(self, id)
    }
    fn stop_enter(&self, id: i64) -> (f32, f32) {
        World::stop_enter(self, id)
    }
    fn stop_side(&self, id: i64) -> f32 {
        World::stop_side(self, id)
    }
    fn stop_length(&self, id: i64) -> f32 {
        World::stop_length(self, id)
    }
    fn bus_stops(&self) -> MutexGuard<'_, Vec<(i64, DVec3, f64, String)>> {
        self.bus_stops.lock()
    }
    fn waiting_places(&self) -> MutexGuard<'_, Vec<(i64, DVec3, f64, f32)>> {
        self.waiting_places.lock()
    }
    fn object_positions(&self) -> MutexGuard<'_, HashMap<i64, (DVec3, [f64; 3])>> {
        self.object_positions.lock()
    }
    fn collision(&self) -> MutexGuard<'_, Arc<omsi_sim::collision::CollisionWorld>> {
        self.collision.lock()
    }
    fn parked_boxes(&self) -> MutexGuard<'_, Arc<Vec<omsi_sim::collision::Obb>>> {
        self.parked_boxes.lock()
    }
    fn tiles_generation(&self) -> u64 {
        self.tiles_generation.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// What the timetable needs of the loaded world (see `omsi_sim::timetable_run::TimetableWorld`).
impl TimetableWorld for World {
    fn root(&self) -> &Path {
        &self.root
    }
    fn map_dir(&self) -> &Path {
        &self.map_dir
    }
    fn chrono_dirs(&self) -> Vec<PathBuf> {
        self.chrono_dirs.read().clone()
    }
    fn ailists(&self) -> &omsi_map::AiLists {
        &self.ailists
    }
    fn raw_tiles(&self) -> Vec<(i32, i32)> {
        self.global.raw_tiles.clone()
    }
    fn has_tile(&self, key: (i32, i32)) -> bool {
        World::has_tile(self, key)
    }
    fn date(&self) -> i32 {
        self.date
    }
    fn object_positions(&self) -> MutexGuard<'_, HashMap<i64, (glam::DVec3, [f64; 3])>> {
        self.object_positions.lock()
    }
    fn stop_side(&self, id: i64) -> f32 {
        World::stop_side(self, id)
    }
    fn object_of_tile(&self, tile: Option<usize>, id: i64) -> Option<(glam::DVec3, [f64; 3])> {
        World::object_of_tile(self, tile, id)
    }
}
