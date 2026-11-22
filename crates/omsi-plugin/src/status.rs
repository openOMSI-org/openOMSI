//! What became of each plugin of a session, for the launcher's plugin list (#3919):
//! loaded, left off by the player, failed to load (a syntax error), or switched off while
//! the game ran. The game writes it as JSON beside its settings; the launcher only reads it.

use std::path::{Path, PathBuf};

/// In the game's data folder (`~/.openomsi`): the plugins the player switched off in the
/// launcher, one name a line ([`key`]): [`plugin_key`] or [`opl_key`], or a bare stem an
/// older launcher wrote ([`is_off`]).
pub const OFF_FILE: &str = "plugins-off.txt";
/// In the game's data folder: the last session's [`Report`].
pub const STATUS_FILE: &str = "plugins-status.json";

/// The names in `dir`'s [`OFF_FILE`] (none without one).
pub fn read_off(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join(OFF_FILE))
        .map(|t| {
            t.lines()
                .map(key)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect()
        })
        .unwrap_or_default()
}

/// The game's data folder (`~/.openomsi`), where [`OFF_FILE`] and [`STATUS_FILE`] are.
pub fn game_data() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| PathBuf::from(h).join(".openomsi"))
}

/// A session that runs no plugin (plugins disabled): an empty report in `data` replaces the
/// last session's, so the launcher no longer shows a plugin as run in the last game.
pub fn write_none(data: Option<&Path>) {
    Status::new(data.map(|d| d.join(STATUS_FILE))).write();
}

/// One plugin's line.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    /// The name the launcher switches it by (in lower case): its path under `plugins`
    /// ([`plugin_key`], [`opl_key`]).
    pub name: String,
    pub path: String,
    /// `"lua"`, `"oop"` or `"opl"`.
    pub kind: String,
    /// `"loaded"`, `"off"`, `"failed"` or `"switched_off"`.
    pub state: String,
    /// What went wrong, for `failed` and `switched_off`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Report {
    /// Seconds since 1970 when the session loaded its plugins.
    pub at: u64,
    pub plugins: Vec<Entry>,
}

/// The report of the running session and where it goes.
#[derive(Debug, Default)]
pub struct Status {
    pub file: Option<PathBuf>,
    pub report: Report,
}

/// A plugin's name as the launcher's off-list holds it.
pub fn key(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

/// An `.opl` plugin's name: its path under the `plugins` folder `dir`, `/` between the parts
/// (`Pablo/MBAS.opl`), so two `Config.opl` in different subfolders are switched apart.
pub fn opl_key(opl: &Path, dir: &Path) -> String {
    let rel = opl.strip_prefix(dir).unwrap_or(opl);
    key(&rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/"))
}

/// A Lua or `.oop` plugin's name: its path under the `plugins` folder `dir` (`foo.lua`,
/// `foo.oop`), a folder plugin's folder with a `/` after it (`foo/` for `foo/main.lua`), so
/// `foo.lua` and `foo/main.lua` are switched apart.
pub fn plugin_key(path: &Path, dir: &Path) -> String {
    let main = path
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("main.lua"))
        && path.parent() != Some(dir);
    match path.parent().filter(|_| main) {
        Some(folder) => format!("{}/", opl_key(folder, dir)),
        None => opl_key(path, dir),
    }
}

/// Whether the plugin `key` is in the off-list `off`; `stem` is the name an older launcher
/// wrote for it (a `.lua`'s, a folder's or a `.oop`'s stem), which still leaves out every
/// plugin of that stem, so a player's list is kept.
pub fn is_off(off: &[String], key: &str, stem: &str) -> bool {
    let stem = self::key(stem);
    off.iter().any(|n| *n == key || *n == stem)
}

impl Status {
    pub fn new(file: Option<PathBuf>) -> Status {
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Status {
            file,
            report: Report {
                at,
                plugins: Vec::new(),
            },
        }
    }

    pub fn add(&mut self, name: &str, path: &Path, kind: &str, state: &str, error: &str) {
        self.report.plugins.push(Entry {
            name: key(name),
            path: path.display().to_string(),
            kind: kind.into(),
            state: state.into(),
            error: error
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(400)
                .collect(),
        });
    }

    /// A loaded plugin the game switched off (too many errors, a call too long), by the file
    /// it was loaded from: two plugins may have one name.
    pub fn switched_off(&mut self, path: &Path, error: &str) {
        let path = path.display().to_string();
        if let Some(e) = self
            .report
            .plugins
            .iter_mut()
            .find(|e| e.path == path && e.state == "loaded")
        {
            e.state = "switched_off".into();
            e.error = error
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(400)
                .collect();
            self.write();
        }
    }

    /// Written whole to a temporary file and moved over the old one (the launcher may be
    /// reading it).
    pub fn write(&self) {
        let Some(file) = &self.file else { return };
        let Ok(text) = serde_json::to_string_pretty(&self.report) else {
            return;
        };
        let tmp = file.with_extension("json.tmp");
        if let Err(e) = std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, file)) {
            log::warn!("plugin status not written to {}: {e}", file.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_switched_off_plugin_is_noted_and_written() {
        let dir = std::env::temp_dir().join(format!("omsi-plugin-status-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("plugins-status.json");
        let mut s = Status::new(Some(file.clone()));
        s.add(
            "Cruise",
            std::path::Path::new("plugins/cruise.lua"),
            "lua",
            "loaded",
            "",
        );
        s.add(
            "broken",
            std::path::Path::new("plugins/broken.lua"),
            "lua",
            "failed",
            "syntax error near 'end'\nstack",
        );
        s.write();
        s.switched_off(
            std::path::Path::new("plugins/cruise.lua"),
            "attempt to index a nil value",
        );
        let back: Report = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(back.plugins[0].state, "switched_off");
        assert_eq!(back.plugins[1].error, "syntax error near 'end'");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_session_without_plugins_clears_the_last_report() {
        let dir = std::env::temp_dir().join(format!("omsi-plugin-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Status::new(Some(dir.join(STATUS_FILE)));
        s.add(
            "cruise",
            Path::new("plugins/cruise.lua"),
            "lua",
            "loaded",
            "",
        );
        s.write();
        write_none(Some(&dir));
        let back: Report =
            serde_json::from_str(&std::fs::read_to_string(dir.join(STATUS_FILE)).unwrap()).unwrap();
        assert!(back.plugins.is_empty());
        assert!(back.at > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_opl_is_named_by_its_path_under_plugins() {
        let dir = Path::new("omsi/plugins");
        assert_eq!(
            opl_key(&dir.join("Pablo").join("MBAS.opl"), dir),
            "pablo/mbas.opl"
        );
        assert_ne!(
            opl_key(&dir.join("Pablo").join("Config.opl"), dir),
            opl_key(&dir.join("Config.opl"), dir)
        );
    }

    #[test]
    fn a_lua_file_and_a_folder_of_one_name_are_two_plugins() {
        let dir = Path::new("omsi/plugins");
        let file = plugin_key(&dir.join("Foo.lua"), dir);
        let folder = plugin_key(&dir.join("Foo").join("main.lua"), dir);
        assert_eq!((file.as_str(), folder.as_str()), ("foo.lua", "foo/"));
        assert_eq!(plugin_key(&dir.join("main.lua"), dir), "main.lua");
        assert_eq!(plugin_key(&dir.join("foo.oop"), dir), "foo.oop");
        let off = ["foo/".to_string()];
        assert!(is_off(&off, &folder, "Foo") && !is_off(&off, &file, "Foo"));
        // (an older launcher's bare name leaves out both, as it did)
        let old = ["foo".to_string()];
        assert!(is_off(&old, &folder, "Foo") && is_off(&old, &file, "Foo"));
    }
}
