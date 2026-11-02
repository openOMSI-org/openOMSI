//! Ray intersection with physical vehicle bodies.

use glam::DVec3;
use crate::collision::Obb;

pub fn ray_box(origin: DVec3, direction: DVec3, b: &Obb) -> Option<f64> {
    let [right, forward] = b.axes();
    let relative = origin.truncate() - b.center;
    let o = [relative.dot(right), relative.dot(forward), origin.z];
    let d = [
        direction.truncate().dot(right),
        direction.truncate().dot(forward),
        direction.z,
    ];
    let lo = [-b.half.x, -b.half.y, b.z0];
    let hi = [b.half.x, b.half.y, b.z1];
    let (mut near, mut far) = (0.0f64, 2000.0f64);
    for i in 0..3 {
        if d[i].abs() < 1e-9 {
            if o[i] < lo[i] || o[i] > hi[i] {
                return None;
            }
        } else {
            let (a, z) = ((lo[i] - o[i]) / d[i], (hi[i] - o[i]) / d[i]);
            near = near.max(a.min(z));
            far = far.min(a.max(z));
            if near > far {
                return None;
            }
        }
    }
    Some(near)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_ray_hits_the_nearest_face_and_respects_height() {
        let b = Obb::from_box(
            [2.5, 12.0, 3.0, 0.0, 0.0, 1.5],
            DVec3::new(0.0, 20.0, 0.0),
            0.0,
        );
        assert_eq!(ray_box(DVec3::new(0.0, 0.0, 1.0), DVec3::Y, &b), Some(14.0));
        assert!(ray_box(DVec3::new(0.0, 0.0, 5.0), DVec3::Y, &b).is_none());
        assert!(ray_box(DVec3::new(0.0, 0.0, 1.0), -DVec3::Y, &b).is_none());
    }

}
