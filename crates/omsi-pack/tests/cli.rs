use std::{
    process::Command,
    sync::atomic::AtomicBool,
    time::{SystemTime, UNIX_EPOCH},
};

use omsi_pack::{
    create_pack, forget_unlock, random_content_key, AssetInput, Dependency, PackMetadata,
    SigningKey, SystemKeyStore, TrustedPublishers,
};
use sha2::Digest;

fn hash(bytes: &[u8]) -> String {
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn private_file(path: &std::path::Path, bytes: &[u8]) {
    use std::fs::OpenOptions;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write as _;
    options.open(path).unwrap().write_all(bytes).unwrap();
}

fn bounded_cli_error(stderr: &[u8], secret: &[u8]) -> String {
    if stderr.windows(secret.len()).any(|window| window == secret) {
        return "[redacted diagnostic]".into();
    }
    String::from_utf8_lossy(stderr).chars().take(512).collect()
}

#[cfg(any(target_os = "macos", windows))]
struct UnlockCleanup {
    pack: std::path::PathBuf,
    trusted: TrustedPublishers,
}

#[cfg(any(target_os = "macos", windows))]
impl Drop for UnlockCleanup {
    fn drop(&mut self) {
        let _ = forget_unlock(&self.pack, &self.trusted, &SystemKeyStore);
    }
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn command_line_issues_unlocks_and_reopens_a_signed_pack_in_a_new_process() {
    let root = std::env::temp_dir().join(format!("omsi-pack-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let private = root.join("publisher.pk8");
    let trust = root.join("Publishers.json");
    let binary = env!("CARGO_BIN_EXE_omsi-pack");
    let result = Command::new(binary)
        .args([
            "keygen",
            private.to_str().unwrap(),
            trust.to_str().unwrap(),
            "owned-cli-publisher",
        ])
        .output()
        .unwrap();
    assert!(result.status.success(), "keygen failed");
    assert_eq!(std::fs::metadata(&private).unwrap().len() > 0, true);

    let signer = SigningKey::from_pkcs8(&std::fs::read(&private).unwrap()).unwrap();
    let key = random_content_key().unwrap();
    let pack_key = root.join("publisher-pack-key.bin");
    private_file(&pack_key, &key);
    let pack = root.join("fixture.oopp");
    create_pack(
        &pack,
        PackMetadata {
            publisher_id: "owned-cli-publisher".into(),
            author: "Fixture Author".into(),
            pack_id: "owned-cli-pack".into(),
            version: "1.0.0".into(),
            license: "CC0-1.0".into(),
            redistribution: false,
            key_id: Some(hash(&key)),
            dependencies: vec![Dependency {
                publisher_id: "base".into(),
                pack_id: "base-pack".into(),
                version: "1.0.0".into(),
            }],
        },
        &[AssetInput {
            id: "owned-fixture-asset".into(),
            path: "maps/Fixture/global.cfg".into(),
            source_sha256: hash(b"source"),
            bytes: b"[name]\r\nFixture\r\n".to_vec(),
        }],
        &signer,
        Some(&key),
        &AtomicBool::new(false),
    )
    .unwrap();
    let entitlement = root.join("entitlement.json");
    let expires = (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600)
        .to_string();
    let issued = Command::new(binary)
        .args([
            "issue",
            pack.to_str().unwrap(),
            private.to_str().unwrap(),
            pack_key.to_str().unwrap(),
            &expires,
            entitlement.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        issued.status.success(),
        "issuer failed: {}",
        bounded_cli_error(&issued.stderr, &key)
    );
    assert!(!String::from_utf8_lossy(&issued.stdout).contains("\"content_key\""));

    let trusted = TrustedPublishers::load_file(&trust).unwrap();
    let _cleanup = UnlockCleanup {
        pack: pack.clone(),
        trusted,
    };
    let unlocked = Command::new(binary)
        .args([
            "unlock",
            pack.to_str().unwrap(),
            entitlement.to_str().unwrap(),
            trust.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(unlocked.status.success(), "unlock failed");
    let after_restart = Command::new(binary)
        .args(["inspect", pack.to_str().unwrap(), trust.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        after_restart.status.success(),
        "inspection after restart failed"
    );
    assert!(String::from_utf8_lossy(&after_restart.stdout).contains("owned-cli-pack"));
    assert!(!String::from_utf8_lossy(&after_restart.stdout).contains("content_key"));
    drop(_cleanup);
    std::fs::remove_dir_all(root).unwrap();
}
