//! The audit log: who did what to whom, when, and what became of it - the logins, every
//! operation, whatever way it came. The last entries are kept in memory for the panels; with a
//! file (`admin-audit.log` beside `server.cfg`) each one is also a JSON line there, the file
//! put aside as `.1` when it passes its size, the `.1` before as `.2` (so at most three times
//! that on the disk), readable by the server's own account only. No password
//! or token is ever given to it, and its texts are cleaned and cut like an operation's.

use crate::op::clean;
use crate::perm::Role;
use serde::Serialize;
use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;

/// The entries kept in memory.
const KEEP: usize = 500;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Entry {
    /// Counted up from 1 for this server run (`after=` of the events).
    pub seq: u64,
    /// Seconds since 1970.
    pub at: u64,
    /// Who: `web#2 (192.0.2.1)`, `player 5 'Name'`, `local tool`.
    pub actor: String,
    pub role: Option<Role>,
    /// `login`, `logout`, or the operation (`kick`, `clock` …).
    pub action: String,
    /// Whom or what (`player 3 'Name'`), empty when nobody.
    pub target: String,
    /// `ok`, `refused`, `queued`, `applied`, `rejected`, `pending`.
    pub result: String,
    pub detail: String,
}

/// Who, without the address (`web#2 (192.0.2.1)` → `web#2`): for whom may not see it.
pub fn without_address(actor: &str) -> String {
    match actor.rsplit_once(" (") {
        Some((who, rest)) if rest.strip_suffix(')').is_some_and(|a| a.parse::<std::net::IpAddr>().is_ok()) => who.to_string(),
        _ => actor.to_string(),
    }
}

pub struct Audit {
    entries: VecDeque<Entry>,
    seq: u64,
    file: Option<PathBuf>,
    max_bytes: u64,
}

impl Audit {
    /// `file`: the log on the disk (None: in memory only); `max_bytes`: when it is put aside.
    pub fn new(file: Option<PathBuf>, max_bytes: u64) -> Audit {
        Audit { entries: VecDeque::new(), seq: 0, file, max_bytes: max_bytes.max(4096) }
    }

    /// Note something done. Its number.
    pub fn record(&mut self, actor: &str, role: Option<Role>, action: &str, target: &str, result: &str, detail: &str) -> u64 {
        self.seq += 1;
        let e = Entry { seq: self.seq, at: crate::unix_now(), actor: clean(actor, 80), role, action: clean(action, 40), target: clean(target, 120), result: clean(result, 20), detail: clean(detail, 200) };
        // (the general log is no account's own: no address in it)
        log::info!("admin audit: {} [{}] {} {} -> {} {}", without_address(&e.actor), e.role.map(Role::name).unwrap_or("-"), e.action, e.target, e.result, e.detail);
        self.write(&e);
        if self.entries.len() >= KEEP {
            self.entries.pop_front();
        }
        self.entries.push_back(e);
        self.seq
    }

    fn write(&self, e: &Entry) {
        let Some(path) = self.file.as_ref() else { return };
        let Ok(line) = serde_json::to_string(e) else { return };
        if std::fs::metadata(path).map(|m| m.len() + line.len() as u64 > self.max_bytes).unwrap_or(false) {
            // (two put aside: `.1` the last, `.2` the one before)
            let aside = |n: u8| {
                let mut p = path.clone().into_os_string();
                p.push(format!(".{n}"));
                PathBuf::from(p)
            };
            let _ = std::fs::rename(aside(1), aside(2));
            if let Err(err) = std::fs::rename(path, aside(1)) {
                log::warn!("admin audit: {} not put aside: {err}", path.display());
            }
        }
        let mut open = std::fs::OpenOptions::new();
        open.create(true).append(true);
        // (the records name the admins' addresses: for the server's own account only)
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
        let r = open.open(path).and_then(|mut f| {
            // (a file of before, made with another mode, too)
            #[cfg(unix)]
            f.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
            writeln!(f, "{line}")
        });
        if let Err(err) = r {
            log::warn!("admin audit: not written to {}: {err}", path.display());
        }
    }

    /// The entries after `seq`, oldest first, at most `max`.
    pub fn since(&self, seq: u64, max: usize) -> Vec<Entry> {
        let mut v: Vec<Entry> = self.entries.iter().filter(|e| e.seq > seq).cloned().collect();
        if v.len() > max {
            v.drain(..v.len() - max);
        }
        v
    }

    pub fn last_seq(&self) -> u64 {
        self.seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_keeps_the_last_and_cleans_its_texts() {
        let mut a = Audit::new(None, 0);
        for k in 0..(KEEP + 10) {
            a.record("web#1 (192.0.2.1)", Some(Role::Admin), "say", "", "applied", &format!("line {k}"));
        }
        let all = a.since(0, usize::MAX);
        assert_eq!(all.len(), KEEP);
        assert_eq!(all.last().map(|e| e.seq), Some((KEEP + 10) as u64));
        assert_eq!(a.since(a.last_seq() - 3, 10).len(), 3);
        assert_eq!(a.since(0, 2).iter().map(|e| e.seq).collect::<Vec<_>>(), vec![(KEEP + 9) as u64, (KEEP + 10) as u64]);
        a.record("player 2 'x\nkick 3'", None, "kick", "player 3 '<b>'", "applied", "why\r\nnot");
        let e = a.since(a.last_seq() - 1, 1).remove(0);
        assert_eq!((e.actor.as_str(), e.target.as_str(), e.detail.as_str()), ("player 2 'x kick 3'", "player 3 '<b>'", "why not"));
    }

    /// A log made before with another mode (`0644`) is the server's own account's too once
    /// it is written to; the address is out of the general log and of a moderator's view.
    #[cfg(unix)]
    #[test]
    fn a_log_of_before_is_closed_too() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("omsi-audit-mode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("admin-audit.log");
        std::fs::write(&path, "").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        Audit::new(Some(path.clone()), 1 << 20).record("web#1 (192.0.2.1)", Some(Role::Admin), "say", "", "applied", "");
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_address_can_be_left_out() {
        assert_eq!(without_address("web#2 (192.0.2.1)"), "web#2");
        assert_eq!(without_address("web (2001:db8::1)"), "web");
        assert_eq!(without_address("player 5 'Name (x)'"), "player 5 'Name (x)'");
        assert_eq!(without_address("web#1 (admin)"), "web#1 (admin)");
        assert_eq!(without_address("local tool"), "local tool");
    }

    #[test]
    fn the_file_is_put_aside_when_it_grows() {
        let dir = std::env::temp_dir().join(format!("omsi-audit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("admin-audit.log");
        let mut a = Audit::new(Some(path.clone()), 4096);
        for k in 0..100 {
            a.record("local tool", Some(Role::Admin), "say", "", "applied", &format!("a line of some length to fill the file {k}"));
        }
        let now = std::fs::metadata(&path).unwrap().len();
        let old = std::fs::metadata(dir.join("admin-audit.log.1")).unwrap().len();
        let older = std::fs::metadata(dir.join("admin-audit.log.2")).unwrap().len();
        assert!(now <= 4096 && old <= 4096 && older <= 4096, "{now} {old} {older}");
        // (the server's own account alone reads who did what)
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&path).unwrap().permissions()) & 0o777, 0o600);
        // each line a JSON object, nothing secret in it
        let text = std::fs::read_to_string(&path).unwrap();
        for line in text.lines() {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(v["action"], "say");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
