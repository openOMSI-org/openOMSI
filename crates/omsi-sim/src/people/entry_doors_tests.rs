//! Which door of the player's bus a boarding passenger walks to (#3124).

use super::entry_doors::EntryDoors;
use super::*;

fn bus_with_cabin_entries(tag: &str, cabin_entries: &str) -> (PeopleSim, BusNow) {
    let dir = std::env::temp_dir().join(format!("omsi-entry-doors-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("test.bus"),
        "[boundingbox]\n2.5\n12\n3\n0\n0\n1.5\n\n[passengercabin]\ncabin.cfg\n\n[paths]\npaths.cfg\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("paths.cfg"),
        "[pathpnt]\n-1.1\n4\n0.4\n\n[pathpnt]\n-1.1\n0\n0.5\n\n[pathpnt]\n-1.1\n-4\n0.5\n\n[pathlink]\n0\n1\n\n[pathlink]\n1\n2\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("cabin.cfg"),
        format!("{cabin_entries}[passpos]\n-0.5\n0\n0.9\n0.45\n0\n"),
    )
    .unwrap();
    std::fs::write(dir.join("person.hum"), "[model]\nmodel.cfg\n").unwrap();
    std::fs::write(dir.join("model.cfg"), "").unwrap();
    let def = omsi_vehicle::Vehicle::load(&dir.join("test.bus")).unwrap();
    let cabin = Arc::new(Cabin::load_train(&[(&def, Vec3::ZERO, f32::INFINITY, false)]).unwrap());
    let ty = Arc::new(HumanType::load(&dir.join("person.hum")).unwrap());
    std::fs::remove_dir_all(&dir).ok();

    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    let mut pax = Pax::new(1.1, 0.5);
    pax.stop = Some(42);
    pax.bus = Some(BusId::Player);
    pax.pos = DVec3::new(-3.0, 0.2, 0.0);
    h.people.push(Person {
        id: 1,
        ty,
        variant: 0,
        position: pax.pos,
        heading: 0.0,
        lheading: 0.0,
        place: Place::Ground,
        vel: DVec2::ZERO,
        pace: 1.1,
        activity: Activity::Stand,
        anim: OmsiAnim::default(),
        state: State::Pax(Box::new(pax)),
        t_state: 0.0,
        interior: 0.0,
        tilt: Mat4::IDENTITY,
        age: 40.0,
        stuck: 0.0,
        stuck_total: 0.0,
        ghost: 0.0,
        car_wait: 0.0,
        detour: 0.0,
        detour_side: 0.0,
        why: "",
        puppet: None,
        remote: false,
    });
    h.stops.insert(
        42,
        PaxStop {
            name: "Kerb".into(),
            alias: String::new(),
            pos: DVec3::new(-4.0, 0.0, 0.0),
            heading: 0.0,
            side: 1,
            gather: DVec3::new(-3.0, 1.0, 0.0),
            spots: Vec::new(),
            taken: Vec::new(),
            enter_max: 1.0,
            enter_min: 0.0,
            length: 30.0,
            lane: None,
            was_near: false,
            near: false,
            clock_ms: 0.0,
            want: 0,
            factor: 1.0,
            buses: Vec::new(),
            dests: Vec::new(),
            lines: Vec::new(),
        },
    );
    let bus = BusNow {
        id: BusId::Player,
        next_stop: Some(RequestStop {
            id: 42,
            name: "Kerb".into(),
            alias: String::new(),
            pos: DVec3::new(-4.0, 0.0, 0.0),
        }),
        entry_open: vec![true; cabin.entries.len()],
        exit_open: vec![false; cabin.exits.len()],
        walk_open: None,
        cabin,
        pos: DVec3::ZERO,
        rot: Mat4::IDENTITY,
        heading: 0.0,
        speed: 0.0,
        interior: 0.0,
        air: CabinAir::default(),
        half: DVec2::new(1.25, 6.0),
        centre: DVec2::ZERO,
        accel: DVec2::ZERO,
        trailers: Vec::new(),
        terminus: None,
        takes: Takes::Terminus,
        places_off: Vec::new(),
        served: Some(42),
        route: None,
    };
    (h, bus)
}

fn bus_with_three_doors(tag: &str) -> (PeopleSim, BusNow) {
    bus_with_cabin_entries(
        tag,
        "[entry]\n0\n\n[entry]\n1\n\n[entry]\n2\n\n[exit]\n2\n\n",
    )
}

fn bus_with_hitched_trailer(tag: &str) -> (PeopleSim, BusNow) {
    let dir = std::env::temp_dir().join(format!("omsi-entry-trailer-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("paths_bus.cfg"),
        "[pathpnt]\n-1.1\n4\n0.4\n[pathpnt]\n-1.1\n0\n0.5\n[pathlink]\n0\n1\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("paths_trail.cfg"),
        "[pathpnt]\n-1.1\n-2\n0.5\n[pathpnt]\n-1.1\n-5\n0.5\n[pathlink]\n0\n1\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("cabin_bus.cfg"),
        "[entry]\n0\n[passpos]\n-0.5\n2\n0.9\n0.45\n0\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("cabin_trail.cfg"),
        "[entry]\n0\n[passpos]\n-0.5\n-3\n0.9\n0.45\n0\n",
    )
    .unwrap();
    std::fs::write(dir.join("person.hum"), "[model]\nmodel.cfg\n").unwrap();
    std::fs::write(dir.join("model.cfg"), "").unwrap();

    let def = |cabin: &str, paths: &str, coupling: Option<f32>| omsi_vehicle::Vehicle {
        path: dir.join("bus.bus"),
        passenger_cabin: Some(cabin.into()),
        paths: Some(paths.into()),
        bounding_box: Some([2.5, 9.0, 3.0, 0.0, 0.0, 1.5]),
        coupling_front_character: coupling.map(|kind| [30.0, -10.0, 10.0, kind]),
        ..Default::default()
    };
    let bus_def = def("cabin_bus.cfg", "paths_bus.cfg", None);
    let trail_def = def("cabin_trail.cfg", "paths_trail.cfg", Some(0.0));
    let cabin = Arc::new(
        Cabin::load_train(&[
            (&bus_def, Vec3::ZERO, f32::INFINITY, false),
            (&trail_def, Vec3::new(0.0, -10.0, 0.0), -5.0, false),
        ])
        .unwrap(),
    );
    let ty = Arc::new(HumanType::load(&dir.join("person.hum")).unwrap());
    std::fs::remove_dir_all(&dir).ok();

    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    let mut pax = Pax::new(1.1, 0.5);
    pax.stop = Some(42);
    pax.bus = Some(BusId::Player);
    pax.pos = DVec3::new(-3.0, -3.0, 0.0);
    h.people.push(Person {
        id: 1,
        ty,
        variant: 0,
        position: pax.pos,
        heading: 0.0,
        lheading: 0.0,
        place: Place::Ground,
        vel: DVec2::ZERO,
        pace: 1.1,
        activity: Activity::Stand,
        anim: OmsiAnim::default(),
        state: State::Pax(Box::new(pax)),
        t_state: 0.0,
        interior: 0.0,
        tilt: Mat4::IDENTITY,
        age: 40.0,
        stuck: 0.0,
        stuck_total: 0.0,
        ghost: 0.0,
        car_wait: 0.0,
        detour: 0.0,
        detour_side: 0.0,
        why: "",
        puppet: None,
        remote: false,
    });
    h.stops.insert(
        42,
        PaxStop {
            name: "Kerb".into(),
            alias: String::new(),
            pos: DVec3::new(-4.0, 0.0, 0.0),
            heading: 0.0,
            side: 1,
            gather: DVec3::new(-3.0, 1.0, 0.0),
            spots: Vec::new(),
            taken: Vec::new(),
            enter_max: 1.0,
            enter_min: 0.0,
            length: 30.0,
            lane: None,
            was_near: false,
            near: false,
            clock_ms: 0.0,
            want: 0,
            factor: 1.0,
            buses: Vec::new(),
            dests: Vec::new(),
            lines: Vec::new(),
        },
    );
    let bus = BusNow {
        id: BusId::Player,
        next_stop: Some(RequestStop {
            id: 42,
            name: "Kerb".into(),
            alias: String::new(),
            pos: DVec3::new(-4.0, 0.0, 0.0),
        }),
        entry_open: vec![true; cabin.entries.len()],
        exit_open: vec![false; cabin.exits.len()],
        walk_open: None,
        cabin,
        pos: DVec3::ZERO,
        rot: Mat4::IDENTITY,
        heading: 0.0,
        speed: 0.0,
        interior: 0.0,
        air: CabinAir::default(),
        half: DVec2::new(1.25, 6.0),
        centre: DVec2::ZERO,
        accel: DVec2::ZERO,
        trailers: Vec::new(),
        terminus: None,
        takes: Takes::Terminus,
        places_off: Vec::new(),
        served: Some(42),
        route: None,
    };
    (h, bus)
}

fn door_chosen(h: &mut PeopleSim, bus: &BusNow) -> Option<usize> {
    h.choose_entry(
        0,
        std::slice::from_ref(bus),
        &[(bus.id, 0)].into_iter().collect(),
    );
    h.pax(0).unwrap().door
}

#[test]
fn the_door_set_decides_which_doors_a_passenger_walks_to() {
    let (mut h, bus) = bus_with_three_doors("sets");
    assert_eq!(door_chosen(&mut h, &bus), Some(1));
    h.entry_doors = EntryDoors::First;
    assert_eq!(door_chosen(&mut h, &bus), Some(0));
    h.entry_doors = EntryDoors::FirstTwo;
    assert_eq!(door_chosen(&mut h, &bus), Some(1));
    h.entry_doors = EntryDoors::Last;
    assert_eq!(door_chosen(&mut h, &bus), Some(2));
    h.entry_doors = EntryDoors::AsBus;
    assert_eq!(door_chosen(&mut h, &bus), Some(1));
}

#[test]
fn a_door_the_set_leaves_out_is_not_used_even_when_nearest() {
    let (mut h, bus) = bus_with_three_doors("near");
    h.people[0].position = DVec3::new(-3.0, -1.0, 0.0);
    h.pax_mut(0).unwrap().pos = DVec3::new(-3.0, -1.0, 0.0);
    h.entry_doors = EntryDoors::First;
    assert_eq!(door_chosen(&mut h, &bus), Some(0));
    h.entry_doors = EntryDoors::AsBus;
    assert_eq!(door_chosen(&mut h, &bus), Some(1));
}

#[test]
fn with_the_rear_door_only_a_shut_rear_door_is_still_the_one_asked_for() {
    let (mut h, mut bus) = bus_with_three_doors("shut-rear");
    bus.entry_open = vec![true, true, false];
    h.entry_doors = EntryDoors::Last;
    assert_eq!(door_chosen(&mut h, &bus), Some(2));
}

#[test]
fn ai_and_remote_buses_keep_their_own_doors_regardless_of_entry_doors_setting() {
    let (mut h, mut bus) = bus_with_three_doors("ai-doors");
    bus.id = BusId::Ai(5);
    h.pax_mut(0).unwrap().bus = Some(BusId::Ai(5));
    h.entry_doors = EntryDoors::First;
    assert_eq!(door_chosen(&mut h, &bus), Some(1));
    h.entry_doors = EntryDoors::Last;
    assert_eq!(door_chosen(&mut h, &bus), Some(1));
}

#[test]
fn ticket_buyers_avoid_noticketsale_doors_when_setting_selects_only_those_doors() {
    let (mut h, bus) = bus_with_cabin_entries(
        "no-ticket-sale",
        "[entry]\n0\n\n[entry]\n1\n{noticketsale}\n\n[entry]\n2\n{noticketsale}\n\n[exit]\n2\n\n",
    );
    h.pax_mut(0).unwrap().ticket = TICKET_BUY;
    h.entry_doors = EntryDoors::Last;
    assert_eq!(door_chosen(&mut h, &bus), None);
    h.entry_doors = EntryDoors::First;
    assert_eq!(door_chosen(&mut h, &bus), Some(0));
}

#[test]
fn passengers_in_disconnected_cabin_section_preserve_reachable_doors() {
    let (mut h, bus) = bus_with_hitched_trailer("trailer");
    h.pax_mut(0).unwrap().seat = Some(1);
    h.entry_doors = EntryDoors::First;
    assert_eq!(door_chosen(&mut h, &bus), Some(1));
}

/// #3759: with the selling front door shut (no button) and the rear door that sells no
/// tickets open, a ticket buyer walks to the open rear door, as Omsi.exe searches again
/// without avoiding `{noticketsale}` (sub_72506c, 0x725227) - not to the shut front door.
#[test]
fn ticket_buyers_take_an_open_noticketsale_door_when_the_selling_door_is_shut() {
    let (mut h, mut bus) = bus_with_cabin_entries(
        "shut-selling-open-nosale",
        "[entry]\n0\n\n[entry]\n1\n{noticketsale}\n\n[exit]\n1\n\n",
    );
    h.pax_mut(0).unwrap().ticket = TICKET_BUY;
    bus.entry_open = vec![false, true];
    h.entry_doors = EntryDoors::AsBus;
    assert_eq!(door_chosen(&mut h, &bus), Some(1));
    // (an AI bus the same)
    bus.id = BusId::Ai(4);
    h.pax_mut(0).unwrap().bus = Some(BusId::Ai(4));
    assert_eq!(door_chosen(&mut h, &bus), Some(1));
    // both open: the selling door, though the other is as near
    bus.entry_open = vec![true, true];
    assert_eq!(door_chosen(&mut h, &bus), Some(0));
}

#[test]
fn ai_bus_ticket_buyers_keep_fallback_when_all_doors_are_noticketsale() {
    let (mut h, mut bus) = bus_with_cabin_entries(
        "ai-all-nosale",
        "[entry]\n0\n{noticketsale}\n\n[entry]\n1\n{noticketsale}\n\n[exit]\n1\n\n",
    );
    bus.id = BusId::Ai(3);
    h.pax_mut(0).unwrap().bus = Some(BusId::Ai(3));
    h.pax_mut(0).unwrap().ticket = TICKET_BUY;
    assert!(door_chosen(&mut h, &bus).is_some());
}

#[test]
fn player_default_as_bus_ticket_buyers_keep_fallback_when_all_doors_are_noticketsale() {
    let (mut h, bus) = bus_with_cabin_entries(
        "player-as-bus-all-nosale",
        "[entry]\n0\n{noticketsale}\n\n[entry]\n1\n{noticketsale}\n\n[exit]\n1\n\n",
    );
    h.pax_mut(0).unwrap().ticket = TICKET_BUY;
    h.entry_doors = EntryDoors::AsBus;
    assert!(door_chosen(&mut h, &bus).is_some());
}
