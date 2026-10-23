//! The hands on a trolleybus's poles. From outside the bus (walking, the outside camera, the
//! free camera) near its back the rope of the pole nearest to the player is held with R: the
//! pole rises and follows where the player looks, and when R is let go it springs up to the
//! wire it was aimed at (a wire missed, and the pole flies loose). T pulls the pole down;
//! Shift takes both poles. In the cab only a pole with a motor answers, to the same keys.

use glam::{DVec3, Vec3};
use omsi_sim::VehicleInstance;
use omsi_trolley::RopeCmd;
use winit::keyboard::KeyCode;

/// How near the ropes the player must be (m): on foot, and from an outside camera.
const FOOT_REACH: f64 = 9.0;
const CAMERA_REACH: f64 = 30.0;

pub(crate) struct Hands<'a> {
    pub keys: &'a dyn Fn(KeyCode) -> bool,
    pub view: &'a str,
    /// The player's eye and the way it looks.
    pub eye: Option<(DVec3, Vec3)>,
}

/// Set the ropes of the vehicle's poles from the keys; a message when the keys were held out of
/// reach of any rope.
pub(crate) fn apply(v: &mut VehicleInstance, h: &Hands) -> Option<&'static str> {
    let (pos, rot, bbox) = v.trolley_frame();
    let rear = bbox.map_or(-5.5, |b| b[4] - b[1].abs() * 0.5);
    let g = v.trolley.as_deref_mut()?;
    if g.rig.poles.is_empty() {
        return None;
    }
    g.ropes.iter_mut().for_each(|r| *r = RopeCmd::Idle);
    let raise = (h.keys)(KeyCode::KeyR);
    let pull = (h.keys)(KeyCode::KeyT);
    if !(raise || pull) {
        return None;
    }
    let both = (h.keys)(KeyCode::ShiftLeft) || (h.keys)(KeyCode::ShiftRight);
    let cmd = |ray: Option<(DVec3, DVec3)>| if pull { RopeCmd::Pull } else { RopeCmd::Raise { ray, auto: false } };
    if matches!(h.view, "driver" | "pax") {
        let mut any = false;
        for (k, p) in g.rig.poles.iter().enumerate() {
            if p.def.motor {
                g.ropes[k] = if pull { RopeCmd::Pull } else { RopeCmd::Raise { ray: None, auto: true } };
                any = true;
            }
        }
        return (!any).then_some("These poles are raised by hand from outside the bus");
    }
    let (eye, fwd) = h.eye?;
    let reach = if h.view == "foot" { FOOT_REACH } else { CAMERA_REACH };
    let handle = |side: f32| pos + rot.transform_point3(Vec3::new(side.signum() * 0.45, rear + 0.1, 0.9)).as_dvec3();
    let mut near: Vec<(usize, f64)> = g
        .rig
        .poles
        .iter()
        .enumerate()
        .map(|(k, p)| {
            let hd = handle(p.def.base.x);
            (k, ((hd.x - eye.x).powi(2) + (hd.y - eye.y).powi(2)).sqrt())
        })
        .filter(|(_, d)| *d <= reach)
        .collect();
    near.sort_by(|a, b| a.1.total_cmp(&b.1));
    if near.is_empty() {
        return Some("Too far from the ropes: go to the back of the bus");
    }
    let ray = Some((eye, fwd.as_dvec3()));
    let chosen: Vec<usize> = if both { near.iter().map(|n| n.0).collect() } else { vec![near[0].0] };
    for k in chosen {
        g.ropes[k] = cmd(ray);
    }
    None
}

/// `OMSI_TROLLEY_HANDS`: the hands of a scripted player at time `t` (s): `from-to:raise|pull:poles`
/// entries separated by `;`. A raising hand stands a few metres behind the bus and looks at where
/// the wire should be for the pole's working angle.
pub(crate) fn scripted(v: &mut VehicleInstance, spec: &str, t: f64) {
    let (pos, rot, _) = v.trolley_frame();
    let Some(g) = v.trolley.as_deref_mut() else { return };
    g.ropes.iter_mut().for_each(|r| *r = RopeCmd::Idle);
    for item in spec.split(';') {
        let mut it = item.trim().split(':');
        let (Some(range), Some(cmd), Some(poles)) = (it.next(), it.next(), it.next()) else { continue };
        let Some((a, b)) = range.split_once('-') else { continue };
        let (Ok(a), Ok(b)) = (a.trim().parse::<f64>(), b.trim().parse::<f64>()) else { continue };
        if t < a || t >= b {
            continue;
        }
        for c in poles.chars() {
            let Some(k) = c.to_digit(10).map(|d| d as usize).filter(|k| *k < g.rig.poles.len()) else { continue };
            g.ropes[k] = if cmd == "pull" {
                RopeCmd::Pull
            } else {
                let p = &g.rig.poles[k];
                let base = pos + rot.transform_point3(p.def.base).as_dvec3();
                let work = (p.def.work_angle as f64).to_radians();
                let l = p.def.length as f64;
                let tip_local = Vec3::new(0.0, -(work.cos() * l) as f32, (work.sin() * l) as f32);
                let target = pos + rot.transform_point3(p.def.base + tip_local).as_dvec3();
                let eye = pos + rot.transform_point3(Vec3::new(p.def.base.x, -9.5, 1.7)).as_dvec3();
                let _ = base;
                RopeCmd::Raise { ray: Some((eye, (target - eye).normalize())), auto: false }
            };
        }
    }
}
