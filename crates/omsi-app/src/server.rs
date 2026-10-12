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

pub(crate) use omsi_lan::server_cfg::{allows, enforce_vehicles, player_info, ServerCfg, SERVER_VEHICLES};

/// The server's status as the web gateway tells it (see `omsi_net::ws::ServerInfo`).
pub(crate) fn info_of(cfg: &ServerCfg) -> omsi_net::ws::ServerInfo {
    omsi_net::ws::ServerInfo {
        name: cfg.name.clone(),
        motd: cfg.motd.clone(),
        map: cfg.map.clone(),
        players: 0,
        max_players: cfg.max_players,
        version: format!("{} ({})", env!("CARGO_PKG_VERSION"), BUILD),
        icon: cfg.icon.clone(),
        time: cfg.time.clone(),
        date: cfg.date.clone().unwrap_or_default(),
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
    let cfg = ServerCfg::load(path, crate::game_lists::TRAFFIC_MAX)?;
    SERVER_MODE.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = SERVER_ADMIN.set((cfg.admin_password.clone(), if cfg.real_time { 1.0 } else { cfg.time_speed }));
    crate::real_time::set_server_real(cfg.real_time);
    let _ = SERVER_VEHICLES.set(cfg.vehicles.clone());
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
    let station = cfg.metar_sync.then(|| if cfg.metar_station.is_empty() { crate::launcher::drive::nearest_airport(&args.root.to_string_lossy(), &args.map) } else { cfg.metar_station.clone() });
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
    args.lan_host = Some(cfg.port);
    args.lan_join = None;
    args.lan_name = cfg.name.clone();
    args.bus = None;
    args.radius = Some(if cfg.radius <= 0 { 99 } else { cfg.radius });
    args.size = "64x36".into();
    if args.offscreen.is_none() {
        args.offscreen = Some(std::env::temp_dir().join("omsi-server-unused.png"));
    }
    log::info!("server '{}': map {}, {} at {}, traffic {}, timetable {}, passengers {}, UDP {} / web {}, at most {} players", cfg.name, cfg.map, cfg.date.as_deref().unwrap_or("today"), cfg.time, cfg.traffic, cfg.timetable, cfg.passengers, cfg.port, cfg.web_port, cfg.max_players);
    Ok(cfg)
}

/// The airport whose METAR report a dedicated server's weather follows (None: no METAR sync).
pub(crate) static SERVER_METAR: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// A dedicated server's voice server (`voice_*` of `server.cfg`).
pub(crate) use omsi_lan::voice::SERVER_VOICE;

/// A dedicated server's admin password and clock speed (for the host loop).
pub(crate) static SERVER_ADMIN: std::sync::OnceLock<(String, f64)> = std::sync::OnceLock::new();

/// A dedicated server run: graphics without a device, the whole world by interest.
pub(crate) static SERVER_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Every second of a server run: what the status page says (players, time, weather) and
/// who is where (`GET /players`, when `share_positions` is on).
pub(crate) fn tick_status(lan: &omsi_net::LanSession, time: f64, weather: &str) {
    let players = lan.peers().filter(|p| p.has_info).count();
    // (the admin's clock shift may take the time below 0 or past midnight: 23:08 had come
    // out as "00:-52")
    crate::lan::update_server_info(players, &crate::schedule::hhmm(time.rem_euclid(86400.0)), weather, "");
    let poses: Vec<&omsi_net::Pose> = lan.peers().filter(|p| p.has_info && p.has_pose).map(|p| &p.pose).collect();
    let list = poses.iter().filter_map(|q| player_info(q, |id| poses.iter().copied().find(|o| o.id == id))).collect();
    crate::lan::update_server_players(list);
}
