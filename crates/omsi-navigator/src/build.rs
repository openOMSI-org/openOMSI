//! The ground and the houses of the small map and the city map built on a worker thread: a few
//! hundred thousand triangles around a place (the paving, the woods, the houses with their
//! shadows) took the game tens of milliseconds whenever the bus drove on or the city map was
//! dragged. The picture keeps what it has until the new part is there.

use super::mapmesh::{GroundLook, MapData};
use super::style::Palette;
use super::*;
use omsi_ui::Vertex;
use std::sync::Arc;

// Bound the CPU meshes before creating GPU vertex buffers. Four parts share the
// budget: ground (roads/land) and houses (shadows/3D/flat). This applies to
// the panel and the full city map, including fast mouse drags on huge maps.
// Vertex is 60 bytes; together these caps use at most ~31 MiB per rebuild.
const MAX_GROUND_VERTS: usize = 260_000;
const MAX_HOUSE_VERTS: usize = 90_000;

/// What a build reads: the network's carriageways (each with whether the map's paving covers
/// it), the network for the switches, the map's layers and its package.
#[derive(Clone)]
pub(super) struct GroundParts {
    pub roads: Arc<Vec<(MapRoad, bool)>>,
    pub net: Option<Arc<Network>>,
    pub md: Option<Arc<MapData>>,
    pub surface: Option<Arc<omsi_map::package::Package>>,
    /// The terrain view: how strong the contour lines are (None: off).
    pub contours: Option<f32>,
}

/// What a build makes, relative to `anchor`: the ground, and the houses' shadows, standing
/// walls and roofs, and flat roofs.
pub struct Built {
    pub anchor: DVec2,
    pub counts: [usize; 4],
    pub(crate) runs: [super::cull::Runs; 4],
    verts: [Vec<Vertex>; 4],
    made: Option<[omsi_ui::Vertices; 4]>,
}

impl Built {
    fn new(anchor: DVec2, verts: [Vec<Vertex>; 4]) -> Built {
        Built { anchor, counts: verts.each_ref().map(Vec::len), runs: verts.each_ref().map(|v| super::cull::runs(v)), verts, made: None }
    }

    /// The vertices as built (empty once `made_on` put them on the GPU): the ground, then the
    /// houses' shadows, their standing walls and roofs, and their flat roofs.
    pub fn vertices(&self) -> &[Vec<Vertex>; 4] {
        &self.verts
    }

    pub fn made_on(mut self, device: &wgpu::Device) -> Built {
        self.made = Some(self.verts.each_ref().map(|v| omsi_ui::Gpu::vertices(device, v)));
        self.verts = Default::default();
        self
    }

    pub fn put(self, gpu: &mut omsi_ui::Gpu, device: &wgpu::Device, queue: &wgpu::Queue, ids: [usize; 4]) {
        match self.made {
            Some(made) => ids.into_iter().zip(made).for_each(|(id, v)| gpu.put(id, v)),
            None => ids.into_iter().zip(&self.verts).for_each(|(id, v)| gpu.upload(device, queue, id, v)),
        }
    }
}

/// Everything within `reach` of `anchor`. `paved`: the map's paving is the roads (else every
/// carriageway is a band, as from far); `houses`: build them too.
pub(super) fn build(parts: &GroundParts, anchor: DVec2, reach: f64, pal: &Palette, look: GroundLook, paved: bool, houses: Option<usize>) -> Built {
    let mut p = Painter::with_vertex_budget(MAX_GROUND_VERTS);
    let mut layers = None;
    if let Some(sf) = parts.surface.as_deref() {
        layers = Some(super::surface::build_surface(&mut p, sf, anchor, Some((anchor, reach)), pal, parts.contours.is_some()));
    }
    if let Some(md) = parts.md.as_deref() {
        md.build_land(&mut p, anchor, anchor, reach, pal, look);
    }
    if let (Some(layers), Some(k)) = (layers.as_ref(), parts.contours) {
        super::surface::build_contours(&mut p, layers, anchor, pal, k);
    }
    let paved = paved && parts.md.is_some();
    let rel = |q: DVec3| Vec3::new((q.x - anchor.x) as f32, (q.y - anchor.y) as f32, 0.0);
    // the network's carriageways where the map has no paving of its own for them (else the
    // paving is the road, and a path across a junction or a median is no road)
    let tol = if paved { 0.12 } else { ((reach / 4000.0) as f32).max(look.tol) };
    let span = |pts: &[Vec3]| {
        let (lo, hi) = pts.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), q| (lo.min(*q), hi.max(*q)));
        (hi - lo).truncate().max_element()
    };
    let near: Vec<(&MapRoad, Vec<Vec3>)> = parts
        .roads
        .iter()
        .filter(|(l, covered)| !(paved && *covered) && l.points.iter().any(|q| (q.truncate() - anchor).length() < reach))
        .map(|(l, _)| (l, simplify(&l.points.iter().map(|q| rel(*q)).collect::<Vec<_>>(), tol)))
        // (a road shorter than a pixel is a dot its neighbours' ends already cover)
        .filter(|(_, pts)| look.tol == 0.0 || span(pts) >= look.tol * 2.0)
        .collect();
    for (l, pts) in &near {
        p.ribbon(pts, l.width + 1.0, if l.main { 3.4 } else { 2.8 }, pal.road_edge, true);
    }
    if let (true, Some(md)) = (paved, parts.md.as_deref()) {
        md.build_paved(&mut p, anchor, anchor, reach, pal, look);
    }
    for (l, pts) in &near {
        p.ribbon(pts, l.width, if l.main { 2.2 } else { 1.6 }, pal.road, true);
    }
    if let Some(net) = parts.net.as_deref() {
        build_switches(&mut p, net, anchor, anchor, reach, pal);
    }
    if let Some(md) = parts.md.as_deref() {
        md.build_lines(&mut p, anchor, anchor, reach, pal, look);
    }
    let (mut sh, mut so, mut fl) = (
        Painter::with_vertex_budget(MAX_HOUSE_VERTS),
        Painter::with_vertex_budget(MAX_HOUSE_VERTS),
        Painter::with_vertex_budget(MAX_HOUSE_VERTS),
    );
    if let (Some(models), Some(md)) = (houses, parts.md.as_deref()) {
        md.build_houses(&mut sh, &mut so, &mut fl, anchor, anchor, reach, models, pal);
    }
    if p.verts.len() >= MAX_GROUND_VERTS - 3 || [sh.verts.len(), so.verts.len(), fl.verts.len()].into_iter().any(|n| n >= MAX_HOUSE_VERTS - 3) {
        log::warn!("navigator: map geometry reached its safety budget; distant detail may be omitted ({} ground, {} shadow, {} walls, {} flat vertices)", p.verts.len(), sh.verts.len(), so.verts.len(), fl.verts.len());
    }
    Built::new(anchor, [p.verts, sh.verts, so.verts, fl.verts])
}

/// A map's ground read outside a session - the launcher's map - built by the very code the
/// city map builds its ground with, so the two look the same: the carriageways of the map's
/// network (with whether its paving covers them), its layers and its package.
pub struct MapGround {
    parts: GroundParts,
}

impl MapGround {
    pub fn new(net: Arc<Network>, layers: Option<Arc<omsi_map::layers::Layers>>, package: Option<omsi_map::package::Package>) -> MapGround {
        let md = layers.map(|l| Arc::new(MapData::new(l)));
        let roads: Vec<(MapRoad, bool)> = road_geometry(&net)
            .into_iter()
            .map(|l| {
                let c = md.as_ref().is_some_and(|m| m.covers(&l.points));
                (l, c)
            })
            .collect();
        MapGround { parts: GroundParts { roads: Arc::new(roads), net: Some(net), md, surface: package.map(Arc::new), contours: None } }
    }

    /// Everything within `reach` of `anchor` as the city map shows it at `mpp` metres a pixel
    /// (its details, paving and houses come and go at the same scales: see `city_ground`).
    pub fn build(&self, anchor: DVec2, reach: f64, mpp: f64, light: bool) -> Built {
        // (a road's line to half a pixel: finer points only make triangles no one sees)
        let look = GroundLook { surfaces: mpp < 0.7, trees: mpp < 0.7, dashes: mpp < 3.5, tol: (mpp * 0.5) as f32 };
        build(&self.parts, anchor, reach, style::palette(light), look, mpp < 2.2, (mpp < 6.0).then_some(omsi_mapdraw::MODEL_TRIANGLES))
    }
}

/// A build running on a worker: what it was asked for and where its result comes.
pub(super) struct Job<T> {
    pub meta: T,
    pub rx: std::sync::mpsc::Receiver<Built>,
}

impl<T> Job<T> {
    pub fn start(meta: T, f: impl FnOnce() -> Built + Send + 'static) -> Job<T> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("navigator ground".into())
            .spawn(move || {
                let _ = tx.send(f());
            })
            .ok();
        Job { meta, rx }
    }

    /// The result, once there (`Err(())`: the worker is gone).
    pub fn poll(&self) -> Result<Option<Built>, ()> {
        match self.rx.try_recv() {
            Ok(b) => Ok(Some(b)),
            Err(std::sync::mpsc::TryRecvError::Empty) => Ok(None),
            Err(_) => Err(()),
        }
    }
}

impl Navigator {
    /// The carriageways of `net` with whether the map's paving covers each, worked out on a
    /// worker once per version of the network and of the layers (the whole network's grouping
    /// took the better part of a second); until then the last list, or none.
    pub(super) fn road_list(&mut self, net: &Network) -> Arc<Vec<(MapRoad, bool)>> {
        let key = (self.global_version << 32) ^ (net.lanes.len() as u64) ^ (self.map_version << 56);
        self.poll_road_job();
        let fresh = self.road_cache.as_ref().is_some_and(|(k, _)| *k == key);
        if !fresh && self.road_job.as_ref().is_none_or(|(k, _)| *k != key) {
            // (the map's own network goes to the worker; a traffic network is small: at once)
            match self.global.clone().filter(|g| g.lanes.len() == net.lanes.len()) {
                Some(g) => {
                    let md = self.map_data.clone();
                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::Builder::new()
                        .name("navigator roads".into())
                        .spawn(move || {
                            let list: Vec<(MapRoad, bool)> = road_geometry(&g).into_iter().map(|l| {
                                let c = md.as_ref().is_some_and(|m| m.covers(&l.points));
                                (l, c)
                            }).collect();
                            let _ = tx.send(Arc::new(list));
                        })
                        .ok();
                    self.road_job = Some((key, rx));
                }
                None => {
                    let md = self.map_data.clone();
                    let list: Vec<(MapRoad, bool)> = road_geometry(net).into_iter().map(|l| {
                        let c = md.as_ref().is_some_and(|m| m.covers(&l.points));
                        (l, c)
                    }).collect();
                    self.road_cache = Some((key, Arc::new(list)));
                    self.roads_version += 1;
                }
            }
        }
        self.road_cache.as_ref().map(|c| c.1.clone()).unwrap_or_default()
    }

    /// A part of the maps is still being built on a worker (an offscreen picture waits for it).
    pub fn building(&self) -> bool {
        self.road_job.is_some() || self.panel_job.is_some() || self.city_job.is_some()
    }

    /// A list of carriageways that has arrived from its worker.
    pub(super) fn poll_road_job(&mut self) {
        if let Some((k, rx)) = self.road_job.as_ref() {
            if let Ok(list) = rx.try_recv() {
                self.road_cache = Some((*k, list));
                self.road_job = None;
                self.roads_version += 1;
            }
        }
    }

    /// What a build reads now (`net`: the network the roads are drawn from).
    pub(super) fn ground_parts(&mut self, net: Option<&Network>, contours: Option<f32>) -> GroundParts {
        let roads = net.map(|n| self.road_list(n)).unwrap_or_default();
        GroundParts {
            roads,
            net: self.global.clone(),
            md: self.map_data.clone(),
            surface: self.surface.clone().filter(|_| self.show_surface),
            contours,
        }
    }
}

/// The tracks the map lays with objects rather than splines (points and crossings - switches,
/// a tram's junction): the rail lanes of `net` that come from objects, as the railway or the
/// tram is drawn (a track beside a street is the tram's).
fn build_switches(p: &mut Painter, net: &Network, anchor: DVec2, centre: DVec2, radius: f64, pal: &Palette) {
    let rel = |q: DVec3| Vec3::new((q.x - anchor.x) as f32, (q.y - anchor.y) as f32, 0.0);
    for l in net.lanes.iter().filter(|l| l.kind == LaneKind::Rail && l.source != 1 && l.points.len() >= 2) {
        if (l.start().truncate() - centre).length() > radius {
            continue;
        }
        let pts: Vec<Vec3> = l.points.iter().map(|q| rel(*q)).collect();
        let mid = l.at(l.length() * 0.5).0;
        let street = net.nearest_lane_near(mid, LaneKind::Street).is_some_and(|s| s.2 < 4.0);
        if street {
            p.ribbon(&pts, 0.9, 1.3, pal.tram, false);
        } else {
            p.ribbon(&pts, 3.6, 3.4, pal.rail, false);
        }
    }
}
