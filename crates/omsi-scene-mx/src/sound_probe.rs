//! What the world tells a vehicle's scripts of the ground: the surface under a tyre.

use super::*;

impl World {
    /// OMSI's `[surface]` id under a tyre whose contact is at `at` (see
    /// `TileSurface::surface_under`); `None` where no tile is loaded.
    pub fn surface_under(&self, at: DVec3) -> Option<u8> {
        let key = tile_key(at.x, at.y);
        let (lx, ly) = ((at.x - key.0 as f64 * tile_size()) as f32, (at.y - key.1 as f64 * tile_size()) as f32);
        let data = self.data.peek();
        let terrain = data.ground.terrains.get(&key).map(|t| t.sample(lx, ly));
        let s = data.ground.surfaces.get(&key)?;
        Some(s.surface_under(lx, ly, at.z as f32, terrain))
    }
}

impl World {
    /// What stands around `ear` for the ambience: the loaded tiles within earshot of a church
    /// bell, read from their sound data (see [`omsi_soundscape::place`]).
    pub fn sound_place(&self, ear: DVec3) -> omsi_soundscape::place::Place {
        let mut g = omsi_soundscape::place::Gather::new(ear);
        let reach = (omsi_soundscape::place::SPOT_REACH / tile_size()).ceil() as i32;
        let key = tile_key(ear.x, ear.y);
        let data = self.data.peek();
        let surfaces = &data.ground.surfaces;
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let k = (key.0 + dx, key.1 + dy);
                if let Some(s) = surfaces.get(&k) {
                    g.tile(&s.sound, DVec3::new(k.0 as f64 * tile_size(), k.1 as f64 * tile_size(), 0.0));
                }
            }
        }
        g.finish()
    }
}
