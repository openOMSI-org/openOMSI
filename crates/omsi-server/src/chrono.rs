//! Non-blocking Chrono map migration at a date transition.
//!
//! Never parse tiles, AI lists or timetables on the LAN tick thread. Only
//! install a fully prepared CPU world for the currently advertised date.

use crate::reboot::{self, Prepared};
use crate::streaming::StreamWorker;
use anyhow::Result;
use omsi_lan::server_cfg::ServerCfg;
use omsi_sim::SimClock;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

const RETRY_DELAY: Duration = Duration::from_secs(30);

struct Completed {
    date: i32,
    dirs: Vec<PathBuf>,
    result: Result<Prepared>,
}

/// The active Chrono selection is tied to a CPU world snapshot, not merely
/// to the clock shown to LAN peers. Changing the date alone is insufficient.
pub(crate) struct ChronoRefresh {
    map_dir: PathBuf,
    observed_date: i32,
    wanted_dirs: Vec<PathBuf>,
    installed_dirs: Vec<PathBuf>,
    pending: Option<Receiver<Completed>>,
    retry_after: Instant,
}

fn needs_rebuild(installed: &[PathBuf], selected: &[PathBuf]) -> bool {
    installed != selected
}

impl ChronoRefresh {
    pub(crate) fn new(map_dir: PathBuf, date: i32) -> Self {
        let dirs = omsi_map::active_chrono_dirs(&map_dir, date);
        Self {
            map_dir,
            observed_date: date,
            wanted_dirs: dirs.clone(),
            installed_dirs: dirs,
            pending: None,
            retry_after: Instant::now(),
        }
    }

    /// Explicit /restart has already installed the current-date selection.
    /// Invalidates any older background preparation, so it cannot overwrite it.
    pub(crate) fn manual_restart(&mut self, date: i32) {
        let dirs = omsi_map::active_chrono_dirs(&self.map_dir, date);
        self.observed_date = date;
        self.wanted_dirs = dirs.clone();
        self.installed_dirs = dirs;
        self.pending = None;
        self.retry_after = Instant::now();
    }

    pub(crate) fn installed(&mut self) {
        self.installed_dirs = self.wanted_dirs.clone();
    }

    /// Advance one non-blocking check. Returns a prepared world only when its
    /// date is still current. On failure, keep the old world and retry later.
    pub(crate) fn tick(
        &mut self,
        root: &Path,
        cfg: &ServerCfg,
        clock: &SimClock,
        simulated: bool,
        active_keys: &[(i32, i32)],
    ) -> Option<Prepared> {
        let today = clock.date_code();
        if self.observed_date != today {
            self.observed_date = today;
            self.wanted_dirs = omsi_map::active_chrono_dirs(&self.map_dir, today);
            self.retry_after = Instant::now();
        }

        if let Some(receiver) = self.pending.as_ref() {
            match receiver.try_recv() {
                Ok(completed) => {
                    self.pending = None;
                    match completed.result {
                        Ok(prepared) => {
                            if completed.date == today
                                && completed.dirs == self.wanted_dirs
                                && needs_rebuild(&self.installed_dirs, &completed.dirs)
                            {
                                return Some(prepared);
                            }
                            // A rapid clock jump or a successful manual restart
                            // superseded this background build.
                            retire_worker(prepared.streamer);
                        }
                        Err(error) => {
                            log::warn!(
                                "native Chrono: CPU date {} rebuild failed; retaining the previous map, AI and LAN session: {error:#}",
                                completed.date
                            );
                            self.retry_after = Instant::now() + RETRY_DELAY;
                        }
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.retry_after = Instant::now() + RETRY_DELAY;
                    log::warn!("native Chrono: background CPU map loader disconnected");
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        if self.pending.is_none()
            && needs_rebuild(&self.installed_dirs, &self.wanted_dirs)
            && Instant::now() >= self.retry_after
        {
            let (sender, receiver) = mpsc::channel();
            let root = root.to_path_buf();
            let cfg = cfg.clone();
            let next_clock = clock.clone();
            let retained_tiles = active_keys.to_vec();
            let dirs = self.wanted_dirs.clone();
            match std::thread::Builder::new()
                .name("omsi-chrono-rebuild".to_string())
                .spawn(move || {
                    let result = reboot::prepare_with_active(
                        &root,
                        &cfg,
                        &next_clock,
                        simulated,
                        &retained_tiles,
                    );
                    if let Err(disconnected) = sender.send(Completed {
                        date: today,
                        dirs,
                        result,
                    }) {
                        if let Ok(prepared) = disconnected.0.result {
                            retire_worker(prepared.streamer);
                        }
                    }
                }) {
                Ok(thread) => {
                    drop(thread);
                    self.pending = Some(receiver);
                    log::info!(
                        "native Chrono: preparing date {} CPU map, AI lists and timetable without blocking LAN ticks",
                        today
                    );
                }
                Err(error) => {
                    self.retry_after = Instant::now() + RETRY_DELAY;
                    log::warn!("native Chrono: cannot start CPU map refresh: {error}");
                }
            }
        }

        None
    }
}

/// Retire the old tile worker off the LAN tick thread. Its WorldCell is
/// independent of the newly published snapshot and can finish safely.
pub(crate) fn retire_worker(worker: StreamWorker) {
    if let Err(error) = std::thread::Builder::new()
        .name("omsi-chrono-retire".to_string())
        .spawn(move || worker.stop())
    {
        log::warn!("native Chrono: cannot spawn tile worker cleanup thread: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_only_when_the_chrono_selection_changes() {
        let a = PathBuf::from("Chrono/weekday");
        let b = PathBuf::from("Chrono/weekend");
        assert!(!needs_rebuild(&[], &[]));
        assert!(!needs_rebuild(&[a.clone()], &[a.clone()]));
        assert!(needs_rebuild(&[a], &[b]));
    }

    #[test]
    fn midnight_activates_chrono_without_rebuilding_on_an_unchanged_day() {
        use std::fs;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let map_dir =
            std::env::temp_dir().join(format!("omsi-native-chrono-{}-{stamp}", std::process::id()));
        let chrono = map_dir.join("Chrono").join("SummerTimetable");
        fs::create_dir_all(&chrono).unwrap();
        fs::write(chrono.join("Chrono.cfg"), "[startdate]\n19890531\n").unwrap();

        let mut clock = crate::clock::NativeCalendar::new("1989-05-30", "23:59");
        let before = clock.at(86_399.0).date_code();
        let after = clock.at(86_400.0).date_code();
        assert_eq!((before, after), (19890530, 19890531));
        let before_dirs = omsi_map::active_chrono_dirs(&map_dir, before);
        let after_dirs = omsi_map::active_chrono_dirs(&map_dir, after);
        assert!(before_dirs.is_empty());
        assert_eq!(after_dirs, vec![chrono.clone()]);
        assert!(needs_rebuild(&before_dirs, &after_dirs));
        assert!(!needs_rebuild(&after_dirs, &after_dirs));

        fs::remove_dir_all(&map_dir).unwrap();
    }

    #[test]
    fn manual_restart_discards_a_stale_chrono_request() {
        let path = std::env::temp_dir().join(format!("omsi-chrono-empty-{}", std::process::id()));
        let mut refresh = ChronoRefresh::new(path, 19890530);
        let (_send, receive) = mpsc::channel();
        refresh.pending = Some(receive);
        refresh.manual_restart(19890531);
        assert!(refresh.pending.is_none());
        assert_eq!(refresh.observed_date, 19890531);
        assert_eq!(refresh.wanted_dirs, refresh.installed_dirs);
    }
}
