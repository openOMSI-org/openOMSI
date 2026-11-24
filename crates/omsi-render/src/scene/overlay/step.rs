use super::{Flat, ObjectRun, normal_key};
use std::sync::Arc;

impl ObjectRun {
    pub(super) fn remember_unstepped_flats(&mut self, flats: &[&Flat], mesh: usize) {
        for flat in flats {
            self.by_normal
                .entry(normal_key(flat.normal))
                .or_default()
                .push(self.flats.len());
            self.flats.push((**flat, 0));
            self.flat_caps.push(Arc::from([]));
            self.flat_meshes.push(mesh);
        }
    }
}
