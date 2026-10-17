//! The host's mods for the players who join: whatever of the host's session is not stock
//! OMSI 2 content - its map, the bus it drives, the objects, splines, AI vehicles and people
//! the map uses, and the mod fonts - goes to every player who joins, for the session only.
//!
//! The host serves the list of those files and the files themselves over TCP, on the same
//! port number as its LAN session (UDP). It serves nothing but the files of that list, by
//! their number in it: no path a client names is ever opened. A joining game asks for the
//! list before its world is loaded, fetches what it does not have in the same version
//! (compared by SHA-256), checks every file against the list's size and hash, and mounts
//! them for this session at `~/.openomsi/lan-mods/<pid>`, which becomes the first content
//! root. The mount goes when the session ends (and a folder an older version left there
//! goes at the next start).
//!
//! Not to be copied: the connection is encrypted (`channel`: a key agreement of its own,
//! bound to the session, then authenticated frames - an older game is told to update), and
//! nothing the host sends is ever written to the disk in plain form. The files are kept
//! sealed in the store (`store`, `~/.openomsi/lan-store`, so the next join need not fetch
//! them again), named by a keyed hash, and the session's mount is a sealed store of
//! `omsi_cfg::vfs`: the engine reads them decrypted into memory under their own names,
//! while a copy of the game's folders gives nothing but noise. Nothing the engine derives
//! from them is cached on the disk either (`omsi_cfg::vfs::is_sealed`). What this does not
//! do, honestly: stop a determined player. openOMSI is open source and the decryption runs
//! on the player's machine, with the store's key beside it (see `store`) - a changed build,
//! or a dump of the game's memory, has the files in plain form. And the host's own copy is
//! the host's mod folder, as it always was. It keeps the mods from being copied out of the
//! game's folders or read off the network, no more.
//!
//! What may come: files under the content folders a map or a vehicle lives in (maps,
//! Vehicles, Sceneryobjects, Splines, Humans, Fonts, TicketPacks, Money, Weather, Texture,
//! Sound, Sounds), with plain relative names, and nothing that runs code: executables,
//! libraries, scripts of the operating system and OMSI plugins are refused by name, and by
//! content (a Windows, Linux or macOS executable or a script with `#!`, whatever it is
//! called). Each file must also be what its name says it is: a picture of a size a texture
//! can have, a sound, a mesh the reader takes, a text (`quarantine`). OMSI's own vehicle
//! scripts are data for the game's own interpreter, which touches nothing outside the
//! vehicle, and a run of one is cut short when it would not end (`omsi_script`'s budget).
//! No plugin is ever loaded from the session folder (`omsi_cfg::mark_sandbox`).

mod budget;
mod bus_files;
mod channel;
pub(crate) mod failure;
pub mod identity;
mod paced;
mod pieces;
pub mod placeholder;
mod quarantine;
mod store;
pub mod upload;
pub mod vehicles;

use channel::{SecureReader, SecureWriter};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The most one file may have (a map tile or a texture is a few MB; a sound a few tens).
const MAX_FILE: u64 = 1 << 30;
/// The most a whole session may bring.
const MAX_TOTAL: u64 = 40 << 30;
/// Room left free on the disk after the download.
const KEEP_FREE: u64 = 3 << 30;
/// The most files a list may name.
const MAX_FILES: usize = 400_000;

/// The top-level content folders a session may bring files into.
const FOLDERS: &[&str] = &["maps", "vehicles", "sceneryobjects", "splines", "humans", "fonts", "ticketpacks", "money", "weather", "texture", "sound", "sounds", "trains", "announcements", "scripts"];

/// Names that run code somewhere (Windows, macOS, Linux, the JVM, Office macros, OMSI
/// plugins and their configuration).
const REFUSED: &[&str] = &[
    "exe", "dll", "com", "bat", "cmd", "scr", "ps1", "psm1", "psd1", "vbs", "vbe", "js", "jse", "wsf", "wsh", "msi", "msp", "mst", "jar", "hta", "cpl", "sys", "drv", "ocx", "ax", "lnk", "url", "reg", "inf", "sh", "bash", "zsh", "csh", "ksh", "command", "tool", "app", "dylib", "so", "py", "pyc", "pyw", "pl", "rb", "php", "lua", "apk", "run", "pif", "gadget", "appx", "msix", "iso", "img", "dmg", "pkg", "vhd", "vhdx", "opl", "docm", "xlsm", "pptm", "dotm", "xlam", "scpt", "applescript", "workflow", "action", "zip", "rar", "7z", "cab", "tar", "gz",
];

/// One file of the list: path relative to a content root (`/`-separated, as the host
/// spells it), its size and SHA-256 (hex).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    /// The host's map (`maps/<name>/global.cfg`).
    pub map: String,
    pub bus: String,
    pub entries: Vec<Entry>,
}

// -------------------------------------------------------------------------------------
// checks (both sides)

/// Is `path` something a session may bring? Returns the reason when not.
pub fn refuse_path(path: &str) -> Option<String> {
    if path.is_empty() || path.len() > 400 {
        return Some("empty or too long".into());
    }
    if path.starts_with('/') || path.starts_with('\\') || path.contains(':') || path.contains('\\') {
        return Some("not a plain relative name".into());
    }
    let comps: Vec<&str> = path.split('/').collect();
    if comps.len() < 2 {
        return Some("not inside a content folder".into());
    }
    for c in &comps {
        if c.is_empty() || *c == "." || *c == ".." || c.chars().any(|ch| ch.is_control() || matches!(ch, '<' | '>' | '"' | '|' | '?' | '*')) || c.trim() != *c && c.trim().is_empty() {
            return Some(format!("bad name component {c:?}"));
        }
    }
    let top = comps[0].to_ascii_lowercase();
    if !FOLDERS.contains(&top.as_str()) {
        return Some(format!("{} is no content folder a session brings", comps[0]));
    }
    if comps.iter().any(|c| c.eq_ignore_ascii_case("plugins")) {
        return Some("plugins are never taken from another machine".into());
    }
    let name = comps.last().unwrap().to_ascii_lowercase();
    if let Some((_, ext)) = name.rsplit_once('.') {
        if REFUSED.contains(&ext) {
            return Some(format!(".{ext} files are never taken from another machine"));
        }
    }
    None
}

/// Does the start of a file show a program (whatever its name says)?
pub fn looks_executable(head: &[u8]) -> bool {
    head.starts_with(b"MZ")
        || head.starts_with(b"\x7fELF")
        || head.starts_with(b"#!")
        || head.starts_with(&[0xfe, 0xed, 0xfa, 0xce])
        || head.starts_with(&[0xfe, 0xed, 0xfa, 0xcf])
        || head.starts_with(&[0xce, 0xfa, 0xed, 0xfe])
        || head.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
        || head.starts_with(&[0xca, 0xfe, 0xba, 0xbe])
        || head.starts_with(b"PK\x03\x04")
}

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_of(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

// -------------------------------------------------------------------------------------
// the host

/// Is content root `r` the OMSI 2 installation itself (its files are taken to be there on
/// every machine)? A content root inside it is not: openOMSI unpacked
/// into the OMSI 2 folder keeps what it installs in `<OMSI 2>/openOMSI`, and those mods
/// were never passed on.
fn is_original(r: &Path, original: &Path) -> bool {
    r == original
}

/// What of the host's game decides the files it shares (from its command line): the
/// content root, the map, the bus and the weather.
#[derive(Debug, Clone, Default)]
pub struct ModSource {
    pub root: PathBuf,
    pub map: String,
    pub bus: Option<String>,
    pub weather: Option<String>,
}

/// The files the host's session uses that are not stock content, with where each is read
/// from (a folder or a mounted archive).
fn collect(args: &ModSource) -> (Manifest, Vec<PathBuf>) {
    let t0 = Instant::now();
    let (list, _) = session_files(&args.map, args.bus.as_deref(), args.weather.as_deref(), Some(&args.root));
    // (read in parallel, and the hashes remembered by size and time: a map of thousands of
    // files - put on a server through a slow shared folder - is read once, not at each start)
    let entries = upload::hashed(&list);
    let by_rel: HashMap<&str, &PathBuf> = list.iter().map(|(r, p)| (r.as_str(), p)).collect();
    let sources: Vec<PathBuf> = entries.iter().filter_map(|e| by_rel.get(e.path.as_str()).map(|p| (*p).clone())).collect();
    let total: u64 = entries.iter().map(|e| e.size).sum();
    log::info!(
        "LAN mods: {} files ({:.1} MB) of this session are not stock content and go to joining players (listed in {:.1} s)",
        entries.len(),
        total as f64 / 1e6,
        t0.elapsed().as_secs_f64()
    );
    (
        Manifest { map: args.map.replace('\\', "/"), bus: args.bus.clone().unwrap_or_default().replace('\\', "/"), entries },
        sources,
    )
}

/// The files a session on `map` (driving `bus`, in `weather`) uses - the map's folder, the
/// objects, splines, vehicles and people its files name and the folders those name in turn,
/// its ground pictures, the fonts its displays write with - as (relative path, where it is
/// read from), and what its files name that is nowhere here. Those of the content root
/// `original` (the OMSI 2 installation, found on every machine) are left out; with none,
/// everything the map needs is in (`map_pack`: a server without this installation).
pub fn session_files(map: &str, bus: Option<&str>, weather: Option<&str>, original: Option<&Path>) -> (Vec<(String, PathBuf)>, Vec<String>) {
    let original = original.map(Path::to_path_buf);
    let mut missing: Vec<String> = Vec::new();
    // (lower-case relative path) -> (spelling, source)
    let mut files: HashMap<String, (String, PathBuf)> = HashMap::new();
    let mut folders_done: HashSet<String> = HashSet::new();
    let mut text_todo: Vec<(String, PathBuf)> = Vec::new();
    let norm = |p: &str| -> String {
        let mut parts: Vec<String> = Vec::new();
        for c in p.split(['/', '\\']) {
            match c.trim() {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                c => parts.push(c.to_string()),
            }
        }
        parts.join("/")
    };
    // a folder (relative) and everything in it, from the content roots that are not the
    // original installation (a mod's copy of a stock folder brings only what it adds)
    fn add_folder(rel: &str, original: Option<&Path>, files: &mut HashMap<String, (String, PathBuf)>, text_todo: &mut Vec<(String, PathBuf)>, depth: usize) {
        if depth > 12 {
            return;
        }
        let roots = omsi_cfg::content_roots();
        let comps = omsi_cfg::windows_components(rel);
        let mut seen: HashSet<String> = HashSet::new();
        for r in roots {
            if original.is_some_and(|o| is_original(&r, o)) {
                continue;
            }
            let dir = comps.iter().fold(r.clone(), |p, c| p.join(c));
            let Some(list) = omsi_cfg::vfs::list_dir(&dir).or_else(|| {
                // (case-insensitively, as Windows would find it)
                omsi_cfg::find_in_roots(rel).filter(|(root, _)| *root == r).and_then(|(_, p)| omsi_cfg::vfs::list_dir(&p))
            }) else {
                continue;
            };
            for (name, is_dir) in list {
                let name = name.to_string_lossy().to_string();
                if !seen.insert(name.to_lowercase()) {
                    continue;
                }
                let child = format!("{rel}/{name}");
                if is_dir {
                    add_folder(&child, original, files, text_todo, depth + 1);
                } else {
                    let key = child.to_lowercase();
                    if files.contains_key(&key) {
                        continue;
                    }
                    if let Some((root, path)) = omsi_cfg::find_in_roots(&child) {
                        if original.is_some_and(|o| is_original(&root, o)) {
                            continue;
                        }
                        let lower = name.to_lowercase();
                        if [".cfg", ".sco", ".sli", ".bus", ".ovh", ".zug", ".hum", ".txt", ".hof"].iter().any(|e| lower.ends_with(e)) {
                            text_todo.push((child.clone(), path.clone()));
                        }
                        files.insert(key, (child, path));
                    }
                }
            }
        }
    }
    // the folder an object, spline, vehicle or person file lives in
    let owner_folder = |rel: &str| -> Option<String> {
        let comps: Vec<&str> = rel.split('/').collect();
        if comps.len() < 3 {
            return None;
        }
        let top = comps[0].to_ascii_lowercase();
        // a vehicle is its whole folder (Vehicles/<name>); an object, spline or person the
        // folder it is in
        if top == "vehicles" || top == "trains" {
            return Some(comps[..2].join("/"));
        }
        Some(comps[..comps.len() - 1].join("/"))
    };
    let mut want_folder = |rel: String, files: &mut HashMap<String, (String, PathBuf)>, text_todo: &mut Vec<(String, PathBuf)>| {
        if folders_done.insert(rel.to_lowercase()) {
            add_folder(&rel, original.as_deref(), files, text_todo, 0);
        }
    };
    let map_dir = norm(Path::new(map).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default().as_str());
    if !map_dir.is_empty() {
        want_folder(map_dir.clone(), &mut files, &mut text_todo);
    }
    if let Some(b) = bus {
        if let Some(f) = owner_folder(&norm(b)) {
            want_folder(f, &mut files, &mut text_todo);
        }
    }
    if let Some(w) = weather {
        let w = norm(w);
        if let Some((root, path)) = omsi_cfg::find_in_roots(&w) {
            if original.as_ref() != Some(&root) {
                files.insert(w.to_lowercase(), (w, path));
            }
        }
    }

    // the map's tiles and lists, and every text file of what they bring: the objects,
    // splines, vehicles and people they name, with the folders those name in turn
    let mut map_texts: Vec<(String, PathBuf)> = Vec::new();
    for r in omsi_cfg::content_roots() {
        let comps = omsi_cfg::windows_components(&map_dir);
        let dir = comps.iter().fold(r.clone(), |p, c| p.join(c));
        for (name, is_dir) in omsi_cfg::vfs::list_dir(&dir).unwrap_or_default() {
            let n = name.to_string_lossy().to_string();
            let l = n.to_lowercase();
            if !is_dir && (l.ends_with(".map") || l.ends_with(".cfg") || l.ends_with(".txt")) {
                map_texts.push((format!("{map_dir}/{n}"), dir.join(&n)));
            }
        }
    }
    let mut scanned: HashSet<String> = HashSet::new();
    let mut fonts_used: HashSet<String> = HashSet::new();
    let mut queue: Vec<(String, PathBuf)> = map_texts;
    queue.append(&mut text_todo);
    let mut rounds = 0;
    while !queue.is_empty() && rounds < 6 {
        rounds += 1;
        let mut next: Vec<(String, PathBuf)> = Vec::new();
        for (rel, path) in std::mem::take(&mut queue) {
            if !scanned.insert(rel.to_lowercase()) {
                continue;
            }
            let Ok(text) = omsi_cfg::vfs::read_text(&path) else { continue };
            let here = rel.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
            // the fonts its text textures write with (`[texttexture]`: variable, font, …)
            let lines: Vec<&str> = text.lines().collect();
            for (i, l) in lines.iter().enumerate() {
                if l.trim().to_ascii_lowercase().starts_with("[texttexture") {
                    if let Some(f) = lines.get(i + 2) {
                        fonts_used.insert(f.trim().to_lowercase());
                    }
                }
            }
            for line in text.lines() {
                let t = line.trim();
                if t.len() < 5 || t.len() > 300 || t.starts_with('[') {
                    continue;
                }
                let lower = t.to_ascii_lowercase();
                let is_ref = [".sco", ".sli", ".bus", ".ovh", ".zug", ".hum", ".owt"].iter().any(|e| lower.ends_with(e));
                let relative = t.contains("..");
                // a picture named from the root: a map's ground textures (`[groundtex]` of its
                // global.cfg: `Texture\<map>\gras.jpg`) - without them its ground was white
                let picture = [".bmp", ".jpg", ".jpeg", ".dds", ".tga", ".png"].iter().any(|e| lower.ends_with(e));
                // a script named from the root (`Scripts\AI_Cars\x.osc` of a parked car):
                // its folder, with the variable lists beside it - else the server's traffic
                // ran without it
                let script = lower.ends_with(".osc") || (lower.starts_with("scripts") && lower.ends_with(".txt"));
                if !is_ref && !relative && !picture && !script {
                    continue;
                }
                // a path from the root ("Sceneryobjects\\...") or from this file's folder
                let candidates = [norm(t), norm(&format!("{here}/{t}"))];
                let mut found = false;
                for c in candidates {
                    if c.split('/').count() < 2 {
                        continue;
                    }
                    let top = c.split('/').next().unwrap_or("").to_ascii_lowercase();
                    if !FOLDERS.contains(&top.as_str()) {
                        continue;
                    }
                    let Some((root, _)) = omsi_cfg::find_in_roots(&c) else { continue };
                    found = true;
                    if original.as_ref().is_some_and(|o| is_original(&root, o)) {
                        break;
                    }
                    let folder = if is_ref { owner_folder(&c) } else { c.rsplit_once('/').map(|(d, _)| d.to_string()) };
                    match folder.filter(|f| f.split('/').count() >= 2) {
                        Some(f) => want_folder(f, &mut files, &mut next),
                        // (one right in a content folder, `Texture\x.bmp`: that file alone)
                        None if picture || script => {
                            if let Some((_, path)) = omsi_cfg::find_in_roots(&c) {
                                files.entry(c.to_lowercase()).or_insert((c.clone(), path));
                            }
                        }
                        None => {}
                    }
                    break;
                }
                // (a name that is a path into a content folder, found nowhere: missing here)
                if !found && (is_ref || picture || script) && FOLDERS.contains(&norm(t).split('/').next().unwrap_or("").to_ascii_lowercase().as_str()) {
                    missing.push(format!("{} (named in {rel})", norm(t)));
                }
            }
        }
        queue = next;
    }
    // the mod fonts those text textures use (OMSI reads the `.oft` files of `Fonts` itself,
    // not its sub-folders): each `.oft` with a `[newfont]` of a name in use, and its bitmaps
    for r in omsi_cfg::content_roots() {
        if original.as_ref().is_some_and(|o| is_original(&r, o)) {
            continue;
        }
        let Some(dir) = omsi_cfg::find_in_roots("Fonts").filter(|(root, _)| *root == r).map(|(_, p)| p).or_else(|| Some(r.join("Fonts"))) else { continue };
        for (name, is_dir) in omsi_cfg::vfs::list_dir(&dir).unwrap_or_default() {
            let n = name.to_string_lossy().to_string();
            if is_dir || !n.to_lowercase().ends_with(".oft") {
                continue;
            }
            let Ok(text) = omsi_cfg::vfs::read_text(&dir.join(&n)) else { continue };
            let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
            let mut wanted = false;
            let mut bitmaps: Vec<String> = Vec::new();
            for (i, l) in lines.iter().enumerate() {
                if l.eq_ignore_ascii_case("[newfont]") {
                    let name = lines.get(i + 1).map(|x| x.to_lowercase()).unwrap_or_default();
                    if fonts_used.contains(&name) {
                        wanted = true;
                        bitmaps.extend(lines.iter().skip(i + 2).take(2).map(|b| b.to_string()));
                    }
                }
            }
            if !wanted {
                continue;
            }
            for f in std::iter::once(n.clone()).chain(bitmaps) {
                let rel = format!("Fonts/{f}");
                if let Some((root, path)) = omsi_cfg::find_in_roots(&rel) {
                    if !original.as_ref().is_some_and(|o| is_original(&root, o)) {
                        files.insert(rel.to_lowercase(), (rel, path));
                    }
                }
            }
        }
    }
    let mut list: Vec<(String, PathBuf)> = files.into_values().filter(|(rel, _)| refuse_path(rel).is_none()).collect();
    list.sort_by(|a, b| a.0.cmp(&b.0));
    missing.sort();
    missing.dedup();
    (list, missing)
}

/// Whether the map goes to players who join without it (`share_maps` of a server; a game
/// that hosts a session shares it).
static SHARE_MAPS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn share_maps(on: bool) {
    SHARE_MAPS.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Serve the session's mods on TCP `port` (a thread; the list is made in the background).
pub fn serve(port: u16, key: Arc<omsi_net::hostkey::HostKey>, args: &ModSource) {
    if omsi_cfg::flags::OMSI_NO_LAN_MODS.is_set() {
        return;
    }
    let listener = match TcpListener::bind(("0.0.0.0", port)) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("LAN mods: cannot serve on TCP port {port}: {e} (joining players need the host's mods installed)");
            return;
        }
    };
    let ready: Arc<Mutex<Option<Arc<(Manifest, Vec<PathBuf>)>>>> = Arc::new(Mutex::new(None));
    // (a server that takes its players' buses opens those sent before: they are asked for by
    // their fingerprint when their players are there, not put on everybody's list)
    upload::open();
    {
        let ready = ready.clone();
        let args = args.clone();
        std::thread::Builder::new()
            .name("lan-mods-list".into())
            .spawn(move || {
                // (no map passed on: an empty list, and no reading through it at the start)
                let m = if SHARE_MAPS.load(std::sync::atomic::Ordering::Relaxed) {
                    collect(&args)
                } else {
                    log::info!("LAN mods: the map is not passed on (share_maps = 0): players who join need {} installed", args.map);
                    (Manifest { map: args.map.replace('\\', "/"), bus: String::new(), entries: Vec::new() }, Vec::new())
                };
                *ready.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(m));
            })
            .ok();
    }
    log::info!("LAN mods: serving this session's mods on TCP port {port}");
    std::thread::Builder::new()
        .name("lan-mods".into())
        .spawn(move || {
            // (one thread per connection, so only so many at once)
            let open = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            for conn in listener.incoming() {
                let Ok(stream) = conn else { continue };
                if open.load(std::sync::atomic::Ordering::Relaxed) >= MAX_CONNECTIONS {
                    log::info!("LAN mods: {MAX_CONNECTIONS} connections open already; one more told to come back");
                    // (said, not just closed: the player's game asks again in a moment instead
                    // of taking the bus for one not to be had)
                    let mut s = stream;
                    let _ = s.write_all(b"ERR busy\n");
                    continue;
                }
                open.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let (ready, held, key) = (ready.clone(), open.clone(), key.clone());
                let spawned = std::thread::Builder::new().name("lan-mods-conn".into()).spawn(move || {
                    let peer = stream.peer_addr().ok();
                    if let Err(e) = handle(stream, &key, &ready) {
                        log::info!("LAN mods: connection from {peer:?} ended: {e}");
                    }
                    held.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                });
                if spawned.is_err() {
                    open.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
        })
        .ok();
}

/// Connections the mods server serves at once (players fetch and send buses over several
/// each; one more is answered `ERR busy` and comes back). A new bus among 16 players: the
/// 15 others each wait for its list and then fetch it over four connections, while its
/// player sends it and fetches theirs - about 90 at once, where 50 sent half of them away.
/// As many as the web gateway carries (`omsi_net::ws`), whose mod streams all end here.
const MAX_CONNECTIONS: usize = 160;

/// What an attempt that broke off on the way says (the system's own words follow, in the
/// player's language): asked again in a moment, never given up.
pub(crate) const CONNECTION_LOST: &str = "connection lost";
/// The longest request line (they are a few words).
const MAX_LINE: u64 = 256;
const MAX_REPLY: u64 = 4096;

/// One line of at most `MAX_LINE` bytes (a longer one ends the connection).
fn read_line_limited(input: &mut impl BufRead, line: &mut String) -> std::io::Result<usize> {
    read_line_within(input, line, MAX_LINE)
}

fn read_line_within(input: &mut impl BufRead, line: &mut String, max: u64) -> std::io::Result<usize> {
    let n = input.by_ref().take(max).read_line(line)?;
    if n as u64 >= max && !line.ends_with('\n') {
        return Err(std::io::Error::other("line too long"));
    }
    Ok(n)
}

fn handle(stream: TcpStream, key: &omsi_net::hostkey::HostKey, ready: &Mutex<Option<Arc<(Manifest, Vec<PathBuf>)>>>) -> std::io::Result<()> {
    let session = key.session();
    stream.set_read_timeout(Some(Duration::from_secs(120)))?;
    stream.set_nodelay(true).ok();
    let mut out = stream.try_clone()?;
    let mut input = BufReader::new(stream);
    let mut line = String::new();
    read_line_limited(&mut input, &mut line)?;
    let hello: Vec<&str> = line.split_whitespace().collect();
    // (a game of the unencrypted protocol is told why it gets nothing)
    if hello.first().is_some_and(|m| channel::is_old(m)) {
        writeln!(out, "{}", channel::OLD_PEER)?;
        return Ok(());
    }
    if hello.len() != 3 || hello[0] != channel::MAGIC || u64::from_str_radix(hello[1], 16).ok() != Some(session) {
        out.write_all(b"ERR not this session\n")?;
        return Ok(());
    }
    // the list may still be being made (hashing a big map takes a while)
    let t0 = Instant::now();
    let data = loop {
        if let Some(d) = ready.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            break d;
        }
        if t0.elapsed() > Duration::from_secs(600) {
            out.write_all(b"ERR the host has no list\n")?;
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let (manifest, sources) = (&data.0, &data.1);
    let keys = channel::Handshake::new()?;
    writeln!(out, "OK {} {}", keys.public_hex(), channel::prove(key, hello[2], &keys.public_hex()))?;
    let (send, receive) = keys.finish(session, false, hello[2])?;
    let mut out = SecureWriter::new(out, send);
    let mut input = SecureReader::new(input, receive);
    // (the players' buses on this connection: `upload::requests`)
    let mut buses = upload::BusRequests::default();
    loop {
        // (what was answered goes out before the next request is waited for)
        out.flush()?;
        line.clear();
        if read_line_limited(&mut input, &mut line)? == 0 {
            return Ok(());
        }
        let req: Vec<&str> = line.split_whitespace().collect();
        match req.as_slice() {
            ["LIST"] => {
                let body = serde_json::to_vec(manifest).unwrap_or_default();
                writeln!(out, "OK {}", body.len())?;
                out.write_all(&body)?;
            }
            ["GET", idx] => {
                let Some(k) = idx.parse::<usize>().ok().filter(|k| *k < sources.len()) else {
                    writeln!(out, "ERR no such file")?;
                    continue;
                };
                // a plain file goes as it is read (never all of it in memory); one in a
                // mounted archive is read whole (archives hold small files)
                let src = &sources[k];
                if omsi_cfg::vfs::archive_of(src).is_none() && !omsi_cfg::vfs::is_sealed(src) {
                    match std::fs::File::open(src).and_then(|f| f.metadata().map(|m| (f, m.len()))) {
                        Ok((f, len)) => {
                            writeln!(out, "OK {len}")?;
                            let sent = std::io::copy(&mut f.take(len), &mut out)?;
                            if sent != len {
                                // (the file shrank meanwhile: the stream is out of step)
                                return Err(std::io::Error::other("a file changed while it was sent"));
                            }
                        }
                        Err(e) => writeln!(out, "ERR {e}")?,
                    }
                } else {
                    match omsi_cfg::vfs::read(src) {
                        Ok(bytes) => {
                            writeln!(out, "OK {}", bytes.len())?;
                            out.write_all(&bytes)?;
                        }
                        Err(e) => writeln!(out, "ERR {e}")?,
                    }
                }
            }
            ["BYE"] | [] => return out.flush(),
            // a player's bus for the session, and the others' by their content (`upload`,
            // `vehicles`): read and checked once (`upload::requests`), then answered
            words => match upload::Request::parse(words) {
                Some(Ok(request)) => buses.serve(request, &mut input, &mut out)?,
                Some(Err(upload::Rejected::Answer(words))) => writeln!(out, "ERR {words}")?,
                Some(Err(upload::Rejected::End(why))) => return Err(std::io::Error::other(why)),
                None => writeln!(out, "ERR {}", failure::says::WHAT)?,
            },
        }
    }
}

// -------------------------------------------------------------------------------------
// the joining player

/// The session folder of this game (`~/.openomsi/lan-mods/<pid>`).
fn sandbox_base() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".openomsi").join("lan-mods"))
}

/// The host's mods of this session, mounted as a content root (process-wide like the
/// content roots; taken away by `clean_up` when the session ends).
static SANDBOX: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Remove the session folders of games that are no longer running (one that ended without
/// cleaning up).
pub fn remove_stale() {
    // (and the plain files an older version kept in the store)
    if let Some(s) = store_dir() {
        store::remove_legacy(&s);
    }
    let Some(base) = sandbox_base() else { return };
    let Ok(rd) = std::fs::read_dir(&base) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let alive = name.parse::<u32>().map(process_alive).unwrap_or(false);
        if !alive {
            let _ = std::fs::remove_dir_all(e.path());
            log::info!("LAN mods: removed the content of an old session ({name})");
        }
    }
}

fn process_alive(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

/// The session is over: its content goes.
pub fn clean_up() {
    let taken = SANDBOX.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(dir) = taken {
        omsi_cfg::remove_content_root(&dir);
        omsi_cfg::vfs::unmount_sealed(&dir);
        log::info!("LAN mods: the host's mods of this session were taken away (they stay sealed in the store)");
    }
}

/// Free room on the disk `dir` is on (Windows too).
fn free_space(dir: &Path) -> Option<u64> {
    store::free_space(dir)
}

/// What the joining game found out: to say in the HUD/launcher.
#[derive(Debug, Default)]
pub struct Report {
    pub fetched: usize,
    pub bytes: u64,
    pub had: usize,
    pub refused: Vec<String>,
    /// Files of vehicles this machine has of its own (each file it has), left as they are here.
    pub kept_own: usize,
}

/// Is `rel` a file of a vehicle (`Vehicles/<name>/...`, `Trains/<name>/...`) this machine has
/// of its own - in the original installation or its own mods, not among what a host sent?
/// The host's map may use a bus pack the player has in another version: the host's copy put
/// first (the session's files are searched before the installation) would hide the player's
/// paints (`.cti`) and textures of the same names for the whole session, the bus the player
/// drives too. The player's stays; what the player lacks of the folder (an add-on repaint, a
/// `.bus` the host's map uses) comes, and hides nothing of the player's.
fn own_vehicle_file(rel: &str, root: &Path) -> bool {
    let comps: Vec<&str> = rel.split('/').collect();
    if comps.len() < 3 || !(comps[0].eq_ignore_ascii_case("vehicles") || comps[0].eq_ignore_ascii_case("trains")) {
        return false;
    }
    let mut roots: Vec<PathBuf> = omsi_cfg::content_roots().into_iter().filter(|r| !omsi_cfg::is_sandbox(r)).collect();
    if !roots.iter().any(|r| r == root) {
        roots.push(root.to_path_buf());
    }
    roots.iter().any(|r| omsi_cfg::vfs::is_file(&omsi_cfg::resolve_path(r, rel)))
}

/// Is `rel` a script (`Scripts/...`) this machine has of its own? The host's comes only
/// where this machine has none: the session's content is searched before the installation,
/// so a host's `Scripts/AI_Cars/<stock name>.osc` ran in every AI car of every player who
/// joined, in place of the game's own.
fn own_script(rel: &str) -> bool {
    rel.split('/').next().is_some_and(|top| top.eq_ignore_ascii_case("scripts")) && omsi_cfg::find_in_roots(rel).is_some_and(|(r, _)| !omsi_cfg::is_sandbox(&r))
}

fn read_reply(input: &mut impl BufRead) -> Result<Option<u64>, String> {
    let mut line = String::new();
    // (the connection closed, the host restarted: asked again soon; a line longer than a reply ends it)
    if read_line_within(input, &mut line, MAX_REPLY).map_err(|e| format!("{CONNECTION_LOST}: {e}"))? == 0 {
        return Err(format!("{CONNECTION_LOST}: closed by the host"));
    }
    let t = line.trim();
    if let Some(rest) = t.strip_prefix("OK") {
        let rest = rest.trim();
        if rest.is_empty() {
            return Ok(None);
        }
        return rest.parse::<u64>().map(Some).map_err(|_| format!("bad reply {t:?}"));
    }
    Err(format!("the host says: {}", t.strip_prefix("ERR").unwrap_or(t).trim()))
}

/// Local files' hashes by path: (size, modification time, SHA-256).
type HashCache = std::collections::HashMap<String, (u64, u64, String)>;

fn hash_cache_path() -> Option<std::path::PathBuf> {
    sandbox_base().map(|b| b.with_file_name("lan-hash-cache.json"))
}

fn hash_cache() -> HashCache {
    hash_cache_path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_hash_cache(c: &HashCache) {
    if let Some(p) = hash_cache_path() {
        let _ = std::fs::write(p, serde_json::to_vec(c).unwrap_or_default());
    }
}

/// Fetch the host's mods (a joining player, before its world is loaded): what this machine
/// does not have in the host's version goes into the session folder, which becomes the
/// first content root; the host's map becomes ours. `progress` is told (done, total bytes).
/// A connection to the host's mods, greeted and its keys agreed: the socket (to shut it
/// down), and the encrypted halves.
type Connection = (TcpStream, SecureWriter<TcpStream>, SecureReader<BufReader<paced::Paced<TcpStream>>>);

/// A connection to the host's mods, greeted.
fn open(host: SocketAddr, session: u64) -> Result<Connection, String> {
    open_waiting(host, session, Duration::from_secs(25))
}

/// A connection to the host's mods, greeted, whose answers are waited for up to `wait` (on
/// the handle that reads: a copy of the socket set later did not change it on Windows).
fn open_waiting(host: SocketAddr, session: u64, wait: Duration) -> Result<Connection, String> {
    let stream = TcpStream::connect_timeout(&host, Duration::from_secs(6)).map_err(|e| format!("cannot reach the host's mods on TCP {host}: {e}"))?;
    // the host greets once its list is made, which takes a while on a big map (it waits
    // up to ten minutes for it): after 25 s a join gave up on a big add-on map, and the
    // player was left without the host's map
    stream.set_read_timeout(Some(Duration::from_secs(600))).ok();
    stream.set_nodelay(true).ok();
    let raw = stream.try_clone().map_err(|e| e.to_string())?;
    let mut out = stream.try_clone().map_err(|e| e.to_string())?;
    let mut input = BufReader::with_capacity(1 << 20, paced::Paced::greeting(stream));
    let keys = channel::Handshake::new().map_err(|e| e.to_string())?;
    writeln!(out, "{} {} {}", channel::MAGIC, omsi_net::session_hex(session), keys.public_hex()).map_err(|e| e.to_string())?;
    let mut line = String::new();
    // (a connection the host closed or broke before it greeted: asked again soon; a line longer than a reply ends it)
    if read_line_within(&mut input, &mut line, MAX_REPLY).map_err(|e| format!("{CONNECTION_LOST}: {e}"))? == 0 {
        return Err(format!("{CONNECTION_LOST}: closed by the host"));
    }
    let t = line.trim();
    let Some(host_key) = t.strip_prefix("OK ") else {
        let said = t.strip_prefix("ERR").unwrap_or(t).trim();
        // (what a host of the unencrypted protocol says to every greeting of this one)
        if said == "not this session" {
            return Err("the host says: not this session (or the host's openOMSI is older and sends its mods unencrypted: both need the same version)".into());
        }
        return Err(format!("the host says: {said}"));
    };
    let proof: Vec<&str> = host_key.split_whitespace().collect();
    let [host_key, public, sig] = proof[..] else {
        return Err("the host's openOMSI is older: both need the same version".into());
    };
    if !channel::proven(session, &keys.public_hex(), host_key, public, sig) {
        return Err("the host could not prove it runs this session: its mods are not taken".into());
    }
    let (send, receive) = keys.finish(session, true, host_key).map_err(|e| e.to_string())?;
    // (then a stalled transfer ends the attempt instead of holding the game's start for ever)
    input.get_ref().get_ref().set_read_timeout(Some(wait)).ok();
    input.get_mut().pace();
    Ok((raw, SecureWriter::new(out, send), SecureReader::new(input, receive)))
}

/// The downloads kept between sessions, sealed (`~/.openomsi/lan-store`, see `store`): a
/// map fetched once is not fetched again at the next join.
fn store_dir() -> Option<PathBuf> {
    sandbox_base().map(|b| b.with_file_name("lan-store"))
}

pub fn fetch(root: &Path, map: &mut String, host: SocketAddr, session: u64, progress: &mut dyn FnMut(u64, u64, &str)) -> Result<Report, String> {
    fetch_with(root, map, host, session, progress, false)
}

/// How many files the session's mount holds (a look again that finds no more leaves it).
static MOUNTED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// `fetch`, or (`again`) a look at the host's list in the middle of the session: then the
/// session's content is mounted again only when it has more files than before. `root` is
/// the game's own root (its vehicles' files are never covered by the host's).
fn fetch_with(root: &Path, map: &mut String, host: SocketAddr, session: u64, progress: &mut dyn FnMut(u64, u64, &str), again: bool) -> Result<Report, String> {
    if omsi_cfg::flags::OMSI_NO_LAN_MODS.is_set() {
        return Err("switched off (OMSI_NO_LAN_MODS)".into());
    }
    let (raw, mut out, mut input) = open(host, session)?;
    writeln!(out, "LIST").and_then(|_| out.flush()).map_err(|e| e.to_string())?;
    let len = read_reply(&mut input)?.ok_or("no list")?;
    if len > 256 << 20 {
        return Err("the list is too large".into());
    }
    let mut body = vec![0u8; len as usize];
    input.read_exact(&mut body).map_err(|e| e.to_string())?;
    let manifest: Manifest = serde_json::from_slice(&body).map_err(|e| format!("bad list: {e}"))?;
    if manifest.entries.len() > MAX_FILES {
        return Err(format!("{} files are too many", manifest.entries.len()));
    }
    let mut report = Report::default();
    // which of them this machine lacks (or has in another version)
    let mut todo: Vec<usize> = Vec::new();
    let mut total = 0u64;
    let mut seen: HashSet<String> = HashSet::new();
    for (k, e) in manifest.entries.iter().enumerate() {
        if let Some(why) = refuse_path(&e.path) {
            report.refused.push(format!("{}: {why}", e.path));
            continue;
        }
        // a vehicle's file this machine has of its own is never covered by the host's: its
        // paints (`.cti`) and textures stay this player's, also on the bus it drives
        if own_vehicle_file(&e.path, root) || own_script(&e.path) {
            report.kept_own += 1;
            continue;
        }
        // (the hash names the file in the store: nothing but 64 hex digits may go there)
        if e.size > MAX_FILE || e.sha256.len() != 64 || !e.sha256.bytes().all(|b| b.is_ascii_hexdigit()) || !seen.insert(e.path.to_lowercase()) {
            report.refused.push(format!("{}: refused", e.path));
            continue;
        }
        todo.push(k);
    }
    // which of them this machine has already: the sizes first, the hashes remembered by
    // size and time (hashing every local file again took two minutes on each join, long
    // enough for the session to give the player up), the rest in parallel
    let cache = hash_cache();
    let checks: Vec<(usize, bool, Option<(String, (u64, u64, String))>)> = {
        use rayon::prelude::*;
        todo.par_iter()
            .map(|&k| {
                let e = &manifest.entries[k];
                let Some((_, p)) = omsi_cfg::find_in_roots(&e.path).filter(|(root, _)| !omsi_cfg::is_sandbox(root)) else { return (k, false, None) };
                let key = p.to_string_lossy().to_string();
                if let Ok(md) = std::fs::metadata(&p) {
                    if md.len() != e.size {
                        return (k, false, None);
                    }
                    let mtime = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
                    if let Some((sz, mt, h)) = cache.get(&key) {
                        if *sz == md.len() && *mt == mtime {
                            return (k, *h == e.sha256, None);
                        }
                    }
                    let Ok(d) = std::fs::read(&p) else { return (k, false, None) };
                    let h = sha256_of(&d);
                    return (k, h == e.sha256, Some((key, (md.len(), mtime, h))));
                }
                let same = omsi_cfg::vfs::read(&p).ok().map(|d| d.len() as u64 == e.size && sha256_of(&d) == e.sha256).unwrap_or(false);
                (k, same, None)
            })
            .collect()
    };
    let mut cache = cache;
    let base = sandbox_base().ok_or("no home folder")?;
    // (the mount point: no folder on the disk)
    let dir = base.join(std::process::id().to_string());
    let store = store::Store::open(&store_dir().ok_or("no home folder")?)?;
    // the session's files: in the store, sealed
    let mut mounted: Vec<omsi_cfg::vfs::SealedFile> = Vec::new();
    todo.clear();
    for (k, same, fresh) in checks {
        if let Some((key, v)) = fresh {
            cache.insert(key, v);
        }
        let e = &manifest.entries[k];
        if same {
            report.had += 1;
            continue;
        }
        // fetched at an earlier join
        if let Some(f) = store.has(e) {
            mounted.push(f);
            report.had += 1;
            continue;
        }
        todo.push(k);
        total += e.size;
    }
    save_hash_cache(&cache);
    if total > MAX_TOTAL {
        return Err(format!("the host's mods are {:.1} GB, more than a session takes ({:.0} GB)", total as f64 / 1e9, MAX_TOTAL as f64 / 1e9));
    }
    *SANDBOX.lock().unwrap_or_else(|e| e.into_inner()) = Some(dir.clone());
    log::info!("LAN mods: {} files of the host's are here already, {} to fetch ({:.1} MB)", report.had, todo.len(), total as f64 / 1e6);
    if !todo.is_empty() {
        let some: Vec<&str> = todo.iter().take(6).map(|k| manifest.entries[*k].path.as_str()).collect();
        log::info!("LAN mods: to fetch, e.g. {}", some.join(", "));
    }
    // (nothing to fetch needs no room: a nearly full disk turned away a join that had
    // everything already)
    if let Some(free) = free_space(store.dir()).filter(|_| total > 0) {
        if free < total + KEEP_FREE {
            return Err(format!("{:.1} GB are needed for the host's mods, {:.1} GB are free", (total + KEEP_FREE) as f64 / 1e9, free as f64 / 1e9));
        }
    }
    // every request at once (one after the other took a round trip per file: through a
    // tunnel a few hundred kB/s), the answers read as they come; a broken connection is
    // opened again and goes on with what is left (what came is in the store)
    let mut done = 0u64;
    let mut left: Vec<usize> = todo.clone();
    let mut attempt = 0;
    let mut conn = Some((raw, out, input));
    while !left.is_empty() {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(500 * attempt as u64));
            match open(host, session) {
                Ok(c) => conn = Some(c),
                Err(e) if attempt < 8 => {
                    log::warn!("LAN mods: {e}; trying again");
                    attempt += 1;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        attempt += 1;
        let mut req = String::with_capacity(left.len() * 10);
        for k in &left {
            req.push_str(&format!("GET {k}\n"));
        }
        req.push_str("BYE\n");
        let (raw, mut w, mut input) = conn.take().ok_or("no connection")?;
        let writer = std::thread::spawn(move || {
            let _ = w.write_all(req.as_bytes()).and_then(|_| w.flush());
        });
        let mut got = 0usize;
        let mut failed: Option<String> = None;
        for &k in &left {
            let e = &manifest.entries[k];
            let r = (|| -> Result<Option<Vec<u8>>, String> {
                let len = read_reply(&mut input)?.ok_or("no size")?;
                if len != e.size {
                    return Err(format!("{}: {len} bytes, the list said {}", e.path, e.size));
                }
                let mut data = vec![0u8; len as usize];
                input.read_exact(&mut data).map_err(|x| format!("{}: {x}", e.path))?;
                if sha256_of(&data) != e.sha256 {
                    return Err(format!("{}: not the file the list names (hash)", e.path));
                }
                Ok(Some(data))
            })();
            let data = match r {
                Ok(Some(d)) => d,
                Ok(None) => continue,
                Err(x) => {
                    failed = Some(x);
                    break;
                }
            };
            got += 1;
            let len = data.len() as u64;
            // (no program, and what it says it is: a texture must be a picture of a size a
            // texture can have, a sound a sound - `quarantine`)
            if let Err(why) = quarantine::screen(&e.path, &data) {
                report.refused.push(why);
                continue;
            }
            // (sealed straight away: the plain bytes never reach the disk)
            mounted.push(store.put(e, &data).map_err(|x| format!("{}: {x}", e.path))?);
            done += len;
            report.fetched += 1;
            report.bytes += len;
            progress(done, total, &e.path);
        }
        let _ = raw.shutdown(std::net::Shutdown::Both);
        let _ = writer.join();
        left.drain(..got);
        if let Some(x) = failed {
            if attempt >= 8 || x.contains("the host says") {
                return Err(x);
            }
            log::warn!("LAN mods: {x}; {} files left, connecting again", left.len());
        }
    }
    if again && mounted.len() <= MOUNTED.load(std::sync::atomic::Ordering::Relaxed) {
        upload::remember_host(host, session);
        return Ok(report);
    }
    MOUNTED.store(mounted.len(), std::sync::atomic::Ordering::Relaxed);
    // the session folder is content like any other, searched first, and never a source of
    // plugins
    omsi_cfg::vfs::mount_sealed(&dir, store.key(), mounted);
    omsi_cfg::mark_sandbox(dir.clone());
    omsi_cfg::add_content_root_first(dir.clone());
    // the host's map (now that we have it)
    let map_ok = refuse_path(&manifest.map).is_none() && omsi_cfg::find_in_roots(&manifest.map).is_some();
    if map_ok && !manifest.map.is_empty() && !manifest.map.eq_ignore_ascii_case(&map.replace('\\', "/")) {
        log::info!("LAN mods: the session is on the host's map {}", manifest.map);
        *map = manifest.map.clone();
    }
    if !report.refused.is_empty() {
        log::warn!("LAN mods: {} files refused: {}", report.refused.len(), report.refused.iter().take(8).cloned().collect::<Vec<_>>().join("; "));
    }
    if report.kept_own > 0 {
        log::info!("LAN mods: {} files of vehicles this machine has of its own were left as they are here", report.kept_own);
    }
    log::info!("LAN mods: fetched {} files ({:.1} MB), {} were here already", report.fetched, report.bytes as f64 / 1e6, report.had);
    upload::remember_host(host, session);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_that_are_refused() {
        assert!(refuse_path("maps/Ahlheim/global.cfg").is_none());
        assert!(refuse_path("Vehicles/LiAZ/Model/model.cfg").is_none());
        assert!(refuse_path("Sceneryobjects/X/texture/a.dds").is_none());
        for bad in [
            "../etc/passwd",
            "maps/../../x.cfg",
            "/abs/maps/x",
            "C:/Windows/x.cfg",
            "maps\\x\\y.cfg",
            "Plugins/evil.dll",
            "Vehicles/X/Plugins/a.cfg",
            "Vehicles/X/setup.exe",
            "maps/x/run.BAT",
            "maps/x/a.sh",
            "maps/x/lib.dylib",
            "maps/x/pack.zip",
            "Startup/x.cfg",
            "x.cfg",
            "maps//x.cfg",
            "maps/x/y?.cfg",
        ] {
            assert!(refuse_path(bad).is_some(), "{bad} should be refused");
        }
    }

    /// A host serving `files` (relative path, contents) on a local port: its address.
    fn serve_files(files: &[(&str, Vec<u8>)]) -> (SocketAddr, PathBuf, u64) {
        let key = Arc::new(omsi_net::hostkey::HostKey::generate().unwrap());
        let session = key.session();
        let dir = std::env::temp_dir().join(format!("openomsi-lan-serve-{}-{session:x}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut entries = Vec::new();
        let mut sources = Vec::new();
        for (k, (rel, data)) in files.iter().enumerate() {
            let p = dir.join(format!("{k}"));
            std::fs::write(&p, data).unwrap();
            entries.push(Entry { path: rel.to_string(), size: data.len() as u64, sha256: sha256_of(data) });
            sources.push(p);
        }
        let ready = Arc::new(Mutex::new(Some(Arc::new((Manifest { map: "maps/M/global.cfg".into(), bus: String::new(), entries }, sources)))));
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (ready, key) = (ready.clone(), key.clone());
                std::thread::spawn(move || handle(stream, &key, &ready));
            }
        });
        (addr, dir, session)
    }

    #[test]
    fn mods_go_encrypted_over_tcp() {
        let marker = b"[name]\nPLAINTEXT-MARKER-wire\n".repeat(5000);
        let small = b"[mesh]\nPLAINTEXT-MARKER-two\n".to_vec();
        let (addr, dir, session) = serve_files(&[("maps/M/global.cfg", marker.clone()), ("maps/M/x.sco", small.clone())]);
        // what goes over the wire: a relay in between records it
        let relay = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let relay_addr = relay.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::<u8>::new()));
        {
            let seen = seen.clone();
            std::thread::spawn(move || {
                let (client, _) = relay.accept().unwrap();
                let host = TcpStream::connect(addr).unwrap();
                let (mut c2, mut h2) = (client.try_clone().unwrap(), host.try_clone().unwrap());
                std::thread::spawn(move || std::io::copy(&mut c2, &mut h2));
                let (mut h, mut c) = (host, client);
                let mut buf = [0u8; 8192];
                while let Ok(n) = h.read(&mut buf) {
                    if n == 0 || c.write_all(&buf[..n]).is_err() {
                        break;
                    }
                    seen.lock().unwrap().extend_from_slice(&buf[..n]);
                }
                let _ = c.shutdown(std::net::Shutdown::Both);
            });
        }
        let (raw, mut out, mut input) = open(relay_addr, session).unwrap();
        out.write_all(b"LIST\nGET 0\nGET 1\nGET 7\nGET ../../etc/passwd\nGET maps/M/global.cfg\nBYE\n").unwrap();
        out.flush().unwrap();
        let len = read_reply(&mut input).unwrap().unwrap();
        let mut body = vec![0u8; len as usize];
        input.read_exact(&mut body).unwrap();
        let manifest: Manifest = serde_json::from_slice(&body).unwrap();
        assert_eq!(manifest.entries.len(), 2);
        for (k, want) in [marker.clone(), small.clone()].into_iter().enumerate() {
            let n = read_reply(&mut input).unwrap().unwrap();
            assert_eq!(n, manifest.entries[k].size);
            let mut data = vec![0u8; n as usize];
            input.read_exact(&mut data).unwrap();
            assert_eq!(data, want);
        }
        // (files by their number in the list only: no name is ever opened)
        for _ in 0..3 {
            assert!(read_reply(&mut input).unwrap_err().contains("no such file"));
        }
        let _ = raw.shutdown(std::net::Shutdown::Both);
        std::thread::sleep(Duration::from_millis(100));
        let wire = seen.lock().unwrap().clone();
        assert!(wire.len() > marker.len(), "{} bytes seen", wire.len());
        assert!(!wire.windows(16).any(|w| w == b"PLAINTEXT-MARKER"), "plain text on the wire");
        assert!(!wire.windows(10).any(|w| w == b"global.cfg"), "a file name on the wire");
        // another session's id gets nothing
        assert!(open(addr, session ^ 1).err().unwrap().contains("not this session"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_old_peer_is_refused_clearly() {
        // a game of the old protocol, at a new host
        let (addr, dir, session) = serve_files(&[("maps/M/global.cfg", b"x".to_vec())]);
        for magic in ["OMSIMODS/1", "OMSIMODS/2"] {
            let mut s = TcpStream::connect(addr).unwrap();
            s.write_all(format!("{magic} {} {}\n", omsi_net::session_hex(session), "00".repeat(32)).as_bytes()).unwrap();
            let reply = read_reply(&mut BufReader::new(s)).unwrap_err();
            assert!(reply.contains("update openOMSI"), "{reply}");
        }
        std::fs::remove_dir_all(&dir).ok();
        // a host of the old protocol answers this one's greeting as it answered any other
        let old = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let old_addr = old.local_addr().unwrap();
        std::thread::spawn(move || {
            let (s, _) = old.accept().unwrap();
            let mut line = String::new();
            BufReader::new(s.try_clone().unwrap()).read_line(&mut line).unwrap();
            (&s).write_all(b"ERR not this session\n").unwrap();
        });
        let err = open(old_addr, 0x01d).err().unwrap();
        assert!(err.contains("older") && err.contains("same version"), "{err}");
    }

    /// A vehicle's file this machine has of its own is never covered by the host's (its
    /// paints stay this player's); what it lacks - of a vehicle it has, too - comes, and so
    /// does everything else of the map.
    #[test]
    fn the_hosts_vehicle_files_never_cover_ours() {
        let root = std::env::temp_dir().join(format!("openomsi-own-vehicles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Vehicles").join("MyBus").join("Texture")).unwrap();
        std::fs::write(root.join("Vehicles").join("MyBus").join("Texture").join("repaints.cti"), b"[item]\nMine\nx\ny.dds\n").unwrap();
        assert!(own_vehicle_file("Vehicles/MyBus/Texture/repaints.cti", &root));
        assert!(own_vehicle_file("vehicles/mybus/texture/REPAINTS.cti", &root), "as Windows finds it");
        // what the player lacks of a bus it has: an add-on repaint, a .bus the map uses
        assert!(!own_vehicle_file("Vehicles/MyBus/Texture/addon_repaint.dds", &root));
        assert!(!own_vehicle_file("Vehicles/MyBus/MyBus_newer.bus", &root));
        assert!(!own_vehicle_file("Vehicles/TheirBus/Texture/repaints.cti", &root));
        assert!(!own_vehicle_file("maps/Town/global.cfg", &root));
        assert!(!own_vehicle_file("Sceneryobjects/X/x.sco", &root));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_mods_server_without_the_session_key_is_refused() {
        let real = omsi_net::hostkey::HostKey::generate().unwrap();
        let impostor = omsi_net::hostkey::HostKey::generate().unwrap();
        let session = real.session();
        let fake = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let fake_addr = fake.local_addr().unwrap();
        std::thread::spawn(move || {
            for i in 0..2 {
                let (s, _) = fake.accept().unwrap();
                let mut line = String::new();
                BufReader::new(s.try_clone().unwrap()).read_line(&mut line).unwrap();
                let client = line.split_whitespace().nth(2).unwrap_or("").to_string();
                let keys = channel::Handshake::new().unwrap();
                let host = keys.public_hex();
                let reply = if i == 0 { channel::prove(&impostor, &client, &host) } else { format!("{} {}", omsi_net::hostkey::hex(real.public()), "00".repeat(64)) };
                let _ = (&s).write_all(format!("OK {host} {reply}\n").as_bytes());
            }
        });
        for _ in 0..2 {
            let err = open(fake_addr, session).err().unwrap();
            assert!(err.contains("could not prove"), "{err}");
        }
    }

    #[test]
    fn a_host_that_never_ends_a_line_is_given_up() {
        assert!(read_reply(&mut std::io::Cursor::new(vec![b'A'; 1 << 20])).unwrap_err().contains("too long"));
        let host = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = host.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = host.accept().unwrap();
            let mut line = String::new();
            BufReader::new(s.try_clone().unwrap()).read_line(&mut line).unwrap();
            let _ = s.write_all(&[b'A'; 64 * 1024]);
            std::thread::sleep(Duration::from_secs(2));
        });
        assert!(open(addr, 0x5e57).err().unwrap().contains("too long"));
    }

    #[test]
    fn programs_are_seen_by_their_content() {
        assert!(looks_executable(b"MZ\x90\x00"));
        assert!(looks_executable(b"\x7fELF\x02"));
        assert!(looks_executable(b"#!/bin/sh"));
        assert!(looks_executable(&[0xcf, 0xfa, 0xed, 0xfe]));
        assert!(!looks_executable(b"DDS "));
        assert!(!looks_executable(b"[mesh]"));
    }
}
