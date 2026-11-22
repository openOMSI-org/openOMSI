//! Performance history and readouts shared by interface views.
#[cfg(test)]
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::time::Instant;

pub const HISTORY: usize = 240;
pub const RECENT: usize = 30;
const REFRESH_S: f32 = 0.25;
const CPU_S: f32 = 0.5;
const STUTTER_S: f32 = 0.05;
const STUTTER_RATIO: f32 = 2.5;
const WARM_UP: u32 = 60;
const MONITOR_S: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Simulation,
    Loading,
    Scenery,
    Interface,
    Drawing,
    Gpu,
    Limit,
    Other,
}

impl Part {
    pub const ALL: [Part; 8] = [
        Part::Simulation,
        Part::Loading,
        Part::Scenery,
        Part::Interface,
        Part::Drawing,
        Part::Gpu,
        Part::Limit,
        Part::Other,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Part::Simulation => "Simulation",
            Part::Loading => "Map loading",
            Part::Scenery => "Lights and scenery",
            Part::Interface => "HUD and menus",
            Part::Drawing => "Drawing",
            Part::Gpu => "Graphics card",
            Part::Limit => "Frame limit",
            Part::Other => "Rest of the frame",
        }
    }

    pub fn color(self) -> [u8; 4] {
        match self {
            Part::Simulation => [86, 156, 255, 255],
            Part::Loading => [255, 176, 64, 255],
            Part::Scenery => [178, 120, 255, 255],
            Part::Interface => [64, 210, 200, 255],
            Part::Drawing => [92, 214, 110, 255],
            Part::Gpu => [255, 92, 92, 255],
            Part::Limit => [120, 120, 120, 255],
            Part::Other => [200, 200, 200, 255],
        }
    }
}

pub fn part_of(key: &str) -> Option<Part> {
    Some(match key {
        k if k.contains('.') => return None,
        "streaming" | "placement" => Part::Loading,
        "lights+rain" | "scripted" | "lighting" => Part::Scenery,
        "hud" | "photo" | "shell" => Part::Interface,
        "render" | "mirrors" => Part::Drawing,
        "acquire" | "present" | "gpu" => Part::Gpu,
        "limiter" => Part::Limit,
        _ => Part::Simulation,
    })
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Sample {
    pub total: f32,
    pub parts: [f32; 8],
}

impl Sample {
    fn new(total: f32, mut parts: [f32; 8]) -> Sample {
        let staged: f32 = parts.iter().sum();
        parts[Part::Other as usize] = (total - staged).max(0.0);
        Sample { total, parts }
    }

    fn largest(&self) -> Part {
        Part::ALL
            .into_iter()
            .max_by(|a, b| self.parts[*a as usize].total_cmp(&self.parts[*b as usize]))
            .unwrap_or(Part::Other)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Stutter {
    pub at: Instant,
    pub ms: f32,
    pub part: Part,
}

#[derive(Debug, Clone, Copy)]
struct CpuMark {
    at: Instant,
    thread: Option<f64>,
    process: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Load {
    pub main: Option<f32>,
    pub others: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct Machine {
    pub vsync_bound: bool,
    pub window: (u32, u32),
    pub scene: (u32, u32),
    pub msaa: u32,
    pub texture_mb: u64,
    pub tiles: usize,
    pub ai: usize,
    pub people: usize,
    pub gpu: String,
}

#[derive(Debug, Clone, Default)]
pub struct Readout {
    pub title: String,
    pub head: String,
    pub parts: Vec<(Part, String, String)>,
    pub lines: Vec<String>,
    pub brief: Vec<String>,
}

pub struct PerfOverlay {
    cpu_sources: (fn() -> Option<f64>, fn() -> Option<f64>),
    prev: Option<[f64; 8]>,
    pub frames: VecDeque<Sample>,
    pub version: u64,
    seen: u32,
    typical: f32,
    pub stutters: u32,
    pub last_stutter: Option<Stutter>,
    cpu: Option<CpuMark>,
    load: Option<Load>,
    cores: usize,
    draws: (usize, usize),
    monitor: Option<(Instant, Option<f32>)>,
    since_refresh: f32,
    pub readout: Readout,
}

impl Default for PerfOverlay {
    fn default() -> Self {
        PerfOverlay {
            cpu_sources: (|| None, || None),
            prev: None,
            frames: VecDeque::with_capacity(HISTORY),
            version: 0,
            seen: 0,
            typical: 1.0 / 60.0,
            stutters: 0,
            last_stutter: None,
            cpu: None,
            load: None,
            cores: 0,
            draws: (0, 0),
            monitor: None,
            since_refresh: REFRESH_S,
            readout: Readout::default(),
        }
    }
}

impl PerfOverlay {
    /// CPU counters supplied by the app, so the shared UI model needs no OS bindings.
    pub fn with_cpu_sources(thread: fn() -> Option<f64>, process: fn() -> Option<f64>) -> Self {
        Self {
            cpu_sources: (thread, process),
            ..Default::default()
        }
    }

    pub fn reset(&mut self) {
        *self = Self::with_cpu_sources(self.cpu_sources.0, self.cpu_sources.1);
    }

    pub fn started(&self) -> bool {
        self.prev.is_some()
    }

    pub fn sample(
        &mut self,
        raw_dt: f32,
        profile: impl IntoIterator<Item = (&'static str, f64)>,
        draws: (usize, usize),
    ) {
        self.draws = draws;
        let mut now = [0.0; 8];
        for (key, value) in profile {
            if let Some(part) = part_of(key) {
                now[part as usize] += value;
            }
        }
        let Some(prev) = self.prev.replace(now) else {
            return;
        };
        let mut parts = [0.0f32; 8];
        for k in 0..8 {
            parts[k] = (now[k] - prev[k]).max(0.0) as f32;
        }
        let s = Sample::new(raw_dt, parts);
        self.seen = self.seen.saturating_add(1);
        if self.seen > WARM_UP && raw_dt > STUTTER_S.max(self.typical * STUTTER_RATIO) {
            self.stutters += 1;
            self.last_stutter = Some(Stutter {
                at: Instant::now(),
                ms: raw_dt * 1000.0,
                part: s.largest(),
            });
            self.typical += (raw_dt.min(self.typical * STUTTER_RATIO * 2.0) - self.typical) * 0.01;
        } else {
            self.typical += (raw_dt - self.typical) * 0.05;
        }
        if self.frames.len() == HISTORY {
            self.frames.pop_front();
        }
        self.frames.push_back(s);
        self.version += 1;
        self.since_refresh += raw_dt;
        self.measure_cpu();
    }

    fn measure_cpu(&mut self) {
        let due = self
            .cpu
            .is_none_or(|m| m.at.elapsed().as_secs_f32() >= CPU_S);
        if !due {
            return;
        }
        let mark = CpuMark {
            at: Instant::now(),
            thread: (self.cpu_sources.0)(),
            process: (self.cpu_sources.1)(),
        };
        if let Some(old) = self.cpu.replace(mark) {
            let wall = (mark.at - old.at).as_secs_f64().max(1e-3);
            let thread = old.thread.zip(mark.thread).map(|(a, b)| (b - a).max(0.0));
            let process = old.process.zip(mark.process).map(|(a, b)| (b - a).max(0.0));
            self.load = Some(Load {
                main: thread.map(|t| (t / wall).min(1.0) as f32),
                others: thread
                    .zip(process)
                    .map(|(t, p)| ((p - t).max(0.0) / wall) as f32),
            });
        }
    }

    pub fn refresh_due(&self) -> bool {
        self.since_refresh >= REFRESH_S && !self.frames.is_empty()
    }

    pub fn refresh_hz(&mut self, ask: impl FnOnce() -> Option<f32>) -> Option<f32> {
        match self.monitor {
            Some((at, hz)) if at.elapsed().as_secs_f32() < MONITOR_S => hz,
            _ => {
                let hz = ask();
                self.monitor = Some((Instant::now(), hz));
                hz
            }
        }
    }

    pub fn refresh(&mut self, m: &Machine) {
        self.since_refresh = 0.0;
        if self.cores == 0 {
            self.cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        }
        let recent = average(self.frames.iter().rev().take(RECENT));
        let frame_ms = recent.total * 1000.0;
        let fps = if recent.total > 0.0 {
            1.0 / recent.total
        } else {
            0.0
        };
        let worst = self.frames.iter().map(|s| s.total).fold(0.0f32, f32::max) * 1000.0;
        let mut r = Readout {
            title: crate::tr("Performance").into_owned(),
            head: format!("{} fps   {:.1} ms", fps.round() as i32, frame_ms),
            ..Default::default()
        };
        r.parts = Part::ALL
            .into_iter()
            .filter(|p| *p != Part::Limit || recent.parts[*p as usize] > 0.0)
            .map(|p| {
                let ms = format!("{:.1} ms", recent.parts[p as usize] * 1000.0);
                (p, crate::tr(p.name()).into_owned(), ms)
            })
            .collect();
        r.lines.push(format!(
            "Slowest frame of the graph: {} ms",
            worst.round() as i32
        ));
        let (by, share) = limiting(&recent, m.vsync_bound);
        r.lines.push(format!(
            "Limited by: {} ({}% of the frame)",
            by,
            (share * 100.0).round() as i32
        ));
        r.lines.push(match self.last_stutter {
            Some(s) => format!(
                "Stutters: {}, last {} ms {} s ago ({})",
                self.stutters,
                s.ms.round() as i32,
                s.at.elapsed().as_secs(),
                s.part.name()
            ),
            None => "No stutter yet".to_string(),
        });
        match self.load {
            Some(Load {
                main: Some(main),
                others: Some(others),
            }) => {
                r.lines.push(format!(
                    "Main thread {}% busy, other threads {} of {} cores",
                    (main * 100.0).round() as i32,
                    tenths(others),
                    self.cores
                ));
            }
            Some(Load {
                main: Some(main),
                others: None,
            }) => r.lines.push(format!(
                "Main thread {}% busy",
                (main * 100.0).round() as i32
            )),
            _ => {}
        }
        if m.window.0 > 0 {
            let pct = (m.scene.0 as f32 / m.window.0 as f32 * 100.0).round() as i32;
            r.lines.push(format!(
                "Picture {}×{}, drawn at {} %, MSAA {}×",
                m.window.0, m.window.1, pct, m.msaa
            ));
        }
        if self.draws.0 > 0 {
            r.lines.push(format!(
                "Draw calls {}, objects visible {}",
                self.draws.0, self.draws.1
            ));
        }
        r.lines.push(format!(
            "Texture memory {} MB, map tiles {}",
            m.texture_mb, m.tiles
        ));
        r.lines
            .push(format!("AI vehicles {}, people {}", m.ai, m.people));
        for line in &mut r.lines {
            if let std::borrow::Cow::Owned(t) = crate::tr(line) {
                *line = t;
            }
        }
        r.brief = r.lines[1..3].to_vec();
        if !m.gpu.is_empty() {
            r.lines.push(m.gpu.clone());
        }
        self.readout = r;
    }
}

fn average<'a>(samples: impl Iterator<Item = &'a Sample>) -> Sample {
    let mut sum = Sample::default();
    let mut n = 0;
    for s in samples {
        sum.total += s.total;
        for k in 0..8 {
            sum.parts[k] += s.parts[k];
        }
        n += 1;
    }
    if n > 0 {
        sum.total /= n as f32;
        for p in &mut sum.parts {
            *p /= n as f32;
        }
    }
    sum
}

fn tenths(x: f32) -> f32 {
    (x * 10.0).round() / 10.0
}

pub fn limiting(recent: &Sample, vsync_bound: bool) -> (&'static str, f32) {
    let p = recent.largest();
    let share = if recent.total > 0.0 {
        (recent.parts[p as usize] / recent.total).min(1.0)
    } else {
        0.0
    };
    match p {
        Part::Gpu if vsync_bound => ("V-sync", share),
        p => (p.name(), share),
    }
}

pub fn graph_scale(frames: &VecDeque<Sample>) -> f32 {
    let mut totals: Vec<f32> = frames.iter().map(|s| s.total).collect();
    totals.sort_unstable_by(f32::total_cmp);
    let high = totals.get(totals.len() * 49 / 50).copied().unwrap_or(0.0);
    [1.0 / 30.0, 0.05, 0.1, 0.2]
        .into_iter()
        .find(|&top| high <= top)
        .unwrap_or(0.2)
}

#[cfg(test)]
mod tests {
    #[test]
    fn reset_keeps_the_apps_cpu_counters() {
        let mut overlay = super::PerfOverlay::with_cpu_sources(|| Some(1.0), || Some(2.0));
        overlay.reset();
        overlay.measure_cpu();
        let mark = overlay.cpu.unwrap();
        assert_eq!(mark.thread, Some(1.0));
        assert_eq!(mark.process, Some(2.0));
    }

    use super::*;

    fn profile(entries: &[(&'static str, f64)]) -> BTreeMap<&'static str, f64> {
        entries.iter().copied().collect()
    }

    #[test]
    fn the_stage_keys_fall_into_their_parts() {
        assert_eq!(part_of("traffic"), Some(Part::Simulation));
        assert_eq!(part_of("traffic.tick"), None);
        assert_eq!(part_of("streaming"), Some(Part::Loading));
        assert_eq!(part_of("lights+rain"), Some(Part::Scenery));
        assert_eq!(part_of("hud"), Some(Part::Interface));
        assert_eq!(part_of("render"), Some(Part::Drawing));
        assert_eq!(part_of("present"), Some(Part::Gpu));
        assert_eq!(part_of("limiter"), Some(Part::Limit));
    }

    #[test]
    fn a_frame_is_what_its_stages_took_since_the_last_one() {
        let mut o = PerfOverlay::default();
        o.sample(
            0.016,
            profile(&[("traffic", 1.0), ("present", 2.0)]).into_iter(),
            (0, 0),
        );
        assert!(o.frames.is_empty());
        o.sample(
            0.020,
            profile(&[
                ("traffic", 1.004),
                ("traffic.tick", 1.5),
                ("present", 2.010),
            ]),
            (0, 0),
        );
        let s = o.frames.back().copied().unwrap();
        assert!((s.parts[Part::Simulation as usize] - 0.004).abs() < 1e-5);
        assert!((s.parts[Part::Gpu as usize] - 0.010).abs() < 1e-5);
        assert!((s.parts[Part::Other as usize] - 0.006).abs() < 1e-5);
    }

    #[test]
    fn a_long_frame_after_steady_ones_is_a_stutter_with_its_reason() {
        let mut o = PerfOverlay::default();
        let mut t = 0.0;
        for _ in 0..100 {
            o.sample(0.016, profile(&[("streaming", t)]).into_iter(), (0, 0));
        }
        assert_eq!(o.stutters, 0);
        t += 0.12;
        o.sample(0.13, profile(&[("streaming", t)]).into_iter(), (0, 0));
        assert_eq!(o.stutters, 1);
        assert_eq!(o.last_stutter.map(|s| s.part), Some(Part::Loading));
        assert_eq!(o.frames.len(), 100);
    }

    #[test]
    fn a_frame_rate_that_stays_low_stops_counting_as_stutters() {
        let mut o = PerfOverlay::default();
        for _ in 0..100 {
            o.sample(0.016, BTreeMap::new().into_iter(), (0, 0));
        }
        for _ in 0..600 {
            o.sample(0.08, BTreeMap::new().into_iter(), (0, 0));
        }
        let after = o.stutters;
        for _ in 0..100 {
            o.sample(0.08, BTreeMap::new().into_iter(), (0, 0));
        }
        assert!(after > 0);
        assert_eq!(o.stutters, after);
    }

    #[test]
    fn the_history_keeps_its_length() {
        let mut o = PerfOverlay::default();
        for _ in 0..HISTORY + 50 {
            o.sample(0.01, BTreeMap::new().into_iter(), (0, 0));
        }
        assert_eq!(o.frames.len(), HISTORY);
    }

    #[test]
    fn what_holds_the_frame_back_is_the_largest_part() {
        let mut s = Sample::new(0.03, [0.002, 0.0, 0.0, 0.0, 0.004, 0.02, 0.0, 0.0]);
        assert_eq!(limiting(&s, false).0, "Graphics card");
        assert!((limiting(&s, false).1 - 2.0 / 3.0).abs() < 1e-3);
        assert_eq!(limiting(&s, true).0, "V-sync");
        s.parts[Part::Simulation as usize] = 0.025;
        assert_eq!(limiting(&s, true).0, "Simulation");
    }

    #[test]
    fn the_graph_fits_its_slowest_frame() {
        let mut f: VecDeque<Sample> =
            std::iter::repeat_n(Sample::new(0.012, [0.0; 8]), 100).collect();
        assert_eq!(graph_scale(&f), 1.0 / 30.0);
        f.push_back(Sample::new(0.9, [0.0; 8]));
        assert_eq!(graph_scale(&f), 1.0 / 30.0);
        f.extend(std::iter::repeat_n(Sample::new(0.07, [0.0; 8]), 10));
        assert_eq!(graph_scale(&f), 0.1);
        f.extend(std::iter::repeat_n(Sample::new(0.9, [0.0; 8]), 10));
        assert_eq!(graph_scale(&f), 0.2);
    }
}
