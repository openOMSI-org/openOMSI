//! #3124: which door of the player's bus a boarding passenger walks to, when the line
//! says they board at one door only.

use super::entry_doors::EntryDoors;
use super::*;

/// A bus with three entry doors on the kerb side - at the front (path point 0), in the
/// middle (1) and at the rear (2) - standing at a stop on the right.
fn bus_with_three_doors(tag: &str) -> (PeopleSim, BusNow) {
    let dir = std::env::temp_dir().join(format!("omsi-entry-doors-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("test.bus"), "[boundingbox]\n2.5\n12\n3\n0\n0\n1.5\n\n[passengercabin]\ncabin.cfg\n\n[paths]\npaths.cfg\n").unwrap();
    std::fs::write(
        dir.join("paths.cfg"),
        "[pathpnt]\n-1.1\n4\n0.4\n\n[pathpnt]\n-1.1\n0\n0.5\n\n[pathpnt]\n-1.1\n-4\n0.5\n\n[pathlink]\n0\n1\n\n[pathlink]\n1\n2\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("cabin.cfg"),
        "[entry]\n0\n\n[entry]\n1\n\n[entry]\n2\n\n[exit]\n2\n\n[passpos]\n-0.5\n0\n0.9\n0.45\n0\n",
    )
    .unwrap();
    std::fs::write(dir.join("person.hum"), "[model]\nmodel.cfg\n").unwrap();
    std::fs::write(dir.join("model.cfg"), "").unwrap();
    let def = omsi_vehicle::Vehicle::load(&dir.join("test.bus")).unwrap();
    let cabin = Arc::new(Cabin::load_train(&[(&def, Vec3::ZERO, f32::INFINITY)]).unwrap());
    let ty = Arc::new(HumanType::load(&dir.join("person.hum")).unwrap());
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(cabin.entries.len(), 3, "three doors");

    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    let mut pax = Pax::new(1.1, 0.5);
    pax.stop = Some(42);
    pax.bus = Some(BusId::Player);
    // at the middle door, so the nearest one is not the front one
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
        entry_open: vec![true; 3],
        exit_open: vec![false; 1],
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
    // as the bus says: the nearest open one - the middle door, the person standing by it
    assert_eq!(door_chosen(&mut h, &bus), Some(1), "as the vehicle says");
    h.entry_doors = EntryDoors::First;
    assert_eq!(door_chosen(&mut h, &bus), Some(0), "the front door only");
    h.entry_doors = EntryDoors::FirstTwo;
    assert_eq!(
        door_chosen(&mut h, &bus),
        Some(1),
        "the front or the middle one - the nearer"
    );
    h.entry_doors = EntryDoors::Last;
    assert_eq!(door_chosen(&mut h, &bus), Some(2), "the rear door only");
    h.entry_doors = EntryDoors::AsBus;
    assert_eq!(
        door_chosen(&mut h, &bus),
        Some(1),
        "back to the bus's own choice"
    );
}

#[test]
fn a_door_the_set_leaves_out_is_not_used_even_when_nearest() {
    let (mut h, bus) = bus_with_three_doors("near");
    // alone in the front half of the bus: the middle door is a step away, the front one
    // is not - the line nevertheless boards at the front door only
    h.people[0].position = DVec3::new(-3.0, -1.0, 0.0);
    h.pax_mut(0).unwrap().pos = DVec3::new(-3.0, -1.0, 0.0);
    h.entry_doors = EntryDoors::First;
    assert_eq!(door_chosen(&mut h, &bus), Some(0));
    h.entry_doors = EntryDoors::AsBus;
    assert_eq!(
        door_chosen(&mut h, &bus),
        Some(1),
        "the bus lets them in at the nearest"
    );
}

#[test]
fn with_the_rear_door_only_a_shut_rear_door_is_still_the_one_asked_for() {
    let (mut h, mut bus) = bus_with_three_doors("shut-rear");
    bus.entry_open = vec![true, true, false];
    h.entry_doors = EntryDoors::Last;
    assert_eq!(
        door_chosen(&mut h, &bus),
        Some(2),
        "they wait at the door the line boards at"
    );
}

#[test]
fn ai_and_remote_buses_keep_their_own_doors_regardless_of_entry_doors_setting() {
    let (mut h, mut bus) = bus_with_three_doors("ai-doors");
    bus.id = BusId::Ai(5);
    h.pax_mut(0).unwrap().bus = Some(BusId::Ai(5));
    // Player's setting is front door only, but AI bus's passenger takes the nearest open middle door
    h.entry_doors = EntryDoors::First;
    assert_eq!(
        door_chosen(&mut h, &bus),
        Some(1),
        "AI bus keeps the vehicle's own choice"
    );
    h.entry_doors = EntryDoors::Last;
    assert_eq!(
        door_chosen(&mut h, &bus),
        Some(1),
        "AI bus does not restrict to rear door"
    );
}
