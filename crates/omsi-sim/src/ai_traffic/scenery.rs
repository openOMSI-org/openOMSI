//! The bodies and the scenery: an AI vehicle keeps out of the map's collision boxes and
//! meshes (the player's collision world, `TrafficSim::occluders`) instead of driving
//! through them.
//!
//! Every quarter of a second a car looks along its way: the body is driven ahead along the
//! way by its own steering (`AiBody::predict` - the rate-limited wheel and the pure pursuit
//! that cuts a bend, not the way's centre line), each pose a box from 15 cm over the road
//! (the road surface and the kerbs are no walls) up to the vehicle's roof, the sections of an
//! articulated bus or a trailer each about its own coupling (`Shape::follow`, the joint
//! stopping at its `[coupling_front_character]` angle) rather than one long box that swings
//! its tail wide. The first pose that touches something is the distance to contact, and the
//! car stops short of it (`STOP_SHORT`), braking for it as for a line - before the thing,
//! not against it.
//!
//! A contact at the side (a corner of the body meeting a post, a house's corner on a tight
//! turn) is first steered round: the way is nudged within the lane by 0.35, 0.6 or 0.2 m
//! away from it (at a junction or on a turning lane up to a metre, a wide vehicle up to
//! 0.6 m past the lane's edge), each candidate driven ahead the same way and checked against
//! the scenery and the other vehicles' bodies. The nudge is a steering request, the body
//! still drives there itself.
//!
//! What the look ahead missed, the step catches: a step of the body that takes it deeper
//! into something is taken back (the body stands where it was, its wheels still turned as
//! the step turned them) and the body recovers - it steers with the look-ahead distance,
//! from half to one and a half of the usual, that frees the corner best. The planner waits
//! for the body meanwhile (`body_link`).
//!
//! The AI of Omsi.exe does not know the scenery at all; a map can lay a path through an
//! object. A car that stood `PATIENCE` seconds for one thing gives it up, and so does every
//! car after it: it drives through as Omsi.exe's would rather than block the road for good.

use super::*;
use crate::collision::CollisionWorld;
use crate::vehicle::TrailerPart;

/// Seconds between two looks along the way.
pub const LOOK_EVERY: f32 = 0.25;
/// The checked box starts this far over the road (m): kerbs, drains and the road's own
/// surface are no walls.
pub const ROAD_CLEAR: f64 = 0.15;
/// A car stops this far short of where its body would touch (m, of travel).
pub const STOP_SHORT: f32 = 0.5;
/// Seconds a car stands for one thing in the scenery (or tries to steer free of it) before
/// it gives it up and drives through, as Omsi.exe's AI would.
pub const PATIENCE: f32 = 6.0;
/// Seconds a body steers to work free after a step was taken back.
pub const RECOVER_FOR: f32 = 2.0;
/// A step that takes the body this much deeper into something (m) is taken back.
const DEEPER: f64 = 0.005;
/// Spacing of the predicted poses (m, twice that beyond 20 m).
const STEP: f32 = 0.5;
/// How far the way is nudged round something at the side, in the order tried (m).
const NUDGES: [f32; 3] = [0.35, 0.6, 0.2];
/// ... and at a junction or on a turning lane.
const JUNCTION_NUDGE: f32 = 1.0;
/// A wide vehicle may be nudged this far past its lane's edge (m).
const WIDE_PAST_EDGE: f32 = 0.6;
/// The look-ahead distances a recovering body tries, as shares of the usual.
const HORIZONS: [f32; 5] = [1.0, 0.5, 0.75, 1.25, 1.5];
/// A contact whose middle lies further out than this share of the half width touches at
/// the side (a corner), nearer the middle across the front.
const SIDE_SHARE: f64 = 0.4;
/// The scenery near a parked car's place is the parked car: it is passed as one
/// (`kerb_and_squeeze`), not as a wall.
const PARKED_NEAR: f64 = 1.5;

/// A vehicle as the checks see it: its main body's box from its origin (to the front and
/// rear bumper, half its width, m), how high its roof is over the road (m), and the
/// sections coupled behind it.
#[derive(Debug, Clone, Default)]
pub struct Shape {
    pub front: f64,
    pub rear: f64,
    pub half_width: f64,
    pub height: f64,
    pub sections: Vec<Section>,
}

/// A section coupled behind: where its front coupling is on the part in front (that part's
/// frame, x right and y forward, from its origin - the vehicle's origin, or the coupling of
/// a section), how far its turning axle is behind that coupling, its box (centre from the
/// coupling in its own frame, half extents, the box's turn against the section's heading in
/// radians, its roof over the road) and the angle its joint stops at (deg).
#[derive(Debug, Clone, Copy)]
pub struct Section {
    pub hitch: DVec2,
    pub length: f64,
    pub center: DVec2,
    pub half: DVec2,
    pub turned: f64,
    pub height: f64,
    pub max_alpha: Option<f64>,
}

/// Where a section stands: its front coupling (world, plan view) and heading (deg).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionPose {
    pub coupling: DVec2,
    pub heading: f64,
}

fn fwd(h: f64) -> DVec2 {
    let r = h.to_radians();
    DVec2::new(r.sin(), r.cos())
}

/// A vector in the frame of something heading `h` (deg; x right, y forward) in the world.
fn to_world(v: DVec2, h: f64) -> DVec2 {
    let f = fwd(h);
    DVec2::new(f.y, -f.x) * v.x + f * v.y
}

fn to_local(v: DVec2, h: f64) -> DVec2 {
    let f = fwd(h);
    DVec2::new(v.dot(DVec2::new(f.y, -f.x)), v.dot(f))
}

fn wrap(d: f64) -> f64 {
    (d + 540.0).rem_euclid(360.0) - 180.0
}

impl Shape {
    /// The vehicle `v` as it stands, with its box `extent` (origin to front bumper, to
    /// rear bumper, half width) and the road `road_z` under it, and where its sections
    /// stand now.
    pub fn of(
        v: &VehicleInstance,
        extent: (f32, f32, f32),
        road_z: f64,
    ) -> (Shape, Vec<SectionPose>) {
        let (front, rear, half_width) = extent;
        let height = match v.ty.def.bounding_box {
            Some(bb) => (v.position.z + (bb[5] + 0.5 * bb[2]) as f64 - road_z).max(0.5),
            None => 1.5,
        };
        let mut shape = Shape {
            front: front as f64,
            rear: rear as f64,
            half_width: half_width as f64,
            height,
            sections: Vec::new(),
        };
        let mut poses = Vec::new();
        let (mut lead_pos, mut lead_rot) = (v.position, v.body_rotation());
        let (mut reference, mut heading) = (v.position.truncate(), v.heading);
        for t in &v.trailers {
            let c = t.coupling_point(lead_pos, lead_rot).truncate();
            shape
                .sections
                .push(section_of(t, c, reference, heading, road_z));
            poses.push(SectionPose {
                coupling: c,
                heading: t.heading,
            });
            (lead_pos, lead_rot) = (t.position, t.body_rotation());
            (reference, heading) = (c, t.heading);
        }
        (shape, poses)
    }

    /// Carry the sections along after the main body has moved to `origin`, `heading`: each
    /// section's coupling goes where the part in front holds it, its axle is dragged after
    /// it (the section's heading is the line from its axle to its coupling), and the joint
    /// stops at its angle - as `TrailerPart` moves a real one.
    pub fn follow(&self, origin: DVec2, heading: f64, poses: &mut [SectionPose]) {
        let (mut reference, mut lead) = (origin, heading);
        for (s, p) in self.sections.iter().zip(poses.iter_mut()) {
            let c = reference + to_world(s.hitch, lead);
            let axle = p.coupling - fwd(p.heading) * s.length;
            let d = c - axle;
            let mut h = if d.length() > 1e-6 {
                d.x.atan2(d.y).to_degrees()
            } else {
                p.heading
            };
            if let Some(a) = s.max_alpha {
                let rel = wrap(lead - h);
                if rel.abs() > a {
                    h = lead - rel.clamp(-a, a);
                }
            }
            *p = SectionPose {
                coupling: c,
                heading: h,
            };
            reference = c;
            lead = h;
        }
    }

    /// The main body's box at `origin`, `heading`, on the road at `road_z`.
    pub fn main_box(&self, origin: DVec2, heading: f64, road_z: f64) -> Obb {
        let mut b = Obb::vehicle(origin, heading, self.front, self.rear, self.half_width);
        b.z0 = road_z + ROAD_CLEAR;
        b.z1 = road_z + self.height;
        b
    }

    /// Every box of the vehicle: the main body's, then each section's.
    pub fn boxes(
        &self,
        origin: DVec2,
        heading: f64,
        road_z: f64,
        poses: &[SectionPose],
    ) -> Vec<Obb> {
        let mut out = vec![self.main_box(origin, heading, road_z)];
        for (s, p) in self.sections.iter().zip(poses) {
            out.push(Obb {
                center: p.coupling + to_world(s.center, p.heading),
                half: s.half,
                heading: p.heading.to_radians() + s.turned,
                z0: road_z + ROAD_CLEAR,
                z1: road_z + s.height,
                ..Obb::point(DVec3::ZERO, 0.0)
            });
        }
        out
    }
}

/// One coupled part as a `Section`, standing with its coupling at `c` behind a part whose
/// reference point is `reference`, heading `heading`.
fn section_of(t: &TrailerPart, c: DVec2, reference: DVec2, heading: f64, road_z: f64) -> Section {
    let length = t.pivot_length().max(0.5) as f64;
    let (center, half, turned, height) = match t.ty.def.bounding_box {
        Some(bb) => {
            let b = Obb::from_box(bb, t.position, t.body_heading());
            (
                to_local(b.center - c, t.heading),
                b.half,
                b.heading - t.heading.to_radians(),
                (b.z1 - road_z).max(0.5),
            )
        }
        // (no box: a body as long as its coupling is from its axle, and a metre more)
        None => (
            DVec2::new(0.0, -0.5 * length - 0.5),
            DVec2::new(1.0, 0.5 * length + 1.0),
            0.0,
            2.5,
        ),
    };
    let max_alpha =
        t.ty.def
            .coupling_front_character
            .filter(|c| c[3] != 0.0 && c[0] > 0.0)
            .map(|c| c[0] as f64);
    Section {
        hitch: to_local(c - reference, heading),
        length,
        center,
        half,
        turned,
        height,
        max_alpha,
    }
}

/// Where a vehicle driven ahead first touches something.
#[derive(Debug, Clone, Copy)]
pub struct Hit {
    /// Distance driven to the touching pose (m; 0 = touching now).
    pub at: f32,
    pub obstacle: Obb,
    /// Which box touches (0 the main body, 1.. the sections).
    pub part: usize,
    /// Where it touches: how far to the right of the touching box's middle (m).
    pub lateral: f64,
    /// It touches at a side or a corner rather than across its front: a corner meeting a
    /// post or a house's corner, not a wall ahead.
    pub side: bool,
}

/// How box `bx` (part `part` of the vehicle) touches `o`, if it does.
fn hit_of(bx: &Obb, o: &Obb, part: usize) -> Option<Hit> {
    let c = bx.contact(o)?;
    let lateral = (c.point - bx.center).dot(bx.axes()[0]);
    Some(Hit {
        at: 0.0,
        obstacle: *o,
        part,
        lateral,
        side: lateral.abs() > SIDE_SHARE * bx.half.x,
    })
}

/// How deep box `b` is in `o` (m; None apart).
fn depth(b: &Obb, o: &Obb) -> Option<f64> {
    b.contact(o).map(|c| c.depth)
}

/// The first contact of the body driven ahead along `way` (see `AiBody::predict`) with
/// one of `obstacles`, within `distance` metres. `road_z` is the road under the body now;
/// further on the road rises and falls with the way. What the vehicle touches already where
/// it stands is not ahead of it - except, with `frontal_now`, a thing it stands against
/// with its front (a wall ahead is a stop at once, a post beside is steered away from).
/// Between two poses that touch and do not, the boxes are taken at fifths of the way
/// between, so that the distance comes out to a tenth of a metre.
#[allow(clippy::too_many_arguments)]
pub fn first_contact(
    body: &AiBody,
    shape: &Shape,
    poses: &[SectionPose],
    way: &dyn Fn(f32) -> DVec3,
    road_z: f64,
    speed: f32,
    distance: f32,
    obstacles: &[Obb],
    frontal_now: bool,
) -> Option<Hit> {
    if obstacles.is_empty() {
        return None;
    }
    let origin = body.position.truncate();
    let now = shape.boxes(origin, body.heading, road_z, poses);
    let mut touching = vec![false; obstacles.len()];
    for (k, o) in obstacles.iter().enumerate() {
        for (part, b) in now.iter().enumerate() {
            let Some(hit) = hit_of(b, o, part) else {
                continue;
            };
            // (in front of the main body's middle: ahead of it, not beside)
            if frontal_now && !hit.side && part == 0 && (o.center - b.center).dot(b.axes()[1]) > 0.0
            {
                return Some(hit);
            }
            touching[k] = true;
        }
    }
    let touches = |origin: DVec2, heading: f64, z: f64, sections: &[SectionPose]| -> Option<Hit> {
        for (part, bx) in shape.boxes(origin, heading, z, sections).iter().enumerate() {
            for (o, _) in obstacles.iter().zip(&touching).filter(|(_, t)| !**t) {
                if let Some(h) = hit_of(bx, o, part) {
                    return Some(h);
                }
            }
        }
        None
    };
    let z0 = way(0.0).z;
    let mut sections = poses.to_vec();
    let mut last = (0.0f32, origin, body.heading, sections.clone());
    let mut hit = None;
    body.predict(way, speed, STEP, distance, &mut |x, b| {
        let origin = b.position.truncate();
        shape.follow(origin, b.heading, &mut sections);
        let z = road_z + (way(x).z - z0);
        if let Some(h) = touches(origin, b.heading, z, &sections) {
            // (where between the last pose and this one)
            let (x0, o0, h0, s0) = &last;
            for k in 1..=5 {
                let u = k as f64 / 5.0;
                let lerp = |a: f64, b: f64| a + wrap(b - a) * u;
                let mid: Vec<SectionPose> = s0
                    .iter()
                    .zip(&sections)
                    .map(|(a, b)| SectionPose {
                        coupling: a.coupling.lerp(b.coupling, u),
                        heading: lerp(a.heading, b.heading),
                    })
                    .collect();
                if let Some(h) = touches(o0.lerp(origin, u), lerp(*h0, b.heading), z, &mid) {
                    hit = Some(Hit {
                        at: x0 + (x - x0) * u as f32,
                        ..h
                    });
                    return true;
                }
            }
            hit = Some(Hit { at: x, ..h });
            return true;
        }
        last = (x, origin, b.heading, sections.clone());
        false
    });
    hit
}

/// One car's watch over the scenery (see the module).
#[derive(Debug, Clone, Default)]
pub struct SceneryGuard {
    /// The scenery near its way as of the last look (world boxes), and its shape then.
    pub near: Vec<Obb>,
    pub shape: Shape,
    /// When the next look is due (sim time, s).
    pub next_look: f32,
    /// Where it would touch: its odometer at that pose, and what it would touch.
    pub contact: Option<(f32, Obb)>,
    /// Seconds it has stood for that.
    pub held: f32,
    /// Nudged round something until the odometer reads this.
    pub nudge_until: Option<f32>,
    /// Seconds of recovery left after a step was taken back, and how long it has been
    /// recovering without getting free.
    pub recover: f32,
    pub stuck: f32,
    /// A step was taken back in the last tick.
    pub rejected: bool,
    /// What it gave up keeping out of while recovering (for everybody's list).
    pub gave_up: Vec<Obb>,
}

impl SceneryGuard {
    /// Is there scenery near enough for its steps to be checked?
    pub fn watching(&self, kind: MotionKind) -> bool {
        kind == MotionKind::Road && !self.near.is_empty()
    }

    /// One step of the body of the car `state` plans for (`AiBody::step` along `way`, for
    /// `dt` seconds), watched near the scenery - a recovering body picks its look-ahead
    /// first, and a step into something is taken back - and what the planner learns of
    /// where the body got to (`body_link`).
    pub fn step(
        &mut self,
        body: &mut AiBody,
        (state, net, dt): (&AiState, &Network, f32),
        way: &dyn Fn(f32) -> DVec3,
        ground: Option<&dyn Fn(f64, f64) -> Option<f64>>,
        contact: Option<&dyn crate::rigid::Ground>,
    ) -> crate::traffic::BodyLink {
        let before = self.watching(body.kind).then(|| body.clone());
        if before.is_some() {
            self.pick_horizon(body, way, state.speed);
        }
        body.step(dt, state.speed, way, ground, contact);
        match before {
            Some(b) => self.after_step(b, body, dt),
            None => self.rejected = false,
        }
        super::body_link::link(state, net, body)
    }

    /// Before a step of a recovering body: the look-ahead distance that keeps it furthest
    /// from what it is caught on over the next two metres.
    pub fn pick_horizon(&self, body: &mut AiBody, way: &dyn Fn(f32) -> DVec3, speed: f32) {
        if self.recover <= 0.0 {
            body.look_scale = 1.0;
            return;
        }
        let z = body.position.z;
        let mut best = (f64::MIN, 1.0f32);
        for scale in HORIZONS {
            let mut copy = body.clone();
            copy.look_scale = scale;
            let mut worst = f64::MAX;
            copy.predict(way, speed.max(1.0), STEP, 2.0, &mut |_, b| {
                let me = self.shape.main_box(b.position.truncate(), b.heading, z);
                for o in &self.near {
                    if me.z1 >= o.z0 && o.z1 >= me.z0 {
                        worst = worst.min(me.separation(o));
                    }
                }
                false
            });
            // (the usual first where they are alike)
            if worst > best.0 + 0.01 {
                best = (worst, scale);
            }
        }
        body.look_scale = best.1;
    }

    /// After a step from `before` to `body`: a step that took the main body deeper into
    /// something is taken back, and the body recovers.
    pub fn after_step(&mut self, before: AiBody, body: &mut AiBody, dt: f32) {
        let old = self.shape.main_box(
            before.position.truncate(),
            before.heading,
            before.position.z,
        );
        let new = self
            .shape
            .main_box(body.position.truncate(), body.heading, body.position.z);
        let into: Vec<Obb> = self
            .near
            .iter()
            .filter(|o| depth(&new, o).is_some_and(|d| d > depth(&old, o).unwrap_or(0.0) + DEEPER))
            .copied()
            .collect();
        self.rejected = !into.is_empty();
        if self.rejected {
            body.take_back(before);
            self.recover = RECOVER_FOR;
            self.stuck += dt;
            if self.stuck > PATIENCE {
                // (it cannot get free: through it, as Omsi.exe's AI drives)
                self.near.retain(|o| !into.iter().any(|g| same(g, o)));
                self.gave_up.extend(into);
                self.stuck = 0.0;
            }
        } else {
            self.recover = (self.recover - dt).max(0.0);
            if self.recover <= 0.0 {
                self.stuck = 0.0;
                body.look_scale = 1.0;
            }
        }
    }
}

/// The same thing in the scenery: the same object's part in the same place.
fn same(a: &Obb, b: &Obb) -> bool {
    a.id == b.id && (a.center - b.center).length() < 0.05
}

impl TrafficSim {
    /// Car `i` and the scenery ahead: a look along its way when one is due, a nudge round
    /// something at the side, and where it has to stop short of the scenery (folded into
    /// `stop_at` and `why`, as `stop_points` does).
    pub(super) fn scenery_stop(
        &mut self,
        i: usize,
        ts: &TickScene,
        stop_at: Option<f32>,
        why: (&'static str, f32),
    ) -> (Option<f32>, (&'static str, f32)) {
        let Some(world) = self.occluders.clone() else {
            return (stop_at, why);
        };
        if (world.boxes.is_empty() && world.meshes.is_empty())
            || self.cars[i].body.kind != MotionKind::Road
            || self.net.lanes[self.cars[i].state.lane].kind == LaneKind::Air
        {
            return (stop_at, why);
        }
        // what a body gave up while it recovered is given up for everybody
        let gave_up = std::mem::take(&mut self.cars[i].guard.gave_up);
        for g in gave_up {
            self.give_up(i, g, "could not steer free of");
        }
        if self.time >= self.cars[i].guard.next_look {
            self.look_at_scenery(i, &world, ts);
        }
        let car = &mut self.cars[i];
        if car
            .guard
            .nudge_until
            .is_some_and(|u| car.state.odometer > u)
        {
            // past it: back to the middle of the lane, gently
            car.state.nudge_to(0.0, 12.0);
            car.guard.nudge_until = None;
        }
        let Some((odo, what)) = car.guard.contact else {
            car.guard.held = 0.0;
            return (stop_at, why);
        };
        let at = car.state.front + (odo - car.state.odometer) - STOP_SHORT;
        if stop_at.is_some_and(|s| s < at) {
            // something nearer holds it
            car.guard.held = 0.0;
            return (stop_at, why);
        }
        if car.state.speed < 0.1 {
            car.guard.held += ts.dt;
            if car.guard.held > PATIENCE {
                self.give_up(i, what, "stood for");
                return (stop_at, why);
            }
        } else {
            car.guard.held = 0.0;
        }
        (Some(at), if at < why.1 { ("scenery", at) } else { why })
    }

    /// Car `i` gives `what` up: it drives through it, and so does every car after it.
    fn give_up(&mut self, i: usize, what: Obb, how: &str) {
        let car = &mut self.cars[i];
        car.guard.contact = None;
        car.guard.held = 0.0;
        car.guard.near.retain(|o| !same(o, &what));
        if !self.scenery_through.iter().any(|o| same(o, &what)) {
            log::info!(
                "t={:.1}: car {} {how} the scenery (object {}, at ({:.1}, {:.1})) for {PATIENCE:.0} s: its way runs through it, the traffic drives through it from now on",
                self.time,
                car.id,
                what.id,
                what.center.x,
                what.center.y
            );
            self.scenery_through.push(what);
        }
    }

    /// A look along car `i`'s way (see the module): the scenery near it, where the body
    /// would first touch it, and a nudge round it where it touches at the side.
    fn look_at_scenery(&mut self, i: usize, world: &CollisionWorld, ts: &TickScene) {
        let near = self.scenery_near_way(i, world);
        let net = &self.net;
        let car = &self.cars[i];
        let st = &car.state;
        let road_z = car.body.position.z;
        let (shape, poses) = Shape::of(&car.vehicle, (st.front, st.rear, car.half_width), road_z);
        let reach = scenery_reach(st);
        let hit = first_contact(
            &car.body,
            &shape,
            &poses,
            &|d| st.way_point(net, d),
            road_z,
            st.speed,
            reach,
            &near,
            true,
        );
        // round it at the side, if a nudge within the lane clears it and everybody else -
        // or away from what a corner touches already
        let beside_now = || {
            let me = shape.main_box(car.body.position.truncate(), car.body.heading, road_z);
            near.iter()
                .find_map(|o| hit_of(&me, o, 0).filter(|h| h.side))
        };
        let side = hit
            .filter(|h| h.side)
            .or_else(|| hit.is_none().then(beside_now).flatten());
        let nudge = side.and_then(|h| self.nudge_round(i, &h, &shape, &poses, &near, ts));
        let was = self.cars[i].guard.contact.is_some();
        let car = &mut self.cars[i];
        car.guard.next_look = self.time + LOOK_EVERY;
        car.guard.near = near;
        car.guard.shape = shape;
        car.guard.contact = None;
        match (side.or(hit), nudge) {
            (Some(h), Some((to, ramp))) => {
                car.state.nudge_to(to, ramp);
                let past = car.state.odometer + h.at + (car.state.front + car.state.rear) + 3.0;
                car.guard.nudge_until = Some(car.guard.nudge_until.map_or(past, |u| u.max(past)));
            }
            (Some(h), None) if hit.is_some() => {
                car.guard.contact = Some((car.state.odometer + h.at, h.obstacle));
                if !was {
                    if let Some(s) = self.stats.as_mut() {
                        s.window.scenery_stops += 1;
                    }
                    if omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
                        log::info!(
                            "t={:.1}: car {} would touch the scenery (object {}, part {}{}) in {:.1} m: it stops short",
                            self.time,
                            car.id,
                            h.obstacle.id,
                            h.part,
                            if h.side { ", at the side" } else { "" },
                            h.at
                        );
                    }
                }
            }
            _ => {}
        }
    }

    /// The scenery along car `i`'s way within its reach and beside it by what a body can
    /// swing out, less what the traffic drives through and the parked cars (which it
    /// passes as parked cars).
    fn scenery_near_way(&self, i: usize, world: &CollisionWorld) -> Vec<Obb> {
        let car = &self.cars[i];
        let st = &car.state;
        let reach = scenery_reach(st);
        let height = car
            .vehicle
            .ty
            .def
            .bounding_box
            .map(|bb| (bb[5] + 0.5 * bb[2]) as f64)
            .unwrap_or(1.5)
            .max(1.0);
        let wide = car.half_width as f64 + 2.0 + 0.5 * car.vehicle.trailers.len() as f64;
        let parked: Vec<DVec2> = self
            .way_lanes(st, reach + 10.0)
            .into_iter()
            .chain(st.prev_lane.map(|p| (p, 0.0)))
            .flat_map(|(l, _)| {
                let lane = &self.net.lanes[l];
                self.parked
                    .get(&l)
                    .into_iter()
                    .flatten()
                    .map(move |&(s, lat)| {
                        let (q, h) = lane.at(s);
                        let hr = (h as f64).to_radians();
                        q.truncate() + DVec2::new(hr.cos(), -hr.sin()) * lat as f64
                    })
            })
            .collect();
        let mut out: Vec<Obb> = Vec::new();
        let behind = -(st.rear + 2.0 + 12.0 * car.vehicle.trailers.len() as f32);
        let mut d = behind;
        while d < reach + st.front {
            let (p, q) = (
                st.way_point(&self.net, d),
                st.way_point(&self.net, d + 10.0),
            );
            let along = (q - p).truncate();
            let chunk = Obb {
                center: (p.truncate() + q.truncate()) * 0.5,
                half: DVec2::new(wide, 0.5 * along.length() + 1.0),
                heading: along.x.atan2(along.y),
                z0: p.z.min(q.z) + ROAD_CLEAR - 0.5,
                z1: p.z.max(q.z) + height + 0.5,
                ..Obb::point(DVec3::ZERO, 0.0)
            };
            for o in world.obstacles_near(&chunk) {
                if out.iter().any(|x| same(x, &o))
                    || self.scenery_through.iter().any(|x| same(x, &o))
                    || parked
                        .iter()
                        .any(|p| (*p - o.center).length() < PARKED_NEAR)
                {
                    continue;
                }
                out.push(o);
            }
            d += 10.0;
        }
        out
    }

    /// A nudge within the lane that takes car `i` clear of what it would touch at the side
    /// (`hit`) and of everybody else: (where to, over how many metres), the first of the
    /// candidates that clears.
    fn nudge_round(
        &self,
        i: usize,
        hit: &Hit,
        shape: &Shape,
        poses: &[SectionPose],
        near: &[Obb],
        ts: &TickScene,
    ) -> Option<(f32, f32)> {
        let car = &self.cars[i];
        let st = &car.state;
        let lane = &self.net.lanes[st.lane];
        let junction = !self.net.crossings[st.lane].is_empty() || lane.turn != 0;
        let mut limit = (0.5 * lane.width - car.half_width).max(0.0);
        if car.half_width > 1.1 {
            limit += WIDE_PAST_EDGE;
        }
        if junction {
            limit = limit.max(JUNCTION_NUDGE);
        }
        // away from it: to the left of something touching on the right
        let sign = if hit.lateral > 0.0 { -1.0f32 } else { 1.0 };
        let reach = scenery_reach(st);
        let ramp = (hit.at - 0.5).clamp(3.0, 20.0);
        // the other vehicles' bodies near the way
        let pos = car.vehicle.position.truncate();
        let bodies: Vec<Obb> = ts
            .feet
            .near(pos, reach as f64 + 15.0)
            .filter(|f| f.car != i)
            .map(|f| Obb {
                center: f.center,
                half: DVec2::new(f.half_w, f.half_len),
                heading: f.fwd.x.atan2(f.fwd.y),
                z0: f.z - 1.0,
                z1: f.z + 4.0,
                ..Obb::point(DVec3::ZERO, 0.0)
            })
            .collect();
        let road_z = car.body.position.z;
        let mut candidates: Vec<f32> = NUDGES.to_vec();
        if junction {
            candidates.push(JUNCTION_NUDGE);
        }
        for m in candidates.into_iter().filter(|m| *m <= limit + 1e-3) {
            let to = st.nudge_at(st.odometer) + sign * m;
            if to.abs() > limit + 1e-3 {
                continue;
            }
            let mut alt = st.clone();
            alt.nudge_to(to, ramp);
            let way = |d: f32| alt.way_point(&self.net, d);
            let clear = first_contact(
                &car.body, shape, poses, &way, road_z, st.speed, reach, near, false,
            )
            .is_none()
                && first_contact(
                    &car.body,
                    shape,
                    poses,
                    &way,
                    road_z,
                    st.speed,
                    reach.min(hit.at + 10.0),
                    &bodies,
                    false,
                )
                .is_none();
            if clear {
                return Some((to, ramp));
            }
        }
        None
    }
}

/// How far ahead a car looks for the scenery (m): its stopping distance at its comfortable
/// braking, a second and a half more and a car's length, 10 to 60 m.
fn scenery_reach(st: &AiState) -> f32 {
    let v = st.speed.max(0.0);
    (v * v / (2.0 * st.decel.max(1.0)) + 1.5 * v + 8.0).clamp(10.0, 60.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(center: DVec2, half: DVec2) -> Obb {
        Obb {
            center,
            half,
            heading: 0.0,
            z0: -1.0,
            z1: 5.0,
            ..Obb::point(DVec3::ZERO, 0.0)
        }
    }

    /// An articulated bus: a 12 m front body, a section of 8 m on a joint 5 m behind the
    /// origin with its axle 5 m behind the joint, the joint stopping at 52.5°.
    fn artic() -> (Shape, Vec<SectionPose>) {
        let section = Section {
            hitch: DVec2::new(0.0, -5.0),
            length: 5.0,
            center: DVec2::new(0.0, -4.5),
            half: DVec2::new(1.25, 4.5),
            turned: 0.0,
            height: 3.0,
            max_alpha: Some(52.5),
        };
        let shape = Shape {
            front: 7.0,
            rear: 5.0,
            half_width: 1.25,
            height: 3.0,
            sections: vec![section],
        };
        let poses = vec![SectionPose {
            coupling: DVec2::new(0.0, -5.0),
            heading: 0.0,
        }];
        (shape, poses)
    }

    /// Round a right turn the rear section runs inside its coupling's track and its tail
    /// does not swing out: sections about their couplings, not one long box. The joint
    /// stops at its angle however the front turns.
    #[test]
    fn a_rear_section_follows_its_coupling() {
        let (shape, mut poses) = artic();
        // the front's origin drives a quarter circle of radius 12 to the right about (12, 0);
        // its coupling, 5 m behind it, runs on a circle of 13 m
        let (r, centre) = (12.0, DVec2::new(12.0, 0.0));
        let hitch_r = (r * r + 25.0f64).sqrt();
        let mut inside = f64::MIN;
        for k in 0..=200 {
            let a = std::f64::consts::FRAC_PI_2 * k as f64 / 200.0;
            let origin = DVec2::new(r - r * a.cos(), r * a.sin());
            shape.follow(origin, a.to_degrees(), &mut poses);
            assert!(((poses[0].coupling - centre).length() - hitch_r).abs() < 1e-6);
            let axle = poses[0].coupling - fwd(poses[0].heading) * 5.0;
            inside = inside.max(hitch_r - (axle - centre).length());
        }
        // (12 m steady: the axle 1 m inside its coupling's circle)
        assert!(
            inside > 0.6 && inside < 1.05,
            "the rear axle cut in by {inside:.2} m"
        );
        // the section's tail stays inside the coupling's circle; one long box from the
        // front's origin would have swung it out to 18 m
        let p = poses[0];
        let tail = p.coupling + to_world(DVec2::new(0.0, -9.0), p.heading);
        assert!(
            (tail - centre).length() < hitch_r,
            "tail {:.2} m from the centre",
            (tail - centre).length()
        );
        // jackknifed: the front turned 90° on the spot, the joint holds at 52.5°
        let (shape, mut poses) = artic();
        shape.follow(DVec2::ZERO, 90.0, &mut poses);
        assert!(
            (wrap(90.0 - poses[0].heading) - 52.5).abs() < 1e-6,
            "{:?}",
            poses[0]
        );
    }

    fn golf() -> omsi_vehicle::Vehicle {
        use omsi_vehicle::vehicle::Axle;
        let mut v = omsi_vehicle::Vehicle {
            mass: 1.0,
            rot_pnt_long: -1.3,
            inv_min_turn_radius: 0.19,
            ..Default::default()
        };
        v.axles = vec![
            Axle {
                long: 1.2,
                max_width: 1.6,
                ..Default::default()
            },
            Axle {
                long: -1.3,
                max_width: 1.6,
                ..Default::default()
            },
        ];
        v
    }

    /// The body driven ahead meets a wall across its way where its bumper gets there; a
    /// post beside the way is touched at the side by a corner; what it stands in already
    /// does not count; a post well clear is not touched at all.
    #[test]
    fn the_first_contact_along_the_way() {
        let way = |d: f32| DVec3::new(0.0, d as f64, 0.0);
        let mut body = AiBody::new(&golf(), MotionKind::Road);
        body.place(&way, None, None, 0.0);
        let shape = Shape {
            front: 2.1,
            rear: 2.2,
            half_width: 0.85,
            height: 1.5,
            sections: Vec::new(),
        };
        let hit = first_contact(
            &body,
            &shape,
            &[],
            &way,
            0.0,
            8.0,
            40.0,
            &[wall(DVec2::new(0.0, 20.0), DVec2::new(3.0, 0.2))],
            false,
        )
        .expect("the wall");
        assert!((hit.at - 17.7).abs() < 0.6 && !hit.side, "{hit:?}");
        let hit = first_contact(
            &body,
            &shape,
            &[],
            &way,
            0.0,
            8.0,
            40.0,
            &[wall(DVec2::new(1.0, 15.0), DVec2::new(0.2, 0.2))],
            false,
        )
        .expect("the post");
        assert!(hit.side, "{hit:?}");
        assert!(
            first_contact(
                &body,
                &shape,
                &[],
                &way,
                0.0,
                8.0,
                40.0,
                &[wall(DVec2::new(0.0, 1.0), DVec2::new(0.3, 0.3))],
                false
            )
            .is_none()
        );
        assert!(
            first_contact(
                &body,
                &shape,
                &[],
                &way,
                0.0,
                8.0,
                40.0,
                &[wall(DVec2::new(1.4, 15.0), DVec2::new(0.2, 0.2))],
                false
            )
            .is_none()
        );
        // a kerb-high block is driven over, an overhead sign under
        let mut low = wall(DVec2::new(0.0, 20.0), DVec2::new(3.0, 0.2));
        low.z1 = 0.1;
        let mut high = low;
        (high.z0, high.z1) = (4.5, 5.0);
        assert!(
            first_contact(
                &body,
                &shape,
                &[],
                &way,
                0.0,
                8.0,
                40.0,
                &[low, high],
                false
            )
            .is_none()
        );
    }

    /// The rear section of an articulated bus turning right cuts in: a post just inside
    /// the turn that the front body passes is touched by the section, found by its own box.
    #[test]
    fn a_rear_section_is_checked_on_its_own() {
        let r = 12.0;
        let way = move |d: f32| {
            let d = d as f64;
            if d < 0.0 {
                DVec3::new(0.0, d, 0.0)
            } else {
                let a = (d / r).min(std::f64::consts::PI);
                DVec3::new(r - r * a.cos(), r * a.sin(), 0.0)
            }
        };
        let mut def = golf();
        def.axles[0].long = 2.0;
        def.axles[1].long = -4.0;
        def.rot_pnt_long = -4.0;
        def.inv_min_turn_radius = 0.1;
        let mut body = AiBody::new(&def, MotionKind::Road);
        body.place(&way, None, None, 0.0);
        let (shape, poses) = artic();
        let front_only = Shape {
            sections: Vec::new(),
            ..shape.clone()
        };
        // a post just inside where the rear section's inner side passes once the bus has
        // come round (the section cuts in further than the front body)
        let mut probe = poses.clone();
        body.predict(&way, 4.0, 0.5, 18.0, &mut |_, p| {
            shape.follow(p.position.truncate(), p.heading, &mut probe);
            false
        });
        let s = probe[0];
        let post = wall(
            s.coupling + to_world(DVec2::new(1.25 - 0.1, -6.0), s.heading),
            DVec2::new(0.05, 0.05),
        );
        assert!(
            first_contact(
                &body,
                &front_only,
                &[],
                &way,
                0.0,
                4.0,
                18.0,
                &[post],
                false
            )
            .is_none(),
            "the front body touches the post"
        );
        let hit = first_contact(&body, &shape, &poses, &way, 0.0, 4.0, 18.0, &[post], false)
            .expect("the section touches it");
        assert_eq!(hit.part, 1, "{hit:?}");
    }
}
