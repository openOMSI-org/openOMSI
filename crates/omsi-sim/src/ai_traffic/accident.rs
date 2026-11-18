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
/// Seconds a client keeps sending a hit: a car gone by then is gone on the host as well.
const HIT_GIVE_UP: f32 = 10.0;
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

#[derive(Debug)]
struct Outgoing<T> {
    msg: T,
    /// Seconds to the next sending, and since the first.
    wait: f32,
    age: f32,
}

impl<T> Outgoing<T> {
    fn new(msg: T) -> Self {
        Outgoing {
            msg,
            wait: 0.0,
            age: 0.0,
        }
    }

    /// Whether to send it in this step of `dt` seconds.
    fn due(&mut self, dt: f32) -> bool {
        self.age += dt;
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
    /// Which player knocked each loose car off its route.
    by_car: HashMap<u64, u32>,
    /// This game's player: accidents new and left behind not yet shown.
    new: u32,
    fled: u32,
    clients: HashMap<u32, Client>,
    host: Host,
}

impl Accidents {
    pub(super) fn knock(&mut self, car: u64, player: u32) {
        self.by_car.insert(car, player);
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
        self.cars
            .iter()
            .any(|c| c.body.is_loose() && self.accident.by_car.get(&c.id) == Some(&player))
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
        let mut n = 0;
        for c in self.cars.iter_mut().filter(|c| c.body.is_loose()) {
            if self.accident.by_car.get(&c.id) == Some(&player) {
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
        match self.accident.by_car.remove(&id).unwrap_or(LOCAL) {
            LOCAL => self.accident.fled += 1,
            player => self.accident.clients.entry(player).or_default().fled += 1,
        }
        self.take_off(i, control::Removal::Unloaded)
    }

    /// Client: what to send the host in this step of `dt` seconds.
    pub fn accident_to_host(&mut self, dt: f32) -> Vec<AccidentMsg> {
        let host = &mut self.accident.host;
        host.hits.retain(|h| h.age < HIT_GIVE_UP);
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
                    host.open = open && host.police.is_none();
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
                    self.player_impacts_by(vec![hit], player);
                }
                Some(AccidentMsg::HitSeen { seq })
            }
            AccidentMsg::Police { seq } => {
                let client = self.accident.clients.entry(player).or_default();
                if seq > client.police {
                    client.police = seq;
                    self.clear_accidents_by(player);
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
