//! Boarding must follow the display even when the duty or IBIS route stays unchanged.
use super::*;

fn setup(tag: &str) -> (PeopleSim, PlayerBusView) {
    let (mut people, bus) = pax_stops::brt_tests::setup(tag, None);
    people.player_cabin = Some(bus.cabin.clone());
    people.duty = Some((
        Arc::new(DutyTrip {
            name: "X10 duty".into(),
            departure: 0.0,
            terminus: "X10 terminus".into(),
            public: true,
            stops: vec![(42, "Platform".into(), true), (43, "X10 stop".into(), true)],
        }),
        0,
        false,
    ));
    let stop = people.stops.get_mut(&42).unwrap();
    stop.lines = [
        ("X10 stop", "X10 terminus"),
        ("M29 stop", "M29 terminus"),
        ("X10 stop", "X10 alternate"),
    ]
    .into_iter()
    .map(|(dest, term)| (dest.into(), [term.into()].into_iter().collect()))
    .collect();
    let pax = people.pax_mut(0).unwrap();
    pax.bus = None;
    pax.task = Task::WaitingForBus;
    pax.line = Some(0);
    pax.dest = Some("X10 stop".into());
    let view = PlayerBusView {
        position: bus.pos,
        heading: bus.heading,
        rot: bus.rot,
        speed: 0.0,
        speed_kmh: 0.0,
        a_trans: Vec3::ZERO,
        bounding_box: None,
        interior: bus.interior,
        air: bus.air,
        given_ticket: None,
        terminus: Some(Some("M29 terminus".into())),
        tt_stop: Some((42, Some("Platform".into()))),
        act_busstop: String::new(),
        // Deliberately stale: changing only the destination does not reset the route.
        route: Some(SetRoute {
            last_stop: Some("X10 stop".into()),
            same_service: vec!["X10 terminus".into(), "X10 alternate".into()],
        }),
        cabin: Some(CabinView {
            entry_open: vec![true; bus.cabin.entries.len()],
            exit_open: vec![true; bus.cabin.exits.len()],
            trailers: Vec::new(),
            places_off: Vec::new(),
        }),
    };
    (people, view)
}

fn choice(
    people: &mut PeopleSim,
    view: &PlayerBusView,
    line: usize,
) -> (Option<(BusId, Fit)>, BusAtStops) {
    let dest = people.stops[&42].lines[line].0.clone();
    let pax = people.pax_mut(0).unwrap();
    pax.line = Some(line);
    pax.dest = Some(dest);
    let buses = people.gather_buses(&crate::world_data::WorldData::default(), Some(view), None);
    let mut registered = people.register_buses(&buses, 0.0);
    let index = [(BusId::Player, 0)].into_iter().collect();
    (
        people.bus_for(0, 42, &buses, &index),
        registered.remove(&BusId::Player).unwrap(),
    )
}

#[test]
fn destination_boarding_overrides_duty_and_stale_ibis_route() {
    let (mut people, mut view) = setup("destination-duty");
    let boards = Some((BusId::Player, Fit::Terminus));
    assert_eq!(
        choice(&mut people, &view, 0).0,
        None,
        "X10 passengers reject the M29 sign"
    );
    assert_eq!(
        choice(&mut people, &view, 2).0,
        None,
        "a later stop on the duty is not enough either"
    );
    assert_eq!(
        choice(&mut people, &view, 1).0,
        boards,
        "M29 passengers use the displayed destination"
    );

    // The old duty and IBIS endpoint must not empty a bus showing another service.
    people.stops.get_mut(&42).unwrap().name = "X10 stop".into();
    people.duty.as_mut().unwrap().2 = true;
    let (chosen, registered) = choice(&mut people, &view, 1);
    assert_eq!(chosen, boards);
    assert!(!registered.all_exit);
    people.stops.get_mut(&42).unwrap().name = "Platform".into();
    people.duty.as_mut().unwrap().2 = false;

    // Changing the sign takes effect on the next snapshot, with the same duty selected.
    view.terminus = Some(Some("X10 terminus".into()));
    assert_eq!(choice(&mut people, &view, 0).0, boards);
    assert_eq!(choice(&mut people, &view, 1).0, None);
    assert_eq!(
        choice(&mut people, &view, 2).0,
        boards,
        "matching route aliases still work"
    );
    people.duty = None;
    assert_eq!(
        choice(&mut people, &view, 0).0,
        boards,
        "free drive uses the same display"
    );
}

#[test]
fn destination_boarding_distinguishes_no_display_support_from_out_of_service() {
    let (mut people, mut view) = setup("destination-empty");
    view.terminus = Some(None);
    for line in 0..3 {
        let (chosen, registered) = choice(&mut people, &view, line);
        assert_eq!(chosen, None);
        assert!(registered.all_exit);
    }
    // Preserve the compatibility fallback for mods with no destination variables.
    view.terminus = None;
    assert!(choice(&mut people, &view, 0).0.is_some());
    people.duty = None;
    assert_eq!(choice(&mut people, &view, 0).0, None);
}

#[test]
fn destination_boarding_reads_display_before_ibis_and_rejects_invalid_signs() {
    let dir = std::env::temp_dir().join(format!("omsi-destination-view-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("test.bus"),
        "[model]\nmodel.cfg\n[varnamelist]\n1\nvars.txt\n",
    )
    .unwrap();
    std::fs::write(dir.join("model.cfg"), "").unwrap();
    std::fs::write(
        dir.join("vars.txt"),
        "target_index_int\nIBIS_TerminusIndex\nIBIS_TerminusCode\n",
    )
    .unwrap();
    let ty = Arc::new(crate::VehicleType::load(&dir, &dir.join("test.bus")).unwrap());
    let mut vehicle = VehicleInstance::new(ty, crate::VehicleHost::new(Default::default()));
    std::fs::remove_dir_all(&dir).unwrap();
    vehicle.host.hof = Some(Arc::new(omsi_vehicle::Hof {
        termini: vec![
            omsi_vehicle::hof::Terminus {
                code: 10,
                texture_id: "X10 terminus".into(),
                ..Default::default()
            },
            omsi_vehicle::hof::Terminus {
                code: 29,
                texture_id: "M29 terminus".into(),
                ..Default::default()
            },
            omsi_vehicle::hof::Terminus {
                code: 99,
                texture_id: "Not in service".into(),
                all_exit: true,
                ..Default::default()
            },
            omsi_vehicle::hof::Terminus {
                code: 0,
                texture_id: "  ".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    }));
    let people = PeopleSim::new(Path::new("/nonexistent"), 200);
    vehicle.set_var("IBIS_TerminusCode", 10.0);
    vehicle.set_var("IBIS_TerminusIndex", 0.0);
    vehicle.set_var("target_index_int", 1.0);
    assert_eq!(
        people.player_bus_view(&vehicle).terminus,
        Some(Some("M29 terminus".into()))
    );
    for row in [2.0, 3.0] {
        vehicle.set_var("target_index_int", row);
        assert_eq!(people.player_bus_view(&vehicle).terminus, Some(None));
    }
    // Invalid values are a reported but unusable sign, not permission to use the duty.
    vehicle.set_var("IBIS_TerminusCode", 999.0);
    vehicle.set_var("IBIS_TerminusIndex", 999.0);
    for row in [-1.0, f32::NAN, 999.0] {
        vehicle.set_var("target_index_int", row);
        assert_eq!(people.player_bus_view(&vehicle).terminus, Some(None));
    }
}
