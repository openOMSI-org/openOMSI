//! Lightning: when a thunderstorm stands over the place, flashes come at the rate its
//! activity gives, each a few return strokes of a tens of milliseconds a hundred milliseconds
//! or less apart (the flicker of a real flash), lighting the clouds from inside and the
//! street with a cold white light for as long as they last. The thunder follows by the
//! speed of sound.
//!
//! Numbers: an active storm cell makes some ten flashes a minute within sight, a third of
//! them to the ground (the rest between and inside the clouds, which light the cloud and
//! the street but show no channel); a flash has two to four strokes, the first the
//! brightest, 40 to 80 ms apart, each decaying over about 20 ms on a continuing glow of a
//! tenth, which fades over 150 ms. A stroke a kilometre away lights the ground with some
//! 20 lux (the units here: 1 = 10 000 lux, `FLASH_E1KM`), falling as the inverse square.

/// Illuminance of a first stroke at a kilometre (1 = 10 000 lux).
pub(crate) const FLASH_E1KM: f32 = 2.0e-3;
/// The speed of sound (m/s).
pub(crate) const SOUND_SPEED: f32 = 343.0;
/// Flashes a minute within sight of the strongest storm.
const FLASHES_PER_MINUTE: f32 = 10.0;

/// What one frame sees of the lightning.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Flash {
    /// Illuminance this frame (1 = 10 000 lux, on a surface facing the flash).
    pub(crate) light: f32,
    /// Towards the flash: horizontal unit vector, and its elevation over the horizon (rad).
    pub(crate) dir: [f32; 2],
    pub(crate) elevation: f32,
    /// A number for this flash's channel (its shape is drawn from it), and whether the
    /// flash has one (a cloud-to-ground stroke: the others light the cloud alone).
    pub(crate) seed: f32,
    pub(crate) channel: f32,
    /// How bright the channel is now (0..1).
    pub(crate) bolt: f32,
}

/// A strike for the sound: seconds until the thunder is heard, and the distance (m).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Strike {
    pub(crate) delay: f32,
    pub(crate) distance: f32,
    /// Towards the flash: horizontal unit vector (where the thunder comes from).
    pub(crate) dir: [f32; 2],
}

/// `OMSI_STORM=activity[,azimuth]`: the forced storm activity, and the azimuth (degrees) a
/// flash is frozen at.
pub(crate) fn forced_storm() -> Option<(f32, Option<f32>)> {
    omsi_cfg::flags::OMSI_STORM.var().and_then(|v| {
        let mut it = v.split(',').map(|x| x.trim().parse::<f32>().ok());
        Some((it.next()??, it.next().flatten()))
    })
}

pub(crate) struct Lightning {
    seeded: Option<u32>,
    rng: u64,
    /// Seconds to the next flash.
    wait: f32,
    /// Seconds since the flash began (negative: none under way).
    age: f32,
    strokes: [(f32, f32); 5],
    stroke_count: usize,
    dir: [f32; 2],
    elevation: f32,
    distance: f32,
    seed: f32,
    channel: bool,
    /// Strikes not yet taken by the sound.
    pub(crate) strikes: Vec<Strike>,
}

impl Lightning {
    pub(crate) fn new(seed: u64) -> Lightning {
        let mut l = Lightning {
            seeded: None,
            rng: seed | 1,
            wait: 5.0,
            age: -1.0,
            strokes: [(0.0, 0.0); 5],
            stroke_count: 0,
            dir: [0.0, 1.0],
            elevation: 0.2,
            distance: 3000.0,
            seed: 0.0,
            channel: false,
            strikes: Vec::new(),
        };
        l.wait = l.next_wait(0.5);
        l
    }

    /// Take the day's seed (again when the day or the session's changes): the flashes of a
    /// place and day come in the same order in every replay of it (when the frames are the
    /// same: each client steps them by its own frame times).
    pub(crate) fn seed_once(&mut self, day_seed: u32) {
        if self.seeded != Some(day_seed) {
            self.seeded = Some(day_seed);
            self.rng = (u64::from(day_seed) ^ 0x9e37_79b9_7f4a_7c15).wrapping_mul(0x2545_F491_4F6C_DD1D) | 1;
            self.wait = self.next_wait(0.5);
        }
    }

    fn rand(&mut self) -> f32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        ((self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32) / 16_777_216.0
    }

    fn next_wait(&mut self, storm: f32) -> f32 {
        // exponential waiting time at the storm's rate (activity to the 1.5 power: a weak
        // cell flashes rarely)
        let rate = FLASHES_PER_MINUTE / 60.0 * storm.max(0.05).powf(1.5);
        -(1.0 - self.rand()).max(1e-4).ln() / rate
    }

    fn start_flash(&mut self) {
        self.stroke_count = 2 + (self.rand() * 3.0) as usize;
        let mut t = 0.0;
        for k in 0..self.stroke_count {
            let a = if k == 0 { 1.0 } else { 0.35 + 0.5 * self.rand() };
            self.strokes[k] = (t, a);
            t += 0.04 + 0.04 * self.rand();
        }
        let az = self.rand() * std::f32::consts::TAU;
        self.dir = [az.cos(), az.sin()];
        // storms are seen from 1 to 12 km, mostly the middle distances
        let u = self.rand();
        self.distance = 900.0 + 11_000.0 * u * u;
        // the flash's height as seen: its channel from the cloud base, about 2 km, down; the
        // flash itself at the cloud's middle
        self.elevation = (1500.0f32 / self.distance).atan().clamp(0.08, 0.9);
        self.seed = self.rand() * 100.0;
        self.channel = self.rand() < 0.4;
        self.age = 0.0;
        self.strikes.push(Strike { delay: self.distance / SOUND_SPEED, distance: self.distance, dir: self.dir });
    }

    /// A flash standing still at its first stroke, for checks (`OMSI_STORM=activity,azimuth`):
    /// 1.2 km away over the azimuth (degrees, 0 = east, 90 = north), with its channel.
    pub(crate) fn frozen(seed: f32) -> Flash {
        let az = seed.to_radians();
        let km = 1.2f32;
        Flash {
            light: FLASH_E1KM / (km * km),
            dir: [az.cos(), az.sin()],
            elevation: (1500.0f32 / (km * 1000.0)).atan(),
            seed,
            channel: 1.0,
            bolt: 1.0,
        }
    }

    /// Carry the lightning on by `dt` seconds at storm activity `storm` (0..1) and give the
    /// flash this frame shows. `night` is the share of the dark (the sky's brightness is
    /// the exposure's business, not the flash's: only the flash itself is drawn).
    pub(crate) fn step(&mut self, storm: f32, dt: f32) -> Flash {
        if storm < 0.12 {
            self.age = -1.0;
            self.wait = self.wait.max(2.0);
            return Flash::default();
        }
        if self.age < 0.0 {
            self.wait -= dt;
            if self.wait <= 0.0 {
                self.start_flash();
                self.wait = self.next_wait(storm);
            }
        } else {
            self.age += dt;
        }
        if self.age < 0.0 {
            return Flash::default();
        }
        // the strokes' light: each decays in about 20 ms on top of a continuing glow
        let mut light = 0.0;
        let mut bolt = 0.0;
        for k in 0..self.stroke_count {
            let (t0, a) = self.strokes[k];
            let t = self.age - t0;
            if t >= 0.0 {
                light += a * (-t / 0.02).exp() + a * 0.12 * (-t / 0.15).exp();
                bolt += a * (-t / 0.03).exp();
            }
        }
        if self.age > 0.8 {
            self.age = -1.0;
            return Flash::default();
        }
        let km = (self.distance / 1000.0).max(0.3);
        Flash {
            light: light * FLASH_E1KM / (km * km),
            dir: self.dir,
            elevation: self.elevation,
            seed: self.seed,
            channel: if self.channel { 1.0 } else { 0.0 },
            bolt: bolt.min(1.0) * (if self.channel { 1.0 } else { 0.0 }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(seed: u64, storm: f32, minutes: f32) -> (usize, Vec<Strike>, f32) {
        let mut l = Lightning::new(seed);
        let (mut flashes, mut was, mut peak) = (0, false, 0.0f32);
        for _ in 0..(minutes * 60.0 * 60.0) as usize {
            let f = l.step(storm, 1.0 / 60.0);
            peak = peak.max(f.light);
            if f.light > 0.0 && !was {
                flashes += 1;
            }
            was = f.light > 0.0;
        }
        (flashes, std::mem::take(&mut l.strikes), peak)
    }

    #[test]
    fn no_storm_no_flash() {
        let (flashes, strikes, peak) = run(3, 0.05, 10.0);
        assert_eq!((flashes, strikes.len(), peak), (0, 0, 0.0));
    }

    #[test]
    fn a_strong_storm_flashes_at_its_rate_and_the_sound_follows() {
        let (flashes, strikes, peak) = run(11, 0.9, 10.0);
        // some ten a minute at full activity, a little fewer at 0.9 (activity^1.5)
        assert!((50..140).contains(&flashes), "{flashes}");
        assert_eq!(strikes.len(), flashes);
        for s in &strikes {
            assert!(s.distance >= 900.0 && s.distance <= 12_000.0, "{s:?}");
            assert!((s.delay - s.distance / SOUND_SPEED).abs() < 1e-3);
            assert!((s.dir[0].hypot(s.dir[1]) - 1.0).abs() < 1e-4);
        }
        // the nearest first stroke lights the ground well, the farthest hardly at all
        assert!(peak > FLASH_E1KM / 16.0 && peak <= FLASH_E1KM / 0.7, "{peak}");
    }

    #[test]
    fn the_day_seed_repeats_the_storm() {
        let mut a = Lightning::new(1);
        let mut b = Lightning::new(99);
        a.seed_once(4242);
        b.seed_once(4242);
        for _ in 0..20_000 {
            let (x, y) = (a.step(0.8, 1.0 / 60.0), b.step(0.8, 1.0 / 60.0));
            assert_eq!((x.light, x.dir, x.seed), (y.light, y.dir, y.seed));
        }
        // (the same seed again does not turn it; another one does)
        a.seed_once(4242);
        for _ in 0..2_000 {
            let (x, y) = (a.step(0.8, 1.0 / 60.0), b.step(0.8, 1.0 / 60.0));
            assert_eq!((x.light, x.dir, x.seed), (y.light, y.dir, y.seed));
        }
        a.seed_once(7);
        let differs = (0..60_000).any(|_| {
            let (x, y) = (a.step(0.8, 1.0 / 60.0), b.step(0.8, 1.0 / 60.0));
            (x.light, x.dir, x.seed) != (y.light, y.dir, y.seed)
        });
        assert!(differs);
    }

    #[test]
    fn a_flash_ends() {
        let mut l = Lightning::new(5);
        let mut lit = 0;
        for _ in 0..(120 * 60) {
            if l.step(1.0, 1.0 / 60.0).light > 0.0 {
                lit += 1;
            }
        }
        // flashes of under a second, never a steady light
        assert!(lit > 0 && lit < 120 * 60 / 2, "{lit}");
    }

    #[test]
    fn the_frozen_flash_looks_where_asked() {
        let f = Lightning::frozen(90.0);
        assert!(f.dir[0].abs() < 1e-6 && (f.dir[1] - 1.0).abs() < 1e-6);
        assert!(f.light > 0.0 && f.channel == 1.0);
    }
}
