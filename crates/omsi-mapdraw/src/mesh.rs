//! The map's own picture (`omsi_map::layers`, read off the map by `omsi_scene::mapgen`) as
//! triangles for the small map and the city map: the ground, water and woods, every paved
//! surface as it lies (with a fine edge round it, so a car park reads as one grey yard and
//! not as ten road ribbons), tracks, trees, and the houses - flat from far, standing in 3D with
//! lit walls and soft shadows from near.

use crate::style::{Palette, SHADOW_DIR};
use glam::{DVec2, DVec3, Vec2, Vec3};
use hashbrown::HashMap;
use omsi_map::layers::{self, Layers};
use omsi_ui::{Color, Painter};

/// The layers cut into square cells, so that the part around a place is found at once.
pub struct MapData {
    pub layers: std::sync::Arc<Layers>,
    cells: HashMap<(i32, i32), Cell>,
    /// Lines with their boxes (they run across many cells).
    lines: Vec<(Vec2, Vec2, usize)>,
    /// The 2 m cells a carriageway, a yard or a car park covers.
    paved_cells: hashbrown::HashSet<(i32, i32)>,
    /// The middle of each house model, in its own frame (walls face away from it).
    model_centre: Vec<Vec2>,
}

/// The most triangles of house models one build of the city map draws (nearest first); the other
/// houses are footprints with walls. The in-game panel, rebuilt as the bus drives and drawn every
/// frame, asks for fewer.
pub const MODEL_TRIANGLES: usize = 200_000;
/// Beyond this radius (m) of a build no model is drawn: from so high a roof is a speck.
const MODEL_RADIUS: f64 = 1500.0;
/// Cell of `MapData::paved_cells` (m).
const PAVED_CELL: f32 = 2.0;

#[derive(Default)]
struct Cell {
    land: Vec<Vec<u32>>,
    paved: Vec<Vec<u32>>,
    paint: Vec<Vec<u32>>,
    buildings: Vec<u32>,
    /// Placed house models (`Layers::instances`).
    instances: Vec<u32>,
    trees: Vec<u32>,
}

const CELL: f32 = 200.0;

fn cell_of(p: Vec2) -> (i32, i32) {
    ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
}

fn tri(all: &[Vec2], i: u32) -> &[Vec2] {
    &all[i as usize * 3..i as usize * 3 + 3]
}

/// What to draw of the ground.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GroundLook {
    /// Pavements and car parks as surfaces (near); else carriageways only.
    pub surfaces: bool,
    pub trees: bool,
    /// The railways' dashes (lost in a blur from far).
    pub dashes: bool,
    /// The least a road's line is simplified by, metres (0: as the build's reach makes it).
    pub tol: f32,
}

impl MapData {
    pub fn new(layers: std::sync::Arc<Layers>) -> MapData {
        let mut cells: HashMap<(i32, i32), Cell> = HashMap::new();
        let new_cell = || Cell { land: vec![Vec::new(); layers::LAND_CLASSES], paved: vec![Vec::new(); layers::PAVE_CLASSES], paint: vec![Vec::new(); layers::PAINT_CLASSES], ..Default::default() };
        for (class, tris) in (0..layers::LAND_CLASSES).map(|c| (c, layers.land_of(c))) {
            for (i, t) in tris.chunks_exact(3).enumerate() {
                let c = cell_of((t[0] + t[1] + t[2]) / 3.0);
                cells.entry(c).or_insert_with(new_cell).land[class].push(i as u32);
            }
        }
        for (class, tris) in (0..layers::PAVE_CLASSES).map(|c| (c, layers.paved_of(c))) {
            for (i, t) in tris.chunks_exact(3).enumerate() {
                let c = cell_of((t[0] + t[1] + t[2]) / 3.0);
                cells.entry(c).or_insert_with(new_cell).paved[class].push(i as u32);
            }
        }
        for (class, tris) in (0..layers::PAINT_CLASSES).map(|c| (c, layers.paint_of(c))) {
            for (i, t) in tris.chunks_exact(3).enumerate() {
                let c = cell_of((t[0] + t[1] + t[2]) / 3.0);
                cells.entry(c).or_insert_with(new_cell).paint[class].push(i as u32);
            }
        }
        for (i, b) in layers.buildings.iter().enumerate() {
            let c = cell_of(b.outline.iter().fold(Vec2::ZERO, |a, p| a + *p) / b.outline.len().max(1) as f32);
            cells.entry(c).or_insert_with(new_cell).buildings.push(i as u32);
        }
        for (i, t) in layers.trees.iter().enumerate() {
            cells.entry(cell_of(Vec2::new(t[0], t[1]))).or_insert_with(new_cell).trees.push(i as u32);
        }
        for (i, inst) in layers.instances.iter().enumerate() {
            cells.entry(cell_of(inst.at)).or_insert_with(new_cell).instances.push(i as u32);
        }
        let model_centre = layers
            .models
            .iter()
            .map(|m| {
                let (lo, hi) = m.iter().flatten().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(l, h), p| (l.min(p.truncate()), h.max(p.truncate())));
                if lo.x <= hi.x { (lo + hi) * 0.5 } else { Vec2::ZERO }
            })
            .collect();
        let lines = layers
            .lines
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let lo = l.points.iter().fold(Vec2::splat(f32::MAX), |a, p| a.min(*p));
                let hi = l.points.iter().fold(Vec2::splat(f32::MIN), |a, p| a.max(*p));
                (lo, hi, i)
            })
            .collect();
        let mut paved_cells = hashbrown::HashSet::new();
        for class in [layers::PAVE_ROAD, layers::PAVE_AREA] {
            for t in layers.paved_of(class).chunks_exact(3) {
                let lo = t[0].min(t[1]).min(t[2]);
                let hi = t[0].max(t[1]).max(t[2]);
                let a = (t[1] - t[0]).perp_dot(t[2] - t[0]);
                if a.abs() < 1e-3 {
                    continue;
                }
                let (x0, y0) = ((lo.x / PAVED_CELL).floor() as i32, (lo.y / PAVED_CELL).floor() as i32);
                let (x1, y1) = ((hi.x / PAVED_CELL).floor() as i32, (hi.y / PAVED_CELL).floor() as i32);
                if (x1 as i64 - x0 as i64 + 1) * (y1 as i64 - y0 as i64 + 1) > 40_000 {
                    continue;
                }
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        let q = Vec2::new((x as f32 + 0.5) * PAVED_CELL, (y as f32 + 0.5) * PAVED_CELL);
                        let sgn = a.signum();
                        let e = |u: Vec2, v: Vec2| (v - u).perp_dot(q - u) * sgn >= -PAVED_CELL;
                        if e(t[0], t[1]) && e(t[1], t[2]) && e(t[2], t[0]) {
                            paved_cells.insert((x, y));
                        }
                    }
                }
            }
        }
        MapData { layers, cells, lines, paved_cells, model_centre }
    }

    /// The map's paving lies under most of a road along `points` (its middle sampled every
    /// 4 m): the road is drawn as its paving, and a path across a junction or a median is no
    /// road of its own.
    pub fn covers(&self, points: &[DVec3]) -> bool {
        let o = self.layers.origin;
        let (mut n, mut hit, mut len) = (0u32, 0u32, 0.0f64);
        for ab in points.windows(2) {
            let (a, b) = (ab[0].truncate() - o, ab[1].truncate() - o);
            let d = (b - a).length();
            len += d;
            let k = (d / 4.0).ceil().max(1.0) as usize;
            for i in 0..k {
                let q = a.lerp(b, (i as f64 + 0.5) / k as f64).as_vec2();
                n += 1;
                if self.paved_cells.contains(&((q.x / PAVED_CELL).floor() as i32, (q.y / PAVED_CELL).floor() as i32)) {
                    hit += 1;
                }
            }
        }
        // (a short piece - a turn across a junction - counts as covered sooner)
        let need = if len < 60.0 { 0.35 } else { 0.6 };
        n > 0 && hit as f64 >= n as f64 * need
    }

    /// The cells within `r` metres of `c` (layer coordinates).
    fn cells_near(&self, c: Vec2, r: f32) -> impl Iterator<Item = &Cell> {
        let (lo, hi) = (cell_of(c - Vec2::splat(r)), cell_of(c + Vec2::splat(r)));
        (lo.1..=hi.1).flat_map(move |y| (lo.0..=hi.0).filter_map(move |x| self.cells.get(&(x, y))))
    }

    /// From the layers' frame to one anchored at `anchor` (world).
    fn shift(&self, anchor: DVec2) -> Vec2 {
        (self.layers.origin - anchor).as_vec2()
    }

    /// Everything of the ground within `radius` of `centre` (world), relative to `anchor`: the
    /// land, the paved surfaces and the tracks.
    #[allow(dead_code)]
    pub fn build_ground(&self, p: &mut Painter, anchor: DVec2, centre: DVec2, radius: f64, pal: &Palette, look: GroundLook) {
        self.build_land(p, anchor, centre, radius, pal, look);
        self.build_paved(p, anchor, centre, radius, pal, look);
        self.build_lines(p, anchor, centre, radius, pal, look);
    }

    /// The tiles' ground, water, woods and trees.
    pub fn build_land(&self, p: &mut Painter, anchor: DVec2, centre: DVec2, radius: f64, pal: &Palette, look: GroundLook) {
        let k = self.shift(anchor);
        let c = (centre - self.layers.origin).as_vec2();
        let v = |q: Vec2| (q + k).extend(0.0);
        let cells: Vec<&Cell> = self.cells_near(c, radius as f32 + CELL * 1.25).collect();
        let land_order = [
            (layers::LAND_GRASS, pal.grass),
            (layers::LAND_FIELD, pal.field),
            (layers::LAND_SAND, pal.sand),
            (layers::LAND_PAVED, pal.paved_land),
            (layers::LAND_FOREST, pal.forest),
            (layers::LAND_WATER, pal.water),
        ];
        for (class, col) in land_order {
            for cell in &cells {
                for t in cell.land[class].iter().map(|&i| tri(self.layers.land_of(class), i)) {
                    p.world_poly(&[v(t[0]), v(t[1]), v(t[2])], col);
                    if p.at_vertex_limit() { return; }
                }
            }
        }
        if look.trees {
            for cell in self.cells_near(c, radius as f32) {
                for &i in &cell.trees {
                    let t = self.layers.trees[i as usize];
                    let q = v(Vec2::new(t[0], t[1]));
                    crown(p, q, t[2] * 0.95, pal.tree, pal.tree_edge);
                    if p.at_vertex_limit() { return; }
                }
            }
        }
    }

    /// The splines' and the models' ground as they lay it, pressed flat: grass strips, yards and
    /// car parks, then the streets - carriageways and pavements one dark surface, their middle
    /// (`layers::PAVE_INNER`) bright over it, so that a dark rim runs along their edge.
    pub fn build_paved(&self, p: &mut Painter, anchor: DVec2, centre: DVec2, radius: f64, pal: &Palette, look: GroundLook) {
        let k = self.shift(anchor);
        let c = (centre - self.layers.origin).as_vec2();
        let v = |q: Vec2| (q + k).extend(0.0);
        let cells: Vec<&Cell> = self.cells_near(c, radius as f32).collect();
        for (classes, fill) in [(&[layers::PAVE_GREEN][..], pal.grass), (&[layers::PAVE_AREA][..], pal.area), (&[layers::PAVE_FOOT, layers::PAVE_ROAD][..], pal.foot), (&[layers::PAVE_INNER][..], pal.road)] {
            for cell in &cells {
                for &class in classes {
                    for t in cell.paved[class].iter().map(|&i| tri(self.layers.paved_of(class), i)) {
                        p.world_poly(&[v(t[0]), v(t[1]), v(t[2])], fill);
                        if p.at_vertex_limit() { return; }
                    }
                }
            }
        }
        // the paint on the roads (lane lines, crossings, arrows) over the street, near only: from
        // far it is a blur that only dirties the white of the roads
        if look.surfaces {
            for (class, col) in [(layers::PAINT_WHITE, pal.lane_paint), (layers::PAINT_AMBER, pal.lane_amber)] {
                for cell in &cells {
                    for t in cell.paint[class].iter().map(|&i| tri(self.layers.paint_of(class), i)) {
                        p.world_poly(&[v(t[0]), v(t[1]), v(t[2])], col);
                        if p.at_vertex_limit() { return; }
                    }
                }
            }
        }
    }

    /// Railways as a grey band with white dashes (Google's way), tram tracks as a fine line.
    pub fn build_lines(&self, p: &mut Painter, anchor: DVec2, centre: DVec2, radius: f64, pal: &Palette, look: GroundLook) {
        let k = self.shift(anchor);
        let c = (centre - self.layers.origin).as_vec2();
        let r = radius as f32;
        for &(lo, hi, i) in &self.lines {
            if lo.x > c.x + r || hi.x < c.x - r || lo.y > c.y + r || hi.y < c.y - r {
                continue;
            }
            let l = &self.layers.lines[i];
            let pts: Vec<Vec3> = l.points.iter().map(|q| (*q + k).extend(0.0)).collect();
            match l.class {
                layers::LINE_RAIL => {
                    p.ribbon(&pts, 3.6, 3.4, pal.rail, false);
                    if look.dashes {
                        dashes(p, &pts, 9.0, 9.0, 1.3, 1.4, pal.rail_dash);
                    }
                }
                layers::LINE_TRAM => p.ribbon(&pts, 0.9, 1.3, pal.tram, false),
                layers::LINE_RIVER => p.ribbon(&pts, l.width.max(3.0), 2.0, pal.water, false),
                _ => {}
            }
        }
    }

    /// The houses within `radius` of `centre`, relative to `anchor`, three ways (the models of the
    /// houses as they are for the nearest `models` triangles within `MODEL_RADIUS`): their shadows
    /// on the ground (`shadows`, for a shadow layer), their walls lit by which way they face
    /// and their roofs with a fine edge (`solid`, for a depth-tested layer), both at full height
    /// (the layer's `height` raises them), and their roofs flat from above (`flat`).
    #[allow(clippy::too_many_arguments)]
    pub fn build_houses(&self, shadows: &mut Painter, solid: &mut Painter, flat: &mut Painter, anchor: DVec2, centre: DVec2, radius: f64, models: usize, pal: &Palette) {
        let k = self.shift(anchor);
        let c = (centre - self.layers.origin).as_vec2();
        let ids: Vec<u32> = self.cells_near(c, radius as f32).flat_map(|cell| cell.buildings.iter().copied()).collect();
        let light = -Vec2::from(SHADOW_DIR).normalize();
        let sd = Vec2::from(SHADOW_DIR);
        let t0 = std::time::Instant::now();
        // the models that are drawn as they are: the nearest first, while the build's budget of
        // triangles lasts and the view is near enough to tell a roof from a prism; the rest of
        // the houses are footprints with walls
        let mut inst: Vec<u32> = if radius <= MODEL_RADIUS { self.cells_near(c, radius as f32).flat_map(|cell| cell.instances.iter().copied()).collect() } else { Vec::new() };
        inst.sort_by(|a, b| {
            let d = |i: &u32| (self.layers.instances[*i as usize].at - c).length_squared();
            d(a).total_cmp(&d(b))
        });
        let mut drawn: hashbrown::HashSet<u32> = hashbrown::HashSet::new();
        let mut spent = 0usize;
        for &i in &inst {
            let tris = self.layers.models.get(self.layers.instances[i as usize].model as usize).map_or(0, |m| m.len());
            if spent + tris > models {
                continue;
            }
            spent += tris;
            drawn.insert(i);
        }
        let meshed = |b: &layers::Building| b.has_mesh && drawn.contains(&b.inst);
        for &i in &ids {
            let b = &self.layers.buildings[i as usize];
            let o: Vec<Vec2> = b.outline.iter().map(|q| *q + k).collect();
            let h = b.height;
            // (a tall block's shadow is held short: a map is lit from high up)
            let d = sd * h.min(30.0);
            let z = Vec2::ZERO;
            if b.open {
                // a roof on posts: its shadow on the ground, and the roof itself in the air -
                // no walls, no filling of the flat map: the way under it stays visible
                for t in &b.roof {
                    let (a, bb, cc) = (o[t[0] as usize], o[t[1] as usize], o[t[2] as usize]);
                    shadows.shadow_tri([(a, d), (bb, d), (cc, d)], pal.shadow);
                    if !meshed(b) {
                        let q = |m: u16| o[m as usize].extend(h);
                        solid.solid_tri(q(t[0]), q(t[1]), q(t[2]), pal.roof, pal.roof, pal.roof);
                    }
                }
                continue;
            }
            // the shadow: the footprint swept along the light
            for t in &b.roof {
                let (a, bb, cc) = (o[t[0] as usize], o[t[1] as usize], o[t[2] as usize]);
                shadows.shadow_tri([(a, d), (bb, d), (cc, d)], pal.shadow);
                shadows.shadow_tri([(a, z), (bb, z), (cc, z)], pal.shadow);
            }
            let n = o.len();
            for j in 0..n {
                let (a, bb) = (o[j], o[(j + 1) % n]);
                shadows.shadow_tri([(a, z), (bb, z), (bb, d)], pal.shadow);
                shadows.shadow_tri([(a, z), (bb, d), (a, d)], pal.shadow);
            }
            if meshed(b) {
                // (the model itself is drawn below)
                // flat: an edge, then the roof
                for t in &b.roof {
                    let tri = [o[t[0] as usize].extend(0.0), o[t[1] as usize].extend(0.0), o[t[2] as usize].extend(0.0)];
                    flat.grown_tri(tri, 0.15, 0.7, pal.roof_edge);
                }
                continue;
            }
            // walls: outward normal of an anticlockwise outline is to the right of each edge
            for j in 0..n {
                let (a, bb) = (o[j], o[(j + 1) % n]);
                let e = bb - a;
                if e.length_squared() < 1e-4 {
                    continue;
                }
                let nrm = Vec2::new(e.y, -e.x).normalize();
                let lit = (0.5 + 0.5 * nrm.dot(light)).clamp(0.0, 1.0);
                let col = pal.wall_dark.mix(pal.wall_lit, lit);
                let foot = col.darken(0.06);
                let (a0, b0, a1, b1) = (a.extend(0.0), bb.extend(0.0), a.extend(h), bb.extend(h));
                solid.solid_tri(a0, b0, b1, foot, foot, col);
                solid.solid_tri(a0, b1, a1, foot, col, col);
            }
            for t in &b.roof {
                let q = |m: u16| o[m as usize].extend(h);
                solid.solid_tri(q(t[0]), q(t[1]), q(t[2]), pal.roof, pal.roof, pal.roof);
            }
            // flat: an edge, then the roof
            for t in &b.roof {
                let tri = [o[t[0] as usize].extend(0.0), o[t[1] as usize].extend(0.0), o[t[2] as usize].extend(0.0)];
                flat.grown_tri(tri, 0.15, 0.7, pal.roof_edge);
            }
        }
        // the models of the houses as they are, untextured
        for &i in &drawn {
            let it = self.layers.instances[i as usize];
            self.model(solid, it, k, light, pal);
        }
        log::debug!("city map houses: {} models drawn of {} in range ({} triangles), {} buildings, {:.0} ms", drawn.len(), inst.len(), spent, ids.len(), t0.elapsed().as_secs_f64() * 1000.0);
        for &i in &ids {
            let b = &self.layers.buildings[i as usize];
            if b.open {
                continue;
            }
            for t in &b.roof {
                let q = |m: u16| (b.outline[m as usize] + k).extend(0.0);
                flat.world_poly(&[q(t[0]), q(t[1]), q(t[2])], pal.roof);
            }
        }
    }

    /// A placed house model: its faces as they are, lit by which way they face (walls turned to
    /// face away from the model's middle, roofs by their pitch against the light).
    fn model(&self, solid: &mut Painter, it: layers::Instance, k: Vec2, light: Vec2, pal: &Palette) {
        let Some(faces) = self.layers.models.get(it.model as usize) else { return };
        let centre_local = self.model_centre.get(it.model as usize).copied().unwrap_or(Vec2::ZERO);
        let a = (-(it.heading as f64)).to_radians();
        let (sn, cs) = (a.sin() as f32, a.cos() as f32);
        let to_world = |v: Vec3| Vec3::new(v.x * cs - v.y * sn + it.at.x + k.x, v.x * sn + v.y * cs + it.at.y + k.y, v.z);
        let c = {
            let v = Vec3::new(centre_local.x, centre_local.y, 0.0);
            to_world(v).truncate()
        };
        for t in faces {
            let p = t.map(to_world);
            let n = (p[1] - p[0]).cross(p[2] - p[0]);
            let len = n.length();
            if len < 1e-9 {
                continue;
            }
            let mut n = n / len;
            if n.z.abs() < 0.35 {
                // a wall: turned to face away from the middle of the house
                let mid = (p[0] + p[1] + p[2]).truncate() / 3.0;
                if n.truncate().dot(mid - c) < 0.0 {
                    n = -n;
                }
                let lit = (0.5 + 0.5 * n.truncate().normalize_or_zero().dot(light)).clamp(0.0, 1.0);
                let col = pal.wall_dark.mix(pal.wall_lit, lit);
                let foot = col.darken(0.06);
                let at = |v: Vec3| if v.z < 0.5 { foot } else { col };
                solid.solid_tri(p[0], p[1], p[2], at(p[0]), at(p[1]), at(p[2]));
            } else {
                // a roof (or a floor): its pitch turned from the light is a little darker
                if n.z < 0.0 {
                    n = -n;
                }
                let away = (1.0 - n.z) * (1.0 - (0.5 + 0.5 * n.truncate().normalize_or_zero().dot(light)).clamp(0.0, 1.0));
                let col = pal.roof.mix(pal.wall_dark, (away * 0.9).clamp(0.0, 0.45));
                solid.solid_tri(p[0], p[1], p[2], col, col, col);
            }
        }
    }

    /// The layers' extent (world).
    pub fn extent(&self) -> Option<(DVec2, DVec2)> {
        let (lo, hi) = self.layers.bounds()?;
        Some((self.layers.origin + lo.as_dvec2(), self.layers.origin + hi.as_dvec2()))
    }
}

/// A tree's crown: a ten-sided disc with a darker rim (a full circle each was most of the
/// vertices of a leafy suburb).
fn crown(p: &mut Painter, at: Vec3, r: f32, fill: Color, edge: Color) {
    let ring = |r: f32| -> Vec<Vec3> { (0..10).map(|k| { let a = std::f32::consts::TAU * k as f32 / 10.0; at + Vec3::new(a.cos() * r, a.sin() * r, 0.0) }).collect() };
    p.world_poly(&ring(r + 0.35), edge);
    p.world_poly(&ring(r), fill);
}

/// Dashes along a polyline: `on` metres drawn, `off` left, `w_m` wide (at least `w_px`).
fn dashes(p: &mut Painter, pts: &[Vec3], on: f32, off: f32, w_m: f32, w_px: f32, c: Color) {
    let mut run: Vec<Vec3> = Vec::new();
    let mut phase = 0.0f32;
    let mut drawing = true;
    for ab in pts.windows(2) {
        let (a, b) = (ab[0], ab[1]);
        let len = (b - a).truncate().length();
        let mut s = 0.0;
        while s < len {
            let left = (if drawing { on } else { off }) - phase;
            let step = left.min(len - s);
            let q0 = a.lerp(b, s / len);
            let q1 = a.lerp(b, (s + step) / len);
            if drawing {
                if run.is_empty() {
                    run.push(q0);
                }
                run.push(q1);
            }
            s += step;
            phase += step;
            if phase >= (if drawing { on } else { off }) - 1e-4 {
                if drawing && run.len() >= 2 {
                    p.ribbon(&run, w_m, w_px, c, false);
                }
                run.clear();
                drawing = !drawing;
                phase = 0.0;
            }
        }
    }
    if drawing && run.len() >= 2 {
        p.ribbon(&run, w_m, w_px, c, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_draws_the_paved_triangles_of_the_cells_near_it_only() {
        let mut l = Layers::empty(DVec2::ZERO);
        let near = [Vec2::new(10.0, 10.0), Vec2::new(20.0, 10.0), Vec2::new(10.0, 20.0)];
        let far = [Vec2::new(5010.0, 10.0), Vec2::new(5020.0, 10.0), Vec2::new(5010.0, 20.0)];
        let other = [Vec2::new(40.0, 40.0), Vec2::new(50.0, 40.0), Vec2::new(40.0, 50.0)];
        l.paved[layers::PAVE_INNER] = [near, far, other].concat();
        let md = MapData::new(std::sync::Arc::new(l));
        let mut p = Painter::new();
        let look = GroundLook { surfaces: false, trees: false, dashes: false, tol: 0.0 };
        md.build_paved(&mut p, DVec2::ZERO, DVec2::new(15.0, 15.0), 300.0, crate::style::palette(false), look);
        assert!(!p.verts.is_empty());
        assert!(p.verts.iter().all(|v| v.pos[0] < 100.0), "{:?}", p.verts.iter().map(|v| v.pos).collect::<Vec<_>>());
        let mut q = Painter::new();
        q.world_poly(&near.map(|v| v.extend(0.0)), crate::style::palette(false).road);
        assert_eq!(p.verts.len(), 2 * q.verts.len());
        assert!(p.verts.iter().any(|v| v.pos[0] == 50.0 && v.pos[1] == 40.0));
        assert!(p.verts.iter().any(|v| v.pos[0] == 20.0 && v.pos[1] == 10.0));
    }
}
