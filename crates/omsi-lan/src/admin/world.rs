//! A dedicated server's own world as its admins change it: the host loop's part of an
//! administration operation (`exec` checked it and left it to the loop) - the clock, its
//! speed, the weather, the AI traffic, the passengers - done on the loop's step and answered
//! with what became of it.

use super::{Action, TrafficOrder, WeatherAction};
use omsi_admin::Outcome;

/// The server's AI traffic as the admins change it (the game's, whatever it draws of it).
pub trait ServerTraffic {
    fn set_density(&mut self, n: usize);
    /// Every AI car off the road (the timetable's buses stay): how many went.
    fn clear_cars(&mut self) -> usize;
}

/// What of the server's world an operation may change.
pub struct ServerWorld<'a> {
    pub lan: &'a mut omsi_net::LanSession,
    /// The clock moved by the admins (`ServerAdmin::shift`), and the passengers' density.
    pub shift: &'a mut f64,
    pub passenger_density: &'a mut Option<f32>,
    /// The clock without the admins' shift (s from the start day's midnight).
    pub clock: f64,
    pub traffic: Option<&'a mut dyn ServerTraffic>,
    pub traffic_max: usize,
}

/// The installed weathers (`Weather/<file>.owt`), sorted.
fn weathers() -> Vec<String> {
    let mut files: Vec<String> = omsi_cfg::read_dir_merged("Weather")
        .into_iter()
        .filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("owt")).unwrap_or(false))
        .filter_map(|p| p.file_name().map(|n| format!("Weather/{}", n.to_string_lossy())))
        .collect();
    files.sort();
    files.dedup();
    files
}

/// Do a world operation (one `exec` checked: the real time and the METAR sync were refused
/// there).
pub fn apply(w: &mut ServerWorld, action: &Action) -> Outcome {
    match action {
        Action::Time(s) => {
            *w.shift += s;
            Outcome::applied(format!("clock {:+.0} min", s / 60.0))
        }
        // the time of day asked for: the shift that makes the clock read it
        Action::Clock(want) => {
            let now = (w.clock + *w.shift).rem_euclid(86_400.0);
            *w.shift += (want - now + 43_200.0).rem_euclid(86_400.0) - 43_200.0;
            Outcome::applied(format!("clock {}", omsi_sim::timetable_run::hhmm(*want)))
        }
        Action::Speed(s) => {
            w.lan.clock_speed = *s;
            Outcome::applied(format!("time speed x{s}"))
        }
        Action::Weather(WeatherAction::Set(want)) => {
            // only an installed weather (the name came over the network)
            match weathers().into_iter().find(|f| f.eq_ignore_ascii_case(&want.replace('\\', "/"))) {
                Some(f) => {
                    log::info!("server: weather now {f}");
                    w.lan.set_weather(&f);
                    Outcome::applied(format!("weather {f}"))
                }
                None => Outcome::rejected(format!("{want} is not installed on the server")),
            }
        }
        // (a server has no weather cycle of its own: the menu's "cycle" is the next weather)
        Action::Weather(WeatherAction::Next | WeatherAction::Cycle) => {
            let files = weathers();
            if files.is_empty() {
                return Outcome::rejected("no weather installed on the server");
            }
            let cur = w.lan.weather().replace('\\', "/").to_ascii_lowercase();
            let k = files.iter().position(|f| f.to_ascii_lowercase() == cur).map(|k| (k + 1) % files.len()).unwrap_or(0);
            log::info!("server: weather now {}", files[k]);
            w.lan.set_weather(&files[k]);
            Outcome::applied(format!("weather {}", files[k]))
        }
        Action::Traffic(arg) => {
            let Some(t) = w.traffic.as_deref_mut() else { return Outcome::rejected("the server has no AI traffic") };
            match TrafficOrder::parse(arg, w.traffic_max) {
                Some(TrafficOrder::Density(n)) => {
                    t.set_density(n);
                    log::info!("server: traffic density now {n}");
                    Outcome::applied(format!("traffic {n}"))
                }
                // every AI car off the road (a jam; the timetable's buses stay)
                Some(TrafficOrder::Clear) => {
                    let n = t.clear_cars();
                    log::info!("server: {n} AI vehicles taken off the road");
                    Outcome::applied(format!("{n} AI vehicles taken off the road"))
                }
                None => Outcome::rejected(format!("a traffic density from 0 to {} or 'clear'", w.traffic_max)),
            }
        }
        // the people the stops and pavements get from now on: those already waiting,
        // walking or aboard stay where they are
        Action::PassengerDensity(d) => {
            log::info!("server: passenger density now {d}");
            *w.passenger_density = Some(*d);
            Outcome::applied(format!("passenger density {d} (new people from now on)"))
        }
        _ => Outcome::rejected("not an operation on the server's world"),
    }
}
