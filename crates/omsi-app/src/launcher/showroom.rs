//! The launcher's bus preview: the chosen bus drawn by the game's own renderer - its
//! model, paint and materials - on the plain (Vanilla+) path without the costly passes,
//! under the light of the chosen time and weather, into a picture the launcher shows
//! in a card. It is drawn again only when something changed (another bus, paint, light,
//! the preview turned by the mouse), never every frame.
//!
//! A bus is read on a worker (its type, its scripts run to their resting state, its
//! textures and meshes put on the GPU ahead) and then placed into a scene of its own with
//! a fresh `World` (whose caches belong to one scene); the old scene goes when the new one
//! is ready, so the picture never goes blank while switching.

use super::super::*;
use glam::DVec3;
use omsi_render::{Camera, Lighting, Renderer, Scene};
use std::sync::mpsc::{channel, Receiver};

/// What the showroom shows.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Look {
    pub root: PathBuf,
    pub map: String,
    pub bus: String,
    pub paint: String,
    pub weather: String,
    /// Minutes of the day, and the date.
    pub time: i32,
    pub date: String,
}

struct Ready {
    look: Look,
    world: Arc<scene::World>,
    vt: Arc<omsi_sim::VehicleType>,
    vehicle: omsi_sim::VehicleInstance,
    scheme: Option<usize>,
}

struct Shown {
    look: Look,
    scene: Scene,
    #[allow(dead_code)]
    world: Option<Arc<scene::World>>,
    vehicle: Option<omsi_sim::VehicleInstance>,
    render: Option<scene::VehicleRender>,
    trailers: Vec<scene::VehicleRender>,
    /// Centre and size of the bus (its bounding box).
    centre: glam::Vec3,
    length: f32,
    weather: omsi_content::weather::Weather,
    /// The sky's cloud type and weather model its look is drawn with.
    sky: crate::weather_setup::SkyState,
    lighting: Lighting,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreviewView {
    #[default]
    Outside,
    Passenger,
    Driver,
}

impl PreviewView {
    pub const ALL: [PreviewView; 3] = [PreviewView::Outside, PreviewView::Passenger, PreviewView::Driver];

    pub fn name(self) -> &'static str {
        match self {
            PreviewView::Outside => "outside",
            PreviewView::Passenger => "passenger",
            PreviewView::Driver => "driver",
        }
    }

    pub fn from_name(name: &str) -> Option<PreviewView> {
        PreviewView::ALL.into_iter().find(|v| v.name() == name)
    }
}

fn authored_camera(def: &omsi_vehicle::Vehicle, view: PreviewView) -> Option<&omsi_vehicle::Camera> {
    match view {
        PreviewView::Outside => None,
        PreviewView::Passenger => def.cameras_pax.first(),
        PreviewView::Driver => def.cameras_driver.get(def.camera_std).or(def.cameras_driver.first()),
    }
}

impl Shown {
    fn inside_camera(&self, view: PreviewView, look: (f32, f32), zoom: f32) -> Option<Camera> {
        let vehicle = self.vehicle.as_ref()?;
        let camera = |c: &omsi_vehicle::Camera, (position, yaw, pitch, roll): (DVec3, f32, f32, f32)| Camera {
            position,
            yaw,
            pitch: pitch.clamp(-89.0, 89.0),
            roll,
            fov_deg: (c.fov * zoom).clamp(20.0, 100.0),
            near: 0.1,
            far: 6000.0,
        };
        let turned = |c: &omsi_vehicle::Camera| omsi_vehicle::Camera { yaw: c.yaw + look.0, pitch: (c.pitch + look.1).clamp(-89.0, 89.0), ..c.clone() };
        if let Some(c) = authored_camera(&vehicle.ty.def, view) {
            return Some(camera(c, vehicle.camera_world_full(&turned(c))));
        }
        vehicle.trailers.iter().find_map(|t| {
            let c = authored_camera(&t.ty.def, view)?;
            Some(camera(c, t.camera_world_full(&turned(c))))
        })
    }
}

pub struct Showroom {
    shown: Option<Shown>,
    wanted: Option<Look>,
    /// The last look that could not be shown (not read again until something changes).
    failed: Option<Look>,
    loading: Option<(Look, Receiver<Result<Ready, String>>)>,
    /// The map the bus being read opened on its worker, for this thread to give it the
    /// renderer's prefetch (see `start_loading`).
    opening: Option<(Receiver<Arc<scene::World>>, std::sync::mpsc::Sender<scene::VehiclePrefetch>)>,
    pub error: Option<String>,
    /// Orbit: yaw and pitch (degrees) and distance factor, eased towards the targets.
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    yaw_to: f32,
    pitch_to: f32,
    zoom_to: f32,
    pub auto_turn: bool,
    pub view: PreviewView,
    inside_look: (f32, f32),
    inside_zoom: f32,
    idle: f32,
    /// Where the bus should appear on screen: the share of the width its centre is at.
    pub focus_x: f32,
    focus_now: f32,
    pub busy: bool,
    /// The picture: its texture and size, and whether it must be drawn again.
    target: Option<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
    dirty: bool,
    /// Bumped whenever `target` was made anew (the interface binds it again).
    pub generation: u64,
    /// Bumped at every drawing of the picture (the launcher shows an unchanged one only once).
    pub drawn: u64,
}

/// Names the bus whose preview is being read and placed, until it is in the picture.
fn placing_mark() -> PathBuf {
    preview_guard_path(&omsi_launcher_lib::data_dir(), "placing", omsi_platform::VERSION)
}

fn preview_guard_path(dir: &Path, kind: &str, version: &str) -> PathBuf {
    // A newer renderer or loader must get a chance to retry previews an older one failed.
    dir.join(format!("showroom-{kind}-{version}.txt"))
}

fn preview_is_skipped(dir: &Path, version: &str, bus: &str) -> bool {
    let mark = preview_guard_path(dir, "placing", version);
    let skip = preview_guard_path(dir, "skip", version);
    if std::fs::read_to_string(&mark).is_ok_and(|b| b.trim() == bus.trim()) {
        log::warn!("showroom: the preview of {bus} did not finish when it was last tried; it is left out for this version ({})", skip.display());
        let _ = std::fs::remove_file(mark);
        let mut list = std::fs::read_to_string(&skip).unwrap_or_default();
        list.push_str(bus.trim());
        list.push('\n');
        let _ = std::fs::write(&skip, list);
    }
    std::fs::read_to_string(skip).is_ok_and(|t| t.lines().any(|b| b.trim() == bus.trim()))
}

/// The launcher ends in order (closed, or a test's time is up) while a preview is being
/// placed: that is no hang, and the bus must not be left out of the previews from then on
/// (a test ended mid-placing put the bus it showed into the skip list).
pub(super) fn clear_placing_mark() {
    let _ = std::fs::remove_file(placing_mark());
}

fn args_for(look: &Look) -> Args {
    let mut v = vec!["omsi".to_string(), "--root".into(), look.root.to_string_lossy().to_string()];
    if !look.map.is_empty() {
        v.extend(["--map".into(), look.map.clone()]);
    }
    if !look.bus.is_empty() {
        v.extend(["--bus".into(), look.bus.clone()]);
    }
    if !look.paint.is_empty() {
        v.extend(["--paint".into(), look.paint.clone()]);
    }
    // (the current weather is fetched by the game, not by the preview)
    if !look.weather.is_empty() && !look.weather.starts_with("metar:") {
        v.extend(["--weather".into(), look.weather.clone()]);
    }
    v.extend(["--time".into(), format!("{:02}:{:02}", look.time / 60, look.time % 60)]);
    if !look.date.is_empty() {
        v.extend(["--date".into(), look.date.clone()]);
    }
    let mut args = Args::try_parse_from(v).unwrap_or_else(|_| Args::parse_from(["omsi"]));
    // (drawn the way the game will be)
    args.session_settings = crate::startup::SessionSettings::for_showroom(&crate::settings::Settings::load());
    args
}

// (a launcher closed while a preview loads did not hang: the mark goes with it)
impl Drop for Showroom {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(placing_mark());
    }
}

impl Showroom {
    pub fn new() -> Showroom {
        Showroom {
            shown: None,
            wanted: None,
            failed: None,
            loading: None,
            opening: None,
            error: None,
            yaw: 215.0,
            pitch: 8.0,
            zoom: 1.0,
            yaw_to: 215.0,
            pitch_to: 8.0,
            zoom_to: 1.0,
            auto_turn: false,
            view: PreviewView::Outside,
            inside_look: (0.0, 0.0),
            inside_zoom: 1.0,
            idle: 0.0,
            focus_x: 0.5,
            focus_now: 0.5,
            busy: false,
            target: None,
            dirty: true,
            generation: 0,
            drawn: 0,
        }
    }

    /// Show this (a bus, its paint, the time and weather to light it with).
    pub fn want(&mut self, look: Look) {
        if self.wanted.as_ref() != Some(&look) {
            self.wanted = Some(look);
            self.failed = None;
            self.error = None;
        }
    }

    fn fail(&mut self, look: Look, error: String) {
        self.error = Some(error);
        self.failed = Some(look);
        // A failed selection must not expose the previous bus through any preview API.
        self.shown = None;
        self.target = None;
        self.loading = None;
        self.opening = None;
    }

    /// The mouse dragged over the empty part of the window (degrees), or turned the wheel.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        if self.view != PreviewView::Outside {
            self.inside_look.0 += dx * 0.35;
            self.inside_look.1 = (self.inside_look.1 - dy * 0.25).clamp(-85.0, 85.0);
            self.dirty = true;
            self.idle = 0.0;
            return;
        }
        self.yaw_to += dx * 0.35;
        self.pitch_to = (self.pitch_to + dy * 0.25).clamp(-4.0, 55.0);
        self.idle = 0.0;
    }
    pub fn zoom_by(&mut self, k: f32) {
        if self.view != PreviewView::Outside {
            self.inside_zoom = (self.inside_zoom * k).clamp(0.5, 1.5);
            self.dirty = true;
            self.idle = 0.0;
            return;
        }
        self.zoom_to = (self.zoom_to * k).clamp(0.55, 2.2);
        self.idle = 0.0;
    }

    pub fn can_view(&self, view: PreviewView) -> bool {
        self.shown.as_ref().is_some_and(|s| s.vehicle.is_some() && (view == PreviewView::Outside || s.inside_camera(view, (0.0, 0.0), 1.0).is_some()))
    }

    pub fn views(&self) -> Vec<&'static str> {
        PreviewView::ALL.into_iter().filter(|v| self.can_view(*v)).map(PreviewView::name).collect()
    }

    pub fn set_view(&mut self, view: PreviewView) {
        if self.view != view && self.can_view(view) {
            self.view = view;
            self.inside_look = (0.0, 0.0);
            self.inside_zoom = 1.0;
            self.idle = 0.0;
            self.dirty = true;
        }
    }

    /// Per frame: start loading what is wanted, take over what finished loading, move the
    /// camera. Returns true when a new scene was put in place.
    pub fn update(&mut self, renderer: &Renderer, dt: f32) -> bool {
        let mut swapped = false;
        // the map of the bus being read is open: the worker goes on with the renderer's
        // prefetch of the bus's textures and meshes
        if let Some((worlds, prefetch)) = self.opening.as_ref() {
            match worlds.try_recv() {
                Ok(world) => {
                    let _ = prefetch.send(world.vehicle_prefetch(renderer));
                    self.opening = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => self.opening = None,
            }
        }
        // what finished loading
        if let Some((look, rx)) = self.loading.as_ref() {
            match rx.try_recv() {
                Ok(Ok(ready)) => {
                    self.loading = None;
                    self.shown = Some(self.place(renderer, ready));
                    if !self.can_view(self.view) {
                        self.view = PreviewView::Outside;
                    }
                    self.inside_look = (0.0, 0.0);
                    self.inside_zoom = 1.0;
                    self.error = None;
                    self.dirty = true;
                    swapped = true;
                }
                Ok(Err(e)) => {
                    let _ = std::fs::remove_file(placing_mark());
                    log::warn!("showroom {}: {e}", look.bus);
                    self.fail(look.clone(), e);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => {
                    let _ = std::fs::remove_file(placing_mark());
                    self.fail(look.clone(), "The preview loader stopped before the bus was ready".into());
                }
            }
        }
        // what is wanted and not shown or loading
        if let Some(w) = self.wanted.clone() {
            let shown = self.shown.as_ref().map(|s| &s.look);
            let loading = self.loading.as_ref().map(|l| &l.0);
            if shown != Some(&w) && loading != Some(&w) && self.loading.is_none() && self.failed.as_ref() != Some(&w) {
                // only the light changed: no need to read the bus again
                let same_bus = shown.map(|s| s.bus == w.bus && s.paint == w.paint && s.map == w.map && s.root == w.root).unwrap_or(false);
                if same_bus {
                    if let Some(s) = self.shown.as_mut() {
                        s.look = w.clone();
                        let args = args_for(&w);
                        (s.weather, s.sky.model) = crate::weather_setup::load_weather_run(&args);
                        setup_sky(&args, renderer, &mut s.scene, omsi_content::Envir::load(&args.root.join("envir.cfg")).ok().as_ref(), Some(&s.weather), &mut s.sky.clouds);
                        s.lighting = lighting_for(&args, &s.weather, &s.sky);
                    }
                    self.dirty = true;
                } else {
                    self.start_loading(w);
                }
            }
        }
        self.busy = self.loading.is_some();
        // camera
        self.idle += dt;
        if self.auto_turn && self.view == PreviewView::Outside && self.idle > 4.0 {
            self.yaw_to += dt * 6.0;
        }
        let k = 1.0 - (-dt / 0.18).exp();
        // still turning: the picture must follow
        if (self.yaw_to - self.yaw).abs() > 0.05 || (self.pitch_to - self.pitch).abs() > 0.05 || (self.zoom_to - self.zoom).abs() > 0.001 {
            self.dirty = true;
        }
        self.yaw += (self.yaw_to - self.yaw) * k;
        self.pitch += (self.pitch_to - self.pitch) * k;
        self.zoom += (self.zoom_to - self.zoom) * k;
        self.focus_now += (self.focus_x - self.focus_now) * (1.0 - (-dt / 0.35).exp());
        swapped
    }

    fn start_loading(&mut self, look: Look) {
        self.error = None;
        // Prevent a repeat hang on restart, but retry after an update (#1478, #1635).
        if preview_is_skipped(&omsi_launcher_lib::data_dir(), omsi_platform::VERSION, &look.bus) {
            self.fail(look, "The 3D preview of this bus stopped the launcher once, so it is left out".into());
            return;
        }
        let _ = std::fs::write(placing_mark(), &look.bus);
        let args = args_for(&look);
        let root = look.root.clone();
        let map_cfg = omsi_cfg::resolve_path(&root, &look.map);
        let date = start_clock(&args).date_code();
        let (tx, rx) = channel();
        // (the map is opened on the worker as well: on this thread it held the launcher's
        // frames up - a big map's, or any on a phone's storage, for a second and more)
        let (world_tx, world_rx) = channel();
        let (prefetch_tx, prefetch_rx) = channel::<scene::VehiclePrefetch>();
        let l2 = look.clone();
        std::thread::spawn(move || {
            let r = (|| -> Result<Ready> {
                let t0 = std::time::Instant::now();
                let world = Arc::new(scene::World::open(&root, &map_cfg, date)?);
                log::info!("showroom: {} opened in {:.2} s", map_cfg.display(), t0.elapsed().as_secs_f64());
                let _ = world_tx.send(world.clone());
                // (made by the launcher's thread, which has the renderer; gone with the
                // showroom when the device is given up meanwhile)
                let prefetch = prefetch_rx.recv().map_err(|_| anyhow!("the preview was let go of"))?;
                if l2.bus.is_empty() {
                    return Err(anyhow!("no bus"));
                }
                let path = player_bus_path(&root, &l2.bus)?;
                let vt = Arc::new(omsi_sim::VehicleType::load(&root, &path)?);
                let scheme = paint_scheme(&vt, Some(l2.paint.as_str()).filter(|p| !p.is_empty()));
                let host = omsi_sim::VehicleHost::new(start_clock(&args));
                let mut vehicle = omsi_sim::VehicleInstance::new(vt.clone(), host);
                if let Some(i) = scheme {
                    for (var, v) in vt.paint_schemes[i].set_vars.clone() {
                        vehicle.set_var(&var, v);
                    }
                }
                load_coupled_parts(&root, &mut vehicle);
                for _ in 0..3 {
                    vehicle.update(1.0 / 30.0);
                }
                prefetch.prefetch(&vt, scheme);
                for t in &vehicle.trailers {
                    prefetch.prefetch(&t.ty, scheme.filter(|i| *i < t.ty.paint_schemes.len()));
                }
                Ok(Ready { look: l2, world, vt, vehicle, scheme })
            })();
            let _ = tx.send(r.map_err(|e| format!("{e:#}")));
        });
        self.opening = Some((world_rx, prefetch_tx));
        self.loading = Some((look, rx));
    }

    fn place(&mut self, renderer: &Renderer, r: Ready) -> Shown {
        let t0 = std::time::Instant::now();
        let mut scene = renderer.new_scene();
        let args = args_for(&r.look);
        let (weather, model) = crate::weather_setup::load_weather_run(&args);
        let envir = omsi_content::Envir::load(&args.root.join("envir.cfg")).ok();
        let mut sky = crate::weather_setup::SkyState { clouds: None, model, look: args.session_settings };
        setup_sky(&args, renderer, &mut scene, envir.as_ref(), Some(&weather), &mut sky.clouds);
        add_floor(renderer, &mut scene);
        let world = r.world;
        // (the panes wear this weather's snow as the bus goes onto the GPU)
        world.settings.set_snow(weather.snow);
        let mut vehicle = r.vehicle;
        vehicle.position = DVec3::ZERO;
        vehicle.heading = 0.0;
        let render = world.add_vehicle(renderer, &mut scene, &r.vt, r.scheme);
        let trailers: Vec<scene::VehicleRender> = vehicle.trailers.iter().map(|t| world.add_vehicle_part(renderer, &mut scene, &t.ty, r.scheme.filter(|i| *i < t.ty.paint_schemes.len()), &render)).collect();
        vehicle.init_text_textures(&mut world.fonts.lock(), &|p| omsi_texture::decode_file(p).ok().map(|i| (i.width, i.height, i.rgba)));
        for t in vehicle.trailers.iter_mut() {
            t.init_text_textures(&mut world.fonts.lock(), &|p| omsi_texture::decode_file(p).ok().map(|i| (i.width, i.height, i.rgba)));
        }
        vehicle.update(1.0 / 30.0);
        // the bus's size from its bounding box (with the rear section behind it)
        let bb = r.vt.def.bounding_box.unwrap_or([2.5, 12.0, 3.0, 0.0, 0.0, 1.5]);
        let mut length = bb[1];
        let mut centre = glam::Vec3::new(bb[3], bb[4], bb[5]);
        for t in &vehicle.trailers {
            let tb = t.ty.def.bounding_box.unwrap_or([2.5, 8.0, 3.0, 0.0, 0.0, 1.5]);
            let back = (t.position - vehicle.position).truncate().length() as f32 + tb[1] * 0.5;
            let total = bb[1] * 0.5 + back;
            centre.y = bb[4] + bb[1] * 0.5 - total * 0.5;
            length = total;
        }
        let lighting = lighting_for(&args, &weather, &sky);
        log::info!("showroom: {} ({} meshes, {:.1} m long) placed in {:.2} s", r.look.bus, render.instances.len(), length, t0.elapsed().as_secs_f64());
        let _ = std::fs::remove_file(placing_mark());
        Shown { look: r.look, scene, world: Some(world), vehicle: Some(vehicle), render: Some(render), trailers, centre, length, weather, sky, lighting }
    }

    /// The picture of the bus at `w` x `h` pixels, drawn again when something changed.
    pub fn preview(&mut self, renderer: &mut Renderer, w: u32, h: u32) -> Option<wgpu::TextureView> {
        self.shown.as_ref()?;
        let (w, h) = (w.max(16), h.max(16));
        if self.target.as_ref().map(|t| (t.2, t.3) != (w, h)).unwrap_or(true) {
            let tex = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bus preview"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: renderer.format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            self.target = Some((tex, view, w, h));
            self.generation += 1;
            self.dirty = true;
        }
        if self.dirty {
            self.dirty = false;
            self.drawn += 1;
            let view = self.target.as_ref().unwrap().1.clone();
            self.render(renderer, &view, w, h);
        }
        self.target.as_ref().map(|t| t.1.clone())
    }

    fn render(&mut self, renderer: &mut Renderer, target: &wgpu::TextureView, w: u32, h: u32) {
        let (yaw, pitch, zoom, focus) = (self.yaw, self.pitch, self.zoom, self.focus_now);
        let Some(s) = self.shown.as_mut() else { return };
        if let (Some(v), Some(r)) = (s.vehicle.as_mut(), s.render.as_mut()) {
            player::sync_vehicle_transforms(renderer, &mut s.scene, v, r, &mut s.trailers, self.view != PreviewView::Outside);
        }
        if let Some(cam) = s.inside_camera(self.view, self.inside_look, self.inside_zoom) {
            s.scene.overlays.clear();
            renderer.render(&mut s.scene, target, w, h, &cam, &s.lighting);
            return;
        }
        let fov = 30.0f32;
        let aspect = w as f32 / h.max(1) as f32;
        // far enough that the whole bus fits the free part of the window
        let half_v = (fov.to_radians() * 0.5).tan();
        // (the free part is the share of the window right of the panels: twice the room
        // from the focus to the right edge)
        let free = ((1.0 - focus) * 2.0).clamp(0.3, 1.0);
        let fit = (s.length * 0.5 + 1.0) / (half_v * aspect * free * 0.92).max(0.1);
        // and its height the picture's (a wide, low picture - a card on a page - had a
        // double-decker cut at the roof and the wheels)
        let fit_v = (s.centre.z.max(1.0) * 1.3 + 1.0) / half_v.max(0.05);
        let dist = (fit.max(fit_v) * zoom).max(8.0);
        let (sy, cy) = yaw.to_radians().sin_cos();
        let (sp, cp) = pitch.to_radians().sin_cos();
        let target_pt = DVec3::new(s.centre.x as f64, s.centre.y as f64, (s.centre.z * 0.75) as f64);
        // from the camera towards the bus: forward along the yaw, down by the pitch
        let dir = DVec3::new((sy * cp) as f64, (cy * cp) as f64, -sp as f64);
        let mut pos = target_pt - dir * dist as f64;
        pos.z = pos.z.max(0.6);
        // the bus stands in the free part of the window: the camera looks past it to the
        // side by as much
        let side = (focus - 0.5) * 2.0 * half_v * aspect;
        let look_yaw = yaw - side.atan().to_degrees();
        let cam = Camera { position: pos, yaw: look_yaw, pitch: -pitch, roll: 0.0, fov_deg: fov, near: 0.2, far: 6000.0 };
        s.scene.overlays.clear();
        let _ = &s.weather;
        renderer.render(&mut s.scene, target, w, h, &cam, &s.lighting);
    }

    /// The picture's texture as last drawn (the desktop launcher's service reads it back).
    pub fn picture(&self) -> Option<&wgpu::Texture> {
        self.target.as_ref().map(|t| &t.0)
    }

    /// The camera is still easing towards where the mouse turned it (or turning by itself):
    /// more pictures are coming.
    pub fn moving(&self) -> bool {
        self.dirty || (self.yaw_to - self.yaw).abs() > 0.05 || (self.pitch_to - self.pitch).abs() > 0.05 || (self.zoom_to - self.zoom).abs() > 0.001 || (self.auto_turn && self.view == PreviewView::Outside && self.idle > 4.0)
    }

    /// The last picture drawn, as RGBA rows (width, height, pixels) - read back from the
    /// graphics card for a launcher that shows it itself (the Swift one, see `apple_bridge`).
    #[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
    pub fn pixels(&self, renderer: &Renderer) -> Option<(u32, u32, Vec<u8>)> {
        let (tex, _, w, h) = self.target.as_ref()?;
        let (w, h) = (*w, *h);
        // (rows of a copy are 256-byte aligned)
        let row = w * 4;
        let padded = row.div_ceil(256) * 256;
        let device = &renderer.device;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("bus preview read-back"), size: (padded * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("bus preview read-back") });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        renderer.queue.submit([enc.finish()]);
        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        rx.recv().ok()?.ok()?;
        let data = slice.get_mapped_range();
        let bgra = matches!(renderer.format(), wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        let mut out = Vec::with_capacity((row * h) as usize);
        for y in 0..h as usize {
            let line = &data[y * padded as usize..y * padded as usize + row as usize];
            if bgra {
                for p in line.chunks_exact(4) {
                    out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
                }
            } else {
                out.extend_from_slice(line);
            }
        }
        drop(data);
        buffer.unmap();
        Some((w, h, out))
    }

    /// A bus is there to show.
    pub fn has_picture(&self) -> bool {
        self.shown.as_ref().map(|s| s.vehicle.is_some()).unwrap_or(false) && self.target.is_some()
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::ManuallyDrop;

    fn look(bus: &str) -> Look {
        Look { bus: bus.into(), ..Look::default() }
    }

    #[test]
    fn uses_the_authored_standard_driver_camera_and_passenger_camera() {
        let mut def = omsi_vehicle::Vehicle::default();
        def.cameras_driver = vec![omsi_vehicle::Camera { yaw: 10.0, ..Default::default() }, omsi_vehicle::Camera { yaw: 35.0, ..Default::default() }];
        def.camera_std = 1;
        def.cameras_pax = vec![omsi_vehicle::Camera { yaw: 180.0, ..Default::default() }];
        assert_eq!(authored_camera(&def, PreviewView::Driver).unwrap().yaw, 35.0);
        assert_eq!(authored_camera(&def, PreviewView::Passenger).unwrap().yaw, 180.0);
        def.camera_std = 99;
        assert_eq!(authored_camera(&def, PreviewView::Driver).unwrap().yaw, 10.0);
        assert!(authored_camera(&def, PreviewView::Outside).is_none());
        assert!(authored_camera(&omsi_vehicle::Vehicle::default(), PreviewView::Passenger).is_none());
    }

    #[test]
    fn looking_and_zooming_inside_preserves_the_exterior_orbit() {
        let mut showroom = ManuallyDrop::new(Showroom::new());
        showroom.view = PreviewView::Passenger;
        let exterior = (showroom.yaw_to, showroom.pitch_to, showroom.zoom_to);
        showroom.orbit(100.0, 100.0);
        showroom.zoom_by(0.8);
        assert_eq!((showroom.yaw_to, showroom.pitch_to, showroom.zoom_to), exterior);
        assert_eq!(showroom.inside_look, (35.0, -25.0));
        assert!((showroom.inside_zoom - 0.8).abs() < 1e-6);
    }

    #[test]
    fn a_view_needs_a_bus_and_its_camera() {
        let mut showroom = ManuallyDrop::new(Showroom::new());
        assert!(!showroom.can_view(PreviewView::Outside));
        assert!(showroom.views().is_empty());
        showroom.set_view(PreviewView::Driver);
        assert_eq!(showroom.view, PreviewView::Outside);
        assert_eq!(PreviewView::from_name("passenger"), Some(PreviewView::Passenger));
        assert_eq!(PreviewView::from_name("roof"), None);
    }

    #[test]
    fn failure_ends_loading_and_hides_the_picture() {
        // Avoid Drop's installation-wide placing marker in these self-contained tests.
        let mut showroom = ManuallyDrop::new(Showroom::new());
        let selected = look("unavailable.bus");
        showroom.want(selected.clone());
        let (_tx, rx) = channel();
        showroom.loading = Some((selected.clone(), rx));
        let (_world_tx, world_rx) = channel();
        let (prefetch_tx, _prefetch_rx) = channel();
        showroom.opening = Some((world_rx, prefetch_tx));

        showroom.fail(selected.clone(), "Cannot load the bus".into());

        assert_eq!(showroom.failed.as_ref(), Some(&selected));
        assert_eq!(showroom.error.as_deref(), Some("Cannot load the bus"));
        assert!(showroom.loading.is_none());
        assert!(showroom.opening.is_none());
        assert!(!showroom.has_picture());
        assert!(showroom.picture().is_none());
    }

    #[test]
    fn changed_selection_clears_failure_but_unchanged_selection_does_not_retry() {
        let mut showroom = ManuallyDrop::new(Showroom::new());
        let failed = look("unavailable.bus");
        showroom.want(failed.clone());
        showroom.fail(failed.clone(), "Cannot load the bus".into());

        showroom.want(failed.clone());
        assert_eq!(showroom.failed.as_ref(), Some(&failed));
        assert!(showroom.error.is_some());

        showroom.want(look("available.bus"));
        assert!(showroom.failed.is_none());
        assert!(showroom.error.is_none());

        showroom.want(failed.clone());
        assert_eq!(showroom.wanted.as_ref(), Some(&failed));
        assert!(showroom.failed.is_none());
        assert!(showroom.error.is_none());
    }
}

/// The light of the look's time and weather, with the sun's shadow under the bus. Always
/// the plain renderer, whatever the game's graphics setting: a preview is to be quick and
/// clear, not the game's picture (no enhanced exposure and glow, no weather effects).
fn lighting_for(args: &Args, weather: &omsi_content::weather::Weather, sky: &crate::weather_setup::SkyState) -> Lighting {
    let clock = start_clock(args);
    let envir = omsi_content::Envir::load(&args.root.join("envir.cfg")).ok();
    let daylight = omsi_sim::Daylight::compute(&clock, envir.as_ref());
    let mut l = weather_lighting(&daylight, weather, crate::weather_setup::cloud_drift_at(weather, clock.time), 0.0, true, sky);
    l.shadows = daylight.altitude_deg > 2.0;
    l.enhanced = false;
    l.classic = false;
    l.detail = false;
    l.wetness = 0.0;
    l.snow = 0.0;
    l.fog_density = 0.0;
    l
}

/// A round showroom floor under the bus: dark, matt, catching its shadow.
fn add_floor(renderer: &Renderer, scene: &mut Scene) {
    let n = 96;
    let r = 400.0f32;
    let mut positions = vec![glam::Vec3::ZERO];
    let mut normals = vec![glam::Vec3::Z];
    let mut uvs = vec![glam::Vec2::ZERO];
    for k in 0..n {
        let a = std::f32::consts::TAU * k as f32 / n as f32;
        positions.push(glam::Vec3::new(a.cos() * r, a.sin() * r, 0.0));
        normals.push(glam::Vec3::Z);
        uvs.push(glam::Vec2::new(a.cos(), a.sin()));
    }
    let mut indices = Vec::new();
    for k in 0..n {
        indices.extend([0u32, 1 + ((k + 1) % n) as u32, 1 + k as u32]);
    }
    let data = omsi_geometry::MeshData { positions, normals, uvs, ranges: vec![(0, indices.len() as u32, 0)], indices, one_sided: false };
    let mesh = renderer.add_mesh(scene, &data);
    let mat = renderer.add_material(scene, None, omsi_render::AlphaMode::Opaque, [0.12, 0.125, 0.135, 1.0], false);
    renderer.add_instance(scene, mesh, DVec3::new(0.0, 0.0, -0.005), glam::Mat4::IDENTITY, vec![mat]);
}

#[cfg(test)]
mod guard_tests {
    use super::*;

    #[test]
    fn old_preview_failures_do_not_block_a_new_version() {
        let dir = std::env::temp_dir().join(format!("openomsi-preview-old-failures-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bus = "Vehicles/test/bus.bus";
        std::fs::write(dir.join("showroom-skip.txt"), bus).unwrap();
        std::fs::write(dir.join("showroom-placing.txt"), bus).unwrap();
        std::fs::write(preview_guard_path(&dir, "skip", "0.1.1"), bus).unwrap();
        std::fs::write(preview_guard_path(&dir, "placing", "0.1.1"), bus).unwrap();

        assert!(!preview_is_skipped(&dir, "0.1.2", bus));
        assert!(!preview_guard_path(&dir, "skip", "0.1.2").exists());
        assert!(preview_guard_path(&dir, "placing", "0.1.1").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unfinished_preview_is_still_blocked_in_the_same_version() {
        let dir = std::env::temp_dir().join(format!("openomsi-preview-current-failure-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bus = "Vehicles/test/bus.bus";
        std::fs::write(preview_guard_path(&dir, "placing", "0.1.1"), bus).unwrap();

        assert!(preview_is_skipped(&dir, "0.1.1", bus));
        assert!(!preview_guard_path(&dir, "placing", "0.1.1").exists());
        assert!(preview_is_skipped(&dir, "0.1.1", bus));
        assert!(!preview_is_skipped(&dir, "0.1.1", "Vehicles/test/other.bus"));
        assert!(!preview_is_skipped(&dir, "0.1.2", bus));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
