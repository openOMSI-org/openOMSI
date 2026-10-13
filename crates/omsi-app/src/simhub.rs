//! Telemetry for SimHub: the player's bus as UDP datagrams in the "Data Out"
//! format of Forza Motorsport 7 (the 311-byte "car dash" packet), which SimHub reads as
//! it is - choose *Forza Motorsport 7* there and the port set here - so its dashboards,
//! bass shakers, LEDs and wind work with the bus: speed, engine speed, gear, the pedals,
//! the steering, the accelerations, the fuel, the position. SimHub's own OMSI 2 support
//! reads OMSI's memory through OmsiHook and cannot see this program; the format of a game
//! SimHub already knows is the way in without a plugin of our own.
//!
//! Off by default (`simhub_port` 0 in the settings); sent to 127.0.0.1 about 60 times a
//! second while a bus is driven. Nothing leaves the computer unless the port is changed to
//! another machine's by hand (`OMSI_SIMHUB_HOST`).

use std::net::UdpSocket;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How often a packet goes out at most.
const EVERY: Duration = Duration::from_millis(16);

struct Sender {
    socket: UdpSocket,
    port: u16,
    host: String,
    last: Instant,
    started: Instant,
}

static SENDER: Mutex<Option<Sender>> = Mutex::new(None);

fn host() -> String {
    omsi_cfg::flags::OMSI_SIMHUB_HOST.var().filter(|h| !h.trim().is_empty()).map(str::to_string).unwrap_or_else(|| "127.0.0.1".into())
}

/// The values of one packet, read from the bus by whoever drives the frame.
#[derive(Default)]
pub(crate) struct Sample {
    pub speed_ms: f32,
    pub rpm: f32,
    pub rpm_max: f32,
    pub rpm_idle: f32,
    /// -1 reverse, 0 neutral, 1.. forward.
    pub gear: i32,
    pub throttle: f32,
    pub brake: f32,
    pub clutch: f32,
    pub handbrake: f32,
    /// -1 .. 1, left negative.
    pub steer: f32,
    /// Longitudinal, lateral and vertical acceleration (m/s²), the car's frame.
    pub accel: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    pub position: [f32; 3],
    /// 0 .. 1 of the tank.
    pub fuel: f32,
    pub wheel_rpm: [f32; 4],
    pub suspension: [f32; 4],
    pub distance_m: f32,
}

/// The sample read from the player's bus: the physics for what it measures, the bus's
/// script for the rest (the names most OMSI buses use; a bus without one gives 0).
pub(crate) fn sample(v: &omsi_sim::VehicleInstance) -> Sample {
    let var = |n: &str| v.var(n).unwrap_or(0.0);
    let first = |names: &[&str]| names.iter().find_map(|n| v.var(n)).unwrap_or(0.0);
    let p = &v.physics;
    let rpm = first(&["engine_n", "engine_rpm", "motor_n"]);
    let rpm_max = first(&["engine_n_max", "engine_n_max_power", "engine_maxrpm"]).max(1.0);
    let rpm_max = if rpm_max <= 1.0 { 2500.0 } else { rpm_max };
    let rpm_idle = first(&["engine_n_idle", "engine_idle_n", "engine_n_min"]);
    let rpm_idle = if rpm_idle <= 0.0 { 600.0 } else { rpm_idle };
    let reverse = first(&["antrieb_getr_reverse", "gear_reverse"]) > 0.5 || var("antrieb_getr_aktugang") < 0.0;
    let gear_raw = first(&["antrieb_getr_aktugang", "antrieb_getr_gang", "gear", "gear_actual"]);
    let gear = if reverse { -1 } else { gear_raw.round() as i32 };
    // (the tank's size is a constant of the script, not a variable: the fuel gauge's own
    // reading is the fraction, where the bus has one)
    let gauge = first(&["cockpit_tankuhr", "cockpit_fuel_gauge", "tankuhr"]);
    let tank = first(&["engine_tank_content", "fuel_tank_content", "tank_content"]);
    let tank_max = first(&["engine_tank_volume", "fuel_tank_volume", "tank_volume", "engine_tank_content_max"]);
    let fuel = if gauge > 0.0 { gauge.clamp(0.0, 1.0) } else if tank_max > 0.0 { (tank / tank_max).clamp(0.0, 1.0) } else { 0.0 };
    let mut wheel_rpm = [0.0f32; 4];
    let mut suspension = [0.0f32; 4];
    let mut k = 0;
    for axle in p.wheels.iter().take(2) {
        for w in axle {
            if k < 4 {
                wheel_rpm[k] = w.rpm;
                suspension[k] = w.suspension;
                k += 1;
            }
        }
    }
    let pos = v.position;
    Sample {
        speed_ms: p.speed,
        rpm,
        rpm_max,
        rpm_idle,
        gear,
        throttle: p.controls.throttle.clamp(0.0, 1.0),
        brake: p.controls.brake.clamp(0.0, 1.0),
        clutch: p.controls.clutch.clamp(0.0, 1.0),
        handbrake: first(&["bremse_feststell", "handbrake", "parkbrake"]).clamp(0.0, 1.0),
        steer: if p.max_steer_deg > 0.0 { (p.steer_deg / p.max_steer_deg).clamp(-1.0, 1.0) } else { p.controls.steering.clamp(-1.0, 1.0) },
        accel: [p.accel.y, p.accel.x, p.accel.z],
        yaw: v.heading as f32,
        pitch: v.pitch,
        roll: first(&["roll", "veh_roll"]),
        position: [pos.x as f32, pos.z as f32, pos.y as f32],
        fuel,
        wheel_rpm,
        suspension,
        distance_m: first(&["antrieb_wegstrecke", "odometer", "km_counter"]),
    }
}

/// The packet as Forza Motorsport 7 sends it (the "car dash" layout, 311 bytes): the sled
/// part first, the dash part after it. Little-endian throughout.
pub(crate) fn packet(s: &Sample, elapsed_ms: u32) -> Vec<u8> {
    let mut b: Vec<u8> = Vec::with_capacity(311);
    let f = |b: &mut Vec<u8>, x: f32| b.extend_from_slice(&x.to_le_bytes());
    let i = |b: &mut Vec<u8>, x: i32| b.extend_from_slice(&x.to_le_bytes());
    // --- sled (232 bytes)
    i(&mut b, 1); // IsRaceOn
    b.extend_from_slice(&elapsed_ms.to_le_bytes()); // TimestampMS
    f(&mut b, s.rpm_max); // EngineMaxRpm
    f(&mut b, s.rpm_idle); // EngineIdleRpm
    f(&mut b, s.rpm); // CurrentEngineRpm
    // AccelerationX (right), Y (up), Z (forward) in the car's frame
    f(&mut b, s.accel[1]);
    f(&mut b, s.accel[2]);
    f(&mut b, s.accel[0]);
    // VelocityX, Y, Z: the speed along the car's forward axis
    f(&mut b, 0.0);
    f(&mut b, 0.0);
    f(&mut b, s.speed_ms);
    // AngularVelocityX, Y, Z
    f(&mut b, 0.0);
    f(&mut b, 0.0);
    f(&mut b, 0.0);
    f(&mut b, s.yaw);
    f(&mut b, s.pitch);
    f(&mut b, s.roll);
    // NormalizedSuspensionTravel FL, FR, RL, RR (0 = full stretch, 1 = full compression)
    for k in 0..4 {
        f(&mut b, s.suspension[k].clamp(0.0, 1.0));
    }
    // TireSlipRatio x4
    for _ in 0..4 {
        f(&mut b, 0.0);
    }
    // WheelRotationSpeed x4 (rad/s)
    for k in 0..4 {
        f(&mut b, s.wheel_rpm[k] * std::f32::consts::TAU / 60.0);
    }
    // WheelOnRumbleStrip x4 (i32)
    for _ in 0..4 {
        i(&mut b, 0);
    }
    // WheelInPuddleDepth x4, SurfaceRumble x4, TireSlipAngle x4, TireCombinedSlip x4
    for _ in 0..16 {
        f(&mut b, 0.0);
    }
    // SuspensionTravelMeters x4
    for k in 0..4 {
        f(&mut b, s.suspension[k] * 0.1);
    }
    i(&mut b, 1); // CarOrdinal
    i(&mut b, 0); // CarClass
    i(&mut b, 100); // CarPerformanceIndex
    i(&mut b, 1); // DrivetrainType (1 = RWD)
    i(&mut b, 6); // NumCylinders
    debug_assert_eq!(b.len(), 232);
    // --- dash (79 bytes)
    f(&mut b, s.position[0]);
    f(&mut b, s.position[1]);
    f(&mut b, s.position[2]);
    f(&mut b, s.speed_ms); // Speed (m/s)
    f(&mut b, 0.0); // Power (W)
    f(&mut b, 0.0); // Torque (Nm)
    for _ in 0..4 {
        f(&mut b, 60.0); // TireTemp
    }
    f(&mut b, 0.0); // Boost
    f(&mut b, s.fuel); // Fuel (0..1)
    f(&mut b, s.distance_m); // DistanceTraveled
    f(&mut b, 0.0); // BestLap
    f(&mut b, 0.0); // LastLap
    f(&mut b, 0.0); // CurrentLap
    f(&mut b, elapsed_ms as f32 / 1000.0); // CurrentRaceTime
    b.extend_from_slice(&1u16.to_le_bytes()); // LapNumber
    b.push(1); // RacePosition
    b.push((s.throttle * 255.0).round() as u8); // Accel
    b.push((s.brake * 255.0).round() as u8); // Brake
    b.push((s.clutch * 255.0).round() as u8); // Clutch
    b.push((s.handbrake * 255.0).round() as u8); // HandBrake
    // Gear: 0 = reverse, 1.. = forward (neutral is sent as 0 too, as the game does at rest)
    b.push(if s.gear <= 0 { 0 } else { s.gear.min(10) as u8 });
    b.push((s.steer * 127.0).round() as i8 as u8); // Steer
    b.push(0u8); // NormalizedDrivingLine
    b.push(0u8); // NormalizedAIBrakeDifference
    debug_assert_eq!(b.len(), 311);
    b
}

/// Called every frame with the player's bus; sends while `port` is not 0.
pub(crate) fn publish(port: u16, vehicle: Option<&omsi_sim::VehicleInstance>) {
    let mut lock = SENDER.lock().unwrap_or_else(|e| e.into_inner());
    if port == 0 {
        if lock.take().is_some() {
            log::info!("SimHub telemetry: off");
        }
        return;
    }
    let host = host();
    if lock.as_ref().is_none_or(|s| s.port != port || s.host != host) {
        match UdpSocket::bind("0.0.0.0:0") {
            Ok(socket) => {
                let _ = socket.set_nonblocking(true);
                log::info!("SimHub telemetry: Forza Motorsport 7 packets to {host}:{port} (set SimHub to Forza Motorsport 7, UDP port {port})");
                *lock = Some(Sender { socket, port, host, last: Instant::now() - EVERY, started: Instant::now() });
            }
            Err(e) => {
                log::warn!("SimHub telemetry: no UDP socket: {e}");
                return;
            }
        }
    }
    let Some(s) = lock.as_mut() else { return };
    if s.last.elapsed() < EVERY {
        return;
    }
    s.last = Instant::now();
    let sample = match vehicle {
        Some(v) => sample(v),
        None => Sample::default(),
    };
    let pkt = packet(&sample, s.started.elapsed().as_millis() as u32);
    let _ = s.socket.send_to(&pkt, (s.host.as_str(), s.port));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_packet_has_forza_motorsport_7_s_shape() {
        let s = Sample { speed_ms: 13.9, rpm: 1500.0, rpm_max: 2400.0, rpm_idle: 600.0, gear: 3, throttle: 0.5, brake: 0.0, clutch: 0.0, handbrake: 1.0, steer: -0.5, fuel: 0.75, ..Default::default() };
        let p = packet(&s, 1234);
        assert_eq!(p.len(), 311);
        assert_eq!(i32::from_le_bytes(p[0..4].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(p[4..8].try_into().unwrap()), 1234);
        assert_eq!(f32::from_le_bytes(p[16..20].try_into().unwrap()), 1500.0);
        assert_eq!(f32::from_le_bytes(p[244..248].try_into().unwrap()), 13.9, "Speed");
        assert_eq!(f32::from_le_bytes(p[276..280].try_into().unwrap()), 0.75, "Fuel");
        assert_eq!((p[303], p[304], p[305], p[306], p[307]), (128, 0, 0, 255, 3), "Accel, Brake, Clutch, HandBrake, Gear");
        assert_eq!(p[308] as i8, -64, "Steer");
        let r = Sample { gear: -1, ..Default::default() };
        assert_eq!(packet(&r, 0)[307], 0, "reverse is gear 0");
    }
}
