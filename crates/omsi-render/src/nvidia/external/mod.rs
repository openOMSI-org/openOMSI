//! Explicit, slow first-frame transport to an independently started, trusted host.
//! No DLL loading, injection, subprocess launch or hardware capability substitution.
mod capture;
mod protocol;
mod resources;
use anyhow::{Context, Result, ensure};
use protocol::*;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Disabled,
    UnsupportedOperatingSystem,
    UnsupportedGraphicsAPI,
    BackendUnavailable,
    ReadyForTransport,
    Capturing,
    CaptureComplete,
    EvaluationFailed,
    Active,
}

/// Cumulative evidence; Active requires an accepted response AND GPU composition completion.
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    pub state: State,
    pub requests: u64,
    pub evaluations: u64,
    pub compositions: u64,
    pub captures: u64,
    pub last_stages: u32,
    pub last_result: i32,
    pub last_error: Option<String>,
}
impl Diagnostics {
    pub fn runtime_loaded(&self) -> bool {
        self.last_stages & LOADED != 0
    }
    pub fn feature_initialized(&self) -> bool {
        self.last_stages & INITIALIZED != 0
    }
    pub fn evaluation_attempted(&self) -> bool {
        self.last_stages & ATTEMPTED != 0
    }
}

struct Session {
    directory: PathBuf,
    id: u64,
    frame: u64,
    timeout: Duration,
}
impl Session {
    fn connect(directory: &Path, timeout: Duration) -> Result<Self> {
        ensure!(
            (Duration::from_millis(10)..=Duration::from_secs(30)).contains(&timeout),
            "timeout outside 10..30000ms"
        );
        ensure!(
            !fs::symlink_metadata(directory)?.file_type().is_symlink(),
            "session directory must not be a symlink"
        );
        let directory = directory.canonicalize()?;
        ensure!(directory.is_dir(), "session is not a directory");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                fs::metadata(&directory)?.permissions().mode() & 0o077 == 0,
                "session directory must be private (0700)"
            );
        }
        ensure!(
            fs::read_to_string(directory.join("host.ready"))? == "openomsi-streamline-host/1\n",
            "unknown host protocol"
        );
        // Nonce prevents old outputs from another run being accepted. This private local
        // transport is not an authentication protocol for untrusted directories/hosts.
        let id = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos() as u64)
            ^ u64::from(std::process::id());
        let mut lock = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("client.lock"))
            .context(
                "session already owned; remove a stale client.lock only after stopping its owner",
            )?;
        let session = Self {
            directory,
            id,
            frame: 0,
            timeout,
        };
        write!(lock, "{} {id}", std::process::id())?;
        Ok(session)
    }
    fn exchange(
        &mut self,
        mut header: Header,
        payload: &[u8],
        diagnostics: &mut Diagnostics,
    ) -> Result<Vec<u8>> {
        self.frame = self
            .frame
            .checked_add(1)
            .context("frame counter exhausted")?;
        header.session = self.id;
        header.frame = self.frame;
        ensure!(payload.len() as u64 == header.payload, "wrong request size");
        let basename = format!("{:016x}-{:016x}", self.id, self.frame);
        let pending = self.directory.join(format!("{basename}.pending"));
        let request = self.directory.join(format!("{basename}.request"));
        let response = self.directory.join(format!("{basename}.response"));
        let cleanup = FrameFiles([pending.clone(), request.clone(), response.clone()]);
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)?;
        header.write(&mut f)?;
        f.write_all(payload)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&pending, &request)?;
        diagnostics.requests += 1;
        let start = Instant::now();
        loop {
            match fs::symlink_metadata(&response) {
                Ok(m) => {
                    ensure!(
                        m.is_file() && !m.file_type().is_symlink(),
                        "response must be a regular file"
                    );
                    ensure!(
                        m.len() <= HEADER_SIZE as u64 + header.pixels()? as u64 * 4,
                        "oversized response"
                    );
                    let mut f = fs::File::open(&response)?;
                    let out = Header::read(&mut f)?;
                    diagnostics.last_stages = out.stages;
                    diagnostics.last_result = out.result;
                    out.validate_response(&header)?;
                    let mut bytes = vec![0; out.payload as usize];
                    f.read_exact(&mut bytes)?;
                    let mut extra = [0];
                    ensure!(f.read(&mut extra)? == 0, "trailing response bytes");
                    diagnostics.evaluations += 1;
                    drop(cleanup);
                    return Ok(bytes);
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
            ensure!(
                start.elapsed() < self.timeout,
                "neural host timeout; original frame retained"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
struct FrameFiles([PathBuf; 3]);
impl Drop for FrameFiles {
    fn drop(&mut self) {
        for f in &self.0 {
            let _ = fs::remove_file(f);
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.directory.join("client.lock"));
    }
}

#[derive(Default)]
pub(crate) struct ExternalAdapter {
    session: Option<Session>,
    capture: Option<capture::Capture>,
    resources: Option<resources::Resources>,
    diagnostics: Diagnostics,
    environment_checked: bool,
    generation: u32,
}
impl ExternalAdapter {
    pub(crate) fn enabled(&self) -> bool {
        self.session.is_some() || self.capture.is_some()
    }
    pub(crate) fn fail(&mut self, error: impl std::fmt::Display) {
        self.diagnostics.last_error = Some(error.to_string());
        self.diagnostics.state = State::EvaluationFailed;
        log::warn!("neural external disabled: {error}");
        self.session = None;
        self.capture = None;
    }
    fn environment(&mut self) {
        if self.environment_checked {
            return;
        }
        self.environment_checked = true;
        if let Some(path) = std::env::var_os("OPENOMSI_NEURAL_CAPTURE_DIR") {
            let count = std::env::var("OPENOMSI_NEURAL_CAPTURE_FRAMES")
                .unwrap_or_else(|_| "2".into())
                .parse::<u32>();
            match count
                .map_err(anyhow::Error::from)
                .and_then(|count| capture::Capture::new(Path::new(&path), count))
            {
                Ok(capture) => {
                    self.capture = Some(capture);
                    self.diagnostics.state = State::Capturing;
                }
                Err(error) => self.fail(error),
            }
            return;
        }
        let Some(path) = std::env::var_os("OPENOMSI_NEURAL_SESSION") else {
            return;
        };
        if std::env::var("OPENOMSI_NEURAL_CONSENT").as_deref() != Ok("1") {
            self.fail(
                "set OPENOMSI_NEURAL_CONSENT=1 only for a trusted, independently started host",
            );
            return;
        }
        if !cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            self.diagnostics.state = State::UnsupportedOperatingSystem;
            self.diagnostics.last_error = Some(
                "the Streamline host requires Windows x64; native Linux NR is unavailable".into(),
            );
            log::warn!("{}", self.diagnostics.last_error.as_ref().unwrap());
            return;
        }
        let timeout = std::env::var("OPENOMSI_NEURAL_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(5000);
        match Session::connect(Path::new(&path), Duration::from_millis(timeout)) {
            Ok(session) => {
                self.session = Some(session);
                self.diagnostics.state = State::ReadyForTransport;
                log::info!(
                    "neural external: trusted host attached; staging prototype, no evaluation yet"
                );
            }
            Err(e) => {
                self.fail(e);
                self.diagnostics.state = State::BackendUnavailable;
            }
        }
    }
}

impl crate::Renderer {
    /// Export consecutive real renderer inputs, without loading or simulating NR.
    /// The directory must not exist. Available on Linux and Windows with this feature.
    pub fn request_neural_capture(&mut self, directory: &Path, frames: u32) -> Result<()> {
        ensure!(
            !self.neural_external.enabled(),
            "stop the existing capture/host first"
        );
        self.neural_external.capture = Some(capture::Capture::new(directory, frames)?);
        self.neural_external.environment_checked = true;
        self.neural_external.diagnostics.state = State::Capturing;
        Ok(())
    }
    pub fn external_neural_diagnostics(&self) -> &Diagnostics {
        &self.neural_external.diagnostics
    }
    pub fn disable_external_neural(&mut self) {
        self.neural_external.session = None;
        self.neural_external.capture = None;
        self.neural_external.resources = None;
        self.neural_external.environment_checked = true;
        self.neural_external.diagnostics.state = State::Disabled;
    }
    pub(crate) fn external_neural_requested(&self) -> bool {
        self.neural_external.enabled()
    }
}

#[cfg(test)]
mod tests;
