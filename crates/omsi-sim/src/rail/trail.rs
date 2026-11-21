//! The track a rail vehicle's train lies on: where its origin has been, by the distance
//! travelled, with where on the rails each place is. The coupled parts are placed on it
//! (`VehicleInstance::retrail`), and a vehicle backing follows it back instead of choosing
//! its way at the switches again (it took another branch from its own cars, and the train
//! came apart at the points). Backing, the track beyond the train's rear end is found anew,
//! the way the switches are set, as the front end does going forward.
//!
//! How to step along the rails (and throw the points on the way) is the caller's: the
//! functions here take it as a closure.

use glam::DVec3;
use std::collections::VecDeque;

/// A place on the rails: the lane, the distance along it, and whether the vehicle faces the
/// lane's direction.
pub type Spot = (usize, f32, bool);

/// The track behind (and, backing, a little ahead of) the vehicle's origin.
#[derive(Debug, Clone, Default)]
pub struct Trail {
    /// (distance travelled, the origin's position), oldest first.
    points: VecDeque<(f64, DVec3)>,
    /// Where on the rails each of `points` is.
    spots: VecDeque<(f64, Spot)>,
    /// How far behind the origin the train's last part was asked for last frame (m): the
    /// track beyond that is found anew when the train backs.
    pub reach_back: f64,
}

impl Trail {
    /// A trail of the places given, oldest first: (distance travelled, position, spot).
    pub fn laid(places: impl IntoIterator<Item = (f64, DVec3, Spot)>, reach_back: f64) -> Trail {
        let (points, spots) = places.into_iter().map(|(u, p, s)| ((u, p), (u, s))).unzip();
        Trail {
            points,
            spots,
            reach_back,
        }
    }

    /// The positions, oldest first.
    pub fn points(&self) -> &VecDeque<(f64, DVec3)> {
        &self.points
    }

    /// The spots on the rails, oldest first.
    pub fn spots(&self) -> impl Iterator<Item = &(f64, Spot)> {
        self.spots.iter()
    }

    /// Whether the trail reaches travelled distance `u`, the vehicle being at `now`.
    pub fn covers(&self, u: f64, now: f64) -> bool {
        self.points.front().is_some_and(|f| f.0 <= u)
            && u <= self.points.back().map_or(now, |b| b.0.max(now))
    }

    /// The trail's point at travelled distance `u` (see [`point_at`]).
    pub fn point_at(&self, head: Option<(f64, DVec3)>, u: f64) -> Option<DVec3> {
        point_at(&self.points, head, u)
    }

    /// Where on the trail the track is at `u` and which way it runs there (degrees, as the
    /// vehicle faces), from the trail's points 0.4 m either side; the heading None where
    /// those lie on top of each other. None where the trail does not reach.
    pub fn heading_at(&self, head: (f64, DVec3), u: f64) -> Option<(DVec3, Option<f64>)> {
        if !self.covers(u, head.0) {
            return None;
        }
        let p = self.point_at(Some(head), u)?;
        let near = |x: f64| {
            self.point_at(Some(head), if self.covers(x, head.0) { x } else { u })
                .unwrap_or(p)
        };
        let d = near(u + 0.4) - near(u - 0.4);
        Some((
            p,
            (d.truncate().length() > 0.05).then(|| d.x.atan2(d.y).to_degrees().rem_euclid(360.0)),
        ))
    }

    /// Where the vehicle's origin is on the rails at travelled distance `u` of the trail:
    /// between two of its places (along their lane, or from the nearer one), or by a step on
    /// from where the vehicle is `now`. `look(spot, ds)` steps along the rails without
    /// touching the switches. None where the trail does not reach back so far.
    pub fn spot_at(
        &self,
        u: f64,
        now: (f64, Spot),
        look: impl Fn(Spot, f32) -> Spot,
    ) -> Option<Spot> {
        let i = self.spots.partition_point(|e| e.0 <= u).checked_sub(1)?;
        let (ua, a) = self.spots[i];
        let Some(&(ub, b)) = self.spots.get(i + 1) else {
            return Some(look(now.1, (u - now.0) as f32));
        };
        let t = ((u - ua) / (ub - ua).max(1e-6)).clamp(0.0, 1.0) as f32;
        if a.0 == b.0 {
            return Some((a.0, a.1 + (b.1 - a.1) * t, a.2));
        }
        // (the two lie on two lanes: stepped on from the first, kept to those two lanes)
        let probe = look(a, (u - ua) as f32);
        Some(if probe.0 == a.0 || probe.0 == b.0 {
            probe
        } else if t < 0.5 {
            a
        } else {
            b
        })
    }

    /// Going forward to `now` at `pos` (`spot`): what lay ahead of it from backing is gone
    /// (the switches decide the way again), the newest place is added where the vehicle has
    /// moved on 10 cm, and the places more than `keep` metres behind are dropped.
    pub fn record(&mut self, now: f64, pos: DVec3, spot: Spot, keep: f64) {
        while self.points.back().is_some_and(|b| b.0 > now) {
            self.pop_back();
        }
        if self.points.back().is_none_or(|b| (now - b.0).abs() > 0.1) {
            self.points.push_back((now, pos));
            self.spots.push_back((now, spot));
        }
        while self.points.front().is_some_and(|f| now - f.0 > keep) {
            self.points.pop_front();
            self.spots.pop_front();
        }
    }

    /// Backing: the places beyond `limit` (ahead of the origin, past its front bogie and the
    /// parts hung ahead of it) are dropped.
    pub fn drop_ahead(&mut self, limit: f64) {
        while self.points.back().is_some_and(|b| b.0 > limit) {
            self.pop_back();
        }
    }

    fn pop_back(&mut self) {
        self.points.pop_back();
        self.spots.pop_back();
    }

    /// Backing from `now` to `target` (< `now`): what was recorded beyond the train's rear
    /// end is the way it came and is dropped (the switches, perhaps set otherwise since,
    /// decide it again), and the track beyond the rear end at `target` is found anew by
    /// `step(spot, ds)` - a step along the rails that reads and throws the switches, giving
    /// the new spot and its position, None at the end of the track. False at the end of the
    /// track.
    pub fn back_up(
        &mut self,
        now: f64,
        target: f64,
        mut step: impl FnMut(Spot, f32) -> Option<(Spot, DVec3)>,
    ) -> bool {
        let keep = now - self.reach_back - 1.5;
        while self.spots.len() > 2 && self.spots.get(1).is_some_and(|e| e.0 < keep) {
            self.points.pop_front();
            self.spots.pop_front();
        }
        let need = target - self.reach_back - 1.0;
        let mut guard = 0;
        while let Some(&(fu, spot)) = self.spots.front().filter(|f| f.0 > need) {
            guard += 1;
            if guard > 400 {
                break;
            }
            let ds = (fu - need).clamp(0.02, 0.5);
            let Some((next, pos)) = step(spot, -(ds as f32)) else {
                return false;
            };
            self.points.push_front((fu - ds, pos));
            self.spots.push_front((fu - ds, next));
        }
        true
    }
}

/// The trail's point at travelled distance `u` (between its samples; None beyond its ends).
/// `head` is where the vehicle is right now, `(u, position)`: the trail is only sampled every
/// 10 cm of travel, so its newest sample lags behind the vehicle by up to that much (at a
/// walking pace most of the time), and a coupled part asked for a place just behind the vehicle
/// got the stale sample while one asked for a place just ahead got the exact one - the cars
/// jumped back and forth by a few centimetres as they crossed from one answer to the other.
pub fn point_at(
    trail: &VecDeque<(f64, DVec3)>,
    head: Option<(f64, DVec3)>,
    u: f64,
) -> Option<DVec3> {
    if trail.is_empty() {
        return head.map(|h| h.1);
    }
    let newest = *trail.back().unwrap();
    if let Some((hu, hp)) = head.filter(|h| h.0 > newest.0 + 1e-9 && u > newest.0) {
        let t = ((u - newest.0) / (hu - newest.0)).clamp(0.0, 1.0);
        return Some(newest.1 + (hp - newest.1) * t);
    }
    if u <= trail.front().unwrap().0 {
        return Some(trail.front().unwrap().1);
    }
    if u >= newest.0 {
        return Some(newest.1);
    }
    let i = trail.partition_point(|(v, _)| *v < u);
    if i == 0 {
        return Some(trail[0].1);
    }
    let (a, b) = (trail[i - 1], trail[i]);
    let t = ((u - a.0) / (b.0 - a.0).max(1e-6)).clamp(0.0, 1.0);
    Some(a.1 + (b.1 - a.1) * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_at_interpolates_properly() {
        let mut trail = VecDeque::new();
        trail.push_back((0.0, DVec3::new(0.0, 0.0, 0.0)));
        trail.push_back((10.0, DVec3::new(10.0, 0.0, 0.0)));
        trail.push_back((20.0, DVec3::new(20.0, 10.0, 5.0)));

        let p0 = point_at(&trail, None, 0.0).unwrap();
        assert!((p0 - DVec3::new(0.0, 0.0, 0.0)).length() < 1e-4);

        let p5 = point_at(&trail, None, 5.0).unwrap();
        assert!((p5 - DVec3::new(5.0, 0.0, 0.0)).length() < 1e-4);

        let p15 = point_at(&trail, None, 15.0).unwrap();
        assert!((p15 - DVec3::new(15.0, 5.0, 2.5)).length() < 1e-4);
    }

    /// The trail is sampled every 10 cm; a place just behind the vehicle is the same distance
    /// from where it is whichever way it is asked: a stale newest sample made it jump.
    #[test]
    fn the_newest_place_follows_the_vehicle_not_the_last_sample() {
        let mut trail = VecDeque::new();
        trail.push_back((0.0, DVec3::new(0.0, 0.0, 0.0)));
        trail.push_back((10.0, DVec3::new(10.0, 0.0, 0.0)));
        // 7 cm further on, not yet sampled
        let head = Some((10.07, DVec3::new(10.07, 0.0, 0.0)));
        let behind = point_at(&trail, head, 10.07 - 0.02).unwrap();
        assert!((behind.x - 10.05).abs() < 1e-6, "{behind:?}");
        assert!((point_at(&trail, head, 10.07).unwrap().x - 10.07).abs() < 1e-6);
        // the samples older than the newest are unchanged
        assert!((point_at(&trail, head, 5.0).unwrap().x - 5.0).abs() < 1e-6);
        // without a head the newest sample is the end
        assert_eq!(point_at(&trail, None, 10.05).unwrap().x, 10.0);
    }

    /// Going forward the trail keeps the places behind, newest last, and forgets what lay
    /// ahead from backing; backing it drops what was recorded beyond the rear end and finds
    /// the track there anew by the step it is given.
    #[test]
    fn the_trail_follows_forward_and_is_found_anew_beyond_the_rear_end_backing() {
        // a straight lane 0 along y, the vehicle's place s = travelled distance + 50
        let at = |u: f64| DVec3::new(0.0, 50.0 + u, 0.0);
        let mut t = Trail::laid(
            (0..=20).map(|k| {
                (
                    k as f64 - 20.0,
                    at(k as f64 - 20.0),
                    (0, (30 + k) as f32, true),
                )
            }),
            5.0,
        );
        for k in 1..=10 {
            let u = k as f64 * 0.5;
            t.record(u, at(u), (0, 50.0 + u as f32, true), 15.0);
        }
        assert_eq!(t.points().back().unwrap().0, 5.0);
        assert!(
            t.points().front().unwrap().0 >= -10.0,
            "kept 15 m: {:?}",
            t.points().front()
        );
        assert!(t.covers(-3.0, 5.0) && !t.covers(6.0, 5.0));
        let look = |s: Spot, ds: f32| (s.0, s.1 + ds, s.2);
        assert_eq!(
            t.spot_at(2.25, (5.0, (0, 55.0, true)), look),
            Some((0, 52.25, true))
        );
        // backing 2 m: the rear end (5 m behind) is found anew 1 m further back, by the steps
        // given - here onto lane 1
        let mut steps = 0;
        assert!(t.back_up(5.0, 3.0, |s, ds| {
            steps += 1;
            let n = (1, s.1 + ds, s.2);
            Some((n, DVec3::new(1.0, n.1 as f64, 0.0)))
        }));
        assert!(steps > 0);
        assert!(
            t.points().front().unwrap().0 <= 3.0 - 5.0 - 1.0 + 1e-9,
            "{:?}",
            t.points().front()
        );
        assert_eq!(
            t.spots().next().unwrap().1.0,
            1,
            "the new track beyond the rear end"
        );
        // at the end of the track it stops
        assert!(!t.back_up(3.0, 0.0, |_, _| None));
        // forward again: what lay ahead is gone
        t.drop_ahead(4.0);
        assert!(t.points().back().unwrap().0 <= 4.0);
        let heading = t.heading_at((4.0, at(4.0)), 0.0).unwrap();
        assert_eq!(
            heading.1.map(|h| h.round()),
            Some(0.0),
            "the trail runs north: {heading:?}"
        );
    }
}
