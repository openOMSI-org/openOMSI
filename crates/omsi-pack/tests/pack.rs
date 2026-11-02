use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, AtomicU64},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use omsi_pack::{
    create_pack, forget_unlock, issue_entitlement, load_manifest, random_content_key, unlock,
    AssetInput, Dependency, KeyStore, PackError, PackMetadata, Result, SigningKey,
    TrustedPublishers, VerifiedPack,
};
use ring::rand::SystemRandom;

fn scratch() -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "omsi-pack-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ))
}

fn hash(bytes: &[u8]) -> String {
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Default)]
struct MemoryKeys(Mutex<BTreeMap<String, Vec<u8>>>);

impl KeyStore for MemoryKeys {
    fn store(&self, scope: &str, bytes: &[u8]) -> Result<()> {
        self.0.lock().unwrap().insert(scope.into(), bytes.to_vec());
        Ok(())
    }
    fn load(&self, scope: &str) -> Result<Vec<u8>> {
        self.0
            .lock()
            .unwrap()
            .get(scope)
            .cloned()
            .ok_or(PackError::KeyNotFound)
    }
    fn delete(&self, scope: &str) -> Result<()> {
        self.0
            .lock()
            .unwrap()
            .remove(scope)
            .map(|_| ())
            .ok_or(PackError::KeyNotFound)
    }
}

fn metadata(key_id: Option<String>) -> PackMetadata {
    PackMetadata {
        publisher_id: "owned-fixture-publisher".into(),
        author: "Fixture Author".into(),
        pack_id: "owned-fixture-pack".into(),
        version: "1.0.0".into(),
        license: "CC0-1.0".into(),
        redistribution: false,
        key_id,
        dependencies: vec![Dependency {
            publisher_id: "other-publisher".into(),
            pack_id: "shared-dependency".into(),
            version: "2.1.0".into(),
        }],
    }
}

fn make_pack(
    root: &std::path::Path,
    name: &str,
    key: &[u8; 32],
    bytes: &[u8],
) -> (std::path::PathBuf, SigningKey, TrustedPublishers) {
    let path = root.join(name);
    let signer = SigningKey::generate(&SystemRandom::new()).unwrap();
    let mut trust = TrustedPublishers::default();
    trust.insert(
        "owned-fixture-publisher",
        signer.key_id(),
        signer.public_key(),
    );
    let source_sha256 = hash(b"owned original source bytes");
    create_pack(
        &path,
        metadata(Some(hash(key))),
        &[AssetInput {
            id: "asset-1".into(),
            path: "Sceneryobjects/Pack/model/mesh.x".into(),
            source_sha256,
            bytes: bytes.to_vec(),
        }],
        &signer,
        Some(key),
        &AtomicBool::new(false),
    )
    .unwrap();
    (path, signer, trust)
}

#[test]
fn trusted_signed_encrypted_pack_round_trips_bounded_assets() {
    let root = scratch();
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let out = root.join("pack.oopp");
    let signer = SigningKey::generate(&SystemRandom::new()).unwrap();
    let mut trust = TrustedPublishers::default();
    trust.insert(
        "owned-fixture-publisher",
        signer.key_id(),
        signer.public_key(),
    );
    let key = random_content_key().unwrap();
    let bytes = b"synthetic map bytes".to_vec();
    let source_hash = hash(b"original source bytes");
    let manifest = create_pack(
        &out,
        metadata(Some(hash(&key))),
        &[AssetInput {
            id: "map-global".into(),
            path: "maps/Test/global.cfg".into(),
            source_sha256: source_hash,
            bytes: bytes.clone(),
        }],
        &signer,
        Some(&key),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(manifest.dependencies[0].version, "2.1.0");
    assert!(!std::fs::read(out.join("manifest.json"))
        .unwrap()
        .windows(bytes.len())
        .any(|w| w == bytes));

    let keys = MemoryKeys::default();
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let entitlement = issue_entitlement(&manifest, key, expires, &signer).unwrap();
    unlock(&out, &entitlement, &trust, &keys).unwrap();
    let pack = VerifiedPack::open_unlocked(&out, &trust, &keys).unwrap();
    assert_eq!(pack.read("maps/Test/global.cfg").unwrap(), bytes);
    assert_eq!(pack.assets().len(), 1);
    forget_unlock(&out, &trust, &keys).unwrap();
    assert!(VerifiedPack::open_unlocked(&out, &trust, &keys).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unknown_signer_wrong_key_tampering_and_traversal_fail_closed() {
    let root = scratch();
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let out = root.join("pack.oopp");
    let signer = SigningKey::generate(&SystemRandom::new()).unwrap();
    let trust = TrustedPublishers::default();
    let source_hash = hash(b"original");
    let key = random_content_key().unwrap();
    create_pack(
        &out,
        metadata(Some(hash(&key))),
        &[AssetInput {
            id: "map-global".into(),
            path: "maps/Test/global.cfg".into(),
            source_sha256: source_hash,
            bytes: b"fixture".to_vec(),
        }],
        &signer,
        Some(&key),
        &AtomicBool::new(false),
    )
    .unwrap();
    let keys = MemoryKeys::default();
    assert!(VerifiedPack::open_unlocked(&out, &trust, &keys).is_err());
    let mut trusted = TrustedPublishers::default();
    trusted.insert(
        "owned-fixture-publisher",
        signer.key_id(),
        signer.public_key(),
    );
    let manifest = load_manifest(&out, &trusted).unwrap();
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    assert!(
        VerifiedPack::open_for_authoring(&out, &trusted, Some(random_content_key().unwrap()))
            .is_err()
    );
    let correct = issue_entitlement(&manifest, key, expires, &signer).unwrap();
    unlock(&out, &correct, &trusted, &keys).unwrap();
    let chunk = &manifest.assets[0].chunks[0].file;
    let mut bytes = std::fs::read(out.join(chunk)).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(out.join(chunk), bytes).unwrap();
    assert!(VerifiedPack::open_unlocked(&out, &trusted, &keys).is_err());

    let bad = root.join("traversal.oopp");
    let plain_metadata = metadata(None);
    assert!(create_pack(
        &bad,
        plain_metadata,
        &[AssetInput {
            id: "escape".into(),
            path: "../escape.txt".into(),
            source_sha256: "00".repeat(32),
            bytes: b"x".to_vec()
        }],
        &signer,
        None,
        &AtomicBool::new(false),
    )
    .is_err());
    assert!(!bad.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn publisher_keys_round_trip_and_cached_manifest_does_not_need_asset_blobs() {
    let root = scratch();
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let out = root.join("pack.oopp");
    let signer = SigningKey::generate(&SystemRandom::new()).unwrap();
    let mut trust = TrustedPublishers::default();
    trust.insert(
        "owned-fixture-publisher",
        signer.key_id(),
        signer.public_key(),
    );
    let saved = trust.to_json().unwrap();
    let trust = TrustedPublishers::load_json(&saved).unwrap();
    let meta = metadata(None);
    let original = b"before conversion";
    let bytes = b"converted fixture bytes".to_vec();
    let source_hash = hash(original);
    let expected_output_hash = hash(&bytes);
    let manifest = create_pack(
        &out,
        meta,
        &[AssetInput {
            id: "mesh-1".into(),
            path: "Sceneryobjects/Pack/model/mesh.x".into(),
            source_sha256: source_hash,
            bytes,
        }],
        &signer,
        None,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(manifest.assets[0].id, "mesh-1");
    assert_eq!(manifest.assets[0].output_sha256, expected_output_hash);
    assert!(manifest.assets[0].size < 1024);
    std::fs::remove_dir_all(out.join("assets")).unwrap();
    assert_eq!(load_manifest(&out, &trust).unwrap(), manifest);
    std::fs::remove_dir_all(root).unwrap();
}

use sha2::Digest;

#[cfg(any(target_os = "macos", windows))]
#[test]
fn native_protected_store_survives_a_new_store_handle_and_deletes_only_its_fixture() {
    use omsi_pack::SystemKeyStore;
    let keys = SystemKeyStore;
    let scope = hash(format!("disposable-native-store-{}", std::process::id()).as_bytes());
    let payload = b"synthetic signed entitlement envelope";
    keys.store(&scope, payload).unwrap();
    drop(keys);
    let restarted = SystemKeyStore;
    assert_eq!(restarted.load(&scope).unwrap(), payload);
    restarted.delete(&scope).unwrap();
    assert!(restarted.load(&scope).is_err());
}

#[test]
fn chunked_and_empty_assets_round_trip_with_distinct_nonces() {
    let root = scratch();
    std::fs::create_dir_all(&root).unwrap();
    let out = root.join("multi.oopp");
    let signer = SigningKey::generate(&SystemRandom::new()).unwrap();
    let mut trust = TrustedPublishers::default();
    trust.insert(
        "owned-fixture-publisher",
        signer.key_id(),
        signer.public_key(),
    );
    let key = random_content_key().unwrap();
    let large = vec![0x6a; 65 * 1024];
    let manifest = create_pack(
        &out,
        metadata(Some(hash(&key))),
        &[
            AssetInput {
                id: "large".into(),
                path: "maps/Test/large.map".into(),
                source_sha256: hash(b"source large"),
                bytes: large.clone(),
            },
            AssetInput {
                id: "empty".into(),
                path: "maps/Test/empty.cfg".into(),
                source_sha256: hash(b"source empty"),
                bytes: Vec::new(),
            },
        ],
        &signer,
        Some(&key),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(manifest.assets[0].chunks.len(), 2);
    assert_eq!(manifest.assets[1].chunks.len(), 1);
    let nonces: std::collections::BTreeSet<Vec<u8>> = manifest
        .assets
        .iter()
        .flat_map(|a| &a.chunks)
        .map(|c| std::fs::read(out.join(&c.file)).unwrap()[..12].to_vec())
        .collect();
    assert_eq!(nonces.len(), 3);
    let keys = MemoryKeys::default();
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let grant = issue_entitlement(&manifest, key, expires, &signer).unwrap();
    unlock(&out, &grant, &trust, &keys).unwrap();
    let pack = VerifiedPack::open_unlocked(&out, &trust, &keys).unwrap();
    assert_eq!(pack.read("maps/Test/large.map").unwrap(), large);
    assert_eq!(pack.read("maps/Test/empty.cfg").unwrap(), b"");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_directory_prefix_collision_is_rejected_before_output() {
    let root = scratch();
    std::fs::create_dir_all(&root).unwrap();
    let out = root.join("collision.oopp");
    let signer = SigningKey::generate(&SystemRandom::new()).unwrap();
    let key = random_content_key().unwrap();
    let meta = metadata(Some(hash(&key)));
    assert!(create_pack(
        &out,
        meta,
        &[
            AssetInput {
                id: "file".into(),
                path: "Sceneryobjects/Pack/model".into(),
                source_sha256: hash(b"a"),
                bytes: b"a".to_vec()
            },
            AssetInput {
                id: "child".into(),
                path: "Sceneryobjects/Pack/model/mesh.x".into(),
                source_sha256: hash(b"b"),
                bytes: b"b".to_vec()
            },
        ],
        &signer,
        Some(&key),
        &AtomicBool::new(false),
    )
    .is_err());
    assert!(!out.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unicode_normalized_file_directory_prefix_collision_is_rejected_before_output() {
    let root = scratch();
    std::fs::create_dir_all(&root).unwrap();
    let out = root.join("unicode-collision.oopp");
    let signer = SigningKey::generate(&SystemRandom::new()).unwrap();
    let key = random_content_key().unwrap();
    assert!(create_pack(
        &out,
        metadata(Some(hash(&key))),
        &[
            AssetInput {
                id: "file".into(),
                path: "maps/Café".into(),
                source_sha256: hash(b"a"),
                bytes: b"a".to_vec()
            },
            AssetInput {
                id: "child".into(),
                path: "maps/Cafe\u{301}/child".into(),
                source_sha256: hash(b"b"),
                bytes: b"b".to_vec()
            },
        ],
        &signer,
        Some(&key),
        &AtomicBool::new(false),
    )
    .is_err());
    assert!(!out.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancellation_truncation_reordering_and_storage_failure_fail_closed() {
    let root = scratch();
    std::fs::create_dir_all(&root).unwrap();
    let signer = SigningKey::generate(&SystemRandom::new()).unwrap();
    let key = random_content_key().unwrap();
    let source = vec![0x8f; 65 * 1024];
    let cancelled_path = root.join("cancelled.oopp");
    let meta = metadata(Some(hash(&key)));
    let cancelled = AtomicBool::new(true);
    assert!(create_pack(
        &cancelled_path,
        meta,
        &[AssetInput {
            id: "asset".into(),
            path: "maps/Test/asset.bin".into(),
            source_sha256: hash(b"source"),
            bytes: source.clone()
        }],
        &signer,
        Some(&key),
        &cancelled,
    )
    .is_err());
    assert!(!cancelled_path.exists());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);

    let (out, signer, trust) = make_pack(&root, "tampered.oopp", &key, &source);
    let manifest = load_manifest(&out, &trust).unwrap();
    let first = &manifest.assets[0].chunks[0].file;
    let second = &manifest.assets[0].chunks[1].file;
    let first_blob = std::fs::read(out.join(first)).unwrap();
    let second_blob = std::fs::read(out.join(second)).unwrap();
    std::fs::write(out.join(first), second_blob).unwrap();
    std::fs::write(out.join(second), first_blob).unwrap();
    let no_keys = MemoryKeys::default();
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let grant = issue_entitlement(&manifest, key, expires, &signer).unwrap();
    unlock(&out, &grant, &trust, &no_keys).unwrap();
    assert!(VerifiedPack::open_unlocked(&out, &trust, &no_keys).is_err());

    let (truncated, signer, trust) = make_pack(&root, "truncated.oopp", &key, &source);
    let manifest = load_manifest(&truncated, &trust).unwrap();
    let first = truncated.join(&manifest.assets[0].chunks[0].file);
    std::fs::write(&first, b"short").unwrap();
    let grant = issue_entitlement(&manifest, key, expires, &signer).unwrap();
    let failing_store = MemoryKeys(Mutex::new(BTreeMap::new()));
    assert!(unlock(&truncated, &grant, &trust, &failing_store).is_ok());
    assert!(VerifiedPack::open_unlocked(&truncated, &trust, &failing_store).is_err());

    struct BrokenStore;
    impl KeyStore for BrokenStore {
        fn store(&self, _: &str, _: &[u8]) -> Result<()> {
            Err(PackError::SecureStorage("fixture storage failure".into()))
        }
        fn load(&self, _: &str) -> Result<Vec<u8>> {
            Err(PackError::KeyNotFound)
        }
        fn delete(&self, _: &str) -> Result<()> {
            Err(PackError::KeyNotFound)
        }
    }
    let (healthy, signer, trust) = make_pack(&root, "storage.oopp", &key, b"fixture");
    let manifest = load_manifest(&healthy, &trust).unwrap();
    let grant = issue_entitlement(&manifest, key, expires, &signer).unwrap();
    assert!(unlock(&healthy, &grant, &trust, &BrokenStore).is_err());
    assert!(VerifiedPack::open_unlocked(&healthy, &trust, &BrokenStore).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
