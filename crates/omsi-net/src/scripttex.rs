//! Owner-authoritative RGBA script textures. Receivers subscribe only after loading the
//! same vehicle. An initialization contains a full image; subsequent transfers contain
//! changed pixel runs. Transactions are ordered, fragmented, acknowledged end to end,
//! and retried until committed (including a lost final acknowledgement).
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::Arc;

pub const MAGIC: u8 = 0xB6;
const HEADER: usize = 37;
const CHUNK: usize = 1200;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_BLOB: usize = MAX_BYTES * 2 + 64;
const MAX_TEXTURES: usize = 64;
const MAX_RETAINED: usize = 64 * 1024 * 1024;
const WINDOW: usize = 16;
const RETRY: f32 = 0.25;
const SUBSCRIBE: u8 = 0;
const DATA: u8 = 1;
const PART_ACK: u8 = 2;
const COMMIT: u8 = 3;
const RETRY_ALL: u8 = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub mipmaps: bool,
    pub rgba: Arc<Vec<u8>>,
}

impl Image {
    fn valid(&self) -> bool {
        image_len(self.width, self.height) == Some(self.rgba.len())
    }
}

fn image_len(w: u32, h: u32) -> Option<usize> {
    let n = (w as usize).checked_mul(h as usize)?.checked_mul(4)?;
    (w > 0 && h > 0 && n <= MAX_BYTES).then_some(n)
}

/// A complete, committed image ready for the renderer. The wire carried only its delta.
#[derive(Clone, Debug)]
pub struct Incoming {
    pub owner: u32,
    pub table: u32,
    pub cookie: u64,
    pub index: u16,
    pub image: Image,
}

fn encode_image(image: &Image, previous: Option<&Image>) -> Option<Vec<u8>> {
    if !image.valid() {
        return None;
    }
    // A size change creates a new surface and needs its own initialization.
    let previous = previous.filter(|p| (p.width, p.height) == (image.width, image.height));
    if previous == Some(image) {
        return None;
    }
    let mut raw = Vec::new();
    raw.push(u8::from(previous.is_none()));
    raw.extend(image.width.to_le_bytes());
    raw.extend(image.height.to_le_bytes());
    raw.push(u8::from(image.mipmaps));
    if let Some(p) = previous {
        let mut pixel = 0;
        while pixel < image.rgba.len() / 4 {
            if image.rgba[pixel * 4..pixel * 4 + 4] == p.rgba[pixel * 4..pixel * 4 + 4] {
                pixel += 1;
                continue;
            }
            let first = pixel;
            while pixel < image.rgba.len() / 4
                && image.rgba[pixel * 4..pixel * 4 + 4] != p.rgba[pixel * 4..pixel * 4 + 4]
            {
                pixel += 1;
            }
            raw.extend((first as u32).to_le_bytes());
            raw.extend(((pixel - first) as u32).to_le_bytes());
            raw.extend(&image.rgba[first * 4..pixel * 4]);
        }
    } else {
        raw.extend(image.rgba.as_slice());
    }
    let mut compressor = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    compressor.write_all(&raw).ok()?;
    let compressed = compressor.finish().ok()?;
    (compressed.len() <= MAX_BLOB).then_some(compressed)
}

fn encode_reference(image: &Image, key: [u8; 32]) -> Option<Vec<u8>> {
    let mut raw = vec![2];
    raw.extend(image.width.to_le_bytes());
    raw.extend(image.height.to_le_bytes());
    raw.push(u8::from(image.mipmaps));
    raw.extend(key);
    let mut compressor = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    compressor.write_all(&raw).ok()?;
    compressor.finish().ok()
}

#[cfg(test)]
fn decode_image(blob: &[u8], previous: Option<&Image>) -> Option<Image> {
    decode_cached_image(blob, previous, &BTreeMap::new())
}

fn decode_cached_image(
    blob: &[u8],
    previous: Option<&Image>,
    cache: &BTreeMap<[u8; 32], Image>,
) -> Option<Image> {
    let mut raw = Vec::new();
    flate2::read::ZlibDecoder::new(blob)
        .take((MAX_BLOB + 1) as u64)
        .read_to_end(&mut raw)
        .ok()?;
    if raw.len() < 10 || raw.len() > MAX_BLOB || raw[0] > 2 || raw[9] > 1 {
        return None;
    }
    let width = u32::from_le_bytes(raw[1..5].try_into().ok()?);
    let height = u32::from_le_bytes(raw[5..9].try_into().ok()?);
    let n = image_len(width, height)?;
    if raw[0] == 2 {
        let key: [u8; 32] = raw.get(10..)?.try_into().ok()?;
        let image = cache.get(&key)?;
        return ((image.width, image.height, image.mipmaps) == (width, height, raw[9] != 0))
            .then(|| image.clone());
    }
    let mut rgba;
    if raw[0] == 1 {
        if raw.len() != 10 + n {
            return None;
        }
        rgba = raw[10..].to_vec();
    } else {
        let previous = previous?;
        if (width, height) != (previous.width, previous.height) {
            return None;
        }
        rgba = previous.rgba.as_ref().clone();
        let mut at = 10;
        let mut end = 0;
        while at < raw.len() {
            let first = u32::from_le_bytes(raw.get(at..at + 4)?.try_into().ok()?) as usize;
            let count = u32::from_le_bytes(raw.get(at + 4..at + 8)?.try_into().ok()?) as usize;
            if count == 0 || first < end {
                return None;
            }
            end = first.checked_add(count)?;
            if end > n / 4 {
                return None;
            }
            let bytes = count.checked_mul(4)?;
            rgba[first * 4..end * 4].copy_from_slice(raw.get(at + 8..at + 8 + bytes)?);
            at += 8 + bytes;
        }
    }
    Some(Image {
        width,
        height,
        mipmaps: raw[9] != 0,
        rgba: Arc::new(rgba),
    })
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Address {
    pub from: u32,
    pub to: u32,
    table: u32,
    cookie: u64,
    index: u16,
    seq: u32,
    epoch: u64,
}

pub(crate) fn address(data: &[u8], protocol: u8) -> Option<Address> {
    address_with_magic(data, protocol, MAGIC)
}

pub(crate) fn address_with_magic(data: &[u8], protocol: u8, magic: u8) -> Option<Address> {
    if data.len() < HEADER || data[0] != magic || data[1] != protocol || data[2] > RETRY_ALL {
        return None;
    }
    Some(Address {
        from: u32::from_le_bytes(data[3..7].try_into().ok()?),
        to: u32::from_le_bytes(data[7..11].try_into().ok()?),
        table: u32::from_le_bytes(data[11..15].try_into().ok()?),
        cookie: u64::from_le_bytes(data[15..23].try_into().ok()?),
        index: u16::from_le_bytes(data[23..25].try_into().ok()?),
        seq: u32::from_le_bytes(data[25..29].try_into().ok()?),
        epoch: u64::from_le_bytes(data[29..37].try_into().ok()?),
    })
}

fn packet(protocol: u8, kind: u8, a: Address, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![MAGIC, protocol, kind];
    out.extend(a.from.to_le_bytes());
    out.extend(a.to.to_le_bytes());
    out.extend(a.table.to_le_bytes());
    out.extend(a.cookie.to_le_bytes());
    out.extend(a.index.to_le_bytes());
    out.extend(a.seq.to_le_bytes());
    out.extend(a.epoch.to_le_bytes());
    out.extend(payload);
    out
}

struct Transfer {
    seq: u32,
    index: u16,
    image: Image,
    blob: Vec<u8>,
    acked: Vec<bool>,
    sent: Vec<f32>,
    reference: bool,
}
struct Sender {
    epoch: u64,
    table: u32,
    cookie: u64,
    seq: u32,
    baseline: Vec<Option<Image>>,
    pending: Option<Transfer>,
    cursor: usize,
    requested: f32,
}
struct Assembly {
    seq: u32,
    index: u16,
    len: usize,
    chunks: Vec<Option<Vec<u8>>>,
}
struct Receiver {
    reference_wait: Option<(u32, f32)>,
    epoch: Option<u64>,
    table: u32,
    cookie: u64,
    seq: u32,
    images: BTreeMap<u16, Image>,
    pending: Option<Assembly>,
    requested: f32,
}

pub struct Sync {
    references: Vec<Option<[u8; 32]>>,
    reusable: BTreeMap<[u8; 32], Image>,
    packet_budget: usize,
    local: Option<(u32, Vec<Option<Image>>)>,
    senders: BTreeMap<u32, Sender>,
    receivers: BTreeMap<u32, Receiver>,
    incoming: BTreeMap<(u32, u16), Incoming>,
    clock: f32,
    acc: f32,
    cookie: u64,
    epoch: u64,
    peer_cursor: usize,
}

impl Default for Sync {
    fn default() -> Self {
        // Subscription cookies survive neither process nor session restarts. Stream
        // generations increase within an owner, so delayed packets cannot reset a copy.
        let seed = crate::random_session_id() & (u64::MAX >> 1);
        Self {
            references: Vec::new(),
            reusable: BTreeMap::new(),
            packet_budget: 32,
            local: None,
            senders: BTreeMap::new(),
            receivers: BTreeMap::new(),
            incoming: BTreeMap::new(),
            clock: 0.0,
            acc: 0.0,
            cookie: seed,
            epoch: seed,
            peer_cursor: 0,
        }
    }
}

impl Sync {
    /// Optional content references for freetex only. The caller computes identities and
    /// supplies trusted local images off the game thread. A cache miss retries as pixels.
    pub fn set_references(
        &mut self,
        references: Vec<Option<[u8; 32]>>,
        reusable: BTreeMap<[u8; 32], Image>,
    ) {
        self.references = references.into_iter().take(MAX_TEXTURES).collect();
        self.reusable.clear();
        let mut bytes = 0;
        for (key, image) in reusable {
            if image.valid() && bytes + image.rgba.len() <= MAX_RETAINED {
                bytes += image.rgba.len();
                self.reusable.insert(key, image);
            }
        }
    }
    /// Aggregate data-packet budget per 50 ms, shared fairly by all subscribers.
    pub fn with_packet_budget(packets: usize) -> Self {
        Self {
            packet_budget: packets.clamp(1, 32),
            ..Self::default()
        }
    }
    pub fn set_local(&mut self, table: u32, images: Vec<Option<Image>>) {
        if images.len() > MAX_TEXTURES
            || images.iter().flatten().any(|i| !i.valid())
            || images.iter().flatten().map(|i| i.rgba.len()).sum::<usize>() > MAX_RETAINED
        {
            self.clear_local();
            log::warn!(
                "LAN: script texture dimensions or total storage exceed the transport limits"
            );
            return;
        }
        if self.local.as_ref().map(|x| (x.0, x.1.len())) != Some((table, images.len())) {
            self.senders.clear();
        }
        self.local = Some((table, images));
    }

    pub fn clear_local(&mut self) {
        self.local = None;
        self.senders.clear();
    }

    /// Start a fresh initialization for a newly constructed copy of a matching vehicle.
    pub fn subscribe(&mut self, owner: u32, table: u32) -> u64 {
        self.cookie = self
            .cookie
            .checked_add(1)
            .expect("texture subscription counter exhausted");
        let cookie = self.cookie;
        self.receivers.insert(
            owner,
            Receiver {
                reference_wait: None,
                epoch: None,
                table,
                cookie,
                seq: 0,
                images: BTreeMap::new(),
                pending: None,
                requested: f32::NEG_INFINITY,
            },
        );
        self.incoming.retain(|(id, _), _| *id != owner);
        cookie
    }

    pub fn retain_peers(&mut self, peers: &[u32]) {
        self.senders.retain(|id, _| peers.contains(id));
        self.receivers.retain(|id, _| peers.contains(id));
        self.incoming.retain(|(id, _), _| peers.contains(id));
    }

    pub fn unsubscribe(&mut self, owner: u32) {
        self.receivers.remove(&owner);
        self.incoming.retain(|(id, _), _| *id != owner);
    }

    pub fn take_incoming(&mut self) -> Vec<Incoming> {
        std::mem::take(&mut self.incoming).into_values().collect()
    }

    pub fn tick(&mut self, protocol: u8, id: u32, dt: f32) -> Vec<Vec<u8>> {
        self.clock += dt.clamp(0.0, 0.5);
        self.acc += dt.clamp(0.0, 0.5);
        if self.acc < 0.05 {
            return Vec::new();
        }
        self.acc = 0.0;
        let mut out = Vec::new();
        self.senders
            .retain(|_, tx| self.clock - tx.requested <= 5.0);
        for (&owner, rx) in &mut self.receivers {
            // Keep the subscription alive; a receiver reconnecting to a restarted owner
            // must also receive a new initialization rather than wait for old deltas.
            if self.clock - rx.requested >= 1.0 {
                rx.requested = self.clock;
                out.push(packet(
                    protocol,
                    SUBSCRIBE,
                    Address {
                        from: id,
                        to: owner,
                        table: rx.table,
                        cookie: rx.cookie,
                        index: 0,
                        seq: rx.seq,
                        epoch: rx.epoch.unwrap_or(0),
                    },
                    &[],
                ));
            }
        }
        let Some((table, images)) = &self.local else {
            return out;
        };
        let peers: Vec<u32> = self.senders.keys().copied().collect();
        let mut budget = self.packet_budget;
        let mut retained = self
            .senders
            .values()
            .filter_map(|tx| tx.pending.as_ref())
            .map(|p| p.blob.len())
            .sum::<usize>();
        for offset in 0..peers.len() {
            let peer = peers[(self.peer_cursor + offset) % peers.len()];
            let tx = self.senders.get_mut(&peer).unwrap();
            if tx.table != *table {
                continue;
            }
            if tx.pending.is_none() && !images.is_empty() {
                for _ in 0..images.len() {
                    let index = tx.cursor % images.len();
                    tx.cursor = (index + 1) % images.len();
                    let Some(image) = &images[index] else {
                        continue;
                    };
                    if tx.baseline[index].as_ref() == Some(image) {
                        continue;
                    }
                    let reference = self.references.get(index).copied().flatten();
                    let Some(blob) = reference
                        .and_then(|key| encode_reference(image, key))
                        .or_else(|| encode_image(image, tx.baseline[index].as_ref()))
                    else {
                        continue;
                    };
                    if retained + blob.len() > MAX_RETAINED {
                        continue;
                    }
                    retained += blob.len();
                    let Some(seq) = tx.seq.checked_add(1) else {
                        break;
                    };
                    tx.seq = seq;
                    let chunks = blob.len().div_ceil(CHUNK);
                    tx.pending = Some(Transfer {
                        seq: tx.seq,
                        index: index as u16,
                        image: image.clone(),
                        blob,
                        acked: vec![false; chunks],
                        sent: vec![f32::NEG_INFINITY; chunks],
                        reference: reference.is_some(),
                    });
                    break;
                }
            }
            let Some(p) = &mut tx.pending else { continue };
            let mut in_flight = p
                .sent
                .iter()
                .zip(&p.acked)
                .filter(|(at, ack)| !**ack && self.clock - **at < RETRY)
                .count();
            let committed = p.acked.iter().all(|x| *x);
            for part in 0..p.acked.len() {
                if budget == 0 {
                    break;
                }
                // If the final commit ACK was lost, retransmit one fragment to elicit it.
                if (p.acked[part] && !(committed && part == p.acked.len() - 1))
                    || self.clock - p.sent[part] < RETRY
                {
                    continue;
                }
                if !committed && in_flight >= WINDOW {
                    break;
                }
                let mut payload = Vec::new();
                payload.extend((part as u16).to_le_bytes());
                payload.extend((p.acked.len() as u16).to_le_bytes());
                payload.extend((p.blob.len() as u32).to_le_bytes());
                payload.extend(&p.blob[part * CHUNK..((part + 1) * CHUNK).min(p.blob.len())]);
                out.push(packet(
                    protocol,
                    DATA,
                    Address {
                        from: id,
                        to: peer,
                        table: tx.table,
                        cookie: tx.cookie,
                        index: p.index,
                        seq: p.seq,
                        epoch: tx.epoch,
                    },
                    &payload,
                ));
                p.sent[part] = self.clock;
                in_flight += 1;
                budget -= 1;
            }
        }
        if !peers.is_empty() {
            self.peer_cursor = (self.peer_cursor + 1) % peers.len();
        }
        out
    }

    pub fn receive(&mut self, protocol: u8, id: u32, data: &[u8]) -> Vec<Vec<u8>> {
        let Some(a) = address(data, protocol) else {
            return Vec::new();
        };
        if a.to != id || a.from == id || a.from == 0 {
            return Vec::new();
        }
        let reply = Address {
            from: id,
            to: a.from,
            ..a
        };
        match data[2] {
            SUBSCRIBE => {
                if data.len() != HEADER {
                    return Vec::new();
                }
                let Some((table, images)) = &self.local else {
                    return Vec::new();
                };
                if *table != a.table {
                    return Vec::new();
                }
                if self
                    .senders
                    .get(&a.from)
                    .is_none_or(|tx| (tx.table, tx.cookie) != (a.table, a.cookie))
                {
                    self.epoch += 1;
                    self.senders.insert(
                        a.from,
                        Sender {
                            epoch: self.epoch,
                            table: a.table,
                            cookie: a.cookie,
                            seq: 0,
                            baseline: vec![None; images.len()],
                            pending: None,
                            cursor: 0,
                            requested: self.clock,
                        },
                    );
                }
                self.senders.get_mut(&a.from).unwrap().requested = self.clock;
            }
            PART_ACK | COMMIT | RETRY_ALL => {
                let retained = self
                    .senders
                    .values()
                    .filter_map(|tx| tx.pending.as_ref())
                    .map(|p| p.blob.len())
                    .sum::<usize>();
                let Some(tx) = self.senders.get_mut(&a.from) else {
                    return Vec::new();
                };
                if (tx.table, tx.cookie, tx.epoch) != (a.table, a.cookie, a.epoch) {
                    return Vec::new();
                }
                let Some(p) = &mut tx.pending else {
                    return Vec::new();
                };
                if (p.seq, p.index) != (a.seq, a.index) {
                    return Vec::new();
                }
                if data[2] == RETRY_ALL && data.len() == HEADER {
                    if p.reference {
                        let Some(blob) =
                            encode_image(&p.image, tx.baseline[p.index as usize].as_ref())
                        else {
                            return Vec::new();
                        };
                        if retained - p.blob.len() + blob.len() > MAX_RETAINED {
                            return Vec::new();
                        }
                        let chunks = blob.len().div_ceil(CHUNK);
                        p.blob = blob;
                        p.acked = vec![false; chunks];
                        p.sent = vec![f32::NEG_INFINITY; chunks];
                        p.reference = false;
                    }
                    p.acked.fill(false);
                    p.sent.fill(f32::NEG_INFINITY);
                } else if data[2] == COMMIT && data.len() == HEADER {
                    tx.baseline[p.index as usize] = Some(p.image.clone());
                    tx.pending = None;
                } else if data[2] == PART_ACK && data.len() == HEADER + 2 {
                    let part = u16::from_le_bytes(data[HEADER..].try_into().unwrap()) as usize;
                    if let Some(ack) = p.acked.get_mut(part) {
                        *ack = true;
                    }
                }
            }
            DATA => return self.receive_data(protocol, data, a, reply),
            _ => {}
        }
        Vec::new()
    }

    fn receive_data(
        &mut self,
        protocol: u8,
        data: &[u8],
        a: Address,
        reply: Address,
    ) -> Vec<Vec<u8>> {
        let reserved = self
            .receivers
            .values()
            .filter_map(|rx| rx.pending.as_ref())
            .map(|p| p.len)
            .sum::<usize>();
        let cached = self
            .receivers
            .values()
            .flat_map(|rx| rx.images.values())
            .map(|i| i.rgba.len())
            .sum::<usize>();
        let Some(rx) = self.receivers.get_mut(&a.from) else {
            return Vec::new();
        };
        if (rx.table, rx.cookie) != (a.table, a.cookie) || a.index as usize >= MAX_TEXTURES {
            return Vec::new();
        }
        if data.len() < HEADER + 8 {
            return Vec::new();
        }
        let part = u16::from_le_bytes(data[HEADER..HEADER + 2].try_into().unwrap()) as usize;
        let count = u16::from_le_bytes(data[HEADER + 2..HEADER + 4].try_into().unwrap()) as usize;
        let len = u32::from_le_bytes(data[HEADER + 4..HEADER + 8].try_into().unwrap()) as usize;
        if len == 0
            || len > MAX_BLOB
            || count != len.div_ceil(CHUNK)
            || part >= count
            || data.len() != HEADER + 8 + (len - part * CHUNK).min(CHUNK)
        {
            return Vec::new();
        }
        if rx.epoch != Some(a.epoch) {
            if a.seq != 1 || rx.epoch.is_some_and(|epoch| a.epoch <= epoch) {
                return Vec::new();
            }
            rx.epoch = Some(a.epoch);
            rx.seq = 0;
            rx.images.clear();
            rx.pending = None;
        }
        if a.seq <= rx.seq && a.seq > 0 {
            return vec![packet(protocol, COMMIT, reply, &[])];
        }
        if Some(a.seq) != rx.seq.checked_add(1) {
            return Vec::new();
        }
        if rx.pending.is_none() && reserved + len > MAX_RETAINED {
            return Vec::new();
        }
        let pending = rx.pending.get_or_insert_with(|| Assembly {
            seq: a.seq,
            index: a.index,
            len,
            chunks: vec![None; count],
        });
        if (pending.seq, pending.index, pending.len) != (a.seq, a.index, len) {
            return Vec::new();
        }
        pending.chunks[part] = Some(data[HEADER + 8..].to_vec());
        let mut replies = vec![packet(
            protocol,
            PART_ACK,
            reply,
            &(part as u16).to_le_bytes(),
        )];
        if pending.chunks.iter().all(Option::is_some) {
            let mut blob = Vec::with_capacity(len);
            for chunk in &pending.chunks {
                blob.extend(chunk.as_ref().unwrap());
            }
            let Some(image) = decode_cached_image(&blob, rx.images.get(&a.index), &self.reusable)
            else {
                rx.pending = None;
                // Variable/name packets can arrive after the content reference.
                // Allow the background local lookup to catch up before requesting
                // pixels, rather than uploading an already installed picture.
                let mut kind = [0u8];
                if flate2::read::ZlibDecoder::new(blob.as_slice())
                    .read_exact(&mut kind)
                    .is_ok()
                    && kind[0] == 2
                {
                    let wait = rx.reference_wait.get_or_insert((a.seq, self.clock));
                    if wait.0 != a.seq {
                        *wait = (a.seq, self.clock);
                    }
                    if self.clock - wait.1 < 0.5 {
                        return Vec::new();
                    }
                }
                return vec![packet(protocol, RETRY_ALL, reply, &[])];
            };
            let previous_len = rx.images.get(&a.index).map_or(0, |i| i.rgba.len());
            if cached - previous_len + image.rgba.len() > MAX_RETAINED {
                return Vec::new();
            }
            rx.images.insert(a.index, image.clone());
            rx.seq = a.seq;
            rx.reference_wait = None;
            rx.pending = None;
            self.incoming.insert(
                (a.from, a.index),
                Incoming {
                    owner: a.from,
                    table: a.table,
                    cookie: a.cookie,
                    index: a.index,
                    image,
                },
            );
            replies.push(packet(protocol, COMMIT, reply, &[]));
        }
        return replies;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(w: u32, h: u32) -> Image {
        // Incompressible enough to exercise fragmentation and all four RGBA channels.
        let mut seed = 99u32;
        let rgba = (0..w * h * 4)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as u8
            })
            .collect();
        Image {
            width: w,
            height: h,
            mipmaps: false,
            rgba: Arc::new(rgba),
        }
    }

    fn exchange(owner: &mut Sync, reader: &mut Sync, rounds: usize, lossy: bool) {
        let mut lost_commit = false;
        let mut packet_number = 0;
        for _ in 0..rounds {
            for request in reader.tick(5, 2, 0.05) {
                owner.receive(5, 1, &request);
            }
            let mut packets = owner.tick(5, 1, 0.05);
            packets.reverse();
            for packet in packets {
                assert!(packet.len() <= crate::MAX_DATAGRAM);
                packet_number += 1;
                if lossy && packet_number % 5 == 0 {
                    continue;
                }
                for _ in 0..if lossy { 2 } else { 1 } {
                    for ack in reader.receive(5, 2, &packet) {
                        if lossy && ack[2] == COMMIT && !lost_commit {
                            lost_commit = true;
                            continue;
                        }
                        if lossy && ack[2] == PART_ACK && packet_number % 7 == 0 {
                            continue;
                        }
                        owner.receive(5, 1, &ack);
                    }
                }
            }
        }
    }

    #[test]
    fn rgba_initialization_and_sparse_alpha_changes_survive_loss_reordering_and_duplicates() {
        let original = image(128, 64);
        let mut owner = Sync::default();
        let mut reader = Sync::default();
        owner.set_local(77, vec![Some(original.clone())]);
        let cookie = reader.subscribe(1, 77);
        exchange(&mut owner, &mut reader, 100, true);
        let received = reader.take_incoming();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].cookie, cookie);
        assert_eq!(received[0].image, original);
        let mut changed = original.clone();
        Arc::make_mut(&mut changed.rgba)[3] ^= 255;
        let n = changed.rgba.len();
        Arc::make_mut(&mut changed.rgba)[n - 4..].copy_from_slice(&[12, 34, 56, 78]);
        changed.mipmaps = true;
        let delta = encode_image(&changed, Some(&original)).unwrap();
        let mut raw = Vec::new();
        flate2::read::ZlibDecoder::new(delta.as_slice())
            .read_to_end(&mut raw)
            .unwrap();
        assert_eq!(raw[0], 0, "an update is never a full-image packet");
        assert_eq!(raw.len(), 10 + 2 * (8 + 4), "only two changed RGBA pixels");
        owner.set_local(77, vec![Some(changed.clone())]);
        exchange(&mut owner, &mut reader, 100, true);
        assert_eq!(reader.take_incoming()[0].image, changed);
        assert!(
            owner.senders[&2].pending.is_none(),
            "a lost commit acknowledgement is recovered"
        );
        exchange(&mut owner, &mut reader, 20, false);
        assert!(
            reader.take_incoming().is_empty(),
            "unchanged images are not sent again"
        );
    }

    #[test]
    fn fragments_are_atomic_and_a_lost_commit_ack_is_retried() {
        let original = image(64, 64);
        let mut owner = Sync::default();
        let mut reader = Sync::default();
        owner.set_local(7, vec![Some(original.clone())]);
        reader.subscribe(1, 7);
        for request in reader.tick(5, 2, 0.05) {
            owner.receive(5, 1, &request);
        }
        let packets = owner.tick(5, 1, 0.05);
        assert!(packets.len() > 1);
        for packet in &packets[1..] {
            reader.receive(5, 2, packet);
        }
        assert!(
            reader.take_incoming().is_empty(),
            "partial images cannot reach the renderer"
        );
        for packet in &packets {
            for ack in reader.receive(5, 2, packet) {
                if ack[2] == PART_ACK {
                    owner.receive(5, 1, &ack);
                }
            }
        }
        assert_eq!(reader.take_incoming()[0].image, original);
        assert!(owner.senders[&2].pending.is_some());
        exchange(&mut owner, &mut reader, 20, false);
        assert!(owner.senders[&2].pending.is_none());
        assert!(
            reader.take_incoming().is_empty(),
            "a duplicate commit does not redraw"
        );
    }

    #[test]
    fn new_changes_wait_for_the_acknowledged_baseline_and_late_joiners_get_initialization() {
        let original = image(64, 64);
        let mut owner = Sync::default();
        let mut reader = Sync::default();
        owner.set_local(7, vec![Some(original.clone())]);
        reader.subscribe(1, 7);
        for request in reader.tick(5, 2, 0.05) {
            owner.receive(5, 1, &request);
        }
        owner.tick(5, 1, 0.05); // initialization in flight, all packets lost
        let mut changed = original.clone();
        Arc::make_mut(&mut changed.rgba)[4..8].copy_from_slice(&[9, 8, 7, 6]);
        owner.set_local(7, vec![Some(changed.clone())]);
        exchange(&mut owner, &mut reader, 100, true);
        assert_eq!(reader.take_incoming()[0].image, changed);
        let cookie = reader.subscribe(1, 7); // same bus reconstructed, old stream discarded
        exchange(&mut owner, &mut reader, 100, true);
        let received = reader.take_incoming();
        assert_eq!(received[0].cookie, cookie);
        assert_eq!(received[0].image, changed);
        // An owner changing bus or restarting its surfaces initializes from the receiver's
        // last acknowledged sequence; its old sequence must not block the new stream.
        owner.clear_local();
        owner.set_local(7, vec![Some(original.clone())]);
        exchange(&mut owner, &mut reader, 100, true);
        assert_eq!(reader.take_incoming()[0].image, original);
    }

    #[test]
    fn a_reinitialized_surface_rejects_old_stream_data_and_acknowledgements() {
        let original = image(4, 4);
        let mut owner = Sync::default();
        let mut reader = Sync::default();
        owner.set_local(7, vec![Some(original.clone())]);
        reader.subscribe(1, 7);
        let stale_request = reader.tick(5, 2, 0.05).remove(0);
        owner.receive(5, 1, &stale_request);
        let old_packet = owner.tick(5, 1, 0.05).remove(0);
        let old_address = address(&old_packet, 5).unwrap();
        exchange(&mut owner, &mut reader, 20, false);
        assert_eq!(reader.take_incoming()[0].image, original);
        let mut changed = original.clone();
        Arc::make_mut(&mut changed.rgba)[3] ^= 255;
        owner.clear_local();
        owner.set_local(7, vec![Some(changed.clone())]);
        // This subscribe predates the last commit, so its sequence is out of date.
        owner.receive(5, 1, &stale_request);
        owner.tick(5, 1, 0.05);
        let old_ack = packet(
            5,
            COMMIT,
            Address {
                from: 2,
                to: 1,
                ..old_address
            },
            &[],
        );
        owner.receive(5, 1, &old_ack);
        assert!(
            owner.senders[&2].pending.is_some(),
            "an old ACK cannot commit the new initialization"
        );
        exchange(&mut owner, &mut reader, 20, false);
        assert_eq!(reader.take_incoming()[0].image, changed);
        assert!(reader.receive(5, 2, &old_packet).is_empty());
        assert!(
            reader.take_incoming().is_empty(),
            "old pixels cannot overwrite the new surface"
        );
    }

    #[test]
    fn a_corrupt_fragment_retries_the_transaction_without_losing_its_acknowledged_parts() {
        let original = image(64, 64);
        let mut owner = Sync::default();
        let mut reader = Sync::default();
        owner.set_local(7, vec![Some(original.clone())]);
        reader.subscribe(1, 7);
        for request in reader.tick(5, 2, 0.05) {
            owner.receive(5, 1, &request);
        }
        let packets = owner.tick(5, 1, 0.05);
        for (index, mut packet) in packets.into_iter().enumerate() {
            if index == 0 {
                let last = packet.len() - 1;
                packet[last] ^= 1;
            }
            for ack in reader.receive(5, 2, &packet) {
                owner.receive(5, 1, &ack);
            }
        }
        assert!(reader.take_incoming().is_empty());
        exchange(&mut owner, &mut reader, 40, false);
        assert_eq!(reader.take_incoming()[0].image, original);
        reader.unsubscribe(1);
        for _ in 0..12 {
            owner.tick(5, 1, 0.5);
        }
        assert!(
            owner.senders.is_empty(),
            "unsubscribed vehicles release their queued images"
        );
    }

    #[test]
    fn a_foreign_table_or_subscription_and_invalid_pixels_are_not_applied() {
        let mut owner = Sync::default();
        let mut reader = Sync::default();
        owner.set_local(7, vec![Some(image(4, 4))]);
        reader.subscribe(1, 8);
        exchange(&mut owner, &mut reader, 20, false);
        assert!(owner.senders.is_empty());
        assert!(reader.take_incoming().is_empty());
        reader.subscribe(1, 7);
        for request in reader.tick(5, 2, 1.0) {
            owner.receive(5, 1, &request);
        }
        let old = owner.tick(5, 1, 0.1);
        reader.subscribe(1, 7);
        for packet in old {
            assert!(reader.receive(5, 2, &packet).is_empty());
        }
        assert!(reader.take_incoming().is_empty());
        let invalid = Image {
            width: u32::MAX,
            height: u32::MAX,
            mipmaps: false,
            rgba: Arc::new(vec![]),
        };
        assert!(encode_image(&invalid, None).is_none());
        assert!(decode_image(&[0; 32], None).is_none());
    }
}
