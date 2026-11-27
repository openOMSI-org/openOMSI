//! Native LAN host's AI bus script context, independent of graphics.
//!
//! Client traffic tables describe the visible vehicle. The separate AI
//! context pages are still needed for HOF stop lists, active trip, delay,
//! script system variables and timetable display. Without them a scheduled
//! bus may be drawn but cannot be initialized correctly on another client.

use glam::DVec3;
use omsi_net::traffic_table::TrafficTable;
use omsi_net::{LanSession, Role};
use omsi_sim::ai_traffic::TrafficSim;
use omsi_sim::spatial::SpatialIndex;
use std::collections::{HashMap, HashSet};

const CAR_RADIUS: f64 = 650.0;
const MAX_CARS_PER_PEER: usize = 48;
const MAX_CONTEXT_PACKETS_PER_FRAME: usize = 4;
const MAX_CONTEXT_BYTES_PER_FRAME: usize = 4_800;

#[derive(Default)]
struct SentContext {
    page: usize,
    sent: u32,
    age: f32,
}

impl SentContext {
    fn due(&mut self, dt: f32, scheduled_bus: bool) -> bool {
        self.age += dt;
        let interval = if self.sent < 4 {
            0.15
        } else if scheduled_bus {
            1.0
        } else {
            4.0
        };
        self.age + 1.0e-4 >= interval
    }

    fn mark_sent(&mut self) {
        self.page = self.page.wrapping_add(1);
        self.sent = self.sent.saturating_add(1);
        self.age = 0.0;
    }
}

#[derive(Default)]
struct PeerContext {
    sent: HashMap<u32, SentContext>,
}

#[derive(Default)]
pub struct HeadlessContextSync {
    elapsed: f32,
    peers: HashMap<u32, PeerContext>,
    pub sent_bytes: u64,
}

/// The graphical host uses exactly this period/radius and retransmits pages
/// in rotation, so a late joiner or a lost datagram eventually receives the
/// whole timetable without new protocol messages.
impl HeadlessContextSync {
    pub fn tick(
        &mut self,
        lan: &mut LanSession,
        traffic: &TrafficSim,
        table: &TrafficTable,
        dt: f32,
    ) -> usize {
        if lan.role != Role::Host || !dt.is_finite() || dt <= 0.0 {
            return 0;
        }
        self.elapsed += dt;
        if self.elapsed < omsi_net::world::MOVING_EVERY {
            return 0;
        }
        self.elapsed =
            (self.elapsed - omsi_net::world::MOVING_EVERY).min(omsi_net::world::MOVING_EVERY);
        let viewers: Vec<_> = lan
            .peers()
            .filter(|peer| peer.has_pose && peer.has_info && peer.pose.has_vehicle())
            .filter_map(|peer| {
                let at = DVec3::new(peer.pose.x, peer.pose.y, peer.pose.z);
                (at.is_finite() && at.x.abs() < 1.0e8 && at.y.abs() < 1.0e8 && at.z.abs() < 1.0e8)
                    .then_some((peer.pose.id, at))
            })
            .collect();
        let active: HashSet<_> = viewers.iter().map(|(id, _)| *id).collect();
        self.peers.retain(|id, _| active.contains(id));
        if viewers.is_empty() {
            return 0;
        }
        // Build once for the snapshot, not once per player. Keep each
        // player's exact original radius and car-list priority, including
        // MAX_CARS_PER_PEER, so LAN payloads and client behavior do not change.
        let nearby_cars = SpatialIndex::from_positions(
            traffic
                .cars
                .iter()
                .map(|car| car.vehicle.position.truncate()),
        );
        let mut candidates = Vec::new();
        let mut bytes = 0;
        for (peer, at) in viewers {
            let view = self.peers.entry(peer).or_default();
            let mut nearby = HashSet::new();
            let mut sent_packets = 0;
            let mut sent_bytes = 0;
            nearby_cars.within(at.truncate(), CAR_RADIUS, &mut candidates);
            for &index in candidates.iter().take(MAX_CARS_PER_PEER) {
                let car = &traffic.cars[index];
                let Some(table_id) = table.id_of(car.id) else {
                    continue;
                };
                let id = table_id.raw();
                nearby.insert(id);
                let status = view.sent.entry(id).or_default();
                if !status.due(omsi_net::world::MOVING_EVERY, car.is_bus())
                    || sent_packets >= MAX_CONTEXT_PACKETS_PER_FRAME
                {
                    continue;
                }
                let Some(packet) = omsi_lan::mirror::ai_context(&car.vehicle.host).packet(
                    omsi_net::PROTOCOL as u8,
                    id,
                    lan.stamp_ms(),
                    status.page,
                ) else {
                    continue;
                };
                if sent_bytes + packet.len() > MAX_CONTEXT_BYTES_PER_FRAME {
                    continue;
                }
                lan.send_ai_context(peer, &packet);
                status.mark_sent();
                sent_packets += 1;
                sent_bytes += packet.len();
            }
            view.sent.retain(|id, _| nearby.contains(id));
            bytes += sent_bytes;
        }
        self.sent_bytes += bytes as u64;
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_initial_pages_repeat_then_back_off() {
        let mut sent = SentContext::default();
        assert!(!sent.due(0.1, true));
        assert!(sent.due(0.1, true));
        sent.mark_sent();
        assert_eq!(sent.page, 1);
        assert!(!sent.due(0.1, true));
        assert!(sent.due(0.1, true));
        sent.mark_sent();
        sent.mark_sent();
        sent.mark_sent();
        assert!(!sent.due(0.9, true));
        assert!(sent.due(0.1, true));
        sent.mark_sent();
        assert!(!sent.due(3.9, false));
        assert!(sent.due(0.1, false));
    }

    #[test]
    fn native_ai_context_has_a_fixed_peer_bandwidth_ceiling() {
        // A single wire page is at most the 120-byte protocol header and
        // 1000-byte layout chunk; the cap protects slow client connections.
        assert!(MAX_CONTEXT_PACKETS_PER_FRAME * 1_120 <= MAX_CONTEXT_BYTES_PER_FRAME);
        assert!(MAX_CARS_PER_PEER <= 64);
    }
}
