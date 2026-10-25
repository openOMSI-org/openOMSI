//! The data the shaders read, laid out as they declare it: the vertex, and the uniforms of
//! the camera, the post pass, the enhanced lighting, the lamps in the fog and the AO.

use crate::*;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct CameraUniform {
    pub(crate) view_proj: [[f32; 4]; 4],
    pub(crate) cam_pos: [f32; 4],
    /// World-space render origin modulo 1000 m (the procedural patterns' period), added back
    /// for procedural patterns that must not jump when the floating origin moves to the next
    /// 100 m cell.
    pub(crate) world_origin: [f32; 4],
    pub(crate) sun_dir: [f32; 4],
    pub(crate) ambient: [f32; 4],
    pub(crate) fog: [f32; 4],
    pub(crate) sun_color: [f32; 4],
    pub(crate) sky_color: [f32; 4],
    pub(crate) light_grid: [f32; 4],
    pub(crate) sky: [f32; 4],
    pub(crate) cam_right: [f32; 4],
    pub(crate) cam_up: [f32; 4],
    pub(crate) clouds: [f32; 4],
    pub(crate) light_view_proj: [[f32; 4]; 4],
    pub(crate) light_view_proj_far: [[f32; 4]; 4],
    pub(crate) shadow: [f32; 4],
    /// x enhanced graphics (0/1), y seconds since start, z sun ndc x, w sun ndc y
    pub(crate) post: [f32; 4],
    /// The player's vehicle as a box the weather does not reach into: origin (render-origin
    /// relative) and sin(heading); cos(heading) and the half extents; the box centre offset
    /// and whether there is one.
    pub(crate) inside_a: [f32; 4],
    pub(crate) inside_b: [f32; 4],
    pub(crate) inside_c: [f32; 4],
    /// x procedural detail texturing (0/1), y soft shadow penumbra (0/1), z aerial
    /// perspective strength, w the close shadow cascade's half range
    pub(crate) flags: [f32; 4],
    /// The close shadow cascade (right half of the near map).
    pub(crate) light_view_proj_close: [[f32; 4]; 4],
    /// The player's vehicle's velocity (m/s, world) and 1: the airstream the rain on its
    /// glass meets (see `Lighting::glass_wind`).
    pub(crate) wind: [f32; 4],
    /// Enhanced: the lamps' shadow maps (`LAMP_SHADOWS` tiles under the far map, zero for a
    /// tile nothing is drawn in), and where the tiles lie (`LampAtlas::uniform`: a tile's
    /// side, tiles to a row, the far map's texture height, tiles in all). A light names its
    /// tile in its record (`LAMP_MARK`).
    pub(crate) lamp_view_proj: [[[f32; 4]; 4]; LAMP_SHADOWS],
    pub(crate) lamp_atlas: [f32; 4],
    /// Windy trees: xy the weather's wind (m/s, world; 0 with the setting off), zw how far
    /// the air has carried the gusts since the start (m, modulo the shaders' PATTERN_PERIOD).
    pub(crate) tree_wind: [f32; 4],
    /// The rear section of the player's articulated vehicle as `inside_*` (w of the third
    /// 0 without one): the weather stays out of it as well (#1967: it snowed and rained in
    /// the back of an articulated bus). Last, so that the shaders that do not read it can
    /// leave it out of their copy of this struct.
    pub(crate) inside2_a: [f32; 4],
    pub(crate) inside2_b: [f32; 4],
    pub(crate) inside2_c: [f32; 4],
    /// Enhanced: the field each light cookie layer covers (`cookies::CookieAtlas::fields`).
    /// After the rest: the shaders that light nothing by a cookie leave it out.
    pub(crate) cookies: [[f32; 4]; COOKIE_LAYERS],
    /// Enhanced: the lights of the player's saloon that have a shadow map, by their records in
    /// the light buffer (-1: none), see `cabin_visibility` in enhanced/02_lamps.wgsl.
    pub(crate) cabin_lamps: [[f32; 4]; 4],
    /// x: the textures' mip bias - negative (log2 of the scale) while FSR 1 scales the picture
    /// up, as AMD advises for it: its textures as detailed as the window's size wants, not
    /// the smaller picture's (letters, gauges and signs came out soft); 0 otherwise.
    pub(crate) lod: [f32; 4],
    /// The classic picture's headlight, Omsi.exe's one Direct3D spot light (the player's
    /// `[spotlight]`, see the scene shader's `classic_spot`): [0] xyz where its light leaves
    /// from (render-origin relative), w its declared range (0: none); [1] xyz its direction,
    /// w the cosine of half its outer cone; [2] rgb its colour, w the cosine of half its inner
    /// cone; [3] x how far behind [0] along the vehicle's axis the spotlight itself lies.
    /// (Every shader's copy of this struct carries it, read or not - see the test
    /// `shaders_validate_and_match_the_uniforms`.)
    pub(crate) spot: [[f32; 4]; 4],
}

/// The period the sky's cloud patterns repeat with (m): 5 x the cloud field (14 km), 8 x
/// its billow detail (8.75 km) and 28 x the high layer's (2.5 km) - the render origin is
/// taken modulo this for them, which keeps centimetres of precision in 32 bits.
pub(crate) const CLOUD_ORIGIN_PERIOD: f64 = 70000.0;

/// The enhanced path's post passes (post.wgsl `PostParams`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct PostUniform {
    pub(crate) a: [f32; 4],
    pub(crate) b: [f32; 4],
    pub(crate) c: [f32; 4],
    pub(crate) d: [f32; 4],
    /// x how much of a metered shortfall (a picture darker than the light model expects: a
    /// tunnel, a dark cab) the eye makes up, the rest as post.wgsl's `c.x` does for a surplus
    pub(crate) e: [f32; 4],
}

/// The enhanced lighting (enhanced_common.wgsl `Enhanced`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct EnhancedUniform {
    pub(crate) exposure: [f32; 4],
    pub(crate) sun: [f32; 4],
    pub(crate) sh: [[f32; 4]; 9],
    pub(crate) ground: [f32; 4],
    pub(crate) fog: [f32; 4],
    pub(crate) fog_color: [f32; 4],
    pub(crate) weather: [f32; 4],
    pub(crate) lights: [f32; 4],
    pub(crate) sun_disc: [f32; 4],
    pub(crate) debug: [f32; 4],
    /// xyz where the sky cube was drawn from, relative to the camera (the dome looks the
    /// clouds up through it with the parallax taken out); w the clouds' march steps
    /// (`Lighting::low_clouds`)
    pub(crate) eye: [f32; 4],
    /// x how bright an LED panel's dots burn (`Lighting::led_glow`), y how much of the
    /// mip chain an LED panel is held at (`Lighting::led_mips`)
    pub(crate) led: [f32; 4],
    /// xyz towards the moon, w its angular radius
    pub(crate) moon: [f32; 4],
    /// rgb the moon disc's irradiance at the ground before the clouds, w how much of the
    /// starry sky shows (the city's lamps and the haze wash the faint stars out)
    pub(crate) moon_disc: [f32; 4],
    /// rgb the sun's irradiance at the clouds' heights (`atmosphere::CLOUD_SUN_HEIGHTS`)
    pub(crate) cloud_sun: [[f32; 4]; 4],
    /// rgb the moonlight on a surface facing the moon (after the clouds), w 1 while the
    /// shadow maps are the moon's (`Lighting::casts_moon_shadows`)
    pub(crate) moon_light: [f32; 4],
    /// `Lighting::condensation`
    pub(crate) condensation: [f32; 4],
    /// x the cumulus layer's base, y its top (m over the ground), z the altocumulus cover,
    /// w the storm's activity
    pub(crate) layers: [f32; 4],
    /// The lightning of this frame: x the light it throws on a surface facing it (1 = 10 000
    /// lux), yz towards it (horizontal unit vector), w its elevation (rad)
    pub(crate) flash: [f32; 4],
    /// xyz towards the flash (unit vector), w how bright its channel is
    pub(crate) flash2: [f32; 4],
    /// x a number the channel's shape is drawn from
    pub(crate) flash3: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct FogLampUniform {
    pub(crate) inv_view_proj: [[f32; 4]; 4],
    pub(crate) size: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SsaoUniform {
    pub(crate) inv_proj: [[f32; 4]; 4],
    pub(crate) params: [f32; 4],
    /// xy: the projection's off-centre shift (0 for the window's own symmetric frustum;
    /// a headset eye or a triple screen's side panel has one)
    pub(crate) shift: [f32; 4],
    /// x the wide occlusion's reach (m), y its strength (0 = off), z the bounced light's
    /// gain (0 = off), w the largest radiance a bounce is taken from
    pub(crate) wide: [f32; 4],
    /// From a view-space point to the last frame's clip space (the bounced light's history),
    /// and in x whether that picture can be read (see `bounce_history`).
    pub(crate) reproj: [[f32; 4]; 4],
    pub(crate) history: [f32; 4],
}
