//! The ground of the loaded tiles - terrain and the road faces over it - and the questions
//! asked of it: what a wheel stands on (`drive_probe`), what is drawn, where somebody on foot
//! stands, whether a puddle could lie there.

use super::{WorldCell, WorldData};
use crate::rigid::GroundProbe;
use glam::DVec3;
use hashbrown::HashMap;
use omsi_geometry::TileSurface;
use omsi_map::{Terrain, tile_size};
use std::sync::Arc;

/// The ground of the loaded tiles: terrain and surface rasters (roads) by tile coordinate.
/// A tile's terrain comes when it is placed, its surface when its ground has been cut.
#[derive(Clone, Default)]
pub struct GroundTiles {
    pub terrains: HashMap<(i32, i32), Arc<Terrain>>,
    pub surfaces: HashMap<(i32, i32), Arc<TileSurface>>,
}

/// Faces this close over another road face are paint on it, not a step (m). Omsi.exe's
/// ground query (0x7a0814) takes the highest face whatever lies under it; this keeps only
/// the thinnest layers flat (a marking a centimetre or two over the asphalt). At 4.5 cm it
/// also took the speed cushions, manhole and plate objects, lowered kerbs and slab edges
/// away, and the bottom 4.5 cm of every speed bump's ramp - "no road bumps", and wheels
/// drawn sunk into what they drove on.
pub const PAINT_LAYER: f32 = 0.02;

/// How far drawn ground may poke through a road face and the wheel still stands on the
/// road (m): the coarse terrain grid rising a few centimetres through the asphalt between its
/// samples, which Omsi.exe's highest face would turn into bumps.
pub const GROUND_THROUGH_ROAD: f32 = 0.1;

/// How far a road face may lie under drawn ground before it counts as buried (m): from here
/// the wheel stands on the ground, as on Omsi.exe's highest face. Between this and
/// [`GROUND_THROUGH_ROAD`] it rises smoothly from the one to the other, so no step is met
/// where the ground slowly covers a road. (1 m once: a footway 0.77 m under Quellengrunder
/// Straße on Ahlheim & Laurenzbach sank the buses that far into the street.)
pub const BURIED_FACE: f32 = 0.3;

/// How far over the ground a wall's top must stand to be a wall to the wheels (a kerb is
/// less, and the tyre climbs it).
pub const WALL_TOP_STEP: f32 = 0.3;

/// How far a road surface may ride above the ground and still have the ground cut away
/// under it (`OMSI_SURFACE_FLUSH`, 0.12 m). Anything higher is a bridge or an embankment,
/// where cutting would open a hole.
pub fn surface_flush() -> f32 {
    // (global: an environment setting read once per process; asked by the ground cut on
    // the staging threads and by the probes, which run without a world)
    static V: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *V.get_or_init(|| omsi_cfg::flags::OMSI_SURFACE_FLUSH.parse().unwrap_or(0.12))
}

/// The tile that holds world (x, y).
pub fn tile_key(x: f64, y: f64) -> (i32, i32) {
    (
        (x / tile_size()).floor() as i32,
        (y / tile_size()).floor() as i32,
    )
}

/// World (x, y) within tile `key`.
fn local(key: (i32, i32), x: f64, y: f64) -> (f32, f32) {
    (
        (x - key.0 as f64 * tile_size()) as f32,
        (y - key.1 as f64 * tile_size()) as f32,
    )
}

impl GroundTiles {
    /// The surface and the terrain of tile `key`.
    pub fn tile(&self, key: (i32, i32)) -> (Option<&Arc<TileSurface>>, Option<&Arc<Terrain>>) {
        (self.surfaces.get(&key), self.terrains.get(&key))
    }

    /// What a wheel stands on at world (x, y): the faces of the roads, crossings and surface
    /// objects there, and the terrain wherever it is not cut away under them - the highest at
    /// or below `top`, and the lowest above it (a kerb the tyre is up against).
    pub fn drive_probe(&self, x: f64, y: f64, top: f64) -> GroundProbe {
        let key = tile_key(x, y);
        let (surface, terrain) = self.tile(key);
        probe_tile(surface.map(|s| &**s), terrain.map(|t| &**t), key, x, y, top)
    }

    /// What is drawn at world (x, y) under `top`: the highest face of the splines and surface
    /// objects (as lifted for drawing) and the terrain where it is not cut away - the picture's
    /// ground, without any of the wheel rules of [`GroundTiles::drive_probe`]
    /// (`OMSI_GROUND_GAP` measures the tyres against it).
    pub fn drawn_ground(&self, x: f64, y: f64, top: f64) -> Option<f64> {
        let key = tile_key(x, y);
        let (lx, ly) = local(key, x, y);
        let (surface, terrain) = self.tile(key);
        let mut best: Option<f32> = None;
        if let Some(s) = surface {
            let a = s.drive.probe(lx, ly, top as f32).below;
            let b = s.drive.probe_walls(lx, ly, top as f32).below;
            best = a.into_iter().chain(b).reduce(f32::max);
        }
        if let Some(t) = terrain {
            let h = omsi_geometry::terrain_height(t, lx, ly);
            let cut = surface.is_some_and(|s| s.cut_at(lx, ly, h, surface_flush()));
            if !cut && h <= top as f32 {
                best = Some(best.map_or(h, |b| b.max(h)));
            }
        }
        best.map(|z| z as f64)
    }
}

/// [`GroundTiles::drive_probe`] on the tile `key` that holds (x, y).
pub fn probe_tile(
    surface: Option<&TileSurface>,
    terrain: Option<&Terrain>,
    key: (i32, i32),
    x: f64,
    y: f64,
    top: f64,
) -> GroundProbe {
    let (lx, ly) = local(key, x, y);
    let mut probe = omsi_geometry::Probe::default();
    let mut zs = [0f32; 64];
    let mut walls = None;
    let n = surface.map_or(0, |s| s.drive.heights(lx, ly, &mut zs, &mut walls));
    if let Some(s) = surface {
        let below = |lim: f32| {
            if n > zs.len() {
                s.drive.probe(lx, ly, lim)
            } else {
                zs[..n]
                    .iter()
                    .fold(omsi_geometry::Probe::default(), |p, &z| {
                        p.merge(omsi_geometry::Probe::of(z, lim))
                    })
            }
        };
        probe = below(top as f32);
        // a painted layer is no step: road markings made as `[surface]` objects or as
        // splines with a height profile lie a centimetre or three over the asphalt, and the
        // wheels climbed every line - the bus hopped at stops and over dotted lines (Horizon).
        // Where another road face lies that little below, the wheel stands on that one.
        // (only road faces: the terrain under a road is often that close too)
        // (layer under layer: a marking over a marking over the asphalt, as the map editor
        // stacks them where lines cross or a box junction lies over a lane's arrows, was
        // still a step when only the first was looked through)
        let mut layers = 0;
        while let Some(z1) = probe.below.filter(|_| layers < 4) {
            layers += 1;
            match below(z1 - 0.0005).below {
                Some(z2) if z1 - z2 < PAINT_LAYER => probe.below = Some(z2),
                _ => break,
            }
        }
    }
    // On a road the wheel stands on the road, as in OMSI: the terrain under it or over it
    // (an embankment the road runs under, ground poking through the asphalt) is no
    // ground and no wall there. Taken with the road, a terrain face over the carriageway was
    // an invisible wall under bridges, and one through it a bump that threw the bus.
    let ground = terrain.map(|t| {
        let h = omsi_geometry::terrain_height(t, lx, ly);
        let cut = surface
            .map(|s| s.cut_at(lx, ly, h, surface_flush()))
            .unwrap_or(false);
        (h, cut)
    });
    // ... unless that face lies buried well under ground that is drawn here and is under the
    // wheel, not over it: the lower slope of an embankment spline (Marcel's `Damm1` falls
    // 20 m over 30 m on each side) reaching under a junction the terrain carries. Omsi.exe
    // takes the highest face there, the ground; taken as the road, it dropped the bus 8 m
    // through the asphalt into the slope (Cotterell, the junction by the park at 250, 427).
    let sunk = match (probe.below, ground) {
        (Some(z), Some((h, false))) if h <= top as f32 => h - z,
        _ => 0.0,
    };
    let buried = sunk >= BURIED_FACE;
    if let (Some(z), false) = (probe.below, buried) {
        if sunk > GROUND_THROUGH_ROAD {
            let t = (sunk - GROUND_THROUGH_ROAD) / (BURIED_FACE - GROUND_THROUGH_ROAD);
            probe.below = Some(z + sunk * t * t * (3.0 - 2.0 * t));
        }
    }
    let on_road = probe.below.is_some() && !buried;
    if let (Some((h, cut)), false) = (ground, on_road) {
        // the ground counts where it is drawn; where it is cut away and nothing else is
        // there (a surface without a collision), it still carries rather than let the
        // vehicle drop out of the world
        if !cut || (probe.below.is_none() && h <= top as f32) {
            probe = probe.merge(omsi_geometry::Probe::of(h, top as f32));
        }
    }
    // A wall's top (a narrow height profile high on a wall spline) is never stood on: where
    // it stands a step over the ground here it is a wall the tyre meets, whatever the height
    // it is probed from; nearer the ground than that the wheel rolls on the road beside it
    // (where the wall's top met the road the wheels went up onto it and rode along it)
    if let Some(s) = surface {
        let walls = if n > zs.len() {
            s.drive.probe_walls(lx, ly, f32::MAX).below
        } else {
            walls
        };
        if let (Some(zw), Some(g)) = (walls, probe.below) {
            if zw > g + WALL_TOP_STEP {
                probe.above = Some(probe.above.map_or(zw, |a| a.min(zw)));
            }
        }
    }
    GroundProbe {
        below: probe.below.map(|z| z as f64),
        above: probe.above.map(|z| z as f64),
    }
}

impl WorldData {
    /// [`GroundTiles::drive_probe`] of the loaded tiles.
    pub fn drive_probe(&self, x: f64, y: f64, top: f64) -> GroundProbe {
        self.ground.drive_probe(x, y, top)
    }

    /// [`GroundTiles::drawn_ground`] of the loaded tiles.
    pub fn drawn_ground(&self, x: f64, y: f64, top: f64) -> Option<f64> {
        self.ground.drawn_ground(x, y, top)
    }

    /// Terrain height alone at world x, y (no road surfaces).
    pub fn ground_terrain(&self, x: f64, y: f64) -> Option<f64> {
        let key = tile_key(x, y);
        let (lx, ly) = local(key, x, y);
        Some(self.ground.terrains.get(&key)?.sample(lx, ly) as f64)
    }

    /// Ground height (road surface where present, else terrain) at world x, y.
    pub fn ground_height(&self, x: f64, y: f64) -> Option<f64> {
        let key = tile_key(x, y);
        let (lx, ly) = local(key, x, y);
        if let Some(s) = self.ground.surfaces.get(&key) {
            // the road the wheels stand on, not a bridge deck or an embankment over it
            if let Some(h) = s.sample_road(lx, ly).or_else(|| s.sample(lx, ly)) {
                return Some(h as f64);
            }
        }
        Some(self.ground.terrains.get(&key)?.sample(lx, ly) as f64)
    }

    /// The height a vehicle put down at (x, y) stands at: the face its wheels would stand on
    /// (a road, a deck, a floor, the ground; [`WorldData::drive_probe`]) under `near` + 1.5 m
    /// and at most 3 m below it. The raster's [`WorldData::ground_height`] takes the surface
    /// of its texel, and a bus put down beside a wall (an entry point on a pavement, London)
    /// stood on the wall's top and floated there.
    pub fn stand_height(&self, x: f64, y: f64, near: f64) -> Option<f64> {
        self.drive_probe(x, y, near + 1.5)
            .below
            .filter(|g| near - g < 3.0)
    }

    /// The ground under a point of the outside camera's arm: the highest face of the roads,
    /// crossings, surface objects and terrain at or below `top`. A face higher up - the
    /// roof over a petrol station's forecourt, a bridge deck - is not the ground there (the
    /// top surface of the raster is, and it put the camera on the canopy); a roof's mesh
    /// stops the camera instead.
    pub fn camera_ground(&self, x: f64, y: f64, top: f64) -> Option<f64> {
        self.drive_probe(x, y, top).below
    }

    /// Whether the ground at world (x, y) is loaded.
    pub fn has_ground(&self, x: f64, y: f64) -> bool {
        self.ground.terrains.contains_key(&tile_key(x, y))
    }

    /// The wetness a puddle would use at world (x, y): `wetness` where a road surface is
    /// under the point (the same `[moisture]` ground `enhanced.wgsl`'s reflective puddle
    /// patches sit on), 0 on bare terrain or where no surface is loaded there yet. Approximate
    /// on purpose - `puddles::water_at` only needs to agree with the shader's own mask
    /// closely enough that a tyre's spray starts where the reflection does, not to the texel.
    pub fn wet_road_at(&self, x: f64, y: f64, wetness: f32) -> f32 {
        let key = tile_key(x, y);
        let (lx, ly) = local(key, x, y);
        let on_road = self
            .ground
            .surfaces
            .get(&key)
            .is_some_and(|s| s.sample_road(lx, ly).is_some());
        if on_road {
            wetness.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Local visible road plane under a vehicle: the exact faces where there are any (raster
    /// heights are coarser). Choose the nearby deck, never a roof above it.
    pub fn puddle_surface(&self, position: DVec3) -> Option<(f64, glam::Vec3)> {
        let height = self.camera_ground(position.x, position.y, position.z + 0.35)?;
        let key = tile_key(position.x, position.y);
        let (x, y) = local(key, position.x, position.y);
        let normal = self
            .ground
            .surfaces
            .get(&key)
            .and_then(|s| s.drive.surface_below(x, y, height as f32 + 0.002))
            .filter(|(z, _)| (*z as f64 - height).abs() < 0.005)
            .map(|(_, n)| n)
            .unwrap_or(glam::Vec3::Z);
        Some((height, normal))
    }

    /// Height for somebody on foot: the top of whatever is here - a pavement, a platform,
    /// a painted yard - and the bare ground where there is nothing. [`WorldData::ground_height`]
    /// is the wheels' answer instead: it picks the drivable surface, which is the road *under*
    /// the kerb, and standing people on that buried them to the ankles in the pavement.
    pub fn walk_height(&self, x: f64, y: f64) -> Option<f64> {
        let key = tile_key(x, y);
        let (lx, ly) = local(key, x, y);
        let surface = self
            .ground
            .surfaces
            .get(&key)
            .and_then(|s| s.sample(lx, ly))
            .map(|h| h as f64);
        let terrain = self
            .ground
            .terrains
            .get(&key)
            .map(|t| t.sample(lx, ly) as f64);
        let rough = match (surface, terrain) {
            (Some(s), Some(t)) => Some(s.max(t)),
            (s, t) => s.or(t),
        }?;
        // The raster says roughly where the floor is (a texel is 0.7 m on a Berlin tile, and
        // it holds the highest surface in it): the faces themselves say exactly. Read from the
        // raster, people stood 15 cm up in the air beside a kerb or sank into it, and climbed
        // every slope in steps. The highest face a little over the raster's height is taken:
        // the kerb's top on the pavement, the carriageway beside it.
        let probe = self.drive_probe(x, y, rough + 0.3);
        match probe.below {
            Some(b) if rough - (b as f64) < 1.0 => Some(b as f64),
            _ => Some(rough),
        }
    }

    /// The floor under somebody at height `near` at (x, y): the highest face no more than a
    /// step (0.5 m, Omsi.exe 0x630498) over them - a station's floor under its roof, a car
    /// park's level under the deck above - else [`WorldData::walk_height`]'s highest one.
    /// (Asked for the highest, the people of an indoor station stood on its roof.)
    ///
    /// Nothing under them within 3 m: the highest face, but only up to 0.5 m over them - a
    /// pavement whose tile came after them. Omsi.exe keeps its people at the heights of
    /// their paths and waiting places; the highest face, a bus shelter's roof 2.5 m up, put
    /// the people waiting under it on top of it.
    pub fn walk_height_near(&self, x: f64, y: f64, near: f64) -> Option<f64> {
        self.walk_height_reach(x, y, near, 0.5)
    }

    /// [`WorldData::walk_height_near`] that also sees faces up to `reach` over `near`: the
    /// walker on foot looks a metre up to stop at a face too high to step onto (a platform's
    /// edge) instead of walking under it.
    pub fn walk_height_reach(&self, x: f64, y: f64, near: f64, reach: f64) -> Option<f64> {
        let probe = self.drive_probe(x, y, near + reach);
        match probe.below {
            Some(b) if near - b < 3.0 => Some(b),
            _ => self
                .walk_height(x, y)
                .filter(|z| *z < near + reach.max(0.5)),
        }
    }
}

/// The ground the wheels of a vehicle stand on: the loaded tiles of the world as they are
/// at each step (a vehicle keeps it for its life; tiles come and go under it).
pub struct LiveGround(pub Arc<WorldCell>);

type TileRefs = ((i32, i32), Option<Arc<TileSurface>>, Option<Arc<Terrain>>);

impl crate::rigid::Ground for LiveGround {
    fn probe(&self, x: f64, y: f64, top: f64) -> GroundProbe {
        self.0.peek().drive_probe(x, y, top)
    }

    /// The world is taken once per step, and the tiles under the vehicle looked up once, not
    /// for every one of the few hundred points its tyres ask for.
    fn session(&self) -> Box<dyn Fn(f64, f64, f64) -> GroundProbe + '_> {
        let world = self.0.snapshot();
        let tiles: std::cell::RefCell<Vec<TileRefs>> =
            std::cell::RefCell::new(Vec::with_capacity(4));
        Box::new(move |x, y, top| {
            let key = tile_key(x, y);
            let mut tiles = tiles.borrow_mut();
            let i = match tiles.iter().position(|t| t.0 == key) {
                Some(i) => i,
                None => {
                    let (s, t) = world.ground.tile(key);
                    tiles.push((key, s.cloned(), t.cloned()));
                    tiles.len() - 1
                }
            };
            let (_, surface, terrain) = &tiles[i];
            probe_tile(surface.as_deref(), terrain.as_deref(), key, x, y, top)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Mat4;

    /// A flat road face 20 m square at height `z` in tile (0, 0), and the terrain at `h`.
    fn tile(z: f32, h: f32) -> (TileSurface, Terrain) {
        let x = format!(
            "xof 0303txt 0032\nMesh {{\n4;\n100;{z};100;,\n120;{z};100;,\n100;{z};120;,\n120;{z};120;;\n2;\n3;0,2,1;,\n3;1,2,3;;\n}}\n"
        );
        let road = omsi_geometry::mesh_from_o3d(&omsi_o3d::xfile::parse_x(x.as_bytes()).unwrap());
        let mut surface = TileSurface::new(128);
        surface.add_drive_mesh(&road, &Mat4::IDENTITY, DVec3::ZERO, 0, 0);
        surface.finish();
        let mut terrain = Terrain::flat();
        terrain.heights.iter_mut().for_each(|v| *v = h);
        (surface, terrain)
    }

    fn wheel(z: f32, h: f32) -> f64 {
        let (s, t) = tile(z, h);
        probe_tile(Some(&s), Some(&t), (0, 0), 110.0, 110.0, 5.0)
            .below
            .unwrap()
    }

    /// Ground drawn over a road face carries the wheel as Omsi.exe's highest face does - a
    /// footway 0.77 m under Quellengrunder Straße (Ahlheim & Laurenzbach) sank the buses into
    /// the street - while ground poking a few centimetres through the asphalt stays no bump,
    /// and in between the wheel rises without a step.
    #[test]
    fn drawn_ground_over_a_buried_road_face_carries_the_wheel() {
        assert!(
            (wheel(-0.67, 0.1) - 0.1).abs() < 1e-4,
            "{}",
            wheel(-0.67, 0.1)
        );
        assert!((wheel(0.0, 0.06) - 0.0).abs() < 1e-4);
        assert!((wheel(0.0, BURIED_FACE) - BURIED_FACE as f64).abs() < 1e-4);
        let mut last = 0.0;
        for i in 0..=40 {
            let h = i as f32 * 0.01;
            let z = wheel(0.0, h);
            assert!(z >= last - 1e-6 && z <= h as f64 + 1e-6, "{h}: {z}");
            assert!(z - last < 0.03, "a step at {h}: {last} -> {z}");
            last = z;
        }
        // the road over the ground stays the road
        assert!((wheel(0.5, 0.0) - 0.5).abs() < 1e-4);
    }
}
