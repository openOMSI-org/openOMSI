//! Buses that stand in the host's world with nobody in them, for the players to take: the
//! host tells them as it tells a player's parked bus (`INFO` and a state now and then, under
//! a number no player gets), so every game draws them with what it draws the players' buses
//! with, and a game that knows nothing of them sees buses somebody left there.

use super::*;

/// The numbers of the parked buses: from here up, never given to a player.
pub const FIRST_PARKED_ID: u32 = 65000;

/// How often a parked bus's state is told (it does not move; a game forgets a player it
/// hears nothing of in `TIMEOUT`).
const STATE_EVERY: f32 = 1.0;

/// A player is told the parked buses within this distance (m) of where it is, the nearest
/// `TOLD_AT_MOST` of them: however many stand in the world, a game draws those round it (it
/// keeps `MAX_PEERS` others at most, the players among them). One it gets away from is told
/// no more, and its game forgets it as it forgets a player it hears nothing of.
const TOLD_WITHIN: f64 = 700.0;
const TOLD_AT_MOST: usize = 24;
/// Places among a game's `MAX_PEERS` that parked buses never take: the players'.
const PLAYERS_ROOM: usize = 34;

/// Is `id` a parked bus's number, not a player's?
pub fn is_parked(id: u32) -> bool {
    (FIRST_PARKED_ID..=u16::MAX as u32).contains(&id)
}

/// The host's parked buses and when they were last told.
#[derive(Default)]
pub(crate) struct Parked {
    buses: Vec<Pose>,
    seq: u16,
    state_acc: f32,
    info_acc: f32,
}

impl Parked {
    pub(crate) fn footprints(&self) -> impl Iterator<Item = Footprint> + '_ {
        self.buses.iter().map(Pose::footprint)
    }
}

impl LanSession {
    /// The buses standing free in the host's world (host), each under its own number
    /// (`is_parked`; others are left out). One that is no longer among them - taken by a
    /// player, or gone - leaves the players' games at once.
    pub fn set_parked(&mut self, buses: Vec<Pose>) {
        let mut buses: Vec<Pose> = buses.into_iter().filter(|p| is_parked(p.id) && !p.bus.is_empty()).collect();
        for p in &mut buses {
            p.flags = FLAG_VEHICLE;
            p.walker = None;
        }
        for old in &self.parked.buses {
            if !buses.iter().any(|p| p.id == old.id) {
                self.broadcast(format!("BYE|{}", old.id).as_bytes(), None);
            }
        }
        if buses != self.parked.buses {
            // (told at once: a bus left a moment ago stands there for the others again)
            self.parked.state_acc = STATE_EVERY;
            self.parked.info_acc = INFO_EVERY;
        }
        self.parked.buses = buses;
    }

    /// Tell the players the parked buses when it is time (host, every frame).
    pub fn tell_parked(&mut self, dt: f32) {
        if self.parked.buses.is_empty() {
            return;
        }
        self.parked.state_acc += dt;
        self.parked.info_acc += dt;
        let info = self.parked.info_acc >= INFO_EVERY;
        let state = self.parked.state_acc >= STATE_EVERY;
        if !info && !state {
            return;
        }
        if info {
            self.parked.info_acc = 0.0;
        }
        if state {
            self.parked.state_acc = 0.0;
            self.parked.seq = self.parked.seq.wrapping_add(1);
        }
        let sent_ms = self.stamp_ms().max(1);
        for peer in self.peers.values() {
            // where the player is: its bus, or the player itself when it walks
            let (Some(addr), true) = (peer.addr, peer.has_pose) else { continue };
            let (x, y) = peer.pose.walker.map(|w| (w.x, w.y)).unwrap_or((peer.pose.x, peer.pose.y));
            let mut near: Vec<(f64, &Pose)> = self.parked.buses.iter().map(|p| ((p.x - x).hypot(p.y - y), p)).filter(|n| n.0 < TOLD_WITHIN).collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            // (fewer of them in a full session: a game keeps `MAX_PEERS` others, players first)
            let most = TOLD_AT_MOST.min(MAX_PEERS.saturating_sub(self.peers.len() + 1));
            for (_, p) in near.into_iter().take(most) {
                if info {
                    self.send(p.encode_info().as_bytes(), addr);
                }
                if state {
                    let state = Pose { sent_ms, ..p.clone() };
                    self.send(&wire::encode_state(&state, PROTOCOL as u8, self.parked.seq), addr);
                }
            }
        }
    }

    /// Is there room among the peers for `id`, new here (a client)? A player always finds
    /// room while parked buses hold places: one of them goes (the game hears of it again
    /// when there is room). A parked bus finds none in the players' places.
    pub(crate) fn room_for(&mut self, id: u32) -> bool {
        let parked = self.peers.keys().filter(|k| is_parked(**k)).count();
        if is_parked(id) {
            return self.peers.len() < MAX_PEERS && parked < MAX_PEERS - PLAYERS_ROOM;
        }
        if self.peers.len() < MAX_PEERS {
            return true;
        }
        let Some(out) = self.peers.keys().copied().filter(|k| is_parked(*k)).max() else { return false };
        // (the game drops what it drew of a peer the session no longer has)
        self.peers.remove(&out);
        true
    }

    /// The next player id nobody has (2 up to the parked buses' numbers; the host is 1).
    pub(crate) fn fresh_id(&mut self) -> u32 {
        loop {
            let id = self.next_id;
            self.next_id = if self.next_id + 1 >= FIRST_PARKED_ID { 2 } else { self.next_id + 1 };
            if !self.peers.contains_key(&id) {
                return id;
            }
        }
    }
}
