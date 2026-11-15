use glam::DVec3;

pub(super) const EYE_FORWARD: f64 = 0.2;
pub(super) const FOOT_NEAR: f32 = 0.1;
const HORIZONTAL_FOLLOW_RATE: f64 = 48.0;
const MAX_HORIZONTAL_LAG: f64 = 0.08;

pub(super) fn horizontal_follow_k(dt: f64, speed: f64) -> f64 {
    let rate = HORIZONTAL_FOLLOW_RATE.max(speed.max(0.0) / MAX_HORIZONTAL_LAG);
    1.0 - (-dt * rate).exp()
}

pub(super) fn follow_eye(
    previous: Option<DVec3>,
    want: DVec3,
    horizontal_k: f64,
    vertical_k: f64,
) -> DVec3 {
    match previous.filter(|eye| (*eye - want).length() < 60.0) {
        Some(eye) => DVec3::new(
            eye.x + (want.x - eye.x) * horizontal_k,
            eye.y + (want.y - eye.y) * horizontal_k,
            eye.z + (want.z - eye.z) * vertical_k,
        ),
        None => want,
    }
}
