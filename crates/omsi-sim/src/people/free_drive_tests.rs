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
    h.due_dests = Some(HashMap::new()); // Empty: no trips due at stop 42

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
