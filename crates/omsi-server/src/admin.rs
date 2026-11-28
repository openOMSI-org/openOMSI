//! Dedicated host administrative lifecycle, status publication and public listing.
//!
//! No graphical application, wgpu, scene graph, input or audio dependencies.

use omsi_lan::server_cfg::{ServerCfg, player_info};
use std::path::Path;

fn hhmm(seconds: f64) -> String {
    let s = seconds.rem_euclid(86_400.0) as u32;
    format!("{:02}:{:02}", s / 3600, (s / 60) % 60)
}

/// What of a dedicated server's state stays as `server.cfg` says (the rest is the loop's).
static SERVER_STATE: std::sync::OnceLock<omsi_admin::ServerState> = std::sync::OnceLock::new();

/// The administration's hub (see `omsi_admin`): its records, its sessions, its audit log
/// (`admin-audit.log` beside `server.cfg`, put aside at 1 MB).
pub fn start_administration(cfg: &ServerCfg, path: &Path, traffic_max: usize, version: &str) {
    if !cfg.moderator_password.is_empty() && cfg.moderator_password == cfg.admin_password {
        log::warn!(
            "server.cfg: moderator_password is the admin_password: it gives the admin's role"
        );
    }
    let pw = omsi_admin::Passwords {
        admin: cfg.admin_password.clone(),
        moderator: cfg.moderator_password.clone(),
    };
    let audit = path.parent().map(|d| d.join("admin-audit.log"));
    let hub = std::sync::Arc::new(omsi_admin::Hub::new(
        omsi_admin::Auth::new(pw, Default::default()),
        omsi_admin::Audit::new(audit, 1 << 20),
    ));
    omsi_lan::admin::install_hub(hub.clone());
    omsi_net::ws::believe_cf_connecting_ip(cfg.gateway_cf_connecting_ip);
    // the web panel and its API on the gateway (`admin_web`: who may reach them)
    let web = omsi_admin::WebCfg {
        mode: cfg.admin_web,
        origins: cfg.admin_web_origins.clone(),
        forwarding_proxies: cfg.admin_forwarding_proxies.clone(),
        proxy_header: cfg.admin_proxy_header,
        traffic_max: traffic_max,
    };
    let has_password = !cfg.admin_password.is_empty() || !cfg.moderator_password.is_empty();
    match (web.mode, has_password) {
        (omsi_admin::WebMode::Off, _) => {
            log::info!("server: no web administration (admin_web = off)")
        }
        (_, false) => log::info!("server: no admin_password: the administration is off"),
        (omsi_admin::WebMode::Local, true) => log::info!(
            "server: web administration at http://127.0.0.1:{}/admin/ (from this machine only)",
            cfg.web_port
        ),
        (omsi_admin::WebMode::Remote, true) => log::warn!(
            "server: web administration at http://127.0.0.1:{}/admin/ and over HTTPS through {:?}{}",
            cfg.web_port,
            web.forwarding_proxies,
            if cfg.tunnel {
                " - with the public tunnel too: only the passwords guard it"
            } else {
                ""
            }
        ),
    }
    omsi_lan::gateway::open_admin_api(hub, web);
    let _ = SERVER_STATE.set(omsi_admin::ServerState {
        name: cfg.name.clone(),
        map: cfg.map.clone(),
        version: version.to_owned(),
        max_players: cfg.max_players,
        real_time: cfg.real_time,
        traffic_max: traffic_max,
        timetable: cfg.timetable,
        passenger_density_max: omsi_admin::PASSENGER_DENSITY_MAX,
        ..Default::default()
    });
}

/// The server's state for the admins' panels now (`time`: the clock, s; `world`: as for
/// `GET /status`).
pub fn admin_state(
    time: f64,
    speed: f64,
    weather: &str,
    metar: Option<&str>,
    world: omsi_net::ws::WorldCounts,
    metrics: &omsi_admin::metrics::TickMeter,
    profiler: &omsi_admin::profiler::Profiler,
) -> omsi_admin::ServerState {
    let clock = time.rem_euclid(86_400.0);
    omsi_admin::ServerState {
        time: hhmm(clock),
        clock,
        clock_speed: speed,
        tps: metrics.tps(),
        target_tps: metrics.target(),
        profiler: Some(profiler.report()),
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

/// Every second of a server run: what the status page says (players, time, weather) and
/// who is where (`GET /players`, when `share_positions` is on).
pub fn tick_status(lan: &omsi_net::LanSession, time: f64, weather: &str) {
    let players = lan.peers().filter(|p| p.has_info).count();
    // (the admin's clock shift may take the time below 0 or past midnight: 23:08 had come
    // out as "00:-52")
    omsi_net::addrs::find_direct(lan, |a| omsi_lan::gateway::update_server_direct(a));
    omsi_lan::gateway::update_server_info(
        players,
        &lan.world.date,
        &hhmm(time.rem_euclid(86400.0)),
        weather,
    );
    let parked = omsi_lan::parked::server_free();
    let poses: Vec<&omsi_net::Pose> = lan
        .peers()
        .filter(|p| p.has_info && p.has_pose)
        .map(|p| &p.pose)
        .chain(&parked)
        .collect();
    let list = poses
        .iter()
        .filter_map(|q| player_info(q, |id| poses.iter().copied().find(|o| o.id == id)))
        .collect();
    omsi_lan::gateway::update_server_players(list);
}

/// The public server list's publisher while the server runs (`public_server = 1`).
static PUBLIC_LIST: std::sync::Mutex<Option<omsi_lan::public_list::Handle>> =
    std::sync::Mutex::new(None);

/// List the server in the public list if `server.cfg` says so; the log says why it is not.
pub fn start_public_list(cfg: &ServerCfg, version: &str) {
    if !cfg.public_server {
        return;
    }
    match omsi_lan::public_list::start(cfg, &version.to_owned()) {
        Ok(h) => {
            log::info!(
                "server: public_server = 1: listing in the public server list (checked first; it appears once the list service has reached the web port or the tunnel)"
            );
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

/// Publish player and simulation summaries once per wall-clock interval.
/// All snapshots and authorization remain owned by the authoritative host.
#[allow(clippy::too_many_arguments)]
pub fn publish_status(
    lan: &mut omsi_net::LanSession,
    admin: &mut omsi_lan::admin::ServerAdmin,
    time: f64,
    weather: &str,
    metar: Option<&str>,
    world: omsi_net::ws::WorldCounts,
    metrics: &omsi_admin::metrics::TickMeter,
    profiler: &omsi_admin::profiler::Profiler,
) {
    omsi_lan::server_cfg::enforce_vehicles(lan);
    tick_status(lan, time, weather);
    omsi_lan::gateway::update_server_world(world);
    if let Some(hub) = omsi_lan::admin::hub() {
        let state = admin_state(
            time,
            lan.clock_speed,
            weather,
            metar,
            world,
            metrics,
            profiler,
        );
        omsi_lan::admin::publish(&hub, lan, admin, state);
    }
}

/// The server's status as the web gateway tells it (see `omsi_net::ws::ServerInfo`).
pub fn info_of(cfg: &ServerCfg, version: &str) -> omsi_net::ws::ServerInfo {
    omsi_net::ws::ServerInfo {
        name: cfg.name.clone(),
        motd: cfg.motd.clone(),
        map: cfg.map.clone(),
        players: 0,
        max_players: cfg.max_players,
        version: version.to_string(),
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
        direct: Some(cfg.public_address.clone())
            .filter(|a| a != "off")
            .unwrap_or_default(),
        ..Default::default()
    }
}
