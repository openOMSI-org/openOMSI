//! The city map: the whole map in a large window over the game, after Apple Maps - flat from
//! above when zoomed out, tilting into 3D with the houses standing as it zooms in (the `nav_3d`
//! setting, and its own 2D/3D button) - every road and paved surface, the trip's route with its
//! traffic, its stops with their names, the street names, the bus, the other players and every
//! vehicle of the traffic; and the navigator panel's placement, dragging and sizing.

use super::mapmesh::GroundLook;
use super::panel::{car_dot, puck, solid_and_ghost, stop_pin};
use super::style::{self, Palette};
use super::*;

/// The vertical field of view of the city map's camera (degrees).
const CITY_FOV: f32 = 36.0;
/// Closest and farthest zoom (metres a pixel at the middle of the view).
const MIN_MPP: f64 = 0.1;

#[derive(Default)]
pub struct CityMap {
    pub open: bool,
    /// Where the window is on the screen (physical pixels).
    pub rect: [f32; 4],
    /// Centre of the view (world) and metres per pixel.
    pub(super) center: DVec2,
    pub(super) mpp: f64,
    /// The view stays on the bus until the map is dragged.
    pub(super) follow: bool,
    pub(super) drag: Option<(f32, f32)>,
    /// How far the mouse has moved (pixels) since it was pressed on the map: a press let go
    /// without moving is a click.
    travel: f32,
    pub(super) pin_scale: f32,
    pub(super) entry_hits: Vec<(Rect, usize)>,
    pub(super) waypoint_hits: Vec<(Rect, usize)>,
    pub(super) waypoint_remove: Vec<(Rect, usize)>,
    pub(super) pointer: Option<Vec2>,
    target: Option<(TextureId, u32, u32)>,
    /// The ground (buffer 7) and houses (buffer 10): what they were built for and their sizes.
    ground: Option<CityGround>,
    /// The route meshes (buffers 4 and 11): the route, traffic, arrow spacing and look they
    /// were built for, their anchor, and their size.
    route: ((u64, u64, u32, u64), DVec2, u32),
    /// The map's extent (world), for the zoom limits.
    extent: (DVec2, DVec2),
    /// The buttons (window pixels): 0 centre on the bus, 1 zoom in, 2 zoom out, 3 close,
    /// 4 2D / 3D.
    pub(super) buttons: Vec<(Rect, u8)>,
    /// The lines panel: which lines are ticked and what is on show.
    pub(super) lines: LinesView,
    /// The houses stand up when zoomed in (starts as the `nav_3d` setting; the 2D/3D button).
    pub(super) three_d: Option<bool>,
    /// How far the view is tilted into 3D (0 flat … 1), eased.
    tilt: f32,
    /// The turn of the view (radians): the map direction that points up on the screen lies this far clockwise from the
    /// map's up. Eased.
    pub(super) phi: f64,
}

/// A city map ground being built: what for.
pub struct CityJob {
    key: u64,
    centre: DVec2,
    radius: f64,
    mpp: f64,
}

#[derive(Clone, Copy)]
struct CityGround {
    key: u64,
    anchor: DVec2,
    centre: DVec2,
    radius: f64,
    mpp: f64,
    verts: u32,
    /// The houses' shadows, standing and flat (buffers 14, 10 and 15).
    houses: [u32; 3],
}

/// The city map's camera: its view-projection (relative to `centre`) and its inverse.
pub(super) struct CityCam {
    vp: Mat4,
    inv: Mat4,
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Navigator {
    /// Where OMSI 2's dynamic route arrows stand on the route ahead (the `nav_arrows`
    /// setting): every junction lane of the next `reach` metres with the way through it
    /// (`L`, `R`, or `dn` for straight on), and the stops of the trip ahead with their
    /// names (`busstop`). Each: a key that stays the same while it is ahead, the place,
    /// the heading, the kind and its text.
    /// `stop_pose` gives a stop object's place and heading where its tile is loaded.
    pub fn arrow_spots(&self, traffic: Option<&Network>, reach: f64, stop_pose: &dyn Fn(i64) -> Option<(DVec3, f64)>) -> Vec<(u64, DVec3, f64, &'static str, String)> {
        let mut out = Vec::new();
        if !self.arrows {
            return out;
        }
        let global = self.global.clone();
        let Some(net) = global.as_deref().or(traffic) else { return out };
        let r = &self.route;
        if !r.on_route {
            return out;
        }
        let mut acc = -(r.s as f64);
        let mut prev_end: Option<f32> = None;
        for (j, &l) in r.lanes.iter().enumerate().skip(r.progress) {
            let Some(lane) = net.lanes.get(l) else { break };
            let len = lane.length();
            if acc > reach {
                break;
            }
            let (h0, h1) = (lane.start_heading(), lane.end_heading());
            let mut d = omsi_sim::traffic::wrap_deg(h1 - h0);
            if let Some(pe) = prev_end {
                d += omsi_sim::traffic::wrap_deg(h0 - pe);
            }
            let junction = net.crossings.get(l).map(|c| !c.is_empty()).unwrap_or(false) || lane.turn != 0;
            let turn = d.abs() > 35.0 && (len < 60.0 || d.abs() > 70.0);
            if (turn || junction) && acc + len as f64 > 5.0 {
                let kind = if !turn { "dn" } else if d > 0.0 { "R" } else { "L" };
                // ten metres on from where the path begins, as OMSI puts it (the original:
                // the path's start moved by (0, 0, 10) in its own frame); the mesh hangs
                // 6-11 m over that point
                let (p, _) = lane.at(10.0f32.min(len * 0.7));
                // facing the driver coming in: the path's heading where it begins
                let h = h0;
                // the street the route goes on into (the junction's own lanes have no name)
                let text = r.lanes.iter().skip(j).take(5).find_map(|&x| self.street_of(x)).map(str::to_string).unwrap_or_default();
                out.push((l as u64, p, h as f64, kind, text));
            }
            prev_end = Some(h1);
            acc += len as f64;
        }
        // The stop's helper: Omsi.exe puts `routearrows_busstop.sco` on the stop object
        // itself, at its place and with its rotation (0x61fc04: the station record's
        // position +0x3c and quaternion +0x54) - where and how the mapper set the stop down,
        // not turned to the bus as it comes.
        for (k, (p, name, h, id)) in self.stop_spots.iter().enumerate() {
            let (p, h) = stop_pose(*id).unwrap_or((*p, *h));
            let d = (p - self.bus_at).truncate().length();
            if d < reach && k < 2 {
                out.push(((1u64 << 40) + p.x.to_bits().rotate_left(7) ^ p.y.to_bits() ^ h.to_bits(), p, h, "busstop", name.clone()));
            }
        }
        out
    }

    /// Open or close the city map.
    pub fn toggle_map(&mut self) {
        self.city.open = !self.city.open;
        if self.city.open {
            self.city.follow = true;
            if self.city.mpp <= 0.0 {
                // For repeatable offscreen GPS comparisons at a chosen zoom.
                self.city.mpp = omsi_cfg::flags::OMSI_NAV_MAP_MPP.parse::<f64>().filter(|v| v.is_finite() && (MIN_MPP..=20.0).contains(v)).unwrap_or(2.5);
            }
            self.city.tilt = self.tilt_for(self.city.mpp);
        }
        self.city.drag = None;
    }

    pub fn map_open(&self) -> bool {
        self.city.open
    }

    /// Where the small navigator is on the screen, when it is shown.
    pub fn screen_rect(&self) -> Option<[f32; 4]> {
        let r = self.panel_rect;
        (self.enabled && !self.city.open && r[2] > r[0]).then_some(r)
    }

    /// The point (physical pixels) is on the small navigator.
    pub fn over_panel(&self, x: f32, y: f32) -> bool {
        let r = self.panel_rect;
        self.enabled && x >= r[0] && y >= r[1] && x < r[2] && y < r[3]
    }

    /// Where the small navigator (`size`) goes on a screen of `screen`: its corner, the top
    /// middle with the on-screen controls (`touch`), below the information bar (`info`) where
    /// they would lie over each other - unless it was dragged somewhere, which it keeps
    /// (kept inside the window), by the mouse or a finger.
    pub(super) fn panel_origin(&mut self, screen: (f32, f32), size: (f32, f32), touch: bool, info: Option<[f32; 4]>, gap: Option<[f32; 3]>) -> (f32, f32) {
        let ((sw, sh), (pw, ph)) = (screen, size);
        let margin = (sh * 0.018).max(10.0).round();
        // (with the on-screen controls the corners are theirs: the top middle)
        let right = self.corner.contains("right");
        let top = self.corner.contains("top") || touch;
        // ("top-center": a phone's, between its on-screen buttons)
        let x0 = if self.corner.contains("center") || touch { ((sw - pw) * 0.5).round() } else if right { sw - margin - pw } else { margin };
        let y0 = if top { margin } else { sh - margin - ph };
        // (below the information bar where they would lie over each other: in its rows on a
        // phone it took the navigator's place in the top middle, #1164)
        let y0 = match info {
            Some(i) if top && x0 < i[2] && x0 + pw > i[0] => y0.max((i[3] + margin * 0.5).round()),
            _ => y0,
        };
        // (and below the on-screen buttons where it is wider than the gap between them)
        let y0 = match gap {
            Some([l, r, bottom]) if top && (x0 < l || x0 + pw > r) => y0.max((bottom + margin * 0.5).round()),
            _ => y0,
        };
        // (dragged somewhere else: there - on a phone too, where it had to stay in the
        // middle it covered, #1138)
        let room = [(sw - pw).max(0.0), (sh - ph).max(0.0)];
        self.panel_room = room;
        match self.at {
            Some(a) => ((a[0].clamp(0.0, 1.0) * room[0]).round(), (a[1].clamp(0.0, 1.0) * room[1]).round()),
            None => (x0, y0),
        }
    }

    /// The mouse button went down on the small navigator: a click opens the city map, a
    /// drag moves the navigator (see [`Navigator::panel_move`]).
    pub fn panel_press(&mut self, x: f32, y: f32) {
        // (the widget's corner: pulled, it sizes the navigator)
        let r = self.panel_rect;
        let grip = 22.0 * ((r[2] - r[0]) / 360.0).max(0.8);
        if self.widget && x > r[2] - grip && y > r[3] - grip {
            self.resizing = Some((self.size, [x, y], (r[2] - r[0]).max(100.0)));
        }
        self.panel_drag = Some((
            [x, y],
            [self.panel_rect[0] - self.origin_x, self.panel_rect[1]],
            false,
        ));
    }

    /// The cursor moved with the button held on the navigator: past a few pixels it follows
    /// the cursor. True while it is being dragged.
    pub fn panel_move(&mut self, x: f32, y: f32) -> bool {
        let Some((from, rect, moved)) = self.panel_drag.as_mut() else { return false };
        let (dx, dy) = (x - from[0], y - from[1]);
        if !*moved && dx.hypot(dy) < 6.0 {
            return false;
        }
        // (not a widget: it stays in its corner, a press only opens the city map)
        if !self.widget {
            return false;
        }
        *moved = true;
        if let Some((size0, at, w0)) = self.resizing {
            let grow = ((x - at[0]) + (y - at[1])) * 0.5;
            self.size = (size0 * (w0 + grow) / w0).clamp(0.6, 2.0);
            return true;
        }
        let room = self.panel_room;
        let share = |p: f32, r: f32| if r > 0.0 { (p / r).clamp(0.0, 1.0) } else { 0.0 };
        self.at = Some([share(rect[0] + dx, room[0]), share(rect[1] + dy, room[1])]);
        true
    }

    /// The button came up after a press on the navigator: Some(true) when it was dragged
    /// (the place is then the setting's, [`Navigator::placement`]), Some(false) for a click.
    pub fn panel_release(&mut self) -> Option<bool> {
        self.resizing = None;
        self.panel_drag.take().map(|d| d.2)
    }

    /// The setting `navigator_corner` for where the navigator is now.
    pub fn placement(&self) -> String {
        match self.at {
            Some(a) => format!("at {:.3},{:.3}", a[0], a[1]),
            None => self.corner.clone(),
        }
    }

    fn map_hit(&self, x: f32, y: f32) -> bool {
        let r = self.city.rect;
        x >= r[0] && y >= r[1] && x < r[2] && y < r[3]
    }

    /// How far the view should tilt at a zoom (0 flat from above … 1 in 3D).
    fn tilt_for(&self, mpp: f64) -> f32 {
        if !self.city.three_d.unwrap_or(self.show_3d) || self.map_data.is_none() {
            return 0.0;
        }
        smoothstep(1.3, 0.45, mpp as f32)
    }

    /// The camera at `mpp` with the view `tilt`ed, looking at `centre` (relative to it), for a
    /// window of `w` x `h` pixels: north up.
    pub(super) fn city_cam(mpp: f64, tilt: f32, phi: f64, w: f32, h: f32) -> CityCam {
        let t = (CITY_FOV.to_radians() * 0.5).tan();
        let dist = (mpp * h as f64 / (2.0 * t as f64)) as f32;
        let pitch = (90.0 - 40.0 * tilt).to_radians();
        let eye = Vec3::new(0.0, -dist * pitch.cos(), dist * pitch.sin());
        let up = Vec3::new(0.0, pitch.sin(), pitch.cos());
        // (the view turned: the camera goes round the vertical axis)
        let turn = Mat3::from_rotation_z(-phi as f32);
        let (eye, up) = (turn * eye, turn * up);
        let view = Mat4::look_at_rh(eye, Vec3::ZERO, up);
        let proj = Mat4::perspective_rh(CITY_FOV.to_radians(), w / h.max(1.0), (dist * 0.02).max(0.5), dist * 6.0 + 4000.0);
        let vp = proj * view;
        CityCam { vp, inv: vp.inverse() }
    }

    /// The ground point under a window point (pixels from the map's top left), relative to the
    /// view's centre.
    pub(super) fn unproject(cam: &CityCam, w: f32, h: f32, x: f32, y: f32) -> Option<DVec2> {
        let ndc = Vec2::new(x / w * 2.0 - 1.0, 1.0 - y / h * 2.0);
        let a = cam.inv.project_point3(ndc.extend(0.0));
        let b = cam.inv.project_point3(ndc.extend(1.0));
        let d = b - a;
        if d.z.abs() < 1e-6 {
            return None;
        }
        let k = -a.z / d.z;
        (k > 0.0).then(|| (a + d * k).truncate().as_dvec2())
    }

    fn city_size(&self) -> (f32, f32) {
        let r = self.city.rect;
        ((r[2] - r[0]).max(1.0), (r[3] - r[1]).max(1.0))
    }

    /// The world point under a window point at the view as it is.
    fn city_world_at(&self, x: f32, y: f32) -> Option<DVec2> {
        let (w, h) = self.city_size();
        let cam = Self::city_cam(self.city.mpp, self.city.tilt, self.city.phi, w, h);
        Self::unproject(&cam, w, h, x - self.city.rect[0], y - self.city.rect[1]).map(|d| self.city.center + d)
    }

    /// A ground point in the city map's local screen coordinates.
    pub(super) fn city_screen_at(&self, at: DVec2) -> Option<Vec2> {
        let (w, h) = self.city_size();
        let cam = Self::city_cam(self.city.mpp, self.city.tilt, self.city.phi, w, h);
        project(cam.vp, [0.0, 0.0, w, h], (at - self.city.center).as_vec2().extend(0.0))
    }

    /// A mouse press while the map is open: on it, a drag or a button; outside, nothing (the map
    /// closes with its cross or Escape only).
    pub fn map_press(&mut self, x: f32, y: f32) {
        if !self.map_hit(x, y) {
            return;
        }
        let local = Vec2::new(x - self.city.rect[0], y - self.city.rect[1]);
        let hit = self.city.buttons.iter().find(|(r, _)| r.contains(local)).map(|b| b.1);
        if hit.is_none() && !self.city.lines.has_point(local) {
            if let Some(index) = self.city.waypoint_remove.iter().rev().find(|(r, _)| r.contains(local)).map(|(_, i)| *i) {
                self.remove_destination(index);
                self.city.drag = None;
                return;
            }
            if self.city.waypoint_hits.iter().any(|(r, _)| r.contains(local)) {
                self.city.drag = None;
                return;
            }
        }
        // the lines panel: its rows and tabs, or its background (never a drag of the map)
        if hit.is_none() && self.city.lines.has_point(local) {
            if let Some(h) = self.city.lines.hit(local) {
                self.lines_click(h);
            }
            return;
        }
        match hit {
            Some(5) => self.city.lines.toggle(),
            Some(6) => self.show_contours = !self.show_contours,
            Some(7) => self.orient = (self.orient + 1) % 3,
            Some(8) => self.cancel_destination(),
            Some(9) => {},
            Some(0) => self.city.follow = true,
            Some(1) => self.city.mpp = (self.city.mpp / 1.6).max(MIN_MPP),
            Some(2) => self.city.mpp = (self.city.mpp * 1.6).min(self.max_mpp()),
            Some(3) => self.city.open = false,
            Some(4) => {
                let on = !self.city.three_d.unwrap_or(self.show_3d);
                self.city.three_d = Some(on);
                // (switched on far out: zoomed in to where the houses stand)
                if on && self.city.mpp > 0.9 {
                    self.city.mpp = 0.6;
                }
            }
            _ => {
                self.city.drag = Some((x, y));
                self.city.travel = 0.0;
            }
        }
    }

    /// The world point (x, y) under the window point, when the map is open and the point
    /// is on it.
    pub fn map_point(&self, x: f32, y: f32) -> Option<DVec2> {
        if !self.city.open || !self.map_hit(x, y) {
            return None;
        }
        self.city_world_at(x, y)
    }

    /// The mouse let go over the map: true when the press was a click on the map itself
    /// (not a drag, not one of its buttons).
    pub fn map_release(&mut self) -> bool {
        self.city.drag.take().is_some() && self.city.travel < 6.0
    }

    /// The cursor moved with the button held: the ground under it stays under it.
    pub fn map_move(&mut self, x: f32, y: f32) {
        self.city.pointer = Some(Vec2::new(x - self.city.rect[0], y - self.city.rect[1]));
        if let Some((px, py)) = self.city.drag {
            let (dx, dy) = (x - px, y - py);
            self.city.travel += dx.abs() + dy.abs();
            if dx.abs() + dy.abs() > 0.5 {
                self.city.follow = false;
            }
            if let (Some(a), Some(b)) = (self.city_world_at(px, py), self.city_world_at(x, y)) {
                let d = a - b;
                // (dragged towards the horizon of a tilted view: never more than a screen at once)
                let cap = self.city.mpp * self.city_size().0 as f64;
                self.city.center += if d.length() > cap { d.normalize() * cap } else { d };
            }
            self.city.drag = Some((x, y));
        }
    }

    /// The wheel over the map: zoom, keeping the point under the cursor where it is.
    pub fn map_wheel(&mut self, amount: f32, x: f32, y: f32) {
        // over the lines list the wheel scrolls it (anywhere else it zooms the map)
        let r = self.city.rect;
        if self.city.lines.list_has_point(Vec2::new(x - r[0], y - r[1])) {
            self.city.lines.scroll_by(-amount * 2.0);
            return;
        }
        let before = self.city_world_at(x, y);
        let k = (1.0 - amount as f64 * 0.15).clamp(0.6, 1.6);
        self.city.mpp = (self.city.mpp * k).clamp(MIN_MPP, self.max_mpp());
        self.city.tilt = self.tilt_for(self.city.mpp);
        let after = self.city_world_at(x, y);
        if let (Some(b), Some(a)) = (before, after) {
            self.city.center += b - a;
        }
        let r = self.city.rect;
        let (lx, ly) = (x - (r[0] + r[2]) * 0.5, y - (r[1] + r[3]) * 0.5);
        if amount.abs() > 0.0 && (lx.abs() > 40.0 || ly.abs() > 40.0) {
            self.city.follow = false;
        }
    }

    pub(super) fn max_mpp(&self) -> f64 {
        let (lo, hi) = self.city.extent;
        let span = (hi - lo).max_element().max(2000.0);
        let r = self.city.rect;
        span / ((r[2] - r[0]).max(200.0) as f64) * 1.2
    }

    /// The map's extent (for the zoom limits): its layers' tiles, and its carriageways.
    fn city_extent(&mut self, net: &Network) {
        let roads = self.road_list(net);
        let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
        for (l, _) in roads.iter() {
            for p in [l.points.first(), l.points.last()].into_iter().flatten() {
                lo = lo.min(p.truncate());
                hi = hi.max(p.truncate());
            }
        }
        if let Some((a, b)) = self.map_data.as_ref().and_then(|m| m.extent()) {
            lo = lo.min(a);
            hi = hi.max(b);
        }
        if lo.x <= hi.x {
            self.city.extent = (lo, hi);
        }
    }

    /// The city map's ground and houses around the view, built on a worker when the view left
    /// what was built (the picture keeps the old part meanwhile): what has just arrived.
    fn city_ground(&mut self, device: &wgpu::Device, net: Option<&Network>, w: f32, h: f32, pal: &Palette) -> Option<super::build::Built> {
        let mpp = self.city.mpp;
        let tilt = self.city.tilt;
        let centre = self.city.center;
        // (tilted, the far part of the view reaches well beyond the middle's scale)
        let radius = (w.max(h) as f64 * mpp * (0.8 + 1.4 * tilt as f64)).max(500.0);
        let detail = (mpp < 0.7) as u64 | ((mpp < 2.2) as u64) << 1 | ((mpp < 3.5) as u64) << 2 | ((mpp < 6.0) as u64) << 3;
        let band = self.contour_band();
        let key = self.look_key() ^ detail << 40 ^ self.global_version << 48 ^ band.map_or(0, |b| 1 + b.1) << 60;
        let mut arrived = None;
        if let Some(job) = self.city_job.as_ref() {
            match job.poll() {
                Ok(Some(b)) => {
                    let m = &job.meta;
                    let [verts, houses @ ..] = b.counts.map(|n| n as u32);
                    self.city.ground = Some(CityGround { key: m.key, anchor: b.anchor, centre: m.centre, radius: m.radius, mpp: m.mpp, verts, houses });
                    arrived = Some(b);
                    self.city_job = None;
                }
                Ok(None) => return None,
                Err(()) => self.city_job = None,
            }
        }
        let stale = match self.city.ground {
            None => true,
            Some(g) => g.key != key || (g.centre - centre).length() > g.radius * 0.35 || g.mpp / mpp > 1.7 || mpp / g.mpp > 1.7,
        };
        if stale && arrived.is_none() {
            let look = GroundLook { surfaces: mpp < 0.7, trees: mpp < 0.7, dashes: mpp < 3.5, tol: (mpp * 0.5) as f32 };
            let parts = self.ground_parts(net, band.map(|b| b.0));
            let pal2 = *pal;
            let houses = mpp < 6.0;
            let job = move || super::build::build(&parts, centre, radius, &pal2, look, mpp < 2.2, houses.then_some(omsi_mapdraw::MODEL_TRIANGLES));
            if self.city.ground.is_none() {
                // (the first picture at once: nothing to show meanwhile)
                let b = job();
                let [verts, houses @ ..] = b.counts.map(|n| n as u32);
                self.city.ground = Some(CityGround { key, anchor: b.anchor, centre, radius, mpp, verts, houses });
                return Some(b.made_on(device));
            }
            let device = device.clone();
            self.city_job = Some(super::build::Job::start(CityJob { key, centre, radius, mpp }, move || job().made_on(&device)));
        }
        arrived
    }

    /// Draw the city map and lay it over the picture.
    /// The whole route for the city map (buffers 4 and 11), when it changed: the part driven
    /// pale, the rest by its traffic, with chevrons about 90 pixels apart.
    fn city_route(&mut self, net: Option<&Network>, f: &NavFrame, anchor: DVec2, s: f32, pal: &Palette) -> Option<Painter> {
        let rel = |q: DVec3| Vec3::new((q.x - anchor.x) as f32, (q.y - anchor.y) as f32, 0.0);
        let every = 2f64.powf((90.0 * s as f64 * self.city.mpp).log2().round()) as f32;
        let stamp = (self.route.version, self.jam_version, every.to_bits(), self.look_key());
        if self.city.route.0 == stamp && self.city.route.1 == anchor {
            return None;
        }
        let mut p = Painter::new();
        if let Some(n) = net {
            let r = &self.route;
            let done = r.progress.min(r.lanes.len());
            for &l in &r.lanes[..done] {
                let Some(lane) = n.lanes.get(l) else { continue };
                let pts: Vec<Vec3> = lane.points.iter().map(|q| rel(*q)).collect();
                p.ribbon(&pts, lane.width.max(3.0), 4.0, pal.driven, true);
            }
            let style = RouteStyle { extra_m: 0.0, min_px: 6.0, arrows: Some((every, f32::MAX)), max_len: f32::MAX, near: None };
            let bus_lane = r.lanes.get(done).map(|&l| lane_from_right(n, l, f.bus)).unwrap_or(0);
            mapmesh::roads::build_route_to(&mut p, n, &r.lanes[done..], anchor, r.s, &self.route_jam, &style, bus_lane, pal, self.destination.as_ref().and_then(|d| d.path.as_ref().map(|p| p.end_s)));
            self.draw_waypoint_routes(&mut p, n, anchor, &style, pal);
        }
        self.city.route = (stamp, anchor, p.len());
        Some(p)
    }

    pub(super) fn city(&mut self, renderer: &Renderer, scene: &mut Scene, f: &NavFrame) {
        self.atlas.begin_frame();
        let pal = *style::palette(self.light);
        let (sw, sh) = f.screen;
        let (w, h) = ((sw * 0.86).round(), (sh * 0.86).round());
        let (x0, y0) = (((sw - w) * 0.5).round(), ((sh - h) * 0.5).round());
        self.city.rect = [x0, y0, x0 + w, y0 + h];
        let (tw, th) = (w as u32, h as u32);
        if self.gpu.is_none() {
            self.gpu = Some(Gpu::with_depth(&renderer.device, renderer.format(), map_samples(renderer.format()), self.atlas.size));
        }
        if self.city.target.map(|t| (t.1, t.2) != (tw, th)).unwrap_or(true) {
            if let Some((t, _, _)) = self.city.target.take() {
                renderer.free_texture(scene, t);
                scene.premultiplied.remove(&t);
            }
            let t = renderer.add_render_texture(scene, tw, th);
            scene.premultiplied.insert(t);
            self.city.target = Some((t, tw, th));
        }
        if self.city.follow {
            self.city.center = f.bus.truncate();
        }
        let want_tilt = self.tilt_for(self.city.mpp);
        self.city.tilt += (want_tilt - self.city.tilt) * (1.0 - (-f.dt * 7.0).exp());
        if (self.city.tilt - want_tilt).abs() < 0.01 {
            self.city.tilt = want_tilt;
        }
        let phi = self.ease_turn(f.heading, f.dt as f64);
        let s = (h / 760.0).clamp(0.95, 2.0) * f.ui_scale;
        self.city.pin_scale = s;
        let global = self.global.clone();
        let net = global.as_deref().or(f.traffic.map(|t| &t.net));
        if let Some(n) = net {
            self.city_extent(n);
        }
        let built = self.city_ground(&renderer.device, net, w, h, &pal);
        let anchor = self.city.ground.map(|g| g.anchor).unwrap_or(self.city.center);
        let rel = |q: DVec3| Vec3::new((q.x - anchor.x) as f32, (q.y - anchor.y) as f32, 0.0);
        let route_verts = self.city_route(net, f, anchor, s, &pal).map(|p| p.verts);
        // the ways of the ticked lines (buffer 18)
        let ln_verts = self.lines_mesh(anchor, w, h, s);
        let ln_n = self.lines_mesh_len();
        // the camera, relative to the anchor
        let cam = Self::city_cam(self.city.mpp, self.city.tilt, self.city.phi, w, h);
        let shift = (self.city.center - anchor).as_vec2();
        let vpm = cam.vp * Mat4::from_translation(-shift.extend(0.0));
        let vp = [0.0, 0.0, w, h];
        let world = Layer { view_proj: vpm, viewport: vp, clip: [0.0, 0.0, w, h], radius: 14.0 * s, opacity: 1.0, px_scale: 2.0 * (CITY_FOV.to_radians() * 0.5).tan() / h, depth: false, shadow: false, height: 1.0 };
        let depth = world.with_depth();
        let houses = depth.with_height(self.city.tilt);
        let shadows = world.with_shadow().with_height(self.city.tilt);
        let to_screen = |q: DVec3| -> Option<Vec2> { project(vpm, vp, rel(q)) };
        // (for the package's places: a point behind the camera lies far off the window)
        let on_screen = |q: DVec3| -> Vec2 { to_screen(q).unwrap_or(Vec2::splat(-1e6)) };
        let win = Rect::new(0.0, 0.0, w, h);
        let mut bg = Painter::new();
        bg.rounded(win, 14.0 * s, pal.land);
        let n_bg = bg.len();
        // the traffic: every car, the public transport in its colours with its line
        let mut dots = Painter::new();
        let mut transit: Vec<(DVec3, f64, Option<String>)> = Vec::new();
        // (from far the cars are left out; the map's own timetabled trains always)
        let far = self.city.mpp > 1.6;
        if let Some(t) = f.traffic.filter(|_| self.show_ai) {
            for car in t.cars.iter().filter(|c| !c.gone && !c.is_rail() && !in_the_air(c, &t.net)) {
                let (color, line) = traffic_kind(car);
                let at = self.smoothed_position(car);
                if color == DOT {
                    if !far {
                        car_dot(&mut dots, rel(at), self.light);
                    }
                } else {
                    transit.push((at, car.vehicle.heading, line));
                }
            }
        }
        if self.show_ai {
            for v in self.live_vehicles().into_iter().filter(|v| !v.rail) {
                transit.push((v.pos, v.heading, Some(v.line.clone())));
            }
        }
        // the vehicles on a ticked line ringed in that line's colour (and counted for its card)
        let on_line = self.ring_ticked_lines(&transit, anchor, &mut dots);
        // (over the route in their order, no depth: see the small map)
        let dots_solid = dots.verts;
        // marks, names, cards
        let mut ui = Painter::new();
        let n_stops = f.stops.len();
        let markers = spaced_markers(f.stops.iter().enumerate().filter_map(|(k, st)| to_screen(st.position).map(|p| (k, p))).filter(|(_, p)| win.contains(*p)), 20.0 * s);
        let mut taken: Vec<Rect> = markers.iter().map(|(_, p)| Rect::new(p.x - 10.0 * s, p.y - 10.0 * s, 20.0 * s, 20.0 * s)).collect();
        taken.push(Rect::new(0.0, 0.0, w * 0.55, 70.0 * s));
        taken.push(Rect::new(w - 70.0 * s, 0.0, 70.0 * s, 230.0 * s));
        let mut stop_labels = Vec::new();
        for &(k, p) in &markers {
            if self.city.mpp >= 4.0 && k != 0 && k + 1 != n_stops {
                continue;
            }
            let st = &f.stops[k];
            let weight = if k == 0 { Weight::Bold } else { Weight::Medium };
            let name = format!("{}  {:02}:{:02}", st.name.trim(), (st.arrival / 3600.0) as i32 % 24, ((st.arrival % 3600.0) / 60.0) as i32);
            let name = self.fonts.fit(&name, 12.5 * s, weight, (300.0 * s).min(w * 0.3));
            let lw = self.fonts.width(&name, 12.5 * s, weight) + 16.0 * s;
            if let Some(r) = stop_label_rect(p, lw, s, win, &taken) {
                taken.push(r);
                stop_labels.push((k, name, r));
            }
        }
        transit.sort_by(|a, b| (a.0 - f.bus).length().total_cmp(&(b.0 - f.bus).length()));
        for (pos, heading, line) in &transit {
            let Some(p) = to_screen(*pos).filter(|p| win.contains(*p)) else { continue };
            let h = heading.to_radians();
            let Some(ahead) = to_screen(*pos + DVec3::new(h.sin(), h.cos(), 0.0) * 5.0) else { continue };
            transit_drop(&mut ui, &mut self.atlas, &self.fonts, p, ahead - p, line.as_deref(), s, &mut taken, &pal);
        }
        if let Some(sf) = self.surface.as_deref().filter(|_| self.show_surface) {
            self.place.draw_names(&mut ui, &mut self.atlas, &self.fonts, sf, self.light, self.city.mpp, &mut taken, win, s, &on_screen);
        }
        // street names along their roads, with a halo, where there is room
        if let (Some(st), true) = (self.streets.clone(), self.city.mpp < 3.2) {
            let px = 12.0 * s;
            for (q, a, id) in &st.labels {
                let Some(p) = to_screen(q.extend(0.0)) else { continue };
                if !win.pad(60.0 * s, 30.0 * s).contains(p) {
                    continue;
                }
                // the road's direction on the screen (tilted views turn it)
                let d = DVec2::new(a.cos() as f64, a.sin() as f64);
                let Some(ahead) = to_screen((*q + d * 10.0).extend(0.0)) else { continue };
                let dir = ahead - p;
                let mut ang = dir.y.atan2(dir.x);
                if ang > std::f32::consts::FRAC_PI_2 {
                    ang -= std::f32::consts::PI;
                } else if ang <= -std::f32::consts::FRAC_PI_2 {
                    ang += std::f32::consts::PI;
                }
                let name = &st.names[*id as usize];
                let tw = self.fonts.width(name, px, Weight::Medium);
                let (hx, hy) = ((ang.cos() * tw * 0.5).abs() + (ang.sin() * px * 0.6).abs(), (ang.sin() * tw * 0.5).abs() + (ang.cos() * px * 0.6).abs());
                let bb = Rect::new(p.x - hx, p.y - hy, hx * 2.0, hy * 2.0);
                if taken.iter().any(|t| rects_overlap(t, &bb)) {
                    continue;
                }
                taken.push(bb);
                for o in [Vec2::new(1.2, 0.0), Vec2::new(-1.2, 0.0), Vec2::new(0.0, 1.2), Vec2::new(0.0, -1.2)] {
                    ui.text_rotated(&mut self.atlas, &self.fonts, name, px, Weight::Medium, p + o * s, ang, pal.label_halo);
                }
                ui.text_rotated(&mut self.atlas, &self.fonts, name, px, Weight::Medium, p, ang, pal.label);
            }
        }
        self.terrain_labels(&mut ui, &on_screen, win, s, &mut taken);
        for (k, p) in markers.into_iter().rev() {
            if self.city.mpp < 4.0 || k == 0 || k + 1 == n_stops {
                stop_pin(&mut ui, &mut self.atlas, p, k == 0, k + 1 == n_stops, s);
            } else {
                ui.circle(p, 5.0 * s, Color::WHITE);
                ui.circle(p, 3.4 * s, Color::hex(0x5d6b80));
            }
        }
        for (k, name, r) in stop_labels {
            ui.shadow(r, 7.0 * s, 4.0 * s, Color::rgba(0, 0, 0, 0.12));
            ui.rounded(r, 7.0 * s, pal.card);
            ui.text_in(&mut self.atlas, &self.fonts, &name, 12.5 * s, if k == 0 { Weight::Bold } else { Weight::Medium }, r.pad(8.0 * s, 0.0), Align::Left, if k == 0 { pal.card_text } else { pal.card_dim });
        }
        for pl in &f.players {
            let Some(p) = to_screen(pl.position).filter(|p| win.contains(*p)) else { continue };
            puck(&mut ui, p, ((pl.heading as f64).to_radians() - phi) as f32, 8.5 * s, PLAYER);
            let px = 11.0 * s;
            let name = self.fonts.fit(&pl.name, px, Weight::Bold, 140.0 * s);
            let tw = self.fonts.width(&name, px, Weight::Bold) + 12.0 * s;
            let r = Rect::new(p.x - tw * 0.5, p.y - 30.0 * s, tw, 16.0 * s);
            if taken.iter().any(|o| rects_overlap(o, &r)) {
                continue;
            }
            taken.push(r);
            ui.rounded(r, 8.0 * s, PLAYER);
            ui.text_in(&mut self.atlas, &self.fonts, &name, px, Weight::Bold, r, Align::Center, Color::WHITE);
        }
        if let Some(bp) = to_screen(f.bus) {
            puck(&mut ui, bp, ((f.heading as f64).to_radians() - phi) as f32, 10.0 * s, pal.route);
        }
        self.draw_destination_markers(&mut ui, &|q| to_screen(q.extend(0.0)), win, s, true, &pal);
        let icon_verts = self.city_icons(&on_screen, win, s);
        // --- the cards: the trip top left, the buttons top right, the scale bottom left
        let pad = 14.0 * s;
        let wd = &WORDS;
        let head = Rect::new(pad, pad, (w * 0.5).min(520.0 * s), 58.0 * s);
        ui.shadow(head, 14.0 * s, 10.0 * s, Color::rgba(0, 0, 0, 0.14));
        ui.rounded(head, 14.0 * s, pal.card);
        let mut x = head.x + 14.0 * s;
        if let Some(l) = f.line.as_deref().map(str::trim).filter(|l| !l.is_empty() && self.destination.is_none()) {
            let px = 13.0 * s;
            let l = self.fonts.fit(l, px, Weight::Bold, 60.0 * s);
            let lw = self.fonts.width(&l, px, Weight::Bold) + 14.0 * s;
            let r = Rect::new(x, head.y + 10.0 * s, lw, 20.0 * s);
            ui.rounded(r, 5.0 * s, BUS);
            ui.text_in(&mut self.atlas, &self.fonts, &l, px, Weight::Bold, r, Align::Center, Color::WHITE);
            x += lw + 9.0 * s;
        }
        let title = self.destination.as_ref().map(|d| format!("1/{} · {}", self.waypoints.len() + 1, d.name)).unwrap_or_else(|| f.terminus.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string).unwrap_or_else(|| wd.no_duty.to_string()));
        let title = self.fonts.fit(&title, 15.0 * s, Weight::Bold, head.right() - x - 12.0 * s);
        ui.text_in(&mut self.atlas, &self.fonts, &title, 15.0 * s, Weight::Bold, Rect::new(x, head.y + 8.0 * s, head.right() - x - 12.0 * s, 24.0 * s), Align::Left, pal.card_text);
        let sub = if self.destination.is_some() { self.destination_info(net) } else { match f.stops.first() {
            Some(st) => {
                let d = self.next_dist.map(|(_, d)| omsi_ui::units::Units::now().distance(d)).unwrap_or_default();
                format!("{}  ·  {}  ·  {:02}:{:02}", st.name.trim(), d, (st.arrival / 3600.0) as i32 % 24, ((st.arrival % 3600.0) / 60.0) as i32)
            }
            None => omsi_ui::tr("Click map to navigate").into_owned(),
        } };
        let sub = self.fonts.fit(&sub, 12.5 * s, Weight::Medium, head.w - 28.0 * s);
        ui.text_in(&mut self.atlas, &self.fonts, &sub, 12.5 * s, Weight::Medium, Rect::new(head.x + 14.0 * s, head.y + 32.0 * s, head.w - 28.0 * s, 20.0 * s), Align::Left, pal.card_dim);
        // buttons: close apart, then a column (2D/3D, zoom in, zoom out, the bus)
        self.city.buttons.clear();
        let bs = 38.0 * s;
        let bx = w - pad - bs;
        let close = Rect::new(bx, pad, bs, bs);
        ui.shadow(close, bs * 0.5, 8.0 * s, Color::rgba(0, 0, 0, 0.14));
        ui.rounded(close, bs * 0.5, pal.card);
        ui.icon(&mut self.atlas, "close", close.center(), 20.0 * s, pal.card_text);
        self.city.buttons.push((close, 3));
        let col = Rect::new(bx, close.bottom() + 12.0 * s, bs, bs * 4.0);
        ui.shadow(col, 12.0 * s, 8.0 * s, Color::rgba(0, 0, 0, 0.14));
        ui.rounded(col, 12.0 * s, pal.card);
        let three = self.city.three_d.unwrap_or(self.show_3d);
        for (k, id) in [4u8, 1, 2, 0].into_iter().enumerate() {
            let r = Rect::new(col.x, col.y + k as f32 * bs, bs, bs);
            if k > 0 {
                ui.rect(Rect::new(r.x + 8.0 * s, r.y, r.w - 16.0 * s, 1.0), pal.card_dim.alpha(0.25));
            }
            match id {
                4 => {
                    ui.text_in(&mut self.atlas, &self.fonts, if three { "3D" } else { "2D" }, 13.0 * s, Weight::Bold, r, Align::Center, if three { pal.route } else { pal.card_text });
                }
                0 => ui.icon(&mut self.atlas, if self.city.follow { "navigation" } else { "near_me" }, r.center(), 20.0 * s, if self.city.follow { pal.route } else { pal.card_text }),
                1 => ui.icon(&mut self.atlas, "add", r.center(), 22.0 * s, pal.card_text),
                _ => ui.icon(&mut self.atlas, "remove", r.center(), 22.0 * s, pal.card_text),
            }
            self.city.buttons.push((r, id));
        }
        // the Terrain and Lines buttons left of the close button, the lines panel left of the column, the compass
        self.draw_top_controls(&mut ui, (w, h, s), (pad, bs, bx, close), &on_line, &on_screen);
        self.draw_destination_controls(&mut ui, head, s, &pal);
        if self.show_surface && self.place.has_location() {
            self.place.draw_chips(&mut ui, &mut self.atlas, &self.fonts, head, w, pad, s);
        }
        self.draw_credit(&mut ui, w, h, s);
        self.draw_scale_bar(&mut ui, h, pad, s, &pal);

        // --- to the GPU
        let (tex, _, _) = self.city.target.unwrap();
        let Some(view) = renderer.texture_view(scene, tex) else { return };
        let (Some(gpu), device, queue) = (self.gpu.as_mut(), &renderer.device, &renderer.queue) else { return };
        if let Some(b) = built {
            b.put(gpu, device, queue, [7, 14, 10, 15]);
        }
        if let Some(v) = route_verts {
            let (solid, ghost) = solid_and_ghost(&v, 0.3);
            gpu.upload(device, queue, 4, &solid);
            gpu.upload(device, queue, 11, &ghost);
        }
        if let Some(v) = ln_verts {
            gpu.upload(device, queue, 18, &v);
        }
        let mut all = bg.verts;
        let n_dots = dots_solid.len() as u32;
        all.extend(dots_solid);
        let n_ui = all.len() as u32;
        all.extend(ui.verts);
        gpu.upload(device, queue, 5, &all);
        let (icon_n, icon_tex) = self.place.upload(gpu, device, queue, 17, &icon_verts);
        gpu.upload_atlas(queue, &mut self.atlas);
        let flat = Layer::flat([0.0, 0.0, w, h], 14.0 * s, 1.0);
        let layers = [flat, world, shadows, houses, depth];
        let (ground_n, [shadow_n, solid_n, flat_n]) = self.city.ground.map(|g| (g.verts, g.houses)).unwrap_or((0, [0; 3]));
        let route_n = self.city.route.2;
        // (from the 2D view the houses are their flat roofs; as it tilts they rise)
        let stand = self.city.tilt > 0.03;
        let draws = [
            Draw { buffer: 5, range: 0..n_bg, layer: 0, texture: 0 },
            Draw { buffer: 7, range: 0..ground_n, layer: 1, texture: 0 },
            Draw { buffer: 15, range: 0..if stand { 0 } else { flat_n }, layer: 1, texture: 0 },
            Draw { buffer: 14, range: 0..if stand { shadow_n } else { 0 }, layer: 2, texture: 0 },
            Draw { buffer: 10, range: 0..if stand { solid_n } else { 0 }, layer: 3, texture: 0 },
            Draw { buffer: 11, range: 0..if stand { route_n } else { 0 }, layer: 1, texture: 0 },
            // (flat from above the route needs no depth: from far it fought with itself)
            Draw { buffer: 4, range: 0..route_n, layer: if stand { 4 } else { 1 }, texture: 0 },
            Draw { buffer: 18, range: 0..ln_n, layer: if stand { 4 } else { 1 }, texture: 0 },
            Draw { buffer: 5, range: n_bg..n_bg + n_dots, layer: 1, texture: 0 },
            Draw { buffer: 17, range: 0..icon_n, layer: 0, texture: icon_tex },
            Draw { buffer: 5, range: n_ui..all.len() as u32, layer: 0, texture: 0 },
        ];
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("city map") });
        gpu.render(device, queue, &mut enc, &view, (tw, th), Some(wgpu::Color::TRANSPARENT), &layers, &draws);
        queue.submit([enc.finish()]);
        scene.overlays.push((tex, [x0, y0, x0 + w, y0 + h]));
    }

    /// A scale bar, bottom left (of the middle of the view), on a card.
    fn draw_scale_bar(&mut self, ui: &mut Painter, h: f32, pad: f32, s: f32, pal: &Palette) {
        let nice = [5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0];
        let metres = nice.iter().copied().find(|m| m / self.city.mpp > 60.0 * s as f64).unwrap_or(5000.0);
        let len = (metres / self.city.mpp) as f32;
        let units = omsi_ui::units::Units::now();
        let label = units.distance(metres);
        let lw = self.fonts.width(&label, 11.5 * s, Weight::Medium);
        let card = Rect::new(pad, h - pad - 26.0 * s, len + lw + 30.0 * s, 26.0 * s);
        ui.rounded(card, 13.0 * s, pal.card);
        let by = card.center().y;
        ui.rect(Rect::new(card.x + 10.0 * s, by - 1.0 * s, len, 2.0 * s), pal.card_text);
        ui.rect(Rect::new(card.x + 10.0 * s, by - 4.0 * s, 1.5 * s, 8.0 * s), pal.card_text);
        ui.rect(Rect::new(card.x + 10.0 * s + len - 1.5 * s, by - 4.0 * s, 1.5 * s, 8.0 * s), pal.card_text);
        ui.text(&mut self.atlas, &self.fonts, &label, 11.5 * s, Weight::Medium, Vec2::new(card.x + 18.0 * s + len, by + 4.0 * s), Align::Left, pal.card_text);
    }
}
