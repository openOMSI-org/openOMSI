//! Print every entry of a sound configuration with the level it is heard at for one fixed
//! state: `sound_levels <sound.cfg> <inside|outside|ai> <cab 0|1> <Snd_OutsideVol> <x,y,z>
//! <vars file>` (the listener in the vehicle's own coordinates; the vars file holds
//! `name value` lines, e.g. from `OMSI_DEBUG_VARS`). `sound_levels <sound.cfg> vars` lists
//! the variables the entries read.

use glam::{Mat4, Vec3};
use omsi_audio::{AudioEngine, SoundSet};
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let path = std::path::Path::new(&a[1]);
    let cfg = omsi_vehicle::SoundCfg::load(path).expect("sound config");
    if a.get(2).map(String::as_str) == Some("vars") {
        let mut names: Vec<String> = Vec::new();
        for s in &cfg.sounds {
            let mut add = |n: &str| {
                if n.parse::<i32>().is_err() && !n.is_empty() && !names.iter().any(|m| m == n) {
                    names.push(n.to_string());
                }
            };
            s.vol_curves.iter().for_each(|c| add(&c.variable));
            s.conditions.iter().for_each(|c| add(&c.variable));
            add(&s.pitch_variable);
        }
        println!("{}", names.join(","));
        return;
    }
    let view = a[2].as_str();
    let cab = a[3] == "1";
    let outside: f32 = a[4].parse().unwrap();
    let l: Vec<f32> = a[5].split(',').map(|x| x.parse().unwrap()).collect();
    let listener = Vec3::new(l[0], l[1], l[2]);
    let vars: HashMap<String, f32> = std::fs::read_to_string(&a[6])
        .unwrap()
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.to_string(), it.next()?.parse().ok()?))
        })
        .collect();
    let var = |n: &str| vars.get(n).copied();
    let engine = AudioEngine::silent();
    let dir = path.parent().unwrap();
    let mut ss = if view == "ai" {
        SoundSet::new_exterior(&engine, &cfg, dir)
    } else {
        SoundSet::new(&engine, &cfg, dir)
    };
    ss.set_inside(view == "inside");
    ss.set_muffled(cab);
    omsi_audio::soundset::set_outside_open(Some(outside));
    let new = ss.levels(&var, &Mat4::IDENTITY, listener);
    println!(
        "{:<40} {:>8} {:>6}   {:>8} {:>6}",
        "entry", "old", "pitch", "new", "pitch"
    );
    for (k, def) in cfg.sounds.iter().enumerate() {
        let clip_rate = omsi_audio::mixer::read_clip(&omsi_cfg::resolve_path(dir, &def.file))
            .map(|c| c.sample_rate as f32);
        let (og, op) = old::level(def, &var, view, cab, outside, listener, clip_rate);
        let (_, ng, np, _) = &new[k];
        let mark = if (og - ng).abs() > 0.005 || (og > 0.0 && *ng > 0.0 && (op - np).abs() > 0.01) {
            "  *"
        } else {
            ""
        };
        println!(
            "{:<40} {:>8.3} {:>6.2}   {:>8.3} {:>6.2}{mark}",
            format!("{k:>3} {}", def.file.trim()),
            og,
            op,
            ng,
            np
        );
    }
}

/// The level computation before the Omsi.exe rebuild, for the comparison.
mod old {
    use glam::Vec3;
    use omsi_vehicle::SoundEntry;

    fn curve(points: &[(f32, f32)], x: f32) -> f32 {
        if points.is_empty() {
            return 1.0;
        }
        if x <= points[0].0 {
            return points[0].1;
        }
        let last = points[points.len() - 1];
        if x >= last.0 {
            return last.1;
        }
        for w in points.windows(2) {
            let (x0, y0) = w[0];
            let (x1, y1) = w[1];
            if x >= x0 && x <= x1 {
                return if x1 == x0 {
                    y1
                } else {
                    y0 + (y1 - y0) * (x - x0) / (x1 - x0)
                };
            }
        }
        last.1
    }

    fn holds(c: &omsi_vehicle::sound::Condition, v: f32) -> bool {
        let eq = (v - c.value).abs() < 1.0e-4;
        match c.relation {
            0 => !eq,
            1 => eq,
            2 => v < c.value,
            3 => v > c.value,
            4 => v <= c.value || eq,
            5 => v >= c.value || eq,
            _ => true,
        }
    }

    pub fn level(
        def: &SoundEntry,
        var: &dyn Fn(&str) -> Option<f32>,
        view: &str,
        cab: bool,
        o: f32,
        listener: Vec3,
        clip_rate: Option<f32>,
    ) -> (f32, f32) {
        let ai = view == "ai";
        let inside = view == "inside";
        let mask = (if inside { 2 } else { 1 }) | (if ai { 4 } else { 0 });
        let mut through = 1.0;
        if def.viewpoint != 0 && def.viewpoint & mask == 0 {
            if mask == 2 && def.viewpoint & 2 == 0 && o > 0.01 {
                through = o;
            } else {
                return (0.0, 1.0);
            }
        }
        if def.triggers.is_empty()
            && !def
                .conditions
                .iter()
                .all(|c| holds(c, var(&c.variable).unwrap_or(0.0)))
        {
            return (0.0, 1.0);
        }
        let mut vol = def.volume;
        for vc in &def.vol_curves {
            let x = match vc.variable.trim().parse::<i32>() {
                Ok(-1) => Some(5.0),
                Ok(-2) => Some(1.0),
                Ok(n) if n < 0 => None,
                _ => Some(var(&vc.variable).unwrap_or(0.0)),
            };
            if let Some(x) = x {
                vol *= curve(&vc.points, x);
            }
        }
        let vol = (vol * through).clamp(0.0, 1.0);
        let outside_gain = if cab && ai {
            (0.25 + 1.5 * o.clamp(0.0, 0.5)).min(1.0)
        } else {
            1.0
        };
        let pos = def.pos.map(Vec3::from_array).or(ai.then_some(Vec3::ZERO));
        let range = if def.range > 0.0 {
            def.range
        } else if ai {
            40.0
        } else {
            5.0
        };
        let dist = pos
            .map(|p| (range.max(0.01) / (p - listener).length().max(0.1).max(0.01)).min(1.0))
            .unwrap_or(1.0);
        let (pitch, fast) = match clip_rate {
            Some(cr) if def.is_loop && def.pitch_ref != 0.0 && !def.pitch_variable.is_empty() => {
                let rate = if def.sample_rate > 0.0 {
                    def.sample_rate
                } else {
                    cr
                };
                let hz = var(&def.pitch_variable).unwrap_or(0.0).abs() * rate / def.pitch_ref;
                (hz / cr, hz >= 100.0)
            }
            _ => (1.0, true),
        };
        if vol > 0.001 && fast {
            (vol * outside_gain * dist, pitch)
        } else {
            (0.0, pitch)
        }
    }
}
