//! The frame, phase by phase: `Renderer::render_inner` plans what is drawn (the shadow
//! cascades, the culling, the batches) and then encodes the passes in their order. What
//! the phases share is the frame's `FrameCtx`, built once at its start.

use super::*;

pub(crate) mod batch;
mod cull;
mod effects;
mod headlamp_shadow;
mod main_pass;
mod post;
mod precip_map;
pub(crate) use precip_map::*;
mod prepass;
mod setup;
mod shadow;
mod shadow_plan;
mod submit;

pub(crate) use batch::DrawPlan;
pub(crate) use headlamp_shadow::{headlamp_shadow, lamp_shadow_code, own_headlamp, VehicleBox};
pub(crate) use shadow::LampTiles;
pub(crate) use shadow_plan::ShadowPlan;

/// The frame's switches from the environment: read once at the start of every frame
/// (`omsi_cfg::flags`), not wherever they are tested.
pub(crate) struct FrameEnv {
    /// `OMSI_FAKE_GPU_ERROR`: `lost` / `frame`, the device-loss and fallback test hooks
    #[cfg(feature = "test-hooks")]
    pub fake_gpu_error: Option<&'static str>,
    pub no_enhanced: bool,
    pub no_fxaa: bool,
    pub no_glass_picture: bool,
    pub mirror_enhanced: bool,
    pub no_puddle_reflections: bool,
    pub no_rt_frame: bool,
    pub no_ao: bool,
    pub no_ao_reach: bool,
    pub shadow_near_every_frame: bool,
    pub shadow_far_every_frame: bool,
    pub lamp_shadows_every_frame: bool,
    pub debug_shadow_far: bool,
    pub debug_view_lamps: bool,
    pub debug_sky: bool,
    pub debug_draws: bool,
    /// `OMSI_DEBUG_CULL`: set at all, and its value (`x,y,radius`: the instances to dump)
    pub debug_cull: bool,
    pub debug_cull_at: Option<&'static str>,
    /// `OMSI_DEBUG_SHADOW`: set at all, and its value (the smallest radius logged)
    pub debug_shadow: bool,
    pub debug_shadow_r: Option<&'static str>,
    pub debug_flicker: bool,
    pub only_surfaces: bool,
    pub skip_pipe: Option<&'static str>,
    pub no_bundles: bool,
    pub no_msaa_prepass: bool,
    pub no_main_split: bool,
    pub no_post_split: bool,
    pub no_smoke: bool,
    pub no_snowfall: bool,
    pub no_rain: bool,
    pub no_precip_roof: bool,
    pub no_coronas: bool,
    pub no_glare: bool,
    pub debug_fog_lamps: bool,
    pub no_rt_grade: bool,
    pub no_own_vehicle_lamp_block: bool,
    pub sky_cube_every_frame: bool,
}

impl FrameEnv {
    pub(crate) fn read() -> FrameEnv {
        FrameEnv {
            #[cfg(feature = "test-hooks")]
            fake_gpu_error: omsi_cfg::flags::OMSI_FAKE_GPU_ERROR.var(),
            no_enhanced: omsi_cfg::flags::OMSI_NO_ENHANCED.is_set(),
            no_fxaa: omsi_cfg::flags::OMSI_NO_FXAA.is_set(),
            no_glass_picture: omsi_cfg::flags::OMSI_NO_GLASS_PICTURE.is_set(),
            mirror_enhanced: omsi_cfg::flags::OMSI_MIRROR_ENHANCED.is_set(),
            no_puddle_reflections: omsi_cfg::flags::OMSI_NO_PUDDLE_REFLECTIONS.is_set(),
            no_rt_frame: omsi_cfg::flags::OMSI_NO_RT_FRAME.is_set(),
            no_ao: omsi_cfg::flags::OMSI_NO_AO.is_set(),
            no_ao_reach: omsi_cfg::flags::OMSI_NO_AO_REACH.is_set(),
            shadow_near_every_frame: omsi_cfg::flags::OMSI_SHADOW_NEAR_EVERY_FRAME.is_set(),
            shadow_far_every_frame: omsi_cfg::flags::OMSI_SHADOW_FAR_EVERY_FRAME.is_set(),
            lamp_shadows_every_frame: omsi_cfg::flags::OMSI_LAMP_SHADOWS_EVERY_FRAME.is_set(),
            debug_shadow_far: omsi_cfg::flags::OMSI_DEBUG_SHADOW_FAR.is_set(),
            debug_view_lamps: omsi_cfg::flags::OMSI_DEBUG_VIEW_LAMPS.is_set(),
            debug_sky: omsi_cfg::flags::OMSI_DEBUG_SKY.is_set(),
            debug_draws: omsi_cfg::flags::OMSI_DEBUG_DRAWS.is_set(),
            debug_cull: omsi_cfg::flags::OMSI_DEBUG_CULL.is_set(),
            debug_cull_at: omsi_cfg::flags::OMSI_DEBUG_CULL.var(),
            debug_shadow: omsi_cfg::flags::OMSI_DEBUG_SHADOW.is_set(),
            debug_shadow_r: omsi_cfg::flags::OMSI_DEBUG_SHADOW.var(),
            debug_flicker: omsi_cfg::flags::OMSI_DEBUG_FLICKER.is_set(),
            only_surfaces: omsi_cfg::flags::OMSI_ONLY_SURFACES.is_set(),
            skip_pipe: omsi_cfg::flags::OMSI_SKIP_PIPE.var(),
            no_bundles: omsi_cfg::flags::OMSI_NO_BUNDLES.is_set(),
            no_msaa_prepass: omsi_cfg::flags::OMSI_NO_MSAA_PREPASS.is_set(),
            no_main_split: omsi_cfg::flags::OMSI_NO_MAIN_SPLIT.is_set(),
            no_post_split: omsi_cfg::flags::OMSI_NO_POST_SPLIT.is_set(),
            no_smoke: omsi_cfg::flags::OMSI_NO_SMOKE.is_set(),
            no_snowfall: omsi_cfg::flags::OMSI_NO_SNOWFALL.is_set(),
            no_rain: omsi_cfg::flags::OMSI_NO_RAIN.is_set(),
            no_precip_roof: omsi_cfg::flags::OMSI_NO_PRECIP_ROOF.is_set(),
            no_coronas: omsi_cfg::flags::OMSI_NO_CORONAS.is_set(),
            no_glare: omsi_cfg::flags::OMSI_NO_GLARE.is_set(),
            debug_fog_lamps: omsi_cfg::flags::OMSI_DEBUG_FOG_LAMPS.is_set(),
            no_rt_grade: omsi_cfg::flags::OMSI_NO_RT_GRADE.is_set(),
            no_own_vehicle_lamp_block: omsi_cfg::flags::OMSI_NO_OWN_VEHICLE_LAMP_BLOCK.is_set(),
            sky_cube_every_frame: omsi_cfg::flags::OMSI_SKY_CUBE_EVERY_FRAME.is_set(),
        }
    }
}

/// What `render_inner` was asked to draw.
pub(crate) struct FrameArgs<'a> {
    pub target: &'a wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    pub camera: &'a Camera,
    pub lighting: &'a Lighting,
    pub with_overlays: bool,
    pub exclude_texture: Option<TextureId>,
    pub projection: Option<Mat4>,
    pub second_eye: bool,
}

/// What every phase of a frame shares: the view drawn, the frame's mode switches and its
/// targets (see `Renderer::begin_frame`).
pub(crate) struct FrameCtx<'a> {
    pub env: FrameEnv,
    pub target: &'a wgpu::TextureView,
    pub camera: &'a Camera,
    pub lighting: &'a Lighting,
    pub with_overlays: bool,
    pub exclude_texture: Option<TextureId>,
    pub projection: Option<Mat4>,
    pub second_eye: bool,
    /// the render origin
    pub ro: DVec3,
    /// the window's width (the HUD is drawn at the window's size)
    pub full_w: u32,
    /// the 3D picture's size (smaller than the window's with a render scale)
    pub width: u32,
    pub height: u32,
    pub vanilla_fxaa: bool,
    pub glass_on: bool,
    pub scaled: bool,
    pub scene_target: Option<(wgpu::TextureView, wgpu::BindGroup)>,
    pub aspect: f32,
    pub cam_rel: Vec3,
    pub xr_view: bool,
    pub lead_view: bool,
    pub enhanced_frame: bool,
    pub enhanced: bool,
    pub puddles_wanted: bool,
    pub reflection_frame: bool,
    pub masked_frame: bool,
    pub grid: [f32; 4],
    pub lamp_shadows: Vec<LampShadow>,
    pub rt_frame: bool,
    pub ao_on: bool,
    pub prepass_on: bool,
    pub prepass_reach: f32,
    pub dt: f32,
    pub overlays: Vec<(TextureId, [f32; 4])>,
}

impl FrameCtx<'_> {
    /// where the 3D picture is drawn: the scaled target, or the window's own
    pub(crate) fn scene_view(&self) -> &wgpu::TextureView {
        self.scene_target.as_ref().map(|t| &t.0).unwrap_or(self.target)
    }
}

/// OMSI_GPU_TIMERS: the query set of this frame (when it is timed) and the passes timed
/// so far.
pub(crate) struct PassTimers {
    pub set: Option<wgpu::QuerySet>,
    pub timed: Vec<&'static str>,
}

/// OMSI_PROFILE: time per stage, the mirrors apart from the window's picture
pub(crate) struct StageClock {
    t: std::time::Instant,
    with_overlays: bool,
}

impl StageClock {
    pub(crate) fn new(with_overlays: bool) -> StageClock {
        StageClock { t: std::time::Instant::now(), with_overlays }
    }

    pub(crate) fn stage(&mut self, r: &Renderer, window: &'static str, mirror: &'static str) {
        if r.profiling {
            let now = std::time::Instant::now();
            *r.stats
                .borrow_mut()
                .entry(if self.with_overlays { window } else { mirror })
                .or_default() += (now - self.t).as_secs_f64();
            self.t = now;
        }
    }
}

/// a material's alpha mode as the pipeline kind it is drawn with
pub(crate) fn kind_of(alpha: AlphaMode) -> u8 {
    match alpha {
        AlphaMode::Opaque => PIPE_OPAQUE,
        AlphaMode::Test => PIPE_ALPHA_TEST,
        AlphaMode::Blend => PIPE_BLEND,
    }
}

/// the three command encoders of a frame, finished side by side (see `finish_and_submit`):
/// the shadow maps, the depth prepass with the ambient occlusion, and the picture itself
pub(crate) struct Encoders {
    pub shadow: wgpu::CommandEncoder,
    pub prepass: wgpu::CommandEncoder,
    pub picture: wgpu::CommandEncoder,
}
