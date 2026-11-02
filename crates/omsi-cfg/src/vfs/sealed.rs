//! Sealed stores: content kept on disk only in encrypted form, mounted as a folder.
//!
//! The mods a LAN host sends a joining player are never written in plain form: each file is
//! one *blob* of its own (ChaCha20-Poly1305 under the store's key, a random nonce, and the
//! caller's associated data - the file's SHA-256 - so a blob cannot stand in for another),
//! named by the caller (a keyed hash, never the original name). A mount maps the original
//! relative paths onto those blobs, and [`super::read`], [`super::exists`],
//! [`super::list_dir`] … read a mounted store like an unpacked folder: a file is decrypted
//! into memory when it is read, and the plain bytes go nowhere else.
//!
//! Blob layout: `OOSEAL1\n` (8 bytes), nonce (12), ciphertext, tag (16).

use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, RwLock};

const MAGIC: &[u8; 8] = b"OOSEAL1\n";
const TAG_LEN: usize = 16;

/// The bytes a blob has more than the file it holds.
pub const SEALED_OVERHEAD: u64 = (MAGIC.len() + NONCE_LEN + TAG_LEN) as u64;

/// A blob's size for a file of `plain` bytes.
pub fn sealed_len(plain: u64) -> u64 {
    plain + SEALED_OVERHEAD
}

/// 32 random bytes from the operating system (a key).
pub fn random_key() -> io::Result<[u8; 32]> {
    let mut k = [0u8; 32];
    SystemRandom::new()
        .fill(&mut k)
        .map_err(|_| io::Error::other("no random numbers from the system"))?;
    Ok(k)
}

/// The key a store's blobs are sealed with.
pub struct SealKey(LessSafeKey);

impl SealKey {
    pub fn new(bytes: &[u8; 32]) -> SealKey {
        SealKey(LessSafeKey::new(
            UnboundKey::new(&CHACHA20_POLY1305, bytes).expect("a 32-byte key"),
        ))
    }
}

impl std::fmt::Debug for SealKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SealKey(..)")
    }
}

/// Seal `plain` into a blob (`aad` is bound to it: [`unseal`] wants the same).
pub fn seal(key: &SealKey, aad: &[u8], plain: &[u8]) -> io::Result<Vec<u8>> {
    let mut nonce = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| io::Error::other("no random numbers from the system"))?;
    let mut out = Vec::with_capacity(sealed_len(plain.len() as u64) as usize);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(plain);
    let mut body = out.split_off(MAGIC.len() + NONCE_LEN);
    key.0
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad),
            &mut body,
        )
        .map_err(|_| io::Error::other("cannot seal"))?;
    out.append(&mut body);
    Ok(out)
}

/// The file a blob holds; an error for a blob that was changed, sealed with another key or
/// for other associated data.
pub fn unseal(key: &SealKey, aad: &[u8], mut blob: Vec<u8>) -> io::Result<Vec<u8>> {
    if blob.len() < SEALED_OVERHEAD as usize || &blob[..MAGIC.len()] != MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a sealed blob",
        ));
    }
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&blob[MAGIC.len()..MAGIC.len() + NONCE_LEN]);
    let start = MAGIC.len() + NONCE_LEN;
    let n = key
        .0
        .open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad),
            &mut blob[start..],
        )
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "a sealed blob does not open (changed, or another key)",
            )
        })?
        .len();
    blob.copy_within(start..start + n, 0);
    blob.truncate(n);
    Ok(blob)
}

/// One file of a sealed mount.
#[derive(Debug, Clone)]
pub struct SealedFile {
    /// Path relative to the mount (`/`-separated, original spelling).
    pub rel: String,
    /// The blob on disk.
    pub blob: PathBuf,
    /// What the blob was sealed for (see [`seal`]).
    pub aad: Vec<u8>,
    /// The file's size.
    pub size: u64,
}

pub(super) struct SealedMount {
    pub(super) path: PathBuf,
    key: Option<Arc<SealKey>>,
    publisher: Option<Arc<omsi_pack::VerifiedPack>>,
    /// Files by lower-case relative path.
    entries: HashMap<String, SealedFile>,
    /// Folder contents by lower-case folder path ("" = the root): (name as given, is folder).
    pub(super) dirs: HashMap<String, Vec<(String, bool)>>,
}

impl SealedMount {
    pub(super) fn read(&self, key: &str) -> io::Result<Vec<u8>> {
        let f = self.entries.get(key).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{}: no file {key}", self.path.display()),
            )
        })?;
        if let Some(pack) = &self.publisher {
            return pack
                .read(&f.blob.to_string_lossy())
                .map_err(io::Error::other);
        }
        let key = self
            .key
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "sealed store has no key"))?;
        let plain = unseal(key, &f.aad, std::fs::read(&f.blob)?).map_err(|e| {
            io::Error::new(e.kind(), format!("{}/{}: {e}", self.path.display(), f.rel))
        })?;
        if plain.len() as u64 != f.size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{}/{}: {} bytes, {} expected",
                    self.path.display(),
                    f.rel,
                    plain.len(),
                    f.size
                ),
            ));
        }
        Ok(plain)
    }

    pub(super) fn size(&self, key: &str) -> Option<u64> {
        self.entries.get(key).map(|f| f.size)
    }

    pub(super) fn is_file(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    pub(super) fn is_dir(&self, key: &str) -> bool {
        key.is_empty() || self.dirs.contains_key(key)
    }

    pub(super) fn list(&self, key: &str) -> Option<Vec<(OsString, bool)>> {
        self.dirs
            .get(key)
            .map(|l| l.iter().map(|(n, d)| (OsString::from(n), *d)).collect())
    }
}

static SEALED: RwLock<Vec<Arc<SealedMount>>> = RwLock::new(Vec::new());

/// Mount `files` (sealed with `key`) as the folder `mount`, which need not exist on disk;
/// a store mounted there before is replaced. Returns the mount point.
pub fn mount_sealed(mount: &Path, key: Arc<SealKey>, files: Vec<SealedFile>) -> PathBuf {
    mount_store(mount, Some(key), files, None)
}

/// Mount an authenticated publisher pack without writing decoded content to disk.
pub fn mount_publisher(
    mount: &Path,
    pack: Arc<omsi_pack::VerifiedPack>,
    aliases: std::collections::BTreeMap<String, String>,
) -> io::Result<PathBuf> {
    let aliases = if aliases.is_empty() {
        pack.assets()
            .iter()
            .map(|a| (a.path.clone(), a.path.clone()))
            .collect()
    } else {
        aliases
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut files = Vec::new();
    for (alias, original) in aliases {
        if alias.is_empty()
            || alias.len() > 1024
            || alias.contains(['\\', ':', '\0'])
            || alias
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == ".." || p.ends_with(['.', ' ']))
            || !seen.insert(alias.to_lowercase())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsafe or duplicate publisher asset alias",
            ));
        }
        let asset = pack
            .assets()
            .iter()
            .find(|a| a.path == original)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "publisher asset alias was not found",
                )
            })?;
        files.push(SealedFile {
            rel: alias,
            blob: PathBuf::from(original),
            aad: Vec::new(),
            size: asset.size,
        });
    }
    Ok(mount_store(mount, None, files, Some(pack)))
}

fn mount_store(
    mount: &Path,
    key: Option<Arc<SealKey>>,
    files: Vec<SealedFile>,
    publisher: Option<Arc<omsi_pack::VerifiedPack>>,
) -> PathBuf {
    let mut m = SealedMount {
        path: mount.to_path_buf(),
        key,
        publisher,
        entries: HashMap::new(),
        dirs: HashMap::new(),
    };
    m.dirs.insert(String::new(), Vec::new());
    for f in files {
        let parts: Vec<&str> = f
            .rel
            .split(['/', '\\'])
            .filter(|c| !c.is_empty() && *c != "." && *c != "..")
            .collect();
        if parts.is_empty() {
            continue;
        }
        let lower: Vec<String> = parts.iter().map(|c| c.to_lowercase()).collect();
        for k in 0..parts.len() {
            let parent = lower[..k].join("/");
            let here = lower[..=k].join("/");
            let is_dir = k + 1 < parts.len();
            if is_dir && m.dirs.contains_key(&here) || !is_dir && m.entries.contains_key(&here) {
                continue;
            }
            m.dirs
                .entry(parent)
                .or_default()
                .push((parts[k].to_string(), is_dir));
            if is_dir {
                m.dirs.insert(here, Vec::new());
            }
        }
        m.entries.insert(lower.join("/"), f);
    }
    log::info!(
        "sealed store {}: {} files mounted",
        mount.display(),
        m.entries.len()
    );
    let mut all = SEALED.write().unwrap();
    all.retain(|x| x.path != mount);
    all.push(Arc::new(m));
    mount.to_path_buf()
}

/// Take the store mounted at `mount` away.
pub fn unmount_sealed(mount: &Path) {
    SEALED.write().unwrap().retain(|x| x.path != mount);
}

/// Does `path` lie in a sealed mount (content that must never be written out in plain
/// form, nor anything derived from it be cached on disk)?
pub fn is_sealed(path: &Path) -> bool {
    locate(path).is_some_and(|(m, _)| {
        m.publisher
            .as_ref()
            .is_none_or(|p| p.manifest().key_id.is_some())
    })
}

/// The sealed mount `path` lies in, and its lower-case path inside it.
pub(super) fn locate(path: &Path) -> Option<(Arc<SealedMount>, String)> {
    let mounts = SEALED.read().unwrap();
    if mounts.is_empty() {
        return None;
    }
    for m in mounts.iter() {
        if let Ok(rest) = path.strip_prefix(&m.path) {
            let mut parts: Vec<String> = Vec::new();
            for c in rest.components() {
                match c {
                    Component::Normal(s) => parts.push(s.to_string_lossy().to_lowercase()),
                    Component::ParentDir => {
                        parts.pop();
                    }
                    _ => {}
                }
            }
            return Some((m.clone(), parts.join("/")));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKER: &[u8] = b"PLAINTEXT-MARKER-7f3a";

    /// A store of two files in a temporary folder, blobs named by number.
    fn store(dir: &Path, key: &SealKey) -> Vec<SealedFile> {
        std::fs::create_dir_all(dir).unwrap();
        let files: [(&str, Vec<u8>); 3] = [
            (
                "maps/Test Map/global.cfg",
                [b"[name]\r\n".as_slice(), MARKER, b"\r\n"].concat(),
            ),
            (
                "Sceneryobjects/Stra\u{df}e/Schild.sco",
                [b"[mesh]\r\n".as_slice(), MARKER].concat(),
            ),
            ("maps/Test Map/tile_0_0.map", MARKER.repeat(100)),
        ];
        files
            .iter()
            .enumerate()
            .map(|(i, (rel, data))| {
                let aad = format!("file {i}").into_bytes();
                let blob = dir.join(format!("{i:064x}"));
                std::fs::write(&blob, seal(key, &aad, data).unwrap()).unwrap();
                SealedFile {
                    rel: rel.to_string(),
                    blob,
                    aad,
                    size: data.len() as u64,
                }
            })
            .collect()
    }

    #[test]
    fn sealed_mount_reads_like_a_folder() {
        let dir = std::env::temp_dir().join(format!("omsi-cfg-sealed-{}", std::process::id()));
        let key = Arc::new(SealKey::new(&random_key().unwrap()));
        let files = store(&dir.join("blobs"), &key);
        // nothing on disk carries a plain byte of the files, nor their names
        for e in std::fs::read_dir(dir.join("blobs")).unwrap().flatten() {
            let b = std::fs::read(e.path()).unwrap();
            assert!(
                !b.windows(MARKER.len()).any(|w| w == MARKER),
                "plain text in {}",
                e.path().display()
            );
            assert!(!e.file_name().to_string_lossy().contains("global"));
            assert_eq!(&b[..8], MAGIC);
        }
        let mount = mount_sealed(&dir.join("session"), key.clone(), files.clone());
        assert!(!mount.exists(), "the mount is no folder on disk");
        let cfg = mount.join("MAPS").join("test map").join("Global.CFG");
        assert!(super::super::is_file(&cfg));
        assert!(super::super::exists(&cfg));
        assert_eq!(
            super::super::read(&cfg).unwrap(),
            [b"[name]\r\n".as_slice(), MARKER, b"\r\n"].concat()
        );
        assert!(
            super::super::read_text(&cfg)
                .unwrap()
                .contains("PLAINTEXT-MARKER")
        );
        assert_eq!(
            super::super::file_size(&cfg),
            Some(MARKER.len() as u64 + 10)
        );
        assert!(super::super::is_dir(&mount.join("maps")));
        assert!(super::super::is_dir(&mount));
        assert!(!super::super::exists(&mount.join("Vehicles")));
        let top: Vec<String> = super::super::list_dir(&mount)
            .unwrap()
            .into_iter()
            .map(|(n, _)| n.to_string_lossy().into_owned())
            .collect();
        assert_eq!(top, ["maps", "Sceneryobjects"]);
        let map: Vec<(String, bool)> = super::super::list_dir(&mount.join("maps/test map"))
            .unwrap()
            .into_iter()
            .map(|(n, d)| (n.to_string_lossy().into_owned(), d))
            .collect();
        assert_eq!(
            map,
            [
                ("global.cfg".to_string(), false),
                ("tile_0_0.map".to_string(), false)
            ]
        );
        // the original spelling, found the way Windows finds it
        let sco = crate::resolve_path(&mount, "Sceneryobjects\\straße\\schild.sco");
        assert!(super::super::is_file(&sco), "{}", sco.display());
        assert!(is_sealed(&sco));
        let f = crate::CfgFile::read(&cfg).unwrap();
        assert_eq!(f.lines[0], "[name]");

        // a changed blob, or one swapped for another, is refused
        let mut b = std::fs::read(&files[2].blob).unwrap();
        b[40] ^= 1;
        std::fs::write(&files[2].blob, &b).unwrap();
        assert!(super::super::read(&mount.join("maps/Test Map/tile_0_0.map")).is_err());
        std::fs::copy(&files[1].blob, &files[0].blob).unwrap();
        assert!(super::super::read(&cfg).is_err());
        // and so is the right blob under another key
        let other = Arc::new(SealKey::new(&random_key().unwrap()));
        let fresh = store(&dir.join("blobs2"), &key);
        mount_sealed(&mount, other, fresh);
        assert!(super::super::read(&cfg).is_err());

        unmount_sealed(&mount);
        assert!(!super::super::exists(&cfg));
        assert!(!is_sealed(&cfg));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn seal_round_trip() {
        let key = SealKey::new(&[7; 32]);
        let blob = seal(&key, b"a", b"hello").unwrap();
        assert_eq!(blob.len() as u64, sealed_len(5));
        assert_eq!(unseal(&key, b"a", blob.clone()).unwrap(), b"hello");
        assert!(unseal(&key, b"b", blob.clone()).is_err());
        assert!(unseal(&key, b"a", blob[..10].to_vec()).is_err());
        // a fresh nonce every time
        assert_ne!(seal(&key, b"a", b"hello").unwrap(), blob);
        assert_eq!(
            unseal(&key, b"", seal(&key, b"", b"").unwrap()).unwrap(),
            b""
        );
    }
}
