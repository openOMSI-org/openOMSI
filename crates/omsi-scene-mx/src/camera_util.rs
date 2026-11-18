//! Camera geometry: the map's default camera, the ray under a window pixel and the angle of
//! a pixel, how far the outside camera may orbit, picking's bounding test, and where a
//! mirror's camera looks (as Omsi.exe aims it) and whether a view sees it.

use crate::World;
use glam::{DVec3, Vec3};
use omsi_render::Camera;

/// Default camera from `[mapcam]`: tile x, tile y, x, z, y, yaw, pitch, distance.
pub fn default_camera(world: &World) -> Camera {
    let mc = &world.global.map_cam;
    if mc.len() >= 8 {
        let tx = mc[0];
        let ty = mc[1];
        let x = tx * omsi_map::tile_size() + mc[2];
        let z = mc[3];
        let y = ty * omsi_map::tile_size() + mc[4];
        let yaw = mc[5] as f32;
        let pitch = mc[6] as f32;
        let dist = mc[7] as f32;
        let mut cam = Camera {
            position: DVec3::new(x, y, z),
            yaw,
            pitch,
            roll: 0.0,
            fov_deg: 60.0,
            near: 0.1,
            far: 6000.0,
        };
        // the editor camera orbits the point at `dist`; step back along the view direction
        cam.position -= (cam.forward() * dist).as_dvec3();
        cam.position.z = cam.position.z.max(z + 5.0);
        cam
    } else {
        Camera {
            position: DVec3::new(150.0, 150.0, 60.0),
            yaw: 0.0,
            pitch: -20.0,
            roll: 0.0,
            fov_deg: 60.0,
            near: 0.1,
            far: 6000.0,
        }
    }
}

/// Can the ray from `o` along `dir` (vehicle frame) come near mesh `i` of the type, posed
/// by `xf`? `widen` is the extra angle (radians) of the rays spread around it. The hover
/// test cast a ray at every triangle of the whole bus each frame - 0.7 ms of the main
/// thread on a fast machine, three times that on a slow one - while the mouse was over
/// nothing; a sphere per mesh passes most of them by.
pub fn ray_may_hit(
    ty: &omsi_sim::VehicleType,
    i: usize,
    xf: &glam::Mat4,
    o: Vec3,
    dir: Vec3,
    widen: f32,
) -> bool {
    let Some(&(c, r)) = ty.mesh_bounds.get(i) else {
        return true;
    };
    if r <= 0.0 {
        return true;
    }
    let scale = xf
        .x_axis
        .truncate()
        .length()
        .max(xf.y_axis.truncate().length())
        .max(xf.z_axis.truncate().length());
    let c = xf.transform_point3(c);
    let r = r * scale + (c - o).length() * widen + 0.01;
    omsi_geometry::ray_near_sphere(o, dir.normalize_or_zero(), c, r)
}

/// How far the outside camera sits from the bus: at the start, and the nearest and
/// farthest the wheel or +/- take it. The old 18 m default with a 200 m maximum was a
/// helicopter view; a bus is 11 m long and reads best from ten.
pub const ORBIT_DEFAULT: f32 = 10.0;

pub const ORBIT_MIN: f32 = 3.5;

pub const ORBIT_MAX: f32 = 40.0;

/// The angle one pixel subtends vertically, so that a pick tolerance can be stated in
/// pixels and mean the same at any window size or field of view.
pub fn pixel_angle(cam: &Camera, height: f32) -> f32 {
    2.0 * (cam.fov_deg.to_radians() * 0.5).tan() / height.max(1.0)
}

/// World-space ray through a window pixel: (camera position, unit direction).
pub fn cursor_ray(cam: &Camera, x: f32, y: f32, w: f32, h: f32) -> (DVec3, Vec3) {
    let (_, d) = cam.ray(
        x / w * 2.0 - 1.0,
        1.0 - y / h * 2.0,
        w / h.max(1.0),
        cam.position,
    );
    (cam.position, d)
}

/// Whether a mirror is in the picture of `view` (camera and aspect): its camera's place as
/// a sphere of the `[add_camera_reflexion_2]` radius (a point for `[add_camera_reflexion]`)
/// against the view's frustum, as Omsi.exe tests it (0x6f611f, 0x7f41ac) before it redraws
/// a mirror.
pub fn mirror_in_view(eye: DVec3, radius: f32, view: &(Camera, f32)) -> bool {
    let (cam, aspect) = view;
    let d = (eye - cam.position).as_vec3();
    let f = cam.forward();
    let (r, u) = (cam.right(), cam.up());
    let z = d.dot(f);
    if z < -radius {
        return false;
    }
    let tan_y = (cam.fov_deg.to_radians() * 0.5).tan();
    let tan_x = tan_y * aspect;
    let slack_x = radius * (1.0 + tan_x * tan_x).sqrt();
    let slack_y = radius * (1.0 + tan_y * tan_y).sqrt();
    d.dot(r).abs() <= z.max(0.0) * tan_x + slack_x && d.dot(u).abs() <= z.max(0.0) * tan_y + slack_y
}

/// A mirror's camera as Omsi.exe aims it, every frame (0x6f6468 -> 0x7ed0e8): the yaw and
/// pitch of an `[add_camera_reflexion]` are not where it looks but which way the mirror's
/// face is turned - (cos p sin y, cos p cos y, sin p) in the vehicle's frame (0x7edfd0); the
/// ray from the eye of the view being drawn to the mirror is reflected in that face, and the
/// camera looks along the reflection (yaw 90 - atan2(forward, right) and its elevation) from
/// the mirror - what a mirror shows whoever looks into it. `off` is the player's own turn of
/// the mirror (Ctrl+Alt+arrows), which turns the face. (The yaw was turned round instead,
/// which happened to fit a mirror facing straight back at the driver and no other: the left
/// mirrors, the kerb-side blind-spot mirrors and the door monitors of many buses looked into
/// the saloon or at the sky.)
pub fn mirror_view(v: &omsi_sim::VehicleInstance, c: &omsi_vehicle::Camera, eye: DVec3, off: [f32; 2]) -> omsi_vehicle::Camera {
    mirror_view_in(v.position, v.body_rotation(), c, eye, off)
}

/// [`mirror_view`] for a camera of a body at `position`, turned by `rot` (a rear section's).
/// A `[add_camera_reflexion_static]` camera (`fixed`) is no mirror: it looks along its own
/// yaw and pitch (and the player's turn of it), wherever the eye is.
pub fn mirror_view_in(position: DVec3, rot: glam::Mat4, c: &omsi_vehicle::Camera, eye: DVec3, off: [f32; 2]) -> omsi_vehicle::Camera {
    if c.fixed {
        return omsi_vehicle::Camera { yaw: c.yaw + off[0], pitch: (c.pitch + off[1]).clamp(-89.0, 89.0), ..c.clone() };
    }
    let at = position + rot.transform_point3(glam::Vec3::new(c.pos[0], c.pos[1], c.pos[2])).as_dvec3();
    let d = rot.inverse().transform_vector3((at - eye).as_vec3());
    let Some(d) = d.try_normalize() else { return c.clone() };
    let (y, p) = ((c.yaw + off[0]).to_radians(), (c.pitch + off[1]).to_radians());
    let m = glam::Vec3::new(p.cos() * y.sin(), p.cos() * y.cos(), p.sin());
    let r = d - m * (2.0 * d.dot(m));
    omsi_vehicle::Camera { yaw: r.x.atan2(r.y).to_degrees(), pitch: mirror_pitch_deg(r), ..c.clone() }
}

/// The pitch Omsi.exe gives a mirror camera looking along `r` (0x7efc60, arcsine 0x45a3d4):
/// the arcsine of the rise over the run on the ground, kept within [-1, 1] - so 30 degrees
/// down looks 35.26 degrees down, and from 45 degrees down on straight down. (The elevation
/// of `r` itself turned the Lion's City's corner mirror up to the saloon from the kerb.)
fn mirror_pitch_deg(r: glam::Vec3) -> f32 {
    let run = r.x.hypot(r.y).max(f32::MIN_POSITIVE);
    (r.z / run).clamp(-1.0, 1.0).asin().to_degrees()
}

/// A mirror camera as the player has set it: moved by a shift (bus frame, m) and with
/// `dfov` degrees added to its field of view (a mirror that gives none draws with 50).
pub fn adjusted(c: &omsi_vehicle::Camera, s: [f32; 3], dfov: f32) -> omsi_vehicle::Camera {
    let fov = if dfov.abs() > 0.01 { (if c.fov > 1.0 { c.fov } else { 50.0 } + dfov).clamp(8.0, 110.0) } else { c.fov };
    omsi_vehicle::Camera { pos: [c.pos[0] + s[0], c.pos[1] + s[1], c.pos[2] + s[2]], fov, ..c.clone() }
}

#[cfg(test)]
mod tests {
    use glam::{DVec3, Mat4};

    /// Pitch for a ray going down or up by `deg` degrees, seen from a few headings.
    #[test]
    fn mirror_pitch_is_the_arcsine_of_rise_over_run() {
        for heading in [0.0f32, 40.0, 135.0, 260.0] {
            let pitch = |deg: f32| {
                let (h, e) = (heading.to_radians(), deg.to_radians());
                super::mirror_pitch_deg(glam::Vec3::new(h.sin() * e.cos(), h.cos() * e.cos(), e.sin()))
            };
            let want = |deg: f32| deg.to_radians().tan().clamp(-1.0, 1.0).asin().to_degrees();
            for deg in [0.0, 12.0, -30.0, 44.0, -60.0] {
                assert!((pitch(deg) - want(deg)).abs() < 1e-3, "{heading} {deg}: {}", pitch(deg));
            }
            assert!((pitch(-30.0) + 35.264).abs() < 1e-2);
        }
        assert_eq!(super::mirror_pitch_deg(glam::Vec3::NEG_Z), -90.0);
    }

    /// A mirror's camera looks along the eye's ray reflected in its face; a static one looks
    /// where it is pointed, wherever the eye is, turned by the player's offset.
    #[test]
    fn a_static_camera_does_not_follow_the_eye() {
        let mirror = omsi_vehicle::Camera { pos: [1.2, 5.0, 2.0], fov: 30.0, yaw: 180.0, ..Default::default() };
        let cctv = omsi_vehicle::Camera { fixed: true, yaw: 90.0, pitch: -40.0, ..mirror.clone() };
        let views = |c: &omsi_vehicle::Camera| {
            [DVec3::new(0.0, 4.0, 2.0), DVec3::new(-0.5, 3.0, 2.2)]
                .map(|eye| super::mirror_view_in(DVec3::ZERO, Mat4::IDENTITY, c, eye, [0.0; 2]))
                .map(|c| (c.yaw, c.pitch))
        };
        let [a, b] = views(&mirror);
        assert!((a.0 - b.0).abs() > 1.0, "{a:?} {b:?}");
        assert_eq!(views(&cctv), [(90.0, -40.0); 2]);
        let turned = super::mirror_view_in(DVec3::ZERO, Mat4::IDENTITY, &cctv, DVec3::ZERO, [5.0, -60.0]);
        assert_eq!((turned.yaw, turned.pitch), (95.0, -89.0));
    }
}
