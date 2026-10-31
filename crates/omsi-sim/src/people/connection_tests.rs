use super::*;

#[derive(Default)]
struct FlatWorld {
    collision: Arc<crate::collision::CollisionWorld>,
    parked: Arc<Vec<crate::collision::Obb>>,
    objects: crate::world_data::ObjectPlaces,
    ground_samples: std::sync::atomic::AtomicUsize,
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
        self.ground_samples.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
fn connection_rejects_unloaded_ground_and_large_steps() {
    let from = DVec3::ZERO;
    let to = DVec3::X * 10.0;
    assert!(connection_walkable(from, to, |_, _, _| Some(0.0)));
    assert!(!connection_walkable(from, to, |x, _, _| (x < 4.0
        || x > 6.0)
        .then_some(0.0)));
    assert!(!connection_walkable(from, to, |x, _, _| Some(if x < 5.0 {
        0.0
    } else {
        2.0
    })));
    assert!(!connection_walkable(from, DVec3::NAN, |_, _, _| Some(0.0)));
    assert!(!connection_walkable(from, DVec3::X * 81.0, |_, _, _| Some(
        0.0
    )));
}

#[test]
fn connection_keeps_identity_and_stops_after_two_transfers() {
    let (mut h, mut bus) = super::super::pax_stops::brt_tests::setup("connections-identity", None);
    h.transfer_percent = 100.0;
    bus.entry_open.fill(true);
    let mut other = bus.clone();
    other.id = BusId::Ai(4);
    other.entry_open.fill(true);
    let stop = h.stops.get_mut(&42).unwrap();
    stop.dests = vec![("Elsewhere".into(), 1.0)];
    stop.lines = vec![(
        "Elsewhere".into(),
        ["Elsewhere".into()].into_iter().collect(),
    )];
    stop.buses = vec![(other.id, true), (bus.id, true)];
    let id = h.people[0].id;
    let ty = h.people[0].ty.clone();
    let buses = [bus, other];
    let ix = buses.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let at = h.people[0].position;
    let world = FlatWorld::default();
    for count in 1..=2 {
        assert!(h.try_connection(
            0,
            at,
            0.0,
            Some(42),
            buses[(count - 1) % 2].id,
            &buses,
            &ix,
            &world
        ));
        assert_eq!(h.people.len(), 1);
        assert_eq!(h.people[0].id, id);
        assert!(Arc::ptr_eq(&h.people[0].ty, &ty));
        assert_eq!(h.pax(0).unwrap().transfer_count, count as u8);
        assert_eq!(h.pax(0).unwrap().task, Task::WalkingToBusstop);
        assert_eq!(
            h.pax(0).unwrap().transfer_from,
            Some(buses[(count - 1) % 2].id)
        );
    }
    assert!(!h.try_connection(0, at, 0.0, Some(42), buses[0].id, &buses, &ix, &world));
    assert_eq!(h.people[0].id, id);
}

#[test]
fn connections_need_a_service_and_disabled_path_does_not_draw_randomness() {
    let (mut h, bus) = super::super::pax_stops::brt_tests::setup("connections-service", None);
    let buses = [bus];
    let ix = [(buses[0].id, 0)].into_iter().collect();
    let at = h.people[0].position;
    let world = FlatWorld::default();
    let rng = h.rng;
    assert!(!h.try_connection(0, at, 0.0, Some(42), buses[0].id, &buses, &ix, &world));
    assert_eq!(h.rng, rng);
    h.transfer_percent = 100.0;
    let stop = h.stops.get_mut(&42).unwrap();
    stop.dests = vec![("Elsewhere".into(), 1.0)];
    stop.lines = vec![(
        "Elsewhere".into(),
        ["Elsewhere".into()].into_iter().collect(),
    )];
    assert!(!h.try_connection(0, at, 0.0, Some(42), buses[0].id, &buses, &ix, &world));
    assert_eq!(h.pax(0).unwrap().transfer_count, 0);
    assert_eq!(world.ground_samples.load(std::sync::atomic::Ordering::Relaxed), 0,
        "stops without valid onward service must not trigger terrain sampling");
}

#[test]
fn chance_zero_twenty_and_hundred_respect_draw_and_service() {
    let (mut h, mut bus) = super::super::pax_stops::brt_tests::setup("connections-chance", None);
    bus.entry_open.fill(true);
    let mut other = bus.clone();
    other.id = BusId::Ai(4);
    other.entry_open.fill(true);
    let stop = h.stops.get_mut(&42).unwrap();
    stop.dests = vec![("Elsewhere".into(), 1.0)];
    stop.lines = vec![("Elsewhere".into(), ["Elsewhere".into()].into_iter().collect())];
    stop.buses = vec![(other.id, true), (bus.id, true)];
    let buses = [bus, other];
    let ix = buses.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let at = h.people[0].position;
    let world = FlatWorld::default();

    h.transfer_percent = 0.0;
    let before = h.rng;
    assert!(!h.try_connection(0, at, 0.0, Some(42), buses[0].id, &buses, &ix, &world));
    assert_eq!(h.rng, before);
    assert_eq!(world.ground_samples.load(std::sync::atomic::Ordering::Relaxed), 0);

    h.transfer_percent = 20.0;
    let mut success_seed = None;
    let mut failure_seed = None;
    for seed in 1..=10000u64 {
        h.rng = seed;
        if h.rand_f() < 0.2 { success_seed.get_or_insert(seed); }
        else { failure_seed.get_or_insert(seed); }
        if success_seed.is_some() && failure_seed.is_some() { break; }
    }
    let fail = failure_seed.expect("20% should have a non-selected deterministic seed");
    h.rng = fail;
    let expected_next = { let _ = h.rand_f(); h.rng };
    h.rng = fail;
    assert!(!h.try_connection(0, at, 0.0, Some(42), buses[0].id, &buses, &ix, &world));
    assert_eq!(h.rng, expected_next, "rejected draw must consume exactly one RNG sample");

    h.rng = success_seed.expect("20% should have a selected deterministic seed");
    assert!(h.try_connection(0, at, 0.0, Some(42), buses[0].id, &buses, &ix, &world));
    assert_eq!(h.pax(0).unwrap().transfer_count, 1);

    h.transfer_percent = 100.0;
    assert!(h.try_connection(0, at, 0.0, Some(42), buses[1].id, &buses, &ix, &world));
    assert_eq!(h.pax(0).unwrap().transfer_count, 2);
}
