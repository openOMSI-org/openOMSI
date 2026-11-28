//! openOMSI's own additions to the model vocabulary that need more than a line of the
//! keyword handler: `[spotlight_cookie]`, and the tidying of `[illumination_interior]`.

use super::{LightEmitterInterior, Model};
use omsi_cfg::CfgReader;

/// `[spotlight_cookie]` (openOMSI): a lamp whose beam is a picture - how strong and in what
/// colour it shines every way within the field the picture covers (see docs/FORMATS.md).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpotlightCookie {
    /// Where it is and which way it shines, in the vehicle's frame.
    pub pos: [f32; 3],
    pub dir: [f32; 3],
    /// Its colour (0..255 each), the picture's colours times this.
    pub color: [f32; 3],
    /// How far it reaches (m), as a `[spotlight]`'s range.
    pub range: f32,
    /// The angles the picture covers across and up and down (degrees, whole).
    pub width: f32,
    pub height: f32,
    /// The picture, relative to the model's folder.
    pub picture: String,
    /// The variable that switches it (0 off, 1 full, in between dimmed; a number is a
    /// constant).
    pub variable: String,
    /// Seconds it takes to come to 63 % of a new brightness (0: at once).
    pub fade: f32,
    /// Variables turning it from its direction (degrees): to the right about the vehicle's
    /// up axis, and up about its own horizontal one.
    pub yaw: Option<String>,
    pub pitch: Option<String>,
    /// A twin lamp across the vehicle's axis is wanted (the flag of the Sobol layout; the
    /// openOMSI layout has one lamp per block).
    pub mirrored: bool,
}

/// The picture carries the colour and covers the whole beam in the Sobol layout: a 1280x1024
/// frame, 0.09375 degrees a pixel (-60..60 across, 36..-60 up/down).
const SOBOL_WIDTH: f32 = 120.0;
const SOBOL_HEIGHT: f32 = 96.0;

fn numeric(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty() && s.parse::<f32>().is_ok()
}

/// The two layouts share the position and direction. In openOMSI's own layout the next five
/// lines are red, green, blue, range, width (then height, picture and variable). In Sobol's
/// buses they are range, variable, mirror flag, picture, fade: its line 3 is a flag of 0 or 1
/// and its line 4 a picture name. That is enough to tell them apart without the bus's name.
fn sobol_layout(ahead: &[String]) -> bool {
    ahead.len() == 5
        && numeric(&ahead[0])
        && !ahead[1].trim().is_empty()
        && numeric(&ahead[2])
        && omsi_cfg::parse_f32(&ahead[2]) < 2.0
        && !numeric(&ahead[3])
        && !ahead[3].trim().is_empty()
        && (ahead[4].trim().is_empty() || numeric(&ahead[4]))
}

impl SpotlightCookie {
    /// The block after its keyword (see the struct).
    pub(crate) fn read(r: &mut CfgReader) -> SpotlightCookie {
        let pos = r.f32s::<3>();
        let dir = r.f32s::<3>();
        let mut ahead = r.clone();
        let lines: Vec<String> = (0..5)
            .map(|_| ahead.param_line().trim().to_string())
            .collect();
        if sobol_layout(&lines) {
            return Self::read_sobol(r, pos, dir);
        }
        Self::read_openomsi(r, pos, dir)
    }

    /// openOMSI's own layout: rgb, range, two angles, picture, variable and optional fade,
    /// yaw and pitch (in that order).
    fn read_openomsi(r: &mut CfgReader, pos: [f32; 3], dir: [f32; 3]) -> SpotlightCookie {
        let color = r.f32s::<3>();
        let range = r.f32();
        let width = r.f32();
        let height = r.f32();
        let picture = r.word().to_string();
        let variable = r.word().to_string();
        // (then, each where it is given - a blank line or the next keyword ends the
        // block: the fade time, the yaw and the pitch variables)
        let mut more: Vec<String> = Vec::new();
        while more.len() < 3 {
            let mut ahead = r.clone();
            let l = ahead.param_line().trim();
            if l.is_empty() {
                break;
            }
            more.push(l.to_string());
            *r = ahead;
        }
        let fade = more
            .first()
            .map_or(0.0, |v| omsi_cfg::parse_f32(v).max(0.0));
        let name = |k: usize| more.get(k).filter(|v| !v.is_empty()).cloned();
        SpotlightCookie {
            pos,
            dir,
            color,
            range,
            width,
            height,
            picture,
            variable,
            fade,
            yaw: name(1),
            pitch: name(2),
            mirrored: false,
        }
    }

    /// Sobol's layout: range, variable, a mirror flag, picture and fade, then optional pitch
    /// and yaw (in that order - the opposite of openOMSI's offsets).
    fn read_sobol(r: &mut CfgReader, pos: [f32; 3], dir: [f32; 3]) -> SpotlightCookie {
        let range = r.f32();
        let variable = r.word().to_string();
        let flag = r.f32();
        let picture = r.word().to_string();
        // (the fade and the two offsets are optional - a blank line or the next keyword ends
        // the block, so the keyword must not be read as a number)
        let mut more: Vec<String> = Vec::new();
        while more.len() < 3 {
            let mut ahead = r.clone();
            let l = ahead.param_line().trim();
            if l.is_empty() {
                break;
            }
            more.push(l.to_string());
            *r = ahead;
        }
        let name = |k: usize| more.get(k).filter(|v| !v.is_empty()).cloned();
        SpotlightCookie {
            pos,
            dir,
            color: [255.0; 3],
            range,
            width: SOBOL_WIDTH,
            height: SOBOL_HEIGHT,
            picture,
            variable,
            fade: more
                .first()
                .map_or(0.0, |v| omsi_cfg::parse_f32(v).max(0.0)),
            // (the picture is the same way round on both lamps; only their places and
            // directions are mirrored, see MODDING.md)
            yaw: name(2),
            pitch: name(1),
            mirrored: flag < 0.5,
        }
    }
}

impl Model {
    /// Keep in each mesh's `[illumination_interior]` only the lamps that exist, each once.
    /// `-1` is how the stock files write "no lamp" (the SD202 lists four of them for its
    /// outer meshes) and goes without a word; an index past the model's `[interiorlight]`s
    /// or one written twice is a mistake of the file - said once for the whole model, with
    /// the first such mesh, rather than met again by every frame's lamp code.
    pub fn tidy_interior_lamps(&mut self) {
        let n = self.interior_lights.len() as i64;
        let mut first_bad: Option<(String, Vec<i32>)> = None;
        let mut bad_meshes = 0;
        for mesh in &mut self.meshes {
            let mut kept: Vec<i32> = Vec::with_capacity(mesh.illumination_interior.len());
            let mut bad = false;
            for &k in &mesh.illumination_interior {
                if k == -1 {
                    continue;
                }
                if (k as i64) < 0 || k as i64 >= n || kept.contains(&k) {
                    bad = true;
                    continue;
                }
                kept.push(k);
            }
            // (the list a mesh gets without a line of its own, 0 to 3, is no mistake of the
            // file; nor is anything in a model without lamps, an AI car's copy of a bus)
            let implicit = mesh.illumination_interior == [0, 1, 2, 3];
            if bad && !implicit && n > 0 {
                bad_meshes += 1;
                first_bad
                    .get_or_insert_with(|| (mesh.file.clone(), mesh.illumination_interior.clone()));
            }
            mesh.illumination_interior = kept;
        }
        if let Some((file, list)) = first_bad {
            log::warn!(
                "{}: [illumination_interior] of {bad_meshes} mesh(es) names lamps that do not exist or names one twice (first {file}: {list:?}, {n} [interiorlight]s); those entries are left out",
                self.path.display()
            );
        }
    }
}

impl Model {
    /// `[light_emitter_interior]` with its optional lines (see [`LightEmitterInterior`]): a
    /// number for the strength, a whole number for the most lamps, a name for the variable;
    /// a blank line, a free text line or the next keyword ends it.
    pub(crate) fn read_light_emitter_interior(&mut self, r: &mut CfgReader) {
        // (every line optional: a number for the strength, a whole number for the
        // most lamps, a name for the variable; the next keyword ends it)
        let mut e = LightEmitterInterior::default();
        for k in 0..3 {
            let save = r.pos();
            let t = r.str().trim();
            if t.is_empty() || omsi_cfg::keyword_of(t).is_some() {
                r.seek(save);
                break;
            }
            let num = t.replace(',', ".").parse::<f32>();
            match (k, num) {
                (0, Ok(v)) => e.strength = v.max(0.0),
                (1, Ok(v)) => e.max_lamps = (v as u32).clamp(1, 16),
                // (a free text line - a comment - is no variable name)
                (_, Err(_))
                    if t.chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                        && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') =>
                {
                    e.variable = Some(t.to_string());
                    break;
                }
                _ => {
                    r.seek(save);
                    break;
                }
            }
        }
        if let Some(m) = self.cur_mesh() {
            m.light_emitter_interior = Some(e);
        }
    }
}
