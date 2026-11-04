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

/// The host's depot files by name in lower case (the file name without `.hof`).
static OFFERED: Mutex<Option<HashMap<String, PathBuf>>> = Mutex::new(None);

/// A player's fetches: the file, once here, or None when the host had none to give.
static FETCHED: Mutex<Option<HashMap<String, Fetch>>> = Mutex::new(None);

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
pub(super) fn answer(name: &str, out: &mut impl Write) -> std::io::Result<()> {
    let Some(path) = served(name) else {
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
    let mut guard = FETCHED.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    match map.get(&key) {
        Some(Fetch::Have(p)) => return Some(p.clone()),
        Some(Fetch::Asking | Fetch::None) => return None,
        None => {}
    }
    let (host, session) = super::upload::host()?;
    map.insert(key.clone(), Fetch::Asking);
    drop(guard);
    let name = name.trim().to_string();
    std::thread::spawn(move || {
        let got = fetch(host, session, &name);
        let state = match got {
            Ok(p) => {
                log::info!("depot file {name}: the host's taken ({})", p.display());
                Fetch::Have(p)
            }
            Err(e) => {
                log::info!("depot file {name}: not had from the host ({e}); this game's own is used");
                Fetch::None
            }
        };
        FETCHED.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(key, state);
    });
    None
}

/// A player: has the host's answer about depot file `name` come (the file, or that it has
/// none)? Before, a lookup of our own stands in only for the moment.
pub fn settled(name: &str) -> bool {
    let Some(key) = valid(name) else { return true };
    let guard = FETCHED.lock().unwrap_or_else(|e| e.into_inner());
    match guard.as_ref().and_then(|m| m.get(&key)) {
        Some(Fetch::Have(_) | Fetch::None) => true,
        Some(Fetch::Asking) => false,
        // (never asked: no host to ask, `fetched` gave up at once)
        None => super::upload::host().is_none(),
    }
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
        answer("TOWN", &mut out).unwrap();
        assert_eq!(out, b"OK 12\n[name]\nTown\n");
        let mut out = Vec::new();
        answer("../Town", &mut out).unwrap();
        assert!(out.starts_with(b"ERR"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
