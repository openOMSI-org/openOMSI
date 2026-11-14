//! What became of each plugin of a session, for the launcher's plugin list (#3919):
//! loaded, left off by the player, failed to load (a syntax error), or switched off while
//! the game ran. The game writes it as JSON beside its settings; the launcher only reads it.

use std::path::{Path, PathBuf};

/// In the game's data folder (`~/.openomsi`): the plugins the player switched off in the
/// launcher, one name a line ([`key`]).
pub const OFF_FILE: &str = "plugins-off.txt";
/// In the game's data folder: the last session's [`Report`].
pub const STATUS_FILE: &str = "plugins-status.json";

/// The names in `dir`'s [`OFF_FILE`] (none without one).
pub fn read_off(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join(OFF_FILE))
        .map(|t| t.lines().map(key).filter(|l| !l.is_empty() && !l.starts_with('#')).collect())
        .unwrap_or_default()
}

/// One plugin's line.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    /// The name the launcher switches it by: a `.lua` file's stem, a folder's name, a
    /// `.oop`'s or `.opl`'s stem (in lower case).
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

impl Status {
    pub fn new(file: Option<PathBuf>) -> Status {
        let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        Status { file, report: Report { at, plugins: Vec::new() } }
    }

    pub fn add(&mut self, name: &str, path: &Path, kind: &str, state: &str, error: &str) {
        self.report.plugins.push(Entry {
            name: key(name),
            path: path.display().to_string(),
            kind: kind.into(),
            state: state.into(),
            error: error.lines().next().unwrap_or("").chars().take(400).collect(),
        });
    }

    /// A loaded plugin the game switched off (too many errors, a call too long).
    pub fn switched_off(&mut self, name: &str, error: &str) {
        let k = key(name);
        if let Some(e) = self.report.plugins.iter_mut().find(|e| e.name == k && e.state == "loaded") {
            e.state = "switched_off".into();
            e.error = error.lines().next().unwrap_or("").chars().take(400).collect();
            self.write();
        }
    }

    /// Written whole to a temporary file and moved over the old one (the launcher may be
    /// reading it).
    pub fn write(&self) {
        let Some(file) = &self.file else { return };
        let Ok(text) = serde_json::to_string_pretty(&self.report) else { return };
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
        s.add("Cruise", std::path::Path::new("plugins/cruise.lua"), "lua", "loaded", "");
        s.add("broken", std::path::Path::new("plugins/broken.lua"), "lua", "failed", "syntax error near 'end'\nstack");
        s.write();
        s.switched_off("CRUISE", "attempt to index a nil value");
        let back: Report = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(back.plugins[0].state, "switched_off");
        assert_eq!(back.plugins[1].error, "syntax error near 'end'");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
