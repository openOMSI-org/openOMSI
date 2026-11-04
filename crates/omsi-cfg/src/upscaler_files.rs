//! The vendors' DLLs temporal upscaling needs beside the game (`omsi_render::temporal`), and
//! whether this machine can use them. They are not part of the game's source: without them the
//! choice is not offered, as a setting that can only ever say "unavailable" helps nobody. In this
//! crate because the game and the launcher, which know nothing of each other, both ask.
//!
//! DLSS is offered only where Windows lists an NVIDIA GeForce RTX (or RTX workstation) card: on
//! anything else NVIDIA's runtime refuses it anyway. FSR 3 runs on any card, on DirectX 12 or
//! Vulkan, each with AMD's DLLs for it.

use std::path::{Path, PathBuf};

/// Streamline and its DLSS plugin: the same four for DirectX 12 and Vulkan.
const DLSS: [&str; 4] = ["sl.interposer.dll", "sl.common.dll", "sl.dlss.dll", "nvngx_dlss.dll"];
const FSR3_DX12: [&str; 2] = ["amd_fidelityfx_loader_dx12.dll", "amd_fidelityfx_upscaler_dx12.dll"];
const FSR3_VULKAN: [&str; 2] = ["amd_fidelityfx_loader_vk.dll", "amd_fidelityfx_upscaler_vk.dll"];

/// The graphics interface an upscaler is to run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Api {
    Dx12,
    Vulkan,
}

/// The folder of the running program.
pub fn program_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(|| ".".into())
}

/// The DLLs an upscaler (`dlss`, `fsr3`) needs on `api`.
pub fn files(upscaler: &str, api: Api) -> &'static [&'static str] {
    match (upscaler, api) {
        ("dlss", _) => &DLSS,
        ("fsr3", Api::Dx12) => &FSR3_DX12,
        ("fsr3", Api::Vulkan) => &FSR3_VULKAN,
        _ => &[],
    }
}

/// Which of the DLLs an upscaler needs on `api` are not in `dir`.
pub fn missing(dir: &Path, upscaler: &str, api: Api) -> Vec<&'static str> {
    files(upscaler, api).iter().copied().filter(|f| !dir.join(f).is_file()).collect()
}

/// Whether the upscaler's DLLs for one interface or the other are all in `dir`.
fn present(dir: &Path, upscaler: &str) -> bool {
    [Api::Dx12, Api::Vulkan].into_iter().any(|api| !files(upscaler, api).is_empty() && missing(dir, upscaler, api).is_empty())
}

/// An adapter DLSS may run on: NVIDIA's (PCI vendor 0x10de) with RTX in its name. A candidate,
/// not a promise: NVIDIA's runtime still checks the card and its driver when it starts.
pub fn rtx_candidate(vendor: u32, name: &str) -> bool {
    vendor == 0x10de && name.to_ascii_uppercase().contains("RTX")
}

/// Whether Windows lists an adapter DLSS may run on (`rtx_candidate`). Asked once a run.
pub fn rtx_present() -> bool {
    static RTX: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *RTX.get_or_init(|| adapters().iter().any(|(vendor, name)| rtx_candidate(*vendor, name)))
}

/// The adapters Windows lists (vendor id, name).
#[cfg(windows)]
fn adapters() -> Vec<(u32, String)> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for index in 0..64 {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(index) }) else { break };
        let Ok(info) = (unsafe { adapter.GetDesc1() }) else { continue };
        let end = info.Description.iter().position(|&c| c == 0).unwrap_or(info.Description.len());
        out.push((info.VendorId, String::from_utf16_lossy(&info.Description[..end])));
    }
    out
}

/// (no NVIDIA runtime outside Windows)
#[cfg(not(windows))]
fn adapters() -> Vec<(u32, String)> {
    Vec::new()
}

/// The upscalers that can run from `dir` on this machine, as the `upscaler=` setting names them:
/// Windows only. FSR 3 with AMD's DLLs for DirectX 12 or Vulkan; DLSS with NVIDIA's and an RTX
/// card.
pub fn available(dir: &Path) -> Vec<&'static str> {
    offered(dir, cfg!(windows), rtx_present())
}

/// `available` for a machine that is Windows or not and has an RTX card or not.
fn offered(dir: &Path, windows: bool, rtx: bool) -> Vec<&'static str> {
    ["dlss", "fsr3"].into_iter().filter(|u| windows && present(dir, u) && (*u != "dlss" || rtx)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("omsi-upscaler-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_upscaler_needs_all_of_its_files() {
        let dir = scratch("all");
        assert_eq!(missing(&dir, "fsr3", Api::Dx12), FSR3_DX12);
        assert!(offered(&dir, true, true).is_empty());
        std::fs::write(dir.join(FSR3_DX12[0]), b"").unwrap();
        assert_eq!(missing(&dir, "fsr3", Api::Dx12), [FSR3_DX12[1]]);
        assert!(offered(&dir, true, true).is_empty());
        std::fs::write(dir.join(FSR3_DX12[1]), b"").unwrap();
        assert!(missing(&dir, "fsr3", Api::Dx12).is_empty());
        assert_eq!(missing(&dir, "fsr3", Api::Vulkan), FSR3_VULKAN);
        assert_eq!(offered(&dir, true, false), ["fsr3"]);
        assert!(offered(&dir, false, true).is_empty(), "Windows only");
        assert_eq!(missing(&dir, "dlss", Api::Vulkan), DLSS);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// FSR 3 is offered with AMD's Vulkan DLLs alone too.
    #[test]
    fn fsr3_on_vulkan_alone() {
        let dir = scratch("vk");
        for f in FSR3_VULKAN {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        assert_eq!(offered(&dir, true, false), ["fsr3"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DLSS only with an RTX card, FSR 3 on any.
    #[test]
    fn dlss_only_with_an_rtx_card() {
        let dir = scratch("rtx");
        for f in DLSS.iter().chain(&FSR3_DX12) {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        assert_eq!(offered(&dir, true, true), ["dlss", "fsr3"]);
        assert_eq!(offered(&dir, true, false), ["fsr3"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rtx_name_is_not_enough_without_nvidias_vendor() {
        assert!(rtx_candidate(0x10de, "NVIDIA GeForce RTX 4060"));
        assert!(rtx_candidate(0x10de, "NVIDIA GeForce RTX 2080 Ti"));
        assert!(rtx_candidate(0x10de, "NVIDIA RTX A4000"));
        assert!(!rtx_candidate(0x10de, "NVIDIA GeForce GTX 1080"));
        assert!(!rtx_candidate(0x10de, "NVIDIA"));
        assert!(!rtx_candidate(0x1002, "NVIDIA GeForce RTX 4060"));
        assert!(!rtx_candidate(0x8086, "Intel Arc A770"));
        assert!(!rtx_candidate(0, "Microsoft Basic Render Driver"));
    }
}
