//! Synthetic regressions for platform registration and actual boarding targets.
use super::*;

fn setup(name: &str, trailer: Option<f32>) -> (PeopleSim, BusNow) {
    let dir = std::env::temp_dir().join(format!("omsi-brt-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in [
        (
            "paths.cfg",
            "[pathpnt]\n1.1\n4\n0.4\n[pathpnt]\n-1.1\n4\n0.4\n[pathpnt]\n-1.1\n-4\n0.4\n[pathlink]\n0\n1\n[pathlink]\n1\n2\n[pathpnt]\n0\n4\n0.4\n[pathpnt]\n0\n-4\n0.4\n[pathlink]\n1\n3\n[pathlink]\n3\n4\n",
        ),
        (
            "cabin.cfg",
            "[entry]\n0\n[entry]\n1\n{noticketsale}\n[entry]\n2\n{withbutton}\n[exit]\n0\n[exit]\n1\n[passpos]\n0\n0\n1\n0.45\n0\n",
        ),
        ("person.hum", "[model]\nmodel.cfg\n"),
        ("model.cfg", ""),
    ] {
        std::fs::write(dir.join(file), text).unwrap();
    }
    let def = omsi_vehicle::Vehicle {
        path: dir.join("bus.bus"),
        passenger_cabin: Some("cabin.cfg".into()),
        paths: Some("paths.cfg".into()),
        bounding_box: Some([2.5, 9.0, 3.0, 0.0, 0.0, 1.5]),
        ..Default::default()
    };
    let rear = omsi_vehicle::Vehicle {
        coupling_front_character: trailer.map(|kind| [30.0, -10.0, 10.0, kind]),
        ..def.clone()
    };
    let mut parts = vec![(&def, Vec3::ZERO, f32::INFINITY)];
    if trailer.is_some() {
        parts.push((&rear, Vec3::new(0.0, -10.0, 0.0), -5.0));
    }
    let cabin = Arc::new(Cabin::load_train(&parts).unwrap());
    let ty = Arc::new(HumanType::load(&dir.join("person.hum")).unwrap());
    std::fs::remove_dir_all(&dir).unwrap();
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    let mut pax = Pax::new(1.1, 0.5);
    pax.stop = Some(42);
    pax.bus = Some(BusId::Ai(3));
    pax.pos = DVec3::new(-3.0, 4.0, 0.0);
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
            name: "Platform".into(),
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
        id: BusId::Ai(3),
        next_stop: Some(RequestStop {
            id: 42,
            name: "Platform".into(),
            alias: String::new(),
            pos: DVec3::new(-4.0, 0.0, 0.0),
        }),
        entry_open: vec![false; cabin.entries.len()],
        exit_open: vec![false; cabin.exits.len()],
        cabin,
        pos: DVec3::ZERO,
        rot: Mat4::IDENTITY,
        heading: 0.0,
        speed: 0.0,
        walk_open: None,
        interior: 0.0,
        air: CabinAir::default(),
        half: DVec2::new(1.25, 6.0),
        centre: DVec2::ZERO,
        accel: DVec2::ZERO,
        trailers: Vec::new(),
        terminus: Some("Elsewhere".into()),
        takes: Takes::Terminus,
        places_off: Vec::new(),
        served: Some(42),
        route: None,
    };
    (h, bus)
}

fn choose(h: &mut PeopleSim, bus: &BusNow) -> usize {
    h.choose_entry(
        0,
        std::slice::from_ref(bus),
        &[(bus.id, 0)].into_iter().collect(),
    );
    h.pax(0)
        .unwrap()
        .door
        .expect("a reachable platform-facing target")
}

#[test]
fn brt_side_configuration_respects_both_traffic_hands() {
    for left in [false, true] {
        let kerb = if left { -1.0 } else { 1.0 };
        // The stop box may lie on the centreline or the opposite side; metadata wins.
        for x in [-4.0, 0.0, 4.0] {
            assert_eq!(boarding_side(0, left, x), Some(kerb));
            assert_eq!(boarding_side(1, left, x), Some(-kerb));
        }
        assert_eq!(boarding_side(2, left, -3.0), Some(-1.0));
        assert_eq!(boarding_side(2, left, 3.0), Some(1.0));
        assert_eq!(boarding_side(2, left, 0.0), None);
        assert_eq!(boarding_side(2, left, f32::NAN), None);
    }
}

#[test]
fn brt_shut_left_doors_keep_a_target_and_ignore_an_open_right_door() {
    let (mut h, mut bus) = setup("shut", None);
    // Closed button door is worth walking to even when the opposite side is open.
    bus.entry_open[0] = true;
    assert_eq!(choose(&mut h, &bus), 2);
    bus.entry_open[1] = true;
    assert_eq!(choose(&mut h, &bus), 1);
    // Ticket buyers prefer the selling left door, preserving vehicle flags.
    bus.entry_open[2] = true;
    h.pax_mut(0).unwrap().ticket = TICKET_BUY;
    assert_eq!(choose(&mut h, &bus), 2);
    // Without buttons or open doors, retain a valid left target (slot 0 is filtered).
    Arc::get_mut(&mut bus.cabin).unwrap().entries[2].button = false;
    bus.entry_open.fill(false);
    assert_eq!(choose(&mut h, &bus), 2);
    assert!(h.pax(0).unwrap().target.x < 0.0);
}

#[test]
fn brt_two_sided_stop_uses_the_passengers_side_with_a_centreline_box() {
    let (mut h, bus) = setup("two-sides", None);
    h.stops.get_mut(&42).unwrap().side = 2;
    h.stops.get_mut(&42).unwrap().pos = DVec3::ZERO;
    assert_ne!(choose(&mut h, &bus), 0);
    h.pax_mut(0).unwrap().pos.x = 3.0;
    assert_eq!(choose(&mut h, &bus), 0);
}

#[test]
fn brt_served_platform_is_registered_without_serving_its_neighbour() {
    let (mut h, bus) = setup("register", None);
    let mut other = h.stops.remove(&42).unwrap();
    // Opposite-facing stop object is still the exact scheduled berth.
    other.heading = 180.0;
    h.stops.insert(42, other);
    let mut neighbour = h.build_pax_stop(
        &crate::world_data::WorldData::default(),
        None,
        99,
        DVec3::ZERO,
        0.0,
        "Neighbour",
    );
    neighbour.lines.clear();
    h.stops.insert(99, neighbour);
    let reg = h.register_buses(std::slice::from_ref(&bus), 0.0);
    assert_eq!(reg[&bus.id].next, Some(42));
    assert!(h.in_stop_box(42, bus.id));
    assert!(!h.listed_at(99, bus.id));
    let ix = [(bus.id, 0)].into_iter().collect();
    assert_eq!(
        h.bus_for(0, 42, std::slice::from_ref(&bus), &ix),
        Some((bus.id, Fit::Any))
    );
    // A bus without any valid left entry must not attract these passengers.
    let mut bus = bus;
    Arc::get_mut(&mut bus.cabin)
        .unwrap()
        .entries
        .retain(|d| d.side > 0.0);
    assert!(h.bus_for(0, 42, std::slice::from_ref(&bus), &ix).is_none());
}

#[test]
fn brt_disconnected_trailer_keeps_its_own_door_indices() {
    let (mut h, mut bus) = setup("trailer", Some(0.0));
    assert_eq!(bus.cabin.groups, 2);
    h.pax_mut(0).unwrap().seat = Some(1);
    assert!(
        choose(&mut h, &bus) >= 4,
        "use the rear group's left entries"
    );
    // A rotated whole bus keeps local platform side and the same door slot.
    let before = choose(&mut h, &bus);
    bus.rot = Mat4::from_rotation_z(1.2);
    bus.heading = -1.2_f64.to_degrees();
    let local = h.pax(0).unwrap().pos.as_vec3();
    h.pax_mut(0).unwrap().pos = bus.world(local);
    assert_eq!(choose(&mut h, &bus), before);
}

#[test]
fn brt_articulated_rear_uses_its_rotated_frame_at_a_two_sided_platform() {
    let (mut h, mut bus) = setup("articulated", Some(1.0));
    assert_eq!(bus.cabin.groups, 1);
    h.stops.get_mut(&42).unwrap().side = 2;
    bus.trailers.push(PartFrame {
        pos: DVec3::new(0.0, -10.0, 0.0),
        rot: Mat4::from_rotation_z(0.4),
        heading: -0.4_f64.to_degrees(),
        offset: Vec3::new(0.0, -10.0, 0.0),
        joint_y: -5.0,
        half: DVec2::new(1.25, 4.5),
        centre: DVec2::ZERO,
    });
    let position = bus.world(Vec3::new(-3.0, -14.0, 0.0));
    h.pax_mut(0).unwrap().pos = position;
    h.people[0].position = position;
    bus.entry_open.fill(true);
    assert_eq!(
        choose(&mut h, &bus),
        5,
        "nearest left entry in the angled rear"
    );
    h.pax_mut(0).unwrap().pos = bus.world(Vec3::new(3.0, -6.0, 0.0));
    assert_eq!(
        choose(&mut h, &bus),
        3,
        "right rear entry with original request index"
    );
}

#[test]
fn brt_boarding_reservation_respects_script_disabled_seats_and_reachable_groups() {
    let (mut h, mut bus) = setup("reservation", Some(0.0));
    // The trailer has seats but no left entry. Its seats must stay unreserved.
    Arc::get_mut(&mut bus.cabin)
        .unwrap()
        .entries
        .retain(|d| d.group == 0 || d.side > 0.0);
    bus.places_off = vec![true, false];
    h.pax_mut(0).unwrap().task = Task::ToBus;
    h.register_buses(std::slice::from_ref(&bus), 0.0);
    let ix = [(bus.id, 0)].into_iter().collect();
    let world = crate::world_data::WorldData::default();
    h.pax_task(
        0,
        0.1,
        &world,
        std::slice::from_ref(&bus),
        &ix,
        &HashMap::new(),
        None,
        &mut false,
        &mut Vec::new(),
    );
    assert_eq!(
        h.pax(0).unwrap().seat,
        None,
        "do not reserve an unreachable or disabled seat"
    );
    bus.places_off[0] = false;
    h.pax_task(
        0,
        0.1,
        &world,
        std::slice::from_ref(&bus),
        &ix,
        &HashMap::new(),
        None,
        &mut false,
        &mut Vec::new(),
    );
    assert_eq!(h.pax(0).unwrap().seat, Some(0));
    assert_eq!(h.pax(0).unwrap().task, Task::WalkingToBus);
    assert!(bus.cabin.entries[h.pax(0).unwrap().door.unwrap()].side < 0.0);
}
