//! Pedal order and force direction from `defaults/*.json`. A device that already has a
//! setup is left alone. `ffb: flipped` is the motor's direction when the force is sent,
//! and is not written into the player's invert.

use std::sync::OnceLock;

use serde_json::Value;

use crate::controllers::Func;

include!(concat!(env!("OUT_DIR"), "/wheel_defaults_files.rs"));

struct Pedal {
    axis: usize,
    func: Func,
    optional: bool,
}

struct Entry {
    ids: Vec<(u16, u16)>,
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
    ENTRIES.get_or_init(|| FILES.iter().filter_map(|text| parse_file(text)).collect())
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

fn parse_file(text: &str) -> Option<Entry> {
    let v: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            log::error!("wheel defaults: {e}");
            return None;
        }
    };
    let ids = v.get("id")?.as_array()?.iter().filter_map(|id| id.as_str().and_then(parse_id)).collect::<Vec<_>>();
    let platforms = match v.get("platform")? {
        Value::String(s) => vec![s.clone()],
        Value::Array(a) => a.iter().filter_map(|p| p.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    };
    if ids.is_empty() || platforms.is_empty() {
        log::error!("wheel defaults: an entry has no id or platform");
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
        ids,
        platforms,
        wheel: v.get("wheel").and_then(axis_of),
        pedals,
        ffb_flipped: v.get("ffb").and_then(Value::as_str) == Some("flipped"),
    })
}

fn matches(entry: &Entry, platform: &str, id: (u16, u16)) -> bool {
    entry.platforms.iter().any(|p| p == platform) && entry.ids.contains(&id)
}

pub(crate) fn ffb_flipped(id: Option<(u16, u16)>) -> bool {
    ffb_flipped_on(platform(), id)
}

pub(crate) fn ffb_flipped_on(platform: &str, id: Option<(u16, u16)>) -> bool {
    let Some(id) = id else { return false };
    entries().iter().any(|e| e.ffb_flipped && matches(e, platform, id))
}

/// The file's wheel axis and the pedals that are resting, when every resting pedal is listed
/// and every pedal that is not optional is resting.
pub(crate) fn axes_for(id: Option<(u16, u16)>, resting: &[usize]) -> Option<(Option<usize>, Vec<(usize, Func)>)> {
    axes_for_on(platform(), id, resting)
}

pub(crate) fn axes_for_on(platform: &str, id: Option<(u16, u16)>, resting: &[usize]) -> Option<(Option<usize>, Vec<(usize, Func)>)> {
    let id = id?;
    let mut slots = resting.to_vec();
    slots.sort_unstable();
    slots.dedup();
    let entry = entries().iter().find(|e| matches(e, platform, id) && covers(e, &slots))?;
    let pedals = entry.pedals.iter().filter(|p| slots.contains(&p.axis)).map(|p| (p.axis, p.func)).collect();
    Some((entry.wheel, pedals))
}

/// Every resting slot is listed, and every pedal that is not optional is resting.
fn covers(entry: &Entry, slots: &[usize]) -> bool {
    if entry.pedals.is_empty() {
        return false;
    }
    slots.iter().all(|s| entry.pedals.iter().any(|p| p.axis == *s)) && entry.pedals.iter().filter(|p| !p.optional).all(|p| slots.contains(&p.axis))
}

/// Axes the setup guide can offer. None when this id has no wheel or pedals on this platform.
pub(crate) fn preset(id: Option<(u16, u16)>) -> Option<Preset> {
    let id = id?;
    let entry = entries().iter().find(|e| matches(e, platform(), id) && (e.wheel.is_some() || !e.pedals.is_empty()))?;
    Some(Preset {
        wheel: entry.wheel,
        pedals: entry.pedals.iter().map(|p| PresetPedal { axis: p.axis, function: func_name(p.func), optional: p.optional }).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_moved_lists_keep_their_behaviour() {
        let g29 = Some((0x046d, 0xc24f));
        let g923 = Some((0x046d, 0xc266));
        let slots = [1, 2, 5];
        assert_eq!(axes_for_on("linux", g29, &slots).unwrap().1, vec![(1, Func::Clutch), (2, Func::Throttle), (5, Func::Brake)]);
        assert!(axes_for_on("macos", g923, &slots).is_some());
        assert!(axes_for_on("windows", g29, &slots).is_none());
        assert!(axes_for_on("linux", g29, &[1, 5]).is_none());
        assert!(ffb_flipped_on("windows", g923));
        assert!(!ffb_flipped_on("windows", g29));
        assert!(!ffb_flipped_on("linux", g923));
        assert!(!ffb_flipped(None));
        let t300 = Some((0x044f, 0xb66e));
        assert!(ffb_flipped_on("windows", t300));
        assert!(!ffb_flipped_on("linux", t300));
        assert_eq!(
            axes_for_on("windows", t300, &[1, 5]).unwrap(),
            (Some(0), vec![(1, Func::Brake), (5, Func::Throttle)])
        );
        assert_eq!(
            axes_for_on("windows", t300, &[1, 5, 7]).unwrap().1,
            vec![(1, Func::Brake), (5, Func::Throttle), (7, Func::Clutch)]
        );
        assert!(axes_for_on("linux", t300, &[1, 5]).is_none());
        assert!(axes_for_on("windows", t300, &[1, 6]).is_none());
        if cfg!(windows) {
            assert!(preset(t300).is_some());
            assert!(preset(g923).is_none());
        } else {
            assert!(preset(t300).is_none());
            assert!(preset(g923).is_some());
        }
    }
}
