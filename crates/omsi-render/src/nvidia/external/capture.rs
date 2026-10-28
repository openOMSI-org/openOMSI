//! Persist the existing input ABI, including its temporal sequence. No neural output.
use super::*;

pub(super) struct Capture {
    directory: PathBuf,
    id: u64,
    frame: u32,
    count: u32,
}
impl Capture {
    pub(super) fn new(directory: &Path, count: u32) -> Result<Self> {
        ensure!((1..=16).contains(&count), "capture count must be 1..16");
        // create_dir is exclusive: never overwrite an earlier capture or follow its symlink.
        fs::create_dir(directory)
            .context("capture requires a new directory with an existing parent")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self {
            directory: directory.canonicalize()?,
            id: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos() as u64
                ^ u64::from(std::process::id()),
            frame: 0,
            count,
        })
    }
    pub(super) fn write(&mut self, mut header: Header, payload: &[u8]) -> Result<bool> {
        ensure!(self.frame < self.count, "capture already complete");
        ensure!(
            payload.len() as u64 == header.payload,
            "capture payload size mismatch"
        );
        self.frame += 1;
        header.session = self.id;
        header.frame = u64::from(self.frame);
        // A replay starts a new model history, even if capture began mid-game.
        if self.frame == 1 {
            header.flags |= 2;
        }
        let pending = self.directory.join(format!("{:08}.pending", self.frame));
        let final_path = pending.with_extension("omsi-nr");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)?;
        header.write(&mut file)?;
        file.write_all(payload)?;
        file.sync_all()?;
        drop(file);
        ensure!(!final_path.exists(), "capture destination already exists");
        fs::rename(pending, final_path)?;
        if self.frame == self.count {
            let files = (1..=self.count)
                .map(|i| format!("\"{i:08}.omsi-nr\""))
                .collect::<Vec<_>>()
                .join(",");
            let manifest = format!(
                concat!(
                    "{{\"protocol\":\"openOMSI NR v1\",\"source\":\"renderer GPU readback before HUD\",",
                    "\"color\":\"RGBA8 encoded sRGB LDR after tone mapping\",",
                    "\"motion\":\"RG32F previous-current pixels Y-down\",",
                    "\"depth\":\"R32F reverse Z [0,1]\",\"validity\":\"R8 engine mask, not NVIDIA ControlMask\",",
                    "\"jitter\":[0,0],\"neural_evaluation\":false,\"frames\":[{}]}}\n"
                ),
                files
            );
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.directory.join("capture.json"))?;
            file.write_all(manifest.as_bytes())?;
            file.sync_all()?;
            log::info!(
                "neural input capture complete: {} ({} frames; no inference)",
                self.directory.display(),
                self.count
            );
        }
        Ok(self.frame == self.count)
    }
}
