//! The AI drivers' horn (`ev_AI_Horn`, a trigger of the vehicle's script): presentation
//! only, it decides nothing - no claim, no stop, no speed depends on it.
//!
//! A driver honks as an annoyed driver does, in two situations:
//! 1. Somebody stops or brakes sharply right in front of him: he is still doing more than
//!    10 km/h and the vehicle ahead is suddenly closer than 0.3 times his braking distance
//!    (with a second of reaction, (v)²/a + v), or he has to brake harder than any driver
//!    brakes by choice for the vehicle ahead. One short honk. This is where Omsi.exe's own
//!    condition comes from: its AI tick (FUN_007d9128, asm 0x7db3a8-0x7db426) triggers
//!    `ev_AI_Horn` when the speed is over 10 km/h, the distance to the obstacle is below
//!    0.3 times the braking distance (v/3.6)²/a + v/3.6 and a second has passed since the
//!    last honk - but only while a flag (TPathInfo+0x108) is set that its look-ahead clears
//!    at its start (0x716b27) and nothing ever sets, so in 2.3.004 the AI never honks.
//!    Taken as it is, it would sound behind every leader at an ordinary following
//!    distance; here it needs the leader to be closing in fast or braking hard.
//! 2. He has stood a long time behind a vehicle that does not move with nothing lawful to
//!    explain it - no red light, no right of way it waits for, no queue that moves or
//!    stands at a light or junction further on, no bus at its stop: a car or a bus that
//!    just stands. After a patience of his own (20-45 s, a timetable bus's driver longer)
//!    he honks, and again every 15-30 s while it lasts.
//!
//! The one honked at, if he stands there because something blocks him in turn, may honk
//! back once (four times in ten) half a second to a second and a half later. Nobody honks
//! more often than every `HORN_COOLDOWN` seconds, and rail vehicles never do. All the
//! chances are drawn from the car's seed and its own count of honks, never from a clock or
//! a shared generator, so a run is the same at any frame rate and in any order of the cars.

use super::*;

/// Seconds between two honks of one driver, of whatever kind.
pub const HORN_COOLDOWN: f32 = 3.0;
/// Below this speed (m/s, 10 km/h) nobody honks at a sharp stop ahead.
pub const HORN_MIN_SPEED: f32 = 10.0 / 3.6;
/// The share of his braking distance within which a sharp stop ahead makes him honk.
pub const HORN_SHARE: f32 = 0.3;
/// Braking harder than this (m/s²) for the vehicle ahead is an emergency stop.
pub const HORN_EMERGENCY: f32 = 5.0;

/// A driver's horn: when he last honked, how often, and how long he has stood stuck.
#[derive(Debug, Clone, Copy)]
pub struct Horn {
    pub last: f32,
    pub honks: u32,
    /// Seconds stuck behind a vehicle that just stands.
    pub stuck: f32,
    /// When the next honk at it is due (sim time), once the first has sounded.
    pub again: f32,
    /// A sharp stop ahead was seen the step before (one honk per stop).
    pub sharp: bool,
    /// When he answers a honk (sim time), and how many honks he has been given.
    pub reply_at: Option<f32>,
    pub honked_at: u32,
}

impl Default for Horn {
    fn default() -> Horn {
        Horn { last: f32::MIN, honks: 0, stuck: 0.0, again: f32::MAX, sharp: false, reply_at: None, honked_at: 0 }
    }
}

/// Omsi.exe's braking distance with a second of reaction (m) at `v` m/s with deceleration
/// `a` (m/s²).
pub fn braking_distance(v: f32, a: f32) -> f32 {
    v * v / a.max(0.5) + v
}

/// Situation 1: is the vehicle `gap` m ahead, at `lead_v` m/s and braking at `lead_acc`, a
/// sharp stop in front of a driver at `v` m/s who brakes at `a` and is braking at `acc`?
pub fn sharp_stop(v: f32, a: f32, acc: f32, gap: f32, lead_v: f32, lead_acc: f32) -> bool {
    if v <= HORN_MIN_SPEED {
        return false;
    }
    let close = gap < HORN_SHARE * braking_distance(v, a) && (v - lead_v > 2.0 || lead_acc < -3.0);
    close || (acc < -HORN_EMERGENCY && gap < 30.0)
}

/// Seconds a driver (`seed`) stands stuck before he honks the first time.
pub fn patience(seed: u64, honk_factor: f32, bus: bool) -> f32 {
    let p = 20.0 + 25.0 * random::draw(seed, 1);
    (if bus {
        p * 1.5
    } else {
        p 
    }) * honk_factor
}

/// Seconds until he honks again (his `n`-th honk).
pub fn repeat(seed: u64, honk_factor: f32, n: u32) -> f32 {
    (15.0 + 15.0 * random::draw(seed, 100 + n as u64)) * honk_factor
}

impl TrafficSim {
    /// Why the vehicle car `i` stands behind, if it stands for nothing lawful: the car it
    /// follows (or the player's vehicle, `stats::WAITS_ON_PLAYER`) when the queue ahead
    /// ends in a vehicle that just stands.
    fn stuck_behind(&self, i: usize, player_standing: f32) -> Option<u64> {
        let c = &self.cars[i];
        if c.state.speed > 0.3 || c.why.1 > 12.0 {
            return None;
        }
        match c.why.0 {
            "player" => return (player_standing > 3.0).then_some(super::stats::WAITS_ON_PLAYER),
            // (behind one that has stood long it keeps back, to be able to go round it)
            "lead" | "keep_back" => {}
            _ => return None,
        }
        let first = c.lead_car?;
        // the queue ahead, as far as it stands nose to tail
        let mut at = first;
        for _ in 0..16 {
            let &k = self.index_of.get(&at)?;
            let o = self.cars.get(k).filter(|o| o.id == at)?;
            let lawful = o.state.speed > 0.3
                || o.light_hold
                || o.yielding
                || o.at_stop()
                || matches!(o.why.0, "light" | "yield" | "people" | "merge" | "service" | "end" | "park" | "pull_out" | "let_out");
            if lawful {
                return None;
            }
            match (o.why.0, o.lead_car) {
                ("lead" | "keep_back", Some(next)) if o.why.1 < 12.0 => at = next,
                ("player", _) if o.why.1 < 12.0 => return (player_standing > 3.0).then_some(first),
                // it stands with nothing before it that a driver behind could see
                _ => return Some(first),
            }
        }
        None
    }

    /// Every driver's horn this step (see the module), in the order of their ids.
    pub fn sound_horns(&mut self, player: Option<PlayerBox>, player_standing: f32) {
        let dt = self.last_dt;
        let mut order: Vec<usize> = (0..self.cars.len()).collect();
        order.sort_unstable_by_key(|&i| self.cars[i].id);
        for i in order {
            if self.cars[i].is_rail() {
                continue;
            }
            let now = self.time;
            // the vehicle ahead, as the step saw it: (gap, its speed, its braking)
            let c = &self.cars[i];
            let ahead = match c.why.0 {
                "lead" => c.lead_car.and_then(|id| self.index_of.get(&id)).and_then(|&k| self.cars.get(k)).map(|o| (c.why.1, o.state.speed, o.state.acc)),
                "player" => player.map(|p| (c.why.1, p.4.max(0.0), 0.0)),
                _ => None,
            };
            let st = &c.state;
            let honk_factor: f32 = st.honk_factor;
            let sharp = ahead.is_some_and(|(gap, v, acc)| sharp_stop(st.speed, st.decel, st.acc, gap, v, acc));
            let stuck = self.stuck_behind(i, player_standing);
            let (seed, bus) = (c.seed, c.is_bus());
            let mut honk = false;
            let mut at_whom: Option<u64> = None;
            {
                let h = &mut self.cars[i].horn;
                let ready = now - h.last >= HORN_COOLDOWN;
                if sharp && !h.sharp && ready {
                    honk = true;
                }
                h.sharp = sharp;
                if let Some(who) = stuck {
                    h.stuck += dt;
                    // (the first honk once his patience has run out, then the repeats)
                    let due = if h.again == f32::MAX { h.stuck >= patience(seed, honk_factor, bus) } else { now >= h.again };
                    if due && ready {
                        honk = true;
                        at_whom = Some(who);
                    }
                } else {
                    h.stuck = 0.0;
                    h.again = f32::MAX;
                }
                if !honk && h.reply_at.is_some_and(|t| now >= t) {
                    honk = ready;
                }
                if h.reply_at.is_some_and(|t| now >= t) {
                    h.reply_at = None;
                }
                if honk {
                    h.last = now;
                    h.honks += 1;
                    if at_whom.is_some() {
                        h.again = now + repeat(seed, honk_factor, h.honks);
                    }
                }
            }
            if !honk {
                continue;
            }
            let _ = self.cars[i].vehicle.trigger("ev_AI_Horn");
            let heavy = self.cars[i].is_bus() || matches!(self.cars[i].state.veh_type, 2 | 3) || self.cars[i].state.length > 7.0;
            self.honked.push((self.cars[i].id, heavy));
            // (nobody asks without sound: the oldest go)
            if self.honked.len() > 64 {
                self.honked.remove(0);
            }
            if omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
                log::info!("t={:.1}: car {} honks{}", now, self.cars[i].id, if sharp { " (a sharp stop ahead)" } else if at_whom.is_some() { " (stuck behind a vehicle that just stands)" } else { " back" });
            }
            // the one honked at may answer, if something blocks him in turn
            let Some(j) = at_whom.and_then(|id| self.index_of.get(&id).copied()).filter(|&j| j < self.cars.len() && self.cars[j].id == at_whom.unwrap_or(0)) else { continue };
            let o = &self.cars[j];
            let blocked = o.state.speed < 0.3 && !o.why.0.is_empty() && !o.is_rail();
            let n = o.horn.honked_at as u64;
            let oseed = o.seed;
            let h = &mut self.cars[j].horn;
            h.honked_at += 1;
            if blocked && h.reply_at.is_none() && random::draw(oseed, 200 + n) < 0.4 {
                h.reply_at = Some(now + 0.5 + random::draw(oseed, 300 + n));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sharp_stop_ahead_is_closing_in_or_hard_braking_within_the_share() {
        // 12 m/s, braking at 3: braking distance 60 m, its share 18 m
        assert!(sharp_stop(12.0, 3.0, 0.0, 15.0, 0.0, -6.0));
        // following at an ordinary distance, the leader as fast: no honk
        assert!(!sharp_stop(12.0, 3.0, 0.0, 15.0, 12.0, 0.0));
        // far enough
        assert!(!sharp_stop(12.0, 3.0, 0.0, 25.0, 0.0, -6.0));
        // under 10 km/h nobody honks
        assert!(!sharp_stop(2.5, 3.0, -6.0, 3.0, 0.0, -6.0));
        // an emergency stop for the vehicle ahead
        assert!(sharp_stop(8.0, 3.0, -6.0, 20.0, 7.0, 0.0));
    }

    #[test]
    fn draws_are_the_drivers_own() {
        for s in 0..200u64 {
            let p = patience(s, 1.0, false);
            assert!((20.0..=45.0).contains(&p));
            assert!(patience(s, 1.0, true) > p);
            assert!((15.0..=30.0).contains(&repeat(s, 1.0, 3)));
        }
    }

    use super::super::scenario_tests::{add_car, street, traffic, Fixture};
    use crate::traffic::TrafficLightController;

    fn road(f: &Fixture) -> TrafficSim {
        let mut net = Network { lanes: vec![street(DVec3::ZERO, 0.0, 600.0)], ..Default::default() };
        net.link(1.5);
        traffic(f, net)
    }

    fn put(t: &mut TrafficSim, f: &Fixture, s: f32, seed: u64, v: f32) -> u64 {
        let id = add_car(t, f, 0, s, seed, Some(v));
        let c = t.cars.iter_mut().find(|c| c.id == id).unwrap();
        c.fresh = 0.0;
        c.state.speed = v;
        id
    }

    fn horn(t: &TrafficSim, id: u64) -> Horn {
        t.cars.iter().find(|c| c.id == id).unwrap().horn
    }

    fn stand(t: &mut TrafficSim, id: u64) {
        let c = t.cars.iter_mut().find(|c| c.id == id).unwrap();
        c.state.speed = 0.0;
        c.state.max_speed_kmh = 0.0;
    }

    /// Runs `secs`, holding `still` where they are; the times car `who` honked.
    fn honks(t: &mut TrafficSim, still: &[u64], who: u64, secs: f32) -> Vec<f32> {
        let mut out = Vec::new();
        let mut n = horn(t, who).honks;
        for _ in 0..(secs / super::super::fixed_step::SIM_DT) as usize {
            for &id in still {
                stand(t, id);
            }
            t.tick(super::super::fixed_step::SIM_DT, None);
            let h = horn(t, who);
            if h.honks != n {
                n = h.honks;
                out.push(t.time);
            }
        }
        out
    }

    #[test]
    fn a_sharp_stop_in_front_gives_one_honk() {
        let f = Fixture::new();
        let mut t = road(&f);
        let ahead = put(&mut t, &f, 120.0, 0x11, 12.0);
        let me = put(&mut t, &f, 100.0, 0x12, 12.0);
        t.tick(super::super::fixed_step::SIM_DT, None);
        // the car ahead stops dead
        let h = honks(&mut t, &[ahead], me, 8.0);
        assert_eq!(h.len(), 1, "honks at {h:?}");
    }

    #[test]
    fn following_at_an_ordinary_distance_gives_no_honk() {
        let f = Fixture::new();
        let mut t = road(&f);
        put(&mut t, &f, 130.0, 0x13, 12.0);
        let me = put(&mut t, &f, 100.0, 0x14, 12.0);
        assert!(honks(&mut t, &[], me, 20.0).is_empty());
    }

    #[test]
    fn stuck_behind_a_car_that_just_stands_he_honks_after_his_patience_and_again() {
        let f = Fixture::new();
        let mut t = road(&f);
        let blocker = put(&mut t, &f, 100.0, 0x15, 0.0);
        let me = put(&mut t, &f, 80.0, 0x16, 5.0);
        let h = honks(&mut t, &[blocker], me, 110.0);
        let seed = t.cars.iter().find(|c| c.id == me).unwrap().seed;
        assert!(h.len() >= 2, "honks at {h:?}");
        assert!(h[0] >= patience(seed, 1.0, false) && h[0] < patience(seed, 1.0, false) + 10.0, "first honk at {:.1} s", h[0]);
        let gap = h[1] - h[0];
        assert!((15.0..=30.1).contains(&gap), "again after {gap:.1} s");
    }

    #[test]
    fn a_queue_at_a_red_light_does_not_honk() {
        let f = Fixture::new();
        let mut lanes = vec![street(DVec3::ZERO, 0.0, 150.0), street(DVec3::new(0.0, 150.0, 0.0), 0.0, 300.0)];
        lanes[1].traffic_light = Some((0, 0));
        let mut net = Network { lanes, ..Default::default() };
        net.link(1.5);
        net.build_grid();
        net.compute_reach();
        let random = super::super::setup::RandomTypes { types: Vec::new(), groups: Vec::new(), group_curves: false, group_uvg: Vec::new(), uvg_defaults: Vec::new() };
        let red = TrafficLightController::new(vec![vec![(0, 1000.0)]], 0.0);
        let mut t = TrafficSim::assemble(&f.dir, net, random, vec![red], HashMap::new(), (Vec::new(), Vec::new()), Vec::new(), (1.0, 0), 0);
        let ids: Vec<u64> = (0..4).map(|k| put(&mut t, &f, 130.0 - 9.0 * k as f32, 0x20 + k, 6.0)).collect();
        for _ in 0..(120.0 / super::super::fixed_step::SIM_DT) as usize {
            t.tick(super::super::fixed_step::SIM_DT, None);
        }
        for id in ids {
            assert_eq!(horn(&t, id).honks, 0, "car {id} honked in the queue at red");
        }
    }

    #[test]
    fn only_a_blocker_that_is_blocked_itself_honks_back() {
        // alone in front: honked at many times, never answers
        let f = Fixture::new();
        for seed in 0..6u64 {
            let mut t = road(&f);
            let blocker = put(&mut t, &f, 100.0, 0x30 + seed * 7, 0.0);
            let me = put(&mut t, &f, 80.0, 0x31 + seed * 7, 3.0);
            honks(&mut t, &[blocker], me, 150.0);
            assert!(horn(&t, me).honks >= 3);
            let b = horn(&t, blocker);
            assert!(b.honked_at >= 3 && b.honks == 0, "the blocker answered: {b:?}");
        }
        // standing behind another that just stands: sometimes answers
        let mut answered = 0;
        for seed in 0..12u64 {
            let mut t = road(&f);
            let head = put(&mut t, &f, 120.0, 0x50 + seed * 11, 0.0);
            let middle = put(&mut t, &f, 112.0, 0x51 + seed * 11, 0.0);
            let me = put(&mut t, &f, 90.0, 0x52 + seed * 11, 3.0);
            honks(&mut t, &[head], me, 150.0);
            let (m, mine) = (horn(&t, middle), horn(&t, me));
            assert!(m.honked_at >= 1 && mine.honks >= 1);
            // (its own honks at the head, its patience run out, are counted with them)
            let own = honks_alone(&f, seed);
            if m.honks > own {
                answered += 1;
            }
        }
        assert!(answered > 0, "nobody ever honked back");
    }

    /// How often the middle car of `only_a_blocker_that_is_blocked_itself_honks_back`
    /// honks with nobody behind it (its own honks at the car ahead).
    fn honks_alone(f: &Fixture, seed: u64) -> u32 {
        let mut t = road(f);
        let head = put(&mut t, f, 120.0, 0x50 + seed * 11, 0.0);
        let middle = put(&mut t, f, 112.0, 0x51 + seed * 11, 0.0);
        honks(&mut t, &[head], middle, 150.0);
        horn(&t, middle).honks
    }
}
