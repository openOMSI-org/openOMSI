use super::*;
use crate::ai_traffic::{bus_service::AiTimetable, model::BusSetup, setup::RandomTypes};
use crate::traffic::{LaneBuilder, Network};
use glam::DVec3;

struct World {
    root: PathBuf,
    ai: omsi_map::AiLists,
    places: crate::world_data::ObjectPlaces,
}
impl TimetableWorld for World {
    fn root(&self) -> &Path {
        &self.root
    }
    fn map_dir(&self) -> &Path {
        &self.root
    }
    fn chrono_dirs(&self) -> Vec<PathBuf> {
        vec![]
    }
    fn ailists(&self) -> &omsi_map::AiLists {
        &self.ai
    }
    fn raw_tiles(&self) -> Vec<(i32, i32)> {
        (0..4).map(|i| (i, 0)).collect()
    }
    fn has_tile(&self, key: (i32, i32)) -> bool {
        self.raw_tiles().contains(&key)
    }
    fn date(&self) -> i32 {
        20221010
    }
    fn object_positions(&self) -> &crate::world_data::ObjectPlaces {
        &self.places
    }
    fn stop_side(&self, _: i64) -> f32 {
        0.0
    }
}

fn load(traffic: &mut TrafficSim, world: &mut World, tile: usize) {
    let mut lane = LaneBuilder::arc(
        DVec3::new(0.0, tile as f64 * 100.0, 0.0),
        0.0,
        100.0,
        0.0,
        0.0,
        LaneKind::Street,
        3.0,
    );
    let key = LaneKey {
        tile: (tile as i32, 0),
        id: 1,
        path: 0,
    };
    lane.key = Some(key);
    let index = traffic.net.lanes.len();
    traffic.net.lanes.push(lane);
    traffic.net.by_key.insert(key, vec![index]);
    traffic.net.link(1.5);
    traffic.net.build_grid();
    traffic.net.compute_reach();
    traffic.lane_tiles.insert(key.tile);
    traffic.lanes_generation += 1;
    world.places.add_index(
        [(
            10 + tile as i64,
            (DVec3::new(3.0, tile as f64 * 100.0 + 50.0, 0.0), [0.0; 3]),
        )],
        [],
    );
}

fn fixture(loaded: &[usize]) -> (World, ScheduleSim, TrafficSim, PlayerDuty, VehicleInstance) {
    let mut world = World {
        root: std::env::temp_dir().join("omsi-streamed-handover-no-assets"),
        ai: Default::default(),
        places: Default::default(),
    };
    let mut schedule = ScheduleSim::new(&world.root, &world, &crate::SimClock::default());
    schedule.data.trips.push(omsi_timetable::Trip {
        name: "trip".into(),
        stations: (10..14).collect(),
        ..Default::default()
    });
    schedule.data.tracks.push(omsi_timetable::Track {
        path: "trip.ttr".into(),
        entries: (0..4)
            .map(|i| omsi_timetable::TrackEntry {
                values: vec![1.0, 0.0, i as f64, 0.0, 100.0],
            })
            .collect(),
    });
    let stops = (0..4)
        .map(|i| PlannedStop {
            object_id: 10 + i as i64,
            name: format!("Stop {i}"),
            arr: 3600.0 + i as f64 * 60.0,
            dep: 3605.0 + i as f64 * 60.0,
            position: None,
            dir: Default::default(),
            stops: true,
            closed: None,
        })
        .collect();
    let trip = PlannedTrip {
        name: "trip".into(),
        line: "L".into(),
        terminus: "End".into(),
        departure: 3600.0,
        end: 3785.0,
        stops,
    };
    let mut duty = PlayerDuty::new("L".into(), "T".into(), vec![trip], 0, 0, true);
    duty.next_stop = 1;
    let mut vehicle = crate::timetable_run::tests::script_test_vehicle("{frame}\n{end}\n", "", "");
    vehicle.position = DVec3::new(0.0, 60.0, 0.0);
    let random = RandomTypes {
        types: vec![],
        groups: vec![],
        group_curves: false,
        group_uvg: vec![],
        uvg_defaults: vec![],
    };
    let mut traffic = TrafficSim::assemble(
        &world.root,
        Network::default(),
        random,
        vec![],
        HashMap::new(),
        (vec![], vec![]),
        vec![],
        (1.0, 0),
        0,
    );
    for &tile in loaded {
        load(&mut traffic, &mut world, tile);
    }
    (world, schedule, traffic, duty, vehicle)
}

fn adopt(
    schedule: &mut ScheduleSim,
    traffic: &mut TrafficSim,
    vehicle: VehicleInstance,
    p: HandoverRoute,
) -> u64 {
    let setup = BusSetup {
        route: p.route,
        stops: p.stops,
        number: None,
        hof: None,
        timetable: AiTimetable {
            line: "L".into(),
            terminus: "End".into(),
            stops: vec![],
        },
    };
    let id = traffic.adopt_vehicle(vehicle, p.lane, p.s, Some(setup));
    schedule.resume_handover_route(traffic, id, p.continuation);
    id
}

#[test]
fn a_missing_middle_tile_is_not_bridged_to_a_loaded_downstream_tile() {
    let (world, mut schedule, mut traffic, duty, vehicle) = fixture(&[0, 2, 3]);
    let plan = schedule
        .prepare_handover_route(&world, &mut traffic, &duty, &vehicle)
        .unwrap();
    assert_eq!(plan.route, vec![0]);
    assert!(plan.stops.is_empty());
    assert!(plan.continuation.is_some());
    assert!(
        schedule.running.is_empty(),
        "preflight must not register a second bus"
    );
    let id = adopt(&mut schedule, &mut traffic, vehicle, plan);
    assert_eq!(schedule.running[0].car, id);
    assert_eq!(schedule.running[0].next, 1);
    assert_eq!(schedule.running[0].served, vec![true, false, false, false]);
    assert_eq!(
        traffic.cars[0].bus.as_ref().unwrap().waiting_tile,
        Some((1, 0))
    );
}

#[test]
fn streamed_tiles_append_each_remaining_stop_once_and_close_the_route_at_the_end() {
    let (mut world, mut schedule, mut traffic, duty, vehicle) = fixture(&[0, 1]);
    let plan = schedule
        .prepare_handover_route(&world, &mut traffic, &duty, &vehicle)
        .unwrap();
    assert_eq!(
        plan.stops.iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![11]
    );
    let id = adopt(&mut schedule, &mut traffic, vehicle, plan);
    load(&mut traffic, &mut world, 2);
    schedule.queue_due(&world, &mut traffic, 3600.0, 0.0);
    let bus = traffic.cars[0].bus.as_ref().unwrap();
    assert_eq!(
        bus.stops
            .iter()
            .map(|s| (s.id, s.depart))
            .collect::<Vec<_>>(),
        vec![(11, 3665.0), (12, 3725.0)]
    );
    assert_eq!(bus.waiting_tile, Some((3, 0)));
    assert!(bus.route_open);
    load(&mut traffic, &mut world, 3);
    schedule.queue_due(&world, &mut traffic, 3600.0, 0.0);
    schedule.queue_due(&world, &mut traffic, 3600.0, 0.0);
    let car = traffic.cars.iter().find(|c| c.id == id).unwrap();
    assert_eq!(car.state.route, vec![0, 1, 2, 3]);
    let bus = car.bus.as_ref().unwrap();
    assert_eq!(
        bus.stops.iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![11, 12, 13]
    );
    assert_eq!(
        bus.stops.iter().map(|s| s.ri).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(!bus.route_open);
    assert_eq!(bus.waiting_tile, None);
    assert!(schedule.running.is_empty());
}

#[test]
fn unloaded_past_tiles_do_not_prevent_handover_of_the_remaining_complete_route() {
    let (world, schedule, mut traffic, mut duty, mut vehicle) = fixture(&[2, 3]);
    vehicle.position.y = 260.0;
    duty.next_stop = 3;
    let plan = schedule
        .prepare_handover_route(&world, &mut traffic, &duty, &vehicle)
        .unwrap();
    assert_eq!(plan.route, vec![0, 1]);
    assert!(plan.continuation.is_none());
    assert_eq!(
        plan.stops.iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![13]
    );
}

#[test]
fn closed_and_passed_stops_are_not_reintroduced_when_tiles_arrive() {
    let (mut world, mut schedule, mut traffic, mut duty, vehicle) = fixture(&[0]);
    duty.trips[0].stops[2].closed = Some("closed".into());
    let plan = schedule
        .prepare_handover_route(&world, &mut traffic, &duty, &vehicle)
        .unwrap();
    adopt(&mut schedule, &mut traffic, vehicle, plan);
    for tile in 1..4 {
        load(&mut traffic, &mut world, tile);
    }
    schedule.queue_due(&world, &mut traffic, 3600.0, 0.0);
    assert_eq!(
        traffic.cars[0]
            .bus
            .as_ref()
            .unwrap()
            .stops
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        vec![11, 13]
    );
}

#[test]
fn a_failed_preflight_leaves_the_vehicle_and_timetable_ownership_unchanged() {
    let (world, schedule, mut traffic, duty, mut vehicle) = fixture(&[0]);
    vehicle.position.x = 100.0;
    assert!(
        schedule
            .prepare_handover_route(&world, &mut traffic, &duty, &vehicle)
            .is_err()
    );
    assert!(schedule.running.is_empty());
    assert!(traffic.cars.is_empty());
    assert_eq!(duty.next_stop, 1);
    assert_eq!(vehicle.position.x, 100.0);
}

#[test]
fn a_complete_route_keeps_all_remaining_stops_without_a_streaming_reservation() {
    let (world, schedule, mut traffic, duty, vehicle) = fixture(&[0, 1, 2, 3]);
    let plan = schedule
        .prepare_handover_route(&world, &mut traffic, &duty, &vehicle)
        .unwrap();
    assert_eq!(plan.route, vec![0, 1, 2, 3]);
    assert_eq!(
        plan.stops.iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![11, 12, 13]
    );
    assert!(plan.continuation.is_none());
}

#[test]
fn authored_station_entries_survive_until_their_own_tile_is_loaded() {
    let (mut world, mut schedule, mut traffic, duty, vehicle) = fixture(&[0, 1]);
    let trip = &mut schedule.data.trips[0];
    trip.stations.clear();
    trip.stations_legacy = (0..4)
        .map(|i| vec![(10 + i).to_string(), i.to_string()])
        .collect();
    let plan = schedule
        .prepare_handover_route(&world, &mut traffic, &duty, &vehicle)
        .unwrap();
    adopt(&mut schedule, &mut traffic, vehicle, plan);
    assert_eq!(
        schedule.running[0].station_steps,
        vec![Some(0), Some(1), Some(2), Some(3)]
    );
    for tile in 2..4 {
        load(&mut traffic, &mut world, tile);
    }
    schedule.queue_due(&world, &mut traffic, 3600.0, 0.0);
    assert_eq!(
        traffic.cars[0]
            .bus
            .as_ref()
            .unwrap()
            .stops
            .iter()
            .map(|s| (s.id, s.ri))
            .collect::<Vec<_>>(),
        vec![(11, 1), (12, 2), (13, 3)]
    );
}
