//! The vendors' DLLs temporal upscaling needs beside the game (`omsi_render::temporal`), and
//! whether this machine can use them. They are not part of the game's source: without them the
//! choice is not offered, as a setting that can only ever say "unavailable" helps nobody. On a
//! Mac the upscaler is Apple's MetalFX, which is part of the system and needs no file here. In
//! this crate because the game and the launcher, which know nothing of each other, both ask.

use std::path::{Path, PathBuf};

const DLSS: [&str; 4] = ["sl.interposer.dll", "sl.common.dll", "sl.dlss.dll", "nvngx_dlss.dll"];
const FSR3_DX12: [&str; 2] = ["amd_fidelityfx_loader_dx12.dll", "amd_fidelityfx_upscaler_dx12.dll"];
const FSR3_VULKAN: [&str; 2] = ["amd_fidelityfx_loader_vk.dll", "amd_fidelityfx_upscaler_vk.dll"];
/// Where macOS 13 and newer have MetalFX (the folder is there, its code is in the system's cache).
const METALFX: &str = "/System/Library/Frameworks/MetalFX.framework";
/// Where macOS tells its version.
const SYSTEM_VERSION: &str = "/System/Library/CoreServices/SystemVersion.plist";

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

/// Apple's MetalFX temporal scaler: a Mac with Apple silicon on macOS 13 or newer. A candidate,
/// as an RTX card is for DLSS: the scaler itself is asked when the game starts it.
pub fn metalfx_present() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64")) && Path::new(METALFX).is_dir()
}

pub fn available(dir: &Path) -> Vec<&'static str> {
    offered(dir, cfg!(windows), rtx_present(), metalfx_present())
}

fn offered(dir: &Path, windows: bool, rtx: bool, metalfx: bool) -> Vec<&'static str> {
    let files = ["dlss", "fsr3"].into_iter().filter(|u| windows && present(dir, u) && (*u != "dlss" || rtx));
    files.chain(metalfx.then_some("metalfx")).collect()
}

/// The upscalers that need no file beside the game: what a launcher that has not found the
/// game yet can still offer.
pub fn of_the_system() -> Vec<&'static str> {
    metalfx_present().then_some("metalfx").into_iter().collect()
}

/// An upscaler's name as a menu shows it.
pub fn label(upscaler: &str) -> &'static str {
    match upscaler {
        "dlss" => "NVIDIA DLSS (experimental)",
        "metalfx" => "Apple MetalFX (experimental)",
        _ => "AMD FSR 3 (experimental)",
    }
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

/// The Upscaling row's help text, as the in-game menu shows it: what the temporal upscalers this
/// machine has do. A Mac's only one is Apple's own, and Metal its only graphics interface.
pub fn menu_help(offered: bool) -> &'static str {
    if !offered {
        "Standard Catmull-Rom or AMD FSR 1; changes show at once"
    } else if cfg!(target_os = "macos") {
        "Standard Catmull-Rom or AMD FSR 1 change at once; Apple MetalFX takes the place of the anti-aliasing and the render scale, from the next start"
    } else {
        "Standard Catmull-Rom or AMD FSR 1 change at once; NVIDIA DLSS or AMD FSR 3 take the place of the anti-aliasing and the render scale, on DirectX 12 or Vulkan, from the next start"
    }
}

/// The `frame_generation` setting's word: `metalfx` (Apple MetalFX); `fsr3` and `dlss2x` to
/// `dlss4x` too, the Windows words of #3763, so that a Mac build saving the settings file does
/// not turn a Windows choice into `off` (where the game cannot run one, it simply does not);
/// anything else `off`.
pub fn frame_generation_word(s: &str) -> &'static str {
    match s.trim().to_ascii_lowercase().as_str() {
        "metalfx" => "metalfx",
        "fsr3" => "fsr3",
        "dlss" | "dlss2x" => "dlss2x",
        "dlss3x" => "dlss3x",
        "dlss4x" => "dlss4x",
        _ => "off",
    }
}

/// macOS's major version, out of its `SystemVersion.plist`.
fn macos_major(plist: &str) -> Option<u32> {
    let version = plist.split("<key>ProductVersion</key>").nth(1)?.split("<string>").nth(1)?.split("</string>").next()?;
    version.trim().split('.').next()?.parse().ok()
}

/// Apple's MetalFX frame interpolator: MetalFX on macOS 26 or newer. A candidate, as
/// `metalfx_present` is: the interpolator itself is asked when the game starts it.
pub fn metalfx_frames_present() -> bool {
    static PRESENT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PRESENT.get_or_init(|| metalfx_present() && std::fs::read_to_string(SYSTEM_VERSION).ok().and_then(|p| macos_major(&p)).is_some_and(|v| v >= 26))
}

/// The frame generation this machine can run, as the menus offer it: `metalfx` on a Mac.
pub fn frame_generation_choices() -> Vec<&'static str> {
    generators(metalfx_frames_present())
}

fn generators(metalfx: bool) -> Vec<&'static str> {
    metalfx.then_some("metalfx").into_iter().collect()
}

/// The frame generation choices of a menu, from `frame_generation_choices`.
pub fn frame_generation_menu(choices: &[&str]) -> Vec<(&'static str, &'static str)> {
    let mut menu = vec![("off", "Off")];
    if choices.contains(&"metalfx") {
        menu.push(("metalfx", label("metalfx")));
    }
    menu
}

/// The Frame generation row's help text, as the in-game menu shows it.
pub fn frame_generation_help() -> &'static str {
    "Shows a generated frame between every two drawn ones: smoother motion for a little more input lag. Needs Apple MetalFX upscaling and works best from 30 drawn frames a second; with V-sync the game draws half the display's rate"
}

/// The upscalers that can run from the game's folder, as a menu lists them.
pub fn menu_choices() -> impl Iterator<Item = (&'static str, &'static str)> {
    available(&program_dir()).into_iter().map(|u| (u, label(u)))
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
        assert!(offered(&dir, true, true, false).is_empty());
        std::fs::write(dir.join(FSR3_DX12[0]), b"").unwrap();
        assert_eq!(missing(&dir, "fsr3", Api::Dx12), [FSR3_DX12[1]]);
        assert!(offered(&dir, true, true, false).is_empty());
        std::fs::write(dir.join(FSR3_DX12[1]), b"").unwrap();
        assert!(missing(&dir, "fsr3", Api::Dx12).is_empty());
        assert_eq!(missing(&dir, "fsr3", Api::Vulkan), FSR3_VULKAN);
        assert_eq!(offered(&dir, true, false, false), ["fsr3"]);
        assert!(offered(&dir, false, true, false).is_empty(), "Windows only");
        assert_eq!(missing(&dir, "dlss", Api::Vulkan), DLSS);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fsr3_on_vulkan_alone() {
        let dir = scratch("vk");
        for f in FSR3_VULKAN {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        assert_eq!(offered(&dir, true, false, false), ["fsr3"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dlss_only_with_an_rtx_card() {
        let dir = scratch("rtx");
        for f in DLSS.iter().chain(&FSR3_DX12) {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        assert_eq!(offered(&dir, true, true, false), ["dlss", "fsr3"]);
        assert_eq!(offered(&dir, true, false, false), ["fsr3"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MetalFX is part of macOS: it is offered without a file beside the game, and never on
    /// another system, whatever lies in the folder.
    #[test]
    fn metalfx_needs_no_files_and_only_a_mac_has_it() {
        let dir = scratch("metalfx");
        assert_eq!(offered(&dir, false, false, true), ["metalfx"]);
        assert!(offered(&dir, false, false, false).is_empty());
        for f in DLSS.iter().chain(&FSR3_DX12) {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        assert_eq!(offered(&dir, true, true, false), ["dlss", "fsr3"]);
        assert_eq!(offered(&dir, false, true, true), ["metalfx"], "the DLLs are of no use on a Mac");
        assert!(files("metalfx", Api::Dx12).is_empty() && files("metalfx", Api::Vulkan).is_empty());
        assert_eq!(metalfx_present(), cfg!(all(target_os = "macos", target_arch = "aarch64")) && Path::new(METALFX).is_dir());
        assert_eq!((label("dlss"), label("fsr3"), label("metalfx")), ("NVIDIA DLSS (experimental)", "AMD FSR 3 (experimental)", "Apple MetalFX (experimental)"));
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

    #[test]
    fn frame_generation_words_and_choices() {
        assert_eq!(frame_generation_word(" MetalFX "), "metalfx");
        for word in ["off", "", "on", "dlss8x"] {
            assert_eq!(frame_generation_word(word), "off");
        }
        // (the Windows words of #3763 are kept, not turned into `off` by a Mac build)
        assert_eq!([frame_generation_word("FSR3"), frame_generation_word("dlss"), frame_generation_word("dlss4x")], ["fsr3", "dlss2x", "dlss4x"]);
        assert_eq!(generators(true), ["metalfx"]);
        assert!(generators(false).is_empty());
        assert_eq!(frame_generation_menu(&[]), [("off", "Off")]);
        assert_eq!(frame_generation_menu(&["metalfx"]), [("off", "Off"), ("metalfx", "Apple MetalFX (experimental)")]);
        // (never where MetalFX itself is not: Windows, Linux, a Mac with an Intel chip)
        assert!(!metalfx_frames_present() || metalfx_present());
        assert_eq!(frame_generation_choices().is_empty(), !metalfx_frames_present());
    }

    #[test]
    fn frame_generation_needs_macos_26() {
        let plist = |v: &str| format!("<dict>\n\t<key>ProductName</key>\n\t<string>macOS</string>\n\t<key>ProductVersion</key>\n\t<string>{v}</string>\n</dict>");
        assert_eq!(macos_major(&plist("15.6.1")), Some(15));
        assert_eq!(macos_major(&plist("26.0")), Some(26));
        assert_eq!(macos_major(&plist("27.2.1")), Some(27));
        assert_eq!(macos_major("<dict/>"), None);
        assert_eq!(macos_major(&plist("")), None);
    }
}
