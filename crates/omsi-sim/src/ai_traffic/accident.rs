//! OMSI 2's accident (manual 2.11, "First Aid").

use super::*;

impl TrafficSim {
    pub fn accident_open(&self) -> bool {
        self.cars.iter().any(|c| c.body.is_loose())
    }

    /// How many vehicles went back onto their route.
    pub fn clear_accidents(&mut self) -> usize {
        let mut n = 0;
        for c in self.cars.iter_mut().filter(|c| c.body.is_loose()) {
            c.body.set_loose(false);
            c.state.blinker = 0;
            n += 1;
        }
        n
    }

    pub fn take_fled(&mut self) -> u32 {
        std::mem::take(&mut self.fled)
    }

    pub fn take_accidents(&mut self) -> u32 {
        std::mem::take(&mut self.accidents)
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
