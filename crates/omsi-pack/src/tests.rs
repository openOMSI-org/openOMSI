use crate::*;
use ring::rand::SystemRandom;
use std::{
    fs,
    sync::atomic::AtomicBool,
    time::{SystemTime, UNIX_EPOCH},
};
#[test]
fn digest_format_is_lowercase_and_stable() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn a_trusted_signature_does_not_make_unicode_file_directory_collisions_valid() {
    let root =
        std::env::temp_dir().join(format!("omsi-pack-signed-collision-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let pack = root.join("signed.oopp");
    let signer = SigningKey::generate(&SystemRandom::new()).unwrap();
    let manifest = create_pack(
        &pack,
        PackMetadata {
            publisher_id: "owned-fixture-publisher".into(),
            author: "Fixture".into(),
            pack_id: "collision-fixture".into(),
            version: "1".into(),
            license: "CC0-1.0".into(),
            redistribution: false,
            key_id: None,
            dependencies: Vec::new(),
        },
        &[AssetInput {
            id: "file".into(),
            path: "maps/Café".into(),
            source_sha256: sha256_hex(b"source"),
            bytes: b"a".to_vec(),
        }],
        &signer,
        None,
        &AtomicBool::new(false),
    )
    .unwrap();
    let mut hostile = manifest;
    hostile.assets.push(Asset {
        id: "child".into(),
        path: "maps/Cafe\u{301}/child".into(),
        source_sha256: sha256_hex(b"child source"),
        output_sha256: sha256_hex(b"b"),
        size: 1,
        chunks: Vec::new(),
    });
    let bytes = serde_json::to_vec(&hostile).unwrap();
    fs::write(pack.join("manifest.json"), &bytes).unwrap();
    fs::write(pack.join("manifest.sig"), signer.pair.sign(&bytes).as_ref()).unwrap();
    let mut trusted = TrustedPublishers::default();
    trusted.insert(
        "owned-fixture-publisher",
        signer.key_id(),
        signer.public_key(),
    );
    assert!(load_manifest(&pack, &trusted).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn concurrent_publisher_file_saves_never_replace_the_winner() {
    use std::sync::{Arc, Barrier};

    let root = std::env::temp_dir().join(format!("omsi-pack-save-race-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let path = root.join("Publishers.json");
    let barrier = Arc::new(Barrier::new(2));
    let writers: Vec<_> = ["publisher-a", "publisher-b"]
        .into_iter()
        .map(|name| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut trusted = TrustedPublishers::default();
                trusted.insert(name, "00".repeat(32), [0; 32]);
                write_atomic_after_check(&path, &trusted.to_json().unwrap(), || {
                    barrier.wait();
                })
            })
        })
        .collect();
    let successes = writers
        .into_iter()
        .map(|writer| writer.join().unwrap())
        .filter(Result::is_ok)
        .count();
    assert_eq!(successes, 1);
    let written = TrustedPublishers::load_file(&path).unwrap();
    assert_eq!(written.publishers.len(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_expired_signed_grant_is_rejected_even_when_already_cached() {
    let root = std::env::temp_dir().join(format!("omsi-pack-expiry-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let pack = root.join("expired.oopp");
    let signer = SigningKey::generate(&SystemRandom::new()).unwrap();
    let key = random_content_key().unwrap();
    let manifest = create_pack(
        &pack,
        PackMetadata {
            publisher_id: "expiry-fixture-publisher".into(),
            author: "Fixture".into(),
            pack_id: "expiry-fixture-pack".into(),
            version: "1".into(),
            license: "CC0-1.0".into(),
            redistribution: false,
            key_id: Some(sha256_hex(&key)),
            dependencies: Vec::new(),
        },
        &[AssetInput {
            id: "fixture".into(),
            path: "maps/Fixture/global.cfg".into(),
            source_sha256: sha256_hex(b"source"),
            bytes: b"owned synthetic fixture".to_vec(),
        }],
        &signer,
        Some(&key),
        &AtomicBool::new(false),
    )
    .unwrap();
    let mut trusted = TrustedPublishers::default();
    trusted.insert(
        "expiry-fixture-publisher",
        signer.key_id(),
        signer.public_key(),
    );
    let expires = now_unix().unwrap() + 3600;
    let bytes = issue_entitlement(&manifest, key, expires, &signer).unwrap();
    let mut signed: SignedEntitlement = serde_json::from_slice(&bytes).unwrap();
    signed.entitlement.expires_unix = now_unix().unwrap() - 1;
    let canonical = serde_json::to_vec(&signed.entitlement).unwrap();
    signed.signature = signer.pair.sign(&canonical).as_ref().to_vec();
    let expired = serde_json::to_vec(&signed).unwrap();
    let scope = entitlement_scope(&manifest, &manifest.key_id.clone().unwrap()).unwrap();
    struct Cached {
        scope: String,
        bytes: Vec<u8>,
    }
    impl KeyStore for Cached {
        fn store(&self, _: &str, _: &[u8]) -> Result<()> {
            Ok(())
        }
        fn load(&self, scope: &str) -> Result<Vec<u8>> {
            if scope == self.scope {
                Ok(self.bytes.clone())
            } else {
                Err(PackError::KeyNotFound)
            }
        }
        fn delete(&self, _: &str) -> Result<()> {
            Ok(())
        }
    }
    let cached = Cached {
        scope,
        bytes: expired.clone(),
    };
    assert!(matches!(
        VerifiedPack::open_unlocked(&pack, &trusted, &cached),
        Err(PackError::Expired)
    ));
    assert!(matches!(
        unlock(&pack, &expired, &trusted, &cached),
        Err(PackError::Expired)
    ));
    fs::remove_dir_all(root).unwrap();
}

#[derive(Default)]
struct MemoryKeys(std::sync::Mutex<std::collections::BTreeMap<String, Vec<u8>>>);

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

fn licence_metadata(key: &[u8; 32]) -> PackMetadata {
    PackMetadata {
        publisher_id: "licence-fixture-publisher".into(),
        author: "Fixture".into(),
        pack_id: "licence-fixture-pack".into(),
        version: "1".into(),
        license: "LicenseRef-Owned-Fixture".into(),
        redistribution: false,
        key_id: Some(sha256_hex(key)),
        dependencies: Vec::new(),
    }
}

fn licence_input() -> Vec<AssetInput> {
    vec![AssetInput {
        id: "fixture".into(),
        path: "maps/Fixture/global.cfg".into(),
        source_sha256: sha256_hex(b"source"),
        bytes: b"owned synthetic fixture".to_vec(),
    }]
}

fn licensed_pack(
    root: &std::path::Path,
    licence: &str,
    signer: &SigningKey,
) -> (std::path::PathBuf, Manifest, [u8; 32]) {
    let pack = root.join(format!("{licence}.oopp"));
    let key = random_content_key().unwrap();
    let manifest = create_licensed_pack(
        &pack,
        licence_metadata(&key),
        licence,
        &licence_input(),
        signer,
        &key,
        &AtomicBool::new(false),
    )
    .unwrap();
    (pack, manifest, key)
}

fn licence_trust(signer: &SigningKey) -> TrustedPublishers {
    let mut trusted = TrustedPublishers::default();
    trusted.insert(
        "licence-fixture-publisher",
        signer.key_id(),
        signer.public_key(),
    );
    trusted
}

fn licence_scratch(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("omsi-pack-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

fn resign(signed: &SignedEntitlement, signer: &SigningKey) -> Vec<u8> {
    let canonical = serde_json::to_vec(&signed.entitlement).unwrap();
    let copy = SignedEntitlement {
        entitlement: serde_json::from_slice(&canonical).unwrap(),
        signature: signer.pair.sign(&canonical).as_ref().to_vec(),
    };
    serde_json::to_vec(&copy).unwrap()
}

#[test]
fn each_buyers_licence_unlocks_only_the_pack_that_names_it() {
    let root = licence_scratch("licence-pair");
    let signer = SigningKey::generate(&SystemRandom::new()).unwrap();
    let trusted = licence_trust(&signer);
    let (pack_a, manifest_a, key_a) = licensed_pack(&root, "lic-a", &signer);
    let (pack_b, manifest_b, key_b) = licensed_pack(&root, "lic-b", &signer);
    assert_eq!(manifest_a.licence_id.as_deref(), Some("lic-a"));
    assert_eq!(manifest_b.licence_id.as_deref(), Some("lic-b"));
    assert_ne!(manifest_a.key_id, manifest_b.key_id);
    let stored = fs::read(pack_a.join("manifest.json")).unwrap();
    assert!(String::from_utf8_lossy(&stored).contains("\"licence_id\":\"lic-a\""));
    assert_eq!(
        load_manifest(&pack_a, &trusted)
            .unwrap()
            .licence_id
            .as_deref(),
        Some("lic-a")
    );

    let expires = now_unix().unwrap() + 3600;
    let grant_a = issue_entitlement(&manifest_a, key_a, expires, &signer).unwrap();
    let grant_b = issue_entitlement(&manifest_b, key_b, expires, &signer).unwrap();
    let parsed: SignedEntitlement = serde_json::from_slice(&grant_a).unwrap();
    assert_eq!(parsed.entitlement.licence_id.as_deref(), Some("lic-a"));

    let keys = MemoryKeys::default();
    assert!(unlock(&pack_b, &grant_a, &trusted, &keys).is_err());
    assert!(unlock(&pack_a, &grant_b, &trusted, &keys).is_err());
    assert!(keys.0.lock().unwrap().is_empty());
    unlock(&pack_a, &grant_a, &trusted, &keys).unwrap();
    unlock(&pack_b, &grant_b, &trusted, &keys).unwrap();
    let opened = VerifiedPack::open_unlocked(&pack_a, &trusted, &keys).unwrap();
    assert_eq!(
        opened.read("maps/Fixture/global.cfg").unwrap(),
        b"owned synthetic fixture"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_licence_re_signed_with_another_licence_id_is_rejected() {
    let root = licence_scratch("licence-forged");
    let signer = SigningKey::generate(&SystemRandom::new()).unwrap();
    let trusted = licence_trust(&signer);
    let (pack, manifest, key) = licensed_pack(&root, "lic-a", &signer);
    let expires = now_unix().unwrap() + 3600;
    let grant = issue_entitlement(&manifest, key, expires, &signer).unwrap();
    let mut signed: SignedEntitlement = serde_json::from_slice(&grant).unwrap();
    let keys = MemoryKeys::default();
    for forged in [Some("lic-b".to_string()), None] {
        signed.entitlement.licence_id = forged;
        assert!(matches!(
            unlock(&pack, &resign(&signed, &signer), &trusted, &keys),
            Err(PackError::Invalid(_))
        ));
    }
    assert!(keys.0.lock().unwrap().is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn packs_and_licences_without_a_licence_id_keep_their_existing_bytes() {
    let root = licence_scratch("licence-absent");
    let signer = SigningKey::generate(&SystemRandom::new()).unwrap();
    let trusted = licence_trust(&signer);
    let key = random_content_key().unwrap();
    let pack = root.join("shared.oopp");
    let manifest = create_pack(
        &pack,
        licence_metadata(&key),
        &licence_input(),
        &signer,
        Some(&key),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(manifest.licence_id, None);
    let stored = fs::read(pack.join("manifest.json")).unwrap();
    assert!(!String::from_utf8_lossy(&stored).contains("licence_id"));
    let grant = issue_entitlement(&manifest, key, now_unix().unwrap() + 3600, &signer).unwrap();
    assert!(!String::from_utf8_lossy(&grant).contains("licence_id"));
    let keys = MemoryKeys::default();
    unlock(&pack, &grant, &trusted, &keys).unwrap();
    assert!(VerifiedPack::open_unlocked(&pack, &trusted, &keys).is_ok());

    let plain = root.join("plain.oopp");
    let unprotected = create_pack(
        &plain,
        PackMetadata {
            key_id: None,
            ..licence_metadata(&key)
        },
        &licence_input(),
        &signer,
        None,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(unprotected.licence_id, None);
    assert!(
        !String::from_utf8_lossy(&fs::read(plain.join("manifest.json")).unwrap())
            .contains("licence_id")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_licence_ids_and_unprotected_licensed_manifests_are_rejected() {
    let root = licence_scratch("licence-malformed");
    let signer = SigningKey::generate(&SystemRandom::new()).unwrap();
    let trusted = licence_trust(&signer);
    let key = random_content_key().unwrap();
    let too_long = "x".repeat(129);
    for (index, bad) in [
        "",
        "has space",
        "lic/a",
        "lic-\u{fc}",
        "lic\n",
        too_long.as_str(),
    ]
    .into_iter()
    .enumerate()
    {
        let pack = root.join(format!("bad-{index}.oopp"));
        assert!(create_licensed_pack(
            &pack,
            licence_metadata(&key),
            bad,
            &licence_input(),
            &signer,
            &key,
            &AtomicBool::new(false),
        )
        .is_err());
        assert!(!pack.exists());
    }
    let longest = "x".repeat(128);
    let (_, accepted, _) = licensed_pack(&root, &longest, &signer);
    assert_eq!(accepted.licence_id.as_deref(), Some(longest.as_str()));

    let (pack, manifest, _) = licensed_pack(&root, "lic-a", &signer);
    assert!(load_manifest(&pack, &trusted).is_ok());
    let mut hostile = manifest;
    hostile.licence_id = Some("bad id".into());
    let bytes = serde_json::to_vec(&hostile).unwrap();
    fs::write(pack.join("manifest.json"), &bytes).unwrap();
    fs::write(pack.join("manifest.sig"), signer.pair.sign(&bytes).as_ref()).unwrap();
    assert!(load_manifest(&pack, &trusted).is_err());

    let plain = root.join("plain.oopp");
    let mut unprotected = create_pack(
        &plain,
        PackMetadata {
            key_id: None,
            ..licence_metadata(&key)
        },
        &licence_input(),
        &signer,
        None,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(load_manifest(&plain, &trusted).is_ok());
    unprotected.licence_id = Some("lic-a".into());
    let bytes = serde_json::to_vec(&unprotected).unwrap();
    fs::write(plain.join("manifest.json"), &bytes).unwrap();
    fs::write(
        plain.join("manifest.sig"),
        signer.pair.sign(&bytes).as_ref(),
    )
    .unwrap();
    assert!(load_manifest(&plain, &trusted).is_err());
    fs::remove_dir_all(root).unwrap();
}
