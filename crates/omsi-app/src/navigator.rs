//! The route navigator: a small tilted 3D map in a corner of the screen, after the Route
//! Advisor of Euro Truck Simulator 2, made for a bus driver's duty.
//!
//! * The map turns with the bus and zooms out with speed; the camera looks over the bus
//!   from behind and above, so the road ahead fills the picture.
//! * Roads are drawn from the traffic network's lanes (dark casing, grey surface); the
//!   trip's route from the first stop to the last lies on them with arrows, coloured by
//!   how busy each stretch is (blue empty, green, yellow, red, dark red jammed), and its
//!   stops as markers (the last is a chequered flag).
//! * Leaving the route is noticed after a moment: a way back is searched on the lanes
//!   (Dijkstra, towards any lane of the route still ahead) and drawn instead - the route
//!   is recalculated as often as the driver goes wrong.
//! * Traffic: every AI vehicle near the bus is on the map, and roads whose cars crawl or
//!   stand are tinted amber or deep red (smoothed over seconds, so one car at a red light
//!   is no jam); the route itself takes the colour where it runs into one, and the header
//!   says how long it costs.
//! * The header: speed, the line, passengers aboard, day and time; the next stop with its
//!   distance and whether the bus is early or late; the speed limit sign and signals ahead
//!   on the map.
//!
//! Everything is drawn with `omsi-ui` into a texture of its own (4x MSAA) that the game
//! shows as a premultiplied overlay. Roads are built once per area and kept on the GPU;
//! their width is in metres near the camera and in pixels far away, so zooming rebuilds
//! nothing.

use glam::{DMat3, DVec2, DVec3, Mat4, Vec2, Vec3};
use hashbrown::HashMap;
use omsi_render::{Renderer, Scene, TextureId};
use omsi_sim::traffic::{LaneKey, LaneKind, Network};
use omsi_ui::paint::Align;
use omsi_ui::{Atlas, Color, Draw, Fonts, Gpu, Layer, Painter, Rect, Weight};

use crate::traffic::Traffic;

// --- colours (sRGB) -------------------------------------------------------------------

// neutral dark, half transparent, calm
const NAV_REDRAW_S: f32 = 1.0 / 30.0;
/// Frames between two drawings of the navigator at least. Below 30 fps it was drawn every
/// frame, and a drawing is a pass of its own (4x MSAA, its own submit): on OpenGL, where
/// wgpu makes the GL calls in the submit, 4.2 ms of a 43 ms frame (a Radeon R7 200, 23
/// fps), against 0.6 ms on Vulkan. Every other frame halves that where frames are slow.
const NAV_REDRAW_FRAMES: u32 = 2;
// (the launcher's graphite surface, a little see-through over the cab; no edge round it)
const PANEL: Color = Color::rgba(28, 28, 28, 0.88);
// (the bars under the texts: the panel's own grey a step darker, which also keeps whatever the
// opacity setting leaves of the panel readable - at a third the cab showed through behind the
// next stop)
const BAR: Color = Color::rgba(18, 18, 18, 0.55);
/// The plates over the map (the next turn, the street): the launcher's raised surface.
const PLATE: Color = Color::rgba(37, 37, 37, 0.92);
// (the launcher's map picture draws with these too, so the two maps cannot look apart)
pub(crate) const ROAD_CASING: Color = Color::rgba(30, 30, 30, 0.9);
pub(crate) const ROAD: Color = Color::rgba(92, 92, 92, 1.0);
pub(crate) const ROAD_MAIN: Color = Color::rgba(112, 112, 112, 1.0);
pub(crate) const ROUTE: Color = Color::rgba(214, 48, 40, 1.0);
const DOT: Color = Color::rgba(70, 140, 255, 1.0);
/// The public transport's dots: trolleybuses, buses, trams, and the text on their line tags.
const TROLLEY: Color = Color::rgba(46, 184, 92, 1.0);
const BUS: Color = Color::rgba(226, 58, 52, 1.0);
const TRAM: Color = Color::rgba(240, 190, 30, 1.0);
const LINE_TEXT: Color = Color::rgba(15, 15, 15, 1.0);
/// The other players of a LAN session or server: an arrow the way they face, with their name.
const PLAYER: Color = Color::rgba(190, 96, 255, 1.0);

/// What a traffic vehicle is on the map: a trolleybus (its model has trolley poles to
/// raise, `cp_SHTANGALEV` or `shtanga_lev_rot`), a tram, a bus (one on a timetable or
/// showing a line), or any other car; and the line it shows, if it is public transport.
fn traffic_kind(c: &crate::traffic::AiCar) -> (Color, Option<String>) {
    let v = &c.vehicle;
    let line = v
        .ty
        .program
        .str_var("SetLineTo")
        .and_then(|i| v.state.str_vars.get(i as usize))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "0");
    let trolley = v.var("cp_SHTANGALEV").is_some() || v.var("shtanga_lev_rot").is_some();
    let color = if c.is_rail() {
        TRAM
    } else if trolley {
        TROLLEY
    } else if c.bus.is_some() || line.is_some() {
        BUS
    } else {
        return (DOT, None);
    };
    (color, line)
}
/// A bus, trolleybus or tram on the map as a pictogram, not a dot: a body with a cut front and
/// a windscreen, turned to the way it heads (`heading`: degrees clockwise from north), a dark
/// edge round it. `min_px` keeps it readable from far away; the line number is drawn beside it.
fn vehicle_icon(p: &mut Painter, at: Vec3, heading: f64, color: Color, long: bool, min_px: f32) {
    let h = heading.to_radians() as f32;
    let (sn, cs) = h.sin_cos();
    // local: x to the right, y forward; turned onto the world (x east, y north)
    let turn = |v: Vec2| Vec2::new(v.x * cs + v.y * sn, -v.x * sn + v.y * cs);
    let l = if long { 1.5 } else { 1.0 };
    let body = |k: f32| -> Vec<Vec2> {
        [(-0.34, -1.0), (0.34, -1.0), (0.34, 0.55), (0.2, 1.0), (-0.2, 1.0), (-0.34, 0.55)].iter().map(|&(x, y)| turn(Vec2::new(x * k, y * l * k))).collect()
    };
    let (m, px) = (6.0, min_px);
    p.world_shape(at, &body(1.28), m, px * 1.28, Color::rgba(8, 8, 8, 0.92));
    p.world_shape(at, &body(1.0), m, px, color);
    let glass: Vec<Vec2> = [(-0.24, 0.5), (0.24, 0.5), (0.15, 0.8), (-0.15, 0.8)].iter().map(|&(x, y)| turn(Vec2::new(x, y * l))).collect();
    p.world_shape(at, &glass, m, px, Color::rgba(240, 244, 248, 0.9));
}

/// The route by how busy its roads are: empty, light, busy, heavy, jammed.
const LEVEL: [Color; 5] = [
    Color::rgba(46, 116, 240, 1.0),
    Color::rgba(56, 178, 86, 1.0),
    Color::rgba(236, 192, 40, 1.0),
    Color::rgba(224, 56, 44, 1.0),
    Color::rgba(122, 16, 22, 1.0),
];
/// The arrows on it, in a colour that stands out from each.
const ARROW: [Color; 5] = [
    Color::rgba(236, 244, 255, 1.0),
    Color::rgba(12, 66, 28, 1.0),
    Color::rgba(92, 58, 0, 1.0),
    Color::rgba(150, 240, 150, 1.0),
    Color::rgba(255, 206, 80, 1.0),
];
/// The part of the route already driven (city map).
const DRIVEN: Color = Color::rgba(62, 70, 86, 1.0);
const STREET: Color = Color::rgba(178, 178, 178, 1.0);

/// How busy a road is (0 empty … 4 jammed) from its congestion score.
fn level(score: f32) -> usize {
    match score {
        s if s < 0.12 => 0,
        s if s < 0.40 => 1,
        s if s < 0.60 => 2,
        s if s < 0.80 => 3,
        _ => 4,
    }
}
const TEXT: Color = Color::rgba(237, 237, 237, 1.0);
// (the second texts - units, the day, the times - bright enough to read on a lit cab)
const TEXT_DIM: Color = Color::rgba(184, 184, 184, 1.0);
const LATE: Color = Color::rgba(235, 85, 70, 1.0);
const EARLY: Color = Color::rgba(90, 160, 240, 1.0);
const ON_TIME: Color = Color::rgba(110, 200, 120, 1.0);
const WARN: Color = Color::rgba(235, 170, 60, 1.0);
const STOP_REQUEST: Color = Color::hex(0xF0A030);

pub(crate) fn stop_requested(vehicle: &omsi_sim::vehicle::VehicleInstance) -> bool {
    ["haltewunsch", "haltewunschlampe"].iter().any(|name| vehicle.var(name).is_some_and(|v| v > 0.5))
}

fn stop_request_icon(ui: &mut Painter, atlas: &mut Atlas, requested: bool, mut row: Rect, scale: f32) -> Rect {
    if requested {
        let size = 32.0 * scale;
        ui.icon(atlas, "stop_request", Vec2::new(row.right() - size * 0.5, row.center().y), size, STOP_REQUEST);
        row.w -= size + 6.0 * scale;
    }
    row
}

/// Vertical field of view and tilt of the map camera (degrees).
const FOV: f32 = 40.0;
const PITCH: f64 = 52.0;
/// Roads are built for this far around the bus (m) and again when it has gone half-way.
const ROAD_RADIUS: f64 = 1300.0;
/// Off the route for this long (s) before a way back is looked for, and between tries.
const OFF_ROUTE_AFTER: f32 = 2.0;
const REROUTE_EVERY: f32 = 2.5;

/// One stop of the trip for the map.
#[derive(Debug, Clone)]
pub struct NavStop {
    /// The stop's map object (its place is looked up in the navigator's map when the
    /// timetable does not know it: its tile is not loaded yet).
    pub object_id: i64,
    pub position: DVec3,
    pub name: String,
    /// Planned arrival (s of the day).
    pub arrival: f64,
}

/// Another player of the session as the maps show them (the session's, see `lan`).
pub use omsi_lan::game::NavPlayer;

/// What the navigator is told each frame.
pub struct NavFrame<'a> {
    pub traffic: Option<&'a Traffic>,
    /// The other players of a LAN session or server, on both maps (#1011, #1080).
    pub players: Vec<NavPlayer>,
    pub bus: DVec3,
    /// Compass heading (degrees, 0 = +y, clockwise).
    pub heading: f64,
    pub speed_kmh: f32,
    /// Outside weather and cabin air temperatures (°C).
    pub outside_temp: f32,
    pub inside_temp: f32,
    /// Line, terminus, and the trip's stops from the next one on (the next is first).
    pub line: Option<String>,
    pub terminus: Option<String>,
    pub stops: Vec<NavStop>,
    /// How late the bus is (s, negative early), when on a duty.
    pub delay: Option<f64>,
    pub passengers: Option<usize>,
    /// The vehicle script's latched stop request or illuminated request lamp.
    pub stop_requested: bool,
    /// Seconds of the day and weekday (0 = Monday).
    pub time: f64,
    pub weekday: i32,
    pub language: &'a str,
    /// Window size in physical pixels.
    pub screen: (f32, f32),
    /// The player's interface size (`Settings::ui_scale`): the panel and the city map's
    /// texts and buttons grow with it.
    pub ui_scale: f32,
    /// The interface grows with a tall window (`Settings::ui_scale_window`); off, the panel
    /// is held to 480 px, as before.
    pub follow_window: bool,
    pub dt: f32,
    /// Where the information bar is (`ui::Ui::info_rect`): a navigator along the top keeps
    /// below it.
    pub info_rect: Option<[f32; 4]>,
    /// The on-screen controls' row along the top (`touch::Touch::top_gap`): the gap between
    /// its buttons, left and right, and the buttons' bottom.
    pub touch_gap: Option<[f32; 3]>,
}

/// The texts, per language.
struct Words {
    kmh: &'static str,
    days: [&'static str; 7],
    off_route: &'static str,
    rerouting: &'static str,
    recalculated: &'static str,
    jam: &'static str,
    slow: &'static str,
    no_duty: &'static str,
    last_stop: &'static str,
    on_time: &'static str,
}

fn words(lang: &str) -> Words {
    match lang.to_ascii_uppercase().as_str() {
        "DEU" | "DE" | "GER" => Words { kmh: "km/h", days: ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"], off_route: "Abseits der Route", rerouting: "Route wird neu berechnet", recalculated: "Route neu berechnet", jam: "Stau", slow: "Zähfließend", no_duty: "Freie Fahrt", last_stop: "Endhaltestelle", on_time: "pünktlich" },
        "FRA" | "FR" => Words { kmh: "km/h", days: ["Lun", "Mar", "Mer", "Jeu", "Ven", "Sam", "Dim"], off_route: "Hors itinéraire", rerouting: "Recalcul de l'itinéraire", recalculated: "Itinéraire recalculé", jam: "Bouchon", slow: "Ralentissement", no_duty: "Conduite libre", last_stop: "Terminus", on_time: "à l'heure" },
        "RUS" | "RU" => Words { kmh: "км/ч", days: ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"], off_route: "Вне маршрута", rerouting: "Перестроение маршрута", recalculated: "Маршрут перестроен", jam: "Пробка", slow: "Затруднено", no_duty: "Свободная езда", last_stop: "Конечная", on_time: "по графику" },
        _ => Words { kmh: "km/h", days: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"], off_route: "Off route", rerouting: "Recalculating route", recalculated: "Route recalculated", jam: "Traffic jam", slow: "Slow traffic", no_duty: "Free drive", last_stop: "Final stop", on_time: "on time" },
    }
}

/// The route being followed: the lanes, how far along the bus is, and whether it was
/// replaced by a way back after leaving it.
#[derive(Default)]
struct Route {
    key: String,
    lanes: Vec<usize>,
    complete: bool,
    generation: u64,
    /// Index into `lanes` of the lane the bus is on, and how far along it.
    progress: usize,
    s: f32,
    on_route: bool,
    off_for: f32,
    retry_in: f32,
    /// Seconds the "recalculated" note still shows.
    note: f32,
    /// Bumped whenever `lanes` or `progress` change (the cached route mesh).
    version: u64,
    /// `lanes` is a way to the next stop found by the navigator itself (the trip's own
    /// route is not there yet: its tiles still load); the trip's replaces it.
    provisional: bool,
    /// The bus has been on the route: before that, the way to it is simply the way to the
    /// next stop (found at once, and no "recalculating").
    joined: bool,
    /// `lanes` begins with a way to the route (the bus is looked for on it too).
    approach: bool,
}

/// Roads around a centre, built into vertex buffer 0.
struct Roads {
    light: bool,
    anchor: DVec2,
    lanes_seen: usize,
    verts: usize,
    built_at: f32,
}

pub struct Navigator {
    /// The current small-map texture in Scene::overlays, for the cockpit display.
    pub panel_overlay: Option<usize>,
    pub cockpit_display: bool,
    /// The speed limit (km/h) the top bar showed last, for the information bar (#2476).
    pub(crate) speed_limit: Option<f32>,
    drawn_at: f32,
    /// Frames since the last drawing (see `NAV_REDRAW_FRAMES`).
    frames_since_drawn: u32,
    pub enabled: bool,
    /// The next stops with their times under the map (Shift+N cycles map, map and
    /// schedule, off).
    pub schedule: bool,
    /// Smoothed speed (m/s) for the time to the next stop.
    speed_avg: f32,
    pub opacity: f32,
    pub corner: String,
    /// Where the navigator was dragged to (#940): its top-left corner as a share of the
    /// room the window leaves it across and down (`navigator_corner = at x,y`); None: the
    /// corner.
    pub at: Option<[f32; 2]>,
    /// A press on the navigator: where it was (cursor and the panel's top-left) and whether
    /// it has moved far enough to be a drag rather than a click.
    panel_drag: Option<([f32; 2], [f32; 2], bool)>,
    /// The room the window leaves the navigator (width, height) when it was last placed.
    panel_room: [f32; 2],
    /// The city map (a click on the navigator or Shift+M) and where the navigator is on the
    /// screen.
    pub city: CityMap,
    panel_rect: [f32; 4],
    origin_x: f32,
    gpu: Option<Gpu>,
    fonts: Fonts,
    atlas: Atlas,
    target: Option<(TextureId, u32, u32)>,
    /// The map's own lanes when there is no traffic system.
    own_net: Option<std::sync::Arc<Network>>,
    /// The whole map's road network and object places (read in the background at the
    /// start, see `World::navigation_map`): routes, roads and stops beyond the loaded
    /// tiles. Its version changes when it arrives.
    global: Option<std::sync::Arc<Network>>,
    stop_pos: std::sync::Arc<HashMap<i64, DVec3>>,
    /// Street names of the map's lanes (from its street name signs).
    streets: Option<std::sync::Arc<Streets>>,
    #[allow(clippy::type_complexity)]
    building: Option<std::sync::mpsc::Receiver<(Network, HashMap<i64, DVec3>, Streets, Option<omsi_map::package::Package>)>>,
    pub global_version: u64,
    roads: Option<Roads>,
    route: Route,
    /// The route mesh (buffer 1): the route and traffic versions and the anchor it was
    /// built for, and its size.
    route_mesh: (u64, u64, DVec2, u32, usize),
    /// Per lane of the traffic: how congested it is (0 free … 1 standing), smoothed.
    congestion: HashMap<usize, f32>,
    /// The same for the route's lanes ahead (in the route's network), and a version that
    /// changes when any of their levels does (the route is drawn again).
    route_jam: HashMap<usize, f32>,
    jam_version: u64,
    congestion_t: f32,
    /// Map camera: distance, heading, both smoothed.
    zoom: f64,
    cam_heading: f64,
    time: f32,
    /// Distance along the route to the next stop (m), refreshed now and then.
    next_dist: Option<f64>,
    dist_t: f32,
    /// The next turn on the route: direction (-1 left, 1 right, 2 back), how sharp
    /// (degrees), how far (m) and the street it turns into.
    /// OMSI 2's route arrows are wanted (see `turn_hint`).
    pub arrows: bool,
    /// The other (AI) vehicles are drawn on the maps (the `nav_ai` setting).
    pub show_ai: bool,
    /// The map's own surface (`Maps/<Map>/openomsi-realmap`), once read, and a number that changes
    /// when it arrives (the meshes are built again).
    surface: Option<std::sync::Arc<omsi_map::package::Package>>,
    surface_version: u64,
    /// The surface is drawn (the `nav_surface` setting).
    pub show_surface: bool,
    /// The light look of the maps (the `nav_light` setting).
    pub light: bool,
    /// The small map's surface mesh (buffer 6): the surface version and anchor it was built for, and its size.
    surface_mesh: Option<(u64, DVec2, u32)>,
    /// What the package names: icons, names, where the bus is.
    place: place::PlaceView,
    /// How far the panel has faded in (Shift+N fades it in and out rather than cutting).
    shown: f32,
    /// The trip's next stops (place, name, the bus's heading there) and where the bus is,
    /// for the route arrows.
    stop_spots: Vec<(DVec3, String, f64, i64)>,
    bus_at: DVec3,
    next_turn: Option<(i32, f32, f64, Option<String>)>,
    /// The street the bus is on.
    street_here: Option<String>,
    /// Seconds of delay the jams ahead on the route cost.
    jam_cost: f32,
    first: bool,
}

/// The duty as the navigator shows it: line, terminus, the stops from the next one on,
/// and the current trip (a key that changes with it, and its name).
#[allow(clippy::type_complexity)]
pub fn duty_parts(duty: Option<&crate::schedule::PlayerDuty>) -> (Option<String>, Option<String>, Vec<NavStop>, Option<(String, String)>) {
    let Some(d) = duty else { return (None, None, Vec::new(), None) };
    let Some(trip) = d.trips.get(d.trip_index) else { return (None, None, Vec::new(), None) };
    let line = if trip.line.trim().is_empty() { d.line.trim() } else { trip.line.trim() };
    let stops = trip
        .stops
        .iter()
        .skip(d.next_stop)
        .filter(|s| s.stops)
        .map(|s| NavStop { object_id: s.object_id, position: s.position.unwrap_or(DVec3::ZERO), name: s.name.clone(), arrival: s.arr })
        .collect();
    (Some(line.to_string()), Some(trip.terminus.clone()), stops, Some((format!("{}/{}", d.trip_index, trip.name), trip.name.clone())))
}

/// Screen position of a world point (relative to the anchor) in target pixels.
fn project(vp: Mat4, viewport: [f32; 4], p: Vec3) -> Option<Vec2> {
    let c = vp * p.extend(1.0);
    if c.w <= 0.1 {
        return None;
    }
    let n = c.truncate() / c.w;
    Some(Vec2::new(viewport[0] + (n.x * 0.5 + 0.5) * viewport[2], viewport[1] + (0.5 - n.y * 0.5) * viewport[3]))
}

fn angle_diff(a: f64, b: f64) -> f64 {
    let mut d = (b - a) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    d
}

fn ease(dt: f32, tau: f32) -> f64 {
    (1.0 - (-dt / tau.max(1e-3)).exp()) as f64
}

/// A `navigator_corner` of the form `at x,y` (where the navigator was dragged to, #940).
pub(crate) fn placed_at(corner: &str) -> Option<[f32; 2]> {
    let (x, y) = corner.trim().strip_prefix("at")?.trim().split_once(',')?;
    let (x, y) = (x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?);
    (x.is_finite() && y.is_finite()).then(|| [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)])
}

/// The GPS is a separate render texture composed after the scene's post AA.
fn map_samples(format: wgpu::TextureFormat) -> u32 {
    if format.guaranteed_format_features(wgpu::Features::empty()).flags.sample_count_supported(4) { 4 } else { 1 }
}

impl Navigator {
    pub fn new(enabled: bool, opacity: f32, corner: &str) -> Navigator {
        Navigator {
            panel_overlay: None,
            cockpit_display: false,
            speed_limit: None,
            drawn_at: f32::MIN,
            frames_since_drawn: u32::MAX,
            enabled,
            schedule: false,
            speed_avg: 8.0,
            opacity: opacity.clamp(0.2, 1.0),
            corner: corner.to_string(),
            at: placed_at(corner),
            panel_drag: None,
            panel_room: [0.0; 2],
            city: CityMap::default(),
            panel_rect: [0.0; 4],
            origin_x: 0.0,
            gpu: None,
            fonts: Fonts::new(),
            atlas: Atlas::new(1024),
            target: None,
            own_net: None,
            global: None,
            stop_pos: Default::default(),
            streets: None,
            building: None,
            global_version: 0,
            roads: None,
            route: Route::default(),
            route_mesh: (u64::MAX, 0, DVec2::ZERO, 0, 0),
            congestion: HashMap::new(),
            route_jam: HashMap::new(),
            jam_version: 0,
            congestion_t: 0.0,
            zoom: 120.0,
            cam_heading: 0.0,
            time: 0.0,
            next_dist: None,
            arrows: false,
            show_ai: true,
            surface: None,
            surface_version: 0,
            show_surface: true,
            light: false,
            surface_mesh: None,
            place: Default::default(),
            shown: if enabled { 1.0 } else { 0.0 },
            stop_spots: Vec::new(),
            bus_at: DVec3::ZERO,
            next_turn: None,
            street_here: None,
            dist_t: 0.0,
            jam_cost: 0.0,
            first: true,
        }
    }

    /// Read the whole map's road network on a worker (once per session).
    pub fn start_map(&mut self, world: std::sync::Arc<crate::scene::World>) {
        if self.building.is_some() || self.global.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("navigator map".into())
            .spawn(move || {
                // (the map's package: Maps/<Map>/openomsi-realmap, if it has one)
                let surface = omsi_map::package::Package::open(&world.map_dir);
                let m = world.navigation_map();
                let mut net = Network { lanes: m.lanes, ..Default::default() };
                net.link(1.5);
                confirm_road_surfaces(&mut net, &m.road_surfaces);
                probe_lanes(&net);
                let streets = build_streets(&net, &m.signs);
                let _ = tx.send((net, m.places, streets, surface));
            })
            .ok();
        self.building = Some(rx);
    }

    /// The map's network given at once (an offscreen picture reads it on its own thread).
    pub fn set_map(&mut self, map: crate::scene::NavigationMap) {
        let mut net = Network { lanes: map.lanes, ..Default::default() };
        net.link(1.5);
        confirm_road_surfaces(&mut net, &map.road_surfaces);
        probe_lanes(&net);
        self.streets = Some(std::sync::Arc::new(build_streets(&net, &map.signs)));
        let global = std::sync::Arc::new(net);
        self.global = Some(global);
        self.stop_pos = std::sync::Arc::new(map.places);
        self.global_version += 1;
    }

    /// The places of every object on the map (stops beyond the loaded tiles), once read.
    pub fn places(&self) -> Option<&HashMap<i64, DVec3>> {
        self.global.as_ref().map(|_| &*self.stop_pos)
    }

    /// The whole map's network, once read.
    pub fn map_net(&self) -> Option<&Network> {
        self.global.as_deref()
    }

    /// The map's lanes, for a session without a traffic system (added as tiles stream in).
    pub fn add_lanes(&mut self, lanes: Vec<omsi_sim::traffic::Lane>) {
        if lanes.is_empty() {
            return;
        }
        match self.own_net.as_mut() {
            Some(n) => {
                // (only `draw` borrows it, for the length of a frame)
                if let Some(n) = std::sync::Arc::get_mut(n) {
                    n.extend(lanes, omsi_sim::traffic::LINK_TOLERANCE);
                    n.build_grid();
                }
            }
            None => {
                let mut n = Network { lanes, ..Default::default() };
                n.link(omsi_sim::traffic::LINK_TOLERANCE);
                n.build_grid();
                self.own_net = Some(std::sync::Arc::new(n));
            }
        }
    }

    /// Does the route of trip `key` need (re)building? (a new trip, or tiles brought the
    /// lanes a partial route was missing)
    pub fn wants_route(&self, key: &str, generation: u64) -> bool {
        if self.global.is_some() {
            return self.route.key != key || self.route.generation != self.global_version + (1 << 40);
        }
        self.route.key != key || (!self.route.complete && self.route.generation != generation)
    }

    /// The route of trip `key`: its lanes as the timetable drives them.
    pub fn set_route(&mut self, key: &str, lanes: Vec<usize>, complete: bool, generation: u64) {
        let same_trip = self.route.key == key;
        self.route.key = key.to_string();
        self.route.complete = complete;
        self.route.generation = generation;
        if same_trip && !self.route.lanes.is_empty() && !self.route.on_route && !self.route.provisional {
            // a partial route grew while the driver was off it: keep the way back
            return;
        }
        if self.route.provisional && lanes.is_empty() {
            return;
        }
        self.route.provisional = false;
        self.route.lanes = lanes;
        if !same_trip {
            self.route.progress = 0;
            self.route.on_route = false;
            self.route.off_for = 0.0;
        }
        self.route.version += 1;
    }

    /// No duty: nothing to follow.
    pub fn clear_route(&mut self) {
        if !self.route.key.is_empty() || !self.route.lanes.is_empty() {
            self.route = Route { version: self.route.version + 1, ..Route::default() };
        }
    }


    /// Where the bus is along the route, and a way back when it left it.
    fn follow(&mut self, f: &NavFrame) {
        let global = self.global.clone();
        let Some(net) = global.as_deref().or(f.traffic.map(|t| &t.net)) else { return };
        let r = &mut self.route;
        if r.lanes.is_empty() {
            // no route (yet): the way to the next stop, looked for now and then
            let Some(stop) = f.stops.first() else { return };
            r.retry_in -= f.dt;
            if r.retry_in > 0.0 {
                return;
            }
            r.retry_in = REROUTE_EVERY * 2.0;
            let targets: Vec<usize> = lanes_near(net, stop.position.truncate(), 40.0)
                .into_iter()
                .filter(|&i| net.lanes[i].kind == LaneKind::Street && net.lanes[i].nearest_point(stop.position).map(|p| p.1 < 20.0).unwrap_or(false))
                .collect();
            if let Some((mut path, k)) = way_back(net, f.bus, f.heading, &targets, 30_000.0) {
                path.push(targets[k]);
                log::info!("navigator: no route of the trip here yet; {} lanes to the next stop '{}'", path.len(), stop.name.trim());
                r.lanes = path;
                r.progress = 0;
                r.s = 0.0;
                r.version += 1;
                r.provisional = true;
            }
            return;
        }
        r.note = (r.note - f.dt).max(0.0);
        // where the next stop is on the route: the way to the route must not skip it
        let stop_at = f.stops.first().and_then(|st| {
            (r.progress..r.lanes.len().min(r.progress + 1500)).find(|&k| {
                net.lanes.get(r.lanes[k]).and_then(|l| l.nearest_point(st.position)).map(|p| p.1 < 25.0).unwrap_or(false)
            })
        });
        // the nearest route lane near the bus that runs its way, a little back to a lot ahead
        // (before the bus first reached the route: only the stretch up to the next stop)
        let (from, to) = match (r.joined, stop_at) {
            (false, Some(k)) if r.approach => (r.progress.saturating_sub(3), (k + 2).min(r.lanes.len())),
            (false, Some(k)) => (k.saturating_sub(40).max(r.progress), (k + 2).min(r.lanes.len())),
            _ => (r.progress.saturating_sub(3), (r.progress + 60).min(r.lanes.len())),
        };
        let mut best: Option<(usize, f32, f64)> = None;
        for k in from..to {
            let Some(l) = net.lanes.get(r.lanes[k]) else { continue };
            let Some((s, d)) = l.nearest_point(f.bus) else { continue };
            // (a wide road, a stop bay, a lane change or a turn in progress: the bus is still
            // on its route well beyond the lane's middle; at 11 m and 75 deg it counted as off
            // and the navigator recalculated for ever)
            if d > 16.0 {
                continue;
            }
            let (_, h) = l.at(s);
            if angle_diff(f.heading, h as f64).abs() > 100.0 {
                continue;
            }
            // (ahead of where the bus was preferred: a route crossing itself)
            let score = d + if k < r.progress { 4.0 } else { 0.0 } + (k.saturating_sub(r.progress) as f64) * 0.05;
            if best.map(|b| score < b.2).unwrap_or(true) {
                best = Some((k, s, score));
            }
        }
        match best {
            Some((k, s, _)) => {
                if k != r.progress {
                    r.version += 1;
                }
                r.progress = k;
                r.s = s;
                r.on_route = true;
                r.joined = true;
                r.off_for = 0.0;
            }
            None => {
                r.on_route = false;
                r.off_for += f.dt;
            }
        }
        // the way to the route: at once before the bus first reached it (from the depot or
        // wherever it starts), after a moment off it once driving it
        if r.on_route || (r.joined && r.off_for < OFF_ROUTE_AFTER) {
            return;
        }
        r.retry_in -= f.dt;
        if r.retry_in > 0.0 {
            return;
        }
        r.retry_in = REROUTE_EVERY;
        let base = r.progress.min(r.lanes.len() - 1);
        // targets: the route from where the bus left it up to the next stop (joining it
        // later would skip that stop), or on for 120 lanes when no stop is ahead
        let (lo, hi) = match stop_at {
            Some(k) => (k.saturating_sub(150).max(base), k + 1),
            None => (base, (base + 120).min(r.lanes.len())),
        };
        // (from the depot to the first stop may be a long way; back onto the route is not)
        let way = way_back(net, f.bus, f.heading, &r.lanes[lo..hi], if r.joined { 6000.0 } else { 30_000.0 });
        if way.is_none() && omsi_cfg::flags::OMSI_DEBUG_NAV.is_set() {
            let info: Vec<_> = r.lanes[lo..hi].iter().map(|&l| (l, net.lanes[l].kind, net.lanes[l].name.clone(), net.lanes.iter().filter(|x| x.next.contains(&l)).count(), net.lanes[l].start())).collect();
            log::info!("navigator: off the route for {:.1} s and no way back found; targets {info:?}", r.off_for);
        }
        // the route's own lanes before a stop may be cut off from the road network (the
        // map links nothing into the start of line 31's trip at Maulbeerallee): then a
        // road past the stop running the route's way, else the route just after the stop
        let max = if r.joined { 6000.0 } else { 30_000.0 };
        let way = way.map(|(p, j)| (p, lo + j)).or_else(|| {
            let k = stop_at?;
            let st = f.stops.first()?;
            let (_, h) = net.lanes[r.lanes[k]].nearest_point(st.position).map(|(s, _)| net.lanes[r.lanes[k]].at(s))?;
            let near: Vec<usize> = lanes_near(net, st.position.truncate(), 40.0)
                .into_iter()
                .filter(|&i| {
                    let l = &net.lanes[i];
                    l.kind == LaneKind::Street
                        && l.nearest_point(st.position).map(|(s, d)| d < 20.0 && angle_diff(l.at(s).1 as f64, h as f64).abs() < 60.0).unwrap_or(false)
                })
                .collect();
            let (mut path, j) = way_back(net, f.bus, f.heading, &near, max)?;
            path.push(near[j]);
            log::info!("navigator: the route before stop '{}' cannot be reached; led onto the road past it", st.name.trim());
            Some((path, k + 1))
        })
            .or_else(|| {
                let k = stop_at?;
                let hi = (k + 120).min(r.lanes.len());
                (k + 1 < hi).then_some(())?;
                way_back(net, f.bus, f.heading, &r.lanes[k + 1..hi], max).map(|(p, j)| (p, k + 1 + j))
            });
        if let Some((path, join)) = way {
            let rest = r.lanes[join.min(r.lanes.len())..].to_vec();
            log::info!(
                "navigator: {} of {} lanes joins the route {} lanes on{}",
                if r.joined { "a way back" } else { "the way to the route" },
                path.len(),
                join,
                stop_at.map(|k| format!(" (the next stop is on route lane {k})")).unwrap_or_default()
            );
            let mut lanes = path;
            lanes.extend(rest);
            // (drawn from where the bus is on its first lane, not from that lane's start)
            r.s = lanes.first().and_then(|&l| net.lanes.get(l)).and_then(|l| l.nearest_point(f.bus)).map(|p| p.0).unwrap_or(0.0);
            r.lanes = lanes;
            r.progress = 0;
            r.version += 1;
            if r.joined {
                r.note = 4.0;
                r.on_route = true;
                r.off_for = 0.0;
            } else {
                r.approach = true;
            }
        }
    }

    /// How congested the lanes near the bus are: a lane with cars on it is light traffic,
    /// more so the fuller it is; cars crawling or standing in a row make it heavy or a jam
    /// (one car waiting at a light is not a jam). Smoothed over seconds.
    fn update_congestion(&mut self, f: &NavFrame) {
        self.congestion_t -= f.dt;
        if self.congestion_t > 0.0 {
            return;
        }
        let step = 0.25f32;
        self.congestion_t = step;
        let Some(t) = f.traffic else { return };
        let mut by_lane: HashMap<usize, (f32, u32)> = HashMap::new();
        for c in &t.cars {
            if c.gone || (c.vehicle.position - f.bus).truncate().length() > 1200.0 {
                continue;
            }
            let e = by_lane.entry(c.state.lane).or_insert((0.0, 0));
            e.0 += c.state.speed.max(0.0);
            e.1 += 1;
        }
        let k = ease(step, 4.0) as f32;
        for v in self.congestion.values_mut() {
            *v -= *v * k;
        }
        for (lane, (sum, n)) in by_lane {
            let Some(l) = t.net.lanes.get(lane) else { continue };
            let expected = (l.speed_limit_kmh.clamp(20.0, 70.0) / 3.6) * 0.75;
            let slow = (1.0 - (sum / n as f32) / expected).clamp(0.0, 1.0);
            let full = (n as f32 * 7.5 / l.length().max(25.0)).min(1.0);
            let light = 0.22 + 0.33 * full;
            let jam = slow * (n as f32 / 3.0).min(1.0);
            let score = if jam > 0.2 { light.max(0.3 + 0.7 * jam) } else { light };
            let v = self.congestion.entry(lane).or_insert(0.0);
            *v += (score - *v) * k;
        }
        self.congestion.retain(|_, v| *v > 0.03);
        // the same for the route ahead, in the route's own network
        let global = self.global.clone();
        let jam = match global.as_deref() {
            Some(g) => congestion_on(g, &t.net, &self.congestion),
            None => self.congestion.clone(),
        };
        let net = global.as_deref().unwrap_or(&t.net);
        let r = &self.route;
        let mut route_jam = HashMap::new();
        let mut cost = 0.0;
        for &l in r.lanes.iter().skip(r.progress).take(600) {
            let Some(&c) = jam.get(&l) else { continue };
            route_jam.insert(l, c);
            // the time the jams ahead cost
            if let Some(lane) = net.lanes.get(l) {
                if c > 0.6 {
                    let v_free = lane.speed_limit_kmh.clamp(20.0, 70.0) / 3.6;
                    let v = (v_free * (1.0 - c)).max(1.2);
                    cost += lane.length() / v - lane.length() / v_free;
                }
            }
        }
        self.jam_cost = cost;
        let changed = route_jam.len() != self.route_jam.len() || route_jam.iter().any(|(l, c)| self.route_jam.get(l).map(|o| level(*o) != level(*c)).unwrap_or(true));
        self.route_jam = route_jam;
        if changed {
            self.jam_version += 1;
        }
    }

    /// The first real turn on the route within 1.5 km: a lane whose heading changes by more
    /// than 35 degrees from its start to its end (a junction's curve), or a sharp bend
    /// between two lanes. Gentle curves of a road are not turns.
    fn turn_ahead(&self, net: &Network) -> Option<(i32, f32, f64, Option<String>)> {
        let r = &self.route;
        if !r.on_route {
            return None;
        }
        let mut acc = -(r.s as f64);
        let mut prev_end: Option<f32> = None;
        for (j, &l) in r.lanes.iter().enumerate().skip(r.progress) {
            let lane = net.lanes.get(l)?;
            let len = lane.length();
            if acc > 1500.0 {
                break;
            }
            let (h0, h1) = (lane.start_heading(), lane.end_heading());
            let mut d = omsi_sim::traffic::wrap_deg(h1 - h0);
            if let Some(pe) = prev_end {
                d += omsi_sim::traffic::wrap_deg(h0 - pe);
            }
            // (a short junction lane turns within a few metres; a long road curve does not count)
            if d.abs() > 35.0 && (len < 60.0 || d.abs() > 70.0) && acc + len as f64 > 0.0 {
                let dir = if d.abs() > 150.0 { 2 } else if d > 0.0 { 1 } else { -1 };
                // the street it turns into: the first named lane after the turn
                let street = r.lanes.iter().skip(j + 1).take(4).chain(std::iter::once(&l)).find_map(|&x| self.street_of(x)).map(str::to_string);
                return Some((dir, d.abs(), acc.max(0.0), street));
            }
            prev_end = Some(h1);
            acc += len as f64;
        }
        None
    }

    /// The street a lane of the route's network belongs to (the map's network only).
    fn street_of(&self, lane: usize) -> Option<&str> {
        self.global.as_ref()?;
        let st = self.streets.as_deref()?;
        let i = *st.of_lane.get(lane)?;
        st.names.get(i as usize).map(String::as_str)
    }

    /// Distance along the route to `stop` (m).
    fn route_distance(&self, net: &Network, stop: DVec3) -> Option<f64> {
        let r = &self.route;
        let mut acc = -(r.s as f64);
        for &l in r.lanes.iter().skip(r.progress) {
            let lane = net.lanes.get(l)?;
            if let Some((s, d)) = lane.nearest_point(stop) {
                if d < 25.0 && (acc + s as f64) >= -5.0 {
                    return Some((acc + s as f64).max(0.0));
                }
            }
            acc += lane.length() as f64;
            if acc > 30_000.0 {
                break;
            }
        }
        None
    }

    /// Advance, draw into the texture and put it on the screen.
    pub fn frame_at(
        &mut self,
        renderer: &Renderer,
        scene: &mut Scene,
        f: &NavFrame,
        origin_x: f32,
    ) {
        for rect in [&mut self.panel_rect, &mut self.city.rect] {
            rect[0] -= self.origin_x;
            rect[2] -= self.origin_x;
        }
        if self.origin_x != origin_x {
            self.panel_drag = None;
            self.city.drag = None;
        }
        let start = scene.overlays.len();
        self.frame(renderer, scene, f);
        crate::ui::shift_overlays(scene, start, origin_x);
        for rect in [&mut self.panel_rect, &mut self.city.rect] {
            rect[0] += origin_x;
            rect[2] += origin_x;
        }
        self.origin_x = origin_x;
    }

    pub fn frame(&mut self, renderer: &Renderer, scene: &mut Scene, f: &NavFrame) {
        self.panel_overlay = None;
        // (with the navigator off the route is still followed for OMSI 2's arrows)
        if !self.enabled && !self.city.open && !self.arrows && self.shown < 0.01 {
            return;
        }
        let target = if self.enabled { 1.0 } else { 0.0 };
        self.shown += (target - self.shown) * (1.0 - (-f.dt * 8.0).exp());
        if (self.shown - target).abs() < 0.005 {
            self.shown = target;
        }
        self.time += f.dt;
        if let Some(rx) = self.building.as_ref() {
            if let Ok((net, pos, streets, surface)) = rx.try_recv() {
                log::info!("navigator: the map's road network is there ({} lanes, {} streets named)", net.lanes.len(), streets.names.len());
                self.streets = Some(std::sync::Arc::new(streets));
                self.set_surface(surface);
                self.global = Some(std::sync::Arc::new(net));
                self.stop_pos = std::sync::Arc::new(pos);
                self.global_version += 1;
                self.building = None;
                // routes and roads again on the whole map
                self.roads = None;
                self.route = Route { version: self.route.version + 1, ..Route::default() };
                self.route_mesh.0 = u64::MAX;
                self.route_jam.clear();
            }
        }
        // stops whose tiles are not loaded: their place from the map
        if omsi_cfg::flags::OMSI_DEBUG_NAV.is_set() && self.time < 0.15 {
            log::info!("navigator: stops {:?}", f.stops.iter().map(|s| (s.name.clone(), s.object_id, s.position != DVec3::ZERO, self.stop_pos.contains_key(&s.object_id))).collect::<Vec<_>>());
        }
        let f2;
        let f = if f.stops.iter().any(|s| s.position == DVec3::ZERO) {
            f2 = NavFrame { stops: f.stops.iter().filter_map(|s| if s.position == DVec3::ZERO { self.stop_pos.get(&s.object_id).map(|p| NavStop { position: *p, ..s.clone() }) } else { Some(s.clone()) }).collect(), ..f.clone_ref() };
            &f2
        } else {
            f
        };
        self.follow(f);
        self.bus_at = f.bus;
        self.stop_spots = f.stops.iter().take(3).map(|st| (st.position, st.name.clone(), f.heading, st.object_id)).collect();
        if omsi_cfg::flags::OMSI_DEBUG_NAV.is_set() && (self.time % 1.0) < f.dt {
            log::info!("navigator: route {} lanes (complete {}, provisional {}, at {}, on it {}, off for {:.1} s), {} stops ahead, next {:?}, key {:?}", self.route.lanes.len(), self.route.complete, self.route.provisional, self.route.progress, self.route.on_route, self.route.off_for, f.stops.len(), f.stops.first().map(|s| (s.name.clone(), s.position.x.round(), s.position.y.round())), self.route.key);
        }
        self.update_congestion(f);
        // camera: zoomed out with speed, turned with the bus (both eased)
        let want = (110.0 + f.speed_kmh as f64 * 2.2).clamp(110.0, 280.0);
        if self.first {
            self.zoom = want;
            self.cam_heading = f.heading;
        }
        self.zoom += (want - self.zoom) * ease(f.dt, 1.8);
        self.cam_heading += angle_diff(self.cam_heading, f.heading) * ease(f.dt, 0.3);
        let v = f.speed_kmh.abs() / 3.6;
        self.speed_avg += (v - self.speed_avg) * ease(f.dt, 20.0) as f32;
        self.dist_t -= f.dt;
        if self.dist_t <= 0.0 || self.first {
            self.dist_t = 0.4;
            let global = self.global.clone();
            let net = global.as_deref().or(f.traffic.map(|t| &t.net));
            self.next_dist = match (f.stops.first(), net) {
                (Some(s), Some(n)) if !self.route.lanes.is_empty() => self.route_distance(n, s.position).or_else(|| Some((s.position - f.bus).truncate().length())),
                (Some(s), _) => Some((s.position - f.bus).truncate().length()),
                _ => None,
            };
            self.next_turn = net.and_then(|n| self.turn_ahead(n));
            self.street_here = if self.route.on_route {
                self.route.lanes.get(self.route.progress).and_then(|&l| self.street_of(l))
            } else {
                global.as_deref().and_then(|g| g.nearest_lane_near(f.bus, LaneKind::Street)).filter(|l| l.2 < 10.0).and_then(|l| self.street_of(l.0))
            }
                .map(str::to_string);
            self.update_place(f.bus.truncate());
        }
        self.first = false;
        if !self.enabled && self.shown < 0.01 {
            self.panel_rect = [0.0; 4];
            // (followed for the route arrows alone, with the city map shut: nothing to draw)
            if self.city.open {
                self.city(renderer, scene, f);
            }
            return;
        }

        // --- size and place on the screen: small, a corner of its own
        let (sw, sh) = f.screen;
        // (a third of the window's height however tall it is - held to 480 px, it was a
        // sixth of a 4K screen's - but 300 px at least, where its smallest texts were 8 px
        // high on a 720p window; made larger, still short enough to fit the window with its
        // schedule)
        // (of the shorter side: a tablet held upright gave it half the screen's width, over
        // the on-screen buttons along the top)
        let base = (sh.min(sw) * 0.33).max(300.0);
        let base = if f.follow_window { base } else { base.min(480.0) };
        let pw = (base * f.ui_scale).min((sh * 0.7).max(300.0)).round();
        let map_h = (pw * 0.62).round();
        let s = pw / 360.0;
        let bars = (34.0 + 46.0) * s;
        let sched = if self.schedule { (f.stops.len().clamp(1, 5) as f32 * 22.0 + 12.0) * s } else { 0.0 };
        let ph = (map_h + bars + self.place_row_height(s) + sched).round();
        let (w, h) = (pw as u32, ph as u32);
        let (x0, y0) = self.panel_origin((sw, sh), (pw, ph), crate::platform::touch_controls(), f.info_rect, f.touch_gap);

        if self.gpu.is_none() {
            self.gpu = Some(Gpu::new(&renderer.device, renderer.format(), map_samples(renderer.format()), self.atlas.size));
        }
        let resized = self.target.map(|t| (t.1, t.2) != (w, h)).unwrap_or(true);
        if resized {
            if let Some((t, _, _)) = self.target.take() {
                renderer.free_texture(scene, t);
                scene.premultiplied.remove(&t);
            }
            let t = renderer.add_render_texture(scene, w, h);
            scene.premultiplied.insert(t);
            self.target = Some((t, w, h));
        }
        let (tex, _, _) = self.target.unwrap();
        let Some(view) = renderer.texture_view(scene, tex) else { return };
        self.frames_since_drawn = self.frames_since_drawn.saturating_add(1);
        let due = self.time - self.drawn_at >= NAV_REDRAW_S && self.frames_since_drawn >= NAV_REDRAW_FRAMES;
        if !self.city.open && (resized || due) {
            self.drawn_at = self.time;
            self.frames_since_drawn = 0;
            self.draw(renderer, &view, (w, h), map_h, f);
        }
        // (the small navigator steps aside while the city map is open)
        if !self.city.open {
            self.panel_overlay = Some(scene.overlays.len());
            scene.overlays.push((tex, [x0, y0, x0 + pw, y0 + ph]));
        }
        self.panel_rect = [x0, y0, x0 + pw, y0 + ph];
        if self.city.open {
            self.city(renderer, scene, f);
        }
    }

    fn draw(&mut self, renderer: &Renderer, target: &wgpu::TextureView, size: (u32, u32), map_h: f32, f: &NavFrame) {
        let (pw, ph) = (size.0 as f32, size.1 as f32);
        let s = pw / 360.0;
        let wd = words(f.language);
        self.atlas.begin_frame();
        let panel = Rect::new(0.0, 0.0, pw, ph);
        // (the corners of the other panels: 8 px at their size, never less)
        let radius = (8.0 * s).max(8.0);
        let top_h = (34.0 * s).round();
        let map = Rect::new(0.0, top_h, pw, map_h);
        let vp = [map.x, map.y, map.w, map.h];

        // --- roads (buffer 0): rebuilt when the bus went far or tiles brought lanes
        let own = self.own_net.clone();
        let global = self.global.clone();
        let net = global.as_deref().or(f.traffic.map(|t| &t.net)).or(own.as_deref());
        let lanes_now = net.map(|n| n.lanes.len()).unwrap_or(0);
        let rebuild = match &self.roads {
            None => lanes_now > 0,
            Some(r) => r.light != self.light || (r.anchor - f.bus.truncate()).length() > ROAD_RADIUS * 0.45 || (r.lanes_seen != lanes_now && self.time - r.built_at > 1.5),
        };
        let mut road_verts = None;
        if rebuild {
            if let Some(net) = net {
                let anchor = f.bus.truncate();
                let mut p = Painter::new();
                let t0 = std::time::Instant::now();
                build_roads(&mut p, net, anchor, self.light);
                if omsi_cfg::flags::OMSI_DEBUG_NAV.is_set() {
                    log::info!("navigator: roads around ({:.0}, {:.0}) in {:.1} ms: {} vertices", anchor.x, anchor.y, t0.elapsed().as_secs_f64() * 1000.0, p.verts.len());
                }
                road_verts = Some(p.verts);
                self.roads = Some(Roads { light: self.light, anchor, lanes_seen: lanes_now, verts: 0, built_at: self.time });
            }
        }

        // --- the map camera (world coordinates relative to the anchor)
        let anchor = self.roads.as_ref().map(|r| r.anchor).unwrap_or(f.bus.truncate());
        // --- the map's surface (buffer 6)
        let surface_verts = self.minimap_surface(anchor);
        let surface_n = if self.show_surface { self.surface_mesh.map(|m| m.2).unwrap_or(0) } else { 0 };
        let rel = |p: DVec3| Vec3::new((p.x - anchor.x) as f32, (p.y - anchor.y) as f32, 0.0);
        let hd = self.cam_heading.to_radians();
        let fwd = DVec2::new(hd.sin(), hd.cos());
        let look_at = f.bus.truncate() - anchor + fwd * self.zoom * 0.28;
        let pitch = PITCH.to_radians();
        let back = fwd * self.zoom * pitch.cos();
        let eye = DVec3::new(look_at.x - back.x, look_at.y - back.y, self.zoom * pitch.sin());
        let view = Mat4::look_at_rh(eye.as_vec3(), Vec3::new(look_at.x as f32, look_at.y as f32, 0.0), Vec3::Z);
        let clip_panel = [0.0, 0.0, pw, ph];
        let map_layer = Layer::world(view, FOV.to_radians(), vp, [map.x, map.y, map.right(), map.bottom()], 0.0, 1.0);
        let vpm = map_layer.view_proj;

        // --- the route (buffer 1): rebuilt when it or the lane the bus is on changed
        let mut route_verts = None;
        let route_net = global.as_deref().or(f.traffic.map(|t| &t.net));
        if let (Some(rn), true) = (route_net, !self.route.lanes.is_empty()) {
            let first = self.route.lanes.get(self.route.progress.min(self.route.lanes.len().saturating_sub(1))).copied();
            let bus_lane = first.map(|l| lane_from_right(rn, l, f.bus)).unwrap_or(0);
            if self.route_mesh.0 != self.route.version || self.route_mesh.1 != self.jam_version || self.route_mesh.2 != anchor || self.route_mesh.4 != bus_lane {
                let mut p = Painter::new();
                // (one lane wide: a little narrower than the lane, so that it never spills onto the next)
                let style = RouteStyle { extra_m: -0.9, min_px: 4.0, arrows: Some((28.0, 900.0)), max_len: 12_000.0, near: Some((anchor, ROAD_RADIUS * 1.6)) };
                build_route(&mut p, rn, &self.route.lanes[self.route.progress.min(self.route.lanes.len())..], anchor, self.route.s, &self.route_jam, &style, bus_lane);
                self.route_mesh = (self.route.version, self.jam_version, anchor, p.len(), bus_lane);
                route_verts = Some(p.verts);
            }
        } else if self.route_mesh.3 != 0 {
            route_verts = Some(Vec::new());
            self.route_mesh = (self.route.version, self.jam_version, anchor, 0, 0);
        }

        // --- background (half transparent), traffic, markers, text
        let mut bg = Painter::new();
        bg.rounded(panel, radius, if self.cockpit_display { Color::rgba(28, 28, 28, 1.0) } else { PANEL });
        // the map's own ground under the roads (the panel's opacity setting dims it as it does the panel)
        if let (true, Some(sf)) = (self.show_surface, self.surface.as_ref()) {
            bg.rect(map, ground(sf, self.light));
        }
        let n_bg = bg.len();

        let mut dy = Painter::new();
        // other roads where the traffic is heavy or stands
        if let Some(net) = f.traffic.map(|t| &t.net) {
            for (&lane, &c) in &self.congestion {
                let lv = level(c);
                if lv < 3 {
                    continue;
                }
                let Some(l) = net.lanes.get(lane) else { continue };
                if (l.start() - f.bus).truncate().length() > 900.0 {
                    continue;
                }
                let pts: Vec<Vec3> = l.points.iter().map(|p| rel(*p)).collect();
                dy.ribbon(&pts, l.width.max(2.5) * 0.6, 2.0, LEVEL[lv].alpha(0.8), true);
            }
        }
        let n_traffic = dy.len();
        // the other vehicles: blue dots, as the other drivers in ETS2; the public transport in
        // its own colours (trolleybus, bus, tram), a little bigger, its line beside it
        let mut lines: Vec<(DVec3, Color, String)> = Vec::new();
        if let Some(t) = f.traffic.filter(|_| self.show_ai) {
            for c in &t.cars {
                if c.gone || (c.vehicle.position - f.bus).truncate().length() > self.zoom * 3.5 + 150.0 {
                    continue;
                }
                let (color, line) = traffic_kind(c);
                if color == DOT {
                    dy.world_disc(rel(c.vehicle.position), 1.7, 3.6, Color::rgba(8, 8, 8, 0.9));
                    dy.world_disc(rel(c.vehicle.position), 1.2, 2.6, color);
                } else {
                    vehicle_icon(&mut dy, rel(c.vehicle.position), c.vehicle.heading, color, c.is_rail(), 7.0);
                }
                if let Some(l) = line {
                    lines.push((c.vehicle.position, color, l));
                }
            }
        }
        let light_net = net.filter(|n| n.lanes.iter().any(|l| l.traffic_light.is_some()))
            .or(f.traffic.filter(|t| t.net.lanes.iter().any(|l| l.traffic_light.is_some())).map(|t| &t.net));
        if let Some(net) = light_net {
            build_lights(&mut dy, net, anchor);
        }
        let n_world = dy.len();

        let mut ui = Painter::new();
        self.draw_map_marks(&mut ui, f, map, vpm, vp, anchor, s, lines);

        let pad = self.draw_top_bar(&mut ui, f, &wd, net, pw, top_h, s);

        self.draw_bottom_bar(&mut ui, f, &wd, map, pw, pad, s);
        let icon_verts = self.minimap_icons(vpm, vp, anchor, f.bus.truncate(), map, s);

        // --- to the GPU
        let (Some(gpu), device, queue) = (self.gpu.as_mut(), &renderer.device, &renderer.queue) else { return };
        if let Some(v) = road_verts {
            if let Some(r) = self.roads.as_mut() {
                r.verts = v.len();
            }
            gpu.upload(device, queue, 0, &v);
        }
        if let Some(v) = surface_verts {
            gpu.upload(device, queue, 6, &v);
        }
        if let Some(v) = route_verts {
            gpu.upload(device, queue, 1, &v);
        }
        let mut all = bg.verts;
        all.extend(dy.verts);
        let n_ui_start = all.len() as u32;
        all.extend(ui.verts);
        gpu.upload(device, queue, 2, &all);
        let (icon_n, icon_tex) = self.place.upload(gpu, device, queue, 8, &icon_verts);
        gpu.upload_atlas(queue, &mut self.atlas);
        // (the opacity setting is the background's: the map and the text stay solid)
        let flat = Layer::flat(clip_panel, radius, 1.0);
        let backdrop = Layer::flat(clip_panel, radius, if self.cockpit_display { self.opacity } else { crate::ui::backdrop(self.opacity).min(1.0) });
        let mut layers = [flat, map_layer, backdrop];
        for l in layers.iter_mut() {
            l.opacity *= self.shown;
        }
        let roads_n = self.roads.as_ref().map(|r| r.verts as u32).unwrap_or(0);
        let draws = [
            Draw { buffer: 2, range: 0..n_bg, layer: 2, texture: 0 },
            Draw { buffer: 6, range: 0..surface_n, layer: 1, texture: 0 },
            Draw { buffer: 0, range: 0..roads_n, layer: 1, texture: 0 },
            Draw { buffer: 2, range: n_bg..n_bg + n_traffic, layer: 1, texture: 0 },
            Draw { buffer: 1, range: 0..self.route_mesh.3, layer: 1, texture: 0 },
            Draw { buffer: 2, range: n_bg + n_traffic..n_bg + n_world, layer: 1, texture: 0 },
            Draw { buffer: 8, range: 0..icon_n, layer: 0, texture: icon_tex },
            Draw { buffer: 2, range: n_ui_start..all.len() as u32, layer: 0, texture: 0 },
        ];
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("navigator") });
        gpu.render(device, queue, &mut enc, target, size, Some(wgpu::Color::TRANSPARENT), &layers, &draws);
        queue.submit([enc.finish()]);
    }

    /// The map's marks over the roads: the next turn, the street the bus is on, the stops
    /// ahead, the other players, the public transport's lines and the bus itself.
    #[allow(clippy::too_many_arguments)]
    fn draw_map_marks(&mut self, ui: &mut Painter, f: &NavFrame, map: Rect, vpm: Mat4, vp: [f32; 4], anchor: DVec2, s: f32, mut lines: Vec<(DVec3, Color, String)>) {
        let rel = |p: DVec3| Vec3::new((p.x - anchor.x) as f32, (p.y - anchor.y) as f32, 0.0);
        // the far end of the map fades into the panel
        ui.gradient(Rect::new(map.x, map.y, map.w, map.h * 0.3), Color::rgba(28, 28, 28, 0.75), Color::rgba(28, 28, 28, 0.0));
        // the next turn: an arrow and how far, top left of the map
        if let Some((dir, angle, dist, street)) = self.next_turn.as_ref() {
            let icon = match *dir {
                2 => "u_turn_left",
                -1 if *angle < 60.0 => "turn_slight_left",
                1 if *angle < 60.0 => "turn_slight_right",
                -1 => "turn_left",
                _ => "turn_right",
            };
            let t = crate::units::Units::now().distance(*dist as f64);
            let tw = self.fonts.width(&t, 14.0 * s, Weight::Bold);
            // with the street it turns into, when the map names it
            let street = street.as_deref().map(|n| self.fonts.fit(n, 12.0 * s, Weight::Medium, map.w * 0.62 - 50.0 * s - tw));
            let sw_ = street.as_deref().map(|n| self.fonts.width(n, 12.0 * s, Weight::Medium) + 10.0 * s).unwrap_or(0.0);
            let b = Rect::new(map.x + 8.0 * s, map.y + 8.0 * s, 44.0 * s + tw + sw_, 34.0 * s);
            ui.rounded(b, 6.0 * s, PLATE);
            ui.icon(&mut self.atlas, icon, Vec2::new(b.x + 18.0 * s, b.center().y), 24.0 * s, TEXT);
            ui.text_in(&mut self.atlas, &self.fonts, &t, 14.0 * s, Weight::Bold, Rect::new(b.x + 34.0 * s, b.y, tw + 4.0, b.h), Align::Left, TEXT);
            if let Some(n) = street.as_deref() {
                ui.text_in(&mut self.atlas, &self.fonts, n, 12.0 * s, Weight::Medium, Rect::new(b.x + 42.0 * s + tw, b.y, sw_, b.h), Align::Left, TEXT_DIM);
            }
        }
        // the street the bus is on, bottom middle of the map
        if let Some(n) = self.street_here.as_deref() {
            let px = 11.5 * s;
            let n = self.fonts.fit(n, px, Weight::Medium, map.w * 0.7);
            let w = self.fonts.width(&n, px, Weight::Medium) + 14.0 * s;
            let r = Rect::new(map.center().x - w * 0.5, map.bottom() - 24.0 * s, w, 18.0 * s);
            ui.rounded(r, 9.0 * s, PLATE);
            ui.text_in(&mut self.atlas, &self.fonts, &n, px, Weight::Medium, r, Align::Center, STREET);
        }
        // Stops ahead use a bus badge so they read as stops, not generic route dots.
        let n_stops = f.stops.len();
        let markers = spaced_markers(f.stops.iter().enumerate().filter_map(|(k, st)| project(vpm, vp, rel(st.position)).filter(|p| map.contains(*p)).map(|p| (k, p))), 20.0 * s);
        for (k, sp) in markers.into_iter().rev() {
            if k + 1 == n_stops {
                ui.icon(&mut self.atlas, "sports_score", sp - Vec2::new(0.0, 12.0 * s), 14.0 * s, TEXT);
            }
            let next = k == 0;
            let badge = if next { 8.0 } else { 6.5 } * s;
            let fill = if next { Color::rgba(45, 116, 205, 1.0) } else if k + 1 == n_stops { ROUTE } else { Color::rgba(76, 91, 112, 0.98) };
            ui.circle(sp, badge, fill);
            ui.icon(&mut self.atlas, "directions_bus", sp, if next { 12.5 } else { 10.5 } * s, TEXT);
        }
        // the public transport's lines: a tag in its colour above its dot (the nearest first,
        // none on top of another)
        lines.sort_by(|a, b| (a.0 - f.bus).length().total_cmp(&(b.0 - f.bus).length()));
        let mut taken: Vec<Rect> = Vec::new();
        // the other players: an arrow each, their name above it (before the lines' tags)
        for pl in &f.players {
            let Some(p) = project(vpm, vp, rel(pl.position)).filter(|p| map.contains(*p)) else { continue };
            let a = (angle_diff(self.cam_heading, pl.heading) as f32).to_radians();
            arrow(ui, p, a, 7.5 * s, 1.25, Color::rgba(10, 10, 10, 0.8), PLAYER);
            let px = 9.5 * s;
            let name = self.fonts.fit(&pl.name, px, Weight::Bold, 90.0 * s);
            let w = self.fonts.width(&name, px, Weight::Bold) + 8.0 * s;
            let r = Rect::new(p.x - w * 0.5, p.y - 23.0 * s, w, 13.0 * s);
            if taken.iter().any(|o| rects_overlap(o, &r)) {
                continue;
            }
            taken.push(r);
            ui.rounded(r, 3.5 * s, PLAYER);
            ui.text_in(&mut self.atlas, &self.fonts, &name, px, Weight::Bold, r, Align::Center, LINE_TEXT);
        }
        for (pos, color, l) in &lines {
            let Some(p) = project(vpm, vp, rel(*pos)).filter(|p| map.contains(*p)) else { continue };
            let px = 9.5 * s;
            let l = self.fonts.fit(l, px, Weight::Bold, 40.0 * s);
            let w = self.fonts.width(&l, px, Weight::Bold) + 8.0 * s;
            let r = Rect::new(p.x - w * 0.5, p.y - 19.0 * s, w, 13.0 * s);
            if taken.iter().any(|o| o.x < r.right() && r.x < o.right() && o.y < r.bottom() && r.y < o.bottom()) {
                continue;
            }
            taken.push(r);
            ui.rounded(r, 3.5 * s, *color);
            ui.text_in(&mut self.atlas, &self.fonts, &l, px, Weight::Bold, r, Align::Center, LINE_TEXT);
        }
        // the bus: a plain white arrow
        if let Some(bp) = project(vpm, vp, rel(f.bus)) {
            let a = (angle_diff(self.cam_heading, f.heading) as f32).to_radians();
            arrow(ui, bp, a, 9.0 * s, 1.25, Color::rgba(10, 10, 10, 0.8), TEXT);
        }
    }

    /// The top bar: speed (and the limit), temperatures and line, day and game time. Gives
    /// the bars' side padding.
    #[allow(clippy::too_many_arguments)]
    fn draw_top_bar(&mut self, ui: &mut Painter, f: &NavFrame, wd: &Words, net: Option<&Network>, pw: f32, top_h: f32, s: f32) -> f32 {
        // top bar: speed (and the limit) · line ……… game time
        let top = Rect::new(0.0, 0.0, pw, top_h);
        ui.rect(top, BAR);
        let pad = 11.0 * s;
        let base = top.y + top.h * 0.5 + self.fonts.cap_height(17.0 * s, Weight::Bold) * 0.5;
        let mut x = pad;
        let units = crate::units::Units::now();
        let (speed, unit) = units.speed(f.speed_kmh.abs());
        x += ui.text(&mut self.atlas, &self.fonts, &format!("{speed:.0}"), 17.0 * s, Weight::Bold, Vec2::new(x, base), Align::Left, TEXT);
        x += 4.0 * s;
        let unit = if units == crate::units::Units::Metric { wd.kmh } else { unit };
        x += ui.text(&mut self.atlas, &self.fonts, unit, 12.0 * s, Weight::Medium, Vec2::new(x, base), Align::Left, TEXT_DIM);
        let limit = net.and_then(|n| {
            let lane = if self.route.on_route { self.route.lanes.get(self.route.progress).copied() } else { None };
            let lane = lane.or_else(|| n.nearest_lane_near(f.bus, LaneKind::Street).filter(|l| l.2 < 8.0).map(|l| l.0))?;
            let v = n.lanes.get(lane)?.speed_limit_kmh;
            (v > 1.0 && v < 200.0).then_some(v)
        });
        self.speed_limit = limit;
        if let Some(v) = limit {
            x += 10.0 * s;
            let c = Vec2::new(x + 10.0 * s, top.center().y);
            ui.circle(c, 10.5 * s, Color::rgba(200, 40, 40, 1.0));
            ui.circle(c, 8.3 * s, Color::rgba(235, 235, 235, 1.0));
            let t = format!("{:.0}", units.speed_limit(v));
            let px = if t.len() > 2 { 7.5 } else { 9.0 } * s;
            ui.text(&mut self.atlas, &self.fonts, &t, px, Weight::Black, Vec2::new(c.x, c.y + self.fonts.cap_height(px, Weight::Black) * 0.5), Align::Center, Color::rgba(15, 15, 15, 1.0));
        }
        let hh = (f.time / 3600.0) as i32 % 24;
        let mm = ((f.time % 3600.0) / 60.0) as i32;
        let time_text = format!("{hh:02}:{mm:02}");
        let day_text = wd.days[f.weekday.clamp(0, 6) as usize];
        let time_w = self.fonts.width(&time_text, 14.0 * s, Weight::Bold);
        let day_w = self.fonts.width(day_text, 12.0 * s, Weight::Medium);
        ui.text(&mut self.atlas, &self.fonts, &time_text, 14.0 * s, Weight::Bold, Vec2::new(pw - pad, base), Align::Right, TEXT);
        ui.text(&mut self.atlas, &self.fonts, day_text, 12.0 * s, Weight::Medium, Vec2::new(pw - pad - time_w - 5.0 * s, base), Align::Right, TEXT_DIM);

        // Temperatures stay in the navigator header on every bus. The simulator always keeps
        // Cabinair_Temp, while scripts that model heating/air conditioning can overwrite it.
        let temp = format!("EXT {:.0}°C | INT {:.0}°C", f.outside_temp, f.inside_temp);
        let center = match f.line.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
            Some(line) => format!("{temp} | {line}"),
            None => temp,
        };
        let left_edge = (if limit.is_some() { x + 24.0 * s } else { x }) + 6.0 * s;
        let right_edge = pw - pad - time_w - 5.0 * s - day_w - 6.0 * s;
        if right_edge > left_edge {
            let rect = Rect::new(left_edge, top.y, right_edge - left_edge, top.h);
            let center = self.fonts.fit(&center, 11.5 * s, Weight::Bold, rect.w);
            ui.text_in(&mut self.atlas, &self.fonts, &center, 11.5 * s, Weight::Bold, rect, Align::Center, TEXT_DIM);
        }
        pad
    }

    /// The bottom bar with the next stop, and under it the schedule of the next stops.
    #[allow(clippy::too_many_arguments)]
    fn draw_bottom_bar(&mut self, ui: &mut Painter, f: &NavFrame, wd: &Words, map: Rect, pw: f32, pad: f32, s: f32) {
        let n_stops = f.stops.len();
        // bottom bar: the next stop; its distance, the time to it, the planned time and
        // whether the bus is early or late
        let bottom = Rect::new(0.0, map.bottom(), pw, 46.0 * s);
        let loc_h = self.place_row_height(s);
        self.draw_place_row(ui, Rect::new(0.0, bottom.bottom(), pw, loc_h), pad, s);
        ui.rect(bottom, BAR);
        let stop_row = if f.stops.is_empty() {
            Rect::new(pad, bottom.y, pw - 2.0 * pad, bottom.h)
        } else {
            Rect::new(pad, bottom.y + 4.0 * s, pw - 2.0 * pad, 20.0 * s)
        };
        let stop_row = stop_request_icon(ui, &mut self.atlas, f.stop_requested, stop_row, s);
        let note = if self.route.note > 0.0 {
            Some((wd.recalculated, ON_TIME))
        } else if self.route.joined && !self.route.lanes.is_empty() && !self.route.on_route && self.route.off_for > OFF_ROUTE_AFTER {
            // (no way back found after a while: it says so instead of recalculating for ever)
            Some((if self.route.off_for < OFF_ROUTE_AFTER + 20.0 { wd.rerouting } else { wd.off_route }, WARN))
        } else {
            None
        };
        // what the traffic on the route ahead costs, as the module promises: a jam from a
        // minute on, slow traffic from half of one
        let jam_note = (self.jam_cost >= 30.0).then(|| {
            let what = if self.jam_cost >= 60.0 { wd.jam } else { wd.slow };
            (format!("{what} +{:.0} min", (self.jam_cost / 60.0).max(1.0).round()), if self.jam_cost >= 60.0 { LATE } else { WARN })
        });
        match f.stops.first() {
            Some(st) => {
                let name = if n_stops == 1 { format!("{} | {}", st.name.trim(), wd.last_stop) } else { st.name.trim().to_string() };
                ui.text_in(&mut self.atlas, &self.fonts, &name, 13.5 * s, Weight::Bold, stop_row, Align::Left, TEXT);
                let mut parts = Vec::new();
                if let Some(d) = self.next_dist {
                    parts.push(crate::units::Units::now().distance(d as f64));
                    let secs = d / (self.speed_avg.max(5.0) as f64);
                    parts.push(if secs < 60.0 { "<1 min".to_string() } else { format!("{:.0} min", (secs / 60.0).round()) });
                }
                parts.push(format!("{:02}:{:02}", (st.arrival / 3600.0) as i32 % 24, ((st.arrival % 3600.0) / 60.0) as i32));
                let line2 = parts.join("  |  ");
                let y2 = Rect::new(pad, bottom.y + 24.0 * s, pw - 2.0 * pad, 18.0 * s);
                match note {
                    Some((t, c)) => {
                        ui.text_in(&mut self.atlas, &self.fonts, t, 12.5 * s, Weight::Medium, y2, Align::Left, c);
                    }
                    None => match &jam_note {
                        Some((t, c)) => {
                            ui.text_in(&mut self.atlas, &self.fonts, &format!("{line2}  |  {t}"), 12.5 * s, Weight::Medium, y2, Align::Left, *c);
                        }
                        None => {
                            ui.text_in(&mut self.atlas, &self.fonts, &line2, 12.5 * s, Weight::Medium, y2, Align::Left, TEXT_DIM);
                        }
                    },
                }
                if let Some(d) = f.delay {
                    let (txt, c) = if d > 59.0 {
                        (format!("+{}:{:02}", (d / 60.0) as i32, (d % 60.0) as i32), LATE)
                    } else if d < -59.0 {
                        (format!("-{}:{:02}", (-d / 60.0) as i32, (-d % 60.0) as i32), EARLY)
                    } else {
                        (wd.on_time.to_string(), ON_TIME)
                    };
                    ui.text_in(&mut self.atlas, &self.fonts, &txt, 12.5 * s, Weight::Bold, y2, Align::Right, c);
                }
            }
            None => {
                let t = f.terminus.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| wd.no_duty.to_string());
                ui.text_in(&mut self.atlas, &self.fonts, &t, 13.0 * s, Weight::Medium, stop_row, Align::Left, TEXT_DIM);
            }
        }
        // the schedule: the next stops with their planned times
        if self.schedule && !f.stops.is_empty() {
            let mut y = bottom.bottom() + loc_h + 6.0 * s;
            ui.rect(Rect::new(pad, bottom.bottom() + loc_h, pw - 2.0 * pad, 1.0), Color::WHITE.alpha(0.06));
            let late = f.delay.unwrap_or(0.0);
            for st in f.stops.iter().take(5) {
                let r = Rect::new(pad, y, pw - 2.0 * pad, 22.0 * s);
                let planned = format!("{:02}:{:02}", (st.arrival / 3600.0) as i32 % 24, ((st.arrival % 3600.0) / 60.0) as i32);
                ui.text_in(&mut self.atlas, &self.fonts, &planned, 12.5 * s, Weight::Bold, r, Align::Left, TEXT_DIM);
                ui.text_in(&mut self.atlas, &self.fonts, st.name.trim(), 13.0 * s, Weight::Medium, Rect::new(r.x + 46.0 * s, r.y, r.w - 100.0 * s, r.h), Align::Left, TEXT);
                // when the bus will be there at this lateness
                let exp = st.arrival + late;
                let e = format!("{:02}:{:02}", (exp / 3600.0).rem_euclid(24.0) as i32, ((exp.rem_euclid(3600.0)) / 60.0) as i32);
                ui.text_in(&mut self.atlas, &self.fonts, &e, 12.5 * s, Weight::Medium, r, Align::Right, if late > 59.0 { LATE } else if late < -59.0 { EARLY } else { TEXT_DIM });
                y += 22.0 * s;
            }
        }
    }
}

/// Congestion of the traffic's lanes carried over to the same paths of `net`.
fn congestion_on(net: &Network, traffic: &Network, c: &HashMap<usize, f32>) -> HashMap<usize, f32> {
    let mut out = HashMap::new();
    for (&l, &v) in c {
        let Some(lane) = traffic.lanes.get(l) else { continue };
        let Some(key) = lane.key else { continue };
        if let Some(cands) = net.by_key.get(&key) {
            for &g in cands {
                if net.lanes[g].reversed == lane.reversed {
                    out.insert(g, v);
                }
            }
        }
    }
    out
}

impl<'a> NavFrame<'a> {
    fn clone_ref(&self) -> NavFrame<'a> {
        NavFrame { traffic: self.traffic, players: self.players.clone(), bus: self.bus, heading: self.heading, speed_kmh: self.speed_kmh, outside_temp: self.outside_temp, inside_temp: self.inside_temp, line: self.line.clone(), terminus: self.terminus.clone(), stops: self.stops.clone(), delay: self.delay, passengers: self.passengers, time: self.time, weekday: self.weekday, language: self.language, screen: self.screen, ui_scale: self.ui_scale, follow_window: self.follow_window, dt: self.dt, stop_requested: self.stop_requested, info_rect: self.info_rect, touch_gap: self.touch_gap }
    }
}


mod roads;
pub(crate) use roads::*;
mod place;
mod surface;
use surface::ground;
mod city_map;
pub use city_map::*;
#[cfg(test)]
mod tests;
