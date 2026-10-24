//! `omsi` - load an OMSI 2 map and render it.
//!
//! ```text
//! omsi --root "/path/to/OMSI 2" --map maps/Grundorf/global.cfg            # window
//! omsi --root ... --map ... --offscreen out.png --size 1600x900            # PNG
//! ```
//!
//! The game is this library; `main.rs` calls [`run`], and on Android the NativeActivity
//! calls `android_main` (see `android.rs`), which runs the launcher and the game in one
//! window of one process. On an iPhone or an iPad the Swift app calls `openomsi_ios_main`
//! (see `ios.rs`); its launcher, and the Mac's Swift launcher, read and change what the
//! launcher knows through `apple_bridge`.

mod admin;
mod admin_panel;
mod auto_announce;
mod player_list;
mod voice_chat;
use omsi_scene::check;
use omsi_platform::discord;
#[cfg(steam)]
mod steam;
pub(crate) use omsi_scene::head_idle;
use omsi_input::{head_filter, headtrack, tobii};
#[cfg(windows)]
use omsi_xr::openxr;
#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
mod android_launcher;
#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "ios")]
mod apple;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple_bridge;
mod platform;
mod touch;
mod units;
mod placing;
use omsi_mt as mt;
use omsi_platform::updater;
mod update_watch;
use omsi_platform::presence;
use omsi_soundscape::ambience;
mod soundscape;
use omsi_scene::wheel_surface;
use omsi_scene::camera_arm;
use omsi_vehicle::cam_approach;
use omsi_career::career;
use omsi_input::describe;
pub(crate) use omsi_scene::editor;
mod duty_book;
mod game_lists;
mod game_panels;
mod game_controller_menu;
mod rail_drive;
use omsi_scene::driver;
use omsi_scene::export;
pub(crate) use omsi_scene::hud;
mod humans;
use omsi_career::journey;
use omsi_input::keys;
// (LAN play: the session is omsi-lan's `game`, see `lan_play`; the shared world's mirror
// stays here: it drives the app's `Traffic`, `Humans` and their view directly)
mod lan_play;
use lan_play as lan;
mod lan_world;
use omsi_scene::lights;
mod launcher;
mod menu;
mod mirror_hud;
use omsi_navigator as navigator;
mod vr_navigator;
use omsi_scene::money;
mod milezero;
mod radio;

use omsi_scene::puddles;
use omsi_platform::quit;
use omsi_weather::condensation;
use omsi_scene::{rain, window_wipers};
mod scene;
mod season_phase;
mod schedule;
/// The cab's paper timetable (`omsi_scene::schedule_paper`) in the game's language.
mod schedule_paper {
    pub(crate) fn update_vehicle(
        vehicle: &mut omsi_sim::VehicleInstance,
        duty: &omsi_sim::timetable_run::PlayerDuty,
        fonts: &mut omsi_sim::texttex::FontLibrary,
    ) -> anyhow::Result<()> {
        omsi_scene::schedule_paper::update_vehicle(vehicle, duty, fonts, || crate::settings::Settings::load().language)
    }
}
mod real_time;
mod settings;
mod shell;
mod photo;
mod depot_bus;
mod devtools;
mod telemetry;
use omsi_scene::threads;
use omsi_scene::tiles;
mod traffic;
mod view_sync;
mod ui;

// the game itself, split by what each part does
mod app;
mod applog;
mod app_events;
mod bus_service;
mod camera_util;
// the game controllers, their mappings and force feedback (`omsi-input`), at their old paths
use omsi_input::{controllers, gamepad_profile, hpattern};
mod cli;
use omsi_input::diagnostics;
mod perf_overlay;
mod perf_report;
pub(crate) use omsi_cfg::profile_keys;
mod frame_stages;
mod support_bundle;
mod duty_start;
mod input_script;
mod app_impl;
use omsi_guard::launcher_link;
use omsi_lan::mods as lan_mods;
mod memory;
mod offscreen;
pub(crate) use omsi_sim::ground_gap;
mod on_foot;
mod trolley_hands;
mod trolley_mouse;
pub(crate) mod trolley_net;
mod trolley_sound;
mod optimiser;
use omsi_scene::route_arrows;
mod server;
mod player;
mod plugin_io;
// the plugins' panels (`omsi-plugin-ui`) at their old path, and the app's side of their input
use omsi_plugin_ui as plugin_ui;
mod plugin_panels;
mod process_permissions;
mod plugins;
mod services;
mod sim_thread;
mod situation;
mod spawn;
use omsi_input::stock_keys;
mod startup;
mod traffic_link;
pub(crate) use omsi_content::tutorial;
mod weather_setup;
mod weather_tomorrow;
use omsi_weather::{weather_cycle, weather_model};
mod world_load;

// the interface's translations (locales/app.yml; the English text is the key)
rust_i18n::i18n!("locales");
// (the tables are read when this crate compiles: this makes cargo compile it again when they
// change - the macro alone left the old texts in the program)
use omsi_launcher_lib::i18n::TABLES as _LOCALES;

/// Show the interface in `code` (the settings' ENG / DEU / FRA / RUS).
pub(crate) fn ui_language(code: &str) {
    omsi_ui::i18n::set_lookup(|lang, text| _rust_i18n_try_translate(lang, text).map(|t| t.into_owned()));
    static TEMPLATES: std::sync::Once = std::sync::Once::new();
    TEMPLATES.call_once(|| omsi_ui::i18n::set_templates(locale_keys(_LOCALES)));
    let iso = omsi_launcher_lib::language_iso(code);
    omsi_ui::i18n::set_language(iso);
    omsi_sim::vehicle_api::set_locale(iso);
}

const MISSING_ORIGINAL: &str = "The original OMSI 2 was not found.\n\n\
    openOMSI needs a complete installation of the original game (any version). \
    Choose its folder in the launcher (Setup), or start once with \
    --root \"/path/to/OMSI 2\", the folder with Omsi.exe, maps and Vehicles in it.\n\n\
    Missing in {folder}: {missing}";

fn locale_keys(yml: &str) -> impl Iterator<Item = String> + '_ {
    yml.lines().filter_map(|l| l.strip_prefix('"')?.strip_suffix("\":")).filter(|k| k.contains('{')).filter_map(|k| {
        let mut out = String::new();
        let mut chars = k.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            out.push(match chars.next()? {
                'n' => '\n',
                't' => '\t',
                x @ ('"' | '\\') => x,
                _ => return None,
            });
        }
        Some(out)
    })
}

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use glam::{DVec3, Vec3};
use omsi_render::{Camera, Renderer, Scene, SurfaceState};
use scene::World;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};
use app::*;
use camera_util::*;
use cli::*;
use diagnostics::*;
use duty_start::*;
use input_script::*;
use launcher_link::*;
use memory::*;
use offscreen::*;
use player::*;
use services::*;
use situation::*;
use spawn::*;
use startup::*;
use traffic_link::*;
use weather_setup::*;
use world_load::*;

/// The game (and its launcher) from the command line: what `main` does.
pub fn run() -> Result<()> {
    view_sync::install_sim_decoder();
    startup::install_scene_settings();
    omsi_cfg::migrate_legacy_data_dir();
    omsi_launcher_lib::adopt_own_inputs();
    #[cfg(target_os = "macos")]
    restart_with_allocator_settings();
    #[cfg(windows)]
    attach_parent_console();
    // `--check <file or folder>` (`--check-vehicle`, its old name): the loaders' findings
    // as JSON, no window, no log
    if matches!(std::env::args().nth(1).as_deref(), Some("--check" | "--check-vehicle")) {
        let rest: Vec<String> = std::env::args().skip(2).collect();
        return check::run(&rest, crate::startup::find_root);
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    // a panic goes into the log (which the launcher keeps per session) with where it
    // happened and a backtrace, not only to a terminal that may not be there
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if omsi_render::catching() {
            log::warn!("caught by the renderer: {info}");
            return;
        }
        log::error!(
            "the game stopped on an error (build {BUILD}): {info}\n{}",
            std::backtrace::Backtrace::force_capture()
        );
        default_hook(info);
    }));
    log::info!(
        "openOMSI {VERSION}, build {BUILD}{}",
        if std::env::var_os("MallocLargeCache").is_some() {
            " (large allocations returned at once)"
        } else {
            ""
        }
    );
    // the desktop launcher's helper: no window, requests on stdin (see `launcher::service`)
    if std::env::args().nth(1).as_deref() == Some("--launcher-service") {
        return launcher::run_service();
    }
    let args = Args::parse();
    // (before the content and the graphics device are looked for: it must work without them)
    if let Some(out) = &args.export_diagnostics {
        let value = support_bundle::snapshot(None, &settings::Settings::load(), None, None, "cli");
        support_bundle::export(out, &value)?;
        println!("Support package written to {} - look inside before you attach it to an issue", out.display());
        return Ok(());
    }
    // Started by a double click or with no arguments at all: that is the launcher's job.
    // The launcher itself runs the game with a full command line (--no-menu, --map, ...).
    let bare = std::env::args().len() == 1;
    let Some((args, server_cfg)) = prepare(args, bare)? else { return Ok(()) };
    if bare && !args.menu {
        // the launcher, another program (it starts the game again with a full command
        // line): OMSI_LAUNCHER=<program>, else the desktop launcher beside the game
        if omsi_cfg::flags::OMSI_LAUNCHER.is_set() && open_launcher()? {
            return Ok(());
        }
        return open_app_launcher(fatal_message);
    }
    let Some(app) = make_app(args, server_cfg)? else { return Ok(()) };
    // a game session's start goes into the start guard's journal (a version whose sessions
    // keep failing before their first frames is healed at the launcher's next start)
    if app.args.server.is_none() {
        omsi_guard::start(omsi_guard::journal::Kind::Game);
    }
    let event_loop = game_event_loop()?;
    // SIGTERM (the launcher's Stop) and Ctrl+C end the session the way Escape does
    let proxy = event_loop.create_proxy();
    quit::install(move |_| {
        let _ = proxy.send_event(());
    });
    let mut app = app;
    let r = event_loop.run_app(&mut app);
    // the host's mods of this session go with it
    lan_mods::clean_up();
    r?;
    Ok(())
}

/// The game's event loop. With OMSI_HIDDEN_WINDOW the process stays out of the Dock and the
/// menu bar on macOS, as its window stays out of sight.
fn game_event_loop() -> Result<EventLoop<()>> {
    #[cfg(target_os = "macos")]
    if omsi_cfg::flags::OMSI_HIDDEN_WINDOW.is_set() {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        return Ok(EventLoop::builder().with_activation_policy(ActivationPolicy::Accessory).build()?);
    }
    Ok(EventLoop::new()?)
}


/// Everything before a window: the language, the session's random seed, the original
/// installation and the content roots (mods, archives). None when the program has
/// nothing more to do (a fatal error was shown).
pub(crate) fn prepare(mut args: Args, bare: bool) -> Result<Option<(Args, Option<server::ServerCfg>)>> {
    optimiser::recover_cut_short();
    ui_language(&settings::Settings::load().language);
    units::Units::from_setting(&settings::Settings::load().units).set();
    // (a server has no interface to translate)
    mt::enable(settings::Settings::load().machine_translation && args.server.is_none());
    // the dedicated server: server.cfg decides the world, the rest is a host without a window
    let server_cfg = match args.server.clone() {
        // (a start script asking for the maps: server.cfg written when missing, nothing more
        // of a server's start - no METAR report fetched)
        Some(p) if args.list_maps => {
            if let Err(e) = server::ServerCfg::load(&p, game_lists::TRAFFIC_MAX) {
                eprintln!("server: {e:#}");
                std::process::exit(2);
            }
            None
        }
        Some(p) => match server::prepare(&mut args, &p) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("server: {e:#}");
                std::process::exit(2);
            }
        },
        None => None,
    };
    // the scripts' `random` differs from session to session (starting air pressure, part
    // lifetimes ...); OMSI_SEED=n repeats a session's numbers
    let seed = omsi_cfg::flags::OMSI_SEED
        .var()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or_else(|| {
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(1);
            (t ^ ((std::process::id() as u64) << 32)) % 1_000_000_000
        });
    omsi_script::set_session_seed(seed);
    log::info!("script random seed {seed} (OMSI_SEED={seed} repeats it)");
    let launcher_mode = bare && !args.menu;
    if !is_omsi_root(&args.root) {
        // (a start script checks the folder it was given: never another one found instead)
        match find_root().filter(|_| !args.list_maps) {
            Some(p) => {
                log::info!("OMSI 2 found at {}", p.display());
                args.root = p;
            }
            None if launcher_mode => {
                // the launcher opens without the game: its Setup page is where the folder is
                // chosen (only a session needs the original installation)
                log::warn!("the original OMSI 2 was not found; the launcher asks for it");
            }
            None => {
                // openOMSI plays on the original game's content: without a complete
                // installation of it there is nothing to play on, and every player must have
                // the same base whatever copy of OMSI 2 they own
                let missing = omsi_cfg::missing_original_essentials(&args.root);
                let text = omsi_ui::tr(MISSING_ORIGINAL).replace("{folder}", &args.root.display().to_string()).replace("{missing}", &missing.join(", "));
                if server_cfg.is_some() || args.list_maps {
                    eprintln!("{text}");
                } else {
                    fatal_dialog(&omsi_ui::tr("openOMSI cannot start"), &text);
                }
                if cfg!(any(target_os = "android", target_os = "ios")) {
                    // (a phone's app is not ended from inside: back to the launcher)
                    return Ok(None);
                }
                std::process::exit(1);
            }
        }
    }
    if let Some(memo) = root_memo().filter(|_| is_omsi_root(&args.root) && server_cfg.is_none() && args.pack_map.is_none()) {
        let _ = std::fs::write(memo, args.root.to_string_lossy().as_bytes());
    }
    // content roots: the game's own folder (mods) first, then the original installation
    if let Some(c) = content_dir() {
        match omsi_cfg::ensure_content_layout(&c) {
            Ok(()) => {
                omsi_cfg::add_content_root(c.clone());
                log::info!("content folder (mods): {}", c.display());
            }
            Err(e) => log::warn!("content folder {}: {e}", c.display()),
        }
    }
    // archives read in place: searched after the content folder, before the installation
    // (`--content-zip`, `OMSI_CONTENT_ZIP`, and every .zip in the content folder's `Archives`)
    for z in &args.content_zip {
        if let Err(e) = omsi_cfg::add_content_zip(z) {
            log::warn!("content zip {}: {e}", z.display());
        }
    }
    omsi_cfg::vfs::mount_env_zips();
    if let Some(c) = content_dir() {
        omsi_cfg::vfs::mount_dir_zips(&c.join("Archives"));
    }
    omsi_cfg::add_content_root(args.root.clone());
    if args.list_maps {
        for m in server::installed_maps() {
            println!("{m}");
        }
        std::process::exit(0);
    }
    // a server's map must be installed: said now, with the maps there are, rather than as
    // a file the loading could not read
    if server_cfg.is_some() {
        if let Err(e) = server::check_map(&args) {
            eprintln!("server: {e}");
            std::process::exit(2);
        }
    }
    Ok(Some((args, server_cfg)))
}

/// The game for `args`, ready for its window; None when there is no window to open (a
/// picture or a model was written instead).
pub(crate) fn make_app(mut args: Args, server_cfg: Option<server::ServerCfg>) -> Result<Option<App>> {
    if let (Some(map), Some(to)) = (args.pack_map.clone(), args.pack_to.clone()) {
        omsi_lan::map_pack::run(&map, &to)?;
        return Ok(None);
    }
    // the window's simulation beside the picture or inline, for the whole session (an
    // offscreen run - a dedicated server, a picture - steps inline and never asks)
    if args.offscreen.is_none() {
        let sim_thread = omsi_sim::sim_thread::set_enabled(settings::Settings::load().sim_thread);
        log::info!("sim thread: {}", if sim_thread { "the AI traffic, the people and the scenery scripts step beside the picture" } else { "off: every step on the main thread" });
    }
    let _lan_status = lan::StatusFileGuard;
    // OMSI's tutorials: the lesson's own situation
    if let Some(n) = args.tutorial.filter(|n| (1..=4).contains(n)) {
        args.situation = Some(tutorial::SITUATIONS[n - 1].to_string());
    }
    apply_situation(&mut args)?;
    // `openomsi`: the official server, wherever its tunnel is today (see omsi_net::official)
    if let Some(t) = args.lan_join.clone().filter(|t| omsi_net::official::is_alias(t)) {
        match omsi_net::official::resolve_target(&t) {
            Ok(url) => {
                log::info!("LAN: the official server is at {url}");
                args.lan_join = Some(url);
            }
            Err(e) => log::warn!("LAN: {e}"),
        }
    }
    // a duty starts at its trip, as in OMSI (not at the map's entry point); a joining
    // player's once the host's world is known (below): it was never placed at all, and
    // "Automatic" put it at the map's first entry point, the depot
    // real-time sync: the game starts at this device's date and time (a joining player's
    // clock is the host's, a server's is its server.cfg's); a duty does not move it
    if settings::Settings::load().time_sync && args.lan_join.is_none() && args.server.is_none() && args.offscreen.is_none() {
        real_time::start_at_now(&mut args);
    }
    // a season chosen with its phase moves the date into it (the sun, the weather and the
    // timetable go by the date); a joining player takes the host's date below
    season_phase::apply_season_date(&mut args);
    if args.export_glb.is_none() && args.lan_join.is_none() {
        place_on_duty(&mut args);
    }
    let settings = settings::Settings::load();
    applog::log_system(&settings);
    if args.drive_keys.eq_ignore_ascii_case("simple")
        && !settings.drive_keys.eq_ignore_ascii_case("simple")
    {
        args.drive_keys = settings.drive_keys.clone();
    }
    args.session_settings = startup::SessionSettings::for_session(&settings, &args);
    // the LAN session (offscreen too, so that one game's view of another can be rendered);
    // a joining player's world is the host's (taken over again when the window loads it)
    let mut lan = if args.export_glb.is_none() {
        lan::start(&args.lan_config(), &lan::AppLanHost { args: &args, world: None })
    } else {
        None
    };
    // a dedicated server without its session has nobody to serve: why, and the end (it went
    // on running alone, unreachable, with a warning among the log's lines)
    if let (Some(cfg), None) = (server_cfg.as_ref(), lan.as_ref()) {
        eprintln!("server: cannot open the session on UDP port {} (the line above says why): another server or program uses it, or the system refuses it. Stop the other one, or choose another `port` in {}", cfg.port, args.server.as_deref().map(|p| p.display().to_string()).unwrap_or_else(|| "server.cfg".into()));
        std::process::exit(3);
    }
    // turned away at the door (banned, full, another version): no game to play there
    if lan.as_ref().and_then(lan::turned_away).is_some() {
        log::info!("game ends");
        return Ok(None);
    }
    // the host's mods: served by the host, fetched by a joining player before its world is
    // made (see `lan_mods`)
    lan_mods::remove_stale();
    if let Some(l) = lan.as_mut() {
        let mut c = args.lan_config();
        lan::share_mods(&mut c, l);
        lan::take_host_map(&mut c, l);
        args.take_lan_config(&c);
    }
    if let (Some(cfg), Some(l)) = (server_cfg.as_ref(), lan.as_ref()) {
        lan::open_public_gateway(l, server::info_of(cfg), cfg.web_port, cfg.tunnel);
        lan::publish_vehicles(args.root.clone(), cfg.vehicles.clone());
        server::start_public_list(cfg);
        if cfg.tunnel {
            // the address to give the players, as soon as cloudflared says it
            std::thread::spawn(|| {
                for _ in 0..300 {
                    if let Some(u) = lan::tunnel_url() {
                        println!("\n  Server address for the players: {u}\n  (Multiplayer -> Servers -> Add)\n");
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
                println!("  No tunnel address (is cloudflared installed?): players join at this machine's address and the UDP port");
            });
        }
    }
    // a host's clock runs at its time speed (a server's: its server.cfg)
    if let (Some(l), None) = (lan.as_mut(), server_cfg.as_ref()) {
        if l.role == omsi_net::Role::Host {
            l.clock_speed = if settings.time_sync { 1.0 } else { settings.time_speed.clamp(1.0, 30.0) };
        }
    }
    // a server's voice chat: what its server.cfg allows (a game that hosts has its own settings)
    if let (Some(l), Some(c)) = (lan.as_mut(), server_cfg.as_ref()) {
        l.set_voice_policy(c.voice);
    }
    let mut lan_game = lan::LanGame::default();
    if args.export_glb.is_none() && args.lan_join.is_some() {
        if let Some(t) = lan.as_ref().and_then(lan::host_time_now) {
            args.time = t;
        }
        place_on_duty(&mut args);
    }
    if let Some(out) = args.export_glb.clone() {
        return run_export(&args, &out).map(|_| None);
    }
    if let Some(out) = args.offscreen.clone() {
        if let Some(l) = lan.as_mut() {
            let mut c = args.lan_config();
            lan::adopt_host_world(&mut c, &crate::app_impl::lan_tick::AppLanHost { args: &args, world: None }, l, &mut lan_game);
            args.take_lan_config(&c);
        }
        let r = run_offscreen(&args, &out, lan, lan_game);
        lan_mods::clean_up();
        return r.map(|_| None);
    }
    let mut app = assemble_app(args, settings);
    app.net.lan = lan;
    app.net.remotes = lan_game;
    // mouse steering as the player left it (the wheel eases to the cursor for a second)
    if app.settings.mouse_steering {
        app.input.mouse_drive = true;
        app.input.mouse_steer = (0.0, 1.0);
        app.input.center_cursor = true;
    }
    // (the LAN status file stays while the game runs; `exiting` removes it)
    std::mem::forget(_lan_status);
    Ok(Some(app))
}

/// The game's App for `args` and `settings`, every part of it as it starts (the window comes
/// later).
fn assemble_app(args: Args, settings: settings::Settings) -> App {
    let view = args.view.clone();
    let args_root_for_keys = args.root.clone();
    let clock_note = args.clock_moved.clone();
    // (as the last session left it, #1164)
    let info_bar = settings.info_bar;
    let is_server = args.server.is_some();
    let sky = crate::weather_setup::SkyState { look: args.session_settings, ..Default::default() };
    // What loads, starts or reads the clock, made one after the other in a fixed order before
    // the App and its groups are put together (their logs and threads come in this order).
    // Steam must be initialized before the graphics instance and window so its overlay can
    // hook the rendering process before the first surface is created.
    #[cfg(steam)]
    let steam = if is_server { None } else { crate::steam::Steam::start() };
    let instance = graphics_instance();
    let vr_nav_profiles = crate::vr_navigator::Profiles::load(crate::vr_navigator::profiles_path().as_deref());
    let ui = ui::Ui::new();
    let rain = rain::Rain::new();
    let cabin_air = crate::condensation::CabinAir::new();
    let mut spray = puddles::Spray::new();
    spray.graphics = args.session_settings.graphics();
    let radio = radio::Radio::load(&args_root_for_keys);
    let started = Instant::now();
    let last = Instant::now();
    let input_script = parse_input_script();
    let game_keys = omsi_content::KeyboardCfg::load(&crate::startup::keyboard_cfg(&args_root_for_keys)).unwrap_or_default().with_game_defaults().with_vr_defaults().game;
    let own_keys = crate::startup::own_keys(&args_root_for_keys);
    let own_shift = crate::startup::own_bindings(&args_root_for_keys, omsi_content::input::KEY_SHIFT);
    let fps_t = Instant::now();
    let update_watch = crate::update_watch::UpdateWatch::new();
    // (a server counts its players by their own games, not itself)
    let presence = if is_server || cfg!(test) { None } else { crate::presence::Presence::start() };
    let touch = touch::Touch::new();
    App {
        args,
        gfx: GfxState {
            instance,
            surface: None,
            streamer: None,
            spanned: false,
            mirror_budget: 1.0,
            mirror_dt: 0.0,
            mirrors_seen: 2,
            mirror_turn: 0,
            mirror_panel_cams: Vec::new(),
            frozen_mirrors: None,
            mirror_hud: Default::default(),
            window_hidden: false,
            route_arrows: Default::default(),
            hidden_frames: 0,
            stand_in: None,
            sim_view: Default::default(),
            starting: None,
        },
        window: None,
        renderer: None,
        xr: VrState {
            #[cfg(windows)]
            vr: None,
            vr_nav_profiles,
            vr_nav_edit: None,
            vr_cursor_physical: None,
            vr_cursor_warp_pending: None,
            vr_zoom_active: false,
        },
        scene: None,
        camera: None,
        player: None,
        session: SessionState {
            placed: Vec::new(),
            scenery: Default::default(),
            traffic: None,
            schedule: None,
            timetable_book: None,
            humans: None,
            duty: None,
            duty_places: false,
            rain,
            cabin_air,
            spray,
            lamps_on: None,
            populate_t: 0.0,
            humans_populate_t: 0.0,
            traffic_heavy_frame: false,
            first_populate: true,
            envir: None,
            weather: None,
            clock_jump: 0.0,
            seat_bus: String::new(),
            on_foot: None,
            safe_pose: None,
            safe_age: 0.0,
            pending_time: None,
            world_day: None,
            autosave_t: 0.0,
            pumping: None,
            career: Default::default(),
            journey: None,
            wetness: 0.0,
            cloud_drift: [0.0; 4],
            weather_blend: None,
            weather_cycle: None,
            metar_rx: None,
            metar_once: false,
            metar_next: 0.0,
            tomorrow: Default::default(),
            sky,
        },
        menus: MenuState {
            list_dirty: false,
            page_titles: Default::default(),
            chooser: None,
            editor: None,
            vehicle_list: Vec::new(),
            dropdown: None,
            vehicle_meta: std::collections::HashMap::new(),
            hud: None,
            navigator: None,
            menu: None,
            hover_key: None,
            hover: None,
            hover_part: None,
            hover_hand: false,
            game_menu: None,
            menu_top: None,
            menu_scroll_drag: false,
            dd_scroll_drag: None,
            pane_scroll_drag: None,
            pane_scroll: None,
            teleport_pick: false,
            game_panels: Default::default(),
            tutorial: None,
            wheel_acc: 0.0,
            editor_drag: false,
            editor_sync_t: 0.0,
            remote_added: Default::default(),
            placing: None,
            admin_list: None,
            list_kind: None,
            key_capture: None,
            ctrl_add_action: false,
            menu_prev_pause: false,
            info_bar,
            timetable: false,
            hud_resize: None,
            notices: Vec::new(),
            note_watch: Default::default(),
            loading: Default::default(),
            menu_edit: None,
            menu_edit_icao: false,
            menu_edit_number: false,
            menu_edit_ident: false,
            menu_edit_search: false,
            menu_search: String::new(),
            swap_pending: false,
            pending_placement: None,
            menu_drag: None,
            menu_kbd: true,
        },
        world: None,
        cam: ViewState {
            starting: None,
            speed: 30.0,
            f1_reset: None,
            view_yaw: 0.0,
            view_fov: None,
            head_idle_hold: Default::default(),
            ego: false,
            in_cab: false,
            look: (0.0, 0.0),
            look_smooth: (0.0, 0.0),
            view_looks: Default::default(),
            look_view: String::new(),
            cam_blend: Default::default(),
            view_zoom: Default::default(),
            cam_zooms: Default::default(),
            orbit: ORBIT_DEFAULT,
        },
        ui,
        perf: PerfState::new(input_script, fps_t),
        sound: SoundState {
            radio,
            milezero: Default::default(),
            audio: None,
            ambience: None,
            soundscape: None,
            voice: None,
            voice_mutes: crate::voice_chat::load_mutes(),
            foot_in: None,
            voices_waiting: Vec::new(),
        },
        clock: omsi_sim::SimClock::default(),
        started,
        view,
        input: InputState {
            cursor: (0.0, 0.0),
            window_focused: false,
            input_away: false,
            keys: Default::default(),
            door_key_triggers: Default::default(),
            mouse_look: false,
            buttons_held: (false, false),
            mmb_held: false,
            both_drag: None,
            clock_hold: 0.0,
            pad_look: [false; 4],
            pad_repeat: Vec::new(),
            pad_voice_radio: false,
            pad_voice_nearby: false,
            arrow_glance: false,
            headtrack: None,
            head_filter: Default::default(),
            headtrack_failed: None,
            headtrack_scale_last: None,
            headtrack_scale_bias: [0.0; 6],
            headtrack_invert_last: None,
            controllers: None,
            mouse_drive: false,
            mouse_steer: (0.0, 0.0),
            mouse_grab: Default::default(),
            steer_cursor: None,
            center_cursor: false,
            cruise: None,
            look_lock: None,
            cursor_hidden: None,
            last_ctl_steer: None,
            mouse_pedals: (0.0, 0.0),
            mouse_kmh: 0.0,
            wheel_zoomed_at: None,
            camera_keys_off: false,
            pad_kmh: 0.0,
            pad_steer_target: 0.0,
            game_keys,
            own_keys,
            own_shift,
            dragging: false,
            html_pressed: None,
            html_object_pressed: None,
            pressed_scenery_object: None,
            drag_delta: (0.0, 0.0),
            cursor_kind: 0,
            touch,
        },
        last,
        paused: false,
        integrations: Integrations {
            plugin_keys: Vec::new(),
            plugin_events: Vec::new(),
            plugin_events_ex: Vec::new(),
            plugin_ticket_asks: Vec::new(),
            plugin_pointer: Default::default(),
            plugin_voices: Vec::new(),
            plugin_seen: Default::default(),
            plugin_command: false,
            plugin_panels: Default::default(),
            discord: None,
            discord_t: 0.0,
            #[cfg(steam)]
            steam,
            update_watch,
            presence,
            plugins: None,
        },
        net: NetState::default(),
        service_msg: clock_note.map(|m| (m, 10.0)),
        settings,
        exiting: false,
        shell: crate::shell::Shell::new(),
        photo: None,
        photo_settings: None,
        devtools: None,
        fixed_tick: omsi_sim::tick::SimTicks::default(),
        pipeline: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_starts_around_the_bus() {
        let cam = Camera {
            position: DVec3::new(5000.0, 6000.0, 50.0),
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            fov_deg: 60.0,
            near: 0.5,
            far: 100.0,
        };
        let args = |extra: &[&str]| Args::parse_from(["omsi"].iter().chain(extra.iter()).copied());
        // the camera alone
        assert_eq!(start_centers(&args(&[]), &cam, None), vec![cam.position]);
        // a bus viewed from its seat: only where it stands, even with a camera given (the
        // view follows the bus)
        let seat = vec![DVec3::new(10.0, 20.0, 0.0)];
        assert_eq!(
            start_centers(
                &args(&["--bus", "Vehicles/x.bus", "--spawn", "10,20,90"]),
                &cam,
                None
            ),
            seat
        );
        assert_eq!(
            start_centers(
                &args(&[
                    "--bus",
                    "Vehicles/x.bus",
                    "--spawn",
                    "10,20,90",
                    "--cam",
                    "5000,6000,50,0,0"
                ]),
                &cam,
                None
            ),
            seat
        );
        // ... and with the free camera: both places
        let both = start_centers(
            &args(&[
                "--bus",
                "Vehicles/x.bus",
                "--spawn",
                "10,20,90",
                "--cam",
                "5000,6000,50,0,0",
                "--view",
                "free",
            ]),
            &cam,
            None,
        );
        assert_eq!(both, vec![DVec3::new(10.0, 20.0, 0.0), cam.position]);
    }
}

#[cfg(test)]
pub(crate) fn with_test_language<T>(code: &str, f: impl FnOnce() -> T) -> T {
    use std::sync::{Mutex, OnceLock};
    static LANGUAGE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = LANGUAGE_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner());
    ui_language(code);
    let result = f();
    ui_language("ENG");
    result
}

#[cfg(test)]
mod locale_tests {
    use omsi_ui::i18n::{templated, templates, Piece};

    #[test]
    fn the_template_keys_of_the_tables_are_read_as_the_code_writes_them() {
        let raw = super::_LOCALES.lines().filter(|l| l.starts_with('"') && l.contains('{')).count();
        assert_eq!(super::locale_keys(super::_LOCALES).count(), raw);
        let used: Vec<String> = templates(super::locale_keys(super::_LOCALES)).into_iter().map(|(k, _)| k).collect();
        for k in ["Graphics profile \"{name}\" not found", "Mirror panel added ({} in all)", "Head tracking with opentrack (UDP port {})"] {
            assert!(used.iter().any(|x| x == k), "{k}");
        }
    }

    #[test]
    fn multiplayer_messages_are_translated_in_traditional_chinese() {
        for key in [
            "Host's mods could not be fetched: {e}",
            "Host's mods: everything the host uses is installed here",
            "LAN: other players join with the code {}  (or by an address: {})",
            "LAN: the host's weather: {}",
            "Sent to the server: {action}",
            "The server does not offer {name}",
            "Online: {n} other players",
            "Online: nobody else yet",
            "Online: 1 other player",
            "Online: connecting ...",
            "Online: not connected: {reason} (chat /reconnect)",
            "Online: in {host}'s game, {others}",
            "Online: {warning}",
            "nobody else yet",
            "1 other player",
            "{n} other players",
        ] {
            let translated = super::_rust_i18n_try_translate("zh-tw", key).map(|text| text.into_owned());
            assert!(translated.as_ref().is_some_and(|text| !text.trim().is_empty() && text != key), "zh-tw: {key}");
        }
    }

    #[test]
    fn a_translated_template_is_left_as_it_is_when_translated_again() {
        let t = templates(super::locale_keys(super::_LOCALES));
        for lang in super::_rust_i18n_available_locales() {
            let tr = |key: &str| super::_rust_i18n_try_translate(&lang, key).map(|t| t.into_owned());
            for (key, pieces) in &t {
                // the text the code would draw: a value per placeholder, the same one for a name used twice
                let mut names: Vec<&str> = Vec::new();
                let sample: String = pieces
                    .iter()
                    .map(|p| match p {
                        Piece::Text(s) => s.clone(),
                        Piece::Hole(n) => {
                            if n.is_empty() || !names.contains(&n.as_str()) {
                                names.push(n);
                            }
                            format!("V{}", if n.is_empty() { names.len() } else { names.iter().position(|x| x == n).unwrap() + 1 })
                        }
                    })
                    .collect();
                let Some(once) = templated(&t, &sample, tr) else { continue };
                // no value is lost (a named one may be used twice: "{e} electrics ... with {e}")
                for v in sample.split('V').skip(1).filter_map(|s| s.split(|c: char| !c.is_ascii_digit()).next()) {
                    assert!(once.contains(&format!("V{v}")), "{lang}: {key}: {once}");
                }
                assert!(templated(&t, &once, tr).is_none_or(|twice| twice == once), "{lang}: {key}: {once}");
            }
        }
    }
}
