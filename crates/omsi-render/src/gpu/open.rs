//! Opening the graphics device and building the renderer on it, with the fallbacks.

use crate::*;

/// The chosen MSAA is being tried again after a remembered fallback (see `build_with_fallbacks`).
static MSAA_RETRY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl Renderer {
    /// Create a renderer with the default options. `surface` is used to pick a compatible
    /// adapter and format.
    pub async fn new(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
        format: Option<wgpu::TextureFormat>,
    ) -> Result<Renderer> {
        Self::new_with(instance, surface, format, RenderOptions::default()).await
    }

    /// Create a renderer with the given graphics settings.
    pub async fn new_with(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
        format: Option<wgpu::TextureFormat>,
        options: RenderOptions,
    ) -> Result<Renderer> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: surface, force_fallback_adapter: false, ..Default::default() })
            .await
            .map_err(|e| anyhow!("no graphics adapter that can draw the game was found (Metal, Vulkan, DirectX 12 or OpenGL 3.3 or later); updating the graphics driver often helps: {e}"))?;
        Self::new_on(adapter, surface, format, options).await
    }

    /// The adapters of `instance` that can show `surface`, the ones worth trying first first:
    /// a graphics card of its own, then the processor's graphics, then anything else (a
    /// software renderer last).
    pub fn adapters_for(instance: &wgpu::Instance, surface: &wgpu::Surface<'_>) -> Vec<wgpu::Adapter> {
        let mut v: Vec<wgpu::Adapter> = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()))
            .into_iter()
            .filter(|a| a.is_surface_supported(surface))
            .collect();
        let rank = |a: &wgpu::Adapter| match a.get_info().device_type {
            wgpu::DeviceType::DiscreteGpu => 0,
            wgpu::DeviceType::IntegratedGpu => 1,
            wgpu::DeviceType::VirtualGpu | wgpu::DeviceType::Other => 2,
            wgpu::DeviceType::Cpu => 3,
        };
        v.sort_by_key(rank);
        v
    }

    /// Create a renderer on this adapter.
    pub async fn new_on(
        adapter: wgpu::Adapter,
        surface: Option<&wgpu::Surface<'_>>,
        format: Option<wgpu::TextureFormat>,
        options: RenderOptions,
    ) -> Result<Renderer> {
        Self::open_on(adapter, surface, format, options).await?.finish().await
    }

    /// The graphics device for a renderer on this adapter, its pipelines not built yet: see
    /// [`Opened`].
    pub async fn open_on(
        adapter: wgpu::Adapter,
        surface: Option<&wgpu::Surface<'_>>,
        format: Option<wgpu::TextureFormat>,
        options: RenderOptions,
    ) -> Result<Opened> {
        let info = adapter.get_info();
        let asked_options = options;
        // test hooks for an adapter that cannot be opened: an error, or wgpu going down
        #[cfg(feature = "test-hooks")]
        match omsi_cfg::flags::OMSI_FAKE_GPU_ERROR.var() {
            Some("open") => return Err(anyhow!("test: {} refused (OMSI_FAKE_GPU_ERROR=open)", info.name)),
            Some("open-panic") => panic!("test: {} went down while being opened (OMSI_FAKE_GPU_ERROR=open-panic)", info.name),
            _ => {}
        }
        // What the textures may take on this adapter (wgpu does not tell a card's memory):
        // a discrete card is taken for one of 2-4 GB, whose rest the pictures (the render
        // targets, the shadow maps) and the driver need; an integrated one shares the
        // system's memory, Apple's generously
        let mem = adapter.memory_info();
        let vram = adapter_vram_mb(&adapter, &info, mem.as_ref());
        let guess_mb = gpu_memory::texture_allowance_mb(&info, vram, mem.as_ref());
        ADAPTER_TEXTURE_MB.store(guess_mb, std::sync::atomic::Ordering::Relaxed);
        let discrete_vram = gpu_memory::own_card_vram_mb(&info, vram);
        ADAPTER_VRAM_MB.store(discrete_vram, std::sync::atomic::Ordering::Relaxed);
        *ADAPTER_INFO.lock().unwrap_or_else(|e| e.into_inner()) = Some(info.clone());
        log::info!("graphics adapter: {} ({:?}, {:?}{}), texture memory taken for it: {guess_mb} MB", info.name, info.device_type, info.backend, vram.map(|v| format!(", {v} MB of its own")).unwrap_or_default());
        if let Some(m) = mem {
            let mb = |b: Option<u64>| b.map_or_else(|| "-".to_string(), |b| format!("{} MB", b >> 20));
            log::info!("graphics memory as {:?} reports it: {} MB its own, {} MB shared, budget {}, in use {}", info.backend, m.dedicated_bytes >> 20, m.shared_bytes >> 20, mb(m.budget_bytes), mb(m.usage_bytes));
        }
        // The legacy Intel Windows Vulkan branch has repeatedly crashed inside igvk64.dll
        // while compiling the larger multisampled/SSAO pipeline set. This is a driver access
        // violation, so wgpu cannot turn it into a recoverable error. Start those adapters
        // with the conservative feature set instead. OMSI_INTEL_FULL_GPU=1 is useful for
        // retesting after a driver update without rebuilding the game.
        let intel_vulkan_safe = cfg!(windows)
            && info.backend == wgpu::Backend::Vulkan
            && info.vendor == 0x8086
            && !omsi_cfg::flags::OMSI_INTEL_FULL_GPU.is_set();
        let options = if intel_vulkan_safe {
            log::warn!(
                "Intel Vulkan adapter detected ({}): using the stable driver profile (1x MSAA, 1x anisotropy, SSAO and runtime texture compression off); set OMSI_INTEL_FULL_GPU=1 after updating the Intel driver to retry the requested settings",
                info.name
            );
            RenderOptions {
                msaa: 1,
                anisotropy: 1,
                ssao: false,
                compress_textures: false,
                ..options
            }
        } else {
            options
        };
        // a small or shared graphics chip gets a lighter picture (see `lighter_picture`)
        GL_BACKEND.store(info.backend == wgpu::Backend::Gl, std::sync::atomic::Ordering::Relaxed);
        let options = gpu_memory::lighter_picture(&info, vram, options);
        let most = gpu_memory::ceiling(&info, vram);
        *CEILING.lock().unwrap_or_else(|e| e.into_inner()) = Some(if intel_vulkan_safe { gpu_memory::Ceiling { msaa: 1, ssao: false, shadow_size: most.shadow_size.min(2048), ..most } } else { most });
        let shadow_size = options
            .shadow_size
            .clamp(512, if intel_vulkan_safe { 2048 } else { 8192 });
        let mut limits = wgpu::Limits::default().using_resolution(adapter.limits());
        if intel_vulkan_safe {
            // Do not request every large limit the Intel driver advertises. In particular,
            // asking for its maximum storage-buffer and buffer sizes makes 31.0.101.2141
            // crash in vkCreateDevice instead of returning a VkResult. The WebGPU defaults
            // are ample for the renderer and stay on the driver's well-tested path.
            limits = wgpu::Limits::default().using_resolution(adapter.limits());
        } else {
            limits.max_storage_buffer_binding_size =
                adapter.limits().max_storage_buffer_binding_size;
            limits.max_buffer_size = adapter.limits().max_buffer_size;
        }
        // an older or smaller graphics chip (an OpenGL one, a GT 530) does not reach the
        // WebGPU defaults: asked for them anyway, the device was never opened
        if !limits.check_limits(&adapter.limits()) {
            log::warn!("{}: below the standard limits; using what it has", info.name);
            limits = adapter.limits();
        }
        // OMSI_GPU_LIMITS=default|downlevel: the WebGPU defaults (or the downlevel ones) and
        // nothing more, whatever this machine could do - to find what a stricter driver refuses
        match omsi_cfg::flags::OMSI_GPU_LIMITS.var() {
            Some("default") => limits = wgpu::Limits::default(),
            Some("downlevel") => limits = wgpu::Limits::downlevel_defaults(),
            _ => {}
        }
        // the shadow atlas is two maps wide: no wider than the card draws
        let shadow_size = shadow_size.min(limits.max_texture_dimension_2d / 2).max(256);
        let format = format
            .or_else(|| {
                surface.map(|s| {
                    let formats = s.get_capabilities(&adapter).formats;
                    // (an Android driver lists the plain RGBA8 first: the picture, written in
                    // linear light for an sRGB target, came out dark and flat)
                    if cfg!(target_os = "android") {
                        if let Some(f) = formats.iter().find(|f| f.is_srgb()) {
                            return *f;
                        }
                    }
                    formats[0]
                })
            })
            .unwrap_or(wgpu::TextureFormat::Rgba8UnormSrgb);
        // Every target the scene is drawn into with multisampling (the swap chain or mirror
        // format, the HDR target of the enhanced path and its screen mask, the depth buffer)
        // must take the sample count, and the colour targets must resolve. A device judges
        // that by the WebGPU table, which promises only 1x and 4x, unless it was asked for
        // the adapter's own table: the launcher's "2x MSAA" (which Apple GPUs do support)
        // was a fatal validation error before the first frame because the adapter's table
        // said yes and the device's said no. So the adapter's table is asked for when the
        // wanted count needs it (2x, 8x), and the count is checked against the table the
        // device will use.
        let wanted = match options.msaa {
            1 | 2 | 4 | 8 => options.msaa,
            _ => MSAA,
        };
        let targets = [format, wgpu::TextureFormat::Rgba16Float, DEPTH_FORMAT, MASK_FORMAT];
        let takes = |flags: wgpu::TextureFormatFeatureFlags, f: wgpu::TextureFormat, n: u32| {
            flags.sample_count_supported(n)
                && (n == 1
                    || f.is_depth_stencil_format()
                    || flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE))
        };
        let adapter_table_needed = !targets.iter().all(|&f| {
            takes(
                f.guaranteed_format_features(wgpu::Features::empty()).flags,
                f,
                wanted,
            )
        });
        let mut required_features = if adapter_table_needed && info.backend != wgpu::Backend::Noop {
            adapter.features() & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
        } else {
            wgpu::Features::empty()
        };
        if omsi_cfg::flags::OMSI_GPU_TIMERS.is_set() {
            required_features |= adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
        }
        // DXT textures stay compressed on the GPU where it takes them (Apple silicon does);
        // OMSI_NO_BC=1 uploads everything as RGBA (the old way, for comparisons), and so does
        // the settings' `gpu_texture_compression` off
        if !omsi_cfg::flags::OMSI_NO_BC.is_set() && options.gpu_texture_compression {
            required_features |= adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
        } else if !options.gpu_texture_compression {
            log::info!("GPU texture compression off in the settings: DXT/BC textures are decoded to RGBA");
        }
        if intel_vulkan_safe {
            // Keep vkCreateDevice entirely free of optional extensions. Compressed source
            // textures are decoded to RGBA by upload_texture on this device.
            required_features = wgpu::Features::empty();
        }
        // the driver's compiled pipelines kept for the next start (Vulkan, OpenGL)
        if !intel_vulkan_safe {
            required_features |= pipeline_cache::wanted(&adapter);
        }
        // Enhanced+: hardware ray queries where the device has them (Apple silicon from the
        // M3/A17 on, RTX and RDNA 2 cards and newer through Vulkan and Direct3D 12 - with DXC,
        // which the Windows build ships beside the game); OMSI_NO_RT=1 leaves them out. Should
        // the device refuse its work all the same, it falls back to Enhanced (`rt_error`).
        let ray_query = options.ray_tracing
            && !intel_vulkan_safe
            && info.backend != wgpu::Backend::Noop
            && adapter.features().contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY)
            && !omsi_cfg::flags::OMSI_NO_RT.is_set();
        if ray_query {
            required_features |= wgpu::Features::EXPERIMENTAL_RAY_QUERY;
            limits = limits.using_acceleration_structure_values(adapter.limits());
        } else if options.ray_tracing {
            log::warn!("{}: no hardware ray queries; Enhanced+ is drawn as Enhanced", info.name);
        }
        device_selection::select_array_path(&adapter, &limits, &info);
        log::info!("opening graphics device: {} ({:?}, vendor {:#06x}, device {:#06x}), features {:?}, max buffer {} MB, max storage binding {} MB", info.name, info.backend, info.vendor, info.device, required_features, limits.max_buffer_size / 1_000_000, limits.max_storage_buffer_binding_size as u64 / 1_000_000);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("omsi"),
                required_features,
                required_limits: limits,
                // (the allocator's small blocks, 8-64 MB, for every card: a block goes back
                // to the driver only once it is empty, and with the scenery's thousands of
                // textures coming and going a 128-256 MB block of the performance hint hardly
                // ever was. They left hundreds of MB reserved and unused, 2 GB cards lost the
                // device to "Out of memory" in the first frames with the textures well under
                // budget, #332, #295, and a 12 GB card held 13 GB in 63 blocks for 8 GB of
                // textures and buffers and paged, #2295. A few hundred blocks are well under
                // a driver's allocation limit, NVIDIA's 4096.)
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                // (the ray queries are still an experimental feature of wgpu)
                experimental_features: if ray_query { unsafe { wgpu::ExperimentalFeatures::enabled() } } else { wgpu::ExperimentalFeatures::disabled() },
                ..Default::default()
            })
            .await
            .context("request_device")?;
        log::info!("graphics device opened; compiling renderer pipelines");
        pipeline_cache::open(&device, &info);
        // the same choice wgpu-core makes when it validates a texture or a pipeline
        let adapter_table = device
            .features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
            || !adapter
                .get_downlevel_capabilities()
                .flags
                .contains(wgpu::DownlevelFlags::WEBGPU_TEXTURE_FORMAT_SUPPORT);
        let flags_of = |f: wgpu::TextureFormat| {
            if adapter_table {
                adapter.get_texture_format_features(f).flags
            } else {
                f.guaranteed_format_features(device.features()).flags
            }
        };
        let supported = |n: u32| targets.iter().all(|&f| takes(flags_of(f), f, n));
        let wanted = if ray_query && wanted > 4 && !omsi_cfg::flags::OMSI_FULL_GPU.is_set() {
            log::info!("Enhanced+: {wanted}x MSAA drawn at 4x (8x doubles the ray traced passes' cost and their targets' memory)");
            4
        } else {
            wanted
        };
        let msaa = [wanted, 8, 4, 2, 1]
            .into_iter()
            .filter(|&n| n <= wanted)
            .find(|&n| supported(n))
            .unwrap_or(1);
        if let Some(c) = CEILING.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            c.sample_counts = [1, 2, 4, 8].into_iter().filter(|&n| supported(n)).fold(0, |m, n| m | n);
        }
        if msaa != wanted {
            log::warn!("{}x MSAA is not supported by {} (the device takes {:?} samples for {:?}, {:?} for the HDR target, {:?} for depth); using {}x", wanted, info.name, flags_of(format).supported_sample_counts(), format, flags_of(targets[1]).supported_sample_counts(), flags_of(DEPTH_FORMAT).supported_sample_counts(), msaa);
        }
        let options = RenderOptions {
            msaa,
            shadow_size,
            // (16x was held at 8x once, for lane-line dashes far down a road seen to alias
            // into two streaks running apart like an arrow; Spandau's Heerstrasse at 8x and
            // 16x showed none of it - 16x kept the far dashes narrow where 8x smeared them
            // sideways - so 16x is the player's choice again, 8x the default)
            anisotropy: options.anisotropy.clamp(1, 16),
            ray_tracing: ray_query,
            ..options
        };
        RT_BUFFERS.store(ray_query, std::sync::atomic::Ordering::Relaxed);
        let bc = device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
        let compress =
            bc && options.compress_textures && !omsi_cfg::flags::OMSI_NO_TEXCOMPRESS.is_set();
        omsi_texture::set_gpu_options(omsi_texture::GpuOptions { bc, compress });
        log::info!("renderer: {} ({:?}), {:?}, {}x MSAA{}, anisotropy {}, shadow map {}, SSAO {}, render scale {}, textures {}", info.name, info.backend, format, options.msaa, if adapter_table { " (adapter format table)" } else { "" }, options.anisotropy, options.shadow_size, options.ssao, if options.render_scale > 0.0 { format!("{:.2}", options.render_scale.clamp(0.5, 1.0)) } else { "auto".to_string() }, match (bc, compress) { (false, _) => "RGBA (no BC on this device)", (true, false) => "DXT as blocks, others RGBA", (true, true) => "DXT as blocks, others compressed where close" });
        // Every pipeline is built under an error scope of every kind and the renderer built
        // again with less where one fails: without multisampling (a driver whose table
        // promises more than it takes), then without the pipelines a picture can do without
        // (`basic_pipelines`), then both. A phone driver whose shader compiler gives up on a
        // pipeline reports it as an *internal* error, not a validation error: caught as
        // validation errors only, it went by, the pipeline stayed invalid and the first frame
        // stopped on "RenderPipeline with 'omsi' label is invalid" (#1663, #1664 - Adreno
        // 740/830 since 0.2.0), and the retry without multisampling was not checked at all.
        let mesh_pages = adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::BASE_VERTEX);
        let name = format!("{} ({:?})", info.name, info.backend);
        Ok(Opened { adapter, info, device, queue, format, options, asked_options, mesh_pages, name })
    }
}

/// A graphics device opened for a renderer whose pipelines are not built yet
/// ([`Renderer::open_on`]). A window can be drawn into with it already - a first picture, an
/// interface - while [`Opened::finish`] builds them, on another thread if need be: the ~110
/// pipelines took seconds (8 on DirectX 12, which keeps no cache of them for us), and a
/// window shown meanwhile stood blank, or white where it had been smaller.
pub struct Opened {
    pub(crate) adapter: wgpu::Adapter,
    pub(crate) info: wgpu::AdapterInfo,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) options: RenderOptions,
    pub(crate) asked_options: RenderOptions,
    pub(crate) mesh_pages: bool,
    pub(crate) name: String,
}

impl Opened {
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// The format the renderer draws its pictures in (the window's).
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.info
    }

    /// The renderer: its pipelines built on this device (on another one when the device was
    /// lost meanwhile, see below).
    pub async fn finish(self) -> Result<Renderer> {
        let Opened { adapter, info, device, queue, format, options, asked_options, mesh_pages, name } = self;
        let mut made = Renderer::build_with_fallbacks(&device, &queue, &info, &name, format, options, mesh_pages).await;
        // (its own adapter, asked what the game holds on it: with several renderers in one
        // process a shared one asked another's)
        if let Ok(r) = made.as_mut() {
            r.adapter = Some(adapter.clone());
        }
        // A driver whose shader compiler fails on a pipeline answers "out of memory" or an
        // unknown error, and wgpu takes that as the device lost: on phones (Adreno, Mali)
        // one of 0.2's new pipelines did so, the renderer was made all the same and every
        // frame came out black. The device is opened again and the pipelines made without
        // the ones a picture can do without (`basic_pipelines`); lost even so, the next
        // graphics interface is tried.
        // (test hook: OMSI_FAKE_GPU_ERROR=lost-build loses the first device so)
        #[cfg(feature = "test-hooks")]
        if let (Ok(r), Some("lost-build")) = (made.as_ref(), omsi_cfg::flags::OMSI_FAKE_GPU_ERROR.var()) {
            if !basic_pipelines() {
                *r.errors.device_lost.lock().unwrap_or_else(|e| e.into_inner()) = Some("test (OMSI_FAKE_GPU_ERROR=lost-build)".into());
            }
        }
        if let Some(why) = made.as_ref().ok().and_then(|r| r.device_lost()) {
            drop(made);
            pipeline_cache::close(&device);
            // (lost while the chosen MSAA was tried again: what worked before is written down
            // for this version already, and the device opened again keeps to it - not to the
            // basic pipelines at 1x, which it never needed)
            if MSAA_RETRY.swap(false, std::sync::atomic::Ordering::Relaxed) {
                log::warn!("{}: the graphics device was lost while the chosen MSAA was tried again ({why}); keeping to what worked before", info.name);
                return Box::pin(Renderer::new_on(adapter, None, Some(format), asked_options)).await;
            }
            if basic_pipelines() && rainfall_pipeline() {
                log::warn!("{}: the graphics device was lost while the basic pipelines were made ({why}); opening it again without the rainfall", info.name);
                RAINFALL_LEFT_OUT.store(true, std::sync::atomic::Ordering::Relaxed);
                fallback_store(&name, Some((1, true)));
                return Box::pin(Renderer::new_on(adapter, None, Some(format), asked_options)).await;
            }
            if basic_pipelines() {
                return Err(anyhow!("the graphics device was lost while the pipelines were made: {why}"));
            }
            log::warn!("{}: the graphics device was lost while the pipelines were made ({why}); opening it again without the snowfall, the lamps in the fog and the street lamps' shadows", info.name);
            BASIC_PIPELINES.store(true, std::sync::atomic::Ordering::Relaxed);
            // (and so from the start next time, see `fallback_load`)
            fallback_store(&name, Some((1, true)));
            // (the format chosen for the window before: the same the surface would give)
            return Box::pin(Renderer::new_on(adapter, None, Some(format), asked_options)).await;
        }
        // (the retry is over; a device lost later in the run takes the usual way)
        MSAA_RETRY.store(false, std::sync::atomic::Ordering::Relaxed);
        match &made {
            Ok(_) => pipeline_cache::save(),
            Err(_) => pipeline_cache::close(&device),
        }
        made
    }
}

impl Renderer {
    /// Build the renderer on `device`, again with less where a build fails (see `new_on`).
    pub(crate) async fn build_with_fallbacks(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        info: &wgpu::AdapterInfo,
        name: &str,
        format: wgpu::TextureFormat,
        options: RenderOptions,
        mesh_pages: bool,
    ) -> Result<Renderer> {
        // As few builds as can be: each one compiles every shader again, and four of them in
        // a row - a phone whose driver failed the first went through them all - kept the
        // launcher from opening for a minute until Android closed it (#1708, since 0.2.4).
        // What worked on this adapter before is remembered (`fallback_store`) and tried first;
        // after a failure the one most likely to work (no multisampling, basic pipelines).
        let found = fallback_load(name);
        let remembered = found.map(|(m, b, _)| (m, b));
        let mut attempts: Vec<(u32, bool)> = match found {
            // (a computer tries the full set again once with every new version of the game:
            // remembered for good, one failure - a driver of the time, a device lost once -
            // took the snowfall off DirectX 12 for ever; a phone keeps to what worked, its
            // second build took long enough for Android to close the app, #1708)
            Some((m, true, false)) if !cfg!(any(target_os = "android", target_os = "ios")) => vec![(m.min(options.msaa).max(1), false), (m.min(options.msaa).max(1), true), (1, true)],
            // (and the multisampling chosen: one failure of it - a driver of the time, a
            // device lost while building - kept a computer at 1x MSAA for good, whatever the
            // options said, #2597)
            // What worked is written down with this version first, so that the retry is made
            // once per version whatever comes of it: failed, it is not made again at every
            // start; crashing the driver or losing the device, the next start keeps to what
            // worked (see the device-lost path in `new_on`).
            Some((m, b, false)) if !cfg!(any(target_os = "android", target_os = "ios")) && m < options.msaa => {
                fallback_store(name, Some((m, b)));
                MSAA_RETRY.store(true, std::sync::atomic::Ordering::Relaxed);
                vec![(options.msaa, b), (m.max(1), b), (1, true)]
            }
            Some((m, b, _)) => vec![(m.min(options.msaa).max(1), b), (1, true)],
            // (a phone starts with the basic set: the full one failed on Adreno and the
            // second build after it took long enough for Android to close the app)
            None if cfg!(any(target_os = "android", target_os = "ios")) => vec![(options.msaa, true), (1, true)],
            None => vec![(options.msaa, false), (1, false), (1, true)],
        };
        attempts.dedup();
        let mut made: Result<Renderer> = Err(anyhow!("renderer pipelines: not built"));
        let mut tried: Vec<String> = Vec::new();
        for (msaa, basic) in attempts {
            if tried.iter().any(|t| t == &format!("{msaa}{basic}")) || (!basic && basic_pipelines()) {
                continue;
            }
            tried.push(format!("{msaa}{basic}"));
            if basic {
                BASIC_PIPELINES.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            // (the error scopes are the thread's own: pushed and popped where the pipelines
            // are made, on ANGLE the compiler thread - from the calling thread they caught
            // nothing there, and a failed build was taken for a working one)
            let build = || {
                let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
                let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
                let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
                let (renderer, caught) = pipelines::parallel::catching(|| Self::build(device.clone(), queue.clone(), name.to_string(), format, RenderOptions { msaa, ..options }));
                // (test hook: OMSI_FAKE_GPU_ERROR=pipeline fails a pipeline until the basic ones)
                #[cfg(feature = "test-hooks")]
                if !basic && omsi_cfg::flags::OMSI_FAKE_GPU_ERROR.var() == Some("pipeline") {
                    let _ = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("test"), source: wgpu::ShaderSource::Wgsl("fn broken( {".into()) });
                }
                (renderer, [validation.pop(), memory.pop(), internal.pop()], caught)
            };
            let (renderer, scopes, caught) = if is_angle(info) { angle::compile(device, build) } else { build() };
            let [validation, memory, internal] = scopes;
            let errors = [validation.await, memory.await, internal.await];
            match errors.iter().flatten().chain(&caught).next() {
                None => {
                    if msaa != options.msaa || basic {
                        log::warn!("{}: drawing with {}x MSAA{}", info.name, msaa, match (basic, rainfall_pipeline()) {
                            (true, true) => ", without the snowfall, the lamps in the fog and the street lamps' shadows",
                            (true, false) => ", without the snowfall, the rainfall, the lamps in the fog and the street lamps' shadows",
                            _ => "",
                        });
                    }
                    // (a fallback that worked is the first try next time; the full set
                    // working again forgets it)
                    if remembered != Some((msaa, basic)) && (msaa != options.msaa || basic || remembered.is_some()) {
                        fallback_store(name, (msaa != options.msaa || basic).then_some((msaa, basic)));
                    }
                    made = Ok(Renderer { mesh_pages, ..renderer });
                    break;
                }
                Some(e) => {
                    log::error!("{}: the renderer's pipelines failed ({}x MSAA{}): {}", info.name, msaa, if basic { ", basic pipelines" } else { "" }, gpu_error_text(e));
                    made = Err(anyhow!("renderer pipelines: {}", gpu_error_text(e)));
                    drop(renderer);
                }
            }
        }
        made
    }

    /// The pipelines, samplers and fixed textures of a renderer whose options are settled.
    pub(crate) fn build(
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter_name: String,
        format: wgpu::TextureFormat,
        options: RenderOptions,
    ) -> Renderer {
        use pipelines::{coronas, defaults, enhanced, errors, mip, overlays, passes, post, scene, shadows, sky, ssao, upscale};
        let (msaa, shadow_size) = (options.msaa, options.shadow_size);
        RT_GBUF.store(options.ray_tracing, std::sync::atomic::Ordering::Relaxed);
        let errors = errors::install(&device, format, &options);
        let leave_out_enhanced = passes::leave_out_enhanced(&options, &adapter_name);
        if sixteen_texture_units() && !options.no_enhanced && !options.preview_only {
            log::warn!("renderer: the enhanced graphics take more textures than OpenGL has units for on {adapter_name}; drawing vanilla+");
        }
        let scene = scene::SceneBase::new(&device, leave_out_enhanced);
        let hdr_format = wgpu::TextureFormat::Rgba16Float;
        let shadows = shadows::build(&device, &scene, shadow_size, !leave_out_enhanced && !options.no_enhanced);
        let defaults = defaults::build(&device, &queue, options.anisotropy);
        let (coronas, corona_texture) = coronas::Coronas::new(&device, &queue, &scene);
        let precip = crate::passes::PrecipMap::new(&device);
        let snow = coronas::Precip::snow(&device, &scene.camera_layout, &precip.view);
        let rain = coronas::Precip::rain(&device, &scene.camera_layout, &precip.view);
        drop(corona_texture);
        let sky = sky::SkyBase::new(&device, &queue, &scene.camera_layout, !leave_out_enhanced);
        let passes = passes::PassKit { device: &device, scene: &scene, coronas: &coronas, snow: &snow, rain: &rain, sky: &sky, msaa }
            .build(format, hdr_format, leave_out_enhanced);
        let (sky_sampler, sky_mesh) = sky::dome(&device, &queue);
        let overlays = overlays::Overlays::new(&device, format, msaa);
        let gl = GL_BACKEND.load(std::sync::atomic::Ordering::Relaxed);
        let ssao = ssao::ssao(&device, gl);
        let fog_lamps = ssao::fog_lamps(&device, &scene.camera_layout, hdr_format, gl);
        let prepass = scene::prepass(&device, &scene, msaa, !leave_out_enhanced);
        let mip = mip::build(&device);
        let post = post::build(&device, format, hdr_format, &defaults.white_texture);
        let enhanced = enhanced::build(&device, &queue, hdr_format, &defaults.camera_buf, &sky, !leave_out_enhanced);
        let (overlay_pipeline_1x, xr_ui_pipeline) = overlays.single_sampled(&device, format);
        let upscale = upscale::build(&device, format, options.fsr1 || crate::fsr1_enabled());
        // (asked for at the start and refused: not tried again from the menu)
        let fsr_refused = (options.fsr1 || crate::fsr1_enabled()) && upscale.fsr.is_none();
        // (OMSI_FSR1: FSR 1 from the start whatever the settings say - the menu can still turn
        // it off; what the settings asked for is kept, so that only a change of theirs does)
        let upscaler_asked = (options.fsr1, options.fsr_sharpness, options.render_scale);
        let options = RenderOptions { fsr1: (options.fsr1 || crate::fsr1_enabled()) && upscale.fsr.is_some(), ..options };
        let gpu_timers = [GpuTimers::new(&device), GpuTimers::new(&device)];
        // GLES cannot reliably read the depth buffer (the same limitation as SSAO).
        let puddles = (!gl && !passes.leave_out_enhanced).then(|| {
            puddles::Pipelines::new(&device, &scene.shader, &scene.camera_layout, &scene.material_layout)
        });
        let rt = options.ray_tracing.then(|| rt::RayTracer::new(&device));
        let cookie_atlas = cookies::CookieAtlas::new();
        Renderer {
            targets: Targets {
                scale_targets: HashMap::new(),
                triple_targets: None,
                glass_picture: None,
                target_use: HashMap::new(),
                evicted_at: std::time::Instant::now(),
                hdr_targets: HashMap::new(),
                ao: None,
                depth: None,
                msaa_targets: HashMap::new(),
            },
            scale: Upscale {
                upscale_pipeline: upscale.upscale_pipeline,
                fsr: upscale.fsr,
                fsr_target: std::cell::RefCell::new(Vec::new()),
                fsr_refused,
                upscaler_asked,
                fsr_raw_bg: Default::default(),
                fsr_mip_bias_off: Default::default(),
                copy_pipeline: upscale.copy_pipeline,
                panel_pipeline: upscale.panel_pipeline,
                upscale_layout: upscale.layout,
                upscale_buf: upscale.buf,
                dynamic_scale: std::cell::Cell::new(1.0),
            },
            sky: SkyRes {
                sky_lut: enhanced.sky_lut,
                sky_lut_view: enhanced.sky_lut_view,
                sky_state: None,
                city_glow: None,
                sky_job: None,
                sky_mirror_pipeline: passes.sky_mirror_pipeline,
                sky_layout: sky.layout,
                sky_sampler,
                vanilla_sky_buf: sky.vanilla_buf,
                cloud_shape_view: sky.cloud_shape_view,
                cloud_detail_view: sky.cloud_detail_view,
                cloud_sampler: sky.cloud_sampler,
                cloud_shape_cpu: sky.cloud_shape_cpu,
                cloud_sun: None,
                sky_mesh,
            },
            lights: LightRes {
                lm_atlas_view: scene.lm_atlas.create_view(&wgpu::TextureViewDescriptor::default()),
                lm_atlas: scene.lm_atlas,
                lm_uniform: scene.lm_uniform,
                lm_place: std::cell::Cell::new((0.0, 0.0, 0.0)),
                fog_lamps_pipeline: fog_lamps.pipeline,
                fog_lamps_layout: fog_lamps.layout,
                fog_lamps_buf: fog_lamps.buf,
                corona_bind_group: coronas.bind_group,
                snow_buf: snow.buf,
                snow_bind_group: snow.bind_group,
                rain_buf: rain.buf,
                rain_bind_group: rain.bind_group,
                smoke_bind_group: coronas.smoke_bind_group,
                corona_textures: Vec::new(),
                pictures_taken: (0, 0),
                cookies: cookie_atlas,
                corona_layout: scene.corona_layout,
                corona_sampler: coronas.sampler,
            },
            tone: ToneMapping {
                post: post.pipelines,
                post_layout: post.layout,
                post_buf: post.buf,
                post_sampler: post.sampler,
                meter_view: post.meter_view,
                adapt_views: post.adapt_views,
                adapt_bg: post.adapt_bg,
                adapt_front: 0,
                exposure_log: ExposureLog::new(&device),
                lamp_sight: LampSight::new(&device, leave_out_enhanced),
                enh_buf: enhanced.enh_buf,
                lin_sampler: enhanced.lin_sampler,
                probe: enhanced.probe,
                view_lamps: None,
                exposure: None,
                enh_last: bytemuck::Zeroable::zeroed(),
                lightning: crate::lighting::Lightning::new(7),
                flash: Default::default(),
                bounce_prev: None,
                bounce_reproj: (Mat4::ZERO.to_cols_array_2d(), 0.0),
            },
            shadows: ShadowRes {
                shadow_far_cache: std::cell::Cell::new((Mat4::IDENTITY, 0, DVec3::ZERO, Vec3::ZERO)),
                shadow_near_cache: std::cell::Cell::new((Mat4::IDENTITY, 0, DVec3::ZERO, Vec3::ZERO)),
                shadow_close_cache: std::cell::Cell::new((Mat4::IDENTITY, DVec3::ZERO, Vec3::ZERO)),
                xr_shadow_cache: std::cell::Cell::new(None),
                shadow_clear_pipeline: shadows.clear_pipeline,
                lamp_tiles: Default::default(),
                cabin_motion: Default::default(),
                precip,
                shadow_view: shadows.view,
                shadow_view_far: shadows.view_far,
                lamp_atlas: shadows.lamp_atlas,
                lamp_matrices: std::cell::RefCell::new(vec![Mat4::ZERO; LAMP_SHADOWS]),
                shadow_sampler: shadows.sampler,
                shadow_layout: scene.shadow_layout,
                shadow_pipelines: shadows.pipelines,
            },
            history: ViewHistory {
                triple_culling: Default::default(),
                flicker: std::cell::RefCell::new(HashMap::new()),
                main_bundles: Default::default(),
                mirror_bundles: Default::default(),
                bundle_stats: Default::default(),
                cull_drawn: std::cell::RefCell::new(Vec::new()),
                object_sizes: Default::default(),
                object_sizes_scratch: Default::default(),
            },
            errors: GpuErrors {
                gpu_error: errors.gpu_error,
                rt_error: errors.rt_error,
                out_of_memory: errors.out_of_memory,
                device_lost: errors.device_lost,
            },
            _device_poller: DevicePoller::start(&device),
            mip_pipeline: mip.pipeline,
            mip_text_pipeline: mip.text_pipeline,
            mip_layout: mip.layout,
            mip_sampler: mip.sampler,
            clamp_sampler: defaults.clamp_sampler,
            mirror_sampler: defaults.mirror_sampler,
            address_next: std::cell::Cell::new(TexAddressing::Wrap),
            light_map_next: std::cell::Cell::new(false),
            puddles,
            texture_aspect: None,
            last_frame: None,
            adapter: None,
            gpu_memory_seen: Default::default(),
            instant_exposure: false,
            depth_readback: false,
            overlay_pipeline_1x,
            xr_ui_pipeline,
            // (OMSI_RENDER_CLOCK=<s>: the animations' clock starts that far on - offscreen
            // stills of moving things, the windy trees, at different moments)
            started: std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs_f64(omsi_cfg::flags::OMSI_RENDER_CLOCK.var().and_then(|v| v.trim().parse::<f64>().ok()).filter(|s| s.is_finite() && *s >= 0.0).unwrap_or(0.0)))
                .unwrap_or_else(std::time::Instant::now),
            tree_gust_drift: std::cell::Cell::new((glam::DVec2::ZERO, 0.0)),
            ao_sampler: ssao.sampler,
            ao_layout: ssao.layout,
            ao_buf: ssao.buf,
            prepass_pipelines: prepass.pipelines,
            prepass_msaa_pipelines: prepass.msaa_pipelines,
            ssao_pipeline: ssao.ssao_pipeline,
            blur_pipeline: ssao.blur_pipeline,
            plain_ssao_pipeline: ssao.plain_ssao_pipeline,
            plain_blur_pipeline: ssao.plain_blur_pipeline,
            device,
            queue,
            adapter_name,
            camera_layout: scene.camera_layout,
            material_layout: scene.material_layout,
            pass: passes.pass,
            hdr_pass: passes.hdr_pass,
            reflection_pass: passes.reflection_pass,
            overlay_pipeline: overlays.pipeline,
            overlay_layout: overlays.layout,
            sampler: defaults.sampler,
            camera_buf: defaults.camera_buf,
            white_texture: defaults.white_texture,
            black_texture: defaults.black_texture,
            flat_normal_texture: defaults.flat_normal_texture,
            format,
            shadow_blobs: options.shadow_blobs,
            options,
            env_heading: Default::default(),
            blend_by_origin: false,
            gpu_timers,
            pictures: Default::default(),
            stats: Default::default(),
            counts: Default::default(),
            frame_draws: (0, 0),
            profiling: omsi_cfg::flags::OMSI_PROFILE.is_set(),
            draw_audit_at: std::time::Instant::now(),
            encoding_pool: if omsi_cfg::flags::OMSI_NO_RENDER_POOL.is_set() {
                None
            } else {
                let workers = std::thread::available_parallelism().map(|n| n.get() / 2).unwrap_or(2).clamp(2, 8);
                rayon::ThreadPoolBuilder::new().num_threads(workers)
                    .thread_name(|i| format!("omsi-render-{i}"))
                    .build().ok()
            },
            pending_meshes: Default::default(),
            freed: std::cell::OnceCell::new(),
            rt,
            mesh_pages: false,
        }
    }
}
