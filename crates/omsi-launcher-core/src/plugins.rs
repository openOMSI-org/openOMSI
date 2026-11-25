//! The plugin list of the launcher (#3919): the Lua, compiled (`.oop`) and OMSI (`.opl`)
//! plugins of the game's `plugins` folders, each switched on or off without moving its
//! files, what became of it in the last session (loaded, a syntax error, switched off for its
//! errors), and the settings it declared with `plugin.settings`.
//!
//! The game reads the same files: `~/.openomsi/plugins-off.txt` (one path a line) before it
//! loads the plugins, and writes `~/.openomsi/plugins-status.json` after (see
//! `omsi_plugin::status`); a plugin's settings are its data folder's `settings.json`, and
//! `settings.schema.json` beside it says what they are.

use anyhow::{Result, anyhow};
use serde_json::{Map, Value, json};
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
    /// The name the game switches it by: its path under the `plugins` folder (`foo.lua`, a
    /// folder plugin's `foo/`, `foo.oop`, `Pablo/MBAS.opl`), as `omsi_plugin::status::plugin_key`
    /// and `opl_key` make it: `foo.lua` and `foo/main.lua` are two plugins.
    pub name: String,
    /// The name an older launcher wrote into the off-list for it, which the game still
    /// honours: the stem of a `.lua`, a folder (`my.plugin/` is `my`) or a `.oop`; an `.opl`'s
    /// path.
    pub stem: String,
    pub kind: &'static str,
    /// The file (or the folder of a `main.lua` plugin).
    pub path: PathBuf,
    /// Its data folder (`settings.json`), for a Lua plugin.
    pub data_dir: Option<PathBuf>,
}

fn child(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        })
}

/// `dir`'s child `name`, in any case (`Plugins`, `plugins`).
pub(crate) fn child_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    child(dir, name).filter(|d| d.is_dir())
}

fn key(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

/// An `.opl`'s name: its path under the `plugins` folder, `/` between the parts
/// (`omsi_plugin::status::opl_key`).
fn opl_name(opl: &Path, plugins: &Path) -> String {
    let rel = opl.strip_prefix(plugins).unwrap_or(opl);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Every `.opl` under `dir`, subfolders first, as `omsi_plugin::find_opls` finds them.
fn find_opls(plugins: &Path, dir: &Path, out: &mut Vec<Found>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort_by_key(|p| {
        p.file_name()
            .map(|n| n.to_string_lossy().to_uppercase())
            .unwrap_or_default()
    });
    for p in entries.iter().filter(|p| p.is_dir()) {
        find_opls(plugins, p, out);
    }
    for p in entries
        .into_iter()
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("opl")))
    {
        let name = opl_name(&p, plugins);
        out.push(Found {
            stem: name.clone(),
            name,
            kind: "opl",
            data_dir: None,
            path: p,
        });
    }
}

/// The file name of the DLL an `.opl` names (`omsi_plugin::parse_opl`), in lower case; None
/// when the file cannot be read.
fn opl_dll(opl: &Path) -> Option<String> {
    let text = String::from_utf8_lossy(&std::fs::read(opl).ok()?).into_owned();
    let mut lines = text.lines().map(str::trim);
    let mut dll = "";
    while let Some(l) = lines.next() {
        if l == "[dll]" {
            dll = lines.next().unwrap_or("");
        }
    }
    Some(
        dll.rsplit(['\\', '/'])
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase(),
    )
}

/// Every root's plugins as the game takes them: a Lua or `.oop` plugin of a path no earlier
/// root has, an `.opl` unless an earlier root has one of its file name and DLL
/// (`omsi_plugin::plugin_opls`). The game loads a later `.opl` of a path already taken when its
/// DLL differs, so that one is named `path@dll` (`omsi_plugin::status::opl_name`).
pub fn find_all(dirs: &[PathBuf]) -> Vec<Found> {
    let mut names = std::collections::HashSet::new();
    let mut opls = std::collections::HashSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        let earlier = opls.clone();
        for mut f in find(dir) {
            if f.kind == "opl" {
                let dll = opl_dll(&f.path);
                let file = f
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_ascii_lowercase());
                if earlier.contains(&(file.clone(), dll.clone())) {
                    continue;
                }
                opls.insert((file, dll.clone()));
                if names.contains(&(key(&f.name), "opl")) {
                    f.name = format!("{}@{}", f.name, dll.unwrap_or_default());
                    f.stem = f.name.clone();
                }
            }
            if names.insert((key(&f.name), f.kind)) {
                out.push(f);
            }
        }
    }
    out
}

/// The plugins of one `plugins` folder, as the game finds them: the `.opl` files of every
/// subfolder, then `omsi_plugin::lua::find_lua`'s and `oop::find_oop`'s.
pub fn find(dir: &Path) -> Vec<Found> {
    let mut out = Vec::new();
    find_opls(dir, dir, &mut out);
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort_by_key(|p| {
        p.file_name()
            .map(|n| n.to_string_lossy().to_uppercase())
            .unwrap_or_default()
    });
    let stem = |p: &Path| {
        p.file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let ext = |p: &Path, e: &str| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(e));
    for p in entries {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let file = name.to_ascii_lowercase();
        // (no entry is skipped for a leading dot: `lua::find_lua` loads those too)
        if p.is_dir() {
            if child(&p, "main.lua").is_some_and(|m| m.is_file()) {
                out.push(Found {
                    name: format!("{name}/"),
                    // (as `LuaSpec::disk` names it: `my.plugin/` is `my`)
                    stem: stem(&p),
                    kind: "lua",
                    data_dir: Some(p.join("data")),
                    path: p,
                });
            }
        } else if ext(&p, "lua") && !file.ends_with(".save.lua") {
            // (a `main.lua` loose in the folder is a folder plugin to `LuaSpec::disk`: named
            // after the folder, its data in `data/` beside it)
            let main = file == "main.lua";
            out.push(Found {
                name,
                stem: if main { stem(dir) } else { stem(&p) },
                kind: "lua",
                data_dir: Some(if main {
                    dir.join("data")
                } else {
                    p.with_extension("data")
                }),
                path: p,
            });
        } else if ext(&p, "oop") {
            out.push(Found {
                name,
                stem: stem(&p),
                kind: "oop",
                data_dir: Some(p.with_extension("data")),
                path: p,
            });
        }
    }
    out
}

/// The `plugins` folders the game reads: the content folder's, then the OMSI 2 folder's.
pub fn dirs() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for root in [crate::content_dir(), crate::root().ok()]
        .into_iter()
        .flatten()
    {
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
    std::fs::read_to_string(data.join(OFF_FILE))
        .map(|t| {
            t.lines()
                .map(key)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether the off-list `off` leaves `f` out: by its name, or by the stem an older launcher
/// wrote (`omsi_plugin::status::is_off`).
fn is_off(off: &[String], f: &Found) -> bool {
    off.contains(&key(&f.name)) || off.contains(&key(&f.stem))
}

/// Switch the plugin `name` (a [`Found::name`] of `dirs`) on or off for the next game.
pub fn set_enabled_in(data: &Path, dirs: &[PathBuf], name: &str, on: bool) -> Result<()> {
    let k = key(name);
    if k.is_empty() || k.contains(['\n', '\r']) {
        return Err(anyhow!("no plugin name"));
    }
    let mut off = off_list(data);
    let found = find_all(dirs);
    if let Some(f) = found.iter().find(|f| key(&f.name) == k) {
        let stem = key(&f.stem);
        if on && stem != k && off.contains(&stem) {
            // (an older launcher's stem left out every plugin of it: the others stay off, by
            // their own names)
            off.retain(|n| *n != stem);
            off.extend(
                found
                    .iter()
                    .filter(|o| key(&o.stem) == stem && key(&o.name) != k)
                    .map(|o| key(&o.name)),
            );
        }
    }
    off.retain(|n| *n != k);
    if !on {
        off.push(k);
    }
    off.sort();
    off.dedup();
    let text = format!(
        "# Plugins switched off in the launcher (one path under plugins a line); the game leaves them out.\n{}",
        off.iter().map(|n| format!("{n}\n")).collect::<String>()
    );
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
    let seen = |name: &str, kind: &str| {
        report.as_ref().and_then(|r| {
            r["plugins"]
                .as_array()?
                .iter()
                .find(|e| {
                    e["name"].as_str() == Some(&key(name)) && e["kind"].as_str() == Some(kind)
                })
                .cloned()
        })
    };
    let mut list = Vec::new();
    for f in find_all(dirs) {
        let last = seen(&f.name, f.kind);
        let schema = f
            .data_dir
            .as_ref()
            .and_then(|d| read_json(&d.join(SCHEMA_FILE)));
        let values = f
            .data_dir
            .as_ref()
            .and_then(|d| read_json(&d.join(SETTINGS_FILE)));
        list.push(json!({
            "name": f.name,
            "title": schema.as_ref().and_then(|s| s["title"].as_str()).map(str::to_string).unwrap_or_else(|| if f.kind == "opl" { f.path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() } else { f.stem.clone() }),
            "kind": f.kind,
            "path": f.path.display().to_string(),
            "enabled": !is_off(&off, &f),
            "state": last.as_ref().map(|e| e["state"].clone()).unwrap_or(Value::Null),
            "error": last.as_ref().and_then(|e| e["error"].as_str()).unwrap_or(""),
            "settings": schema.as_ref().map(|s| s["settings"].clone()).filter(|s| s.as_array().is_some_and(|a| !a.is_empty())).unwrap_or(Value::Null),
            "values": values.unwrap_or(Value::Null),
            // (a folder plugin the catalog installed: its id and version there)
            "catalog": (f.kind == "lua" && f.path.is_dir()).then(|| f.path.file_name().and_then(|n| f.path.parent().and_then(|dir| crate::plugin_catalog::installed(dir, &n.to_string_lossy())))).flatten(),
        }));
    }
    json!({ "dirs": dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>(), "session": at, "plugins": list })
}

/// A setting's value held to what its description allows (None: not one of it).
fn fit(setting: &Value, v: &Value) -> Option<Value> {
    match setting["type"].as_str()? {
        "bool" => v.as_bool().map(Value::from),
        "number" => {
            let (min, max) = (
                setting["min"].as_f64().unwrap_or(f64::MIN),
                setting["max"].as_f64().unwrap_or(f64::MAX),
            );
            let n = v.as_f64()?.clamp(min, max);
            let step = setting["step"].as_f64().unwrap_or(0.0);
            let n = if step > 0.0 && min.is_finite() {
                // (again after the step: a range not a multiple of it went past max)
                (min + ((n - min) / step).round() * step).clamp(min, max)
            } else {
                n
            };
            serde_json::Number::from_f64(n).map(Value::Number)
        }
        "text" => v
            .as_str()
            .map(|s| Value::from(s.chars().take(256).collect::<String>())),
        "choice" => v
            .as_str()
            .filter(|s| {
                setting["choices"]
                    .as_array()
                    .is_some_and(|c| c.iter().any(|x| x.as_str() == Some(s)))
            })
            .map(Value::from),
        _ => None,
    }
}

/// Save the plugin's settings the player changed (only keys its description has, each held
/// to its range); the game reads them when the plugin starts.
pub fn save_settings_in(dirs: &[PathBuf], name: &str, values: &Value) -> Result<()> {
    let f = find_all(dirs)
        .into_iter()
        .find(|f| key(&f.name) == key(name))
        .ok_or_else(|| anyhow!("no plugin {name}"))?;
    let data = f
        .data_dir
        .ok_or_else(|| anyhow!("{name} has no settings"))?;
    let schema =
        read_json(&data.join(SCHEMA_FILE)).ok_or_else(|| anyhow!("{name} has no settings"))?;
    let mut saved: Map<String, Value> = read_json(&data.join(SETTINGS_FILE))
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    for s in schema["settings"].as_array().into_iter().flatten() {
        let Some(k) = s["key"].as_str() else { continue };
        if let Some(v) = values.get(k).and_then(|v| fit(s, v)) {
            saved.insert(k.to_string(), v);
        }
    }
    std::fs::create_dir_all(&data)?;
    std::fs::write(
        data.join(SETTINGS_FILE),
        serde_json::to_string_pretty(&Value::Object(saved))?,
    )?;
    Ok(())
}

pub fn status() -> Value {
    status_in(&crate::data_dir(), &dirs())
}

pub fn set_enabled(name: &str, on: bool) -> Result<()> {
    set_enabled_in(&crate::data_dir(), &dirs(), name, on)
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

    /// A game with plugins disabled writes an empty report: the list then shows no plugin as
    /// run in the last game.
    #[test]
    fn after_a_game_without_plugins_none_shows_as_run() {
        let data = tmp("data-none");
        let plugins = tmp("plugins-none");
        std::fs::write(plugins.join("cruise.lua"), "").unwrap();
        let dirs = vec![plugins.clone()];
        std::fs::write(data.join(STATUS_FILE), r#"{"at":5,"plugins":[{"name":"cruise.lua","path":"x","kind":"lua","state":"loaded"}]}"#).unwrap();
        assert_eq!(status_in(&data, &dirs)["plugins"][0]["state"], "loaded");
        std::fs::write(data.join(STATUS_FILE), r#"{"at":9,"plugins":[]}"#).unwrap();
        let after = status_in(&data, &dirs);
        assert_eq!(after["session"], 9);
        assert_eq!(after["plugins"][0]["state"], Value::Null);
    }

    #[test]
    fn an_opl_of_one_path_in_two_roots_with_two_dlls_is_two_plugins() {
        let data = tmp("data-opl2");
        let content = tmp("content-opl2");
        let omsi = tmp("omsi-opl2");
        for (root, dll) in [(&content, "MBAS.dll"), (&omsi, "Plugins\\MBAS2.dll")] {
            std::fs::create_dir_all(root.join("Pablo")).unwrap();
            std::fs::write(root.join("Pablo/MBAS.opl"), format!("[dll]\n{dll}\n")).unwrap();
        }
        let dirs = vec![content.clone(), omsi.clone()];
        let names: Vec<String> = find_all(&dirs).into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["Pablo/MBAS.opl", "Pablo/MBAS.opl@mbas2.dll"]);
        set_enabled_in(&data, &dirs, "Pablo/MBAS.opl@mbas2.dll", false).unwrap();
        let list = status_in(&data, &dirs);
        let enabled: Vec<bool> = list["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["enabled"].as_bool().unwrap())
            .collect();
        assert_eq!(enabled, [true, false]);
        std::fs::write(omsi.join("Pablo/MBAS.opl"), "[dll]\nmbas.dll\n").unwrap();
        assert_eq!(find_all(&dirs).len(), 1);
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
        std::fs::write(data.join(STATUS_FILE), r#"{"at":5,"plugins":[{"name":"board/","path":"x","kind":"lua","state":"failed","error":"syntax error"},{"name":"old.opl","path":"y","kind":"lua","state":"loaded"}]}"#).unwrap();
        let dirs = vec![plugins.clone()];
        let found: Vec<_> = find(&plugins)
            .into_iter()
            .map(|f| (f.name, f.kind))
            .collect();
        assert_eq!(
            found,
            [
                ("old.opl".to_string(), "opl"),
                ("Board/".to_string(), "lua"),
                ("cruise.lua".to_string(), "lua")
            ]
        );

        set_enabled_in(&data, &dirs, "Cruise.lua", false).unwrap();
        assert_eq!(off_list(&data), ["cruise.lua"]);
        let s = status_in(&data, &dirs);
        let get = |n: &str| {
            s["plugins"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == n)
                .unwrap()
                .clone()
        };
        assert_eq!(get("cruise.lua")["enabled"], false);
        assert_eq!(get("cruise.lua")["title"], "Cruise");
        assert_eq!(get("Board/")["title"], "Board");
        assert_eq!(get("Board/")["state"], "failed");
        assert_eq!(get("Board/")["error"], "syntax error");
        assert_eq!(get("old.opl")["title"], "old");
        assert_eq!(
            get("old.opl")["state"],
            Value::Null,
            "a Lua plugin's line is not an OMSI plugin's"
        );
        set_enabled_in(&data, &dirs, "cruise.lua", true).unwrap();
        assert!(off_list(&data).is_empty());

        save_settings_in(
            &dirs,
            "cruise.lua",
            &json!({"step": 7.2, "mode": "zzz", "other": 1}),
        )
        .unwrap();
        let saved = read_json(&plugins.join("cruise.data/settings.json")).unwrap();
        assert_eq!(saved, json!({"step": 8.0}));
        assert!(save_settings_in(&dirs, "old", &json!({})).is_err());
        let _ = std::fs::remove_dir_all(&data);
        let _ = std::fs::remove_dir_all(&plugins);
    }

    #[test]
    fn a_number_rounded_to_its_step_stays_in_range() {
        let setting = json!({"type": "number", "min": 0, "max": 10, "step": 4});
        assert_eq!(fit(&setting, &json!(10)), Some(json!(10.0)));
        assert_eq!(fit(&setting, &json!(5)), Some(json!(4.0)));
        assert_eq!(fit(&setting, &json!(-3)), Some(json!(0.0)));
    }

    #[test]
    fn opls_in_subfolders_are_listed_by_their_path_and_folders_by_their_stem() {
        let plugins = tmp("subfolders");
        for f in [
            "Pablo/MBAS.opl",
            "Pablo/Config.opl",
            "Config.opl",
            "my.plugin/main.lua",
        ] {
            std::fs::create_dir_all(plugins.join(f).parent().unwrap()).unwrap();
            std::fs::write(plugins.join(f), "").unwrap();
        }
        let found: Vec<_> = find(&plugins)
            .into_iter()
            .map(|f| (f.name, f.kind))
            .collect();
        assert_eq!(
            found,
            [
                ("Pablo/Config.opl", "opl"),
                ("Pablo/MBAS.opl", "opl"),
                ("Config.opl", "opl"),
                ("my.plugin/", "lua")
            ]
            .map(|(n, k)| (n.to_string(), k))
        );
        let _ = std::fs::remove_dir_all(&plugins);
    }

    #[test]
    fn a_lua_file_and_a_folder_of_one_name_are_listed_switched_and_set_apart() {
        let data = tmp("same-name-data");
        let plugins = tmp("same-name");
        std::fs::write(plugins.join("foo.lua"), "").unwrap();
        std::fs::create_dir_all(plugins.join("foo/data")).unwrap();
        std::fs::write(plugins.join("foo/main.lua"), "").unwrap();
        std::fs::create_dir_all(plugins.join("foo.data")).unwrap();
        let schema = |title: &str| {
            format!(
                r#"{{"title":"{title}","settings":[{{"key":"n","type":"number","min":0,"max":10}}]}}"#
            )
        };
        std::fs::write(
            plugins.join("foo.data/settings.schema.json"),
            schema("File"),
        )
        .unwrap();
        std::fs::write(
            plugins.join("foo/data/settings.schema.json"),
            schema("Folder"),
        )
        .unwrap();
        std::fs::write(data.join(STATUS_FILE), r#"{"at":5,"plugins":[{"name":"foo/","path":"x","kind":"lua","state":"failed","error":"syntax error"},{"name":"foo.lua","path":"y","kind":"lua","state":"loaded"}]}"#).unwrap();
        let dirs = vec![plugins.clone()];
        let list = || {
            status_in(&data, &dirs)["plugins"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    (
                        p["name"].as_str().unwrap().to_string(),
                        p["title"].as_str().unwrap().to_string(),
                        p["enabled"].as_bool().unwrap(),
                        p["state"].as_str().unwrap().to_string(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let row = |n: &str, t: &str, on: bool, st: &str| {
            (n.to_string(), t.to_string(), on, st.to_string())
        };
        assert_eq!(
            list(),
            [
                row("foo/", "Folder", true, "failed"),
                row("foo.lua", "File", true, "loaded")
            ]
        );

        set_enabled_in(&data, &dirs, "foo/", false).unwrap();
        assert_eq!(off_list(&data), ["foo/"]);
        assert_eq!(
            list(),
            [
                row("foo/", "Folder", false, "failed"),
                row("foo.lua", "File", true, "loaded")
            ]
        );
        set_enabled_in(&data, &dirs, "foo/", true).unwrap();
        set_enabled_in(&data, &dirs, "foo.lua", false).unwrap();
        assert_eq!(off_list(&data), ["foo.lua"]);

        // an older launcher's "foo" leaves out both; switching one on keeps the other off
        std::fs::write(data.join(OFF_FILE), "foo\n").unwrap();
        assert!(list().iter().all(|r| !r.2));
        set_enabled_in(&data, &dirs, "foo.lua", true).unwrap();
        assert_eq!(off_list(&data), ["foo/"]);

        save_settings_in(&dirs, "foo.lua", &json!({"n": 3})).unwrap();
        save_settings_in(&dirs, "foo/", &json!({"n": 7})).unwrap();
        assert_eq!(
            read_json(&plugins.join("foo.data/settings.json")).unwrap(),
            json!({"n": 3.0})
        );
        assert_eq!(
            read_json(&plugins.join("foo/data/settings.json")).unwrap(),
            json!({"n": 7.0})
        );
        let _ = std::fs::remove_dir_all(&data);
        let _ = std::fs::remove_dir_all(&plugins);
    }

    #[test]
    fn hidden_plugins_and_a_loose_main_lua_are_found_as_the_game_loads_them() {
        let data = tmp("hidden-data");
        let plugins = tmp("hidden");
        std::fs::write(plugins.join(".quiet.lua"), "").unwrap();
        std::fs::create_dir_all(plugins.join(".tools")).unwrap();
        std::fs::write(plugins.join(".tools/main.lua"), "").unwrap();
        std::fs::write(plugins.join("main.lua"), "").unwrap();
        std::fs::create_dir_all(plugins.join("data")).unwrap();
        std::fs::write(
            plugins.join("data/settings.schema.json"),
            r#"{"title":"Loose","settings":[{"key":"n","type":"number","min":0,"max":10}]}"#,
        )
        .unwrap();
        std::fs::write(plugins.join("data/settings.json"), r#"{"n":3}"#).unwrap();

        let found = find(&plugins);
        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, [".quiet.lua", ".tools/", "main.lua"]);
        let main = found.iter().find(|f| f.name == "main.lua").unwrap();
        assert_eq!(
            main.data_dir.as_deref(),
            Some(plugins.join("data").as_path())
        );

        let dirs = vec![plugins.clone()];
        let list = status_in(&data, &dirs);
        let row = list["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "main.lua")
            .unwrap()
            .clone();
        assert_eq!(row["title"], "Loose");
        assert_eq!(row["values"]["n"], 3);
        save_settings_in(&dirs, "main.lua", &json!({"n": 4})).unwrap();
        assert!(
            std::fs::read_to_string(plugins.join("data/settings.json"))
                .unwrap()
                .contains('4')
        );
        assert!(!plugins.join("main.data").exists());
    }
}
