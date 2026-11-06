//! The vendors' DLLs temporal upscaling needs beside the game (`omsi_render::temporal`), and
//! whether this machine can use them. They are not part of the game's source: without them the
//! choice is not offered, as a setting that can only ever say "unavailable" helps nobody. In this
//! crate because the game and the launcher, which know nothing of each other, both ask.

use std::path::{Path, PathBuf};

const DLSS: [&str; 4] = ["sl.interposer.dll", "sl.common.dll", "sl.dlss.dll", "nvngx_dlss.dll"];
const FSR3_DX12: [&str; 2] = ["amd_fidelityfx_loader_dx12.dll", "amd_fidelityfx_upscaler_dx12.dll"];
const FSR3_VULKAN: [&str; 2] = ["amd_fidelityfx_loader_vk.dll", "amd_fidelityfx_upscaler_vk.dll"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Api {
    Dx12,
    Vulkan,
}

pub fn program_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(|| ".".into())
}

pub fn files(upscaler: &str, api: Api) -> &'static [&'static str] {
    match (upscaler, api) {
        ("dlss", _) => &DLSS,
        ("fsr3", Api::Dx12) => &FSR3_DX12,
        ("fsr3", Api::Vulkan) => &FSR3_VULKAN,
        _ => &[],
    }
}

pub fn missing(dir: &Path, upscaler: &str, api: Api) -> Vec<&'static str> {
    files(upscaler, api).iter().copied().filter(|f| !dir.join(f).is_file()).collect()
}

fn present(dir: &Path, upscaler: &str) -> bool {
    [Api::Dx12, Api::Vulkan].into_iter().any(|api| !files(upscaler, api).is_empty() && missing(dir, upscaler, api).is_empty())
}

/// An adapter DLSS may run on: NVIDIA's (PCI vendor 0x10de) with RTX in its name. A candidate,
/// not a promise: NVIDIA's runtime still checks the card and its driver when it starts.
pub fn rtx_candidate(vendor: u32, name: &str) -> bool {
    vendor == 0x10de && name.to_ascii_uppercase().contains("RTX")
}

pub fn rtx_present() -> bool {
    static RTX: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *RTX.get_or_init(|| adapters().iter().any(|(vendor, name)| rtx_candidate(*vendor, name)))
}

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

#[cfg(not(windows))]
fn adapters() -> Vec<(u32, String)> {
    Vec::new()
}

pub fn available(dir: &Path) -> Vec<&'static str> {
    offered(dir, cfg!(windows), rtx_present())
}

fn offered(dir: &Path, windows: bool, rtx: bool) -> Vec<&'static str> {
    ["dlss", "fsr3"].into_iter().filter(|u| windows && present(dir, u) && (*u != "dlss" || rtx)).collect()
}

pub fn quality_word(s: &str) -> &'static str {
    match s.trim().to_ascii_lowercase().as_str() {
        "native" => "native",
        "balanced" => "balanced",
        "performance" => "performance",
        "ultra_performance" => "ultra_performance",
        _ => "quality",
    }
}

/// The graphics interface to ask for first with DLSS or FSR 3: Vulkan where that is the choice
/// and the upscaler's DLLs for it are in `dir`, else DirectX 12.
pub fn first_api(chosen: &str, upscaler: &str, dir: &Path) -> &'static str {
    let vulkan = missing(dir, upscaler, Api::Vulkan).is_empty();
    if vulkan && (chosen.trim().eq_ignore_ascii_case("vulkan") || !missing(dir, upscaler, Api::Dx12).is_empty()) {
        "vulkan"
    } else {
        "dx12"
    }
}

/// The upscalers that can run from the game's folder, as a menu lists them.
pub fn menu_choices() -> impl Iterator<Item = (&'static str, &'static str)> {
    available(&program_dir()).into_iter().map(|u| if u == "dlss" { ("dlss", "NVIDIA DLSS (experimental)") } else { ("fsr3", "AMD FSR 3 (experimental)") })
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_upscaler_keeps_vulkan_with_its_dlls() {
        let dir = std::env::temp_dir().join(format!("omsi-upscaling-api-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(first_api("vulkan", "fsr3", &dir), "dx12");
        for f in files("fsr3", Api::Vulkan) {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        assert_eq!(first_api("Vulkan", "fsr3", &dir), "vulkan");
        // (only the Vulkan DLLs here: DirectX 12 could not run it)
        assert_eq!(first_api("auto", "fsr3", &dir), "vulkan");
        for f in files("fsr3", Api::Dx12) {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        assert_eq!(first_api("auto", "fsr3", &dir), "dx12");
        assert_eq!(first_api("vulkan", "dlss", &dir), "dx12");
        let _ = std::fs::remove_dir_all(&dir);
    }

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

    #[test]
    fn fsr3_on_vulkan_alone() {
        let dir = scratch("vk");
        for f in FSR3_VULKAN {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        assert_eq!(offered(&dir, true, false), ["fsr3"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

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
