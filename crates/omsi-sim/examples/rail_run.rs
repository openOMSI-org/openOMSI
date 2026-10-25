//! A rail vehicle on a straight, level track: its class, and how it runs under the pedals
//! (or its scripts) - 30 s of full throttle, then the service brake to a stop.
//!
//! usage: rail_run <vehicle.ovh|.bus> [--grade 0.02] [--wet 0..2]
//! (the OMSI 2 installation is taken from $OMSI_ROOT or ~/.openomsi-root)
use std::path::{Path, PathBuf};

fn original_root() -> Option<PathBuf> {
    if let Ok(r) = omsi_cfg::flags::OMSI_ROOT.live_var() {
        return Some(PathBuf::from(r));
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(std::fs::read_to_string(Path::new(&home).join(".openomsi-root")).ok()?.trim()))
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let path = PathBuf::from(&a[1]);
    let flag = |n: &str| a.iter().position(|s| s == n).and_then(|i| a.get(i + 1)).and_then(|v| v.parse::<f32>().ok());
    let root = original_root().expect("no OMSI 2 installation: set OMSI_ROOT");
    omsi_cfg::add_content_root(root.clone());
    let def = omsi_vehicle::Vehicle::load(&path).expect("cannot read the vehicle");
    println!("{}: class {:?}", path.display(), omsi_sim::rail::RailClass::of(&def).map(|c| c.name()));
    let vt = std::sync::Arc::new(omsi_sim::VehicleType::load(&root, &path).expect("cannot load the vehicle"));
    let mut v = omsi_sim::VehicleInstance::new(vt, omsi_sim::VehicleHost::new(Default::default()));
    v.host.street_cond = flag("--wet").unwrap_or(0.0);
    v.host.temperature = 15.0;
    v.position = glam::DVec3::new(0.0, 0.0, 0.0);
    if a.iter().any(|s| s == "--rigid") { v.enable_rigid_body(); }
    v.set_rail_env(Some(omsi_sim::vehicle::RailEnv { grade: flag("--grade").unwrap_or(0.0), radius: f32::INFINITY, in_tunnel: false, wire: None, third_rail: None }));
    let dt = 1.0 / 60.0;
    for step in 0..(70.0 / dt) as usize {
        let t = step as f32 * dt;
        let c = if t < 30.0 { omsi_sim::physics::Controls { throttle: 1.0, ..Default::default() } } else { omsi_sim::physics::Controls { brake: 0.9, ..Default::default() } };
        v.set_controls(c);
        v.update(dt);
        if step % 300 == 0 {
            let o = v.rail_outcome();
            println!("t {t:5.1} s  v {:6.1} km/h  a {:5.2} m/s²  spin {} slide {}", v.physics.speed * 3.6, o.map(|o| o.accel).unwrap_or(0.0), o.is_some_and(|o| o.spinning), o.is_some_and(|o| o.sliding));
        }
    }
}
