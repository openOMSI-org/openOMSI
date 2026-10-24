//! wgpu renderer.
//!
//! `Renderer` holds the device and what lives as long as it; a `Scene` holds what is drawn.
//! Where things are:
//! - `gpu/`: the device - opening it, building the pipelines (with fallbacks), adapter
//!   capabilities, bind group layouts, timers and errors; `pipelines/`: the pipelines
//! - `resources/`: meshes, textures, materials, and freed slots taken over again
//! - `scene/`: the scene and its instances, and their upload each frame
//! - `view.rs`: camera and lighting parameters; `uniforms.rs`: what the shaders read
//! - `lighting/`: point lights, coronas and smoke, and the enhanced sky and exposure model
//! - `targets.rs`, `sky.rs`: the size-keyed targets and the sky's resources
//! - `frame_plan.rs` and `passes/`: what a frame draws, and its passes in order;
//!   `render.rs`: the entry points (window, triple screen, headset, images)
//! - `shaders.rs` + `shaders/`: the long WGSL modules as parts, and their per-backend rewrites
//!
//! Items of these modules are re-exported here, so the crate's paths are flat.

pub mod angle;
pub mod atmosphere;
pub mod clouds;
mod cookies;
pub use cookies::{Cookie, COOKIE_LAYERS};
mod gpu_memory;
pub use gpu_memory::{Ceiling, GpuMemory};
mod passes;
mod device_selection;
use passes::{Encoders, FrameArgs, FrameEnv, PassTimers, StageClock};
use gpu_memory::adapter_vram_mb;
pub mod pipeline_cache;
mod pipelines;
mod puddles;
mod rt;
mod rt_tex;
mod shaders;
mod srgb_encode;
mod triple;
mod frame_plan;
mod gpu;
mod lighting;
mod render;
mod resources;
mod scene;
mod sky;
mod surface;
mod targets;
mod uniforms;
mod view;
pub(crate) use frame_plan::*;
pub use gpu::*;
pub use lighting::*;
pub(crate) use render::*;
pub use resources::*;
pub use scene::*;
pub(crate) use shaders::*;
pub use sky::*;
pub use surface::*;
pub(crate) use targets::*;
pub use uniforms::*;
pub use view::*;
pub use triple::{panel_width, ScreenView, TripleScreen};

use anyhow::{anyhow, Context, Result};
use glam::{DVec3, Mat4, Vec3, Vec4};
use omsi_geometry::MeshData;
use std::collections::HashMap;
use std::sync::Arc;

pub type MeshId = usize;
pub type TextureId = usize;
pub type MaterialId = usize;

/// RCAS's strength for the FSR sharpening slider (0..1): its whole range onto 0..0.8 of AMD's
/// lobe. Measured on a GCN card, AMD's strongest (1.0) grained the gauges and outlined the
/// bus and the trees, 0.8 was as sharp as the standard upscaler and clean - so that is 100 %.
pub(crate) fn fsr1_rcas_strength(slider: f32) -> f32 {
    0.8 * slider.clamp(0.0, 1.0)
}

/// Optional FSR1 experiment from the environment (`OMSI_FSR1`), read once for the run (the
/// setting itself can change while the game runs: `Renderer::set_upscaler`). The standard
/// upscaler and all shader pipelines stay as-is when neither asks for it.
pub(crate) fn fsr1_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        omsi_cfg::flags::OMSI_FSR1.var()
            .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "on"))
    })
}

/// The enhanced path's post pipelines (post.wgsl).
struct PostPipelines {
    down_first: wgpu::RenderPipeline,
    down: wgpu::RenderPipeline,
    up: wgpu::RenderPipeline,
    meter: wgpu::RenderPipeline,
    adapt: wgpu::RenderPipeline,
    /// Straight into the target, or gamma-encoded for FXAA.
    tonemap: wgpu::RenderPipeline,
    tonemap_encoded: wgpu::RenderPipeline,
    fxaa: wgpu::RenderPipeline,
    /// The picture as absolute radiance at half size, for the next frame's bounced light.
    history: wgpu::RenderPipeline,
}

/// The pipelines of the main pass for one colour target format: the swap chain's, and
/// the high-range one of the enhanced path.
struct PassPipelines {
    /// Single-sampled films, drawn after resolving the scene and its reflections.
    rain_pipelines: Vec<wgpu::RenderPipeline>,
    /// Indexed by `pipe_code`: 4 depth/blend kinds x culled x depth-biased.
    pipelines: Vec<wgpu::RenderPipeline>,
    corona_pipeline: wgpu::RenderPipeline,
    /// Smoke particles (`[smoke]`): the corona sprite alpha-blended with the smoke texture.
    smoke_pipeline: wgpu::RenderPipeline,
    /// The snowfall (snow.wgsl), opaque over the scene (premultiplied); none with
    /// `basic_pipelines`.
    snow_pipeline: Option<wgpu::RenderPipeline>,
    /// The rainfall (rain.wgsl), as the snowfall; with `basic_pipelines` on computers too.
    rain_fall_pipeline: Option<wgpu::RenderPipeline>,
    sky_pipeline: wgpu::RenderPipeline,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_name: String,
    camera_layout: wgpu::BindGroupLayout,
    material_layout: wgpu::BindGroupLayout,
    pass: PassPipelines,
    hdr_pass: Option<PassPipelines>,
    reflection_pass: Option<PassPipelines>,
    overlay_pipeline: wgpu::RenderPipeline,
    overlay_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    camera_buf: wgpu::Buffer,
    white_texture: GpuTexture,
    black_texture: GpuTexture,
    /// A tangent-space normal pointing straight out (the PBR normal map's stand-in).
    flat_normal_texture: GpuTexture,
    format: wgpu::TextureFormat,
    ao_sampler: wgpu::Sampler,
    ao_layout: wgpu::BindGroupLayout,
    ao_buf: wgpu::Buffer,
    /// Depth prepass pipelines (plain, alpha-tested; each two-sided and culled),
    /// single-sampled, camera projection.
    /// Plain opaque, alpha-tested, and the opaque portions of blended transmap materials;
    /// each has a two-sided and a culled variant.
    prepass_pipelines: [wgpu::RenderPipeline; 6],
    /// The same, at the main pass's sample count (colour writes off):
    /// its own depth laid first (see `encode_prefill`), so that its costly shading runs
    /// once per visible surface.
    prepass_msaa_pipelines: Option<[wgpu::RenderPipeline; 6]>,
    /// Ambient occlusion and its blur; none on OpenGL (GLES), whose shading language cannot
    /// read a depth texture texel by texel - the pipelines failed there, AO off or not (#422).
    ssao_pipeline: Option<wgpu::RenderPipeline>,
    blur_pipeline: Option<wgpu::RenderPipeline>,
    /// (into two-channel targets: see `pipelines::ssao::Ssao`)
    plain_ssao_pipeline: Option<wgpu::RenderPipeline>,
    plain_blur_pipeline: Option<wgpu::RenderPipeline>,
    /// The settings this renderer was built with.
    pub options: RenderOptions,
    puddles: Option<puddles::Pipelines>,
    /// The projection's width over height while a picture is drawn into a texture (see
    /// `render_to_texture`), whatever the texture's own shape.
    texture_aspect: Option<f32>,
    last_frame: Option<std::time::Instant>,
    /// The adapter the device was opened on, and what its driver last said the game holds
    /// on the card (see `gpu_memory`).
    adapter: Option<wgpu::Adapter>,
    gpu_memory_seen: std::cell::RefCell<gpu_memory::Seen>,
    /// The next frame stands alone (an offscreen picture): the exposure is there at once.
    pub instant_exposure: bool,
    /// The next picture keeps its depth to be read back (`render_to_image_with_depth`): the
    /// depth prepass runs whatever else it would.
    pub(crate) depth_readback: bool,
    /// Overlay pipeline without multisampling, for drawing the HUD after the post pass.
    overlay_pipeline_1x: wgpu::RenderPipeline,
    xr_ui_pipeline: wgpu::RenderPipeline,
    started: std::time::Instant,
    /// Windy trees: how far the wind has carried its gusts (m, modulo 1000) and when.
    tree_gust_drift: std::cell::Cell<(glam::DVec2, f64)>,
    /// `[matl_texadress_clamp]` (and border, mirror-once) and `[matl_texadress_mirror]`.
    clamp_sampler: wgpu::Sampler,
    mirror_sampler: wgpu::Sampler,
    /// The addressing of the next material's textures (`[matl_texadress_*]`).
    pub address_next: std::cell::Cell<TexAddressing>,
    /// The next material is lit at night by the tile light maps (`[LightMapMapping]`, the
    /// splines) instead of the map's lamps.
    pub light_map_next: std::cell::Cell<bool>,
    /// Mipmap generation on the GPU (a blit per level).
    mip_pipeline: wgpu::RenderPipeline,
    /// The same for text and script textures (`MipAverage::Independent`).
    mip_text_pipeline: wgpu::RenderPipeline,
    mip_layout: wgpu::BindGroupLayout,
    mip_sampler: wgpu::Sampler,
    /// The headset's pictures: the heading (degrees) the `[matl_envmap]` sphere maps are
    /// laid out by instead of each eye's view (see `set_env_heading`).
    env_heading: std::cell::Cell<Option<f32>>,
    /// Sort the blended draws by origin distance alone, as before the camera-enclosing
    /// objects were drawn last (only for before/after pictures, `OMSI_BLEND_AB`).
    pub blend_by_origin: bool,
    /// Draw the models' `[isshadow]` shadow blobs (see [`RenderOptions::shadow_blobs`]).
    /// Settable while the game runs, so the graphics list can switch it off at once.
    pub shadow_blobs: bool,
    /// GPU time per pass (OMSI_GPU_TIMERS, when the device has timestamp queries): the
    /// mirrors and the window's picture apart, each timed on its own.
    gpu_timers: [Option<GpuTimers>; 2],
    /// Pictures drawn since start, the mirrors' and the window's (as `gpu_timers`): a mirror's
    /// GPU time is measured one picture at a time, and a frame may draw two.
    pictures: std::cell::Cell<[u64; 2]>,
    /// Seconds spent per stage of `render_inner` since start (OMSI_PROFILE).
    pub stats: std::cell::RefCell<std::collections::BTreeMap<&'static str, f64>>,
    /// What the window's pictures drew since start, summed (OMSI_PROFILE): instances that
    /// passed the culling and draws per pass.
    pub counts: std::cell::RefCell<std::collections::BTreeMap<&'static str, f64>>,
    pub frame_draws: (usize, usize),
    profiling: bool,
    draw_audit_at: std::time::Instant,
    /// Reuse a small set of encoding workers instead of creating OS threads for each
    /// main/mirror picture. Keep these separate from simulation's worker queue.
    encoding_pool: Option<rayon::ThreadPool>,
    _device_poller: Option<DevicePoller>,
    /// Vertex data of changed meshes (skinned people, the driver) waiting for the next
    /// picture: (mesh, bytes). Written with one staging buffer and a copy each at the start
    /// of the frame - a `write_buffer` per mesh made wgpu create a staging buffer for every
    /// one of them, forty a frame with a crowd at a stop.
    pending_meshes: std::cell::RefCell<Vec<(MeshId, Vec<u8>)>>,
    /// What a freed mesh and a freed material hold (see `free_mesh`), made once.
    freed: std::cell::OnceCell<Freed>,
    /// Enhanced+: the ray tracer (on a device with ray queries, see `RenderOptions::ray_tracing`).
    rt: Option<rt::RayTracer>,
    /// Meshes share pages of buffers (the adapter draws with a base vertex).
    mesh_pages: bool,
    /// The size-keyed targets (see targets.rs): made on first use, let go of when unused.
    targets: Targets,
    /// The render scale: the pipelines that scale the 3D picture up to the window or copy a panel, and the governor's factor.
    scale: Upscale,
    /// The sky: its layout, sampler, dome, the classic sky's weather, the enhanced clouds' noise, the sky table and the sky being computed.
    sky: SkyRes,
    /// The lights' resources beside the scene's grid: coronas, smoke, snowfall, cookies, the tile light maps and the lamps in the fog.
    lights: LightRes,
    /// The enhanced picture's post pass and exposure: pipelines, metering, the eye's adaptation, the lighting uniform and probe.
    tone: ToneMapping,
    /// The sun's and the lamps' shadow maps, their pipelines, and the cascades as last drawn.
    shadows: ShadowRes,
    /// What the views drew in their last frames, which the next frame's culling and bundles go by.
    history: ViewHistory,
    /// What the device's error handlers report: errors with multisampling or ray tracing on, out of memory, the device lost.
    errors: GpuErrors,
}

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Samples per pixel of the scene passes when nothing else is asked for (the shadow maps
/// stay single-sampled). Four samples take the staircase off every edge; the multisampled
/// colour target resolves into the window or the mirror texture at the end of the main pass.
pub const MSAA: u32 = 4;
/// Sun shadow map resolution when nothing else is asked for.
pub const SHADOW_SIZE: u32 = 2048;

/// What the renderer is built with: the user's graphics settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderOptions {
    /// Samples per pixel: 1 (off), 2, 4 or 8.
    pub msaa: u32,
    /// Anisotropic filtering: 1 (off) .. 16.
    pub anisotropy: u16,
    /// Sun shadow map size per cascade (1024, 2048, 4096).
    pub shadow_size: u32,
    /// Screen-space ambient occlusion.
    pub ssao: bool,
    /// The 3D picture of the window drawn at this fraction of its size and scaled up
    /// (0.5..1); 0 = automatic: full size up to `AUTO_SCALE_PIXELS`, smaller above.
    pub render_scale: f32,
    /// Experimental FSR1 spatial upscaler (at the start, or later with `set_upscaler`).
    pub fsr1: bool,
    /// RCAS sharpening strength, from 0 (none) to 1.
    pub fsr_sharpness: f32,
    /// VR: each eye is drawn at this fraction of its headset image and scaled up to it (by
    /// FSR 1, or the standard upscaler with FSR 1 off); 1 = drawn at the image's size.
    /// Set by `start_xr`: below 1 only when VR started with FSR 1.
    pub xr_scale: f32,
    /// Uncompressed texture files (BMP, TGA, JPG) are compressed to BC1/BC3 on loading
    /// where the device takes block formats and the result is close to the picture
    /// (DXT files always go up as blocks there).
    pub compress_textures: bool,
    /// DXT/BC textures may stay compressed on the GPU (where it takes them); off, the
    /// device is opened without block formats and every texture goes up as RGBA, as with
    /// `OMSI_NO_BC=1` (the settings' `gpu_texture_compression`, for a driver that
    /// mishandles them).
    pub gpu_texture_compression: bool,
    /// FXAA over the enhanced path's tone-mapped picture.
    pub fxaa: bool,
    /// The original's `performance_minObjSize` (see `Lighting::min_obj_size`, which may
    /// raise it for one picture, as the mirrors do).
    pub min_obj_size: f32,
    /// The original's `performance_maxObjDist` (m): objects farther away are not drawn
    /// unless they say `[noDistanceCheck]`. 0 = no limit.
    pub max_obj_dist: f32,
    /// The mirrors of an Enhanced picture are shaded as the picture is (the settings'
    /// `mirror_quality=enhanced`), not with the plain light (see `Renderer::render_inner`).
    pub mirror_enhanced: bool,
    /// The mirrors show the sun's shadows (the window's shadow maps, as they were last drawn,
    /// are looked up from the mirror's camera: no shadow pass of their own).
    pub mirror_shadows: bool,
    /// The mirrors show the smoke and spray particles as the window last had them (OMSI's
    /// `No PS in reflections`, turned round).
    pub mirror_particles: bool,
    /// Only the meshes the models mark `[shadow]` cast sun shadows, as in OMSI 2 (else every
    /// solid mesh does).
    pub omsi_shadow_casters: bool,
    /// Draw the models' `[isshadow]` shadow meshes - OMSI's flat blob under a vehicle,
    /// standing in for the sky light the body keeps off the road (see [`Instance::blob`]).
    /// Off, the sun shadow map is all the shading under a vehicle, and the blob (which
    /// OMSI draws whatever the depth) cannot be seen at all.
    pub shadow_blobs: bool,
    /// The materials' reflection maps (`[matl_envmap]`: the shine of paint, chrome and
    /// glass). Off, nothing mirrors the sky photo - some players find it too strong.
    pub reflections: bool,
    /// Enhanced graphics are not asked for: a phone or OpenGL then leaves their pipelines,
    /// sky shader, cloud noise and probe out (compiling the ray-marched clouds' sky killed
    /// Mali and Adreno drivers before the first frame, #364, #333, #316, #371). Asked for,
    /// they are built where the texture layout supports them. Native desktop backends
    /// keep them available for switching modes in-game.
    pub no_enhanced: bool,
    /// The launcher's preview (the showroom) in plain graphics: no enhanced pipelines,
    /// reflection probe or cloud noise are made for it, on any device.
    pub preview_only: bool,
    /// Enhanced+: the enhanced path with ray-traced sun shadows, ambient occlusion and
    /// reflections, where the device can trace rays (hardware ray queries); elsewhere the
    /// enhanced picture as it is.
    pub ray_tracing: bool,
    /// The Enhanced picture's street-scale light: the sky's occlusion by houses and trees
    /// over 18 m, and the light the surfaces send to each other (the settings'
    /// `street_light`). `None`: as the chip allows (off on a small or shared one, see
    /// `gpu_memory::lighter_picture`).
    pub street_light: Option<bool>,
    pub max_scene: Option<(u32, u32)>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            msaa: MSAA,
            anisotropy: 8,
            shadow_size: SHADOW_SIZE,
            ssao: true,
            render_scale: 0.0,
            fsr1: false,
            xr_scale: 1.0,
            fsr_sharpness: 0.8,
            compress_textures: true,
            gpu_texture_compression: true,
            fxaa: true,
            min_obj_size: 0.013,
            max_obj_dist: 0.0,
            mirror_enhanced: false,
            mirror_shadows: false,
            mirror_particles: false,
            omsi_shadow_casters: false,
            shadow_blobs: true,
            reflections: true,
            no_enhanced: false,
            preview_only: false,
            ray_tracing: false,
            street_light: None,
            max_scene: None,
        }
    }
}

/// Automatic render scale: a window of up to this many pixels is drawn at full size (the
/// default 1600x900 window and a 2560x1080 screen are); a bigger one - a Retina window has
/// four times the pixels of its size in points - gets a 3D picture of about this many
/// pixels, scaled up. The HUD is always drawn at full size. Elsewhere (a desktop card on a
/// 1440p or 4K screen) the picture is drawn at full size up to 4K: scaled down to 2.8
/// million pixels, a 4K screen showed a picture of 58 % its size, and the enhanced
/// graphics looked like textures of low quality; the frame-rate governor still steps down
/// on a card that cannot keep up.
pub const AUTO_SCALE_PIXELS: f32 = if cfg!(any(target_os = "macos", target_os = "android", target_os = "ios")) { 2_800_000.0 } else { 8_400_000.0 };

/// Interior lamps one mesh may be lit by (OMSI: four; a model may list more in its
/// `[illumination_interior]`), and the step of the lamp code sent to the shaders (`first *
/// stride + count`, exact in the f32 it travels in for a quarter of a million lamp slots).
pub const MAX_LAMPS_PER_MESH: u32 = 63;
pub const LAMP_CODE_STRIDE: u32 = 64;

/// The enhanced pass's second target: r is 1 where the bus's own screens are
/// (`MaterialExtra::screen`), 0 elsewhere - the glow takes no light from them and FXAA
/// passes them through; g is 1 on an LED panel's own dots (`MaterialExtra::led`), which the
/// glow's source keeps and multiplies up (see `post.wgsl`). b carries the puddle's
/// reflected-light weight; a is blend coverage. Sharing this attachment avoids another
/// geometry pass or a full normal/material buffer just for water.
const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Enhanced+: the enhanced pass's third and fourth targets, what the ray-traced reflections
/// start from (rt.wgsl): the surface's normal (xyz) and how much it reflects (w), and its
/// view depth and roughness (rg) - all weighted by that reflection (premultiplied), so that
/// a pane blended over a wet road leaves the stronger reflection's surface in them.
const GBUF_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const AUX_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
/// The renderer is built for Enhanced+: its HDR pipelines and targets have the two above.
static RT_GBUF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn rt_gbuf() -> bool {
    RT_GBUF.load(std::sync::atomic::Ordering::Relaxed)
}
const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The colour targets of a pipeline drawing into `format`: Enhanced and classic puddle
/// shading draw into `HDR_FORMAT` with the screen mask beside it,
/// written by the scene's own shader only (`mask`), coverage-blended where the colour is.
fn color_targets(format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>, write: wgpu::ColorWrites, mask: bool, gbuf: bool) -> Vec<Option<wgpu::ColorTargetState>> {
    let mut v = vec![Some(wgpu::ColorTargetState { format, blend, write_mask: write })];
    if format == HDR_FORMAT {
        v.push(Some(wgpu::ColorTargetState {
            format: MASK_FORMAT,
            blend: blend.map(|_| wgpu::BlendState {
                color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
                alpha: wgpu::BlendComponent::REPLACE,
            }),
            write_mask: if mask { wgpu::ColorWrites::ALL } else { wgpu::ColorWrites::empty() },
        }));
        if rt_gbuf() {
            // (premultiplied by the reflection's weight, the shader's alpha)
            let over = blend.map(|_| wgpu::BlendState {
                color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
                alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
            });
            let write = if gbuf { wgpu::ColorWrites::ALL } else { wgpu::ColorWrites::empty() };
            v.push(Some(wgpu::ColorTargetState { format: GBUF_FORMAT, blend: over, write_mask: write }));
            v.push(Some(wgpu::ColorTargetState { format: AUX_FORMAT, blend: over, write_mask: write }));
        }
    }
    v
}
/// Half size of the area around the camera covered by the near shadow cascade (m).
pub const SHADOW_RANGE: f32 = 140.0;
/// Half size of the far cascade (m): coarser, but reaches the whole visible street.
pub const SHADOW_RANGE_FAR: f32 = 700.0;
/// Half size of the close cascade around the camera (m). The near cascade's texel is
/// 280 m / 2048 = 14 cm, and a moving bus's shadow edge stepped from texel to texel: the
/// shadow trembled while driving. The close map (the right half of the near map's atlas)
/// has 3 cm texels for the bus and everything next to it.
pub const SHADOW_RANGE_CLOSE: f32 = 32.0;
/// The close cascade's map is drawn no bigger than this (texels a side), whatever the shadow
/// setting: 3 cm texels. At the 4096 setting it had been 1.6 cm, and drawing it was a third
/// of the shadow pass's 4.6 ms.
const SHADOW_CLOSE_MAX: u32 = 2048;

impl Renderer {

    fn main_pass(&self, enhanced: bool, reflections: bool) -> &PassPipelines {
        if enhanced { self.hdr_pass.as_ref().unwrap() }
        else if reflections { self.reflection_pass.as_ref().unwrap() }
        else { &self.pass }
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// The fraction of a window of this size the 3D picture is drawn at (`render_scale`).
    pub fn scene_scale(&self, width: u32, height: u32) -> f32 {
        let s = scene_scale_for(self.options.render_scale, width, height);
        let s = scene_scale_within(s, self.options.max_scene, width, height);
        // (a fixed render scale is the player's choice: the governor works on automatic)
        if self.options.render_scale > 0.0 {
            s
        } else {
            (s * self.scale.dynamic_scale.get()).clamp(0.5, 1.0)
        }
    }

    /// Draw the 3D picture at this fraction of what the render scale says (0.6..1): the
    /// game lowers it while a slow graphics chip cannot keep the frame rate up, and raises
    /// it again when there is room. Steps of a twentieth, so that the few sizes it takes
    /// keep their render targets.
    pub fn set_dynamic_scale(&self, s: f32) {
        // (1, 0.85, 0.7 or 0.55 - the last for a phone's chip that is still too slow: see
        // the game's governor)
        let s = s.clamp(0.55, 1.0);
        let level = [1.0f32, 0.85, 0.7, 0.55].into_iter().min_by(|a, b| (a - s).abs().total_cmp(&(b - s).abs())).unwrap_or(1.0);
        self.scale.dynamic_scale.set(level);
    }

    pub fn dynamic_scale(&self) -> f32 {
        self.scale.dynamic_scale.get()
    }

    /// The upscaler, its sharpening and the render scale while the game runs: FSR 1's
    /// passes are built the first time it is asked for (nothing for whoever never does).
    /// False when FSR 1 was asked for and the driver has just refused it (said once): the
    /// standard upscaler stays, and it is not tried again in this run.
    pub fn set_upscaler(&mut self, fsr1: bool, sharpness: f32, render_scale: f32) -> bool {
        self.scale.upscaler_asked = (fsr1, sharpness, render_scale);
        self.options.fsr_sharpness = sharpness;
        self.options.render_scale = render_scale;
        let mut refused_now = false;
        if fsr1 && self.scale.fsr.is_none() && !self.scale.fsr_refused {
            match pipelines::upscale::build_fsr(&self.device, self.format, &self.scale.upscale_layout) {
                Some(p) => self.scale.fsr = Some(p),
                None => (self.scale.fsr_refused, refused_now) = (true, true),
            }
        }
        self.options.fsr1 = fsr1 && self.scale.fsr.is_some();
        // Keep compiled pipelines for quick toggles, but release full-resolution images
        // while neither RCAS nor FSR needs them. They are recreated on the next draw.
        if !self.options.fsr1 || fsr1_rcas_strength(sharpness) == 0.0 {
            self.scale.fsr_target.get_mut().clear();
        }
        !refused_now
    }

    /// FSR 1, its sharpening and the render scale as last asked for (`set_upscaler`; at the
    /// start, the settings'): a menu change of something else leaves the upscaler alone.
    pub fn upscaler_asked(&self) -> (bool, f32, f32) {
        self.scale.upscaler_asked
    }

    /// VR starts with eyes of `eye_scale` (the eye resolution): the headset image's size as
    /// a share of the runtime's recommendation. With FSR 1 on and built, the image is
    /// full-sized and each eye is drawn at `eye_scale` of it and scaled up by FSR 1 (by
    /// the standard upscaler if FSR 1 is turned off later); else the image itself is that
    /// small and the runtime scales it, as before. `OMSI_OPENXR_SCALE` takes the place of
    /// `eye_scale` here, and only here (read again for the headset image as well, it shrank
    /// the image and FSR 1 shrank the eyes within it a second time).
    pub fn start_xr(&mut self, eye_scale: f32) -> f32 {
        let eye_scale = omsi_cfg::flags::OMSI_OPENXR_SCALE.parse::<f32>().filter(|s| s.is_finite()).unwrap_or(eye_scale).clamp(0.5, 1.0);
        let fsr = self.options.fsr1 && self.scale.fsr.is_some() && eye_scale < 0.999;
        self.options.xr_scale = if fsr { eye_scale } else { 1.0 };
        if fsr { 1.0 } else { eye_scale }
    }

    /// The fraction of the headset's image an eye is drawn at (`start_xr`), when below 1.
    pub(crate) fn xr_fsr_scale(&self) -> Option<f32> {
        let s = self.options.xr_scale.clamp(0.5, 1.0);
        (s < 0.999).then_some(s)
    }

    /// FSR 1 upscales the window's picture now (asked for, built, and the picture drawn
    /// smaller than the window).
    pub fn fsr_active(&self, width: u32, height: u32) -> bool {
        self.options.fsr1 && self.scale.fsr.is_some() && self.scene_size(width, height) != (width, height)
    }

    /// The size the 3D scene is drawn at for a window of this size.
    pub fn scene_size(&self, width: u32, height: u32) -> (u32, u32) {
        let s = self.scene_scale(width, height);
        if s >= 0.999 {
            return (width, height);
        }
        (
            ((width as f32 * s).round() as u32).max(1),
            ((height as f32 * s).round() as u32).max(1),
        )
    }
}

/// The render scale for a picture of this size: the requested one (0.5..1), or with 0
/// (automatic) full size up to `AUTO_SCALE_PIXELS` and that many pixels above it.
/// A requested one keeps to the same budget on a Mac or a phone: the Low preset's fixed
/// 0.75 of a Retina window drew more pixels than the budget allows (3.24 of 2.8 million
/// on a 3200x1800 window), and the automatic scale would never have drawn that many.
fn scene_scale_for(requested: f32, width: u32, height: u32) -> f32 {
    let pixels = width as f32 * height as f32;
    if requested > 0.0 {
        let requested = requested.clamp(0.5, 1.0);
        if cfg!(any(target_os = "macos", target_os = "android", target_os = "ios")) && pixels > AUTO_SCALE_PIXELS {
            return requested.min((AUTO_SCALE_PIXELS / pixels).sqrt().clamp(0.5, 1.0));
        }
        return requested;
    }
    if pixels <= AUTO_SCALE_PIXELS {
        1.0
    } else {
        (AUTO_SCALE_PIXELS / pixels).sqrt().clamp(0.5, 1.0)
    }
}

fn scene_scale_within(s: f32, max_scene: Option<(u32, u32)>, width: u32, height: u32) -> f32 {
    max_scene.map_or(s, |(w, h)| s.min(w as f32 / width as f32).min(h as f32 / height as f32).max(0.5))
}

/// Is `p` inside a vehicle's `[boundingbox]` (origin, heading in degrees, w l h cx cy cz)?
/// The same box the shader keeps the weather out of, without its margins.
fn point_in_vehicle_box(p: DVec3, (origin, heading, bb): &(DVec3, f64, [f32; 6])) -> bool {
    let d = (p - *origin).as_vec3();
    let (sh, ch) = (*heading as f32).to_radians().sin_cos();
    let x = d.x * ch - d.y * sh - bb[3];
    let y = d.x * sh + d.y * ch - bb[4];
    let z = d.z - bb[5];
    x.abs() < bb[0] * 0.5 && y.abs() < bb[1] * 0.5 && z.abs() < bb[2] * 0.5
}

/// An overlay's rectangle (physical pixels) moved onto whole pixels, its size kept. The
/// overlays are pictures drawn texel for pixel - a text, a plate - and the linear filter
/// blended every pixel of one placed between pixels with its neighbour: the interface's texts
/// were soft at every size whose layout fell between them (most but 100 %, and the timetable's
/// rows at that too). A line thinner than a pixel stays one pixel wide or high.
fn snap_rect(r: [f32; 4]) -> [f32; 4] {
    // (half up the same way left of the window as right of it: `round` goes away from zero,
    // and a rectangle across the left edge came out a pixel wider)
    let snap = |v: f32| (v + 0.5).floor();
    let (x0, y0) = (snap(r[0]), snap(r[1]));
    let x1 = if r[2] > r[0] { snap(r[2]).max(x0 + 1.0) } else { snap(r[2]) };
    let y1 = if r[3] > r[1] { snap(r[3]).max(y0 + 1.0) } else { snap(r[3]) };
    [x0, y0, x1, y1]
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod stream_tests;

/// Panics caught on purpose (a driver wgpu cannot use): see `omsi_cfg::panic_guard`.
pub use omsi_cfg::panic_guard::{catch, catching};
