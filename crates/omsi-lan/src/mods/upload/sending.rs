//! A joining player's game sends its bus to the host: under the loading screen at the start,
//! and again whenever it changes.

use super::*;
use crate::mods::failure::{self, says, Failure};

/// The loading screen waits for the sending of our bus at the start (then the others see it
/// at once), but not for ever: a big bus over a slow line goes on in the background.
pub(super) const START_WAIT: Duration = Duration::from_secs(180);

/// After the bus (or its paint) changed in the game: how long it is left before it is
/// offered (a player going through the paints sends only the one kept), and how often after
/// that it is offered again - in case the server lost it; what it has costs only the list.
pub(super) const SETTLE: Duration = Duration::from_secs(3);
pub(super) const AGAIN: Duration = Duration::from_secs(300);

/// What sending our bus is at (`Sender::tick`).
pub(super) struct Sending {
    key: String,
    since: Instant,
    last: Option<Instant>,
    busy: bool,
}

/// A joining player's game sending its bus: the host it goes to, whether that host takes
/// uploads at all, what is being sent, and whether the loading screen waits for it. The
/// game has one (`sender`, behind this module's functions).
#[derive(Default)]
pub(in crate::mods) struct Sender {
    /// Since when the start waits for our bus (`START_WAIT`), while it does.
    at_start: Mutex<Option<Instant>>,
    /// The host takes no uploads (a player's game hosting, or `accept_uploads = 0`).
    refused: AtomicBool,
    /// The host whose mods this game fetched (its mods connection, the session).
    host: Mutex<Option<(SocketAddr, u64)>>,
    sending: Mutex<Option<Sending>>,
}

static SENDER: std::sync::LazyLock<Sender> = std::sync::LazyLock::new(Sender::default);

/// The game's own `Sender`.
pub(in crate::mods) fn sender() -> &'static Sender {
    &SENDER
}

// (what the rest of the game asks, of the game's own sender)

/// Our world and bus have just been made (a joining player, still under the loading screen):
/// the bus goes to the host now, if it lacks it. `own` (the bus's files, `own_files`) is
/// worked out only when there is a host to send to (a player alone paid for it on every
/// start), and on the sending thread, not the frame's.
pub fn begin(root: &Path, bus: &str, paint: &str, own: impl FnOnce() -> Vec<PathBuf> + Send + 'static) {
    sender().begin(root, bus, paint, own);
}

pub(in crate::mods) fn remember_host(host: SocketAddr, session: u64) {
    sender().remember_host(host, session);
}

/// The host of the session this game joined, when the start's fetch of its mods did not
/// say it (a welcome that came late leaves that fetch out): its mods connection is on the
/// session's port number. One the fetch found (a tunnel's) is kept.
pub(crate) fn session_host(host: SocketAddr, session: u64) {
    sender().session_host(host, session);
}

/// Is the way to the host's mods of `session` known?
pub(crate) fn knows_host(session: u64) -> bool {
    sender().knows_host(session)
}

pub(in crate::mods) fn host() -> Option<(SocketAddr, u64)> {
    sender().host()
}

/// Once a frame (a joining player): offer our bus to the host once it is loaded.
pub(crate) fn tick(root: &Path, bus: &str, paint: &str, own: impl FnOnce() -> Vec<PathBuf> + Send + 'static) {
    sender().tick(root, bus, paint, own);
}

/// The host's answer says it takes no uploads: switched off, or a host of a version
/// without them (which does not know the request).
pub(super) fn no_uploads(e: &Failure) -> bool {
    e.host_says(says::TAKES_NO_UPLOADS) || e.older_host()
}

impl Sender {
    /// Since when the start waits for our bus, while it does.
    pub(super) fn waiting_since(&self) -> Option<Instant> {
        *self.at_start.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set_waiting(&self, since: Option<Instant>) {
        *self.at_start.lock().unwrap_or_else(|e| e.into_inner()) = since;
    }

    pub(in crate::mods) fn begin(&'static self, root: &Path, bus: &str, paint: &str, own: impl FnOnce() -> Vec<PathBuf> + Send + 'static) {
        let Some((host, session)) = self.host() else { return };
        if bus.is_empty() || self.refused.load(Ordering::Relaxed) || self.sending.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|s| s.busy) {
            return;
        }
        self.set_waiting(Some(Instant::now()));
        show("Preparing your bus for the other players...", 0, 0, true);
        let (root, bus, paint) = (root.to_path_buf(), bus.to_string(), paint.to_string());
        // (no waiting for the game's reads to settle: the list comes from the bus's own
        // description (`bus_files`); what a run read only feeds a log of what it misses)
        let spawned = std::thread::Builder::new().name("lan-upload".into()).spawn(move || {
            let r = match own_manifest(&root, &bus, &paint, own) {
                Some((_, _, files)) => send_files(&bus, files, host, session, false),
                None => Err(Failure::from("its files make no valid list")),
            };
            self.finish(&bus, r, false);
            self.set_waiting(None);
        });
        if spawned.is_err() {
            self.set_waiting(None);
        }
    }

    /// What became of sending our bus, for the player (`quiet`: a look again after a while,
    /// which says nothing when the server had everything).
    pub(super) fn finish(&self, bus: &str, r: Result<usize, Failure>, quiet: bool) {
        let name = bus.rsplit(['/', '\\']).next().unwrap_or(bus).trim_end_matches(".bus");
        match r {
            Ok(0) if quiet => {}
            Ok(0) => show(format!("The server has your bus ({name}): everybody sees it"), 0, 0, false),
            Ok(n) => show(format!("Your bus ({name}) is on the server now ({n} files): everybody sees it"), 1, 1, false),
            Err(e) if no_uploads(&e) => {
                log::info!("uploads: {bus}: {e}");
                self.refused.store(true, Ordering::Relaxed);
                notices().clear();
            }
            Err(e) => {
                log::warn!("uploads: {bus}: {e}");
                show(format!("Your bus could not be sent to the server ({e}): the others see a stand-in in its place"), 0, 0, false);
            }
        }
    }

    pub(in crate::mods) fn remember_host(&self, host: SocketAddr, session: u64) {
        *self.host.lock().unwrap_or_else(|e| e.into_inner()) = Some((host, session));
    }

    pub(in crate::mods) fn session_host(&self, host: SocketAddr, session: u64) {
        let mut h = self.host.lock().unwrap_or_else(|e| e.into_inner());
        if h.is_none_or(|(_, s)| s != session) {
            log::info!("uploads: the host's mods connection is {host}");
            *h = Some((host, session));
        }
    }

    pub(in crate::mods) fn knows_host(&self, session: u64) -> bool {
        self.host.lock().unwrap_or_else(|e| e.into_inner()).is_some_and(|(_, s)| s == session)
    }

    pub(in crate::mods) fn host(&self) -> Option<(SocketAddr, u64)> {
        *self.host.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(in crate::mods) fn tick(&'static self, root: &Path, bus: &str, paint: &str, own: impl FnOnce() -> Vec<PathBuf> + Send + 'static) {
        if bus.is_empty() {
            return;
        }
        let Some((host, session)) = self.host() else { return };
        let key = format!("{bus}|{paint}");
        let mut guard = self.sending.lock().unwrap_or_else(|e| e.into_inner());
        let s = guard.get_or_insert_with(|| Sending { key: String::new(), since: Instant::now(), last: None, busy: false });
        if self.refused.load(Ordering::Relaxed) || s.busy || self.waiting_since().is_some() {
            return;
        }
        if s.key != key {
            s.key = key;
            s.since = Instant::now();
            s.last = None;
            return;
        }
        let due = match s.last {
            None => s.since.elapsed() > SETTLE,
            Some(t) => t.elapsed() > AGAIN,
        };
        if !due {
            return;
        }
        s.busy = true;
        s.last = Some(Instant::now());
        let (root, bus, paint) = (root.to_path_buf(), bus.to_string(), paint.to_string());
        let spawned = std::thread::Builder::new().name("lan-upload".into()).spawn(move || {
            let r = match own_manifest(&root, &bus, &paint, own) {
                Some((_, _, files)) => send_files(&bus, files, host, session, true),
                None => Err(Failure::from("its files make no valid list")),
            };
            self.finish(&bus, r, true);
            if let Some(s) = self.sending.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                s.busy = false;
            }
        });
        if spawned.is_err() {
            s.busy = false;
        }
    }
}

/// `OFFER2` on a connection: for each file of `manifest` (as `body`) the host wants, the
/// pieces of it it lacks.
/// (A host of a version before the pieces answers `ERR what`: `Failure::older_host`.)
pub(super) fn offer_pieces(out: &mut impl Write, input: &mut impl std::io::BufRead, body: &[u8]) -> Result<Vec<(usize, Vec<u64>)>, Failure> {
    writeln!(out, "OFFER2 {}", body.len()).and_then(|_| out.write_all(body)).and_then(|_| out.flush()).map_err(|e| e.to_string())?;
    let len = failure::reply(input)?.ok_or("no answer")?;
    if len > 64 << 20 {
        return Err("the host's answer is too large".into());
    }
    let mut answer = vec![0u8; len as usize];
    input.read_exact(&mut answer).map_err(|e| e.to_string())?;
    serde_json::from_slice(&answer).map_err(|e| Failure::Other(e.to_string()))
}

/// Piece `i` of the file `e` of ours, read from `path`.
pub(super) fn local_piece(path: &Path, e: &crate::mods::Entry, i: u64) -> Result<Vec<u8>, String> {
    let (start, len) = crate::mods::pieces::span(e.size, i);
    let changed = || format!("{} changed while it was sent", path.display());
    if omsi_cfg::vfs::archive_of(path).is_none() && !omsi_cfg::vfs::is_sealed(path) {
        use std::io::Seek;
        let mut f = std::fs::File::open(path).map_err(|x| format!("{}: {x}", path.display()))?;
        if f.metadata().map(|m| m.len()).ok() != Some(e.size) {
            return Err(changed());
        }
        f.seek(std::io::SeekFrom::Start(start)).map_err(|x| x.to_string())?;
        let mut data = vec![0u8; len as usize];
        f.read_exact(&mut data).map_err(|x| format!("{}: {x}", path.display()))?;
        return Ok(data);
    }
    // (a file in a mounted archive is read whole: archives hold small files)
    let all = omsi_cfg::vfs::read(path).map_err(|x| format!("{}: {x}", path.display()))?;
    if all.len() as u64 != e.size {
        return Err(changed());
    }
    Ok(all[start as usize..(start + len) as usize].to_vec())
}

/// Our bus to the host in pieces (`pieces`), several connections at once: only the pieces
/// it lacks - what came before, in an upload that broke off, is not sent again. Then the
/// host keeps the bus (`DONE`). How many files it took.
pub(super) fn send_pieces(bus: &str, manifest: &VehicleManifest, source: &HashMap<String, PathBuf>, host: SocketAddr, session: u64) -> Result<usize, Failure> {
    let body = serde_json::to_vec(manifest).map_err(|e| e.to_string())?;
    let wait = Duration::from_secs(600);
    let (raw, mut out, mut input) = failure::connect(host, session, wait)?;
    let plan = offer_pieces(&mut out, &mut input, &body)?;
    let _ = writeln!(out, "BYE").and_then(|_| out.flush());
    let _ = raw.shutdown(std::net::Shutdown::Both);
    let sources: HashMap<usize, PathBuf> = plan
        .iter()
        .map(|(k, _)| manifest.entries.get(*k).and_then(|e| source.get(&e.path.to_lowercase())).map(|p| (*k, p.clone())).ok_or("the server asked for a file that is not ours"))
        .collect::<Result<_, _>>()?;
    let total: u64 = plan.iter().map(|(k, _)| manifest.entries[*k].size).sum();
    let left: u64 = plan.iter().map(|(k, ps)| ps.iter().map(|i| crate::mods::pieces::span(manifest.entries[*k].size, *i).1).sum::<u64>()).sum();
    // (a file the bus has twice - the same texture in two folders - goes once: the host keeps
    // pieces by their content, so the other is whole with it)
    let mut sent = HashSet::new();
    let mut twice = 0u64;
    let mut items: Vec<(usize, u64)> = Vec::new();
    for (k, ps) in &plan {
        let e = &manifest.entries[*k];
        for i in ps {
            if sent.insert((e.sha256.clone(), *i)) {
                items.push((*k, *i));
            } else {
                twice += crate::mods::pieces::span(e.size, *i).1;
            }
        }
    }
    let left = left - twice;
    let refused: Mutex<Vec<String>> = Mutex::new(Vec::new());
    if !items.is_empty() {
        log::info!(
            "uploads: the host lacks {} files of {bus} ({:.1} MB, {:.1} MB of it came before): {} pieces over up to {} connections",
            plan.len(),
            total as f64 / 1e6,
            (total - left) as f64 / 1e6,
            items.len(),
            crate::mods::pieces::CONNECTIONS
        );
        let done = std::sync::atomic::AtomicU64::new(total - left);
        let said = Mutex::new(Instant::now() - Duration::from_secs(1));
        let say = |n: u64| {
            let d = done.fetch_add(n, Ordering::Relaxed) + n;
            let mut s = said.lock().unwrap_or_else(|e| e.into_inner());
            if s.elapsed() > Duration::from_millis(250) || d == total {
                *s = Instant::now();
                let pct = d * 100 / total.max(1);
                show(format!("Sending your bus to the server for the other players: {pct}% ({} of {})", mb(d), mb(total)), d, total, true);
            }
        };
        say(0);
        crate::mods::pieces::share_out(items, crate::mods::pieces::CONNECTIONS, |take| {
            let (raw, mut out, mut input) = failure::connect(host, session, wait)?;
            let r = (|| {
                // (this connection's offer: the pieces it sends are of that list)
                offer_pieces(&mut out, &mut input, &body)?;
                loop {
                    let batch = take();
                    if batch.is_empty() {
                        return Ok(());
                    }
                    for (k, i) in &batch {
                        let data = local_piece(&sources[k], &manifest.entries[*k], *i)?;
                        let (codec, wire) = crate::mods::pieces::pack(&data);
                        writeln!(out, "PUTPIECE {k} {i} {} {codec} {}", data.len(), wire.len()).and_then(|_| out.write_all(&wire)).map_err(|e| e.to_string())?;
                    }
                    out.flush().map_err(|e| e.to_string())?;
                    for (k, i) in &batch {
                        let e = &manifest.entries[*k];
                        match failure::reply(&mut input) {
                            Ok(_) => {}
                            // (a file the host refused: the others go on)
                            Err(x @ Failure::HostSays(_)) => refused.lock().unwrap_or_else(|p| p.into_inner()).push(format!("{}: {x}", e.path)),
                            Err(x) => return Err(x),
                        }
                        say(crate::mods::pieces::span(e.size, *i).1);
                    }
                }
            })();
            let _ = writeln!(out, "BYE").and_then(|_| out.flush());
            let _ = raw.shutdown(std::net::Shutdown::Both);
            r
        })?;
    }
    let mut refused = refused.into_inner().unwrap_or_default();
    refused.dedup();
    if !refused.is_empty() {
        log::warn!("uploads: {} files refused by the host: {}", refused.len(), refused.iter().take(6).cloned().collect::<Vec<_>>().join("; "));
    }
    // every file there: the host keeps the bus by this fingerprint (also when it had every
    // file already - else the others asked it for this bus and waited for one never kept)
    let (raw, mut out, mut input) = failure::connect(host, session, wait)?;
    let r = (|| {
        let still = offer_pieces(&mut out, &mut input, &body)?;
        if !still.is_empty() {
            return Err(format!("{} files of {bus} did not come whole{}", still.len(), refused.first().map(|r| format!(" ({r})")).unwrap_or_default()).into());
        }
        writeln!(out, "DONE").and_then(|_| out.flush()).map_err(|e| e.to_string())?;
        failure::reply(&mut input)
    })();
    let _ = writeln!(out, "BYE").and_then(|_| out.flush());
    let _ = raw.shutdown(std::net::Shutdown::Both);
    r?;
    let taken = plan.len().saturating_sub(refused.len());
    if taken > 0 {
        log::info!("uploads: {bus}: {taken} files taken by the host");
    }
    Ok(taken)
}

/// `send` with the files chosen: (relative path, where it is read from).
pub(super) fn send_files(bus: &str, files: Vec<(String, PathBuf)>, host: SocketAddr, session: u64, quiet: bool) -> Result<usize, Failure> {
    if files.is_empty() {
        return Ok(0);
    }
    // (a big texture over a slow upload: the answer comes when all of it has arrived)
    let (raw, mut out, mut input) = failure::connect(host, session, Duration::from_secs(1800))?;
    // whether the host takes uploads at all, before a big bus is hashed for nothing
    let ask = serde_json::to_vec(&VehicleManifest { bus: bus.to_string(), entries: Vec::new() }).map_err(|e| e.to_string())?;
    writeln!(out, "OFFER {}", ask.len()).and_then(|_| out.write_all(&ask)).and_then(|_| out.flush()).map_err(|e| e.to_string())?;
    let len = failure::reply(&mut input)?.ok_or("no answer")?;
    let mut none = vec![0u8; len.min(64) as usize];
    input.read_exact(&mut none).map_err(|e| e.to_string())?;
    if !quiet {
        show("Checking which files of your bus the server lacks...", 0, 0, true);
    }
    let bus_rel = omsi_net::vehicle_path(bus).unwrap_or_else(|| bus.to_string());
    // (in the order the server keeps it: the files it asks for are named by their place there)
    let manifest = VehicleManifest { bus: bus_rel, entries: hashed(&files) }.validated()?;
    let source: HashMap<String, PathBuf> = files.iter().map(|(rel, p)| (rel.to_lowercase(), p.clone())).collect();
    // in pieces (a server of this version); whole files to an older one, on this connection
    match send_pieces(bus, &manifest, &source, host, session) {
        Err(e) if e.older_host() => log::info!("uploads: the host takes whole files only"),
        r => {
            let _ = writeln!(out, "BYE").and_then(|_| out.flush());
            let _ = raw.shutdown(std::net::Shutdown::Both);
            return r;
        }
    }
    let body = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
    writeln!(out, "OFFER {}", body.len()).and_then(|_| out.write_all(&body)).and_then(|_| out.flush()).map_err(|e| e.to_string())?;
    let len = failure::reply(&mut input)?.ok_or("no answer")?;
    if len > 64 << 20 {
        return Err("the host's answer is too large".into());
    }
    let mut answer = vec![0u8; len as usize];
    input.read_exact(&mut answer).map_err(|e| e.to_string())?;
    let wanted: Vec<usize> = serde_json::from_slice(&answer).map_err(|e| e.to_string())?;
    let wanted: Vec<usize> = wanted.into_iter().filter(|k| *k < manifest.entries.len()).collect();
    if wanted.is_empty() {
        // it has every file (its own installation's, another player's): DONE all the same, so
        // that it keeps the bus by this fingerprint - else the others asked it for this bus
        // and waited for one that never came
        writeln!(out, "DONE").and_then(|_| out.flush()).map_err(|e| e.to_string())?;
        let kept = failure::reply(&mut input);
        let _ = writeln!(out, "BYE").and_then(|_| out.flush());
        let _ = raw.shutdown(std::net::Shutdown::Both);
        kept?;
        return Ok(0);
    }
    let bytes: u64 = wanted.iter().map(|k| manifest.entries[*k].size).sum();
    log::info!("uploads: the host lacks {} files of {bus} ({:.1} MB): sending them", wanted.len(), bytes as f64 / 1e6);
    let say = |done: u64| {
        let pct = done * 100 / bytes.max(1);
        show(format!("Sending your bus to the server for the other players: {pct}% ({} of {})", mb(done), mb(bytes)), done, bytes, true);
    };
    say(0);
    // (a file that fails on our side ends the connection, so the answers stop waiting)
    let cut = raw.try_clone().map_err(|e| e.to_string())?;
    // every file at once, the answers read as they come (one round trip per file is slow
    // through a tunnel)
    let sources: Vec<(usize, u64, PathBuf)> = wanted.iter().filter_map(|k| {
        let e = &manifest.entries[*k];
        source.get(&e.path.to_lowercase()).map(|p| (*k, e.size, p.clone()))
    }).collect();
    if sources.len() != wanted.len() {
        return Err("the server asked for a file that is not ours".into());
    }
    let writer = std::thread::spawn(move || -> Result<(), String> {
        let r = (|| {
            for (k, size, path) in sources {
                let data = omsi_cfg::vfs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                if data.len() as u64 != size {
                    return Err(format!("{} changed while it was sent", path.display()));
                }
                writeln!(out, "PUT {k} {size}").and_then(|_| out.write_all(&data)).and_then(|_| out.flush()).map_err(|e| e.to_string())?;
            }
            writeln!(out, "DONE").and_then(|_| writeln!(out, "BYE")).and_then(|_| out.flush()).map_err(|e| e.to_string())
        })();
        if r.is_err() {
            let _ = cut.shutdown(std::net::Shutdown::Both);
        }
        r
    });
    let mut refused = Vec::new();
    let mut done = 0u64;
    for k in &wanted {
        let r = failure::reply(&mut input);
        done += manifest.entries[*k].size;
        say(done);
        if let Err(e) = r {
            if !matches!(e, Failure::HostSays(_)) {
                let _ = raw.shutdown(std::net::Shutdown::Both);
                // (our side's reason first: it is why the connection ended)
                return Err(match writer.join() {
                    Ok(Err(w)) => Failure::from(w),
                    _ => e,
                });
            }
            refused.push(format!("{}: {e}", manifest.entries[*k].path));
        }
    }
    let taken = failure::reply(&mut input);
    // (the writer's last words, BYE, go before the connection is closed: closed first, they
    // failed now and then, and a whole upload was taken for one that failed)
    let written = writer.join().map_err(|_| "the upload stopped".to_string());
    let _ = raw.shutdown(std::net::Shutdown::Both);
    written??;
    if !refused.is_empty() {
        log::warn!("uploads: {} files refused by the host: {}", refused.len(), refused.iter().take(6).cloned().collect::<Vec<_>>().join("; "));
    }
    let taken = taken?.unwrap_or(0) as usize;
    log::info!("uploads: {bus}: {taken} files taken by the host");
    Ok(taken)
}
