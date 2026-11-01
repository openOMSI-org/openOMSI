//! Handle world datagrams outside the main UDP receive loop.
//! World updates still obey the same peer rate limits and per-session queues.

use super::*;

impl LanSession {
    pub(super) fn on_world_datagram(&mut self, data: &[u8], from: SocketAddr, n: usize) {
        // The host only takes updates from a known, rate-limited client.
        // Clients accept the host's world from the validated receive path.
        if self.role == Role::Host {
            let Some(id) = self
                .peers
                .iter()
                .find(|(_, p)| p.addr == Some(from))
                .map(|(id, _)| *id)
            else {
                return;
            };
            if self.checked_peer(id, from, STATE_RATE, PeerTraffic::State).is_none() {
                return;
            }
            if let Some(mut f) = world::decode(data, PROTOCOL as u8) {
                self.world_received += n as u64;
                f.lights.clear();
                if self.world_up.len() < 256 {
                    self.world_up.push((id, Instant::now(), f));
                }
            }
        } else if let Some(f) = world::decode(data, PROTOCOL as u8) {
            self.world_received += n as u64;
            if self.world_in.len() < 256 {
                self.world_in.push((Instant::now(), f));
            }
        }
    }
}
