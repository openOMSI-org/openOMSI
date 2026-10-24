//! The driving physics: the scripts' torque and brakes into the simple or the rigid-body model,
//! and the wheels, springs and contact back into the scripts' variables.

use super::*;

/// Scripts often test a stopped bus with `!Velocity_Ground`, so do not expose tiny solver drift.
pub(super) fn script_speed(speed_kmh: f32) -> f32 {
    if speed_kmh.abs() < 0.01 { 0.0 } else { speed_kmh }
}

/// The speed the driven wheels' turning gives (m/s): the mean of their rims' speed, each
/// wheel's spin (rad/s) times its radius - Omsi.exe's `Velocity` (0x7e2574 sums each driven
/// axle's two wheel speeds times its size, averages them over the driven wheels and turns
/// that into km/h), while its `Velocity_Ground` is the body's speed. They part when a wheel
/// spins or locks. `None` for a vehicle without driven wheels: then OMSI's `Velocity` is the
/// body's speed too.
pub(super) fn driven_wheels_speed(wheels: impl Iterator<Item = (f32, f32, bool)>) -> Option<f32> {
    let (sum, n) = wheels
        .filter(|w| w.2)
        .fold((0.0f32, 0u32), |(sum, n), (spin, radius, _)| (sum + spin * radius, n + 1));
    (n > 0).then(|| sum / n as f32)
}

impl VehicleInstance {
    pub fn set_controls(&mut self, c: Controls) {
        self.physics.controls = c;
    }

    /// Physics step: read the script's torque/brake outputs, integrate, write the motion
    /// variables back for the next script frame.
    /// Switch the player vehicle to rigid-body dynamics at its current pose.
    pub fn enable_rigid_body(&mut self) {
        let mut rb =
            crate::rigid::RigidBody::from_definition(&self.ty.def, &self.ty.hub_heights(0));
        for (i, w) in rb.wheels.iter_mut().enumerate() {
            w.spring_factor = self.spring_factor(i / 2, i % 2);
        }
        rb.place(self.position, self.heading);
        self.rigid = Some(rb);
    }

    /// `Axle_Springfactor_<axle>_<side>` as the scripts left it.
    /// The axle that steers: the frontmost, as the rigid body takes it (the simple model
    /// took the first listed, which is not always the front one).
    pub(super) fn steered_axle(&self) -> usize {
        self.ty.def.axles.iter().enumerate().max_by(|a, b| a.1.long.total_cmp(&b.1.long)).map(|(i, _)| i).unwrap_or(0)
    }

    pub(super) fn spring_factor(&self, axle: usize, side: usize) -> f32 {
        self.v_springfactor
            .get(axle)
            .and_then(|a| a[side])
            .map(|i| self.state.vars[i as usize])
            .unwrap_or(1.0)
    }

    /// Set the forward speed (m/s), for tests that need a run-up without the road.
    pub fn set_speed(&mut self, v: f32) {
        self.physics.speed = v;
        if let Some(rb) = self.rigid.as_mut() {
            rb.velocity = rb.orientation.mul_vec3(Vec3::Y) * v;
        }
    }

    pub(super) fn step_physics(&mut self, dt: f32) {
        self.step_trolley(dt);
        let c = self.physics.controls;
        self.put(self.v_throttle, c.throttle);
        self.put(self.v_brake, c.brake);
        self.put(self.v_clutch, c.clutch);
        let m_wheel = self.trolley_torque(self.get(self.v_m_wheel));
        // each wheel: its own brake force, and its share of the vehicle's (the rolling
        // resistance's share is the physics' own)
        let n = (self.v_wheels.len() * 2).max(1) as f32;
        let shared = self.get(self.v_brakeforce).max(0.0) / n;
        let brakes: Vec<f32> = self
            .v_wheels
            .iter()
            .flat_map(|a| a.iter().map(|w| self.get(w[4]).max(0.0) + shared))
            .collect();
        // Read, the brake forces go back to 0, as Omsi.exe clears them every frame before
        // the scripts run (0x7e58d9..0x7e5930): a script sets them each frame it brakes. Kept,
        // a mod that brakes only inside an {if} (a retarder, a stop brake, its own physics)
        // stayed braked for good, and one that adds to its own value kept on growing.
        self.put(self.v_brakeforce, 0.0);
        // (an axle's ids are copied out one by one: the whole list was copied every frame)
        for a in 0..self.v_wheels.len() {
            for w in self.v_wheels[a] {
                self.put(w[4], 0.0);
            }
        }
        if self.rigid.is_some() {
            self.step_rigid(dt, m_wheel, &brakes, c.steering);
            return;
        }
        let v0 = self.physics.speed;
        let (mut ds, dheading) = self.physics.step(dt, m_wheel, &brakes);
        // a rail vehicle's speed is the rails', not the road tyres'
        if let Some(covered) = self.rail_override_simple(dt, v0, m_wheel, &brakes) {
            ds = covered;
        }
        // move along the heading
        let prev = (self.position, self.heading);
        let h = self.heading.to_radians();
        self.position.x += h.sin() * ds as f64;
        self.position.y += h.cos() * ds as f64;
        self.heading = (self.heading + dheading as f64).rem_euclid(360.0);
        // collisions: back out of obstacles and stop
        if let Some(bb) = self.ty.def.bounding_box {
            let obb = crate::collision::Obb::from_box(bb, self.position, self.body_heading());
            // an obstacle we were already inside before this step (spawned on it, pushed into
            // it) never blocks: only entering an obstacle does
            let prev_obb = crate::collision::Obb::from_box(
                bb,
                prev.0,
                body_heading(&self.ty.def, prev.1, false),
            );
            let mut obstacles = self
                .collision
                .as_ref()
                .map(|cw| cw.obstacles_near(&obb))
                .unwrap_or_default();
            obstacles.extend(self.dynamic_boxes.iter().copied());
            let hit = obstacles.iter().copied().find_map(|obstacle| {
                let was_overlapping = if obstacle.mass > 0.0 {
                    let mut previous = obstacle;
                    previous.center -= obstacle.velocity * dt as f64;
                    previous.overlaps(&prev_obb)
                } else {
                    obstacle.overlaps(&prev_obb)
                };
                if !obstacle.overlaps(&obb)
                    || (was_overlapping && obstacle.mass <= 0.0)
                    || (obstacle.id >= 0 && self.knocked.contains(&obstacle.id))
                {
                    return None;
                }
                obb.contact(&obstacle)
                    .map(|contact| (obstacle, contact, was_overlapping))
            });
            if let Some((hit, contact, was_overlapping)) = hit {
                let body_h = body_heading(&self.ty.def, prev.1, false).to_radians();
                let own_velocity =
                    glam::DVec2::new(body_h.sin(), body_h.cos()) * self.physics.speed as f64;
                let relative_velocity = own_velocity - hit.velocity;
                let closing_speed = -relative_velocity.dot(contact.normal);
                let speed = if hit.mass > 0.0 {
                    closing_speed.max(0.0) as f32
                } else {
                    relative_velocity.length() as f32
                };
                // out of a moving car that comes on only as far as the bus itself ran into
                // it this step (as the rigid model has it): a car creeping into the standing
                // bus does not shove it along; out of one standing or drawing away all the way
                let approaching = hit.mass > 0.0 && hit.velocity.dot(contact.normal) > 0.0;
                let out = if approaching {
                    contact.depth.min((-own_velocity.dot(contact.normal)).max(0.0) * dt as f64 + 0.001)
                } else {
                    contact.depth + 0.001
                };
                if hit.mass <= 0.0 || closing_speed >= -(crate::rigid::CRASH_SPEED as f64) {
                    if was_overlapping {
                        self.position += contact.normal.extend(0.0) * out;
                        if speed > crate::rigid::CRASH_SPEED {
                            self.physics.speed = 0.0;
                        }
                    } else if speed <= crate::rigid::CRASH_SPEED {
                        self.position += contact.normal.extend(0.0) * out;
                    } else {
                        // what the blow takes: the bus's motion into a wall, and between two
                        // vehicles what the pair loses, 0.5 v² (1 - e²) over their inverse
                        // mass, as the rigid model counts it: a 1 t car running into the
                        // standing bus at 10 m/s is some 44 kJ, not the 500 kJ of the bus
                        // itself at that speed
                        let e = if hit.mass > 0.0 {
                            let back = if speed > 1.0 { crate::rigid::RESTITUTION } else { 0.0 };
                            0.5 * speed * speed * (1.0 - back * back) / (1.0 / self.physics.mass_kg + 1.0 / hit.mass)
                        } else {
                            0.5 * self.physics.mass_kg * speed * speed
                        };
                        let point = glam::DVec3::new(
                            contact.point.x,
                            contact.point.y,
                            crate::collision::impact_height(contact.z0, contact.z1),
                        );
                        let rel = point.truncate() - self.position.truncate();
                        let (sh, ch) = (body_h.sin(), body_h.cos());
                        let coll_pos = [
                            (rel.x * ch - rel.y * sh) as f32,
                            (rel.x * sh + rel.y * ch) as f32,
                            (point.z - self.position.z) as f32,
                        ];
                        self.host.coll_pos = coll_pos;
                        // kJ, like the rigid model reports it
                        self.host.coll_energy += e / 1000.0;
                        self.last_crash += e;
                        self.crashes += 1;
                        self.last_impact = e;
                        self.collided = true;
                        self.break_glass_for_impact(Vec3::from_array(coll_pos), speed, e);
                        if hit.mass > 0.0 && hit.id <= -2 && e >= 1000.0 {
                            self.dynamic_impacts.push(DynamicImpact {
                                obstacle_id: hit.id,
                                point,
                                push: (-contact.normal).extend(0.0),
                                speed,
                                energy: e,
                                mass: self.physics.mass_kg,
                            });
                        }
                        self.position = prev.0;
                        self.heading = prev.1;
                        self.physics.speed = 0.0;
                    }
                }
            }
        }
        // ground under each wheel: height, terrain pitch and bank of the body
        let (mut z_sum, mut n_z) = (0.0, 0);
        let (mut zf, mut zr, mut zl, mut zrr, mut nf, mut nr, mut nl, mut nrr) =
            (0.0, 0.0, 0.0, 0.0, 0, 0, 0, 0);
        let (sh, ch) = (h.sin(), h.cos());
        let wheelbase = self.physics.wheelbase as f64;
        let mut track = 2.0f64;
        if let Some(g) = &self.ground {
            for axle in &self.physics.wheels {
                for w in axle {
                    let (lat, long) = (w.lat as f64, w.long as f64);
                    track = track.max(w.lat.abs() as f64 * 2.0);
                    let wx = self.position.x + lat * ch + long * sh;
                    let wy = self.position.y - lat * sh + long * ch;
                    if let Some(z) = g(wx, wy) {
                        z_sum += z;
                        n_z += 1;
                        if long > 0.0 {
                            zf += z;
                            nf += 1;
                        } else {
                            zr += z;
                            nr += 1;
                        }
                        if lat < 0.0 {
                            zl += z;
                            nl += 1;
                        } else {
                            zrr += z;
                            nrr += 1;
                        }
                    }
                }
            }
            // the body sits down on its springs like the rigid one does
            let n_axles = self.rest_sag.len().max(1) as f32;
            let lift = self.rest_sag.iter().map(|(c, off)| off - c).sum::<f32>() / n_axles;
            if n_z > 0 {
                self.position.z = z_sum / n_z as f64 + lift as f64;
            } else if let Some(z) = g(self.position.x, self.position.y) {
                self.position.z = z + lift as f64;
            }
        }
        let pitch_terrain = if nf > 0 && nr > 0 {
            ((zf / nf as f64 - zr / nr as f64) / wheelbase)
                .atan()
                .to_degrees()
        } else {
            0.0
        };
        let bank_terrain = if nl > 0 && nrr > 0 {
            ((zl / nl as f64 - zrr / nrr as f64) / track)
                .atan()
                .to_degrees()
        } else {
            0.0
        };
        // dynamic body motion: nose dives when braking, leans out of curves
        let a_long = self.physics.accel.y;
        let curvature = self.physics.steer_deg.to_radians().tan() / self.physics.wheelbase;
        let a_lat = self.physics.speed * self.physics.speed * curvature;
        let pitch_target = pitch_terrain as f32 - a_long * 0.45;
        let bank_target = bank_terrain as f32 - a_lat * 0.6;
        let k = (dt / 0.35).min(1.0);
        self.pitch += (pitch_target - self.pitch) * k;
        self.bank += (bank_target - self.bank) * k;
        // suspension travel per wheel (m): weight transfer from the same accelerations
        for (ai, axle) in self.physics.wheels.iter_mut().enumerate() {
            let rest = self.rest_sag.get(ai).map(|r| r.0).unwrap_or(0.0);
            for w in axle.iter_mut() {
                let front = if w.long > 0.0 { 1.0 } else { -1.0 };
                let right = if w.lat > 0.0 { 1.0 } else { -1.0 };
                // positive = the wheel drops below the body (see step_rigid): braking
                // compresses the front, so its wheels go up (negative)
                let target = -rest + a_long * 0.008 * front - a_lat * 0.01 * right;
                w.suspension += (target - w.suspension) * k;
            }
        }
        let v = script_speed(self.physics.velocity_kmh());
        self.put(self.v_velocity, v);
        self.put(self.v_velocity_ground, v);
        let n_wheel = self
            .physics
            .wheels
            .iter()
            .find(|w| w[0].driven)
            .or(self.physics.wheels.first())
            .map(|w| w[0].rpm)
            .unwrap_or(0.0);
        self.put(self.v_n_wheel, n_wheel);
        let a = self.physics.accel;
        self.physics.a_trans = a;
        self.put(self.v_accel[0], a.x);
        self.put(self.v_accel[1], a.y);
        self.put(self.v_accel[2], a.z);
        // Wheel_Rotation_* and Axle_Steering_* are radians in the original: the stock
        // model.cfg turns a wheel with anim_rot ... 57.29577951308232 (180/pi) and the
        // steering wheel with 1450 (SD200) or 1680 (SD202), roughly two and a half turns
        // from lock to lock. Positive is to the right, as here - checked by looking
        // straight down on the front wheel alone (OMSI_ONLY_MESH=SD_Rad_VL) and at the
        // steering wheel in the cab, which turn together with this sign.
        let steer = self.physics.steer_deg.to_radians();
        for ai in 0..self.v_wheels.len() {
            let axle = self.v_wheels[ai];
            for (si, w) in axle.iter().enumerate() {
                let ws = self.physics.wheels[ai][si].clone();
                self.put(w[0], ws.rotation_deg.to_radians());
                self.put(w[1], ws.rpm);
                if !self.ty.def.is_rail() {
                    self.put(w[2], if ai == self.steered_axle() { steer } else { 0.0 });
                }
                self.put(w[3], ws.suspension);
            }
        }
    }

    /// The coupled parts as the leading body feels them at its rear coupling (each along
    /// the direction of the part hung on that coupling; the parts follow kinematically and
    /// pass on what acts on them): drive, brakes, rolling resistance and mass.
    pub(super) fn coupled_parts(&self) -> Vec<crate::rigid::CoupledPart> {
        let Some(first) = self.trailers.first() else {
            return Vec::new();
        };
        // everything behind comes through the first coupling (the chain passes it on), but
        // each part pulls or holds back along its own heading: the third section of a
        // double articulated bus in a bend does not point where the second does
        let dir_of = |t: &TrailerPart| {
            let h = if t.pivot.is_some() { t.heading } else { self.heading }.to_radians();
            Vec3::new(h.sin() as f32, h.cos() as f32, 0.0)
        };
        self.trailers
            .iter()
            .map(|t| {
                let dir = dir_of(t);
                let def = &t.ty.def;
                let driven: Vec<usize> = def
                    .axles
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.driven)
                    .map(|(i, _)| i)
                    .collect();
                let wheel_load = |a: usize| t.rest.get(a).map(|r| r.1).unwrap_or(0.0) * 2.0;
                crate::rigid::CoupledPart {
                    point: first.coupling_back,
                    dir,
                    mass: if def.mass < 100.0 {
                        def.mass * 1000.0
                    } else {
                        def.mass
                    }
                    .max(0.0),
                    driven_wheels: driven.len() * 2,
                    radius: driven
                        .first()
                        .map(|&a| (def.axles[a].wheel_diameter / 2.0).max(0.15))
                        .unwrap_or(0.5),
                    driven_load: driven.iter().map(|&a| wheel_load(a)).sum(),
                    brake: t
                        .v_brakes
                        .iter()
                        .map(|&id| self.get(id).max(0.0))
                        .sum::<f32>()
                        + def.rolling_resistance.max(0.0),
                    load: (0..def.axles.len()).map(wheel_load).sum(),
                    joint_load: t.joint_load,
                }
            })
            .collect()
    }

    pub(super) fn step_rigid(&mut self, dt: f32, m_wheel: f32, brakes: &[f32], steer: f32) {
        let mut rb = self.rigid.take().unwrap();
        for (i, w) in rb.wheels.iter_mut().enumerate() {
            w.spring_factor = self.spring_factor(i / 2, i % 2);
            // what each tyre rolls on (the map tells the scripts, and so us, every frame)
            let side = usize::from(w.attach.x >= 0.0);
            if let Some(k) = self.v_surface.get(rb.wheel_axle.get(i).copied().unwrap_or(0)).and_then(|a| a[side]) {
                w.surface = self.state.vars[k as usize].clamp(0.0, 255.0) as u8;
            }
        }
        // and the weather it rolls in: the road's wetness and the air's density and motion
        rb.wetness = self.host.street_cond;
        rb.air_density = crate::rigid::realism::air_density(self.host.temperature, 0.0);
        rb.wind = self.host.wind;
        rb.coupled = self.coupled_parts();
        let v0 = rb.forward_speed();
        {
            let ground = self.ground.clone();
            let flat = |_: f64, _: f64, _: f64| crate::rigid::GroundProbe {
                below: Some(0.0),
                above: None,
            };
            let from_ground = |x: f64, y: f64, top: f64| {
                let z = ground.as_ref().and_then(|g| g(x, y));
                match z {
                    Some(z) if z > top => crate::rigid::GroundProbe {
                        below: None,
                        above: Some(z),
                    },
                    z => crate::rigid::GroundProbe {
                        below: z,
                        above: None,
                    },
                }
            };
            // one session for the whole step: the tyres ask for a few hundred points close
            // together, and it looks the tiles under them up once
            let session = self.contact.as_ref().map(|c| c.session());
            let probe: &dyn Fn(f64, f64, f64) -> crate::rigid::GroundProbe =
                match (&session, &self.ground) {
                    (Some(s), _) => s.as_ref(),
                    (None, Some(_)) => &from_ground,
                    (None, None) => &flat,
                };
            rb.wheel_walls = self.wheel_walls;
            rb.step(dt, m_wheel, brakes, steer, probe);
        }
        // a rail vehicle's speed is the rails', not the road tyres'
        self.rail_override(&mut rb, dt, v0, m_wheel, brakes);
        // what the crashes of this step destroyed, and the worst of them
        let mut energy = 0.0f32;
        let mut worst: Option<crate::rigid::Impact> = None;
        // a wheel stopped by a face it cannot climb (a platform, a step in the road)
        for hit in &rb.wheel_impacts {
            if hit.speed < crate::rigid::CRASH_SPEED || hit.energy < 1000.0 {
                continue;
            }
            energy += hit.energy;
            if worst.map(|w| hit.energy > w.energy).unwrap_or(true) {
                worst = Some(*hit);
            }
        }
        // Obstacles: the static boxes around the vehicle and the moving ones, answered with
        // impulses. The old answer - put the bus back where it was and take its speed - held
        // it against whatever it touched: it could not slide along a wall or turn away, and
        // every frame of pushing counted as another crash.
        if let Some(bb) = self.ty.def.bounding_box {
            let me = rb.body_box(bb);
            let mut obstacles: Vec<crate::collision::Obb> = Vec::new();
            if let Some(cw) = &self.collision {
                let mut probe_box = me;
                probe_box.half += glam::DVec2::splat(1.0);
                // the faces of collision meshes only where they are in the body as it
                // stands: pitched on a ramp, the plan-view box reaches down to its lowest
                // corner all along its length and ran into the ramp it drove on
                let solid = rb.body_solid(bb, 0.05);
                obstacles.extend(cw.obstacles_near_solid(&probe_box, Some(&solid)));
            }
            obstacles.extend(self.dynamic_boxes.iter().copied());
            let knocked = self.knocked.clone();
            let impacts = rb.collide(
                bb,
                &obstacles,
                &|i| obstacles[i].id >= 0 && knocked.contains(&obstacles[i].id),
                dt,
            );
            let touching: Vec<i64> = impacts
                .iter()
                .map(|h| &obstacles[h.obstacle])
                .filter(|o| o.mass > 0.0)
                .map(|o| o.id)
                .collect();
            for hit in &impacts {
                if hit.broke {
                    let o = &obstacles[hit.obstacle];
                    if o.id >= 0 && !self.knocked.contains(&o.id) {
                        self.knocked.push(o.id);
                        self.knocked_now.push((o.id, hit.push.as_dvec3()));
                    }
                }
                // a brush that takes less than a kilojoule is no accident (a broken post is), and
                // a car still inside the bus from the frame before is the same accident
                let o = &obstacles[hit.obstacle];
                if hit.speed >= crate::rigid::CRASH_SPEED && omsi_cfg::flags::OMSI_DEBUG_PHYSICS.is_set() {
                    log::info!("  hit obstacle {} at ({:.1}, {:.1}) z {:.2}..{:.2} half {:.2}x{:.2} heading {:.0}, {:.1} km/h", o.id, o.center.x, o.center.y, o.z0, o.z1, o.half.x, o.half.y, o.heading.to_degrees(), hit.speed * 3.6);
                }
                if hit.speed < crate::rigid::CRASH_SPEED
                    || (hit.energy < 1000.0 && !hit.broke)
                    || (o.mass > 0.0 && self.touching.contains(&o.id))
                {
                    continue;
                }
                self.break_glass_for_impact(hit.point, hit.speed, hit.energy);
                if o.mass > 0.0 && o.id <= -2 {
                    let point = rb.position
                        + rb.orientation.mul_vec3(hit.point - rb.cog).as_dvec3();
                    self.dynamic_impacts.push(DynamicImpact {
                        obstacle_id: o.id,
                        point,
                        push: hit.push.as_dvec3(),
                        speed: hit.speed,
                        energy: hit.energy,
                        mass: self.physics.mass_kg,
                    });
                }
                energy += hit.energy;
                if worst.map(|w| hit.energy > w.energy).unwrap_or(true) {
                    worst = Some(*hit);
                }
            }
            self.touching = touching;
        }
        if let Some(hit) = worst {
            if omsi_cfg::flags::OMSI_DEBUG_PHYSICS.is_set() {
                log::info!(
                    "impact at {:.1} km/h, {:.1} kJ, body point ({:.2}, {:.2}, {:.2}){}",
                    hit.speed * 3.6,
                    energy / 1000.0,
                    hit.point.x,
                    hit.point.y,
                    hit.point.z,
                    if hit.broke { ", a post broke off" } else { "" }
                );
            }
            // `coll_energy` is in kJ: the stock and mod collision scripts compare it with
            // 100-1000 for the electrics and 1-10 for a lamp - in joules a bump at
            // walking pace took the electrics out
            self.host.coll_pos = hit.point.to_array();
            self.host.coll_energy += energy / 1000.0;
            self.last_crash += energy;
            self.last_impact = energy;
            self.crashes += 1;
            self.collided = true;
            self.break_glass_for_impact(hit.point, hit.speed, hit.energy);
        }
        self.position = rb.origin();
        let (heading, pitch, bank) = rb.heading_pitch_bank();
        self.heading = heading;
        self.pitch = pitch;
        self.bank = bank;
        // script interface
        let speed = rb.forward_speed();
        self.physics.speed = speed;
        self.physics.steer_deg = rb.steer_deg;
        self.physics.accel = rb.accel_body;
        // `Velocity` is what the driven wheels' turning makes of the speed, `Velocity_Ground`
        // the body's own (see `driven_wheels_speed`): a wheel spinning on ice or locked by
        // the brake shows on the speedometer as in OMSI.
        let wheels = rb.wheels.iter().map(|w| (w.spin, w.radius, w.driven));
        // (a bus standing on its brakes: the wheels' rocking of a few hundredths of a metre a
        // second is no speed, scripts test `!Velocity` for a bus at rest)
        let ground = script_speed(speed * 3.6);
        let wheels = driven_wheels_speed(wheels).unwrap_or(speed) * 3.6;
        let v = if ground == 0.0 && wheels.abs() < 0.5 { 0.0 } else { script_speed(wheels) };
        self.put(self.v_velocity, v);
        self.put(self.v_velocity_ground, ground);
        let n_wheel = rb
            .wheels
            .iter()
            .find(|w| w.driven)
            .or(rb.wheels.first())
            .map(|w| w.rpm)
            .unwrap_or(0.0);
        self.put(self.v_n_wheel, n_wheel);
        // `A_Trans_*` as Omsi.exe has them (0x7d5124): the change of the body's velocity
        // over the frame, turned into the body frame - its acceleration, not what an
        // accelerometer reads, so 0 standing or cruising. `accel_body` carries gravity's
        // 9.81 m/s² (the wheels' springs need it), which as `A_Trans_Z` kept checks such
        // as the NEOMAN ECAS's "|A_Trans_Z| < 3 while driving" from ever passing.
        // (over OMSI's frames: the rattle scripts take its change from one frame to the next)
        let a = self.a_trans.push(scripts_acceleration(rb.accel_body, rb.orientation), dt);
        self.physics.a_trans = a;
        self.put(self.v_accel[0], a.x);
        self.put(self.v_accel[1], a.y);
        self.put(self.v_accel[2], a.z);
        for ai in 0..self.v_wheels.len() {
            let axle = self.v_wheels[ai];
            for (si, w) in axle.iter().enumerate() {
                if let Some(rw) = rb.wheels.get(ai * 2 + si) {
                    self.put(w[0], rw.rotation_deg.to_radians());
                    self.put(w[1], rw.rpm);
                    // (each axle's own angle: OMSI turns every axle towards the centre of
                    // the bend on the `[rot_pnt_long]` line - one angle for both sides)
                    self.put(w[2], rb.axle_steer(ai * 2 + si));
                    // `Axle_Suspension_*` is the wheel's travel *relative to the body*, and
                    // the stock model.cfg moves the wheel down for a positive value
                    // (`origin_rot_y -90` + `anim_trans`, checked with OMSI_DEBUG_ANIM on
                    // the SD200's front wheel). A compressed spring means the body has
                    // come down towards the wheel, i.e. the wheel sits *up* in its arch:
                    // the value is minus the compression. Handing over the compression
                    // itself pushed every wheel down by twice its travel - into the road
                    // under braking, with the body riding high above the arches.
                    // (never below where the spring is unloaded: Omsi.exe hands over
                    // -clamp(travel, 0, maxforce / k), 0x7e4afa - a wheel in the air stays
                    // where it hangs at rest)
                    let shown = -rw.compression.max(0.0);
                    self.put(w[3], shown);
                    if let Some(ws) = self.physics.wheels.get_mut(ai).and_then(|a| a.get_mut(si)) {
                        ws.rotation_deg = rw.rotation_deg;
                        ws.rpm = rw.rpm;
                        ws.suspension = shown;
                    }
                }
            }
        }
        self.rigid = Some(rb);
    }
}
