//! The players this player does not want to hear, kept between sessions. A player is known
//! to the others by name (the host's hardware identifier never reaches the other games),
//! so a mute is by name, case ignored. Nothing but the names is stored.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// The most names kept (a list that grows without end is a file that is never read again).
const MAX_ENTRIES: usize = 500;
const MAX_NAME: usize = 64;

#[derive(Default)]
pub struct MuteList {
    names: BTreeSet<String>,
    path: Option<PathBuf>,
}

fn key(name: &str) -> String {
    name.trim()
        .chars()
        .take(MAX_NAME)
        .collect::<String>()
        .to_lowercase()
}

impl MuteList {
    /// The list in `path` (missing or unreadable: empty), saved there when it changes.
    pub fn load(path: PathBuf) -> MuteList {
        let names = std::fs::read_to_string(&path)
            .map(|t| {
                t.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .map(key)
                    .take(MAX_ENTRIES)
                    .collect()
            })
            .unwrap_or_default();
        MuteList {
            names,
            path: Some(path),
        }
    }

    pub fn is_muted(&self, name: &str) -> bool {
        self.names.contains(&key(name))
    }

    /// Mute or unmute `name`; false when the list is full.
    pub fn set(&mut self, name: &str, muted: bool) -> bool {
        let k = key(name);
        if k.is_empty() {
            return false;
        }
        if muted {
            if self.names.len() >= MAX_ENTRIES && !self.names.contains(&k) {
                return false;
            }
            self.names.insert(k);
        } else {
            self.names.remove(&k);
        }
        self.save();
        true
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    fn save(&self) {
        let Some(p) = &self.path else { return };
        let text = format!(
            "# players whose voice chat is muted (one name per line)\n{}\n",
            self.names.iter().cloned().collect::<Vec<_>>().join("\n")
        );
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(p, text) {
            log::warn!("voice: the mute list could not be saved: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutes_are_by_name_ignore_case_and_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("omsi-voice-mutes-{}", std::process::id()));
        let path = dir.join("voice_mutes.txt");
        let mut m = MuteList::load(path.clone());
        assert!(m.set("Pichau", true));
        assert!(m.set("  Anna K ", true));
        assert!(!m.set("   ", true));
        assert!(m.is_muted("pichau") && m.is_muted("ANNA K") && !m.is_muted("Bob"));
        let again = MuteList::load(path.clone());
        assert!(again.is_muted("Pichau") && again.is_muted("anna k") && again.len() == 2);
        let mut m = again;
        m.set("PICHAU", false);
        assert!(!MuteList::load(path).is_muted("pichau"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_list_is_bounded() {
        let mut m = MuteList::default();
        for i in 0..MAX_ENTRIES {
            assert!(m.set(&format!("p{i}"), true));
        }
        assert!(!m.set("one too many", true));
        assert!(m.set("p3", true), "a name already there is no new entry");
        assert!(m.set("p3", false));
        assert!(m.set("one too many", true));
    }
}
