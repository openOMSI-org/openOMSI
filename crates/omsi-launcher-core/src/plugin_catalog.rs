//! The plugin catalog (#3920): the reviewed, open source Lua plugins of the repository
//! openOMSI-org/openOMSI-Plugins, listed in the launcher's Plugins page and installed from it.
//!
//! The catalog (`catalog.json`) names each plugin's files with their size and SHA-256; a file
//! is downloaded from the same repository and installed only when both match, into
//! `plugins/<id>/` of the content folder. An update replaces the files and keeps
//! `plugins/<id>/data/` (the plugin's settings and saved data). Only plugins the catalog put
//! there (`.catalog.json` beside them) are updated or removed: a folder of that name the
//! player made is never touched.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::path::{Path, PathBuf};

/// The repository's catalog, and where its plugins' files are.
pub const CATALOG_URL: &str = "https://raw.githubusercontent.com/openOMSI-org/openOMSI-Plugins/main/catalog.json";
pub const FILES_URL: &str = "https://raw.githubusercontent.com/openOMSI-org/openOMSI-Plugins/main/plugins/";
/// The plugin's page in the repository (its source, for the player to read).
pub const SOURCE_URL: &str = "https://github.com/openOMSI-org/openOMSI-Plugins/tree/main/plugins/";
/// The catalog's format; a newer one is not read (the page says to update the launcher).
const VERSION: u32 = 1;
/// What the catalog put into a plugin's folder: `{id, version}`.
const MARK: &str = ".catalog.json";
const MAX_FILE: u64 = 2 * 1024 * 1024;
const MAX_PLUGIN: u64 = 10 * 1024 * 1024;
const EXTENSIONS: &[&str] = &["lua", "json", "txt", "md", "csv", "png", "jpg", "ogg", "wav", "ttf", "otf"];

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
    (2..=40).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') && !id.starts_with('-')
}

/// A file's path inside the plugin's folder: plain names joined by `/`, no `..`, a taken
/// extension.
fn path_ok(p: &str) -> bool {
    let plain = |part: &str| !part.is_empty() && !part.starts_with('.') && part.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'));
    p.len() <= 120
        && p.split('/').all(plain)
        && !p.split('/').any(|x| x == "..")
        && p.rsplit_once('.').is_some_and(|(_, e)| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        && !p.eq_ignore_ascii_case(MARK)
        && !p.split('/').next().is_some_and(|first| first.eq_ignore_ascii_case("data") && p.contains('/'))
}

fn hex_ok(h: &str) -> bool {
    h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// What is wrong with an entry (it is then left out of the list).
fn problem(e: &Entry) -> Option<&'static str> {
    let line = |t: &str, n: usize| !t.trim().is_empty() && t.chars().count() <= n && !t.contains(['\n', '\r']);
    if !id_ok(&e.id) {
        return Some("id");
    }
    if !line(&e.name, 60) || !line(&e.author, 60) || !line(&e.description, 300) || !line(&e.license, 40) || version(&e.version).is_none() {
        return Some("text");
    }
    if e.files.is_empty() || e.files.len() > 200 || !e.files.iter().any(|f| f.path == "main.lua") {
        return Some("files");
    }
    if e.files.iter().any(|f| !path_ok(&f.path) || !hex_ok(&f.sha256) || f.size > MAX_FILE) {
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

/// The catalog version of the plugin installed in `plugins/<id>/` (None: not from the
/// catalog, or not there).
pub fn installed(plugins: &Path, id: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(plugins.join(id).join(MARK)).ok()?).ok()?;
    (v["id"].as_str() == Some(id)).then(|| v["version"].as_str().map(str::to_string)).flatten()
}

/// Download (`fetch`: a URL's bytes), check and install `e` into `plugins/<id>/`, keeping its
/// `data` folder on an update. Nothing changes when a file does not match.
pub fn install(plugins: &Path, e: &Entry, fetch: &dyn Fn(&str) -> Result<Vec<u8>>) -> Result<()> {
    if let Some(why) = problem(e) {
        bail!("{} is not a plugin of the catalog ({why})", e.id);
    }
    let target = plugins.join(&e.id);
    if target.exists() && installed(plugins, &e.id).is_none() {
        bail!("plugins/{} is there already and did not come from the catalog: rename or remove it first", e.id);
    }
    std::fs::create_dir_all(plugins)?;
    // (beside the plugins folder, on the same disk, where the game finds no plugin)
    let parent = plugins.parent().unwrap_or(plugins);
    let tmp = parent.join(format!(".plugin-install-{}", e.id));
    let _ = std::fs::remove_dir_all(&tmp);
    let result = (|| -> Result<()> {
        for f in &e.files {
            let bytes = fetch(&format!("{FILES_URL}{}/{}", e.id, f.path)).with_context(|| format!("{}: {}", e.id, f.path))?;
            let got: String = sha2::Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
            if bytes.len() as u64 != f.size || got != f.sha256 {
                bail!("{}: {} is not the file the catalog names (changed meanwhile? try again)", e.id, f.path);
            }
            let to = tmp.join(&f.path);
            std::fs::create_dir_all(to.parent().unwrap_or(&tmp))?;
            std::fs::write(&to, bytes)?;
        }
        std::fs::write(tmp.join(MARK), serde_json::to_string_pretty(&serde_json::json!({ "id": e.id, "version": e.version }))?)?;
        if target.exists() {
            if target.join("data").is_dir() {
                std::fs::rename(target.join("data"), tmp.join("data"))?;
            }
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::rename(&tmp, &target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    result
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
    let content = crate::content_dir().ok_or_else(|| anyhow!(crate::i18n::tr("no game binary configured, so no content folder")))?;
    Ok(crate::plugins::child_dir(&content, "plugins").unwrap_or_else(|| content.join("plugins")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(b: &[u8]) -> String {
        sha2::Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
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
            files: vec![FileRef { path: "main.lua".into(), size: main.len() as u64, sha256: sha(main) }],
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
        let text = serde_json::json!({ "version": 1, "plugins": [good, escape, exe, { "id": 5 }] }).to_string();
        let list = parse(&text).unwrap();
        assert_eq!(list.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["speed-warning"]);
        assert!(parse(r#"{"version": 2, "plugins": []}"#).is_err());
        assert!(!path_ok("data/settings.json") && path_ok("data.lua") && !path_ok(".catalog.json"));
        assert!(version("1.2.3") > version("1.2.0") && version("1.2").is_none());
    }

    #[test]
    fn installs_checks_updates_keeps_data_and_removes() {
        let root = std::env::temp_dir().join(format!("plugin-catalog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let plugins = root.join("plugins");
        let v1 = entry("1.0.0", b"-- one");
        install(&plugins, &v1, &|_| Ok(b"-- one".to_vec())).unwrap();
        assert_eq!(installed(&plugins, "speed-warning").as_deref(), Some("1.0.0"));
        std::fs::create_dir_all(plugins.join("speed-warning/data")).unwrap();
        std::fs::write(plugins.join("speed-warning/data/settings.json"), "{}").unwrap();

        // a file that is not the catalog's: nothing changes
        let v2 = entry("1.1.0", b"-- two");
        assert!(install(&plugins, &v2, &|_| Ok(b"-- evil".to_vec())).is_err());
        assert_eq!(std::fs::read(plugins.join("speed-warning/main.lua")).unwrap(), b"-- one");
        assert!(!root.join(".plugin-install-speed-warning").exists());

        install(&plugins, &v2, &|url| {
            assert!(url.ends_with("/plugins/speed-warning/main.lua"));
            Ok(b"-- two".to_vec())
        })
        .unwrap();
        assert_eq!(std::fs::read(plugins.join("speed-warning/main.lua")).unwrap(), b"-- two");
        assert!(plugins.join("speed-warning/data/settings.json").is_file());
        assert_eq!(installed(&plugins, "speed-warning").as_deref(), Some("1.1.0"));

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
}
