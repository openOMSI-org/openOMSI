//! CPU-only whole-pipeline smoke test and offline throughput diagnostic.
//!
//! Unlike the future omsi-server process this NEVER opens a multiplayer
//! listener or accepts clients: road surfaces, traffic lights and schedules
//! are not yet authoritative. Use only with locally installed licensed OMSI
//! content. The output is unpaced simulation throughput, not live server TPS.

use anyhow::{Context, Result, ensure};
use omsi_admin::profiler::Profiler;
use omsi_server::simulation::CpuSimulation;
use omsi_server::{traffic, world::HostWorld};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

struct Options {
    root: PathBuf,
    map: PathBuf,
    date: i32,
    ticks: usize,
    tiles: usize,
}

fn map_path(root: &Path, map: &str) -> PathBuf {
    let path = omsi_cfg::resolve_path(root, map);
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cfg"))
    {
        path
    } else {
        omsi_cfg::resolve_path(&path, "global.cfg")
    }
}

fn parse_options(args: impl IntoIterator<Item = String>) -> Result<Options> {
    let mut args = args.into_iter();
    let root = args
        .next()
        .context("usage: simcheck <OMSI2-root> <map-or-global.cfg> [YYYYMMDD] [ticks] [tiles]")?;
    let map = args
        .next()
        .context("usage: simcheck <OMSI2-root> <map-or-global.cfg> [YYYYMMDD] [ticks] [tiles]")?;
    let date = args.next().unwrap_or_else(|| "19890501".into());
    let date = date.replace('-', "");
    ensure!(
        date.len() == 8 && date.bytes().all(|digit| digit.is_ascii_digit()),
        "date must be YYYYMMDD"
    );
    let ticks: usize = args
        .next()
        .unwrap_or_else(|| "600".into())
        .parse()
        .context("ticks must be an integer")?;
    ensure!((1..=60_000).contains(&ticks), "ticks must be 1..=60000");
    let tiles: usize = args
        .next()
        .unwrap_or_else(|| "16".into())
        .parse()
        .context("tiles must be an integer")?;
    ensure!((1..=64).contains(&tiles), "tiles must be 1..=64");
    ensure!(args.next().is_none(), "unexpected extra argument");
    let root = PathBuf::from(root);
    let map = map_path(&root, &map);
    Ok(Options {
        root,
        map,
        date: date.parse().context("invalid date")?,
        ticks,
        tiles,
    })
}

fn main() -> Result<()> {
    let opts = parse_options(std::env::args().skip(1))?;
    ensure!(opts.root.is_dir(), "OMSI root directory does not exist");
    let started = Instant::now();
    let mut world = HostWorld::open(&opts.root, &opts.map, opts.date)
        .with_context(|| format!("opening {}", opts.map.display()))?;
    ensure!(world.index.tiles_read > 0, "no readable map tiles");
    let tile_keys: Vec<(i32, i32)> = world
        .global
        .tiles
        .iter()
        .take(opts.tiles)
        .map(|tile| (tile.x, tile.y))
        .collect();
    ensure!(!tile_keys.is_empty(), "map contains no tiles");
    let surface_counts = world.install_spline_ground_surfaces(&tile_keys);
    if surface_counts.incomplete() {
        eprintln!(
            "partial CPU wheel surface: {} drivable meshes across {} tiles; {} scenery surfaces, {} invalid splines and {} missing spline definitions remain",
            surface_counts.wheel_meshes,
            surface_counts.target_tiles,
            surface_counts.pending_scenery_surfaces,
            surface_counts.skipped_invalid,
            surface_counts.missing_definitions,
        );
    }
    let stop_counts = world.install_stop_lists(&tile_keys);
    let signal_network = world.network_with_traffic_lights(&tile_keys);
    let signal_count = signal_network.controllers.len();
    let network = signal_network.network;
    ensure!(
        !network.lanes.is_empty(),
        "selected tiles have no drivable or foot paths"
    );
    let colliders = world.install_static_box_collisions(&tile_keys, &network.lanes);
    let lanes = network.lanes.len();
    let facts = Arc::clone(&world.data.snapshot().map);
    let random = traffic::random_types(&opts.root, &world.global, &world.map_dir, &facts.ailists);
    let ai_types = random.types.len();
    // Target zero: this tool validates parsers and the CPU simulation, not
    // actual traffic population, which needs full collisions and timetable.
    let mut traffic = traffic::assemble(
        &opts.root,
        network,
        random,
        world.global.traffic_density_road.clone(),
        tile_keys,
        0,
    );
    traffic.lights = signal_network.controllers;
    traffic.controller_of_object = signal_network.controller_of_object;
    let mut simulation = CpuSimulation::new(&opts.root, Arc::clone(&world.data), traffic, 0);
    if colliders.incomplete() {
        eprintln!(
            "partial static collision: {} bounding boxes, {} mesh obstacles; {} unresolved mesh files, {} tilted placements, {} attachments and {} missing definitions still need CPU placement",
            colliders.static_boxes,
            colliders.static_meshes,
            colliders.pending_meshes,
            colliders.pending_tilted,
            colliders.pending_attachments,
            colliders.missing_definitions
        );
    }
    let load_elapsed = started.elapsed();

    let mut profiler = Profiler::new();
    profiler.start();
    let tick_start = Instant::now();
    for _ in 0..opts.ticks {
        let before = Instant::now();
        simulation.step(1.0 / 60.0);
        simulation.discard_presentation_events_after_publish();
        profiler.tick(before.elapsed());
    }
    let elapsed = tick_start.elapsed();
    profiler.stop();
    let report = profiler.report();
    println!(
        "CPU simcheck (NOT a multiplayer server): map={}, tiles={}, lanes={}, signal_programs={}, bus_stops={}, waiting_places={}, AI types={}, load_s={:.3}, ticks={}, sim_s={:.2}, unpaced_tps={:.1}, avg_tick_ms={:.3}, p95_tick_ms={:.3}, p99_tick_ms={:.3}, simulated_people={}",
        world.global.name,
        world.loaded_tiles(),
        lanes,
        signal_count,
        stop_counts.bus_stops,
        stop_counts.waiting_places,
        ai_types,
        load_elapsed.as_secs_f64(),
        opts.ticks,
        opts.ticks as f64 / 60.0,
        opts.ticks as f64 / elapsed.as_secs_f64().max(1e-9),
        report.avg_tick_ms,
        report.p95_tick_ms,
        report.p99_tick_ms,
        simulation.people.people.len(),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_bounded_and_use_only_local_content() {
        let opts = parse_options(["/omsi", "Maps/Test"].into_iter().map(String::from)).unwrap();
        assert_eq!(opts.ticks, 600);
        assert_eq!(opts.tiles, 16);
        assert!(opts.map.ends_with("Maps/Test/global.cfg"));
    }

    #[test]
    fn invalid_workloads_are_rejected_before_disk_access() {
        for args in [
            vec!["/omsi", "Maps/Test", "19890501", "60001"],
            vec!["/omsi", "Maps/Test", "19890501", "0"],
            vec!["/omsi", "Maps/Test", "19890501", "600", "65"],
            vec!["/omsi", "Maps/Test", "19890501", "600", "0"],
            vec!["/omsi", "Maps/Test", "notadate"],
        ] {
            assert!(parse_options(args.into_iter().map(String::from)).is_err());
        }
    }
}
