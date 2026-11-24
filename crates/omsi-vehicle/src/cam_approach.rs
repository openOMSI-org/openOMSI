//! The glide of the cab's view to another camera, as Omsi.exe makes it: every value of the
//! camera - the eye's three coordinates, the turn and tilt of the view, the field of view
//! and the distance - comes closer to the camera taken by the same share of what is left of
//! the way each frame, `exp(-0.005 * dt)` of it left over with `dt` the frame's time in
//! milliseconds (0x7ee17c; the time constant is 200 ms). Fast off the mark, settling
//! softly, and a frame that comes late only goes further on the same curve - no more than
//! 200 ms of it, so a hitch does not jump to the end.

/// How much of the way is left after `dt` seconds (one frame, at most `MAX_DT` of it).
pub fn left_after(dt: f32) -> f32 {
    (-PER_MS * dt.clamp(0.0, MAX_DT) * 1000.0).exp()
}

/// Omsi.exe's rate: the share of the way left over falls by e every 200 ms.
pub const PER_MS: f32 = 0.005;
/// The longest frame the glide counts (seconds).
pub const MAX_DT: f32 = 0.2;

fn wrap_deg(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

/// `cur` one frame of `dt` seconds closer to `target`, value by value (both cameras fixed in
/// the bus's frame; the bus's motion and the head go on top afterwards). The view turns the
/// short way round.
pub fn approach(cur: &crate::Camera, target: &crate::Camera, dt: f32) -> crate::Camera {
    let rest = left_after(dt);
    let l = |x: f32, y: f32| y - (y - x) * rest;
    crate::Camera {
        pos: [
            l(cur.pos[0], target.pos[0]),
            l(cur.pos[1], target.pos[1]),
            l(cur.pos[2], target.pos[2]),
        ],
        dist: l(cur.dist, target.dist),
        fov: l(cur.fov, target.fov),
        yaw: target.yaw - wrap_deg(target.yaw - cur.yaw) * rest,
        pitch: l(cur.pitch, target.pitch),
        extra: target.extra,
        fixed: target.fixed,
    }
}

/// Whether `cur` is as good as `target`: under a millimetre, a hundredth of a degree and a
/// hundredth of the field of view away - what is left goes to the hand-over's carry.
pub fn settled(cur: &crate::Camera, target: &crate::Camera) -> bool {
    let [x, y, z] = [0, 1, 2].map(|i| cur.pos[i] - target.pos[i]);
    (x * x + y * y + z * z).sqrt() < 1e-3
        && (cur.dist - target.dist).abs() < 1e-3
        && (cur.fov - target.fov).abs() < 0.01
        && wrap_deg(cur.yaw - target.yaw).abs() < 0.01
        && (cur.pitch - target.pitch).abs() < 0.01
}

/// The F1 / Space return of the head's turn and the zoom, on the same curve: `t` the glide's
/// time so far (each frame's share capped at `MAX_DT`, see `step`); the look, the zoom and
/// whether it has arrived.
pub fn home(look_from: (f32, f32), zoom_from: f32, t: f32) -> ((f32, f32), f32, bool) {
    let rest = (-PER_MS * t.max(0.0) * 1000.0).exp();
    let look = (look_from.0 * rest, look_from.1 * rest);
    let zoom = 1.0 + (zoom_from - 1.0) * rest;
    let done = look.0.abs() < 0.01 && look.1.abs() < 0.01 && (zoom - 1.0).abs() < 1e-4;
    if done {
        ((0.0, 0.0), 1.0, true)
    } else {
        (look, zoom, false)
    }
}

/// The time a frame of `dt` seconds adds to `home`'s glide.
pub fn step(dt: f32) -> f32 {
    dt.clamp(0.0, MAX_DT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam(x: f32, yaw: f32, fov: f32) -> crate::Camera {
        crate::Camera {
            pos: [x, 0.0, 1.8],
            dist: 0.0,
            fov,
            yaw,
            pitch: 0.0,
            extra: None,
            fixed: false,
        }
    }

    #[test]
    fn each_value_closes_by_omsis_share_per_frame() {
        // 1 m and 20 degrees away, 60 frames a second: after one frame 1 - e^(-1/12) of
        // the way is gone, after 200 ms (twelve frames) all but 1/e
        let (from, to) = (cam(1.0, 20.0, 50.0), cam(0.0, 0.0, 60.0));
        let one = approach(&from, &to, 1.0 / 60.0);
        let k = (-1.0f32 / 12.0).exp();
        assert!(
            (one.pos[0] - k).abs() < 1e-5
                && (one.yaw - 20.0 * k).abs() < 1e-4
                && (one.fov - (60.0 - 10.0 * k)).abs() < 1e-4
        );
        let mut c = from.clone();
        for _ in 0..12 {
            c = approach(&c, &to, 1.0 / 60.0);
        }
        assert!((c.pos[0] - (-1.0f32).exp()).abs() < 1e-4, "{}", c.pos[0]);
        assert!(!settled(&c, &to));
        for _ in 0..240 {
            c = approach(&c, &to, 1.0 / 60.0);
        }
        assert!(settled(&c, &to));
    }

    #[test]
    fn a_long_frame_counts_no_more_than_200_ms() {
        let (from, to) = (cam(1.0, 0.0, 60.0), cam(0.0, 0.0, 60.0));
        assert_eq!(
            approach(&from, &to, 2.0).pos[0],
            approach(&from, &to, 0.2).pos[0]
        );
        assert!((left_after(0.2) - (-1.0f32).exp()).abs() < 1e-6);
    }

    #[test]
    fn the_view_turns_the_short_way_round() {
        let c = approach(&cam(0.0, 170.0, 60.0), &cam(0.0, -170.0, 60.0), 0.2);
        let yaw = c.yaw.rem_euclid(360.0);
        assert!(yaw > 170.0 && yaw < 190.0, "{}", c.yaw);
    }

    #[test]
    fn f1_returns_home_on_the_same_curve() {
        assert_eq!(home((30.0, -10.0), 0.5, 0.0), ((30.0, -10.0), 0.5, false));
        let (look, zoom, done) = home((30.0, -10.0), 0.5, 0.2);
        assert!(
            (look.0 - 30.0 * (-1.0f32).exp()).abs() < 1e-3 && zoom > 0.5 && zoom < 1.0 && !done
        );
        assert_eq!(home((30.0, -10.0), 0.5, 5.0), ((0.0, 0.0), 1.0, true));
        assert_eq!(step(1.0), 0.2);
    }
}
