//! The device as temporal upscaling needs it (`crate::temporal`): which upscaler this adapter
//! can run, and on Vulkan, DLSS's device extensions added to mX's own
//! (`mx::DeviceDescriptor::vulkan_extensions`; mX turns on descriptor indexing and buffer device
//! addresses, the Vulkan 1.2 features DLSS needs, wherever the device has them).

use crate::temporal::{Temporal, TemporalUpscaler};
use std::ffi::CStr;

pub(super) fn usable(asked: Option<Temporal>, info: &mx::AdapterInfo) -> Option<Temporal> {
    let t = asked?;
    let windows = cfg!(windows) && matches!(info.backend, mx::Backend::Dx12 | mx::Backend::Vulkan);
    let (can, needed) = match t.upscaler {
        TemporalUpscaler::Dlss => (
            windows && omsi_cfg::upscaler_files::rtx_candidate(info.vendor, &info.name),
            "DirectX 12 or Vulkan on Windows and an NVIDIA RTX card are needed",
        ),
        TemporalUpscaler::Fsr3 => (windows, "DirectX 12 or Vulkan on Windows is needed"),
        TemporalUpscaler::Xess => (windows, "DirectX 12 or Vulkan on Windows is needed"),
        TemporalUpscaler::MetalFx => (
            cfg!(target_os = "macos") && info.backend == mx::Backend::Metal,
            "Metal on a Mac is needed",
        ),
    };
    if can {
        return Some(t);
    }
    log::warn!(
        "{} upscaling asked for, but {} on {:?} cannot do it ({needed}); drawing without it",
        t.upscaler.as_str(),
        info.name,
        info.backend
    );
    None
}

pub(super) fn dlss_vulkan(
    temporal: Option<Temporal>,
    adapter: &mx::Adapter,
    info: &mx::AdapterInfo,
) -> (Option<Temporal>, Option<Vec<&'static CStr>>) {
    let Some(t) = temporal
        .filter(|t| t.upscaler == TemporalUpscaler::Dlss && info.backend == mx::Backend::Vulkan)
    else {
        return (temporal, None);
    };
    let wanted = match crate::temporal::dlss_vulkan_extensions() {
        Ok(e) => e,
        Err(e) => {
            log::warn!("dlss upscaling is off on Vulkan: {e:#}");
            return (None, None);
        }
    };
    let missing: Vec<String> = match adapter_extensions(adapter) {
        Some(have) => wanted
            .iter()
            .filter(|w| !have.iter().any(|h| h.as_c_str() == **w))
            .map(|w| w.to_string_lossy().into_owned())
            .collect(),
        None => vec!["(the adapter's extension list)".into()],
    };
    if !missing.is_empty() {
        crate::temporal::dlss_vulkan_abandon();
        log::warn!(
            "dlss upscaling is off: {} lacks the Vulkan extensions DLSS needs ({}); a newer driver, or DirectX 12, may have them",
            info.name,
            missing.join(", ")
        );
        return (None, None);
    }
    log::info!(
        "DLSS on Vulkan: the device is made with {}",
        wanted
            .iter()
            .map(|w| w.to_string_lossy())
            .collect::<Vec<_>>()
            .join(", ")
    );
    (Some(t), Some(wanted))
}

fn adapter_extensions(adapter: &mx::Adapter) -> Option<Vec<std::ffi::CString>> {
    Some(adapter.as_vulkan()?.extensions())
}
