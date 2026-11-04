//! The depot files (`.hof`) of the host's AI buses, sent to every player and used there in
//! place of any of their own. The host tells each bus's depot file by name (`TRAF`'s numbers
//! section); a player who looked that name up in its own files found another file under it, or
//! none, and showed other signs. So the host serves the very file its bus uses (`offer`), else
//! the one of that name in the global `HOFs` folder, and a player fetches it once a session
//! (`fetched`) and draws the host's buses with it whatever it has itself.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A depot file larger than this is not one (the largest are a few hundred KB).
pub(super) const MAX_HOF: u64 = 4 << 20;

/// At most this many depot files are fetched from one host.
const MAX_FETCHES: usize = 64;

/// The host's depot files by name in lower case (the file name without `.hof`).
static OFFERED: Mutex<Option<HashMap<String, PathBuf>>> = Mutex::new(None);

/// A player's fetches in the session they are of: the file, once here, or None when the host had
/// none to give. Another session (another host, in the same process: the phone's launcher and
/// game are one) starts afresh.
static FETCHED: Mutex<Option<(u64, HashMap<String, Fetch>)>> = Mutex::new(None);

/// This session's fetches, the table emptied when it is another session's.
fn of_session(guard: &mut Option<(u64, HashMap<String, Fetch>)>, session: u64) -> &mut HashMap<String, Fetch> {
    if guard.as_ref().is_none_or(|(s, _)| *s != session) {
        *guard = Some((session, HashMap::new()));
    }
    &mut guard.as_mut().expect("set above").1
}

#[derive(Clone)]
enum Fetch {
    Asking,
    Have(PathBuf),
    None,
}

/// A depot file name as it may travel: a file name without its extension, no folder.
fn valid(name: &str) -> Option<String> {
    let n = name.trim();
    let ok = !n.is_empty() && n.len() <= 128 && n != "." && n != ".." && !n.chars().any(|c| matches!(c, '/' | '\\' | ':' | '\0') || c.is_control());
    ok.then(|| n.to_lowercase())
}

/// The host: `path` is the depot file its AI bus of depot `name` uses; a player asking for
/// `name` gets that file.
pub fn offer(name: &str, path: &Path) {
    let Some(key) = valid(name) else { return };
    let mut guard = OFFERED.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    if !map.contains_key(&key) {
        map.insert(key, path.to_path_buf());
    }
}

/// The host's file for depot `name`: its bus's own, else `HOFs/<name>.hof` of the content.
pub(super) fn served(name: &str) -> Option<PathBuf> {
    let key = valid(name)?;
    let offered = OFFERED.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&key).cloned());
    offered.or_else(|| omsi_cfg::find_in_roots(&format!("HOFs/{}.hof", name.trim())).map(|(_, p)| p)).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("hof")))
}

/// The host's answer to `HOF <name>`: `OK <len>` and the file, or `ERR`.
pub(super) fn answer(root: &Path, name: &str, out: &mut impl Write) -> std::io::Result<()> {
    // (as every file sent: never one that resolves outside the content's folders)
    let Some(path) = served(name).filter(|p| super::server_fleet::trusted_source(root, p)) else {
        return writeln!(out, "ERR no depot file {name}");
    };
    match std::fs::read(&path) {
        Ok(bytes) if bytes.len() as u64 <= MAX_HOF => {
            writeln!(out, "OK {}", bytes.len())?;
            out.write_all(&bytes)
        }
        Ok(_) => writeln!(out, "ERR depot file too large"),
        Err(e) => writeln!(out, "ERR {e}"),
    }
}

/// A player: the host's depot file `name`, once fetched. The first ask starts the fetch in the
/// background; until it is here (or when the host has none) None, and the caller's own lookup
/// stands in.
pub fn fetched(name: &str) -> Option<PathBuf> {
    let key = valid(name)?;
    let (host, session) = super::upload::host()?;
    let mut guard = FETCHED.lock().unwrap_or_else(|e| e.into_inner());
    let map = of_session(&mut guard, session);
    match map.get(&key) {
        Some(Fetch::Have(p)) => return Some(p.clone()),
        Some(Fetch::Asking | Fetch::None) => return None,
        None => {}
    }
    // (a map has a few depot files: a host naming more gets no more fetches)
    if map.len() >= MAX_FETCHES {
        return None;
    }
    map.insert(key.clone(), Fetch::Asking);
    drop(guard);
    let name = name.trim().to_string();
    std::thread::spawn(move || {
        let state = match fetch(host, session, &name) {
            Ok(p) => {
                log::info!("depot file {name}: the host's taken ({})", p.display());
                Fetch::Have(p)
            }
            Err(e) => {
                log::info!("depot file {name}: not had from the host ({e}); this game's own is used");
                Fetch::None
            }
        };
        // (kept only while its session is still the one: an answer from the last host is not
        // this one's)
        let mut guard = FETCHED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, map)) = guard.as_mut().filter(|(s, _)| *s == session) {
            map.insert(key, state);
        }
    });
    None
}

/// A player: has the host's answer about depot file `name` come (the file, or that it has
/// none)? Before, a lookup of our own stands in only for the moment.
pub fn settled(name: &str) -> bool {
    let Some(key) = valid(name) else { return true };
    // (no host to ask: our own lookup is all there is)
    let Some((_, session)) = super::upload::host() else { return true };
    let mut guard = FETCHED.lock().unwrap_or_else(|e| e.into_inner());
    let map = of_session(&mut guard, session);
    match map.get(&key) {
        Some(Fetch::Have(_) | Fetch::None) => true,
        Some(Fetch::Asking) => false,
        // (never asked: past the fetches one host gets)
        None => map.len() >= MAX_FETCHES,
    }
}

/// A player's depot file of a host's vehicle, kept in `cache` by `key` with whether it is final:
/// the host's file of `name` once it is here (`host` loads it), else `own` (this game's lookup,
/// made once per key: kept as provisional while the host's answer is on its way, and final once
/// it has come). No name: `own`, final.
pub fn cached<K: std::hash::Hash + Eq + Clone, T: Clone, S: std::hash::BuildHasher>(cache: &mut hashbrown::HashMap<K, (T, bool), S>, key: K, name: Option<&str>, host: impl Fn(&Path) -> Option<T>, own: impl FnOnce() -> T) -> T {
    cached_with(cache, key, name, fetched, settled, host, own)
}

/// `cached`, with how the host's answer is asked for and known to have come.
fn cached_with<K: std::hash::Hash + Eq + Clone, T: Clone, S: std::hash::BuildHasher>(cache: &mut hashbrown::HashMap<K, (T, bool), S>, key: K, name: Option<&str>, fetched: impl Fn(&str) -> Option<PathBuf>, settled: impl Fn(&str) -> bool, host: impl Fn(&Path) -> Option<T>, own: impl FnOnce() -> T) -> T {
    let provisional = match cache.get(&key) {
        Some((t, true)) => return t.clone(),
        Some((t, false)) => Some(t.clone()),
        None => None,
    };
    if let Some(t) = name.and_then(&fetched).and_then(|p| host(&p)) {
        cache.insert(key, (t.clone(), true));
        return t;
    }
    let settled = name.is_none_or(&settled);
    let t = provisional.unwrap_or_else(own);
    cache.insert(key, (t.clone(), settled));
    t
}

/// Asks the host for depot file `name` and keeps it in this session's folder.
fn fetch(host: std::net::SocketAddr, session: u64, name: &str) -> Result<PathBuf, String> {
    let (_raw, mut out, mut input) = super::failure::open(host, session).map_err(|e| e.to_string())?;
    writeln!(out, "HOF {}", super::server_fleet::hex_text(name)).and_then(|_| out.flush()).map_err(|e| e.to_string())?;
    let len = super::failure::reply(&mut input).map_err(|e| e.to_string())?.ok_or("none")?;
    if len > MAX_HOF {
        return Err("too large".into());
    }
    let mut body = vec![0u8; len as usize];
    input.read_exact(&mut body).map_err(|e| e.to_string())?;
    let _ = writeln!(out, "BYE").and_then(|_| out.flush());
    let dir = super::sandbox_base().ok_or("no home folder")?.join("hofs").join(format!("{session:016x}"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.hof", valid(name).ok_or("invalid name")?));
    std::fs::write(&path, &body).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_depot_name_is_a_file_name_only() {
        assert_eq!(valid(" Spandau 1986 ").as_deref(), Some("spandau 1986"));
        for bad in ["", "..", "../x", "a/b", "a\\b", "c:x", "a\nb"] {
            assert!(valid(bad).is_none(), "{bad:?}");
        }
    }

    /// While the host's file is on its way this game's own lookup stands in, made once; the
    /// host's file replaces it once here and is kept. Without a host the own one is final.
    #[test]
    fn the_own_lookup_stands_in_until_the_hosts_file_is_here() {
        use std::cell::Cell;
        let mut cache = hashbrown::HashMap::new();
        let (asked, here) = (Cell::new(0), Cell::new(false));
        let mut look = |cache: &mut hashbrown::HashMap<_, _>| {
            cached_with(cache, ("bus", "town"), Some("Town"), |_| here.get().then(|| PathBuf::from("Town.hof")), |_| here.get(), |_| Some(99), || {
                asked.set(asked.get() + 1);
                7
            })
        };
        assert_eq!(look(&mut cache), 7, "on its way: our own");
        assert_eq!(look(&mut cache), 7);
        assert_eq!(asked.get(), 1, "our own looked up once");
        here.set(true);
        assert_eq!(look(&mut cache), 99, "the host's, once here");
        here.set(false);
        assert_eq!(look(&mut cache), 99, "and kept");
        assert_eq!(asked.get(), 1);
        let mut none = hashbrown::HashMap::new();
        assert_eq!(cached_with(&mut none, 1, Some("Town"), |_| None, |_| true, |_| Some(99), || 7), 7);
        assert_eq!(none.get(&1), Some(&(7, true)), "the host has none: ours is final");
    }

    /// Another session's fetches are not this one's: a file, a failure or the count.
    #[test]
    fn a_new_session_fetches_afresh() {
        let mut table = None;
        of_session(&mut table, 1).insert("town".into(), Fetch::None);
        assert!(of_session(&mut table, 1).contains_key("town"));
        assert!(of_session(&mut table, 2).is_empty());
    }

    /// The bus's own depot file is served under its name; another name is not a path.
    #[test]
    fn the_host_serves_its_buses_depot_file() {
        let dir = std::env::temp_dir().join(format!("omsi-hofs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Town.hof");
        std::fs::write(&file, b"[name]\nTown\n").unwrap();
        offer("Town", &file);
        assert_eq!(served("town"), Some(file.clone()));
        let mut out = Vec::new();
        answer(&dir, "TOWN", &mut out).unwrap();
        assert_eq!(out, b"OK 12\n[name]\nTown\n");
        let mut out = Vec::new();
        answer(&dir, "../Town", &mut out).unwrap();
        assert!(out.starts_with(b"ERR"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
