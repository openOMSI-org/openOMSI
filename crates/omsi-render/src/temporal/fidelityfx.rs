// The FidelityFX API structures, constants and identifiers below follow AMD's FidelityFX SDK
// headers (https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK, v2.3.0:
// `ffx_api.h`, `ffx_api_types.h`, `dx12/ffx_api_dx12.h`, `ffx_upscale.h`), which come under this
// licence:
//
// Copyright (C) 2024 Advanced Micro Devices, Inc.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files(the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and /or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions :
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

//! AMD FSR 3 upscaling by way of AMD's FidelityFX API: Windows, DirectX 12, any card that runs it
//! well. It takes the same inputs as DLSS (`super::Inputs`) and leaves the same kind of picture:
//! the sRGB-encoded values it was given, at the window's size.
//!
//! `amd_fidelityfx_loader_dx12.dll` and `amd_fidelityfx_upscaler_dx12.dll` (AMD's signed DLLs
//! from the SDK, MIT) are opened from the game's folder when the first frame asks for them;
//! nothing is linked.

use super::d3d12::{game_dir, wide, ListRing};
use super::{FrameConstants, Inputs};
use anyhow::{anyhow, bail, Context, Result};
use std::ffi::{c_void, CStr};
use std::sync::OnceLock;
use windows::core::{Interface, PCSTR, PCWSTR};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR};

// --- the FidelityFX API's types, as its v2.3.0 headers lay them out

type FfxContext = *mut c_void;

#[repr(C)]
struct Header {
    kind: u64,
    next: *const Header,
}

impl Header {
    const fn new(kind: u64) -> Header {
        Header { kind, next: std::ptr::null() }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Dimensions {
    width: u32,
    height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Coords {
    x: f32,
    y: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ResourceDescription {
    kind: u32,
    format: u32,
    width: u32,
    height: u32,
    depth: u32,
    mip_count: u32,
    flags: u32,
    usage: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Resource {
    resource: *mut c_void,
    description: ResourceDescription,
    state: u32,
}

impl Resource {
    const NONE: Resource = Resource { resource: std::ptr::null_mut(), description: ResourceDescription { kind: 0, format: 0, width: 0, height: 0, depth: 0, mip_count: 0, flags: 0, usage: 0 }, state: 0 };
}

#[repr(C)]
struct CreateBackendDx12 {
    header: Header,
    device: *mut c_void,
}

#[repr(C)]
struct CreateVersion {
    header: Header,
    version: u32,
}

#[repr(C)]
struct CreateUpscale {
    header: Header,
    flags: u32,
    max_render_size: Dimensions,
    max_upscale_size: Dimensions,
    message: *const c_void,
}

#[repr(C)]
struct DispatchUpscale {
    header: Header,
    command_list: *mut c_void,
    color: Resource,
    depth: Resource,
    motion_vectors: Resource,
    exposure: Resource,
    reactive: Resource,
    transparency_and_composition: Resource,
    output: Resource,
    jitter_offset: Coords,
    motion_vector_scale: Coords,
    render_size: Dimensions,
    upscale_size: Dimensions,
    enable_sharpening: bool,
    sharpness: f32,
    frame_time_delta: f32,
    pre_exposure: f32,
    reset: bool,
    camera_near: f32,
    camera_far: f32,
    camera_fov_vertical: f32,
    view_space_to_meters: f32,
    flags: u32,
}

// (sizes the SDK's headers give on x64: a field out of place fails the build here)
const _: () = {
    assert!(std::mem::size_of::<Resource>() == 48);
    assert!(std::mem::size_of::<CreateVersion>() == 24);
    assert!(std::mem::size_of::<CreateUpscale>() == 48);
    assert!(std::mem::size_of::<DispatchUpscale>() == 432);
    assert!(std::mem::offset_of!(DispatchUpscale, output) == 312);
    assert!(std::mem::offset_of!(DispatchUpscale, flags) == 428);
};

const CREATE_BACKEND_DX12: u64 = 0x0000_0002;
const CREATE_UPSCALE: u64 = 0x0001_0000;
const DISPATCH_UPSCALE: u64 = 0x0001_0001;
const CREATE_UPSCALE_VERSION: u64 = 0x0001_000b;
/// `FFX_UPSCALER_VERSION` (4.1.1) of the header these structs follow.
const UPSCALER_VERSION: u32 = (4 << 22) | (1 << 12) | 1;
// FfxApiCreateContextUpscaleFlags
const UPSCALE_DEPTH_INVERTED: u32 = 1 << 3;
const UPSCALE_NON_LINEAR_COLORSPACE: u32 = 1 << 8;
// FfxApiSurfaceFormat
const FORMAT_UNKNOWN: u32 = 0;
const FORMAT_R16G16_FLOAT: u32 = 18;
const FORMAT_R8G8B8A8_UNORM: u32 = 10;
const FORMAT_R8G8B8A8_SRGB: u32 = 12;
const FORMAT_B8G8R8A8_UNORM: u32 = 14;
const FORMAT_B8G8R8A8_SRGB: u32 = 15;
const FORMAT_R32_FLOAT: u32 = 28;
const FORMAT_R8_UNORM: u32 = 25;
// FfxApiResourceState and FfxApiResourceUsage
const STATE_RENDER_TARGET: u32 = 1 << 8;
const STATE_DEPTH_ATTACHMENT: u32 = 1 << 9;
const USAGE_UAV: u32 = 1 << 1;
const USAGE_DEPTHTARGET: u32 = 1 << 2;
const TYPE_TEXTURE2D: u32 = 2;

type FnCreateContext = unsafe extern "C" fn(*mut FfxContext, *const Header, *const c_void) -> u32;
type FnDestroyContext = unsafe extern "C" fn(*mut FfxContext, *const c_void) -> u32;
type FnDispatch = unsafe extern "C" fn(*mut FfxContext, *const Header) -> u32;

/// The loader DLL's entry points.
struct Api {
    _module: HMODULE,
    create_context: FnCreateContext,
    destroy_context: FnDestroyContext,
    dispatch: FnDispatch,
}

// (function pointers into a DLL loaded for the process's lifetime)
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

static API: OnceLock<Option<Api>> = OnceLock::new();

fn load_api() -> Result<Api> {
    let loader = game_dir().join("amd_fidelityfx_loader_dx12.dll");
    let path = wide(&loader);
    let module = unsafe { LoadLibraryExW(PCWSTR(path.as_ptr()), None, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) }.with_context(|| format!("opening {}", loader.display()))?;
    fn get<T: Copy>(module: HMODULE, name: &CStr) -> Result<T> {
        let f = unsafe { GetProcAddress(module, PCSTR(name.as_ptr().cast())) }.ok_or_else(|| anyhow!("{} is not exported by AMD's loader DLL", name.to_string_lossy()))?;
        Ok(unsafe { std::mem::transmute_copy::<_, T>(&f) })
    }
    Ok(Api {
        create_context: get(module, c"ffxCreateContext")?,
        destroy_context: get(module, c"ffxDestroyContext")?,
        dispatch: get(module, c"ffxDispatch")?,
        _module: module,
    })
}

fn api() -> Result<&'static Api> {
    API.get_or_init(|| match load_api() {
        Ok(a) => Some(a),
        Err(e) => {
            log::warn!("AMD FidelityFX is not available: {e:#}");
            None
        }
    })
    .as_ref()
    .ok_or_else(|| anyhow!("AMD's FidelityFX loader DLL could not be opened"))
}

fn check(what: &str, r: u32) -> Result<()> {
    // (FfxApiReturnCodes; newer DLLs may return codes the headers do not list yet)
    const NAMES: [&str; 8] = ["ok", "error", "unknown description type", "runtime error", "no provider", "out of memory", "invalid parameter", "not supported by this provider"];
    if r == 0 {
        Ok(())
    } else {
        Err(anyhow!("{what}: {} ({r})", NAMES.get(r as usize).copied().unwrap_or("unknown code")))
    }
}

fn ffx_format(f: DXGI_FORMAT) -> u32 {
    match f {
        DXGI_FORMAT_B8G8R8A8_UNORM => FORMAT_B8G8R8A8_UNORM,
        DXGI_FORMAT_B8G8R8A8_UNORM_SRGB => FORMAT_B8G8R8A8_SRGB,
        DXGI_FORMAT_R8G8B8A8_UNORM => FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => FORMAT_R8G8B8A8_SRGB,
        DXGI_FORMAT_R16G16_FLOAT => FORMAT_R16G16_FLOAT,
        DXGI_FORMAT_R8_UNORM => FORMAT_R8_UNORM,
        DXGI_FORMAT_D32_FLOAT | DXGI_FORMAT_R32_FLOAT | DXGI_FORMAT_R32_TYPELESS => FORMAT_R32_FLOAT,
        _ => FORMAT_UNKNOWN,
    }
}

/// A texture as the FidelityFX API describes it (`ffxApiGetResourceDX12` in its headers).
fn resource(t: &wgpu::Texture, state: u32) -> Result<Resource> {
    let hal = unsafe { t.as_hal::<wgpu_hal::api::Dx12>() }.ok_or_else(|| anyhow!("no Direct3D 12 resource under an FSR 3 texture"))?;
    let raw = unsafe { hal.raw_resource() }.clone();
    drop(hal);
    let desc = unsafe { raw.GetDesc() };
    let mut usage = if matches!(desc.Format, DXGI_FORMAT_D32_FLOAT | DXGI_FORMAT_D16_UNORM) { USAGE_DEPTHTARGET } else { 0 };
    if (desc.Flags & D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS).0 != 0 {
        usage |= USAGE_UAV;
    }
    let resource = raw.as_raw();
    // (the resource lives as long as its wgpu texture, which outlives the frame)
    drop(raw);
    Ok(Resource {
        resource,
        description: ResourceDescription { kind: TYPE_TEXTURE2D, format: ffx_format(desc.Format), width: desc.Width as u32, height: desc.Height, depth: desc.DepthOrArraySize as u32, mip_count: desc.MipLevels as u32, flags: 0, usage },
        state,
    })
}

/// AMD's context for one window size, and what it was created from (AMD may keep pointers into
/// it).
struct UpscaleContext {
    context: Box<FfxContext>,
    display: (u32, u32),
    _create: Box<(CreateUpscale, CreateVersion, CreateBackendDx12)>,
}

pub struct Upscaler {
    device: ID3D12Device,
    ring: ListRing,
    context: Option<UpscaleContext>,
}

// (used from the render thread only)
unsafe impl Send for Upscaler {}

impl Upscaler {
    /// FSR 3 upscaling on wgpu's device; the error says what is missing.
    pub fn load(device: &wgpu::Device) -> Result<Upscaler> {
        let dir = game_dir();
        let missing = omsi_cfg::upscaler_files::missing(&dir, "fsr3");
        if !missing.is_empty() {
            bail!("{} not found in {} (they come with AMD's FidelityFX SDK, Kits/FidelityFX/signedbin)", missing.join(", "), dir.display());
        }
        api()?;
        let hal = unsafe { device.as_hal::<wgpu_hal::api::Dx12>() }.ok_or_else(|| anyhow!("the game is not running on DirectX 12 (Settings: Graphics API)"))?;
        let raw = hal.raw_device().clone();
        drop(hal);
        let ring = ListRing::new(&raw).context("FSR 3's command lists")?;
        log::info!("FSR 3 upscaling: AMD FidelityFX loaded from {}", dir.display());
        Ok(Upscaler { device: raw, ring, context: None })
    }

    /// AMD's context for a window of `display` pixels (made anew for a new size).
    fn context_for(&mut self, display: (u32, u32)) -> Result<*mut FfxContext> {
        let api = api()?;
        if self.context.as_ref().is_some_and(|c| c.display != display) {
            // (the old one may still be in a list on the GPU)
            self.ring.wait();
            if let Some(mut c) = self.context.take() {
                unsafe { (api.destroy_context)(&mut *c.context, std::ptr::null()) };
            }
        }
        if self.context.is_none() {
            let size = Dimensions { width: display.0, height: display.1 };
            let mut create = Box::new((
                // (the picture is the finished, tone-mapped one, sRGB-encoded: see `upscale`)
                CreateUpscale { header: Header::new(CREATE_UPSCALE), flags: UPSCALE_DEPTH_INVERTED | UPSCALE_NON_LINEAR_COLORSPACE, max_render_size: size, max_upscale_size: size, message: std::ptr::null() },
                CreateVersion { header: Header::new(CREATE_UPSCALE_VERSION), version: UPSCALER_VERSION },
                CreateBackendDx12 { header: Header::new(CREATE_BACKEND_DX12), device: self.device.as_raw() },
            ));
            create.1.header.next = &create.2.header;
            create.0.header.next = &create.1.header;
            let mut context: Box<FfxContext> = Box::new(std::ptr::null_mut());
            check("creating the FSR 3 upscaling context", unsafe { (api.create_context)(&mut *context, &create.0.header, std::ptr::null()) })?;
            log::info!("FSR 3 upscaling: {}x{}", display.0, display.1);
            self.context = Some(UpscaleContext { context, display, _create: create });
        }
        Ok(&mut *self.context.as_mut().expect("made above").context)
    }

    /// Upscales this frame into `inputs.output`: recorded into a command list of ours and put
    /// on wgpu's queue, after the submit that drew the inputs.
    pub fn upscale(&mut self, queue: &wgpu::Queue, inputs: &Inputs, c: &FrameConstants) -> Result<()> {
        let api = api()?;
        let context = self.context_for(inputs.output_size)?;
        let queue_hal = unsafe { queue.as_hal::<wgpu_hal::api::Dx12>() }.ok_or_else(|| anyhow!("no Direct3D 12 queue under wgpu's"))?;
        let raw_queue = queue_hal.as_raw().clone();
        drop(queue_hal);
        // (the picture read as plain UNORM and FSR left to work on those numbers as they are -
        // the context's non-linear flag - so it hands back the same sRGB-encoded numbers, as
        // DLSS does)
        let mut color = resource(inputs.color, STATE_RENDER_TARGET)?;
        color.description.format = match inputs.color.format().remove_srgb_suffix() {
            wgpu::TextureFormat::Bgra8Unorm => FORMAT_B8G8R8A8_UNORM,
            wgpu::TextureFormat::Rgba8Unorm => FORMAT_R8G8B8A8_UNORM,
            f => bail!("FSR 3 upscaling does not take a picture in {f:?}"),
        };
        let depth = resource(inputs.depth, STATE_DEPTH_ATTACHMENT)?;
        let motion_vectors = resource(inputs.motion, STATE_RENDER_TARGET)?;
        let reactive = resource(inputs.mask, STATE_RENDER_TARGET)?;
        let output = resource(inputs.output, STATE_RENDER_TARGET)?;
        let list = self.ring.begin()?;
        let desc = DispatchUpscale {
            header: Header::new(DISPATCH_UPSCALE),
            command_list: list.as_raw(),
            color,
            depth,
            motion_vectors,
            exposure: Resource::NONE,
            // (glass, its reflections, the rain on it: FSR leans on this frame there instead of
            // the history, see `vs_mask`)
            reactive,
            transparency_and_composition: Resource::NONE,
            output,
            jitter_offset: Coords { x: c.jitter[0], y: c.jitter[1] },
            // (the motion is in pixels already)
            motion_vector_scale: Coords { x: 1.0, y: 1.0 },
            render_size: Dimensions { width: inputs.render_size.0, height: inputs.render_size.1 },
            upscale_size: Dimensions { width: inputs.output_size.0, height: inputs.output_size.1 },
            // (a little of FSR's own sharpening: its upscaled picture is soft otherwise)
            enable_sharpening: inputs.render_size != inputs.output_size,
            sharpness: 0.3,
            frame_time_delta: c.frame_ms,
            pre_exposure: 1.0,
            reset: c.reset,
            camera_near: c.near,
            camera_far: c.far,
            camera_fov_vertical: c.fov,
            view_space_to_meters: 1.0,
            // (not NON_LINEAR_COLOR_SRGB: with it FSR takes the input to linear and hands it
            // back linear, which is then shown as if it were sRGB, the whole picture too dark)
            flags: 0,
        };
        let recorded = check("FSR 3 upscaling", unsafe { (api.dispatch)(context, &desc.header) });
        // (submitted either way: an empty list is harmless and keeps the fence right)
        self.ring.submit(&raw_queue)?;
        recorded
    }
}

impl Drop for Upscaler {
    fn drop(&mut self) {
        self.ring.wait();
        if let (Some(api), Some(mut c)) = (API.get().and_then(|a| a.as_ref()), self.context.take()) {
            unsafe { (api.destroy_context)(&mut *c.context, std::ptr::null()) };
        }
    }
}
