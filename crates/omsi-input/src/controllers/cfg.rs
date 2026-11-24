use std::path::{Path, PathBuf};

use super::{FfTune, Func};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceCfg {
    pub name: String,
    /// The line after the name (kept as the file has it).
    pub second: String,
    /// Per DirectInput axis: the function and whether it runs the other way.
    pub axes: [Option<(Func, bool)>; 8],
    /// Per axis the file's flags beyond bit 0 (kept as they are).
    pub axis_flags: [i32; 8],
    /// Per button: the key action (empty: none) and the number after it.
    pub buttons: Vec<(String, String)>,
    /// `[FFScale]`: steering forces (centering and drag), then vibration strength.
    pub ff_scale: Option<(f32, f32)>,
    /// `[openOMSI.FFTuning]`: how this wheel's steering force is weighted (None: the
    /// defaults, and nothing written).
    pub ff_tune: Option<FfTune>,
    /// Motor polarity for this device; None uses the existing global setting.
    pub ff_invert: Option<bool>,
    /// `[openOMSI.Latching]`: the buttons (from 0) that are latching switches - a turn signal
    /// lever, a lit hazard button - and switch back when they come out.
    pub latching: Vec<usize>,
    pub more: Vec<(usize, String)>,
}

impl DeviceCfg {
    pub fn steers(&self) -> bool {
        self.axes
            .iter()
            .flatten()
            .any(|(f, _)| *f == Func::Steering)
    }

    pub fn actions_of(&self, button: usize) -> Vec<String> {
        let first = self.buttons.get(button).map(|b| b.0.clone()).into_iter();
        let more = self
            .more
            .iter()
            .filter(|(b, _)| *b == button)
            .map(|(_, a)| a.clone());
        let mut out: Vec<String> = Vec::new();
        for a in first.chain(more).filter(|a| !a.is_empty()) {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(&a)) {
                out.push(a);
            }
        }
        out
    }

    pub fn continuous(&self, button: usize) -> bool {
        self.buttons.get(button).is_some_and(|b| b.1.trim() == "1")
    }
}

/// The `gamectrler.cfg` in use: the newer of `~/.openomsi/Inputs`' (written by the launcher)
/// and the content folder's, before OMSI 2's own.
pub fn cfg_path(root: &Path) -> PathBuf {
    omsi_cfg::input_in_use("gamectrler.cfg")
        .or_else(|| omsi_cfg::find_in_roots("Inputs/gamectrler.cfg").map(|(_, p)| p))
        .unwrap_or_else(|| root.join("Inputs").join("gamectrler.cfg"))
}

/// `Inputs/gamectrler.cfg`: the configured devices.
pub fn read_cfg(root: &Path) -> Vec<DeviceCfg> {
    read_cfg_checked(root).unwrap_or_default()
}

pub fn read_cfg_checked(root: &Path) -> Result<Vec<DeviceCfg>, String> {
    let path = cfg_path(root);
    let text = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut devices = parse_cfg(&omsi_cfg::codepage::decode(&text));
    // An inherited OMSI file can contain 0/0 FFScale on a wheel. Keep its axis and
    // button bindings, but use openOMSI's 100/100 default until our own file is saved.
    let original = root.join("Inputs").join("gamectrler.cfg");
    let from_original = path == original
        || std::fs::canonicalize(&path)
            .ok()
            .zip(std::fs::canonicalize(&original).ok())
            .is_some_and(|(a, b)| a == b);
    if from_original {
        for d in &mut devices {
            if d.ff_scale == Some((0.0, 0.0)) {
                d.ff_scale = None;
            }
        }
    }
    Ok(devices)
}

pub fn parse_cfg(text: &str) -> Vec<DeviceCfg> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let mut out: Vec<DeviceCfg> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        match lines[i] {
            "[ctrl]" => {
                out.push(DeviceCfg {
                    name: lines.get(i + 1).unwrap_or(&"").to_string(),
                    second: lines.get(i + 2).unwrap_or(&"0").to_string(),
                    ..Default::default()
                });
                i += 3;
            }
            "[axis]" => {
                if let Some(d) = out.last_mut() {
                    for a in 0..8 {
                        let f: i32 = lines
                            .get(i + 1 + a * 2)
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(-1);
                        let flags: i32 = lines
                            .get(i + 2 + a * 2)
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0);
                        d.axes[a] = Func::from_code(f).map(|f| (f, flags & 1 != 0));
                        d.axis_flags[a] = flags & !1;
                    }
                }
                i += 17;
            }
            "[buttons]" => {
                let n: usize = lines
                    .get(i + 1)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0)
                    .min(512);
                if let Some(d) = out.last_mut() {
                    for b in 0..n {
                        d.buttons.push((
                            lines.get(i + 2 + b * 2).unwrap_or(&"").to_string(),
                            lines.get(i + 3 + b * 2).unwrap_or(&"0").to_string(),
                        ));
                    }
                }
                i += 2 + n * 2;
            }
            "[ffscale]" | "[FFScale]" => {
                if let Some(d) = out.last_mut() {
                    let f = |k: usize| {
                        lines
                            .get(i + k)
                            .map(|v| omsi_cfg::parse_f64(v))
                            .unwrap_or(1.0) as f32
                    };
                    d.ff_scale = Some((f(1), f(2)));
                }
                i += 3;
            }
            "[openOMSI.Latching]" => {
                if let Some(d) = out.last_mut() {
                    // (button numbers as the launcher shows them, from 1)
                    d.latching = lines
                        .get(i + 1)
                        .unwrap_or(&"")
                        .split_whitespace()
                        .filter_map(|v| v.parse::<usize>().ok()?.checked_sub(1))
                        .collect();
                }
                i += 2;
            }
            "[openOMSI.MoreActions]" => {
                let n: usize = lines
                    .get(i + 1)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0)
                    .min(4096);
                if let Some(d) = out.last_mut() {
                    for k in 0..n {
                        let button = lines
                            .get(i + 2 + k * 2)
                            .and_then(|v| v.parse::<usize>().ok()?.checked_sub(1));
                        let action = lines.get(i + 3 + k * 2).unwrap_or(&"");
                        if let Some(b) = button.filter(|_| !action.is_empty()) {
                            d.more.push((b, action.to_string()));
                        }
                    }
                }
                i += 2 + n * 2;
            }
            "[openOMSI.FFTuning]" => {
                // centring, damping, parking, min_force, smoothing: as many as are there
                // before the next blank line or section, the rest (and a NaN) at their defaults
                let given: Vec<f32> = lines[i + 1..]
                    .iter()
                    .take(5)
                    .take_while(|v| !v.is_empty() && !v.starts_with('['))
                    .map(|v| {
                        if v.parse::<f32>().is_ok_and(f32::is_nan) {
                            f32::NAN
                        } else {
                            omsi_cfg::parse_f64(v) as f32
                        }
                    })
                    .collect();
                if let Some(d) = out.last_mut() {
                    d.ff_tune = Some(FfTune::from_values(&given));
                }
                i += 1 + given.len();
            }
            "[openOMSI.FFInvert]" => {
                if let Some(d) = out.last_mut() {
                    d.ff_invert = lines.get(i + 1).and_then(|v| match *v {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    });
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    out
}

/// The file's text for `devices`, as OMSI writes it (CR LF).
pub fn cfg_text(devices: &[DeviceCfg]) -> String {
    let mut t = String::new();
    for d in devices {
        t.push_str(&format!(
            "\r\n[ctrl]\r\n{}\r\n{}\r\n\r\n[axis]\r\n",
            d.name,
            if d.second.is_empty() { "0" } else { &d.second }
        ));
        for a in 0..8 {
            let (f, inv) = match d.axes[a] {
                Some((f, inv)) => (Func::code(Some(f)), inv),
                None => (-1, false),
            };
            t.push_str(&format!("{f}\r\n{}\r\n", d.axis_flags[a] | inv as i32));
        }
        t.push_str(&format!("\r\n[buttons]\r\n{}\r\n", d.buttons.len()));
        for (action, n) in &d.buttons {
            t.push_str(&format!(
                "{action}\r\n{}\r\n",
                if n.is_empty() { "0" } else { n }
            ));
        }
        let (a, b) = d.ff_scale.unwrap_or((1.0, 1.0));
        t.push_str(&format!("\r\n[FFScale]\r\n{a:.3}\r\n{b:.3}\r\n\r\n"));
        // (only where it says something the defaults do not, as the file will read it back)
        let tune_lines = |tune: FfTune| {
            tune.clamped()
                .values()
                .iter()
                .map(|v| format!("{v:.3}\r\n"))
                .collect::<String>()
        };
        if let Some(lines) = d
            .ff_tune
            .map(tune_lines)
            .filter(|l| *l != tune_lines(FfTune::default()))
        {
            t.push_str(&format!("[openOMSI.FFTuning]\r\n{lines}\r\n"));
        }
        if let Some(invert) = d.ff_invert {
            t.push_str(&format!("[openOMSI.FFInvert]\r\n{}\r\n\r\n", invert as u8));
        }
        if !d.latching.is_empty() {
            let numbers: Vec<String> = d.latching.iter().map(|b| (b + 1).to_string()).collect();
            t.push_str(&format!(
                "[openOMSI.Latching]\r\n{}\r\n\r\n",
                numbers.join(" ")
            ));
        }
        if !d.more.is_empty() {
            t.push_str(&format!("[openOMSI.MoreActions]\r\n{}\r\n", d.more.len()));
            for (b, action) in &d.more {
                t.push_str(&format!("{}\r\n{action}\r\n", b + 1));
            }
            t.push_str("\r\n");
        }
    }
    t
}
