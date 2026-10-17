//! The buses kept here by their content: opened, filled file by file or piece by piece,
//! committed under the fingerprint worked out here, served, waited for and mounted.

use super::*;

// (what the rest of the game asks, of the game's own store)

pub(in crate::mods) fn open() -> bool {
    buses().open()
}

pub(in crate::mods) fn is_open() -> bool {
    buses().is_open()
}

pub(crate) fn manifest(fp: &str) -> Option<VehicleManifest> {
    buses().manifest(fp)
}

pub(in crate::mods) fn missing(m: &VehicleManifest) -> Vec<usize> {
    buses().missing(m)
}

pub(in crate::mods) fn withhold(e: &Entry, why: &str) {
    buses().withhold(e, why)
}

pub(in crate::mods) fn is_withheld(e: &Entry) -> bool {
    buses().is_withheld(e)
}

pub(crate) fn withheld_of(fp: &str) -> Vec<Withheld> {
    buses().withheld_of(fp)
}

pub(in crate::mods) fn put(e: &Entry, data: &[u8]) -> Result<(), String> {
    buses().put(e, data)
}

pub(in crate::mods) fn put_sent(e: &Entry, data: &[u8]) -> Result<(), String> {
    buses().put_sent(e, data)
}

pub(in crate::mods) fn blob(sha: &str) -> Option<Vec<u8>> {
    buses().blob(sha)
}

pub(in crate::mods) fn served(sha: &str) -> Option<Arc<Vec<u8>>> {
    buses().served(sha)
}

pub(in crate::mods) fn missing_pieces(e: &Entry) -> Vec<u64> {
    buses().missing_pieces(e)
}

pub(in crate::mods) fn put_piece(e: &Entry, i: u64, data: &[u8]) -> Result<bool, String> {
    buses().put_piece(e, i, data)
}

pub(in crate::mods) fn put_piece_sent(e: &Entry, i: u64, data: &[u8]) -> Result<bool, String> {
    buses().put_piece_sent(e, i, data)
}

pub(in crate::mods) fn commit(m: VehicleManifest) -> Result<(String, bool), String> {
    buses().commit(m)
}

pub(in crate::mods) fn wait_for(fp: &str, wait: std::time::Duration) -> bool {
    buses().wait_for(fp, wait)
}

pub(in crate::mods) fn list_memory(fp: &str) -> Option<u64> {
    buses().list_memory(fp)
}

pub(crate) fn announce<'a>(fps: impl Iterator<Item = &'a str>) {
    buses().announce(fps)
}

pub(crate) fn mount(fp: &str) -> Option<PathBuf> {
    buses().mount(fp)
}

pub(crate) fn bus_file(fp: &str) -> Option<PathBuf> {
    buses().bus_file(fp)
}

/// What a refused file is held back by: its SHA-256 and the kind of file its name says it is.
/// `quarantine` judges a file by its name, so the same bytes under another name are another
/// case: a `.bus` file sent again as a `.wav` (refused: no sound) must not cost the bus its
/// own `.bus` - a player could otherwise make any bus they know undrawable for everybody.
pub(super) fn held_key(e: &Entry) -> String {
    let name = e.path.rsplit('/').next().unwrap_or("");
    let ext = name.rsplit_once('.').map(|(_, x)| x.to_ascii_lowercase()).unwrap_or_default();
    format!("{}.{ext}", e.sha256.to_ascii_lowercase())
}

/// How long putting a file together waits for room in the transfers' budget
/// (`budget::transfers`); past it the file is left in its pieces and its last piece is asked
/// for again (`missing_pieces`).
pub(super) const FINISH_WAIT: std::time::Duration = std::time::Duration::from_secs(if cfg!(test) { 1 } else { 60 });

/// At most this many bytes of the files served last are kept unsealed (`served`).
pub(super) const SERVED_BYTES: usize = 512 << 20;

pub(super) const ANNOUNCE_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

impl Buses {
    /// The buses kept in the store's folder opened, each with all its files there.
    /// False when there is no store.
    pub(super) fn open(&self) -> bool {
        match crate::mods::store_dir().map(|d| self.open_in(&d)) {
            Some(Ok(())) => true,
            Some(Err(e)) => {
                log::warn!("vehicles: {e}");
                false
            }
            None => false,
        }
    }

    pub(super) fn open_in(&self, dir: &Path) -> Result<(), String> {
        let store = Store::open(dir)?;
        let vdir = dir.join("vehicles");
        std::fs::create_dir_all(&vdir).map_err(|e| format!("{}: {e}", vdir.display()))?;
        let mut found: Vec<(std::time::SystemTime, String, VehicleManifest)> = Vec::new();
        let (mut withheld, mut withheld_of) = (HashMap::new(), HashMap::new());
        for e in std::fs::read_dir(&vdir).map_err(|e| e.to_string())?.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Some(fp) = name.strip_suffix(".json").filter(|f| omsi_net::is_fingerprint(f)) else { continue };
            let Some(m) = std::fs::read(e.path()).ok().and_then(|b| serde_json::from_slice::<VehicleManifest>(&b).ok()).and_then(|m| m.validated().ok()) else {
                log::warn!("vehicles: {name} is no valid list; left aside");
                continue;
            };
            // (a file named by another fingerprint than its list's is not that bus)
            if m.fingerprint() != fp {
                log::warn!("vehicles: {name} lists another bus; left aside");
                continue;
            }
            // (the files it was kept without are none of its files to have)
            let held: Vec<Withheld> = std::fs::read(vdir.join(format!("{fp}.withheld.json"))).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            if let Some(lost) = m.entries.iter().find(|e| store.has(e).is_none() && !held.iter().any(|w| w.path == e.path)) {
                log::warn!("vehicles: {name} lacks {} in the store; left aside", lost.path);
                continue;
            }
            if !held.is_empty() {
                for w in &held {
                    if let Some(e) = m.entries.iter().find(|e| e.path == w.path) {
                        withheld.insert(held_key(e), w.why.clone());
                    }
                }
                withheld_of.insert(fp.to_string(), held);
            }
            let t = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            found.push((t, fp.to_string(), m));
        }
        found.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut sizes = HashMap::new();
        for (_, _, m) in &found {
            for e in &m.entries {
                sizes.insert(e.sha256.clone(), e.size);
            }
        }
        log::info!("vehicles: {} buses players sent are kept", found.len());
        *self.lock() = Some(Vehicles { store, dir: vdir, manifests: found.into_iter().map(|(_, fp, m)| (fp, m)).collect(), sizes, mounted: HashSet::new(), withheld, withheld_of });
        // (what nobody has used for longer than buses are kept goes at once)
        self.collect(0, IN_USE);
        Ok(())
    }

    pub(super) fn is_open(&self) -> bool {
        self.lock().is_some()
    }

    /// The manifest of the bus `fp`, if it is kept here.
    pub(super) fn manifest(&self, fp: &str) -> Option<VehicleManifest> {
        self.lock().as_ref()?.manifests.iter().find(|(f, _)| f == fp).map(|(_, m)| m.clone())
    }

    /// Roughly the memory the list of the kept bus `fp` takes (`VehicleManifest::memory`),
    /// worked out without a copy of it.
    pub(super) fn list_memory(&self, fp: &str) -> Option<u64> {
        self.lock().as_ref()?.manifests.iter().find(|(f, _)| f == fp).map(|(_, m)| m.memory())
    }

    /// Is the bus `fp` kept here?
    fn is_kept(&self, fp: &str) -> bool {
        self.lock().as_ref().is_some_and(|v| v.manifests.iter().any(|(f, _)| f == fp))
    }

    /// Which files of `m` are not kept here (their index in the list) - but for those refused
    /// for what they are (`withhold`): the bus is kept without them.
    pub(super) fn missing(&self, m: &VehicleManifest) -> Vec<usize> {
        let g = self.lock();
        let Some(v) = g.as_ref() else { return (0..m.entries.len()).collect() };
        m.entries.iter().enumerate().filter(|(_, e)| v.store.has(e).is_none() && !v.withheld.contains_key(&held_key(e))).map(|(k, _)| k).collect()
    }

    /// `e` was refused for what it is (`why`, `quarantine`): a bus is kept and drawn without
    /// it - never with another file in its place - and it is not asked for again.
    pub(super) fn withhold(&self, e: &Entry, why: &str) {
        if let Some(v) = self.lock().as_mut() {
            v.withheld.insert(held_key(e), why.to_string());
        }
    }

    /// Was `e` refused for what it is (`withhold`)?
    pub(super) fn is_withheld(&self, e: &Entry) -> bool {
        self.lock().as_ref().is_some_and(|v| v.withheld.contains_key(&held_key(e)))
    }

    /// The files the kept bus `fp` is without (`withhold`).
    pub(super) fn withheld_of(&self, fp: &str) -> Vec<Withheld> {
        self.lock().as_ref().and_then(|v| v.withheld_of.get(fp).cloned()).unwrap_or_default()
    }

    /// Keep a file (checked against `e` already) this game fetched or has itself.
    pub(super) fn put(&self, e: &Entry, data: &[u8]) -> Result<(), String> {
        self.put_making_room(e, data, IN_USE)
    }

    /// Keep a file a player sent (checked against `e` already): room is made of buses nobody
    /// has used for a day at least (`SENT_MAKES_ROOM_OF`), never of the ones played lately.
    pub(super) fn put_sent(&self, e: &Entry, data: &[u8]) -> Result<(), String> {
        self.put_making_room(e, data, SENT_MAKES_ROOM_OF)
    }

    pub(super) fn put_making_room(&self, e: &Entry, data: &[u8], unused_for: std::time::Duration) -> Result<(), String> {
        self.make_room(e.size, unused_for)?;
        // (sealed and written with the store let go, as a piece is: a file of a GB held the
        // store - and the host's frame asking which buses are in use, or mounting one - for
        // the whole write. The write goes to a file of its own and is renamed into place.)
        let store = self.lock().as_ref().ok_or("no store")?.store.clone();
        store.put(e, data).map_err(|x| x.to_string())?;
        if let Some(v) = self.lock().as_mut() {
            v.sizes.insert(e.sha256.to_ascii_lowercase(), e.size);
        }
        Ok(())
    }

    /// The file with that SHA-256, if it is kept here.
    /// (Read and unsealed with the store let go: a file of a GB held every other request up.)
    pub(super) fn blob(&self, sha: &str) -> Option<Vec<u8>> {
        let sha = sha.to_ascii_lowercase();
        let (store, size) = {
            let g = self.lock();
            let v = g.as_ref()?;
            (v.store.clone(), *v.sizes.get(&sha)?)
        };
        store.read(&Entry { path: String::new(), size, sha256: sha }).ok()
    }

    /// The file with that SHA-256, if it is kept here - from the files served last when it is
    /// one of them.
    pub(super) fn served(&self, sha: &str) -> Option<Arc<Vec<u8>>> {
        let sha = sha.to_ascii_lowercase();
        let last = || self.served_last.lock().unwrap_or_else(|e| e.into_inner());
        {
            let mut c = last();
            if let Some(k) = c.iter().position(|(s, _)| *s == sha) {
                let hit = c.remove(k)?;
                let data = hit.1.clone();
                c.push_front(hit);
                return Some(data);
            }
        }
        // (one file opened at a time: asked for a hundred times at once, each a GB, they were
        // all read into memory together)
        let _one = self.opening.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, d)) = last().iter().find(|(s, _)| *s == sha) {
            return Some(d.clone());
        }
        let data = Arc::new(self.blob(&sha)?);
        let mut c = last();
        c.push_front((sha, data.clone()));
        let mut total = 0;
        c.retain(|(_, d)| {
            total += d.len();
            total <= SERVED_BYTES || Arc::ptr_eq(d, &data)
        });
        Some(data)
    }

    /// The pieces of `e` not here (`pieces`): none when the file is kept whole.
    pub(super) fn missing_pieces(&self, e: &Entry) -> Vec<u64> {
        let g = self.lock();
        let Some(v) = g.as_ref() else { return (0..pieces::count(e.size)).collect() };
        if v.store.has(e).is_some() || v.withheld.contains_key(&held_key(e)) {
            return Vec::new();
        }
        let count = pieces::count(e.size);
        let missing: Vec<u64> = (0..count).filter(|i| !v.store.has_piece(&e.sha256, *i, pieces::span(e.size, *i).1)).collect();
        // (every piece here, the file never put together - no room for it in the budget then:
        // its last piece is asked for again, and the file is put together when it comes)
        if missing.is_empty() && count > 0 {
            return vec![count - 1];
        }
        missing
    }

    /// Keep piece `i` of `e` as it came; once every piece is here the file is checked and
    /// kept whole (`finish`). True when it is.
    pub(super) fn put_piece(&self, e: &Entry, i: u64, data: &[u8]) -> Result<bool, String> {
        self.put_piece_making_room(e, i, data, IN_USE)
    }

    /// `put_piece` for a piece a player sent: room only of buses unused for a day (`put_sent`).
    pub(super) fn put_piece_sent(&self, e: &Entry, i: u64, data: &[u8]) -> Result<bool, String> {
        self.put_piece_making_room(e, i, data, SENT_MAKES_ROOM_OF)
    }

    pub(super) fn put_piece_making_room(&self, e: &Entry, i: u64, data: &[u8], unused_for: std::time::Duration) -> Result<bool, String> {
        if i >= pieces::count(e.size) || data.len() as u64 != pieces::span(e.size, i).1 {
            return Err(format!("{}: not a piece of it", e.path));
        }
        // (a piece takes room as a file does: pieces of files never finished filled the disk)
        self.make_room(data.len() as u64, unused_for)?;
        let store = self.lock().as_ref().ok_or("no store")?.store.clone();
        store.put_piece(&e.sha256, i, data).map_err(|x| format!("{}: {x}", e.path))?;
        self.finish(e, unused_for)
    }

    /// `e` put together from its pieces, if they are all here: checked (its SHA-256, no
    /// program, what it says it is), kept whole, its pieces gone. True when it is kept.
    pub(super) fn finish(&self, e: &Entry, unused_for: std::time::Duration) -> Result<bool, String> {
        let _one = self.finishing.lock().unwrap_or_else(|x| x.into_inner());
        let count = pieces::count(e.size);
        // (read with the store let go: a file of a GB held every other request up)
        let store = self.lock().as_ref().ok_or("no store")?.store.clone();
        let data = {
            if store.has(e).is_some() {
                return Ok(true);
            }
            if (0..count).any(|i| !store.has_piece(&e.sha256, i, pieces::span(e.size, i).1)) {
                return Ok(false);
            }
            // (the whole file in memory while it is checked and kept: held against the
            // transfers' budget, as a file sent whole is; with no room before `FINISH_WAIT` the
            // pieces stay and the sender is told to come back)
            let Some(held) = crate::mods::budget::transfers().hold_waiting(e.size, FINISH_WAIT) else {
                return Err(crate::mods::failure::says::BUSY.to_string());
            };
            let mut data = Vec::with_capacity(e.size as usize);
            for i in 0..count {
                data.extend(store.piece(&e.sha256, i, pieces::span(e.size, i).1).map_err(|x| format!("{}: {x}", e.path))?);
            }
            (data, held)
        };
        let (data, _held) = data;
        let drop_pieces = || {
            if let Some(v) = self.lock().as_ref() {
                v.store.remove_pieces(&e.sha256, count);
            }
        };
        if crate::mods::sha256_of(&data) != e.sha256 {
            drop_pieces();
            return Err(format!("{}: not the file the list names", e.path));
        }
        // (refused for what it is: the bus goes on without it - `withhold`)
        let refused = crate::mods::quarantine::screen(&e.path, &data).err();
        if let Some(why) = refused {
            drop_pieces();
            self.withhold(e, &why);
            return Err(why);
        }
        self.put_making_room(e, &data, unused_for)?;
        drop_pieces();
        Ok(true)
    }

    /// A bus all of whose files are here becomes one of the kept ones: its fingerprint,
    /// worked out here from what was checked here (never the one its sender says).
    pub(super) fn commit(&self, m: VehicleManifest) -> Result<(String, bool), String> {
        let m = m.validated()?;
        let fp = m.fingerprint();
        let mut g = self.lock();
        let v = g.as_mut().ok_or("no store")?;
        if v.manifests.iter().any(|(f, _)| *f == fp) {
            return Ok((fp, false));
        }
        // (a file refused for what it is is left out - never the bus file itself: without it
        // there is no bus)
        let bus_key = identity::key(&m.bus);
        let mut held = Vec::new();
        for e in m.entries.iter().filter(|e| v.store.has(e).is_none()) {
            match v.withheld.get(&held_key(e)) {
                Some(why) if identity::key(&e.path) != bus_key => held.push(Withheld { path: e.path.clone(), why: why.clone() }),
                _ => return Err(format!("{} has not come", e.path)),
            }
        }
        if !held.is_empty() {
            let file = v.dir.join(format!("{fp}.withheld.json"));
            std::fs::write(&file, serde_json::to_vec(&held).unwrap_or_default()).map_err(|e| format!("{}: {e}", file.display()))?;
            log::warn!("vehicles: {} is kept without {} of its files ({})", m.bus, held.len(), held.iter().take(4).map(|w| w.why.as_str()).collect::<Vec<_>>().join("; "));
            v.withheld_of.insert(fp.clone(), held);
        }
        // (what is served by its SHA-256 is known by the kept lists after a restart: a file
        // kept before one - for a bus never finished then - was in the store but not served,
        // and the bus kept now that has it was asked for in vain: "no such file")
        for e in &m.entries {
            v.sizes.insert(e.sha256.to_ascii_lowercase(), e.size);
        }
        let file = v.dir.join(format!("{fp}.json"));
        let tmp = file.with_extension("part");
        std::fs::write(&tmp, serde_json::to_vec(&m).unwrap_or_default()).and_then(|_| std::fs::rename(&tmp, &file)).map_err(|e| format!("{}: {e}", file.display()))?;
        v.manifests.push((fp.clone(), m));
        drop(g);
        // (whoever waits for a bus hears of it at once: `wait_for`)
        let (count, arrived) = &self.arrived;
        *count.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        arrived.notify_all();
        Ok((fp, true))
    }

    /// Is `fp` kept here, waiting for it up to `wait` when it is not yet (no copy of its list
    /// taken meanwhile: `manifest` takes one once the answer is held against the budget)? A bus
    /// asked for by another player is most likely on its way here from its own (it sends it
    /// under its loading screen): the answer goes the moment it is kept, not when the asker
    /// asks again.
    ///
    /// Only a bus on its way is waited for (`on_its_way`): one a player of the session
    /// announces, or one being sent. A player who announced a new made-up fingerprint every
    /// second had every other game wait 90 s for each, a connection held here for each.
    pub(super) fn wait_for(&self, fp: &str, wait: std::time::Duration) -> bool {
        let t0 = std::time::Instant::now();
        let until = t0 + wait;
        let (count, arrived) = &self.arrived;
        let mut seen = count.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if self.is_kept(fp) {
                return true;
            }
            // (a moment's grace: the server says its players' buses every second, and a game
            // may ask for one it heard of a moment before that)
            if !self.on_its_way(fp) && t0.elapsed() >= ANNOUNCE_GRACE {
                return false;
            }
            let Some(left) = until.checked_duration_since(std::time::Instant::now()) else {
                return false;
            };
            seen = arrived.wait_timeout(seen, left.min(std::time::Duration::from_secs(1))).unwrap_or_else(|e| e.into_inner()).0;
        }
    }

    /// A server's game, every second: the buses its players announce.
    pub(super) fn announce<'a>(&self, fps: impl Iterator<Item = &'a str>) {
        *self.announced.lock().unwrap_or_else(|e| e.into_inner()) = Some(fps.map(str::to_string).collect());
    }

    /// Is the bus `fp` on its way here: announced by a player of the session, or being sent
    /// (`lease`)? (Before any player was announced - no game of a session - every one is.)
    pub(super) fn on_its_way(&self, fp: &str) -> bool {
        self.leased(fp) || self.announced.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_none_or(|a| a.contains(fp))
    }

    /// Where the bus `fp` can be read: its files under `lan-vehicles/<fp>`, mounted once.
    pub(super) fn mount(&self, fp: &str) -> Option<PathBuf> {
        if !omsi_net::is_fingerprint(fp) {
            return None;
        }
        let base = mounts()?;
        let at = base.join(fp);
        let mut g = self.lock();
        let v = g.as_mut()?;
        if v.mounted.contains(fp) {
            return Some(at);
        }
        let m = &v.manifests.iter().find(|(f, _)| f == fp)?.1;
        let files: Vec<_> = m.entries.iter().filter_map(|e| v.store.has(e)).collect();
        // (every file of it, but those it is kept without)
        let held = v.withheld_of.get(fp).map(Vec::len).unwrap_or(0);
        if files.len() + held != m.entries.len() {
            return None;
        }
        omsi_cfg::vfs::mount_sealed(&at, v.store.key(), files);
        v.mounted.insert(fp.to_string());
        drop(g);
        // (what the folder listings knew of that place is old now - of that place only: the
        // lookups elsewhere stay, and every fetched bus no longer emptied them all)
        omsi_cfg::content_added_at(&at);
        Some(at)
    }

    /// The bus file of `fp` where it is mounted.
    pub(super) fn bus_file(&self, fp: &str) -> Option<PathBuf> {
        let m = self.manifest(fp)?;
        let at = self.mount(fp)?;
        let rel = m.entries.iter().find(|e| identity::key(&e.path) == identity::key(&m.bus))?.path.clone();
        Some(rel.split('/').fold(at, |p, c| p.join(c)))
    }
}
