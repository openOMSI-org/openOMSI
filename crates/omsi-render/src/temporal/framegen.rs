// The FidelityFX API structures, constants and identifiers below follow AMD's FidelityFX SDK
// headers (https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK, v2.4.0:
// `ffx_framegeneration.h`, `dx12/ffx_api_framegeneration_dx12.h`), which come under this
// licence:
//
// Copyright (C) 2026 Advanced Micro Devices, Inc.
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

//! Frame generation (DirectX 12): the window's swap chain shows made-up frames between the ones
//! the game draws. With AMD FSR 3 it is AMD's frame-interpolation swap chain, which each upscaled
//! frame hands the depth, motion and camera the upscaler had, and the picture before the HUD went
//! over it; with NVIDIA DLSS it is Streamline's (`dlssg`), which DLSS hands the same.

use super::d3d12::ListRing;
use super::fidelityfx::{api, check, resource, Api, Coords, CreateBackendDx12, CreateVersion, Dimensions, FfxContext, Header, Resource, CREATE_BACKEND_DX12, STATE_DEPTH_ATTACHMENT, STATE_RENDER_TARGET};
use super::{FrameConstants, FrameGenerationKind};
use anyhow::{anyhow, bail, Context, Result};
use omsi_cfg::upscaler_files::Api as Gpu;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use windows::core::Interface;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Rect {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

#[repr(C)]
struct CreateFrameGeneration {
    header: Header,
    flags: u32,
    display_size: Dimensions,
    max_render_size: Dimensions,
    back_buffer_format: u32,
}

#[repr(C)]
struct CreateSwapChainForHwnd {
    header: Header,
    swapchain: *mut *mut c_void,
    hwnd: HWND,
    desc: *mut DXGI_SWAP_CHAIN_DESC1,
    fullscreen_desc: *mut c_void,
    factory: *mut c_void,
    game_queue: *mut c_void,
}

type DispatchCallback = unsafe extern "C" fn(*mut Header, *mut c_void) -> u32;

#[repr(C)]
struct ConfigureFrameGeneration {
    header: Header,
    swapchain: *mut c_void,
    present_callback: Option<DispatchCallback>,
    present_callback_context: *mut c_void,
    frame_generation_callback: Option<DispatchCallback>,
    frame_generation_callback_context: *mut c_void,
    enabled: bool,
    allow_async_workloads: bool,
    hudless_color: Resource,
    flags: u32,
    only_present_generated: bool,
    generation_rect: Rect,
    frame_id: u64,
}

#[repr(C)]
struct DispatchPrepare {
    header: Header,
    frame_id: u64,
    flags: u32,
    command_list: *mut c_void,
    render_size: Dimensions,
    jitter_offset: Coords,
    motion_vector_scale: Coords,
    frame_time_delta: f32,
    reset: bool,
    camera_near: f32,
    camera_far: f32,
    camera_fov_vertical: f32,
    view_space_to_meters: f32,
    depth: Resource,
    motion_vectors: Resource,
    camera_position: [f32; 3],
    camera_up: [f32; 3],
    camera_right: [f32; 3],
    camera_forward: [f32; 3],
}

// (sizes the SDK's headers give on x64: a field out of place fails the build here)
const _: () = {
    assert!(std::mem::size_of::<CreateFrameGeneration>() == 40);
    assert!(std::mem::size_of::<CreateSwapChainForHwnd>() == 64);
    assert!(std::mem::size_of::<ConfigureFrameGeneration>() == 144);
    assert!(std::mem::offset_of!(ConfigureFrameGeneration, hudless_color) == 64);
    assert!(std::mem::size_of::<QueryMemory>() == 24 && std::mem::size_of::<MemoryUsage>() == 16);
    assert!(std::mem::offset_of!(ConfigureFrameGeneration, frame_id) == 136);
    assert!(std::mem::size_of::<DispatchPrepare>() == 232);
    assert!(std::mem::offset_of!(DispatchPrepare, depth) == 88);
    assert!(std::mem::offset_of!(DispatchPrepare, camera_forward) == 220);
};

const CREATE_FRAMEGENERATION: u64 = 0x0002_0001;
const CONFIGURE_FRAMEGENERATION: u64 = 0x0002_0002;
const DISPATCH_PREPARE_V2: u64 = 0x0002_000c;
const CREATE_FRAMEGENERATION_VERSION: u64 = 0x0002_000e;
const CREATE_SWAPCHAIN_FOR_HWND_DX12: u64 = 0x0003_0006;
const CREATE_SWAPCHAIN_VERSION_DX12: u64 = 0x0003_000b;
const QUERY_FRAMEGENERATION_MEMORY: u64 = 0x0002_0007;
const QUERY_SWAPCHAIN_MEMORY_DX12: u64 = 0x0003_0009;
/// Frames frame generation has run before what AMD's context and swap chain hold is logged:
/// some of it is made only once it works.
const MEMORY_LOGGED_AFTER: u32 = 60;
const FRAMEGENERATION_VERSION: u32 = (4 << 22) | 2;
const SWAPCHAIN_DX12_VERSION: u32 = 4 << 22;
const FLAG_DEPTH_INVERTED: u32 = 1 << 3;
const FORMAT_R8G8B8A8_UNORM: u32 = 10;
const FORMAT_B8G8R8A8_UNORM: u32 = 14;

/// The swap chain's buffers: two frames in flight and the one on screen, as wgpu's surface has.
const BUFFERS: u32 = 3;

fn dxgi_format(f: wgpu::TextureFormat) -> Option<(DXGI_FORMAT, u32)> {
    use wgpu::TextureFormat as F;
    Some(match f {
        F::Bgra8Unorm | F::Bgra8UnormSrgb => (DXGI_FORMAT_B8G8R8A8_UNORM, FORMAT_B8G8R8A8_UNORM),
        F::Rgba8Unorm | F::Rgba8UnormSrgb => (DXGI_FORMAT_R8G8B8A8_UNORM, FORMAT_R8G8B8A8_UNORM),
        _ => return None,
    })
}

/// `FfxApiEffectMemoryUsage`.
#[repr(C)]
#[derive(Default)]
struct MemoryUsage {
    total: u64,
    _aliasable: u64,
}

/// The frame generation context's and the swap chain's memory query: one pointer to fill.
#[repr(C)]
struct QueryMemory {
    header: Header,
    usage: *mut MemoryUsage,
}

/// AMD's frame generation for the size the window has now; made again when that changes.
struct Generator {
    context: Box<FfxContext>,
    display: (u32, u32),
    _create: Box<(CreateFrameGeneration, CreateVersion, CreateBackendDx12)>,
}

/// What the window's swap chain and the renderer share: the frame generation context, and
/// whether this frame gave it anything to work with.
struct Shared {
    api: &'static Api,
    device: ID3D12Device,
    queue: ID3D12CommandQueue,
    swapchain: IDXGISwapChain4,
    /// The swap chain's own context, owned by `AmdSwapChain`: asked what it holds.
    swapchain_context: FfxContext,
    format: u32,
    ring: ListRing,
    generator: Option<Generator>,
    /// Frames since the context was made, until its video memory is logged.
    since_made: u32,
    frame_id: u64,
    prepared: bool,
    enabled: bool,
    failed: bool,
}

// (COM objects used from the render thread only, behind the mutex)
unsafe impl Send for Shared {}

/// The renderer's handle on the window's frame generation (`Renderer::set_frame_generation`).
#[derive(Clone)]
pub struct FrameGeneration(Generation, Arc<Mutex<Held>>);

/// Textures the renderer let go of that frame generation may still read after a present, on the
/// vendor's own queues, which `wait_gpu` does not wait for (`FrameGeneration::hold`).
#[derive(Default)]
struct Held {
    textures: Vec<wgpu::Texture>,
    /// Presents to go before they are let go of.
    presents: u32,
}

/// Presents after which nothing of an earlier frame is read any more (the vendors queue a few).
const HELD_PRESENTS: u32 = 8;

#[derive(Clone)]
enum Generation {
    /// AMD FSR 3's, handed each frame's inputs by `prepare`.
    Amd(Arc<Mutex<Shared>>),
    /// NVIDIA DLSS's, handed them by DLSS itself (`streamline::Runtime::evaluate`).
    Dlss,
}

/// The in-between frames made since the start (`FrameGeneration::generated`), by either.
pub(super) static GENERATED: AtomicU64 = AtomicU64::new(0);

/// The frame-interpolation swap chain's dispatch, asked for by the swap chain for each frame
/// it makes; the context comes back as the user pointer.
unsafe extern "C" fn dispatch(params: *mut Header, context: *mut c_void) -> u32 {
    match api(Gpu::Dx12) {
        Ok(api) => {
            let result = unsafe { (api.dispatch)(context.cast(), params) };
            if result == 0 {
                GENERATED.fetch_add(1, Ordering::Relaxed);
            }
            result
        }
        Err(_) => 1,
    }
}

impl Shared {
    fn configure(&mut self, enabled: bool, hudless: Resource) -> Result<()> {
        let Some(g) = self.generator.as_mut() else { return Ok(()) };
        let desc = ConfigureFrameGeneration {
            header: Header::new(CONFIGURE_FRAMEGENERATION),
            swapchain: self.swapchain.as_raw(),
            present_callback: None,
            present_callback_context: std::ptr::null_mut(),
            frame_generation_callback: Some(dispatch),
            frame_generation_callback_context: (&mut *g.context as *mut FfxContext).cast(),
            enabled,
            allow_async_workloads: false,
            hudless_color: hudless,
            flags: 0,
            only_present_generated: false,
            generation_rect: Rect { left: 0, top: 0, width: g.display.0 as i32, height: g.display.1 as i32 },
            frame_id: self.frame_id,
        };
        self.enabled = enabled;
        check("configuring frame generation", unsafe { (self.api.configure)(&mut *g.context, &desc.header) })
    }

    fn generator_for(&mut self, display: (u32, u32)) -> Result<()> {
        if self.generator.as_ref().is_some_and(|g| g.display == display) {
            return Ok(());
        }
        self.drop_generator();
        let size = Dimensions { width: display.0, height: display.1 };
        let mut create = Box::new((
            CreateFrameGeneration { header: Header::new(CREATE_FRAMEGENERATION), flags: FLAG_DEPTH_INVERTED, display_size: size, max_render_size: size, back_buffer_format: self.format },
            CreateVersion { header: Header::new(CREATE_FRAMEGENERATION_VERSION), version: FRAMEGENERATION_VERSION },
            CreateBackendDx12 { header: Header::new(CREATE_BACKEND_DX12), device: self.device.as_raw() },
        ));
        create.1.header.next = &create.2.header;
        create.0.header.next = &create.1.header;
        let mut context: Box<FfxContext> = Box::new(std::ptr::null_mut());
        check("creating the frame generation context", unsafe { (self.api.create_context)(&mut *context, &create.0.header, std::ptr::null()) })?;
        self.generator = Some(Generator { context, display, _create: create });
        self.since_made = 0;
        log::info!("frame generation: AMD FSR 3 for {}x{}", display.0, display.1);
        Ok(())
    }

    fn drop_generator(&mut self) {
        if self.generator.is_none() {
            return;
        }
        // (switched off first, which waits for the swap chain's own work with it)
        let _ = self.configure(false, Resource::NONE);
        self.ring.wait();
        if let Some(mut g) = self.generator.take() {
            let _ = unsafe { (self.api.destroy_context)(&mut *g.context, std::ptr::null()) };
        }
    }

    fn prepare(&mut self, depth: &wgpu::Texture, motion: &wgpu::Texture, hudless: &wgpu::Texture, render: (u32, u32), c: &FrameConstants) -> Result<()> {
        let display = (hudless.width(), hudless.height());
        self.generator_for(display)?;
        let reset = c.reset || !self.enabled;
        self.configure(true, resource(hudless, STATE_RENDER_TARGET)?)?;
        let list = self.ring.begin()?;
        let g = self.generator.as_mut().expect("made above");
        let desc = DispatchPrepare {
            header: Header::new(DISPATCH_PREPARE_V2),
            frame_id: self.frame_id,
            flags: 0,
            command_list: list.as_raw(),
            render_size: Dimensions { width: render.0, height: render.1 },
            jitter_offset: Coords { x: c.jitter[0], y: c.jitter[1] },
            // (the motion is in pixels already)
            motion_vector_scale: Coords { x: 1.0, y: 1.0 },
            frame_time_delta: c.frame_ms,
            reset,
            camera_near: c.near,
            camera_far: c.far,
            camera_fov_vertical: c.fov,
            view_space_to_meters: 1.0,
            depth: resource(depth, STATE_DEPTH_ATTACHMENT)?,
            motion_vectors: resource(motion, STATE_RENDER_TARGET)?,
            camera_position: c.camera_pos,
            camera_up: c.camera_up,
            camera_right: c.camera_right,
            camera_forward: c.camera_fwd,
        };
        let recorded = check("frame generation's prepare pass", unsafe { (self.api.dispatch)(&mut *g.context, &desc.header) });
        // (submitted either way: the ring's fence then still says when the list is free)
        self.ring.submit(&self.queue)?;
        recorded?;
        self.frame_id += 1;
        self.prepared = true;
        self.since_made += 1;
        if self.since_made == MEMORY_LOGGED_AFTER {
            self.log_memory();
        }
        Ok(())
    }

    /// The video memory AMD's frame generation and its swap chain hold, as they say.
    fn log_memory(&mut self) {
        let Some(g) = self.generator.as_mut() else { return };
        let mut ask = |context: *mut FfxContext, kind: u64| -> Option<u64> {
            let mut usage = MemoryUsage::default();
            let mut q = QueryMemory { header: Header::new(kind), usage: &mut usage };
            (unsafe { (self.api.query)(context, &mut q.header) } == 0).then_some(usage.total)
        };
        let generation = ask(&mut *g.context, QUERY_FRAMEGENERATION_MEMORY);
        let mut swapchain_context = self.swapchain_context;
        let swapchain = ask(&mut swapchain_context, QUERY_SWAPCHAIN_MEMORY_DX12);
        let mb = |b: Option<u64>| b.map_or_else(|| "?".to_string(), |b| format!("{:.0}", b as f64 / 1_048_576.0));
        log::info!("frame generation: AMD's video memory {} MB for frame generation, {} MB for its swap chain", mb(generation), mb(swapchain));
    }
}

impl FrameGeneration {
    /// This frame's inputs, after the scene's and the HUD's submits and before the window's
    /// picture is presented. `hudless` is the window's picture before the HUD went over it.
    pub(crate) fn prepare(&self, depth: &wgpu::Texture, motion: &wgpu::Texture, hudless: &wgpu::Texture, render: (u32, u32), c: &FrameConstants) {
        let Generation::Amd(s) = &self.0 else { return };
        let mut s = s.lock().unwrap_or_else(|e| e.into_inner());
        if s.failed {
            return;
        }
        if let Err(e) = s.prepare(depth, motion, hudless, render, c) {
            log::error!("frame generation stopped working ({e:#}); the frames are shown without it from now on");
            s.failed = true;
            s.drop_generator();
        }
    }

    /// Before the renderer drops the textures frame generation reads (`rebuild_in_place`): off,
    /// and the work it had queued with them done, until the next upscaled frame.
    pub(crate) fn pause(&self) {
        match &self.0 {
            Generation::Amd(s) => {
                let mut s = s.lock().unwrap_or_else(|e| e.into_inner());
                if s.enabled {
                    let _ = s.configure(false, Resource::NONE);
                }
                s.ring.wait();
                s.prepared = false;
            }
            Generation::Dlss => super::dlssg::pause(),
        }
    }

    /// The renderer lets go of `textures` (a rebuild) that frame generation may still read after
    /// a present: kept for some more presents, or until the window's swap chain is gone.
    pub(crate) fn hold(&self, textures: Vec<wgpu::Texture>) {
        let mut h = self.1.lock().unwrap_or_else(|e| e.into_inner());
        h.textures.extend(textures);
        h.presents = HELD_PRESENTS;
    }

    fn presented(&self) {
        let mut h = self.1.lock().unwrap_or_else(|e| e.into_inner());
        if h.presents > 0 {
            h.presents -= 1;
            if h.presents == 0 {
                h.textures.clear();
            }
        }
    }

    /// Whether the window takes the picture before the HUD this frame.
    pub(crate) fn working(&self) -> bool {
        match &self.0 {
            Generation::Amd(s) => !s.lock().unwrap_or_else(|e| e.into_inner()).failed,
            Generation::Dlss => super::dlssg::working(),
        }
    }

    /// The in-between frames made since the start.
    pub(crate) fn generated(&self) -> u64 {
        GENERATED.load(Ordering::Relaxed)
    }

    /// Right before the window's picture is presented: a frame drawn without the upscaler
    /// (a menu, the loading screen) has nothing to make an in-between frame from.
    fn before_present(&self) {
        let Generation::Amd(s) = &self.0 else { return };
        let mut s = s.lock().unwrap_or_else(|e| e.into_inner());
        if !s.prepared && s.enabled {
            let _ = s.configure(false, Resource::NONE);
        }
        s.prepared = false;
    }
}

/// AMD's swap chain context, which the swap chain goes with.
struct AmdSwapChain {
    api: &'static Api,
    context: Box<FfxContext>,
    /// (AMD may keep pointers into what the context was created from)
    _create: Box<(CreateVersion, DXGI_SWAP_CHAIN_DESC1, *mut c_void)>,
}

/// The window's swap chain while frame generation is on: AMD's or Streamline's, drawn into
/// through wgpu textures wrapping its buffers.
pub struct OwnSwapChain {
    generation: FrameGeneration,
    swapchain: IDXGISwapChain4,
    amd: Option<AmdSwapChain>,
    buffers: Vec<wgpu::Texture>,
    format: wgpu::TextureFormat,
    size: (u32, u32),
    vsync: bool,
    tearing: bool,
}

fn wrap(device: &wgpu::Device, swapchain: &IDXGISwapChain4, format: wgpu::TextureFormat, (width, height): (u32, u32)) -> Result<Vec<wgpu::Texture>> {
    let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
    (0..BUFFERS)
        .map(|i| {
            let buffer: ID3D12Resource = unsafe { swapchain.GetBuffer(i) }.with_context(|| format!("the swap chain's buffer {i}"))?;
            let hal = unsafe { wgpu_hal::dx12::Device::texture_from_raw(buffer, format, wgpu::TextureDimension::D2, size, 1, 1) };
            let desc = wgpu::TextureDescriptor {
                label: Some("frame generation swap chain buffer"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            };
            // (a swap chain's buffer starts in the present state)
            Ok(unsafe { device.create_texture_from_hal_with_uses::<wgpu_hal::api::Dx12>(hal, &desc, wgpu::TextureUses::PRESENT) })
        })
        .collect()
}

impl OwnSwapChain {
    /// Frame generation's swap chain for `hwnd`, on wgpu's queue: AMD's for FSR 3, Streamline's
    /// for DLSS. Fails without the vendor's DLLs, off DirectX 12, on a card that cannot, or for
    /// a format frame generation does not take.
    pub fn new(device: &wgpu::Device, hwnd: HWND, format: wgpu::TextureFormat, size: (u32, u32), vsync: bool, kind: FrameGenerationKind) -> Result<OwnSwapChain> {
        let size = (size.0.max(1), size.1.max(1));
        let (dxgi, ffx) = dxgi_format(format).ok_or_else(|| anyhow!("frame generation takes no {format:?} window"))?;
        let (raw_device, queue) = {
            let hal = unsafe { device.as_hal::<wgpu_hal::api::Dx12>() }.ok_or_else(|| anyhow!("the game is not on DirectX 12"))?;
            (hal.raw_device().clone(), hal.raw_queue().clone())
        };
        let factory: IDXGIFactory5 = unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)) }.context("the DXGI factory")?;
        let tearing = {
            let mut allow = windows::core::BOOL::default();
            unsafe { factory.CheckFeatureSupport(DXGI_FEATURE_PRESENT_ALLOW_TEARING, &mut allow as *mut _ as *mut c_void, std::mem::size_of_val(&allow) as u32) }.is_ok() && allow.as_bool()
        };
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: size.0,
            Height: size.1,
            Format: dxgi,
            Stereo: false.into(),
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: BUFFERS,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            AlphaMode: DXGI_ALPHA_MODE_UNSPECIFIED,
            Flags: if tearing { DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING.0 as u32 } else { 0 },
        };
        let (swapchain, generation, amd) = match kind {
            FrameGenerationKind::Fsr3 => {
                let api = api(Gpu::Dx12)?;
                if !omsi_cfg::upscaler_files::frame_generation_available(&super::d3d12::game_dir()) {
                    bail!("AMD's frame generation DLLs are not beside the game");
                }
                let mut create = Box::new((CreateVersion { header: Header::new(CREATE_SWAPCHAIN_VERSION_DX12), version: SWAPCHAIN_DX12_VERSION }, desc, std::ptr::null_mut::<c_void>()));
                let make = CreateSwapChainForHwnd {
                    header: Header { kind: CREATE_SWAPCHAIN_FOR_HWND_DX12, next: &create.0.header },
                    swapchain: &mut create.2,
                    hwnd,
                    desc: &mut create.1,
                    fullscreen_desc: std::ptr::null_mut(),
                    factory: factory.as_raw(),
                    game_queue: queue.as_raw(),
                };
                let mut context: Box<FfxContext> = Box::new(std::ptr::null_mut());
                check("creating AMD's frame-interpolation swap chain", unsafe { (api.create_context)(&mut *context, &make.header, std::ptr::null()) })?;
                let amd = AmdSwapChain { api, context, _create: create };
                if amd._create.2.is_null() {
                    OwnSwapChain::destroy_amd(amd);
                    bail!("AMD's FidelityFX API made no swap chain");
                }
                // (the swap chain's reference is ours now; the context lets go of its own on destroy)
                let swapchain = unsafe { IDXGISwapChain4::from_raw(amd._create.2) };
                let ring = match ListRing::new(&raw_device) {
                    Ok(r) => r,
                    Err(e) => {
                        OwnSwapChain::destroy_amd(amd);
                        return Err(e.context("frame generation's command lists"));
                    }
                };
                let shared = Shared { api, device: raw_device, queue, swapchain: swapchain.clone(), swapchain_context: *amd.context, format: ffx, ring, generator: None, since_made: 0, frame_id: 0, prepared: false, enabled: false, failed: false };
                (swapchain, Generation::Amd(Arc::new(Mutex::new(shared))), Some(amd))
            }
            FrameGenerationKind::Dlss(frames) => {
                let made = (|| -> Result<IDXGISwapChain4> {
                    let proxy = super::dlssg::proxy_factory(&raw_device, &factory, frames)?;
                    let made: IDXGISwapChain1 = unsafe { proxy.CreateSwapChainForHwnd(&queue, hwnd, &desc, None, None) }.context("creating Streamline's swap chain")?;
                    made.cast().context("Streamline's swap chain")
                })();
                match made {
                    Ok(s) => (s, Generation::Dlss, None),
                    Err(e) => {
                        super::dlssg::abandon();
                        return Err(e);
                    }
                }
            }
        };
        // (winit does the full screen; DXGI's Alt+Enter would fight it)
        let _ = unsafe { factory.MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER | DXGI_MWA_NO_WINDOW_CHANGES) };
        let mut own = OwnSwapChain { generation: FrameGeneration(generation, Default::default()), swapchain, amd, buffers: Vec::new(), format, size, vsync, tearing };
        // (dropped on failure, which takes down what was made above)
        own.buffers = wrap(device, &own.swapchain, format, size)?;
        if matches!(kind, FrameGenerationKind::Dlss(_)) {
            super::dlssg::set_window(true);
        }
        log::info!("frame generation: {} swap chain for the window, {}x{}, tearing {tearing}", if own.amd.is_some() { "AMD's" } else { "Streamline's" }, size.0, size.1);
        Ok(own)
    }

    fn destroy_amd(mut amd: AmdSwapChain) {
        let _ = unsafe { (amd.api.destroy_context)(&mut *amd.context, std::ptr::null()) };
    }

    pub fn frame_generation(&self) -> FrameGeneration {
        self.generation.clone()
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// The buffer this frame is drawn into.
    pub fn current(&self) -> Option<wgpu::Texture> {
        // (Streamline's frame generation checks this is asked every frame)
        let i = unsafe { self.swapchain.GetCurrentBackBufferIndex() } as usize;
        self.buffers.get(i).cloned()
    }

    /// Shows `buffer`, drawn this frame: back into the present state, then `Present`.
    pub fn present(&self, device: &wgpu::Device, queue: &wgpu::Queue, buffer: &wgpu::Texture) {
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame generation present") });
        enc.transition_resources(std::iter::empty(), std::iter::once(wgpu::TextureTransition { texture: buffer, selector: None, state: wgpu::TextureUses::PRESENT }));
        queue.submit([enc.finish()]);
        let (interval, flags) = match (self.vsync, self.tearing) {
            (true, _) => (1, DXGI_PRESENT(0)),
            (false, true) => (0, DXGI_PRESENT_ALLOW_TEARING),
            (false, false) => (0, DXGI_PRESENT(0)),
        };
        let present = || {
            if let Err(e) = unsafe { self.swapchain.Present(interval, flags) }.ok() {
                log::warn!("frame generation: Present failed ({e})");
            }
        };
        match &self.generation.0 {
            Generation::Amd(_) => {
                self.generation.before_present();
                present();
            }
            Generation::Dlss => super::dlssg::present(present),
        }
        self.generation.presented();
    }

    /// New buffers for a window of `size`: the old ones go first and the GPU is waited for, as
    /// `ResizeBuffers` fails while anything still holds a buffer.
    pub fn resize(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        let size = (size.0.max(1), size.1.max(1));
        if size == self.size {
            return;
        }
        for b in self.buffers.drain(..) {
            b.destroy();
        }
        let _ = crate::wait_gpu(device, None);
        let flags = if self.tearing { DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING.0 } else { 0 };
        let resized = unsafe { self.swapchain.ResizeBuffers(BUFFERS, size.0, size.1, DXGI_FORMAT_UNKNOWN, DXGI_SWAP_CHAIN_FLAG(flags)) };
        if let Err(e) = &resized {
            log::error!("frame generation: ResizeBuffers to {}x{} failed ({e}); the old size stays", size.0, size.1);
        } else {
            self.size = size;
        }
        match wrap(device, &self.swapchain, self.format, self.size) {
            Ok(b) => self.buffers = b,
            Err(e) => log::error!("frame generation: the swap chain's buffers: {e:#}"),
        }
    }

    pub fn set_vsync(&mut self, vsync: bool) {
        self.vsync = vsync;
    }

    /// Another count of NVIDIA's frames on screen per drawn one, on this swap chain (false: the
    /// swap chain is AMD's, made again for another kind of frame generation).
    pub fn ask_frames(&self, frames: u32) -> bool {
        match &self.generation.0 {
            Generation::Dlss => {
                super::dlssg::ask(frames);
                true
            }
            Generation::Amd(_) => false,
        }
    }
}

impl Drop for OwnSwapChain {
    fn drop(&mut self) {
        match &self.generation.0 {
            Generation::Amd(s) => {
                let mut s = s.lock().unwrap_or_else(|e| e.into_inner());
                s.drop_generator();
                s.failed = true;
            }
            Generation::Dlss => super::dlssg::set_window(false),
        }
        // (destroyed, not only let go of: the window takes another swap chain only once nothing
        // holds this one's buffers)
        for b in self.buffers.drain(..) {
            b.destroy();
        }
        if let Some(amd) = self.amd.take() {
            OwnSwapChain::destroy_amd(amd);
        }
        // (AMD's swap chain waited for its queues as it went; Streamline's is let go of with it)
        self.generation.1.lock().unwrap_or_else(|e| e.into_inner()).textures.clear();
    }
}
