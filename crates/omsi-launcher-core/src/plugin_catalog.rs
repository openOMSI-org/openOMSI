//! The plugin catalog (#3920). Only files whose size and SHA-256 match `catalog.json` are
//! installed, and only folders the catalog put there (`.catalog.json`) are updated or removed.

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::path::{Path, PathBuf};

pub const CATALOG_URL: &str =
    "https://raw.githubusercontent.com/openOMSI-org/openOMSI-Plugins/main/catalog.json";
pub const FILES_URL: &str =
    "https://raw.githubusercontent.com/openOMSI-org/openOMSI-Plugins/main/plugins/";
pub const SOURCE_URL: &str = "https://github.com/openOMSI-org/openOMSI-Plugins/tree/main/plugins/";
/// A newer catalog format is not read: the page says to update the launcher.
const VERSION: u32 = 1;
/// `{id, version}`, in the folder of every plugin the catalog installed.
const MARK: &str = ".catalog.json";
const MAX_FILE: u64 = 2 * 1024 * 1024;
const MAX_PLUGIN: u64 = 10 * 1024 * 1024;
const EXTENSIONS: &[&str] = &[
    "lua", "json", "txt", "md", "csv", "png", "jpg", "ogg", "wav", "ttf", "otf",
];

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FileRef {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub license: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_game: Option<String>,
    #[serde(default)]
    pub bytes: u64,
    pub files: Vec<FileRef>,
}

#[derive(Debug, Deserialize)]
struct Catalog {
    version: u32,
    plugins: Vec<serde_json::Value>,
}

fn id_ok(id: &str) -> bool {
    (2..=40).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
}

/// A file's path inside the plugin's folder: plain names joined by `/`, no `..`, a taken
/// extension.
fn path_ok(p: &str) -> bool {
    let plain = |part: &str| {
        !part.is_empty()
            && !part.starts_with('.')
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    };
    p.len() <= 120
        && p.split('/').all(plain)
        && !p.split('/').any(|x| x == "..")
        && p.rsplit_once('.')
            .is_some_and(|(_, e)| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        && !p.eq_ignore_ascii_case(MARK)
        && !(!p.contains('/') && is_saved(&p.to_ascii_lowercase(), false))
        && !p
            .split('/')
            .next()
            .is_some_and(|first| first.eq_ignore_ascii_case("data") && p.contains('/'))
}

fn hex_ok(h: &str) -> bool {
    h.len() == 64
        && h.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// What is wrong with an entry (it is then left out of the list).
fn problem(e: &Entry) -> Option<&'static str> {
    let line = |t: &str, n: usize| {
        !t.trim().is_empty() && t.chars().count() <= n && !t.contains(['\n', '\r'])
    };
    if !id_ok(&e.id) {
        return Some("id");
    }
    if !line(&e.name, 60)
        || !line(&e.author, 60)
        || !line(&e.description, 300)
        || !line(&e.license, 40)
        || version(&e.version).is_none()
    {
        return Some("text");
    }
    if e.files.is_empty() || e.files.len() > 200 || !e.files.iter().any(|f| f.path == "main.lua") {
        return Some("files");
    }
    if e.files
        .iter()
        .any(|f| !path_ok(&f.path) || !hex_ok(&f.sha256) || f.size > MAX_FILE)
    {
        return Some("file");
    }
    if e.files.iter().map(|f| f.size).sum::<u64>() > MAX_PLUGIN {
        return Some("size");
    }
    None
}

/// `MAJOR.MINOR.PATCH` as numbers.
pub fn version(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.trim().split('.').map(|x| x.parse::<u64>().ok());
    let out = (it.next()??, it.next()??, it.next()??);
    it.next().is_none().then_some(out)
}

/// The catalog's plugins, each checked; the ones that break a rule are left out.
pub fn parse(text: &str) -> Result<Vec<Entry>> {
    let c: Catalog = serde_json::from_str(text).context("the plugin catalog cannot be read")?;
    if c.version > VERSION {
        bail!("the plugin catalog is newer than this launcher: update openOMSI to see it");
    }
    let mut out: Vec<Entry> = Vec::new();
    for v in c.plugins {
        // (an entry that breaks a rule, or comes twice, is left out: the rest still shows)
        if let Ok(e) = serde_json::from_value::<Entry>(v) {
            if problem(&e).is_none() && !out.iter().any(|o| o.id == e.id) {
                out.push(e);
            }
        }
    }
    Ok(out)
}

/// The version the catalog installed in `plugins/<id>/` (None: not from the catalog).
pub fn installed(plugins: &Path, id: &str) -> Option<String> {
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(plugins.join(id).join(MARK)).ok()?).ok()?;
    (v["id"].as_str() == Some(id))
        .then(|| v["version"].as_str().map(str::to_string))
        .flatten()
}

/// Nothing changes when a file does not match; an update keeps what the plugin saved.
pub fn install(plugins: &Path, e: &Entry, fetch: &dyn Fn(&str) -> Result<Vec<u8>>) -> Result<()> {
    if let Some(why) = problem(e) {
        bail!("{} is not a plugin of the catalog ({why})", e.id);
    }
    let target = plugins.join(&e.id);
    std::fs::create_dir_all(plugins)?;
    // (beside the plugins folder, on the same disk, where the game finds no plugin)
    let parent = plugins.parent().unwrap_or(plugins);
    let tmp = parent.join(format!(".plugin-install-{}", e.id));
    let old = parent.join(format!(".plugin-old-{}", e.id));
    recover(&target, &old)?;
    if target.exists() && installed(plugins, &e.id).is_none() {
        bail!(
            "plugins/{} is there already and did not come from the catalog: rename or remove it first",
            e.id
        );
    }
    let _ = std::fs::remove_dir_all(&tmp);
    let result = (|| -> Result<()> {
        for f in &e.files {
            let bytes = fetch(&format!("{FILES_URL}{}/{}", e.id, f.path))
                .with_context(|| format!("{}: {}", e.id, f.path))?;
            let got: String = sha2::Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            if bytes.len() as u64 != f.size || got != f.sha256 {
                bail!(
                    "{}: {} is not the file the catalog names (changed meanwhile? try again)",
                    e.id,
                    f.path
                );
            }
            let to = tmp.join(&f.path);
            std::fs::create_dir_all(to.parent().unwrap_or(&tmp))?;
            std::fs::write(&to, bytes)?;
        }
        std::fs::write(
            tmp.join(MARK),
            serde_json::to_string_pretty(&serde_json::json!({ "id": e.id, "version": e.version }))?,
        )?;
        replace(&tmp, &target, &old, &|a, b| std::fs::rename(a, b))
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    result
}

/// Put `new` in place of `target` and move what the plugin saved over (`saved`). The old
/// folder waits at `old` until then, so on an error the plugin and its data are put back.
fn replace(
    new: &Path,
    target: &Path,
    old: &Path,
    rename: &dyn Fn(&Path, &Path) -> std::io::Result<()>,
) -> Result<()> {
    if !target.exists() {
        rename(new, target)?;
        return Ok(());
    }
    rename(target, old)?;
    if let Err(e) = rename(new, target) {
        let _ = rename(old, target);
        return Err(e.into());
    }
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for name in saved(old) {
        let (from, to) = (old.join(&name), free_name(target, &name));
        if let Err(e) = rename(from.as_path(), to.as_path()) {
            for (from, to) in moved.iter().rev() {
                let _ = rename(to.as_path(), from.as_path());
            }
            let _ = rename(target, new).and_then(|_| rename(old, target));
            return Err(e.into());
        }
        moved.push((from, to));
    }
    // (what is left there is the old version's code: a leftover is cleared by `recover`)
    let _ = std::fs::remove_dir_all(old);
    Ok(())
}

/// An update cut short (the launcher closed, a failed rollback) left the old folder at `old`:
/// put it back, or move what the plugin saved into the new one.
fn recover(target: &Path, old: &Path) -> Result<()> {
    if !old.exists() {
        return Ok(());
    }
    if !target.exists() {
        std::fs::rename(old, target)?;
        return Ok(());
    }
    // (a game started in between may have saved anew: the old copy is kept beside it)
    for name in saved(old) {
        std::fs::rename(old.join(&name), free_name(target, &name))?;
    }
    std::fs::remove_dir_all(old).with_context(|| {
        format!(
            "{} is left from an earlier update and cannot be removed: close what holds it open, or delete it",
            old.display()
        )
    })
}

/// How the file a Lua plugin keeps `omsi.data` in ends (`LuaSpec::disk` in omsi-plugin).
const SAVE: &str = ".save.lua";
const KEPT: &str = "data.before-update";

/// Whether `name` is what a plugin saved (`data/`, `data.save.lua`) or a copy of it an update
/// kept (`data.before-update…/`, `data.before-update….save.lua`).
fn is_saved(name: &str, dir: bool) -> bool {
    match name.strip_suffix(SAVE) {
        Some(stem) => !dir && (stem == "data" || stem.starts_with(KEPT)),
        None => dir && (name == "data" || name.starts_with(KEPT)),
    }
}

/// What the plugin saved in its folder and the copies kept beside it, `data` and
/// `data.save.lua` first.
fn saved(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    is_saved(&n, e.path().is_dir()).then_some(n)
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|n| (!matches!(n.as_str(), "data" | "data.save.lua"), n.clone()));
    v
}

/// `name` in `dir`, or the first free `data.before-update`, `data.before-update-2`… (with
/// `.save.lua` for the saved file) when taken.
fn free_name(dir: &Path, name: &str) -> PathBuf {
    let suffix = if name.ends_with(SAVE) { SAVE } else { "" };
    std::iter::once(dir.join(name))
        .chain((1..).map(|n| match n {
            1 => dir.join(format!("{KEPT}{suffix}")),
            n => dir.join(format!("{KEPT}-{n}{suffix}")),
        }))
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(name))
}

/// Remove a plugin the catalog installed, its settings and data with it.
pub fn remove(plugins: &Path, id: &str) -> Result<()> {
    if !id_ok(id) || installed(plugins, id).is_none() {
        bail!("plugins/{id} did not come from the catalog");
    }
    std::fs::remove_dir_all(plugins.join(id)).map_err(|e| anyhow!("plugins/{id}: {e}"))
}

/// The content folder's `plugins` (made when missing): where the catalog installs.
pub fn plugins_dir() -> Result<PathBuf> {
    let content = crate::content_dir().ok_or_else(|| {
        anyhow!(crate::i18n::tr(
            "no game binary configured, so no content folder"
        ))
    })?;
    Ok(crate::plugins::child_dir(&content, "plugins").unwrap_or_else(|| content.join("plugins")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(b: &[u8]) -> String {
        sha2::Sha256::digest(b)
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect()
    }

    fn entry(version: &str, main: &[u8]) -> Entry {
        Entry {
            id: "speed-warning".into(),
            name: "Speed warning".into(),
            author: "openOMSI".into(),
            version: version.into(),
            description: "Warns".into(),
            license: "MIT".into(),
            tags: vec![],
            homepage: None,
            min_game: None,
            bytes: main.len() as u64,
            files: vec![FileRef {
                path: "main.lua".into(),
                size: main.len() as u64,
                sha256: sha(main),
            }],
        }
    }

    #[test]
    fn bad_entries_are_left_out_and_a_newer_format_is_refused() {
        let good = serde_json::to_value(entry("1.0.0", b"x")).unwrap();
        let mut escape = good.clone();
        escape["files"][0]["path"] = "../evil.lua".into();
        let mut exe = good.clone();
        exe["id"] = "other".into();
        exe["files"] = serde_json::json!([{ "path": "main.lua", "size": 1, "sha256": sha(b"x") }, { "path": "run.exe", "size": 1, "sha256": sha(b"x") }]);
        let text = serde_json::json!({ "version": 1, "plugins": [good, escape, exe, { "id": 5 }] })
            .to_string();
        let list = parse(&text).unwrap();
        assert_eq!(
            list.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["speed-warning"]
        );
        assert!(parse(r#"{"version": 2, "plugins": []}"#).is_err());
        assert!(!path_ok("data/settings.json") && path_ok("data.lua") && !path_ok(".catalog.json"));
        assert!(!path_ok("data.save.lua") && path_ok("lib/data.save.lua"));
        assert!(version("1.2.3") > version("1.2.0") && version("1.2").is_none());
    }

    #[test]
    fn installs_checks_updates_keeps_data_and_removes() {
        let root = std::env::temp_dir().join(format!("plugin-catalog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let plugins = root.join("plugins");
        let v1 = entry("1.0.0", b"-- one");
        install(&plugins, &v1, &|_| Ok(b"-- one".to_vec())).unwrap();
        assert_eq!(
            installed(&plugins, "speed-warning").as_deref(),
            Some("1.0.0")
        );
        std::fs::create_dir_all(plugins.join("speed-warning/data")).unwrap();
        std::fs::write(plugins.join("speed-warning/data/settings.json"), "{}").unwrap();

        // a file that is not the catalog's: nothing changes
        let v2 = entry("1.1.0", b"-- two");
        assert!(install(&plugins, &v2, &|_| Ok(b"-- evil".to_vec())).is_err());
        assert_eq!(
            std::fs::read(plugins.join("speed-warning/main.lua")).unwrap(),
            b"-- one"
        );
        assert!(!root.join(".plugin-install-speed-warning").exists());

        install(&plugins, &v2, &|url| {
            assert!(url.ends_with("/plugins/speed-warning/main.lua"));
            Ok(b"-- two".to_vec())
        })
        .unwrap();
        assert_eq!(
            std::fs::read(plugins.join("speed-warning/main.lua")).unwrap(),
            b"-- two"
        );
        assert!(plugins.join("speed-warning/data/settings.json").is_file());
        assert_eq!(
            installed(&plugins, "speed-warning").as_deref(),
            Some("1.1.0")
        );

        // the player's own folder of that name is never replaced or removed
        std::fs::create_dir_all(plugins.join("mine")).unwrap();
        let mut mine = entry("1.0.0", b"x");
        mine.id = "mine".into();
        assert!(install(&plugins, &mine, &|_| Ok(b"x".to_vec())).is_err());
        assert!(remove(&plugins, "mine").is_err());

        remove(&plugins, "speed-warning").unwrap();
        assert!(!plugins.join("speed-warning").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_update_cut_short_keeps_the_plugin_and_its_data() {
        fn read(p: &Path) -> String {
            std::fs::read_to_string(p).unwrap()
        }
        let root = std::env::temp_dir().join(format!("plugin-catalog-swap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (new, target, old) = (root.join("new"), root.join("plugins/p"), root.join("old"));
        std::fs::create_dir_all(target.join("data")).unwrap();
        std::fs::write(target.join("main.lua"), "-- one").unwrap();
        std::fs::write(target.join("data/settings.json"), "{}").unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("main.lua"), "-- two").unwrap();

        // the new folder cannot be moved in, then the data cannot be moved over: each time
        // the plugin and its data are back as they were
        for fails in [new.clone(), old.join("data")] {
            let r = replace(&new, &target, &old, &|a, b| {
                if a == fails.as_path() {
                    Err(std::io::Error::other("held open"))
                } else {
                    std::fs::rename(a, b)
                }
            });
            assert!(r.is_err());
            assert_eq!(read(&target.join("main.lua")), "-- one");
            assert!(target.join("data/settings.json").is_file());
            assert_eq!(read(&new.join("main.lua")), "-- two");
            assert!(!old.exists());
        }

        replace(&new, &target, &old, &|a, b| std::fs::rename(a, b)).unwrap();
        assert_eq!(read(&target.join("main.lua")), "-- two");
        assert!(target.join("data/settings.json").is_file());
        assert!(!old.exists() && !new.exists());

        // stopped with the old folder aside: the next install puts it back
        std::fs::rename(&target, &old).unwrap();
        recover(&target, &old).unwrap();
        assert!(target.join("data/settings.json").is_file() && !old.exists());

        // stopped after the new folder went in, and a game run since made a new `data/`:
        // both copies stay
        std::fs::create_dir_all(old.join("data")).unwrap();
        std::fs::write(old.join("data/settings.json"), "{\"old\":1}").unwrap();
        std::fs::write(target.join("data/settings.json"), "{\"new\":1}").unwrap();
        recover(&target, &old).unwrap();
        assert_eq!(read(&target.join("data/settings.json")), "{\"new\":1}");
        assert_eq!(
            read(&target.join("data.before-update/settings.json")),
            "{\"old\":1}"
        );
        assert!(!old.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn data_kept_by_a_recovery_survives_the_install_that_follows() {
        let root =
            std::env::temp_dir().join(format!("plugin-catalog-recover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let plugins = root.join("plugins");
        let (target, old) = (
            plugins.join("speed-warning"),
            root.join(".plugin-old-speed-warning"),
        );
        install(&plugins, &entry("1.0.0", b"-- one"), &|_| {
            Ok(b"-- one".to_vec())
        })
        .unwrap();
        std::fs::create_dir_all(target.join("data")).unwrap();
        std::fs::write(target.join("data/settings.json"), "old").unwrap();

        // an update to 1.1.0 cut short after the new version went in (the old folder still
        // aside with its data), and a game run since then made a new `data/`
        std::fs::rename(&target, &old).unwrap();
        std::fs::create_dir_all(target.join("data")).unwrap();
        std::fs::write(target.join("main.lua"), "-- two").unwrap();
        std::fs::write(
            target.join(MARK),
            r#"{"id": "speed-warning", "version": "1.1.0"}"#,
        )
        .unwrap();
        std::fs::write(target.join("data/settings.json"), "new").unwrap();

        install(&plugins, &entry("1.2.0", b"-- three"), &|_| {
            Ok(b"-- three".to_vec())
        })
        .unwrap();
        let read = |p: &str| std::fs::read_to_string(target.join(p)).unwrap();
        assert_eq!(read("main.lua"), "-- three");
        assert_eq!(read("data/settings.json"), "new");
        assert_eq!(read("data.before-update/settings.json"), "old");
        assert!(!old.exists());

        // and the update after that keeps both again
        install(&plugins, &entry("1.3.0", b"-- four"), &|_| {
            Ok(b"-- four".to_vec())
        })
        .unwrap();
        assert_eq!(read("data/settings.json"), "new");
        assert_eq!(read("data.before-update/settings.json"), "old");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn omsi_data_survives_updates_and_recovery() {
        let root = std::env::temp_dir().join(format!("plugin-catalog-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let plugins = root.join("plugins");
        let (target, old) = (
            plugins.join("speed-warning"),
            root.join(".plugin-old-speed-warning"),
        );
        let read = |p: &str| std::fs::read_to_string(target.join(p)).unwrap();
        install(&plugins, &entry("1.0.0", b"-- one"), &|_| {
            Ok(b"-- one".to_vec())
        })
        .unwrap();
        // what the game writes for `omsi.data` and `storage.*`
        std::fs::write(target.join("data.save.lua"), "return { trips = 3 }").unwrap();
        std::fs::create_dir_all(target.join("data")).unwrap();
        std::fs::write(target.join("data/storage.json"), "{\"km\":12}").unwrap();

        install(&plugins, &entry("1.1.0", b"-- two"), &|_| {
            Ok(b"-- two".to_vec())
        })
        .unwrap();
        assert_eq!(read("main.lua"), "-- two");
        assert_eq!(read("data.save.lua"), "return { trips = 3 }");
        assert_eq!(read("data/storage.json"), "{\"km\":12}");

        // the saved file cannot be moved over: the old version and its state are back
        let new = root.join("new");
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("main.lua"), "-- three").unwrap();
        let fails = old.join("data.save.lua");
        let r = replace(&new, &target, &old, &|a, b| {
            if a == fails.as_path() {
                Err(std::io::Error::other("held open"))
            } else {
                std::fs::rename(a, b)
            }
        });
        assert!(r.is_err() && !old.exists());
        assert_eq!(read("main.lua"), "-- two");
        assert_eq!(read("data.save.lua"), "return { trips = 3 }");

        // an update cut short after the new version went in, and a game run since saved anew:
        // the next install keeps both copies, and the one after that too
        std::fs::rename(&target, &old).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("main.lua"), "-- three").unwrap();
        std::fs::write(
            target.join(MARK),
            r#"{"id": "speed-warning", "version": "1.2.0"}"#,
        )
        .unwrap();
        std::fs::write(target.join("data.save.lua"), "return { trips = 1 }").unwrap();
        for v in ["1.3.0", "1.4.0"] {
            install(
                &plugins,
                &entry(v, b"-- four"),
                &|_| Ok(b"-- four".to_vec()),
            )
            .unwrap();
            assert_eq!(read("main.lua"), "-- four");
            assert_eq!(read("data.save.lua"), "return { trips = 1 }");
            assert_eq!(read("data.before-update.save.lua"), "return { trips = 3 }");
            assert_eq!(read("data/storage.json"), "{\"km\":12}");
            assert!(!old.exists());
        }

        // stopped with the old folder aside and nothing in its place: it is put back whole
        std::fs::rename(&target, &old).unwrap();
        recover(&target, &old).unwrap();
        assert_eq!(read("data.save.lua"), "return { trips = 1 }");
        assert!(!old.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
