//! The plants of a season's phase in two looks (`omsi_texture::season_mix`): which objects
//! are plants, and their copies with the other season's textures.
//!
//! OMSI's season folders hold per texture file whatever the content's author painted for
//! the season: the trees' foliage in `Fall` and `Spring`, bare crowns in `Winter`, but
//! also snow on a roof in `WinterSnow` or, rarely, a building's own winter picture. Only
//! plants mix: a `[tree]` (its card's picture) and an object filed as a plant - the same
//! test the windy trees use (`vegetation_give_of`: its `[groups]`, else the words of its
//! file's name), and of those only the ones with a texture that differs between the two
//! looks. Houses, roads and everything else show the look that prevails.
use super::*;

/// The textures the second look costs (distinct files on top of the map's season) and the
/// object types that have one, for the log (kept by the world: `World::season_extra`).
#[derive(Default)]
pub(crate) struct SeasonExtra {
    textures: hashbrown::HashSet<PathBuf>,
    types: usize,
}

/// Count the files of the other look and say so (each new one is a texture more to load).
fn count_extra(extra: &Mutex<SeasonExtra>, paths: impl IntoIterator<Item = PathBuf>, what: &str) {
    let mut extra = extra.lock();
    let types = extra.types;
    let set = &mut extra.textures;
    let before = set.len();
    set.extend(paths);
    if set.len() > before {
        log::info!(
            "season mix: {what} - {} textures more, {} in all for {} object types",
            set.len() - before,
            set.len(),
            types
        );
    }
}

/// Whether an object type is a plant whose look may differ from its neighbours'.
pub(super) fn is_plant(sco: &SceneryObject) -> bool {
    sco.tree.is_some() || vegetation_give_of(sco).is_some()
}

/// The stable number of a placed object: its id and tile (the same in every session and
/// for every LAN player on the map).
fn plant_key(key: i64, tx: i32, ty: i32) -> u64 {
    (key as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((tx as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
        .wrapping_add((ty as i64 as u64).wrapping_mul(0x1656_67B1_9E37_79F9))
}

impl World {
    /// The type a placed plant takes in a season's phase: `ot` itself, or its copy in the
    /// other look when the plant `key` (in tile `tx`, `ty`) has that look.
    pub(super) fn plant_look(&self, file: &str, ot: Arc<ObjectType>, key: i64, tx: i32, ty: i32) -> Arc<ObjectType> {
        if ot.sco.tree.is_some() || !is_plant(&ot.sco) {
            // ([tree] cards take their picture per tree: see `tree_look_texture`)
            return ot;
        }
        let Some(look) = omsi_texture::mixed_look(plant_key(key, tx, ty)) else { return ot };
        self.object_type_look(file, None, Some(&look)).unwrap_or(ot)
    }

    /// The picture of a `[tree]` card in the plant's own look: the file of the other
    /// season named in full, or `texture` as it is.
    pub(super) fn tree_look_texture(&self, ot: &ObjectType, texture: &str, key: i64, pos: DVec3) -> String {
        if omsi_texture::season_mix().is_none() {
            return texture.to_string();
        }
        let tile = |v: f64| (v / tile_size()).floor() as i32;
        let Some(look) = omsi_texture::mixed_look(plant_key(key, tile(pos.x), tile(pos.y))) else {
            return texture.to_string();
        };
        let dirs = ot.texture_dirs(&self.root);
        let dirs_ref: Vec<&Path> = dirs.iter().map(|d| d.as_path()).collect();
        let (Some(now), Some(then)) = (
            omsi_texture::find_texture(texture, &dirs_ref),
            omsi_texture::find_texture_in_look(texture, &dirs_ref, look.as_deref()),
        ) else {
            return texture.to_string();
        };
        if now == then {
            return texture.to_string();
        }
        count_extra(&self.season_extra, [then.clone()], &format!("tree {texture} also in {}", look.as_deref().unwrap_or("summer")));
        then.to_string_lossy().into_owned()
    }

    /// Give a freshly loaded type the textures of the season `look` (`None`: summer's),
    /// each named by its full path so that the map's season does not apply to it again.
    /// None when none of its textures looks different then.
    pub(super) fn retexture_for_look(&self, ot: &mut ObjectType, look: Option<&str>) -> Option<()> {
        let dirs = ot.texture_dirs(&self.root);
        let dirs_ref: Vec<&Path> = dirs.iter().map(|d| d.as_path()).collect();
        let mut names: HashMap<String, Option<String>> = HashMap::new();
        let mut renamed = |name: &str| -> Option<String> {
            if name.trim().is_empty() {
                return None;
            }
            names
                .entry(name.to_ascii_lowercase())
                .or_insert_with(|| {
                    let now = omsi_texture::find_texture(name, &dirs_ref)?;
                    let then = omsi_texture::find_texture_in_look(name, &dirs_ref, look)?;
                    (now != then).then(|| then.to_string_lossy().into_owned())
                })
                .clone()
        };
        let mut changed = Vec::new();
        for (_, mats, overrides) in ot.meshes.iter_mut().chain(ot.lower_lods.iter_mut().flat_map(|l| l.1.iter_mut())) {
            for m in mats.iter_mut() {
                if let Some(n) = renamed(&m.texture) {
                    m.texture = n.clone();
                    changed.push(PathBuf::from(n));
                }
            }
            // (the `[matl]` blocks find their slot by the same name)
            for o in overrides.iter_mut() {
                if let Some(n) = renamed(&o.texture) {
                    o.texture = n;
                }
            }
        }
        if changed.is_empty() {
            return None;
        }
        self.season_extra.lock().types += 1;
        count_extra(&self.season_extra, changed, &format!("{} also in {}", ot.sco.path.display(), look.unwrap_or("summer")));
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plant_keys_differ_by_tile() {
        assert_ne!(plant_key(5, 0, 0), plant_key(5, 1, 0));
        assert_ne!(plant_key(5, 0, 0), plant_key(5, 0, 1));
        assert_eq!(plant_key(5, 2, -3), plant_key(5, 2, -3));
    }
}
