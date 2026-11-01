//! User-visible cache budgets. These apply to the *cache* and the joining player's
//! *sealed LAN store*, not the full ~/.openomsi folder (settings, saves and mods live there).
//! Cleanup is suspended while any game holds a cache lease, including CLI runs.

use serde::Serialize;
use omsi_cfg::cache_budget::{self, Kind};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Serialize)]
pub struct Status {
    pub local_bytes: u64,
    pub local_limit: u64,
    pub lan_bytes: u64,
    pub lan_limit: u64,
    pub reclaimed_local: u64,
    pub reclaimed_lan: u64,
    pub busy: bool,
    pub server_store: bool,
}

fn running() -> bool {
    crate::instances::list().iter().any(|game| game.running)
        || omsi_cfg::cache_activity::maintenance(&crate::data_dir()).is_none()
}

pub fn status() -> Status {
    let base = crate::data_dir();
    let local = cache_budget::usage(&base.join("cache"), Kind::Local);
    let lan = cache_budget::usage(&base.join("lan-store"), Kind::ClientLan);
    Status {
        local_bytes: local.bytes, local_limit: cache_budget::LOCAL_LIMIT,
        lan_bytes: lan.bytes, lan_limit: cache_budget::LAN_LIMIT,
        reclaimed_local: 0, reclaimed_lan: 0,
        busy: running(),
        server_store: lan.server_store,
    }
}

/// A no-disk-access preview is supplied separately by status(). Cleanup only ever
/// modifies the two dedicated cache directories. Explicit "clear" sets budget to zero.
pub fn cleanup(scope: &str, clear: bool) -> Result<Status, String> {
    if !matches!(scope, "local" | "lan" | "both") {
        return Err(crate::i18n::tr("Unknown cache scope."));
    }
    if running() {
        return Err(crate::i18n::tr("Close all running openOMSI games before cleaning the cache."));
    }
    let base = crate::data_dir();
    let _maintenance = omsi_cfg::cache_activity::maintenance(&base)
        .ok_or_else(|| crate::i18n::tr("Close all running openOMSI games before cleaning the cache."))?;
    let mut reclaimed_local = 0;
    let mut reclaimed_lan = 0;
    if matches!(scope, "local" | "both") {
        let u = cache_budget::trim(&base.join("cache"), Kind::Local,
            if clear { 0 } else { cache_budget::LOCAL_LIMIT });
        reclaimed_local = u.freed;
    }
    if matches!(scope, "lan" | "both") {
        let u = cache_budget::trim(&base.join("lan-store"), Kind::ClientLan,
            if clear { 0 } else { cache_budget::LAN_LIMIT });
        reclaimed_lan = u.freed;
    }
    drop(_maintenance);
    let mut state = status();
    state.reclaimed_local = reclaimed_local;
    state.reclaimed_lan = reclaimed_lan;
    Ok(state)
}

/// The launcher invokes this on regular polls. Disk scanning is throttled, performed
/// away from the UI thread, and deferred until no registered game is running.
pub fn automatic_cleanup() {
    if cfg!(any(target_os = "android", target_os = "ios")) { return; }
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.is_some_and(|t| t.elapsed() < Duration::from_secs(120)) { return; }
    if running() { return; }
    *last = Some(Instant::now());
    std::thread::spawn(|| {
        // A game might have started since the idle check: cleanup re-checks.
        let _ = cleanup("both", false);
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_known_scopes_are_allowed() {
        assert!(super::cleanup("../../settings", false).is_err());
    }
}
