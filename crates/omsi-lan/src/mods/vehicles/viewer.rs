//! A player's game: another player's bus, by its fingerprint - its own copy when it is the
//! same, else fetched from the host - and what stands for it meanwhile.

use super::*;
use crate::mods::failure::{self, Failure};

/// A font library shared by the vehicles that write with it (the world's, or one fetched
/// bus's own).
pub type SharedFonts = Arc<parking_lot::Mutex<omsi_sim::texttex::FontLibrary>>;

/// The fonts of each fetched bus, by its mount (the same mount is the same files).
static MOUNT_FONTS: std::sync::LazyLock<Mutex<HashMap<PathBuf, SharedFonts>>> = std::sync::LazyLock::new(Default::default);

/// The fonts the bus fetched as `fp` writes its displays with: those of its own mount alone
/// (`FontLibrary::only`), never this machine's fonts of the same names. One library a
/// mount, shared by every vehicle made of it and built once (a flyweight): a fresh one for
/// every remake read its fonts and built their atlases again on the frame. Asked on the
/// frame for a bus `Wanted::Mounted` says is mounted: its place is known without the store,
/// whose lock an upload or a collection may hold (None when `fp` is no fingerprint).
pub(crate) fn fonts_of(fp: &str) -> Option<SharedFonts> {
    if !omsi_net::is_fingerprint(fp) {
        return None;
    }
    let at = mounts()?.join(fp);
    let mut fonts = MOUNT_FONTS.lock().unwrap_or_else(|e| e.into_inner());
    Some(fonts.entry(at.clone()).or_insert_with(|| Arc::new(parking_lot::Mutex::new(omsi_sim::texttex::FontLibrary::only(&at)))).clone())
}

/// Where another player's bus stands on this machine.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Wanted {
    /// Being made sure of: (bytes here, bytes in all) once a download is under way.
    Pending(u64, u64),
    /// This machine's own copy is that bus, file for file: loaded from its own place.
    Local(PathBuf),
    /// Fetched by its content: loaded from its mount.
    Mounted(PathBuf),
    /// Neither here nor to be had (why); tried again after a while.
    Failed(Failure),
}

/// What this game knows of the other players' buses: where each stands, who drives which,
/// and the downloads under way. The game has one (`viewer`, behind this module's functions);
/// a test makes its own.
#[derive(Default)]
pub(crate) struct Viewer {
    /// By fingerprint: where it stands, since when, and how many times running it could not
    /// be had.
    wanted: Mutex<HashMap<String, (Wanted, std::time::Instant, u32)>>,
    /// The bus each other player drives (by player): its fingerprint, and when it changed lately.
    drivers: Mutex<Drivers>,
    /// Downloads under way (at most `DOWNLOADS`), and the ones waiting for a place.
    slots: (Mutex<usize>, std::sync::Condvar),
    /// The buses whose reads from outside their own files were logged already (`audit`).
    told: Mutex<HashSet<String>>,
    /// By fingerprint: when it was first asked for (`hopeless`).
    asked: Mutex<HashMap<String, std::time::Instant>>,
    /// Bytes of other players' buses fetched this session (`MAX_SESSION_DOWNLOAD`).
    fetched: std::sync::atomic::AtomicU64,
}

/// A bus whose files to fetch take more than this is not fetched: another stands in for it
/// (the heaviest buses players drive are ~800 MB). Any announced bus was downloaded, however
/// large; the host's map has had a ceiling (`MAX_TOTAL`), its buses had none.
pub(super) const MAX_BUS_DOWNLOAD: u64 = 2 << 30;
/// Other players' buses fetched in one session, at most (as much as a host's mods may take).
pub(super) const MAX_SESSION_DOWNLOAD: u64 = crate::mods::MAX_TOTAL;

pub(super) type Drivers = HashMap<u32, (String, VecDeque<std::time::Instant>)>;

static VIEWER: std::sync::LazyLock<Viewer> = std::sync::LazyLock::new(Viewer::default);

/// The game's own `Viewer`.
pub(crate) fn viewer() -> &'static Viewer {
    &VIEWER
}

/// A bus that could not be had this many times running (for another reason than not being
/// on the server yet) is not asked for again this session: a bad list or file is not fetched
/// over and over.
pub(super) const GIVE_UP: u32 = 3;
pub(super) const RETRY: std::time::Duration = std::time::Duration::from_secs(60);

/// A bus on its way that has not come this long after it was first asked for is not waited
/// for any more: another stands in for it (`hopeless`), and it is still asked for.
pub(super) const FALL_BACK_AFTER: std::time::Duration = std::time::Duration::from_secs(180);

/// Downloads of buses under way at once, at most.
pub(super) const DOWNLOADS: usize = 3;

/// A player changes bus or paint this many times within `CHANGES_WITHIN` at most.
pub(super) const CHANGES: usize = 6;
pub(super) const CHANGES_WITHIN: std::time::Duration = std::time::Duration::from_secs(300);

/// Is a bus that could not be had (`why`, `since` how long, `fails` times running) to be
/// asked for again now?
pub(super) fn may_retry(why: &Failure, since: std::time::Duration, fails: u32) -> bool {
    // (a bus on its way, or a host not reached - never given up: what can come, comes)
    (why.coming() || fails < GIVE_UP) && since >= why.retry_after(RETRY)
}

// (what the rest of the game asks, of the game's own viewer)

pub(crate) fn ready(fp: &str) -> bool {
    viewer().ready(fp)
}

pub(crate) fn retry_due(fp: &str) -> bool {
    viewer().retry_due(fp)
}

pub(crate) fn status(fp: &str) -> Option<Wanted> {
    viewer().status(fp)
}

pub(crate) fn hopeless(fp: &str) -> bool {
    viewer().hopeless(fp)
}

pub(crate) fn audit(fp: &str, root: &Path, reads: &[PathBuf]) -> Vec<PathBuf> {
    viewer().audit(fp, root, reads)
}

pub(crate) fn want(root: &Path, player: u32, fp: &str, bus: &str, paint: &str) -> Wanted {
    viewer().want(root, player, fp, bus, paint)
}

impl Viewer {
    fn wanted(&self) -> std::sync::MutexGuard<'_, HashMap<String, (Wanted, std::time::Instant, u32)>> {
        self.wanted.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Is the bus `fp` here to be drawn (this machine's own copy, or fetched and mounted)? A
    /// player's box is made again then: the mount made it again a moment before the fetch
    /// said so, as a box still, and nothing else did - the box stayed with the bus right here.
    pub(crate) fn ready(&self, fp: &str) -> bool {
        matches!(self.wanted().get(fp), Some((Wanted::Local(_) | Wanted::Mounted(_), _, _)))
    }

    /// Is the bus `fp`, which could not be had, to be asked for again now? (A player's box is
    /// made again then: else only another bus coming asked for it once more.)
    pub(crate) fn retry_due(&self, fp: &str) -> bool {
        matches!(self.wanted().get(fp), Some((Wanted::Failed(why), at, fails)) if may_retry(why, at.elapsed(), *fails))
    }

    /// Where the bus `fp` stands, as far as it has been asked for (nothing is started).
    pub(crate) fn status(&self, fp: &str) -> Option<Wanted> {
        self.wanted().get(fp).map(|(w, _, _)| w.clone())
    }

    /// Is the bus `fp` not to be had as itself, for now: given up (`GIVE_UP` failures that
    /// are no bus on its way), or not come `FALL_BACK_AFTER` after it was first asked for? A
    /// player's bus is drawn with another then (`lan::remote_type`), and still asked for in
    /// the background. A download under way is never hopeless, however long it takes.
    pub(crate) fn hopeless(&self, fp: &str) -> bool {
        let since = self.asked.lock().unwrap_or_else(|e| e.into_inner()).get(fp).map(|t| t.elapsed());
        match self.wanted().get(fp) {
            Some((Wanted::Failed(why), _, fails)) => (!why.coming() && *fails >= GIVE_UP) || since.is_some_and(|s| s >= FALL_BACK_AFTER),
            _ => false,
        }
    }

    /// `fp` was asked for at `at` (a test's way to have asked long ago).
    pub(super) fn asked_at(&self, fp: &str, at: std::time::Instant) {
        let mut asked = self.asked.lock().unwrap_or_else(|e| e.into_inner());
        // (buses asked for by players who made fingerprints up: the oldest go)
        if asked.len() > 1024 {
            asked.retain(|_, t| t.elapsed() < std::time::Duration::from_secs(3600));
        }
        asked.entry(fp.to_string()).or_insert(at);
    }

    pub(super) fn set(&self, fp: &str, w: Wanted) {
        let mut map = self.wanted();
        let before = map.get(fp).map(|e| e.2).unwrap_or(0);
        let fails = match &w {
            Wanted::Failed(_) => before + 1,
            Wanted::Pending(..) => before,
            _ => 0,
        };
        map.insert(fp.to_string(), (w, std::time::Instant::now(), fails));
        // (buses that could not be had, long ago: a player who made fingerprints up left one
        // behind for each)
        if map.len() > 1024 {
            map.retain(|_, (w, at, _)| !matches!(w, Wanted::Failed(_)) || at.elapsed() < std::time::Duration::from_secs(600));
        }
        // (a stand-in shown for it is made again once it is ready: `lan::tick` asks `ready`.
        // The whole content was said to have changed here, which emptied every lookup cache
        // and remade every box of the session for each bus that came.)
    }

    /// `player` drives the bus `fp`: false when the bus has changed too often of late.
    pub(super) fn driven_by(&self, player: u32, fp: &str) -> bool {
        let mut drivers = self.drivers.lock().unwrap_or_else(|e| e.into_inner());
        let (now, (was, changes)) = (std::time::Instant::now(), drivers.entry(player).or_insert_with(|| (fp.to_string(), VecDeque::new())));
        while changes.front().is_some_and(|t| now.duration_since(*t) > CHANGES_WITHIN) {
            changes.pop_front();
        }
        if was.as_str() != fp {
            *was = fp.to_string();
            changes.push_back(now);
        }
        changes.len() <= CHANGES
    }

    /// Does some player here drive the bus `fp`? (Each one asked for by `want`; before any
    /// was, every bus is.)
    pub(super) fn driven(&self, fp: &str) -> bool {
        let drivers = self.drivers.lock().unwrap_or_else(|e| e.into_inner());
        drivers.is_empty() || drivers.values().any(|(f, _)| f == fp)
    }

    /// One of the `DOWNLOADS` places, waited for when they are all taken.
    fn take_slot(&self) -> Slot<'_> {
        let (n, freed) = &self.slots;
        let mut busy = n.lock().unwrap_or_else(|e| e.into_inner());
        while *busy >= DOWNLOADS {
            busy = freed.wait(busy).unwrap_or_else(|e| e.into_inner());
        }
        *busy += 1;
        Slot(self)
    }

    /// What loading a fetched bus read from outside its own files (`reads`, see
    /// `omsi_cfg::vfs::Audit`): the game's own variable lists aside, every such file is one
    /// this player may have in another version than its driver - a way in for a bus that is
    /// not quite the same. Logged, once for each bus.
    pub(crate) fn audit(&self, fp: &str, root: &Path, reads: &[PathBuf]) -> Vec<PathBuf> {
        if !matches!(self.status(fp), Some(Wanted::Mounted(_))) {
            return Vec::new();
        }
        let Some(at) = mounts().map(|m| m.join(fp)) else { return Vec::new() };
        let out = escapes(&at, root, reads);
        if !out.is_empty() && self.told.lock().unwrap_or_else(|e| e.into_inner()).insert(fp.to_string()) {
            let some: Vec<String> = out.iter().take(10).map(|p| p.display().to_string()).collect();
            log::warn!("vehicles: {fp} read {} files from outside its own while it loaded (they may differ from its driver's): {}", out.len(), some.join(", "));
        }
        out
    }

    /// The bus with fingerprint `fp` that a player announces as `bus` in `paint`: where it is
    /// to be loaded from, once that is known for sure - this machine's own copy when it is the
    /// same bus file for file, else the bus fetched from the host by its content. Until then
    /// (and when it cannot be had) the state says how far it is; the work goes on in the
    /// background.
    ///
    /// `player` is who drives it: a player whose bus changes more often than a player changes
    /// bus (`driven_by`) is drawn as a box - each made-up fingerprint started a download here.
    pub(crate) fn want(&'static self, root: &Path, player: u32, fp: &str, bus: &str, paint: &str) -> Wanted {
        if !omsi_net::is_fingerprint(fp) {
            return Wanted::Failed("no fingerprint".into());
        }
        if !self.driven_by(player, fp) {
            return Wanted::Failed("its bus changes too often".into());
        }
        self.asked_at(fp, std::time::Instant::now());
        {
            let mut map = self.wanted();
            match map.get(fp) {
                Some((Wanted::Failed(why), at, fails)) if !may_retry(why, at.elapsed(), *fails) => return map[fp].0.clone(),
                Some((Wanted::Failed(_), _, fails)) => {
                    let fails = *fails;
                    map.insert(fp.to_string(), (Wanted::Pending(0, 0), std::time::Instant::now(), fails));
                }
                None => {
                    map.insert(fp.to_string(), (Wanted::Pending(0, 0), std::time::Instant::now(), 0));
                }
                Some((w, _, _)) => return w.clone(),
            }
        }
        let (root, key, bus, paint) = (root.to_path_buf(), fp.to_string(), bus.to_string(), paint.to_string());
        let fp = key.clone();
        let spawned = std::thread::Builder::new().name("bus-by-content".into()).spawn(move || {
            let w = self.make_sure(&root, &fp, &bus, &paint);
            match &w {
                Wanted::Local(_) => log::info!("vehicles: {bus} ({paint}) is the same here ({fp})"),
                Wanted::Mounted(_) => log::info!("vehicles: {bus} ({paint}) fetched by its content ({fp})"),
                Wanted::Failed(e) => log::warn!("vehicles: {bus} ({paint}) as {fp}: {e}"),
                Wanted::Pending(..) => {}
            }
            self.set(&fp, w);
        });
        if spawned.is_err() {
            self.set(&key, Wanted::Failed("no thread".into()));
        }
        Wanted::Pending(0, 0)
    }

    pub(super) fn make_sure(&self, root: &Path, fp: &str, bus: &str, paint: &str) -> Wanted {
        // kept here already (fetched before, or sent by this machine's own player)
        if !is_open() && !open() {
            return Wanted::Failed("no store".into());
        }
        if let Some(p) = bus_file(fp) {
            return Wanted::Mounted(p);
        }
        // this machine's own copy, if it is that bus file for file
        match local_copy(root, fp, bus, paint) {
            Ok(path) => return Wanted::Local(path),
            Err(why) => log::info!("vehicles: {bus} ({paint}): {why}; asking the host for it by its content"),
        }
        // the host itself (a server): nobody to ask - its player is most likely still sending
        // it here; waited for as a joining game's question is (`wait_for`)
        if crate::mods::upload::host().is_none() {
            // (a joining game whose way to the host is still being found asks once it is: the
            // store is open on every game, only the server waits here)
            if !crate::mods::upload::serving() {
                return Wanted::Failed(Failure::NoHost);
            }
            return match wait_for(fp, crate::mods::upload::MANIFEST_WAIT).then(|| bus_file(fp)).flatten() {
                Some(p) => Wanted::Mounted(p),
                None => Wanted::Failed(Failure::NotHereYet),
            };
        }
        match self.fetch(fp) {
            Ok(()) => bus_file(fp).map(Wanted::Mounted).unwrap_or_else(|| Wanted::Failed("fetched, but not to be mounted".into())),
            Err(e) => Wanted::Failed(e),
        }
    }

    /// The bus `fp` from the host: its list (checked to be that bus), and each of its files
    /// this machine does not keep - taken from its own copy where that is the same file, else
    /// fetched.
    pub(super) fn fetch(&self, fp: &str) -> Result<(), Failure> {
        let (host, session) = crate::mods::upload::host().ok_or(Failure::NoHost)?;
        // (a few buses at a time: the nearest players first come first, the line is not choked)
        let _slot = self.take_slot();
        // (the player may have gone, or changed bus, while this waited for its turn)
        if !self.driven(fp) {
            return Err(Failure::NotDriven);
        }
        // (the host waits for a bus on its way before it answers: the answer was given up at
        // 25 s while the host still waited for it)
        let (raw, mut out, mut input) = failure::connect(host, session, crate::mods::upload::MANIFEST_WAIT + std::time::Duration::from_secs(30))?;
        writeln!(out, "MANIFEST {fp}").and_then(|_| out.flush()).map_err(|e| e.to_string())?;
        let len = failure::reply(&mut input)?.ok_or("no list")?;
        if len > 256 << 20 {
            return Err("the list is too large".into());
        }
        let mut body = vec![0u8; len as usize];
        input.read_exact(&mut body).map_err(|e| e.to_string())?;
        let m: VehicleManifest = serde_json::from_slice(&body).map_err(|e| format!("bad list: {e}"))?;
        let m = m.validated()?;
        // (the list must be the bus asked for: the host could not pass off another one)
        if m.fingerprint() != fp {
            return Err("the host's list is another bus".into());
        }
        if let Some(bad) = m.entries.iter().find(|e| crate::mods::refuse_path(&e.path).is_some() || e.size > crate::mods::MAX_FILE) {
            return Err(format!("{}: refused", bad.path).into());
        }
        // the files the host keeps it without (refused there for what they are): not asked
        // for, and never taken from anywhere else - the bus is drawn without them
        let held = serde_json::from_slice::<HeldBack>(&body).unwrap_or_default().withheld;
        for w in &held {
            if let Some(e) = m.entries.iter().find(|e| e.path == w.path).filter(|e| identity::key(&e.path) != identity::key(&m.bus)) {
                withhold(e, &format!("{} (on the host)", w.why));
            }
        }
        let missing = missing(&m);
        let total: u64 = missing.iter().map(|k| m.entries[*k].size).sum();
        // (a ceiling for a bus, and for all of a session's: another stands in past them)
        use std::sync::atomic::Ordering;
        if total > MAX_BUS_DOWNLOAD {
            return Err(format!("{} would take {} to fetch, more than a bus may ({})", m.bus, crate::mods::upload::mb(total), crate::mods::upload::mb(MAX_BUS_DOWNLOAD)).into());
        }
        if self.fetched.fetch_add(total, Ordering::AcqRel).saturating_add(total) > MAX_SESSION_DOWNLOAD {
            self.fetched.fetch_sub(total, Ordering::AcqRel);
            return Err(format!("other players' buses fetched this session passed {}", crate::mods::upload::mb(MAX_SESSION_DOWNLOAD)).into());
        }
        // (given back unless the bus is kept: a fetch that fails does not count)
        let mut reserved = Reserved(&self.fetched, total, false);
        self.set(fp, Wanted::Pending(0, total));
        let mut done = 0u64;
        // what this machine has itself, in the same version
        let mut todo = Vec::new();
        for k in missing {
            let e = &m.entries[k];
            match buses().own_copy(e) {
                Some(d) => {
                    put(e, &d)?;
                    done += e.size;
                }
                None => todo.push(k),
            }
        }
        let _ = writeln!(out, "BYE").and_then(|_| out.flush());
        let _ = raw.shutdown(std::net::Shutdown::Both);
        // the rest from the host, in pieces over several connections; a host of an older
        // version, file by file
        let progress = Progress::new(self, fp, done, total);
        match fetch_pieces(&m, &todo, host, session, &progress) {
            Err(e) if e.older_host() => fetch_blobs(&m, &todo, host, session, &progress)?,
            r => r?,
        }
        commit(m)?;
        reserved.2 = true;
        if total > 0 {
            crate::mods::upload::show(format!("Another player's bus is here ({}): it is shown now", crate::mods::upload::mb(total)), 1, 1, false);
        }
        Ok(())
    }
}

/// Bytes of a session's download budget taken for one bus; given back when dropped unless
/// the bus was kept (`.2`).
struct Reserved<'a>(&'a std::sync::atomic::AtomicU64, u64, bool);

impl Drop for Reserved<'_> {
    fn drop(&mut self) {
        if !self.2 {
            self.0.fetch_sub(self.1, std::sync::atomic::Ordering::AcqRel);
        }
    }
}

/// One of the `DOWNLOADS` places of a viewer, held while a bus is fetched.
pub(super) struct Slot<'a>(&'a Viewer);

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        let (n, freed) = &self.0.slots;
        *n.lock().unwrap_or_else(|e| e.into_inner()) -= 1;
        freed.notify_one();
    }
}

/// The files of `reads` outside the bus's mount `at` - but for the game's own program files.
pub(super) fn escapes(at: &Path, root: &Path, reads: &[PathBuf]) -> Vec<PathBuf> {
    let program = root.join("program");
    let mut out: Vec<PathBuf> = reads.iter().filter(|p| !p.starts_with(at) && !p.starts_with(&program)).cloned().collect();
    out.sort();
    out.dedup();
    out
}

/// This machine's copy of `bus`, if its files in `paint` - listed as its driver's game lists
/// them (`bus_files`) - are the bus `fp`.
pub(super) fn local_copy(root: &Path, fp: &str, bus: &str, paint: &str) -> Result<PathBuf, String> {
    let rel = omsi_net::vehicle_path(bus).ok_or("no vehicle path")?;
    let path = omsi_cfg::find_in_roots(&rel).map(|(_, p)| p).ok_or("not installed here")?;
    let ty = omsi_sim::VehicleType::load(root, &path).map_err(|e| e.to_string())?;
    // (the paint by its name; a player who picked none may be in the type's own look)
    let named = ty.paint_schemes.iter().position(|s| s.name.eq_ignore_ascii_case(paint));
    let mut tries = vec![named];
    if named.is_some() {
        tries.push(None);
    }
    for scheme in tries {
        let files = crate::mods::upload::listed(root, crate::mods::bus_files::of_type(root, &ty, scheme, None));
        let m = VehicleManifest { bus: rel.clone(), entries: crate::mods::upload::hashed(&files) };
        if m.validated().is_ok_and(|m| m.fingerprint() == fp) {
            return Ok(path);
        }
    }
    Err("the copy here is another version".into())
}

/// How far a bus being fetched has come, told to the box that stands for it and to the
/// player (every connection adds to it).
pub(super) struct Progress<'a> {
    viewer: &'a Viewer,
    fp: &'a str,
    done: std::sync::atomic::AtomicU64,
    total: u64,
    /// What it took on the wire (pieces packed).
    wire: std::sync::atomic::AtomicU64,
    said: Mutex<std::time::Instant>,
}

impl<'a> Progress<'a> {
    fn new(viewer: &'a Viewer, fp: &'a str, done: u64, total: u64) -> Self {
        Progress { viewer, fp, done: done.into(), total, wire: 0.into(), said: Mutex::new(std::time::Instant::now()) }
    }

    fn add(&self, n: u64) {
        let done = self.done.fetch_add(n, std::sync::atomic::Ordering::Relaxed) + n;
        let mut said = self.said.lock().unwrap_or_else(|e| e.into_inner());
        if said.elapsed() > std::time::Duration::from_millis(250) {
            *said = std::time::Instant::now();
            self.viewer.set(self.fp, Wanted::Pending(done, self.total));
            let pct = done * 100 / self.total.max(1);
            crate::mods::upload::show(format!("Downloading another player's bus: {pct}% ({} of {})", crate::mods::upload::mb(done), crate::mods::upload::mb(self.total)), done, self.total, true);
        }
    }
}

/// The files `todo` of `m` from the host in pieces (`pieces`), several connections at once:
/// each piece kept as it comes - what came before, in an attempt that broke off, is not
/// asked for again - and each file checked and kept once it is whole.
pub(super) fn fetch_pieces(m: &VehicleManifest, todo: &[usize], host: std::net::SocketAddr, session: u64, progress: &Progress) -> Result<(), Failure> {
    let mut items = Vec::new();
    let mut asked = HashSet::new();
    for k in todo {
        let e = &m.entries[*k];
        let left = missing_pieces(e);
        let had: u64 = (0..pieces::count(e.size)).filter(|i| !left.contains(i)).map(|i| pieces::span(e.size, i).1).sum();
        if had > 0 {
            progress.add(had);
        }
        // (a file whose every piece came before is put together now)
        if left.is_empty() {
            buses().finish(e, IN_USE)?;
        }
        // (a file the bus has twice - the same texture in two folders - is fetched once: its
        // pieces are kept by their content, so the other is whole with it)
        for i in left {
            if asked.insert((e.sha256.clone(), i)) {
                items.push((*k, i));
            } else {
                progress.add(pieces::span(e.size, i).1);
            }
        }
    }
    if !items.is_empty() {
        log::info!("vehicles: {} pieces of {} files to fetch over up to {} connections", items.len(), todo.len(), pieces::CONNECTIONS);
    }
    let bytes: u64 = items.iter().map(|(k, i)| pieces::span(m.entries[*k].size, *i).1).sum();
    let t0 = std::time::Instant::now();
    let r = pieces::share_out(items, pieces::CONNECTIONS, |take| {
        let (raw, mut out, mut input) = failure::open(host, session)?;
        // (we read pieces packed with zstd: an older host answers `ERR what` and packs them
        // as before - the answer is read ahead of the first pieces, no wait for it)
        let mut asked_zstd = writeln!(out, "ZSTD").is_ok();
        let r = (|| loop {
            let batch = take();
            if batch.is_empty() {
                return Ok(());
            }
            for (k, i) in &batch {
                writeln!(out, "PIECE {} {i}", m.entries[*k].sha256).map_err(|e| Failure::ConnectionLost(e.to_string()))?;
            }
            out.flush().map_err(|e| Failure::ConnectionLost(e.to_string()))?;
            if std::mem::take(&mut asked_zstd) {
                let mut line = String::new();
                crate::mods::read_line_within(&mut input, &mut line, crate::mods::MAX_REPLY).map_err(|e| Failure::ConnectionLost(e.to_string()))?;
            }
            for (k, i) in &batch {
                let e = &m.entries[*k];
                let len = pieces::span(e.size, *i).1;
                let (data, wire) = pieces::receive(&mut input, len).map_err(|x| x.about(&e.path))?;
                match put_piece(e, *i, &data) {
                    // (refused here for what it is: the bus is kept without it - `withhold`)
                    Err(why) if is_withheld(e) => log::warn!("vehicles: {why}; the bus is drawn without it"),
                    r => {
                        r?;
                    }
                }
                progress.wire.fetch_add(wire, std::sync::atomic::Ordering::Relaxed);
                progress.add(len);
            }
        })();
        let _ = writeln!(out, "BYE").and_then(|_| out.flush());
        let _ = raw.shutdown(std::net::Shutdown::Both);
        r
    });
    if r.is_ok() && bytes > 0 {
        let wire = progress.wire.load(std::sync::atomic::Ordering::Relaxed);
        let secs = t0.elapsed().as_secs_f64();
        log::info!("vehicles: {:.1} MB fetched as {:.1} MB on the wire ({:.0}%) in {secs:.1} s", bytes as f64 / 1e6, wire as f64 / 1e6, wire as f64 * 100.0 / bytes as f64);
    }
    r
}

/// The files `todo` of `m` from a host of an older version: whole, one connection, every
/// request at once and the answers as they come.
pub(super) fn fetch_blobs(m: &VehicleManifest, todo: &[usize], host: std::net::SocketAddr, session: u64, progress: &Progress) -> Result<(), Failure> {
    let (raw, mut out, mut input) = failure::open(host, session)?;
    let todo: Vec<usize> = todo.iter().copied().filter(|k| bus_store_lacks(&m.entries[*k])).collect();
    let requests: String = todo.iter().map(|k| format!("BLOB {}\n", m.entries[*k].sha256)).chain(std::iter::once("BYE\n".to_string())).collect();
    let writer = std::thread::spawn(move || {
        let _ = out.write_all(requests.as_bytes()).and_then(|_| out.flush());
    });
    for k in &todo {
        let e = &m.entries[*k];
        let len = failure::reply(&mut input)?.ok_or("no size")?;
        if len != e.size {
            return Err(format!("{}: {len} bytes, the list says {}", e.path, e.size).into());
        }
        let mut data = vec![0u8; len as usize];
        input.read_exact(&mut data).map_err(|x| format!("{}: {x}", e.path))?;
        if crate::mods::sha256_of(&data) != e.sha256 {
            return Err(format!("{}: not the file the list names", e.path).into());
        }
        // (refused for what it is: the bus is kept without it - `withhold`)
        let refused = crate::mods::quarantine::screen(&e.path, &data).err();
        match refused {
            Some(why) => {
                log::warn!("vehicles: {why}; the bus is drawn without it");
                withhold(e, &why);
            }
            None => put(e, &data)?,
        }
        progress.add(e.size);
    }
    let _ = raw.shutdown(std::net::Shutdown::Both);
    let _ = writer.join();
    Ok(())
}

/// `e` is not kept whole here.
pub(super) fn bus_store_lacks(e: &Entry) -> bool {
    buses().lock().as_ref().is_none_or(|v| v.store.has(e).is_none())
}
