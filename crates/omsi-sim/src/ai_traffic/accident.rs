//! OMSI 2's accident (manual 2.11, "First Aid"): an AI vehicle the player's vehicle runs into
//! is knocked off its route (`TrafficSim::player_impacts`). It stands where the blow left it,
//! fully braked with its hazard lights on, until the player calls the police - then it goes
//! back onto its way and drives on - or until its tile is unloaded: then it goes, and the
//! player has left the scene of the accident (omsi-app's traffic population).

use super::*;

impl TrafficSim {
    /// Whether an accident waits for the police: an AI vehicle the player knocked off its
    /// route still stands where it was left.
    pub fn accident_open(&self) -> bool {
        self.cars.iter().any(|c| c.body.is_loose())
    }

    /// The police have taken up the accident (OMSI 2's "Call the police"): every vehicle
    /// knocked off its route goes back onto it where its driver left it and drives on. How
    /// many there were.
    pub fn clear_accidents(&mut self) -> usize {
        let mut n = 0;
        for c in self.cars.iter_mut().filter(|c| c.body.is_loose()) {
            c.body.set_loose(false);
            c.state.blinker = 0;
            n += 1;
        }
        n
    }

    /// Vehicles knocked off their route that went with their tile since the last call:
    /// the player left the scene of the accident without calling the police.
    pub fn take_fled(&mut self) -> u32 {
        std::mem::take(&mut self.fled)
    }

    /// AI vehicles knocked off their route since the last call (new accidents).
    pub fn take_accidents(&mut self) -> u32 {
        std::mem::take(&mut self.accidents)
    }

    /// A car knocked off its route by the player (OMSI 2's accident): no driver plans for
    /// it, it stands fully braked with its hazard lights on where the blow left it until the
    /// police are called (`clear_accidents`) or its tile goes.
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
