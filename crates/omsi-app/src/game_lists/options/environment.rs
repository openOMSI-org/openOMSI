use super::*;

pub(super) fn option_set_environment(
    app: &mut App,
    verb: &str,
    _arg: &str,
    v: f32,
) -> Option<(&'static str, String)> {
    match verb {
        // the clock set directly: the hour or the minute (the seconds stay)
        "hour" | "minute" => {
            if app
                .net
                .lan
                .as_ref()
                .is_some_and(|l| l.role == omsi_net::Role::Client)
            {
                app.service_msg = Some(("In a LAN session the host sets the clock".into(), 3.0));
                return None;
            }
            let t = app.clock.time;
            let (h, m) = (
                ((t / 3600.0) as i64).rem_euclid(24),
                ((t / 60.0) as i64) % 60,
            );
            let (h, m) = if verb == "hour" {
                (v.round() as i64, m)
            } else {
                (h, v.round() as i64)
            };
            let target = (h * 3600 + m * 60) as f64 + t % 60.0;
            app.shift_clock(target - t);
            None
        }
        // the weather, made by hand (what the preset was stays as it was but for this)
        "visibility" => {
            app.edit_weather(|w| w.fog.0 = v);
            None
        }
        "rain_amt" => {
            app.edit_weather(|w| {
                w.precip[1] = (v * 255.0).round();
                if v > 0.0 && w.precip[0] < 0.5 {
                    w.precip[0] = 1.0;
                }
            });
            None
        }
        "wet" => {
            let mut c = custom_state(app);
            c.road_wetness = v;
            app.set_custom_weather(c);
            None
        }
        "cloud_cover" => {
            // (and the kind the cover stands nearest: the picture the sky is drawn with)
            let v = v.clamp(0.0, 1.0);
            app.edit_weather(|w| {
                w.cloud_cover = Some(v);
                w.clouds.0 = CLOUD_KINDS[crate::weather_setup::cloud_kind_nearest(v)]
                    .id
                    .to_string();
                if v <= 0.0 {
                    w.clouds.1 = 0.0;
                }
            });
            None
        }
        "brightness" => {
            let mut c = custom_state(app);
            c.brightness = v;
            app.set_custom_weather(c);
            None
        }
        "humidity" => {
            let mut c = custom_state(app);
            c.humidity = v;
            app.set_custom_weather(c);
            None
        }
        "temp" => {
            app.edit_weather(|w| w.temp.0 = v);
            None
        }
        "wind_speed" => {
            app.edit_weather(|w| w.wind.1 = v);
            None
        }
        "wind_dir" => {
            app.edit_weather(|w| w.wind.0 = v);
            None
        }
        _ => set_graphics_brightness(app, verb, v),
    }
}
