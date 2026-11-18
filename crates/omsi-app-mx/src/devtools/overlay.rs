//! What the developer tools draw over the game's picture: the player's bus as its files
//! describe it - the paths and their points, the seats and standing places, the boxes, the
//! axles and wheels, the interior lights with their variables, the ticket and stamper
//! points, the mirrors, the centre of gravity and the views - each part of a train in its
//! own place. Lines in the picture's plane, drawn with the panel's painter: what is behind
//! the camera is cut off at a hand's breadth in front of it.

use glam::{DVec3, Mat4, Vec2, Vec3, Vec4};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Weight};

use omsi_ui::kit::Ui;

/// The overlays, by their place in [`Overlays::on`].
pub(crate) const NAMES: [&str; 10] = [
    "Paths",
    "Path points",
    "Seats and standing places",
    "Bounding boxes",
    "Axles and wheels",
    "Interior lights",
    "Tickets and stampers",
    "Mirrors",
    "Centre of gravity",
    "Views",
];

/// The overlays' short names, for `OMSI_INPUT` (`devtools overlay paths`).
pub(crate) const KEYS: [&str; 10] = ["paths", "points", "seats", "boxes", "axles", "lights", "tickets", "mirrors", "cog", "views"];

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub(crate) struct Overlays {
    pub on: [bool; 10],
}

impl Overlays {
    pub fn any(&self) -> bool {
        self.on.iter().any(|x| *x)
    }
}

const PATH: Color = Color::rgba(90, 200, 255, 0.95);
const POINT: Color = Color::rgba(255, 255, 255, 0.95);
const SEAT: Color = Color::rgba(120, 230, 120, 0.95);
const DRIVER: Color = Color::rgba(255, 210, 80, 0.95);
const BOX: Color = Color::rgba(255, 120, 60, 0.9);
const AXLE: Color = Color::rgba(200, 200, 200, 0.9);
const LIGHT: Color = Color::rgba(255, 240, 150, 0.95);
const TICKET: Color = Color::rgba(240, 110, 220, 0.95);
const MIRROR: Color = Color::rgba(110, 160, 255, 0.95);
const COG: Color = Color::rgba(255, 70, 70, 1.0);
const VIEW: Color = Color::rgba(180, 255, 230, 0.95);

/// The files of one part of the bus an overlay shows (read when the tools open and when
/// they are read again).
#[derive(Default)]
pub(crate) struct PartFiles {
    pub paths: Option<omsi_vehicle::VehiclePaths>,
    pub cabin: Option<omsi_vehicle::PassengerCabin>,
}

impl PartFiles {
    pub fn load(def: &omsi_vehicle::Vehicle) -> PartFiles {
        let paths = def.paths.as_ref().and_then(|rel| omsi_vehicle::VehiclePaths::load(&omsi_cfg::resolve_path(def.dir(), rel)).ok());
        let cabin = def.passenger_cabin.as_ref().and_then(|rel| omsi_vehicle::PassengerCabin::load(&omsi_cfg::resolve_path(def.dir(), rel)).ok());
        PartFiles { paths, cabin }
    }
}

/// The camera as the picture was drawn: the view-projection relative to its own place and
/// the picture's size in points.
pub(crate) struct Screen {
    pub origin: DVec3,
    pub vp: Mat4,
    pub size: Vec2,
}

impl Screen {
    pub fn new(cam: &omsi_render::Camera, size: Vec2) -> Screen {
        Screen { origin: cam.position, vp: cam.view_proj(size.x / size.y.max(1.0), cam.position), size }
    }

    fn clip(&self, p: DVec3) -> Vec4 {
        self.vp * (p - self.origin).as_vec3().extend(1.0)
    }

    fn to_screen(&self, c: Vec4) -> Vec2 {
        let ndc = c.truncate() / c.w;
        Vec2::new((ndc.x * 0.5 + 0.5) * self.size.x, (0.5 - ndc.y * 0.5) * self.size.y)
    }

    /// Where `p` is in the picture, None behind the camera.
    pub fn point(&self, p: DVec3) -> Option<Vec2> {
        let c = self.clip(p);
        (c.w > NEAR).then(|| self.to_screen(c))
    }

    /// The part of the line `a`-`b` in front of the camera, in the picture.
    pub fn segment(&self, a: DVec3, b: DVec3) -> Option<(Vec2, Vec2)> {
        let (mut ca, mut cb) = (self.clip(a), self.clip(b));
        if ca.w <= NEAR && cb.w <= NEAR {
            return None;
        }
        if ca.w < NEAR {
            ca = ca + (cb - ca) * ((NEAR - ca.w) / (cb.w - ca.w));
        } else if cb.w < NEAR {
            cb = cb + (ca - cb) * ((NEAR - cb.w) / (ca.w - cb.w));
        }
        Some((self.to_screen(ca), self.to_screen(cb)))
    }
}

/// How far in front of the camera a line is cut (m).
const NEAR: f32 = 0.05;

/// One part of the bus: its definition, files, and where it is.
pub(crate) struct Part<'a> {
    pub def: &'a omsi_vehicle::Vehicle,
    pub model: &'a omsi_model::Model,
    pub files: Option<&'a PartFiles>,
    pub position: DVec3,
    pub rot: Mat4,
}

impl Part<'_> {
    fn at(&self, local: Vec3) -> DVec3 {
        self.position + self.rot.transform_vector3(local).as_dvec3()
    }
}

/// Draws into the panel's picture.
struct Pen<'u> {
    ui: &'u mut Ui,
    screen: &'u Screen,
}

impl Pen<'_> {
    fn line(&mut self, a: DVec3, b: DVec3, w: f32, c: Color) {
        if let Some((a, b)) = self.screen.segment(a, b) {
            self.ui.p().line(a, b, w, c);
        }
    }

    fn dot(&mut self, p: DVec3, r: f32, c: Color) -> Option<Vec2> {
        let at = self.screen.point(p)?;
        self.ui.p().circle(at, r, c);
        Some(at)
    }

    fn label(&mut self, p: DVec3, text: &str, c: Color) {
        if let Some(at) = self.screen.point(p) {
            let at = at + Vec2::new(6.0, -4.0);
            let w = self.ui.width(text, 11.0, Weight::Medium);
            self.ui.p().rounded(omsi_ui::Rect::new(at.x - 3.0, at.y - 11.0, w + 6.0, 15.0), 3.0, Color::rgba(0, 0, 0, 0.55));
            self.ui.text(text, at, 11.0, Weight::Medium, c, Align::Left);
        }
    }

    /// A box of `half` extents round `centre` in the part's frame.
    fn cuboid(&mut self, part: &Part, centre: Vec3, half: Vec3, c: Color) {
        let corner = |i: usize| centre + Vec3::new(if i & 1 == 0 { -half.x } else { half.x }, if i & 2 == 0 { -half.y } else { half.y }, if i & 4 == 0 { -half.z } else { half.z });
        for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
            self.line(part.at(corner(a)), part.at(corner(b)), 1.5, c);
        }
    }

    /// A circle of radius `r` round `centre` in the plane of the part's axes `u` and `v`.
    fn ring(&mut self, part: &Part, centre: Vec3, u: Vec3, v: Vec3, r: f32, c: Color) {
        let n = 24;
        let p = |k: usize| {
            let a = std::f32::consts::TAU * k as f32 / n as f32;
            part.at(centre + (u * a.cos() + v * a.sin()) * r)
        };
        for k in 0..n {
            self.line(p(k), p(k + 1), 1.5, c);
        }
    }

    /// An arrow from `from` along `dir` (part frame), `len` metres.
    fn arrow(&mut self, part: &Part, from: Vec3, dir: Vec3, len: f32, c: Color) {
        let dir = dir.normalize_or_zero();
        let tip = from + dir * len;
        self.line(part.at(from), part.at(tip), 2.0, c);
        let side = dir.cross(Vec3::Z).normalize_or(Vec3::X) * len * 0.15;
        self.line(part.at(tip), part.at(tip - dir * len * 0.25 + side), 2.0, c);
        self.line(part.at(tip), part.at(tip - dir * len * 0.25 - side), 2.0, c);
    }
}

/// The direction a yaw and pitch (degrees, OMSI's: yaw clockwise from forward) point in a
/// vehicle's frame.
fn direction(yaw: f32, pitch: f32) -> Vec3 {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    Vec3::new(p.cos() * y.sin(), p.cos() * y.cos(), p.sin())
}

/// Draw the overlays that are on for the parts of the bus; `var` reads the bus's variables
/// (for the interior lights).
pub(crate) fn draw(ui: &mut Ui, screen: &Screen, on: &Overlays, parts: &[Part], var: &dyn Fn(&str) -> Option<f32>) {
    let mut pen = Pen { ui, screen };
    for (k, part) in parts.iter().enumerate() {
        let def = part.def;
        let files = part.files;
        let paths = files.and_then(|f| f.paths.as_ref());
        let cabin = files.and_then(|f| f.cabin.as_ref());
        let point = |i: i32| paths.and_then(|p| p.points.get(usize::try_from(i).ok()?)).map(|q| Vec3::from(q.pos));
        if on.on[0] {
            if let Some(p) = paths {
                for &(a, b, _) in &p.links {
                    if let (Some(a), Some(b)) = (point(a), point(b)) {
                        pen.line(part.at(a), part.at(b), 2.0, PATH);
                    }
                }
            }
        }
        if on.on[1] {
            if let Some(p) = paths {
                for (i, q) in p.points.iter().enumerate() {
                    let at = part.at(Vec3::from(q.pos));
                    pen.dot(at, 3.0, POINT);
                    pen.label(at, &i.to_string(), POINT);
                }
            }
            // the doors' points: where people come in and go out
            if let Some(c) = cabin {
                for (i, e) in c.entries.iter().enumerate() {
                    if let Some(p) = point(e.path_point) {
                        pen.label(part.at(p + Vec3::Z * 0.25), &format!("{} {i}", omsi_ui::tr("Entry point")), PATH);
                    }
                }
                for (i, e) in c.exits.iter().enumerate() {
                    if let Some(p) = point(*e) {
                        pen.label(part.at(p + Vec3::Z * 0.45), &format!("{} {i}", omsi_ui::tr("Exit point")), PATH);
                    }
                }
            }
        }
        if on.on[2] {
            if let Some(c) = cabin {
                for (list, colour, name) in [(&c.pass_positions, SEAT, "seat"), (&c.driver_positions, DRIVER, "driver")] {
                    for (i, s) in list.iter().enumerate() {
                        // (`height`: the seat's above the floor, 0 for a standing place)
                        let standing = s.height < 0.05;
                        let foot = Vec3::from(s.pos);
                        let head = foot + Vec3::Z * if standing { 1.7 } else { s.height + 0.85 };
                        pen.line(part.at(foot), part.at(head), 2.0, colour);
                        pen.dot(part.at(foot), 3.5, colour);
                        // (the way the seat faces: `[passpos]` rotation, degrees about z)
                        pen.arrow(part, foot + Vec3::Z * 0.05, direction(s.rot, 0.0), 0.4, colour);
                        let kind = match (standing, name) {
                            (true, "seat") => "Standing position",
                            (_, "seat") => "Seat position",
                            _ => "Driver position",
                        };
                        pen.label(part.at(head), &format!("{} {i}", omsi_ui::tr(kind)), colour);
                    }
                }
            }
        }
        if on.on[3] {
            if let Some(bb) = def.bounding_box {
                pen.cuboid(part, Vec3::new(bb[3], bb[4], bb[5]), Vec3::new(bb[0], bb[1], bb[2]) * 0.5, BOX);
                let label = format!("{}: {:.2} x {:.2} x {:.2}", omsi_ui::tr("Bounding box"), bb[0], bb[1], bb[2]);
                pen.label(part.at(Vec3::new(bb[3], bb[4] + bb[1] * 0.5, bb[5] + bb[2] * 0.5)), &label, BOX);
            }
        }
        if on.on[4] {
            for (i, a) in def.axles.iter().enumerate() {
                let r = (a.wheel_diameter * 0.5).max(0.05);
                let half = a.max_width * 0.5;
                let (l, rr) = (Vec3::new(-half, a.long, r), Vec3::new(half, a.long, r));
                pen.line(part.at(l), part.at(rr), 2.0, AXLE);
                for c in [l, rr] {
                    pen.ring(part, c, Vec3::Y, Vec3::Z, r, AXLE);
                }
                let driven = if a.driven { format!(" ({})", omsi_ui::tr("driven")) } else { String::new() };
                let label = format!("{} {i}{driven}  y {:.2}  d {:.2}", omsi_ui::tr("Axle"), a.long, a.wheel_diameter);
                pen.label(part.at(rr + Vec3::Z * r), &label, AXLE);
            }
        }
        if on.on[5] {
            for (i, l) in part.model.interior_lights.iter().enumerate() {
                let at = Vec3::from(l.pos);
                let lit = var(&l.variable).unwrap_or(0.0);
                let c = if lit > 0.0 { LIGHT } else { LIGHT.alpha(0.4) };
                pen.dot(part.at(at), 4.0, c);
                pen.ring(part, at, Vec3::X, Vec3::Y, l.range.clamp(0.05, 3.0), c.alpha(0.35));
                let label = format!("{} {i}: {} = {lit:.2}", omsi_ui::tr("Interior light"), l.variable);
                pen.label(part.at(at), &label, c);
            }
        }
        if on.on[6] {
            if let Some(c) = cabin {
                for (list, name) in [(&c.stampers, "stamper"), (&c.ticket_sales, "ticket sale")] {
                    let name = omsi_ui::tr(name);
                    for (i, s) in list.iter().enumerate() {
                        let at = part.at(Vec3::from(s.pos));
                        pen.dot(at, 4.0, TICKET);
                        let point_source = format!("(path point {})", s.path_point);
                        let point = omsi_ui::tr(&point_source);
                        pen.label(at, &format!("{name} {i} {point}"), TICKET);
                    }
                }
                for (list, name) in [(&c.money_points, "money"), (&c.change_points, "change")] {
                    for (i, s) in list.iter().enumerate() {
                        let at = part.at(Vec3::from(s.pos));
                        pen.dot(at, 3.0, TICKET);
                        let point = if name == "money" { "Money point" } else { "Change point" };
                        pen.label(at, &format!("{} {i}", omsi_ui::tr(point)), TICKET);
                    }
                }
            }
        }
        if on.on[7] {
            for (i, m) in def.cameras_reflexion.iter().enumerate() {
                let at = Vec3::from(m.pos);
                pen.dot(part.at(at), 4.0, MIRROR);
                // (which way the mirror's face is turned, see `camera_util::mirror_view`)
                pen.arrow(part, at, direction(m.yaw, m.pitch), 0.35, MIRROR);
                let source = format!("mirror {i}  fov {:.0}", m.fov);
                let label = omsi_ui::tr(&source);
                pen.label(part.at(at), &label, MIRROR);
            }
        }
        if on.on[8] && k == 0 {
            let cog = def.cog.map(Vec3::from).unwrap_or(Vec3::new(0.0, 0.0, def.cog_height));
            let s = 0.3;
            for d in [Vec3::X, Vec3::Y, Vec3::Z] {
                pen.line(part.at(cog - d * s), part.at(cog + d * s), 2.5, COG);
            }
            pen.dot(part.at(cog), 4.0, COG);
            let label = format!("{} {:.2}, {:.2}, {:.2}", omsi_ui::tr("Centre of gravity"), cog.x, cog.y, cog.z);
            pen.label(part.at(cog), &label, COG);
        }
        if on.on[9] {
            for (list, name) in [(&def.cameras_driver, "driver view"), (&def.cameras_pax, "passenger view")] {
                for (i, cam) in list.iter().enumerate() {
                    let at = Vec3::from(cam.pos);
                    pen.dot(part.at(at), 3.5, VIEW);
                    pen.arrow(part, at, direction(cam.yaw, cam.pitch), 0.5, VIEW);
                    let view = omsi_ui::tr(name);
                    pen.label(part.at(at), &format!("{view} {i}"), VIEW);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Screen {
        let cam = omsi_render::Camera { position: DVec3::new(1000.0, 2000.0, 1.7), yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 5000.0 };
        Screen::new(&cam, Vec2::new(1600.0, 900.0))
    }

    #[test]
    fn a_point_ahead_is_in_the_picture_and_one_behind_is_not() {
        let s = screen();
        let ahead = s.point(DVec3::new(1000.0, 2010.0, 1.7)).unwrap();
        assert!((ahead - Vec2::new(800.0, 450.0)).length() < 0.5, "{ahead}");
        // to the right and up: right of and above the middle
        let p = s.point(DVec3::new(1001.0, 2010.0, 2.7)).unwrap();
        assert!(p.x > 800.0 && p.y < 450.0, "{p}");
        assert!(s.point(DVec3::new(1000.0, 1990.0, 1.7)).is_none());
    }

    #[test]
    fn a_line_through_the_camera_is_cut_in_front_of_it() {
        let s = screen();
        let (a, b) = s.segment(DVec3::new(1000.5, 1995.0, 1.7), DVec3::new(1000.5, 2005.0, 1.7)).unwrap();
        assert!(a.x.is_finite() && b.x.is_finite());
        assert!((b.x - 800.0) > 0.0 && b.x < a.x, "the far end nearer the middle: {a} {b}");
        assert!(s.segment(DVec3::new(1000.0, 1990.0, 1.7), DVec3::new(1000.0, 1995.0, 1.7)).is_none());
    }

    #[test]
    fn yaw_and_pitch_point_as_omsi_turns_them() {
        assert!((direction(0.0, 0.0) - Vec3::Y).length() < 1e-6);
        assert!((direction(90.0, 0.0) - Vec3::X).length() < 1e-6);
        assert!((direction(0.0, 90.0) - Vec3::Z).length() < 1e-6);
    }
}
