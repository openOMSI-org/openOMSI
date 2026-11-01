//! Disk budgets for *rebuildable* local cache and the joining player's sealed LAN store.
//! 1 GB local + 4 GB LAN, independent, soft limits: active/recent downloads may exceed
//! the quota temporarily, then old eligible data is reclaimed at safe maintenance points.
//! Never walk symlinks or touch settings, saves, installed mods or server vehicle manifests.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const LOCAL_LIMIT: u64 = 1_000_000_000;
pub const LAN_LIMIT: u64 = 4_000_000_000;
const TRANSFER_GRACE: Duration = Duration::from_secs(600);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Local,
    ClientLan,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Usage {
    pub bytes: u64,
    pub freed: u64,
    /// Server-side bus manifests must retain their blobs under the server's own policy.
    pub server_store: bool,
}

struct Candidate {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
    eligible: bool,
}

/// Examine the known cache roots only, never arbitrary paths supplied by a multiplayer host.
/// Missing directories and inaccessible entries fail closed (no deletion).
fn scan(root: &Path, kind: Kind) -> (Usage, Vec<Candidate>) {
    let mut usage = Usage::default();
    let Ok(meta) = fs::symlink_metadata(root) else { return (usage, vec![]) };
    if !meta.file_type().is_dir() { return (usage, vec![]) }
    usage.server_store = kind == Kind::ClientLan && root.join("vehicles").is_dir();
    let mut dirs = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(md) = fs::symlink_metadata(&path) else { continue };
            if md.file_type().is_symlink() { continue }
            if md.file_type().is_dir() {
                if kind == Kind::Local { dirs.push(path); }
                continue;
            }
            if !md.file_type().is_file() { continue; }
            if kind == Kind::ClientLan {
                // The client store's sealed objects; never the encryption key, a manifest,
                // metadata or an unknown file. The server also uses this directory, so
                // when vehicle manifests exist we report the usage but never evict.
                if !matches!(path.extension().and_then(|x| x.to_str()),
                    Some("blob" | "piece" | "zpiece" | "partpiece" | "part")) { continue; }
            }
            let modified = md.modified().unwrap_or(UNIX_EPOCH);
            usage.bytes = usage.bytes.saturating_add(md.len());
            // In-progress files are never reaped immediately. The packed zpiece is
            // itself a partial transfer artifact and must receive the same grace.
            let temporary = temporary(&path, kind);
            let eligible = !usage.server_store &&
                (!temporary || modified.elapsed().is_ok_and(|age| age >= TRANSFER_GRACE));
            files.push(Candidate { path, bytes: md.len(), modified, eligible });
        }
    }
    (usage, files)
}

fn temporary(path: &Path, kind: Kind) -> bool {
    let ext = path.extension().and_then(|x| x.to_str());
    match kind {
        Kind::Local => matches!(ext, Some("tmp" | "part")),
        Kind::ClientLan => matches!(ext, Some("piece" | "zpiece" | "partpiece" | "part")),
    }
}

pub fn usage(root: &Path, kind: Kind) -> Usage {
    scan(root, kind).0
}

/// Reclaim oldest eligible files to the soft budget, or zero on a *confirmed* manual clear.
/// Must only be called when no game is actively using this cache, or after a LAN session
/// has unmounted its store. In-use/young LAN files are never touched. The caller decides
/// when it is safe; for a large session, exceeding the limit is explicitly allowed.
pub fn trim(root: &Path, kind: Kind, budget: u64) -> Usage {
    let (mut usage, mut candidates) = scan(root, kind);
    if usage.server_store || usage.bytes <= budget { return usage }
    candidates.retain(|c| c.eligible);
    candidates.sort_by_key(|c| c.modified);
    for item in candidates {
        if usage.bytes <= budget { break; }
        // Do not follow newly substituted symlinks or delete a file replaced/reused since scan.
        let Ok(md) = fs::symlink_metadata(&item.path) else { continue };
        if !md.file_type().is_file() || md.len() != item.bytes ||
            md.modified().ok() != Some(item.modified) { continue; }
        if temporary(&item.path, kind) &&
            md.modified().ok().and_then(|t| t.elapsed().ok()).is_none_or(|d| d < TRANSFER_GRACE) {
            continue;
        }
        if fs::remove_file(&item.path).is_ok() {
            usage.bytes = usage.bytes.saturating_sub(item.bytes);
            usage.freed = usage.freed.saturating_add(item.bytes);
        }
    }
    usage
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("openomsi-budget-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn age(path: &Path, secs: u64) {
        fs::File::options().write(true).open(path).unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(secs)).unwrap();
    }

    #[test]
    fn local_cache_reclaims_oldest_and_leaves_non_cache_siblings() {
        let base = temp("local");
        let cache = base.join("cache");
        fs::create_dir_all(cache.join("textures")).unwrap();
        let old = cache.join("textures").join("old.tex");
        let fresh = cache.join("new.json");
        fs::write(&old, [1; 12]).unwrap();
        fs::write(&fresh, [2; 12]).unwrap();
        fs::write(base.join("settings.cfg"), "keep").unwrap();
        age(&old, 7200);
        age(&fresh, 3600);
        let result = trim(&cache, Kind::Local, 12);
        assert_eq!(result.bytes, 12);
        assert_eq!(result.freed, 12);
        assert!(!old.exists() && fresh.exists() && base.join("settings.cfg").exists());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn lan_keeps_recent_transfers_and_keys_but_reclaims_old_blobs() {
        let root = temp("lan");
        let old = root.join("old.blob");
        let new = root.join("new.blob");
        let piece = root.join("active.piece");
        for path in [&old, &new, &piece] { fs::write(path, [7; 8]).unwrap(); }
        fs::write(root.join("store.key"), [5; 32]).unwrap();
        age(&old, 7200);
        age(&new, 3600);
        let result = trim(&root, Kind::ClientLan, 16);
        assert_eq!(result.bytes, 16);
        assert!(!old.exists() && new.exists() && piece.exists() && root.join("store.key").exists());
        assert_eq!(trim(&root, Kind::ClientLan, 0).bytes, 8); // recent piece remains
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn server_manifests_disable_client_eviction() {
        let root = temp("server");
        fs::create_dir(root.join("vehicles")).unwrap();
        let blob = root.join("old.blob");
        fs::write(&blob, [1; 20]).unwrap();
        age(&blob, 7200);
        let result = trim(&root, Kind::ClientLan, 0);
        assert!(result.server_store && blob.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn temporary_lan_download_can_exceed_then_returns_to_budget_after_grace() {
        let root = temp("lan-grace");
        let older = root.join("older.blob");
        let incoming = root.join("incoming.zpiece");
        fs::write(&older, [1; 8]).unwrap();
        fs::write(&incoming, [2; 8]).unwrap();
        age(&older, 3600);
        let early = trim(&root, Kind::ClientLan, 0);
        assert_eq!(early.freed, 8);
        assert_eq!(early.bytes, 8);
        assert!(incoming.exists());
        age(&incoming, TRANSFER_GRACE.as_secs() + 5);
        let later = trim(&root, Kind::ClientLan, 0);
        assert_eq!(later.bytes, 0);
        assert_eq!(later.freed, 8);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_partial_cache_write_is_kept_until_safe() {
        let root = temp("local-grace");
        let file = root.join("active.tmp");
        fs::write(&file, [9; 10]).unwrap();
        assert_eq!(trim(&root, Kind::Local, 0).bytes, 10);
        age(&file, TRANSFER_GRACE.as_secs() + 5);
        assert_eq!(trim(&root, Kind::Local, 0).bytes, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_never_followed() {
        use std::os::unix::fs::symlink;
        let root = temp("symlink");
        let outside = temp("outside");
        fs::write(outside.join("protected"), [1; 8]).unwrap();
        symlink(&outside, root.join("outside")).unwrap();
        symlink(outside.join("protected"), root.join("link.tex")).unwrap();
        let result = trim(&root, Kind::Local, 0);
        assert_eq!(result.freed, 0);
        assert!(outside.join("protected").exists());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }
}
