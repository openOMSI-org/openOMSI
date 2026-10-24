//! Driving a rail vehicle (a tram, a train): the player's vehicle is bound to the track.
//! Its own physics give its speed along the rails; where it stands and which way it faces
//! come from the rail lanes of the map's paths, as the AI trains' do. At a fork it takes
//! the branch its indicator points to (Berlin's trams set their points so), else the
//! branch the switch is set to, else the straightest - and throws the points it runs over
//! (`World::set_switches`), so they move with it.
//!
//! A vehicle is rail-bound when its file says so: `[rail_body_osc]`, a `[contact_shoe]` or
//! `[boogies]`.

use crate::player::Player;
use crate::scene::World;
use glam::DVec3;
use omsi_sim::traffic::{LaneKind, Network};

/// Where on the track the vehicle is.
#[derive(Debug, Clone)]
pub(crate) struct RailDrive {
    pub lane: usize,
    pub s: f32,
    /// The vehicle faces the lane's direction (else it drives it backwards).
    pub along: bool,
    /// The track the vehicle has come along: (distance travelled, the origin's position),
    /// oldest first - where its coupled parts are placed (see `VehicleInstance::retrail`).
    trail: std::collections::VecDeque<(f64, DVec3)>,
    /// Distance travelled along the track (forward positive).
    u: f64,
    /// The body on its springs (see `omsi_sim::rail::Body`) and the heading of the last frame.
    body: omsi_sim::rail::Body,
    last_heading: Option<f64>,
}

/// How much trail is kept behind the vehicle (m): a long train's length.
const TRAIL: f64 = 400.0;

/// Whether the vehicle the arguments name is bound to rails (it then needs the rail
/// network the traffic builds, with or without cars).
pub(crate) fn args_rail(args: &crate::Args) -> bool {
    args.bus
        .as_deref()
        .map(|b| omsi_cfg::resolve_path(&args.root, b))
        .and_then(|p| omsi_vehicle::Vehicle::load(&p).ok())
        .is_some_and(|d| is_rail(&d))
}

/// Whether the vehicle `def` is bound to rails.
pub(crate) fn is_rail(def: &omsi_vehicle::Vehicle) -> bool {
    def.is_rail()
}

/// Beyond this the vehicle was not put on the track beside it: `attach` had to reach for a
/// lane far away, which puts a tram on a line its entry point is nowhere near. Kept
/// generous, because a map's entry point is a stop for every vehicle and a rail vehicle
/// often gets a road one.
const NEAR_TRACK: f64 = 100.0;

/// Put the vehicle on the rail lane nearest it (within `reach` m) that runs the way the
/// vehicle faces: the two tracks of a tram street are both `RailKind::Rail` and a metre or
/// two apart, so the nearest one is as often the one going the opposite way, and the whole
/// consist is then laid out along it facing back the way it came.
pub(crate) fn attach(p: &mut Player, net: &Network, reach: f64) -> Option<RailDrive> {
    let pos = search_point(p.vehicle.position, p.rail_placed_z);
    let heading = p.vehicle.heading;
    let (lane, s, dist) = track_for(net, pos, heading, reach)?;
    let (_, h) = net.lanes[lane].at(s);
    let diff = angle_diff(h as f64, heading);
    if dist > NEAR_TRACK {
        log::warn!(
            "rail: {} stands on rail lane {lane} {dist:.0} m from its entry point ({:.0}, {:.0}) - the map's entry point is not on a tram line",
            p.vehicle.ty.def.path.display(),
            pos.x,
            pos.y
        );
    }
    let mut r = RailDrive { lane, s, along: diff.abs() <= 90.0, trail: Default::default(), u: 0.0, body: omsi_sim::rail::Body::new(lane as u64 + 7), last_heading: None };
    // the track behind it, for its coupled parts: walked backwards from where it stands
    let mut back = r.clone();
    let mut points = Vec::new();
    let step_dist = 0.5;
    let steps = (TRAIL / step_dist).round() as i32;
    for k in 1..=steps {
        back.step(net, None, -step_dist as f32, 0);
        let (pos, _) = net.lanes[back.lane].at(back.s);
        points.push((-step_dist * k as f64, pos));
    }
    let (here, _) = net.lanes[lane].at(s);
    r.trail = points.into_iter().rev().chain(std::iter::once((0.0, here))).collect();
    log::info!("rail: {} stands on rail lane {lane} at {s:.1} m ({dist:.1} m away) at ({:.1}, {:.1})", p.vehicle.ty.def.path.display(), net.lanes[lane].at(s).0.x, net.lanes[lane].at(s).0.y);
    Some(r)
}

fn search_point(at: DVec3, placed_z: f64) -> DVec3 {
    DVec3::new(at.x, at.y, placed_z)
}

pub(crate) fn placed(p: &mut Player, z: f64) {
    p.rail_placed_z = z;
    p.rail = None;
}

/// The rail lane to put a vehicle at `pos` on: the nearest one within `reach` that runs
/// within 60° of `heading` - a tram's own track, not its neighbour going the other way - and
/// failing that (a map that runs its trams the other way round, a stub of track) simply the
/// nearest one, so a vehicle is never left standing off the rails.
fn track_for(net: &Network, pos: DVec3, heading: f64, reach: f64) -> Option<(usize, f32, f64)> {
    let facing = nearest_rail(net, pos, heading, reach, Some(60.0));
    facing.or_else(|| nearest_rail(net, pos, heading, reach, None))
}

/// The nearest rail lane of the whole network within `reach` of `p` (the grid only looks
/// nearby, and a rail vehicle may have to be lifted onto the line from across the map),
/// taking only those running within `within` degrees of `heading` where that is given.
fn nearest_rail(net: &Network, p: DVec3, heading: f64, reach: f64, within: Option<f64>) -> Option<(usize, f32, f64)> {
    let mut best: Option<(usize, f32, f64)> = None;
    for (i, l) in net.lanes.iter().enumerate().filter(|(_, l)| l.kind == LaneKind::Rail) {
        let Some((s, d)) = l.nearest_point(p) else { continue };
        if d > reach || best.is_some_and(|b| d >= b.2) {
            continue;
        }
        if let Some(within) = within {
            let turn = angle_diff(l.at(s).1 as f64, heading);
            if turn.abs() > within {
                continue;
            }
        }
        best = Some((i, s, d));
    }
    best
}

/// The trail's point at travelled distance `u` (between its samples; None beyond its ends).
/// `head` is where the vehicle is right now, `(u, position)`: the trail is only sampled every
/// 10 cm of travel, so its newest sample lags behind the vehicle by up to that much (at a
/// walking pace most of the time), and a coupled part asked for a place just behind the vehicle
/// got the stale sample while one asked for a place just ahead got the exact one - the cars
/// jumped back and forth by a few centimetres as they crossed from one answer to the other.
pub(crate) fn point_at(trail: &std::collections::VecDeque<(f64, DVec3)>, head: Option<(f64, DVec3)>, u: f64) -> Option<DVec3> {
    let mut at = |u: f64| -> Option<DVec3> {
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
        let i = trail.iter().position(|(v, _)| *v >= u)?;
        if i == 0 {
            return Some(trail[0].1);
        }
        let (a, b) = (trail[i - 1], trail[i]);
        let t = ((u - a.0) / (b.0 - a.0).max(1e-6)).clamp(0.0, 1.0);
        Some(a.1 + (b.1 - a.1) * t)
    };
    at(u)
}

fn angle_diff(a: f64, b: f64) -> f64 {
    (a - b + 540.0).rem_euclid(360.0) - 180.0
}

/// The height of the contact wire over the rails (m): the standard one of the vehicle's class,
/// kept within what its pantograph reaches (a wire it cannot touch would strand it on a map
/// that draws none).
fn wire_height(class: Option<omsi_sim::rail::RailClass>, shape: &omsi_trolley::PantoShape) -> f32 {
    use omsi_sim::rail::RailClass;
    let standard = match class {
        Some(RailClass::Train) => 5.6,
        _ => 5.7,
    };
    let (lo, hi) = (shape.height_at(0.15), shape.height_at(0.92));
    if (lo..=hi).contains(&standard) { standard } else { shape.height_at(0.6) }
}

/// How rough the wire is for the head: more in the wet, and in frost with ice on it.
fn wire_roughness(wet: f32, temperature: f32) -> f32 {
    let cold = temperature < 0.0 && wet > 0.05;
    1.3 + 0.5 * wet.clamp(0.0, 2.0) + if cold { 1.5 } else { 0.0 }
}

impl RailDrive {
    /// A copy to look ahead along the track with: the position only, not the trail kept for the
    /// coupled parts (about 100 KB).
    fn probe(&self) -> RailDrive {
        RailDrive { lane: self.lane, s: self.s, along: self.along, trail: Default::default(), u: self.u, body: omsi_sim::rail::Body::default(), last_heading: None }
    }

    /// The contact wire above the track at the pantograph: at the standard height of the
    /// vehicle's class, kept within what its pantograph reaches (a wire it cannot touch would
    /// strand it on a map that draws none), staggered across the track as a real wire is so
    /// that the strip wears evenly, hard where the track forks or crosses, rougher in the wet.
    pub(crate) fn catenary(&self, net: &Network, v: &omsi_sim::VehicleInstance) -> Option<omsi_sim::vehicle::Catenary> {
        let g = v.pantograph()?;
        let shape = g.shape()?;
        use omsi_sim::rail::RailClass;
        let height = wire_height(RailClass::of(&v.ty.def), shape);
        // the stagger: a zigzag of 20 cm either side, 60 m to the swing
        let lateral = 0.2 * (self.u * std::f64::consts::TAU / 60.0).sin() as f32;
        // the pantograph's head is ahead of or behind the origin: look where it is on the lane
        let mut probe = self.probe();
        probe.step(net, None, shape.head[1], 0);
        let lane = &net.lanes[probe.lane];
        let to_end = (lane.length() - probe.s).min(probe.s);
        let junction = (probe.s < lane.length() * 0.5 && net.prev.get(probe.lane).is_some_and(|p| p.len() > 1))
            || (probe.s >= lane.length() * 0.5 && lane.next.len() > 1);
        let hard = if junction { (1.0 - to_end / 4.0).clamp(0.0, 1.0) } else { 0.0 };
        let roughness = wire_roughness(v.host.street_cond, v.host.temperature);
        Some(omsi_sim::vehicle::Catenary { height, lateral, live: true, hard, roughness })
    }

    /// Move `ds` metres (forward along the vehicle, negative backwards) and put the
    /// vehicle there. `blinker`: 1 left, 2 right (the branch at the next fork).
    pub(crate) fn advance(&mut self, p: &mut Player, net: &Network, world: &World, scenery: &mut omsi_sim::scenery::SceneryState, dt: f32, ds: f32, blinker: u8) {
        if !self.step(net, Some((world, &mut *scenery)), ds, blinker) {
            p.vehicle.set_speed(0.0);
        }
        // (`[boogies]` is how far each bogie stands from the origin, not the span between
        // them: the A3's axles at ±2.835 and ±4.735 m are its 3.785. Halved, the bogies ran
        // under the middle of the car and its ends swung off the track in every bend.)
        let bogie_dist = p.vehicle.ty.def.boogies.map(|b| b.abs()).filter(|&b| b > 0.5)
            .or_else(|| {
                let axles = &p.vehicle.ty.def.axles;
                if axles.len() >= 2 {
                    let front = axles.iter().map(|a| a.long).fold(f32::MIN, f32::max);
                    let rear = axles.iter().map(|a| a.long).fold(f32::MAX, f32::min);
                    let span = (front - rear).abs() * 0.5;
                    if span > 0.5 { Some(span) } else { None }
                } else {
                    None
                }
            });

        let center_y = if p.vehicle.ty.def.is_rail() {
            if p.vehicle.ty.def.rot_pnt_long != 0.0 {
                p.vehicle.ty.def.rot_pnt_long
            } else if p.vehicle.ty.def.axles.len() >= 4 {
                // If 4 axles (2 bogies), center_y is the midpoint between front and rear bogie centers
                let mut longs: Vec<f32> = p.vehicle.ty.def.axles.iter().map(|a| a.long).collect();
                longs.sort_by(|a, b| b.partial_cmp(a).unwrap());
                let b0 = (longs[0] + longs[1]) * 0.5;
                let b1 = (longs[longs.len() - 2] + longs[longs.len() - 1]) * 0.5;
                (b0 + b1) * 0.5
            } else {
                0.0
            }
        } else {
            0.0
        };

        let (pos, heading, pitch) = if let Some(l) = bogie_dist {
            let mut front_probe = self.clone();
            front_probe.step(net, Some((world, &mut *scenery)), center_y + l, blinker);
            let (pos_f, h_f) = net.lanes[front_probe.lane].at(front_probe.s);

            let mut rear_probe = self.clone();
            rear_probe.step(net, Some((world, &mut *scenery)), center_y - l, blinker);
            let (pos_r, h_r) = net.lanes[rear_probe.lane].at(rear_probe.s);

            let diff = pos_f - pos_r;
            let run = diff.truncate().length();
            let (h, p_deg) = if run > 0.05 {
                let h = diff.x.atan2(diff.y).to_degrees().rem_euclid(360.0);
                let p = (diff.z / run).atan().to_degrees() as f32;
                (h, p)
            } else {
                let (_, h) = net.lanes[self.lane].at(self.s);
                let heading = if self.along { h as f64 } else { (h as f64 + 180.0).rem_euclid(360.0) };
                (heading, 0.0)
            };

            let h_track_f = if front_probe.along { h_f as f64 } else { (h_f as f64 + 180.0).rem_euclid(360.0) };
            let h_track_r = if rear_probe.along { h_r as f64 } else { (h_r as f64 + 180.0).rem_euclid(360.0) };
            let bogie_0_deg = angle_diff(h_track_f, h) as f32;
            let bogie_1_deg = angle_diff(h_track_r, h) as f32;
            let bogie_0_rad = bogie_0_deg.to_radians();
            let bogie_1_rad = bogie_1_deg.to_radians();

            let v = &mut p.vehicle;
            v.set_var("boogie_0_rot", bogie_0_deg);
            v.set_var("boogie_1_rot", bogie_1_deg);
            v.set_var("rot_boogie_0", bogie_0_deg);
            v.set_var("rot_boogie_1", bogie_1_deg);
            v.set_var("boogie_0_rot_rad", bogie_0_rad);
            v.set_var("boogie_1_rot_rad", bogie_1_rad);
            v.set_var("Axle_Steering_0_L", bogie_0_rad);
            v.set_var("Axle_Steering_0_R", bogie_0_rad);
            v.set_var("Axle_Steering_1_L", bogie_1_rad);
            v.set_var("Axle_Steering_1_R", bogie_1_rad);

            let inv_r_f = bogie_0_rad / l.max(0.1);
            let inv_r_r = bogie_1_rad / l.max(0.1);
            v.set_var("boogie_0_invradius", inv_r_f);
            v.set_var("boogie_1_invradius", inv_r_r);

            let bogie_midpoint = (pos_f + pos_r) * 0.5;
            let car_pos = if center_y.abs() > 0.001 {
                let h_rad = h.to_radians();
                bogie_midpoint - DVec3::new(h_rad.sin(), h_rad.cos(), 0.0) * center_y as f64
            } else {
                bogie_midpoint
            };
            (car_pos, h, p_deg)
        } else {
            let (pos, h) = net.lanes[self.lane].at(self.s);
            let heading = if self.along { h as f64 } else { (h as f64 + 180.0).rem_euclid(360.0) };
            (pos, heading, 0.0)
        };

        let track_pos = net.lanes[self.lane].at(self.s).0;
        let v = &mut p.vehicle;
        v.position = pos;
        v.heading = heading;
        v.pitch = pitch;
        v.bank = 0.0;
        // the body rides on its springs: it leans out of bends, dips under braking and feels the
        // rails' unevenness, instead of being nailed to the line between the bogies
        if let (Some(class), true) = (omsi_sim::rail::RailClass::of(&v.ty.def), dt > 0.0 && omsi_sim::rail::realistic()) {
            let span = bogie_dist.map_or(8.0, |l| l * 2.0);
            let yaw = self.last_heading.map_or(0.0, |h| angle_diff(heading, h).to_radians() / dt as f64) as f32;
            let a_long = v.rail_outcome().map_or(0.0, |o| o.accel);
            let params = omsi_sim::rail::BodyParams::of(class, span);
            let out = self.body.step(&params, dt, v.physics.speed, a_long, yaw.clamp(-1.0, 1.0));
            v.pitch += out.pitch_deg;
            v.bank = out.roll_deg;
            v.position.z += out.heave as f64;
            v.set_engine_var("rail_body_roll", out.roll_deg);
            v.set_engine_var("rail_body_pitch", out.pitch_deg);
            v.set_engine_var("rail_body_heave", out.heave);
            v.set_engine_var("rail_accel_side", out.a_side);
            v.set_engine_var("rail_accel_vert", out.a_vert);
        }
        self.last_heading = Some(heading);
        // the track steers: the wheel stays straight
        let mut c = v.physics.controls;
        c.steering = 0.0;
        v.set_controls(c);
        // the trail, and the coupled parts on it: store the rail centerline position at u,
        // so that trailing cars and bogies are placed on the track and not chord-inset
        self.u += ds as f64;
        if self.trail.back().map(|b| (self.u - b.0).abs() > 0.1).unwrap_or(true) {
            // (backing up takes the trail back with it)
            while self.trail.back().is_some_and(|b| b.0 > self.u) {
                self.trail.pop_back();
            }
            self.trail.push_back((self.u, track_pos));
            while self.trail.front().is_some_and(|f| self.u - f.0 > TRAIL) {
                self.trail.pop_front();
            }
        }
        if !v.trailers.is_empty() {
            let trail = &self.trail;
            let u = self.u;
            let r_probe = self.clone();
            let probe = |d: f64| {
                let mut probe = r_probe.clone();
                probe.step(net, None, -d as f32, 0);
                Some(net.lanes[probe.lane].at(probe.s).0)
            };
            // (a part may run ahead of the vehicle's origin - a bogie frame under its front
            // end - where the trail, which holds on to its newest point, has nothing yet)
            let head = Some((u, track_pos));
            v.retrail(dt, &|d| if d < 0.0 { probe(d) } else { point_at(trail, head, u - d).or_else(|| probe(d)) });
        }
    }

    /// Move along the rails by `ds` (the vehicle's forward); false at the end of the track.
    /// `world`: the switches (read and thrown); None looks without touching them.
    fn step(&mut self, net: &Network, mut world: Option<(&World, &mut omsi_sim::scenery::SceneryState)>, ds: f32, blinker: u8) -> bool {
        // along the lane: the vehicle's forward is the lane's if it faces it
        let mut d = if self.along { ds } else { -ds };
        let mut guard = 0;
        while guard < 16 {
            guard += 1;
            let len = net.lanes[self.lane].length();
            let t = self.s + d;
            if t > len {
                let facing = self.along;
                match self.pick(net, world.as_mut().map(|(w, s)| (*w, &mut **s)), &net.lanes[self.lane].next, true, blinker, facing) {
                    Some(n) => {
                        d = t - len;
                        self.lane = n;
                        self.s = 0.0;
                        continue;
                    }
                    None => {
                        // the end of the track: the vehicle stops at its buffer
                        self.s = len;
                        return false;
                    }
                }
            } else if t < 0.0 {
                let prev = net.prev.get(self.lane).cloned().unwrap_or_default();
                match self.pick(net, world.as_mut().map(|(w, s)| (*w, &mut **s)), &prev, false, blinker, !self.along) {
                    Some(n) => {
                        d = t;
                        self.lane = n;
                        self.s = net.lanes[n].length();
                        continue;
                    }
                    None => {
                        self.s = 0.0;
                        return false;
                    }
                }
            } else {
                self.s = t;
                break;
            }
        }
        true
    }

    /// What the track under the vehicle is like for its dynamics: the grade in the direction
    /// the vehicle faces, the radius of the curve and whether it is in a tunnel.
    pub(crate) fn env(&self, net: &Network) -> omsi_sim::vehicle::RailEnv {
        let lane = &net.lanes[self.lane];
        let len = lane.length();
        // the rise over 10 m of this lane (the two ends of a lane are joined by `step`, so a
        // short lane just looks at what it has)
        let (s0, s1) = ((self.s - 5.0).max(0.0), (self.s + 5.0).min(len));
        let (p0, p1) = (lane.at(s0).0, lane.at(s1).0);
        let run = (p1.truncate() - p0.truncate()).length();
        let along_slope = if run > 0.5 { ((p1.z - p0.z) / run) as f32 } else { 0.0 };
        let grade = if self.along { along_slope } else { -along_slope };
        let k = lane.curvature_at(self.s).abs();
        let name = lane.name.to_ascii_lowercase();
        omsi_sim::vehicle::RailEnv {
            grade: grade.clamp(-0.2, 0.2),
            radius: if k > 1e-4 { 1.0 / k } else { f32::INFINITY },
            in_tunnel: lane.track.is_some_and(|t| t.tunnel) || ["tunnel", "tunel", "subway", "ubahn", "u-bhf", "u_bhf", "metro"].iter().any(|t| name.contains(t)),
        }
    }

    /// The next (or previous) rail lane among `candidates`: the indicator's branch, else
    /// the one the switch is set to, else the straightest. The points it takes are thrown.
    fn pick(&self, net: &Network, mut world: Option<(&World, &mut omsi_sim::scenery::SceneryState)>, candidates: &[usize], forward: bool, blinker: u8, _facing: bool) -> Option<usize> {
        let here = &net.lanes[self.lane];
        let h0 = if forward { here.at(here.length()).1 } else { here.at(0.0).1 } as f64;
        let mut rails: Vec<(usize, f64)> = candidates
            .iter()
            .copied()
            .filter(|&i| net.lanes.get(i).map(|l| l.kind == LaneKind::Rail).unwrap_or(false))
            .map(|i| {
                let l = &net.lanes[i];
                // how the branch turns over its first 15 m (positive: right)
                let h1 = if forward { l.at(15.0f32.min(l.length())).1 } else { l.at((l.length() - 15.0).max(0.0)).1 } as f64;
                let turn = angle_diff(h1, h0) * if forward { 1.0 } else { -1.0 };
                (i, turn)
            })
            .collect();
        if rails.len() <= 1 {
            return rails.first().map(|r| r.0);
        }
        rails.sort_by(|a, b| a.1.total_cmp(&b.1));
        let chosen = match blinker {
            1 => rails.first().map(|r| r.0),
            2 => rails.last().map(|r| r.0),
            _ => rails
                .iter()
                .find(|(i, _)| net.lanes[*i].key.and_then(|k| world.as_mut().and_then(|(w, s)| w.switch_set_to(s, k.id, k.path))) == Some(true))
                .or_else(|| rails.iter().min_by(|a, b| a.1.abs().total_cmp(&b.1.abs())))
                .map(|r| r.0),
        }?;
        if let (Some(k), Some((w, s))) = (net.lanes[chosen].key, world) {
            w.set_switches(s, &[(k.id, k.path)]);
        }
        Some(chosen)
    }
}

/// The indicator the driver has set (1 left, 2 right, else 0), from the script's switch or
/// its lamps.
fn blinker_of(v: &omsi_sim::VehicleInstance) -> u8 {
    let on = |n: &str| v.var(n).map(|x| x > 0.5).unwrap_or(false);
    match v.var("lights_sw_blinker") {
        Some(s) if (0.5..2.5).contains(&s) => s.round() as u8,
        _ => match (on("lights_blinker_l"), on("lights_blinker_r")) {
            (true, false) => 1,
            (false, true) => 2,
            _ => 0,
        },
    }
}

/// One frame of a rail-bound player vehicle after its own update: onto the track (first),
/// then along it by the distance its speed covered.
pub(crate) fn frame(p: &mut Player, net: Option<&Network>, world: &World, scenery: &mut omsi_sim::scenery::SceneryState, dt: f32) {
    if !p.rail_bound {
        return;
    }
    let Some(net) = net else { return };
    if p.rail.is_none() {
        if let Some(mut r) = attach(p, net, 3000.0) {
            r.advance(p, net, world, scenery, dt, 0.0, 0);
            p.rail = Some(r);
        }
        return;
    }
    // The track's grade and curve go to the vehicle's own dynamics (`omsi_sim::rail`): its
    // drive and brakes - the scripts', or a class's typical ones for a vehicle scripted for the
    // AI only - against the resistance of the rails, the weight on the slope and the adhesion
    // of wheel on rail. The speed they make is the distance covered below.
    if let Some(r) = p.rail.as_ref() {
        let env = r.env(net);
        p.vehicle.set_rail_env(Some(env));
        if omsi_sim::rail::realistic() {
            let wire = r.catenary(net, &p.vehicle);
            p.vehicle.set_catenary(wire);
        }
    }
    let ds = p.vehicle.physics.speed * dt;
    let blinker = blinker_of(&p.vehicle);
    if let Some(mut r) = p.rail.take() {
        r.advance(p, net, world, scenery, dt, ds, blinker);
        p.rail = Some(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(top: f32) -> omsi_trolley::PantoShape {
        omsi_trolley::PantoShape { heights: (0..=32).map(|i| 3.3 + (top - 3.3) * i as f32 / 32.0).collect(), head: [0.0, 0.0], half_width: 0.5 }
    }

    #[test]
    fn the_wire_is_the_standard_height_where_the_pantograph_reaches_it() {
        use omsi_sim::rail::RailClass;
        assert_eq!(wire_height(Some(RailClass::Tram), &shape(6.5)), 5.7);
        assert_eq!(wire_height(Some(RailClass::Train), &shape(6.5)), 5.6);
    }

    #[test]
    fn a_wire_beyond_the_reach_is_brought_within_it() {
        use omsi_sim::rail::RailClass;
        let s = shape(4.6);
        let h = wire_height(Some(RailClass::Tram), &s);
        assert!(h <= s.top() && h >= s.bottom(), "{h}");
    }

    #[test]
    fn the_wire_is_rougher_in_the_wet_and_in_frost() {
        let dry = wire_roughness(0.0, 15.0);
        assert!(wire_roughness(1.0, 15.0) > dry);
        assert!(wire_roughness(1.0, -5.0) > wire_roughness(1.0, 15.0));
        assert_eq!(wire_roughness(0.0, -5.0), dry, "dry frost is not ice");
    }

    #[test]
    fn point_at_interpolates_properly() {
        let mut trail = std::collections::VecDeque::new();
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
        let mut trail = std::collections::VecDeque::new();
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

    #[test]
    fn angle_diff_wraps_correctly() {
        assert!((angle_diff(10.0, 0.0) - 10.0).abs() < 1e-4);
        assert!((angle_diff(350.0, 10.0) - (-20.0)).abs() < 1e-4);
        assert!((angle_diff(10.0, 350.0) - 20.0).abs() < 1e-4);
        assert!((angle_diff(180.0, 0.0) - 180.0).abs() < 1e-4 || (angle_diff(180.0, 0.0) - (-180.0)).abs() < 1e-4);
    }

    #[test]
    fn dual_bogie_heading_aligned_forward_for_both_lane_directions() {
        use omsi_sim::traffic::LaneBuilder;

        // Lane going North (+Y) from (0, 0, 0) to (0, 100, 0)
        let lane = LaneBuilder::polyline(
            vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 100.0, 0.0)],
            LaneKind::Rail,
            1.435,
        );
        let net = Network {
            lanes: vec![lane],
            ..Default::default()
        };

        let l = 5.9; // bogie half-span

        // 1. Vehicle facing North (along == true)
        let r_north = RailDrive {
            lane: 0,
            s: 50.0,
            along: true,
            trail: Default::default(),
            u: 0.0,
            body: Default::default(),
            last_heading: None,
        };
        let mut f_north = r_north.clone();
        f_north.step(&net, None, l, 0);
        let mut b_north = r_north.clone();
        b_north.step(&net, None, -l, 0);
        let pos_fn = net.lanes[f_north.lane].at(f_north.s).0;
        let pos_rn = net.lanes[b_north.lane].at(b_north.s).0;
        let diff_n = pos_fn - pos_rn;
        let h_north = diff_n.x.atan2(diff_n.y).to_degrees().rem_euclid(360.0);
        assert!((h_north - 0.0).abs() < 1e-4, "North-bound heading must be 0°, got {h_north}");

        // 2. Vehicle facing South (along == false)
        let r_south = RailDrive {
            lane: 0,
            s: 50.0,
            along: false,
            trail: Default::default(),
            u: 0.0,
            body: Default::default(),
            last_heading: None,
        };
        let mut f_south = r_south.clone();
        f_south.step(&net, None, l, 0);
        let mut b_south = r_south.clone();
        b_south.step(&net, None, -l, 0);
        let pos_fs = net.lanes[f_south.lane].at(f_south.s).0;
        let pos_rs = net.lanes[b_south.lane].at(b_south.s).0;
        let diff_s = pos_fs - pos_rs;
        let h_south = diff_s.x.atan2(diff_s.y).to_degrees().rem_euclid(360.0);
        assert!((h_south - 180.0).abs() < 1e-4, "South-bound heading must be 180°, got {h_south}");
    }

    /// The two tracks of a tram street are both `LaneKind::Rail` and a metre or two apart.
    /// A vehicle put on the one going the other way drives the whole consist backwards, so
    /// the lane is chosen by which way it runs, not by which is nearest.
    #[test]
    fn a_rail_vehicle_is_put_on_the_track_running_its_own_way() {
        use omsi_sim::traffic::LaneBuilder;
        // lane 0 runs north, lane 1 the same street southwards beside it
        let net = Network {
            lanes: vec![
                LaneBuilder::polyline(
                    vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 100.0, 0.0)],
                    LaneKind::Rail,
                    1.435,
                ),
                LaneBuilder::polyline(
                    vec![DVec3::new(-1.5, 100.0, 0.0), DVec3::new(-1.5, 0.0, 0.0)],
                    LaneKind::Rail,
                    1.435,
                ),
            ],
            ..Default::default()
        };
        let here = DVec3::new(0.0, 50.0, 0.0);

        // the vehicle stands on lane 0, but faces south: lane 1 runs south and lane 0 runs
        // north, and the way the track runs beats the metre and a half between them
        let (lane, ..) = track_for(&net, here, 180.0, 100.0).unwrap();
        assert_eq!(lane, 1, "a vehicle facing south belongs on the southbound track");
        // and the other way about
        let (lane, ..) = track_for(&net, here, 0.0, 100.0).unwrap();
        assert_eq!(lane, 0, "a vehicle facing north belongs on the northbound track");

        // both tracks run the wrong way: the nearest one still takes it, off the rails
        // rather than nowhere
        let (lane, ..) = track_for(&net, here, 90.0, 100.0).unwrap();
        assert_eq!(lane, 0, "with neither track the right way, the nearest one takes it");
        // out of reach: nothing, rather than the nearest lane anywhere on the map
        assert!(track_for(&net, DVec3::new(500.0, 50.0, 0.0), 0.0, 0.5).is_none());
    }

    #[test]
    fn the_track_is_looked_for_at_the_height_the_vehicle_was_placed_at() {
        use omsi_sim::traffic::LaneBuilder;
        let level = |z: f64| LaneBuilder::polyline(vec![DVec3::new(0.0, 0.0, z), DVec3::new(0.0, 100.0, z)], LaneKind::Rail, 1.435);
        let net = Network { lanes: vec![level(0.0), level(20.0)], ..Default::default() };
        let lifted = DVec3::new(0.0, 50.0, 50005.0);
        assert!(track_for(&net, lifted, 0.0, 3000.0).is_none());
        let (lane, ..) = track_for(&net, search_point(lifted, 0.0), 0.0, 3000.0).unwrap();
        assert_eq!(lane, 0);
        let (lane, ..) = track_for(&net, search_point(lifted, 20.0), 0.0, 3000.0).unwrap();
        assert_eq!(lane, 1);
    }
}
