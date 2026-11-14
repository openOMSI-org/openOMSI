//! The host's world as the clients see it (`WORLD`): the people on the pavements and at
//! the stops, the riders of the timetable buses and the traffic lights around each client,
//! bit-packed. (The AI vehicles themselves are the traffic table's: `traffic_table`.)
//!
//! The host simulates the shared world alone; a client draws what the host sends and
//! simulates only its own bus (and the passengers that board it, see `CLAIM`). Every
//! datagram is complete in itself - the pose of each thing it lists, not a change against
//! an earlier one - so a lost datagram only delays the things it carried until the next
//! one. Things that move are sent ten times a second, things that stand once a second; a
//! client that has heard nothing of something for `FORGET_AFTER` lets it go, and the host
//! also names what it took away (`gone`).
//!
//! Positions are centimetres from an anchor near the client (whole metres in the header),
//! so that they fit into 19 bits (±2.6 km) whatever the map's world coordinates:
//!
//! ```text
//! byte 0      0xB4
//! byte 1      protocol version
//! bytes 2-3   sequence number (little endian, wraps)
//! bytes 4-7   the host's clock (ms since its session began, little endian, wraps)
//! bytes 8-15  anchor x, y (whole metres, i32 little endian)
//! bytes 16-17 anchor z (whole metres, i16 little endian)
//! then, least significant bit first:
//!   people 8, each:
//!     id 24, where 2 (0 on foot, 1 aboard a timetable bus, 2 aboard a player's bus),
//!     activity 2 (stand, walk, sit, run)
//!     on foot: x 19, y 19, z 17 (cm), heading 8 (360/256 deg), speed 6 (0.05 m/s),
//!       waiting 1 (then: stop 32, the stop object's id, and its waiting place 8)
//!     aboard: bus 24 (the car's id, or the player's), x 12, y 13, z 10 (cm in the bus frame), heading 8, seat 8 (255:
//!       none - walking, at the desk or the door)
//!   lights 6, each: crossing object 32, cycle position 17 (0.05 s), held 1
//!   gone 6, each: kind 1 (1 person; 0 is no longer used), id 24
//!   (first datagram, once a second) parked 1: then complete 1, count 7, each: the map
//!   id of a parking space whose car has driven off 32
//! ```
//!
//! A person takes 12 bytes (17 while waiting at a stop), a light program 6.

use crate::wire::{BitReader, BitWriter};

pub const WORLD_MAGIC: u8 = 0xB4;
/// Bytes before the bit stream.
pub const WORLD_HEADER: usize = 18;
/// The longest world datagram: below the MTU of the VPNs players use (Hamachi's is 1404,
/// Tailscale's 1280, minus the IP and UDP headers), so nothing is ever fragmented.
pub const MAX_WORLD_DATAGRAM: usize = 1180;
/// A client forgets a thing it has not heard of for this long (s); standing things are
/// sent every `STANDING_EVERY` s.
pub const FORGET_AFTER: f32 = 3.5;
pub const MOVING_EVERY: f32 = 0.1;
pub const STANDING_EVERY: f32 = 1.0;
/// Lights are sent this often (s).
pub const LIGHTS_EVERY: f32 = 1.0;

const PERSON_FOOT_BITS: usize = 24 + 2 + 2 + 19 + 19 + 17 + 8 + 6 + 1;
const PERSON_WAIT_BITS: usize = 32 + 8;
const PERSON_ABOARD_BITS: usize = 24 + 2 + 2 + 24 + 12 + 13 + 10 + 8 + 8;
const LIGHT_BITS: usize = 32 + 17 + 1;
const GONE_BITS: usize = 25;
const COUNT_BITS: usize = 8 + 6 + 6;

pub const MAX_ID: u32 = (1 << 24) - 1;
/// `PersonPlace::Aboard::bus` with this bit set is a player's bus: the rest is the player's
/// id (a rider of that player, whom the host passes on to the others).
pub const PLAYER_BUS: u32 = 1 << 31;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Activity {
    #[default]
    Stand,
    Walk,
    Sit,
    Run,
}

impl Activity {
    fn code(self) -> u64 {
        match self {
            Activity::Stand => 0,
            Activity::Walk => 1,
            Activity::Sit => 2,
            Activity::Run => 3,
        }
    }
    fn from_code(c: u64) -> Activity {
        match c {
            1 => Activity::Walk,
            2 => Activity::Sit,
            3 => Activity::Run,
            _ => Activity::Stand,
        }
    }
}

/// Where a person is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PersonPlace {
    /// On foot: world position, heading (deg), walking speed (m/s), and the stop (its
    /// object id) and waiting place when waiting there - such a person may board a
    /// client's bus (`CLAIM`).
    Foot {
        x: f64,
        y: f64,
        z: f64,
        heading: f32,
        speed: f32,
        waiting: Option<(i64, u8)>,
    },
    /// Aboard the timetable bus `bus`, at a point of its frame (m) facing `heading` (deg,
    /// in the bus frame), on seat (or standing place) `seat` of its cabin.
    Aboard {
        bus: u32,
        x: f32,
        y: f32,
        z: f32,
        heading: f32,
        seat: Option<u8>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PersonState {
    pub id: u32,
    pub activity: Activity,
    pub place: PersonPlace,
}

/// A traffic light program: the crossing object it belongs to, where in its cycle it
/// stands (s) and whether its clock waits at a stop point for a request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightState {
    pub object: i64,
    pub time: f64,
    pub held: bool,
}

/// What one datagram (or a batch of them, before `encode`) carries.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WorldFrame {
    pub seq: u16,
    /// The host's clock (ms since its session began).
    pub host_ms: u32,
    pub people: Vec<PersonState>,
    pub lights: Vec<LightState>,
    /// People (true) the host took away.
    pub gone: Vec<(bool, u32)>,
    /// The parking spaces whose cars have driven off into the traffic at the host (their
    /// map ids), and whether that is all of them: a client takes the same cars away, and
    /// puts back those the host has had park again. Without it every client kept the
    /// car the host had driven off, and the players' buses drove through it.
    pub parked: Option<(bool, Vec<u32>)>,
}

fn anchor_of(frame: &WorldFrame) -> (i32, i32, i16) {
    // the first person on foot decides; they all lie within a few km of it
    let p = frame
        .people
        .iter()
        .find_map(|p| match p.place {
            PersonPlace::Foot { x, y, z, .. } => Some((x, y, z)),
            _ => None,
        })
        .unwrap_or((0.0, 0.0, 0.0));
    let r = |v: f64| {
        if v.is_finite() {
            v.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
        } else {
            0
        }
    };
    (r(p.0), r(p.1), r(p.2).clamp(i16::MIN as i32, i16::MAX as i32) as i16)
}

/// `frame` as datagrams of at most `MAX_WORLD_DATAGRAM` bytes: the lights and the gone
/// list go into the first, the people are shared out in order. Things too far from
/// the first one to fit the anchor's range (2.6 km) are left out.
pub fn encode(frame: &WorldFrame, protocol: u8) -> Vec<Vec<u8>> {
    let (ax, ay, az) = anchor_of(frame);
    let rel = |x: f64, y: f64, z: f64| (x - ax as f64, y - ay as f64, z - az as f64);
    let fits = |x: f64, y: f64, z: f64| {
        let (dx, dy, dz) = rel(x, y, z);
        dx.abs() < 2600.0 && dy.abs() < 2600.0 && dz.abs() < 650.0
    };
    let people: Vec<&PersonState> = frame
        .people
        .iter()
        .filter(|p| match p.place {
            PersonPlace::Foot { x, y, z, .. } => fits(x, y, z),
            PersonPlace::Aboard { .. } => true,
        })
        .collect();
    let budget = (MAX_WORLD_DATAGRAM - WORLD_HEADER) * 8 - COUNT_BITS;
    let mut out = Vec::new();
    let mut pi = 0usize;
    let mut first = true;
    loop {
        let mut left = budget;
        let lights: &[LightState] = if first {
            &frame.lights[..frame.lights.len().min(63)]
        } else {
            &[]
        };
        let gone: &[(bool, u32)] = if first {
            &frame.gone[..frame.gone.len().min(63)]
        } else {
            &[]
        };
        let parked: Option<(bool, &[u32])> = if first { frame.parked.as_ref().map(|(c, k)| (*c && k.len() <= 127, &k[..k.len().min(127)])) } else { None };
        left = left.saturating_sub(lights.len() * LIGHT_BITS + gone.len() * GONE_BITS + 1 + parked.map(|p| 8 + p.1.len() * 32).unwrap_or(0));
        let p0 = pi;
        while pi < people.len() && pi - p0 < 255 {
            let bits = match people[pi].place {
                PersonPlace::Foot { waiting, .. } => {
                    PERSON_FOOT_BITS + if waiting.is_some() { PERSON_WAIT_BITS } else { 0 }
                }
                PersonPlace::Aboard { .. } => PERSON_ABOARD_BITS,
            };
            if left < bits {
                break;
            }
            left -= bits;
            pi += 1;
        }
        let mut header = Vec::with_capacity(WORLD_HEADER);
        header.extend_from_slice(&[WORLD_MAGIC, protocol]);
        header.extend_from_slice(&frame.seq.to_le_bytes());
        header.extend_from_slice(&frame.host_ms.to_le_bytes());
        header.extend_from_slice(&ax.to_le_bytes());
        header.extend_from_slice(&ay.to_le_bytes());
        header.extend_from_slice(&az.to_le_bytes());
        let mut w = BitWriter::with_header(&header);
        w.put((pi - p0) as u64, 8);
        for p in &people[p0..pi] {
            w.put(p.id.min(MAX_ID) as u64, 24);
            match p.place {
                PersonPlace::Foot {
                    x,
                    y,
                    z,
                    heading,
                    speed,
                    waiting,
                } => {
                    w.put(0, 2);
                    w.put(p.activity.code(), 2);
                    let (dx, dy, dz) = rel(x, y, z);
                    w.put_fixed(dx, 0.01, 19);
                    w.put_fixed(dy, 0.01, 19);
                    w.put_fixed(dz, 0.01, 17);
                    w.put(
                        ((heading as f64).rem_euclid(360.0) / 360.0 * 256.0).round() as u64 % 256,
                        8,
                    );
                    w.put_unsigned((speed as f64 / 0.05).round() as i64, 6);
                    match waiting {
                        Some((stop, spot)) => {
                            w.put(1, 1);
                            w.put(stop as u32 as u64, 32);
                            w.put(spot as u64, 8);
                        }
                        None => w.put(0, 1),
                    }
                }
                PersonPlace::Aboard {
                    bus,
                    x,
                    y,
                    z,
                    heading,
                    seat,
                } => {
                    w.put(if bus & PLAYER_BUS != 0 { 2 } else { 1 }, 2);
                    w.put(p.activity.code(), 2);
                    w.put((bus & !PLAYER_BUS).min(MAX_ID) as u64, 24);
                    w.put_fixed(x as f64, 0.01, 12);
                    w.put_fixed(y as f64, 0.01, 13);
                    w.put_fixed(z as f64, 0.01, 10);
                    w.put(
                        ((heading as f64).rem_euclid(360.0) / 360.0 * 256.0).round() as u64 % 256,
                        8,
                    );
                    w.put(seat.map(|s| s.min(254)).unwrap_or(255) as u64, 8);
                }
            }
        }
        w.put(lights.len() as u64, 6);
        for l in lights {
            w.put(l.object as u32 as u64, 32);
            // (a held clock rests just before its stop: rounding up would put every
            // client on the far side of the phase change, yellow while the host is green)
            let units = if l.held {
                ((l.time + 5e-6) / 0.05).floor()
            } else {
                (l.time / 0.05).round()
            };
            w.put_unsigned(units as i64, 17);
            w.put(l.held as u64, 1);
        }
        w.put(gone.len() as u64, 6);
        for (person, id) in gone {
            w.put(*person as u64, 1);
            w.put((*id).min(MAX_ID) as u64, 24);
        }
        w.put(parked.is_some() as u64, 1);
        if let Some((complete, keys)) = parked {
            w.put(complete as u64, 1);
            w.put(keys.len() as u64, 7);
            for k in keys {
                w.put(*k as u64, 32);
            }
        }
        out.push(w.finish());
        first = false;
        if pi >= people.len() {
            break;
        }
    }
    out
}

/// One datagram, or None when it is not a world datagram of this protocol or is cut short.
/// Whatever it holds decodes to finite numbers in the ranges above.
pub fn decode(data: &[u8], protocol: u8) -> Option<WorldFrame> {
    if data.len() < WORLD_HEADER
        || data.len() > MAX_WORLD_DATAGRAM
        || data[0] != WORLD_MAGIC
        || data[1] != protocol
    {
        return None;
    }
    let seq = u16::from_le_bytes([data[2], data[3]]);
    let host_ms = u32::from_le_bytes(data[4..8].try_into().ok()?);
    let ax = i32::from_le_bytes(data[8..12].try_into().ok()?) as f64;
    let ay = i32::from_le_bytes(data[12..16].try_into().ok()?) as f64;
    let az = i16::from_le_bytes(data[16..18].try_into().ok()?) as f64;
    let mut r = BitReader::new(&data[WORLD_HEADER..]);
    let mut f = WorldFrame {
        seq,
        host_ms,
        ..Default::default()
    };
    let n = r.get(8)?;
    for _ in 0..n {
        let id = r.get(24)? as u32;
        let place = r.get(2)?;
        let activity = Activity::from_code(r.get(2)?);
        let place = match place {
            0 => {
                let x = ax + r.get_fixed(0.01, 19)?;
                let y = ay + r.get_fixed(0.01, 19)?;
                let z = az + r.get_fixed(0.01, 17)?;
                let heading = r.get(8)? as f32 * 360.0 / 256.0;
                let speed = r.get(6)? as f32 * 0.05;
                let waiting = if r.get(1)? == 1 {
                    let stop = r.get(32)? as i64;
                    let spot = r.get(8)? as u8;
                    Some((stop, spot))
                } else {
                    None
                };
                PersonPlace::Foot {
                    x,
                    y,
                    z,
                    heading,
                    speed,
                    waiting,
                }
            }
            w @ (1 | 2) => PersonPlace::Aboard {
                bus: r.get(24)? as u32 | if w == 2 { PLAYER_BUS } else { 0 },
                x: r.get_fixed(0.01, 12)? as f32,
                y: r.get_fixed(0.01, 13)? as f32,
                z: r.get_fixed(0.01, 10)? as f32,
                heading: r.get(8)? as f32 * 360.0 / 256.0,
                seat: Some(r.get(8)? as u8).filter(|s| *s != 255),
            },
            _ => return None,
        };
        f.people.push(PersonState {
            id,
            activity,
            place,
        });
    }
    let n = r.get(6)?;
    for _ in 0..n {
        let object = r.get(32)? as i64;
        let time = r.get(17)? as f64 * 0.05;
        let held = r.get(1)? == 1;
        f.lights.push(LightState { object, time, held });
    }
    let n = r.get(6)?;
    for _ in 0..n {
        let person = r.get(1)? == 1;
        let id = r.get(24)? as u32;
        f.gone.push((person, id));
    }
    // (an older host's datagram ends here: its padding reads as "no parked list")
    if r.get(1).unwrap_or(0) == 1 {
        let complete = r.get(1)? == 1;
        let n = r.get(7)?;
        let mut keys = Vec::with_capacity(n as usize);
        for _ in 0..n {
            keys.push(r.get(32)? as u32);
        }
        f.parked = Some((complete, keys));
    }
    Some(f)
}

/// A car (false) or a person (true) of the host's world, by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityRef {
    pub person: bool,
    pub id: u32,
}

impl EntityRef {
    /// `c12,p7,…`
    pub fn list(refs: &[EntityRef]) -> String {
        refs.iter()
            .map(|r| format!("{}{}", if r.person { 'p' } else { 'c' }, r.id))
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn parse_list(text: &str) -> Vec<EntityRef> {
        text.split(',')
            .filter_map(|t| {
                let t = t.trim();
                let person = match t.chars().next()? {
                    'c' => false,
                    'p' => true,
                    _ => return None,
                };
                let id = t[1..].parse::<u32>().ok().filter(|i| *i <= MAX_ID)?;
                Some(EntityRef { person, id })
            })
            .take(64)
            .collect()
    }
}

/// What a person is, told once per client (and again when it asks, `WANT`):
///
/// ```text
/// DESC|p|<id>|<human file>
/// ```
///
/// The file is relative to a content root (`Humans/…/x.hum`) and checked like a player's
/// vehicle path. (What a car is - its vehicle file, repaint, line and destination - is told
/// with the car's row, see `traffic_table`; `DESC|c|…` was protocol 14's.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Desc {
    Person { id: u32, file: String },
}

impl Desc {
    pub fn encode(&self) -> String {
        match self {
            Desc::Person { id, file } => {
                format!("DESC|p|{id}|{}", crate::clean_text(file, 260))
            }
        }
    }

    /// From the fields of a `DESC` message (checked and cleaned), or None.
    pub fn decode(parts: &[&str]) -> Option<Desc> {
        if parts.len() < 4 || parts[0] != "DESC" {
            return None;
        }
        let id = parts[2].trim().parse::<u32>().ok().filter(|i| *i <= MAX_ID)?;
        match parts[1] {
            "p" => Some(Desc::Person {
                id,
                file: content_file(parts[3], &["hum"])?,
            }),
            _ => None,
        }
    }
}

/// A file path from the network as a path relative to a content root, or None: relative,
/// no `..`, `.` or empty parts, one of `exts` (see `vehicle_path`).
pub fn content_file(p: &str, exts: &[&str]) -> Option<String> {
    let p = p.trim().replace('\\', "/");
    if p.is_empty()
        || p.len() > 260
        || p.starts_with('/')
        || p.chars().any(|c| c == ':' || c == '|' || c.is_control())
    {
        return None;
    }
    if p.split('/')
        .any(|part| part.trim_matches(|c| c == '.' || c == ' ').is_empty())
    {
        return None;
    }
    let ext = p.rsplit('/').next()?.rsplit_once('.')?.1.to_ascii_lowercase();
    exts.contains(&ext.as_str()).then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_round_trips_in_spandau_coordinates() {
        let f = WorldFrame {
            seq: 65535,
            host_ms: 123_456_789,
            people: vec![
                PersonState {
                    id: 12,
                    activity: Activity::Sit,
                    place: PersonPlace::Foot {
                        x: 892_250.5,
                        y: 4_196_470.25,
                        z: 33.9,
                        heading: 45.0,
                        speed: 0.0,
                        waiting: Some((3_000_123_456, 3)),
                    },
                },
                PersonState {
                    id: 13,
                    activity: Activity::Walk,
                    place: PersonPlace::Foot {
                        x: 892_240.0,
                        y: 4_196_400.0,
                        z: 34.0,
                        heading: 180.0,
                        speed: 1.4,
                        waiting: None,
                    },
                },
                PersonState {
                    id: 14,
                    activity: Activity::Sit,
                    place: PersonPlace::Aboard {
                        bus: 7,
                        x: -0.62,
                        y: -8.4,
                        z: 1.05,
                        heading: 180.0,
                        seat: Some(12),
                    },
                },
                PersonState {
                    id: 15,
                    activity: Activity::Stand,
                    place: PersonPlace::Aboard {
                        bus: PLAYER_BUS | 3,
                        x: 0.4,
                        y: 2.0,
                        z: 1.0,
                        heading: 90.0,
                        seat: None,
                    },
                },
            ],
            lights: vec![LightState {
                object: 4711,
                time: 63.45,
                held: true,
            }],
            gone: vec![(true, 3), (true, 99)],
            parked: Some((true, vec![242685, 7])),
        };
        let d = encode(&f, 4);
        assert_eq!(d.len(), 1);
        // (two parked spaces add 9 bytes: 1 + 1 + 7 + 2 x 32 bits)
        assert!(d[0].len() <= 132, "{} bytes", d[0].len());
        let g = decode(&d[0], 4).unwrap();
        assert_eq!((g.seq, g.host_ms), (65535, 123_456_789));
        assert_eq!(g.people.len(), 4);
        match g.people[0].place {
            PersonPlace::Foot { waiting, x, .. } => {
                assert_eq!(waiting, Some((3_000_123_456, 3)));
                assert!((x - 892_250.5).abs() < 0.006);
            }
            _ => panic!(),
        }
        assert_eq!(g.people[0].activity, Activity::Sit);
        match g.people[2].place {
            PersonPlace::Aboard { bus, y, seat, .. } => {
                assert!(bus == 7 && (y + 8.4).abs() < 0.006 && seat == Some(12))
            }
            _ => panic!(),
        }
        // a rider of player 3's bus
        match g.people[3].place {
            PersonPlace::Aboard { bus, seat, .. } => assert!(bus == PLAYER_BUS | 3 && seat.is_none()),
            _ => panic!(),
        }
        assert_eq!(g.lights.len(), 1);
        assert!(g.lights[0].held && (g.lights[0].time - 63.45).abs() < 0.026);
        assert_eq!(g.gone, vec![(true, 3), (true, 99)]);
        assert_eq!(g.parked, Some((true, vec![242685, 7])));
        // anything else is not a frame
        assert!(decode(&d[0], 3).is_none());
        assert!(decode(&d[0][..d[0].len() - 3], 4).is_none() || d[0].len() < 3);
        for n in 0..d[0].len() {
            let _ = decode(&d[0][..n], 4);
        }
    }

    #[test]
    fn a_held_clock_just_before_a_phase_change_is_encoded_before_it() {
        // Green until 22 s, yellow after: the host holds at 22.0 - 1e-5 while a bus asks.
        let frame = WorldFrame {
            lights: vec![LightState { object: 1, time: 22.0 - 1e-5, held: true }],
            ..WorldFrame::default()
        };
        let g = decode(&encode(&frame, 4)[0], 4).unwrap();
        assert!(g.lights[0].held);
        assert!(g.lights[0].time < 22.0, "held at {}", g.lights[0].time);
        assert!(22.0 - g.lights[0].time <= 0.05 + 1e-9, "held at {}", g.lights[0].time);
    }

    #[test]
    fn a_busy_street_is_shared_out_over_datagrams() {
        let f = WorldFrame {
            people: (0..300)
                .map(|i| PersonState {
                    id: 1000 + i,
                    activity: Activity::Walk,
                    place: PersonPlace::Foot {
                        x: 892_000.0 + i as f64,
                        y: 4_196_000.0,
                        z: 30.0,
                        heading: 0.0,
                        speed: 1.2,
                        waiting: (i % 3 == 0).then_some((i as i64, 1)),
                    },
                })
                .collect(),
            lights: (0..10)
                .map(|i| LightState {
                    object: i,
                    time: 1.0,
                    held: false,
                })
                .collect(),
            ..Default::default()
        };
        let d = encode(&f, 4);
        assert!(d.iter().all(|x| x.len() <= MAX_WORLD_DATAGRAM));
        let frames: Vec<WorldFrame> = d.iter().map(|x| decode(x, 4).unwrap()).collect();
        assert_eq!(frames.iter().map(|g| g.people.len()).sum::<usize>(), 300);
        assert_eq!(frames.iter().map(|g| g.lights.len()).sum::<usize>(), 10);
        let bytes: usize = d.iter().map(|x| x.len()).sum();
        // 12 to 17 bytes a person
        assert!(bytes < 300 * 15 + 200, "{bytes} bytes in {} datagrams", d.len());
        // something 5 km away from the rest does not fit the anchor and is left out
        let person = |x: f64| PersonState {
            id: 1,
            activity: Activity::Walk,
            place: PersonPlace::Foot { x, y: 0.0, z: 0.0, heading: 0.0, speed: 0.0, waiting: None },
        };
        let far = WorldFrame {
            people: vec![person(0.0), person(5000.0)],
            ..Default::default()
        };
        assert_eq!(decode(&encode(&far, 4)[0], 4).unwrap().people.len(), 1);
    }

    #[test]
    fn descriptions_and_requests() {
        for bad in [
            "DESC|p|1|Humans/x.bus",
            "DESC|p|99999999|Humans/x.hum",
            "DESC|x|1|Humans/x.hum",
            "DESC|c|1|Vehicles/x.bus|-||",
        ] {
            let parts: Vec<&str> = bad.split('|').collect();
            assert!(Desc::decode(&parts).is_none(), "{bad}");
        }
        let p = Desc::Person {
            id: 5,
            file: "Humans\\Generic\\m1.hum".into(),
        };
        let text = p.encode();
        let parts: Vec<&str> = text.split('|').collect();
        assert_eq!(
            Desc::decode(&parts).unwrap(),
            Desc::Person {
                id: 5,
                file: "Humans/Generic/m1.hum".into()
            }
        );
        let refs = vec![
            EntityRef { person: false, id: 3 },
            EntityRef { person: true, id: 16_777_215 },
        ];
        assert_eq!(EntityRef::parse_list(&EntityRef::list(&refs)), refs);
        assert!(EntityRef::parse_list("x1,c,p99999999,c-1").is_empty());
    }
}
