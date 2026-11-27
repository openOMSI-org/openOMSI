#[cfg(any(target_os = "linux", target_os = "macos", test))]
use super::button_index;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use super::declared_buttons;
use super::{HAT_BUTTONS, code_button};

fn slots_file() -> Option<std::path::PathBuf> {
    omsi_cfg::own_input("button_slots.cfg")
}

type Slots = Vec<(String, Vec<(u32, Option<usize>)>)>;

fn load_slots_from(path: &std::path::Path) -> Slots {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out: Slots = Vec::new();
    for line in content.lines() {
        let mut parts = line.split('|');
        let (Some(dev), Some(code_str)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Ok(code) = u32::from_str_radix(code_str.trim(), 16) else {
            continue;
        };
        let saved = parts.next().and_then(|v| v.trim().parse::<usize>().ok());
        if let Some(entry) = out.iter_mut().find(|(n, _)| n == dev.trim()) {
            match entry.1.iter_mut().find(|(c, _)| *c == code) {
                Some((_, id)) if id.is_none() => *id = saved,
                Some(_) => {}
                None => entry.1.push((code, saved)),
            }
        } else {
            out.push((dev.trim().to_string(), vec![(code, saved)]));
        }
    }
    out
}

fn append_slot_to(path: &std::path::Path, device: &str, code: u32, slot: usize) {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{}|{:x}|{}", device.trim(), code, slot);
    }
}

pub(super) fn stable_unknown_button(device: &str, code: u32) -> Option<usize> {
    stable_unknown_button_in(None, device, code)
}

pub(crate) fn stable_unknown_button_in(
    file: Option<&std::path::Path>,
    device: &str,
    code: u32,
) -> Option<usize> {
    static SLOTS: std::sync::Mutex<Option<Slots>> = std::sync::Mutex::new(None);
    let mut slots_guard = SLOTS.lock().unwrap_or_else(|e| e.into_inner());
    let path = file.map(|p| p.to_path_buf()).or_else(slots_file);
    if file.is_none() && slots_guard.is_none() {
        *slots_guard = Some(path.as_deref().map(load_slots_from).unwrap_or_default());
    }
    if file.is_some() {
        let mut slots = path.as_deref().map(load_slots_from).unwrap_or_default();
        return resolve_slot(&mut slots, path.as_deref(), device, code);
    }
    resolve_slot(slots_guard.as_mut().unwrap(), path.as_deref(), device, code)
}

fn resolve_slot(
    slots: &mut Slots,
    path: Option<&std::path::Path>,
    device: &str,
    code: u32,
) -> Option<usize> {
    let reserved = declared_button_ids(device);
    let entry = match slots.iter_mut().find(|(n, _)| n == device) {
        Some(e) => &mut e.1,
        None => {
            slots.push((device.to_string(), Vec::new()));
            &mut slots.last_mut().unwrap().1
        }
    };
    let pos = match entry.iter().position(|(c, _)| *c == code) {
        Some(i) => i,
        None => {
            let used: Vec<usize> = entry
                .iter()
                .enumerate()
                .filter_map(|(i, (_, id))| {
                    id.or_else(|| {
                        let slot = nth_available_slot(&reserved, i);
                        (slot < HAT_BUTTONS).then_some(slot)
                    })
                })
                .collect();
            let mut taken = reserved.clone();
            taken.extend(used);
            let slot = nth_available_slot(&taken, 0);
            entry.push((code, Some(slot)));
            if let Some(p) = path {
                append_slot_to(p, device, code, slot);
            }
            return (slot < HAT_BUTTONS).then_some(slot);
        }
    };
    match entry[pos].1 {
        Some(id) if id >= HAT_BUTTONS || reserved.contains(&id) => {
            log::warn!(
                "controller {device}: saved fallback button ID {id} conflicts with a known button; ignoring that fallback mapping"
            );
            None
        }
        Some(id) => Some(id),
        None => {
            let saved = entry.iter().enumerate().filter_map(|(i, (_, id))| {
                (*id).or_else(|| (i < pos).then(|| nth_available_slot(&reserved, i)))
            });
            let used: Vec<_> = reserved.iter().copied().chain(saved).collect();
            let slot = nth_available_slot(&used, 0);
            if slot < HAT_BUTTONS {
                entry[pos].1 = Some(slot);
                if let Some(p) = path {
                    append_slot_to(p, device, code, slot);
                }
            }
            (slot < HAT_BUTTONS).then_some(slot)
        }
    }
}

fn nth_available_slot(reserved: &[usize], pos: usize) -> usize {
    let mut slot = 82;
    let mut available_count = 0;
    loop {
        if !reserved.contains(&slot) {
            if available_count == pos {
                return slot;
            }
            available_count += 1;
        }
        slot += 1;
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
pub(crate) fn declared_button_ids(name: &str) -> Vec<usize> {
    with_declared(name, |codes| {
        codes
            .iter()
            .filter_map(|&c| {
                if cfg!(target_os = "macos") || (c >> 16 == 9) {
                    code_button(c)
                } else {
                    button_index(codes, c & 0xFFFF).or_else(|| code_button(c))
                }
            })
            .collect()
    })
    .unwrap_or_default()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", test)))]
pub(crate) fn declared_button_ids(_name: &str) -> Vec<usize> {
    Vec::new()
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
static DECLARED: std::sync::Mutex<Vec<(String, Option<Vec<u32>>)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(any(target_os = "linux", target_os = "macos", test))]
pub(super) fn with_declared<R>(name: &str, f: impl FnOnce(&[u32]) -> R) -> Option<R> {
    let mut cache = DECLARED.lock().unwrap_or_else(|e| e.into_inner());
    if !cache.iter().any(|(n, _)| n == name) {
        #[cfg(target_os = "linux")]
        let codes = declared_buttons(name);
        #[cfg(target_os = "macos")]
        let codes = declared_buttons(name);
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let codes = None;
        cache.push((name.to_string(), codes));
    }
    cache.iter().find(|(n, _)| n == name)?.1.as_deref().map(f)
}

#[cfg(test)]
pub(crate) fn set_declared_buttons(name: &str, codes: Option<Vec<u32>>) {
    let mut cache = DECLARED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = cache.iter_mut().find(|(n, _)| n == name) {
        entry.1 = codes;
    } else {
        cache.push((name.to_string(), codes));
    }
}
