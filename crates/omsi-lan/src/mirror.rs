//! A client's copy of the host's world, the parts that need no game: the poses of a thing
//! as they came (`Track`), the moment they are drawn at (`PlayClock`), the files named
//! relative to the content roots, and the activities on the wire and in the simulation.

use glam::{DVec2, DVec3, Vec3};
use omsi_net::traffic_table::Kind;
use omsi_net::world::{self as nw, Activity as NetActivity, PersonPlace, PersonState};
use omsi_sim::human::Activity;
use omsi_sim::people::MirrorPose;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Serialize the authoritative vehicle script context from a CPU-only or
/// graphical LAN host. Keeping this in omsi-lan makes scheduled AI buses
/// display the same stop list and system variables in both host modes.
pub fn ai_context(host: &omsi_sim::VehicleHost) -> omsi_net::ai_context::Context {
    omsi_net::ai_context::Context {
        active: host.schedule_active,
        line: host.tt_line.clone(),
        terminus: host.tt_terminus_index,
        stops: host
            .tt_stops
            .iter()
            .enumerate()
            .map(|(i, stop)| {
                (
                    host.tt_stop_ids.get(i).copied().unwrap_or(0),
                    stop.0.clone(),
                    stop.1,
                    stop.2,
                )
            })
            .collect(),
        index: host.tt_busstop_index,
        delay: host.tt_delay,
        system: host.system_values(),
    }
}

/// The host owns road height. Network traffic interpolation can extrapolate
/// X/Y when a frame is late, but extrapolating its vertical velocity without
/// road contact information may push cars through asphalt or above bridges.
/// Hold the latest host Z only in that short extrapolation window; ordinary
/// between-packet interpolation, rail motion and horizontal movement stay intact.
pub fn vehicle_position(
    pose: &omsi_net::traffic_table::Pose,
    latest: Option<(Kind, f64)>,
) -> [f64; 3] {
    let mut position = pose.pos;
    if pose.extrapolated {
        if let Some((kind, z)) = latest {
            let road = matches!(kind, Kind::Car | Kind::Bus | Kind::Truck | Kind::Motorcycle);
            if road && z.is_finite() {
                position[2] = z;
            }
        }
    }
    position
}

/// What decides whether a person's pose is sent again (see `quant`).
pub fn person_q(p: &PersonState) -> [i64; 4] {
    let q = match p.place {
        PersonPlace::Foot {
            x, y, z, heading, ..
        } => quant(x, y, z, heading as f64),
        PersonPlace::Aboard {
            bus,
            x,
            y,
            z,
            heading,
            ..
        } => quant(
            x as f64,
            y as f64,
            z as f64,
            heading as f64 + (bus & 0xFFFF) as f64 * 1000.0,
        ),
    };
    let act = match p.activity {
        NetActivity::Stand => 0,
        NetActivity::Walk => 1,
        NetActivity::Sit => 2,
        NetActivity::Run => 3,
    };
    [q[0], q[1], q[2], q[3] ^ (act << 40)]
}

/// Where a person is at `k` between two of the host's poses.
pub fn person_pose(a: &PersonState, b: &PersonState, k: f64) -> MirrorPose {
    match (a.place, b.place) {
        (
            PersonPlace::Foot {
                x: ax,
                y: ay,
                z: az,
                heading: ah,
                ..
            },
            PersonPlace::Foot {
                x,
                y,
                z,
                heading,
                speed,
                waiting,
            },
        ) => {
            let jump = DVec2::new(x - ax, y - ay).length() > 8.0;
            let k = if jump {
                1.0
            } else if speed < 0.05 {
                k.min(1.0)
            } else {
                k
            };
            let pos = DVec3::new(
                ax + (x - ax) * k,
                ay + (y - ay) * k,
                az + (z - az) * k.min(1.0),
            );
            let h = lerp_angle(ah as f64, heading as f64, k.min(1.0));
            let dir = DVec2::new(x - ax, y - ay);
            let vel = if speed > 0.05 && dir.length() > 1.0e-3 {
                dir.normalize() * speed as f64
            } else {
                DVec2::ZERO
            };
            MirrorPose {
                pos,
                heading: h,
                vel,
                activity: sim_activity(b.activity),
                aboard: None,
                waiting: waiting.map(|(s, k)| (s, k as usize)),
            }
        }
        (
            _,
            PersonPlace::Aboard {
                bus,
                x,
                y,
                z,
                heading,
                seat,
            },
        ) => MirrorPose {
            pos: DVec3::ZERO,
            heading: 0.0,
            vel: DVec2::ZERO,
            activity: sim_activity(b.activity),
            aboard: Some((
                if bus & nw::PLAYER_BUS != 0 {
                    omsi_sim::people::remote_bus_id(bus & nw::MAX_ID)
                } else {
                    bus as u64
                },
                Vec3::new(x, y, z),
                heading as f64,
                seat.map(|s| s as usize),
            )),
            waiting: None,
        },
        (_, PersonPlace::Foot { .. }) => person_pose(b, b, 1.0),
    }
}

/// How long a client guesses on past the last frame of something moving (ms).
pub const MAX_EXTRAPOLATE: f64 = 300.0;

/// One pose of a thing as a client got it, on the host's clock.
#[derive(Clone, Copy)]
pub struct Sample<T> {
    pub ms: f64,
    pub v: T,
}

pub struct Track<T> {
    pub samples: Vec<Sample<T>>,
    pub heard: Instant,
}

impl<T: Copy> Track<T> {
    /// An older UDP GONE must not erase a person restored by a newer snapshot.
    pub fn remove_is_current(&self, host_ms: f64) -> bool {
        self.samples
            .last()
            .is_none_or(|sample| sample.ms <= host_ms)
    }

    pub fn push(&mut self, ms: f64, v: T) {
        if self.samples.last().map(|s| ms <= s.ms).unwrap_or(false) {
            return;
        }
        self.samples.push(Sample { ms, v });
        if self.samples.len() > 12 {
            self.samples.remove(0);
        }
        self.heard = Instant::now();
    }

    /// The samples around `ms` and how far between them it lies (0..1, more when guessing
    /// on past the last).
    pub fn around(&self, ms: f64) -> Option<(&T, &T, f64)> {
        let s = &self.samples;
        let last = s.last()?;
        if s.len() == 1 || ms >= last.ms {
            let prev = if s.len() > 1 { &s[s.len() - 2] } else { last };
            let span = (last.ms - prev.ms).max(1.0);
            let t = if s.len() > 1 {
                1.0 + ((ms - last.ms).min(MAX_EXTRAPOLATE)).max(0.0) / span
            } else {
                1.0
            };
            return Some((&prev.v, &last.v, t));
        }
        if ms <= s[0].ms {
            return Some((&s[0].v, &s[0].v, 0.0));
        }
        let k = s.iter().position(|x| x.ms > ms)?;
        let (a, b) = (&s[k - 1], &s[k]);
        Some((&a.v, &b.v, (ms - a.ms) / (b.ms - a.ms).max(1.0)))
    }
}

/// The moment the others' things are drawn at, on their clock: it runs on with ours and is
/// pulled towards where it should be (now, less the delay, on their clock) by at most a
/// few per cent of its pace. Set anew each frame from the offset of the clocks and the
/// delay, it jumped with them - a datagram that came quicker than the others, a delay
/// grown after a slow frame of theirs - and everything drawn went back or on a bit at
/// once: the micro-teleports.
#[derive(Default, Clone, Copy)]
pub struct PlayClock {
    /// (the moment drawn, our clock then)
    at: Option<(f64, f64)>,
}

impl PlayClock {
    /// The moment to draw at `now` (our clock) when it should be `want`; more than `snap`
    /// off (a new session, a pause), it is taken at once.
    pub fn step(&mut self, now: f64, want: f64, snap: f64) -> f64 {
        let t = match self.at {
            Some((t, then)) => {
                let d = (now - then).max(0.0);
                let run = t + d;
                let err = want - run;
                if err.abs() > snap {
                    want
                } else {
                    run + err.clamp(-0.06 * d, 0.06 * d)
                }
            }
            None => want,
        };
        self.at = Some((t, now));
        t
    }
}

pub fn lerp_angle(a: f64, b: f64, t: f64) -> f64 {
    let d = (b - a + 540.0).rem_euclid(360.0) - 180.0;
    (a + d * t).rem_euclid(360.0)
}

pub fn quant(x: f64, y: f64, z: f64, h: f64) -> [i64; 4] {
    [
        (x * 20.0) as i64,
        (y * 20.0) as i64,
        (z * 20.0) as i64,
        (h * 4.0) as i64,
    ]
}

pub fn fnv(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// A file path as it is written for the other games: relative to its content root.
pub fn relative_file(path: &Path, root: &Path) -> String {
    let mut roots = omsi_cfg::content_roots();
    roots.push(root.to_path_buf());
    relative_to_roots(path, &roots).unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"))
}

/// `path` relative to the root of `roots` it lies in, the deepest one: an OMSI 2 folder
/// inside the content folder (`openOMSI/OMSI 2`, as a server is often laid out) names its
/// files `Vehicles/...` as the other games find them, not `OMSI 2/Vehicles/...`, which no
/// other game has (a Linux server's AI cars were left out on every client, #1097).
pub fn relative_to_roots(path: &Path, roots: &[PathBuf]) -> Option<String> {
    let r = roots
        .iter()
        .filter(|r| path.starts_with(r))
        .max_by_key(|r| r.components().count())?;
    path.strip_prefix(r)
        .ok()
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
}

/// A content-relative file from the host as a file here, when it exists here.
pub fn local_file(root: &Path, rel: &str) -> Option<PathBuf> {
    let path = omsi_cfg::resolve_path(root, rel);
    if omsi_cfg::vfs::is_file(&path) {
        return Some(path);
    }
    // (a host that named it under a folder of its own, "OMSI 2/Vehicles/...": the same
    // vehicle under any content root here, as a remote player's bus is looked for)
    let k = rel
        .to_ascii_lowercase()
        .replace('\\', "/")
        .find("vehicles/")?;
    omsi_cfg::find_in_roots(&rel.replace('\\', "/")[k..])
        .map(|(_, p)| p)
        .filter(|p| omsi_cfg::vfs::is_file(p))
}

pub fn net_activity(a: Activity) -> NetActivity {
    match a {
        Activity::Walk => NetActivity::Walk,
        Activity::Sit => NetActivity::Sit,
        _ => NetActivity::Stand,
    }
}

pub fn sim_activity(a: NetActivity) -> Activity {
    match a {
        NetActivity::Walk | NetActivity::Run => Activity::Walk,
        NetActivity::Sit => Activity::Sit,
        NetActivity::Stand => Activity::Stand,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphical_and_native_hosts_share_the_same_ai_context_wire_contract() {
        let mut host = omsi_sim::VehicleHost::new(omsi_sim::SimClock::default());
        host.schedule_active = 1.0;
        host.tt_line = "76".into();
        host.tt_terminus_index = 3;
        host.tt_stops.push(("Terminal".into(), 600.0, 610.0));
        host.tt_stop_ids.push(102);
        host.tt_busstop_index = 0;
        host.tt_delay = 12.0;
        let context = ai_context(&host);
        assert_eq!(context.line, "76");
        assert_eq!(context.stops, vec![(102, "Terminal".into(), 600.0, 610.0)]);
        let packet = context
            .packet(omsi_net::PROTOCOL as u8, 123, 900, 0)
            .expect("encode real host context");
        let mut receiver = omsi_net::ai_context::Receiver::default();
        let (id, decoded) = receiver
            .receive(&packet, omsi_net::PROTOCOL as u8)
            .expect("client decodes native host context");
        assert_eq!(id, 123);
        assert_eq!(decoded, context);
    }

    #[test]
    fn a_file_is_named_relative_to_the_deepest_root_holding_it() {
        // the content folder first, the OMSI 2 folder inside it after (a server's layout)
        let roots = [
            PathBuf::from("/srv/openOMSI"),
            PathBuf::from("/srv/openOMSI/OMSI 2"),
        ];
        let golf = Path::new("/srv/openOMSI/OMSI 2/Vehicles/VW_Golf_2/ai_vw_golf_2.bus");
        assert_eq!(
            relative_to_roots(golf, &roots).as_deref(),
            Some("Vehicles/VW_Golf_2/ai_vw_golf_2.bus")
        );
        // a mod in the content folder itself stays relative to that
        let m = Path::new("/srv/openOMSI/Vehicles/Mod/mod.bus");
        assert_eq!(
            relative_to_roots(m, &roots).as_deref(),
            Some("Vehicles/Mod/mod.bus")
        );
        assert_eq!(
            relative_to_roots(Path::new("/elsewhere/x.bus"), &roots),
            None
        );
    }

    #[test]
    fn mirrored_road_cars_keep_server_z_when_packets_are_late() {
        use omsi_net::traffic_table::{Kind, Pose};
        let pose = Pose {
            pos: [30.0, 40.0, 7.6],
            heading: 90.0,
            pitch: 0.0,
            bank: 0.0,
            speed: 18.0,
            steer: 0.0,
            blinker: 0,
            brake: false,
            lights: false,
            at_station: 0,
            extrapolated: true,
            teleported: false,
        };
        for kind in [Kind::Car, Kind::Bus, Kind::Truck, Kind::Motorcycle] {
            assert_eq!(
                vehicle_position(&pose, Some((kind, 10.0))),
                [30.0, 40.0, 10.0]
            );
        }
        let high = Pose {
            pos: [30.0, 40.0, 12.4],
            ..pose
        };
        assert_eq!(vehicle_position(&high, Some((Kind::Car, 10.0)))[2], 10.0);
        let between = Pose {
            extrapolated: false,
            ..pose
        };
        assert_eq!(vehicle_position(&between, Some((Kind::Car, 10.0)))[2], 7.6);
        assert_eq!(vehicle_position(&pose, Some((Kind::Rail, 10.0)))[2], 7.6);
        assert_eq!(vehicle_position(&pose, Some((Kind::Other, 10.0)))[2], 7.6);
        assert_eq!(vehicle_position(&pose, None)[2], 7.6);
    }

    #[test]
    fn stale_udp_gone_does_not_erase_a_newer_pedestrian() {
        let mut track = Track {
            samples: Vec::new(),
            heard: Instant::now(),
        };
        track.push(1_000.0, 5u32);
        track.push(1_200.0, 6);
        assert!(!track.remove_is_current(1_100.0));
        assert!(track.remove_is_current(1_200.0));
        assert!(track.remove_is_current(1_300.0));
        track.push(1_100.0, 9);
        assert_eq!(track.samples.last().unwrap().v, 6);
    }

    #[test]
    fn the_moment_drawn_follows_a_changed_delay_smoothly() {
        let mut c = PlayClock::default();
        let mut now = 0.0;
        let mut t = c.step(now, now - 120.0, 500.0);
        // a slow frame of theirs: the delay grows by 70 ms at once
        let mut steps = Vec::new();
        for _ in 0..100 {
            now += 16.0;
            let n = c.step(now, now - 190.0, 500.0);
            steps.push(n - t);
            t = n;
        }
        // never back, never on more than a few per cent faster or slower than the clock
        assert!(
            steps.iter().all(|d| *d > 16.0 * 0.93 && *d < 16.0 * 1.07),
            "{steps:?}"
        );
        assert!(
            (t - (now - 190.0)).abs() < 1e-6,
            "caught up: {}",
            t - (now - 190.0)
        );
        // a new session: taken at once
        now += 16.0;
        assert_eq!(c.step(now, now + 5000.0, 500.0), now + 5000.0);
    }
}
