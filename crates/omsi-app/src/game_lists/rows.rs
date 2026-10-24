//! Rows of the settings windows, their values' steps, and the hand-set weather's state.

use super::*;

// ---------------------------------------------------------------------------------------
// the settings windows (options, vehicle, world): pages of rows

/// How a row's value is changed.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Move {
    /// Enter or a click on the line: the next value (a switch flips, a row of values wraps).
    Next,
    /// Left: the previous value, or off.
    Dec,
    /// Right: the next value, or on.
    Inc,
    /// A click on a slider: this far (0 to 1) along it.
    To(f32),
}

/// One page of a settings window: its title and its rows (label, action).
pub(super) type Page = (&'static str, Vec<(String, String)>);

/// A row of a settings window (see `ui::MenuKind::Options` for the format).
pub(crate) fn row(name: &str, kind: char, value: &str, desc: &str, frac: Option<f32>) -> String {
    format!("{name}\u{1f}{kind}\u{1f}{value}\u{1f}{desc}\u{1f}{}", frac.map(|f| format!("{f:.3}")).unwrap_or_default())
}

/// A row that opens another list.
pub(crate) fn opens(name: &str, desc: &str, id: &str) -> (String, String) {
    (row(name, 'o', "", desc, None), id.to_string())
}

/// A row with a button that does something.
pub(crate) fn button(name: &str, text: &str, desc: &str, id: &str) -> (String, String) {
    (row(name, 'a', text, desc, None), id.to_string())
}

/// A switch row, if the setting `id` is one.
/// The duty given up (the game menu's "End the duty", Free drive in the list of lines): the
/// bus is unscheduled, and the `GetTT*` callbacks answer ""/0/-1 again as in Omsi.exe. "End
/// the duty" dropped the duty alone, and the bus's own displays went on with the old trip -
/// a paper sign with its line in the window, the IBIS's stop list (#1317).
pub(crate) fn end_duty(app: &mut App) {
    app.session.duty = None;
    if let Some(p) = app.player.as_mut() {
        clear_timetable(&mut p.vehicle);
    }
    app.service_msg = Some(("Free drive: no duty".into(), 4.0));
}

/// A vehicle's timetable for its scripts emptied (see `end_duty`).
pub(crate) fn clear_timetable(v: &mut omsi_sim::VehicleInstance) {
    let h = &mut v.host;
    h.tt_line.clear();
    h.tt_stops.clear();
    h.tt_stop_ids.clear();
    h.tt_busstop_index = -1;
    h.tt_terminus_index = -1;
    h.tt_delay = 0.0;
    h.schedule_active = 0.0;
    v.set_var("schedule_active", 0.0);
}

pub(crate) fn switch_row(app: &App, id: &str, name: &str, desc: &str) -> Option<(String, String)> {
    let on = toggle_now(app, id)?;
    Some((row(name, 's', if on { "on" } else { "off" }, desc, None), id.to_string()))
}

/// A slider row for the setting `id` ("verb" or "verb arg"); `fmt` writes its value.
pub(crate) fn slider_row(app: &App, id: &str, name: &str, desc: &str, fmt: &dyn Fn(f32) -> String) -> Option<(String, String)> {
    let (verb, arg) = id.split_once(' ').unwrap_or((id, ""));
    let steps = steps_of(verb)?;
    let now = option_now(app, verb, arg)?;
    let i = nearest(&steps, now);
    let frac = if steps.len() > 1 { i as f32 / (steps.len() - 1) as f32 } else { 0.0 };
    Some((row(name, 'v', &fmt(now), desc, Some(frac)), id.to_string()))
}

/// The values a slider's setting runs through.
pub(super) fn steps_of(verb: &str) -> Option<Vec<f32>> {
    Some(match verb {
        "vr_nav_x" | "vr_nav_y" | "vr_nav_z" => (-100..=100).map(|v| v as f32 * 0.02).collect(),
        "triple_width_mm" => (20..=200).map(|v| v as f32 * 10.0).collect(),
        "triple_distance_mm" => (20..=300).map(|v| v as f32 * 10.0).collect(),
        "triple_bezel_mm" => (0..=100).map(|v| v as f32).collect(),
        "triple_left_angle_deg" | "triple_right_angle_deg" => (0..=90).map(|v| v as f32).collect(),
        "triple_eye_height_mm" => (-100..=100).map(|v| v as f32 * 5.0).collect(),
        "vr_nav_width" => (12..=65).map(|v| v as f32 * 0.01).collect(),
        "vr_nav_yaw" | "vr_nav_roll" => (-90..=90).map(|v| v as f32 * 2.0).collect(),
        "vr_nav_tilt" => (-40..=40).map(|v| v as f32 * 2.0).collect(),
        "vr_nav_opacity" => (6..=20).map(|v| v as f32 * 0.05).collect(),
        "speed" => SPEEDS.iter().map(|&v| v as f32).collect(),
        "traffic" => TRAFFIC.iter().map(|&v| v as f32).collect(),
        "pax" => PAX.to_vec(),
        "volume" => VOLUME.to_vec(),
        "led_glow" => (0..16).map(|v| v as f32).collect(),
        "night_brightness" => (0..=12).map(|v| v as f32 * 0.25).collect(),
        "fsr_sharpness" => (0..=20).map(|v| v as f32 * 0.05).collect(),
        "reflections" => (0..=20).map(|v| v as f32 / 20.0).collect(),
        "led_mips" => (0..=80).map(|v| v as f32 * 0.05).collect(),
        "ui_scale" => (10..=40).map(|v| v as f32 * 0.05).collect(),
        "chat_size" => (5..=30).map(|v| v as f32 * 0.1).collect(),
        "ui_opacity" => (4..=20).map(|v| v as f32 * 0.05).collect(),
        "voice_volume" | "voice_mic_gain" => (0..=40).map(|v| v as f32 * 0.05).collect(),
        "vol_ai" | "vol_scenery" | "vol_ambient" | "radio_speaker" | "radio_hiss" | "radio_dropouts" => (0..=20).map(|v| v as f32 * 0.05).collect(),
        "wheel_range" => (6..=60).map(|v| v as f32 * 30.0).collect(),
        "wheel_lock" => std::iter::once(0.0).chain((2..=60).map(|v| v as f32 * 30.0)).collect(),
        "fov" => std::iter::once(0.0).chain((20..=120).map(|v| v as f32)).collect(),
        "steer_look_angle" => (0..=60).map(|v| v as f32).collect(),
        "steer_look_response" => (1..=20).map(|v| v as f32 * 0.05).collect(),
        "head_idle" => (0..=20).map(|v| v as f32 * 0.05).collect(),
        "head_idle_pace" => (10..=40).map(|v| v as f32 * 0.05).collect(),
        "head_tracking_smoothing" | "head_tracking_dead_zone" => (0..=20).map(|v| v as f32 * 0.05).collect(),
        "head_tracking_yaw_sens" | "head_tracking_pitch_sens" | "head_tracking_roll_sens" | "head_tracking_x_sens" | "head_tracking_y_sens" | "head_tracking_z_sens" => (0..=100).map(|v| v as f32).collect(),
        "pedal_t" | "pedal_b" => PEDAL.to_vec(),
        "ctrl_deadzone" => (0..=30).map(|v| v as f32 * 0.01).collect(),
        "mouse_sens" => (10..=300).map(|v| v as f32 / 100.0).collect(),
        "key_pedal_speed" => (5..=40).map(|v| v as f32 * 0.05).collect(),
        "mouse_pedal_strength" => (10..=40).map(|v| v as f32 * 0.05).collect(),
        "look_sens" => (2..=40).map(|v| v as f32 * 0.05).collect(),
        "look_smoothing_ms" => (0..=20).map(|v| v as f32 * 10.0).collect(),
        "pad_steer_smooth" => (0..=30).map(|v| v as f32 * 10.0).collect(),
        "pad_steer_speed" => (8..=50).map(|v| v as f32 / 10.0).collect(),
        "pad_deadzone" => (0..=40).map(|v| v as f32 * 0.01).collect(),
        "seat" => (-50..=50).map(|v| v as f32 / 100.0).collect(),
        "seat_pitch" => (-45..=45).map(|v| v as f32).collect(),
        "hour" => (0..24).map(|v| v as f32).collect(),
        "minute" => (0..60).map(|v| v as f32).collect(),
        // the weather, made by hand
        // (visibility goes by a few percent at a time from 100 m to 50 km, on whole tens of metres)
        "visibility" => {
            let mut v: Vec<f32> = (0..=120)
                .map(|i| {
                    let x = 100.0 * 500f32.powf(i as f32 / 120.0);
                    let step = if x < 1000.0 { 10.0 } else if x < 10000.0 { 100.0 } else { 500.0 };
                    (x / step).round() * step
                })
                .collect();
            v.dedup();
            v
        }
        "rain_amt" | "wet" => (0..=100).map(|v| v as f32 / 100.0).collect(),
        // the cloud cover: one value from a clear sky to a closed one, in steps of 2 %
        "cloud_cover" => (0..=50).map(|v| v as f32 / 50.0).collect(),
        "brightness" => (0..=30).map(|v| v as f32 * 0.05).collect(),
        "humidity" => (0..=100).map(|v| v as f32).collect(),
        "temp" => (-20..=45).map(|v| v as f32).collect(),
        "wind_speed" => (0..=25).map(|v| v as f32).collect(),
        "wind_dir" => (0..360).map(|v| v as f32).collect(),
        _ => return None,
    })
}

/// The kinds of precipitation of a weather file (`[precip]`'s first number).
pub(super) const PRECIP_KINDS: [&str; 3] = ["None", "Rain", "Snow"];

/// The name the weather has once it was set by hand.
pub(crate) const CUSTOM_WEATHER: &str = "Custom weather";

/// The index of the cloud type `kind` (a weather file's) in `CLOUD_KINDS`.
pub(super) fn cloud_index(kind: &str) -> Option<usize> {
    crate::weather_setup::cloud_kind_index(kind)
}

/// Set the kind of precipitation (an index of `PRECIP_KINDS`) of the weather set by hand.
pub(super) fn custom_state(app:&App)->crate::weather_setup::CustomWeather{
    if let Some(mut c)=crate::weather_setup::custom_weather(app.args.weather.as_deref()){
        // Wetness keeps evolving while driving; never restore an old serialized value just
        // because another custom field (brightness, humidity, etc.) was edited.
        c.road_wetness=app.session.wetness;
        return c
    }
    match app.session.weather.as_ref(){
        Some(w)=>crate::weather_setup::CustomWeather::from_weather(w,1.0,app.session.wetness),
        None=>crate::weather_setup::CustomWeather::default(),
    }
}

pub(crate) fn set_precip(app: &mut App, to: usize) {
    let to = to.min(PRECIP_KINDS.len() - 1);
    app.edit_weather(|w| {
        w.precip[0] = to as f32;
        w.snow = to == 2;
        // (rain or snow with no strength would be nothing: a moderate one)
        if to != 0 && w.precip[1] < 1.0 {
            w.precip[1] = 100.0;
        }
    });
}

/// Whether `verb` is a slider (a click on its track sets the value there).
pub(crate) fn is_slider(verb: &str) -> bool {
    steps_of(verb).is_some()
}

/// The index of the value in `steps` nearest to `now`.
pub(super) fn nearest(steps: &[f32], now: f32) -> usize {
    steps.iter().enumerate().min_by(|a, b| (a.1 - now).abs().total_cmp(&(b.1 - now).abs())).map(|x| x.0).unwrap_or(0)
}

/// The value of `steps` after moving from `now`.
pub(super) fn step_move(steps: &[f32], now: f32, mv: Move) -> f32 {
    let n = steps.len().max(1);
    let i = nearest(steps, now);
    let to = match mv {
        Move::Next => (i + 1) % n,
        Move::Inc => (i + 1).min(n - 1),
        Move::Dec => i.saturating_sub(1),
        Move::To(f) => (f.clamp(0.0, 1.0) * (n - 1) as f32).round() as usize,
    };
    steps.get(to).copied().unwrap_or(now)
}

/// Keep the physical triple-screen calibration separate from the single-screen FOV.
pub(super) fn set_camera_fov(settings: &mut crate::settings::Settings, value: f32) -> (&'static str, String) {
    let value = if value < 20.0 {
        0.0
    } else {
        value.round().min(120.0)
    };
    if settings.triple.enabled && !settings.vr_requested() {
        settings.triple.fov_deg = value;
        ("triple_fov_deg", value.to_string())
    } else {
        settings.fov = value;
        ("fov", value.to_string())
    }
}
