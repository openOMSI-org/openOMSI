//! Rail points in a LAN game. The host's game keeps them, as it keeps the traffic lights:
//! each player's game sets the points around it as the host's lie (they go with the world
//! frames, `WorldFrame::points`). Points a player's game throws itself (its train runs over
//! them, the J key, a click) go up to the host, and from there to the others.

use glam::DVec3;
use hashbrown::HashMap;
use omsi_net::world::{MAX_POINTS, WorldFrame};
use omsi_net::{LanSession, Role};
use omsi_sim::scenery::SceneryState;

/// How far around a player its game is told the host's points (m).
pub const RADIUS: f64 = 600.0;
/// How often the host looks at the points around the players, and a player's game at its
/// own (s).
const LOOK_EVERY: f32 = 0.25;
/// How long points a player's game threw keep its setting against the host's (s): the host
/// may still have sent the old one before it took the new.
const HOLD: f32 = 3.0;
/// The most a player's game sends up at once.
const MAX_UP: usize = 32;

#[derive(Default)]
pub struct PointsSync {
    acc: f32,
    /// Host: what the players' games threw, to set here.
    up: Vec<(i64, f32)>,
    /// Host: the points around each player, for its next world frame.
    around: HashMap<u32, Vec<(i64, f32)>>,
    /// Player: the host's points as they came.
    down: Vec<(i64, f32)>,
    /// Player: the points as this game last had them from the host or sent them up, with
    /// the key their object was placed with.
    known: HashMap<i64, (u64, f32)>,
    /// Player: points this game threw, with the value and the hold left (s).
    own: HashMap<i64, (f32, f32)>,
}

impl PointsSync {
    /// Host: points a player's world frame carried.
    pub fn came_up(&mut self, points: &[(i64, f32)]) {
        let room = (MAX_POINTS * 4).saturating_sub(self.up.len());
        self.up.extend(points.iter().take(room).copied());
    }

    /// Player: points the host's world frame carried.
    pub fn came_down(&mut self, points: &[(i64, f32)]) {
        let room = (MAX_POINTS * 4).saturating_sub(self.down.len());
        self.down.extend(points.iter().take(room).copied());
    }

    /// Host: the points around player `id`, for its world frame.
    pub fn around(&self, id: u32) -> Vec<(i64, f32)> {
        self.around.get(&id).cloned().unwrap_or_default()
    }

    /// Set what came, and look at the points (`me`: where our vehicle is).
    pub fn step(
        &mut self,
        lan: &mut LanSession,
        scenery: &mut SceneryState,
        me: Option<DVec3>,
        dt: f32,
    ) {
        match lan.role {
            Role::Host => {
                for (id, value) in self.up.drain(..) {
                    scenery.set_switch_value(id, value);
                }
            }
            Role::Client => self.take_host(scenery),
        }
        self.acc += dt;
        if self.acc < LOOK_EVERY {
            return;
        }
        let dt = std::mem::take(&mut self.acc);
        match lan.role {
            Role::Host => {
                self.around.clear();
                for p in lan.peers().filter(|p| p.has_pose && p.pose.has_vehicle()) {
                    let at = DVec3::new(p.pose.x, p.pose.y, p.pose.z);
                    self.around.insert(p.pose.id, near(scenery, at));
                }
            }
            Role::Client => {
                let Some(me) = me else { return };
                let up = self.own_changes(scenery, me, dt);
                if !up.is_empty() {
                    lan.send_world_up(&WorldFrame {
                        points: up,
                        ..Default::default()
                    });
                }
            }
        }
    }

    /// The host's points, except those this game threw and the host has not taken yet.
    fn take_host(&mut self, scenery: &mut SceneryState) {
        for (id, value) in std::mem::take(&mut self.down) {
            if let Some(&(mine, _)) = self.own.get(&id) {
                if mine != value {
                    continue;
                }
                self.own.remove(&id);
            }
            if scenery.set_switch_value(id, value) {
                if let Some(k) = self.known.get_mut(&id) {
                    k.1 = value;
                }
            }
        }
    }

    /// The points around `me` this game threw since it last looked, and those the host has
    /// not taken yet: to send up.
    fn own_changes(&mut self, scenery: &SceneryState, me: DVec3, dt: f32) -> Vec<(i64, f32)> {
        for (id, key, value) in scenery.switch_values_near(me, RADIUS) {
            match self.known.insert(id, (key, value)) {
                Some(was) if was.0 == key && was.1 != value => {
                    self.own.insert(id, (value, HOLD));
                }
                _ => {}
            }
        }
        self.own.retain(|_, (_, hold)| {
            *hold -= dt;
            *hold > 0.0
        });
        self.own
            .iter()
            .take(MAX_UP)
            .map(|(&id, &(value, _))| (id, value))
            .collect()
    }
}

/// The points within `RADIUS` of `at` that fit the world frame (ids of 32 bits).
fn near(scenery: &SceneryState, at: DVec3) -> Vec<(i64, f32)> {
    let mut v: Vec<(i64, f32)> = scenery
        .switch_values_near(at, RADIUS)
        .into_iter()
        .filter(|(id, _, _)| (0..=u32::MAX as i64).contains(id))
        .map(|(id, _, value)| (id, value))
        .collect();
    v.truncate(MAX_POINTS);
    v
}

#[cfg(test)]
#[path = "points_tests.rs"]
mod tests;
