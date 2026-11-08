//! The per-tile raster of the surfaces lying on the terrain: where the ground is cut away
//! and what height a ground query meets there.

use super::*;

/// Per-tile raster of the surfaces (roads, crossings) lying on the terrain: where a texel is
/// covered the terrain is not drawn (the original cuts the terrain polygons instead) and the
/// surface height is used for ground queries.
/// How far below the ground a surface may lie and still take the ground away: enough for a
/// Take the ground away under every road surface lying about its height (`OMSI_ROAD_CUT=1`).
/// Omsi.exe does not: the ground goes only where the map says (`[terrainhole]` meshes, the
/// splines' `[terrainholeprofile]`, a tile's `.hole` file), and a road lying on it wins by
/// its depth bias. Cut by a raster of 1.5-3 m texels, the ground went a metre or two past
/// the road's edge too, and the sky showed through along kerbs and car parks.
pub fn road_cut() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| omsi_cfg::flags::OMSI_ROAD_CUT.live_os().is_some())
}

/// How far below the ground a surface may lie and still take the ground away: enough for a
/// sunken road or an underpass, not enough for a junction plate left at height zero.
const DEEP_CUT: f32 = 4.0;

/// How far around an aligned spline's outline the mask is looked at texel by texel (m).
const OUTLINE_EDGE: f32 = 1.2;
/// How far inside an outline the ground is still kept (m): the filtered mask puts a slanting
/// edge up to ~7 cm off, and past the outline (3 cm inside the road's edge) the ground gone
/// shows the sky under a kerb's top - pale slivers every two metres along the kerbs.
const OUTLINE_KEEP: f32 = 0.08;
/// How far inside a `[terrainhole]` cutter's rim the ground is still kept (m): the stock
/// junction cutters reach 10-20 cm past the kerbs and footways they lie under (Spandau,
/// `Transition_Bahnstr_Hansastr_hole.o3d`), and cut to the rim the ground showed the sky
/// along them.
pub(crate) const HOLE_KEEP: f32 = 0.2;

pub struct TileSurface {
    pub size: usize,
    /// The raster in blocks of `BLOCK`² texels, made when a surface first touches one: a
    /// tile's roads cover a part of it, and seven full layers of 512² texels took 5 MB a
    /// tile (270 MB around the Ahlheim main station). Per texel: whether a surface and a
    /// surface a vehicle can stand on cover it, the highest surface's height, the *lowest*
    /// surface's height (the ground has to be cut wherever it would come up through any of
    /// them - at a junction the plate can ride a metre over the road that runs under its
    /// edge), and the drivable surface's height (a railway embankment or a bridge deck
    /// passing over a road is a surface - the ground is cut under it - but the wheels must
    /// not be lifted onto it). [`TileSurface::finish`] drops a block's lowest or drivable
    /// heights where they are the highest one's.
    blocks: Vec<Option<Box<SurfaceBlock>>>,
    /// `[terrainhole]`: the ground is cut here whatever its height - a junction, an
    /// underpass or a tunnel mouth names a cutter mesh in its model, and OMSI takes the
    /// terrain away under it instead of leaving a mound over the carriageway. Stores the
    /// highest point of the cutter over each texel, in blocks like the surfaces; most
    /// tiles have none. Explicit holes do not use the optional road-cut height heuristic.
    holes: Vec<Option<Box<[f32; BLOCK * BLOCK]>>>,
    /// Which of 4×4 points over each texel a `[terrainhole]` cutter covers (bit 4y + x):
    /// the mask cuts the ground along the cutter's rim as exactly as along an aligned
    /// spline's outline, not by the texels whose middle it covers - eroded by one texel
    /// besides, a strip of grass up to a metre wide was left standing over the edges of a
    /// junction's carriageway (#823).
    hole_cover: Vec<Option<Box<[u16; BLOCK * BLOCK]>>>,
    /// The outlines the splines laid with `[spline_terrain_align]` cut out of the ground
    /// ([`spline_hole_outlines`]), in tile metres, with their bounds (x0, y0, x1, y1): cut
    /// exactly along them, as Omsi.exe cuts the terrain's triangles, not by texel.
    outlines: Vec<(Vec<Vec2>, [f32; 4])>,
    /// The faces the wheels stand on over this tile (spline height profiles, surface objects):
    /// a texel of the raster is most of a metre wide and holds one height, which put a wheel
    /// next to a kerb on top of the pavement or into the road's camber.
    pub drive: DriveGrid,
    /// What the tile's ground and trees sound like (see [`GroundSound`]).
    pub sound: GroundSound,
}

/// Edge of a [`TileSurface`] block (texels).
const BLOCK: usize = 16;
const COVERED: u8 = 1;
const ROAD: u8 = 2;

struct SurfaceBlock {
    flags: [u8; BLOCK * BLOCK],
    height: [f32; BLOCK * BLOCK],
    /// None: the same as `height`.
    low: Option<Box<[f32; BLOCK * BLOCK]>>,
    road: Option<Box<[f32; BLOCK * BLOCK]>>,
}

impl SurfaceBlock {
    fn new() -> Box<SurfaceBlock> {
        Box::new(SurfaceBlock { flags: [0; BLOCK * BLOCK], height: [0.0; BLOCK * BLOCK], low: Some(Box::new([0.0; BLOCK * BLOCK])), road: Some(Box::new([0.0; BLOCK * BLOCK])) })
    }
}

impl TileSurface {
    /// Bytes the rasters and the wheel grid hold on the heap: (rasters, wheel grid).
    pub fn heap_bytes(&self) -> (usize, usize) {
        let n = BLOCK * BLOCK;
        let mut rasters = self.blocks.capacity() * 8 + self.holes.capacity() * 8;
        for b in self.blocks.iter().flatten() {
            rasters += n * 5 + b.low.as_ref().map(|_| n * 4).unwrap_or(0) + b.road.as_ref().map(|_| n * 4).unwrap_or(0);
        }
        rasters += self.holes.iter().flatten().count() * n * 4 + self.hole_cover.iter().flatten().count() * n * 2;
        (rasters, self.drive.heap_bytes())
    }

    pub fn new(size: usize) -> TileSurface {
        let blocks = size.div_ceil(BLOCK);
        TileSurface { size, blocks: (0..blocks * blocks).map(|_| None).collect(), holes: (0..blocks * blocks).map(|_| None).collect(), hole_cover: (0..blocks * blocks).map(|_| None).collect(), outlines: Vec::new(), drive: DriveGrid::default(), sound: GroundSound::default() }
    }

    /// Block and index within it of texel `k`.
    #[inline]
    fn at(&self, k: usize) -> (usize, usize) {
        let (x, y) = (k % self.size, k / self.size);
        let per_row = self.size.div_ceil(BLOCK);
        ((y / BLOCK) * per_row + x / BLOCK, (y % BLOCK) * BLOCK + x % BLOCK)
    }

    /// Whether a surface covers texel `k`.
    #[inline]
    pub fn covered(&self, k: usize) -> bool {
        let (b, i) = self.at(k);
        self.blocks[b].as_ref().map(|b| b.flags[i] & COVERED != 0).unwrap_or(false)
    }

    /// Whether a drivable surface covers texel `k`.
    #[inline]
    pub fn road_covered(&self, k: usize) -> bool {
        let (b, i) = self.at(k);
        self.blocks[b].as_ref().map(|b| b.flags[i] & ROAD != 0).unwrap_or(false)
    }

    /// Height of the highest surface over texel `k` (0 where none).
    #[inline]
    pub fn height(&self, k: usize) -> f32 {
        let (b, i) = self.at(k);
        self.blocks[b].as_ref().map(|b| b.height[i]).unwrap_or(0.0)
    }

    /// Height of the lowest surface over texel `k`.
    #[inline]
    pub fn low_height(&self, k: usize) -> f32 {
        let (b, i) = self.at(k);
        self.blocks[b].as_ref().map(|b| b.low.as_ref().map(|l| l[i]).unwrap_or(b.height[i])).unwrap_or(0.0)
    }

    /// Height of the drivable surface over texel `k`.
    #[inline]
    pub fn road_height(&self, k: usize) -> f32 {
        let (b, i) = self.at(k);
        self.blocks[b].as_ref().map(|b| b.road.as_ref().map(|l| l[i]).unwrap_or(b.height[i])).unwrap_or(0.0)
    }

    /// The top of a `[terrainhole]` cutter over texel `k`, if one is there.
    #[inline]
    pub fn hole_height(&self, k: usize) -> Option<f32> {
        let (b, i) = self.at(k);
        self.holes[b].as_ref().map(|h| h[i]).filter(|h| *h > f32::MIN)
    }

    fn block_mut(&mut self, k: usize) -> (&mut SurfaceBlock, usize) {
        let (b, i) = self.at(k);
        (self.blocks[b].get_or_insert_with(SurfaceBlock::new), i)
    }

    /// The 4×4 points over texel `k` a cutter covers (see `hole_cover`).
    #[inline]
    fn hole_coverage(&self, k: usize) -> u16 {
        let (b, i) = self.at(k);
        self.hole_cover[b].as_ref().map(|c| c[i]).unwrap_or(0)
    }

    fn add_hole_cover(&mut self, k: usize, bits: u16) {
        let (b, i) = self.at(k);
        let block = self.hole_cover[b].get_or_insert_with(|| Box::new([0u16; BLOCK * BLOCK]));
        block[i] |= bits;
    }

    fn add_hole(&mut self, k: usize, h: f32) {
        let (b, i) = self.at(k);
        let block = self.holes[b].get_or_insert_with(|| Box::new([f32::MIN; BLOCK * BLOCK]));
        block[i] = block[i].max(h);
    }

    /// Put a surface at height `h` over texel `k`.
    fn add_surface(&mut self, k: usize, h: f32, drivable: bool) {
        let (b, i) = self.block_mut(k);
        let first = b.flags[i] & COVERED == 0;
        if first || h > b.height[i] {
            b.height[i] = h;
        }
        if let Some(low) = b.low.as_mut() {
            if first || h < low[i] {
                low[i] = h;
            }
        }
        b.flags[i] |= COVERED;
        if drivable {
            if let Some(road) = b.road.as_mut() {
                if b.flags[i] & ROAD == 0 || h > road[i] {
                    road[i] = h;
                }
            }
            b.flags[i] |= ROAD;
        }
    }

    /// Let go of what the finished raster does not need: a block's lowest and drivable
    /// heights where every texel has them equal to the highest one.
    fn compact(&mut self) {
        for b in self.blocks.iter_mut().flatten() {
            let (flags, height) = (&b.flags, &b.height);
            if b.low.as_ref().map(|l| (0..BLOCK * BLOCK).all(|i| flags[i] & COVERED == 0 || l[i] == height[i])).unwrap_or(false) {
                b.low = None;
            }
            if b.road.as_ref().map(|r| (0..BLOCK * BLOCK).all(|i| flags[i] & ROAD == 0 || r[i] == height[i])).unwrap_or(false) {
                b.road = None;
            }
        }
    }

    /// Add a mesh the wheels stand on: a spline's height profile, a surface object, or a low
    /// object with a `[collision_mesh]` (a traffic island) that cuts no ground. The probe
    /// takes the highest face below the axle, so a bridge deck overhead is never picked.
    pub fn add_drive_mesh(&mut self, mesh: &MeshData, transform: &Mat4, origin: DVec3, tx: i32, ty: i32) {
        self.add_drive_mesh_surf(mesh, transform, origin, tx, ty, None);
    }

    /// [`TileSurface::add_drive_mesh`] with the `.surf` maps of the mesh's textures: the
    /// faces drawn with one carry it.
    pub fn add_drive_mesh_surf(&mut self, mesh: &MeshData, transform: &Mat4, origin: DVec3, tx: i32, ty: i32, surf: Option<&SurfFaces>) {
        let ident = *transform == Mat4::IDENTITY;
        let off = (origin - DVec3::new(tx as f64 * tile_size(), ty as f64 * tile_size(), 0.0)).as_vec3();
        for (j, tri) in mesh.indices.chunks_exact(3).enumerate() {
            let mut p = [Vec3::ZERO; 3];
            for k in 0..3 {
                let v = mesh.positions[tri[k] as usize];
                p[k] = (if ident { v } else { transform.transform_point3(v) }) + off;
            }
            let pushed = match surf.and_then(|s| s.face(j, tri)) {
                Some((m, uv)) => self.drive.push_surf(p, uv, m),
                None => self.drive.push_kind(p, false),
            };
            if pushed {
                self.drive.tag_last(surf.map(|s| s.id(j)).unwrap_or(0));
            }
        }
    }

    /// Add a spline's height profiles ([`build_height_profile_mesh`]): its wall tops (the
    /// range of material 1) go in as walls.
    pub fn add_height_profiles(&mut self, mesh: &MeshData, origin: DVec3, tx: i32, ty: i32) {
        self.add_spline_drive(mesh, None, origin, tx, ty);
    }

    /// [`TileSurface::add_height_profiles`] for a spline's drawn mesh, with the `.surf` maps
    /// of its textures where it has any.
    pub fn add_spline_drive(&mut self, mesh: &MeshData, surf: Option<&SurfFaces>, origin: DVec3, tx: i32, ty: i32) {
        let off = (origin - DVec3::new(tx as f64 * tile_size(), ty as f64 * tile_size(), 0.0)).as_vec3();
        let ridge_from = mesh.ranges.iter().find(|r| r.2 == 1).map(|r| r.0 as usize).unwrap_or(usize::MAX);
        for (j, tri) in mesh.indices.chunks_exact(3).enumerate() {
            let p = [0, 1, 2].map(|k| mesh.positions[tri[k] as usize] + off);
            let pushed = match surf.and_then(|s| s.face(j, tri)) {
                Some((m, uv)) => self.drive.push_surf(p, uv, m),
                None => self.drive.push_kind(p, j * 3 >= ridge_from),
            };
            if pushed {
                self.drive.tag_last(surf.map(|s| s.id(j)).unwrap_or(0));
            }
        }
    }

    /// Build the wheel lookup once every mesh of the tile is in (and let go of the raster
    /// layers that repeat the top surface).
    pub fn finish(&mut self) {
        self.drive.build(tile_size() as f32);
        self.compact();
    }

    /// Rasterize a mesh into this tile at (tx, ty). Mesh positions are relative to
    /// `origin` (after `transform`).
    pub fn rasterize(&mut self, mesh: &MeshData, transform: &Mat4, origin: DVec3, tx: i32, ty: i32) {
        self.rasterize_kind(mesh, transform, origin, tx, ty, true)
    }

    /// Cut the ground inside an aligned spline's outline (world x, y) on tile (tx, ty).
    pub fn add_outline(&mut self, ring: &[DVec2], tx: i32, ty: i32) {
        let o = DVec2::new(tx as f64 * tile_size(), ty as f64 * tile_size());
        let pts: Vec<Vec2> = ring.iter().map(|p| (*p - o).as_vec2()).collect();
        let b = pts.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)]);
        let t = tile_size() as f32;
        if pts.len() < 3 || b[2] < -OUTLINE_EDGE || b[3] < -OUTLINE_EDGE || b[0] > t + OUTLINE_EDGE || b[1] > t + OUTLINE_EDGE {
            return;
        }
        self.outlines.push((pts, b));
    }

    /// Signed distance (m) from tile point (x, y) to the aligned splines' cut, negative
    /// inside it, as far as `OUTLINE_EDGE` (farther: None).
    fn outline_distance(&self, x: f32, y: f32) -> Option<f32> {
        let p = Vec2::new(x, y);
        let mut best: Option<f32> = None;
        for (ring, b) in &self.outlines {
            if x < b[0] - OUTLINE_EDGE || y < b[1] - OUTLINE_EDGE || x > b[2] + OUTLINE_EDGE || y > b[3] + OUTLINE_EDGE {
                continue;
            }
            let mut d2 = f32::MAX;
            let mut inside = false;
            let n = ring.len();
            for i in 0..n {
                let (a, c) = (ring[i], ring[(i + 1) % n]);
                let e = c - a;
                let t = ((p - a).dot(e) / e.length_squared().max(1e-12)).clamp(0.0, 1.0);
                d2 = d2.min((a + e * t - p).length_squared());
                if (a.y > y) != (c.y > y) && x < a.x + (y - a.y) / (c.y - a.y) * (c.x - a.x) {
                    inside = !inside;
                }
            }
            let d = if inside { -d2.sqrt() } else { d2.sqrt() };
            best = Some(best.map_or(d, |b: f32| b.min(d)));
        }
        best.filter(|d| *d < OUTLINE_EDGE)
    }

    /// Is tile point (x, y) inside an aligned spline's outline?
    fn in_outlines(&self, x: f32, y: f32) -> bool {
        self.outlines.iter().any(|(ring, b)| {
            if x < b[0] || y < b[1] || x > b[2] || y > b[3] {
                return false;
            }
            let mut inside = false;
            let n = ring.len();
            for i in 0..n {
                let (a, c) = (ring[i], ring[(i + 1) % n]);
                if (a.y > y) != (c.y > y) && x < a.x + (y - a.y) / (c.y - a.y) * (c.x - a.x) {
                    inside = !inside;
                }
            }
            inside
        })
    }

    /// Mark the texels a `[terrainhole]` mesh covers: there the ground goes away entirely.
    /// Its faces may be vertical walls, so nothing is skipped by orientation here.
    pub fn rasterize_hole(&mut self, mesh: &MeshData, transform: &Mat4, origin: DVec3, tx: i32, ty: i32) {
        let n = self.size as f32;
        let scale = n / tile_size() as f32;
        let ident = *transform == Mat4::IDENTITY;
        let off = (origin - DVec3::new(tx as f64 * tile_size(), ty as f64 * tile_size(), 0.0)).as_vec3();
        let (ox, oy) = (-off.x, -off.y);
        // the triangles in texel units (x, y) with their heights, walls left out
        let mut tris: Vec<[Vec3; 3]> = Vec::new();
        for tri in mesh.indices.chunks_exact(3) {
            let mut p = [Vec3::ZERO; 3];
            for k in 0..3 {
                let v = mesh.positions[tri[k] as usize];
                let w = if ident { v } else { transform.transform_point3(v) };
                p[k] = Vec3::new((w.x - ox) * scale, (w.y - oy) * scale, w.z + off.z);
            }
            let det = (p[1].x - p[0].x) * (p[2].y - p[0].y) - (p[2].x - p[0].x) * (p[1].y - p[0].y);
            if det.abs() >= 1e-9 {
                tris.push(p);
            }
        }
        // The cutter's rim: the edges only one of its faces has (by position, a mesh repeats
        // its vertices per face). Just inside it the ground is kept (`HOLE_KEEP`), as along
        // an aligned spline's outline: a junction's cutter reaches a little past its kerbs
        // and footways, and cut that far the ground left the sky showing along them.
        let key = |v: Vec3| ((v.x * 256.0).round() as i64, (v.y * 256.0).round() as i64);
        let mut edges: std::collections::HashMap<((i64, i64), (i64, i64)), (u32, Vec2, Vec2)> = std::collections::HashMap::new();
        for p in &tris {
            for k in 0..3 {
                let (a, b) = (p[k], p[(k + 1) % 3]);
                let (ka, kb) = (key(a), key(b));
                if ka == kb {
                    continue;
                }
                let e = if ka < kb { (ka, kb) } else { (kb, ka) };
                edges.entry(e).or_insert((0, a.truncate(), b.truncate())).0 += 1;
            }
        }
        let rim: Vec<(Vec2, Vec2)> = edges.into_values().filter(|e| e.0 == 1).map(|e| (e.1, e.2)).collect();
        let keep = HOLE_KEEP * scale;
        let near_rim = |q: Vec2| {
            rim.iter().any(|(a, c)| {
                let e = *c - *a;
                let t = ((q - *a).dot(e) / e.length_squared().max(1e-12)).clamp(0.0, 1.0);
                (*a + e * t - q).length_squared() < keep * keep
            })
        };
        for p in &tris {
            let (x0, y0, x1, y1, x2, y2) = (p[0].x, p[0].y, p[1].x, p[1].y, p[2].x, p[2].y);
            let minx = x0.min(x1).min(x2).floor().max(0.0) as i32;
            let maxx = x0.max(x1).max(x2).ceil().min(n - 1.0) as i32;
            let miny = y0.min(y1).min(y2).floor().max(0.0) as i32;
            let maxy = y0.max(y1).max(y2).ceil().min(n - 1.0) as i32;
            if minx > maxx || miny > maxy {
                continue;
            }
            let inv = 1.0 / ((x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0));
            let eps = -0.002;
            for py in miny..=maxy {
                let cy = py as f32 + 0.5;
                for px in minx..=maxx {
                    let cx = px as f32 + 0.5;
                    let l1 = ((x1 - x0) * (cy - y0) - (cx - x0) * (y1 - y0)) * inv;
                    let l2 = ((cx - x0) * (y2 - y0) - (x2 - x0) * (cy - y0)) * inv;
                    let l0 = 1.0 - l1 - l2;
                    let i = py as usize * self.size + px as usize;
                    if l0 >= eps && l1 >= eps && l2 >= eps {
                        let h = l0 * p[0].z + l2 * p[1].z + l1 * p[2].z;
                        self.add_hole(i, h);
                    }
                    // the texel's 4×4 points the triangle covers, short of the rim
                    let mut bits = 0u16;
                    for q in 0..16 {
                        let sx = px as f32 + ((q % 4) as f32 + 0.5) / 4.0;
                        let sy = py as f32 + ((q / 4) as f32 + 0.5) / 4.0;
                        let m1 = ((x1 - x0) * (sy - y0) - (sx - x0) * (y1 - y0)) * inv;
                        let m2 = ((sx - x0) * (y2 - y0) - (x2 - x0) * (sy - y0)) * inv;
                        if m1 >= eps && m2 >= eps && 1.0 - m1 - m2 >= eps && !near_rim(Vec2::new(sx, sy)) {
                            bits |= 1 << q;
                        }
                    }
                    if bits != 0 {
                        self.add_hole_cover(i, bits);
                    }
                }
            }
        }
    }

    /// `drivable`: this surface also carries vehicles (a road, a crossing plate, a car park).
    pub fn rasterize_kind(&mut self, mesh: &MeshData, transform: &Mat4, origin: DVec3, tx: i32, ty: i32, drivable: bool) {
        let n = self.size as f32;
        let scale = n / tile_size() as f32;
        let ident = *transform == Mat4::IDENTITY;
        // offset from the mesh origin to this tile's corner, in f64
        let off = (origin - DVec3::new(tx as f64 * tile_size(), ty as f64 * tile_size(), 0.0)).as_vec3();
        let (ox, oy) = (-off.x, -off.y);
        for tri in mesh.indices.chunks_exact(3) {
            let mut p = [Vec3::ZERO; 3];
            for k in 0..3 {
                let v = mesh.positions[tri[k] as usize];
                p[k] = if ident { v } else { transform.transform_point3(v) };
                p[k].z += off.z;
            }
            // skip (near-)vertical faces: they are walls, not surfaces
            let nrm = (p[1] - p[0]).cross(p[2] - p[0]);
            let len = nrm.length();
            if len < 1e-6 || nrm.z.abs() / len < 0.3 {
                continue;
            }
            let (x0, y0) = ((p[0].x - ox) * scale, (p[0].y - oy) * scale);
            let (x1, y1) = ((p[1].x - ox) * scale, (p[1].y - oy) * scale);
            let (x2, y2) = ((p[2].x - ox) * scale, (p[2].y - oy) * scale);
            let minx = x0.min(x1).min(x2).floor().max(0.0) as i32;
            let maxx = x0.max(x1).max(x2).ceil().min(n - 1.0) as i32;
            let miny = y0.min(y1).min(y2).floor().max(0.0) as i32;
            let maxy = y0.max(y1).max(y2).ceil().min(n - 1.0) as i32;
            if minx > maxx || miny > maxy {
                continue;
            }
            let det = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0);
            if det.abs() < 1e-9 {
                continue;
            }
            let inv = 1.0 / det;
            for py in miny..=maxy {
                let cy = py as f32 + 0.5;
                for px in minx..=maxx {
                    let cx = px as f32 + 0.5;
                    let l1 = ((x1 - x0) * (cy - y0) - (cx - x0) * (y1 - y0)) * inv; // weight of p2
                    let l2 = ((cx - x0) * (y2 - y0) - (x2 - x0) * (cy - y0)) * inv; // weight of p1
                    let l0 = 1.0 - l1 - l2;
                    let eps = -0.002;
                    if l0 >= eps && l1 >= eps && l2 >= eps {
                        let i = py as usize * self.size + px as usize;
                        let h = l0 * p[0].z + l2 * p[1].z + l1 * p[2].z;
                        self.add_surface(i, h, drivable);
                    }
                }
            }
        }
    }

    /// Surface height at tile-local metres, if covered.
    pub fn sample(&self, x: f32, y: f32) -> Option<f32> {
        let i = self.texel(x, y);
        self.covered(i).then(|| self.height(i))
    }

    /// The raster height and the tile-local coordinates where it was measured.
    pub fn sample_at_texel_center(&self, x: f32, y: f32) -> Option<(f32, f32, f32)> {
        let i = self.texel(x, y);
        self.covered(i).then(|| {
            let cell = tile_size() as f32 / self.size as f32;
            let x = ((i % self.size) as f32 + 0.5) * cell;
            let y = ((i / self.size) as f32 + 0.5) * cell;
            (x, y, self.height(i))
        })
    }

    /// Height of the drivable surface here, if any: what the wheels stand on.
    pub fn sample_road(&self, x: f32, y: f32) -> Option<f32> {
        let i = self.texel(x, y);
        self.road_covered(i).then(|| self.road_height(i))
    }

    /// Height of the drivable surface (else of any surface) here, blended between the four
    /// nearest covered texels. A texel is 0.7 m on a Berlin tile, and a wheel reading the
    /// nearest one climbs a slope in centimetre steps - the AI cars shook on every hill.
    pub fn sample_road_smooth(&self, x: f32, y: f32) -> Option<f32> {
        self.blend(x, y, true).or_else(|| self.blend(x, y, false))
    }

    fn blend(&self, x: f32, y: f32, road: bool) -> Option<f32> {
        let n = self.size as i32;
        let cell = tile_size() as f32 / self.size as f32;
        let (fx, fy) = (x / cell - 0.5, y / cell - 0.5);
        let (ix, iy) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - ix, fy - iy);
        let (mut acc, mut sum) = (0.0f32, 0.0f32);
        for (dx, dy, w) in [(0, 0, (1.0 - tx) * (1.0 - ty)), (1, 0, tx * (1.0 - ty)), (0, 1, (1.0 - tx) * ty), (1, 1, tx * ty)] {
            let cx = (ix as i32 + dx).clamp(0, n - 1);
            let cy = (iy as i32 + dy).clamp(0, n - 1);
            let k = (cy * n + cx) as usize;
            let (covered, height) = if road { (self.road_covered(k), self.road_height(k)) } else { (self.covered(k), self.height(k)) };
            if covered && w > 0.0 {
                acc += height * w;
                sum += w;
            }
        }
        (sum > 0.05).then(|| acc / sum)
    }

    pub(crate) fn texel(&self, x: f32, y: f32) -> usize {
        let n = self.size as f32;
        let fx = (x / tile_size() as f32 * n).clamp(0.0, n - 1.0);
        let fy = (y / tile_size() as f32 * n).clamp(0.0, n - 1.0);
        fy as usize * self.size + fx as usize
    }

    /// The terrain mask as an RGBA image: alpha 255 where the terrain is visible.
    /// Alpha mask that cuts the terrain away under flush surfaces (255 = keep the terrain).
    ///
    /// Only texels whose surface lies within `flush` metres of the terrain are cut: a road
    /// that sits on a kerb 25 cm above the grass needs no hole, and cutting one there left a
    /// gap you could see the sky through. The cut is eroded by one texel as well, so the
    /// hole always stays inside the surface even after the mask is filtered.
    pub fn mask_image(&self, terrain_at: &dyn Fn(f32, f32) -> f32, flush: f32) -> Vec<u8> {
        let n = self.size;
        let cell = tile_size() as f32 / n as f32;
        let mut cut = vec![false; n * n];
        for j in 0..n {
            for i in 0..n {
                let k = j * n + i;
                if self.hole_height(k).is_some() {
                    // An authored excavation can lie more than a storey below the ground.
                    cut[k] = true;
                    continue;
                }
                if !self.covered(k) {
                    continue;
                }
                let t = terrain_at((i as f32 + 0.5) * cell, (j as f32 + 0.5) * cell);
                cut[k] = self.cuts(k, t, flush);
            }
        }
        let mut out = vec![255u8; n * n * 4];
        // the aligned splines' outlines: how much of each texel keeps its ground (4×4
        // samples where an edge passes through it). Filtered, the half-way value lies on the
        // outline; and a strip of ground between two holes narrower than two texels (a
        // median, 0.96 m on the Falkenseer Chaussee) keeps its full value, where a distance
        // field's ridge sank to the cut-off and let the sky through in lens-shaped patches.
        if !self.outlines.is_empty() {
            let (mut lo, mut hi) = ([usize::MAX; 2], [0usize; 2]);
            for (_, b) in &self.outlines {
                lo[0] = lo[0].min(((b[0] - OUTLINE_EDGE) / cell).floor().max(0.0) as usize);
                lo[1] = lo[1].min(((b[1] - OUTLINE_EDGE) / cell).floor().max(0.0) as usize);
                hi[0] = hi[0].max((((b[2] + OUTLINE_EDGE) / cell).ceil().max(0.0) as usize).min(n));
                hi[1] = hi[1].max((((b[3] + OUTLINE_EDGE) / cell).ceil().max(0.0) as usize).min(n));
            }
            for j in lo[1]..hi[1] {
                for i in lo[0]..hi[0] {
                    let (cx, cy) = ((i as f32 + 0.5) * cell, (j as f32 + 0.5) * cell);
                    let Some(d) = self.outline_distance(cx, cy) else { continue };
                    let a = if d < -cell * 0.75 {
                        0
                    } else if d > cell * 0.75 {
                        255
                    } else {
                        let kept = (0..16)
                            .filter(|q| {
                                let (x, y) = (cx + ((q % 4) as f32 - 1.5) * cell / 4.0, cy + ((q / 4) as f32 - 1.5) * cell / 4.0);
                                !self.in_outlines(x, y) || self.outline_distance(x, y).is_some_and(|d| d > -OUTLINE_KEEP)
                            })
                            .count();
                        (kept * 255 / 16) as u8
                    };
                    out[(j * n + i) * 4 + 3] = a;
                }
            }
        }
        for j in 0..n {
            for i in 0..n {
                // erode: cut only where the whole 3×3 neighbourhood is cut
                let all = (-1i32..=1).all(|dj| {
                    (-1i32..=1).all(|di| {
                        let (x, y) = (i as i32 + di, j as i32 + dj);
                        x < 0 || y < 0 || x >= n as i32 || y >= n as i32 || cut[y as usize * n + x as usize]
                    })
                });
                if all && cut[j * n + i] {
                    out[(j * n + i) * 4 + 3] = 0;
                }
            }
        }
        // the `[terrainhole]` cutters: each texel keeps the part of its ground the cutter
        // leaves (filtered, the half-way value lies on the cutter's rim), not eroded
        for k in self.touched() {
            let c = self.hole_coverage(k);
            if c != 0 {
                let kept = (255 * (16 - c.count_ones()) / 16) as u8;
                let a = &mut out[k * 4 + 3];
                *a = (*a).min(kept);
            }
        }
        out
    }

    /// Would the ground be taken away at this point? (the per-texel decision `mask_image`
    /// makes, for checks that need to know whether a road is visible)
    pub fn cut_at(&self, x: f32, y: f32, terrain_h: f32, flush: f32) -> bool {
        if self.in_outlines(x, y) {
            return true;
        }
        let i = self.texel(x, y);
        if self.hole_height(i).is_some() {
            return true;
        }
        self.cuts(i, terrain_h, flush)
    }

    /// The ground goes where a surface crosses it: nothing more than `flush` above it (a
    /// bridge or an embankment keeps its ground). Below it, a road or footway may lie up to
    /// a storey deep - a sunken road or an underpass still shows - but anything else only
    /// takes the ground away where it is flush with it: the far slope of a railway
    /// embankment runs on under the grass (Berlin-Spandau's `Damm1.sli` down to 16 m below
    /// it), and cutting the ground over it opened a band you could see the sky through.
    /// A plate a mapper left thirty metres down takes nothing either.
    pub fn cuts(&self, k: usize, t: f32, flush: f32) -> bool {
        if !road_cut() || !self.covered(k) || self.low_height(k) - flush > t {
            return false;
        }
        if self.road_covered(k) && t <= self.road_height(k) + DEEP_CUT {
            return true;
        }
        t <= self.height(k) + flush
    }

    /// The texels some block of the raster holds (surfaces or holes), in order: the others
    /// are empty.
    fn touched(&self) -> impl Iterator<Item = usize> + '_ {
        let per_row = self.size.div_ceil(BLOCK);
        (0..self.blocks.len()).filter(|b| self.blocks[*b].is_some() || self.holes[*b].is_some() || self.hole_cover[*b].is_some()).flat_map(move |b| {
            let (bx, by) = (b % per_row, b / per_row);
            (0..BLOCK * BLOCK).filter_map(move |i| {
                let (x, y) = (bx * BLOCK + i % BLOCK, by * BLOCK + i / BLOCK);
                (x < self.size && y < self.size).then_some(y * self.size + x)
            })
        })
    }

    /// Is any texel actually cut? (`mask_image` with the same arguments would do something.)
    /// (Only the map's own holes - `[terrainhole]` meshes, the splines' `[terrainholeprofile]`
    /// - unless `road_cut`.)
    pub fn cuts_anything(&self, terrain_at: &dyn Fn(f32, f32) -> f32, flush: f32) -> bool {
        if !self.outlines.is_empty() {
            return true;
        }
        let n = self.size;
        let cell = tile_size() as f32 / n as f32;
        self.touched().any(|k| {
            let (i, j) = (k % n, k / n);
            let t = terrain_at((i as f32 + 0.5) * cell, (j as f32 + 0.5) * cell);
            self.hole_height(k).is_some() || (self.covered(k) && self.cuts(k, t, flush))
        })
    }
}
