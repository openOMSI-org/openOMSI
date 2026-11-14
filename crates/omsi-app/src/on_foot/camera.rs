use glam::DVec3;

pub(super) const EYE_FORWARD: f64 = 0.2;
pub(super) const FOOT_NEAR: f32 = 0.25;

pub(super) fn follow_eye(previous: Option<DVec3>, want: DVec3, horizontal_k: f64, vertical_k: f64) -> DVec3 {
    match previous.filter(|eye| (*eye - want).length() < 60.0) {
        Some(eye) => DVec3::new(
            eye.x + (want.x - eye.x) * horizontal_k,
            eye.y + (want.y - eye.y) * horizontal_k,
            eye.z + (want.z - eye.z) * vertical_k,
        ),
        None => want,
    }
}
