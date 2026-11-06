//! The device as temporal upscaling needs it (`crate::temporal`): which upscaler this adapter
//! can run, and on Vulkan, DLSS's device extensions added to wgpu's own.

use crate::temporal::{Temporal, TemporalUpscaler};
use std::ffi::CStr;

pub(super) fn usable(asked: Option<Temporal>, info: &wgpu::AdapterInfo) -> Option<Temporal> {
    let t = asked?;
    let api = cfg!(windows) && matches!(info.backend, wgpu::Backend::Dx12 | wgpu::Backend::Vulkan);
    let card = t.upscaler == TemporalUpscaler::Fsr3 || omsi_cfg::upscaler_files::rtx_candidate(info.vendor, &info.name);
    if api && card {
        return Some(t);
    }
    log::warn!(
        "{} upscaling asked for, but {} on {:?} cannot do it (DirectX 12 or Vulkan is needed, and for DLSS an NVIDIA RTX card); drawing without it",
        t.upscaler.as_str(),
        info.name,
        info.backend
    );
    None
}

pub(super) fn dlss_vulkan(temporal: Option<Temporal>, adapter: &wgpu::Adapter, info: &wgpu::AdapterInfo) -> (Option<Temporal>, Option<Vec<&'static CStr>>) {
    let Some(t) = temporal.filter(|t| t.upscaler == TemporalUpscaler::Dlss && info.backend == wgpu::Backend::Vulkan) else {
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
        Some(have) => wanted.iter().filter(|w| !have.iter().any(|h| h.as_c_str() == **w)).map(|w| w.to_string_lossy().into_owned()).collect(),
        None => vec!["(the adapter's extension list)".into()],
    };
    if !missing.is_empty() {
        crate::temporal::dlss_vulkan_abandon();
        log::warn!("dlss upscaling is off: {} lacks the Vulkan extensions DLSS needs ({}); a newer driver, or DirectX 12, may have them", info.name, missing.join(", "));
        return (None, None);
    }
    log::info!("DLSS on Vulkan: the device is made with {}", wanted.iter().map(|w| w.to_string_lossy()).collect::<Vec<_>>().join(", "));
    (Some(t), Some(wanted))
}

#[cfg(windows)]
fn adapter_extensions(adapter: &wgpu::Adapter) -> Option<Vec<std::ffi::CString>> {
    let hal = unsafe { adapter.as_hal::<wgpu_hal::api::Vulkan>() }?;
    let instance = hal.shared_instance().raw_instance();
    let props = unsafe { instance.enumerate_device_extension_properties(hal.raw_physical_device()) }.ok()?;
    Some(props.iter().filter_map(|p| p.extension_name_as_c_str().ok().map(CStr::to_owned)).collect())
}

#[cfg(not(windows))]
fn adapter_extensions(_adapter: &wgpu::Adapter) -> Option<Vec<std::ffi::CString>> {
    None
}

#[cfg(windows)]
pub(super) fn open_with(adapter: &wgpu::Adapter, desc: &wgpu::DeviceDescriptor<'_>, extensions: Vec<&'static CStr>) -> anyhow::Result<(wgpu::Device, wgpu::Queue)> {
    use anyhow::{anyhow, Context};
    let hal = unsafe { adapter.as_hal::<wgpu_hal::api::Vulkan>() }.ok_or_else(|| anyhow!("not a Vulkan adapter"))?;
    // (DLSS's Vulkan 1.2 features, `streamline::VULKAN_FEATURES`: timeline semaphores wgpu
    // turns on itself; descriptor indexing and buffer device addresses chained here, as far as
    // the adapter has them, unless wgpu chains its own for features of its - made once a run)
    use ash::vk;
    let instance = hal.shared_instance().raw_instance();
    let mut indexing = vk::PhysicalDeviceDescriptorIndexingFeatures::default();
    let mut addresses = vk::PhysicalDeviceBufferDeviceAddressFeatures::default();
    {
        let mut all = vk::PhysicalDeviceFeatures2::default().push_next(&mut indexing).push_next(&mut addresses);
        unsafe { instance.get_physical_device_features2(hal.raw_physical_device(), &mut all) };
    }
    let wgpu_indexing = desc.required_features.intersects(wgpu::Features::TEXTURE_BINDING_ARRAY | wgpu::Features::BUFFER_BINDING_ARRAY | wgpu::Features::STORAGE_RESOURCE_BINDING_ARRAY | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING | wgpu::Features::STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING | wgpu::Features::UNIFORM_BUFFER_BINDING_ARRAYS | wgpu::Features::PARTIALLY_BOUND_BINDING_ARRAY);
    let wgpu_addresses = desc.required_features.contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY);
    let indexing: Option<&'static mut vk::PhysicalDeviceDescriptorIndexingFeatures<'static>> = (!wgpu_indexing).then(|| {
        indexing.p_next = std::ptr::null_mut();
        Box::leak(Box::new(indexing))
    });
    let addresses: Option<&'static mut vk::PhysicalDeviceBufferDeviceAddressFeatures<'static>> = (!wgpu_addresses && addresses.buffer_device_address == vk::TRUE).then(|| {
        Box::leak(Box::new(vk::PhysicalDeviceBufferDeviceAddressFeatures::default().buffer_device_address(true)))
    });
    let add = move |args: wgpu_hal::vulkan::CreateDeviceCallbackArgs<'_, '_, '_>| {
        for e in &extensions {
            if !args.extensions.contains(e) {
                args.extensions.push(e);
            }
        }
        if let Some(f) = indexing {
            *args.create_info = std::mem::take(args.create_info).push_next(f);
        }
        if let Some(f) = addresses {
            *args.create_info = std::mem::take(args.create_info).push_next(f);
        }
    };
    let open = unsafe { hal.open_with_callback(desc.required_features, &desc.required_limits, &desc.memory_hints, Some(Box::new(add))) }.map_err(|e| anyhow!("{e}")).context("opening the Vulkan device with DLSS's extensions")?;
    drop(hal);
    unsafe { adapter.create_device_from_hal(open, desc) }.context("request_device (Vulkan, with DLSS's extensions)")
}

#[cfg(not(windows))]
pub(super) fn open_with(_adapter: &wgpu::Adapter, _desc: &wgpu::DeviceDescriptor<'_>, _extensions: Vec<&'static CStr>) -> anyhow::Result<(wgpu::Device, wgpu::Queue)> {
    anyhow::bail!("DLSS is Windows-only")
}
