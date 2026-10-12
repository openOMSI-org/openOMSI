//! Display fonts drawn the way a destination display shows letters: in its own grid of dots.
//!
//! A display font the player chose for a bus ([`DisplayFontSpec`]) is an OMSI `.oft` font (by
//! its `[newfont]` name) or a TrueType/OpenType font (`.ttf`, `.otf`, `.ttc`; one face of the
//! file). It stands in for the display's own font where that font's letters are: the display's
//! grid of dots ([`DotGrid`], read from its own font - a font drawn as a picture of a matrix,
//! the Annax fonts' 3x3 dots in 4 px cells or the Hamburg LEDs' round dots, comes back as the
//! matrix it shows) and where its letters sit and how wide they are ([`LetterBand`]). A vector
//! font is rasterised into those rows, its capitals as tall as the display's and on the same
//! line ([`DotFont::from_vector_in`]); an `.oft` font is read in its own grid and magnified by
//! whole times or shrunk to them ([`DotFont::fit_to`]); either narrowed to about the width of
//! the letters it replaces ([`condense_for`]), with their gap and space - a matrix script that
//! measures its text to pick a size then picks the size it picks in its own fonts. Bold and the
//! letter spacing are dots too. The dots are laid into the display's grid, thresholded as a
//! matrix shows them (on or off) unless the display's own font is a smooth one, into a
//! [`FontAtlas`] of the height of a line of the display's own font ([`DotFont::to_atlas`]);
//! what is no text - a script's pictograms behind braces, `#`, boxes - keeps the display's own
//! drawing ([`merge_kept`]). The game's text and script textures (`omsi_sim::texttex`,
//! `omsi_sim::host`) and the launcher's previews draw with that same atlas.

use crate::font::{Font, FontAtlas, FontChar};
use std::path::{Path, PathBuf};

// --- the choice -------------------------------------------------------------------------------

/// A display font with the settings that matter on a display, as the launcher keeps it per
/// bus and hands it to the game (`--display-font`, see [`DisplayFontSpec::to_arg`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct DisplayFontSpec {
    /// The `[newfont]` name of an `.oft` font, or the full name of a vector font's face.
    pub name: String,
    /// The TrueType/OpenType file of a vector font (none: an `.oft` font, found by its name).
    pub file: Option<PathBuf>,
    /// The face in a font collection (`.ttc`).
    pub face: u32,
    /// How tall its letters are drawn, capitals and descenders, in rows of dots (none: as
    /// tall as the display's own font's letters; see `LetterBand::scaled_to`). A choice of
    /// before the letters were fitted to the display's own meant the rows of the whole line,
    /// which it still does on a display of no font of its own.
    pub rows: Option<u32>,
    /// Every stroke a dot wider.
    pub bold: bool,
    /// Dots between two letters (none: as the display's own font has them).
    pub spacing: Option<u32>,
}

impl DisplayFontSpec {
    /// An `.oft` font by its name, as it is.
    pub fn named(name: &str) -> DisplayFontSpec {
        DisplayFontSpec { name: name.trim().to_string(), ..Default::default() }
    }

    /// One face of a vector font file, as it is.
    pub fn vector(name: &str, file: &Path, face: u32) -> DisplayFontSpec {
        DisplayFontSpec { name: name.trim().to_string(), file: Some(file.to_path_buf()), face, ..Default::default() }
    }

    /// Read a `--display-font` argument: a font's name alone (as the launcher wrote it before
    /// vector fonts and the settings), or the name with `|key=value` parts after it - `file=`,
    /// `face=`, `rows=`, `bold`, `spacing=`. None for a blank one.
    pub fn parse(arg: &str) -> Option<DisplayFontSpec> {
        let mut parts = arg.split('|');
        let name = parts.next().unwrap_or_default().trim();
        if name.is_empty() {
            return None;
        }
        let mut spec = DisplayFontSpec::named(name);
        for p in parts {
            let (k, v) = p.split_once('=').map(|(k, v)| (k.trim(), v.trim())).unwrap_or((p.trim(), ""));
            match k.to_ascii_lowercase().as_str() {
                "file" if !v.is_empty() => spec.file = Some(PathBuf::from(v)),
                "face" => spec.face = v.parse().unwrap_or(0),
                "rows" => spec.rows = v.parse().ok().filter(|r| *r > 0),
                "bold" => spec.bold = !matches!(v, "0" | "false" | "no"),
                "spacing" => spec.spacing = v.parse().ok(),
                _ => {}
            }
        }
        Some(spec)
    }

    /// The argument the game reads back with [`DisplayFontSpec::parse`]: the name alone when
    /// there is nothing else to say (what a game of before understands too).
    pub fn to_arg(&self) -> String {
        let mut out = self.name.trim().to_string();
        if let Some(f) = &self.file {
            out.push_str(&format!("|file={}", f.display()));
            if self.face > 0 {
                out.push_str(&format!("|face={}", self.face));
            }
        }
        if let Some(r) = self.rows {
            out.push_str(&format!("|rows={r}"));
        }
        if self.bold {
            out.push_str("|bold");
        }
        if let Some(s) = self.spacing {
            out.push_str(&format!("|spacing={s}"));
        }
        out
    }

    /// The font itself, without the settings (what a list of fonts offers).
    pub fn plain(&self) -> DisplayFontSpec {
        DisplayFontSpec { name: self.name.clone(), file: self.file.clone(), face: self.face, ..Default::default() }
    }

    /// Whether it is a TrueType/OpenType font.
    pub fn is_vector(&self) -> bool {
        self.file.is_some()
    }

    /// The same font (whatever the settings): the same name, and the same file and face.
    pub fn same_font(&self, other: &DisplayFontSpec) -> bool {
        self.name.trim().eq_ignore_ascii_case(other.name.trim()) && self.face == other.face && self.file.as_deref().map(path_key) == other.file.as_deref().map(path_key)
    }

    /// A key for caches and previews: the argument in lower case.
    pub fn key(&self) -> String {
        self.to_arg().to_lowercase()
    }
}

fn path_key(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/").to_lowercase()
}

// --- the display's grid -----------------------------------------------------------------------

/// The grid of dots a display's own font draws in: a dot every `pitch` pixels, `dot` pixels
/// of it lit (less than the pitch: a dark seam between the dots, as the Annax fonts draw
/// them), the first one `phase_x`/`phase_y` pixels into a letter, and how many rows of dots a
/// line of the font has. `smooth`: the font is no matrix but smooth letters (anti-aliased
/// pixels at a pitch of one), which a chosen font keeps too instead of being thresholded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DotGrid {
    pub pitch: u32,
    pub dot: u32,
    pub phase_x: u32,
    pub phase_y: u32,
    pub rows: u32,
    pub smooth: bool,
}

impl DotGrid {
    /// A matrix of `rows` rows whose every pixel is a dot (what a script's matrix texture is:
    /// the script lights an LED for each of its pixels).
    pub fn plain(rows: u32) -> DotGrid {
        DotGrid { pitch: 1, dot: 1, phase_x: 0, phase_y: 0, rows: rows.max(1), smooth: false }
    }

    /// The grid `atlas`'s font draws in, read from its letters. A picture of a matrix whose
    /// dots stand apart (a dark seam between them) is read by its dots, a matrix whose dots
    /// touch (a pixel font magnified a whole number of times) by its stretches of ink; each
    /// letter may sit in the grid at a phase of its own - the stock LED fonts' letters were
    /// cut out of their bitmaps a pixel off here and there - but the pitch and the dot must
    /// agree. A font of no such grid (smooth letters, a pixel font as it is) has a pitch of
    /// one, and is `smooth` when much of its ink is only partly covered.
    pub fn of_atlas(atlas: &FontAtlas) -> DotGrid {
        let h = atlas.font.height.max(1) as u32;
        let glyphs = sample_glyphs(atlas);
        let ink = Ink::of(atlas, &glyphs, h);
        // (LEDs drawn with a wide glow - SG_LED's - are blobs of one size only nearer their
        // full ink: there the dots are read again)
        let brighter = ((ink.full as u32 * 3 / 4) as u8).max(ink.core);
        let found = grid_of_dots(atlas, &glyphs, h, ink.core).or_else(|| (brighter > ink.core).then(|| grid_of_dots(atlas, &glyphs, h, brighter)).flatten()).or_else(|| grid_of_cells(atlas, &glyphs, h, ink.core));
        if let Some(g) = found {
            return g;
        }
        DotGrid { smooth: ink.partial > 0.05, ..DotGrid::plain(h) }
    }
}

/// How a font's letters are inked, read from their pixels: the alpha a pixel needs to be
/// part of a dot's core (half of what the ink mostly is: an anti-aliased LED's halo is left
/// out), what the ink mostly is (a dot read at that is fully lit), and the share of the inked
/// pixels that are only partly covered.
#[derive(Debug, Clone, Copy)]
struct Ink {
    core: u8,
    full: u8,
    partial: f32,
}

impl Ink {
    fn of(atlas: &FontAtlas, glyphs: &[&FontChar], h: u32) -> Ink {
        let mut hist = [0usize; 256];
        for g in glyphs {
            for y in 0..h as i32 {
                for x in g.x0..g.x1 {
                    hist[alpha_at(atlas, x, g.y + y) as usize] += 1;
                }
            }
        }
        let inked: usize = hist[1..].iter().sum();
        if inked == 0 {
            return Ink { core: 128, full: 255, partial: 0.0 };
        }
        // (the alpha nine tenths of the ink stay under)
        let mut seen = 0usize;
        let mut p90 = 255u8;
        for (a, n) in hist.iter().enumerate().skip(1) {
            seen += n;
            if seen * 10 >= inked * 9 {
                p90 = a as u8;
                break;
            }
        }
        let partial: usize = hist[16..240].iter().sum();
        Ink { core: (p90 / 2).max(48), full: p90.max(1), partial: partial as f32 / inked as f32 }
    }
}

/// The letters of `atlas` its grid is read from: each character once, no whitespace and
/// nothing empty, the first 256.
fn sample_glyphs(atlas: &FontAtlas) -> Vec<&FontChar> {
    let mut out: Vec<&FontChar> = Vec::new();
    for g in atlas.font.chars.iter().filter(|g| !g.ch.is_whitespace() && g.x1 > g.x0) {
        if out.iter().any(|o| o.ch == g.ch) {
            continue;
        }
        out.push(g);
        if out.len() == 256 {
            break;
        }
    }
    out
}

/// The coverage of `atlas`'s pixel (`x`, `y`) (none outside it).
fn alpha_at(atlas: &FontAtlas, x: i32, y: i32) -> u8 {
    if x < 0 || y < 0 || x >= atlas.width as i32 || y >= atlas.height as i32 {
        return 0;
    }
    atlas.alpha.get(((y as u32 * atlas.width + x as u32) * 4) as usize).copied().unwrap_or(0)
}

/// The value most of `values` have (the smallest of those as common), and how many have it.
fn mode_of(values: impl Iterator<Item = u32>) -> Option<(u32, usize)> {
    let mut counts: Vec<(u32, usize)> = Vec::new();
    for v in values {
        match counts.iter_mut().find(|c| c.0 == v) {
            Some(c) => c.1 += 1,
            None => counts.push((v, 1)),
        }
    }
    counts.into_iter().max_by_key(|&(v, n)| (n, std::cmp::Reverse(v)))
}

/// Each letter's own phase along one direction - the residue most of its dots (or stretches)
/// start at - and the share of them over all letters that keep to their letter's; with the
/// phase most letters have. `starts` are (letter, start) pairs.
fn phases(starts: &[(usize, u32)], p: u32) -> (f32, u32) {
    let mut letters: Vec<usize> = starts.iter().map(|s| s.0).collect();
    letters.dedup();
    let (mut agree, mut of_letters) = (0usize, Vec::new());
    for l in letters {
        if let Some((r, n)) = mode_of(starts.iter().filter(|s| s.0 == l).map(|s| s.1 % p)) {
            agree += n;
            of_letters.push((r, n));
        }
    }
    let common = mode_of(of_letters.iter().flat_map(|&(r, n)| std::iter::repeat_n(r, n))).map(|m| m.0).unwrap_or(0);
    (if starts.is_empty() { 0.0 } else { agree as f32 / starts.len() as f32 }, common)
}

/// The most dots a matrix font's dot may span (pixels), and so the largest pitch read.
const MAX_PITCH: u32 = 12;

/// `DotGrid::of_atlas` of a picture of a matrix whose dots stand apart: every dot a blob of
/// ink of its own - square, round, or the L of three pixels the Hamburg LED fonts draw - and
/// the blobs a pitch apart (the step between two neighbouring dots of a stroke). None when
/// the ink is no such blobs (smooth letters, a pixel font: its letters are whole blobs).
fn grid_of_dots(atlas: &FontAtlas, glyphs: &[&FontChar], h: u32, core: u8) -> Option<DotGrid> {
    // (letter, x, y, width, height) of every blob, from the letter's corner
    let mut blobs: Vec<(usize, u32, u32, u32, u32)> = Vec::new();
    let mut large = 0usize;
    for (li, g) in glyphs.iter().enumerate() {
        let gw = (g.x1 - g.x0) as u32;
        let inked: Vec<bool> = (0..h).flat_map(|y| (0..gw).map(move |x| (x, y))).map(|(x, y)| alpha_at(atlas, g.x0 + x as i32, g.y + y as i32) >= core).collect();
        let mut seen = vec![false; inked.len()];
        for start in 0..inked.len() {
            if !inked[start] || seen[start] {
                continue;
            }
            // (the pixels that touch, corners too)
            let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
            let mut stack = vec![start];
            seen[start] = true;
            while let Some(i) = stack.pop() {
                let (x, y) = (i as u32 % gw, i as u32 / gw);
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                for (dx, dy) in [(-1i32, -1i32), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= gw as i32 || ny >= h as i32 {
                        continue;
                    }
                    let n = (ny as u32 * gw + nx as u32) as usize;
                    if inked[n] && !seen[n] {
                        seen[n] = true;
                        stack.push(n);
                    }
                }
            }
            let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
            if bw >= MAX_PITCH || bh >= MAX_PITCH {
                large += 1;
            } else {
                blobs.push((li, x0, y0, bw, bh));
            }
        }
    }
    if blobs.len() < 12 || large * 10 > (blobs.len() + large) {
        return None;
    }
    // the step from a dot to the next one along a row and down a column
    let mut steps: Vec<u32> = Vec::new();
    for li in 0..glyphs.len() {
        let mut mine: Vec<(u32, u32)> = blobs.iter().filter(|b| b.0 == li).map(|b| (b.1, b.2)).collect();
        mine.sort_by_key(|&(x, y)| (y, x));
        steps.extend(mine.windows(2).filter(|w| w[0].1 == w[1].1).map(|w| w[1].0 - w[0].0));
        mine.sort();
        steps.extend(mine.windows(2).filter(|w| w[0].0 == w[1].0).map(|w| w[1].1 - w[0].1));
    }
    let (p, _) = mode_of(steps.iter().copied().filter(|s| (2..=MAX_PITCH).contains(s)))?;
    let (dot, _) = mode_of(blobs.iter().map(|b| b.3.max(b.4)))?;
    if steps.len() < 12 || dot >= p || p * 2 > h || steps.iter().filter(|s| *s % p == 0).count() * 10 < steps.len() * 9 {
        return None;
    }
    let (fx, phase_x) = phases(&blobs.iter().map(|b| (b.0, b.1)).collect::<Vec<_>>(), p);
    let (fy, phase_y) = phases(&blobs.iter().map(|b| (b.0, b.2)).collect::<Vec<_>>(), p);
    if fx < 0.9 || fy < 0.9 {
        return None;
    }
    let rows = if h >= phase_y + dot { (h - phase_y - dot) / p + 1 } else { (h / p).max(1) };
    Some(DotGrid { pitch: p, dot, phase_x, phase_y, rows, smooth: false })
}

/// `DotGrid::of_atlas` of a matrix whose dots touch (a pixel font magnified a whole number of
/// times, no seams): the largest pitch at which every stretch of ink along the rows and the
/// columns of a letter starts at the same place of a cell and is a whole number of cells long.
fn grid_of_cells(atlas: &FontAtlas, glyphs: &[&FontChar], h: u32, core: u8) -> Option<DotGrid> {
    // (letter, start, length) of every stretch, from the letter's corner
    let (mut along, mut down): (Vec<(usize, u32, u32)>, Vec<(usize, u32, u32)>) = (Vec::new(), Vec::new());
    for (li, g) in glyphs.iter().enumerate() {
        let gw = (g.x1 - g.x0) as u32;
        let on = |x: u32, y: u32| x < gw && y < h && alpha_at(atlas, g.x0 + x as i32, g.y + y as i32) >= core;
        for y in 0..h {
            let mut start: Option<u32> = None;
            for x in 0..=gw {
                match (on(x, y), start) {
                    (true, None) => start = Some(x),
                    (false, Some(s)) => {
                        along.push((li, s, x - s));
                        start = None;
                    }
                    _ => {}
                }
            }
        }
        for x in 0..gw {
            let mut start: Option<u32> = None;
            for y in 0..=h {
                match (on(x, y), start) {
                    (true, None) => start = Some(y),
                    (false, Some(s)) => {
                        down.push((li, s, y - s));
                        start = None;
                    }
                    _ => {}
                }
            }
        }
    }
    if along.len() < 12 || down.len() < 12 {
        return None;
    }
    for p in (2..=MAX_PITCH).rev() {
        if p * 2 > h {
            continue;
        }
        let whole = along.iter().chain(&down).filter(|r| r.2 % p == 0).count();
        if whole * 100 < (along.len() + down.len()) * 92 {
            continue;
        }
        let (fx, phase_x) = phases(&along.iter().map(|r| (r.0, r.1)).collect::<Vec<_>>(), p);
        let (fy, phase_y) = phases(&down.iter().map(|r| (r.0, r.1)).collect::<Vec<_>>(), p);
        if fx >= 0.92 && fy >= 0.92 {
            return Some(DotGrid { pitch: p, dot: p, phase_x, phase_y, rows: ((h - phase_y.min(h)) / p).max(1), smooth: false });
        }
    }
    None
}

// --- a font of dots ---------------------------------------------------------------------------

/// One letter of a [`DotFont`]: `w` columns of the font's rows, each dot's coverage (0-255),
/// row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct DotGlyph {
    pub ch: char,
    pub w: u32,
    pub cov: Vec<u8>,
}

/// A font as dots: its letters in `rows` rows, `gap` dots between two of them, a space
/// `space` dots wide.
#[derive(Debug, Clone, PartialEq)]
pub struct DotFont {
    pub name: String,
    pub rows: u32,
    pub gap: u32,
    pub space: u32,
    pub glyphs: Vec<DotGlyph>,
}

/// Fewer rows than this are too few for small letters with descenders: a font is made into
/// capitals there (its small letters written as capitals), as signs of so few rows write.
pub const CAPITALS_BELOW: u32 = 10;

/// The capital a small letter is written as on a sign of capitals.
fn capital_of(c: char) -> Option<char> {
    if !c.is_lowercase() {
        return None;
    }
    let mut up = c.to_uppercase();
    match (up.next(), up.next()) {
        (Some(u), None) if u != c => Some(u),
        _ => None,
    }
}

/// The letters a vector font is made into dots for: Latin with its accents (Western and
/// Central Europe), Greek, Cyrillic and the punctuation of destinations.
pub fn charset() -> Vec<char> {
    let ranges: [(u32, u32); 6] = [(0x21, 0x7e), (0xa1, 0x17f), (0x386, 0x3ce), (0x400, 0x45f), (0x490, 0x491), (0x2013, 0x2014)];
    let mut out: Vec<char> = ranges.iter().flat_map(|&(a, b)| (a..=b).filter_map(char::from_u32)).collect();
    out.extend(['‘', '’', '‚', '“', '”', '„', '…', '•', '€', '№']);
    out
}

impl DotFont {
    /// The letters of `atlas` as dots of `grid` (the font's own, [`DotGrid::of_atlas`]): each
    /// letter read at its own phase in the grid (where its dots hold the most ink, the grid's
    /// phase on a tie), each dot as the most ink of its pixels - a round or L-shaped LED is
    /// lit as a square one - made full where it reaches what the font's ink mostly is.
    pub fn from_atlas(atlas: &FontAtlas, grid: &DotGrid) -> DotFont {
        let p = grid.pitch.max(1);
        let d = grid.dot.clamp(1, p);
        let rows = grid.rows.max(1);
        let h = atlas.font.height.max(1) as u32;
        let full = Ink::of(atlas, &sample_glyphs(atlas), h).full as u32;
        let mut glyphs: Vec<DotGlyph> = Vec::new();
        for g in atlas.font.chars.iter().filter(|g| !g.ch.is_whitespace()) {
            if glyphs.iter().any(|o| o.ch == g.ch) {
                continue;
            }
            let gw = (g.x1 - g.x0).max(0) as u32;
            let cols_at = |fx: u32| if gw > fx { (gw - fx).div_ceil(p) } else { 0 };
            // (the pixels of the dot in column i and row j at phase (fx, fy), in the letter
            // and its line)
            let pixels = |fx: u32, fy: u32, i: u32, j: u32| {
                let (x, y) = (fx + i * p, fy + j * p);
                (y..(y + d).min(h)).flat_map(move |yy| (x..(x + d).min(gw)).map(move |xx| alpha_at(atlas, g.x0 + xx as i32, g.y + yy as i32)))
            };
            let (fx, fy) = if p == 1 {
                (0, 0)
            } else {
                let held = |fx: u32, fy: u32| -> u64 { (0..rows).flat_map(|j| (0..cols_at(fx)).map(move |i| (i, j))).map(|(i, j)| pixels(fx, fy, i, j).map(u64::from).sum::<u64>()).sum() };
                let mut best = ((grid.phase_x % p, grid.phase_y % p), 0u64);
                best.1 = held(best.0 .0, best.0 .1);
                for fy in 0..p {
                    for fx in 0..p {
                        let s = held(fx, fy);
                        if s > best.1 {
                            best = ((fx, fy), s);
                        }
                    }
                }
                best.0
            };
            let cols = cols_at(fx);
            let mut cov = vec![0u8; (cols * rows) as usize];
            for j in 0..rows {
                for i in 0..cols {
                    let most = pixels(fx, fy, i, j).max().unwrap_or(0) as u32;
                    cov[(j * cols + i) as usize] = (most * 255 / full).min(255) as u8;
                }
            }
            glyphs.push(DotGlyph { ch: g.ch, w: cols, cov });
        }
        let dots = |px: i32| ((px.max(0) as f32) / p as f32).round() as u32;
        DotFont { name: atlas.font.name.trim().to_string(), rows, gap: dots(atlas.font.gap), space: dots(atlas.font.space_width()).max(1), glyphs }
    }

    /// Face `face` of the TrueType/OpenType font `data` rasterised at `rows` rows of dots, for
    /// the letters `chars`, as a matrix sign's font fills its rows: from the top of its
    /// capitals and tall letters to the bottom of its descenders (a letter reaching further -
    /// a capital with an accent - squeezed into them, as signs do, not cut off), each letter's
    /// outline as much as it covers of every dot, its empty columns at either side left out:
    /// the letters stand `gap` dots apart. In fewer than `CAPITALS_BELOW` rows the capitals
    /// fill them and small letters are written as capitals. (`from_vector_in` a band that
    /// fills the rows: for a display whose own font says nothing of where its letters sit.)
    pub fn from_vector(data: &[u8], face: u32, rows: u32, chars: &[char]) -> Result<DotFont, String> {
        DotFont::from_vector_in(data, face, &LetterBand::filling(rows), chars, 1.0)
    }

    /// Face `face` of the TrueType/OpenType font `data` made into dots for the letters `chars`
    /// as the font of `band` has them: in its rows, the capitals as tall as its capitals
    /// (an H's height) and standing on its line, every letter `condense` times as wide as the
    /// face draws it (see `condense_for`). A letter reaching above the first row or below the
    /// last - an accent over a capital, a descender on a sign of capitals - is squeezed
    /// upright into them, not cut off. The letters stand the band's gap apart, a space is as
    /// wide as its font's (so that a destination padded with spaces lies as it does in it);
    /// in capitals (`band.capitals_only`, or too few rows for small letters) small letters
    /// are written as capitals.
    pub fn from_vector_in(data: &[u8], face: u32, band: &LetterBand, chars: &[char], condense: f32) -> Result<DotFont, String> {
        if band.fill {
            return DotFont::filling_from_vector(data, face, band.rows, chars);
        }
        use ab_glyph::{Font as _, FontRef, PxScale};
        let font = FontRef::try_from_slice_and_index(data, face).map_err(|e| e.to_string())?;
        let rows = band.rows.clamp(1, 512);
        let capitals = band.capitals_only || band.cap_rows() + band.below < CAPITALS_BELOW;
        let has = |c: char| font.glyph_id(c).0 != 0;
        // (an outline's bounds run from its top, `min`, to its bottom, `max`, in units
        // counted upwards from the line)
        let bounds = |c: char| -> Option<(f32, f32)> {
            let id = font.glyph_id(c);
            (id.0 != 0).then(|| font.outline(id)).flatten().map(|o| (o.bounds.min.y, o.bounds.max.y))
        };
        // the capitals' height in the font's units: a flat-topped capital's
        let cap_units = ['H', 'E', 'I', 'T', 'N', 'L', 'F'].iter().find_map(|&c| bounds(c).map(|b| b.0).filter(|t| *t > 0.0)).unwrap_or(font.ascent_unscaled() * 0.7).max(1.0);
        let units = font.height_unscaled().max(1.0);
        // (pixels per unit; a `PxScale` is the pixels of the font's whole height)
        let f = band.cap_rows() as f32 / cap_units;
        let condense = condense.clamp(0.1, 1.0);
        let base = band.base as f32;
        let mut glyphs: Vec<DotGlyph> = Vec::new();
        for &c in chars {
            if c.is_whitespace() || glyphs.iter().any(|g| g.ch == c) || (capitals && capital_of(c).is_some_and(has)) {
                continue;
            }
            let id = font.glyph_id(c);
            let Some((gt, gb)) = bounds(c).filter(|b| b.0 > b.1) else { continue };
            // where it reaches on the line, and squeezed upright into the rows when that is
            // past them (from where it starts, or the first row, to where it ends, or the last)
            let (top, bottom) = (base - gt * f, base - gb * f);
            let (fy, line) = if top < 0.0 || bottom > rows as f32 {
                let (t, b) = (top.max(0.0), bottom.min(rows as f32));
                let fy = (b - t).max(1.0) / (gt - gb);
                (fy, t + gt * fy)
            } else {
                (f, base)
            };
            let scale = PxScale { x: f * units * condense, y: fy * units };
            let Some(outlined) = font.outline_glyph(id.with_scale_and_position(scale, ab_glyph::point(0.0, line))) else { continue };
            if let Some(g) = dots_of_outline(c, &outlined, rows) {
                glyphs.push(g);
            }
        }
        if glyphs.is_empty() {
            return Err("the font has none of the letters".into());
        }
        if capitals {
            add_capitals_for_small(&mut glyphs, chars);
        }
        Ok(DotFont { name: String::new(), rows, gap: band.gap, space: band.space.max(1), glyphs })
    }

    /// `from_vector` itself: the letters fill the rows, from the top of the capitals and tall
    /// letters to the bottom of the descenders.
    fn filling_from_vector(data: &[u8], face: u32, rows: u32, chars: &[char]) -> Result<DotFont, String> {
        use ab_glyph::{Font as _, FontRef, PxScale, ScaleFont as _};
        let font = FontRef::try_from_slice_and_index(data, face).map_err(|e| e.to_string())?;
        let rows = rows.clamp(3, 512);
        let capitals = rows < CAPITALS_BELOW;
        let has = |c: char| font.glyph_id(c).0 != 0;
        // the rows' span in the font's units: the plain letters' and digits' tops and bottoms
        let (mut top, mut bottom) = (0.0f32, 0.0f32);
        let small: Vec<char> = if capitals { Vec::new() } else { ('a'..='z').collect() };
        for c in ('A'..='Z').chain(small).chain('0'..='9') {
            let id = font.glyph_id(c);
            // (an outline's bounds run from its top, `min`, to its bottom, `max`, in units
            // counted upwards)
            if let Some(o) = (id.0 != 0).then(|| font.outline(id)).flatten() {
                top = top.max(o.bounds.min.y);
                bottom = bottom.min(o.bounds.max.y);
            }
        }
        if capitals {
            // (capitals stand on the line: a Q's tail and a J below it are squeezed in)
            bottom = 0.0;
        }
        if top <= bottom {
            (top, bottom) = (font.ascent_unscaled(), font.descent_unscaled());
        }
        let units = font.height_unscaled().max(1.0);
        // (pixels per unit; a `PxScale` is the pixels of the font's whole height)
        let f = rows as f32 / (top - bottom).max(1.0);
        let scale = PxScale::from(f * units);
        let scaled = font.as_scaled(scale);
        let mut glyphs: Vec<DotGlyph> = Vec::new();
        for &c in chars {
            if c.is_whitespace() || glyphs.iter().any(|g| g.ch == c) || (capitals && capital_of(c).is_some_and(has)) {
                continue;
            }
            let id = font.glyph_id(c);
            if id.0 == 0 {
                continue;
            }
            // (a letter taller or deeper than the rows: squeezed upright into them, from its
            // top to its bottom)
            let (gt, gb) = font.outline(id).map(|o| (o.bounds.min.y.max(top), o.bounds.max.y.min(bottom))).unwrap_or((top, bottom));
            let fy = rows as f32 / (gt - gb).max(1.0);
            let glyph_scale = PxScale { x: f * units, y: fy * units };
            let Some(outlined) = font.outline_glyph(id.with_scale_and_position(glyph_scale, ab_glyph::point(0.0, gt * fy))) else { continue };
            if let Some(g) = dots_of_outline(c, &outlined, rows) {
                glyphs.push(g);
            }
        }
        if glyphs.is_empty() {
            return Err("the font has none of the letters".into());
        }
        if capitals {
            add_capitals_for_small(&mut glyphs, chars);
        }
        let space_id = font.glyph_id(' ');
        let space = if space_id.0 != 0 { scaled.h_advance(space_id) } else { rows as f32 / 4.0 };
        // (a dot between letters on a small sign, more on a large one, as the signs' own fonts)
        let gap = ((rows as f32 / 12.0).round() as u32).max(1);
        Ok(DotFont { name: String::new(), rows, gap, space: (space.round() as u32).max(1), glyphs })
    }

    /// The font in `rows` rows: every letter scaled by as much (the share of each dot it
    /// covers averaged), the spacing too. Made smaller than `CAPITALS_BELOW` rows, its capitals
    /// fill them (a letter reaching above or below them squeezed in) and its small letters are
    /// written as capitals.
    pub fn resized(&self, rows: u32) -> DotFont {
        let rows = rows.max(1);
        if rows == self.rows {
            return self.clone();
        }
        if rows < CAPITALS_BELOW && rows < self.rows {
            if let Some(f) = self.capitals_in(rows) {
                return f;
            }
        }
        let f = rows as f32 / self.rows.max(1) as f32;
        let glyphs = self
            .glyphs
            .iter()
            .map(|g| {
                let nw = ((g.w as f32 * f).round() as u32).max(1);
                DotGlyph { ch: g.ch, w: nw, cov: resample_plane(&g.cov, g.w, self.rows, nw, rows) }
            })
            .collect();
        DotFont { name: self.name.clone(), rows, gap: (self.gap as f32 * f).round() as u32, space: ((self.space as f32 * f).round() as u32).max(1), glyphs }
    }

    /// The rows a glyph's dots are lit in (`LIT` and more), first and last.
    fn lit_rows(&self, g: &DotGlyph) -> Option<(u32, u32)> {
        lit_rows_of(g, self.rows)
    }

    /// `resized` to a sign of capitals: the rows of the capitals (an H's) made `rows` rows.
    fn capitals_in(&self, rows: u32) -> Option<DotFont> {
        let (ct, cb) = ['H', 'E', 'I', 'T', 'N'].iter().find_map(|&c| self.glyphs.iter().find(|g| g.ch == c).and_then(|g| self.lit_rows(g)))?;
        let f = rows as f32 / (cb - ct + 1) as f32;
        let mut glyphs: Vec<DotGlyph> = Vec::new();
        for g in &self.glyphs {
            if capital_of(g.ch).is_some_and(|u| self.glyphs.iter().any(|o| o.ch == u)) {
                continue;
            }
            // the capitals' rows, and as much more as the letter reaches (an accent, a comma)
            let (gt, gb) = self.lit_rows(g).unwrap_or((ct, cb));
            let (bt, bb) = (gt.min(ct), gb.max(cb));
            let band: Vec<u8> = g.cov[(bt * g.w) as usize..((bb + 1) * g.w) as usize].to_vec();
            let nw = ((g.w as f32 * f).round() as u32).max(1);
            glyphs.push(DotGlyph { ch: g.ch, w: nw, cov: resample_plane(&band, g.w, bb - bt + 1, nw, rows) });
        }
        let small: Vec<char> = self.glyphs.iter().map(|g| g.ch).collect();
        add_capitals_for_small(&mut glyphs, &small);
        Some(DotFont { name: self.name.clone(), rows, gap: ((self.gap as f32 * f).round() as u32).max(1), space: ((self.space as f32 * f).round() as u32).max(1), glyphs })
    }

    /// The font bold (every stroke a dot wider, to the right) and with `spacing` dots between
    /// letters (none: its own).
    pub fn styled(&self, bold: bool, spacing: Option<u32>) -> DotFont {
        let mut out = self.clone();
        if bold {
            for g in out.glyphs.iter_mut() {
                let (w, nw) = (g.w, g.w + 1);
                let mut cov = vec![0u8; (nw * out.rows) as usize];
                for y in 0..out.rows {
                    for x in 0..nw {
                        let at = |x: u32| if x < w { g.cov[(y * w + x) as usize] } else { 0 };
                        cov[(y * nw + x) as usize] = at(x).max(if x > 0 { at(x - 1) } else { 0 });
                    }
                }
                g.w = nw;
                g.cov = cov;
            }
        }
        if let Some(s) = spacing {
            out.gap = s;
        }
        out
    }

    /// The share of `text`'s letters (not its spaces) the font has.
    pub fn coverage(&self, text: &str) -> f32 {
        let letters: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
        if letters.is_empty() {
            return 1.0;
        }
        letters.iter().filter(|&&c| self.glyph(c).is_some()).count() as f32 / letters.len() as f32
    }

    /// The letter drawn for `c`: its own, else (a small letter the font lacks) its capital.
    pub fn glyph(&self, c: char) -> Option<&DotGlyph> {
        self.glyphs.iter().find(|g| g.ch == c).or_else(|| {
            let up: Vec<char> = c.to_uppercase().collect();
            (up.len() == 1 && up[0] != c).then(|| self.glyphs.iter().find(|g| g.ch == up[0])).flatten()
        })
    }

    /// The font with only the letters among `chars`.
    pub fn only(&self, chars: &[char]) -> DotFont {
        DotFont { glyphs: self.glyphs.iter().filter(|g| chars.contains(&g.ch)).cloned().collect(), ..self.clone() }
    }

    /// The mean width (dots, its ink alone) of the letters among `chars` the font has.
    pub fn mean_width(&self, chars: &[char]) -> Option<f32> {
        let widths: Vec<u32> = chars.iter().filter_map(|&c| self.glyphs.iter().find(|g| g.ch == c)).filter_map(|g| inked_cols(g, self.rows)).map(|(a, b)| b - a + 1).collect();
        (!widths.is_empty()).then(|| widths.iter().sum::<u32>() as f32 / widths.len() as f32)
    }

    /// An `.oft` font's dots (`from_atlas`) as the letters of the font of `band` stand: its
    /// capitals as tall as those, by whole times where they fit twice or more (each dot a
    /// square of dots, crisp), as they are up to that, shrunk where they are taller; standing
    /// on its line (a letter that would reach past its rows squeezed into them, not cut off);
    /// each letter its ink alone, `condense` times as wide (see `condense_for`), the band's
    /// gap between two. On a sign of capitals small letters are written as capitals.
    pub fn fit_to(&self, band: &LetterBand, condense: f32) -> DotFont {
        if band.fill {
            return if self.rows > band.rows { self.resized(band.rows) } else { self.clone() };
        }
        let src = LetterBand::of(self);
        let rows = band.rows.max(1);
        let condense = condense.clamp(0.1, 1.0);
        let r = band.cap_rows().max(1) as f32 / src.cap_rows().max(1) as f32;
        let k = if r >= 2.0 { r.floor() as u32 } else { 1 };
        let shrink = r < 1.0;
        let (scale, scaled_rows) = if shrink { (r, ((self.rows as f32 * r).round() as u32).max(1)) } else { (k as f32, self.rows * k) };
        let off = band.base as i32 - (src.base as f32 * scale).round() as i32;
        let capitals = band.capitals_only || band.cap_rows() + band.below < CAPITALS_BELOW;
        let mut glyphs: Vec<DotGlyph> = Vec::new();
        for g in &self.glyphs {
            if capitals && capital_of(g.ch).is_some_and(|u| self.glyphs.iter().any(|o| o.ch == u)) {
                continue;
            }
            let g = trimmed(g, self.rows);
            let (w, plane) = if k >= 2 {
                // (narrower by whole dots too: a dot k high stays a block of whole dots)
                let kx = ((k as f32 * condense).round() as u32).max(1);
                (g.w * kx, magnified(&g.cov, g.w, self.rows, kx, k))
            } else {
                let nw = ((g.w as f32 * if shrink { r } else { 1.0 } * condense).round() as u32).max(1);
                if nw == g.w && !shrink {
                    (g.w, g.cov.clone())
                } else {
                    (nw, resample_plane(&g.cov, g.w, self.rows, nw, scaled_rows))
                }
            };
            glyphs.push(DotGlyph { ch: g.ch, w, cov: placed(&plane, w, scaled_rows, off, rows) });
        }
        if capitals {
            let small: Vec<char> = self.glyphs.iter().map(|g| g.ch).collect();
            add_capitals_for_small(&mut glyphs, &small);
        }
        DotFont { name: self.name.clone(), rows, gap: band.gap, space: band.space.max(1), glyphs }
    }

    /// The font as `grid`'s display draws it, `line_h` pixels high (a line of the display's
    /// own font): as many of the display's rows as it has - shrunk to them when it has more,
    /// every dot a square of whole dots when it fits that many times - in the middle of the
    /// line, each dot drawn where the display has one; on or off, unless the display's font is
    /// a smooth one. White letters on nothing: the display's colour is laid over them as over
    /// its own font. A dot is on from half its letter's most ink (at most `LIT`), so that a
    /// letter shrunk until no dot of it is half covered still shows; from a quarter for every
    /// letter when that still leaves the font far darker than its ink; and a stroke no dot of
    /// which reaches that lights the dot it covers most (`lit_dots`).
    pub fn to_atlas(&self, grid: &DotGrid, line_h: u32, name: &str) -> FontAtlas {
        let line_h = line_h.max(1);
        let p = grid.pitch.max(1);
        let d = grid.dot.clamp(1, p);
        let n = grid.rows.max(1);
        let font = if self.rows > n { self.resized(n) } else { self.clone() };
        let k = (n / font.rows.max(1)).max(1);
        let top = n.saturating_sub(font.rows * k) / 2;
        let own_threshold = |g: &DotGlyph| (g.cov.iter().copied().max().unwrap_or(0) / 2).clamp(1, LIT);
        let (mut ink, mut on, mut cells) = (0.0f32, 0usize, 0usize);
        for g in &font.glyphs {
            let t = own_threshold(g);
            ink += g.cov.iter().map(|&c| c as f32 / 255.0).sum::<f32>();
            on += g.cov.iter().filter(|&&c| c >= t).count();
            cells += g.cov.len();
        }
        let dark = cells > 0 && (on as f32) < ink * 0.25;
        let threshold = |g: &DotGlyph| if dark { own_threshold(g).min(64) } else { own_threshold(g) };
        // (each letter's dots as the display shows them: its coverage on a smooth display, else
        // on or off - no stroke dropped)
        let shown = |g: &DotGlyph| -> Vec<u8> {
            if grid.smooth {
                g.cov.clone()
            } else {
                lit_dots(&g.cov, g.w, font.rows, threshold(g)).into_iter().map(|on| if on { 255 } else { 0 }).collect()
            }
        };
        // letters side by side in rows of the atlas, a pitch apart (a dot past a letter's last
        // cell does not reach into the next one)
        let mut glyphs: Vec<(char, u32, &DotGlyph)> = font.glyphs.iter().map(|g| (g.ch, g.w * k * p, g)).collect();
        glyphs.sort_by_key(|g| g.0);
        let widest = glyphs.iter().map(|g| g.1).max().unwrap_or(1) + p;
        let atlas_w = widest.max(1024).min(4096).max(widest);
        let mut places: Vec<(u32, u32)> = Vec::with_capacity(glyphs.len());
        let (mut x, mut y) = (0u32, 0u32);
        for g in &glyphs {
            if x + g.1 + p > atlas_w && x > 0 {
                x = 0;
                y += line_h;
            }
            places.push((x, y));
            x += g.1 + p;
        }
        let atlas_h = y + line_h;
        let mut alpha = vec![0u8; (atlas_w * atlas_h * 4) as usize];
        let mut color = vec![0u8; (atlas_w * atlas_h * 4) as usize];
        let mut chars: Vec<FontChar> = Vec::with_capacity(glyphs.len() + 1);
        for (&(ch, w_px, g), &(gx, gy)) in glyphs.iter().zip(&places) {
            chars.push(FontChar { ch, x0: gx as i32, x1: (gx + w_px) as i32, y: gy as i32 });
            let dots = shown(g);
            for j in 0..font.rows {
                for i in 0..g.w {
                    let a = dots[(j * g.w + i) as usize];
                    if a == 0 {
                        continue;
                    }
                    for kj in 0..k {
                        for ki in 0..k {
                            let px = gx + grid.phase_x + (i * k + ki) * p;
                            let py = gy + grid.phase_y + (top + j * k + kj) * p;
                            for yy in py..(py + d).min(gy + line_h) {
                                for xx in px..(px + d).min(atlas_w) {
                                    let at = ((yy * atlas_w + xx) * 4) as usize;
                                    let v = a.max(alpha[at]);
                                    alpha[at..at + 4].copy_from_slice(&[v; 4]);
                                    color[at..at + 4].copy_from_slice(&[255; 4]);
                                }
                            }
                        }
                    }
                }
            }
        }
        // the space: as wide as the font's, nothing in it
        chars.push(FontChar { ch: ' ', x0: 0, x1: (font.space * k * p) as i32, y: atlas_h as i32 });
        let font_def = Font { path: PathBuf::new(), name: name.to_string(), bitmap: String::new(), alpha: String::new(), height: line_h as i32, gap: (font.gap * k * p) as i32, chars };
        // (white where lit, black between: a script that takes the font's own colours lights
        // its LEDs from them)
        FontAtlas::new(font_def, atlas_w, atlas_h, color, alpha)
    }
}

/// The letter `c` of an outline placed in `rows` rows (its line where the outline was
/// positioned): each dot as much as the outline covers of it, the empty columns at either side
/// (its side bearings, which the gap takes over) left out. None for a letter of no ink.
fn dots_of_outline(c: char, outlined: &ab_glyph::OutlinedGlyph, rows: u32) -> Option<DotGlyph> {
    let b = outlined.px_bounds();
    let (w, top) = (b.width().max(0.0) as u32, b.min.y as i32);
    if w == 0 {
        return None;
    }
    let mut cov = vec![0u8; (w * rows) as usize];
    outlined.draw(|x, y, c| {
        let yy = top + y as i32;
        if x < w && yy >= 0 && (yy as u32) < rows {
            let i = (yy as u32 * w + x) as usize;
            cov[i] = cov[i].max((c.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    });
    let used = |x: u32| (0..rows).any(|y| cov[(y * w + x) as usize] >= 100);
    let first = (0..w).find(|&x| used(x))?;
    let last = (0..w).rev().find(|&x| used(x)).unwrap_or(first);
    let cut: Vec<u8> = (0..rows).flat_map(|y| (first..=last).map(move |x| (y, x))).map(|(y, x)| cov[(y * w + x) as usize]).collect();
    Some(DotGlyph { ch: c, w: last - first + 1, cov: cut })
}

/// The small letters among `chars` written as their capitals among `glyphs` (a sign of
/// capitals): each a copy of its capital.
fn add_capitals_for_small(glyphs: &mut Vec<DotGlyph>, chars: &[char]) {
    for &c in chars {
        if glyphs.iter().any(|g| g.ch == c) {
            continue;
        }
        if let Some(cap) = capital_of(c).and_then(|u| glyphs.iter().find(|g| g.ch == u)).cloned() {
            glyphs.push(DotGlyph { ch: c, ..cap });
        }
    }
}

/// A dot is lit from this much coverage on (as a matrix shows it: on or off).
pub const LIT: u8 = 112;

/// The dots a matrix lights of a letter `w` dots wide and `rows` high (`cov` its coverage, row
/// by row): those covered from `t` on; and where a stroke has none such - one or two dots along
/// a row or down a column covered from a quarter of `t` on, none lit: a light face made small,
/// a stroke one dot wide that falls across two, each less than half covered - the one it
/// covers most, so that no stroke drops out of the letter. Not at a stroke's end (lit on one
/// side across it and empty on the other): an arm or a serif grows no dot longer.
fn lit_dots(cov: &[u8], w: u32, rows: u32, t: u8) -> Vec<bool> {
    let at = |x: i32, y: i32| if x < 0 || y < 0 || x >= w as i32 || y >= rows as i32 { 0 } else { cov[(y as u32 * w + x as u32) as usize] };
    let on = |x: i32, y: i32| at(x, y) >= t;
    let floor = (t / 4).max(1);
    let mut out: Vec<bool> = cov.iter().map(|&c| c >= t).collect();
    // along every row (an upright stroke dropped), then down every column (a bar)
    for along_rows in [true, false] {
        let (len, lines) = if along_rows { (w, rows) } else { (rows, w) };
        let cell = |l: u32, k: u32| if along_rows { (k as i32, l as i32) } else { (l as i32, k as i32) };
        // (across the run: the dots either side of it)
        let (ax, ay) = if along_rows { (0, 1) } else { (1, 0) };
        for l in 0..lines {
            let mut k = 0;
            while k < len {
                let start = k;
                while k < len && at(cell(l, k).0, cell(l, k).1) >= floor {
                    k += 1;
                }
                if k == start {
                    k += 1;
                    continue;
                }
                // (a longer run is a faint area, or a bar seen along; one lit - also by the
                // rows' pass - is a stroke that shows)
                let lit = |k: u32| {
                    let (x, y) = cell(l, k);
                    out[(y as u32 * w + x as u32) as usize]
                };
                if k - start > 2 || (start..k).any(lit) {
                    continue;
                }
                let Some(best) = (start..k).max_by_key(|&k| (at(cell(l, k).0, cell(l, k).1), std::cmp::Reverse(k))) else { continue };
                let (x, y) = cell(l, best);
                let ((bx, by), (cx, cy)) = ((x - ax, y - ay), (x + ax, y + ay));
                let end = (on(bx, by) && at(cx, cy) < floor) || (on(cx, cy) && at(bx, by) < floor);
                if !end {
                    out[(y as u32 * w + x as u32) as usize] = true;
                }
            }
        }
    }
    out
}

/// The rows of `rows` a glyph's dots are lit in, first and last.
fn lit_rows_of(g: &DotGlyph, rows: u32) -> Option<(u32, u32)> {
    let lit = |y: u32| (0..g.w).any(|x| g.cov.get((y * g.w + x) as usize).is_some_and(|c| *c >= LIT));
    let first = (0..rows).find(|&y| lit(y))?;
    Some((first, (0..rows).rev().find(|&y| lit(y)).unwrap_or(first)))
}

/// The columns a glyph (of `rows` rows) has lit dots in, first and last.
fn inked_cols(g: &DotGlyph, rows: u32) -> Option<(u32, u32)> {
    let lit = |x: u32| (0..rows).any(|y| g.cov.get((y * g.w + x) as usize).is_some_and(|c| *c >= LIT));
    let first = (0..g.w).find(|&x| lit(x))?;
    Some((first, (0..g.w).rev().find(|&x| lit(x)).unwrap_or(first)))
}

/// A glyph without the empty columns either side of its ink (one of no ink as it is).
fn trimmed(g: &DotGlyph, rows: u32) -> DotGlyph {
    let Some((a, b)) = inked_cols(g, rows) else { return g.clone() };
    if a == 0 && b + 1 == g.w {
        return g.clone();
    }
    DotGlyph { ch: g.ch, w: b - a + 1, cov: (0..rows).flat_map(|y| (a..=b).map(move |x| (y, x))).map(|(y, x)| g.cov[(y * g.w + x) as usize]).collect() }
}

/// A plane of `w` x `h` dots with every dot a block of `kx` x `ky`.
fn magnified(src: &[u8], w: u32, h: u32, kx: u32, ky: u32) -> Vec<u8> {
    let nw = w * kx;
    (0..h * ky).flat_map(|y| (0..nw).map(move |x| src[((y / ky) * w + x / kx) as usize])).collect()
}

/// A plane of `w` x `h` dots moved `off` rows down into a plane of `rows` rows; its ink
/// squeezed upright into them where it would reach past them (from where it starts, or the
/// first row, to where it ends, or the last), not cut off. Ink too faint to light a dot (less
/// than half the letter's most, as `DotFont::to_atlas` lights them) - the soft edge or glow of
/// a smooth font - is cut off instead (squeezing a letter in for it shrank it off the
/// display's line).
fn placed(src: &[u8], w: u32, h: u32, off: i32, rows: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w * rows) as usize];
    let faint = (src.iter().copied().max().unwrap_or(0) / 2).clamp(1, LIT);
    let inked = |y: u32| (0..w).any(|x| src[(y * w + x) as usize] >= faint);
    let Some(first) = (0..h).find(|&y| inked(y)) else { return out };
    let last = (0..h).rev().find(|&y| inked(y)).unwrap_or(first);
    let (top, bottom) = (first as i32 + off, last as i32 + off);
    let row = |y: u32| (y * w) as usize..((y + 1) * w) as usize;
    if top >= 0 && bottom < rows as i32 {
        // (its faint edge too, where it lands in the rows)
        for y in (0..h).filter(|&y| (0..rows as i32).contains(&(y as i32 + off))) {
            out[row((y as i32 + off) as u32)].copy_from_slice(&src[row(y)]);
        }
        return out;
    }
    let t = top.clamp(0, rows as i32 - 1) as u32;
    let b = (bottom.clamp(0, rows as i32 - 1) as u32).max(t);
    let squeezed = resample_plane(&src[(first * w) as usize..((last + 1) * w) as usize], w, last - first + 1, w, b - t + 1);
    out[(t * w) as usize..((b + 1) * w) as usize].copy_from_slice(&squeezed);
    out
}

// --- where a font's letters sit -----------------------------------------------------------------

/// Where the letters of a font of dots sit in its rows and how wide they are: what a display
/// font standing in for it is fitted to (`DotFont::from_vector_in`, `DotFont::fit_to`), so
/// that it is as tall, stands on the same line and is about as wide - a matrix script that
/// measures its text (`TextLength`) to pick a size then picks the size it picks in its own
/// fonts. In dots of the font's grid: its capitals stand from row `cap_top` on the line
/// `base` (the row under them), its descenders reach `below` rows deeper.
#[derive(Debug, Clone, PartialEq)]
pub struct LetterBand {
    pub rows: u32,
    pub cap_top: u32,
    pub base: u32,
    pub below: u32,
    /// It writes capitals: it has fewer than half the small letters.
    pub capitals_only: bool,
    /// The mean width of its capitals (its digits', or any letters', when it has none), its
    /// small letters and its digits: in dots, their ink alone.
    pub cap_w: f32,
    pub small_w: Option<f32>,
    pub digit_w: Option<f32>,
    /// Dots between two letters (its gap, and the empty columns its letters carry beside their
    /// ink), and a space's width.
    pub gap: u32,
    pub space: u32,
    /// No font says where the letters sit (a display whose own font is missing, or one of too
    /// few letters to tell): a font standing in fills the rows, from the top of its tallest
    /// letters to the bottom of its deepest (`DotFont::from_vector`).
    pub fill: bool,
}

impl LetterBand {
    /// Where the letters of `dots` sit: its capitals from the median top to the median bottom
    /// of a flat H, E, I, T, N, L and F (of its digits for a line number's font, else of all
    /// it has), its descenders as deep as its g, j, p, q and y reach. A font of fewer than three
    /// such letters fills its rows.
    pub fn of(dots: &DotFont) -> LetterBand {
        let rows = dots.rows.max(1);
        let find = |c: char| dots.glyphs.iter().find(|g| g.ch == c);
        let spans = |set: &[char]| -> Vec<(u32, u32)> { set.iter().filter_map(|&c| find(c)).filter_map(|g| lit_rows_of(g, rows)).collect() };
        let capitals: Vec<char> = ('A'..='Z').collect();
        let digits: Vec<char> = ('0'..='9').collect();
        let all: Vec<char> = dots.glyphs.iter().map(|g| g.ch).collect();
        let mut letters = spans(&['H', 'E', 'I', 'T', 'N', 'L', 'F']);
        if letters.len() < 3 {
            letters = spans(&digits);
        }
        if letters.len() < 3 {
            letters = spans(&all);
        }
        let cap_w = dots.mean_width(&capitals).or_else(|| dots.mean_width(&digits)).or_else(|| dots.mean_width(&all)).unwrap_or(rows as f32 * 0.6);
        let small: Vec<char> = ('a'..='z').collect();
        // (the empty columns a letter carries beside its ink: part of the space between two)
        let mut sides: Vec<u32> = capitals.iter().filter_map(|&c| find(c)).filter_map(|g| inked_cols(g, rows).map(|(a, b)| g.w - (b - a + 1))).collect();
        if sides.is_empty() {
            sides = dots.glyphs.iter().filter_map(|g| inked_cols(g, rows).map(|(a, b)| g.w - (b - a + 1))).collect();
        }
        sides.sort_unstable();
        let gap = dots.gap + sides.get(sides.len() / 2).copied().unwrap_or(0);
        if letters.len() < 3 {
            return LetterBand { cap_w, gap, space: dots.space.max(1), ..LetterBand::filling(rows) };
        }
        let median = |mut v: Vec<u32>| -> u32 {
            v.sort_unstable();
            v[v.len() / 2]
        };
        let cap_top = median(letters.iter().map(|s| s.0).collect());
        let base = (median(letters.iter().map(|s| s.1).collect()) + 1).clamp(cap_top + 1, rows);
        let capitals_only = small.iter().filter(|&&c| find(c).is_some()).count() < 13;
        let below = if capitals_only { 0 } else { spans(&['g', 'j', 'p', 'q', 'y']).iter().map(|s| (s.1 + 1).saturating_sub(base)).max().unwrap_or(0) };
        LetterBand { rows, cap_top, base, below, capitals_only, cap_w, small_w: dots.mean_width(&small), digit_w: dots.mean_width(&digits), gap, space: dots.space.max(1), fill: false }
    }

    /// The letters of a display whose own font is missing: they fill its `rows`.
    pub fn filling(rows: u32) -> LetterBand {
        let rows = rows.max(1);
        LetterBand { rows, cap_top: 0, base: rows, below: 0, capitals_only: rows < CAPITALS_BELOW, cap_w: rows as f32 * 0.6, small_w: None, digit_w: None, gap: ((rows as f32 / 12.0).round() as u32).max(1), space: (rows / 4).max(1), fill: true }
    }

    /// How many rows its capitals take.
    pub fn cap_rows(&self) -> u32 {
        self.base.saturating_sub(self.cap_top).max(1)
    }

    /// The letters `rows` rows high, capitals and descenders (the player's letter height): as
    /// much larger or smaller each, their widths too, in the middle of where they were (in
    /// the display's rows). Letters that fill the rows fill that many of them.
    pub fn scaled_to(&self, rows: u32) -> LetterBand {
        if self.fill {
            let r = rows.clamp(3, self.rows.max(3));
            return LetterBand { rows: r, base: r, capitals_only: r < CAPITALS_BELOW, ..self.clone() };
        }
        let total = self.cap_rows() + self.below;
        let want = rows.clamp(1, self.rows);
        if want == total {
            return self.clone();
        }
        let f = want as f32 / total as f32;
        let cap = if self.below == 0 { want } else { ((self.cap_rows() as f32 * f).round() as u32).clamp(1, want) };
        let top = (self.cap_top as i32 + (total as i32 - want as i32) / 2).clamp(0, (self.rows - want) as i32) as u32;
        LetterBand { cap_top: top, base: top + cap, below: want - cap, cap_w: self.cap_w * f, small_w: self.small_w.map(|w| w * f), digit_w: self.digit_w.map(|w| w * f), ..self.clone() }
    }
}

/// The share of `text`'s letters (not its spaces) face `face` of the TrueType/OpenType font
/// `data` has, a small letter as its capital where it has none (none of a file that is no
/// font).
pub fn vector_coverage(data: &[u8], face: u32, text: &str) -> f32 {
    use ab_glyph::Font as _;
    let Ok(font) = ab_glyph::FontRef::try_from_slice_and_index(data, face) else { return 0.0 };
    let letters: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    if letters.is_empty() {
        return 1.0;
    }
    let has = |c: char| font.glyph_id(c).0 != 0;
    letters.iter().filter(|&&c| has(c) || capital_of(c).is_some_and(has)).count() as f32 / letters.len() as f32
}

/// How much narrower a display font is drawn to be about as wide as the letters it stands in
/// for: `natural` the mean width of its letters, `target` of theirs. As it is when it is no more
/// than a tenth wider (a font keeps its own shape where it can), else narrowed to that width -
/// but to no less than half its own.
pub fn condense_for(natural: f32, target: f32) -> f32 {
    if !(natural > 0.0 && target > 0.0) || natural <= target * 1.1 {
        return 1.0;
    }
    (target / natural).clamp(0.5, 1.0)
}

/// Whether a display font standing in for a display's own writes `c`: letters, digits and the
/// punctuation of destinations. Everything else - the braces and bars a matrix script makes its
/// pictograms of, `#`, signs, boxes, private characters - stays as the display's own font draws
/// it (or does not: a character a script leaves out on purpose).
pub fn is_text_char(c: char) -> bool {
    c.is_alphanumeric() || ".,-/()&'’\":+!?".contains(c)
}

/// The characters of a display's own font - `dots` (`DotFont::from_atlas`), its letters sitting
/// as `band` says - that it draws as something else than the letter or mark they are coded as
/// (its pictograms): a display font standing in for it leaves them as it draws them
/// (`merge_kept`). A small letter a to z of a font of capitals: Omsi.exe writes those as their
/// capitals, so a font of capitals that has some draws pictures there (the SD200's Annax Large:
/// a wheelchair over `g` and `h`, a checked flag over `x`; the line number fonts' symbols). A
/// character lit from the line's first row to its last where the letters leave rows free above
/// and below, as no letter is (the Hamburg LED fonts' `í`: a column of every LED, the display's
/// test). A `/` lit right across its width in a row, as no slash is (the Annax depot's roof,
/// drawn over `/^\`, standing on a bar; the Krüger font's framed sign).
pub fn pictograms_of(dots: &DotFont, band: &LetterBand) -> Vec<char> {
    if band.fill {
        return Vec::new();
    }
    let rows = dots.rows;
    // (the first row under the descenders, or under the capitals of a font of no descenders)
    let deepest = band.base + band.below;
    let whole_line = |g: &DotGlyph| band.cap_top > 0 && deepest < rows && lit_rows_of(g, rows) == Some((0, rows - 1));
    let mut out: Vec<char> = dots
        .glyphs
        .iter()
        .filter(|g| is_text_char(g.ch))
        .filter(|g| (band.capitals_only && g.ch.is_ascii_lowercase()) || whole_line(g) || (g.ch == '/' && lit_across(g, rows)))
        .map(|g| g.ch)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Whether a row of `g` (`rows` rows) is lit right across its ink, three dots or more wide: a
/// bar, not a stroke that slants.
fn lit_across(g: &DotGlyph, rows: u32) -> bool {
    let Some((a, b)) = inked_cols(g, rows) else { return false };
    b - a >= 2 && (0..rows).any(|y| (a..=b).all(|x| g.cov[(y * g.w + x) as usize] >= LIT))
}

/// The characters a display font standing in for `font` is made for: every letter, digit and
/// mark of text (`is_text_char`) `font` draws - its own, a small letter as its capital, the same
/// byte of another code page - and the small letters of its capitals.
pub fn text_chars_of(font: &Font) -> Vec<char> {
    let mut all = charset();
    all.extend(font.chars.iter().map(|g| g.ch));
    all.sort_unstable();
    all.dedup();
    all.into_iter().filter(|&c| is_text_char(c) && (font.glyph(c).is_some() || capital_of(c).is_some_and(|u| font.has_glyph(u)))).collect()
}

/// A display font's atlas `sub` (made for the display of the font `own`: `DotFont::to_atlas`
/// of its line) with what is no text drawn as `own` draws it. Its characters are `own`'s: a
/// letter, digit or mark of text (`is_text_char`) in `sub`'s drawing where `sub` has it with
/// ink - a small letter `sub` has none of as `sub`'s capital, as Omsi.exe writes a font of
/// capitals, so that a line does not come out in two fonts - anything else, and `own`'s
/// pictograms among them (`kept`, see `pictograms_of`), with `own`'s pixels and colours; and the
/// characters `own` draws through others (a small letter as its capital, a letter of another
/// code page) in `sub`'s too. Nothing `own` lacks is added, so that what a script leaves out on
/// purpose (the `#` a line number is padded with) stays out. A space is as wide as `own`'s; the
/// letters stand `sub`'s gap apart.
pub fn merge_kept(sub: &FontAtlas, own: &FontAtlas, kept: &[char]) -> FontAtlas {
    let h = own.font.height.max(1) as u32;
    let inked = |a: &FontAtlas, g: &FontChar| (0..h as i32).any(|y| (g.x0..g.x1).any(|x| alpha_at(a, x, g.y + y) > 0));
    let in_sub = |c: char| sub.font.chars.iter().find(|g| g.ch == c && g.x1 > g.x0 && inked(sub, g));
    // (`sub`'s drawing of a letter: its own, else a small letter's capital)
    let drawn_in_sub = |c: char| in_sub(c).or_else(|| capital_of(c).and_then(in_sub));
    // (the character, whether it is `sub`'s, its glyph there)
    let mut picks: Vec<(char, bool, FontChar)> = Vec::new();
    for g in own.font.chars.iter().filter(|g| !g.ch.is_whitespace()) {
        if picks.iter().any(|p| p.0 == g.ch) {
            continue;
        }
        match drawn_in_sub(g.ch).filter(|_| is_text_char(g.ch) && !kept.contains(&g.ch)) {
            Some(s) => picks.push((g.ch, true, s.clone())),
            None => picks.push((g.ch, false, g.clone())),
        }
    }
    let drawn_by_own = |c: char| own.font.glyph(c).is_some() || capital_of(c).is_some_and(|u| own.font.has_glyph(u));
    for s in sub.font.chars.iter().filter(|s| !s.ch.is_whitespace() && is_text_char(s.ch) && drawn_by_own(s.ch)) {
        if !picks.iter().any(|p| p.0 == s.ch) {
            if let Some(s) = in_sub(s.ch) {
                picks.push((s.ch, true, s.clone()));
            }
        }
    }
    for c in charset().into_iter().filter(|c| c.is_lowercase()) {
        let Some(u) = capital_of(c) else { continue };
        if picks.iter().any(|p| p.0 == c) || own.font.has_glyph(c) || !own.font.has_glyph(u) {
            continue;
        }
        if let Some(cap) = in_sub(u) {
            picks.push((c, true, cap.clone()));
        }
    }
    picks.sort_by_key(|p| p.0);
    // laid side by side in rows of the atlas, a pixel apart
    let width = |g: &FontChar| (g.x1 - g.x0).max(0) as u32;
    let widest = picks.iter().map(|p| width(&p.2)).max().unwrap_or(1) + 1;
    let atlas_w = widest.max(1024).min(4096).max(widest);
    let mut places: Vec<(u32, u32)> = Vec::with_capacity(picks.len());
    let (mut x, mut y) = (0u32, 0u32);
    for p in &picks {
        if x + width(&p.2) + 1 > atlas_w && x > 0 {
            x = 0;
            y += h;
        }
        places.push((x, y));
        x += width(&p.2) + 1;
    }
    let atlas_h = y + h;
    let mut alpha = vec![0u8; (atlas_w * atlas_h * 4) as usize];
    let mut color = vec![0u8; (atlas_w * atlas_h * 4) as usize];
    let mut chars: Vec<FontChar> = Vec::with_capacity(picks.len() + 1);
    for ((ch, from_sub, g), &(gx, gy)) in picks.iter().zip(&places) {
        let src = if *from_sub { sub } else { own };
        let w = width(g);
        chars.push(FontChar { ch: *ch, x0: gx as i32, x1: (gx + w) as i32, y: gy as i32 });
        for yy in 0..h {
            for xx in 0..w {
                let (sx, sy) = (g.x0 + xx as i32, g.y + yy as i32);
                if sx < 0 || sy < 0 || sx >= src.width as i32 || sy >= src.height as i32 {
                    continue;
                }
                let si = ((sy as u32 * src.width + sx as u32) * 4) as usize;
                let di = (((gy + yy) * atlas_w + gx + xx) * 4) as usize;
                if let Some(a) = src.alpha.get(si..si + 4) {
                    alpha[di..di + 4].copy_from_slice(a);
                }
                if let Some(c) = src.color.get(si..si + 4) {
                    color[di..di + 4].copy_from_slice(c);
                }
            }
        }
    }
    chars.push(FontChar { ch: ' ', x0: 0, x1: own.font.space_width(), y: atlas_h as i32 });
    let font = Font { path: PathBuf::new(), name: sub.font.name.clone(), bitmap: String::new(), alpha: String::new(), height: h as i32, gap: sub.font.gap, chars };
    FontAtlas::new(font, atlas_w, atlas_h, color, alpha)
}

/// A plane of coverage `sw` x `sh` scaled to `dw` x `dh`, every new cell the average of the
/// part of the old ones under it.
fn resample_plane(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dw * dh) as usize];
    if sw == 0 || sh == 0 {
        return out;
    }
    let (fx, fy) = (sw as f32 / dw as f32, sh as f32 / dh as f32);
    let span = |a: f32, b: f32, n: u32| -> Vec<(usize, f32)> {
        let mut v = Vec::new();
        let mut i = a.floor().max(0.0) as i64;
        while (i as f32) < b && i < n as i64 {
            let w = b.min(i as f32 + 1.0) - a.max(i as f32);
            if w > 0.0 {
                v.push((i as usize, w));
            }
            i += 1;
        }
        v
    };
    for y in 0..dh {
        let rows = span(y as f32 * fy, (y + 1) as f32 * fy, sh);
        for x in 0..dw {
            let cols = span(x as f32 * fx, (x + 1) as f32 * fx, sw);
            let (mut sum, mut total) = (0.0f32, 0.0f32);
            for &(sy, wy) in &rows {
                for &(sx, wx) in &cols {
                    sum += src[sy * sw as usize + sx] as f32 * wx * wy;
                    total += wx * wy;
                }
            }
            if total > 0.0 {
                out[(y * dw + x) as usize] = (sum / total).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

// --- TrueType and OpenType files --------------------------------------------------------------

/// One face of a TrueType/OpenType file: the file, its place in a collection, and its names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorFace {
    pub path: PathBuf,
    pub face: u32,
    pub family: String,
    pub style: String,
    /// Its full name ("Arial Bold"): what the player picks it by.
    pub name: String,
}

impl VectorFace {
    /// The choice of this face, as it is.
    pub fn spec(&self) -> DisplayFontSpec {
        DisplayFontSpec::vector(&self.name, &self.path, self.face)
    }
}

/// Whether `path` is a TrueType/OpenType font file (or a collection of them).
pub fn is_vector_font(path: &Path) -> bool {
    path.extension().and_then(|x| x.to_str()).is_some_and(|x| ["ttf", "otf", "ttc", "otc"].contains(&x.to_ascii_lowercase().as_str()))
}

/// The faces of the font file `path` and their names - read from its tables of names alone,
/// not the whole file (a list of every font of the system reads only those). None of a file
/// that is no font.
pub fn vector_faces(path: &Path) -> Vec<VectorFace> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else {
        // (a font in an archive of the content: the whole file through the content's VFS)
        return omsi_cfg::vfs::read(path).map(|d| faces_in(path, &mut |off, len| d.get(off as usize..off as usize + len).map(<[u8]>::to_vec))).unwrap_or_default();
    };
    let mut read = |off: u64, len: usize| -> Option<Vec<u8>> {
        file.seek(SeekFrom::Start(off)).ok()?;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf).ok()?;
        Some(buf)
    };
    faces_in(path, &mut read)
}

/// `vector_faces` of a font whose bytes `read` gives (offset, length).
fn faces_in(path: &Path, read: &mut dyn FnMut(u64, usize) -> Option<Vec<u8>>) -> Vec<VectorFace> {
    let be32 = |b: &[u8], i: usize| b.get(i..i + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]));
    let Some(head) = read(0, 12) else { return Vec::new() };
    let offsets: Vec<u64> = if &head[0..4] == b"ttcf" {
        let n = be32(&head, 8).unwrap_or(0).min(64) as usize;
        let Some(list) = read(12, n * 4) else { return Vec::new() };
        (0..n).filter_map(|i| be32(&list, i * 4).map(u64::from)).collect()
    } else {
        vec![0]
    };
    offsets.iter().enumerate().filter_map(|(face, &off)| face_names(read, off).map(|(family, style, name)| VectorFace { path: path.to_path_buf(), face: face as u32, family, style, name })).collect()
}

/// The family, style and full name of the face whose table directory is at `off`.
fn face_names(read: &mut dyn FnMut(u64, usize) -> Option<Vec<u8>>, off: u64) -> Option<(String, String, String)> {
    let be16 = |b: &[u8], i: usize| b.get(i..i + 2).map(|s| u16::from_be_bytes([s[0], s[1]]) as usize);
    let be32 = |b: &[u8], i: usize| b.get(i..i + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]) as u64);
    let dir = read(off, 12)?;
    let tag = &dir[0..4];
    if !(tag == [0, 1, 0, 0] || tag == b"OTTO" || tag == b"true") {
        return None;
    }
    let n = be16(&dir, 4)?.min(256);
    let tables = read(off + 12, n * 16)?;
    let (name_off, name_len) = (0..n).find(|i| &tables[i * 16..i * 16 + 4] == b"name").map(|i| (be32(&tables, i * 16 + 8), be32(&tables, i * 16 + 12)))?;
    let (name_off, name_len) = (name_off?, (name_len? as usize).min(1 << 20));
    let t = read(name_off, name_len)?;
    let count = be16(&t, 2)?;
    let strings = be16(&t, 4)?;
    // (each name in the best language there is: Windows' English, any of Windows', the Mac's)
    let mut best: [(u32, String); 18] = Default::default();
    for r in 0..count {
        let at = 6 + r * 12;
        let (Some(platform), Some(encoding), Some(lang), Some(id), Some(len), Some(o)) = (be16(&t, at), be16(&t, at + 2), be16(&t, at + 4), be16(&t, at + 6), be16(&t, at + 8), be16(&t, at + 10)) else { break };
        if id >= best.len() {
            continue;
        }
        let Some(bytes) = t.get(strings + o..strings + o + len) else { continue };
        let (rank, text) = match (platform, encoding) {
            (3, 0 | 1 | 10) => (if lang == 0x409 { 3 } else { 2 }, String::from_utf16_lossy(&bytes.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<u16>>())),
            (0, _) => (2, String::from_utf16_lossy(&bytes.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<u16>>())),
            (1, 0) => (1, bytes.iter().map(|&b| b as char).collect()),
            _ => continue,
        };
        let text = text.trim().trim_matches('\0').to_string();
        if !text.is_empty() && rank > best[id].0 {
            best[id] = (rank, text);
        }
    }
    let pick = |a: usize, b: usize| if best[a].1.is_empty() { best[b].1.clone() } else { best[a].1.clone() };
    let family = pick(16, 1);
    let style = pick(17, 2);
    let name = if best[4].1.is_empty() { format!("{family} {style}").trim().to_string() } else { best[4].1.clone() };
    (!name.is_empty()).then_some((family, style, name))
}

/// The folders the system keeps its fonts in (those that exist): Windows' own and the
/// user's, the Mac's, Linux's, Android's.
pub fn system_font_dirs() -> Vec<PathBuf> {
    let var = |n: &str| omsi_cfg::env::var_os(n).map(PathBuf::from);
    let mut dirs: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        dirs.push(var("SystemRoot").or_else(|| var("WINDIR")).unwrap_or_else(|| PathBuf::from("C:\\Windows")).join("Fonts"));
        if let Some(l) = var("LOCALAPPDATA") {
            dirs.push(l.join("Microsoft").join("Windows").join("Fonts"));
        }
    } else if cfg!(target_os = "android") {
        dirs.push(PathBuf::from("/system/fonts"));
    } else if cfg!(target_os = "macos") {
        dirs.extend(["/System/Library/Fonts", "/Library/Fonts"].map(PathBuf::from));
        if let Some(h) = var("HOME") {
            dirs.push(h.join("Library").join("Fonts"));
        }
    } else {
        dirs.extend(["/usr/share/fonts", "/usr/local/share/fonts"].map(PathBuf::from));
        if let Some(h) = var("HOME") {
            dirs.push(h.join(".local").join("share").join("fonts"));
            dirs.push(h.join(".fonts"));
        }
    }
    dirs.retain(|d| d.is_dir());
    dirs
}

/// The TrueType/OpenType files in `dir` and the folders under it (three deep), by name.
pub fn vector_font_files(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<(PathBuf, bool)> = match std::fs::read_dir(dir) {
            Ok(rd) => rd.flatten().map(|e| (e.path(), e.file_type().map(|t| t.is_dir()).unwrap_or(false))).collect(),
            // (a content folder's archive: listed through the content's VFS)
            Err(_) => omsi_cfg::vfs::list_dir(dir).unwrap_or_default().into_iter().map(|(n, d)| (dir.join(n), d)).collect(),
        };
        entries.sort_by_key(|e| e.0.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default());
        for (p, is_dir) in entries {
            if is_dir {
                if depth < 3 {
                    walk(&p, depth + 1, out);
                }
            } else if is_vector_font(&p) {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, 0, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HANKEN: &[u8] = include_bytes!("../../../assets/fonts/HankenGrotesk/HankenGrotesk-latin-700.ttf");

    /// The rows and columns of a glyph's lit dots in an atlas, from its corner.
    fn lit(a: &FontAtlas, ch: char) -> Vec<(u32, u32)> {
        let g = a.font.chars.iter().find(|g| g.ch == ch).unwrap();
        let mut out = Vec::new();
        for y in 0..a.font.height as u32 {
            for x in 0..(g.x1 - g.x0) as u32 {
                let i = (((g.y as u32 + y) * a.width + g.x0 as u32 + x) * 4 + 3) as usize;
                if a.alpha[i] > 0 {
                    out.push((x, y));
                }
            }
        }
        out
    }

    #[test]
    fn a_choice_goes_to_the_game_and_back() {
        assert_eq!(DisplayFontSpec::parse("  "), None);
        let old = DisplayFontSpec::parse(" Annax Small ").unwrap();
        assert_eq!(old, DisplayFontSpec::named("Annax Small"));
        assert_eq!(old.to_arg(), "Annax Small", "a font alone is written as before");
        let mut s = DisplayFontSpec::vector("Arial Bold", Path::new("C:\\Windows\\Fonts\\arialbd.ttf"), 0);
        s.rows = Some(16);
        s.bold = true;
        s.spacing = Some(2);
        assert_eq!(s.to_arg(), "Arial Bold|file=C:\\Windows\\Fonts\\arialbd.ttf|rows=16|bold|spacing=2");
        assert_eq!(DisplayFontSpec::parse(&s.to_arg()), Some(s.clone()));
        let ttc = DisplayFontSpec::vector("Cambria Math", Path::new("/f/cambria.ttc"), 1);
        assert_eq!(DisplayFontSpec::parse(&ttc.to_arg()), Some(ttc.clone()));
        assert!(s.same_font(&DisplayFontSpec::vector("arial bold", Path::new("c:/windows/fonts/ARIALBD.ttf"), 0)));
        assert!(!s.same_font(&ttc));
        assert_eq!(s.plain(), DisplayFontSpec::vector("Arial Bold", Path::new("C:\\Windows\\Fonts\\arialbd.ttf"), 0));
        // (what a later version may add is left alone)
        assert_eq!(DisplayFontSpec::parse("X|colour=red|rows=x"), Some(DisplayFontSpec::named("X")));
    }

    /// A font of `w` x `h` letters 'A' and 'B' whose every dot is drawn `dot` pixels square
    /// in cells of `pitch`, `phase` pixels into the cell; the letters are 'A' a frame, 'B' a
    /// diagonal.
    fn matrix_font(cols: u32, rows: u32, pitch: u32, dot: u32, phase: u32) -> FontAtlas {
        let (gw, h) = (cols * pitch, rows * pitch + phase);
        let aw = gw * 2;
        let mut alpha = vec![0u8; (aw * h * 4) as usize];
        let mut set = |gx: u32, i: u32, j: u32| {
            for y in 0..dot {
                for x in 0..dot {
                    let (px, py) = (gx + phase + i * pitch + x, phase + j * pitch + y);
                    let at = ((py * aw + px) * 4) as usize;
                    alpha[at..at + 4].copy_from_slice(&[255; 4]);
                }
            }
        };
        for j in 0..rows {
            for i in 0..cols {
                if i == 0 || j == 0 || i == cols - 1 || j == rows - 1 {
                    set(0, i, j);
                }
                if i == j * cols / rows {
                    set(gw, i, j);
                }
            }
        }
        let font = Font { name: "Matrix".into(), height: h as i32, gap: pitch as i32, chars: vec![FontChar { ch: 'A', x0: 0, x1: gw as i32, y: 0 }, FontChar { ch: 'B', x0: gw as i32, x1: aw as i32, y: 0 }], ..Default::default() };
        FontAtlas::new(font, aw, h, alpha.clone(), alpha)
    }

    #[test]
    fn a_font_drawn_as_a_matrix_is_read_in_its_own_grid() {
        // the Annax fonts: 3x3 dots in 4 px cells, a pixel into the cell; 7 rows
        let annax = matrix_font(5, 7, 4, 3, 1);
        let g = DotGrid::of_atlas(&annax);
        assert_eq!((g.pitch, g.dot, g.phase_x, g.phase_y, g.rows, g.smooth), (4, 3, 1, 1, 7, false));
        let dots = DotFont::from_atlas(&annax, &g);
        assert_eq!(dots.rows, 7);
        let a = dots.glyph('A').unwrap();
        assert_eq!(a.w, 5);
        assert_eq!(a.cov.iter().filter(|c| **c > 0).count(), 2 * 5 + 2 * 5);
        // a pixel font magnified four times, no seams: its pixels are its dots
        let big = matrix_font(6, 8, 4, 4, 0);
        let g = DotGrid::of_atlas(&big);
        assert_eq!((g.pitch, g.dot, g.rows), (4, 4, 8));
        assert_eq!(DotFont::from_atlas(&big, &g).glyph('A').unwrap().w, 6);
        // a pixel font as it is
        let plain = matrix_font(6, 8, 1, 1, 0);
        assert_eq!(DotGrid::of_atlas(&plain), DotGrid::plain(8));
    }

    #[test]
    fn a_vector_font_becomes_dots_at_the_rows_asked_for() {
        let f = DotFont::from_vector(HANKEN, 0, 16, &charset()).unwrap();
        assert_eq!(f.rows, 16);
        let h = f.glyph('H').unwrap();
        // an H: two upright strokes and a bar, as tall as a capital is in 16 rows
        let on = |x: u32, y: u32| h.cov[(y * h.w + x) as usize] >= 112;
        let rows_lit: Vec<u32> = (0..16).filter(|&y| (0..h.w).any(|x| on(x, y))).collect();
        assert!(rows_lit.len() >= 10 && rows_lit.len() <= 14 && rows_lit[0] == 0, "{rows_lit:?}");
        // a descender reaches the last row; a capital with an accent keeps it, in the rows
        let g = f.glyph('g').unwrap();
        assert!((0..g.w).any(|x| g.cov[(15 * g.w + x) as usize] >= 112));
        let u = f.glyph('Ü').unwrap();
        let first = (0..16).find(|&y| (0..u.w).any(|x| u.cov[(y * u.w + x) as usize] >= 112)).unwrap();
        let gap_row = (first..16).find(|&y| (0..u.w).all(|x| u.cov[(y * u.w + x) as usize] < 112));
        assert!(gap_row.is_some_and(|r| r < 6), "the dots above the U, apart from it");
        assert!(rows_lit.iter().any(|&y| (0..h.w).all(|x| on(x, y))), "the bar");
        let top = rows_lit[0];
        assert!((0..h.w / 3).any(|x| on(x, top)) && (2 * h.w / 3..h.w).any(|x| on(x, top)), "a stroke at either side");
        assert!(!on(h.w / 2, top), "and nothing between them at the top");
        // letters it has, small ones as their capitals where it has none, a gap of a dot
        assert!(f.coverage("12 Hauptbahnhof") > 0.99);
        assert_eq!(f.gap, 1);
        assert!(f.space >= 2);
        // more rows: larger letters
        let big = DotFont::from_vector(HANKEN, 0, 32, &['H']).unwrap();
        assert!(big.glyph('H').unwrap().w > h.w);
        // a sign of seven rows writes capitals: an H fills them, small letters are capitals
        let seven = DotFont::from_vector(HANKEN, 0, 7, &['H', 'a', 'A', 'ü', 'Ü', 'g', 'G']).unwrap();
        let h7 = seven.glyph('H').unwrap();
        assert_eq!(seven.lit_rows(h7), Some((0, 6)));
        assert_eq!(seven.glyph('a').unwrap().cov, seven.glyph('A').unwrap().cov);
        assert_eq!(seven.glyph('ü').unwrap().cov, seven.glyph('Ü').unwrap().cov);
        // and a font of more rows made smaller than that: the same
        let shrunk = f.resized(7);
        assert_eq!(shrunk.rows, 7);
        assert_eq!(shrunk.lit_rows(shrunk.glyph('H').unwrap()), Some((0, 6)));
        assert_eq!(shrunk.glyph('g').unwrap().cov, shrunk.glyph('G').unwrap().cov);
        assert!(DotFont::from_vector(b"no font", 0, 16, &['H']).is_err());
    }

    #[test]
    fn a_vector_font_on_a_display_is_drawn_in_its_dots() {
        let f = DotFont::from_vector(HANKEN, 0, 7, &['E', 'I']).unwrap();
        // an Annax display: 7 rows of 3x3 dots in 4 px cells, a line of 30 px
        let grid = DotGrid { pitch: 4, dot: 3, phase_x: 1, phase_y: 1, rows: 7, smooth: false };
        let a = f.to_atlas(&grid, 30, "Hanken");
        assert_eq!(a.font.height, 30);
        let e = lit(&a, 'E');
        assert!(!e.is_empty());
        // every lit pixel is a dot's pixel (none in a seam), each fully lit
        assert!(e.iter().all(|&(x, y)| x % 4 != 0 && y % 4 != 0 && y < 29), "{e:?}");
        let g = a.font.chars.iter().find(|g| g.ch == 'E').unwrap();
        assert!(e.iter().all(|&(x, y)| a.alpha[(((g.y as u32 + y) * a.width + g.x0 as u32 + x) * 4 + 3) as usize] == 255));
        // the same dots as the font has, three by three
        let dots: usize = f.glyph('E').unwrap().cov.iter().filter(|c| **c >= 112).count();
        assert_eq!(e.len(), dots * 9);
        // a matrix of 14 rows whose pixels are its LEDs: the 7 rows twice
        let m = f.to_atlas(&DotGrid::plain(14), 14, "Hanken");
        assert_eq!(lit(&m, 'E').len(), dots * 4);
        // a font of more rows than the display: shrunk to them
        let tall = DotFont::from_vector(HANKEN, 0, 32, &['I']).unwrap();
        let small = tall.to_atlas(&DotGrid::plain(8), 8, "x");
        assert!(lit(&small, 'I').iter().all(|&(_, y)| y < 8));
        // bold: a dot wider; spacing: as asked
        let bold = f.styled(true, Some(3));
        assert_eq!(bold.glyph('I').unwrap().w, f.glyph('I').unwrap().w + 1);
        assert_eq!(bold.gap, 3);
        assert_eq!(bold.to_atlas(&grid, 30, "x").font.gap, 12);
    }

    /// Five letters of 5 x 7 dots ('#' lit).
    const LETTERS: [(char, [&str; 7]); 5] = [
        ('H', ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"]),
        ('E', ["#####", "#....", "#....", "####.", "#....", "#....", "#####"]),
        ('L', ["#....", "#....", "#....", "#....", "#....", "#....", "#####"]),
        ('T', ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."]),
        ('O', [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."]),
    ];

    /// A font drawn as a picture of a matrix: the dots of `LETTERS` on one grid of `pitch` px
    /// over the whole bitmap, each dot `draw` (its pixels, from its cell's corner, and their
    /// alpha); letter `i` in cells from column `i * 6` of row 1, its box cut out of the bitmap
    /// from `x0(i)` and `y(i)` (its dots at another place of their cells than the next one's,
    /// as the stock LED fonts' boxes are cut), `bw` wide and `h` high.
    fn led_picture(pitch: u32, bw: u32, h: u32, draw: &[(u32, u32, u8)], x0: impl Fn(u32) -> u32, y: impl Fn(u32) -> u32) -> FontAtlas {
        let n = LETTERS.len() as u32;
        let (aw, ah) = (pitch * 6 * n + 2 * pitch, pitch + h + 2 * pitch);
        let mut alpha = vec![0u8; (aw * ah * 4) as usize];
        let mut chars = Vec::new();
        for (i, (ch, rows)) in LETTERS.iter().enumerate() {
            let i = i as u32;
            for (r, row) in rows.iter().enumerate() {
                for (c, b) in row.bytes().enumerate() {
                    if b != b'#' {
                        continue;
                    }
                    let (cx, cy) = ((i * 6 + c as u32) * pitch, (1 + r as u32) * pitch);
                    for &(dx, dy, a) in draw {
                        let at = (((cy + dy) * aw + cx + dx) * 4) as usize;
                        let v = alpha[at].max(a);
                        alpha[at..at + 4].copy_from_slice(&[v; 4]);
                    }
                }
            }
            chars.push(FontChar { ch: *ch, x0: x0(i) as i32, x1: (x0(i) + bw) as i32, y: y(i) as i32 });
        }
        let font = Font { name: "LED".into(), height: h as i32, gap: 0, chars, ..Default::default() };
        FontAtlas::new(font, aw, ah, alpha.clone(), alpha)
    }

    /// Whether the letter `ch` of `dots` is lit as `LETTERS` draws it, from row `top`.
    fn reads_as_drawn(dots: &DotFont, ch: char, top: u32) -> bool {
        let g = dots.glyph(ch).unwrap();
        let rows = LETTERS.iter().find(|l| l.0 == ch).unwrap().1;
        (0..dots.rows).all(|y| {
            (0..g.w).all(|x| {
                let want = y >= top && y < top + 7 && rows[(y - top) as usize].as_bytes().get(x as usize) == Some(&b'#');
                (g.cov[(y * g.w + x) as usize] >= LIT) == want
            })
        })
    }

    /// The Hamburg LED fonts (`4_LW_Benefit_*`): round LEDs - a 2x2 core in a halo - in cells
    /// of 4 px, the letters' boxes cut from the bitmap a pixel into a cell and at two places
    /// of the cell's rows. Read in their grid all the same, each letter at its own phase.
    #[test]
    fn round_leds_are_read_in_their_grid() {
        let mut led: Vec<(u32, u32, u8)> = (0..4).flat_map(|y| (0..4).map(move |x| (x, y, 60u8))).collect();
        led.extend([(1, 1, 255), (2, 1, 255), (1, 2, 255), (2, 2, 255)]);
        // (boxes from x ≡ 1 of the cells, their tops at y ≡ 0 and ≡ 2 of them)
        let a = led_picture(4, 20, 30, &led, |i| i * 24 + 1, |i| if i % 2 == 0 { 4 } else { 2 });
        let g = DotGrid::of_atlas(&a);
        assert_eq!((g.pitch, g.dot, g.rows, g.smooth), (4, 2, 7, false), "{g:?}");
        let dots = DotFont::from_atlas(&a, &g);
        for (ch, _) in LETTERS {
            assert!(reads_as_drawn(&dots, ch, 0), "{ch}");
        }
        // the L of three pixels of `17_LED_Gross`, its corner left dark: the same
        let l: Vec<(u32, u32, u8)> = vec![(1, 1, 255), (1, 2, 255), (2, 2, 255)];
        let a = led_picture(4, 20, 30, &l, |i| i * 24 + 1, |i| if i % 3 == 0 { 2 } else { 4 });
        let g = DotGrid::of_atlas(&a);
        assert_eq!((g.pitch, g.dot), (4, 2), "{g:?}");
        let dots = DotFont::from_atlas(&a, &g);
        assert!(LETTERS.iter().all(|(ch, _)| reads_as_drawn(&dots, *ch, 0)));
    }

    /// `krueger-font_linie`: 2 px dots on one 3 px grid over the bitmap, the letters' boxes cut
    /// out of it from any pixel of a cell.
    #[test]
    fn glyphs_cut_off_the_grid() {
        let square: Vec<(u32, u32, u8)> = vec![(0, 0, 255), (1, 0, 255), (0, 1, 255), (1, 1, 255)];
        let a = led_picture(3, 17, 24, &square, |i| i * 18 - i % 3, |_| 3);
        let g = DotGrid::of_atlas(&a);
        assert_eq!((g.pitch, g.dot), (3, 2), "{g:?}");
        let dots = DotFont::from_atlas(&a, &g);
        assert!(LETTERS.iter().all(|(ch, _)| reads_as_drawn(&dots, *ch, 0)));
    }

    /// A picture of LEDs read as a plain grid (every pixel a dot: rows of dots and dark seams)
    /// and shrunk onto a matrix of few rows still lights every letter - on no dot half covered.
    #[test]
    fn a_shrunk_picture_never_goes_dark() {
        let led: Vec<(u32, u32, u8)> = vec![(1, 1, 255), (2, 1, 255), (1, 2, 255), (2, 2, 255)];
        let a = led_picture(4, 20, 30, &led, |i| i * 24 + 1, |_| 4);
        let dots = DotFont::from_atlas(&a, &DotGrid::plain(30));
        assert_eq!(dots.rows, 30);
        for rows in [7, 16] {
            let small = dots.to_atlas(&DotGrid::plain(rows), rows, "x");
            for (ch, _) in LETTERS {
                assert!(!lit(&small, ch).is_empty(), "{ch} on {rows} rows");
            }
        }
    }

    /// A stroke a dot wide that falls across two columns or two rows, each less than half
    /// covered, is lit in one of them (it dropped out of the letter); the half covered end of an
    /// arm is not (the arm would grow a dot), nor is a faint area.
    #[test]
    fn no_stroke_drops_out() {
        let plane = |w: u32, h: u32, f: &dyn Fn(u32, u32) -> u8| -> Vec<u8> { (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| f(x, y)).collect() };
        let on = |v: Vec<bool>, w: u32| -> Vec<(u32, u32)> { v.iter().enumerate().filter(|(_, on)| **on).map(|(i, _)| (i as u32 % w, i as u32 / w)).collect() };
        // an upright stroke over columns 1 and 2: one of them, all the way down
        let upright = plane(4, 5, &|x, _| if x == 1 || x == 2 { 90 } else { 0 });
        assert_eq!(on(lit_dots(&upright, 4, 5, LIT), 4), (0..5).map(|y| (1, y)).collect::<Vec<_>>());
        // a bar over rows 2 and 3: one of them, all the way along
        let bar = plane(5, 6, &|_, y| if y == 2 || y == 3 { 90 } else { 0 });
        assert_eq!(on(lit_dots(&bar, 5, 6, LIT), 5), (0..5).map(|x| (x, 2)).collect::<Vec<_>>());
        // an arm lit in columns 0 to 2 of row 1, its end in column 3 half covered: as it is
        let arm = plane(5, 3, &|x, y| if y == 1 && x <= 2 { 255 } else if (x, y) == (3, 1) { 90 } else { 0 });
        assert_eq!(on(lit_dots(&arm, 5, 3, LIT), 5), [(0, 1), (1, 1), (2, 1)]);
        // a faint area (a glow) lights nothing
        assert!(on(lit_dots(&plane(5, 5, &|_, _| 60), 5, 5, LIT), 5).is_empty());
    }

    /// A letter moved onto a display's line whose faint edge (a smooth font's) falls past the
    /// last row keeps its size: the edge is cut off; ink that lights a dot is squeezed in.
    #[test]
    fn a_faint_edge_is_cut_not_squeezed_in() {
        // one column: ink in rows 0..=2, a faint edge in row 3; three rows to land in
        let src = [255u8, 255, 255, 40];
        assert_eq!(placed(&src, 1, 4, 0, 3), vec![255, 255, 255]);
        // moved a row down, the ink itself reaches past the last row: squeezed into rows 1..=2
        let out = placed(&[255u8, 255, 255, 0], 1, 4, 1, 3);
        assert_eq!(out[0], 0);
        assert!(out[1] > 0 && out[2] > 0, "{out:?}");
        // the faint edge is kept where it lands within the rows
        assert_eq!(placed(&[0u8, 255, 255, 40], 1, 4, -1, 3), vec![255, 255, 40]);
    }

    /// A font of dots of `rows` rows: capitals lit from `cap.0` to `cap.1` (5 wide, two empty
    /// columns beside), small letters from `x_top` down to `cap.1`, their g, j, p, q and y to
    /// `desc` (no small letters: `x_top` None).
    fn band_font(rows: u32, cap: (u32, u32), x_top: Option<u32>, desc: u32) -> DotFont {
        let glyph = |ch: char, top: u32, bottom: u32, w: u32| {
            let cov = (0..rows).flat_map(|y| (0..w + 2).map(move |x| if (top..=bottom).contains(&y) && x >= 1 && x <= w { 255 } else { 0 })).collect();
            DotGlyph { ch, w: w + 2, cov }
        };
        let mut glyphs: Vec<DotGlyph> = ('A'..='Z').map(|c| glyph(c, cap.0, cap.1, 5)).collect();
        if let Some(x) = x_top {
            glyphs.extend(('a'..='z').map(|c| glyph(c, x, if "gjpqy".contains(c) { desc } else { cap.1 }, 4)));
        }
        glyphs.extend(('0'..='9').map(|c| glyph(c, cap.0, cap.1, 4)));
        DotFont { name: "Band".into(), rows, gap: 1, space: 3, glyphs }
    }

    #[test]
    fn the_band_of_a_matrix_font() {
        // churafont CE 16x5: capitals rows 3 to 12, descenders to 15
        let b = LetterBand::of(&band_font(16, (3, 12), Some(6), 15));
        assert_eq!((b.cap_top, b.base, b.below, b.capitals_only, b.fill), (3, 13, 3, false, false));
        assert_eq!((b.cap_w, b.small_w, b.digit_w), (5.0, Some(4.0), Some(4.0)));
        // (the empty columns beside the letters: part of the space between them)
        assert_eq!((b.gap, b.space), (3, 3));
        // Krueger 11x5: capitals only, rows 2 to 12 of 13
        let k = LetterBand::of(&band_font(13, (2, 12), None, 0));
        assert_eq!((k.cap_top, k.base, k.below, k.capitals_only), (2, 13, 0, true));
        // a font of too few letters to tell: they fill its rows
        let none = LetterBand::of(&DotFont { name: "x".into(), rows: 9, gap: 1, space: 2, glyphs: Vec::new() });
        assert!(none.fill && none.base == 9);
        // the player's letter height: as much smaller, in the middle of where the letters were
        let s = b.scaled_to(10);
        assert_eq!((s.cap_top, s.cap_rows(), s.below), (4, 8, 2));
        assert!((s.cap_w - 5.0 * 10.0 / 13.0).abs() < 1e-4);
        assert_eq!(b.scaled_to(13), b);
        assert_eq!(k.scaled_to(20).cap_rows(), 13, "no taller than the rows");
        assert_eq!(LetterBand::filling(16).scaled_to(9).rows, 9);
    }

    #[test]
    fn a_vector_font_takes_the_band_it_replaces() {
        let own = band_font(16, (3, 12), Some(6), 15);
        let band = LetterBand::of(&own);
        let capitals: Vec<char> = ('A'..='Z').collect();
        let natural = DotFont::from_vector_in(HANKEN, 0, &band, &capitals, 1.0).unwrap().mean_width(&capitals).unwrap();
        let c = condense_for(natural, band.cap_w);
        assert!(c < 1.0, "Hanken's capitals are wider than 5 dots: {natural}");
        let f = DotFont::from_vector_in(HANKEN, 0, &band, &charset(), c).unwrap();
        assert_eq!(f.rows, 16);
        // its capitals in the capitals' rows, its descenders no deeper than the rows
        assert_eq!(f.lit_rows(f.glyph('H').unwrap()), Some((3, 12)));
        assert!(f.lit_rows(f.glyph('g').unwrap()).is_some_and(|(_, b)| b > 12 && b <= 15));
        assert!(f.lit_rows(f.glyph('x').unwrap()).is_some_and(|(t, b)| t > 3 && b == 12), "a small letter stands on the line");
        // a capital with an accent squeezed in, not cut off
        assert!(f.lit_rows(f.glyph('Ä').unwrap()).is_some_and(|(t, b)| t <= 2 && b == 12));
        // about as wide; the gap and the space of the font it replaces
        assert!(f.mean_width(&capitals).unwrap() <= band.cap_w * 1.1 + 0.25, "{:?}", f.mean_width(&capitals));
        assert_eq!((f.gap, f.space), (band.gap, band.space));
        assert_eq!(condense_for(5.4, 5.0), 1.0, "a tenth wider: as it is");
        assert_eq!(condense_for(20.0, 5.0), 0.5, "no narrower than half");
        // and drawn in the display's grid: k = 1, its dots where the band says
        let a = f.to_atlas(&DotGrid::plain(16), 16, "Hanken");
        let rows: Vec<u32> = lit(&a, 'H').iter().map(|p| p.1).collect();
        assert_eq!((rows.iter().min(), rows.iter().max()), (Some(&3), Some(&12)));
    }

    #[test]
    fn capitals_only_band() {
        let band = LetterBand::of(&band_font(13, (2, 12), None, 0));
        let f = DotFont::from_vector_in(HANKEN, 0, &band, &['A', 'a', 'Q', 'ü', 'Ü', 'H'], 1.0).unwrap();
        assert_eq!(f.glyph('a').unwrap().cov, f.glyph('A').unwrap().cov);
        assert_eq!(f.glyph('ü').unwrap().cov, f.glyph('Ü').unwrap().cov);
        assert_eq!(f.lit_rows(f.glyph('H').unwrap()), Some((2, 12)));
        // (a Q's tail in the rows, the capitals on their line)
        assert!(f.lit_rows(f.glyph('Q').unwrap()).is_some_and(|(t, b)| t == 2 && b == 12));
    }

    /// A pixel font of `h` rows: each of `chars` a 4 x `h` letter (its own pattern of `lit`).
    fn pixel_atlas(h: u32, chars: &str, lit: impl Fn(char, u32, u32) -> bool) -> FontAtlas {
        let n = chars.chars().count() as u32;
        let aw = n * 5;
        let mut alpha = vec![0u8; (aw * h * 4) as usize];
        let mut glyphs = Vec::new();
        for (i, ch) in chars.chars().enumerate() {
            let x0 = i as u32 * 5;
            for y in 0..h {
                for x in 0..4 {
                    if lit(ch, x, y) {
                        let at = ((y * aw + x0 + x) * 4) as usize;
                        alpha[at..at + 4].copy_from_slice(&[255; 4]);
                    }
                }
            }
            glyphs.push(FontChar { ch, x0: x0 as i32, x1: (x0 + 4) as i32, y: 0 });
        }
        let font = Font { name: "Own".into(), height: h as i32, gap: 1, chars: glyphs, ..Default::default() };
        FontAtlas::new(font, aw, h, alpha.clone(), alpha)
    }

    #[test]
    fn kept_glyphs_stay_the_buses() {
        // a bus's font of capitals in rows 2 to 11 of 13, and a '{' that is a pictogram there
        let own = pixel_atlas(13, "AEHNTU{", |c, x, y| match c {
            '{' => (x + y) % 2 == 0,
            _ => (2..12).contains(&y) && (x == 0 || x == 3 || y == 2),
        });
        let grid = DotGrid::of_atlas(&own);
        assert_eq!(grid, DotGrid::plain(13));
        let band = LetterBand::of(&DotFont::from_atlas(&own, &grid));
        assert_eq!((band.cap_top, band.base), (2, 12));
        let chars = text_chars_of(&own.font);
        assert!(chars.contains(&'a') && chars.contains(&'H') && !chars.contains(&'{') && !chars.contains(&'#') && !chars.contains(&'B'), "{chars:?}");
        let sub = DotFont::from_vector_in(HANKEN, 0, &band, &chars, 1.0).unwrap().to_atlas(&grid, 13, "Hanken");
        let merged = merge_kept(&sub, &own, &[]);
        assert_eq!(merged.font.height, 13);
        let pixels = |a: &FontAtlas, ch: char| -> Vec<u8> {
            let g = a.font.chars.iter().find(|g| g.ch == ch).unwrap();
            (0..13).flat_map(|y| (g.x0..g.x1).map(move |x| (x, y))).map(|(x, y)| a.alpha[(((g.y + y) as u32 * a.width + x as u32) * 4) as usize]).collect()
        };
        // the pictogram stays the bus's, pixel for pixel; a letter is the chosen font's
        assert_eq!(pixels(&merged, '{'), pixels(&own, '{'));
        assert_ne!(pixels(&merged, 'A'), pixels(&own, 'A'));
        assert_eq!(pixels(&merged, 'A'), pixels(&sub, 'A'));
        // what the bus's font lacks stays out ('#' padding a line number draws nothing)
        assert!(merged.font.glyph('#').is_none() && merged.font.glyph('B').is_none());
        // a small letter the bus writes as its capital: the chosen font's capital
        assert_eq!(merged.font.glyph('a').map(|g| g.ch), Some('a'));
        // a space as wide as the bus's, the gap the chosen font's
        assert_eq!((merged.font.space_width(), merged.font.gap), (own.font.space_width(), sub.font.gap));
    }

    /// The pixels of the glyph `ch` of `a`, row by row (empty: none).
    fn glyph_pixels(a: &FontAtlas, ch: char) -> Vec<u8> {
        let Some(g) = a.font.chars.iter().find(|g| g.ch == ch) else { return Vec::new() };
        (0..a.font.height).flat_map(|y| (g.x0..g.x1).map(move |x| (x, y))).map(|(x, y)| a.alpha[(((g.y + y) as u32 * a.width + x as u32) * 4) as usize]).collect()
    }

    /// A chosen font of capitals alone (the stock "Krueger 16x9", "Annax Small") on a display
    /// whose font has small letters: a small letter is written as the chosen font's capital, as
    /// Omsi.exe writes a font of capitals - not left in the bus's own font, the line in two.
    #[test]
    fn a_chosen_font_of_capitals_writes_small_letters_as_capitals() {
        // capitals in rows 2 to 11 of 13, small letters from row 5, all of them
        let chars: String = ('A'..='Z').chain('a'..='z').chain(['Ü', 'ü']).collect();
        let own = pixel_atlas(13, &chars, |c, x, y| if c.is_uppercase() { (2..12).contains(&y) && (x == 0 || x == 3 || y == 2) } else { (5..12).contains(&y) && (x == 1 || y == 11) });
        let grid = DotGrid::of_atlas(&own);
        assert_eq!(grid, DotGrid::plain(13));
        let band = LetterBand::of(&DotFont::from_atlas(&own, &grid));
        assert!(!band.capitals_only);
        let capitals: Vec<char> = ('A'..='Z').chain(['Ü']).collect();
        let sub = DotFont::from_vector_in(HANKEN, 0, &band, &capitals, 1.0).unwrap().to_atlas(&grid, 13, "Capitals");
        assert!(sub.font.chars.iter().all(|g| !g.ch.is_lowercase()));
        let merged = merge_kept(&sub, &own, &[]);
        for (small, cap) in "hauptbahnhofü".chars().map(|c| (c, c.to_uppercase().next().unwrap())) {
            assert_eq!(glyph_pixels(&merged, small), glyph_pixels(&sub, cap), "{small}: the chosen font's {cap}");
            assert_ne!(glyph_pixels(&merged, small), glyph_pixels(&own, small), "{small}: not the bus's own");
        }
    }

    /// The pictograms a bus's font draws over letters and marks of text stay its own: the
    /// SD200's Annax Large - a font of capitals - draws a wheelchair over `g` and `h` and a
    /// checked flag over `x`, its depot's roof over `/^\`; the Hamburg LED fonts a column of
    /// every LED (the display's test) over `í`. A slash that is one, a small letter of a font of
    /// small letters and a capital stay the chosen font's.
    #[test]
    fn pictograms_stay_the_buses() {
        // Annax Large: capitals in rows 0 to 14 of 16, no small letters but its pictures
        let annax = pixel_atlas(16, "AEFGHILNTXgx/^", |c, x, y| match c {
            'g' => y == 3 || x == 1 || (x + y) % 5 == 0,
            'x' => y < 15 && (x + y) % 2 == 0,
            // (a roof's slope on a bar)
            '/' => (13..15).contains(&y) || ((9..13).contains(&y) && x == 12 - y),
            '^' => y < 4 && (1..=2).contains(&x),
            _ => y < 15 && (x == 0 || x == 3 || y == 0),
        });
        let grid = DotGrid::of_atlas(&annax);
        assert_eq!(grid, DotGrid::plain(16));
        let dots = DotFont::from_atlas(&annax, &grid);
        let band = LetterBand::of(&dots);
        assert!(band.capitals_only && !band.fill, "{band:?}");
        let kept = pictograms_of(&dots, &band);
        assert_eq!(kept, vec!['/', 'g', 'x']);
        // a vector font has a g, an x and a slash: they are not drawn in it
        let chars: Vec<char> = text_chars_of(&annax.font).into_iter().filter(|c| !kept.contains(c)).collect();
        let sub = DotFont::from_vector_in(HANKEN, 0, &band, &chars, 1.0).unwrap().to_atlas(&grid, 16, "Hanken");
        let merged = merge_kept(&sub, &annax, &kept);
        for c in ['g', 'x', '/', '^'] {
            assert_eq!(glyph_pixels(&merged, c), glyph_pixels(&annax, c), "{c}: the bus's own");
        }
        assert_eq!(glyph_pixels(&merged, 'H'), glyph_pixels(&sub, 'H'));
        // ... nor taken from one made with them (they were: every letter and mark of text)
        let all = DotFont::from_vector_in(HANKEN, 0, &band, &text_chars_of(&annax.font), 1.0).unwrap().to_atlas(&grid, 16, "Hanken");
        assert!(!glyph_pixels(&all, 'g').is_empty());
        assert_eq!(glyph_pixels(&merge_kept(&all, &annax, &kept), 'g'), glyph_pixels(&annax, 'g'));
        // a slash that is one: the chosen font's
        let slash = pixel_atlas(16, "AEFHILNT/", |c, x, y| match c {
            '/' => (2..15).contains(&y) && x == 3 - (y - 2) * 4 / 13,
            _ => y < 15 && (x == 0 || x == 3 || y == 0),
        });
        let dots = DotFont::from_atlas(&slash, &DotGrid::of_atlas(&slash));
        assert!(pictograms_of(&dots, &LetterBand::of(&dots)).is_empty());
        // 4_LW_Benefit_Gross: small letters, capitals in rows 3 to 12 of 18, descenders to 15;
        // its `í` lit from the first row to the last
        let led_chars: String = ('A'..='Z').chain('a'..='z').chain(['í', 'é']).collect();
        let led = pixel_atlas(18, &led_chars, |c, x, y| match c {
            'í' => x == 1,
            'é' => ((5..13).contains(&y) && x != 2) || (y == 3 && x == 2),
            'g' | 'j' | 'p' | 'q' | 'y' => (6..16).contains(&y) && x == 1,
            c if c.is_lowercase() => (6..13).contains(&y) && x == 1,
            _ => (3..13).contains(&y) && (x == 0 || x == 3 || y == 3),
        });
        let dots = DotFont::from_atlas(&led, &DotGrid::of_atlas(&led));
        let band = LetterBand::of(&dots);
        assert_eq!((band.cap_top, band.base, band.below, band.capitals_only), (3, 13, 3, false));
        assert_eq!(pictograms_of(&dots, &band), vec!['í']);
    }

    #[test]
    fn the_names_of_a_font_file() {
        let dir = std::env::temp_dir().join(format!("omsi_dotfont_names_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let file = dir.join("sub").join("Hanken.ttf");
        std::fs::write(&file, HANKEN).unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        std::fs::write(dir.join("broken.otf"), "not a font").unwrap();
        let faces = vector_faces(&file);
        assert_eq!(faces.len(), 1);
        assert!(faces[0].family.starts_with("Hanken Grotesk"), "{faces:?}");
        assert!(faces[0].name.starts_with("Hanken Grotesk"), "{faces:?}");
        assert_eq!(faces[0].spec().file.as_deref(), Some(file.as_path()));
        assert!(vector_faces(&dir.join("broken.otf")).is_empty());
        assert_eq!(vector_font_files(&dir), vec![dir.join("broken.otf"), file.clone()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
