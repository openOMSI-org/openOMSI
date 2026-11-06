//! Drawing into a window: the surface, its configuration and the frames taken from it.

use crate::*;

/// Helper for windowed rendering.
pub struct SurfaceState<'w> {
    /// (let go of in `drop`, unless the device was lost - see there)
    pub surface: std::mem::ManuallyDrop<wgpu::Surface<'w>>,
    pub config: wgpu::SurfaceConfiguration,
    /// The renderer's `device_lost`.
    pub(crate) lost: Arc<std::sync::Mutex<Option<String>>>,
    /// On ANGLE: the frames' sRGB stand-in, encoded into the plain window (see there).
    pub(crate) encode: Option<srgb_encode::SrgbEncode>,
    /// A glass theme is in force: the window's picture is copied from (`set_glass`).
    glass: bool,
    /// AMD's frame-interpolation swap chain, in place of the surface's, while frame generation
    /// is on (the surface is then never configured).
    own: Option<temporal::OwnSwapChain>,
}

/// The window's next picture, as `wgpu::CurrentSurfaceTexture` names the cases.
pub enum Acquired {
    Success(WindowFrame),
    Suboptimal(WindowFrame),
    Timeout,
    Occluded,
    Outdated,
    Lost,
    Validation,
}

/// A picture of the window's: the surface's, or a buffer of frame generation's swap chain.
pub struct WindowFrame {
    pub texture: wgpu::Texture,
    surface: Option<wgpu::SurfaceTexture>,
}

impl From<wgpu::CurrentSurfaceTexture> for Acquired {
    fn from(c: wgpu::CurrentSurfaceTexture) -> Acquired {
        let frame = |f: wgpu::SurfaceTexture| WindowFrame { texture: f.texture.clone(), surface: Some(f) };
        match c {
            wgpu::CurrentSurfaceTexture::Success(f) => Acquired::Success(frame(f)),
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => Acquired::Suboptimal(frame(f)),
            wgpu::CurrentSurfaceTexture::Timeout => Acquired::Timeout,
            wgpu::CurrentSurfaceTexture::Occluded => Acquired::Occluded,
            wgpu::CurrentSurfaceTexture::Outdated => Acquired::Outdated,
            wgpu::CurrentSurfaceTexture::Lost => Acquired::Lost,
            wgpu::CurrentSurfaceTexture::Validation => Acquired::Validation,
        }
    }
}

/// Frame generation's swap chain for `window`, or None (logged) where it cannot be made.
fn own_swap_chain(window: &winit_window::Window, device: &wgpu::Device, format: wgpu::TextureFormat, size: (u32, u32), vsync: bool, kind: temporal::FrameGenerationKind) -> Option<temporal::OwnSwapChain> {
    #[cfg(windows)]
    {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let made = match window.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(h)) => temporal::OwnSwapChain::new(device, windows::Win32::Foundation::HWND(h.hwnd.get() as *mut std::ffi::c_void), format, size, vsync, kind),
            _ => Err(anyhow!("no Win32 window")),
        };
        match made {
            Ok(s) => Some(s),
            Err(e) => {
                log::warn!("frame generation is off: {e:#}");
                None
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (window, device, format, size, vsync, kind);
        None
    }
}

impl Drop for SurfaceState<'_> {
    fn drop(&mut self) {
        // After a lost device the frame it was drawing never finishes, and its swapchain
        // image with it: letting the surface go (or configuring it again) then tears the
        // swapchain down under that image - "Trying to destroy a SwapchainAcquireSemaphore
        // that is still in use by a SurfaceTexture" (Vulkan) ended the game instead of the
        // session ending in order. The window goes with the process anyway. So on a panic:
        // the frame being drawn is let go of while unwinding, and the same message then
        // took the place of the panic that ended the game in its report (#112: a sort).
        if !std::thread::panicking() && self.lost.lock().unwrap_or_else(|e| e.into_inner()).is_none() {
            // SAFETY: dropped only here, once
            unsafe { std::mem::ManuallyDrop::drop(&mut self.surface) };
        }
    }
}

pub fn create_surface(instance: &wgpu::Instance, window: &Arc<winit_window::Window>) -> Result<wgpu::Surface<'static>> {
    crate::catch(|| instance.create_surface(window.clone()))
        .ok_or_else(|| anyhow!("the graphics driver refused the window (still held after a lost graphics device? restarting the game frees it)"))?
        .context("create_surface")
}

/// The present mode for the window's vsync setting: `AutoVsync` / `AutoNoVsync`, as ever
/// (`Fifo` for everybody would make a game that runs just below the refresh rate wait a whole
/// blank for each late frame, where `AutoVsync` tears a little). `OMSI_PRESENT_MODE` picks a
/// mode for an A/B run, when the surface lists it among `supported` (None: not known yet).
pub(crate) fn present_mode(vsync: bool, capped: bool, supported: Option<&[wgpu::PresentMode]>) -> wgpu::PresentMode {
    let auto = match (vsync, capped) {
        (false, _) => wgpu::PresentMode::AutoNoVsync,
        (true, true) => wgpu::PresentMode::Fifo,
        (true, false) => wgpu::PresentMode::AutoVsync,
    };
    let asked = match omsi_cfg::flags::OMSI_PRESENT_MODE.var().map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("fifo") => wgpu::PresentMode::Fifo,
        Some("fifo_relaxed") | Some("relaxed") => wgpu::PresentMode::FifoRelaxed,
        Some("mailbox") => wgpu::PresentMode::Mailbox,
        Some("immediate") => wgpu::PresentMode::Immediate,
        _ => return auto,
    };
    match supported {
        Some(list) if list.contains(&asked) => asked,
        Some(_) => {
            log::warn!("OMSI_PRESENT_MODE: the surface has no {asked:?}; using {auto:?}");
            auto
        }
        None => auto,
    }
}

/// The surface's present modes, asked of the driver only for `OMSI_PRESENT_MODE` (`set_vsync` runs every frame).
fn listed_modes(surface: &wgpu::Surface<'_>, renderer: &Renderer) -> Option<Vec<wgpu::PresentMode>> {
    omsi_cfg::flags::OMSI_PRESENT_MODE.var()?;
    renderer.adapter.as_ref().map(|a| surface.get_capabilities(a).present_modes)
}

impl<'w> SurfaceState<'w> {
    pub fn new(
        instance: &wgpu::Instance,
        window: Arc<winit_window::Window>,
        renderer: &Renderer,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        Self::new_with(instance, window, renderer, width, height, true, None)
    }

    /// `vsync` false lets the frames go out as fast as they are drawn. `frame_generation`: the
    /// window gets frame generation's swap chain where it can (DirectX 12).
    pub fn new_with(
        instance: &wgpu::Instance,
        window: Arc<winit_window::Window>,
        renderer: &Renderer,
        width: u32,
        height: u32,
        vsync: bool,
        frame_generation: Option<temporal::FrameGenerationKind>,
    ) -> Result<Self> {
        let surface = create_surface(instance, &window)?;
        let own = frame_generation.and_then(|kind| own_swap_chain(&window, &renderer.device, renderer.format(), (width, height), vsync, kind));
        let encode = srgb_encode::SrgbEncode::wanted(renderer, renderer.format());
        let modes = listed_modes(&surface, renderer);
        // (copied from only for a glass theme: `set_glass`)
        let mut config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: encode.as_ref().map_or(renderer.format(), |e| e.window_format()),
            width: width.max(1),
            height: height.max(1),
            present_mode: present_mode(vsync, false, modes.as_deref()),
            // (two frames in flight keep the graphics chip busy while the next frame is
            // recorded: with one, the whole loop serialized behind the vsync'd drawable -
            // on Apple silicon a frame cost GPU + CPU instead of the larger of the two,
            // 50 ms where 24 of them were GPU. The price is one more frame of input
            // delay, 33 ms at 60 Hz.)
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        config.width = width.max(1);
        config.height = height.max(1);
        if own.is_none() {
            surface.configure(&renderer.device, &config);
        }
        Ok(SurfaceState { surface: std::mem::ManuallyDrop::new(surface), config, lost: renderer.errors.device_lost.clone(), encode, glass: false, own })
    }

    /// The window's surface on a device whose renderer is still being built ([`Opened`]'s
    /// device and format): pictures without the renderer (a clear, an interface) can be
    /// shown until [`Self::adopt`] hands it over.
    pub fn early(instance: &wgpu::Instance, window: Arc<winit_window::Window>, device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32, vsync: bool, frame_generation: Option<temporal::FrameGenerationKind>) -> Result<Self> {
        let surface = create_surface(instance, &window)?;
        let own = frame_generation.and_then(|kind| own_swap_chain(&window, device, format, (width, height), vsync, kind));
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: present_mode(vsync, false, None),
            // (two frames in flight: see `new_with`)
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        if own.is_none() {
            surface.configure(device, &config);
        }
        Ok(SurfaceState { surface: std::mem::ManuallyDrop::new(surface), config, lost: Default::default(), encode: None, glass: false, own })
    }

    /// The renderer built for the device this surface was made on ([`Self::early`]) takes
    /// it over. False when the renderer is on another device (the first was lost while the
    /// pipelines were built): the surface is then made anew for it. (Not for ANGLE, whose
    /// window takes the sRGB pictures encoded: its renderer is built before the surface.)
    pub fn adopt(&mut self, renderer: &Renderer, device: &wgpu::Device) -> bool {
        if &renderer.device != device {
            return false;
        }
        self.lost = renderer.errors.device_lost.clone();
        true
    }

    /// Whether a glass theme is in force: only then is the window's picture copied from (the
    /// glass behind the game's screens, see `picture`), where the window allows it - a window
    /// that may be copied from can cost a mobile GPU its framebuffer compression. Reconfigures
    /// the window when that changes; cheap to call every frame.
    pub fn set_glass(&mut self, renderer: &Renderer, on: bool) {
        if self.glass == on {
            return;
        }
        self.glass = on;
        // (frame generation's buffers can always be copied from)
        if self.own.is_some() {
            return;
        }
        let copy = on && renderer.adapter.as_ref().is_some_and(|a| self.surface.get_capabilities(a).usages.contains(wgpu::TextureUsages::COPY_SRC));
        let usage = if copy { wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC } else { wgpu::TextureUsages::RENDER_ATTACHMENT };
        if self.config.usage != usage {
            self.config.usage = usage;
            self.surface.configure(&renderer.device, &self.config);
        }
    }

    /// The window's next picture.
    pub fn acquire(&self) -> Acquired {
        match &self.own {
            Some(own) => match own.current() {
                Some(texture) => Acquired::Success(WindowFrame { texture, surface: None }),
                None => Acquired::Outdated,
            },
            None => self.surface.get_current_texture().into(),
        }
    }

    /// Frame generation's handle for the renderer, while the window has its swap chain.
    pub fn frame_generation(&self) -> Option<temporal::FrameGeneration> {
        self.own.as_ref().map(|o| o.frame_generation())
    }

    /// The view `frame` is drawn into (of the renderer's format).
    pub fn view(&self, device: &wgpu::Device, frame: &WindowFrame) -> wgpu::TextureView {
        match &self.encode {
            Some(e) => e.view(device, frame.texture.size()),
            None => frame.texture.create_view(&Default::default()),
        }
    }

    /// The texture `frame` is drawn into ([`Self::view`]: the window's own, or the stand-in it is
    /// encoded from), to be copied from - none where it cannot be (a window that allows no copy).
    pub fn picture(&self, frame: &WindowFrame) -> Option<wgpu::Texture> {
        let t = match &self.encode {
            Some(e) => e.texture()?,
            None => frame.texture.clone(),
        };
        t.usage().contains(wgpu::TextureUsages::COPY_SRC).then_some(t)
    }

    /// Shows `frame`, drawn into [`Self::view`].
    pub fn present(&self, device: &wgpu::Device, queue: &wgpu::Queue, frame: WindowFrame) {
        if let Some(own) = &self.own {
            own.present(device, queue, &frame.texture);
            return;
        }
        if let Some(e) = &self.encode {
            e.encode(device, queue, &frame.texture);
        }
        if let Some(f) = frame.surface {
            f.present();
        }
    }

    pub fn resize(&mut self, renderer: &Renderer, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        // (not after a lost device: see `drop`)
        if renderer.device_lost().is_some() {
            return;
        }
        match self.own.as_mut() {
            Some(own) => own.resize(&renderer.device, (self.config.width, self.config.height)),
            None => self.surface.configure(&renderer.device, &self.config),
        }
    }

    /// [`Self::resize`] while the renderer is still being built (see [`Self::early`]).
    pub fn resize_on(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        match self.own.as_mut() {
            Some(own) => own.resize(device, (self.config.width, self.config.height)),
            None => self.surface.configure(device, &self.config),
        }
    }

    pub fn set_vsync(&mut self, renderer: &Renderer, enabled: bool, capped: bool) {
        let modes = listed_modes(&self.surface, renderer);
        let mode = present_mode(enabled, capped, modes.as_deref());
        if self.config.present_mode == mode || renderer.device_lost().is_some() {
            return;
        }
        self.config.present_mode = mode;
        if let Some(own) = self.own.as_mut() {
            own.set_vsync(enabled);
            return;
        }
        // (two frames in flight: see `new_with`)
        self.config.desired_maximum_frame_latency = 2;
        self.surface.configure(&renderer.device, &self.config);
    }
}

/// Re-export so the app does not need to depend on winit's window type path.
pub mod winit_window {
    pub use winit::window::Window;
}
