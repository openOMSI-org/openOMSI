//! How a bus handles on the rigid body, from its own `.bus` file: a steering step at a
//! steady speed on a flat road, and the body settling after the wheel is straightened.
//!
//! `cargo run --release -p omsi-sim --example handling -- <file.bus>... [--kmh 50] [--steer 0.25] [--realistic]`
//! (`--realistic`: the model of the "Realistic road physics" setting; without it OMSI 2's)
//!
//! Per bus it prints what the numbers of its file make of it: the yaw rate against the one
//! the steering geometry asks for (1.00 = the bus follows its wheels), the time to 90 % of
//! it, the side slip at the centre of gravity, the body roll in the bend (peak and steady),
//! the roll frequency and how many swings the body makes before it is still again.

use glam::{DVec3, Vec3};
use omsi_sim::rigid::{GroundProbe, RigidBody};
use omsi_vehicle::Vehicle;
use std::path::PathBuf;

fn main() {
    let mut files: Vec<PathBuf> = Vec::new();
    let (mut kmh, mut steer) = (50.0f32, 0.25f32);
    let mut realistic = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--kmh" => kmh = args.next().and_then(|v| v.parse().ok()).unwrap_or(kmh),
            "--realistic" => realistic = true,
            "--steer" => steer = args.next().and_then(|v| v.parse().ok()).unwrap_or(steer),
            _ => files.push(a.into()),
        }
    }
    let flat = |_x: f64, _y: f64, _t: f64| GroundProbe { below: Some(0.0), above: None };
    println!("{:<34} {:>6} {:>7} {:>7} {:>7} {:>7} {:>7} {:>6} {:>6}", "bus", "yaw/k", "t90 s", "slip°", "roll°", "peak°", "roll Hz", "swings", "settle");
    for f in files {
        let def = match Vehicle::load(&f) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{}: {e}", f.display());
                continue;
            }
        };
        let mut rb = RigidBody::from_definition(&def, &[]);
        rb.realistic = realistic;
        rb.place(DVec3::ZERO, 0.0);
        let dt = 1.0 / 60.0;
        let brakes = vec![0.0; rb.wheels.len()];
        let v_target = kmh / 3.6;
        // settle standing, then roll at the speed
        for _ in 0..120 {
            rb.step(dt, 0.0, &brakes, 0.0, &flat);
        }
        rb.velocity = rb.orientation.mul_vec3(Vec3::Y) * v_target;
        for w in rb.wheels.iter_mut() {
            w.spin = v_target / w.radius;
        }
        let r_drive = rb.wheels.iter().find(|w| w.driven).map(|w| w.radius).unwrap_or(0.5);
        let hold_speed = |rb: &RigidBody| -> f32 { ((v_target - rb.forward_speed()) * rb.mass * 2.0 + rb.rolling_resistance + 0.36 * rb.forward_speed().powi(2) * (rb.mass / 2200.0).clamp(2.0, 8.5)) * r_drive };
        for _ in 0..180 {
            let m = hold_speed(&rb);
            rb.step(dt, m, &brakes, 0.0, &flat);
        }
        // the step: the steering geometry's own yaw rate is v * curvature
        let kappa = steer * def.inv_min_turn_radius;
        let (mut t90, mut yaw_ss, mut slip_ss, mut roll_ss, mut roll_peak) = (None, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let secs = 8.0;
        let n = (secs / dt) as usize;
        let mut yaw_trace = Vec::with_capacity(n);
        for i in 0..n {
            let m = hold_speed(&rb);
            rb.step(dt, m, &brakes, steer, &flat);
            let yaw = rb.omega.z.abs();
            yaw_trace.push(yaw);
            let (_, _, bank) = rb.heading_pitch_bank();
            roll_peak = roll_peak.max(bank.abs());
            if i + 60 >= n {
                let v_body = rb.orientation.inverse().mul_vec3(rb.velocity);
                yaw_ss += yaw / 60.0;
                // side slip where the bus turns about (the rear axle line): zero when it
                // follows its wheels, the drift angle when it does not
                let x_at = v_body.x - rb.omega.z * (def.rot_pnt_long - rb.cog.y);
                slip_ss += x_at.atan2(v_body.y).to_degrees().abs() / 60.0;
                roll_ss += bank.abs() / 60.0;
            }
        }
        for (i, y) in yaw_trace.iter().enumerate() {
            if t90.is_none() && *y >= 0.9 * yaw_ss {
                t90 = Some(i as f32 * dt);
            }
        }
        let v = rb.forward_speed();
        let ratio = yaw_ss / (v * kappa).max(1e-6);
        // straighten the wheel: how the body comes back
        let mut rolls = Vec::new();
        for _ in 0..(6.0 / dt) as usize {
            let m = hold_speed(&rb);
            rb.step(dt, m, &brakes, 0.0, &flat);
            rolls.push(rb.heading_pitch_bank().2);
        }
        let mean = rolls[rolls.len() - 60..].iter().sum::<f32>() / 60.0;
        let mut crossings = Vec::new();
        for i in 1..rolls.len() {
            if (rolls[i - 1] - mean).signum() != (rolls[i] - mean).signum() {
                crossings.push(i as f32 * dt);
            }
        }
        let freq = if crossings.len() >= 3 { (crossings.len() - 1) as f32 / 2.0 / (crossings[crossings.len() - 1] - crossings[0]).max(1e-3) } else { 0.0 };
        let settle = rolls.iter().rposition(|r| (r - mean).abs() > 0.05).map(|i| i as f32 * dt).unwrap_or(0.0);
        let name = format!("{}/{}", f.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), f.file_name().unwrap().to_string_lossy());
        println!("{:<34} {:>6.2} {:>7.2} {:>7.2} {:>7.2} {:>7.2} {:>7.2} {:>6} {:>5.1}s", name.chars().take(34).collect::<String>(), ratio, t90.unwrap_or(f32::NAN), slip_ss, roll_ss, roll_peak, freq, crossings.len() / 2, settle);
    }
}
