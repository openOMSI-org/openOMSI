use super::*;
use omsi_sim::people::{BusId, PeopleSim, placed_bus_id};
use std::sync::Arc;

fn loaded_buses() -> (App, Arc<omsi_sim::world_data::WorldData>, u64, u64) {
    let dir = std::env::temp_dir().join(format!(
        "omsi-alt-riders-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, text) in [
        (
            "test.bus",
            "[model]\nmodel.cfg\n[passengercabin]\ncabin.cfg\n[paths]\npaths.cfg\n",
        ),
        ("model.cfg", ""),
        ("person.hum", "[model]\nmodel.cfg\n"),
        (
            "paths.cfg",
            "[pathpnt]\n1\n0\n0.5\n[pathpnt]\n0\n0\n0.5\n[pathlink]\n0\n1\n",
        ),
        (
            "cabin.cfg",
            "[entry]\n0\n[exit]\n0\n[passpos]\n0\n0\n0.9\n0.45\n0\n",
        ),
    ] {
        std::fs::write(dir.join(name), text).unwrap();
    }
    let mut ty = Arc::new(omsi_sim::VehicleType::load(&dir, &dir.join("test.bus")).unwrap());
    let human = Arc::new(omsi_sim::human::HumanType::load(&dir.join("person.hum")).unwrap());
    Arc::make_mut(&mut ty).def.cameras_driver = player().vehicle.ty.def.cameras_driver.clone();
    let mut app = app();
    let mut a = player();
    let mut b = player();
    a.vehicle =
        omsi_sim::VehicleInstance::new(ty.clone(), omsi_sim::VehicleHost::new(Default::default()));
    b.vehicle = omsi_sim::VehicleInstance::new(ty, omsi_sim::VehicleHost::new(Default::default()));
    a.vehicle.position.x = 1000.0;
    let (a_id, b_id) = (a.uid, b.uid);
    let world = Arc::new(omsi_sim::world_data::WorldData::default());
    let mut sim = PeopleSim::new(&dir, 10);
    sim.types.push(human);
    sim.set_cabin(&mut b.vehicle);
    sim.seed_riders(1, &b.vehicle, &*world);
    assert_eq!(sim.people.len(), 1);
    sim.set_placed_buses(std::iter::once((a_id, &a.vehicle)));
    app.pipeline
        .owe_people(0.02, world.clone(), Some(sim.player_bus_view(&b.vehicle)));
    app.session.humans = Some(crate::humans::Humans { sim });
    app.session.placed.push(a);
    app.player = Some(b);
    std::fs::remove_dir_all(dir).unwrap();
    (app, world, a_id, b_id)
}

fn check_queued_return(enter: impl FnOnce(&mut App, u64)) {
    let (mut app, world, a, b) = loaded_buses();
    enter(&mut app, a);
    assert_eq!(app.player.as_ref().unwrap().uid, a);
    app.start_sim_thread();
    app.join_sim_thread();
    let h = app.session.humans.as_mut().unwrap();
    assert_eq!(
        h.people.len(),
        1,
        "a queued old frame must not erase B's passenger"
    );
    assert_eq!(h.pax(0).unwrap().inside, Some(BusId::Ai(placed_bus_id(b))));
    assert!(
        h.placed_now
            .iter()
            .any(|bus| bus.id == BusId::Ai(placed_bus_id(b)))
    );
    assert!(
        !h.placed_now
            .iter()
            .any(|bus| bus.id == BusId::Ai(placed_bus_id(a)))
    );
    assert_eq!(
        h.last_buses
            .iter()
            .find(|bus| bus.id == BusId::Player)
            .unwrap()
            .pos
            .x,
        1000.0
    );
    assert!(
        h.people[0].position.x.abs() < 2.0,
        "B's riders must not follow A's world position"
    );
    let mut frame = omsi_sim::view_events::PeopleFrame::default();
    h.snapshot_into(&mut frame);
    assert!(
        matches!(frame.people[0].place, omsi_sim::people::Place::Bus(id, _) if id == BusId::Ai(placed_bus_id(b)))
    );
    assert_eq!(frame.people[0].hidden, None);
    let view = h.player_bus_view(&app.player.as_ref().unwrap().vehicle);
    h.tick_inner(0.02, &*world, Some(&view), None);
    assert_eq!(h.people.len(), 1);
}

#[test]
fn alt_return_preserves_riders_when_a_people_tick_is_already_queued() {
    check_queued_return(|app, _| app.switch_vehicle());
}

#[test]
fn pedestrian_return_preserves_riders_when_a_people_tick_is_already_queued() {
    check_queued_return(|app, _| app.take_placed(0));
}

#[test]
fn f4_return_preserves_riders_when_a_people_tick_is_already_queued() {
    check_queued_return(|app, a| app.take_retained_bus(a).unwrap());
}
