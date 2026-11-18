//! The points (switches) ahead of a place on the rails, as a tram driver sets them from
//! the cab.

use super::{LaneKind, Network, wrap_deg};

impl Network {
    /// The first points within `reach` metres from `s` on rail lane `lane`, going along the
    /// lane (`lane_fwd`) or against it: how far, and the rail branches there with how each
    /// turns over its first 15 m (degrees, positive right), sorted left to right. None where
    /// the track ends or none lies within reach. The way is kept through the lanes after: on
    /// from a lane's end into the next one's start, back from its start into the previous
    /// one's end.
    pub fn points_ahead(
        &self,
        mut lane: usize,
        mut s: f32,
        lane_fwd: bool,
        reach: f32,
    ) -> Option<(f32, Vec<(usize, f64)>)> {
        let mut walked = 0.0f32;
        for _ in 0..64 {
            let l = self.lanes.get(lane)?;
            walked += if lane_fwd { l.length() - s } else { s };
            if walked > reach {
                return None;
            }
            let next: &[usize] = if lane_fwd {
                &l.next
            } else {
                self.prev.get(lane).map_or(&[], |p| p.as_slice())
            };
            let rails: Vec<usize> = next
                .iter()
                .copied()
                .filter(|&i| self.lanes.get(i).is_some_and(|x| x.kind == LaneKind::Rail))
                .collect();
            if rails.len() > 1 {
                let h0 = if lane_fwd {
                    l.at(l.length()).1
                } else {
                    l.at(0.0).1
                };
                let mut turns: Vec<(usize, f64)> = rails
                    .iter()
                    .map(|&i| {
                        let b = &self.lanes[i];
                        let h1 = if lane_fwd {
                            b.at(15.0f32.min(b.length())).1
                        } else {
                            b.at((b.length() - 15.0).max(0.0)).1
                        };
                        (
                            i,
                            wrap_deg(h1 - h0) as f64 * if lane_fwd { 1.0 } else { -1.0 },
                        )
                    })
                    .collect();
                turns.sort_by(|a, b| a.1.total_cmp(&b.1));
                return Some((walked, turns));
            }
            let &[n] = rails.as_slice() else { return None };
            lane = n;
            s = if lane_fwd {
                0.0
            } else {
                self.lanes[n].length()
            };
        }
        None
    }
}

/// The branch points are set to next, among `branches` (as [`Network::points_ahead`] gives
/// them) with the one set now (`set`, by its index): the one after it, left to right; points
/// not set to any branch go to the first one that is not the straightest. Its index, and
/// which way it goes from the straightest: "straight on", "to the left", "to the right".
pub fn next_branch(branches: &[(usize, f64)], set: Option<usize>) -> (usize, &'static str) {
    let straightest = branches
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.1.abs().total_cmp(&b.1.1.abs()))
        .map_or(0, |(k, _)| k);
    let next = (set.unwrap_or(straightest) + 1) % branches.len().max(1);
    let way = match branches.get(next).zip(branches.get(straightest)) {
        _ if next == straightest => "straight on",
        Some((n, s)) if n.1 < s.1 => "to the left",
        _ => "to the right",
    };
    (next, way)
}

#[cfg(test)]
mod tests {
    use crate::traffic::{LaneBuilder, LaneKind, Network};
    use glam::DVec3;

    /// Points 30 m ahead: found within 50 m, the branches left to right; beyond reach none.
    #[test]
    fn the_points_ahead_are_found_with_their_branches_left_to_right() {
        let rail = |pts: Vec<DVec3>| LaneBuilder::polyline(pts, LaneKind::Rail, 1.435);
        let mut lanes = vec![
            rail(vec![DVec3::new(0.0, -40.0, 0.0), DVec3::new(0.0, 0.0, 0.0)]),
            rail(vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(-5.0, 20.0, 0.0),
                DVec3::new(-30.0, 60.0, 0.0),
            ]),
            rail(vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(5.0, 20.0, 0.0),
                DVec3::new(30.0, 60.0, 0.0),
            ]),
        ];
        lanes[0].next = vec![2, 1];
        let net = Network {
            lanes,
            prev: vec![vec![], vec![0], vec![0]],
            ..Default::default()
        };
        let (dist, branches) = net.points_ahead(0, 10.0, true, 50.0).expect("the points");
        assert!((dist - 30.0).abs() < 1e-3, "{dist}");
        assert_eq!(
            branches.iter().map(|b| b.0).collect::<Vec<_>>(),
            vec![1, 2],
            "left first: {branches:?}"
        );
        assert!(branches[0].1 < 0.0 && branches[1].1 > 0.0, "{branches:?}");
        // (from the branch back towards them, they are no points: one way on)
        assert!(net.points_ahead(1, 30.0, false, 50.0).is_none());
        // 30 m away: beyond a reach of 20 m
        assert!(net.points_ahead(0, 10.0, true, 20.0).is_none());
        // set to neither branch: the first that is not the straightest; then on round
        let three = [(7, -20.0), (8, 1.0), (9, 25.0)];
        assert_eq!(super::next_branch(&three, None), (2, "to the right"));
        assert_eq!(super::next_branch(&three, Some(2)), (0, "to the left"));
        assert_eq!(super::next_branch(&three, Some(0)), (1, "straight on"));
    }
}
