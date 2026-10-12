//! What the launcher asks of the program it runs in: the game program (omsi-app) installs a
//! `Host` once, before the launcher's window opens (`set`). Everything here is what only the
//! game has - its settings file's reading, the renderer's start, the bus preview drawn with
//! the game's scene, the city map read the way the navigator reads it, the interface's
//! language and machine translation, the bus radio's stations, the
//! support package - so the launcher's pages need none of the game's modules.
//!
//! Without a host (the launcher's own tests) every call answers as if nothing were there.

use glam::{DVec3, Vec3};
use omsi_render::{RenderOptions, Renderer};
use omsi_ui::Color;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

/// The game program's side of the launcher (see the module).
pub trait Host: Send + Sync {
    /// Show the interface in `code` (the settings' ENG / DEU / FRA / RUS).
    fn ui_language(&self, _code: &str) {}
    /// Machine translation of the interface on or off, what it is doing, and texts it must
    /// leave alone (names of maps, buses, lines).
    fn mt_enable(&self, _on: bool) {}
    fn mt_status(&self) -> String {
        String::new()
    }
    fn mt_protect(&self, _texts: &mut dyn Iterator<Item = &str>) {}
    /// A phone: what the previous run said last and the end of its log, when it crashed.
    fn previous_run_crash(&self) -> Option<(String, String)> {
        None
    }
    /// A phone: keep the gallery out of the content folder.
    fn hide_content_from_gallery(&self) {}
    /// The bus radio's own stations (name, address), and saving them.
    fn radio_stations(&self) -> Vec<(String, String)> {
        Vec::new()
    }
    fn save_radio_stations(&self, _list: &[(String, String)]) -> std::io::Result<()> {
        Ok(())
    }
    /// Where a navigator window was dragged to in the game (`at x,y`), as shares of the screen.
    fn navigator_placed_at(&self, _corner: &str) -> Option<[f32; 2]> {
        None
    }
    /// The support package of what the launcher has chosen: gathered now, written by the
    /// job returned (on a worker), which answers where it went.
    fn support_bundle(&self, _renderer: Option<&Renderer>, _devices: Option<Vec<omsi_input::controllers::Connected>>, _selection: Selection<'_>) -> SupportJob {
        Box::new(|| Err("no support package without the game".to_string()))
    }
    /// The ambience's recordings fetched now, when the settings want them.
    fn fetch_ambience(&self) {}
    /// How the bus preview is drawn (the plain path of the settings' renderer options).
    fn showroom_options(&self) -> RenderOptions {
        RenderOptions::default()
    }
    /// A window's renderer on its way: the device open, the pipelines built on a worker.
    fn begin_renderer(&self, _instance: &mut wgpu::Instance, _window: &Arc<Window>, _options: RenderOptions) -> anyhow::Result<Box<dyn RendererStart>> {
        Err(anyhow::anyhow!("no renderer without the game"))
    }
    /// Tell the player why the program cannot go on (a dialog where there is one).
    fn fatal_message(&self, text: &str) {
        log::error!("{text}");
    }
    fn window_icon(&self) -> Option<winit::window::Icon> {
        None
    }
    /// The launcher's window size and place, fitted to the screen it opens on.
    fn fit_window(&self, _event_loop: &ActiveEventLoop, w: f64, h: f64) -> (winit::dpi::LogicalSize<f64>, Option<winit::dpi::PhysicalPosition<i32>>) {
        (winit::dpi::LogicalSize::new(w, h), None)
    }
    /// The colour a window shows before it has anything else to show.
    fn window_background(&self) -> wgpu::Color {
        wgpu::Color::BLACK
    }
    /// The bus preview (drawn by the game's renderer with the game's scene).
    fn new_showroom(&self) -> Box<dyn Showroom> {
        Box::new(NoShowroom::default())
    }
    /// The launcher ends in order while a preview is being placed: no hang to remember.
    fn clear_showroom_mark(&self) {}
    /// A map's roads, the network behind them and its objects' places, read the way the
    /// game's navigator reads them, so the two maps cannot drift apart.
    fn city_map(&self, _root: &Path, _tiles: &[(i32, i32, PathBuf)], _chrono: &[PathBuf]) -> CityMap {
        CityMap::default()
    }
    /// The game's city map colours: road casing, road, main road, route.
    fn city_map_colors(&self) -> [Color; 4] {
        [Color::rgba(30, 30, 30, 0.9), Color::rgba(92, 92, 92, 1.0), Color::rgba(112, 112, 112, 1.0), Color::rgba(214, 48, 40, 1.0)]
    }
    /// A line's points thinned to `tol` (the navigator's own simplification).
    fn simplify(&self, pts: &[Vec3], _tol: f32) -> Vec<Vec3> {
        pts.to_vec()
    }

    // --- what Omsi-Hub's launcher asks besides
    /// The studio that takes the bus picker's photos (a showroom of its own: the bus from
    /// the same corner every time, nothing eased).
    fn new_studio(&self) -> Box<dyn Showroom> {
        Box::new(NoShowroom::default())
    }
    /// The vehicle file the game spawns for `bus` (an articulated bus's rear section: the
    /// whole bus; one nothing couples is refused).
    fn player_bus_path(&self, root: &Path, bus: &str) -> anyhow::Result<PathBuf> {
        Ok(omsi_cfg::resolve_path(root, bus))
    }
    /// The `[couple_back]` chain behind `vehicle`, read and coupled as the game does it.
    fn load_coupled_parts(&self, _root: &Path, _vehicle: &mut omsi_sim::VehicleInstance) -> Vec<Arc<omsi_sim::VehicleType>> {
        Vec::new()
    }
    /// The phone companion's tunnel program: there, missing, being fetched; and fetching it.
    fn cloudflared(&self) -> Cloudflared {
        Cloudflared::Missing
    }
    fn download_cloudflared(&self) {}
    /// The personnel number and code driver `name` signs on with at the navigator.
    fn personnel_of_driver(&self, _name: &str) -> Option<(String, String)> {
        None
    }
}

/// The phone companion's tunnel program (cloudflared), as the game program finds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cloudflared {
    Installed(PathBuf),
    Missing,
    Downloading,
    Failed,
}

/// What the launcher has chosen, for the support package: map, bus, line, tour.
pub type Selection<'a> = (&'a str, &'a str, Option<&'a str>, Option<&'a str>);
/// Writes a support package; answers where it went.
pub type SupportJob = Box<dyn FnOnce() -> Result<PathBuf, String> + Send>;

/// A window's renderer being built (the game's `startup::RendererStart`): the window can be
/// drawn into with its device meanwhile.
pub trait RendererStart: Send {
    fn device(&self) -> &wgpu::Device;
    fn queue(&self) -> &wgpu::Queue;
    fn format(&self) -> wgpu::TextureFormat;
    /// Whether the window can be drawn into before the renderer is there.
    fn early(&self) -> bool;
    /// The pipelines are built (or failed): `wait` returns at once.
    fn is_done(&self) -> bool;
    /// Let go of it: waits for its pipelines and drops it.
    fn abandon(self: Box<Self>);
    /// The renderer, once its pipelines are built (or the next adapter's).
    fn wait(self: Box<Self>, instance: &mut wgpu::Instance) -> anyhow::Result<Renderer>;
}

/// The bus preview: the chosen bus in a picture of its own, drawn again only when it changed.
pub trait Showroom {
    /// Show the launcher's choice (compared first; read anew only when it changed).
    fn want_choice(&mut self, root: &str, choice: &crate::state::Choice);
    /// Turn it by a drag, nearer or farther by a factor.
    fn orbit(&mut self, dx: f32, dy: f32);
    fn zoom_by(&mut self, k: f32);
    /// Where the bus appears on screen: the share of the width its centre is at.
    fn set_focus_x(&mut self, x: f32);
    /// Load and ease; whether the picture must be drawn again.
    fn update(&mut self, renderer: &Renderer, dt: f32) -> bool;
    /// The picture at this size (drawn now if it changed).
    fn preview(&mut self, renderer: &mut Renderer, w: u32, h: u32) -> Option<wgpu::TextureView>;
    fn has_picture(&self) -> bool;
    /// The bus could not be shown.
    fn failed(&self) -> bool;
    /// A bus is being read.
    fn busy(&self) -> bool;
    /// Bumped when the picture's texture is made anew / at every drawing of it.
    fn generation(&self) -> u64;
    fn drawn(&self) -> u64;

    // --- Omsi-Hub's launcher: the stage, the bus options, the livery studio, the photos
    /// Why the bus could not be shown (None: it could, or it is not known yet).
    fn error(&self) -> Option<String> {
        None
    }
    /// The part of the picture the bus is to stand in (see `showroom::stage_in`).
    fn set_stage(&mut self, _stage: crate::showroom::Stage) {}
    /// Show this look (a bus, its paint, the time and weather to light it with).
    fn want(&mut self, _look: crate::showroom::Look) {}
    /// `look` is in place, ready to be drawn; it will not be (the bus did not load).
    fn shows(&self, _look: &crate::showroom::Look) -> bool {
        false
    }
    fn gave_up(&self, _look: &crate::showroom::Look) -> bool {
        true
    }
    /// The bus options it is to wear, and those it wears (None: no bus shown).
    fn dress(&mut self, _options: Vec<(String, f32)>) {}
    fn dressed(&self) -> Option<Vec<(String, f32)>> {
        None
    }
    /// The display font its destination displays are drawn in (None: their own).
    fn letter(&mut self, _font: Option<String>) {}
    /// The files of bus `bus` changed: read it again.
    fn reread(&mut self, _bus: &str) {}
    /// Let go of the bus shown and its scene.
    fn forget(&mut self) {}
    /// Draw the picture again (a texture of the bus changed).
    fn touch(&mut self) {}
    /// A camera of the caller's own instead of the orbit (None: the orbit).
    fn set_camera(&mut self, _cam: Option<omsi_render::Camera>) {}
    /// A second picture with its own camera and size, drawn whenever the first is.
    fn set_second(&mut self, _cam: Option<(omsi_render::Camera, u32, u32)>) {}
    fn second_view(&self) -> Option<(wgpu::TextureView, u32, u32)> {
        None
    }
    fn second_generation(&self) -> u64 {
        0
    }
    /// The bus shown opened up for the livery studio (None while none is placed).
    fn parts(&mut self) -> Option<crate::showroom::Parts<'_>> {
        None
    }
    /// The bus shown, drawn at `w` x `h` and read back (the bus picker's photos).
    fn photograph(&mut self, _renderer: &mut Renderer, _w: u32, _h: u32) -> Option<image::RgbaImage> {
        None
    }
    /// The bus picker's photos, on the GPU with this showroom (see `busphoto`).
    fn photos(&mut self) -> &mut crate::busphoto::Photos;
}

/// No preview (no host).
#[derive(Default)]
struct NoShowroom {
    photos: crate::busphoto::Photos,
}

impl Showroom for NoShowroom {
    fn want_choice(&mut self, _root: &str, _choice: &crate::state::Choice) {}
    fn orbit(&mut self, _dx: f32, _dy: f32) {}
    fn zoom_by(&mut self, _k: f32) {}
    fn set_focus_x(&mut self, _x: f32) {}
    fn update(&mut self, _renderer: &Renderer, _dt: f32) -> bool {
        false
    }
    fn preview(&mut self, _renderer: &mut Renderer, _w: u32, _h: u32) -> Option<wgpu::TextureView> {
        None
    }
    fn has_picture(&self) -> bool {
        false
    }
    fn failed(&self) -> bool {
        true
    }
    fn busy(&self) -> bool {
        false
    }
    fn generation(&self) -> u64 {
        0
    }
    fn drawn(&self) -> u64 {
        0
    }
    fn photos(&mut self) -> &mut crate::busphoto::Photos {
        &mut self.photos
    }
}

/// A carriageway of the city map: points in world metres.
pub struct MapRoad {
    pub points: Vec<DVec3>,
    pub width: f32,
    /// A road cars drive far along: drawn brighter.
    pub main: bool,
}

/// A map as the city map draws it (see `Host::city_map`).
#[derive(Default)]
pub struct CityMap {
    pub roads: Vec<MapRoad>,
    /// The linked network behind the roads (a trip's road pieces are lanes of it).
    pub net: omsi_sim::traffic::Network,
    /// Every placed object's world place by id.
    pub places: HashMap<i64, DVec3>,
    /// The map's bus stops (Omsi-Hub's map draws them as the navigator's stop signs).
    pub stops: Vec<omsi_scene::MapStop>,
}

// (an install-once hook: the program's side, the same for every launcher window)
static HOST: OnceLock<Box<dyn Host>> = OnceLock::new();

/// Install the program's side of the launcher (once; later calls are ignored).
pub fn set(host: Box<dyn Host>) {
    let _ = HOST.set(host);
}

struct NoHost;
impl Host for NoHost {}

/// The installed host (or one that has nothing).
pub fn host() -> &'static dyn Host {
    static NONE: NoHost = NoHost;
    HOST.get().map(|h| h.as_ref()).unwrap_or(&NONE)
}
