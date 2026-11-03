//! Our own bus as the others are to know it: the files it is made of, and their fingerprint.

use super::*;

/// Our own bus as the others are to know it: its files and their fingerprint, worked out
/// once for each bus and paint (`identify`, `begin`). The list stays as it was made, so the
/// fingerprint does not change under the others while the game reads on; what is read later
/// that the list lacks is a miss of `bus_files` to be found, not a new bus.
pub(super) struct Own {
    key: String,
    manifest: crate::mods::identity::VehicleManifest,
    fingerprint: String,
    files: Vec<(String, PathBuf)>,
}

/// Our bus's list, its fingerprint and its files (where each is read from).
pub(in crate::mods) type OwnIdentity = (crate::mods::identity::VehicleManifest, String, Vec<(String, PathBuf)>);

/// Our bus's identity, worked out once for each bus and paint. The game has one (`own_bus`,
/// behind this module's functions).
#[derive(Default)]
pub(in crate::mods) struct OwnBus {
    own: Mutex<Option<Own>>,
    /// (key, fingerprint) of `own`, for the frame: it never waits for one being worked out.
    fp: Mutex<(String, String)>,
    identifying: AtomicBool,
    /// The bus and paint whose files made no valid list: not worked out again every frame
    /// (only when the bus or its paint changes).
    failed: Mutex<String>,
}

static OWN_BUS: std::sync::LazyLock<OwnBus> = std::sync::LazyLock::new(OwnBus::default);

/// The game's own `OwnBus`.
pub(in crate::mods) fn own_bus() -> &'static OwnBus {
    &OWN_BUS
}

/// The fingerprint of our bus in that paint, once it is known (else empty).
pub(crate) fn own_fingerprint(bus: &str, paint: &str) -> String {
    own_bus().fingerprint(bus, paint)
}

/// Once a frame: work out our bus's fingerprint in the background when the bus or its
/// paint is new. `own` lists the bus's files (`own_files`): it runs on that thread too.
pub(crate) fn identify(root: &Path, bus: &str, paint: &str, own: impl FnOnce() -> Vec<PathBuf> + Send + 'static) {
    own_bus().identify(root, bus, paint, own);
}

/// Our bus's manifest in that paint, worked out if it is not yet (only then are its files
/// listed, `own`). None when the bus's files make no valid list.
pub(super) fn own_manifest(root: &Path, bus: &str, paint: &str, own: impl FnOnce() -> Vec<PathBuf>) -> Option<OwnIdentity> {
    own_bus().manifest(root, bus, paint, own)
}

impl OwnBus {
    pub(in crate::mods) fn fingerprint(&self, bus: &str, paint: &str) -> String {
        let fp = self.fp.lock().unwrap_or_else(|e| e.into_inner());
        if fp.0 == format!("{bus}|{paint}") {
            fp.1.clone()
        } else {
            String::new()
        }
    }

    pub(in crate::mods) fn identify(&'static self, root: &Path, bus: &str, paint: &str, own: impl FnOnce() -> Vec<PathBuf> + Send + 'static) {
        if bus.is_empty() || self.identifying.load(Ordering::Relaxed) {
            return;
        }
        let key = format!("{bus}|{paint}");
        if self.fp.lock().unwrap_or_else(|e| e.into_inner()).0 == key || *self.failed.lock().unwrap_or_else(|e| e.into_inner()) == key {
            return;
        }
        self.identifying.store(true, Ordering::Relaxed);
        let (root, bus, paint) = (root.to_path_buf(), bus.to_string(), paint.to_string());
        // (the files are listed on the thread: `bus_files` looks up sounds, textures and fonts)
        let spawned = std::thread::Builder::new().name("bus-identity".into()).spawn(move || {
            self.manifest(&root, &bus, &paint, own);
            self.identifying.store(false, Ordering::Relaxed);
        });
        if spawned.is_err() {
            self.identifying.store(false, Ordering::Relaxed);
        }
    }

    pub(in crate::mods) fn manifest(&self, root: &Path, bus: &str, paint: &str, own: impl FnOnce() -> Vec<PathBuf>) -> Option<OwnIdentity> {
        let key = format!("{bus}|{paint}");
        let mut guard = self.own.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(o) = guard.as_ref().filter(|o| o.key == key) {
            return Some((o.manifest.clone(), o.fingerprint.clone(), o.files.clone()));
        }
        let files = chosen(root, bus, own());
        // (what this run read was only for `chosen`'s log of what the description misses: the
        // recording stops and its set goes - it was kept, and grew, for the whole session)
        omsi_cfg::vfs::record_reads(false);
        let bus_rel = omsi_net::vehicle_path(bus).unwrap_or_else(|| bus.to_string());
        let made = crate::mods::identity::VehicleManifest { bus: bus_rel, entries: hashed(&files) }.validated();
        let (mut manifest, mut files) = match made {
            Ok(m) => (m, files),
            Err(e) => {
                log::warn!("bus identity: {bus}: {e}");
                *self.failed.lock().unwrap_or_else(|e| e.into_inner()) = key;
                return None;
            }
        };
        // a bus taken from the server's fleet: the server's version of it, when all of it is
        // here (ours or the multiplayer cache) - what it knows it by, whatever else we keep
        if crate::mods::host_bus::from_server(bus, paint) {
            crate::mods::host_bus::compare(bus, paint, &manifest.entries);
            if let Some((theirs, at)) = crate::mods::host_bus::server_version(bus, paint).filter(|(m, _)| m.fingerprint() != manifest.fingerprint()) {
                log::info!("bus identity: {bus} ({paint}): the server's version is all here; the others know it by that ({} files, ours {})", theirs.entries.len(), manifest.entries.len());
                (manifest, files) = (theirs, at);
            }
        }
        let fingerprint = manifest.fingerprint();
        log::info!("bus identity: {bus} ({paint}) is {fingerprint}: {} files, {:.1} MB", manifest.entries.len(), manifest.size() as f64 / 1e6);
        *self.fp.lock().unwrap_or_else(|e| e.into_inner()) = (key.clone(), fingerprint.clone());
        self.failed.lock().unwrap_or_else(|e| e.into_inner()).clear();
        *guard = Some(Own { key, manifest: manifest.clone(), fingerprint: fingerprint.clone(), files: files.clone() });
        Some((manifest, fingerprint, files))
    }

    /// The bus worked out last is forgotten (it is worked out again).
    #[cfg(test)]
    pub(in crate::mods) fn forget(&self) {
        *self.own.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.failed.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// Whether the files of that bus and paint made no valid list.
    #[cfg(test)]
    pub(in crate::mods) fn has_failed(&self, bus: &str, paint: &str) -> bool {
        *self.failed.lock().unwrap_or_else(|e| e.into_inner()) == format!("{bus}|{paint}")
    }
}

/// What goes for `bus` - and what its fingerprint is over: the files its description names
/// (`bus_files`, `own`), each once by the path every machine knows it by. Only those: any
/// machine with the same files works out the same list, so a player who has the same version
/// of the bus knows it is the same without fetching anything (`vehicles::want`). What this
/// game read for the bus besides is no part of it: it is logged, a miss of `bus_files` to
/// look into.
pub(super) fn chosen(root: &Path, bus: &str, own: Vec<PathBuf>) -> Vec<(String, PathBuf)> {
    let out = listed(root, own);
    let named: HashSet<String> = out.iter().filter_map(|(rel, _)| crate::mods::identity::key(rel)).collect();
    let extra: Vec<String> = files_of(root, bus).into_iter().map(|(rel, _)| rel).filter(|rel| crate::mods::identity::key(rel).is_some_and(|k| !named.contains(&k))).collect();
    if !extra.is_empty() {
        log::info!("bus identity: {bus}: {} files were read for it that its description does not name (not in its fingerprint), e.g. {}", extra.len(), extra.iter().take(8).cloned().collect::<Vec<_>>().join(", "));
    }
    out
}

/// `own` (absolute) as (path every machine knows it by, where it is), each once, in order -
/// once by the key the list is checked by (`identity::key`: case and Unicode form aside), so
/// one file reached by two spellings (NFC and NFD, as macOS may give) is listed once.
pub(crate) fn listed(root: &Path, own: Vec<PathBuf>) -> Vec<(String, PathBuf)> {
    let mut seen = HashSet::new();
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for p in own {
        if let Some(rel) = crate::mods::bus_files::relative(root, &p).filter(|r| crate::mods::refuse_path(r).is_none()) {
            if crate::mods::identity::key(&rel).is_some_and(|k| seen.insert(k)) {
                out.push((rel, p));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The files of the bus `v` the player drives (None: no bus, no files), in its paint
/// (`bus_files`), listed when the answer is called - on the thread that needs them, not the
/// frame's: `bus_files` looks up sound configs, the textures of every material and the
/// fonts. Taken now is only what that needs (the vehicle's types are shared, not copied).
/// (Not its depot file: which one a player picked is no part of the bus, and the displays'
/// texts go along in INFO.)
pub fn own_files(root: &Path, v: Option<&omsi_sim::VehicleInstance>) -> impl FnOnce() -> Vec<PathBuf> + Send + 'static {
    let root = root.to_path_buf();
    // (its rear sections as the vehicle has them loaded: not read from the disk again)
    let taken = v.map(|v| (v.ty.clone(), v.trailers.iter().map(|t| t.ty.clone()).collect::<Vec<_>>(), v.host.paint_scheme.flatten()));
    move || {
        let Some((ty, trailers, scheme)) = taken else { return Vec::new() };
        let parts: Vec<&omsi_sim::VehicleType> = trailers.iter().map(|t| &**t).collect();
        crate::mods::bus_files::of_parts(&root, &ty, &parts, scheme, None)
    }
}

/// The files of ours this game has read for `bus`: its folder and the folders its texts
/// point into, and the fonts its text textures use. (relative path, where it is read from)
pub(super) fn files_of(root: &Path, bus: &str) -> Vec<(String, PathBuf)> {
    let Some(bus_rel) = omsi_net::vehicle_path(bus).map(|b| norm(&b.replace('\\', "/"))) else { return Vec::new() };
    // our own content (not the original installation, not what the host sent)
    let roots: Vec<PathBuf> = omsi_cfg::content_roots().into_iter().filter(|r| r != root && !omsi_cfg::is_sandbox(r)).collect();
    let mut read: HashMap<String, (String, PathBuf)> = HashMap::new();
    for p in omsi_cfg::vfs::reads() {
        let Some(r) = roots.iter().filter(|r| p.starts_with(r)).max_by_key(|r| r.components().count()) else { continue };
        let Ok(rest) = p.strip_prefix(r) else { continue };
        let rel: Vec<String> = rest.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        let rel = rel.join("/");
        read.insert(rel.to_lowercase(), (rel, p));
    }
    let Some(own) = owner_folder(&bus_rel) else { return Vec::new() };
    let mut prefixes: HashSet<String> = HashSet::from([own.to_lowercase()]);
    let inside = |prefixes: &HashSet<String>, lower: &str| prefixes.iter().any(|f| lower.starts_with(&format!("{f}/")));
    let mut scanned: HashSet<String> = HashSet::new();
    let mut fonts_used: HashSet<String> = HashSet::new();
    for _ in 0..6 {
        let before = prefixes.len();
        let texts: Vec<(String, PathBuf)> = read
            .iter()
            .filter(|(lower, _)| inside(&prefixes, lower) && !scanned.contains(*lower))
            .filter(|(lower, _)| [".bus", ".cfg", ".osc", ".txt", ".ovh", ".zug"].iter().any(|e| lower.ends_with(e)))
            .map(|(lower, (rel, p))| {
                let _ = lower;
                (rel.clone(), p.clone())
            })
            .collect();
        for (rel, path) in texts {
            scanned.insert(rel.to_lowercase());
            let Ok(text) = omsi_cfg::vfs::read_text(&path) else { continue };
            let here = rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            let lines: Vec<&str> = text.lines().collect();
            for (i, l) in lines.iter().enumerate() {
                let t = l.trim();
                if t.to_ascii_lowercase().starts_with("[texttexture") {
                    if let Some(f) = lines.get(i + 2) {
                        fonts_used.insert(f.trim().to_lowercase());
                    }
                }
                if !t.contains("..") || t.starts_with('[') || t.len() > 300 {
                    continue;
                }
                let target = norm(&format!("{here}/{t}"));
                let top = target.split('/').next().unwrap_or("").to_ascii_lowercase();
                if !crate::mods::FOLDERS.contains(&top.as_str()) {
                    continue;
                }
                if let Some(f) = owner_folder(&target).filter(|f| f.split('/').count() >= 2) {
                    prefixes.insert(f.to_lowercase());
                }
            }
        }
        if prefixes.len() == before {
            break;
        }
    }
    let mut out: Vec<(String, PathBuf)> = read.iter().filter(|(lower, _)| inside(&prefixes, lower)).map(|(_, v)| v.clone()).collect();
    // the fonts: each `.oft` read with a `[newfont]` of a name in use, and its bitmaps
    for (lower, (rel, path)) in &read {
        if !lower.starts_with("fonts/") || !lower.ends_with(".oft") {
            continue;
        }
        let Ok(text) = omsi_cfg::vfs::read_text(path) else { continue };
        let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
        let mut wanted = false;
        for (i, l) in lines.iter().enumerate() {
            if l.eq_ignore_ascii_case("[newfont]") && lines.get(i + 1).is_some_and(|n| fonts_used.contains(&n.to_lowercase())) {
                wanted = true;
                for b in lines.iter().skip(i + 2).take(2) {
                    if let Some(v) = read.get(&format!("fonts/{}", b.to_lowercase())) {
                        out.push(v.clone());
                    }
                }
            }
        }
        if wanted {
            out.push((rel.clone(), path.clone()));
        }
    }
    let mut seen = HashSet::new();
    out.retain(|(rel, _)| crate::mods::refuse_path(rel).is_none() && seen.insert(rel.to_lowercase()));
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// A local file's hash as remembered (`crate::mods::hash_cache`): its key, then (size,
/// modification time, SHA-256).
type Remembered = (String, (u64, u64, String));

fn modified_secs(md: &std::fs::Metadata) -> u64 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Hash without buffering the entire mod in RAM. Refuse programs by their magic
/// bytes before processing the rest, as the old whole-file implementation did.
fn hash_stream(mut reader: impl Read) -> std::io::Result<Option<(u64, String)>> {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    // A Read is allowed to return fewer than 8 bytes even before EOF. Collect
    // the whole magic header first so a short read cannot bypass filtering.
    let mut header = [0u8; 8];
    let mut head_len = 0;
    while head_len < header.len() {
        let n = reader.read(&mut header[head_len..])?;
        if n == 0 { break; }
        head_len += n;
    }
    if crate::mods::looks_executable(&header[..head_len]) { return Ok(None); }
    hasher.update(&header[..head_len]);
    let mut size = head_len as u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 { break; }
        size += n as u64;
        if size > crate::mods::MAX_FILE { return Ok(None); }
        hasher.update(&buffer[..n]);
    }
    let digest = hasher.finalize();
    Ok(Some((size, digest.iter().map(|b| format!("{b:02x}")).collect())))
}

fn hashed_virtual(rel: &str, p: &Path) -> Option<(Entry, Option<Remembered>)> {
    // Unlike disk reads, VFS reads still buffer an entry. Allow only one at once.
    static VIRTUAL_READ: Mutex<()> = Mutex::new(());
    let _guard = VIRTUAL_READ.lock().unwrap_or_else(|e| e.into_inner());
    let data = omsi_cfg::vfs::read(p).ok()?;
    if data.len() as u64 > crate::mods::MAX_FILE { return None; }
    let (size, hash) = hash_stream(data.as_slice()).ok()??;
    Some((Entry { path: rel.to_owned(), size, sha256: hash }, None))
}

fn hashed_one(rel: &str, p: &Path, cache: &std::collections::HashMap<String, (u64, u64, String)>) -> Option<(Entry, Option<Remembered>)> {
    // Keep the same memory -> sealed -> ZIP -> disk precedence as vfs::read.
    if omsi_cfg::vfs::is_virtual(p) { return hashed_virtual(rel, p); }
    let key = p.to_string_lossy().into_owned();
    for attempt in 0..2 {
        let mut file = match std::fs::File::open(p) {
            Ok(file) => file,
            Err(_) => return hashed_virtual(rel, p),
        };
        let before = file.metadata().ok()?;
        if !before.is_file() || before.len() > crate::mods::MAX_FILE { return None; }
        let modified = modified_secs(&before);
        if let Some((len, mtime, hash)) = cache.get(&key) {
            if *len == before.len() && *mtime == modified {
                let mut head = Vec::with_capacity(8);
                (&mut file).take(8).read_to_end(&mut head).ok()?;
                if crate::mods::looks_executable(&head) { return None; }
                return Some((Entry { path: rel.to_owned(), size: *len, sha256: hash.clone() }, None));
            }
        }
        let (size, hash) = hash_stream(file).ok()??;
        let after = std::fs::metadata(p).ok()?;
        if size != before.len() || after.len() != before.len() || after.modified().ok() != before.modified().ok() {
            log::warn!("LAN mods: file changed while hashing (attempt {}): {}", attempt + 1, p.display());
            continue;
        }
        return Some((Entry { path: rel.to_owned(), size, sha256: hash.clone() }, Some((key, (size, modified, hash)))));
    }
    None
}

/// Size and SHA-256 of each file. A private two-worker pool streams disk files;
/// virtual entries are buffered one at a time and never use Rayon's global pool.
pub(in crate::mods) fn hashed(files: &[(String, PathBuf)]) -> Vec<Entry> {
    hashed_counted(files).0
}

/// `hashed`, and how many of the files the hash cache answered and how many bytes were read
/// to hash the others (the server's fleet check logs them).
pub(in crate::mods) fn hashed_counted(files: &[(String, PathBuf)]) -> (Vec<Entry>, usize, u64) {
    use rayon::prelude::*;
    let pool = rayon::ThreadPoolBuilder::new().num_threads(2).thread_name(|n| format!("mod-hash-{n}")).build();
    let mut cache = crate::mods::hash_cache();
    let mut entries = Vec::with_capacity(files.len());
    let mut dirty = false;
    let (mut cached, mut read) = (0usize, 0u64);
    let mut last_save = std::time::Instant::now();
    // Bounded result batches keep memory low while preserving manifest order.
    for batch in files.chunks(16) {
        let done: Vec<_> = match &pool {
            Ok(pool) => pool.install(|| batch.par_iter().map(|(rel, p)| hashed_one(rel, p, &cache)).collect()),
            Err(_) => batch.iter().map(|(rel, p)| hashed_one(rel, p, &cache)).collect(),
        };
        for (entry, fresh) in done.into_iter().flatten() {
            if let Some((key, value)) = fresh {
                read += value.0;
                cache.insert(key, value);
                dirty = true;
            } else {
                cached += 1;
            }
            entries.push(entry);
        }
        if dirty && last_save.elapsed() >= std::time::Duration::from_secs(5) {
            crate::mods::save_hash_cache(&cache);
            dirty = false;
            last_save = std::time::Instant::now();
        }
    }
    if dirty { crate::mods::save_hash_cache(&cache); }
    (entries, cached, read)
}

#[cfg(test)]
mod hash_stream_tests {
    use super::hash_stream;
    use sha2::{Digest, Sha256};
    use std::io::Cursor;

    #[test]
    fn streams_files_without_changing_sha256() {
        let data = vec![0x41u8; 256 * 1024 + 19];
        let (size, sha) = hash_stream(Cursor::new(&data)).unwrap().unwrap();
        let expected = Sha256::digest(&data);
        assert_eq!(size, data.len() as u64);
        assert_eq!(sha, expected.iter().map(|b| format!("{b:02x}")).collect::<String>());
    }

    #[test]
    fn rejects_program_headers() {
        assert!(hash_stream(Cursor::new(b"MZfake executable".as_slice())).unwrap().is_none());
    }

    #[test]
    fn rejects_program_headers_even_across_short_reads() {
        struct OneByte<'a>(&'a [u8]);
        impl std::io::Read for OneByte<'_> {
            fn read(&mut self, dst: &mut [u8]) -> std::io::Result<usize> {
                if self.0.is_empty() || dst.is_empty() { return Ok(0); }
                dst[0] = self.0[0];
                self.0 = &self.0[1..];
                Ok(1)
            }
        }
        assert!(hash_stream(OneByte(b"MZfake executable")).unwrap().is_none());
    }
}

#[cfg(test)]
#[path = "hash_tests.rs"]
mod hash_tests;

/// Does the plain file `p` start as a program does (`looks_executable`)? One that cannot be
/// read is taken for one.
pub(super) fn starts_as_program(p: &Path) -> bool {
    let mut head = [0u8; 8];
    let Ok(mut f) = std::fs::File::open(p) else { return true };
    let mut n = 0;
    while n < head.len() {
        match f.read(&mut head[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(_) => return true,
        }
    }
    crate::mods::looks_executable(&head[..n])
}

/// `a/b/../c` → `a/c`, `/`-separated.
pub(super) fn norm(p: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for c in p.split(['/', '\\']) {
        match c.trim() {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    parts.join("/")
}

/// The folder a file belongs to: a vehicle's whole folder (`Vehicles/<name>`), else the
/// folder the file is in.
pub(super) fn owner_folder(rel: &str) -> Option<String> {
    let comps: Vec<&str> = rel.split('/').collect();
    if comps.len() < 3 {
        return None;
    }
    let top = comps[0].to_ascii_lowercase();
    if top == "vehicles" || top == "trains" {
        return Some(comps[..2].join("/"));
    }
    Some(comps[..comps.len() - 1].join("/"))
}
