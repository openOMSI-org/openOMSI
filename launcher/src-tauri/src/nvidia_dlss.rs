//! Optional NVIDIA DLSS runtime installer for the desktop launcher.
//! The AGPLv3 application contains no NVIDIA binaries. The player chooses to
//! download the unmodified SDK directly from NVIDIA and accepts its terms.

use serde_json::{json, Value};
#[cfg(windows)]
use std::path::PathBuf;

const NVIDIA_FILES: [&str; 4] = [
    "sl.interposer.dll",
    "sl.common.dll",
    "sl.dlss.dll",
    "nvngx_dlss.dll",
];

#[cfg(windows)]
fn game_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Launcher executable: {e}"))?;
    let dir = exe
        .parent()
        .ok_or("Launcher has no installation directory")?;
    if !dir.join("openomsi.exe").is_file() {
        return Err("openomsi.exe must be in the same directory as the launcher".into());
    }
    Ok(dir.to_path_buf())
}

/// A presence check only: the signatures are checked when installing.
pub fn status() -> Value {
    #[cfg(windows)]
    {
        let game = game_dir();
        let missing: Vec<&str> = match &game {
            Ok(dir) => NVIDIA_FILES
                .iter()
                .copied()
                .filter(|name| !dir.join(name).is_file())
                .collect(),
            Err(_) => NVIDIA_FILES.to_vec(),
        };
        return json!({
            "supported": game.is_ok(),
            "installed": game.is_ok() && missing.is_empty(),
            "rtx": omsi_cfg::upscaler_files::rtx_present(),
            "missing": missing,
        });
    }
    #[cfg(not(windows))]
    {
        json!({ "supported": false, "installed": false, "rtx": false, "missing": NVIDIA_FILES })
    }
}

#[cfg(windows)]
static INSTALLING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(windows)]
struct InstallGuard;

#[cfg(windows)]
impl InstallGuard {
    fn new() -> Result<Self, String> {
        use std::sync::atomic::Ordering;
        INSTALLING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .map_err(|_| "An NVIDIA DLSS installation is already running".to_string())?;
        Ok(Self)
    }
}

#[cfg(windows)]
impl Drop for InstallGuard {
    fn drop(&mut self) {
        INSTALLING.store(false, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(windows)]
struct TemporaryScripts(PathBuf);

#[cfg(windows)]
impl Drop for TemporaryScripts {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A user-initiated download, not an auto-fetch on startup.
/// The UI must show NVIDIA's terms and require an explicit opt-in.
pub fn install(accepted_terms: bool) -> Result<Value, String> {
    if !accepted_terms {
        return Err("Read and accept the NVIDIA SDK license before installing".into());
    }
    #[cfg(not(windows))]
    {
        Err("NVIDIA DLSS installation is available only on Windows".into())
    }
    #[cfg(windows)]
    {
        let _guard = InstallGuard::new()?;
        let dir = game_dir()?;
        if !omsi_cfg::upscaler_files::rtx_present() {
            return Err("NVIDIA DLSS requires an NVIDIA RTX GPU".into());
        }

        // (embedded scripts: the zip's SHA-256, each DLL's signature, a fixed list of files)
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let temp =
            std::env::temp_dir().join(format!("openomsi-dlss-{}-{unique}", std::process::id()));
        std::fs::create_dir(&temp).map_err(|e| format!("Create installer workspace: {e}"))?;
        let _cleanup = TemporaryScripts(temp.clone());
        let package = temp.join("package-streamline.ps1");
        let installer = temp.join("install-streamline.ps1");
        std::fs::write(
            &package,
            include_str!("../../../scripts/windows/package-streamline.ps1"),
        )
        .map_err(|e| format!("Prepare NVIDIA verifier: {e}"))?;
        std::fs::write(
            &installer,
            include_str!("../../../scripts/windows/install-streamline.ps1"),
        )
        .map_err(|e| format!("Prepare NVIDIA installer: {e}"))?;

        // (CREATE_NO_WINDOW: the launcher's button shows the progress, no console over it)
        use std::os::windows::process::CommandExt;
        let output = std::process::Command::new(
            // (by its full path: Windows looks in the launcher's folder first for a bare name)
            PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
                .join(r"System32\WindowsPowerShell\v1.0\powershell.exe"),
        )
        .creation_flags(0x0800_0000)
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&installer)
        .arg("-GameDirectory")
        .arg(&dir)
        .arg("-PackagingScript")
        .arg(&package)
        .output()
        .map_err(|e| format!("Launch the NVIDIA SDK installer: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let detail = if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            };
            return Err(format!(
                "NVIDIA DLSS installation failed: {}",
                detail.chars().take(1200).collect::<String>()
            ));
        }
        if NVIDIA_FILES.iter().any(|name| !dir.join(name).is_file()) {
            return Err("NVIDIA installer completed without all four required files".into());
        }
        Ok(status())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_allowlist_is_exactly_dlss_super_resolution() {
        assert_eq!(
            NVIDIA_FILES,
            [
                "sl.interposer.dll",
                "sl.common.dll",
                "sl.dlss.dll",
                "nvngx_dlss.dll"
            ]
        );
    }
}
