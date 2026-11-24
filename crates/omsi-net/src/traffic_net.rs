//! The traffic table (`traffic_table`) over the session's socket: the host publishes it to
//! every player around that player, a client holds the host's table and answers what it
//! heard. See `docs/TRAFFIC_TABLE.md`.

use super::*;
use traffic_table::wire::{self, Ack};
use traffic_table::{ClientSync, ClientTable, TrafficTable};

/// Answers a player may send the host (a second's worth is a few dozen; a burst allowed).
pub(super) const TACK_RATE: (f32, f32) = (40.0, 80.0);

#[derive(Default)]
pub(super) struct TrafficNet {
    /// Per player (host): what it is told and what it is known to hold.
    pub(super) host: HashMap<u32, ClientSync>,
    /// The host's table as the client has it (client).
    pub(super) client: ClientTable,
    /// Epochs given to players so far (host).
    epochs: u8,
    /// Datagrams answered, and answers heard (for the log).
    pub(super) acks_sent: u64,
    pub(super) acks_heard: u64,
}

/// The table's traffic, for the log.
#[derive(Debug, Clone, Copy, Default)]
pub struct TrafficNetStats {
    /// Host: vehicles told of, summed over the players.
    pub tracked: usize,
    pub datagrams: u64,
    pub bytes: u64,
    pub full_rows: u64,
    pub delta_rows: u64,
    pub deferred: u64,
    pub acks_heard: u64,
}

impl LanSession {
    /// Tell each player (`viewers`: its id and where it is) what is within its range of the
    /// host's traffic `table`, as the byte budget `bytes_per_second` allows (host). Call
    /// once a frame; the snapshots themselves come ten times a second. Returns the bytes sent.
    pub fn traffic_publish(
        &mut self,
        table: &TrafficTable,
        viewers: &[(u32, [f64; 3])],
        dt: f32,
        bytes_per_second: f64,
    ) -> usize {
        if self.role != Role::Host {
            return 0;
        }
        let now = self.started.elapsed().as_secs_f64();
        let stamp = self.stamp_ms();
        self.traffic
            .host
            .retain(|id, _| viewers.iter().any(|(v, _)| v == id));
        let mut n = 0;
        for (id, at) in viewers {
            let Some(to) = self.peer_addr(*id) else {
                continue;
            };
            if !self.traffic.host.contains_key(id) {
                self.traffic.epochs = self.traffic.epochs.wrapping_add(1);
                let mut s = ClientSync::new(self.traffic.epochs.max(1));
                s.bytes_per_second = bytes_per_second;
                self.traffic.host.insert(*id, s);
            }
            let sync = self.traffic.host.get_mut(id).unwrap();
            sync.bytes_per_second = bytes_per_second;
            for d in sync.publish(table, *at, dt, stamp, now) {
                n += d.len();
                self.send(&d, to);
            }
        }
        self.world_sent.set(self.world_sent.get() + n as u64);
        n
    }

    /// The host's table as this client has it (client).
    pub fn traffic_client(&self) -> &ClientTable {
        &self.traffic.client
    }

    pub fn traffic_client_mut(&mut self) -> &mut ClientTable {
        &mut self.traffic.client
    }

    /// Our clock in seconds, the one the client's table is kept on.
    pub fn traffic_clock(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    pub fn traffic_stats(&self) -> TrafficNetStats {
        let mut s = TrafficNetStats {
            acks_heard: self.traffic.acks_heard,
            ..Default::default()
        };
        for c in self.traffic.host.values() {
            s.tracked += c.tracked();
            s.datagrams += c.stats.datagrams;
            s.bytes += c.stats.bytes;
            s.full_rows += c.stats.full_rows;
            s.delta_rows += c.stats.delta_rows;
            s.deferred += c.stats.deferred;
        }
        let c = &self.traffic.client.stats;
        s.datagrams += c.datagrams;
        s.bytes += c.bytes;
        s
    }

    /// A datagram of the traffic table or of the voice chat: handled here when it is one.
    pub(super) fn on_table_or_voice(&mut self, data: &[u8], from: SocketAddr) -> bool {
        match data[0] {
            wire::MAGIC => self.on_traffic_packet(data),
            wire::ACK_MAGIC => self.on_traffic_ack(data, from),
            voice_net::MAGIC => self.on_voice(data, from),
            _ => return false,
        }
        true
    }

    /// A datagram of the host's table (client).
    pub(super) fn on_traffic_packet(&mut self, data: &[u8]) {
        if self.role != Role::Client || !self.connected {
            return;
        }
        let at = self.started.elapsed().as_secs_f64();
        if self.traffic.client.on_packet(data, at) {
            self.world_received += data.len() as u64;
        }
    }

    /// A client's answer (host).
    pub(super) fn on_traffic_ack(&mut self, data: &[u8], from: SocketAddr) {
        if self.role != Role::Host {
            return;
        }
        let Some(id) = self
            .peers
            .iter()
            .find(|(_, p)| p.addr == Some(from))
            .map(|(id, _)| *id)
        else {
            return;
        };
        // (a player's answers come at the rate of the datagrams they answer, not faster)
        if self
            .checked_peer(id, from, TACK_RATE, PeerTraffic::Tack)
            .is_none()
        {
            return;
        }
        let Some(ack) = wire::decode_ack(data, PROTOCOL as u8) else {
            return;
        };
        if let Some(sync) = self.traffic.host.get_mut(&id) {
            sync.on_ack(&ack);
            self.traffic.acks_heard += 1;
        }
    }

    /// What the client owes the host for the datagrams of this frame (client).
    pub(super) fn traffic_flush(&mut self) {
        if self.role != Role::Client || !self.connected {
            return;
        }
        let at = self.started.elapsed().as_secs_f64();
        self.traffic.client.tick(at);
        let Some(host) = self.host else { return };
        if let Some(ack) = self.traffic.client.take_ack() {
            let ack: Ack = ack;
            self.send(&wire::encode_ack(&ack, PROTOCOL as u8), host);
            self.traffic.acks_sent += 1;
        }
    }
}
