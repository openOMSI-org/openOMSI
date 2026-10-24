//! A trip's route as one line. The timetable names every lane the bus may use on a stretch of
//! road, one after another - three side-by-side lanes of the same span back to back - so
//! drawn lane by lane the route runs doubled and tripled, and a lane change shows as a step
//! sideways. The game's own map shows one lane and a smooth curve over to the next one; this
//! does the same from the lanes' points.

use glam::DVec2;

/// A lane that begins this close beside the lane before it (and not at its end) is another
/// lane of the same road, not the next piece of the route.
const SIBLING: f64 = 6.0;
/// Pieces whose ends are closer than this are one line already.
const JOINED: f64 = 0.8;
/// A hole wider than this is a hole in the route: the line is cut there, not drawn across.
const HOLE: f64 = 12.0;
/// How far along the new lane a change of lane takes (metres, at most 60 % of the lane).
const BLEND: f64 = 28.0;

fn length(p: &[DVec2]) -> f64 {
    p.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// The nearest point of `poly` to `p`: its distance and how far along the line it lies.
fn project(p: DVec2, poly: &[DVec2]) -> (f64, f64) {
    let (mut best, mut at, mut acc) = (f64::MAX, 0.0, 0.0);
    for w in poly.windows(2) {
        let ab = w[1] - w[0];
        let span = ab.length();
        let t = if span < 1e-6 { 0.0 } else { ((p - w[0]).dot(ab) / (span * span)).clamp(0.0, 1.0) };
        let d = p.distance(w[0] + ab * t);
        if d < best {
            best = d;
            at = acc + span * t;
        }
        acc += span;
    }
    (best, at)
}

/// `poly` up to `s` metres along it.
fn cut(poly: &[DVec2], s: f64) -> Vec<DVec2> {
    let mut out = vec![poly[0]];
    let mut acc = 0.0;
    for w in poly.windows(2) {
        let span = w[0].distance(w[1]);
        if acc + span >= s {
            out.push(w[0].lerp(w[1], ((s - acc) / span.max(1e-9)).clamp(0.0, 1.0)));
            return out;
        }
        acc += span;
        out.push(w[1]);
    }
    out
}

fn heading(a: DVec2, b: DVec2) -> DVec2 {
    (b - a).normalize_or_zero()
}

/// The lanes of a route, in the order the timetable lists them, as lines: one for each run
/// of the route that hangs together (a hole between two pieces starts another).
pub fn route_lines(lanes: &[Vec<DVec2>]) -> Vec<Vec<DVec2>> {
    let mut segs: Vec<Vec<DVec2>> = Vec::new();
    for lane in lanes.iter().filter(|l| l.len() >= 2) {
        if let Some(prev) = segs.last() {
            if prev[prev.len() - 1].distance(lane[0]) > JOINED {
                let (d, s) = project(lane[0], prev);
                if d <= SIBLING && s < length(prev) - 3.0 {
                    if s < 1.0 {
                        segs.pop();
                    } else {
                        let last = segs.len() - 1;
                        segs[last] = cut(&segs[last], s);
                    }
                }
            }
        }
        segs.push(lane.clone());
    }
    let mut lines: Vec<Vec<DVec2>> = Vec::new();
    let mut cur: Vec<DVec2> = Vec::new();
    for lane in segs {
        if cur.is_empty() {
            cur = lane;
            continue;
        }
        let p0 = cur[cur.len() - 1];
        let gap = p0.distance(lane[0]);
        if gap <= JOINED {
            cur.extend(lane.iter().skip(if gap < 0.3 { 1 } else { 0 }));
            continue;
        }
        if gap > HOLE {
            lines.push(std::mem::replace(&mut cur, lane));
            continue;
        }
        // a change of lane: a smooth curve from here over to the new lane, a stretch in
        let blend = (length(&lane) * 0.6).min(BLEND);
        let mut acc = 0.0;
        let mut k = lane.len() - 1;
        for i in 1..lane.len() {
            acc += lane[i - 1].distance(lane[i]);
            if acc >= blend {
                k = i;
                break;
            }
        }
        let p1 = lane[k];
        let t0 = if cur.len() > 1 { heading(cur[cur.len() - 2], p0) } else { heading(p0, p1) };
        let t1 = if k + 1 < lane.len() { heading(p1, lane[k + 1]) } else { heading(lane[k - 1], p1) };
        let l = p0.distance(p1);
        let (c0, c1) = (p0 + t0 * l / 3.0, p1 - t1 * l / 3.0);
        for j in 1..=10 {
            let (t, u) = (j as f64 / 10.0, 1.0 - j as f64 / 10.0);
            cur.push(p0 * (u * u * u) + c0 * (3.0 * u * u * t) + c1 * (3.0 * u * t * t) + p1 * (t * t * t));
        }
        cur.extend(lane.iter().skip(k + 1));
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lane(x0: f64, x1: f64, y: f64) -> Vec<DVec2> {
        vec![DVec2::new(x0, y), DVec2::new(x1, y)]
    }

    #[test]
    fn pieces_that_meet_are_one_line() {
        let l = route_lines(&[lane(0.0, 50.0, 0.0), lane(50.0, 100.0, 0.0)]);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].len(), 3);
    }

    #[test]
    fn side_by_side_lanes_of_one_span_are_drawn_once() {
        // three lanes of the same 100 m, then the road goes on from the third
        let l = route_lines(&[lane(0.0, 100.0, 0.0), lane(0.0, 100.0, 3.0), lane(0.0, 100.0, 6.0), lane(100.0, 200.0, 6.0)]);
        assert_eq!(l.len(), 1);
        assert!(l[0].iter().all(|p| p.y > 5.9), "only the last lane remains: {:?}", l[0]);
        assert!((l[0].last().unwrap().x - 200.0).abs() < 1e-9);
    }

    #[test]
    fn a_lane_beside_the_middle_of_the_one_before_changes_over_smoothly() {
        // the first lane runs 0..200; the next begins beside its middle on another lane
        let l = route_lines(&[lane(0.0, 200.0, 0.0), lane(100.0, 300.0, 3.5)]);
        assert_eq!(l.len(), 1);
        // the old lane is cut where the new one starts, and the line never steps sideways
        for w in l[0].windows(2) {
            assert!((w[1].y - w[0].y).abs() < 1.0, "step sideways at {:?}", w);
        }
        assert!((l[0].last().unwrap().x - 300.0).abs() < 1e-9);
    }

    #[test]
    fn a_real_hole_cuts_the_line() {
        let l = route_lines(&[lane(0.0, 50.0, 0.0), lane(500.0, 550.0, 0.0)]);
        assert_eq!(l.len(), 2);
    }
}
