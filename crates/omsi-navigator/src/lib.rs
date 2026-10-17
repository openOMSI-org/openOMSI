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

use glam::{DVec2, DVec3, Mat4, Vec2, Vec3};
use hashbrown::HashMap;
use omsi_render::{Renderer, Scene, TextureId};
use omsi_sim::traffic::{LaneKind, Network};
use omsi_ui::paint::Align;
use omsi_ui::{Atlas, Color, Draw, Fonts, Gpu, Layer, Painter, Rect, Weight};

use omsi_sim::ai_traffic::TrafficSim;

// --- colours (sRGB) -------------------------------------------------------------------

// neutral dark, half transparent, calm
const NAV_SLOW_FPS: f32 = 30.0;
const NAV_MAX_FPS: f32 = 75.0;
// (the launcher's map picture draws with these too, so the two maps cannot look apart)
pub const ROAD_CASING: Color = style::LIGHT.road_edge;
pub const ROAD: Color = style::LIGHT.road;
pub const ROAD_MAIN: Color = style::LIGHT.road;
pub const ROUTE: Color = style::LIGHT.route;
const DOT: Color = Color::rgba(70, 140, 255, 1.0);
/// The public transport's kinds: trolleybuses, buses, trams.
const TROLLEY: Color = Color::rgba(46, 184, 92, 1.0);
/// The public transport's drops and their line numbers.
const TRANSIT: Color = Color::rgba(52, 168, 83, 1.0);
const TRANSIT_TEXT: Color = Color::rgba(80, 200, 110, 1.0);
const BUS: Color = Color::rgba(226, 58, 52, 1.0);
const TRAM: Color = Color::rgba(240, 190, 30, 1.0);

/// The other players of a LAN session or server: an arrow the way they face, with their name.
const PLAYER: Color = Color::rgba(190, 96, 255, 1.0);

/// An aircraft of the timetable (Spandau's approaches into Tegel run as line `TXL` on the
/// map's air paths): no vehicle of the streets, left off the maps.
pub fn in_the_air(c: &omsi_sim::ai_traffic::model::AiCar, net: &Network) -> bool {
    net.lanes.get(c.state.lane).is_some_and(|l| l.kind == LaneKind::Air)
}

/// What a traffic vehicle is on the map: a trolleybus (its model has trolley poles to
/// raise, `cp_SHTANGALEV` or `shtanga_lev_rot`), a tram, a bus (one on a timetable or
/// showing a line), or any other car; and the line it shows, if it is public transport.
fn traffic_kind(c: &omsi_sim::ai_traffic::model::AiCar) -> (Color, Option<String>) {
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
/// The public transport's mark, as the city transport maps draw it: a green drop with a bus
/// in it, its point the way the vehicle heads (`dir`, on the screen), and the line beside it
/// in green on a dark tag where there is room (`taken`: what is drawn there already).
#[allow(clippy::too_many_arguments)]
fn transit_drop(ui: &mut Painter, atlas: &mut Atlas, fonts: &Fonts, p: Vec2, dir: Vec2, line: Option<&str>, s: f32, taken: &mut Vec<Rect>, pal: &style::Palette) {
    let r = 8.5 * s;
    let d = dir.normalize_or(Vec2::new(0.0, -1.0));
    let drop = |r: f32| -> Vec<Vec2> {
        // the circle from where the point's sides touch it, round to the other side
        let tip = 1.75 * r;
        let side = (r / tip).acos();
        let a0 = d.y.atan2(d.x);
        let mut pts = vec![p + d * tip];
        let k = 20;
        for i in 0..=k {
            let a = a0 + side + (std::f32::consts::TAU - 2.0 * side) * i as f32 / k as f32;
            pts.push(p + Vec2::new(a.cos(), a.sin()) * r);
        }
        pts
    };
    ui.convex(&drop(r + 1.6 * s), Color::rgba(255, 255, 255, 0.95));
    ui.convex(&drop(r), TRANSIT);
    ui.icon(atlas, "directions_bus", p, 11.5 * s, Color::WHITE);
    taken.push(Rect::new(p.x - r, p.y - r, 2.0 * r, 2.0 * r));
    let Some(line) = line.map(str::trim).filter(|l| !l.is_empty()) else { return };
    let px = 11.5 * s;
    let l = fonts.fit(line, px, Weight::Bold, 54.0 * s);
    let w = fonts.width(&l, px, Weight::Bold) + 10.0 * s;
    let h = 17.0 * s;
    // (beside the drop, away from its point)
    let right = d.x <= 0.3;
    for side in if right { [1.0f32, -1.0] } else { [-1.0, 1.0] } {
        let x = if side > 0.0 { p.x + r + 4.0 * s } else { p.x - r - 4.0 * s - w };
        let tag = Rect::new(x, p.y - h * 0.5, w, h);
        if taken.iter().any(|o| roads::rects_overlap(o, &tag)) {
            continue;
        }
        taken.push(tag);
        ui.rounded(tag, 4.0 * s, pal.card);
        ui.text_in(atlas, fonts, &l, px, Weight::Bold, tag, Align::Center, TRANSIT_TEXT);
        return;
    }
}

const ON_TIME: Color = Color::rgba(110, 200, 120, 1.0);
const WARN: Color = Color::rgba(235, 170, 60, 1.0);
const STOP_REQUEST: Color = Color::hex(0xF0A030);

pub fn stop_requested(vehicle: &omsi_sim::vehicle::VehicleInstance) -> bool {
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
    pub traffic: Option<&'a TrafficSim>,
    /// The game shows on-screen touch controls (a phone or tablet, or `OMSI_TOUCH`): the panel
    /// keeps clear of their row.
    pub touch: bool,
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
    /// The date of the session (yyyymmdd): which form of each line holds (a map with eras).
    pub date: i32,
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

/// Street name signs (place, heading, name) and weaker hints (place, name).
type NameHints = (Vec<(DVec3, f64, String)>, Vec<(DVec3, String)>);

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
    /// `Navigator::look_key` it was built for.
    look: u64,
    anchor: DVec2,
    lanes_seen: usize,
    /// Vertices of the ground (buffer 0) and of the houses: their shadows (buffer 12), standing
    /// (8) and flat (13).
    verts: usize,
    houses: [usize; 3],
    built_at: f32,
}

pub struct Navigator {
    /// The current small-map texture in Scene::overlays, for the cockpit display.
    pub panel_overlay: Option<usize>,
    pub cockpit_display: bool,
    /// The speed limit (km/h) the top bar showed last, for the information bar (#2476).
    pub speed_limit: Option<f32>,
    frame_dt: f32,
    last_frame: Option<std::time::Instant>,
    draw_every: u32,
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
    building: Option<std::sync::mpsc::Receiver<(Network, HashMap<i64, DVec3>, Streets, Option<omsi_map::package::Package>, NameHints)>>,
    /// What the street names were made from (signs and weaker hints), to make them again
    /// when the timetable's stop names add hints; and whether those were added.
    name_hints: NameHints,
    stop_names_learnt: bool,
    pub global_version: u64,
    roads: Option<Roads>,
    route: Route,
    /// The route mesh (buffers 1 and 9): the route and traffic versions, the anchor, its
    /// size, the bus's lane and the look it was built for.
    route_mesh: (u64, u64, DVec2, u32, usize, u64),
    /// Per lane of the traffic: how congested it is (0 free … 1 standing), smoothed.
    congestion: HashMap<usize, f32>,
    /// The same for the route's lanes ahead (in the route's network), and a version that
    /// changes when any of their levels does (the route is drawn again).
    route_jam: HashMap<usize, f32>,
    jam_version: u64,
    congestion_t: f32,
    /// Traffic lane → the same lanes of the map's network, and the versions it is for.
    lane_map: HashMap<usize, Vec<usize>>,
    lane_map_gen: (u64, u64),
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
    /// The terrain view of the city map: contour lines and their heights, where the map's package carries them (the
    /// `nav_contours` setting; a button on the map).
    pub show_contours: bool,
    /// The light look of the maps (the `nav_light` setting).
    pub light: bool,
    /// The map's own layers (houses, ground, paving: `omsi_scene::mapgen`), once read, and a
    /// number that changes when they arrive.
    pub map_data: Option<std::sync::Arc<mapmesh::MapData>>,
    map_version: u64,
    layers_rx: Option<std::sync::mpsc::Receiver<mapmesh::MapData>>,
    /// Houses standing in 3D (the `nav_3d` setting); flat roofs otherwise.
    pub show_3d: bool,
    /// The navigator is a widget: dragged anywhere and resized by its corner (`nav_widget`).
    pub widget: bool,
    /// Its size against the usual one (`nav_size`, 0.6 - 2).
    pub size: f32,
    /// A press on the navigator's corner: the size, the cursor and the panel's width then.
    resizing: Option<(f32, [f32; 2], f32)>,
    /// The ground being built on a worker for the small map (its look and lane count) and for
    /// the city map, and the network's carriageways with whether the paving covers each.
    panel_job: Option<build::Job<(u64, usize)>>,
    city_job: Option<build::Job<CityJob>>,
    road_cache: Option<(u64, std::sync::Arc<Vec<(MapRoad, bool)>>)>,
    road_job: Option<(u64, std::sync::mpsc::Receiver<std::sync::Arc<Vec<(MapRoad, bool)>>>)>,
    /// Bumped when a new list of carriageways arrives (the ground is built again).
    roads_version: u64,
    /// Where each car was drawn last (eased towards where it is, so they glide).
    drawn_cars: HashMap<u64, (DVec3, f32)>,
    /// The timetable's vehicles the traffic does not drive: where they are, where they are
    /// drawn, when to look again, and the trips' routes worked out so far.
    live: Vec<live::LiveVehicle>,
    live_drawn: Vec<live::LiveVehicle>,
    live_t: f32,
    /// The departures on the road (looked for every few seconds) and their timetables.
    live_list: Vec<usize>,
    live_list_t: f32,
    live_tt: HashMap<usize, std::sync::Arc<omsi_sim::ai_traffic::bus_service::AiTimetable>>,
    trip_paths: HashMap<usize, Option<std::sync::Arc<live::TripPath>>>,
    /// What the package names: icons, names, where the bus is.
    place: place::PlaceView,
    /// The date of the session (yyyymmdd).
    date: i32,
    /// The game's clock (seconds of the day) and weekday (0 = Monday): the timetable lights the next departure.
    day_clock: (f64, i32),
    /// The map's timetable for the lines panel, for the date it was read for.
    times: Option<(i32, std::sync::Arc<line_times::LineTimes>)>,
    /// The worker reading it (for which date, and where the result comes).
    times_job: Option<(i32, std::sync::mpsc::Receiver<std::sync::Arc<line_times::LineTimes>>)>,
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
pub fn duty_parts(duty: Option<&omsi_sim::timetable_run::PlayerDuty>) -> (Option<String>, Option<String>, Vec<NavStop>, Option<(String, String)>) {
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

fn draw_every(now: u32, frame_dt: f32) -> u32 {
    let slow = 1.0 / (NAV_SLOW_FPS * frame_dt.max(1e-4));
    let ideal = 1.0 / (NAV_MAX_FPS * frame_dt.max(1e-4));
    if slow < 0.92 || (now == 2 && slow < 1.08) {
        return 2;
    }
    let n = now as f32;
    if ideal > n - 1.0 - 0.08 && ideal <= n + 0.08 {
        return now;
    }
    (ideal.ceil() as u32).clamp(1, 4)
}

fn zoom_target(speed_kmh: f32, turn_dist: Option<f64>, turning_deg: f64) -> f64 {
    let speed = 150.0 + speed_kmh.abs() as f64 * 2.4;
    let turn = turn_dist.map_or(f64::MAX, |d| 150.0 + d * 0.6);
    let bend = 1.0 - (turning_deg.abs() / 25.0).clamp(0.0, 1.0) * 0.25;
    (speed.min(turn) * bend).clamp(150.0, 320.0)
}

fn ease(dt: f32, tau: f32) -> f64 {
    (1.0 - (-dt / tau.max(1e-3)).exp()) as f64
}

/// A `navigator_corner` of the form `at x,y` (where the navigator was dragged to, #940).
pub fn placed_at(corner: &str) -> Option<[f32; 2]> {
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
            frame_dt: 1.0 / 60.0,
            last_frame: None,
            draw_every: 1,
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
            name_hints: Default::default(),
            stop_names_learnt: false,
            global_version: 0,
            roads: None,
            route: Route::default(),
            route_mesh: (u64::MAX, 0, DVec2::ZERO, 0, 0, 0),
            congestion: HashMap::new(),
            route_jam: HashMap::new(),
            jam_version: 0,
            congestion_t: 0.0,
            lane_map: HashMap::new(),
            lane_map_gen: (u64::MAX, u64::MAX),
            zoom: 120.0,
            cam_heading: 0.0,
            time: 0.0,
            next_dist: None,
            arrows: false,
            show_ai: true,
            surface: None,
            surface_version: 0,
            show_surface: true,
            show_contours: false,
            light: false,
            map_data: None,
            map_version: 0,
            layers_rx: None,
            show_3d: true,
            widget: true,
            size: 1.0,
            resizing: None,
            drawn_cars: HashMap::new(),
            panel_job: None,
            city_job: None,
            road_cache: None,
            road_job: None,
            roads_version: 0,
            live: Vec::new(),
            live_drawn: Vec::new(),
            live_t: 0.0,
            live_list: Vec::new(),
            live_list_t: 0.0,
            live_tt: HashMap::new(),
            trip_paths: HashMap::new(),
            place: Default::default(),
            date: 0,
            day_clock: (0.0, 0),
            times: None,
            times_job: None,
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
    pub fn start_map(&mut self, world: std::sync::Arc<omsi_scene::World>) {
        if self.building.is_some() || self.global.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let wm = world.clone();
        std::thread::Builder::new()
            .name("navigator map".into())
            .spawn(move || {
                let world = wm;
                // (the map's package: Maps/<Map>/openomsi-realmap, if it has one)
                let surface = omsi_map::package::Package::open(&world.map_dir);
                let m = world.navigation_map();
                let mut net = Network { lanes: m.lanes, ..Default::default() };
                net.link(1.5);
                confirm_road_surfaces(&mut net, &m.road_surfaces);
                probe_lanes(&net);
                let streets = build_streets(&net, &m.signs, &m.names);
                let _ = tx.send((net, m.places, streets, surface, (m.signs, m.names)));
            })
            .ok();
        self.building = Some(rx);
        // the map's own layers: a second worker, they take longer the first time (then come
        // from the cache in a moment)
        if self.map_data.is_none() && self.layers_rx.is_none() {
            let (ltx, lrx) = std::sync::mpsc::channel();
            let w = world.clone();
            std::thread::Builder::new()
                .name("navigator layers".into())
                .spawn(move || {
                    let _ = ltx.send(mapmesh::MapData::new(std::sync::Arc::new(omsi_scene::threads::background_pool().install(|| w.map_layers()))));
                })
                .ok();
            self.layers_rx = Some(lrx);
        }
    }

    /// The map's layers, given at once (an offscreen picture reads them on its own thread).
    pub fn set_layers(&mut self, layers: omsi_map::layers::Layers) {
        self.map_data = Some(std::sync::Arc::new(mapmesh::MapData::new(std::sync::Arc::new(layers))));
        self.map_version += 1;
    }

    /// The map's network given at once (an offscreen picture reads it on its own thread).
    pub fn set_map(&mut self, map: omsi_scene::NavigationMap) {
        let mut net = Network { lanes: map.lanes, ..Default::default() };
        net.link(1.5);
        confirm_road_surfaces(&mut net, &map.road_surfaces);
        probe_lanes(&net);
        self.streets = Some(std::sync::Arc::new(build_streets(&net, &map.signs, &map.names)));
        self.name_hints = (map.signs.clone(), map.names.clone());
        let global = std::sync::Arc::new(net);
        self.global = Some(global);
        self.stop_pos = std::sync::Arc::new(map.places);
        self.global_version += 1;
    }

    /// The timetable's stop names (stop object → name) as hints of the streets' names: a stop
    /// named after its street names the street where no sign does. Once, when the map's
    /// network is there.
    pub fn stop_names_learnt(&self) -> bool {
        self.stop_names_learnt
    }

    pub fn learn_stop_names(&mut self, names: &HashMap<i64, String>) {
        if self.stop_names_learnt {
            return;
        }
        let Some(net) = self.global.clone() else { return };
        self.stop_names_learnt = true;
        let mut hints = self.name_hints.1.clone();
        let before = hints.len();
        for (id, name) in names {
            if let (Some(p), Some(n)) = (self.stop_pos.get(id), omsi_scene::street_name_of(name)) {
                hints.push((*p, n));
            }
        }
        if hints.len() > before {
            self.streets = Some(std::sync::Arc::new(build_streets(&net, &self.name_hints.0, &hints)));
            self.name_hints.1 = hints;
        }
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
            // (the lanes that pass the stop on its side of the road: a way that came to the
            // stop on the far side turned round in front of it)
            let targets: Vec<usize> = lanes_near(net, stop.position.truncate(), 40.0)
                .into_iter()
                .filter(|&i| net.lanes[i].kind == LaneKind::Street && passes_on_its_side(net, i, stop.position, 20.0))
                .collect();
            if let Some((mut path, k)) = way_to_any(net, f.bus, f.heading, &targets, 30_000.0) {
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
            let (mut path, j) = way_to_any(net, f.bus, f.heading, &near, max)?;
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

    /// Where each vehicle of the traffic is drawn: eased towards where it is, so that one the
    /// traffic steps seldom (far from the bus) glides instead of jumping; a long way (a car put
    /// back on the road elsewhere) is taken at once.
    fn update_drawn_cars(&mut self, f: &NavFrame) {
        let Some(t) = f.traffic else {
            self.drawn_cars.clear();
            return;
        };
        let k = ease(f.dt, 0.12);
        let stamp = self.time;
        for c in t.cars.iter().filter(|c| !c.gone) {
            let now = c.vehicle.position;
            let e = self.drawn_cars.entry(c.id).or_insert((now, stamp));
            if (e.0 - now).truncate().length() > 40.0 {
                e.0 = now;
            } else {
                e.0 += (now - e.0) * k;
            }
            e.1 = stamp;
        }
        self.drawn_cars.retain(|_, v| v.1 == stamp);
    }

    /// Where a vehicle of the traffic is drawn (see `update_drawn_cars`).
    fn smoothed_position(&self, c: &omsi_sim::ai_traffic::model::AiCar) -> DVec3 {
        self.drawn_cars.get(&c.id).map(|v| v.0).unwrap_or(c.vehicle.position)
    }

    /// How congested the lanes near the bus are, from how fast the traffic on them flows
    /// against how fast it could (0 free … 1 standing), smoothed over seconds:
    ///
    /// * a car waiting at a red light, a bus at its stop, a car parked or pulling out is no
    ///   evidence of a jam - only cars held for longer than a light's cycle count as standing;
    /// * one car is no evidence either way: a lane needs two moving or standing in it;
    /// * a full lane that moves is free (the old count of cars made busy streets amber and
    ///   two cars at a light red).
    ///
    /// The route's own lanes take the levels of the traffic's lanes they are (by their key,
    /// else by place and heading: junction paths have no key, and were never coloured).
    fn update_congestion(&mut self, f: &NavFrame) {
        self.congestion_t -= f.dt;
        if self.congestion_t > 0.0 {
            return;
        }
        let step = 0.25f32;
        self.congestion_t = step;
        let Some(t) = f.traffic else { return };
        // per lane: the sum of the cars' speed against the lane's free speed, how many, and
        // how many stand for long
        let mut by_lane: HashMap<usize, (f32, u32, u32)> = HashMap::new();
        for c in &t.cars {
            if c.gone || (c.vehicle.position - f.bus).truncate().length() > 1500.0 {
                continue;
            }
            // (a bus at its stop, or a car standing as part of the furniture)
            if c.bus.as_ref().is_some_and(|b| !matches!(b.phase, omsi_sim::ai_traffic::bus_service::Phase::Running)) {
                continue;
            }
            let Some(l) = t.net.lanes.get(c.state.lane) else { continue };
            let free = (l.speed_limit_kmh.clamp(20.0, 60.0) / 3.6) * 0.8;
            let v = c.state.speed.max(0.0);
            // (waiting at a light up to a cycle is how traffic flows there)
            let ratio = if v < 0.8 && c.stopped < 35.0 { 0.75 } else { (v / free).min(1.0) };
            let e = by_lane.entry(c.state.lane).or_insert((0.0, 0, 0));
            e.0 += ratio;
            e.1 += 1;
            if c.stopped > 45.0 {
                e.2 += 1;
            }
        }
        let k = ease(step, 6.0) as f32;
        // (no cars: back to free, slowly)
        for v in self.congestion.values_mut() {
            *v -= *v * k * 0.5;
        }
        for (lane, (sum, n, standing)) in by_lane {
            if n < 2 && standing == 0 {
                continue;
            }
            let flow = sum / n as f32;
            let mut score = (1.0 - flow).clamp(0.0, 1.0);
            if standing >= 2 {
                score = score.max(0.9);
            }
            let v = self.congestion.entry(lane).or_insert(0.0);
            *v += (score - *v) * k;
        }
        self.congestion.retain(|_, v| *v > 0.03);
        // the same for the route ahead, in the route's own network
        let global = self.global.clone();
        if t.lanes_generation != self.lane_map_gen.0 || self.global_version != self.lane_map_gen.1 {
            self.lane_map.clear();
            self.lane_map_gen = (t.lanes_generation, self.global_version);
        }
        let jam = match global.as_deref() {
            Some(g) => {
                let mut out = HashMap::new();
                for (&l, &v) in &self.congestion {
                    let targets = self.lane_map.entry(l).or_insert_with(|| same_paths(g, &t.net, l));
                    for &gi in targets.iter() {
                        out.insert(gi, v);
                    }
                }
                out
            }
            None => self.congestion.clone(),
        };
        let net = global.as_deref().unwrap_or(&t.net);
        let r = &self.route;
        let mut route_jam = HashMap::new();
        let mut cost = 0.0;
        for &l in r.lanes.iter().skip(r.progress).take(600) {
            let Some(&c) = jam.get(&l) else { continue };
            route_jam.insert(l, c);
            // the time the slow stretches ahead cost
            if let Some(lane) = net.lanes.get(l) {
                if level(c) >= 2 {
                    let v_free = lane.speed_limit_kmh.clamp(20.0, 60.0) / 3.6;
                    let v = (v_free * (1.0 - c)).max(1.0);
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
        scene.shift_overlays(start, origin_x);
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
        self.date = f.date;
        self.day_clock = (f.time, f.weekday);
        if let Some(rx) = self.building.as_ref() {
            if let Ok((net, pos, streets, surface, hints)) = rx.try_recv() {
                self.name_hints = hints;
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
        if let Some(rx) = self.layers_rx.as_ref() {
            if let Ok(m) = rx.try_recv() {
                log::info!("navigator: the map's layers are there ({} houses, {} trees)", m.layers.buildings.len(), m.layers.trees.len());
                self.map_data = Some(std::sync::Arc::new(m));
                self.map_version += 1;
                self.layers_rx = None;
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
        self.update_drawn_cars(f);
        self.poll_road_job();
        // camera: zoomed out with speed, turned with the bus (both eased)
        let want = zoom_target(f.speed_kmh, self.next_turn.as_ref().map(|t| t.2), angle_diff(self.cam_heading, f.heading));
        if self.first {
            self.zoom = want;
            self.cam_heading = f.heading;
        }
        self.zoom += (want - self.zoom) * ease(f.dt, if want < self.zoom { 0.7 } else { 1.8 });
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
        // (the widget's own size on top: `nav_size`, dragged at its corner)
        let pw = (base * f.ui_scale * self.size.clamp(0.6, 2.0)).min((sh * 0.7).max(300.0) * self.size.clamp(1.0, 2.0)).min(sw * 0.92).round();
        let map_h = (pw * 0.8).round();
        let s = pw / 360.0;
        let sched = if self.schedule { (f.stops.len().clamp(1, 5) as f32 * 22.0 + 12.0) * s } else { 0.0 };
        let ph = (map_h + self.place_row_height(s) + sched).round();
        let (w, h) = (pw as u32, ph as u32);
        let (x0, y0) = self.panel_origin((sw, sh), (pw, ph), f.touch, f.info_rect, f.touch_gap);

        if self.gpu.is_none() {
            self.gpu = Some(Gpu::with_depth(&renderer.device, renderer.format(), map_samples(renderer.format()), self.atlas.size));
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
        let now = std::time::Instant::now();
        if let Some(dt) = self.last_frame.replace(now).map(|t| (now - t).as_secs_f32()).filter(|dt| *dt < 0.1) {
            self.frame_dt += (dt - self.frame_dt) * ease(dt, 0.5) as f32;
        }
        self.draw_every = draw_every(self.draw_every, self.frame_dt);
        self.frames_since_drawn = self.frames_since_drawn.saturating_add(1);
        if !self.city.open && (resized || self.frames_since_drawn >= self.draw_every) {
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

}

/// The lanes of `net` that are traffic lane `l`: the same path by its key, else (a junction's
/// paths have none) the lane of `net` that starts and ends where it does, the same way.
fn same_paths(net: &Network, traffic: &Network, l: usize) -> Vec<usize> {
    let Some(lane) = traffic.lanes.get(l) else { return Vec::new() };
    if let Some(cands) = lane.key.and_then(|k| net.by_key.get(&k)) {
        let v: Vec<usize> = cands.iter().copied().filter(|&g| net.lanes[g].reversed == lane.reversed).collect();
        if !v.is_empty() {
            return v;
        }
    }
    let mid = lane.at(lane.length() * 0.5);
    roads::lanes_near(net, mid.0.truncate(), 20.0)
        .into_iter()
        .filter(|&g| {
            let o = &net.lanes[g];
            o.kind == lane.kind
                && (o.start() - lane.start()).truncate().length() < 3.0
                && (o.end() - lane.end()).truncate().length() < 3.0
                && angle_diff(o.start_heading() as f64, lane.start_heading() as f64).abs() < 30.0
        })
        .collect()
}

impl<'a> NavFrame<'a> {
    fn clone_ref(&self) -> NavFrame<'a> {
        NavFrame { traffic: self.traffic, touch: self.touch, players: self.players.clone(), bus: self.bus, heading: self.heading, speed_kmh: self.speed_kmh, outside_temp: self.outside_temp, inside_temp: self.inside_temp, line: self.line.clone(), terminus: self.terminus.clone(), stops: self.stops.clone(), delay: self.delay, passengers: self.passengers, time: self.time, weekday: self.weekday, date: self.date, language: self.language, screen: self.screen, ui_scale: self.ui_scale, follow_window: self.follow_window, dt: self.dt, stop_requested: self.stop_requested, info_rect: self.info_rect, touch_gap: self.touch_gap }
    }
}


pub use omsi_mapdraw::roads::{self, *};
mod line_times;
mod lines_panel;
mod lines_sheet;
mod place;
mod relief;
mod surface;
pub use omsi_mapdraw::style;
pub use omsi_mapdraw as mapmesh;
mod panel;
mod live;
mod build;
mod city_map;
pub use city_map::*;
use lines_panel::*;
#[cfg(test)]
mod tests;


