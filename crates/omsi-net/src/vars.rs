//! Every script variable of a player's vehicle, for the copies the others draw (`VARS`).
//!
//! The pose (`wire.rs`) carries what every copy needs at the full rate: where the bus is, its
//! lamps, switches and the values its sounds and moving parts follow. Everything else the
//! scripts work out - a gearbox's state, a display's mode, an IBIS, a ticket printer, a
//! variable a plugin or a `setvar` set - went nowhere, and another player's bus showed what
//! its own copy of the scripts made of it. This sends all of them: ten times a second the
//! variables that changed since they were last sent, and all of them in turn in between (a
//! "key frame" a slice at a time), so a lost datagram is mended within seconds and a player
//! who joins late sees the whole state. Strings go the same way.
//!
//! A message names the vehicle's variable table by a hash of its variables' names (the
//! game's). When tables differ the game can use the separate name manifest to
//! map common variables, after verifying model and sound configuration references.
//!
//! ```text
//! byte 0     0xB5 (no text message starts with it)
//! byte 1     protocol
//! bytes 2-5  sender's id (u32 LE)
//! bytes 6-9  table hash (u32 LE)
//! byte 10    kind: 0 floats by index, 1 a run of floats, 2 strings, 3 string fragment,
//!                 4 timestamped animation floats
//! then       0: count u16, (index u16, f32)*      1: first u16, count u16, f32*
//!            2: count u16, (index u16, version u64, length u16, UTF-8)*
//!            3: index u16, version u64, total u32, offset u32, UTF-8 bytes
//!            4: sent_ms u32, count u16, (index u16, f32)* (animation samples)
//! ```
//! All numbers little-endian. Indices refer to name-sorted tables, independently of local compiler IDs.

/// The first byte of a `VARS` datagram.
pub const VARS_MAGIC: u8 = 0xB5;
/// The first byte of a datagram of the coupled cars' own variables (a tram's rear cab, whose
/// scripts keep values of their own): laid out as `VARS`, on a channel of its own so that
/// its strings do not discard the leading vehicle's in flight. A game without it ignores it.
pub const CAR_VARS_MAGIC: u8 = 0xBE;
const HEADER: usize = 11;
/// What one datagram holds at most (within `MAX_DATAGRAM`, with room to spare).
const ROOM: usize = 1300;
/// How often changes go out (s), and how many ticks between key frame slices.
const EVERY: f32 = 0.1;
const KEY_EVERY: u32 = 5;
const STRINGS_KEY_EVERY: u32 = 10;
/// Long strings are fragmented without cutting UTF-8 or their contents.
const MAX_INLINE: usize = ROOM - HEADER - 2 - 12;
const MAX_STRING: usize = 1024 * 1024;
const CHUNK: usize = ROOM - HEADER - 18;

#[derive(Debug)]
struct PendingString {
    index: u16,
    version: u64,
    value: String,
    offset: usize,
    awaiting: Vec<u32>,
}

/// Variables of a player's vehicle as they came (`LanSession::take_vars`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VarsIn {
    pub id: u32,
    pub table: u32,
    /// Owner's frame clock for animation samples; zero for ordinary variable updates.
    pub sent_ms: u32,
    pub floats: Vec<(u16, f32)>,
    pub strings: Vec<(u16, String)>,
}

/// Our side: what was sent last, and where the key frame has got to.
#[derive(Debug, Default)]
pub struct VarSender {
    table: u32,
    floats: Vec<f32>,
    strings: Vec<String>,
    acc: f32,
    ticks: u32,
    delta_from: usize,
    delta_string: usize,
    key_float: usize,
    key_string: usize,
    long_delta: usize,
    long_key: usize,
    long_key_due: bool,
    versions: Vec<u64>,
    sequence: u64,
    pending: Option<PendingString>,
    subscribers: Vec<u32>,
}

fn header(protocol: u8, id: u32, table: u32, kind: u8) -> Vec<u8> {
    let mut d = Vec::with_capacity(ROOM + HEADER);
    d.push(VARS_MAGIC);
    d.push(protocol);
    d.extend_from_slice(&id.to_le_bytes());
    d.extend_from_slice(&table.to_le_bytes());
    d.push(kind);
    d
}

fn same(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

/// Complete, timestamped animation inputs, including unchanged values. Delta-only
/// samples would interpolate a new movement across the preceding stationary period.
#[derive(Debug, Default)]
pub struct AnimationSender {
    table: Option<u32>,
    acc: f32,
}

impl AnimationSender {
    pub fn interval(count: usize) -> f32 {
        let per_packet = (ROOM - HEADER - 6) / 6;
        EVERY.max(count.div_ceil(per_packet) as f32 / 20.0)
    }

    pub fn tick(&mut self, protocol: u8, id: u32, table: u32, sent_ms: u32, values: &[(u16, f32)], dt: f32) -> Vec<Vec<u8>> {
        if self.table != Some(table) {
            self.table = Some(table);
            self.acc = EVERY;
        }
        self.acc += dt;
        // Keep the added traffic at twenty datagrams/second even for large models.
        let per_packet = (ROOM - HEADER - 6) / 6;
        let interval = Self::interval(values.len());
        if values.is_empty() || self.acc < interval { return Vec::new(); }
        self.acc = 0.0;
        values.chunks(per_packet).map(|chunk| {
            let mut d = header(protocol, id, table, 4);
            d.extend_from_slice(&sent_ms.max(1).to_le_bytes());
            d.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
            for &(index, value) in chunk {
                d.extend_from_slice(&index.to_le_bytes());
                d.extend_from_slice(&value.to_le_bytes());
            }
            d
        }).collect()
    }
}

impl VarSender {
    pub fn subscribers(&mut self, peers: &[u32]) {
        self.subscribers = peers.to_vec();
        if let Some(p) = &mut self.pending { p.awaiting.retain(|id| peers.contains(id)); }
    }

    pub fn acknowledge(&mut self, peer: u32, table: u32, index: u16, version: u64) {
        if self.table != table { return; }
        if let Some(p) = &mut self.pending {
            if p.index == index && p.version == version { p.awaiting.retain(|id| *id != peer); }
        }
    }

    /// Our vehicle's variables (`ids[k]` holds `values[k]`, the same for strings) for this
    /// frame: the datagrams due now (none most frames).
    #[allow(clippy::too_many_arguments)]
    pub fn tick(&mut self, protocol: u8, id: u32, table: u32, float_ids: &[u16], floats: &[f32], string_ids: &[u16], strings: &[String], dt: f32) -> Vec<Vec<u8>> {
        if table != self.table || self.floats.len() != floats.len() || self.strings.len() != strings.len() {
            // another vehicle (or another table): everything is new
            let subscribers = self.subscribers.clone();
            let sequence = self.sequence;
            *self = VarSender { sequence, subscribers, versions: vec![0; strings.len()], table, floats: vec![f32::NAN; floats.len()], strings: vec!["\u{0}".into(); strings.len()], acc: EVERY, ..VarSender::default() };
        }
        self.acc += dt;
        if self.acc < EVERY {
            return Vec::new();
        }
        self.acc = 0.0;
        self.ticks = self.ticks.wrapping_add(1);
        let mut out = Vec::new();
        let n = floats.len().min(float_ids.len());
        // what changed, from where the last datagram had to stop
        if n > 0 {
            let mut d = header(protocol, id, table, 0);
            d.extend_from_slice(&0u16.to_le_bytes());
            let mut count = 0u16;
            let mut k = self.delta_from % n;
            let mut looked = 0;
            while looked < n && d.len() + 6 <= ROOM {
                if !same(floats[k], self.floats[k]) {
                    d.extend_from_slice(&float_ids[k].to_le_bytes());
                    d.extend_from_slice(&floats[k].to_le_bytes());
                    self.floats[k] = floats[k];
                    count += 1;
                }
                k = (k + 1) % n;
                looked += 1;
            }
            self.delta_from = k;
            if count > 0 {
                d[HEADER..HEADER + 2].copy_from_slice(&count.to_le_bytes());
                out.push(d);
            }
        }
        // a slice of the key frame: a run of consecutive variables as they are now
        if n > 0 && self.ticks % KEY_EVERY == 0 {
            let first = self.key_float % n;
            let take = ((ROOM - HEADER - 4) / 4).min(n - first);
            // (only a run of ids one after another can go as a run: they mostly are)
            let mut run = 1;
            while run < take && float_ids[first].checked_add(run as u16) == Some(float_ids[first + run]) {
                run += 1;
            }
            // Sparse tables must progress by a packetful too, rather than one id every
            // half second. Keep the compact run encoding where it holds more entries.
            let indexed = ((ROOM - HEADER - 2) / 6).min(n - first);
            let consecutive = run >= indexed;
            let count = if consecutive { run } else { indexed };
            let mut d = header(protocol, id, table, if consecutive { 1 } else { 0 });
            if consecutive {
                d.extend_from_slice(&float_ids[first].to_le_bytes());
            }
            d.extend_from_slice(&(count as u16).to_le_bytes());
            for k in first..first + count {
                if !consecutive {
                    d.extend_from_slice(&float_ids[k].to_le_bytes());
                }
                d.extend_from_slice(&floats[k].to_le_bytes());
                self.floats[k] = floats[k];
            }
            self.key_float = (first + count) % n;
            out.push(d);
        }
        // strings: those that changed, and in turn the others
        let m = strings.len().min(string_ids.len());
        if m > 0 {
            let key = self.ticks % STRINGS_KEY_EVERY == 0;
            let mut d = header(protocol, id, table, 2);
            d.extend_from_slice(&0u16.to_le_bytes());
            let mut count = 0u16;
            let put = |this: &mut Self, d: &mut Vec<u8>, k: usize, count: &mut u16| -> bool {
                let s = &strings[k];
                if s.len() > MAX_INLINE { return true; }
                if d.len() + 12 + s.len() > ROOM {
                    return false;
                }
                if this.strings[k] != *s {
                    this.sequence += 1;
                    this.versions[k] = this.sequence;
                    this.strings[k].clone_from(s);
                }
                d.extend_from_slice(&string_ids[k].to_le_bytes());
                d.extend_from_slice(&this.versions[k].to_le_bytes());
                d.extend_from_slice(&(s.len() as u16).to_le_bytes());
                d.extend_from_slice(s.as_bytes());
                *count += 1;
                true
            };
            // Reserve key ticks for recovery. Changes cannot consume the space needed
            // by unchanged values whose last datagram was lost or predates a new peer.
            if key {
                let mut k = self.key_string % m;
                for _ in 0..m {
                    if !put(self, &mut d, k, &mut count) {
                        break;
                    }
                    k = (k + 1) % m;
                }
                self.key_string = k;
            } else {
                let mut k = self.delta_string % m;
                for _ in 0..m {
                    if strings[k] != self.strings[k] {
                        if !put(self, &mut d, k, &mut count) {
                            break;
                        }
                    }
                    k = (k + 1) % m;
                }
                self.delta_string = k;
            }
            if count > 0 {
                d[HEADER..HEADER + 2].copy_from_slice(&count.to_le_bytes());
                out.push(d);
            }
        }
        // An inline replacement supersedes an obsolete long transfer immediately.
        if self.pending.as_ref().is_some_and(|p| string_ids.iter().position(|id| *id == p.index)
            .is_some_and(|k| strings.get(k).is_some_and(|s| s.len() <= MAX_INLINE))) {
            self.pending = None;
        }
        // Finish a selected snapshot even when its source changes every frame. Recovery
        // uses the same version, so fragments lost on one pass fill in on the next pass.
        if self.ticks % STRINGS_KEY_EVERY == 0 { self.long_key_due = true; }
        if m > 0 && self.pending.is_none() {
            let key = std::mem::take(&mut self.long_key_due);
            let mut k = if key { self.long_key } else { self.long_delta } % m;
            for _ in 0..m {
                let value = &strings[k];
                if value.len() > MAX_INLINE && value.len() <= MAX_STRING && (key || *value != self.strings[k]) {
                    if *value != self.strings[k] {
                        self.sequence += 1;
                        self.versions[k] = self.sequence;
                        self.strings[k].clone_from(value);
                    }
                    self.pending = Some(PendingString { index: string_ids[k], version: self.versions[k], value: value.clone(), offset: 0, awaiting: self.subscribers.clone() });
                    k = (k + 1) % m;
                    break;
                }
                k = (k + 1) % m;
            }
            if key { self.long_key = k; } else { self.long_delta = k; }
        }
        if let Some(pending) = &mut self.pending {
            let end = (pending.offset + CHUNK).min(pending.value.len());
            let mut d = header(protocol, id, table, 3);
            d.extend_from_slice(&pending.index.to_le_bytes());
            d.extend_from_slice(&pending.version.to_le_bytes());
            d.extend_from_slice(&(pending.value.len() as u32).to_le_bytes());
            d.extend_from_slice(&(pending.offset as u32).to_le_bytes());
            d.extend_from_slice(&pending.value.as_bytes()[pending.offset..end]);
            pending.offset = end;
            out.push(d);
            if end == pending.value.len() {
                if pending.awaiting.is_empty() { self.pending = None; }
                else { pending.offset = 0; }
            }
        }
        out
    }
}

/// A `VARS` datagram read (None: not one, another protocol, or cut short).
pub fn decode(data: &[u8], protocol: u8) -> Option<VarsIn> {
    if data.len() < HEADER + 2 || data[0] != VARS_MAGIC || data[1] != protocol {
        return None;
    }
    let u16_at = |at: usize| -> Option<u16> { Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?)) };
    let f32_at = |at: usize| -> Option<f32> { Some(f32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?)) };
    let id = u32::from_le_bytes(data[2..6].try_into().ok()?);
    let table = u32::from_le_bytes(data[6..10].try_into().ok()?);
    let mut v = VarsIn { id, table, ..VarsIn::default() };
    let mut at = HEADER;
    match data[10] {
        4 => {
            v.sent_ms = u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?);
            if v.sent_ms == 0 { return None; }
            at += 4;
            let count = u16_at(at)? as usize;
            at += 2;
            if data.len() != at + count * 6 { return None; }
            for _ in 0..count {
                v.floats.push((u16_at(at)?, f32_at(at + 2)?));
                at += 6;
            }
        }
        0 => {
            let count = u16_at(at)? as usize;
            at += 2;
            for _ in 0..count {
                v.floats.push((u16_at(at)?, f32_at(at + 2)?));
                at += 6;
            }
        }
        1 => {
            let first = u16_at(at)?;
            let count = u16_at(at + 2)?;
            at += 4;
            for k in 0..count {
                v.floats.push((first.checked_add(k)?, f32_at(at)?));
                at += 4;
            }
        }
        2 => {
            let count = u16_at(at)? as usize;
            at += 2;
            for _ in 0..count {
                let idx = u16_at(at)?;
                let len = u16_at(at + 10)? as usize;
                let s = std::str::from_utf8(data.get(at + 12..at + 12 + len)?).ok()?;
                v.strings.push((idx, s.to_string()));
                at += 12 + len;
            }
        }
        3 => {
            let len = u32::from_le_bytes(data.get(at + 10..at + 14)?.try_into().ok()?) as usize;
            let offset = u32::from_le_bytes(data.get(at + 14..at + 18)?.try_into().ok()?) as usize;
            let bytes = data.get(at + 18..)?;
            if len <= MAX_INLINE || len > MAX_STRING || offset % CHUNK != 0 || offset >= len || bytes.len() != CHUNK.min(len - offset) { return None; }
        }
        _ => return None,
    }
    Some(v)
}

#[derive(Debug, Default)]
pub struct VarReceiver {
    peers: std::collections::HashMap<u32, (u32, std::collections::HashMap<u16, ReceivedString>)>,
}

#[derive(Debug)]
struct ReceivedString {
    version: u64,
    complete: bool,
    bytes: Vec<u8>,
    parts: Vec<bool>,
}

impl VarReceiver {
    pub fn completed_fragment(&self, data: &[u8]) -> Option<(u16, u64)> {
        if *data.get(10)? != 3 { return None; }
        let id = u32::from_le_bytes(data.get(2..6)?.try_into().ok()?);
        let index = u16::from_le_bytes(data.get(HEADER..HEADER + 2)?.try_into().ok()?);
        let version = u64::from_le_bytes(data.get(HEADER + 2..HEADER + 10)?.try_into().ok()?);
        let state = self.peers.get(&id)?.1.get(&index)?;
        (state.complete && state.version == version).then_some((index, version))
    }

    pub fn retain_peers(&mut self, peers: &[u32]) {
        self.peers.retain(|id, _| peers.contains(id));
    }

    pub fn receive(&mut self, data: &[u8], protocol: u8) -> Option<VarsIn> {
        let mut value = decode(data, protocol)?;
        if data[10] != 2 && data[10] != 3 { return Some(value); }
        let reserved: usize = self.peers.values().flat_map(|(_, strings)| strings.values()).map(|s| s.bytes.len()).sum();
        let (table, strings) = self.peers.entry(value.id).or_default();
        if *table != value.table { *table = value.table; strings.clear(); }
        fn accept(strings: &mut std::collections::HashMap<u16, ReceivedString>, reserved: usize, index: u16, version: u64, len: usize) -> Option<&mut ReceivedString> {
            if strings.get(&index).is_some_and(|s| s.version > version) { return None; }
            if strings.get(&index).is_none_or(|s| s.version != version) {
                let old = strings.get(&index).map_or(0, |s| s.bytes.len());
                if reserved - old + len > 16 * MAX_STRING { return None; }
                strings.insert(index, ReceivedString { version, complete: false, bytes: vec![0; len], parts: vec![false; len.div_ceil(CHUNK)] });
            }
            strings.get_mut(&index)
        }
        if data[10] == 2 {
            let mut at = HEADER + 2;
            value.strings.retain(|(index, text)| {
                let version = u64::from_le_bytes(data[at + 2..at + 10].try_into().unwrap());
                at += 12 + text.len();
                if let Some(state) = accept(strings, reserved, *index, version, 0) { state.complete = true; true } else { false }
            });
        } else {
            let index = u16::from_le_bytes(data[HEADER..HEADER + 2].try_into().ok()?);
            let version = u64::from_le_bytes(data[HEADER + 2..HEADER + 10].try_into().ok()?);
            let len = u32::from_le_bytes(data[HEADER + 10..HEADER + 14].try_into().ok()?) as usize;
            let offset = u32::from_le_bytes(data[HEADER + 14..HEADER + 18].try_into().ok()?) as usize;
            let bytes = &data[HEADER + 18..];
            if let Some(state) = accept(strings, reserved, index, version, len) {
                if state.bytes.len() != len { return None; }
                if state.complete {
                    if offset == 0 {
                        value.strings.push((index, String::from_utf8(state.bytes.clone()).ok()?));
                    }
                    return Some(value);
                }
                state.bytes[offset..offset + bytes.len()].copy_from_slice(bytes);
                state.parts[offset / CHUNK] = true;
                if state.parts.iter().all(|p| *p) {
                    let text = String::from_utf8(state.bytes.clone()).ok()?;
                    state.parts.clear();
                    state.complete = true;
                    value.strings.push((index, text));
                }
            }
        }
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_long_strings_recover_loss_before_advancing_the_snapshot() {
        let mut tx = VarSender::default();
        let mut rx = VarReceiver::default();
        let mut seen = std::collections::HashSet::new();
        for tick in 0..300 {
            tx.subscribers(&[9]);
            let texts = [format!("{tick}:{}", "ő".repeat(3000)), "other".repeat(900)];
            for packet in tx.tick(14, 7, 99, &[], &[], &[0, 1], &texts, 0.1) {
                // Lose fragments and acknowledgements while both fields keep getting turns.
                if tick % 7 == 1 { continue; }
                for (index, value) in rx.receive(&packet, 14).unwrap().strings {
                    assert!(value == texts[1] || value.ends_with(&"ő".repeat(3000)));
                    seen.insert(index);
                }
                if tick % 5 != 0 {
                    if let Some((index, version)) = rx.completed_fragment(&packet) {
                        tx.acknowledge(9, 99, index, version);
                    }
                }
            }
        }
        assert_eq!(seen.len(), 2);
        assert!(tx.sequence > 5, "continuous changes must advance after recovery");
    }

    #[test]
    fn animation_snapshots_keep_timestamps_and_unchanged_values() {
        let mut sender = AnimationSender::default();
        let mut receiver = VarReceiver::default();
        let values = vec![(0, 0.0), (65535, 1.0)];
        for ms in [1000, 1100, 1200] {
            let packets = sender.tick(12, 7, 99, ms, &values, 0.1);
            assert_eq!(packets.len(), 1);
            let got = receiver.receive(&packets[0], 12).unwrap();
            assert_eq!((got.id, got.table, got.sent_ms), (7, 99, ms));
            assert_eq!(got.floats, values);
            assert!(decode(&packets[0], 11).is_none());
            assert!(decode(&packets[0][..packets[0].len() - 1], 12).is_none());
        }
        assert!(sender.tick(12, 7, 99, 1201, &values, 0.001).is_empty());
    }

    #[test]
    fn animation_snapshots_cover_large_models_without_truncation() {
        let values: Vec<_> = (0..3000).map(|id| (id, id as f32)).collect();
        let mut sender = AnimationSender::default();
        let interval = AnimationSender::interval(values.len());
        let packets = sender.tick(12, 7, 99, 1000, &values, interval);
        assert!(packets.len() as f32 / interval <= 20.0);
        let mut got = Vec::new();
        for packet in packets {
            assert!(packet.len() <= ROOM && packet.len() <= crate::MAX_DATAGRAM);
            let state = decode(&packet, 12).unwrap();
            assert_eq!(state.sent_ms, 1000);
            got.extend(state.floats);
        }
        assert_eq!(got, values);
    }

    /// What goes out is what comes in: the changes at once, the rest by key frame slices,
    /// strings too - and after a few seconds every variable has arrived, whatever was lost.
    #[test]
    fn every_variable_arrives_and_changes_go_at_once() {
        let n = 3000;
        let ids: Vec<u16> = (0..n as u16).collect();
        let mut floats: Vec<f32> = (0..n).map(|k| k as f32 * 0.5).collect();
        let sids: Vec<u16> = (0..40).collect();
        let mut strings: Vec<String> = (0..40).map(|k| format!("text {k}")).collect();
        let mut tx = VarSender::default();
        let mut got = vec![f32::NAN; n];
        let mut got_s = vec![String::new(); 40];
        let take = |msgs: Vec<Vec<u8>>, got: &mut Vec<f32>, got_s: &mut Vec<String>| {
            for m in msgs {
                assert!(m.len() <= crate::MAX_DATAGRAM, "{}", m.len());
                let v = decode(&m, 5).unwrap();
                assert_eq!((v.id, v.table), (7, 99));
                for (i, x) in v.floats {
                    got[i as usize] = x;
                }
                for (i, s) in v.strings {
                    got_s[i as usize] = s;
                }
            }
        };
        // the first message after a change carries it
        take(tx.tick(5, 7, 99, &ids, &floats, &sids, &strings, 0.1), &mut got, &mut got_s);
        floats[2500] = -1.0;
        strings[3] = "Hauptbahnhof".into();
        take(tx.tick(5, 7, 99, &ids, &floats, &sids, &strings, 0.1), &mut got, &mut got_s);
        // (the first tick sent everything changed from "unknown", as far as a datagram holds)
        for _ in 0..200 {
            take(tx.tick(5, 7, 99, &ids, &floats, &sids, &strings, 0.1), &mut got, &mut got_s);
        }
        assert_eq!(got, floats);
        assert_eq!(got_s, strings);
        // an unchanged state sends little: the key frame slices only
        let quiet: usize = (0..10).map(|_| tx.tick(5, 7, 99, &ids, &floats, &sids, &strings, 0.1).iter().map(Vec::len).sum::<usize>()).sum();
        assert!(quiet < 4 * ROOM, "{quiet} bytes a second for nothing new");
        // one change: in the very next datagram
        floats[17] = 4.25;
        let m = tx.tick(5, 7, 99, &ids, &floats, &sids, &strings, 0.1);
        let v = decode(&m[0], 5).unwrap();
        assert_eq!(v.floats, vec![(17, 4.25)]);
    }

    #[test]
    fn changing_strings_do_not_starve_later_strings_or_lost_updates() {
        let ids: Vec<u16> = (0..30).collect();
        let mut strings: Vec<String> = (0..30).map(|k| format!("{k}:{}", "x".repeat(240))).collect();
        let mut tx = VarSender::default();
        // Establish the sender's baseline, but lose these datagrams.
        for _ in 0..20 {
            tx.tick(5, 7, 99, &[], &[], &ids, &strings, 0.1);
        }
        let mut got = vec![None; strings.len()];
        for tick in 0..100 {
            // The first fields change forever and can fill a whole datagram.
            for k in 0..5 {
                strings[k] = format!("{tick}:{k}:{}", "x".repeat(240));
            }
            if tick == 0 {
                strings[29] = "later changed field".into();
            }
            for msg in tx.tick(5, 7, 99, &[], &[], &ids, &strings, 0.1) {
                assert!(msg.len() <= ROOM);
                for (id, value) in decode(&msg, 5).unwrap().strings {
                    got[id as usize] = Some(value);
                }
            }
        }
        for k in 5..strings.len() {
            assert_eq!(got[k].as_deref(), Some(strings[k].as_str()), "string {k} never recovered");
        }
    }

    #[test]
    fn changing_strings_rotate_even_when_every_packet_is_full() {
        let ids: Vec<u16> = (0..20).collect();
        let mut strings = vec!["x".repeat(240); ids.len()];
        let mut tx = VarSender::default();
        for _ in 0..20 {
            tx.tick(5, 7, 99, &[], &[], &ids, &strings, 0.1);
        }
        let mut seen = std::collections::HashSet::new();
        for tick in 0..5 {
            for (k, s) in strings.iter_mut().enumerate() {
                *s = format!("{tick}:{k}:{}", "x".repeat(240));
            }
            for msg in tx.tick(5, 7, 99, &[], &[], &ids, &strings, 0.1) {
                seen.extend(decode(&msg, 5).unwrap().strings.into_iter().map(|(id, _)| id));
            }
        }
        assert_eq!(seen.len(), ids.len(), "all changed strings need a turn before the next key frame");
    }

    #[test]
    fn sparse_float_ids_recover_a_lost_baseline_within_seconds() {
        let ids: Vec<u16> = (0..1000).map(|k| k * 2).collect();
        let values: Vec<f32> = (0..ids.len()).map(|k| k as f32).collect();
        let mut tx = VarSender::default();
        for _ in 0..10 {
            tx.tick(5, 7, 99, &ids, &values, &[], &[], 0.1);
        }
        let mut got = std::collections::HashMap::new();
        for _ in 0..30 {
            for msg in tx.tick(5, 7, 99, &ids, &values, &[], &[], 0.1) {
                assert!(msg.len() <= ROOM);
                got.extend(decode(&msg, 5).unwrap().floats);
            }
        }
        for (&id, &value) in ids.iter().zip(&values) {
            assert_eq!(got.get(&id), Some(&value), "variable {id} never recovered");
        }
    }

    #[test]
    fn long_utf8_strings_recover_lost_fragments_and_never_apply_partial_values() {
        let strings = vec!["árvíztűrő tükörfúrógép 🚍".repeat(200)];
        let mut tx = VarSender::default();
        let mut rx = VarReceiver::default();
        let mut got = None;
        for tick in 0..100 {
            let mut packets = tx.tick(8, 7, 99, &[], &[], &[5], &strings, 0.1);
            packets.reverse();
            for packet in packets {
                assert!(packet.len() <= ROOM);
                if tick == 1 { continue; } // lose one chunk in the first snapshot
                for (_, value) in rx.receive(&packet, 8).unwrap().strings {
                    assert_eq!(value, strings[0]);
                    got = Some(value);
                }
            }
            if tick < 9 { assert!(got.is_none(), "partial snapshot became visible"); }
        }
        assert_eq!(got, Some(strings[0].clone()));
        // A game may have consumed the first complete value before its remote bus exists.
        // Repeated key frames must expose the value again, even after network assembly.
        let mut repeated = false;
        for _ in 0..30 {
            for packet in tx.tick(8, 7, 99, &[], &[], &[5], &strings, 0.1) {
                repeated |= !rx.receive(&packet, 8).unwrap().strings.is_empty();
            }
        }
        assert!(repeated);
    }

    #[test]
    fn delayed_fragments_cannot_overwrite_a_new_short_string() {
        let mut tx = VarSender::default();
        let mut rx = VarReceiver::default();
        let mut delayed = Vec::new();
        for _ in 0..10 {
            delayed.extend(tx.tick(8, 7, 99, &[], &[], &[0], &["ő".repeat(2000)], 0.1));
        }
        let short = vec!["new value".to_string()];
        let messages = tx.tick(8, 7, 99, &[], &[], &[0], &short, 0.1);
        assert!(messages.iter().any(|p| rx.receive(p, 8).unwrap().strings == vec![(0, short[0].clone())]));
        for packet in delayed {
            assert!(rx.receive(&packet, 8).unwrap().strings.is_empty());
        }
    }

    #[test]
    fn constantly_changing_long_strings_finish_and_other_fields_get_a_turn() {
        let mut tx = VarSender::default();
        let mut rx = VarReceiver::default();
        let mut seen = std::collections::HashSet::new();
        for tick in 0..100 {
            let strings = vec![format!("{tick}:{}", "x".repeat(6000)), "y".repeat(4000), format!("small {tick}")];
            for packet in tx.tick(8, 7, 99, &[], &[], &[0, 1, 2], &strings, 0.1) {
                for (id, value) in rx.receive(&packet, 8).unwrap().strings {
                    assert!(!value.is_empty());
                    seen.insert(id);
                }
            }
        }
        assert_eq!(seen.len(), 3);
    }

    #[test]
    fn recovery_ticks_wait_for_busy_long_string_transfers() {
        let mut tx = VarSender::default();
        let mut strings = vec!["x".repeat(CHUNK * 10), "unchanged but lost".repeat(200)];
        // Lose the complete initial state, then let the first field occupy every key tick.
        for _ in 0..100 { tx.tick(8, 7, 99, &[], &[], &[0, 1], &strings, 0.1); }
        let mut rx = VarReceiver::default();
        let mut recovered = false;
        for tick in 0..100 {
            strings[0] = format!("{tick:04}{}", "x".repeat(CHUNK * 10 - 4));
            for packet in tx.tick(8, 7, 99, &[], &[], &[0, 1], &strings, 0.1) {
                recovered |= rx.receive(&packet, 8).unwrap().strings.iter().any(|(id, s)| *id == 1 && *s == strings[1]);
            }
        }
        assert!(recovered, "a busy transfer must not discard recovery work");
    }

    #[test]
    fn small_numeric_changes_are_sent_without_waiting_for_key_frames() {
        let mut tx = VarSender::default();
        tx.tick(8, 7, 99, &[0], &[1.0], &[], &[], 0.1);
        let next = f32::from_bits(1.0f32.to_bits() + 1);
        let packets = tx.tick(8, 7, 99, &[0], &[next], &[], &[], 0.1);
        assert_eq!(decode(&packets[0], 8).unwrap().floats, vec![(0, next)]);
    }

    #[test]
    fn a_short_or_foreign_datagram_is_not_taken() {
        let mut tx = VarSender::default();
        let m = tx.tick(5, 1, 2, &[0, 1], &[1.0, 2.0], &[0], &["x".into()], 0.1);
        assert!(decode(&m[0], 4).is_none(), "another protocol");
        assert!(decode(&m[0][..m[0].len() - 1], 5).is_none(), "cut short");
        assert!(decode(b"STATE|1", 5).is_none());
    }
}
