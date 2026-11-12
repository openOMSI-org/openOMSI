// The Streamline structures, constants and identifiers below are transcribed from NVIDIA's
// Streamline SDK headers (https://github.com/NVIDIA-RTX/Streamline), which come under this
// licence:
//
// Copyright (c) 2022-2024 NVIDIA CORPORATION. All rights reserved
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! NVIDIA DLSS (Super Resolution, and DLAA at full size) by way of NVIDIA Streamline: Windows,
//! DirectX 12 or Vulkan, a GeForce RTX card.
//!
//! DLSS records into a command list of ours on wgpu's queue, between the scene's submit and the
//! HUD's: the queue's order is the only synchronisation. Textures go over in wgpu's last state.

use super::d3d12::{game_dir, wide, ListRing};
use super::vulkan::{VkGpu, VkRing};
use ash::vk;
use super::{fallback_render_size, FrameConstants, Inputs, TemporalQuality};
use anyhow::{anyhow, bail, Context, Result};
use std::ffi::{c_char, c_void, CStr};
use windows::core::{Interface, PCSTR, PCWSTR};
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::{IDXGIDeviceSubObject_Impl, IDXGIObject_Impl, IDXGIOutput, IDXGISwapChain, IDXGISwapChain_Impl, DXGI_FRAME_STATISTICS, DXGI_PRESENT, DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_CHAIN_FLAG};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR};


#[repr(C)]
#[derive(Clone, Copy)]
struct StructType {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Base {
    next: *mut c_void,
    struct_type: StructType,
    struct_version: usize,
}

impl Base {
    const fn new(struct_type: StructType, struct_version: usize) -> Base {
        Base { next: std::ptr::null_mut(), struct_type, struct_version }
    }
}

const PREFERENCES: StructType = StructType { data1: 0x1ca10965, data2: 0xbf8e, data3: 0x432b, data4: [0x8d, 0xa1, 0x67, 0x16, 0xd8, 0x79, 0xfb, 0x14] };
const RESOURCE: StructType = StructType { data1: 0x3a9d70cf, data2: 0x2418, data3: 0x4b72, data4: [0x83, 0x91, 0x13, 0xf8, 0x72, 0x1c, 0x72, 0x61] };
const RESOURCE_TAG: StructType = StructType { data1: 0x4c6a5aad, data2: 0xb445, data3: 0x496c, data4: [0x87, 0xff, 0x1a, 0xf3, 0x84, 0x5b, 0xe6, 0x53] };
const VIEWPORT_HANDLE: StructType = StructType { data1: 0x171b6435, data2: 0x9b3c, data3: 0x4fc8, data4: [0x99, 0x94, 0xfb, 0xe5, 0x25, 0x69, 0xaa, 0xa4] };
const ADAPTER_INFO: StructType = StructType { data1: 0x0677315f, data2: 0xa746, data3: 0x4492, data4: [0x9f, 0x42, 0xcb, 0x61, 0x42, 0xc9, 0xc3, 0xd4] };
const CONSTANTS: StructType = StructType { data1: 0xdcd35ad7, data2: 0x4e4a, data3: 0x4bad, data4: [0xa9, 0x0c, 0xe0, 0xc4, 0x9e, 0xb2, 0x3a, 0xfe] };
const DLSS_OPTIONS: StructType = StructType { data1: 0x6ac826e4, data2: 0x4c61, data3: 0x4101, data4: [0xa9, 0x2d, 0x63, 0x8d, 0x42, 0x10, 0x57, 0xb8] };
const VULKAN_INFO: StructType = StructType { data1: 0x0eed6fd5, data2: 0x82cd, data3: 0x43a9, data4: [0xbd, 0xb5, 0x47, 0xa5, 0xba, 0x2f, 0x45, 0xd6] };
const FEATURE_REQUIREMENTS: StructType = StructType { data1: 0x66714097, data2: 0xac6d, data3: 0x4bc6, data4: [0x89, 0x15, 0x1e, 0x0f, 0x55, 0xa6, 0xb6, 0x1f] };
const DLSS_OPTIMAL_SETTINGS: StructType = StructType { data1: 0xef1d0957, data2: 0xfd58, data3: 0x4df7, data4: [0xb5, 0x04, 0x8b, 0x69, 0xd8, 0xaa, 0x6b, 0x76] };

const SDK_VERSION: u64 = (2 << 48) | (14 << 32) | (1 << 16) | 0xfedc;
const FEATURE_DLSS: u32 = 0;
const BUFFER_DEPTH: u32 = 0;
const BUFFER_MOTION_VECTORS: u32 = 1;
// sl::DLSSPreset
const PRESET_K: u32 = 11;
const PRESET_L: u32 = 12;
const PRESET_M: u32 = 13;
// kBufferTypeBiasCurrentColorHint
const BUFFER_BIAS_CURRENT_COLOR: u32 = 29;
const BUFFER_SCALING_INPUT_COLOR: u32 = 3;
const BUFFER_SCALING_OUTPUT_COLOR: u32 = 4;
const VALID_UNTIL_EVALUATE: u32 = 2;
const FLAG_DISABLE_CL_STATE_TRACKING: u64 = 1 << 0;
const FLAG_DISABLE_DEBUG_TEXT: u64 = 1 << 1;
const FLAG_USE_MANUAL_HOOKING: u64 = 1 << 2;
const FLAG_FRAME_BASED_TAGGING: u64 = 1 << 7;
const FALSE: u8 = 0;
const TRUE: u8 = 1;
/// The project id NGX is given: a custom engine has to name one (any GUID of its own).
const PROJECT_ID: &CStr = c"08d79c23-3148-4b8e-b9cd-1bbcaefa49a1";

#[repr(C)]
struct Preferences {
    base: Base,
    show_console: bool,
    log_level: u32,
    paths_to_plugins: *const *const u16,
    num_paths_to_plugins: u32,
    path_to_logs_and_data: *const u16,
    allocate_callback: *const c_void,
    release_callback: *const c_void,
    log_message_callback: Option<unsafe extern "C" fn(u32, *const c_char)>,
    flags: u64,
    features_to_load: *const u32,
    num_features_to_load: u32,
    application_id: u32,
    engine: u32,
    engine_version: *const c_char,
    project_id: *const c_char,
    render_api: u32,
}

#[repr(C)]
struct Resource {
    base: Base,
    kind: u8,
    native: *mut c_void,
    memory: *mut c_void,
    view: *mut c_void,
    state: u32,
    width: u32,
    height: u32,
    native_format: u32,
    mip_levels: u32,
    array_layers: u32,
    gpu_virtual_address: u64,
    flags: u32,
    usage: u32,
    internal_flags: u16,
    reserved: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Extent {
    top: u32,
    left: u32,
    width: u32,
    height: u32,
}

#[repr(C)]
struct ResourceTag {
    base: Base,
    resource: *mut Resource,
    kind: u32,
    lifecycle: u32,
    extent: Extent,
}

#[repr(C)]
struct ViewportHandle {
    base: Base,
    value: u32,
}

#[repr(C)]
struct AdapterInfo {
    base: Base,
    device_luid: *mut u8,
    device_luid_size_in_bytes: u32,
    vk_physical_device: *mut c_void,
}

#[repr(C)]
struct VulkanInfo {
    base: Base,
    device: vk::Device,
    instance: vk::Instance,
    physical_device: vk::PhysicalDevice,
    compute_queue_index: u32,
    compute_queue_family: u32,
    graphics_queue_index: u32,
    graphics_queue_family: u32,
    optical_flow_queue_index: u32,
    optical_flow_queue_family: u32,
    use_native_optical_flow_mode: bool,
    compute_queue_create_flags: u32,
    graphics_queue_create_flags: u32,
    optical_flow_queue_create_flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Version {
    major: u32,
    minor: u32,
    build: u32,
}

#[repr(C)]
struct FeatureRequirements {
    base: Base,
    flags: u32,
    max_num_cpu_threads: u32,
    max_num_viewports: u32,
    num_required_tags: u32,
    required_tags: *const u32,
    os_version_detected: Version,
    os_version_required: Version,
    driver_version_detected: Version,
    driver_version_required: Version,
    vk_num_compute_queues_required: u32,
    vk_num_graphics_queues_required: u32,
    vk_num_device_extensions: u32,
    vk_device_extensions: *const *const c_char,
    vk_num_instance_extensions: u32,
    vk_instance_extensions: *const *const c_char,
    vk_num_features12: u32,
    vk_features12: *const *const c_char,
    vk_num_features13: u32,
    vk_features13: *const *const c_char,
    vk_num_optical_flow_queues_required: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Float2 {
    x: f32,
    y: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Float3 {
    x: f32,
    y: f32,
    z: f32,
}

#[repr(C)]
struct Constants {
    base: Base,
    camera_view_to_clip: [[f32; 4]; 4],
    clip_to_camera_view: [[f32; 4]; 4],
    clip_to_lens_clip: [[f32; 4]; 4],
    clip_to_prev_clip: [[f32; 4]; 4],
    prev_clip_to_clip: [[f32; 4]; 4],
    jitter_offset: Float2,
    mvec_scale: Float2,
    camera_pinhole_offset: Float2,
    camera_pos: Float3,
    camera_up: Float3,
    camera_right: Float3,
    camera_fwd: Float3,
    camera_near: f32,
    camera_far: f32,
    camera_fov: f32,
    camera_aspect_ratio: f32,
    motion_vectors_invalid_value: f32,
    depth_inverted: u8,
    camera_motion_included: u8,
    motion_vectors_3d: u8,
    reset: u8,
    orthographic_projection: u8,
    motion_vectors_dilated: u8,
    motion_vectors_jittered: u8,
    min_relative_linear_depth_object_separation: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct DlssOptions {
    base: Base,
    mode: u32,
    output_width: u32,
    output_height: u32,
    sharpness: f32,
    pre_exposure: f32,
    exposure_scale: f32,
    color_buffers_hdr: u8,
    indicator_invert_axis_x: u8,
    indicator_invert_axis_y: u8,
    dlaa_preset: u32,
    quality_preset: u32,
    balanced_preset: u32,
    performance_preset: u32,
    ultra_performance_preset: u32,
    ultra_quality_preset: u32,
    use_auto_exposure: u8,
    alpha_upscaling_enabled: u8,
}

#[repr(C)]
#[derive(Default)]
struct DlssOptimalSettings {
    optimal_render_width: u32,
    optimal_render_height: u32,
    optimal_sharpness: f32,
    render_width_min: u32,
    render_height_min: u32,
    render_width_max: u32,
    render_height_max: u32,
}

#[repr(C)]
struct DlssOptimalSettingsS {
    base: Base,
    s: DlssOptimalSettings,
}

// (the sizes Streamline's own `sl::test::AbiValidation` expects: a field off by a byte
// fails the build here, not in NVIDIA's DLL)
const _: () = {
    assert!(std::mem::size_of::<Base>() == 32);
    assert!(std::mem::size_of::<Preferences>() == 144);
    assert!(std::mem::size_of::<Resource>() == 112);
    assert!(std::mem::size_of::<ResourceTag>() == 64);
    assert!(std::mem::size_of::<ViewportHandle>() == 40);
    assert!(std::mem::size_of::<Constants>() == 456);
    assert!(std::mem::size_of::<DlssOptions>() == 88);
    assert!(std::mem::size_of::<VulkanInfo>() == 96);
    assert!(std::mem::offset_of!(FeatureRequirements, vk_num_compute_queues_required) == 104);
    assert!(std::mem::offset_of!(FeatureRequirements, vk_device_extensions) == 120);
    assert!(std::mem::size_of::<FeatureRequirements>() == 184);
};

type Fn0 = unsafe extern "C" fn() -> i32;
type FnInit = unsafe extern "C" fn(*const Preferences, u64) -> i32;
type FnIsFeatureSupported = unsafe extern "C" fn(u32, *const AdapterInfo) -> i32;
type FnIsFeatureLoaded = unsafe extern "C" fn(u32, *mut bool) -> i32;
type FnSetD3DDevice = unsafe extern "C" fn(*mut c_void) -> i32;
type FnSetVulkanInfo = unsafe extern "C" fn(*const VulkanInfo) -> i32;
type FnGetFeatureRequirements = unsafe extern "C" fn(u32, *mut FeatureRequirements) -> i32;
type FnGetFeatureFunction = unsafe extern "C" fn(u32, *const c_char, *mut *mut c_void) -> i32;
type FnGetNewFrameToken = unsafe extern "C" fn(*mut *const c_void, *const u32) -> i32;
type FnSetTagForFrame = unsafe extern "C" fn(*const c_void, *const ViewportHandle, *const ResourceTag, u32, *mut c_void) -> i32;
type FnSetConstants = unsafe extern "C" fn(*const Constants, *const c_void, *const ViewportHandle) -> i32;
type FnEvaluateFeature = unsafe extern "C" fn(u32, *const c_void, *const *const Base, u32, *mut c_void) -> i32;
type FnFreeResources = unsafe extern "C" fn(u32, *const ViewportHandle) -> i32;
type FnUpgradeInterface = unsafe extern "C" fn(*mut *mut c_void) -> i32;
type FnDlssGetOptimalSettings = unsafe extern "C" fn(*const DlssOptions, *mut DlssOptimalSettingsS) -> i32;
type FnDlssSetOptions = unsafe extern "C" fn(*const ViewportHandle, *const DlssOptions) -> i32;

/// `sl::Result::eWarnOutOfVRAM`: a warning. The call did its work, the card is just over its
/// memory budget; treated as a failure it would switch DLSS off on a card that is merely full.
const WARN_OUT_OF_VRAM: i32 = 39;

fn result_name(r: i32) -> &'static str {
    const NAMES: [&str; 41] = [
        "eOk", "eErrorIO", "eErrorDriverOutOfDate", "eErrorOSOutOfDate", "eErrorOSDisabledHWS", "eErrorDeviceNotCreated",
        "eErrorNoSupportedAdapterFound", "eErrorAdapterNotSupported", "eErrorNoPlugins", "eErrorVulkanAPI", "eErrorDXGIAPI",
        "eErrorD3DAPI", "eErrorNRDAPI", "eErrorNVAPI", "eErrorReflexAPI", "eErrorNGXFailed", "eErrorJSONParsing",
        "eErrorMissingProxy", "eErrorMissingResourceState", "eErrorInvalidIntegration", "eErrorMissingInputParameter",
        "eErrorNotInitialized", "eErrorComputeFailed", "eErrorInitNotCalled", "eErrorExceptionHandler",
        "eErrorInvalidParameter", "eErrorMissingConstants", "eErrorDuplicatedConstants", "eErrorMissingOrInvalidAPI",
        "eErrorCommonConstantsMissing", "eErrorUnsupportedInterface", "eErrorFeatureMissing", "eErrorFeatureNotSupported",
        "eErrorFeatureMissingHooks", "eErrorFeatureFailedToLoad", "eErrorFeatureWrongPriority",
        "eErrorFeatureMissingDependency", "eErrorFeatureManagerInvalidState", "eErrorInvalidState", "eWarnOutOfVRAM", "?",
    ];
    NAMES.get(r as usize).copied().unwrap_or("?")
}



fn check(what: &str, r: i32) -> Result<()> {
    static VRAM_WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    match r {
        0 => Ok(()),
        WARN_OUT_OF_VRAM => {
            if !VRAM_WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                log::warn!("{what}: Streamline says the graphics card is over its memory budget; expect stutter, or lower the video memory or the window size");
            }
            Ok(())
        }
        _ => Err(anyhow!("{what} failed in Streamline: {} ({r})", result_name(r))),
    }
}

unsafe extern "C" fn sl_log(kind: u32, msg: *const c_char) {
    if msg.is_null() {
        return;
    }
    let text = unsafe { CStr::from_ptr(msg) }.to_string_lossy();
    let text = text.trim_end();
    match kind {
        2 => log::error!("Streamline: {text}"),
        1 => log::warn!("Streamline: {text}"),
        _ => log::debug!("Streamline: {text}"),
    }
}

/// The quality as `sl::DLSSMode` numbers it (0 off, 1 max performance, 2 balanced, 3 max
/// quality, 4 ultra performance, 5 ultra quality, 6 DLAA).
fn mode_code(quality: TemporalQuality) -> u32 {
    match quality {
        TemporalQuality::Performance => 1,
        TemporalQuality::Balanced => 2,
        TemporalQuality::Quality => 3,
        TemporalQuality::UltraPerformance => 4,
        TemporalQuality::Native => 6,
    }
}

fn dxgi_format(f: wgpu::TextureFormat) -> u32 {
    use wgpu::TextureFormat as Tf;
    (match f {
        Tf::Bgra8UnormSrgb => DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
        Tf::Bgra8Unorm => DXGI_FORMAT_B8G8R8A8_UNORM,
        Tf::Rgba8UnormSrgb => DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        Tf::Rgba8Unorm => DXGI_FORMAT_R8G8B8A8_UNORM,
        Tf::Rgb10a2Unorm => DXGI_FORMAT_R10G10B10A2_UNORM,
        Tf::Rgba16Float => DXGI_FORMAT_R16G16B16A16_FLOAT,
        Tf::Rg16Float => DXGI_FORMAT_R16G16_FLOAT,
        Tf::Rg32Float => DXGI_FORMAT_R32G32_FLOAT,
        Tf::R16Float => DXGI_FORMAT_R16_FLOAT,
        Tf::Depth32Float => DXGI_FORMAT_R32_FLOAT,
        _ => DXGI_FORMAT_UNKNOWN,
    })
    .0 as u32
}

/// A swap chain whose `Present` does nothing: Streamline's per-frame housekeeping runs on its
/// proxy's present, and the window's real swap chain is wgpu's (manual hooking).
#[windows::core::implement(IDXGISwapChain)]
struct NullSwapChain {
    device: ID3D12Device,
}

fn not_implemented<T>() -> windows::core::Result<T> {
    Err(windows::Win32::Foundation::E_NOTIMPL.into())
}

impl IDXGIObject_Impl for NullSwapChain_Impl {
    fn SetPrivateData(&self, _: *const windows::core::GUID, _: u32, _: *const c_void) -> windows::core::Result<()> {
        not_implemented()
    }
    fn SetPrivateDataInterface(&self, _: *const windows::core::GUID, _: windows::core::Ref<windows::core::IUnknown>) -> windows::core::Result<()> {
        not_implemented()
    }
    fn GetPrivateData(&self, _: *const windows::core::GUID, _: *mut u32, _: *mut c_void) -> windows::core::Result<()> {
        not_implemented()
    }
    fn GetParent(&self, _: *const windows::core::GUID, _: *mut *mut c_void) -> windows::core::Result<()> {
        not_implemented()
    }
}

impl IDXGIDeviceSubObject_Impl for NullSwapChain_Impl {
    fn GetDevice(&self, riid: *const windows::core::GUID, device: *mut *mut c_void) -> windows::core::Result<()> {
        unsafe { self.device.query(riid, device) }.ok()
    }
}

impl IDXGISwapChain_Impl for NullSwapChain_Impl {
    fn Present(&self, _: u32, _: DXGI_PRESENT) -> windows::core::HRESULT {
        windows::Win32::Foundation::S_OK
    }
    fn GetBuffer(&self, _: u32, _: *const windows::core::GUID, _: *mut *mut c_void) -> windows::core::Result<()> {
        not_implemented()
    }
    fn SetFullscreenState(&self, _: windows::core::BOOL, _: windows::core::Ref<IDXGIOutput>) -> windows::core::Result<()> {
        not_implemented()
    }
    fn GetFullscreenState(&self, _: *mut windows::core::BOOL, _: windows::core::OutRef<IDXGIOutput>) -> windows::core::Result<()> {
        not_implemented()
    }
    fn GetDesc(&self) -> windows::core::Result<DXGI_SWAP_CHAIN_DESC> {
        not_implemented()
    }
    fn ResizeBuffers(&self, _: u32, _: u32, _: u32, _: DXGI_FORMAT, _: &DXGI_SWAP_CHAIN_FLAG) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResizeTarget(&self, _: *const DXGI_MODE_DESC) -> windows::core::Result<()> {
        not_implemented()
    }
    fn GetContainingOutput(&self) -> windows::core::Result<IDXGIOutput> {
        not_implemented()
    }
    fn GetFrameStatistics(&self, _: *mut DXGI_FRAME_STATISTICS) -> windows::core::Result<()> {
        not_implemented()
    }
    fn GetLastPresentCount(&self) -> windows::core::Result<u32> {
        not_implemented()
    }
}

fn load_fn<T: Copy>(module: HMODULE, name: &CStr) -> Result<T> {
    debug_assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<usize>());
    let f = unsafe { GetProcAddress(module, PCSTR(name.as_ptr().cast())) }
        .ok_or_else(|| anyhow!("{} is not exported by sl.interposer.dll (an older Streamline?)", name.to_string_lossy()))?;
    Ok(unsafe { std::mem::transmute_copy::<_, T>(&f) })
}

fn init(module: HMODULE, dir: &std::path::Path, features: &'static [u32], render_api: u32) -> Result<(Vec<u16>, Vec<*const u16>)> {
    let init: FnInit = load_fn(module, c"slInit")?;
    let plugin_dir = wide(dir);
    let plugin_paths = vec![plugin_dir.as_ptr()];
    let engine_version = concat!("openOMSI ", env!("CARGO_PKG_VERSION"), "\0");
    let prefs = Preferences {
        base: Base::new(PREFERENCES, 1),
        show_console: false,
        log_level: 1,
        paths_to_plugins: plugin_paths.as_ptr(),
        num_paths_to_plugins: 1,
        path_to_logs_and_data: std::ptr::null(),
        allocate_callback: std::ptr::null(),
        release_callback: std::ptr::null(),
        log_message_callback: Some(sl_log),
        flags: FLAG_DISABLE_CL_STATE_TRACKING | FLAG_DISABLE_DEBUG_TEXT | FLAG_USE_MANUAL_HOOKING | FLAG_FRAME_BASED_TAGGING,
        features_to_load: features.as_ptr(),
        num_features_to_load: features.len() as u32,
        application_id: 0,
        engine: 0,
        engine_version: engine_version.as_ptr().cast(),
        project_id: PROJECT_ID.as_ptr(),
        // (sl::RenderAPI: 1 Direct3D 12, 2 Vulkan)
        render_api,
    };
    check("slInit", unsafe { init(&prefs, SDK_VERSION) })?;
    Ok((plugin_dir, plugin_paths))
}

fn feature_fn(module: HMODULE, feature: u32, name: &CStr) -> Result<*mut c_void> {
    let get_feature_function: FnGetFeatureFunction = load_fn(module, c"slGetFeatureFunction")?;
    let mut f: *mut c_void = std::ptr::null_mut();
    check(&format!("slGetFeatureFunction({})", name.to_string_lossy()), unsafe { get_feature_function(feature, name.as_ptr(), &mut f) })?;
    if f.is_null() {
        bail!("Streamline does not offer {}", name.to_string_lossy());
    }
    Ok(f)
}

enum Device {
    Dx12(ID3D12Device),
    Vulkan(VkGpu),
}

enum Backend {
    Dx12 {
        ring: ListRing,
        pacer: Option<IDXGISwapChain>,
    },
    Vulkan {
        ring: VkRing,
    },
}

impl Backend {
    fn wait(&mut self) {
        match self {
            Backend::Dx12 { ring, .. } => ring.wait(),
            Backend::Vulkan { ring, .. } => ring.wait(),
        }
    }
}

pub struct Runtime {
    module: HMODULE,
    shutdown: Fn0,
    get_new_frame_token: FnGetNewFrameToken,
    set_tag_for_frame: FnSetTagForFrame,
    set_constants: FnSetConstants,
    evaluate_feature: FnEvaluateFeature,
    free_resources: FnFreeResources,
    get_optimal_settings: FnDlssGetOptimalSettings,
    set_options: FnDlssSetOptions,
    /// (kept alive: `slInit` was given pointers into them)
    _paths: (Vec<u16>, Vec<*const u16>),
    viewport: ViewportHandle,
    options_set: Option<(TemporalQuality, (u32, u32))>,
    optimal: Vec<((TemporalQuality, (u32, u32)), (u32, u32))>,
    frame: u32,
    backend: Backend,
    allocated: bool,
}

// (only the render thread ever calls into Streamline; what is held are raw handles)
unsafe impl Send for Runtime {}

struct Prepared {
    module: HMODULE,
    paths: (Vec<u16>, Vec<*const u16>),
}

// (handles and strings handed over to the render thread once)
unsafe impl Send for Prepared {}

static PREPARED: std::sync::Mutex<Option<Prepared>> = std::sync::Mutex::new(None);

fn open_interposer(dir: &std::path::Path) -> Result<HMODULE> {
    let missing = omsi_cfg::upscaler_files::missing(dir, "dlss", omsi_cfg::upscaler_files::Api::Dx12);
    if !missing.is_empty() {
        bail!("{} not found in {} (they come with NVIDIA's Streamline SDK, bin/x64)", missing.join(", "), dir.display());
    }
    let interposer = wide(&dir.join("sl.interposer.dll"));
    Ok(unsafe { LoadLibraryExW(PCWSTR(interposer.as_ptr()), None, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) }.context("opening sl.interposer.dll")?)
}

fn static_names(count: u32, names: *const *const c_char) -> Vec<&'static CStr> {
    (0..count as usize)
        .filter_map(|i| {
            let p = unsafe { *names.add(i) };
            (!p.is_null()).then(|| &*Box::leak(unsafe { CStr::from_ptr(p) }.to_owned().into_boxed_c_str()))
        })
        .collect()
}

pub fn vulkan_extensions() -> Result<Vec<&'static CStr>> {
    vulkan_abandon();
    let dir = game_dir();
    let module = open_interposer(&dir)?;
    let started = (|| -> Result<(Vec<&'static CStr>, (Vec<u16>, Vec<*const u16>))> {
        let shutdown: Fn0 = load_fn(module, c"slShutdown")?;
        let requirements: FnGetFeatureRequirements = load_fn(module, c"slGetFeatureRequirements")?;
        static FEATURES: [u32; 1] = [FEATURE_DLSS];
        let paths = init(module, &dir, &FEATURES, 2)?;
        (|| {
            let mut r = FeatureRequirements {
                base: Base::new(FEATURE_REQUIREMENTS, 2),
                flags: 0,
                max_num_cpu_threads: 0,
                max_num_viewports: 0,
                num_required_tags: 0,
                required_tags: std::ptr::null(),
                os_version_detected: Version::default(),
                os_version_required: Version::default(),
                driver_version_detected: Version::default(),
                driver_version_required: Version::default(),
                vk_num_compute_queues_required: 0,
                vk_num_graphics_queues_required: 0,
                vk_num_device_extensions: 0,
                vk_device_extensions: std::ptr::null(),
                vk_num_instance_extensions: 0,
                vk_instance_extensions: std::ptr::null(),
                vk_num_features12: 0,
                vk_features12: std::ptr::null(),
                vk_num_features13: 0,
                vk_features13: std::ptr::null(),
                vk_num_optical_flow_queues_required: 0,
            };
            check("slGetFeatureRequirements(DLSS)", unsafe { requirements(FEATURE_DLSS, &mut r) })?;
            // (sl::FeatureRequirementFlags::eVulkanSupported)
            if r.flags & (1 << 2) == 0 {
                bail!("this Streamline's DLSS plugin does not run on Vulkan");
            }
            let names = |n, p| static_names(n, p).iter().map(|c| c.to_string_lossy().into_owned()).collect::<Vec<_>>().join(", ");
            let wanted: Vec<String> = static_names(r.vk_num_features12, r.vk_features12).iter().chain(&static_names(r.vk_num_features13, r.vk_features13)).map(|c| c.to_string_lossy().into_owned()).collect();
            let other: Vec<&String> = wanted.iter().filter(|f| !VULKAN_FEATURES.contains(&f.as_str())).collect();
            if !other.is_empty() {
                bail!("DLSS asks for Vulkan features wgpu's device is not made with ({}); choose DirectX 12 for DLSS", other.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
            }
            let _ = names;
            if r.vk_num_compute_queues_required + r.vk_num_graphics_queues_required + r.vk_num_optical_flow_queues_required > 0 {
                bail!("DLSS asks for Vulkan queues of its own, which wgpu's device does not make; choose DirectX 12 for DLSS");
            }
            if r.vk_num_instance_extensions > 0 {
                log::info!("DLSS on Vulkan wants the instance extensions {} (wgpu's instance must have them)", names(r.vk_num_instance_extensions, r.vk_instance_extensions));
            }
            Ok((static_names(r.vk_num_device_extensions, r.vk_device_extensions), paths))
        })()
        .inspect_err(|_| unsafe {
            shutdown();
        })
    })();
    match started {
        Ok((extensions, paths)) => {
            *PREPARED.lock().unwrap_or_else(|e| e.into_inner()) = Some(Prepared { module, paths });
            Ok(extensions)
        }
        Err(e) => {
            unsafe {
                let _ = FreeLibrary(module);
            }
            Err(e)
        }
    }
}

/// The Vulkan 1.2 features DLSS may ask for that the device is made with
pub const VULKAN_FEATURES: [&str; 3] = ["timelineSemaphore", "descriptorIndexing", "bufferDeviceAddress"];

pub fn vulkan_abandon() {
    if let Some(p) = PREPARED.lock().unwrap_or_else(|e| e.into_inner()).take() {
        if let Ok(shutdown) = load_fn::<Fn0>(p.module, c"slShutdown") {
            unsafe { shutdown() };
        }
        unsafe {
            let _ = FreeLibrary(p.module);
        }
    }
}

struct Calls {
    is_supported: FnIsFeatureSupported,
    is_loaded: FnIsFeatureLoaded,
}

impl Runtime {
    pub(crate) fn wait(&mut self) {
        self.backend.wait();
    }

    pub fn load(device: &wgpu::Device) -> Result<Runtime> {
        if let Some(hal) = unsafe { device.as_hal::<wgpu_hal::api::Dx12>() } {
            let raw_device = hal.raw_device().clone();
            drop(hal);
            // (the game fell back from Vulkan: `slInit` must not run on a session still open)
            vulkan_abandon();
            let dir = game_dir();
            let module = open_interposer(&dir)?;
            static FEATURES: [u32; 1] = [FEATURE_DLSS];
            return (|| {
                let shutdown: Fn0 = load_fn(module, c"slShutdown")?;
                let paths = init(module, &dir, &FEATURES, 1)?;
                Self::start(module, &dir, paths, Device::Dx12(raw_device)).inspect_err(|_| unsafe {
                    shutdown();
                })
            })()
            .inspect_err(|_| unsafe {
                let _ = FreeLibrary(module);
            });
        }
        let gpu = VkGpu::of(device).ok_or_else(|| anyhow!("the game is not running on DirectX 12 or Vulkan (Settings: Graphics API)"))?;
        let prepared = PREPARED.lock().unwrap_or_else(|e| e.into_inner()).take().ok_or_else(|| anyhow!("on Vulkan DLSS takes its place from the next start of the game, chosen before it (its device extensions are given when the device is made); this run goes on without it"))?;
        let module = prepared.module;
        let shutdown: Fn0 = load_fn(module, c"slShutdown")?;
        Self::start(module, &game_dir(), prepared.paths, Device::Vulkan(gpu)).inspect_err(|_| unsafe {
            shutdown();
            let _ = FreeLibrary(module);
        })
    }

    fn start(module: HMODULE, dir: &std::path::Path, paths: (Vec<u16>, Vec<*const u16>), device: Device) -> Result<Runtime> {
        let shutdown: Fn0 = load_fn(module, c"slShutdown")?;
        let calls = Calls { is_supported: load_fn(module, c"slIsFeatureSupported")?, is_loaded: load_fn(module, c"slIsFeatureLoaded")? };
        let get_new_frame_token: FnGetNewFrameToken = load_fn(module, c"slGetNewFrameToken")?;
        let set_tag_for_frame: FnSetTagForFrame = load_fn(module, c"slSetTagForFrame")?;
        let set_constants: FnSetConstants = load_fn(module, c"slSetConstants")?;
        let evaluate_feature: FnEvaluateFeature = load_fn(module, c"slEvaluateFeature")?;
        let free_resources: FnFreeResources = load_fn(module, c"slFreeResources")?;
        let backend = match device {
            Device::Dx12(device) => {
                let set_device: FnSetD3DDevice = load_fn(module, c"slSetD3DDevice")?;
                let upgrade_interface: FnUpgradeInterface = load_fn(module, c"slUpgradeInterface")?;
                check("slSetD3DDevice", unsafe { set_device(device.as_raw()) })?;
                let mut luid = unsafe { device.GetAdapterLuid() };
                let adapter = AdapterInfo {
                    base: Base::new(ADAPTER_INFO, 1),
                    device_luid: (&mut luid as *mut windows::Win32::Foundation::LUID).cast(),
                    device_luid_size_in_bytes: std::mem::size_of::<windows::Win32::Foundation::LUID>() as u32,
                    vk_physical_device: std::ptr::null_mut(),
                };
                Self::supported(module, &calls, &adapter)?;
                let ring = ListRing::new(&device).context("DLSS's command lists")?;
                let pacer: IDXGISwapChain = NullSwapChain { device: device.clone() }.into();
                let mut raw = pacer.as_raw();
                let r = unsafe { upgrade_interface(&mut raw) };
                let pacer = if r == 0 && !raw.is_null() && raw != pacer.as_raw() {
                    Some(unsafe { IDXGISwapChain::from_raw(raw) })
                } else {
                    log::warn!("DLSS: no swap-chain proxy from Streamline ({}), so its per-frame housekeeping is skipped", result_name(r));
                    None
                };
                Backend::Dx12 { ring, pacer }
            }
            Device::Vulkan(gpu) => {
                let set_vulkan_info: FnSetVulkanInfo = load_fn(module, c"slSetVulkanInfo")?;
                let info = VulkanInfo {
                    base: Base::new(VULKAN_INFO, 3),
                    device: gpu.device.handle(),
                    instance: gpu.instance,
                    physical_device: gpu.physical,
                    compute_queue_index: gpu.queue_index,
                    compute_queue_family: gpu.family,
                    graphics_queue_index: gpu.queue_index,
                    graphics_queue_family: gpu.family,
                    optical_flow_queue_index: 0,
                    optical_flow_queue_family: 0,
                    use_native_optical_flow_mode: false,
                    compute_queue_create_flags: 0,
                    graphics_queue_create_flags: 0,
                    optical_flow_queue_create_flags: 0,
                };
                check("slSetVulkanInfo", unsafe { set_vulkan_info(&info) })?;
                let adapter = AdapterInfo {
                    base: Base::new(ADAPTER_INFO, 1),
                    device_luid: std::ptr::null_mut(),
                    device_luid_size_in_bytes: 0,
                    vk_physical_device: vk::Handle::as_raw(gpu.physical) as *mut c_void,
                };
                Self::supported(module, &calls, &adapter)?;
                Backend::Vulkan { ring: VkRing::new(&gpu).context("DLSS's command buffers")? }
            }
        };
        let get_optimal_settings: FnDlssGetOptimalSettings = unsafe { std::mem::transmute(feature_fn(module, FEATURE_DLSS, c"slDLSSGetOptimalSettings")?) };
        let set_options: FnDlssSetOptions = unsafe { std::mem::transmute(feature_fn(module, FEATURE_DLSS, c"slDLSSSetOptions")?) };
        let api = if matches!(backend, Backend::Dx12 { .. }) { "DirectX 12" } else { "Vulkan" };
        log::info!("DLSS: Streamline {}.{}.{} and its DLSS plugin loaded from {} ({api})", SDK_VERSION >> 48, (SDK_VERSION >> 32) & 0xffff, (SDK_VERSION >> 16) & 0xffff, dir.display());
        Ok(Runtime {
            module,
            shutdown,
            get_new_frame_token,
            set_tag_for_frame,
            set_constants,
            evaluate_feature,
            free_resources,
            get_optimal_settings,
            set_options,
            _paths: paths,
            viewport: ViewportHandle { base: Base::new(VIEWPORT_HANDLE, 1), value: 0 },
            options_set: None,
            optimal: Vec::new(),
            frame: 0,
            backend,
            allocated: false,
        })
    }

    fn supported(_module: HMODULE, calls: &Calls, adapter: &AdapterInfo) -> Result<()> {
        check("the DLSS check of this card (it takes a GeForce RTX and an up-to-date driver)", unsafe { (calls.is_supported)(FEATURE_DLSS, adapter) })?;
        let mut loaded = false;
        check("slIsFeatureLoaded", unsafe { (calls.is_loaded)(FEATURE_DLSS, &mut loaded) })?;
        if !loaded {
            bail!("sl.dlss.dll was not loaded by Streamline");
        }
        Ok(())
    }

    /// `sl::DLSSOptions`, with the newer SDK's default presets set by hand: an older nvngx_dlss.dll
    /// would pick the CNN model.
    fn options(quality: TemporalQuality, (w, h): (u32, u32)) -> DlssOptions {
        DlssOptions {
            base: Base::new(DLSS_OPTIONS, 3),
            mode: mode_code(quality),
            output_width: w,
            output_height: h,
            sharpness: 0.0,
            pre_exposure: 1.0,
            exposure_scale: 1.0,
            color_buffers_hdr: FALSE,
            indicator_invert_axis_x: FALSE,
            indicator_invert_axis_y: FALSE,
            dlaa_preset: PRESET_K,
            quality_preset: PRESET_K,
            balanced_preset: PRESET_K,
            performance_preset: PRESET_M,
            ultra_performance_preset: PRESET_L,
            ultra_quality_preset: PRESET_K,
            use_auto_exposure: TRUE,
            alpha_upscaling_enabled: FALSE,
        }
    }

    pub fn render_size(&mut self, quality: TemporalQuality, out: (u32, u32)) -> (u32, u32) {
        if quality == TemporalQuality::Native {
            return out;
        }
        if let Some((_, s)) = self.optimal.iter().find(|(k, _)| *k == (quality, out)) {
            return *s;
        }
        let options = Self::options(quality, out);
        let mut settings = DlssOptimalSettingsS { base: Base::new(DLSS_OPTIMAL_SETTINGS, 1), s: DlssOptimalSettings::default() };
        let r = unsafe { (self.get_optimal_settings)(&options, &mut settings) };
        let size = if r == 0 && settings.s.optimal_render_width > 0 && settings.s.optimal_render_height > 0 {
            (settings.s.optimal_render_width.min(out.0), settings.s.optimal_render_height.min(out.1))
        } else {
            log::warn!("DLSS: Streamline gave no picture size for {quality:?} in a {}x{} window ({}); taking NVIDIA's ratio", out.0, out.1, result_name(r));
            fallback_render_size(quality, out)
        };
        // (only this window's sizes are kept and told: a live resize asks for one at every step)
        if self.optimal.is_empty() {
            log::info!("DLSS {quality:?}: the {}x{} window is drawn at {}x{}", out.0, out.1, size.0, size.1);
        }
        self.optimal.retain(|((_, window), _)| *window == out);
        self.optimal.push(((quality, out), size));
        size
    }

    pub fn evaluate(&mut self, queue: &wgpu::Queue, quality: TemporalQuality, inputs: &Inputs, c: &FrameConstants) -> Result<()> {
        if self.options_set != Some((quality, inputs.output_size)) {
            // (a new quality or window size: resizing the proxy makes Streamline drop what it
            // made for the old one)
            if let (Backend::Dx12 { pacer: Some(p), .. }, Some(_)) = (&self.backend, self.options_set) {
                let _ = unsafe { p.ResizeBuffers(0, inputs.output_size.0, inputs.output_size.1, DXGI_FORMAT_UNKNOWN, DXGI_SWAP_CHAIN_FLAG(0)) };
            }
            // (on Vulkan no present of ours ever empties Streamline's queue of old contexts: without
            // this every window size kept its own, up to a full card)
            if matches!(self.backend, Backend::Vulkan { .. }) && self.allocated {
                self.backend.wait();
                check("slFreeResources(DLSS)", unsafe { (self.free_resources)(FEATURE_DLSS, &self.viewport) })?;
                self.allocated = false;
            }
            let options = Self::options(quality, inputs.output_size);
            check("slDLSSSetOptions", unsafe { (self.set_options)(&self.viewport, &options) })?;
            self.options_set = Some((quality, inputs.output_size));
        }
        let (rw, rh) = inputs.render_size;
        let (ow, oh) = inputs.output_size;
        let blank = |native: *mut c_void, state: u32, format: u32, (w, h): (u32, u32)| Resource {
            base: Base::new(RESOURCE, 1),
            kind: 0,
            native,
            memory: std::ptr::null_mut(),
            view: std::ptr::null_mut(),
            state,
            width: w,
            height: h,
            native_format: format,
            mip_levels: 1,
            array_layers: 1,
            gpu_virtual_address: 0,
            flags: 0,
            usage: 0,
            internal_flags: 0,
            reserved: 0,
        };
        let textures = [inputs.color, inputs.depth, inputs.motion, inputs.output, inputs.mask];
        let sizes = [(rw, rh), (rw, rh), (rw, rh), (ow, oh), (rw, rh)];
        // (the picture's resource is plain UNORM and drawn through an sRGB view: see
        // `temporal_targets`. DLSS takes its sRGB-encoded numbers as they are, as FSR 3 does:
        // read through an sRGB view it made them linear and gave them back so, too dark)
        let formats = [inputs.color.format(), inputs.depth.format(), inputs.motion.format(), inputs.output.format(), inputs.mask.format()];
        // (the states the scene's submit leaves them in: `temporal_end_of_scene` makes sure the
        // last pass on each is a render pass)
        let depth_at = 1;
        let mut res: [Resource; 5] = match &self.backend {
            Backend::Dx12 { .. } => {
                let raw = |t: &wgpu::Texture| -> Result<ID3D12Resource> {
                    let h = unsafe { t.as_hal::<wgpu_hal::api::Dx12>() }.ok_or_else(|| anyhow!("no Direct3D 12 resource under a DLSS texture"))?;
                    Ok(unsafe { h.raw_resource() }.clone())
                };
                let mut out = Vec::with_capacity(5);
                for (k, t) in textures.iter().enumerate() {
                    let state = if k == depth_at { D3D12_RESOURCE_STATE_DEPTH_WRITE } else { D3D12_RESOURCE_STATE_RENDER_TARGET };
                    out.push(blank(raw(t)?.as_raw(), state.0 as u32, dxgi_format(formats[k]), sizes[k]));
                }
                out.try_into().map_err(|_| anyhow!("five resources"))?
            }
            Backend::Vulkan { .. } => {
                let order = [0usize, 1, 2, 4, 3];
                let mut out = Vec::with_capacity(5);
                for (k, t) in textures.iter().enumerate() {
                    let (image, memory) = super::vulkan::image_and_memory(t)?;
                    let view = super::vulkan::view(inputs.views[order[k]])?;
                    let depth = k == depth_at;
                    let layout = if depth { vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL } else { vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL };
                    let mut r = blank(vk::Handle::as_raw(image) as *mut c_void, layout.as_raw() as u32, super::vulkan::format(formats[k]).as_raw() as u32, sizes[k]);
                    r.memory = vk::Handle::as_raw(memory) as *mut c_void;
                    r.view = vk::Handle::as_raw(view) as *mut c_void;
                    r.flags = if t.format() != formats[k] { vk::ImageCreateFlags::MUTABLE_FORMAT.as_raw() } else { 0 };
                    r.usage = super::vulkan::usage(t.usage(), depth).as_raw();
                    out.push(r);
                }
                out.try_into().map_err(|_| anyhow!("five resources"))?
            }
        };
        let render_extent = Extent { top: 0, left: 0, width: rw, height: rh };
        let output_extent = Extent { top: 0, left: 0, width: ow, height: oh };
        let [c0, c1, c2, c3, c4] = &mut res;
        let tag = |r: &mut Resource, kind: u32, extent: Extent| ResourceTag { base: Base::new(RESOURCE_TAG, 1), resource: r, kind, lifecycle: VALID_UNTIL_EVALUATE, extent };
        let tags = [
            tag(c0, BUFFER_SCALING_INPUT_COLOR, render_extent),
            tag(c1, BUFFER_DEPTH, render_extent),
            tag(c2, BUFFER_MOTION_VECTORS, render_extent),
            tag(c3, BUFFER_SCALING_OUTPUT_COLOR, output_extent),
            tag(c4, BUFFER_BIAS_CURRENT_COLOR, render_extent),
        ];
        let mut token: *const c_void = std::ptr::null();
        self.frame = self.frame.wrapping_add(1);
        check("slGetNewFrameToken", unsafe { (self.get_new_frame_token)(&mut token, &self.frame) })?;
        if token.is_null() {
            bail!("slGetNewFrameToken returned no token");
        }
        let f2 = |v: [f32; 2]| Float2 { x: v[0], y: v[1] };
        let f3 = |v: [f32; 3]| Float3 { x: v[0], y: v[1], z: v[2] };
        let identity = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        let constants = Constants {
            base: Base::new(CONSTANTS, 2),
            camera_view_to_clip: c.view_to_clip,
            clip_to_camera_view: c.clip_to_view,
            clip_to_lens_clip: identity,
            clip_to_prev_clip: c.clip_to_prev_clip,
            prev_clip_to_clip: c.prev_clip_to_clip,
            jitter_offset: f2(c.jitter),
            mvec_scale: f2([1.0 / rw as f32, 1.0 / rh as f32]),
            camera_pinhole_offset: f2([0.0, 0.0]),
            camera_pos: f3(c.camera_pos),
            camera_up: f3(c.camera_up),
            camera_right: f3(c.camera_right),
            camera_fwd: f3(c.camera_fwd),
            camera_near: c.near,
            camera_far: c.far,
            camera_fov: c.fov,
            camera_aspect_ratio: c.aspect,
            motion_vectors_invalid_value: 0.0,
            depth_inverted: TRUE,
            camera_motion_included: TRUE,
            motion_vectors_3d: FALSE,
            reset: if c.reset { TRUE } else { FALSE },
            orthographic_projection: FALSE,
            motion_vectors_dilated: FALSE,
            motion_vectors_jittered: FALSE,
            min_relative_linear_depth_object_separation: 40.0,
        };
        let (list_ptr, dx12_list) = match &mut self.backend {
            Backend::Dx12 { ring, .. } => {
                let list = ring.begin()?;
                (list.as_raw(), Some(list))
            }
            Backend::Vulkan { ring, .. } => (vk::Handle::as_raw(ring.begin()?) as *mut c_void, None),
        };
        let recorded = (|| -> Result<()> {
            check("slSetConstants", unsafe { (self.set_constants)(&constants, token, &self.viewport) })?;
            check("slSetTagForFrame", unsafe { (self.set_tag_for_frame)(token, &self.viewport, tags.as_ptr(), tags.len() as u32, list_ptr) })?;
            let inputs: [*const Base; 1] = [&self.viewport.base];
            check("slEvaluateFeature(DLSS)", unsafe { (self.evaluate_feature)(FEATURE_DLSS, token, inputs.as_ptr(), 1, list_ptr) })?;
            Ok(())
        })();
        // (closed and submitted even when a call above failed: an empty list does no harm, and
        // the fence then still says when this slot is free)
        drop(dx12_list);
        match &mut self.backend {
            Backend::Dx12 { ring, .. } => {
                let queue_hal = unsafe { queue.as_hal::<wgpu_hal::api::Dx12>() }.ok_or_else(|| anyhow!("no Direct3D 12 queue under wgpu's"))?;
                let raw_queue = queue_hal.as_raw().clone();
                drop(queue_hal);
                ring.submit(&raw_queue)?;
            }
            Backend::Vulkan { ring, .. } => ring.submit(super::vulkan::queue(queue)?)?,
        }
        recorded?;
        self.allocated = true;
        if let Backend::Dx12 { pacer: Some(p), .. } = &self.backend {
            let _ = unsafe { p.Present(0, DXGI_PRESENT(0)) };
        }
        Ok(())
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.backend.wait();
        unsafe {
            if self.allocated {
                let _ = (self.free_resources)(FEATURE_DLSS, &self.viewport);
            }
            if let Backend::Dx12 { pacer, .. } = &mut self.backend {
                *pacer = None;
            }
            let r = (self.shutdown)();
            if r != 0 {
                log::warn!("DLSS: Streamline did not shut down cleanly: {}", result_name(r));
            }
            let _ = FreeLibrary(self.module);
        }
    }
}
