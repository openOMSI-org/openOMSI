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
        "bus identity: {bus} ({paint}) is not the server's list ({} files there, {} here): {} only on the server [{}]; {} only here [{}]; {} with other content [{}]",
        theirs.len(), ours.len(), only_there.len(), names(&only_there), only_here.len(), names(&only_here), differ.len(), names(&differ)
    );
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
