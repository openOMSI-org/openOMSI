use super::*;

pub const FLASH_MAX: f32 = 1.5;
pub const WAVE_SECS: f32 = 6.0;
pub const WAVE_MAX_SPEED: f32 = 3.0;

impl TrafficSim {
    pub fn note_high_beam(&mut self, on: bool, dt: f32) {
        let slow = self.player.is_none_or(|p| p.4.abs() < WAVE_MAX_SPEED);
        self.player_waved = if slow { (self.player_waved - dt).max(0.0) } else { 0.0 };
        if on {
            self.player_beam_on += dt;
            return;
        }
        if slow && self.player_beam_on > 0.0 && self.player_beam_on <= FLASH_MAX {
            self.player_waved = WAVE_SECS;
        }
        self.player_beam_on = 0.0;
    }

    pub fn player_waving(&self) -> bool {
        self.player_waved > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::super::scenario_tests::{add_car, traffic, Fixture};
    use super::*;

    fn beam(t: &mut TrafficSim, on: bool, secs: f32) {
        for _ in 0..(secs / 0.05).round() as usize {
            t.note_high_beam(on, 0.05);
        }
    }

    #[test]
    fn a_short_flash_waves_the_traffic_on_for_a_while() {
        let f = Fixture::new();
        let mut t = traffic(&f, Network::default());
        beam(&mut t, true, 0.4);
        assert!(!t.player_waving());
        beam(&mut t, false, 0.05);
        assert!(t.player_waving());
        beam(&mut t, false, WAVE_SECS - 0.5);
        assert!(t.player_waving());
        beam(&mut t, false, 1.0);
        assert!(!t.player_waving());
    }

    fn side_car_across(flash: bool, x0: f64, v: f32) -> Option<f32> {
        let f = Fixture::new();
        let mut t = traffic(&f, super::super::scenario_tests::side_road());
        let me = add_car(&mut t, &f, 3, 80.0, 0x99, Some(0.0));
        let mut x = x0;
        let mut time = 0.0f32;
        while time < 14.0 {
            let beam_on = flash && (time % 5.0) < 0.3;
            t.note_high_beam(beam_on, 0.05);
            x += v as f64 * 0.05;
            t.tick(0.05, Some((DVec3::new(x, 0.0, 0.0), 90.0, 6.0, 1.25, v)));
            time += 0.05;
            if t.cars.iter().find(|c| c.id == me).is_none_or(|c| c.state.lane == 5 || (c.state.lane == 4 && c.state.s > 15.0)) {
                return Some(time);
            }
        }
        None
    }

    #[test]
    fn a_side_road_car_waiting_for_the_bus_goes_when_waved_on() {
        let waited = side_car_across(false, -20.0, 0.3);
        let waved = side_car_across(true, -20.0, 0.3);
        assert!(waited.is_none(), "the car waits for the creeping bus: {waited:?}");
        assert!(waved.is_some(), "waved on, it goes");
    }

    #[test]
    fn a_moving_bus_waves_nobody_on() {
        let f = Fixture::new();
        let mut t = traffic(&f, Network::default());
        t.player = Some((DVec3::ZERO, 0.0, 6.0, 1.25, 10.0));
        beam(&mut t, true, 0.4);
        beam(&mut t, false, 0.05);
        assert!(!t.player_waving(), "at 36 km/h a flash is no invitation");
        t.player = Some((DVec3::ZERO, 0.0, 6.0, 1.25, 0.5));
        beam(&mut t, true, 0.4);
        beam(&mut t, false, 0.05);
        assert!(t.player_waving());
        t.player = Some((DVec3::ZERO, 0.0, 6.0, 1.25, 8.0));
        beam(&mut t, false, 0.05);
        assert!(!t.player_waving(), "driving off ends it");
    }

    #[test]
    fn the_high_beam_left_on_is_no_flash() {
        let f = Fixture::new();
        let mut t = traffic(&f, Network::default());
        beam(&mut t, true, 5.0);
        beam(&mut t, false, 0.05);
        assert!(!t.player_waving());
    }
}
