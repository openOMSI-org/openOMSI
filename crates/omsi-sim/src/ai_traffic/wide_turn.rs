//! A long vehicle in a tight bend. The AI keeps a vehicle on the middle of a lane drawn for
//! cars, and a bus cannot stay on it: the front swings out over the centre line and the rear
//! section cuts in over the kerb. Each step such a vehicle picks its line across the lane for
//! the bend ahead from the room it has: with the oncoming side clear it swings out and keeps
//! off the kerb; with something coming it keeps to the kerb side, slowly and with its hazard
//! lights on while a tyre has to ride the kerb, and waits for the oncoming vehicle to pass
//! if even that would take it into the oncoming lane. Where it is over the centre line the
//! oncoming traffic is held back, as it is for a car that is passing.

use super::*;
use crate::traffic::wrap_deg;

/// How far (m) the rear axle cuts inside the line it is steered along.
const CUT: f32 = 0.3;
/// A bend is looked for this far ahead of the vehicle's origin (m), and the line is taken
/// up this far before it.
const LOOK: f32 = 30.0;
const APPROACH: f32 = 14.0;
/// Curvature (1/m) under which a stretch is no bend.
const MIN_K: f32 = 1.0 / 60.0;
/// How far a line may go over the lane's edge on the kerb side (m), and how far out the
/// other way it is tried.
const KERB_LIMIT: f32 = 0.8;
const MAX_OUT: f32 = 2.0;
/// Speed (m/s) riding the kerb, going over the centre line, and waiting for the oncoming side.
const CRAWL: f32 = 2.2;
const SWING: f32 = 3.0;
const WAIT: f32 = 0.3;
/// Seconds it waits for the oncoming side before it goes on, slowly.
const MAX_WAIT: f32 = 20.0;

/// What the swept path of a vehicle depends on (m, from its definition).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Swept {
    /// The rear axle to the front axle, and the front axle to the front of the body.
    pub wheelbase: f32,
    pub front: f32,
    /// The rear axle to the joint behind it, and the joint to the last axle (both 0 without
    /// a rear section).
    pub hitch: f32,
    pub tail: f32,
    /// Half the width of the body, and of the rear axles out to the tyres.
    pub body: f32,
    pub rear: f32,
    /// How far behind its origin the vehicle reaches (m).
    pub back: f32,
}

/// A line across the lane: how far it lies towards the kerb (m), and how far the body goes
/// over the lane's edge on the kerb side and on the centre line's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    pub x: f32,
    pub kerb: f32,
    pub white: f32,
}

impl Swept {
    /// The dimensions of a vehicle that needs more than a lane for a tight bend: a bus or
    /// a vehicle with a rear section. None for a car and for rail vehicles.
    pub fn of(v: &VehicleInstance) -> Option<Self> {
        let def = &v.ty.def;
        let bb = def.bounding_box?;
        if def.axles.len() < 2 || def.is_rail() || (bb[1] < 9.0 && v.trailers.is_empty()) {
            return None;
        }
        let (rot, wheelbase) = crate::ai_motion::rotation_point(def);
        let front_axle = rot + wheelbase;
        let rear_axle = def.axles.iter().min_by(|a, b| a.long.total_cmp(&b.long))?;
        let (hitch, tail, rear) = match v.trailers.first() {
            Some(t) => {
                let joint = def.coupling_back.as_ref()?.pos[1];
                let td = &t.ty.def;
                let last = td.axles.iter().min_by(|a, b| a.long.total_cmp(&b.long))?;
                let hold = td.coupling_front.as_ref()?.pos[1];
                (
                    (rot - joint).max(0.0),
                    (hold - last.long).max(1.0),
                    rear_axle.max_width.max(last.max_width) * 0.5,
                )
            }
            None => (0.0, 0.0, rear_axle.max_width * 0.5),
        };
        let back = (hitch + tail).max(-rot + bb[1] * 0.5 - bb[4]) + 2.0;
        Some(Self {
            wheelbase,
            front: (bb[4] + bb[1] * 0.5 - front_axle).max(0.0),
            hitch,
            tail,
            body: bb[0] * 0.5,
            rear,
            back,
        })
    }

    /// How far (m) the vehicle goes over the edge of a lane `half_lane` wide on the kerb side
    /// and on the other, in a bend of radius `r` along the middle of the lane, when its rear
    /// axle follows a line `x` metres towards the kerb of it. `kerb_inner`: the kerb is on
    /// the inside of the bend.
    pub fn overshoot(&self, r: f32, x: f32, half_lane: f32, kerb_inner: bool) -> (f32, f32) {
        let line = (if kerb_inner { r - x } else { r + x }).max(2.0);
        let rear = (line - CUT).max(1.0);
        // (the last axle of a vehicle with a rear section runs on a tighter circle: the steady
        // state of a joint behind the axle)
        let last = if self.tail > 0.0 {
            (rear * rear + self.hitch * self.hitch - self.tail * self.tail)
                .max(1.0)
                .sqrt()
                .min(rear)
        } else {
            rear
        };
        let inner = ((r - half_lane) - (last - self.rear)).max(0.0);
        let outer = ((((rear + self.body).powi(2) + (self.wheelbase + self.front).powi(2)).sqrt())
            - (r + half_lane))
            .max(0.0);
        if kerb_inner {
            (inner, outer)
        } else {
            (outer, inner)
        }
    }

    /// The line the vehicle takes through a bend of radius `r`. With nothing coming
    /// (`blocked` false) it keeps off the kerb and goes over the centre line as far as that
    /// needs; blocked, it keeps over the centre line as little as it can, at most
    /// `KERB_LIMIT` over the kerb's edge.
    pub fn choose(&self, r: f32, half_lane: f32, kerb_inner: bool, blocked: bool) -> Line {
        let mut best: Option<(f32, Line)> = None;
        let mut fallback: Option<(f32, Line)> = None;
        let mut x = -MAX_OUT;
        while x <= KERB_LIMIT + 1.0 {
            let (kerb, white) = self.overshoot(r, x, half_lane, kerb_inner);
            let line = Line { x, kerb, white };
            let cost = if blocked {
                white * 20.0 + kerb * 2.0
            } else {
                kerb * 10.0 + white * 2.0
            } + 0.05 * x.abs();
            if kerb <= KERB_LIMIT && best.map_or(true, |(c, _)| cost < c) {
                best = Some((cost, line));
            }
            if fallback.map_or(true, |(c, _)| kerb < c) {
                fallback = Some((kerb, line));
            }
            x += 0.1;
        }
        best.or(fallback).map(|b| b.1).unwrap_or(Line {
            x: 0.0,
            kerb: 0.0,
            white: 0.0,
        })
    }
}

/// What a long vehicle does about the bend ahead (`TrafficSim::wide_turn`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WideTurn {
    pub active: bool,
    /// Its lateral target (m, + right).
    pub offset: f32,
    /// How far its body goes over the centre line's side of the lane (m).
    pub white: f32,
    /// Seconds it has waited for the oncoming side.
    pub waited: f32,
    /// The lane and place of the bend's tightest point (the oncoming side is held back there).
    pub lane: usize,
    pub at: f32,
    /// The vehicle's dimensions, once looked up (`None` inside: not a long vehicle).
    pub swept: Option<Option<Swept>>,
}

/// The bend ahead: its radius, which way it turns (+1 right, -1 left), the distances from
/// the vehicle's origin to where it starts and ends, and the lane and place of its tightest
/// point.
#[derive(Debug, Clone, Copy)]
struct Bend {
    radius: f32,
    turn: f32,
    start: f32,
    end: f32,
    lane: usize,
    at: f32,
}

impl TrafficSim {
    /// The first bend within `LOOK` metres of car `i`'s way, from just behind its origin (its
    /// tail may still be in one).
    fn bend_ahead(&self, i: usize, back: f32) -> Option<Bend> {
        let car = &self.cars[i];
        let from = -back;
        let mut bend: Option<Bend> = None;
        let mut last_in = f32::MIN;
        for (l, d0) in self.way_lanes(&car.state, LOOK + 20.0) {
            let lane = &self.net.lanes[l];
            let mut u = (from - d0).max(0.0);
            while u <= lane.length() {
                let dist = d0 + u;
                if dist > LOOK + 20.0 {
                    break;
                }
                let k = lane.curvature_at(u);
                if k.abs() > MIN_K {
                    let same = bend
                        .as_ref()
                        .map(|b| (k > 0.0) == (b.turn > 0.0) && dist - last_in <= 12.0);
                    match same {
                        Some(true) => {
                            if let Some(b) = bend.as_mut() {
                                b.end = dist;
                                if k.abs() > 1.0 / b.radius {
                                    b.radius = 1.0 / k.abs();
                                    b.lane = l;
                                    b.at = u;
                                }
                            }
                        }
                        Some(false) => return bend,
                        None if dist <= LOOK => {
                            bend = Some(Bend {
                                radius: 1.0 / k.abs(),
                                turn: k.signum(),
                                start: dist,
                                end: dist,
                                lane: l,
                                at: u,
                            });
                        }
                        None => {}
                    }
                    last_in = dist;
                } else if bend.is_some() && dist - last_in > 12.0 {
                    return bend;
                }
                u += 1.5;
            }
        }
        bend
    }

    /// Something on the oncoming side that the swing of car `i` through `b` would meet: a
    /// vehicle on the oncoming lane there, or on its way to it before the swing is over, or
    /// the player's or another player's vehicle coming.
    fn oncoming_near(&self, i: usize, ts: &TickScene, b: &Bend, back: f32) -> bool {
        let lane = &self.net.lanes[b.lane];
        let (mid, heading) = lane.at(b.at);
        let half_span = (b.end - b.start) * 0.5 + back + 8.0;
        let secs = (2.0 * half_span + 8.0) / CRAWL;
        if let Some((ol, os, _)) = self.net.opposite(b.lane, b.at) {
            for &(j, sj, _, _) in ts.by_lane.get(&ol).map(|v| v.as_slice()).unwrap_or(&[]) {
                if j == i || j >= self.cars.len() {
                    continue;
                }
                let speed = self.cars[j].state.speed.max(0.0);
                if sj <= os + half_span + 4.0 && sj + speed * secs >= os - half_span {
                    return true;
                }
            }
        }
        ts.player
            .iter()
            .chain(ts.others.iter().map(|(_, p)| p))
            .any(|&(centre, h, half_len, _, speed)| {
                let away = (centre - mid).truncate().length() as f32;
                wrap_deg(h as f32 - heading).abs() > 100.0
                    && away < half_len + half_span + speed.abs() * secs + 10.0
            })
    }

    /// Car `i`'s line through the bend ahead, if it is a long vehicle that needs one:
    /// its lateral target, its speed, its hazard lights and the room it asks of the
    /// oncoming traffic (see the module's notes).
    pub(super) fn wide_turn(&mut self, i: usize, ts: &TickScene, kerb_swerve: Option<f32>) {
        if !self.wide_turns {
            return;
        }
        if self.cars[i].wide.swept.is_none() {
            let swept = Swept::of(&self.cars[i].vehicle);
            self.cars[i].wide.swept = Some(swept);
        }
        let Some(Some(swept)) = self.cars[i].wide.swept else {
            return;
        };
        let car = &self.cars[i];
        let st = &car.state;
        let own = car.passing.is_some()
            || car.park.is_some()
            || st.change.is_some()
            || car.pull_out > 0.0
            || kerb_swerve.is_some();
        let near_stop = car.next_stop().is_some_and(|(ri, ss)| {
            ri >= st.route_index && st.route_distance(&self.net, ri, ss) < 40.0
        });
        let bend = if own || near_stop {
            None
        } else {
            self.bend_ahead(i, swept.back)
        };
        let mut plan: Option<(Bend, Line, bool, bool)> = None;
        if let Some(b) = bend.filter(|b| b.start <= APPROACH && b.end >= -swept.back) {
            let half = (self.net.lanes[b.lane].width * 0.5).max(1.2);
            let kerb_inner = (b.turn > 0.0) != self.net.left_hand;
            let near = self.oncoming_near(i, ts, &b, swept.back);
            // (after `MAX_WAIT` it stops giving way and goes on, as long as the other is near)
            let blocked = near && car.wide.waited <= MAX_WAIT;
            let line = swept.choose(b.radius, half, kerb_inner, blocked);
            let waiting = near && line.white > 0.05 && b.start > 0.0;
            if line.x.abs() > 0.05 || line.kerb > 0.05 || line.white > 0.05 {
                plan = Some((b, line, blocked, waiting));
            }
        }
        let sign = -self.net.oncoming_sign();
        let dt = ts.dt;
        let car = &mut self.cars[i];
        match plan {
            Some((b, line, _, waiting)) => {
                let riding = line.kerb > 0.1;
                let waited = if waiting { car.wide.waited + dt } else { 0.0 };
                car.wide = WideTurn {
                    active: true,
                    offset: line.x * sign,
                    white: line.white,
                    waited,
                    lane: b.lane,
                    at: b.at,
                    swept: car.wide.swept,
                };
                car.state.lateral_target = car.wide.offset;
                car.state.wide_swing = true;
                let holding = waiting && waited <= MAX_WAIT;
                car.state.hazard = riding || holding;
                car.state.wide_cap = if holding {
                    Some(WAIT)
                } else if riding {
                    Some(CRAWL)
                } else if line.white > 0.3 {
                    Some(SWING)
                } else {
                    None
                };
            }
            None => {
                car.wide = WideTurn {
                    swept: car.wide.swept,
                    ..Default::default()
                };
                car.state.wide_swing = false;
                car.state.hazard = false;
                car.state.wide_cap = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The MAN GN92 articulated bus (Vehicles\MAN_NL_NG).
    fn gn92() -> Swept {
        Swept {
            wheelbase: 5.94,
            front: 2.44,
            hitch: 1.631,
            tail: 4.556,
            body: 1.236,
            rear: 1.2,
            back: 8.2,
        }
    }

    #[test]
    fn a_gentle_bend_asks_nothing_of_the_lane() {
        let (kerb, white) = gn92().overshoot(60.0, 0.0, 1.75, true);
        assert!(kerb < 0.1 && white < 0.1, "{kerb} {white}");
    }

    #[test]
    fn a_tight_bend_cuts_the_kerb_on_the_middle_line() {
        let (kerb, _) = gn92().overshoot(12.0, 0.0, 1.75, true);
        assert!(kerb > 0.3, "{kerb}");
        // (the other way round it is the outer side that goes over the kerb)
        let (kerb_outer, _) = gn92().overshoot(12.0, 0.0, 1.75, false);
        assert!(kerb_outer > 0.3, "{kerb_outer}");
    }

    #[test]
    fn with_the_road_clear_it_swings_out_and_keeps_off_the_kerb() {
        let line = gn92().choose(12.0, 1.75, true, false);
        assert!(line.x < -0.3, "{line:?}");
        assert!(line.kerb < 0.1, "{line:?}");
        assert!(line.white > 0.0, "{line:?}");
    }

    #[test]
    fn with_something_coming_it_keeps_to_the_kerb_side() {
        let free = gn92().choose(12.0, 1.75, true, false);
        let held = gn92().choose(12.0, 1.75, true, true);
        assert!(held.x > free.x + 0.3, "{free:?} {held:?}");
        assert!(held.white < free.white, "{free:?} {held:?}");
        assert!(held.kerb <= KERB_LIMIT, "{held:?}");
    }

    #[test]
    fn the_kerb_side_is_mirrored_for_a_bend_the_other_way() {
        let right = gn92().choose(12.0, 1.75, true, false);
        let left = gn92().choose(12.0, 1.75, false, false);
        // (a left bend on a right-hand road: the kerb is on the outside of it)
        assert!(left.x != right.x || left.kerb != right.kerb);
    }
}
