//! Renderer-free host runtime. It can exercise real multiplayer relay in
//! explicit *experimental free-roam* mode. Full AI, script, collisions and
//! timetable authority are blocked until CPU world parity is verified.

use crate::admin;
use crate::ai_sync::HeadlessTrafficSync;
use crate::chrono::ChronoRefresh;
use crate::context_sync::HeadlessContextSync;
use crate::people_sync::HeadlessPeopleSync;
use crate::population::{CpuTrafficPopulation, MAX_EXPERIMENTAL_TRAFFIC};
use crate::reboot::Prepared;
use crate::simulation::CpuSimulation;
use crate::streaming::{
    MAX_POPULATED_TILES, STREAM_TILE_LIMIT, StreamWorker, apply_free_roam_update,
    apply_populated_update, interest_tiles, populated_new_tiles,
};
use crate::tick::{FIXED_DT, TickPacer};
use crate::timetable::CpuTimetable;
use crate::transport::HostTransport;
use crate::{console, traffic};
use anyhow::{Context, Result, ensure};
use glam::DVec3;
use omsi_admin::{metrics::TickMeter, profiler::Profiler};
use omsi_lan::admin::{ServerAdmin, ServerRules};
use omsi_lan::server_cfg::ServerCfg;
use omsi_net::{WorldInfo, ws::WorldCounts};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const INITIAL_TILE_LIMIT: usize = 64;
const STREAM_REFRESH_TICKS: u64 = 180;

/// Measure named stages only while the operator-enabled profiler is running.
fn profile<T>(profiler: &mut Profiler, name: &'static str, f: impl FnOnce() -> T) -> T {
    let started = profiler.enabled().then(Instant::now);
    let result = f();
    if let Some(started) = started {
        profiler.stage(name, started.elapsed());
    }
    result
}

/// Bound the experimental loaded area around the map's first real entrypoint,
/// not the arbitrary ordering of [map] records in global.cfg. Without this a
/// large map could load 64 distant empty tiles and reject an otherwise valid
/// multiplayer test due to missing road paths.
pub(crate) fn initial_tiles(
    global: &omsi_map::global::GlobalCfg,
    index: &omsi_world::map_index::MapIndex,
    limit: usize,
) -> Vec<(i32, i32)> {
    let tiles: Vec<_> = global.tiles.iter().map(|t| (t.x, t.y)).collect();
    if limit == 0 {
        return Vec::new();
    }
    let mut selected = Vec::new();
    // The graphical server indexes ALL entrypoints, not just the first one.
    // Keep each distinct entrypoint's tile inside the bounded initial CPU
    // simulation before filling the remaining slots with nearby road tiles.
    for entry in &global.entry_points {
        let key = usize::try_from(entry.group)
            .ok()
            .and_then(|i| global.raw_tiles.get(i))
            .copied()
            .or_else(|| {
                crate::world::resolve_entry_point(global, index, entry)
                    .map(|(pos, _)| omsi_map::world_to_tile_local(pos.x, pos.y).0)
            });
        if let Some(key) = key {
            if tiles.contains(&key) && !selected.contains(&key) {
                selected.push(key);
                if selected.len() == limit {
                    return selected;
                }
            }
        }
    }
    if selected.is_empty() {
        return tiles.into_iter().take(limit).collect();
    }
    let anchors = selected.clone();
    let mut remaining: Vec<_> = tiles
        .into_iter()
        .filter(|k| !selected.contains(k))
        .collect();
    remaining.sort_by_key(|&(x, y)| {
        anchors
            .iter()
            .map(|&(ax, ay)| {
                let dx = (i64::from(x) - i64::from(ax)).unsigned_abs() as u128;
                let dy = (i64::from(y) - i64::from(ay)).unsigned_abs() as u128;
                dx * dx + dy * dy
            })
            .min()
            .unwrap_or(u128::MAX)
    });
    selected.extend(remaining.into_iter().take(limit - selected.len()));
    selected
}

/// Parse a configured date without inferring locale or system timezone.
pub fn checked_date(text: &str) -> Result<(i32, String)> {
    let digits = text.replace('-', "");
    ensure!(
        digits.len() == 8 && digits.bytes().all(|x| x.is_ascii_digit()),
        "date must be YYYY-MM-DD"
    );
    let year: u32 = digits[..4].parse()?;
    let month: u32 = digits[4..6].parse()?;
    let day: u32 = digits[6..].parse()?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    ensure!(
        (1900..=2099).contains(&year) && day > 0 && day <= max_day,
        "invalid date"
    );
    Ok((digits.parse()?, format!("{year:04}-{month:02}-{day:02}")))
}

pub fn checked_clock(text: &str) -> Result<f64> {
    let (h, m) = text.split_once(':').context("time must be HH:MM")?;
    let h: u32 = h.parse()?;
    let m: u32 = m.parse()?;
    ensure!(h < 24 && m < 60, "time outside 00:00..23:59");
    Ok(f64::from(h * 3600 + m * 60))
}

/// Avoid pretending that a partially simulated map is a complete host.
pub fn allow_free_roam(cfg: &ServerCfg) -> Result<()> {
    ensure!(
        !cfg.passengers && !cfg.timetable && cfg.traffic == 0,
        "experimental relay requires passengers=0, timetable=0, traffic=0"
    );
    // A headless process has no authoritative parked-vehicle model yet.
    // Maps, approved fleet packages and client uploads are independent,
    // explicit opt-ins validated against the actual configuration file.
    ensure!(
        cfg.parked_buses <= 32 && (cfg.parked_buses == 0 || cfg.accept_uploads),
        "experimental parked_buses requires accept_uploads=1 and no more than 32 buses"
    );
    ensure!(
        cfg.vehicles
            .iter()
            .all(|bus| omsi_net::vehicle_path(bus).is_some()),
        "experimental relay vehicles must contain valid relative vehicle paths"
    );
    // Public discovery uses the normal authenticated gateway and moderated
    // listing. Both remain explicit operator opt-ins via server.cfg.
    // This host has a real clock and an asynchronous METAR relay; neither
    // feature needs a renderer or native vehicle-script authority.
    ensure!(
        !cfg.metar_sync || cfg.metar_station.len() == 4,
        "native METAR requires a four-letter metar_station in server.cfg"
    );
    // Voice/radio is transported as validated, rate-limited opaque LAN packets;
    // the native host already applies server.cfg's policy without audio devices.
    // Self-teleport requests use the same authenticated per-player gate as LAN.
    ensure!(
        cfg.date.is_some() || cfg.real_time,
        "experimental relay requires a date or real_time=1"
    );
    Ok(())
}

/// Opt-in, local-only CPU simulation. Keep the strict transfer, public
/// exposure and moderation restrictions of the free-roam test mode.
pub fn allow_simulated_roam(cfg: &ServerCfg) -> Result<()> {
    let mut base = cfg.clone();
    base.traffic = 0;
    base.timetable = false;
    base.passengers = false;
    allow_free_roam(&base)?;
    ensure!(
        cfg.traffic <= MAX_EXPERIMENTAL_TRAFFIC,
        "experimental CPU simulation supports no more than {MAX_EXPERIMENTAL_TRAFFIC} ordinary AI cars"
    );
    ensure!(
        cfg.ai_limits.max_humans.unwrap_or(64) <= 2_000,
        "experimental CPU simulation supports no more than 2000 humans"
    );
    Ok(())
}

/// Only authenticated peers with finite world coordinates may inform
/// moderation and player teleport commands. Reject invalid coordinate data.
fn peer_positions(net: &HostTransport) -> HashMap<u32, (DVec3, f64)> {
    net.session()
        .peers()
        .filter(|peer| {
            peer.has_pose
                && peer.has_info
                && peer.pose.x.is_finite()
                && peer.pose.y.is_finite()
                && peer.pose.z.is_finite()
                && peer.pose.heading.is_finite()
        })
        .map(|peer| {
            (
                peer.pose.id,
                (
                    DVec3::new(peer.pose.x, peer.pose.y, peer.pose.z),
                    f64::from(peer.pose.heading),
                ),
            )
        })
        .collect()
}

/// Preserve the existing free-roam behavior for regression tests.
pub fn free_roam(
    root: &Path,
    plan: &crate::startup::LaunchPlan,
    max_ticks: Option<u64>,
) -> Result<()> {
    run(root, plan, max_ticks, false)
}

/// Explicit test mode with CPU AI and pedestrians. Still not full server parity.
pub fn simulated_roam(
    root: &Path,
    plan: &crate::startup::LaunchPlan,
    max_ticks: Option<u64>,
) -> Result<()> {
    run(root, plan, max_ticks, true)
}

/// Streaming is deliberately free-roam only. Replacing a populated lane
/// network would invalidate AI routes and pedestrian lane indices.
#[allow(clippy::too_many_arguments)]
fn service_streaming(
    worker: &StreamWorker,
    sim: &mut CpuSimulation,
    net: &HostTransport,
    profiler: &mut Profiler,
    root: &Path,
    global: &omsi_map::GlobalCfg,
    initial: &[(i32, i32)],
    active_keys: &mut Vec<(i32, i32)>,
    busy: &mut bool,
    tick: u64,
    populated: bool,
) -> bool {
    match worker.poll() {
        Ok(Some(update)) => {
            *busy = false;
            if populated {
                let new = profile(profiler, "tile_stream_apply", || {
                    apply_populated_update(sim, update)
                });
                active_keys.extend(new);
            } else {
                *active_keys = profile(profiler, "tile_stream_apply", || {
                    apply_free_roam_update(sim, root, &global.traffic_density_road, update)
                });
            }
        }
        Ok(None) => *busy = false,
        Err(std::sync::mpsc::TryRecvError::Empty) => {}
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            log::error!("native CPU tile stream worker stopped unexpectedly");
            return false;
        }
    }
    if !*busy && tick % STREAM_REFRESH_TICKS == 0 {
        let centers: Vec<(f64, f64)> = net
            .session()
            .peers()
            .filter(|peer| peer.has_pose && peer.has_info)
            .filter_map(|peer| crate::interest::client_position(&peer.pose).map(|p| (p.x, p.y)))
            .collect();
        let desired = interest_tiles(global, initial, &centers, STREAM_TILE_LIMIT);
        let current: BTreeSet<_> = active_keys.iter().copied().collect();
        let next: BTreeSet<_> = desired.iter().copied().collect();
        let needed = if populated {
            let to_load = populated_new_tiles(&desired, &current, MAX_POPULATED_TILES);
            if to_load.is_empty()
                && current.len() >= MAX_POPULATED_TILES
                && desired.iter().any(|key| !current.contains(key))
                && tick % 3600 == 0
            {
                log::warn!(
                    "native populated tiles: retained limit of {} reached; a distant area was not loaded to protect RAM and live AI lane IDs",
                    MAX_POPULATED_TILES
                );
            }
            to_load
        } else if next != current {
            desired
        } else {
            Vec::new()
        };
        if !needed.is_empty() {
            *busy = worker.request(needed);
            if !*busy {
                log::warn!("native tile stream worker disconnected; retaining the previous world");
            }
        }
    }
    true
}

fn report_entrypoints(map: &crate::world::HostWorld, keys: &[(i32, i32)], simulated: bool) {
    let resolved_entries = map
        .global
        .entry_points
        .iter()
        .filter(|entry| crate::world::resolve_entry_point(&map.global, &map.index, entry).is_some())
        .count();
    let entry_tiles = map
        .global
        .entry_points
        .iter()
        .filter_map(|entry| {
            usize::try_from(entry.group)
                .ok()
                .and_then(|i| map.global.raw_tiles.get(i))
                .copied()
        })
        .filter(|key| keys.contains(key))
        .count();
    log::info!(
        "native entrypoints: {} listed, {} resolved, {} covered by initial {} CPU tiles",
        map.global.entry_points.len(),
        resolved_entries,
        entry_tiles,
        keys.len()
    );
    if entry_tiles < map.global.entry_points.len() {
        log::warn!(
            "some entrypoints are outside the initial {}-tile CPU simulation; mode={}",
            INITIAL_TILE_LIMIT,
            if simulated {
                "fixed-map CPU simulation"
            } else {
                "streaming free-roam"
            }
        );
    }
}
pub(crate) fn make_cpu_sim(
    root: &Path,
    cfg: &ServerCfg,
    map: &crate::world::HostWorld,
    keys: &[(i32, i32)],
    signals: crate::signals::SignalNetwork,
    simulated: bool,
    initial_time: f64,
) -> CpuSimulation {
    // Only the explicit simulation mode loads installed AI definitions and
    // script programs. The original free-roam relay retains its zero-AI path.
    // The simulation flag opts in to locally installed AI definitions even
    // with traffic=0. This is essential: /reload may raise traffic from zero
    // without replacing the road network or reopening the server.
    let random = if simulated {
        let facts = map.data.snapshot();
        traffic::random_types(root, &map.global, &map.map_dir, &facts.map.ailists)
    } else {
        omsi_sim::ai_traffic::setup::RandomTypes {
            types: Vec::new(),
            groups: Vec::new(),
            group_curves: false,
            group_uvg: Vec::new(),
            uvg_defaults: Vec::new(),
        }
    };
    let mut ai = traffic::assemble(
        root,
        signals.network,
        random,
        map.global.traffic_density_road.clone(),
        keys.to_vec(),
        if simulated { cfg.traffic } else { 0 },
    );
    ai.lights = signals.controllers;
    ai.controller_of_object = signals.controller_of_object;
    let mut sim = CpuSimulation::new(
        root,
        Arc::clone(&map.data),
        ai,
        if simulated && cfg.passengers {
            cfg.ai_limits.max_humans.unwrap_or(64) as usize
        } else {
            0
        },
    );
    sim.people.players_only = true;
    // Preserve the ordinary player experience, but concentrate the limited
    // headless human pool around actual connected drivers (never the host).
    sim.people.lan_population_boost = 1.75;
    sim.people.density = if simulated && cfg.passengers {
        cfg.passenger_density.unwrap_or(1.0).clamp(0.0, 3.0)
    } else {
        0.0
    };
    sim.traffic.day_time = initial_time;
    sim.traffic.time_scale = cfg.time_speed;

    sim
}

/// Isolate renderer-free world publication from the authoritative 60 Hz
/// simulation runner. The existing LAN traffic table, rotating script
/// context and pedestrian publishers share a single immutable sim snapshot.
#[derive(Default)]
struct NativePublishers {
    signals: crate::lan_world::HeadlessWorldSync,
    traffic: HeadlessTrafficSync,
    context: HeadlessContextSync,
    people: HeadlessPeopleSync,
}

impl NativePublishers {
    fn tick(
        &mut self,
        net: &mut HostTransport,
        sim: &CpuSimulation,
        root: &Path,
        profiler: &mut Profiler,
        allow_client_people: bool,
    ) {
        profile(profiler, "world_sync", || {
            self.signals.tick(net.session_mut(), &sim.traffic, FIXED_DT)
        });
        profile(profiler, "traffic_sync", || {
            self.traffic.update(&sim.traffic, root);
            self.traffic.publish(net.session_mut(), FIXED_DT)
        });
        // The traffic table only describes car poses and appearances:
        // the same scripts and timetable context must accompany each car.
        profile(profiler, "ai_context", || {
            self.context.tick(
                net.session_mut(),
                &sim.traffic,
                &self.traffic.table,
                FIXED_DT,
            )
        });
        profile(profiler, "people_sync", || {
            self.people.tick(
                net.session_mut(),
                &sim.people,
                &self.traffic.table,
                FIXED_DT,
                allow_client_people,
            )
        });
    }
}

/// World commands run on the authoritative tick, never in the network reader.
fn apply_native_admin_actions(
    net: &mut HostTransport,
    admin: &mut ServerAdmin,
    sim: &mut CpuSimulation,
    clock: f64,
    traffic_max: usize,
    simulated: bool,
    hub: Option<&omsi_admin::Hub>,
) {
    if simulated {
        let mut traffic = crate::commands::NativeTraffic(&mut sim.traffic);
        crate::commands::apply_world_jobs(
            net.session_mut(),
            admin,
            Some(&mut traffic),
            clock,
            traffic_max,
            hub,
        );
    } else {
        crate::commands::apply_world_jobs(net.session_mut(), admin, None, clock, 0, hub);
    }
}

/// World commands, reload and the native traffic clock must see the same
/// values before any CPU script advances.
fn sync_native_simulation_settings(
    sim: &mut CpuSimulation,
    net: &HostTransport,
    admin: &ServerAdmin,
    cfg: &ServerCfg,
    simulated: bool,
) {
    sim.traffic.time_scale = net.session().clock_speed;
    sim.people.density = if simulated && cfg.passengers {
        admin
            .passenger_density
            .unwrap_or(cfg.passenger_density.unwrap_or(1.0))
            .clamp(0.0, 3.0)
    } else {
        0.0
    };
}

fn advance_native_ai_population(
    sim: &mut CpuSimulation,
    population: &mut CpuTrafficPopulation,
    clock: &omsi_sim::SimClock,
) -> usize {
    let centers = sim.traffic.lan_centers.clone();
    let target = sim.traffic.target;
    population.tick(&mut sim.traffic, &sim.world, &centers, target, clock, 1.0)
}

fn live_world_counts(sim: &CpuSimulation, simulated: bool, passengers: bool) -> WorldCounts {
    let (cars, buses, dormant, parked) = sim.traffic.counts();
    let (walking, waiting, aboard) = sim.people.counts();
    WorldCounts {
        cars,
        buses,
        dormant,
        parked,
        walking,
        waiting,
        aboard,
        traffic: if simulated { sim.traffic.target } else { 0 },
        passengers: simulated && passengers,
        passenger_density: sim.people.density,
    }
}

/// Low-overhead native diagnostics. These are *CPU ground data estimates*,
/// not whole-process RSS: script heaps, content caches and sockets are separate.
/// Sample only once per 60 seconds to avoid harming a steady 59–60 TPS host.
fn report_native_ground_load(
    sim: &CpuSimulation,
    active_tiles: usize,
    meter: &TickMeter,
    profiler: &Profiler,
) {
    let world = sim.world.peek();
    let terrain_bytes: usize = world
        .ground
        .terrains
        .values()
        .map(|terrain| terrain.heights.capacity() * std::mem::size_of::<f32>())
        .sum();
    let (raster_bytes, drive_bytes) = world
        .ground
        .surfaces
        .values()
        .map(|surface| surface.heap_bytes())
        .fold((0_usize, 0_usize), |(r, d), (next_r, next_d)| {
            (r.saturating_add(next_r), d.saturating_add(next_d))
        });
    let (cars, buses, _, _) = sim.traffic.counts();
    let (walking, waiting, aboard) = sim.people.counts();
    let (player_buses, player_aboard, reported_boarded, reported_alighted) =
        sim.player_passenger_totals();
    log::info!(
        "native player passengers: {} active buses, {} currently aboard (client-reported), {} observed boardings, {} observed alightings since the current CPU world started; GRANT is not counted as boarding",
        player_buses,
        player_aboard,
        reported_boarded,
        reported_alighted,
    );
    for (player_id, aboard, boarded, alighted) in sim.player_passenger_counts() {
        log::info!(
            "native player bus #{player_id}: {aboard} aboard, {boarded} observed boardings and {alighted} observed alightings (player-reported)",
        );
    }
    // Debug-only vertical sanity sample at the reporting cadence (one minute).
    // Never snap network cars to this value: the client/server may be viewing
    // different bridge decks, and a model's origin need not lie on the road.
    if omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
        let mut below = Vec::new();
        let mut below_total = 0usize;
        let mut without_ground = 0usize;
        for car in &sim.traffic.cars {
            let p = car.vehicle.position;
            match world.drawn_ground(p.x, p.y, p.z + 3.0) {
                Some(z) if z - p.z > 1.0 => {
                    below_total += 1;
                    if below.len() < 5 {
                        below.push(format!(
                            "id={} at ({:.1},{:.1},{:.2}), drawn ground {:.2}, difference +{:.2} m",
                            car.id,
                            p.x,
                            p.y,
                            p.z,
                            z,
                            z - p.z
                        ));
                    }
                }
                None => without_ground += 1,
                _ => {}
            }
        }
        if below_total > 0 || without_ground > 0 {
            log::warn!(
                "native AI ground check (diagnostic only): {} sample(s) with surface above the vehicle origin; {} without drawable ground; examples: {}",
                below_total,
                without_ground,
                below.join("; ")
            );
        }
    }
    let profile = profiler.report();
    log::info!(
        "native load: TPS {:.1}/{:.0}, {} active CPU tiles, {} AI cars, {} AI buses, {} pedestrians ({} walking, {} waiting, {} aboard); ground heap est.: {:.1} MiB terrain, {:.1} MiB raster, {:.1} MiB wheel; tick p95 {:.1} ms p99 {:.1} ms (profiling {})",
        meter.tps().unwrap_or_default(),
        meter.target(),
        active_tiles,
        cars,
        buses,
        walking + waiting + aboard,
        walking,
        waiting,
        aboard,
        terrain_bytes as f64 / 1_048_576.0,
        raster_bytes as f64 / 1_048_576.0,
        drive_bytes as f64 / 1_048_576.0,
        profile.p95_tick_ms,
        profile.p99_tick_ms,
        if profiler.enabled() { "on" } else { "off" }
    );
}

/// Reuse the existing server's verified, persisted parked-bus service. This
/// adds no artificial host driver and keeps uploaded vehicles separate from AI.
fn start_verified_parked_buses(
    root: &Path,
    cfg: &ServerCfg,
    map: &crate::world::HostWorld,
    sim: &CpuSimulation,
) {
    if cfg.parked_buses == 0 {
        return;
    }
    let spots: Vec<omsi_lan::parked::Spot> = map
        .global
        .entry_points
        .iter()
        .filter_map(|ep| crate::world::resolve_entry_point(&map.global, &map.index, ep))
        .filter(|(at, heading)| at.is_finite() && heading[0].is_finite())
        .map(|(at, heading)| (at.to_array(), heading[0] as f32))
        .collect();
    // stand_height is on the full WorldData snapshot, which resolves roads,
    // bridges and ground with the same rules as the graphical host.
    let world = sim.world.snapshot();
    omsi_lan::parked::start(root, &cfg.map, &spots, &|x, y, near| {
        world
            .stand_height(x, y, near)
            .or_else(|| world.stand_height(x, y, near + 4.0))
            .or_else(|| world.ground_height(x, y))
    });
    log::info!(
        "native parked buses: limit {}, {} spawn points",
        cfg.parked_buses,
        spots.len()
    );
}

/// Keep native console, gateway and authenticated moderator actions on one
/// authoritative tick before mutating AI/people state.
#[allow(clippy::too_many_arguments)]
fn service_native_admin_tick(
    net: &mut HostTransport,
    admin: &mut ServerAdmin,
    meter: &TickMeter,
    profiler: &mut Profiler,
    sim: &mut CpuSimulation,
    clock: f64,
    rules: &ServerRules<'_>,
    simulated: bool,
) {
    let positions = peer_positions(net);
    // Parked-bus administration needs actual ground/road height, not a
    // permanently empty callback. Snapshot the CPU world (Arc-only) before
    // borrowing the simulation mutably for administrative world actions.
    let world = sim.world.snapshot();
    let hub = omsi_lan::admin::hub().map(|h| &**h);
    crate::commands::service_commands(
        net.session_mut(),
        admin,
        meter,
        profiler,
        &|id| positions.get(&id).copied(),
        rules,
        hub,
        &|x, y, near| {
            world
                .stand_height(x, y, near)
                .or_else(|| world.stand_height(x, y, near + 4.0))
                .or_else(|| world.ground_height(x, y))
        },
        FIXED_DT,
    );
    apply_native_admin_actions(
        net,
        admin,
        sim,
        clock,
        if simulated {
            MAX_EXPERIMENTAL_TRAFFIC
        } else {
            0
        },
        simulated,
        hub,
    );
}

/// Atomically swap a completely staged CPU world while retaining LAN sockets,
/// credentials, publisher history and all connected players. The caller
/// chooses whether old-worker cleanup can block (manual vs. automatic).
#[allow(clippy::too_many_arguments)]
fn install_cpu_world(
    prepared: Prepared,
    sim: &mut CpuSimulation,
    schedule: &mut Option<CpuTimetable>,
    streamer: &mut Option<StreamWorker>,
    keys: &mut Vec<(i32, i32)>,
    active_keys: &mut Vec<(i32, i32)>,
    stream_global: &mut omsi_map::global::GlobalCfg,
    stream_busy: &mut bool,
    population: &mut CpuTrafficPopulation,
) -> Option<StreamWorker> {
    let previous = streamer.replace(prepared.streamer);
    *sim = prepared.simulation;
    *schedule = prepared.timetable;
    *keys = prepared.keys;
    *active_keys = keys.clone();
    *stream_global = prepared.global;
    *stream_busy = false;
    *population = CpuTrafficPopulation::default();
    previous
}

/// Apply local console changes on the host tick. A rejected reload/restart
/// leaves the existing world, joined peers and network services unchanged.
#[allow(clippy::too_many_arguments)]
fn process_console_changes(
    root: &Path,
    config_path: &Path,
    simulated: bool,
    clock: f64,
    calendar: &mut crate::clock::NativeCalendar,
    live_cfg: &mut ServerCfg,
    sim: &mut CpuSimulation,
    net: &mut HostTransport,
    schedule: &mut Option<CpuTimetable>,
    rules: &mut ServerRules<'_>,
    admin_state: &mut ServerAdmin,
    streamer: &mut Option<StreamWorker>,
    keys: &mut Vec<(i32, i32)>,
    active_keys: &mut Vec<(i32, i32)>,
    stream_global: &mut omsi_map::global::GlobalCfg,
    stream_busy: &mut bool,
    population: &mut CpuTrafficPopulation,
) -> bool {
    if console::take_reload_requested() {
        if let Err(error) = crate::reload::apply(
            config_path,
            live_cfg,
            simulated,
            sim,
            net,
            schedule,
            rules,
            admin_state,
        ) {
            log::warn!("server console: /reload refused; settings unchanged: {error}");
        }
    }
    if !console::take_restart_requested() {
        return false;
    }
    // Build the candidate before modifying active simulation or LAN state.
    let result = crate::reboot::read_config(config_path, live_cfg, simulated).and_then(|next| {
        let current_clock = calendar.at(clock + admin_state.shift).clone();
        crate::reboot::prepare(root, &next, &current_clock, simulated)
            .map(|prepared| (next, prepared))
    });
    match result {
        Ok((next, prepared)) => {
            if let Some(old) = install_cpu_world(
                prepared,
                sim,
                schedule,
                streamer,
                keys,
                active_keys,
                stream_global,
                stream_busy,
                population,
            ) {
                old.stop();
            }
            // Keep LAN publisher history: clients must receive GONE/retire
            // deltas for old CPU entities after the swap, not orphaned rows.
            admin_state.passenger_density = next.passenger_density;
            net.session_mut().clock_speed = next.time_speed;
            omsi_lan::server_cfg::set_server_speed_limit_kmh(next.speed_limit_kmh);
            *live_cfg = next;
            log::info!(
                "server console: /restart completed; CPU world and timetable rebuilt, LAN session, code, mods listener and gateway retained"
            );
            true
        }
        Err(error) => {
            log::warn!("server console: /restart refused; live world unchanged: {error}");
            false
        }
    }
}

/// Prepared CPU world for the first native host tick. Keep bootstrap
/// parsing outside the fixed-rate LAN loop and below the graphical layer.
struct CpuBootstrap {
    date_num: i32,
    date: String,
    initial_time: f64,
    map: crate::world::HostWorld,
    keys: Vec<(i32, i32)>,
    sim: CpuSimulation,
    sim_clock: omsi_sim::SimClock,
    calendar: crate::clock::NativeCalendar,
    schedule: Option<CpuTimetable>,
    collisions: crate::collision::CollisionCounts,
}

fn load_initial_cpu_world(root: &Path, cfg: &ServerCfg, simulated: bool) -> Result<CpuBootstrap> {
    let (date_num, date, initial_time, time_label) = if cfg.real_time {
        let now = omsi_career::real_time::now().context("local real-time clock unavailable")?;
        let (_, date) = checked_date(&format!("{:04}-{:02}-{:02}", now.year, now.month, now.day))?;
        let (number, _) = checked_date(&date)?;
        let time = format!(
            "{:02}:{:02}",
            (now.secs / 3600.0) as u32,
            (now.secs as u32 / 60) % 60
        );
        (number, date, now.secs, time)
    } else {
        let (number, date) = checked_date(cfg.date.as_deref().context("missing date")?)?;
        (number, date, checked_clock(&cfg.time)?, cfg.time.clone())
    };
    let global = omsi_cfg::resolve_path(root, &cfg.map);
    let mut map = crate::world::HostWorld::open(root, &global, date_num)
        .with_context(|| format!("loading {}", global.display()))?;
    ensure!(map.index.tiles_read > 0, "no readable map tiles");
    let keys = initial_tiles(&map.global, &map.index, INITIAL_TILE_LIMIT);
    ensure!(!keys.is_empty(), "map has no active tiles");
    report_entrypoints(&map, &keys, simulated);
    let ground_surfaces = map.install_spline_ground_surfaces(&keys);
    log::info!(
        "CPU road surfaces: {} active tiles, {} rasterized spline meshes, {} wheel meshes; {} pending scenery surfaces",
        ground_surfaces.target_tiles,
        ground_surfaces.road_meshes,
        ground_surfaces.wheel_meshes,
        ground_surfaces.pending_scenery_surfaces
    );
    let _stops = map.install_stop_lists(&keys);
    let signals = map.network_with_traffic_lights(&keys);
    let collisions = map.install_static_box_collisions(&keys, &signals.network.lanes);
    ensure!(
        !signals.network.lanes.is_empty(),
        "no drivable or foot paths in selected tiles"
    );
    let mut sim = make_cpu_sim(root, cfg, &map, &keys, signals, simulated, initial_time);
    start_verified_parked_buses(root, cfg, &map, &sim);
    let calendar = crate::clock::NativeCalendar::new(&date, &time_label);
    let sim_clock = calendar.snapshot();
    let schedule = if simulated && cfg.timetable {
        Some(CpuTimetable::new(
            root,
            &sim.world,
            &sim_clock,
            crate::timetable::scheduled_budget(cfg.ai_limits.max_scheduled),
        ))
    } else {
        None
    };
    if let Some(ref timetable) = schedule {
        if cfg.passengers {
            sim.people.stop_targets = Some(timetable.sim.stop_targets());
            sim.people.stop_target_objects = Some(timetable.sim.stop_target_objects());
            sim.people.stop_names = Some(timetable.sim.stop_names());
        }
    }
    Ok(CpuBootstrap {
        date_num,
        date,
        initial_time,
        map,
        keys,
        sim,
        sim_clock,
        calendar,
        schedule,
        collisions,
    })
}

fn run(
    root: &Path,
    plan: &crate::startup::LaunchPlan,
    max_ticks: Option<u64>,
    simulated: bool,
) -> Result<()> {
    let cfg = &plan.config;
    // A separate applied snapshot changes only after full /reload validation.
    let mut live_cfg = cfg.clone();
    let weather = plan.weather.as_deref();
    if simulated {
        allow_simulated_roam(cfg)?;
    } else {
        allow_free_roam(cfg)?;
    }
    crate::mod_policy::verify_explicit_opt_in(cfg, &plan.config_path)?;
    let CpuBootstrap {
        date_num,
        date,
        initial_time,
        map,
        keys,
        sim: mut sim,
        sim_clock: mut sim_clock,
        calendar: mut calendar,
        schedule: mut schedule,
        collisions,
    } = load_initial_cpu_world(root, cfg, simulated)?;
    let world = WorldInfo {
        map: cfg.map.replace('\\', "/"),
        date: date.clone(),
        time: initial_time,
        weather: weather.unwrap_or_default().to_owned(),
        ..Default::default()
    };
    // Use the configured admission path: hardware bans, player limit and
    // protocol discovery must be the same as the legacy host.
    let mut net = HostTransport::bind_configured(plan, world)
        .with_context(|| format!("binding dedicated UDP port {}", cfg.port))?;
    net.session_mut().clock_speed = if cfg.real_time { 1.0 } else { cfg.time_speed };
    // A graphical client ALWAYS requests the host's authenticated TCP
    // manifest after WELCOME, even when it has the same map and buses.
    // Reserving TCP in LanSession::host is not enough: the original server
    // hands that listener to omsi_lan::mods::serve before accepting players.
    // The same authenticated service handles opt-in map manifests, declared
    // server fleet and permitted client bus uploads. Off by default for tests;
    // opt-ins are checked before any player joins. Maps are not sent by default.
    let listener = net
        .session_mut()
        .take_mods_listener()
        .context("native LAN requires the TCP mod listener for normal clients")?;
    let host_key = net
        .session()
        .host_key()
        .context("native LAN requires the session authentication key")?;
    let mods_source = omsi_lan::mods::ModSource {
        root: root.to_path_buf(),
        map: cfg.map.clone(),
        bus: None,
        weather: None,
    };
    let mods_server = omsi_lan::mods::serve(listener, host_key, &mods_source)
        .context("starting authenticated local-only LAN manifest service")?;
    // Do not open the gateway or allow players during CPU map initialization.
    let info = admin::info_of(cfg, env!("CARGO_PKG_VERSION"));
    omsi_lan::gateway::open_public_gateway(net.session(), info, cfg.web_port, cfg.tunnel);
    // Do not advertise or hash an untransferable bus fleet in relay-only mode.
    log::warn!(
        "EXPERIMENTAL headless ONLY: {} static boxes, {} exact collision meshes; {} unresolved mesh files, {} tilted objects, {} attachments; traffic {}, passengers {}, timetable {}",
        collisions.static_boxes,
        collisions.static_meshes,
        collisions.pending_meshes,
        collisions.pending_tilted,
        collisions.pending_attachments,
        if simulated { cfg.traffic } else { 0 },
        simulated && cfg.passengers,
        simulated && cfg.timetable
    );
    eprintln!(
        "Experimental headless {}: AI timetable experimental, no authoritative player-bus parity",
        if simulated {
            "CPU SIMULATION"
        } else {
            "FREE-ROAM relay"
        }
    );
    net.ready();
    admin::start_public_list(cfg, env!("CARGO_PKG_VERSION"));
    if cfg.public_server {
        log::warn!(
            "EXPERIMENTAL public server listed: enable a firewall, configure moderation, and only share redistributable mods"
        );
    }
    let port = net
        .session()
        .local_addr()
        .map_or(cfg.port, |addr| addr.port());
    log::info!("native headless test host ready; UDP port {port}");
    if let Some(code) = net.session().code() {
        log::info!("join code for approved testers: {}", code.encode());
    }
    log::info!(
        "web gateway on TCP port {} (visibility follows public_server, tunnel, and firewall settings)",
        cfg.web_port
    );
    let mut meter = TickMeter::new(60.0);
    let mut profiler = Profiler::new();
    let mut admin_state = ServerAdmin::default();
    admin_state.password = cfg.admin_password.clone();
    admin_state.moderator_password = cfg.moderator_password.clone();
    let mut rules = ServerRules {
        real_time: cfg.real_time,
        metar: cfg.metar_sync,
        traffic_max: if simulated {
            MAX_EXPERIMENTAL_TRAFFIC
        } else {
            0
        },
        notice_ok: &|notice| notice.len() <= 2048,
    };
    let mut pacer = TickPacer::new(Instant::now());
    let mut publishers = NativePublishers::default();
    let mut population = CpuTrafficPopulation::default();
    let mut chrono_refresh = ChronoRefresh::new(map.map_dir.clone(), date_num);
    let mut stream_global = map.global.clone();
    // Keep already referenced lane and route identities stable during tile streaming.
    let mut streamer = Some(if simulated {
        StreamWorker::start_populated(map, keys.clone())
            .context("starting populated CPU tile stream worker")?
    } else {
        StreamWorker::start(map).context("starting CPU tile stream worker")?
    });
    let mut stream_busy = false;
    let mut keys = keys;
    let mut active_keys = keys.clone();
    let mut clock = initial_time;
    let mut weather_state = crate::weather::NativeWeather::new(
        cfg.metar_sync.then(|| cfg.metar_station.clone()),
        weather.unwrap_or_default(),
    );
    let mut n = 0_u64;
    let mut report = Instant::now();
    loop {
        if console::stop_requested() || max_ticks.is_some_and(|limit| n >= limit) {
            break;
        }
        let started = Instant::now();
        service_native_admin_tick(
            &mut net,
            &mut admin_state,
            &meter,
            &mut profiler,
            &mut sim,
            clock,
            &rules,
            simulated,
        );
        let manually_restarted = process_console_changes(
            root,
            &plan.config_path,
            simulated,
            clock,
            &mut calendar,
            &mut live_cfg,
            &mut sim,
            &mut net,
            &mut schedule,
            &mut rules,
            &mut admin_state,
            &mut streamer,
            &mut keys,
            &mut active_keys,
            &mut stream_global,
            &mut stream_busy,
            &mut population,
        );
        if manually_restarted {
            let current_date = calendar.at(clock + admin_state.shift).date_code();
            chrono_refresh.manual_restart(current_date);
        }
        weather_state.tick(net.session_mut());
        sync_native_simulation_settings(&mut sim, &net, &admin_state, &live_cfg, simulated);
        clock += f64::from(FIXED_DT) * net.session().clock_speed;
        if live_cfg.real_time && n % 60 == 0 {
            crate::clock::correct_real_time(&mut calendar, clock, &mut admin_state);
        }
        let authoritative = clock + admin_state.shift;
        // Date and time-of-day on the wire must roll over with the world clock;
        // ScheduleSim still sees the continuous elapsed-day timeline.
        sim_clock = calendar.at(authoritative).clone();
        if let Some(prepared) =
            chrono_refresh.tick(root, &live_cfg, &sim_clock, simulated, &active_keys)
        {
            if let Some(old) = install_cpu_world(
                prepared,
                &mut sim,
                &mut schedule,
                &mut streamer,
                &mut keys,
                &mut active_keys,
                &mut stream_global,
                &mut stream_busy,
                &mut population,
            ) {
                crate::chrono::retire_worker(old);
            }
            chrono_refresh.installed();
            log::info!(
                "native Chrono: installed date {} CPU map, AI lists and timetable; LAN session and connected players retained",
                sim_clock.date_code()
            );
        }
        let (day, month) = sim_clock.day_month();
        net.session_mut().set_clock(
            &format!("{:04}-{month:02}-{day:02}", sim_clock.year),
            sim_clock.time,
        );
        profile(&mut profiler, "network", || net.tick(FIXED_DT));
        if let Some(ref worker) = streamer {
            if !service_streaming(
                worker,
                &mut sim,
                &net,
                &mut profiler,
                root,
                &stream_global,
                &keys,
                &mut active_keys,
                &mut stream_busy,
                n,
                simulated,
            ) {
                break;
            }
        }
        profile(&mut profiler, "interest", || {
            sim.sync_host_clients(net.session_mut())
        });
        if simulated && n % 60 == 0 {
            profile(&mut profiler, "ai_population", || {
                let viewers = sim.traffic.lan_centers.clone();
                CpuTrafficPopulation::recycle_distant(&mut sim.traffic, &viewers);
                advance_native_ai_population(&mut sim, &mut population, &sim_clock)
            });
        }
        // Match LAN clock shifts; TrafficSim advances day_time inside step.
        sim.traffic.day_time = authoritative - f64::from(FIXED_DT) * sim.traffic.time_scale;
        profile(&mut profiler, "simulation", || sim.step(FIXED_DT));
        if let Some(ref mut timetable) = schedule {
            if n % 60 == 0 {
                profile(&mut profiler, "timetable", || {
                    timetable.tick(root, &sim.world, &mut sim.traffic, &sim_clock, 1.0)
                });
            }
            // Keep passenger stop demand in sync with Chrono when the
            // simulation clock is accelerated by time_speed.
            if live_cfg.passengers && n % 60 == 0 {
                sim.people.due_dests = Some(timetable.sim.due_destinations(authoritative));
                sim.people.due_at = authoritative;
            }
        }
        publishers.tick(
            &mut net,
            &sim,
            root,
            &mut profiler,
            simulated && live_cfg.passengers,
        );
        sim.discard_presentation_events_after_publish();
        profiler.tick(started.elapsed());
        meter.completed(Instant::now());
        n += 1;
        if n % 3600 == 0 {
            report_native_ground_load(&sim, active_keys.len(), &meter, &profiler);
        }
        if report.elapsed() >= Duration::from_secs(1) {
            let counts = live_world_counts(&sim, simulated, live_cfg.passengers);
            admin::publish_status(
                net.session_mut(),
                &mut admin_state,
                authoritative,
                weather_state.name(),
                weather_state.station(),
                counts,
                &meter,
                &profiler,
            );
            report = Instant::now();
        }
        std::thread::sleep(pacer.after_tick(Instant::now()).wait);
    }
    net.session_mut().leave();
    if let Some(streamer) = streamer {
        streamer.stop();
    }
    drop(mods_server);
    omsi_lan::gateway::close_public_gateway();
    eprintln!("Dedicated relay stopped cleanly after {n} ticks");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiling_is_off_by_default_and_records_cpu_stages_only_after_opt_in() {
        let mut profiler = Profiler::new();
        assert_eq!(profile(&mut profiler, "network", || 7_u32), 7);
        assert!(profiler.report().stages.is_empty());
        profiler.start();
        assert_eq!(profile(&mut profiler, "simulation", || 9_u32), 9);
        assert_eq!(profiler.report().stages[0].name, "simulation");
        profiler.stop();
        profile(&mut profiler, "network", || ());
        assert_eq!(profiler.report().stages.len(), 1);
    }

    #[test]
    fn experimental_initial_tiles_start_near_the_actual_entrypoint() {
        use omsi_map::global::{EntryPoint, MapTileRef};
        let mut global = omsi_map::global::GlobalCfg::default();
        global.tiles = (0..100)
            .map(|i| MapTileRef {
                x: i,
                y: 0,
                index: i as usize,
                ..Default::default()
            })
            .collect();
        global.raw_tiles = (0..100).map(|x| (x, 0)).collect();
        global.entry_points.push(EntryPoint {
            group: 80,
            object_id: 95,
            ..Default::default()
        });
        let index = omsi_world::map_index::MapIndex::default();
        let selected = initial_tiles(&global, &index, 5);
        assert_eq!(selected.len(), 5);
        assert_eq!(selected[0], (80, 0));
        assert!(selected.iter().all(|&(x, y)| (x - 80).abs() <= 3 && y == 0));
    }

    #[test]
    fn initial_cpu_tiles_include_multiple_distant_entrypoints() {
        use omsi_map::global::{EntryPoint, MapTileRef};
        let mut global = omsi_map::global::GlobalCfg::default();
        global.tiles = (0..100)
            .map(|x| MapTileRef {
                x,
                index: x as usize,
                ..Default::default()
            })
            .collect();
        global.raw_tiles = (0..100).map(|x| (x, 0)).collect();
        for group in [2, 89, 42] {
            global.entry_points.push(EntryPoint {
                group,
                ..Default::default()
            });
        }
        let keys = initial_tiles(&global, &omsi_world::map_index::MapIndex::default(), 8);
        assert_eq!(keys.len(), 8);
        assert_eq!(&keys[..3], &[(2, 0), (89, 0), (42, 0)]);
        assert!(keys.contains(&(3, 0)));
        assert!(keys.contains(&(88, 0)));
    }

    #[test]
    fn tiles_keep_map_order_when_no_entrypoint_exists() {
        let mut global = omsi_map::global::GlobalCfg::default();
        global.tiles = vec![
            omsi_map::global::MapTileRef {
                x: 10,
                ..Default::default()
            },
            omsi_map::global::MapTileRef {
                x: 20,
                ..Default::default()
            },
        ];
        assert_eq!(
            initial_tiles(&global, &omsi_world::map_index::MapIndex::default(), 1),
            vec![(10, 0)]
        );
    }

    #[test]
    fn invalid_time_and_date_cannot_start_a_session() {
        for t in ["24:00", "08:60", "8:xx", "nan", "01:02:03"] {
            assert!(checked_clock(t).is_err());
        }
        assert_eq!(checked_clock("08:30").unwrap(), 30_600.0);
        assert_eq!(
            checked_date("1989-05-30").unwrap(),
            (19890530, "1989-05-30".into())
        );
        assert!(checked_date("2050-99-01").is_err());
        assert!(checked_date("2026-02-29").is_err());
        assert!(checked_date("2024-02-29").is_ok());
        assert!(checked_date("2024-04-31").is_err());
    }
    #[test]
    fn experimental_admission_keeps_public_and_parked_policy_private() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("omsi-headless-{}-{nanos}.cfg", std::process::id()));
        // Package defaults must pass the same admission guard as production
        // runtime parsing, preventing accidental unsafe test ZIPs.
        let config = include_str!("../../../scripts/server-headless/server-test.cfg");
        std::fs::write(&path, config).unwrap();
        let mut cfg = ServerCfg::load(&path, MAX_EXPERIMENTAL_TRAFFIC).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(allow_free_roam(&cfg).is_ok());
        cfg.accept_uploads = true;
        assert!(allow_free_roam(&cfg).is_ok());
        cfg.accept_uploads = false;
        cfg.share_maps = true;
        assert!(allow_free_roam(&cfg).is_ok());
        cfg.share_maps = false;
        cfg.parked_buses = 1;
        assert!(allow_free_roam(&cfg).is_err());
        cfg.parked_buses = 0;
        cfg.public_address.clear();
        assert!(allow_free_roam(&cfg).is_ok());
        cfg.public_address = "off".into();
        cfg.tunnel = true;
        assert!(allow_free_roam(&cfg).is_ok());
        cfg.public_server = true;
        assert!(allow_free_roam(&cfg).is_ok());
        cfg.public_server = false;
        cfg.tunnel = false;
        cfg.voice.enabled = true;
        cfg.voice.radio = true;
        assert!(allow_free_roam(&cfg).is_ok());
        cfg.voice.enabled = false;
        cfg.voice.radio = false;
        cfg.allow_player_teleport = true;
        assert!(allow_free_roam(&cfg).is_ok());
        cfg.allow_player_teleport = false;
        cfg.vehicles.push("Vehicles/Test/Bus.bus".into());
        assert!(allow_free_roam(&cfg).is_ok());
    }
    #[test]
    fn experimental_traffic_config_accepts_512_and_caps_larger_requests() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "omsi-traffic-limit-{}-{nanos}.cfg",
            std::process::id()
        ));
        let sample = include_str!("../../../scripts/server-headless/server-simulation.cfg");
        std::fs::write(&path, sample.replace("traffic = 8", "traffic = 512")).unwrap();
        let cfg = ServerCfg::load(&path, MAX_EXPERIMENTAL_TRAFFIC).unwrap();
        assert_eq!(cfg.traffic, 512);
        assert!(allow_simulated_roam(&cfg).is_ok());

        std::fs::write(&path, sample.replace("traffic = 8", "traffic = 513")).unwrap();
        let cfg = ServerCfg::load(&path, MAX_EXPERIMENTAL_TRAFFIC).unwrap();
        assert_eq!(cfg.traffic, 512);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn experimental_simulation_admission_is_explicit_and_still_private() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("omsi-native-ai-{}-{nanos}.cfg", std::process::id()));
        let sample = include_str!("../../../scripts/server-headless/server-simulation.cfg");
        std::fs::write(&path, sample).unwrap();
        let mut cfg = ServerCfg::load(&path, 256).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(allow_simulated_roam(&cfg).is_ok());
        assert!(allow_free_roam(&cfg).is_err());
        cfg.traffic = MAX_EXPERIMENTAL_TRAFFIC + 1;
        assert!(allow_simulated_roam(&cfg).is_err());
        cfg.traffic = 8;
        cfg.ai_limits.max_humans = Some(2_000);
        assert!(allow_simulated_roam(&cfg).is_ok());
        cfg.ai_limits.max_humans = Some(2_001);
        assert!(allow_simulated_roam(&cfg).is_err());
        cfg.ai_limits.max_humans = Some(64);
        cfg.accept_uploads = true;
        assert!(allow_simulated_roam(&cfg).is_ok());
        cfg.accept_uploads = false;
        cfg.share_maps = true;
        assert!(allow_simulated_roam(&cfg).is_ok());
        cfg.share_maps = false;
        cfg.parked_buses = 1;
        assert!(allow_simulated_roam(&cfg).is_err());
        cfg.parked_buses = 0;
        cfg.public_server = true;
        assert!(allow_simulated_roam(&cfg).is_ok());
        cfg.public_server = false;
        cfg.vehicles.push("Vehicles/Test/vehicle.bus".into());
        assert!(allow_simulated_roam(&cfg).is_ok());
    }
}
