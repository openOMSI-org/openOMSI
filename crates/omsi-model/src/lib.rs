//! `model.cfg` - the visual description of a vehicle, scenery object or human
//! (original unit `mc_complobj`, material commands from `mc_MatlMan`).
//!
//! The same vocabulary appears inline in `.sco` files, therefore parsing is exposed as a
//! keyword handler ([`Model::handle_keyword`]) that other parsers can delegate to.

use omsi_cfg::{CfgFile, CfgReader, Entry};
use std::path::{Path, PathBuf};

mod extensions;
pub use extensions::SpotlightCookie;
#[cfg(test)]
mod extension_tests;
mod pages;

#[derive(Debug, Clone, PartialEq)]
pub enum AnimOrigin {
    Trans([f32; 3]),
    RotX(f32),
    RotY(f32),
    RotZ(f32),
    FromMesh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimKind {
    Rot,
    Trans,
}

/// `[newanim]`: a chain of origin transforms followed by one animated rotation/translation
/// driven by a script variable. `delay` and `maxspeed` smooth the variable.
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    pub origins: Vec<AnimOrigin>,
    pub kind: Option<AnimKind>,
    pub variable: String,
    pub factor: f32,
    pub offset: f32,
    pub delay: f32,
    pub max_speed: f32,
}

impl Default for Animation {
    fn default() -> Self {
        Self { origins: Vec::new(), kind: None, variable: String::new(), factor: 1.0, offset: 0.0, delay: 0.0, max_speed: 0.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TexAddress {
    #[default]
    Wrap,
    Mirror,
    Clamp,
    Border,
    MirrorOnce,
}

/// One `[matl]` block and the material commands that follow it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MaterialDef {
    /// Texture file name as written in the mesh; identifies which mesh material is meant.
    pub texture: String,
    /// Index when the same texture occurs several times in the mesh.
    pub index: i32,
    /// 0 = opaque, 1 = alpha test, 2 = alpha blend (OMSI `[matl_alpha]` modes).
    pub alpha: i32,
    /// This block has a `[matl_alpha]` of its own. A later `[matl]` of the same slot only
    /// changes the slot's mode when it has one (see `material_alpha` in omsi-app).
    pub alpha_set: bool,
    pub no_z_write: bool,
    pub no_z_check: bool,
    pub z_bias: i32,
    pub envmap: Option<(String, f32)>,
    pub envmap_realtime: bool,
    pub envmap_mask: Option<String>,
    pub bumpmap: Option<(String, f32)>,
    pub transmap: Option<String>,
    /// `[matl_change] texture index variable`: texture swapped by the CTC/variable.
    pub change: Option<(String, i32, String)>,
    pub item: bool,
    pub raindropmap: Option<String>,
    pub tex_address: TexAddress,
    pub border_color: [f32; 4],
    pub texcoord_trans_x: Option<String>,
    pub texcoord_trans_y: Option<String>,
    pub use_script_texture: Option<i32>,
    pub use_text_texture: Option<i32>,
    pub alphascale: Option<String>,
    pub freetex: Option<(String, String)>,
    /// Every `[matl_freetex]` (texture, string variable) of the record in order: Omsi.exe
    /// appends each to the record's own list (0x5f54de), and a `[matl_item]` starts with a
    /// copy of the slot's list, so an item shows the plain slot's free pictures beside the
    /// ones it adds, and none of another item's. `freetex` is the last of them.
    pub freetexes: Vec<(String, String)>,
    pub lightmap: Option<(String, String)>,
    /// Every `[matl_lightmap]` of the material in order (OMSI keeps them all, a
    /// texture and a variable each: the LiAZ's saloon has one per lighting circuit);
    /// `lightmap` is the last of them.
    pub lightmaps: Vec<(String, String)>,
    pub nightmap: Option<String>,
    /// `[matl_glow] <texture> <value>`: the material is its own light. The named texture is a
    /// **greyscale mask** of where and how much it shines (white full, black none) and the
    /// light is the material's own colour; `value` is its strength on the LED panels' scale
    /// (the `Led glow` setting's 16 levels: the shader's strength is `value * 0.25`, so 6 is
    /// as bright as that setting's default and 20 brighter than its top). The mask takes the
    /// light map's slot: a slot with a `[matl_lightmap]` keeps that and has no glow. An
    /// openOMSI extension: Omsi.exe does not know the keyword, and the classic picture draws
    /// the material as if it were not there.
    pub glow: Option<(String, f32)>,
    /// `[led_glow_effect] <value>` (openOMSI): how much of an LED panel's glow the slot
    /// gets, 0..1 - 0 none (a flipdot or an LCD the automatic guess took for LED dots),
    /// 1 the full `Led glow` setting. The modeller's word replaces the guess the enhanced
    /// picture otherwise makes from the slot's `\S:n` mask and white light map.
    pub led_glow: Option<f32>,
    pub allcolor: Option<[f32; 14]>,
}

impl MaterialDef {
    /// Take what this block leaves unset from `base` (a `[matl_item]` over its slot's plain
    /// `[matl]`).
    pub fn inherit(&mut self, base: &MaterialDef) {
        if !self.alpha_set && base.alpha_set {
            self.alpha = base.alpha;
            self.alpha_set = true;
        }
        self.no_z_write |= base.no_z_write;
        self.no_z_check |= base.no_z_check;
        if self.z_bias == 0 {
            self.z_bias = base.z_bias;
        }
        macro_rules! opt {
            ($($f:ident),*) => { $( if self.$f.is_none() { self.$f = base.$f.clone(); } )* };
        }
        opt!(envmap, envmap_mask, bumpmap, transmap, raindropmap, texcoord_trans_x, texcoord_trans_y, use_script_texture, use_text_texture, alphascale, freetex, nightmap, glow, led_glow, allcolor);
        self.envmap_realtime |= base.envmap_realtime;
        if self.tex_address == TexAddress::Wrap {
            self.tex_address = base.tex_address;
            if self.border_color == [0.0; 4] {
                self.border_color = base.border_color;
            }
        }
        if self.freetexes.is_empty() {
            self.freetexes = base.freetexes.clone();
        }
        if self.lightmaps.is_empty() && !base.lightmaps.is_empty() {
            self.lightmaps = base.lightmaps.clone();
            self.lightmap = base.lightmap.clone();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LightEnh {
    pub pos: [f32; 3],
    pub color: [f32; 3],
    pub size: f32,
    pub variable: String,
    pub values: Vec<f32>,
    pub texture: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LightEnh2 {
    pub pos: [f32; 3],
    pub dir: [f32; 3],
    pub up: [f32; 3],
    pub omni: bool,
    pub rotating: i32,
    pub color: [f32; 3],
    pub size: f32,
    pub cone_inner: f32,
    pub cone_outer: f32,
    pub variable: String,
    pub factor: f32,
    pub z_offset: f32,
    pub values: Vec<String>,
    /// The cone effect in fog (`cone` 1; only for a directional light).
    pub cone: bool,
    /// Seconds to reach 63 % of the brightness when switched on (`timeconst`; 0 = at once).
    pub time_const: f32,
    /// The light's own effect texture (`bitmap`; none = the standard one).
    pub bitmap: Option<String>,
}

/// One condition of a mesh's visibility (see [`MeshDef::visible_any`]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VisibleCond {
    pub variable: String,
    /// `[visible]`: the value (met within 0.5 of it, as the original compares); `[visible_range]`:
    /// the lowest value.
    pub lo: f32,
    /// `[visible_range]`: the highest value (both ends included).
    pub hi: f32,
    pub range: bool,
    /// Written `var !`: met when the plain condition is not.
    pub not: bool,
}

impl VisibleCond {
    /// Whether the condition holds with its variable at `x`.
    pub fn holds(&self, x: f32) -> bool {
        let met = if self.range { x >= self.lo && x <= self.hi } else { (x - self.lo).abs() < 0.5 };
        met != self.not
    }
}

impl MeshDef {
    /// The groups of `visible_any` that hold conditions (the mark `||` leaves is not one).
    pub fn visible_groups(&self) -> impl Iterator<Item = &Vec<VisibleCond>> {
        self.visible_any.iter().filter(|g| !g.is_empty())
    }

    /// Whether the mesh is shown with its variables read by `var`. A condition on a
    /// missing variable is ignored; a declared variable at zero is still compared.
    /// None: the mesh has no condition.
    pub fn visible_by(&self, var: impl Fn(&str) -> Option<f32>) -> Option<bool> {
        let mut groups = self.visible_groups().peekable();
        groups.peek()?;
        Some(groups.any(|g| g.iter().all(|c| var(&c.variable).is_none_or(|x| c.holds(x)))))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeshDef {
    pub file: String,
    pub lod: usize,
    pub viewpoint: i32,
    pub shadow: bool,
    pub is_shadow: bool,
    pub mouse_event: Option<String>,
    pub bones: Vec<(String, i32)>,
    pub smooth_skin: bool,
    pub anim_parent: Option<String>,
    pub animations: Vec<Animation>,
    /// `[visible] var value`: the mesh's last plain condition (what the original reads).
    pub visible: Option<(String, f32)>,
    /// Every visibility condition of the mesh (openOMSI, #729): the mesh is shown when all
    /// conditions of any one group hold. Several `[visible]` and `[visible_range] var min
    /// max` blocks of a mesh make one group (all must hold; with plain `[visible]` blocks
    /// alone the last one counts, as in the original), a variable written with `!`
    /// after it (`door_open !`) negates its condition, and a line `||` between two blocks
    /// starts the next group (either may hold). Empty: no condition. (An empty group is
    /// the mark a `||` leaves before its condition: see [`MeshDef::visible_groups`].)
    pub visible_any: Vec<Vec<VisibleCond>>,
    pub illumination: Vec<i32>,
    pub illumination_interior: Vec<i32>,
    pub materials: Vec<MaterialDef>,
    pub mesh_ident: Option<String>,
    pub light_enh: Vec<LightEnh>,
    pub light_enh_2: Vec<LightEnh2>,
    pub no_distance_check: bool,
    /// Compatibility mirror of the last `[terrainhole]` after this mesh. Use
    /// [`Model::terrain_hole_meshes`] to include model-wide and repeated declarations.
    pub terrain_hole: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Lod {
    /// Minimum screen size factor at which this LOD is used.
    pub min_size: f32,
    pub first_mesh: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct HtmlTextureDef {
    pub script_index: usize,
    pub width: i32,
    pub height: i32,
    pub path: String,
    /// `lit`, the word on the line after the page's path: the page is a **surface** - shaded
    /// by the world's light like every other material - rather than the display such a page
    /// is by default, drawn unlit at its own brightness, which is what a bus's own panel is.
    /// The slots that take their picture from this page are drawn lit with it, wherever
    /// they are - a scenery object's and a vehicle's alike (`Model::html_page_lit`). An
    /// openOMSI extension: Omsi.exe knows neither part, and draws such a slot unlit.
    pub lit: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextTexture {
    pub variable: String,
    pub font: String,
    pub width: i32,
    pub height: i32,
    pub full_color: bool,
    pub color: [f32; 3],
    /// `[texttexture_enh]` placement: 0 centred (plain `[texttexture]`), 1 left, 2 right,
    /// 3..5 centred over the letter spacing.
    pub orientation: i32,
    /// `[texttexture_enh]`: the text starts on a multiple of this many pixels.
    pub grid: i32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ctc {
    pub variable: String,
    pub path: String,
    pub value: i32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Spotlight {
    pub values: [f32; 12],
}

/// `[spotlight_2]` (openOMSI): a `[spotlight]`'s twelve numbers, then the variable that
/// switches it (0 off, 1 full, a constant too) and a flag: 0 (or none) puts a twin lamp on
/// the other side of the vehicle, mirrored across its axis, 1 keeps the one lamp.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Spotlight2 {
    pub values: [f32; 12],
    pub variable: String,
    pub mirrored: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InteriorLight {
    pub variable: String,
    pub range: f32,
    pub color: [f32; 3],
    pub pos: [f32; 3],
}

/// `[smoke]` particle system (19 parameters; some may be variable names).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Smoke {
    pub params: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParticleEmitter {
    pub lines: Vec<String>,
    pub attach_to: Option<[f32; 3]>,
}

/// A particle parameter: a number, or the name of a variable of the object that owns it
/// (OMSI reads every value of `[smoke]` after the direction this way, the original).
#[derive(Debug, Clone, PartialEq)]
pub enum PsValue {
    Const(f32),
    Var(String),
}

impl Default for PsValue {
    fn default() -> Self {
        PsValue::Const(0.0)
    }
}

impl PsValue {
    /// A line that begins like a number is one (read as Delphi reads it); anything else
    /// names a variable.
    pub fn parse(s: &str) -> PsValue {
        let t = s.trim();
        let numeric = t.chars().next().map(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | ',')).unwrap_or(true);
        if numeric {
            PsValue::Const(omsi_cfg::parse_f32(t))
        } else {
            PsValue::Var(t.split_whitespace().next().unwrap_or("").to_string())
        }
    }
}

/// A value and the random spread around it.
pub type PsRange = (PsValue, PsValue);

/// A particle system of a model: `[smoke]` (exhaust, boiling coolant, wheel spray, chimney
/// smoke) or `[particle_emitter]` (the fireworks and the memorial's flame), in one form.
/// OMSI (TRauch/TRauchInst, the original): at most 100 particles an
/// emitter; each flies off along `dir` at `velocity`, its speed multiplied by `brake` every
/// frame, falls with `gravity` (negative: rises), grows from `size_start` by `size_grow` a
/// second and fades from `alpha_initial` at birth to `alpha_final` at the end of `life`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParticleSystemDef {
    pub pos: [f32; 3],
    pub dir: [f32; 3],
    pub velocity: PsRange,
    /// `--PS_veloc_constvar--`: the speed is the same, the direction any (an explosion).
    pub velocity_all_round: bool,
    /// Particles a second.
    pub freq: PsRange,
    /// `--PS_instExplosion_partcount--`: this many at once (at the start, or when the
    /// particle it is attached to ends).
    pub burst: Option<PsRange>,
    pub life: PsRange,
    pub brake: PsRange,
    pub gravity: PsRange,
    pub size_start: PsRange,
    pub size_grow: PsRange,
    pub alpha_initial: PsRange,
    pub alpha_final: PsRange,
    pub rgb: [PsRange; 3],
    /// Distance from the camera within which it runs (m).
    pub calc_dist: f32,
    /// Glows (drawn additively, unlit).
    pub emissive: bool,
    /// Its picture, relative to the game folder (`texture\rauch.tga` by default).
    pub bitmap: Option<String>,
    /// `[PS_attachTo] emitter mode n`: emits from the particles of an earlier emitter of the
    /// model - mode 0 all along their way (a trail), 1 when one ends (a burst), 2 against
    /// its motion (a rocket's jet).
    pub attach: Option<(usize, u8)>,
}

impl ParticleSystemDef {
    /// `[smoke]`: position, direction, then speed, its spread, frequency, lifetime, brake
    /// factor, gravity, start size, growth, initial alpha, a line Omsi.exe skips, red,
    /// green, blue (the reader at 0x5f5e58: two lines on after the alpha, 0x5f654c). The
    /// final alpha is not read: it stays 0, as Omsi.exe sets it before the lines
    /// (0x5f5ec1) - a puff fades out over its life. (Taken from the skipped line, the
    /// stock buses' `10`, every exhaust puff went opaque within a few tenths of a second.)
    pub fn from_smoke(p: &[String]) -> ParticleSystemDef {
        let f = |i: usize| p.get(i).map(|s| omsi_cfg::parse_f32(s)).unwrap_or(0.0);
        let v = |i: usize| p.get(i).map(|s| PsValue::parse(s)).unwrap_or_default();
        let c = |i: usize| (v(i), PsValue::Const(0.0));
        ParticleSystemDef {
            pos: [f(0), f(1), f(2)],
            dir: [f(3), f(4), f(5)],
            velocity: (v(6), v(7)),
            freq: c(8),
            life: c(9),
            brake: c(10),
            gravity: c(11),
            size_start: c(12),
            size_grow: c(13),
            alpha_initial: c(14),
            alpha_final: (PsValue::Const(0.0), PsValue::Const(0.0)),
            rgb: [c(16), c(17), c(18)],
            calc_dist: 500.0,
            ..Default::default()
        }
    }

    /// `[particle_emitter]`: position and direction, then labelled pairs (value, spread).
    pub fn from_emitter(e: &ParticleEmitter) -> ParticleSystemDef {
        let lines: Vec<&str> = e.lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
        let f = |i: usize| lines.get(i).map(|s| omsi_cfg::parse_f32(s)).unwrap_or(0.0);
        let mut d = ParticleSystemDef { pos: [f(0), f(1), f(2)], dir: [f(3), f(4), f(5)], calc_dist: 500.0, ..Default::default() };
        let mut i = 6;
        let pair = |i: usize| -> PsRange {
            (lines.get(i).map(|s| PsValue::parse(s)).unwrap_or_default(), lines.get(i + 1).map(|s| PsValue::parse(s)).unwrap_or_default())
        };
        while i < lines.len() {
            let l = lines[i];
            if !l.starts_with("--") {
                i += 1;
                continue;
            }
            let label = l.trim_matches('-').to_ascii_lowercase();
            i += 1;
            match label.as_str() {
                "ps_veloc" | "ps_veloc_constvar" => {
                    d.velocity = pair(i);
                    d.velocity_all_round = label.ends_with("constvar");
                    i += 2;
                }
                "ps_freq" => { d.freq = pair(i); i += 2; }
                "ps_instexplosion_partcount" => { d.burst = Some(pair(i)); i += 2; }
                "ps_livetime" => { d.life = pair(i); i += 2; }
                "ps_brakefactor" => { d.brake = pair(i); i += 2; }
                "ps_g" => { d.gravity = pair(i); i += 2; }
                "ps_size_start" => { d.size_start = pair(i); i += 2; }
                "ps_size_grow" => { d.size_grow = pair(i); i += 2; }
                "ps_alpha_initial" => { d.alpha_initial = pair(i); i += 2; }
                "ps_alpha_final" => { d.alpha_final = pair(i); i += 2; }
                "ps_rgb" => {
                    d.rgb = [pair(i), pair(i + 2), pair(i + 4)];
                    i += 6;
                }
                "ps_calcdist" => { d.calc_dist = f(i); i += 1; }
                "ps_emissive" => d.emissive = true,
                "ps_bitmap" => {
                    d.bitmap = lines.get(i).map(|s| s.to_string()).filter(|s| !s.starts_with("--"));
                    i += 1;
                }
                _ => {}
            }
        }
        if let Some(a) = e.attach_to {
            d.attach = Some((a[0].max(0.0) as usize, a[1].clamp(0.0, 2.0) as u8));
        }
        d
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Model {
    pub path: PathBuf,
    /// Scenery render queue when a model.cfg supplies `[rendertype]` (inherited by its .sco
    /// wrapper unless the wrapper explicitly overrides it).
    pub render_type: Option<String>,
    /// `[surface]` from model.cfg, inherited by a scenery object's .sco wrapper when absent.
    pub surface: Option<bool>,
    /// Object-wide `[terrainhole]` cutters, independent of render meshes and LODs.
    pub terrain_holes: Vec<String>,
    pub lods: Vec<Lod>,
    /// The first level was opened by a `[mesh]` before any `[LOD]` (see "lod" below).
    pub implicit_lod: bool,
    pub meshes: Vec<MeshDef>,
    pub vfd_max_min: Option<[f32; 6]>,
    pub detail_factor: f32,
    pub tex_detail_factor: f32,
    pub no_distance_check: bool,
    pub ctc: Vec<Ctc>,
    pub ctc_textures: Vec<(String, String)>,
    pub script_textures: Vec<(i32, i32)>,
    pub html_textures: Vec<HtmlTextureDef>,
    pub text_textures: Vec<TextTexture>,
    /// `[texttexture_enh]` raw parameter lines.
    pub text_textures_enh: Vec<Vec<String>>,
    pub texchanges: Vec<String>,
    pub smokes: Vec<Smoke>,
    pub particle_emitters: Vec<ParticleEmitter>,
    pub spotlights: Vec<Spotlight>,
    pub spotlights_2: Vec<Spotlight2>,
    pub spotlight_cookies: Vec<SpotlightCookie>,
    pub interior_lights: Vec<InteriorLight>,
    /// `[light]` legacy lights (raw).
    pub lights: Vec<Vec<String>>,
    /// `[setvar]` lines before any `[item]`: Omsi.exe files them under an item that is
    /// never chosen, so they set nothing (kept for the record).
    pub set_vars: Vec<(String, f32)>,
    /// The model's own paint items (`[item]`: name, `[CTCTexture]` name, texture), each with
    /// the `[setvar]` lines after it - a `.cti` written into the model.cfg (0x5efae8 files
    /// a `[setvar]` under the item before it).
    pub items: Vec<ModelItem>,
    pub unknown_keywords: Vec<(String, usize)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModelItem {
    pub name: String,
    pub ctc: String,
    pub texture: String,
    pub set_vars: Vec<(String, f32)>,
}

impl Model {
    /// All declared cutters, plus legacy mesh fields supplied by programmatic callers.
    pub fn terrain_hole_meshes(&self) -> impl Iterator<Item = &str> {
        self.terrain_holes.iter().map(String::as_str).chain(
            self.meshes.iter().filter_map(|m| m.terrain_hole.as_deref())
                .filter(move |f| !self.terrain_holes.iter().any(|declared| declared.as_str() == *f)),
        )
    }

    pub fn load(path: &Path) -> Result<Model, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        Ok(Self::parse(&f))
    }

    pub fn parse(file: &CfgFile) -> Model {
        let mut m = Model { path: file.path.clone(), detail_factor: 1.0, tex_detail_factor: 1.0, ..Default::default() };
        let mut r = file.reader().disabled_blocks();
        while let Some(e) = r.next_entry(ANIM_TOKENS) {
            match e {
                Entry::Keyword(k) => {
                    if !m.handle_keyword(&k, &mut r) {
                        m.unknown_keywords.push((k, r.block_line()));
                    }
                }
                Entry::Token(t) => m.handle_token(t, &mut r),
            }
        }
        m.tidy_interior_lamps();
        m.tidy_visible();
        m
    }

    pub fn tidy_visible(&mut self) {
        for m in &mut self.meshes {
            let groups = m.visible_any.iter().filter(|g| !g.is_empty()).count();
            if groups == 1 && m.visible_any.iter().flatten().all(|c| !c.range && !c.not) {
                if let Some(last) = m.visible_any.iter().flatten().last().cloned() {
                    m.visible_any = vec![vec![last]];
                }
            }
        }
    }

    /// Handle one `[newanim]` sub-command line (one of [`ANIM_TOKENS`]): it goes to the
    /// last animation of the current mesh, wherever it stands after the `[newanim]`.
    pub fn handle_token(&mut self, t: &str, r: &mut CfgReader) {
        // `||` between two `[visible]` blocks: the next conditions are another group (#729)
        if t == "||" {
            if let Some(m) = self.meshes.last_mut().filter(|m| m.visible_any.last().is_some_and(|g| !g.is_empty())) {
                m.visible_any.push(Vec::new());
            }
            return;
        }
        let origin = match t {
            "origin_trans" => Some(AnimOrigin::Trans(r.f32s::<3>())),
            "origin_rot_x" => Some(AnimOrigin::RotX(r.f32())),
            "origin_rot_y" => Some(AnimOrigin::RotY(r.f32())),
            "origin_rot_z" => Some(AnimOrigin::RotZ(r.f32())),
            "origin_from_mesh" => Some(AnimOrigin::FromMesh),
            _ => None,
        };
        let kind = match t {
            "anim_rot" => Some(AnimKind::Rot),
            "anim_trans" => Some(AnimKind::Trans),
            _ => None,
        };
        let driven = kind.map(|_| (r.word().to_string(), r.f32()));
        let value = matches!(t, "offset" | "delay" | "maxspeed").then(|| r.f32());
        // a sub-command before any `[newanim]` of the mesh has nothing to act on (the
        // original stops reading the model there; no known file has one)
        let Some(a) = self.meshes.last_mut().and_then(|m| m.animations.last_mut()) else { return };
        if let Some(o) = origin {
            a.origins.push(o);
        }
        if let (Some(kind), Some((variable, factor))) = (kind, driven) {
            a.kind = Some(kind);
            a.variable = variable;
            a.factor = factor;
        }
        match (t, value) {
            ("offset", Some(v)) => a.offset = v,
            ("delay", Some(v)) => a.delay = v,
            ("maxspeed", Some(v)) => a.max_speed = v,
            _ => {}
        }
    }

    fn cur_mesh(&mut self) -> Option<&mut MeshDef> {
        self.meshes.last_mut()
    }

    /// The material the `matl_*` commands act on: the last one selected - except right after
    /// a `[matl_change]`, where Omsi.exe goes on with the slot's own material until the first
    /// `[matl_item]` (0x5efae8: the change keeps the slot's record as the current one, and the
    /// item later starts as a copy of it). An LED matrix that opens its change first and
    /// writes `[matl_alpha]` and its script texture after it gives them to the slot itself:
    /// kept on the change record, the slot stayed opaque and every dot of the panel shone.
    /// A slot without a `[matl]` of its own gets one here, as Omsi.exe has a record for every
    /// material of the mesh.
    fn cur_matl(&mut self) -> Option<&mut MaterialDef> {
        let mesh = self.meshes.last_mut()?;
        let (texture, index) = match mesh.materials.last() {
            Some(d) if !d.item && d.change.is_some() => (d.texture.clone(), d.index),
            _ => return mesh.materials.last_mut(),
        };
        let base = |d: &MaterialDef| !d.item && d.change.is_none() && d.index == index && d.texture.eq_ignore_ascii_case(&texture);
        let k = match mesh.materials.iter().rposition(base) {
            Some(k) => k,
            None => {
                // (before the change, which stays the last record: the commands after this
                // one still find it)
                let at = mesh.materials.len() - 1;
                mesh.materials.insert(at, MaterialDef { texture, index, ..Default::default() });
                at
            }
        };
        mesh.materials.get_mut(k)
    }

    /// `lit`, the word on the line after a page's path (see `HtmlTextureDef::lit`): taken
    /// when it is there, and only that word - anything else leaves the reader where it was.
    fn take_lit_flag(r: &mut CfgReader<'_>) -> bool {
        let mut ahead = r.clone();
        if ahead.word().eq_ignore_ascii_case("lit") {
            *r = ahead;
            true
        } else {
            false
        }
    }

    /// A text texture's full colour flag: Omsi.exe reads it with StrToInt into a byte and
    /// tests that for non-zero (0x5f0919), so the `255` some add-ons write is set - read as
    /// "not 1" the glyphs were filled with the RGB that follows (yellow on those fonts).
    fn text_full_color(r: &mut CfgReader<'_>) -> bool {
        let w = r.word();
        w.eq_ignore_ascii_case("true") || omsi_cfg::parse_i32(w) != 0
    }

    /// Handle one model.cfg keyword. Returns `false` when the keyword is not part of the
    /// model vocabulary (so the caller can try its own).
    pub fn handle_keyword(&mut self, k: &str, r: &mut CfgReader) -> bool {
        match k {
            "rendertype" => self.render_type = Some(r.word().to_ascii_lowercase()),
            "surface" => {
                let value = r.word();
                self.surface = Some(value != "0");
            }
            "lod" => {
                let min_size = r.f32();
                // Meshes written before the first [LOD] belong to that first level: OMSI gives
                // them the level index its loader holds then, which no [LOD] has set yet. The
                // WH UK AI cars (Westcountry, London) put their shadow there: as a level of its
                // own the shadow was all an AI car had (only the first level is loaded for a
                // vehicle), and the traffic drove about as shadows and lamps.
                if self.implicit_lod && self.lods.len() == 1 {
                    self.lods[0].min_size = min_size;
                    self.implicit_lod = false;
                } else {
                    self.implicit_lod = false;
                    self.lods.push(Lod { min_size, first_mesh: self.meshes.len() });
                }
            }
            "vfdmaxmin" => self.vfd_max_min = Some(r.f32s::<6>()),
            "detail_factor" => self.detail_factor = r.f32(),
            "tex_detail_factor" => self.tex_detail_factor = r.f32(),
            "nodistancecheck" => {
                if let Some(m) = self.cur_mesh() {
                    m.no_distance_check = true;
                } else {
                    self.no_distance_check = true;
                }
            }
            "terrainhole" => {
                let f = r.str().to_string();
                // This declares an object-wide cutter, often before the first [mesh].
                // Keep every command; multiple cutters may follow a single render mesh.
                self.terrain_holes.push(f.clone());
                if let Some(m) = self.cur_mesh() {
                    m.terrain_hole = Some(f);
                }
            }
            "ctc" => {
                let variable = r.str().to_string();
                let path = r.str().to_string();
                let value = r.i32();
                self.ctc.push(Ctc { variable, path, value });
            }
            "ctctexture" => {
                let a = r.str().to_string();
                let b = r.str().to_string();
                self.ctc_textures.push((a, b));
            }
            "scripttexture" => {
                let w = r.i32();
                let h = r.i32();
                self.script_textures.push((w, h));
            }
            "htmltexture" => {
                let width = r.i32();
                let height = r.i32();
                let path = r.str().to_string();
                let lit = Self::take_lit_flag(r);
                let script_index = self.script_textures.len();
                self.script_textures.push((width, height));
                self.html_textures.push(HtmlTextureDef { script_index, width, height, path, lit });
            }
            "texttexture" => {
                let variable = r.str().to_string();
                let font = r.str().to_string();
                let width = r.i32();
                let height = r.i32();
                let full_color = Self::text_full_color(r);
                let color = r.f32s::<3>();
                self.text_textures.push(TextTexture { variable, font, width, height, full_color, color, orientation: 0, grid: 1 });
            }
            "texttexture_enh" => {
                // The same first eight fields as [texttexture] and two more: orientation
                // and grid (the SDK's own list in the stock model files; all 190 stock and
                // installed uses follow it). It takes a slot in the same index space
                // [useTextTexture] counts through - the D86's number plate is texture 0 and
                // its matrix texts 3..5; without the slot every later index was off by one
                // and the destination display showed the temperature's 7-segment digits.
                let variable = r.str().to_string();
                let font = r.str().to_string();
                let width = r.i32();
                let height = r.i32();
                let full_color = Self::text_full_color(r);
                let color = r.f32s::<3>();
                let extra: Vec<String> = (0..2).map(|_| r.str().to_string()).collect();
                let orientation = omsi_cfg::parse_i32(&extra[0]);
                let grid = omsi_cfg::parse_i32(&extra[1]).max(1);
                self.text_textures.push(TextTexture { variable: variable.clone(), font: font.clone(), width, height, full_color, color, orientation, grid });
                let mut lines = vec![variable, font, width.to_string(), height.to_string()];
                lines.extend(extra);
                self.text_textures_enh.push(lines);
            }
            "mesh" => {
                if self.lods.is_empty() {
                    self.lods.push(Lod { min_size: 0.0, first_mesh: 0 });
                    self.implicit_lod = true;
                }
                let file = r.str().to_string();
                // OMSI: a new mesh takes the interior lights of the
                // mesh before it, the first one lights 0 to 3 - a mesh without its own
                // [illumination_interior] is lit like the one written before it (the GN92's
                // rear saloon, its walls and seats, has no line of its own and stayed dark)
                let illumination_interior = self.meshes.last().map(|m| m.illumination_interior.clone()).unwrap_or_else(|| vec![0, 1, 2, 3]);
                self.meshes.push(MeshDef { file, lod: self.lods.len() - 1, illumination_interior, ..Default::default() });
            }
            "item" => {
                let name = r.str().to_string();
                let ctc = r.str().to_string();
                let texture = r.str().to_string();
                self.items.push(ModelItem { name, ctc, texture, set_vars: Vec::new() });
            }
            "setvar" => {
                let n = r.str().to_string();
                let v = r.f32();
                match self.items.last_mut() {
                    Some(i) => i.set_vars.push((n, v)),
                    None => self.set_vars.push((n, v)),
                }
            }
            "mesh_ident" => {
                let s = r.str().to_string();
                if let Some(m) = self.cur_mesh() {
                    m.mesh_ident = Some(s);
                }
            }
            "viewpoint" => {
                let v = r.i32();
                if let Some(m) = self.cur_mesh() {
                    m.viewpoint = v;
                }
            }
            "shadow" => {
                if let Some(m) = self.cur_mesh() {
                    m.shadow = true;
                }
            }
            "isshadow" => {
                if let Some(m) = self.cur_mesh() {
                    m.is_shadow = true;
                }
            }
            "mouseevent" => {
                let s = r.str().to_string();
                if let Some(m) = self.cur_mesh() {
                    m.mouse_event = Some(s);
                }
            }
            "setbone" => {
                let n = r.str().to_string();
                let i = r.i32();
                if let Some(m) = self.cur_mesh() {
                    m.bones.push((n, i));
                }
            }
            "smoothskin" => {
                if let Some(m) = self.cur_mesh() {
                    m.smooth_skin = true;
                }
            }
            "animparent" => {
                let s = r.str().to_string();
                if let Some(m) = self.cur_mesh() {
                    m.anim_parent = Some(s);
                }
            }
            "newanim" => {
                if let Some(m) = self.cur_mesh() {
                    m.animations.push(Animation::default());
                }
            }
            "visible" | "visible_range" => {
                // (`var !`: the condition negated, an openOMSI extension like the range)
                let line = r.str().trim().to_string();
                let not = line.ends_with('!');
                let v = line.trim_end_matches('!').trim().to_string();
                let lo = r.f32();
                let (hi, range) = if k == "visible_range" { (r.f32(), true) } else { (lo, false) };
                if let Some(m) = self.cur_mesh() {
                    if !range && !not {
                        m.visible = Some((v.clone(), lo));
                    }
                    if m.visible_any.is_empty() {
                        m.visible_any.push(Vec::new());
                    }
                    if let Some(g) = m.visible_any.last_mut() {
                        g.push(VisibleCond { variable: v, lo: lo.min(hi), hi: lo.max(hi), range, not });
                    }
                }
            }
            "illumination" => {
                let v: Vec<i32> = (0..4).map(|_| r.i32()).collect();
                if let Some(m) = self.cur_mesh() {
                    m.illumination = v;
                }
            }
            // (OMSI's four lamps, and openOMSI takes as many more as the lines that follow
            // give)
            "illumination_interior" => {
                let v: Vec<i32> = r.i32_list(4);
                if let Some(m) = self.cur_mesh() {
                    m.illumination_interior = v;
                }
            }
            "light" => {
                let v: Vec<String> = (0..7).map(|_| r.str().to_string()).collect();
                self.lights.push(v);
            }
            "light_enh" => {
                let pos = r.f32s::<3>();
                let color = r.f32s::<3>();
                let size = r.f32();
                let variable = r.str().to_string();
                let values: Vec<f32> = (0..4).map(|_| r.f32()).collect();
                let mut texture = None;
                // optional texture line (not a keyword, not empty, not numeric)
                let save = r.pos();
                let t = r.str();
                if !t.trim().is_empty() && omsi_cfg::keyword_of(t).is_none() && t.trim().parse::<f64>().is_err() && t.contains('.') {
                    texture = Some(t.to_string());
                } else {
                    r.seek(save);
                }
                if let Some(m) = self.cur_mesh() {
                    m.light_enh.push(LightEnh { pos, color, size, variable, values, texture });
                }
            }
            "light_enh_2" => {
                let pos = r.f32s::<3>();
                let dir = r.f32s::<3>();
                let up = r.f32s::<3>();
                let omni = r.bool();
                let rotating = r.i32();
                let color = r.f32s::<3>();
                let size = r.f32();
                let cone_inner = r.f32();
                let cone_outer = r.f32();
                let variable = r.str().to_string();
                let factor = r.f32();
                let z_offset = r.f32();
                let values: Vec<String> = (0..3).map(|_| r.str().to_string()).collect();
                // the effect bitmap follows on the next line - when it names a picture (an
                // empty line keeps the standard one)
                let mut bitmap = None;
                {
                    let mut ahead = r.clone();
                    let l = ahead.str().trim().to_string();
                    let lower = l.to_ascii_lowercase();
                    if [".bmp", ".tga", ".dds", ".png", ".jpg"].iter().any(|e| lower.ends_with(e)) {
                        bitmap = Some(l);
                        *r = ahead;
                    }
                }
                let cone = values.get(1).map(|v| omsi_cfg::parse_f32(v) >= 0.5).unwrap_or(false);
                let time_const = values.get(2).map(|v| omsi_cfg::parse_f32(v).max(0.0)).unwrap_or(0.0);
                if let Some(m) = self.cur_mesh() {
                    m.light_enh_2.push(LightEnh2 { pos, dir, up, omni, rotating, color, size, cone_inner, cone_outer, variable, factor, z_offset, values, cone, time_const, bitmap });
                }
            }
            "spotlight" => self.spotlights.push(Spotlight { values: r.f32s::<12>() }),
            "spotlight_2" => {
                let values = r.f32s::<12>();
                let variable = r.str().to_string();
                // (the flag may be left out: a keyword next is not it)
                let mut ahead = r.clone();
                let flag = ahead.line();
                let mirrored = if omsi_cfg::keyword_of(flag).is_some() {
                    true
                } else {
                    *r = ahead;
                    omsi_cfg::parse_f32(flag) < 0.5
                };
                self.spotlights_2.push(Spotlight2 { values, variable, mirrored });
            }
            "spotlight_cookie" => self.spotlight_cookies.push(SpotlightCookie::read(r)),
            "interiorlight" => {
                let variable = r.str().to_string();
                let range = r.f32();
                let color = r.f32s::<3>();
                let pos = r.f32s::<3>();
                self.interior_lights.push(InteriorLight { variable, range, color, pos });
            }
            _ => return self.handle_material_keyword(k, r),
        }
        true
    }

    /// `handle_keyword`'s second half: the texture changes, `[matl]` and the commands on the
    /// material it opened, and the smoke and particle emitters.
    fn handle_material_keyword(&mut self, k: &str, r: &mut CfgReader) -> bool {
        match k {
            "texchanges" => self.texchanges.push(r.str().to_string()),
            "matl" => {
                let texture = r.str().to_string();
                let index = r.i32();
                if let Some(m) = self.cur_mesh() {
                    // [matl] selects a material and what follows changes it: a second [matl] of
                    // the same one goes on with it (TH_Wald's chain barrier gives its slot an
                    // envmap in one block and [matl_alpha] 1 in the next - the second block was
                    // lost and the chain stood on a white band)
                    let same = |d: &MaterialDef| !d.item && d.change.is_none() && d.index == index && d.texture.eq_ignore_ascii_case(&texture);
                    match m.materials.iter().position(same) {
                        Some(k) => {
                            let d = m.materials.remove(k);
                            m.materials.push(d);
                        }
                        None => m.materials.push(MaterialDef { texture, index, ..Default::default() }),
                    }
                }
            }
            "matl_change" => {
                let texture = r.str().to_string();
                let index = r.i32();
                let var = r.str().to_string();
                if let Some(m) = self.cur_mesh() {
                    m.materials.push(MaterialDef { texture: texture.clone(), index, change: Some((texture, index, var)), ..Default::default() });
                }
            }
            "matl_item" => {
                // the variant of the preceding [matl_change]: starts as a copy of the base
                // material, the following matl_* keywords change the variant only
                if let Some(mesh) = self.cur_mesh() {
                    // (a second item starts from the slot's material as well, not from the
                    // item before it: 0x5efae8 copies the record the [matl_change] picked)
                    let open = mesh.materials.last().is_some_and(|m| m.change.is_some());
                    if let Some(change) = mesh.materials.iter().rev().find(|m| open && m.change.is_some() && !m.item).cloned() {
                        // inherit the plain [matl] of the same slot (alpha, transmap, ...)
                        let base = mesh
                            .materials
                            .iter()
                            .rev()
                            .find(|m| !m.item && m.change.is_none() && m.texture.eq_ignore_ascii_case(&change.texture) && m.index == change.index)
                            .cloned();
                        // - with what was set after the [matl_change], which went to that
                        // [matl] (see `cur_matl`): Omsi.exe copies the whole material record
                        // into the new item (0x5efae8 -> 0x40a058), its
                        // free texture, scrolling and border with it. Without them the SD77's
                        // lit roller blind (`lights_stand`) lost its destination text and
                        // scrolling with the lights on (#1419).
                        let mut item = change.clone();
                        if let Some(base) = base {
                            item.inherit(&base);
                        }
                        item.item = true;
                        mesh.materials.push(item);
                    }
                }
            }
            "matl_raindropmap" => {
                let t = r.str().to_string();
                if let Some(m) = self.cur_matl() {
                    m.raindropmap = Some(t);
                }
            }
            "matl_texadress_mirror" => {
                if let Some(m) = self.cur_matl() {
                    m.tex_address = TexAddress::Mirror;
                }
            }
            "matl_texadress_clamp" => {
                if let Some(m) = self.cur_matl() {
                    m.tex_address = TexAddress::Clamp;
                }
            }
            "matl_texadress_border" => {
                let c = r.f32s::<4>();
                if let Some(m) = self.cur_matl() {
                    m.tex_address = TexAddress::Border;
                    m.border_color = c;
                }
            }
            "matl_texadress_mirroronce" => {
                if let Some(m) = self.cur_matl() {
                    m.tex_address = TexAddress::MirrorOnce;
                }
            }
            "texcoordtransx" => {
                let v = r.str().to_string();
                if let Some(m) = self.cur_matl() {
                    m.texcoord_trans_x = Some(v);
                }
            }
            "texcoordtransy" => {
                let v = r.str().to_string();
                if let Some(m) = self.cur_matl() {
                    m.texcoord_trans_y = Some(v);
                }
            }
            "usescripttexture" => {
                let v = r.i32();
                if let Some(m) = self.cur_matl() {
                    m.use_script_texture = Some(v);
                }
            }
            "usehtmltexture" => {
                let v = r.i32();
                let index = usize::try_from(v)
                    .ok()
                    .and_then(|n| self.html_textures.get(n))
                    .map(|d| d.script_index as i32);
                if let (Some(m), Some(i)) = (self.cur_matl(), index) {
                    m.use_script_texture = Some(i);
                }
            }
            "usetexttexture" => {
                let v = r.i32();
                if let Some(m) = self.cur_matl() {
                    m.use_text_texture = Some(v);
                }
            }
            "alphascale" => {
                let v = r.str().to_string();
                if let Some(m) = self.cur_matl() {
                    m.alphascale = Some(v);
                }
            }
            "matl_freetex" => {
                let t = r.str().to_string();
                let v = r.str().to_string();
                if let Some(m) = self.cur_matl() {
                    m.freetexes.push((t.clone(), v.clone()));
                    m.freetex = Some((t, v));
                }
            }
            "matl_lightmap" => {
                let t = r.str().to_string();
                let v = r.str().to_string();
                if let Some(m) = self.cur_matl() {
                    m.lightmaps.push((t.clone(), v.clone()));
                    m.lightmap = Some((t, v));
                }
            }
            "matl_nightmap" => {
                let t = r.str().to_string();
                if let Some(m) = self.cur_matl() {
                    m.nightmap = Some(t);
                }
            }
            "matl_glow" => {
                // [matl_glow] <texture> <value>: the material is its own light. The picture is
                // a greyscale mask of how much shines where, the light is the material's own
                // colour (see `MaterialDef::glow`). A slot that also names a [matl_lightmap]
                // keeps that, and the classic picture is left as if the keyword were absent.
                let t = r.str().to_string();
                let v = r.f32();
                if let Some(m) = self.cur_matl() {
                    m.glow = Some((t, v));
                }
            }
            "led_glow_effect" => {
                // (a value out of 0..1 is taken to the nearer end; a line that is no number
                // reads as 0, as any number of the format does)
                let v = r.f32();
                if let Some(m) = self.cur_matl() {
                    m.led_glow = Some(if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 });
                }
            }
            "matl_allcolor" => {
                let v = r.f32s::<14>();
                if let Some(m) = self.cur_matl() {
                    m.allcolor = Some(v);
                }
            }
            "matl_alpha" => {
                let v = r.i32();
                if let Some(m) = self.cur_matl() {
                    m.alpha = v;
                    m.alpha_set = true;
                }
            }
            "matl_nozwrite" => {
                if let Some(m) = self.cur_matl() {
                    m.no_z_write = true;
                }
            }
            "matl_nozcheck" => {
                if let Some(m) = self.cur_matl() {
                    m.no_z_check = true;
                }
            }
            "matl_zbias" => {
                let v = r.i32();
                if let Some(m) = self.cur_matl() {
                    m.z_bias = v;
                }
            }
            "matl_envmap" => {
                let t = r.str().to_string();
                let f = r.f32();
                if let Some(m) = self.cur_matl() {
                    m.envmap = Some((t, f));
                }
            }
            "matl_envmaprealtime" => {
                if let Some(m) = self.cur_matl() {
                    m.envmap_realtime = true;
                }
            }
            "matl_bumpmap" => {
                let t = r.str().to_string();
                let f = r.f32();
                if let Some(m) = self.cur_matl() {
                    m.bumpmap = Some((t, f));
                }
            }
            "matl_envmap_mask" => {
                let t = r.str().to_string();
                if let Some(m) = self.cur_matl() {
                    m.envmap_mask = Some(t);
                }
            }
            "matl_transmap" => {
                let t = r.str().to_string();
                if let Some(m) = self.cur_matl() {
                    m.transmap = Some(t);
                }
            }
            "smoke" => {
                let params: Vec<String> = (0..19).map(|_| r.str().to_string()).collect();
                self.smokes.push(Smoke { params });
            }
            "particle_emitter" => {
                let lines: Vec<String> = r.rest_of_block().into_iter().map(|s| s.to_string()).collect();
                self.particle_emitters.push(ParticleEmitter { lines, attach_to: None });
            }
            "ps_attachto" => {
                let v = r.f32s::<3>();
                if let Some(p) = self.particle_emitters.last_mut() {
                    p.attach_to = Some(v);
                }
            }
            _ => return false,
        }
        true
    }

    /// The model's particle systems: its `[particle_emitter]`s in file order (the numbers
    /// `[PS_attachTo]` counts in), then its `[smoke]`s.
    pub fn particle_systems(&self) -> Vec<ParticleSystemDef> {
        self.particle_emitters
            .iter()
            .map(ParticleSystemDef::from_emitter)
            .chain(self.smokes.iter().map(|s| ParticleSystemDef::from_smoke(&s.params)))
            .collect()
    }

    /// Meshes belonging to LOD `i`.
    pub fn lod_meshes(&self, i: usize) -> &[MeshDef] {
        let start = self.lods[i].first_mesh;
        let end = self.lods.get(i + 1).map(|l| l.first_mesh).unwrap_or(self.meshes.len());
        &self.meshes[start..end]
    }
}

/// The `[newanim]` sub-commands. The original's model loader compares them with whole lines
/// the way it compares keywords (so the EN92 ignition key's tab-indented origin lines, the
/// BR481 mirrors' `origin_rot_Y` and the F90 lorry's indented, switched-off second rear axle
/// are free text), anywhere after the `[newanim]` they belong to.
pub const ANIM_TOKENS: &[&str] = &["origin_trans", "origin_rot_x", "origin_rot_y", "origin_rot_z", "origin_from_mesh", "anim_rot", "anim_trans", "offset", "delay", "maxspeed", "||"];

/// One `[newtexchangemaster]` of a `[texchanges]` file: the texture name used in the mesh,
/// the script variable that picks a replacement, and the replacements themselves.
///
/// The variable holds the index of the entry (0 = the first one, as the SD200's
/// `rlbnd_lnN_bmp` shows: the roller position is clamped to 0…15 for sixteen entries).
/// The texture named by the master usually does not exist on disk at all - the mesh only
/// carries the name so that the master can be found.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TexChangeMaster {
    pub texture: String,
    pub variable: String,
    pub entries: Vec<String>,
    /// Folder of the `.cfg` the master came from: the entries live next to it.
    pub dir: PathBuf,
}

impl TexChangeMaster {
    /// The entry a variable value selects, or `None` when the value is outside the list.
    pub fn entry(&self, value: f32) -> Option<&str> {
        if !value.is_finite() {
            return None;
        }
        let i = value.trunc() as i64;
        usize::try_from(i).ok().and_then(|i| self.entries.get(i)).map(|s| s.as_str())
    }
}

/// Read one `[texchanges]` file (`chtex_*.cfg`).
pub fn parse_texchanges(file: &CfgFile) -> Vec<TexChangeMaster> {
    let dir = file.path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut out: Vec<TexChangeMaster> = Vec::new();
    let mut r = file.reader();
    while let Some(k) = r.next_keyword() {
        match k.as_str() {
            "newtexchangemaster" => {
                let texture = r.str().trim().to_string();
                let variable = r.word().to_string();
                out.push(TexChangeMaster { texture, variable, entries: Vec::new(), dir: dir.clone() });
            }
            "entries" => {
                let n = r.count();
                let entries: Vec<String> = (0..n).map(|_| r.str().trim().to_string()).collect();
                if let Some(m) = out.last_mut() {
                    m.entries = entries;
                }
            }
            _ => {}
        }
    }
    out
}

/// Load every `[texchanges]` file a model refers to. The paths are relative to the
/// vehicle's own folder (`texture\chtex_SD.cfg`, `..\Anzeigen\Rollband_SD79\chtex_rollband.cfg`),
/// not to the folder of the `model.cfg`.
pub fn load_texchanges(base: &Path, files: &[String]) -> Vec<TexChangeMaster> {
    let mut out = Vec::new();
    for f in files {
        let path = omsi_cfg::resolve_path(base, f);
        match CfgFile::read(&path) {
            Ok(cfg) => out.extend(parse_texchanges(&cfg)),
            Err(e) => log::warn!("[texchanges] {}: {e}", path.display()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_visible_names_ignore_comparisons_but_declared_zero_is_compared() {
        for value in 0..=2 {
            let model = super::Model::parse(&omsi_cfg::CfgFile::from_str(
                "visibility.cfg",
                &format!("[mesh]\nindicator.o3d\n[visible]\nmissing\n{value}\n"),
            ));
            let mesh = &model.meshes[0];
            assert_eq!(mesh.visible_by(|_| None), Some(true));
            assert_eq!(mesh.visible_by(|_| Some(0.0)), Some(value == 0));
        }
        assert_eq!(super::MeshDef::default().visible_by(|_| None), None);
    }

    /// `[visible]` conditions combined (#729): plain `[visible]` blocks alone keep the last,
    /// with a range they all hold, a `!` turns one round, `||` between blocks gives another
    /// group; one alone is as before.
    #[test]
    fn several_visible_conditions_combine() {
        let text = "[mesh]\na.o3d\n[visible]\ndoor\n1\n[visible]\nlight\n1\n\n[mesh]\ne.o3d\n[visible_range]\ndoor\n1\n1\n[visible]\nlight\n1\n\n[mesh]\nb.o3d\n[visible_range]\ngear\n1\n4\n\n[mesh]\nc.o3d\n[visible]\ndoor !\n1\n||\n[visible]\nlight\n1\n\n[mesh]\nd.o3d\n[visible]\ndoor\n2\n";
        let m = super::Model::parse(&omsi_cfg::CfgFile::from_str("model.cfg", text));
        let vars = |door: f32, light: f32, gear: f32| move |n: &str| match n {
            "door" => Some(door),
            "light" => Some(light),
            "gear" => Some(gear),
            _ => None,
        };
        let shown = |i: usize, d, l, g| m.meshes[i].visible_by(vars(d, l, g)).unwrap();
        assert!(shown(0, 1.0, 1.0, 0.0) && !shown(0, 1.0, 0.0, 0.0) && shown(0, 0.0, 1.0, 0.0));
        assert_eq!(m.meshes[0].visible, Some(("light".to_string(), 1.0)));
        assert!(shown(1, 1.0, 1.0, 0.0) && !shown(1, 1.0, 0.0, 0.0) && !shown(1, 0.0, 1.0, 0.0));
        assert!(shown(2, 0.0, 0.0, 1.0) && shown(2, 0.0, 0.0, 4.0) && !shown(2, 0.0, 0.0, 5.0) && !shown(2, 0.0, 0.0, 0.0));
        assert!(shown(3, 0.0, 0.0, 0.0) && shown(3, 1.0, 1.0, 0.0) && !shown(3, 1.0, 0.0, 0.0));
        assert!(shown(4, 2.0, 0.0, 0.0) && !shown(4, 1.0, 0.0, 0.0));
        assert_eq!(m.meshes[4].visible, Some(("door".to_string(), 2.0)));
        assert_eq!(m.meshes[3].visible, Some(("light".to_string(), 1.0)));
        assert!(m.unknown_keywords.is_empty(), "{:?}", m.unknown_keywords);
    }


    #[test]
    fn compatibility_fixture_preserves_material_slots_and_variant_inheritance() {
        let file = omsi_cfg::CfgFile::from_str(
            "synthetic-material.cfg",
            include_str!("../tests/fixtures/material-compatibility.cfg"),
        );
        // OMSI's known keywords require their original spelling. A typo in a
        // synthetic fixture must not masquerade as missing material support.
        for line in file.lines.iter().filter(|l| l.starts_with('[')) {
            assert!(omsi_cfg::keyword_of(line).is_some(), "invalid fixture keyword: {line}");
        }
        let model = super::Model::parse(&file);
        let materials = &model.meshes[0].materials;
        assert_eq!(materials.len(), 4);
        let base = &materials[0];
        assert_eq!((base.alpha, base.alpha_set), (2, true));
        assert!(base.no_z_write && base.no_z_check);
        assert_eq!(base.z_bias, 1);
        assert_eq!(base.alphascale.as_deref(), Some("display_alpha"));
        assert_eq!(base.freetex, Some(("display.dds".into(), "display_texture".into())));
        assert_eq!(base.lightmaps, vec![("bright.dds".into(), "bright".into()), ("dim.dds".into(), "dim".into())]);
        assert_eq!(base.lightmap, base.lightmaps.last().cloned());
        let other = &materials[1];
        assert_eq!((other.texture.as_str(), other.index), ("display.dds", 1));
        assert!(!other.alpha_set && other.lightmaps.is_empty());
        let variant = &materials[3];
        assert!(variant.item);
        assert_eq!(variant.change, Some(("DISPLAY.dds".into(), 0, "display_mode".into())));
        assert_eq!(variant.alpha, base.alpha);
        assert_eq!((variant.no_z_write, variant.no_z_check, variant.z_bias),
            (base.no_z_write, base.no_z_check, base.z_bias));
        assert_eq!(variant.alphascale, base.alphascale);
        assert_eq!(variant.freetex, base.freetex);
        assert_eq!(variant.lightmaps, base.lightmaps);
        assert_eq!(variant.transmap.as_deref(), Some("mask.dds"));
        assert!(base.transmap.is_none(), "variant changes must not mutate the base slot");
    }

    #[test]
    fn terrain_hole_meshes_are_independent_of_render_meshes() {
        let mut model = super::Model::parse(&omsi_cfg::CfgFile::from_str(
            "cutters.cfg",
            "[terrainhole]\nfirst.o3d\n[terrainhole]\nsecond.o3d\n",
        ));
        assert!(model.meshes.is_empty());
        assert!(model.lods.is_empty());
        assert_eq!(model.terrain_hole_meshes().collect::<Vec<_>>(), ["first.o3d", "second.o3d"]);

        model = super::Model::parse(&omsi_cfg::CfgFile::from_str(
            "cutters.cfg",
            "[terrainhole]\nfirst.o3d\n[mesh]\nvisible.o3d\n[terrainhole]\nsecond.o3d\n[terrainhole]\nthird.o3d\n",
        ));
        assert_eq!(model.meshes.len(), 1);
        assert_eq!(model.meshes[0].file, "visible.o3d");
        assert_eq!(model.meshes[0].terrain_hole.as_deref(), Some("third.o3d"));
        assert_eq!(model.terrain_hole_meshes().collect::<Vec<_>>(), ["first.o3d", "second.o3d", "third.o3d"]);

        model.meshes.push(super::MeshDef {
            terrain_hole: Some("legacy.o3d".into()),
            ..Default::default()
        });
        assert_eq!(model.terrain_hole_meshes().collect::<Vec<_>>(), ["first.o3d", "second.o3d", "third.o3d", "legacy.o3d"]);
    }

    /// `[spotlight_2]`: the spot's numbers, its variable and whether it has a mirrored twin
    /// (yes unless the flag says 1, also when the flag is left out).
    #[test]
    fn a_spotlight_2_reads_its_variable_and_mirror_flag() {
        let spot = "0\n5.95\n0.652\n0\n1\n-0.3\n255\n255\n233\n200\n30\n80\n";
        let text = format!("[spotlight]\n{spot}\n[spotlight_2]\n{spot}lights_fern\n0\n\n[spotlight_2]\n{spot}door_light\n1\n[spotlight_2]\n{spot}lights_nebel\n[mesh]\nbody.o3d\n");
        let m = super::Model::parse(&omsi_cfg::CfgFile::from_str("model.cfg", &text));
        assert_eq!(m.spotlights.len(), 1);
        let s = &m.spotlights_2;
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].values, m.spotlights[0].values);
        assert_eq!((s[0].variable.as_str(), s[0].mirrored), ("lights_fern", true));
        assert_eq!((s[1].variable.as_str(), s[1].mirrored), ("door_light", false));
        assert_eq!((s[2].variable.as_str(), s[2].mirrored), ("lights_nebel", true));
        assert_eq!(m.meshes.len(), 1);
    }

    /// Two [matl] blocks of one material are one material (Absperrung_grau.sco).
    #[test]
    fn a_second_matl_block_goes_on_with_the_same_material() {
        let f = omsi_cfg::CfgFile::from_str("x.sco", "[mesh]\nx.o3d\n\n[matl]\nAbsperr_gr.dds\n0\n[matl_envmap]\nenvmap_Glas.dds\n0.03\n\n[matl]\nOther.dds\n0\n\n[matl]\nAbsperr_gr.dds\n0\n[matl_alpha]\n1\n");
        let m = super::Model::parse(&f);
        let mats = &m.meshes[0].materials;
        assert_eq!(mats.len(), 2, "{mats:?}");
        let a = mats.iter().find(|d| d.texture == "Absperr_gr.dds").unwrap();
        assert_eq!(a.alpha, 1);
        assert!(a.envmap.is_some());
    }

    /// `[matl_glow] <texture> <value>`: the material is its own light - the picture and the
    /// strength are kept. Additive: a slot that also has a `[matl_lightmap]` keeps it.
    #[test]
    fn a_matl_glow_reads_its_picture_and_strength() {
        let text = "[mesh]\nled.o3d\n\n[matl]\nled.dds\n0\n[matl_glow]\nled.png\n20\n";
        let m = Model::parse(&omsi_cfg::CfgFile::from_str("model.cfg", text));
        let (tex, v) = m.meshes[0].materials[0].glow.clone().expect("a [matl_glow]");
        assert_eq!(tex, "led.png");
        assert_eq!(v, 20.0);
        // the slot after it still reads as itself (an old .cfg that wrote a third number
        // there leaves a line the material manager has no use for, and nothing else)
        let text = "[mesh]\nled.o3d\n\n[matl]\nled.dds\n0\n[matl_glow]\nled.png\n5\n[matl_alpha]\n1\n";
        let m = Model::parse(&omsi_cfg::CfgFile::from_str("model.cfg", text));
        let (_, v) = m.meshes[0].materials[0].glow.clone().expect("a [matl_glow]");
        assert_eq!(v, 5.0);
        assert_eq!(m.meshes[0].materials[0].alpha, 1, "the keyword after it still counts");
        let both = "[mesh]\nled.o3d\n\n[matl]\nled.dds\n0\n[matl_lightmap]\nl.bmp\nlights\n[matl_glow]\nled.png\n5\n";
        let m = Model::parse(&omsi_cfg::CfgFile::from_str("model.cfg", both));
        let d = &m.meshes[0].materials[0];
        assert!(d.lightmap.is_some(), "{d:?}");
        assert_eq!(d.glow.as_ref().map(|g| g.1), Some(5.0));
    }

    /// A mesh before the first [LOD] belongs to that level (the WH UK AI cars' shadow).
    #[test]
    fn a_mesh_before_the_first_lod_joins_it() {
        let text = "[mesh]\nshadow.o3d\n\n[LOD]\n0.075\n[mesh]\nbody.o3d\n\n[LOD]\n0.04\n[mesh]\nlow.o3d\n";
        let m = Model::parse(&omsi_cfg::CfgFile::from_str("car.cfg", text));
        assert_eq!(m.lods.len(), 2);
        assert_eq!(m.lods[0].min_size, 0.075);
        let first: Vec<&str> = m.lod_meshes(0).iter().map(|d| d.file.as_str()).collect();
        assert_eq!(first, vec!["shadow.o3d", "body.o3d"]);
        assert_eq!(m.lod_meshes(1)[0].file, "low.o3d");
    }

    use super::*;

    /// The SD202's exhaust `[smoke]` read as Omsi.exe reads it: its 16th line (`10`) is
    /// skipped, the colour is the three after it and a puff fades out to nothing.
    #[test]
    fn a_smoke_skips_the_line_after_its_alpha_and_fades_out() {
        let text = "[smoke]\n-1.100\n-5.334\n0.406\n-1\n-0.7\n0\nauspuff_vel\n0.2\nauspuff_freq\nauspuff_leben\n0.95\n-0.2\n0.5\n3\nauspuff_alpha\n10\n0.66\n0.66\n0.8\n";
        let m = Model::parse(&CfgFile::from_str("model.cfg", text));
        let d = &m.particle_systems()[0];
        assert_eq!(d.pos, [-1.1, -5.334, 0.406]);
        assert_eq!(d.velocity, (PsValue::Var("auspuff_vel".into()), PsValue::Const(0.2)));
        assert_eq!(d.freq.0, PsValue::Var("auspuff_freq".into()));
        assert_eq!(d.life.0, PsValue::Var("auspuff_leben".into()));
        assert_eq!((d.brake.0.clone(), d.gravity.0.clone()), (PsValue::Const(0.95), PsValue::Const(-0.2)));
        assert_eq!((d.size_start.0.clone(), d.size_grow.0.clone()), (PsValue::Const(0.5), PsValue::Const(3.0)));
        assert_eq!(d.alpha_initial.0, PsValue::Var("auspuff_alpha".into()));
        assert_eq!(d.alpha_final.0, PsValue::Const(0.0));
        assert_eq!(d.rgb.clone().map(|c| c.0), [PsValue::Const(0.66), PsValue::Const(0.66), PsValue::Const(0.8)]);
    }

    /// `[setvar]` belongs to the `[item]` before it (a paint scheme in the model.cfg), as
    /// Omsi.exe files it; one before any item sets nothing.
    #[test]
    fn setvar_belongs_to_the_item_before_it() {
        let text = "[setvar]\nlost\n1\n[item]\nBVG\nbody\nbvg.dds\n[setvar]\nDisplay_Type\n2\n[item]\nHVL\nbody\nhvl.dds\n";
        let m = Model::parse(&CfgFile::from_str("model.cfg", text));
        assert_eq!(m.items.len(), 2);
        assert_eq!(m.items[0].set_vars, vec![("Display_Type".to_string(), 2.0)]);
        assert!(m.items[1].set_vars.is_empty());
        assert_eq!((m.items[1].name.as_str(), m.items[1].ctc.as_str(), m.items[1].texture.as_str()), ("HVL", "body", "hvl.dds"));
        assert_eq!(m.set_vars, vec![("lost".to_string(), 1.0)]);
    }

    /// A text texture's full colour flag is set by any non-zero number, as Omsi.exe tests it:
    /// add-ons that write 255 there drew their texts in the yellow that follows.
    #[test]
    fn text_texture_full_colour_flag_is_any_non_zero_value() {
        let text = |flag: &str| {
            format!("[texttexture]\nnr\nArial\n256\n32\n{flag}\n255\n255\n0\n[texttexture_enh]\nnr\nArial\n256\n32\n{flag}\n255\n255\n0\n1\n1\n")
        };
        for (flag, want) in [("255", true), ("1", true), ("true", true), ("0", false)] {
            let m = Model::parse(&CfgFile::from_str("model.cfg", &text(flag)));
            assert!(m.text_textures.iter().all(|t| t.full_color == want), "{flag}");
            assert_eq!(m.text_textures.len(), 2);
        }
    }

    /// Each `[matl_item]` keeps the slot's free textures and adds its own; what one item
    /// adds stays out of the slot and out of the other items.
    #[test]
    fn free_textures_are_listed_per_record() {
        let text = "[mesh]\npanel.o3d\n[matl_change]\nface.dds\n0\npanel_power\n[matl_freetex]\nface.dds\npanel_face\n[matl_item]\n[matl_nightmap]\nglow_off.bmp\n[matl_freetex]\nglow_off.bmp\npanel_glow\n[matl_item]\n[matl_lightmap]\nlit.bmp\npanel_lit\n";
        let m = Model::parse(&CfgFile::from_str("model.cfg", text));
        let mats = &m.meshes[0].materials;
        let plain = mats.iter().find(|x| !x.item && x.change.is_none()).unwrap();
        let items: Vec<&MaterialDef> = mats.iter().filter(|x| x.item).collect();
        let vars = |d: &MaterialDef| d.freetexes.iter().map(|f| f.1.clone()).collect::<Vec<_>>();
        assert_eq!(vars(plain), ["panel_face"]);
        assert_eq!(vars(items[0]), ["panel_face", "panel_glow"]);
        assert_eq!(vars(items[1]), ["panel_face"]);
    }

    /// A `[matl_item]` is a copy of the material before it: the SD77's lit roller blind
    /// keeps its free texture, scrolling and border (#1419).
    #[test]
    fn a_material_item_copies_the_change_before_it() {
        let text = "[mesh]\nrollo.o3d\n[matl_change]\nzielband1.bmp\n0\nlights_stand\n[matl_freetex]\nzielband1.bmp\nRollband_Tex_V\n[texcoordtransY]\nrlbnd_ziel_trans\n[matl_texadress_border]\n255\n255\n255\n0\n[matl_alpha]\n1\n[matl_item]\n[matl_lightmap]\nzielbeleuchtung.bmp\n";
        let m = Model::parse(&CfgFile::from_str("model.cfg", text));
        let item = m.meshes[0].materials.iter().find(|x| x.item).expect("the item");
        assert_eq!(item.freetex.as_ref().map(|f| f.1.as_str()), Some("Rollband_Tex_V"));
        assert_eq!(item.texcoord_trans_y.as_deref(), Some("rlbnd_ziel_trans"));
        assert_eq!((item.alpha, item.tex_address), (1, TexAddress::Border));
        assert_eq!(item.lightmap.as_ref().map(|l| l.0.as_str()), Some("zielbeleuchtung.bmp"));
        let base = m.meshes[0].materials.iter().find(|x| !x.item).unwrap();
        assert!(base.lightmap.is_none(), "the lit map is the item's only");
    }

    #[test]
    fn an_html_texture_takes_a_script_texture_index() {
        let text = "[scripttexture]\n64\n32\n\n[htmltexture]\n800\n480\nhtml\\demo.html\n\n[mesh]\nx.o3d\n\n[matl]\nx.dds\n0\n[useHtmlTexture]\n0\n";
        let m = Model::parse(&CfgFile::from_str("model.cfg", text));
        assert_eq!(m.script_textures, vec![(64, 32), (800, 480)]);
        assert_eq!(m.html_textures.len(), 1);
        assert_eq!(m.html_textures[0].script_index, 1);
        assert_eq!(m.html_textures[0].path, "html\\demo.html");
        assert_eq!(m.meshes[0].materials[0].use_script_texture, Some(1));
    }

    /// `lit` after a page's path: the page is a surface rather than a display, and the
    /// keyword after it is still read as a keyword.
    #[test]
    fn a_page_can_be_declared_lit() {
        let text = "[htmltexture]\n800\n480\nhtml\\demo.html\nlit\n\n[mesh]\nx.o3d\n\n[matl]\nx.dds\n0\n[useHtmlTexture]\n0\n";
        let m = Model::parse(&omsi_cfg::CfgFile::from_str("model.cfg", text));
        assert_eq!(m.html_textures.len(), 1);
        assert!(m.html_textures[0].lit, "the word after the path");
        assert_eq!(m.meshes[0].materials[0].use_script_texture, Some(0), "the slots still bind the page");
        let text = "[htmltexture]\n800\n480\nhtml\\demo.html\n\n[mesh]\nx.o3d\n\n[matl]\nx.dds\n0\n[useHtmlTexture]\n0\n";
        let m = Model::parse(&omsi_cfg::CfgFile::from_str("model.cfg", text));
        assert!(!m.html_textures[0].lit, "without the word it stays a display");
        assert_eq!(m.meshes[0].materials[0].use_script_texture, Some(0));
        let text = "[htmltexture]\n64\n32\nsign.html\nlit\n\n[htmltexture]\n64\n32\nother.html\n\n[mesh]\nx.o3d\n";
        let m = Model::parse(&omsi_cfg::CfgFile::from_str("model.cfg", text));
        assert!(m.html_textures[0].lit && !m.html_textures[1].lit, "each page has its own word");
        assert!(m.html_page_lit(0) && !m.html_page_lit(1), "the lookup every consumer asks");
        assert_eq!(m.html_page(1).map(|p| p.path.as_str()), Some("other.html"));
        assert!(m.html_page(2).is_none(), "no page of that script index");
    }

    /// A tab-indented block (the stock F90 lorry's second rear axle, whose mesh does not
    /// exist) is switched off: no mesh, and its lines do not reach the animation above.
    #[test]
    fn indented_block_is_free_text() {
        let text = "[mesh]\na.o3d\n\n[newanim]\norigin_rot_y\n-90\nanim_trans\nAxle_Suspension_1_R\n1\n\n\t[mesh]\n\tb.o3d\n\t[newanim]\n\torigin_from_mesh\n\tanim_rot\n\tWheel_Rotation_1_R\n\t57.3\n";
        let m = Model::parse(&CfgFile::from_str("model.cfg", text));
        assert_eq!(m.meshes.len(), 1);
        let a = &m.meshes[0].animations[0];
        assert_eq!(a.origins, vec![AnimOrigin::RotY(-90.0)]);
        assert_eq!((a.kind, a.variable.as_str()), (Some(AnimKind::Trans), "Axle_Suspension_1_R"));
    }

    /// The EN92's ignition key: indented origin lines are free text, the column-0 ones count;
    /// `origin_rot_Y` is not a sub-command; one after another keyword still is.
    #[test]
    fn animation_sub_commands() {
        let text = "[mesh]\nkey.o3d\n[newanim]\n\torigin_trans\n\t-0.634\n\t5.255\n\t1.371\n\n\torigin_rot_x\n\t-23.9\norigin_from_mesh\nanim_rot\ncp_schluessel_rot\n-90\n\n[newanim]\norigin_rot_Y\n90\norigin_rot_z\n90\n[matl]\nkey.bmp\n0\nmaxspeed\n30\n";
        let m = Model::parse(&CfgFile::from_str("model.cfg", text));
        let anims = &m.meshes[0].animations;
        assert_eq!(anims[0].origins, vec![AnimOrigin::FromMesh]);
        assert_eq!((anims[0].kind, anims[0].variable.as_str(), anims[0].factor), (Some(AnimKind::Rot), "cp_schluessel_rot", -90.0));
        assert_eq!(anims[1].origins, vec![AnimOrigin::RotZ(90.0)]);
        assert_eq!(anims[1].max_speed, 30.0);
        assert_eq!(m.meshes[0].materials[0].texture, "key.bmp");
    }

    #[test]
    fn disabled_meshes() {
        let text = "[mesh]\na.o3d\n-<DISABLED>-\nSchalter06\n[mesh]\nb.o3d\n[newanim]\nanim_rot\nx\n1\n-<ENABLED>-\n[mesh]\nc.o3d\n";
        let m = Model::parse(&CfgFile::from_str("model.cfg", text));
        let files: Vec<&str> = m.meshes.iter().map(|m| m.file.as_str()).collect();
        assert_eq!(files, vec!["a.o3d", "c.o3d"]);
    }

    /// `[matl_alpha]` is marked as given where a block has one: a `[matl]` without it
    /// leaves the flag off, and a second block of the same material that has it turns it on
    /// for the one definition both blocks make.
    #[test]
    fn repeated_matl_marks_its_own_alpha() {
        let text = "[mesh]\nb.o3d\n[matl]\nchain.dds\n0\n[matl_envmap]\nenv.dds\n0.03\n\n[matl]\nother.dds\n0\n";
        let m = Model::parse(&CfgFile::from_str("x.sco", text));
        assert!(m.meshes[0].materials.iter().all(|d| !d.alpha_set));
        let text = format!("{text}\n[matl]\nchain.dds\n0\n[matl_alpha]\n1\n");
        let m = Model::parse(&CfgFile::from_str("x.sco", &text));
        let d = &m.meshes[0].materials;
        assert_eq!(d.len(), 2);
        let chain = d.iter().find(|d| d.texture == "chain.dds").unwrap();
        assert!(chain.envmap.is_some() && chain.alpha_set && chain.alpha == 1);
    }
}
