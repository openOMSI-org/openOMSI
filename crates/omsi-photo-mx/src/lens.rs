//! The photo camera's lens: its focal length as a field of view (a full-frame camera's, 24 mm
//! high), and the pictures one photo is made of - each taken from another point of the
//! aperture, all aimed at the same plane of focus (what is on it stays sharp, the rest
//! blurs as much as a real lens of that focal length and f-number blurs it), a little
//! turned within the pixel (the edges smoothed), at another moment of the shutter.

use glam::{DVec3, Vec3};
use omsi_render::Camera;

/// A full-frame sensor's height (mm): the field of view and the focal length go together
/// through it, as a 35 mm camera's lens is named.
const SENSOR_H_MM: f32 = 24.0;

/// The focal length (mm) of a vertical field of view (degrees).
pub fn focal_mm(fov_deg: f32) -> f32 {
    SENSOR_H_MM * 0.5 / (fov_deg.to_radians() * 0.5).tan().max(1e-4)
}

/// The vertical field of view (degrees) of a focal length (mm).
pub fn fov_for(focal_mm: f32) -> f32 {
    (2.0 * (SENSOR_H_MM * 0.5 / focal_mm.max(1.0)).atan()).to_degrees()
}

/// The `i`th number of the Halton sequence of `base` (0..1): points that fill the aperture
/// and the shutter evenly however many are taken.
pub fn halton(mut i: u32, base: u32) -> f32 {
    let mut f = 1.0f32;
    let mut r = 0.0f32;
    i += 1;
    while i > 0 {
        f /= base as f32;
        r += f * (i % base) as f32;
        i /= base;
    }
    r
}

/// A point of the aperture (unit radius) for the square's point (u, v): the disc evenly
/// (Shirley's concentric mapping), or with `blades` (5 and more) the polygon the blades
/// close to - the shape every out-of-focus light takes.
pub fn aperture_point(u: f32, v: f32, blades: u32) -> (f32, f32) {
    let (a, b) = (2.0 * u - 1.0, 2.0 * v - 1.0);
    if a == 0.0 && b == 0.0 {
        return (0.0, 0.0);
    }
    let (r, phi) = if a.abs() > b.abs() { (a, std::f32::consts::FRAC_PI_4 * (b / a)) } else { (b, std::f32::consts::FRAC_PI_2 - std::f32::consts::FRAC_PI_4 * (a / b)) };
    let mut r = r;
    if blades >= 5 {
        // the polygon's edge at this angle, as a share of its corners' radius
        let seg = std::f32::consts::TAU / blades as f32;
        let a = (phi + std::f32::consts::FRAC_PI_2).rem_euclid(seg) - seg * 0.5;
        r *= (seg * 0.5).cos() / a.cos();
    }
    (r * phi.cos(), r * phi.sin())
}

/// How the photo is taken.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    /// Depth of field: the aperture opens to `fstop`, focused at `focus` metres.
    pub dof: bool,
    pub fstop: f32,
    pub focus: f32,
    /// 0: a round aperture; 5 and more: that many blades.
    pub blades: u32,
    /// The bus's velocity (m/s, world) and how long the shutter is open (s): the bus and the
    /// camera travel together while the world passes (a rolling shot).
    pub motion: Option<(Vec3, f32)>,
    /// The camera stands on a tripod while the shutter is open: the bus moves through the
    /// picture (blurred) and the street stays sharp - else it pans with the bus.
    pub tripod: bool,
    /// The sensor's height (mm): with the field of view it gives the lens's real focal
    /// length, and so how wide the aperture opens at an f-number.
    pub sensor_h: f32,
    /// The lens tilted against the sensor (degrees: about the frame's horizontal axis, then
    /// its vertical one): the plane of focus leans (Scheimpflug).
    pub tilt: [f32; 2],
    /// The lens shifted over the sensor (shares of the frame's height: right, up): the view
    /// moves without the camera turning, the verticals stay upright.
    pub shift: [f32; 2],
    /// How much brighter than the picture's white the lights burnt out in it are taken to be
    /// (the pictures are 8 bit: a lamp is as white as a white wall) - out of focus they
    /// spread into the bright discs a lens makes of them.
    pub highlights: f32,
}

impl Default for Shot {
    fn default() -> Shot {
        Shot { dof: false, fstop: 2.8, focus: 10.0, blades: 0, motion: None, tripod: false, sensor_h: SENSOR_H_MM, tilt: [0.0; 2], shift: [0.0; 2], highlights: 0.0 }
    }
}

/// The real focal length (mm) of a vertical field of view on a sensor `sensor_h` mm high.
pub fn focal_on(fov_deg: f32, sensor_h: f32) -> f32 {
    sensor_h * 0.5 / (fov_deg.to_radians() * 0.5).tan().max(1e-4)
}

/// The aperture's radius in metres: the real focal length over twice the f-number.
pub fn aperture_radius(fov_deg: f32, shot: &Shot) -> f32 {
    focal_on(fov_deg, shot.sensor_h) / 1000.0 / (2.0 * shot.fstop.max(0.7))
}

/// One picture of a photo: the camera it is taken with, the moment of the shutter (s from
/// its middle) and the point of the aperture (m: right, up) it looks through.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub cam: Camera,
    pub t: f32,
    pub aperture: glam::Vec2,
}

/// The inverse of the distance (along the view) of the plane of focus at the view direction
/// (u, v) (tangents: right, up), for a lens of `focal_m` metres: the focus distance where
/// the lens is not tilted. A lens tilted by an angle turns the plane of focus about a hinge
/// line `focal / sin(tilt)` below it (Scheimpflug) - tilted down, through the hinge under the
/// camera and the point of focus, the plane lies along the ground.
pub fn inv_focus_at(shot: &Shot, focal_m: f32, u: f32, v: f32) -> f32 {
    let (sv, su) = (shot.tilt[0].to_radians().sin(), shot.tilt[1].to_radians().sin());
    1.0 / shot.focus.max(0.05) - (v * sv + u * su) / focal_m.max(1e-3)
}

/// The radius (pixels) of the blur circle of a point `z` metres away along the view, at the
/// view direction (u, v): `pix` is the tangent a pixel spans.
pub fn coc_px(shot: &Shot, radius_m: f32, focal_m: f32, z: f32, u: f32, v: f32, pix: f32) -> f32 {
    if !shot.dof {
        return 0.0;
    }
    radius_m * (1.0 / z.max(0.05) - inv_focus_at(shot, focal_m, u, v)).abs() / pix.max(1e-9)
}

/// The `i`th picture of a photo: the camera moved over the aperture and aimed back at the
/// plane of focus, turned within the pixel (`height_px`: the picture's height), and the
/// moment of the shutter (seconds from its middle) it is taken at.
pub fn sample(base: &Camera, i: u32, shot: &Shot, height_px: f32) -> Sample {
    let mut cam = *base;
    let f = base.forward();
    let (right, up) = (base.right(), base.up());
    let mut dir = f;
    let mut aperture = glam::Vec2::ZERO;
    if shot.dof && i > 0 {
        let radius = aperture_radius(base.fov_deg, shot);
        let (ax, ay) = aperture_point(halton(i, 2), halton(i, 3), shot.blades);
        aperture = glam::Vec2::new(ax, ay) * radius;
        let off = right * aperture.x + up * aperture.y;
        let focus = f * shot.focus.max(0.05);
        cam.position = base.position + off.as_dvec3();
        dir = (focus - off).normalize_or(f);
    }
    // within the pixel (the first picture straight)
    if i > 0 {
        let k = 2.0 * (base.fov_deg.to_radians() * 0.5).tan() / height_px.max(1.0);
        // (half a pixel across: the edges smoothed, the detail kept - a whole pixel's box
        // softened the photo next to the live picture)
        let (jx, jy) = ((halton(i, 5) - 0.5) * 0.5, (halton(i, 7) - 0.5) * 0.5);
        dir = (dir + right * (jx * k) + up * (jy * k)).normalize_or(dir);
    }
    cam.yaw = dir.x.atan2(dir.y).to_degrees();
    cam.pitch = dir.z.clamp(-1.0, 1.0).asin().to_degrees();
    let t = match shot.motion {
        // (the golden ratio's sequence: the moments evenly over the shutter's time, apart
        // from the aperture's and the pixel's points)
        Some((_, shutter)) if i > 0 => ((i as f32 * 0.618_034).fract() - 0.5) * shutter,
        _ => 0.0,
    };
    Sample { cam, t, aperture }
}

/// Where the bus is moved for a picture taken `t` seconds from the shutter's middle.
pub fn travel(shot: &Shot, t: f32) -> DVec3 {
    // (on a tripod the camera stays; the bus moves all the same, see the app)
    shot.motion.map(|(v, _)| (v * t).as_dvec3()).unwrap_or(DVec3::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam() -> Camera {
        Camera { position: DVec3::new(10.0, 20.0, 2.0), yaw: 30.0, pitch: -5.0, roll: 0.0, fov_deg: 40.0, near: 0.1, far: 1000.0 }
    }

    #[test]
    fn focal_length_and_field_of_view_go_together() {
        assert!((focal_mm(fov_for(50.0)) - 50.0).abs() < 1e-3);
        // a 50 mm lens on full frame: 27 degrees high
        assert!((fov_for(50.0) - 26.99).abs() < 0.05, "{}", fov_for(50.0));
    }

    #[test]
    fn the_aperture_points_stay_inside_its_shape() {
        for blades in [0, 5, 6, 8] {
            for i in 0..256 {
                let (x, y) = aperture_point(halton(i, 2), halton(i, 3), blades);
                assert!((x * x + y * y).sqrt() <= 1.0 + 1e-4, "{blades}: {x} {y}");
            }
        }
        // a hexagon reaches its corners but not the circle between them
        let edge = (0..4096).map(|i| { let (x, y) = aperture_point(halton(i, 2), halton(i, 3), 6); (x * x + y * y).sqrt() }).fold(0.0f32, f32::max);
        assert!(edge > 0.97, "{edge}");
    }

    #[test]
    fn every_picture_aims_at_the_same_point_of_the_focus_plane() {
        let base = cam();
        let shot = Shot { dof: true, fstop: 1.4, focus: 8.0, ..Shot::default() };
        let target = base.position + (base.forward() * 8.0).as_dvec3();
        let mut moved = 0.0f64;
        for i in 1..32 {
            let c = sample(&base, i, &shot, 100_000.0).cam;
            moved = moved.max((c.position - base.position).length());
            // (the ray from the moved camera passes the focus point)
            let to = (target - c.position).as_vec3().normalize();
            assert!(to.dot(c.forward()) > 0.999_99, "{i}");
        }
        // a 50-odd mm lens at f/1.4: the aperture is a couple of centimetres across
        assert!(moved > 0.005 && moved < 0.03, "{moved}");
    }

    #[test]
    fn without_depth_of_field_the_camera_stays_put() {
        let base = cam();
        let shot = Shot { dof: false, fstop: 2.8, focus: 5.0, ..Shot::default() };
        for i in 0..8 {
            let Sample { cam: c, t, .. } = sample(&base, i, &shot, 1080.0);
            assert_eq!(c.position, base.position);
            assert_eq!(t, 0.0);
            assert!((c.yaw - base.yaw).abs() < 0.05 && (c.pitch - base.pitch).abs() < 0.05);
        }
    }

    #[test]
    fn the_shutter_spreads_the_pictures_over_its_time() {
        let shot = Shot { dof: false, fstop: 2.8, focus: 5.0, motion: Some((Vec3::new(10.0, 0.0, 0.0), 1.0 / 30.0)), ..Shot::default() };
        let ts: Vec<f32> = (1..64).map(|i| sample(&cam(), i, &shot, 1080.0).t).collect();
        let (lo, hi) = ts.iter().fold((1.0f32, -1.0f32), |(a, b), t| (a.min(*t), b.max(*t)));
        assert!(lo < -0.015 && hi > 0.015 && lo >= -0.0168 && hi <= 0.0168, "{lo} {hi}");
        assert!((travel(&shot, hi).x - 10.0 * hi as f64).abs() < 1e-6);
    }

    #[test]
    fn a_tilted_lens_leans_the_plane_of_focus() {
        let flat = Shot { dof: true, focus: 10.0, ..Shot::default() };
        assert!((inv_focus_at(&flat, 0.05, 0.3, -0.2) - 0.1).abs() < 1e-6);
        // tilted down: the plane goes further off towards the top of the frame (the ground)
        let tilted = Shot { tilt: [8.0, 0.0], ..flat };
        assert!(inv_focus_at(&tilted, 0.05, 0.0, 0.2) < inv_focus_at(&tilted, 0.05, 0.0, -0.2));
        assert!((inv_focus_at(&tilted, 0.05, 0.0, 0.0) - 0.1).abs() < 1e-6);
        // the hinge: 50 mm tilted 8 degrees, some 36 cm under the lens - a camera 1.5 m up
        // looking level has the ground (1/z = -v / 1.5) nearly in focus
        let hinge = 0.05 / 8f32.to_radians().sin();
        assert!((hinge - 0.359).abs() < 0.01, "{hinge}");
    }

    #[test]
    fn the_blur_circle_grows_away_from_the_focus_and_with_the_sensor() {
        let shot = Shot { dof: true, fstop: 2.0, focus: 10.0, ..Shot::default() };
        let r = aperture_radius(40.0, &shot);
        assert!(coc_px(&shot, r, 0.05, 10.0, 0.0, 0.0, 0.001) < 1e-3);
        assert!(coc_px(&shot, r, 0.05, 100.0, 0.0, 0.0, 0.001) > coc_px(&shot, r, 0.05, 20.0, 0.0, 0.0, 0.001));
        // a smaller sensor at the same view: a shorter lens, a smaller aperture, less blur
        let small = Shot { sensor_h: 24.0 / 2.0, ..shot };
        assert!((aperture_radius(40.0, &small) - r * 0.5).abs() < 1e-6);
    }
}
