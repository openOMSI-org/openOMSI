//! Authoritative pedestrian snapshots for the renderer-free host.
//!
//! Uses the existing WORLD and DESC protocols. Physics and population are
//! separate: experimental free-roam currently creates no pedestrians, but
//! this publisher is ready to handle future CPU-simulated pedestrians.

use crate::people_upstream::{ClientUpstream, RELAY_ID_START};
use glam::DVec3;
use omsi_net::traffic_table::TrafficTable;
use omsi_net::world::{self, Activity, Desc, PersonPlace, PersonState, WorldFrame};
use omsi_net::{LanSession, Role};
use omsi_sim::people::{LanPerson, PeopleSim};
use omsi_sim::spatial::SpatialIndex;
use std::collections::{HashMap, HashSet};

const PERSON_RADIUS: f64 = 260.0;
const MAX_PEOPLE_PER_CLIENT: usize = 96;
const MAX_WANTS_PER_CLIENT: usize = 64;
const MAX_GONE_PER_FRAME: usize = 63;
const MAX_RELAY_PER_CLIENT: usize = 48;

/// Keep nearby people in the interest set while they walk slowly near the
/// 96-person cut-off; farther people may still replace them when they approach.
fn visibility_distance(near: DVec3, position: DVec3, already_sent: bool) -> f64 {
    ((position - near).length() - if already_sent { 5.0 } else { 0.0 }).max(0.0)
}

#[derive(Default)]
struct ClientPeople {
    sent: HashMap<u32, (PersonState, f32)>,
    described: HashSet<u32>,
}

impl ClientPeople {
    fn frame(&mut self, visible: &[PersonState], dt: f32) -> WorldFrame {
        let mut frame = WorldFrame::default();
        let seen: HashSet<u32> = visible.iter().map(|p| p.id).collect();
        for state in visible {
            let due = self
                .sent
                .get(&state.id)
                .is_none_or(|(old, age)| old != state || *age >= world::STANDING_EVERY);
            if due {
                self.sent.insert(state.id, (*state, 0.0));
                frame.people.push(*state);
            } else if let Some((_, age)) = self.sent.get_mut(&state.id) {
                *age += dt;
            }
        }
        let gone: Vec<u32> = self
            .sent
            .keys()
            .filter(|id| !seen.contains(id))
            .take(MAX_GONE_PER_FRAME)
            .copied()
            .collect();
        for id in gone {
            self.sent.remove(&id);
            self.described.remove(&id);
            frame.gone.push((true, id));
        }
        frame
    }
}

#[derive(Default)]
pub struct HeadlessPeopleSync {
    elapsed: f32,
    clients: HashMap<u32, ClientPeople>,
    pub sent_bytes: u64,
    upstream: ClientUpstream,
    /// Client updates rejected when passenger simulation is disabled.
    pub ignored_client_people: u64,
}

fn state_of(person: &LanPerson, traffic: &TrafficTable) -> Option<PersonState> {
    if person.id >= RELAY_ID_START
        || !person.pos.is_finite()
        || !person.heading.is_finite()
        || !person.speed.is_finite()
        || person.pos.x.abs() > 1.0e8
        || person.pos.y.abs() > 1.0e8
        || person.pos.z.abs() > 1.0e8
    {
        return None;
    }
    let place = match person.aboard {
        Some((bus, local, heading, seat)) => {
            // A client cannot materialize an AI rider until the bus has an
            // acknowledged traffic row. Never invent one or use raw sim IDs.
            let bus = traffic.id_of(bus)?.raw();
            if !local.is_finite() || !heading.is_finite() {
                return None;
            }
            PersonPlace::Aboard {
                bus,
                x: local.x,
                y: local.y,
                z: local.z,
                heading: heading as f32,
                seat: seat.map(|index| index.min(254) as u8),
            }
        }
        None => PersonPlace::Foot {
            x: person.pos.x,
            y: person.pos.y,
            z: person.pos.z,
            heading: person.heading as f32,
            speed: person.speed.max(0.0).min(20.0) as f32,
            waiting: person
                .waiting
                .filter(|(stop, _)| (0..=u32::MAX as i64).contains(stop))
                .map(|(stop, index)| (stop, index.min(255) as u8)),
        },
    };
    Some(PersonState {
        id: person.id,
        activity: omsi_lan::mirror::net_activity(person.activity),
        place,
    })
}

impl HeadlessPeopleSync {
    /// Send descriptions and changed positions only to players within the
    /// original host's interest radius. Late joiners receive full snapshots;
    /// despawns and out-of-range people receive GONE notifications.
    pub fn tick(
        &mut self,
        lan: &mut LanSession,
        people: &PeopleSim,
        table: &TrafficTable,
        dt: f32,
        allow_client_people: bool,
    ) -> usize {
        if lan.role != Role::Host || !dt.is_finite() || dt <= 0.0 {
            return 0;
        }
        // Upstream person state belongs to the driver's local vehicle.
        // Walking peers are observers, not authoritative rider uploaders.
        let positions: HashMap<u32, DVec3> = lan
            .peers()
            .filter(|p| p.has_info && p.has_pose && p.pose.has_vehicle())
            .filter_map(|p| crate::interest::client_position(&p.pose).map(|at| (p.pose.id, at)))
            .collect();
        self.upstream.retain_peers(&positions);
        let descs = lan.take_descs_up();
        let frames = lan.take_world_up();
        if allow_client_people {
            for (peer, desc) in descs {
                self.upstream.describe(peer, desc);
            }
            for (peer, _, frame) in frames {
                self.upstream.frame(peer, frame);
            }
        } else {
            self.ignored_client_people += (descs.len() + frames.len()) as u64;
        }
        // Same recovery channel as the graphical host when a DESC was lost.
        for (id, wants) in lan.take_wants() {
            for wanted in wants.into_iter().take(MAX_WANTS_PER_CLIENT) {
                if !wanted.person {
                    continue;
                }
                if let Some(file) = self.upstream.description(wanted.id) {
                    lan.send_desc(
                        id,
                        &Desc::Person {
                            id: wanted.id,
                            file,
                        },
                    );
                } else if let Some(ty) = people.lan_people_by_id(wanted.id) {
                    lan.send_desc(
                        id,
                        &Desc::Person {
                            id: wanted.id,
                            file: PeopleSim::type_file(&ty),
                        },
                    );
                }
            }
        }
        self.elapsed += dt;
        if self.elapsed < world::MOVING_EVERY {
            return 0;
        }
        self.elapsed = (self.elapsed - world::MOVING_EVERY).min(world::MOVING_EVERY);
        let clients: Vec<_> = lan
            .peers()
            .filter(|peer| peer.has_info && peer.has_pose)
            // A walking client publishes its position in Walker, not Pose.
            // Use the same validated interest coordinates as AI and tiles.
            .filter_map(|peer| {
                crate::interest::client_position(&peer.pose).map(|at| (peer.pose.id, at))
            })
            .collect();
        let live: HashSet<u32> = clients.iter().map(|(id, _)| *id).collect();
        self.clients.retain(|id, _| live.contains(id));
        if clients.is_empty() {
            return 0;
        }
        // The native host can have thousands of pedestrians and dozens of
        // observers. Build the broad-phase once and reuse it across players.
        // The people layer still performs the original exact 3D test.
        let person_index = SpatialIndex::from_positions(
            people
                .people
                .iter()
                .map(|person| person.position.truncate()),
        );
        let mut candidates = Vec::new();
        let mut bytes = 0;
        for (id, near) in clients {
            let view = self.clients.entry(id).or_default();
            person_index.within(near.truncate(), PERSON_RADIUS, &mut candidates);
            let nearby = people.lan_people_subset(near, PERSON_RADIUS, &candidates);
            // Enforce the per-client bound BEFORE emitting descriptions.
            // Previously filter_map() sent DESC for every nearby pedestrian
            // and only then take(96) capped the WORLD snapshot.
            let relayed = if allow_client_people {
                self.upstream.visible(id, near, MAX_RELAY_PER_CLIENT)
            } else {
                Vec::new()
            };
            // Select closest people first, with a small retention margin for
            // already-drawn IDs. Iteration order of the population can change
            // between ticks; taking the first 96 made real nearby people vanish.
            let mut eligible: Vec<_> = nearby
                .iter()
                .filter_map(|person| state_of(person, table).map(|state| (person, state)))
                .collect();
            eligible.sort_unstable_by(|(a, sa), (b, sb)| {
                let score =
                    |p: &LanPerson| visibility_distance(near, p.pos, view.sent.contains_key(&p.id));
                score(a)
                    .total_cmp(&score(b))
                    .then_with(|| sa.id.cmp(&sb.id))
            });
            let mut visible: Vec<_> = eligible
                .into_iter()
                .take(MAX_PEOPLE_PER_CLIENT.saturating_sub(relayed.len()))
                .map(|(person, state)| {
                    if view.described.insert(state.id) {
                        lan.send_desc(
                            id,
                            &Desc::Person {
                                id: state.id,
                                file: PeopleSim::type_file(&person.ty),
                            },
                        );
                    }
                    state
                })
                .collect();
            for (state, file) in relayed {
                if view.described.insert(state.id) {
                    lan.send_desc(id, &Desc::Person { id: state.id, file });
                }
                visible.push(state);
            }
            let frame = view.frame(&visible, world::MOVING_EVERY);
            if !frame.people.is_empty() || !frame.gone.is_empty() {
                bytes += lan.send_world(id, &frame);
            }
        }
        self.sent_bytes += bytes as u64;
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(id: u32, x: f64) -> PersonState {
        PersonState {
            id,
            activity: Activity::Walk,
            place: PersonPlace::Foot {
                x,
                y: 3.0,
                z: 0.0,
                heading: 90.0,
                speed: 1.0,
                waiting: None,
            },
        }
    }

    #[test]
    fn walkers_observe_pedestrians_at_their_actual_walker_coordinates() {
        let walking = omsi_net::Pose {
            x: 0.0,
            walker: Some(omsi_net::Walker {
                x: 4_000.0,
                y: 12.0,
                z: 2.0,
                ..Default::default()
            }),
            ..Default::default()
        };
        let at = crate::interest::client_position(&walking).unwrap();
        assert_eq!(at, DVec3::new(4_000.0, 12.0, 2.0));
        let invalid = omsi_net::Pose {
            walker: Some(omsi_net::Walker {
                x: f64::NAN,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(crate::interest::client_position(&invalid).is_none());
    }

    #[test]
    fn late_join_gets_first_snapshot_and_only_changed_poses_follow() {
        let mut view = ClientPeople::default();
        assert_eq!(
            view.frame(&[person(12, 2.0)], 0.1).people,
            vec![person(12, 2.0)]
        );
        assert!(view.frame(&[person(12, 2.0)], 0.1).people.is_empty());
        assert_eq!(
            view.frame(&[person(12, 4.0)], 0.1).people,
            vec![person(12, 4.0)]
        );
    }

    #[test]
    fn leaving_range_sends_a_single_removal_and_clears_description() {
        let mut view = ClientPeople::default();
        view.described.insert(12);
        view.frame(&[person(12, 2.0)], 0.1);
        assert_eq!(view.frame(&[], 0.1).gone, vec![(true, 12)]);
        assert!(view.frame(&[], 0.1).gone.is_empty());
        assert!(!view.described.contains(&12));
        assert_eq!(view.frame(&[person(12, 2.0)], 0.1).people.len(), 1);
    }

    #[test]
    fn crowded_stop_retains_nearby_people_regardless_of_population_order() {
        let near = DVec3::ZERO;
        let mut people: Vec<(u32, DVec3, bool)> = (0..150)
            .map(|id| (id, DVec3::new(30.0 + f64::from(id), 0.0, 0.0), id < 96))
            .collect();
        people.reverse(); // A population reorder must not change which 96 are visible.
        people.sort_unstable_by(|a, b| {
            visibility_distance(near, a.1, a.2)
                .total_cmp(&visibility_distance(near, b.1, b.2))
                .then_with(|| a.0.cmp(&b.0))
        });
        assert_eq!(people[0].0, 0);
        assert!(people.iter().take(96).all(|p| p.0 < 96));
        assert_eq!(people.iter().take(96).count(), MAX_PEOPLE_PER_CLIENT);
        assert!(
            visibility_distance(near, DVec3::new(20.0, 0.0, 0.0), true)
                < visibility_distance(near, DVec3::new(18.0, 0.0, 0.0), false)
        );
    }

    #[test]
    fn visibility_limit_also_caps_description_candidates() {
        let visible: Vec<_> = (0..200).map(|id| person(id, id as f64)).collect();
        let limited: Vec<_> = visible
            .iter()
            .take(MAX_PEOPLE_PER_CLIENT)
            .map(|p| p.id)
            .collect();
        assert_eq!(limited.len(), MAX_PEOPLE_PER_CLIENT);
        assert_eq!(*limited.last().unwrap(), 95);
        let mut view = ClientPeople::default();
        assert_eq!(
            view.frame(&visible[..MAX_PEOPLE_PER_CLIENT], 0.1)
                .people
                .len(),
            96
        );
        assert!(
            view.frame(&visible[..MAX_PEOPLE_PER_CLIENT], 0.1)
                .gone
                .is_empty()
        );
    }

    #[test]
    fn removals_are_bounded_to_the_wire_protocol_limit() {
        let mut view = ClientPeople::default();
        let people: Vec<_> = (0..90).map(|id| person(id, id as f64)).collect();
        view.frame(&people, 0.1);
        assert_eq!(view.frame(&[], 0.1).gone.len(), MAX_GONE_PER_FRAME);
        assert_eq!(view.frame(&[], 0.1).gone.len(), 90 - MAX_GONE_PER_FRAME);
    }
}
