use omsi_pack::{AssetInput, PackMetadata, SigningKey, TrustedPublishers};
use std::sync::atomic::AtomicBool;
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn fixture_asset(id: &str, path: &str) -> AssetInput {
    AssetInput {
        id: id.into(),
        path: path.into(),
        source_sha256: "12".repeat(32),
        bytes: b"fixture".to_vec(),
    }
}

fn fixture_pack(
    root: &std::path::Path,
    signer: &SigningKey,
    pack: &str,
    dependencies: Vec<omsi_pack::Dependency>,
) {
    let metadata = PackMetadata {
        publisher_id: "fixture".into(),
        author: "Fixture".into(),
        pack_id: pack.into(),
        version: "1.0.0".into(),
        license: "CC0-1.0".into(),
        redistribution: true,
        key_id: None,
        dependencies,
    };
    omsi_pack::create_pack(
        &root.join("Packs").join(format!("{pack}.oopp")),
        metadata,
        &[fixture_asset(
            "fixture-asset",
            &format!("Sceneryobjects/Fixture/{pack}.sco"),
        )],
        signer,
        None,
        &AtomicBool::new(false),
    )
    .unwrap();
}

#[test]
fn signed_dependency_reads_through_the_real_vfs_and_cached_provenance_survives_manifest_removal() {
    let _guard = TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let root =
        std::env::temp_dir().join(format!("omsi-publisher-integration-{}", std::process::id()));
    std::fs::create_dir_all(root.join("Packs")).unwrap();
    let signer = SigningKey::generate(&ring::rand::SystemRandom::new()).unwrap();
    let mut trust = TrustedPublishers::default();
    trust.insert("fixture", signer.key_id(), signer.public_key());
    trust.save_file(&root.join("Publishers.json")).unwrap();
    omsi_cfg::add_content_root(root.clone());
    let source = "12".repeat(32);
    let metadata = PackMetadata {
        publisher_id: "fixture".into(),
        author: "Fixture".into(),
        pack_id: "scenery".into(),
        version: "1.0.0".into(),
        license: "CC0-1.0".into(),
        redistribution: false,
        key_id: None,
        dependencies: vec![],
    };
    let pack = root.join("Packs/scenery.oopp");
    omsi_pack::create_pack(
        &pack,
        metadata,
        &[
            AssetInput {
                id: "object".into(),
                path: "Sceneryobjects/Fixture/sign.sco".into(),
                source_sha256: source.clone(),
                bytes: b"[mesh]\nmodel/sign.x\n".to_vec(),
            },
            AssetInput {
                id: "mesh".into(),
                path: "Sceneryobjects/Fixture/model/sign.x".into(),
                source_sha256: "34".repeat(32),
                bytes: b"synthetic mesh".to_vec(),
            },
        ],
        &signer,
        None,
        &AtomicBool::new(false),
    )
    .unwrap();
    let catalog = omsi_cfg::publisher::Catalog::load(&root).unwrap();
    let found = catalog.recognize(&source).unwrap();
    assert_eq!(found.dependency.version, "1.0.0");
    assert!(!found.redistribution);
    let mounts = omsi_cfg::publisher::mount_packs(&root).unwrap();
    let object = omsi_cfg::resolve_path(&mounts[0], "Sceneryobjects/Fixture/sign.sco");
    let cfg = omsi_cfg::CfgFile::read(&object).unwrap();
    assert_eq!(cfg.lines[1], "model/sign.x");
    let map = root.join("maps/Fixture");
    std::fs::create_dir_all(&map).unwrap();
    std::fs::write(map.join("dependencies.json"), serde_json::json!({
        "format_version": 1,
        "dependencies": [{"publisher_id":"fixture","pack_id":"scenery","version":"1.0.0"}],
        "assets": [{"path":"Sceneryobjects/Fixture/renamed.sco","dependency":{"publisher_id":"fixture","pack_id":"scenery","version":"1.0.0"},"asset_id":"object"}]
    }).to_string()).unwrap();
    let before = omsi_cfg::content_roots();
    {
        let mounts = omsi_cfg::publisher::validate_map(&map).unwrap();
        let second = omsi_cfg::publisher::validate_map(&map).unwrap();
        assert_eq!(omsi_cfg::content_roots(), before);
        let renamed = omsi_cfg::resolve_path(&map, "Sceneryobjects/Fixture/renamed.sco");
        assert_eq!(
            omsi_cfg::CfgFile::read(&renamed).unwrap().lines[1],
            "model/sign.x"
        );
        let mesh = omsi_cfg::resolve_path(renamed.parent().unwrap(), "model/sign.x");
        assert_eq!(omsi_cfg::vfs::read(&mesh).unwrap(), b"synthetic mesh");
        drop(mounts);
        assert_eq!(
            omsi_cfg::vfs::read(&renamed).unwrap(),
            b"[mesh]\nmodel/sign.x\n"
        );
        drop(second);
    }
    assert_eq!(omsi_cfg::content_roots(), before);
    assert!(
        omsi_cfg::vfs::read(&omsi_cfg::resolve_path(
            &map,
            "Sceneryobjects/Fixture/renamed.sco"
        ))
        .is_err()
    );
    std::fs::remove_file(pack.join("manifest.json")).unwrap();
    assert!(
        omsi_cfg::publisher::Catalog::load(&root)
            .unwrap()
            .recognize(&source)
            .is_some()
    );
    for mount in mounts {
        omsi_cfg::vfs::unmount_sealed(&mount);
        omsi_cfg::remove_content_root(&mount);
    }
    omsi_cfg::remove_content_root(&root);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn transitive_missing_or_cyclic_dependencies_and_oversized_map_refs_fail_before_mounting() {
    let _guard = TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let root = std::env::temp_dir().join(format!("omsi-dependency-closure-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("Packs")).unwrap();
    let signer = SigningKey::generate(&ring::rand::SystemRandom::new()).unwrap();
    let mut trust = TrustedPublishers::default();
    trust.insert("fixture", signer.key_id(), signer.public_key());
    trust.save_file(&root.join("Publishers.json")).unwrap();
    fixture_pack(
        &root,
        &signer,
        "root",
        vec![omsi_pack::Dependency {
            publisher_id: "fixture".into(),
            pack_id: "branch".into(),
            version: "1.0.0".into(),
        }],
    );
    fixture_pack(
        &root,
        &signer,
        "branch",
        vec![omsi_pack::Dependency {
            publisher_id: "fixture".into(),
            pack_id: "missing-leaf".into(),
            version: "9.0.0".into(),
        }],
    );
    let map = root.join("maps/Fixture");
    std::fs::create_dir_all(&map).unwrap();
    std::fs::write(map.join("dependencies.json"), serde_json::json!({"format_version":1,"dependencies":[{"publisher_id":"fixture","pack_id":"root","version":"1.0.0"}],"assets":[]}).to_string()).unwrap();
    omsi_cfg::add_content_root(root.clone());
    let before = omsi_cfg::content_roots();
    assert!(
        omsi_cfg::publisher::validate_map(&map)
            .unwrap_err()
            .to_string()
            .contains("missing-leaf")
    );
    std::fs::remove_dir_all(root.join("Packs/branch.oopp")).unwrap();
    fixture_pack(
        &root,
        &signer,
        "branch",
        vec![omsi_pack::Dependency {
            publisher_id: "fixture".into(),
            pack_id: "root".into(),
            version: "1.0.0".into(),
        }],
    );
    assert!(
        omsi_cfg::publisher::validate_map(&map)
            .unwrap_err()
            .to_string()
            .contains("cycle")
    );
    std::fs::write(map.join("dependencies.json"), vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(omsi_cfg::publisher::validate_map(&map).is_err());
    assert_eq!(omsi_cfg::content_roots(), before);
    omsi_cfg::remove_content_root(&root);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ordinary_map_has_no_compulsory_manifest_and_missing_dependency_is_an_error() {
    let _guard = TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let root = std::env::temp_dir().join(format!(
        "omsi-dependency-integration-{}",
        std::process::id()
    ));
    let map = root.join("maps/Fixture");
    std::fs::create_dir_all(&map).unwrap();
    omsi_cfg::publisher::validate_map(&map).unwrap();
    std::fs::write(map.join("dependencies.json"), r#"{"format_version":1,"dependencies":[{"publisher_id":"missing","pack_id":"missing","version":"1.0.0"}],"assets":[]}"#).unwrap();
    omsi_cfg::add_content_root(root.clone());
    assert!(
        omsi_cfg::publisher::validate_map(&map)
            .unwrap_err()
            .to_string()
            .contains("missing")
    );
    omsi_cfg::remove_content_root(&root);
    std::fs::write(map.join("dependencies.json"), r#"{"format_version":1,"dependencies":[{"publisher_id":"missing","pack_id":"missing","version":"1.0.0"}],"assets":[{"path":"Plugins/evil.lua","dependency":{"publisher_id":"missing","pack_id":"missing","version":"1.0.0"},"asset_id":"missing"}]}"#).unwrap();
    omsi_cfg::add_content_root(root.clone());
    let before = omsi_cfg::content_roots();
    assert!(omsi_cfg::publisher::validate_map(&map).is_err());
    assert_eq!(omsi_cfg::content_roots(), before);
    omsi_cfg::remove_content_root(&root);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn one_locked_optional_pack_does_not_hide_other_installed_maps() {
    let _guard = TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let root =
        std::env::temp_dir().join(format!("omsi-publisher-isolation-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("Packs")).unwrap();
    let plain_signer = SigningKey::generate(&ring::rand::SystemRandom::new()).unwrap();
    let locked_signer = SigningKey::generate(&ring::rand::SystemRandom::new()).unwrap();
    let mut trust = TrustedPublishers::default();
    trust.insert("fixture", plain_signer.key_id(), plain_signer.public_key());
    trust.insert("locked", locked_signer.key_id(), locked_signer.public_key());
    trust.save_file(&root.join("Publishers.json")).unwrap();
    omsi_pack::create_pack(
        &root.join("Packs/plain.oopp"),
        PackMetadata {
            publisher_id: "fixture".into(),
            author: "Fixture".into(),
            pack_id: "plain".into(),
            version: "1.0.0".into(),
            license: "CC0-1.0".into(),
            redistribution: true,
            key_id: None,
            dependencies: vec![],
        },
        &[fixture_asset("plain-map", "maps/Plain/global.cfg")],
        &plain_signer,
        None,
        &AtomicBool::new(false),
    )
    .unwrap();
    let content_key = omsi_pack::random_content_key().unwrap();
    let key_id = omsi_pack::sha256_hex(&content_key);
    omsi_pack::create_pack(
        &root.join("Packs/locked.oopp"),
        PackMetadata {
            publisher_id: "locked".into(),
            author: "Fixture".into(),
            pack_id: "locked".into(),
            version: "1.0.0".into(),
            license: "CC0-1.0".into(),
            redistribution: false,
            key_id: Some(key_id),
            dependencies: vec![],
        },
        &[fixture_asset("locked-map", "maps/Locked/global.cfg")],
        &locked_signer,
        Some(&content_key),
        &AtomicBool::new(false),
    )
    .unwrap();
    let map = root.join("maps/RequiresLocked");
    std::fs::create_dir_all(&map).unwrap();
    std::fs::write(map.join("dependencies.json"), serde_json::json!({"format_version":1,"dependencies":[{"publisher_id":"locked","pack_id":"locked","version":"1.0.0"}],"assets":[]}).to_string()).unwrap();

    let mounts = omsi_cfg::publisher::mount_packs(&root).unwrap();
    assert_eq!(mounts.len(), 1, "the locked optional pack must be skipped");
    assert!(mounts[0].to_string_lossy().contains("plain.oopp"));
    let plain_mount = mounts
        .iter()
        .find(|m| m.to_string_lossy().contains("plain.oopp"))
        .unwrap();
    let plain = omsi_cfg::resolve_path(plain_mount, "maps/Plain/global.cfg");
    assert_eq!(omsi_cfg::vfs::read(&plain).unwrap(), b"fixture");
    omsi_cfg::add_content_root(root.clone());
    assert!(
        omsi_cfg::publisher::validate_map(&map).is_err(),
        "map dependency should require its own unlock"
    );
    omsi_cfg::remove_content_root(&root);
    for mount in mounts {
        omsi_cfg::vfs::unmount_sealed(&mount);
        omsi_cfg::remove_content_root(&mount);
    }
    std::fs::remove_dir_all(root).unwrap();
}
