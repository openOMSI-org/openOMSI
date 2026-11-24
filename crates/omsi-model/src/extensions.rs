//! openOMSI's own additions to the model vocabulary that need more than a line of the
//! keyword handler: `[spotlight_cookie]`, and the tidying of `[illumination_interior]`.

use super::Model;
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
}

impl SpotlightCookie {
    /// The block after its keyword (see the struct).
    pub(crate) fn read(r: &mut CfgReader) -> SpotlightCookie {
        let pos = r.f32s::<3>();
        let dir = r.f32s::<3>();
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
