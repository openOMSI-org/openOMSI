//! The traffic table on the wire (`TRAF`, host to client; `TACK`, client to host).
//!
//! ```text
//! TRAF
//! byte 0      0xBA
//! byte 1      protocol version
//! byte 2      epoch: changes when the host starts the client's table anew
//! byte 3      0
//! bytes 4-5   datagram number (little endian, wraps): what a TACK names
//! bytes 6-7   snapshot number (wraps): the datagrams of one moment share it; a row's
//!             baseline is named by it
//! bytes 8-11  the host's clock (ms since its session began)
//! bytes 12-19 anchor x, y (whole metres, i32), bytes 20-21 anchor z (i16): the client's
//!             place; full rows are centimetres from it
//! then, least significant bit first:
//!   looks 5, each:    index 12, file (8 + bytes), scheme 8 (255: none), paint (8 + bytes)
//!   spawns 6, each:   id 20, look 12, kind 3, a full row
//!   metas 5, each:    id 20, line (8 + bytes), destination (8 + bytes)
//!   removes 6, each:  id 20
//!   rows 8, each:     id 20, delta 1, then
//!       full row:  x 20, y 20 (cm from the anchor), z 18, heading 12 (360/4096 deg),
//!                  pitch 8, bank 8 (0.1 deg), speed 11 (0.05 m/s), steer 8 (0.5 deg),
//!                  flags 6 (indicators 2, brake, headlights, stop 2)
//!       delta:     baseline 5 (snapshots back), pose 1, [dx 13, dy 13, dz 9 (cm), dh 8],
//!                  changed 4 (attitude, speed, steer, flags), then those fields as in a
//!                  full row
//!   meta numbers 5 (as many as the metas, in their order; 0: none), each:
//!                     number (8 + bytes), registration (8 + bytes), depot file (8 + bytes)
//!
//! The meta numbers come last so that a client older than them reads the datagram as before
//! and leaves them be (a decoder stops after the rows); a datagram of an older host ends
//! there, its padding reading as no numbers.
//!
//! TACK
//! byte 0 0xBB, byte 1 protocol, byte 2 epoch, bytes 3-4 the newest datagram number heard,
//! bytes 5-8 which of the 32 before it were heard too (bit 0: the one before), byte 9 the
//! number n of troubles, then n times 3 bytes: id 20 bits, trouble 4 bits (0: send this
//! row whole, 1: I know no such vehicle)
//! ```
//!
//! A row that is not a delta, and every spawn, stands by itself. A delta is against the row
//! the host knows the client holds (one it acknowledged), `baseline` snapshots back.

use super::ids::{TrafficId, ID_BITS};
use super::row::{Kind, Look, Meta, Row, FLAGS_BITS, MAX_DESTINATION, MAX_HOF, MAX_IDENT, MAX_LINE, MAX_NUMBER};
use crate::wire::{BitReader, BitWriter};

pub const MAGIC: u8 = 0xBA;
pub const ACK_MAGIC: u8 = 0xBB;
pub const HEADER: usize = 22;
/// Below the MTU of the VPNs players use (see `world::MAX_WORLD_DATAGRAM`).
pub const MAX_DATAGRAM: usize = 1180;
/// The farthest back (in snapshots) a delta's baseline may lie.
pub const MAX_BASELINE: u32 = 31;
pub const MAX_STRING: usize = 255;

pub const COUNT_BITS: usize = 5 + 6 + 5 + 6 + 8 + 5;
const MAX_LOOKS: usize = 31;
const MAX_SPAWNS: usize = 63;
const MAX_METAS: usize = 31;
const MAX_REMOVES: usize = 63;
const MAX_ROWS: usize = 255;
pub const FULL_BITS: usize = 20 + 20 + 18 + 12 + 8 + 8 + 11 + 8 + FLAGS_BITS as usize;
pub const SPAWN_BITS: usize = ID_BITS as usize + 12 + 3 + FULL_BITS;
pub const REMOVE_BITS: usize = ID_BITS as usize;
pub const FULL_ROW_BITS: usize = ID_BITS as usize + 1 + FULL_BITS;

/// Where a client is, in whole metres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Anchor {
    pub x: i32,
    pub y: i32,
    pub z: i16,
}

impl Anchor {
    pub fn at(pos: [f64; 3]) -> Anchor {
        let r = |v: f64| if v.is_finite() { v.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32 } else { 0 };
        Anchor { x: r(pos[0]), y: r(pos[1]), z: r(pos[2]).clamp(i16::MIN as i32, i16::MAX as i32) as i16 }
    }

    /// `row` in centimetres from the anchor, when that fits a full row.
    fn rel(&self, row: &Row) -> Option<(i64, i64, i64)> {
        let (dx, dy, dz) = (row.x as i64 - self.x as i64 * 100, row.y as i64 - self.y as i64 * 100, row.z as i64 - self.z as i64 * 100);
        let ok = |v: i64, bits: u32| v >= -(1 << (bits - 1)) && v < 1 << (bits - 1);
        (ok(dx, 20) && ok(dy, 20) && ok(dz, 18)).then_some((dx, dy, dz))
    }

    pub fn reaches(&self, row: &Row) -> bool {
        self.rel(row).is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spawn {
    pub id: TrafficId,
    pub look: u16,
    pub kind: Kind,
    pub row: Row,
}

/// What a delta changes of its baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Delta {
    /// dx, dy, dz (cm) and the turn of the heading (steps).
    pub pose: Option<[i16; 4]>,
    pub attitude: Option<(i8, i8)>,
    pub speed: Option<i16>,
    pub steer: Option<i8>,
    pub flags: Option<u8>,
}

impl Delta {
    /// `new` as a change of `base`; None when it is too far from it for a delta.
    pub fn between(base: &Row, new: &Row) -> Option<Delta> {
        let (dx, dy, dz) = (new.x as i64 - base.x as i64, new.y as i64 - base.y as i64, new.z as i64 - base.z as i64);
        let dh = (new.heading as i64 - base.heading as i64 + 2048).rem_euclid(4096) - 2048;
        let fits = |v: i64, bits: u32| v >= -(1 << (bits - 1)) && v < 1 << (bits - 1);
        if !(fits(dx, 13) && fits(dy, 13) && fits(dz, 9) && fits(dh, 8)) {
            return None;
        }
        Some(Delta {
            pose: (dx != 0 || dy != 0 || dz != 0 || dh != 0).then_some([dx as i16, dy as i16, dz as i16, dh as i16]),
            attitude: (new.pitch != base.pitch || new.bank != base.bank).then_some((new.pitch, new.bank)),
            speed: (new.speed != base.speed).then_some(new.speed),
            steer: (new.steer != base.steer).then_some(new.steer),
            flags: (new.flags != base.flags).then_some(new.flags),
        })
    }

    pub fn apply(&self, base: &Row) -> Row {
        let mut r = *base;
        if let Some([dx, dy, dz, dh]) = self.pose {
            r.x = r.x.saturating_add(dx as i32);
            r.y = r.y.saturating_add(dy as i32);
            r.z = r.z.saturating_add(dz as i32);
            r.heading = (r.heading as i32 + dh as i32).rem_euclid(4096) as u16;
        }
        if let Some((p, b)) = self.attitude {
            (r.pitch, r.bank) = (p, b);
        }
        if let Some(s) = self.speed {
            r.speed = s;
        }
        if let Some(s) = self.steer {
            r.steer = s;
        }
        if let Some(f) = self.flags {
            r.flags = f;
        }
        r
    }

    pub fn bits(&self) -> usize {
        ID_BITS as usize
            + 1
            + 5
            + 1
            + if self.pose.is_some() { 43 } else { 0 }
            + 4
            + if self.attitude.is_some() { 16 } else { 0 }
            + if self.speed.is_some() { 11 } else { 0 }
            + if self.steer.is_some() { 8 } else { 0 }
            + if self.flags.is_some() { FLAGS_BITS as usize } else { 0 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RowUpdate {
    /// A row by itself (absolute here: the anchor is added when it is read).
    Full { id: TrafficId, row: Row },
    /// A change of the row the sender knows the receiver holds, `base_back` snapshots back.
    Delta { id: TrafficId, base_back: u8, delta: Delta },
}

impl RowUpdate {
    pub fn id(&self) -> TrafficId {
        match self {
            RowUpdate::Full { id, .. } | RowUpdate::Delta { id, .. } => *id,
        }
    }

    pub fn bits(&self) -> usize {
        match self {
            RowUpdate::Full { .. } => FULL_ROW_BITS,
            RowUpdate::Delta { delta, .. } => delta.bits(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Packet {
    pub epoch: u8,
    pub seq: u16,
    pub snap: u16,
    pub host_ms: u32,
    pub anchor: Anchor,
    pub looks: Vec<(u16, Look)>,
    pub spawns: Vec<Spawn>,
    pub metas: Vec<(TrafficId, Meta)>,
    pub removes: Vec<TrafficId>,
    pub rows: Vec<RowUpdate>,
}

/// `s` cut to what a string of the wire holds.
pub fn wire_str(s: &str) -> &str {
    if s.len() <= MAX_STRING {
        return s;
    }
    let mut end = MAX_STRING;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

pub fn str_bits(s: &str) -> usize {
    8 + 8 * wire_str(s).len()
}

pub fn look_bits(l: &Look) -> usize {
    12 + str_bits(&l.file) + 8 + str_bits(&l.paint)
}

pub fn meta_bits(m: &Meta) -> usize {
    ID_BITS as usize + str_bits(&m.line) + str_bits(&m.destination) + str_bits(&m.number) + str_bits(&m.ident) + str_bits(&m.hof)
}

/// A look that fits the wire, and a file a client may open.
pub fn look_is_sendable(l: &Look) -> bool {
    l.file.len() <= MAX_STRING && l.paint.len() <= MAX_STRING && crate::world::content_file(&l.file, &["bus", "ovh", "sco"]).is_some()
}

impl Packet {
    /// The size of the datagram this makes, in bytes.
    pub fn size(&self) -> usize {
        let bits = COUNT_BITS
            + self.looks.iter().map(|(_, l)| look_bits(l)).sum::<usize>()
            + self.spawns.len() * SPAWN_BITS
            + self.metas.iter().map(|(_, m)| meta_bits(m)).sum::<usize>()
            + self.removes.len() * REMOVE_BITS
            + self.rows.iter().map(|r| r.bits()).sum::<usize>();
        HEADER + bits.div_ceil(8)
    }

    pub fn is_empty(&self) -> bool {
        self.looks.is_empty() && self.spawns.is_empty() && self.metas.is_empty() && self.removes.is_empty() && self.rows.is_empty()
    }
}

fn put_str(w: &mut BitWriter, s: &str) {
    let s = wire_str(s);
    w.put(s.len() as u64, 8);
    for b in s.bytes() {
        w.put(b as u64, 8);
    }
}

/// A string of the wire, cleaned of separators and control characters and cut to
/// `max_chars` (a file path, which is checked as it is, to `None`).
fn get_str(r: &mut BitReader, max_chars: Option<usize>) -> Option<String> {
    let n = r.get(8)? as usize;
    let mut bytes = Vec::with_capacity(n);
    for _ in 0..n {
        bytes.push(r.get(8)? as u8);
    }
    let s = String::from_utf8(bytes).ok()?;
    Some(match max_chars {
        Some(m) => crate::clean_text(&s, m),
        None => s,
    })
}

fn put_full(w: &mut BitWriter, row: &Row, rel: (i64, i64, i64)) {
    w.put_signed(rel.0, 20);
    w.put_signed(rel.1, 20);
    w.put_signed(rel.2, 18);
    w.put(row.heading as u64, 12);
    w.put_signed(row.pitch as i64, 8);
    w.put_signed(row.bank as i64, 8);
    w.put_signed(row.speed as i64, 11);
    w.put_signed(row.steer as i64, 8);
    w.put(row.flags as u64, FLAGS_BITS);
}

fn get_full(r: &mut BitReader, a: &Anchor) -> Option<Row> {
    let dx = r.get_signed(20)?;
    let dy = r.get_signed(20)?;
    let dz = r.get_signed(18)?;
    Some(Row {
        x: (a.x as i64 * 100 + dx) as i32,
        y: (a.y as i64 * 100 + dy) as i32,
        z: (a.z as i64 * 100 + dz) as i32,
        heading: r.get(12)? as u16,
        pitch: r.get_signed(8)? as i8,
        bank: r.get_signed(8)? as i8,
        speed: r.get_signed(11)? as i16,
        steer: r.get_signed(8)? as i8,
        flags: r.get(FLAGS_BITS)? as u8,
    })
}

/// The datagram of `p`. None when something does not fit: a count, a string, a place too
/// far from the anchor, a datagram of more than `MAX_DATAGRAM` bytes.
pub fn encode(p: &Packet, protocol: u8) -> Option<Vec<u8>> {
    encode_with(p, protocol, true)
}

/// `encode`, or (`numbers` false) as a host older than the metas' numbers wrote it.
fn encode_with(p: &Packet, protocol: u8, numbers: bool) -> Option<Vec<u8>> {
    if p.looks.len() > MAX_LOOKS
        || p.spawns.len() > MAX_SPAWNS
        || p.metas.len() > MAX_METAS
        || p.removes.len() > MAX_REMOVES
        || p.rows.len() > MAX_ROWS
        || p.size() > MAX_DATAGRAM
    {
        return None;
    }
    let mut head = Vec::with_capacity(HEADER);
    head.extend_from_slice(&[MAGIC, protocol, p.epoch, 0]);
    head.extend_from_slice(&p.seq.to_le_bytes());
    head.extend_from_slice(&p.snap.to_le_bytes());
    head.extend_from_slice(&p.host_ms.to_le_bytes());
    head.extend_from_slice(&p.anchor.x.to_le_bytes());
    head.extend_from_slice(&p.anchor.y.to_le_bytes());
    head.extend_from_slice(&p.anchor.z.to_le_bytes());
    let mut w = BitWriter::with_header(&head);
    w.put(p.looks.len() as u64, 5);
    for (i, l) in &p.looks {
        w.put(*i as u64, 12);
        put_str(&mut w, &l.file);
        w.put(l.scheme.map(|s| s.min(254)).unwrap_or(255) as u64, 8);
        put_str(&mut w, &l.paint);
    }
    w.put(p.spawns.len() as u64, 6);
    for s in &p.spawns {
        w.put(s.id.raw() as u64, ID_BITS);
        w.put(s.look as u64, 12);
        w.put(s.kind.code(), 3);
        put_full(&mut w, &s.row, p.anchor.rel(&s.row)?);
    }
    w.put(p.metas.len() as u64, 5);
    for (id, m) in &p.metas {
        w.put(id.raw() as u64, ID_BITS);
        put_str(&mut w, &m.line);
        put_str(&mut w, &m.destination);
    }
    w.put(p.removes.len() as u64, 6);
    for id in &p.removes {
        w.put(id.raw() as u64, ID_BITS);
    }
    w.put(p.rows.len() as u64, 8);
    for r in &p.rows {
        match r {
            RowUpdate::Full { id, row } => {
                w.put(id.raw() as u64, ID_BITS);
                w.put(0, 1);
                put_full(&mut w, row, p.anchor.rel(row)?);
            }
            RowUpdate::Delta { id, base_back, delta } => {
                if *base_back == 0 || *base_back as u32 > MAX_BASELINE {
                    return None;
                }
                w.put(id.raw() as u64, ID_BITS);
                w.put(1, 1);
                w.put(*base_back as u64, 5);
                match delta.pose {
                    Some([dx, dy, dz, dh]) => {
                        w.put(1, 1);
                        w.put_signed(dx as i64, 13);
                        w.put_signed(dy as i64, 13);
                        w.put_signed(dz as i64, 9);
                        w.put_signed(dh as i64, 8);
                    }
                    None => w.put(0, 1),
                }
                let mask = delta.attitude.is_some() as u64 | (delta.speed.is_some() as u64) << 1 | (delta.steer.is_some() as u64) << 2 | (delta.flags.is_some() as u64) << 3;
                w.put(mask, 4);
                if let Some((pitch, bank)) = delta.attitude {
                    w.put_signed(pitch as i64, 8);
                    w.put_signed(bank as i64, 8);
                }
                if let Some(s) = delta.speed {
                    w.put_signed(s as i64, 11);
                }
                if let Some(s) = delta.steer {
                    w.put_signed(s as i64, 8);
                }
                if let Some(f) = delta.flags {
                    w.put(f as u64, FLAGS_BITS);
                }
            }
        }
    }
    if !numbers {
        return Some(w.finish());
    }
    w.put(p.metas.len() as u64, 5);
    for (_, m) in &p.metas {
        put_str(&mut w, &m.number);
        put_str(&mut w, &m.ident);
        put_str(&mut w, &m.hof);
    }
    let out = w.finish();
    debug_assert_eq!(out.len(), p.size());
    Some(out)
}

/// The datagram's packet, or None when it is not a traffic datagram of this protocol or is
/// cut short or holds anything a sender cannot make. Whatever it holds decodes to values in
/// range.
pub fn decode(data: &[u8], protocol: u8) -> Option<Packet> {
    if data.len() < HEADER || data.len() > MAX_DATAGRAM || data[0] != MAGIC || data[1] != protocol {
        return None;
    }
    let anchor = Anchor {
        x: i32::from_le_bytes(data[12..16].try_into().ok()?),
        y: i32::from_le_bytes(data[16..20].try_into().ok()?),
        z: i16::from_le_bytes(data[20..22].try_into().ok()?),
    };
    let mut p = Packet {
        epoch: data[2],
        seq: u16::from_le_bytes([data[4], data[5]]),
        snap: u16::from_le_bytes([data[6], data[7]]),
        host_ms: u32::from_le_bytes(data[8..12].try_into().ok()?),
        anchor,
        ..Default::default()
    };
    let mut r = BitReader::new(&data[HEADER..]);
    for _ in 0..r.get(5)? {
        let i = r.get(12)? as u16;
        let file = get_str(&mut r, None)?;
        let scheme = Some(r.get(8)? as u8).filter(|s| *s != 255);
        let paint = get_str(&mut r, Some(64))?;
        // (a file a client would not open is not a vehicle)
        let file = crate::world::content_file(&file, &["bus", "ovh", "sco"])?;
        let look = Look { file, scheme, paint };
        p.looks.push((i, look));
    }
    for _ in 0..r.get(6)? {
        let id = TrafficId::from_raw(r.get(ID_BITS)? as u32)?;
        let look = r.get(12)? as u16;
        let kind = Kind::from_code(r.get(3)?);
        let row = get_full(&mut r, &anchor)?;
        p.spawns.push(Spawn { id, look, kind, row });
    }
    for _ in 0..r.get(5)? {
        let id = TrafficId::from_raw(r.get(ID_BITS)? as u32)?;
        let line = get_str(&mut r, Some(MAX_LINE))?;
        let destination = get_str(&mut r, Some(MAX_DESTINATION))?;
        p.metas.push((id, Meta { line, destination, ..Default::default() }));
    }
    for _ in 0..r.get(6)? {
        p.removes.push(TrafficId::from_raw(r.get(ID_BITS)? as u32)?);
    }
    for _ in 0..r.get(8)? {
        let id = TrafficId::from_raw(r.get(ID_BITS)? as u32)?;
        if r.get(1)? == 0 {
            p.rows.push(RowUpdate::Full { id, row: get_full(&mut r, &anchor)? });
            continue;
        }
        let base_back = r.get(5)? as u8;
        if base_back == 0 {
            return None;
        }
        let pose = if r.get(1)? == 1 {
            Some([r.get_signed(13)? as i16, r.get_signed(13)? as i16, r.get_signed(9)? as i16, r.get_signed(8)? as i16])
        } else {
            None
        };
        let mask = r.get(4)?;
        let attitude = if mask & 1 != 0 { Some((r.get_signed(8)? as i8, r.get_signed(8)? as i8)) } else { None };
        let speed = if mask & 2 != 0 { Some(r.get_signed(11)? as i16) } else { None };
        let steer = if mask & 4 != 0 { Some(r.get_signed(8)? as i8) } else { None };
        let flags = if mask & 8 != 0 { Some(r.get(FLAGS_BITS)? as u8) } else { None };
        p.rows.push(RowUpdate::Delta { id, base_back, delta: Delta { pose, attitude, speed, steer, flags } });
    }
    // the metas' numbers (a host older than them sends none: the padding reads as 0, or
    // there are not even 5 bits left)
    if !p.metas.is_empty() && r.get(5) == Some(p.metas.len() as u64) {
        let mut numbers = Vec::with_capacity(p.metas.len());
        for _ in 0..p.metas.len() {
            let Some(n) = (|| Some((get_str(&mut r, Some(MAX_NUMBER))?, get_str(&mut r, Some(MAX_IDENT))?, get_str(&mut r, Some(MAX_HOF))?)))() else {
                numbers.clear();
                break;
            };
            numbers.push(n);
        }
        for ((_, m), (number, ident, hof)) in p.metas.iter_mut().zip(numbers) {
            (m.number, m.ident, m.hof) = (number, ident, hof);
        }
    }
    Some(p)
}

/// What a client says of what it has heard.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ack {
    pub epoch: u8,
    /// The newest datagram number heard.
    pub seq: u16,
    /// Bit i: the datagram `seq - 1 - i` was heard too.
    pub mask: u32,
    pub troubles: Vec<(TrafficId, Trouble)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trouble {
    /// The client cannot use deltas of this vehicle: send the row whole.
    SendWhole,
    /// The client knows no such vehicle.
    Unknown,
}

pub const MAX_TROUBLES: usize = 8;

impl Ack {
    /// Whether datagram `seq` is named by this ack.
    pub fn covers(&self, seq: u16) -> bool {
        let back = self.seq.wrapping_sub(seq);
        back == 0 || (back <= 32 && self.mask >> (back - 1) & 1 == 1)
    }
}

pub fn encode_ack(a: &Ack, protocol: u8) -> Vec<u8> {
    let mut out = vec![ACK_MAGIC, protocol, a.epoch];
    out.extend_from_slice(&a.seq.to_le_bytes());
    out.extend_from_slice(&a.mask.to_le_bytes());
    let n = a.troubles.len().min(MAX_TROUBLES);
    out.push(n as u8);
    for (id, t) in a.troubles.iter().take(n) {
        let v = id.raw() | match t {
            Trouble::SendWhole => 0,
            Trouble::Unknown => 1,
        } << ID_BITS;
        out.extend_from_slice(&v.to_le_bytes()[..3]);
    }
    out
}

pub fn decode_ack(data: &[u8], protocol: u8) -> Option<Ack> {
    if data.len() < 10 || data[0] != ACK_MAGIC || data[1] != protocol {
        return None;
    }
    let n = data[9] as usize;
    if n > MAX_TROUBLES || data.len() != 10 + 3 * n {
        return None;
    }
    let mut troubles = Vec::with_capacity(n);
    for k in 0..n {
        let b = &data[10 + 3 * k..13 + 3 * k];
        let v = u32::from_le_bytes([b[0], b[1], b[2], 0]);
        let id = TrafficId::from_raw(v & ((1 << ID_BITS) - 1))?;
        let t = match v >> ID_BITS {
            0 => Trouble::SendWhole,
            1 => Trouble::Unknown,
            _ => return None,
        };
        troubles.push((id, t));
    }
    Some(Ack { epoch: data[2], seq: u16::from_le_bytes([data[3], data[4]]), mask: u32::from_le_bytes(data[5..9].try_into().ok()?), troubles })
}

/// Whether snapshot number `a` is newer than `b` (they wrap).
pub fn newer(a: u16, b: u16) -> bool {
    a != b && a.wrapping_sub(b) < 0x8000
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(slot: u16, g: u8) -> TrafficId {
        TrafficId::new(slot, g)
    }

    fn row(i: i32) -> Row {
        Row::from_state([892_248.18 + i as f64, 4_196_461.37, 33.21], 271.3 + i as f64, -1.2, 0.4, 13.85, -7.5, 2, true, true, -1)
    }

    fn look() -> Look {
        Look { file: "Vehicles/MAN_NL_NG/MAN_EN92_main.bus".into(), scheme: Some(3), paint: "BVG 1990".into() }
    }

    fn packet() -> Packet {
        let a = Anchor::at([892_300.0, 4_196_400.0, 33.0]);
        let base = row(0);
        let moved = Row { x: base.x + 120, y: base.y - 40, heading: (base.heading + 4090) % 4096, speed: base.speed + 7, ..base };
        Packet {
            epoch: 9,
            seq: 65535,
            snap: 3,
            host_ms: 123_456_789,
            anchor: a,
            looks: vec![(7, look())],
            spawns: vec![Spawn { id: id(1, 1), look: 7, kind: Kind::Bus, row: row(1) }],
            metas: vec![(id(1, 1), Meta { line: "37".into(), destination: "#4".into(), number: "80.849".into(), ident: "EMT-1234".into(), hof: "Brigadeiro".into() })],
            removes: vec![id(2, 5), id(4095, 255)],
            rows: vec![
                RowUpdate::Full { id: id(3, 1), row: row(2) },
                RowUpdate::Delta { id: id(4, 2), base_back: 2, delta: Delta::between(&base, &moved).unwrap() },
                RowUpdate::Delta { id: id(5, 2), base_back: 31, delta: Delta::default() },
            ],
        }
    }

    #[test]
    fn a_packet_round_trips_in_spandau_coordinates() {
        let p = packet();
        let d = encode(&p, 15).unwrap();
        assert_eq!(d.len(), p.size());
        let q = decode(&d, 15).unwrap();
        assert_eq!(q, p);
        // anything else is not a packet; cut short is none either
        assert!(decode(&d, 14).is_none());
        for n in 0..d.len() {
            let _ = decode(&d[..n], 15);
        }
        // cut short in the rows it is none; cut in the metas' numbers it reads as an older
        // host's datagram, without them
        let old = encode_with(&p, 15, false).unwrap();
        assert!(decode(&old[..old.len() - 4], 15).is_none());
        for n in old.len()..d.len() {
            if let Some(q) = decode(&d[..n], 15) {
                assert_eq!(q.rows, p.rows);
                assert!(q.metas.iter().all(|(_, m)| m.number.is_empty() && m.hof.is_empty()), "{n}");
            }
        }
    }

    /// A datagram of a host older than the metas' numbers (it ends after the rows) reads as
    /// before, the numbers empty; an older client stops after the rows, and what it reads of a
    /// new host's datagram is that same older datagram.
    #[test]
    fn the_metas_numbers_are_left_out_by_an_older_host_and_left_be_by_an_older_client() {
        let p = packet();
        let old = encode_with(&p, 15, false).unwrap();
        let q = decode(&old, 15).unwrap();
        assert_eq!(q.metas[0].1, Meta { line: "37".into(), destination: "#4".into(), ..Default::default() });
        assert_eq!((&q.looks, &q.spawns, &q.removes, &q.rows), (&p.looks, &p.spawns, &p.removes, &p.rows));
        // the new datagram is the old one with the numbers after it (an older client's
        // decoder is this one's without them)
        let new = encode(&p, 15).unwrap();
        assert_eq!(new[..old.len() - 1], old[..old.len() - 1]);
        assert_eq!(decode(&new, 15).unwrap().metas, p.metas);
    }

    #[test]
    fn a_delta_costs_less_than_a_whole_row() {
        let base = row(0);
        let moved = Row { x: base.x + 150, y: base.y + 20, ..base };
        let d = Delta::between(&base, &moved).unwrap();
        assert_eq!(d.apply(&base), moved);
        assert!(d.bits() + 20 < FULL_ROW_BITS, "{} bits against {}", d.bits(), FULL_ROW_BITS);
        // an unchanged row is a few bits
        let same = Delta::between(&base, &base).unwrap();
        assert_eq!(same, Delta::default());
        assert_eq!(same.bits(), ID_BITS as usize + 1 + 5 + 1 + 4);
        // too far for a delta: a whole row is sent instead
        assert!(Delta::between(&base, &Row { x: base.x + 5000, ..base }).is_none());
        assert!(Delta::between(&base, &Row { heading: (base.heading + 200) % 4096, ..base }).is_none());
        // the heading turns the short way round north
        let a = Row { heading: 4090, ..base };
        let b = Row { heading: 6, ..base };
        let d = Delta::between(&a, &b).unwrap();
        assert_eq!(d.pose.unwrap()[3], 12);
        assert_eq!(d.apply(&a), b);
    }

    #[test]
    fn what_does_not_fit_is_not_encoded() {
        let mut p = packet();
        p.rows.push(RowUpdate::Full { id: id(9, 1), row: Row { x: p.anchor.x * 100 + 600_000, ..row(0) } });
        assert!(encode(&p, 15).is_none(), "6 km from the anchor");
        let mut p = packet();
        p.rows = (0..100).map(|i| RowUpdate::Full { id: id(i + 10, 1), row: row(0) }).collect();
        assert!(p.size() > MAX_DATAGRAM && encode(&p, 15).is_none());
        let mut p = packet();
        p.rows = vec![RowUpdate::Delta { id: id(1, 1), base_back: 0, delta: Delta::default() }];
        assert!(encode(&p, 15).is_none());
    }

    #[test]
    fn a_bad_file_or_a_bad_id_is_not_taken() {
        for file in ["../../x.bus", "/etc/passwd.bus", "Vehicles/x.exe", "Vehicles/a|b.bus"] {
            let mut p = packet();
            p.looks = vec![(0, Look { file: file.into(), ..look() })];
            // (the sender checks nothing; the receiver does)
            let d = encode(&p, 15);
            assert!(d.is_none_or(|d| decode(&d, 15).is_none()), "{file}");
        }
        // generation 0 is no id
        assert!(TrafficId::from_raw(0x0FFF).is_none());
    }

    #[test]
    fn an_ack_names_the_datagrams_heard() {
        let a = Ack { epoch: 4, seq: 2, mask: 0b101, troubles: vec![(id(7, 3), Trouble::SendWhole), (id(9, 1), Trouble::Unknown)] };
        let b = decode_ack(&encode_ack(&a, 15), 15).unwrap();
        assert_eq!(a, b);
        assert!(a.covers(2) && a.covers(1) && !a.covers(0) && a.covers(65535));
        assert!(!a.covers(3), "newer than the ack");
        assert!(!a.covers(2u16.wrapping_sub(40)), "too long ago");
        assert!(decode_ack(&encode_ack(&a, 15), 14).is_none());
        assert!(decode_ack(&encode_ack(&a, 15)[..11], 15).is_none());
    }

    #[test]
    fn snapshot_numbers_wrap() {
        assert!(newer(1, 65535) && !newer(65535, 1) && !newer(5, 5) && newer(10, 9));
    }
}
