//! A client's copy of the host's world, the parts that need no game: the poses of a thing
//! as they came (`Track`), the moment they are drawn at (`PlayClock`), the files named
//! relative to the content roots, and the activities on the wire and in the simulation.

use omsi_net::world::Activity as NetActivity;
use omsi_sim::human::Activity;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// How long a client guesses on past the last frame of something moving (ms).
pub const MAX_EXTRAPOLATE: f64 = 300.0;

/// One pose of a thing as a client got it, on the host's clock.
#[derive(Clone, Copy)]
pub struct Sample<T> {
    pub ms: f64,
    pub v: T,
}

pub struct Track<T> {
    pub samples: Vec<Sample<T>>,
    pub heard: Instant,
}

impl<T: Copy> Track<T> {
    pub fn push(&mut self, ms: f64, v: T) {
        if self.samples.last().map(|s| ms <= s.ms).unwrap_or(false) {
            return;
        }
        self.samples.push(Sample { ms, v });
        if self.samples.len() > 12 {
            self.samples.remove(0);
        }
        self.heard = Instant::now();
    }

    /// The samples around `ms` and how far between them it lies (0..1, more when guessing
    /// on past the last).
    pub fn around(&self, ms: f64) -> Option<(&T, &T, f64)> {
        let s = &self.samples;
        let last = s.last()?;
        if s.len() == 1 || ms >= last.ms {
            let prev = if s.len() > 1 { &s[s.len() - 2] } else { last };
            let span = (last.ms - prev.ms).max(1.0);
            let t = if s.len() > 1 {
                1.0 + ((ms - last.ms).min(MAX_EXTRAPOLATE)).max(0.0) / span
            } else {
                1.0
            };
            return Some((&prev.v, &last.v, t));
        }
        if ms <= s[0].ms {
            return Some((&s[0].v, &s[0].v, 0.0));
        }
        let k = s.iter().position(|x| x.ms > ms)?;
        let (a, b) = (&s[k - 1], &s[k]);
        Some((&a.v, &b.v, (ms - a.ms) / (b.ms - a.ms).max(1.0)))
    }
}

/// The moment the others' things are drawn at, on their clock: it runs on with ours and is
/// pulled towards where it should be (now, less the delay, on their clock) by at most a
/// few per cent of its pace. Set anew each frame from the offset of the clocks and the
/// delay, it jumped with them - a datagram that came quicker than the others, a delay
/// grown after a slow frame of theirs - and everything drawn went back or on a bit at
/// once: the micro-teleports.
#[derive(Default, Clone, Copy)]
pub struct PlayClock {
    /// (the moment drawn, our clock then)
    at: Option<(f64, f64)>,
}

impl PlayClock {
    /// The moment to draw at `now` (our clock) when it should be `want`; more than `snap`
    /// off (a new session, a pause), it is taken at once.
    pub fn step(&mut self, now: f64, want: f64, snap: f64) -> f64 {
        let t = match self.at {
            Some((t, then)) => {
                let d = (now - then).max(0.0);
                let run = t + d;
                let err = want - run;
                if err.abs() > snap {
                    want
                } else {
                    run + err.clamp(-0.06 * d, 0.06 * d)
                }
            }
            None => want,
        };
        self.at = Some((t, now));
        t
    }
}

pub fn lerp_angle(a: f64, b: f64, t: f64) -> f64 {
    let d = (b - a + 540.0).rem_euclid(360.0) - 180.0;
    (a + d * t).rem_euclid(360.0)
}

pub fn quant(x: f64, y: f64, z: f64, h: f64) -> [i64; 4] {
    [
        (x * 20.0) as i64,
        (y * 20.0) as i64,
        (z * 20.0) as i64,
        (h * 4.0) as i64,
    ]
}

pub fn fnv(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// A file path as it is written for the other games: relative to its content root.
pub fn relative_file(path: &Path, root: &Path) -> String {
    let mut roots = omsi_cfg::content_roots();
    roots.push(root.to_path_buf());
    relative_to_roots(path, &roots).unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"))
}

/// `path` relative to the root of `roots` it lies in, the deepest one: an OMSI 2 folder
/// inside the content folder (`openOMSI/OMSI 2`, as a server is often laid out) names its
/// files `Vehicles/...` as the other games find them, not `OMSI 2/Vehicles/...`, which no
/// other game has (a Linux server's AI cars were left out on every client, #1097).
pub fn relative_to_roots(path: &Path, roots: &[PathBuf]) -> Option<String> {
    let r = roots
        .iter()
        .filter(|r| path.starts_with(r))
        .max_by_key(|r| r.components().count())?;
    path.strip_prefix(r)
        .ok()
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
}

/// A content-relative file from the host as a file here, when it exists here.
pub fn local_file(root: &Path, rel: &str) -> Option<PathBuf> {
    let path = omsi_cfg::resolve_path(root, rel);
    if omsi_cfg::vfs::is_file(&path) {
        return Some(path);
    }
    // (a host that named it under a folder of its own, "OMSI 2/Vehicles/...": the same
    // vehicle under any content root here, as a remote player's bus is looked for)
    let k = rel
        .to_ascii_lowercase()
        .replace('\\', "/")
        .find("vehicles/")?;
    omsi_cfg::find_in_roots(&rel.replace('\\', "/")[k..])
        .map(|(_, p)| p)
        .filter(|p| omsi_cfg::vfs::is_file(p))
}

pub fn net_activity(a: Activity) -> NetActivity {
    match a {
        Activity::Walk => NetActivity::Walk,
        Activity::Sit => NetActivity::Sit,
        _ => NetActivity::Stand,
    }
}

pub fn sim_activity(a: NetActivity) -> Activity {
    match a {
        NetActivity::Walk | NetActivity::Run => Activity::Walk,
        NetActivity::Sit => Activity::Sit,
        NetActivity::Stand => Activity::Stand,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_named_relative_to_the_deepest_root_holding_it() {
        // the content folder first, the OMSI 2 folder inside it after (a server's layout)
        let roots = [
            PathBuf::from("/srv/openOMSI"),
            PathBuf::from("/srv/openOMSI/OMSI 2"),
        ];
        let golf = Path::new("/srv/openOMSI/OMSI 2/Vehicles/VW_Golf_2/ai_vw_golf_2.bus");
        assert_eq!(
            relative_to_roots(golf, &roots).as_deref(),
            Some("Vehicles/VW_Golf_2/ai_vw_golf_2.bus")
        );
        // a mod in the content folder itself stays relative to that
        let m = Path::new("/srv/openOMSI/Vehicles/Mod/mod.bus");
        assert_eq!(
            relative_to_roots(m, &roots).as_deref(),
            Some("Vehicles/Mod/mod.bus")
        );
        assert_eq!(
            relative_to_roots(Path::new("/elsewhere/x.bus"), &roots),
            None
        );
    }

    #[test]
    fn the_moment_drawn_follows_a_changed_delay_smoothly() {
        let mut c = PlayClock::default();
        let mut now = 0.0;
        let mut t = c.step(now, now - 120.0, 500.0);
        // a slow frame of theirs: the delay grows by 70 ms at once
        let mut steps = Vec::new();
        for _ in 0..100 {
            now += 16.0;
            let n = c.step(now, now - 190.0, 500.0);
            steps.push(n - t);
            t = n;
        }
        // never back, never on more than a few per cent faster or slower than the clock
        assert!(
            steps.iter().all(|d| *d > 16.0 * 0.93 && *d < 16.0 * 1.07),
            "{steps:?}"
        );
        assert!(
            (t - (now - 190.0)).abs() < 1e-6,
            "caught up: {}",
            t - (now - 190.0)
        );
        // a new session: taken at once
        now += 16.0;
        assert_eq!(c.step(now, now + 5000.0, 500.0), now + 5000.0);
    }
}
