//! Bounded, ID-isolated relay for passengers carried or released by clients.
//! This uses the legacy LAN WORLDUP/DESCUP protocol without loading a renderer.

use glam::DVec3;
use omsi_net::world::{self, Desc, PersonPlace, PersonState, WorldFrame};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const RELAY_ID_START: u32 = 0xC0_0000;
const MAX_SOURCE_PEOPLE: usize = 96;
const MAX_SOURCE_FRAME_PEOPLE: usize = 128;
const MAX_FILE_LEN: usize = 260;
const OWNER_RADIUS: f64 = 300.0;
const OBSERVER_RADIUS: f64 = 260.0;
const BUS_RADIUS: f64 = 500.0;
const STALE_AFTER: Duration = Duration::from_secs(4);
const EVICT_AFTER: Duration = Duration::from_secs(20);

struct Rider {
    id: u32,
    file: String,
    state: Option<PersonState>,
    updated: Instant,
}

#[derive(Default)]
struct Source {
    position: DVec3,
    riders: HashMap<u32, Rider>,
}

#[derive(Default)]
pub struct ClientUpstream {
    sources: HashMap<u32, Source>,
    next_id: u32,
    pub rejected: u64,
}

fn file_ok(file: &str) -> bool {
    if file.is_empty() || file.len() > MAX_FILE_LEN || file.contains(':') {
        return false;
    }
    let slash = file.replace('\\', "/");
    !slash.starts_with('/')
        && slash
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && !slash.chars().any(char::is_control)
}

fn valid_state(owner: u32, near: DVec3, state: &PersonState) -> bool {
    if state.id > world::MAX_ID || !near.is_finite() {
        return false;
    }
    match state.place {
        PersonPlace::Foot {
            x,
            y,
            z,
            heading,
            speed,
            ..
        } => {
            [x, y, z, f64::from(heading), f64::from(speed)]
                .iter()
                .all(|v| v.is_finite() && v.abs() < 1.0e8)
                && speed >= 0.0
                && speed <= 20.0
                && (DVec3::new(x, y, z) - near).truncate().length() <= OWNER_RADIUS
        }
        PersonPlace::Aboard {
            bus,
            x,
            y,
            z,
            heading,
            ..
        } => {
            bus == (world::PLAYER_BUS | owner)
                && [x, y, z, heading]
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 100.0)
        }
    }
}

impl ClientUpstream {
    pub fn retain_peers(&mut self, positions: &HashMap<u32, DVec3>) {
        self.sources.retain(|id, _| positions.contains_key(id));
        for (&id, &position) in positions {
            self.sources.entry(id).or_default().position = position;
        }
        let now = Instant::now();
        for source in self.sources.values_mut() {
            source.riders.retain(|_, rider| {
                if now.duration_since(rider.updated) > STALE_AFTER {
                    rider.state = None;
                }
                now.duration_since(rider.updated) <= EVICT_AFTER
            });
        }
    }

    pub fn describe(&mut self, owner: u32, desc: Desc) {
        let Desc::Person { id, file } = desc;
        if id > world::MAX_ID || !file_ok(&file) {
            self.rejected += 1;
            return;
        }
        let Some(source) = self.sources.get_mut(&owner) else {
            self.rejected += 1;
            return;
        };
        if let Some(rider) = source.riders.get_mut(&id) {
            rider.file = file;
            return;
        }
        if source.riders.len() >= MAX_SOURCE_PEOPLE {
            self.rejected += 1;
            return;
        }
        if self.next_id == 0 {
            self.next_id = RELAY_ID_START;
        }
        if self.next_id > world::MAX_ID {
            self.rejected += 1;
            return;
        }
        let relayed = self.next_id;
        self.next_id += 1; // Never reuse an ID belonging to an earlier client.
        source.riders.insert(
            id,
            Rider {
                id: relayed,
                file,
                state: None,
                updated: Instant::now(),
            },
        );
    }

    pub fn frame(&mut self, owner: u32, frame: WorldFrame) {
        let Some(source) = self.sources.get_mut(&owner) else {
            self.rejected += 1;
            return;
        };
        for p in frame.people.into_iter().take(MAX_SOURCE_FRAME_PEOPLE) {
            if !valid_state(owner, source.position, &p) {
                self.rejected += 1;
                continue;
            }
            let Some(rider) = source.riders.get_mut(&p.id) else {
                // Unverified type/description: do not forward an unknown model.
                self.rejected += 1;
                continue;
            };
            let mut state = p;
            state.id = rider.id;
            if let PersonPlace::Foot {
                ref mut waiting, ..
            } = state.place
            {
                *waiting = None; // Clients may never claim relayed people.
            }
            rider.state = Some(state);
            rider.updated = Instant::now();
        }
        for (person, id) in frame.gone {
            if person {
                if let Some(rider) = source.riders.get_mut(&id) {
                    rider.state = None;
                    rider.updated = Instant::now();
                }
            }
        }
    }

    pub fn visible(&self, observer: u32, near: DVec3, limit: usize) -> Vec<(PersonState, String)> {
        let mut visible = Vec::new();
        for (&owner, source) in &self.sources {
            if owner == observer {
                continue;
            }
            for rider in source.riders.values() {
                let Some(state) = rider.state else { continue };
                if rider.updated.elapsed() > STALE_AFTER {
                    continue;
                }
                let nearby = match state.place {
                    PersonPlace::Foot { x, y, .. } => {
                        (DVec3::new(x, y, near.z) - near).length() <= OBSERVER_RADIUS
                    }
                    PersonPlace::Aboard { .. } => {
                        (source.position - near).truncate().length() <= BUS_RADIUS
                    }
                };
                if nearby {
                    visible.push((state, rider.file.clone()));
                    if visible.len() >= limit {
                        return visible;
                    }
                }
            }
        }
        visible
    }

    pub fn description(&self, id: u32) -> Option<String> {
        self.sources
            .values()
            .flat_map(|source| source.riders.values())
            .find(|rider| rider.id == id && rider.state.is_some())
            .map(|rider| rider.file.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_net::world::Activity;

    fn pos(x: f64) -> PersonState {
        PersonState {
            id: 7,
            activity: Activity::Walk,
            place: PersonPlace::Foot {
                x,
                y: 0.0,
                z: 0.0,
                heading: 0.0,
                speed: 1.0,
                waiting: None,
            },
        }
    }

    #[test]
    fn two_clients_with_the_same_local_id_get_distinct_host_ids() {
        let mut up = ClientUpstream::default();
        up.retain_peers(&HashMap::from([
            (1, DVec3::ZERO),
            (2, DVec3::ZERO),
            (3, DVec3::ZERO),
        ]));
        for peer in [1, 2] {
            up.describe(
                peer,
                Desc::Person {
                    id: 7,
                    file: "Humans/Example.hum".into(),
                },
            );
            up.frame(
                peer,
                WorldFrame {
                    people: vec![pos(2.0)],
                    ..Default::default()
                },
            );
        }
        let v = up.visible(3, DVec3::ZERO, 48);
        assert_eq!(v.len(), 2);
        assert_ne!(v[0].0.id, v[1].0.id);
        assert!(v.iter().all(|p| p.0.id >= RELAY_ID_START));
        assert_eq!(up.visible(1, DVec3::ZERO, 48).len(), 1);
    }

    #[test]
    fn foreign_bus_and_invalid_file_cannot_be_relayed() {
        let mut up = ClientUpstream::default();
        up.retain_peers(&HashMap::from([(1, DVec3::ZERO), (2, DVec3::ZERO)]));
        up.describe(
            1,
            Desc::Person {
                id: 7,
                file: "../secret.hum".into(),
            },
        );
        assert!(up.sources.get(&1).unwrap().riders.is_empty());
        up.describe(
            1,
            Desc::Person {
                id: 7,
                file: "Humans/Example.hum".into(),
            },
        );
        let foreign = PersonState {
            id: 7,
            activity: Activity::Sit,
            place: PersonPlace::Aboard {
                bus: world::PLAYER_BUS | 2,
                x: 0.0,
                y: 0.0,
                z: 0.0,
                heading: 0.0,
                seat: None,
            },
        };
        up.frame(
            1,
            WorldFrame {
                people: vec![foreign],
                ..Default::default()
            },
        );
        assert!(up.visible(2, DVec3::ZERO, 48).is_empty());
        assert!(up.rejected >= 2);
    }

    #[test]
    fn removal_and_disconnect_forget_the_relay_without_reusing_its_id() {
        let mut up = ClientUpstream::default();
        let clients = HashMap::from([(1, DVec3::ZERO), (2, DVec3::ZERO)]);
        up.retain_peers(&clients);
        up.describe(
            1,
            Desc::Person {
                id: 7,
                file: "Humans/Example.hum".into(),
            },
        );
        up.frame(
            1,
            WorldFrame {
                people: vec![pos(1.0)],
                ..Default::default()
            },
        );
        let id = up.visible(2, DVec3::ZERO, 48)[0].0.id;
        up.frame(
            1,
            WorldFrame {
                gone: vec![(true, 7)],
                ..Default::default()
            },
        );
        assert!(up.visible(2, DVec3::ZERO, 48).is_empty());
        up.retain_peers(&HashMap::from([(2, DVec3::ZERO)]));
        up.retain_peers(&clients);
        up.describe(
            1,
            Desc::Person {
                id: 7,
                file: "Humans/Example.hum".into(),
            },
        );
        up.frame(
            1,
            WorldFrame {
                people: vec![pos(1.0)],
                ..Default::default()
            },
        );
        assert!(up.visible(2, DVec3::ZERO, 48)[0].0.id > id);
    }
}
