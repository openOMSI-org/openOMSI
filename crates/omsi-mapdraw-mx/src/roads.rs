//! The navigator's roads from the traffic network: which lanes are drawn, their meshes, the
//! route on them with its arrows and colours, the street names, and the way back to the route.

use crate::style;
use omsi_sim::crowd::angle_diff;
use glam::{DMat3, DVec2, DVec3, Vec2, Vec3};
use hashbrown::HashMap;
use omsi_sim::traffic::{LaneKey, LaneKind, Network};
use omsi_ui::{Color, Painter, Rect};

/// Street lanes that stand for drawn roads, including editor-only paths corroborated by
/// road surfaces. Uncorroborated helpers must not make a phantom road on the GPS.
pub fn visible_road_lanes(net: &Network) -> Vec<(usize, &omsi_sim::traffic::Lane)> {
    let mut seen = hashbrown::HashSet::<(LaneKey, u32)>::new();
    net.lanes
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind == LaneKind::Street && !l.invisible && l.points.len() >= 2)
        .filter(|(_, l)| {
            if let Some(key) = l.key {
                // A two-way [path] has the same geometry in both directions. Keep one
                // casing and surface; retain separate [path]s, which may be real lanes.
                return seen.insert((key, l.source));
            }
            true
        })
        .collect()
}

/// The roads a city map draws, from a whole map's lanes as they were read off the tiles:
/// every path linked to its neighbours, the editor-only ones confirmed by the map's own
/// asphalt, then grouped into carriageways with their real widths. The navigator's city map
/// and the launcher's map picture draw the same roads this way, so the two cannot drift
/// apart; the second value is the linked network the caller may walk (a trip's own lanes).
pub fn city_roads(lanes: Vec<omsi_sim::traffic::Lane>, surfaces: &[(Vec<DVec3>, f32)]) -> (Vec<MapRoad>, Network) {
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    confirm_road_surfaces(&mut net, surfaces);
    let roads = road_geometry(&net);
    (roads, net)
}

/// A road of the city map: the centre line of a carriageway, as wide as the map makes it.
pub struct MapRoad {
    pub points: Vec<DVec3>,
    pub width: f32,
    /// A road cars drive far along (the map's own speed limits say so): drawn brighter.
    pub main: bool,
}

/// Some maps separate the asphalt mesh from their editor-only traffic splines. Use the
/// actual road footprint to distinguish those paths from invisible scenery helpers. This
/// changes only the navigator's copy of the network; driving still uses the original data.
pub fn confirm_road_surfaces(net: &mut Network, surfaces: &[(Vec<DVec3>, f32)]) {
    let streets = || net.lanes.iter().filter(|l| l.kind == LaneKind::Street);
    if surfaces.is_empty() && streets().next().is_some() && streets().all(|l| l.invisible) {
        for lane in net.lanes.iter_mut().filter(|l| l.kind == LaneKind::Street) {
            lane.invisible = false;
        }
        return;
    }
    let mut segments = Vec::new();
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (pts, width) in surfaces {
        for ab in pts.windows(2) {
            let (a, b) = (ab[0], ab[1]);
            let margin = *width as f64 * 0.5 + 0.5;
            let lo = a.truncate().min(b.truncate()) - DVec2::splat(margin);
            let hi = a.truncate().max(b.truncate()) + DVec2::splat(margin);
            let (x0, y0) = Network::grid_cell(lo.extend(0.0));
            let (x1, y1) = Network::grid_cell(hi.extend(0.0));
            let i = segments.len();
            segments.push((a, b, margin));
            for x in x0..=x1 { for y in y0..=y1 { grid.entry((x, y)).or_default().push(i); } }
        }
    }
    for lane in net.lanes.iter_mut().filter(|l| l.invisible && l.kind == LaneKind::Street) {
        let n = (lane.length() / 8.0).ceil().max(1.0) as usize;
        let mut covered = 0;
        for k in 0..n {
            let (p, _) = lane.at(lane.length() * (k as f32 + 0.5) / n as f32);
            let on_surface = grid.get(&Network::grid_cell(p)).map(|ids| ids.iter().any(|&i| {
                let (a, b, margin) = segments[i];
                let ab = (b - a).truncate();
                let t = ((p - a).truncate().dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                let q = a.lerp(b, t);
                (p - q).truncate().length() <= margin && (p.z - q.z).abs() <= 2.0
            })).unwrap_or(false);
            if on_surface { covered += 1; }
        }
        if covered * 4 >= n * 3 { lane.invisible = false; }
    }
    // Crossings made of objects can have no asphalt spline. Keep helpers joining two
    // corroborated roads; an isolated invisible helper still does not become a street.
    let mut prev = vec![Vec::new(); net.lanes.len()];
    for (i, lane) in net.lanes.iter().enumerate().filter(|(_, l)| l.kind == LaneKind::Street) {
        for &j in &lane.next {
            if net.lanes.get(j).map(|l| l.kind == LaneKind::Street).unwrap_or(false) { prev[j].push(i); }
        }
    }
    let mut seen = vec![false; net.lanes.len()];
    let mut keep = Vec::new();
    for seed in 0..net.lanes.len() {
        if seen[seed] || !net.lanes[seed].invisible || net.lanes[seed].kind != LaneKind::Street { continue; }
        let mut queue = vec![seed];
        let mut component = Vec::new();
        let mut anchors = hashbrown::HashSet::new();
        while let Some(i) = queue.pop() {
            if seen[i] { continue; }
            seen[i] = true;
            component.push(i);
            for &j in net.lanes[i].next.iter().chain(prev[i].iter()) {
                let Some(l) = net.lanes.get(j).filter(|l| l.kind == LaneKind::Street) else { continue };
                if l.invisible {
                    if !seen[j] { queue.push(j); }
                } else {
                    anchors.insert((l.key, l.source, if l.key.is_none() { j } else { 0 }));
                }
            }
        }
        if anchors.len() >= 2 { keep.extend(component); }
    }
    for i in keep { net.lanes[i].invisible = false; }
}

/// Traffic paths are lane centres, not separate streets. Adjacent paths on one spline
/// describe one carriageway; combine their footprints without filling a median. Align
/// opposing directions before averaging so bends, mirrors and ramps retain their shape.
pub fn road_geometry(net: &Network) -> Vec<MapRoad> {
    let mut roads = Vec::new();
    let mut splines = std::collections::BTreeMap::<((i32, i32), i64), Vec<&omsi_sim::traffic::Lane>>::new();
    for (_, lane) in visible_road_lanes(net) {
        if let Some(key) = lane.key.filter(|_| lane.source == 1) {
            splines.entry((key.tile, key.id)).or_default().push(lane);
        } else {
            roads.push(MapRoad { points: lane.points.clone(), width: lane.width.max(2.6), main: lane.speed_limit_kmh >= 55.0 });
        }
    }
    for mut lanes in splines.into_values() {
        lanes.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        let mut start = 0;
        while start < lanes.len() {
            let a = lanes[start];
            let lo = a.offset - a.width.max(2.6) * 0.5;
            let mut hi = a.offset + a.width.max(2.6) * 0.5;
            let mut end = start + 1;
            while end < lanes.len() {
                let b = lanes[end];
                if b.points.len() != a.points.len() || b.offset - b.width.max(2.6) * 0.5 > hi + 0.65 { break; }
                let mid = a.points.len() / 2;
                let za = a.points[if a.reversed { a.points.len() - 1 - mid } else { mid }].z;
                let zb = b.points[if b.reversed { b.points.len() - 1 - mid } else { mid }].z;
                if (za - zb).abs() > 1.5 { break; }
                hi = hi.max(b.offset + b.width.max(2.6) * 0.5);
                end += 1;
            }
            let b = lanes[end - 1];
            let span = b.offset - a.offset;
            let t = if span.abs() > 0.01 { (((lo + hi) * 0.5 - a.offset) / span) as f64 } else { 0.0 };
            let point = |l: &omsi_sim::traffic::Lane, i: usize| l.points[if l.reversed { l.points.len() - 1 - i } else { i }];
            let points = (0..a.points.len()).map(|i| point(a, i).lerp(point(b, i), t)).collect();
            roads.push(MapRoad { points, width: hi - lo, main: lanes[start..end].iter().any(|l| l.speed_limit_kmh >= 55.0) });
            start = end;
        }
    }
    // Bridge only explicit graph connections within the map's placement tolerance.
    for lane in net.lanes.iter().filter(|l| l.kind == LaneKind::Street && !l.invisible && l.points.len() >= 2) {
        for &j in &lane.next {
            let Some(next) = net.lanes.get(j).filter(|l| l.kind == LaneKind::Street && !l.invisible && l.points.len() >= 2) else { continue };
            let distance = lane.end().distance(next.start());
            if distance > 0.05 && distance <= 2.0 {
                roads.push(MapRoad { points: vec![lane.end(), next.start()], width: lane.width.min(next.width).max(2.6), main: lane.speed_limit_kmh >= 55.0 && next.speed_limit_kmh >= 55.0 });
            }
        }
    }
    roads
}

pub fn rects_overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

/// Stop positions stay on the route. Move only their labels, or omit a label if none of
/// the nearby positions fits. Call in route order so the next stop has first choice.
pub fn stop_label_rect(p: Vec2, width: f32, s: f32, win: Rect, taken: &[Rect]) -> Option<Rect> {
    let h = 20.0 * s;
    let gap = 10.0 * s;
    let positions = [
        Vec2::new(p.x + gap, p.y - h * 0.5),
        Vec2::new(p.x - gap - width, p.y - h * 0.5),
        Vec2::new(p.x + gap, p.y - h - gap),
        Vec2::new(p.x + gap, p.y + gap),
        Vec2::new(p.x - gap - width, p.y - h - gap),
        Vec2::new(p.x - gap - width, p.y + gap),
    ];
    positions.into_iter().map(|q| Rect::new(q.x, q.y, width, h)).find(|r| {
        r.x >= 8.0 * s && r.right() <= win.right() - 8.0 * s && r.y >= 50.0 * s && r.bottom() <= win.bottom() - 8.0 * s
            && !taken.iter().any(|t| rects_overlap(t, r))
    })
}

pub fn spaced_markers(points: impl IntoIterator<Item = (usize, Vec2)>, gap: f32) -> Vec<(usize, Vec2)> {
    let mut kept: Vec<(usize, Vec2)> = Vec::new();
    for (k, p) in points {
        if kept.iter().all(|(_, q)| p.distance(*q) >= gap) { kept.push((k, p)); }
    }
    kept
}

/// `pts` with the points dropped that lie within `tol` metres of the line through their
/// neighbours kept (Douglas-Peucker): a lane is sampled every metre or two, a ribbon needs
/// only its bends. The launcher's map picture draws its roads with this too.
pub fn simplify(pts: &[Vec3], tol: f32) -> Vec<Vec3> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0usize, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (pa, pb) = (pts[a].truncate(), pts[b].truncate());
        let ab = pb - pa;
        let len = ab.length().max(1e-6);
        let mut worst = (0.0f32, 0usize);
        for k in a + 1..b {
            let d = (pts[k].truncate() - pa).perp_dot(ab).abs() / len;
            if d > worst.0 {
                worst = (d, k);
            }
        }
        if worst.0 > tol {
            keep[worst.1] = true;
            stack.push((a, worst.1));
            stack.push((worst.1, b));
        }
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

/// `OMSI_NAV_PROBE=x,y[,r]`: the lanes of the map's network that start or end within r
/// metres (25) of a point - how they link, to see why a route cannot reach a place.
pub fn probe_lanes(net: &Network) {
    let Some(v) = omsi_cfg::flags::OMSI_NAV_PROBE.var() else { return };
    let f: Vec<f64> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    if f.len() < 2 {
        return;
    }
    let c = DVec2::new(f[0], f[1]);
    let r = f.get(2).copied().unwrap_or(25.0);
    for (i, l) in net.lanes.iter().enumerate() {
        let (s, e) = (l.start(), l.end());
        if (s.truncate() - c).length() > r && (e.truncate() - c).length() > r && l.nearest_point(DVec3::new(c.x, c.y, s.z)).map(|p| p.1 > r).unwrap_or(true) {
            continue;
        }
        let prev = net.prev.get(i).cloned().unwrap_or_default();
        log::info!(
            "nav probe: lane {i} {:?} {:?} key {:?} rev {} start ({:.1}, {:.1}, {:.1}) h {:.0} end ({:.1}, {:.1}, {:.1}) h {:.0} len {:.1} next {:?} prev {:?}",
            l.kind, l.name, l.key, l.reversed, s.x, s.y, s.z, l.start_heading(), e.x, e.y, e.z, l.end_heading(), l.length(), l.next, prev
        );
    }
}

/// How a route is drawn: its width (lane width plus `extra_m`, at least `min_px`), arrows
/// every so many metres for so far, how much of it, and only near a point.
pub struct RouteStyle {
    pub extra_m: f32,
    pub min_px: f32,
    pub arrows: Option<(f32, f32)>,
    pub max_len: f32,
    pub near: Option<(DVec2, f64)>,
}

/// Which lane of `lane`'s road (the lanes beside it going the same way) the bus at `bus`
/// is in, counted from the kerb (0 = the kerb lane: the rightmost, the leftmost on a
/// left-hand-traffic map).
pub fn lane_from_right(net: &Network, lane: usize, bus: DVec3) -> usize {
    let Some(mut cur) = net.lanes.get(lane).map(|_| lane) else { return 0 };
    let kerb = |l: &omsi_sim::traffic::Lane| if net.left_hand { l.left } else { l.right };
    let away = |l: &omsi_sim::traffic::Lane| if net.left_hand { l.right } else { l.left };
    for _ in 0..6 {
        match kerb(&net.lanes[cur]) {
            Some(n) if n < net.lanes.len() && net.lanes[n].kind == LaneKind::Street => cur = n,
            _ => break,
        }
    }
    let (mut best, mut best_d, mut k) = (0usize, f64::MAX, 0usize);
    loop {
        if let Some((_, d)) = net.lanes[cur].nearest_point(bus) {
            if d < best_d {
                best_d = d;
                best = k;
            }
        }
        match away(&net.lanes[cur]) {
            Some(n) if n < net.lanes.len() && net.lanes[n].kind == LaneKind::Street && k < 6 => {
                cur = n;
                k += 1;
            }
            _ => break,
        }
    }
    best
}

/// The route from the bus on: one band, coloured stretch by stretch by how busy the road
/// is (`jam`: congestion per lane), with arrows along it in a colour that stands out.
#[allow(clippy::too_many_arguments)]
pub fn build_route(p: &mut Painter, net: &Network, lanes: &[usize], anchor: DVec2, s0: f32, jam: &HashMap<usize, f32>, style: &RouteStyle, bus_lane: usize, pal: &style::Palette) {
    build_route_to(p, net, lanes, anchor, s0, jam, style, bus_lane, pal, None);
}

/// Draw a route optionally ending partway through its final lane. Existing callers keep
/// drawing the whole route with `build_route`; a free destination supplies its endpoint.
#[allow(clippy::too_many_arguments)]
pub fn build_route_to(p: &mut Painter, net: &Network, lanes: &[usize], anchor: DVec2, s0: f32, jam: &HashMap<usize, f32>, style: &RouteStyle, bus_lane: usize, pal: &style::Palette, end_s: Option<f32>) {
    let rel = |q: DVec3| Vec3::new((q.x - anchor.x) as f32, (q.y - anchor.y) as f32, 0.0);
    // runs of one colour: the lanes' points joined up, the first lane cut where the bus is
    let mut runs: Vec<(Vec<Vec3>, f32, usize)> = Vec::new();
    let mut arrows: Vec<(DVec3, f32, usize, f32)> = Vec::new();
    let mut total = 0.0f32;
    // the lane to show: of a road with several lanes each way, the one a driver takes - the
    // leftmost before a left turn, the rightmost before a right turn, and otherwise the one
    // the bus is driving in (`bus_lane`, counted from the right). Always the rightmost, the
    // route ran along the kerb on Berlin's three-lane roads, where the cars are parked; the
    // timetable's own track often runs in the middle lane.
    let turn_after = |k: usize| -> i32 {
        let mut acc = 0.0f32;
        for &j in lanes.iter().skip(k + 1).take(40) {
            let Some(l) = net.lanes.get(j) else { break };
            let d = omsi_sim::traffic::wrap_deg(l.end_heading() - l.start_heading());
            if d.abs() > 35.0 && l.length() < 60.0 {
                return if d > 0.0 { 1 } else { -1 };
            }
            // a lane that has no neighbours ends the stretch of several lanes
            if l.left.is_none() && l.right.is_none() {
                break;
            }
            acc += l.length();
            if acc > 250.0 {
                break;
            }
        }
        0
    };
    let shown = |k: usize, l: usize| -> usize {
        let side = turn_after(k);
        let step = |cur: usize, left: bool| -> Option<usize> {
            let n = if left { net.lanes[cur].left } else { net.lanes[cur].right }?;
            (n < net.lanes.len() && net.lanes[n].kind == LaneKind::Street).then_some(n)
        };
        let mut cur = l;
        // before a turn the lane on its side; straight on the bus's own, counted from the kerb
        // (the right, or the left on a left-hand-traffic map)
        let to_left = if side == 0 { net.left_hand } else { side < 0 };
        for _ in 0..6 {
            match step(cur, to_left) {
                Some(n) => cur = n,
                None => break,
            }
        }
        if side == 0 {
            for _ in 0..bus_lane {
                match step(cur, !net.left_hand) {
                    Some(n) => cur = n,
                    None => break,
                }
            }
        }
        cur
    };
    for (k, &l) in lanes.iter().enumerate() {
        let Some(lane) = net.lanes.get(l) else { break };
        let route_l = l;
        let l = if end_s.is_some() && k + 1 == lanes.len() { l } else { shown(k, l) };
        let lane = net.lanes.get(l).unwrap_or(lane);
        if let Some((c, r)) = style.near {
            if (c - lane.start().truncate()).length() > r {
                break;
            }
        }
        let from = if k == 0 { s0 } else { 0.0 };
        let to = if k + 1 == lanes.len() { end_s.unwrap_or(lane.length()).clamp(0.0, lane.length()) } else { lane.length() };
        if to <= from { continue; }
        let mut pts: Vec<Vec3> = Vec::new();
        if k == 0 {
            pts.push(rel(lane.at(s0).0));
        }
        for (q, d) in lane.points.iter().zip(&lane.dist) {
            if (*d > from + 0.05 || k > 0) && *d <= to {
                pts.push(rel(*q));
            }
        }
        if end_s.is_some() && k + 1 == lanes.len() { pts.push(rel(lane.at(to).0)); }
        // a change of lane (the lane shown beside the last one): a smooth curve over to the
        // new lane rather than a step sideways at the junction of the two
        let mut skip_arrows = 0.0f32;
        if let Some(prev) = runs.last().map(|r| &r.0).filter(|r| r.len() >= 2) {
            let p0 = prev[prev.len() - 1];
            if pts.first().is_some_and(|q| (*q - p0).truncate().length() > 0.8) {
                let t0 = (p0 - prev[prev.len() - 2]).truncate().normalize_or_zero();
                let blend = (lane.length() * 0.6).min(28.0);
                let mut acc = 0.0;
                let mut cut = pts.len() - 1;
                for i in 1..pts.len() {
                    acc += (pts[i] - pts[i - 1]).truncate().length();
                    if acc >= blend {
                        cut = i;
                        break;
                    }
                }
                let p1 = pts[cut];
                let t1 = if cut + 1 < pts.len() { (pts[cut + 1] - p1).truncate().normalize_or_zero() } else { (p1 - pts[cut.saturating_sub(1)]).truncate().normalize_or_zero() };
                let l = (p1 - p0).truncate().length();
                let (c0, c1) = (p0 + (t0 * l / 3.0).extend(0.0), p1 - (t1 * l / 3.0).extend(0.0));
                let mut curve: Vec<Vec3> = (1..=10)
                    .map(|i| {
                        let t = i as f32 / 10.0;
                        let u = 1.0 - t;
                        p0 * (u * u * u) + c0 * (3.0 * u * u * t) + c1 * (3.0 * u * t * t) + p1 * (t * t * t)
                    })
                    .collect();
                curve.extend_from_slice(&pts[cut + 1..]);
                pts = curve;
                skip_arrows = blend;
            }
        }
        let lv = level(jam.get(&route_l).copied().unwrap_or(0.0));
        let w = lane.width.max(2.6);
        match runs.last_mut() {
            Some(run) if run.2 == lv => {
                run.0.extend(pts);
                run.1 = run.1.max(w);
            }
            _ => {
                // (a new colour starts where the last one ended)
                let mut start = runs.last().and_then(|r| r.0.last().copied()).map(|q| vec![q]).unwrap_or_default();
                start.extend(pts);
                runs.push((start, w, lv));
            }
        }
        // arrows at even steps along each lane (they stay put while the bus drives)
        if let Some((every, reach)) = style.arrows {
            let len = lane.length();
            // (never more than a few dozen a lane: zoomed far in on the city map the step
            // came out a few millimetres and the loop ran for minutes - the game froze)
            if total < reach && len > every * 0.4 && every > 0.5 {
                let n = (len / every).round().clamp(1.0, 64.0);
                let step = len / n;
                for i in 0..n as usize {
                    let at = step * (i as f32 + 0.5);
                    if at > from + 4.0 && at <= to && at > skip_arrows && total + at - from < reach {
                        let (q, h) = lane.at(at);
                        arrows.push((q, h, lv, w));
                    }
                }
            }
        }
        total += to - from;
        if total > style.max_len {
            break;
        }
    }
    // the edge under the whole route, then each stretch in its colour: the route's blue where
    // the traffic flows, amber where it is slow, red where it is heavy, dark red where it stands
    let colour = |lv: usize| match lv {
        0 | 1 => pal.route,
        2 => style::TRAFFIC[1],
        3 => style::TRAFFIC[2],
        _ => style::TRAFFIC[3],
    };
    for (pts, w, _) in &runs {
        p.ribbon(pts, w + style.extra_m + 1.0, style.min_px + 2.4, pal.route_edge, true);
    }
    for (pts, w, lv) in &runs {
        p.ribbon(pts, w + style.extra_m, style.min_px, colour(*lv), true);
    }
    for (q, h, _, w) in arrows {
        let hr = h.to_radians();
        let d = Vec2::new(hr.sin(), hr.cos());
        let n = Vec2::new(-d.y, d.x);
        let (tip, bl, br) = (d * 0.42, -d * 0.36 + n * 0.6, -d * 0.36 - n * 0.6);
        let t = -d * 0.4;
        let (sm, spx) = ((w + style.extra_m) * 0.5 * 0.8, style.min_px * 0.5 * 1.05);
        let c = Color::rgba(255, 255, 255, 0.92);
        p.world_shape(rel(q), &[tip, bl, bl + t, tip + t], sm, spx, c);
        p.world_shape(rel(q), &[tip, br, br + t, tip + t], sm, spx, c);
    }
}

/// The street names of the map: per lane of its network the street it belongs to, and
/// where to write each name on the city map. OMSI maps have no street names of their own,
/// but their street name signs carry them: a sign names the road its plate runs along,
/// and the name is carried on along that road until it turns or meets another name.
pub struct Streets {
    pub names: Vec<String>,
    /// Per lane: index into `names`, or `u32::MAX`.
    pub of_lane: Vec<u32>,
    /// A place to write a name: the point, the road's direction (radians, world, from +x
    /// anticlockwise) and the name.
    pub labels: Vec<(DVec2, f32, u32)>,
}

/// The heading of a sign whose plate runs along its road, relative to the road (degrees;
/// settled on the stock Verkehrszeichen_MC signs of Berlin-Spandau, see `build_streets`).
pub const SIGN_ALONG: f64 = 90.0;

pub fn build_streets(net: &Network, signs: &[(DVec3, f64, String)], hints: &[(DVec3, String)]) -> Streets {
    let t0 = std::time::Instant::now();
    let n = net.lanes.len();
    let mut names: Vec<String> = Vec::new();
    let mut index: HashMap<String, u32> = HashMap::new();
    let mut of_lane = vec![u32::MAX; n];
    // lanes into each lane, to carry names backwards
    let mut prev: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, l) in net.lanes.iter().enumerate() {
        for &j in &l.next {
            if j < n {
                prev[j].push(i);
            }
        }
    }
    let straight = |l: &omsi_sim::traffic::Lane| l.kind == LaneKind::Street && omsi_sim::traffic::wrap_deg(l.end_heading() - l.start_heading()).abs() < 30.0;
    let debug = omsi_cfg::flags::OMSI_DEBUG_NAV.is_set();
    let mut hist = [0u32; 12];
    let mut seeds: Vec<(usize, u32)> = Vec::new();
    for (pos, rot, name) in signs {
        let id = *index.entry(name.clone()).or_insert_with(|| {
            names.push(name.clone());
            (names.len() - 1) as u32
        });
        // the straight street lanes near the sign, and how well each runs along its plate
        let mut best: Option<(usize, f64)> = None;
        for i in lanes_near(net, pos.truncate(), 30.0) {
            let l = &net.lanes[i];
            if !straight(l) || l.length() < 12.0 {
                continue;
            }
            let Some((s, d)) = l.nearest_point(DVec3::new(pos.x, pos.y, l.start().z)) else { continue };
            if d > 22.0 {
                continue;
            }
            let (_, h) = l.at(s);
            // (the direction of a road does not matter: modulo 180)
            let off = (angle_diff(*rot, h as f64).abs() - SIGN_ALONG).abs();
            let off = off.min(180.0 - off);
            if debug && d < 12.0 {
                let raw = angle_diff(*rot, h as f64).rem_euclid(180.0);
                hist[((raw / 15.0) as usize).min(11)] += 1;
            }
            let score = d + off * 0.5;
            if off < 35.0 && best.map(|b| score < b.1).unwrap_or(true) {
                best = Some((i, score));
            }
        }
        if let Some((i, _)) = best {
            seeds.push((i, id));
        }
    }
    if debug {
        if let Some((pos, rot, name)) = signs.first() {
            let near = lanes_near(net, pos.truncate(), 30.0);
            let best = near.iter().filter_map(|&i| net.lanes[i].nearest_point(*pos).map(|p| (i, p.1, net.lanes[i].kind, net.lanes[i].length()))).min_by(|a, b| a.1.total_cmp(&b.1));
            log::info!("navigator: first sign '{name}' at {pos:?} heading {rot}: {} lanes near, nearest {best:?}", near.len());
        }
        // which way a plate runs: two signs of one name far apart lie along their street
        let mut along = [0u32; 12];
        for (i, (p, r, n)) in signs.iter().enumerate() {
            for (q, _, m) in &signs[i + 1..] {
                let d = (*q - *p).truncate();
                if n == m && d.length() > 150.0 && d.length() < 700.0 {
                    let h = d.x.atan2(d.y).to_degrees();
                    let raw = angle_diff(*r, h).rem_euclid(180.0);
                    along[((raw / 15.0) as usize).min(11)] += 1;
                }
            }
        }
        log::info!("navigator: sign heading minus the line to another sign of its name (mod 180): {along:?}");
        log::info!("navigator: sign heading minus road heading (mod 180, 15-degree bins): {hist:?}");
    }
    // carry each name along its road: the seed lane, its other direction, and on through
    // straight continuations for up to 1.5 km each way
    for &(seed, id) in &seeds {
        if of_lane[seed] != u32::MAX {
            continue;
        }
        let mut queue = vec![(seed, 0.0f32)];
        while let Some((i, far)) = queue.pop() {
            if of_lane[i] != u32::MAX && i != seed {
                continue;
            }
            of_lane[i] = id;
            let l = &net.lanes[i];
            if let Some(k) = l.key {
                for &j in net.by_key.get(&k).map(|v| v.as_slice()).unwrap_or(&[]) {
                    if of_lane[j] == u32::MAX {
                        of_lane[j] = id;
                    }
                }
            }
            if far > 1500.0 {
                continue;
            }
            for &j in l.next.iter() {
                let m = &net.lanes[j];
                if of_lane[j] == u32::MAX && straight(m) && omsi_sim::traffic::wrap_deg(m.start_heading() - l.end_heading()).abs() < 20.0 {
                    queue.push((j, far + m.length()));
                }
            }
            for &j in &prev[i] {
                let m = &net.lanes[j];
                if of_lane[j] == u32::MAX && straight(m) && omsi_sim::traffic::wrap_deg(l.start_heading() - m.end_heading()).abs() < 20.0 {
                    queue.push((j, far + m.length()));
                }
            }
        }
    }
    // the weaker hints (a stop named after its street): the street lane passing it on its
    // side, carried along that street as far as no sign has named it, 800 m each way at most
    for (pos, name) in hints {
        let Some(seed) = lanes_near(net, pos.truncate(), 30.0)
            .into_iter()
            .filter(|&i| straight(&net.lanes[i]) && net.lanes[i].length() >= 12.0 && of_lane[i] == u32::MAX && passes_on_its_side(net, i, *pos, 22.0))
            .min_by(|&a, &b| {
                let d = |i: usize| net.lanes[i].nearest_point(*pos).map(|p| p.1).unwrap_or(f64::MAX);
                d(a).total_cmp(&d(b))
            })
        else {
            continue;
        };
        let id = *index.entry(name.clone()).or_insert_with(|| {
            names.push(name.clone());
            (names.len() - 1) as u32
        });
        let mut queue = vec![(seed, 0.0f32)];
        while let Some((i, far)) = queue.pop() {
            if of_lane[i] != u32::MAX {
                continue;
            }
            of_lane[i] = id;
            let l = &net.lanes[i];
            if let Some(k) = l.key {
                for &j in net.by_key.get(&k).map(|v| v.as_slice()).unwrap_or(&[]) {
                    if of_lane[j] == u32::MAX {
                        of_lane[j] = id;
                    }
                }
            }
            if far > 800.0 {
                continue;
            }
            for &j in l.next.iter() {
                let m = &net.lanes[j];
                if of_lane[j] == u32::MAX && straight(m) && omsi_sim::traffic::wrap_deg(m.start_heading() - l.end_heading()).abs() < 20.0 {
                    queue.push((j, far + m.length()));
                }
            }
            for &j in &prev[i] {
                let m = &net.lanes[j];
                if of_lane[j] == u32::MAX && straight(m) && omsi_sim::traffic::wrap_deg(l.start_heading() - m.end_heading()).abs() < 20.0 {
                    queue.push((j, far + m.length()));
                }
            }
        }
    }
    // where to write them: the middle of long named lanes, a name every 350 m at most
    let mut order: Vec<usize> = (0..n).filter(|&i| of_lane[i] != u32::MAX && !net.lanes[i].reversed && net.lanes[i].length() > 30.0).collect();
    order.sort_by(|a, b| net.lanes[*b].length().total_cmp(&net.lanes[*a].length()));
    let mut labels: Vec<(DVec2, f32, u32)> = Vec::new();
    for i in order {
        let l = &net.lanes[i];
        let (q, h) = l.at(l.length() * 0.5);
        let q = q.truncate();
        if labels.iter().any(|(p, _, id)| *id == of_lane[i] && (*p - q).length() < 350.0) {
            continue;
        }
        let hr = (h as f64).to_radians();
        labels.push((q, (hr.cos()).atan2(hr.sin()) as f32, of_lane[i]));
    }
    let named = of_lane.iter().filter(|&&x| x != u32::MAX).count();
    log::info!("navigator: {} street name signs, {} names, {} of {} lanes named, {} labels, {:.0} ms", signs.len(), names.len(), named, n, labels.len(), t0.elapsed().as_secs_f64() * 1000.0);
    Streets { names, of_lane, labels }
}

// (the way back to the route and to a stop: `omsi_sim::traffic::way`)
pub use omsi_sim::traffic::way::{lanes_near, way_back, way_to_any};

/// Rotation of a 2D direction by a compass heading (for tests).
#[allow(dead_code)]
pub fn heading_vec(h: f64) -> DVec2 {
    let m = DMat3::from_rotation_z(-h.to_radians());
    m.transform_vector2(DVec2::Y)
}

/// Lane `i` passes `stop` within `reach` metres with the stop on its kerb side (the right, or
/// the left on a left-hand-traffic map), or right on it.
pub fn passes_on_its_side(net: &Network, i: usize, stop: DVec3, reach: f64) -> bool {
    let l = &net.lanes[i];
    let Some((s, d)) = l.nearest_point(stop) else { return false };
    if d > reach {
        return false;
    }
    if d < 2.5 {
        return true;
    }
    let (p, h) = l.at(s);
    let dir = DVec2::new((h as f64).to_radians().sin(), (h as f64).to_radians().cos());
    let side = dir.perp_dot((stop - p).truncate());
    if net.left_hand { side > 0.0 } else { side < 0.0 }
}

/// How busy a road is from its congestion score: 0 flowing, 2 slow, 3 heavy, 4 standing.
pub fn level(score: f32) -> usize {
    match score {
        s if s < 0.42 => 0,
        s if s < 0.66 => 2,
        s if s < 0.85 => 3,
        _ => 4,
    }
}
