//! Explicit, local-console-only release checks for the native headless host.
//! The graphical updater targets another ZIP. Never replace a running server,
//! its server.cfg, OMSI content or private mod store from this module.

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const OFFICIAL_RELEASE: &str = "https://api.github.com/repos/openOMSI-org/openOMSI/releases/latest";
const OFFICIAL_ASSETS: &str = "https://github.com/openOMSI-org/openOMSI/releases/download/";
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
static BUSY: AtomicBool = AtomicBool::new(false);
static STATUS: OnceLock<Mutex<String>> = OnceLock::new();

fn current() -> &'static str {
    option_env!("OPENOMSI_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

fn status_slot() -> &'static Mutex<String> {
    STATUS.get_or_init(|| Mutex::new("no update check requested".into()))
}

fn status(text: String) {
    log::info!("server update: {text}");
    *status_slot().lock().unwrap_or_else(|e| e.into_inner()) = text;
}

pub fn report() {
    let text = status_slot().lock().unwrap_or_else(|e| e.into_inner());
    log::info!("server update: {text}");
}

/// /update is a read-only check. /update download stages a validated ZIP.
/// No network work runs on the authoritative simulation tick.
pub fn start(download: bool) {
    if BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        report();
        return;
    }
    status(format!(
        "{} official stable headless release (running {})",
        if download {
            "checking and staging"
        } else {
            "checking"
        },
        current()
    ));
    let spawned = std::thread::Builder::new()
        .name("headless update check".into())
        .spawn(move || {
            let result = check_and_stage(download);
            if let Err(error) = result {
                status(format!("failed: {error:#}"));
            }
            BUSY.store(false, Ordering::Release);
        });
    if let Err(error) = spawned {
        BUSY.store(false, Ordering::Release);
        status(format!("cannot start update worker: {error}"));
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Candidate {
    version: String,
    filename: String,
    url: String,
    sha256: String,
    size: u64,
}

fn number_version(text: &str) -> Option<[u64; 3]> {
    let clean = text.trim_start_matches('v');
    let mut fields = clean.split('.');
    let v = [
        fields.next()?.parse().ok()?,
        fields.next()?.parse().ok()?,
        fields.next()?.parse().ok()?,
    ];
    fields.next().is_none().then_some(v)
}

fn file_for(version: &str) -> Option<String> {
    let platform = if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "windows-x64"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x64"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "linux-arm64"
    } else {
        return None;
    };
    Some(format!("openOMSI-{version}-headless-test-{platform}.zip"))
}

fn parse_release(release: &Value, version: &str) -> Result<Option<Candidate>> {
    ensure!(
        release["draft"] != true,
        "the selected GitHub release is a draft"
    );
    ensure!(
        release["prerelease"] != true,
        "only stable releases are supported"
    );
    let tag = release["tag_name"]
        .as_str()
        .context("release missing tag_name")?;
    let candidate = tag
        .strip_prefix('v')
        .context("release tag must start with v")?;
    let Some(new) = number_version(candidate) else {
        bail!("invalid official release version");
    };
    // The test build must never silently downgrade to the last stable version.
    let Some(installed) = number_version(version) else {
        bail!("this build is not a stable release; do not replace PR/dev builds");
    };
    if new <= installed {
        return Ok(None);
    }
    let filename = file_for(candidate).context("no native headless release for this platform")?;
    let assets = release["assets"]
        .as_array()
        .context("release assets missing")?;
    let asset = assets
        .iter()
        .find(|asset| asset["name"].as_str() == Some(filename.as_str()))
        .context("the release has no matching headless-test package")?;
    let url = asset["browser_download_url"]
        .as_str()
        .context("asset URL missing")?;
    let expected_url = format!("{OFFICIAL_ASSETS}{tag}/{filename}");
    ensure!(
        url == expected_url,
        "asset URL does not belong to the expected official release"
    );
    let sha256 = asset["digest"]
        .as_str()
        .and_then(|value| value.strip_prefix("sha256:"))
        .context("GitHub published no SHA-256 digest: refusing an unverified update")?;
    ensure!(
        sha256.len() == 64 && sha256.bytes().all(|c| c.is_ascii_hexdigit()),
        "invalid release SHA-256 digest"
    );
    let size = asset["size"].as_u64().context("archive size missing")?;
    ensure!(
        (1..=MAX_ARCHIVE_BYTES).contains(&size),
        "archive size exceeds safety limit"
    );
    Ok(Some(Candidate {
        version: candidate.to_owned(),
        filename,
        url: url.to_owned(),
        sha256: sha256.to_ascii_lowercase(),
        size,
    }))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .user_agent(concat!("openOMSI-headless/", env!("CARGO_PKG_VERSION")))
        .build()
}

fn check_and_stage(download: bool) -> Result<()> {
    ensure!(
        option_env!("OPENOMSI_VERSION").is_some(),
        "development server build: update using git and cargo, not an official release ZIP"
    );
    ensure!(
        std::env::var_os("OMSI_NO_UPDATE").is_none(),
        "updates disabled by OMSI_NO_UPDATE"
    );
    let response = agent()
        .get(OFFICIAL_RELEASE)
        .set("Accept", "application/vnd.github+json")
        .call()
        .context("checking official GitHub release")?;
    let body = response
        .into_string()
        .context("reading official release metadata")?;
    let json: Value = serde_json::from_str(&body).context("parsing official release metadata")?;
    let Some(candidate) = parse_release(&json, current())? else {
        status(format!(
            "already at the newest stable headless release ({})",
            current()
        ));
        return Ok(());
    };
    status(format!(
        "official headless version {} available: {}",
        candidate.version, candidate.url
    ));
    if download {
        let file = stage(&candidate)?;
        status(format!(
            "verified version {} staged at {}; stop the server before replacing its executable; server.cfg was untouched",
            candidate.version,
            file.display()
        ));
    }
    Ok(())
}

fn stage(candidate: &Candidate) -> Result<PathBuf> {
    let executable = std::env::current_exe().context("locating server binary")?;
    let dir = executable
        .parent()
        .context("server binary has no parent directory")?
        .join(".headless-updates");
    fs::create_dir_all(&dir).context("creating private update staging directory")?;
    let file = dir.join(&candidate.filename);
    let partial = dir.join(format!("{}.part", candidate.filename));
    let mut downloaded = 0u64;
    let mut hasher = Sha256::new();
    let stream = agent()
        .get(&candidate.url)
        .call()
        .context("downloading headless ZIP")?;
    let mut reader = stream.into_reader();
    let result = (|| -> Result<()> {
        let mut output = File::create(&partial).context("opening partial update file")?;
        let mut chunk = [0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut chunk).context("reading update archive")?;
            if n == 0 {
                break;
            }
            downloaded += n as u64;
            ensure!(
                downloaded <= candidate.size,
                "update archive exceeded advertised size"
            );
            hasher.update(&chunk[..n]);
            output
                .write_all(&chunk[..n])
                .context("writing update archive")?;
        }
        output.sync_all().context("flushing downloaded archive")?;
        ensure!(downloaded == candidate.size, "incomplete archive");
        let got: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        ensure!(
            got == candidate.sha256,
            "SHA-256 does not match GitHub release digest"
        );
        if file.exists() {
            fs::remove_file(&file).context("replacing an older staged archive")?;
        }
        fs::rename(&partial, &file).context("finalizing verified update archive")?;
        // Sidecar is for an offline installer to recheck the exact ZIP bytes
        // after the server is fully stopped.
        fs::write(
            dir.join(format!("{}.sha256", candidate.filename)),
            format!("{}  {}\n", candidate.sha256, candidate.filename),
        )
        .context("writing verified checksum sidecar")?;
        let marker = dir.join("ready.txt");
        let temporary = dir.join("ready.txt.tmp");
        fs::write(
            &temporary,
            format!(
                "{}\n{}\n{}\n",
                candidate.filename, candidate.sha256, candidate.version
            ),
        )
        .context("writing staged update marker")?;
        if marker.exists() {
            fs::remove_file(&marker).context("replacing an older staged update marker")?;
        }
        fs::rename(&temporary, &marker).context("publishing verified staged update")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(partial);
    }
    result?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn version_parser_rejects_pr_and_non_numeric_names() {
        assert_eq!(number_version("0.3.54"), Some([0, 3, 54]));
        assert_eq!(number_version("v0.3.54"), Some([0, 3, 54]));
        assert_eq!(number_version("0.3.54-pr405"), None);
        assert_eq!(number_version("0.3.54-extra"), None);
        assert_eq!(number_version("../0.3.54"), None);
    }

    #[test]
    fn release_selection_rejects_spoofs_and_missing_digest() {
        let Some(name) = file_for("9.9.9") else {
            return;
        };
        let url = format!("{OFFICIAL_ASSETS}v9.9.9/{name}");
        let base = json!({
            "tag_name": "v9.9.9",
            "draft": false,
            "prerelease": false,
            "assets": [{
                "name": name,
                "browser_download_url": url,
                "size": 2000,
                "digest": format!("sha256:{}", "a".repeat(64))
            }]
        });
        assert!(parse_release(&base, "0.3.54").unwrap().is_some());
        let mut wrong = base.clone();
        wrong["assets"][0]["browser_download_url"] = json!("https://evil.example/archive.zip");
        assert!(parse_release(&wrong, "0.3.54").is_err());
        let mut missing = base.clone();
        missing["assets"][0]["digest"] = Value::Null;
        assert!(parse_release(&missing, "0.3.54").is_err());
        assert!(parse_release(&base, "9.9.9").unwrap().is_none());
        assert!(parse_release(&base, "0.3.54-pr405").is_err());
    }
}
