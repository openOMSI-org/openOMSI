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
}

struct Entry {
    ids: Vec<(u16, u16)>,
    platforms: Vec<String>,
    pedals: Vec<Pedal>,
    ffb_flipped: bool,
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
        let axis = p.get("axis")?.as_u64()? as usize;
        if axis >= 8 {
            log::error!("wheel defaults: an axis is out of range");
            return None;
        }
        pedals.push(Pedal { axis, func: func_of(p.get("function")?.as_str()?)? });
    }
    Some(Entry { ids, platforms, pedals, ffb_flipped: v.get("ffb").and_then(Value::as_str) == Some("flipped") })
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

/// Pedal functions when a file lists exactly these resting axes for this id and platform.
pub(crate) fn pedals_for(id: Option<(u16, u16)>, resting: &[usize]) -> Option<Vec<(usize, Func)>> {
    pedals_for_on(platform(), id, resting)
}

pub(crate) fn pedals_for_on(platform: &str, id: Option<(u16, u16)>, resting: &[usize]) -> Option<Vec<(usize, Func)>> {
    let id = id?;
    let mut slots = resting.to_vec();
    slots.sort_unstable();
    slots.dedup();
    entries().iter().find(|e| matches(e, platform, id) && same_axes(e, &slots)).map(|e| e.pedals.iter().map(|p| (p.axis, p.func)).collect())
}

fn same_axes(entry: &Entry, slots: &[usize]) -> bool {
    if entry.pedals.is_empty() {
        return false;
    }
    let mut axes: Vec<usize> = entry.pedals.iter().map(|p| p.axis).collect();
    axes.sort_unstable();
    axes.dedup();
    axes == slots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_moved_lists_keep_their_behaviour() {
        let g29 = Some((0x046d, 0xc24f));
        let g923 = Some((0x046d, 0xc266));
        let slots = [1, 2, 5];
        assert_eq!(
            pedals_for_on("linux", g29, &slots).unwrap(),
            vec![(1, Func::Clutch), (2, Func::Throttle), (5, Func::Brake)]
        );
        assert!(pedals_for_on("macos", g923, &slots).is_some());
        assert!(pedals_for_on("windows", g29, &slots).is_none());
        assert!(pedals_for_on("linux", g29, &[1, 5]).is_none());
        assert!(ffb_flipped_on("windows", g923));
        assert!(!ffb_flipped_on("windows", g29));
        assert!(!ffb_flipped_on("linux", g923));
        assert!(!ffb_flipped(None));
    }
}
