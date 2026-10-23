//! Installing mods as background jobs.
//!
//! A mod is a folder or a `.zip`. Installing it happens in four steps, each of which can be
//! followed on the Mods page and cancelled:
//!
//! 1. **Plan** - from the archive's central directory (or the folder listing) alone, work
//!    out what goes where: OMSI-style folders (`Vehicles`, `maps`, `Sceneryobjects` ...) are
//!    merged into the same folders of the content folder, a lone bus / map / object folder is
//!    put under the right folder, and everything else (read-mes, screenshots) is left out.
//!    A pack that only adds paints or textures to a bus that is not installed is kept aside
//!    in `Mods/waiting` and installed by itself once the bus is there.
//! 2. **Check** - the unpacked size of the planned files plus a margin must fit on the disk
//!    of the content folder; otherwise the job stops with the numbers before anything is
//!    written.
//! 3. **Unpack / copy** into `<content>/.install-staging/<pid>-<job>`, on the same volume
//!    as the content folder, with the free space watched while it runs. The files of a
//!    folder in the Mods inbox are hard-linked there instead (no space, no time), so the
//!    staging folder never holds the only copy of anything.
//! 4. **Move** into place: a new folder is renamed into the content folder in one step,
//!    files for an existing folder are renamed over it one by one. Only then are the
//!    linked inbox files removed from the inbox.
//!
//! The staging folder is removed whatever happens (done, failed, cancelled, or the launcher
//! killed half-way: stale ones of launchers that died - and the old `~/.openomsi/unzip`
//! - are removed on start).
//!
//! Every path inside a source goes through `safe_rel` before it is planned, so no entry
//! (`..\..\x` in a zip made on Windows included) can be written outside the staging folder.
//!
//! **In place** (`InstallMode::InPlace`): a `.zip` laid out like OMSI 2 is not unpacked at
//! all. It is put into `<content>/Archives/` - hard-linked when it is on the same disk
//! (no space, no time), moved when it came through the Mods inbox, else copied through the
//! staging folder after the same free-space check - and the game mounts every archive
//! there as a content root (`omsi_cfg::vfs`), as the launcher's own lists do. The plan is
//! still made, for the report and to check that the archive's folders are where the game
//! looks for them. `InstallMode::Auto` (the page's default) unpacks what fits on the disk
//! and uses an archive in place when its unpacked size does not.

use crate::i18n::{tr, trf};
use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Keep at least this much free on the disk after an install (bytes).
const MIN_MARGIN: u64 = 512 * 1024 * 1024;
/// Stop unpacking when the disk gets this full while a job runs (something else writes).
const ABORT_BELOW: u64 = 1024 * 1024 * 1024;
/// Folder of the running jobs' staging areas, inside the content folder.
pub const STAGING: &str = ".install-staging";
/// Packs waiting for the bus they belong to.
pub const WAITING: &str = "waiting";
/// Archives used in place, inside the content folder (the game mounts every `.zip` there).
pub const ARCHIVES: &str = "Archives";

/// How an archive is installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum InstallMode {
    /// Unpack (or copy) the files into the content folder.
    #[default]
    Extract,
    /// Put the archive into `<content>/Archives` and let the game read it as a content root.
    InPlace,
    /// Unpack when the unpacked files fit on the disk, else use the archive in place.
    Auto,
}

impl InstallMode {
    pub fn parse(s: &str) -> InstallMode {
        match s.trim().to_ascii_lowercase().as_str() {
            "inplace" | "in-place" | "mount" => InstallMode::InPlace,
            "auto" => InstallMode::Auto,
            _ => InstallMode::Extract,
        }
    }
}

/// What a job has done so far, as the page shows it.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Progress {
    pub id: u64,
    pub source: String,
    pub name: String,
    /// queued, planning, checking, unpacking, copying, moving, done, failed, cancelled
    pub state: String,
    pub mode: InstallMode,
    pub files_done: u64,
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// Free bytes on the content folder's disk when the job checked, and what it needs.
    pub free_bytes: u64,
    pub needed_bytes: u64,
    pub message: String,
    pub report: Vec<String>,
    pub warnings: Vec<String>,
    /// What was put into the content folder: `Vehicles/Foo`, `maps/Bar`, `Sceneryobjects`.
    pub installed: Vec<String>,
    /// Packs put into `Mods/waiting` (their names).
    pub kept_aside: Vec<String>,
    pub from_inbox: bool,
    pub started: u64,
    pub finished: Option<u64>,
}

pub struct Job {
    pub id: u64,
    pub source: PathBuf,
    pub mode: InstallMode,
    /// The source lies in the Mods inbox: its files may be linked instead of copied (and
    /// removed from the inbox once installed), and what is left of it goes to
    /// `Mods/installed`.
    pub from_inbox: bool,
    cancel: AtomicBool,
    progress: Mutex<Progress>,
    bytes_done: AtomicU64,
    files_done: AtomicU64,
    /// Inbox files hard-linked (not copied) into the staging folder. They stay where they
    /// are until the install is in place, and are removed from the inbox only then.
    linked: Mutex<Vec<PathBuf>>,
    /// Tests: wait for the cancel after this many files.
    stall_at: u64,
}

impl Job {
    pub fn snapshot(&self) -> Progress {
        let mut p = self.progress.lock().unwrap().clone();
        p.bytes_done = self.bytes_done.load(Ordering::Relaxed);
        p.files_done = self.files_done.load(Ordering::Relaxed);
        p
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn set(&self, f: impl FnOnce(&mut Progress)) {
        f(&mut self.progress.lock().unwrap());
    }

    fn state(&self, s: &str, msg: impl Into<String>) {
        let msg = msg.into();
        self.set(|p| {
            p.state = s.into();
            p.message = msg;
        });
    }

    fn is_active(&self) -> bool {
        self.progress.lock().unwrap().finished.is_none()
    }
}

static JOBS: Mutex<Vec<Arc<Job>>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
/// One install at a time: two unpacking at once would both pass the space check.
static RUNNING: Mutex<()> = Mutex::new(());

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// All jobs of this launcher, newest first.
pub fn jobs() -> Vec<Progress> {
    let mut v: Vec<Progress> = JOBS.lock().unwrap().iter().map(|j| j.snapshot()).collect();
    v.sort_by(|a, b| b.id.cmp(&a.id));
    v
}

pub fn cancel(id: u64) -> bool {
    match JOBS.lock().unwrap().iter().find(|j| j.id == id) {
        Some(j) if j.is_active() => {
            j.cancel();
            true
        }
        _ => false,
    }
}

/// Forget finished jobs (the page's "clear").
pub fn clear_finished() {
    JOBS.lock().unwrap().retain(|j| j.is_active());
}

/// Is `source` being installed right now?
pub fn is_busy(source: &Path) -> bool {
    JOBS.lock().unwrap().iter().any(|j| j.is_active() && j.source == source)
}

/// Queue an install; it runs on its own thread. `content` is the content folder.
pub fn start(content: PathBuf, root: Option<PathBuf>, source: PathBuf, mode: InstallMode, from_inbox: bool) -> Arc<Job> {
    start_inner(content, root, source, mode, from_inbox, u64::MAX)
}

fn start_inner(content: PathBuf, root: Option<PathBuf>, source: PathBuf, mode: InstallMode, from_inbox: bool, stall_at: u64) -> Arc<Job> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let name = source.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| source.display().to_string());
    let job = Arc::new(Job {
        id,
        source: source.clone(),
        mode,
        from_inbox,
        cancel: AtomicBool::new(false),
        progress: Mutex::new(Progress { id, source: source.to_string_lossy().to_string(), name, state: "queued".into(), mode, from_inbox, started: now_secs(), message: tr("waiting for the install before it"), ..Default::default() }),
        bytes_done: AtomicU64::new(0),
        files_done: AtomicU64::new(0),
        linked: Mutex::new(Vec::new()),
        stall_at,
    });
    JOBS.lock().unwrap().push(job.clone());
    let j = job.clone();
    std::thread::spawn(move || {
        let _one = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
        let result = if j.cancelled() {
            Err(anyhow!(Cancelled))
        } else {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&j, &content, root.as_deref()))).unwrap_or_else(|_| Err(anyhow!(tr("the install stopped with an internal error"))))
        };
        // (an inbox source's files were only linked into it: removing it loses nothing)
        let staging = staging_dir(&content, j.id);
        let _ = std::fs::remove_dir_all(&staging);
        let _ = std::fs::remove_dir(content.join(STAGING));
        // (whatever was written, the game's lookups start afresh)
        omsi_cfg::content_changed();
        match result {
            Ok(()) => j.set(|p| {
                p.state = "done".into();
                p.finished = Some(now_secs());
            }),
            Err(e) if e.downcast_ref::<Cancelled>().is_some() => j.set(|p| {
                p.state = "cancelled".into();
                p.message = tr("cancelled - nothing was installed, the unpacked files were removed");
                p.finished = Some(now_secs());
            }),
            Err(e) => j.set(|p| {
                p.state = "failed".into();
                p.message = format!("{e:#}");
                p.report.push(format!("failed: {e:#}"));
                p.finished = Some(now_secs());
            }),
        }
    });
    job
}

/// Run an install on this thread and return how it ended (the CLI).
pub fn run_blocking(content: PathBuf, root: Option<PathBuf>, source: PathBuf, mode: InstallMode, cancel_after: Option<std::time::Duration>, verbose: bool) -> Progress {
    let job = start(content, root, source, mode, false);
    let t0 = std::time::Instant::now();
    let mut last = String::new();
    loop {
        let p = job.snapshot();
        if verbose {
            let line = format!("{} {} {}/{} files {:.1}/{:.1} MB {}", p.state, p.name, p.files_done, p.files_total, p.bytes_done as f64 / 1e6, p.bytes_total as f64 / 1e6, p.message);
            if line != last {
                eprintln!("{line}");
                last = line;
            }
        }
        if p.finished.is_some() {
            return p;
        }
        if let Some(c) = cancel_after {
            if t0.elapsed() >= c {
                job.cancel();
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[derive(Debug)]
struct Cancelled;
impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}
impl std::error::Error for Cancelled {}

fn staging_dir(content: &Path, id: u64) -> PathBuf {
    content.join(STAGING).join(format!("{}-{id}", std::process::id()))
}

// ---------------------------------------------------------------------------------------
// disk space and processes

/// Free bytes for this user on the volume of `path` (the nearest existing ancestor).
pub fn free_space(path: &Path) -> Option<u64> {
    let mut p = path.to_path_buf();
    while !p.exists() {
        if !p.pop() {
            return None;
        }
    }
    free_space_impl(&p)
}

#[cfg(unix)]
fn free_space_impl(p: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(p.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    Some(s.f_bavail as u64 * s.f_frsize as u64)
}

#[cfg(windows)]
fn free_space_impl(p: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
    }
    let wide: Vec<u16> = p.as_os_str().encode_wide().chain(Some(0)).collect();
    let (mut a, mut t, mut f) = (0u64, 0u64, 0u64);
    (unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut a, &mut t, &mut f) } != 0).then_some(a)
}

#[cfg(not(any(unix, windows)))]
fn free_space_impl(_p: &Path) -> Option<u64> {
    None
}

/// Is the process `pid` still running?
#[cfg(unix)]
pub fn pid_alive(pid: u32) -> bool {
    let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
    r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
pub fn pid_alive(pid: u32) -> bool {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn GetExitCodeProcess(h: isize, code: *mut u32) -> i32;
        fn CloseHandle(h: isize) -> i32;
    }
    const QUERY_LIMITED: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    unsafe {
        let h = OpenProcess(QUERY_LIMITED, 0, pid);
        if h == 0 {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code) != 0 && code == STILL_ACTIVE;
        CloseHandle(h);
        ok
    }
}

#[cfg(not(any(unix, windows)))]
pub fn pid_alive(_pid: u32) -> bool {
    true
}

pub fn gb(b: u64) -> String {
    if b >= 1 << 30 {
        format!("{:.1} GB", b as f64 / (1u64 << 30) as f64)
    } else if b >= 1 << 20 {
        format!("{:.0} MB", b as f64 / (1u64 << 20) as f64)
    } else {
        format!("{} KB", b.div_ceil(1024))
    }
}

/// Remove what interrupted installs left behind: the old `unzip` folder of earlier
/// launchers, and staging folders of launchers that are no longer running. Returns what
/// was removed.
pub fn cleanup_stale(data_dir: &Path, content: Option<&Path>) -> Vec<String> {
    let mut out = Vec::new();
    let legacy = data_dir.join("unzip");
    if legacy.exists() {
        let size = tree_size(&legacy);
        if std::fs::remove_dir_all(&legacy).is_ok() {
            out.push(format!("removed {} of unpacked files an earlier install left in {}", gb(size), legacy.display()));
        }
    }
    let Some(content) = content else { return out };
    let dir = content.join(STAGING);
    let Ok(rd) = std::fs::read_dir(&dir) else { return out };
    let active: Vec<u64> = JOBS.lock().unwrap().iter().filter(|j| j.is_active()).map(|j| j.id).collect();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let (pid, id) = match name.split_once('-') {
            Some((p, i)) => (p.parse::<u32>().unwrap_or(0), i.parse::<u64>().unwrap_or(0)),
            None => (0, 0),
        };
        let mine = pid == std::process::id();
        let stale = if mine { !active.contains(&id) } else { pid == 0 || !pid_alive(pid) };
        if stale {
            let size = tree_size(&e.path());
            if std::fs::remove_dir_all(e.path()).is_ok() {
                out.push(format!("removed {} of partial files of an interrupted install ({})", gb(size), e.path().display()));
            }
        }
    }
    let _ = std::fs::remove_dir(&dir);
    out
}

fn tree_size(p: &Path) -> u64 {
    let Ok(md) = std::fs::symlink_metadata(p) else { return 0 };
    if !md.is_dir() {
        return md.len();
    }
    std::fs::read_dir(p).map(|rd| rd.flatten().map(|e| tree_size(&e.path())).sum()).unwrap_or(0)
}

// ---------------------------------------------------------------------------------------
// the plan

/// One file of the source.
#[derive(Debug, Clone)]
struct Entry {
    /// Path inside the source, `/`-separated, no leading slash.
    rel: String,
    size: u64,
    /// Index in the archive (zip sources).
    index: usize,
}

/// A folder of the source as a tree of names.
#[derive(Default, Debug)]
struct Node {
    dirs: BTreeMap<String, Node>,
    /// (file name, entry index)
    files: Vec<(String, usize)>,
}

impl Node {
    fn insert(&mut self, rel: &str, entry: usize) {
        let mut parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
        let Some(file) = parts.pop() else { return };
        let mut n = self;
        for p in parts {
            n = n.dirs.entry(p.to_string()).or_default();
        }
        n.files.push((file.to_string(), entry));
    }

    fn get(&self, path: &str) -> Option<&Node> {
        let mut n = self;
        for p in path.split('/').filter(|p| !p.is_empty()) {
            n = n.dirs.get(p)?;
        }
        Some(n)
    }

    /// Every entry index below this node.
    fn all(&self, out: &mut Vec<usize>) {
        let mut stack = vec![self];
        while let Some(n) = stack.pop() {
            out.extend(n.files.iter().map(|f| f.1));
            stack.extend(n.dirs.values().rev());
        }
    }

    fn has_ext_deep(&self, exts: &[&str]) -> bool {
        let mut stack = vec![self];
        while let Some(n) = stack.pop() {
            if n.files.iter().any(|(f, _)| has_ext(f, exts)) {
                return true;
            }
            stack.extend(n.dirs.values());
        }
        false
    }
}

fn has_ext(name: &str, exts: &[&str]) -> bool {
    name.rsplit_once('.').map(|(_, x)| exts.iter().any(|e| x.eq_ignore_ascii_case(e))).unwrap_or(false)
}

/// A path inside a source (`/` or `\` separated) as a relative `/`-separated path, or None
/// when it could lead out of the folder it is unpacked into: absolute, with a drive or a
/// `:` (a Windows drive-relative path or stream), with a `..`, or a NUL. `zip`'s own check
/// reads `\` as a plain character on macOS and Linux, so `..\..\x` passes it and only
/// turns into a way out once the separators are unified - which is why this runs after.
fn safe_rel(name: &str) -> Option<String> {
    let name = name.replace('\\', "/");
    if name.starts_with('/') || name.contains('\0') || name.len() > MAX_REL_BYTES {
        return None;
    }
    let mut parts = Vec::new();
    for p in name.split('/') {
        match p {
            "" | "." => continue,
            ".." => return None,
            p if p.contains(':') || p.ends_with(['.', ' ']) || windows_device(p) => return None,
            p => parts.push(p),
        }
    }
    (!parts.is_empty() && parts.len() <= MAX_REL_DEPTH).then(|| parts.join("/"))
}

const MAX_REL_BYTES: usize = 1024;
const MAX_REL_DEPTH: usize = 64;

fn windows_device(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or(part).trim_end_matches(' ').to_ascii_uppercase();
    match stem.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" => true,
        s => (s.starts_with("COM") || s.starts_with("LPT")) && matches!(&s[3..], "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"),
    }
}

/// An error unless `rel` is a `safe_rel` path: checked again right before anything is
/// written, so that nothing lands outside its folder whatever the plan says.
fn check_rel(rel: &str) -> Result<()> {
    if safe_rel(rel).as_deref() != Some(rel) {
        return Err(anyhow!("refused to write {rel:?}: it is not a plain path inside the mod"));
    }
    Ok(())
}

pub(crate) fn check_installed_rel(rel: &str) -> Result<()> {
    let plain = !rel.is_empty()
        && !rel.starts_with(['/', '\\'])
        && !rel.contains('\0')
        && rel.split(['/', '\\']).all(|p| !p.is_empty() && p != "." && p != ".." && !p.contains(':'));
    if !plain {
        return Err(anyhow!("refused {rel:?}: it is not a plain path inside the content folder"));
    }
    Ok(())
}

/// Where a downloaded plugin is put instead of `Plugins` (see `plan`).
pub const PLUGINS_HELD: &str = "plugins-not-enabled";

/// Files that are never installed.
fn is_junk(rel: &str) -> bool {
    rel.split('/').any(|p| p == "__MACOSX" || p == ".DS_Store" || p.eq_ignore_ascii_case("thumbs.db") || p.eq_ignore_ascii_case("desktop.ini") || p.starts_with("._"))
}

/// The canonical spelling of a content folder name, if `name` is one.
pub fn content_folder_of(name: &str) -> Option<&'static str> {
    omsi_cfg::CONTENT_FOLDERS.iter().copied().find(|f| f.eq_ignore_ascii_case(name))
}

/// Content folders that never lie inside a bus, map or object folder (unlike `Texture`,
/// `Sound`, `Fonts` or `Scripts`, which a bus folder has too).
const PACK_ONLY: &[&str] = &["Vehicles", "maps", "Sceneryobjects", "Splines", "Humans", "Trains", "Drivers", "TicketPacks", "Plugins", "Situations", "Weather", "Money", "Announcements", "Inputs"];

/// What a folder is, judged by the files directly in it.
fn guess_kind(n: &Node) -> Option<&'static str> {
    if n.files.iter().any(|(f, _)| f.eq_ignore_ascii_case("global.cfg")) {
        return Some("maps");
    }
    for (ext, kind) in [("bus", "Vehicles"), ("ovh", "Vehicles"), ("zug", "Trains"), ("sco", "Sceneryobjects"), ("sli", "Splines"), ("hum", "Humans"), ("owt", "Weather"), ("otp", "TicketPacks"), ("oft", "Fonts"), ("odr", "Drivers"), ("osn", "Situations"), ("dll", "Plugins"), ("opl", "Plugins")] {
        if n.files.iter().any(|(f, _)| has_ext(f, &[ext])) {
            return Some(kind);
        }
    }
    None
}

/// Where a part of the source goes.
#[derive(Debug, Clone)]
struct Mapping {
    /// Folder inside the source ("" = its root).
    src: String,
    /// Destination relative to the content folder.
    dest: String,
    /// What it is, for the report.
    what: String,
    /// Entries (with their path below `src`).
    files: Vec<(usize, String)>,
    bytes: u64,
    /// Kept aside: a repaint / add-on for a bus that is not installed.
    aside: bool,
}

#[derive(Debug, Default)]
struct Plan {
    maps: Vec<Mapping>,
    notes: Vec<String>,
    warnings: Vec<String>,
}

/// Lookup of what is installed already (content folder and the OMSI 2 folder).
struct Installed<'a> {
    content: &'a Path,
    root: Option<&'a Path>,
}

impl Installed<'_> {
    /// The installed spelling of `Vehicles/<name>` (content folder first), if any.
    fn vehicle(&self, name: &str) -> Option<String> {
        // the content folder, the archives used in place there, the OMSI folder
        let archives = self.content.join(ARCHIVES);
        let mounted: Vec<PathBuf> = omsi_cfg::vfs::mounts().iter().map(|m| m.path().to_path_buf()).filter(|p| p.starts_with(&archives)).collect();
        let bases: Vec<&Path> = std::iter::once(self.content).chain(mounted.iter().map(|p| p.as_path())).chain(self.root).collect();
        for base in bases {
            for (n, is_dir) in omsi_cfg::vfs::list_dir(&base.join("Vehicles")).unwrap_or_default() {
                let n = n.to_string_lossy().to_string();
                if is_dir && n.eq_ignore_ascii_case(name) {
                    return Some(n);
                }
            }
        }
        None
    }
}

fn collect(node: &Node, src: &str, entries: &[Entry]) -> (Vec<(usize, String)>, u64) {
    let mut idx = Vec::new();
    node.all(&mut idx);
    let mut bytes = 0;
    let files = idx
        .into_iter()
        .map(|i| {
            bytes += entries[i].size;
            let below = entries[i].rel.strip_prefix(src).unwrap_or(&entries[i].rel).trim_start_matches('/').to_string();
            (i, below)
        })
        .collect();
    (files, bytes)
}

fn join(a: &str, b: &str) -> String {
    if a.is_empty() {
        b.to_string()
    } else {
        format!("{a}/{b}")
    }
}

/// Work out what goes where.
fn plan(entries: &[Entry], source_name: &str, installed: &Installed) -> Plan {
    let mut tree = Node::default();
    for (i, e) in entries.iter().enumerate() {
        tree.insert(&e.rel, i);
    }
    let mut plan = Plan::default();
    let mut used: HashSet<usize> = HashSet::new();
    // level by level: a folder that *is* a thing (a bus, a map ...) is taken whole; the
    // OMSI folders among a folder's children are merged; stop below the first level that
    // gave anything
    let mut level: Vec<String> = vec![String::new()];
    for _ in 0..5 {
        let mut next = Vec::new();
        let mut found = false;
        for path in &level {
            let Some(node) = tree.get(path) else { continue };
            // a folder holding bus files is a bus even with `Sound` / `Texture` in it; one
            // holding `Vehicles` or `Sceneryobjects` is a pack whatever lies beside them
            let pack = node.dirs.keys().any(|d| PACK_ONLY.iter().any(|f| f.eq_ignore_ascii_case(d)));
            if let Some(kind) = guess_kind(node).filter(|k| *k == "maps" || !pack) {
                let name = if path.is_empty() { source_name.to_string() } else { path.rsplit('/').next().unwrap_or(path).to_string() };
                let (files, bytes) = collect(node, path, entries);
                used.extend(files.iter().map(|f| f.0));
                let dest = format!("{kind}/{name}");
                plan.maps.push(Mapping { src: path.clone(), dest, what: kind_label(kind).into(), files, bytes, aside: false });
                found = true;
                continue;
            }
            for (child, sub) in &node.dirs {
                let cpath = join(path, child);
                match content_folder_of(child) {
                    Some("Vehicles") => {
                        found = true;
                        // each bus folder separately: a folder without a vehicle file only
                        // adds to a bus (paints, textures), which must be installed
                        for (bus, bn) in &sub.dirs {
                            let bpath = join(&cpath, bus);
                            let (files, bytes) = collect(bn, &bpath, entries);
                            used.extend(files.iter().map(|f| f.0));
                            if bn.has_ext_deep(&["bus", "ovh", "zug", "sco"]) {
                                plan.maps.push(Mapping { src: bpath, dest: format!("Vehicles/{bus}"), what: "vehicle".into(), files, bytes, aside: false });
                            } else {
                                match installed.vehicle(bus) {
                                    Some(have) => plan.maps.push(Mapping { src: bpath, dest: format!("Vehicles/{have}"), what: format!("add-on for {have} (paints / textures)"), files, bytes, aside: false }),
                                    None => {
                                        plan.warnings.push(trf("{bus}: only paints or textures for a bus that is not installed (Vehicles/{bus}) - kept aside in Mods/{folder} and installed by itself once that bus is installed", &[("bus", &bus), ("folder", &WAITING)]));
                                        plan.maps.push(Mapping { src: bpath, dest: format!("Vehicles/{bus}"), what: format!("add-on for {bus}, which is not installed"), files, bytes, aside: true });
                                    }
                                }
                            }
                        }
                        let loose: Vec<(usize, String)> = sub.files.iter().map(|(f, i)| (*i, f.clone())).collect();
                        if !loose.is_empty() {
                            plan.notes.push(format!("{} loose file(s) directly in {cpath} left out", loose.len()));
                        }
                    }
                    Some(canon) => {
                        found = true;
                        let (files, bytes) = collect(sub, &cpath, entries);
                        used.extend(files.iter().map(|f| f.0));
                        plan.maps.push(Mapping { src: cpath, dest: canon.to_string(), what: format!("{canon} folder"), files, bytes, aside: false });
                    }
                    None => next.push(cpath),
                }
            }
        }
        if found || next.is_empty() {
            break;
        }
        level = next;
    }
    // plugins are native code that the game loads by itself: one from a download is put
    // aside, and only the player moving it into `Plugins` makes it run
    for m in plan.maps.iter_mut() {
        let lower = m.dest.to_ascii_lowercase();
        if lower == "plugins" || lower.starts_with("plugins/") {
            m.dest = format!("Mods/{PLUGINS_HELD}{}", &m.dest["plugins".len()..]);
            plan.warnings.push(trf(
                "{name}: a plugin runs its own program code inside the game, so it was not enabled - it is in Mods/{folder}; move it into Plugins yourself if you trust where it came from",
                &[("name", &if m.src.is_empty() { source_name } else { &m.src }), ("folder", &PLUGINS_HELD)],
            ));
        }
    }
    let unused = entries.len() - used.len();
    if unused > 0 && !plan.maps.is_empty() {
        let mut sample: Vec<&str> = entries.iter().enumerate().filter(|(i, _)| !used.contains(i)).map(|(_, e)| e.rel.as_str()).take(4).collect();
        sample.sort();
        plan.notes.push(format!("{unused} file(s) that belong to no OMSI folder were left out (e.g. {})", sample.join(", ")));
    }
    plan
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "maps" => "map",
        "Vehicles" => "vehicle",
        "Trains" => "train",
        "Sceneryobjects" => "scenery objects",
        "Splines" => "splines",
        "Humans" => "people",
        "Weather" => "weather",
        "TicketPacks" => "ticket pack",
        "Fonts" => "fonts",
        "Drivers" => "driver",
        "Situations" => "situation",
        "Plugins" => "plugin",
        _ => "content",
    }
}

mod source;
use source::*;

// ---------------------------------------------------------------------------------------
// the job

fn run(job: &Job, content: &Path, root: Option<&Path>) -> Result<()> {
    // (the unit tests leave the user's own data folder alone)
    let data_dir = if cfg!(test) { content.join(".test-data") } else { crate::data_dir() };
    for line in cleanup_stale(&data_dir, Some(content)) {
        job.set(|p| p.report.push(line.clone()));
    }
    let src = job.source.clone();
    let archive = archive_kind(&src);
    let is_zip = archive == Some(ArchiveKind::Zip);
    let staged_archive = matches!(archive, Some(ArchiveKind::SevenZip | ArchiveKind::Rar));
    if archive.is_none() && !src.is_dir() {
        return Err(anyhow!(trf("{name} is neither a folder nor a supported mod archive (.zip, .7z, .rar)", &[("name", &src.display())])));
    }
    if src.starts_with(content) && !job.from_inbox && !src.starts_with(content.join("Mods")) {
        return Err(anyhow!(trf("{name} is already inside the content folder", &[("name", &src.display())])));
    }
    // (the name becomes a folder name: `...zip` has the stem `..`)
    let source_name = if archive.is_some() { src.file_stem() } else { src.file_name() }.map(|s| s.to_string_lossy().to_string()).filter(|n| safe_rel(n).as_deref() == Some(n.as_str())).unwrap_or_else(|| "mod".into());
    let (mut source, entries) = if staged_archive {
        let unpacked = staging_dir(content, job.id).join("source");
        job.state("unpacking", tr("checking the archive and unpacking it into staging"));
        unpack_archive(job, content, &src, &unpacked, archive.unwrap())?;
        let (entries, paths, refused) = list_folder(&unpacked, job)?;
        if refused > 0 {
            job.set(|p| p.warnings.push(trf("{refused} file(s) whose names cannot be installed safely were left out", &[("refused", &refused)])));
        }
        (Source::Folder(paths), entries)
    } else if is_zip {
        job.state("planning", tr("reading the archive's table of contents"));
        let f = std::fs::File::open(&src).with_context(|| format!("opening {}", src.display()))?;
        let mut z = zip::ZipArchive::new(std::io::BufReader::with_capacity(ZIP_BUFFER, f)).with_context(|| trf("{name} is not a readable zip archive", &[("name", &src.display())]))?;
        let (entries, problems) = list_zip(&mut z, job)?;
        if !problems.is_empty() {
            return Err(anyhow!("{}: {}", src.display(), problems.join("; ")));
        }
        (Source::Zip(z), entries)
    } else {
        job.state("planning", tr("listing the folder"));
        let (entries, paths, refused) = list_folder(&src, job)?;
        if refused > 0 {
            job.set(|p| p.warnings.push(trf("{refused} file(s) whose names cannot be installed safely (a '..', '\\' or ':' in them) were left out", &[("refused", &refused)])));
        }
        (Source::Folder(paths), entries)
    };
    if entries.is_empty() {
        return Err(anyhow!(trf("{name} is empty", &[("name", &src.display())])));
    }
    let installed = Installed { content, root };
    let plan = plan(&entries, &source_name, &installed);
    if plan.maps.is_empty() {
        return Err(anyhow!(trf("could not tell what {name} is: no Vehicles / maps / Sceneryobjects ... folders and no .bus / .sco / .sli / global.cfg files in it", &[("name", &src.display())])));
    }
    for m in &plan.maps {
        let line = format!("{} -> {}{}/ ({} files, {}){}", if m.src.is_empty() { source_name.as_str() } else { m.src.as_str() }, if m.aside { format!("Mods/{WAITING}/{source_name}/") } else { String::new() }, m.dest, m.files.len(), gb(m.bytes), if m.what.is_empty() { String::new() } else { format!(" - {}", m.what) });
        job.set(|p| p.report.push(line.clone()));
    }
    job.set(|p| {
        p.report.extend(plan.notes.iter().cloned());
        p.warnings.extend(plan.warnings.iter().cloned());
    });
    // 2. does it fit?
    job.state("checking", tr("checking the free disk space"));
    let total_files: u64 = plan.maps.iter().map(|m| m.files.len() as u64).sum();
    let total_bytes: u64 = plan.maps.iter().map(|m| m.bytes).sum();
    // an inbox folder's files are hard-linked into the staging folder (no space, and the
    // originals stay until the install is in place); everything else is written anew
    let moving = (job.from_inbox && !is_zip) || staged_archive;
    let needed = if moving { 0 } else { total_bytes + (total_bytes / 20).max(MIN_MARGIN) };
    let free = free_space(content).unwrap_or(u64::MAX);
    job.set(|p| {
        p.files_total = total_files;
        p.bytes_total = total_bytes;
        p.free_bytes = free;
        p.needed_bytes = needed;
    });
    // unpack, or use the archive in place?
    let layout = if is_zip { in_place_layout(&src, &plan) } else { Err(anyhow!(tr("only a .zip archive can be used in place; this archive must be unpacked"))) };
    let mode = if staged_archive {
        InstallMode::Extract
    } else { match job.mode {
        InstallMode::Auto if is_zip && needed > free => match &layout {
            Ok(()) => {
                let line = format!("{} unpacks to {}, which does not fit ({} free): using the archive in place instead", source_name, gb(total_bytes), gb(free));
                job.set(|p| p.report.push(line.clone()));
                InstallMode::InPlace
            }
            Err(e) => {
                let line = trf("the archive cannot be used in place either: {e}", &[("e", &format!("{e:#}"))]);
                job.set(|p| p.warnings.push(line.clone()));
                InstallMode::Extract
            }
        },
        InstallMode::Auto => InstallMode::Extract,
        m => m,
    }};
    job.set(|p| p.mode = mode);
    if mode == InstallMode::InPlace {
        layout?;
        drop(source);
        return place_archive(job, content, &src, &plan);
    }
    if needed > free {
        return Err(anyhow!(
            "not enough disk space: {} unpacks to {} ({} files), and with {} kept free that needs {}, but only {} is free on the disk of {}. Nothing was unpacked - free some space and try again.",
            source_name,
            gb(total_bytes),
            total_files,
            gb(needed - total_bytes),
            gb(needed),
            gb(free),
            content.display()
        ));
    }
    // 3. unpack / copy into the staging folder
    let staging = staging_dir(content, job.id);
    std::fs::create_dir_all(&staging).with_context(|| format!("creating {}", staging.display()))?;
    job.state(if is_zip { "unpacking" } else if moving { "moving" } else { "copying" }, trf("{files} files, {size}", &[("files", &total_files), ("size", &gb(total_bytes))]));
    let mut buf = vec![0u8; 1 << 20];
    let mut since_check = 0u64;
    for (mi, m) in plan.maps.iter().enumerate() {
        let base = staging.join(format!("{mi}"));
        for (idx, rel) in &m.files {
            if job.cancelled() {
                return Err(anyhow!(Cancelled));
            }
            check_rel(rel)?;
            let out = base.join(rel);
            if let Some(d) = out.parent() {
                std::fs::create_dir_all(d).with_context(|| format!("creating {}", d.display()))?;
            }
            // two entries of the same name: the later one wins, and it must not write
            // through a link to an inbox file
            if std::fs::symlink_metadata(&out).is_ok() {
                std::fs::remove_file(&out).with_context(|| format!("replacing {}", out.display()))?;
            }
            let e = &entries[*idx];
            match &mut source {
                Source::Zip(z) => {
                    let mut f = z.by_index(e.index).with_context(|| format!("reading {} from the archive", e.rel))?;
                    let mut w = std::fs::File::create(&out).with_context(|| format!("writing {}", out.display()))?;
                    loop {
                        let n = f.read(&mut buf).with_context(|| trf("unpacking {name} (the archive may be damaged)", &[("name", &e.rel)]))?;
                        if n == 0 {
                            break;
                        }
                        w.write_all(&buf[..n]).with_context(|| format!("writing {}", out.display()))?;
                        job.bytes_done.fetch_add(n as u64, Ordering::Relaxed);
                        since_check += n as u64;
                        if since_check > 64 << 20 {
                            since_check = 0;
                            if job.cancelled() {
                                return Err(anyhow!(Cancelled));
                            }
                            check_space(content)?;
                        }
                    }
                }
                Source::Folder(paths) => {
                    let from = &paths[e.index];
                    if moving && std::fs::hard_link(from, &out).is_ok() {
                        job.linked.lock().unwrap().push(from.clone());
                        job.bytes_done.fetch_add(e.size, Ordering::Relaxed);
                    } else {
                        let mut r = std::fs::File::open(from).with_context(|| format!("reading {}", from.display()))?;
                        let mut w = std::fs::File::create(&out).with_context(|| format!("writing {}", out.display()))?;
                        loop {
                            let n = r.read(&mut buf)?;
                            if n == 0 {
                                break;
                            }
                            w.write_all(&buf[..n]).with_context(|| format!("writing {}", out.display()))?;
                            job.bytes_done.fetch_add(n as u64, Ordering::Relaxed);
                            since_check += n as u64;
                            if since_check > 64 << 20 {
                                since_check = 0;
                                if job.cancelled() {
                                    return Err(anyhow!(Cancelled));
                                }
                                check_space(content)?;
                            }
                        }
                    }
                }
            }
            if job.files_done.fetch_add(1, Ordering::Relaxed) + 1 == job.stall_at {
                while !job.cancelled() {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
        }
    }
    drop(source);
    if job.cancelled() {
        return Err(anyhow!(Cancelled));
    }
    // 4. into place
    job.state("moving", tr("moving the files into the content folder"));
    let mut installed_items = Vec::new();
    let mut aside_items = Vec::new();
    // (the folders this install made: they go again when the mod is taken out of
    // Mods/installed, see `uninstall_removed`)
    let mut created = Vec::new();
    for (mi, m) in plan.maps.iter().enumerate() {
        let from = staging.join(format!("{mi}"));
        if !from.exists() {
            continue;
        }
        check_rel(&m.dest)?;
        let dest = if m.aside { content.join("Mods").join(WAITING).join(&source_name).join(&m.dest) } else { case_path(content, &m.dest) };
        if !m.aside && !dest.exists() && m.dest.contains('/') {
            if let Ok(rel) = dest.strip_prefix(content) {
                created.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        // (into a folder all mods share - Fonts, Texture, Sounds: what of it is this mod's,
        // each new file and folder at its top. Noted as the folder alone they were nobody's:
        // the list showed no fonts or textures of a mod, and they stayed when it went)
        if !m.aside && !m.dest.contains('/') {
            let mut tops: Vec<String> = m.files.iter().filter_map(|(_, rel)| rel.split(['/', '\\']).next().map(str::to_string)).filter(|t| !t.is_empty()).collect();
            tops.sort_by_key(|t| t.to_ascii_lowercase());
            tops.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
            for t in tops {
                if !case_path(&dest, &t).exists() {
                    if let Ok(rel) = case_path(&dest, &t).strip_prefix(content) {
                        created.push(rel.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }
        let replaced = move_into(&from, &dest).with_context(|| format!("moving into {}", dest.display()))?;
        if replaced > 0 {
            let line = format!("{}: {replaced} existing file(s) replaced", m.dest);
            job.set(|p| p.report.push(line.clone()));
        }
        if m.aside {
            aside_items.push(format!("{source_name} ({})", m.dest));
        } else {
            installed_items.push(m.dest.clone());
        }
    }
    // the linked inbox files are installed now: they leave the inbox, and what is left of
    // it goes to Mods/installed
    for original in std::mem::take(&mut *job.linked.lock().unwrap()) {
        let _ = std::fs::remove_file(&original);
    }
    if job.from_inbox {
        let done = content.join("Mods").join("installed");
        let _ = std::fs::create_dir_all(&done);
        let target = done.join(src.file_name().unwrap_or_default());
        let _ = std::fs::remove_dir_all(&target);
        let _ = std::fs::remove_file(&target);
        if std::fs::rename(&src, &target).is_err() {
            if src.is_dir() {
                let _ = std::fs::remove_dir_all(&src);
            }
        } else {
            note_installed(content, &src.file_name().unwrap_or_default().to_string_lossy(), &created);
        }
    }
    // (noted as one mod: its own folders, see `mods`)
    crate::mods::record(content, &source_name, &created);
    let summary = match (installed_items.is_empty(), aside_items.is_empty()) {
        (false, true) => trf("installed {items} - it is in the lists now", &[("items", &installed_items.join(", "))]),
        (false, false) => trf("installed {items}; kept aside: {aside}", &[("items", &installed_items.join(", ")), ("aside", &aside_items.join(", "))]),
        (true, _) => trf("nothing installed; kept aside: {aside}", &[("aside", &aside_items.join(", "))]),
    };
    job.set(|p| {
        p.installed = installed_items.clone();
        p.kept_aside = aside_items.clone();
        p.message = summary.clone();
        p.report.push(summary.clone());
    });
    Ok(())
}

/// Where a mod moved to `Mods/installed` keeps the list of the folders its install made
/// (`<name>.txt`, one content-relative path a line).
pub const RECORDS: &str = ".installed-records";
/// Where the folders of a mod taken out of `Mods/installed` go (nothing is deleted: moved
/// back into the content folder, or the mod into `Mods`, it is there again).
pub const UNINSTALLED: &str = "uninstalled";

/// Note the folders the install of `name` made (with those of an earlier install of it that
/// are still there).
fn note_installed(content: &Path, name: &str, created: &[String]) {
    let dir = content.join("Mods").join(RECORDS);
    let file = dir.join(format!("{name}.txt"));
    let mut all: Vec<String> = std::fs::read_to_string(&file).map(|t| t.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty() && content.join(l).exists()).collect()).unwrap_or_default();
    for c in created {
        if !all.contains(c) {
            all.push(c.clone());
        }
    }
    if all.is_empty() {
        let _ = std::fs::remove_file(&file);
        return;
    }
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&file, all.join("\n") + "\n");
}

/// A mod the player deleted from `Mods/installed` is uninstalled (#819): the folders its
/// install made (a bus's own folder) are moved into `Mods/uninstalled/<name>`, so the
/// lists no longer show it - and nothing is lost. Folders a mod only added to (an existing
/// bus, the shared `Sceneryobjects`) stay. Returns the mods uninstalled.
pub fn uninstall_removed(content: &Path) -> Vec<String> {
    // (not while an install runs: a mod installed again leaves Mods/installed for a moment)
    if !content.join("Mods").join(RECORDS).is_dir() || jobs().iter().any(|j| j.finished.is_none()) {
        return Vec::new();
    }
    uninstall_removed_now(content)
}

fn uninstall_removed_now(content: &Path) -> Vec<String> {
    let records = content.join("Mods").join(RECORDS);
    let Ok(rd) = std::fs::read_dir(&records) else { return Vec::new() };
    let mut done = Vec::new();
    for e in rd.flatten() {
        let file = e.path();
        let Some(name) = file.file_name().map(|n| n.to_string_lossy().to_string()).and_then(|n| n.strip_suffix(".txt").map(str::to_string)) else { continue };
        let installed = content.join("Mods").join("installed").join(&name);
        if installed.exists() || installed.is_symlink() {
            continue;
        }
        let list: Vec<String> = std::fs::read_to_string(&file).unwrap_or_default().lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
        let mut left = Vec::new();
        for rel in list {
            // (only plain paths inside the content folder, never Mods itself)
            if check_rel(&rel).is_err() || rel.to_ascii_lowercase().starts_with("mods/") {
                continue;
            }
            let from = content.join(&rel);
            if !from.exists() {
                continue;
            }
            let to = content.join("Mods").join(UNINSTALLED).join(&name).join(&rel);
            if to.exists() {
                let _ = std::fs::remove_dir_all(&to);
            }
            let moved = to.parent().map(|p| std::fs::create_dir_all(p).is_ok()).unwrap_or(false) && std::fs::rename(&from, &to).is_ok();
            if !moved {
                left.push(rel);
            }
        }
        if left.is_empty() {
            let _ = std::fs::remove_file(&file);
            done.push(name);
        } else {
            let _ = std::fs::write(&file, left.join("\n") + "\n");
        }
    }
    done
}

/// Whether the game finds what `plan` says is in the zip at `src` when the archive is
/// mounted as it is: the mount starts at the archive's folder holding OMSI's content
/// folders, so every part of the plan must lie right there (a lone bus folder, which the
/// plan puts under `Vehicles`, must be unpacked).
fn in_place_layout(src: &Path, plan: &Plan) -> Result<()> {
    let archive = omsi_cfg::vfs::ZipArchive::open(src).with_context(|| format!("reading {}", src.display()))?;
    let prefix = archive.prefix().trim_end_matches('/').to_lowercase();
    let mut wrong: Vec<String> = Vec::new();
    for m in plan.maps.iter().filter(|m| !m.aside) {
        let want = if prefix.is_empty() { m.dest.to_lowercase() } else { format!("{prefix}/{}", m.dest.to_lowercase()) };
        if m.src.to_lowercase() != want {
            wrong.push(trf("{item} (the game would look for it at {path})", &[("item", &if m.src.is_empty() { tr("the archive itself") } else { m.src.clone() }), ("path", &want)]));
        }
    }
    if wrong.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(trf("the archive is not laid out like an OMSI 2 folder, so it has to be unpacked: {why}", &[("why", &wrong.join("; "))])))
    }
}

/// Step 3/4 for an archive used in place: into `<content>/Archives/<name>.zip`, linked,
/// moved (from the inbox) or copied through the staging folder, and mounted for the lists.
fn place_archive(job: &Job, content: &Path, src: &Path, plan: &Plan) -> Result<()> {
    let name = src.file_name().map(|n| n.to_string_lossy().to_string()).filter(|n| safe_rel(n).as_deref() == Some(n.as_str()) && !n.contains('/')).ok_or_else(|| anyhow!("{} has a name that cannot be used", src.display()))?;
    let archives = content.join(ARCHIVES);
    std::fs::create_dir_all(&archives).with_context(|| format!("creating {}", archives.display()))?;
    let dest = archives.join(&name);
    let size = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    job.set(|p| {
        p.files_total = 1;
        p.bytes_total = size;
    });
    let same = |a: &Path, b: &Path| -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if let (Ok(x), Ok(y)) = (std::fs::metadata(a), std::fs::metadata(b)) {
                return x.dev() == y.dev() && x.ino() == y.ino();
            }
            false
        }
        #[cfg(not(unix))]
        {
            std::fs::canonicalize(a).ok() == std::fs::canonicalize(b).ok() && a.exists()
        }
    };
    let how;
    if dest.exists() && same(src, &dest) {
        how = tr("already in place");
    } else if job.from_inbox && std::fs::rename(src, &dest).is_ok() {
        how = tr("moved from the Mods folder");
    } else {
        // through the staging folder, so that the game never mounts half an archive
        let staging = staging_dir(content, job.id);
        std::fs::create_dir_all(&staging).with_context(|| format!("creating {}", staging.display()))?;
        let tmp = staging.join(&name);
        if std::fs::hard_link(src, &tmp).is_ok() {
            how = tr("linked: no disk space used, the archive also stays where it was");
        } else {
            let needed = size + (size / 20).max(MIN_MARGIN);
            let free = free_space(content).unwrap_or(u64::MAX);
            job.set(|p| {
                p.free_bytes = free;
                p.needed_bytes = needed;
            });
            if needed > free {
                return Err(anyhow!(trf("not enough disk space to copy {name} ({size}) into {folder}: with {margin} kept free that needs {needed}, but only {free} is free. Nothing was copied.", &[("name", &name), ("size", &gb(size)), ("folder", &archives.display()), ("margin", &gb(needed - size)), ("needed", &gb(needed)), ("free", &gb(free))])));
            }
            job.state("copying", trf("copying the archive ({size})", &[("size", &gb(size))]));
            let mut r = std::fs::File::open(src).with_context(|| format!("reading {}", src.display()))?;
            let mut w = std::fs::File::create(&tmp).with_context(|| format!("writing {}", tmp.display()))?;
            let mut buf = vec![0u8; 1 << 20];
            let mut since_check = 0u64;
            loop {
                let n = r.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                w.write_all(&buf[..n]).with_context(|| format!("writing {}", tmp.display()))?;
                job.bytes_done.fetch_add(n as u64, Ordering::Relaxed);
                since_check += n as u64;
                if since_check > 64 << 20 {
                    since_check = 0;
                    if job.cancelled() {
                        return Err(anyhow!(Cancelled));
                    }
                    check_space(content)?;
                }
            }
            w.flush()?;
            how = tr("copied");
        }
        if job.cancelled() {
            return Err(anyhow!(Cancelled));
        }
        job.state("moving", tr("putting the archive into place"));
        std::fs::rename(&tmp, &dest).with_context(|| format!("moving into {}", dest.display()))?;
        if job.from_inbox {
            // the inbox copy is in place now (it was linked or copied)
            let done = content.join("Mods").join("installed");
            let _ = std::fs::create_dir_all(&done);
            let _ = std::fs::remove_file(done.join(&name));
            if std::fs::rename(src, done.join(&name)).is_err() {
                let _ = std::fs::remove_file(src);
            }
        }
    }
    job.bytes_done.store(size, Ordering::Relaxed);
    job.files_done.store(1, Ordering::Relaxed);
    if !cfg!(test) {
        crate::mount_archive(&dest);
    }
    crate::mods::record(content, &name, &[format!("{ARCHIVES}/{name}")]);
    let items: Vec<String> = plan.maps.iter().filter(|m| !m.aside).map(|m| m.dest.clone()).collect();
    let summary = if items.is_empty() { "{name} used in place ({how}) as {path} - its content is in the lists now; the game reads it without unpacking" } else { "{name} used in place ({how}) as {path} - {items} are in the lists now; the game reads it without unpacking" };
    let summary = trf(summary, &[("name", &name), ("how", &how), ("path", &format!("{ARCHIVES}/{name}")), ("items", &items.join(", "))]);
    job.set(|p| {
        p.installed = items.iter().map(|i| format!("{i} (in {ARCHIVES}/{name})")).collect();
        p.message = summary.clone();
        p.report.push(summary.clone());
    });
    Ok(())
}

/// What the page shows before an install: how big a source unpacks, what is free, and
/// whether it could be used in place.
#[derive(Debug, Clone, Serialize, Default)]
pub struct SourceInfo {
    pub is_archive: bool,
    pub is_zip: bool,
    pub files: u64,
    pub unpacked_bytes: u64,
    pub archive_bytes: u64,
    pub needed_bytes: u64,
    pub free_bytes: u64,
    pub fits: bool,
    /// Why it cannot be used in place (empty when it can).
    pub in_place: String,
    pub in_place_ok: bool,
    /// The mode `auto` will take.
    pub suggested: InstallMode,
}

/// Look at a source without installing it.
pub fn inspect(content: &Path, root: Option<&Path>, src: &Path) -> Result<SourceInfo> {
    let archive = archive_kind(src);
    let is_zip = archive == Some(ArchiveKind::Zip);
    let free = free_space(content).unwrap_or(u64::MAX);
    if archive.is_none() {
        return Ok(SourceInfo { free_bytes: free, fits: true, in_place: tr("only a .zip archive can be used in place"), suggested: InstallMode::Extract, ..Default::default() });
    }
    let job = Job { id: 0, source: src.to_path_buf(), mode: InstallMode::Auto, from_inbox: false, cancel: AtomicBool::new(false), progress: Mutex::new(Progress::default()), bytes_done: AtomicU64::new(0), files_done: AtomicU64::new(0), linked: Mutex::new(Vec::new()), stall_at: u64::MAX };
    let archive_bytes = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    if !is_zip {
        let (files, unpacked) = match archive.unwrap() {
            ArchiveKind::SevenZip => {
                let a = sevenz_rust2::ArchiveReader::open(src, sevenz_rust2::Password::empty()).with_context(|| format!("{} is not a readable 7z archive", src.display()))?;
                let entries = &a.archive().files;
                (entries.iter().filter(|e| e.has_stream && !e.is_directory).count() as u64,
                    entries.iter().filter(|e| e.has_stream && !e.is_directory).fold(0u64, |n, e| n.saturating_add(e.size)))
            }
            ArchiveKind::Rar => {
                let a = unrar_rs::RarArchive::open(std::fs::File::open(src)?)
                    .with_context(|| format!("{} is not a readable RAR archive", src.display()))?;
                let members: Vec<_> = a.entries().collect();
                let regular = members.iter().filter(|m| !m.is_directory && !m.is_symlink && !m.is_hardlink);
                let entries: Vec<_> = regular.collect();
                if entries.iter().any(|m| m.unpacked_size.is_none()) {
                    return Err(anyhow!("multi-volume RAR archive; add all parts and start with the first .rar file"));
                }
                (entries.len() as u64, entries.iter().fold(0u64, |n, m| n.saturating_add(m.unpacked_size.unwrap_or(0))))
            }
            ArchiveKind::Zip => unreachable!(),
        };
        let needed = unpacked.saturating_add(MIN_MARGIN);
        return Ok(SourceInfo { is_archive: true, is_zip: false, files, unpacked_bytes: unpacked, archive_bytes, needed_bytes: needed, free_bytes: free, fits: needed <= free, in_place: "7z and RAR archives must be unpacked".into(), in_place_ok: false, suggested: InstallMode::Extract });
    }
    let f = std::fs::File::open(src).with_context(|| format!("opening {}", src.display()))?;
    let mut z = zip::ZipArchive::new(std::io::BufReader::with_capacity(ZIP_BUFFER, f)).with_context(|| trf("{name} is not a readable zip archive", &[("name", &src.display())]))?;
    let (entries, _) = list_zip(&mut z, &job)?;
    let source_name = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "mod".into());
    let plan = plan(&entries, &source_name, &Installed { content, root });
    let files: u64 = plan.maps.iter().map(|m| m.files.len() as u64).sum();
    let unpacked: u64 = plan.maps.iter().map(|m| m.bytes).sum();
    let needed = unpacked + (unpacked / 20).max(MIN_MARGIN);
    let layout = if plan.maps.is_empty() { Err(anyhow!(tr("nothing in it the game would use"))) } else { in_place_layout(src, &plan) };
    let fits = needed <= free;
    let in_place_ok = layout.is_ok();
    Ok(SourceInfo { is_archive: true, is_zip, files, unpacked_bytes: unpacked, archive_bytes, needed_bytes: needed, free_bytes: free, fits, in_place: layout.err().map(|e| format!("{e:#}")).unwrap_or_default(), in_place_ok, suggested: if !fits && in_place_ok { InstallMode::InPlace } else { InstallMode::Extract } })
}

fn check_space(content: &Path) -> Result<()> {
    if let Some(free) = free_space(content) {
        if free < ABORT_BELOW {
            return Err(anyhow!(trf("the disk is almost full ({free} free) - stopped unpacking and removed the partial files", &[("free", &gb(free))])));
        }
    }
    Ok(())
}

/// `rel` under `base`, taking the spelling of folders that exist already (a mod's
/// `vehicles/foo/texture` goes into `Vehicles/Foo/Texture` on a case-sensitive disk).
fn case_path(base: &Path, rel: &str) -> PathBuf {
    let mut cur = base.to_path_buf();
    for comp in rel.split('/').filter(|c| !c.is_empty()) {
        let direct = cur.join(comp);
        if direct.exists() {
            cur = direct;
            continue;
        }
        let found = std::fs::read_dir(&cur).ok().and_then(|rd| rd.flatten().map(|e| e.file_name()).find(|n| n.to_string_lossy().eq_ignore_ascii_case(comp)));
        cur = match found {
            Some(n) => cur.join(n),
            None => direct,
        };
    }
    cur
}

/// Move the tree `from` to `to`: in one rename when `to` does not exist, else merged file
/// by file (files replace files of the same name). Returns how many files were replaced.
fn move_into(from: &Path, to: &Path) -> Result<usize> {
    if !to.exists() {
        if let Some(p) = to.parent() {
            std::fs::create_dir_all(p)?;
        }
        if std::fs::rename(from, to).is_ok() {
            return Ok(0);
        }
    }
    std::fs::create_dir_all(to)?;
    let mut replaced = 0;
    for e in std::fs::read_dir(from)?.flatten() {
        let name = e.file_name();
        let src = e.path();
        let dest = case_path(to, &name.to_string_lossy());
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if dest.exists() && !dest.is_dir() {
                std::fs::remove_file(&dest)?;
            }
            replaced += move_into(&src, &dest)?;
        } else {
            if dest.is_dir() {
                // (a file of a mod never takes the place of a whole installed folder)
                return Err(anyhow!(trf("{name} is a folder; the mod's file of that name was not installed over it", &[("name", &dest.display())])));
            } else if dest.exists() {
                replaced += 1;
            }
            if std::fs::rename(&src, &dest).is_err() {
                // a new file, never written through whatever `dest` was (a link)
                let _ = std::fs::remove_file(&dest);
                std::fs::copy(&src, &dest).with_context(|| format!("copying to {}", dest.display()))?;
            }
        }
    }
    Ok(replaced)
}

/// Packs in `Mods/waiting` whose buses are installed now.
pub fn waiting_ready(content: &Path, root: Option<&Path>) -> Vec<PathBuf> {
    let installed = Installed { content, root };
    let Ok(rd) = std::fs::read_dir(content.join("Mods").join(WAITING)) else { return Vec::new() };
    rd.flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|p| {
            let buses: Vec<String> = std::fs::read_dir(p.join("Vehicles")).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default();
            !buses.is_empty() && buses.iter().all(|b| installed.vehicle(b).is_some())
        })
        .collect()
}


#[cfg(test)]
mod tests;
