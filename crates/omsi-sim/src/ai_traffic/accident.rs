//! OMSI 2's accident (manual 2.11, "First Aid").
//!
//! In LAN play the host's traffic is the one simulated: a client's hits on its mirrored cars
//! and its call for the police go to the host, and the host tells each client of the
//! accidents it caused. The messages may be lost: each is sent again until it is answered
//! (`accident_to_host`, `accident_to_players`), the host carries out a hit or a call only
//! once, and the state it tells is the whole of it (totals, not changes), numbered so that
//! an old one never overwrites a newer.

use super::*;
use crate::vehicle::DynamicImpact;

/// The player of this game, as opposed to a LAN client's session id.
pub const LOCAL: u32 = 0;
/// Seconds between two sendings of a message that has not been answered.
pub const RESEND: f32 = 0.5;
/// The hits of each client the host remembers having carried out.
const HITS_REMEMBERED: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AccidentMsg {
    /// Client → host: a hit on one of the host's cars (`obstacle_id` names it the way the
    /// receiving game knows it).
    Hit {
        seq: u32,
        hit: DynamicImpact,
    },
    HitSeen {
        seq: u32,
    },
    /// Client → host: the police called to the client's accidents.
    Police {
        seq: u32,
    },
    PoliceSeen {
        seq: u32,
    },
    /// Host → client: an accident open, how many it caused and left behind in all.
    State {
        seq: u32,
        open: bool,
        new: u32,
        fled: u32,
    },
    StateSeen {
        seq: u32,
    },
}

impl AccidentMsg {
    /// The message as a LAN command. A hit names its car by `-2 - obstacle_id`, the id the
    /// host's traffic table gives it (a client's mirrored car has that id); None for a hit
    /// that names none.
    pub fn command(&self) -> Option<String> {
        Some(match *self {
            AccidentMsg::Hit { seq, hit } => {
                let car = u32::try_from(-2 - hit.obstacle_id).ok()?;
                let (p, d) = (hit.point, hit.push);
                format!(
                    "crash {seq} {car} {:.3} {:.3} {:.3} {:.4} {:.4} {:.4} {:.3} {:.1} {:.1}",
                    p.x, p.y, p.z, d.x, d.y, d.z, hit.speed, hit.energy, hit.mass
                )
            }
            AccidentMsg::HitSeen { seq } => format!("crash-seen {seq}"),
            AccidentMsg::Police { seq } => format!("police {seq}"),
            AccidentMsg::PoliceSeen { seq } => format!("police-seen {seq}"),
            AccidentMsg::State {
                seq,
                open,
                new,
                fled,
            } => format!("accident {seq} {} {new} {fled}", u8::from(open)),
            AccidentMsg::StateSeen { seq } => format!("accident-seen {seq}"),
        })
    }

    /// A LAN command read back; the host's own messages are taken only from the host. A
    /// hit's `obstacle_id` is `-2 -` the car's traffic id.
    pub fn parse(text: &str, from_host: bool) -> Option<AccidentMsg> {
        let (verb, arg) = text.split_once(' ')?;
        let n: Vec<f64> = arg
            .split_whitespace()
            .map(|x| x.parse::<f64>().ok().filter(|v| v.is_finite()))
            .collect::<Option<_>>()?;
        let seq = |v: f64| (v >= 0.0 && v <= u32::MAX as f64).then_some(v as u32);
        let host = |m: AccidentMsg| from_host.then_some(m);
        match (verb, n.as_slice()) {
            ("crash", &[s, car, px, py, pz, dx, dy, dz, speed, energy, mass])
                if speed >= 0.0 && energy >= 0.0 && mass >= 0.0 =>
            {
                Some(AccidentMsg::Hit {
                    seq: seq(s)?,
                    hit: DynamicImpact {
                        obstacle_id: -2 - seq(car)? as i64,
                        point: DVec3::new(px, py, pz),
                        push: DVec3::new(dx, dy, dz),
                        speed: speed as f32,
                        energy: energy as f32,
                        mass: mass as f32,
                    },
                })
            }
            ("police", &[s]) => Some(AccidentMsg::Police { seq: seq(s)? }),
            ("accident-seen", &[s]) => Some(AccidentMsg::StateSeen { seq: seq(s)? }),
            ("crash-seen", &[s]) => host(AccidentMsg::HitSeen { seq: seq(s)? }),
            ("police-seen", &[s]) => host(AccidentMsg::PoliceSeen { seq: seq(s)? }),
            ("accident", &[s, open, new, fled]) => host(AccidentMsg::State {
                seq: seq(s)?,
                open: open > 0.5,
                new: seq(new)?,
                fled: seq(fled)?,
            }),
            _ => None,
        }
    }
}

/// Whether a bus's hazard lights flash, for "Call the police". A bus without a hazard
/// switch only shows its lamps, dark half of the time: read at one frame, hazards that are
/// on looked off in every dark phase.
#[derive(Debug, Default, Clone, Copy)]
pub struct HazardWatch {
    since_both: Option<f32>,
}

/// Longer than an indicator's dark phase (about half a second in the stock scripts).
pub const HAZARD_HOLD: f32 = 1.5;

impl HazardWatch {
    /// One frame of `dt` seconds: whether both indicator lamps are lit now.
    pub fn step(&mut self, dt: f32, both_lit: bool) {
        self.since_both = if both_lit {
            Some(0.0)
        } else {
            self.since_both
                .map(|t| t + dt.max(0.0))
                .filter(|t| *t <= HAZARD_HOLD)
        };
    }

    /// On: the script's hazard switch (`switch`) where it has one, else both lamps lit
    /// together within the last `HAZARD_HOLD` seconds.
    pub fn on(&self, switch: Option<f32>) -> bool {
        match switch {
            Some(s) => s > 0.5,
            None => self.since_both.is_some(),
        }
    }
}

/// OMSI 2 (manual 2.11): the police are called with the bus standing and its hazard lights on.
pub fn police_may_be_called(
    hazard_switch: Option<f32>,
    speed_kmh: f32,
    watch: &HazardWatch,
) -> bool {
    watch.on(hazard_switch) && speed_kmh.abs() < 1.0
}

#[derive(Debug)]
struct Outgoing<T> {
    msg: T,
    /// Seconds to the next sending.
    wait: f32,
}

impl<T> Outgoing<T> {
    fn new(msg: T) -> Self {
        Outgoing { msg, wait: 0.0 }
    }

    /// Whether to send it in this step of `dt` seconds.
    fn due(&mut self, dt: f32) -> bool {
        self.wait -= dt;
        if self.wait > 0.0 {
            return false;
        }
        self.wait = RESEND;
        true
    }
}

/// The host's side of one client.
#[derive(Debug, Default)]
struct Client {
    /// Accidents caused and left behind in all.
    new: u32,
    fled: u32,
    /// The state last numbered, its number and the one the client answered.
    state: Option<(bool, u32, u32)>,
    seq: u32,
    seen: u32,
    wait: f32,
    hits: std::collections::VecDeque<u32>,
    police: u32,
}

/// The client's side.
#[derive(Debug, Default)]
struct Host {
    next: u32,
    hits: Vec<Outgoing<(u32, DynamicImpact)>>,
    police: Option<Outgoing<u32>>,
    /// The newest state the host told, and the totals of it already taken in.
    seq: u32,
    open: bool,
    new: u32,
    fled: u32,
    /// Accidents new and left behind not yet shown.
    shown_new: u32,
    shown_fled: u32,
}

#[derive(Debug, Default)]
pub struct Accidents {
    /// Which player knocked each loose car off its route, and the number of that player's
    /// last hit on it (0 for this game's player): a call for the police covers the hits
    /// numbered before it, whatever order they arrive in.
    by_car: HashMap<u64, (u32, u32)>,
    /// This game's player: accidents new and left behind not yet shown.
    new: u32,
    fled: u32,
    clients: HashMap<u32, Client>,
    host: Host,
}

impl Accidents {
    /// Whether a hit of `player`'s numbered `seq` may knock a car loose: one numbered before
    /// a call for the police the host has had is covered by it.
    pub(super) fn may_knock(&self, player: u32, seq: u32) -> bool {
        player == LOCAL || self.clients.get(&player).is_none_or(|c| seq > c.police)
    }

    /// A loose car hit again: the call that clears it has to come after this hit.
    pub(super) fn hit_again(&mut self, car: u64, player: u32, seq: u32) {
        if let Some((p, s)) = self.by_car.get_mut(&car) {
            if *p == player {
                *s = (*s).max(seq);
            }
        }
    }

    pub(super) fn knock(&mut self, car: u64, player: u32, seq: u32) {
        self.by_car.insert(car, (player, seq));
        if player == LOCAL {
            self.new += 1;
        } else {
            self.clients.entry(player).or_default().new += 1;
        }
    }

    pub(super) fn for_host(&mut self, hits: &[DynamicImpact]) {
        for hit in hits {
            self.host.next += 1;
            let seq = self.host.next;
            self.host.hits.push(Outgoing::new((seq, *hit)));
        }
    }
}

impl TrafficSim {
    pub fn accident_open(&self) -> bool {
        if self.mirror {
            return self.accident.host.open;
        }
        self.accident_open_by(LOCAL)
    }

    pub fn accident_open_by(&self, player: u32) -> bool {
        self.cars.iter().any(|c| {
            c.body.is_loose()
                && self
                    .accident
                    .by_car
                    .get(&c.id)
                    .is_some_and(|b| b.0 == player)
        })
    }

    /// How many vehicles go back onto their route (a client asks the host for it).
    pub fn clear_accidents(&mut self) -> usize {
        if self.mirror {
            let host = &mut self.accident.host;
            if !std::mem::take(&mut host.open) {
                return 0;
            }
            host.next += 1;
            host.police = Some(Outgoing::new(host.next));
            return 1;
        }
        self.clear_accidents_by(LOCAL)
    }

    pub fn clear_accidents_by(&mut self, player: u32) -> usize {
        self.clear_accidents_before(player, u32::MAX)
    }

    /// The accidents of `player`'s hits numbered before `seq`.
    fn clear_accidents_before(&mut self, player: u32, seq: u32) -> usize {
        let mut n = 0;
        for c in self.cars.iter_mut().filter(|c| c.body.is_loose()) {
            if self
                .accident
                .by_car
                .get(&c.id)
                .is_some_and(|b| b.0 == player && b.1 < seq)
            {
                self.accident.by_car.remove(&c.id);
                c.body.set_loose(false);
                c.state.blinker = 0;
                n += 1;
            }
        }
        n
    }

    pub fn take_fled(&mut self) -> u32 {
        if self.mirror {
            return std::mem::take(&mut self.accident.host.shown_fled);
        }
        std::mem::take(&mut self.accident.fled)
    }

    pub fn take_accidents(&mut self) -> u32 {
        if self.mirror {
            return std::mem::take(&mut self.accident.host.shown_new);
        }
        std::mem::take(&mut self.accident.new)
    }

    /// A loose car whose tile went: whoever knocked it loose left the scene.
    pub fn leave_scene(&mut self, i: usize) -> AiCar {
        let id = self.cars[i].id;
        match self.accident.by_car.remove(&id).map_or(LOCAL, |b| b.0) {
            LOCAL => self.accident.fled += 1,
            player => self.accident.clients.entry(player).or_default().fled += 1,
        }
        self.take_off(i, control::Removal::Unloaded)
    }

    /// The LAN session began or ended: what was owed to the other side goes with it.
    pub fn accident_session_changed(&mut self) {
        self.accident.clients.clear();
        self.accident.host = Host::default();
    }

    /// Client: what to send the host in this step of `dt` seconds.
    pub fn accident_to_host(&mut self, dt: f32) -> Vec<AccidentMsg> {
        let host = &mut self.accident.host;
        let mut out: Vec<AccidentMsg> = host
            .hits
            .iter_mut()
            .filter_map(|h| {
                h.due(dt).then_some(AccidentMsg::Hit {
                    seq: h.msg.0,
                    hit: h.msg.1,
                })
            })
            .collect();
        if let Some(p) = host.police.as_mut() {
            if p.due(dt) {
                out.push(AccidentMsg::Police { seq: p.msg });
            }
        }
        out
    }

    /// Client: a message of the host's, and the answer to send it.
    pub fn accident_from_host(&mut self, msg: AccidentMsg) -> Option<AccidentMsg> {
        let host = &mut self.accident.host;
        match msg {
            AccidentMsg::HitSeen { seq } => host.hits.retain(|h| h.msg.0 != seq),
            AccidentMsg::PoliceSeen { seq } => {
                if host.police.as_ref().is_some_and(|p| p.msg == seq) {
                    host.police = None;
                }
            }
            AccidentMsg::State {
                seq,
                open,
                new,
                fled,
            } => {
                if seq > host.seq {
                    host.seq = seq;
                    host.open = open;
                    host.shown_new += new.saturating_sub(host.new);
                    host.shown_fled += fled.saturating_sub(host.fled);
                    host.new = host.new.max(new);
                    host.fled = host.fled.max(fled);
                }
                return Some(AccidentMsg::StateSeen { seq });
            }
            _ => {}
        }
        None
    }

    /// Host: a message of client `player`'s, and the answer to send it. A hit names the car
    /// as this game knows it.
    pub fn accident_from_player(&mut self, player: u32, msg: AccidentMsg) -> Option<AccidentMsg> {
        match msg {
            AccidentMsg::Hit { seq, hit } => {
                let client = self.accident.clients.entry(player).or_default();
                if !client.hits.contains(&seq) {
                    client.hits.push_back(seq);
                    if client.hits.len() > HITS_REMEMBERED {
                        client.hits.pop_front();
                    }
                    self.player_impacts_by(vec![hit], player, seq);
                }
                Some(AccidentMsg::HitSeen { seq })
            }
            AccidentMsg::Police { seq } => {
                let client = self.accident.clients.entry(player).or_default();
                if seq > client.police {
                    client.police = seq;
                    self.clear_accidents_before(player, seq);
                }
                Some(AccidentMsg::PoliceSeen { seq })
            }
            AccidentMsg::StateSeen { seq } => {
                let client = self.accident.clients.entry(player).or_default();
                client.seen = client.seen.max(seq);
                None
            }
            _ => None,
        }
    }

    /// Host: what to send each of `players` in this step of `dt` seconds.
    pub fn accident_to_players(&mut self, players: &[u32], dt: f32) -> Vec<(u32, AccidentMsg)> {
        let mut out = Vec::new();
        for &player in players {
            let open = self.accident_open_by(player);
            let client = self.accident.clients.entry(player).or_default();
            let state = (open, client.new, client.fled);
            if client.state.is_none_or(|s| s != state)
                && (client.state.is_some() || state != (false, 0, 0))
            {
                client.state = Some(state);
                client.seq += 1;
                client.wait = 0.0;
            }
            client.wait -= dt;
            if client.seen < client.seq && client.wait <= 0.0 {
                client.wait = RESEND;
                let (open, new, fled) = state;
                out.push((
                    player,
                    AccidentMsg::State {
                        seq: client.seq,
                        open,
                        new,
                        fled,
                    },
                ));
            }
        }
        out
    }

    pub(super) fn stand_loose(&mut self, i: usize) -> AiFrame {
        let night = self.night;
        let car = &mut self.cars[i];
        car.state.speed = 0.0;
        car.state.acc = 0.0;
        car.state.cmd = 0.0;
        car.state.braking = true;
        car.state.blinker = 3;
        car.why = ("accident", 0.0);
        car.vehicle.set_controls(crate::Controls {
            brake: 1.0,
            ..Default::default()
        });
        AiFrame {
            speed: 0.0,
            odometer: car.state.odometer,
            steer_deg: 0.0,
            blinker: 3,
            brake: true,
            lights: night,
            at_station: 0,
            at_station_side: 0.0,
            priority_warning: false,
            engine_off: false,
        }
    }
}
