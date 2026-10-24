//! What a picture is drawn from: the camera, and the scene's lighting and weather.

use crate::*;

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    /// World position (f64: maps span millions of metres).
    pub position: DVec3,
    /// Degrees, 0 = looking north (+y), clockwise positive (OMSI heading).
    pub yaw: f32,
    /// Degrees, positive = looking up.
    pub pitch: f32,
    /// Degrees about the view direction (0 = the horizon level; see [`Camera::up`]). A
    /// camera fixed to a vehicle - a mirror's - leans with its body.
    pub roll: f32,
    pub fov_deg: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.to_radians().sin_cos();
        let (sp, cp) = self.pitch.to_radians().sin_cos();
        Vec3::new(sy * cp, cy * cp, sp)
    }
    pub fn right(&self) -> Vec3 {
        let f = self.forward();
        let r0 = Vec3::new(f.y, -f.x, 0.0).normalize_or_zero();
        if self.roll == 0.0 {
            return r0;
        }
        f.cross(self.up()).normalize_or(r0)
    }
    /// The picture's up: world up for a level camera, turned about the view direction by
    /// `roll` (positive: the top leans to the right).
    pub fn up(&self) -> Vec3 {
        let f = self.forward();
        let r0 = Vec3::new(f.y, -f.x, 0.0).normalize_or_zero();
        if self.roll == 0.0 || r0 == Vec3::ZERO {
            return Vec3::Z;
        }
        let u0 = r0.cross(f);
        let (s, c) = self.roll.to_radians().sin_cos();
        (u0 * c + r0 * s).normalize_or(Vec3::Z)
    }
    /// View-projection relative to a render origin (the camera itself when `origin` is its
    /// position), so that GPU maths stays in small numbers.
    pub fn view_proj(&self, aspect: f32, origin: DVec3) -> Mat4 {
        let view = Mat4::look_to_rh((self.position - origin).as_vec3(), self.forward(), self.up());
        // Reversed Z (near and far swapped): the depth buffer then spends its float
        // precision where the scene is far away instead of where it is close, which is what
        // stops distant roads, kerbs and painted ground from flickering against each other
        // - with a plain 0..1 depth the resolution at a kilometre is a good quarter of a
        // metre, less than the gap between a road surface and the ground under it.
        let proj = Mat4::perspective_rh(self.fov_deg.to_radians(), aspect, self.far, self.near);
        proj * view
    }

    /// Ray through a normalized device coordinate, relative to `origin`.
    pub fn ray(&self, ndc_x: f32, ndc_y: f32, aspect: f32, origin: DVec3) -> (Vec3, Vec3) {
        let inv = self.view_proj(aspect, origin).inverse();
        // ndc z = 0 is the far plane with reversed Z: the longest baseline for the ray
        let p = inv.project_point3(Vec3::new(ndc_x, ndc_y, 0.0));
        let o = (self.position - origin).as_vec3();
        (o, (p - o).normalize_or_zero())
    }
}

#[derive(Clone, Debug)]
pub struct Lighting {
    /// Objects smaller on the screen than this are not drawn: the original's
    /// `performance_minObjSize`, in its own measure (the object's diameter over its distance,
    /// as a share of the vertical field of view; see `render_inner`), times the object's
    /// `[detail_factor]`. OMSI's presets say 0.013 (0.020 for the slowest machines). The
    /// renderer's `RenderOptions::min_obj_size` is the floor; a picture may ask for more
    /// (the mirrors take the original's `performance_minObjSizeRefl`).
    pub min_obj_size: f32,
    pub sun_dir: Vec3,
    pub sun_intensity: f32,
    /// Direct sun colour (envir light A).
    pub sun_color: Vec3,
    /// Light from above (envir light B).
    pub secondary: Vec3,
    /// Undirected light (envir light C).
    pub ambient: Vec3,
    pub fog_color: Vec3,
    pub fog_density: f32,
    pub sky_color: Vec3,
    /// 0 at day, 1 at night: strength of nightmaps and coronas.
    pub night: f32,
    /// The night maps (`[matl_nightmap]`, the tiles' light maps) switched as Omsi.exe
    /// switches them - on with the lamps, not faded in with the dusk (its stage is set when
    /// the object's `NightlightA` is over 0.5, 0x61197a/0x7fee02); None: by `night`.
    pub night_maps: Option<f32>,
    /// Sun azimuth (radians, clockwise from north) and day/twilight/night sky texture weights.
    pub sun_azimuth: f32,
    pub sky_weights: [f32; 3],
    /// Cloud layer: density 0..1 and the texture offset (wind drift), 0 = no clouds.
    pub cloud_density: f32,
    pub cloud_offset: [f32; 2],
    /// The weather of the classic sky (Vanilla, Vanilla+): its haze and its cloud layer, as
    /// Omsi.exe draws them (sky.wgsl `fs_main`).
    pub vanilla_sky: VanillaSky,
    /// Sun shadow map (off in mirrors and at night).
    pub shadows: bool,
    /// How wet the roads are (0..1): rain darkens them and makes them mirror the sky.
    pub wetness: f32,
    /// Snow cover on the ground and the roads (0..1).
    pub snow: f32,
    /// The roads are kept clear of the snow (the weather's "snow on road" off): no cover is
    /// laid on road surfaces.
    pub roads_clear: bool,
    /// Enhanced graphics: the physically based high-range renderer (enhanced.wgsl) with its
    /// computed sky, automatic exposure, glow and tone mapping (post.wgsl).
    pub enhanced: bool,
    /// Vanilla graphics - the picture as OMSI 2 draws it: none of the extras the
    /// rewrite's own vanilla renderer (Vanilla+) adds (snow laid on the surfaces, rain drops
    /// running down the panes). Shadows, ambient occlusion and the detail grain are switched off by the settings.
    pub classic: bool,
    /// The player's vehicle (origin, heading in degrees, `[boundingbox]` w l h cx cy cz):
    /// no rain sheen or snow cover is shaded inside it.
    pub inside: Option<(DVec3, f64, [f32; 6])>,
    /// The player's vehicle's body rotation (pitch and roll as well as heading): the
    /// saloon's lamps look down from the vehicle's own frame (`LampShadow::cabin`). Without
    /// it the heading alone stands in.
    pub inside_rotation: Option<Mat4>,
    /// Actual road height beneath the player's vehicle; independent of suspension motion.
    /// The local puddle capture is skipped when no road height is known.
    pub puddle_ground: Option<f64>,
    /// Upward normal of that actual road face (including road grade and camber).
    pub puddle_normal: Vec3,
    /// Coupled parts of the player's vehicle (same layout as `inside`). Their own
    /// origins keep shared AI meshes out of the local puddle capture.
    pub puddle_parts: Vec<(DVec3, f64, [f32; 6])>,
    /// Procedural (fractal) detail texturing of the ground and roads up close - the
    /// `detail_textures` setting; independent of `enhanced`.
    pub detail: bool,
    /// Enhanced path: how closed the cloud cover is (0..1; `sun_intensity` already says
    /// how much sun comes through), how hard it rains (0..1), the height the weather's fog
    /// lies on (the ground under the player; `None` = just under the camera), and
    /// envir.cfg's light colours relative to the stock ones (A sun, B sky, C ambient).
    pub overcast: f32,
    pub rain: f32,
    /// Enhanced path: the street lamps cast shadows (the shadows setting; unlike the sun's,
    /// whatever the weather).
    pub lamp_shadows: bool,
    /// How hard it snows (0..1): the snowfall's flakes (snow.wgsl, every path), and in the
    /// enhanced picture the view they take (`enhanced_weather_fog`) - falling snow takes far
    /// more of it than rain of the same water does.
    pub snowfall: f32,
    /// The weather's wind (m/s, world): the snowfall drifts with it.
    pub wind: Vec3,
    /// The condensation on the player's bus's panes as optical depths (the share of the
    /// light they scatter is 1 - e^-depth): windscreen, side windows, rear window, and how
    /// far the defroster has cleared the windscreen (0..1; enhanced path).
    pub condensation: [f32; 4],
    pub fog_base: Option<f64>,
    pub envir_tint: [Vec3; 3],
    /// How bright an LED panel's dots burn (`MaterialExtra::led`; the settings' 16 levels
    /// give 0 = off .. 3.75): the enhanced picture draws them this much above their own
    /// colour, bright enough for the glow to bloom a halo around the panel.
    pub led_glow: f32,
    /// How much of the mip chain an LED panel is held at - the `\S:n` mask's (`STFilter`)
    /// and the panel's own grid picture's: both are sampled at the level their screen
    /// footprint asks for, never coarser than this. 0 point-samples them (the sharpest
    /// dots, and the worst shimmer - a regular grid is the worst case for a point sample);
    /// 1.3 (the default) keeps a matrix's dots a couple of pixels across where the full
    /// chain has run them together, and what shimmer is left is a fraction of a
    /// full-resolution sample's; 4 is near the calm of the full chain.
    pub led_mips: f32,
    /// Enhanced: the volumetric clouds marched in `CLOUD_STEPS_LOW` steps instead of
    /// `CLOUD_STEPS` (the settings' cloud quality "low"): the same clouds, sampled more
    /// coarsely - each redraw of the sky cube starts its steps elsewhere and is blended into
    /// what is there, so the coarser grain mostly averages out.
    pub low_clouds: bool,
    /// How much brighter the night is shown, in exposure steps after sunset (the settings' 0 .. 3).
    pub night_brightness: f32,
    pub reflection_strength: f32,
    /// The wet roads' reflections (`puddles`): 0 none, 1 low (a quarter of the rays, within
    /// 40 m, no capture of the player's vehicle), 2 high (as they were). Read every frame.
    pub puddles: u8,
    /// The player's vehicle's velocity (m/s, world): at speed the airstream drives the drops
    /// on its glass up the windscreen and back along the side windows.
    pub glass_wind: Vec3,
    /// Windy trees (the `windy_trees` setting): the foliage of trees bends and sways in
    /// `wind` (see shader.wgsl `tree_sway`).
    pub windy_trees: bool,
    /// Towards the moon (world space) and how much of its disc is lit (0 new .. 1 full):
    /// the enhanced night's moonlight and the moon in its sky.
    pub moon_dir: Vec3,
    pub moon_illum: f32,
    /// The day of the year (1..366) and the latitude (degrees north): the season's air.
    pub day_of_year: f32,
    pub latitude: f32,
    /// One number per calendar day, which the enhanced sky draws the day's own air from.
    pub day_seed: u32,
    /// The optical depth of a high ice-cloud veil (0 none .. 2 a thick milky one).
    pub veil: f32,
    /// The air as a weather model knows it: aerosol amount relative to a clear day, its
    /// Ångström exponent and its layer's depth (m); without it the day's own air is drawn
    /// from the calendar (`day_air`).
    pub air: Option<[f32; 3]>,
    /// The cumulus layer's base and top over the ground (m) as a weather model knows them
    /// (the condensation level and where the convection stops); without it the stock layer
    /// of 1400-2800 m.
    pub cloud_layer: Option<[f32; 2]>,
    /// Altocumulus cover of the layer at 2-6 km (0..1).
    pub alto: f32,
    /// The optical depth of the grey deck's liquid water as a weather model knows it;
    /// without it a stratus' 15 and, in rain, up to a nimbostratus' 60.
    pub deck_tau: Option<f32>,
    /// Thunderstorm activity (0 none .. 1 violent): lightning flashes and the storm's cloud.
    pub storm: f32,
    /// Simulation seconds for scene animation; standalone views use elapsed real time.
    pub animation_time: Option<f32>,
}

impl Lighting {
    /// Whether the sun shadow map is drawn with this light (not once the sun is about a
    /// degree below the horizon - Omsi.exe's cutoff, sun z -0.02 in sub_754c80 - nor with
    /// the sun dim, nor with OMSI_NO_SHADOWS).
    /// By night the moon casts the shadows the sun casts by day (the enhanced path): the
    /// sun well down, the moon up and more than a quarter lit - a full moon's 0.3 lux leave
    /// sharp shadows on a road beyond the lamps.
    pub fn casts_moon_shadows(&self) -> bool {
        self.enhanced
            && self.lamp_shadows
            && self.sun_dir.normalize_or_zero().z < -0.1
            && self.moon_dir.normalize_or_zero().z > 0.1
            && self.moon_illum > 0.25
            && !omsi_cfg::flags::OMSI_NO_SHADOWS.is_set()
    }

    pub fn casts_sun_shadows(&self) -> bool {
        self.shadows
            && self.sun_dir.normalize_or_zero().z > -0.02
            && self.sun_intensity > 0.05
            && !omsi_cfg::flags::OMSI_NO_SHADOWS.is_set()
    }
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            min_obj_size: 0.0,
            sun_dir: Vec3::new(0.3, 0.2, 0.9).normalize(),
            sun_intensity: 0.9,
            sun_color: Vec3::ONE,
            secondary: Vec3::splat(0.15),
            ambient: Vec3::splat(0.25),
            fog_color: Vec3::new(0.70, 0.78, 0.90),
            fog_density: 0.0006,
            sky_color: Vec3::new(0.55, 0.70, 0.92),
            night: 0.0,
            night_maps: None,
            sun_azimuth: 0.0,
            sky_weights: [1.0, 0.0, 0.0],
            cloud_density: 0.0,
            cloud_offset: [0.0; 2],
            vanilla_sky: VanillaSky::default(),
            shadows: true,
            snowfall: 0.0,
            wind: Vec3::ZERO,
            condensation: [0.0; 4],
            lamp_shadows: false,
            wetness: 0.0,
            snow: 0.0,
            roads_clear: false,
            enhanced: false,
            classic: false,
            inside: None,
            inside_rotation: None,
            puddle_ground: None,
            puddle_normal: Vec3::Z,
            puddle_parts: Vec::new(),
            detail: true,
            overcast: 0.0,
            rain: 0.0,
            fog_base: None,
            envir_tint: [Vec3::ONE; 3],
            night_brightness: 0.0,
            reflection_strength: 1.0,
            puddles: 2,
            led_glow: 1.5,
            led_mips: 1.3,
            low_clouds: false,
            glass_wind: Vec3::ZERO,
            windy_trees: false,
            moon_dir: Vec3::new(0.0, -0.5, -0.866),
            moon_illum: 0.0,
            day_of_year: 150.0,
            latitude: 52.5,
            day_seed: 0,
            veil: 0.0,
            air: None,
            cloud_layer: None,
            alto: 0.0,
            deck_tau: None,
            storm: 0.0,
            animation_time: None,
        }
    }
}
