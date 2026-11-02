//! The joining player's store of the host's mods (`~/.openomsi/lan-store`), kept between
//! sessions so a map fetched once is not fetched again: every file only sealed (see
//! `omsi_cfg::vfs::seal`), its blob named by a keyed hash of its SHA-256 - neither its name
//! nor its hash can be read off the disk.
//!
//! The store's key has to outlive the game, so it is on the disk as well: on Windows sealed
//! for the user's account by the system (DPAPI), elsewhere in a file only the user may read
//! (`store.key`, mode 0600). That is obfuscation, not protection from the user: whoever runs
//! as the player can read the key, and openOMSI's source says what to do with it. (The
//! macOS keychain would ask the player to allow every new build of the game.) A key that is
//! lost or unreadable gives a new one, and the blobs of the old one go.

use super::Entry;
use omsi_cfg::vfs::{SealKey, SealedFile};
use ring::hkdf::{KeyType, Salt, HKDF_SHA256};
use ring::hmac;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Free bytes for this user on the volume of `path` (the nearest existing ancestor).
pub(crate) fn free_space(path: &Path) -> Option<u64> {
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

#[cfg(windows)]
const KEY_FILE: &str = "store.key.dpapi";
#[cfg(not(windows))]
const KEY_FILE: &str = "store.key";
const BLOB: &str = "blob";
const PIECE: &str = "piece";
const PACKED: &str = "zpiece";

#[derive(Clone)]
pub(crate) struct Store {
    dir: PathBuf,
    key: Arc<SealKey>,
    names: hmac::Key,
}

struct Len32;

impl KeyType for Len32 {
    fn len(&self) -> usize {
        32
    }
}

impl Store {
    /// The store in `dir` (made when missing), with its key.
    pub(crate) fn open(dir: &Path) -> Result<Store, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        remove_legacy(dir);
        let key_path = dir.join(KEY_FILE);
        let master = match load_key(&key_path) {
            Some(k) => k,
            None => {
                // (blobs of a key that is gone can never be read again)
                let removed = remove_blobs(dir);
                if removed > 0 {
                    log::info!("LAN mods: the store's key was missing; {removed} files of the old one removed");
                }
                let k = omsi_cfg::vfs::random_key().map_err(|e| e.to_string())?;
                save_key(&key_path, &k).map_err(|e| format!("{}: {e}", key_path.display()))?;
                k
            }
        };
        // one key for the blobs, one for their names, both from the store's key
        let prk = Salt::new(HKDF_SHA256, b"openOMSI LAN store").extract(&master);
        let sub = |what: &[u8]| -> Result<[u8; 32], String> {
            let mut k = [0u8; 32];
            prk.expand(&[what], Len32).and_then(|o| o.fill(&mut k)).map_err(|_| "no store key".to_string())?;
            Ok(k)
        };
        Ok(Store { dir: dir.to_path_buf(), key: Arc::new(SealKey::new(&sub(b"blobs")?)), names: hmac::Key::new(hmac::HMAC_SHA256, &sub(b"names")?) })
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn key(&self) -> Arc<SealKey> {
        self.key.clone()
    }

    fn file(&self, e: &Entry) -> SealedFile {
        let sha = e.sha256.to_ascii_lowercase();
        let name: String = hmac::sign(&self.names, sha.as_bytes()).as_ref().iter().map(|b| format!("{b:02x}")).collect();
        SealedFile { rel: e.path.clone(), blob: self.dir.join(format!("{name}.{BLOB}")), aad: sha.into_bytes(), size: e.size }
    }

    /// `e`'s file, if a join before fetched it.
    pub(crate) fn has(&self, e: &Entry) -> Option<SealedFile> {
        let f = self.file(e);
        let md = std::fs::metadata(&f.blob).ok()?;
        if md.len() != omsi_cfg::vfs::sealed_len(e.size) {
            return None;
        }
        // Eviction is oldest-accessed-first, not oldest-downloaded-first:
        // an old bus/map used regularly should not be fetched again unnecessarily.
        // Touch at most once an hour to avoid per-read disk writes.
        if md.modified().ok().and_then(|t| t.elapsed().ok())
            .is_none_or(|age| age > std::time::Duration::from_secs(3600))
        {
            let _ = std::fs::File::options().write(true).open(&f.blob)
                .and_then(|file| file.set_modified(std::time::SystemTime::now()));
        }
        Some(f)
    }

    /// `e`'s file read back (unsealed and checked: the seal is bound to its SHA-256).
    pub(crate) fn read(&self, e: &Entry) -> std::io::Result<Vec<u8>> {
        let f = self.has(e).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "not in the store"))?;
        let plain = omsi_cfg::vfs::unseal(&self.key, &f.aad, std::fs::read(&f.blob)?)?;
        if plain.len() as u64 != e.size {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "another size than its list's"));
        }
        Ok(plain)
    }

    /// Keep `data` (checked against `e` already) sealed.
    pub(crate) fn put(&self, e: &Entry, data: &[u8]) -> std::io::Result<SealedFile> {
        let f = self.file(e);
        let sealed = omsi_cfg::vfs::seal(&self.key, &f.aad, data)?;
        let tmp = temp_of(&f.blob, "part");
        std::fs::write(&tmp, sealed).and_then(|_| std::fs::rename(&tmp, &f.blob))?;
        Ok(f)
    }

    /// Piece `i` of the file `sha` (see `pieces`): where it is kept, and what its seal is
    /// bound to - that file and that place in it, so no piece passes for another.
    fn piece_file(&self, sha: &str, i: u64) -> (PathBuf, Vec<u8>) {
        let aad = format!("{}:{i}", sha.to_ascii_lowercase()).into_bytes();
        let name: String = hmac::sign(&self.names, &aad).as_ref().iter().map(|b| format!("{b:02x}")).collect();
        (self.dir.join(format!("{name}.{PIECE}")), aad)
    }

    /// Is piece `i` (`len` bytes) of `sha` here?
    pub(crate) fn has_piece(&self, sha: &str, i: u64, len: u64) -> bool {
        std::fs::metadata(self.piece_file(sha, i).0).is_ok_and(|m| m.len() == omsi_cfg::vfs::sealed_len(len))
    }

    /// Keep piece `i` of `sha` sealed, as it came (its file is checked once it is whole).
    pub(crate) fn put_piece(&self, sha: &str, i: u64, data: &[u8]) -> std::io::Result<()> {
        let (path, aad) = self.piece_file(sha, i);
        let sealed = omsi_cfg::vfs::seal(&self.key, &aad, data)?;
        let tmp = temp_of(&path, "partpiece");
        std::fs::write(&tmp, sealed).and_then(|_| std::fs::rename(&tmp, &path))
    }

    /// Piece `i` of `sha` read back (unsealed: it is the piece kept for that place).
    pub(crate) fn piece(&self, sha: &str, i: u64, len: u64) -> std::io::Result<Vec<u8>> {
        let (path, aad) = self.piece_file(sha, i);
        let plain = omsi_cfg::vfs::unseal(&self.key, &aad, std::fs::read(path)?)?;
        if plain.len() as u64 != len {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "a piece of another size"));
        }
        Ok(plain)
    }

    /// The pieces of `sha` (`count` of them) go: its file is kept whole, or was not that file.
    pub(crate) fn remove_pieces(&self, sha: &str, count: u64) {
        for i in 0..count {
            let _ = std::fs::remove_file(self.piece_file(sha, i).0);
        }
    }

    /// Piece `i` of the kept file `sha` packed at its smallest, as it goes over the wire (see
    /// `vehicles::recompress`): where it is kept, and what its seal is bound to.
    fn packed_file(&self, sha: &str, i: u64) -> (PathBuf, Vec<u8>) {
        let aad = format!("{}:{i}:packed", sha.to_ascii_lowercase()).into_bytes();
        let name: String = hmac::sign(&self.names, &aad).as_ref().iter().map(|b| format!("{b:02x}")).collect();
        (self.dir.join(format!("{name}.{PACKED}")), aad)
    }

    /// Keep piece `i` of `sha` packed (`codec` `-` with no bytes: it goes as it is).
    pub(crate) fn put_packed(&self, sha: &str, i: u64, codec: char, wire: &[u8]) -> std::io::Result<()> {
        let (path, aad) = self.packed_file(sha, i);
        let mut data = Vec::with_capacity(1 + wire.len());
        data.push(codec as u8);
        data.extend_from_slice(wire);
        let sealed = omsi_cfg::vfs::seal(&self.key, &aad, &data)?;
        let tmp = temp_of(&path, "partpiece");
        std::fs::write(&tmp, sealed).and_then(|_| std::fs::rename(&tmp, &path))
    }

    /// Piece `i` of `sha` packed, if it is: (codec, bytes on the wire).
    pub(crate) fn packed(&self, sha: &str, i: u64) -> Option<(char, Vec<u8>)> {
        let (path, aad) = self.packed_file(sha, i);
        let mut data = omsi_cfg::vfs::unseal(&self.key, &aad, std::fs::read(path).ok()?).ok()?;
        let codec = *data.first()? as char;
        data.remove(0);
        Some((codec, data))
    }

    /// The packed pieces of `sha` (`count` of them) go with its file.
    pub(crate) fn remove_packed(&self, sha: &str, count: u64) {
        for i in 0..count {
            let _ = std::fs::remove_file(self.packed_file(sha, i).0);
        }
    }

    /// Loose pieces (of files not finished yet, so not checked yet) go, the least lately
    /// written first, until `need` bytes are freed - but none written within `grace` (an
    /// upload under way is still writing them; one that loses a piece sends it again, the
    /// transfer is resumable). The bytes freed. Pieces are kept before their file can be
    /// checked, so without this a player could fill the disk with them: `make_room` only let
    /// whole buses go.
    pub(crate) fn remove_pieces_for_room(&self, need: u64, grace: std::time::Duration) -> u64 {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return 0 };
        let mut loose: Vec<(std::time::SystemTime, u64, PathBuf)> = rd
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == PIECE || x == "partpiece"))
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                let t = m.modified().ok()?;
                t.elapsed().ok().filter(|d| *d >= grace)?;
                Some((t, m.len(), e.path()))
            })
            .collect();
        loose.sort_by_key(|(t, _, _)| *t);
        let mut freed = 0;
        for (_, len, path) in loose {
            if freed >= need {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                freed += len;
            }
        }
        freed
    }

    /// Pieces of files left unfinished for longer than `age` go. How many.
    pub(crate) fn remove_stale_pieces(&self, age: std::time::Duration) -> usize {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return 0 };
        let old = |e: &std::fs::DirEntry| e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|d| d > age);
        rd.flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == PIECE || x == "partpiece") && old(e) && std::fs::remove_file(e.path()).is_ok())
            .count()
    }
}

/// Where a file of the store is written before it takes its name: a name of its own for each
/// write (`<name>.<pid>-<n>.<ext>`). Two connections putting the same file at once - the same
/// texture of two buses, or one bus sent twice - wrote into one temporary file and could
/// leave it mixed. The extension stays, so what is left of a write that broke off is still
/// cleared as before.
fn temp_of(path: &Path, ext: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    path.with_extension(format!("{}-{n}.{ext}", std::process::id()))
}

/// What a game before the encryption kept in the store: the plain files, named by their
/// SHA-256, and half-written ones.
pub(crate) fn remove_legacy(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut n = 0;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let plain = name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit());
        if (plain || name.ends_with(".part")) && std::fs::remove_file(e.path()).is_ok() {
            n += 1;
        }
    }
    if n > 0 {
        log::info!("LAN mods: {n} unencrypted files of an older version removed from the store");
    }
}

fn remove_blobs(dir: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    rd.flatten().filter(|e| e.path().extension().is_some_and(|x| x == BLOB || x == PIECE || x == PACKED) && std::fs::remove_file(e.path()).is_ok()).count()
}

fn load_key(path: &Path) -> Option<[u8; 32]> {
    let raw = std::fs::read(path).ok()?;
    #[cfg(windows)]
    let raw = dpapi(&raw, false)?;
    raw.try_into().ok()
}

fn save_key(path: &Path, key: &[u8; 32]) -> std::io::Result<()> {
    #[cfg(windows)]
    let bytes = dpapi(key, true).ok_or_else(|| std::io::Error::other("the system did not protect the key"))?;
    #[cfg(not(windows))]
    let bytes = key.to_vec();
    let tmp = path.with_extension("new");
    let _ = std::fs::remove_file(&tmp);
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    std::io::Write::write_all(&mut o.open(&tmp)?, &bytes)?;
    std::fs::rename(&tmp, path)
}

/// Seal (`protect`) or open `data` for this Windows user account.
#[cfg(windows)]
fn dpapi(data: &[u8], protect: bool) -> Option<Vec<u8>> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `input` points at `data` for the call; `out` is the system's buffer, copied
    // and then freed with LocalFree as the documentation asks
    unsafe {
        let r = if protect {
            CryptProtectData(&input, windows::core::w!("openOMSI LAN store"), None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        } else {
            CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        };
        r.ok()?;
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Loose pieces - kept before their file can be checked - go when room is needed, the
    /// least lately written first, but never one written within the grace (an upload under
    /// way): a player could otherwise fill the disk with pieces of files never finished.
    #[test]
    fn loose_pieces_go_oldest_first_to_make_room_but_not_recent_ones() {
        let dir = std::env::temp_dir().join(format!("openomsi-lan-pieces-room-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir).unwrap();
        let piece = vec![7u8; 4096];
        let (old, older, young) = ("a".repeat(64), "b".repeat(64), "c".repeat(64));
        for sha in [&old, &older, &young] {
            store.put_piece(sha, 0, &piece).unwrap();
        }
        let now = std::time::SystemTime::now();
        let age = |sha: &str, secs: u64| {
            let f = std::fs::File::options().write(true).open(store.piece_file(sha, 0).0).unwrap();
            f.set_modified(now - std::time::Duration::from_secs(secs)).unwrap();
        };
        age(&older, 3 * 3600);
        age(&old, 2 * 3600);
        age(&young, 60);
        let grace = std::time::Duration::from_secs(600);
        // room for one piece: the least lately written goes, the others stay
        assert!(store.remove_pieces_for_room(1, grace) > 0);
        assert!(!store.has_piece(&older, 0, piece.len() as u64), "the oldest went first");
        assert!(store.has_piece(&old, 0, piece.len() as u64) && store.has_piece(&young, 0, piece.len() as u64));
        // asked for far more: everything past the grace goes, the piece of an upload under way stays
        store.remove_pieces_for_room(u64::MAX, grace);
        assert!(!store.has_piece(&old, 0, piece.len() as u64));
        assert!(store.has_piece(&young, 0, piece.len() as u64), "a piece written a minute ago is spared");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reused_blob_updates_eviction_age_without_changing_content() {
        let dir = std::env::temp_dir().join(format!("openomsi-lan-reuse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(&dir).unwrap();
        let old_data = b"bus file used again";
        let other_data = b"another bus file";
        let old = Entry { path: "vehicles/old.bus".into(), size: old_data.len() as u64,
            sha256: super::super::sha256_of(old_data) };
        let other = Entry { path: "vehicles/other.bus".into(), size: other_data.len() as u64,
            sha256: super::super::sha256_of(other_data) };
        let old_file = store.put(&old, old_data).unwrap();
        let other_file = store.put(&other, other_data).unwrap();
        let now = std::time::SystemTime::now();
        std::fs::File::options().write(true).open(&old_file.blob).unwrap()
            .set_modified(now - std::time::Duration::from_secs(7200)).unwrap();
        std::fs::File::options().write(true).open(&other_file.blob).unwrap()
            .set_modified(now - std::time::Duration::from_secs(3700)).unwrap();
        assert!(store.has(&old).is_some());
        let used = std::fs::metadata(&old_file.blob).unwrap().modified().unwrap();
        let unused = std::fs::metadata(&other_file.blob).unwrap().modified().unwrap();
        assert!(used > unused, "recently reused blob must survive before an unused blob");
        assert_eq!(store.read(&old).unwrap(), old_data);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_store_keeps_nothing_plain() {
        let dir = std::env::temp_dir().join(format!("openomsi-lan-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // an older version's plain file goes
        let legacy = dir.join("ab".repeat(32));
        std::fs::write(&legacy, b"old plain").unwrap();
        let marker = b"[name]\nPLAINTEXT-MARKER-store\n".repeat(50);
        let e = Entry { path: "maps/Secret Map/global.cfg".into(), size: marker.len() as u64, sha256: super::super::sha256_of(&marker) };
        let store = Store::open(&dir).unwrap();
        assert!(!legacy.exists());
        assert!(store.has(&e).is_none());
        let f = store.put(&e, &marker).unwrap();
        assert!(store.has(&e).is_some());
        for p in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = p.file_name().to_string_lossy().to_string();
            let bytes = std::fs::read(p.path()).unwrap();
            assert!(!bytes.windows(16).any(|w| w == b"PLAINTEXT-MARKER"), "plain text in {name}");
            assert!(!name.contains(&e.sha256) && !name.to_lowercase().contains("global"), "{name}");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(dir.join(KEY_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // the key survives: the same store opens the blob again
        let again = Store::open(&dir).unwrap();
        assert_eq!(omsi_cfg::vfs::unseal(&again.key, &f.aad, std::fs::read(&f.blob).unwrap()).unwrap(), marker);
        // mounted, the engine reads it under its own name
        let mount = dir.join("session");
        omsi_cfg::vfs::mount_sealed(&mount, again.key(), vec![again.has(&e).unwrap()]);
        let cfg = omsi_cfg::resolve_path(&mount, "MAPS\\secret map\\Global.cfg");
        assert_eq!(omsi_cfg::vfs::read(&cfg).unwrap(), marker);
        assert_eq!(omsi_cfg::vfs::list_dir(&mount.join("maps")).unwrap()[0].0, "Secret Map");
        omsi_cfg::vfs::unmount_sealed(&mount);
        // a lost key: the blobs go with it
        std::fs::remove_file(dir.join(KEY_FILE)).unwrap();
        let fresh = Store::open(&dir).unwrap();
        assert!(!f.blob.exists());
        assert!(fresh.has(&e).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
