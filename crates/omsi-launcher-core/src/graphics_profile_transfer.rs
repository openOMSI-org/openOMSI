//! Portable graphics profiles share the existing on-disk format and allowlist.
use super::graphics_profiles::*;
use super::*;
use std::io::{Read, Write};

const MAX_PROFILE_BYTES: u64 = 64 * 1024;

fn clean_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(40)
        .collect::<String>()
        .trim()
        .to_string()
}

fn checked_graphics(value: &Value) -> Result<Value> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!(i18n::tr("Invalid graphics profile file.")))?;
    // Round-trip defaults so the schema also includes fields supplied by the settings writer.
    let defaults = settings_from_text(Some(&settings_to_text(&settings_from_text(None), None)));
    let mut graphics = serde_json::Map::new();
    for key in GRAPHICS_PROFILE_KEYS {
        let Some(value) = object.get(key) else {
            continue;
        };
        // Numeric selections have been stored both as JSON numbers and strings. Reflections
        // used to be a toggle; retain compatibility with profiles saved by older releases.
        let normalized = match &defaults[key] {
            Value::Bool(_) if value.is_boolean() => Some(value.clone()),
            Value::Number(_) => {
                if value.is_number() || (key == "reflections" && value.is_boolean()) {
                    Some(value.clone())
                } else {
                    // The settings writer accepts numeric strings for integer choices but
                    // requires JSON numbers for floats. Normalize both before saving.
                    value
                        .as_str()
                        .and_then(|s| s.parse::<serde_json::Number>().ok())
                        .map(Value::Number)
                }
            }
            Value::String(_) => {
                let text = value
                    .as_str()
                    .is_some_and(|s| s.len() <= 128 && !s.chars().any(char::is_control));
                let numeric_choice = ["render_scale", "view_distance", "max_obj_dist"]
                    .contains(&key)
                    && value.is_number();
                (text || numeric_choice).then(|| value.clone())
            }
            _ => None,
        };
        let normalized = normalized.ok_or_else(|| {
            anyhow!(i18n::trf(
                "Invalid graphics profile value: {key}.",
                &[("key", &key)]
            ))
        })?;
        graphics.insert(key.to_string(), normalized);
    }
    if graphics.is_empty() {
        return Err(anyhow!(i18n::tr(
            "The file contains no supported graphics settings."
        )));
    }
    Ok(Value::Object(graphics))
}

/// Import a shared file without applying it or replacing an existing profile.
pub fn import_graphics_profile(path: &Path) -> Result<String> {
    import_graphics_profile_in(&data_dir(), path)
}

pub(crate) fn import_graphics_profile_in(base: &Path, path: &Path) -> Result<String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_PROFILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PROFILE_BYTES {
        return Err(anyhow!(i18n::tr(
            "Graphics profile files must not exceed 64 KiB."
        )));
    }
    let file: Value = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow!(i18n::tr("Invalid graphics profile file.")))?;
    let name = clean_name(
        file.get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!(i18n::tr("Invalid graphics profile file.")))?,
    );
    if name.is_empty() {
        return Err(anyhow!(i18n::tr("Give the profile a name.")));
    }
    let graphics = checked_graphics(&file["graphics"])?;
    let profiles = graphics_profiles_in(base);
    let mut kept = name.clone();
    let mut number = 2;
    while profiles.contains_key(&kept) || graphics_profile_file(base, &kept).exists() {
        let suffix = format!(" ({number})");
        kept = format!(
            "{}{}",
            name.chars()
                .take(40 - suffix.chars().count())
                .collect::<String>(),
            suffix
        );
        number += 1;
    }
    save_graphics_profile_in(base, &kept, &graphics)
}

/// Export only portable graphics keys, using the same format as saved profiles.
pub fn export_graphics_profile(name: &str, path: &Path) -> Result<()> {
    export_graphics_profile_in(&data_dir(), name, path)
}

pub(crate) fn export_graphics_profile_in(base: &Path, name: &str, path: &Path) -> Result<()> {
    let profiles = graphics_profiles_in(base);
    let profile = profiles
        .get(name)
        .ok_or_else(|| anyhow!(i18n::tr("The graphics profile no longer exists.")))?;
    let graphics = checked_graphics(profile)?;
    let text = serde_json::to_string_pretty(&json!({"name": name, "graphics": graphics}))?;
    // Keep an existing export intact if writing or replacing it fails. The temporary
    // file is beside the destination, so renaming stays on the same filesystem.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temporary = path.with_file_name(format!(
        ".openomsi-graphics-{}-{nonce}.tmp",
        std::process::id()
    ));
    let mut file = std::fs::File::create_new(&temporary)?;
    let result = (|| {
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result?;
    Ok(())
}
