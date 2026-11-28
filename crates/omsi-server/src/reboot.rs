//! In-session CPU world restart. This is deliberately NOT a process exit:
//! the authenticated LAN session owns its port, session code and gateway.

use anyhow::{Context, Result, ensure};
use omsi_lan::server_cfg::ServerCfg;
use omsi_map::global::GlobalCfg;
use omsi_sim::SimClock;
use std::collections::HashSet;
use std::path::Path;

use crate::runtime::{INITIAL_TILE_LIMIT, initial_tiles, make_cpu_sim};
use crate::simulation::CpuSimulation;
use crate::streaming::{MAX_POPULATED_TILES, StreamWorker};
use crate::timetable::{CpuTimetable, scheduled_budget};

/// Sim changes that can safely be applied after recreating the CPU world.
/// Network identity, map, time origin, publishing, authentication, content
/// sharing, admin settings and ports are deliberately immutable.
const REBUILDABLE: &[&str] = &["timetable", "passengers", "ai_max_humans"];

pub fn read_config(path: &Path, active: &ServerCfg, simulated: bool) -> Result<ServerCfg> {
    ensure!(path.is_file(), "configuration file is missing");
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    crate::reload::check_hot_values(&text)?;
    let next = ServerCfg::load(path, crate::population::MAX_EXPERIMENTAL_TRAFFIC)?;
    crate::mod_policy::verify_explicit_opt_in(&next, path)?;
    if simulated {
        crate::runtime::allow_simulated_roam(&next)?;
    } else {
        crate::runtime::allow_free_roam(&next)?;
    }
    let blocked: Vec<_> = crate::reload::restart_required(active, &next)
        .into_iter()
        .filter(|field| !REBUILDABLE.contains(field))
        .collect();
    ensure!(
        blocked.is_empty(),
        "full process restart required for {} (LAN code and tunnel unchanged)",
        blocked.join(", ")
    );
    Ok(next)
}

pub struct Prepared {
    pub simulation: CpuSimulation,
    pub timetable: Option<CpuTimetable>,
    pub streamer: StreamWorker,
    pub keys: Vec<(i32, i32)>,
    pub global: GlobalCfg,
}

/// Build the candidate completely before modifying any live simulation state.
/// Failed loads leave the old map, gateway, ports and existing LAN identity intact.
pub fn prepare(
    root: &Path,
    cfg: &ServerCfg,
    clock: &SimClock,
    simulated: bool,
) -> Result<Prepared> {
    prepare_with_active(root, cfg, clock, simulated, &[])
}

/// A Chrono transition must not forget the remote drivers' already streamed
/// neighborhoods. Keep their active tiles first, followed by the normal
/// entrypoint selection, with the same bounded CPU tile budget.
fn selected_tiles(
    global: &GlobalCfg,
    index: &omsi_world::map_index::MapIndex,
    previous: &[(i32, i32)],
) -> Vec<(i32, i32)> {
    let valid: HashSet<_> = global.tiles.iter().map(|t| (t.x, t.y)).collect();
    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    for key in previous
        .iter()
        .copied()
        .chain(initial_tiles(global, index, INITIAL_TILE_LIMIT))
    {
        if valid.contains(&key) && seen.insert(key) {
            selected.push(key);
            if selected.len() == MAX_POPULATED_TILES {
                break;
            }
        }
    }
    selected
}

pub(crate) fn prepare_with_active(
    root: &Path,
    cfg: &ServerCfg,
    clock: &SimClock,
    simulated: bool,
    previous: &[(i32, i32)],
) -> Result<Prepared> {
    let path = omsi_cfg::resolve_path(root, &cfg.map);
    // Chrono overrides use the actual current simulated date after midnight,
    // rather than reinstating the original day from server.cfg.
    let mut map = crate::world::HostWorld::open(root, &path, clock.date_code())
        .with_context(|| format!("reloading CPU map {}", path.display()))?;
    ensure!(
        map.index.tiles_read > 0,
        "restart map has no readable tiles"
    );
    let keys = selected_tiles(&map.global, &map.index, previous);
    ensure!(!keys.is_empty(), "restart map has no selected tiles");
    map.install_spline_ground_surfaces(&keys);
    map.install_stop_lists(&keys);
    let signals = map.network_with_traffic_lights(&keys);
    map.install_static_box_collisions(&keys, &signals.network.lanes);
    ensure!(
        !signals.network.lanes.is_empty(),
        "restart found no usable lanes"
    );
    let mut simulation = make_cpu_sim(root, cfg, &map, &keys, signals, simulated, clock.time);
    let timetable = if simulated && cfg.timetable {
        let next = CpuTimetable::new(
            root,
            &simulation.world,
            clock,
            scheduled_budget(cfg.ai_limits.max_scheduled),
        );
        if cfg.passengers {
            simulation.people.stop_targets = Some(next.sim.stop_targets());
            simulation.people.stop_target_objects = Some(next.sim.stop_target_objects());
            simulation.people.stop_names = Some(next.sim.stop_names());
        }
        Some(next)
    } else {
        None
    };
    let global = map.global.clone();
    let streamer = if simulated {
        StreamWorker::start_populated(map, keys.clone())?
    } else {
        StreamWorker::start(map)?
    };
    Ok(Prepared {
        simulation,
        timetable,
        streamer,
        keys,
        global,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrono_rebuild_keeps_valid_loaded_player_tiles_without_duplicates() {
        use omsi_map::global::MapTileRef;
        let mut global = GlobalCfg::default();
        global.tiles = (0..220)
            .map(|x| MapTileRef {
                x,
                y: 0,
                ..Default::default()
            })
            .collect();
        let index = omsi_world::map_index::MapIndex::default();
        let retained = selected_tiles(&global, &index, &[(210, 0), (211, 0), (210, 0), (999, 0)]);
        assert_eq!(&retained[..2], &[(210, 0), (211, 0)]);
        assert!(!retained.contains(&(999, 0)));
        assert!(retained.contains(&(0, 0)));
        assert_eq!(
            retained.len(),
            retained.iter().copied().collect::<HashSet<_>>().len()
        );
        assert!(retained.len() <= MAX_POPULATED_TILES);
    }

    #[test]
    fn restart_accepts_human_budget_and_timetable_but_not_socket_or_map_change() {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("omsi-restart-{}-{n}.cfg", std::process::id()));
        let original = include_str!("../../../scripts/server-headless/server-simulation.cfg");
        std::fs::write(&path, original).unwrap();
        let current = ServerCfg::load(&path, 320).unwrap();
        let next = format!("{original}\npassengers = 0\ntimetable = 0\nai_max_humans = 2000\n");
        std::fs::write(&path, &next).unwrap();
        let accepted = read_config(&path, &current, true).unwrap();
        assert!(!accepted.passengers);
        assert!(!accepted.timetable);
        assert_eq!(accepted.ai_limits.max_humans, Some(2000));
        let changed = format!("{next}\nport = 27016\n");
        std::fs::write(&path, &changed).unwrap();
        assert!(read_config(&path, &current, true).is_err());
        std::fs::write(&path, format!("{next}\nmap = Maps/Other/global.cfg\n")).unwrap();
        assert!(read_config(&path, &current, true).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
