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
