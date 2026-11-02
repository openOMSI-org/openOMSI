//! Looking round: the head's direction per view, the zoom, and the camera's easing.

use super::*;

impl App {
    /// Turn the view by (dx, dy) degrees, as dragging with the right button does: the free
    /// camera turns, inside the bus the head turns, outside the camera swings around it.
    /// Keeps `look` with the view it belongs to: on a change of view the direction of the
    /// view left is put away and the one of the view entered comes back (straight ahead
    /// the first time).
    pub(crate) fn sync_view_look(&mut self) {
        sync_view_look(&mut self.cam, &self.view, self.player.as_ref());
    }

    /// Which camera the look belongs to: the view, and for the driver's and the passengers'
    /// view the camera chosen in it. Each of Omsi.exe's cameras keeps where it was turned
    /// (a `TCamera` has its own yaw and pitch besides the file's, 0x7edde4 resets them): the
    /// look went back to straight ahead whenever the viewpoint changed.
    pub(crate) fn look_key(&self) -> String {
        look_key_of(&self.view, self.player.as_ref().map(|p| p.cam_choice))
    }

    /// Zoom the view inside the bus by `notches` of the mouse wheel (in: positive).
    pub(crate) fn zoom_by(&mut self, notches: f32) {
        zoom_view(&mut self.cam, &self.view, notches);
    }

    pub(crate) fn look_by(&mut self, dx: f32, dy: f32) {
        self.sync_view_look();
        // a hand on the view cancels an eased Space return.
        self.cam.f1_reset = None;
        if self.view == "foot" {
            self.foot_look(dx, dy);
            return;
        }
        if self.view == "free" || self.player.is_none() {
            if let Some(cam) = self.camera.as_mut() {
                cam.yaw = (cam.yaw + dx).rem_euclid(360.0);
                cam.pitch = (cam.pitch - dy).clamp(-89.0, 89.0);
            }
        } else if self.view == "outside" {
            // F3 chase orbit: full turn in yaw; pitch stops between near
            // top-down and just below eye level so the camera never swings
            // under the bus (see `chase_orbit_step` for the mouse gain).
            self.cam.look.0 = (self.cam.look.0 + dx).rem_euclid(360.0);
            self.cam.look.1 = (self.cam.look.1 - dy).clamp(-60.0, 25.0);
        } else {
            self.cam.look.0 = cab_look_yaw(self.cam.look.0 + dx);
            self.cam.look.1 = (self.cam.look.1 - dy).clamp(-85.0, 85.0);
        }
    }
}

/// F3 chase orbit step from raw drag pixels: full turn in yaw at 0.35
/// deg/px (faster than the head's 0.15), pitch between -60 (near top-down)
/// and +25 (just below eye level) around the -15 rest pose, so the camera
/// never swings under the bus. Pure (tested below).
pub(crate) fn chase_orbit_step(yaw: f32, pitch: f32, dx_px: f32, dy_px: f32) -> (f32, f32) {
    const GAIN: f32 = 0.35;
    (
        (yaw + dx_px * GAIN).rem_euclid(360.0),
        (pitch - dy_px * GAIN).clamp(-60.0, 25.0),
    )
}

/// Precision zoom step from a vertical drag: the zoom state `z` (0 wide ..
/// 1 full zoom) travels at `intent` per 364 px, and the FOV multiplier is
/// `1/(1+5.5*z)` - full zoom ~6.5x in. Drag down (`dy > 0`) zooms in.
/// Never past 1.0 (never wider than the bus's own field of view); the floor
/// is the caller's clamp. Pure (tested below).
pub(crate) fn precision_zoom_step(mult: f32, dy_px: f32, intent: f32) -> f32 {
    const RANGE: f32 = 5.5;
    const FULL_DRAG_PX: f32 = 364.0;
    let z = ((1.0 / mult.max(0.154) - 1.0) / RANGE).clamp(0.0, 1.0);
    let z2 = (z + dy_px * intent / FULL_DRAG_PX).clamp(0.0, 1.0);
    1.0 / (1.0 + RANGE * z2)
}

/// F1 zoom intent: the head zoom runs 20% slower than outside/free.
pub(crate) const ZOOM_INTENT_F1: f32 = 0.56;
/// Outside/free zoom intent: a full 364 px drag takes `z` 0 to 0.70.
pub(crate) const ZOOM_INTENT: f32 = 0.70;

/// Eased Space return for the F1 head: look and zoom glide home on the same
/// curve as the viewpoint switch instead of teleporting (`cam_approach::home`).
/// `t` seconds in; returns the current look, zoom and done.
pub(crate) fn reset_blend(look_from: (f32, f32), zoom_from: f32, t: f32) -> ((f32, f32), f32, bool) {
    crate::cam_approach::home(look_from, zoom_from, t)
}

#[cfg(test)]
mod look_tests {
    #[test]
    fn chase_orbits_at_035_deg_px_with_stops_above_and_below() {
        // 100 px drag down-right: +35 yaw, -35 pitch.
        let (y, p) = super::chase_orbit_step(0.0, 0.0, 100.0, 100.0);
        assert!((y - 35.0).abs() < 1e-4 && (p + 35.0).abs() < 1e-4, "{y} {p}");
        // yaw wraps the full circle.
        assert!((super::chase_orbit_step(350.0, 0.0, 100.0, 0.0).0 - 25.0).abs() < 1e-3);
        // pitch never leaves the stops, whichever way it is dragged.
        assert_eq!(super::chase_orbit_step(0.0, 0.0, 0.0, -1000.0).1, 25.0);
        assert_eq!(super::chase_orbit_step(0.0, 0.0, 0.0, 1000.0).1, -60.0);
    }

    #[test]
    fn space_return_eases_home_like_the_viewpoint_switch() {
        // start: untouched; partway: well on the way (ease-out); end: exact and done.
        let (look, zoom, done) = super::reset_blend((30.0, -10.0), 0.5, 0.0);
        assert_eq!((look, zoom, done), ((30.0, -10.0), 0.5, false));
        let (look, zoom, done) = super::reset_blend((30.0, -10.0), 0.5, 0.27);
        assert!(look.0 > 3.0 && look.0 < 27.0 && zoom > 0.5 && zoom < 1.0 && !done);
        let (look, zoom, done) = super::reset_blend((30.0, -10.0), 0.5, 2.0);
        assert_eq!((look, zoom, done), ((0.0, 0.0), 1.0, true));
        assert!(super::reset_blend((30.0, -10.0), 0.5, 5.0).2);
    }

    #[test]
    fn the_zoom_key_zooms_while_held_or_until_pressed_again() {
        let mut z = super::ZoomHold::default();
        for _ in 0..60 { z.step(true, 0.5, false, 1.0 / 60.0); }
        assert_eq!(z.factor, 0.5, "held: zoomed in");
        for _ in 0..60 { z.step(false, 0.5, false, 1.0 / 60.0); }
        assert_eq!(z.factor, 1.0, "let go: back");
        z.step(true, 0.4, true, 1.0 / 60.0);
        for _ in 0..60 { z.step(false, 0.4, true, 1.0 / 60.0); }
        assert_eq!(z.factor, 0.4, "latching: stays after one press");
        z.step(true, 0.4, true, 1.0 / 60.0);
        for _ in 0..60 { z.step(false, 0.4, true, 1.0 / 60.0); }
        assert_eq!(z.factor, 1.0, "a second press zooms out");
    }

    #[test]
    fn precision_zoom_follows_the_fov_curve_and_never_widens() {
        // a full 364 px drag down takes z 0 to 0.70: m = 1/(1+5.5*0.70).
        let m = super::precision_zoom_step(1.0, 364.0, super::ZOOM_INTENT);
        assert!((m - 1.0 / (1.0 + 5.5 * 0.70)).abs() < 1e-4, "{m}");
        // drag down zooms in, drag up undoes it, never past 1.0.
        let mid = super::precision_zoom_step(1.0, 100.0, super::ZOOM_INTENT);
        assert!(mid < 1.0 && mid > 0.45, "{mid}");
        assert!((super::precision_zoom_step(mid, -100.0, super::ZOOM_INTENT) - 1.0).abs() < 1e-4);
        assert_eq!(super::precision_zoom_step(1.0, -50.0, super::ZOOM_INTENT), 1.0);
        // F1 runs the same curve 20% slower.
        let slow = super::precision_zoom_step(1.0, 100.0, super::ZOOM_INTENT_F1);
        assert!(slow > mid && slow < 1.0, "{slow} vs {mid}");
    }
}

/// `App::sync_view_look` for where `self` is borrowed in parts.
/// See `App::look_key`.
/// Whether `key` is `look_key_of(view, cam)` (without making that key: asked every frame).
pub(crate) fn look_key_is(key: &str, view: &str, cam: Option<(usize, usize)>) -> bool {
    let numbered = |prefix: &str, n: usize| {
        key.strip_prefix(prefix).and_then(|k| k.strip_prefix('#')).is_some_and(|k| {
            let mut digits = [0u8; 20];
            let mut at = digits.len();
            let mut v = n;
            loop {
                at -= 1;
                digits[at] = b'0' + (v % 10) as u8;
                v /= 10;
                if v == 0 {
                    break;
                }
            }
            k.as_bytes() == &digits[at..]
        })
    };
    match (view, cam) {
        ("driver", Some((d, _))) => numbered("driver", d),
        ("pax", Some((_, x))) => numbered("pax", x),
        _ => key == view,
    }
}

/// [`App::zoom_by`] on the view's state `cam` and the view `view`.
pub(crate) fn with_zoom_hold(zoom: Option<f32>, hold: f32) -> Option<f32> {
    if (hold - 1.0).abs() < 1e-3 { zoom } else { Some(zoom.unwrap_or(1.0) * hold) }
}

pub(crate) struct ZoomHold {
    pub(crate) factor: f32,
    latched: bool,
    was: bool,
}

impl Default for ZoomHold {
    fn default() -> Self {
        ZoomHold { factor: 1.0, latched: false, was: false }
    }
}

impl ZoomHold {
    pub(crate) fn step(&mut self, held: bool, amount: f32, latch: bool, dt: f32) {
        if held && !self.was {
            self.latched = latch && !self.latched;
        }
        self.was = held;
        let on = if latch { self.latched } else { held };
        let target = if on { amount.clamp(0.15, 1.0) } else { 1.0 };
        self.factor += (target - self.factor) * (1.0 - (-dt / 0.08).exp());
        if (self.factor - target).abs() < 1e-3 {
            self.factor = target;
        }
    }
}

pub(crate) fn zoom_view(cam: &mut ViewState, view: &str, notches: f32) {
    // a hand on the zoom cancels an eased Space return.
    cam.f1_reset = None;
    let z = cam.view_zoom.entry(view.to_string()).or_insert(1.0);
    *z = (*z * (1.0 - 0.08 * notches.clamp(-5.0, 5.0))).clamp(0.2, 1.6);
}

/// [`App::sync_view_look`] on the view's state `cam`, the view `view` and the bus driven.
pub(crate) fn sync_view_look(cam: &mut ViewState, view: &str, player: Option<&Player>) {
    let key = look_key_of(view, player.map(|p| p.cam_choice));
    let left = cam.look_view.clone();
    // A change of view is not a turn of the head: the direction of the view entered is
    // where the head already is, so the angle the camera is drawn at starts there as
    // well (a glide belongs between angles of one and the same view).
    if swap_view_look(&mut cam.look, &mut cam.view_looks, &mut cam.look_view, &key) {
        cam.look_smooth = cam.look;
        swap_cam_zoom(&mut cam.view_zoom, &mut cam.cam_zooms, &left, &key, view);
    }
}

pub(crate) fn look_key_of(view: &str, cam: Option<(usize, usize)>) -> String {
    match (view, cam) {
        ("driver", Some((d, _))) => format!("driver#{d}"),
        ("pax", Some((_, x))) => format!("pax#{x}"),
        _ => view.to_string(),
    }
}

/// How far the head turns inside the bus: all the way round, in the driver's seat as in a
/// passenger's - Omsi.exe's mouse look (0x82c5f8) adds the cursor's way to the camera's
/// yaw with no stop. Capped at 140 degrees each way, a quarter of the coach stayed out of
/// sight (#909). The turn is kept within -180..180 so that letting go of a glance still
/// swings the short way back.
pub(crate) fn cab_look_yaw(yaw: f32) -> f32 {
    (yaw + 180.0).rem_euclid(360.0) - 180.0
}

/// The zoom of a camera changed to (`left` -> `key`, look keys): the camera left keeps its
/// own, the one taken gets its own back. It was one zoom for every camera of a view (#2183).
pub(crate) fn swap_cam_zoom(
    view_zoom: &mut std::collections::HashMap<String, f32>,
    cam_zooms: &mut std::collections::HashMap<String, f32>,
    left: &str,
    key: &str,
    view: &str,
) {
    if let Some((left_view, _)) = left.split_once('#') {
        match view_zoom.get(left_view).copied() {
            Some(z) => cam_zooms.insert(left.to_string(), z),
            None => cam_zooms.remove(left),
        };
    }
    if key.contains('#') {
        match cam_zooms.get(key).copied() {
            Some(z) => view_zoom.insert(view.to_string(), z),
            None => view_zoom.remove(view),
        };
    }
}

/// Put the direction of the view left away and take up the one of the view entered; `true`
/// when that was another view (the caller then knows the head did not just move: it is where
/// the view entered last left it).
pub(crate) fn swap_view_look(look: &mut (f32, f32), looks: &mut std::collections::HashMap<String, (f32, f32)>, look_view: &mut String, view: &str) -> bool {
    if look_view == view {
        return false;
    }
    let old = std::mem::replace(look_view, view.to_string());
    if !old.is_empty() {
        looks.insert(old, *look);
    }
    *look = looks.get(view).copied().unwrap_or((0.0, 0.0));
    true
}

/// Ease the angle the view is drawn at out of the head's own angle, once a frame, and give it
/// back: `shown` keeps the angle of the frame before, `ms` is the time constant
/// (`look_smoothing_ms`, 0 = the view is drawn where the head points at once).
///
/// The step is `1 - e^(-dt/tau)`, so 30 and 300 frames a second smooth alike (a plain
/// fraction of the way would smooth the faster machine harder) and the yaw goes the short way
/// round. Off, the angle is taken over at once, which is also what puts the view back in step
/// after a change of view or a `look` a script set whole.
///
/// A free function: it is called where `self` is borrowed in parts, and needs nothing of the
/// app but the two angles.
pub(crate) fn ease_look(shown: &mut (f32, f32), wanted: (f32, f32), dt: f32, ms: f32) -> (f32, f32) {
    let tau = ms * 0.001;
    if !(tau > 0.0) || !dt.is_finite() || dt <= 0.0 {
        *shown = wanted;
        return wanted;
    }
    let f = 1.0 - (-dt / tau).exp();
    let dyaw = (wanted.0 - shown.0 + 180.0).rem_euclid(360.0) - 180.0;
    // (in the range the head's yaw is kept in: -180..180 in the cab, `cab_look_yaw`, else
    // 0..360 - a glide across the back would otherwise hand the cab a yaw of 350)
    let yaw = shown.0 + dyaw * f;
    shown.0 = if (-180.0..=180.0).contains(&wanted.0) { (yaw + 180.0).rem_euclid(360.0) - 180.0 } else { yaw.rem_euclid(360.0) };
    shown.1 += (wanted.1 - shown.1) * f;
    *shown
}

#[cfg(test)]
mod look_smoothing_tests {
    use super::ease_look;

    /// A cab's yaw (-180..180) eased across the back stays in its range: from 170 to -170 it
    /// goes through 180, not out to 190 or round to 350.
    #[test]
    fn a_cab_yaw_stays_within_half_a_turn() {
        let mut shown = (170.0, 0.0);
        for _ in 0..30 {
            let (yaw, _) = ease_look(&mut shown, (-170.0, 0.0), 1.0 / 60.0, 100.0);
            assert!((-180.0..=180.0).contains(&yaw), "{yaw}");
            assert!(yaw >= 170.0 || yaw <= -170.0, "the short way round: {yaw}");
        }
    }

    /// The step is `1 - e^(-dt/tau)`: a machine drawing 600 frames a second glides exactly
    /// as far in a second as one drawing 60 (a plain fraction of the way would smooth the
    /// faster of the two harder, and the same setting would feel different on a faster PC).
    #[test]
    fn the_glide_does_not_depend_on_the_frame_rate() {
        let (from, to) = ((0.0, 0.0), (30.0, 10.0));
        let mut slow = from;
        for _ in 0..60 {
            ease_look(&mut slow, to, 1.0 / 60.0, 100.0);
        }
        let mut fast = from;
        for _ in 0..600 {
            ease_look(&mut fast, to, 1.0 / 600.0, 100.0);
        }
        assert!((slow.0 - fast.0).abs() < 0.01 && (slow.1 - fast.1).abs() < 0.01, "{slow:?} against {fast:?}");
        // a whole second at a time constant of 100 ms is very nearly there
        assert!(slow.0 > 29.9 && slow.1 > 9.9, "{slow:?}");
    }

    /// A head turned a degree past north eases a degree forward, not 359 back.
    #[test]
    fn the_glide_goes_the_short_way_round() {
        let mut shown = (359.0, 0.0);
        let there = ease_look(&mut shown, (1.0, 0.0), 0.05, 100.0);
        assert!((there.0 - 359.0).rem_euclid(360.0) < 1.0, "{there:?}");
    }

    /// Off, the view is drawn exactly where the head points, at once: what every machine
    /// had before this was a setting.
    #[test]
    fn without_the_setting_the_view_is_where_the_head_points() {
        let mut shown = (0.0, 0.0);
        assert_eq!(ease_look(&mut shown, (30.0, 10.0), 1.0 / 60.0, 0.0), (30.0, 10.0));
        assert_eq!(shown, (30.0, 10.0));
    }
}
#[cfg(test)]
mod cab_look_tests {
    use super::cab_look_yaw;

    /// A passenger and the driver turn all the way round, as in Omsi.exe (#909).
    #[test]
    fn the_head_turns_all_the_way_round() {
        let mut yaw = 0.0;
        for _ in 0..40 {
            yaw = cab_look_yaw(yaw + 10.0);
        }
        // 400 degrees turned: 40 past straight ahead, the short way
        assert!((yaw - 40.0).abs() < 1e-3, "{yaw}");
        assert!((cab_look_yaw(170.0 + 20.0) + 170.0).abs() < 1e-3);
        // (the driver looks back down the saloon: no stop at 140 degrees)
        assert!((cab_look_yaw(175.0) - 175.0).abs() < 1e-3);
        assert!((cab_look_yaw(-160.0) + 160.0).abs() < 1e-3);
    }
}

#[cfg(test)]
mod look_key_tests {
    use super::{look_key_is, look_key_of};

    #[test]
    fn look_key_is_the_key_made() {
        let views = ["driver", "pax", "free", "foot", "outside", ""];
        let cams = [None, Some((0, 0)), Some((1, 12)), Some((10, 3)), Some((usize::MAX, 7))];
        let keys = ["driver#0", "driver#1", "driver#10", "driver#01", "pax#12", "pax#3", "pax#03", "driver", "pax", "free", "", "driver#", "pax#7"];
        for v in views {
            for c in cams {
                assert!(look_key_is(&look_key_of(v, c), v, c), "{v} {c:?}");
                for k in keys {
                    assert_eq!(look_key_is(k, v, c), k == look_key_of(v, c), "{k} {v} {c:?}");
                }
            }
        }
    }
}

#[cfg(test)]
mod zoom_tests {
    use super::swap_cam_zoom;
    use std::collections::HashMap;

    /// Each cab camera keeps its zoom: zoomed in the right-hand view, the front view taken
    /// again comes back unzoomed, and the right-hand one zoomed (#2183).
    #[test]
    fn each_cab_camera_keeps_its_zoom() {
        let (mut zoom, mut kept) = (HashMap::new(), HashMap::new());
        swap_cam_zoom(&mut zoom, &mut kept, "driver#0", "driver#1", "driver");
        zoom.insert("driver".to_string(), 0.5);
        swap_cam_zoom(&mut zoom, &mut kept, "driver#1", "driver#0", "driver");
        assert_eq!(zoom.get("driver"), None);
        swap_cam_zoom(&mut zoom, &mut kept, "driver#0", "driver#1", "driver");
        assert_eq!(zoom.get("driver"), Some(&0.5));
    }
}
