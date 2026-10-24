//! The body's motion: placing it, stepping it through a frame (suspension, tyres, grip,
//! drive and brakes) and its collisions with obstacles.

use super::*;

impl RigidBody {
    /// `hub_heights[a]`: unloaded hub height of axle `a` above the model origin; the tyre
    /// radius when the model does not say (see `VehicleType::wheel_geometry`).
    pub fn from_definition(def: &Vehicle, hub_heights: &[Option<f32>]) -> RigidBody {
        let mass = if def.mass < 100.0 { def.mass * 1000.0 } else { def.mass }.max(500.0);
        let moi = def.moment_of_inertia;
        let scale = if moi[0] < 5000.0 { 1000.0 } else { 1.0 };
        // `[momentofintertia]` goes to ODE as I11, I22, I33 of Omsi.exe's y-up body frame
        // (0x7af0a4): about the lateral axis (pitch), the vertical one (yaw) and the
        // longitudinal one (roll) - roll takes the third value (0x7e4ef3), yaw the second
        // (0x7e5110), whatever the SDK's comment says. Here (right, forward, up): pitch,
        // roll, yaw. Read in the comment's order the SD202 rolled on 80 t m² instead of
        // 300, twice as fast, and every uneven patch rocked it like a boat.
        let inertia = Vec3::new((moi[0] * scale).max(100.0), (moi[2] * scale).max(100.0), (moi[1] * scale).max(100.0));
        let cog_xy = def.cog.map(|c| (c[0], c[1])).unwrap_or((0.0, 0.0));
        let cog_z = if def.cog_height > 0.0 { def.cog_height } else { def.cog.map(|c| c[2]).filter(|z| *z > 0.0).unwrap_or(1.0) };
        let cog = Vec3::new(cog_xy.0, cog_xy.1, cog_z);
        let front_long = def.axles.iter().map(|a| a.long).fold(f32::MIN, f32::max);
        let mut wheels = Vec::new();
        let mut wheel_axle = Vec::new();
        for (ai, a) in def.axles.iter().enumerate() {
            let r = (a.wheel_diameter / 2.0).max(0.15);
            let z = hub_heights.get(ai).copied().flatten().unwrap_or(r);
            let k = if a.spring > 0.0 { a.spring * 1000.0 } else { 150_000.0 };
            let c = if a.damper > 0.0 { a.damper * 1000.0 } else { 12_000.0 };
            let max_force = if a.max_force > 0.0 { a.max_force * 1000.0 } else { 200_000.0 };
            // where the tyres stand across (the middle of the band between the two widths)
            // and where the strut pushes the body (the outer width)
            let outer = (a.max_width / 2.0).max(0.3);
            let inner = if a.min_width > 0.0 && a.min_width < a.max_width { (a.max_width + a.min_width) / 4.0 } else { outer * 0.85 };
            let tyre_k = TYRE_K.max(k * TYRE_OVER_SPRING);
            let tyre_c = TYRE_C * (tyre_k / TYRE_K).sqrt();
            for side in [-1.0f32, 1.0] {
                let inertia_inv = if a.inertia_inv > 0.0 { a.inertia_inv } else { DEFAULT_INERTIA_INV };
                wheels.push(RigidWheel { spin: 0.0, slipping: false, locked: false, inertia_inv, attach: Vec3::new(side * inner, a.long, z), lever: side * (outer - inner), radius: r, driven: a.driven, steered: (a.long - front_long).abs() < 0.01, steer: 0.0, tyre_k, tyre_c, spring: k, damper: c, max_force, spring_factor: 1.0, rest_load: 0.0, compression: 0.0, compression_rate: 0.0, touch: None, on_ground: true, rotation_deg: 0.0, rpm: 0.0, load: 0.0, ground_z: 0.0, ground_seen: false, walls: Vec::new(), step_force: 0.0, lat_defl: 0.0, surface: 0, surface_now: 0, surface_was: 0, surface_mix: 1.0, mu_scale: 1.0, rough: 0.0 });
                wheel_axle.push(ai);
            }
        }
        let loads = wheel_rest_loads(def);
        for (w, a) in wheels.iter_mut().zip(&wheel_axle) {
            w.rest_load = loads.get(*a).copied().unwrap_or(0.0);
        }
        let s = (front_long - def.rot_pnt_long).abs().max(1.0);
        let max_steer_deg = (def.inv_min_turn_radius * s).atan().to_degrees().clamp(10.0, 60.0);
        // a file without a usable `[inv_min_turnradius]` still steers its front axle as far
        // as the old default lock
        let inv_min_turn_radius = if def.inv_min_turn_radius > 0.0 { def.inv_min_turn_radius } else { max_steer_deg.to_radians().tan() / s };
        let springs: f32 = def.axles.iter().map(|a| 2.0 * if a.spring > 0.0 { a.spring } else { 150.0 }).sum();
        let body_freq = (springs / (mass / 1000.0)).max(0.0).sqrt();
        RigidBody { mass, inertia, cog, position: DVec3::ZERO, orientation: Quat::IDENTITY, velocity: Vec3::ZERO, omega: Vec3::ZERO, wheels, wheel_axle, steer_deg: 0.0, kappa: 0.0, max_steer_deg, rot_pnt_long: def.rot_pnt_long, inv_min_turn_radius, body_freq, holding: true, rolling_resistance: if def.rolling_resistance > 0.0 { def.rolling_resistance } else { 0.008 * mass * 9.81 }, accel_body: Vec3::ZERO, friction: 0.85, wheel_impacts: Vec::new(), coupled: Vec::new(), spawned_inside: None, wheel_walls: true, aero: realism::Aero::of(def.bounding_box.map(|b| [b[0], b[1], b[2]]), mass), air_density: realism::air_density(20.0, 0.0), wind: Vec3::ZERO, wetness: 0.0, road_s: 0.0, realistic: realistic_default() }
    }

    /// Place the body at rest with its wheels on the ground plane at `origin.z`: heading
    /// `heading_deg`, springs already carrying the load (no drop and bounce at the start).
    pub fn place(&mut self, origin: DVec3, heading_deg: f64) {
        self.orientation = Quat::from_rotation_z((-heading_deg).to_radians() as f32);
        let n = self.wheels.len().max(1) as f32;
        let sag: f32 = self.wheels.iter().map(|w| w.radius - w.attach.z - w.rest_compression().min(BUMP)).sum::<f32>() / n;
        let origin = origin + DVec3::Z * sag as f64;
        self.position = origin + self.orientation.mul_vec3(self.cog).as_dvec3();
        self.velocity = Vec3::ZERO;
        self.omega = Vec3::ZERO;
        self.spawned_inside = None;
        for w in self.wheels.iter_mut() {
            w.compression = w.rest_compression().min(BUMP);
            w.compression_rate = 0.0;
            w.touch = None;
            w.lat_defl = 0.0;
        }
        // standing: the ground holds the body up against gravity
        self.accel_body = Vec3::new(0.0, 0.0, 9.81);
        for w in self.wheels.iter_mut() {
            w.ground_seen = false;
            w.walls.clear();
        }
    }

    /// Model origin (the point the renderer places) in the world.
    pub fn origin(&self) -> DVec3 {
        self.position - self.orientation.mul_vec3(self.cog).as_dvec3()
    }

    /// Heading (deg, clockwise from north), pitch and bank (deg) of the body.
    pub fn heading_pitch_bank(&self) -> (f64, f32, f32) {
        let f = self.orientation.mul_vec3(Vec3::Y);
        let r = self.orientation.mul_vec3(Vec3::X);
        let heading = (f.x.atan2(f.y) as f64).to_degrees().rem_euclid(360.0);
        let pitch = f.z.clamp(-1.0, 1.0).asin().to_degrees();
        let bank = (-r.z).clamp(-1.0, 1.0).asin().to_degrees();
        (heading, pitch, bank)
    }

    /// The steering angle of wheel `i`'s axle (rad, positive to the right): the angle at the
    /// middle of the axle, atan((long - rot_pnt_long) x curvature), the same for its left and
    /// right wheel. That is what Omsi.exe hands the scripts as `Axle_Steering_<n>_L` and
    /// `_R` alike (0x7cffb8 writes both variables of 0x7ea834 from the axle record's +8) -
    /// the tyres' own angles (Ackermann, `RigidWheel::steer`) made the steering
    /// wheel of the cab, which turns with `Axle_Steering_0_L`, go further to the left than
    /// to the right (#953, #1073).
    pub fn axle_steer(&self, i: usize) -> f32 {
        let Some(w) = self.wheels.get(i) else { return 0.0 };
        ((w.attach.y - self.rot_pnt_long) * self.kappa).atan().clamp(-1.05, 1.05)
    }

    /// Forward speed (m/s) in the body frame.
    pub fn forward_speed(&self) -> f32 {
        self.velocity.dot(self.orientation.mul_vec3(Vec3::Y))
    }

    pub fn kinetic_energy(&self) -> f32 {
        0.5 * self.mass * self.velocity.length_squared() + 0.5 * (self.inertia * self.omega * self.omega).element_sum()
    }

    /// Inverse of the effective mass the body shows along world direction `n` at world
    /// offset `r` from its centre of gravity.
    pub(super) fn inv_mass_at(&self, r: Vec3, n: Vec3) -> f32 {
        let inv = self.orientation.inverse();
        let (rb, nb) = (inv.mul_vec3(r), inv.mul_vec3(n));
        let w = rb.cross(nb) / self.inertia;
        1.0 / self.mass + w.cross(rb).dot(nb)
    }

    /// Apply the world impulse `p` at world offset `r` from the centre of gravity.
    fn apply_impulse(&mut self, r: Vec3, p: Vec3) {
        let inv = self.orientation.inverse();
        self.velocity += p / self.mass;
        self.omega += inv.mul_vec3(r).cross(inv.mul_vec3(p)) / self.inertia;
    }

    /// The body's `[boundingbox]` in the world: its footprint turned with the heading, its
    /// height range over all eight corners (a pitched bus reaches lower at one end).
    pub fn body_box(&self, bb: [f32; 6]) -> Obb {
        let (heading, _, _) = self.heading_pitch_bank();
        let origin = self.origin();
        let mut o = Obb::from_box(bb, origin, heading);
        let centre = origin + self.orientation.mul_vec3(Vec3::new(bb[3], bb[4], bb[5])).as_dvec3();
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for sx in [-0.5f32, 0.5] {
            for sy in [-0.5f32, 0.5] {
                for sz in [-0.5f32, 0.5] {
                    let z = centre.z + self.orientation.mul_vec3(Vec3::new(bb[0] * sx, bb[1] * sy, bb[2] * sz)).z as f64;
                    lo = lo.min(z);
                    hi = hi.max(z);
                }
            }
        }
        o.z0 = lo;
        o.z1 = hi;
        o
    }

    /// The body's `[boundingbox]` as it stands, turned, pitched and banked, grown by
    /// `margin` on every side.
    pub fn body_solid(&self, bb: [f32; 6], margin: f32) -> crate::collision::Box3 {
        let r = self.orientation;
        let centre = self.origin() + r.mul_vec3(Vec3::new(bb[3], bb[4], bb[5])).as_dvec3();
        crate::collision::Box3 {
            center: centre,
            axes: [r.mul_vec3(Vec3::X).as_dvec3(), r.mul_vec3(Vec3::Y).as_dvec3(), r.mul_vec3(Vec3::Z).as_dvec3()],
            half: (Vec3::new(bb[0], bb[1], bb[2]) * 0.5 + Vec3::splat(margin)).as_dvec3(),
        }
    }

    /// One step. `drive_torque` (N m at the driven wheels, from `M_Wheel`), `brake` per
    /// wheel (N), `steer` −1..1, `probe(x, y, z_top)` the ground at a point.
    pub fn step(&mut self, dt: f32, drive_torque: f32, brake: &[f32], steer: f32, probe: &dyn Fn(f64, f64, f64) -> GroundProbe) {
        // a long frame (below 20 fps) is stepped in slices of at most 50 ms, so that the
        // body covers the whole frame the clock, the odometer and the scripts count (it was
        // cut to 50 ms: at 10 fps the bus went half as far as the time went on)
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.25) } else { 0.0 };
        // A script's value that is no number - a bus's `M_Wheel` or a brake force of 0/0 or
        // 1/0 - put the whole body at NaN within a step: the bus vanished and the steering's
        // clamp stopped the game a frame later ("min > max, or either was NaN", #1045).
        // Such an input counts as none; and should the body still come out of the step at NaN
        // (a value of the file's), it stays where it stood before, at rest.
        let finite = |x: f32| if x.is_finite() { x } else { 0.0 };
        let (drive_torque, steer) = (finite(drive_torque), finite(steer));
        let sane: Vec<f32>;
        let brake = if brake.iter().all(|b| b.is_finite()) {
            brake
        } else {
            sane = brake.iter().map(|&b| finite(b)).collect();
            &sane
        };
        let before = (self.position, self.orientation, self.steer_deg);
        let wheels_before: Vec<(f32, Option<f32>)> = self.wheels.iter().map(|w| (w.compression, w.touch)).collect();
        let slices = (dt / 0.05).ceil().max(1.0) as usize;
        let mut impacts = Vec::new();
        for _ in 0..slices {
            self.step_slice(dt / slices as f32, drive_torque, brake, steer, probe);
            // OMSI damps the body's pitch and roll - never its yaw - by sin(x)/x each frame,
            // x = 1.5 x sqrt(springs / mass) x frame (Omsi.exe 0x7e4f55..0x7e50b0). Its
            // frames are OMSI's (its options.cfg caps them at 30 a second): taken per frame
            // of this game, a machine drawing 150 frames a second damped the body a fifth as
            // much, and the bus rocked on its springs like a boat. The same damping a second
            // as OMSI's at 30 frames, however many are drawn:
            let k = body_damping(self.body_freq, dt / slices as f32);
            self.omega.x *= k;
            self.omega.y *= k;
            // (one impact per obstacle a frame, as a single slice gives)
            for m in self.wheel_impacts.drain(..) {
                match impacts.iter_mut().find(|x: &&mut Impact| x.obstacle == m.obstacle) {
                    Some(x) if x.energy >= m.energy => {}
                    Some(x) => *x = m,
                    None => impacts.push(m),
                }
            }
        }
        self.wheel_impacts = impacts;
        if !(self.position.is_finite() && self.orientation.is_finite() && self.velocity.is_finite() && self.omega.is_finite()) {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| log::warn!("vehicle physics: the body's state became no number; it stays where it was, at rest"));
            (self.position, self.orientation, self.steer_deg) = before;
            self.velocity = Vec3::ZERO;
            self.omega = Vec3::ZERO;
            self.accel_body = Vec3::ZERO;
            for (w, (c, t)) in self.wheels.iter_mut().zip(wheels_before) {
                (w.compression, w.touch) = (c, t);
                w.compression_rate = 0.0;
                w.spin = 0.0;
            }
            self.wheel_impacts.clear();
        }
    }

    fn step_slice(&mut self, dt: f32, drive_torque: f32, brake: &[f32], steer: f32, probe: &dyn Fn(f64, f64, f64) -> GroundProbe) {
        self.steer_axles(steer);
        // (substeps of at most ~4 ms whatever the frame: a frame held up by loading - 50 ms -
        // made 12 ms substeps, too long for the stiff tyres, and the body hopped on its
        // springs for no reason the driver could see. As many as the frame's time needs and
        // no more: at least four a frame made the physics cost grow with the frame rate - a
        // thousand substeps a second at 240 frames, four times those at 60 - for substeps
        // of a millisecond the tyres do not need)
        let substeps = ((dt / 0.0042).ceil() as usize).clamp(1, 16);
        let h = dt / substeps as f32;
        let mut accel_sum = Vec3::ZERO;
        self.wheel_impacts.clear();
        for _ in 0..substeps {
            let rot = self.orientation;
            let up = rot.mul_vec3(Vec3::Z);
            let mut force = Vec3::new(0.0, 0.0, -9.81 * self.mass);
            // the air and the road under the tyres (see `air_and_road`)
            let (air_force, air_torque) = self.air_and_road(rot, h);
            force += air_force;
            let mut torque = air_torque; // body frame
            // Every driven wheel of the train takes its share of `M_Wheel`. A body without
            // driven wheels of its own (a pusher's front section) was pushed by nothing: the
            // count was clamped to one and the torque went to wheels that do not drive.
            let coupled_wheels: usize = self.coupled.iter().map(|d| d.driven_wheels).sum();
            let driven = (self.wheels.iter().filter(|w| w.driven).count() + coupled_wheels).max(1) as f32;
            // the towed mass moves with the body along its length
            let body_fwd = rot.mul_vec3(Vec3::Y);
            let towed: f32 = self.coupled.iter().map(|d| d.mass).sum();
            let n_wheels = self.wheels.len().max(1) as f32;
            // tyre plane (forward turned by the wheel's steering angle), the hub with the
            // spring unloaded and where it is now
            let (position, cog) = (self.position, self.cog);
            let tyre = move |w: &RigidWheel| {
                let ang = w.steer;
                let fwd = rot.mul_vec3(Vec3::new(ang.sin(), ang.cos(), 0.0));
                let right = rot.mul_vec3(Vec3::new(ang.cos(), -ang.sin(), 0.0));
                let hub0 = position + rot.mul_vec3(w.attach - cog).as_dvec3();
                let hub = hub0 + (up * w.compression.max(-DROOP)).as_dvec3();
                (fwd, right, Vec3::new(fwd.x, fwd.y, 0.0).normalize_or(Vec3::Y), hub0, hub)
            };
            let found = self.read_ground(&tyre, rot, h, probe);
            self.stop_at_faces(&tyre, rot, h);
            let omega_world = rot.mul_vec3(self.omega);
            let mut contacts: Vec<Contact> = Vec::with_capacity(self.wheels.len());
            for (i, w) in self.wheels.iter_mut().enumerate() {
                let (fwd, right, fwd_h, hub0, hub) = tyre(w);
                let r = w.radius;
                // pressed into a face: pushed back out
                let mut f_world = Vec3::ZERO;
                for face in &w.walls {
                    let n = face.normal.extend(0.0);
                    let pen = face.reach(fwd_h, r) - face.distance(hub);
                    if pen > 0.0 {
                        let v_hub = self.velocity + omega_world.cross((hub - self.position).as_vec3());
                        f_world += n * (WALL_K * pen - WALL_C * v_hub.dot(n)).max(0.0);
                    }
                }
                w.step_force += f_world.dot(fwd_h);
                // The tyre pushes the wheel up, the strut pushes it down and the body up, and
                // the wheel's own mass moves between the two. The tyre carries the static
                // load already where it just touches, so that the ride height stays the
                // springs' alone (see the module's notes).
                let (tyre_f, tyre_static, n) = if omsi_suspension() {
                    omsi_strut(w, r, hub0, position, cog, self.cog, rot, up, self.velocity, omega_world, h, probe)
                } else {
                    let m_w = (w.rest_load / 9.81 * UNSPRUNG).max(40.0);
                    let k = w.spring * w.spring_factor.max(0.05);
                    // How far the ground under the tyre can rise within a substep: a tyre rolls up
                    // a step along its envelope, never steeper than 2.4 (see CLIMB). A reading
                    // that leaps higher (a kerb's edge or a surface object's rim caught for one
                    // substep: +25 cm under the Urbino's front wheel at a Spandau kerb) is
                    // climbed at that pace - taken at once it struck the wheel with 260 kN and
                    // threw it into its arch.
                    let v_along = (self.velocity + omega_world.cross((hub - self.position).as_vec3())).dot(fwd_h).abs();
                    let max_rise = (v_along.max(0.5) * 2.4 + 0.3) * h;
                    let need = found[i].map(|(need, _, _)| if w.ground_seen { need.min(w.ground_z + r as f64 + max_rise as f64) } else { need });
                    let touch = need.map(|need| ((need - hub0.z) / up.z.max(0.3) as f64) as f32);
                    if let Some(need) = need {
                        w.ground_z = need - r as f64;
                        w.ground_seen = true;
                    }
                    let (tyre_f, tyre_static) = match touch {
                        // (a wheel that cannot reach the ground at full droop hangs in the air)
                        Some(t) if t >= -DROOP - 0.05 => {
                            let pen = t - w.compression;
                            let closing = w.touch.map(|p| ((t - p) / h).clamp(-3.0, 3.0)).unwrap_or(0.0) - w.compression_rate;
                            let s = w.tyre_k * pen + w.rest_load;
                            if s > 0.0 {
                                ((s + w.tyre_c * closing).clamp(0.0, w.max_force * 3.0), s)
                            } else {
                                (0.0, 0.0)
                            }
                        }
                        _ => (0.0, 0.0),
                    };
                    w.touch = touch;
                    // the strut: spring, a digressive damper (a kerb struck at speed must not fire
                    // the body into the air) and the bump stop, a stiff rubber block; as a whole it
                    // takes no more than three times the load it is built for (`achse_maxforce`)
                    // (the spring only pushes: below its unloaded length the wheel hangs on the
                    // damper, which also works on the rebound)
                    // (OMSI: spring and damper together never pass more than `achse_maxforce`;
                    // beyond it the bump stop, a rubber block of its own, takes over)
                    let c = w.compression;
                    let rate_c = w.compression_rate.clamp(-1.5, 1.5);
                    let bump = if c > BUMP { (w.spring * 10.0 * (c - BUMP)).min(w.max_force * 2.0) } else { 0.0 };
                    let n = (k * c.max(0.0) + w.damper * rate_c).clamp(-w.max_force * 0.5, w.max_force) + bump;
                    // the wheel moves against the body, which is itself accelerated by what holds
                    // it up (`accel_body.z`, 9.81 m/s² standing, 0 in the air: a wheel off a kerb
                    // drops, a wheel of a body in the air does not) - semi-implicit Euler, stable
                    // with the stiff tyre at these steps
                    w.compression_rate += ((tyre_f - n) / m_w - self.accel_body.z) * h;
                    w.compression += w.compression_rate * h;
                    if w.compression < -DROOP {
                        w.compression = -DROOP;
                        w.compression_rate = w.compression_rate.max(0.0);
                    } else if w.compression > BUMP + 0.08 {
                        w.compression = BUMP + 0.08;
                        w.compression_rate = w.compression_rate.min(0.0);
                    }
                    w.on_ground = tyre_f > 0.0;
                    w.load = tyre_f;
                    (tyre_f, tyre_static, n)
                };
                // the strut's force turns the body about its length from the outer width, not
                // from where the tyre stands (see `RigidWheel::lever`): body x cross body z
                torque += Vec3::new(0.0, -w.lever * n, 0.0);
                // and OMSI's damper reads the body's speed out there too (0x7e4899: roll rate
                // x maxwidth / 2): the part of it the hub does not see, while the tyre carries
                if tyre_f > 0.0 && !omsi_suspension() {
                    torque.y -= (w.attach.x + w.lever) * w.damper * self.omega.y * w.lever;
                }
                let Some((_, slope, contact_dx)) = found[i].filter(|_| tyre_f > 0.0) else {
                    // off the ground the wheel keeps its turning (a brake stops it)
                    let (drive_w, brake_w) = (if w.driven { drive_torque / r / driven } else { 0.0 }, brake.get(i).copied().unwrap_or(0.0).max(0.0));
                    let before = w.spin;
                    w.spin += h * w.inertia_inv * (drive_w - sign(w.spin) * brake_w);
                    if before != 0.0 && sign(before) != sign(w.spin) && brake_w > 0.0 {
                        w.spin = 0.0;
                    }
                    w.slipping = !omsi_cfg::flags::OMSI_NO_WHEEL_SLIP.is_set();
                    w.rpm = w.spin * 60.0 / std::f32::consts::TAU;
                    w.rotation_deg = (w.rotation_deg + w.spin.to_degrees() * h).rem_euclid(360.0);
                    // the tyre off the ground (or nothing under it: the edge of the loaded
                    // world): the strut still pushes on the body
                    let f = f_world + up * n;
                    force += f;
                    torque += rot.inverse().mul_vec3((hub - self.position).as_vec3()).cross(rot.inverse().mul_vec3(f));
                    continue;
                };
                // The ground pushes along its own normal; the strut carries the part along
                // the body's up axis, and the rest pushes the wheel back up a kerb (or on
                // down an edge). Measured against the body, a slope the bus stands on
                // already leans with it and adds nothing; the tyre's static load does the
                // pushing - the damping kick as the tyre meets the edge is taken by the
                // wheel's own mass.
                let normal = (Vec3::Z - fwd_h * slope).normalize();
                let along_up = normal.dot(up).max(0.3);
                let carried = tyre_static.min(w.max_force * 2.0);
                let step = (normal - up * normal.dot(up)) * (carried / along_up);
                w.step_force += step.dot(fwd_h);
                let contact = hub + (fwd_h * contact_dx.clamp(-r, r)).as_dvec3() - DVec3::Z * r as f64;
                let r_world = (contact - self.position).as_vec3();
                let v_point = self.velocity + omega_world.cross(r_world);
                let v_long = v_point.dot(fwd);
                // the tyre works with at most its rated load: the spike of a wheel landing on
                // its bump stop is not grip, and fed into the side force it spun a bus round
                // on a verge
                let grip = tyre_f.min(w.max_force);
                contacts.push(Contact {
                    r: r_world,
                    fwd,
                    right,
                    base: f_world + up * n + step,
                    drive: if w.driven { drive_torque / r / driven } else { 0.0 },
                    brake: brake.get(i).copied().unwrap_or(0.0).max(0.0) + self.rolling_resistance / n_wheels * if self.realistic { realism::rolling_scale(w.surface_now, v_long, tyre_f / w.rest_load.max(1.0)) } else { 1.0 },
                    grip,
                    mu: self.friction * w.mu_scale,
                    v_long,
                    lateral: true,
                    wheel: Some(i),
                });
            }
            // The coupled parts, each a tyre at the joint that pushes and brakes along the
            // body's axis and takes no side force.
            let fwd_flat = Vec3::new(body_fwd.x, body_fwd.y, 0.0).normalize_or(Vec3::Y);
            for d in &self.coupled {
                let share = Vec3::new(d.dir.x, d.dir.y, 0.0).normalize_or(fwd_flat).dot(fwd_flat).max(0.0);
                let r_world = rot.mul_vec3(d.point - self.cog);
                let cap = self.friction * d.driven_load;
                let drive = if d.driven_wheels > 0 { (drive_torque / d.radius.max(0.1) / driven * d.driven_wheels as f32).clamp(-cap, cap) } else { 0.0 };
                contacts.push(Contact {
                    r: r_world,
                    fwd: body_fwd,
                    right: Vec3::ZERO,
                    base: body_fwd * (-9.81 * d.mass * body_fwd.z) - Vec3::Z * d.joint_load,
                    drive: drive * share,
                    brake: d.brake.max(0.0) * share,
                    grip: d.load,
                    mu: self.friction,
                    v_long: (self.velocity + omega_world.cross(r_world)).dot(body_fwd),
                    lateral: false,
                    wheel: None,
                });
            }
            // Tyre forces as OMSI has them (Omsi.exe 0x7e2dc1..0x7e44ee).
            // Along the tyre: a rolling one passes on the drive and the brake, a slipping one
            // the sliding friction. Tyres that stand hold like static friction - but
            // together: they take up everything else that pushes the body (gravity on a
            // slope, the drive, the rolling ones) and what is left of its speed, shared by
            // what each brake can bear. One wheel at a time, each cancelling only its quarter
            // of the bus, a parked bus crept down an 8 % slope at 2 mm/s.
            let mut outer = force + contacts.iter().map(|c| c.base).sum::<Vec3>();
            let mut rolling_long = vec![0.0f32; contacts.len()];
            for (k, c) in contacts.iter().enumerate() {
                // a slipping wheel passes on the sliding friction, whichever way the tyre
                // slides over the road (the wheel's own speed against the ground's)
                if let Some(w) = c.wheel.map(|i| &self.wheels[i]).filter(|w| w.slipping) {
                    let mu_n = c.mu * c.grip;
                    let slide = w.spin * w.radius - c.v_long;
                    rolling_long[k] = mu_n * sign(slide);
                    outer += c.fwd * rolling_long[k];
                    continue;
                }
                if c.v_long.abs() > STANDING {
                    rolling_long[k] = c.drive - c.brake * c.v_long.signum();
                    outer += c.fwd * rolling_long[k];
                } else {
                    outer += c.fwd * c.drive;
                }
            }
            let standing: Vec<usize> = (0..contacts.len()).filter(|&k| contacts[k].v_long.abs() <= STANDING && !contacts[k].wheel.is_some_and(|i| self.wheels[i].slipping)).collect();
            let hold = |axis: Vec3, mass: f32, cap: &dyn Fn(&Contact) -> f32| -> Vec<f32> {
                let need = -outer.dot(axis) - self.velocity.dot(axis) * mass / h;
                let total: f32 = standing.iter().map(|&k| cap(&contacts[k])).sum();
                standing.iter().map(|&k| if total > 1e-3 { (need * cap(&contacts[k]) / total).clamp(-cap(&contacts[k]), cap(&contacts[k])) } else { 0.0 }).collect()
            };
            let hold_long = hold(body_fwd, self.mass + towed, &|c| c.brake);
            // (the tyres' pull along the body, for the pitch lever below)
            let mut long_sum = 0.0f32;
            let mut longs = vec![0.0f32; contacts.len()];
            for (k, c) in contacts.iter().enumerate() {
                let mut f_long = match standing.iter().position(|&j| j == k) {
                    Some(si) => c.drive + hold_long[si],
                    None => rolling_long[k],
                };
                let max_f = c.mu * c.grip;
                f_long = f_long.clamp(-max_f, max_f);
                longs[k] = f_long;
                let f_world = c.base + c.fwd * f_long;
                long_sum += f_long * c.fwd.dot(body_fwd);
                if let Some(i) = c.wheel {
                    self.wheels[i].advance_spin(c, f_long, c.mu, h);
                }
                force += f_world;
                torque += rot.inverse().mul_vec3(c.r).cross(rot.inverse().mul_vec3(f_world));
            }
            // Driving and braking pitch the body about a lever longer than the centre of
            // gravity's height over the road: Omsi.exe takes the tyres' forces at the hubs,
            // their radius more (0x7e46a5: (drive - brake) x d/2 besides [schwerpunkt] x
            // the pull), and not while the bus stands with its brakes holding (0x7e4660).
            if self.velocity.dot(body_fwd).abs() > 0.2 {
                let r = self.wheels.iter().map(|w| w.radius).sum::<f32>() / self.wheels.len().max(1) as f32;
                torque.x += long_sum * r;
            }
            (force, torque) = self.hold_across(&contacts, &longs, force, torque, rot, body_fwd, towed, h);
            // integrate
            let along = force.dot(body_fwd);
            let acc = (force - body_fwd * along) / self.mass + body_fwd * (along / (self.mass + towed));
            accel_sum += rot.inverse().mul_vec3(acc - Vec3::new(0.0, 0.0, -9.81));
            self.velocity += acc * h;
            self.position += (self.velocity * h).as_dvec3();
            let iw = self.inertia * self.omega;
            let omega_dot = (torque - self.omega.cross(iw)) / self.inertia;
            self.omega += omega_dot * h;
            let dq = Quat::from_scaled_axis(rot.mul_vec3(self.omega) * h);
            self.orientation = (dq * rot).normalize();
        }
        self.accel_body = accel_sum / substeps as f32;
        self.settle(drive_torque, brake);
    }

    /// What the air does to the body this substep (world force, body-frame torque) and where the
    /// tyres stand on the road's unevenness and its surfaces (see `realism`). Without it a bus
    /// rolling downhill without its brakes ran on to any speed, and no gale ever moved one.
    fn air_and_road(&mut self, rot: Quat, h: f32) -> (Vec3, Vec3) {
        let inv = rot.inverse();
        let (mut force, mut torque) = (Vec3::ZERO, Vec3::ZERO);
        if self.realistic {
            // (every OMSI 2 bus already has its still-air drag in its brake script - bremse.osc
            // adds 9.5 m2 x cw 0.8 x 0.6 v2 to every wheel's brake force - so only what the weather
            // changes is added here: the wind and an air that is not 1.2 kg/m3)
            let (f, t) = realism::wind_force(&self.aero, self.air_density, inv.mul_vec3(self.velocity), inv.mul_vec3(self.wind));
            force = rot.mul_vec3(f);
            torque = t;
        } else {
            let v = self.velocity;
            let area = (self.mass / 2200.0).clamp(2.0, 8.5);
            force = -v * v.length() * (0.5 * 1.2 * 0.6 * area);
        }
        // where on the road's unevenness the body stands (a reversing bus retraces it)
        let ds = self.velocity.dot(rot.mul_vec3(Vec3::Y)) as f64 * h as f64;
        self.road_s += ds;
        for w in self.wheels.iter_mut() {
            w.surface_mix = (w.surface_mix + ds.abs() as f32).min(1.0);
            if w.surface != w.surface_now {
                (w.surface_was, w.surface_now, w.surface_mix) = (w.surface_now, w.surface, 0.0);
            }
            if !self.realistic {
                (w.mu_scale, w.rough) = (1.0, 0.0);
                continue;
            }
            w.mu_scale = realism::grip_scale(w.surface_now, self.wetness);
            // the road under each tyre, eased from the old surface's over a metre
            let side = (w.attach.x > 0.0) as u8;
            let at = self.road_s + w.attach.y as f64;
            let now = realism::roughness(w.surface_now, at, side);
            w.rough = if w.surface_mix < 1.0 { realism::roughness(w.surface_was, at, side) * (1.0 - w.surface_mix) + now * w.surface_mix } else { now };
        }
        (force, torque)
    }

    /// Point every axle at the centre of the turn the steering asks for.
    fn steer_axles(&mut self, steer: f32) {
        // The steering sets the curvature the bus turns on (`steer` 1 = the full lock of
        // `[inv_min_turnradius]`) and every axle points at the centre of that turn on the
        // `[rot_pnt_long]` line, as OMSI does it. How fast the curvature may change is the
        // input's business (keys, mouse, wheel), not the axle's: the old fixed rate here
        // lagged every mouse and controller movement by up to 0.4 s.
        let kappa = steer.clamp(-1.0, 1.0) * self.inv_min_turn_radius;
        self.kappa = kappa;
        let front = self.wheels.iter().map(|w| w.attach.y).fold(f32::MIN, f32::max);
        for w in self.wheels.iter_mut() {
            // each tyre square to the line from the centre of the turn to itself - the inner
            // one turned further than the outer (Ackermann). With both at the axle's angle the
            // tyres of an axle pulled against each other through the side constraint: the
            // scrub ate the drive, 60 % steering left a twentieth of the push and full lock
            // none ("the engine revs but the bus gets slower the more I steer")
            let across = (1.0 - w.attach.x * kappa).max(0.2);
            w.steer = ((w.attach.y - self.rot_pnt_long) * kappa / across).atan().clamp(-1.05, 1.05);
            w.steered = w.steer != 0.0;
        }
        self.steer_deg = ((front - self.rot_pnt_long) * kappa).atan().clamp(-1.05, 1.05).to_degrees();
    }

    /// What each tyre finds this substep: the hub height it needs (with how steeply that
    /// rises and where), and the faces it cannot climb, which it keeps.
    fn read_ground(&mut self, tyre: &impl Fn(&RigidWheel) -> (Vec3, Vec3, Vec3, DVec3, DVec3), rot: Quat, h: f32, probe: &dyn Fn(f64, f64, f64) -> GroundProbe) -> Vec<Option<(f64, f32, f32)>> {
        let position = self.position;
        // What each tyre finds under and around it: the hub height it needs, how steeply
        // that rises as the wheel rolls on, and the faces it cannot climb. Steps up to
        // CLIMB radii over the ground it stood on are ground; the first step after
        // `place` (which knows no ground) takes anything up to the hub, and so does a
        // tyre that finds nothing lower, to lift a body set down a little too low.
        let mut found: Vec<Option<(f64, f32, f32)>> = Vec::with_capacity(self.wheels.len());
        for i in 0..self.wheels.len() {
            let w = &self.wheels[i];
            let (_, _, fwd_h, _, hub) = tyre(w);
            let r = w.radius;
            let z_top = if w.ground_seen { w.ground_z + (CLIMB * r) as f64 } else { hub.z + 0.05 };
            let wall_top = hub.z + WALL_HEIGHT;
            // faces are looked for as far ahead as the hub moves in a substep, so that
            // the first touch is already answered before the tyre is in the face
            let v_hub = self.velocity + rot.mul_vec3(self.omega).cross((hub - position).as_vec3());
            let look = Vec2::new(v_hub.dot(fwd_h).abs(), v_hub.dot(Vec3::new(fwd_h.y, -fwd_h.x, 0.0)).abs()) * h;
            let line = tyre_line(probe, hub, fwd_h, r, z_top, wall_top, look.x);
            let ground = match line.ground {
                None if w.ground_seen => tyre_line(probe, hub, fwd_h, r, hub.z + 0.05, wall_top, look.x).ground,
                g => g,
            };
            // across the tread: a kerb under part of it (see `tread_step`)
            let ground = ground.map(|(need, slope, x)| (need + tread_step(probe, hub, fwd_h, r, z_top), slope, x));
            let walls = if self.wheel_walls { track_walls(probe, &w.walls, &line, hub, fwd_h, r, z_top, wall_top, look.y) } else { Vec::new() };
            found.push(ground);
            let w = &mut self.wheels[i];
            w.walls = walls;
            w.step_force = 0.0;
        }
        found
    }

    /// Stop the hubs at the faces the tyres meet (see `step_slice`).
    fn stop_at_faces(&mut self, tyre: &impl Fn(&RigidWheel) -> (Vec3, Vec3, Vec3, DVec3, DVec3), rot: Quat, h: f32) {
        // A face stops the hub at once: whatever would close on it within the substep is
        // taken away by an impulse at the hub, as the body's own collisions do (and it is
        // a crash like theirs). A spring alone gave way - a front wheel met at 30 km/h
        // went through a platform's face, and once the hub was over the face nothing
        // pushed at all.
        for _ in 0..2 {
            for i in 0..self.wheels.len() {
                let (_, _, fwd_h, _, hub) = tyre(&self.wheels[i]);
                let r_hub = (hub - self.position).as_vec3();
                for f in 0..self.wheels[i].walls.len() {
                    let face = self.wheels[i].walls[f];
                    let n = face.normal.extend(0.0);
                    let reach = face.reach(fwd_h, self.wheels[i].radius);
                    let gap = face.distance(hub) - reach;
                    let v_hub = self.velocity + rot.mul_vec3(self.omega).cross(r_hub);
                    let closing = -v_hub.dot(n);
                    let excess = closing - gap.max(0.0) / h;
                    if excess <= 0.0 {
                        continue;
                    }
                    let j = excess / self.inv_mass_at(r_hub, n);
                    let before = self.kinetic_energy();
                    self.apply_impulse(r_hub, n * j);
                    let lost = (before - self.kinetic_energy()).max(0.0);
                    self.wheels[i].step_force += j / h * n.dot(fwd_h);
                    match self.wheel_impacts.iter_mut().find(|m| m.obstacle == i) {
                        Some(m) => {
                            m.speed = m.speed.max(closing);
                            m.energy += lost;
                        }
                        None => {
                            let point = rot.inverse().mul_vec3(r_hub - n * reach) + self.cog;
                            self.wheel_impacts.push(Impact { point, speed: closing, energy: lost, obstacle: i, broke: false, push: -n });
                        }
                    }
                }
            }
        }
    }

    /// The tyres' hold across, solved over the substep with everything else that pushes the
    /// body; returns the force and the torque with it.
    #[allow(clippy::too_many_arguments)]
    fn hold_across(&mut self, contacts: &[Contact], longs: &[f32], mut force: Vec3, mut torque: Vec3, rot: Quat, body_fwd: Vec3, towed: f32, h: f32) -> (Vec3, Vec3) {
        // Across the tyre: OMSI has no slip angle. Each axle takes away the sideways
        // speed it has - the bus follows its wheels exactly - with no more force than
        // the road's grip times the load on it; only beyond that does it slide
        // (0x7e4012: -0.5 x mass x side speed / frame per axle, capped at mu x load).
        // Here the same as a constraint on every tyre, solved over this substep with
        // everything else that pushes the body. The old tyre instead gave 12 x load per
        // radian of slip: every bus turned at 70 % of what its steering asked, 0.8 s
        // late, drifting 2-3 degrees and swaying - the same for every `.bus`.
        if self.realistic {
            return self.slip_across(contacts, longs, force, torque, rot, h);
        }
        let (m, mt) = (self.mass, self.mass + towed);
        let lin = |p: Vec3| (p - body_fwd * p.dot(body_fwd)) / m + body_fwd * (p.dot(body_fwd) / mt);
        let along = force.dot(body_fwd);
        let mut v = self.velocity + ((force - body_fwd * along) / m + body_fwd * (along / mt)) * h;
        let mut w = self.omega + (torque - self.omega.cross(self.inertia * self.omega)) / self.inertia * h;
        let lat: Vec<usize> = (0..contacts.len()).filter(|&k| contacts[k].lateral && contacts[k].grip > 0.0).collect();
        // (following its wheels: hardly any speed across the body)
        let gripped = self.velocity.dot(rot.mul_vec3(Vec3::X)).abs() < 0.5 && self.velocity.dot(body_fwd).abs() > 1.0;
        let arms: Vec<(Vec3, f32)> = lat
            .iter()
            .map(|&k| {
                let c = &contacts[k];
                let mut rn = rot.inverse().mul_vec3(c.r).cross(rot.inverse().mul_vec3(c.right));
                // (holding, no roll from the tyres' hold across: Omsi.exe then sets the
                // sideways motion outright and rolls the body by the bend's pull alone,
                // below - taken at the ground, every turn of the wheel kicked the body
                // over; sliding, the tyres trip it as before)
                if self.holding && gripped {
                    rn.y = 0.0;
                }
                let inv = c.right.dot(lin(c.right)) + (rn / self.inertia).dot(rn);
                (rn, inv.max(1e-9))
            })
            .collect();
        // holding: the bend's pull against the grip of all tyres (0x7e4dee), and
        // no wheel spinning or locked (0x7e3c3b)
        let grip_all: f32 = lat.iter().map(|&k| contacts[k].mu * contacts[k].grip).sum();
        let v_fwd = self.velocity.dot(body_fwd);
        let kappa = (self.steer_deg.to_radians().tan() / (self.wheels.iter().map(|w| w.attach.y).fold(f32::MIN, f32::max) - self.rot_pnt_long).abs().max(0.5)).abs();
        let slipping = self.wheels.iter().any(|w| w.slipping && w.on_ground);
        // (and a wheel in the air: Omsi.exe drops the holding state there, 0x7e4b13)
        let airborne = omsi_suspension() && self.wheels.iter().any(|w| !w.on_ground);
        if self.holding && (m * v_fwd * v_fwd * kappa > grip_all || slipping || airborne) {
            self.holding = false;
        }
        // (holding, a tyre may pass several times its grip for the moment it takes
        // the body to follow a quick turn of the wheel: OMSI sets the yaw rate outright)
        let reach = if self.holding { 8.0 } else { 1.0 };
        let mut j = vec![0.0f32; lat.len()];
        for _ in 0..8 {
            for (q, &k) in lat.iter().enumerate() {
                let c = &contacts[k];
                let v_lat = (v + rot.mul_vec3(w).cross(c.r)).dot(c.right);
                let cap = c.mu * c.grip * h * reach;
                let before = j[q];
                j[q] = (before - v_lat / arms[q].1).clamp(-cap, cap);
                let dj = j[q] - before;
                v += lin(c.right) * dj;
                w += arms[q].0 / self.inertia * dj;
            }
        }
        for (q, &k) in lat.iter().enumerate() {
            force += contacts[k].right * (j[q] / h);
            torque += arms[q].0 * (j[q] / h);
        }
        if !self.holding && !slipping && lat.iter().all(|&k| (v + rot.mul_vec3(w).cross(contacts[k].r)).dot(contacts[k].right).abs() < 0.1) {
            self.holding = true;
        }
        // the bend's pull rolls the body out of it (0x7e4629: [schwerpunkt] x mass x
        // the centripetal acceleration)
        if self.holding && gripped && !lat.is_empty() {
            torque.y += m * self.cog.z * v_fwd * self.omega.z;
        }
        (force, torque)
    }

    /// The tyres' side forces from their slip, as a real tyre makes them (a stretched-string
    /// model after Pacejka's and von Schlippe's): the contact patch is held by the road while
    /// the wheel moves on, so it lags behind it by `y` metres, and the tyre pulls back with
    /// `Cα/σ · y` (the cornering stiffness over the relaxation length) - at speed that is the
    /// force of a slip angle `y/σ` (`Cα` per radian), at a standstill the spring of a tyre
    /// that holds the vehicle on a slope. The patch lets go beyond what the road gives: the
    /// grip `μ N` shared with the longitudinal force by the friction ellipse (a locked or
    /// spinning wheel has none left for the side), the deflection then stays where that
    /// force is reached and the tyre slides.
    ///
    /// This is why a bus understeers as it speeds up, turns in over a second at 90 km/h and
    /// not at once, lets its tail out when the rear tyres are overloaded, and why a braking
    /// wheel loses its hold across: none of it follows from OMSI's rule that the body follows
    /// its wheels exactly until the grip is passed. Realistic road physics off gives that back.
    fn slip_across(&mut self, contacts: &[Contact], longs: &[f32], mut force: Vec3, mut torque: Vec3, rot: Quat, h: f32) -> (Vec3, Vec3) {
        let inv = rot.inverse();
        let omega_world = rot.mul_vec3(self.omega);
        let n = contacts.len().max(1) as f32;
        // a tyre in the air or off the ground has no patch: it starts afresh when it lands
        for (i, w) in self.wheels.iter_mut().enumerate() {
            if !contacts.iter().any(|c| c.wheel == Some(i) && c.lateral && c.grip > 0.0) {
                w.lat_defl = 0.0;
            }
        }
        for (k, c) in contacts.iter().enumerate() {
            let Some(i) = c.wheel.filter(|_| c.lateral && c.grip > 0.0) else { continue };
            let v_pt = self.velocity + omega_world.cross(c.r);
            let (v_lat, v_lng) = (v_pt.dot(c.right), v_pt.dot(c.fwd));
            let w = &mut self.wheels[i];
            // the tyre's stiffness across: Cα = tyre::CORNERING x load, less than in proportion to the
            // load (a tyre loaded far beyond what it was built for gives less per newton)
            let n_tyre = c.grip;
            let rated = w.max_force.max(1.0);
            let c_alpha = tyre::CORNERING * n_tyre * (rated / n_tyre.max(0.2 * rated)).powf(0.15).min(1.5);
            let sigma = tyre::RELAXATION * w.radius.max(0.2);
            let k_y = c_alpha / sigma;
            // damping: critical for the vehicle's share at this tyre, at a standstill only
            // (at speed the lag in the deflection damps it)
            let c_y = 2.0 * 0.35 * (k_y * self.mass / n).sqrt();
            // the patch is carried along by the road and dragged by the wheel
            let rate = v_lat - v_lng.abs() / sigma * w.lat_defl;
            w.lat_defl += rate * h;
            let grip = c.mu * n_tyre;
            // the friction ellipse: what the longitudinal force takes the side cannot
            let lng = (longs.get(k).copied().unwrap_or(0.0) / grip.max(1.0)).clamp(-1.0, 1.0);
            let cap = grip * (1.0 - lng * lng).max(0.0).sqrt();
            let damping = if v_lng.abs() < 3.0 { c_y * rate * (1.0 - v_lng.abs() / 3.0) } else { 0.0 };
            let mut f = -(k_y * w.lat_defl + damping);
            if f.abs() > cap {
                f = f.signum() * cap;
                // the patch slides: it holds the deflection of the force it can give
                w.lat_defl = -f / k_y;
            }
            let f_world = c.right * f;
            force += f_world;
            torque += inv.mul_vec3(c.r).cross(inv.mul_vec3(f_world));
        }
        (force, torque)
    }

    /// After the substeps: a braked body stands truly still and never lies on its side.
    fn settle(&mut self, drive_torque: f32, brake: &[f32]) {
        // Asleep: with the brakes holding and no drive, the last few millimetres per
        // second of horizontal drift are noise from the tyre model, not motion. Take them
        // away so the bus stands truly still (the vertical velocity stays: the suspension
        // may still be settling).
        let braked: f32 = brake.iter().sum();
        let horizontal = Vec3::new(self.velocity.x, self.velocity.y, 0.0).length();
        if horizontal < 0.03 && drive_torque.abs() < 1.0 && braked > 0.02 * self.mass * 9.81 {
            self.velocity.x = 0.0;
            self.velocity.y = 0.0;
            self.omega.z = 0.0;
        }
        // rotation keeps the body from tipping over completely (safety net for bad ground)
        let up = self.orientation.mul_vec3(Vec3::Z);
        if up.z < 0.3 {
            let (heading, _, _) = self.heading_pitch_bank();
            self.orientation = Quat::from_rotation_z((-heading).to_radians() as f32);
            self.omega = Vec3::ZERO;
        }
    }

    /// Keep the body's `[boundingbox]` out of the obstacles and answer each hit with an
    /// impulse at the point of contact: the closing speed goes (a fifth of it comes back),
    /// sliding along the obstacle is braked by friction, and an off-centre blow turns the
    /// body. Nothing is frozen - the vehicle can drive off or reverse at once.
    ///
    /// `skip(i)`: obstacles already broken off. `dt` is the frame, to tell a hit from a box
    /// the vehicle has been standing in all along (spawned in a depot hall's box): that one
    /// is deeper than any frame of motion could have taken it, and never pushes.
    ///
    /// Each obstacle is answered once per call (up to four of them, deepest first). A car
    /// that drives into the bus is still there after its answer - the bus only steps a
    /// millimetre out of its way and takes none of its speed - and was taken as the same
    /// crash four times over.
    pub fn collide(&mut self, bb: [f32; 6], obstacles: &[Obb], skip: &dyn Fn(usize) -> bool, dt: f32) -> Vec<Impact> {
        let mut impacts: Vec<Impact> = Vec::new();
        let mut answered: Vec<usize> = Vec::new();
        {
            let me = self.body_box(bb);
            let touching: Vec<i64> = obstacles.iter().enumerate().filter(|(i, o)| o.id >= 0 && o.mass <= 0.0 && !skip(*i) && me.contact(o).is_some()).map(|(_, o)| o.id).collect();
            match self.spawned_inside.as_mut() {
                None => self.spawned_inside = Some(touching),
                // (once out of one, it counts again)
                Some(list) => list.retain(|id| touching.contains(id)),
            }
        }
        let inside = self.spawned_inside.clone().unwrap_or_default();
        for _ in 0..4 {
            let me = self.body_box(bb);
            let mut deepest: Option<(usize, crate::collision::Contact)> = None;
            for (i, o) in obstacles.iter().enumerate() {
                if skip(i) || answered.contains(&i) || (o.id >= 0 && inside.contains(&o.id)) {
                    continue;
                }
                let Some(c) = me.contact(o) else { continue };
                let rel = (glam::DVec2::new(self.velocity.x as f64, self.velocity.y as f64) - o.velocity).length() + self.omega.z.abs() as f64 * 6.0;
                if c.depth > 0.5 + rel * dt as f64 * 2.0 {
                    continue;
                }
                if deepest.map(|d| c.depth > d.1.depth).unwrap_or(true) {
                    deepest = Some((i, c));
                }
            }
            let Some((i, c)) = deepest else { break };
            answered.push(i);
            let o = &obstacles[i];
            let n = Vec3::new(c.normal.x as f32, c.normal.y as f32, 0.0);
            // A wall as tall as the vehicle stops it at the height of its centre of gravity (a
            // real front is stiffest down at the frame; the middle of a double-decker's face
            // tipped it onto its rear wheels); a low bollard or a high beam acts where it is.
            let z = self.position.z.clamp(c.z0, c.z1.max(c.z0));
            let r = (DVec3::new(c.point.x, c.point.y, z) - self.position).as_vec3();
            // A moving obstacle (an AI car) has a mass of its own, which shares the impulse and
            // the way out of the overlap. But the AI moves it by itself and never gives up its
            // speed: answered with its full approach, it came back every frame and shoved the
            // bus across the road at its own pace until the bus nearly rolled over. So it blocks
            // the bus's motion into it; its own run into the bus counts as the crash it is, and
            // pushes nothing.
            let v_other = Vec3::new(o.velocity.x as f32, o.velocity.y as f32, 0.0);
            let v_body = self.velocity + self.orientation.mul_vec3(self.omega).cross(r);
            let vn_full = (v_body - v_other).dot(n);
            let v_c = v_body - (v_other - n * v_other.dot(n).max(0.0));
            let vn = v_c.dot(n);
            let inv_other = if o.mass > 0.0 { 1.0 / o.mass } else { 0.0 };
            let k_body = self.inv_mass_at(r, n);
            let k_n = k_body + inv_other;
            let before = self.kinetic_energy();
            // The scripts are told where the bodies meet, low down where the bumpers are
            // (the stock buses damage their rear engine only below 1.10 m), not where the
            // blow is taken: at the centre of gravity every wall and car read 1.2-1.3 m.
            let seen = DVec3::new(c.point.x, c.point.y, crate::collision::impact_height(c.z0, c.z1));
            let point = self.orientation.inverse().mul_vec3((seen - self.position).as_vec3()) + self.cog;
            if let Some((mass_t, load_kn)) = o.pole.filter(|_| vn < 0.0) {
                // A post breaks at its foot when stopping the vehicle would take more than it
                // can bear; the vehicle then only loses what it takes to throw the post.
                let stop = -vn / k_n / dt.max(1e-3);
                if -vn > CRASH_SPEED && stop > load_kn.max(0.1) * 1000.0 {
                    let m_pole = (mass_t * 1000.0).max(5.0);
                    let j = -(1.0 + RESTITUTION) * vn / (k_n + 1.0 / m_pole);
                    self.apply_impulse(r, n * j);
                    impacts.push(Impact { point, speed: -vn, energy: (before - self.kinetic_energy()).max(0.0), obstacle: i, broke: true, push: -n });
                    continue;
                }
            }
            // (a car leaving the body fast goes out of it by itself)
            if o.mass > 0.0 && vn_full > CRASH_SPEED {
                continue;
            }
            // out of the obstacle along the shortest way, a millimetre clear - out of a moving
            // one that comes on only as far as the body itself ran into it this frame: a
            // standing car is not ploughed through, and one that drives into the bus does not
            // drag it along (not even creeping, below the crash speed). Out of a car that
            // stands or draws away the bus comes all the way: it no longer hangs in it.
            let approaching = o.mass > 0.0 && v_other.dot(n) > 0.0;
            let out = if approaching { (c.depth as f32).min((-vn).max(0.0) * dt + 0.001) } else { c.depth as f32 + 0.001 };
            self.position += (n * out).as_dvec3();
            if vn_full >= 0.0 {
                continue;
            }
            // a gentle push is dead, a real blow gives a little back
            let e = if -vn_full > 1.0 { RESTITUTION } else { 0.0 };
            let j = -(1.0 + e) * vn.min(0.0) / k_n;
            let mut p = n * j;
            let vt = v_c - n * vn;
            let vt = Vec3::new(vt.x, vt.y, 0.0);
            if vt.length() > 1e-3 {
                let t = vt.normalize();
                let jt = (vt.length() / (self.inv_mass_at(r, t) + inv_other)).min(SCRAPE_FRICTION * j);
                p -= t * jt;
            }
            self.apply_impulse(r, p);
            // what the crash destroyed: the closing motion of the two, less what bounced back
            // (the bus alone may even gain energy when the other one does the hitting)
            let energy = (before - self.kinetic_energy()).max(0.5 * vn_full * vn_full * (1.0 - e * e) / k_n);
            impacts.push(Impact { point, speed: -vn_full, energy, obstacle: i, broke: false, push: -n });
        }
        // a blow the tyres have to catch: they slide until the body is back on its wheels
        if impacts.iter().any(|m| m.speed >= CRASH_SPEED) {
            self.holding = false;
        }
        impacts
    }
}

/// Omsi.exe's strut for one wheel (see `RigidBody::step_slice`): returns the tyre's force,
/// its static part and the strut's force.
#[allow(clippy::too_many_arguments)]
fn omsi_strut(w: &mut RigidWheel, r: f32, hub0: DVec3, position: DVec3, cog: Vec3, body_cog: Vec3, rot: Quat, up: Vec3, velocity: Vec3, omega_world: Vec3, h: f32, probe: &dyn Fn(f64, f64, f64) -> GroundProbe) -> (f32, f32, f32) {
    // Omsi.exe's suspension (0x7e47aa..0x7e4de8): the body hangs straight on
    // the ground point under each wheel. The spring pushes by how far the
    // ground lies over the wheel's unloaded place, times `Axle_Springfactor`
    // and `achse_feder`; below it the wheel is in the air, pushes nothing and
    // the tyres stop holding (0x7e4b13). Pressed, the damper takes
    // `achse_daempfer` times the body's upward speed where the strut acts
    // (the roll rate at `achse_maxwidth` / 2, 0x7e4899), and the whole never
    // passes `achse_maxforce` (0x7e4b71) - there is no wheel of its own mass,
    // no tyre and no bump stop between the road and the body. (Those, with a
    // tyre envelope smoothing the road, had the bus float over what it drove
    // on: "like a boat on the sea".)
    let k = w.spring * w.spring_factor.max(0.0);
    let top = hub0.z + (CLIMB * r) as f64;
    // (nothing under it: Omsi.exe's ground query looks from 3 m over the model
    // origin's plane, 0x7a0985 - a wheel that sank through a face at a joint of
    // two surfaces, or into a bridge deck, found nothing within reach, the
    // spring let go and the bus fell through the world)
    // The spring's point is where Omsi.exe puts it (0x7e47aa): on the model's
    // origin plane (z = 0) under the wheel, and its height over the ground is
    // measured straight up. Measured from the hub less the `.bus` file's tyre
    // radius instead, a mod whose tyre mesh is larger than that radius stood
    // with its wheels drawn sunk a few centimetres into the road.
    let plane0 = position + rot.mul_vec3(Vec3::new(w.attach.x, w.attach.y, 0.0) - cog).as_dvec3();
    let under = probe(plane0.x, plane0.y, top).below.or_else(|| probe(plane0.x, plane0.y, plane0.z + 3.0).below);
    // (the road's own unevenness lifts the ground under the tyre; the ground itself stays
    // what it is for everything that asks where it lies)
    let t = under.map(|g| (g - plane0.z) as f32 + w.rough);
    if let Some(g) = under {
        w.ground_z = g;
        w.ground_seen = true;
    }
    let r_out = rot.mul_vec3(w.attach - body_cog + Vec3::new(w.lever, 0.0, 0.0));
    let v_up = (velocity + omega_world.cross(r_out)).dot(up);
    let mut n = 0.0f32;
    if let Some(t) = t {
        let spring = k * t;
        if spring >= 0.0 {
            n = (spring - w.damper * v_up).min(w.max_force);
        }
    }
    let travel = t.unwrap_or(-DROOP);
    let full = if k > 1.0 { w.max_force / k } else { BUMP };
    w.compression_rate = if w.touch.is_some() { (travel.clamp(-DROOP, full) - w.compression) / h } else { 0.0 };
    w.compression = travel.clamp(-DROOP, full);
    w.touch = t;
    let on = t.is_some_and(|t| k * t >= 0.0);
    w.on_ground = on;
    w.load = n.max(0.0);
    (if on { n.max(0.0) } else { 0.0 }, if on { n.max(0.0) } else { 0.0 }, n)
}
