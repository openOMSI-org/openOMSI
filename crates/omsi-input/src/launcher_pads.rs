type Reply = std::sync::Arc<dyn Fn(&serde_json::Value, serde_json::Value, &[u8]) + Send + Sync>;

pub struct Paths { pub read_root: fn() -> std::path::PathBuf }
use crate::controllers::{self, Connected, DeviceCfg, Devices, FfTune, Func};
use crate::ffb_calibration::Calibration;
use crate::gamepad_profile::{self, PadKind};
use serde_json::{json, Value};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_secs(15);
static STOPPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub struct Pads {
    tx: Sender<(Value, Reply)>,
}

impl Pads {
    pub fn start(paths: Paths) -> Pads {
        let (tx, rx) = channel();
        std::thread::Builder::new().name("launcher controllers".into()).spawn(move || run(rx, paths)).ok();
        Pads { tx }
    }

    pub fn ask(&self, req: Value, reply: Reply) {
        if req.get("op").and_then(Value::as_str) == Some("ffb_stop") {
            STOPPED.store(req.get("token").and_then(Value::as_u64).unwrap_or(0), std::sync::atomic::Ordering::Relaxed);
            return reply(&req.get("id").cloned().unwrap_or(Value::Null), json!({ "ok": true }), &[]);
        }
        if let Err(e) = self.tx.send((req, reply)) {
            let (req, reply) = e.0;
            reply(&req.get("id").cloned().unwrap_or(Value::Null), json!({ "ok": false, "error": "the controllers' thread ended" }), &[]);
        }
    }
}

fn run(rx: Receiver<(Value, Reply)>, paths: Paths) {
    let mut window: Option<Option<isize>> = None;
    let mut io: Option<Devices> = None;
    let mut pressed: Vec<(String, usize)> = Vec::new();
    let mut rests = crate::auto_setup::Rests::default();
    let mut saved: Vec<DeviceCfg> = Vec::new();
    let mut asked = Instant::now();
    loop {
        let wait = match (&io, window) {
            (Some(_), _) => Duration::from_millis(20),
            (None, Some(Some(_))) => Duration::from_millis(100),
            _ => Duration::from_secs(1),
        };
        match rx.recv_timeout(wait) {
            Ok((req, reply)) => {
                asked = Instant::now();
                let id = req.get("id").cloned().unwrap_or(Value::Null);
                let hwnd = *window.get_or_insert_with(crate::background::window);
                let answer = match req.get("op").and_then(Value::as_str).unwrap_or("") {
                    "load" => Ok(load(&mut saved, paths.read_root)),
                    "state" => {
                        let d = io.get_or_insert_with(|| Devices::new(hwnd, false));
                        poll(d, &mut pressed);
                        watch(&mut rests, d);
                        Ok(state(d, std::mem::take(&mut pressed), &rests, &saved, &off(&req)))
                    }
                    "save" => save(&req, &mut saved),
                    "ffb_test" => {
                        io = None;
                        pressed.clear();
                        rests = Default::default();
                        ffb_test(&req, hwnd)
                    }
                    "release" => {
                        io = None;
                        pressed.clear();
                        rests = Default::default();
                        Ok(json!({}))
                    }
                    other => Err(format!("unknown controllers request {other}")),
                };
                let head = match answer {
                    Ok(mut v) => {
                        v["ok"] = json!(true);
                        v
                    }
                    Err(e) => json!({ "ok": false, "error": e }),
                };
                reply(&id, head, &[]);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        crate::background::pump();
        if let Some(d) = io.as_mut() {
            poll(d, &mut pressed);
            watch(&mut rests, d);
            if asked.elapsed() > IDLE {
                io = None;
                pressed.clear();
                rests = Default::default();
            }
        }
    }
}

fn poll(d: &mut Devices, pressed: &mut Vec<(String, usize)>) {
    pressed.extend(d.poll().into_iter().filter(|(_, _, down)| *down).map(|(name, n, _)| (name, n)));
    let over = pressed.len().saturating_sub(64);
    pressed.drain(..over);
}

fn load(saved: &mut Vec<DeviceCfg>, read_root: fn() -> std::path::PathBuf) -> Value {
    let root = read_root();
    let (devices, error) = match controllers::read_cfg_checked(&root) {
        Ok(d) => (d, None),
        Err(e) => (Vec::new(), (controllers::cfg_path(&root).exists()).then_some(e)),
    };
    saved.clone_from(&devices);
    let pad_buttons: Vec<Value> = gamepad_profile::PRESET_BUTTONS
        .iter()
        .filter_map(|b| {
            let action = gamepad_profile::default_action(*b)?;
            Some(json!({
                "action": action,
                "xbox": PadKind::Xbox.label(*b),
                "ps4": PadKind::Ps4.label(*b),
                "ps5": PadKind::Ps5.label(*b),
            }))
        })
        .collect();
    json!({
        "devices": devices.iter().map(device_json).collect::<Vec<_>>(),
        "path": controllers::cfg_path(&root).to_string_lossy(),
        "error": error,
        "pad_buttons": pad_buttons,
    })
}

fn watch(rests: &mut crate::auto_setup::Rests, d: &Devices) {
    for c in d.connected() {
        rests.watch(&c);
    }
}

fn off(req: &Value) -> Vec<String> {
    req.get("off").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect()
}

fn state(d: &Devices, pressed: Vec<(String, usize)>, rests: &crate::auto_setup::Rests, saved: &[DeviceCfg], off: &[String]) -> Value {
    let all = d.connected();
    let on: Vec<Connected> = all.iter().filter(|c| !off.iter().any(|o| controllers::names_match(o, &c.name))).cloned().collect();
    // (a gamepad drives with its own layout; a device of buttons alone has nothing to guess)
    let fresh: Vec<(&Connected, &[(usize, f32)])> = on.iter().filter(|c| !c.gamepad && !d.buttons_only(&c.name) && !saved.iter().any(|s| controllers::names_match(&s.name, &c.name))).filter_map(|c| Some((c, rests.of(&c.name)?))).collect();
    let autos = crate::auto_setup::guess_all(&fresh, controllers::wheel_set_up(saved, &on));
    let connected: Vec<Value> = all.iter().map(|c| connected_json(d, c, autos.iter().find(|a| a.name == c.name))).collect();
    json!({ "connected": connected, "pressed": pressed })
}

fn connected_json(d: &Devices, c: &Connected, auto: Option<&DeviceCfg>) -> Value {
    json!({
        "auto": auto.map(|a| json!({ "device": device_json(a), "what": crate::auto_setup::describe(a) })),
        "name": c.name,
        "axes": c.axes,
        "buttons": c.buttons,
        "gamepad": c.gamepad,
        "ff": c.ff_capable,
        "buttons_only": d.buttons_only(&c.name),
        "pad": PadKind::detect(&c.name, c.hardware_id).map(|k| match k {
            PadKind::Xbox => "xbox",
            PadKind::Ps4 => "ps4",
            PadKind::Ps5 => "ps5",
        }),
    })
}

fn device_json(d: &DeviceCfg) -> Value {
    json!({
        "name": d.name,
        "second": d.second,
        "axes": (0..8).map(|a| json!([Func::code(d.axes[a].map(|x| x.0)), d.axes[a].is_some_and(|x| x.1), d.axis_flags[a]])).collect::<Vec<_>>(),
        "buttons": d.buttons,
        "ff_scale": d.ff_scale.map(|(a, b)| [round3(a), round3(b)]),
        "ff_tune": d.ff_tune.map(|t| TUNE_KEYS.iter().zip(t.values()).map(|(k, v)| (k.to_string(), json!(round3(v)))).collect::<serde_json::Map<_, _>>()),
        "ff_invert": d.ff_invert,
        "latching": d.latching,
        "more": d.more,
    })
}

/// `ff_tune`'s keys, in `FfTune::values`' order.
const TUNE_KEYS: [&str; 5] = ["centring", "damping", "parking", "min_force", "smoothing"];

fn round3(v: f32) -> f64 {
    (v as f64 * 1000.0).round() / 1000.0
}

fn device_from(v: &Value) -> Result<DeviceCfg, String> {
    let name = v.get("name").and_then(Value::as_str).filter(|n| !n.trim().is_empty() && !n.contains(['\r', '\n'])).ok_or("a device without a name")?;
    let mut d = DeviceCfg { name: name.to_string(), second: v.get("second").and_then(Value::as_str).filter(|s| !s.contains(['\r', '\n'])).unwrap_or("0").to_string(), ..Default::default() };
    for (a, axis) in v.get("axes").and_then(Value::as_array).into_iter().flatten().take(8).enumerate() {
        let func = axis.get(0).and_then(Value::as_i64).unwrap_or(-1) as i32;
        let reversed = axis.get(1).and_then(Value::as_bool).unwrap_or(false);
        d.axes[a] = Func::from_code(func).map(|f| (f, reversed));
        d.axis_flags[a] = axis.get(2).and_then(Value::as_i64).unwrap_or(0) as i32 & !1;
    }
    for b in v.get("buttons").and_then(Value::as_array).into_iter().flatten().take(512) {
        let action = b.get(0).and_then(Value::as_str).unwrap_or("").trim();
        if action.contains(['\r', '\n']) {
            return Err(format!("{action:?} is not an action"));
        }
        d.buttons.push((action.to_string(), b.get(1).and_then(Value::as_str).unwrap_or("0").to_string()));
    }
    d.ff_scale = v.get("ff_scale").and_then(Value::as_array).and_then(|s| Some((s.first()?.as_f64()? as f32, s.get(1)?.as_f64()? as f32)));
    // (a value missing or no number is its default, and each is brought into its range)
    d.ff_tune = v.get("ff_tune").filter(|t| t.is_object()).map(|t| FfTune::from_values(&TUNE_KEYS.map(|k| t.get(k).and_then(Value::as_f64).map_or(f32::NAN, |v| v as f32))));
    d.ff_invert = v.get("ff_invert").and_then(Value::as_bool);
    d.latching = v.get("latching").and_then(Value::as_array).into_iter().flatten().filter_map(|b| b.as_u64().map(|b| b as usize)).filter(|b| *b < d.buttons.len()).collect();
    d.latching.sort_unstable();
    d.latching.dedup();
    for m in v.get("more").and_then(Value::as_array).into_iter().flatten().take(4096) {
        let (Some(b), Some(action)) = (m.get(0).and_then(Value::as_u64), m.get(1).and_then(Value::as_str).map(str::trim)) else { continue };
        if action.contains(['\r', '\n']) {
            return Err(format!("{action:?} is not an action"));
        }
        let b = b as usize;
        if !action.is_empty() && b < d.buttons.len() && !d.actions_of(b).iter().any(|a| a.eq_ignore_ascii_case(action)) {
            d.more.push((b, action.to_string()));
        }
    }
    Ok(d)
}

fn save(req: &Value, saved: &mut Vec<DeviceCfg>) -> Result<Value, String> {
    let devices = req.get("devices").and_then(Value::as_array).ok_or("no devices")?.iter().map(device_from).collect::<Result<Vec<_>, _>>()?;
    let path = controllers::save_cfg(&devices)?;
    *saved = devices;
    Ok(json!({ "path": path.to_string_lossy() }))
}

fn ffb_test(req: &Value, hwnd: Option<isize>) -> Result<Value, String> {
    let name = req.get("name").and_then(Value::as_str).ok_or("no device")?.to_string();
    let axis = req.get("axis").and_then(Value::as_u64).ok_or("no steering axis")? as usize;
    let strength = req.get("strength").and_then(Value::as_f64).unwrap_or(crate::ffb_calibration::PULSE_FORCE as f64) as f32;
    log::info!("FFB calibration from the launcher: device {name}, raw steering axis {axis}, test strength {:.0}%", strength * 100.0);
    let token = req.get("token").and_then(Value::as_u64).filter(|t| *t != 0);
    let mut d = Devices::new(hwnd, true);
    let mut test = Calibration::new(strength);
    let started = Instant::now();
    while test.result.is_none() && started.elapsed() < Duration::from_secs(8) && token != Some(STOPPED.load(std::sync::atomic::Ordering::Relaxed)) {
        d.poll();
        crate::background::pump();
        let position = controllers::find_connected(&d.connected(), &name).and_then(|c| c.axes.iter().find(|(k, _)| *k == axis).map(|(_, v)| *v));
        if let Some(force) = test.update(started.elapsed().as_secs_f32(), position) {
            if !d.calibration_pulse(&name, axis, force) {
                test.fail("Force feedback is unavailable. Choose the direction manually.");
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    match test.result {
        Some(Ok(invert)) => Ok(json!({ "invert": invert })),
        Some(Err(e)) => Err(e.to_string()),
        None => Err("The wheel did not settle. Let go and try again.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_goes_to_the_page_and_back_unchanged() {
        let text = "[ctrl]\r\nLogitech G25 Racing Wheel USB\r\n1\r\n\r\n[axis]\r\n0\r\n0\r\n1\r\n9\r\n2\r\n1\r\n-1\r\n0\r\n-1\r\n0\r\n3\r\n17\r\n-1\r\n0\r\n-1\r\n0\r\n\r\n[buttons]\r\n3\r\nhorn\r\n0\r\n\r\n0\r\nblinker_left_set\r\n0\r\n\r\n[FFScale]\r\n0.800\r\n1.200\r\n\r\n[openOMSI.FFInvert]\r\n1\r\n\r\n[openOMSI.Latching]\r\n3\r\n";
        let devices = controllers::parse_cfg(text);
        let back: Vec<DeviceCfg> = devices.iter().map(|d| device_from(&device_json(d)).unwrap()).collect();
        assert_eq!(back, devices);
        assert_eq!(back[0].axes[1], Some((Func::Throttle, true)));
        assert_eq!(back[0].axis_flags[1], 8);
        assert_eq!(back[0].latching, vec![2]);
    }

    #[test]
    fn a_button_action_cannot_break_the_file() {
        let v = json!({ "name": "Wheel", "buttons": [["horn\r\n[ctrl]", "0"]] });
        assert!(device_from(&v).is_err());
        assert!(device_from(&json!({ "name": " " })).is_err());
    }

    #[test]
    fn more_actions_reach_the_file_once_and_only_on_bound_buttons() {
        let v = json!({ "name": "Wheel", "buttons": [["door_1", "1"], ["", "0"]], "more": [[0, "door_2"], [0, "door_2"], [0, "DOOR_1"], [0, ""], [5, "horn"]] });
        let d = device_from(&v).unwrap();
        assert_eq!(d.more, vec![(0, "door_2".to_string())]);
        assert!(d.continuous(0));
        let back = crate::controllers::parse_cfg(&crate::controllers::cfg_text(&[d.clone()]));
        assert_eq!(back[0].more, d.more);
        assert_eq!(back[0].actions_of(0), ["door_1", "door_2"]);
        assert_eq!(device_json(&back[0])["more"], json!([[0, "door_2"]]));
    }

    #[test]
    fn a_wheels_force_feedback_tuning_goes_to_the_page_and_back() {
        let text = "\r\n[ctrl]\r\nWheel\r\n1\r\n\r\n[FFScale]\r\n1.000\r\n1.000\r\n\r\n[openOMSI.FFTuning]\r\n0.800\r\n2.500\r\n0.000\r\n0.050\r\n0.020\r\n\r\n";
        let devices = controllers::parse_cfg(text);
        let json = device_json(&devices[0]);
        assert_eq!(json["ff_tune"], json!({ "centring": 0.8, "damping": 2.5, "parking": 0.0, "min_force": 0.05, "smoothing": 0.02 }));
        let back = device_from(&json).unwrap();
        assert_eq!(back, devices[0]);
        assert_eq!(controllers::cfg_text(&[back]), controllers::cfg_text(&devices));
        // a page that knows nothing of it, or has it at null, leaves the defaults
        assert_eq!(device_json(&controllers::parse_cfg("[ctrl]\r\nWheel\r\n1\r\n")[0])["ff_tune"], Value::Null);
        assert_eq!(device_from(&json!({ "name": "Wheel" })).unwrap().ff_tune, None);
        assert_eq!(device_from(&json!({ "name": "Wheel", "ff_tune": null })).unwrap().ff_tune, None);
        // and what it sends is kept in range, a value it left out at its default
        let wild = device_from(&json!({ "name": "Wheel", "ff_tune": { "centring": 9.0, "damping": -1.0, "min_force": 0.5, "smoothing": "soft" } })).unwrap();
        assert_eq!(wild.ff_tune, Some(FfTune { centring: 2.0, damping: 0.0, parking: 1.0, min_force: 0.2, smoothing: 0.0 }));
    }

    #[test]
    fn latching_buttons_past_the_list_are_dropped() {
        let v = json!({ "name": "Wheel", "buttons": [["horn", "0"], ["", "0"]], "latching": [1, 1, 5] });
        assert_eq!(device_from(&v).unwrap().latching, vec![1]);
    }
}
