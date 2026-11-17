use super::*;
use crate::collision::{CollisionWorld, Obb};
use std::path::Path;
use std::sync::Arc;

/// Ground at `pavement` everywhere, seen (as `WorldData::walk_height_near`) only up to a step
/// over whoever asks; `None`: nothing loaded.
#[derive(Default)]
struct Pavement {
    pavement: Option<f64>,
    collision: Arc<CollisionWorld>,
    parked: Arc<Vec<Obb>>,
    objects: crate::world_data::ObjectPlaces,
}

impl World for Pavement {
    fn root(&self) -> &Path {
        Path::new("/nonexistent")
    }
    fn map_dir(&self) -> &Path {
        self.root()
    }
    fn walk_height(&self, _: f64, _: f64) -> Option<f64> {
        self.pavement
    }
    fn walk_height_near(&self, _: f64, _: f64, near: f64) -> Option<f64> {
        self.pavement.filter(|z| *z < near + 0.5)
    }
    fn has_ground(&self, _: f64, _: f64) -> bool {
        self.pavement.is_some()
    }
    fn stop_exit_weight(&self, _: i64) -> f32 {
        0.5
    }
    fn stop_enter(&self, _: i64) -> (f32, f32) {
        (1.0, 0.0)
    }
    fn stop_side(&self, _: i64) -> f32 {
        1.0
    }
    fn stop_length(&self, _: i64) -> f32 {
        30.0
    }
    fn bus_stops(&self) -> &[(i64, DVec3, f64, String)] {
        &[]
    }
    fn waiting_places(&self) -> &[(i64, DVec3, f64, f32)] {
        &[]
    }
    fn object_positions(&self) -> &crate::world_data::ObjectPlaces {
        &self.objects
    }
    fn collision(&self) -> &Arc<CollisionWorld> {
        &self.collision
    }
    fn parked_boxes(&self) -> &Arc<Vec<Obb>> {
        &self.parked
    }
    fn tiles_generation(&self) -> u64 {
        0
    }
}

#[test]
fn a_waiting_place_is_lifted_onto_the_pavement_but_never_lowered() {
    let at = |pavement| raised_by_ground(&Pavement { pavement, ..Default::default() }, 10.0, 0.0, 0.0);
    assert_eq!(at(Some(10.235)), 10.235, "the pavement 23.5 cm above the place");
    assert_eq!(at(Some(9.85)), 10.0, "the road below the place");
    assert_eq!(at(Some(10.8)), 10.0, "a face more than a step above");
    assert_eq!(at(None), 10.0, "nothing loaded");
}
