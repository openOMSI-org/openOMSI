//! Server-authoritative AI type loading and traffic physics without graphics.
//!
//! This is shared with the game adapter to keep AI group weights and vehicle
//! admission rules identical. VehicleType::load_ai reads scripts and driving
//! configuration; it never creates an omsi-render renderer.

use hashbrown::HashMap;
use omsi_sim::ai_traffic::TrafficSim;
use omsi_sim::traffic::{LaneKind, Network};
use omsi_sim::{VehicleType, ai_traffic::setup::RandomTypes};
use std::path::Path;
use std::sync::Arc;

pub fn random_types(
    root: &Path,
    global: &omsi_map::GlobalCfg,
    map_dir: &Path,
    lists: &omsi_map::AiLists,
) -> RandomTypes {
    let mut types = Vec::new();
    let mut groups: Vec<omsi_map::ailists::UnschedGroup> = Vec::new();
    let mut group_uvg: Vec<Option<usize>> = Vec::new();
    let mut uvg_defaults: Vec<i32> = Vec::new();
    // `unsched_trafficdens.txt`: per random group a factor and its density over the day
    // (by day of the week); the global.cfg curve is the fallback of maps without it
    let dens: Vec<omsi_map::ailists::UnschedGroup> =
        omsi_cfg::CfgFile::read(&map_dir.join("unsched_trafficdens.txt"))
            .ok()
            .map(|f| omsi_map::ailists::parse_unsched_trafficdens(&f))
            .unwrap_or_default();
    let group_curves = !dens.is_empty();
    {
        // `unsched_vehgroups.txt` names the groups the random traffic is made of. The
        // other `[aigroup_2]`s exist only for the timetable: on Berlin-Spandau "Pan Am"
        // and "Mi-8 Soviet AF" fly TXL.ttl and Relais.ttl, and taking them into the
        // random pool put airliners on the flight paths at any hour of the day. Its
        // number is the group's default density on the paths without a `[rule]
        // trafficdensity` for it (see `uvg_density`): 0 means only where the paths ask
        // for the group. Taken as "off", Spandau had no trucks and no Trabant at all,
        // though 865 paths ask for the one and 462 around Falkensee for the other.
        // `OMSI_TRAFFIC_ALL_GROUPS=1` lets such groups drive everywhere (and, on a map
        // without the file, every group, not only the default one).
        let all_groups = omsi_cfg::flags::OMSI_TRAFFIC_ALL_GROUPS.is_set();
        let unscheduled: Option<Vec<(String, i32)>> =
            omsi_cfg::CfgFile::read(&map_dir.join("unsched_vehgroups.txt"))
                .ok()
                .map(|f| {
                    omsi_map::ailists::parse_unsched_vehgroups(&f)
                        .into_iter()
                        .map(|(n, c)| (n.trim().to_ascii_lowercase(), c))
                        .collect()
                });
        if let Some(names) = &unscheduled {
            log::info!("random traffic groups (unsched_vehgroups.txt): {names:?}");
            uvg_defaults = names
                .iter()
                .map(|n| if all_groups && n.1 <= 0 { 1 } else { n.1 })
                .collect();
        }
        // Without `unsched_vehgroups.txt` the random traffic is the ailists' default group
        // alone (the first, or the one the `[ailist]` header names): Omsi.exe 0x785f98
        // makes one nameless group then, and a nameless group takes the default group.
        // Taking every group instead, a map whose ailists keep an ambulance (or a bus,
        // or a lorry) in a group of its own had one car in four of that kind (#1025).
        if unscheduled.is_none() && !all_groups {
            if let Some(g) = lists.groups.get(lists.default_group) {
                log::info!(
                    "random traffic: no unsched_vehgroups.txt, only the default AI group {}",
                    g.name
                );
            }
        }
        for (_, g) in lists.groups.iter().enumerate().filter(|(i, g)| {
            !g.is_depot
                && g.hof.is_none()
                && (unscheduled.is_some() || all_groups || *i == lists.default_group)
                && !g
                    .vehicles
                    .iter()
                    .any(|v| v.file.to_ascii_lowercase().ends_with(".zug"))
        }) {
            let lname = g.name.trim().to_ascii_lowercase();
            let uvg = match &unscheduled {
                Some(names) => match names.iter().position(|n| n.0 == lname) {
                    None => continue,
                    Some(u) => {
                        if uvg_defaults.get(u).copied().unwrap_or(0) <= 0 {
                            log::info!(
                                "random traffic group {} drives only where its paths ask for it (unsched_vehgroups.txt)",
                                g.name
                            );
                        }
                        Some(u)
                    }
                },
                None => None,
            };
            let gi = groups.len();
            group_uvg.push(uvg);
            groups.push(
                dens.iter()
                    .find(|d| d.name.trim().eq_ignore_ascii_case(g.name.trim()))
                    .cloned()
                    .unwrap_or(omsi_map::ailists::UnschedGroup {
                        name: g.name.clone(),
                        factor: if group_curves { 0.0 } else { 1.0 },
                        densities: Vec::new(),
                    }),
            );
            for v in &g.vehicles {
                let lower = v.file.to_ascii_lowercase();
                if lower.ends_with(".zug")
                    || lower.contains("trains\\")
                    || lower.contains("trains/")
                {
                    continue;
                }
                let path = omsi_cfg::resolve_path(root, &v.file);
                match VehicleType::load_ai(root, &path) {
                    // a vehicle that declares its modes stays off a map built for others (cars and
                    // lorries declare none: they are scenery on any street)
                    Ok(t)
                        if !t.def.transport.is_empty()
                            && !omsi_cfg::transport::admit(
                                global.transport,
                                t.def.transport,
                                true,
                            )
                            .is_allowed() =>
                    {
                        log::debug!(
                            "AI vehicle {} is made for {}, not for this map",
                            v.file,
                            t.def.transport.describe()
                        );
                    }
                    Ok(t) => {
                        // rail (only as scheduled trains), 3 = aircraft on flight paths
                        let rail = t.def.is_rail();
                        let air =
                            matches!(t.def.kind, omsi_vehicle::vehicle::VehicleKind::Other(3));
                        if rail {
                            log::debug!("AI vehicle {} is rail-bound, not street traffic", v.file);
                        } else {
                            types.push((
                                Arc::new(t),
                                v.weight.max(0.0),
                                if air { LaneKind::Air } else { LaneKind::Street },
                                gi,
                            ));
                        }
                    }
                    Err(e) => log::warn!("AI vehicle {}: {e}", v.file),
                }
            }
        }
    }
    RandomTypes {
        types,
        groups,
        group_curves,
        group_uvg,
        uvg_defaults,
    }
}

/// `OMSI_DEBUG_LANES`: the chosen lanes as the network has them.

/// Prepare the same CPU simulation as the graphical client, given the lanes
/// produced by HostWorld. There are no local camera, visual, audio or texture
/// instances. Road signals, timetable routes and population policy are added
/// by the dedicated simulation runtime as their world data becomes available.
pub fn assemble(
    root: &Path,
    mut network: Network,
    random: RandomTypes,
    density_curve: Vec<(f32, f32)>,
    tiles: Vec<(i32, i32)>,
    target: usize,
) -> TrafficSim {
    network.link(1.5);
    TrafficSim::assemble(
        root,
        network,
        random,
        Vec::new(),
        HashMap::new(),
        (Vec::new(), tiles),
        density_curve,
        (1.0, 0),
        target,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_cpu_simulation_needs_no_render_device_or_player() {
        let random = RandomTypes {
            types: Vec::new(),
            groups: Vec::new(),
            group_curves: false,
            group_uvg: Vec::new(),
            uvg_defaults: Vec::new(),
        };
        let mut sim = assemble(
            Path::new("."),
            Network::default(),
            random,
            Vec::new(),
            Vec::new(),
            0,
        );
        assert_eq!(sim.cars.len(), 0);
        sim.advance(1.0 / 60.0, None);
        sim.advance(1.0 / 60.0, None);
        assert_eq!(sim.cars.len(), 0);
        assert!(sim.net.lanes.is_empty());
    }
}
