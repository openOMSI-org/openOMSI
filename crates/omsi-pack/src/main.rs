use std::{
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
    process,
};

use omsi_pack::{
    issue_entitlement, load_manifest_signed_by, unlock, SigningKey, SystemKeyStore,
    TrustedPublishers, VerifiedPack,
};
use zeroize::Zeroizing;

const PRIVATE_KEY_LIMIT: usize = 16 * 1024;
const CONTENT_KEY_LENGTH: usize = 32;
const ENTITLEMENT_LIMIT: usize = 64 * 1024;

fn main() {
    if let Err(error) = run() {
        eprintln!("omsi-pack: {error}");
        process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<OsString> = env::args_os().collect();
    match args.get(1).and_then(|s| s.to_str()) {
        Some("keygen") if args.len() == 5 => keygen(Path::new(&args[2]), Path::new(&args[3]), &text_arg(&args[4])?),
        Some("issue") if args.len() == 7 => issue(
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
            &text_arg(&args[5])?,
            Path::new(&args[6]),
        ),
        Some("unlock") if args.len() == 5 => unlock_pack(
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
        ),
        Some("inspect") if args.len() == 4 => inspect_pack(Path::new(&args[2]), Path::new(&args[3])),
        _ => Err("usage: omsi-pack keygen <signing.pk8> <Publishers.json> <publisher-id> | issue <pack.oopp> <signing.pk8> <pack-key.bin> <expires-unix> <entitlement.json> | unlock <pack.oopp> <entitlement.json> <Publishers.json> | inspect <pack.oopp> <Publishers.json>".into()),
    }
}

fn keygen(private_path: &Path, trust_path: &Path, publisher: &str) -> Result<(), String> {
    if private_path.exists() || trust_path.exists() {
        return Err("refusing to overwrite an existing key or publisher file".into());
    }
    let signer = SigningKey::generate(&ring::rand::SystemRandom::new()).map_err(show)?;
    let mut trust = TrustedPublishers::default();
    trust.insert(publisher, signer.key_id(), signer.public_key());
    signer.save_private_key(private_path).map_err(show)?;
    if let Err(error) = trust.save_file(trust_path) {
        let _ = fs::remove_file(private_path);
        return Err(show(error));
    }
    println!(
        "Created publisher signing key {} and trusted publisher file.",
        signer.key_id()
    );
    Ok(())
}

fn issue(
    pack: &Path,
    private_path: &Path,
    key_path: &Path,
    expires: &str,
    output: &Path,
) -> Result<(), String> {
    let expires = expires
        .parse::<u64>()
        .map_err(|_| "expiry must be Unix seconds".to_string())?;
    let signing_bytes =
        Zeroizing::new(read_private_file(private_path, PRIVATE_KEY_LIMIT).map_err(show)?);
    let signer = SigningKey::from_pkcs8(&signing_bytes).map_err(show)?;
    let key_bytes = Zeroizing::new(read_private_file(key_path, CONTENT_KEY_LENGTH).map_err(show)?);
    let key: [u8; CONTENT_KEY_LENGTH] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "pack key must contain exactly 32 bytes".to_string())?;
    let key = Zeroizing::new(key);
    let manifest = load_manifest_signed_by(pack, &signer).map_err(show)?;
    let entitlement =
        Zeroizing::new(issue_entitlement(&manifest, *key, expires, &signer).map_err(show)?);
    write_private_file(output, &entitlement).map_err(show)?;
    println!(
        "Created publisher-signed offline entitlement for {} {}.",
        manifest.pack_id, manifest.version
    );
    Ok(())
}

fn unlock_pack(pack: &Path, entitlement_path: &Path, trust_path: &Path) -> Result<(), String> {
    let trusted = TrustedPublishers::load_file(trust_path).map_err(show)?;
    let entitlement =
        Zeroizing::new(read_private_file(entitlement_path, ENTITLEMENT_LIMIT).map_err(show)?);
    let manifest = unlock(pack, &entitlement, &trusted, &SystemKeyStore).map_err(show)?;
    println!(
        "Unlocked {} {} for offline play on this account.",
        manifest.pack_id, manifest.version
    );
    Ok(())
}

fn inspect_pack(pack: &Path, trust_path: &Path) -> Result<(), String> {
    let trusted = TrustedPublishers::load_file(trust_path).map_err(show)?;
    let verified = VerifiedPack::open_unlocked(pack, &trusted, &SystemKeyStore).map_err(show)?;
    let json = serde_json::to_string_pretty(verified.manifest()).map_err(|e| e.to_string())?;
    println!("{json}");
    Ok(())
}

fn read_private_file(path: &Path, max: usize) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > max as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "private input is not a bounded regular file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private input must be owner-only",
            ));
        }
    }
    let file = File::open(path)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(max as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "private input exceeds its size limit",
        ));
    }
    Ok(bytes)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "refusing to overwrite an existing private file",
        ));
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn text_arg(value: &OsString) -> Result<&str, String> {
    value
        .to_str()
        .ok_or_else(|| "text arguments must be UTF-8".into())
}

fn show(error: impl std::fmt::Display) -> String {
    error.to_string()
}
