//! The launcher's graphics device: opened for its window with the interface's pipeline at
//! once, the renderer (the bus preview's) built on a worker meanwhile (see
//! `host::RendererStart`); made again after a game or a lost device; and the window's
//! picture read back for OMSI_LAUNCHER_SHOT.

use super::*;

impl Launcher {
    /// The surface for the window the launcher has (created again after the app was in the
    /// background: a phone takes the window's surface away meanwhile), and the graphics device
    /// when it has none.
    pub(super) fn make_surface(&mut self) {
        let Some(window) = self.window.clone() else { return };
        if !self.device_open() {
            if let Err(e) = self.open_device(&window, true) {
                host().fatal_message(&format!("openOMSI cannot draw on this computer: {e:#}"));
                std::process::exit(1);
            }
            self.last = Instant::now();
            return;
        }
        let size = window.inner_size();
        self.surface = match (self.renderer.as_ref(), self.dev.as_ref()) {
            (Some(r), _) => SurfaceState::new_with(&self.instance, window.clone(), r, size.width.max(1), size.height.max(1), true).ok(),
            (None, Some((device, _, format))) => SurfaceState::early(&self.instance, window.clone(), device, *format, size.width.max(1), size.height.max(1), true).ok(),
            (None, None) => None,
        };
        self.last = Instant::now();
    }

    /// A phone's first start: the window, its graphics device and surface made before the
    /// launcher (`android::Shell`), which shows a first picture while the launcher is
    /// put together. The launcher draws on them from its first frame.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub fn adopt_early(&mut self, window: Arc<Window>, start: Box<dyn host::RendererStart>, surface: SurfaceState<'static>) {
        self.gpu = Some(omsi_ui::Gpu::new(start.device(), start.format(), 4, self.ui.atlas.size));
        self.dev = Some((start.device().clone(), start.queue().clone(), start.format()));
        self.starting = Some(start);
        self.surface = Some(surface);
        self.window = Some(window);
        self.last = Instant::now();
    }

    /// The window has a graphics device: the renderer, or the one being built.
    pub(super) fn device_open(&self) -> bool {
        self.renderer.is_some() || self.starting.is_some()
    }

    /// The graphics device for the window, its surface and the interface's pipeline; the
    /// renderer (the bus preview's) is built on a worker meanwhile and taken when it is there
    /// (`poll_renderer`). Its ~110 pipelines took seconds before the launcher's first
    /// picture - 8 s on DirectX 12 - while the window stood blank; the interface needs one.
    /// `fresh_atlas`: the interface's glyphs go up anew (a device after another one).
    pub(super) fn open_device(&mut self, window: &Arc<Window>, fresh_atlas: bool) -> anyhow::Result<()> {
        // Finish dropping the device given up for a game before opening another one.
        if let Some(h) = self.gpu_rest_drop.take() {
            let _ = h.join();
        }
        let start = host().begin_renderer(&mut self.instance, window, host().showroom_options())?;
        if fresh_atlas {
            self.ui.atlas = omsi_ui::Atlas::new(self.ui.atlas.size);
        }
        if !start.early() {
            // (ANGLE: built on the spot, see `RendererStart::begin`)
            let renderer = start.wait(&mut self.instance)?;
            let size = window.inner_size();
            self.surface = Some(SurfaceState::new_with(&self.instance, window.clone(), &renderer, size.width.max(1), size.height.max(1), true).map_err(|e| anyhow::anyhow!("its window cannot be drawn into: {e:#}"))?);
            self.renderer_ready(renderer);
            return Ok(());
        }
        let size = window.inner_size();
        self.surface = Some(SurfaceState::early(&self.instance, window.clone(), start.device(), start.format(), size.width.max(1), size.height.max(1), true).map_err(|e| anyhow::anyhow!("its window cannot be drawn into: {e:#}"))?);
        self.gpu = Some(omsi_ui::Gpu::new(start.device(), start.format(), 4, self.ui.atlas.size));
        self.dev = Some((start.device().clone(), start.queue().clone(), start.format()));
        self.starting = Some(start);
        Ok(())
    }

    /// The renderer being built is there (or failed: then the launcher cannot go on, as
    /// before when it was built on the spot).
    pub(super) fn poll_renderer(&mut self) {
        if !self.starting.as_ref().is_some_and(|s| s.is_done()) {
            return;
        }
        let start = self.starting.take().expect("renderer start");
        let early = start.device().clone();
        match start.wait(&mut self.instance) {
            Ok(r) => {
                // (on another device - the first was lost while the pipelines were built -
                // everything made on the first goes, and is made anew on the renderer's; a
                // phone's window without its surface meanwhile gets it when it is back)
                if r.device == early {
                    if let Some(s) = self.surface.as_mut() {
                        s.adopt(&r, &early);
                    }
                } else {
                    self.surface = None;
                    self.drop_gpu();
                    self.ui.atlas = omsi_ui::Atlas::new(self.ui.atlas.size);
                    if let Some(window) = self.window.clone() {
                        let size = window.inner_size();
                        self.surface = SurfaceState::new_with(&self.instance, window, &r, size.width.max(1), size.height.max(1), true).ok();
                    }
                }
                self.renderer_ready(r);
            }
            Err(e) => {
                host().fatal_message(&format!("openOMSI cannot draw on this computer: {e:#}"));
                std::process::exit(1);
            }
        }
    }

    pub(super) fn renderer_ready(&mut self, renderer: Renderer) {
        if let Some(window) = self.window.as_ref() {
            let size = window.inner_size();
            log::info!("launcher window {}x{} (scale {:.2}), adapter {}, renderer ready {:.2} s after the start", size.width, size.height, window.scale_factor(), renderer.adapter_name, self.started.elapsed().as_secs_f64());
        }
        if self.gpu.is_none() {
            self.gpu = Some(omsi_ui::Gpu::new(&renderer.device, renderer.format(), 4, self.ui.atlas.size));
        }
        self.dev = Some((renderer.device.clone(), renderer.queue.clone(), renderer.format()));
        self.renderer = Some(renderer);
    }

    /// The graphics device was lost (#274: an AMD Radeon's DX12 driver gave up while the
    /// preview's textures went up, and the launcher then drew on the dead device, with
    /// thousands of errors a second): everything made on it goes, the other interface is
    /// taken - DirectX 12 and Vulkan for each other, remembered in the settings for the game
    /// as well - and the window is drawn again on a new device. Twice at most.
    pub(super) fn recover_device(&mut self) -> bool {
        let Some(why) = self.renderer.as_ref().and_then(|r| r.device_lost()) else { return false };
        let tries = LAUNCHER_RECOVERIES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = self.renderer.as_ref().map(|r| r.adapter_name.clone()).unwrap_or_default();
        let other = if name.contains("(Dx12)") { Some("vulkan") } else if name.contains("(Vulkan)") && cfg!(windows) { Some("dx12") } else if name.contains("(Vulkan)") { Some("gl") } else { None };
        log::error!("launcher: the graphics device was lost on {name} ({why}); {}", match (tries < 2, other) {
            (true, Some(o)) => format!("drawing on {o} from now on"),
            (true, None) => "drawing on a new device".to_string(),
            _ => "giving up".to_string(),
        });
        if tries >= 2 {
            return false;
        }
        if let Some(o) = other {
            std::env::set_var("OMSI_BACKEND", o);
            self.state.settings["graphics_api"] = serde_json::json!(o);
            self.state.settings_dirty = 0.3;
        }
        self.surface = None;
        self.drop_gpu();
        if let Some(h) = self.gpu_rest_drop.take() {
            let _ = h.join();
        }
        self.renderer = None;
        self.dev = None;
        self.make_surface();
        true
    }

    /// The window made hidden is shown once it has a picture (or a few frames on without one:
    /// better a blank window than none).
    pub(super) fn show_window(&mut self) {
        self.tries += 1;
        if self.tries >= 3 && std::mem::take(&mut self.show) {
            if let Some(w) = self.window.as_ref() {
                w.set_visible(true);
            }
        }
    }

    /// The window's picture drawn again into a texture that stays on the graphics card, to be
    /// drawn with (the old look while the new one spreads, `theme_switch`): nothing is read
    /// back and nothing waits for it. None when the card cannot make one (a format it cannot
    /// read back in a shader): the look then changes at once.
    pub(super) fn look_picture(&mut self, w: u32, h: u32, layers: &[omsi_ui::Layer], draws: &[Draw], bg: wgpu::Color) -> Option<(wgpu::Texture, wgpu::TextureView)> {
        let (device, queue, format) = self.dev.clone()?;
        let gpu = self.gpu.as_mut()?;
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let tex = device.create_texture(&wgpu::TextureDescriptor { label: Some("launcher look"), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
        if let Some(e) = pollster::block_on(scope.pop()) {
            log::warn!("launcher: no picture of the look for its switch ({e}); it changes at once");
            return None;
        }
        let view = tex.create_view(&Default::default());
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("launcher look") });
        gpu.render(&device, &queue, &mut enc, &view, (w, h), Some(bg), layers, draws);
        queue.submit([enc.finish()]);
        Some((tex, view))
    }

    /// The window's picture drawn again into a texture and read back (for OMSI_LAUNCHER_SHOT).
    pub(super) fn shot_image(&mut self, w: u32, h: u32, layers: &[omsi_ui::Layer], draws: &[Draw], bg: wgpu::Color) -> Option<image::RgbaImage> {
        let (device, queue, format) = self.dev.clone()?;
        let gpu = self.gpu.as_mut()?;
        let tex = device.create_texture(&wgpu::TextureDescriptor { label: Some("launcher shot"), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
        let view = tex.create_view(&Default::default());
        let mut enc = device.create_command_encoder(&Default::default());
        gpu.render(&device, &queue, &mut enc, &view, (w, h), Some(bg), layers, draws);
        let stride = (w * 4).div_ceil(256) * 256;
        let buf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (stride * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        enc.copy_texture_to_buffer(tex.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: None } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
        queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        let data = buf.slice(..).get_mapped_range();
        let bgra = matches!(format, wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Bgra8Unorm);
        let mut img = image::RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let i = (y * stride + x * 4) as usize;
                let (r_, g, b) = if bgra { (data[i + 2], data[i + 1], data[i]) } else { (data[i], data[i + 1], data[i + 2]) };
                img.put_pixel(x, y, image::Rgba([r_, g, b, 255]));
            }
        }
        Some(img)
    }
}
