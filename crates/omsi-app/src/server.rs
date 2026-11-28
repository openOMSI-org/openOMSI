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

pub(crate) use omsi_server::console;

pub(crate) use omsi_lan::server_cfg::{SERVER_VEHICLES, ServerCfg, enforce_vehicles, player_info};

pub(crate) use omsi_server::fleet::announce_fleet;

// Keep the legacy graphical host's admin panel compatible while the native
// headless server owns real timing measurements. Do not invent legacy TPS.
pub(crate) use omsi_server::admin::tick_status;

pub(crate) fn admin_state(
    time: f64,
    speed: f64,
    weather: &str,
    metar: Option<&str>,
    world: omsi_net::ws::WorldCounts,
) -> omsi_admin::ServerState {
    let meter = omsi_admin::metrics::TickMeter::new(omsi_server::tick::SERVER_TPS as f64);
    let profiler = omsi_admin::profiler::Profiler::new();
    let mut state =
        omsi_server::admin::admin_state(time, speed, weather, metar, world, &meter, &profiler);
    state.profiler = None;
    state
}

pub(crate) fn info_of(cfg: &ServerCfg, _root: &Path) -> omsi_net::ws::ServerInfo {
    omsi_server::admin::info_of(cfg, &format!("{} ({})", omsi_platform::VERSION, BUILD))
}

/// The floor nearest the height a parked bus stands at (a bus on the lower of two floors
/// stays on it); only where there is none, the one further up, then the terrain.
pub(crate) fn parked_floor(world: &World, x: f64, y: f64, near: f64) -> Option<f64> {
    world
        .stand_height(x, y, near)
        .or_else(|| world.stand_height(x, y, near + 4.0))
        .or_else(|| world.ground_height(x, y))
}

pub(crate) fn park_buses(server: bool, args: &Args, world: &World) {
    if server {
        let spots: Vec<omsi_lan::parked::Spot> = world
            .global
            .entry_points
            .iter()
            .filter_map(|e| world.entry_point_place(e))
            .map(|(at, h)| (at.to_array(), h[0] as f32))
            .collect();
        omsi_lan::parked::start(&args.root, &args.map, &spots, &|x, y, near| {
            parked_floor(world, x, y, near)
        });
    }
}

/// Prepare the legacy offscreen adapter while the new headless engine replaces it.
pub(crate) fn prepare(args: &mut Args, path: &Path) -> Result<ServerCfg> {
    let map_override = args.server_map.take();
    let plan = omsi_server::startup::prepare(
        path,
        &args.root,
        map_override,
        crate::game_lists::TRAFFIC_MAX,
        &format!("{} ({})", omsi_platform::VERSION, BUILD),
        &|root, map| omsi_launcher_lib::nearest_airport(&root.to_string_lossy(), map),
    )?;
    let cfg = plan.config;
    SERVER_MODE.store(true, std::sync::atomic::Ordering::Relaxed);
    crate::real_time::set_server_real(cfg.real_time);
    crate::logmask::keep_whole(cfg.log_addresses);
    args.map = cfg.map.clone();
    args.time = cfg.time.clone();
    if let Some(date) = &cfg.date {
        args.date = Some(date.clone());
    }
    if cfg.real_time {
        crate::real_time::start_at_now(args);
    }
    args.weather = plan.weather;
    args.traffic = cfg.traffic;
    args.schedule = cfg.timetable;
    args.ai_drive_through_parked = cfg.ai_drive_through_parked;
    args.passengers = cfg.passengers;
    if !plan.ai_limits.is_empty() {
        let _ = crate::settings::SERVER_AI_LIMITS.set(plan.ai_limits);
    }
    args.lan_host = Some(cfg.port);
    args.lan_join = None;
    args.lan_name = cfg.name.clone();
    args.bus = None;
    args.radius = Some(if cfg.radius <= 0 { 99 } else { cfg.radius });
    args.size = "64x36".into();
    if args.offscreen.is_none() {
        args.offscreen = Some(std::env::temp_dir().join("omsi-server-unused.png"));
    }
    Ok(cfg)
}

pub(crate) use omsi_server::startup::{SERVER_ADMIN, SERVER_METAR, SERVER_PASSENGERS};

pub(crate) fn check_map(args: &Args) -> Result<(), String> {
    omsi_server::world::check_map(&args.root, &args.map)
}

pub(crate) use omsi_lan::server_cfg::installed_maps;

/// A dedicated server run: graphics without a device, the whole world by interest.
pub(crate) static SERVER_MODE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Start publishing the dedicated host in the public server list.
pub(crate) fn start_public_list(cfg: &ServerCfg) {
    omsi_server::admin::start_public_list(cfg, &format!("{} ({})", omsi_platform::VERSION, BUILD));
}
