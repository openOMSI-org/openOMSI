//! Dedicated server configuration and security initialization, without game UI.
//! All persistent moderation, transfer and visibility policy is applied here.

use anyhow::Result;
use omsi_lan::server_cfg::{SERVER_VEHICLES, ServerCfg};
use std::path::Path;
use std::sync::OnceLock;

/// The live server settings that the simulation thread reads after boot.
pub static SERVER_PASSENGERS: OnceLock<(bool, Option<f32>)> = OnceLock::new();
pub static SERVER_METAR: OnceLock<Option<String>> = OnceLock::new();
pub static SERVER_ADMIN: OnceLock<(String, String, f64)> = OnceLock::new();

pub struct LaunchPlan {
    /// The exact configured filename, including alternate test config names.
    pub config_path: std::path::PathBuf,
    pub config: ServerCfg,
    pub weather: Option<String>,
    pub ai_limits: String,
}

/// Load and validate server.cfg; install existing bans, sharing policy and admin
/// permissions before any client can join. Presentation is not initialized here.
pub fn prepare(
    path: &Path,
    root: &Path,
    map_override: Option<String>,
    traffic_max: usize,
    version: &str,
    nearest_airport: &dyn Fn(&Path, &str) -> String,
) -> Result<LaunchPlan> {
    let mut cfg = ServerCfg::load(path, traffic_max)?;
    if let Some(map) = map_override.filter(|m| !m.trim().is_empty()) {
        cfg.map = map.trim().replace('\\', "/");
    }
    let _ = SERVER_ADMIN.set((
        cfg.admin_password.clone(),
        cfg.moderator_password.clone(),
        if cfg.real_time { 1.0 } else { cfg.time_speed },
    ));
    crate::admin::start_administration(&cfg, path, traffic_max, version);
    crate::console::start();
    omsi_lan::server_cfg::set_server_speed_limit_kmh(cfg.speed_limit_kmh);
    omsi_lan::admin::allow_player_teleport(cfg.allow_player_teleport);
    omsi_lan::bans::announce_local_hardware_id();
    omsi_lan::bans::require_hardware_id(cfg.require_hardware_id);
    omsi_lan::bans::install(omsi_lan::bans::BanStore::open(
        Some(cfg.dir.join("bans.json")),
        cfg.owner_bans
            .clone()
            .or_else(omsi_lan::bans::BanStore::default_owner_path),
    ));
    // Missing keys in legacy configs default to transfer-on. Require a real,
    // explicit opt-in before applying process-global upload/share policy.
    crate::mod_policy::verify_explicit_opt_in(&cfg, path)?;
    let _ = SERVER_VEHICLES.set(cfg.vehicles.clone());
    cfg.apply_to_mods();
    let _ = SERVER_PASSENGERS.set((cfg.passengers, cfg.passenger_density));
    if cfg.public_address.is_empty() {
        omsi_net::addrs::auto_direct(cfg.web_port);
    }
    let station = cfg.metar_sync.then(|| {
        if cfg.metar_station.is_empty() {
            nearest_airport(root, &cfg.map)
        } else {
            cfg.metar_station.clone()
        }
    });
    let mut weather = cfg.weather.clone();
    if let Some(icao) = station.as_ref() {
        match crate::metar::try_metar(icao).and_then(|w| crate::metar::report_wire(&w)) {
            Some(wire) => weather = Some(wire),
            None => log::warn!("server: no METAR report for {icao} yet; trying again soon"),
        }
        log::info!("server: the weather follows the METAR report of {icao}");
    }
    let _ = SERVER_METAR.set(station);
    let ai_limits = cfg.ai_limits.settings_lines();
    if !ai_limits.is_empty() {
        log::info!(
            "server: AI limits in server.cfg: {}",
            ai_limits.trim_end().replace('\n', ", ")
        );
    }
    if cfg.ai_drive_through_parked {
        log::info!("server: AI drives through blocked parked vehicles (server.cfg)");
    }
    if cfg.log_addresses {
        log::info!("server: log_addresses = 1: player internet addresses are logged whole");
    }
    log::info!(
        "server '{}': map {}, {} at {}, traffic {}, timetable {}, passengers {}, UDP {} / web {}, at most {} players",
        cfg.name,
        cfg.map,
        cfg.date.as_deref().unwrap_or("today"),
        cfg.time,
        cfg.traffic,
        cfg.timetable,
        cfg.passengers,
        cfg.port,
        cfg.web_port,
        cfg.max_players,
    );
    log::info!(
        "server operator: you are responsible for your players' private data, logs and moderation records"
    );
    Ok(LaunchPlan {
        config_path: path.to_path_buf(),
        config: cfg,
        weather,
        ai_limits,
    })
}

/// No web request thread touches these values: the dedicated host owns them.
pub fn server_administration_state(
    server: bool,
    weather: &omsi_content::weather::Weather,
    lan_off: &mut Option<omsi_net::LanSession>,
) -> (
    omsi_lan::admin::ServerAdmin,
    Option<String>,
    std::time::Instant,
    Option<std::sync::mpsc::Receiver<Option<omsi_content::weather::Weather>>>,
    String,
) {
    let mut admin = omsi_lan::admin::ServerAdmin::default();
    let metar = if server {
        SERVER_METAR.get().cloned().flatten()
    } else {
        None
    };
    let due = std::time::Instant::now();
    let weather_name = if weather.path.to_string_lossy().starts_with("metar:") {
        weather.name.clone()
    } else {
        weather
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    };
    if let (true, Some((password, moderator, speed))) = (server, SERVER_ADMIN.get()) {
        admin.password = password.clone();
        admin.moderator_password = moderator.clone();
        if let Some((enabled, density)) = SERVER_PASSENGERS.get() {
            (admin.passengers_on, admin.passenger_density) = (*enabled, *density);
        }
        if let Some(lan) = lan_off.as_mut() {
            lan.clock_speed = *speed;
        }
    }
    (admin, metar, due, None, weather_name)
}
