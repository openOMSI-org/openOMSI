//! Bounded caches for script-switched vehicle pictures.
use super::*;

const KEEP_PICTURES: usize = 32;
const KEEP_BYTES: u64 = 16 * 1024 * 1024;

impl FreeTex {
    pub fn load_picture(
        &mut self,
        renderer: &Renderer,
        scene: &mut Scene,
        path: PathBuf,
    ) -> Option<TextureId> {
        let mut player = self.player_textures.lock();
        let mut shared = self.shared.lock();
        let max_dimension = renderer.device.limits().max_texture_dimension_2d;
        let id = if let Some(e) = shared.get_mut(&path) {
            e.1 += 1;
            if self.reloads.snapshot(e.0).is_none() {
                if let Some((data, _)) = self.textures.get_gpu_fast(&path) {
                    self.reloads.remember(e.0, data);
                }
            }
            e.0
        } else {
            let (img, worth) = self.textures.get_gpu_fast(&path)?;
            let id = if let Some(id) = self.spare.pop() {
                renderer.replace_texture(scene, id, &img);
                id
            } else {
                renderer.add_texture_data(scene, &img)
            };
            self.reloads.remember(id, img.clone());
            if worth {
                self.wants_upgrade.lock().push(path.clone());
            }
            attach_pbr(renderer, scene, &path, id);
            shared.insert(path.clone(), (id, 1));
            id
        };
        if self.is_player {
            player.insert(path.clone());
        }
        self.reloads
            .watch(path.clone(), id, &self.shared, max_dimension);
        self.held.push(path);
        Some(id)
    }

    fn over_limit(&self, scene: &Scene) -> bool {
        self.pictures.len() > KEEP_PICTURES || {
            let current_bytes = self.image.map(|id| scene.texture_bytes_of(id)).unwrap_or(0);
            let keep_bytes = KEEP_BYTES.max(current_bytes.saturating_mul(2));
            let shared = self.shared.lock();
            self.held
                .iter()
                .filter_map(|p| shared.get(p))
                .map(|(id, _)| scene.texture_bytes_of(*id))
                .sum::<u64>()
                > keep_bytes
        }
    }

    fn trim(&mut self, renderer: &Renderer, scene: &mut Scene, materials: &[MaterialId]) {
        let mut player = self.player_textures.lock();
        let mut shared = self.shared.lock();
        let keep = |id| {
            self.image == Some(id)
                || materials
                    .iter()
                    .any(|m| scene.materials.get(*m).is_some_and(|m| m.uses_texture(id)))
        };
        self.pictures
            .retain(|name, id| self.current.as_ref() == Some(name) || id.is_some_and(&keep));
        let mut gone = Vec::new();
        self.held.retain(|path| {
            let Some(e) = shared.get_mut(path) else {
                return false;
            };
            if keep(e.0) {
                return true;
            }
            e.1 = e.1.saturating_sub(1);
            if e.1 == 0 {
                gone.push(e.0);
                shared.remove(path);
                self.textures.release(path);
                player.remove(path);
            }
            false
        });
        for id in gone {
            self.reloads.forget(id);
            renderer.free_texture(scene, id);
            self.spare.push(id);
        }
    }
}

impl VariantSlot {
    pub fn trim_free_textures(&mut self, renderer: &Renderer, scene: &mut Scene) {
        if self.free_built.len() <= KEEP_PICTURES && !self.free.iter().any(|f| f.over_limit(scene))
        {
            return;
        }
        let shown = |b: &FreeBuilt| {
            b.materials()
                .any(|m| m == self.base || m == self.item || self.more.contains(&m))
        };
        let mut materials: Vec<_> = self
            .free_built
            .values()
            .filter(|b| shown(b))
            .flat_map(FreeBuilt::materials)
            .collect();
        materials.extend([self.base, self.item]);
        materials.extend(&self.more);
        self.free_built.retain(|_, b| {
            if shown(b) {
                return true;
            }
            for m in b.materials().filter(|m| !materials.contains(m)) {
                renderer.free_material(scene, m);
            }
            false
        });
        // Drop materials before their old pictures; bind groups otherwise keep the pixels alive.
        for f in &mut self.free {
            f.trim(renderer, scene, &materials);
        }
    }
}

#[cfg(test)]
#[path = "vehicle_freetex_tests.rs"]
mod tests;

impl World {
    /// Cache state and reload ownership shared by free textures in this world: one
    /// `[matl_freetex]` of a vehicle slot, standing in for `key` (`diffuse`: the slot's own
    /// texture), on the halves its bits name (see `free_texture_defs`).
    pub(super) fn free_texture_slot(
        &self,
        var: String,
        key: Option<TextureId>,
        diffuse: bool,
        halves: u32,
        dirs: &[PathBuf],
    ) -> FreeTex {
        FreeTex {
            player_textures: self.player_textures.clone(),
            is_player: false,
            remote: None,
            remote_key: String::new(),
            image: None,
            reloads: self.freetex_reloads.clone(),
            var,
            diffuse,
            key,
            halves,
            dirs: dirs.to_vec(),
            textures: self.textures.clone(),
            pictures: HashMap::new(),
            spare: Vec::new(),
            current: None,
            shared: self.vehicle_textures.clone(),
            held: Vec::new(),
            wants_upgrade: self.freetex_upgrades.clone(),
        }
    }
}
