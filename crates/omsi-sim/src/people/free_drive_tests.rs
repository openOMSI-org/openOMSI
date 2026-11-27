//! #2712: the stops of the map fill in free drive, where no trip of the timetable is due
//! anywhere - as OMSI's passengers turned up whatever the player was driving.

use super::free_drive::{FreeDriveFill, stop_served};
use super::*;

#[derive(Default)]
struct FlatWorld {
    collision: Arc<crate::collision::CollisionWorld>,
    parked: Arc<Vec<crate::collision::Obb>>,
    objects: crate::world_data::ObjectPlaces,
}

impl World for FlatWorld {
    fn root(&self) -> &Path {
        Path::new("/nonexistent")
    }
    fn map_dir(&self) -> &Path {
        self.root()
    }
    fn walk_height(&self, _: f64, _: f64) -> Option<f64> {
        Some(0.0)
    }
    fn walk_height_near(&self, x: f64, y: f64, _: f64) -> Option<f64> {
        self.walk_height(x, y)
    }
    fn has_ground(&self, _: f64, _: f64) -> bool {
        true
    }
    fn stop_exit_weight(&self, _: i64) -> f32 {
        0.5
    }
    fn stop_enter(&self, _: i64) -> (f32, f32) {
        (1.0, 0.0)
    }
    fn stop_side(&self, _: i64) -> f32 {
        1.0
    }
    fn stop_length(&self, _: i64) -> f32 {
        30.0
    }
    fn bus_stops(&self) -> &[(i64, DVec3, f64, String)] {
        &[]
    }
    fn waiting_places(&self) -> &[(i64, DVec3, f64, f32)] {
        &[]
    }
    fn object_positions(&self) -> &crate::world_data::ObjectPlaces {
        &self.objects
    }
    fn collision(&self) -> &Arc<crate::collision::CollisionWorld> {
        &self.collision
    }
    fn parked_boxes(&self) -> &Arc<Vec<crate::collision::Obb>> {
        &self.parked
    }
    fn tiles_generation(&self) -> u64 {
        0
    }
}

#[test]
fn the_setting_spells_the_three_ways() {
    assert_eq!(
        FreeDriveFill::default(),
        FreeDriveFill::Auto,
        "as OMSI had it"
    );
    for on in ["on", "1", "yes", "true", "always", " ON "] {
        assert_eq!(FreeDriveFill::from_setting(on), FreeDriveFill::On, "{on:?}");
    }
    for off in ["off", "0", "no", "false", "never"] {
        assert_eq!(
            FreeDriveFill::from_setting(off),
            FreeDriveFill::Off,
            "{off:?}"
        );
    }
    for auto in ["auto", "", "nonsense"] {
        assert_eq!(
            FreeDriveFill::from_setting(auto),
            FreeDriveFill::Auto,
            "{auto:?}"
        );
    }
    // and what it writes is read back as itself
    for f in [FreeDriveFill::Auto, FreeDriveFill::On, FreeDriveFill::Off] {
        assert_eq!(FreeDriveFill::from_setting(f.setting_name()), f);
    }
}

#[test]
fn free_drive_fills_the_stops_and_a_duty_keeps_to_the_timetable() {
    assert!(
        FreeDriveFill::Auto.fills(false),
        "no duty: the map fills as in OMSI"
    );
    assert!(
        !FreeDriveFill::Auto.fills(true),
        "on a duty: only the trips due"
    );
    assert!(FreeDriveFill::On.fills(true), "always");
    assert!(FreeDriveFill::On.fills(false));
    assert!(!FreeDriveFill::Off.fills(false), "never");
    assert!(!FreeDriveFill::Off.fills(true));
}

#[test]
fn a_stop_is_filled_unless_the_timetable_is_worked_and_has_nothing_due_there() {
    // free drive, a timetable in the map: filled by the stop's own numbers
    assert_eq!(
        stop_served(true, true, true, false),
        1.0,
        "nothing due anywhere in free drive"
    );
    assert_eq!(
        stop_served(true, true, false, false),
        1.0,
        "not even a line listed"
    );
    // on a duty: as before - a stop with no trip due, or no line, stays empty
    assert_eq!(stop_served(true, false, true, false), 0.0_f32);
    assert_eq!(
        stop_served(true, false, false, true),
        0.0,
        "no line listed there"
    );
    assert_eq!(stop_served(true, false, true, true), 1.0, "a trip is due");
    // a map with no timetable at all: everything fills, as it always did
    assert_eq!(stop_served(false, false, false, false), 1.0_f32);
}

#[test]
fn populating_stop_in_free_drive_with_no_due_trips_assigns_valid_destination() {
    let (mut h, _) = pax_stops::brt_tests::setup("free-drive-populate", None);
    h.types = vec![h.people[0].ty.clone()];
    h.pool_types = 1;
    h.people.clear();
    h.map_humans_done = true;
    h.free_drive_fill = FreeDriveFill::Auto;
    h.duty = None; // Free drive
    let mut targets = HashMap::new();
    targets.insert(
        42,
        vec![("Centrum".into(), HashSet::from(["Centrum".into()]))],
    );
    h.stop_targets = Some(targets); // Timetable targets exist, but no trips due at stop 42
    h.due_dests = Some(HashMap::new().into()); // Empty: no trips due at stop 42

    let stop = h.stops.get_mut(&42).unwrap();
    h.center = stop.pos;
    stop.spots = (0..4)
        .map(|k| WaitSpot {
            pos: stop.pos + DVec3::X * k as f64,
            face: 0.0,
            height: 0.0,
        })
        .collect();
    stop.taken = vec![false; 4];
    stop.enter_max = 2.0;
    stop.enter_min = 2.0;
    stop.near = true;
    stop.clock_ms = 15_001.0;
    stop.dests = vec![("Centrum".into(), 1.0)];
    stop.lines = vec![("Centrum".into(), HashSet::from(["Centrum".into()]))];

    let world = FlatWorld::default();
    h.stops_tick(0.1, &world);

    // Stop has spawned waiting passengers despite no trips being due
    let waiting_pax: Vec<_> = h
        .people
        .iter()
        .filter_map(|p| match &p.state {
            State::Pax(x) if x.stop == Some(42) => Some(x.clone()),
            _ => None,
        })
        .collect();

    assert!(
        !waiting_pax.is_empty(),
        "passengers should spawn in free drive"
    );
    // Verify destination and line are assigned, not filtered to None
    for p in &waiting_pax {
        assert_eq!(p.dest.as_deref(), Some("Centrum"));
        assert_eq!(p.line, Some(0));
    }
}

/// #3506: on a duty a stop fills to the share of its number its trips ask for now (Omsi.exe
/// 0x61a068 / 0x61bf94) - half of it while the next bus is still some minutes off.
#[test]
fn a_worked_timetable_fills_a_stop_to_the_share_its_trips_ask_for() {
    let (mut h, _) = pax_stops::brt_tests::setup("due-share", None);
    h.types = vec![h.people[0].ty.clone()];
    h.pool_types = 1;
    h.people.clear();
    h.map_humans_done = true;
    h.free_drive_fill = FreeDriveFill::Off;
    h.density = 1.0;
    let mut targets = HashMap::new();
    targets.insert(
        42,
        vec![("Centrum".into(), HashSet::from(["Centrum".into()]))],
    );
    h.stop_targets = Some(targets);
    let mut due = crate::timetable_run::DueTrips::default();
    due.dests.insert(42, HashSet::from(["Centrum".to_string()]));
    let call = |departure, share| crate::timetable_run::DueVisit {
        tour: 1,
        departure,
        station: 0,
        share,
    };
    due.visits.insert(42, vec![call(0, 0.5)]);
    h.due_dests = Some(due);
    let stop = h.stops.get_mut(&42).unwrap();
    h.center = stop.pos;
    stop.spots = (0..8)
        .map(|k| WaitSpot {
            pos: stop.pos + DVec3::X * k as f64,
            face: 0.0,
            height: 0.0,
        })
        .collect();
    stop.taken = vec![false; 8];
    stop.enter_max = 4.0;
    stop.enter_min = 4.0;
    stop.factor = 1.0;
    stop.near = true;
    stop.dests = vec![("Centrum".into(), 1.0)];
    stop.lines = vec![("Centrum".into(), HashSet::from(["Centrum".into()]))];
    let world = FlatWorld::default();
    h.stops_tick(0.1, &world);
    assert_eq!(h.stops[&42].want, 2, "half of the stop's 4");
    // two trips due at once ask for all of it, never more
    h.due_dests
        .as_mut()
        .unwrap()
        .visits
        .insert(42, vec![call(0, 1.0), call(1, 0.7)]);
    h.stops_tick(0.1, &world);
    assert_eq!(h.stops[&42].want, 4);
    // the tour's bus has left the stop on the second of them: neither asks for anybody now
    h.due_dests.as_mut().unwrap().buses.insert(1, 1, 1);
    h.stops_tick(0.1, &world);
    assert_eq!(h.stops[&42].want, 0);
}

/// #3841: a player on a duty finds people at the stops even when no trip of the map's
/// timetable is due anywhere (an early hour, or a tour that does not run today). The reporter
/// saw the whole map empty on stock and modded maps alike.
#[test]
fn a_duty_with_no_trips_due_still_fills_its_stops() {
    let (mut h, _) = pax_stops::brt_tests::setup("issue-3841-duty", None);
    h.types = vec![h.people[0].ty.clone()];
    h.pool_types = 1;
    h.people.clear();
    h.map_humans_done = true;
    h.free_drive_fill = FreeDriveFill::Auto;
    h.density = 1.0;
    h.duty = Some((
        Arc::new(DutyTrip {
            name: "T".into(),
            departure: 36_000.0,
            terminus: "43".into(),
            public: true,
            stops: vec![(42, "42".into(), true), (43, "43".into(), true)],
        }),
        0,
        false,
    ));
    let mut targets = HashMap::new();
    targets.insert(42, vec![("43".into(), HashSet::from(["43".into()]))]);
    h.stop_targets = Some(targets);
    // The timetable is loaded, but no trip of it is due anywhere now (the reporter's
    // "any map, any bus" case). The stops must still carry their own people.
    h.due_dests = Some(crate::timetable_run::DueTrips::default());
    let stop = h.stops.get_mut(&42).unwrap();
    h.center = stop.pos;
    stop.spots = (0..6)
        .map(|k| WaitSpot {
            pos: stop.pos + DVec3::X * k as f64,
            face: 0.0,
            height: 0.0,
        })
        .collect();
    stop.taken = vec![false; 6];
    stop.enter_max = 3.0;
    stop.enter_min = 3.0;
    stop.factor = 1.0;
    stop.near = true;
    stop.clock_ms = 15_001.0;
    stop.dests = vec![("43".into(), 1.0)];
    stop.lines = vec![("43".into(), HashSet::from(["43".into()]))];

    let world = FlatWorld::default();
    h.stops_tick(0.1, &world);
    assert_eq!(
        h.stops[&42].want, 3,
        "no trip is due, so the stop fills by its own number"
    );
    let waiting: Vec<usize> = h
        .people
        .iter()
        .enumerate()
        .filter(|(_, p)| matches!(&p.state, State::Pax(x) if x.stop == Some(42)))
        .map(|(i, _)| i)
        .collect();
    assert!(
        !waiting.is_empty(),
        "a worked duty must have waiting people"
    );
    for i in waiting {
        assert_eq!(h.pax(i).unwrap().dest.as_deref(), Some("43"));
    }
}
