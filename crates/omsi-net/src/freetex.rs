//! Authenticated freetex pixel packets. Encoding and decoding belong to a LAN worker.
use super::*;

pub const MAGIC: u8 = 0xBD;

impl LanSession {
    pub fn take_freetex_packets(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.freetex_packets)
    }

    pub fn send_freetex_packet(&self, data: &[u8]) {
        let Some(a) = scripttex::address_with_magic(data, PROTOCOL as u8, MAGIC) else {
            return;
        };
        if self.connected && a.from == self.my_id && self.peers.contains_key(&a.to) {
            self.route_freetex(data);
        }
    }

    fn route_freetex(&self, data: &[u8]) {
        let Some(a) = scripttex::address_with_magic(data, PROTOCOL as u8, MAGIC) else {
            return;
        };
        match self.role {
            Role::Host => {
                if let Some(to) = self.peers.get(&a.to).and_then(|p| p.addr) {
                    self.send(data, to);
                }
            }
            Role::Client => {
                if let Some(host) = self.host {
                    self.send(data, host);
                }
            }
        }
    }

    pub(super) fn on_freetex(&mut self, data: &[u8], from: SocketAddr) {
        let Some(a) = scripttex::address_with_magic(data, PROTOCOL as u8, MAGIC) else {
            return;
        };
        if a.from == self.my_id || a.from == 0 || a.to == 0 || data.len() > 1245 {
            return;
        }
        match self.role {
            Role::Host => {
                if self
                    .checked_peer(a.from, from, TEXTURE_RATE, PeerTraffic::Textures)
                    .is_none()
                {
                    return;
                }
                if a.to != self.my_id {
                    self.route_freetex(data);
                    return;
                }
            }
            Role::Client => {
                if a.to != self.my_id || !self.peers.contains_key(&a.from) {
                    return;
                }
            }
        }
        // Overflow is packet loss, recovered by the existing end-to-end ACK/retry protocol.
        if self.freetex_packets.len() < 128 {
            self.freetex_packets.push(data.to_vec());
        }
    }
}
