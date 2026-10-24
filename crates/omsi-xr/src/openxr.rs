//! Optional PCVR session. This module is only compiled on Windows; the normal
//! desktop and Android render paths do not depend on an OpenXR runtime.

use anyhow::{anyhow, bail, Context, Result};
use glam::{DVec3, Mat4, Quat, Vec3, Vec4};
use openxr as xr;
use std::time::{Duration, Instant};
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D12::{
    ID3D12CommandAllocator, ID3D12CommandList, ID3D12GraphicsCommandList, ID3D12PipelineState,
    ID3D12Resource, D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_RESOURCE_BARRIER,
    D3D12_RESOURCE_BARRIER_0, D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
    D3D12_RESOURCE_BARRIER_FLAG_NONE, D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
    D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_RENDER_TARGET,
    D3D12_RESOURCE_TRANSITION_BARRIER,
};
use windows::Win32::Graphics::Dxgi::Common::*;

use omsi_render::{Camera, Lighting, Renderer, Scene};

pub struct Vr {
    instance: xr::Instance,
    session: xr::Session<xr::D3D12>,
    frame_waiter: xr::FrameWaiter,
    frame_stream: xr::FrameStream<xr::D3D12>,
    space: xr::Space,
    swapchain: xr::Swapchain<xr::D3D12>,
    images: Vec<wgpu::Texture>,
    resources: Vec<ID3D12Resource>,
    prepared: Vec<bool>,
    width: u32,
    height: u32,
    blend: xr::EnvironmentBlendMode,
    mirror: Option<Mirror>,
    desktop_mirror: bool,
    origin: Option<Vec3>,
    player_uid: Option<u64>,
    origin_rotation: Option<Quat>,
    smoothed_head: Option<(Vec3, Quat, Instant)>,
    mirror_camera: Option<(Camera, Mat4)>,
    menu_anchor: Option<UiAnchor>,
    cursor_anchor: Option<UiAnchor>,
    cursor_surface_local: Option<Vec3>,
    cockpit_cursor_anchor: Option<UiAnchor>,
    cockpit_cursor_surface_local: Option<Vec3>,
    last_cursor_position: Option<(f32, f32)>,
    last_cursor_move: Option<Instant>,
    cursor_menu_open: bool,
    zoom_from: f32,
    zoom_to: f32,
    zoom_changed: Instant,
    running: bool,
    focused: bool,
    events: xr::EventDataBuffer,
    stats_since: Instant,
    stats_frames: u32,
    stats_wait: Duration,
    stats_image_wait: Duration,
    stats_render: Duration,
    stats_gpu_wait: Duration,
    stats_eyes: [Duration; 2],
    stats_desktop_mirror: Duration,
    stats_stages_prev: std::collections::BTreeMap<&'static str, f64>,
}

struct Mirror {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    params: wgpu::Buffer,
}

/// A flat UI surface fixed in the world after it is placed.
#[derive(Clone, Copy)]
struct UiAnchor {
    origin: DVec3,
    right: Vec3,
    up: Vec3,
    half_width: f32,
    half_height: f32,
}

impl UiAnchor {
    fn at(origin: DVec3, camera: &Camera, fov: xr::Fovf, distance: f32) -> Self {
        Self {
            origin,
            right: camera.right(),
            up: camera.up(),
            half_width: distance * (fov.angle_right.tan() - fov.angle_left.tan()) * 0.5,
            half_height: distance * (fov.angle_up.tan() - fov.angle_down.tan()) * 0.5,
        }
    }

    fn menu(
        origin: DVec3,
        camera: &Camera,
        fov: xr::Fovf,
        distance: f32,
        desktop_size: (u32, u32),
    ) -> Self {
        let mut anchor = Self::at(origin, camera, fov, distance);
        anchor.half_height *= 0.60;
        anchor.half_width =
            anchor.half_height * desktop_size.0.max(1) as f32 / desktop_size.1.max(1) as f32;
        anchor
    }

    fn point(&self, x: f32, y: f32) -> DVec3 {
        self.origin
            + (self.right * (x * self.half_width) + self.up * (y * self.half_height)).as_dvec3()
    }

    fn transform(&self, eye: &Camera, projection: Mat4) -> Mat4 {
        let position = (self.origin - eye.position).as_vec3();
        let basis = Mat4::from_cols(
            (self.right * self.half_width).extend(0.0),
            (self.up * self.half_height).extend(0.0),
            Vec4::Z,
            position.extend(1.0),
        );
        projection * Mat4::look_to_rh(Vec3::ZERO, eye.forward(), eye.up()) * basis
    }

    /// Project the 3D hit point, then face the dot towards each eye. A quad fixed
    /// to the bus surface becomes an ellipse when the head looks across it.
    fn cursor_transform(&self, eye: &Camera, projection: Mat4) -> Option<Mat4> {
        let clip = self.cursor_clip(eye, projection);
        (clip.w > 0.0).then(|| {
            Mat4::from_cols(
                Vec4::new(clip.w, 0.0, 0.0, 0.0),
                Vec4::new(0.0, clip.w, 0.0, 0.0),
                Vec4::Z,
                clip,
            )
        })
    }

    fn cursor_clip(&self, eye: &Camera, projection: Mat4) -> Vec4 {
        let position = (self.origin - eye.position).as_vec3();
        projection * Mat4::look_to_rh(Vec3::ZERO, eye.forward(), eye.up()) * position.extend(1.0)
    }

    fn cursor_on_screen(&self, eye: &Camera, projection: Mat4) -> bool {
        let clip = self.cursor_clip(eye, projection);
        clip.w > 0.0 && (clip.x / clip.w).abs() < 0.85 && (clip.y / clip.w).abs() < 0.85
    }
}

impl Vr {
    pub fn new(
        renderer: &Renderer,
        configured_scale: f32,
        desktop_mirror: bool,
    ) -> Result<Self> {
        let entry = xr::Entry::linked(&()).context("load OpenXR")?;
        if !entry.enumerate_extensions()?.khr_d3d12_enable {
            bail!("the active OpenXR runtime does not support D3D12");
        }
        let mut extensions = xr::ExtensionSet::default();
        extensions.khr_d3d12_enable = true;
        let instance = entry.create_instance(
            &xr::ApplicationInfo {
                application_name: "openOMSI",
                application_version: 1,
                engine_name: "openOMSI",
                engine_version: 1,
                api_version: xr::Version::new(1, 0, 0),
            },
            &extensions,
            &[],
            &(),
        )?;
        let system = instance.system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)?;
        let properties = instance.system_properties(system)?;
        let requirements = instance.graphics_requirements::<xr::D3D12>(system)?;

        // OpenXR must receive the exact device and command queue used for the
        // textures submitted by wgpu. A second D3D12 device cannot present them.
        let device = unsafe { renderer.device.as_hal::<wgpu_hal::api::Dx12>() }
            .ok_or_else(|| anyhow!("OpenXR PCVR requires the DX12 graphics backend"))?;
        let queue = unsafe { renderer.queue.as_hal::<wgpu_hal::api::Dx12>() }
            .ok_or_else(|| anyhow!("wgpu did not expose its DX12 queue"))?;
        let luid = unsafe { device.raw_device().GetAdapterLuid() };
        if luid.LowPart != requirements.adapter_luid.LowPart
            || luid.HighPart != requirements.adapter_luid.HighPart
        {
            bail!("the OpenXR headset and openOMSI are using different graphics adapters");
        }
        let binding = xr::d3d::SessionCreateInfoD3D12 {
            device: device.raw_device().as_raw().cast(),
            queue: queue.as_raw().as_raw().cast(),
        };
        let (session, frame_waiter, frame_stream) =
            unsafe { instance.create_session::<xr::D3D12>(system, &binding) }
                .context("create OpenXR D3D12 session")?;
        let space = session
            .create_reference_space(xr::ReferenceSpaceType::LOCAL, xr::Posef::IDENTITY)
            .context("create OpenXR local space")?;
        let view_type = xr::ViewConfigurationType::PRIMARY_STEREO;
        let views = instance.enumerate_view_configuration_views(system, view_type)?;
        if views.len() != 2 {
            bail!("OpenXR runtime did not return two stereo views");
        }
        let recommended_width = views[0].recommended_image_rect_width;
        let recommended_height = views[0].recommended_image_rect_height;
        if views[1].recommended_image_rect_width != recommended_width
            || views[1].recommended_image_rect_height != recommended_height
        {
            bail!("OpenXR eye resolutions differ; this renderer needs matching eye sizes");
        }
        // The runtime's recommendation is a quality target, not a minimum.
        // Rendering both eyes at the full Quest Link size is expensive.
        // (`OMSI_OPENXR_SCALE` is in `configured_scale` already: `Renderer::start_xr`)
        let scale = configured_scale.clamp(0.5, 1.0);
        let width = ((recommended_width as f32 * scale).round() as u32).max(1);
        let height = ((recommended_height as f32 * scale).round() as u32).max(1);
        let (format, dxgi_format) = match renderer.format() {
            wgpu::TextureFormat::Bgra8UnormSrgb => (
                wgpu::TextureFormat::Bgra8UnormSrgb,
                DXGI_FORMAT_B8G8R8A8_UNORM_SRGB.0 as u32,
            ),
            wgpu::TextureFormat::Bgra8Unorm => (
                wgpu::TextureFormat::Bgra8Unorm,
                DXGI_FORMAT_B8G8R8A8_UNORM.0 as u32,
            ),
            wgpu::TextureFormat::Rgba8UnormSrgb => (
                wgpu::TextureFormat::Rgba8UnormSrgb,
                DXGI_FORMAT_R8G8B8A8_UNORM_SRGB.0 as u32,
            ),
            wgpu::TextureFormat::Rgba8Unorm => (
                wgpu::TextureFormat::Rgba8Unorm,
                DXGI_FORMAT_R8G8B8A8_UNORM.0 as u32,
            ),
            other => bail!("OpenXR cannot use renderer format {other:?}"),
        };
        if !session
            .enumerate_swapchain_formats()?
            .contains(&dxgi_format)
        {
            bail!("OpenXR runtime does not support the window's {format:?} color format");
        }
        let make_swapchain = |usage_flags| {
            session.create_swapchain(&xr::SwapchainCreateInfo {
                create_flags: xr::SwapchainCreateFlags::EMPTY,
                usage_flags,
                format: dxgi_format,
                sample_count: 1,
                width,
                height,
                face_count: 1,
                array_size: 2,
                mip_count: 1,
            })
        };
        let (swapchain, sampled) = match make_swapchain(
            xr::SwapchainUsageFlags::COLOR_ATTACHMENT | xr::SwapchainUsageFlags::SAMPLED,
        ) {
            Ok(swapchain) => (swapchain, true),
            Err(error) => {
                log::warn!(
                    "OpenXR desktop mirror unavailable ({error}); using normal desktop view"
                );
                (
                    make_swapchain(xr::SwapchainUsageFlags::COLOR_ATTACHMENT)?,
                    false,
                )
            }
        };
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 2,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("OpenXR stereo swapchain"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | if sampled {
                    wgpu::TextureUsages::TEXTURE_BINDING
                } else {
                    wgpu::TextureUsages::empty()
                },
            view_formats: &[],
        };
        let imported = swapchain
            .enumerate_images()?
            .into_iter()
            .map(|image| {
                // OpenXR retains ownership; cloning the COM interface gives wgpu a
                // separate reference for the lifetime of the imported texture.
                let raw = image.cast();
                let resource = unsafe {
                    windows::Win32::Graphics::Direct3D12::ID3D12Resource::from_raw_borrowed(&raw)
                }
                .ok_or_else(|| anyhow!("OpenXR returned a null swapchain image"))?
                .clone();
                let hal_texture = unsafe {
                    wgpu_hal::dx12::Device::texture_from_raw(
                        resource.clone(),
                        format,
                        wgpu::TextureDimension::D2,
                        size,
                        1,
                        1,
                    )
                };
                let texture = unsafe {
                    renderer
                        .device
                        .create_texture_from_hal::<wgpu_hal::api::Dx12>(hal_texture, &desc)
                };
                Ok((texture, resource))
            })
            .collect::<Result<Vec<_>>>()?;
        let (images, resources): (Vec<_>, Vec<_>) = imported.into_iter().unzip();
        let prepared = vec![false; images.len()];
        let blend = instance
            .enumerate_environment_blend_modes(system, view_type)?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no OpenXR blend mode"))?;
        let mirror = sampled.then(|| Mirror::new(renderer));
        log::info!(
            "OpenXR session opened for {} at {}x{} per eye ({}x{} recommended, scale {:.2})",
            properties.system_name,
            width,
            height,
            recommended_width,
            recommended_height,
            scale
        );
        if let Some(rate) = omsi_cfg::flags::OMSI_OPENXR_MIRROR_RATE.var() {
            log::info!("OpenXR bus mirror rate override: {rate} pictures/s (-1: every mirror each game frame)");
        }
        Ok(Self {
            instance,
            session,
            frame_waiter,
            frame_stream,
            space,
            swapchain,
            images,
            resources,
            prepared,
            width,
            height,
            blend,
            mirror,
            desktop_mirror,
            origin: None,
            player_uid: None,
            origin_rotation: None,
            smoothed_head: None,
            mirror_camera: None,
            menu_anchor: None,
            cursor_anchor: None,
            cursor_surface_local: None,
            cockpit_cursor_anchor: None,
            cockpit_cursor_surface_local: None,
            last_cursor_position: None,
            last_cursor_move: None,
            cursor_menu_open: false,
            zoom_from: 0.0,
            zoom_to: 0.0,
            zoom_changed: Instant::now(),
            running: false,
            focused: false,
            events: xr::EventDataBuffer::new(),
            stats_since: Instant::now(),
            stats_frames: 0,
            stats_wait: Duration::ZERO,
            stats_image_wait: Duration::ZERO,
            stats_render: Duration::ZERO,
            stats_gpu_wait: Duration::ZERO,
            stats_eyes: [Duration::ZERO; 2],
            stats_desktop_mirror: Duration::ZERO,
            stats_stages_prev: Default::default(),
        })
    }

    pub fn poll(&mut self) -> Result<()> {
        let mut refocus = false;
        while let Some(event) = self.instance.poll_event(&mut self.events)? {
            if let xr::Event::SessionStateChanged(change) = event {
                let focused = change.state() == xr::SessionState::FOCUSED;
                refocus |= focused && !self.focused;
                self.focused = focused;
                match change.state() {
                    xr::SessionState::READY => {
                        self.session
                            .begin(xr::ViewConfigurationType::PRIMARY_STEREO)?;
                        self.running = true;
                    }
                    xr::SessionState::STOPPING => {
                        self.running = false;
                        self.session.end()?;
                    }
                    xr::SessionState::EXITING | xr::SessionState::LOSS_PENDING => {
                        self.running = false;
                    }
                    _ => {}
                }
            }
        }
        // Valid poses can already exist while the headset is resting on a desk.
        // Its position when the runtime gives us focus is the seated reference.
        if refocus {
            self.recenter();
        }
        Ok(())
    }

    pub fn recenter(&mut self) {
        self.origin = None;
        self.origin_rotation = None;
        self.smoothed_head = None;
        self.mirror_camera = None;
        self.menu_anchor = None;
        self.recenter_pointer();
    }

    pub fn recenter_pointer(&mut self) {
        self.cursor_anchor = None;
        self.cursor_surface_local = None;
        self.cockpit_cursor_anchor = None;
        self.cockpit_cursor_surface_local = None;
        self.last_cursor_position = None;
        self.last_cursor_move = Some(Instant::now());
    }

    pub fn toggle_desktop_mirror(&mut self) -> bool {
        self.desktop_mirror = !self.desktop_mirror;
        self.desktop_mirror
    }

    pub fn desktop_crop(&self, size: (u32, u32)) -> (f32, f32) {
        let eye_aspect = self.width as f32 / self.height.max(1) as f32;
        let desktop_aspect = size.0.max(1) as f32 / size.1.max(1) as f32;
        if desktop_aspect > eye_aspect {
            (1.0, eye_aspect / desktop_aspect)
        } else {
            (desktop_aspect / eye_aspect, 1.0)
        }
    }

    pub fn needs_cursor_surface(&self, position: (f32, f32), menu_open: bool) -> bool {
        !menu_open
            && (self.cursor_menu_open
                || self.last_cursor_position.is_none_or(|p| {
                    (p.0 - position.0).abs() > 0.5 || (p.1 - position.1).abs() > 0.5
                }))
    }

    pub fn set_cursor_surface(
        &mut self,
        hit: Option<DVec3>,
        bus_pose: Option<(DVec3, Mat4)>,
    ) {
        self.cursor_surface_local = hit.zip(bus_pose).map(|(point, (position, rotation))| {
            rotation
                .inverse()
                .transform_point3((point - position).as_vec3())
        });
    }

    fn animated_zoom(&mut self, active: bool) -> f32 {
        let now = Instant::now();
        let progress = (now.duration_since(self.zoom_changed).as_secs_f32() / 0.24).clamp(0.0, 1.0);
        let eased = progress * progress * (3.0 - 2.0 * progress);
        let current = self.zoom_from + (self.zoom_to - self.zoom_from) * eased;
        let wanted = if active { 1.0 } else { 0.0 };
        if wanted != self.zoom_to {
            self.zoom_from = current;
            self.zoom_to = wanted;
            self.zoom_changed = now;
        }
        1.0 + current * 0.6
    }

    fn cursor_target(
        &self,
        position: (f32, f32),
        size: (u32, u32),
        camera: &Camera,
        projection: Mat4,
    ) -> DVec3 {
        if !self.cursor_menu_open {
            if let (Some(anchor), Some(previous)) = (self.cursor_anchor, self.last_cursor_position)
            {
                let dx = (position.0 - previous.0) * 2.0 / size.0.max(1) as f32;
                let dy = (previous.1 - position.1) * 2.0 / size.1.max(1) as f32;
                return anchor.origin
                    + (camera.right() * (dx * anchor.half_width)
                        + camera.up() * (dy * anchor.half_height))
                        .as_dvec3();
            }
        }
        let direction = eye_ray(
            camera,
            projection,
            position.0,
            position.1,
            size,
            self.desktop_crop(size),
        );
        camera.position + (direction * 1.5).as_dvec3()
    }

    /// Pick through the same cropped left-eye picture shown on the desktop.
    pub fn cursor_ray(
        &self,
        x: f32,
        y: f32,
        size: (u32, u32),
    ) -> Option<(DVec3, Vec3, f32)> {
        let (camera, projection) = self.mirror_camera?;
        let h = size.1.max(1) as f32;
        let crop = self.desktop_crop(size);
        let direction = if self.cursor_anchor.is_some() && !self.cursor_menu_open {
            (self.cursor_target((x, y), size, &camera, projection) - camera.position)
                .as_vec3()
                .normalize_or_zero()
        } else {
            eye_ray(&camera, projection, x, y, size, crop)
        };
        let spread = 2.0 * (camera.fov_deg.to_radians() * 0.5).tan() * crop.1 / h;
        Some((camera.position, direction, spread))
    }

    pub fn navigator_edit_camera(&self) -> Option<Camera> {
        self.mirror_camera.map(|(camera, _)| camera)
    }

    pub fn render(
        &mut self,
        renderer: &mut Renderer,
        scene: &mut Scene,
        base: &Camera,
        lighting: &Lighting,
        desktop: &wgpu::TextureView,
        desktop_size: (u32, u32),
        menu_range: std::ops::Range<usize>,
        cursor_overlay: Option<usize>,
        tooltip_overlay: Option<usize>,
        cursor_position: (f32, f32),
        bus_pose: Option<(DVec3, Mat4)>,
        navigator: Option<(usize, crate::navigator::Display)>,
        player_uid: Option<u64>,
        head_smoothing_ms: f32,
        cockpit_pointer_enabled: bool,
        zoom_active: bool,
    ) -> Result<bool> {
        if self.player_uid != player_uid {
            self.recenter();
            self.player_uid = player_uid;
        }
        self.poll()?;
        if !self.running {
            return Ok(false);
        }
        if self.stats_frames == 0 {
            self.stats_since = Instant::now();
        }
        let wait_start = Instant::now();
        let frame = self.frame_waiter.wait()?;
        let wait_time = wait_start.elapsed();
        self.frame_stream.begin()?;
        if !frame.should_render {
            self.frame_stream
                .end(frame.predicted_display_time, self.blend, &[])?;
            return Ok(false);
        }
        let (view_state, views) = self.session.locate_views(
            xr::ViewConfigurationType::PRIMARY_STEREO,
            frame.predicted_display_time,
            &self.space,
        )?;
        if views.len() != 2 {
            self.frame_stream
                .end(frame.predicted_display_time, self.blend, &[])?;
            bail!("OpenXR stopped providing stereo views");
        }
        // Valid poses may be inferred even when active tracking is unavailable.
        // Input focus must not gate rendering: VISIBLE sessions should continue
        // submitting layers, including underneath runtime overlays. Refocusing
        // still recenters the seated origin in poll().
        let valid = xr::ViewStateFlags::POSITION_VALID | xr::ViewStateFlags::ORIENTATION_VALID;
        if !view_state.contains(valid) {
            self.frame_stream.end(frame.predicted_display_time, self.blend, &[])?;
            return Ok(false);
        }
        let midpoint = (xr_position(views[0].pose) + xr_position(views[1].pose)) * 0.5;
        let raw_rotation = xr_rotation(views[0].pose);
        let (head_position, head_rotation) = if head_smoothing_ms > 0.0 {
            let now = Instant::now();
            let (position, rotation) =
                if let Some((previous_position, previous_rotation, previous_time)) =
                    self.smoothed_head
                {
                    let dt = now.duration_since(previous_time).as_secs_f32();
                    let tau = head_smoothing_ms.clamp(0.0, 30.0) * 0.001;
                    let alpha = 1.0 - (-dt / tau).exp();
                    (
                        previous_position.lerp(midpoint, alpha),
                        previous_rotation.slerp(raw_rotation, alpha),
                    )
                } else {
                    (midpoint, raw_rotation)
                };
            self.smoothed_head = Some((position, rotation, now));
            (position, rotation)
        } else {
            self.smoothed_head = None;
            (midpoint, raw_rotation)
        };
        // Move both eyes as one head, preserving the runtime's eye offsets and
        // their small orientation differences. Render and submit these same poses.
        let eye_poses = [0, 1].map(|eye| {
            if head_smoothing_ms <= 0.0 {
                return views[eye].pose;
            }
            let eye_position = head_position
                + head_rotation
                    * (raw_rotation.inverse() * (xr_position(views[eye].pose) - midpoint));
            let eye_rotation =
                head_rotation * (raw_rotation.inverse() * xr_rotation(views[eye].pose));
            xr::Posef {
                orientation: xr::Quaternionf {
                    x: eye_rotation.x,
                    y: eye_rotation.y,
                    z: eye_rotation.z,
                    w: eye_rotation.w,
                },
                position: xr::Vector3f {
                    x: eye_position.x,
                    y: eye_position.y,
                    z: eye_position.z,
                },
            }
        });
        let origin = *self.origin.get_or_insert(midpoint);
        let origin_rotation = *self
            .origin_rotation
            .get_or_insert_with(|| initial_yaw_rotation(xr_rotation(views[0].pose)));
        let image_wait_start = Instant::now();
        let index = self.swapchain.acquire_image()? as usize;
        self.swapchain.wait_image(xr::Duration::INFINITE)?;
        self.stats_image_wait += image_wait_start.elapsed();
        let render_start = Instant::now();
        // OpenXR promises RENDER_TARGET on wait, while wgpu 29 initializes an
        // imported texture's tracker to COMMON. On each image's first use,
        // transition the actual resource to COMMON before wgpu's first barrier.
        // Later acquisitions need no fix: wgpu leaves both layers in
        // RENDER_TARGET, which is also the state OpenXR requires on release.
        let first_use = if !self.prepared[index] {
            Some(transition_first_use(renderer, &self.resources[index])?)
        } else {
            None
        };
        let mut vr_base = *base;
        vr_base.roll = 0.0;
        // Driver cameras may look down at the dashboard by default. In VR the
        // headset supplies the player's pitch, so start from a level view.
        vr_base.pitch = 0.0;
        let zoom = self.animated_zoom(zoom_active);
        let targets = [0, 1].map(|eye| {
            self.images[index].create_view(&wgpu::TextureViewDescriptor {
                label: Some("OpenXR eye"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: eye as u32,
                array_layer_count: Some(1),
                ..Default::default()
            })
        });
        renderer.set_env_heading(Some(vr_base.yaw));
        let mut eye_cameras = [vr_base; 2];
        let mut ui_cameras = [vr_base; 2];
        let mut eye_projections = [Mat4::IDENTITY; 2];
        let mut ui_projections = [Mat4::IDENTITY; 2];
        let stereo_center = (xr_position(eye_poses[0]) + xr_position(eye_poses[1])) * 0.5;
        for (eye, xr_view) in views.iter().enumerate() {
            let eye_start = Instant::now();
            let tracked_eye = xr_position(eye_poses[eye]);
            let tracked_relative = origin_rotation.inverse() * (tracked_eye - origin);
            // A narrower picture magnifies stereo disparity. Reduce the virtual
            // eye separation by the same factor so convergence stays unchanged.
            let zoomed_eye = zoom_eye_position(tracked_eye, stereo_center, zoom);
            let relative_position = origin_rotation.inverse() * (zoomed_eye - origin);
            let relative_rotation = origin_rotation.inverse() * xr_rotation(eye_poses[eye]);
            let mut camera =
                eye_camera(&vr_base, relative_position, relative_rotation, xr_view.fov);
            let ui_camera = eye_camera(&vr_base, tracked_relative, relative_rotation, xr_view.fov);
            let ui_projection = eye_projection(xr_view.fov, camera.near, camera.far);
            // Magnify focal lengths but preserve each eye's asymmetric projection
            // offset. Scaling that offset too displaced the two pictures apart.
            let projection = zoom_projection(ui_projection, zoom);
            camera.fov_deg = (2.0
                * (((xr_view.fov.angle_up - xr_view.fov.angle_down) * 0.5).tan() / zoom).atan())
            .to_degrees();
            eye_cameras[eye] = camera;
            ui_cameras[eye] = ui_camera;
            eye_projections[eye] = projection;
            ui_projections[eye] = ui_projection;
            if eye == 0 {
                self.mirror_camera = Some((camera, projection));
            }
            renderer.render_xr_eye(
                scene,
                &targets[eye],
                self.width,
                self.height,
                &camera,
                lighting,
                projection,
                eye == 1,
            );
            self.stats_eyes[eye] += eye_start.elapsed();
        }
        renderer.set_env_heading(None);
        let menu_open = !menu_range.is_empty();
        self.place_cursor(menu_open, cockpit_pointer_enabled, cursor_position, desktop_size, bus_pose, &eye_cameras, &eye_projections, &ui_cameras, views[0].fov);
        let menu_transforms = [0, 1].map(|eye| {
            self.menu_anchor
                .map(|anchor| anchor.transform(&ui_cameras[eye], ui_projections[eye]))
                .unwrap_or(Mat4::IDENTITY)
        });
        let cursor_visible = (menu_open || cockpit_pointer_enabled)
            && self
                .last_cursor_move
                .is_some_and(|t| t.elapsed() < Duration::from_secs(10));
        let cursor_transforms = [0, 1].map(|eye| {
            self.cursor_anchor
                .filter(|_| cursor_visible)
                .and_then(|anchor| {
                    if menu_open {
                        anchor.cursor_transform(&ui_cameras[eye], ui_projections[eye])
                    } else {
                        anchor.cursor_transform(&eye_cameras[eye], eye_projections[eye])
                    }
                })
        });
        let navigator = navigator.zip(bus_pose).and_then(|((index, display), (position, body))| {
            let (texture, rect) = scene.overlays.get(index)?;
            let aspect = (rect[2] - rect[0]) / (rect[3] - rect[1]);
            if !aspect.is_finite() || aspect <= 0.0 { return None; }
            Some((*texture, [0, 1].map(|eye| display.transform(position, body, &eye_cameras[eye], eye_projections[eye], aspect))))
        });
        renderer.render_xr_ui(
            scene,
            &targets,
            desktop_size,
            (self.width, self.height),
            menu_range,
            menu_transforms,
            cursor_overlay,
            tooltip_overlay,
            cursor_transforms,
            navigator,
        );
        self.draw_desktop(renderer, index, desktop, desktop_size);
        // The runtime may sample this image immediately after release. Wait for
        // wgpu's submissions until explicit D3D12 fence handoff is implemented.
        let gpu_wait_start = Instant::now();
        renderer.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })?;
        let gpu_wait_time = gpu_wait_start.elapsed();
        self.prepared[index] = true;
        drop(first_use);
        self.swapchain.release_image()?;
        let rect = xr::Rect2Di {
            offset: xr::Offset2Di { x: 0, y: 0 },
            extent: xr::Extent2Di {
                width: self.width as i32,
                height: self.height as i32,
            },
        };
        let projection_views = [0, 1].map(|eye| {
            xr::CompositionLayerProjectionView::new()
                .pose(eye_poses[eye])
                .fov(views[eye].fov)
                .sub_image(
                    xr::SwapchainSubImage::new()
                        .swapchain(&self.swapchain)
                        .image_array_index(eye as u32)
                        .image_rect(rect),
                )
        });
        let layer = xr::CompositionLayerProjection::new()
            .space(&self.space)
            .views(&projection_views);
        self.frame_stream
            .end(frame.predicted_display_time, self.blend, &[&layer])?;
        self.record_frame_stats(renderer, wait_time, render_start, gpu_wait_time);
        Ok(true)
    }

    /// Where the menu and the cursor hang in front of the eyes this frame: the menu before the
    /// head while it is open, the cursor on the menu, on what it points at in the bus (kept in
    /// the bus's frame as it drives on) or before the eyes, hidden after ten idle seconds.
    #[allow(clippy::too_many_arguments)]
    fn place_cursor(&mut self, menu_open: bool, cockpit_pointer_enabled: bool, cursor_position: (f32, f32), desktop_size: (u32, u32), bus_pose: Option<(DVec3, Mat4)>, eye_cameras: &[Camera; 2], eye_projections: &[Mat4; 2], ui_cameras: &[Camera; 2], fov: xr::Fovf) {
        if !menu_open
            && cockpit_pointer_enabled
            && self
                .last_cursor_move
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(10))
        {
            self.cursor_anchor = None;
            self.cursor_surface_local = None;
            self.last_cursor_move = None;
        }
        if menu_open && !self.cursor_menu_open {
            self.cockpit_cursor_anchor = self.cursor_anchor;
            self.cockpit_cursor_surface_local = self.cursor_surface_local;
        } else if !menu_open && self.cursor_menu_open {
            self.cursor_anchor = self.cockpit_cursor_anchor.take();
            self.cursor_surface_local = self.cockpit_cursor_surface_local.take();
            self.last_cursor_position = self.cursor_anchor.map(|_| cursor_position);
            self.last_cursor_move = Some(Instant::now());
            self.cursor_menu_open = false;
        }
        if menu_open {
            let center = (ui_cameras[0].position + ui_cameras[1].position) * 0.5
                + (ui_cameras[0].forward() * 2.0).as_dvec3();
            self.menu_anchor = Some(UiAnchor::menu(
                center,
                &ui_cameras[0],
                fov,
                2.0,
                desktop_size,
            ));
        } else {
            self.menu_anchor = None;
        }
        let moved = self.last_cursor_position.is_none_or(|p| {
            (p.0 - cursor_position.0).abs() > 0.5 || (p.1 - cursor_position.1).abs() > 0.5
        }) || self.cursor_menu_open != menu_open;
        let place_in_front = !menu_open
            && (self.cursor_anchor.is_none()
                || (moved
                    && self.cursor_anchor.is_some_and(|anchor| {
                        !anchor.cursor_on_screen(&eye_cameras[0], eye_projections[0])
                    })));
        let cursor_target = if place_in_front {
            eye_cameras[0].position + (eye_cameras[0].forward() * 1.5).as_dvec3()
        } else {
            self.cursor_target(
                cursor_position,
                desktop_size,
                &eye_cameras[0],
                eye_projections[0],
            )
        };
        if moved {
            self.last_cursor_move = Some(Instant::now());
            self.last_cursor_position = Some(cursor_position);
        }
        if let Some(menu) = self.menu_anchor {
            let (w, h) = (desktop_size.0.max(1) as f32, desktop_size.1.max(1) as f32);
            let x = cursor_position.0 / w * 2.0 - 1.0;
            let y = 1.0 - cursor_position.1 / h * 2.0;
            self.cursor_anchor = Some(UiAnchor {
                origin: menu.point(x, y),
                ..menu
            });
        } else if moved {
            if place_in_front {
                self.cursor_surface_local = None;
            }
            let hit =
                self.cursor_surface_local
                    .zip(bus_pose)
                    .map(|(local, (position, rotation))| {
                        position + rotation.transform_point3(local).as_dvec3()
                    });
            let point = hit.unwrap_or(cursor_target);
            // Keep free-space cursor positions in the bus frame too. Otherwise a
            // cursor without a surface hit stays in the world as the bus drives on.
            self.cursor_surface_local = bus_pose.map(|(position, rotation)| {
                rotation
                    .inverse()
                    .transform_point3((point - position).as_vec3())
            });
            let distance = (point - eye_cameras[0].position).length() as f32;
            self.cursor_anchor = Some(UiAnchor::at(
                point,
                &eye_cameras[0],
                fov,
                distance.clamp(0.3, 8.0),
            ));
        } else if let (Some(local), Some((position, rotation)), Some(anchor)) = (
            self.cursor_surface_local,
            bus_pose,
            self.cursor_anchor.as_mut(),
        ) {
            anchor.origin = position + rotation.transform_point3(local).as_dvec3();
        }
        self.cursor_menu_open = menu_open;
    }

    /// The desktop window while the headset shows the game: the left eye's picture when the
    /// desktop mirror is on, else black.
    fn draw_desktop(&mut self, renderer: &Renderer, index: usize, desktop: &wgpu::TextureView, desktop_size: (u32, u32)) {
        if let Some(mirror) = self.mirror.as_ref().filter(|_| self.desktop_mirror) {
            let mirror_start = Instant::now();
            let left_eye = self.images[index].create_view(&wgpu::TextureViewDescriptor {
                label: Some("OpenXR desktop mirror source"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: 0,
                array_layer_count: Some(1),
                ..Default::default()
            });
            mirror.draw(
                renderer,
                &left_eye,
                desktop,
                (self.width, self.height),
                desktop_size,
            );
            self.stats_desktop_mirror += mirror_start.elapsed();
        } else {
            let mut encoder =
                renderer
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("OpenXR blank desktop"),
                    });
            drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("OpenXR blank desktop"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: desktop,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            }));
            renderer.queue.submit(Some(encoder.finish()));
        }
    }

    /// The frame's times added to the statistics, logged every five seconds (with the
    /// renderer's mirror stages under `OMSI_PROFILE`).
    fn record_frame_stats(&mut self, renderer: &Renderer, wait_time: Duration, render_start: Instant, gpu_wait_time: Duration) {
        self.stats_frames += 1;
        self.stats_wait += wait_time;
        self.stats_render += render_start.elapsed();
        self.stats_gpu_wait += gpu_wait_time;
        let interval = self.stats_since.elapsed();
        if interval >= Duration::from_secs(5) {
            let count = self.stats_frames as f64;
            log::info!(
                "OpenXR: {:.1} fps, frame wait {:.1} ms/frame, image wait {:.1}, render {:.1} (eyes {:.1}+{:.1}, desktop mirror {:.1}, GPU sync {:.1} ms/frame)",
                count / interval.as_secs_f64(),
                self.stats_wait.as_secs_f64() * 1000.0 / count,
                self.stats_image_wait.as_secs_f64() * 1000.0 / count,
                self.stats_render.as_secs_f64() * 1000.0 / count,
                self.stats_eyes[0].as_secs_f64() * 1000.0 / count,
                self.stats_eyes[1].as_secs_f64() * 1000.0 / count,
                self.stats_desktop_mirror.as_secs_f64() * 1000.0 / count,
                self.stats_gpu_wait.as_secs_f64() * 1000.0 / count,
            );
            if omsi_cfg::flags::OMSI_PROFILE.is_set() {
                let stages = renderer.stats.borrow();
                let mut detail: Vec<_> = stages
                    .iter()
                    .filter(|(name, _)| name.starts_with("mirror."))
                    .filter_map(|(&name, &total)| {
                        let previous = self.stats_stages_prev.get(name).copied().unwrap_or(0.0);
                        let ms = (total - previous) * 1000.0 / count;
                        (ms >= 0.1).then_some((name, ms))
                    })
                    .collect();
                detail.sort_by(|a, b| b.1.total_cmp(&a.1));
                log::info!(
                    "OpenXR renderer CPU stages: {}",
                    detail
                        .iter()
                        .map(|(name, ms)| format!(
                            "{} {:.1}",
                            name.trim_start_matches("mirror."),
                            ms
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                self.stats_stages_prev.clone_from(&stages);
            }
            self.stats_since = Instant::now();
            self.stats_frames = 0;
            self.stats_wait = Duration::ZERO;
            self.stats_image_wait = Duration::ZERO;
            self.stats_render = Duration::ZERO;
            self.stats_gpu_wait = Duration::ZERO;
            self.stats_eyes = [Duration::ZERO; 2];
            self.stats_desktop_mirror = Duration::ZERO;
        }
    }
}

impl Mirror {
    fn new(renderer: &Renderer) -> Self {
        let device = &renderer.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("OpenXR desktop mirror"),
            source: wgpu::ShaderSource::Wgsl(include_str!("xr_mirror.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("OpenXR desktop mirror"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("OpenXR desktop mirror"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("OpenXR desktop mirror"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: renderer.format(),
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("OpenXR desktop mirror"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("OpenXR desktop mirror crop"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            layout,
            sampler,
            params,
        }
    }

    fn draw(
        &self,
        renderer: &Renderer,
        eye: &wgpu::TextureView,
        desktop: &wgpu::TextureView,
        source_size: (u32, u32),
        desktop_size: (u32, u32),
    ) {
        let source_aspect = source_size.0 as f32 / source_size.1.max(1) as f32;
        let desktop_aspect = desktop_size.0 as f32 / desktop_size.1.max(1) as f32;
        let crop = if desktop_aspect > source_aspect {
            [1.0, source_aspect / desktop_aspect]
        } else {
            [desktop_aspect / source_aspect, 1.0]
        };
        let mut params = [0u8; 16];
        params[0..4].copy_from_slice(&crop[0].to_ne_bytes());
        params[4..8].copy_from_slice(&crop[1].to_ne_bytes());
        renderer.queue.write_buffer(&self.params, 0, &params);
        let bind_group = renderer
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("OpenXR desktop mirror"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(eye),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
        let mut encoder = renderer
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("OpenXR desktop mirror"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("OpenXR desktop mirror"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: desktop,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        // Sampling changed the XR image to a shader resource. OpenXR requires
        // RENDER_TARGET again when we release it, and the load preserves pixels.
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("OpenXR return to render target"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: eye,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }));
        renderer.queue.submit(Some(encoder.finish()));
    }
}

fn transition_first_use(
    renderer: &Renderer,
    resource: &ID3D12Resource,
) -> Result<(ID3D12CommandAllocator, ID3D12GraphicsCommandList)> {
    let device = unsafe { renderer.device.as_hal::<wgpu_hal::api::Dx12>() }
        .ok_or_else(|| anyhow!("OpenXR lost the DX12 graphics device"))?;
    let queue = unsafe { renderer.queue.as_hal::<wgpu_hal::api::Dx12>() }
        .ok_or_else(|| anyhow!("OpenXR lost the DX12 command queue"))?;
    // Submit on wgpu's own queue so its subsequent COMMON -> RENDER_TARGET
    // transition executes after this barrier. Keep the command objects alive
    // until the frame's device.poll completes.
    let allocator: ID3D12CommandAllocator = unsafe {
        device
            .raw_device()
            .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
    }?;
    let list: ID3D12GraphicsCommandList = unsafe {
        device.raw_device().CreateCommandList(
            0,
            D3D12_COMMAND_LIST_TYPE_DIRECT,
            &allocator,
            None::<&ID3D12PipelineState>,
        )
    }?;
    let mut barrier = D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: std::mem::ManuallyDrop::new(Some(resource.clone())),
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: D3D12_RESOURCE_STATE_RENDER_TARGET,
                StateAfter: D3D12_RESOURCE_STATE_COMMON,
            }),
        },
    };
    unsafe {
        list.ResourceBarrier(std::slice::from_ref(&barrier));
        std::mem::ManuallyDrop::drop(&mut (*barrier.Anonymous.Transition).pResource);
        list.Close()?;
        let command_list: ID3D12CommandList = list.cast()?;
        queue.as_raw().ExecuteCommandLists(&[Some(command_list)]);
    }
    Ok((allocator, list))
}

fn xr_position(pose: xr::Posef) -> Vec3 {
    Vec3::new(pose.position.x, pose.position.y, pose.position.z)
}

fn xr_rotation(pose: xr::Posef) -> Quat {
    let q = pose.orientation;
    Quat::from_xyzw(q.x, q.y, q.z, q.w).normalize()
}

fn initial_yaw_rotation(rotation: Quat) -> Quat {
    let forward = rotation * Vec3::NEG_Z;
    let horizontal = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    if horizontal == Vec3::ZERO {
        Quat::IDENTITY
    } else {
        Quat::from_rotation_y((-horizontal.x).atan2(-horizontal.z))
    }
}

fn eye_camera(base: &Camera, position: Vec3, rotation: Quat, fov: xr::Fovf) -> Camera {
    let right = base.right().normalize_or(Vec3::X);
    // The headset-to-world transform must be rigid. Camera::up() returns world
    // up for a level roll, even if the bus camera is pitched on a slope; that
    // vector is then not perpendicular to its forward direction.
    let up = right.cross(base.forward()).normalize_or(Vec3::Y);
    let back = -base.forward();
    let to_world = |v: Vec3| right * v.x + up * v.y + back * v.z;
    let forward = to_world(rotation * Vec3::NEG_Z).normalize();
    let eye_up = to_world(rotation * Vec3::Y).normalize();
    let level_right = Vec3::new(forward.y, -forward.x, 0.0).normalize_or_zero();
    let level_up = level_right.cross(forward);
    Camera {
        position: base.position + DVec3::from(to_world(position)),
        yaw: forward.x.atan2(forward.y).to_degrees(),
        pitch: forward.z.asin().to_degrees(),
        roll: eye_up
            .dot(level_right)
            .atan2(eye_up.dot(level_up))
            .to_degrees(),
        fov_deg: (fov.angle_up - fov.angle_down).to_degrees(),
        ..*base
    }
}

fn eye_projection(fov: xr::Fovf, near: f32, far: f32) -> Mat4 {
    let left = fov.angle_left.tan();
    let right = fov.angle_right.tan();
    let bottom = fov.angle_down.tan();
    let top = fov.angle_up.tan();
    let depth = far - near;
    Mat4::from_cols_array(&[
        2.0 / (right - left),
        0.0,
        0.0,
        0.0,
        0.0,
        2.0 / (top - bottom),
        0.0,
        0.0,
        (right + left) / (right - left),
        (top + bottom) / (top - bottom),
        near / depth,
        -1.0,
        0.0,
        0.0,
        near * far / depth,
        0.0,
    ])
}

fn zoom_eye_position(eye: Vec3, center: Vec3, zoom: f32) -> Vec3 {
    center + (eye - center) / zoom
}

fn zoom_projection(mut projection: Mat4, zoom: f32) -> Mat4 {
    projection.x_axis.x *= zoom;
    projection.y_axis.y *= zoom;
    projection
}

#[cfg(test)]
mod zoom_tests {
    use super::*;

    #[test]
    fn stereo_disparity_stays_stable_through_zoom_animation() {
        let left_eye = Vec3::new(-0.032, 0.0, 0.0);
        let right_eye = Vec3::new(0.032, 0.0, 0.0);
        let left = eye_projection(
            xr::Fovf {
                angle_left: -0.9,
                angle_right: 0.8,
                angle_up: 0.8,
                angle_down: -0.8,
            },
            0.05,
            1000.0,
        );
        let right = eye_projection(
            xr::Fovf {
                angle_left: -0.8,
                angle_right: 0.9,
                angle_up: 0.8,
                angle_down: -0.8,
            },
            0.05,
            1000.0,
        );
        let screen_x = |eye: Vec3, projection: Mat4, point: Vec3| {
            let clip = projection * (point - eye).extend(1.0);
            clip.x / clip.w
        };
        for depth in [0.8, 2.0, 10.0] {
            let point = Vec3::new(0.12, 0.0, -depth);
            let normal = screen_x(left_eye, left, point) - screen_x(right_eye, right, point);
            for zoom in [1.0, 1.1, 1.3, 1.45, 1.6] {
                let magnified = screen_x(
                    zoom_eye_position(left_eye, Vec3::ZERO, zoom),
                    zoom_projection(left, zoom),
                    point,
                ) - screen_x(
                    zoom_eye_position(right_eye, Vec3::ZERO, zoom),
                    zoom_projection(right, zoom),
                    point,
                );
                assert!(
                    (magnified - normal).abs() < 1e-5,
                    "depth {depth}, zoom {zoom}: {magnified} instead of {normal}"
                );
            }
        }
    }
}

fn eye_ray(
    camera: &Camera,
    projection: Mat4,
    x: f32,
    y: f32,
    size: (u32, u32),
    crop: (f32, f32),
) -> Vec3 {
    let w = size.0.max(1) as f32;
    let h = size.1.max(1) as f32;
    let nx = (x / w * 2.0 - 1.0) * crop.0;
    let ny = (1.0 - y / h * 2.0) * crop.1;
    let view = Mat4::look_to_rh(Vec3::ZERO, camera.forward(), camera.up());
    (projection * view)
        .inverse()
        .project_point3(Vec3::new(nx, ny, 0.0))
        .normalize_or_zero()
}
