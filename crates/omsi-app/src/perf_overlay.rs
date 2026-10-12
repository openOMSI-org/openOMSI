use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

pub(crate) const HISTORY: usize = 240;
const RECENT: usize = 30;
const REFRESH_S: f32 = 0.25;
const CPU_S: f32 = 0.5;
const STUTTER_S: f32 = 0.05;
const STUTTER_RATIO: f32 = 2.5;
const WARM_UP: u32 = 60;
const MONITOR_S: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Part {
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
    pub(crate) const ALL: [Part; 8] = [
        Part::Simulation,
        Part::Loading,
        Part::Scenery,
        Part::Interface,
        Part::Drawing,
        Part::Gpu,
        Part::Limit,
        Part::Other,
    ];

    pub(crate) fn name(self) -> &'static str {
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

    pub(crate) fn color(self) -> [u8; 4] {
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

pub(crate) fn part_of(key: &str) -> Option<Part> {
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

fn part_totals(profile: &BTreeMap<&'static str, f64>) -> [f64; 8] {
    let mut out = [0.0; 8];
    for (k, v) in profile {
        if let Some(p) = part_of(k) {
            out[p as usize] += v;
        }
    }
    out
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Sample {
    pub(crate) total: f32,
    pub(crate) parts: [f32; 8],
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
pub(crate) struct Stutter {
    pub(crate) at: Instant,
    pub(crate) ms: f32,
    pub(crate) part: Part,
}

#[derive(Debug, Clone, Copy)]
struct CpuMark {
    at: Instant,
    thread: Option<f64>,
    process: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Load {
    pub(crate) main: Option<f32>,
    pub(crate) others: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Machine {
    pub(crate) vsync_bound: bool,
    pub(crate) window: (u32, u32),
    pub(crate) scene: (u32, u32),
    pub(crate) msaa: u32,
    pub(crate) texture_mb: u64,
    pub(crate) tiles: usize,
    pub(crate) ai: usize,
    pub(crate) people: usize,
    pub(crate) gpu: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Readout {
    pub(crate) title: String,
    pub(crate) head: String,
    pub(crate) parts: Vec<(Part, String, String)>,
    pub(crate) lines: Vec<String>,
    pub(crate) brief: Vec<String>,
}

pub(crate) struct PerfOverlay {
    prev: Option<[f64; 8]>,
    pub(crate) frames: VecDeque<Sample>,
    pub(crate) version: u64,
    seen: u32,
    typical: f32,
    pub(crate) stutters: u32,
    pub(crate) last_stutter: Option<Stutter>,
    cpu: Option<CpuMark>,
    load: Option<Load>,
    cores: usize,
    draws: (usize, usize),
    monitor: Option<(Instant, Option<f32>)>,
    since_refresh: f32,
    pub(crate) readout: Readout,
}

impl Default for PerfOverlay {
    fn default() -> Self {
        PerfOverlay {
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
    pub(crate) fn reset(&mut self) {
        *self = PerfOverlay::default();
    }

    pub(crate) fn started(&self) -> bool {
        self.prev.is_some()
    }

    pub(crate) fn sample(
        &mut self,
        raw_dt: f32,
        profile: &BTreeMap<&'static str, f64>,
        draws: (usize, usize),
    ) {
        self.draws = draws;
        let now = part_totals(profile);
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
            thread: crate::startup::thread_cpu_seconds(),
            process: process_cpu(),
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

    pub(crate) fn refresh_due(&self) -> bool {
        self.since_refresh >= REFRESH_S && !self.frames.is_empty()
    }

    fn refresh_hz(&mut self, ask: impl FnOnce() -> Option<f32>) -> Option<f32> {
        match self.monitor {
            Some((at, hz)) if at.elapsed().as_secs_f32() < MONITOR_S => hz,
            _ => {
                let hz = ask();
                self.monitor = Some((Instant::now(), hz));
                hz
            }
        }
    }

    pub(crate) fn refresh(&mut self, m: &Machine) {
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
            title: omsi_ui::tr("Performance").into_owned(),
            head: format!("{} fps   {:.1} ms", fps.round() as i32, frame_ms),
            ..Default::default()
        };
        r.parts = Part::ALL
            .into_iter()
            .filter(|p| *p != Part::Limit || recent.parts[*p as usize] > 0.0)
            .map(|p| {
                let ms = format!("{:.1} ms", recent.parts[p as usize] * 1000.0);
                (p, omsi_ui::tr(p.name()).into_owned(), ms)
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
            if let std::borrow::Cow::Owned(t) = omsi_ui::tr(line) {
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

impl crate::App {
    pub(crate) fn refresh_perf_overlay(&mut self) {
        if !self.settings.perf_overlay_on() || !self.perf.overlay.refresh_due() {
            return;
        }
        let fps = self
            .perf
            .overlay
            .frames
            .iter()
            .rev()
            .take(RECENT)
            .map(|s| s.total)
            .sum::<f32>();
        let fps = if fps > 0.0 {
            RECENT.min(self.perf.overlay.frames.len()) as f32 / fps
        } else {
            0.0
        };
        let vsync = self.settings.vsync && !self.vr_active();
        let window = self.window.as_ref();
        let refresh = vsync
            .then(|| {
                self.perf.overlay.refresh_hz(|| {
                    window
                        .and_then(|w| w.current_monitor())
                        .and_then(|m| m.refresh_rate_millihertz())
                        .map(|mhz| mhz as f32 / 1000.0)
                })
            })
            .flatten();
        let mut m = Machine {
            vsync_bound: refresh.is_some_and(|hz| fps >= hz * 0.95),
            tiles: self.world.as_ref().map_or(0, |w| w.loaded_tiles().len()),
            ai: self.session.traffic.as_ref().map_or(0, |t| {
                let (cars, buses, _, _) = t.sim.counts();
                cars + buses
            }),
            people: self.session.humans.as_ref().map_or(0, |h| h.people.len()),
            ..Default::default()
        };
        if let Some(r) = self.renderer.as_ref() {
            if let Some(s) = self.gfx.surface.as_ref() {
                m.window = (s.config.width, s.config.height);
                m.scene = r.scene_size(s.config.width, s.config.height);
            }
            m.msaa = r.options.msaa;
            m.texture_mb = self
                .scene
                .as_ref()
                .map_or(0, |sc| r.texture_bytes(sc) / 1_000_000);
            m.gpu = r.adapter_name.clone();
        }
        self.perf.overlay.refresh(&m);
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

pub(crate) fn limiting(recent: &Sample, vsync_bound: bool) -> (&'static str, f32) {
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

pub(crate) fn graph_scale(frames: &VecDeque<Sample>) -> f32 {
    let mut totals: Vec<f32> = frames.iter().map(|s| s.total).collect();
    totals.sort_unstable_by(f32::total_cmp);
    let high = totals.get(totals.len() * 49 / 50).copied().unwrap_or(0.0);
    [1.0 / 30.0, 0.05, 0.1, 0.2]
        .into_iter()
        .find(|&top| high <= top)
        .unwrap_or(0.2)
}

#[cfg(windows)]
fn process_cpu() -> Option<f64> {
    crate::startup::process_cpu_seconds()
}

#[cfg(unix)]
fn process_cpu() -> Option<f64> {
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) } != 0 {
        return None;
    }
    let secs = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 * 1e-6;
    Some(secs(u.ru_utime) + secs(u.ru_stime))
}

#[cfg(not(any(windows, unix)))]
fn process_cpu() -> Option<f64> {
    None
}

#[cfg(test)]
mod tests {
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
            &profile(&[("traffic", 1.0), ("present", 2.0)]),
            (0, 0),
        );
        assert!(o.frames.is_empty());
        o.sample(
            0.020,
            &profile(&[
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
            o.sample(0.016, &profile(&[("streaming", t)]), (0, 0));
        }
        assert_eq!(o.stutters, 0);
        t += 0.12;
        o.sample(0.13, &profile(&[("streaming", t)]), (0, 0));
        assert_eq!(o.stutters, 1);
        assert_eq!(o.last_stutter.map(|s| s.part), Some(Part::Loading));
        assert_eq!(o.frames.len(), 100);
    }

    #[test]
    fn a_frame_rate_that_stays_low_stops_counting_as_stutters() {
        let mut o = PerfOverlay::default();
        for _ in 0..100 {
            o.sample(0.016, &BTreeMap::new(), (0, 0));
        }
        for _ in 0..600 {
            o.sample(0.08, &BTreeMap::new(), (0, 0));
        }
        let after = o.stutters;
        for _ in 0..100 {
            o.sample(0.08, &BTreeMap::new(), (0, 0));
        }
        assert!(after > 0);
        assert_eq!(o.stutters, after);
    }

    #[test]
    fn the_history_keeps_its_length() {
        let mut o = PerfOverlay::default();
        for _ in 0..HISTORY + 50 {
            o.sample(0.01, &BTreeMap::new(), (0, 0));
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
