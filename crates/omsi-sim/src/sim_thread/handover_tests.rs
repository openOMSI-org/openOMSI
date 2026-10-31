use super::*;
use crate::people::pax::Task;
use crate::people::{BusId, Place, placed_bus_id};

#[test]
fn handover_riders_keep_their_bus_odometer() {
    let (bus, mut people) = bus_and_people();
    let world = crate::world_data::WorldData::default();
    people.seed_riders(4, &bus, &world);
    people.odometer.insert(BusId::Player, 25.0);
    for i in 0..4 {
        people.pax_mut(i).unwrap().km_start = 25.0;
    }
    let ai = BusId::Ai(77);
    people.remap_bus(BusId::Player, ai);
    people.odometer.insert(BusId::Player, 1000.0);
    people.remap_bus(BusId::Player, BusId::Ai(placed_bus_id(1)));
    people.remap_bus(ai, BusId::Player);
    let view = people.player_bus_view(&bus);
    people.tick_inner(0.02, &world, Some(&view), None);
    assert_eq!(people.people.len(), 4);
    for i in 0..4 {
        assert_eq!(
            people.pax(i).unwrap().task,
            Task::SittingInBus,
            "the previous player's mileage must not make the new riders alight"
        );
    }
    assert_eq!(people.odometer.get(&BusId::Player), Some(&25.0));
}

#[test]
fn handover_parked_bus_does_not_order_every_rider_to_alight() {
    let (bus, mut people) = bus_and_people();
    let world = crate::world_data::WorldData::default();
    let stop = people.build_pax_stop(&world, None, 77, bus.position, bus.heading, "Intermediate");
    people.stops.insert(77, stop);
    people.seed_riders(4, &bus, &world);
    let parked = BusId::Ai(placed_bus_id(2));
    people.remap_bus(BusId::Player, parked);
    people.set_placed_buses(std::iter::once((2, &bus)));
    let buses = people.gather_buses(&world, None, None);
    let stops = people.register_buses(&buses, 0.02);
    assert!(
        !stops[&parked].all_exit,
        "leaving the driving seat is not ending service"
    );
    people.tick_inner(0.02, &world, None, None);
    assert_eq!(people.people.len(), 4);
    for i in 0..4 {
        assert_eq!(people.pax(i).unwrap().task, Task::SittingInBus);
        assert!(matches!(people.people[i].place, Place::Bus(id, _) if id == parked));
    }
}

#[test]
fn handover_waiting_to_alight_riders_survive_alt_return() {
    let (mut a, mut people) = bus_and_people();
    let (mut b, _) = bus_and_people();
    let world = crate::world_data::WorldData::default();
    people.seed_riders(4, &b, &world);
    for i in 0..4 {
        let rider = people.pax_mut(i).unwrap();
        rider.task = Task::InBusToExit;
        rider.door = Some(0);
        rider.st = 7;
        rider.target_bus = true;
    }
    b.set_pax_var("Exit", 0, "Open", 1.0);
    people.player_bus_swapped(2, 1, &mut a);
    people.set_placed_buses(std::iter::once((2, &b)));
    let mut traffic = empty_traffic();
    let mut pipeline = Pipeline::default();
    let world = std::sync::Arc::new(world);
    for _ in 0..150 {
        let view = people.player_bus_view(&a);
        pipeline.owe_traffic(0.02, None);
        pipeline.owe_people(0.02, world.clone(), Some(view));
        assert!(
            pipeline
                .start(Some(&mut traffic), None, Some(people))
                .is_none()
        );
        let (_, back) = pipeline.join(Some(&mut traffic), None);
        people = back.unwrap();
    }
    assert_eq!(
        people.people.len(),
        4,
        "riders waiting in retained B must not disappear"
    );
    b.set_pax_var("Exit", 0, "Open", 0.0);
    people.player_bus_swapped(1, 2, &mut b);
    people.set_placed_buses(std::iter::once((1, &a)));
    let view = people.player_bus_view(&b);
    people.tick_inner(0.02, &*world, Some(&view), None);
    assert_eq!(people.people.len(), 4);
    for i in 0..4 {
        assert_eq!(people.pax(i).unwrap().task, Task::InBusToExit);
        assert_eq!(people.pax(i).unwrap().inside, Some(BusId::Player));
    }
}

#[test]
fn handover_moves_bus_local_requests_and_clears_absent_history() {
    let (_, mut people) = bus_and_people();
    let from = BusId::Ai(77);
    let to = BusId::Player;
    people.odometer.insert(from, 12.0);
    people.last_door_open.insert(from, 30.0);
    people.pax_req.insert(from, (vec![false], vec![true]));
    people.pax_busy.insert(from, (vec![true], vec![false]));
    people
        .pax_places
        .insert(from, vec![("occupied".into(), true)]);
    people.remap_bus(from, to);
    assert_eq!(people.odometer.get(&to), Some(&12.0));
    assert_eq!(people.last_door_open.get(&to), Some(&30.0));
    assert_eq!(people.pax_req.get(&to), Some(&(vec![false], vec![true])));
    assert_eq!(people.pax_busy.get(&to), Some(&(vec![true], vec![false])));
    assert_eq!(
        people.pax_places.get(&to),
        Some(&vec![("occupied".into(), true)])
    );
    assert!(!people.odometer.contains_key(&from));
    people.remap_bus(to, to);
    assert_eq!(people.odometer.get(&to), Some(&12.0));
    people.remap_bus(BusId::Ai(99), to);
    assert!(
        !people.odometer.contains_key(&to),
        "a new bus must not inherit a previous bus's history"
    );
    assert!(!people.last_door_open.contains_key(&to));
}

#[test]
fn handover_before_a_queued_tick_keeps_retained_riders() {
    let (mut a, mut people) = bus_and_people();
    let (b, _) = bus_and_people();
    let world = std::sync::Arc::new(crate::world_data::WorldData::default());
    people.seed_riders(4, &b, &*world);
    people.set_placed_buses(std::iter::once((1, &a)));
    let mut traffic = empty_traffic();
    let mut pipeline = Pipeline::default();
    pipeline.owe_traffic(0.02, None);
    pipeline.owe_people(0.02, world.clone(), Some(people.player_bus_view(&b)));
    // A queued frame still describes B as Player and A as retained.
    people.player_bus_swapped(2, 1, &mut a);
    people.set_placed_buses(std::iter::once((2, &b)));
    pipeline.rebind_people_bus(Some(people.player_bus_view(&a)));
    assert!(
        pipeline
            .start(Some(&mut traffic), None, Some(people))
            .is_none()
    );
    let (_, back) = pipeline.join(Some(&mut traffic), None);
    let people = back.unwrap();
    assert!(
        (people.time - 0.02).abs() < 1e-6,
        "rebinding must not discard the queued time"
    );
    assert_eq!(
        people.people.len(),
        4,
        "a queued frame must not despawn the newly retained B's riders"
    );
}
