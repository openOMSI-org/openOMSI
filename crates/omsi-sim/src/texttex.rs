//! `[texttexture]`: textures generated from script string variables with `.oft` fonts.
//!
//! Every text texture is identified by its index in the model's `[texttexture]` list;
//! materials refer to it with `[useTextTexture] n`.

use hashbrown::HashMap;
use omsi_content::font::{Font, FontAtlas};
use omsi_model::TextTexture;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
struct DecodedFontBitmap {
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
}

pub struct FontLibrary {
    atlases: HashMap<String, Option<Arc<FontAtlas>>>,
    // An .oft can describe several sizes using one image. Loading each size must not
    // decode that image again while scripts wait for the font library's lock.
    images: HashMap<std::path::PathBuf, Option<DecodedFontBitmap>>,
    root: std::path::PathBuf,
    /// Every `[newfont]` of every content root's `Fonts/*.oft`, in lookup order; read once.
    index: Option<Vec<Font>>,
    /// The fonts of `root/Fonts` and no other (`only`).
    only_root: bool,
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
        FontLibrary { atlases: HashMap::new(), images: HashMap::new(), root: root.to_path_buf(), index: None, only_root: false }
    }

    /// The fonts of `root/Fonts` alone - not of every content folder: one bus's own (another
    /// player's bus, mounted as its driver has it), which this machine's fonts of the same
    /// names must not stand in for.
    pub fn only(root: &Path) -> FontLibrary {
        FontLibrary { only_root: true, ..FontLibrary::new(root) }
    }

    fn index(&mut self) -> &[Font] {
        if self.index.is_none() {
            // the Fonts folder of every content root (installed mods and archives first, then
            // the installation): a mod's display fonts live in the content folder, and looking
            // only in the installation's `Fonts` left the O530's number plate, matrix and
            // dashboard displays empty. A file of the same name higher up replaces the stock one.
            let mut files = if self.only_root { Vec::new() } else { omsi_cfg::read_dir_merged("Fonts") };
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

    /// `get` with the registered picture decoder (`crate::image_decode`).
    pub fn load(&mut self, name: &str) -> Option<Arc<FontAtlas>> {
        self.get(name, &|p| crate::image_decode::decode(p).ok().map(|i| (i.width, i.height, i.rgba)))
    }

    fn decoded_image(&mut self, file: std::path::PathBuf, decode: &dyn Fn(&Path) -> Option<(u32, u32, Vec<u8>)>) -> Option<DecodedFontBitmap> {
        if let Some(image) = self.images.get(&file) {
            return image.clone();
        }
        let image = decode(&file).map(|(width, height, rgba)| DecodedFontBitmap { width, height, pixels: rgba.into() });
        self.images.insert(file, image.clone());
        image
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
        let atlas = found.and_then(|f| {
            // the bitmaps sit beside the .oft that names them
            let fonts_dir = f.path.parent().map(Path::to_path_buf).unwrap_or_else(|| self.root.join("Fonts"));
            let alpha_path = omsi_cfg::resolve_path(&fonts_dir, &f.alpha);
            let color_path = omsi_cfg::resolve_path(&fonts_dir, &f.bitmap);
            let alpha = self.decoded_image(alpha_path, decode)?;
            let (aw, ah) = (alpha.width, alpha.height);
            let color = match self.decoded_image(color_path, decode) {
                Some(c) if (c.width, c.height) == (aw, ah) => c.pixels,
                Some(c) if c.width > 0 && c.height > 0 && c.pixels.len() == (c.width as usize * c.height as usize * 4) => {
                    color_at_alpha_pixels(&c.pixels, c.width, c.height, aw, ah).into()
                }
                _ => alpha.pixels.clone(),
            };
            Some(Arc::new(FontAtlas::from_shared(f, aw, ah, color, alpha.pixels)))
        });
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

/// Runtime state of one `[texttexture]`.
#[derive(Clone)]
pub struct TextTextureState {
    pub def: TextTexture,
    pub atlas: Option<Arc<FontAtlas>>,
    pub last_text: Option<String>,
    /// Latest rendered RGBA image, present when it changed since the last upload.
    pub pending: Option<Vec<u8>>,
}

impl TextTextureState {
    pub fn new(def: TextTexture, atlas: Option<Arc<FontAtlas>>) -> Self {
        Self { def, atlas, last_text: None, pending: None }
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
        match &self.atlas {
            Some(a) => a.render_aligned(text, w, h, self.def.full_color, rgb, align),
            None => vec![0u8; (w * h * 4) as usize],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bitmap_font(name: &str, folder: &str) -> Font {
        Font {
            path: Path::new(folder).join("display.oft"),
            name: name.into(),
            bitmap: "shared.bmp".into(),
            alpha: "shared.bmp".into(),
            height: 2,
            chars: vec![omsi_content::font::FontChar { ch: 'A', x0: 0, x1: 1, y: 0 }],
            ..Default::default()
        }
    }

    #[test]
    fn different_fonts_share_pixels_without_sharing_their_metrics() {
        let mut lib = FontLibrary::new(Path::new("fonts"));
        let first = bitmap_font("Small", "fonts");
        let mut second = bitmap_font("Wide", "fonts");
        second.gap = 3;
        lib.index = Some(vec![first.clone(), second.clone()]);
        let reads = std::cell::Cell::new(0);
        let pixels = vec![255; 16];
        let decode = |_: &Path| {
            reads.set(reads.get() + 1);
            Some((2, 2, pixels.clone()))
        };
        let small = lib.get("Small", &decode).unwrap();
        let wide = lib.get("Wide", &decode).unwrap();
        assert_eq!(reads.get(), 1);
        assert!(Arc::ptr_eq(&small.alpha, &wide.alpha));
        assert!(Arc::ptr_eq(&small.color, &small.alpha));
        assert_eq!(small.text_width("AA"), 2);
        assert_eq!(wide.text_width("AA"), 8);
        for (actual, font) in [(&small, first), (&wide, second)] {
            let uncached = FontAtlas::new(font, 2, 2, pixels.clone(), pixels.clone());
            assert_eq!(actual.render("AA", 12, 4, true, [0; 3]), uncached.render("AA", 12, 4, true, [0; 3]));
        }
        assert!(Arc::ptr_eq(&small, &lib.get("SMALL", &decode).unwrap()));
        assert_eq!(reads.get(), 1);
    }

    #[test]
    fn equal_bitmap_names_in_different_folders_stay_separate() {
        let mut lib = FontLibrary::new(Path::new("fonts"));
        lib.index = Some(vec![bitmap_font("Pack A", "pack_a"), bitmap_font("Pack B", "pack_b")]);
        let reads = std::cell::Cell::new(0);
        let decode = |p: &Path| {
            reads.set(reads.get() + 1);
            Some((2, 2, vec![if p.starts_with("pack_a") { 10 } else { 20 }; 16]))
        };
        let a = lib.get("Pack A", &decode).unwrap();
        let b = lib.get("Pack B", &decode).unwrap();
        assert_eq!(reads.get(), 2);
        assert_eq!((a.color[0], b.color[0]), (10, 20));
        assert!(!Arc::ptr_eq(&a.color, &b.color));
    }

    #[test]
    fn failed_images_are_remembered_only_in_this_library() {
        let fonts = vec![bitmap_font("First", "fonts"), bitmap_font("Second", "fonts")];
        let mut lib = FontLibrary::new(Path::new("fonts"));
        lib.index = Some(fonts.clone());
        let reads = std::cell::Cell::new(0);
        let missing = |_: &Path| {
            reads.set(reads.get() + 1);
            None
        };
        assert!(lib.get("First", &missing).is_none());
        assert!(lib.get("Second", &missing).is_none());
        assert_eq!(reads.get(), 1);
        let mut next = FontLibrary::new(Path::new("fonts"));
        next.index = Some(fonts);
        assert!(next.get("First", &|_| Some((2, 2, vec![255; 16]))).is_some());
    }

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
}
