//! The mouse's hand on a trolleybus's poles. A click on the cab's pole switch (or on a pole
//! itself from outside the cab) takes the pole in hand: it follows the mouse - its tip goes
//! towards the wire the cursor points at, or where the cursor points when there is none -
//! and the next click lets it go, to spring on to the wire it was aimed at (a wire missed,
//! and the pole flies loose). The keys R and T keep their ropes (see `trolley_hands`).

use super::*;
use glam::{DMat3, DVec3};

/// How near a pole the cursor's ray must pass to pick it (m, at a metre; it grows with the
/// distance as the cursor's own width does).
const PICK_RADIUS: f64 = 0.28;
/// How far a pole can be picked from (m).
const PICK_REACH: f64 = 45.0;

/// The distance between a ray and a segment, and where along the ray it is.
fn ray_segment(o: DVec3, d: DVec3, a: DVec3, b: DVec3) -> (f64, f64) {
    let u = d;
    let v = b - a;
    let w = o - a;
    let (uu, uv, vv) = (u.dot(u), u.dot(v), v.dot(v));
    let (uw, vw) = (u.dot(w), v.dot(w));
    let den = uu * vv - uv * uv;
    let mut s = if den.abs() < 1e-9 { 0.0 } else { (uv * vw - vv * uw) / den };
    s = s.max(0.0);
    let t = if vv < 1e-12 { 0.0 } else { ((uv * s + vw) / vv).clamp(0.0, 1.0) };
    // (the ray's parameter again for the clamped point on the segment)
    let p = a + v * t;
    let s2 = ((p - o).dot(u) / uu).max(0.0);
    ((o + u * s2 - p).length(), s2)
}

impl App {
    /// The pole the cursor's ray points at, among the player's trolleybus's.
    fn trolley_pole_under_cursor(&self) -> Option<usize> {
        let p = self.player.as_ref()?;
        let g = p.vehicle.trolley.as_deref()?;
        let (o, d, _) = self.cursor_ray_now()?;
        let d = d.as_dvec3().normalize_or_zero();
        let (pos, m, _) = p.vehicle.trolley_frame();
        let rot = DMat3::from_cols(m.x_axis.truncate().as_dvec3(), m.y_axis.truncate().as_dvec3(), m.z_axis.truncate().as_dvec3());
        let mut best: Option<(usize, f64)> = None;
        for (k, pole) in g.rig.poles.iter().enumerate() {
            let (a, b) = (pole.base_at(pos, &rot), pole.tip_at(pos, &rot));
            let (dist, along) = ray_segment(o, d, a, b);
            if along > PICK_REACH || dist > PICK_RADIUS + 0.012 * along {
                continue;
            }
            if best.map_or(true, |(_, bd)| dist < bd) {
                best = Some((k, dist));
            }
        }
        best.map(|b| b.0)
    }

    /// A left press: lets go of the pole in the hand, or takes the one under the cursor (not
    /// from the driver's seat, where the cab's own switches are under the cursor). True when
    /// the press was the pole's.
    pub(crate) fn trolley_mouse_click(&mut self) -> bool {
        let holding = self.player.as_ref().and_then(|p| p.vehicle.trolley.as_deref()).is_some_and(|g| g.grab.is_some());
        if holding {
            if let Some(g) = self.player.as_mut().and_then(|p| p.vehicle.trolley.as_deref_mut()) {
                g.grab = None;
            }
            return true;
        }
        if matches!(self.view.as_str(), "driver" | "pax") {
            return false;
        }
        let Some(k) = self.trolley_pole_under_cursor() else { return false };
        if let Some(g) = self.player.as_mut().and_then(|p| p.vehicle.trolley.as_deref_mut()) {
            g.grab = Some(k);
            g.grab_fresh = true;
        }
        true
    }

    /// While a pole is in the hand: the cursor's ray, every frame.
    pub(crate) fn trolley_mouse_feed(&mut self) {
        let holding = self.player.as_ref().and_then(|p| p.vehicle.trolley.as_deref()).is_some_and(|g| g.grab.is_some());
        if !holding {
            return;
        }
        let ray = self.cursor_ray_now().map(|(o, d, _)| (o, d.as_dvec3().normalize_or_zero()));
        // (from the driver's seat the poles are behind the head and the wire is out of the cursor's
        // reach: the motor raises the pole to the wire by itself)
        let from_cab = matches!(self.view.as_str(), "driver" | "pax");
        if let Some(g) = self.player.as_mut().and_then(|p| p.vehicle.trolley.as_deref_mut()) {
            g.grab_auto = from_cab;
            if let Some(r) = ray {
                g.grab_ray = Some(r);
                g.grab_fresh = true;
            }
        }
        if self.service_msg.is_none() && !from_cab {
            self.service_msg = Some(("Move the mouse to put the pole on the wire, click to let it go".into(), 0.6));
        }
    }
}
