//! The brake of a train: a handle, a brake pipe, cylinders that fill, an electric brake that
//! goes first.
//!
//! * **Air brake** (`type air`): the driver's handle lowers the pressure in the brake pipe at the
//!   head of the train; the drop runs down the pipe as a wave (`pipe_wave`, ~250 m/s) and each
//!   car's distributor fills its cylinder to match, in `apply_time`. Releasing refills the pipe and
//!   empties the cylinders slower (`release_time`). An emergency application vents the pipe at
//!   every car at once, the wave running a little faster.
//! * **Electro-pneumatic** (`type ep`): every car is told over a wire, so no wave; the cylinders
//!   still fill and empty.
//! * **Direct** (`type direct`): the force follows the handle (a tram's electro-hydraulic brake).
//!
//! Full service gives the vehicle's service deceleration (`service`, else the class's), an
//! emergency application the emergency one. With `blend`, the electric brake of the motors takes
//! what it can at the speed the train has and the friction brake only the rest: the friction
//! cylinders fill for the whole demand less what the electric brake holds.

use omsi_vehicle::rail_spec::{BrakeKind, RailBrake};
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct BrakeSystem {
    spec: RailBrake,
    /// Distance of each car's distributor from the head (m).
    dist: Vec<f32>,
    /// Cylinder fill of each car: 1 = the full service application, up to emergency/service.
    cyl: Vec<f32>,
    /// The demand at the head over time, for the wave: (time, demand).
    history: VecDeque<(f32, f32)>,
    clock: f32,
    /// Reference decelerations (m/s²).
    service: f32,
    emergency: f32,
    mass: f32,
}

/// What the brake does in a step.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BrakeOut {
    /// The friction brake's force on the whole train (N).
    pub friction: f32,
    /// The electric brake's force (N).
    pub electric: f32,
    /// The pipe/cylinder state at the head and at the tail (0..), for the cab and the debug log.
    pub head: f32,
    pub tail: f32,
}

impl BrakeSystem {
    /// `cars` cars over `length` m with `mass` kg in all; `class_service` / `class_emergency` are
    /// the decelerations the vehicle's class gives where the file says none.
    pub fn new(spec: RailBrake, cars: usize, length: f32, mass: f32, class_service: f32, class_emergency: f32) -> BrakeSystem {
        let cars = cars.max(1);
        let step = if cars > 1 { length / cars as f32 } else { 0.0 };
        BrakeSystem {
            service: spec.service.unwrap_or(class_service),
            emergency: spec.emergency.unwrap_or(class_emergency).max(spec.service.unwrap_or(class_service)),
            spec,
            dist: (0..cars).map(|i| i as f32 * step).collect(),
            cyl: vec![0.0; cars],
            history: VecDeque::new(),
            clock: 0.0,
            mass: mass.max(1000.0),
        }
    }

    pub fn cars(&self) -> usize {
        self.cyl.len()
    }

    /// The demand of a handle position: 0..0.9 is a service application up to the full one,
    /// above it up to the emergency one.
    pub fn demand_of(&self, handle: f32) -> f32 {
        let h = handle.clamp(0.0, 1.0);
        if h < 0.9 {
            h / 0.9
        } else {
            1.0 + (self.emergency / self.service - 1.0) * (h - 0.9) / 0.1
        }
    }

    /// What the demand at the head looks like at car `i` now: delayed by the wave.
    fn local_demand(&self, i: usize, emergency: bool) -> f32 {
        let wave = self.spec.pipe_wave * if emergency { 1.2 } else { 1.0 };
        let delay = match self.spec.kind {
            BrakeKind::Air => self.dist[i] / wave,
            _ => 0.0,
        };
        let t = self.clock - delay;
        let mut last = 0.0;
        for &(ts, d) in self.history.iter() {
            if ts > t {
                break;
            }
            last = d;
        }
        last
    }

    /// One step. `handle` 0..1, `v` the speed (m/s) and `electric_max` the force the electric brake
    /// could hold at it (N, 0 without one).
    pub fn step(&mut self, dt: f32, handle: f32, v: f32, electric_max: f32) -> BrakeOut {
        let dt = dt.clamp(0.0, 0.25);
        self.clock += dt;
        let demand = self.demand_of(handle);
        let emergency = demand > 1.0;
        // (the history keeps the last ten seconds, enough for a 2.5 km train)
        if self.history.back().map(|b| (b.1 - demand).abs() > 1e-4).unwrap_or(true) {
            self.history.push_back((self.clock, demand));
        }
        while self.history.len() > 2 && self.history[1].0 < self.clock - 12.0 {
            self.history.pop_front();
        }
        // the electric brake takes its share of the demand at the head, at once
        let want = demand * self.service * self.mass;
        let electric = if self.spec.blend { want.min(electric_max.max(0.0)) } else { 0.0 };
        let electric = if emergency { electric.min(electric_max) } else { electric };
        let share = if want > 1.0 { (want - electric) / want } else { 1.0 };
        let apply = 1.0 / self.spec.apply_time.max(0.05);
        let release = 1.0 / self.spec.release_time.max(0.05);
        for i in 0..self.cyl.len() {
            let target = self.local_demand(i, emergency) * share;
            let cur = self.cyl[i];
            self.cyl[i] = match self.spec.kind {
                BrakeKind::Direct => target,
                _ => {
                    if target > cur {
                        (cur + apply * dt * target.max(1.0)).min(target)
                    } else {
                        (cur - release * dt).max(target)
                    }
                }
            };
        }
        let mean = self.cyl.iter().sum::<f32>() / self.cyl.len() as f32;
        let friction = mean * self.service * self.mass;
        // (a train at a standstill holds on its friction brake only: the motors do not)
        let electric = if v.abs() < 0.3 { 0.0 } else { electric };
        BrakeOut { friction, electric, head: self.cyl[0], tail: self.cyl[self.cyl.len() - 1] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn train(kind: BrakeKind, cars: usize) -> BrakeSystem {
        BrakeSystem::new(RailBrake { kind, ..Default::default() }, cars, 200.0, 400_000.0, 0.9, 1.3)
    }

    fn settle(b: &mut BrakeSystem, handle: f32, secs: f32) -> BrakeOut {
        let mut out = BrakeOut::default();
        for _ in 0..(secs / 0.02) as usize {
            out = b.step(0.02, handle, 20.0, 0.0);
        }
        out
    }

    #[test]
    fn full_service_gives_the_service_deceleration_in_the_end() {
        let mut b = train(BrakeKind::Air, 8);
        let out = settle(&mut b, 0.9, 20.0);
        assert!((out.friction - 0.9 * 400_000.0).abs() < 400.0, "{}", out.friction);
        assert!((out.head - 1.0).abs() < 0.01 && (out.tail - 1.0).abs() < 0.01);
    }

    #[test]
    fn the_brake_reaches_the_tail_later_than_the_head() {
        let mut b = train(BrakeKind::Air, 8);
        // a second after the application the head cylinder has filled, the tail's has hardly begun
        let out = settle(&mut b, 0.9, 1.2);
        assert!(out.head > 0.2, "{}", out.head);
        assert!(out.tail < out.head * 0.5, "head {} tail {}", out.head, out.tail);
        // by wire, no wave
        let mut ep = train(BrakeKind::Ep, 8);
        let out = settle(&mut ep, 0.9, 1.2);
        assert!((out.tail - out.head).abs() < 1e-3);
    }

    #[test]
    fn releasing_is_slower_than_applying() {
        let mut b = train(BrakeKind::Air, 4);
        settle(&mut b, 0.9, 20.0);
        let released = settle(&mut b, 0.0, 6.0);
        let applied = {
            let mut c = train(BrakeKind::Air, 4);
            settle(&mut c, 0.9, 6.0)
        };
        assert!(released.friction > 0.1 * 0.9 * 400_000.0, "after 6 s the brake is still on: {}", released.friction);
        assert!(applied.friction > released.friction, "applied {} released {}", applied.friction, released.friction);
        assert!(settle(&mut b, 0.0, 30.0).friction < 1.0);
    }

    #[test]
    fn an_emergency_application_is_harder_and_quicker() {
        let mut service = train(BrakeKind::Air, 8);
        let mut emergency = train(BrakeKind::Air, 8);
        let s = settle(&mut service, 0.9, 3.0);
        let e = settle(&mut emergency, 1.0, 3.0);
        assert!(e.friction > s.friction * 1.2, "{} against {}", e.friction, s.friction);
        let full = settle(&mut emergency, 1.0, 20.0);
        assert!((full.friction - 1.3 * 400_000.0).abs() < 600.0, "{}", full.friction);
    }

    #[test]
    fn the_electric_brake_goes_first_and_the_friction_brake_makes_up_the_rest() {
        let mut b = BrakeSystem::new(RailBrake { kind: BrakeKind::Ep, ..Default::default() }, 4, 80.0, 200_000.0, 1.0, 1.4);
        let mut out = BrakeOut::default();
        for _ in 0..1500 {
            out = b.step(0.02, 0.9, 15.0, 120_000.0);
        }
        assert!((out.electric - 120_000.0).abs() < 1.0);
        assert!((out.friction + out.electric - 200_000.0).abs() < 600.0, "friction {} electric {}", out.friction, out.electric);
        // at a standstill the motors hold nothing and the friction brake takes it all
        let mut out = BrakeOut::default();
        for _ in 0..1500 {
            out = b.step(0.02, 0.9, 0.0, 120_000.0);
        }
        assert_eq!(out.electric, 0.0);
    }

    #[test]
    fn a_direct_brake_follows_the_handle() {
        let mut b = train(BrakeKind::Direct, 2);
        let out = b.step(0.02, 0.45, 5.0, 0.0);
        assert!((out.friction - 0.5 * 0.9 * 400_000.0).abs() < 400.0, "{}", out.friction);
    }

    /// The brake pipe costs distance: the same train stops further on air than by wire.
    #[test]
    fn an_air_brake_stops_a_train_later_than_an_ep_brake() {
        use crate::rail::{dynamics, Conditions, Consist, RailClass};
        let c = Consist { class: RailClass::Train, mass: 400_000.0, driven_share: 0.25, cars: 8, cd_a: 8.0, length: 200.0 };
        let stop = |kind| {
            let mut b = BrakeSystem::new(RailBrake { kind, ..Default::default() }, 8, 200.0, 400_000.0, 0.9, 1.3);
            let (mut v, mut s) = (30.0f32, 0.0f32);
            for _ in 0..(120.0 / 0.02) as usize {
                let out = b.step(0.02, 0.9, v, 0.0);
                let f = dynamics::Forces { brake: out.friction, ..Default::default() };
                v = dynamics::step(&c, &Conditions::default(), &f, v, 0.02).speed;
                s += v * 0.02;
                if v == 0.0 {
                    break;
                }
            }
            (v, s)
        };
        let (va, air) = stop(BrakeKind::Air);
        let (ve, ep) = stop(BrakeKind::Ep);
        assert_eq!((va, ve), (0.0, 0.0));
        assert!(air > ep + 10.0, "air {air} m, ep {ep} m");
        assert!(ep > 30.0 * 30.0 / (2.0 * 0.9), "even by wire the cylinders take time to fill: {ep}");
    }
}
