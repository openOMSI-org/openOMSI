//! Snow on the roads that builds up while it snows and thaws again (`RoadSnow`), and the
//! field of ruts and tyre tracks around the camera the renderer lays into it (`SnowTracks`,
//! see omsi-render's road_snow.wgsl).
//!
//! The weather's "snow on road" was a switch: the roads were white or bare from one moment
//! to the next, and the season's WinterSnowfall textures went with it. Here the carriageways
//! carry a cover of their own instead: it starts as the weather has it, grows while snow
//! falls (a heavy fall closes a road in some ten minutes), thaws when it is warm or rains,
//! and the traffic keeps the lanes' wheel tracks down to slush. The tyres of every vehicle
//! near the camera press tracks into it, which the snow falling after them fills again.
//!
//! The game's frame drives it through `frame`, `light` and `show_roofs` on one state kept
//! here, as `lights::CORONA_TEXTURES` keeps the coronas' pictures: the window and the
//! offscreen pictures both take it, and the app (the glue) only hands over its vehicles.

use std::collections::HashMap;

use glam::{DVec2, DVec3};
use omsi_render::{Renderer, SNOW_TRACK_SIDE, SNOW_TRACK_TEXELS, SNOW_TRACK_TILE};
use omsi_sim::traffic::{LaneKind, Network, GRID_CELL};

use crate::puddles::{vehicle_tyres, Tyre};

/// The roads' snow and its field of ruts and tracks, for the one session a process runs.
static STATE: parking_lot::Mutex<Option<(RoadSnow, SnowTracks)>> = parking_lot::Mutex::new(None);

/// The vehicles whose tyres and roofs take the snow, by the spray's keys: the player's bus
/// (0) and the traffic's cars.
pub fn vehicles<'a>(player: Option<&'a omsi_sim::VehicleInstance>, traffic: Option<&'a omsi_sim::ai_traffic::TrafficSim>) -> Vec<(u64, &'a omsi_sim::VehicleInstance)> {
    let mut out: Vec<_> = player.map(|v| (0, v)).into_iter().collect();
    out.extend(traffic.into_iter().flat_map(|t| &t.cars).map(|c| (c.id.wrapping_add(1), &c.vehicle)));
    out
}

/// One frame of `dt` weather seconds: the roads' cover, the field around `eye` (the lanes
/// of `net`, the tyres of `vehicles`) and the roofs of `vehicles`.
pub fn frame(r: &Renderer, dt: f32, weather: Option<&omsi_content::weather::Weather>, eye: DVec3, net: Option<&Network>, vehicles: &[(u64, &omsi_sim::VehicleInstance)]) {
    if !enabled() {
        switched_off(&mut STATE.lock());
        return;
    }
    let t0 = std::time::Instant::now();
    let mut state = STATE.lock();
    let (snow, tracks) = state.get_or_insert_with(Default::default);
    if let Some(w) = weather {
        snow.step(dt, w);
    }
    let (cover, fallen) = (snow.cover, snow.fallen);
    // Outside the winter - the roads bare, nothing falling, no roof holding any and none in
    // the weather's picture - there is nothing to do: the roofs go back to the weather's
    // snow (see `Roofs::amount`), which shows none either, so none of them changes, and the
    // field keeps what it had, unseen under no cover, until it snows again.
    if cover > 0.0 || snow.falling || !snow.roofs.bare() || snow.roofs.weather.is_some_and(|w| w > 0.0) {
        let mut tyres = std::mem::take(&mut snow.tyres);
        tyres.clear();
        for &(key, v) in vehicles {
            vehicle_tyres(v, key, &mut tyres);
        }
        tracks.update(r, eye, net, &tyres, cover, fallen);
        snow.tyres = tyres;
        // (the roofs once the weather's snow is known, from the first picture: till then
        // they show it, see `Roofs::amount`)
        if snow.roofs.weather.is_some() {
            snow.roofs.step(vehicles, cover, fallen);
        }
    } else {
        snow.roofs.clear();
        tracks.last.clear();
    }
    // (OMSI_ROAD_SNOW_LOG: the state every ten seconds of weather, to see why it grows or not,
    // and what this costs a frame: its mean and worst time, the rut texels drawn)
    if omsi_cfg::flags::OMSI_ROAD_SNOW_LOG.is_set() {
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        snow.log_cost = (snow.log_cost.0 + ms, snow.log_cost.1.max(ms), snow.log_cost.2 + 1);
        snow.log_t += dt;
        if snow.log_t >= 10.0 {
            snow.log_t = 0.0;
            let (kind, rate) = weather.map(omsi_weather::conditions::precip_of).unwrap_or((-1, 0.0));
            let (sum, worst, n) = std::mem::take(&mut snow.log_cost);
            log::info!("road snow: cover {cover:.3}, fallen {fallen:.0}, precip kind {kind} rate {rate:.2}, temp {:?}, snow on road {:?}, field ready {}, kept tracks (tiles, bytes, handed back) {:?}, frame {:.3} ms mean, {worst:.3} ms worst of {n}, rut texels {}", weather.map(|w| w.temp.0), weather.map(|w| w.snow_on_road), tracks.ready(), tracks.kept(), sum / n.max(1) as f64, std::mem::take(&mut tracks.rut_work));
        }
    }
}

/// What the picture draws the roads' snow with (nothing before the first `frame`), and
/// whether it can draw it at all (`enabled`): `on`, the player's setting (`choose`).
pub fn light(lighting: &mut omsi_render::Lighting, on: bool) {
    choose(on);
    DRAWN.store(!lighting.classic, std::sync::atomic::Ordering::Relaxed);
    if let (true, Some((snow, tracks))) = (enabled(), STATE.lock().as_mut()) {
        // (the weather's own snow on everything: what a roof not kept here shows)
        snow.roofs.weather = Some(lighting.snow.clamp(0.0, 1.0));
        snow.light(lighting, tracks);
    }
}

/// The roofs' snow on the renders `render_of` finds by the vehicles' keys: only on a
/// vehicle whose roof has changed by a step the shader tells apart, or whose render is new
/// (walking every part of every vehicle each frame cost the same in summer as in a storm).
pub fn show_roofs<'a>(
    r: &Renderer,
    scene: &mut omsi_render::Scene,
    keys: impl IntoIterator<Item = u64>,
    render_of: impl Fn(u64) -> Option<(&'a crate::VehicleRender, &'a [crate::VehicleRender])>,
) {
    // (switched off, the shader takes no roof's own snow: nothing to show, nor a cache to
    // keep - `frame` lets the state go)
    if !enabled() {
        return;
    }
    let mut state = STATE.lock();
    let Some((snow, _)) = state.as_mut() else { return };
    let roofs = &mut snow.roofs;
    roofs.shows = roofs.shows.wrapping_add(1);
    let now = roofs.shows;
    for key in keys {
        let Some((body, trailers)) = render_of(key) else { continue };
        let amount = roofs.amount(key);
        let step = amount.map(|a| (a.clamp(0.0, 1.0) * 40.0).round() as u8);
        let parts = std::iter::once(body).chain(trailers).map(|v| v.instances.len() + v.far.len()).sum();
        let render = (body.instances.first().copied().unwrap_or(usize::MAX), parts);
        if let Some(s) = roofs.shown.get_mut(&key) {
            s.2 = now;
            if (s.0, s.1) == (render, step) {
                continue;
            }
        }
        show_roof(r, scene, body, trailers, amount);
        roofs.shown.insert(key, (render, step, now));
    }
    // (the vehicles gone for a while are forgotten)
    if now % 600 == 0 {
        roofs.shown.retain(|_, s| now.wrapping_sub(s.2) < 600);
    }
}

/// Built-up snow on the roads instead of the weather's on/off "snow on road", where the
/// player has it on (the `road_snow` setting, see `choose`; a set `OMSI_ROAD_SNOW` decides
/// instead: `off` for the old way, a number for the cover to start with) and the picture can
/// draw it: not in the classic Vanilla picture, which shows snow as OMSI 2 does, only through
/// the season's WinterSnowfall textures (`light` learns the picture's kind and the setting;
/// the season's textures follow within a second, see the app's `follow_date`).
pub fn enabled() -> bool {
    DRAWN.load(std::sync::atomic::Ordering::Relaxed) && switched_on() && omsi_render::road_snow_supported()
}

/// The season's folder for a weather with snow lying: with snow on the road the snowy roads
/// of WinterSnowfall, unless the roads' snow is built up here instead (`enabled`).
pub fn snow_folder(snow_on_road: bool) -> &'static str {
    if snow_on_road && !enabled() { "WinterSnowfall" } else { "WinterSnow" }
}

/// The player's `road_snow` setting, unless `OMSI_ROAD_SNOW` is set.
fn switched_on() -> bool {
    switched_on_by(omsi_cfg::flags::OMSI_ROAD_SNOW.var(), CHOSEN.load(std::sync::atomic::Ordering::Relaxed))
}

/// `switched_on` by the flag's value (`flag`) and the setting (`chosen`): a set flag decides
/// both ways (`off` off, a cover to start with on), as `OMSI_WINDY_TREES` does for its setting.
fn switched_on_by(flag: Option<&str>, chosen: bool) -> bool {
    match flag.map(str::trim) {
        Some("off" | "0ff" | "false") => false,
        Some(_) => true,
        None => chosen,
    }
}

/// The player's `road_snow` setting (see `choose`).
static CHOSEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// The player's `road_snow` setting: told before the world opens, so that its season
/// textures are chosen by it (`enabled`), and by every picture (`light`), so that a switch in
/// the Settings or a loaded graphics profile takes at the next frame.
pub fn choose(on: bool) {
    CHOSEN.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Switched off (the setting, or a picture that cannot draw it): the roads' snow and its
/// field are let go, so that switched on again it starts from the weather as at the start,
/// and the tracks kept of the tiles let go take no memory meanwhile.
fn switched_off(state: &mut Option<(RoadSnow, SnowTracks)>) {
    if state.as_ref().is_some_and(|(snow, _)| snow.started) {
        *state = None;
    }
}

/// The picture draws the roads' built-up snow (not the classic Vanilla one, see `enabled`).
static DRAWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// The session's picture is the classic Vanilla one (`classic`, handed back): told before
/// the world opens, so that its season textures are chosen by it (`enabled`); `light` keeps
/// it up to date when the graphics are switched during the session.
pub fn picture_classic(classic: bool) -> bool {
    DRAWN.store(!classic, std::sync::atomic::Ordering::Relaxed);
    classic
}

/// A full road cover per second of the heaviest snowfall: ten minutes to close a road.
const SETTLE_RATE: f32 = 1.0 / 600.0;
/// How deep the lanes' ruts show (the traffic of a road that has had any).
const RUTS: f32 = 0.8;

/// The snow on the roads over time.
#[derive(Default)]
pub struct RoadSnow {
    /// How far it covers the carriageways (0..1).
    pub cover: f32,
    /// The snow fallen since the start, in thousandths of a full cover (it only grows): a
    /// tyre track remembers it and fills as more falls.
    pub fallen: f64,
    /// Snow is falling (and settles).
    pub falling: bool,
    started: bool,
    log_t: f32,
    /// The frames' time since the last log (ms: sum, worst, frames).
    log_cost: (f64, f64, u32),
    /// The snow on the roofs of the vehicles around the camera.
    pub roofs: Roofs,
    /// The tyres of the frame (kept for the next one's).
    tyres: Vec<Tyre>,
}

impl RoadSnow {
    /// `dt` seconds of the weather: the first call takes the weather's "snow on road" (or
    /// `OMSI_ROAD_SNOW`'s number) as the roads' state to begin with.
    pub fn step(&mut self, dt: f32, w: &omsi_content::weather::Weather) {
        if !self.started {
            self.started = true;
            self.cover = omsi_cfg::flags::OMSI_ROAD_SNOW
                .parse::<f32>()
                .unwrap_or(if w.snow_on_road { 1.0 } else { 0.0 })
                .clamp(0.0, 1.0);
        }
        let (kind, rate) = omsi_weather::conditions::precip_of(w);
        let temp = w.temp.0;
        self.falling = kind == 2 && rate > 0.0 && temp < 2.0;
        if self.falling {
            let fall = dt * rate * SETTLE_RATE;
            self.cover += fall;
            self.fallen += fall as f64 * 1000.0;
        } else {
            // it thaws by the warmth of the air, and faster under rain
            let warm = (temp - 0.5).max(0.0) * 0.0002;
            let washed = if kind == 1 { rate * 0.003 } else { 0.0 };
            self.cover -= dt * (warm + washed);
        }
        self.cover = self.cover.clamp(0.0, 1.0);
    }

    /// What the renderer draws the roads' snow with.
    pub fn light(&self, lighting: &mut omsi_render::Lighting, tracks: &SnowTracks) {
        lighting.road_snow = Some(self.cover);
        lighting.snow_fallen = self.fallen;
        lighting.snow_ruts = RUTS;
        lighting.snow_tracks = tracks.ready();
    }
}

/// The snow fallen on a roof that closes it (thousandths of a road cover, as `fallen`): a
/// roof is white a little before the road, which the traffic keeps down.
const ROOF_FILL: f64 = 700.0;
/// Speed (m/s) above which the airstream takes snow off a roof, and how much a metre at
/// 12 m/s above it: some 400 m at 50 km/h, 200 m at 80.
const ROOF_BLOW_FROM: f32 = 6.0;
const ROOF_BLOW: f32 = 0.004;

/// The snow on one vehicle's roof: it gathers what falls, is blown off by the airstream
/// when the vehicle goes fast, and thaws as the roads' snow does. Kept by the snow fallen
/// and the way driven, not by the clock, so a run that renders only now and then (the
/// offscreen pictures) keeps it as the window does.
#[derive(Clone, Copy, Debug)]
pub struct RoofSnow {
    /// How much lies on it (0..1).
    pub amount: f32,
    fallen: f64,
    cover: f32,
    at: DVec3,
    /// The step it was last seen in (see `Roofs::step`).
    seen: u32,
}

impl RoofSnow {
    fn new(amount: f32, fallen: f64, cover: f32, at: DVec3) -> Self {
        RoofSnow { amount: amount.clamp(0.0, 1.0), fallen, cover, at, seen: 0 }
    }

    fn step(&mut self, fallen: f64, cover: f32, at: DVec3, speed: f32) {
        // what has fallen since
        self.amount += ((fallen - self.fallen).max(0.0) / ROOF_FILL) as f32;
        self.fallen = fallen;
        // the airstream, over the way driven (a jump - a car put elsewhere - blows nothing)
        let way = (at - self.at).truncate().length() as f32;
        self.at = at;
        if way < 50.0 && speed.abs() > ROOF_BLOW_FROM {
            self.amount -= way * (speed.abs() - ROOF_BLOW_FROM) / 12.0 * ROOF_BLOW;
        }
        // the thaw: as the roads' snow goes, the roofs' goes a little faster
        if cover < self.cover {
            self.amount -= (self.cover - cover) * 1.3;
        }
        self.cover = cover;
        if cover <= 0.0 {
            self.amount = self.amount.min(0.0);
        }
        self.amount = self.amount.clamp(0.0, 1.0);
    }
}

/// The roofs' snow of the vehicles around the camera, by the spray's keys (0 the player's).
#[derive(Default)]
pub struct Roofs {
    map: HashMap<u64, RoofSnow>,
    /// The steps taken (see `RoofSnow::seen`).
    steps: u32,
    /// What each vehicle's render was last shown with (its first instance and parts, the
    /// shader's step of its snow) and when (`show_roofs`), and how many times it was.
    shown: HashMap<u64, ((usize, usize), Option<u8>, u32)>,
    shows: u32,
    /// The weather's own snow on everything (0..1, the picture's `Lighting::snow`): what a
    /// roof not kept here shows, and what a roof kept here starts from and holds while it
    /// lies; None until the first picture has told it (`light`).
    pub weather: Option<f32>,
}

impl Roofs {
    /// One step of `vehicles` (key, vehicle): one seen for the first time starts with as
    /// much as has settled on the roads (the player's bus) or a little less (the traffic,
    /// which has been driving), and no less than the weather's snow it showed until then
    /// (the roofs do not jump bare when it starts to fall on snow lying); one no longer
    /// there is forgotten. A roof thaws with the roads' snow and the weather's, whichever
    /// stays longer: a roof the roads' thaw has cleared while the weather still shows snow
    /// would turn white again the moment it is no longer kept.
    pub fn step(&mut self, vehicles: &[(u64, &omsi_sim::VehicleInstance)], cover: f32, fallen: f64) {
        self.step_at(vehicles.iter().map(|&(key, v)| (key, v.position, v.physics.speed)), cover, fallen);
    }

    /// `step` by the vehicles' keys, places and speeds.
    fn step_at(&mut self, vehicles: impl Iterator<Item = (u64, DVec3, f32)>, cover: f32, fallen: f64) {
        self.steps = self.steps.wrapping_add(1);
        let now = self.steps;
        let weather = self.weather.unwrap_or(0.0);
        let lying = cover.max(weather);
        for (key, at, speed) in vehicles {
            let r = self.map.entry(key).or_insert_with(|| {
                RoofSnow::new(if key == 0 { lying } else { (cover * 0.6).max(weather) }, fallen, lying, at)
            });
            r.step(fallen, lying, at, speed);
            r.seen = now;
        }
        self.map.retain(|_, r| r.seen == now);
    }

    /// How much snow lies on vehicle `key`'s roof, None for a vehicle whose roof is not
    /// kept here (another player's bus over the LAN, every vehicle outside the winter),
    /// which shows the weather's snow as before.
    pub fn amount(&self, key: u64) -> Option<f32> {
        self.map.get(&key).map(|r| r.amount)
    }

    /// No roof holds snow.
    pub fn bare(&self) -> bool {
        self.map.values().all(|r| r.amount <= 0.0)
    }

    /// Forget every roof (they show the weather's snow again).
    pub fn clear(&mut self) {
        self.map.clear();
    }
}

/// Show the roof's snow on every part of a vehicle's render (its coupled parts' too): None
/// gives it back the weather's snow.
pub fn show_roof(
    r: &Renderer,
    scene: &mut omsi_render::Scene,
    body: &crate::VehicleRender,
    trailers: &[crate::VehicleRender],
    amount: Option<f32>,
) {
    // (the far levels of detail too: a far car showed the weather's snow, its near model its own)
    for render in std::iter::once(body).chain(trailers) {
        for &i in render.instances.iter().chain(&render.far) {
            r.set_roof_snow(scene, i, amount);
        }
    }
}

/// A tile of the field: 32 m a side, 256 texels.
const TILE_M: f64 = 32.0;
const TILE_PX: usize = 256;
/// Tiles a side of the field (it wraps around the world every `SNOW_TRACK_SIDE` metres).
const SLOTS: usize = 8;
const TEXEL: f64 = TILE_M / TILE_PX as f64;
/// The texels near a lane the ruts look at in a frame, about (a tile is begun while some
/// are left), each tile counting `RUT_TILE_WORK` more for its own passes and upload, and the
/// tiles a frame draws at most: well under a millisecond. The row of tiles the camera moves
/// into is drawn over the next eight frames (at the field's far edge, beyond TRACKS_SEEN),
/// a jump of the camera or the lanes' growth over a second or so. (The whole row in one
/// frame, each tile sent to the GPU cleared and again with its ruts, measured 4 to 6 ms each
/// time the camera crossed a tile's edge - the sending, mostly.)
const RUT_WORK_PER_FRAME: usize = 40_000;
const RUT_TILE_WORK: usize = TILE_PX * TILE_PX / 4;
const RUT_TILES_PER_FRAME: usize = 1;
/// How near the camera the ruts and tracks show (m): road_snow.wgsl fades them out from 75 m
/// and reads none beyond 90. The tiles the field takes as the camera moves lie 96 m off and
/// more, and are drawn (a row of eight, a tile a frame) before it comes 6 m nearer.
const TRACKS_SEEN: f64 = 90.0;
/// The longest piece of a lane the ruts take as one (m): the tracks' wander is taken at its
/// ends and blended between, and its box stays narrow where the lane runs on a diagonal.
const RUT_STEP: f64 = 2.0;
/// Frames the lane network must stay as it is before the ruts are drawn again from it.
const LANES_STEADY: u32 = 30;
/// Half the gauge of a lane's wheel tracks and how wide each rut is (m): between a car's
/// and a bus's, so both drive in them.
const RUT_HALF_GAUGE: f64 = 0.9;
const RUT_SIGMA: f64 = 0.24;
/// How far from a lane's line its ruts reach (m): the gauge, the most they wander and the
/// widest bell out to where it rounds to nothing in a byte (see `rut_piece`).
const RUT_REACH: f64 = RUT_HALF_GAUGE + 0.1 + 3.6 * 1.25 * RUT_SIGMA;
/// Half a tyre's width (m).
const TYRE_HALF: f64 = 0.13;
/// Tyres farther from the camera leave no track (m).
const TRACK_RANGE: f64 = 110.0;
/// Snow fallen on a tyre track that has filled it for good (thousandths of a cover: the
/// rut, the last to go, is gone at 520, see road_snow.wgsl), and the most the tracks kept
/// of the tiles the field has let go may take (bytes).
const KEPT_FILLED: f64 = 600.0;
const KEPT_MAX_BYTES: usize = 64 << 20;

#[derive(Default)]
struct Slot {
    /// The world tile it holds (x, y in tiles).
    tile: Option<(i64, i64)>,
    /// Its texels (RGBA, rows along world y).
    px: Vec<u8>,
    ruts_done: bool,
    /// Its ruts have been drawn since it took this tile (and may be drawn again).
    drawn: bool,
    /// The texels changed since the last upload (x0, y0, x1, y1, exclusive ends).
    dirty: Option<(usize, usize, usize, usize)>,
}

impl Slot {
    fn touch(&mut self, x0: usize, y0: usize, x1: usize, y1: usize) {
        self.dirty = Some(match self.dirty {
            Some((a, b, c, d)) => (a.min(x0), b.min(y0), c.max(x1), d.max(y1)),
            None => (x0, y0, x1, y1),
        });
    }
}

/// The ruts and the tyre tracks of the roads around the camera (see road_snow.wgsl).
#[derive(Default)]
pub struct SnowTracks {
    slots: Vec<Slot>,
    /// The lanes the ruts were drawn from (the network grows as tiles load), the count
    /// last seen and the frames it has stayed so.
    lanes_drawn: usize,
    lanes_seen: usize,
    lanes_steady: u32,
    /// Each tyre's contact last frame.
    last: HashMap<u64, DVec3>,
    ready: bool,
    /// The tracks of the tiles the field has let go (it follows the camera), by world tile:
    /// handed back when the camera comes back, filled as much as the snow fallen since says
    /// (see `keep_tracks`).
    kept: HashMap<(i64, i64), Vec<u8>>,
    kept_bytes: usize,
    /// Frames since the kept tracks were last looked over, and tiles handed back so far.
    kept_frames: u32,
    restored: u32,
    /// The rut texels looked at since the last log (OMSI_ROAD_SNOW_LOG).
    rut_work: usize,
    /// A tile's ruts before they are drawn again (kept between the tiles, see `draw_ruts`).
    ruts_before: Vec<u8>,
}

/// A tile's tyre tracks, packed: the snow fallen when the newest was pressed (2 bytes), then
/// each pressed texel's index (2), how much of it the tyre covered (1) and its moment (2).
/// None when it holds no track.
fn keep_tracks(px: &[u8]) -> Option<Vec<u8>> {
    let mut out = vec![0u8; 2];
    let mut newest: Option<u16> = None;
    for (i, t) in px.chunks_exact(4).enumerate() {
        if t[1] == 0 {
            continue;
        }
        let stamp = u16::from_be_bytes([t[2], t[3]]);
        out.extend_from_slice(&(i as u16).to_le_bytes());
        out.push(t[1]);
        out.extend_from_slice(&stamp.to_le_bytes());
        // (the newest by the wrapping difference: the stamps count modulo 65536)
        newest = Some(match newest {
            Some(n) if stamp.wrapping_sub(n) > 32768 => n,
            _ => stamp,
        });
    }
    let newest = newest?;
    out[..2].copy_from_slice(&newest.to_le_bytes());
    Some(out)
}

/// The snow fallen since a kept tile's newest track was pressed (thousandths of a cover).
fn kept_since(kept: &[u8], fallen: f64) -> f64 {
    let newest = u16::from_le_bytes([kept[0], kept[1]]) as f64;
    (fallen.rem_euclid(65536.0) - newest).rem_euclid(65536.0)
}

/// Put a kept tile's tracks back into its texels.
fn restore_tracks(px: &mut [u8], kept: &[u8]) {
    for r in kept[2..].chunks_exact(5) {
        let o = u16::from_le_bytes([r[0], r[1]]) as usize * 4;
        let stamp = u16::from_le_bytes([r[3], r[4]]);
        px[o + 1] = r[2];
        px[o + 2..o + 4].copy_from_slice(&stamp.to_be_bytes());
    }
}

impl SnowTracks {
    /// Tiles whose tracks are kept, the bytes they take, and tiles handed back so far.
    pub fn kept(&self) -> (usize, usize, u32) {
        (self.kept.len(), self.kept_bytes, self.restored)
    }

    /// The field holds the ruts of the tiles in sight around the camera (see TRACKS_SEEN).
    pub fn ready(&self) -> bool {
        self.ready
    }

    /// The field around `eye`: the ruts of `net`'s lanes, the tracks of `tyres` pressed in
    /// while the roads carry snow (`cover`), with the snow `fallen` so far.
    pub fn update(&mut self, r: &Renderer, eye: DVec3, net: Option<&Network>, tyres: &[Tyre], cover: f32, fallen: f64) {
        debug_assert_eq!(SLOTS * TILE_PX, SNOW_TRACK_TEXELS as usize);
        debug_assert_eq!(TILE_PX, SNOW_TRACK_TILE as usize);
        debug_assert!((SLOTS as f64 * TILE_M - SNOW_TRACK_SIDE).abs() < 1e-9);
        if cover <= 0.0 && !self.ready {
            // (no snow on the roads, none before: nothing to keep)
            self.last.clear();
            return;
        }
        if self.slots.is_empty() {
            self.slots = (0..SLOTS * SLOTS).map(|_| Slot::default()).collect();
        }
        // new lanes: the ruts drawn again (the tracks stay) - once the network has stopped
        // growing for a moment: tiles placed in the background bring their lanes a few at
        // a time, and redrawing at each set kept the field from ever being whole
        let lanes = net.map_or(0, |n| n.lanes.len());
        if lanes != self.lanes_seen {
            self.lanes_seen = lanes;
            self.lanes_steady = 0;
        }
        self.lanes_steady = self.lanes_steady.saturating_add(1);
        if lanes != self.lanes_drawn && self.lanes_steady >= LANES_STEADY {
            self.lanes_drawn = lanes;
            for s in &mut self.slots {
                s.ruts_done = false;
            }
        }
        // the tiles around the camera, each in its slot of the wrapping field
        let (xs, ys) = field_tiles(eye.truncate());
        for ty in ys {
            for tx in xs.clone() {
                let s = &mut self.slots[slot_of(tx, ty)];
                if s.tile != Some((tx, ty)) {
                    // the tile let go keeps its tracks, the one taken gets its own back
                    if let Some((old, packed)) = s.tile.zip(keep_tracks(&s.px)) {
                        self.kept_bytes += packed.len();
                        if let Some(was) = self.kept.insert(old, packed) {
                            self.kept_bytes -= was.len();
                        }
                    }
                    s.tile = Some((tx, ty));
                    s.px.clear();
                    s.px.resize(TILE_PX * TILE_PX * 4, 0);
                    if let Some(packed) = self.kept.remove(&(tx, ty)) {
                        self.kept_bytes -= packed.len();
                        if kept_since(&packed, fallen) < KEPT_FILLED {
                            restore_tracks(&mut s.px, &packed);
                            self.restored += 1;
                        }
                    }
                    s.ruts_done = false;
                    s.drawn = false;
                }
            }
        }
        // the ruts, as far as the frame's share goes: of the tiles not drawn since they were
        // taken first, the nearest first, then of those the lanes' growth asks for again; a
        // tile goes to the GPU once its ruts are in (a whole tile at a time cost far more to
        // send than to draw), one drawn again only where they changed
        let off = |t: Option<(i64, i64)>| t.map_or(0.0, |t| tile_off(t, eye.truncate()));
        let mut order: Vec<(bool, f64, usize)> =
            self.slots.iter().enumerate().filter(|(_, s)| !s.ruts_done).map(|(i, s)| (s.drawn, off(s.tile), i)).collect();
        order.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let (mut work, mut tiles) = (RUT_WORK_PER_FRAME, RUT_TILES_PER_FRAME);
        for &(was_drawn, _, i) in &order {
            if work == 0 || tiles == 0 {
                break;
            }
            let s = &mut self.slots[i];
            let tile = s.tile.expect("a slot of the field holds a tile");
            let (done, changed) = draw_ruts(s, tile, net, fallen, &mut self.ruts_before);
            work = work.saturating_sub(done + RUT_TILE_WORK);
            tiles -= 1;
            self.rut_work += done;
            s.ruts_done = true;
            s.drawn = true;
            if changed || !was_drawn {
                s.touch(0, 0, TILE_PX, TILE_PX);
            }
        }
        // (a tile taken whose ruts wait for a later frame shows on the GPU what its slot held
        // before: where the camera moves, 96 m off and more, beyond the ruts' sight (see
        // TRACKS_SEEN). After a jump of the camera the field shows nothing until the tiles
        // in sight are drawn, the nearest first - half a second - rather than sending all
        // of them cleared at once.)
        let in_sight = self.slots.iter().all(|s| s.drawn || off(s.tile) >= TRACKS_SEEN);
        // the tyres' tracks, from where each touched the road last frame
        let mut now = HashMap::with_capacity(tyres.len());
        if cover > 0.02 {
            let stamp = (fallen.rem_euclid(65536.0)) as u16;
            // the kept tracks the snow has filled since are let go, and the oldest when
            // they would take too much
            self.kept_frames += 1;
            if self.kept_frames >= 300 || self.kept_bytes > KEPT_MAX_BYTES {
                self.kept_frames = 0;
                self.kept.retain(|_, k| kept_since(k, fallen) < KEPT_FILLED);
                self.kept_bytes = self.kept.values().map(Vec::len).sum();
                while self.kept_bytes > KEPT_MAX_BYTES {
                    let Some(oldest) = self.kept.iter().max_by(|a, b| kept_since(a.1, fallen).total_cmp(&kept_since(b.1, fallen))).map(|(k, _)| *k) else { break };
                    if let Some(k) = self.kept.remove(&oldest) {
                        self.kept_bytes -= k.len();
                    }
                }
            }
            for t in tyres {
                if (t.contact - eye).truncate().length() > TRACK_RANGE {
                    continue;
                }
                if let Some(&prev) = self.last.get(&t.key) {
                    let d = (t.contact - prev).truncate().length();
                    if d > 0.005 && d < 4.0 {
                        self.press(prev.truncate(), t.contact.truncate(), stamp);
                    }
                }
                now.insert(t.key, t.contact);
            }
        }
        self.last = now;
        // what changed goes to the GPU: the changed rows of each tile (whole rows lie one
        // after the other in the field's buffer)
        for (i, s) in self.slots.iter_mut().enumerate() {
            let Some((_, y0, _, y1)) = s.dirty.take() else { continue };
            r.set_snow_track_rows(i as u32, y0 as u32, &s.px[y0 * TILE_PX * 4..y1 * TILE_PX * 4]);
        }
        if in_sight && !self.ready {
            let rutted = self.slots.iter().filter(|s| s.px.chunks_exact(4).any(|p| p[0] > 64)).count();
            log::info!("road snow: track field ready around ({:.0}, {:.0}), {lanes} lanes, ruts on {rutted} of {} tiles", eye.x, eye.y, self.slots.len());
        }
        self.ready = in_sight;
    }

    /// A tyre's track from `a` to `b` (world), stamped with the snow fallen when it was
    /// pressed.
    fn press(&mut self, a: DVec2, b: DVec2, stamp: u16) {
        let lo = a.min(b) - DVec2::splat(TYRE_HALF + TEXEL);
        let hi = a.max(b) + DVec2::splat(TYRE_HALF + TEXEL);
        let (gx0, gy0) = ((lo.x / TEXEL).floor() as i64, (lo.y / TEXEL).floor() as i64);
        let (gx1, gy1) = ((hi.x / TEXEL).ceil() as i64, (hi.y / TEXEL).ceil() as i64);
        let ab = b - a;
        let len2 = ab.length_squared().max(1e-12);
        for gy in gy0..gy1 {
            for gx in gx0..gx1 {
                let p = DVec2::new((gx as f64 + 0.5) * TEXEL, (gy as f64 + 0.5) * TEXEL);
                let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
                // how much of the texel the tyre covers, over a texel's width at its edge:
                // pressed all or nothing, the track's edge was a staircase of texels
                let d = (p - (a + ab * t)).length();
                let cover = ((TYRE_HALF + TEXEL * 0.5 - d) / TEXEL).clamp(0.0, 1.0);
                if cover <= 0.0 {
                    continue;
                }
                let (tx, ty) = (gx.div_euclid(TILE_PX as i64), gy.div_euclid(TILE_PX as i64));
                let s = &mut self.slots[slot_of(tx, ty)];
                if s.tile != Some((tx, ty)) {
                    continue; // (a tile outside the field)
                }
                let (lx, ly) = (gx.rem_euclid(TILE_PX as i64) as usize, gy.rem_euclid(TILE_PX as i64) as usize);
                let o = (ly * TILE_PX + lx) * 4;
                let g = (cover * 255.0).round() as u8;
                // (a texel the tyre crosses mostly takes this moment's snow; its edge texels
                // keep the older moment of a fuller track under them)
                if g >= s.px[o + 1] || cover > 0.5 {
                    s.px[o + 2] = (stamp >> 8) as u8;
                    s.px[o + 3] = stamp as u8;
                }
                s.px[o + 1] = s.px[o + 1].max(g);
                s.touch(lx, ly, lx + 1, ly + 1);
            }
        }
    }
}

/// The world tiles the field holds around `eye` (x, y in tiles): eight a side, four ahead
/// of the camera's own and three behind it.
fn field_tiles(eye: DVec2) -> (std::ops::RangeInclusive<i64>, std::ops::RangeInclusive<i64>) {
    let (cx, cy) = ((eye.x / TILE_M).floor() as i64, (eye.y / TILE_M).floor() as i64);
    let half = SLOTS as i64 / 2;
    (cx - half + 1..=cx + half, cy - half + 1..=cy + half)
}

/// How far world tile (x, y) is from `eye` (m, 0 for the tile it stands in).
fn tile_off((x, y): (i64, i64), eye: DVec2) -> f64 {
    let lo = DVec2::new(x as f64, y as f64) * TILE_M;
    (lo - eye).max(eye - lo - DVec2::splat(TILE_M)).max(DVec2::ZERO).length()
}

/// The slot of the wrapping field that holds world tile (x, y).
fn slot_of(tx: i64, ty: i64) -> usize {
    ty.rem_euclid(SLOTS as i64) as usize * SLOTS + tx.rem_euclid(SLOTS as i64) as usize
}

/// The ruts of the street lanes over a tile: two wheel tracks along each lane, a bell's
/// profile across each (`r`, the strongest where lanes overlap).
///
/// A rut fills with the snow that falls while no wheel runs in it (road_snow.wgsl): each
/// texel keeps the moment a tyre last pressed it (`b`, `a`). Where none has yet, it takes
/// a moment as long ago as the lane's traffic makes likely - a busy street's ruts are
/// fresh, a quiet lane's half filled already.
///
/// Gives back the texels it looked at (the frame's share of the work, see
/// `RUT_WORK_PER_FRAME`) and whether the tile's ruts changed (`before`: room for the ruts
/// it had).
fn draw_ruts(s: &mut Slot, (tx, ty): (i64, i64), net: Option<&Network>, fallen: f64, before: &mut Vec<u8>) -> (usize, bool) {
    // (drawn again - the network has grown - a rut keeps the moment it had: its filling
    // goes on where it was)
    before.clear();
    before.extend(s.px.chunks_exact(4).map(|p| p[0]));
    for p in s.px.chunks_exact_mut(4) {
        p[0] = 0;
    }
    let work = draw_lanes(s, (tx, ty), net, fallen, before);
    (work, s.px.chunks_exact(4).zip(before.iter()).any(|(p, b)| p[0] != *b))
}

/// The ruts of `net`'s street lanes into the tile's cleared `r` (`before`: the ruts it had).
fn draw_lanes(s: &mut Slot, (tx, ty): (i64, i64), net: Option<&Network>, fallen: f64, before: &[u8]) -> usize {
    let x0 = tx as f64 * TILE_M;
    let y0 = ty as f64 * TILE_M;
    // (OMSI_ROAD_SNOW_GRID: a line every 4 m along both axes instead of the ruts, and a
    // thicker one where world x is a multiple of 32 m - to see the field lie on the world)
    if omsi_cfg::flags::OMSI_ROAD_SNOW_GRID.is_set() {
        for py in 0..TILE_PX {
            for px in 0..TILE_PX {
                let (gx, gy) = (px % 32, py % 32);
                let v = gx < 2 || gy < 2 || (px < 6);
                s.px[(py * TILE_PX + px) * 4] = if v { 255 } else { 0 };
            }
        }
        return TILE_PX * TILE_PX;
    }
    let Some(net) = net else { return 0 };
    let reach = RUT_REACH;
    // the lanes of the network's cells the tile (and the ruts' reach) touches
    let (c0x, c0y) = (((x0 - reach) / GRID_CELL).floor() as i32, ((y0 - reach) / GRID_CELL).floor() as i32);
    let (c1x, c1y) = (((x0 + TILE_M + reach) / GRID_CELL).floor() as i32, ((y0 + TILE_M + reach) / GRID_CELL).floor() as i32);
    let mut lanes: Vec<usize> = Vec::new();
    for cy in c0y..=c1y {
        for cx in c0x..=c1x {
            if let Some(l) = net.grid.get(&(cx, cy)) {
                lanes.extend_from_slice(l);
            }
        }
    }
    lanes.sort_unstable();
    lanes.dedup();
    let mut work = 0;
    for li in lanes {
        let lane = &net.lanes[li];
        // (every street lane: a bus lane or a terminus loop the random traffic keeps out
        // of has its ruts all the same)
        if lane.kind != LaneKind::Street {
            continue;
        }
        // (each lane its own wander, so that neighbouring lanes do not wander alike)
        let seed = li as f64 * 1.618;
        // (snow fallen since a wheel last ran here, thousandths of a cover: the ruts fill
        // from 150 on and are gone at 520, see the shader - a lane of the medium traffic
        // fresh, one the random traffic keeps out of (a terminus loop, a bus lane: the
        // buses still use it) a third filled)
        let since = 280.0 / (1.0 + 3.0 * lane.density.max(0.0) as f64);
        let then = (fallen - since).rem_euclid(65536.0) as u16;
        let point = |k: usize| lane.points.get(k).map(|p| p.truncate());
        for (k, w) in lane.points.windows(2).enumerate() {
            let (a, b) = (w[0].truncate(), w[1].truncate());
            let (d0, d1) = (
                lane.dist.get(k).copied().unwrap_or(0.0) as f64,
                lane.dist.get(k + 1).copied().unwrap_or(0.0) as f64,
            );
            let lo = a.min(b) - DVec2::splat(reach);
            let hi = a.max(b) + DVec2::splat(reach);
            if hi.x < x0 || hi.y < y0 || lo.x > x0 + TILE_M || lo.y > y0 + TILE_M {
                continue;
            }
            let near = (k.checked_sub(1).and_then(point), point(k + 2));
            work += rut_segment(s, (x0, y0), (a, b), near, (d0, d1), seed, then, before);
        }
    }
    work
}

/// The ruts along one segment `a`-`b` of a lane (`near`: the points before and after it,
/// `d0`, `d1`: how far down the lane its ends are), in pieces of at most RUT_STEP with the
/// tracks' shape taken at each one's ends; gives back the texels it looked at.
///
/// Each end reaches past itself as far as the bend there needs, up to the bisector of its
/// angle (a mitre): on the outside of a bend the two segments' ruts part like a fan, and a
/// texel in the gap belonged to neither (a fixed overlap left a row of snow specks in the
/// outer rut of a curve, one at every point); on its inside the next segment's rut already
/// turns away, and a rut drawn on straight there grew a wedge into the lane's middle.
#[allow(clippy::too_many_arguments)]
fn rut_segment(s: &mut Slot, origin: (f64, f64), (a, b): (DVec2, DVec2), (prev, next): (Option<DVec2>, Option<DVec2>), (d0, d1): (f64, f64), seed: f64, then: u16, before: &[u8]) -> usize {
    let len = (b - a).length();
    if len < 1e-3 {
        return 0;
    }
    let dir = (b - a) / len;
    let slack = 0.02 * len;
    // (how far past an end its bend needs - the gap's half angle's tangent at the reach, at
    // most a metre - and the bisector there; a lane's own end as little as a piece did before)
    let bend = |d: Option<DVec2>| {
        d.map_or((slack, None), |d| {
            let cos = d.dot(dir).clamp(-1.0, 1.0);
            let past = (RUT_REACH * ((1.0 - cos) / (1.0 + cos).max(1e-6)).sqrt()).min(1.0) + slack;
            (past, (d + dir).try_normalize())
        })
    };
    let (back, start) = bend(prev.and_then(|p| (a - p).try_normalize()));
    let (ahead, end) = bend(next.and_then(|n| (n - b).try_normalize()));
    let steps = (len / RUT_STEP).ceil().max(1.0) as usize;
    let piece_len = len / steps as f64;
    let mut work = 0;
    let mut f0 = 0.0;
    let mut shape0 = rut_shape(d0, seed);
    for j in 1..=steps {
        let f1 = j as f64 / steps as f64;
        let shape1 = rut_shape(d0 + (d1 - d0) * f1, seed);
        let ends = Ends {
            lo: if j == 1 { -back / piece_len } else { -0.02 },
            hi: if j == steps { 1.0 + ahead / piece_len } else { 1.02 },
            start: start.filter(|_| j == 1).map(|m| (a, m)),
            end: end.filter(|_| j == steps).map(|m| (b, m)),
            slack,
        };
        work += rut_piece(s, origin, (a + (b - a) * f0, a + (b - a) * f1), (shape0, shape1), ends, then, before);
        (f0, shape0) = (f1, shape1);
    }
    work
}

/// How far along a piece its ruts reach: `t` from `lo` to `hi` (0 at its start, 1 at its
/// end), and past a point where the lane bends only up to the bisector there, give or take
/// `slack` m (`start`, `end`: the point and the bisector's normal, along the lane).
#[derive(Clone, Copy)]
struct Ends {
    lo: f64,
    hi: f64,
    start: Option<(DVec2, DVec2)>,
    end: Option<(DVec2, DVec2)>,
    slack: f64,
}

/// The wheel tracks' shape `along` m down a lane: they do not run on a rail but wander a
/// hand's breadth off its line, widen and narrow, and are worn deeper in places
/// (wander, width, depth).
fn rut_shape(along: f64, seed: f64) -> [f64; 3] {
    [
        (along / 9.0 + seed).sin() * 0.07 + (along / 3.7 + seed * 2.3).sin() * 0.03,
        RUT_SIGMA * (1.0 + 0.25 * (along / 6.3 + seed * 0.7).sin()),
        0.75 + 0.25 * (along / 4.1 + seed * 1.9).sin(),
    ]
}

/// The ruts along one short piece `a`-`b` of a lane over the tile at `x0`, `y0` (world), with
/// the tracks' shape at its two ends, as far as `ends` says; gives back the texels it looked
/// at (`before`: the tile's ruts before they were cleared).
#[allow(clippy::too_many_arguments)]
fn rut_piece(s: &mut Slot, (x0, y0): (f64, f64), (a, b): (DVec2, DVec2), (s0, s1): ([f64; 3], [f64; 3]), ends: Ends, then: u16, before: &[u8]) -> usize {
    let reach = RUT_REACH;
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 < 1e-6 {
        return 0;
    }
    let (t_lo, t_hi) = (ends.lo, ends.hi);
    let (ea, eb) = (a + ab * t_lo, a + ab * t_hi);
    let lo = ea.min(eb) - DVec2::splat(reach);
    let hi = ea.max(eb) + DVec2::splat(reach);
    if hi.x < x0 || hi.y < y0 || lo.x > x0 + TILE_M || lo.y > y0 + TILE_M {
        return 0;
    }
    let py0 = (((lo.y - y0) / TEXEL).floor().max(0.0)) as usize;
    let py1 = (((hi.y - y0) / TEXEL).ceil() as usize).min(TILE_PX);
    let n = DVec2::new(-ab.y, ab.x) / len2.sqrt();
    let mut work = 0;
    for py in py0..py1 {
        // (of the row, only the texels within reach across the piece and along it: on a
        // diagonal its box held three times as many)
        let y = y0 + (py as f64 + 0.5) * TEXEL;
        let (mut xlo, mut xhi) = (lo.x, hi.x);
        let mut clip = |k: f64, c: f64, from: f64, to: f64| {
            if k.abs() < 1e-12 {
                if c < from || c > to {
                    xhi = f64::MIN;
                }
            } else {
                let (u, v) = ((from - c) / k, (to - c) / k);
                xlo = xlo.max(u.min(v));
                xhi = xhi.min(u.max(v));
            }
        };
        clip(n.x, n.y * (y - a.y) - n.x * a.x, -reach, reach);
        clip(ab.x / len2, (ab.y * (y - a.y) - ab.x * a.x) / len2, t_lo, t_hi);
        if xhi < xlo {
            continue;
        }
        let px0 = ((xlo - x0) / TEXEL - 0.5).ceil().max(0.0) as usize;
        let px1 = (((xhi - x0) / TEXEL - 0.5).floor() + 1.0).clamp(0.0, TILE_PX as f64) as usize;
        work += px1.saturating_sub(px0);
        for px in px0..px1 {
            let p = DVec2::new(x0 + (px as f64 + 0.5) * TEXEL, y0 + (py as f64 + 0.5) * TEXEL);
            let t = (p - a).dot(ab) / len2;
            // (each piece of the lane up to its ends: the next piece goes on from there)
            if !(t_lo..=t_hi).contains(&t) {
                continue;
            }
            // (past a bend's point only as far as its bisector)
            let cut = |e: Option<(DVec2, DVec2)>, sign: f64| e.is_some_and(|(q, m)| (p - q).dot(m) * sign > ends.slack);
            if (t < 0.0 && cut(ends.start, -1.0)) || (t > 1.0 && cut(ends.end, 1.0)) {
                continue;
            }
            let across = (p - (a + ab * t)).length();
            if across > reach {
                continue;
            }
            let k = t.clamp(0.0, 1.0);
            let [wander, width, depth] = std::array::from_fn(|n| s0[n] + (s1[n] - s0[n]) * k);
            let off = (across - RUT_HALF_GAUGE - wander) / width;
            // (farther out the bell rounds to nothing in a byte)
            if off.abs() > 3.6 {
                continue;
            }
            let rut = (-0.5 * off * off).exp() * depth;
            let v = (rut * 255.0).round() as u8;
            let o = (py * TILE_PX + px) * 4;
            if v > s.px[o] {
                s.px[o] = v;
                // (a texel a tyre has pressed keeps its own moment, as one
                // that was a rut before)
                if s.px[o + 1] == 0 && before[py * TILE_PX + px] == 0 {
                    s.px[o + 2] = (then >> 8) as u8;
                    s.px[o + 3] = then as u8;
                }
            }
        }
    }
    work
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tile_let_go_gets_its_tracks_back_filled_as_far_as_the_snow_since() {
        let mut px = vec![0u8; TILE_PX * TILE_PX * 4];
        // a rut texel (r) with no tyre, and two pressed texels of different moments
        px[0] = 200;
        for (i, g, stamp) in [(10usize, 255u8, 1000u16), (70_000 % (TILE_PX * TILE_PX), 128, 1100)] {
            px[i * 4 + 1] = g;
            px[i * 4 + 2..i * 4 + 4].copy_from_slice(&stamp.to_be_bytes());
        }
        let packed = keep_tracks(&px).expect("tracks kept");
        assert_eq!(packed.len(), 2 + 2 * 5);
        // the newest moment decides when the tile is filled for good
        assert!((kept_since(&packed, 1300.0) - 200.0).abs() < 1e-9);
        let mut back = vec![0u8; TILE_PX * TILE_PX * 4];
        restore_tracks(&mut back, &packed);
        for i in [10usize, 70_000 % (TILE_PX * TILE_PX)] {
            assert_eq!(&back[i * 4 + 1..i * 4 + 4], &px[i * 4 + 1..i * 4 + 4]);
        }
        // (the ruts are drawn again from the lanes, not kept)
        assert_eq!(back[0], 0);
        // a tile without a track keeps nothing, and the moments wrap
        assert!(keep_tracks(&vec![0u8; TILE_PX * TILE_PX * 4]).is_none());
        assert!((kept_since(&packed, 65536.0 + 1100.0 + 5.0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn the_ruts_drawn_in_short_pieces_are_the_ruts_of_every_texel() {
        // a long lane piece across the tile, on a diagonal: drawn in RUT_STEP pieces with the
        // tracks' shape blended between their ends, against the shape taken at each texel
        let (a, b) = (DVec2::new(1.0, 2.0), DVec2::new(30.5, 21.0));
        let (d0, d1, seed) = (40.0, 40.0 + (b - a).length(), 3.1);
        let mut s = Slot { px: vec![0u8; TILE_PX * TILE_PX * 4], ..Default::default() };
        let before = vec![0u8; TILE_PX * TILE_PX];
        let work = rut_segment(&mut s, (0.0, 0.0), (a, b), (None, None), (d0, d1), seed, 0, &before);
        // (the whole piece's box, as it was looked at before: far more than the pieces')
        let reach = RUT_REACH;
        let whole = ((b.x - a.x + 2.0 * reach) / TEXEL) * ((b.y - a.y + 2.0 * reach) / TEXEL);
        assert!((work as f64) < whole * 0.4, "{work} of {whole}");
        let (ab, mut worst, mut ruts) = (b - a, 0i32, 0);
        for py in 0..TILE_PX {
            for px in 0..TILE_PX {
                let p = DVec2::new((px as f64 + 0.5) * TEXEL, (py as f64 + 0.5) * TEXEL);
                let t = (p - a).dot(ab) / ab.length_squared();
                let mut want = 0u8;
                if (0.0..=1.0).contains(&t) {
                    let [wander, width, depth] = rut_shape(d0 + (d1 - d0) * t, seed);
                    let off = ((p - (a + ab * t)).length() - RUT_HALF_GAUGE - wander) / width;
                    want = ((-0.5 * off * off).exp() * depth * 255.0).round() as u8;
                }
                let got = s.px[(py * TILE_PX + px) * 4];
                if (0.0..=1.0).contains(&t) {
                    worst = worst.max((got as i32 - want as i32).abs());
                }
                ruts += (got > 128) as usize;
            }
        }
        assert!(worst <= 2, "{worst}");
        assert!(ruts > 1000, "{ruts}");
    }

    /// A lane on an arc of `radius` m in chords of `chord` m, its ruts drawn: the texels of
    /// the outer rut's line left in snow, how many there are, and the most rut on the inner
    /// flank (between the inner rut's line and the lane's middle) and in the middle.
    fn arc_ruts(radius: f64, chord: f64) -> (usize, usize, u8, u8) {
        let c = DVec2::new(16.0, 16.0);
        let step = chord / radius;
        let angles: Vec<f64> = (0..).map(|i| 0.3 + i as f64 * step).take_while(|t| *t < 2.9).collect();
        let points: Vec<DVec2> = angles.iter().map(|&t| c + DVec2::new(t.cos(), t.sin()) * radius).collect();
        let mut s = Slot { px: vec![0u8; TILE_PX * TILE_PX * 4], ..Default::default() };
        let before = vec![0u8; TILE_PX * TILE_PX];
        for k in 0..points.len() - 1 {
            let near = (k.checked_sub(1).map(|i| points[i]), points.get(k + 2).copied());
            let d = (k as f64 * chord, (k + 1) as f64 * chord);
            rut_segment(&mut s, (0.0, 0.0), (points[k], points[k + 1]), near, d, 0.0, 0, &before);
        }
        // (each texel by its distance to the nearest chord, on the bend's outside or inside;
        // the ruts wander 10 cm off their line)
        let (mut holes, mut outer, mut flank, mut middle) = (0, 0, 0u8, 0u8);
        for py in 0..TILE_PX {
            for px in 0..TILE_PX {
                let p = DVec2::new((px as f64 + 0.5) * TEXEL, (py as f64 + 0.5) * TEXEL);
                let angle = (p - c).y.atan2((p - c).x);
                if angle < angles[0] + step || angle > angles[angles.len() - 1] - step {
                    continue;
                }
                let (dist, out) = points
                    .windows(2)
                    .map(|w| {
                        let ab = w[1] - w[0];
                        let q = w[0] + ab * ((p - w[0]).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                        ((p - q).length(), (p - q).dot(q - c) > 0.0)
                    })
                    .min_by(|x, y| x.0.total_cmp(&y.0))
                    .unwrap();
                let got = s.px[(py * TILE_PX + px) * 4];
                if out && (0.82..=0.9).contains(&dist) {
                    outer += 1;
                    holes += (got == 0) as usize;
                }
                if !out && (0.35..=0.45).contains(&dist) {
                    flank = flank.max(got);
                }
                if dist < 0.1 {
                    middle = middle.max(got);
                }
            }
        }
        (holes, outer, flank, middle)
    }

    #[test]
    fn the_ruts_of_a_curve_have_no_gaps_outside_and_no_wedges_inside() {
        // spline lanes in 3 m chords of a 12 m bend (14 degrees at each point), a turning
        // path's in 2 m chords of 4.5 m (25 degrees): the outer rut's line has no texel left
        // in snow where two chords meet, and the inner rut grows no wedge toward the middle
        for (radius, chord) in [(12.0, 3.0), (4.5, 2.0)] {
            let (holes, outer, flank, middle) = arc_ruts(radius, chord);
            eprintln!("r {radius} chord {chord}: holes {holes} of {outer}, flank {flank}, middle {middle}");
            assert!(outer > 30, "{outer}");
            assert_eq!(holes, 0, "r {radius}");
            assert!(flank < 90, "r {radius}: {flank}");
            assert!(middle < 40, "r {radius}: {middle}");
        }
    }

    #[test]
    fn a_roof_not_kept_shows_the_weathers_snow() {
        let mut roofs = Roofs::default();
        // (another player's bus over the LAN, or no step yet: the weather's snow, not none)
        assert_eq!(roofs.amount(7), None);
        assert!(roofs.bare());
        roofs.step_at([(0, DVec3::ZERO, 0.0), (7, DVec3::X, 0.0)].into_iter(), 0.5, 0.0);
        assert_eq!(roofs.amount(0), Some(0.5));
        assert_eq!(roofs.amount(7), Some(0.3));
        assert!(!roofs.bare());
        // one gone is forgotten, and all of them outside the winter
        roofs.step_at([(0, DVec3::ZERO, 0.0)].into_iter(), 0.5, 0.0);
        assert_eq!(roofs.amount(7), None);
        roofs.clear();
        assert_eq!(roofs.amount(0), None);
        // snow starting to fall on snow lying: a roof starts from the white the weather's
        // snow showed on it, not bare, and keeps it while it lies after the roads thawed
        roofs.weather = Some(1.0);
        roofs.step_at([(5, DVec3::ZERO, 0.0)].into_iter(), 0.0, 0.0);
        assert_eq!(roofs.amount(5), Some(1.0));
        roofs.step_at([(5, DVec3::ZERO, 0.0)].into_iter(), 0.0, 0.0);
        assert_eq!(roofs.amount(5), Some(1.0));
    }

    #[test]
    fn the_tiles_the_field_takes_lie_beyond_the_ruts_sight() {
        // the camera going every way across the tiles' edges, a third of a metre a frame (70
        // km/h): each tile the field takes lies 96 m off or more, beyond TRACKS_SEEN by the
        // 6 m the row it is in takes to be drawn, and those in sight are all still held
        for k in 0..72 {
            let dir = DVec2::from_angle(k as f64 * 5.0_f64.to_radians());
            let mut eye = DVec2::new(1000.3, -2000.7);
            let mut held: std::collections::HashSet<(i64, i64)> = std::collections::HashSet::new();
            for step in 0..2000 {
                let (xs, ys) = field_tiles(eye);
                let now: std::collections::HashSet<(i64, i64)> = ys.flat_map(|y| xs.clone().map(move |x| (x, y))).collect();
                if step > 0 {
                    for &t in now.difference(&held) {
                        assert!(tile_off(t, eye) >= 95.6, "dir {k}: tile {t:?} taken {:.2} m off", tile_off(t, eye));
                    }
                }
                let seen = |t: (i64, i64)| tile_off(t, eye) < TRACKS_SEEN;
                let (bx, by) = ((eye.x / TILE_M).floor() as i64, (eye.y / TILE_M).floor() as i64);
                for y in by - 4..=by + 4 {
                    for x in bx - 4..=bx + 4 {
                        assert!(!seen((x, y)) || now.contains(&(x, y)), "dir {k}: tile ({x}, {y}) in sight and not held");
                    }
                }
                held = now;
                eye += dir * 0.33;
            }
        }
    }

    #[test]
    fn the_field_wraps_as_the_shader_reads_it() {
        // a texel's place in the GPU field is its world index modulo the field's size,
        // whichever tile of the wrap it is in
        for (gx, gy) in [(0i64, 0i64), (255, 256), (-1, -1), (2047, 2048), (-2049, 12345)] {
            let (tx, ty) = (gx.div_euclid(TILE_PX as i64), gy.div_euclid(TILE_PX as i64));
            let i = slot_of(tx, ty);
            let (sx, sy) = (i % SLOTS, i / SLOTS);
            let x = sx * TILE_PX + gx.rem_euclid(TILE_PX as i64) as usize;
            let y = sy * TILE_PX + gy.rem_euclid(TILE_PX as i64) as usize;
            let n = SNOW_TRACK_TEXELS as i64;
            assert_eq!((x as i64, y as i64), (gx.rem_euclid(n), gy.rem_euclid(n)));
        }
    }

    #[test]
    fn a_roof_gathers_snow_and_the_airstream_takes_it() {
        let mut r = RoofSnow::new(0.0, 0.0, 0.5, DVec3::ZERO);
        // standing in the snow: 700 thousandths fallen close it
        r.step(350.0, 0.5, DVec3::ZERO, 0.0);
        assert!((r.amount - 0.5).abs() < 1e-4, "{}", r.amount);
        // slow through town: it stays
        r.step(350.0, 0.5, DVec3::new(30.0, 0.0, 0.0), 5.0);
        assert!((r.amount - 0.5).abs() < 1e-4);
        // 50 km/h for 120 m: a good part of it goes
        let mut x = 30.0;
        for _ in 0..12 {
            x += 10.0;
            r.step(350.0, 0.5, DVec3::new(x, 0.0, 0.0), 13.9);
        }
        assert!(r.amount < 0.25 && r.amount > 0.0, "{}", r.amount);
        // the thaw takes the rest with the roads' snow
        r.step(350.0, 0.0, DVec3::new(x, 0.0, 0.0), 0.0);
        assert_eq!(r.amount, 0.0);
    }

    #[test]
    fn the_setting_and_the_flag_switch_the_built_up_snow() {
        // (no flag: the player's setting decides)
        assert!(switched_on_by(None, true));
        assert!(!switched_on_by(None, false));
        // (a set flag decides both ways: off is off whatever the setting, a cover to start
        // with - the renders' A/B - is on even with the setting off)
        for off in ["off", "0ff", "false", " off "] {
            assert!(!switched_on_by(Some(off), true) && !switched_on_by(Some(off), false), "{off}");
        }
        assert!(switched_on_by(Some("0.5"), false) && switched_on_by(Some("on"), false));
    }

    #[test]
    fn switched_off_it_starts_again_from_the_weather() {
        let mut w = omsi_content::weather::Weather::default();
        w.temp.0 = -3.0;
        w.precip = vec![2.0, 255.0];
        let mut snow = RoadSnow::default();
        for _ in 0..60 {
            snow.step(1.0, &w);
        }
        snow.roofs.weather = Some(0.0);
        snow.roofs.step_at([(1, DVec3::ZERO, 0.0)].into_iter(), snow.cover, snow.fallen);
        assert!(snow.cover > 0.0 && snow.roofs.amount(1).is_some());
        let mut state = Some((snow, SnowTracks::default()));
        switched_off(&mut state);
        assert!(state.is_none(), "the snow built up and its roofs are let go");
        // (switched on again, `frame` starts a state that takes the weather's snow on road
        // as the roads' cover, and keeps no roof yet)
        let mut again = RoadSnow::default();
        w.snow_on_road = true;
        again.step(0.0, &w);
        assert_eq!(again.cover, 1.0);
        assert_eq!(again.roofs.amount(1), None);
        // (a state that never started has nothing to let go)
        let mut fresh = Some((RoadSnow::default(), SnowTracks::default()));
        switched_off(&mut fresh);
        assert!(fresh.is_some());
    }

    /// The fallback: with the setting off the picture draws no built-up cover and the snowy
    /// roads come from the season's WinterSnowfall textures again, as before; on again (where
    /// the device draws it), the picture takes the cover and the roads the plain WinterSnow.
    #[test]
    fn switched_off_the_roads_take_the_weathers_snow_again() {
        // (a set flag decides instead of the setting)
        if omsi_cfg::flags::OMSI_ROAD_SNOW.var().is_some() {
            return;
        }
        let mut snow = RoadSnow::default();
        snow.started = true;
        snow.cover = 0.5;
        *STATE.lock() = Some((snow, SnowTracks::default()));
        let mut off = omsi_render::Lighting::default();
        light(&mut off, false);
        assert!(!enabled());
        assert_eq!(off.road_snow, None);
        assert_eq!(snow_folder(true), "WinterSnowfall");
        assert_eq!(snow_folder(false), "WinterSnow");
        let mut on = omsi_render::Lighting::default();
        light(&mut on, true);
        if omsi_render::road_snow_supported() {
            assert!(enabled());
            assert_eq!(on.road_snow, Some(0.5));
            assert_eq!(snow_folder(true), "WinterSnow");
        }
        *STATE.lock() = None;
    }

    #[test]
    fn snow_settles_while_it_falls_and_thaws_in_the_warm() {
        let mut w = omsi_content::weather::Weather::default();
        w.temp.0 = -3.0;
        w.precip = vec![2.0, 255.0];
        let mut r = RoadSnow::default();
        r.step(0.0, &w);
        assert_eq!(r.cover, 0.0);
        assert!(r.falling);
        for _ in 0..300 {
            r.step(1.0, &w);
        }
        // five minutes of the heaviest snow: half a cover, and that much fallen
        assert!((r.cover - 0.5).abs() < 0.01, "{}", r.cover);
        assert!((r.fallen - 500.0).abs() < 10.0, "{}", r.fallen);
        // rain on it: it goes again, and what fell is still counted
        w.temp.0 = 4.0;
        w.precip = vec![1.0, 255.0];
        for _ in 0..600 {
            r.step(1.0, &w);
        }
        assert_eq!(r.cover, 0.0);
        assert!(!r.falling);
        assert!((r.fallen - 500.0).abs() < 10.0);
    }
}
