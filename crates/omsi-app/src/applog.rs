//! The game's log as a record of the whole session (`game.log`, which the launcher keeps per
//! session): the machine and the settings at the start, and while playing what was said on
//! the screen, the views, the pause, and a status line every minute - what a bug report
//! needs to be understood without asking.

use crate::App;

#[derive(Default)]
pub(crate) struct LogState {
    last_msg: Option<String>,
    last_view: String,
    last_paused: bool,
    t: f32,
    frames: u32,
    worst_dt: f32,
    /// The support snapshot of this game is kept (see `support_bundle::record`).
    support_recorded: bool,
}

/// The machine, the program and its settings, once at the start.
pub(crate) fn log_system(settings: &crate::settings::Settings) {
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0);
    let ram = crate::memory::physical_memory().map(|b| b / 1_000_000).unwrap_or(0);
    log::info!("system: {} {} ({}), {cpus} threads, {ram} MB memory", std::env::consts::OS, std::env::consts::ARCH, os_version());
    log::info!("command line: {}", std::env::args().collect::<Vec<_>>().join(" "));
    if let Ok(d) = std::env::current_dir() {
        log::info!("working folder: {}", d.display());
    }
    let env: Vec<String> = std::env::vars().filter_map(|(k, v)| environment_entry(&k, &v)).collect();
    if !env.is_empty() {
        log::info!("environment: {}", env.join(" "));
    }
    log::info!("all settings: {settings:?}");
}

/// Keep diagnostic switches readable, but never put a player's credential in a log.
fn environment_entry(key: &str, value: &str) -> Option<String> {
    if !(key.starts_with("OMSI_") || key == "RUST_LOG" || key == "WGPU_BACKEND") {
        return None;
    }
    // Environment names are case-insensitive on Windows.
    let value = if key.eq_ignore_ascii_case("OMSI_TOMORROW_API_KEY") { "<set>" } else { value };
    Some(format!("{key}={value}"))
}

#[cfg(test)]
mod tests {
    use super::environment_entry;

    #[test]
    fn startup_environment_masks_credentials_and_keeps_diagnostic_values() {
        let entries = [
            ("OMSI_TOMORROW_API_KEY", "synthetic-private-credential"),
            ("OMSI_TOMORROW_KEY_FILE", "private/key.txt"),
            ("OMSI_BACKEND", "vulkan"),
            ("RUST_LOG", "info"),
            ("WGPU_BACKEND", "vulkan"),
            ("UNRELATED", "not-logged"),
        ];
        let log = entries.into_iter().filter_map(|(k, v)| environment_entry(k, v)).collect::<Vec<_>>().join(" ");
        assert!(log.contains("OMSI_TOMORROW_API_KEY=<set>"));
        assert!(!log.contains("synthetic-private-credential"));
        assert!(!log.contains("not-logged"));
        for diagnostic in ["OMSI_TOMORROW_KEY_FILE=private/key.txt", "OMSI_BACKEND=vulkan", "RUST_LOG=info", "WGPU_BACKEND=vulkan"] {
            assert!(log.contains(diagnostic));
        }
        assert_eq!(environment_entry("OMSI_tomorrow_api_key", "synthetic-private-credential").as_deref(), Some("OMSI_tomorrow_api_key=<set>"));
    }
}

/// The system's name and version (asked once: on macOS and Windows a program is run for it).
pub(crate) fn os_version() -> String {
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION.get_or_init(ask_os_version).clone()
}

fn ask_os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(o) = std::process::Command::new("sw_vers").arg("-productVersion").output() {
            return format!("macOS {}", String::from_utf8_lossy(&o.stdout).trim());
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/etc/os-release") {
            if let Some(l) = s.lines().find(|l| l.starts_with("PRETTY_NAME=")) {
                return l.trim_start_matches("PRETTY_NAME=").trim_matches('"').to_string();
            }
        }
    }
    #[cfg(windows)]
    {
        if let Ok(o) = std::process::Command::new("cmd").args(["/C", "ver"]).output() {
            return String::from_utf8_lossy(&o.stdout).trim().to_string();
        }
    }
    "version unknown".to_string()
}

impl App {
    /// Once a frame: what changed on the screen, and every minute where things stand.
    pub(crate) fn log_frame(&mut self, dt: f32) {
        if !self.perf.log_state.support_recorded && self.renderer.is_some() {
            crate::support_bundle::record(self);
            self.perf.log_state.support_recorded = true;
        }
        // (compared in place: the message was copied every frame it stood on the screen)
        if self.service_msg.as_ref().map(|m| &m.0) != self.perf.log_state.last_msg.as_ref() {
            let msg = self.service_msg.as_ref().map(|m| m.0.clone());
            if msg.is_some() {
                log::info!("on screen: {}", omsi_ui::i18n::english(msg.as_deref().unwrap_or_default()));
            }
            self.perf.log_state.last_msg = msg;
        }
        if self.view != self.perf.log_state.last_view {
            log::info!("view: {}", self.view);
            self.perf.log_state.last_view = self.view.clone();
        }
        if self.paused != self.perf.log_state.last_paused {
            log::info!("{}", if self.paused { "paused" } else { "resumed" });
            self.perf.log_state.last_paused = self.paused;
        }
        let s = &mut self.perf.log_state;
        s.t += dt;
        s.frames += 1;
        s.worst_dt = s.worst_dt.max(dt);
        if s.t < 60.0 {
            return;
        }
        let (fps, worst) = (s.frames as f32 / s.t, 1.0 / s.worst_dt.max(1e-3));
        s.t = 0.0;
        s.frames = 0;
        s.worst_dt = 0.0;
        let bus = self.player.as_ref().map(|p| {
            let v = &p.vehicle;
            format!("bus at ({:.1}, {:.1}, {:.1}) heading {:.0}, {:.0} km/h", v.position.x, v.position.y, v.position.z, v.heading, v.physics.velocity_kmh())
        });
        let cam = self.camera.as_ref().map(|c| format!("camera at ({:.0}, {:.0}, {:.0})", c.position.x, c.position.y, c.position.z));
        let traffic = self.session.traffic.as_ref().map(|t| t.cars.len()).unwrap_or(0);
        let time = self.clock.time;
        let gpu = match (self.renderer.as_ref(), self.scene.as_ref()) {
            (Some(r), Some(sc)) => format!(", GPU memory: textures {:.0} MB, meshes {:.0} MB", r.texture_bytes(sc) as f64 / 1e6, r.mesh_bytes(sc) as f64 / 1e6),
            _ => String::new(),
        };
        // Cumulative diagnostics: visible in an ordinary game.log without requiring
        // profiling flags, and cheap to sample at the existing one-minute status interval.
        let faults = omsi_render::texture_upload_faults();
        let texture_faults = if faults.malformed_mips > 0 || faults.failed_creations > 0 {
            format!(
                ", GPU texture failures: {} malformed mip uploads, {} failed texture creations",
                faults.malformed_mips, faults.failed_creations
            )
        } else {
            String::new()
        };
        log::info!(
            "status: {fps:.0} fps (worst frame {worst:.0} fps), {}, view {}, game time {:02}:{:02}, {traffic} AI vehicles{}{gpu}{texture_faults}",
            bus.or(cam).unwrap_or_default(),
            self.view,
            (time / 3600.0) as i64 % 24,
            (time / 60.0) as i64 % 60,
            if self.paused { ", paused" } else { "" }
        );
    }
}
