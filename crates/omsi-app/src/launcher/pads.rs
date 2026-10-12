use super::service::Reply;
use omsi_input::controllers::{self, Connected, DeviceCfg, Devices, Func};
use omsi_input::ffb_calibration::Calibration;
use omsi_input::gamepad_profile::{self, PadKind};
use serde_json::{json, Value};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_secs(15);

pub struct Pads {
    tx: Sender<(Value, Reply)>,
}

impl Pads {
    pub fn start() -> Pads {
        let (tx, rx) = channel();
        std::thread::Builder::new().name("launcher controllers".into()).spawn(move || run(rx)).ok();
        Pads { tx }
    }

    pub fn ask(&self, req: Value, reply: Reply) {
        if let Err(e) = self.tx.send((req, reply)) {
            let (req, reply) = e.0;
            reply(&req.get("id").cloned().unwrap_or(Value::Null), json!({ "ok": false, "error": "the controllers' thread ended" }), &[]);
        }
    }
}

fn run(rx: Receiver<(Value, Reply)>) {
    let mut window: Option<Option<isize>> = None;
    let mut io: Option<Devices> = None;
    let mut pressed: Vec<(String, usize)> = Vec::new();
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
                let hwnd = *window.get_or_insert_with(omsi_input::background::window);
                let answer = match req.get("op").and_then(Value::as_str).unwrap_or("") {
                    "load" => Ok(load()),
                    "state" => {
                        let d = io.get_or_insert_with(|| Devices::new(hwnd, false));
                        poll(d, &mut pressed);
                        Ok(state(d, std::mem::take(&mut pressed)))
                    }
                    "save" => save(&req),
                    "ffb_test" => {
                        io = None;
                        pressed.clear();
                        ffb_test(&req, hwnd)
                    }
                    "release" => {
                        io = None;
                        pressed.clear();
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
        omsi_input::background::pump();
        if let Some(d) = io.as_mut() {
            poll(d, &mut pressed);
            if asked.elapsed() > IDLE {
                io = None;
                pressed.clear();
            }
        }
    }
}

fn poll(d: &mut Devices, pressed: &mut Vec<(String, usize)>) {
    pressed.extend(d.poll().into_iter().filter(|(_, _, down)| *down).map(|(name, n, _)| (name, n)));
    let over = pressed.len().saturating_sub(64);
    pressed.drain(..over);
}

fn load() -> Value {
    let _ = omsi_launcher_lib::content_dir();
    let root = omsi_launcher_lib::root_dir().unwrap_or_default();
    let (devices, error) = match controllers::read_cfg_checked(&root) {
        Ok(d) => (d, None),
        Err(e) => (Vec::new(), (controllers::cfg_path(&root).exists()).then_some(e)),
    };
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

fn state(d: &Devices, pressed: Vec<(String, usize)>) -> Value {
    let connected: Vec<Value> = d.connected().iter().map(|c| connected_json(d, c)).collect();
    json!({ "connected": connected, "pressed": pressed })
}

fn connected_json(d: &Devices, c: &Connected) -> Value {
    json!({
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
        "ff_invert": d.ff_invert,
        "latching": d.latching,
    })
}

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
    d.ff_invert = v.get("ff_invert").and_then(Value::as_bool);
    d.latching = v.get("latching").and_then(Value::as_array).into_iter().flatten().filter_map(|b| b.as_u64().map(|b| b as usize)).filter(|b| *b < d.buttons.len()).collect();
    d.latching.sort_unstable();
    d.latching.dedup();
    Ok(d)
}

fn save(req: &Value) -> Result<Value, String> {
    let devices = req.get("devices").and_then(Value::as_array).ok_or("no devices")?.iter().map(device_from).collect::<Result<Vec<_>, _>>()?;
    let base = omsi_launcher_lib::content_dir()
        .filter(|c| c.join("Inputs").exists() || std::fs::create_dir_all(c.join("Inputs")).is_ok())
        .filter(|c| omsi_cfg::is_writable(&c.join("Inputs")))
        .unwrap_or_else(|| omsi_launcher_lib::data_dir().join("content"));
    controllers::save_cfg(Some(base.clone()), &devices)?;
    Ok(json!({ "path": base.join("Inputs").join("gamectrler.cfg").to_string_lossy() }))
}

fn ffb_test(req: &Value, hwnd: Option<isize>) -> Result<Value, String> {
    let name = req.get("name").and_then(Value::as_str).ok_or("no device")?.to_string();
    let axis = req.get("axis").and_then(Value::as_u64).ok_or("no steering axis")? as usize;
    let strength = req.get("strength").and_then(Value::as_f64).unwrap_or(omsi_input::ffb_calibration::PULSE_FORCE as f64) as f32;
    log::info!("FFB calibration from the launcher: device {name}, raw steering axis {axis}, test strength {:.0}%", strength * 100.0);
    let mut d = Devices::new(hwnd, true);
    let mut test = Calibration::new(strength);
    let started = Instant::now();
    while test.result.is_none() && started.elapsed() < Duration::from_secs(8) {
        d.poll();
        omsi_input::background::pump();
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
    fn latching_buttons_past_the_list_are_dropped() {
        let v = json!({ "name": "Wheel", "buttons": [["horn", "0"], ["", "0"]], "latching": [1, 1, 5] });
        assert_eq!(device_from(&v).unwrap().latching, vec![1]);
    }
}
