//! The route navigator: a small tilted 3D map in a corner of the screen, after the Route
//! Advisor of Euro Truck Simulator 2, made for a bus driver's duty.
//!
//! * The map turns with the bus and zooms out with speed; the camera looks over the bus
//!   from behind and above, so the road ahead fills the picture.
//! * Roads are drawn from the traffic network's lanes (dark casing, grey surface); the
//!   trip's route from the first stop to the last lies on them with arrows, coloured by
//!   how busy each stretch is (blue empty, green, yellow, red, dark red jammed), and its
//!   stops as the stop sign the player chose (`stop_signs`: the next larger, the last ringed).
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
//! * The player's own pins (`nav_pins`), set on the city map with its pin tool or a
//!   right-click: on a free drive the route goes through them to a destination of the
//!   player's choosing; on a duty they are a diversion for the navigation alone, back onto
//!   the trip's route after the last. Followed, recalculated and drawn as a trip's route is.
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

use crate::companion::Stage;
use crate::traffic::Traffic;

// --- colours (sRGB) -------------------------------------------------------------------

// Omsi-Hub's night blue, half transparent, calm (its overlay's glass)
const NAV_REDRAW_S: f32 = 1.0 / 30.0;
/// Frames between two drawings of the navigator at least. Below 30 fps it was drawn every
/// frame, and a drawing is a pass of its own (4x MSAA, its own submit): on OpenGL, where
/// wgpu makes the GL calls in the submit, 4.2 ms of a 43 ms frame (a Radeon R7 200, 23
/// fps), against 0.6 ms on Vulkan. Every other frame halves that where frames are slow.
const NAV_REDRAW_FRAMES: u32 = 2;
const PANEL: Color = Color::rgba(20, 26, 38, 0.70);
// (the bars under the texts darken whatever the opacity setting leaves of the panel: at a
// third the cab showed through behind the next stop)
const BAR: Color = Color::rgba(6, 9, 16, 0.55);
// (the launcher's map picture draws with these too, so the two maps cannot look apart)
pub(crate) const ROAD_CASING: Color = Color::rgba(30, 30, 30, 0.9);
pub(crate) const ROAD: Color = Color::rgba(92, 92, 92, 1.0);
pub(crate) const ROAD_MAIN: Color = Color::rgba(112, 112, 112, 1.0);
pub(crate) const ROUTE: Color = Color::rgba(214, 48, 40, 1.0);
const DOT: Color = Color::rgba(70, 140, 255, 1.0);
/// The public transport's dots: trolleybuses, buses, trams (their line tags in the same
/// colours: `stop_signs::draw_chip`).
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
const TEXT: Color = Color::rgba(235, 235, 235, 1.0);
// (the second texts - units, the day, the times - bright enough to read on a lit cab)
const TEXT_DIM: Color = Color::rgba(178, 178, 178, 1.0);
// (punctuality in Omsi-Hub's colours, as the duty board writes it)
const LATE: Color = crate::nav_duty::LATE_INK;
const EARLY: Color = crate::nav_duty::EARLY_INK;
const ON_TIME: Color = crate::nav_duty::ON_TIME_INK;
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

/// Another player of the session as the maps show them.
#[derive(Debug, Clone, PartialEq)]
pub struct NavPlayer {
    /// Where they are: their bus, or on foot where they walk (riding in someone's bus: that bus).
    pub position: DVec3,
    /// Compass heading (degrees, 0 = +y, clockwise).
    pub heading: f64,
    pub name: String,
}

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
    /// The player's duty (its trips, legs and progress), for the duty board under the map
    /// and the duty sheet beside the city map (`nav_duty`).
    pub duty: Option<&'a crate::schedule::PlayerDuty>,
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
        "NLD" | "NL" => Words { kmh: "km/u", days: ["Ma", "Di", "Wo", "Do", "Vr", "Za", "Zo"], off_route: "Buiten de route", rerouting: "Route wordt herberekend", recalculated: "Route herberekend", jam: "File", slow: "Langzaam verkeer", no_duty: "Vrij rijden", last_stop: "Eindhalte", on_time: "op tijd" },
        "UKR" | "UK" | "UA" => Words { kmh: "км/год", days: ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Нд"], off_route: "Поза маршрутом", rerouting: "Перебудова маршруту", recalculated: "Маршрут перебудовано", jam: "Затор", slow: "Повільний рух", no_duty: "Вільна поїздка", last_stop: "Кінцева", on_time: "за графіком" },
        "POL" | "PL" => Words { kmh: "km/h", days: ["Pn", "Wt", "Śr", "Cz", "Pt", "So", "Nd"], off_route: "Poza trasą", rerouting: "Wyznaczanie nowej trasy", recalculated: "Trasa wyznaczona ponownie", jam: "Korek", slow: "Wolny ruch", no_duty: "Jazda swobodna", last_stop: "Przystanek końcowy", on_time: "punktualnie" },
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
    /// The route ends this far along its last lane (the player's own destination, `nav_pins`),
    /// not at that lane's end.
    end: Option<f32>,
}

/// The key of the route to the player's own destination on a free drive (no trip's key is
/// ever this).
const DEST_KEY: &str = "\u{1}destination";

/// Roads around a centre, built into vertex buffer 0.
struct Roads {
    anchor: DVec2,
    lanes_seen: usize,
    verts: usize,
    built_at: f32,
}

pub struct Navigator {
    /// The current small-map texture in Scene::overlays, for the cockpit display.
    pub panel_overlay: Option<usize>,
    pub cockpit_display: bool,
    drawn_at: f32,
    /// Frames since the last drawing (see `NAV_REDRAW_FRAMES`).
    frames_since_drawn: u32,
    pub enabled: bool,
    /// The duty board under the map (`nav_duty`; Shift+N cycles map, map and board, off).
    pub schedule: bool,
    /// The board is wanted under the map (the `nav_board` setting, and its handle on the
    /// panel): it opens by itself on a duty and is a step of Shift+N; off, the map alone.
    pub board: bool,
    /// The board's handle was pressed: the setting to keep (taken by [`Navigator::take_board`]).
    board_changed: Option<bool>,
    /// The panel's size as the player dragged it, as shares of the window's height (the
    /// `nav_rect` setting's); None: its own size, from the interface's and `size`.
    pub custom: Option<[f32; 2]>,
    /// The place or the size changed in the game: `nav_rect` to keep ([`Navigator::take_rect`]).
    rect_changed: bool,
    /// How the small navigator was laid out last (its own pixels): the handle's place.
    layout: Option<crate::nav_panel::Layout>,
    /// What the mouse is over on the small navigator (the handle lit, the edge marked).
    panel_hover: Option<HoverPart>,
    /// The window (physical pixels) and the interface's pixels a point, when last placed.
    window: [f32; 2],
    unit: f32,
    /// A duty has been seen: the board opened by itself for it once.
    duty_seen: bool,
    /// Smoothed speed (m/s) for the time to the next stop.
    speed_avg: f32,
    pub opacity: f32,
    pub corner: String,
    /// Where the navigator was dragged to (#940): its top-left corner as a share of the
    /// room the window leaves it across and down (`navigator_corner = at x,y`); None: the
    /// corner.
    pub at: Option<[f32; 2]>,
    /// A press on the navigator: where it was, the panel as it was then, what it took hold of
    /// (the panel, or edges to size it by), and whether it has moved far enough to be a drag
    /// rather than a click.
    panel_drag: Option<PanelGrab>,
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
    building: Option<std::sync::mpsc::Receiver<(Network, HashMap<i64, DVec3>, Streets)>>,
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
    /// The navigator's own size on top of the interface's (`nav_scale`): the small panel and
    /// the city map's panels, texts and buttons. Ctrl + the wheel over either changes it, and
    /// the city map's - and + (the caller keeps it: [`Navigator::take_resized`]).
    pub size: f32,
    /// The wheel's notches towards the next step of the size (a touchpad gives parts of one).
    size_wheel: f32,
    /// The size changed in the game, not yet kept as the setting.
    resized: Option<f32>,
    /// When the size last changed (the navigator's clock): it says so for a moment.
    size_at: f32,
    /// While the duty waits to be signed for, the small navigator is the sign-on page from
    /// edge to edge (`nav_signon`, [`crate::nav_signon::Room::Panel`]): the page as it is laid
    /// out and where a press on it does what. None: the map.
    signing: Option<PanelPage>,
    /// The pairing QR code ([`crate::companion::pair_qr`]), and the address, code and time it
    /// was asked for (it is asked again when they change, and now and then).
    qr: (String, f32, Option<std::sync::Arc<crate::nav_signon::Qr>>),
    /// The player's own destination and waypoints, set on the city map (`nav_pins`): on a free
    /// drive the route goes to them, on a duty they are a diversion for the navigation only.
    pins: crate::nav_pins::Pins,
    /// The timetable's stops by their map objects, for the pins' names (given once).
    stop_names: Option<Vec<(i64, String)>>,
}

/// A press held on the small navigator: where the cursor was, the panel then (x0, y0, x1,
/// y1, the window's pixels without the origin), what it holds, and whether it has moved yet.
#[derive(Debug, Clone, Copy)]
struct PanelGrab {
    from: [f32; 2],
    rect: [f32; 4],
    grip: crate::nav_panel::Grip,
    moved: bool,
}

/// The small navigator as the sign-on page: the page as it fits, where a press does what
/// (the panel's pixels), and the stage it was laid out at.
struct PanelPage {
    fit: crate::nav_signon::Fitted,
    hits: Vec<(Rect, crate::nav_signon::Action)>,
    stage: Stage,
}

/// Each notch of Ctrl + the wheel, and each press of the city map's - and +, changes the
/// navigator's size by this much (the setting's own steps).
const SIZE_STEP: f32 = 0.05;
/// How long the new size shows after it changed (s).
const SIZE_NOTE: f32 = 1.4;

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

pub(crate) fn angle_diff(a: f64, b: f64) -> f64 {
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
            drawn_at: f32::MIN,
            frames_since_drawn: u32::MAX,
            enabled,
            schedule: false,
            board: true,
            board_changed: None,
            custom: None,
            rect_changed: false,
            layout: None,
            panel_hover: None,
            window: [0.0; 2],
            unit: 1.0,
            duty_seen: false,
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
            // (Omsi-Hub's typeface, as the launcher: the duty board is Omsi-Hub's overlay)
            fonts: Fonts::hanken(),
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
            shown: if enabled { 1.0 } else { 0.0 },
            stop_spots: Vec::new(),
            bus_at: DVec3::ZERO,
            next_turn: None,
            street_here: None,
            dist_t: 0.0,
            jam_cost: 0.0,
            first: true,
            size: 1.0,
            size_wheel: 0.0,
            resized: None,
            size_at: f32::MIN,
            signing: None,
            qr: (String::new(), f32::MIN, None),
            pins: crate::nav_pins::Pins::default(),
            stop_names: None,
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
                let m = world.navigation_map();
                let mut net = Network { lanes: m.lanes, ..Default::default() };
                net.link(1.5);
                confirm_road_surfaces(&mut net, &m.road_surfaces);
                probe_lanes(&net);
                let streets = build_streets(&net, &m.signs);
                let _ = tx.send((net, m.places, streets));
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
        // (the player's own pins: while a diversion of this trip runs its route stays, and the
        // trip's lanes are what it goes back onto; a free drive's destination ends when a duty
        // begins, a diversion with its trip)
        if self.pins.diverts(key) {
            // (the navigation map came meanwhile and took the route with it: the trip's again,
            // until a way through the pins is found on the new network - kept when none is)
            if self.route.lanes.is_empty() && !lanes.is_empty() {
                self.route.provisional = false;
                self.route.lanes = lanes.clone();
                self.route.end = None;
                self.route.progress = 0;
                self.route.on_route = false;
                self.route.off_for = 0.0;
                self.route.version += 1;
            }
            if self.pins.rest.is_empty() && !lanes.is_empty() {
                self.pins.rest = lanes;
                self.pins.dirty = true;
            }
            self.route.key = key.to_string();
            self.route.complete = complete;
            self.route.generation = generation;
            return;
        }
        if !self.pins.list.is_empty() || self.pins.diversion() {
            self.pins.finish();
        }
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
        self.route.end = None;
        if !same_trip {
            self.route.progress = 0;
            self.route.on_route = false;
            self.route.off_for = 0.0;
        }
        self.route.version += 1;
    }

    /// No duty: nothing to follow but the player's own destination (`nav_pins`). A diversion
    /// ends with its trip.
    pub fn clear_route(&mut self) {
        if self.pins.diversion() {
            self.pins.finish();
        }
        if self.route.key == DEST_KEY && !self.pins.list.is_empty() {
            return;
        }
        if !self.route.key.is_empty() || !self.route.lanes.is_empty() {
            self.route = Route { version: self.route.version + 1, ..Route::default() };
        }
    }


    /// Where the bus is along the route, and a way back when it left it.
    fn follow(&mut self, f: &NavFrame) {
        let global = self.global.clone();
        let Some(net) = global.as_deref().or(f.traffic.map(|t| &t.net)) else { return };
        // the player's own pins: what the city map asked, the one reached, a new plan
        self.pins_step(net, f);
        let pinned = self.pins.steers();
        let r = &mut self.route;
        if r.lanes.is_empty() && pinned {
            // (no way through the pins from here - no road near the bus yet: tried again now
            // and then)
            r.retry_in -= f.dt;
            if r.retry_in <= 0.0 {
                r.retry_in = REROUTE_EVERY * 2.0;
                self.plan_pins(net, f, true);
            }
            return;
        }
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
        // (before the bus first reached the route: only the stretch up to the next stop; the
        // pins' route begins where the bus is)
        let (from, to) = match (r.joined || pinned, stop_at) {
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
        if pinned {
            // (off the pins' route: a new one from the bus through the pins still ahead)
            self.plan_pins(net, f, true);
            return;
        }
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

    /// The timetable's stops have not been given yet (they name the player's pins).
    pub fn wants_stop_names(&self) -> bool {
        self.stop_names.is_none()
    }

    /// The timetable's stops by their map objects (`Schedule::stop_names`): a pin near one is
    /// called after it.
    pub fn set_stop_names(&mut self, names: impl IntoIterator<Item = (i64, String)>) {
        self.stop_names = Some(names.into_iter().collect());
    }

    /// The player's pins, once a frame: what the city map asked of them, the one the bus
    /// reached, and a new route through them when they changed.
    fn pins_step(&mut self, net: &Network, f: &NavFrame) {
        use crate::nav_pins::Note;
        self.pins.tick(f.dt);
        for op in std::mem::take(&mut self.pins.ops) {
            self.pin_op(net, f, op);
        }
        // at the destination: the route and the pins go after a moment
        if let Some(t) = self.pins.arrived.as_mut() {
            *t -= f.dt;
            if *t <= 0.0 {
                self.end_pins(net, f);
            }
            return;
        }
        if let Some(Note::Reached(n)) = self.pins.pass(f.bus) {
            log::info!("navigator: via {n} of the player's pins reached");
            if self.pins.list.is_empty() {
                self.diversion_done(net, f);
            }
        }
        if self.pins.dirty {
            self.plan_pins(net, f, false);
        }
    }

    /// Do what the city map asked of the pins.
    fn pin_op(&mut self, net: &Network, f: &NavFrame, op: crate::nav_pins::Op) {
        use crate::nav_pins::{self as pins, Note, Op, Pin};
        match op {
            Op::Add(p) => {
                // (set while the last destination still says "arrived": a new drive)
                if self.pins.arrived.is_some() {
                    self.end_pins(net, f);
                }
                let Some(at) = pins::snap(net, p, pins::SNAP_REACH) else {
                    self.pins.say(Note::NoRoad);
                    return;
                };
                if self.pins.list.is_empty() {
                    // a new set: a diversion of the trip whose route this is, or a free drive's
                    // destination; a diversion goes back onto the trip's route ahead
                    let key = &self.route.key;
                    self.pins.trip = (!key.is_empty() && key != DEST_KEY).then(|| key.clone());
                    self.pins.passed = 0;
                    let r = &self.route;
                    self.pins.rest = if self.pins.trip.is_some() && !r.provisional { r.lanes[r.progress.min(r.lanes.len())..].to_vec() } else { Vec::new() };
                }
                self.pins.made += 1;
                let name = self.pin_name(net, at.at, self.pins.made);
                let pin = Pin { at: at.at, name, n: self.pins.made };
                let diversion = self.pins.diversion();
                match pins::add(&mut self.pins.list, pin, diversion) {
                    Some(_) => self.pins.dirty = true,
                    None => self.pins.say(Note::Full),
                }
            }
            Op::Move(k, to, from) => {
                let Some(pin) = self.pins.list.get(k).cloned() else { return };
                match pins::snap(net, to, pins::SNAP_REACH) {
                    Some(at) => {
                        let name = self.pin_name(net, at.at, pin.n);
                        self.pins.list[k] = Pin { at: at.at, name, n: pin.n };
                        self.pins.dirty = true;
                    }
                    None => {
                        self.pins.list[k].at = from;
                        self.pins.say(Note::NoRoad);
                    }
                }
            }
            Op::Remove(k) => {
                if k < self.pins.list.len() {
                    self.pins.list.remove(k);
                    self.pins.dirty = true;
                }
                if self.pins.list.is_empty() {
                    self.end_pins(net, f);
                }
            }
            Op::Shift(k, up) => {
                if pins::shift(&mut self.pins.list, k, up) {
                    self.pins.dirty = true;
                }
            }
            Op::Clear => self.end_pins(net, f),
        }
    }

    /// A pin's name at `at`: the stop nearest within 200 m, else the street, else "Point n".
    fn pin_name(&self, net: &Network, at: DVec2, n: u32) -> String {
        let stops: Vec<(DVec2, String)> = self.stop_names.iter().flatten().filter_map(|(id, name)| self.stop_pos.get(id).map(|p| (p.truncate(), name.clone()))).collect();
        let street = crate::nav_pins::snap(net, at, 15.0).and_then(|s| self.street_of(s.lane));
        crate::nav_pins::name(at, &stops, street, n)
    }

    /// A route through the pins from the bus (`recalculated`: because the bus left it), for
    /// the navigator to follow as it follows a trip's. A diversion keeps the trip's route when
    /// no way leads through its pins.
    fn plan_pins(&mut self, net: &Network, f: &NavFrame, recalculated: bool) {
        use crate::nav_pins::Note;
        self.pins.dirty = false;
        if !self.pins.steers() {
            return;
        }
        let Some(lane) = start_lane(net, f.bus, f.heading) else { return };
        let (s, d) = net.lanes[lane].nearest_point(f.bus).unwrap_or((0.0, f64::MAX));
        let at: Vec<DVec2> = self.pins.list.iter().map(|p| p.at).collect();
        let rest = Some(self.pins.rest.as_slice()).filter(|r| self.pins.diversion() && !r.is_empty());
        let plan = crate::nav_pins::plan(net, (lane, s), &at, rest);
        if plan.marks.iter().any(Option::is_none) && !recalculated {
            self.pins.say(Note::NoWay);
        }
        if plan.lanes.is_empty() {
            // (no way from here to the first pin: a diversion keeps the trip's route, a free
            // drive has none for now - tried again now and then)
            if !self.pins.diversion() && !self.route.lanes.is_empty() {
                self.route = Route { key: DEST_KEY.to_string(), version: self.route.version + 1, retry_in: REROUTE_EVERY * 2.0, ..Route::default() };
            }
            self.pins.marks = plan.marks;
            self.pins.join = None;
            return;
        }
        log::info!("navigator: {} lanes through {} of the player's pins{}", plan.lanes.len(), at.len(), if plan.join.is_some() { ", back onto the trip's route" } else { "" });
        self.pins.marks = plan.marks;
        self.pins.join = plan.join;
        let r = &mut self.route;
        if !self.pins.diversion() {
            r.key = DEST_KEY.to_string();
            r.complete = true;
        }
        // (a bus standing off the roads - a depot - is not on it yet: no "recalculated" then)
        let near = d < 16.0;
        if recalculated && r.joined && near {
            r.note = 4.0;
        }
        r.lanes = plan.lanes;
        r.progress = 0;
        r.s = plan.s0;
        r.end = plan.end;
        r.on_route = near;
        r.joined = near;
        r.approach = false;
        r.provisional = false;
        r.off_for = 0.0;
        r.retry_in = REROUTE_EVERY;
        r.version += 1;
        // (the distances on the card and in the header at once)
        self.dist_t = 0.0;
    }

    /// How far along the route each pin is, now.
    fn pin_distances(&mut self, net: &Network) {
        let r = &self.route;
        self.pins.dist = self.pins.marks.iter().map(|m| m.and_then(|m| crate::nav_pins::along(net, &r.lanes, r.progress, r.s, m))).collect();
    }

    /// The pins gone (cleared, or the destination reached a moment ago): a free drive's route
    /// goes with them; a diversion's goes back to the trip's, unless the bus is on it again.
    fn end_pins(&mut self, net: &Network, f: &NavFrame) {
        if self.pins.diversion() {
            let rest = std::mem::take(&mut self.pins.rest);
            let rejoined = self.pins.join.is_some_and(|j| self.route.progress >= j);
            if !rejoined && !rest.is_empty() {
                self.back_to_trip(net, f, rest);
            }
        } else if self.route.key == DEST_KEY {
            self.route = Route { version: self.route.version + 1, ..Route::default() };
        }
        self.pins.finish();
    }

    /// A diversion's last via was reached: on along the trip's route it rejoins - or, when no
    /// way led back onto it, the trip's route again (the bus finds its way back to it).
    fn diversion_done(&mut self, net: &Network, f: &NavFrame) {
        let rest = std::mem::take(&mut self.pins.rest);
        if self.pins.join.is_none() && !rest.is_empty() {
            self.back_to_trip(net, f, rest);
        }
        self.pins.finish();
    }

    /// The trip's route `rest` again: where the bus is on it, else a way back to it is looked
    /// for at once (as after leaving it).
    fn back_to_trip(&mut self, net: &Network, f: &NavFrame, rest: Vec<usize>) {
        let at = crate::nav_pins::locate(net, &rest, f.bus, f.heading);
        let r = &mut self.route;
        r.lanes = rest;
        r.end = None;
        r.joined = true;
        r.approach = false;
        r.provisional = false;
        r.version += 1;
        match at {
            Some((k, s)) => {
                r.progress = k;
                r.s = s;
                r.on_route = true;
                r.off_for = 0.0;
            }
            None => {
                r.progress = 0;
                r.s = 0.0;
                r.on_route = false;
                r.off_for = OFF_ROUTE_AFTER;
                r.retry_in = 0.0;
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
            if let Ok((net, pos, streets)) = rx.try_recv() {
                log::info!("navigator: the map's road network is there ({} lanes, {} streets named)", net.lanes.len(), streets.names.len());
                self.streets = Some(std::sync::Arc::new(streets));
                self.global = Some(std::sync::Arc::new(net));
                self.stop_pos = std::sync::Arc::new(pos);
                self.global_version += 1;
                self.building = None;
                // routes and roads again on the whole map
                self.roads = None;
                self.route = Route { version: self.route.version + 1, ..Route::default() };
                self.route_mesh.0 = u64::MAX;
                self.route_jam.clear();
                // (the player's pins stay where they are; their way, on the new network - and
                // a diversion goes back onto the trip's route as the new network has it)
                if !self.pins.list.is_empty() {
                    self.pins.rest.clear();
                    self.pins.marks.clear();
                    self.pins.join = None;
                    self.pins.dirty = true;
                }
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
            if let Some(n) = net {
                self.pin_distances(n);
            }
            self.street_here = if self.route.on_route {
                self.route.lanes.get(self.route.progress).and_then(|&l| self.street_of(l))
            } else {
                global.as_deref().and_then(|g| g.nearest_lane_near(f.bus, LaneKind::Street)).filter(|l| l.2 < 10.0).and_then(|l| self.street_of(l.0))
            }
                .map(str::to_string);
        }
        self.first = false;
        if !self.enabled && self.shown < 0.01 {
            self.panel_rect = [0.0; 4];
            self.signing = None;
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
        // (the interface's pixels a point here: the smallest panel and its scale's range)
        let unit = base * f.ui_scale / crate::nav_panel::WIDTH;
        self.unit = unit;
        self.window = [sw, sh];
        // (the navigator's own size on top of the interface's; the cockpit's display is the
        // bus's, as large as it is)
        let size = if self.cockpit_display { 1.0 } else { self.size };
        let margin = (sh * 0.018).max(10.0).round();
        // (with the on-screen controls the corners are theirs: the top middle)
        let touch = crate::platform::touch_controls();
        // the size the player dragged it to (by its corners and edges): what is in it is laid
        // out for that shape; else its own, as wide as the interface makes it
        let custom = self
            .custom
            .filter(|_| !self.cockpit_display && !touch)
            .map(|c| crate::nav_panel::clamp_size(c[0] * sh, c[1] * sh, self.min_size(), [sw, sh]));
        let pw = match custom {
            Some((w, _)) => w,
            None => (base * f.ui_scale * size).min((sh * 0.7).max(300.0)).min((sw * 0.9).max(300.0)).round(),
        };
        // the duty board under the map (Shift+N's second step, the handle on the bar): as
        // many stops ahead as there is room for, four at most
        let duty = f.duty.and_then(|d| crate::nav_duty::DutyState::of(d, f.time));
        // (on a duty the board opens by itself, once - Omsi-Hub's duty overlay was there
        // whenever a duty ran - unless the player wants the map alone; Shift+N and the
        // handle put it away)
        if duty.is_some() && !self.duty_seen {
            self.duty_seen = true;
            self.schedule = self.board;
        }
        // (until the duty is signed for the whole navigator is the sign-on page - Omsi-Hub's
        // duty panel was "sign on first" and nothing else until then; signed, the map and the
        // board. Shift+N's map alone leaves it out, as it leaves out the board)
        self.signing = None;
        if self.schedule && duty.is_some() {
            let companion = crate::companion::state();
            if crate::nav_signon::waiting(&companion) {
                let (s, room) = match custom {
                    Some((cw, ch)) => (crate::nav_panel::scale(cw, ch, unit * size, false).0, ch),
                    None => (pw / crate::nav_panel::WIDTH, sh - 2.0 * margin),
                };
                self.signing = Some(self.panel_page(&companion, duty.as_ref(), f.time, pw, s, room, custom.is_some()));
            }
        }
        let handle = duty.is_some() || self.schedule;
        let (ph, board) = if let Some(p) = self.signing.as_ref() {
            self.layout = None;
            (p.fit.height.round(), Vec::new())
        } else if let Some((cw, ch)) = custom {
            // (the navigator's own size on top: its texts as large as they fit in the shape)
            let (s, beside) = crate::nav_panel::scale(cw, ch, unit * size, self.schedule);
            let (rows, bh) = if self.schedule { crate::nav_duty::board_within(duty.as_ref(), s, crate::nav_panel::board_room(ch, s, beside)) } else { (Vec::new(), 0.0) };
            let shown = !rows.is_empty();
            self.layout = Some(crate::nav_panel::layout(cw, ch, s, shown.then_some(bh), beside, handle));
            (ch, rows)
        } else {
            let s = pw / crate::nav_panel::WIDTH;
            let map_h = (pw * 0.62).round();
            let bars = (crate::nav_panel::HEAD * s).round() + (crate::nav_panel::NEXT * s).round();
            let room = sh - 2.0 * margin - map_h - bars;
            let (rows, sched) = if self.schedule { crate::nav_duty::board_fitting(duty.as_ref(), s, room) } else { (Vec::new(), 0.0) };
            let ph = (map_h + bars + sched).round();
            self.layout = Some(crate::nav_panel::layout(pw, ph, s, (!rows.is_empty()).then_some(sched), false, handle));
            (ph, rows)
        };
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
            match self.layout {
                Some(lay) if self.signing.is_none() => self.draw(renderer, &view, (w, h), &lay, f, &board),
                _ => self.draw_signon(renderer, &view, (w, h)),
            }
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

    /// Draw the small navigator into its texture (`size` pixels), laid out as `lay`.
    fn draw(&mut self, renderer: &Renderer, target: &wgpu::TextureView, size: (u32, u32), lay: &crate::nav_panel::Layout, f: &NavFrame, board: &[crate::nav_duty::Row]) {
        let reach = self.map_zoom(lay) * 3.5 + 150.0;
        let vehicles = map_vehicles(f.traffic.filter(|_| self.show_ai), f.bus, reach);
        let p = self.paint_panel((size.0 as f32, size.1 as f32), lay, f, board, &vehicles);
        let (Some(gpu), device, queue) = (self.gpu.as_mut(), &renderer.device, &renderer.queue) else { return };
        if let Some(v) = p.roads {
            if let Some(r) = self.roads.as_mut() {
                r.verts = v.len();
            }
            gpu.upload(device, queue, 0, &v);
        }
        if let Some(v) = p.route {
            gpu.upload(device, queue, 1, &v);
        }
        let (n_bg, n_traffic) = (p.bg.len(), p.traffic.len());
        let n_world = n_traffic + p.vehicles.len();
        let mut all = p.bg.verts;
        all.extend(p.traffic.verts);
        all.extend(p.vehicles.verts);
        let n_ui_start = all.len() as u32;
        all.extend(p.ui.verts);
        gpu.upload(device, queue, 2, &all);
        gpu.upload_atlas(queue, &mut self.atlas);
        let clip_panel = [0.0, 0.0, size.0 as f32, size.1 as f32];
        // (the opacity setting is the background's: the map and the text stay solid)
        let flat = Layer::flat(clip_panel, p.radius, 1.0);
        let backdrop = Layer::flat(clip_panel, p.radius, if self.cockpit_display { self.opacity } else { crate::ui::backdrop(self.opacity).min(1.0) });
        let mut layers = [flat, p.map_layer, backdrop];
        for l in layers.iter_mut() {
            l.opacity *= self.shown;
        }
        let roads_n = self.roads.as_ref().map(|r| r.verts as u32).unwrap_or(0);
        let draws = [
            Draw { buffer: 2, range: 0..n_bg, layer: 2, texture: 0 },
            Draw { buffer: 0, range: 0..roads_n, layer: 1, texture: 0 },
            Draw { buffer: 2, range: n_bg..n_bg + n_traffic, layer: 1, texture: 0 },
            Draw { buffer: 1, range: 0..self.route_mesh.3, layer: 1, texture: 0 },
            Draw { buffer: 2, range: n_bg + n_traffic..n_bg + n_world, layer: 1, texture: 0 },
            Draw { buffer: 2, range: n_ui_start..all.len() as u32, layer: 0, texture: 0 },
        ];
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("navigator") });
        gpu.render(device, queue, &mut enc, target, size, Some(wgpu::Color::TRANSPARENT), &layers, &draws);
        queue.submit([enc.finish()]);
    }

    /// The map camera's distance for the map's part of the panel: the eased zoom, made
    /// larger as the map is taller than the design's (a tall map shows more of the way, the
    /// things on it as large as ever).
    fn map_zoom(&self, lay: &crate::nav_panel::Layout) -> f64 {
        let design = 0.62 * crate::nav_panel::WIDTH * lay.s;
        self.zoom * (lay.map.h / design.max(1.0)).clamp(0.5, 3.0) as f64
    }

    /// The small navigator as painted, before it goes to the GPU (`size` pixels, laid out as
    /// `lay`), with the other `vehicles` on the map.
    fn paint_panel(&mut self, size: (f32, f32), lay: &crate::nav_panel::Layout, f: &NavFrame, board: &[crate::nav_duty::Row], vehicles: &[MapVehicle]) -> PanelPaint {
        let (pw, ph) = size;
        let s = lay.s;
        let wd = words(f.language);
        self.atlas.begin_frame();
        let panel = Rect::new(0.0, 0.0, pw, ph);
        let radius = 7.0 * s;
        let map = lay.map;
        let zoom = self.map_zoom(lay);
        let vp = [map.x, map.y, map.w, map.h.max(1.0)];

        // --- roads (buffer 0): rebuilt when the bus went far or tiles brought lanes
        let own = self.own_net.clone();
        let global = self.global.clone();
        let net = global.as_deref().or(f.traffic.map(|t| &t.net)).or(own.as_deref());
        let lanes_now = net.map(|n| n.lanes.len()).unwrap_or(0);
        let rebuild = match &self.roads {
            None => lanes_now > 0,
            Some(r) => (r.anchor - f.bus.truncate()).length() > ROAD_RADIUS * 0.45 || (r.lanes_seen != lanes_now && self.time - r.built_at > 1.5),
        };
        let mut road_verts = None;
        if rebuild {
            if let Some(net) = net {
                let anchor = f.bus.truncate();
                let mut p = Painter::new();
                let t0 = std::time::Instant::now();
                build_roads(&mut p, net, anchor);
                if omsi_cfg::flags::OMSI_DEBUG_NAV.is_set() {
                    log::info!("navigator: roads around ({:.0}, {:.0}) in {:.1} ms: {} vertices", anchor.x, anchor.y, t0.elapsed().as_secs_f64() * 1000.0, p.verts.len());
                }
                road_verts = Some(p.verts);
                self.roads = Some(Roads { anchor, lanes_seen: lanes_now, verts: 0, built_at: self.time });
            }
        }

        // --- the map camera (world coordinates relative to the anchor)
        let anchor = self.roads.as_ref().map(|r| r.anchor).unwrap_or(f.bus.truncate());
        let rel = |p: DVec3| Vec3::new((p.x - anchor.x) as f32, (p.y - anchor.y) as f32, 0.0);
        let hd = self.cam_heading.to_radians();
        let fwd = DVec2::new(hd.sin(), hd.cos());
        let look_at = f.bus.truncate() - anchor + fwd * zoom * 0.28;
        let pitch = PITCH.to_radians();
        let back = fwd * zoom * pitch.cos();
        let eye = DVec3::new(look_at.x - back.x, look_at.y - back.y, zoom * pitch.sin());
        let view = Mat4::look_at_rh(eye.as_vec3(), Vec3::new(look_at.x as f32, look_at.y as f32, 0.0), Vec3::Z);
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
                let style = RouteStyle { extra_m: -0.9, min_px: 4.0, arrows: Some((28.0, 900.0)), max_len: 12_000.0, near: Some((anchor, ROAD_RADIUS * 1.6)), end: self.route.end };
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
        bg.rounded(panel, radius, if self.cockpit_display { Color::rgba(10, 10, 10, 1.0) } else { PANEL });

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
        // the other vehicles: small cars in blue, as the other drivers in ETS2; the public
        // transport as buses in its own colours (trolleybus, bus, tram), its line above it -
        // turned as they head, a dot with a tip where they would be too small to make out
        let mut cars = Painter::new();
        let mpp_here = zoom as f32 * map_layer.px_scale;
        paint_vehicles(&mut cars, vehicles, &rel, mpp_here, s);
        let mut lines: Vec<(DVec3, Color, String, [DVec3; 2])> = vehicles.iter().filter_map(|v| v.line.clone().map(|l| (v.at, v.color, l, v.drawn_ends(mpp_here, s)))).collect();

        let mut ui = Painter::new();
        // the far end of the map fades into the panel
        ui.gradient(Rect::new(map.x, map.y, map.w, map.h * 0.3), Color::rgba(10, 10, 10, 0.75), Color::rgba(10, 10, 10, 0.0));
        // the next turn: an arrow and how far, top left of the map
        if let Some((dir, angle, dist, street)) = self.next_turn.as_ref() {
            let icon = match *dir {
                2 => "u_turn_left",
                -1 if *angle < 60.0 => "turn_slight_left",
                1 if *angle < 60.0 => "turn_slight_right",
                -1 => "turn_left",
                _ => "turn_right",
            };
            let t = if *dist >= 1000.0 { format!("{:.1} km", dist / 1000.0) } else { format!("{:.0} m", ((dist / 10.0).round() * 10.0).max(10.0)) };
            let tw = self.fonts.width(&t, 14.0 * s, Weight::Bold);
            // with the street it turns into, when the map names it
            let street = street.as_deref().map(|n| self.fonts.fit(n, 12.0 * s, Weight::Medium, map.w * 0.62 - 50.0 * s - tw));
            let sw_ = street.as_deref().map(|n| self.fonts.width(n, 12.0 * s, Weight::Medium) + 10.0 * s).unwrap_or(0.0);
            let b = Rect::new(map.x + 8.0 * s, map.y + 8.0 * s, 44.0 * s + tw + sw_, 34.0 * s);
            ui.rounded(b, 5.0 * s, Color::rgba(10, 10, 10, 0.85));
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
            ui.rounded(r, 9.0 * s, Color::rgba(10, 10, 10, 0.8));
            ui.text_in(&mut self.atlas, &self.fonts, &n, px, Weight::Medium, r, Align::Center, STREET);
        }
        // Stops as the stop sign the player chose (`stop_signs`): the next one larger, the
        // terminus ringed, the last two served faded behind the bus.
        let n_stops = f.stops.len();
        let signs = crate::stop_signs::style();
        let markers = spaced_markers(f.stops.iter().enumerate().filter_map(|(k, st)| project(vpm, vp, rel(st.position)).filter(|p| map.contains(*p)).map(|p| (k, p))), 20.0 * s);
        let served = crate::stop_signs::served(f.duty, &self.stop_pos);
        for q in served.iter().rev().take(2).filter_map(|q| project(vpm, vp, rel(*q))).filter(|p| map.contains(*p) && markers.iter().all(|(_, m)| m.distance(*p) >= 14.0 * s)) {
            crate::stop_signs::draw(&mut ui, signs, crate::stop_signs::Kind::Passed, q, 13.5 * s);
        }
        for (k, sp) in markers.into_iter().rev() {
            crate::stop_signs::draw(&mut ui, signs, crate::stop_signs::Kind::ahead(k, n_stops), sp, 13.5 * s);
        }
        // the player's own pins (`nav_pins`): the destination's flag, the vias numbered
        for k in (0..self.pins.list.len()).rev() {
            let Some(p) = project(vpm, vp, rel(self.pins.list[k].at.extend(0.0))).filter(|p| map.contains(*p)) else { continue };
            match self.pins.role(k) {
                crate::nav_pins::Role::Destination => crate::nav_pins::draw_destination(&mut ui, &mut self.atlas, p, 7.0 * s, self.pins.arrived.is_some(), 1.0),
                crate::nav_pins::Role::Via(n) => crate::nav_pins::draw_via(&mut ui, &mut self.atlas, &self.fonts, p, n, 7.0 * s, 1.0),
            }
        }
        // the public transport's lines: a tag in its colour above its dot (the nearest first,
        // none on top of another)
        lines.sort_by(|a, b| (a.0 - f.bus).length().total_cmp(&(b.0 - f.bus).length()));
        let mut taken: Vec<Rect> = Vec::new();
        // the other players: an arrow each, their name above it (before the lines' tags)
        for pl in &f.players {
            let Some(p) = project(vpm, vp, rel(pl.position)).filter(|p| map.contains(*p)) else { continue };
            let a = (angle_diff(self.cam_heading, pl.heading) as f32).to_radians();
            arrow(&mut ui, p, a, 7.5 * s, 1.25, Color::rgba(10, 10, 10, 0.8), PLAYER);
            let px = 9.5 * s;
            let name = self.fonts.fit(&pl.name, px, Weight::Bold, 90.0 * s);
            let w = self.fonts.width(&name, px, Weight::Bold) + 8.0 * s;
            let r = Rect::new(p.x - w * 0.5, p.y - 23.0 * s, w, 13.0 * s);
            if taken.iter().any(|o| rects_overlap(o, &r)) {
                continue;
            }
            taken.push(r);
            ui.rounded(Rect::new(r.x - 1.0 * s, r.y - 1.0 * s, r.w + 2.0 * s, r.h + 2.0 * s), 4.0 * s, Color::rgba(10, 10, 10, 0.9));
            ui.rounded(r, 3.5 * s, PLAYER);
            ui.text_in(&mut self.atlas, &self.fonts, &name, px, Weight::Bold, r, Align::Center, LINE_TEXT);
        }
        for (pos, color, l, ends) in &lines {
            let Some(p) = project(vpm, vp, rel(*pos)).filter(|p| map.contains(*p)) else { continue };
            let px = 9.5 * s;
            // (a rounded chip in its kind's colour, the duty's own line on its yellow plate -
            // above the bus's end that is higher on the map, not over the bus)
            let own = f.line.as_deref().is_some_and(|o| o.trim().eq_ignore_ascii_case(l.trim()));
            let l = self.fonts.fit(l, px, Weight::Bold, 40.0 * s);
            let w = self.fonts.width(&l, px, Weight::Bold) + crate::stop_signs::CHIP_PAD * s;
            let top = ends.iter().filter_map(|e| project(vpm, vp, rel(*e))).fold(p.y, |t, e| t.min(e.y));
            let r = Rect::new(p.x - w * 0.5, top.min(p.y - 6.0 * s) - 15.0 * s, w, 13.0 * s);
            if r.y < map.y || taken.iter().any(|o| o.x < r.right() && r.x < o.right() && o.y < r.bottom() && r.y < o.bottom()) {
                continue;
            }
            taken.push(r);
            crate::stop_signs::draw_chip(&mut ui, &mut self.atlas, &self.fonts, r, &l, px, *color, own, s);
        }
        // the bus: a plain white arrow
        if let Some(bp) = project(vpm, vp, rel(f.bus)) {
            let a = (angle_diff(self.cam_heading, f.heading) as f32).to_radians();
            arrow(&mut ui, bp, a, 9.0 * s, 1.25, Color::rgba(10, 10, 10, 0.8), TEXT);
        }

        // top bar: speed (and the limit) · line ……… game time
        let top = lay.head;
        ui.rect(top, BAR);
        let pad = 11.0 * s;
        let base = top.y + top.h * 0.5 + self.fonts.cap_height(17.0 * s, Weight::Bold) * 0.5;
        let mut x = pad;
        // (in km/h, or in miles an hour where the player chose it: `crate::units`)
        x += ui.text(&mut self.atlas, &self.fonts, &format!("{:.0}", crate::units::speed(f.speed_kmh.abs())), 17.0 * s, Weight::Bold, Vec2::new(x, base), Align::Left, TEXT);
        x += 4.0 * s;
        x += ui.text(&mut self.atlas, &self.fonts, crate::units::label(wd.kmh), 12.0 * s, Weight::Medium, Vec2::new(x, base), Align::Left, TEXT_DIM);
        let limit = net.and_then(|n| {
            let lane = if self.route.on_route { self.route.lanes.get(self.route.progress).copied() } else { None };
            let lane = lane.or_else(|| n.nearest_lane_near(f.bus, LaneKind::Street).filter(|l| l.2 < 8.0).map(|l| l.0))?;
            let v = n.lanes.get(lane)?.speed_limit_kmh;
            (v > 1.0 && v < 200.0).then_some(v)
        });
        if let Some(v) = limit {
            x += 10.0 * s;
            let c = Vec2::new(x + 10.0 * s, top.center().y);
            ui.circle(c, 10.5 * s, Color::rgba(200, 40, 40, 1.0));
            ui.circle(c, 8.3 * s, Color::rgba(235, 235, 235, 1.0));
            let t = format!("{:.0}", crate::units::sign(v));
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

        // bottom bar: the next stop (or the player's own destination); its distance, the time
        // to it, the planned time and whether the bus is early or late
        let next = lay.next_text();
        self.paint_next(&mut ui, f, next, s, board);
        // (the board's handle at the bar's right end: a chevron - down to show the board,
        // up to put it away; beside the map, towards it)
        if let Some(h) = lay.handle {
            ui.rect(Rect::new(next.right(), lay.next.y, lay.next.right() - next.right(), lay.next.h), BAR);
            let on = self.schedule;
            let lit = self.panel_hover == Some(HoverPart::Handle);
            ui.rounded(h, 6.0 * s, Color::WHITE.alpha(if lit { 0.2 } else { 0.08 }));
            ui.rounded_border(h, 6.0 * s, 1.0, Color::WHITE.alpha(if lit { 0.35 } else { 0.16 }));
            let icon = match (on, lay.beside) {
                (true, true) => "chevron_right",
                (true, false) => "expand_less",
                (false, _) => "expand_more",
            };
            ui.icon(&mut self.atlas, icon, h.center(), h.w * 0.8, if on { TEXT_DIM } else { crate::nav_duty::NOW });
        }
        // the duty board (Omsi-Hub's duty overlay): the trip, its stops behind and ahead
        // with their times, and what comes after it - under the bar, or beside the map
        if let Some(area) = lay.board.filter(|_| self.schedule && !board.is_empty()) {
            if lay.beside {
                ui.rect(Rect::new(area.x, area.y, 1.0, area.h), Color::WHITE.alpha(0.1));
            }
            crate::nav_duty::draw_board(&mut crate::nav_duty::Pen { p: &mut ui, atlas: &mut self.atlas, fonts: &self.fonts }, board, area, s);
        }
        // (the size just changed with Ctrl + the wheel: the new one, over the map)
        self.size_note(&mut ui, Vec2::new(map.center().x, map.y + 22.0 * s), s);
        // (the mouse over an edge or a corner: a mark along it, where a drag sizes the panel)
        if let Some(HoverPart::Grip(crate::nav_panel::Grip::Size { left, right, top, bottom })) = self.panel_hover {
            let (t, c) = (3.0 * s.max(1.0), crate::nav_duty::NOW.alpha(0.9));
            let (len_x, len_y) = ((pw * 0.25).min(80.0 * s), (ph * 0.25).min(80.0 * s));
            let (x0, x1) = if left { (0.0, len_x) } else if right { (pw - len_x, pw) } else { (pw * 0.5 - len_x, pw * 0.5 + len_x) };
            let (y0, y1) = if top { (0.0, len_y) } else if bottom { (ph - len_y, ph) } else { (ph * 0.5 - len_y, ph * 0.5 + len_y) };
            if top || bottom {
                ui.rect(Rect::new(x0, if top { 0.0 } else { ph - t }, x1 - x0, t), c);
            }
            if left || right {
                ui.rect(Rect::new(if left { 0.0 } else { pw - t }, y0, t, y1 - y0), c);
            }
        }

        PanelPaint { bg, traffic: dy, vehicles: cars, ui, roads: road_verts, route: route_verts, map_layer, radius }
    }
}

/// The small navigator as painted, before it goes to the GPU: the background, the jams on
/// the map, the other vehicles (both in the map's world, relative to the roads' anchor),
/// what lies over it, the roads' and the route's meshes when they were built again, the map's
/// view and the panel's corner radius.
struct PanelPaint {
    bg: Painter,
    traffic: Painter,
    vehicles: Painter,
    ui: Painter,
    roads: Option<Vec<omsi_ui::Vertex>>,
    route: Option<Vec<omsi_ui::Vertex>>,
    map_layer: Layer,
    radius: f32,
}

/// What the mouse is over on the small navigator: the board's handle, or what a press
/// there takes hold of.
#[derive(Debug, Clone, Copy, PartialEq)]
enum HoverPart {
    Handle,
    Grip(crate::nav_panel::Grip),
}

/// Another vehicle as the maps draw it: the middle of its body (world), its heading, its
/// length and width (m), its colour (cars blue, the public transport by kind), whether it is
/// public transport (drawn as a bus) and the line it shows.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MapVehicle {
    pub at: DVec3,
    pub heading: f64,
    pub len: f32,
    pub width: f32,
    pub color: Color,
    pub bus: bool,
    pub line: Option<String>,
}

/// The traffic's vehicles within `reach` metres of `bus`, as the maps draw them.
fn map_vehicles(traffic: Option<&Traffic>, bus: DVec3, reach: f64) -> Vec<MapVehicle> {
    let Some(t) = traffic else { return Vec::new() };
    t.cars
        .iter()
        .filter(|c| !c.gone && (c.vehicle.position - bus).truncate().length() <= reach)
        .map(|c| {
            let (color, line) = traffic_kind(c);
            let h = c.vehicle.heading.to_radians();
            let (front, rear) = (c.state.front.max(0.5), c.state.rear.max(0.5));
            // (the body's middle: its origin is where its front and rear are measured from)
            let mid = c.vehicle.position.truncate() + DVec2::new(h.sin(), h.cos()) * ((front - rear) * 0.5) as f64;
            MapVehicle { at: mid.extend(c.vehicle.position.z), heading: c.vehicle.heading, len: front + rear, width: (c.half_width * 2.0).max(1.2), color, bus: color != DOT, line }
        })
        .collect()
}

/// The other vehicles onto the map `p` (its world, `rel` giving a world point relative to its
/// anchor), at `mpp` metres a pixel where they stand and scale `s`: each turned as it heads, at
/// its own size or - when that is small - a little larger, with a dark outline; a dot with a tip
/// where even that would be too small.
fn paint_vehicles(p: &mut Painter, vehicles: &[MapVehicle], rel: &dyn Fn(DVec3) -> Vec3, mpp: f32, s: f32) {
    use crate::nav_panel::{self as np, Tone};
    let outline = Color::rgba(8, 8, 8, 0.9);
    // (the cars first: the buses over them)
    for pass in [false, true] {
        for v in vehicles.iter().filter(|v| v.bus == pass) {
            let at = rel(v.at);
            match vehicle_size(v, mpp, s) {
                Some((_, width)) => {
                    // (metres, but at least `min_px` long: a scale of at least that a metre;
                    // wide enough to make out as a bus or a car)
                    let px_m = min_len(v, s) / v.len.max(1.0);
                    let parts = np::vehicle_shape(v.len, width, v.bus);
                    let edge = (1.1 * mpp).clamp(0.25, 0.9);
                    p.world_shape(at, &np::turned(&np::grown(&parts[0].1, edge), v.heading as f32), 1.0, px_m, outline);
                    for (tone, pts) in &parts {
                        let c = match tone {
                            Tone::Body => v.color,
                            Tone::Glass => Color::rgba(18, 24, 34, 0.9),
                            Tone::Roof => v.color.lighten(0.28),
                            Tone::Joint => v.color.darken(0.45),
                        };
                        p.world_shape(at, &np::turned(pts, v.heading as f32), 1.0, px_m, c);
                    }
                }
                None => {
                    let (disc, tip) = np::dot_shape();
                    let r = if v.bus { 3.4 } else { 2.6 } * s;
                    let turn = |pts: &[Vec2]| np::turned(pts, v.heading as f32);
                    p.world_shape(at, &turn(&np::grown(&tip, 0.3)), 0.0, r, outline);
                    p.world_shape(at, &turn(&disc), 0.0, r + 1.2 * s, outline);
                    p.world_shape(at, &turn(&disc), 0.0, r, v.color);
                    p.world_shape(at, &turn(&tip), 0.0, r, v.color);
                }
            }
        }
    }
}

/// The least length a vehicle is drawn at as its shape (pixels at scale `s`).
fn min_len(v: &MapVehicle, s: f32) -> f32 {
    (if v.bus { 15.0 } else { 9.0 }) * s
}

/// How a vehicle is drawn at `mpp` metres a pixel and scale `s`: as its shape - how many
/// times its own size, and its width (m) as drawn - or None: as a dot.
fn vehicle_size(v: &MapVehicle, mpp: f32, s: f32) -> Option<(f32, f32)> {
    let min_px = min_len(v, s);
    (crate::nav_panel::glyph(v.len, mpp, min_px) == crate::nav_panel::Glyph::Shape).then(|| {
        let k = (min_px / v.len.max(1.0) * mpp).max(1.0);
        let min_w = if v.bus { 6.0 } else { 5.0 } * s;
        (k, v.width.max(min_w * mpp / k))
    })
}

impl MapVehicle {
    /// Its front and its back as drawn at `mpp` metres a pixel and scale `s` (a dot: its
    /// middle), for the line's tag to stand above.
    fn drawn_ends(&self, mpp: f32, s: f32) -> [DVec3; 2] {
        let half = vehicle_size(self, mpp, s).map(|(k, _)| self.len * k * 0.5).unwrap_or(0.0) as f64;
        let h = self.heading.to_radians();
        let ahead = DVec3::new(h.sin(), h.cos(), 0.0) * half;
        [self.at + ahead, self.at - ahead]
    }
}

impl Navigator {
    /// The small navigator's bottom bar `bottom` at scale `s`: the next stop with its distance,
    /// the time to it and its planned time, and how the bus stands against the timetable
    /// (unless the duty board under it - `board` - says so on its chip); on a diversion of the
    /// player's own, that and its next via in place of the way to the stop. On a free drive
    /// the player's own destination where the next stop goes, with how far, how long and when
    /// there, and the next via on the right; else what the drive is. A note of the pins' or of
    /// the route's takes the second line for a moment.
    fn paint_next(&mut self, ui: &mut Painter, f: &NavFrame, bottom: Rect, s: f32, board: &[crate::nav_duty::Row]) {
        use crate::nav_pins as pins;
        let wd = words(f.language);
        let pad = 11.0 * s;
        ui.rect(bottom, BAR);
        let dest = self.pins.list.last().filter(|_| !self.pins.diversion());
        let stop_row = if f.stops.is_empty() && dest.is_none() {
            Rect::new(bottom.x + pad, bottom.y, bottom.w - 2.0 * pad, bottom.h)
        } else {
            Rect::new(bottom.x + pad, bottom.y + 4.0 * s, bottom.w - 2.0 * pad, 20.0 * s)
        };
        let stop_row = stop_request_icon(ui, &mut self.atlas, f.stop_requested, stop_row, s);
        let y2 = Rect::new(bottom.x + pad, bottom.y + 24.0 * s, bottom.w - 2.0 * pad, 18.0 * s);
        // (the pins' note first - a via reached, arrived - then the route's)
        let note: Option<(String, Color)> = if let Some((n, _)) = self.pins.note {
            Some((n.text(), if n.good() { ON_TIME } else { WARN }))
        } else if self.route.note > 0.0 {
            Some((wd.recalculated.to_string(), ON_TIME))
        } else if self.route.joined && !self.route.lanes.is_empty() && !self.route.on_route && self.route.off_for > OFF_ROUTE_AFTER {
            // (no way back found after a while: it says so instead of recalculating for ever)
            Some(((if self.route.off_for < OFF_ROUTE_AFTER + 20.0 { wd.rerouting } else { wd.off_route }).to_string(), WARN))
        } else {
            None
        };
        // what the traffic on the route ahead costs, as the module promises: a jam from a
        // minute on, slow traffic from half of one
        let jam_note = (self.jam_cost >= 30.0).then(|| {
            let what = if self.jam_cost >= 60.0 { wd.jam } else { wd.slow };
            (format!("{what} +{:.0} min", (self.jam_cost / 60.0).max(1.0).round()), if self.jam_cost >= 60.0 { LATE } else { WARN })
        });
        // the next via of the player's own, and how far along the route it is
        let via = self.pins.list.first().filter(|_| self.pins.steers() && (self.pins.diversion() || self.pins.list.len() > 1)).map(|_| {
            let label = self.pins.role(0).label();
            match self.pins.dist.first().copied().flatten() {
                Some(d) => format!("{label} · {}", pins::distance_text(d)),
                None => label,
            }
        });
        match (f.stops.first(), dest) {
            (Some(st), _) => {
                let name = if f.stops.len() == 1 { format!("{} | {}", st.name.trim(), wd.last_stop) } else { st.name.trim().to_string() };
                ui.text_in(&mut self.atlas, &self.fonts, &name, 13.5 * s, Weight::Bold, stop_row, Align::Left, TEXT);
                let mut parts = Vec::new();
                // (a diversion of the player's own: where it goes first; the timetable's next
                // stop stays the next stop)
                let diverted = via.is_some() && self.pins.diversion();
                if let Some(v) = via.clone().filter(|_| diverted) {
                    parts.push(omsi_ui::tr("Diversion").into_owned());
                    parts.push(v);
                } else if let Some(d) = self.next_dist {
                    parts.push(pins::distance_text(d));
                    parts.push(pins::eta_text(pins::eta(d, self.speed_avg)));
                }
                parts.push(format!("{:02}:{:02}", (st.arrival / 3600.0) as i32 % 24, ((st.arrival % 3600.0) / 60.0) as i32));
                let line2 = parts.join("  |  ");
                match (&note, &jam_note) {
                    (Some((t, c)), _) => {
                        ui.text_in(&mut self.atlas, &self.fonts, t, 12.5 * s, Weight::Medium, y2, Align::Left, *c);
                    }
                    (None, Some((t, c))) => {
                        ui.text_in(&mut self.atlas, &self.fonts, &format!("{line2}  |  {t}"), 12.5 * s, Weight::Medium, y2, Align::Left, *c);
                    }
                    (None, None) => {
                        ui.text_in(&mut self.atlas, &self.fonts, &line2, 12.5 * s, Weight::Medium, y2, Align::Left, if diverted { crate::nav_duty::NOW } else { TEXT_DIM });
                    }
                }
                // (OMSI 2's punctuality, as the duty board's; the board's chip says it when it
                // is there)
                let on_board = board.iter().any(|r| matches!(r, crate::nav_duty::Row::Head { .. }));
                if let Some(d) = f.delay.filter(|_| !on_board) {
                    let (txt, c) = match crate::nav_duty::punctuality(d) {
                        crate::nav_duty::Punctuality::Late => (crate::nav_duty::offset(d), LATE),
                        crate::nav_duty::Punctuality::Early => (crate::nav_duty::offset(d), EARLY),
                        crate::nav_duty::Punctuality::OnTime => (wd.on_time.to_string(), ON_TIME),
                    };
                    ui.text_in(&mut self.atlas, &self.fonts, &txt, 12.5 * s, Weight::Bold, y2, Align::Right, c);
                }
            }
            (None, Some(pin)) => {
                // the player's own destination, where a duty's next stop is: its flag and name;
                // how far, how long and when there
                let arrived = self.pins.arrived.is_some();
                ui.icon(&mut self.atlas, if arrived { "check" } else { "sports_score" }, Vec2::new(stop_row.x + 8.0 * s, stop_row.center().y), 16.0 * s, if arrived { ON_TIME } else { pins::PIN_LIGHT });
                let name_r = Rect::new(stop_row.x + 21.0 * s, stop_row.y, (stop_row.w - 21.0 * s).max(0.0), stop_row.h);
                ui.text_in(&mut self.atlas, &self.fonts, &pin.name, 13.5 * s, Weight::Bold, name_r, Align::Left, TEXT);
                let total = self.pins.dist.last().copied().flatten().filter(|_| self.pins.dist.len() == self.pins.list.len());
                let line2 = total
                    .map(|d| {
                        let secs = pins::eta(d, self.speed_avg);
                        format!("{}  ·  {}  ·  {}", pins::distance_text(d), pins::eta_text(secs), crate::nav_duty::hhmm(f.time + secs))
                    })
                    .unwrap_or_default();
                match (&note, &jam_note) {
                    (Some((t, c)), _) => {
                        ui.text_in(&mut self.atlas, &self.fonts, t, 12.5 * s, Weight::Medium, y2, Align::Left, *c);
                    }
                    (None, jam) => {
                        // (the next via on the right, the way to the destination left of it)
                        let mut left = y2;
                        if let Some(v) = via.as_deref() {
                            let vw = self.fonts.width(v, 12.5 * s, Weight::Bold) + 2.0 * s;
                            ui.text_in(&mut self.atlas, &self.fonts, v, 12.5 * s, Weight::Bold, y2, Align::Right, pins::PIN_LIGHT);
                            left.w = (left.w - vw - 10.0 * s).max(0.0);
                        }
                        let (t, c) = match jam {
                            Some((j, c)) if !line2.is_empty() => (format!("{line2}  ·  {j}"), *c),
                            _ => (line2, TEXT_DIM),
                        };
                        ui.text_in(&mut self.atlas, &self.fonts, &t, 12.5 * s, Weight::Medium, left, Align::Left, c);
                    }
                }
            }
            (None, None) => {
                let t = f.terminus.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| wd.no_duty.to_string());
                ui.text_in(&mut self.atlas, &self.fonts, &t, 13.0 * s, Weight::Medium, stop_row, Align::Left, TEXT_DIM);
            }
        }
    }
}

// --- the sign-on page as the navigator, and its size ----------------------------------------

impl Navigator {
    /// The pairing QR code for a device ([`crate::companion::pair_qr`]): asked again when the
    /// address or the code changes, and every few seconds (the companion may make it later).
    fn pair_qr(&mut self, st: &crate::companion::CompanionState) -> Option<std::sync::Arc<crate::nav_signon::Qr>> {
        let key = qr_key(st);
        if self.qr.0 != key || (self.time - self.qr.1).abs() > 3.0 {
            let qr = crate::companion::pair_qr().and_then(|(side, dark)| crate::nav_signon::Qr::new(side, dark)).map(std::sync::Arc::new);
            // (the same code again keeps its `Arc`: the page is no different)
            let qr = match (&self.qr.2, qr) {
                (Some(old), Some(new)) if **old == *new => Some(old.clone()),
                (_, new) => new,
            };
            self.qr = (key, self.time, qr);
        }
        self.qr.2.clone()
    }

    /// The small navigator as the sign-on page, `pw` pixels wide at scale `s` and at most
    /// `room` tall - all of it when `fill` (a panel the player sized: its height is theirs).
    #[allow(clippy::too_many_arguments)]
    fn panel_page(&mut self, st: &crate::companion::CompanionState, duty: Option<&crate::nav_duty::DutyState>, time: f64, pw: f32, s: f32, room: f32, fill: bool) -> PanelPage {
        use crate::nav_signon as signon;
        self.city.phone.follow(st);
        if let Some(r) = self.city.phone.wants(st) {
            crate::companion::request(r);
        }
        let qr = self.pair_qr(st);
        let page = signon::page_in(st, &self.city.phone, duty, time, signon::Room::Panel, qr.as_ref());
        let mut fit = signon::fit_panel(page, pw, s, room, &self.fonts);
        if fill {
            fit.height = room.round();
        }
        // (a page that is taller than the window scrolls: the wheel over it - held to this
        // page's range only while it is the one on the screen; the city map's page scrolls by
        // the same, further, and is kept in its own range where it is drawn)
        if !self.city.open {
            self.city.phone.scroll = self.city.phone.scroll.clamp(0.0, signon::panel_scroll_max(&fit)).round();
        }
        let hits = signon::panel_hits(&fit, self.city.phone.scroll);
        PanelPage { fit, hits, stage: st.stage }
    }

    /// The small navigator as the sign-on page, painted for a panel `pw` x `ph`: its
    /// background (drawn at the opacity setting's), its head, and its items (to be clipped to
    /// the part under the head, which they scroll in: the last).
    fn paint_signon(&mut self, pw: f32, ph: f32) -> Option<(Painter, Painter, Painter, Rect)> {
        use crate::nav_signon as signon;
        let sp = self.signing.take()?;
        let panel = Rect::new(0.0, 0.0, pw, ph);
        let scroll = self.city.phone.scroll;
        let mut bg = Painter::new();
        bg.rounded(panel, 7.0 * (pw / 360.0), if self.cockpit_display { Color::rgba(10, 10, 10, 1.0) } else { crate::nav_duty::SHEET.alpha(0.86) });
        let mut ui = Painter::new();
        signon::draw_panel_head(&mut crate::nav_duty::Pen { p: &mut ui, atlas: &mut self.atlas, fonts: &self.fonts }, &sp.fit, panel, scroll);
        let mut list = Painter::new();
        signon::draw_panel_items(&mut crate::nav_duty::Pen { p: &mut list, atlas: &mut self.atlas, fonts: &self.fonts }, &sp.fit, panel, scroll);
        self.size_note(&mut ui, Vec2::new(pw * 0.5, ph * 0.5), sp.fit.s);
        let view = signon::panel_view(&sp.fit, panel);
        self.signing = Some(sp);
        Some((bg, ui, list, view))
    }

    /// Draw the small navigator as the sign-on page into its texture: its background (the
    /// opacity setting's, as the map's), the page's head and items.
    fn draw_signon(&mut self, renderer: &Renderer, target: &wgpu::TextureView, size: (u32, u32)) {
        let (pw, ph) = (size.0 as f32, size.1 as f32);
        self.atlas.begin_frame();
        let Some((bg, ui, list, view)) = self.paint_signon(pw, ph) else { return };
        let radius = 7.0 * (pw / 360.0);
        let (Some(gpu), device, queue) = (self.gpu.as_mut(), &renderer.device, &renderer.queue) else { return };
        let n_bg = bg.len();
        let mut all = bg.verts;
        all.extend(list.verts);
        let n_list = all.len() as u32;
        all.extend(ui.verts);
        gpu.upload(device, queue, 2, &all);
        gpu.upload_atlas(queue, &mut self.atlas);
        let clip = [0.0, 0.0, pw, ph];
        let backdrop = Layer::flat(clip, radius, if self.cockpit_display { self.opacity } else { crate::ui::backdrop(self.opacity).min(1.0) });
        let mut layers = [Layer::flat(clip, radius, 1.0), backdrop, Layer::flat([view.x, view.y, view.right(), view.bottom()], radius, 1.0)];
        for l in layers.iter_mut() {
            l.opacity *= self.shown;
        }
        let draws = [
            Draw { buffer: 2, range: 0..n_bg, layer: 1, texture: 0 },
            Draw { buffer: 2, range: n_bg..n_list, layer: 2, texture: 0 },
            Draw { buffer: 2, range: n_list..all.len() as u32, layer: 0, texture: 0 },
        ];
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("navigator sign-on") });
        gpu.render(device, queue, &mut enc, target, size, Some(wgpu::Color::TRANSPARENT), &layers, &draws);
        queue.submit([enc.finish()]);
    }

    /// The size, for a moment after it changed: "110 %" in a dark pill centred on `at`.
    fn size_note(&mut self, ui: &mut Painter, at: Vec2, s: f32) {
        let t = self.time - self.size_at;
        if !(0.0..SIZE_NOTE).contains(&t) {
            return;
        }
        let a = ((SIZE_NOTE - t) / 0.3).min(1.0);
        let text = size_text(self.size);
        let px = 14.0 * s;
        let w = self.fonts.width(&text, px, Weight::Bold) + 40.0 * s;
        let r = Rect::new(at.x - w * 0.5, at.y - 15.0 * s, w, 30.0 * s);
        ui.rounded(r, 15.0 * s, Color::rgba(10, 12, 18, 0.9 * a));
        ui.icon(&mut self.atlas, "zoom_in", Vec2::new(r.x + 16.0 * s, r.center().y), 16.0 * s, TEXT.alpha(a));
        ui.text_in(&mut self.atlas, &self.fonts, &text, px, Weight::Bold, Rect::new(r.x + 28.0 * s, r.y, r.w - 34.0 * s, r.h), Align::Center, TEXT.alpha(a));
    }

    /// Make the navigator larger (`steps` > 0) or smaller by the setting's steps, within its
    /// range; the caller keeps the new size ([`Navigator::take_resized`]).
    pub fn resize_by(&mut self, steps: i32) {
        let to = omsi_launcher_lib::nav_scale(Some(self.size as f64 + (steps as f64) * SIZE_STEP as f64)) as f32;
        if (to - self.size).abs() > 1e-4 {
            // (a panel the player sized keeps its size: its texts grow or shrink in it, as far
            // as they fit - see `nav_panel::scale`)
            self.size = to;
            self.drawn_at = f32::MIN;
            self.resized = Some(to);
        }
        // (also at the end of the range: the note says where it stands)
        self.size_at = self.time;
    }

    /// A size changed in the game since this was last asked: the setting to keep it as.
    pub fn take_resized(&mut self) -> Option<f32> {
        self.resized.take()
    }

    /// Ctrl + the wheel: the notches, whole steps of the size as they add up.
    fn size_wheel(&mut self, amount: f32) {
        // (the other way: what was left of the last notches counts no more)
        if self.size_wheel * amount < 0.0 {
            self.size_wheel = 0.0;
        }
        self.size_wheel += amount;
        let steps = self.size_wheel.trunc();
        if steps != 0.0 {
            self.size_wheel -= steps;
            self.resize_by(steps as i32);
        }
    }

    /// The mouse wheel at (`x`, `y`): the city map takes it while it is open (Ctrl: the
    /// size); over the small navigator Ctrl + the wheel sizes it, and the wheel alone scrolls
    /// its sign-on page when that is taller than the window (`panel`: the small navigator is
    /// on the screen there, not in VR's cab). True when it was the navigator's.
    pub fn wheel(&mut self, amount: f32, x: f32, y: f32, ctrl: bool, panel: bool) -> bool {
        if self.city.open {
            if ctrl {
                self.size_wheel(amount);
            } else {
                self.map_wheel(amount, x, y);
            }
            return true;
        }
        if !panel || !self.over_panel(x, y) {
            return false;
        }
        if ctrl {
            self.size_wheel(amount);
            return true;
        }
        // (the sign-on page taller than the window: the wheel scrolls it)
        let Some(fit) = self.signing.as_ref().map(|p| &p.fit).filter(|f| crate::nav_signon::panel_scroll_max(f) > 0.0) else { return false };
        let (by, max) = (amount * 48.0 * fit.s, crate::nav_signon::panel_scroll_max(fit));
        self.city.phone.scroll = (self.city.phone.scroll - by).clamp(0.0, max);
        self.drawn_at = f32::MIN;
        true
    }
}

/// What the pairing QR code is made of, as far as the navigator sees: where and with which
/// code a device pairs.
fn qr_key(st: &crate::companion::CompanionState) -> String {
    format!("{:?} {:?} {}", st.listening, st.addresses.first(), st.pairing_code)
}

/// The navigator's size as it is said: "110 %".
fn size_text(size: f32) -> String {
    format!("{:.0} %", size * 100.0)
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
        NavFrame { traffic: self.traffic, players: self.players.clone(), bus: self.bus, heading: self.heading, speed_kmh: self.speed_kmh, outside_temp: self.outside_temp, inside_temp: self.inside_temp, line: self.line.clone(), terminus: self.terminus.clone(), stops: self.stops.clone(), delay: self.delay, duty: self.duty, passengers: self.passengers, time: self.time, weekday: self.weekday, language: self.language, screen: self.screen, ui_scale: self.ui_scale, follow_window: self.follow_window, dt: self.dt, stop_requested: self.stop_requested, info_rect: self.info_rect, touch_gap: self.touch_gap }
    }
}


mod roads;
pub(crate) use roads::*;
mod city_map;
pub use city_map::*;
#[cfg(test)]
mod tests;


// --- for the phone and tablet companion (`companion::nav`): what the navigator follows, read
// only, so that a phone or tablet shows the same route, turn, stop and note as the panel

impl Navigator {
    /// What the navigator follows now, as the companion hands it to a phone or tablet: the
    /// route's network (the whole map's, else the traffic's), the route's lanes with the one
    /// the bus is on and how far along it, the note the panel shows, the next turn, the way to
    /// the next stop, the street, the speed limit at the bus and what the jams ahead cost.
    pub(crate) fn companion_look<'a>(&'a self, traffic: Option<&'a Traffic>, bus: DVec3) -> crate::companion::NavLook<'a> {
        use crate::companion::RouteNote;
        let net = self.global.as_deref().or(traffic.map(|t| &t.net)).or(self.own_net.as_deref());
        let r = &self.route;
        // (as the panel's bottom bar has it)
        let note = if r.note > 0.0 {
            Some(RouteNote::Recalculated)
        } else if r.joined && !r.lanes.is_empty() && !r.on_route && r.off_for > OFF_ROUTE_AFTER {
            Some(if r.off_for < OFF_ROUTE_AFTER + 20.0 { RouteNote::Rerouting } else { RouteNote::OffRoute })
        } else {
            None
        };
        // (as the panel's speed limit sign has it)
        let limit = net.and_then(|n| {
            let lane = if r.on_route { r.lanes.get(r.progress).copied() } else { None };
            let lane = lane.or_else(|| n.nearest_lane_near(bus, LaneKind::Street).filter(|l| l.2 < 8.0).map(|l| l.0))?;
            let v = n.lanes.get(lane)?.speed_limit_kmh;
            (v > 1.0 && v < 200.0).then_some(v)
        });
        crate::companion::NavLook {
            net: self.global.as_deref().or(traffic.map(|t| &t.net)),
            map: self.global.clone().map(|g| (self.global_version, g)),
            lanes: &r.lanes,
            progress: r.progress,
            s: r.s,
            on_route: r.on_route,
            note,
            turn: self.next_turn.clone(),
            next_dist: self.next_dist,
            street: self.street_here.as_deref(),
            limit,
            speed_avg: self.speed_avg,
            jam_cost: self.jam_cost,
            places: &self.stop_pos,
            pins: self
                .pins
                .list
                .iter()
                .enumerate()
                .map(|(k, p)| crate::companion::NavPin {
                    at: p.at,
                    via: match self.pins.role(k) {
                        crate::nav_pins::Role::Via(n) => n,
                        crate::nav_pins::Role::Destination => 0,
                    },
                    name: p.name.clone(),
                    dist: self.pins.dist.get(k).copied().flatten(),
                })
                .collect(),
            diversion: self.pins.diversion() && self.pins.steers(),
            arrived: self.pins.arrived.is_some(),
            pin_note: self.pins.note.map(|(n, _)| (n.key(), match n {
                crate::nav_pins::Note::Reached(v) => v,
                _ => 0,
            })),
        }
    }

    /// The other vehicles within `reach` metres of `bus` as the maps draw them (none with the
    /// `nav_ai` setting off): place, heading, what it is (0 a car, 1 a trolleybus, 2 a bus,
    /// 3 a tram) and the line it shows.
    pub(crate) fn companion_traffic(&self, traffic: Option<&Traffic>, bus: DVec3, reach: f64) -> Vec<(DVec3, f64, u8, Option<String>)> {
        let Some(t) = traffic.filter(|_| self.show_ai) else { return Vec::new() };
        t.cars
            .iter()
            .filter(|c| !c.gone && (c.vehicle.position - bus).truncate().length() <= reach)
            .map(|c| {
                let (color, line) = traffic_kind(c);
                let kind = if color == TROLLEY { 1 } else if color == BUS { 2 } else if color == TRAM { 3 } else { 0 };
                (c.vehicle.position, c.vehicle.heading, kind, line)
            })
            .collect()
    }

    /// The roads of a map's network as the city map draws them: carriageways with their
    /// widths, the main roads marked (the phone's map draws the same roads).
    pub(crate) fn companion_roads(net: &Network) -> Vec<MapRoad> {
        road_geometry(net)
    }
}
