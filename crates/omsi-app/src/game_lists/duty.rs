//! Lines and tours, the settings kept between sessions, drivers, fleet numbers and starting a duty.

use super::*;

/// The time (seconds of the day) a tour starts: its earliest trip's departure.
pub(crate) fn tour_start(tour: &omsi_timetable::Tour) -> Option<f64> {
    tour.trips.iter().map(|t| t.departure as f64 * 60.0).fold(None, |a: Option<f64>, d| Some(a.map_or(d, |x| x.min(d))))
}

/// Whether a tour is listed now: it runs on this day and is current at `now` (seconds of the
/// day) - under way, or leaving within half an hour; one that has finished is not.
pub(super) fn tour_listed(sch: &crate::schedule::Schedule, line: &str, tour: &omsi_timetable::Tour, now: f64) -> bool {
    if !sch.tour_available(tour) {
        return false;
    }
    let start = tour_start(tour).unwrap_or(0.0);
    let end = sch.tour_stops(line, &tour.number).iter().map(|s| s.3).fold(start, f64::max);
    // (a night tour's times go on past 24:00: the early hours of the next day count too)
    [now, now + 86400.0].iter().any(|n| *n >= start - 1800.0 && *n <= end + 60.0)
}

/// The line number a line's trips carry (`.ttp` line), else the name of the line's file.
pub(super) fn line_sign(schedule: Option<&crate::schedule::Schedule>, line: &omsi_timetable::Line) -> String {
    let sign = schedule.and_then(|sch| {
        line.tours.iter().flat_map(|t| t.trips.iter()).find_map(|tt| {
            let t = sch.data.trips.iter().find(|x| x.name.eq_ignore_ascii_case(&tt.trip))?;
            Some(t.line.trim().to_string()).filter(|n| !n.is_empty())
        })
    });
    sign.unwrap_or_else(|| line.name.clone())
}

/// A line's tours in alphabetical order of their numbers (numbers inside them as numbers:
/// "2" before "10"; equal numbers by the time they start).
pub(super) fn sorted_tours(line: &omsi_timetable::Line) -> Vec<&omsi_timetable::Tour> {
    let mut tours: Vec<&omsi_timetable::Tour> = line.tours.iter().collect();
    tours.sort_by(|a, b| {
        bus_cmp(a.number.trim(), b.number.trim()).then_with(|| {
            let (ta, tb) = (tour_start(a).unwrap_or(f64::MAX), tour_start(b).unwrap_or(f64::MAX));
            ta.partial_cmp(&tb).unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    tours
}

/// The name of trip number `k` of a tour, counted as `Schedule::tour_stops` does (trips the
/// timetable does not know are left out).
pub(super) fn tour_trip_name(sch: &crate::schedule::Schedule, tour: &omsi_timetable::Tour, k: usize) -> Option<String> {
    tour.trips
        .iter()
        .filter(|tt| sch.data.trips.iter().any(|x| x.name.eq_ignore_ascii_case(&tt.trip)))
        .nth(k)
        .map(|tt| tt.trip.clone())
}

/// Numbers compared as numbers where they are ("5" before "13", "N30" after "M49").
pub(super) fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    let key = |s: &str| {
        let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
        (digits.parse::<u64>().unwrap_or(u64::MAX), s.to_ascii_lowercase())
    };
    key(a).cmp(&key(b))
}

// The settings file's keys waiting to be written, and the file as last read: process-wide
// like the file itself (written on exit by `flush_settings(true)`).
pub(super) static PENDING_SETTINGS: std::sync::Mutex<(Vec<(String, String)>, Option<std::time::Instant>)> =
    std::sync::Mutex::new((Vec::new(), None));
pub(super) const SETTINGS_FLUSH_MS: u128 = 250;
pub(super) static SETTINGS_CACHE: std::sync::Mutex<Option<serde_json::Value>> = std::sync::Mutex::new(None);

/// Write one key of `~/.openomsi/settings.cfg` (the launcher's file; the other lines
/// stay as they are). The write is delayed a moment and joined with the ones that follow.
/// The key the seat of the player's bus is kept under (`settings::bus_seats`): its `.bus`
/// file, relative to the installation where it lies in it.
pub(crate) fn seat_key(p: &crate::player::Player) -> String {
    // (from its `Vehicles` folder on, so that a copy of the installation finds it)
    let path = p.vehicle.ty.def.path.to_string_lossy().replace('\\', "/");
    match path.to_ascii_lowercase().rfind("/vehicles/") {
        Some(i) => path[i + 1..].to_string(),
        None => path,
    }
}

/// The seat as the menu has it now, kept for the player's bus.
pub(super) fn remember_bus_seat(app: &App) {
    if let Some(p) = app.player.as_ref() {
        crate::settings::bus_seats::remember(&seat_key(p), app.settings.seat, app.settings.seat_pitch_deg);
    }
}

pub(crate) fn remember_setting(key: &str, value: &str) {
    {
        let mut p = PENDING_SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
        match p.0.iter_mut().find(|(k, _)| k == key) {
            Some(e) => e.1 = value.to_string(),
            None => p.0.push((key.to_string(), value.to_string())),
        }
    }
    flush_settings(false);
}

/// Write the remembered keys out: all of them when `force`, else only when the last write
/// is `SETTINGS_FLUSH_MS` ago. Called every frame, before the file is read and on exit.
pub(crate) fn flush_settings(force: bool) {
    let pending = {
        let mut p = PENDING_SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
        if p.0.is_empty() {
            return;
        }
        if !force && p.1.is_some_and(|t| t.elapsed().as_millis() < SETTINGS_FLUSH_MS) {
            return;
        }
        p.1 = Some(std::time::Instant::now());
        std::mem::take(&mut p.0)
    };
    let Ok(mut v) = omsi_launcher_lib::get_settings() else { return };
    apply_pending(&mut v, &pending);
    if let Err(e) = omsi_launcher_lib::save_settings(&v) {
        log::warn!("settings not saved: {e:#}");
    }
    *SETTINGS_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

pub(crate) fn apply_pending(v: &mut serde_json::Value, pending: &[(String, String)]) {
    for (key, value) in pending {
        let parsed: serde_json::Value = value.parse::<f64>().map(serde_json::Value::from).unwrap_or_else(|_| serde_json::Value::from(value.as_str()));
        // a switch goes in as true/false, as the launcher's own values are: written as 1 it
        // was read as not set and saved back as its default (the pause menu's options were
        // lost with the next game)
        let parsed = match (&v[key.as_str()], &parsed) {
            (serde_json::Value::Bool(_), serde_json::Value::Number(n)) => serde_json::Value::Bool(n.as_f64().unwrap_or(0.0) > 0.5),
            _ if key == "time_speed" => serde_json::Value::from(value.as_str()),
            _ => parsed,
        };
        v[key.as_str()] = parsed;
    }
}

/// The personnel files there are (content folder and OMSI 2's `Drivers`), by name.
pub(super) fn driver_names(app: &App) -> Vec<String> {
    let mut names: Vec<String> = omsi_cfg::read_dir_merged("Drivers")
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("odr")))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        .collect();
    let _ = app;
    names.sort_by_key(|n| n.to_ascii_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    names
}

/// Go on with another driver: this run so far into the old personnel file, the rest into
/// the new one.
pub(super) fn switch_driver(app: &mut App, name: &str) {
    if app.session.career.path.is_some() {
        if let Err(e) = app.session.career.save() {
            log::warn!("writing the personnel file: {e}");
        }
    }
    let rel = format!("Drivers/{name}.odr");
    let mut next = crate::career::Career::load(&app.args.root, &rel);
    // (the distance and the clock of the run go on; the counters start with the new file;
    // the bus's motion and the trip driven go on)
    next.seconds = app.session.career.seconds;
    next.go_on_from(&mut app.session.career);
    app.session.career = next;
    app.args.driver = Some(rel);
    app.service_msg = Some((omsi_ui::i18n::tr_fill("Driver: {name}", &[("{name}", &name)]), 3.0));
}

/// The fleet numbers of the bus's `[number]` list with their registrations.
pub(super) fn fleet_numbers(v: &omsi_sim::VehicleInstance) -> Vec<(String, String)> {
    let def = &v.ty.def;
    def.numbers_with_plates()
        .iter()
        .map(|(n, _)| {
            let reg = if def.registration_mode == 1 { String::new() } else { def.chosen_plate_of_number(n) };
            (n.clone(), reg)
        })
        .collect()
}

/// Take on line `line`, tour `tour` from now: the duty, and the IBIS typed for it.
/// The tour on row `k` of the open list of tours: (line, tour).
pub(crate) fn tour_at(app: &App, k: usize) -> Option<(String, String)> {
    let action = app.menus.admin_list.as_ref()?.get(k)?.1.strip_prefix("tour ")?;
    let (line, tour) = action.split_once('\u{1}')?;
    Some((line.to_string(), tour.to_string()))
}

/// For the tour on row `k`: how many stops its chosen trip has, the stop chosen to start
/// from, the trip chosen and how many trips the tour has.
pub(crate) fn tour_choice(app: &App, k: usize) -> Option<(usize, usize, usize, usize)> {
    let (line, tour) = tour_at(app, k)?;
    let sch = crate::duty_book::timetable(app)?;
    let trips = sch.tour_trip_count(&line, &tour);
    let (stop, trip) = match app.menus.list_kind.as_ref() {
        Some(ListKind::Tours(_, Some(p))) if p.0 == tour => (p.1, p.2),
        _ => (0, sch.tour_trip_now(&line, &tour, app.clock.time)),
    };
    let trip = trip.min(trips.saturating_sub(1));
    let n = sch.tour_trip_stops(&line, &tour, trip).len();
    (n > 0).then(|| (n, stop.min(n - 1), trip, trips))
}

/// Start the tour at stop number `chosen` of trip number `trip` of the tour (the trip chosen
/// by its time): the duty goes on from that stop, the bus stays where it is.
pub(crate) fn start_duty_at(app: &mut App, line: &str, tour: &str, trip: usize, chosen: usize) {
    crate::duty_book::timetable_for_duty(app);
    let now = app.clock.time;
    let Some((k, j)) = app.session.schedule.as_ref().and_then(|s| s.tour_trip_stops(line, tour, trip).get(chosen).map(|x| (x.0, x.1))) else {
        return start_duty(app, line, tour);
    };
    let (Some(w), Some(sch)) = (app.world.clone(), app.session.schedule.as_mut()) else { return };
    // (the duty is made at the time it is now, as `start_duty` does: a night tour picked
    // after midnight is then yesterday's, under way, not tonight's)
    let mut d = match sch.player_duty(&w, line, tour, now, None, false) {
        Ok(d) => d,
        Err(e) => {
            app.service_msg = Some((omsi_ui::i18n::tr_fill_words("No duty ({e})", &[("{e}", &e)]), 8.0));
            return;
        }
    };
    // (no teleport: the stop chosen is the one the bus drives to next)
    d.start_at_here(k, j);
    if let Some(p) = app.player.as_mut() {
        // (nothing typed into the IBIS or the blind, as in OMSI 2: Shift+U does that)
        d.update(&mut p.vehicle, now);
        if let Some(w) = app.world.as_ref() {
            let mut fonts = w.fonts.lock();
            if let Err(e) = crate::schedule_paper::update_vehicle(&mut p.vehicle, &d, &mut fonts) {
                log::warn!("driver timetable paper: {e:#}");
            }
        }
    }
    app.args.line = Some(line.to_string());
    app.args.tour = Some(tour.to_string());
    app.session.duty = Some(d);
    app.service_msg = Some((format!("Line {line}, tour {}", tour.trim()), 4.0));
}

pub(super) fn start_duty(app: &mut App, line: &str, tour: &str) {
    crate::duty_book::timetable_for_duty(app);
    let (Some(w), Some(sch)) = (app.world.clone(), app.session.schedule.as_mut()) else { return };
    let now = app.clock.time;
    match sch.player_duty(&w, line, tour, now, None, false) {
        Ok(mut d) => {
            if let Some(p) = app.player.as_mut() {
                d.update(&mut p.vehicle, now);
                let mut fonts = w.fonts.lock();
                if let Err(e) = crate::schedule_paper::update_vehicle(
                    &mut p.vehicle,
                    &d,
                    &mut fonts,
                ) {
                    log::warn!("driver timetable paper: {e:#}");
                }
            }
            app.args.line = Some(line.to_string());
            app.args.tour = Some(tour.to_string());
            app.session.duty = Some(d);
            app.service_msg = Some((format!("Line {line}, tour {}", tour.trim()), 4.0));
        }
        Err(e) => app.service_msg = Some((omsi_ui::i18n::tr_fill_words("No duty ({e})", &[("{e}", &e)]), 8.0)),
    }
}
