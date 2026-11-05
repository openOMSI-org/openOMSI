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

/// Announce the server's own bus paints without allocating GPU resources. Only
/// buses explicitly in server.cfg are indexed; an unrestricted fleet has no finite
/// advertised catalogue. The launcher remains responsible for authenticated content
/// checks before offering any remote-only vehicle for driving.
fn fleet_paints(cfg: &ServerCfg, root: &Path) -> Vec<omsi_net::ws::FleetPaints> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let total = cfg.vehicles.len().min(256);
    if cfg.vehicles.len() > 256 {
        log::warn!("server fleet: {} buses in `vehicles`: only the first 256 are announced", cfg.vehicles.len());
    }
    if total == 0 {
        return Vec::new();
    }
    // (every file of every bus and paint is read and hashed - their hashes are kept for the
    // next start - four buses at a time; the web port is open meanwhile)
    log::info!("server fleet: checking {total} buses and their paints (a first start reads every file; later starts are faster)");
    let (t0, done, failed) = (std::time::Instant::now(), AtomicUsize::new(0), AtomicUsize::new(0));
    let one = |bus: &String| -> Option<omsi_net::ws::FleetPaints> {
        let fail = |why: String| {
            failed.fetch_add(1, Ordering::Relaxed);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            log::warn!("server fleet: {n}/{total}: {why}");
        };
        let Some(rel) = omsi_net::vehicle_path(bus) else {
            fail(format!("ignoring invalid bus path {bus:?}"));
            return None;
        };
        let found = omsi_cfg::find_in_roots(&rel).map(|(_, p)| p);
        let path = found.clone().unwrap_or_else(|| omsi_cfg::resolve_path(root, &rel));
        let t = std::time::Instant::now();
        let vehicle = match omsi_sim::VehicleType::load(root, &path) {
            Ok(v) => v,
            Err(e) => {
                let why = if found.is_none() { " (not found in the content folders)" } else { "" };
                fail(format!("cannot load {}{why}: {e}", path.display()));
                return None;
            }
        };
        // Use the same resolved scheme names as the authenticated LISTBUS
        // transfer. A server build must not depend on launcher-only crates.
        let loaded = t.elapsed().as_secs_f64();
        let paints: Vec<String> = vehicle.paint_schemes.iter().take(128).map(|p| p.name.clone()).collect();
        let content = match omsi_lan::mods::fleet_fingerprint_loaded(root, &rel, &path, &vehicle) {
            Ok(c) => c,
            Err(e) => {
                fail(format!("{rel}: its files cannot be offered to the players ({e})"));
                String::new()
            }
        };
        if !content.is_empty() {
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            log::info!("server fleet: {n}/{total}: {rel}: {} paints ({:.1} s, loading the bus {loaded:.1} s)", paints.len(), t.elapsed().as_secs_f64());
        }
        Some(omsi_net::ws::FleetPaints { bus: rel, paints, content })
    };
    // four plain threads taking the next bus in turn - not a Rayon pool: the hashing inside
    // runs on a pool of its own, and a Rayon worker waiting on another pool takes more of its
    // own work meanwhile, so all 54 buses of a server were begun at once and ended together
    let buses = &cfg.vehicles[..total];
    let next = AtomicUsize::new(0);
    let checked: Vec<(usize, omsi_net::ws::FleetPaints)> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..4.min(total))
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let k = next.fetch_add(1, Ordering::Relaxed);
                        let Some(bus) = buses.get(k) else { break };
                        if let Some(offer) = one(bus) {
                            mine.push((k, offer));
                        }
                    }
                    mine
                })
            })
            .collect();
        workers.into_iter().flat_map(|w| w.join().unwrap_or_default()).collect()
    });
    let mut checked = checked;
    // (in the order of `vehicles`, as before)
    checked.sort_by_key(|(k, _)| *k);
    let offers: Vec<_> = checked.into_iter().map(|(_, o)| o).collect();
    let failed = failed.load(Ordering::Relaxed);
    log::info!(
        "server fleet: announced {} models and {} paint schemes in {:.1} s{}",
        offers.len(),
        offers.iter().map(|o| o.paints.len()).sum::<usize>(),
        t0.elapsed().as_secs_f64(),
        if failed > 0 { format!(", {failed} with problems (the lines above say which)") } else { String::new() }
    );
    offers
}

/// Check the server's fleet on a thread of its own and announce it when done: the web port
/// (status page, players' WebSockets, admin) is open meanwhile, telling no fleet as the
/// servers before 0.3.47 did - a first check of a large fleet took minutes, and the web port
/// refused every connection until it was over.
pub(crate) fn announce_fleet(cfg: &ServerCfg, root: &Path) {
    if cfg.vehicles.is_empty() || omsi_cfg::flags::OMSI_NO_LAN_MODS.is_set() {
        return;
    }
    let (cfg, root) = (cfg.clone(), root.to_path_buf());
    let spawned = std::thread::Builder::new().name("server fleet".into()).spawn(move || {
        let fleet = fleet_paints(&cfg, &root);
        omsi_lan::game::update_server_fleet(fleet, 1);
    });
    if let Err(e) = spawned {
        log::warn!("server fleet: cannot check the fleet: {e}");
    }
}

/// The server's status as the web gateway tells it (see `omsi_net::ws::ServerInfo`).
pub(crate) fn info_of(cfg: &ServerCfg, _root: &Path) -> omsi_net::ws::ServerInfo {
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
        // (announced once checked: `announce_fleet`)
        fleet: Vec::new(),
        fleet_version: 0,
        reached_at: String::new(),
        players_public: cfg.share_positions,
        player_list: Vec::new(),
        local_admin_password: cfg.admin_password.clone(),
        direct: Some(cfg.public_address.clone()).filter(|a| a != "off").unwrap_or_default(),
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
    omsi_lan::server_cfg::set_server_speed_limit_kmh(cfg.speed_limit_kmh);
    log::info!("server: RP speed limit: {}", if cfg.speed_limit_kmh == 0 { "disabled".to_string() } else { format!("{} km/h", cfg.speed_limit_kmh) });
    omsi_lan::admin::allow_player_teleport(cfg.allow_player_teleport);
    // the machine bans: this server's list beside server.cfg, and the owner's (all my sessions)
    omsi_lan::bans::announce_local_hardware_id();
    omsi_lan::bans::require_hardware_id(cfg.require_hardware_id);
    omsi_lan::bans::install(omsi_lan::bans::BanStore::open(Some(cfg.dir.join("bans.json")), cfg.owner_bans.clone().or_else(omsi_lan::bans::BanStore::default_owner_path)));
    crate::real_time::set_server_real(cfg.real_time);
    let _ = SERVER_VEHICLES.set(cfg.vehicles.clone());
    cfg.apply_to_mods();
    crate::logmask::keep_whole(cfg.log_addresses);
    if cfg.log_addresses {
        log::info!("server: log_addresses = 1: the players' internet addresses are logged whole");
    }
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
    args.ai_drive_through_parked = cfg.ai_drive_through_parked;
    if cfg.ai_drive_through_parked {
        log::info!("server: ai_drive_through_parked = 1: the AI drives on through a parked car standing in its lane, as OMSI 2's does");
    }
    args.passengers = cfg.passengers;
    let _ = SERVER_PASSENGERS.set((cfg.passengers, cfg.passenger_density));
    if cfg.public_address.is_empty() {
        omsi_net::addrs::auto_direct(cfg.web_port);
    }
    let limits = cfg.ai_limits.settings_lines();
    if !limits.is_empty() {
        log::info!("server: AI limits of server.cfg over settings.cfg: {}", limits.trim_end().replace('\n', ", "));
        let _ = crate::settings::SERVER_AI_LIMITS.set(limits);
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
    let density = cfg.passenger_density.map(|d| d.to_string()).unwrap_or_else(|| "as settings.cfg".into());
    log::info!("server: you run this server, not openOMSI: you are responsible for the data your players send it (driver names, network addresses, chat, voice) and for what you keep of it, such as logs and bans");
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
    omsi_net::addrs::find_direct(lan, |a| crate::lan::update_server_direct(a));
    crate::lan::update_server_info(players, &lan.world.date, &crate::schedule::hhmm(time.rem_euclid(86400.0)), weather);
    let poses: Vec<&omsi_net::Pose> = lan.peers().filter(|p| p.has_info && p.has_pose).map(|p| &p.pose).collect();
    let list = poses.iter().filter_map(|q| player_info(q, |id| poses.iter().copied().find(|o| o.id == id))).collect();
    crate::lan::update_server_players(list);
}

/// The public server list's publisher while the server runs (`public_server = 1`).
static PUBLIC_LIST: std::sync::Mutex<Option<omsi_lan::public_list::Handle>> = std::sync::Mutex::new(None);

/// List the server in the public list if `server.cfg` says so; the log says why it is not.
pub(crate) fn start_public_list(cfg: &ServerCfg) {
    if !cfg.public_server {
        return;
    }
    match omsi_lan::public_list::start(cfg, &format!("{} ({})", omsi_platform::VERSION, BUILD)) {
        Ok(h) => {
            log::info!("server: public_server = 1: listing in the public server list (checked first; it appears once the list service has reached the web port or the tunnel)");
            if let Ok(mut p) = PUBLIC_LIST.lock() {
                *p = Some(h);
            }
        }
        Err(why) => {
            log::warn!("server: not listed in the public server list: {why}");
            eprintln!("  Not listed in the public server list: {why}");
        }
    }
}
