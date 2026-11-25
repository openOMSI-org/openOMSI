//! Opening the graphics device and building the renderer on it, with the fallbacks.

use super::upscaler_device;
use crate::*;

/// The chosen MSAA is being tried again after a remembered fallback (see `build_with_fallbacks`).
/// How many times this thread published the adapter's state, its texture options and
/// its memory and name (tests: other tests open
/// renderers on their own threads at the same time, so the global value alone races).
#[cfg(test)]
thread_local! {
    pub(crate) static ADAPTER_PUBLISHED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

static MSAA_RETRY: super::auxiliary::AdapterBool = super::auxiliary::AdapterBool::new(5);

impl Renderer {
    /// The adapter used by this renderer. Auxiliary devices must reuse it, not select again.
    pub fn adapter(&self) -> Option<mx::Adapter> {
        self.adapter.clone()
    }

    /// Create a renderer with the default options. `surface` is used to pick a compatible
    /// adapter and format.
    pub async fn new(
        instance: &mx::Instance,
        surface: Option<&mx::Surface<'_>>,
        format: Option<mx::TextureFormat>,
    ) -> Result<Renderer> {
        Self::new_with(instance, surface, format, RenderOptions::default()).await
    }

    /// Create a renderer with the given graphics settings.
    pub async fn new_with(
        instance: &mx::Instance,
        surface: Option<&mx::Surface<'_>>,
        format: Option<mx::TextureFormat>,
        options: RenderOptions,
    ) -> Result<Renderer> {
        let adapter = surface
            .map(|s| Self::adapters_for(instance, s))
            .unwrap_or_else(|| instance.enumerate_adapters())
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no graphics adapter that can draw the game was found (Metal, Vulkan, DirectX 12 or OpenGL 3.3 or later); updating the graphics driver often helps"))?;
        Self::new_on(adapter, surface, format, options).await
    }

    /// The adapters of `instance` that can show `surface`, the ones worth trying first first:
    /// a graphics card of its own, then the processor's graphics, then anything else (a
    /// software renderer last).
    pub fn adapters_for(instance: &mx::Instance, surface: &mx::Surface<'_>) -> Vec<mx::Adapter> {
        let mut v: Vec<mx::Adapter> = instance
            .enumerate_adapters()
            .into_iter()
            .filter(|a| surface.is_supported_by(a))
            .collect();
        let rank = |a: &mx::Adapter| match a.info().device_type {
            mx::DeviceType::DiscreteGpu => 0,
            mx::DeviceType::IntegratedGpu => 1,
            mx::DeviceType::VirtualGpu | mx::DeviceType::Other => 2,
            mx::DeviceType::Cpu => 3,
        };
        v.sort_by_key(rank);
        v
    }

    /// Create a renderer on this adapter.
    pub async fn new_on(
        adapter: mx::Adapter,
        surface: Option<&mx::Surface<'_>>,
        format: Option<mx::TextureFormat>,
        options: RenderOptions,
    ) -> Result<Renderer> {
        Self::open_on(adapter, surface, format, options)
            .await?
            .finish()
            .await
    }

    /// [`Renderer::new_on`] again on the adapter whose device was just lost, keeping what the
    /// loss took off (the basic pipelines, the rainfall, the MSAA retry): `open_on` starts a
    /// new adapter or interface without them.
    async fn reopen_on(
        adapter: mx::Adapter,
        format: mx::TextureFormat,
        options: RenderOptions,
    ) -> Result<Renderer> {
        Self::open_device(adapter, None, Some(format), options)
            .await?
            .finish()
            .await
    }

    /// The graphics device for a renderer on this adapter, its pipelines not built yet: see
    /// [`Opened`].
    pub async fn open_on(
        adapter: mx::Adapter,
        surface: Option<&mx::Surface<'_>>,
        format: Option<mx::TextureFormat>,
        options: RenderOptions,
    ) -> Result<Opened> {
        // What one device had to do without is that device's: the flags are process-wide, and
        // Direct3D 12 losing its device to ray tracing set the basic pipelines for the Vulkan
        // device the game went on with - drawn at 1x MSAA, without the snowfall, the lamps in
        // the fog and the street lamps' shadows, and remembered so for Vulkan as well.
        // (What worked on this adapter before comes back from `fallback_load`.)
        BASIC_PIPELINES.store(false, std::sync::atomic::Ordering::Relaxed);
        RAINFALL_LEFT_OUT.store(false, std::sync::atomic::Ordering::Relaxed);
        MSAA_RETRY.store(false, std::sync::atomic::Ordering::Relaxed);
        Self::open_device(adapter, surface, format, options).await
    }

    async fn open_device(
        adapter: mx::Adapter,
        surface: Option<&mx::Surface<'_>>,
        format: Option<mx::TextureFormat>,
        options: RenderOptions,
    ) -> Result<Opened> {
        let info = adapter.info();
        let asked_options = options;
        // test hooks for an adapter that cannot be opened: an error, or wgpu going down
        #[cfg(feature = "test-hooks")]
        match omsi_cfg::flags::OMSI_FAKE_GPU_ERROR.var() {
            Some("open") => {
                return Err(anyhow!(
                    "test: {} refused (OMSI_FAKE_GPU_ERROR=open)",
                    info.name
                ));
            }
            Some("open-panic") => panic!(
                "test: {} went down while being opened (OMSI_FAKE_GPU_ERROR=open-panic)",
                info.name
            ),
            _ => {}
        }
        // What the textures may take on this adapter (wgpu does not tell a card's memory):
        // a discrete card is taken for one of 2-4 GB, whose rest the pictures (the render
        // targets, the shadow maps) and the driver need; an integrated one shares the
        // system's memory, Apple's generously
        let mem = adapter.memory_info();
        let vram = adapter_vram_mb(&info, mem.as_ref());
        let guess_mb = gpu_memory::texture_allowance_mb(&info, vram, mem.as_ref());
        let discrete_vram = gpu_memory::own_card_vram_mb(&info, vram);
        log::info!(
            "graphics adapter: {} ({:?}, {:?}{}), texture memory taken for it: {guess_mb} MB",
            info.name,
            info.device_type,
            info.backend,
            vram.map(|v| format!(", {v} MB of its own"))
                .unwrap_or_default()
        );
        if let Some(m) = mem {
            let mb =
                |b: Option<u64>| b.map_or_else(|| "-".to_string(), |b| format!("{} MB", b >> 20));
            log::info!(
                "graphics memory as {:?} reports it: {} MB its own, budget {}",
                info.backend,
                m.dedicated_bytes >> 20,
                mb(m.budget_bytes)
            );
        }
        // The legacy Intel Windows Vulkan branch has repeatedly crashed inside igvk64.dll
        // while compiling the larger multisampled/SSAO pipeline set. This is a driver access
        // violation, so wgpu cannot turn it into a recoverable error. Start those adapters
        // with the conservative feature set instead. OMSI_INTEL_FULL_GPU=1 is useful for
        // retesting after a driver update without rebuilding the game.
        let intel_vulkan_safe = cfg!(windows)
            && info.backend == mx::Backend::Vulkan
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
        GL_BACKEND.store(
            info.backend == mx::Backend::Gl,
            std::sync::atomic::Ordering::Relaxed,
        );
        AMD_VULKAN.store(
            info.backend == mx::Backend::Vulkan && info.vendor == 0x1002,
            std::sync::atomic::Ordering::Relaxed,
        );
        // (per-sample shading: every bindless backend of mX has it)
        MULTISAMPLED_SHADING.store(
            !cfg!(any(target_os = "android", target_vendor = "apple"))
                && info.backend != mx::Backend::Gl,
            std::sync::atomic::Ordering::Relaxed,
        );
        let options = gpu_memory::lighter_picture(&info, vram, options);
        // Temporal upscaling (DirectX 12 or Vulkan, DLSS an NVIDIA RTX card too; MetalFX on
        // Metal) replaces the MSAA. DLSS on Vulkan starts Streamline now: the device is made with its extensions.
        let temporal = upscaler_device::usable(options.temporal, &info);
        let (temporal, dlss_extensions) = upscaler_device::dlss_vulkan(temporal, &adapter, &info);
        let mut options = match temporal {
            Some(t) => RenderOptions {
                msaa: 1,
                msaa_fallback: options.msaa,
                temporal: Some(t),
                ..options
            },
            None => RenderOptions {
                temporal: None,
                ..options
            },
        };
        let most = gpu_memory::ceiling(&info, vram);
        update_ceiling(|c| {
            *c = Some(if intel_vulkan_safe {
                gpu_memory::Ceiling {
                    msaa: 1,
                    ssao: false,
                    shadow_size: most.shadow_size.min(2048),
                    ..most
                }
            } else {
                most
            })
        });
        let shadow_size = options
            .shadow_size
            .clamp(512, if intel_vulkan_safe { 2048 } else { 8192 });
        // (mX opens every device with all it has: no limits or features to ask for)
        // OMSI_GPU_LIMITS=default|downlevel: the device keeps to the common limits (or an older
        // chip's) and nothing more, whatever this machine could do - to find what a stricter
        // driver refuses
        let cap = match omsi_cfg::flags::OMSI_GPU_LIMITS.var() {
            Some("default") => Some(mx::Limits::default()),
            Some("downlevel") => Some(mx::Limits::downlevel()),
            _ => None,
        };
        let limits = adapter.limits().capped(cap.as_ref());
        // the shadow atlas is two maps wide: no wider than the card draws
        let shadow_size = shadow_size
            .min(limits.max_texture_dimension_2d / 2)
            .max(256);
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
            .unwrap_or(mx::TextureFormat::Rgba8UnormSrgb);
        // Every target the scene is drawn into with multisampling (the swap chain or mirror
        // format, the HDR target of the enhanced path and its screen mask, the depth buffer)
        // must take the sample count, and the colour targets must resolve.
        let wanted = match options.msaa {
            1 | 2 | 4 | 8 => options.msaa,
            _ => MSAA,
        };
        let targets = [
            format,
            mx::TextureFormat::Rgba16Float,
            DEPTH_FORMAT,
            MASK_FORMAT,
        ];
        let supported = |n: u32| targets.iter().all(|&f| adapter.supports_msaa(f, n));
        // DXT textures stay compressed on the GPU where it takes them (Apple silicon does);
        // OMSI_NO_BC=1 uploads everything as RGBA (the old way, for comparisons), and so does
        // the settings' `gpu_texture_compression` off; the Intel Vulkan profile decodes them
        let bc = adapter.supports_bc()
            && !omsi_cfg::flags::OMSI_NO_BC.is_set()
            && options.gpu_texture_compression
            && !intel_vulkan_safe;
        if !options.gpu_texture_compression {
            log::info!(
                "GPU texture compression off in the settings: DXT/BC textures are decoded to RGBA"
            );
        }
        // Enhanced+: hardware ray queries where the device has them (Apple silicon from the
        // M3/A17 on, RTX and RDNA 2 cards and newer through Vulkan and Direct3D 12 - with DXC,
        // which the Windows build ships beside the game); OMSI_NO_RT=1 leaves them out. Should
        // the device refuse its work all the same, it falls back to Enhanced (`rt_error`).
        let ray_query = options.ray_tracing
            && !intel_vulkan_safe
            && adapter.supports_ray_query()
            && !omsi_cfg::flags::OMSI_NO_RT.is_set();
        if options.ray_tracing && !ray_query {
            log::warn!(
                "{}: no hardware ray queries; Enhanced+ is drawn as Enhanced",
                info.name
            );
        }
        device_selection::select_array_path(&adapter, &info);
        log::info!(
            "opening graphics device: {} ({:?}, vendor {:#06x}, device {:#06x}), BC {bc}, ray queries {ray_query}",
            info.name,
            info.backend,
            info.vendor,
            info.device
        );
        // (DLSS on Vulkan: its device extensions beside mX's own)
        let open = |vulkan_extensions: &[&'static std::ffi::CStr]| {
            adapter.request_device(&mx::DeviceDescriptor {
                label: Some("omsi"),
                vulkan_extensions,
                limits: cap,
                ..Default::default()
            })
        };
        let (device, queue) =
            match dlss_extensions.map(|extensions| (open(&extensions), extensions)) {
                Some((Ok(made), _)) => made,
                // (the device as mX makes it, DLSS off: better than no picture)
                Some((Err(e), _)) => {
                    crate::temporal::dlss_vulkan_abandon();
                    log::warn!(
                        "dlss upscaling is off: the device with its extensions did not open ({e})"
                    );
                    options.temporal = None;
                    options.msaa = options.msaa_fallback.max(1);
                    open(&[]).context("opening the graphics device")?
                }
                None => open(&[]).context("opening the graphics device")?,
            };
        log::info!("graphics device opened; compiling renderer pipelines");
        if auxiliary_context().is_none() {
            #[cfg(test)]
            ADAPTER_PUBLISHED.with(|n| n.set(n.get() + 1));
            ADAPTER_TEXTURE_MB.store(guess_mb, std::sync::atomic::Ordering::Relaxed);
            ADAPTER_VRAM_MB.store(discrete_vram, std::sync::atomic::Ordering::Relaxed);
            *ADAPTER_INFO.lock().unwrap_or_else(|e| e.into_inner()) = Some(info.clone());
        }
        let wanted = if ray_query && wanted > 4 && !omsi_cfg::flags::OMSI_FULL_GPU.is_set() {
            log::info!(
                "Enhanced+: {wanted}x MSAA drawn at 4x (8x doubles the ray traced passes' cost and their targets' memory)"
            );
            4
        } else {
            wanted
        };
        let msaa = [wanted, 8, 4, 2, 1]
            .into_iter()
            .filter(|&n| n <= wanted)
            .find(|&n| supported(n))
            .unwrap_or(1);
        update_ceiling(|ceiling| {
            if let Some(c) = ceiling.as_mut() {
                c.sample_counts = [1, 2, 4, 8]
                    .into_iter()
                    .filter(|&n| supported(n))
                    .fold(0, |m, n| m | n);
            }
        });
        if msaa != wanted {
            log::warn!(
                "{wanted}x MSAA is not supported by {} for {format:?}, the HDR target and depth; using {msaa}x",
                info.name
            );
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
        let compress =
            bc && options.compress_textures && !omsi_cfg::flags::OMSI_NO_TEXCOMPRESS.is_set();
        let halve = halves_plain(adapter.supports_bc(), &options);
        if auxiliary_context().is_none() {
            omsi_texture::set_gpu_options(omsi_texture::GpuOptions { bc, compress });
            #[cfg(test)]
            ADAPTER_PUBLISHED.with(|n| n.set(n.get() + 1));
            omsi_texture::gpu::set_halve_plain(halve);
        }
        log::info!(
            "renderer: {} ({:?}), {:?}, {}x MSAA, anisotropy {}, shadow map {}, SSAO {}, render scale {}, textures {}",
            info.name,
            info.backend,
            format,
            options.msaa,
            options.anisotropy,
            options.shadow_size,
            options.ssao,
            if options.render_scale > 0.0 {
                format!("{:.2}", options.render_scale.clamp(0.5, 1.0))
            } else {
                "auto".to_string()
            },
            match (bc, compress) {
                (false, _) if halve => "RGBA at half size (no BC on this device)",
                (false, _) => "RGBA (no BC on this device)",
                (true, false) => "DXT as blocks, others RGBA",
                (true, true) => "DXT as blocks, others compressed where close",
            }
        );
        let mesh_pages = adapter.supports_base_vertex();
        let name = format!("{} ({:?})", info.name, info.backend);
        Ok(Opened {
            adapter,
            info,
            device,
            queue,
            format,
            options,
            asked_options,
            mesh_pages,
            name,
        })
    }
}

/// Only a chip with no BC at all (a phone's), not one whose BC a setting turned off.
fn halves_plain(adapter_bc: bool, options: &RenderOptions) -> bool {
    !adapter_bc && options.compress_textures && !omsi_cfg::flags::OMSI_NO_TEXCOMPRESS.is_set()
}

/// A graphics device opened for a renderer whose pipelines are not built yet
/// ([`Renderer::open_on`]). A window can be drawn into with it already - a first picture, an
/// interface - while [`Opened::finish`] builds them, on another thread if need be: the ~110
/// pipelines took seconds (8 on DirectX 12, which keeps no cache of them for us), and a
/// window shown meanwhile stood blank, or white where it had been smaller.
pub struct Opened {
    pub(crate) adapter: mx::Adapter,
    pub(crate) info: mx::AdapterInfo,
    pub(crate) device: mx::Device,
    pub(crate) queue: mx::Queue,
    pub(crate) format: mx::TextureFormat,
    pub(crate) options: RenderOptions,
    pub(crate) asked_options: RenderOptions,
    pub(crate) mesh_pages: bool,
    pub(crate) name: String,
}

impl Opened {
    pub fn device(&self) -> &mx::Device {
        &self.device
    }

    pub fn queue(&self) -> &mx::Queue {
        &self.queue
    }

    /// The format the renderer draws its pictures in (the window's).
    pub fn format(&self) -> mx::TextureFormat {
        self.format
    }

    pub fn adapter_info(&self) -> &mx::AdapterInfo {
        &self.info
    }

    /// The renderer: its pipelines built on this device (on another one when the device was
    /// lost meanwhile, see below).
    pub async fn finish(self) -> Result<Renderer> {
        let Opened {
            adapter,
            info,
            device,
            queue,
            format,
            options,
            asked_options,
            mesh_pages,
            name,
        } = self;
        let mut made = Renderer::build_with_fallbacks(
            &device, &queue, &info, &name, format, options, mesh_pages,
        )
        .await;
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
        if let (Ok(r), Some("lost-build")) =
            (made.as_ref(), omsi_cfg::flags::OMSI_FAKE_GPU_ERROR.var())
        {
            if !basic_pipelines() {
                *r.errors
                    .device_lost
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) =
                    Some("test (OMSI_FAKE_GPU_ERROR=lost-build)".into());
            }
        }
        if let Some(why) = made.as_ref().ok().and_then(|r| r.device_lost()) {
            drop(made);
            // (lost while the chosen MSAA was tried again: what worked before is written down
            // for this version already, and the device opened again keeps to it - not to the
            // basic pipelines at 1x, which it never needed)
            if MSAA_RETRY.swap(false, std::sync::atomic::Ordering::Relaxed) {
                log::warn!(
                    "{}: the graphics device was lost while the chosen MSAA was tried again ({why}); keeping to what worked before",
                    info.name
                );
                return Box::pin(Renderer::reopen_on(adapter, format, asked_options)).await;
            }
            if basic_pipelines() && rainfall_pipeline() {
                log::warn!(
                    "{}: the graphics device was lost while the basic pipelines were made ({why}); opening it again without the rainfall and the snowfall",
                    info.name
                );
                RAINFALL_LEFT_OUT.store(true, std::sync::atomic::Ordering::Relaxed);
                fallback_store(&name, Some((1, true)));
                return Box::pin(Renderer::reopen_on(adapter, format, asked_options)).await;
            }
            if basic_pipelines() {
                return Err(anyhow!(
                    "the graphics device was lost while the pipelines were made: {why}"
                ));
            }
            log::warn!(
                "{}: the graphics device was lost while the pipelines were made ({why}); opening it again without the snowfall, the lamps in the fog and the street lamps' shadows",
                info.name
            );
            BASIC_PIPELINES.store(true, std::sync::atomic::Ordering::Relaxed);
            // (and so from the start next time, see `fallback_load`)
            fallback_store(&name, Some((1, true)));
            // (the format chosen for the window before: the same the surface would give)
            return Box::pin(Renderer::reopen_on(adapter, format, asked_options)).await;
        }
        // (the retry is over; a device lost later in the run takes the usual way)
        MSAA_RETRY.store(false, std::sync::atomic::Ordering::Relaxed);
        made
    }
}

impl Renderer {
    /// Build the renderer on `device`, again with less where a build fails (see `new_on`).
    pub(crate) async fn build_with_fallbacks(
        device: &mx::Device,
        queue: &mx::Queue,
        info: &mx::AdapterInfo,
        name: &str,
        format: mx::TextureFormat,
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
            Some((m, true, false)) if !cfg!(any(target_os = "android", target_os = "ios")) => vec![
                (m.min(options.msaa).max(1), false),
                (m.min(options.msaa).max(1), true),
                (1, true),
            ],
            // (and the multisampling chosen: one failure of it - a driver of the time, a
            // device lost while building - kept a computer at 1x MSAA for good, whatever the
            // options said, #2597)
            // What worked is written down with this version first, so that the retry is made
            // once per version whatever comes of it: failed, it is not made again at every
            // start; crashing the driver or losing the device, the next start keeps to what
            // worked (see the device-lost path in `new_on`).
            Some((m, b, false))
                if !cfg!(any(target_os = "android", target_os = "ios")) && m < options.msaa =>
            {
                fallback_store(name, Some((m, b)));
                MSAA_RETRY.store(true, std::sync::atomic::Ordering::Relaxed);
                vec![(options.msaa, b), (m.max(1), b), (1, true)]
            }
            Some((m, b, _)) => vec![(m.min(options.msaa).max(1), b), (1, true)],
            // (a phone starts with the basic set: the full one failed on Adreno and the
            // second build after it took long enough for Android to close the app)
            None if cfg!(any(target_os = "android", target_os = "ios")) => {
                vec![(options.msaa, true), (1, true)]
            }
            None => vec![(options.msaa, false), (1, false), (1, true)],
        };
        attempts.dedup();
        let mut made: Result<Renderer> = Err(anyhow!("renderer pipelines: not built"));
        let mut tried: Vec<String> = Vec::new();
        for (msaa, basic) in attempts {
            if tried.iter().any(|t| t == &format!("{msaa}{basic}")) || (!basic && basic_pipelines())
            {
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
                gpu_attempt(|| {
                    let renderer = Self::build(
                        device.clone(),
                        queue.clone(),
                        name.to_string(),
                        format,
                        RenderOptions { msaa, ..options },
                    );
                    // (test hook: OMSI_FAKE_GPU_ERROR=pipeline fails a pipeline until the basic ones)
                    #[cfg(feature = "test-hooks")]
                    if !basic && omsi_cfg::flags::OMSI_FAKE_GPU_ERROR.var() == Some("pipeline") {
                        let _ = device
                            .create_shader_module(&mx::ShaderModuleDescriptor {
                                label: Some("test"),
                                source: "fn broken( {",
                            })
                            .expect("mX: a shader");
                    }
                    renderer
                })
            };
            let attempt = if is_angle(info) {
                angle::compile(device, build)
            } else {
                build()
            };
            match attempt {
                Ok(renderer) => {
                    if msaa != options.msaa || basic {
                        log::warn!(
                            "{}: drawing with {}x MSAA{}",
                            info.name,
                            msaa,
                            match (basic, rainfall_pipeline(), snowfall_pipeline()) {
                                (true, true, true) =>
                                    ", without the lamps in the fog and the street lamps' shadows",
                                (true, true, false) =>
                                    ", without the snowfall, the lamps in the fog and the street lamps' shadows",
                                (true, false, _) =>
                                    ", without the snowfall, the rainfall, the lamps in the fog and the street lamps' shadows",
                                _ => "",
                            }
                        );
                    }
                    // (a fallback that worked is the first try next time; the full set
                    // working again forgets it)
                    if remembered != Some((msaa, basic))
                        && (msaa != options.msaa || basic || remembered.is_some())
                    {
                        fallback_store(
                            name,
                            (msaa != options.msaa || basic).then_some((msaa, basic)),
                        );
                    }
                    made = Ok(Renderer {
                        mesh_pages,
                        ..renderer
                    });
                    break;
                }
                Err(e) => {
                    log::error!(
                        "{}: the renderer's pipelines failed ({}x MSAA{}): {e}",
                        info.name,
                        msaa,
                        if basic { ", basic pipelines" } else { "" }
                    );
                    made = Err(anyhow!("renderer pipelines: {e}"));
                    // (a removed device fails every build after it the same way: the next
                    // graphics interface is tried at once, rather than the basic pipelines on
                    // the dead device - which took the reduction to that interface too)
                    if device.is_lost() {
                        log::error!(
                            "{}: the graphics device was lost while the pipelines were made",
                            info.name
                        );
                        break;
                    }
                }
            }
        }
        made
    }

    /// The pipelines, samplers and fixed textures of a renderer whose options are settled.
    pub(crate) fn build(
        device: mx::Device,
        queue: mx::Queue,
        adapter_name: String,
        format: mx::TextureFormat,
        options: RenderOptions,
    ) -> Renderer {
        use pipelines::{
            coronas, defaults, enhanced, errors, mip, overlays, passes, post, scene, shadows, sky,
            ssao, upscale,
        };
        let (msaa, shadow_size) = (options.msaa, options.shadow_size);
        RT_GBUF.store(options.ray_tracing, std::sync::atomic::Ordering::Relaxed);
        let errors = errors::install(&device, format, &options);
        let leave_out_enhanced = passes::leave_out_enhanced(&options, &adapter_name);
        if sixteen_texture_units() && !options.no_enhanced && !options.preview_only {
            log::warn!(
                "renderer: the enhanced graphics take more textures than OpenGL has units for on {adapter_name}; drawing vanilla+"
            );
        }
        let scene = scene::SceneBase::new(&device, leave_out_enhanced, options.detailed_trees);
        let hdr_format = mx::TextureFormat::Rgba16Float;
        let shadows = shadows::build(
            &device,
            &scene,
            shadow_size,
            !leave_out_enhanced && !options.no_enhanced,
            options.light_shadows >= 2,
        );
        if shadows.size < shadow_size {
            update_ceiling(|ceiling| {
                if let Some(c) = ceiling.as_mut() {
                    c.shadow_size = c.shadow_size.min(shadows.size);
                }
            });
        }
        let options = RenderOptions {
            shadow_size: shadows.size,
            ..options
        };
        let defaults = defaults::build(&device, &queue, options.anisotropy);
        let (coronas, corona_texture) = coronas::Coronas::new(&device, &queue, &scene);
        let precip = crate::passes::PrecipMap::new(&device);
        let flakes = crate::precip::FlakeAtlas::new(&device, &queue);
        let snow = coronas::Precip::snow(&device, &precip.view, &flakes);
        let rain = coronas::Precip::rain(&device, &precip.view, &flakes);
        drop(corona_texture);
        let sky = sky::SkyBase::new(&device, &queue, !leave_out_enhanced);
        let sky_pipelines = sky.pipeline_base();
        let kit = passes::PassKit {
            device: &device,
            scene: &scene,
            coronas: &coronas,
            snow: &snow,
            rain: &rain,
            sky: &sky_pipelines,
            msaa,
        };
        let eye_pipeline_kit =
            (!leave_out_enhanced && options.ray_tracing).then(|| passes::EyePipelineKit::new(&kit));
        let passes = kit.build(format, hdr_format, leave_out_enhanced);
        let (sky_sampler, sky_mesh) = sky::dome(&device, &queue);
        let overlays = overlays::Overlays::new(&device, format, msaa);
        let gl = GL_BACKEND.load(std::sync::atomic::Ordering::Relaxed);
        let ssao = ssao::ssao(&device, gl);
        let fog_lamps = ssao::fog_lamps(&device, hdr_format, gl);
        let prepass = scene::prepass(&device, &scene, msaa, !leave_out_enhanced);
        // (the prepass with motion vectors: only when temporal upscaling is asked for)
        let temporal_pipes =
            pipelines::temporal::build(&device, &scene, options.temporal.map(|t| t.upscaler));
        let ground_pipelines = (!gl && !leave_out_enhanced && !sixteen_texture_units())
            .then(|| std::array::from_fn(|i| scene.ground_pipeline_at(&device, i)));
        let mip = mip::build(&device);
        let post = post::build(&device, format, hdr_format, &defaults.white_texture);
        let enhanced = enhanced::build(
            &device,
            &queue,
            hdr_format,
            &defaults.camera_buf,
            &sky,
            !leave_out_enhanced,
        );
        let (overlay_pipeline_1x, xr_ui_pipeline) = overlays.single_sampled(&device, format);
        let upscale = upscale::build(&device, format, options.fsr1 || crate::fsr1_enabled());
        // (asked for at the start and refused: not tried again from the menu)
        let fsr_refused = (options.fsr1 || crate::fsr1_enabled()) && upscale.fsr.is_none();
        // (OMSI_FSR1: FSR 1 from the start whatever the settings say - the menu can still turn
        // it off; what the settings asked for is kept, so that only a change of theirs does)
        let upscaler_asked = (options.fsr1, options.fsr_sharpness, options.render_scale);
        let options = RenderOptions {
            fsr1: (options.fsr1 || crate::fsr1_enabled()) && upscale.fsr.is_some(),
            ..options
        };
        let gpu_timers = std::array::from_fn(|_| GpuTimers::new(&device));
        // GLES cannot reliably read the depth buffer (the same limitation as SSAO).
        let puddles = (!gl && !passes.leave_out_enhanced)
            .then(|| puddles::Pipelines::new(&device, &scene.shader));
        let rt = options.ray_tracing.then(|| rt::RayTracer::new(&device));
        let cookie_atlas = cookies::CookieAtlas::new();
        Renderer {
            targets: Targets {
                scale_targets: HashMap::new(),
                triple_targets: None,
                glass_picture: None,
                glass_mips: None,
                target_use: HashMap::new(),
                evicted_at: std::time::Instant::now(),
                hdr_targets: HashMap::new(),
                ao: None,
                ground: None,
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
                upscale_buf: upscale.buf,
                dynamic_scale: std::cell::Cell::new(1.0),
            },
            sky: SkyRes {
                layout: sky.layout.clone(),
                sky_lut: enhanced.sky_lut,
                cloud_field: FieldState::default(),
                sky_lut_view: enhanced.sky_lut_view,
                sky_state: None,
                city_glow: None,
                sky_job: None,
                sky_mirror_pipeline: passes.sky_mirror_pipeline,
                sky_sampler,
                vanilla_sky_buf: sky.vanilla_buf,
                cloud_shape_view: sky.cloud_shape_view,
                cloud_detail_view: sky.cloud_detail_view,
                cloud_sampler: sky.cloud_sampler,
                cloud_shape_cpu: sky.cloud_shape_cpu,
                cloud_view_buf: sky.cloud_view_buf,
                cloud_sun: None,
                sky_mesh,
            },
            lights: LightRes {
                lm_atlas_view: scene
                    .lm_atlas
                    .create_view(&mx::TextureViewDescriptor::default())
                    .expect("mX: a texture view"),
                lm_atlas: scene.lm_atlas,
                lm_uniform: scene.lm_uniform,
                lm_place: std::cell::Cell::new((0.0, 0.0, 0.0)),
                snow_track: std::cell::RefCell::new(scene.snow_track),
                snow_track_made: std::cell::Cell::new(0),
                snow_track_uniform: scene.snow_track_uniform,
                fog_lamps_pipeline: fog_lamps.pipeline,
                fog_lamps_buf: fog_lamps.buf,
                fog_lamps_layout: fog_lamps.layout,
                vol_fog: (!leave_out_enhanced)
                    .then(|| crate::vol_fog::VolFog::new(&device, &queue, hdr_format, gl))
                    .flatten(),
                corona_bind_group: coronas.bind_group,
                corona_layout: coronas.layout,
                snow_layout: snow.layout,
                rain_layout: rain.layout,
                snow_buf: snow.buf,
                snow_bind_group: snow.bind_group,
                rain_buf: rain.buf,
                rain_bind_group: rain.bind_group,
                smoke_bind_group: coronas.smoke_bind_group,
                corona_textures: Vec::new(),
                pictures_taken: (0, 0),
                cookies: cookie_atlas,
                corona_sampler: coronas.sampler,
            },
            tone: ToneMapping {
                post: post.pipelines,
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
                shadow_far_cache: std::cell::Cell::new((
                    Mat4::IDENTITY,
                    0,
                    DVec3::ZERO,
                    Vec3::ZERO,
                )),
                shadow_near_cache: std::cell::Cell::new((
                    Mat4::IDENTITY,
                    0,
                    DVec3::ZERO,
                    Vec3::ZERO,
                )),
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
                shadow_pipelines: shadows.pipelines,
            },
            history: ViewHistory {
                triple_culling: Default::default(),
                xr_culling: Default::default(),
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
                report: errors.report.clone(),
                log: errors.log.clone(),
            },
            _device_poller: DevicePoller::start(&device, errors.report.clone()),
            mip_pipeline: mip.pipeline,
            mip_text_pipeline: mip.text_pipeline,
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
                .checked_sub(std::time::Duration::from_secs_f64(
                    omsi_cfg::flags::OMSI_RENDER_CLOCK
                        .var()
                        .and_then(|v| v.trim().parse::<f64>().ok())
                        .filter(|s| s.is_finite() && *s >= 0.0)
                        .unwrap_or(0.0),
                ))
                .unwrap_or_else(std::time::Instant::now),
            tree_gust_drift: std::cell::Cell::new((glam::DVec2::ZERO, 0.0)),
            ao_sampler: ssao.sampler,
            ao_buf: ssao.buf,
            prepass_pipelines: prepass.pipelines,
            temporal_pipes,
            temporal: None,
            temporal_failed: false,
            temporal_window: false,
            temporal_kept: None,
            frame_generation: None,
            temporal_pending: None,
            prepass_msaa_pipelines: prepass.msaa_pipelines,
            prepass_eye_pipelines: prepass.eye_pipelines,
            pane_mask: prepass.pane_mask,
            pane_depth: None,
            ground_pipelines,
            ssao_pipeline: ssao.ssao_pipeline,
            blur_pipeline: ssao.blur_pipeline,
            plain_ssao_pipeline: ssao.plain_ssao_pipeline,
            plain_blur_pipeline: ssao.plain_blur_pipeline,
            device,
            queue,
            adapter_name,
            pass: passes.pass,
            hdr_pass: passes.hdr_pass,
            hdr_eye_pass: passes.hdr_eye_pass,
            eye_pipeline_kit,
            reflection_pass: passes.reflection_pass,
            overlay_pipeline: overlays.pipeline,
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
            deferred: Default::default(),
            frame_draws: (0, 0),
            profiling: omsi_cfg::flags::OMSI_PROFILE.is_set(),
            draw_audit_at: std::time::Instant::now(),
            encoding_pool: if omsi_cfg::flags::OMSI_NO_RENDER_POOL.is_set() {
                None
            } else {
                let workers = std::thread::available_parallelism()
                    .map(|n| n.get() / 2)
                    .unwrap_or(2)
                    .clamp(2, 8);
                rayon::ThreadPoolBuilder::new()
                    .num_threads(workers)
                    .thread_name(|i| format!("omsi-render-{i}"))
                    .build()
                    .ok()
            },
            pending_meshes: Default::default(),
            freed: std::cell::OnceCell::new(),
            rt,
            mesh_pages: false,
        }
    }
}
