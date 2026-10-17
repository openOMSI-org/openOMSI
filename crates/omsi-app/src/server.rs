//! The dedicated server (`omsi --server server.cfg`): a session host that is always on, like
//! a Minecraft server - no window, no sound, no graphics card (the renderer runs on wgpu's
//! no-op device: the world, the traffic, the timetable and the people are simulated as the
//! host's game simulates them, and nothing is drawn). Players reach it at `host:port` (UDP)
//! or at its web address over a WebSocket (`omsi_net::ws`), which is what a free Cloudflare
//! tunnel carries: `tunnel = 1` starts one and prints the `https://….trycloudflare.com`
//! address to add in the launcher's Multiplayer → Servers.
//!
//! The folder of `server.cfg` is the server's: `server-icon.png` beside it is its icon (a
//! 64x64 PNG, like Minecraft's), `server.log` its log. A missing `server.cfg` is written with
//! the defaults and a comment for every key.

use super::*;
use std::path::Path;

pub(crate) use omsi_lan::server_cfg::{enforce_vehicles, player_info, ServerCfg, SERVER_VEHICLES};

/// The server's status as the web gateway tells it (see `omsi_net::ws::ServerInfo`).
pub(crate) fn info_of(cfg: &ServerCfg) -> omsi_net::ws::ServerInfo {
    omsi_net::ws::ServerInfo {
        name: cfg.name.clone(),
        motd: cfg.motd.clone(),
        map: cfg.map.clone(),
        players: 0,
        max_players: cfg.max_players,
        version: format!("{} ({})", omsi_platform::VERSION, BUILD),
        icon: cfg.icon.clone(),
        time: cfg.time.clone(),
        weather: cfg.weather.clone().unwrap_or_default(),
        password: false,
        vehicles: cfg.vehicles.clone(),
        reached_at: String::new(),
        players_public: cfg.share_positions,
        player_list: Vec::new(),
        local_admin_password: cfg.admin_password.clone(),
        ..Default::default()
    }
}

/// Set up the arguments for a server run from `server.cfg` (the rest is the offscreen
/// host loop, see `offscreen::run_offscreen`).
pub(crate) fn prepare(args: &mut Args, path: &Path) -> Result<ServerCfg> {
    let mut cfg = ServerCfg::load(path, crate::game_lists::TRAFFIC_MAX)?;
    // (the map a start script picked, over server.cfg's)
    if let Some(m) = args.server_map.take().filter(|m| !m.trim().is_empty()) {
        cfg.map = m.trim().replace('\\', "/");
    }
    SERVER_MODE.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = SERVER_ADMIN.set((cfg.admin_password.clone(), cfg.moderator_password.clone(), if cfg.real_time { 1.0 } else { cfg.time_speed }));
    start_administration(&cfg, path);
    crate::real_time::set_server_real(cfg.real_time);
    let _ = SERVER_VEHICLES.set(cfg.vehicles.clone());
    cfg.apply_to_mods();
    let _ = SERVER_VOICE.set(cfg.voice.clone());
    args.map = cfg.map.clone();
    args.time = cfg.time.clone();
    if let Some(d) = &cfg.date {
        args.date = Some(d.clone());
    }
    if cfg.real_time {
        crate::real_time::start_at_now(args);
    }
    args.weather = cfg.weather.clone();
    // the METAR sync: the report's weather from the start (the host loop downloads it again)
    let station = cfg.metar_sync.then(|| if cfg.metar_station.is_empty() { omsi_launcher_lib::nearest_airport(&args.root.to_string_lossy(), &args.map) } else { cfg.metar_station.clone() });
    if let Some(icao) = station.as_ref() {
        match crate::weather_setup::try_metar(icao).and_then(|w| crate::weather_setup::report_wire(&w)) {
            Some(wire) => args.weather = Some(wire),
            None => log::warn!("server: no METAR report for {icao} yet; trying again soon"),
        }
        log::info!("server: the weather follows the METAR report of {icao}");
    }
    let _ = SERVER_METAR.set(station);
    args.traffic = cfg.traffic;
    args.schedule = cfg.timetable;
    args.passengers = cfg.passengers;
    let _ = SERVER_PASSENGERS.set((cfg.passengers, cfg.passenger_density));
    args.lan_host = Some(cfg.port);
    args.lan_join = None;
    args.lan_name = cfg.name.clone();
    args.bus = None;
    args.radius = Some(if cfg.radius <= 0 { 99 } else { cfg.radius });
    args.size = "64x36".into();
    if args.offscreen.is_none() {
        args.offscreen = Some(std::env::temp_dir().join("omsi-server-unused.png"));
    }
    let density = cfg.passenger_density.map(|d| d.to_string()).unwrap_or_else(|| "as settings.cfg".into());
    log::info!("server '{}': map {}, {} at {}, traffic {}, timetable {}, passengers {} (density {density}), UDP {} / web {}, at most {} players", cfg.name, cfg.map, cfg.date.as_deref().unwrap_or("today"), cfg.time, cfg.traffic, cfg.timetable, cfg.passengers, cfg.port, cfg.web_port, cfg.max_players);
    Ok(cfg)
}

/// A dedicated server's people: whether it has any (`passengers`) and its
/// `passenger_density` (None: the machine's own passengers setting).
pub(crate) static SERVER_PASSENGERS: std::sync::OnceLock<(bool, Option<f32>)> = std::sync::OnceLock::new();

/// The map of `server.cfg` is installed (in the original folder or the content folder), or
/// what to do: the maps there are.
pub(crate) fn check_map(args: &Args) -> Result<(), String> {
    let path = omsi_cfg::resolve_path(&args.root, &args.map);
    if omsi_cfg::vfs::exists(&path) {
        return Ok(());
    }
    Err(format!("the map {} (server.cfg's map, or --server-map) is not installed ({} not found). Installed maps: {}", args.map, path.display(), Some(installed_maps().join(", ")).filter(|l| !l.is_empty()).unwrap_or_else(|| "none".into())))
}

pub(crate) use omsi_lan::server_cfg::installed_maps;

/// The airport whose METAR report a dedicated server's weather follows (None: no METAR sync).
pub(crate) static SERVER_METAR: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// A dedicated server's voice server (`voice_*` of `server.cfg`).
pub(crate) use omsi_lan::voice::SERVER_VOICE;

/// A dedicated server's admin and moderator passwords and clock speed (for the host loop).
pub(crate) static SERVER_ADMIN: std::sync::OnceLock<(String, String, f64)> = std::sync::OnceLock::new();

/// What of a dedicated server's state stays as `server.cfg` says (the rest is the loop's).
static SERVER_STATE: std::sync::OnceLock<omsi_admin::ServerState> = std::sync::OnceLock::new();

/// The administration's hub (see `omsi_admin`): its records, its sessions, its audit log
/// (`admin-audit.log` beside `server.cfg`, put aside at 1 MB).
fn start_administration(cfg: &ServerCfg, path: &Path) {
    if !cfg.moderator_password.is_empty() && cfg.moderator_password == cfg.admin_password {
        log::warn!("server.cfg: moderator_password is the admin_password: it gives the admin's role");
    }
    let pw = omsi_admin::Passwords { admin: cfg.admin_password.clone(), moderator: cfg.moderator_password.clone() };
    let audit = path.parent().map(|d| d.join("admin-audit.log"));
    let hub = std::sync::Arc::new(omsi_admin::Hub::new(omsi_admin::Auth::new(pw, Default::default()), omsi_admin::Audit::new(audit, 1 << 20)));
    omsi_lan::admin::install_hub(hub.clone());
    omsi_net::ws::believe_cf_connecting_ip(cfg.gateway_cf_connecting_ip);
    // the web panel and its API on the gateway (`admin_web`: who may reach them)
    let web = omsi_admin::WebCfg { mode: cfg.admin_web, origins: cfg.admin_web_origins.clone(), trusted_proxies: cfg.admin_trusted_proxies.clone(), proxy_header: cfg.admin_proxy_header, traffic_max: crate::game_lists::TRAFFIC_MAX };
    let has_password = !cfg.admin_password.is_empty() || !cfg.moderator_password.is_empty();
    match (web.mode, has_password) {
        (omsi_admin::WebMode::Off, _) => log::info!("server: no web administration (admin_web = off)"),
        (_, false) => log::info!("server: no admin_password: the administration is off"),
        (omsi_admin::WebMode::Local, true) => log::info!("server: web administration at http://127.0.0.1:{}/admin/ (from this machine only)", cfg.web_port),
        (omsi_admin::WebMode::Remote, true) => log::warn!("server: web administration at http://127.0.0.1:{}/admin/ and over HTTPS through {:?}{}", cfg.web_port, web.trusted_proxies, if cfg.tunnel { " - with the public tunnel too: only the passwords guard it" } else { "" }),
    }
    omsi_lan::gateway::open_admin_api(hub, web);
    let _ = SERVER_STATE.set(omsi_admin::ServerState {
        name: cfg.name.clone(),
        map: cfg.map.clone(),
        version: format!("{} ({})", omsi_platform::VERSION, BUILD),
        max_players: cfg.max_players,
        real_time: cfg.real_time,
        traffic_max: crate::game_lists::TRAFFIC_MAX,
        timetable: cfg.timetable,
        passenger_density_max: omsi_admin::PASSENGER_DENSITY_MAX,
        ..Default::default()
    });
}

/// The server's state for the admins' panels now (`time`: the clock, s; `world`: as for
/// `GET /status`).
pub(crate) fn admin_state(time: f64, speed: f64, weather: &str, metar: Option<&str>, world: omsi_net::ws::WorldCounts) -> omsi_admin::ServerState {
    let clock = time.rem_euclid(86_400.0);
    omsi_admin::ServerState {
        time: crate::schedule::hhmm(clock),
        clock,
        clock_speed: speed,
        weather: weather.to_string(),
        metar: metar.map(str::to_string),
        traffic: world.traffic,
        passengers: world.passengers,
        passenger_density: world.passenger_density,
        waiting: world.waiting,
        walking: world.walking,
        aboard: world.aboard,
        cars: world.cars,
        buses: world.buses,
        ..SERVER_STATE.get().cloned().unwrap_or_default()
    }
}

/// A dedicated server run: graphics without a device, the whole world by interest.
pub(crate) static SERVER_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Every second of a server run: what the status page says (players, time, weather) and
/// who is where (`GET /players`, when `share_positions` is on).
pub(crate) fn tick_status(lan: &omsi_net::LanSession, time: f64, weather: &str) {
    let players = lan.peers().filter(|p| p.has_info).count();
    // (the admin's clock shift may take the time below 0 or past midnight: 23:08 had come
    // out as "00:-52")
    crate::lan::update_server_info(players, &crate::schedule::hhmm(time.rem_euclid(86400.0)), weather);
    let poses: Vec<&omsi_net::Pose> = lan.peers().filter(|p| p.has_info && p.has_pose).map(|p| &p.pose).collect();
    let list = poses.iter().filter_map(|q| player_info(q, |id| poses.iter().copied().find(|o| o.id == id))).collect();
    crate::lan::update_server_players(list);
}
