//! Reliable, ordered, owner-authored sound events. Only names and paths, never audio.
use std::collections::{BTreeMap, HashMap, VecDeque};

pub const MAGIC: u8 = 0xB9;
const HEAD: usize = 36;
const CHUNK: usize = 1024;
const MAX_SIZE: usize = 64 * 1024;
const WINDOW: u64 = 64;

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    /// Zero denotes the owner's player bus; otherwise the host's AI world vehicle ID.
    pub ai: u64,
    /// Vehicle path prevents delayed events being played on a replacement bus.
    pub bus: String,
    pub trigger: String,
    pub file: Option<String>,
    /// Numeric sound curve values at T.L execution, mapped by name across bus variants.
    pub values: Vec<(String, f32)>,
}
#[derive(Debug)]
pub struct Incoming {
    pub owner: u32,
    pub event: Event,
}
#[derive(Clone, Copy)]
pub(crate) struct Address {
    pub from: u32,
    pub to: u32,
}
struct Packet<'a> {
    addr: Address,
    ack: bool,
    epoch: u64,
    seq: u64,
    part: usize,
    count: usize,
    size: usize,
    data: &'a [u8],
}
fn packet(data: &[u8], proto: u8) -> Option<Packet<'_>> {
    if data.len() < HEAD || data[0] != MAGIC || data[1] != proto || data[2] > 1 || data[3] != 0 {
        return None;
    }
    let p = Packet {
        addr: Address {
            from: u32::from_le_bytes(data[4..8].try_into().ok()?),
            to: u32::from_le_bytes(data[8..12].try_into().ok()?),
        },
        ack: data[2] == 1,
        epoch: u64::from_le_bytes(data[12..20].try_into().ok()?),
        seq: u64::from_le_bytes(data[20..28].try_into().ok()?),
        part: u16::from_le_bytes(data[28..30].try_into().ok()?) as usize,
        count: u16::from_le_bytes(data[30..32].try_into().ok()?) as usize,
        size: u32::from_le_bytes(data[32..36].try_into().ok()?) as usize,
        data: &data[HEAD..],
    };
    if p.seq == 0 || p.epoch == 0 || p.addr.from == 0 || p.addr.to == 0 {
        return None;
    }
    if p.ack {
        if data.len() != HEAD || p.part != 0 || p.count != 0 || p.size != 0 {
            return None;
        }
    } else if p.size == 0
        || p.size > MAX_SIZE
        || p.count != p.size.div_ceil(CHUNK)
        || p.part >= p.count
        || p.data.len() != (p.size - p.part * CHUNK).min(CHUNK)
    {
        return None;
    }
    Some(p)
}
/// Is `data` an acknowledgement (a header and nothing else)?
pub(crate) fn is_ack(data: &[u8], proto: u8) -> bool {
    packet(data, proto).is_some_and(|p| p.ack)
}
pub(crate) fn address(data: &[u8], proto: u8) -> Option<Address> {
    Some(packet(data, proto)?.addr)
}
fn header(proto: u8, from: u32, to: u32, epoch: u64, seq: u64, ack: bool) -> Vec<u8> {
    let mut v = vec![MAGIC, proto, ack as u8, 0];
    v.extend(from.to_le_bytes());
    v.extend(to.to_le_bytes());
    v.extend(epoch.to_le_bytes());
    v.extend(seq.to_le_bytes());
    v.resize(HEAD, 0);
    v
}
fn encode(e: &Event) -> Option<Vec<u8>> {
    if e.values.iter().any(|(_, v)| !v.is_finite()) {
        return None;
    }
    let mut v = e.ai.to_le_bytes().to_vec();
    v.push(e.file.is_some() as u8);
    for s in [&e.bus, &e.trigger, e.file.as_deref().unwrap_or("")] {
        let n = u16::try_from(s.len()).ok()?;
        v.extend(n.to_le_bytes());
        v.extend(s.as_bytes());
    }
    v.extend(u16::try_from(e.values.len()).ok()?.to_le_bytes());
    for (name, value) in &e.values {
        v.extend(u16::try_from(name.len()).ok()?.to_le_bytes());
        v.extend(name.as_bytes());
        v.extend(value.to_le_bytes());
    }
    (v.len() <= MAX_SIZE && !e.bus.is_empty() && !e.trigger.is_empty()).then_some(v)
}
fn decode(v: &[u8]) -> Option<Event> {
    let ai = u64::from_le_bytes(v.get(..8)?.try_into().ok()?);
    let has_file = *v.get(8)?;
    if has_file > 1 {
        return None;
    }
    let mut pos = 9;
    let mut read = || {
        let len = u16::from_le_bytes(v.get(pos..pos + 2)?.try_into().ok()?) as usize;
        pos += 2;
        let s = std::str::from_utf8(v.get(pos..pos + len)?).ok()?.to_owned();
        pos += len;
        Some(s)
    };
    let bus = read()?;
    let trigger = read()?;
    let file = read()?;
    let count = u16::from_le_bytes(v.get(pos..pos + 2)?.try_into().ok()?) as usize;
    pos += 2;
    let mut values = Vec::new();
    for _ in 0..count {
        let len = u16::from_le_bytes(v.get(pos..pos + 2)?.try_into().ok()?) as usize;
        pos += 2;
        let name = std::str::from_utf8(v.get(pos..pos + len)?).ok()?.to_owned();
        pos += len;
        let value = f32::from_le_bytes(v.get(pos..pos + 4)?.try_into().ok()?);
        pos += 4;
        if !value.is_finite() {
            return None;
        }
        values.push((name, value));
    }
    if pos != v.len() || bus.is_empty() || trigger.is_empty() || (has_file == 0 && !file.is_empty())
    {
        return None;
    }
    Some(Event {
        ai,
        bus,
        trigger,
        file: (has_file == 1).then_some(file),
        values,
    })
}
struct Tx {
    seq: u64,
    data: Vec<u8>,
    wait: f32,
    /// Times sent: each resend waits twice as long as the last (`RESEND`), so a slow or
    /// throttled link is not answered with ever more of the same packets.
    tries: u32,
}
/// The first resend of an unacknowledged event, and the longest wait between resends. A
/// fixed 0.1 s through a tunnel of a few hundred ms sent every event several times over to
/// every player; past the host's sound rate those were dropped, acknowledgements with them,
/// and every drop made more resends.
const RESEND: (f32, f32) = (0.1, 2.0);
#[derive(Default)]
struct Sender {
    next: u64,
    queue: VecDeque<Tx>,
    full: bool,
}
struct Assembly {
    size: usize,
    parts: Vec<Option<Vec<u8>>>,
}
struct Receiver {
    epoch: u64,
    next: u64,
    pending: BTreeMap<u64, Assembly>,
}
pub(crate) struct Sync {
    epoch: u64,
    tx: HashMap<u32, Sender>,
    rx: HashMap<u32, Receiver>,
    incoming: Vec<Incoming>,
}
impl Default for Sync {
    fn default() -> Self {
        Self {
            epoch: crate::random_session_id().max(1),
            tx: HashMap::new(),
            rx: HashMap::new(),
            incoming: Vec::new(),
        }
    }
}
impl Sync {
    pub fn publish(&mut self, peers: &[u32], event: Event) {
        let Some(data) = encode(&event) else {
            log::warn!("LAN sound event exceeds protocol limits");
            return;
        };
        for &peer in peers {
            let s = self.tx.entry(peer).or_default();
            if s.queue.len() >= 512 {
                if !std::mem::replace(&mut s.full, true) {
                    log::warn!("LAN sound events awaiting acknowledgement from {peer}: queue full");
                }
                continue;
            }
            s.full &= s.queue.len() >= 256;
            s.next += 1;
            s.queue.push_back(Tx {
                seq: s.next,
                data: data.clone(),
                wait: 0.0,
                tries: 0,
            });
        }
    }
    pub fn retain_peers(&mut self, peers: &[u32]) {
        self.tx.retain(|id, _| peers.contains(id));
        self.rx.retain(|id, _| peers.contains(id));
        self.incoming.retain(|s| peers.contains(&s.owner));
    }
    pub fn tick(&mut self, proto: u8, me: u32, dt: f32) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for (&peer, s) in &mut self.tx {
            let mut budget = 128;
            let first = s.queue.front().map(|t| t.seq).unwrap_or(0);
            for t in &mut s.queue {
                if t.seq >= first + WINDOW || budget < t.data.len().div_ceil(CHUNK) {
                    break;
                }
                t.wait -= dt.max(0.0);
                if t.wait > 0.0 {
                    continue;
                }
                t.wait = (RESEND.0 * 2f32.powi(t.tries.min(8) as i32)).min(RESEND.1);
                t.tries += 1;
                budget -= t.data.len().div_ceil(CHUNK);
                for (part, data) in t.data.chunks(CHUNK).enumerate() {
                    let mut v = header(proto, me, peer, self.epoch, t.seq, false);
                    v[28..30].copy_from_slice(&(part as u16).to_le_bytes());
                    v[30..32].copy_from_slice(&(t.data.len().div_ceil(CHUNK) as u16).to_le_bytes());
                    v[32..36].copy_from_slice(&(t.data.len() as u32).to_le_bytes());
                    v.extend(data);
                    out.push(v);
                }
            }
        }
        out
    }
    pub fn receive(&mut self, proto: u8, me: u32, data: &[u8]) -> Option<Vec<u8>> {
        let p = packet(data, proto)?;
        if p.addr.to != me || p.addr.from == me {
            return None;
        }
        if p.ack {
            if p.epoch == self.epoch {
                if let Some(s) = self.tx.get_mut(&p.addr.from) {
                    s.queue.retain(|t| t.seq != p.seq);
                }
            }
            return None;
        }
        let r = self.rx.entry(p.addr.from).or_insert_with(|| Receiver {
            epoch: p.epoch,
            next: 1,
            pending: BTreeMap::new(),
        });
        // A peer's stream lives for the session. Old epochs cannot reset deduplication.
        if r.epoch != p.epoch || p.seq >= r.next + WINDOW {
            return None;
        }
        if p.seq >= r.next {
            if self.incoming.len() >= 1024 {
                return None;
            }
            let a = r.pending.entry(p.seq).or_insert_with(|| Assembly {
                size: p.size,
                parts: vec![None; p.count],
            });
            if a.size != p.size {
                return None;
            }
            a.parts[p.part].get_or_insert_with(|| p.data.to_vec());
            if a.parts.iter().any(Option::is_none) {
                return None;
            }
            let bytes: Vec<u8> = a
                .parts
                .iter()
                .flat_map(|v| v.as_ref().unwrap().iter().copied())
                .collect();
            decode(&bytes)?;
            drain_ready(p.addr.from, r, &mut self.incoming);
        }
        Some(header(proto, me, p.addr.from, p.epoch, p.seq, true))
    }
    pub fn take_incoming(&mut self) -> Vec<Incoming> {
        for (&owner, r) in &mut self.rx {
            drain_ready(owner, r, &mut self.incoming);
        }
        std::mem::take(&mut self.incoming)
    }
}

fn drain_ready(owner: u32, r: &mut Receiver, out: &mut Vec<Incoming>) {
    while out.len() < 1024 {
        let Some(a) = r.pending.get(&r.next) else {
            break;
        };
        if a.parts.iter().any(Option::is_none) {
            break;
        }
        let bytes: Vec<u8> = a
            .parts
            .iter()
            .flat_map(|v| v.as_ref().unwrap().iter().copied())
            .collect();
        let Some(event) = decode(&bytes) else {
            break;
        };
        out.push(Incoming { owner, event });
        r.pending.remove(&r.next);
        r.next += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(n: &str, file: Option<&str>) -> Event {
        Event {
            ai: 0,
            bus: "Vehicles/test.bus".into(),
            trigger: n.into(),
            file: file.map(str::to_owned),
            values: vec![("doorSpeed".into(), 0.42)],
        }
    }
    #[test]
    fn lost_reordered_fragments_and_acks_deliver_once_in_order() {
        let mut a = Sync::default();
        let mut b = Sync::default();
        let long = "ä/".repeat(1000);
        a.publish(&[2], event("door", None));
        a.publish(&[2], event("announce", Some(&long)));
        let packets = a.tick(11, 1, 0.0);
        // Lose first event and a fragment, reorder the remaining fragments.
        for v in packets.iter().skip(2).rev() {
            b.receive(11, 2, v);
        }
        assert!(b.take_incoming().is_empty());
        for v in a.tick(11, 1, 0.11).iter().rev() {
            b.receive(11, 2, v);
        }
        let received = b.take_incoming();
        assert_eq!(
            received.iter().map(|i| i.event.clone()).collect::<Vec<_>>(),
            vec![event("door", None), event("announce", Some(&long))]
        );
        // Lost ACK: retry is acknowledged again without replaying anything.
        for v in a.tick(11, 1, 0.21) {
            let ack = b.receive(11, 2, &v).unwrap();
            a.receive(11, 1, &ack);
        }
        assert!(b.take_incoming().is_empty());
        assert!(a.tick(11, 1, 1.0).is_empty());
    }
    #[test]
    fn an_unacknowledged_event_is_sent_again_ever_less_often() {
        let mut a = Sync::default();
        a.publish(&[2], event("door", None));
        assert_eq!(a.tick(11, 1, 0.0).len(), 1);
        // 0.1 s, then 0.2, 0.4 ... up to 2 s between sends: over 10 s a handful, not a hundred
        let mut sent = 1;
        for _ in 0..1000 {
            sent += a.tick(11, 1, 0.01).len();
        }
        assert!((5..=12).contains(&sent), "{sent} sends in 10 s");
    }
    #[test]
    fn an_acknowledgement_is_told_from_an_event() {
        let (mut a, mut b) = (Sync::default(), Sync::default());
        a.publish(&[2], event("door", None));
        let data = a.tick(11, 1, 0.0).remove(0);
        let ack = b.receive(11, 2, &data).unwrap();
        assert!(is_ack(&ack, 11) && !is_ack(&data, 11) && !is_ack(&ack, 10));
    }
    #[test]
    fn validates_bounds_protocol_and_recipient() {
        let mut a = Sync::default();
        let mut b = Sync::default();
        a.publish(&[2], event("horn", None));
        let packet = a.tick(11, 1, 0.0).remove(0);
        assert!(b.receive(10, 2, &packet).is_none());
        assert!(b.receive(11, 3, &packet).is_none());
        for n in 0..packet.len() {
            assert!(b.receive(11, 2, &packet[..n]).is_none());
        }
        assert!(b.take_incoming().is_empty());
        assert!(b.receive(11, 2, &packet).is_some());
        assert_eq!(b.take_incoming().len(), 1);
    }
}
