//! The overhead wire of a map, read off the map itself. Trolleybus maps made for OMSI 2 draw
//! the wire as splines whose type has two thin strips high over its centre line (a few
//! centimetres wide, some 5 m up, half a metre apart): the strips are the wires, the spline's
//! centre line is where the pair runs, and the hangers and bends are scenery objects with no
//! behaviour. This finds those splines, takes their centre lines at the wires' height and
//! hands them to `omsi_trolley` as a wire network; three of them meeting end to end are a frog.
//! A `trolley.txt` beside the map overrides it (see `docs/TROLLEYBUS.md`).

use glam::DVec2;
use omsi_trolley::WireNet;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// What a spline type's profile says of it: `Some((centre, spacing, height))` for a pair of
/// wires (lateral centre of the pair, distance between them, height over the spline).
pub fn wire_profile(def: &omsi_scenery::sli::Spline) -> Option<(f64, f64, f64)> {
    let mut strips: Vec<(f64, f64)> = Vec::new();
    for p in &def.profiles {
        let pts = &p.points;
        if pts.len() < 2 {
            continue;
        }
        let (x0, x1) = pts.iter().fold((f32::MAX, f32::MIN), |a, q| (a.0.min(q.x), a.1.max(q.x)));
        let zmin = pts.iter().fold(f32::MAX, |a, q| a.min(q.z));
        let zmax = pts.iter().fold(f32::MIN, |a, q| a.max(q.z));
        // (thin, high over the road, and thin in height too: a wire, not a mast or a fence)
        if x1 - x0 <= 0.2 && zmin >= 3.0 && zmax - zmin <= 0.25 {
            strips.push((((x0 + x1) * 0.5) as f64, ((zmin + zmax) * 0.5) as f64));
        }
    }
    if def.profiles.len() > 3 || strips.len() != 2 {
        return None;
    }
    let spacing = (strips[0].0 - strips[1].0).abs();
    if !(0.2..=1.2).contains(&spacing) {
        return None;
    }
    Some(((strips[0].0 + strips[1].0) * 0.5, spacing, (strips[0].1 + strips[1].1) * 0.5))
}

/// The wire network of a map's tiles, or `None` where there is none.
pub fn derive(root: &Path, tiles: &[(i32, i32, PathBuf)], chrono_dirs: &[PathBuf]) -> Option<WireNet> {
    let text = derive_text(root, tiles, chrono_dirs)?;
    let mut net = WireNet::parse(&text);
    net.drawn_by_map = true;
    (!net.is_empty()).then_some(net)
}

/// The centre lines of the map's wire splines at the wires' height: (spacing of the pair, points).
pub fn wire_spans(root: &Path, tiles: &[(i32, i32, PathBuf)], chrono_dirs: &[PathBuf]) -> Vec<(f64, Vec<[f64; 3]>)> {
    let ts = omsi_map::tile_size();
    let mut slis: HashMap<String, Option<(f64, f64, f64, omsi_scenery::sli::Spline)>> = HashMap::new();
    let mut spans: Vec<(f64, Vec<[f64; 3]>)> = Vec::new();
    for (tx, ty, path) in tiles {
        let Some(tile) = omsi_world::map_index::read_tile(path, chrono_dirs) else { continue };
        let o2 = DVec2::new(*tx as f64 * ts, *ty as f64 * ts);
        for sp in tile.splines.iter().filter(|s| !s.deleted && !s.file.trim().is_empty() && s.length >= 0.5) {
            let key = sp.file.trim().replace('\\', "/").to_ascii_lowercase();
            let entry = slis.entry(key).or_insert_with(|| {
                let def = omsi_scenery::sli::Spline::load(&omsi_cfg::resolve_path(root, &sp.file)).ok()?;
                let (c, s, h) = wire_profile(&def)?;
                Some((c, s, h, def))
            });
            let Some((centre, spacing, height, def)) = entry.as_ref() else { continue };
            let curve = omsi_geometry::SplineCurve::from_map(sp, o2).with_sli(def);
            let side = if sp.mirror { -1.0 } else { 1.0 };
            let turn_deg = if curve.radius != 0.0 { (curve.length / curve.radius).abs().to_degrees() } else { 0.0 };
            let n = ((curve.length / 3.0).ceil() as usize).max((turn_deg / 1.5).ceil() as usize).clamp(1, 600);
            let pts: Vec<[f64; 3]> = (0..=n)
                .map(|k| {
                    let (p, _) = omsi_geometry::skewed_point(&curve, curve.length * k as f64 / n as f64, side * centre, *height);
                    [p.x, p.y, p.z]
                })
                .collect();
            spans.push((*spacing, pts));
        }
    }
    spans
}

/// A branch drawn from the middle of another wire (its end lies on that span, not at its end)
/// is a frog too: the span it leaves is cut there, and the branch's end moved onto the cut, so
/// three ends meet. Authors lay a junction either way.
///
/// One pass: the ends in a grid, the segments in a grid, every lone end looked up in its own
/// cell, the cuts of a span applied from its far end backwards.
pub fn split_t_junctions(spans: &mut Vec<(f64, Vec<[f64; 3]>)>) {
    let join = omsi_trolley::JOIN;
    // a cut needs room for a joint on both sides, or it leaves a stub
    const MARGIN: f64 = 2.0;
    const CELL: f64 = 8.0;
    let cell = |x: f64, y: f64| ((x / CELL).floor() as i64, (y / CELL).floor() as i64);
    let mut ends: HashMap<(i64, i64), Vec<(usize, [f64; 3])>> = HashMap::new();
    for (i, (_, pts)) in spans.iter().enumerate() {
        for p in [pts[0], *pts.last().unwrap()] {
            ends.entry(cell(p[0], p[1])).or_default().push((i, p));
        }
    }
    let mut segs: HashMap<(i64, i64), Vec<(usize, usize)>> = HashMap::new();
    let mut cum: Vec<Vec<f64>> = Vec::with_capacity(spans.len());
    for (j, (_, pts)) in spans.iter().enumerate() {
        let mut c = vec![0.0];
        for (k, w) in pts.windows(2).enumerate() {
            c.push(c[k] + (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]));
            let (c0, c1) = (cell(w[0][0].min(w[1][0]) - join, w[0][1].min(w[1][1]) - join), cell(w[0][0].max(w[1][0]) + join, w[0][1].max(w[1][1]) + join));
            for cx in c0.0..=c1.0 {
                for cy in c0.1..=c1.1 {
                    segs.entry((cx, cy)).or_default().push((j, k));
                }
            }
        }
        cum.push(c);
    }
    // (span, segment, arc length of the cut, the cut point) per span; the branch ends to move
    let mut cuts: BTreeMap<usize, Vec<(usize, f64, [f64; 3])>> = BTreeMap::new();
    let mut moves: Vec<(usize, u8, [f64; 3])> = Vec::new();
    for (i, (_, pts)) in spans.iter().enumerate() {
        for end in 0..2u8 {
            let p = if end == 0 { pts[0] } else { *pts.last().unwrap() };
            let (cx, cy) = cell(p[0], p[1]);
            let lone = !(-1..=1).flat_map(|dx| (-1..=1).map(move |dy| (cx + dx, cy + dy))).filter_map(|c| ends.get(&c)).flatten().any(|(j, q)| *j != i && (q[0] - p[0]).hypot(q[1] - p[1]).hypot(q[2] - p[2]) <= join);
            if !lone {
                continue;
            }
            let mut best: Option<(f64, usize, usize, f64, [f64; 3])> = None;
            for (j, k) in segs.get(&(cx, cy)).into_iter().flatten().copied() {
                if j == i {
                    continue;
                }
                let q = &spans[j].1;
                let (a, b) = (q[k], q[k + 1]);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let l2 = dx * dx + dy * dy;
                if l2 < 1e-9 {
                    continue;
                }
                let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0);
                let (x, y) = (a[0] + t * dx, a[1] + t * dy);
                let z = a[2] + t * (b[2] - a[2]);
                let d = (p[0] - x).hypot(p[1] - y).hypot(p[2] - z);
                let at = cum[j][k] + t * l2.sqrt();
                let total = *cum[j].last().unwrap();
                if d <= join && at > MARGIN && total - at > MARGIN && best.is_none_or(|b| d < b.0) {
                    best = Some((d, j, k, at, [x, y, z]));
                }
            }
            if let Some((_, j, k, at, q)) = best {
                cuts.entry(j).or_default().push((k, at, q));
                moves.push((i, end, q));
            }
        }
    }
    // (a cut within MARGIN of another on its span is left out, and its branch stays loose)
    let mut accepted: BTreeMap<usize, Vec<(usize, f64, [f64; 3])>> = BTreeMap::new();
    for (j, mut list) in cuts {
        list.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut kept: Vec<(usize, f64, [f64; 3])> = Vec::new();
        for c in list {
            if kept.last().is_none_or(|l| c.1 - l.1 >= MARGIN) {
                kept.push(c);
            }
        }
        accepted.insert(j, kept);
    }
    let kept_cuts: std::collections::HashSet<[u64; 3]> = accepted.values().flatten().map(|c| [c.2[0].to_bits(), c.2[1].to_bits(), c.2[2].to_bits()]).collect();
    let mut tails: Vec<(f64, Vec<[f64; 3]>)> = Vec::new();
    for (j, list) in &accepted {
        let spacing = spans[*j].0;
        let mut cur = std::mem::take(&mut spans[*j].1);
        for (k, _, q) in list.iter().rev() {
            let mut tail = vec![*q];
            tail.extend_from_slice(&cur[k + 1..]);
            cur.truncate(k + 1);
            cur.push(*q);
            tails.push((spacing, tail));
        }
        spans[*j].1 = cur;
    }
    spans.extend(tails);
    for (i, end, q) in moves {
        // (only the branches whose cut was kept: its end is within the join distance of the cut)
        let b = &mut spans[i].1;
        let e = if end == 0 { b[0] } else { *b.last().unwrap() };
        if (e[0] - q[0]).hypot(e[1] - q[1]).hypot(e[2] - q[2]) <= join && kept_cuts.contains(&[q[0].to_bits(), q[1].to_bits(), q[2].to_bits()]) {
            if end == 0 { b[0] = q } else { *b.last_mut().unwrap() = q }
        }
    }
}

/// The same as the `trolley.txt` text that says it.
pub fn derive_text(root: &Path, tiles: &[(i32, i32, PathBuf)], chrono_dirs: &[PathBuf]) -> Option<String> {
    let mut spans = wire_spans(root, tiles, chrono_dirs);
    // (a piece shorter than the joining distance would join its own two ends: the wire
    // closes over it as it is; measured in 3-D, as `WireNet::parse` does)
    let join = omsi_trolley::JOIN;
    spans.retain(|(_, pts)| {
        let (a, b) = (pts[0], pts[pts.len() - 1]);
        let d = |p: &[f64; 3], q: &[f64; 3]| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
        let len: f64 = pts.windows(2).map(|w| d(&w[0], &w[1])).sum();
        !(d(&a, &b) <= join && len < 2.0 * join)
    });
    split_t_junctions(&mut spans);
    if spans.is_empty() {
        return None;
    }
    let mut out = String::from("[trolley_net]\nvoltage 600\nspacing 0.52\n\n[trolley_section]\nid 1\nname map\nlive 1\nregen_kw 0\nfeeder_ohm 0.04\n\n");
    // (`OMSI_TROLLEY_DEAD_AT=x,y,r;...`: the wire within r metres of a point is a dead section)
    let zones: Vec<[f64; 3]> = omsi_cfg::flags::OMSI_TROLLEY_DEAD_AT
        .var()
        .map(|v| v.split(';').filter_map(|z| {
            let n: Vec<f64> = z.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (n.len() == 3).then(|| [n[0], n[1], n[2]])
        }).collect())
        .unwrap_or_default();
    if !zones.is_empty() {
        out.push_str("[trolley_section]\nid 2\nname dead\nlive 0\n\n");
    }
    let dead = |p: &[f64; 3]| zones.iter().any(|z| (p[0] - z[0]).hypot(p[1] - z[1]) <= z[2]);
    let seg_dead = |a: &[f64; 3], b: &[f64; 3]| dead(&[(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, 0.0]);
    for (spacing, pts) in &spans {
        // runs of segments on one side of a zone's edge, each a span of its own (the next one
        // starts where it ends: a dead section is joined to a live one by an insulator)
        let mut start = 0;
        for k in 1..pts.len() {
            let last = k + 1 == pts.len();
            let change = !last && seg_dead(&pts[k - 1], &pts[k]) != seg_dead(&pts[k], &pts[k + 1]);
            if last || change {
                let d = !zones.is_empty() && seg_dead(&pts[start], &pts[start + 1]);
                out.push_str(&format!("[trolley_span]\nsection {}\nspacing {spacing:.3}\n", if d { 2 } else { 1 }));
                for p in &pts[start..=k] {
                    out.push_str(&format!("p {:.3} {:.3} {:.3}\n", p[0], p[1], p[2]));
                }
                out.push('\n');
                start = k;
            }
        }
    }
    // frogs: three span ends within the network's joining distance of each other
    let mut ends: Vec<[f64; 3]> = Vec::new();
    for (_, pts) in &spans {
        ends.push(pts[0]);
        ends.push(*pts.last().unwrap());
    }
    let mut used = vec![false; ends.len()];
    let mut n_frogs = 0;
    // (the ends in a grid of cells: a search through all of them for each end is quadratic in a
    // map's thousands)
    let join = omsi_trolley::JOIN;
    let cell = |p: &[f64; 3]| ((p[0] / join).floor() as i64, (p[1] / join).floor() as i64);
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (j, e) in ends.iter().enumerate() {
        grid.entry(cell(e)).or_default().push(j);
    }
    for i in 0..ends.len() {
        if used[i] {
            continue;
        }
        let (cx, cy) = cell(&ends[i]);
        let near: Vec<usize> = (-1..=1)
            .flat_map(|dx| (-1..=1).map(move |dy| (cx + dx, cy + dy)))
            .filter_map(|c| grid.get(&c))
            .flatten()
            .copied()
            .filter(|j| (ends[*j][0] - ends[i][0]).hypot(ends[*j][1] - ends[i][1]).hypot(ends[*j][2] - ends[i][2]) <= join)
            .collect();
        for j in &near {
            used[*j] = true;
        }
        if near.len() == 3 {
            let p = ends[i];
            out.push_str(&format!("[trolley_switch]\nname frog {n_frogs}\npos {:.3} {:.3} {:.3}\nmode power\ntrigger 9\nsense 12\n\n", p[0], p[1], p[2]));
            log::info!("trolley frog {n_frogs} at ({:.1}, {:.1})", p[0], p[1]);
            n_frogs += 1;
        }
    }
    log::info!("trolley wire derived from the map's splines: {} spans, {} frogs", spans.len(), n_frogs);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(strips: &[(f32, f32)]) -> omsi_scenery::sli::Spline {
        let mut s = omsi_scenery::sli::Spline::default();
        for (x, z) in strips {
            s.profiles.push(omsi_scenery::sli::SplineProfile {
                texture: 0,
                points: vec![
                    omsi_scenery::sli::SplineProfilePoint { x: x - 0.01, z: *z, u: 0.0, v_scale: 0.2 },
                    omsi_scenery::sli::SplineProfilePoint { x: x + 0.01, z: *z + 0.02, u: 0.0, v_scale: 0.2 },
                ],
            });
        }
        s
    }

    #[test]
    fn two_thin_strips_high_up_are_a_pair_of_wires() {
        let (c, s, h) = wire_profile(&profile(&[(-0.26, 5.04), (0.26, 5.04)])).unwrap();
        assert!(c.abs() < 1e-6 && (s - 0.52).abs() < 1e-6 && (h - 5.05).abs() < 0.02);
        // one strip is a tram's wire, strips on the ground are road paint, a wide strip a roof
        assert!(wire_profile(&profile(&[(0.0, 5.04)])).is_none());
        assert!(wire_profile(&profile(&[(-0.26, 0.0), (0.26, 0.0)])).is_none());
        assert!(wire_profile(&profile(&[(-1.0, 5.0), (1.0, 5.0)])).is_none());
    }

    #[test]
    fn a_branch_drawn_from_the_middle_of_a_wire_makes_a_frog() {
        let straight: Vec<[f64; 3]> = (0..=10).map(|k| [k as f64 * 10.0, 0.0, 5.0]).collect();
        let branch: Vec<[f64; 3]> = vec![[50.3, 0.4, 5.0], [60.0, 4.0, 5.0], [70.0, 10.0, 5.0]];
        let mut spans = vec![(0.52, straight), (0.52, branch)];
        split_t_junctions(&mut spans);
        assert_eq!(spans.len(), 3);
        // a branch that only ends beside the wire's end or near its start is left alone
        let mut alone = vec![(0.52, (0..=10).map(|k| [k as f64 * 10.0, 0.0, 5.0]).collect::<Vec<_>>()), (0.52, vec![[0.5, 0.5, 5.0], [10.0, 5.0, 5.0]])];
        split_t_junctions(&mut alone);
        assert_eq!(alone.len(), 2);
    }

    #[test]
    fn several_branches_on_one_wire_are_all_cut_in_one_pass() {
        let straight: Vec<[f64; 3]> = (0..=20).map(|k| [k as f64 * 10.0, 0.0, 5.0]).collect();
        let mut spans = vec![
            (0.52, straight),
            (0.52, vec![[50.0, 0.3, 5.0], [60.0, 4.0, 5.0]]),
            (0.52, vec![[120.0, -0.4, 5.0], [130.0, -5.0, 5.0]]),
            (0.52, vec![[150.2, 0.2, 5.0], [160.0, 6.0, 5.0]]),
        ];
        split_t_junctions(&mut spans);
        // the long wire in four pieces, every piece starting where the last ended
        assert_eq!(spans.len(), 7);
        let long: Vec<&Vec<[f64; 3]>> = spans.iter().filter(|s| s.1.len() > 2 || s.1[0][0] == s.1[1][0] - 10.0).map(|s| &s.1).collect();
        assert!(long.len() >= 4);
        // the branches start exactly on a cut
        for b in 1..4 {
            let e = spans[b].1[0];
            assert!(spans.iter().enumerate().any(|(n, s)| n != b && (s.1[0] == e || *s.1.last().unwrap() == e)), "branch {b}");
        }
    }

    #[test]
    fn wires_on_different_levels_do_not_make_a_junction() {
        let mut spans = vec![
            (0.52, vec![[0.0, 0.0, 5.0], [100.0, 0.0, 5.0]]),
            (0.52, vec![[50.0, 0.3, 8.0], [60.0, 5.0, 8.0]]),
        ];
        let original = spans.clone();
        split_t_junctions(&mut spans);
        assert_eq!(spans, original, "a crossing above the wire must not be snapped down");
    }

    #[test]
    fn an_end_above_a_real_branch_does_not_hide_the_junction() {
        let mut spans = vec![
            (0.52, vec![[0.0, 0.0, 5.0], [100.0, 0.0, 5.0]]),
            (0.52, vec![[50.0, 0.3, 5.0], [60.0, 5.0, 5.0]]),
            (0.52, vec![[50.0, 0.5, 8.0], [60.0, 5.0, 8.0]]),
        ];
        split_t_junctions(&mut spans);
        assert_eq!(spans.len(), 4);
        assert_eq!(spans[1].1[0], [50.0, 0.0, 5.0]);
        assert_eq!(spans[2].1[0], [50.0, 0.5, 8.0]);
    }

    #[test]
    fn the_cut_spans_come_in_the_same_order_every_time() {
        let make = || {
            let mut v: Vec<(f64, Vec<[f64; 3]>)> = (0..6).map(|n| (0.52, (0..=20).map(|k| [k as f64 * 10.0, n as f64 * 40.0, 5.0]).collect())).collect();
            for n in 0..6 {
                for x in [40.0, 100.0, 160.0] {
                    v.push((0.52, vec![[x, n as f64 * 40.0 + 0.3, 5.0], [x + 8.0, n as f64 * 40.0 + 9.0, 5.0]]));
                }
            }
            split_t_junctions(&mut v);
            v
        };
        let first = make();
        for _ in 0..5 {
            assert_eq!(make(), first);
        }
    }
}
