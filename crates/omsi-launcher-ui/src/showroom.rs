//! The launcher's side of the bus preview (the showroom the game program draws, see
//! `host::Showroom`): what it shows, the framing of the bus in the part of the window the
//! sheets leave free, the studio's corner for the bus picker's photos, and the bus as the
//! livery studio opens it up. The loading and the drawing are the program's
//! (`crates/omsi-app/src/launcher/showroom.rs`).

use glam::{DVec3, Vec2, Vec3};
use omsi_render::{Camera, Scene};
use std::path::PathBuf;
use std::sync::Arc;

/// What the showroom shows.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Look {
    pub root: PathBuf,
    pub map: String,
    pub bus: String,
    pub paint: String,
    pub weather: String,
    /// Minutes of the day, and the date.
    pub time: i32,
    pub date: String,
}

/// The bus shown, opened up for the livery studio: its scene (whose texture slots it paints),
/// its type, the vehicle and its parts' renders.
pub struct Parts<'a> {
    pub scene: &'a mut Scene,
    pub vt: &'a Arc<omsi_sim::VehicleType>,
    pub vehicle: &'a omsi_sim::VehicleInstance,
    pub render: &'a omsi_scene::VehicleRender,
    pub trailers: &'a [omsi_scene::VehicleRender],
}

/// The studio's corner: in front of the bus on its door side, a little above it. The camera
/// aims ahead of the middle: seen from a front corner the near front looms larger than the
/// far rear, and aimed at the middle the bus's front ran off the photo's edge.
pub const PHOTO_YAW: f32 = 218.0;
pub const PHOTO_PITCH: f32 = 11.0;
pub const PHOTO_ZOOM: f32 = 0.8;
pub const PHOTO_AIM: f32 = 0.16;

// --- framing the bus ------------------------------------------------------------------------
//
// The picture covers the whole window, under the sheet and the bar, but the bus is to stand in
// the part they leave free (the stage), in its middle and as large as it fits. The renderer's
// lens looks straight through the picture's middle, so the camera is turned past the bus
// instead, by as much as the stage's middle is off the picture's; and it stands as far off as
// the bus needs to fit the stage seen from its longest side, so that turning it round keeps
// its size.

/// The part of the picture the bus is framed in, as shares of the picture: left, top, right,
/// bottom (from the top down).
pub type Stage = [f32; 4];

/// The whole picture.
pub const WHOLE: Stage = [0.0, 0.0, 1.0, 1.0];

/// `stage`, a part of the window, as shares of `picture` (the part of the window the picture
/// covers): clipped to it, and never thinner than a twentieth of it.
pub fn stage_in(picture: omsi_ui::Rect, stage: omsi_ui::Rect) -> Stage {
    let (w, h) = (picture.w.max(1.0), picture.h.max(1.0));
    let x0 = ((stage.x - picture.x) / w).clamp(0.0, 0.95);
    let y0 = ((stage.y - picture.y) / h).clamp(0.0, 0.95);
    let x1 = ((stage.right() - picture.x) / w).clamp(x0 + 0.05, 1.0);
    let y1 = ((stage.bottom() - picture.y) / h).clamp(y0 + 0.05, 1.0);
    [x0, y0, x1, y1]
}

/// The bus's box in the showroom: its middle and half its size each way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BusBox {
    pub centre: Vec3,
    pub half: Vec3,
}

impl BusBox {
    /// From OMSI's `[boundingbox]`: the sizes across, along and up, then the middle.
    pub fn of(bb: [f32; 6]) -> BusBox {
        BusBox { centre: Vec3::new(bb[3], bb[4], bb[5]), half: Vec3::new(bb[0], bb[1], bb[2]).abs() * 0.5 }
    }

    /// This box and `other` in one (an articulated bus: its front and its rear section).
    pub fn with(self, other: BusBox) -> BusBox {
        let lo = (self.centre - self.half).min(other.centre - other.half);
        let hi = (self.centre + self.half).max(other.centre + other.half);
        BusBox { centre: (lo + hi) * 0.5, half: (hi - lo) * 0.5 }
    }

    fn corners(&self) -> [Vec3; 8] {
        let s = |k: usize, bit: usize| if k & bit == 0 { -1.0 } else { 1.0 };
        std::array::from_fn(|k| self.centre + self.half * Vec3::new(s(k, 1), s(k, 2), s(k, 4)))
    }
}

/// How far past its `[boundingbox]` a bus's model may reach across and along and still be
/// framed: a bumper, not its mirrors (an O560's hang 0.55 m ahead of it, and framed with them
/// the body stood off the middle: they poke into the margin instead) nor the headlights' light
/// some models lay on the road ahead.
pub const REACH: Vec3 = Vec3::new(0.2, 0.3, 0.0);

/// The box a bus (or its rear section) takes in the showroom, from its `[boundingbox]` `bb` and
/// the box its model's vertices take (`VehicleType::model_box`): across and along the bounding
/// box's, or a little more where the model reaches past it, and up what the model reaches,
/// from just under the floor. The bounding box's height and middle are no guide to what is
/// seen - an O560's stood 0.6 m above the floor, an A20's ended 0.5 m under its roof.
/// `fallback` without either.
pub fn bus_box(bb: Option<[f32; 6]>, model: Option<(Vec3, Vec3)>, fallback: [f32; 6]) -> BusBox {
    let b = BusBox::of(bb.unwrap_or(fallback));
    let Some((mlo, mhi)) = model else { return b };
    let (blo, bhi) = (b.centre - b.half, b.centre + b.half);
    let (lo, hi) = if bb.is_some() { (mlo.min(blo).max(blo - REACH), mhi.max(bhi).min(bhi + REACH)) } else { (mlo, mhi) };
    let z0 = mlo.z.clamp(-0.3, 1.0);
    let z1 = mhi.z.clamp(z0 + 1.0, 6.0);
    let (lo, hi) = (Vec3::new(lo.x, lo.y, z0), Vec3::new(hi.x, hi.y, z1));
    BusBox { centre: (lo + hi) * 0.5, half: (hi - lo) * 0.5 }
}

/// The showroom's lens, its height in degrees, and its near plane.
pub const FOV: f32 = 30.0;
pub const NEAR: f32 = 0.2;
/// The share of the stage the bus takes each way, seen from where it takes the most (its long
/// side): the rest is room round it, for its shadow and the eye.
pub const FILL: f32 = 0.88;
/// The camera never sinks lower than this above the floor (metres).
pub const EYE_LOW: f32 = 0.6;

/// Half the lens' height at a distance of one.
pub fn half_v() -> f32 {
    (FOV.to_radians() * 0.5).tan()
}

/// Where the camera stands and where it looks: heading and pitch in degrees, as `Camera`'s.
#[derive(Clone, Copy, Debug)]
pub struct View {
    eye: Vec3,
    yaw: f32,
    pitch: f32,
}

impl View {
    /// Where `p` lands in a picture `aspect` wide (NDC: -1 to 1, y up); None behind the lens.
    fn project(&self, p: Vec3, aspect: f32) -> Option<Vec2> {
        let half_v = half_v();
        let (sy, cy) = self.yaw.to_radians().sin_cos();
        let (sp, cp) = self.pitch.to_radians().sin_cos();
        // ahead, right and up, as `Camera::view_proj` makes them
        let (f, s, u) = (Vec3::new(sy * cp, cy * cp, sp), Vec3::new(cy, -sy, 0.0), Vec3::new(-sy * sp, -cy * sp, cp));
        let q = p - self.eye;
        let z = q.dot(f);
        (z > NEAR).then(|| Vec2::new(q.dot(s) / (z * half_v * aspect), q.dot(u) / (z * half_v)))
    }

    /// The smallest rectangle (NDC) round the bus's corners; None when one is behind the lens.
    fn bounds(&self, bus: &BusBox, aspect: f32) -> Option<(Vec2, Vec2)> {
        let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
        for c in bus.corners() {
            let p = self.project(c, aspect)?;
            (lo, hi) = (lo.min(p), hi.max(p));
        }
        Some((lo, hi))
    }
}

/// The camera's heading and pitch (degrees) for which a point straight along `w` (a unit
/// vector from the camera) lands at `at` (NDC) in a picture `aspect` wide: the camera turned
/// past the point by as much as `at` is off the middle, its horizon kept level.
pub fn aim(w: Vec3, at: Vec2, aspect: f32) -> (f32, f32) {
    let half_v = half_v();
    // the point's direction as the camera sees it: right, up and ahead
    let v = Vec3::new(at.x * half_v * aspect, at.y * half_v, 1.0).normalize();
    // its height in the world is that of the camera's up and ahead together:
    // w.z = v.y cos(pitch) + v.z sin(pitch)
    let r = (v.y * v.y + v.z * v.z).sqrt().max(1e-6);
    let pitch = (w.z / r).clamp(-1.0, 1.0).asin() - v.y.atan2(v.z);
    // and its heading the camera's, turned by its angle off the camera's level ahead
    let ahead = v.z * pitch.cos() - v.y * pitch.sin();
    let yaw = w.x.atan2(w.y) - v.x.atan2(ahead);
    (yaw.to_degrees(), pitch.to_degrees())
}

/// The camera's place orbiting the bus: `dist` metres from its middle, at heading `yaw` round
/// it, looking down by `pitch` degrees, and never under the floor.
pub fn eye(bus: &BusBox, yaw: f32, pitch: f32, dist: f32) -> Vec3 {
    let (sy, cy) = yaw.to_radians().sin_cos();
    let (sp, cp) = pitch.to_radians().sin_cos();
    let mut e = bus.centre - Vec3::new(sy * cp, cy * cp, -sp) * dist;
    e.z = e.z.max(EYE_LOW);
    e
}

/// The view from `eye` with the bus's middle at `at` (NDC).
pub fn looking(bus: &BusBox, eye: Vec3, at: Vec2, aspect: f32) -> View {
    let (yaw, pitch) = aim((bus.centre - eye).normalize_or(Vec3::Y), at, aspect);
    View { eye, yaw, pitch }
}

/// How far from its middle the camera must stand, looking down by `pitch`, for the bus's
/// outline to fit `room` (NDC) with its middle aimed at `centre`: the farthest of the headings
/// all round it, so that it keeps its distance - and its size - while it is turned.
pub fn fit_distance(bus: &BusBox, pitch: f32, aspect: f32, centre: Vec2, room: Vec2) -> f32 {
    let fits = |yaw: f32, d: f32| looking(bus, eye(bus, yaw, pitch, d), centre, aspect).bounds(bus, aspect).is_some_and(|(lo, hi)| hi.x - lo.x <= room.x && hi.y - lo.y <= room.y);
    let mut need = bus.half.length() + 1.0;
    for k in 0..36 {
        let yaw = k as f32 * 10.0;
        if fits(yaw, need) {
            continue;
        }
        let (mut lo, mut hi) = (need, need * 2.0);
        while !fits(yaw, hi) && hi < 20000.0 {
            (lo, hi) = (hi, hi * 2.0);
        }
        for _ in 0..24 {
            let mid = (lo + hi) * 0.5;
            if fits(yaw, mid) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        need = hi;
    }
    need
}

/// The showroom's view: orbiting the bus at heading `yaw` round it and looking down by `pitch`
/// (degrees), `zoom` times as far off as it must be to fit `stage` whichever way it is turned,
/// and aimed so that the bus stands in the middle of the stage - its outline, not its middle
/// point, which the near end looming larger pushes off to the far side.
pub fn frame_view(bus: &BusBox, yaw: f32, pitch: f32, zoom: f32, aspect: f32, stage: Stage) -> View {
    let centre = Vec2::new(stage[0] + stage[2] - 1.0, 1.0 - stage[1] - stage[3]);
    let room = Vec2::new(stage[2] - stage[0], stage[3] - stage[1]).max(Vec2::splat(0.02)) * 2.0 * FILL;
    let dist = (fit_distance(bus, pitch, aspect, centre, room) * zoom).max(bus.half.length() + NEAR);
    let e = eye(bus, yaw, pitch, dist);
    let mut at = centre;
    let mut view = looking(bus, e, at, aspect);
    for _ in 0..4 {
        let Some((lo, hi)) = view.bounds(bus, aspect) else { break };
        let off = centre - (lo + hi) * 0.5;
        if off.length() < 1e-4 {
            break;
        }
        at = (at + off).clamp(Vec2::splat(-0.9), Vec2::splat(0.9));
        view = looking(bus, e, at, aspect);
    }
    view
}

/// The showroom's camera (see `frame_view`).
pub fn frame(bus: &BusBox, yaw: f32, pitch: f32, zoom: f32, aspect: f32, stage: Stage) -> Camera {
    let v = frame_view(bus, yaw, pitch, zoom, aspect, stage);
    Camera { position: v.eye.as_dvec3(), yaw: v.yaw, pitch: v.pitch, roll: 0.0, fov_deg: FOV, near: NEAR, far: 6000.0 }
}

/// The studio's camera for the bus picker's photos, as the showroom framed the bus before it
/// knew a stage (the photos taken then are kept): the whole picture, the bus's length alone
/// fitted to its width, aimed `aim` of its length ahead of its middle.
pub fn studio_camera(centre: Vec3, length: f32, yaw: f32, pitch: f32, zoom: f32, aim: f32, aspect: f32) -> Camera {
    let fit = (length * 0.5 + 1.0) / (half_v() * aspect * 0.92).max(0.1);
    let dist = (fit * zoom).max(8.0);
    let (sy, cy) = yaw.to_radians().sin_cos();
    let (sp, cp) = pitch.to_radians().sin_cos();
    let target = DVec3::new(centre.x as f64, (centre.y + aim * length) as f64, (centre.z * 0.75) as f64);
    // from the camera towards the bus: forward along the yaw, down by the pitch
    let dir = DVec3::new((sy * cp) as f64, (cy * cp) as f64, -sp as f64);
    let mut pos = target - dir * dist as f64;
    pos.z = pos.z.max(0.6);
    Camera { position: pos, yaw, pitch: -pitch, roll: 0.0, fov_deg: FOV, near: NEAR, far: 6000.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_ui::Rect;

    /// A solo bus, an articulated one (front and rear section), a midibus and a double-decker.
    fn buses() -> Vec<(&'static str, BusBox)> {
        let solo = BusBox::of([2.55, 12.0, 3.1, 0.0, -1.2, 1.55]);
        let front = BusBox::of([2.55, 11.2, 3.1, 0.0, -1.0, 1.55]);
        let rear = BusBox::of([2.55, 7.6, 3.1, 0.0, -1.0 - 9.4, 1.55]);
        let midi = BusBox::of([2.3, 8.5, 2.9, 0.0, -0.8, 1.45]);
        let decker = BusBox::of([2.55, 11.2, 4.3, 0.0, -1.0, 2.15]);
        vec![("solo", solo), ("articulated", front.with(rear)), ("midi", midi), ("double-decker", decker)]
    }

    /// The window's pixel `p` (a point of the world) lands on, seen by `cam` in a window `size`
    /// big: the renderer's own projection, not the framing's.
    fn on_screen(cam: &Camera, size: Vec2, p: Vec3) -> Vec2 {
        let m = cam.view_proj(size.x / size.y, cam.position);
        let n = m.project_point3((p.as_dvec3() - cam.position).as_vec3());
        Vec2::new((n.x + 1.0) * 0.5 * size.x, (1.0 - n.y) * 0.5 * size.y)
    }

    /// The bus's outline in the window, in pixels: left top and right bottom.
    fn outline(cam: &Camera, size: Vec2, bus: &BusBox) -> (Vec2, Vec2) {
        let pts: Vec<Vec2> = bus.corners().iter().map(|c| on_screen(cam, size, *c)).collect();
        (pts.iter().fold(Vec2::splat(f32::INFINITY), |a, p| a.min(*p)), pts.iter().fold(Vec2::splat(f32::NEG_INFINITY), |a, p| a.max(*p)))
    }

    /// The bus step's stage in a window `size` big, as the launcher lays it out.
    fn stage_of(size: Vec2) -> Rect {
        super::super::buspick::stage(size, super::super::flow::sheet_rect(size))
    }

    #[test]
    fn the_camera_turns_past_a_point_to_put_it_where_asked() {
        for (yaw, pitch) in [(0.0f32, 0.0f32), (215.0, 8.0), (90.0, -4.0), (300.0, 55.0)] {
            let (sy, cy) = yaw.to_radians().sin_cos();
            let (sp, cp) = pitch.to_radians().sin_cos();
            let w = Vec3::new(sy * cp, cy * cp, -sp);
            for at in [Vec2::ZERO, Vec2::new(0.3, 0.0), Vec2::new(0.25, 0.08), Vec2::new(-0.6, 0.4), Vec2::new(0.8, -0.5)] {
                let (cyaw, cpitch) = aim(w, at, 1.8);
                let cam = Camera { position: DVec3::ZERO, yaw: cyaw, pitch: cpitch, roll: 0.0, fov_deg: FOV, near: NEAR, far: 6000.0 };
                let n = cam.view_proj(1.8, DVec3::ZERO).project_point3(w * 20.0);
                assert!((n.x - at.x).abs() < 1e-3 && (n.y - at.y).abs() < 1e-3, "{yaw}/{pitch} to {at:?}: {n:?}");
                // and the framing's own projection agrees with the renderer's
                let p = View { eye: Vec3::ZERO, yaw: cyaw, pitch: cpitch }.project(w * 20.0, 1.8).unwrap();
                assert!((p - at).length() < 1e-3, "{p:?}");
            }
        }
    }

    #[test]
    fn the_bus_stands_in_the_middle_of_the_free_part_and_fits_it() {
        for size in [Vec2::new(1440.0, 900.0), Vec2::new(2560.0, 1347.0), Vec2::new(1080.0, 680.0), Vec2::new(1920.0, 1080.0), Vec2::new(1280.0, 1024.0)] {
            let window = Rect::new(0.0, 0.0, size.x, size.y);
            let stage = stage_of(size);
            let shares = stage_in(window, stage);
            for (name, bus) in buses() {
                for yaw in [215.0f32, 90.0, 270.0, 0.0, 180.0, 135.0, 32.0] {
                    for pitch in [8.0f32, -4.0, 25.0, 55.0] {
                        let cam = frame(&bus, yaw, pitch, 1.0, size.x / size.y, shares);
                        let (lo, hi) = outline(&cam, size, &bus);
                        let what = format!("{name} at {yaw}/{pitch} in {size}: {lo} - {hi}, stage {stage:?}");
                        // all of it in the stage, nothing under the sheet or the actions
                        assert!(lo.x >= stage.x - 0.5 && hi.x <= stage.right() + 0.5 && lo.y >= stage.y - 0.5 && hi.y <= stage.bottom() + 0.5, "{what}");
                        // in the stage's middle
                        let mid = (lo + hi) * 0.5;
                        assert!((mid - stage.center()).length() < 2.0, "{what}: middle {mid}, the stage's {}", stage.center());
                    }
                }
                // seen from its long side it takes the stage's width or height, but for the margin
                let cam = frame(&bus, 270.0, 8.0, 1.0, size.x / size.y, shares);
                let (lo, hi) = outline(&cam, size, &bus);
                let fill = ((hi.x - lo.x) / stage.w).max((hi.y - lo.y) / stage.h);
                assert!(fill > 0.8 && fill <= FILL + 0.01, "{name} in {size}: fills {fill}");
                // from the default corner it is not lost in it either
                let cam = frame(&bus, 215.0, 8.0, 1.0, size.x / size.y, shares);
                let (lo, hi) = outline(&cam, size, &bus);
                assert!((hi.x - lo.x) / stage.w > 0.5 || (hi.y - lo.y) / stage.h > 0.5, "{name} in {size}: small from the corner");
            }
        }
    }

    #[test]
    fn turning_keeps_the_distance_and_zooming_keeps_the_middle() {
        let size = Vec2::new(1440.0, 900.0);
        let stage = stage_of(size);
        let shares = stage_in(Rect::new(0.0, 0.0, size.x, size.y), stage);
        let (_, bus) = buses().remove(1);
        let dist = |yaw: f32, zoom: f32| frame_view(&bus, yaw, 8.0, zoom, size.x / size.y, shares).eye.distance(bus.centre);
        let d = dist(215.0, 1.0);
        for yaw in [0.0, 47.0, 90.0, 180.0, 333.0] {
            assert!((dist(yaw, 1.0) - d).abs() < 1e-3 * d, "the bus keeps its size while turned ({yaw})");
        }
        assert!((dist(215.0, 0.7) - d * 0.7).abs() < 1e-3 * d);
        // closer in, larger, and still about the stage's middle
        let near = frame(&bus, 215.0, 8.0, 0.7, size.x / size.y, shares);
        let far = frame(&bus, 215.0, 8.0, 1.0, size.x / size.y, shares);
        let ((a, b), (c, e)) = (outline(&near, size, &bus), outline(&far, size, &bus));
        assert!(b.x - a.x > (e.x - c.x) * 1.2);
        assert!((((a + b) * 0.5) - stage.center()).length() < 2.0);
    }

    #[test]
    fn the_stage_is_what_the_sheet_and_the_actions_leave() {
        use super::super::flow::{ACTION_BOTTOM, ACTION_H, SHEET_TOP};
        for size in [Vec2::new(1440.0, 900.0), Vec2::new(2560.0, 1347.0), Vec2::new(1080.0, 680.0)] {
            let sheet = super::super::flow::sheet_rect(size);
            let s = stage_of(size);
            assert!(s.x > sheet.right() && s.right() < size.x && s.y >= SHEET_TOP, "{size}: {s:?}");
            // above the duty's line over the main action
            assert!(s.bottom() < size.y - ACTION_BOTTOM - ACTION_H - 32.0, "{size}: {s:?}");
            assert!(s.w > 400.0 && s.h > 300.0, "{size}: {s:?}");
            let shares = stage_in(Rect::new(0.0, 0.0, size.x, size.y), s);
            assert!((shares[0] * size.x - s.x).abs() < 1e-3 && (shares[3] * size.y - s.bottom()).abs() < 1e-3);
        }
        // (a stage off the picture is clipped to it, and kept a sliver at the least)
        assert_eq!(stage_in(Rect::new(0.0, 0.0, 100.0, 100.0), Rect::new(-20.0, 50.0, 300.0, 0.0)), [0.0, 0.5, 1.0, 0.55]);
    }

    #[test]
    fn an_articulated_bus_is_framed_with_its_rear_section() {
        let front = BusBox::of([2.55, 11.0, 3.1, 0.0, 0.0, 1.55]);
        let rear = BusBox::of([2.55, 7.0, 3.1, 0.0, -9.5, 1.55]);
        let both = front.with(rear);
        assert!((both.half.y * 2.0 - 18.5).abs() < 1e-4 && (both.centre.y - (5.5 - 9.25)).abs() < 1e-4, "{both:?}");
        assert_eq!(both.half.x, 2.55 * 0.5);
    }

    #[test]
    fn the_box_is_the_model_s_within_reason() {
        // (an O560 coach, its numbers as installed: the bounding box 0.6 m above the floor, the
        // mirrors past its front and sides, the rear bumper a little past its back)
        let bb = [2.46, 12.08, 3.3, 0.0, 0.22, 2.28];
        let b = bus_box(Some(bb), Some((Vec3::new(-1.48, -5.85, 0.0026), Vec3::new(1.46, 6.80, 3.44))), bb);
        let (lo, hi) = (b.centre - b.half, b.centre + b.half);
        assert!((lo.z - 0.0026).abs() < 1e-4 && (hi.z - 3.44).abs() < 1e-4, "on the floor, as high as the model: {b:?}");
        assert!((lo.y + 5.85).abs() < 1e-4, "the bumper in it: {b:?}");
        assert!((hi.y - (0.22 + 6.04 + 0.3)).abs() < 1e-4 && (lo.x + 1.23 + 0.2).abs() < 1e-4, "the mirrors not all: {b:?}");
        // a model with its headlights' light on the road ahead, and a helper far under it: a
        // little past the bounding box at the most, and from just under the floor
        let b = bus_box(Some(bb), Some((Vec3::new(-1.48, -5.85, -2.15), Vec3::new(1.46, 16.0, 3.44))), bb);
        let (lo, hi) = (b.centre - b.half, b.centre + b.half);
        assert!((hi.y - (0.22 + 6.04 + 0.3)).abs() < 1e-4 && (lo.z + 0.3).abs() < 1e-4, "{b:?}");
        // no model: the bounding box; no bounding box: the model
        assert_eq!(bus_box(Some(bb), None, bb), BusBox::of(bb));
        let b = bus_box(None, Some((Vec3::new(-1.2, -5.0, 0.0), Vec3::new(1.2, 6.0, 3.0))), [2.5, 12.0, 3.0, 0.0, 0.0, 1.5]);
        assert!((b.half.y - 5.5).abs() < 1e-4 && (b.centre.y - 0.5).abs() < 1e-4);
    }

    #[test]
    fn the_studio_frames_its_photos_as_before() {
        // (the formula the photos taken so far were framed with)
        let (centre, length, aspect) = (Vec3::new(0.0, -1.0, 1.5), 12.0f32, 560.0 / 315.0);
        let cam = studio_camera(centre, length, PHOTO_YAW, PHOTO_PITCH, PHOTO_ZOOM, PHOTO_AIM, aspect);
        let half_v = (30.0f32.to_radians() * 0.5).tan();
        let dist = ((length * 0.5 + 1.0) / (half_v * aspect * 0.92) * PHOTO_ZOOM).max(8.0);
        let (sy, cy) = PHOTO_YAW.to_radians().sin_cos();
        let (sp, cp) = PHOTO_PITCH.to_radians().sin_cos();
        let target = DVec3::new(0.0, (-1.0 + PHOTO_AIM * length) as f64, 1.5 * 0.75);
        let pos = target - DVec3::new((sy * cp) as f64, (cy * cp) as f64, -sp as f64) * dist as f64;
        assert!((cam.position - pos).length() < 1e-4 && cam.yaw == PHOTO_YAW && cam.pitch == -PHOTO_PITCH && cam.fov_deg == 30.0);
    }

}
