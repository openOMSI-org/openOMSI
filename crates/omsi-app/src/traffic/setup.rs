//! Making the traffic: the network from the tiles, the AI vehicle types of the map.

use super::*;
use crate::scene::World;
use omsi_sim::handover::Inbox;
use omsi_sim::world_data::{TrafficArrival, TrafficArrivals};

/// Start a vehicle of type `ty` once and throw it away: its `{init}` and its displays read
/// the files they need (depot data, fonts) into the caches before the first real one of the
/// type comes along in the middle of a drive.
pub fn warm_up(world: &World, ty: &Arc<VehicleType>, hof: Option<Arc<omsi_vehicle::Hof>>) {
    let t = std::time::Instant::now();
    let mut host = omsi_sim::VehicleHost::new(omsi_sim::SimClock::default());
    host.hof = hof;
    host.font_lib = Some(world.fonts.clone());
    let mut vehicle = VehicleInstance::new(ty.clone(), host);
    vehicle.init_text_textures(&mut world.fonts.lock(), &|p| {
        omsi_texture::decode_file(p)
            .ok()
            .map(|i| (i.width, i.height, i.rgba))
    });
    if omsi_cfg::flags::OMSI_PROFILE.is_set() {
        log::info!(
            "  first start of {}: {:.1} ms",
            ty.def.path.display(),
            t.elapsed().as_secs_f64() * 1000.0
        );
    }
}

impl Traffic {
    /// Build the network from the lanes collected by `World::build_scene` and load the AI
    /// car types of the map's `ailists.cfg` (the `[aigroup_2]` groups that are not depots).
    pub fn new(root: &Path, world: &World, target: usize) -> Result<Traffic> {
        let (lanes, parked_cars, lane_tiles, new_lights) = take_from_tiles(&world.traffic_inbox);
        let mut net = Network {
            lanes,
            // (`[lht]`: priority to the left, passing on the right, keeping left)
            left_hand: world.global.left_hand_traffic,
            ..Default::default()
        };
        if net.left_hand {
            log::info!("traffic: the map drives on the left");
        }
        net.link(1.5);
        // which rail lane is tram track, railway or metro, and how fast its curves allow
        let rail_lanes = omsi_sim::rail::classify(&mut net, &{
            let mut cfg = omsi_sim::rail::RailMapConfig::load(&world.map_dir);
            cfg.add_transit(&world.transit);
            cfg
        });
        if rail_lanes > 0 {
            let of = |c: omsi_sim::rail::TrackClass| {
                net.lanes
                    .iter()
                    .filter(|l| l.track.is_some_and(|t| t.class == c))
                    .count()
            };
            log::info!(
                "rail: {rail_lanes} rail lanes ({} in the street, {} railway)",
                of(omsi_sim::rail::TrackClass::Street),
                of(omsi_sim::rail::TrackClass::Mainline)
            );
            let mut names: Vec<(&str, usize)> = Vec::new();
            for l in net.lanes.iter().filter(|l| l.track.is_some()) {
                match names.iter_mut().find(|(n, _)| *n == l.name) {
                    Some((_, c)) => *c += 1,
                    None => names.push((&l.name, 1)),
                }
            }
            names.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
            log::info!(
                "rail: lanes by file: {}",
                names
                    .iter()
                    .take(16)
                    .map(|(n, c)| format!("{n} x{c}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        let RandomTypes {
            types,
            groups,
            group_curves,
            group_uvg,
            uvg_defaults,
        } = omsi_server::traffic::random_types(
            root,
            &world.global,
            &world.map_dir,
            &world.map().ailists,
        );
        // No buses in the random road traffic: the depot groups of `ailists.cfg` are the
        // fleet the *timetable* drives, and OMSI puts a bus on a street only because a trip
        // of the map's TTData runs there. Mixing the depot fleet into the random pool put
        // the map's one bus type on every road of the map - on Grundorf that is a single
        // articulated GN92, which is why it seemed to be a type of our own choosing.
        log_debug_lanes(&net);
        if omsi_cfg::flags::OMSI_DEBUG_WHEELS.is_set() {
            for (t, ..) in &types {
                let v = VehicleInstance::new(
                    t.clone(),
                    omsi_sim::VehicleHost::new(omsi_sim::SimClock::default()),
                );
                for line in v.wheel_pivot_report() {
                    log::info!("wheel pivot: {line}");
                }
            }
        }
        let controller_of_object: hashbrown::HashMap<i64, usize> =
            new_lights.iter().map(|(i, o, _)| (*o, *i)).collect();
        let lights: Vec<omsi_sim::traffic::TrafficLightController> =
            new_lights.into_iter().map(|(_, _, c)| c).collect();
        let turning = net.lanes.iter().filter(|l| l.turn != 0).count();
        let with_side = net
            .lanes
            .iter()
            .filter(|l| l.left.is_some() || l.right.is_some())
            .count();
        let turn_lanes = net
            .lanes
            .iter()
            .filter(|l| {
                (l.left.is_some() || l.right.is_some())
                    && l.next.iter().any(|&n| net.lanes[n].turn != 0)
            })
            .count();
        let closed = net
            .lanes
            .iter()
            .filter(|l| l.no_cars || l.density <= 0.0)
            .count();
        let closed_junctions = net
            .lanes
            .iter()
            .filter(|l| (l.no_cars || l.density <= 0.0) && l.source == 2)
            .count();
        let quiet = net
            .lanes
            .iter()
            .filter(|l| l.density < 1.0 && l.density > 0.0)
            .count();
        let prio = net
            .lanes
            .iter()
            .filter(|l| (l.priority - omsi_sim::traffic::DEFAULT_PRIORITY).abs() > 0.5)
            .count();
        log::info!(
            "traffic: {} lanes ({turning} turning, {with_side} with a neighbour, {turn_lanes} where a turn lane applies, {closed} closed to cars of which {closed_junctions} are junctions, {quiet} with less traffic by [rule], {prio} with a [rule] priority), {} AI vehicle types in {} groups, {} light programs, {} lamps",
            net.lanes.len(),
            types.len(),
            groups.len(),
            lights.len(),
            world.light_objects.lock().len()
        );
        // lights on paths a car reaches from a lit path of the same crossing (they hold
        // only a car that comes into the crossing there, see `light_at_entry`)
        let inner_lights = (0..net.lanes.len())
            .filter(|&l| {
                let lane = &net.lanes[l];
                lane.traffic_light.is_some()
                    && lane.source == 2
                    && net.prev.get(l).is_some_and(|ps| {
                        ps.iter().any(|&p| {
                            let q = &net.lanes[p];
                            q.source == 2
                                && q.traffic_light.is_some()
                                && q.key.map(|k| (k.tile, k.id)) == lane.key.map(|k| (k.tile, k.id))
                        })
                    })
            })
            .count();
        log::info!(
            "traffic: {inner_lights} lit paths inside crossings (a car already in the crossing is not held there again)"
        );
        let density_curve = world.global.traffic_density_road.clone();
        let unsched_factor = crate::settings::Settings::load().ai_unsched_factor;
        let max_scheduled = crate::settings::Settings::load().ai_max_scheduled;
        let random = RandomTypes {
            types,
            groups,
            group_curves,
            group_uvg,
            uvg_defaults,
        };
        let mut sim = TrafficSim::assemble(
            root,
            net,
            random,
            lights,
            controller_of_object,
            (parked_cars, lane_tiles),
            density_curve,
            (unsched_factor, max_scheduled),
            target,
        );
        // (#1773: off, an early timetable bus waits at every stop, as in OMSI)
        let settings = crate::settings::Settings::load();
        sim.timed_waits_only = settings.ai_wait_timed_stops_only;
        sim.reaction_factor = settings.ai_reaction;
        Ok(Traffic::with_sim(sim, root))
    }

    /// Take in what the tiles loaded since the last call handed over (`inbox`): their lanes
    /// (linked into the network, whose existing indices stay valid), their parked cars
    /// (sorted onto the lanes once those are in) and the light programs of their crossings.
    pub fn add_tiles(&mut self, inbox: &Inbox<TrafficArrival>) -> usize {
        let (new, parked_cars, tiles, new_lights) = take_from_tiles(inbox);
        let n = new.len();
        let mut added = self.sim.net.lanes.len()..self.sim.net.lanes.len();
        if n > 0 {
            added = self.sim.net.extend(new, omsi_sim::traffic::LINK_TOLERANCE);
            self.sim.set_spawn_densities(added.clone());
            self.sim.street_weight += self.sim.net.lanes[added.clone()]
                .iter()
                .filter_map(street_lane_weight)
                .sum::<f64>();
            log::debug!(
                "traffic: {} lanes added ({} in all)",
                added.len(),
                self.sim.net.lanes.len()
            );
        }
        if !tiles.is_empty() {
            self.sim.lane_tiles.extend(tiles);
            self.sim.lanes_generation += 1;
        }
        self.sort_parked(parked_cars, added);
        // new crossings bring their light programs; the running ones keep their clocks
        // (a program is never taken away again: the numbers only grow)
        for (index, object, c) in new_lights {
            debug_assert_eq!(
                index,
                self.sim.lights.len(),
                "light programs arrive in the order of their numbers"
            );
            self.sim.light_prev.push(vec![-100; c.lights.len()]);
            self.sim.lights.push(c);
            self.sim.controller_of_object.insert(object, index);
        }
        n
    }
}

/// What the tiles placed since the last call handed to the traffic: their lanes, their
/// parked cars, which tiles they were (see `TrafficArrival::Tile`) and the new light programs.
#[allow(clippy::type_complexity)]
pub(super) fn take_from_tiles(
    inbox: &Inbox<TrafficArrival>,
) -> (
    Vec<omsi_sim::traffic::Lane>,
    Vec<(DVec3, f64)>,
    Vec<(i32, i32)>,
    Vec<(usize, i64, omsi_sim::traffic::TrafficLightController)>,
) {
    let TrafficArrivals {
        lanes: mut new,
        parked,
        tiles,
        lights,
    } = TrafficArrivals::gather(inbox.take());
    // The tiles are read in parallel and hand in their lanes in the order they finish:
    // sorted by their map identity, the lanes a set of tiles brings are numbered alike in
    // every run, and so is the random traffic drawn from them (a run can be repeated to
    // look at what a car did).
    new.sort_by(|a, b| {
        let first = |l: &omsi_sim::traffic::Lane| {
            l.points
                .first()
                .map(|p| (p.x.to_bits(), p.y.to_bits()))
                .unwrap_or((0, 0))
        };
        (
            a.key.map(|k| (k.tile, k.id, k.path)),
            a.reversed,
            a.source,
            first(a),
        )
            .cmp(&(
                b.key.map(|k| (k.tile, k.id, k.path)),
                b.reversed,
                b.source,
                first(b),
            ))
    });
    (new, parked, tiles, lights)
}

/// Load the AI car types of the map's `ailists.cfg` that make its random traffic.
fn log_debug_lanes(net: &Network) {
    if let Some(list) = omsi_cfg::flags::OMSI_DEBUG_LANES.var() {
        // lane indices, or `at:x,y,r` for the street lanes passing within r m of a point
        // (the indices change from run to run on a map whose tiles load in parallel)
        let chosen: Vec<usize> = match list.strip_prefix("at:") {
            Some(rest) => {
                let v: Vec<f64> = rest
                    .split(',')
                    .filter_map(|x| x.trim().parse().ok())
                    .collect();
                let (p, r) = (
                    DVec3::new(
                        v.first().copied().unwrap_or(0.0),
                        v.get(1).copied().unwrap_or(0.0),
                        0.0,
                    ),
                    v.get(2).copied().unwrap_or(10.0),
                );
                (0..net.lanes.len())
                    .filter(|&i| {
                        net.lanes[i].kind == LaneKind::Street
                            && net.lanes[i]
                                .points
                                .iter()
                                .any(|q| (q.truncate() - p.truncate()).length() < r)
                    })
                    .collect()
            }
            None => list
                .split(',')
                .filter_map(|v| v.trim().parse::<usize>().ok())
                .filter(|&i| i < net.lanes.len())
                .collect(),
        };
        for i in chosen {
            let l = &net.lanes[i];
            let samples: Vec<String> = (0..l.points.len())
                .step_by((l.points.len() / 8).max(1))
                .map(|k| {
                    format!(
                        "[{:.1} m h {:.1} k {:.3}]",
                        l.dist[k],
                        l.headings[k],
                        l.curvature.get(k).copied().unwrap_or(0.0)
                    )
                })
                .collect();
            log::info!(
                "lane {i}: {} {:?} rev {} turn {} prio {} len {:.1} start ({:.1}, {:.1}) end ({:.1}, {:.1}) next {:?} light {:?} crossings {:?} {}",
                l.name,
                l.key,
                l.reversed,
                l.turn,
                l.priority,
                l.length(),
                l.start().x,
                l.start().y,
                l.end().x,
                l.end().y,
                l.next,
                l.traffic_light,
                net.crossings.get(i),
                samples.join(" ")
            );
        }
    }
}
