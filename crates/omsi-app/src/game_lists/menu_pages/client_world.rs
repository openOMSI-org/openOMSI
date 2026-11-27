//! Read-only time and weather snapshot received from the multiplayer host.

use super::*;

pub(super) fn add_server_world_rows(
    app: &App,
    time: &mut Vec<(String, String)>,
    weather: &mut Vec<(String, String)>,
    climate: &mut Vec<(String, String)>,
) {
    let (day, month) = app.clock.day_month();
    let timestamp = format!(
        "{:04}-{month:02}-{day:02} {:02}:{:02}",
        app.clock.year,
        (app.clock.time.rem_euclid(86_400.0) / 3600.0) as u32,
        (app.clock.time.rem_euclid(86_400.0) as u32 / 60) % 60
    );
    time.push((
        row(
            "Server clock",
            'i',
            &timestamp,
            "Synchronized from the host",
            None,
        ),
        "noop".into(),
    ));
    if let Some(w) = app.session.weather.as_ref() {
        weather.push((
            row(
                "Server weather",
                'i',
                &w.name,
                "Read-only shared weather",
                None,
            ),
            "noop".into(),
        ));
        weather.push((
            row(
                "Precipitation",
                'i',
                &format!(
                    "{:.0} %",
                    if w.precip[0] < 0.5 {
                        0.0
                    } else {
                        (w.precip[1] / 255.0).clamp(0.0, 1.0) * 100.0
                    }
                ),
                "Host rain or snow",
                None,
            ),
            "noop".into(),
        ));
        climate.push((
            row(
                "Temperature",
                'i',
                &format!("{:.1} °C", w.temp.0),
                "Host temperature",
                None,
            ),
            "noop".into(),
        ));
        climate.push((
            row(
                "Wind",
                'i',
                &format!("{:.1} m/s, {:.0}°", w.wind.1, w.wind.0.rem_euclid(360.0)),
                "Host wind",
                None,
            ),
            "noop".into(),
        ));
    }
}
