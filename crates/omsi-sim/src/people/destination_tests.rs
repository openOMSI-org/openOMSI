//! Known conflicting signs override the duty; unknown HOF spellings keep its fallback.
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
        Some((BusId::Player, Fit::Duty)),
        "the matching duty still takes its passengers"
    );
    people.duty = None;
    assert_eq!(
        choice(&mut people, &view, 2).0,
        boards,
        "matching route aliases still work"
    );
    assert_eq!(
        choice(&mut people, &view, 0).0,
        boards,
        "free drive uses the same display"
    );
}

#[test]
fn destination_boarding_keeps_duty_for_unknown_hof_spelling() {
    let (mut people, mut view) = setup("destination-spelling");
    let mut vehicle = destination_vehicle("spelling", true);
    Arc::make_mut(vehicle.host.hof.as_mut().unwrap()).termini[0].texture_id =
        "X10 terminus (via station)".into();
    view.terminus = people.player_bus_view(&vehicle).terminus;
    assert_eq!(
        view.terminus,
        Some(Some("X10 terminus (via station)".into()))
    );
    for line in [0, 2] {
        let (chosen, registered) = choice(&mut people, &view, line);
        assert_eq!(chosen, Some((BusId::Player, Fit::Duty)));
        assert!(!registered.all_exit);
    }
    assert_eq!(choice(&mut people, &view, 1).0, None);

    // The duty still unloads at its endpoint despite the different HOF spelling.
    people.stops.get_mut(&42).unwrap().name = "X10 stop".into();
    people.duty.as_mut().unwrap().2 = true;
    let (chosen, registered) = choice(&mut people, &view, 0);
    assert_eq!(chosen, None);
    assert!(registered.all_exit);
}

#[test]
fn destination_boarding_keeps_duty_when_stop_evidence_is_ambiguous() {
    let (mut people, view) = setup("destination-ambiguous");
    // A shared terminus in the same record is not proof of a different service.
    people.stops.get_mut(&42).unwrap().lines[1]
        .1
        .insert("X10 terminus".into());
    assert_eq!(
        choice(&mut people, &view, 0).0,
        Some((BusId::Player, Fit::Duty))
    );
    // Nor is a sign taking people to one of the duty's stops.
    people.stops.get_mut(&42).unwrap().lines[1]
        .1
        .remove("X10 terminus");
    people.stops.get_mut(&42).unwrap().lines[1].0 = "X10 stop".into();
    assert_eq!(
        choice(&mut people, &view, 0).0,
        Some((BusId::Player, Fit::Duty))
    );
    // With no record for the sign, retain compatibility rather than guessing.
    people.stops.get_mut(&42).unwrap().lines.remove(1);
    assert_eq!(
        choice(&mut people, &view, 0).0,
        Some((BusId::Player, Fit::Duty))
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

fn destination_vehicle(tag: &str, reports_display: bool) -> VehicleInstance {
    let dir = std::env::temp_dir().join(format!("omsi-destination-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("test.bus"),
        "[model]\nmodel.cfg\n[varnamelist]\n1\nvars.txt\n",
    )
    .unwrap();
    std::fs::write(dir.join("model.cfg"), "").unwrap();
    std::fs::write(
        dir.join("vars.txt"),
        if reports_display {
            "target_index_int\nIBIS_TerminusIndex\nIBIS_TerminusCode\n"
        } else {
            "IBIS_TerminusIndex\nIBIS_TerminusCode\n"
        },
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
    vehicle
}

#[test]
fn destination_boarding_unresolved_ibis_keeps_duty_but_blank_rows_empty_bus() {
    let (mut people, mut view) = setup("destination-ibis");
    let mut vehicle = destination_vehicle("ibis-only", false);
    assert_eq!(vehicle.var("target_index_int"), None);
    for invalid in [999.0, -1.0, f32::NAN] {
        vehicle.set_var("IBIS_TerminusCode", invalid);
        vehicle.set_var("IBIS_TerminusIndex", invalid);
        view.terminus = people.player_bus_view(&vehicle).terminus;
        assert_eq!(view.terminus, None);
        let (chosen, registered) = choice(&mut people, &view, 0);
        assert!(chosen.is_some());
        assert!(!registered.all_exit);
    }
    for (index, code) in [(2.0, 99.0), (3.0, 0.0)] {
        vehicle.set_var("IBIS_TerminusIndex", index);
        vehicle.set_var("IBIS_TerminusCode", code);
        view.terminus = people.player_bus_view(&vehicle).terminus;
        assert_eq!(view.terminus, Some(None));
        let (chosen, registered) = choice(&mut people, &view, 0);
        assert_eq!(chosen, None);
        assert!(registered.all_exit);
    }
}

#[test]
fn destination_boarding_invalid_display_keeps_duty_unless_ibis_resolves() {
    let (mut people, mut view) = setup("destination-invalid-display");
    let mut vehicle = destination_vehicle("invalid-display", true);
    for invalid in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        vehicle.set_var("target_index_int", invalid);
        vehicle.set_var("IBIS_TerminusCode", 999.0);
        vehicle.set_var("IBIS_TerminusIndex", 999.0);
        view.terminus = people.player_bus_view(&vehicle).terminus;
        assert_eq!(
            view.terminus, None,
            "invalid display {invalid} is not a blank sign"
        );
        let (chosen, registered) = choice(&mut people, &view, 0);
        assert!(
            chosen.is_some(),
            "the active duty still admits its passengers"
        );
        assert!(
            !registered.all_exit,
            "an invalid index must not send riders off"
        );

        // A resolved IBIS row still wins, including explicit blank/all-exit rows.
        for (index, code) in [(1.0, 29.0), (2.0, 99.0), (3.0, 0.0)] {
            vehicle.set_var("IBIS_TerminusIndex", index);
            vehicle.set_var("IBIS_TerminusCode", code);
            view.terminus = people.player_bus_view(&vehicle).terminus;
            let (chosen, registered) = choice(&mut people, &view, 0);
            assert_eq!(chosen, None);
            if code == 29.0 {
                assert_eq!(view.terminus, Some(Some("M29 terminus".into())));
                assert!(!registered.all_exit);
                assert_eq!(
                    choice(&mut people, &view, 1).0,
                    Some((BusId::Player, Fit::Terminus))
                );
            } else {
                assert_eq!(view.terminus, Some(None));
                assert!(registered.all_exit);
            }
        }
    }
}

#[test]
fn destination_boarding_reads_display_before_ibis_and_rejects_blank_or_missing_rows() {
    let mut vehicle = destination_vehicle("display", true);
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
    // A nonnegative display row outside the HOF retains its existing empty-sign behavior.
    vehicle.set_var("IBIS_TerminusCode", 999.0);
    vehicle.set_var("IBIS_TerminusIndex", 999.0);
    vehicle.set_var("target_index_int", 999.0);
    assert_eq!(people.player_bus_view(&vehicle).terminus, Some(None));
}
