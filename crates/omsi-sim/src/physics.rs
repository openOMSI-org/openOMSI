//! Road vehicle dynamics.
//!
//! This is a deliberately simple first model (longitudinal forces + kinematic steering +
//! ground following) that speaks the original script interface: the scripts provide
//! `M_Wheel` (drive torque at the driven wheels, Nm), `Axle_Brakeforce_i_L/R` (N per wheel)
//! and `Axle_Springfactor_*`; the engine provides `Velocity` (km/h), `n_Wheel` (rpm of the
//! driven axle), `Wheel_RotationSpeed_i_*` (rpm), `Wheel_Rotation_i_*` (degrees),
//! `Axle_Steering_i_*` (degrees), `Axle_Suspension_i_*` (m), `A_Trans_*` (m/s²), `Throttle`,
//! `Brake`, `Clutch`. A rigid-body model with wheel contacts replaces it later.

use glam::{DVec3, Vec3};
use omsi_vehicle::Vehicle;

/// Driver inputs, all in `0..1` except steering in `-1..1` (positive = right).
#[derive(Debug, Clone, Copy, Default)]
pub struct Controls {
    pub throttle: f32,
    pub brake: f32,
    pub clutch: f32,
    pub steering: f32,
}

#[derive(Debug, Clone)]
pub struct WheelState {
    /// Longitudinal axle position (m, forward positive) and lateral offset (m, right positive).
    pub long: f32,
    pub lat: f32,
    pub radius: f32,
    pub driven: bool,
    pub rotation_deg: f32,
    pub turned: f32,
    pub turned_put: f32,
    pub rpm: f32,
    pub suspension: f32,
}

#[derive(Debug, Clone)]
pub struct VehiclePhysics {
    pub mass_kg: f32,
    pub rolling_resistance: f32,
    pub inv_min_turn_radius: f32,
    pub rot_pnt_long: f32,
    pub wheelbase: f32,
    /// Per axle: (left, right) wheels.
    pub wheels: Vec<[WheelState; 2]>,
    /// Longitudinal speed, m/s (forward positive).
    pub speed: f32,
    pub accel: Vec3,
    /// The body's acceleration without gravity (m/s², x right, y forward, z up): what the
    /// scripts read as `A_Trans_*`.
    pub a_trans: Vec3,
    /// Current steering angle of the front wheels, degrees, positive = right.
    pub steer_deg: f32,
    pub max_steer_deg: f32,
    pub controls: Controls,
    /// Steering wheel input rate for keyboard steering (fraction per second).
    pub steer_rate: f32,
}

impl VehiclePhysics {
    pub fn from_definition(def: &Vehicle) -> VehiclePhysics {
        let mass_kg = if def.mass < 100.0 { def.mass * 1000.0 } else { def.mass };
        let wheels: Vec<[WheelState; 2]> = def
            .axles
            .iter()
            .map(|a| {
                let mk = |lat: f32| WheelState { long: a.long, lat, radius: (a.wheel_diameter / 2.0).max(0.1), driven: a.driven, rotation_deg: 0.0, turned: 0.0, turned_put: 0.0, rpm: 0.0, suspension: 0.0 };
                [mk(-a.max_width / 2.0), mk(a.max_width / 2.0)]
            })
            .collect();
        let front = wheels.iter().map(|w| w[0].long).fold(f32::MIN, f32::max);
        let rear = wheels.iter().map(|w| w[0].long).fold(f32::MAX, f32::min);
        let wheelbase = (front - rear).abs().max(1.0);
        // inv_min_turnradius = tan(alpha_max) / s  →  alpha_max
        let s = (front - def.rot_pnt_long).abs().max(1.0);
        let max_steer_deg = (def.inv_min_turn_radius * s).atan().to_degrees().clamp(10.0, 60.0);
        VehiclePhysics { mass_kg: mass_kg.max(500.0), rolling_resistance: def.rolling_resistance, inv_min_turn_radius: def.inv_min_turn_radius, rot_pnt_long: def.rot_pnt_long, wheelbase, wheels, speed: 0.0, accel: Vec3::ZERO, a_trans: Vec3::ZERO, steer_deg: 0.0, max_steer_deg, controls: Controls::default(), steer_rate: 0.8 }
    }

    /// Advance one step. `drive_torque` is `M_Wheel`, `brake_forces` the per-wheel brake
    /// forces in the same order as `wheels`. Returns the displacement along the vehicle's
    /// forward axis and the heading change in degrees.
    pub fn step(&mut self, dt: f32, drive_torque: f32, brake_forces: &[f32]) -> (f32, f32) {
        let dt = dt.clamp(0.0, 0.1);
        // steering
        let target = self.controls.steering.clamp(-1.0, 1.0) * self.max_steer_deg;
        let rate = self.max_steer_deg * 2.5 * dt;
        self.steer_deg += (target - self.steer_deg).clamp(-rate, rate);

        // forces along the forward axis
        let driven_radius = self.wheels.iter().find(|w| w[0].driven).or(self.wheels.first()).map(|w| w[0].radius).unwrap_or(0.5);
        let f_drive = drive_torque / driven_radius;
        let f_brake: f32 = brake_forces.iter().sum::<f32>().max(0.0);
        let f_roll = self.rolling_resistance.max(0.0);
        let v = self.speed;
        let resist = f_brake + f_roll;
        let mut f = f_drive;
        // resistances oppose motion; at rest they can only cancel the drive force
        if v.abs() > 0.05 {
            f -= resist * v.signum();
        } else if f_drive.abs() <= resist {
            f = 0.0;
            self.speed = 0.0;
        } else {
            f -= resist * f_drive.signum();
        }
        let a = f / self.mass_kg;
        let new_speed = v + a * dt;
        // resistances must not reverse the direction of travel
        if v.abs() > 0.05 && new_speed.signum() != v.signum() && f_drive.abs() <= resist {
            self.speed = 0.0;
        } else {
            self.speed = new_speed;
        }
        self.accel = Vec3::new(0.0, a, 0.0);
        let ds = self.speed * dt;

        // kinematic turning about the rotation point
        let curvature = self.steer_deg.to_radians().tan() / self.wheelbase;
        let dheading = (ds * curvature).to_degrees();

        // wheels
        for axle in self.wheels.iter_mut() {
            for w in axle.iter_mut() {
                let omega = self.speed / w.radius; // rad/s
                w.rpm = omega * 60.0 / std::f32::consts::TAU;
                w.rotation_deg = (w.rotation_deg + omega.to_degrees() * dt).rem_euclid(360.0);
                w.turned += omega * dt;
            }
        }
        (ds, dheading)
    }

    pub fn velocity_kmh(&self) -> f32 {
        self.speed * 3.6
    }

    /// Ground height under the vehicle, used to follow the terrain.
    pub fn place_on_ground(position: &mut DVec3, ground: f64) {
        position.z = ground;
    }
}
