//! The player's own set-up of each bus's mirrors (`~/.openomsi/mirrors.cfg`).

use super::Settings;

/// The player's own turn of a bus's mirrors (degrees yaw, pitch per `[add_camera_reflexion]`),
/// kept per `.bus` file in `~/.openomsi/mirrors.cfg` as `<bus file>|<mirror>=<yaw>,<pitch>`.
#[rustfmt::skip]
pub fn mirror_offsets(bus: &std::path::Path) -> Vec<[f32; 2]> {
    let key = bus.to_string_lossy().to_ascii_lowercase();
    let Some(p) = Settings::path().map(|p| p.with_file_name("mirrors.cfg")) else { return Vec::new() };
    let text = std::fs::read_to_string(p).unwrap_or_default();
    let mut out: Vec<[f32; 2]> = Vec::new();
    for line in text.lines() {
        let Some((k, v)) = line.rsplit_once('=') else { continue };
        let Some((file, i)) = k.rsplit_once('|') else { continue };
        let (Ok(i), Some((y, p))) = (i.trim().parse::<usize>(), v.split_once(',')) else { continue };
        if file.trim().to_ascii_lowercase() != key || i > 64 {
            continue;
        }
        if out.len() <= i {
            out.resize(i + 1, [0.0; 2]);
        }
        out[i] = [y.trim().parse().unwrap_or(0.0), p.trim().parse().unwrap_or(0.0)];
    }
    out
}

/// A bus's mirror shifts (bus frame, m: across, along, up), kept in the same file as the
/// turns on lines `<bus>|s<n>=x,y,z` (a build that knows only turns skips them).
#[rustfmt::skip]
pub fn mirror_shifts(bus: &std::path::Path) -> Vec<[f32; 3]> {
    let key = bus.to_string_lossy().to_ascii_lowercase();
    let Some(p) = Settings::path().map(|p| p.with_file_name("mirrors.cfg")) else { return Vec::new() };
    let text = std::fs::read_to_string(p).unwrap_or_default();
    let mut out: Vec<[f32; 3]> = Vec::new();
    for line in text.lines() {
        let Some((k, v)) = line.rsplit_once('=') else { continue };
        let Some((file, i)) = k.rsplit_once('|') else { continue };
        let Some(i) = i.trim().strip_prefix('s').and_then(|n| n.parse::<usize>().ok()) else { continue };
        let xyz: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        if file.trim().to_ascii_lowercase() != key || i > 64 || xyz.len() != 3 {
            continue;
        }
        if out.len() <= i {
            out.resize(i + 1, [0.0; 3]);
        }
        out[i] = [xyz[0], xyz[1], xyz[2]];
    }
    out
}

/// A bus's mirror field-of-view changes (degrees added to the mirror camera's), on lines
/// `<bus>|f<n>=degrees` of the same file.
#[rustfmt::skip]
pub fn mirror_fovs(bus: &std::path::Path) -> Vec<f32> {
    let key = bus.to_string_lossy().to_ascii_lowercase();
    let Some(p) = Settings::path().map(|p| p.with_file_name("mirrors.cfg")) else { return Vec::new() };
    let text = std::fs::read_to_string(p).unwrap_or_default();
    let mut out: Vec<f32> = Vec::new();
    for line in text.lines() {
        let Some((k, v)) = line.rsplit_once('=') else { continue };
        let Some((file, i)) = k.rsplit_once('|') else { continue };
        let Some(i) = i.trim().strip_prefix('f').and_then(|n| n.parse::<usize>().ok()) else { continue };
        let Ok(d) = v.trim().parse::<f32>() else { continue };
        if file.trim().to_ascii_lowercase() != key || i > 64 {
            continue;
        }
        if out.len() <= i {
            out.resize(i + 1, 0.0);
        }
        out[i] = d.clamp(-60.0, 60.0);
    }
    out
}

/// Keep a bus's mirror turns, shifts and field-of-view changes (see [`mirror_offsets`],
/// [`mirror_shifts`], [`mirror_fovs`]).
#[rustfmt::skip]
pub fn save_mirror_state(bus: &std::path::Path, offsets: &[[f32; 2]], shifts: &[[f32; 3]], fovs: &[f32]) {
    let key = bus.to_string_lossy().to_ascii_lowercase();
    let Some(p) = Settings::path().map(|p| p.with_file_name("mirrors.cfg")) else { return };
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| l.rsplit_once('=').and_then(|(k, _)| k.rsplit_once('|')).is_none_or(|(f, _)| f.trim().to_ascii_lowercase() != key))
        .map(str::to_string)
        .collect();
    for (i, o) in offsets.iter().enumerate() {
        if o[0].abs() > 0.01 || o[1].abs() > 0.01 {
            lines.push(format!("{}|{i}={:.1},{:.1}", bus.to_string_lossy(), o[0], o[1]));
        }
    }
    for (i, s) in shifts.iter().enumerate() {
        if s.iter().any(|v| v.abs() > 0.002) {
            lines.push(format!("{}|s{i}={:.3},{:.3},{:.3}", bus.to_string_lossy(), s[0], s[1], s[2]));
        }
    }
    for (i, d) in fovs.iter().enumerate() {
        if d.abs() > 0.05 {
            lines.push(format!("{}|f{i}={:.1}", bus.to_string_lossy(), d));
        }
    }
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(&p, lines.join("\n") + "\n");
}
