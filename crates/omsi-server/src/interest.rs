//! Headless authoritative interest and passenger-claim processing.
//!
//! Production TrafficSim and PeopleSim are driven by the connected clients'
//! actual locations, not by a nonexistent dedicated-server camera. Only the
//! host may process boarding claims; the transport acknowledges each claim
//! using the existing network protocol and ownership rules.

use crate::passenger_dispatch::PassengerDispatch;
use glam::DVec3;
use omsi_net::{LanSession, Pose, Role};
use omsi_sim::ai_traffic::TrafficSim;
use omsi_sim::ai_traffic::light_paths::box_outline;
use omsi_sim::ai_traffic::model::PlayerBox;
use omsi_sim::ai_traffic::viewer::Viewer;
use omsi_sim::people::PeopleSim;
use std::collections::{HashMap, HashSet};

const BOARDING_CLAIM_RADIUS: f64 = 80.0;
const BOARDING_CLAIM_VERTICAL: f64 = 10.0;
const MAX_BOARDING_CLAIMS: usize = 64;

/// A bus owner may claim only waiting people in the same local stop area.
/// Validate on the host; possession of a remote passenger ID alone grants no
/// right to steal a person from another player's stop.
fn can_claim_at(bus: DVec3, person: DVec3) -> bool {
    bus.is_finite()
        && person.is_finite()
        && (bus - person).truncate().length() <= BOARDING_CLAIM_RADIUS
        && (bus.z - person.z).abs() <= BOARDING_CLAIM_VERTICAL
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InterestUpdate {
    pub visible_players: usize,
    pub boarding_claims: usize,
    pub granted: usize,
    pub denied: usize,
}

/// Ignore non-finite or clearly invalid client state before passing it to
/// pathfinding, collision or pedestrian visibility. Legacy clients with no
/// supplied vehicle box get the same conservative car/bus fallback as the
/// graphical host until the vehicle's exact model is available server-side.

pub(crate) fn client_position(pose: &Pose) -> Option<DVec3> {
    let (position, heading) = if pose.has_vehicle() {
        (DVec3::new(pose.x, pose.y, pose.z), pose.heading)
    } else {
        // On-foot peers publish their world position in Walker, not Pose.
        let walker = pose.walker?;
        (DVec3::new(walker.x, walker.y, walker.z), walker.heading)
    };
    (position.is_finite()
        && heading.is_finite()
        && position.to_array().iter().all(|v| v.abs() <= 1.0e8))
    .then_some(position)
}

fn vehicle_box(pose: &Pose) -> Option<(DVec3, PlayerBox)> {
    if !pose.has_vehicle() {
        return None;
    }
    let position = client_position(pose)?;
    let width = if pose.width.is_finite() && pose.width > 0.1 {
        pose.width.clamp(1.0, 5.0)
    } else {
        2.5
    };
    let length = if pose.length.is_finite() && pose.length > 0.1 {
        pose.length.clamp(2.5, 40.0)
    } else {
        11.0
    };
    let offset = if pose.box_offset.is_finite() {
        pose.box_offset.clamp(-20.0, 20.0)
    } else {
        0.0
    };
    let speed = if pose.speed_kmh.is_finite() {
        (pose.speed_kmh / 3.6).clamp(-30.0, 90.0)
    } else {
        0.0
    };
    let body = box_outline(
        position,
        f64::from(pose.heading),
        [width, length, 3.0, 0.0, offset, 1.5],
        speed,
    );
    Some((position, body))
}

/// Update both simulations before their physics tick and grant/deny passengers
/// atomically at the server. This does not serialize LAN world frames or
/// handle vehicle content: those remain separate, security-checked services.
pub fn update_host_interest(
    lan: &mut LanSession,
    traffic: &mut TrafficSim,
    people: &mut PeopleSim,
) -> InterestUpdate {
    let mut dispatch = PassengerDispatch::default();
    update_host_interest_with_dispatch(lan, traffic, people, &mut dispatch)
}

/// Persistent passenger ledger belongs to the native CpuSimulation, not to
/// the transport or a single ephemeral CLAIM datagram.
pub(crate) fn update_host_interest_with_dispatch(
    lan: &mut LanSession,
    traffic: &mut TrafficSim,
    people: &mut PeopleSim,
    dispatch: &mut PassengerDispatch,
) -> InterestUpdate {
    if lan.role != Role::Host {
        return InterestUpdate::default();
    }
    let mut summary = InterestUpdate::default();
    traffic.lan_centers.clear();
    traffic.lan_eyes.clear();
    traffic.others.clear();
    traffic.other_blinkers.clear();
    people.lan_centers.clear();

    for peer in lan.peers() {
        if !peer.has_pose || !peer.has_info {
            continue;
        }
        let Some(position) = client_position(&peer.pose) else {
            continue;
        };
        summary.visible_players += 1;
        traffic.lan_centers.push(position);
        let heading = peer
            .pose
            .walker
            .filter(|_| !peer.pose.has_vehicle())
            .map_or(peer.pose.heading, |walker| walker.heading);
        traffic
            .lan_eyes
            .push(Viewer::lan_player(position, f64::from(heading)));
        // Pedestrians and the tile streamer follow players on foot too,
        // but do not pretend that a walking client owns a collision bus.
        if let Some((_, body)) = vehicle_box(&peer.pose) {
            traffic.others.push((peer.pose.id, body));
            traffic
                .other_blinkers
                .insert(peer.pose.id, peer.pose.blinker);
        }
        people.lan_centers.push(position);
    }

    // Process authenticated BOARD/RETURN messages before pruning a departed
    // peer's ledger: a final UDP acknowledgement may arrive on the same tick
    // as a disconnect. GRANT is never proof of physical boarding.
    for (player, ids) in lan.take_pax_boarded() {
        dispatch.confirm_boarded(player, &ids);
        lan.acknowledge_returns(player, &ids);
    }
    for (player, ids) in lan.take_pax_returns() {
        let restored = dispatch.return_unboarded(player, &ids, people);
        lan.acknowledge_returns(player, &restored);
    }
    // Pose.passengers remains a player-reported count, not the GRANT count.
    // Restore abandoned escrow only when the owning connection is truly gone.
    dispatch.observe(lan, people);
    // Keep the authoritative claim decision on the same physics tick as
    // PeopleSim::hand_over. Never trust a caller-supplied passenger ID alone.
    let claims = lan.take_claims();
    if claims.is_empty() {
        return summary;
    }
    // No allocation or ID/peer scan on ordinary 60 Hz ticks without claims.
    let buses: HashMap<u32, DVec3> = lan
        .peers()
        .filter(|peer| peer.has_pose && peer.has_info && peer.pose.has_vehicle())
        .filter_map(|peer| client_position(&peer.pose).map(|pos| (peer.pose.id, pos)))
        .collect();
    let mut passenger_positions: HashMap<u32, DVec3> =
        people.people.iter().map(|p| (p.id, p.position)).collect();
    // Only genuinely waiting people with a known physical platform belong
    // to the fair-share pool. Other people stay subject to hand_over checks.
    let mut waiting = HashMap::new();
    let mut stop_counts: HashMap<i64, usize> = HashMap::new();
    for (index, person) in people.people.iter().enumerate() {
        if let Some(pax) = people.pax(index) {
            if pax.task == omsi_sim::people::pax::Task::WaitingForBus {
                if let Some(stop) = pax.stop {
                    waiting.insert(person.id, stop);
                    *stop_counts.entry(stop).or_default() += 1;
                }
            }
        }
    }
    let stop_positions: HashMap<i64, DVec3> = people
        .stops
        .iter()
        .map(|(&id, stop)| (id, stop.pos))
        .collect();
    for (player, requested) in claims {
        summary.boarding_claims += requested.len();
        let eligible: Vec<u32> = if let Some(&bus) = buses.get(&player) {
            requested
                .iter()
                .copied()
                .take(MAX_BOARDING_CLAIMS)
                .filter(|id| {
                    passenger_positions
                        .get(id)
                        .is_some_and(|&person| can_claim_at(bus, person))
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut granted = dispatch.repeat_grants(player, &requested, &stop_positions);
        let coordinated =
            dispatch.choose(player, &eligible, &waiting, &stop_counts, &stop_positions);
        let saved = people.hand_over_saved(player, &coordinated);
        let newly_granted: Vec<u32> = saved.iter().map(|held| held.person.id).collect();
        dispatch.escrow_granted(player, saved);
        dispatch.granted(player, &newly_granted, &waiting);
        granted.extend(newly_granted.iter().copied());
        for id in &newly_granted {
            // Multiple peers may claim in one network tick. A transferred
            // person must not remain eligible in the next claim batch.
            passenger_positions.remove(id);
            if let Some(stop) = waiting.remove(id) {
                if let Some(count) = stop_counts.get_mut(&stop) {
                    *count = count.saturating_sub(1);
                }
            }
        }
        let granted_set: HashSet<u32> = granted.iter().copied().collect();
        let denied: Vec<u32> = requested
            .iter()
            .copied()
            .filter(|id| !granted_set.contains(id))
            .collect();
        summary.granted += granted.len();
        summary.denied += denied.len();
        lan.answer_claim(player, &granted, &denied);
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_passenger_claims_cannot_steal_from_distant_stops_or_other_floors() {
        let bus = DVec3::new(100.0, 200.0, 5.0);
        assert!(can_claim_at(bus, DVec3::new(105.0, 205.0, 5.0)));
        assert!(!can_claim_at(bus, DVec3::new(220.0, 200.0, 5.0)));
        assert!(!can_claim_at(bus, DVec3::new(100.0, 200.0, 40.0)));
        assert!(!can_claim_at(bus, DVec3::new(f64::NAN, 200.0, 5.0)));
    }

    #[test]
    fn foreign_or_nonfinite_positions_cannot_poison_ai_physics() {
        let mut pose = Pose {
            bus: "Vehicles/Bus.bus".into(),
            flags: omsi_net::FLAG_VEHICLE,
            x: 12.0,
            y: -30.0,
            z: 3.0,
            heading: 90.0,
            ..Default::default()
        };
        let (_, body) = vehicle_box(&pose).expect("legacy vehicle uses safe box");
        assert!((body.2 - 5.5).abs() < 0.001);
        pose.x = f64::NAN;
        assert!(vehicle_box(&pose).is_none());
        pose.x = 1.0e12;
        assert!(vehicle_box(&pose).is_none());
        pose.x = 1.0;
        pose.width = f32::NAN;
        pose.length = f32::INFINITY;
        pose.speed_kmh = f32::NAN;
        let (_, body) = vehicle_box(&pose).expect("invalid extents fall back");
        assert!((body.2 - 5.5).abs() < 0.001);
        assert!((body.3 - 1.25).abs() < 0.001);
        assert_eq!(body.4, 0.0);
    }

    #[test]
    fn connected_client_positions_are_the_authoritative_ai_interest_points() {
        use omsi_net::WorldInfo;
        use omsi_sim::ai_traffic::setup::RandomTypes;
        use omsi_sim::traffic::Network;
        use std::net::SocketAddr;
        use std::path::Path;
        use std::time::Duration;

        let mut host = LanSession::host(0, "Dedicated", WorldInfo::default(), true).unwrap();
        let addr = SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
        let mut a = LanSession::join_addr(vec![addr], None, "a", WorldInfo::default()).unwrap();
        let mut b = LanSession::join_addr(vec![addr], None, "b", WorldInfo::default()).unwrap();
        let pa = Pose {
            bus: "Vehicles/BusA.bus".into(),
            flags: omsi_net::FLAG_VEHICLE,
            x: 100.0,
            y: 210.0,
            width: 2.5,
            length: 12.0,
            ..Default::default()
        };
        let pb = Pose {
            bus: "Vehicles/BusB.bus".into(),
            flags: omsi_net::FLAG_VEHICLE,
            x: -50.0,
            y: 20.0,
            width: 2.5,
            length: 8.0,
            ..Default::default()
        };
        for _ in 0..180 {
            host.tick(0.06, &Pose::default());
            a.tick(0.06, &pa);
            b.tick(0.06, &pb);
            if host
                .peers()
                .filter(|peer| peer.has_pose && peer.has_info && peer.pose.has_vehicle())
                .count()
                == 2
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let random = RandomTypes {
            types: Vec::new(),
            groups: Vec::new(),
            group_curves: false,
            group_uvg: Vec::new(),
            uvg_defaults: Vec::new(),
        };
        let mut traffic = crate::traffic::assemble(
            Path::new("."),
            Network::default(),
            random,
            Vec::new(),
            Vec::new(),
            0,
        );
        let mut people = PeopleSim::new(Path::new("."), 0);
        let status = update_host_interest(&mut host, &mut traffic, &mut people);
        assert_eq!(status.visible_players, 2);
        assert_eq!(traffic.lan_eyes.len(), 2);
        assert_eq!(traffic.others.len(), 2);
        assert_eq!(people.lan_centers.len(), 2);
        assert!(
            traffic
                .lan_centers
                .iter()
                .any(|p| (p.x - 100.0).abs() < 0.01)
        );
        assert!(
            traffic
                .lan_centers
                .iter()
                .any(|p| (p.x + 50.0).abs() < 0.01)
        );
        host.kick(a.my_id, "disconnected", false);
        let status = update_host_interest(&mut host, &mut traffic, &mut people);
        assert_eq!(status.visible_players, 1);
        assert_eq!(traffic.others.len(), 1);
        assert_eq!(people.lan_centers.len(), 1);
    }

    #[test]
    fn walking_peer_is_a_population_center_but_never_a_phantom_bus() {
        use omsi_net::WorldInfo;
        use omsi_sim::ai_traffic::setup::RandomTypes;
        use omsi_sim::traffic::Network;
        use std::net::SocketAddr;
        use std::path::Path;

        let mut host = LanSession::host(0, "Dedicated", WorldInfo::default(), true).unwrap();
        let addr = SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
        let mut walker =
            LanSession::join_addr(vec![addr], None, "walker", WorldInfo::default()).unwrap();
        let pose = Pose {
            name: "walker".into(),
            walker: Some(omsi_net::Walker {
                x: 4_000.0,
                y: 20.0,
                ..Default::default()
            }),
            ..Default::default()
        };
        for _ in 0..180 {
            host.tick(0.06, &Pose::default());
            walker.tick(0.06, &pose);
            if host.peers().any(|p| p.has_pose && p.has_info) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let random = RandomTypes {
            types: Vec::new(),
            groups: Vec::new(),
            group_curves: false,
            group_uvg: Vec::new(),
            uvg_defaults: Vec::new(),
        };
        let mut traffic = crate::traffic::assemble(
            Path::new("."),
            Network::default(),
            random,
            Vec::new(),
            Vec::new(),
            0,
        );
        let mut people = PeopleSim::new(Path::new("."), 0);
        let status = update_host_interest(&mut host, &mut traffic, &mut people);
        assert_eq!(status.visible_players, 1);
        assert_eq!(traffic.lan_centers.len(), 1);
        assert_eq!(people.lan_centers.len(), 1);
        assert!(traffic.others.is_empty());
        assert!((people.lan_centers[0].x - 4_000.0).abs() < 0.01);
        assert_eq!(client_position(&pose).unwrap().x, 4_000.0);
    }

    #[test]
    fn absent_vehicle_has_no_ai_obstacle() {
        assert!(vehicle_box(&Pose::default()).is_none());
    }
}
