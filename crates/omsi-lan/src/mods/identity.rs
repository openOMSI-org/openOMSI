//! What a bus is, by its content: the list of every file it uses (path, size, SHA-256) and
//! one hash over that list - its fingerprint. Two players whose buses have the same
//! fingerprint see the same bus, file for file; a different version of the same mod, or
//! another paint, is another fingerprint. The path a player's game names the bus by is only
//! for showing.
//!
//! The list is hashed in one exact form (format v2), so that every machine gets the same
//! fingerprint for the same files, as Git's tree objects and Nix's NAR archives do:
//!
//! ```text
//! "omsi-vehicle-manifest/v2\0sha256\0"
//! u32 LE  length of the bus key
//! key     the bus file's path (which file of the set is the bus), as a key
//! for each file, ordered by the bytes of its key:
//!     u32 LE  length of the key
//!     key     the path from the game's root, `/`-separated, `.`/`..` resolved, lower case,
//!             in Unicode's composed form (NFC, `omsi_cfg::name_key`)
//!     u64 LE  size
//!     32 B    SHA-256 of the file
//! fingerprint = "v2-" + hex(SHA-256 of all of the above)
//! ```
//!
//! The bus key is hashed too (format v2): the same files with a different `bus` are a
//! different bus, so no one can claim a popular bus's fingerprint by listing its files with
//! `bus` pointing at another of them (which would leave every viewer a box). Older `v1-`
//! fingerprints - which did not hash the bus - are still recognised on the wire, but no
//! longer produced; a peer that announces one is served as a peer with no fingerprint is.
//!
//! The key is in lower case because OMSI runs on Windows, where `Texture\Bus.dds` and
//! `texture\bus.dds` are one file: two entries that are one file there make the list
//! invalid (git's CVE-2014-9390 came from such a pair). Times are left out: the same files
//! copied elsewhere keep their fingerprint.

use super::Entry;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

const HEADER: &[u8] = b"omsi-vehicle-manifest/v2\0sha256\0";
const PREFIX: &str = "v2-";

/// The files of one bus and the bus file itself (its path from the game's root).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VehicleManifest {
    pub bus: String,
    pub entries: Vec<Entry>,
}

/// `Vehicles\X\..\Y/bus.BUS` → `vehicles/y/bus.bus`; None when it leaves the root or is empty.
pub fn key(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for c in path.split(['/', '\\']) {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            c => parts.push(c),
        }
    }
    // (lower case and composed - NFC - as the game compares names: `Bahnübergang.dds` copied
    // from a Mac, where it is often stored with `ü` as `u` + a combining diaeresis, is the
    // same file, not a second fingerprint of the same bus)
    (!parts.is_empty()).then(|| omsi_cfg::name_key(&parts.join("/")))
}

impl VehicleManifest {
    /// Roughly the memory the list takes in this game, or written out as JSON: each entry its
    /// path, a 64-char hash and a size, and the overhead around them (the Vec's, and a kept
    /// offer's lease and `wanted` set).
    pub(crate) fn memory(&self) -> u64 {
        self.entries.iter().map(|e| e.path.len() as u64 + 160).sum()
    }

    /// The list checked and put in order: every path a key, no two the same file, the hashes
    /// 64 hex digits, the bus file among the entries.
    pub fn validated(mut self) -> Result<VehicleManifest, String> {
        let mut seen = HashSet::new();
        for e in &mut self.entries {
            let k =
                key(&e.path).ok_or_else(|| format!("{}: not a path inside the game", e.path))?;
            if !seen.insert(k.clone()) {
                return Err(format!("{} is listed twice (once in another case)", e.path));
            }
            if e.sha256.len() != 64 || !e.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!("{}: no SHA-256", e.path));
            }
            e.sha256.make_ascii_lowercase();
            e.path = e.path.replace('\\', "/");
        }
        let bus = key(&self.bus).ok_or("no bus file")?;
        if !seen.contains(&bus) {
            return Err(format!("{} is not among its files", self.bus));
        }
        self.entries
            .sort_by_cached_key(|e| key(&e.path).unwrap_or_default().into_bytes());
        Ok(self)
    }

    /// The fingerprint of a validated list.
    pub fn fingerprint(&self) -> String {
        let mut h = Sha256::new();
        h.update(HEADER);
        // (which file of the set is the bus is part of what the bus is: v2)
        let bus = key(&self.bus).unwrap_or_default();
        h.update((bus.len() as u32).to_le_bytes());
        h.update(bus.as_bytes());
        let mut keyed: Vec<(String, &Entry)> = self
            .entries
            .iter()
            .map(|e| (key(&e.path).unwrap_or_default(), e))
            .collect();
        keyed.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        for (k, e) in keyed {
            h.update((k.len() as u32).to_le_bytes());
            h.update(k.as_bytes());
            h.update(e.size.to_le_bytes());
            let mut raw = [0u8; 32];
            for (i, b) in raw.iter_mut().enumerate() {
                *b = u8::from_str_radix(e.sha256.get(i * 2..i * 2 + 2).unwrap_or("00"), 16)
                    .unwrap_or(0);
            }
            h.update(raw);
        }
        let digest = h.finalize();
        format!(
            "{PREFIX}{}",
            digest
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
    }

    /// The bytes all its files take.
    pub fn size(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(path: &str, data: &[u8]) -> Entry {
        Entry {
            path: path.into(),
            size: data.len() as u64,
            sha256: super::super::sha256_of(data),
        }
    }

    fn bus(entries: Vec<Entry>) -> VehicleManifest {
        VehicleManifest {
            bus: "Vehicles/B/b.bus".into(),
            entries,
        }
    }

    #[test]
    fn keys_are_the_file_windows_opens() {
        assert_eq!(
            key("Vehicles\\MAN\\model\\..\\..\\Anzeigen\\X.o3d").as_deref(),
            Some("vehicles/anzeigen/x.o3d")
        );
        assert_eq!(
            key("./Vehicles//B/b.BUS").as_deref(),
            Some("vehicles/b/b.bus")
        );
        assert_eq!(key("Vehicles/../../x"), None);
        assert_eq!(key(""), None);
    }

    #[test]
    fn the_same_files_give_the_same_fingerprint_in_any_order_and_spelling() {
        let a = bus(vec![
            e("Vehicles/B/b.bus", b"bus"),
            e("Vehicles/B/t.dds", b"tex"),
        ])
        .validated()
        .unwrap();
        let b = VehicleManifest {
            bus: "vehicles\\b\\B.bus".into(),
            entries: vec![
                e("vehicles\\b\\T.DDS", b"tex"),
                e("VEHICLES/B/B.BUS", b"bus"),
            ],
        }
        .validated()
        .unwrap();
        assert_eq!(a.fingerprint(), b.fingerprint());
        // (in the form the protocol takes: omsi_net::is_fingerprint)
        assert!(omsi_net::is_fingerprint(&a.fingerprint()));
        assert_eq!(a.size(), 6);
    }

    #[test]
    fn any_change_is_another_fingerprint() {
        let base = bus(vec![
            e("Vehicles/B/b.bus", b"bus"),
            e("Vehicles/B/t.dds", b"tex"),
        ])
        .validated()
        .unwrap()
        .fingerprint();
        // another texture (a paint), another file, a file moved
        for other in [
            bus(vec![
                e("Vehicles/B/b.bus", b"bus"),
                e("Vehicles/B/t.dds", b"TEX"),
            ]),
            bus(vec![
                e("Vehicles/B/b.bus", b"bus"),
                e("Vehicles/B/t.dds", b"tex"),
                e("Vehicles/B/n.dds", b"night"),
            ]),
            bus(vec![
                e("Vehicles/B/b.bus", b"bus"),
                e("Vehicles/B/x/t.dds", b"tex"),
            ]),
        ] {
            assert_ne!(other.validated().unwrap().fingerprint(), base);
        }
        // (a key and a size cannot be mistaken for another key: both are delimited)
        let one = bus(vec![
            e("Vehicles/B/b.bus", b"bus"),
            e("Vehicles/B/ab", b"x"),
        ])
        .validated()
        .unwrap()
        .fingerprint();
        let two = bus(vec![e("Vehicles/B/b.bus", b"bus"), e("Vehicles/B/a", b"x")])
            .validated()
            .unwrap()
            .fingerprint();
        assert_ne!(one, two);
    }

    /// A name stored decomposed (NFD, as a Mac often keeps `ü`: `u` + U+0308) is the same file
    /// as the same name composed: one bus, one fingerprint, on either machine.
    #[test]
    fn a_name_in_either_unicode_form_is_one_key() {
        let composed = "Vehicles/B/Bahn\u{00fc}bergang.dds";
        let decomposed = "Vehicles/B/Bahnu\u{0308}bergang.dds";
        assert_ne!(composed, decomposed);
        assert_eq!(key(composed), key(decomposed));
        let fp = |p: &str| {
            bus(vec![e("Vehicles/B/b.bus", b"bus"), e(p, b"tex")])
                .validated()
                .unwrap()
                .fingerprint()
        };
        assert_eq!(fp(composed), fp(decomposed));
        // (and the two spellings in one list are one file listed twice)
        assert!(
            bus(vec![
                e("Vehicles/B/b.bus", b"bus"),
                e(composed, b"1"),
                e(decomposed, b"2")
            ])
            .validated()
            .is_err()
        );
    }

    /// The same files with a different `bus` are a different bus (v2): a player cannot claim a
    /// popular bus's fingerprint by listing its files with `bus` pointing at another of them.
    #[test]
    fn the_bus_file_is_part_of_the_fingerprint() {
        let files = vec![
            e("Vehicles/B/b.bus", b"[model]\nmodel.cfg\n"),
            e("Vehicles/B/other.bus", b"[model]\nother.cfg\n"),
            e("Vehicles/B/t.dds", b"tex"),
        ];
        let a = VehicleManifest {
            bus: "Vehicles/B/b.bus".into(),
            entries: files.clone(),
        }
        .validated()
        .unwrap();
        let b = VehicleManifest {
            bus: "Vehicles/B/other.bus".into(),
            entries: files,
        }
        .validated()
        .unwrap();
        assert_ne!(
            a.fingerprint(),
            b.fingerprint(),
            "same files, another bus file: another fingerprint"
        );
        // (the form the protocol takes - v2 now)
        assert!(a.fingerprint().starts_with("v2-") && omsi_net::is_fingerprint(&a.fingerprint()));
    }

    #[test]
    fn lists_that_are_not_one_bus_are_refused() {
        // one file twice on Windows
        assert!(
            bus(vec![
                e("Vehicles/B/b.bus", b"bus"),
                e("Vehicles/B/T.dds", b"1"),
                e("vehicles/b/t.DDS", b"2")
            ])
            .validated()
            .is_err()
        );
        // outside the game
        assert!(
            bus(vec![e("Vehicles/B/b.bus", b"bus"), e("../x.dds", b"1")])
                .validated()
                .is_err()
        );
        // without its bus file
        assert!(bus(vec![e("Vehicles/B/t.dds", b"1")]).validated().is_err());
        // no hash
        assert!(
            bus(vec![Entry {
                path: "Vehicles/B/b.bus".into(),
                size: 1,
                sha256: "x".into()
            }])
            .validated()
            .is_err()
        );
    }
}
