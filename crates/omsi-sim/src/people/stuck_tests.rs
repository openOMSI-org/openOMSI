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
    traffic: crate::ai_traffic::TrafficSim,
    dt: f32,
}

impl Stuck {
    fn new() -> Stuck {
        let mut world = Bare::default();
        let mut collision = crate::collision::CollisionWorld::default();
        collision.add(crate::collision::Obb {
            center: DVec2::new(0.0, 2.5),
            half: DVec2::new(50.0, 0.25),
            heading: 0.0,
            z0: -1.0,
            z1: 3.0,
            velocity: DVec2::ZERO,
            mass: 0.0,
            pole: None,
            id: 1,
        });
        world.collision = Arc::new(collision);
        let net = pavement();
        let mut sim = PeopleSim::new(Path::new("/nonexistent"), 40);
        sim.players_only = true;
        sim.max_humans = 1;
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
        sim.spawn(
            &world,
            DVec3::new(0.0, 2.0, 0.0),
            0.0,
            State::Strolling(PedWalk::new(vec![leg], true, 0.0)),
        )
        .expect("a person to walk");
        let traffic = crate::ai_traffic::TrafficSim::assemble(
            Path::new("/nonexistent"),
            net.clone(),
            0,
            Vec::new(),
            HashMap::new(),
            (Vec::new(), Vec::new()),
            Vec::new(),
            (1.0, 0),
            0,
        );
        Stuck {
            sim,
            world,
            net,
            traffic,
            dt: 1.0 / 30.0,
        }
    }

    /// One frame driven through normal people update (`tick_inner`).
    fn frame(&mut self, moved: f64) {
        if moved == 0.0 {
            self.sim
                .tick_inner(self.dt, &self.world, None, Some(&self.traffic));
        } else {
            let saved = self.world.collision.clone();
            self.world.collision = Arc::new(crate::collision::CollisionWorld::default());
            self.sim
                .tick_inner(self.dt, &self.world, None, Some(&self.traffic));
            self.world.collision = saved;
        }
    }
}

#[test]
fn two_minutes_of_trampling_in_one_place_gets_a_pedestrian_deleted() {
    let mut s = Stuck::new();
    // a minute and a half: trampling, but not yet deleted
    for _ in 0..(90.0 / s.dt) as usize {
        s.frame(0.0);
    }
    assert_eq!(s.sim.people.len(), 1);
    let walked = s.sim.people[0].stuck_total;
    assert!(
        (walked - 90.0).abs() < 2.0,
        "wanting to walk for {walked:.1} s"
    );
    // Collision assigns nonzero detour velocity, but actual progress is blocked by wall
    assert!(s.sim.people[0].vel.length() > 0.0);

    // two minutes and a bit: normal update deletes them where they stand
    for _ in 0..(35.0 / s.dt) as usize {
        s.frame(0.0);
    }
    assert!(
        s.sim.people.is_empty(),
        "deleted by normal update after two minutes"
    );
}

#[test]
fn a_step_frees_a_pedestrian_the_crowd_only_held_up() {
    let mut s = Stuck::new();
    // a jam that moves in bursts - half a second of walking, a second standing, over and
    // over: ten minutes in the crowd and never two minutes in one place
    for n in 0..(600.0 / s.dt) as usize {
        s.frame(if n % 45 < 15 { 1.0 } else { 0.0 });
    }
    assert_eq!(s.sim.people.len(), 1);
    let held = s.sim.people[0].stuck_total;
    assert!(
        held < 2.0,
        "the crowd let them through, but {held:.1} s stand at the end"
    );
}

#[test]
fn a_pedestrian_standing_still_on_purpose_is_not_the_maps_fault() {
    let mut s = Stuck::new();
    s.sim.people[0].activity = Activity::Stand;
    s.sim.people[0].pace = 0.0;
    if let State::Strolling(ref mut w) = s.sim.people[0].state {
        w.legs.clear();
    }
    for _ in 0..(600.0 / s.dt) as usize {
        s.frame(0.0);
    }
    assert_eq!(s.sim.people.len(), 1);
    assert_eq!(s.sim.people[0].stuck_total, 0.0);
}

#[test]
fn onboard_passenger_stuck_counter_uses_bus_local_coordinates() {
    let mut s = Stuck::new();
    let bus_id = BusId::Player;
    let local_pos = Vec3::new(0.5, 2.0, 0.0);
    s.sim.people[0].place = Place::Bus(bus_id, local_pos);
    s.sim.people[0].position = DVec3::new(500.0, 1000.0, 0.0);

    let want = Want {
        vel: DVec2::new(0.0, 1.1),
        face: None,
        give: 1.0,
        corridor: None,
        idle: Activity::Stand,
    };
    let w = Walker::new(DVec2::new(local_pos.x as f64, local_pos.y as f64), 0.25, 0);
    s.sim.apply(
        0,
        &w,
        &want,
        s.dt,
        &s.world,
        Some(&s.net),
        &[],
        &HashMap::new(),
    );

    assert!(
        (s.sim.people[0].stuck - s.dt).abs() < 1e-4,
        "onboard stuck counter should build up: {}",
        s.sim.people[0].stuck
    );
}
