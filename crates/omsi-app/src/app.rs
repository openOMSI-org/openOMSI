//! The window's game: `App`, its state and its per-frame work.

use super::*;

mod groups;
pub(crate) use groups::*;
mod parts;
pub(crate) use parts::*;

const SLOW_UPLOAD_MB_S: f64 = 300.0;

/// The loading screen's colour (the sky of its blank camera, see `drive_start`): what the
/// game's window shows before its renderer is there - the loading screen's own #121212 (in
/// linear light), so the window does not show a blue-grey before it.
const LOADING_BACKGROUND: wgpu::Color = wgpu::Color { r: 0.0060, g: 0.0060, b: 0.0060, a: 1.0 };

/// The game window's start while its renderer is being built (see `App::create_window`).
pub(crate) struct WindowStart {
    start: crate::startup::RendererStart,
    /// The map of a session given on the command line, opened on a worker meanwhile.
    world: Option<std::thread::JoinHandle<Result<(World, Camera, DVec3)>>>,
    /// The window was made hidden: it is shown with its first picture.
    show: bool,
    /// When the last plain picture was drawn, and how many were.
    shown: Option<Instant>,
    tries: u32,
    /// A surface was made on the device at the start (see `window_ready`).
    had_surface: bool,
}

/// The log's word on the window's first picture (how long the start took to show anything).
/// (A hidden window's - OMSI_HIDDEN_WINDOW - is drawn and not shown: said all the same.)
fn first_picture(started: Instant, shown: bool) {
    static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if (shown || omsi_cfg::flags::OMSI_HIDDEN_WINDOW.is_set()) && !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
        log::info!("window: first picture {} {:.2} s after the start", if shown { "shown" } else { "drawn (the window is hidden)" }, started.elapsed().as_secs_f64());
    }
}

pub(crate) struct App {
    pub(crate) args: Args,
    /// What draws the picture besides the renderer and the scene (see `GfxState`).
    pub(crate) gfx: GfxState,
    pub(crate) window: Option<Arc<Window>>,
    pub(crate) renderer: Option<Renderer>,
    /// The VR headset and what goes with it (see `VrState`).
    pub(crate) xr: VrState,
    pub(crate) scene: Option<Scene>,
    pub(crate) camera: Option<Camera>,
    pub(crate) player: Option<Player>,
    /// The simulated world around the player's bus (see `SessionState`).
    pub(crate) session: SessionState,
    /// The menus, lists, editor and on-screen panels (see `MenuState`).
    pub(crate) menus: MenuState,
    pub(crate) world: Option<Arc<World>>,
    /// Where the camera looks from and to, per view (see `ViewState`).
    pub(crate) cam: ViewState,
    /// Chat, mouse-over names and name tags (Inter).
    pub(crate) ui: Option<ui::Ui>,
    /// Frame timing, profiling and the test hooks (see `PerfState`).
    pub(crate) perf: PerfState,
    /// What the game sounds out: the engine, the world around, the radio, the voice chat.
    pub(crate) sound: SoundState,
    pub(crate) clock: omsi_sim::SimClock,
    pub(crate) started: Instant,
    pub(crate) view: String,
    /// The keyboard, the mouse, controllers, head tracking and touch (see `InputState`).
    pub(crate) input: InputState,
    pub(crate) last: Instant,
    /// The simulation stands still (OMSI's `sim_pause`, P, or the menu): nothing moves,
    /// the clock stops, the picture and the camera go on.
    pub(crate) paused: bool,
    /// Plugins and the services outside the game (see `Integrations`).
    pub(crate) integrations: Integrations,
    /// The multiplayer session and the other players (see `NetState`).
    pub(crate) net: NetState,
    /// Last workshop / fuel pump / wash message, and how long it still shows.
    pub(crate) service_msg: Option<(String, f32)>,
    pub(crate) settings: settings::Settings,
    /// The exit is under way.
    pub(crate) exiting: bool,
    /// The pause menu, its windows and the photo mode, drawn with the launcher's toolkit.
    pub(crate) shell: crate::shell::Shell,
    /// The photo mode, while it is on.
    pub(crate) photo: Option<crate::photo::Photo>,
    /// The photo mode's settings, kept from one photo to the next in the session.
    pub(crate) photo_settings: Option<crate::photo::Settings>,
    /// The developer tools, while they are open (Ctrl+Shift+D).
    pub(crate) devtools: Option<Box<crate::devtools::DevTools>>,
    /// The simulation's fixed step (see `app_events/frame/ticked.rs`).
    pub(crate) fixed_tick: omsi_sim::tick::SimTicks,
    /// The sim thread and the steps out on it while the picture is drawn (`OMSI_SIM_THREAD`).
    pub(crate) pipeline: crate::sim_thread::Pipeline,
}

impl App {
    #[cfg(windows)]
    pub(crate) fn vr_active(&self) -> bool { self.xr.vr.is_some() }

    #[cfg(not(windows))]
    pub(crate) fn vr_active(&self) -> bool { false }

    pub(crate) fn resumed_impl(&mut self, event_loop: &ActiveEventLoop) {
        crate::frame_stages::log_order();
        if let Some(window) = self.window.clone() {
            // back from the background (a phone): the window's surface is made again (on the
            // device whose renderer is still being built, while it is)
            if self.gfx.surface.is_none() {
                let size = crate::platform::surface_size(&window);
                if let Some(r) = self.renderer.as_ref() {
                    let vsync = self.settings.vsync && !self.vr_active();
                    self.gfx.surface = SurfaceState::new_with(&self.gfx.instance, window.clone(), r, size.width.max(1), size.height.max(1), vsync).ok();
                    self.last = Instant::now();
                } else if let Some(ws) = self.gfx.starting.as_ref().filter(|ws| ws.start.early()) {
                    self.gfx.surface = SurfaceState::early(&self.gfx.instance, window.clone(), &ws.start.device, ws.start.format, size.width.max(1), size.height.max(1), self.settings.vsync).ok();
                }
            }
            return;
        }
        self.create_window(event_loop, None);
    }

    /// The game's window (or the launcher's, handed over on a phone), its surface and the
    /// renderer; then the menu or, when the session is given, the world.
    pub(crate) fn create_window(&mut self, event_loop: &ActiveEventLoop, given: Option<Arc<Window>>) {
        // --size sets the window's size in points as well (1600x900 unless given)
        let (lw, lh) = self
            .args
            .size
            .split_once('x')
            .map(|(a, b)| {
                (
                    a.parse::<u32>().unwrap_or(1600),
                    b.parse::<u32>().unwrap_or(900),
                )
            })
            .unwrap_or((1600, 900));
        let (fit, at) = crate::startup::fit_window(event_loop, lw as f64, lh as f64);
        let mut attrs = Window::default_attributes()
            .with_title("openOMSI")
            .with_inner_size(fit)
            .with_window_icon(crate::startup::window_icon());
        if let Some(at) = at {
            attrs = attrs.with_position(at);
        }
        // the window size of the settings (pixels, #904) unless --size names one
        let resolution = crate::settings::Settings::resolution().filter(|_| self.args.size == crate::cli::DEFAULT_SIZE);
        if let Some((w, h)) = resolution {
            attrs = attrs.with_inner_size(winit::dpi::PhysicalSize::new(w, h));
            if let Some(m) = crate::startup::home_monitor(event_loop) {
                let (sw, sh) = (m.size().width as i32, m.size().height as i32);
                attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(m.position().x + ((sw - w as i32) / 2).max(0), m.position().y + ((sh - h as i32) * 2 / 5).max(0)));
            }
        }
        // (a Steam Deck's Gaming Mode: gamescope shows one window over the whole screen and
        // scales whatever size it has to it - a window fitted to 90 % of the screen came out
        // blurred and letterboxed. There the window is the screen's, unless a size is set)
        let gamescope = resolution.is_none() && crate::startup::under_gamescope();
        if gamescope {
            log::info!("gamescope (Steam Deck Gaming Mode): the window fills the screen");
        }
        if self.settings.fullscreen || gamescope {
            attrs = attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(crate::startup::home_monitor(event_loop))));
        }
        if self.settings.triple.enabled
            && self.settings.triple_span
            && !self.settings.vr_requested()
        {
            let mut monitors: Vec<_> = event_loop.available_monitors().collect();
            monitors.sort_by_key(|m| m.position().x);
            let row = monitors.windows(3).find(|row| {
                let size = row[0].size();
                row.iter()
                    .all(|m| m.size() == size && m.position().y == row[0].position().y)
                    && row[1].position().x == row[0].position().x + size.width as i32
                    && row[2].position().x == row[1].position().x + size.width as i32
            });
            if let Some(row) = row {
                let size = row[0].size();
                if self.settings.fullscreen || gamescope || resolution.is_some() || self.args.size != crate::cli::DEFAULT_SIZE {
                    log::info!("triple screen: spanning three monitors instead of the fullscreen / window size settings");
                }
                self.gfx.spanned = true;
                attrs = attrs
                    .with_fullscreen(None)
                    .with_decorations(false)
                    .with_position(row[0].position())
                    .with_inner_size(winit::dpi::PhysicalSize::new(size.width * 3, size.height));
                log::info!("triple screen: spanning {}x{}", size.width * 3, size.height);
            } else {
                log::warn!("triple screen: no equal horizontal monitor row found; using configured window size (Surround/Eyefinity can expose one wide display)");
            }
        }
        // OMSI_BACKGROUND=1: a test window that does not take the keyboard from whoever is
        // working at the screen (OMSI_INPUT drives the handlers directly, it needs no focus)
        if omsi_cfg::flags::OMSI_BACKGROUND.is_set() || omsi_cfg::flags::OMSI_HIDDEN_WINDOW.is_set() {
            attrs = attrs.with_active(false);
        }
        // OMSI_HIDDEN_WINDOW=1: the window is never shown at all, so a benchmark of the
        // window's own frame (its steps, OMSI_PROFILE's stages) can run beside whoever works
        // at the screen; macOS then reports it occluded and the frames are drawn into a
        // texture of its size (see `frame_acquire`)
        if omsi_cfg::flags::OMSI_HIDDEN_WINDOW.is_set() {
            attrs = attrs.with_visible(false);
        }
        // The window is shown once it has a picture. Shown when it was made, Windows put it
        // up at its windowed size, enlarged it to the full screen and painted neither: a
        // white rectangle in the corner of a black screen for the seconds the renderer was
        // built and the map opened. (Not a phone's window, which is on the screen already;
        // nor OMSI_BACKGROUND's, which is shown without the focus only when it is made; nor
        // on macOS, whose windows have a background of their own and whose full screen is a
        // transition of its own on a window that is shown.)
        let show_later = given.is_none()
            && !omsi_cfg::flags::OMSI_HIDDEN_WINDOW.is_set()
            && !omsi_cfg::flags::OMSI_BACKGROUND.is_set()
            && !cfg!(target_os = "macos");
        if show_later {
            attrs = attrs.with_visible(false);
        }
        let window = match given {
            Some(w) => w,
            // (no display to open it on, a compositor that refuses it: said so, not a panic
            // report about "window")
            None => match event_loop.create_window(attrs) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    fatal_message(&format!("The game cannot open its window: {e}"));
                    crate::platform::exit(event_loop);
                    return;
                }
            },
        };
        let start = match crate::startup::RendererStart::begin(&mut self.gfx.instance, &window, omsi_render::RenderOptions { max_scene: resolution.filter(|_| self.settings.fullscreen && !self.gfx.spanned), ..self.args.session_settings.render_options(&self.settings) }) {
            Ok(s) => s,
            Err(e) => {
                fatal_message(&format!("The game cannot draw on this computer: {e:#}"));
                crate::platform::exit(event_loop);
                return;
            }
        };
        // the window's surface on the device already, for the pictures until the renderer
        // is there
        let early = start.early().then(|| {
            let size = crate::platform::surface_size(&window);
            SurfaceState::early(&self.gfx.instance, window.clone(), &start.device, start.format, size.width, size.height, self.settings.vsync).ok()
        }).flatten();
        let had_surface = early.is_some();
        self.gfx.surface = early;
        // the map of a session given on the command line is opened meanwhile (it needs no
        // renderer; a player joining another's game takes the host's world first, so not
        // that one)
        let session = self.args.bus.is_some() || self.args.cam.is_some() || self.args.no_menu;
        let world = (session && !self.args.all && self.args.radius.is_none() && self.net.lan.is_none()).then(|| {
            let args = self.args.clone();
            std::thread::Builder::new().name("map opening".into()).spawn(move || open_world(&args)).ok()
        }).flatten();
        self.window = Some(window);
        self.gfx.starting = Some(WindowStart { start, world, show: show_later, shown: None, tries: 0, had_surface });
        self.drive_window_start(event_loop);
    }

    /// The window still hidden at its start, waiting for its first picture (see `create_window`).
    pub(crate) fn starts_hidden(&self) -> bool {
        self.gfx.starting.as_ref().is_some_and(|ws| ws.show)
    }

    /// The window's start while its renderer is being built (see `create_window`): a plain
    /// picture in the loading screen's colour now and then, the window shown with the first
    /// one, and once the renderer - and the map opened meanwhile - is there, the rest of the
    /// window's start. True while it goes on: nothing else of the frame is done.
    pub(crate) fn drive_window_start(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let Some(ws) = self.gfx.starting.as_mut() else { return false };
        let ready = ws.start.is_done() && ws.world.as_ref().is_none_or(|h| h.is_finished());
        if !ready {
            // (once, and then twice a second: the window keeps its picture, and the main
            // thread keeps taking the window's messages - Windows calls a window that takes
            // none for five seconds "not responding")
            if ws.shown.is_none_or(|t| t.elapsed().as_secs_f32() > 0.5) {
                ws.shown = Some(Instant::now());
                if let (Some(s), Some(win)) = (self.gfx.surface.as_mut(), self.window.as_ref()) {
                    let shown = crate::startup::present_clear(s, &ws.start.device, &ws.start.queue, win, LOADING_BACKGROUND);
                    first_picture(self.started, shown);
                    // (shown all the same when a hidden window gives no picture to draw
                    // into: better a blank window than none)
                    ws.tries += 1;
                    if (shown || ws.tries >= 3) && std::mem::take(&mut ws.show) {
                        win.set_visible(true);
                    }
                }
            } else {
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
            if let Some(limit) = self.args.exit_after {
                if self.started.elapsed().as_secs_f32() > limit {
                    log::info!("exit after {limit} s while the renderer was built");
                    crate::platform::exit(event_loop);
                }
            }
            return true;
        }
        let ws = self.gfx.starting.take().expect("window start");
        let device = ws.start.device.clone();
        let renderer = match ws.start.wait(&mut self.gfx.instance) {
            Ok(r) => r,
            Err(e) => {
                fatal_message(&format!("The game cannot draw on this computer: {e:#}"));
                crate::platform::exit(event_loop);
                return true;
            }
        };
        let world = ws.world.and_then(|h| h.join().ok());
        self.window_ready(event_loop, renderer, &device, world, ws.show, ws.had_surface);
        false
    }

    /// The rest of the window's start, with its renderer: the surface, the scene, then the
    /// menu or, when the session is given, the world (`world`: its map opened meanwhile).
    fn window_ready(&mut self, event_loop: &ActiveEventLoop, mut renderer: Renderer, device: &wgpu::Device, world: Option<Result<(World, Camera, DVec3)>>, show: bool, had_surface: bool) {
        let Some(window) = self.window.clone() else { return };
        #[cfg(windows)]
        if self.settings.vr_requested() {
            match crate::openxr::Vr::new(&renderer, self.settings.vr_scale, self.settings.vr_desktop_mirror) {
                Ok(vr) => self.xr.vr = Some(vr),
                Err(e) => log::error!("OpenXR could not start: {e:#}"),
            }
        }
        let upload = renderer.upload_speed_mb_s();
        log::info!("graphics: {upload:.0} MB/s copied towards the card");
        if upload < SLOW_UPLOAD_MB_S {
            log::error!("graphics: the driver copies only {upload:.0} MB/s towards the card (thousands are usual); every texture and buffer the game sends waits on it, down to a few frames a second - restarting the computer usually brings it back");
            self.service_msg = Some((format!("Graphics driver is slow ({upload:.0} MB/s): the game will stutter. Restarting the computer usually fixes it."), 30.0));
        }
        crate::lights::load_smoke_texture(&mut renderer, &self.args.root);
        crate::lights::set_corona_root(&self.args.root);
        let size = crate::platform::surface_size(&window);
        let vsync = self.settings.vsync && !self.vr_active();
        // the early surface goes on with the renderer; made anew when the renderer is on
        // another device, or when there was none (ANGLE's, see `RendererStart::begin`). A
        // phone that took it away meanwhile (the app went to the background) gets it again
        // when it comes back (`resumed_impl`).
        let adopted = self.gfx.surface.as_mut().is_some_and(|s| s.adopt(&renderer, device));
        if !adopted && (self.gfx.surface.is_some() || !had_surface) {
            self.gfx.surface = None;
            match SurfaceState::new_with(&self.gfx.instance, window.clone(), &renderer, size.width, size.height, vsync) {
                Ok(s) => self.gfx.surface = Some(s),
                Err(e) => {
                    fatal_message(&format!("The game's window cannot be drawn into: {e:#}"));
                    crate::platform::exit(event_loop);
                    return;
                }
            }
        }
        if let Some(surface) = self.gfx.surface.as_mut() {
            if surface.config.width != size.width.max(1) || surface.config.height != size.height.max(1) {
                surface.resize(&renderer, size.width, size.height);
            }
            surface.set_vsync(&renderer, vsync);
            let (sw, sh) = renderer.scene_size(size.width, size.height);
            log::info!(
                "window: {}x{} pixels (scale factor {:.2}), 3D picture {sw}x{sh}, present mode {:?}",
                size.width,
                size.height,
                window.scale_factor(),
                surface.config.present_mode
            );
            // (a window that had no picture yet - ANGLE's, or one whose device was lost - is
            // given one before it is shown)
            if show {
                let shown = crate::startup::present_clear(surface, &renderer.device, &renderer.queue, &window, LOADING_BACKGROUND);
                first_picture(self.started, shown);
            }
        }
        if show {
            window.set_visible(true);
        }
        let scene = renderer.new_scene();
        self.renderer = Some(renderer);
        self.scene = Some(scene);
        // fully specified runs skip the menu
        if self.args.bus.is_some() || self.args.cam.is_some() || self.args.no_menu {
            self.load_world_with(event_loop, world);
        } else {
            let mut fonts = omsi_sim::texttex::FontLibrary::new(&self.args.root);
            self.menus.hud = Some(hud::Hud::new(&mut fonts));
            self.menus.menu = Some(menu::Menu::new(&self.args.root, &self.args.map));
        }
    }

    /// Load the map, vehicle and traffic according to `args`.
    pub(crate) fn load_world_now(&mut self, event_loop: &ActiveEventLoop) {
        self.load_world_with(event_loop, None);
    }

    /// `load_world_now`, with the map opened already (on a worker while the window started).
    fn load_world_with(&mut self, event_loop: &ActiveEventLoop, opened: Option<Result<(World, Camera, DVec3)>>) {
        // a joining player loads the host's date, time, weather and season (after the menu)
        if let Some(l) = self.net.lan.as_mut() {
            let mut c = self.args.lan_config();
            lan::adopt_host_world(&mut c, &crate::app_impl::lan_tick::AppLanHost { args: &self.args, world: None }, l, &mut self.net.remotes);
            self.args.take_lan_config(&c);
        }
        let renderer = self.renderer.take().expect("renderer");
        let mut scene = renderer.new_scene();
        self.session.envir = omsi_content::Envir::load(&self.args.root.join("envir.cfg")).ok();
        // the weather cycle: a first weather that suits the month, the others after it
        if crate::weather_cycle::is_cycle(self.args.weather.as_deref()) {
            let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7);
            let mut c = crate::weather_cycle::Cycle::new(seed);
            let month = start_clock(&self.args).day_month().1;
            let all = crate::weather_cycle::installed();
            let clear = omsi_content::weather::Weather { fog: (50000.0, 1.0), ..Default::default() };
            let r = c.rand();
            self.args.weather = crate::weather_cycle::pick(&all, &clear, "", month, r);
            log::info!("weather cycle: starting with {:?}", self.args.weather);
            self.session.weather_cycle = Some(c);
        }
        let (weather, model) = load_weather_run(&self.args);
        self.session.weather = Some(weather);
        self.session.sky.model = model;
        // the roads start in the state this weather has already left them in, as they do
        // offscreen: a session begun in the rain used to open on a bone-dry street
        self.session.wetness = self.session.weather.as_ref().map(initial_wetness).unwrap_or(0.0);
        self.clock = start_clock(&self.args);
        setup_sky(&self.args, &renderer, &mut scene, self.session.envir.as_ref(), self.session.weather.as_ref(), &mut self.session.sky.clouds);
        // the window streams the tiles around the camera unless a fixed area was asked for
        if !self.args.all && self.args.radius.is_none() {
            match opened.unwrap_or_else(|| open_world(&self.args)) {
                Ok((w, cam, _)) => {
                    let w = Arc::new(w);
                    let distance = self
                        .args
                        .view_distance
                        .or_else(settings::view_distance)
                        .unwrap_or(1200.0)
                        .max(omsi_map::tile_size());
                    w.set_fast_texture_loads(true);
                    let video = video_memory_limit(&self.settings);
                    let textures = texture_budget(video);
                    w.set_texture_budget(textures);
                    w.set_video_memory_limit(video);
                    log::info!(
                        "texture budget: {:.0} MB, video memory: {:.0} MB (0 = the driver's budget)",
                        textures as f64 / 1e6,
                        video as f64 / 1e6
                    );
                    self.gfx.streamer = Some(tiles::Streamer::new(
                        w.clone(),
                        &start_centers(&self.args, &cam, Some(&w)),
                        distance,
                        700.0,
                        crate::release_free_memory,
                    ));
                    self.world = Some(w);
                    self.cam.starting = Some(cam);
                }
                Err(e) => {
                    log::error!("{e:#}");
                    crate::platform::exit(event_loop);
                }
            }
            self.renderer = Some(renderer);
            self.scene = Some(scene);
            self.last = Instant::now();
            return;
        }
        match load_world(&self.args, &renderer, &mut scene) {
            Ok((w, cam)) => self.start_world(Arc::new(w), cam, &renderer, &mut scene),
            Err(e) => {
                log::error!("{e:#}");
                crate::platform::exit(event_loop);
            }
        }
        self.renderer = Some(renderer);
        self.scene = Some(scene);
        self.last = Instant::now();
    }

    /// Everything that needs the map to stand: the player's bus, the passengers, the traffic
    /// and the timetable.
    pub(crate) fn start_world(
        &mut self,
        w: Arc<World>,
        cam: Camera,
        renderer: &Renderer,
        mut scene: &mut Scene,
    ) {
        report_missing_content(&w, &mut self.service_msg);
        {
            {
                // (once more when it fails: a file read while the start was still reading
                // others; a failure is said on the screen - the game went on without a bus
                // and the player found himself on foot, with no word why)
                let first = spawn_player(&self.args, &w, &renderer, &mut scene);
                let spawned = match first {
                    Err(e) if self.args.bus.is_some() => {
                        log::warn!("the bus could not be put down ({e:#}); trying again");
                        spawn_player(&self.args, &w, &renderer, &mut scene).map_err(|e2| {
                            self.service_msg = Some((format!("The bus could not be loaded: {}", format!("{e2:#}").lines().next().unwrap_or_default()), 15.0));
                            e2
                        })
                    }
                    other => other,
                };
                match spawned {
                    Ok(mut p) => {
                        let audio = omsi_audio::AudioEngine::new();
                        audio.set_doppler(self.settings.doppler);
                        if let Some(p) = p.as_mut() {
                            p.vehicle.host.auto_clutch = if self.settings.auto_clutch { 1.0 } else { 0.0 };
                            p.load_sounds(&audio);
                            p.ibis_background = true;
                            // --autostart applies in the window too, not only offscreen
                            if self.args.autostart && !self.args.is_resuming() {
                                let msg = p.start_up();
                                self.service_msg = Some((msg, 6.0));
                            }
                            // a pack this bus borrows parts from is not installed: say so
                            // once, it explains dark displays and missing devices
                            if !p.vehicle.ty.missing_packs.is_empty() {
                                let packs: Vec<String> = p.vehicle.ty.missing_packs.iter().map(|(n, _)| n.clone()).collect();
                                let msg = format!(
                                    "This bus takes parts from vehicle pack(s) that are not installed: {} (install them for its displays and devices)",
                                    packs.join(", ")
                                );
                                self.service_msg = Some(match self.service_msg.take() {
                                    Some((m, _)) => (format!("{m}   |   {msg}"), 12.0),
                                    None => (msg, 12.0),
                                });
                            }
                        }
                        self.sound.ambience = Some(ambience::Ambience::load(&audio, &self.args.root));
                        self.sound.soundscape = Some(crate::soundscape::Soundscape::new(self.settings.ambient, self.settings.vol_ambient));
                        self.sound.audio = Some(audio);
                        if let Some(p) = &p {
                            if self.args.cam.is_none() && self.args.view != "free" {
                                self.camera = Some(p.camera(&self.args.view, &cam));
                            }
                        }
                        self.player = p;
                    }
                    Err(e) => log::error!("{e:#}"),
                }
                // a situation's further vehicles, each as it was saved
                for o in self.args.situation_others.clone() {
                    let one = Args {
                        bus: Some(o.bus.clone()),
                        spawn: Some(o.spawn.clone()),
                        hof: o.hof.clone(),
                        paint: o.paint.clone(),
                        situation_vars: o.vars.clone(),
                        situation_strvars: o.strvars.clone(),
                        situation_odometer_km: o.odometer_km,
                        situation_others: Vec::new(),
                        line: None,
                        tour: None,
                        trip: None,
                        autostart: false,
                        situation_next_stop: None,
                        ..self.args.clone()
                    };
                    match spawn_player(&one, &w, &renderer, &mut scene) {
                        Ok(Some(q)) => {
                            log::info!("situation: {} placed at {}", o.bus, o.spawn);
                            self.session.placed.push(q);
                        }
                        Ok(None) => {}
                        Err(e) => log::warn!("situation vehicle {}: {e:#}", o.bus),
                    }
                }
                if self.camera.is_none() {
                    self.camera = Some(cam);
                }
                self.menus.navigator = Some(navigator::Navigator::new(
                    self.settings.navigator,
                    self.settings.ui_opacity,
                    &self.settings.navigator_corner,
                ));
                if let Some(n) = self.menus.navigator.as_mut() {
                    n.arrows = self.settings.nav_arrows;
                    n.show_ai = self.settings.nav_ai;
                    n.show_surface = self.settings.nav_surface;
                    n.light = self.settings.nav_light;
                    n.show_3d = self.settings.nav_3d;
                    n.widget = self.settings.nav_widget;
                    n.size = self.settings.nav_size;
                }
                if let Some(d) = self.args.driver.as_deref() {
                    self.session.career = career::Career::load(&self.args.root, d);
                }
                // (and a player who joins another's game sees the host's people)
                if self.args.passengers || self.args.lan_join.is_some() {
                    let mut h = humans::Humans::new(&self.args.root, &mut self.gfx.sim_view.people);
                    if let Some(lan) = self.net.lan.as_ref() {
                        h.set_lan_seed(lan::population_seed(lan));
                    }
                    h.exact_fare = self.settings.exact_fare;
                    h.bad_driving = self.settings.pax_bad_driving;
                    h.boarding = self.settings.boarding.clone();
                    h.stand_chance = self.settings.standing_chance;
                    h.voices = match self.settings.pax_voices.as_str() { "off" => 2, "tickets" => 1, _ => 0 };
                    if let Some(p) = self.player.as_mut() {
                        h.set_cabin(&mut p.vehicle);
                        h.ticket_key = ticket_key_name(&self.args.root, &p.bindings);
                        h.tickets = p.vehicle.host.tickets.clone();
                        if !w.global.money_system.trim().is_empty() {
                            h.money =
                                Some(money::Money::new(&self.args.root, &w.global.money_system));
                        }
                    }
                    if let Some(p) = self.player.as_ref() {
                        if self.args.riders > 0 {
                            let centre = p.vehicle.position;
                            h.populate(&w, centre);
                            crate::view_sync::apply_people(&mut h, &mut self.gfx.sim_view.people, &w, &renderer, &mut scene);
                            h.seed_riders(self.args.riders, &p.vehicle, &w);
                            crate::view_sync::apply_people(&mut h, &mut self.gfx.sim_view.people, &w, &renderer, &mut scene);
                        }
                    }
                    self.session.humans = Some(h);
                }
                // (a player who joins draws the host's traffic in it, whatever their own count
                // says: the host's cars had nowhere to go without it)
                let populated = self.args.traffic > 0 || self.args.schedule || crate::rail_drive::args_rail(&self.args) || self.args.lan_join.is_some();
                // Without traffic it still runs the light programs and switches the lamps:
                // they stood frozen with red, yellow and green all lit (#727).
                {
                    match traffic::Traffic::new(&self.args.root, &w, self.args.traffic) {
                        Ok(mut t) => {
                            t.lights_only = !populated;
                            t.no_timetable_buses = self.args.no_timetable_buses;
                            if let Some(lan) = self.net.lan.as_ref() {
                                t.set_lan_seed(lan::population_seed(lan));
                            }
                            if self.args.traffic > 0 {
                                t.precache_random(&w, &renderer, &mut scene);
                            }
                            t.day_time = parse_time(&self.args.time);
                            self.gfx.sim_view.traffic = Default::default();
                            self.session.traffic = Some(t);
                        }
                        Err(e) => log::error!("traffic: {e:#}"),
                    }
                    if self.args.schedule {
                        let mut sch =
                            schedule::Schedule::new(&self.args.root, &w, &start_clock(&self.args));
                        // (the tours' buses drawn alike on every machine of a LAN game)
                        if let Some(lan) = self.net.lan.as_ref() {
                            sch.sim.vehicle_seed = omsi_script::mix_seed(lan::population_seed(lan) ^ 0x746f_7572_5f62_7573);
                        }
                        sch.precache(
                            &w,
                            &renderer,
                            &mut scene,
                            self.session.traffic.as_mut(),
                            parse_time(&self.args.time),
                        );
                        if let (Some(line), Some(p)) = (&self.args.line, self.player.as_mut()) {
                            self.session.duty = match sch.player_duty(
                                &w,
                                line,
                                self.args.tour.as_deref().unwrap_or(""),
                                parse_time(&self.args.time),
                                self.args.trip.as_deref(),
                                self.args.whole_tour,
                            ) {
                                Ok(mut d) => {
                                    if let Some(k) = self.args.duty_trip {
                                        d.start_at(k, self.args.duty_first_stop);
                                    }
                                    if self.args.is_resuming() {
                                        d.resume(&mut p.vehicle, parse_time(&self.args.time), self.args.situation_next_stop);
                                        // the IBIS keeps its saved trip; with --autostart
                                        // the duty's next trips are typed into it again
                                        p.duty_typed = self.args.autostart;
                                    }
                                    Some(d)
                                }
                                Err(e) => {
                                    log::warn!("no player duty: {e}");
                                    self.service_msg = Some((format!("No duty: {e}"), 20.0));
                                    None
                                }
                            };
                            // --autostart in the window puts the duty on the IBIS as well
                            // (it only ever did offscreen: the duty did not exist yet when
                            // the start-up began, and the displays stayed dark)
                            if let (true, Some(d)) = (
                                self.args.autostart && !self.args.is_resuming(),
                                self.session.duty.as_mut(),
                            ) {
                                d.update(&mut p.vehicle, parse_time(&self.args.time));
                                let (trip, stop) = d.trip_for_ibis();
                                p.set_duty_destination(trip, stop);
                            }
                            if let Some(d) = self.session.duty.as_ref() {
                                let mut fonts = w.fonts.lock();
                                if let Err(e) = crate::schedule_paper::update_vehicle(
                                    &mut p.vehicle,
                                    d,
                                    &mut fonts,
                                ) {
                                    log::warn!("driver timetable paper: {e:#}");
                                }
                            }
                        }
                        self.session.schedule = Some(sch);
                    } else if !crate::rail_drive::args_rail(&self.args) && self.args.lan_join.is_none() {
                        // (the timetable is still the map's to read and pick a duty from: a
                        // book, nothing of it runs - see `SessionState::timetable_book`)
                        self.session.timetable_book =
                            Some(schedule::Schedule::new(&self.args.root, &w, &start_clock(&self.args)));
                    }
                }
                if self.session.traffic.is_none() {
                    if let Some(n) = self.menus.navigator.as_mut() {
                        n.add_lanes(omsi_sim::world_data::TrafficArrivals::gather(w.traffic_inbox.take()).lanes);
                    }
                }
                if let (Some(lan), Some(p)) = (self.net.lan.as_mut(), self.player.as_mut()) {
                    lan::settle_spawn(
                        lan,
                        &mut self.net.remotes,
                        p,
                        &self.args,
                        &w,
                        self.session.traffic.as_ref().map(|t| &t.net),
                        // (still loading: a moment's wait for the host's list puts the bus
                        // where it is free at once - without it the bus stood inside another
                        // player's for the first frames and then jumped out of it)
                        std::time::Duration::from_millis(1500),
                    );
                }
                self.world = Some(w);
            }
        }
        self.last = Instant::now();
    }

    /// The first area of a streamed map is loading, then what holds the start (`omsi_lan::game::start_held`):
    /// show how far it got, start the world once it is there. False while the loading screen is up.
    pub(crate) fn drive_start(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let Some(cam) = self.cam.starting.take() else {
            let Some((caption, progress)) = omsi_lan::game::start_held() else { return true };
            if let (Some(renderer), Some(scene)) = (self.renderer.take(), self.scene.take()) { self.loading_frame(renderer, scene, &caption, progress) }
            // (--exit-after holds while the start waits - our bus on its way - as while loading)
            if let Some(limit) = self.args.exit_after.filter(|l| self.started.elapsed().as_secs_f32() > *l) {
                log::info!("exit after {limit} s while the start waited: {caption}");
                crate::platform::exit(event_loop);
            }
            return false;
        };
        let (Some(renderer), Some(mut scene)) = (self.renderer.take(), self.scene.take()) else {
            self.cam.starting = Some(cam);
            return true;
        };
        let centers = start_centers(&self.args, &cam, self.world.as_deref());
        let progress = match self.gfx.streamer.as_mut() {
            Some(streamer) => {
                streamer.update(
                    &renderer,
                    &mut scene,
                    &centers,
                    std::time::Duration::from_millis(30),
                    None,
                );
                streamer.initial_progress()
            }
            None => None,
        };
        let Some((done, total)) = progress else {
            // (the loading screen, if it showed, runs its bar to the end and fades over the
            // first pictures: `frame_ui`)
            self.menus.loading.finish();
            // (how long it took, for the next load of this map's bar)
            if self.menus.loading.elapsed() > 0.0 {
                crate::ui::LoadTimes::record(&self.args.map, self.menus.loading.elapsed());
            }
            let w = self.world.clone().expect("world");
            log::info!(
                "map ready after {:.2} s: {} tiles loaded",
                self.started.elapsed().as_secs_f64(),
                w.loaded_tiles().len()
            );
            self.start_world(w, cam, &renderer, &mut scene);
            self.renderer = Some(renderer);
            self.scene = Some(scene);
            // Started again after the graphics card was lost (`restart_after_device_loss`):
            // said on the screen. The restart with lighter graphics, and on Windows another
            // graphics interface, was silent, and players took the blurred picture and the
            // lower frame rate it left for a slower game, never for a crash of the driver.
            if let Some(n) = omsi_cfg::flags::OMSI_SAFE_GPU.parse::<u32>().filter(|n| *n > 0) {
                log::warn!("this run was started again after the graphics device was lost ({n}): lighter graphics for this run");
                self.service_msg = Some(("The graphics driver reset the card: the game started again with lighter graphics for this drive. Settings → Graphics keeps yours for the next start.".into(), 12.0));
            }
            omsi_lan::game::drive_started(&self.args.lan_config(), &self.args.root, self.player.as_ref().map(|p| &p.vehicle));
            return omsi_lan::game::start_held().is_none();
        };
        let name = self
            .world
            .as_ref()
            .map(|w| {
                if w.global.friendly_name.trim().is_empty() {
                    w.global.name.clone()
                } else {
                    w.global.friendly_name.clone()
                }
            })
            .unwrap_or_default();
        // (the bar eases towards the count: it came in unevenly and jumped to its end)
        let dt = self.menus.loading.tick();
        if self.menus.loading.expected <= 0.0 {
            self.menus.loading.expected = crate::ui::LoadTimes::expected(&self.args.map).unwrap_or(crate::ui::LOADING_FIRST_GUESS);
            log::info!("loading: {:.1} s expected for {}", self.menus.loading.expected, self.args.map);
        }
        self.menus.loading.step(done as f32 / total.max(1) as f32, dt);
        self.menus.loading.title = name.trim().to_string();
        self.loading_frame(renderer, scene, name.trim(), self.menus.loading.shown());
        self.cam.starting = Some(cam);
        if let Some(limit) = self.args.exit_after {
            if self.started.elapsed().as_secs_f32() > limit {
                log::info!("exit after {limit} s while loading: {done} of {total} tiles");
                crate::platform::exit(event_loop);
            }
        }
        false
    }

    /// One loading screen picture (the map, or our bus going to the server): `title`, bar at `shown`.
    fn loading_frame(&mut self, renderer: omsi_render::Renderer, mut scene: Scene, title: &str, shown: f32) {
        let mut reconfigure = false;
        if let (Some(ui), Some(s), Some(win)) = (
            self.ui.as_mut(),
            self.gfx.surface.as_ref(),
            self.window.as_ref(),
        ) {
            scene.overlays.clear();
            let dpi = win.scale_factor() as f32;
            let scale = dpi * crate::ui::size_factor(s.config.height as f32, dpi, self.settings.ui_scale, self.settings.ui_scale_window);
            ui.loading(&renderer, &mut scene, s.config.width as f32, s.config.height as f32, scale, title, shown, self.menus.loading.clock(), self.menus.loading.tip());
            let acquired = s.acquire();
            // a swapchain that no longer fits the window (Vulkan says so after the switch
            // to full screen, without a resize event) is made again, as the game's own
            // frames do: left as it was, every later frame of the loading screen failed
            // the same way and its picture stood still until the map was there (#776)
            reconfigure = matches!(acquired, wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost);
            if let wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) = acquired
            {
                let view = s.view(&renderer.device, &frame);
                // the tiles loaded so far stay out of the picture: the camera looks at nothing
                let blank = Camera {
                    position: DVec3::new(0.0, 0.0, -1.0e6),
                    yaw: 0.0,
                    pitch: -89.0,
                    roll: 0.0,
                    fov_deg: 60.0,
                    near: 0.5,
                    far: 10.0,
                };
                let lighting = omsi_render::Lighting {
                    sky_color: glam::Vec3::new(0.08, 0.10, 0.14),
                    ..Default::default()
                };
                let mut renderer = renderer;
                renderer.render(
                    &mut scene,
                    &view,
                    s.config.width,
                    s.config.height,
                    &blank,
                    &lighting,
                );
                // `shot <file>` of an input script, while the loading screen stands
                if let Some((path, _)) = self.perf.shot.take() {
                    let path = path.to_string_lossy().into_owned();
                    match renderer.render_to_image(&mut scene, s.config.width, s.config.height, &blank, &lighting) {
                        Ok(px) => match image::save_buffer(path.trim(), &px, s.config.width, s.config.height, image::ColorType::Rgba8) {
                            Ok(()) => log::info!("input script: loading screen written to {}", path.trim()),
                            Err(e) => log::warn!("input script: {} could not be written: {e}", path.trim()),
                        },
                        Err(e) => log::warn!("input script: the loading screen could not be rendered: {e}"),
                    }
                }
                win.pre_present_notify();
                s.present(&renderer.device, &renderer.queue, frame);
                self.renderer = Some(renderer);
            } else {
                self.renderer = Some(renderer);
            }
            win.request_redraw();
        } else {
            self.renderer = Some(renderer);
        }
        if reconfigure {
            if let (Some(s), Some(r), Some(win)) = (self.gfx.surface.as_mut(), self.renderer.as_ref(), self.window.as_ref()) {
                let size = crate::platform::surface_size(win);
                s.resize(r, size.width, size.height);
            }
        }
        self.scene = Some(scene);
    }

}

/// Where a streamed map loads around while it starts: the vehicle's spawn point (a bus must
/// stand on ground when it appears, whatever `--cam` says), and the camera unless the view
/// follows the bus (every view but the free camera does, `--cam` or not).
pub(crate) fn start_centers(args: &Args, cam: &Camera, world: Option<&World>) -> Vec<DVec3> {
    let mut out: Vec<DVec3> = spawn_point(args, world).into_iter().collect();
    let view_in_bus = args.bus.is_some() && args.view != "free";
    if out.is_empty() || !view_in_bus {
        out.push(cam.position);
    }
    out
}

/// What the map needs and this installation lacks, said on the screen and written to
/// `~/.openomsi/missing_content.txt` by add-on folder: a map short of an add-on showed
/// holes, bare roads and white objects, and nobody could tell that from a fault of the game.
pub(crate) fn report_missing_content(w: &World, msg: &mut Option<(String, f32)>) {
    let (files, textures) = w.missing_content();
    if files.is_empty() && textures.is_empty() {
        return;
    }
    // the add-on a file comes with: the folder under Sceneryobjects / Splines / Vehicles
    let addon = |f: &str| f.split('/').take(2).collect::<Vec<_>>().join("/");
    let mut by_addon: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (f, what) in &files {
        by_addon.entry(addon(f)).or_default().push(format!("{what}: {f}"));
    }
    let mut text = format!("openOMSI: content this map uses that is not installed\nmap: {}\n\n", w.map_dir.display());
    for (a, list) in &by_addon {
        text.push_str(&format!("{a} ({} files)\n", list.len()));
        for l in list {
            text.push_str(&format!("  {l}\n"));
        }
    }
    if !textures.is_empty() {
        text.push_str(&format!("\ntextures not found ({}):\n", textures.len()));
        for t in &textures {
            text.push_str(&format!("  {t}\n"));
        }
    }
    let Some(dir) = crate::lan::data_dir() else { return };
    let path = dir.join("missing_content.txt");
    let _ = std::fs::write(&path, text);
    let objects = files.iter().filter(|(_, w)| *w != "spline").count();
    let splines = files.len() - objects;
    let addons: Vec<&String> = by_addon.keys().take(4).collect();
    let more = if by_addon.len() > 4 { format!(" and {} more", by_addon.len() - 4) } else { String::new() };
    log::warn!("missing content: {objects} objects, {splines} splines, {} textures (list: {})", textures.len(), path.display());
    if !files.is_empty() {
        *msg = Some((
            format!(
                "This map uses {objects} objects and {splines} splines that are not installed (add-ons: {}{more}). The list is in {}",
                addons.iter().map(|a| a.as_str()).collect::<Vec<_>>().join(", "),
                path.display()
            ),
            15.0,
        ));
    }
}

fn wrap_deg(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

/// A bus whose mirrors are frozen, and the seconds since they were first drawn.
pub(crate) struct FrozenMirrors {
    pub(crate) bus: u64,
    pub(crate) since: f32,
}

#[derive(Default)]
pub(crate) struct CamBlend {
    /// (view, camera numbers) of the last frame: a change of the numbers inside the same
    /// view is a camera switch.
    pub key: Option<(String, (usize, usize))>,
    /// Where the glide has come to (in the bus's frame), while one is under way.
    pub from: Option<omsi_vehicle::Camera>,
    /// The cockpit camera as it was drawn last frame (in the bus's frame): where the next
    /// glide starts from.
    pub shown: Option<omsi_vehicle::Camera>,
    /// Just sat down at the wheel (from on foot): the next cockpit frame glides in from where
    /// the walker's eyes were.
    pub entering: bool,
    /// What the glide's last picture was off from the plain camera by, let go of over a
    /// fraction of a second after the hand-over (so that nothing is left to jump).
    pub carry: Option<CamCarry>,
    /// The zoom the picture has come to while it glides from the camera left's zoom to the
    /// one taken's (each cab camera keeps its own, #2183; it jumped, #2306).
    pub zoom_from: Option<f32>,
    /// The zoom drawn last frame: where the next glide's zoom starts from.
    pub zoom_drawn: Option<f32>,
}

/// A small difference between two pictures of the camera (world position, angles in degrees,
/// field of view) that is eased out instead of being cut.
#[derive(Clone, Copy)]
pub(crate) struct CamCarry {
    pub pos: glam::DVec3,
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    pub fov: f32,
}

impl CamCarry {
    /// `a` minus `b`.
    pub fn between(a: &omsi_render::Camera, b: &omsi_render::Camera) -> Self {
        Self {
            pos: a.position - b.position,
            yaw: wrap_deg(a.yaw - b.yaw),
            pitch: a.pitch - b.pitch,
            roll: wrap_deg(a.roll - b.roll),
            fov: a.fov_deg - b.fov_deg,
        }
    }

    /// Put on a camera.
    pub fn apply(&self, c: &mut omsi_render::Camera) {
        c.position += self.pos;
        c.yaw += self.yaw;
        c.pitch = (c.pitch + self.pitch).clamp(-89.0, 89.0);
        c.roll += self.roll;
        c.fov_deg += self.fov;
    }

    /// Ease out by one frame; false once nothing is left.
    pub fn decay(&mut self, dt: f32) -> bool {
        let k = (-dt.clamp(0.0, 0.1) * 12.0).exp();
        self.pos *= k as f64;
        self.yaw *= k;
        self.pitch *= k;
        self.roll *= k;
        self.fov *= k;
        self.pos.length() > 1e-4 || self.yaw.abs() > 0.01 || self.pitch.abs() > 0.01 || self.roll.abs() > 0.01 || self.fov.abs() > 0.01
    }
}

crate::app::stage_ctx!(
    /// The streaming stage's part of the game.
    StreamingCx { reads: [view, net, sound, weather, world], writes: [gfx, player, traffic, menus] }
);

/// Stream the tiles around the camera and the player's bus (whose ground must stay when
/// the free camera flies off); when the loaded set changed, hand the new data to whatever
/// keeps its own copy (the bus's obstacles, the traffic network, the navigator, the
/// street lamps).
pub(crate) fn streaming(cx: StreamingCx) {
    let mut centers: Vec<DVec3> = cx.view.camera.iter().map(|c| c.position).collect();
    centers.extend(cx.player.player.iter().map(|p| p.vehicle.position));
    // a LAN host simulates the world around every player: the ground and the roads there
    if cx.net.lan.as_ref().map(|l| l.role == omsi_net::Role::Host).unwrap_or(false) {
        centers.extend(cx.net.remotes.remotes.values().map(|r| r.vehicle().position));
    }
    let (Some(streamer), Some(w), Some(r), Some(scene)) = (
        cx.gfx.gfx.streamer.as_mut(),
        cx.world.world.as_ref(),
        cx.gfx.renderer.as_ref(),
        cx.gfx.scene.as_mut(),
    ) else {
        return;
    };
    // textures uploaded as RGBA to spare a frame, compressed on the workers since, and the
    // texture budget
    w.apply_texture_upgrades(
        r,
        scene,
        Some(Instant::now() + std::time::Duration::from_millis(2)),
    );
    w.update_texture_budget(r, scene, &centers, false);
    if centers.is_empty() {
        return;
    }
    let changed = streamer.update(
        r,
        scene,
        &centers,
        std::time::Duration::from_millis(6),
        cx.sound.audio.as_ref(),
    );
    // Uploads can temporarily exceed the texture budget before the next frame's
    // housekeeping pass. Recheck immediately after streaming so far textures are
    // reduced before the renderer allocates more frame resources (only when over it).
    if !changed {
        return;
    }
    w.texture_budget_after_streaming(r, scene, &centers);
    if let Some(p) = cx.player.player.as_mut() {
        // (OMSI's [no_collision]: no solid object stops the bus)
        p.vehicle.collision = cx.common.settings.collision_objects.then(|| w.data.peek().lists.collision.clone());
        p.vehicle.wheel_walls = cx.common.settings.collision_objects;
    }
    match cx.traffic.traffic.as_mut() {
        Some(t) => {
            t.add_tiles(&w.traffic_inbox);
        }
        None => {
            // no traffic system: the navigator keeps the roads for its map
            let lanes = omsi_sim::world_data::TrafficArrivals::gather(w.traffic_inbox.take()).lanes;
            if let Some(n) = cx.menus.menus.navigator.as_mut() {
                n.add_lanes(lanes);
            }
        }
    }
    if let Some(on) = *cx.weather.lamps_on {
        w.set_lamps(r, scene, on);
    }
}
