//! Sound configuration files (unit `mc_sound`) - vehicles, scenery objects and AI cars share
//! the same format.

use omsi_cfg::CfgFile;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VolCurve {
    pub variable: String,
    pub points: Vec<(f32, f32)>,
}

/// `[conditionSingle]` / `[conditionInt]` / `[conditionBool]`: the sound is heard only while
/// `variable <relation> value` holds. The file gives the variable, the value, then the
/// relation: 0 `<>`, 1 `=`, 2 `<`, 3 `>`, 4 `<=`, 5 `>=` - `engine_n 200 3` (the engine runs),
/// `velocity 2 2` (standing), `antrieb_getr_aktugang 2 4` (first or second gear).
///
/// Omsi.exe (`TBoolClass` check 0x7effc8) compares exactly, without a tolerance; a
/// `[conditionInt]` has an integer value and knows the relations 0 to 3 only (4 and 5 leave
/// it out of the check), and a `[conditionBool]` (no relation line) holds when the variable
/// is non-zero for a value of 1 and zero for any other value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Condition {
    pub variable: String,
    pub value: f32,
    pub relation: i32,
    pub kind: ConditionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConditionKind {
    #[default]
    Single,
    Int,
    Bool,
}

impl Condition {
    pub fn holds(&self, v: f32) -> bool {
        match self.kind {
            ConditionKind::Bool => (v != 0.0) == (self.value == 1.0),
            ConditionKind::Int if self.relation > 3 => true,
            _ => match self.relation {
                0 => v != self.value,
                1 => v == self.value,
                2 => v < self.value,
                3 => v > self.value,
                4 => v <= self.value,
                5 => v >= self.value,
                _ => true,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SoundEntry {
    pub file: String,
    pub volume: f32,
    pub is_loop: bool,
    /// Loop sounds: sample rate at which `pitch_variable` == `pitch_ref`.
    pub sample_rate: f32,
    pub pitch_variable: String,
    pub pitch_ref: f32,
    /// `[3d]` position and range
    pub pos: Option<[f32; 3]>,
    pub range: f32,
    pub dir: Option<[f32; 3]>,
    pub no_loop: bool,
    pub important: bool,
    pub viewpoint: i32,
    pub vol_curves: Vec<VolCurve>,
    pub conditions: Vec<Condition>,
    pub triggers: Vec<String>,
    pub check_loading: bool,
    pub only_one: bool,
    /// `[next_random]`: (count, min, max) applied to this entry.
    pub random: Option<(i32, f32, f32)>,
    /// `[traction]`: this is a traction sound - the motor's loops of an electric bus, a
    /// tram's or a trolleybus's (openOMSI's own keyword; OMSI 2 has none and ignores the
    /// line). The settings' traction slider scales these and only these (#3336). A vehicle
    /// file that marks none of its entries has its traction sounds guessed by their file
    /// names (see `omsi_audio::soundset`).
    pub traction: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SoundCfg {
    pub sounds: Vec<SoundEntry>,
    pub unknown_keywords: Vec<(String, usize)>,
}

/// OMSI's number-specific random value (the original, the `NrSpecRandom` callback and
/// `[next_random]`): a value in [0, 1) that is always the same for a vehicle number
/// (`ident`) and a characteristic `n`, so that bus 3261 always has the rattling compressor
/// and 3262 never. The seed is 10 plus the UTF-16 code units of the number (a random one
/// in 0..1000 without a number); `n + 1000 * seed` goes through the linear congruential
/// step `(x * 31415926 + 1) mod 10^8` once and then `n mod 15 + 1` times more.
pub fn spec_random(ident: &str, n: i32) -> f32 {
    use std::hash::BuildHasher;
    let mut seed: u32 = 0;
    if !ident.is_empty() {
        seed = 10;
        for u in ident.encode_utf16() {
            seed = seed.wrapping_add(u as u32);
        }
    }
    if seed == 0 {
        seed = (std::collections::hash_map::RandomState::new().hash_one(n) % 1000) as u32;
    }
    let step = |x: i64| (x.wrapping_mul(31_415_926).wrapping_add(1)).rem_euclid(100_000_000);
    let mut x = step((n as u32).wrapping_add(seed.wrapping_mul(1000)) as i64);
    for _ in 0..=(n as u32 % 15) {
        x = step(x as i32 as i64);
    }
    x as i32 as f32 / 100_000_000.0
}

impl SoundCfg {
    /// The entries a vehicle with this number keeps: a `[next_random] n min max` before an
    /// entry drops it unless the number's characteristic `n` lies in [min, max) - the SD200's
    /// under-idle rattle, compressor burble and loud servo pump are on some buses only.
    pub fn chosen_for(&self, ident: &str) -> SoundCfg {
        let sounds = self
            .sounds
            .iter()
            .filter(|s| match s.random {
                Some((n, lo, hi)) => {
                    let r = spec_random(ident, n);
                    lo <= r && r < hi
                }
                None => true,
            })
            .cloned()
            .collect();
        SoundCfg {
            sounds,
            unknown_keywords: self.unknown_keywords.clone(),
        }
    }

    pub fn load(path: &Path) -> Result<SoundCfg, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        Ok(Self::parse(&f))
    }

    pub fn parse(f: &CfgFile) -> SoundCfg {
        let mut c = SoundCfg::default();
        let mut pending_random: Option<(i32, f32, f32)> = None;
        let mut r = f.reader();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "sound" => {
                    let file = r.str().to_string();
                    let volume = r.f32();
                    c.sounds.push(SoundEntry {
                        file,
                        volume,
                        random: pending_random.take(),
                        ..Default::default()
                    });
                }
                "loopsound" => {
                    // file, sample rate, pitch variable, its reference value, volume: the
                    // exe's loader stores the fifth line where `[sound]` keeps
                    // its volume (+0x6c) - 0.05 to 1.4 in the stock files
                    let file = r.str().to_string();
                    let sample_rate = r.f32();
                    let pitch_variable = r.str().to_string();
                    let pitch_ref = r.f32();
                    let volume = r.f32();
                    c.sounds.push(SoundEntry {
                        file,
                        volume,
                        is_loop: true,
                        sample_rate,
                        pitch_variable,
                        pitch_ref,
                        random: pending_random.take(),
                        ..Default::default()
                    });
                }
                "next_random" => {
                    let n = r.i32();
                    let a = r.f32();
                    let b = r.f32();
                    pending_random = Some((n, a, b));
                }
                "3d" => {
                    let p = r.f32s::<3>();
                    let range = r.f32();
                    if let Some(s) = c.sounds.last_mut() {
                        s.pos = Some(p);
                        s.range = range;
                    }
                }
                "dir" => {
                    let d = r.f32s::<3>();
                    if let Some(s) = c.sounds.last_mut() {
                        s.dir = Some(d);
                    }
                }
                "noloop" => {
                    if let Some(s) = c.sounds.last_mut() {
                        s.no_loop = true;
                    }
                }
                "important" => {
                    if let Some(s) = c.sounds.last_mut() {
                        s.important = true;
                    }
                }
                "viewpoint" => {
                    let v = r.i32();
                    if let Some(s) = c.sounds.last_mut() {
                        s.viewpoint = v;
                    }
                }
                "volcurve" => {
                    let v = r.str().to_string();
                    if let Some(s) = c.sounds.last_mut() {
                        s.vol_curves.push(VolCurve {
                            variable: v,
                            points: Vec::new(),
                        });
                    }
                }
                "pnt" => {
                    let x = r.f32();
                    let y = r.f32();
                    if let Some(cv) = c.sounds.last_mut().and_then(|s| s.vol_curves.last_mut()) {
                        cv.points.push((x, y));
                    }
                }
                "conditionsingle" | "conditionint" | "conditionbool" => {
                    let variable = r.str().to_string();
                    let kind = match k.as_str() {
                        "conditionint" => ConditionKind::Int,
                        "conditionbool" => ConditionKind::Bool,
                        _ => ConditionKind::Single,
                    };
                    // (an int's value is StrToInt, a bool's StrToInt = 1; a bool has no relation)
                    let value = if kind == ConditionKind::Single {
                        r.f32()
                    } else {
                        r.str().trim().parse::<i32>().unwrap_or(0) as f32
                    };
                    let relation = if kind == ConditionKind::Bool {
                        1
                    } else {
                        r.f32() as i32
                    };
                    if let Some(s) = c.sounds.last_mut() {
                        s.conditions.push(Condition {
                            variable,
                            value,
                            relation,
                            kind,
                        });
                    }
                }
                "trigger" => {
                    let t = r.str().to_string();
                    if let Some(s) = c.sounds.last_mut() {
                        s.triggers.push(t);
                    }
                }
                "checkloading" => {
                    if let Some(s) = c.sounds.last_mut() {
                        s.check_loading = true;
                    }
                }
                "onlyone" => {
                    if let Some(s) = c.sounds.last_mut() {
                        s.only_one = true;
                    }
                }
                // (openOMSI's own: the motor of an electric bus, a tram, a trolleybus - the
                // settings' traction volume scales it, #3336)
                "traction" => {
                    if let Some(s) = c.sounds.last_mut() {
                        s.traction = true;
                    }
                }
                _ => c.unknown_keywords.push((k, r.block_line())),
            }
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditions_name_the_value_then_the_relation() {
        let text = "[sound]\nSD_hupe_lang.wav\n1\n\n[volcurve]\ncockpit_hupe_volume\n\n[pnt]\n0\n0\n\n[conditionSingle]\ncockpit_hupe_volume\n3\n0\n\n[loopsound]\nkompressor@600.wav\n44100\nengine_n\n600\n1\n\n[conditionSingle]\nengine_n\n200\n3\n\n[conditionSingle]\nbremse_kompressor\n0\n1\n\n[conditionSingle]\nantrieb_getr_aktugang\n2\n4\n\n[conditionSingle]\nvelocity\n2\n2\n";
        let c = SoundCfg::parse(&CfgFile::from_str("sound.cfg", text));
        assert_eq!(c.sounds.len(), 2);
        let horn = &c.sounds[0].conditions[0];
        assert!(horn.holds(0.0) && horn.holds(1.0), "{horn:?}");
        let conds = &c.sounds[1].conditions;
        assert_eq!((conds[0].value, conds[0].relation), (200.0, 3));
        assert!(!conds[0].holds(0.0) && !conds[0].holds(200.0) && conds[0].holds(520.0));
        assert!(conds[1].holds(0.0) && !conds[1].holds(1.0));
        assert!(conds[2].holds(1.0) && conds[2].holds(2.0) && !conds[2].holds(3.0));
        assert!(conds[3].holds(0.5) && !conds[3].holds(2.0));
    }

    #[test]
    fn conditions_compare_exactly_as_omsi_does() {
        let text = "[sound]\na.wav\n1\n[conditionBool]\nlight\n1\n[conditionInt]\ngear\n2\n4\n[conditionSingle]\nv\n0.5\n1\n";
        let c = SoundCfg::parse(&CfgFile::from_str("sound.cfg", text));
        let conds = &c.sounds[0].conditions;
        // a bool of 1: any non-zero value
        assert!(conds[0].holds(2.0) && conds[0].holds(-1.0) && !conds[0].holds(0.0));
        // an int knows the relations 0 to 3 only
        assert!(conds[1].holds(7.0));
        // no tolerance
        assert!(conds[2].holds(0.5) && !conds[2].holds(0.50001));
    }

    /// `[traction]` marks the entry before it (openOMSI's own keyword, #3336): the motor's
    /// loops of an electric bus, a tram, a trolleybus - and only that entry.
    #[test]
    fn the_traction_keyword_marks_the_entry_before_it() {
        let text = "[loopsound]\nmotor_loop.wav\n44100\nengine_n\n600\n1\n[traction]\n\n[sound]\ndoor.wav\n1\n\n[loopsound]\nfan.wav\n44100\nfan_n\n600\n1\n";
        let c = SoundCfg::parse(&CfgFile::from_str("sound.cfg", text));
        assert_eq!(c.sounds.len(), 3);
        assert!(c.sounds[0].traction, "the motor");
        assert!(
            !c.sounds[1].traction && !c.sounds[2].traction,
            "and nothing else"
        );
        // (a file without the keyword marks nothing: `soundset` guesses by the file names)
        let plain = SoundCfg::parse(&CfgFile::from_str("sound.cfg", "[sound]\na.wav\n1\n"));
        assert!(!plain.sounds[0].traction);
        assert!(
            !plain.unknown_keywords.iter().any(|(k, _)| k == "traction"),
            "the keyword is known"
        );
    }

    #[test]
    fn number_specific_random_is_stable_and_spread() {
        for n in [0, 6, 8, 16, 29] {
            let a = spec_random("3261", n);
            assert!((0.0..1.0).contains(&a));
            assert_eq!(
                a,
                spec_random("3261", n),
                "the same number gives the same value"
            );
        }
        // different numbers give different buses
        let v: Vec<f32> = (3000..3040)
            .map(|k| spec_random(&k.to_string(), 8))
            .collect();
        let below = v.iter().filter(|x| **x < 0.4).count();
        assert!(below > 4 && below < 36, "{below} of 40 below 0.4");
    }

    #[test]
    fn next_random_keeps_an_entry_for_some_numbers_only() {
        let cfg = SoundCfg::parse(&CfgFile::from_str(
            "t.cfg",
            "[next_random]\n8\n0.0\n0.4\n[loopsound]\nrattle.wav\n32000\nengine_n\n497\n0.7\n\n[sound]\nhorn.wav\n1\n",
        ));
        assert_eq!(
            cfg.sounds[0].volume, 0.7,
            "the fifth line of [loopsound] is its volume"
        );
        let kept: usize = (3000..3040)
            .map(|k| cfg.chosen_for(&k.to_string()).sounds.len())
            .sum();
        assert!(
            kept > 40 && kept < 80,
            "the horn always, the rattle on some buses: {kept}"
        );
    }
}
