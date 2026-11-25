//! Random traffic incidents (#2745): a crash, a breakdown or a closed lane that appears while
//! the player drives, holds the traffic up for a few minutes and clears again - the road
//! events of ETS 2, as the issue asks for, with a frequency the player sets.
//!
//! This is the **decision** half of that feature: when an incident happens, where on the road
//! it is, how long it lasts and when it goes away. It is deterministic - the same seed and the
//! same calls give the same incidents - because a map's traffic has to be reproducible in a
//! replay and, in multiplayer, on every machine.
//!
//! What it deliberately does not do is *place* anything: no mesh, no lane blocked, no AI
//! braking. Those need the map's objects and the AI traffic's own idea of a blocked path; the
//! issue asks for a design first (milestone v0.7), and this is the part of that design that can
//! be tested on its own. The caller hands in the candidate places (the stops, junctions or
//! segment midpoints it has) and gets back the incidents it should draw and block.

use glam::DVec3;

/// How often incidents happen, as the options page words it: Off / Low / Medium / High.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Frequency {
    #[default]
    Off,
    Low,
    Medium,
    High,
}

impl Frequency {
    /// An incident every `n` hours, on average, for one road network.
    pub fn per_hour(self) -> f64 {
        match self {
            Frequency::Off => 0.0,
            Frequency::Low => 0.25,
            Frequency::Medium => 1.0,
            Frequency::High => 3.0,
        }
    }

    /// The most incidents that can be active at once: a road network with four crashes on it is
    /// a puzzle, not a traffic jam.
    pub fn most_at_once(self) -> usize {
        match self {
            Frequency::Off => 0,
            Frequency::Low => 1,
            Frequency::Medium => 2,
            Frequency::High => 4,
        }
    }

    /// `off`/`low`/`medium`/`high` (also `0`/`1`/`2`/`3`, as `settings.cfg` writes them), as
    /// the launcher and the options page write it.
    pub fn parse(s: &str) -> Frequency {
        match s.trim().to_ascii_lowercase().as_str() {
            "1" | "low" => Frequency::Low,
            "2" | "medium" => Frequency::Medium,
            "3" | "high" => Frequency::High,
            _ => Frequency::Off,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Frequency::Off => "off",
            Frequency::Low => "low",
            Frequency::Medium => "medium",
            Frequency::High => "high",
        }
    }
}

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Two vehicles in each other's way: the lane is blocked for a while.
    Accident,
    /// A broken-down bus or lorry beside the kerb.
    Breakdown,
    /// A lane closed with cones - the longest of the three.
    Roadworks,
}

impl Kind {
    /// How long it keeps the road as it was found (seconds), for a roll `0..1`: an accident
    /// clears in minutes, roadworks take longer, a breakdown is somewhere between.
    pub fn duration(self, roll: f64) -> f64 {
        let roll = roll.clamp(0.0, 1.0);
        match self {
            Kind::Accident => 120.0 + 300.0 * roll,
            Kind::Breakdown => 90.0 + 210.0 * roll,
            Kind::Roadworks => 600.0 + 1200.0 * roll,
        }
    }
}

/// One incident: where it is, what it is, and until when.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Incident {
    pub id: u64,
    pub kind: Kind,
    pub at: DVec3,
    /// When it appeared and when it clears, in seconds of the game's clock.
    pub from: f64,
    pub until: f64,
}

impl Incident {
    pub fn active_at(&self, now: f64) -> bool {
        now >= self.from && now < self.until
    }
}

/// xorshift64*: the incidents are drawn from a seed of the player's choosing (a map's, a
/// session's), so a run can be repeated exactly.
#[derive(Debug, Clone, Copy)]
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// `0..1`
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The incidents of a running game: when the next is due, the ones on the road now.
#[derive(Debug, Clone)]
pub struct Scheduler {
    frequency: Frequency,
    rng: Rng,
    next_id: u64,
    /// When the next incident is due (`None`: never, with the frequency off). Always kept in
    /// the future when there is one, so `step` is a comparison and not a loop.
    due_at: Option<f64>,
    active: Vec<Incident>,
}

impl Scheduler {
    pub fn new(frequency: Frequency, seed: u64) -> Scheduler {
        // (a zero seed would keep xorshift at zero forever)
        let mut s = Scheduler {
            frequency,
            rng: Rng(seed | 1),
            next_id: 1,
            due_at: None,
            active: Vec::new(),
        };
        s.due_at = s.draw_gap(0.0);
        s
    }

    pub fn frequency(&self) -> Frequency {
        self.frequency
    }

    /// Change the frequency while driving (the options page does): Off clears what is on the
    /// road, since a player who turns incidents off means the ones in front of them too.
    pub fn set_frequency(&mut self, frequency: Frequency, now: f64) {
        if frequency == self.frequency {
            return;
        }
        self.frequency = frequency;
        self.active.clear();
        self.due_at = if frequency == Frequency::Off {
            None
        } else {
            self.draw_gap(now)
        };
    }

    pub fn active(&self) -> &[Incident] {
        &self.active
    }

    /// The incidents on the road at `now`.
    pub fn at(&self, now: f64) -> Vec<Incident> {
        self.active
            .iter()
            .copied()
            .filter(|i| i.active_at(now))
            .collect()
    }

    /// How long until the next incident, if one is due at all.
    pub fn next_in(&self, now: f64) -> Option<f64> {
        self.due_at.map(|d| (d - now).max(0.0))
    }

    /// One tick of the game's clock: clear the incidents that are over, and make a new one if
    /// one is due, at one of `places` (the caller's candidates: junctions, segment midpoints,
    /// the stops of the map - anywhere a vehicle can stop and be in the way).
    ///
    /// `places` empty means there is nowhere to put an incident: it is dropped and the next one
    /// is drawn as usual, so a map without candidates never accumulates a backlog.
    pub fn step(&mut self, now: f64, places: &[DVec3]) {
        self.active.retain(|i| now < i.until);
        let Some(due) = self.due_at else { return };
        if now < due {
            return;
        }
        if self.active.len() < self.frequency.most_at_once() && !places.is_empty() {
            let kind = match self.rng.next_f64() {
                r if r < 0.5 => Kind::Accident,
                r if r < 0.8 => Kind::Breakdown,
                _ => Kind::Roadworks,
            };
            let at = places[(self.rng.next_u64() % places.len() as u64) as usize];
            let until = now + kind.duration(self.rng.next_f64());
            self.active.push(Incident {
                id: self.next_id,
                kind,
                at,
                from: now,
                until,
            });
            self.next_id += 1;
        }
        self.due_at = self.draw_gap(now);
    }

    /// The wait after `now`, drawn from an exponential distribution of `Frequency::per_hour`:
    /// incidents come as a Poisson process, so a player who has waited long is not "owed" one.
    fn draw_gap(&mut self, now: f64) -> Option<f64> {
        let rate = self.frequency.per_hour();
        if rate <= 0.0 {
            return None;
        }
        // (one draw at least: `u == 0` would be an infinite wait)
        let u = self.rng.next_f64().max(1e-12);
        Some(now + -u.ln() / rate * 3600.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn places() -> Vec<DVec3> {
        vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(100.0, 0.0, 0.0),
            DVec3::new(0.0, 100.0, 0.0),
        ]
    }

    /// Run `hours` of game clock in one-minute steps, `places` handed in every tick.
    fn run(freq: Frequency, seed: u64, hours: f64) -> (usize, Scheduler) {
        let mut s = Scheduler::new(freq, seed);
        let p = places();
        let mut seen: Vec<u64> = Vec::new();
        let mut now = 0.0;
        while now < hours * 3600.0 {
            s.step(now, &p);
            for i in s.active() {
                if !seen.contains(&i.id) {
                    seen.push(i.id);
                }
            }
            now += 60.0;
        }
        (seen.len(), s)
    }

    #[test]
    fn off_means_no_incidents_ever() {
        let (n, s) = run(Frequency::Off, 7, 500.0);
        assert_eq!(n, 0);
        assert!(s.next_in(0.0).is_none(), "nothing is ever due");
    }

    #[test]
    fn the_frequency_sets_how_many_happen() {
        // (500 hours: the mean is 125 at Low, 500 at Medium, 1500 at High - a wide band each
        // way, since this is a random process)
        let (low, _) = run(Frequency::Low, 1, 500.0);
        let (medium, _) = run(Frequency::Medium, 2, 500.0);
        let (high, _) = run(Frequency::High, 3, 500.0);
        assert!((60..200).contains(&low), "low: {low}");
        assert!((350..650).contains(&medium), "medium: {medium}");
        assert!((1100..1900).contains(&high), "high: {high}");
    }

    #[test]
    fn the_same_seed_gives_the_same_road() {
        let a: Vec<Incident> = {
            let mut s = Scheduler::new(Frequency::High, 42);
            let p = places();
            (0..200).for_each(|k| s.step(k as f64 * 60.0, &p));
            s.active().to_vec()
        };
        let b: Vec<Incident> = {
            let mut s = Scheduler::new(Frequency::High, 42);
            let p = places();
            (0..200).for_each(|k| s.step(k as f64 * 60.0, &p));
            s.active().to_vec()
        };
        assert_eq!(a, b, "a replay must see the same incidents");
    }

    #[test]
    fn an_incident_is_never_in_two_places_too_many() {
        let mut s = Scheduler::new(Frequency::Low, 11);
        let p = places();
        let mut now = 0.0;
        let mut most = 0;
        while now < 2000.0 * 3600.0 {
            s.step(now, &p);
            most = most.max(s.active().len());
            now += 60.0;
        }
        assert_eq!(most, 1, "Low keeps one on the road at a time");
    }

    #[test]
    fn an_incident_clears_when_its_time_is_up() {
        let mut s = Scheduler::new(Frequency::High, 5);
        let p = places();
        let mut now = 0.0;
        let mut found: Option<Incident> = None;
        while found.is_none() && now < 100.0 * 3600.0 {
            s.step(now, &p);
            found = s.active().first().copied();
            now += 60.0;
        }
        let i = found.expect("an incident in a hundred hours at High");
        assert!(i.until > i.from && i.until - i.from >= 90.0, "{i:?}");
        assert!(
            i.active_at(i.from) && !i.active_at(i.until),
            "over when its time is up"
        );
        // (a step past its end takes it off the road)
        let mut s = Scheduler::new(Frequency::High, i.id as u64 + 1);
        s.active.push(i);
        s.step(i.until, &places());
        assert!(!s.active().contains(&i), "cleared at its end");
    }

    #[test]
    fn nowhere_to_put_one_is_no_incident_and_no_backlog() {
        let mut s = Scheduler::new(Frequency::High, 3);
        let mut now = 0.0;
        while now < 500.0 * 3600.0 {
            s.step(now, &[]);
            now += 60.0;
        }
        assert!(s.active().is_empty(), "an empty map has no incidents");
        assert!(
            s.next_in(now - 60.0).is_some_and(|d| d <= 24.0 * 3600.0),
            "the next one is still a normal wait away, not a jump"
        );
    }

    #[test]
    fn turning_incidents_off_clears_the_road() {
        let mut s = Scheduler::new(Frequency::High, 9);
        let p = places();
        let mut now = 0.0;
        while s.active().is_empty() && now < 100.0 * 3600.0 {
            s.step(now, &p);
            now += 60.0;
        }
        assert!(!s.active().is_empty());
        s.set_frequency(Frequency::Off, now);
        assert!(s.active().is_empty(), "one in front of the player goes too");
        assert!(s.next_in(now).is_none());
    }

    #[test]
    fn the_options_page_s_words_are_read_back() {
        assert_eq!(Frequency::parse("Off"), Frequency::Off);
        assert_eq!(Frequency::parse(" high "), Frequency::High);
        assert_eq!(Frequency::parse("2"), Frequency::Medium);
        assert_eq!(
            Frequency::parse("nonsense"),
            Frequency::Off,
            "what is not understood is off"
        );
        for f in [
            Frequency::Off,
            Frequency::Low,
            Frequency::Medium,
            Frequency::High,
        ] {
            assert_eq!(Frequency::parse(f.as_str()), f, "round trip");
        }
    }

    #[test]
    fn roadworks_last_longer_than_a_breakdown() {
        for roll in [0.0, 0.5, 1.0] {
            assert!(Kind::Roadworks.duration(roll) > Kind::Breakdown.duration(roll));
            assert!(Kind::Breakdown.duration(roll) > 0.0);
            assert!(Kind::Accident.duration(roll) <= Kind::Roadworks.duration(roll));
        }
        assert_eq!(
            Kind::Accident.duration(-1.0),
            Kind::Accident.duration(0.0),
            "a roll out of range is clamped"
        );
    }
}
