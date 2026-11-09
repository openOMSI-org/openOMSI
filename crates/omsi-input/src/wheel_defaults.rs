//! Steering, pedals and force direction for a wheel, by its USB id and this platform.
//! `wheel_defaults.json` ships with the program. A device that already has a setup is
//! left alone. `ffb: flipped` is the motor's direction, read when the force is sent,
//! and is not written into the player's invert.

use std::sync::OnceLock;

use serde_json::Value;

use crate::controllers::Func;

const FILE: &str = include_str!("wheel_defaults.json");
/// Same rest as `auto_setup`: a pedal sits at an end, a wheel near the middle.
const PEDAL_REST: f32 = 0.9;

struct Pedal {
    axis: usize,
    func: Func,
    optional: bool,
}

struct Entry {
    id: (u16, u16),
    platforms: Vec<String>,
    wheel: Option<usize>,
    pedals: Vec<Pedal>,
    ffb_flipped: bool,
}

pub(crate) struct PresetPedal {
    pub axis: usize,
    pub function: &'static str,
    pub optional: bool,
}

pub(crate) struct Preset {
    pub wheel: Option<usize>,
    pub pedals: Vec<PresetPedal>,
}

fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "other"
    }
}

fn entries() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| match serde_json::from_str::<Value>(FILE) {
        Ok(v) => v.as_array().map(|a| a.iter().filter_map(parse_entry).collect()).unwrap_or_default(),
        Err(e) => {
            log::error!("wheel defaults: {e}");
            Vec::new()
        }
    })
}

fn parse_id(text: &str) -> Option<(u16, u16)> {
    let (vendor, product) = text.split_once(':')?;
    Some((u16::from_str_radix(vendor, 16).ok()?, u16::from_str_radix(product, 16).ok()?))
}

fn func_of(name: &str) -> Option<Func> {
    match name {
        "throttle" => Some(Func::Throttle),
        "brake" => Some(Func::Brake),
        "clutch" => Some(Func::Clutch),
        _ => None,
    }
}

fn func_name(func: Func) -> &'static str {
    match func {
        Func::Throttle => "throttle",
        Func::Brake => "brake",
        Func::Clutch => "clutch",
        _ => "",
    }
}

fn axis_of(v: &Value) -> Option<usize> {
    let n = v.as_u64()? as usize;
    (n < 8).then_some(n)
}

fn parse_entry(v: &Value) -> Option<Entry> {
    let id = parse_id(v.get("id")?.as_str()?)?;
    let platforms = match v.get("platform")? {
        Value::String(s) => vec![s.clone()],
        Value::Array(a) => a.iter().filter_map(|p| p.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    };
    if platforms.is_empty() {
        return None;
    }
    let mut pedals = Vec::new();
    for p in v.get("pedals").and_then(Value::as_array).into_iter().flatten() {
        pedals.push(Pedal {
            axis: axis_of(p.get("axis")?)?,
            func: func_of(p.get("function")?.as_str()?)?,
            optional: p.get("optional").and_then(Value::as_bool).unwrap_or(false),
        });
    }
    Some(Entry {
        id,
        platforms,
        wheel: v.get("wheel").and_then(axis_of),
        pedals,
        ffb_flipped: v.get("ffb").and_then(Value::as_str) == Some("flipped"),
    })
}

fn find(platform: &str, id: Option<(u16, u16)>) -> Option<&'static Entry> {
    let id = id?;
    entries().iter().find(|e| e.id == id && e.platforms.iter().any(|p| p == platform))
}

pub(crate) fn ffb_flipped(id: Option<(u16, u16)>) -> bool {
    ffb_flipped_on(platform(), id)
}

pub(crate) fn ffb_flipped_on(platform: &str, id: Option<(u16, u16)>) -> bool {
    find(platform, id).is_some_and(|e| e.ffb_flipped)
}

/// The entry's axes when every resting pedal is listed and every pedal that is
/// not optional is resting. None: no entry, or the pedals at rest are not that list.
pub(crate) fn assign(id: Option<(u16, u16)>, rest: &[(usize, f32)]) -> Option<Vec<(usize, Func, bool)>> {
    assign_on(platform(), id, rest)
}

pub(crate) fn assign_on(platform: &str, id: Option<(u16, u16)>, rest: &[(usize, f32)]) -> Option<Vec<(usize, Func, bool)>> {
    let entry = find(platform, id)?;
    if entry.pedals.is_empty() {
        return None;
    }
    let mut resting: Vec<(usize, f32)> = rest.iter().copied().filter(|&(k, v)| k < 8 && v.abs() >= PEDAL_REST).collect();
    resting.sort_by_key(|p| p.0);
    resting.dedup_by_key(|p| p.0);
    if resting.iter().any(|(k, _)| entry.pedals.iter().all(|p| p.axis != *k)) {
        return None;
    }
    if entry.pedals.iter().any(|p| !p.optional && !resting.iter().any(|(k, _)| *k == p.axis)) {
        return None;
    }
    let mut out = Vec::new();
    if let Some(wheel) = entry.wheel {
        if rest.iter().find(|(k, _)| *k == wheel).is_some_and(|(_, v)| v.abs() < PEDAL_REST) {
            out.push((wheel, Func::Steering, false));
        }
    }
    for pedal in &entry.pedals {
        if let Some((_, v)) = resting.iter().find(|(k, _)| *k == pedal.axis) {
            out.push((pedal.axis, pedal.func, *v > 0.0));
        }
    }
    Some(out)
}

/// What the setup guide can offer. None when this id has no axes on this platform
/// (a force-only entry is applied when the force is sent, not as a saved setup).
pub(crate) fn preset(id: Option<(u16, u16)>) -> Option<Preset> {
    let entry = find(platform(), id)?;
    if entry.wheel.is_none() && entry.pedals.is_empty() {
        return None;
    }
    Some(Preset {
        wheel: entry.wheel,
        pedals: entry.pedals.iter().map(|p| PresetPedal { axis: p.axis, function: func_name(p.func), optional: p.optional }).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_file_lists_a_wheel() {
        assert!(entries().iter().any(|e| e.id == (0x044f, 0xb66e)));
        assert!(entries().iter().any(|e| e.id == (0x046d, 0xc266) && e.ffb_flipped));
    }

    #[test]
    fn force_and_pedals_follow_the_id_and_the_platform() {
        let t300 = Some((0x044f, 0xb66e));
        let rest = [(0, 0.0), (1, -1.0), (5, -1.0), (7, 1.0)];
        assert!(ffb_flipped_on("windows", t300));
        assert!(!ffb_flipped_on("linux", t300));
        assert_eq!(
            assign_on("windows", t300, &rest).unwrap(),
            vec![(0, Func::Steering, false), (1, Func::Brake, false), (5, Func::Throttle, false), (7, Func::Clutch, true)]
        );
        assert!(assign_on("linux", t300, &rest).is_none());
        assert!(ffb_flipped_on("windows", Some((0x046d, 0xc266))));
        assert!(!ffb_flipped_on("windows", Some((0x046d, 0xc24f))));
        assert!(!ffb_flipped_on("linux", Some((0x046d, 0xc266))));
        let hid = [(0, 0.0), (1, 1.0), (2, 1.0), (5, 1.0)];
        assert_eq!(
            assign_on("linux", Some((0x046d, 0xc24f)), &hid).unwrap(),
            vec![(0, Func::Steering, false), (1, Func::Clutch, true), (2, Func::Throttle, true), (5, Func::Brake, true)]
        );
        assert!(assign_on("windows", Some((0x046d, 0xc24f)), &hid).is_none());
        assert!(assign_on("windows", t300, &[(0, 0.0), (1, -1.0), (6, -1.0)]).is_none());
    }
}
