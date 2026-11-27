//! Drawing into a window: the surface, its configuration and the frames taken from it.

use crate::*;

/// Helper for windowed rendering.
pub struct SurfaceState<'w> {
    /// (let go of in `drop`, unless the device was lost - see there)
    pub surface: std::mem::ManuallyDrop<mx::Surface<'w>>,
    pub config: mx::SurfaceConfiguration,
    /// The renderer's `device_lost`.
    pub(crate) lost: Arc<std::sync::Mutex<Option<String>>>,
    /// On ANGLE: the frames' sRGB stand-in, encoded into the plain window (see there).
    pub(crate) encode: Option<srgb_encode::SrgbEncode>,
    /// A glass theme is in force: the window's picture is copied from (`set_glass`).
    glass: bool,
    /// Frame generation, while it is on: the frames are drawn into its buffer and shown by
    /// it, a generated one before each (`set_frame_generation`).
    generation: Option<temporal::GenerationWindow>,
    /// It was asked for and could not start, or gave up: not tried again until it is turned
    /// off and on.
    generation_refused: bool,
    /// What the frame generation is, while it is on.
    generation_kind: Option<temporal::FrameGenerationKind>,
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
        if !std::thread::panicking()
            && self
                .lost
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_none()
        {
            // SAFETY: dropped only here, once
            unsafe { std::mem::ManuallyDrop::drop(&mut self.surface) };
        }
    }
}

pub fn create_surface(
    instance: &mx::Instance,
    window: &Arc<winit_window::Window>,
) -> Result<mx::Surface<'static>> {
    crate::catch(|| instance.create_surface(window.clone()))
        .ok_or_else(|| anyhow!("the graphics driver refused the window (still held after a lost graphics device? restarting the game frees it)"))?
        .context("create_surface")
}

/// The present mode for the window's vsync setting: `AutoVsync` / `AutoNoVsync`, as ever
/// (`Fifo` for everybody would make a game that runs just below the refresh rate wait a whole
/// blank for each late frame, where `AutoVsync` tears a little). `OMSI_PRESENT_MODE` picks a
/// mode for an A/B run, when the surface lists it among `supported` (None: not known yet).
pub(crate) fn present_mode(
    vsync: bool,
    capped: bool,
    supported: Option<&[mx::PresentMode]>,
) -> mx::PresentMode {
    let auto = match (vsync, capped) {
        (false, _) => mx::PresentMode::AutoNoVsync,
        (true, true) => mx::PresentMode::Fifo,
        (true, false) => mx::PresentMode::AutoVsync,
    };
    let asked = match omsi_cfg::flags::OMSI_PRESENT_MODE
        .var()
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("fifo") => mx::PresentMode::Fifo,
        Some("fifo_relaxed") | Some("relaxed") => mx::PresentMode::FifoRelaxed,
        Some("mailbox") => mx::PresentMode::Mailbox,
        Some("immediate") => mx::PresentMode::Immediate,
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
fn listed_modes(surface: &mx::Surface<'_>, renderer: &Renderer) -> Option<Vec<mx::PresentMode>> {
    omsi_cfg::flags::OMSI_PRESENT_MODE.var()?;
    renderer
        .adapter
        .as_ref()
        .map(|a| surface.get_capabilities(a).present_modes)
}

/// Run a surface configuration, turning mX's refusal into the `Err` the window setup
/// already handles. mX has no wgpu error scopes: its backends panic on an unusable
/// surface, so that panic is caught here instead of ending the game (#4256).
fn configure_surface(
    surface: &mx::Surface<'_>,
    device: &mx::Device,
    config: &mx::SurfaceConfiguration,
) -> Result<()> {
    attempt_configure(|| surface.configure(device, config))
}

fn attempt_configure(make: impl FnOnce()) -> Result<()> {
    match omsi_cfg::panic_guard::catch_unwind(make) {
        Ok(()) => Ok(()),
        Err(panic) => {
            let text = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("an unknown error")
                .to_string();
            Err(anyhow!("surface configure: {text}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_surface_configuration_is_an_error_not_a_panic() {
        let err = attempt_configure(|| panic!("mX: invalid surface")).unwrap_err();
        assert!(err.to_string().contains("invalid surface"));
    }

    #[test]
    fn an_accepted_surface_configuration_succeeds() {
        assert!(attempt_configure(|| {}).is_ok());
    }
}

impl<'w> SurfaceState<'w> {
    pub fn new(
        instance: &mx::Instance,
        window: Arc<winit_window::Window>,
        renderer: &Renderer,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        Self::new_with(instance, window, renderer, width, height, true)
    }

    /// `vsync` false lets the frames go out as fast as they are drawn.
    pub fn new_with(
        instance: &mx::Instance,
        window: Arc<winit_window::Window>,
        renderer: &Renderer,
        width: u32,
        height: u32,
        vsync: bool,
    ) -> Result<Self> {
        let surface = create_surface(instance, &window)?;
        let encode = srgb_encode::SrgbEncode::wanted(renderer, renderer.format());
        let modes = listed_modes(&surface, renderer);
        // (copied from only for a glass theme: `set_glass`)
        let mut config = mx::SurfaceConfiguration {
            usage: mx::TextureUsages::RENDER_ATTACHMENT,
            format: encode
                .as_ref()
                .map_or(renderer.format(), |e| e.window_format()),
            width: width.max(1),
            height: height.max(1),
            present_mode: present_mode(vsync, false, modes.as_deref()),
            // (two frames in flight keep the graphics chip busy while the next frame is
            // recorded: with one, the whole loop serialized behind the vsync'd drawable -
            // on Apple silicon a frame cost GPU + CPU instead of the larger of the two,
            // 50 ms where 24 of them were GPU. The price is one more frame of input
            // delay, 33 ms at 60 Hz.)
            desired_maximum_frame_latency: 2,
            alpha_mode: mx::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        config.width = width.max(1);
        config.height = height.max(1);
        configure_surface(&surface, &renderer.device, &config)?;
        Ok(SurfaceState {
            surface: std::mem::ManuallyDrop::new(surface),
            config,
            lost: renderer.errors.device_lost.clone(),
            encode,
            glass: false,
            generation: None,
            generation_refused: false,
            generation_kind: None,
        })
    }

    /// The window's surface on a device whose renderer is still being built ([`Opened`]'s
    /// device and format): pictures without the renderer (a clear, an interface) can be
    /// shown until [`Self::adopt`] hands it over.
    pub fn early(
        instance: &mx::Instance,
        window: Arc<winit_window::Window>,
        device: &mx::Device,
        format: mx::TextureFormat,
        width: u32,
        height: u32,
        vsync: bool,
    ) -> Result<Self> {
        let surface = create_surface(instance, &window)?;
        let config = mx::SurfaceConfiguration {
            usage: mx::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: present_mode(vsync, false, None),
            // (two frames in flight: see `new_with`)
            desired_maximum_frame_latency: 2,
            alpha_mode: mx::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        configure_surface(&surface, device, &config)?;
        Ok(SurfaceState {
            surface: std::mem::ManuallyDrop::new(surface),
            config,
            lost: Default::default(),
            encode: None,
            glass: false,
            generation: None,
            generation_refused: false,
            generation_kind: None,
        })
    }

    /// The renderer built for the device this surface was made on ([`Self::early`]) takes
    /// it over. False when the renderer is on another device (the first was lost while the
    /// pipelines were built): the surface is then made anew for it. (Not for ANGLE, whose
    /// window takes the sRGB pictures encoded: its renderer is built before the surface.)
    pub fn adopt(&mut self, renderer: &Renderer, device: &mx::Device) -> bool {
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
        let copy = on
            && renderer.adapter.as_ref().is_some_and(|a| {
                self.surface
                    .get_capabilities(a)
                    .usages
                    .contains(mx::TextureUsages::COPY_SRC)
            });
        let usage = if copy {
            mx::TextureUsages::RENDER_ATTACHMENT | mx::TextureUsages::COPY_SRC
        } else {
            mx::TextureUsages::RENDER_ATTACHMENT
        };
        if self.config.usage != usage {
            self.config.usage = usage;
            self.surface.configure(&renderer.device, &self.config);
        }
    }

    /// Frame generation turned on or off for this window (cheap to call every frame, before
    /// the frame is acquired): on, the frames are drawn into its buffer ([`Self::view`]) and
    /// [`Self::present`] shows a generated one before each. The handle is the renderer's
    /// ([`Renderer::set_frame_generation`]); None where it is off, cannot run (logged once) or
    /// gave up.
    pub fn set_frame_generation(
        &mut self,
        renderer: &Renderer,
        kind: Option<temporal::FrameGenerationKind>,
    ) -> Option<temporal::FrameGeneration> {
        let kind = kind.filter(|_| renderer.device_lost().is_none());
        if kind != self.generation_kind
            && let Some(g) = self.generation.take()
        {
            self.stop_generation(renderer, g);
            log::info!("frame generation: off");
        }
        self.generation_kind = kind;
        let Some(kind) = kind else {
            self.generation_refused = false;
            return None;
        };
        if self.generation.as_ref().is_some_and(|g| !g.working()) {
            let g = self.generation.take().expect("checked above");
            self.stop_generation(renderer, g);
            self.generation_refused = true;
        }
        if self.generation_refused {
            return None;
        }
        let size = (self.config.width, self.config.height);
        match self.generation.as_mut() {
            Some(g) => g.resize(&renderer.device, &renderer.queue, size),
            None => match temporal::GenerationWindow::new(
                &renderer.device,
                &renderer.queue,
                &self.surface,
                kind,
                self.config.format,
                size,
            ) {
                Ok(g) => {
                    // (the vendor makes the window's swapchain from the next configure on)
                    if g.owns_swapchain() {
                        self.surface.configure(&renderer.device, &self.config);
                    }
                    self.generation = Some(g)
                }
                Err(e) => {
                    log::warn!("frame generation is off: {e:#}");
                    self.generation_refused = true;
                }
            },
        }
        self.generation.as_ref().map(|g| g.handle())
    }

    /// Frame generation off: the window's own swapchain again where the vendor made it.
    fn stop_generation(&mut self, renderer: &Renderer, g: temporal::GenerationWindow) {
        let owned = g.owns_swapchain();
        g.stop(&self.surface);
        if owned {
            self.surface.configure(&renderer.device, &self.config);
        }
    }

    /// The window's next picture.
    pub fn acquire(&self) -> mx::CurrentSurfaceTexture<mx::SurfaceTexture> {
        self.surface.get_current_texture()
    }

    /// The view `frame` is drawn into (of the renderer's format).
    pub fn view(&self, device: &mx::Device, frame: &mx::SurfaceTexture) -> mx::TextureView {
        if let Some(v) = self.generation.as_ref().and_then(|g| g.view()) {
            return v;
        }
        match &self.encode {
            Some(e) => e.view(device, frame.texture.size()),
            None => frame
                .texture
                .create_view(&Default::default())
                .expect("mX: a texture view"),
        }
    }

    /// The texture `frame` is drawn into ([`Self::view`]: the window's own, or the stand-in it is
    /// encoded from), to be copied from - none where it cannot be (a window that allows no copy).
    pub fn picture(&self, frame: &mx::SurfaceTexture) -> Option<mx::Texture> {
        if let Some(t) = self.generation.as_ref().and_then(|g| g.texture()) {
            return Some(t);
        }
        let t = match &self.encode {
            Some(e) => e.texture()?,
            None => frame.texture.clone(),
        };
        t.usage().contains(mx::TextureUsages::COPY_SRC).then_some(t)
    }

    /// Shows `frame`, drawn into [`Self::view`].
    pub fn present(&self, device: &mx::Device, queue: &mx::Queue, frame: mx::SurfaceTexture) {
        if let Some(g) = &self.generation {
            // (a vendor that presents through the window's own swapchain is handed the
            // window's picture as it is shown)
            if g.view().is_none()
                && let Some(e) = &self.encode
            {
                e.encode(device, queue, &frame);
            }
            return g.present(&self.surface, device, queue, frame);
        }
        if let Some(e) = &self.encode {
            e.encode(device, queue, &frame);
        }
        frame.present();
    }

    pub fn resize(&mut self, renderer: &Renderer, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        // (not after a lost device: see `drop`)
        if renderer.device_lost().is_some() {
            return;
        }
        self.surface.configure(&renderer.device, &self.config);
    }

    /// [`Self::resize`] while the renderer is still being built (see [`Self::early`]).
    pub fn resize_on(&mut self, device: &mx::Device, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(device, &self.config);
    }

    pub fn set_vsync(&mut self, renderer: &Renderer, enabled: bool, capped: bool) {
        let modes = listed_modes(&self.surface, renderer);
        let mode = present_mode(enabled, capped, modes.as_deref());
        if self.config.present_mode == mode || renderer.device_lost().is_some() {
            return;
        }
        self.config.present_mode = mode;
        // (two frames in flight: see `new_with`)
        self.config.desired_maximum_frame_latency = 2;
        self.surface.configure(&renderer.device, &self.config);
    }
}

/// Re-export so the app does not need to depend on winit's window type path.
pub mod winit_window {
    pub use winit::window::Window;
}
