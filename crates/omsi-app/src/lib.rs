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
use omsi_platform::discord;
use omsi_scene::check;
#[cfg(steam)]
mod steam;
use omsi_input::{head_filter, headtrack, tobii};
pub(crate) use omsi_scene::head_idle;
#[cfg(windows)]
use omsi_xr::openxr;
#[cfg(target_os = "android")]
#[rustfmt::skip]
mod android;
#[cfg(target_os = "android")]
#[rustfmt::skip]
mod android_launcher;
#[cfg(target_os = "ios")]
mod apple;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple_bridge;
#[cfg(target_os = "ios")]
mod ios;
mod placing;
mod platform;
#[rustfmt::skip]
mod touch;
#[rustfmt::skip]
mod units;
use omsi_mt as mt;
#[rustfmt::skip]
use omsi_platform::updater;
#[rustfmt::skip]
mod update_watch;
use omsi_platform::presence;
use omsi_soundscape::ambience;
mod soundscape;
use omsi_career::career;
use omsi_input::describe;
use omsi_scene::camera_arm;
pub(crate) use omsi_scene::editor;
use omsi_scene::wheel_surface;
#[rustfmt::skip]
use omsi_vehicle::cam_approach;
mod duty_book;
#[rustfmt::skip]
mod first_graphics;
mod game_controller_menu;
mod game_lists;
#[rustfmt::skip]
mod game_panels;
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

use omsi_platform::quit;
use omsi_scene::puddles;
use omsi_scene::{rain, window_wipers};
#[rustfmt::skip]
use omsi_weather::condensation;
#[rustfmt::skip]
mod scene;
mod schedule;
mod season_phase;
/// The cab's paper timetable (`omsi_scene::schedule_paper`) in the game's language.
#[rustfmt::skip]
mod schedule_paper {
    pub(crate) fn update_vehicle(
        vehicle: &mut omsi_sim::VehicleInstance,
        duty: &omsi_sim::timetable_run::PlayerDuty,
        fonts: &mut omsi_sim::texttex::FontLibrary,
    ) -> anyhow::Result<()> {
        omsi_scene::schedule_paper::update_vehicle(vehicle, duty, fonts, || {
            crate::settings::Settings::load().language
        })
    }
}
mod depot_bus;
mod devtools;
mod photo;
mod real_time;
mod remote;
mod settings;
#[rustfmt::skip]
mod shell;
mod telemetry;
#[rustfmt::skip]
mod themes;
use omsi_scene::threads;
use omsi_scene::tiles;
mod traffic;
mod ui;
mod view_sync;
#[rustfmt::skip]
mod watchdog;

// the game itself, split by what each part does
mod app;
mod app_events;
mod applog;
mod bus_service;
mod camera_util;
mod logmask;
// the game controllers, their mappings and force feedback (`omsi-input`), at their old paths
use omsi_input::{controllers, gamepad_profile, hpattern};
mod cli;
use omsi_input::diagnostics;
#[rustfmt::skip]
mod perf_overlay;
#[rustfmt::skip]
mod perf_report;
#[rustfmt::skip]
pub(crate) use omsi_cfg::profile_keys;
mod app_impl;
mod duty_start;
mod frame_stages;
mod input_script;
mod support_bundle;
use omsi_guard::launcher_link;
use omsi_lan::mods as lan_mods;
mod memory;
#[rustfmt::skip]
mod offscreen;
#[rustfmt::skip]
pub(crate) use omsi_sim::ground_gap;
#[rustfmt::skip]
mod on_foot;
mod optimiser;
mod trolley_hands;
mod trolley_mouse;
pub(crate) mod trolley_net;
use omsi_scene::route_arrows;
mod player;
mod plugin_io;
mod server;
// the plugins' panels (`omsi-plugin-ui`) at their old path, and the app's side of their input
use omsi_plugin_ui as plugin_ui;
mod plugin_panels;
mod plugins;
mod process_permissions;
mod services;
#[rustfmt::skip]
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

// the interface's translations (locales/app.yml and app.<language>.yml; the English text is the key)
rust_i18n::i18n!("locales");
// (the tables are read when this crate compiles: this makes cargo compile it again when they
// change - the macro alone left the old texts in the program)
#[rustfmt::skip]
use omsi_launcher_lib::i18n::TABLES as _LOCALES;

/// Show the interface in `code` (the settings' ENG / DEU / FRA / RUS).
#[rustfmt::skip]
pub(crate) fn ui_language(code: &str) {
    omsi_ui::i18n::set_lookup(|lang, text| {
        _rust_i18n_try_translate(lang, text).map(|t| t.into_owned())
    });
    static TEMPLATES: std::sync::Once = std::sync::Once::new();
    TEMPLATES.call_once(|| omsi_ui::i18n::set_templates(locale_keys(_LOCALES)));
    let iso = omsi_launcher_lib::language_iso(code);
    omsi_ui::i18n::set_language(iso);
    omsi_sim::vehicle_api::set_locale(iso);
}

#[rustfmt::skip]
const MISSING_ORIGINAL: &str = "The original OMSI 2 was not found.\n\n\
    openOMSI needs a complete installation of the original game (any version). \
    Choose its folder in the launcher (Setup), or start once with \
    --root \"/path/to/OMSI 2\", the folder with Omsi.exe, maps and Vehicles in it.\n\n\
    Missing in {folder}: {missing}";

#[rustfmt::skip]
fn locale_keys(yml: &str) -> impl Iterator<Item = String> + '_ {
    yml.lines()
        .filter_map(|l| l.strip_prefix('"')?.strip_suffix("\":"))
        .filter(|k| k.contains('{'))
        .filter_map(|k| {
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

use anyhow::{Context, Result, anyhow};
use app::*;
use camera_util::*;
use clap::Parser;
use cli::*;
use diagnostics::*;
use duty_start::*;
use glam::{DVec3, Vec3};
use input_script::*;
use launcher_link::*;
use memory::*;
use offscreen::*;
use omsi_render::{Camera, Renderer, Scene, SurfaceState};
use player::*;
use scene::World;
use services::*;
use situation::*;
use spawn::*;
use startup::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use traffic_link::*;
use weather_setup::*;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};
use world_load::*;

/// The game (and its launcher) from the command line: what `main` does.
#[rustfmt::skip]
pub fn run() -> Result<()> {
    omsi_cfg::migrate_legacy_data_dir();
    // mX Graphics on: the game is mX's from here on, wgpu is not touched
    if startup::mx_graphics() {
        return openomsi_game_mx::run();
    }
    view_sync::install_sim_decoder();
    startup::install_scene_settings();
    omsi_launcher_lib::adopt_own_inputs();
    #[cfg(target_os = "macos")]
    restart_with_allocator_settings();
    #[cfg(windows)]
    attach_parent_console();
    // `--check <file or folder>` (`--check-vehicle`, its old name): the loaders' findings
    // as JSON, no window, no log
    if matches!(
        std::env::args().nth(1).as_deref(),
        Some("--check" | "--check-vehicle")
    ) {
        let rest: Vec<String> = std::env::args().skip(2).collect();
        return check::run(&rest, crate::startup::find_root);
    }
    // (the players' internet addresses cut to their network, see `logmask`)
    logmask::init();
    // a panic goes into the log (which the launcher keeps per session) with where it
    // happened and a backtrace, not only to a terminal that may not be there
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if omsi_render::catching() {
            log::warn!("caught by the renderer: {info}");
            return;
        }
        let backtrace = std::backtrace::Backtrace::force_capture();
        log::error!("the game stopped on an error (build {BUILD}): {info}\n{backtrace}");
        omsi_platform::diagnostics::panicked(omsi_platform::diagnostics::panic_report(
            info, &backtrace,
        ));
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
        println!(
            "Support package written to {} - look inside before you attach it to an issue",
            out.display()
        );
        return Ok(());
    }
    // (a report of the mods, without a window: the content folder's `Mods/`, read against
    // the content folder and the installation, in the order the game searches them - #3606)
    if let Some(out) = args.export_mod_report.clone() {
        let mut args = args;
        discover_root(&mut args, find_root);
        let Some(c) = omsi_cfg::content_dir() else { anyhow::bail!("no content folder: --export-mod-report needs one") };
        let folders = omsi_cfg::mod_dirs(&c);
        let names: Vec<String> = folders.iter().filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).collect();
        let mut roots = folders.clone();
        roots.push(c.clone());
        roots.push(args.root.clone());
        let entries = omsi_cfg::overrides::overrides_in(&c, &roots, &folders, omsi_cfg::overrides::DEFAULT_LIMIT);
        if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&out, omsi_cfg::overrides::report(&entries, &names))?;
        let shared = entries.iter().filter(|e| e.between_mods(&names)).count();
        println!("{} mods, {} files they override ({} shipped by two packs) written to {}", names.len(), entries.len(), shared, out.display());
        return Ok(());
    }
    // (also without the content and the device: the map's `TTData` is all it reads)
    if let Some(out) = &args.export_stop_labels {
        let map_dir = Path::new(&args.map).parent().map(|p| args.root.join(p)).unwrap_or_else(|| args.root.clone());
        let n = omsi_timetable::stop_labels::export(&map_dir, out)
            .with_context(|| format!("stop labels of {}", map_dir.display()))?;
        println!("Stop labels of {} stops written to {}", n, out.display());
        return Ok(());
    }
    // Started by a double click or with no arguments at all: that is the launcher's job.
    // The launcher itself runs the game with a full command line (--no-menu, --map, ...).
    let bare = std::env::args().len() == 1;
    let Some((args, server_cfg)) = prepare(args, bare)? else {
        return Ok(());
    };
    if bare && !args.menu {
        // the launcher, another program (it starts the game again with a full command
        // line): OMSI_LAUNCHER=<program>, else the desktop launcher beside the game
        if omsi_cfg::flags::OMSI_LAUNCHER.is_set() && open_launcher()? {
            return Ok(());
        }
        return open_app_launcher(fatal_message);
    }
    // Launcher helpers and the launcher itself must not count as a running game.
    let _cache_activity = omsi_cfg::cache_activity::game(&omsi_launcher_lib::data_dir())?
        .only_cleanup_when(|| {
            !omsi_launcher_lib::instances::list()
                .iter()
                .any(|game| game.running && game.pid != std::process::id())
        });
    let Some(app) = make_app(args, server_cfg)? else {
        return Ok(());
    };
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
#[rustfmt::skip]
fn game_event_loop() -> Result<EventLoop<()>> {
    #[cfg(target_os = "macos")]
    if omsi_cfg::flags::OMSI_HIDDEN_WINDOW.is_set() {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        return Ok(EventLoop::builder()
            .with_activation_policy(ActivationPolicy::Accessory)
            .build()?);
    }
    Ok(EventLoop::new()?)
}

/// The original installation a start would use: the folder it was given when that is already
/// complete, else the one found by the same search a game start uses. A start script checking
/// its folder with `--list-maps` never gets another one instead.
#[rustfmt::skip]
fn discover_root(args: &mut Args, find: impl FnOnce() -> Option<PathBuf>) -> bool {
    if is_omsi_root(&args.root) || args.list_maps { return false; }
    let Some(root) = find() else { return false; };
    log::info!("OMSI 2 found at {}", root.display());
    args.root = root;
    true
}

/// Everything before a window: the language, the session's random seed, the original
/// installation and the content roots (mods, archives). None when the program has
/// nothing more to do (a fatal error was shown).
pub(crate) fn prepare(
    mut args: Args,
    bare: bool,
) -> Result<Option<(Args, Option<server::ServerCfg>)>> {
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
    if !discover_root(&mut args, find_root) && !is_omsi_root(&args.root) {
        if launcher_mode {
            // the launcher opens without the game: its Setup page is where the folder is
            // chosen (only a session needs the original installation)
            log::warn!("the original OMSI 2 was not found; the launcher asks for it");
        } else {
            // openOMSI plays on the original game's content: without a complete
            // installation of it there is nothing to play on, and every player must have
            // the same base whatever copy of OMSI 2 they own
            let missing = omsi_cfg::missing_original_essentials(&args.root);
            let text = omsi_ui::tr(MISSING_ORIGINAL)
                .replace("{folder}", &args.root.display().to_string())
                .replace("{missing}", &missing.join(", "));
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
    if let Some(memo) = root_memo()
        .filter(|_| is_omsi_root(&args.root) && server_cfg.is_none() && args.pack_map.is_none())
    {
        let _ = std::fs::write(memo, args.root.to_string_lossy().as_bytes());
    }
    // content roots: the game's own folder (mods) first, then the original installation
    if let Some(c) = content_dir() {
        match omsi_cfg::ensure_content_layout(&c) {
            Ok(()) => {
                // a pack lying in `Mods/` as it came is its own content root, searched
                // before the content folder: not merged into its tree, and out again when
                // its folder is deleted (#2135)
                for r in omsi_cfg::mount_mod_dirs(&c) {
                    log::info!("mod folder: {}", r.display());
                }
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
pub(crate) fn make_app(
    mut args: Args,
    server_cfg: Option<server::ServerCfg>,
) -> Result<Option<App>> {
    if let (Some(map), Some(to)) = (args.pack_map.clone(), args.pack_to.clone()) {
        omsi_lan::map_pack::run(&map, &to)?;
        return Ok(None);
    }
    // the window's simulation beside the picture or inline, for the whole session (an
    // offscreen run - a dedicated server, a picture - steps inline and never asks)
    if args.offscreen.is_none() {
        let sim_thread = omsi_sim::sim_thread::set_enabled(settings::Settings::load().sim_thread);
        log::info!(
            "sim thread: {}",
            if sim_thread {
                "the AI traffic, the people and the scenery scripts step beside the picture"
            } else {
                "off: every step on the main thread"
            }
        );
    }
    let _lan_status = lan::StatusFileGuard;
    // (a resumed bus loads its saved sheet by full path before any sheet is made this run)
    omsi_scene::schedule_paper::trust_cache_dir();
    // OMSI's tutorials: the lesson's own situation
    if let Some(n) = args.tutorial.filter(|n| (1..=4).contains(n)) {
        args.situation = Some(tutorial::SITUATIONS[n - 1].to_string());
    }
    apply_situation(&mut args)?;
    // `openomsi`: the official server, wherever its tunnel is today (see omsi_net::official)
    if let Some(t) = args
        .lan_join
        .clone()
        .filter(|t| omsi_net::official::is_alias(t))
    {
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
    if settings::Settings::load().time_sync
        && args.lan_join.is_none()
        && args.server.is_none()
        && args.offscreen.is_none()
    {
        real_time::start_at_now(&mut args);
    }
    // a season chosen with its phase moves the date into it (the sun, the weather and the
    // timetable go by the date); a joining player takes the host's date below
    season_phase::apply_season_date(&mut args);
    if args.export_glb.is_none() && args.lan_join.is_none() {
        place_on_duty(&mut args);
    }
    if server_cfg.is_none() && args.export_glb.is_none() {
        omsi_platform::diagnostics::game_started();
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
    let settings_text =
        std::fs::read_to_string(omsi_launcher_lib::data_dir().join("settings.cfg")).ok();
    omsi_lan::bans::allow_hardware_id(
        omsi_launcher_lib::settings_from_text(settings_text.as_deref())
            .get("hardware_id")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
    );
    let mut lan = if args.export_glb.is_none() {
        lan::start(
            &args.lan_config(),
            &lan::AppLanHost {
                args: &args,
                world: None,
            },
        )
    } else {
        None
    };
    // Match the actual session admission limit to the dedicated server configuration.
    if let (Some(cfg), Some(l)) = (server_cfg.as_ref(), lan.as_mut()) {
        l.set_peer_limit(cfg.max_players);
    }
    // a dedicated server without its session has nobody to serve: why, and the end (it went
    // on running alone, unreachable, with a warning among the log's lines)
    if server_cfg.is_some() && lan.is_none() {
        // Do not pass the password-bearing server configuration to diagnostics.
        // The failed bind above already reports the candidate port and socket error.
        eprintln!(
            "server: cannot open the session (the line above says why): another server or program uses the UDP/TCP port, or the system refuses it. Stop the other one, or choose another `port` in the server configuration."
        );
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
    let mut mods_server = None;
    if let Some(l) = lan.as_mut() {
        let mut c = args.lan_config();
        mods_server = lan::share_mods(&mut c, l);
        if let Some(why) = c.mod_fetch_error.take() {
            return Err(anyhow::anyhow!(
                "The selected server bus could not be verified or downloaded: {why}"
            ));
        }
        lan::take_host_map(&mut c, l);
        args.take_lan_config(&c);
    }
    if let (Some(cfg), Some(l)) = (server_cfg.as_ref(), lan.as_ref()) {
        lan::open_public_gateway(
            l,
            server::info_of(cfg, &args.root),
            cfg.web_port,
            cfg.tunnel,
        );
        server::announce_fleet(cfg, &args.root);
        lan::publish_vehicles(args.root.clone(), cfg.vehicles.clone());
        server::start_public_list(cfg);
        if cfg.tunnel {
            // the address to give the players, as soon as cloudflared says it
            std::thread::spawn(|| {
                for _ in 0..300 {
                    if let Some(u) = lan::tunnel_url() {
                        println!(
                            "\n  Server address for the players: {u}\n  (Multiplayer -> Servers -> Add)\n"
                        );
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
                println!(
                    "  No tunnel address (is cloudflared installed?): players join at this machine's address and the UDP port"
                );
            });
        }
    }
    // a host's clock runs at its time speed (a server's: its server.cfg)
    if let (Some(l), None) = (lan.as_mut(), server_cfg.as_ref()) {
        if l.role == omsi_net::Role::Host {
            l.clock_speed = if settings.time_sync {
                1.0
            } else {
                settings.time_speed.clamp(1.0, 30.0)
            };
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
            lan::adopt_host_world(
                &mut c,
                &crate::app_impl::lan_tick::AppLanHost {
                    args: &args,
                    world: None,
                },
                l,
                &mut lan_game,
            );
            args.take_lan_config(&c);
        }
        let r = run_offscreen(&args, &out, lan, lan_game);
        lan_mods::clean_up();
        return r.map(|_| None);
    }
    let mut app = assemble_app(args, settings);
    app.net.lan = lan;
    app.net.mods_server = mods_server;
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
fn initial_view_state() -> ViewState {
    ViewState {
        starting: None,
        speed: 30.0,
        f1_reset: None,
        view_yaw: 0.0,
        view_fov: None,
        head_idle_hold: Default::default(),
        ego: false,
        flashlight: false,
        flashlight_aim: None,
        in_cab: false,
        heard_inside: false,
        look: (0.0, 0.0),
        look_smooth: (0.0, 0.0),
        view_looks: Default::default(),
        look_view: String::new(),
        cam_blend: Default::default(),
        view_zoom: Default::default(),
        zoom_hold: Default::default(),
        cam_zooms: Default::default(),
        orbit: ORBIT_DEFAULT,
    }
}

fn assemble_app(args: Args, settings: settings::Settings) -> App {
    let view = args.view.clone();
    let args_root_for_keys = args.root.clone();
    let clock_note = args.clock_moved.clone();
    let is_server = args.server.is_some();
    let sky = crate::weather_setup::SkyState {
        look: args.session_settings,
        ..Default::default()
    };
    // What loads, starts or reads the clock, made one after the other in a fixed order before
    // the App and its groups are put together (their logs and threads come in this order).
    // Steam must be initialized before the graphics instance and window so its overlay can
    // hook the rendering process before the first surface is created.
    #[cfg(steam)]
    // Fixtures must not create multiple process-global native Steam clients.
    let steam = if is_server || cfg!(test) {
        None
    } else {
        crate::steam::Steam::start()
    };
    let instance = graphics_instance();
    let vr_nav_profiles =
        crate::vr_navigator::Profiles::load(crate::vr_navigator::profiles_path().as_deref());
    let ui = ui::Ui::new();
    let rain = rain::Rain::new();
    let cabin_air = crate::condensation::CabinAir::new();
    let mut spray = puddles::Spray::new();
    spray.graphics = args.session_settings.graphics();
    let radio = radio::Radio::load(&args_root_for_keys);
    let started = Instant::now();
    let last = Instant::now();
    let input_script = parse_input_script();
    let game_keys =
        omsi_content::KeyboardCfg::load(&crate::startup::keyboard_cfg(&args_root_for_keys))
            .unwrap_or_default()
            .with_game_defaults()
            .with_vr_defaults()
            .game;
    let own_keys = crate::startup::own_keys(&args_root_for_keys);
    let own_shift =
        crate::startup::own_bindings(&args_root_for_keys, omsi_content::input::KEY_SHIFT);
    let fps_t = Instant::now();
    let update_watch = crate::update_watch::UpdateWatch::new();
    // (a server counts its players by their own games, not itself)
    let presence = if is_server || cfg!(test) {
        None
    } else {
        crate::presence::Presence::start()
    };
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
            mirror_debt: 0,
            mirror_panel_cams: Vec::new(),
            frozen_mirrors: None,
            mirror_hud: Default::default(),
            window_hidden: false,
            route_arrows: Default::default(),
            plugin_cables: Default::default(),
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
            depot_base: Default::default(),
            depot_prev: None,
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
            graphics_before: None,
            page_titles: Default::default(),
            online_weather: Default::default(),
            chooser: None,
            editor: None,
            vehicle_list: Vec::new(),
            dropdown: None,
            vehicle_meta: std::collections::HashMap::new(),
            vehicle_about: std::collections::HashMap::new(),
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
            map_place_from: None,
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
            // (as the last session left it, #1164)
            info_bar: settings.info_bar,
            timetable: false,
            clean_screen: false,
            hud_resize: None,
            notices: Vec::new(),
            note_watch: Default::default(),
            loading: Default::default(),
            menu_edit: None,
            load_request: None,
            menu_edit_icao: false,
            menu_edit_number: false,
            menu_edit_ident: false,
            menu_edit_search: false,
            streets_seen: 0,
            menu_search: String::new(),
            swap_pending: false,
            swapped_in: None,
            parked_transfer: None,
            pending_placement: None,
            menu_drag: None,
            menu_kbd: true,
        },
        world: None,
        cam: initial_view_state(),
        ui,
        perf: PerfState::new(input_script, fps_t),
        sound: SoundState {
            radio,
            milezero: Default::default(),
            switch_sound: Default::default(),
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
            pad_zoom_hold: false,
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
            cursor_stale: false,
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
            discord_max: Default::default(),
            #[cfg(steam)]
            steam,
            update_watch,
            presence,
            plugins: None,
            vehicle_scripts_of: None,
            object_scripts: Default::default(),
            remote: Default::default(),
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
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lib_test.rs"]
mod test_support;
#[cfg(test)]
pub(crate) use test_support::with_test_language;

#[cfg(test)]
#[path = "lib_locale_tests.rs"]
mod locale_tests;
