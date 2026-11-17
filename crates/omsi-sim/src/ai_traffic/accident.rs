//! OMSI 2's accident (manual 2.11, "First Aid").
//!
//! In LAN play the host's traffic is the one simulated: a client's hits on its mirrored cars
//! and its call for the police go to the host (`take_for_host`), and the host tells each
//! player about the accidents it caused (`accident_note`, `accident_from_host`).

use super::*;

/// The player of this game, as opposed to a LAN client's session id.
pub const LOCAL: u32 = 0;

#[derive(Debug, Default)]
pub struct Accidents {
    /// Which player knocked each loose car off its route.
    by_car: HashMap<u64, u32>,
    new: HashMap<u32, u32>,
    fled: HashMap<u32, u32>,
    /// Host: whether each client was last told it has an accident open.
    told: HashMap<u32, bool>,
    /// Client: the hits and the call for the police the host has not had yet.
    for_host: Vec<crate::vehicle::DynamicImpact>,
    police_for_host: bool,
    /// Client: what the host said of our accident: open, new ones, left behind.
    from_host: (bool, u32, u32),
}

impl Accidents {
    pub(super) fn knock(&mut self, car: u64, player: u32) {
        self.by_car.insert(car, player);
        *self.new.entry(player).or_default() += 1;
    }

    pub(super) fn for_host(&mut self, hits: &[crate::vehicle::DynamicImpact]) {
        self.for_host.extend_from_slice(hits);
    }
}

impl TrafficSim {
    pub fn accident_open(&self) -> bool {
        if self.mirror {
            return self.accident.from_host.0;
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
            let open = std::mem::take(&mut self.accident.from_host.0);
            self.accident.police_for_host |= open;
            return usize::from(open);
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
            return std::mem::take(&mut self.accident.from_host.2);
        }
        self.accident.fled.remove(&LOCAL).unwrap_or(0)
    }

    pub fn take_accidents(&mut self) -> u32 {
        if self.mirror {
            return std::mem::take(&mut self.accident.from_host.1);
        }
        self.accident.new.remove(&LOCAL).unwrap_or(0)
    }

    /// A loose car whose tile went: whoever knocked it loose left the scene.
    pub fn leave_scene(&mut self, i: usize) -> AiCar {
        let id = self.cars[i].id;
        let player = self.accident.by_car.remove(&id).unwrap_or(LOCAL);
        *self.accident.fled.entry(player).or_default() += 1;
        self.take_off(i, control::Removal::Unloaded)
    }

    /// Host: what to tell client `player` (open, new, left behind), when anything changed.
    pub fn accident_note(&mut self, player: u32) -> Option<(bool, u32, u32)> {
        let open = self.accident_open_by(player);
        let new = self.accident.new.remove(&player).unwrap_or(0);
        let fled = self.accident.fled.remove(&player).unwrap_or(0);
        let told = self.accident.told.insert(player, open).unwrap_or(false);
        (new > 0 || fled > 0 || open != told).then_some((open, new, fled))
    }

    /// Client: the host's `accident_note` for us.
    pub fn accident_from_host(&mut self, open: bool, new: u32, fled: u32) {
        let a = &mut self.accident.from_host;
        *a = (open, a.1 + new, a.2 + fled);
    }

    /// Client: our hits on the host's cars and whether we called the police.
    pub fn take_for_host(&mut self) -> (Vec<crate::vehicle::DynamicImpact>, bool) {
        let police = std::mem::take(&mut self.accident.police_for_host);
        (std::mem::take(&mut self.accident.for_host), police)
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
