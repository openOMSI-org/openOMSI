//! A client's copy of the host's traffic table: the rows it was told of, the last poses of
//! each, and where each is at any moment between them.

use super::ids::TrafficId;
use super::row::{Kind, Look, Meta, Row};
use super::wire::{self, Ack, MAX_TROUBLES, Packet, RowUpdate, Trouble, newer};
use std::collections::{HashMap, VecDeque};

/// Poses kept of a vehicle: more than the farthest a baseline reaches back.
const KEEP_SAMPLES: usize = 40;
/// How long a client guesses on past the last pose of something moving (ms).
pub const MAX_EXTRAPOLATE: f64 = 400.0;
/// A jump of more than this (m) between two poses is a teleport, not a drive.
pub const TELEPORT: f64 = 30.0;
/// A vehicle not heard of for this long, while the host speaks, is let go (s). The host
/// tells every standing one every `KEEPALIVE`.
pub const SILENT_PURGE: f64 = 30.0;
const TOMB_KEEP: f64 = 10.0;
const EVENTS_MAX: usize = 4096;

#[derive(Debug, Clone, Copy)]
struct Sample {
    snap: u16,
    ms: f64,
    row: Row,
}

#[derive(Debug)]
pub struct ClientRow {
    pub id: TrafficId,
    pub look: u16,
    pub kind: Kind,
    pub meta: Meta,
    born_snap: u16,
    samples: VecDeque<Sample>,
    heard: f64,
}

impl ClientRow {
    /// The newest row heard.
    pub fn latest(&self) -> Option<&Row> {
        self.samples.back().map(|s| &s.row)
    }

    pub fn samples(&self) -> usize {
        self.samples.len()
    }
}

/// What happened to the table since it was last asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Spawned(TrafficId),
    /// The vehicle is over, out of the client's range, or the host started anew.
    Removed(TrafficId),
    /// The line or destination changed.
    Meta(TrafficId),
}

/// Where a vehicle is drawn at a moment: the poses around it blended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub pos: [f64; 3],
    pub heading: f64,
    pub pitch: f64,
    pub bank: f64,
    pub speed: f64,
    pub steer: f64,
    pub blinker: u8,
    pub brake: bool,
    pub lights: bool,
    pub at_station: i8,
    /// The moment lies past the newest pose: guessed on from it.
    pub extrapolated: bool,
    /// The vehicle was put somewhere else between the two poses (no gliding).
    pub teleported: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ClientStats {
    pub datagrams: u64,
    pub bytes: u64,
    pub full_rows: u64,
    pub delta_rows: u64,
    pub unusable: u64,
}

#[derive(Debug, Default)]
pub struct ClientTable {
    epoch: Option<u8>,
    rows: HashMap<TrafficId, ClientRow>,
    looks: HashMap<u16, Look>,
    /// Vehicles taken away, and the snapshot of that: a late copy of their spawn is no news.
    tombstones: HashMap<TrafficId, (u16, f64)>,
    events: Vec<Event>,
    newest: Option<u16>,
    mask: u32,
    ack_due: bool,
    troubles: Vec<(TrafficId, Trouble)>,
    last_packet: f64,
    pub stats: ClientStats,
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn lerp_angle(a: f64, b: f64, t: f64) -> f64 {
    let d = (b - a + 540.0).rem_euclid(360.0) - 180.0;
    (a + d * t).rem_euclid(360.0)
}

impl ClientTable {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn ids(&self) -> impl Iterator<Item = TrafficId> + '_ {
        self.rows.keys().copied()
    }

    pub fn get(&self, id: TrafficId) -> Option<&ClientRow> {
        self.rows.get(&id)
    }

    pub fn look(&self, row: &ClientRow) -> Option<&Look> {
        self.looks.get(&row.look)
    }

    /// The events since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    fn event(&mut self, e: Event) {
        if self.events.len() < EVENTS_MAX {
            self.events.push(e);
        }
    }

    /// Forget everything (the host started anew, or the session is over); the vehicles are
    /// reported removed.
    pub fn clear(&mut self) {
        let ids: Vec<TrafficId> = self.rows.keys().copied().collect();
        for id in ids {
            self.event(Event::Removed(id));
        }
        self.rows.clear();
        self.looks.clear();
        self.tombstones.clear();
        self.epoch = None;
        self.newest = None;
        self.mask = 0;
        self.ack_due = false;
        self.troubles.clear();
    }

    fn note_seq(&mut self, seq: u16) {
        match self.newest {
            None => {
                self.newest = Some(seq);
                self.mask = 0;
            }
            Some(n) if newer(seq, n) => {
                let d = seq.wrapping_sub(n) as u32;
                self.mask = if d > 32 {
                    0
                } else {
                    self.mask.checked_shl(d).unwrap_or(0) | 1u32.checked_shl(d - 1).unwrap_or(0)
                };
                self.newest = Some(seq);
            }
            Some(n) => {
                let back = n.wrapping_sub(seq) as u32;
                if (1..=32).contains(&back) {
                    self.mask |= 1 << (back - 1);
                }
            }
        }
        self.ack_due = true;
    }

    fn trouble(&mut self, id: TrafficId, t: Trouble) {
        if self.troubles.len() < MAX_TROUBLES && !self.troubles.iter().any(|(i, _)| *i == id) {
            self.troubles.push((id, t));
        }
    }

    fn push_sample(row: &mut ClientRow, snap: u16, ms: f64, r: Row, local: f64) {
        row.heard = local;
        let at = row
            .samples
            .iter()
            .rposition(|s| s.snap == snap || newer(snap, s.snap))
            .map_or(0, |i| i + 1);
        if at > 0 && row.samples[at - 1].snap == snap {
            return;
        }
        row.samples.insert(at, Sample { snap, ms, row: r });
        while row.samples.len() > KEEP_SAMPLES {
            row.samples.pop_front();
        }
    }

    /// Take a datagram in at local time `local` (s). False when it is not a traffic
    /// datagram of this protocol.
    pub fn on_packet(&mut self, data: &[u8], local: f64) -> bool {
        let Some(p) = wire::decode(data, crate::PROTOCOL as u8) else {
            return false;
        };
        self.stats.datagrams += 1;
        self.stats.bytes += data.len() as u64;
        self.apply(p, local);
        true
    }

    pub fn apply(&mut self, p: Packet, local: f64) {
        if self.epoch != Some(p.epoch) {
            self.clear();
            self.epoch = Some(p.epoch);
        }
        self.last_packet = local;
        self.note_seq(p.seq);
        let ms = p.host_ms as f64;
        for (i, l) in &p.looks {
            self.looks.insert(*i, l.clone());
        }
        for s in &p.spawns {
            if self
                .tombstones
                .get(&s.id)
                .is_some_and(|(snap, _)| newer(*snap, p.snap))
            {
                continue;
            }
            match self.rows.get_mut(&s.id) {
                Some(row) => Self::push_sample(row, p.snap, ms, s.row, local),
                None => {
                    let mut row = ClientRow {
                        id: s.id,
                        look: s.look,
                        kind: s.kind,
                        meta: Meta::default(),
                        born_snap: p.snap,
                        samples: VecDeque::new(),
                        heard: local,
                    };
                    Self::push_sample(&mut row, p.snap, ms, s.row, local);
                    self.rows.insert(s.id, row);
                    self.tombstones.remove(&s.id);
                    self.event(Event::Spawned(s.id));
                }
            }
        }
        for (id, m) in &p.metas {
            let changed = match self.rows.get_mut(id) {
                Some(r) if r.meta != *m => {
                    r.meta = m.clone();
                    true
                }
                _ => false,
            };
            if changed {
                self.event(Event::Meta(*id));
            }
        }
        for id in &p.removes {
            if self
                .rows
                .get(id)
                .is_some_and(|r| !newer(r.born_snap, p.snap))
            {
                self.rows.remove(id);
                self.event(Event::Removed(*id));
            }
            if !self.rows.contains_key(id) {
                self.tombstones.insert(*id, (p.snap, local));
            }
        }
        for u in &p.rows {
            match u {
                RowUpdate::Full { id, row } => {
                    self.stats.full_rows += 1;
                    match self.rows.get_mut(id) {
                        Some(r) => Self::push_sample(r, p.snap, ms, *row, local),
                        None => {
                            self.stats.unusable += 1;
                            if !self.tombstones.contains_key(id) {
                                self.trouble(*id, Trouble::Unknown);
                            }
                        }
                    }
                }
                RowUpdate::Delta {
                    id,
                    base_back,
                    delta,
                } => {
                    self.stats.delta_rows += 1;
                    let base_snap = p.snap.wrapping_sub(*base_back as u16);
                    let Some(r) = self.rows.get_mut(id) else {
                        self.stats.unusable += 1;
                        if !self.tombstones.contains_key(id) {
                            self.trouble(*id, Trouble::Unknown);
                        }
                        continue;
                    };
                    match r
                        .samples
                        .iter()
                        .find(|s| s.snap == base_snap)
                        .map(|s| delta.apply(&s.row))
                    {
                        Some(row) => Self::push_sample(r, p.snap, ms, row, local),
                        None => {
                            self.stats.unusable += 1;
                            self.trouble(*id, Trouble::SendWhole);
                        }
                    }
                }
            }
        }
    }

    /// The answer owed the host for what came since the last call, if any.
    pub fn take_ack(&mut self) -> Option<Ack> {
        if !std::mem::take(&mut self.ack_due) {
            return None;
        }
        Some(Ack {
            epoch: self.epoch?,
            seq: self.newest?,
            mask: self.mask,
            troubles: std::mem::take(&mut self.troubles),
        })
    }

    /// Housekeeping at local time `local`: old tombstones go, and vehicles the host has not
    /// mentioned for `SILENT_PURGE` s while it speaks of others.
    pub fn tick(&mut self, local: f64) {
        self.tombstones.retain(|_, (_, t)| local - *t < TOMB_KEEP);
        if local - self.last_packet < 5.0 {
            let stale: Vec<TrafficId> = self
                .rows
                .values()
                .filter(|r| local - r.heard >= SILENT_PURGE)
                .map(|r| r.id)
                .collect();
            for id in stale {
                self.rows.remove(&id);
                self.event(Event::Removed(id));
            }
        }
    }

    /// Where vehicle `id` stood at the newest pose heard.
    pub fn newest(&self, id: TrafficId) -> Option<Pose> {
        let ms = self.rows.get(&id)?.samples.back()?.ms;
        self.sample(id, ms)
    }

    /// Where vehicle `id` is at host-clock time `ms`, between its poses; past the newest
    /// one it is guessed on (up to `MAX_EXTRAPOLATE`) from its speed and turn.
    pub fn sample(&self, id: TrafficId, ms: f64) -> Option<Pose> {
        let r = self.rows.get(&id)?;
        let s = &r.samples;
        let last = s.back()?;
        let flags = |r: &Row| (r.blinker(), r.brake(), r.lights(), r.at_station());
        let pose = |a: &Row,
                    pos: [f64; 3],
                    heading: f64,
                    extrapolated: bool,
                    teleported: bool,
                    f: &Row| {
            let (blinker, brake, lights, at_station) = flags(f);
            Pose {
                pos,
                heading,
                pitch: a.pitch_deg(),
                bank: a.bank_deg(),
                speed: a.speed_ms(),
                steer: a.steer_deg(),
                blinker,
                brake,
                lights,
                at_station,
                extrapolated,
                teleported,
            }
        };
        if ms >= last.ms {
            let (pos, heading) = if s.len() >= 2 && last.row.speed_ms().abs() >= 0.1 {
                let prev = &s[s.len() - 2];
                let span = ((last.ms - prev.ms) / 1000.0).max(0.02);
                let dt = ((ms - last.ms).min(MAX_EXTRAPOLATE)) / 1000.0;
                let (a, b) = (prev.row.position(), last.row.position());
                let jumped = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt() > TELEPORT;
                // the turn it was making, kept for a while: a quarter turn a second at most
                let turn = if jumped || span > 1.0 {
                    0.0
                } else {
                    let d = (last.row.heading_deg() - prev.row.heading_deg() + 540.0)
                        .rem_euclid(360.0)
                        - 180.0;
                    (d / span).clamp(-90.0, 90.0)
                };
                let mid = (last.row.heading_deg() + turn * dt / 2.0)
                    .rem_euclid(360.0)
                    .to_radians();
                let v = last.row.speed_ms() * dt;
                let vz = if jumped || span > 1.0 {
                    0.0
                } else {
                    ((b[2] - a[2]) / span).clamp(-6.0, 6.0)
                };
                (
                    [b[0] + mid.sin() * v, b[1] + mid.cos() * v, b[2] + vz * dt],
                    (last.row.heading_deg() + turn * dt).rem_euclid(360.0),
                )
            } else {
                (last.row.position(), last.row.heading_deg())
            };
            return Some(pose(
                &last.row,
                pos,
                heading,
                ms > last.ms,
                false,
                &last.row,
            ));
        }
        let first = s.front()?;
        if ms <= first.ms {
            return Some(pose(
                &first.row,
                first.row.position(),
                first.row.heading_deg(),
                false,
                false,
                &first.row,
            ));
        }
        let k = s.partition_point(|x| x.ms <= ms);
        let (a, b) = (&s[k - 1], &s[k]);
        let (pa, pb) = (a.row.position(), b.row.position());
        if ((pb[0] - pa[0]).powi(2) + (pb[1] - pa[1]).powi(2)).sqrt() > TELEPORT {
            return Some(pose(&b.row, pb, b.row.heading_deg(), false, true, &b.row));
        }
        let t = ((ms - a.ms) / (b.ms - a.ms).max(1.0)).clamp(0.0, 1.0);
        let pos = [
            lerp(pa[0], pb[0], t),
            lerp(pa[1], pb[1], t),
            lerp(pa[2], pb[2], t),
        ];
        let heading = lerp_angle(a.row.heading_deg(), b.row.heading_deg(), t);
        let mut out = pose(
            &b.row,
            pos,
            heading,
            false,
            false,
            if t < 0.5 { &a.row } else { &b.row },
        );
        out.pitch = lerp(a.row.pitch_deg(), b.row.pitch_deg(), t);
        out.bank = lerp(a.row.bank_deg(), b.row.bank_deg(), t);
        out.speed = lerp(a.row.speed_ms(), b.row.speed_ms(), t);
        out.steer = lerp(a.row.steer_deg(), b.row.steer_deg(), t);
        Some(out)
    }
}
