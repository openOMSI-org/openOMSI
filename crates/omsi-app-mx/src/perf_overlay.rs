pub(crate) use omsi_ui::performance::{HISTORY, Part, PerfOverlay, Sample, graph_scale};
use omsi_ui::performance::{Machine, RECENT};

pub(crate) fn new() -> PerfOverlay {
    PerfOverlay::with_cpu_sources(crate::startup::thread_cpu_seconds, process_cpu)
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
