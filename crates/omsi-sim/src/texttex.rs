//! `[texttexture]`: textures generated from script string variables with `.oft` fonts.
//!
//! Every text texture is identified by its index in the model's `[texttexture]` list;
//! materials refer to it with `[useTextTexture] n`.

use hashbrown::HashMap;
use omsi_content::dotfont::{DisplayFontSpec, DotFont, DotGrid, LetterBand, VectorFace};
use omsi_content::font::{Font, FontAtlas};
use omsi_model::TextTexture;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct FontLibrary {
    atlases: HashMap<String, Option<Arc<FontAtlas>>>,
    root: std::path::PathBuf,
    /// Every `[newfont]` of every content root's `Fonts/*.oft`, in lookup order; read once.
    index: Option<Vec<Font>>,
    /// Every face of the TrueType/OpenType fonts of the content roots' `Fonts` and of the
    /// system; read once, when a display font's file is not where it was chosen.
    faces: Option<Vec<VectorFace>>,
    /// The display fonts chosen: a vector font's file and face (by the font), an `.oft` font
    /// size's dots and where its letters sit (by its name, lower case); and the atlases made
    /// of them for the displays they are drawn on (by the choice, the display's own font -
    /// its name and file - and its line).
    vector_files: HashMap<String, Option<Arc<(Vec<u8>, u32)>>>,
    oft_dots: HashMap<String, Option<Arc<(DotFont, LetterBand)>>>,
    drawn: HashMap<(String, String, u32), Option<Arc<FontAtlas>>>,
}

/// Style words at the end of a font name ("churafont++ 32x8 Bold").
const STYLE_WORDS: &[&str] = &["bold", "heavy", "black", "light", "thin", "medium", "regular", "italic", "narrow", "condensed", "wide"];

/// A font name without its trailing style words, lower case: the family and size.
fn family_of(name: &str) -> String {
    let mut words: Vec<String> = name.split_whitespace().map(|w| w.to_ascii_lowercase()).collect();
    while words.len() > 1 && words.last().map(|w| STYLE_WORDS.contains(&w.as_str())).unwrap_or(false) {
        words.pop();
    }
    words.join(" ")
}

/// The cell size a font's name carries ("Krueger 16x9", "churafont++ Numeric 26x11 Bold"),
/// used to keep a substitute the same size as the font a script asked for.
fn size_of(name: &str) -> Option<(u32, u32)> {
    name.split_whitespace().find_map(|w| {
        let w = w.to_ascii_lowercase();
        let (a, b) = w.split_once('x')?;
        Some((a.parse().ok()?, b.parse().ok()?))
    })
}

impl FontLibrary {
    pub fn new(root: &Path) -> FontLibrary {
        FontLibrary { atlases: HashMap::new(), root: root.to_path_buf(), index: None, faces: None, vector_files: HashMap::new(), oft_dots: HashMap::new(), drawn: HashMap::new() }
    }

    /// A library of the fonts `fonts` (each `[newfont]`, in lookup order: what `installed`
    /// gave) instead of those of the content roots - the launcher's previews, which have the
    /// list already.
    pub fn with_fonts(root: &Path, fonts: Vec<Font>) -> FontLibrary {
        FontLibrary { index: Some(fonts), ..FontLibrary::new(root) }
    }

    /// Every face of the TrueType/OpenType fonts installed: those added to a content root's
    /// `Fonts` (the launcher's "Add font…") first, then the system's (see
    /// `omsi_content::dotfont::system_font_dirs`).
    pub fn vector_faces(&mut self) -> &[VectorFace] {
        if self.faces.is_none() {
            let mut dirs: Vec<PathBuf> = omsi_cfg::content_roots().into_iter().map(|r| r.join("Fonts")).collect();
            if dirs.is_empty() {
                dirs.push(self.root.join("Fonts"));
            }
            dirs.extend(omsi_content::dotfont::system_font_dirs());
            self.faces = Some(dirs.iter().flat_map(|d| omsi_content::dotfont::vector_font_files(d)).flat_map(|f| omsi_content::dotfont::vector_faces(&f)).collect());
        }
        self.faces.as_deref().unwrap_or(&[])
    }

    /// The display font `spec` as a display whose own font is `own` draws it on a line of
    /// `line_h` pixels (see [`substitute`]); None when the font is not installed.
    pub fn display_font(&mut self, spec: &DisplayFontSpec, own: Option<&FontAtlas>, line_h: i32, decode: &dyn Fn(&Path) -> Option<(u32, u32, Vec<u8>)>) -> Option<Arc<FontAtlas>> {
        let line_h = line_h.max(1) as u32;
        // (the display's own font by its name and file: two fonts of one height and grid are
        // two substitutes, each as tall and as wide as the letters it stands in for)
        let own_key = own.map(|a| format!("{}|{}", a.font.name.trim(), a.font.path.to_string_lossy().replace('\\', "/")).to_lowercase()).unwrap_or_default();
        let key = (spec.key(), own_key, line_h);
        if let Some(a) = self.drawn.get(&key) {
            return a.clone();
        }
        let target = Target::of(own, line_h, spec);
        let made = if spec.is_vector() {
            self.vector_file(spec).and_then(|f| substitute(SubstituteSource::Vector(&f.0, f.1), spec, &target, own))
        } else {
            self.oft_size(&spec.name, &target.band, decode).and_then(|d| substitute(SubstituteSource::Oft(&d.0), spec, &target, own))
        };
        match (&made, own) {
            (None, _) => log::warn!("display font \"{}\" is not installed", spec.name),
            (Some(s), Some(o)) if !s.band.fill => log::info!(
                "display font: \"{}\" (capitals {} rows from row {}, {:.1} dots wide) drawn in \"{}\" at {:.0}% width",
                o.font.name.trim(),
                s.band.cap_rows(),
                s.band.cap_top,
                target.width,
                spec.name,
                s.condense * 100.0
            ),
            (Some(_), Some(o)) => log::info!("display font: \"{}\" (letters of no band to read) drawn in \"{}\" over its {} rows", o.font.name.trim(), spec.name, target.grid.rows),
            (Some(_), None) => log::info!("display font: a display of no font of its own drawn in \"{}\" over {line_h} px", spec.name),
        }
        let atlas = made.map(|s| Arc::new(s.atlas));
        self.drawn.insert(key, atlas.clone());
        atlas
    }

    /// `display_font` with the registered picture decoder (`crate::image_decode`).
    pub fn display_font_for(&mut self, spec: &DisplayFontSpec, own: Option<&FontAtlas>, line_h: i32) -> Option<Arc<FontAtlas>> {
        self.display_font(spec, own, line_h, &|p| crate::image_decode::decode(p).ok().map(|i| (i.width, i.height, i.rgba)))
    }

    /// A vector display font's file and face (`vector_data`), read once.
    fn vector_file(&mut self, spec: &DisplayFontSpec) -> Option<Arc<(Vec<u8>, u32)>> {
        let key = spec.plain().key();
        if let Some(f) = self.vector_files.get(&key) {
            return f.clone();
        }
        let f = self.vector_data(spec).map(Arc::new);
        self.vector_files.insert(key, f.clone());
        f
    }

    /// The size of the `.oft` display font `chosen`'s file that stands in for letters of
    /// `band` (`pick_by_band`: the chosen one where its capitals are no taller, else the
    /// tallest that fits, of those as tall the one as wide), with its dots and where its letters
    /// sit. None when no font of that name is installed.
    pub fn oft_size(&mut self, chosen: &str, band: &LetterBand, decode: &dyn Fn(&Path) -> Option<(u32, u32, Vec<u8>)>) -> Option<Arc<(DotFont, LetterBand)>> {
        let wanted = chosen.trim();
        if wanted.is_empty() {
            return None;
        }
        let index = self.index();
        let first = index.iter().position(|f| f.name.trim().eq_ignore_ascii_case(wanted))?;
        let file = index[first].path.clone();
        let family: Vec<String> = index.iter().filter(|f| f.path == file).map(|f| f.name.clone()).collect();
        let me = family.iter().position(|n| n.trim().eq_ignore_ascii_case(wanted)).unwrap_or(0);
        let sizes: Vec<Option<Arc<(DotFont, LetterBand)>>> = family.iter().map(|n| self.oft_dots_of(n, decode)).collect();
        let bands: Vec<Option<&LetterBand>> = sizes.iter().map(|s| s.as_deref().map(|s| &s.1)).collect();
        let k = pick_by_band(&bands, me, band);
        sizes[k].clone().or_else(|| sizes[me].clone()).or_else(|| sizes.iter().flatten().next().cloned())
    }

    /// An `.oft` font's dots and where its letters sit, read once.
    fn oft_dots_of(&mut self, name: &str, decode: &dyn Fn(&Path) -> Option<(u32, u32, Vec<u8>)>) -> Option<Arc<(DotFont, LetterBand)>> {
        let key = name.trim().to_lowercase();
        if let Some(d) = self.oft_dots.get(&key) {
            return d.clone();
        }
        let d = self.get(name, decode).map(|a| {
            let dots = oft_dots(&a);
            let band = LetterBand::of(&dots);
            Arc::new((dots, band))
        });
        self.oft_dots.insert(key, d.clone());
        d
    }

    /// The bytes and the face of a vector display font: its file, else (moved, or chosen on
    /// another computer) the installed face of the same name.
    fn vector_data(&mut self, spec: &DisplayFontSpec) -> Option<(Vec<u8>, u32)> {
        if let Some(d) = spec.file.as_deref().and_then(|f| std::fs::read(f).ok().or_else(|| omsi_cfg::vfs::read(f).ok())) {
            return Some((d, spec.face));
        }
        let wanted = spec.name.trim().to_lowercase();
        let face = self.vector_faces().iter().find(|f| f.name.trim().to_lowercase() == wanted)?.clone();
        log::info!("display font \"{}\": not at {:?}, taken from {}", spec.name, spec.file, face.path.display());
        let d = std::fs::read(&face.path).ok().or_else(|| omsi_cfg::vfs::read(&face.path).ok())?;
        Some((d, face.face))
    }

    fn index(&mut self) -> &[Font] {
        if self.index.is_none() {
            // the Fonts folder of every content root (installed mods and archives first, then
            // the installation): a mod's display fonts live in the content folder, and looking
            // only in the installation's `Fonts` left the O530's number plate, matrix and
            // dashboard displays empty. A file of the same name higher up replaces the stock one.
            let mut files = omsi_cfg::read_dir_merged("Fonts");
            if files.is_empty() {
                files = omsi_cfg::vfs::read_dir_paths(&self.root.join("Fonts"));
            }
            // each folder in root order, its files in name order (a listing comes in the file
            // system's order)
            let mut folders: Vec<std::path::PathBuf> = Vec::new();
            for p in &files {
                let d = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
                if !folders.contains(&d) {
                    folders.push(d);
                }
            }
            files.sort_by_cached_key(|p| (folders.iter().position(|d| Some(d.as_path()) == p.parent()).unwrap_or(usize::MAX), p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()));
            // Two files of one folder naming the same font: the one read later takes its place,
            // as in the original (the LiAZ's `ANX_S.oft` spells its "ц" as "=", and a
            // "ANX_S - копия.oft" lying beside it, read first, left the letter out). A
            // folder of higher priority keeps its fonts over the folders after it.
            let mut all: Vec<Font> = Vec::new();
            let mut folder_of: Vec<usize> = Vec::new();
            for p in files {
                if !p.extension().map(|x| x.eq_ignore_ascii_case("oft")).unwrap_or(false) {
                    continue;
                }
                let folder = folders.iter().position(|d| Some(d.as_path()) == p.parent()).unwrap_or(usize::MAX);
                // `Font::path` is the .oft (read through the VFS): its bitmaps lie next to it
                if let Ok(list) = Font::load_all(&p) {
                    for f in list {
                        let key = f.name.trim().to_ascii_lowercase();
                        match all.iter().position(|o| o.name.trim().to_ascii_lowercase() == key) {
                            Some(k) if folder_of[k] == folder => all[k] = f,
                            Some(_) => {}
                            None => {
                                all.push(f);
                                folder_of.push(folder);
                            }
                        }
                    }
                }
            }
            self.index = Some(all);
        }
        self.index.as_deref().unwrap_or(&[])
    }

    /// Whether a font of exactly this name is installed.
    pub fn has_exact(&mut self, name: &str) -> bool {
        let wanted = name.trim();
        self.index().iter().any(|f| f.name.trim().eq_ignore_ascii_case(wanted))
    }

    /// The font called `name`, or - when there is none - one of the same family and size in
    /// another weight: mods ask for weights their packs never shipped (the Citaro pack's
    /// Krüger matrix wants "churafont++ Numeric 26x11 Bold" and "churafont++ 32x8 Bold";
    /// the fonts are "churafont++ Numeric 26x11" and "churafont++ 32x8"), and without it
    /// the line number stayed off the destination display. A name of no known family (the
    /// depot strings the matrix tries as custom fonts) is still not found.
    fn find(&mut self, name: &str) -> Option<Font> {
        let wanted = name.trim();
        if wanted.is_empty() {
            return None;
        }
        let index = self.index();
        if let Some(f) = index.iter().find(|f| f.name.trim().eq_ignore_ascii_case(wanted)) {
            return Some(f.clone());
        }
        let family = family_of(wanted);
        // the plain weight first, then the others in file order - but only a font of the same
        // size. A dot-matrix display (the Krüger matrix asks for eight sizes by name, from
        // "Krueger 7x4" to "Krueger 16x9") draws its letters cell by cell: substituting
        // another size there does not make the text wider or narrower, it makes it a soup of
        // letter fragments. Without a font of that size the script must hear "no font" (-1)
        // and pick the next size itself, which is what the original does.
        let sibling = index
            .iter()
            .filter(|f| size_of(&f.name) == size_of(wanted))
            .find(|f| f.name.trim().eq_ignore_ascii_case(&family))
            .or_else(|| index.iter().filter(|f| size_of(&f.name) == size_of(wanted)).find(|f| family_of(&f.name) == family));
        let Some(sibling) = sibling else {
            // a display whose font is missing is worth a line in the log: it is the first
            // thing to look at when letters come out wrong on a matrix or a plate
            log::warn!("font \"{wanted}\" is in no Fonts folder of any content root");
            return None;
        };
        log::warn!("font \"{wanted}\" not found; drawing with \"{}\" (same family and size)", sibling.name.trim());
        Some(sibling.clone())
    }

    /// Every font installed - each `[newfont]` of every content root's `Fonts/*.oft` once, in
    /// lookup order (the launcher's list of display fonts).
    pub fn installed(&mut self) -> Vec<Font> {
        self.index().to_vec()
    }

    /// `get` with the registered picture decoder (`crate::image_decode`).
    pub fn load(&mut self, name: &str) -> Option<Arc<FontAtlas>> {
        self.get(name, &|p| crate::image_decode::decode(p).ok().map(|i| (i.width, i.height, i.rgba)))
    }

    /// Fonts are looked up by their `[newfont]` name across all `Fonts/*.oft` files of every
    /// content root (installed mods first, then the installation): a mod's display fonts
    /// live in the content folder, and looking only in the installation's `Fonts` left the
    /// O530's number plate, matrix and dashboard displays empty.
    pub fn get(&mut self, name: &str, decode: &dyn Fn(&Path) -> Option<(u32, u32, Vec<u8>)>) -> Option<Arc<FontAtlas>> {
        let key = name.to_ascii_lowercase();
        if let Some(a) = self.atlases.get(&key) {
            return a.clone();
        }
        let found = self.find(name);
        let fonts_dir = self.root.join("Fonts");
        let atlas = found.and_then(|f| atlas_of(f, &fonts_dir, decode)).map(Arc::new);
        if atlas.is_none() && !name.trim().is_empty() {
            log::warn!("font \"{name}\" not found");
        }
        self.atlases.insert(key, atlas.clone());
        atlas
    }
}

/// A font's colour bitmap laid over its alpha bitmap pixel for pixel. Omsi.exe reads a
/// glyph's colour at the very pixel it reads its coverage at, in the colour bitmap's own
/// rows (0x5d67bc: the same scanline row and byte column in both), whatever size either
/// has - a colour bitmap need not be the alpha's size, and often is a small swatch of the
/// one colour (the stock `EFADfont.bmp` is 128 x 128 under a 128 x 200 alpha). Past its
/// edges, where Omsi.exe reads into other rows or fails, the swatch repeats. (Taken as no
/// colour bitmap at all, a full-colour text came out in the alpha's white, #829.)
fn color_at_alpha_pixels(color: &[u8], cw: u32, ch: u32, aw: u32, ah: u32) -> Vec<u8> {
    let mut out = vec![0u8; (aw * ah * 4) as usize];
    for y in 0..ah {
        for x in 0..aw {
            let s = (((y % ch) * cw + x % cw) * 4) as usize;
            let d = ((y * aw + x) * 4) as usize;
            out[d..d + 4].copy_from_slice(&color[s..s + 4]);
        }
    }
    out
}

/// `font` with its bitmaps, read with `decode` - not kept anywhere (the launcher's previews of
/// every installed font draw each once). The bitmaps sit beside the `.oft` that names them
/// (`fonts_dir` for a font read from nowhere).
pub fn atlas_of(font: Font, fonts_dir: &Path, decode: &dyn Fn(&Path) -> Option<(u32, u32, Vec<u8>)>) -> Option<FontAtlas> {
    let dir = font.path.parent().map(Path::to_path_buf).unwrap_or_else(|| fonts_dir.to_path_buf());
    let alpha_path = omsi_cfg::resolve_path(&dir, &font.alpha);
    let color_path = omsi_cfg::resolve_path(&dir, &font.bitmap);
    let (aw, ah, alpha) = decode(&alpha_path)?;
    // (a colour bitmap of another size is laid over the alpha pixel for pixel, as Omsi.exe
    // reads it - see `color_at_alpha_pixels`)
    let color = match (color_path != alpha_path).then(|| decode(&color_path)).flatten() {
        Some((cw, ch, c)) if (cw, ch) == (aw, ah) => c,
        Some((cw, ch, c)) if cw > 0 && ch > 0 && c.len() == (cw * ch * 4) as usize => color_at_alpha_pixels(&c, cw, ch, aw, ah),
        _ => alpha.clone(),
    };
    Some(FontAtlas::new(font, aw, ah, color, alpha))
}

/// Which size of a display font fits a line `line_h` high: `heights` are the sizes of one font
/// file, `chosen` the one the player picked. The chosen one when it is no taller than the line
/// (it is magnified by a whole number to fill it); else the tallest that is; none fitting, the
/// smallest.
pub fn pick_size(heights: &[i32], chosen: usize, line_h: i32) -> usize {
    let Some(&own) = heights.get(chosen) else { return 0 };
    if own > 0 && own <= line_h.max(1) {
        return chosen;
    }
    let fitting = heights.iter().enumerate().filter(|(_, &h)| h > 0 && h <= line_h).max_by_key(|(i, &h)| (h, std::cmp::Reverse(*i)));
    match fitting {
        Some((i, _)) => i,
        None => heights.iter().enumerate().filter(|(_, &h)| h > 0).min_by_key(|(i, &h)| (h, *i)).map(|(i, _)| i).unwrap_or(chosen),
    }
}

/// Which size of an `.oft` display font's file stands in for letters of `band` (see
/// `FontLibrary::oft_size`): `sizes` are where the letters of each size of the file sit (None:
/// one that does not load), `chosen` the one the player picked. By the capitals' height
/// (`pick_size`), not the cell's: the chosen one when its capitals are no taller than those
/// (the Krüger pack's "Krueger 16x9" chosen for a 7-row line number becomes one of its 7-row
/// sizes, not 16x9 squeezed into a smear); else the tallest that fits - and of the sizes as
/// tall, the one whose letters are nearest the width of those it stands in for.
pub fn pick_by_band(sizes: &[Option<&LetterBand>], chosen: usize, band: &LetterBand) -> usize {
    let caps: Vec<i32> = sizes.iter().map(|s| s.map(|b| b.cap_rows() as i32).unwrap_or(0)).collect();
    let k = pick_size(&caps, chosen, band.cap_rows() as i32);
    if k == chosen && caps.get(k).is_some_and(|c| *c > 0) {
        return k;
    }
    // (the width a size's letters come out at: magnified as many whole times as fit, or shrunk)
    let width = |b: &LetterBand| {
        let r = band.cap_rows() as f32 / b.cap_rows() as f32;
        b.cap_w * if r >= 2.0 { r.floor() } else if r >= 1.0 { 1.0 } else { r }
    };
    (0..sizes.len()).filter(|&i| caps[i] > 0 && caps[i] == caps.get(k).copied().unwrap_or(0)).min_by(|&a, &b| {
        let (wa, wb) = (sizes[a].map(width).unwrap_or(0.0), sizes[b].map(width).unwrap_or(0.0));
        (wa - band.cap_w).abs().total_cmp(&(wb - band.cap_w).abs()).then(a.cmp(&b))
    }).unwrap_or(k)
}

/// An `.oft` display font's dots: its letters read in its own grid.
pub fn oft_dots(atlas: &FontAtlas) -> DotFont {
    DotFont::from_atlas(atlas, &DotGrid::of_atlas(atlas))
}

/// What a display font is fitted to on one display (see `substitute`): the grid of dots of
/// the display's own font, where that font's letters sit and how wide they are (made the
/// letter height the player chose), the line, the characters the substitute is made for and
/// those it leaves as the display's own font draws them, and the letters its width is
/// measured on.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub grid: DotGrid,
    pub band: LetterBand,
    pub line_h: u32,
    /// The letters, digits and marks of text the display's own font draws
    /// (`dotfont::text_chars_of`), but its pictograms; every one of `charset` on a display of
    /// no font of its own.
    pub chars: Vec<char>,
    /// The display's own font's pictograms that are coded as letters or marks of text
    /// (`dotfont::pictograms_of`): they stay as it draws them.
    pub kept: Vec<char>,
    /// The capitals the display's own font has (its digits, for a line number's font), and
    /// their mean width there (dots): what a substitute is made about as wide as.
    pub refs: Vec<char>,
    pub width: f32,
}

impl Target {
    /// The display whose own font is `own` (None: a display of no font), a line of `line_h`
    /// pixels, with the settings of `spec` (its letter height): `read` and `sized`.
    pub fn of(own: Option<&FontAtlas>, line_h: u32, spec: &DisplayFontSpec) -> Target {
        Target::read(own, line_h).sized(spec.rows)
    }

    /// The display whose own font is `own` (None: a display of no font) on a line of `line_h`
    /// pixels, its letters as tall as that font has them: the font read as dots once (its grid
    /// and its letters, which takes a while on a large picture of LEDs), for every display font
    /// then fitted to it (`sized`).
    pub fn read(own: Option<&FontAtlas>, line_h: u32) -> Target {
        let line_h = line_h.max(1);
        let grid = own.map(DotGrid::of_atlas).unwrap_or_else(|| DotGrid::plain(line_h));
        let own_dots = own.map(|a| DotFont::from_atlas(a, &grid));
        let band = own_dots.as_ref().map(LetterBand::of).unwrap_or_else(|| LetterBand::filling(grid.rows));
        let kept = own_dots.as_ref().map(|d| omsi_content::dotfont::pictograms_of(d, &band)).unwrap_or_default();
        let mut chars = own.map(|a| omsi_content::dotfont::text_chars_of(&a.font)).unwrap_or_else(omsi_content::dotfont::charset);
        chars.retain(|c| !kept.contains(c));
        let has = |c: &char| own_dots.as_ref().is_some_and(|d| d.glyphs.iter().any(|g| g.ch == *c));
        let capitals: Vec<char> = ('A'..='Z').filter(has).collect();
        let digits: Vec<char> = ('0'..='9').filter(has).collect();
        // (a line number's font is measured on its digits)
        let refs = if band.fill {
            Vec::new()
        } else if capitals.len() < 13 && !digits.is_empty() {
            digits
        } else {
            capitals
        };
        let mut t = Target { grid, band, line_h, chars, kept, refs, width: 0.0 };
        t.width = t.ref_width();
        t
    }

    /// The target `read` gave with its letters `rows` rows high, capitals and descenders (the
    /// player's letter height; None: as the display's own font has them). Of a target as
    /// `read` gave it: a sized one is sized again from its new height.
    pub fn sized(&self, rows: Option<u32>) -> Target {
        let Some(r) = rows else { return self.clone() };
        let mut t = Target { band: self.band.scaled_to(r), ..self.clone() };
        t.width = t.ref_width();
        t
    }

    /// The mean width of the letters `refs` in the band (dots): its digits' for a line
    /// number's font, else its capitals'; none on a display whose letters fill its rows.
    fn ref_width(&self) -> f32 {
        if self.band.fill {
            0.0
        } else if !self.refs.is_empty() && self.refs.iter().all(char::is_ascii_digit) {
            self.band.digit_w.unwrap_or(self.band.cap_w)
        } else {
            self.band.cap_w
        }
    }
}

/// The letters a display font stands in with (see `substitute`): a vector font's file and face,
/// or an `.oft` font's dots (the size `FontLibrary::oft_size` picks).
#[derive(Clone, Copy)]
pub enum SubstituteSource<'a> {
    Vector(&'a [u8], u32),
    Oft(&'a DotFont),
}

/// A display font made for a display (`substitute`): the atlas, where its letters sit, and how
/// much narrower than its own they are drawn.
pub struct Substitute {
    pub atlas: FontAtlas,
    pub band: LetterBand,
    pub condense: f32,
}

/// The display font `src` (with the settings of `spec`: its letter height, bold, spacing) as
/// the display of `target`, whose own font is `own`, shows it - what the game draws with (a
/// text texture, a script's matrix) and the launcher's previews show. In the display's grid of
/// dots and line, its capitals as tall as those of the display's own font and standing on its
/// line, its letters about as wide (narrowed by `condense_for`, measured on a first drawing of
/// the letters it is compared on) and the gap and space of that font: a script that measures
/// its text to pick a size picks the one it picks with its own fonts, and a destination padded
/// with spaces lies where it lies in them. What is no text keeps the display's own drawing
/// (`dotfont::merge_kept`). None when a vector font has none of the letters.
pub fn substitute(src: SubstituteSource, spec: &DisplayFontSpec, target: &Target, own: Option<&FontAtlas>) -> Option<Substitute> {
    let band = &target.band;
    let make = |condense: f32, chars: &[char]| -> Result<DotFont, String> {
        match src {
            SubstituteSource::Vector(data, face) => DotFont::from_vector_in(data, face, band, chars, condense),
            SubstituteSource::Oft(dots) => Ok(dots.only(chars).fit_to(band, condense)),
        }
    };
    let mut condense = 1.0;
    if target.width > 0.0 && !target.refs.is_empty() {
        let measure = |c: f32| make(c, &target.refs).ok().and_then(|d| d.mean_width(&target.refs));
        if let Some(natural) = measure(1.0) {
            condense = omsi_content::dotfont::condense_for(natural, target.width);
            // (each letter comes out a whole number of dots wide, which a small size rounds
            // up by much of a dot: narrowed once more by what is left over)
            if let Some(w) = (condense < 1.0).then(|| measure(condense)).flatten().filter(|w| *w > target.width * 1.03) {
                condense = (condense * target.width / w).clamp(0.5, 1.0);
            }
        }
    }
    let dots = match make(condense, &target.chars) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("display font \"{}\": {e}", spec.name);
            return None;
        }
    };
    let atlas = dots.styled(spec.bold, spec.spacing).to_atlas(&target.grid, target.line_h, spec.name.trim());
    let atlas = match own {
        Some(o) => omsi_content::dotfont::merge_kept(&atlas, o, &target.kept),
        None => atlas,
    };
    Some(Substitute { atlas, band: band.clone(), condense })
}

/// `substitute`'s atlas for a display whose own font is `own` (None: one of no font) on a
/// line of `line_h` pixels.
pub fn substitute_atlas(src: SubstituteSource, spec: &DisplayFontSpec, own: Option<&FontAtlas>, line_h: u32) -> Option<FontAtlas> {
    substitute(src, spec, &Target::of(own, line_h, spec), own).map(|s| s.atlas)
}

/// Whether `font` writes letters or a line number - not pictograms, a clock, or a script's
/// helper (the Krüger matrix measures hex digits with the widths of its "Auxiliary" fonts):
/// a display font takes its place on a matrix that a script draws.
pub fn is_letter_font(font: &Font) -> bool {
    let name = format!("{} ", font.name.to_lowercase());
    if KEPT_FONT_PARTS.iter().any(|p| name.contains(p)) {
        return false;
    }
    let letters = ('A'..='Z').filter(|&c| font.glyph(c).is_some_and(|g| g.x1 > g.x0)).count();
    let digits = ('0'..='9').filter(|&c| font.glyph(c).is_some_and(|g| g.x1 > g.x0)).count();
    letters >= 13 || digits == 10
}

/// Parts of a font's name a matrix script keeps its own of (see `is_letter_font`).
const KEPT_FONT_PARTS: &[&str] = &["pictogram", "piktogramm", "icon", "symbol", "auxiliary", "aux ", "chrono", "clock", "uhr", "logo", "arrow", "pfeil"];

/// Parts of a script file's name that make it the one drawing the bus's destination signs.
const SIGN_SCRIPT_PARTS: &[&str] = &["matrix", "matr", "ziel", "annax", "krueger", "krüger", "lawo", "mobitec", "aesys", "gorba", "buse", "brose", "flipdot", "rollband", "destination", "kierunk"];

/// ... and those that make it a device of the cab or the saloon, whatever else it says.
const DEVICE_SCRIPT_PARTS: &[&str] = &["ibis", "cockpit", "dash", "drucker", "printer", "ticket", "rbl", "afr", "innen", "interior", "almex", "cashdesk", "kasse", "tacho", "radio"];

/// A script of the vehicle that draws a destination sign into its script textures with fonts
/// it asks the game for by name (`GetFontIndex` then `STTextOut`): the Krüger matrices and
/// their kin. A display font chosen for the bus takes the place of those fonts there, each
/// fitted to the height, the line and the width of the letters of the one it replaces.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScriptSign {
    pub script: PathBuf,
    /// The fonts it asks for that may write letters or the line number (whether a font does
    /// is known once it is loaded: `is_letter_font`).
    pub fonts: Vec<String>,
    /// The fonts it keeps its own: pictograms, clocks, its helpers.
    pub kept: Vec<String>,
    /// It also loads whole pictures of destinations (`STLoadTex`; a depot file's terminus
    /// with a bitmap): those stay pictures.
    pub pictures: bool,
}

/// The scripts among `scripts` (a vehicle's, see `omsi_vehicle::Vehicle::scripts`) that draw
/// destination signs with fonts (see `ScriptSign`), read from their code: the font names
/// handed to `GetFontIndex` as they are written in it.
pub fn script_signs(scripts: &[PathBuf]) -> Vec<ScriptSign> {
    let mut out = Vec::new();
    for path in scripts {
        let name = path.to_string_lossy().replace('\\', "/").to_lowercase();
        // (the file and the folder it is in: `churaKrueger/VMatrix.osc`)
        let tail: String = name.rsplit('/').take(2).collect::<Vec<_>>().join("/");
        if !SIGN_SCRIPT_PARTS.iter().any(|p| tail.contains(p)) || DEVICE_SCRIPT_PARTS.iter().any(|p| tail.contains(p)) {
            continue;
        }
        let program = omsi_script::compile(&omsi_script::CompileInput { scripts: vec![path.clone()], ..Default::default() });
        let used = program.callbacks_used();
        let calls = |c: &str| used.iter().any(|u| u.eq_ignore_ascii_case(c));
        if !calls("STTextOut") || !calls("GetFontIndex") {
            continue;
        }
        let (mut fonts, mut kept) = (Vec::new(), Vec::new());
        for f in program.literal_arguments("getfontindex") {
            let f = f.trim().to_string();
            if f.is_empty() || fonts.iter().chain(kept.iter()).any(|o: &String| o.eq_ignore_ascii_case(&f)) {
                continue;
            }
            let lower = format!("{} ", f.to_lowercase());
            if KEPT_FONT_PARTS.iter().any(|p| lower.contains(p)) {
                kept.push(f);
            } else {
                fonts.push(f);
            }
        }
        if fonts.is_empty() {
            continue;
        }
        out.push(ScriptSign { script: path.clone(), fonts, kept, pictures: calls("STLoadTex") });
    }
    out
}

/// Parts of a name that make a text texture one of the bus's destination displays: the
/// display itself (matrix, Ziel, terminus, line - the SD200's `Matrix_Terminus`, the Hamburg
/// buses' `LW_show_linie`) or a maker of destination signs whose name its font carries
/// (Annax, Lawo, Krüger, Mobitec - the Hamburg buses' `4_LW_Benefit_*`).
const DISPLAY_PARTS: &[&str] = &[
    "matrix", "ziel", "terminus", "linie", "destination", "kierunek", "annax", "lawo", "benefit", "krueger", "krüger", "mobitec", "aesys", "gorba", "hanover", "brose", "buse", "streetberlin", "flipdot", "rollband",
];

/// Whole words of a name that say the same (`17_LED_Klein`, a `front` or `heck` sign).
const DISPLAY_WORDS: &[&str] = &["led", "lw", "front", "side", "seite", "heck", "rear", "back", "line", "route", "dest"];

/// Words of displays that a display font may be meant for only on a mesh that is seen from
/// outside alone.
const WEAK_WORDS: &[&str] = &["lcd", "display", "anzeige", "sign"];

/// Parts of a name that make a text texture something else, whatever else it says: the cab's
/// and the saloon's devices (IBIS, ticket printers, the ALMEX, the dashboard, the passenger
/// information inside - the O530's `interior_display_monitor_kierunek`), plates and fleet
/// numbers, paper.
const NOT_DISPLAY: &[&str] = &[
    "ibis", "drucker", "ticket", "printer", "almex", "kasownik", "faremaster", "validator", "cockpit", "dash", "tacho", "odo", "innen", "interior", "inside", "monitor", "infotainment", "setvar", "radio", "kennz", "plate", "wagennummer", "vehno", "zettel", "handschrift", "schedule", "fahrplan", "menu", "tablet", "rg6000", "segment", "clock", "uhr",
];

/// Whole words of a name that say the same: `LK_Linie` and `LK_Route` are the Lion's City's
/// ticket printer's line and route, `CD_LineTerminus` its cab display, `ianz_*` the MAN SL's
/// saloon displays (Innenanzeige), `ZD_*` a dashboard's central display, `Hst*` the stops a
/// printer lists.
const NOT_DISPLAY_WORDS: &[&str] = &["lk", "cd", "zd", "ianz", "hst"];

/// The words of a name: apart at everything that is no letter or digit, in lower case.
fn name_words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect()
}

/// Whether a text texture is one of the destination displays a display font is for, by its
/// variable's and its font's names: the sign's own words, and none of a device's. `outside`:
/// a mesh that shows it is seen from outside (see `seen_from_outside`); `outside_only`:
/// every mesh that shows it is seen only from outside (an LCD there is a sign, not a dashboard).
pub fn is_destination_display(tt: &TextTexture, outside: bool, outside_only: bool) -> bool {
    // a plate's and a fleet number's colours come from their font's bitmap
    if tt.full_color || !outside {
        return false;
    }
    let var = tt.variable.trim().to_lowercase();
    if var == "ident" || var == "number" || var.parse::<usize>().is_ok() {
        return false;
    }
    let names = [var, tt.font.trim().to_lowercase()];
    if names.iter().any(|n| NOT_DISPLAY.iter().any(|p| n.contains(p))) {
        return false;
    }
    let words: Vec<String> = names.iter().flat_map(|n| name_words(n)).collect();
    if words.iter().any(|w| NOT_DISPLAY_WORDS.contains(&w.as_str())) {
        return false;
    }
    if names.iter().any(|n| DISPLAY_PARTS.iter().any(|p| n.contains(p))) || words.iter().any(|w| DISPLAY_WORDS.contains(&w.as_str())) {
        return true;
    }
    outside_only && words.iter().any(|w| WEAK_WORDS.contains(&w.as_str()))
}

/// For every text texture of `model`: whether a mesh showing it is seen from outside
/// (`[viewpoint]` 0, all views, or one with the outside view, bit 1), and whether every mesh
/// showing it is seen only from there. One no mesh shows is seen from nowhere (the Lion's
/// City's `Matrix_Liniennummerstring`: its matrix is a script texture).
pub fn seen_from_outside(model: &omsi_model::Model) -> Vec<(bool, bool)> {
    let mut out: Vec<(bool, bool, bool)> = vec![(false, true, false); model.text_textures.len()];
    for m in &model.meshes {
        let outside = m.viewpoint == 0 || m.viewpoint & 1 != 0;
        let only = m.viewpoint == 1;
        for t in m.materials.iter().filter_map(|t| t.use_text_texture) {
            if let Some(o) = usize::try_from(t).ok().and_then(|t| out.get_mut(t)) {
                o.0 |= outside;
                o.1 &= only;
                o.2 = true;
            }
        }
    }
    out.into_iter().map(|(outside, only, used)| if used { (outside, only) } else { (false, false) }).collect()
}

/// For every text texture of `model`: whether every mesh showing it is one of the cab's or
/// the saloon's devices by its file's name (the Lion's City's `Matrix_Liniennummerstring` is
/// the line number of `Drucker\Innenanzeige\Innenanzeige_Linie.o3d`, the passengers' display
/// inside - its outside matrix is a script's). One no mesh shows is none.
pub fn shown_on_devices_only(model: &omsi_model::Model) -> Vec<bool> {
    let mut out: Vec<(bool, bool)> = vec![(false, true); model.text_textures.len()];
    for m in &model.meshes {
        let file = m.file.to_lowercase();
        let words = name_words(&file);
        let device = NOT_DISPLAY.iter().any(|p| file.contains(p)) || words.iter().any(|w| NOT_DISPLAY_WORDS.contains(&w.as_str()));
        for t in m.materials.iter().filter_map(|t| t.use_text_texture) {
            if let Some(o) = usize::try_from(t).ok().and_then(|t| out.get_mut(t)) {
                o.0 = true;
                o.1 &= device;
            }
        }
    }
    out.into_iter().map(|(used, device)| used && device).collect()
}

/// The text textures of `model` that are its destination displays (see
/// `is_destination_display`), by their place in its `[texttexture]` list.
pub fn destination_displays(model: &omsi_model::Model) -> Vec<usize> {
    let seen = seen_from_outside(model);
    let devices = shown_on_devices_only(model);
    model
        .text_textures
        .iter()
        .enumerate()
        .filter(|(i, t)| seen.get(*i).is_some_and(|&(o, only)| is_destination_display(t, o, only)) && !devices.get(*i).copied().unwrap_or(false))
        .map(|(i, _)| i)
        .collect()
}

/// What a destination display shows, by the words of its variable's and font's names: the
/// line number, a side or rear sign, else the destination (at the front).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DisplayRole {
    Destination,
    LineNumber,
    Side,
    Rear,
}

impl DisplayRole {
    pub fn of(tt: &TextTexture) -> DisplayRole {
        let names = format!("{} {}", tt.variable, tt.font).to_lowercase();
        let words = name_words(&names);
        let has = |ws: &[&str]| words.iter().any(|w| ws.contains(&w.as_str()));
        if has(&["heck", "rear", "back", "hinten", "tyl"]) || names.contains("heck") {
            DisplayRole::Rear
        } else if has(&["seite", "side", "bok"]) || names.contains("seite") {
            DisplayRole::Side
        } else if has(&["nr", "nummer", "linie", "line", "route", "number", "liniennummer"]) || names.contains("linie") || names.contains("liniennummer") {
            DisplayRole::LineNumber
        } else {
            DisplayRole::Destination
        }
    }
}

/// Draw the destination displays among `states` (the text textures of `model`, in its
/// order) in the display font `chosen` the player picked for the bus (a
/// `DisplayFontSpec::to_arg`: an `.oft` font's name, or a vector font with its settings), as
/// each display draws letters (`FontLibrary::display_font`: in the grid of dots of the
/// display's own font, its line, its letters' height and width) - its size, colour and
/// placement stay the bus's. Returns how many it changed (none when the font is missing).
pub fn apply_display_font(states: &mut [TextTextureState], model: &omsi_model::Model, chosen: &str, lib: &mut FontLibrary, decode: &dyn Fn(&Path) -> Option<(u32, u32, Vec<u8>)>) -> usize {
    let Some(spec) = DisplayFontSpec::parse(chosen) else { return 0 };
    let mut changed = 0;
    for i in destination_displays(model) {
        let Some(s) = states.get_mut(i) else { continue };
        // the display's line: its own font's height (a font missing: the texture's)
        let line_h = s.atlas.as_ref().map(|a| a.font.height).filter(|h| *h > 0).unwrap_or(s.def.height).max(1);
        let Some(atlas) = lib.display_font(&spec, s.atlas.as_deref(), line_h, decode) else {
            log::warn!("display font \"{}\" is not installed: the bus's own is kept", spec.name);
            return changed;
        };
        log::info!("display font: {} ({}) drawn in \"{}\" for a line of {line_h} px", s.def.variable, s.def.font, spec.name);
        // (made in the line, band and spacing of the display's own font, it is placed as
        // Omsi.exe places that font - '@' lines and clipping too; only on a display of no font
        // of its own is it fitted to the texture's line)
        s.fit = if s.atlas.is_some() { None } else { Some(line_h as u32) };
        s.atlas = Some(atlas);
        s.last_text = None;
        changed += 1;
    }
    changed
}

/// Runtime state of one `[texttexture]`.
#[derive(Clone)]
pub struct TextTextureState {
    pub def: TextTexture,
    pub atlas: Option<Arc<FontAtlas>>,
    pub last_text: Option<String>,
    /// Latest rendered RGBA image, present when it changed since the last upload.
    pub pending: Option<Vec<u8>>,
    /// Drawn in a display font the player chose on a display whose own font is missing: the
    /// height of the texture's line, which the chosen one is fitted to (`render_fitted`).
    pub fit: Option<u32>,
}

impl TextTextureState {
    pub fn new(def: TextTexture, atlas: Option<Arc<FontAtlas>>) -> Self {
        Self { def, atlas, last_text: None, pending: None, fit: None }
    }

    /// Re-render when the string variable changed. Returns true when a new image is pending.
    pub fn update(&mut self, text: &str) -> bool {
        if self.last_text.as_deref() == Some(text) {
            return false;
        }
        self.last_text = Some(text.to_string());
        self.pending = Some(self.image(text));
        true
    }

    /// The picture of `text` in this texture's font, size, colour and placement.
    pub fn image(&self, text: &str) -> Vec<u8> {
        let (w, h) = (self.def.width.max(1) as u32, self.def.height.max(1) as u32);
        let rgb = [self.def.color[0] as u8, self.def.color[1] as u8, self.def.color[2] as u8];
        let align = omsi_content::font::TextAlign { orientation: self.def.orientation, grid: self.def.grid };
        match (&self.atlas, self.fit) {
            (Some(a), Some(line_h)) => a.render_fitted(text, w, h, self.def.full_color, rgb, align, line_h),
            (Some(a), None) => a.render_aligned(text, w, h, self.def.full_color, rgb, align),
            (None, _) => vec![0u8; (w * h * 4) as usize],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full-colour font whose colour bitmap is not the alpha's size takes each glyph
    /// pixel's colour from the same pixel of the colour bitmap, as Omsi.exe reads it, not
    /// the alpha's white (#829).
    #[test]
    fn a_colour_bitmap_of_another_size_still_colours_the_glyphs() {
        let dir = std::path::PathBuf::from("/fonts");
        let font = Font { path: dir.join("colour.oft"), name: "Colour".into(), bitmap: "colour.bmp".into(), alpha: "colour_alpha.bmp".into(), height: 4, gap: 0, chars: Vec::new() };
        let mut lib = FontLibrary::new(&dir);
        lib.index = Some(vec![font]);
        // a 4 x 4 white alpha under a 2 x 2 swatch: red, green / blue, yellow
        let swatch = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 0, 255]];
        let decode = |p: &Path| -> Option<(u32, u32, Vec<u8>)> {
            match p.file_name()?.to_str()? {
                "colour_alpha.bmp" => Some((4, 4, vec![255; 64])),
                "colour.bmp" => Some((2, 2, swatch.concat())),
                _ => None,
            }
        };
        let atlas = lib.get("Colour", &decode).expect("the font loads");
        assert_eq!((atlas.width, atlas.height), (4, 4));
        for (x, y) in [(0usize, 0usize), (1, 0), (0, 1), (1, 1), (3, 2)] {
            let i = (y * 4 + x) * 4;
            assert_eq!(atlas.color[i..i + 4], swatch[(y % 2) * 2 + x % 2], "pixel ({x}, {y})");
        }
    }

    #[test]
    fn font_families() {
        assert_eq!(family_of("churafont++ Numeric 26x11 Bold"), "churafont++ numeric 26x11");
        assert_eq!(family_of("churafont++ 32x10 Heavy"), "churafont++ 32x10");
        assert_eq!(family_of("churafont++ 14x10"), "churafont++ 14x10");
        assert_eq!(family_of("Bold"), "bold");
        assert_eq!(family_of("LEERFELD"), "leerfeld");
    }

    fn tt(variable: &str, font: &str) -> TextTexture {
        TextTexture { variable: variable.into(), font: font.into(), width: 512, height: 128, full_color: false, color: [255.0, 160.0, 0.0], orientation: 0, grid: 1 }
    }

    /// The installed buses' text textures (OMSI 2's `Vehicles`): their destination displays,
    /// and the cab's, the saloon's and the plates' that a display font must leave alone.
    #[test]
    fn destination_displays_by_their_names() {
        let sign = |v: &str, f: &str| is_destination_display(&tt(v, f), true, false);
        // MAN SD200, Kajosoft O530, Hamburg electric bus and city bus 2017
        assert!(sign("Matrix_Terminus", "Annax Small"));
        assert!(sign("Matrix_Nr", "Annax Large"));
        assert!(sign("Matrix_Nr", "StreetBerlin"));
        assert!(sign("LW_show_linie", "4_LW_Benefit_Linie"));
        assert!(sign("LW_show_zeile1", "4_LW_Benefit_Klein"));
        assert!(sign("LW_show_voll", "17_LED_Gross"));
        assert!(sign("stringoutput_zielG_A", "krueger-font_gross"));
        assert!(sign("Rollband_Dest1", "SG_Rlbnd_Dest"));
        // the cab, the saloon, the ticket machines, the dashboards
        for (v, f) in [
            ("IBIS", "IBIS_5x7"),
            ("ticketprinter_display", "IBIS-2_5x7"),
            ("cockpit_temperatur", "LCD_7-Segment"),
            ("odometer", "NLC_LCD_7-Segment"),
            ("innenanz_hst", "HH4_barlow"),
            ("interior_display_monitor_kierunek", "KJT3D_monitor"),
            ("almex_s_ziel", "HH20_HHAschedule_font"),
            ("Drucker_TerminusString", "DIN Narrow"),
            ("Faremaster_terminus_name", "O560_US_MSFont_Faremaster"),
            ("IBIS_cabindisplay", "LCD-Innenanzeige"),
            ("Innenanzeige_Haltestelle", "nlc_Innenanzeige_8px"),
            ("rg6_przystanki", "KJT3D_rg6000"),
            ("LK_Linie", "DIN Narrow"),
            ("CD_LineTerminus", "DIN Narrow"),
            ("ianz_Brose_LVA", "SG_Brose_LVA"),
            ("number", "21_vehno"),
            ("number", "Annax Large"),
        ] {
            assert!(!sign(v, f), "{v} / {f}");
        }
        // a plate (its colours from the font) is none, nor is a display seen from the cab only
        assert!(!is_destination_display(&tt("Kennzeichen_Ziel", "Kennz_DtAlt"), true, false));
        let mut coloured = tt("Matrix_Terminus", "Annax Small");
        coloured.full_color = true;
        assert!(!is_destination_display(&coloured, true, true));
        assert!(!is_destination_display(&tt("ident", "nlc_Kennzeichen"), true, true));
        assert!(!is_destination_display(&tt("Matrix_Terminus", "Annax Small"), false, false));
        // an LCD or a display of no other name: a sign only where it is seen from outside alone
        assert!(is_destination_display(&tt("Anzeige_aussen", "MyFont"), true, true));
        assert!(!is_destination_display(&tt("Anzeige_aussen", "MyFont"), true, false));
        assert!(is_destination_display(&tt("front_text", "MyFont"), true, false));
        // ("led" a word, not a part of one)
        assert!(!sign("filled_text", "Called"));
    }

    #[test]
    fn displays_seen_from_outside() {
        use omsi_model::{MaterialDef, MeshDef, Model};
        let mesh = |vp: i32, tex: &[i32]| MeshDef { viewpoint: vp, materials: tex.iter().map(|t| MaterialDef { use_text_texture: Some(*t), ..Default::default() }).collect(), ..Default::default() };
        let model = Model {
            text_textures: vec![tt("Matrix_Terminus", "Annax Small"), tt("Matrix_Nr", "Annax Large"), tt("Anzeige", "X"), tt("Matrix_script", "Y"), tt("Anzeige_heck", "Z")],
            // 0: all views and the cab; 1: the cab alone; 2: outside alone; 3: none; 4: outside + passengers
            meshes: vec![mesh(0, &[0]), mesh(2, &[0, 1]), mesh(1, &[2]), mesh(5, &[4])],
            ..Default::default()
        };
        assert_eq!(seen_from_outside(&model), vec![(true, false), (false, false), (true, true), (false, false), (true, false)]);
        assert_eq!(destination_displays(&model), vec![0, 2, 4]);
    }

    #[test]
    fn a_display_font_size_that_fits_the_line() {
        // (Krueger 7x4, 9x5, 16x9 of one file)
        let h = [7, 9, 16];
        assert_eq!(pick_size(&h, 0, 16), 0, "smaller: kept, magnified twice");
        assert_eq!(pick_size(&h, 2, 16), 2);
        assert_eq!(pick_size(&h, 2, 12), 1, "taller than the line: the tallest that fits");
        assert_eq!(pick_size(&h, 2, 8), 0);
        assert_eq!(pick_size(&h, 2, 5), 0, "none fits: the smallest, shrunk");
        assert_eq!(pick_size(&[32], 0, 16), 0);
        assert_eq!(pick_size(&[], 0, 16), 0);
    }

    /// The installed OMSI 2's buses (`OMSI_ROOT`; skipped without it): the destination
    /// displays each model has (printed), the SD200's matrix among them and none of a cab's
    /// IBIS or a plate.
    #[test]
    fn installed_buses_destination_displays() {
        let Some(root) = omsi_cfg::env::var_os("OMSI_ROOT").map(std::path::PathBuf::from) else {
            eprintln!("skipped: no OMSI_ROOT");
            return;
        };
        let mut found: Vec<(String, String)> = Vec::new();
        for bus in std::fs::read_dir(root.join("Vehicles")).into_iter().flatten().flatten() {
            let model_dir = bus.path().join("model");
            for cfg in std::fs::read_dir(&model_dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("cfg"))) {
                let Ok(model) = omsi_model::Model::load(&cfg) else { continue };
                let shown: Vec<String> = destination_displays(&model).iter().map(|&i| format!("{} ({})", model.text_textures[i].variable, model.text_textures[i].font)).collect();
                if !shown.is_empty() {
                    eprintln!("{}: {}", cfg.strip_prefix(&root).unwrap_or(&cfg).display(), shown.join(", "));
                }
                let dir = bus.file_name().to_string_lossy().to_string();
                found.extend(destination_displays(&model).iter().map(|&i| (dir.clone(), model.text_textures[i].variable.clone())));
            }
        }
        if found.iter().any(|(d, _)| d == "MAN_SD200") {
            assert!(found.iter().any(|(d, v)| d == "MAN_SD200" && v == "Matrix_Terminus"));
        }
        assert!(!found.iter().any(|(_, v)| v.eq_ignore_ascii_case("ibis") || v.eq_ignore_ascii_case("ident")), "{found:?}");
    }

    /// The installed OMSI 2's fonts (`OMSI_ROOT`; skipped without it): the stock Hamburg LED
    /// fonts are read as the matrices they show - their dots a pixel off from letter to letter,
    /// round or L-shaped - and every font read as a grid is printed with it.
    #[test]
    fn installed_led_fonts_are_read_as_dots() {
        let Some(root) = omsi_cfg::env::var_os("OMSI_ROOT").map(std::path::PathBuf::from) else {
            eprintln!("skipped: no OMSI_ROOT");
            return;
        };
        let mut lib = FontLibrary::new(&root);
        let decode = |p: &Path| omsi_texture::decode_file(p).ok().map(|i| (i.width, i.height, i.rgba));
        let fonts = FontLibrary::new(&root).installed();
        for f in &fonts {
            let Some(a) = lib.get(&f.name, &decode) else { continue };
            let g = DotGrid::of_atlas(&a);
            if g.pitch > 1 || g.smooth {
                eprintln!("{}: pitch {} dot {} phase {}/{} rows {} smooth {}", f.name.trim(), g.pitch, g.dot, g.phase_x, g.phase_y, g.rows, g.smooth);
            }
        }
        for name in ["4_LW_Benefit_Gross", "4_LW_Benefit_Klein", "17_LED_Gross", "HC_LED_Klein", "krueger-font_linie"] {
            let Some(a) = lib.get(name, &decode) else {
                eprintln!("skipped: no {name}");
                continue;
            };
            let g = DotGrid::of_atlas(&a);
            assert!(g.pitch >= 3, "{name}: {g:?}");
            // (its letters as dots: an H of two strokes and a bar, not solid nor striped)
            let dots = oft_dots(&a);
            let band = LetterBand::of(&dots);
            assert!(!band.fill && band.cap_rows() >= 5, "{name}: {band:?}");
        }
    }

    /// The installed OMSI 2's buses and fonts (`OMSI_ROOT`; skipped without it): every font a
    /// bus writes its destination in - a destination display's, a sign script's letters' -
    /// stood in for by the bundled Hanken Grotesk and by every installed `.oft` font of letters.
    /// No letter both fonts draw comes out empty; the capitals stand on the line of the bus's
    /// own (a vector font's as tall as those, an `.oft` font's no taller: it is magnified by
    /// whole times only); on a matrix whose dots stand apart every lit pixel is a dot's. With the
    /// bus's own font, as the game draws, every letter and digit the chosen font writes is in
    /// its drawing (a small letter it lacks as its capital) and the bus's pictograms are its own.
    #[test]
    fn installed_display_fonts_take_any_chosen_font() {
        let Some(root) = omsi_cfg::env::var_os("OMSI_ROOT").map(PathBuf::from) else {
            eprintln!("skipped: no OMSI_ROOT");
            return;
        };
        let decode = |p: &Path| omsi_texture::decode_file(p).ok().map(|i| (i.width, i.height, i.rgba));
        let mut lib = FontLibrary::new(&root);
        // the fonts the buses write their destinations in
        let mut names: Vec<String> = Vec::new();
        let mut scripts_seen: Vec<Vec<PathBuf>> = Vec::new();
        for bus in std::fs::read_dir(root.join("Vehicles")).into_iter().flatten().flatten() {
            for file in std::fs::read_dir(bus.path()).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("bus"))) {
                let Ok(def) = omsi_vehicle::Vehicle::load(&file) else { continue };
                if let Some(m) = def.model.as_deref().and_then(|m| omsi_model::Model::load(&omsi_cfg::resolve_path(def.dir(), m)).ok()) {
                    names.extend(destination_displays(&m).into_iter().map(|i| m.text_textures[i].font.trim().to_string()));
                }
                if !scripts_seen.contains(&def.scripts.scripts) {
                    scripts_seen.push(def.scripts.scripts.clone());
                    names.extend(script_signs(&def.scripts.scripts).into_iter().flat_map(|s| s.fonts));
                }
            }
        }
        let mut own: Vec<Arc<FontAtlas>> = Vec::new();
        for n in names {
            if own.iter().any(|a| a.font.name.trim().eq_ignore_ascii_case(n.trim())) {
                continue;
            }
            if let Some(a) = lib.get(&n, &decode).filter(|a| is_letter_font(&a.font)) {
                own.push(a);
            }
        }
        // the fonts chosen: the bundled vector font, every installed `.oft` font of letters
        let hanken: &[u8] = include_bytes!("../../../assets/fonts/HankenGrotesk/HankenGrotesk-latin-700.ttf");
        let mut chosen = vec![DisplayFontSpec::vector("Hanken Grotesk Bold", Path::new("Hanken.ttf"), 0)];
        chosen.extend(lib.installed().iter().filter(|f| is_letter_font(f)).map(|f| DisplayFontSpec::named(f.name.trim())));
        eprintln!("{} fonts of destinations, {} fonts chosen for them", own.len(), chosen.len());
        let mut failures: Vec<(String, String)> = Vec::new();
        let mut drawn = 0usize;
        for a in &own {
            let target = Target::of(Some(a), a.font.height.max(1) as u32, &DisplayFontSpec::default());
            let (grid, band) = (target.grid, &target.band);
            if band.fill {
                eprintln!("{}: no letters to read where they sit", a.font.name.trim());
                continue;
            }
            eprintln!("{}: grid {}/{} px, {} rows, capitals rows {}..{}, {:.1} dots wide", a.font.name.trim(), grid.pitch, grid.dot, grid.rows, band.cap_top, band.base - 1, target.width);
            let letters: Vec<char> = "ABEFHLNOPTUZ0123456789".chars().filter(|&c| ink_rows(a, c).is_some()).collect();
            let p = grid.pitch.max(1);
            for spec in &chosen {
                let oft = if spec.is_vector() { None } else { lib.oft_size(&spec.name, band, &decode) };
                // (a font of pictures - X10_Hst's stop names, each "letter" 200 px wide - stands
                // in as what it is)
                if oft.as_ref().is_some_and(|d| d.1.fill || d.1.cap_w > d.1.cap_rows() as f32 * 3.0) {
                    continue;
                }
                let (src, has): (SubstituteSource, Box<dyn Fn(char) -> bool>) = match &oft {
                    None if spec.is_vector() => (SubstituteSource::Vector(hanken, 0), Box::new(|_| true)),
                    None => continue,
                    Some(d) => (SubstituteSource::Oft(&d.0), Box::new(|c| d.0.glyph(c).is_some_and(|g| g.cov.iter().any(|&v| v >= omsi_content::dotfont::LIT)))),
                };
                let at = format!("{} in {}", a.font.name.trim(), spec.name);
                let Some(sub) = substitute(src, spec, &target, None) else {
                    failures.push((spec.name.clone(), format!("{at}: nothing made")));
                    continue;
                };
                drawn += 1;
                for &c in letters.iter().filter(|&&c| has(c)) {
                    if ink_rows(&sub.atlas, c).is_none() {
                        failures.push((spec.name.clone(), format!("{at}: {c} empty")));
                        continue;
                    }
                    if p > 1 && !grid.smooth {
                        let g = sub.atlas.font.chars.iter().find(|g| g.ch == c).unwrap();
                        let off = (g.x0..g.x1).flat_map(|x| (0..sub.atlas.font.height).map(move |y| (x, y))).find(|&(x, y)| {
                            let lit = sub.atlas.alpha[(((g.y + y) as u32 * sub.atlas.width + x as u32) * 4) as usize] > 0;
                            let (dx, dy) = ((x - g.x0) as u32, y as u32);
                            lit && (dx < grid.phase_x || (dx - grid.phase_x) % p >= grid.dot || dy < grid.phase_y || (dy - grid.phase_y) % p >= grid.dot)
                        });
                        if let Some((x, y)) = off {
                            failures.push((spec.name.clone(), format!("{at}: {c} lit at ({}, {y}), off the dots", x - g.x0)));
                        }
                    }
                }
                // where its letters stand, read as the bus's own are (the middle of a flat H, E,
                // I, T, N, L and F: a font whose boxes take in ink of the line above or below -
                // some smooth fonts' are cut that tall - keeps it, as Omsi.exe draws it)
                let mine = LetterBand::of(&DotFont::from_atlas(&sub.atlas, &grid));
                let tall_enough = !spec.is_vector() || mine.cap_top.abs_diff(band.cap_top) <= 1;
                if !mine.fill && (mine.base.abs_diff(band.base) > 1 || mine.cap_top + 1 < band.cap_top || !tall_enough) {
                    failures.push((spec.name.clone(), format!("{at}: capitals in rows {}..{}, its own {}..{}", mine.cap_top, mine.base - 1, band.cap_top, band.base - 1)));
                }
                // with the bus's own font, as the game draws: every letter and digit of it the
                // chosen font writes in the chosen font's drawing (a small letter it has none of
                // as its capital: the line in one font), the bus's pictograms in the bus's
                let merged = omsi_content::dotfont::merge_kept(&sub.atlas, a, &target.kept);
                for c in a.font.chars.iter().map(|g| g.ch).filter(|c| c.is_alphanumeric() && !target.kept.contains(c)) {
                    let cap = {
                        let mut u = c.to_uppercase();
                        match (u.next(), u.next()) {
                            (Some(u), None) if u != c => Some(u),
                            _ => None,
                        }
                    };
                    let want = std::iter::once(c).chain(cap).map(|x| pixels_of(&sub.atlas, x)).find(|p| p.iter().any(|v| *v > 0));
                    if want.is_some_and(|w| pixels_of(&merged, c) != w) {
                        failures.push((spec.name.clone(), format!("{at}: {c} not in the chosen font")));
                    }
                }
                for &c in &target.kept {
                    if pixels_of(&merged, c) != pixels_of(a, c) {
                        failures.push((spec.name.clone(), format!("{at}: the pictogram {c} not the bus's own")));
                    }
                }
            }
        }
        let mut by_font: Vec<(String, usize)> = Vec::new();
        for (f, _) in &failures {
            match by_font.iter_mut().find(|b| &b.0 == f) {
                Some(b) => b.1 += 1,
                None => by_font.push((f.clone(), 1)),
            }
        }
        by_font.sort_by_key(|b| std::cmp::Reverse(b.1));
        eprintln!("{drawn} substitutes drawn, {} failures; by the font chosen: {by_font:?}", failures.len());
        assert!(failures.is_empty(), "{}", failures.iter().take(60).map(|f| f.1.clone()).collect::<Vec<_>>().join("\n"));
    }

    /// A text texture fitted to a display font (a display whose own font is missing) draws it
    /// in the bus's own size and place; a line too wide runs off the edge at that size.
    #[test]
    fn a_display_font_keeps_the_display_size_colour_and_place() {
        use omsi_content::font::{Font, FontChar};
        // a 5x7 pixel font: one letter, solid
        let font = Font { name: "Pix 7".into(), height: 7, gap: 1, chars: vec![FontChar { ch: 'A', x0: 0, x1: 5, y: 0 }], ..Default::default() };
        let alpha: Vec<u8> = (0..5 * 7).flat_map(|_| [255u8, 255, 255, 255]).collect();
        let atlas = Arc::new(FontAtlas::new(font, 5, 7, alpha.clone(), alpha));
        let mut def = tt("Matrix_Terminus", "Annax Small");
        (def.width, def.height) = (40, 16);
        let mut s = TextTextureState::new(def, Some(atlas));
        s.fit = Some(16);
        let ink_of = |img: &[u8]| -> Vec<(usize, usize)> { (0..16).flat_map(|y| (0..40).map(move |x| (x, y))).filter(|(x, y)| img[(y * 40 + x) * 4 + 3] > 0).collect() };
        let img = s.image("A");
        let ink = ink_of(&img);
        // twice the size (10 x 14), centred, the bus's colour, every pixel whole
        assert_eq!(ink.len(), 10 * 14);
        assert_eq!(ink.iter().map(|p| p.0).min(), Some(15));
        assert_eq!(ink.iter().map(|p| p.1).min(), Some(1));
        assert!(ink.iter().all(|(x, y)| img[(y * 40 + x) * 4..(y * 40 + x) * 4 + 4] == [255, 160, 0, 255]));
        // (not squeezed by a fraction to fit: every pixel still whole, the letters as tall)
        let img = s.image("AAAAAAA");
        let ink = ink_of(&img);
        assert_eq!((ink.iter().map(|p| p.0).min(), ink.iter().map(|p| p.0).max()), (Some(0), Some(39)));
        assert!(ink.iter().all(|(x, y)| img[(y * 40 + x) * 4 + 3] == 255));
        assert_eq!(ink.iter().map(|p| p.1).max(), Some(14));
    }

    /// 5 x 7 letters ('#' lit; '.' one column wide).
    const PATTERNS: &[(char, [&str; 7])] = &[
        ('H', ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"]),
        ('A', [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"]),
        ('U', ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."]),
        ('P', ["####.", "#...#", "#...#", "####.", "#....", "#....", "#...."]),
        ('T', ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."]),
        ('B', ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####."]),
        ('N', ["#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#", "#...#"]),
        ('O', [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."]),
        ('F', ["#####", "#....", "#....", "####.", "#....", "#....", "#...."]),
        ('E', ["#####", "#....", "#....", "####.", "#....", "#....", "#####"]),
        ('I', [".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###."]),
        ('L', ["#....", "#....", "#....", "#....", "#....", "#....", "#####"]),
        ('W', ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "##.##", "#...#"]),
        ('C', [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."]),
        ('D', ["####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####."]),
        ('1', ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."]),
        ('2', [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"]),
        ('.', [".", ".", ".", ".", ".", ".", "#"]),
    ];

    /// A font of `PATTERNS` drawn as a picture of a matrix: each dot of a letter `mag` x `mag`
    /// cells of `pitch` px, a `dot` x `dot` square lit at the start of each (no seams when it
    /// is the pitch), the letters `top` cells into lines `h` px high; each letter's box its
    /// cells and `pad` more, `gap` px between letters. `solid`: every cell of a letter's box in
    /// its seven rows lit (block letters).
    #[allow(clippy::too_many_arguments)]
    fn dot_atlas(name: &str, pitch: u32, dot: u32, mag: u32, top: u32, h: u32, pad: u32, gap: i32, solid: bool) -> FontAtlas {
        use omsi_content::font::FontChar;
        let widths: Vec<u32> = PATTERNS.iter().map(|p| (p.1[0].len() as u32 * mag + pad) * pitch).collect();
        let aw: u32 = widths.iter().sum::<u32>().max(1);
        let mut alpha = vec![0u8; (aw * h * 4) as usize];
        let mut chars = Vec::new();
        let mut x0 = 0u32;
        for ((ch, rows), w) in PATTERNS.iter().zip(&widths) {
            for (r, row) in rows.iter().enumerate() {
                for (c, b) in row.bytes().enumerate() {
                    if b != b'#' && !solid {
                        continue;
                    }
                    for (my, mx) in (0..mag).flat_map(|my| (0..mag).map(move |mx| (my, mx))) {
                        let (cx, cy) = (x0 + (c as u32 * mag + mx) * pitch, (top + r as u32 * mag + my) * pitch);
                        for (dy, dx) in (0..dot).flat_map(|dy| (0..dot).map(move |dx| (dy, dx))) {
                            if cy + dy < h {
                                let at = (((cy + dy) * aw + cx + dx) * 4) as usize;
                                alpha[at..at + 4].copy_from_slice(&[255; 4]);
                            }
                        }
                    }
                }
            }
            chars.push(FontChar { ch: *ch, x0: x0 as i32, x1: (x0 + w) as i32, y: 0 });
            x0 += w;
        }
        let font = Font { name: name.into(), height: h as i32, gap, chars, ..Default::default() };
        FontAtlas::new(font, aw, h, alpha.clone(), alpha)
    }

    /// The rows the glyph `ch` of `a` has ink in, first and last.
    fn ink_rows(a: &FontAtlas, ch: char) -> Option<(u32, u32)> {
        let g = a.font.chars.iter().find(|g| g.ch == ch)?;
        let lit = |y: u32| (g.x0..g.x1).any(|x| a.alpha[(((g.y as u32 + y) * a.width + x as u32) * 4) as usize] > 0);
        let first = (0..a.font.height as u32).find(|&y| lit(y))?;
        Some((first, (0..a.font.height as u32).rev().find(|&y| lit(y)).unwrap_or(first)))
    }

    /// `glyph_pixels` of a glyph whose box may reach past its bitmap (an installed font's
    /// last line): nothing there.
    fn pixels_of(a: &FontAtlas, ch: char) -> Vec<u8> {
        let Some(g) = a.font.chars.iter().find(|g| g.ch == ch) else { return Vec::new() };
        let at = |x: i32, y: i32| if x < 0 || y < 0 || x >= a.width as i32 || y >= a.height as i32 { 0 } else { a.alpha[((y as u32 * a.width + x as u32) * 4) as usize] };
        (0..a.font.height).flat_map(|y| (g.x0..g.x1).map(move |x| (x, y))).map(|(x, y)| at(x, g.y + y)).collect()
    }

    /// The pixels of the glyph `ch` of `a`, row by row.
    fn glyph_pixels(a: &FontAtlas, ch: char) -> Vec<u8> {
        let Some(g) = a.font.chars.iter().find(|g| g.ch == ch) else { return Vec::new() };
        (0..a.font.height).flat_map(|y| (g.x0..g.x1).map(move |x| (x, y))).map(|(x, y)| a.alpha[(((g.y + y) as u32 * a.width + x as u32) * 4) as usize]).collect()
    }

    fn hanken(dir: &Path) -> DisplayFontSpec {
        std::fs::create_dir_all(dir).unwrap();
        let ttf = dir.join("Hanken.ttf");
        std::fs::write(&ttf, include_bytes!("../../../assets/fonts/HankenGrotesk/HankenGrotesk-latin-700.ttf")).unwrap();
        DisplayFontSpec::vector("Hanken Grotesk Bold", &ttf, 0)
    }

    /// Two fonts of one display's height and grid, their capitals 10 and 13 rows high: a
    /// display font stands in for each as tall as its letters (one atlas for both made them
    /// one size, and a matrix script measuring its sizes found none smaller).
    #[test]
    fn each_size_has_its_own_substitute() {
        let dir = std::env::temp_dir().join(format!("omsi_texttex_sizes_{}", std::process::id()));
        let spec = hanken(&dir);
        let mut lib = FontLibrary::new(&dir);
        let none = |_: &Path| None;
        // (7 rows of letters magnified: 10 x 14 and 13 x 13 cells high is no pattern; solid
        // blocks of 10 and 13 rows are)
        let ten = dot_atlas("Krueger 16x5", 1, 1, 1, 0, 16, 1, 1, true);
        let ten = shifted(&ten, 3, 10);
        let thirteen = shifted(&dot_atlas("Krueger 16x7", 1, 1, 1, 0, 16, 1, 1, true), 1, 13);
        let a = lib.display_font(&spec, Some(&ten), 16, &none).unwrap();
        let b = lib.display_font(&spec, Some(&thirteen), 16, &none).unwrap();
        assert_eq!(ink_rows(&a, 'H'), Some((3, 12)));
        assert_eq!(ink_rows(&b, 'H'), Some((1, 13)));
        assert_eq!((a.font.height, b.font.height), (16, 16));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `a` (a font of block letters 7 rows high from its top) with every letter's seven rows
    /// stretched to `rows` rows from row `top`.
    fn shifted(a: &FontAtlas, top: u32, rows: u32) -> FontAtlas {
        let mut alpha = vec![0u8; a.alpha.len()];
        for y in 0..a.height {
            for x in 0..a.width {
                let src = if y >= top && y < top + rows { (y - top) * 7 / rows } else { u32::MAX };
                if src < a.height {
                    let (s, d) = (((src * a.width + x) * 4) as usize, ((y * a.width + x) * 4) as usize);
                    let v = a.alpha[s];
                    alpha[d..d + 4].copy_from_slice(&[v; 4]);
                }
            }
        }
        FontAtlas::new(a.font.clone(), a.width, a.height, alpha.clone(), alpha)
    }

    /// The stock Hamburg LED fonts chosen for a display of another font: a picture of LEDs
    /// (each a 2x2 square in a cell of 4 px, the letters two LEDs to a dot and 104 px high),
    /// shrunk onto pixel fonts of 7, 13 and 16 rows. No letter of the destination comes out
    /// empty (they did: every dot fell under the threshold, and the sign stayed dark).
    #[test]
    fn a_chosen_led_font_is_never_empty() {
        let dir = PathBuf::from("/fonts");
        let led = dot_atlas("LED Gross", 4, 2, 2, 4, 104, 1, 0, false);
        let (lw, lh, lrgba) = (led.width, led.height, led.alpha.clone());
        let font = Font { path: dir.join("led.oft"), bitmap: "led.bmp".into(), alpha: "led.bmp".into(), ..led.font.clone() };
        let decode = move |p: &Path| -> Option<(u32, u32, Vec<u8>)> { (p.file_name()?.to_str()? == "led.bmp").then(|| (lw, lh, lrgba.clone())) };
        let mut lib = FontLibrary::new(&dir);
        lib.index = Some(vec![font]);
        let spec = DisplayFontSpec::named("LED Gross");
        let read = lib.get("LED Gross", &decode).unwrap();
        let g = DotGrid::of_atlas(&read);
        assert_eq!((g.pitch, g.dot), (4, 2), "{g:?}");
        for (rows, top, caps) in [(7u32, 0u32, 7u32), (13, 2, 11), (16, 3, 10)] {
            let own = shifted(&dot_atlas(&format!("Pix {rows}"), 1, 1, 1, 0, rows, 1, 1, true), top, caps);
            let sub = lib.display_font(&spec, Some(&own), rows as i32, &decode).unwrap();
            assert_eq!(sub.font.height, rows as i32);
            for ch in "HAUPTBAHNHOF 12".chars().filter(|c| !c.is_whitespace()) {
                let px = glyph_pixels(&sub, ch);
                assert!(px.iter().any(|a| *a > 0), "{ch} on {rows} rows is empty");
                assert_ne!(px, glyph_pixels(&own, ch), "{ch} on {rows} rows is the bus's own");
                // (in the bus's capitals' rows)
                let (t, b) = ink_rows(&sub, ch).unwrap();
                assert!(t >= top && b < top + caps, "{ch} on {rows} rows: rows {t}..{b}");
            }
        }
    }

    /// A pixel font of `h` rows: each of `chars` a 4 x `h` letter (its own pattern of `lit`).
    fn pixel_font(name: &str, h: u32, chars: &str, lit: impl Fn(char, u32, u32) -> bool) -> FontAtlas {
        use omsi_content::font::FontChar;
        let aw = chars.chars().count() as u32 * 5;
        let mut alpha = vec![0u8; (aw * h * 4) as usize];
        let mut glyphs = Vec::new();
        for (i, ch) in chars.chars().enumerate() {
            let x0 = i as u32 * 5;
            for (y, x) in (0..h).flat_map(|y| (0..4).map(move |x| (y, x))).filter(|&(y, x)| lit(ch, x, y)) {
                let at = ((y * aw + x0 + x) * 4) as usize;
                alpha[at..at + 4].copy_from_slice(&[255; 4]);
            }
            glyphs.push(FontChar { ch, x0: x0 as i32, x1: (x0 + 4) as i32, y: 0 });
        }
        let font = Font { name: name.into(), height: h as i32, gap: 1, chars: glyphs, ..Default::default() };
        FontAtlas::new(font, aw, h, alpha.clone(), alpha)
    }

    /// A Hamburg bus's LED font - small letters too, as `4_LW_Benefit_Gross` - in a chosen
    /// font of capitals alone (a picture of LEDs, as the stock "Annax Small" or "Krueger 16x9"):
    /// the small letters of a destination are the chosen font's capitals, the whole line in it.
    /// (They were left in the bus's own font, the capitals alone in the chosen one.)
    #[test]
    fn a_chosen_font_of_capitals_writes_the_whole_line() {
        let dir = PathBuf::from("/fonts");
        let led = dot_atlas("LED Gross", 4, 2, 2, 4, 104, 1, 0, false);
        let (lw, lh, lrgba) = (led.width, led.height, led.alpha.clone());
        let font = Font { path: dir.join("led.oft"), bitmap: "led.bmp".into(), alpha: "led.bmp".into(), ..led.font.clone() };
        let decode = move |p: &Path| -> Option<(u32, u32, Vec<u8>)> { (p.file_name()?.to_str()? == "led.bmp").then(|| (lw, lh, lrgba.clone())) };
        let mut lib = FontLibrary::new(&dir);
        lib.index = Some(vec![font]);
        // capitals in rows 3 to 12 of 16, small letters from row 6, descenders to row 15
        let chars: String = "ABEFHILNOPTU".chars().chain('a'..='z').collect();
        let own = pixel_font("LED Own", 16, &chars, |c, x, y| match c {
            'g' | 'j' | 'p' | 'q' | 'y' => (6..16).contains(&y) && (x == 1 || y == 6),
            c if c.is_lowercase() => (6..13).contains(&y) && (x == 1 || y == 12),
            _ => (3..13).contains(&y) && (x == 0 || x == 3 || y == 3),
        });
        let sub = lib.display_font(&DisplayFontSpec::named("LED Gross"), Some(&own), 16, &decode).unwrap();
        for small in "hauptbahnhof".chars() {
            let cap = small.to_ascii_uppercase();
            let px = glyph_pixels(&sub, small);
            assert!(px.iter().any(|a| *a > 0), "{small} is empty");
            assert_eq!(px, glyph_pixels(&sub, cap), "{small}: the chosen font's {cap}");
            assert_ne!(px, glyph_pixels(&own, small), "{small}: the bus's own");
        }
    }

    /// The SD200's Annax Large (a font of capitals) in a chosen vector font: the pictures it
    /// draws over letters and marks of text - a wheelchair over `g` and `h`, a checked flag over
    /// `x`, its depot's roof over `/^\` - stay its own; its letters are the chosen font's.
    #[test]
    fn a_buses_pictograms_stay_its_own() {
        let dir = std::env::temp_dir().join(format!("omsi_texttex_pictograms_{}", std::process::id()));
        let spec = hanken(&dir);
        let mut lib = FontLibrary::new(&dir);
        let none = |_: &Path| None;
        let own = pixel_font("Annax Large", 16, "ABEFGHILNOPTUXghx/^\\", |c, x, y| match c {
            'g' => y == 3 || x == 1 || (x + y) % 5 == 0,
            'h' => (3..15).contains(&y) && (x + 2 * y) % 3 == 0,
            'x' => y < 15 && (x + y) % 2 == 0,
            // (the roof's slopes and its top, on a bar)
            '/' => (13..15).contains(&y) || ((9..13).contains(&y) && x == 12 - y),
            '\\' => (13..15).contains(&y) || ((9..13).contains(&y) && x == y - 9),
            '^' => (13..15).contains(&y) || (y < 9 && (1..=2).contains(&x)),
            _ => y < 15 && (x == 0 || x == 3 || y == 0),
        });
        let sub = lib.display_font(&spec, Some(&own), 16, &none).unwrap();
        for c in ['g', 'h', 'x', '/', '^', '\\'] {
            assert_eq!(glyph_pixels(&sub, c), glyph_pixels(&own, c), "{c}: the bus's own");
        }
        for c in ['H', 'A', 'G'] {
            assert_ne!(glyph_pixels(&sub, c), glyph_pixels(&own, c), "{c}: the chosen font's");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Block capitals `w` wide, `caps` rows high from row `top` of a line of `h` (a Krüger
    /// matrix size), each with a bar in its middle; a gap of 1.
    fn krueger(name: &str, h: u32, top: u32, caps: u32, w: u32) -> FontAtlas {
        use omsi_content::font::FontChar;
        let letters: Vec<char> = ('A'..='Z').chain('0'..='9').collect();
        let aw = letters.len() as u32 * w;
        let mut alpha = vec![0u8; (aw * h * 4) as usize];
        let mut chars = Vec::new();
        for (i, ch) in letters.iter().enumerate() {
            let x0 = i as u32 * w;
            for y in top..top + caps {
                for x in 0..w {
                    if x == 0 || x == w - 1 || y == top || y == top + caps / 2 {
                        let at = ((y * aw + x0 + x) * 4) as usize;
                        alpha[at..at + 4].copy_from_slice(&[255; 4]);
                    }
                }
            }
            chars.push(FontChar { ch: *ch, x0: x0 as i32, x1: (x0 + w) as i32, y: 0 });
        }
        let font = Font { name: name.into(), height: h as i32, gap: 1, chars, ..Default::default() };
        FontAtlas::new(font, aw, h, alpha.clone(), alpha)
    }

    /// The stock MAN NL's Krüger matrix (`VMatrix.osc`): a destination tried in 16x9, 16x7,
    /// 11x5 and 9x4 until it fits the 85 px beside the line number, then spaced out with
    /// `l3 l1 - l6 / trunc l7 min` (the room left over the gaps, at most the size's own). In a
    /// display font standing in for each size it picks the size it picks in its own fonts - a
    /// substitute wider than the font it replaced fell to the smallest size with a negative
    /// spacing, its letters run together.
    #[test]
    fn the_matrix_cascade_picks_the_same_size() {
        let dir = std::env::temp_dir().join(format!("omsi_texttex_cascade_{}", std::process::id()));
        let spec = hanken(&dir);
        let own = [krueger("Krueger 16x9", 16, 0, 16, 9), krueger("Krueger 16x7", 16, 0, 16, 7), krueger("Krueger 11x5", 13, 2, 11, 5), krueger("Krueger 9x4", 9, 0, 9, 4)];
        let most = [9, 7, 5, 4];
        let mut lib = FontLibrary::new(&dir);
        let none = |_: &Path| None;
        let subs: Vec<Arc<FontAtlas>> = own.iter().map(|o| lib.display_font(&spec, Some(o), o.font.height, &none).unwrap()).collect();
        let cascade = |fonts: &[&FontAtlas], text: &str| -> (usize, i32) {
            let room = 85;
            let (mut pick, mut w) = (fonts.len() - 1, 0);
            for (i, f) in fonts.iter().enumerate() {
                (pick, w) = (i, f.font.text_width(text));
                if w <= room {
                    break;
                }
            }
            let gaps = (text.chars().count() as i32 - 1).max(0);
            (pick, if gaps > 0 { ((room - w) / gaps).min(most[pick]) } else { most[pick] })
        };
        let own_refs: Vec<&FontAtlas> = own.iter().collect();
        let sub_refs: Vec<&FontAtlas> = subs.iter().map(|a| a.as_ref()).collect();
        for text in ["ZOO", "BAHNHOF", "HAUPTBAHNHOF"] {
            eprintln!("{text}: {:?} in its own sizes, {:?} in the chosen font", own_refs.iter().map(|f| f.font.text_width(text)).collect::<Vec<_>>(), sub_refs.iter().map(|f| f.font.text_width(text)).collect::<Vec<_>>());
            let (o, os) = cascade(&own_refs, text);
            let (s, ss) = cascade(&sub_refs, text);
            assert_eq!(s, o, "{text}: {} in its own fonts, {} in the chosen one", own[o].font.name, own[s].font.name);
            assert!(os >= 0 && ss >= 0, "{text}: spacing {os} / {ss}");
        }
        // each size in its own rows: the 11x5's capitals from its row 2
        assert_eq!(ink_rows(&subs[2], 'H'), Some((2, 12)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Hamburg city bus's destination (`krueger-font_klein`: 2 px dots in 3 px cells, 7
    /// rows, a line of 20 px) padded with spaces to 45 characters, as its script writes it, on
    /// a texture of 512 px: in a display font it is drawn as the bus draws its own - from the
    /// texture's edge, the rest cut off - not squeezed by a fraction, which smeared every dot.
    #[test]
    fn a_padded_destination_is_not_squeezed() {
        use omsi_model::{MaterialDef, MeshDef, Model};
        let dir = std::env::temp_dir().join(format!("omsi_texttex_padded_{}", std::process::id()));
        let spec = hanken(&dir);
        let own = Arc::new(dot_atlas("krueger-font_klein", 3, 2, 1, 0, 20, 1, 3, false));
        assert_eq!(DotGrid::of_atlas(&own).pitch, 3);
        let mut def = tt("stringoutput_zielM_A", "krueger-font_klein");
        (def.width, def.height) = (512, 512);
        let model = Model { text_textures: vec![def.clone()], meshes: vec![MeshDef { viewpoint: 0, materials: vec![MaterialDef { use_text_texture: Some(0), ..Default::default() }], ..Default::default() }], ..Default::default() };
        let text = format!("{:<45}", "WWW.HOCHBAHN.DE");
        let mine = TextTextureState::new(def.clone(), Some(own.clone()));
        assert!(own.text_width(&text) - own.font.gap > 512, "padded wider than the texture, as on the bus");
        let mut states = vec![TextTextureState::new(def, Some(own.clone()))];
        let mut lib = FontLibrary::new(&dir);
        assert_eq!(apply_display_font(&mut states, &model, &spec.to_arg(), &mut lib, &|_| None), 1);
        assert_eq!(states[0].fit, None, "placed as the bus's own font");
        let (a, b) = (mine.image(&text), states[0].image(&text));
        let first = |img: &[u8]| (0..512u32).find(|&x| (0..512u32).any(|y| img[((y * 512 + x) * 4 + 3) as usize] > 0));
        assert_eq!(first(&b), first(&a));
        assert!(b.chunks(4).all(|p| p[3] == 0 || p[3] == 255), "every dot whole");
        // in the bus's line and its dots' rows
        let rows = |img: &[u8]| -> Vec<u32> { (0..512u32).filter(|&y| (0..512u32).any(|x| img[((y * 512 + x) * 4 + 3) as usize] > 0)).collect() };
        let (ra, rb) = (rows(&a), rows(&b));
        assert!(rb.iter().all(|y| ra.first().is_some_and(|f| y >= f) && ra.last().is_some_and(|l| y <= l)), "{ra:?} / {rb:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
