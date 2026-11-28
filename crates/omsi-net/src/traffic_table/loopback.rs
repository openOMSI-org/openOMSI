//! The table's two ends joined in one process through a made-up network that loses,
//! delays and reorders datagrams: for the tests, and for the game's own check of the wire
//! (`OMSI_TRAFFIC_TABLE_CHECK`) without a second computer.

use super::client::ClientTable;
use super::server::ClientSync;
use super::table::TrafficTable;

/// One direction of a bad network.
#[derive(Debug, Clone)]
pub struct Link {
    /// Chance (0 .. 1) that a datagram is lost.
    pub loss: f64,
    /// Delay (s) and its spread: a datagram arrives `latency` to `latency + jitter` later.
    pub latency: f64,
    pub jitter: f64,
    rng: u64,
    queue: Vec<(f64, Vec<u8>)>,
}

impl Link {
    pub fn new(loss: f64, latency: f64, jitter: f64, seed: u64) -> Link {
        Link {
            loss,
            latency,
            jitter,
            rng: seed | 1,
            queue: Vec::new(),
        }
    }

    fn next(&mut self) -> f64 {
        // (xorshift)
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn send(&mut self, now: f64, data: Vec<u8>) {
        if self.next() < self.loss {
            return;
        }
        let at = now + self.latency + self.next() * self.jitter;
        self.queue.push((at, data));
    }

    /// The datagrams that arrive by `now`, in the order they arrive.
    pub fn deliver(&mut self, now: f64) -> Vec<Vec<u8>> {
        let (mut due, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.queue)
            .into_iter()
            .partition(|(t, _)| *t <= now);
        self.queue = rest;
        due.sort_by(|a, b| a.0.total_cmp(&b.0));
        due.into_iter().map(|(_, d)| d).collect()
    }
}

/// A host's sync for one client, the client's table, and the network between them.
pub struct Loopback {
    pub sync: ClientSync,
    pub client: ClientTable,
    pub down: Link,
    pub up: Link,
    pub now: f64,
    /// Bytes of datagrams the host put on the network, and of the client's answers.
    pub bytes_down: u64,
    pub bytes_up: u64,
}

impl Loopback {
    pub fn new(down: Link, up: Link) -> Loopback {
        Loopback {
            sync: ClientSync::new(1),
            client: ClientTable::default(),
            down,
            up,
            now: 0.0,
            bytes_down: 0,
            bytes_up: 0,
        }
    }

    /// A perfect network.
    pub fn perfect() -> Loopback {
        Loopback::new(Link::new(0.0, 0.0, 0.0, 1), Link::new(0.0, 0.0, 0.0, 2))
    }

    /// One frame of `dt` seconds: the host publishes for a client at `viewer`, what
    /// arrives is taken in, the answers go back.
    pub fn step(&mut self, table: &TrafficTable, viewer: [f64; 3], dt: f32) {
        self.now += dt as f64;
        for d in self
            .sync
            .publish(table, viewer, dt, (self.now * 1000.0) as u32, self.now)
        {
            self.bytes_down += d.len() as u64;
            self.down.send(self.now, d);
        }
        for d in self.down.deliver(self.now) {
            self.client.on_packet(&d, self.now);
        }
        self.client.tick(self.now);
        if let Some(a) = self.client.take_ack() {
            let d = super::wire::encode_ack(&a, crate::PROTOCOL as u8);
            self.bytes_up += d.len() as u64;
            self.up.send(self.now, d);
        }
        for d in self.up.deliver(self.now) {
            if let Some(a) = super::wire::decode_ack(&d, crate::PROTOCOL as u8) {
                self.sync.on_ack(&a);
            }
        }
    }
}
