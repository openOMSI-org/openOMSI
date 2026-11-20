//! #3790: a pedestrian no step can free is deleted after two minutes of walking in place
//! (a zebra whose kerb is too close, a bus standing over the crossing), while one that the
//! crowd only held up for a moment goes on walking.

use super::walk::STUCK_GONE_S;
use super::*;
use std::path::Path;

/// Nothing loaded but the ground under everybody: no walls, no parked cars, no stops.
#[derive(Default)]
struct Bare {
    collision: Arc<crate::collision::CollisionWorld>,
    parked: Arc<Vec<crate::collision::Obb>>,
    objects: crate::world_data::ObjectPlaces,
}

impl World for Bare {
    fn root(&self) -> &Path {
        Path::new("/nonexistent")
    }
    fn map_dir(&self) -> &Path {
        self.root()
    }
    fn walk_height(&self, _: f64, _: f64) -> Option<f64> {
        Some(0.0)
    }
    fn walk_height_near(&self, _: f64, _: f64, _: f64) -> Option<f64> {
        Some(0.0)
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

/// One pavement path north along y, wide enough to walk.
fn pavement() -> Network {
    let mut net = Network::default();
    net.lanes.push(crate::traffic::LaneBuilder::polyline(
        vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 20.0, 0.0)],
        LaneKind::Sidewalk,
        2.5,
    ));
    net.link(1.5);
    net
}

/// A pedestrian set on walking up the pavement, exactly where `STUCK_GONE_S` says.
struct Stuck {
    sim: PeopleSim,
    world: Bare,
    net: Network,
    i: usize,
    want: Want,
    dt: f32,
}

impl Stuck {
    fn new() -> Stuck {
        let world = Bare::default();
        let net = pavement();
        let mut sim = PeopleSim::new(Path::new("/nonexistent"), 40);
        // one figure to put on the pavement (the map's own `.hum`s are not here)
        // (a folder of its own: the tests of this file run side by side)
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("omsi-stuck-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("person.hum"), "[model]\nmodel.cfg\n").unwrap();
        std::fs::write(dir.join("model.cfg"), "").unwrap();
        sim.types
            .push(Arc::new(HumanType::load(&dir.join("person.hum")).unwrap()));
        let leg = Leg {
            lane: 0,
            a: 0.0,
            b: 20.0,
        };
        let i = sim
            .spawn(
                &world,
                DVec3::new(0.0, 2.0, 0.0),
                0.0,
                State::Strolling(PedWalk::new(vec![leg], true, 0.0)),
            )
            .expect("a person to walk");
        // what the walk asks for while it goes up the pavement
        let want = Want {
            vel: DVec2::new(0.0, 1.1),
            face: None,
            give: 1.0,
            corridor: None,
            idle: Activity::Stand,
        };
        Stuck {
            sim,
            world,
            net,
            i,
            want,
            dt: 1.0 / 30.0,
        }
    }

    /// One frame of the crowd's result: `moved` metres of real progress per frame (0 = held
    /// in place by whatever the map put there).
    fn frame(&mut self, moved: f64) {
        let mut walker = Walker::new(DVec2::new(0.0, 2.0), 0.25, 0);
        walker.vel = DVec2::new(0.0, moved);
        let (world, net, i) = (&self.world, &self.net, self.i);
        self.sim.apply(
            i,
            &walker,
            &self.want,
            self.dt,
            world,
            Some(net),
            &[],
            &HashMap::new(),
        );
    }

    fn decide(&mut self) -> Vec<usize> {
        let mut remove = Vec::new();
        let (world, net, i) = (&self.world, &self.net, self.i);
        self.sim
            .decide(i, self.dt, world, Some(net), None, &[], &mut remove);
        remove
    }
}

#[test]
fn two_minutes_of_trampling_in_one_place_gets_a_pedestrian_deleted() {
    let mut s = Stuck::new();
    // a minute and a half: trampling, but not yet deleted
    for _ in 0..(90.0 / s.dt) as usize {
        s.frame(0.0);
    }
    let walked = s.sim.people[s.i].stuck_total;
    assert!(
        (walked - 90.0).abs() < 1.0,
        "wanting to walk for {walked:.1} s"
    );
    assert!(
        s.decide().is_empty(),
        "a minute and a half is not two minutes"
    );

    // two minutes and a bit: gone rather than walking in place for the rest of the duty
    for _ in 0..(35.0 / s.dt) as usize {
        s.frame(0.0);
    }
    assert!(
        s.sim.people[s.i].stuck_total > STUCK_GONE_S,
        "{}",
        s.sim.people[s.i].stuck_total
    );
    assert_eq!(s.decide(), vec![s.i], "deleted where it stands");
    assert_eq!(s.sim.people[s.i].why, "stuck for two minutes");
}

#[test]
fn a_step_frees_a_pedestrian_the_crowd_only_held_up() {
    let mut s = Stuck::new();
    // a jam that moves in bursts - half a second of walking, a second standing, over and
    // over: ten minutes in the crowd and never two minutes in one place
    for n in 0..(600.0 / s.dt) as usize {
        s.frame(if n % 45 < 15 { 1.0 } else { 0.0 });
    }
    // (a second of standing since the last burst, no more: every burst starts it again)
    let held = s.sim.people[s.i].stuck_total;
    assert!(
        held < 2.0,
        "the crowd let them through, but {held:.1} s stand at the end"
    );
    assert!(s.decide().is_empty(), "nobody who can walk is deleted");
}

#[test]
fn a_pedestrian_standing_still_on_purpose_is_not_the_maps_fault() {
    let mut s = Stuck::new();
    // nothing wants to move (the walk is over, they stand face to the bus): no count at all
    s.want.vel = DVec2::ZERO;
    for _ in 0..(600.0 / s.dt) as usize {
        s.frame(0.0);
    }
    assert_eq!(s.sim.people[s.i].stuck_total, 0.0);
    assert!(s.decide().is_empty());
}
