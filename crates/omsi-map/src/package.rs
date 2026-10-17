//! A map's pre-built surface (`Maps/<Map>/openomsi-realmap/`: `manifest.cfg` and `surface.oms`),
//! read as plain data: water, green, built-up land, rivers, rail and borders in world metres.
//!
//! The map author ships it (docs/MAP_PACKAGE.md). Everything in it is made beforehand,
//! triangulated and cut into chunks, so this file only checks the header, keeps the chunk
//! index and hands out the chunks near a place. It draws nothing: the game's navigator and the
//! launcher's route map each turn the data into their own picture. Nothing is computed from
//! OpenStreetMap or any other source here, and a map without the folder gets `None`.
//!
//! Positions in the file are metres of the author's tile grid (`grid` in the manifest: column
//! times grid plus the place in the tile), which is what OMSI 2 itself shows for the map's
//! objects; they are put onto this game's grid with [`crate::tile_local_to_world`], so the grid
//! must have been set first ([`crate::configure_grid`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod lines;
mod places;
mod relief;
pub use lines::{LineInfo, RouteInfo, GROUP_BRT, GROUP_EXPRESS, GROUP_FEEDER, MODAL_BUS, MODAL_METRO, MODAL_RAIL, MODAL_TRAM};
pub use places::{Label, Marker, Region, Street};
pub use relief::{LINE_CONTOUR, LINE_CONTOUR_INDEX};

pub const FOLDER: &str = "openomsi-realmap";
const MAGIC: &[u8; 4] = b"OMSF";
const VERSION: u32 = 1;

/// The filling classes of the file (an unknown class is skipped, so a later version can add more).
pub const FILL_WATER: u8 = 0;
pub const FILL_GREEN: u8 = 1;
pub const FILL_BUILT: u8 = 2;
pub const FILL_GRASS: u8 = 3;
pub const FILL_FOREST: u8 = 4;
pub const FILL_SPORT: u8 = 5;
pub const FILL_FARM: u8 = 6;
pub const FILL_CLASSES: usize = 7;
/// The line classes: a river (its width tells river from stream), the railway, a border.
pub const LINE_RIVER: u8 = 0;
pub const LINE_RAIL: u8 = 1;
pub const LINE_BORDER: u8 = 2;
/// A named street (not drawn, only looked up) and a named street that also says its limits: `width` is the limit
/// (km/h, 0 = none), `flags` the limit of heavy vehicles (km/h, 0 = the same, or none).
pub const LINE_STREET: u8 = 3;
pub const LINE_STREET_LIMITS: u8 = 4;

/// A colour as the manifest writes it, `#rrggbb`.
pub type Rgb = [u8; 3];

/// What the author's manifest may colour (`water = #rrggbb`, and `light_water = ...` for the
/// light map); a key left out keeps the default of that look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// The bare ground the map is drawn on.
    pub land: Rgb,
    pub water: Rgb,
    pub river: Rgb,
    /// Parks and gardens, grass and meadows, woods and forests, sports grounds, farmland.
    pub green: Rgb,
    pub grass: Rgb,
    pub forest: Rgb,
    pub sport: Rgb,
    pub farm: Rgb,
    pub built: Rgb,
    pub rail: Rgb,
    pub rail_edge: Rgb,
    pub border: Rgb,
}

fn hex(v: u32) -> Rgb {
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

impl Palette {
    pub fn dark() -> Palette {
        Palette {
            land: hex(0x1c2329),
            water: hex(0x173341),
            river: hex(0x2b6a86),
            green: hex(0x24402f),
            grass: hex(0x1f3a2b),
            forest: hex(0x183021),
            sport: hex(0x2a4a3a),
            farm: hex(0x2b3a2a),
            built: hex(0x252f37),
            rail: hex(0x7d8a95),
            rail_edge: hex(0x0c1013),
            border: hex(0xb592d6),
        }
    }

    pub fn light() -> Palette {
        Palette {
            land: hex(0xe4e0d6),
            water: hex(0x9cc3d6),
            river: hex(0x7fb0cc),
            green: hex(0xbfd8ab),
            grass: hex(0xd0e0b8),
            forest: hex(0xa9cb94),
            sport: hex(0xc3dfb0),
            farm: hex(0xe8e6c4),
            built: hex(0xd6d1c3),
            rail: hex(0x8c877b),
            rail_edge: hex(0xf4f1ea),
            border: hex(0x9070a8),
        }
    }

    fn set(&mut self, key: &str, c: Rgb) {
        match key {
            "land" => self.land = c,
            "water" => self.water = c,
            "river" => self.river = c,
            "green" => self.green = c,
            "grass" => self.grass = c,
            "forest" => self.forest = c,
            "sport" => self.sport = c,
            "farm" => self.farm = c,
            "built" => self.built = c,
            "rail" => self.rail = c,
            "rail_edge" => self.rail_edge = c,
            "border" => self.border = c,
            _ => {}
        }
    }
}

fn parse_color(v: &str) -> Option<Rgb> {
    let h = v.trim().strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    u32::from_str_radix(h, 16).ok().map(hex)
}

/// One chunk's entry of the index: its column and row, and where its body lies in the file.
#[derive(Clone, Copy, Debug)]
struct Entry {
    cx: i32,
    cy: i32,
    at: usize,
    len: usize,
}

/// A line of the surface in world metres: its class, its width (a river's) and its points.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub class: u8,
    pub width: f32,
    pub points: Vec<(f64, f64)>,
}

/// The triangles and lines of some chunks, in world metres.
#[derive(Clone, Debug, Default)]
pub struct Layers {
    /// Triangles by filling class (index = class, `FILL_CLASSES` lists).
    pub fills: [Vec<[(f64, f64); 3]>; FILL_CLASSES],
    /// Rivers, rail and borders (the streets and contours of a package are not here: the
    /// readers that need them read their own classes).
    pub lines: Vec<Line>,
    /// The contour lines of the ground (classes 5 and 6; `width` is the height in metres): none in a package without relief.
    pub contours: Vec<Line>,
}

/// The package of a map (see the module).
#[derive(Clone, Debug)]
pub struct Package {
    pub name: String,
    /// Attribution the data needs (the map's licence).
    pub credit: String,
    pub dark: Palette,
    pub light: Palette,
    origin: (f64, f64),
    grid: f64,
    chunk: f64,
    index: Vec<Entry>,
    data: Vec<u8>,
    names: Vec<String>,
    by_key: HashMap<(i32, i32), usize>,
    /// The package folder (`markers.cfg`, `regions.cfg` and `icons/` lie in it).
    dir: PathBuf,
    /// The map's folder the package was opened for (its timetable is there).
    map_dir: PathBuf,
    regions: Vec<Region>,
    has_streets: bool,
    /// Where real north lies: degrees clockwise from the map's own up (the manifest's `north`).
    north: f32,
    lines: Vec<LineInfo>,
    /// The package carries contour lines; the manifest's `terrain_offset`; where heights are written (made on first use).
    has_contours: bool,
    terrain_offset: f32,
    contour_marks: std::sync::OnceLock<Vec<((f64, f64), i32, bool)>>,
}

/// Bounds-checked reading: any short or odd file ends in `None`, never a panic.
struct Rd<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.i.checked_add(n)?;
        let s = self.b.get(self.i..end)?;
        self.i = end;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn left(&self) -> usize {
        self.b.len() - self.i
    }
}

impl Package {
    /// The map folder's package, or `None` when there is none or it cannot be used (one line
    /// in the log says why; the map then shows as it always did).
    pub fn open(map_dir: &Path) -> Option<Package> {
        // the package sits in the map's own folder, or in the same place of another content
        // root (a package for a map one does not own goes into the game's content folder, as a
        // mod does: the OMSI 2 installation is never written to); the root with the most
        // priority wins
        let dir = omsi_cfg::mirrored_dirs(map_dir)
            .into_iter()
            .map(|d| d.join(FOLDER))
            .find(|d| omsi_cfg::vfs::is_file(&d.join("manifest.cfg")))?;
        let manifest = omsi_cfg::vfs::read(&dir.join("manifest.cfg")).ok()?;
        let text = String::from_utf8_lossy(&manifest).into_owned();
        let (mut format, mut grid) = (0u32, 0.0f64);
        let (mut name, mut credit) = (String::new(), String::new());
        let (mut dark, mut light) = (Palette::dark(), Palette::light());
        let mut terrain_offset = 0.0f32;
        let mut north = 0.0f32;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            match k.as_str() {
                "format" => format = v.parse().unwrap_or(0),
                "grid" => grid = v.parse().unwrap_or(0.0),
                "name" => name = v.to_string(),
                "credit" => credit = v.to_string(),
                "terrain_offset" => terrain_offset = v.parse().unwrap_or(0.0),
                "north" => north = v.parse::<f32>().ok().filter(|n| n.is_finite()).unwrap_or(0.0),
                _ => {
                    // a colour: `water = #..` for the dark map, `light_water = #..` for the light one
                    if let Some(c) = parse_color(v) {
                        match k.strip_prefix("light_") {
                            Some(key) => light.set(key, c),
                            None => dark.set(&k, c),
                        }
                    }
                }
            }
        }
        if format != VERSION || !(grid > 1.0) {
            log::warn!(
                "map package: {} has format {format}, grid {grid}: not used",
                dir.display()
            );
            return None;
        }
        let data = omsi_cfg::vfs::read(&dir.join("surface.oms")).ok()?;
        let mut p = Self::parse(data, name, credit, dark, light, grid);
        if let Some(p) = p.as_mut() {
            p.dir = dir.clone();
            p.map_dir = map_dir.to_path_buf();
            p.terrain_offset = terrain_offset;
            p.north = north;
            p.load_relief();
            p.load_places();
            p.load_lines();
        }
        if p.is_none() {
            log::warn!(
                "map package: {}/surface.oms is not a valid version {VERSION} file: not used",
                dir.display()
            );
        }
        p
    }

    fn parse(
        data: Vec<u8>,
        name: String,
        credit: String,
        dark: Palette,
        light: Palette,
        grid: f64,
    ) -> Option<Package> {
        let mut r = Rd { b: &data, i: 0 };
        if r.take(4)? != MAGIC || r.u32()? != VERSION {
            return None;
        }
        let origin = (r.f64()?, r.f64()?);
        for _ in 0..4 {
            r.f32()?; // the extent: only readers that size a view by it need it
        }
        let chunk = r.f32()? as f64;
        if !(chunk > 1.0) || !origin.0.is_finite() || !origin.1.is_finite() {
            return None;
        }
        let n_names = r.u32()? as usize;
        if n_names > r.left() {
            return None;
        }
        let mut names = Vec::with_capacity(n_names);
        for _ in 0..n_names {
            let n = r.u16()? as usize;
            names.push(String::from_utf8_lossy(r.take(n)?).into_owned());
        }
        let n_chunks = r.u32()? as usize;
        if n_chunks.checked_mul(20)? > r.left() {
            return None;
        }
        let mut index = Vec::with_capacity(n_chunks);
        for _ in 0..n_chunks {
            let (cx, cy) = (r.i32()?, r.i32()?);
            let (at, len) = (r.u64()? as usize, r.u32()? as usize);
            if at.checked_add(len)? > data.len() {
                return None;
            }
            index.push(Entry { cx, cy, at, len });
        }
        let by_key = index.iter().enumerate().map(|(i, e)| ((e.cx, e.cy), i)).collect();
        Some(Package {
            name,
            credit,
            dark,
            light,
            origin,
            grid,
            chunk,
            index,
            data,
            names,
            by_key,
            dir: PathBuf::new(),
            map_dir: PathBuf::new(),
            regions: Vec::new(),
            has_streets: false,
            north: 0.0,
            lines: Vec::new(),
            has_contours: false,
            terrain_offset: 0.0,
            contour_marks: std::sync::OnceLock::new(),
        })
    }

    /// The map's folder the package was opened for.
    pub fn map_dir(&self) -> &Path {
        &self.map_dir
    }

    /// Where real north lies: degrees clockwise from the map's own up (0 when the manifest does not say).
    pub fn north(&self) -> f32 {
        self.north
    }

    /// The colours of the dark map or of the light one.
    pub fn palette(&self, light: bool) -> &Palette {
        if light {
            &self.light
        } else {
            &self.dark
        }
    }

    /// How many chunks the file has.
    pub fn chunks(&self) -> usize {
        self.index.len()
    }

    /// A position of the file (metres from its origin) as world metres.
    fn to_world(&self, dx: f32, dy: f32) -> (f64, f64) {
        let (ax, ay) = (self.origin.0 + dx as f64, self.origin.1 + dy as f64);
        let (tx, ty) = ((ax / self.grid).floor(), (ay / self.grid).floor());
        crate::tile_local_to_world(
            tx as i32,
            ty as i32,
            ax - tx * self.grid,
            ay - ty * self.grid,
        )
    }

    /// Where a chunk lies in world metres: the middle of its square.
    fn chunk_centre(&self, e: &Entry) -> (f64, f64) {
        self.to_world(
            ((e.cx as f64 + 0.5) * self.chunk) as f32,
            ((e.cy as f64 + 0.5) * self.chunk) as f32,
        )
    }

    /// The triangles and lines of the chunks that reach into the world rectangle `lo` - `hi`
    /// (a chunk's corners can lie a little over its edge once put onto the game's grid, so it
    /// is taken a little wider). `None` for either corner reads the whole file.
    pub fn layers(&self, area: Option<((f64, f64), (f64, f64))>) -> Layers {
        let reach = self.chunk * std::f64::consts::SQRT_2 * 0.5 + 60.0;
        let mut out = Layers::default();
        for e in &self.index {
            if let Some((lo, hi)) = area {
                let c = self.chunk_centre(e);
                if c.0 < lo.0 - reach
                    || c.0 > hi.0 + reach
                    || c.1 < lo.1 - reach
                    || c.1 > hi.1 + reach
                {
                    continue;
                }
            }
            if let Some(body) = self.data.get(e.at..e.at + e.len) {
                let _ = self.walk(body, &mut out);
            }
        }
        out
    }

    /// One chunk's body: fills, then lines, then names (the names are not read here). A chunk
    /// that ends short keeps what was read before it.
    fn walk(&self, body: &[u8], out: &mut Layers) -> Option<()> {
        let mut r = Rd { b: body, i: 0 };
        for _ in 0..r.u32()? {
            let class = r.u8()?;
            let n = r.u32()? as usize;
            if n.checked_mul(24)? > r.left() {
                return None;
            }
            for _ in 0..n {
                let mut c = [(0.0, 0.0); 3];
                for p in c.iter_mut() {
                    *p = self.to_world(r.f32()?, r.f32()?);
                }
                if let Some(list) = out.fills.get_mut(class as usize) {
                    list.push(c);
                }
            }
        }
        for _ in 0..r.u32()? {
            let class = r.u8()?;
            let _flags = r.u8()?;
            let width = r.f32()?;
            let _name = r.u32()?;
            let n = r.u32()? as usize;
            if n.checked_mul(8)? > r.left() {
                return None;
            }
            if class == LINE_CONTOUR || class == LINE_CONTOUR_INDEX {
                let mut points = Vec::with_capacity(n);
                for _ in 0..n {
                    points.push(self.to_world(r.f32()?, r.f32()?));
                }
                out.contours.push(Line { class, width, points });
                continue;
            }
            if !matches!(class, LINE_RIVER | LINE_RAIL | LINE_BORDER) {
                // a street to look up: not for this reader
                r.take(n * 8)?;
                continue;
            }
            let mut points = Vec::with_capacity(n);
            for _ in 0..n {
                points.push(self.to_world(r.f32()?, r.f32()?));
            }
            out.lines.push(Line {
                class,
                width,
                points,
            });
        }
        Some(())
    }
}

#[cfg(test)]
impl Package {
    /// An empty package, for the tests of what hangs on one.
    pub(crate) fn test_stub() -> Package {
        Package {
            name: String::new(),
            credit: String::new(),
            dark: Palette::dark(),
            light: Palette::light(),
            origin: (0.0, 0.0),
            grid: 300.0,
            chunk: 2000.0,
            index: Vec::new(),
            data: Vec::new(),
            names: Vec::new(),
            by_key: HashMap::new(),
            dir: PathBuf::new(),
            map_dir: PathBuf::new(),
            regions: Vec::new(),
            has_streets: false,
            north: 0.0,
            lines: Vec::new(),
            has_contours: false,
            terrain_offset: 0.0,
            contour_marks: std::sync::OnceLock::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One chunk with a triangle, a river piece, a street and a name, written by hand in the file's layout.
    fn sample() -> Vec<u8> {
        let mut body = Vec::new();
        body.extend(1u32.to_le_bytes());
        body.push(FILL_WATER);
        body.extend(1u32.to_le_bytes());
        for v in [0.0f32, 0.0, 10.0, 0.0, 0.0, 10.0] {
            body.extend(v.to_le_bytes());
        }
        body.extend(2u32.to_le_bytes());
        for (class, pts) in [
            (LINE_RIVER, [0.0f32, 0.0, 10.0, 10.0]),
            (3u8, [1.0, 1.0, 2.0, 2.0]),
        ] {
            body.extend([class, 0]);
            body.extend(3.0f32.to_le_bytes());
            body.extend(u32::MAX.to_le_bytes());
            body.extend(2u32.to_le_bytes());
            for v in pts {
                body.extend(v.to_le_bytes());
            }
        }
        body.extend(1u32.to_le_bytes());
        body.extend([0u8, 100]);
        body.extend(0u32.to_le_bytes());
        body.extend(5.0f32.to_le_bytes());
        body.extend(5.0f32.to_le_bytes());
        let mut f = Vec::new();
        f.extend(MAGIC);
        f.extend(VERSION.to_le_bytes());
        f.extend(1000.0f64.to_le_bytes());
        f.extend(2000.0f64.to_le_bytes());
        for v in [0.0f32, 100.0, 0.0, 100.0, 2000.0] {
            f.extend(v.to_le_bytes());
        }
        f.extend(1u32.to_le_bytes());
        f.extend(4u16.to_le_bytes());
        f.extend(b"Test");
        f.extend(1u32.to_le_bytes());
        let at = f.len() + 20;
        f.extend(0i32.to_le_bytes());
        f.extend(0i32.to_le_bytes());
        f.extend((at as u64).to_le_bytes());
        f.extend((body.len() as u32).to_le_bytes());
        f.extend(body);
        f
    }

    fn open(data: Vec<u8>) -> Option<Package> {
        Package::parse(
            data,
            String::new(),
            String::new(),
            Palette::dark(),
            Palette::light(),
            300.0,
        )
    }

    #[test]
    fn a_valid_file_gives_its_triangles_and_lines() {
        let p = open(sample()).expect("valid");
        assert_eq!(p.chunks(), 1);
        let l = p.layers(None);
        assert_eq!(l.fills[FILL_WATER as usize].len(), 1);
        // the street is for readers that look streets up: only the river is here
        assert_eq!(l.lines.len(), 1);
        assert_eq!(l.lines[0].class, LINE_RIVER);
        assert_eq!(l.lines[0].points.len(), 2);
        // the file's origin is (1000, 2000); on a 300 m grid that is tile (3, 6), 100 m in and 200 m up
        let (x, y) = crate::tile_local_to_world(3, 6, 100.0, 200.0);
        let t = l.fills[FILL_WATER as usize][0][0];
        assert!(
            (t.0 - x).abs() < 1e-6 && (t.1 - y).abs() < 1e-6,
            "{t:?} vs {x} {y}"
        );
    }

    #[test]
    fn an_area_leaves_out_the_chunks_that_are_not_in_it() {
        let p = open(sample()).expect("valid");
        let (x, y) = p.chunk_centre(&p.index[0]);
        assert_eq!(
            p.layers(Some(((x - 10.0, y - 10.0), (x + 10.0, y + 10.0))))
                .fills[0]
                .len(),
            1
        );
        assert_eq!(
            p.layers(Some(((x + 100_000.0, y), (x + 100_100.0, y + 10.0))))
                .fills[0]
                .len(),
            0
        );
    }

    #[test]
    fn broken_files_are_refused_not_panicked_on() {
        let good = sample();
        for cut in [0, 3, 8, 20, 40, good.len() - 1] {
            // a short file is either refused or, when only the last chunk is cut, read as far as it goes
            if let Some(p) = open(good[..cut].to_vec()) {
                let _ = p.layers(None);
            }
        }
        let mut bad = good.clone();
        bad[0] = b'X';
        assert!(open(bad).is_none());
        let mut huge = good;
        huge[44..48].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(open(huge).is_none());
    }

    #[test]
    fn the_synthetic_example_of_the_docs_reads() {
        // docs/examples/openomsi-realmap: made up for the tests, in the tile grid of the tutorial map
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples");
        let p = Package::open(&dir).expect("docs/examples/openomsi-realmap is a package");
        assert_eq!(p.name, "Synthetic example");
        assert_eq!(p.chunks(), 1);
        assert_eq!(p.north(), 12.0, "the manifest of the example says where north lies");
        let l = p.layers(None);
        assert_eq!(l.fills[FILL_WATER as usize].len(), 14);
        for class in [FILL_GREEN, FILL_BUILT, FILL_GRASS, FILL_FOREST, FILL_SPORT, FILL_FARM] {
            assert_eq!(l.fills[class as usize].len(), 2, "class {class}");
        }
        let classes: Vec<u8> = l.lines.iter().map(|x| x.class).collect();
        assert_eq!(classes, vec![LINE_RIVER, LINE_RIVER, LINE_RAIL, LINE_BORDER]);
        // a hill: eleven contour lines, from 40 to 60 m, an index line at each tenth
        assert!(p.has_contours());
        assert_eq!(l.contours.len(), 11);
        assert_eq!(l.contours.iter().filter(|c| c.class == LINE_CONTOUR_INDEX).map(|c| c.width as i32).collect::<Vec<_>>(), vec![40, 50, 60]);
        // the file's origin is the south-west corner of the tutorial map's tiles (-300, -600): tile (-1, -2)
        let (x, y) = crate::tile_local_to_world(-1, -2, 0.0, 0.0);
        let first = l.fills[FILL_WATER as usize][0][0];
        assert!((first.0 - (x + 420.0)).abs() < 1e-3 && (first.1 - (y + 800.0)).abs() < 1e-3, "{first:?}");
    }

    #[test]
    fn colours_read_as_hex() {
        assert_eq!(parse_color("#102030"), Some([0x10, 0x20, 0x30]));
        assert_eq!(parse_color("#12"), None);
        assert_eq!(parse_color("red"), None);
    }
}
