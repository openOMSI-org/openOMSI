//! Updates from the project's GitHub releases (github.com/openOMSI-org/openOMSI).
//!
//! A release a maintainer starts from main publishes `v<MAJOR.MINOR.PATCH>` with one archive
//! per platform (see .github/workflows/release.yml). The launcher asks the GitHub API for the
//! latest release when it starts (setting `update_check`), and when it is newer than this
//! build it offers it - or, with `update_auto`, installs it at once. On desktop the stable
//! channel can also return to an older stable release when the project flags the installed
//! release as broken and GitHub supplies a SHA-256 for the replacement:
//!
//! * **Windows, macOS, Linux**: the archive is downloaded (and checked against the SHA-256
//!   GitHub lists for it), unpacked into `.openomsi-update` beside the program, and every
//!   program file it holds takes the place of the old one: the old one is renamed to
//!   `*.old-update` first (Windows lets a running .exe be renamed, not overwritten) and put
//!   back if anything fails. On macOS the running `.app` bundle is the one replaced,
//!   whatever the user named it. Files an earlier update installed that the new archive no
//!   longer has go as well (`.openomsi-files` lists them); nothing else in the folder - the
//!   mods, the content - is touched. Then the new launcher is started and this one ends; the
//!   next start deletes the `*.old-update` files.
//! * **Android**: the APK is downloaded and handed to the system's package installer
//!   (`OmsiActivity.installApk`, a PackageInstaller session). The system asks the player;
//!   Cancel comes back as an error here, Update replaces the app and starts it again.
//!
//! The network is not taken for granted: every request is tried again after a pause when it
//! fails (a timeout, a dropped connection, a 5xx), a download that breaks off goes on where it
//! stopped (an HTTP range) instead of starting over, and when the GitHub API does not answer
//! - or answers 403 because the address has used its 60 requests an hour - the check asks
//! github.com itself which release is the latest (`releases/latest` redirects to its tag) and
//! takes the file by its name, as `release.yml` names it.
//!
//! The game looks for an update too, in the background (omsi-app's `update_watch`): it downloads it
//! during the session (`Updater::prefetch`) and says so over the navigator; the launcher then
//! installs the file already downloaded when the session ends.
//!
//! `OMSI_UPDATE_URL=<url or file:///…json>` points the check at another release description
//! (for testing: a file in the GitHub API's format whose asset URLs may be `file://` too),
//! `OMSI_NO_UPDATE=1` switches the check off. A development build (run from a cargo `target`
//! folder) checks, but never replaces itself. A pull request's test build (a version such as
//! `0.1.1313-pr1192`) is never offered a release: it would replace the build being tested.

// (a phone installs through the system: the unpacking and swapping below are the computers')
#![cfg_attr(any(target_os = "android", target_os = "ios"), allow(dead_code))]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[cfg(test)]
mod downgrade_tests;
#[cfg(test)]
mod tests;
mod unpack;
pub use unpack::unpack;

/// The project on GitHub.
pub const REPO: &str = "openOMSI-org/openOMSI";
pub const REPO_URL: &str = "https://github.com/openOMSI-org/openOMSI";
const LATEST_API: &str = "https://api.github.com/repos/openOMSI-org/openOMSI/releases/latest";
const RELEASES_API: &str =
    "https://api.github.com/repos/openOMSI-org/openOMSI/releases?per_page=100";
const RELEASE_PAGES: u32 = 5;
const NEWEST_API: &str = "https://api.github.com/repos/openOMSI-org/openOMSI/releases?per_page=10";
pub const FIRST_SELF_TESTED: &str = "0.2.32";

/// A build the App Store delivers (every iPhone and iPad build, and a Mac build made with
/// `OPENOMSI_STORE=1`): the store installs its updates, and a store app may not fetch and run
/// a program of its own, so no release is ever looked for or offered.
pub const STORE_BUILD: bool = cfg!(target_os = "ios") || option_env!("OPENOMSI_STORE").is_some();

/// A release newer than this build, with the file for this platform.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    /// The release's page on GitHub.
    pub page: String,
    pub notes: String,
    pub asset_name: String,
    pub asset_url: String,
    pub size: u64,
    /// `sha256:<hex>` as GitHub lists it for the asset (None for releases older than that).
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading {
        release: Release,
        done: u64,
        total: u64,
    },
    /// Downloaded and checked, not installed (`Updater::prefetch`, the game's background
    /// download): `install` puts this file in place without fetching it again.
    Downloaded(Release),
    Installing(Release),
    /// Android: the system's installer has the APK and asks the player.
    #[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
    WaitingForInstaller(Release),
    /// A computer: the new launcher is being started; this one ends.
    Restarting(Release),
    Failed(String),
}

/// The launcher's updater: its state (shared with the worker thread), and whether the
/// player put the offer aside for this start.
pub struct Updater {
    status: Arc<Mutex<Status>>,
    pub dismissed: bool,
    /// Checked (or asked not to) once since the start.
    pub checked_once: bool,
    /// `update_auto` started the installation (once a start), and the new launcher was
    /// started.
    pub auto_started: bool,
    pub relaunched: bool,
    /// This start follows an update to this version (shown for a few seconds).
    pub updated: Option<(String, std::time::Instant)>,
    /// The last look for an update (the launcher looks again every half hour).
    pub last_check: Option<std::time::Instant>,
    /// The version the player put aside with "Not now": offered again only when a newer one
    /// comes, or when a session ends.
    pub dismissed_version: Option<String>,
    /// A game ran last frame (the launcher installs once it has ended).
    pub game_was_running: bool,
    checked_beta: Option<bool>,
    check_generation: Arc<std::sync::atomic::AtomicU64>,
}

impl Default for Updater {
    fn default() -> Self {
        Updater {
            status: Arc::new(Mutex::new(Status::Idle)),
            dismissed: false,
            checked_once: false,
            auto_started: false,
            relaunched: false,
            updated: None,
            last_check: None,
            dismissed_version: None,
            game_was_running: false,
            checked_beta: None,
            check_generation: Arc::default(),
        }
    }
}

fn lock(s: &Mutex<Status>) -> std::sync::MutexGuard<'_, Status> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

impl Updater {
    pub fn status(&self) -> Status {
        lock(&self.status).clone()
    }

    fn set(&self, s: Status) {
        *lock(&self.status) = s;
    }

    /// Ask GitHub for the latest release (in the background).
    pub fn check(&mut self) {
        self.checked_once = true;
        self.last_check = Some(std::time::Instant::now());
        let beta = beta_channel();
        let switched = self.checked_beta.is_some_and(|b| b != beta);
        if check_waits(&self.status(), switched) {
            return;
        }
        self.checked_beta = Some(beta);
        self.dismissed = false;
        let status = self.status.clone();
        let generation = self.check_generation.clone();
        let mine = start_check(&status, &generation);
        std::thread::spawn(move || {
            let s = match latest_on(beta) {
                Ok(Some(r)) => Status::Available(r),
                Ok(None) => Status::UpToDate,
                Err(e) => {
                    log::warn!("update check: {e:#}");
                    Status::Failed(format!("Could not check for updates: {e}."))
                }
            };
            let said = format!("{s:?}");
            if finish_check(&status, &generation, mine, s) {
                log::info!("update check: {said}");
            } else {
                log::info!("update check: a newer look replaced this one ({said})");
            }
        });
    }

    /// Download and install `r` (in the background).
    pub fn install(&mut self, r: Release) {
        if matches!(
            self.status(),
            Status::Downloading { .. }
                | Status::Installing(_)
                | Status::WaitingForInstaller(_)
                | Status::Restarting(_)
        ) {
            return;
        }
        self.dismissed = false;
        self.set(if downloaded(&r) {
            Status::Installing(r.clone())
        } else {
            Status::Downloading {
                release: r.clone(),
                done: 0,
                total: r.size,
            }
        });
        let status = self.status.clone();
        std::thread::spawn(move || {
            let result = download_and_install(&r, &status);
            if let Err(e) = result {
                log::warn!("update to {}: {e:#}", r.version);
                *lock(&status) =
                    Status::Failed(format!("openOMSI was not updated to {}: {e}", r.version));
            }
        });
    }

    /// Download `r` without installing it (in the background, the game's way): the file
    /// waits in the data folder, checked, until `install` puts it in place.
    pub fn prefetch(&mut self, r: Release) {
        if matches!(
            self.status(),
            Status::Downloading { .. }
                | Status::Downloaded(_)
                | Status::Installing(_)
                | Status::WaitingForInstaller(_)
                | Status::Restarting(_)
        ) {
            return;
        }
        self.set(Status::Downloading {
            release: r.clone(),
            done: 0,
            total: r.size,
        });
        let status = self.status.clone();
        std::thread::spawn(move || {
            let file = download_dir().join(&r.asset_name);
            match download(&r, &file, &status) {
                Ok(()) => {
                    log::info!(
                        "update {}: downloaded in the background ({})",
                        r.version,
                        file.display()
                    );
                    *lock(&status) = Status::Downloaded(r);
                }
                Err(e) => {
                    log::warn!("update {}: background download: {e:#}", r.version);
                    *lock(&status) = Status::Failed(format!(
                        "openOMSI {} could not be downloaded: {e}.",
                        r.version
                    ));
                }
            }
        });
    }

    /// Android: what the system's installer answered (polled every frame while it has the
    /// APK). A computer: nothing to poll.
    pub fn poll(&mut self) {
        #[cfg(target_os = "android")]
        if let Status::WaitingForInstaller(r) = self.status() {
            match crate::android::hooks().and_then(|h| (h.install_status)()) {
                Some((3, _)) => self.set(Status::Failed(format!("openOMSI was not updated to {}: the installation was cancelled.", r.version))),
                Some((4, msg)) => self.set(Status::Failed(format!("openOMSI was not updated to {}: {}", r.version, if msg.is_empty() { "the system's installer refused the package." } else { msg.as_str() }))),
                Some((6, _)) => self.set(Status::Failed(format!("openOMSI was not updated to {}: installing apps was not allowed for openOMSI (Settings → Apps → openOMSI → Install unknown apps).", r.version))),
                // (2, success: the system ends this process and starts the new app)
                _ => {}
            }
        }
    }

    /// Forget a failure or an offer (the dialog's "Close" / "Not now").
    pub fn dismiss(&mut self) {
        self.dismissed = true;
        if let Status::Available(r) | Status::Downloaded(r) = self.status() {
            self.dismissed_version = Some(r.version);
        }
        if matches!(self.status(), Status::Failed(_) | Status::UpToDate) {
            self.set(Status::Idle);
        }
    }
}

// --- versions ------------------------------------------------------------------------------

/// This build's version.
pub fn current_version() -> &'static str {
    crate::VERSION
}

/// `0.1.7` / `v0.1.7` as numbers (missing parts are 0).
fn version_parts(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches(['v', 'V'])
        .split(['.', '-', '+'])
        .map_while(|p| p.parse::<u64>().ok())
        .collect()
}

/// A pull request's test build: `release.yml` gives it the version `<release>-pr<number>`.
pub fn is_test_build(version: &str) -> bool {
    version.contains("-pr")
}

/// Whether `candidate` is a newer version than `current`.
pub fn newer(candidate: &str, current: &str) -> bool {
    !version_parts(candidate).is_empty() && compare(candidate, current).is_gt()
}

fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut a, mut b) = (version_parts(a), version_parts(b));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a.cmp(&b)
}

/// The release file for this platform, as `release.yml` names it.
pub fn asset_name(version: &str) -> Option<String> {
    let suffix = if cfg!(target_os = "android") {
        "android-arm64.apk"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "windows-x64.zip"
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        "windows-arm64.zip"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos-arm64.zip"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "macos-x64.zip"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x64.zip"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "linux-arm64.zip"
    } else {
        return None;
    };
    Some(format!("openOMSI-{version}-{suffix}"))
}

// --- the release ----------------------------------------------------------------------------

pub fn agent() -> ureq::Agent {
    // (a read waits 30 s at most - a stalled download goes on with a range request, so a
    // short wait costs nothing; the connection gets 20 s, enough for a slow line or a
    // resolver that tries IPv6 first)
    let b = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(20))
        .timeout_read(std::time::Duration::from_secs(30))
        .user_agent(&format!("openOMSI/{} (updater)", current_version()));
    // (through the proxy the system is set to use, #1000)
    match crate::proxy::system_proxy().and_then(|p| {
        ureq::Proxy::new(&p)
            .map_err(|e| {
                log::warn!(
                    "network: the proxy {} cannot be used: {e}",
                    crate::proxy::redact(&p)
                )
            })
            .ok()
    }) {
        Some(proxy) => b.proxy(proxy).build(),
        None => b.build(),
    }
}

/// How often a request is tried before it counts as failed, and the pauses between.
const TRIES: [u64; 3] = [0, 3, 10];

/// Whether a failed request is worth another try: the network (a timeout, a dropped or
/// refused connection, a name not resolved for now) or the server's own trouble (429, 5xx).
fn transient(e: &ureq::Error) -> bool {
    match e {
        ureq::Error::Status(code, _) => *code == 429 || *code >= 500,
        ureq::Error::Transport(_) => true,
    }
}

/// `f` until it works, at most `TRIES` times with the pauses between (only for failures
/// that may pass, see `transient`).
fn with_retries<T>(
    what: &str,
    mut f: impl FnMut() -> Result<T, ureq::Error>,
) -> Result<T, ureq::Error> {
    let mut last = None;
    for (k, pause) in TRIES.iter().enumerate() {
        if *pause > 0 {
            std::thread::sleep(std::time::Duration::from_secs(*pause));
        }
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if transient(&e) && k + 1 < TRIES.len() => {
                log::info!("update: {what}: {} - trying again", short_error(&e));
                last = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last.expect("at least one try"))
}

/// A URL's body: `file://` read from the disk (tests), anything else over HTTP(S).
fn fetch_text(url: &str) -> anyhow::Result<String> {
    if let Some(p) = url.strip_prefix("file://") {
        return Ok(std::fs::read_to_string(p)?);
    }
    let r = with_retries("asking GitHub", || {
        agent()
            .get(url)
            .set("Accept", "application/vnd.github+json")
            .call()
    })
    .map_err(|e| anyhow::anyhow!("{}", short_error(&e)))?;
    Ok(r.into_string()?)
}

fn short_error(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Status(403 | 429, _) => {
            "GitHub refused for now (too many requests from this address); it is tried again later"
                .to_string()
        }
        ureq::Error::Status(code, _) => format!("the server answered with an error ({code})"),
        ureq::Error::Transport(t) => match t.kind() {
            ureq::ErrorKind::Dns => {
                "the name github.com could not be looked up (no internet?)".to_string()
            }
            ureq::ErrorKind::ConnectionFailed => {
                "no connection to GitHub (a firewall or a proxy?)".to_string()
            }
            ureq::ErrorKind::Io => format!(
                "the connection to GitHub broke off or timed out ({})",
                t.message().unwrap_or("no answer")
            ),
            k => format!("no connection ({k})"),
        },
    }
}

/// The latest release when it is newer than this build and has a file for this platform.
pub fn latest() -> anyhow::Result<Option<Release>> {
    latest_on(beta_channel())
}

fn start_check(status: &Mutex<Status>, generation: &std::sync::atomic::AtomicU64) -> u64 {
    let mut s = lock(status);
    *s = Status::Checking;
    generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
}

fn finish_check(
    status: &Mutex<Status>,
    generation: &std::sync::atomic::AtomicU64,
    mine: u64,
    result: Status,
) -> bool {
    let mut s = lock(status);
    let newest = generation.load(std::sync::atomic::Ordering::SeqCst) == mine
        && matches!(*s, Status::Checking);
    if newest {
        *s = result;
    }
    newest
}

fn check_waits(status: &Status, switched: bool) -> bool {
    match status {
        Status::Downloading { .. }
        | Status::Installing(_)
        | Status::WaitingForInstaller(_)
        | Status::Restarting(_) => true,
        Status::Checking | Status::Downloaded(_) => !switched,
        _ => false,
    }
}

fn latest_on(beta: bool) -> anyhow::Result<Option<Release>> {
    if STORE_BUILD {
        return Ok(None);
    }
    let url = omsi_cfg::flags::OMSI_UPDATE_URL
        .var()
        .map(str::to_string)
        .unwrap_or_else(|| LATEST_API.to_string());
    if url != LATEST_API {
        let v = serde_json::from_str(&fetch_text(&url)?)?;
        return parse_release(&v, current_version(), false);
    }
    if beta {
        return match fetch_text(NEWEST_API)
            .and_then(|t| Ok(serde_json::from_str::<serde_json::Value>(&t)?))
        {
            Ok(v) => newest_of(&v, current_version(), &crate::recovery::listed_broken()),
            Err(e) => {
                log::info!(
                    "update check (beta): the GitHub API: {e:#} - asking github.com for the stable release"
                );
                latest_from_site().map_err(|e2| anyhow::anyhow!("{e} (and github.com: {e2})"))
            }
        };
    }
    let broken = crate::recovery::listed_broken();
    match fetch_text(&url).and_then(|t| Ok(serde_json::from_str::<serde_json::Value>(&t)?)) {
        Ok(v) => stable_release(
            &v,
            current_version(),
            &broken,
            !cfg!(any(target_os = "android", target_os = "ios")),
        ),
        // (the API: an address over its limit gets 403 for an hour, and some networks reach
        // github.com but not api.github.com - github.com says the latest tag as well)
        Err(e) => {
            log::info!("update check: the GitHub API: {e:#} - asking github.com");
            latest_from_site()
                .map(|r| r.filter(|r| !broken.contains(&r.version)))
                .map_err(|e2| anyhow::anyhow!("{e} (and github.com: {e2})"))
        }
    }
}

fn stable_release(
    v: &serde_json::Value,
    current: &str,
    broken: &[String],
    can_downgrade: bool,
) -> anyhow::Result<Option<Release>> {
    let version = v["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("the release has no tag"))?
        .trim_start_matches(['v', 'V']);
    if broken.iter().any(|b| b == version) {
        return Ok(None);
    }
    if let Some(upgrade) = parse_release(v, current, false)? {
        return Ok(Some(upgrade));
    }
    if !can_downgrade
        || is_test_build(current)
        || !broken.iter().any(|b| b == current)
        || v["draft"].as_bool() != Some(false)
        || v["prerelease"].as_bool() != Some(false)
        || !newer(current, version)
        || newer(FIRST_SELF_TESTED, version)
        || crate::recovery::is_bad(version)
    {
        return Ok(None);
    }
    Ok(release_of(v)?.filter(|r| {
        r.sha256
            .as_deref()
            .is_some_and(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
    }))
}

/// The latest release as github.com's `releases/latest` redirect names it, with this
/// platform's file at the address `release.yml` publishes it (no SHA-256: GitHub's TLS is
/// all that vouches for it then, and the size is checked by the download itself).
fn latest_from_site() -> anyhow::Result<Option<Release>> {
    let page = format!("{REPO_URL}/releases/latest");
    let r = with_retries("asking github.com", || agent().head(&page).call())
        .map_err(|e| anyhow::anyhow!("{}", short_error(&e)))?;
    let tag = r.get_url().rsplit('/').next().unwrap_or("").to_string();
    if !tag.starts_with(['v', 'V']) {
        anyhow::bail!("github.com named no release ({})", r.get_url());
    }
    let version = tag.trim_start_matches(['v', 'V']).to_string();
    if is_test_build(current_version())
        || !newer(&version, current_version())
        || crate::recovery::is_bad(&version)
    {
        return Ok(None);
    }
    let Some(name) = asset_name(&version) else {
        return Ok(None);
    };
    let url = format!("{REPO_URL}/releases/download/{tag}/{name}");
    // (the files come a few minutes after the tag: not there yet, nothing to offer yet)
    let size = match with_retries("looking for the file", || agent().head(&url).call()) {
        Ok(r) => r
            .header("Content-Length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(e) => anyhow::bail!("{}", short_error(&e)),
    };
    Ok(Some(Release {
        version,
        page: format!("{REPO_URL}/releases/tag/{tag}"),
        notes: String::new(),
        asset_name: name,
        asset_url: url,
        size,
        sha256: None,
    }))
}

pub fn beta_channel() -> bool {
    let text = std::fs::read_to_string(omsi_launcher_lib::data_dir().join("settings.cfg")).ok();
    omsi_launcher_lib::settings_from_text(text.as_deref())["update_channel"] == "beta"
}

fn newest_of(
    v: &serde_json::Value,
    current: &str,
    broken: &[String],
) -> anyhow::Result<Option<Release>> {
    let tag = |r: &serde_json::Value| r["tag_name"].as_str().unwrap_or("").to_string();
    let mut list: Vec<&serde_json::Value> = v
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("GitHub listed no releases"))?
        .iter()
        .filter(|r| r["draft"].as_bool() != Some(true) && !version_parts(&tag(r)).is_empty())
        .filter(|r| !broken.iter().any(|b| compare(b, &tag(r)).is_eq()))
        .collect();
    list.sort_by(|a, b| compare(&tag(b), &tag(a)));
    for r in list {
        if let Some(found) = parse_release(r, current, true)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// A release described as the GitHub API does, when newer than `current`.
fn parse_release(
    v: &serde_json::Value,
    current: &str,
    beta: bool,
) -> anyhow::Result<Option<Release>> {
    let tag = v["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("the release has no tag"))?;
    let version = tag.trim_start_matches(['v', 'V']).to_string();
    if is_test_build(current) {
        log::info!(
            "update check: {current} is a pull request's test build, {version} is not offered"
        );
        return Ok(None);
    }
    if v["draft"].as_bool() == Some(true)
        || (!beta && v["prerelease"].as_bool() == Some(true))
        || !newer(&version, current)
    {
        return Ok(None);
    }
    // (a release that failed to start here, or would not even run its self-test, is not
    // offered again: only a newer one is, see `recovery`)
    if crate::recovery::is_bad(&version) {
        log::info!(
            "update check: {version} failed on this computer before and is not offered again"
        );
        return Ok(None);
    }
    let found = release_of(v)?;
    if let (None, Some(want)) = (&found, asset_name(&version)) {
        // (the release is still being built: its files come a few minutes after the tag)
        log::info!("update check: {version} has no {want} (yet)");
    }
    Ok(found)
}

fn release_of(v: &serde_json::Value) -> anyhow::Result<Option<Release>> {
    let tag = v["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("the release has no tag"))?;
    let version = tag.trim_start_matches(['v', 'V']).to_string();
    let Some(want) = asset_name(&version) else {
        return Ok(None);
    };
    let Some(a) = v["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["name"].as_str() == Some(want.as_str())))
    else {
        return Ok(None);
    };
    Ok(Some(Release {
        version,
        page: v["html_url"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| format!("{REPO_URL}/releases/tag/{tag}")),
        notes: v["body"].as_str().unwrap_or("").to_string(),
        asset_name: want,
        asset_url: a["browser_download_url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("the release file has no address"))?
            .to_string(),
        size: a["size"].as_u64().unwrap_or(0),
        sha256: a["digest"]
            .as_str()
            .and_then(|d| d.strip_prefix("sha256:"))
            .map(|h| h.to_ascii_lowercase()),
    }))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Listed {
    pub release: Release,
    pub published: String,
    pub beta: bool,
}

pub fn releases() -> anyhow::Result<Vec<Listed>> {
    if STORE_BUILD {
        return Ok(Vec::new());
    }
    releases_paged(
        |page| fetch_text(&format!("{RELEASES_API}&page={page}")),
        beta_channel(),
    )
}

fn releases_paged(
    fetch: impl Fn(u32) -> anyhow::Result<String>,
    beta: bool,
) -> anyhow::Result<Vec<Listed>> {
    let mut all = Vec::new();
    for page in 1..=RELEASE_PAGES {
        let items =
            match fetch(page).and_then(|t| Ok(serde_json::from_str::<serde_json::Value>(&t)?)) {
                Ok(serde_json::Value::Array(items)) => items,
                Err(e) if page == 1 => return Err(e),
                _ => break,
            };
        let full = items.len() == 100;
        let too_old = items
            .iter()
            .filter_map(|r| r["tag_name"].as_str())
            .any(|t| newer(FIRST_SELF_TESTED, t.trim_start_matches(['v', 'V'])));
        all.extend(items);
        if !full || too_old {
            break;
        }
    }
    Ok(releases_of(&serde_json::Value::Array(all), beta))
}

fn releases_of(v: &serde_json::Value, beta: bool) -> Vec<Listed> {
    let mut list: Vec<Listed> = v
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| {
            r["draft"].as_bool() != Some(true) && (beta || r["prerelease"].as_bool() != Some(true))
        })
        .filter_map(|r| {
            let release = release_of(r).ok().flatten()?;
            let published = r["published_at"]
                .as_str()
                .and_then(|d| d.get(..10))
                .unwrap_or("")
                .to_string();
            Some(Listed {
                release,
                published,
                beta: r["prerelease"].as_bool() == Some(true),
            })
        })
        .filter(|l| {
            !is_test_build(&l.release.version) && !newer(FIRST_SELF_TESTED, &l.release.version)
        })
        .collect();
    list.sort_by(|a, b| compare(&b.release.version, &a.release.version));
    list.dedup_by(|a, b| compare(&a.release.version, &b.release.version).is_eq());
    list
}

/// Where downloads wait (the data folder: the program's own folder is only written when the
/// new files go in).
fn download_dir() -> PathBuf {
    let d = omsi_launcher_lib::data_dir().join("updates");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// The note beside a downloaded release file that says it is whole and checked (its size
/// and SHA-256), so that `install` can take it without fetching it again.
fn checked_note(file: &Path) -> PathBuf {
    file.with_extension("checked")
}

/// Whether `r`'s file waits downloaded and checked in the data folder. A file fetched
/// without a SHA-256 to check it by (by github.com's way, see `latest_from_site`) is hashed
/// now when `r` brings one.
pub fn downloaded(r: &Release) -> bool {
    use sha2::Digest;
    let file = download_dir().join(&r.asset_name);
    let note = std::fs::read_to_string(checked_note(&file)).unwrap_or_default();
    let len = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
    let Some((n, sha)) = note.trim().split_once(' ') else {
        return false;
    };
    if len == 0 || n.parse::<u64>().ok() != Some(len) || (r.size > 0 && r.size != len) {
        return false;
    }
    match (r.sha256.as_deref(), sha) {
        (None, _) => true,
        (Some(want), got) if got == want => true,
        (Some(want), "-") => {
            let Ok(mut f) = std::fs::File::open(&file) else {
                return false;
            };
            // (read in blocks: sha2 0.11's hasher is no `io::Write` any more)
            let mut h = sha2::Sha256::new();
            let mut buf = vec![0u8; 1 << 16];
            loop {
                match std::io::Read::read(&mut f, &mut buf) {
                    Ok(0) => break,
                    Ok(n) => h.update(&buf[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => return false,
                }
            }
            let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
            got == want
        }
        _ => false,
    }
}

/// Download the release file to `to`, with the progress in `status`, and check it. A break
/// in the middle (a timeout, a dropped connection) goes on from where it stopped, a few
/// times, before the download counts as failed.
fn download(r: &Release, to: &Path, status: &Mutex<Status>) -> anyhow::Result<()> {
    use sha2::Digest;
    if downloaded(r) {
        return Ok(());
    }
    let part = to.with_extension("part");
    if std::fs::metadata(&part)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < std::time::Duration::from_secs(15))
    {
        anyhow::bail!(
            "another download of openOMSI {} is under way; try again in a moment",
            r.version
        );
    }
    let _ = std::fs::remove_file(checked_note(to));
    let mut hasher = sha2::Sha256::new();
    let mut out = std::fs::File::create(&part)?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    let mut total = r.size;
    // (a break costs a try; one that brought data in between gives the tries back)
    let mut breaks = 0;
    loop {
        let mut reader: Box<dyn Read> = if let Some(p) = r.asset_url.strip_prefix("file://") {
            let f = std::fs::File::open(p)?;
            total = f.metadata()?.len();
            Box::new(f)
        } else {
            let from = done;
            let resp = with_retries("downloading", || {
                let req = agent()
                    .get(&r.asset_url)
                    .set("Accept", "application/octet-stream");
                if from > 0 {
                    req.set("Range", &format!("bytes={from}-")).call()
                } else {
                    req.call()
                }
            })
            .map_err(|e| anyhow::anyhow!("{}", short_error(&e)))?;
            if done > 0 && resp.status() != 206 {
                // (the server sends the whole file again: start over)
                log::info!(
                    "update {}: the server does not continue a download; starting over",
                    r.version
                );
                hasher = sha2::Sha256::new();
                out = std::fs::File::create(&part)?;
                done = 0;
            }
            if done == 0 {
                total = resp
                    .header("Content-Length")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(r.size);
            }
            Box::new(resp.into_reader())
        };
        let before = done;
        let broke = loop {
            match reader.read(&mut buf) {
                Ok(0) => break None,
                Ok(n) => {
                    out.write_all(&buf[..n])?;
                    hasher.update(&buf[..n]);
                    done += n as u64;
                    *lock(status) = Status::Downloading {
                        release: r.clone(),
                        done,
                        total: total.max(done),
                    };
                }
                Err(e) => break Some(e),
            }
        };
        match broke {
            None if total == 0 || done >= total => break,
            // (ended early: the connection was cut without an error)
            None => {}
            Some(e) => log::info!(
                "update {}: the download broke off at {} of {} bytes ({e}) - going on",
                r.version,
                done,
                total
            ),
        }
        if done > before {
            breaks = 0;
        }
        breaks += 1;
        if breaks > 4 {
            anyhow::bail!(
                "the download kept breaking off at {} of {} bytes - the connection to GitHub is too unsteady; it is tried again later",
                done,
                total
            );
        }
        if r.asset_url.starts_with("file://") {
            anyhow::bail!("the file ended at {} of {} bytes", done, total);
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    out.flush()?;
    drop(out);
    if r.size > 0 && done != r.size {
        anyhow::bail!("the download stopped at {} of {} bytes", done, r.size);
    }
    if let Some(want) = &r.sha256 {
        let got: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if &got != want {
            let _ = std::fs::remove_file(&part);
            anyhow::bail!("the downloaded file is damaged (SHA-256 {got}, GitHub lists {want})");
        }
    }
    std::fs::rename(&part, to)?;
    let _ = std::fs::write(
        checked_note(to),
        format!("{} {}", done, r.sha256.as_deref().unwrap_or("-")),
    );
    Ok(())
}

/// Download and install `r` now, on this thread (the start guard's rescue, which has no
/// launcher to show the progress in).
pub fn install_now(r: &Release) -> anyhow::Result<()> {
    download_and_install(r, &Mutex::new(Status::Installing(r.clone())))
}

fn download_and_install(r: &Release, status: &Mutex<Status>) -> anyhow::Result<()> {
    // (on a computer: where it goes must be writable before 15 MB are fetched for nothing)
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let place = install_place()?;
        if !writable(&place.dir) {
            if cfg!(windows) {
                anyhow::bail!(
                    "the folder {} cannot be written. Put openOMSI in a folder of yours (or start it once as administrator) and update again",
                    short_path(&place.dir)
                );
            }
            anyhow::bail!(
                "the folder {} cannot be written. Put openOMSI in a folder of yours and update again",
                short_path(&place.dir)
            );
        }
    }
    let file = download_dir().join(&r.asset_name);
    download(r, &file, status)?;
    log::info!("update {}: downloaded {}", r.version, file.display());
    *lock(status) = Status::Installing(r.clone());
    #[cfg(target_os = "android")]
    {
        (crate::android::hooks()
            .ok_or_else(|| anyhow::anyhow!("the system's package installer could not be reached"))?
            .install_apk)(&file)?;
        // (the new version is noted: the app that starts after the update says so)
        let _ = std::fs::write(download_dir().join("updating-to"), &r.version);
        *lock(status) = Status::WaitingForInstaller(r.clone());
        Ok(())
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let place = install_place()?;
        if let Err(e) = install_archive(&file, &place) {
            // (a program that cannot start here is never offered again, only a newer one)
            if e.downcast_ref::<crate::recovery::SelfTestFailed>()
                .is_some()
            {
                crate::recovery::mark_bad(&r.version);
            }
            return Err(e);
        }
        let _ = std::fs::write(download_dir().join("updating-to"), &r.version);
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_file(checked_note(&file));
        *lock(status) = Status::Restarting(r.clone());
        Ok(())
    }
    #[cfg(target_os = "ios")]
    {
        let _ = file;
        anyhow::bail!("openOMSI {} comes from the App Store", r.version)
    }
}

// --- installing on a computer -----------------------------------------------------------------

/// Where this program is installed: its folder, and on macOS the `.app` bundle it runs in.
#[derive(Clone, Debug)]
pub struct Place {
    pub dir: PathBuf,
    pub bundle: Option<PathBuf>,
    pub exe: PathBuf,
}

/// The installation this process runs from (refused for a development build).
pub fn install_place() -> anyhow::Result<Place> {
    // (taken once: on Linux `current_exe` follows the running file, so after the swap it
    // named `openomsi.old-update` - the old program, started again as "the new launcher" -
    // and after a second swap a deleted file that could not be started at all, #811)
    static EXE: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    let exe = EXE
        .get_or_init(|| {
            std::env::current_exe()
                .ok()
                .map(|e| program_path(&e.canonicalize().unwrap_or(e)))
        })
        .clone()
        .ok_or_else(|| anyhow::anyhow!("the program's own path is not known"))?;
    if is_dev_build(&exe) {
        anyhow::bail!(
            "this is a development build ({}); update it with git and cargo",
            short_path(&exe)
        );
    }
    // (macOS runs an app opened straight from Downloads from a read-only copy elsewhere)
    if exe.to_string_lossy().contains("/AppTranslocation/") {
        anyhow::bail!(
            "macOS runs openOMSI from a temporary read-only copy. Move openOMSI.app into your Applications folder (or any folder), start it from there and update again"
        );
    }
    let bundle = exe
        .ancestors()
        .find(|p| {
            p.extension()
                .map(|e| e.eq_ignore_ascii_case("app"))
                .unwrap_or(false)
        })
        .map(Path::to_path_buf);
    let dir = bundle
        .as_deref()
        .unwrap_or(&exe)
        .parent()
        .ok_or_else(|| anyhow::anyhow!("no folder around {}", exe.display()))?
        .to_path_buf();
    Ok(Place { dir, bundle, exe })
}

/// The program's own path from what the system says the running file is: on Linux that
/// follows a rename (`openomsi.old-update`) and an unlinked file (`... (deleted)`), but the
/// program to start is the one at the original name.
fn program_path(exe: &Path) -> PathBuf {
    let mut s = exe.to_string_lossy().to_string();
    if let Some(t) = s.strip_suffix(" (deleted)") {
        s = t.to_string();
    }
    while let Some(t) = s.strip_suffix(OLD) {
        // (`openomsi.exe.2.old-update`: put aside beside an older one, see `aside_of`)
        s = match t.rsplit_once('.') {
            Some((head, n))
                if !head.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) =>
            {
                head.to_string()
            }
            _ => t.to_string(),
        };
    }
    if s == exe.to_string_lossy() {
        exe.to_path_buf()
    } else {
        PathBuf::from(s)
    }
}

/// A path short enough for a dialog's line: its end, where the telling part is.
fn short_path(p: &Path) -> String {
    let s = p.display().to_string();
    let n = s.chars().count();
    if n <= 48 {
        s
    } else {
        format!("...{}", s.chars().skip(n - 46).collect::<String>())
    }
}

/// Whether files can be put into `dir` (the check before anything is downloaded).
fn writable(dir: &Path) -> bool {
    let probe = dir.join(".openomsi-write-test");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Run from a cargo `target/<profile>` folder.
pub fn is_dev_build(exe: &Path) -> bool {
    let text = exe.to_string_lossy().to_ascii_lowercase();
    let parts: Vec<&str> = text.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    let profile = |s: &str| s == "release" || s == "debug";
    parts.windows(2).any(|w| w[0] == "target" && profile(w[1]))
        || parts.windows(3).any(|w| w[0] == "target" && profile(w[2]))
        || in_cargo_target_dir(exe)
}

/// Whether `exe` was built into a Cargo target directory of any name (`CARGO_TARGET_DIR`):
/// Cargo marks its root with a `CACHEDIR.TAG`, one or two folders above the program. A build
/// in a folder not called `target` was taken for an installed game, so the start guard
/// "updated" it from the releases and opened its dialog over the desktop.
fn in_cargo_target_dir(exe: &Path) -> bool {
    exe.ancestors()
        .skip(2)
        .take(2)
        .any(|d| std::fs::read_to_string(d.join("CACHEDIR.TAG")).is_ok_and(|t| t.contains("cargo")))
}

const STAGING: &str = ".openomsi-update";
pub(crate) const MANIFEST: &str = ".openomsi-files";
const OLD: &str = ".old-update";

fn old_of(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(OLD);
    PathBuf::from(s)
}

/// Where `target` is put aside: its `*.old-update`, or `*.<n>.old-update` while an earlier
/// one cannot be deleted - still loaded (the launcher's DirectX shader compiler,
/// `dxcompiler.dll`) or held by a virus scanner. Windows renames nothing onto a file that
/// is there, and the update failed with "cannot be replaced" (#1753).
fn aside_of(target: &Path) -> PathBuf {
    let first = old_of(target);
    remove_any(&first);
    if !first.exists() && !first.is_symlink() {
        return first;
    }
    for n in 2..100 {
        let mut s = target.as_os_str().to_os_string();
        s.push(format!(".{n}{OLD}"));
        let p = PathBuf::from(s);
        remove_any(&p);
        if !p.exists() && !p.is_symlink() {
            return p;
        }
    }
    first
}

/// What the archive's top-level item `name` replaces in the installation.
pub(crate) fn target_of(name: &str, place: &Place) -> PathBuf {
    // (the macOS app, whatever the player named the one that runs)
    if name.to_ascii_lowercase().ends_with(".app") {
        if let Some(b) = &place.bundle {
            return b.clone();
        }
    }
    place.dir.join(name)
}

/// Put the program files of the release archive `zip` into the installation at `place`,
/// once its program has passed its self-test. The files it replaces stay beside it as
/// `*.old-update`, with the version they belong to (`.openomsi-previous`), until the new
/// version has started well once (`cleanup_after_update`): until then the start guard can
/// go back to them (`recovery::rollback`).
pub fn install_archive(zip: &Path, place: &Place) -> anyhow::Result<()> {
    install_archive_with(zip, place, true)
}

pub(crate) fn install_archive_with(
    zip: &Path,
    place: &Place,
    self_test: bool,
) -> anyhow::Result<()> {
    let staging = place.dir.join(STAGING);
    unpack(zip, &staging).map_err(|e| {
        anyhow::anyhow!(
            "could not unpack the update into {} ({e})",
            short_path(&place.dir)
        )
    })?;
    let mut items: Vec<String> = std::fs::read_dir(&staging)?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    if place.bundle.is_some() {
        items.retain(|n| n.to_ascii_lowercase().ends_with(".app"));
    }
    items.sort();
    if items.is_empty() {
        anyhow::bail!("the update archive is empty");
    }
    // a program must be among them: a wrong archive must not replace anything
    let has_program = items.iter().any(|n| {
        let l = n.to_ascii_lowercase();
        l == "openomsi" || l == "openomsi.exe" || l.ends_with(".app")
    });
    if !has_program {
        anyhow::bail!("the update archive holds no openOMSI program");
    }
    // the new program must start before it replaces the old one: a build that cannot even
    // be loaded here (a library missing, a broken file) stops now, with nothing replaced
    if self_test {
        let program = crate::recovery::staged_program(&staging)
            .ok_or_else(|| anyhow::anyhow!("the update archive holds no program to start"))?;
        if let Err(e) = crate::recovery::self_test(&program, std::time::Duration::from_secs(30)) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e.into());
        }
    }
    // what an earlier update put here and this one no longer brings
    let before: Vec<String> = std::fs::read_to_string(place.dir.join(MANIFEST))
        .map(|t| {
            t.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty() && !l.contains(['/', '\\']) && l != ".." && l != ".")
                .collect()
        })
        .unwrap_or_default();
    let gone: Vec<String> = before
        .into_iter()
        .filter(|n| !items.iter().any(|i| i.eq_ignore_ascii_case(n)))
        .collect();
    // the version these files are, for going back to it; and the earlier list of what an
    // update brought, set aside with the files it lists
    let _ = std::fs::write(place.dir.join(crate::recovery::PREVIOUS), current_version());
    let manifest = place.dir.join(MANIFEST);
    let mut manifest_aside = None;
    if manifest.exists() {
        let aside = aside_of(&manifest);
        if std::fs::rename(&manifest, &aside).is_ok() {
            manifest_aside = Some(aside);
        }
    }
    // swap: the old file aside, the new one in; undone completely on the first failure
    let mut moved: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    let result = (|| -> anyhow::Result<()> {
        for name in &items {
            let target = target_of(name, place);
            let old = aside_of(&target);
            let had = target.exists() || target.is_symlink();
            if had {
                std::fs::rename(&target, &old).map_err(|e| {
                    anyhow::anyhow!(
                        "{} cannot be replaced ({e}) - is the folder writable?",
                        short_path(&target)
                    )
                })?;
            }
            moved.push((target.clone(), had.then_some(old)));
            std::fs::rename(staging.join(name), &target)
                .map_err(|e| anyhow::anyhow!("{} cannot be written ({e})", short_path(&target)))?;
        }
        for name in &gone {
            let target = place.dir.join(name);
            if target.exists() {
                let old = old_of(&target);
                remove_any(&old);
                std::fs::rename(&target, &old)?;
                moved.push((target, None));
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        // back as it was
        for (target, old) in moved.into_iter().rev() {
            match old {
                Some(old) => {
                    remove_any(&target);
                    let _ = std::fs::rename(&old, &target);
                }
                None => {
                    let o = old_of(&target);
                    if o.exists() {
                        let _ = std::fs::rename(&o, &target);
                    } else {
                        remove_any(&target);
                    }
                }
            }
        }
        let _ = std::fs::remove_dir_all(&staging);
        if let Some(aside) = manifest_aside {
            let _ = std::fs::rename(aside, &manifest);
        }
        let _ = std::fs::remove_file(place.dir.join(crate::recovery::PREVIOUS));
        return Err(e);
    }
    let _ = std::fs::write(place.dir.join(MANIFEST), items.join("\n") + "\n");
    let _ = std::fs::remove_dir_all(&staging);
    log::info!(
        "update: {} replaced in {}",
        items.join(", "),
        place.dir.display()
    );
    Ok(())
}

fn remove_any(p: &Path) {
    if p.is_dir() && !p.is_symlink() {
        let _ = std::fs::remove_dir_all(p);
    } else {
        let _ = std::fs::remove_file(p);
    }
}

/// Start the new launcher (after `install_archive`): the program file at the same place -
/// on macOS inside the bundle, which makes it the app with its icon - with this process's
/// environment.
pub fn relaunch(place: &Place) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new(&place.exe);
    cmd.current_dir(&place.dir);
    cmd.spawn()?;
    Ok(())
}

/// Once this version has started well (the start guard's `healthy`): the files an update set
/// aside, kept until now to go back to, are no longer needed - away with them (tried for a
/// while: on Windows the old program may take a moment to end).
pub fn cleanup_after_update() {
    std::thread::spawn(cleanup_now);
}

/// `cleanup_after_update` on this thread (the start guard's watcher, which ends right after).
pub fn cleanup_now() {
    let Ok(place) = install_place() else { return };
    {
        for _ in 0..20 {
            let mut left = false;
            if let Ok(rd) = std::fs::read_dir(&place.dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.file_name()
                        .map(|n| {
                            let n = n.to_string_lossy();
                            n.ends_with(OLD) || n.ends_with(crate::recovery::BAD)
                        })
                        .unwrap_or(false)
                    {
                        remove_any(&p);
                        left |= p.exists();
                    }
                }
            }
            let staging = place.dir.join(STAGING);
            if staging.exists() {
                remove_any(&staging);
            }
            let _ = std::fs::remove_file(place.dir.join(crate::recovery::PREVIOUS));
            if !left {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
}

/// After an update: the version this start was updated to (once), for the launcher to say.
pub fn just_updated() -> Option<String> {
    let p = download_dir().join("updating-to");
    let v = std::fs::read_to_string(&p).ok()?.trim().to_string();
    if v.is_empty() {
        return None;
    }
    // (only when it is this build: a cancelled Android installation leaves the note)
    if !newer(&v, current_version()) {
        let _ = std::fs::remove_file(&p);
        // old APKs and archives go too
        if let Ok(rd) = std::fs::read_dir(download_dir()) {
            for e in rd.flatten() {
                let _ = std::fs::remove_file(e.path());
            }
        }
        return (v == current_version()).then_some(v);
    }
    None
}

/// Open a web page in the system's browser.
pub fn open_url(url: &str) {
    #[cfg(target_os = "android")]
    let _ = crate::android::hooks().map(|h| (h.open_url)(url));
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("rundll32")
        .arg("url.dll,FileProtocolHandler")
        .arg(url)
        .spawn();
    #[cfg(target_os = "ios")]
    {
        // (the Swift app's UIApplication.open, linked into the same program, see omsi-app's
        // `apple.rs`)
        unsafe extern "C" {
            fn openomsi_swift_open_url(url: *const std::ffi::c_char);
        }
        if let Ok(u) = std::ffi::CString::new(url) {
            // SAFETY: a nul-terminated string the call only reads
            unsafe { openomsi_swift_open_url(u.as_ptr()) }
        }
    }
    #[cfg(all(
        unix,
        not(target_os = "macos"),
        not(target_os = "android"),
        not(target_os = "ios")
    ))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}
