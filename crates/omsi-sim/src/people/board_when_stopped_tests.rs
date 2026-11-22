//! #2757: whether the stop's passengers set off for the bus while it is still rolling in and
//! its doors are shut, or hold where they are until it stands with a door open.

use super::*;
use crate::world_data::WorldData;

/// A stop with one passenger waiting for the bus that is shown there, and the bus itself,
/// `speed` m/s.
fn waiting_at_a_stop(
    tag: &str,
    speed: f64,
    doors_open: bool,
) -> (PeopleSim, Vec<BusNow>, HashMap<BusId, usize>, WorldData) {
    let ty = super::ticket_hook_tests::human_type(tag);
    let world = crate::world_data::WorldData::default();
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    let mut stop = h.build_pax_stop(&world, None, 77, DVec3::ZERO, 0.0, "Střížkov");
    stop.buses.push((BusId::Player, true));
    h.stops.insert(77, stop);
    let mut bus = super::ticket_hook_tests::player_bus(tag);
    bus.speed = speed;
    bus.entry_open = vec![doors_open];
    let person = {
        let mut p = super::ticket_hook_tests::boarding(&ty, 1, Task::WaitingForBus);
        if let State::Pax(x) = &mut p.state {
            x.stop = Some(77);
        }
        p
    };
    h.people = vec![person];
    let buses = vec![bus];
    let bus_ix: HashMap<BusId, usize> = [(BusId::Player, 0)].into_iter().collect();
    (h, buses, bus_ix, world)
}

/// One step of the waiting passenger's task, as the tick would give it.
fn step(
    h: &mut PeopleSim,
    buses: &[BusNow],
    bus_ix: &HashMap<BusId, usize>,
    world: &WorldData,
) -> Task {
    h.time += 1.0 / 60.0;
    let mut taken = false;
    let mut remove = Vec::new();
    h.pax_task(
        0,
        1.0 / 60.0,
        world,
        buses,
        bus_ix,
        &HashMap::new(),
        None,
        &mut taken,
        &mut remove,
    );
    h.pax(0).unwrap().task
}

#[test]
fn as_omsi_had_it_the_stops_passengers_set_off_as_the_bus_rolls_in() {
    let (mut h, buses, bus_ix, world) = waiting_at_a_stop("rolling-default", 5.0, false);
    assert!(
        !h.board_when_stopped,
        "off until the settings say otherwise"
    );
    let task = step(&mut h, &buses, &bus_ix, &world);
    assert!(
        matches!(task, Task::ToBus | Task::WalkingToBus),
        "{task:?}: off they go, doors shut"
    );
}

#[test]
fn with_the_setting_on_nobody_walks_to_a_bus_still_rolling_in() {
    let (mut h, buses, bus_ix, world) = waiting_at_a_stop("rolling-wait", 5.0, true);
    h.board_when_stopped = true;
    let task = step(&mut h, &buses, &bus_ix, &world);
    assert_eq!(
        task,
        Task::WaitingForBus,
        "still rolling at 5 m/s, if its doors are open"
    );
    // and standing at the stop but with every door shut
    let (mut h, buses, bus_ix, world) = waiting_at_a_stop("shut-wait", 0.0, false);
    h.board_when_stopped = true;
    assert_eq!(
        step(&mut h, &buses, &bus_ix, &world),
        Task::WaitingForBus,
        "standing still, doors shut"
    );
    // standing with a door open: in they go
    let (mut h, buses, bus_ix, world) = waiting_at_a_stop("open-wait", 0.0, true);
    h.board_when_stopped = true;
    let task = step(&mut h, &buses, &bus_ix, &world);
    assert!(
        matches!(task, Task::ToBus | Task::WalkingToBus),
        "{task:?}: the bus is standing and open"
    );
}

#[test]
fn the_slowest_roll_still_counts_as_moving() {
    // a bus creeping past at half a metre a second is not standing, and the passengers on the
    // setting wait: it is what a driver rolling through the stop looks like from the kerb
    let (mut h, buses, bus_ix, world) = waiting_at_a_stop("creep", 0.5, true);
    h.board_when_stopped = true;
    assert_eq!(
        step(&mut h, &buses, &bus_ix, &world),
        Task::WaitingForBus,
        "{:?}",
        super::model::BOARD_STOPPED_SPEED
    );
    // ... until it is under the threshold the setting names
    let (mut h, buses, bus_ix, world) = waiting_at_a_stop(
        "creep-slow",
        crate::people::model::BOARD_STOPPED_SPEED * 0.5,
        true,
    );
    h.board_when_stopped = true;
    let task = step(&mut h, &buses, &bus_ix, &world);
    assert!(matches!(task, Task::ToBus | Task::WalkingToBus), "{task:?}");
}
