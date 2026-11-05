//! The people's paths as ground for somebody walking by hand: wherever the map gives the
//! passengers a way (a staircase, a ramp, a gate through a fence, a platform under a roof),
//! the player on foot can go too, at the height the path gives.

use super::*;

/// A place on a people's path under a walker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathFooting {
    /// The path's height there (m).
    pub z: f64,
    /// The path's lane and the distance along it.
    pub lane: usize,
    pub s: f32,
    /// How far (m, plan view) the walker is from the path's line.
    pub off: f64,
}

/// How far beyond half the path's width (m) a walker still counts as on it.
const MARGIN: f64 = 0.45;
/// The narrowest a path counts as (m): the map draws some as lines.
const MIN_HALF_WIDTH: f64 = 0.5;

impl Network {
    /// The people's path under a walker at `at` whose feet are at `feet`: the nearest
    /// pedestrian lane within its width (and `MARGIN`) in plan view whose height there is no
    /// more than `reach` from the feet - the height tells a staircase from the street
    /// under a bridge, not a step from a stair. None where no path leads.
    pub fn footing_on_path(&self, at: DVec2, feet: f64, reach: f64) -> Option<PathFooting> {
        let (cx, cy) = Self::grid_cell(at.extend(0.0));
        let mut best: Option<PathFooting> = None;
        let mut seen: Vec<usize> = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(list) = self.grid.get(&(cx + dx, cy + dy)) else { continue };
                for &i in list {
                    if seen.contains(&i) {
                        continue;
                    }
                    seen.push(i);
                    let l = &self.lanes[i];
                    if l.kind != LaneKind::Sidewalk {
                        continue;
                    }
                    let reachable = (l.width as f64 * 0.5).max(MIN_HALF_WIDTH) + MARGIN;
                    for k in 0..l.points.len().saturating_sub(1) {
                        let (a, b) = (l.points[k], l.points[k + 1]);
                        let ab = (b - a).truncate();
                        let t = ((at - a.truncate()).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
                        let off = (a.truncate() + ab * t - at).length();
                        if off > reachable {
                            continue;
                        }
                        let z = a.z + (b.z - a.z) * t;
                        if (z - feet).abs() > reach {
                            continue;
                        }
                        // (the nearer in plan view wins, the nearer in height on a tie: a
                        // staircase crossing its own landing)
                        let better = best.is_none_or(|q| off + (z - feet).abs() * 0.5 < q.off + (q.z - feet).abs() * 0.5);
                        if better {
                            best = Some(PathFooting { z, lane: i, s: l.dist[k] + (l.dist[k + 1] - l.dist[k]) * t as f32, off });
                        }
                    }
                }
            }
        }
        best
    }

    /// Whether a people's path (at about height `z`, within `reach`) passes through the
    /// plan-view rectangle round `center` (`half` either side of the axes `right` and `fwd`,
    /// unit vectors) within `near_r` of `near`: a gate, a doorway, a hole in a fence or a
    /// shelter the map sends the passengers through, where the walker is. A path that
    /// crosses the same box elsewhere (a long fence's gate at its far end) does not count.
    #[allow(clippy::too_many_arguments)]
    pub fn path_through_box(&self, center: DVec2, half: DVec2, right: DVec2, fwd: DVec2, z: f64, reach: f64, near: DVec2, near_r: f64) -> bool {
        let radius = half.length() + 1.0;
        let lo = (center - DVec2::splat(radius)).max(near - DVec2::splat(near_r));
        let hi = (center + DVec2::splat(radius)).min(near + DVec2::splat(near_r));
        if lo.x > hi.x || lo.y > hi.y {
            return false;
        }
        let (c0, c1) = (Self::grid_cell(lo.extend(0.0)), Self::grid_cell(hi.extend(0.0)));
        let inside = |p: DVec2| {
            let d = p - center;
            d.dot(right).abs() <= half.x && d.dot(fwd).abs() <= half.y
        };
        let mut seen: Vec<usize> = Vec::new();
        for gx in c0.0..=c1.0 {
            for gy in c0.1..=c1.1 {
                let Some(list) = self.grid.get(&(gx, gy)) else { continue };
                for &i in list {
                    if seen.contains(&i) {
                        continue;
                    }
                    seen.push(i);
                    let l = &self.lanes[i];
                    if l.kind != LaneKind::Sidewalk {
                        continue;
                    }
                    for w in l.points.windows(2) {
                        let (a, b) = (w[0].truncate(), w[1].truncate());
                        let t = ((near - a).dot(b - a) / (b - a).length_squared().max(1e-12)).clamp(0.0, 1.0);
                        if (a + (b - a) * t - near).length() > near_r {
                            continue;
                        }
                        // (sampled along the segment: a path may cross a thin wall between two points)
                        let n = ((w[1] - w[0]).truncate().length() / 0.25).ceil().max(1.0) as usize;
                        for s in 0..=n {
                            let q = w[0].lerp(w[1], s as f64 / n as f64);
                            if (q.z - z).abs() <= reach && inside(q.truncate()) && (q.truncate() - near).length() <= near_r {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net_with(points: Vec<DVec3>) -> Network {
        let lane = LaneBuilder::polyline(points, LaneKind::Sidewalk, 1.0);
        let mut net = Network { lanes: vec![lane], ..Default::default() };
        net.build_grid();
        net
    }

    #[test]
    fn a_staircase_gives_its_own_height() {
        let net = net_with(vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 10.0, 3.0)]);
        let f = net.footing_on_path(DVec2::new(0.1, 5.0), 1.4, 1.2).unwrap();
        assert!((f.z - 1.5).abs() < 1e-6);
        // off the stairs, or under them at street level: no path
        assert!(net.footing_on_path(DVec2::new(3.0, 5.0), 1.4, 1.2).is_none());
        assert!(net.footing_on_path(DVec2::new(0.0, 5.0), -3.0, 1.2).is_none());
    }

    #[test]
    fn a_path_through_a_wall_marks_a_gate() {
        let net = net_with(vec![DVec3::new(-5.0, 0.0, 0.0), DVec3::new(5.0, 0.0, 0.0)]);
        let (right, fwd) = (DVec2::X, DVec2::Y);
        let fence = (DVec2::ZERO, DVec2::new(4.0, 0.1));
        // a fence along x through which the path goes at the origin
        assert!(net.path_through_box(fence.0, fence.1, right, fwd, 0.0, 1.0, DVec2::new(0.5, 0.0), 1.5));
        // the same fence 3 m off the path
        assert!(!net.path_through_box(DVec2::new(0.0, 3.0), fence.1, right, fwd, 0.0, 1.0, DVec2::new(0.5, 3.0), 1.5));
    }

    /// A long fence with a gate at one end and a sidewalk along its length: the gate opens, the
    /// rest of the fence blocks, and a path at another height does not open the gate.
    #[test]
    fn only_the_gate_of_a_long_fence_opens() {
        let (right, fwd) = (DVec2::X, DVec2::Y);
        // fence 60 m along x, centred on the origin; its gate at x = -28 (the path crosses it
        // going north); a sidewalk along the fence 2 m south of it
        let mut net = net_with(vec![DVec3::new(-28.0, -6.0, 0.0), DVec3::new(-28.0, 6.0, 0.0)]);
        let walk = LaneBuilder::polyline(vec![DVec3::new(-30.0, -2.0, 0.0), DVec3::new(30.0, -2.0, 0.0)], LaneKind::Sidewalk, 1.0);
        net.lanes.push(walk);
        // (a path over the fence's gate at another height: a bridge)
        net.lanes.push(LaneBuilder::polyline(vec![DVec3::new(10.0, -6.0, 5.0), DVec3::new(10.0, 6.0, 5.0)], LaneKind::Sidewalk, 1.0));
        net.build_grid();
        let (c, h) = (DVec2::ZERO, DVec2::new(30.0, 0.1));
        let open = |at: DVec2| net.path_through_box(c, h, right, fwd, 0.0, 1.0, at, 1.5);
        assert!(open(DVec2::new(-28.0, 0.3)), "at the gate");
        assert!(!open(DVec2::new(20.0, -0.4)), "the fence beside the sidewalk, far from the gate");
        assert!(!open(DVec2::new(10.0, 0.3)), "under the bridge");
    }
}
