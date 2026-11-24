//! The start journal (`guard/starts.json` in the data folder): the last starts, whether each
//! got to its first frames, how it ended - and what the guard decided about a version.

use std::path::PathBuf;

use serde_json::{Value, json};

/// What started: the launcher (watched) or a game session it started (noted only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Launcher,
    Game,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Launcher => "launcher",
            Kind::Game => "game",
        }
    }
}

/// How a start ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// On its own, without an error.
    Clean,
    /// On its own, with an error (before its first frames: a failed start).
    Error,
    /// The process went away without saying so.
    Crash,
    /// It did not get to its first frames in time and was ended.
    Hang,
}

impl End {
    fn name(self) -> &'static str {
        match self {
            End::Clean => "clean",
            End::Error => "error",
            End::Crash => "crash",
            End::Hang => "hang",
        }
    }
}

/// One start as the journal keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub kind: String,
    pub version: String,
    pub pid: u32,
    pub healthy: bool,
    /// `clean`, `error`, `crash`, `hang`; empty while it runs (or when nobody saw it end).
    pub end: String,
}

impl Entry {
    /// Whether this start failed: it ended before its first frames other than cleanly - or no
    /// one saw it end and its process is gone (a phone, a watcher that could not run).
    pub fn failed(&self, alive: impl Fn(u32) -> bool) -> bool {
        if self.healthy {
            return false;
        }
        match self.end.as_str() {
            "error" | "crash" | "hang" => true,
            "" => !alive(self.pid),
            _ => false,
        }
    }
}

/// The journal: the starts (newest last) and the guard's notes per version.
#[derive(Clone, Debug, Default)]
pub struct Journal {
    pub starts: Vec<Entry>,
    /// Versions safe mode was tried for, and ones the guard gave up on (told the player).
    pub safe_tried: Vec<String>,
    pub gave_up: Vec<String>,
    /// The project lists this version as broken (seen by a watcher), with its message.
    pub remote_bad: Option<(String, String)>,
}

/// How many starts are kept.
const KEEP: usize = 40;

fn path() -> PathBuf {
    omsi_platform::recovery::guard_dir().join("starts.json")
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

impl Journal {
    pub fn load() -> Journal {
        let Some(v) = std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        else {
            return Journal::default();
        };
        let starts = v["starts"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|e| Entry {
                        kind: e["kind"].as_str().unwrap_or("").to_string(),
                        version: e["version"].as_str().unwrap_or("").to_string(),
                        pid: e["pid"].as_u64().unwrap_or(0) as u32,
                        healthy: e["healthy"].as_bool().unwrap_or(false),
                        end: e["end"].as_str().unwrap_or("").to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let remote_bad = v["remote_bad"]["version"].as_str().map(|ver| {
            (
                ver.to_string(),
                v["remote_bad"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
            )
        });
        Journal {
            starts,
            safe_tried: strings(&v["safe_tried"]),
            gave_up: strings(&v["gave_up"]),
            remote_bad,
        }
    }

    pub fn save(&self) {
        let starts: Vec<Value> = self.starts.iter().map(|e| json!({"kind": e.kind, "version": e.version, "pid": e.pid, "healthy": e.healthy, "end": e.end})).collect();
        let mut v =
            json!({"starts": starts, "safe_tried": self.safe_tried, "gave_up": self.gave_up});
        if let Some((ver, msg)) = &self.remote_bad {
            v["remote_bad"] = json!({"version": ver, "message": msg});
        }
        let _ = omsi_platform::recovery::write_atomic(
            &path(),
            &serde_json::to_string_pretty(&v).unwrap_or_default(),
        );
    }

    pub fn begin(&mut self, kind: Kind, version: &str, _build: &str, pid: u32) {
        self.starts.push(Entry {
            kind: kind.name().to_string(),
            version: version.to_string(),
            pid,
            healthy: false,
            end: String::new(),
        });
        let n = self.starts.len();
        if n > KEEP {
            self.starts.drain(..n - KEEP);
        }
    }

    fn entry(&mut self, pid: u32) -> Option<&mut Entry> {
        self.starts
            .iter_mut()
            .rev()
            .find(|e| e.pid == pid && e.end.is_empty())
    }

    pub fn mark_healthy(&mut self, pid: u32) {
        if let Some(e) = self.entry(pid) {
            e.healthy = true;
        }
    }

    pub fn end(&mut self, pid: u32, end: End) {
        if let Some(e) = self.entry(pid) {
            e.end = end.name().to_string();
        }
    }

    /// The failed starts of `kind` of `version` since its last good one, and whether it ever
    /// started well here.
    pub fn record(&self, kind: Kind, version: &str, alive: impl Fn(u32) -> bool) -> (usize, bool) {
        let mine: Vec<&Entry> = self
            .starts
            .iter()
            .filter(|e| e.kind == kind.name() && e.version == version)
            .collect();
        let ever_healthy = mine.iter().any(|e| e.healthy);
        let failures = mine
            .iter()
            .rev()
            .take_while(|e| !e.healthy)
            .filter(|e| e.failed(&alive))
            .count();
        (failures, ever_healthy)
    }

    /// Whether `version` is broken here: a start of it failed and it never started well (a
    /// fresh update), or two starts in a row failed.
    pub fn broken(&self, kind: Kind, version: &str, alive: impl Fn(u32) -> bool) -> bool {
        let (failures, ever_healthy) = self.record(kind, version, alive);
        (failures >= 1 && !ever_healthy) || failures >= 2
    }
}

/// Load, change, save (the launcher, its watcher and its game sessions each write their own
/// starts; the file is replaced whole, so a reader never sees half of it).
pub fn update(f: impl FnOnce(&mut Journal)) {
    let mut j = Journal::load();
    f(&mut j);
    j.save();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &str, version: &str, pid: u32, healthy: bool, end: &str) -> Entry {
        Entry {
            kind: kind.into(),
            version: version.into(),
            pid,
            healthy,
            end: end.into(),
        }
    }

    #[test]
    fn a_fresh_version_that_fails_once_is_broken_an_old_good_one_after_two() {
        let dead = |_| false;
        let mut j = Journal::default();
        j.starts
            .push(entry("launcher", "0.2.31", 10, false, "crash"));
        assert!(j.broken(Kind::Launcher, "0.2.31", dead));
        let mut j = Journal::default();
        j.starts.push(entry("launcher", "0.2.30", 9, true, "clean"));
        j.starts
            .push(entry("launcher", "0.2.30", 10, false, "crash"));
        assert!(
            !j.broken(Kind::Launcher, "0.2.30", dead),
            "one crash of a version that ran well is no broken release"
        );
        j.starts
            .push(entry("launcher", "0.2.30", 11, false, "hang"));
        assert!(j.broken(Kind::Launcher, "0.2.30", dead));
        j.starts
            .push(entry("launcher", "0.2.30", 12, true, "clean"));
        assert!(
            !j.broken(Kind::Launcher, "0.2.30", dead),
            "a good start clears it"
        );
    }

    #[test]
    fn an_unended_start_whose_process_is_gone_failed_a_running_one_did_not() {
        let mut j = Journal::default();
        j.starts.push(entry("game", "0.2.31", 20, false, ""));
        assert!(!j.broken(Kind::Game, "0.2.31", |_| true));
        assert!(j.broken(Kind::Game, "0.2.31", |_| false));
        // a clean end before the first frames (the player closed it) is no failure
        let mut j = Journal::default();
        j.starts
            .push(entry("launcher", "0.2.31", 21, false, "clean"));
        assert!(!j.broken(Kind::Launcher, "0.2.31", |_| false));
        // other kinds and versions do not count
        assert!(!j.broken(Kind::Game, "0.2.31", |_| false));
    }

    #[test]
    fn the_journal_keeps_its_last_starts_and_notes() {
        let mut j = Journal::default();
        for pid in 0..(KEEP as u32 + 5) {
            j.begin(Kind::Launcher, "0.2.31", "b", pid);
        }
        assert_eq!(j.starts.len(), KEEP);
        assert_eq!(j.starts[0].pid, 5);
        j.mark_healthy(KEEP as u32 + 4);
        j.end(KEEP as u32 + 4, End::Clean);
        assert!(j.starts.last().unwrap().healthy);
        assert_eq!(j.starts.last().unwrap().end, "clean");
    }
}
