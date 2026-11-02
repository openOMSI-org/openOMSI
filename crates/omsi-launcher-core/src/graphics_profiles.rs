use super::*;

/// The settings a graphics profile holds: what the Graphics tab shows, except the machine's
/// own (fullscreen, graphics API).
pub const GRAPHICS_PROFILE_KEYS: [&str; 44] = [
    "graphics", "msaa", "render_scale", "upscaler", "fsr_quality", "fsr_sharpness", "anisotropy", "shadow_size", "ssao", "shadows", "shadow_casters", "detail_textures", "night_brightness", "led_glow", "led_mips", "shadow_blobs", "lamp_shadows", "reflections", "road_glare", "traffic_headlights", "clouds", "cloud_quality", "street_light", "rain_quality", "vehicle_smoke", "puddles", "windy_trees", "road_snow", "ray_tracing", "light_shadows", "cloud_shadows", "light_glow",
    "vsync", "max_fps", "view_distance", "max_obj_dist", "min_obj_size", "mirror_size", "mirror_refresh", "mirror_quality", "mirror_hud_every_frame", "video_memory", "texture_compression", "gpu_texture_compression",
];

/// The folder of the saved graphics profiles, a file each (`~/.openomsi/graphics_profiles/`).
pub(crate) fn graphics_profiles_dir(base: &Path) -> PathBuf {
    base.join("graphics_profiles")
}

/// What 0.3.44 and before kept them in, all in one file.
pub(crate) fn old_graphics_profiles_path(base: &Path) -> PathBuf {
    base.join("graphics_profiles.json")
}

/// The file of profile `name`: the name with what a file name cannot hold taken out, and a
/// number worked out from the whole name - "a/b" and "a_b", or "High" and "high" on a file
/// system that does not tell capitals apart, are two profiles.
pub(crate) fn graphics_profile_file(base: &Path, name: &str) -> PathBuf {
    let readable: String = name
        .chars()
        .map(|c| if c.is_control() || ['<', '>', ':', '"', '/', '\\', '|', '?', '*'].contains(&c) { '_' } else { c })
        .collect::<String>()
        .trim_matches(|c: char| c == '.' || c.is_whitespace())
        .to_string();
    let hash = name.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
    graphics_profiles_dir(base).join(format!("{readable}-{:08x}.json", hash as u32))
}

/// Write `profile` as `name`'s file: whole or not at all (written beside it and moved into
/// place, so that a game closed half way leaves the old one).
pub(crate) fn write_graphics_profile(base: &Path, name: &str, profile: &Value) -> Result<()> {
    let path = graphics_profile_file(base, name);
    std::fs::create_dir_all(graphics_profiles_dir(base))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&serde_json::json!({ "name": name, "graphics": profile }))?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// The profiles of the single file 0.3.44 and before kept: each into a file of its own, and
/// the old file put aside (`graphics_profiles.json.migrated`) once they are all there. A file
/// that cannot be read is left where it is, to be looked at.
pub(crate) fn migrate_graphics_profiles(base: &Path) {
    let old = old_graphics_profiles_path(base);
    let Some(all) = std::fs::read_to_string(&old).ok().and_then(|t| serde_json::from_str::<std::collections::BTreeMap<String, Value>>(&t).ok()) else { return };
    if all.iter().all(|(name, profile)| graphics_profile_file(base, name).exists() || write_graphics_profile(base, name, profile).is_ok()) {
        let _ = std::fs::rename(&old, old.with_extension("json.migrated"));
    }
}

/// The saved graphics profiles by name (`~/.openomsi/graphics_profiles/`, a file each). A
/// file that cannot be read is left out; the others are not lost with it.
pub fn graphics_profiles() -> std::collections::BTreeMap<String, Value> {
    graphics_profiles_in(&data_dir())
}

pub(crate) fn graphics_profiles_in(base: &Path) -> std::collections::BTreeMap<String, Value> {
    migrate_graphics_profiles(base);
    let mut all = std::collections::BTreeMap::new();
    let Ok(dir) = std::fs::read_dir(graphics_profiles_dir(base)) else { return all };
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(file) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { continue };
        if let (Some(name), Some(profile)) = (file.get("name").and_then(Value::as_str), file.get("graphics")) {
            let own = graphics_profile_file(base, name);
            let is_own = path.file_name().zip(own.file_name()).is_some_and(|(a, b)| a.eq_ignore_ascii_case(b));
            if profile.is_object() && (is_own || (!own.exists() && std::fs::rename(&path, &own).is_ok())) {
                all.insert(name.to_string(), profile.clone());
            }
        }
    }
    all
}

/// Keep the graphics of `settings` as profile `name` (an existing one of that name is
/// replaced), in a file of its own. Returns the name as kept.
pub fn save_graphics_profile(name: &str, settings: &Value) -> Result<String> {
    save_graphics_profile_in(&data_dir(), name, settings)
}

pub(crate) fn save_graphics_profile_in(base: &Path, name: &str, settings: &Value) -> Result<String> {
    let name: String = name.chars().filter(|c| !c.is_control()).collect::<String>().trim().chars().take(40).collect();
    if name.is_empty() {
        return Err(anyhow!("Give the profile a name."));
    }
    let mut profile = serde_json::Map::new();
    for k in GRAPHICS_PROFILE_KEYS {
        if let Some(x) = settings.get(k) {
            profile.insert(k.to_string(), x.clone());
        }
    }
    migrate_graphics_profiles(base);
    write_graphics_profile(base, &name, &Value::Object(profile))?;
    Ok(name)
}

/// Remove profile `name`.
pub fn delete_graphics_profile(name: &str) -> Result<()> {
    delete_graphics_profile_in(&data_dir(), name)
}

pub(crate) fn delete_graphics_profile_in(base: &Path, name: &str) -> Result<()> {
    migrate_graphics_profiles(base);
    match std::fs::remove_file(graphics_profile_file(base, name)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Put a profile's values into the page's `settings` (only the keys a profile may hold).
pub fn apply_graphics_profile(profile: &Value, settings: &mut Value) {
    for k in GRAPHICS_PROFILE_KEYS {
        if let Some(x) = profile.get(k) {
            settings[k] = x.clone();
        }
    }
}
