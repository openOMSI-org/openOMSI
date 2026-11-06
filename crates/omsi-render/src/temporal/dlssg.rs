// The Streamline structures, constants and identifiers below are transcribed from NVIDIA's
// Streamline SDK headers (https://github.com/NVIDIA-RTX/Streamline, v2.14.1: `sl_dlss_g.h`,
// `sl_reflex.h`, `sl_pcl.h`), which come under this licence:
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

//! NVIDIA DLSS frame generation (DLSS-G) by way of Streamline: DirectX 12, a GeForce RTX 40 or
//! 50, with DLSS upscaling. The window's swap chain is Streamline's (made through its proxy of
//! the DXGI factory), which puts the generated frames in when the window presents.
//!
//! Streamline is started here, with DLSS, DLSS-G, Reflex and PCL, when the window's swap chain is
//! made for it; DLSS then joins this session (`streamline::Runtime::load`) and tags, frame by
//! frame, what frame generation reads at the present: the depth, the motion and the picture
//! before the HUD. The session is shut down once the window lets go of the swap chain and DLSS
//! has left it (`abandon`). Reflex runs because DLSS-G requires it; its markers are set around
//! the upscaler, so it does not shorten the input lag.

use super::d3d12::game_dir;
use super::framegen::GENERATED;
use super::streamline::{check, feature_fn, init, load_fn, open_interposer, result_name, AdapterInfo, Base, StructType, ViewportHandle, ADAPTER_INFO, FEATURE_DLSS, FnIsFeatureSupported, FnSetD3DDevice, FnUpgradeInterface, VIEWPORT_HANDLE};
use anyhow::{bail, Context, Result};
use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use windows::core::Interface;
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::Graphics::Direct3D12::ID3D12Device;
use windows::Win32::Graphics::Dxgi::{IDXGIFactory2, IDXGIFactory5};

const DLSSG_OPTIONS: StructType = StructType { data1: 0xfac5f1cb, data2: 0x2dfd, data3: 0x4f36, data4: [0xa1, 0xe6, 0x3a, 0x9e, 0x86, 0x52, 0x56, 0xc5] };
const DLSSG_STATE: StructType = StructType { data1: 0xcc8ac8e1, data2: 0xa179, data3: 0x44f5, data4: [0x97, 0xfa, 0xe7, 0x41, 0x12, 0xf9, 0xbc, 0x61] };
const REFLEX_OPTIONS: StructType = StructType { data1: 0xf03af81a, data2: 0x6d0b, data3: 0x4902, data4: [0xa6, 0x51, 0xc4, 0x96, 0x5e, 0x21, 0x54, 0x34] };

const FEATURE_REFLEX: u32 = 3;
const FEATURE_PCL: u32 = 4;
const FEATURE_DLSS_G: u32 = 1000;
// sl::PCLMarker
const MARKER_SIMULATION_START: u32 = 0;
const MARKER_SIMULATION_END: u32 = 1;
const MARKER_RENDER_SUBMIT_START: u32 = 2;
const MARKER_RENDER_SUBMIT_END: u32 = 3;
const MARKER_PRESENT_START: u32 = 4;
const MARKER_PRESENT_END: u32 = 5;
// sl::DLSSGMode, sl::DLSSGFlags::eRetainResourcesWhenOff
const MODE_OFF: u32 = 0;
const MODE_ON: u32 = 1;
const RETAIN_RESOURCES_WHEN_OFF: u32 = 1 << 3;
// sl::ReflexMode::eLowLatency (DLSS-G does not run without Reflex)
const REFLEX_LOW_LATENCY: u32 = 1;
// sl::DLSSGStatus::eFailReflexNotDetectedAtRuntime
const STATUS_NO_REFLEX: u32 = 1 << 1;
/// `sl::Boolean::eInvalid`, the header's value for its reserved field.
const BOOLEAN_INVALID: u8 = 2;

/// `sl::DLSSGOptions` (version 5).
#[repr(C)]
struct Options {
    base: Base,
    mode: u32,
    num_frames_to_generate: u32,
    flags: u32,
    dynamic_res_width: u32,
    dynamic_res_height: u32,
    num_back_buffers: u32,
    mvec_depth_width: u32,
    mvec_depth_height: u32,
    color_width: u32,
    color_height: u32,
    color_buffer_format: u32,
    mvec_buffer_format: u32,
    depth_buffer_format: u32,
    hud_less_buffer_format: u32,
    ui_buffer_format: u32,
    on_error_callback: *const c_void,
    b_reserved15: u8,
    queue_parallelism_mode: u32,
    enable_user_interface_recomposition: u8,
    dynamic_target_frame_rate: f32,
}

/// `sl::DLSSGState` (version 4).
#[repr(C)]
struct State {
    base: Base,
    estimated_vram_usage_in_bytes: u64,
    status: u32,
    min_width_or_height: u32,
    num_frames_actually_presented: u32,
    num_frames_to_generate_max: u32,
    b_reserved4: u8,
    is_vsync_support_available: u8,
    inputs_processing_completion_fence: *mut c_void,
    last_present_inputs_processing_completion_fence_value: u64,
    is_dynamic_mfg_supported: u8,
}

/// `sl::ReflexOptions` (version 1).
#[repr(C)]
struct ReflexOptions {
    base: Base,
    mode: u32,
    frame_limit_us: u32,
    use_markers_to_optimize: bool,
    virtual_key: u16,
    id_thread: u32,
}

// (the sizes Streamline's own `sl::test::AbiValidation` expects)
const _: () = {
    assert!(std::mem::size_of::<Options>() == 120);
    assert!(std::mem::offset_of!(Options, on_error_callback) == 96);
    assert!(std::mem::offset_of!(Options, dynamic_target_frame_rate) == 116);
    assert!(std::mem::size_of::<State>() == 88);
    assert!(std::mem::offset_of!(State, inputs_processing_completion_fence) == 64);
    assert!(std::mem::size_of::<ReflexOptions>() == 48);
};

type FnSetOptions = unsafe extern "C" fn(*const ViewportHandle, *const Options) -> i32;
type FnGetState = unsafe extern "C" fn(*const ViewportHandle, *mut State, *const Options) -> i32;
type FnReflexSetOptions = unsafe extern "C" fn(*const ReflexOptions) -> i32;
/// (`slPCLSetMarker(PCLMarker, const FrameToken&)`: the token goes by its pointer)
type FnSetMarker = unsafe extern "C" fn(u32, *const c_void) -> i32;

/// Streamline started for frame generation, and what it is doing this frame.
struct Session {
    module: HMODULE,
    set_options: FnSetOptions,
    get_state: FnGetState,
    set_marker: FnSetMarker,
    /// (kept alive: `slInit` was given pointers into them)
    _paths: (Vec<u16>, Vec<*const u16>),
    viewport: ViewportHandle,
    /// Frames on screen per drawn one the settings ask for (2 to 4).
    asked: u32,
    /// The most this card makes, once Streamline has said (0 before).
    most: u32,
    /// What `slDLSSGSetOptions` was last given: frames on screen per drawn one (1: off) and
    /// the window's size.
    set: Option<(u32, (u32, u32))>,
    /// This frame's token, between DLSS's evaluate and the present.
    token: *const c_void,
    /// DLSS tagged this frame for frame generation.
    tagged: bool,
    /// Frame generation stopped working (said once); the frames go out without it.
    failed: bool,
    /// The window's swap chain is Streamline's right now.
    window: bool,
    /// DLSS joined the session (`module`): it cannot be shut down under it.
    joined: bool,
    shutdown: unsafe extern "C" fn() -> i32,
}

// (raw handles only; Streamline is called from the render thread)
unsafe impl Send for Session {}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

fn session() -> std::sync::MutexGuard<'static, Option<Session>> {
    SESSION.lock().unwrap_or_else(|e| e.into_inner())
}

/// What DLSS joins instead of starting Streamline itself (`streamline::Runtime::load`).
pub(super) fn module() -> Option<HMODULE> {
    session().as_mut().map(|s| {
        s.joined = true;
        s.module
    })
}

/// Streamline started for the window and nothing holds it (its swap chain could not be made, or the
/// window let go of it and DLSS left it): shut down, so DLSS starts its own.
pub(super) fn abandon() {
    let mut s = session();
    if s.as_ref().is_some_and(|s| !s.joined && !s.window) {
        if let Some(gone) = s.take() {
            unsafe {
                (gone.shutdown)();
                let _ = FreeLibrary(gone.module);
            }
        }
    }
}

/// Streamline with DLSS, DLSS-G, Reflex and PCL on wgpu's device, asking for `frames` on screen
/// per drawn one; started once, the first time the window asks for it.
fn start(device: &ID3D12Device, frames: u32) -> Result<HMODULE> {
    let mut s = session();
    if let Some(s) = s.as_mut() {
        s.asked = frames;
        return Ok(s.module);
    }
    let dir = game_dir();
    if !omsi_cfg::upscaler_files::dlss_frame_generation_available(&dir) {
        bail!("NVIDIA's frame generation DLLs are not beside the game (Launcher: Settings, Graphics, NVIDIA DLSS)");
    }
    let module = open_interposer(&dir)?;
    let started = (|| -> Result<Session> {
        static FEATURES: [u32; 4] = [FEATURE_DLSS, FEATURE_DLSS_G, FEATURE_REFLEX, FEATURE_PCL];
        let paths = init(module, &dir, &FEATURES, 1)?;
        let shutdown: unsafe extern "C" fn() -> i32 = load_fn(module, c"slShutdown")?;
        (|| -> Result<Session> {
            let set_device: FnSetD3DDevice = load_fn(module, c"slSetD3DDevice")?;
            let is_supported: FnIsFeatureSupported = load_fn(module, c"slIsFeatureSupported")?;
            check("slSetD3DDevice", unsafe { set_device(device.as_raw()) })?;
            let mut luid = unsafe { device.GetAdapterLuid() };
            let adapter = AdapterInfo {
                base: Base::new(ADAPTER_INFO, 1),
                device_luid: (&mut luid as *mut windows::Win32::Foundation::LUID).cast(),
                device_luid_size_in_bytes: std::mem::size_of::<windows::Win32::Foundation::LUID>() as u32,
                vk_physical_device: std::ptr::null_mut(),
            };
            let r = unsafe { is_supported(FEATURE_DLSS_G, &adapter) };
            if r != 0 {
                let why = match result_name(r) {
                    "eErrorOSDisabledHWS" => ": turn on hardware-accelerated GPU scheduling (Windows Settings, System, Display, Graphics) and restart Windows",
                    "eErrorDriverOutOfDate" => ": the NVIDIA driver is too old",
                    "eErrorOSOutOfDate" => ": Windows is too old",
                    "eErrorNoSupportedAdapterFound" | "eErrorAdapterNotSupported" => ": it takes a GeForce RTX 40 or 50",
                    _ => "",
                };
                bail!("this card or system cannot run DLSS frame generation ({}){why}", result_name(r));
            }
            let reflex: FnReflexSetOptions = unsafe { std::mem::transmute(feature_fn(module, FEATURE_REFLEX, c"slReflexSetOptions")?) };
            let options = ReflexOptions { base: Base::new(REFLEX_OPTIONS, 1), mode: REFLEX_LOW_LATENCY, frame_limit_us: 0, use_markers_to_optimize: false, virtual_key: 0, id_thread: 0 };
            check("slReflexSetOptions", unsafe { reflex(&options) })?;
            Ok(Session {
                module,
                set_options: unsafe { std::mem::transmute(feature_fn(module, FEATURE_DLSS_G, c"slDLSSGSetOptions")?) },
                get_state: unsafe { std::mem::transmute(feature_fn(module, FEATURE_DLSS_G, c"slDLSSGGetState")?) },
                set_marker: unsafe { std::mem::transmute(feature_fn(module, FEATURE_PCL, c"slPCLSetMarker")?) },
                _paths: paths,
                viewport: ViewportHandle { base: Base::new(VIEWPORT_HANDLE, 1), value: 0 },
                asked: frames,
                most: 0,
                set: None,
                token: std::ptr::null(),
                tagged: false,
                failed: false,
                window: false,
                joined: false,
                shutdown,
            })
        })()
        .inspect_err(|_| unsafe {
            shutdown();
        })
    })();
    match started {
        Ok(session) => {
            log::info!("frame generation: Streamline with DLSS-G, Reflex and PCL loaded from {}", dir.display());
            *s = Some(session);
            Ok(module)
        }
        Err(e) => {
            unsafe {
                let _ = FreeLibrary(module);
            }
            Err(e)
        }
    }
}

/// Streamline's proxy of `factory`, asking for `frames` on screen per drawn one: a swap chain it
/// makes is one Streamline sees every present of.
pub(super) fn proxy_factory(device: &ID3D12Device, factory: &IDXGIFactory5, frames: u32) -> Result<IDXGIFactory2> {
    let module = start(device, frames)?;
    let upgrade: FnUpgradeInterface = load_fn(module, c"slUpgradeInterface")?;
    let mut raw = factory.clone().into_raw();
    let r = unsafe { upgrade(&mut raw) };
    if r != 0 || raw.is_null() || raw == factory.as_raw() {
        if !raw.is_null() {
            // (our reference back, whatever became of it)
            drop(unsafe { windows::core::IUnknown::from_raw(raw) });
        }
        bail!("Streamline gave no proxy of the DXGI factory ({})", result_name(r));
    }
    unsafe { windows::core::IUnknown::from_raw(raw) }.cast().context("Streamline's proxy of the DXGI factory")
}

/// DLSS that joined the session (`module`) is let go of.
pub(super) fn leave() {
    if let Some(s) = session().as_mut() {
        s.joined = false;
    }
}

/// Before the renderer drops what frame generation reads at the present (a rebuild): off until
/// DLSS tags a frame again.
pub(super) fn pause() {
    if let Some(s) = session().as_mut() {
        let size = s.set.map_or((0, 0), |(_, size)| size);
        s.switch(1, size);
        s.tagged = false;
        s.token = std::ptr::null();
    }
}

/// The window's swap chain is Streamline's from now on (`on`), or no longer.
pub(super) fn set_window(on: bool) {
    if let Some(s) = session().as_mut() {
        s.window = on;
        if !on {
            s.switch(1, (0, 0));
            s.token = std::ptr::null();
        }
    }
}

/// Another count of frames on screen per drawn one (the setting changed in the game's menu): from
/// the next frame DLSS tags.
pub(super) fn ask(frames: u32) {
    if let Some(s) = session().as_mut() {
        s.asked = frames;
    }
}

/// Whether DLSS tags this frame for frame generation (the window's swap chain is Streamline's
/// and it still works).
pub(super) fn wanted() -> bool {
    session().as_ref().is_some_and(|s| s.window && !s.failed)
}

/// Whether frame generation still works (`FrameGeneration::working`).
pub(super) fn working() -> bool {
    session().as_ref().is_some_and(|s| !s.failed)
}

/// DLSS has a token for this frame and tags it for frame generation: Reflex's markers up to the
/// render submit (frame generation checks Reflex runs by them).
pub(super) fn frame_begun(token: *const c_void) {
    if let Some(s) = session().as_mut() {
        s.token = token;
        for m in [MARKER_SIMULATION_START, MARKER_SIMULATION_END, MARKER_RENDER_SUBMIT_START] {
            let _ = unsafe { (s.set_marker)(m, token) };
        }
    }
}

/// DLSS's work of this frame is submitted: frame generation on for a window of `size`.
pub(super) fn frame_submitted(size: (u32, u32)) {
    if let Some(s) = session().as_mut() {
        if !s.token.is_null() {
            let _ = unsafe { (s.set_marker)(MARKER_RENDER_SUBMIT_END, s.token) };
        }
        let frames = if s.most > 0 { s.asked.min(s.most) } else { s.asked };
        s.switch(frames, size);
        s.tagged = true;
    }
}

/// The window's present, between PCL's markers. A frame DLSS did not tag (a menu, the loading
/// screen) is shown without frame generation: it has nothing to make frames from.
pub(super) fn present(present: impl FnOnce()) {
    let mut guard = session();
    let Some(s) = guard.as_mut() else {
        drop(guard);
        present();
        return;
    };
    if !s.tagged && s.set.is_some_and(|(f, _)| f > 1) {
        s.switch(1, s.set.map_or((0, 0), |(_, size)| size));
    }
    let on = s.tagged && s.set.is_some_and(|(f, _)| f > 1);
    s.tagged = false;
    let token = std::mem::replace(&mut s.token, std::ptr::null());
    if on {
        let _ = unsafe { (s.set_marker)(MARKER_PRESENT_START, token) };
    }
    let (set_marker, get_state, viewport) = (s.set_marker, s.get_state, ViewportHandle { base: Base::new(VIEWPORT_HANDLE, 1), value: s.viewport.value });
    // (not held over the present: Streamline's work there needs nothing of ours)
    drop(guard);
    present();
    if on {
        let _ = unsafe { set_marker(MARKER_PRESENT_END, token) };
        let mut state = blank_state();
        if unsafe { get_state(&viewport, &mut state, std::ptr::null()) } == 0 {
            // (the frames presented since the last call: this one and the ones made after it)
            GENERATED.fetch_add(u64::from(state.num_frames_actually_presented.saturating_sub(1)), Ordering::Relaxed);
        }
    }
}

fn blank_state() -> State {
    State {
        base: Base::new(DLSSG_STATE, 4),
        estimated_vram_usage_in_bytes: 0,
        status: 0,
        min_width_or_height: 0,
        num_frames_actually_presented: 0,
        num_frames_to_generate_max: 0,
        b_reserved4: 0,
        is_vsync_support_available: 0,
        inputs_processing_completion_fence: std::ptr::null_mut(),
        last_present_inputs_processing_completion_fence_value: 0,
        is_dynamic_mfg_supported: 0,
    }
}

impl Session {
    /// `slDLSSGSetOptions` when the frames on screen per drawn one (1: off) or the window's size
    /// changed. The first time it is on, what the card can do goes to the log.
    fn switch(&mut self, frames: u32, size: (u32, u32)) {
        if self.failed || self.set == Some((frames, size)) || (frames <= 1 && self.set.is_none_or(|(f, _)| f <= 1)) {
            return;
        }
        let options = Options {
            base: Base::new(DLSSG_OPTIONS, 5),
            mode: if frames > 1 { MODE_ON } else { MODE_OFF },
            num_frames_to_generate: frames.saturating_sub(1).max(1),
            // (switched off for a menu it keeps its memory: no hitch when it comes back)
            flags: RETAIN_RESOURCES_WHEN_OFF,
            dynamic_res_width: 0,
            dynamic_res_height: 0,
            num_back_buffers: 0,
            mvec_depth_width: 0,
            mvec_depth_height: 0,
            color_width: 0,
            color_height: 0,
            color_buffer_format: 0,
            mvec_buffer_format: 0,
            depth_buffer_format: 0,
            hud_less_buffer_format: 0,
            ui_buffer_format: 0,
            on_error_callback: std::ptr::null(),
            b_reserved15: BOOLEAN_INVALID,
            queue_parallelism_mode: 0,
            enable_user_interface_recomposition: 0,
            dynamic_target_frame_rate: 0.0,
        };
        if let Err(e) = check("slDLSSGSetOptions", unsafe { (self.set_options)(&self.viewport, &options) }) {
            log::error!("frame generation stopped working ({e:#}); the frames are shown without it from now on");
            self.failed = true;
            return;
        }
        self.set = Some((frames, size));
        if self.most == 0 && frames > 1 {
            let mut state = blank_state();
            match check("slDLSSGGetState", unsafe { (self.get_state)(&self.viewport, &mut state, &options) }) {
                Ok(()) => {
                    self.most = state.num_frames_to_generate_max.max(1) + 1;
                    log::info!(
                        "frame generation: NVIDIA DLSS {frames}x for {}x{}, up to {}x on this card, vsync with it {}{}",
                        size.0,
                        size.1,
                        self.most,
                        if state.is_vsync_support_available == 1 { "supported" } else { "not supported" },
                        if state.status & STATUS_NO_REFLEX != 0 { " (Reflex not seen yet)" } else { "" }
                    );
                    if frames > self.most {
                        log::warn!("frame generation: {frames}x asked, this card makes at most {}x; {}x from the next frame", self.most, self.most);
                    }
                }
                Err(e) => log::warn!("frame generation: {e:#}"),
            }
        }
    }
}
