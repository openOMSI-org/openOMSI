//! Point lights, coronas and smoke: what the frame's lights are, their grid on the GPU,
//! the lamps that cast shadows, and the light map tiles.

use crate::*;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuPointLight {
    pub(crate) pos: [f32; 4],
    pub(crate) color: [f32; 4],
    /// Spot direction and cosine of the outer cone (-2 = a point light).
    pub(crate) dir: [f32; 4],
    /// Cosine of the inner cone, the core radius, a headlamp's beam (1 low, -1 full), the
    /// radius (`pos.w` is 0 on a light only the enhanced path draws, which the vanilla
    /// shader then passes by).
    pub(crate) extra: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuCorona {
    pub(crate) pos: [f32; 3],
    pub(crate) size: f32,
    pub(crate) color: [f32; 4],
    pub(crate) dir: [f32; 4],
    /// xyz: the up axis; w: rotating mode
    pub(crate) up: [f32; 4],
    /// x: inner cone cosine, y: z offset, z: flags
    pub(crate) extra: [f32; 4],
}

/// A point light in world space (`[maplight]`, `[interiorlight]`, headlights).
#[derive(Debug, Clone, Copy)]
pub struct PointLight {
    pub position: DVec3,
    pub radius: f32,
    pub color: [f32; 3],
    pub intensity: f32,
    /// A spot light's direction (zero = a point light) and the cosines of its inner and
    /// outer cone (enhanced path).
    pub direction: Vec3,
    pub cone: [f32; 2],
    /// The radius within which the light is at full strength (`[maplight]`'s); 0 = an
    /// eighth of `radius` (enhanced path; vanilla always takes the eighth).
    pub core: f32,
    /// A headlamp (enhanced path), lit by a road lamp's profile instead of the cone: 1 a low
    /// beam, with its cut-off at the lamp's horizon, -1 a full beam, without; 0 any other light.
    pub beam: f32,
    /// A lamp in a housing - a street lamp's head, a platform's light (`[maplight]`): the
    /// enhanced path sends its light down and out, a few per cent above its horizon.
    pub housed: bool,
    /// A virtual source embedded in a pole fixture. Its fixture cannot occlude
    /// its own output; it remains a caster for other lamps and the sun.
    pub shadow_owner: Option<i64>,
    /// Which path draws the light.
    pub mode: LightMode,
    /// The enhanced path lights it by a picture of its beam (`[spotlight_cookie]`, see
    /// `cookies`) within its cone, when a layer is free for the picture that frame.
    pub cookie: Option<Cookie>,
    pub flash: bool,
}

impl Default for PointLight {
    fn default() -> Self {
        Self {
            position: DVec3::ZERO,
            radius: 0.0,
            color: [1.0; 3],
            intensity: 1.0,
            direction: Vec3::ZERO,
            cone: [1.0, 0.0],
            core: 0.0,
            beam: 0.0,
            housed: false,
            shadow_owner: None,
            mode: LightMode::Both,
            cookie: None,
            flash: false,
        }
    }
}

/// Which renderer a light belongs to: a vehicle's headlight is three point lights along
/// its axis for the vanilla path and one real spot light for the enhanced one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LightMode {
    #[default]
    Both,
    Vanilla,
    Enhanced,
}

/// One particle of a `[smoke]` system (exhaust, boiling coolant, wheel spray, chimneys).
///
/// Omsi.exe draws its puffs (0x5a4180: fog and lighting off, blended over the scene, depth
/// tested but not written) as squares facing the screen, each turned by its own angle and
/// moved a tenth of a metre towards the eye in depth (0x5a2b5c), and lets them sink on
/// through the road, which cuts every one off in a straight line where it goes in. Under a
/// wheel's spray - dozens of fresh puffs a second, falling in - that is a stack of bright
/// bands across the road. Here a puff that knows its `ground` fades out over its lowest
/// part into it instead (see `vs_main` in corona.wgsl).
#[derive(Debug, Clone, Copy, Default)]
pub struct SmokeParticle {
    pub position: DVec3,
    /// Half its width (m).
    pub size: f32,
    pub color: [f32; 3],
    pub alpha: f32,
    /// The angle its picture is turned by about the line of sight (radians).
    pub spin: f32,
    /// How far its depth is moved towards the eye (m), its place and size on the screen
    /// kept: Omsi.exe's 0.1 for every `[smoke]` particle (0x5a1d54 sets it, 0x5a2b5c
    /// subtracts it from the view depth it projects the depth at).
    pub z_offset: f32,
    /// World z of the ground under it (the road its vehicle stands on): it fades out
    /// towards it, and one wholly under it is left out. `None`: drawn as it is.
    pub ground: Option<f64>,
}

/// How high over its ground a smoke puff of half width `size` fades out into it (m): a fifth
/// of its drawn radius, between 6 and 25 cm - enough that no line shows where the road
/// meets it, little enough that a wheel's spray, all but a few tens of centimetres of which
/// is under the road, still shows as the mist round the tyres Omsi.exe draws.
pub(crate) fn smoke_ground_fade(size: f32) -> f32 {
    (0.2 * 0.9 * size).clamp(0.06, 0.25)
}

/// The corona shader's sprite for a smoke particle (`extra.w` 3: the smoke branch of
/// `vs_main`): `dir` the cosine and sine of its spin, `up` its ground (relative to the
/// render origin `ro`, `up.y` 1 when it has one), its z offset and how high the fade into
/// the ground reaches. `None` for a puff too faint or small to draw, or wholly under its
/// ground.
pub(crate) fn smoke_sprite(p: &SmokeParticle, ro: DVec3) -> Option<GpuCorona> {
    if !(p.alpha > 0.002 && p.size > 0.0) {
        return None;
    }
    // (drawn at 0.9 of its size, see `vs_main`)
    if p.ground.is_some_and(|g| p.position.z + (p.size * 0.9) as f64 <= g) {
        return None;
    }
    let (sin, cos) = p.spin.sin_cos();
    let ground = p.ground.filter(|g| g.is_finite());
    Some(GpuCorona {
        pos: (p.position - ro).as_vec3().to_array(),
        size: p.size,
        color: [p.color[0], p.color[1], p.color[2], p.alpha.clamp(0.0, 1.0)],
        dir: [cos, sin, 0.0, 0.0],
        up: [ground.map(|g| (g - ro.z) as f32).unwrap_or(0.0), if ground.is_some() { 1.0 } else { 0.0 }, p.z_offset.max(0.0), smoke_ground_fade(p.size)],
        extra: [-2.0, 0.0, 0.0, 3.0],
    })
}

/// The light map atlas: tiles a side and pixels a tile.
pub const LM_ATLAS_TILES: u32 = 5;
pub const LM_TILE_PX: u32 = 256;

/// A light corona sprite (`[light_enh]`, `[light_enh_2]`).
#[derive(Debug, Clone, Copy)]
pub struct Corona {
    pub position: DVec3,
    pub size: f32,
    pub color: [f32; 3],
    /// 0..2 (a `[light_enh_2]` fading variable of 2 is double brightness).
    pub brightness: f32,
    /// Facing direction (zero = omnidirectional) and cosine of the visibility half-cone
    /// (the outer cone: where it begins to be seen).
    pub direction: Vec3,
    pub cone_cos: f32,
    /// Cosine of the inner cone (full brightness); below `cone_cos` = no inner cone.
    pub inner_cos: f32,
    /// `[light_enh_2]` rotating: 0 a flat sprite facing `direction`, 1 turned to the viewer
    /// about `up`, 2 turned to the viewer about every axis (a billboard).
    pub rotating: u8,
    pub up: Vec3,
    /// How far the spot is moved from its place towards the viewer (m), so that a lamp
    /// inside its housing still shows; negative = the old default (half its size, at most
    /// half a metre).
    pub z_offset: f32,
    /// `[light_enh_2]` parameter bits: 1 star, 2 no fog, 4 only effects.
    pub flags: u8,
    /// Its picture: 0 the standard glow, else one registered with
    /// [`Renderer::set_corona_texture`] (a light's own `bitmap`, the fog cone's picture).
    pub texture: u16,
    /// A light's cone in the fog rather than its glow (OMSI's `light_cone.bmp` fan, see
    /// corona.wgsl): `size` is the fan's radius, `cone_cos` and `inner_cos` hold the outer
    /// and inner half angles (radians), `beam_width` the fog's visibility (m).
    pub beam: bool,
    pub beam_width: f32,
    /// The halo round a light in fog: a billboard of `size`, pulled
    /// towards the viewer, seen from in front of the light; the angles and the visibility
    /// travel as for a cone.
    pub halo: bool,
}

impl Default for Corona {
    fn default() -> Self {
        Corona {
            position: DVec3::ZERO,
            size: 0.1,
            color: [1.0; 3],
            brightness: 0.0,
            direction: Vec3::ZERO,
            cone_cos: -1.0,
            inner_cos: -2.0,
            rotating: 2,
            up: Vec3::Z,
            z_offset: -1.0,
            flags: 0,
            texture: 0,
            beam: false,
            beam_width: 0.0,
            halo: false,
        }
    }
}

pub(crate) const LIGHT_CELL: f32 = 25.0;
/// Enhanced: how far from the camera a lamp's reach may end for it to be given a shadow map.
pub(crate) const LAMP_SHADOW_REACH: f32 = 45.0;
/// A lamp's shadow map looks straight down from its head over this field of view (a
/// street lamp's light leaves it downwards, see `PointLight::housed`).
pub(crate) const LAMP_SHADOW_FOV: f32 = 150.0;
/// The biggest side of a lamp's tile (texels).
pub(crate) const LAMP_TILE_MAX: u32 = 512;

/// How the lamps' tiles lie in the far shadow map's texture: the far cascade takes its top
/// `shadow_size` rows, and the tiles (`LAMP_SHADOWS` of them, `cols` to a row) the rows under
/// it. The shaders read the layout from the camera uniform (`lamp_atlas`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LampAtlas {
    pub(crate) shadow_size: u32,
    pub(crate) tile: u32,
    pub(crate) cols: u32,
    pub(crate) rows: u32,
}

impl LampAtlas {
    /// The layout for a far map `shadow_size` texels wide, in a texture of `max_dim` texels a
    /// side at most. The tiles are an eighth of the far map's width (so that eight lie in a row and
    /// the forty-four of them take six rows: the texture is 1.75 times as high as wide, not 7.5
    /// times, which 512-texel tiles made of it at 2048), at most `LAMP_TILE_MAX`; where the
    /// texture would be too high they are made smaller, down to 64 texels, and left out when
    /// even those do not fit. `lamps` false - the plain pictures, which do not look the lamps'
    /// shadows up, and a phone's pipelines - leaves them out too: Vanilla and Vanilla+ keep the
    /// far map's own 1.25 times its width.
    pub(crate) fn new(shadow_size: u32, lamps: bool, max_dim: u32) -> LampAtlas {
        let layout = |tile: u32| {
            let cols = (shadow_size / tile).max(1);
            LampAtlas { shadow_size, tile, cols, rows: (LAMP_SHADOWS as u32).div_ceil(cols) }
        };
        let none = LampAtlas { shadow_size, tile: 64, cols: 1, rows: 0 };
        if !lamps {
            return none;
        }
        let mut atlas = layout((shadow_size / 8).clamp(64, LAMP_TILE_MAX));
        while atlas.height() > max_dim && atlas.tile > 64 {
            atlas = layout(atlas.tile / 2);
        }
        if atlas.height() > max_dim {
            return none;
        }
        atlas
    }

    /// The texture's height (texels): the far map and the tiles under it.
    pub(crate) fn height(&self) -> u32 {
        self.shadow_size + self.rows * self.tile
    }

    /// The first texel (x, y) of tile `slot`.
    pub(crate) fn origin(&self, slot: usize) -> (u32, u32) {
        let (col, row) = (slot as u32 % self.cols, slot as u32 / self.cols);
        (col * self.tile, self.shadow_size + row * self.tile)
    }

    /// What the camera uniform's `lamp_atlas` holds: a tile's side, tiles to a row, the
    /// texture's height and how many tiles there are.
    pub(crate) fn uniform(&self) -> [f32; 4] {
        [self.tile as f32, self.cols as f32, self.height() as f32, if self.rows > 0 { LAMP_SHADOWS as f32 } else { 0.0 }]
    }
}

/// A light that casts shadows this frame, with the tile of its map.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LampShadow {
    /// Its tile (`LampAtlas::origin`).
    pub(crate) slot: usize,
    /// Where it is (render-origin relative) and its reach.
    pub(crate) position: Vec3,
    pub(crate) range: f32,
    pub(crate) owner: Option<i64>,
    /// A vehicle's headlamps' map (`passes::headlamp_shadow`): it looks this way along the
    /// road (level, unit) instead of down; and the player's vehicle, whose own parts cast
    /// nothing into the player's headlamps' map.
    pub(crate) look: Option<Vec3>,
    pub(crate) own: Option<passes::VehicleBox>,
    /// The lamps of another vehicle: the parts of a vehicle the lamps stand in cast
    /// nothing into the map.
    pub(crate) hosts: bool,
    /// How near its map's depth begins (m): a street lamp's head and a saloon lamp's fixture,
    /// which the light sits in, are no shade for it.
    pub(crate) near: f32,
    /// How wide its map looks down (degrees): a street lamp's light leaves its head over most of
    /// the sky below, a saloon lamp's reaches the floor, the seats and the walls under it.
    pub(crate) fov: f32,
    /// A lamp of the player's saloon: its map is drawn in the vehicle's own frame.
    pub(crate) cabin: Option<Cabin>,
    /// A saloon lamp's record in the light buffer (the first of those at its place).
    pub(crate) index: u32,
}

/// The player's vehicle as a saloon lamp's tile sees it. The lamp looks down from the
/// vehicle's frame, so that the tile holds the saloon as it is, whatever the bus does on the
/// road: the seats, the handrails and the people in them stay where they are in it, and the
/// tile is drawn again only when one of them moves.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Cabin {
    /// The vehicle's box (render-origin relative): what stands in it casts into the tile.
    pub(crate) vehicle: passes::VehicleBox,
    /// The world (render-origin relative) to the vehicle's frame.
    pub(crate) to_local: Mat4,
    /// The lamp in the vehicle's frame.
    pub(crate) lamp: Vec3,
}

/// How far a saloon lamp's map reaches (m).
pub(crate) const CABIN_REACH: f32 = 8.0;
/// How near the saloon lamps' maps begin (m): the lamp's fixture and the roof lining it hangs
/// in are within it.
pub(crate) const CABIN_NEAR: f32 = 0.3;
/// How wide a saloon lamp's map looks down (degrees): the floor, the seats and the walls under a
/// lamp 2 to 3.5 m up, in a tile that is a few hundred texels, three to ten centimetres each.
pub(crate) const CABIN_FOV: f32 = 120.0;

impl LampShadow {
    /// A light with no tile description yet but the slot: the rest to be filled in by the one
    /// that makes it (a world lamp looks straight down, 0.1 m to its reach).
    pub(crate) fn new(slot: usize) -> LampShadow {
        LampShadow { slot, position: Vec3::ZERO, range: 0.0, owner: None, look: None, own: None, hosts: false, near: 0.1, fov: LAMP_SHADOW_FOV, cabin: None, index: 0 }
    }

    /// A sphere (centre, radius) that holds the reach of all of `lamps`; `None` for none.
    pub(crate) fn union_sphere(lamps: &[LampShadow]) -> Option<(Vec3, f32)> {
        let first = lamps.first()?;
        let (lo, hi) = lamps.iter().fold((first.position, first.position), |(lo, hi), l| (lo.min(l.position), hi.max(l.position)));
        let centre = (lo + hi) * 0.5;
        let radius = lamps.iter().map(|l| (l.position - centre).length() + l.range).fold(0.0, f32::max);
        Some((centre, radius))
    }

    /// The shadow map's view and projection: straight down, depth 0..1 over its reach (a
    /// vehicle's headlamps' along the road, see `headlamp_view_proj`).
    pub(crate) fn view_proj(&self) -> Mat4 {
        if let Some(c) = &self.cabin {
            return self.key_matrix() * c.to_local;
        }
        if let Some(look) = self.look {
            return self.headlamp_view_proj(look);
        }
        self.key_matrix()
    }

    /// What the tile's content is judged by: the matrix itself, but for a saloon lamp's
    /// tile the one in the vehicle's frame, which does not change as the vehicle moves.
    pub(crate) fn key_matrix(&self) -> Mat4 {
        if let Some(c) = &self.cabin {
            let proj = Mat4::perspective_rh(self.fov.to_radians(), 1.0, self.near, self.range.max(1.0));
            return proj * Mat4::look_to_rh(c.lamp, -Vec3::Z, Vec3::Y);
        }
        if let Some(look) = self.look {
            return self.headlamp_view_proj(look);
        }
        let proj = Mat4::perspective_rh(self.fov.to_radians(), 1.0, self.near, self.range.max(1.0));
        let view = Mat4::look_to_rh(self.position, -Vec3::Z, Vec3::Y);
        proj * view
    }
}
pub(crate) const LIGHT_GRID_SIDE: usize = 64;
/// (32: a depot or a bus interior with many lamps lost the farthest past 16 in a cell)
pub(crate) const LIGHT_CELL_CAP: usize = 32;
/// The lights a cell holds for the classic picture: Omsi.exe lights a draw with at most the
/// eight Direct3D lights nearest it, and the classic shader working through a cell's 32
/// for every pixel of a city at night halved the frame rate on small graphics chips (#1467).
pub(crate) const VANILLA_CELL_CAP: usize = 8;

/// How strongly light `l` (at `p`, radius `r`) lights a point `d2` (squared metres) away -
/// the classic shader's falloff - by which a full grid cell keeps its strongest lights.
fn cell_strength(intensity: f32, color: [f32; 3], r: f32, d2: f32) -> f32 {
    let r0 = r * 0.125;
    let d = d2.sqrt();
    intensity * (color[0] + color[1] + color[2]) * (r0 * r0 / d2.max(0.01)).min(1.0) * (1.0 - d / r.max(0.01)).max(0.0)
}

/// Does this path draw the light? (See `Renderer::prepare_lights`.)
pub(crate) fn drawn_by(l: &PointLight, enhanced: bool) -> bool {
    l.radius > 0.0
        && l.intensity > 0.0
        && l.mode
            != if enhanced {
                LightMode::Vanilla
            } else {
                LightMode::Enhanced
            }
}

/// A light as the shaders read it, at `p` relative to the render origin.
pub(crate) fn gpu_light(l: &PointLight, p: Vec3) -> GpuPointLight {
    let spot = l.direction.length_squared() > 1e-6;
    let dir = if spot {
        l.direction.normalize().extend(l.cone[1]).to_array()
    } else if l.mode == LightMode::Vanilla {
        // (a vehicle's headlight stand-in: lights a light-mapped road too, see
        // shader.wgsl point_lights)
        [1.0, 0.0, 0.0, -2.0]
    } else if l.housed {
        // (z: a housed lamp, see `PointLight::housed`; the classic shader reads only x)
        [0.0, 0.0, -1.0, -2.0]
    } else {
        [0.0, 0.0, 0.0, -2.0]
    };
    let vanilla_radius = if l.mode == LightMode::Enhanced {
        0.0
    } else {
        l.radius
    };
    GpuPointLight {
        pos: [p.x, p.y, p.z, vanilla_radius],
        color: [l.color[0], l.color[1], l.color[2], l.intensity],
        dir,
        extra: [l.cone[0], l.core, l.beam, l.radius],
    }
}

impl Renderer {
    /// Upload this frame's point lights into the light grid around the camera. Returns the
    /// grid origin (render-origin relative). `enhanced`: the frame is drawn by the enhanced
    /// path, which takes a headlight as one spot light where the vanilla path takes three
    /// points (`LightMode`). Its mirrors (the vanilla shader) and its picture share the
    /// enhanced list, so that it is not sent twice a frame: the vanilla shader passes a spot
    /// by, whose radius it reads as 0, and the mirrors show no headlight pools. The three
    /// stand-in points stay out of it, or a street full of cars would fill the grid cells'
    /// sixteen places before the street lamps got theirs.
    pub(crate) fn prepare_lights(&self, scene: &mut Scene, eye: &LightEye, enhanced: bool, lamp_shadows: bool) -> ([f32; 4], Vec<LampShadow>) {
        // the lights that get a shadow map: the world's lamps lighting the camera's
        // surroundings most (by their strength over the distance), as many as there are
        // tiles, the headlamps of the vehicles in front of the camera and the lamps of the
        // player's saloon (see `lamp_tiles`)
        let mut wants = TileWants::default();
        let cam_rel = eye.cam_rel;
        let ro = scene.render_origin;
        let side = LIGHT_GRID_SIDE;
        let half = side as f32 * LIGHT_CELL * 0.5;
        // snapped to whole cells, so that the grid stays the same while nothing moves
        let origin = [
            ((cam_rel.x - half) / LIGHT_CELL).floor() * LIGHT_CELL,
            ((cam_rel.y - half) / LIGHT_CELL).floor() * LIGHT_CELL,
        ];
        let mut gpu_lights: Vec<GpuPointLight> =
            Vec::with_capacity(scene.interior_lights.len() + scene.lights.len().max(1));
        // the vehicles' interior lamps first, at the indices their meshes name (they are in
        // no grid cell: only those meshes look them up)
        for l in &scene.interior_lights {
            gpu_lights.push(gpu_light(l, (l.position - ro).as_vec3()));
        }
        // (the player's headlamps, which share a map of their own: see `passes::headlamp_shadow`)
        let own = eye.vehicle.filter(|_| lamp_shadows && enhanced).map(|(b, _)| passes::VehicleBox::new(b, ro));
        self.lights.cookies.begin_frame();
        // The grid is made in a room kept all unused but for the cells the last one filled,
        // which are cleared first: half a megabyte set and compared twice a frame (the
        // window's picture and a mirror's) for the few thousand cells lamps reach.
        let cells = side * side;
        let mut grid = std::mem::take(&mut scene.grid_scratch);
        if grid.len() != cells * LIGHT_CELL_CAP {
            grid.clear();
            grid.resize(cells * LIGHT_CELL_CAP, u32::MAX);
            scene.grid_filled.clear();
        }
        for &c in &scene.grid_filled {
            let base = c as usize * LIGHT_CELL_CAP;
            grid[base..base + LIGHT_CELL_CAP].fill(u32::MAX);
        }
        scene.grid_filled.clear();
        let mut counts = std::mem::take(&mut scene.grid_counts);
        counts.clear();
        counts.resize(cells, 0);
        let mut strength = std::mem::take(&mut scene.grid_strength);
        strength.resize(cells * LIGHT_CELL_CAP, 0.0);
        let cap = if enhanced { LIGHT_CELL_CAP } else { VANILLA_CELL_CAP };
        let squares = omsi_cfg::flags::OMSI_LIGHT_GRID_SQUARES.is_set();
        for l in &scene.lights {
            // Every light the grid reaches is in the list, the other path's too (a headlight
            // is three points for the plain shader and one spot for the enhanced): only the
            // grid tells which a path draws. The window's Enhanced picture and its plainly
            // shaded mirrors then send the same list, at the same places, and grids that
            // differ only round the vehicles' lamps - not everything again twice a frame.
            if !(l.radius > 0.0 && l.intensity > 0.0) {
                continue;
            }
            let p = (l.position - ro).as_vec3();
            let x0 = ((p.x - l.radius - origin[0]) / LIGHT_CELL).floor();
            let x1 = ((p.x + l.radius - origin[0]) / LIGHT_CELL).floor();
            let y0 = ((p.y - l.radius - origin[1]) / LIGHT_CELL).floor();
            let y1 = ((p.y + l.radius - origin[1]) / LIGHT_CELL).floor();
            if x1 < 0.0 || y1 < 0.0 || x0 >= side as f32 || y0 >= side as f32 {
                continue;
            }
            let idx = gpu_lights.len() as u32;
            gpu_lights.push(gpu_light(l, p));
            if !drawn_by(l, enhanced) {
                continue;
            }
            // The enhanced path's own marks in `pos.w`, which the classic shader reads as a
            // radius of none (these lights are the enhanced path's alone): 1 + its layer for
            // a lamp with a picture of its beam (without a free layer, its cone lights it),
            // and 16 more for one of the player's headlamps, which read their shared map
            // (see `passes::headlamp_shadow`, enhanced.wgsl `light_marks`).
            let mut marks = 0.0;
            if let Some(layer) = l.cookie.filter(|_| enhanced && l.mode == LightMode::Enhanced).and_then(|c| self.lights.cookies.layer(&c)) {
                marks += 1.0 + layer as f32;
            }
            let is_own = own.as_ref().is_some_and(|b| passes::own_headlamp(l, p, b));
            if is_own {
                marks += 16.0;
                wants.own_headlamp(idx, p, l);
            }
            // The lights that cast shadows (Enhanced): see `TileWants::consider`.
            if enhanced && l.intensity > 0.0 && l.mode != LightMode::Vanilla && !is_own {
                marks += wants.consider(&scene.lamp_slots, eye, l, p, idx, lamp_shadows);
            }
            if marks > 0.0 {
                if let Some(g) = gpu_lights.last_mut() {
                    g.pos[3] = -marks;
                }
            }
            for y in (y0.max(0.0) as usize)..=(y1.min(side as f32 - 1.0) as usize) {
                for x in (x0.max(0.0) as usize)..=(x1.min(side as f32 - 1.0) as usize) {
                    let cell = y * side + x;
                    let n = counts[cell] as usize;
                    // (its light at the cell's middle, or at its nearest point when the
                    // light stands in the cell)
                    let cx = (origin[0] + (x as f32 + 0.5) * LIGHT_CELL - p.x).abs().max(0.0) - LIGHT_CELL * 0.5;
                    let cy = (origin[1] + (y as f32 + 0.5) * LIGHT_CELL - p.y).abs().max(0.0) - LIGHT_CELL * 0.5;
                    let d2 = cx.max(0.0).powi(2) + cy.max(0.0).powi(2);
                    if d2 >= l.radius * l.radius && !squares {
                        continue;
                    }
                    // (a spot - a headlight - counts as strong: it is the player's or a car's
                    // own light, which Omsi.exe gives a light of its own; past half the cell,
                    // a junction full of traffic would leave its street lamps out)
                    let base = cell * LIGHT_CELL_CAP;
                    let spot = l.direction.length_squared() > 1e-6;
                    let s = if spot && strength[base..base + n].iter().filter(|&&x| x == f32::MAX).count() < cap / 2 { f32::MAX } else { cell_strength(l.intensity, l.color, l.radius, d2) };
                    if n < cap {
                        if n == 0 {
                            scene.grid_filled.push(cell as u32);
                        }
                        grid[base + n] = idx;
                        strength[base + n] = s;
                        counts[cell] += 1;
                    } else if let Some(weakest) = (0..cap).min_by(|&a, &b| strength[base + a].total_cmp(&strength[base + b])).filter(|&k| strength[base + k] < s) {
                        // a full cell keeps the lights that light it most, not the first ones
                        grid[base + weakest] = idx;
                        strength[base + weakest] = s;
                    } else if omsi_cfg::flags::OMSI_DEBUG_LIGHT_GRID.is_set() {
                        log::info!("light grid: cell ({x}, {y}) full, light at ({:.1}, {:.1}) radius {:.0} {} left out", p.x, p.y, l.radius, if l.direction.length_squared() > 0.5 { "spot" } else { "point" });
                    }
                }
            }
        }
        let tiles = if lamp_shadows && enhanced { wants.tiles(scene, eye, own, &mut gpu_lights) } else { Vec::new() };
        // (the beams' pictures the lamps found layers for, after the lights: see `cookies`)
        self.lights.cookies.append(&mut gpu_lights);
        scene.grid_counts = counts;
        scene.grid_strength = strength;
        self.send_lights(scene, gpu_lights, grid);
        ([origin[0], origin[1], LIGHT_CELL, side as f32], tiles)
    }

    /// Send the frame's lights and their grid to the GPU: only what differs from what the
    /// buffers hold is written (most lights and most cells stand still).
    fn send_lights(&self, scene: &mut Scene, mut gpu_lights: Vec<GpuPointLight>, grid: Vec<u32>) {
        if gpu_lights.is_empty() {
            gpu_lights.push(GpuPointLight {
                pos: [0.0; 4],
                color: [0.0; 4],
                dir: [0.0; 4],
                extra: [0.0; 4],
            });
        }
        let lbytes: &[u8] = bytemuck::cast_slice(&gpu_lights);
        let cells = LIGHT_GRID_SIDE * LIGHT_GRID_SIDE;
        let mut rebuilt = false;
        match &scene.light_buf {
            Some(b) if b.size() >= lbytes.len() as u64 => {
                // (only the lights from the first that changed to the last: most stand still)
                let unit = std::mem::size_of::<GpuPointLight>();
                let old = &scene.last_lights;
                let first = (0..lbytes.len() / unit).find(|k| old.get(k * unit..(k + 1) * unit) != Some(&lbytes[k * unit..(k + 1) * unit]));
                if let Some(first) = first {
                    let last = (first..lbytes.len() / unit).rev().find(|k| old.get(k * unit..(k + 1) * unit) != Some(&lbytes[k * unit..(k + 1) * unit])).unwrap_or(first);
                    b.write(&self.queue, (first * unit) as u64, &lbytes[first * unit..(last + 1) * unit]);
                }
            }
            _ => {
                let cap = (lbytes.len() * 2).max(64 * std::mem::size_of::<GpuPointLight>());
                let b = GpuArray::new(&self.device, "lights", cap as u64, 16, false);
                b.write(&self.queue, 0, lbytes);
                scene.light_buf = Some(b);
                rebuilt = true;
            }
        }
        if scene.grid_buf.is_none() || scene.last_grid.len() != grid.len() {
            let b = GpuArray::new(&self.device, "light grid", (grid.len() * 4) as u64, 4, false);
            b.write(&self.queue, 0, bytemuck::cast_slice(&grid));
            scene.grid_buf = Some(b);
            scene.last_grid.clone_from(&grid);
            scene.grid_sent.clone_from(&scene.grid_filled);
            rebuilt = true;
        } else if let Some(b) = &scene.grid_buf {
            // Only the cells that differ from what the buffer holds are sent: those either
            // grid has lamps in, compared, in runs of neighbouring cells.
            let mut marks = std::mem::take(&mut scene.grid_marks);
            marks.clear();
            marks.resize(cells, false);
            for &c in scene.grid_sent.iter().chain(&scene.grid_filled) {
                marks[c as usize] = true;
            }
            let mut runs: Vec<(usize, usize)> = Vec::new();
            for (c, m) in marks.iter().enumerate() {
                let r = c * LIGHT_CELL_CAP..(c + 1) * LIGHT_CELL_CAP;
                if !*m || grid[r.clone()] == scene.last_grid[r.clone()] {
                    continue;
                }
                scene.last_grid[r.clone()].copy_from_slice(&grid[r]);
                // (a gap of a few unchanged cells is sent along rather than starting another
                // write: each makes a staging buffer of its own)
                match runs.last_mut() {
                    Some(run) if c - run.1 <= 8 => run.1 = c + 1,
                    _ => runs.push((c, c + 1)),
                }
            }
            if runs.len() > 32 {
                runs = vec![(runs[0].0, runs[runs.len() - 1].1)];
            }
            for (a, z) in runs {
                let r = a * LIGHT_CELL_CAP..z * LIGHT_CELL_CAP;
                b.write(&self.queue, (r.start * 4) as u64, bytemuck::cast_slice(&scene.last_grid[r]));
            }
            scene.grid_marks = marks;
            scene.grid_sent.clone_from(&scene.grid_filled);
        }
        scene.last_lights.clear();
        scene.last_lights.extend_from_slice(lbytes);
        scene.grid_scratch = grid;
        if rebuilt {
            self.rebuild_camera_bind_group(scene);
        }
    }

    /// Upload this frame's smoke particles, farthest first (they are blended over each other).
    pub(crate) fn prepare_smoke(&self, scene: &mut Scene, eye: DVec3) {
        let ro = scene.render_origin;
        // (the order is sorted as keys with the sprite's place in the list, the sprites
        // gathered after: sorting the 80-byte sprites themselves moved them about many
        // times over, for the window's picture and again for each mirror; a stable sort of
        // the same keys gives the same order)
        let (mut sprites, mut order, mut data) = std::mem::take(&mut scene.smoke_scratch);
        sprites.clear();
        sprites.extend(
            scene
                .smoke
                .iter()
                .filter_map(|p| smoke_sprite(p, ro).map(|g| (-(p.position - eye).length_squared(), g))),
        );
        order.clear();
        order.extend(sprites.iter().enumerate().map(|(i, (d, _))| (*d, i as u32)));
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        data.clear();
        data.extend(order.iter().map(|&(_, i)| sprites[i as usize].1));
        scene.smoke_count = data.len() as u32;
        self.upload_smoke(scene, &data);
        scene.smoke_scratch = (sprites, order, data);
    }

    /// The sorted smoke sprites into the smoke buffer, grown when they do not fit.
    pub(crate) fn upload_smoke(&self, scene: &mut Scene, data: &[GpuCorona]) {
        if data.is_empty() {
            return;
        }
        let bytes: &[u8] = bytemuck::cast_slice(data);
        match &scene.smoke_buf {
            Some(b) if b.size() as usize >= bytes.len() => {
                // (compared with what the smoke buffer holds: compared with the coronas'
                // bytes, which it never equals, it was written again every frame)
                if scene.smoke_sent != bytes {
                    self.queue.write_buffer(b, 0, bytes);
                }
            }
            _ => {
                let b = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("smoke"),
                    size: (bytes.len() * 2) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.queue.write_buffer(&b, 0, bytes);
                scene.smoke_buf = Some(b);
            }
        }
        scene.smoke_sent.clear();
        scene.smoke_sent.extend_from_slice(bytes);
    }

    /// Put a tile's light map into its place of the atlas (`slot`: column from the west, row
    /// from the north, 0..5); a picture of another size is scaled to 256 px.
    pub fn set_light_map_tile(&self, slot: (u32, u32), img: Option<&omsi_texture::Image>) {
        let n = LM_TILE_PX as usize;
        let mut px = vec![0u8; n * n * 4];
        if let Some(img) = img.filter(|i| i.width > 0 && i.height > 0) {
            for y in 0..n {
                for x in 0..n {
                    let sx = x * img.width as usize / n;
                    let sy = y * img.height as usize / n;
                    let o = (sy * img.width as usize + sx) * 4;
                    px[(y * n + x) * 4..(y * n + x) * 4 + 4].copy_from_slice(&img.rgba[o..o + 4]);
                }
            }
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.lights.lm_atlas, mip_level: 0, origin: wgpu::Origin3d { x: slot.0 * LM_TILE_PX, y: slot.1 * LM_TILE_PX, z: 0 }, aspect: wgpu::TextureAspect::All },
            &px,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(LM_TILE_PX * 4), rows_per_image: None },
            wgpu::Extent3d { width: LM_TILE_PX, height: LM_TILE_PX, depth_or_array_layers: 1 },
        );
    }

    /// Where the light map atlas lies: its south-west corner (world x, y) and its side (m).
    pub fn set_light_map_place(&self, x: f64, y: f64, side: f64) {
        self.lights.lm_place.set((x, y, side));
    }

    /// Register picture `id` for coronas (`Corona::texture`): a light's own bitmap, whose
    /// brightness is the glow's shape.
    pub fn set_corona_texture(&mut self, id: u16, img: &omsi_texture::Image) {
        let t = upload_texture(&self.device, &self.queue, img, true);
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("corona picture"),
            layout: &self.lights.corona_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&t.view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.lights.corona_sampler) },
            ],
        });
        let i = id as usize;
        if self.lights.corona_textures.len() <= i {
            self.lights.corona_textures.resize_with(i + 1, || None);
        }
        self.lights.corona_textures[i] = Some(bg);
    }

    /// How many of its caller's numbered corona pictures and cookie pictures this renderer
    /// has been handed (as `set_corona_texture` / `set_cookie_picture`). The numbers are the
    /// same for every renderer of the process; a renderer of its own - a second session's,
    /// one made again after a lost device - starts at none and so is handed them all.
    pub fn pictures_taken(&self) -> (usize, usize) {
        self.lights.pictures_taken
    }

    /// See [`Renderer::pictures_taken`].
    pub fn set_pictures_taken(&mut self, taken: (usize, usize)) {
        self.lights.pictures_taken = taken;
    }

    /// A light cookie's picture (`PointLight::cookie`), under the number its lamps name.
    pub fn set_cookie_picture(&self, id: u16, img: &omsi_texture::Image) {
        self.lights.cookies.set_picture(id, img);
    }

    /// The picture of a smoke particle: the game's `Texture/rauch.tga` (OMSI's default
    /// for every `[smoke]`), alpha as its mask.
    pub fn set_smoke_texture(&mut self, img: &omsi_texture::Image) {
        let t = upload_texture(&self.device, &self.queue, img, true);
        self.lights.smoke_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("smoke"),
            layout: &self.lights.corona_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&t.view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.lights.corona_sampler) },
            ],
        });
    }

    /// Upload this frame's coronas.
    /// `cab`: the box of the vehicle the camera is in, whose own flares Omsi.exe draws
    /// after the vehicle (0x6f0a2c -> 0x6f07bc), the others before it (0x6f0400/0x6f0418).
    pub(crate) fn prepare_coronas(&self, scene: &mut Scene, night: f32, cab: Option<&(DVec3, f64, [f32; 6])>) {
        let ro = scene.render_origin;
        // (its lamps sit on the skin of its box: half a metre round it is the vehicle's)
        let cab = cab.map(|&(o, h, mut bb)| {
            for v in &mut bb[..3] {
                *v += 1.0;
            }
            (o, h, bb)
        });
        let late = |c: &Corona| cab.as_ref().is_some_and(|b| point_in_vehicle_box(c.position, b));
        let mut order: Vec<(bool, &Corona)> = scene.coronas.iter().filter(|c| c.brightness > 0.001).map(|c| (late(c), c)).collect();
        order.sort_by_key(|(l, c)| (*l, c.texture));
        let mut runs: Vec<(u16, u32, u32, bool)> = Vec::new();
        for (k, &(l, c)) in order.iter().enumerate() {
            match runs.last_mut() {
                Some(r) if r.0 == c.texture && r.3 == l => r.2 += 1,
                _ => runs.push((c.texture, k as u32, 1, l)),
            }
        }
        let order: Vec<&Corona> = order.into_iter().map(|(_, c)| c).collect();
        if omsi_cfg::flags::OMSI_DEBUG_CONES.is_set() {
            log::info!("coronas: {} in {} runs {:?}, {} beams", order.len(), runs.len(), runs, order.iter().filter(|c| c.beam).count());
        }
        scene.corona_runs = runs;
        let data: Vec<GpuCorona> = order
            .into_iter()
            .map(|c| {
                let p = (c.position - ro).as_vec3();
                // (a raindrop's streak keeps its own level; a snowflake, -3, is scaled as a
                // light's corona is, as it always was in the classic picture)
                let b = if (c.cone_cos < -1.5 && c.cone_cos > -2.5) || c.beam || c.halo {
                    c.brightness
                } else {
                    // the original: ((1 - ambient)^2 + 0.8) 0.6 times the light's
                    // brightness (the shader takes the viewing angle and clamps it to 1)
                    c.brightness * (night * night + 0.8) * 0.6
                };
                GpuCorona {
                    pos: p.to_array(),
                    size: c.size,
                    color: [c.color[0], c.color[1], c.color[2], b],
                    dir: [c.direction.x, c.direction.y, c.direction.z, c.cone_cos],
                    up: [c.up.x, c.up.y, c.up.z, c.rotating as f32],
                    extra: [c.inner_cos, if c.beam || c.halo { c.beam_width } else { c.z_offset }, c.flags as f32, if c.beam { 1.0 } else if c.halo { 2.0 } else { 0.0 }],
                }
            })
            .collect();
        scene.corona_count = data.len() as u32;
        if data.is_empty() {
            return;
        }
        let bytes: &[u8] = bytemuck::cast_slice(&data);
        match &scene.corona_buf {
            Some(b) if b.size() as usize >= bytes.len() => {
                if scene.corona_sent != bytes {
                    self.queue.write_buffer(b, 0, bytes);
                }
            }
            _ => {
                let b = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("coronas"),
                    size: (bytes.len() * 2) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.queue.write_buffer(&b, 0, bytes);
                scene.corona_buf = Some(b);
            }
        }
        scene.corona_sent.clear();
        scene.corona_sent.extend_from_slice(bytes);
    }
}

/// The lights' resources beside the scene's grid: coronas, smoke, snowfall, cookies, the tile light maps and the lamps in the fog.
pub(crate) struct LightRes {
    pub(crate) corona_bind_group: wgpu::BindGroup,
    /// The snowfall's parameters (snow.wgsl `SnowParams`) and their bind group.
    pub(crate) snow_buf: wgpu::Buffer,
    pub(crate) snow_bind_group: wgpu::BindGroup,
    /// The rainfall's (rain.wgsl `RainParams`).
    pub(crate) rain_buf: wgpu::Buffer,
    pub(crate) rain_bind_group: wgpu::BindGroup,
    /// The smoke texture (`Texture/rauch.tga`, see [`Renderer::set_smoke_texture`]).
    pub(crate) smoke_bind_group: wgpu::BindGroup,
    /// The coronas' pictures besides the standard glow (index = `Corona::texture`; entry 0
    /// unused).
    pub(crate) corona_textures: Vec<Option<wgpu::BindGroup>>,
    /// How far this renderer has come through the numbered corona and cookie pictures its
    /// caller keeps (see [`Renderer::pictures_taken`]).
    pub(crate) pictures_taken: (usize, usize),
    /// The light cookies' pictures and the frame's layers of them.
    pub(crate) cookies: cookies::CookieAtlas,
    pub(crate) corona_layout: wgpu::BindGroupLayout,
    pub(crate) corona_sampler: wgpu::Sampler,
    /// Enhanced: the lamps' light the fog scatters, added over the drawn picture (none on
    /// OpenGL and without storage buffers), its bind group layout and parameters.
    pub(crate) fog_lamps_pipeline: Option<[wgpu::RenderPipeline; 2]>,
    pub(crate) fog_lamps_layout: wgpu::BindGroupLayout,
    pub(crate) fog_lamps_buf: wgpu::Buffer,
    /// The tile light maps (`.map.LM.bmp`) of the 5x5 tiles around the camera, north up, and
    /// where that square lies (world x, y of its south-west corner, its side in metres).
    pub(crate) lm_atlas: wgpu::Texture,
    pub(crate) lm_atlas_view: wgpu::TextureView,
    pub(crate) lm_uniform: wgpu::Buffer,
    pub(crate) lm_place: std::cell::Cell<(f64, f64, f64)>,
}

#[cfg(test)]
mod atlas_tests {
    use super::*;

    /// The tiles are an eighth of the far map's width, forty-four of them in six rows, and take
    /// no more of the texture than 1.75 times the far map's width in all.
    #[test]
    fn the_lamp_tiles_take_six_rows_of_an_eighth() {
        for (size, tile) in [(1024, 128), (2048, 256), (4096, 512), (8192, 512)] {
            let a = LampAtlas::new(size, true, 16384);
            assert_eq!((a.tile, a.cols), (tile, size / tile), "{size}");
            assert_eq!(a.rows, if size == 8192 { 3 } else { 6 }, "{size}");
            let last = a.origin(LAMP_SHADOWS - 1);
            assert!(last.0 + a.tile <= size && last.1 + a.tile <= a.height(), "{size}: the last tile lies in the texture");
        }
        let a = LampAtlas::new(2048, true, 16384);
        assert_eq!(a.height(), 2048 + 6 * 256);
        assert_eq!(a.uniform(), [256.0, 8.0, 3584.0, LAMP_SHADOWS as f32]);
    }

    /// Vanilla and Vanilla+ have no tiles (the far map keeps its own height), a texture too low
    /// for them gets smaller tiles, and one too low for the smallest none.
    #[test]
    fn the_atlas_fits_the_texture_or_leaves_the_tiles_out() {
        let plain = LampAtlas::new(2048, false, 16384);
        assert_eq!((plain.rows, plain.height(), plain.uniform()[3]), (0, 2048, 0.0));
        let low = LampAtlas::new(2048, true, 2600);
        assert!(low.rows > 0 && low.height() <= 2600 && low.tile < 256, "{low:?}");
        let none = LampAtlas::new(2048, true, 2048);
        assert_eq!((none.rows, none.uniform()[3]), (0, 0.0));
        // a device that reads 4096 texels at most: the far map is 2048 wide there (see `open`)
        assert!(LampAtlas::new(2048, true, 4096).height() <= 4096);
    }
}
