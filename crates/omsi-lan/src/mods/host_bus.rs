//! The server's list of the bus this game took from its fleet, kept to be set beside our own
//! list of it (`upload::own_bus`): when the two fingerprints differ, the log names the files
//! one side has and the other has not, and the ones both have with other content - the server
//! could not say, it only learns our fingerprint.

use super::Entry;
use std::collections::HashMap;
use std::sync::Mutex;

/// (bus key, paint in lower case, the server's files of it)
static HOST_BUS: Mutex<Option<(String, String, Vec<Entry>)>> = Mutex::new(None);

/// How many paths each part of the log names at most.
const NAMED: usize = 15;

/// The server's list of `bus` in `paint` (`LISTBUS`), as it sent it.
pub(super) fn remember(bus: &str, paint: &str, entries: &[Entry]) {
    let Some(key) = super::identity::key(bus) else { return };
    *HOST_BUS.lock().unwrap_or_else(|e| e.into_inner()) = Some((key, paint.to_lowercase(), entries.to_vec()));
}

/// Our list of `bus` in `paint` beside the server's, if this game took that bus from it.
pub(super) fn compare(bus: &str, paint: &str, ours: &[Entry]) {
    let Some(key) = super::identity::key(bus) else { return };
    let guard = HOST_BUS.lock().unwrap_or_else(|e| e.into_inner());
    let Some((_, _, theirs)) = guard.as_ref().filter(|(b, p, _)| *b == key && *p == paint.to_lowercase()) else { return };
    let (only_there, only_here, differ) = differences(theirs, ours);
    if only_there.is_empty() && only_here.is_empty() && differ.is_empty() {
        log::info!("bus identity: {bus} ({paint}): the same {} files as the server's list", ours.len());
        return;
    }
    let names = |v: &[String]| v.iter().take(NAMED).cloned().collect::<Vec<_>>().join(", ");
    log::warn!(
        "bus identity: {bus} ({paint}) is not the server's list ({} files there, {} here; the server's holds every paint, so other paints' files show as only there): {} only on the server [{}]; {} only here [{}]; {} with other content [{}]",
        theirs.len(), ours.len(), only_there.len(), names(&only_there), only_here.len(), names(&only_here), differ.len(), names(&differ)
    );
}

/// Did this game take `bus` in `paint` from the server's fleet?
pub(super) fn from_server(bus: &str, paint: &str) -> bool {
    let Some(key) = super::identity::key(bus) else { return false };
    HOST_BUS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|(b, p, _)| *b == key && *p == paint.to_lowercase())
}

/// The server's own list of `bus` in `paint` (`FLEETINFO`: that paint only), with where each
/// of its files is here - when every one of them is here (the player's own or the multiplayer
/// cache) with the server's content. Then this game drives the server's version of the bus,
/// and that is the bus the others are told: a file of ours the server has not (a copy of a
/// font, a stray texture) no longer makes it another version. None, and why in the log, when
/// a file is missing or other here.
pub(super) fn server_version(bus: &str, paint: &str) -> Option<(super::identity::VehicleManifest, Vec<(String, std::path::PathBuf)>)> {
    use std::io::{Read, Write};
    let (host, session) = super::upload::host()?;
    let asked = (|| -> Result<super::identity::VehicleManifest, String> {
        let (_raw, mut out, mut input) = super::failure::open(host, session).map_err(|e| e.to_string())?;
        writeln!(out, "FLEETINFO {} {}", super::server_fleet::hex_text(bus), super::server_fleet::hex_text(paint)).and_then(|_| out.flush()).map_err(|e| e.to_string())?;
        let len = super::failure::reply(&mut input).map_err(|e| e.to_string())?.ok_or("no list")?;
        if len > 256 << 20 {
            return Err("the list is too large".into());
        }
        let mut body = vec![0u8; len as usize];
        input.read_exact(&mut body).map_err(|e| e.to_string())?;
        let m: super::identity::VehicleManifest = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
        let asked = omsi_net::vehicle_path(bus).ok_or("invalid bus path")?;
        if m.bus.to_ascii_lowercase() != asked.to_ascii_lowercase() {
            return Err(format!("the server answered with another bus ({})", m.bus));
        }
        m.validated()
    })();
    let m = match asked {
        Ok(m) => m,
        Err(e) => {
            log::info!("bus identity: {bus} ({paint}): the server's own list of it not had ({e}); ours stays");
            return None;
        }
    };
    let mut files = Vec::with_capacity(m.entries.len());
    for e in &m.entries {
        if super::refuse_path(&e.path).is_some() {
            return None;
        }
        let Some((_, p)) = omsi_cfg::find_in_roots(&e.path) else {
            log::info!("bus identity: {bus} ({paint}): {} of the server's version is not here; ours stays", e.path);
            return None;
        };
        files.push((e.path.clone(), p));
    }
    let here: HashMap<String, String> = super::upload::hashed(&files).into_iter().filter_map(|e| super::identity::key(&e.path).map(|k| (k, e.sha256.to_lowercase()))).collect();
    if let Some(e) = m.entries.iter().find(|e| super::identity::key(&e.path).and_then(|k| here.get(&k)) != Some(&e.sha256.to_lowercase())) {
        log::info!("bus identity: {bus} ({paint}): {} here is not the server's; ours stays", e.path);
        return None;
    }
    Some((m, files))
}

/// (paths only in `theirs`, paths only in `ours`, paths in both with other content), each sorted.
/// Paths are matched by `identity::key` (case and spelling aside); map files are left out.
pub(super) fn differences(theirs: &[Entry], ours: &[Entry]) -> (Vec<String>, Vec<String>, Vec<String>) {
    let index = |v: &[Entry]| -> HashMap<String, (String, String)> {
        v.iter().filter_map(|e| super::identity::key(&e.path).filter(|k| !k.starts_with("maps/")).map(|k| (k, (e.path.clone(), e.sha256.to_lowercase())))).collect()
    };
    let (there, here) = (index(theirs), index(ours));
    let mut only_there: Vec<String> = there.iter().filter(|(k, _)| !here.contains_key(*k)).map(|(_, (p, _))| p.clone()).collect();
    let mut only_here: Vec<String> = here.iter().filter(|(k, _)| !there.contains_key(*k)).map(|(_, (p, _))| p.clone()).collect();
    let mut differ: Vec<String> = there.iter().filter(|(k, (_, h))| here.get(*k).is_some_and(|(_, ours)| ours != h)).map(|(_, (p, _))| p.clone()).collect();
    only_there.sort();
    only_here.sort();
    differ.sort();
    (only_there, only_here, differ)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(path: &str, sha: &str) -> Entry {
        Entry { path: path.into(), size: 1, sha256: sha.into() }
    }

    #[test]
    fn names_what_one_side_has_and_the_other_has_not() {
        let theirs = [e("Vehicles/A/a.bus", "11"), e("Vehicles/A/t.dds", "22"), e("Fonts/F.bmp", "33"), e("maps/M/global.cfg", "44")];
        let ours = [e("vehicles/a/A.BUS", "11"), e("Vehicles/A/t.dds", "99"), e("Vehicles/A/x.png", "55")];
        let (there, here, differ) = differences(&theirs, &ours);
        assert_eq!(there, vec!["Fonts/F.bmp".to_string()], "map files are no part of the bus");
        assert_eq!(here, vec!["Vehicles/A/x.png".to_string()]);
        assert_eq!(differ, vec!["Vehicles/A/t.dds".to_string()]);
    }
}
