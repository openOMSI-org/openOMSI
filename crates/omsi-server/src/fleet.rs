//! Authorized server vehicle catalogue and paint fingerprints.
//!
//! No texture upload, GPU mesh, sound engine, UI or renderer is loaded here.
//! Content hashes follow the same authenticated multiplayer transfer policy.

use omsi_lan::server_cfg::ServerCfg;
use std::path::Path;

/// Announce the server's own bus paints without allocating GPU resources. Only
/// buses explicitly in server.cfg are indexed; an unrestricted fleet has no finite
/// advertised catalogue. The launcher remains responsible for authenticated content
/// checks before offering any remote-only vehicle for driving.
fn fleet_paints(cfg: &ServerCfg, root: &Path) -> Vec<omsi_net::ws::FleetPaints> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let total = cfg.vehicles.len().min(256);
    if cfg.vehicles.len() > 256 {
        log::warn!(
            "server fleet: {} buses in `vehicles`: only the first 256 are announced",
            cfg.vehicles.len()
        );
    }
    if total == 0 {
        return Vec::new();
    }
    // (every file of every bus and paint is read and hashed - their hashes are kept for the
    // next start - four buses at a time; the web port is open meanwhile)
    log::info!(
        "server fleet: checking {total} buses and their paints (a first start reads every file; later starts are faster)"
    );
    let (t0, done, failed) = (
        std::time::Instant::now(),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    );
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
        let path = found
            .clone()
            .unwrap_or_else(|| omsi_cfg::resolve_path(root, &rel));
        let t = std::time::Instant::now();
        let vehicle = match omsi_sim::VehicleType::load(root, &path) {
            Ok(v) => v,
            Err(e) => {
                let why = if found.is_none() {
                    " (not found in the content folders)"
                } else {
                    ""
                };
                fail(format!("cannot load {}{why}: {e}", path.display()));
                return None;
            }
        };
        // Use the same resolved scheme names as the authenticated LISTBUS
        // transfer. A server build must not depend on launcher-only crates.
        let loaded = t.elapsed().as_secs_f64();
        let paints: Vec<String> = vehicle
            .paint_schemes
            .iter()
            .take(128)
            .map(|p| p.name.clone())
            .collect();
        let content = match omsi_lan::mods::fleet_fingerprint_loaded(root, &rel, &path, &vehicle) {
            Ok(c) => c,
            Err(e) => {
                fail(format!(
                    "{rel}: its files cannot be offered to the players ({e})"
                ));
                String::new()
            }
        };
        if !content.is_empty() {
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            log::info!(
                "server fleet: {n}/{total}: {rel}: {} paints ({:.1} s, loading the bus {loaded:.1} s)",
                paints.len(),
                t.elapsed().as_secs_f64()
            );
        }
        Some(omsi_net::ws::FleetPaints {
            bus: rel,
            paints,
            content,
        })
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
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap_or_default())
            .collect()
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
        if failed > 0 {
            format!(", {failed} with problems (the lines above say which)")
        } else {
            String::new()
        }
    );
    offers
}

/// Check the server's fleet on a thread of its own and announce it when done: the web port
/// (status page, players' WebSockets, admin) is open meanwhile, telling no fleet as the
/// servers before 0.3.47 did - a first check of a large fleet took minutes, and the web port
/// refused every connection until it was over.
pub fn announce_fleet(cfg: &ServerCfg, root: &Path) {
    if cfg.vehicles.is_empty() || omsi_cfg::flags::OMSI_NO_LAN_MODS.is_set() {
        return;
    }
    let (cfg, root) = (cfg.clone(), root.to_path_buf());
    let spawned = std::thread::Builder::new()
        .name("server fleet".into())
        .spawn(move || {
            let fleet = fleet_paints(&cfg, &root);
            omsi_lan::game::update_server_fleet(fleet, 1);
        });
    if let Err(e) = spawned {
        log::warn!("server fleet: cannot check the fleet: {e}");
    }
}
