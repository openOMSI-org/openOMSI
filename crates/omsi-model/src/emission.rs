//! `[matl_emissive]` and its companions (openOMSI, #2926): a material that gives off light of
//! a colour and strength of its own, optionally switched, dimmed and tinted by the scripts,
//! with a temperature colour and a fade. Omsi.exe does not know the keywords and skips them;
//! only the enhanced picture draws the light (see docs/PBR.md, "Emissive materials").

use super::Model;
use omsi_cfg::CfgReader;

/// What a slot's emissive keywords say. Any of them makes the slot emissive; what they leave
/// out keeps the values of `Emission::default`.
#[derive(Debug, Clone, PartialEq)]
pub struct Emission {
    /// The light's colour (sRGB, 0..1), `[matl_emissive]`'s first three lines. White
    /// without them; replaced by `kelvin`'s colour where that is given.
    pub color: [f32; 3],
    /// How bright, on the artist's scale (`[matl_emissive]`'s fourth line): 1 shines as a
    /// lit display does, more is HDR and blooms.
    pub strength: f32,
    /// `[matl_emissive_nits] <cd/m²>`: the luminance in physical units, in place of
    /// `strength` (the colour's own luminance taken as 1).
    pub nits: Option<f32>,
    /// A variable (or a number) scaling the light, 0 off .. 1 full (`[matl_emissive]`'s fifth
    /// line).
    pub variable: Option<String>,
    /// `[matl_emissive_kelvin] <K>`: the colour of a black body this hot.
    pub kelvin: Option<f32>,
    /// `[matl_emissive_kelvin]`'s second line: a variable giving the temperature (K) ...
    pub kelvin_variable: Option<String>,
    /// ... or the word `filament`: the temperature follows the light as an incandescent
    /// filament's does - dim and red-orange while it warms up or cools down, `kelvin` at full.
    pub filament: bool,
    /// `[matl_emissive_texture] <file>`: where and in what colour it shines (rgb, times the
    /// colour); without one the whole slot shines, through the diffuse texture's colour.
    pub texture: Option<String>,
    /// `[matl_emissive_fade] <up> [<down>]`: seconds the light takes to come to 63 % of a new
    /// level, rising and falling (0: at once).
    pub fade: [f32; 2],
}

impl Emission {
    /// Whether the scripts or a fade drive the light (its level or temperature changes as
    /// the vehicle runs); otherwise it shines as the material says.
    pub fn is_driven(&self) -> bool {
        self.variable.is_some()
            || self.kelvin_variable.is_some()
            || self.filament
            || self.fade.iter().any(|f| *f > 0.0)
    }

    /// Whether the driven value carries a temperature (`slot_value` with a kelvin).
    pub fn is_tinted(&self) -> bool {
        self.kelvin_variable.is_some() || (self.filament && self.kelvin.is_some())
    }
}

impl Default for Emission {
    fn default() -> Self {
        Emission {
            color: [1.0; 3],
            strength: 1.0,
            nits: None,
            variable: None,
            kelvin: None,
            kelvin_variable: None,
            filament: false,
            texture: None,
            fade: [0.0; 2],
        }
    }
}

/// The per-instance value of an emissive slot whose light the scripts drive, as the renderer
/// reads it from the slot's light value (`Instance::slot_light`): the level (0..1) alone,
/// or with a temperature as `-(K + level)`, the level kept under 1 (the scene shader's
/// `emission_slot` undoes it).
pub fn slot_value(level: f32, kelvin: Option<f32>) -> f32 {
    let level = if level.is_finite() {
        level.clamp(0.0, 1.0)
    } else {
        0.0
    };
    match kelvin.filter(|k| k.is_finite()) {
        Some(k) => -(k.clamp(500.0, 40_000.0).round() + level.min(0.999)),
        None => level,
    }
}

/// The temperature an incandescent filament glows at with `level` (0..1) of its full power,
/// `full` K at full power: the power a hot body radiates grows with T^4 (Stefan-Boltzmann).
pub fn filament_kelvin(full: f32, level: f32) -> f32 {
    (full * level.clamp(0.0, 1.0).powf(0.25)).max(800.0)
}

/// The colour (linear sRGB, luminance 1) of a black body at `kelvin`: its chromaticity on
/// the Planckian locus (Kim et al.'s cubic fit, 1667..25000 K, held at the ends), as the
/// scene shader's `blackbody` computes it.
pub fn blackbody_rgb(kelvin: f32) -> [f32; 3] {
    let t = if kelvin.is_finite() { kelvin } else { 6500.0 }.clamp(1667.0, 25_000.0) as f64;
    let (t2, t3) = (t * t, t * t * t);
    let x = if t <= 4000.0 {
        -0.266_123_9e9 / t3 - 0.234_358_9e6 / t2 + 0.877_695_6e3 / t + 0.179_910
    } else {
        -3.025_846_9e9 / t3 + 2.107_037_9e6 / t2 + 0.222_634_7e3 / t + 0.240_390
    };
    let (x2, x3) = (x * x, x * x * x);
    let y = if t <= 2222.0 {
        -1.106_381_4 * x3 - 1.348_110_20 * x2 + 2.185_558_32 * x - 0.202_196_83
    } else if t <= 4000.0 {
        -0.954_947_6 * x3 - 1.374_185_93 * x2 + 2.091_370_15 * x - 0.167_488_67
    } else {
        3.081_758_0 * x3 - 5.873_386_70 * x2 + 3.751_129_97 * x - 0.370_014_83
    };
    let (cx, cy, cz) = (x / y, 1.0, (1.0 - x - y) / y);
    let rgb = [
        3.2406 * cx - 1.5372 * cy - 0.4986 * cz,
        -0.9689 * cx + 1.8758 * cy + 0.0415 * cz,
        0.0557 * cx - 0.2040 * cy + 1.0570 * cz,
    ]
    .map(|c: f64| c.max(0.0));
    let l = (0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]).max(1e-6);
    rgb.map(|c| (c / l) as f32)
}

/// The next line of the block, when it is one of its own (not blank, not a keyword).
fn more(r: &mut CfgReader) -> Option<String> {
    let mut ahead = r.clone();
    let l = ahead.param_line().trim().to_string();
    if l.is_empty() || omsi_cfg::keyword_of(&l).is_some() {
        return None;
    }
    *r = ahead;
    Some(l)
}

/// A number of the format, or `default` for one that is none or out of reach.
fn num(v: f32, default: f32) -> f32 {
    if v.is_finite() { v } else { default }
}

impl Model {
    fn emission(&mut self) -> Option<&mut Emission> {
        Some(
            self.cur_matl()?
                .emission
                .get_or_insert_with(Emission::default),
        )
    }

    /// One of the emissive keywords and its lines (`key` without its brackets); false for
    /// another keyword.
    pub(crate) fn read_emission(&mut self, key: &str, r: &mut CfgReader) -> bool {
        match key {
            "matl_emissive" => {
                let c = r.f32s::<3>().map(|c| num(c, 1.0).clamp(0.0, 1.0));
                let s = num(r.f32(), 1.0).max(0.0);
                let v = more(r);
                if let Some(e) = self.emission() {
                    e.color = c;
                    e.strength = s;
                    e.variable = v;
                }
            }
            "matl_emissive_nits" => {
                let v = num(r.f32(), 0.0).max(0.0);
                if let Some(e) = self.emission() {
                    e.nits = Some(v);
                }
            }
            "matl_emissive_kelvin" => {
                let k = num(r.f32(), 6500.0).clamp(500.0, 40_000.0);
                let v = more(r);
                if let Some(e) = self.emission() {
                    e.kelvin = Some(k);
                    e.filament = v
                        .as_deref()
                        .is_some_and(|v| v.eq_ignore_ascii_case("filament"));
                    e.kelvin_variable = v.filter(|_| !e.filament);
                }
            }
            "matl_emissive_texture" => {
                let t = r.str().trim().to_string();
                if let Some(e) = self.emission() {
                    e.texture = Some(t).filter(|t| !t.is_empty());
                }
            }
            "matl_emissive_fade" => {
                let up = num(r.f32(), 0.0).max(0.0);
                let down = more(r).map_or(up, |l| num(omsi_cfg::parse_f32(&l), up).max(0.0));
                if let Some(e) = self.emission() {
                    e.fade = [up, down];
                }
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(text: &str) -> Option<Emission> {
        let m = Model::parse(&omsi_cfg::CfgFile::from_str(
            "m.cfg",
            &format!("[mesh]\nm.o3d\n\n[matl]\nm.dds\n0\n{text}"),
        ));
        m.meshes[0].materials[0].emission.clone()
    }

    #[test]
    fn black_body_colours() {
        let warm = blackbody_rgb(2700.0);
        let day = blackbody_rgb(6500.0);
        let blue = blackbody_rgb(15_000.0);
        // an incandescent bulb is orange, daylight about white, a very hot body bluish
        assert!(warm[0] > warm[1] && warm[1] > warm[2], "{warm:?}");
        assert!(
            (day[0] - day[2]).abs() < 0.12 && (day[1] - 1.0).abs() < 0.1,
            "{day:?}"
        );
        assert!(blue[2] > blue[0], "{blue:?}");
        for c in [
            warm,
            day,
            blue,
            blackbody_rgb(800.0),
            blackbody_rgb(f32::NAN),
        ] {
            let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            assert!(
                (l - 1.0).abs() < 1e-3 && c.iter().all(|v| *v >= 0.0),
                "{c:?}"
            );
        }
    }

    #[test]
    fn slot_values_carry_the_level_and_the_temperature() {
        assert_eq!(slot_value(0.25, None), 0.25);
        assert_eq!(slot_value(7.0, None), 1.0);
        let v = slot_value(1.0, Some(3200.4));
        assert!(v < 0.0);
        assert_eq!((-v).floor(), 3200.0);
        assert!(((-v).fract() - 0.999).abs() < 1e-3);
        assert_eq!(filament_kelvin(3200.0, 1.0), 3200.0);
        assert!(filament_kelvin(3200.0, 0.1) < 2000.0);
        assert_eq!(filament_kelvin(3200.0, 0.0), 800.0);
    }

    #[test]
    fn plain_material_has_none() {
        assert_eq!(slot(""), None);
    }

    #[test]
    fn colour_strength_and_variable() {
        let e = slot("[matl_emissive]\n1\n0.5\n0\n20\nlights_main\n").unwrap();
        assert_eq!(e.color, [1.0, 0.5, 0.0]);
        assert_eq!(e.strength, 20.0);
        assert_eq!(e.variable.as_deref(), Some("lights_main"));
        // (no variable line: the next keyword is not one)
        let e = slot("[matl_emissive]\n1\n1\n1\n2\n[matl_alpha]\n1\n").unwrap();
        assert_eq!(e.variable, None);
    }

    #[test]
    fn kelvin_filament_fade_texture_nits() {
        let e = slot(
            "[matl_emissive_kelvin]\n3200\nfilament\n\n[matl_emissive_fade]\n0.15\n0.3\n\n[matl_emissive_texture]\nbulb_e.png\n\n[matl_emissive_nits]\n20000000\n",
        )
        .unwrap();
        assert_eq!(e.kelvin, Some(3200.0));
        assert!(e.filament);
        assert_eq!(e.kelvin_variable, None);
        assert_eq!(e.fade, [0.15, 0.3]);
        assert_eq!(e.texture.as_deref(), Some("bulb_e.png"));
        assert_eq!(e.nits, Some(2.0e7));
        // the colour and strength stay the defaults
        assert_eq!(e.color, [1.0; 3]);
        let e =
            slot("[matl_emissive_kelvin]\n2700\nbulb_temp\n[matl_emissive_fade]\n0.5\n").unwrap();
        assert_eq!(e.kelvin_variable.as_deref(), Some("bulb_temp"));
        assert!(!e.filament);
        assert_eq!(e.fade, [0.5, 0.5]);
    }
}
