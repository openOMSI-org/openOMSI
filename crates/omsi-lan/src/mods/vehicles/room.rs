//! Keeping the store within the disk: buses nobody uses go (no size is ever refused).

use super::*;

/// Room left free on the disk after a file is kept.
pub(super) const KEEP_FREE: u64 = 3 << 30;
/// A bus a connected player drives counts as in use for this long after it was seen.
pub(super) const IN_USE: std::time::Duration = std::time::Duration::from_secs(10 * 60);
/// What a player sends makes room only of buses nobody has used for this long: a player
/// sending big buses of no use pushed the buses played that day out, one after the other.
pub(super) const SENT_MAKES_ROOM_OF: std::time::Duration = std::time::Duration::from_secs(86_400);
/// Pieces of a file left unfinished this long go (its player gone): they took room nothing
/// counted, kept as long as buses were.
pub(super) const PIECES_KEEP: std::time::Duration = std::time::Duration::from_secs(6 * 3600);
/// A loose piece younger than this is spared when room is made (an upload under way is still
/// writing its file's pieces); older ones go before any bus does.
pub(super) const PIECES_GRACE: std::time::Duration = std::time::Duration::from_secs(10 * 60);
/// How long a bus nobody has used is kept, unless told otherwise (`keep_for_days`).
pub(super) const KEEP: std::time::Duration = std::time::Duration::from_secs(30 * 86_400);

/// How long a bus nobody has used is kept (`uploads_keep_days` of `server.cfg`).
pub fn keep_for_days(days: u64) {
    buses().keep_for_days(days)
}

pub(in crate::mods) fn lease(m: &VehicleManifest) -> Lease<'static> {
    buses().lease(m)
}

pub(crate) fn in_use<'a>(fps: impl Iterator<Item = &'a str>) {
    buses().in_use(fps)
}

/// A bus being sent - its fingerprint and its files - held from its offer to the end of its
/// connection: neither its list nor its files go meanwhile (another bus that shares a file
/// with it may go; the file stays).
pub(in crate::mods) struct Lease<'a>(&'a Buses, Vec<String>);

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        let mut map = self.0.leases();
        for s in &self.1 {
            if let Some(n) = map.get_mut(s) {
                *n -= 1;
                if *n == 0 {
                    map.remove(s);
                }
            }
        }
    }
}

/// When the bus `fp` was last used: its list's time (touched by `in_use`).
pub(super) fn last_used(v: &Vehicles, fp: &str) -> std::time::SystemTime {
    std::fs::metadata(v.dir.join(format!("{fp}.json"))).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH)
}

/// What a collection did.
#[derive(Debug, Default, PartialEq)]
pub(in crate::mods) struct Collected {
    pub buses: usize,
    pub files: usize,
    pub bytes: u64,
}

impl Buses {
    pub(super) fn keep_for_days(&self, days: u64) {
        *self.keep.lock().unwrap_or_else(|e| e.into_inner()) = std::time::Duration::from_secs(days.max(1) * 86_400);
    }

    fn leases(&self) -> std::sync::MutexGuard<'_, HashMap<String, u32>> {
        self.leases.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `m` held against a collection while it is being sent (`Lease`).
    pub(super) fn lease(&self, m: &VehicleManifest) -> Lease<'_> {
        let shas: Vec<String> = m.entries.iter().map(|e| e.sha256.to_ascii_lowercase()).chain(std::iter::once(m.fingerprint())).collect();
        // (taken with the store held, in the order a collection takes them - store, then
        // leases: a collection under way finishes first, and one that comes after sees the
        // lease. Taken beside it, a collection could pass a file by as unleased and drop it
        // while the upload was being told the server has it.)
        let _store = self.lock();
        let mut map = self.leases();
        for s in &shas {
            *map.entry(s.clone()).or_insert(0) += 1;
        }
        Lease(self, shas)
    }

    pub(super) fn leased(&self, sha: &str) -> bool {
        self.leases().contains_key(sha)
    }

    /// The buses the connected players drive (their fingerprints): in use now. Asked from
    /// the host's frame (once a minute), it never waits for the store: while an upload or a
    /// collection holds it, it looks again next time. (Their lists are touched with the store
    /// let go: a list a collection took meanwhile is simply not there.)
    pub(super) fn in_use<'a>(&self, fps: impl Iterator<Item = &'a str>) {
        let files: Vec<std::path::PathBuf> = {
            let g = match self.kept.try_lock() {
                Ok(g) => g,
                Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => return,
            };
            let Some(v) = g.as_ref() else { return };
            fps.filter(|fp| v.manifests.iter().any(|(f, _)| f == fp)).map(|fp| v.dir.join(format!("{fp}.json"))).collect()
        };
        for file in files {
            if let Ok(f) = std::fs::OpenOptions::new().write(true).open(&file) {
                let _ = f.set_modified(std::time::SystemTime::now());
            }
        }
    }

    /// Let go of the buses nobody has used for longer than they are kept - and, when `need`
    /// bytes must be freed, of more, the least lately used first, of those unused for
    /// `unused_for` at least (never one in use now) - never a file an upload under way holds.
    /// A bus's list goes before its files (a collection broken off leaves no list without its
    /// files); a file goes only when no bus kept uses it.
    pub(super) fn collect(&self, need: u64, unused_for: std::time::Duration) -> Collected {
        let keep = *self.keep.lock().unwrap_or_else(|e| e.into_inner());
        let now = std::time::SystemTime::now();
        let mut g = self.lock();
        let Some(v) = g.as_mut() else { return Collected::default() };
        // (pieces of a file left unfinished that long - its player gone - go too)
        let stale = v.store.remove_stale_pieces(keep.min(PIECES_KEEP));
        if stale > 0 {
            log::info!("vehicles: {stale} pieces of files never finished removed");
        }
        let mut by_age: Vec<(std::time::SystemTime, String)> = v.manifests.iter().map(|(f, _)| (last_used(v, f), f.clone())).collect();
        by_age.sort();
        let age = |t: std::time::SystemTime| now.duration_since(t).unwrap_or_default();
        let mut gone: Vec<VehicleManifest> = Vec::new();
        let mut freed = 0u64;
        for (t, fp) in by_age {
            let expired = age(t) > keep;
            let wanted_room = need > 0 && freed < need && age(t) > unused_for.max(IN_USE);
            if !(expired || wanted_room) {
                continue;
            }
            let Some(k) = v.manifests.iter().position(|(f, _)| *f == fp) else { continue };
            if self.leased(&fp) {
                continue;
            }
            let (_, m) = v.manifests.remove(k);
            let _ = std::fs::remove_file(v.dir.join(format!("{fp}.json")));
            if v.withheld_of.remove(&fp).is_some() {
                let _ = std::fs::remove_file(v.dir.join(format!("{fp}.withheld.json")));
            }
            if v.mounted.remove(&fp) {
                if let Some(base) = mounts() {
                    omsi_cfg::vfs::unmount_sealed(&base.join(&fp));
                }
            }
            freed += m.size();
            gone.push(m);
        }
        let kept: HashSet<String> = v.manifests.iter().flat_map(|(_, m)| m.entries.iter().map(|e| e.sha256.to_ascii_lowercase())).collect();
        let mut out = Collected { buses: gone.len(), ..Default::default() };
        let mut done: HashSet<String> = HashSet::new();
        for e in gone.iter().flat_map(|m| m.entries.iter()) {
            let sha = e.sha256.to_ascii_lowercase();
            if kept.contains(&sha) || self.leased(&sha) || !done.insert(sha.clone()) {
                continue;
            }
            if let Some(f) = v.store.has(e) {
                if std::fs::remove_file(&f.blob).is_ok() {
                    v.store.remove_packed(&sha, pieces::count(e.size));
                    v.sizes.remove(&sha);
                    out.files += 1;
                    out.bytes += e.size;
                }
            }
        }
        if out.buses > 0 {
            log::info!("vehicles: {} buses nobody uses let go - {} files, {:.1} MB", out.buses, out.files, out.bytes as f64 / 1e6);
        }
        out
    }

    /// Room on the disk for a file of `size`: the buses nobody has used for `unused_for` go
    /// until there is; an error only when even then the disk cannot hold it.
    pub(super) fn make_room(&self, size: u64, unused_for: std::time::Duration) -> Result<(), String> {
        let Some(dir) = self.lock().as_ref().map(|v| v.store.dir().to_path_buf()) else { return Ok(()) };
        let free = || crate::mods::store::free_space(&dir);
        let Some(have) = free() else { return Ok(()) };
        if have >= size + KEEP_FREE {
            return Ok(());
        }
        // loose pieces first - kept before their file could be checked, so the cheapest to
        // lose (a transfer that loses one sends it again): without this a player filled the
        // disk with pieces of files never finished, which only went after `PIECES_KEEP`
        let store = self.lock().as_ref().map(|v| v.store.clone());
        if let Some(store) = store {
            let freed = store.remove_pieces_for_room(size + KEEP_FREE - have, PIECES_GRACE);
            if freed > 0 {
                log::info!("vehicles: {:.1} MB of loose pieces let go to make room", freed as f64 / 1e6);
            }
        }
        if let Some(have) = free().filter(|h| *h < size + KEEP_FREE) {
            self.collect(size + KEEP_FREE - have, unused_for);
        }
        match free() {
            Some(now) if now < size + KEEP_FREE => Err(format!("the server's disk is full ({:.1} GB free)", now as f64 / 1e9)),
            _ => Ok(()),
        }
    }
}
