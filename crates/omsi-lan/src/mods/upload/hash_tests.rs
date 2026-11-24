use super::*;
use std::io::Write;

struct Folder(PathBuf);
impl Folder {
    fn new(name: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("omsi-hash-{name}-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cached_hash_checks_header_and_changed_files_are_rehashed() {
    let dir = Folder::new("cache");
    let p = dir.0.join("model.cfg");
    std::fs::write(&p, b"[mesh] original").unwrap();
    let (first, fresh) = hashed_one("model.cfg", &p, &HashMap::new()).unwrap();
    let (key, value) = fresh.unwrap();
    let cache = HashMap::from([(key.clone(), value)]);
    let (hit, fresh) = hashed_one("model.cfg", &p, &cache).unwrap();
    assert!(fresh.is_none());
    assert_eq!(hit.sha256, first.sha256);
    std::fs::write(&p, b"[mesh] changed and longer").unwrap();
    let (changed, fresh) = hashed_one("model.cfg", &p, &cache).unwrap();
    assert!(fresh.is_some());
    assert_eq!(
        changed.sha256,
        crate::mods::sha256_of(&std::fs::read(&p).unwrap())
    );
    assert_ne!(changed.sha256, first.sha256);
    std::fs::write(&p, b"MZnot a model").unwrap();
    let md = std::fs::metadata(&p).unwrap();
    let fake = HashMap::from([(key, (md.len(), modified_secs(&md), first.sha256))]);
    assert!(hashed_one("model.cfg", &p, &fake).is_none());
}

#[test]
fn manifest_hashes_match_the_old_whole_file_fingerprints_in_order() {
    let dir = Folder::new("manifest");
    let mut files = Vec::new();
    for k in 0..35 {
        let rel = format!("{k}.cfg");
        let path = dir.0.join(&rel);
        std::fs::write(&path, vec![b'A' + k as u8; 65536 + k]).unwrap();
        files.push((rel, path));
    }
    let entries = hashed(&files);
    assert_eq!(entries.len(), files.len());
    for (entry, (rel, path)) in entries.iter().zip(&files) {
        let data = std::fs::read(path).unwrap();
        assert_eq!(&entry.path, rel);
        assert_eq!(entry.size, data.len() as u64);
        assert_eq!(entry.sha256, crate::mods::sha256_of(&data));
    }
}

#[test]
fn virtual_memory_takes_precedence_over_disk_and_cache() {
    let dir = Folder::new("memory");
    let p = dir.0.join("texture.cfg");
    std::fs::write(&p, b"disk").unwrap();
    omsi_cfg::vfs::put_memory_file(p.clone(), std::sync::Arc::from(&b"virtual"[..]));
    let md = std::fs::metadata(&p).unwrap();
    let cache = HashMap::from([(
        p.to_string_lossy().into_owned(),
        (md.len(), modified_secs(&md), "stale".into()),
    )]);
    let (entry, fresh) = hashed_one("texture.cfg", &p, &cache).unwrap();
    assert!(fresh.is_none());
    assert_eq!(entry.sha256, crate::mods::sha256_of(b"virtual"));
}

#[test]
fn mounted_zip_entries_use_the_vfs_fingerprint() {
    let dir = Folder::new("zip");
    let zip_path = dir.0.join("pack.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
    zip.start_file(
        "maps/Test/global.cfg",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(b"[name]\nTest map\n").unwrap();
    zip.finish().unwrap();
    let mount = omsi_cfg::vfs::mount_zip(&zip_path).unwrap();
    let p = mount.join("maps/Test/global.cfg");
    let (entry, fresh) = hashed_one("maps/Test/global.cfg", &p, &HashMap::new()).unwrap();
    assert!(fresh.is_none());
    assert_eq!(
        entry.sha256,
        crate::mods::sha256_of(&omsi_cfg::vfs::read(&p).unwrap())
    );
}

#[test]
fn cache_snapshots_replace_valid_json_and_preserve_the_previous_snapshot_on_failure() {
    let dir = Folder::new("atomic-cache");
    let p = dir.0.join("cache.json");
    let old = HashMap::from([("old".into(), (1, 2, "hash".into()))]);
    crate::mods::write_hash_cache(&p, &old).unwrap();
    let new = HashMap::from([("new".into(), (3, 4, "other".into()))]);
    crate::mods::write_hash_cache(&p, &new).unwrap();
    let read: crate::mods::HashCache = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert_eq!(read, new);
    // A failed replacement must not delete the destination or leave a partial file.
    let blocked = dir.0.join("directory");
    std::fs::create_dir(&blocked).unwrap();
    assert!(crate::mods::write_hash_cache(&blocked, &old).is_err());
    assert!(blocked.is_dir());
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 2);
}
