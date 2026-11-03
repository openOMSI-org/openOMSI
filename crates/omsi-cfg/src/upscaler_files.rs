//! The vendors' DLLs temporal upscaling needs beside the game (`omsi_render::temporal`). They are
//! not part of the game: without them the choice is not offered, as a setting that can only ever
//! say "unavailable" helps nobody. In this crate because the game and the launcher, which know
//! nothing of each other, both ask.

use std::path::{Path, PathBuf};

const DLSS: [&str; 4] = ["sl.interposer.dll", "sl.common.dll", "sl.dlss.dll", "nvngx_dlss.dll"];
const FSR3: [&str; 2] = ["amd_fidelityfx_loader_dx12.dll", "amd_fidelityfx_upscaler_dx12.dll"];

/// The folder of the running program.
pub fn program_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(|| ".".into())
}

/// Which DLLs of an upscaler (`dlss`, `fsr3`) are not in `dir`. All of them for anything else.
pub fn missing(dir: &Path, upscaler: &str) -> Vec<&'static str> {
    let files: &[&'static str] = match upscaler {
        "dlss" => &DLSS,
        "fsr3" => &FSR3,
        _ => return DLSS.iter().chain(&FSR3).copied().collect(),
    };
    files.iter().copied().filter(|f| !dir.join(f).is_file()).collect()
}

/// The upscalers that can run from `dir`, as the `upscaler=` setting names them: Windows only,
/// they work on DirectX 12.
pub fn available(dir: &Path) -> Vec<&'static str> {
    ["dlss", "fsr3"].into_iter().filter(|u| cfg!(windows) && missing(dir, u).is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_upscaler_needs_all_of_its_files() {
        let dir = std::env::temp_dir().join(format!("omsi-upscaler-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(missing(&dir, "fsr3"), FSR3);
        assert!(available(&dir).is_empty());
        std::fs::write(dir.join(FSR3[0]), b"").unwrap();
        assert_eq!(missing(&dir, "fsr3"), [FSR3[1]]);
        assert!(available(&dir).is_empty());
        std::fs::write(dir.join(FSR3[1]), b"").unwrap();
        assert!(missing(&dir, "fsr3").is_empty());
        assert_eq!(available(&dir), if cfg!(windows) { vec!["fsr3"] } else { vec![] });
        assert_eq!(missing(&dir, "dlss"), DLSS);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
