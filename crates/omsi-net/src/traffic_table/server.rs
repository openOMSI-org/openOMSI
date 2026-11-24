//! The host's side of one client: which rows it is told of, in what order, in how many
//! bytes, and what it is known to hold.
//!
//! The client tells the host which datagrams it heard (`TACK`); what a datagram carried is
//! held by the client from then on, so a row is told as a change of the last row the client
//! acknowledged, and a standing vehicle costs nothing. Nothing is sent twice that was heard:
//! what is not acknowledged (the datagram was lost, or its answer was) is simply told again
//! with the row's state of that moment.
//!
//! A client is told of the vehicles within `radius_in` of it (they leave at `radius_out`, so
//! that one on the edge does not come and go), the nearer ones more often, within a byte
//! budget a second: what does not fit waits, the longest-waiting nearest first.

use super::ids::TrafficId;
use super::row::{Kind, Row};
use super::table::TrafficTable;
use super::wire::{
    self, Ack, Anchor, Delta, MAX_BASELINE, MAX_DATAGRAM, Packet, RowUpdate, Spawn, Trouble, newer,
};
use std::collections::{HashMap, HashSet, VecDeque};

/// Snapshots per second.
pub const SNAP_HZ: f32 = 10.0;
pub const SNAP_INTERVAL: f32 = 1.0 / SNAP_HZ;
/// How long a datagram's answer is waited for before its news is told again (s).
pub const RESEND_AFTER: f64 = 0.35;
/// A standing vehicle is told again (a few bits) this often as a sign of life (s).
pub const KEEPALIVE: f64 = 8.0;
pub const DEFAULT_RADIUS_IN: f64 = 650.0;
pub const DEFAULT_RADIUS_OUT: f64 = 700.0;
pub const DEFAULT_BYTES_PER_SECOND: f64 = 40_000.0;
/// The most spawns told in one snapshot (a client that has just come is brought up to date
/// over a second or two, not in one burst).
const SPAWNS_PER_SNAP: usize = 24;
const INFLIGHT_KEEP: f64 = 3.0;
const INFLIGHT_MAX: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    /// Told (or not yet) of, not yet acknowledged.
    Spawning,
    Live,
}

#[derive(Debug)]
struct Entry {
    state: State,
    kind: Kind,
    look: u16,
    /// The last spawn datagram's time.
    spawn_sent: Option<f64>,
    /// The row the client is known to hold, and the snapshot it came with.
    known: Option<(u16, Row)>,
    /// The last row sent (acknowledged or not) and when.
    last: Option<(Row, f64)>,
    meta_acked: u32,
    meta_sent: Option<(u32, f64)>,
    /// The vehicle is over, or out of range: the client is told to let it go.
    removing: Option<Removal>,
}

#[derive(Debug)]
struct Removal {
    sent: Option<f64>,
}

#[derive(Debug, Default)]
struct Sent {
    seq: u16,
    snap: u16,
    at: f64,
    looks: Vec<u16>,
    spawns: Vec<(TrafficId, Row)>,
    metas: Vec<(TrafficId, u32)>,
    removes: Vec<TrafficId>,
    rows: Vec<(TrafficId, Row)>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncStats {
    pub datagrams: u64,
    pub bytes: u64,
    pub full_rows: u64,
    pub delta_rows: u64,
    pub spawns: u64,
    pub removes: u64,
    /// Rows that were due but did not fit the byte budget of a snapshot, summed.
    pub deferred: u64,
}

pub struct ClientSync {
    epoch: u8,
    seq: u16,
    snap: u16,
    acc: f32,
    entries: HashMap<TrafficId, Entry>,
    /// Looks the client has acknowledged, and those told and when.
    looks_acked: HashSet<u16>,
    looks_sent: HashMap<u16, f64>,
    inflight: VecDeque<Sent>,
    bucket: f64,
    pub radius_in: f64,
    pub radius_out: f64,
    pub bytes_per_second: f64,
    pub stats: SyncStats,
}

impl Default for ClientSync {
    fn default() -> ClientSync {
        ClientSync::new(1)
    }
}

/// Rows due now, with how much they are due.
struct Due {
    id: TrafficId,
    score: f64,
}

struct Builder {
    cur: Packet,
    sent: Sent,
    done: Vec<(Packet, Sent)>,
    spent: usize,
}

impl Builder {
    fn room(&mut self, bits: usize, count: usize, limit: usize) -> bool {
        let need = bits.div_ceil(8);
        if count >= limit || self.cur.size() + need > MAX_DATAGRAM {
            if self.cur.is_empty() {
                return false;
            }
            self.flush();
            if self.cur.size() + need > MAX_DATAGRAM {
                return false;
            }
        }
        true
    }

    /// Bytes this snapshot has taken so far.
    fn used(&self) -> usize {
        self.spent
            + if self.cur.is_empty() {
                0
            } else {
                self.cur.size()
            }
    }

    fn flush(&mut self) {
        if self.cur.is_empty() {
            return;
        }
        let p = std::mem::take(&mut self.cur);
        let s = std::mem::take(&mut self.sent);
        self.spent += p.size();
        self.done.push((p, s));
    }
}

impl ClientSync {
    pub fn new(epoch: u8) -> ClientSync {
        ClientSync {
            epoch,
            seq: 0,
            snap: 0,
            acc: SNAP_INTERVAL,
            entries: HashMap::new(),
            looks_acked: HashSet::new(),
            looks_sent: HashMap::new(),
            inflight: VecDeque::new(),
            bucket: DEFAULT_BYTES_PER_SECOND * SNAP_INTERVAL as f64,
            radius_in: DEFAULT_RADIUS_IN,
            radius_out: DEFAULT_RADIUS_OUT,
            bytes_per_second: DEFAULT_BYTES_PER_SECOND,
            stats: SyncStats::default(),
        }
    }

    pub fn epoch(&self) -> u8 {
        self.epoch
    }

    /// Tell the client everything anew (it is asked to forget what it holds).
    pub fn reset(&mut self) {
        *self = ClientSync {
            stats: self.stats,
            radius_in: self.radius_in,
            radius_out: self.radius_out,
            bytes_per_second: self.bytes_per_second,
            ..ClientSync::new(self.epoch.wrapping_add(1))
        };
    }

    /// Vehicles the client is told of (or being told of).
    pub fn tracked(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.removing.is_none())
            .count()
    }

    pub fn live(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.state == State::Live && e.removing.is_none())
            .count()
    }

    pub fn knows(&self, id: TrafficId) -> bool {
        self.entries
            .get(&id)
            .is_some_and(|e| e.state == State::Live && e.removing.is_none())
    }

    /// The client's news of what it heard.
    pub fn on_ack(&mut self, ack: &Ack) {
        if ack.epoch != self.epoch {
            return;
        }
        let (done, keep): (VecDeque<Sent>, VecDeque<Sent>) = std::mem::take(&mut self.inflight)
            .into_iter()
            .partition(|s| ack.covers(s.seq));
        self.inflight = keep;
        for sent in done {
            self.confirm(sent);
        }
        for (id, t) in &ack.troubles {
            let Some(e) = self.entries.get_mut(id) else {
                continue;
            };
            match t {
                Trouble::SendWhole => e.known = None,
                Trouble::Unknown => {
                    if e.removing.is_none() {
                        // (a client that lost the vehicle may have lost its look too)
                        self.looks_acked.remove(&e.look);
                        e.state = State::Spawning;
                        e.known = None;
                        e.last = None;
                        e.spawn_sent = None;
                        e.meta_acked = 0;
                        e.meta_sent = None;
                    }
                }
            }
        }
    }

    fn confirm(&mut self, s: Sent) {
        for l in &s.looks {
            self.looks_acked.insert(*l);
        }
        for (id, row) in &s.spawns {
            if let Some(e) = self.entries.get_mut(id) {
                if e.removing.is_none() && e.state == State::Spawning {
                    e.state = State::Live;
                    e.known = Some((s.snap, *row));
                }
            }
        }
        for (id, rev) in &s.metas {
            if let Some(e) = self.entries.get_mut(id) {
                e.meta_acked = e.meta_acked.max(*rev);
            }
        }
        for (id, row) in &s.rows {
            if let Some(e) = self.entries.get_mut(id) {
                if e.state == State::Live && e.known.is_none_or(|(k, _)| newer(s.snap, k)) {
                    e.known = Some((s.snap, *row));
                }
            }
        }
        for id in &s.removes {
            if self.entries.get(id).is_some_and(|e| e.removing.is_some()) {
                self.entries.remove(id);
            }
        }
    }

    /// The datagrams of this moment for a client at `viewer`: none while it is not time for
    /// a snapshot (`SNAP_HZ`). `now` is the table's clock (s), `host_ms` the stamp.
    pub fn publish(
        &mut self,
        table: &TrafficTable,
        viewer: [f64; 3],
        dt: f32,
        host_ms: u32,
        now: f64,
    ) -> Vec<Vec<u8>> {
        self.acc += dt;
        if self.acc + 1.0e-4 < SNAP_INTERVAL {
            return Vec::new();
        }
        self.acc = (self.acc - SNAP_INTERVAL).clamp(0.0, SNAP_INTERVAL);
        self.snap = self.snap.wrapping_add(1);
        let anchor = Anchor::at(viewer);
        let cap = self.bytes_per_second * SNAP_INTERVAL as f64;
        self.bucket = (self.bucket + cap).min(cap * 2.0);
        while self
            .inflight
            .front()
            .is_some_and(|s| now - s.at > INFLIGHT_KEEP)
            || self.inflight.len() > INFLIGHT_MAX
        {
            self.inflight.pop_front();
        }
        self.track(table, viewer, anchor);
        let mut b = Builder {
            cur: Packet::default(),
            sent: Sent::default(),
            done: Vec::new(),
            spent: 0,
        };
        self.pack_events(table, now, anchor, &mut b);
        self.pack_rows(table, viewer, now, anchor, &mut b);
        b.flush();
        let mut out = Vec::with_capacity(b.done.len());
        for (mut p, mut s) in b.done {
            self.seq = self.seq.wrapping_add(1);
            p.epoch = self.epoch;
            p.seq = self.seq;
            p.snap = self.snap;
            p.host_ms = host_ms;
            p.anchor = anchor;
            s.seq = self.seq;
            s.snap = self.snap;
            s.at = now;
            let Some(bytes) = wire::encode(&p, crate::PROTOCOL as u8) else {
                log::warn!(
                    "traffic table: a datagram of {} bytes could not be made",
                    p.size()
                );
                continue;
            };
            self.stats.datagrams += 1;
            self.stats.bytes += bytes.len() as u64;
            self.inflight.push_back(s);
            out.push(bytes);
        }
        out
    }

    /// Who comes into range, who leaves, whose vehicle is over.
    fn track(&mut self, table: &TrafficTable, viewer: [f64; 3], anchor: Anchor) {
        for slot in table.iter() {
            let d = slot.row.distance_to(viewer);
            match self.entries.get_mut(&slot.id) {
                None => {
                    let sendable = table.look(slot.look).is_some_and(wire::look_is_sendable);
                    if d <= self.radius_in && anchor.reaches(&slot.row) && sendable {
                        self.entries.insert(
                            slot.id,
                            Entry {
                                state: State::Spawning,
                                kind: slot.kind,
                                look: slot.look,
                                spawn_sent: None,
                                known: None,
                                last: None,
                                meta_acked: 0,
                                meta_sent: None,
                                removing: None,
                            },
                        );
                    }
                }
                Some(e)
                    if e.removing.is_none()
                        && (d > self.radius_out || !anchor.reaches(&slot.row)) =>
                {
                    e.removing = Some(Removal { sent: None });
                }
                Some(_) => {}
            }
        }
        let ended: Vec<TrafficId> = self
            .entries
            .iter()
            .filter(|(id, e)| e.removing.is_none() && table.get(**id).is_none())
            .map(|(id, _)| *id)
            .collect();
        for id in ended {
            self.entries.get_mut(&id).unwrap().removing = Some(Removal { sent: None });
        }
        // (a vehicle the client was never told of needs no goodbye)
        self.entries
            .retain(|_, e| !(e.removing.is_some() && e.spawn_sent.is_none() && e.known.is_none()));
    }

    fn pack_events(&mut self, table: &TrafficTable, now: f64, anchor: Anchor, b: &mut Builder) {
        // the goodbyes
        let mut removes: Vec<TrafficId> = self
            .entries
            .iter()
            .filter(|(_, e)| {
                e.removing
                    .as_ref()
                    .is_some_and(|r| r.sent.is_none_or(|t| now - t >= RESEND_AFTER))
            })
            .map(|(id, _)| *id)
            .collect();
        removes.sort_unstable();
        for id in removes {
            if !b.room(wire::REMOVE_BITS, b.cur.removes.len(), 63) {
                break;
            }
            b.cur.removes.push(id);
            b.sent.removes.push(id);
            self.entries
                .get_mut(&id)
                .unwrap()
                .removing
                .as_mut()
                .unwrap()
                .sent = Some(now);
            self.stats.removes += 1;
        }
        // the newcomers, the nearest the client's first
        let mut fresh: Vec<TrafficId> = self
            .entries
            .iter()
            .filter(|(_, e)| {
                e.state == State::Spawning
                    && e.removing.is_none()
                    && e.spawn_sent.is_none_or(|t| now - t >= RESEND_AFTER)
            })
            .map(|(id, _)| *id)
            .collect();
        fresh.sort_unstable();
        fresh.truncate(SPAWNS_PER_SNAP);
        for id in fresh {
            let (Some(slot), Some(e)) = (table.get(id), self.entries.get(&id)) else {
                continue;
            };
            let Some(look) = table.look(e.look) else {
                continue;
            };
            let look_bits = if self.looks_acked.contains(&e.look) {
                0
            } else {
                wire::look_bits(look)
            };
            let want_meta = slot.meta_rev > e.meta_acked
                && !(slot.meta == Default::default() && e.meta_acked == 0);
            let meta_bits = if want_meta {
                wire::meta_bits(&slot.meta)
            } else {
                0
            };
            if (look_bits > 0 && b.cur.looks.len() >= 31) || (want_meta && b.cur.metas.len() >= 31)
            {
                b.flush();
            }
            if !b.room(
                look_bits + wire::SPAWN_BITS + meta_bits,
                b.cur.spawns.len(),
                63,
            ) {
                break;
            }
            // (a spawn is told with its look until the look is acknowledged)
            if look_bits > 0 && !b.cur.looks.iter().any(|(i, _)| *i == e.look) {
                b.cur.looks.push((e.look, look.clone()));
                b.sent.looks.push(e.look);
                self.looks_sent.insert(e.look, now);
            }
            b.cur.spawns.push(Spawn {
                id,
                look: e.look,
                kind: e.kind,
                row: slot.row,
            });
            b.sent.spawns.push((id, slot.row));
            if want_meta {
                b.cur.metas.push((id, slot.meta.clone()));
                b.sent.metas.push((id, slot.meta_rev));
            }
            let e = self.entries.get_mut(&id).unwrap();
            e.spawn_sent = Some(now);
            e.last = Some((slot.row, now));
            if want_meta {
                e.meta_sent = Some((slot.meta_rev, now));
            }
            self.stats.spawns += 1;
        }
        // lines and destinations that changed
        let mut metas: Vec<TrafficId> = self
            .entries
            .iter()
            .filter(|(id, e)| {
                e.state == State::Live
                    && e.removing.is_none()
                    && table.get(**id).is_some_and(|s| {
                        s.meta_rev > e.meta_acked
                            && e.meta_sent
                                .is_none_or(|(rev, t)| rev != s.meta_rev || now - t >= RESEND_AFTER)
                    })
            })
            .map(|(id, _)| *id)
            .collect();
        metas.sort_unstable();
        for id in metas {
            let slot = table.get(id).unwrap();
            if !b.room(wire::meta_bits(&slot.meta), b.cur.metas.len(), 31) {
                break;
            }
            b.cur.metas.push((id, slot.meta.clone()));
            b.sent.metas.push((id, slot.meta_rev));
            self.entries.get_mut(&id).unwrap().meta_sent = Some((slot.meta_rev, now));
        }
        let _ = anchor;
    }

    fn pack_rows(
        &mut self,
        table: &TrafficTable,
        viewer: [f64; 3],
        now: f64,
        anchor: Anchor,
        b: &mut Builder,
    ) {
        let mut due: Vec<Due> = Vec::new();
        for (id, e) in &self.entries {
            if e.state != State::Live || e.removing.is_some() {
                continue;
            }
            let Some(slot) = table.get(*id) else { continue };
            let since = e.last.map_or(f64::MAX, |(_, t)| now - t);
            let same_as_known = e.known.is_some_and(|(_, r)| r == slot.row);
            if same_as_known {
                if since >= KEEPALIVE {
                    due.push(Due {
                        id: *id,
                        score: 0.1,
                    });
                }
                continue;
            }
            if e.last.is_some_and(|(r, _)| r == slot.row) && since < RESEND_AFTER {
                continue;
            }
            let d = slot.row.distance_to(viewer);
            let interval = if d < 200.0 {
                0.1
            } else if d < 400.0 {
                0.2
            } else {
                0.3
            };
            // (a little early is on time: snapshots come at 0.1 s, give or take)
            if since + 0.02 < interval {
                continue;
            }
            due.push(Due {
                id: *id,
                score: since.min(10.0) / interval,
            });
        }
        due.sort_unstable_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)));
        let mut deferred = 0;
        for (n, item) in due.iter().enumerate() {
            let e = &self.entries[&item.id];
            let slot = table.get(item.id).unwrap();
            let update = match e.known.and_then(|(ks, kr)| {
                let back = self.snap.wrapping_sub(ks) as u32;
                (1..=MAX_BASELINE)
                    .contains(&back)
                    .then(|| Delta::between(&kr, &slot.row).map(|d| (back as u8, d)))
                    .flatten()
            }) {
                Some((back, delta)) => RowUpdate::Delta {
                    id: item.id,
                    base_back: back,
                    delta,
                },
                None if anchor.reaches(&slot.row) => RowUpdate::Full {
                    id: item.id,
                    row: slot.row,
                },
                None => continue,
            };
            let bits = update.bits();
            // (the budget: what the events took is gone from it)
            if (b.used() + bits.div_ceil(8)) as f64 > self.bucket && n > 0 {
                deferred = due.len() - n;
                break;
            }
            if !b.room(bits, b.cur.rows.len(), 255) {
                deferred = due.len() - n;
                break;
            }
            match &update {
                RowUpdate::Full { .. } => self.stats.full_rows += 1,
                RowUpdate::Delta { .. } => self.stats.delta_rows += 1,
            }
            b.cur.rows.push(update);
            b.sent.rows.push((item.id, slot.row));
            self.entries.get_mut(&item.id).unwrap().last = Some((slot.row, now));
        }
        self.stats.deferred += deferred as u64;
        let spent = b.used() as f64;
        self.bucket -= spent;
        // (a long debt is not carried: a client is not starved for seconds by one burst)
        self.bucket = self.bucket.max(-self.bytes_per_second * 0.2);
    }
}
