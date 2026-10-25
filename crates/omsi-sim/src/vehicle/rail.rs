//! A rail vehicle's speed: the scripts' drive and brakes, the track's grade and curve and the
//! weather on the rails go through [`crate::rail::dynamics`] instead of through the road
//! tyres of the rigid body, which know nothing of rails.

use super::*;
use crate::rail::{self, Conditions, Consist, Forces, RailClass};

/// What the track under a rail vehicle is like, told by whoever lays it on the track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RailEnv {
    /// Rise per metre in the vehicle's forward direction.
    pub grade: f32,
    /// Radius of the curve under it (m), infinite on the straight.
    pub radius: f32,
    pub in_tunnel: bool,
    /// What a pantograph finds under it and what a third-rail shoe finds beside it, where the
    /// map has a `catenary.cfg` / `thirdrail.cfg` (else `None`: nothing is known and the
    /// current is taken for granted, as on every map before the files existed).
    pub wire: Option<omsi_transit::Supply>,
    pub third_rail: Option<omsi_transit::Supply>,
}

impl VehicleInstance {
    /// Where this vehicle gets its current, from its file or its shape.
    pub fn rail_power_kind(&self) -> omsi_vehicle::rail_spec::PowerKind {
        use omsi_vehicle::rail_spec::PowerKind;
        let d = &self.ty.def;
        match d.rail_power {
            PowerKind::Auto if !d.contact_shoes.is_empty() => PowerKind::ThirdRail,
            PowerKind::Auto if pantograph_binds(d) => PowerKind::Catenary,
            PowerKind::Auto => PowerKind::OnBoard,
            k => k,
        }
    }

    /// What the vehicle's collector finds on its line now: `None` where it needs no line or the
    /// map says nothing of one.
    pub fn rail_supply(&self) -> Option<omsi_transit::Supply> {
        use omsi_vehicle::rail_spec::PowerKind;
        let env = self.rail_env?;
        match self.rail_power_kind() {
            PowerKind::Catenary => env.wire,
            PowerKind::ThirdRail => env.third_rail,
            _ => None,
        }
    }

    /// The track the vehicle runs on, or `None` for a road vehicle: its speed then comes from
    /// the rail dynamics, not from the road tyres.
    pub fn set_rail_env(&mut self, env: Option<RailEnv>) {
        self.rail_env = env;
    }

    /// The whole consist as the dynamics see it: this vehicle and everything coupled to it.
    pub fn rail_consist(&self, class: RailClass) -> Consist {
        let mass_of = |d: &omsi_vehicle::Vehicle| d.mass_kg().max(500.0);
        let parts = std::iter::once(&self.ty.def).chain(self.trailers.iter().map(|t| &t.ty.def));
        let (mut mass, mut length, mut wheels, mut driven) = (0.0f32, 0.0f32, 0usize, 0usize);
        // (the weight on driven axles: each car's mass shared over its axles)
        let mut driven_weight = 0.0f32;
        for d in parts {
            let m = mass_of(d);
            mass += m;
            length += d.bounding_box.map(|b| b[1]).unwrap_or(12.0);
            let n = d.axles.len().max(1);
            let dn = d.axles.iter().filter(|a| a.driven).count();
            wheels += n;
            driven += dn;
            driven_weight += m * dn as f32 / n as f32;
        }
        let _ = (wheels, driven);
        let aero = crate::rigid::realism::Aero::of(self.ty.def.bounding_box.map(|b| [b[0], b[1], b[2]]), mass_of(&self.ty.def));
        Consist {
            class,
            mass,
            driven_share: if mass > 0.0 { driven_weight / mass } else { 1.0 },
            cars: 1 + self.trailers.len(),
            cd_a: aero.cd_a,
            length,
        }
    }

    /// The drive and the brakes the scripts ask for this frame, N: `m_wheel` is the torque at
    /// the driven wheels, `brakes` the per-wheel brake forces of the vehicle's own axles and
    /// the coupled cars' come from theirs. `None` for a vehicle whose scripts give neither
    /// (the stock trains are scripted for the AI only).
    fn rail_script_forces(&self, m_wheel: f32, brakes: &[f32]) -> Option<Forces> {
        let own: f32 = brakes.iter().sum();
        let towed: f32 = self.trailers.iter().flat_map(|t| t.v_brakes.iter()).map(|&id| self.get(id).max(0.0)).sum();
        let brake = own + towed;
        if m_wheel.abs() < 1.0 && brake < 1.0 {
            return None;
        }
        let driven = self.ty.def.axles.iter().filter(|a| a.driven).map(|a| a.wheel_diameter / 2.0).collect::<Vec<_>>();
        let radius = if driven.is_empty() { 0.45 } else { driven.iter().sum::<f32>() / driven.len() as f32 }.max(0.15);
        Some(Forces { drive: m_wheel / radius, brake, track_brake: 0.0 })
    }

    /// The speed along the rails after `dt` as the rail dynamics make it (`v0` the speed
    /// before): `None` for a vehicle that is not on a track.
    fn rail_solve(&mut self, dt: f32, v0: f32, m_wheel: f32, brakes: &[f32]) -> Option<rail::Outcome> {
        let (env, class) = (self.rail_env?, RailClass::of(&self.ty.def)?);
        let consist = self.rail_consist(class);
        let cond = Conditions {
            grade: env.grade,
            radius: env.radius,
            wetness: self.host.street_cond,
            temperature: self.host.temperature,
            in_tunnel: env.in_tunnel,
            air_density: crate::rigid::realism::air_density(self.host.temperature, 0.0),
            headwind: 0.0,
            sanding: false,
        };
        let from_scripts = self.rail_script_forces(m_wheel, brakes);
        // no current, no drive in either direction (the brakes still work)
        let powered = self.rail_powered();
        // (a sagging line weakens the motors)
        let sag = self.rail_supply().map_or(1.0, |s| s.drive_factor());
        let from_scripts = from_scripts.map(|f| without_current(Forces { drive: f.drive * sag, ..f }, powered));
        if let Some(f) = from_scripts {
            self.note_traction_current(f.drive, v0);
        }
        self.rail_scripted |= from_scripts.is_some();
        let c = self.physics.controls;
        let forces = if self.rail_scripted {
            from_scripts.unwrap_or_default()
        } else {
            let reverse = self.var("rail_reverse").is_some_and(|r| r > 0.5);
            let pedals = rail::dynamics::pedal_forces(class, consist.mass, c.throttle, c.brake, v0, reverse);
            let f = self.rail_declared_forces(class, &consist, pedals, c.throttle, c.brake, v0, reverse, dt);
            without_current(Forces { drive: f.drive * sag, ..f }, powered)
        };
        let mut out = rail::step(&consist, &cond, &forces, v0, dt);
        if let Some(peak) = self.rail_couplers(&consist, &forces, &out, v0, dt) {
            out.coupler_force = peak;
        }
        if omsi_cfg::flags::OMSI_DEBUG_RAIL.is_set() {
            self.rail_log += dt;
            if self.rail_log >= 0.5 {
                self.rail_log = 0.0;
                log::info!(
                    "rail: {} pantograph {:?} v {:.1} km/h a {:.2} m/s² | M_Wheel {:.0} Nm -> drive {:.0} N, brake {:.0} N ({}) | mass {:.0} kg, driven {:.2}, {} cars, adhesion {:.2} | spin {} slide {} resist {:.0} N",
                    self.ty.def.path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
                    self.pantograph_var().and_then(|v| self.var(v)),
                    out.speed * 3.6, out.accel, m_wheel, forces.drive, forces.brake,
                    if self.rail_scripted { "scripts" } else { "pedals" },
                    consist.mass, consist.driven_share, consist.cars,
                    rail::dynamics::adhesion(out.speed, cond.wetness, cond.temperature, cond.sanding),
                    out.spinning, out.sliding, out.resistance
                );
            }
        }
        self.rail_out = Some(out);
        Some(out)
    }

    /// The pedal-driven vehicle's forces from what its file describes: the traction curve
    /// (`[rail_traction]`) in place of the class's typical pull, the brake system (`[rail_brake]`)
    /// in place of the class's typical braking. What the file leaves out stays the class's.
    #[allow(clippy::too_many_arguments)]
    fn rail_declared_forces(&mut self, class: RailClass, consist: &Consist, pedals: Forces, throttle: f32, brake: f32, v: f32, reverse: bool, dt: f32) -> Forces {
        let traction = self.ty.def.rail_traction.clone();
        let brake_spec = self.ty.def.rail_brake.clone();
        if traction.is_none() && brake_spec.is_none() {
            return pedals;
        }
        let mut f = pedals;
        if let Some(t) = &traction {
            if !t.points.is_empty() {
                f.drive = throttle.clamp(0.0, 1.0) * t.pull(v) * if reverse { -1.0 } else { 1.0 };
            }
        }
        // a broken train has its brakes applied by the air escaping from its pipe
        let handle = if self.rail_broken { 1.0 } else { brake };
        if brake_spec.is_some() || traction.as_ref().is_some_and(|t| !t.regen.is_empty()) {
            let perf = class.performance();
            let spec = brake_spec.unwrap_or_default();
            let rebuild = self.rail_brake_sys.as_ref().is_none_or(|b| b.cars() != consist.cars);
            if rebuild {
                self.rail_brake_sys = Some(rail::brake::BrakeSystem::new(spec, consist.cars, consist.length, consist.mass, perf.service_brake, perf.emergency_brake));
            }
            let electric_max = traction.as_ref().map(|t| t.electric_brake(v)).unwrap_or(0.0);
            if let Some(b) = self.rail_brake_sys.as_mut() {
                let out = b.step(dt, handle, v, electric_max);
                // (a tram's magnetic track brake still comes with its emergency stop)
                f.brake = out.friction + out.electric;
                f.track_brake = if class == RailClass::Tram && handle >= 0.9 { pedals.track_brake } else { 0.0 };
            }
        }
        f
    }

    /// The couplings of a train (`[rail_coupler]`): the force they carry this step, worked out
    /// from how the drive, the brakes and the resistance divide over the cars. `None` for a
    /// single vehicle or a file without a coupler. A train that pulls a coupling apart is broken
    /// and its brakes come on.
    fn rail_couplers(&mut self, consist: &Consist, forces: &Forces, out: &rail::Outcome, v0: f32, dt: f32) -> Option<f32> {
        let spec = self.ty.def.rail_coupler?;
        if consist.cars < 2 {
            return None;
        }
        let parts: Vec<&omsi_vehicle::Vehicle> = std::iter::once(&self.ty.def).chain(self.trailers.iter().map(|t| &t.ty.def)).collect();
        let masses: Vec<f32> = parts.iter().map(|d| d.mass_kg().max(500.0)).collect();
        if self.rail_chain.as_ref().is_none_or(|c| c.mass.len() != masses.len()) {
            self.rail_chain = Some(rail::chain::Chain::new(spec, masses.clone(), v0));
        }
        let total: f32 = masses.iter().sum();
        // the drive divides by the weight on each car's driven axles (the head if none is driven)
        let weights: Vec<f32> = parts.iter().zip(&masses).map(|(d, m)| m * d.axles.iter().filter(|a| a.driven).count() as f32 / d.axles.len().max(1) as f32).collect();
        let wsum: f32 = weights.iter().sum();
        let net = out.accel * total * 1.06;
        let drive_total = forces.drive;
        let rest = net - drive_total;
        let ext: Vec<f32> = masses.iter().enumerate().map(|(i, m)| {
            let d = if wsum > 0.0 { drive_total * weights[i] / wsum } else if i == 0 { drive_total } else { 0.0 };
            d + rest * m / total
        }).collect();
        let chain = self.rail_chain.as_mut()?;
        let o = chain.step(dt, &ext);
        // (the lump's speed is the truth; the cars keep their differences from it)
        let shift = out.speed - o.speed;
        chain.speed.iter_mut().for_each(|s| *s += shift);
        if chain.broken.is_some() && !self.rail_broken {
            self.rail_broken = true;
            log::warn!("rail: {} broke in two at a coupling ({:.0} kN)", self.ty.def.path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(), o.peak / 1000.0);
        }
        Some(o.peak)
    }

    /// After the rigid body's step: the rail dynamics' speed. The wheels turn with it - a
    /// rail wheel does not slip as a road tyre does - and the body keeps to the line.
    pub(super) fn rail_override(&mut self, rb: &mut crate::rigid::RigidBody, dt: f32, v0: f32, m_wheel: f32, brakes: &[f32]) {
        let Some(out) = self.rail_solve(dt, v0, m_wheel, brakes) else { return };
        let fwd = rb.orientation.mul_vec3(glam::Vec3::Y);
        rb.velocity = fwd * out.speed;
        rb.omega = glam::Vec3::ZERO;
        for w in rb.wheels.iter_mut() {
            w.spin = out.speed / w.radius.max(0.05);
            w.slipping = out.spinning && w.driven;
            w.locked = false;
            w.rpm = w.spin * 60.0 / std::f32::consts::TAU;
            w.rotation_deg = (w.rotation_deg + w.spin.to_degrees() * dt).rem_euclid(360.0);
        }
        rb.accel_body = glam::Vec3::new(0.0, out.accel, 9.81);
    }

    /// The same for the simple model, which a rail vehicle runs on (it is put on its track,
    /// not on the road): returns the distance covered this step.
    pub(super) fn rail_override_simple(&mut self, dt: f32, v0: f32, m_wheel: f32, brakes: &[f32]) -> Option<f32> {
        let out = self.rail_solve(dt, v0, m_wheel, brakes)?;
        self.physics.speed = out.speed;
        self.physics.accel = glam::Vec3::new(0.0, out.accel, 0.0);
        for w in self.physics.wheels.iter_mut().flatten() {
            let omega = out.speed / w.radius.max(0.05);
            w.rpm = omega * 60.0 / std::f32::consts::TAU;
            w.rotation_deg = (w.rotation_deg + omega.to_degrees() * dt).rem_euclid(360.0);
        }
        Some(out.speed * dt)
    }

    /// The variable that moves this vehicle's pantograph (declared, or found in its model).
    pub fn pantograph_var(&self) -> Option<&str> {
        self.ty.def.pantograph_var.as_deref()
    }

    /// Whether the pantograph is raised: `None` for a vehicle without one.
    pub fn pantograph_raised(&self) -> Option<bool> {
        let v = self.var(self.pantograph_var()?)?;
        Some(v > 0.5)
    }

    /// Raise or lower the pantograph with the cabin's own switch (the trigger the vehicle
    /// declares or the model's scripts have for it). Returns what it is meant to be now.
    pub fn toggle_pantograph(&mut self) -> Option<bool> {
        let trigger = self.ty.def.pantograph_trigger.clone()?;
        let was = self.pantograph_raised()?;
        // (a door script has a pair: `x` opens, `x_close` closes)
        let close = format!("{trigger}_close");
        let name = if was && self.ty.program.trigger(&close).is_some() { close } else { trigger };
        self.trigger(&name);
        // (the leaves of a door script move on a few frames; what the trigger asked for)
        Some(!was)
    }

    /// Whether the vehicle's drive has current: a rail vehicle with a pantograph moves only with
    /// it raised.
    pub fn rail_powered(&self) -> bool {
        // the pantograph as the wire sees it: raised AND touching a live wire. Where its travel
        // could not be measured the variable alone says (raised: 1).
        let pantograph = !pantograph_binds(&self.ty.def) || self.pantograph_current().unwrap_or_else(|| self.pantograph_raised().unwrap_or(true));
        // and, where the map has a line, the line has current where the collector is
        pantograph && self.rail_supply().is_none_or(|s| s.live())
    }

    /// What the rail dynamics made of the last step (spinning or sliding wheels, resistance).
    pub fn rail_outcome(&self) -> Option<rail::Outcome> {
        self.rail_out
    }
}

/// Whether the vehicle's pantograph decides if it has current: one the file declares does. One
/// the game found from the model does when it also found the switch that raises it - else a
/// pantograph found wrongly (or spawned down with no way to raise it) would keep the vehicle
/// from ever moving.
pub(super) fn pantograph_binds(def: &omsi_vehicle::Vehicle) -> bool {
    def.pantograph_var.is_some() && (!def.pantograph_detected || def.pantograph_trigger.is_some())
}

/// The forces without current: no drive, forward or backward (a scripted car's negative torque
/// pulls as much as a positive one); the brakes work as they did.
pub(super) fn without_current(f: Forces, powered: bool) -> Forces {
    if powered { f } else { Forces { drive: 0.0, ..f } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_current_a_car_can_neither_drive_forward_nor_backward() {
        let f = Forces { drive: -5000.0, brake: 3000.0, track_brake: 1000.0 };
        let off = without_current(f, false);
        assert_eq!((off.drive, off.brake, off.track_brake), (0.0, 3000.0, 1000.0));
        let fwd = without_current(Forces { drive: 5000.0, ..f }, false);
        assert_eq!(fwd.drive, 0.0);
        assert_eq!(without_current(f, true).drive, -5000.0);
    }

    #[test]
    fn a_pantograph_binds_only_when_declared_or_found_with_its_switch() {
        let mut def = omsi_vehicle::Vehicle::default();
        assert!(!pantograph_binds(&def), "none");
        def.pantograph_var = Some("Panto_anim_oben".into());
        assert!(pantograph_binds(&def), "declared by the file");
        def.pantograph_detected = true;
        assert!(!pantograph_binds(&def), "found from the model with no switch: never keeps the car from moving");
        def.pantograph_trigger = Some("bus_door_0".into());
        assert!(pantograph_binds(&def), "found with the switch that raises it");
    }
}
