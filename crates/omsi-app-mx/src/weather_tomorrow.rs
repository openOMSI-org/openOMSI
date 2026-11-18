//! Opt-in real weather: map groups select a location, never a credential.
use crate::weather_model::Observations;
use omsi_platform::tomorrow::{coord, credential, nearest, now, Loaded, MapConfig, Region};
use serde_json::Value;
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

const CFG: &str = "openomsi_weather.cfg";
pub(crate) fn selected(value: Option<&str>) -> bool {
    value.is_some_and(|v| v.eq_ignore_ascii_case("tomorrow"))
}
fn settings_dir() -> Option<PathBuf> {
    crate::settings::Settings::path().and_then(|p| p.parent().map(PathBuf::from))
}
fn fetch(key: String, coord: &str, ttl: u64) -> Result<Loaded<Observations>, &'static str> {
    omsi_platform::tomorrow::fetch(key, coord, ttl, settings_dir().as_deref(), observations)
}
fn observations(v: &Value) -> Result<Observations, &'static str> {
    let n = |key: &str| {
        v.get(key)
            .and_then(Value::as_f64)
            .filter(|x| x.is_finite())
            .map(|x| x as f32)
            .filter(|x| x.is_finite())
    };
    Ok(Observations {
        temperature_c: n("temperature").ok_or("response has no valid temperature")?,
        humidity_percent: n("humidity").ok_or("response has no valid humidity")?,
        wind: (
            n("windDirection").unwrap_or(0.0),
            n("windSpeed").ok_or("response has no valid wind speed")?,
        ),
        pressure_hpa: n("pressureSurfaceLevel")
            .or_else(|| n("pressureSeaLevel"))
            .unwrap_or(1013.0),
        visibility_m: n("visibility").unwrap_or(50.0) * 1000.0,
        cloud_cover: n("cloudCover").unwrap_or(0.0) / 100.0,
        cloud_base_m: n("cloudBase").map(|n| n * 1000.0).unwrap_or(1000.0),
        rain_mm_h: n("rainIntensity").unwrap_or(0.0),
        snow_mm_h: n("snowIntensity").unwrap_or(0.0),
    })
}
#[derive(Default)]
pub(crate) struct Tomorrow {
    map: Option<PathBuf>,
    regions: Vec<Region>,
    current: Option<usize>,
    ttl: u64,
    key: Option<String>,
    receiver: Option<mpsc::Receiver<(String, Result<Loaded<Observations>, &'static str>)>>,
    check: Option<Instant>,
    poll_at: Option<Instant>,
    area_at: Option<Instant>,
    fresh_until: Option<Instant>,
    applied: Option<(String, u64)>,
    disabled: bool,
    reported_errors: HashSet<&'static str>,
}
impl Tomorrow {
    /// Report each cause once per weather session; retries still run normally.
    fn report_error(&mut self, cause: &'static str) -> bool {
        self.reported_errors.insert(cause)
    }
    fn poll_due(&mut self, time: Instant) -> bool {
        if self.poll_at.is_some_and(|at| time < at) {
            return false;
        }
        self.poll_at = Some(time + Duration::from_millis(250));
        true
    }
    fn area_due(&mut self, time: Instant) -> bool {
        if self.area_at.is_some_and(|at| time < at) {
            return false;
        }
        self.area_at = Some(time + Duration::from_secs(1));
        true
    }
}
impl crate::App {
    pub(crate) fn tick_tomorrow(&mut self) {
        if !selected(self.args.weather.as_deref())
            || self
                .net
                .lan
                .as_ref()
                .is_some_and(|l| l.role == omsi_net::Role::Client)
        {
            self.session.tomorrow = Default::default();
            return;
        }
        let time = Instant::now();
        if !self.session.tomorrow.poll_due(time) {
            return;
        }
        let Some(world) = self.world.as_ref() else {
            return;
        };
        if self.session.tomorrow.map.as_ref() != Some(&world.global.path) {
            self.session.tomorrow = Tomorrow {
                map: Some(world.global.path.clone()),
                poll_at: Some(time + Duration::from_millis(250)),
                ..Default::default()
            };
            let rel = std::path::Path::new(&self.args.map)
                .parent()
                .unwrap_or(std::path::Path::new(""))
                .join(CFG);
            let rel = rel.strip_prefix(&self.args.root).unwrap_or(&rel);
            let path = omsi_cfg::find_in_roots(&rel.to_string_lossy())
                .map(|p| p.1)
                .unwrap_or_else(|| world.global.path.parent().unwrap().join(CFG));
            let cfg = std::fs::read(path)
                .map_err(|_| "this map has no openomsi_weather.cfg")
                .and_then(|b| MapConfig::parse(&b));
            match cfg {
                Ok(cfg) => {
                    self.session.tomorrow.ttl = cfg.refresh_minutes * 60;
                    for g in cfg.groups {
                        let points = g
                            .spawnpoints
                            .iter()
                            .filter_map(|s| {
                                let ep = world.global.entry_points.iter().find(|e| {
                                    e.object_id == s.object_id
                                        && usize::try_from(e.group)
                                            .ok()
                                            .and_then(|i| world.global.raw_tiles.get(i))
                                            .is_some_and(|t| *t == (s.tile[0], s.tile[1]))
                                })?;
                                let p = world.entry_point_place(ep)?.0;
                                Some([p.x, p.y])
                            })
                            .collect::<Vec<_>>();
                        if points.len() != g.spawnpoints.len() {
                            self.session.tomorrow.disabled = true;
                            log::warn!(
                                "Tomorrow.io: cfg contains a spawnpoint not present in this map"
                            );
                            break;
                        }
                        self.session.tomorrow.regions.push(Region {
                            name: g.name,
                            coord: coord(g.latitude, g.longitude),
                            points,
                        });
                    }
                    self.session.tomorrow.key = credential(settings_dir().as_deref());
                    if self.session.tomorrow.key.is_none() {
                        self.session.tomorrow.disabled = true;
                        self.service_msg =
                            Some(("Tomorrow.io: set your private API key first".into(), 6.0));
                    }
                }
                Err(e) => {
                    self.session.tomorrow.disabled = true;
                    log::warn!("Tomorrow.io: {e}");
                    self.service_msg = Some((omsi_ui::i18n::said(format!("Tomorrow.io: {}", omsi_ui::tr(e)), format!("Tomorrow.io: {e}")), 6.0));
                }
            }
        }
        if self.session.tomorrow.disabled {
            return;
        }
        let Some(player) = self.player.as_ref() else {
            return;
        };
        if self.session.tomorrow.area_due(time) {
            self.session.tomorrow.current = nearest(
                &self.session.tomorrow.regions,
                [player.vehicle.position.x, player.vehicle.position.y],
                self.session.tomorrow.current,
            );
        }
        let Some(index) = self.session.tomorrow.current else {
            return;
        };
        let site = self.session.tomorrow.regions[index].coord.clone();
        if let Some(rx) = self.session.tomorrow.receiver.as_ref() {
            match rx.try_recv() {
                Ok((location, result)) => {
                    self.session.tomorrow.receiver = None;
                    match result {
                        Ok(c) if location == site => {
                            if self.session.tomorrow.applied.as_ref()
                                != Some(&(site.clone(), c.cached.at))
                            {
                                let name = format!(
                                    "Tomorrow.io · {}",
                                    self.session.tomorrow.regions[index].name
                                );
                                let mut to = c.observed.weather(self.session.wetness);
                                to.name = name;
                                let wire = crate::weather_setup::CustomWeather::from_weather(
                                    &to,
                                    1.0,
                                    self.session.wetness,
                                )
                                .encode();
                                self.apply_report_weather(to, None, &wire);
                                self.session.tomorrow.applied = Some((site.clone(), c.cached.at));
                            }
                            let remaining = self
                                .session
                                .tomorrow
                                .ttl
                                .saturating_sub(now().saturating_sub(c.cached.at));
                            self.session.tomorrow.fresh_until =
                                Some(time + Duration::from_secs(remaining));
                            self.session.tomorrow.check =
                                Some(Instant::now() + Duration::from_secs(5));
                        }
                        Ok(_) => self.session.tomorrow.check = None,
                        Err(e) => {
                            if self.session.tomorrow.report_error(e) {
                                log::warn!("Tomorrow.io: {e}");
                                self.service_msg = Some((omsi_ui::i18n::said(format!("Tomorrow.io: {}", omsi_ui::tr(e)), format!("Tomorrow.io: {e}")), 5.0));
                            }
                            self.session.tomorrow.check =
                                Some(Instant::now() + Duration::from_secs(180));
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.session.tomorrow.receiver = None;
                    self.session.tomorrow.check = Some(Instant::now() + Duration::from_secs(180));
                }
            }
        }
        if self.session.tomorrow.receiver.is_some()
            || self
                .session
                .tomorrow
                .check
                .is_some_and(|t| t > Instant::now())
            || self
                .session
                .tomorrow
                .applied
                .as_ref()
                .is_some_and(|(c, _)| {
                    *c == site
                        && self
                            .session
                            .tomorrow
                            .fresh_until
                            .is_some_and(|at| time < at)
                })
        {
            return;
        }
        let Some(key) = self.session.tomorrow.key.clone() else {
            return;
        };
        let ttl = self.session.tomorrow.ttl;
        let (tx, rx) = mpsc::channel();
        self.session.tomorrow.receiver = Some(rx);
        std::thread::spawn(move || {
            let result = fetch(key, &site, ttl);
            let _ = tx.send((site, result));
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retry_errors_are_reported_once_per_cause_and_session() {
        let mut state = Tomorrow::default();
        assert!(state.report_error("offline"));
        assert!(!state.report_error("offline"));
        assert!(state.report_error("budget exhausted"));
        // Returning to a previous cause must not restart the notifications.
        assert!(!state.report_error("offline"));
        state = Tomorrow::default();
        assert!(state.report_error("offline"));
    }
    #[test]
    fn frame_timers_throttle_response_and_anchor_work() {
        let mut state = Tomorrow::default();
        let time = Instant::now();
        assert!(state.poll_due(time));
        assert!(!state.poll_due(time + Duration::from_millis(1)));
        assert!(state.poll_due(time + Duration::from_millis(250)));
        assert!(state.area_due(time));
        assert!(!state.area_due(time + Duration::from_millis(250)));
        assert!(state.area_due(time + Duration::from_secs(1)));
    }
    #[test]
    fn response_uses_metric_units_and_rejects_missing_values() {
        let v = serde_json::json!({"temperature":12,"humidity":80,"windSpeed":6.6,"windDirection":370,"visibility":2,"cloudCover":90,"weatherCode":5101,"snowIntensity":2});
        let observed = observations(&v).unwrap();
        assert_eq!(observed.snow_mm_h, 2.0);
        assert_eq!(observed.cloud_cover, 0.9);
        let c = observed.weather(0.0);
        assert_eq!(c.wind.1, 6.6);
        assert_eq!(c.wind.0, 10.0);
        assert_eq!(c.fog.0, 2000.0);
        assert_eq!(c.precip[0], 2.0);
        assert_eq!(c.clouds.0, "Overcast 1");
        assert!(observations(&serde_json::json!({"temperature":12})).is_err());
    }
}
