//! Strict, local-console-only server.cfg hot reload.
//!
//! Never recreate a socket, remount assets, replace the CPU road network, or
//! change transfer/auth/public exposure policies while peers are connected.
//! A changed restart-only field rejects the *whole* reload (no partial state).

use anyhow::{Context, Result, ensure};
use omsi_lan::server_cfg::ServerCfg;
use std::path::Path;

pub const PARSE_TRAFFIC_MAX: usize = 320;

#[derive(Debug)]
pub struct Reloaded {
    pub config: ServerCfg,
    pub changed: Vec<&'static str>,
}

/// The normal config reader is intentionally tolerant (invalid numbers can
/// fall back to defaults). A live reload must not silently turn a typo into a
/// new active runtime setting, so verify explicitly supported numeric keys.
pub(crate) fn check_hot_values(text: &str) -> Result<()> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "traffic" => {
                let value: usize = value.parse().context("traffic must be an integer")?;
                ensure!(value <= PARSE_TRAFFIC_MAX, "traffic exceeds parser limit");
            }
            "time_speed" => {
                let speed: f64 = value.parse().context("time_speed must be a number")?;
                ensure!(
                    speed.is_finite() && (1.0..=30.0).contains(&speed),
                    "time_speed must be between 1 and 30"
                );
            }
            "passenger_density" => {
                let density: f32 = value
                    .parse()
                    .context("passenger_density must be a number")?;
                ensure!(
                    density.is_finite() && (0.0..=3.0).contains(&density),
                    "passenger_density must be between 0 and 3"
                );
            }
            "speed_limit_kmh" => {
                let speed: u32 = value
                    .parse()
                    .context("speed_limit_kmh must be an integer")?;
                ensure!(
                    matches!(speed, 0 | 60 | 75 | 80),
                    "speed_limit_kmh must be 0, 60, 75 or 80"
                );
            }
            "ai_max_scheduled" => {
                let count: u32 = value
                    .parse()
                    .context("ai_max_scheduled must be an integer")?;
                ensure!(
                    count <= 4096,
                    "ai_max_scheduled exceeds the supported range"
                );
            }
            _ => {}
        }
    }
    Ok(())
}

/// Names only, never values: admin passwords and private proxy configuration
/// must never be written to the console or to the audit log.
/// All fields except the explicit hot-reload allowlist are restart-only.
pub fn restart_required(old: &ServerCfg, next: &ServerCfg) -> Vec<&'static str> {
    let mut changed = Vec::new();
    macro_rules! compare {
        ($($field:ident),+ $(,)?) => {
            $(
                if format!("{:?}", old.$field) != format!("{:?}", next.$field) {
                    changed.push(stringify!($field));
                }
            )+
        };
    }
    compare!(
        name,
        motd,
        map,
        date,
        time,
        weather,
        timetable,
        passengers,
        port,
        web_port,
        max_players,
        tunnel,
        public_address,
        radius,
        icon,
        admin_password,
        moderator_password,
        allow_player_teleport,
        admin_web,
        admin_web_origins,
        admin_forwarding_proxies,
        admin_proxy_header,
        gateway_cf_connecting_ip,
        real_time,
        metar_sync,
        metar_station,
        vehicles,
        share_positions,
        accept_uploads,
        uploads_keep_days,
        parked_buses,
        share_maps,
        voice,
        public_server,
        public_list_url,
        owner_bans,
        require_hardware_id,
        ai_drive_through_parked,
        log_addresses,
        dir
    );
    if old.ai_limits.unsched_factor != next.ai_limits.unsched_factor {
        changed.push("ai_unsched_factor");
    }
    if old.ai_limits.max_parked != next.ai_limits.max_parked {
        changed.push("ai_max_parked");
    }
    if old.ai_limits.max_humans != next.ai_limits.max_humans {
        changed.push("ai_max_humans");
    }
    changed
}

/// Read exactly the same file passed to the process (not an assumed
/// <config-dir>/server.cfg), validate test-mode safety, then compare with the
/// actively applied config. If any step fails the live state stays untouched.
pub fn read_checked(path: &Path, active: &ServerCfg, simulated: bool) -> Result<Reloaded> {
    ensure!(
        path.is_file(),
        "config file no longer exists: {}",
        path.display()
    );
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    check_hot_values(&text)?;
    let next = ServerCfg::load(path, PARSE_TRAFFIC_MAX)?;
    crate::mod_policy::verify_explicit_opt_in(&next, path)?;
    if simulated {
        crate::runtime::allow_simulated_roam(&next)?;
    } else {
        crate::runtime::allow_free_roam(&next)?;
    }
    let restart = restart_required(active, &next);
    ensure!(
        restart.is_empty(),
        "restart required for {} (no changes applied)",
        restart.join(", ")
    );
    let mut changed = Vec::new();
    if active.traffic != next.traffic {
        changed.push("traffic");
    }
    if active.time_speed != next.time_speed {
        changed.push("time_speed");
    }
    if active.passenger_density != next.passenger_density {
        changed.push("passenger_density");
    }
    if active.speed_limit_kmh != next.speed_limit_kmh {
        changed.push("speed_limit_kmh");
    }
    if active.ai_limits.max_scheduled != next.ai_limits.max_scheduled {
        changed.push("ai_max_scheduled");
    }
    Ok(Reloaded {
        config: next,
        changed,
    })
}

/// Atomically apply the validated hot settings on the simulation thread.
/// A lowered random-AI limit removes only excess unscheduled cars; scheduled
/// buses keep running their current trips and obey the new cap on next spawn.
#[allow(clippy::too_many_arguments)]
pub fn apply(
    path: &Path,
    active: &mut ServerCfg,
    simulated: bool,
    simulation: &mut crate::simulation::CpuSimulation,
    transport: &mut crate::transport::HostTransport,
    schedule: &mut Option<crate::timetable::CpuTimetable>,
    rules: &mut omsi_lan::admin::ServerRules<'_>,
    admin: &mut omsi_lan::admin::ServerAdmin,
) -> Result<()> {
    let Reloaded { config, changed } = read_checked(path, active, simulated)?;
    if changed.is_empty() {
        log::info!("server console: /reload: no changes in {}", path.display());
        return Ok(());
    }
    // All validation has completed, and the following operations cannot
    // fail. No listener, auth hub, asset mount or world network is replaced.
    simulation.traffic.target = if simulated { config.traffic } else { 0 };
    simulation.traffic.time_scale = config.time_speed;
    transport.session_mut().clock_speed = config.time_speed;
    simulation.people.density = if simulated && config.passengers {
        config.passenger_density.unwrap_or(1.0).clamp(0.0, 3.0)
    } else {
        0.0
    };
    admin.passenger_density = config.passenger_density;
    rules.traffic_max = if simulated {
        crate::population::MAX_EXPERIMENTAL_TRAFFIC
    } else {
        0
    };
    omsi_lan::server_cfg::set_server_speed_limit_kmh(config.speed_limit_kmh);
    if let Some(timetable) = schedule {
        let max = crate::timetable::scheduled_budget(config.ai_limits.max_scheduled);
        timetable.set_limit(max);
    }
    let mut excess = simulation
        .traffic
        .cars
        .iter()
        .filter(|car| !car.is_bus())
        .count()
        .saturating_sub(if simulated { config.traffic } else { 0 });
    if excess > 0 {
        for index in (0..simulation.traffic.cars.len()).rev() {
            if excess == 0 {
                break;
            }
            if !simulation.traffic.cars[index].is_bus() {
                let car = simulation
                    .traffic
                    .take_off(index, omsi_sim::ai_traffic::control::Removal::Finished);
                simulation.traffic.retired.push(car.id);
                excess -= 1;
            }
        }
    }
    *active = config;
    log::info!(
        "server console: /reload: applied {} from {} (existing timetable buses finish their trips)",
        changed.join(", "),
        path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_and_hot_fields_pass_while_restart_fields_are_rejected() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("native-reload-{}-{nanos}.cfg", std::process::id()));
        let original = include_str!("../../../scripts/server-headless/server-simulation.cfg");
        std::fs::write(&path, original).unwrap();
        let old = ServerCfg::load(&path, PARSE_TRAFFIC_MAX).unwrap();
        assert!(read_checked(&path, &old, true).unwrap().changed.is_empty());
        let updated = original
            .replace("traffic = 8", "traffic = 16")
            .replace("time_speed = 1", "time_speed = 2")
            .replace("passenger_density = 1.0", "passenger_density = 1.5")
            .replace("ai_max_scheduled = 8", "ai_max_scheduled = 10")
            .replace("speed_limit_kmh = 0", "speed_limit_kmh = 60");
        std::fs::write(&path, &updated).unwrap();
        let accepted = read_checked(&path, &old, true).unwrap();
        assert_eq!(accepted.config.traffic, 16);
        assert_eq!(accepted.config.time_speed, 2.0);
        assert_eq!(accepted.changed.len(), 5);
        std::fs::write(&path, updated.replace("share_maps = 0", "share_maps = 1")).unwrap();
        assert!(read_checked(&path, &old, true).is_err());
        std::fs::write(&path, updated.replace("port = 27015", "port = 27016")).unwrap();
        assert!(
            read_checked(&path, &old, true)
                .unwrap_err()
                .to_string()
                .contains("port")
        );
        std::fs::write(&path, updated.replace("traffic = 16", "traffic = garbage")).unwrap();
        assert!(read_checked(&path, &old, true).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(read_checked(&path, &old, true).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn zero_ai_free_roam_cannot_enable_simulation_using_reload() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "native-roam-reload-{}-{nanos}.cfg",
            std::process::id()
        ));
        let original = include_str!("../../../scripts/server-headless/server-test.cfg");
        std::fs::write(&path, original).unwrap();
        let old = ServerCfg::load(&path, PARSE_TRAFFIC_MAX).unwrap();
        let changed = original.replace("traffic = 0", "traffic = 1");
        std::fs::write(&path, changed).unwrap();
        assert!(read_checked(&path, &old, false).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
