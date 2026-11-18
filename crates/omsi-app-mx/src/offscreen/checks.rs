//! The map checks an offscreen run makes when asked (`OMSI_CHECK_*`, `OMSI_PROBE*`,
//! `OMSI_ROAD_PHOTO`): what the map's data says about its roads, and what the picture of
//! them shows.

use super::*;

/// The checks of the map's data asked for, in turn.
pub(super) fn run_checks(world: &World, traffic: Option<&traffic::Traffic>) {
    check_entries(world);
    check_obstacles(world, traffic);
    check_wheels(world, traffic);
    road_rules(traffic);
    check_roads(world, traffic);
    probe_grid(world);
    probe_line(world);
}

fn check_entries(world: &World) {
    // OMSI_CHECK_ENTRIES: is there anything to stand on where a player is put down?
    if omsi_cfg::flags::OMSI_CHECK_ENTRIES.is_set() {
        let mut bare = 0;
        for ep in &world.global.entry_points {
            let Some((pos, rot)) = world.entry_point_place(ep) else {
                log::info!(
                    "entry {:3} \"{}\": object {} not loaded",
                    ep.index,
                    ep.name,
                    ep.object_id
                );
                bare += 1;
                continue;
            };
            let q = ep.quat;
            let qyaw = (2.0 * q[1].atan2(q[3])).to_degrees();
            let rec = crate::spawn::recorded_entry_pos(ep, pos);
            log::info!("entry {:3} \"{}\": object heading {:.1}, record quaternion yaw {:.1} (q {:?}), object at ({:.1}, {:.1}, {:.1}), recorded at {:?}", ep.index, ep.name, rot[0], qyaw, q, pos.x, pos.y, pos.z, rec.map(|r| (r.x, r.y, r.z)));
            let road = world.ground_height(pos.x, pos.y);
            let walk = world.walk_height(pos.x, pos.y);
            let terrain = world.ground_terrain(pos.x, pos.y);
            if road.is_none() {
                bare += 1;
                log::info!("entry {:3} \"{}\" at ({:.0}, {:.0}) heading {:.0}: NO ROAD (surface {:?}, terrain {:?})", ep.index, ep.name, pos.x, pos.y, rot[0], walk, terrain);
            }
        }
        log::info!(
            "entry point check: {bare} of {} entry points have no road surface under them",
            world.global.entry_points.len()
        );
    }
}

fn check_obstacles(world: &World, traffic: Option<&traffic::Traffic>) {
    // OMSI_CHECK_OBSTACLES: sweep a bus-sized box along every driving lane and list the
    // obstacle boxes it runs into - the "invisible walls" a player meets on an open road
    if omsi_cfg::flags::OMSI_CHECK_OBSTACLES.is_set() {
        if let Some(t) = traffic.as_ref() {
            let boxes = world.data.peek().lists.collision.clone();
            // (parked cars stand beside the lanes by design; the traffic steers round them)
            let parked: std::collections::HashSet<i64> = world.tile_state.lock().values().flat_map(|st| st.parked_boxes.iter().map(|b| b.id).collect::<Vec<_>>()).collect();
            let mut hits: std::collections::BTreeMap<i64, (omsi_sim::collision::Obb, usize, DVec3)> = Default::default();
            let mut probes = 0usize;
            for l in t
                .net
                .lanes
                .iter()
                .filter(|l| l.kind == omsi_sim::traffic::LaneKind::Street && !l.invisible)
            {
                let len = l.length();
                let mut s = 0.0f32;
                while s < len {
                    let (p, hdg) = l.at(s);
                    s += 2.0;
                    probes += 1;
                    // a bus body: 2.5 m wide, from 0.35 m over the lane (its floor clears
                    // kerbs and covers) to 3.2 m
                    let probe = omsi_sim::collision::Obb::from_box([2.4, 2.0, 2.85, 0.0, 0.0, 1.775], p, hdg as f64);
                    // pitched with the lane, as a bus on it is
                    let (ahead, behind) = (l.at((s + 1.0).min(len)).0, l.at((s - 3.0).max(0.0)).0);
                    let fwd = (ahead - behind).try_normalize().unwrap_or(DVec3::Y);
                    let right = fwd.cross(DVec3::Z).normalize_or(DVec3::X);
                    let up = right.cross(fwd);
                    let solid = omsi_sim::collision::Box3 { center: p + up * 1.775, axes: [right, fwd, up], half: DVec3::new(1.2, 1.0, 1.425) };
                    for b in boxes.obstacles_near_solid(&probe, Some(&solid)) {
                        if b.overlaps(&probe) && !parked.contains(&b.id) {
                            hits.entry(b.id).or_insert((b, 0, p)).1 += 1;
                        }
                    }
                }
            }
            log::info!("obstacle check: {probes} lane points, {} obstacle boxes stand on a driving lane", hits.len());
            // `OMSI_LANES_NEAR=x,y,r`: the driving lanes passing there (where to put a test bus)
            if let Some(v) = omsi_cfg::flags::OMSI_LANES_NEAR.var() {
                let v: Vec<f64> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if v.len() == 3 {
                    let c = glam::DVec2::new(v[0], v[1]);
                    for (i, l) in t.net.lanes.iter().enumerate().filter(|(_, l)| l.kind == omsi_sim::traffic::LaneKind::Street) {
                        if let Some(k) = l.points.iter().position(|p| (p.truncate() - c).length() < v[2]) {
                            let p = l.points[k];
                            log::info!("  lane {i} {} at ({:.1}, {:.1}, {:.1}) heading {:.1}, {:.0} m long, width {:.1}", l.name, p.x, p.y, p.z, l.headings[k], l.length(), l.width);
                        }
                    }
                }
            }
            for (id, (b, n, p)) in &hits {
                log::info!(
                    "  obstacle key {id} at ({:.1}, {:.1}) z {:.1}..{:.1} half {:.1}x{:.1} hdg {:.0}: {n} lane points, first at ({:.1}, {:.1}, {:.1})",
                    b.center.x, b.center.y, b.z0, b.z1, b.half.x, b.half.y, b.heading.to_degrees(), p.x, p.y, p.z
                );
            }
        }
    }
}

fn check_wheels(world: &World, traffic: Option<&traffic::Traffic>) {
    // OMSI_CHECK_WHEELS: probe the ground along the wheel tracks of every driving lane as a
    // tyre does - a face a little over the road there is an invisible wall to the wheels, a
    // ground far off the lane's height a hump or a hole
    if omsi_cfg::flags::OMSI_CHECK_WHEELS.is_set() {
        if let Some(t) = traffic.as_ref() {
            let (mut points, mut walls, mut steps) = (0usize, Vec::new(), Vec::new());
            for l in t.net.lanes.iter().filter(|l| l.kind == omsi_sim::traffic::LaneKind::Street && !l.invisible) {
                let len = l.length();
                let mut s = 1.0f32;
                while s < len - 1.0 {
                    let (p, hdg) = l.at(s);
                    s += 2.0;
                    let h = (hdg as f64).to_radians();
                    let right = DVec3::new(h.cos(), -h.sin(), 0.0);
                    for side in [-1.0, 1.0] {
                        let w = p + right * side;
                        points += 1;
                        // (from the ground under the wheel, as the tyre probes: 0.8 of a
                        // half-metre radius over it)
                        let g = world.data.peek().drive_probe(w.x, w.y, p.z + 0.4);
                        let g = match g.below {
                            Some(b) => world.data.peek().drive_probe(w.x, w.y, b + 0.4),
                            None => g,
                        };
                        let on_lane = g.below.is_some_and(|b| (b - p.z).abs() <= 0.3);
                        if let (true, Some(a)) = (on_lane, g.above.filter(|a| *a < p.z + 2.0)) {
                            walls.push((w, a - p.z));
                        } else if let Some(b) = g.below.filter(|b| (b - p.z).abs() > 0.3) {
                            steps.push((w, b - p.z));
                        } else if g.below.is_none() {
                            steps.push((w, f64::NAN));
                        }
                    }
                }
            }
            log::info!("wheel check: {points} wheel points, {} under a face (a wall to the tyre), {} off the lane's height by more than 30 cm", walls.len(), steps.len());
            let mut hist = [0usize; 17];
            for (_, d) in &walls {
                hist[((d * 10.0) as usize).min(16)] += 1;
            }
            log::info!("  wall faces by height over the lane (0.1 m steps from 0): {hist:?}");
            for (w, d) in walls.iter().take(40) {
                log::info!("  wall at ({:.1}, {:.1}, {:.1}): face {d:+.2} m over the lane", w.x, w.y, w.z);
            }
            for (w, d) in steps.iter().take(40) {
                log::info!("  ground at ({:.1}, {:.1}, {:.1}): {d:+.2} m off the lane", w.x, w.y, w.z);
            }
        }
    }
}

fn road_rules(traffic: Option<&traffic::Traffic>) {
    // What the map says about traffic on its roads
    if omsi_cfg::flags::OMSI_CHECK_ROADS.is_set() {
        if let Some(t) = traffic.as_ref() {
            let street: Vec<&omsi_sim::traffic::Lane> = t
                .net
                .lanes
                .iter()
                .filter(|l| l.kind == omsi_sim::traffic::LaneKind::Street)
                .collect();
            let no_cars = street.iter().filter(|l| l.no_cars).count();
            let zero = street
                .iter()
                .filter(|l| !l.no_cars && l.density <= 0.001)
                .count();
            let quiet = street
                .iter()
                .filter(|l| l.density > 0.001 && l.density < 0.9)
                .count();
            log::info!("traffic rules: {} street lanes, {no_cars} closed to cars, {zero} with density 0, {quiet} quieter than normal", street.len());
        }
    }
}

fn check_roads(world: &World, traffic: Option<&traffic::Traffic>) {
    // OMSI_CHECK_ROADS: walk every driving lane and report where no road surface is drawn
    // under it, which is what "the road is missing here" looks like from the driver's seat
    if omsi_cfg::flags::OMSI_CHECK_ROADS.is_set() {
        if let Some(t) = traffic.as_ref() {
            let mut checked = 0usize;
            let mut naked = 0usize;
            let mut worst: Vec<(DVec3, f64)> = Vec::new();
            let mut runs: Vec<(DVec3, f32)> = Vec::new();
            for l in t
                .net
                .lanes
                .iter()
                .filter(|l| l.kind == omsi_sim::traffic::LaneKind::Street && !l.invisible)
            {
                let len = l.length();
                let mut s = 0.0f32;
                let mut run_start: Option<(DVec3, f32)> = None;
                while s < len {
                    let (p, _) = l.at(s);
                    checked += 1;
                    let tx = (p.x / omsi_map::tile_size()).floor() as i32;
                    let ty = (p.y / omsi_map::tile_size()).floor() as i32;
                    let lx = (p.x - tx as f64 * omsi_map::tile_size()) as f32;
                    let ly = (p.y - ty as f64 * omsi_map::tile_size()) as f32;
                    let road = world
                        .data
                        .peek()
                        .ground
                        .surfaces
                        .get(&(tx, ty))
                        .and_then(|su| su.sample_road(lx, ly).or_else(|| su.sample(lx, ly)))
                        .map(|h| h as f64);
                    match road {
                        Some(h) if (h - p.z).abs() < 1.0 => {
                            if let Some((from, at)) = run_start.take() {
                                if s - at >= 15.0 {
                                    runs.push((from, s - at));
                                }
                            }
                        }
                        _ => {
                            naked += 1;
                            if run_start.is_none() {
                                run_start = Some((p, s));
                            }
                            if worst.len() < 6 {
                                worst.push((p, road.map(|h| h - p.z).unwrap_or(f64::NAN)));
                            }
                        }
                    }
                    s += 5.0;
                }
                if let Some((from, at)) = run_start.take() {
                    if len - at >= 15.0 {
                        runs.push((from, len - at));
                    }
                }
            }
            // And the other way a road goes missing: the ground is drawn over it. The
            // terrain is only cut where it lies within a hand's width of the surface, so
            // wherever it stands higher than that the road is buried under a mound.
            buried_roads(world, t);
            // And where a bus wheel (0.47 m) rolling along the lane meets a face it cannot
            // climb: a step over omsi_sim::rigid::CLIMB radii, lower than the hub and a metre
            // and a half more (up to the hub and 5 cm the wheels used to climb anyway).
            wheel_faces(world, t);
            runs.sort_by(|a, b| b.1.total_cmp(&a.1));
            log::info!("road check: {naked} of {checked} points along the driving lanes have no road surface under them ({:.1}%)", naked as f32 / checked.max(1) as f32 * 100.0);
            // OMSI_CHECK_SPLINES: chained splines whose ends do not meet in height
            if omsi_cfg::flags::OMSI_CHECK_SPLINES.is_set() {
                spline_check(world);
            }
            log::info!(
                "  {} stretches longer than 15 m (a hole rather than a raster edge)",
                runs.len()
            );
            for (p, len) in runs.iter().take(8) {
                log::info!(
                    "  {len:.0} m without a road from ({:.0}, {:.0}, {:.1})",
                    p.x,
                    p.y,
                    p.z
                );
            }
            for (p, d) in worst {
                log::info!(
                    "  bare point at ({:.0}, {:.0}, {:.1}), nearest surface {d:+.2} m",
                    p.x,
                    p.y,
                    p.z
                );
            }
        }
    }
}

/// Where the ground is drawn over a road (see `check_roads`).
fn buried_roads(world: &World, t: &traffic::Traffic) {
    let mut buried = 0usize;
    let mut checked2 = 0usize;
    let mut worst_b: Vec<(DVec3, f64)> = Vec::new();
    let mut slight: Vec<(DVec3, f64)> = Vec::new();
    for l in t
        .net
        .lanes
        .iter()
        .filter(|l| l.kind == omsi_sim::traffic::LaneKind::Street && !l.invisible)
    {
        let len = l.length();
        let mut s = 0.0f32;
        while s < len {
            let (p, _) = l.at(s);
            s += 5.0;
            let tx = (p.x / omsi_map::tile_size()).floor() as i32;
            let ty = (p.y / omsi_map::tile_size()).floor() as i32;
            let lx = (p.x - tx as f64 * omsi_map::tile_size()) as f32;
            let ly = (p.y - ty as f64 * omsi_map::tile_size()) as f32;
            let Some(ground) = world.ground_terrain(p.x, p.y) else {
                continue;
            };
            // ground that is cut away under the road is not in the way
            if world
                .data
                .peek()
                .ground
                .surfaces
                .get(&(tx, ty))
                .map(|su| su.cut_at(lx, ly, ground as f32, 0.12))
                .unwrap_or(false)
            {
                continue;
            }
            checked2 += 1;
            // the lane's own height is the road the wheels run on
            let over = ground - p.z;
            // (a road a few centimetres under the ground: the teeth of terrain
            // coming through the carriageway)
            if over > 0.005 && over <= 0.15 {
                slight.push((p, over));
            }
            if over > 0.15 {
                buried += 1;
                if worst_b.len() < 100000 {
                    worst_b.push((p, over));
                }
            }
        }
    }
    log::info!("slightly buried: {} road points 0.5..15 cm under the ground; e.g. {:?}", slight.len(), slight.iter().step_by((slight.len() / 6).max(1)).take(6).map(|(p, o)| format!("({:.0}, {:.0}, {:.1}) {:.2} m", p.x, p.y, p.z, o)).collect::<Vec<_>>());
    worst_b.sort_by(|a, b| b.1.total_cmp(&a.1));
    log::info!("buried check: {buried} of {checked2} road points have ground standing over them ({:.1}%)", buried as f32 / checked2.max(1) as f32 * 100.0);
    for (lo, hi) in [
        (0.15, 0.3),
        (0.3, 0.6),
        (0.6, 1.0),
        (1.0, 2.0),
        (2.0, 5.0),
        (5.0, 1e9),
    ] {
        let n = worst_b.iter().filter(|(_, o)| *o >= lo && *o < hi).count();
        log::info!("   {lo:>4} .. {hi:>4} m: {n}");
    }
    for (p, over) in worst_b.iter().take(8) {
        log::info!(
            "  ground {over:.2} m over the road at ({:.0}, {:.0}, {:.1})",
            p.x,
            p.y,
            p.z
        );
    }
}

/// Where a bus wheel rolling along a lane meets a face it cannot climb (see `check_roads`).
fn wheel_faces(world: &World, t: &traffic::Traffic) {
    let r = 0.47f64;
    let (climb, hub, overhead) = (omsi_sim::rigid::CLIMB as f64 * r, r + 0.05, r + 1.5);
    let (mut rolled, mut steep, mut high) = (0usize, 0usize, 0usize);
    let mut faces: Vec<(DVec3, f64, f32)> = Vec::new();
    // (a lane's own height is not always its road's: on some ramps it runs a metre
    // below the surface)
    let surface = |p: DVec3| {
        world.data.peek().drive_probe(p.x, p.y, p.z + 1.5).below
    };
    for l in t
        .net
        .lanes
        .iter()
        .filter(|l| l.kind == omsi_sim::traffic::LaneKind::Street && !l.invisible)
    {
        let len = l.length();
        let mut ground = surface(l.at(0.0).0);
        let mut s = 0.1f32;
        while s < len {
            let (p, dir) = l.at(s);
            s += 0.1;
            let Some(g) = ground else {
                ground = surface(p);
                continue;
            };
            rolled += 1;
            let wheel = world.data.peek().drive_probe(
                p.x,
                p.y,
                g + climb,
            );
            match wheel.above.filter(|z| *z < g + overhead) {
                Some(face) => {
                    if face - g <= hub {
                        steep += 1;
                    } else {
                        high += 1;
                    }
                    faces.push((p, face - g, dir));
                    // stopped here; the lane goes on from its own road
                    ground = surface(p);
                }
                None => ground = wheel.below.or(ground),
            }
        }
    }
    log::info!("wheel check: {} faces a bus wheel cannot climb along {:.0} km of driving lanes: {steep} steps of {climb:.2}..{hub:.2} m (climbed before), {high} higher", steep + high, rolled as f64 * 0.1 / 1000.0);
    // one line per place (faces within 10 m of each other), lowest first
    let mut places: Vec<(DVec3, f64, f64, usize, f32)> = Vec::new();
    for (p, h, dir) in &faces {
        match places
            .iter_mut()
            .find(|q| (q.0 - *p).truncate().length() < 10.0)
        {
            Some(q) => {
                q.1 = q.1.min(*h);
                q.2 = q.2.max(*h);
                q.3 += 1;
            }
            None => places.push((*p, *h, *h, 1, *dir)),
        }
    }
    places.sort_by(|a, b| a.1.total_cmp(&b.1));
    log::info!("  at {} places", places.len());
    for (p, lo, hi, n, dir) in places.iter().take(40) {
        log::info!(
            "  {lo:.2}..{hi:.2} m ({n}x) at ({:.1}, {:.1}, {:.1}) heading {dir:.0}",
            p.x,
            p.y,
            p.z
        );
    }
}

/// Chained splines whose ends do not meet in height (see `check_roads`).
fn spline_check(world: &World) {
    let ends = world.spline_ends.lock();
    let mut bad: Vec<(f64, String)> = Vec::new();
    for (id, (a, b, prev, next, file)) in ends.iter() {
        for (me, other) in [(*b, *next), (*a, *prev)] {
            let Some((oa, ob, ..)) = ends.get(&other) else { continue };
            // the other's end that lies at this one (a chain may run either way)
            let there = if (oa.truncate() - me.truncate()).length() <= (ob.truncate() - me.truncate()).length() { *oa } else { *ob };
            let (d2, dz) = ((there.truncate() - me.truncate()).length(), (there.z - me.z).abs());
            if d2 < 1.0 && dz > 0.1 && id < &other {
                bad.push((dz, format!("spline {id} ({file}) and {other}: {dz:.2} m apart in height at ({:.0}, {:.0}, {:.2})", me.x, me.y, me.z)));
            }
        }
    }
    bad.sort_by(|x, y| y.0.total_cmp(&x.0));
    log::info!("spline check: {} of {} chained ends differ in height by over 10 cm", bad.len(), ends.len());
    for (_, l) in bad.iter().take(15) {
        log::info!("  {l}");
    }
}

fn probe_grid(world: &World) {
    // OMSI_PROBE_GRID=x,y,half,step: the wheels' ground on a square grid around (x, y), as
    // rows of centimetres relative to the middle ('.' where it is the same, '#' where the
    // ground there is more than 5 cm lower: a gap in the road the wheels fall through)
    if let Some(spec) = omsi_cfg::flags::OMSI_PROBE_GRID.var() {
        let v: Vec<f64> = spec.split(',').filter_map(|t| t.trim().parse().ok()).collect();
        if v.len() >= 4 {
            let (cx, cy, half, step) = (v[0], v[1], v[2], v[3].max(0.001));
            let mid = world.data.peek().drive_probe(cx, cy, 1e6).below.unwrap_or(0.0);
            let n = (half / step).round() as i64;
            for j in (-n..=n).rev() {
                let row: String = (-n..=n)
                    .map(|i| {
                        let (x, y) = (cx + i as f64 * step, cy + j as f64 * step);
                        match world.data.peek().drive_probe(x, y, mid + 1.0).below {
                            Some(z) if z < mid - 0.05 => '#',
                            Some(z) if (z - mid).abs() <= 0.02 => '.',
                            Some(_) => '+',
                            None => ' ',
                        }
                    })
                    .collect();
                log::info!("grid {:.3}: {row}", cy + j as f64 * step);
            }
        }
    }
}

fn probe_line(world: &World) {
    // OMSI_PROBE=x0,y0,x1,y1[,n]: print the terrain height and the road surface height
    // along a line, to see where the ground comes through a road
    if let Some(spec) = omsi_cfg::flags::OMSI_PROBE.var() {
        let v: Vec<f64> = spec
            .split(',')
            .filter_map(|t| t.trim().parse().ok())
            .collect();
        if v.len() >= 4 {
            let n = v.get(4).copied().unwrap_or(20.0).max(2.0) as usize;
            for k in 0..n {
                let t = k as f64 / (n - 1) as f64;
                let (x, y) = (v[0] + (v[2] - v[0]) * t, v[1] + (v[3] - v[1]) * t);
                let tx = (x / omsi_map::tile_size()).floor() as i32;
                let ty = (y / omsi_map::tile_size()).floor() as i32;
                let lx = (x - tx as f64 * omsi_map::tile_size()) as f32;
                let ly = (y - ty as f64 * omsi_map::tile_size()) as f32;
                let terrain = world
                    .data
                    .peek()
                    .ground
                    .terrains
                    .get(&(tx, ty))
                    .map(|t| t.sample(lx, ly));
                let surface = world
                    .data
                    .peek()
                    .ground
                    .surfaces
                    .get(&(tx, ty))
                    .and_then(|s| s.sample(lx, ly));
                let top = terrain.map(|t| t as f64 + 30.0).unwrap_or(1e6);
                let wheel = world.data.peek().drive_probe(x, y, top);
                log::info!("probe ({x:.1}, {y:.1}) tile ({tx}, {ty}) local ({lx:.1}, {ly:.1}): terrain {terrain:?} surface {surface:?} wheels {:?}", wheel.below);
            }
        }
    }
}

/// `OMSI_ROAD_PHOTO` (with the final picture's lighting).
pub(super) fn road_photo(
    renderer: &mut Renderer,
    scene: &mut Scene,
    traffic: Option<&traffic::Traffic>,
    lighting: &omsi_render::Lighting,
) {
    // OMSI_ROAD_PHOTO: photograph the road network from above, point by point, and say
    // where the picture shows grass although the map says there is a carriageway. This is
    // the only check that asks what is actually drawn rather than what the data says.
    if omsi_cfg::flags::OMSI_ROAD_PHOTO.is_set() {
        if let Some(t) = traffic.as_ref() {
            let mut points: Vec<DVec3> = Vec::new();
            let mut headings: Vec<f64> = Vec::new();
            // OMSI_ROAD_PHOTO_SLANT=<m>: from a driver's eye that far back along the lane
            // (2.6 m up) instead of from above - terrain a few millimetres over the road
            // shows only at a slant
            let slant: Option<f64> = omsi_cfg::flags::OMSI_ROAD_PHOTO_SLANT.parse();
            for l in t
                .net
                .lanes
                .iter()
                .filter(|l| l.kind == omsi_sim::traffic::LaneKind::Street && !l.invisible)
            {
                let len = l.length();
                let mut s = 3.0f32;
                // the carriageway itself, and the verge a few metres to either side, which
                // in a city street is paved too
                let side: f64 = omsi_cfg::flags::OMSI_ROAD_PHOTO_SIDE.parse()
                    .unwrap_or(0.0);
                while s < len {
                    let (p, h) = l.at(s);
                    let a = (h as f64).to_radians();
                    points.push(DVec3::new(p.x + side * a.cos(), p.y - side * a.sin(), p.z));
                    headings.push(h as f64);
                    s += 25.0;
                }
            }
            // a spread sample so one long street cannot dominate
            let cap: usize = omsi_cfg::flags::OMSI_ROAD_PHOTO_N.parse().unwrap_or(400);
            let step = (points.len() / cap.max(1)).max(1);
            let sample: Vec<(DVec3, f64)> = points.iter().copied().zip(headings.iter().copied()).step_by(step).collect();
            let (mut green, mut checked) = (0usize, 0usize);
            let mut spots: Vec<(DVec3, [u8; 3])> = Vec::new();
            let mut holes: Vec<(usize, DVec3, Camera)> = Vec::new();
            for (p, h) in &sample {
                let cam = match slant {
                    Some(back) => {
                        let a = h.to_radians();
                        let eye = DVec3::new(p.x - back * a.sin(), p.y - back * a.cos(), p.z + 2.6);
                        Camera {
                            position: eye,
                            yaw: *h as f32,
                            pitch: -(2.6f64 / back).atan().to_degrees() as f32,
                            roll: 0.0,
                            fov_deg: 20.0,
                            near: 0.3,
                            far: 400.0,
                        }
                    }
                    // (only the half metre over the lane: a crown or a sign hanging over the
                    // carriageway is no hole in it)
                    None => Camera {
                        position: DVec3::new(p.x, p.y, p.z + 40.0),
                        yaw: 0.0,
                        pitch: -90.0,
                        roll: 0.0,
                        fov_deg: 40.0,
                        near: 39.5,
                        far: 41.5,
                    },
                };
                let (pw, ph) = (48u32, 48u32);
                // OMSI_HOLE_PHOTO: the same place from above down to 25 m under the lane -
                // what shows the sky there is a hole through the world
                if omsi_cfg::flags::OMSI_HOLE_PHOTO.is_set() {
                    // (at a slant: the lower half of the picture only, which is all under the
                    // horizon)
                    let deep = if slant.is_some() { Camera { near: 0.3, far: 400.0, ..cam } } else { Camera { near: 20.0, far: 65.0, ..cam } };
                    if let Ok(px) = renderer.render_to_image(scene, 96, 96, &deep, lighting) {
                        let from = if slant.is_some() { 48 * 96 * 4 } else { 0 };
                        let sky = px[from..].chunks_exact(4).filter(|c| c[2] as i32 > c[0] as i32 + 30 && c[2] as i32 > c[1] as i32 + 8 && c[2] > 150).count();
                        if sky > 3 {
                            holes.push((sky, *p, deep));
                        }
                    }
                }
                let Ok(px) = renderer.render_to_image(scene, pw, ph, &cam, lighting) else {
                    continue;
                };
                // the centre pixel looks straight down at the lane
                let i = ((ph / 2) * pw + pw / 2) as usize * 4;
                let (r, g, b) = (px[i], px[i + 1], px[i + 2]);
                checked += 1;
                // grass and fields are green; asphalt, concrete and cobbles are grey
                let is_green = g as i32 > r as i32 + 12 && g as i32 > b as i32 + 12;
                if is_green {
                    green += 1;
                    if spots.len() < 12 {
                        spots.push((*p, [r, g, b]));
                    }
                }
            }
            if omsi_cfg::flags::OMSI_HOLE_PHOTO.is_set() {
                holes.sort_by(|a, b| b.0.cmp(&a.0));
                log::info!("hole photo: {} of {checked} places show the sky through the ground", holes.len());
                for (n, p, c) in holes.iter().take(40) {
                    log::info!("   {n} sky pixels at ({:.1}, {:.1}, {:.2})  (--cam {:.1},{:.1},{:.1},{:.0},{:.1},{:.0})", p.x, p.y, p.z, c.position.x, c.position.y, c.position.z, c.yaw, c.pitch, c.fov_deg);
                }
            }
            log::info!("road photo: {green} of {checked} places along the carriageways show ground instead of road ({:.1} %)", green as f32 / checked.max(1) as f32 * 100.0);
            for (p, c) in &spots {
                log::info!(
                    "   green at ({:.1}, {:.1}, {:.2}) rgb {:?}  (--cam {:.0},{:.0},{:.0},0,-89)",
                    p.x,
                    p.y,
                    p.z,
                    c,
                    p.x,
                    p.y,
                    p.z + 40.0
                );
            }
        }
    }
}
