//! The plugin list of the launcher (#3919): the Lua, compiled (`.oop`) and OMSI (`.opl`)
//! plugins of the game's `plugins` folders, each switched on or off without moving its
//! files, what became of it in the last session (loaded, a syntax error, switched off for its
//! errors), and the settings it declared with `plugin.settings`.
//!
//! The game reads the same files: `~/.openomsi/plugins-off.txt` (one name a line) before it
//! loads the plugins, and writes `~/.openomsi/plugins-status.json` after (see
//! `omsi_plugin::status`); a plugin's settings are its data folder's `settings.json`, and
//! `settings.schema.json` beside it says what they are.

use anyhow::{anyhow, Result};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// (`omsi_plugin::status::OFF_FILE` / `STATUS_FILE`: this crate does not depend on the game's
/// plugin crate.)
const OFF_FILE: &str = "plugins-off.txt";
const STATUS_FILE: &str = "plugins-status.json";
const SETTINGS_FILE: &str = "settings.json";
const SCHEMA_FILE: &str = "settings.schema.json";

/// One plugin on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    /// The name the game knows it by: a `.lua`'s stem, a folder's name (`<folder>/main.lua`),
    /// a `.oop`'s or `.opl`'s stem.
    pub name: String,
    pub kind: &'static str,
    /// The file (or the folder of a `main.lua` plugin).
    pub path: PathBuf,
    /// Its data folder (`settings.json`), for a Lua plugin.
    pub data_dir: Option<PathBuf>,
}

fn child(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name)))
}

fn key(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

/// The plugins of one `plugins` folder, as the game finds them (`omsi_plugin::lua::find_lua`,
/// `oop::find_oop`, the `.opl` files).
pub fn find(dir: &Path) -> Vec<Found> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_uppercase()).unwrap_or_default());
    let stem = |p: &Path| p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = |p: &Path, e: &str| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(e));
    let mut out = Vec::new();
    for p in entries {
        let file = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if file.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            if child(&p, "main.lua").is_some_and(|m| m.is_file()) {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                out.push(Found { name, kind: "lua", data_dir: Some(p.join("data")), path: p });
            }
        } else if ext(&p, "lua") && !file.ends_with(".save.lua") {
            out.push(Found { name: stem(&p), kind: "lua", data_dir: Some(p.with_extension("data")), path: p });
        } else if ext(&p, "oop") {
            out.push(Found { name: stem(&p), kind: "oop", data_dir: Some(p.with_extension("data")), path: p });
        } else if ext(&p, "opl") {
            out.push(Found { name: stem(&p), kind: "opl", data_dir: None, path: p });
        }
    }
    out
}

/// The `plugins` folders the game reads: the content folder's, then the OMSI 2 folder's.
pub fn dirs() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for root in [crate::content_dir(), crate::root().ok()].into_iter().flatten() {
        if let Some(d) = child(&root, "plugins").filter(|d| d.is_dir()) {
            if !out.contains(&d) {
                out.push(d);
            }
        }
    }
    out
}

/// The plugins the player switched off.
pub fn off_list(data: &Path) -> Vec<String> {
    std::fs::read_to_string(data.join(OFF_FILE)).map(|t| t.lines().map(key).filter(|l| !l.is_empty() && !l.starts_with('#')).collect()).unwrap_or_default()
}

/// Switch the plugin `name` on or off for the next game.
pub fn set_enabled_in(data: &Path, name: &str, on: bool) -> Result<()> {
    let k = key(name);
    if k.is_empty() || k.contains(['\n', '\r']) {
        return Err(anyhow!("no plugin name"));
    }
    let mut off = off_list(data);
    off.retain(|n| *n != k);
    if !on {
        off.push(k);
    }
    off.sort();
    let text = format!("# Plugins switched off in the launcher (one name a line); the game leaves them out.\n{}", off.iter().map(|n| format!("{n}\n")).collect::<String>());
    std::fs::create_dir_all(data)?;
    std::fs::write(data.join(OFF_FILE), text)?;
    Ok(())
}

fn read_json(p: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

/// The list for the launcher's page: every plugin with `{name, title, kind, path, enabled,
/// state, error, settings, values}`; `state` is what the last session made of it
/// (`loaded`, `failed`, `switched_off`, `off`, or null when it has not run since).
pub fn status_in(data: &Path, dirs: &[PathBuf]) -> Value {
    let off = off_list(data);
    let report = read_json(&data.join(STATUS_FILE));
    let at = report.as_ref().and_then(|r| r["at"].as_u64());
    let seen = |name: &str| report.as_ref().and_then(|r| r["plugins"].as_array()?.iter().find(|e| e["name"].as_str() == Some(&key(name))).cloned());
    let mut names = std::collections::HashSet::new();
    let mut list = Vec::new();
    for dir in dirs {
        for f in find(dir) {
            // (the game takes the first plugin of a name, the content folder's before OMSI's)
            if !names.insert((key(&f.name), f.kind)) {
                continue;
            }
            let last = seen(&f.name);
            let schema = f.data_dir.as_ref().and_then(|d| read_json(&d.join(SCHEMA_FILE)));
            let values = f.data_dir.as_ref().and_then(|d| read_json(&d.join(SETTINGS_FILE)));
            list.push(json!({
                "name": f.name,
                "title": schema.as_ref().and_then(|s| s["title"].as_str()).unwrap_or(&f.name),
                "kind": f.kind,
                "path": f.path.display().to_string(),
                "enabled": !off.contains(&key(&f.name)),
                "state": last.as_ref().map(|e| e["state"].clone()).unwrap_or(Value::Null),
                "error": last.as_ref().and_then(|e| e["error"].as_str()).unwrap_or(""),
                "settings": schema.as_ref().map(|s| s["settings"].clone()).filter(|s| s.as_array().is_some_and(|a| !a.is_empty())).unwrap_or(Value::Null),
                "values": values.unwrap_or(Value::Null),
            }));
        }
    }
    json!({ "dirs": dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>(), "session": at, "plugins": list })
}

/// A setting's value held to what its description allows (None: not one of it).
fn fit(setting: &Value, v: &Value) -> Option<Value> {
    match setting["type"].as_str()? {
        "bool" => v.as_bool().map(Value::from),
        "number" => {
            let (min, max) = (setting["min"].as_f64().unwrap_or(f64::MIN), setting["max"].as_f64().unwrap_or(f64::MAX));
            let n = v.as_f64()?.clamp(min, max);
            let step = setting["step"].as_f64().unwrap_or(0.0);
            let n = if step > 0.0 && min.is_finite() { min + ((n - min) / step).round() * step } else { n };
            serde_json::Number::from_f64(n).map(Value::Number)
        }
        "text" => v.as_str().map(|s| Value::from(s.chars().take(256).collect::<String>())),
        "choice" => v.as_str().filter(|s| setting["choices"].as_array().is_some_and(|c| c.iter().any(|x| x.as_str() == Some(s)))).map(Value::from),
        _ => None,
    }
}

/// Save the plugin's settings the player changed (only keys its description has, each held
/// to its range); the game reads them when the plugin starts.
pub fn save_settings_in(dirs: &[PathBuf], name: &str, values: &Value) -> Result<()> {
    let f = dirs.iter().flat_map(|d| find(d)).find(|f| key(&f.name) == key(name)).ok_or_else(|| anyhow!("no plugin {name}"))?;
    let data = f.data_dir.ok_or_else(|| anyhow!("{name} has no settings"))?;
    let schema = read_json(&data.join(SCHEMA_FILE)).ok_or_else(|| anyhow!("{name} has no settings"))?;
    let mut saved: Map<String, Value> = read_json(&data.join(SETTINGS_FILE)).and_then(|v| v.as_object().cloned()).unwrap_or_default();
    for s in schema["settings"].as_array().into_iter().flatten() {
        let Some(k) = s["key"].as_str() else { continue };
        if let Some(v) = values.get(k).and_then(|v| fit(s, v)) {
            saved.insert(k.to_string(), v);
        }
    }
    std::fs::create_dir_all(&data)?;
    std::fs::write(data.join(SETTINGS_FILE), serde_json::to_string_pretty(&Value::Object(saved))?)?;
    Ok(())
}

pub fn status() -> Value {
    status_in(&crate::data_dir(), &dirs())
}

pub fn set_enabled(name: &str, on: bool) -> Result<()> {
    set_enabled_in(&crate::data_dir(), name, on)
}

pub fn save_settings(name: &str, values: &Value) -> Result<()> {
    save_settings_in(&dirs(), name, values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("launcher-plugins-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn lists_switches_and_saves_settings_as_the_game_reads_them() {
        let data = tmp("data");
        let plugins = tmp("plugins");
        std::fs::write(plugins.join("cruise.lua"), "").unwrap();
        std::fs::write(plugins.join("cruise.save.lua"), "").unwrap();
        std::fs::create_dir_all(plugins.join("Board")).unwrap();
        std::fs::write(plugins.join("Board/main.lua"), "").unwrap();
        std::fs::write(plugins.join("old.opl"), "").unwrap();
        std::fs::create_dir_all(plugins.join("cruise.data")).unwrap();
        std::fs::write(plugins.join("cruise.data/settings.schema.json"), r#"{"title":"Cruise","settings":[{"key":"step","type":"number","min":0,"max":10,"step":2,"label":"Step","default":4},{"key":"mode","type":"choice","choices":["a","b"],"default":"a"}]}"#).unwrap();
        std::fs::write(data.join(STATUS_FILE), r#"{"at":5,"plugins":[{"name":"board","path":"x","kind":"lua","state":"failed","error":"syntax error"}]}"#).unwrap();
        let dirs = vec![plugins.clone()];
        let found: Vec<_> = find(&plugins).into_iter().map(|f| (f.name, f.kind)).collect();
        assert_eq!(found, [("Board".to_string(), "lua"), ("cruise".to_string(), "lua"), ("old".to_string(), "opl")]);

        set_enabled_in(&data, "Cruise", false).unwrap();
        assert_eq!(off_list(&data), ["cruise"]);
        let s = status_in(&data, &dirs);
        let get = |n: &str| s["plugins"].as_array().unwrap().iter().find(|p| p["name"] == n).unwrap().clone();
        assert_eq!(get("cruise")["enabled"], false);
        assert_eq!(get("cruise")["title"], "Cruise");
        assert_eq!(get("Board")["state"], "failed");
        assert_eq!(get("Board")["error"], "syntax error");
        set_enabled_in(&data, "cruise", true).unwrap();
        assert!(off_list(&data).is_empty());

        save_settings_in(&dirs, "cruise", &json!({"step": 7.2, "mode": "zzz", "other": 1})).unwrap();
        let saved = read_json(&plugins.join("cruise.data/settings.json")).unwrap();
        assert_eq!(saved, json!({"step": 8.0}));
        assert!(save_settings_in(&dirs, "old", &json!({})).is_err());
        let _ = std::fs::remove_dir_all(&data);
        let _ = std::fs::remove_dir_all(&plugins);
    }
}
