//! The roads of the launcher's map as they are drawn: the carriageways the game's own city map
//! groups (`navigator::city_roads`), with the pieces of one road that run on into each other
//! joined into one line. A road is a spline or a crossing's path at a time, so a street is
//! many pieces end to end; drawn one by one, every piece ended in a round cap of its own and a
//! road was a row of beads (a cap is a polygon), with a seam at every place one piece met the
//! next. Joined, a road is one surface with mitred or round joins all along.
//!
//! The lines are made once per map read, off the interface thread (`read_map`); the plan
//! thins them for its zoom band (`MapView::build`).

use crate::host::MapRoad;
use glam::{DVec2, DVec3, Vec3};
use hashbrown::HashMap;

/// A road as the map draws it: its middle in metres from the map's origin, how wide it is,
/// and whether it is a main road.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Line {
    pub points: Vec<Vec3>,
    pub width: f32,
    pub main: bool,
}

/// Two ends at most this far apart are one place (metres): pieces of one spline network meet
/// to the millimetre, a crossing's paths to a few centimetres.
const JOIN: f64 = 0.12;
/// Two ends this far apart in height are not one place: a bridge's end over a junction.
const SAME_LEVEL: f64 = 1.0;
/// Pieces whose widths differ by more than this are not one road: where a street widens or a
/// lane begins, each keeps its own width and the two meet in round ends.
const SAME_WIDTH: f32 = 0.35;
/// Where more than two ends meet (a junction), the two that run on most nearly straight are
/// one road if they turn by less than this much (cosine of the angle between them: 30
/// degrees); the others end there.
const STRAIGHT_ON: f64 = 0.866;

/// The roads as lines: every piece of `roads` that continues another end to end, at the same
/// height and of about the same width, joined to it - at a place where only the two meet,
/// whatever the angle; at a junction, the two that run on straight - and the points measured
/// from `origin`.
pub(super) fn lines(roads: &[MapRoad], origin: DVec2) -> Vec<Line> {
    // the pieces, without points that make no direction (closer than a centimetre)
    let pieces: Vec<(Vec<DVec3>, f32, bool)> = roads
        .iter()
        .filter_map(|r| {
            let mut pts: Vec<DVec3> = Vec::with_capacity(r.points.len());
            for q in &r.points {
                if pts.last().is_none_or(|l: &DVec3| (*q - *l).truncate().length_squared() > 1e-4) {
                    pts.push(*q);
                }
            }
            (pts.len() >= 2).then_some((pts, r.width, r.main))
        })
        .collect();
    // every piece's two ends (end `2 i` is piece i's first point, `2 i + 1` its last), with
    // the way the piece leaves that end
    let end_at = |e: usize| -> (DVec3, DVec2) {
        let pts = &pieces[e / 2].0;
        if e % 2 == 0 {
            (pts[0], (pts[1] - pts[0]).truncate().normalize_or_zero())
        } else {
            let n = pts.len();
            (pts[n - 1], (pts[n - 2] - pts[n - 1]).truncate().normalize_or_zero())
        }
    };
    let cell = |p: DVec3| ((p.x / JOIN).floor() as i64, (p.y / JOIN).floor() as i64);
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::with_capacity(pieces.len() * 2);
    for e in 0..pieces.len() * 2 {
        grid.entry(cell(end_at(e).0)).or_default().push(e);
    }
    // the places: the ends near each other, each place once (by its first end)
    let mut partner = vec![usize::MAX; pieces.len() * 2];
    let mut placed = vec![false; pieces.len() * 2];
    for e in 0..pieces.len() * 2 {
        if placed[e] {
            continue;
        }
        let (p, _) = end_at(e);
        let (cx, cy) = cell(p);
        let mut here: Vec<usize> = Vec::new();
        for x in cx - 1..=cx + 1 {
            for y in cy - 1..=cy + 1 {
                for &o in grid.get(&(x, y)).map(Vec::as_slice).unwrap_or(&[]) {
                    let q = end_at(o).0;
                    if !placed[o] && (q - p).truncate().length() <= JOIN && (q.z - p.z).abs() <= SAME_LEVEL {
                        here.push(o);
                    }
                }
            }
        }
        for &o in &here {
            placed[o] = true;
        }
        let alike = |a: usize, b: usize| {
            let (pa, pb) = (&pieces[a / 2], &pieces[b / 2]);
            a / 2 != b / 2 && (pa.1 - pb.1).abs() <= SAME_WIDTH && pa.2 == pb.2
        };
        if here.len() == 2 {
            if alike(here[0], here[1]) {
                partner[here[0]] = here[1];
                partner[here[1]] = here[0];
            }
            continue;
        }
        // a junction: the straightest pairs first (two ends that leave it the opposite way)
        let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
        for (i, &a) in here.iter().enumerate() {
            for &b in &here[i + 1..] {
                let turn = -end_at(a).1.dot(end_at(b).1);
                if alike(a, b) && turn >= STRAIGHT_ON {
                    pairs.push((turn, a, b));
                }
            }
        }
        pairs.sort_by(|x, y| y.0.total_cmp(&x.0));
        for (_, a, b) in pairs {
            if partner[a] == usize::MAX && partner[b] == usize::MAX {
                partner[a] = b;
                partner[b] = a;
            }
        }
    }
    // the lines: from every free end along its partners; what is left is rings
    let rel = |q: DVec3| Vec3::new((q.x - origin.x) as f32, (q.y - origin.y) as f32, 0.0);
    let mut used = vec![false; pieces.len()];
    let mut out = Vec::new();
    let walk = |first: usize, used: &mut Vec<bool>, out: &mut Vec<Line>| {
        let mut points: Vec<Vec3> = Vec::new();
        let (mut width, main) = (0.0f32, pieces[first / 2].2);
        let mut e = first;
        loop {
            let i = e / 2;
            if used[i] {
                break;
            }
            used[i] = true;
            let (pts, w, _) = &pieces[i];
            width = width.max(*w);
            // (the piece's first point is where the line already is)
            let skip = usize::from(!points.is_empty());
            if e % 2 == 0 {
                points.extend(pts.iter().skip(skip).map(|q| rel(*q)));
            } else {
                points.extend(pts.iter().rev().skip(skip).map(|q| rel(*q)));
            }
            // on from the piece's other end
            match partner[e ^ 1] {
                usize::MAX => break,
                next => e = next,
            }
        }
        out.push(Line { points, width, main });
    };
    for e in 0..pieces.len() * 2 {
        if partner[e] == usize::MAX && !used[e / 2] {
            walk(e, &mut used, &mut out);
        }
    }
    for i in 0..pieces.len() {
        if !used[i] {
            walk(2 * i, &mut used, &mut out);
        }
    }
    out
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::*;
    use super::{lines, Line, MapRoad};

    /// The four sample places of a pixel the GPU's standard 4x multisampling uses (D3D's and
    /// Vulkan's), in pixels from its top left corner; one sample is the pixel's middle.
    pub(crate) const MSAA4: [(f32, f32); 4] = [(0.375, 0.125), (0.875, 0.375), (0.125, 0.625), (0.625, 0.875)];
    pub(crate) const MSAA1: [(f32, f32); 1] = [(0.5, 0.5)];

    fn to_linear(c: f32) -> f32 {
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }
    fn to_srgb(c: f32) -> f32 {
        if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
    }

    /// World vertices as `ui.wgsl` places them for an orthographic camera of `mpp` metres a
    /// pixel looking at `center`, in the picture's pixels (y down).
    fn place(v: &Vertex, center: Vec2, mpp: f32, w: usize, h: usize) -> Vec2 {
        let edge = if v.mode[1] < 0.5 { v.uv[0] } else { 0.0 };
        let width = v.width[0].max(v.width[1] * mpp) + edge * mpp;
        let p = Vec2::new(v.pos[0], v.pos[1]) + Vec2::from(v.ext) * width;
        Vec2::new((p.x - center.x) / mpp + w as f32 * 0.5, h as f32 * 0.5 - (p.y - center.y) / mpp)
    }

    /// The picture a list of world vertices makes, worked out on the CPU as the GPU draws it:
    /// every sample takes the colour of the last triangle over it (straight alpha blended in
    /// linear light), and a pixel is the mean of its samples. Linear RGB per pixel.
    pub(crate) fn rasterise(verts: &[Vertex], center: Vec2, mpp: f32, w: usize, h: usize, samples: &[(f32, f32)], ground: [f32; 3]) -> Vec<[f32; 3]> {
        let n = samples.len();
        let mut buf = vec![ground; w * h * n];
        for t in verts.chunks_exact(3) {
            let (a, b, c) = (place(&t[0], center, mpp, w, h), place(&t[1], center, mpp, w, h), place(&t[2], center, mpp, w, h));
            let area = (b - a).perp_dot(c - a);
            if area.abs() < 1e-9 {
                continue;
            }
            let lo = a.min(b).min(c).floor().max(Vec2::ZERO);
            let hi = a.max(b).max(c).ceil().min(Vec2::new(w as f32, h as f32));
            if lo.x >= hi.x || lo.y >= hi.y {
                continue;
            }
            let col = t[0].color;
            let lin = [to_linear(col[0]), to_linear(col[1]), to_linear(col[2])];
            let alpha = col[3];
            for y in lo.y as usize..hi.y as usize {
                for x in lo.x as usize..hi.x as usize {
                    for (k, (sx, sy)) in samples.iter().enumerate() {
                        let p = Vec2::new(x as f32 + sx, y as f32 + sy);
                        let (e0, e1, e2) = ((b - a).perp_dot(p - a), (c - b).perp_dot(p - b), (a - c).perp_dot(p - c));
                        let inside = if area > 0.0 { e0 >= 0.0 && e1 >= 0.0 && e2 >= 0.0 } else { e0 <= 0.0 && e1 <= 0.0 && e2 <= 0.0 };
                        if inside {
                            let d = &mut buf[(y * w + x) * n + k];
                            for i in 0..3 {
                                d[i] = lin[i] * alpha + d[i] * (1.0 - alpha);
                            }
                        }
                    }
                }
            }
        }
        (0..w * h)
            .map(|i| {
                let mut s = [0.0f32; 3];
                for k in 0..n {
                    for (j, v) in s.iter_mut().enumerate() {
                        *v += buf[i * n + k][j] / n as f32;
                    }
                }
                s
            })
            .collect()
    }

    /// A rasterised picture as a PNG.
    pub(crate) fn save(px: &[[f32; 3]], w: usize, h: usize, file: &std::path::Path) {
        let mut img = image::RgbaImage::new(w as u32, h as u32);
        for (i, p) in px.iter().enumerate() {
            let c = |v: f32| (to_srgb(v.clamp(0.0, 1.0)) * 255.0).round() as u8;
            img.put_pixel((i % w) as u32, (i / w) as u32, image::Rgba([c(p[0]), c(p[1]), c(p[2]), 255]));
        }
        img.save(file).expect("png");
    }

    // --- the lines --------------------------------------------------------------------------

    fn road(pts: &[(f64, f64)], width: f32, main: bool) -> MapRoad {
        MapRoad { points: pts.iter().map(|(x, y)| DVec3::new(*x, *y, 0.0)).collect(), width, main }
    }

    fn xy(l: &Line) -> Vec<(f32, f32)> {
        l.points.iter().map(|p| (p.x, p.y)).collect()
    }

    #[test]
    fn pieces_that_run_on_into_each_other_are_one_line() {
        // the second piece runs the other way, the third meets it a few centimetres off
        let pieces = [road(&[(0.0, 0.0), (10.0, 0.0)], 7.0, false), road(&[(20.0, 0.0), (10.0, 0.0)], 7.2, false), road(&[(20.05, 0.03), (30.0, 5.0)], 7.0, false)];
        let l = lines(&pieces, DVec2::ZERO);
        assert_eq!(l.len(), 1);
        assert_eq!(xy(&l[0]), vec![(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 5.0)]);
        assert_eq!(l[0].width, 7.2);
        // measured from the origin, in either direction along the line
        let l = lines(&pieces, DVec2::new(100.0, 50.0));
        assert!(xy(&l[0]) == vec![(-100.0, -50.0), (-90.0, -50.0), (-80.0, -50.0), (-70.0, -45.0)] || xy(&l[0]) == vec![(-70.0, -45.0), (-80.0, -50.0), (-90.0, -50.0), (-100.0, -50.0)]);
    }

    #[test]
    fn a_junction_keeps_the_road_through_it_whole() {
        // a street west to east in two pieces, and a side street off it to the north
        let pieces = [road(&[(-10.0, 0.0), (0.0, 0.0)], 7.0, false), road(&[(0.0, 10.0), (0.0, 0.0)], 7.0, false), road(&[(0.0, 0.0), (10.0, 1.0)], 7.0, false)];
        let mut l = lines(&pieces, DVec2::ZERO);
        l.sort_by_key(|l| l.points.len());
        assert_eq!(l.len(), 2);
        assert_eq!(xy(&l[0]), vec![(0.0, 10.0), (0.0, 0.0)]);
        assert_eq!(xy(&l[1]), vec![(-10.0, 0.0), (0.0, 0.0), (10.0, 1.0)]);
        // where two meet at a corner, whatever its angle, they are one line round it
        let corner = lines(&[road(&[(0.0, 0.0), (10.0, 0.0)], 7.0, true), road(&[(10.0, 0.0), (10.0, 10.0)], 7.0, true)], DVec2::ZERO);
        assert_eq!(corner.len(), 1);
        assert!(corner[0].main);
    }

    #[test]
    fn a_bridge_another_width_or_another_kind_of_road_is_not_joined() {
        let mut bridge = road(&[(10.0, 0.0), (20.0, 0.0)], 7.0, false);
        bridge.points.iter_mut().for_each(|p| p.z = 6.0);
        let base = || road(&[(0.0, 0.0), (10.0, 0.0)], 7.0, false);
        assert_eq!(lines(&[base(), bridge], DVec2::ZERO).len(), 2);
        assert_eq!(lines(&[base(), road(&[(10.0, 0.0), (20.0, 0.0)], 3.6, false)], DVec2::ZERO).len(), 2);
        assert_eq!(lines(&[base(), road(&[(10.0, 0.0), (20.0, 0.0)], 7.0, true)], DVec2::ZERO).len(), 2);
    }

    #[test]
    fn a_roundabout_is_one_ring() {
        // its ring in four quarters, each a crossing's path of its own
        let quarter = |k: usize| {
            let pts: Vec<(f64, f64)> = (0..=8).map(|i| {
                let a = std::f64::consts::FRAC_PI_2 * (k as f64 + i as f64 / 8.0);
                (15.0 * a.cos(), 15.0 * a.sin())
            }).collect();
            road(&pts, 2.6, false)
        };
        let l = lines(&[quarter(0), quarter(2), quarter(1), quarter(3)], DVec2::ZERO);
        assert_eq!(l.len(), 1);
        let (a, b) = (l[0].points[0], l[0].points[l[0].points.len() - 1]);
        assert!(a.distance(b) < 1e-4, "it closes on itself: {a} {b}");
        assert_eq!(l[0].points.len(), 33);
        // (each point of it on the ring)
        assert!(l[0].points.iter().all(|p| (p.length() - 15.0).abs() < 1e-3));
    }

    // --- the pictures of them ---------------------------------------------------------------

    fn linear(c: Color) -> [f32; 3] {
        [to_linear(c.0[0]), to_linear(c.0[1]), to_linear(c.0[2])]
    }

    fn same(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
    }

    /// The roads drawn as the map draws them at `mpp` metres a pixel round `at`, one sample a
    /// pixel: the picture and the world point of each pixel's middle.
    fn picture(pieces: Vec<MapRoad>, mpp: f64, at: Vec2, (w, h): (usize, usize)) -> (Vec<[f32; 3]>, impl Fn(usize) -> Vec2) {
        let lines = lines(&pieces, DVec2::ZERO);
        let roads = Arc::new(Roads { roads: pieces, lines, ..Default::default() });
        let mut m = MapView::new();
        m.roads = Some(roads.clone());
        m.mpp = mpp;
        let (verts, _) = m.build(&roads, m.tolerance(), palette(true));
        let px = rasterise(&verts, at, mpp as f32, w, h, &MSAA1, [0.0; 3]);
        let mpp = mpp as f32;
        (px, move |i: usize| at + Vec2::new(((i % w) as f32 + 0.5 - w as f32 * 0.5) * mpp, (h as f32 * 0.5 - ((i / w) as f32 + 0.5)) * mpp))
    }

    /// How far a point is from a polyline.
    fn off(q: Vec2, pts: &[(f32, f32)]) -> f32 {
        pts.windows(2)
            .map(|s| {
                let (a, b) = (Vec2::from(s[0]), Vec2::from(s[1]));
                let t = ((q - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
                q.distance(a + (b - a) * t)
            })
            .fold(f32::MAX, f32::min)
    }

    /// Half the width a road of `w` metres is drawn at.
    fn half(w: f32) -> f32 {
        (w.max(LANE_MIN) + 2.0 * ROAD_MARGIN) * 0.5
    }

    #[test]
    fn pieces_joined_at_a_bend_leave_no_seam() {
        // a street in three pieces bending 35 and then 50 degrees, drawn close up: everywhere
        // on it is its surface, and its casing runs along its edge
        let path = [(-30.0f32, 0.0f32), (0.0, 0.0), (24.0, 17.0), (30.0, 40.0)];
        let pieces = path.windows(2).map(|s| road(&[(s[0].0 as f64, s[0].1 as f64), (s[1].0 as f64, s[1].1 as f64)], 7.0, false)).collect();
        let mpp = 0.15;
        let (px, world) = picture(pieces, mpp, Vec2::new(5.0, 10.0), (300, 300));
        let (road_c, casing) = (linear(ROAD), linear(ROAD_CASING));
        let mut edge = 0;
        for (i, c) in px.iter().enumerate() {
            let d = off(world(i), &path);
            if d < half(7.0) - mpp as f32 {
                assert!(same(*c, road_c), "a seam at {} ({d:.2} m from the middle): {c:?}", world(i));
            }
            edge += usize::from(same(*c, casing));
        }
        assert!(edge > 100, "the casing is drawn ({edge} pixels)");
    }

    #[test]
    fn the_lanes_of_a_road_side_by_side_are_one_surface() {
        // a crossing's two paths into a roundabout, each as wide as a vehicle, a lane apart:
        // between them is road, not a dark line
        let pieces = vec![road(&[(-1.75, -30.0), (-1.75, 30.0)], 2.6, false), road(&[(1.75, 30.0), (1.75, -30.0)], 2.6, false)];
        let (px, world) = picture(pieces, 0.1, Vec2::ZERO, (120, 120));
        for (i, c) in px.iter().enumerate() {
            if world(i).x.abs() < 1.75 + half(2.6) - 0.1 {
                assert!(same(*c, linear(ROAD)), "between the lanes at {}: {c:?}", world(i));
            }
        }
    }

    #[test]
    fn a_crossing_is_not_cut_by_either_roads_casing() {
        // a main road and a side street across it, and one ending on it
        let pieces = vec![
            road(&[(-30.0, 0.0), (30.0, 0.0)], 7.0, true),
            road(&[(-20.0, -20.0), (20.0, 20.0)], 6.0, false),
            road(&[(10.0, -25.0), (10.0, 0.0)], 5.0, false),
        ];
        let (px, world) = picture(pieces, 0.12, Vec2::ZERO, (400, 400));
        let casing = linear(ROAD_CASING);
        for (i, c) in px.iter().enumerate() {
            let q = world(i);
            let inside = off(q, &[(-30.0, 0.0), (30.0, 0.0)]) < half(7.0) - 0.12 || off(q, &[(-20.0, -20.0), (20.0, 20.0)]) < half(6.0) - 0.12 || off(q, &[(10.0, -25.0), (10.0, 0.0)]) < half(5.0) - 0.12;
            if inside {
                assert!(!same(*c, casing), "casing over a road at {q}");
            }
        }
        // the main road lies over the side streets
        let at = |p: Vec2| px[(0..px.len()).min_by(|a, b| world(*a).distance(p).total_cmp(&world(*b).distance(p))).unwrap()];
        assert!(same(at(Vec2::new(0.0, 0.0)), linear(ROAD_MAIN)));
        assert!(same(at(Vec2::new(10.0, -1.0)), linear(ROAD_MAIN)));
        assert!(same(at(Vec2::new(10.0, -8.0)), linear(ROAD)));
    }

    #[test]
    fn a_main_road_ends_square_on_the_side_street_it_runs_on_as() {
        // the same street, a main road up to x = 0 and a side street from there on
        let pieces = vec![road(&[(-30.0, 0.0), (0.0, 0.0)], 7.0, true), road(&[(0.0, 0.0), (30.0, 0.0)], 7.0, false)];
        let (px, world) = picture(pieces, 0.1, Vec2::ZERO, (200, 100));
        for (i, c) in px.iter().enumerate() {
            let q = world(i);
            if q.y.abs() < half(7.0) - 0.1 && q.x.abs() > 0.1 {
                let want = if q.x < 0.0 { ROAD_MAIN } else { ROAD };
                assert!(same(*c, linear(want)), "at {q}: {c:?}");
            }
        }
        // and a main road that ends on nothing ends round, in its own colour
        let (px, world) = picture(vec![road(&[(-30.0, 0.0), (0.0, 0.0)], 7.0, true)], 0.1, Vec2::ZERO, (200, 100));
        for (i, c) in px.iter().enumerate() {
            let q = world(i);
            if q.x > 0.0 && q.length() < half(7.0) - 0.1 {
                assert!(same(*c, linear(ROAD_MAIN)), "at {q}: {c:?}");
            }
        }
    }

    #[test]
    fn the_casing_is_a_hair_near_and_half_a_pixel_far() {
        // near: a pixel and a half round a surface grown by a pixel; far: half a pixel round
        // the surface as it is, the narrowest road's 2.4 pixels in all
        let near = MapView { mpp: 0.15, ..MapView::new() };
        let far = MapView { mpp: 40.0, ..MapView::new() };
        assert_eq!(casing_edge(near.tolerance()), (CASING_EDGE_NEAR, SURFACE_GROW_NEAR));
        assert_eq!(casing_edge(far.tolerance()), (CASING_EDGE_FAR, 0.0));
        assert!((ROAD_PX + 2.0 * CASING_EDGE_FAR - 2.4).abs() < 1e-6);
        // in between, the nearer the thicker
        let mut last = (0.0, 0.0);
        for mpp in [8.0f64, 4.0, 2.0, 1.0, 0.5, 0.25] {
            let e = casing_edge(MapView { mpp, ..MapView::new() }.tolerance());
            assert!(e.0 >= last.0 && e.1 >= last.1, "{mpp}: {e:?}");
            last = e;
        }
    }

    #[test]
    fn a_sliver_between_two_surfaces_is_road() {
        // a lane merging into a street, its edge running a few centimetres off the street's
        // (as a crossing's paths and slip lanes do): no hair of casing between them, near
        let w = half(7.0);
        let gap = 0.08;
        let y = f64::from(2.0 * w + gap);
        let pieces = vec![road(&[(-30.0, 0.0), (30.0, 0.0)], 7.0, false), road(&[(-30.0, y), (30.0, y)], 7.0, false)];
        let (px, world) = picture(pieces, 0.1, Vec2::new(0.0, w), (200, 200));
        for (i, c) in px.iter().enumerate() {
            let q = world(i);
            if q.x.abs() < 20.0 && q.y > 1.0 && q.y < 2.0 * w + gap - 1.0 {
                assert!(same(*c, linear(ROAD)), "at {q}: {c:?}");
            }
        }
    }

    /// `OMSI_MAP_PICTURE=<global.cfg>;<out folder>[;x,y ...] cargo test -p omsi-app --lib
    /// picture_of_a_map -- --ignored --nocapture`: a real map's roads drawn as the launcher
    /// draws them, from the closest zoom to far out, round the given world points (else the
    /// map's roundabouts), with one sample a pixel and with four - pictures to look at.
    #[test]
    #[ignore]
    fn picture_of_a_map() {
        let Ok(v) = std::env::var("OMSI_MAP_PICTURE") else { return };
        let mut parts = v.split(';');
        let global = PathBuf::from(parts.next().unwrap_or_default());
        let out = PathBuf::from(parts.next().unwrap_or("."));
        std::fs::create_dir_all(&out).ok();
        let roads = Arc::new(read_map(&Look { map: "picture".into(), global, date: String::new(), trips: Vec::new(), entry: -1 }));
        let mut places: Vec<DVec2> = parts
            .filter_map(|p| {
                let (x, y) = p.split_once(',')?;
                Some(DVec2::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
            })
            .collect();
        if places.is_empty() {
            // (the rings are found where the traffic finds them)
            let mut net = Network { lanes: roads.net.lanes.clone(), ..Default::default() };
            net.link(1.5);
            net.compute_conflicts();
            for l in net.lanes.iter().filter(|l| l.ring) {
                let c = l.points.iter().map(|q| q.truncate()).sum::<DVec2>() / l.points.len().max(1) as f64;
                if places.iter().all(|p| (*p - c).length() > 60.0) {
                    places.push(c);
                }
            }
            places.truncate(6);
        }
        println!("{} roads in {} lines, at {places:?}", roads.roads.len(), roads.lines.len());
        let mut m = MapView::new();
        m.roads = Some(roads.clone());
        let ground = [BACKDROP.r as f32, BACKDROP.g as f32, BACKDROP.b as f32];
        for (k, at) in places.iter().enumerate() {
            for mpp in [0.15f64, 0.3, 0.6, 1.5, 4.0] {
                m.mpp = mpp;
                let t0 = Instant::now();
                let (verts, _) = m.build(&roads, m.tolerance(), palette(true));
                let built = t0.elapsed().as_secs_f64();
                let c = Vec2::new((at.x - roads.origin.x) as f32, (at.y - roads.origin.y) as f32);
                let (w, h) = (520, 380);
                for (name, s) in [("1x", &MSAA1[..]), ("4x", &MSAA4[..])] {
                    save(&rasterise(&verts, c, mpp as f32, w, h, s, ground), w, h, &out.join(format!("p{k}-{mpp}-{name}.png")));
                }
                println!("place {k} at {mpp} m a pixel: {} vertices, built in {built:.3} s", verts.len());
            }
        }
    }

    /// `OMSI_MAP_PICTURE=<global.cfg>;<out folder> cargo test -p omsi-app --lib
    /// holes_in_a_maps_junctions -- --ignored --nocapture`: a real map's junctions (three or
    /// more road ends at one place) drawn close up, counting the pixels of casing that lie
    /// inside the roads' surface - pinholes where the surfaces of a crossing's paths leave a
    /// gap between them - and drawing the ones that have any.
    #[test]
    #[ignore]
    fn holes_in_a_maps_junctions() {
        let Ok(v) = std::env::var("OMSI_MAP_PICTURE") else { return };
        let mut parts = v.split(';');
        let global = PathBuf::from(parts.next().unwrap_or_default());
        let out = PathBuf::from(parts.next().unwrap_or("."));
        std::fs::create_dir_all(&out).ok();
        let roads = Arc::new(read_map(&Look { map: "holes".into(), global, date: String::new(), trips: Vec::new(), entry: -1 }));
        // the junctions: places where three or more ends of the roads meet
        let mut ends: HashMap<(i64, i64), (usize, DVec2)> = HashMap::new();
        for r in &roads.roads {
            for q in [r.points[0], r.points[r.points.len() - 1]] {
                let e = ends.entry(((q.x / 4.0).round() as i64, (q.y / 4.0).round() as i64)).or_insert((0, q.truncate()));
                e.0 += 1;
            }
        }
        let mut places: Vec<DVec2> = Vec::new();
        let mut all: Vec<DVec2> = ends.values().filter(|(n, _)| *n >= 3).map(|(_, at)| *at).collect();
        all.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        for at in all {
            if places.iter().all(|p| (*p - at).length() > 25.0) {
                places.push(at);
            }
        }
        places.truncate(300);
        let mut m = MapView::new();
        m.roads = Some(roads.clone());
        let mpp = MPP_MIN;
        m.mpp = mpp;
        let (verts, _) = m.build(&roads, m.tolerance(), palette(true));
        let (casing, surfaces) = (linear(ROAD_CASING), [linear(ROAD), linear(ROAD_MAIN)]);
        let (w, h) = (360usize, 360usize);
        let mut worst: Vec<(usize, usize)> = Vec::new();
        let mut total = 0;
        for (k, at) in places.iter().enumerate() {
            let c = Vec2::new((at.x - roads.origin.x) as f32, (at.y - roads.origin.y) as f32);
            let px = rasterise(&verts, c, mpp as f32, w, h, &MSAA1, [0.0; 3]);
            let road = |x: i64, y: i64| x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && surfaces.iter().any(|s| same(px[y as usize * w + x as usize], *s));
            // (casing with road in all four directions within a metre)
            let reach = (1.0 / mpp) as i64;
            let mut holes = 0;
            for y in 0..h as i64 {
                for x in 0..w as i64 {
                    if same(px[y as usize * w + x as usize], casing) && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().all(|(dx, dy)| (1..=reach).any(|s| road(x + dx * s, y + dy * s))) {
                        holes += 1;
                    }
                }
            }
            total += holes;
            if holes > 0 {
                worst.push((holes, k));
                save(&px, w, h, &out.join(format!("hole-{k}.png")));
            }
        }
        worst.sort_by(|a, b| b.cmp(a));
        println!("{} junctions, {} with holes, {total} hole pixels; worst: {:?}", places.len(), worst.len(), worst.iter().take(12).map(|(n, k)| (*n, *k, places[*k])).collect::<Vec<_>>());
    }
}
