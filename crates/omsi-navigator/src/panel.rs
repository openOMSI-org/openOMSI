//! The small navigator: the map filling a rounded panel, tilted behind the bus, with the
//! houses standing in 3D (or flat, the `nav_3d` setting), the route over the roads, the traffic,
//! and cards floating over it after Apple Maps' driving view - the next turn top left, the time
//! and temperatures top right, the speed and its limit bottom left, the next stop at the bottom.
//! Under a plugin's panel that fills the frame (`frame_panel`) the card alone, in the panel's
//! colour where it gives one, and the map where that panel says, with its own corners.

use super::mapmesh::GroundLook;
use super::style::{self, Palette};
use super::*;
use omsi_ui::Vertex;

const PANEL_REACH: f64 = 640.0;
const PANEL_REBUILD: f64 = 150.0;
/// The triangles of house models the panel's ground may hold: it is drawn every frame, and rebuilt
/// as the bus drives.
const PANEL_MODELS: usize = 30_000;

/// The card behind the next turn (dark in both looks, as a car's guidance banner is).
const BANNER: Color = Color::rgba(28, 30, 33, 0.95);
const BANNER_TEXT: Color = Color::rgba(255, 255, 255, 1.0);
const BANNER_DIM: Color = Color::rgba(205, 208, 214, 1.0);

/// The vertices with their depth turned on (drawn in the depth-tested layer, so that houses
/// hide them), and a faint copy of them drawn through the houses.
/// (Each part a few millimetres over the one drawn before it - the route's edge, its colour, its
/// chevrons - so that they never fight over the same depth.)
pub(super) fn solid_and_ghost(v: &[Vertex], ghost: f32) -> (Vec<Vertex>, Vec<Vertex>) {
    let n = v.len().max(1) as f32;
    let solid = v.iter().enumerate().map(|(i, x)| Vertex { mode: [2.0, x.mode[1]], pos: [x.pos[0], x.pos[1], x.pos[2] + 0.04 + 0.3 * (i / 3) as f32 / n], ..*x }).collect();
    let faint = v.iter().map(|x| Vertex { color: [x.color[0], x.color[1], x.color[2], x.color[3] * ghost], ..*x }).collect();
    (solid, faint)
}

impl Navigator {
    pub(super) fn draw(&mut self, renderer: &Renderer, target: &wgpu::TextureView, size: (u32, u32), map_h: f32, f: &NavFrame) {
        let (pw, ph) = (size.0 as f32, size.1 as f32);
        let s = pw / 360.0;
        let wd = &WORDS;
        let pal = *style::palette(self.light);
        self.atlas.begin_frame();
        let panel = Rect::new(0.0, 0.0, pw, ph);
        let radius = (14.0 * s).max(10.0);
        // (under a plugin's panel in the whole frame: its map where it says, rounded as a card
        // of its own, or no map at all)
        let (map, cards, map_radius) = match self.framed {
            Some(Some((r, cards))) => (r, cards, (10.0 * s).min(radius)),
            Some(None) => return self.draw_card_alone(renderer, target, size, &pal, radius),
            None => (Rect::new(0.0, 0.0, pw, map_h), true, radius),
        };
        let framed = self.framed.is_some();
        let vp = [map.x, map.y, map.w, map.h];

        // --- the ground (buffer 0): built again when the bus went far, tiles brought lanes,
        // the map's layers came or the look changed
        let own = self.own_net.clone();
        let global = self.global.clone();
        let net = global.as_deref().or(f.traffic.map(|t| &t.net)).or(own.as_deref());
        let lanes_now = net.map(|n| n.lanes.len()).unwrap_or(0);
        let look_key = self.look_key();
        let rebuild = match &self.roads {
            None => lanes_now > 0 || self.map_data.is_some(),
            Some(r) => r.look != look_key || (r.anchor - f.bus.truncate()).length() > PANEL_REBUILD || (r.lanes_seen != lanes_now && self.time - r.built_at > 1.5),
        };
        // (on a worker when the map's network is there; the picture keeps the old part meanwhile)
        let mut built = None;
        if let Some(job) = self.panel_job.as_ref() {
            match job.poll() {
                Ok(Some(mut b)) => {
                    let (look, lanes) = job.meta;
                    let [_, houses @ ..] = b.counts;
                    self.roads = Some(Roads { look, anchor: b.anchor, lanes_seen: lanes, houses, runs: std::mem::take(&mut b.runs), built_at: self.time });
                    built = Some(b);
                    self.panel_job = None;
                }
                Ok(None) => {}
                Err(()) => self.panel_job = None,
            }
        }
        if rebuild && self.panel_job.is_none() && built.is_none() {
            let anchor = f.bus.truncate();
            let parts = self.ground_parts(net, None);
            let look = GroundLook { surfaces: true, trees: true, dashes: true, tol: 0.0 };
            let reach = PANEL_REACH;
            if self.global.is_some() {
                let pal2 = pal;
                let device = renderer.device.clone();
                self.panel_job = Some(super::build::Job::start((look_key, lanes_now), move || super::build::build(&parts, anchor, reach, &pal2, look, true, Some(PANEL_MODELS)).made_on(&device)));
            } else {
                let mut b = super::build::build(&parts, anchor, reach, &pal, look, true, Some(PANEL_MODELS));
                let [_, houses @ ..] = b.counts;
                self.roads = Some(Roads { look: look_key, anchor, lanes_seen: lanes_now, houses, runs: std::mem::take(&mut b.runs), built_at: self.time });
                built = Some(b);
            }
        }

        // --- the camera (world relative to the anchor), behind and above the bus
        let anchor = self.roads.as_ref().map(|r| r.anchor).unwrap_or(f.bus.truncate());
        let rel = |p: DVec3| Vec3::new((p.x - anchor.x) as f32, (p.y - anchor.y) as f32, 0.0);
        let hd = self.cam_heading.to_radians();
        let fwd = DVec2::new(hd.sin(), hd.cos());
        let look_at = f.bus.truncate() - anchor + fwd * self.zoom * 0.17;
        let pitch = (if self.show_3d { PITCH } else { PITCH + 8.0 }).to_radians();
        let back = fwd * self.zoom * pitch.cos();
        let eye = DVec3::new(look_at.x - back.x, look_at.y - back.y, self.zoom * pitch.sin());
        let view = Mat4::look_at_rh(eye.as_vec3(), Vec3::new(look_at.x as f32, look_at.y as f32, 0.0), Vec3::Z);
        let clip_panel = [0.0, 0.0, pw, ph];
        let map_layer = Layer::world(view, FOV.to_radians(), vp, [map.x, map.y, map.right(), map.bottom()], map_radius, 1.0);
        let depth_layer = map_layer.with_depth();
        let vpm = map_layer.view_proj;

        // --- the route (buffers 1 and 9): built again when it, its traffic or the bus's lane changed
        let mut route_verts = None;
        let route_net = global.as_deref().or(f.traffic.map(|t| &t.net));
        if let (Some(rn), true) = (route_net, !self.route.lanes.is_empty()) {
            let first = self.route.lanes.get(self.route.progress.min(self.route.lanes.len().saturating_sub(1))).copied();
            let bus_lane = first.map(|l| lane_from_right(rn, l, f.bus)).unwrap_or(0);
            if self.route_mesh.0 != self.route.version || self.route_mesh.1 != self.jam_version || self.route_mesh.2 != anchor || self.route_mesh.4 != bus_lane || self.route_mesh.5 != look_key {
                let mut p = Painter::new();
                let style = RouteStyle { extra_m: -0.6, min_px: 5.0, arrows: Some((32.0, 900.0)), max_len: 12_000.0, near: Some((anchor, PANEL_REACH)) };
                mapmesh::roads::build_route_to(&mut p, rn, &self.route.lanes[self.route.progress.min(self.route.lanes.len())..], anchor, self.route.s, &self.route_jam, &style, bus_lane, &pal, self.destination.as_ref().and_then(|d| d.path.as_ref().map(|p| p.end_s)));
                self.draw_waypoint_routes(&mut p, rn, anchor, &style, &pal);
                self.route_mesh = (self.route.version, self.jam_version, anchor, p.len(), bus_lane, look_key);
                route_verts = Some(p.verts);
            }
        } else if self.route_mesh.3 != 0 {
            route_verts = Some(Vec::new());
            self.route_mesh = (self.route.version, self.jam_version, anchor, 0, 0, look_key);
        }

        // --- the land colour under everything, the traffic, the vehicles
        let mut bg = Painter::new();
        if framed {
            bg.rounded(panel, radius, self.frame_card.unwrap_or(pal.card));
            bg.rounded(map, map_radius, pal.land);
        } else {
            bg.rounded(panel, radius, pal.land);
        }
        let n_bg = bg.len();
        let mut dy = Painter::new();
        // other roads where the traffic is slow or stands: a band in its colour beside the
        // road's middle (as a map service draws it)
        if let Some(net) = f.traffic.map(|t| &t.net) {
            for (&lane, &c) in &self.congestion {
                let lv = level(c);
                if lv < 2 {
                    continue;
                }
                let Some(l) = net.lanes.get(lane) else { continue };
                if (l.start() - f.bus).truncate().length() > 900.0 || l.kind != LaneKind::Street {
                    continue;
                }
                let pts: Vec<Vec3> = l.points.iter().map(|p| rel(*p)).collect();
                dy.ribbon(&pts, (l.width * 0.45).max(1.4), 2.4, style::TRAFFIC[(lv - 1).min(3)], true);
            }
        }
        // the public transport as drops with their lines (over the map's marks, on the screen);
        // the map's own timetabled trains are left out
        let mut transit: Vec<(DVec3, f64, Option<String>)> = Vec::new();
        if let Some(t) = f.traffic.filter(|_| self.show_ai) {
            for c in &t.cars {
                if c.gone || c.is_rail() || in_the_air(c, &t.net) || (c.vehicle.position - f.bus).truncate().length() > self.zoom * 3.5 + 150.0 {
                    continue;
                }
                let (color, line) = traffic_kind(c);
                let at = self.smoothed_position(c);
                if color == DOT {
                    car_dot(&mut dy, rel(at), self.light);
                } else {
                    transit.push((at, c.vehicle.heading, line));
                }
            }
        }
        // (drawn over the route and the houses in their order, with no depth: a vehicle's edge,
        // body and windscreen lie one on the other, and at a depth of their own they fought
        // over it from far)
        let dy_solid = dy.verts;

        let mut ui = Painter::new();
        // the far end of the map: a haze into the land's colour
        ui.gradient(Rect::new(map.x, map.y, map.w, map.h * 0.16), pal.land.alpha(0.7), pal.land.alpha(0.0));
        self.draw_map_marks(&mut ui, f, map, vpm, vp, anchor, s, transit, &pal);
        self.draw_destination_markers(&mut ui, &|q| project(vpm, vp, Vec3::new((q.x - anchor.x) as f32, (q.y - anchor.y) as f32, 0.0)), map, s, false, &pal);
        // the package's places: its icons on the map, the municipality and the street under it
        let icon_verts = self.minimap_icons(vpm, vp, anchor, f.bus.truncate(), map, s);
        // (the place row, the cards and the stop list: the plugin's panel has the rest of the
        // frame, and says whether the cards go over its map)
        if !framed {
            let loc_h = self.place_row_height(s);
            self.draw_place_row(&mut ui, Rect::new(0.0, map.bottom(), pw, loc_h), 10.0 * s, s);
        }
        if cards {
            self.draw_cards(&mut ui, f, wd, net, map, s, &pal);
        }
        if self.schedule && !framed {
            let loc_h = self.place_row_height(s);
            let r = Rect::new(0.0, map.bottom() + loc_h, pw, ph - map.bottom() - loc_h);
            match self.board {
                // (a plugin's board: its panel is drawn over the card here)
                Some(_) => ui.rect(r, pal.card.alpha(1.0)),
                None => self.draw_schedule(&mut ui, f, r, s, &pal),
            }
        }
        self.draw_grip(&mut ui, pw, ph, s, &pal);

        // --- to the GPU
        let (Some(gpu), device, queue) = (self.gpu.as_mut(), &renderer.device, &renderer.queue) else { return };
        if let Some(b) = built {
            b.put(gpu, device, queue, [0, 12, 8, 13]);
        }
        if let Some(v) = route_verts {
            let (solid, ghost) = solid_and_ghost(&v, 0.3);
            gpu.upload(device, queue, 1, &solid);
            gpu.upload(device, queue, 9, &ghost);
        }
        let mut all = bg.verts;
        let n_dy = dy_solid.len() as u32;
        all.extend(dy_solid);
        let n_ui_start = all.len() as u32;
        all.extend(ui.verts);
        gpu.upload(device, queue, 2, &all);
        let (icon_n, icon_tex) = self.place.upload(gpu, device, queue, 16, &icon_verts);
        gpu.upload_atlas(queue, &mut self.atlas);
        let flat = Layer::flat(clip_panel, radius, 1.0);
        let backdrop = Layer::flat(clip_panel, radius, if self.cockpit_display { 1.0 } else { omsi_ui::theme::backdrop(self.opacity).min(1.0) });
        let mut layers = [flat, map_layer, map_layer.with_shadow(), depth_layer, depth_layer, backdrop];
        // The cockpit setting fades the map and its labels too; opaque map geometry
        // would otherwise cover the background whose opacity was changed.
        let opacity = self.shown * if self.cockpit_display { self.opacity } else { 1.0 };
        for l in layers.iter_mut() {
            l.opacity *= opacity;
        }
        let solid_n = self.roads.as_ref().map_or(0, |r| r.houses[1] as u32);
        let three = self.show_3d;
        let route_n = self.route_mesh.3;
        let mpp = map_layer.px_scale * self.zoom as f32 * 4.0;
        let seen = |part: usize, on: bool| match (&self.roads, on) {
            (Some(r), true) => super::cull::visible(&r.runs[part], vpm, mpp),
            _ => Vec::new(),
        };
        let mut draws = vec![Draw { buffer: 2, range: 0..n_bg, layer: 5, texture: 0 }];
        for (buffer, part, on, layer) in [(0, 0, true, 1), (13, 3, !three, 1), (12, 1, three, 2), (8, 2, three, 3)] {
            draws.extend(seen(part, on).into_iter().map(|range| Draw { buffer, range, layer, texture: 0 }));
        }
        draws.extend([
            Draw { buffer: 9, range: 0..if three && solid_n > 0 { route_n } else { 0 }, layer: 1, texture: 0 },
            Draw { buffer: 1, range: 0..route_n, layer: 4, texture: 0 },
            Draw { buffer: 2, range: n_bg..n_bg + n_dy, layer: 1, texture: 0 },
            Draw { buffer: 16, range: 0..icon_n, layer: 0, texture: icon_tex },
            Draw { buffer: 2, range: n_ui_start..all.len() as u32, layer: 0, texture: 0 },
        ]);
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("navigator") });
        gpu.render(device, queue, &mut enc, target, size, Some(wgpu::Color::TRANSPARENT), &layers, &draws);
        queue.submit([enc.finish()]);
    }

    /// The corner to pull for another size, on a navigator that is a widget.
    fn draw_grip(&self, ui: &mut Painter, pw: f32, ph: f32, s: f32, pal: &Palette) {
        if !self.widget {
            return;
        }
        let c = Vec2::new(pw - 7.0 * s, ph - 7.0 * s);
        for k in 0..3 {
            let o = (k as f32 * 4.0 + 3.0) * s;
            ui.line(c + Vec2::new(-o, 0.0), c + Vec2::new(0.0, -o), 1.4 * s, pal.card_dim.alpha(0.7));
        }
    }

    /// The card alone, under a plugin's panel that fills the frame and has no map: nothing of
    /// the map is built or drawn.
    fn draw_card_alone(&mut self, renderer: &Renderer, target: &wgpu::TextureView, size: (u32, u32), pal: &Palette, radius: f32) {
        let (pw, ph) = (size.0 as f32, size.1 as f32);
        let s = pw / 360.0;
        let mut bg = Painter::new();
        bg.rounded(Rect::new(0.0, 0.0, pw, ph), radius, self.frame_card.unwrap_or(pal.card));
        let n_bg = bg.len();
        let mut ui = Painter::new();
        self.draw_grip(&mut ui, pw, ph, s, pal);
        let (Some(gpu), device, queue) = (self.gpu.as_mut(), &renderer.device, &renderer.queue) else { return };
        let mut all = bg.verts;
        all.extend(ui.verts);
        gpu.upload(device, queue, 2, &all);
        let clip = [0.0, 0.0, pw, ph];
        let opacity = self.shown * if self.cockpit_display { self.opacity } else { 1.0 };
        let mut layers = [Layer::flat(clip, radius, 1.0), Layer::flat(clip, radius, omsi_ui::theme::backdrop(self.opacity).min(1.0))];
        for l in layers.iter_mut() {
            l.opacity *= opacity;
        }
        let draws = [Draw { buffer: 2, range: 0..n_bg, layer: 1, texture: 0 }, Draw { buffer: 2, range: n_bg..all.len() as u32, layer: 0, texture: 0 }];
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("navigator") });
        gpu.render(device, queue, &mut enc, target, size, Some(wgpu::Color::TRANSPARENT), &layers, &draws);
        queue.submit([enc.finish()]);
    }

    /// The marks over the map: the stops ahead, the other players, the public transport's line
    /// tags and the bus itself.
    #[allow(clippy::too_many_arguments)]
    fn draw_map_marks(&mut self, ui: &mut Painter, f: &NavFrame, map: Rect, vpm: Mat4, vp: [f32; 4], anchor: DVec2, s: f32, mut transit: Vec<(DVec3, f64, Option<String>)>, pal: &Palette) {
        let rel = |p: DVec3| Vec3::new((p.x - anchor.x) as f32, (p.y - anchor.y) as f32, 0.0);
        let n_stops = f.stops.len();
        let markers = spaced_markers(f.stops.iter().enumerate().filter_map(|(k, st)| project(vpm, vp, rel(st.position)).filter(|p| map.contains(*p)).map(|p| (k, p))), 20.0 * s);
        for (k, sp) in markers.into_iter().rev() {
            stop_pin(ui, &mut self.atlas, sp, k == 0, k + 1 == n_stops, s);
        }
        transit.sort_by(|a, b| (a.0 - f.bus).length().total_cmp(&(b.0 - f.bus).length()));
        let mut taken: Vec<Rect> = Vec::new();
        for pl in &f.players {
            let Some(p) = project(vpm, vp, rel(pl.position)).filter(|p| map.contains(*p)) else { continue };
            let a = (angle_diff(self.cam_heading, pl.heading) as f32).to_radians();
            puck(ui, p, a, 7.5 * s, PLAYER);
            let px = 9.5 * s;
            let name = self.fonts.fit(&pl.name, px, Weight::Bold, 90.0 * s);
            let w = self.fonts.width(&name, px, Weight::Bold) + 10.0 * s;
            let r = Rect::new(p.x - w * 0.5, p.y - 25.0 * s, w, 14.0 * s);
            if taken.iter().any(|o| rects_overlap(o, &r)) {
                continue;
            }
            taken.push(r);
            ui.rounded(r, 7.0 * s, PLAYER);
            ui.text_in(&mut self.atlas, &self.fonts, &name, px, Weight::Bold, r, Align::Center, Color::WHITE);
        }
        for (pos, heading, line) in &transit {
            let Some(p) = project(vpm, vp, rel(*pos)).filter(|p| map.contains(*p)) else { continue };
            let h = heading.to_radians();
            let Some(ahead) = project(vpm, vp, rel(*pos + DVec3::new(h.sin(), h.cos(), 0.0) * 5.0)) else { continue };
            transit_drop(ui, &mut self.atlas, &self.fonts, p, ahead - p, line.as_deref(), s, &mut taken, pal);
        }
        if let Some(bp) = project(vpm, vp, rel(f.bus)) {
            let a = (angle_diff(self.cam_heading, f.heading) as f32).to_radians();
            puck(ui, bp, a, 10.0 * s, style::LIGHT.route);
        }
    }

    /// The cards over the map.
    #[allow(clippy::too_many_arguments)]
    fn draw_cards(&mut self, ui: &mut Painter, f: &NavFrame, wd: &Words, net: Option<&Network>, map: Rect, s: f32, pal: &Palette) {
        let pad = 8.0 * s;
        let units = omsi_ui::units::Units::now();
        // --- top left: the next turn, or the street the bus is on
        let mut left_w = 0.0;
        if let Some((dir, angle, dist, street)) = self.next_turn.clone() {
            let icon = match dir {
                2 => "u_turn_left",
                -1 if angle < 60.0 => "turn_slight_left",
                1 if angle < 60.0 => "turn_slight_right",
                -1 => "turn_left",
                _ => "turn_right",
            };
            let t = units.distance(dist);
            let max_w = map.w * 0.64;
            let tw = self.fonts.width(&t, 19.0 * s, Weight::Bold);
            let street = street.as_deref().map(|n| self.fonts.fit(n, 12.0 * s, Weight::Medium, max_w - 58.0 * s));
            let sw_ = street.as_deref().map(|n| self.fonts.width(n, 12.0 * s, Weight::Medium)).unwrap_or(0.0);
            let w = (52.0 * s + tw.max(sw_) + 14.0 * s).min(max_w);
            let h = if street.is_some() { 54.0 } else { 42.0 } * s;
            let b = Rect::new(map.x + pad, map.y + pad, w, h);
            ui.shadow(b, 12.0 * s, 8.0 * s, Color::rgba(0, 0, 0, 0.18));
            ui.rounded(b, 12.0 * s, BANNER);
            ui.icon(&mut self.atlas, icon, Vec2::new(b.x + 26.0 * s, b.center().y), 32.0 * s, BANNER_TEXT);
            let tx = b.x + 50.0 * s;
            let top = if street.is_some() { b.y + 6.0 * s } else { b.y };
            ui.text_in(&mut self.atlas, &self.fonts, &t, 19.0 * s, Weight::Bold, Rect::new(tx, top, w - 54.0 * s, if street.is_some() { 26.0 * s } else { b.h }), Align::Left, BANNER_TEXT);
            if let Some(n) = street.as_deref() {
                ui.text_in(&mut self.atlas, &self.fonts, n, 12.0 * s, Weight::Medium, Rect::new(tx, b.y + 30.0 * s, w - 54.0 * s, 18.0 * s), Align::Left, BANNER_DIM);
            }
            left_w = w;
        } else if let Some(n) = self.street_here.clone() {
            let px = 12.5 * s;
            let n = self.fonts.fit(&n, px, Weight::Bold, map.w * 0.55);
            let w = self.fonts.width(&n, px, Weight::Bold) + 34.0 * s;
            let b = Rect::new(map.x + pad, map.y + pad, w, 28.0 * s);
            ui.shadow(b, 14.0 * s, 6.0 * s, Color::rgba(0, 0, 0, 0.14));
            ui.rounded(b, 14.0 * s, BANNER);
            ui.icon(&mut self.atlas, "straight", Vec2::new(b.x + 14.0 * s, b.center().y), 16.0 * s, BANNER_TEXT);
            ui.text_in(&mut self.atlas, &self.fonts, &n, px, Weight::Bold, Rect::new(b.x + 26.0 * s, b.y, w - 28.0 * s, b.h), Align::Left, BANNER_TEXT);
            left_w = w;
        }
        // --- top right: the time, the day and the temperatures (and the line)
        let hh = (f.time / 3600.0) as i32 % 24;
        let mm = ((f.time % 3600.0) / 60.0) as i32;
        let time_text = format!("{hh:02}:{mm:02}");
        let day = wd.days[f.weekday.clamp(0, 6) as usize];
        let temp = format!("{:.0}° · {:.0}°", f.outside_temp, f.inside_temp);
        let tw = self.fonts.width(&time_text, 14.0 * s, Weight::Bold) + 5.0 * s + self.fonts.width(day, 11.0 * s, Weight::Medium);
        let w = tw.max(self.fonts.width(&temp, 11.0 * s, Weight::Medium)) + 20.0 * s;
        if map.w - left_w - 3.0 * pad > w {
            let b = Rect::new(map.right() - pad - w, map.y + pad, w, 40.0 * s);
            ui.shadow(b, 12.0 * s, 6.0 * s, Color::rgba(0, 0, 0, 0.12));
            ui.rounded(b, 12.0 * s, pal.card);
            let base = b.y + 17.0 * s;
            ui.text(&mut self.atlas, &self.fonts, &time_text, 14.0 * s, Weight::Bold, Vec2::new(b.right() - 10.0 * s, base), Align::Right, pal.card_text);
            ui.text(&mut self.atlas, &self.fonts, day, 11.0 * s, Weight::Medium, Vec2::new(b.right() - 10.0 * s - self.fonts.width(&time_text, 14.0 * s, Weight::Bold) - 5.0 * s, base), Align::Right, pal.card_dim);
            ui.text(&mut self.atlas, &self.fonts, &temp, 11.0 * s, Weight::Medium, Vec2::new(b.right() - 10.0 * s, b.y + 33.0 * s), Align::Right, pal.card_dim);
        }
        if self.destination.is_some() {
            self.draw_destination_card(ui, net, map, s, pal);
            return;
        }
        // --- bottom: the next stop
        let bottom_h = 52.0 * s;
        let card = Rect::new(map.x + pad, map.bottom() - pad - bottom_h, map.w - 2.0 * pad, bottom_h);
        ui.shadow(card, 14.0 * s, 8.0 * s, Color::rgba(0, 0, 0, 0.16));
        ui.rounded(card, 14.0 * s, pal.card);
        let inner = card.pad(12.0 * s, 0.0);
        let row1 = Rect::new(inner.x, card.y + 7.0 * s, inner.w, 20.0 * s);
        let row2 = Rect::new(inner.x, card.y + 28.0 * s, inner.w, 18.0 * s);
        let row1 = stop_request_icon(ui, &mut self.atlas, f.stop_requested, row1, s * 0.8);
        let note = if self.route.note > 0.0 {
            Some((wd.recalculated.to_string(), ON_TIME))
        } else if self.route.joined && !self.route.lanes.is_empty() && !self.route.on_route && self.route.off_for > OFF_ROUTE_AFTER {
            Some(((if self.route.off_for < OFF_ROUTE_AFTER + 20.0 { wd.rerouting } else { wd.off_route }).to_string(), WARN))
        } else {
            None
        };
        let jam_note = (self.jam_cost >= 30.0).then(|| {
            let n = (self.jam_cost / 60.0).max(1.0).round();
            (if self.jam_cost >= 60.0 { format!("Traffic jam +{n:.0} min") } else { format!("Slow traffic +{n:.0} min") }, if self.jam_cost >= 60.0 { style::TRAFFIC[2] } else { style::TRAFFIC[1] })
        });
        match f.stops.first() {
            Some(st) => {
                let name = if f.stops.len() == 1 { format!("{} · {}", st.name.trim(), omsi_ui::tr(wd.last_stop)) } else { st.name.trim().to_string() };
                let line_w = match f.line.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
                    Some(l) => {
                        let px = 11.0 * s;
                        let l = self.fonts.fit(l, px, Weight::Bold, 48.0 * s);
                        let w = self.fonts.width(&l, px, Weight::Bold) + 10.0 * s;
                        let r = Rect::new(row1.x, row1.y + 2.0 * s, w, 16.0 * s);
                        ui.rounded(r, 4.0 * s, BUS);
                        ui.text_in(&mut self.atlas, &self.fonts, &l, px, Weight::Bold, r, Align::Center, Color::WHITE);
                        w + 7.0 * s
                    }
                    None => 0.0,
                };
                let name = self.fonts.fit(&name, 14.5 * s, Weight::Bold, row1.w - line_w);
                let name_r = Rect::new(row1.x + line_w, row1.y, row1.w - line_w, row1.h);
                let w = ui.text_in(&mut self.atlas, &self.fonts, &name, 14.5 * s, Weight::Bold, name_r, Align::Left, if st.closed.is_some() { pal.card_dim } else { pal.card_text });
                if st.closed.is_some() {
                    strike(ui, name_r.x, w, name_r, s, pal.card_dim);
                }
                let mut parts = Vec::new();
                if let Some((_, d)) = self.next_dist {
                    parts.push(units.distance(d));
                    let secs = d / (self.speed_avg.max(5.0) as f64) + if self.jam_cost > 0.0 { self.jam_cost as f64 } else { 0.0 };
                    parts.push(if secs < 60.0 { omsi_ui::tr("<1 min").into_owned() } else { format!("{:.0} {}", (secs / 60.0).round(), omsi_ui::tr("min")) });
                }
                parts.push(format!("{:02}:{:02}", (st.arrival / 3600.0) as i32 % 24, ((st.arrival % 3600.0) / 60.0) as i32));
                let line2 = parts.join(" · ");
                // (closed: the way round in place of the distance)
                let closed = st.closed.as_deref().map(|via| if via.is_empty() { wd.closed.to_string() } else { format!("{} · {via}", omsi_ui::tr(wd.closed)) });
                let (text, col) = match (&closed, &note, &jam_note) {
                    (Some(t), _, _) => (t.clone(), WARN),
                    (None, Some((t, c)), _) => (t.clone(), *c),
                    (None, None, Some((t, c))) => (format!("{line2} · {}", omsi_ui::tr(t)), *c),
                    _ => (line2, pal.card_dim),
                };
                let mut right = 0.0;
                if let Some(d) = f.delay {
                    let (txt, c) = if d > 59.0 {
                        (format!("+{}:{:02}", (d / 60.0) as i32, (d % 60.0) as i32), style::TRAFFIC[2])
                    } else if d < -59.0 {
                        (format!("-{}:{:02}", (-d / 60.0) as i32, (-d % 60.0) as i32), style::TRAFFIC[0])
                    } else {
                        (wd.on_time.to_string(), Color::hex(0x2fa84f))
                    };
                    let px = 11.5 * s;
                    let w = self.fonts.width(&txt, px, Weight::Bold) + 12.0 * s;
                    let r = Rect::new(row2.right() - w, row2.y, w, 17.0 * s);
                    ui.rounded(r, 8.5 * s, c.alpha(0.16));
                    ui.text_in(&mut self.atlas, &self.fonts, &txt, px, Weight::Bold, r, Align::Center, c);
                    right = w + 6.0 * s;
                }
                let text = self.fonts.fit(&text, 12.0 * s, Weight::Medium, row2.w - right);
                ui.text_in(&mut self.atlas, &self.fonts, &text, 12.0 * s, Weight::Medium, Rect::new(row2.x, row2.y, row2.w - right, row2.h), Align::Left, col);
            }
            None => {
                let t = f.terminus.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| wd.no_duty.to_string());
                ui.text_in(&mut self.atlas, &self.fonts, &t, 14.5 * s, Weight::Bold, row1, Align::Left, pal.card_text);
                let here = self.street_here.clone().unwrap_or_default();
                ui.text_in(&mut self.atlas, &self.fonts, &here, 12.0 * s, Weight::Medium, row2, Align::Left, pal.card_dim);
            }
        }
        // --- the speed and its limit, bottom left over the card
        let (speed, unit) = units.speed(f.speed_kmh.abs());
        let limit = net.and_then(|n| {
            let lane = if self.route.on_route { self.route.lanes.get(self.route.progress).copied() } else { None };
            let lane = lane.or_else(|| n.nearest_lane_near(f.bus, LaneKind::Street).filter(|l| l.2 < 8.0).map(|l| l.0))?;
            let v = n.lanes.get(lane)?.speed_limit_kmh;
            (v > 1.0 && v < 200.0).then_some(v)
        });
        self.speed_limit = limit;
        let r = 20.0 * s;
        let c = Vec2::new(card.x + r + 2.0 * s, card.y - r - 7.0 * s);
        let over = limit.is_some_and(|l| f.speed_kmh.abs() > l + 3.0);
        ui.circle(c + Vec2::new(0.0, 1.5 * s), r + 1.0 * s, Color::rgba(0, 0, 0, 0.14));
        ui.circle(c, r, pal.card);
        let sp = format!("{speed:.0}");
        ui.text(&mut self.atlas, &self.fonts, &sp, 17.0 * s, Weight::Bold, Vec2::new(c.x, c.y + 3.0 * s), Align::Center, if over { style::TRAFFIC[2] } else { pal.card_text });
        ui.text(&mut self.atlas, &self.fonts, unit, 7.5 * s, Weight::Medium, Vec2::new(c.x, c.y + 12.0 * s), Align::Center, pal.card_dim);
        if let Some(v) = limit {
            let lc = Vec2::new(c.x + r * 2.0 + 2.0 * s, c.y);
            let lr = 15.0 * s;
            ui.circle(lc, lr, Color::rgba(214, 38, 38, 1.0));
            ui.circle(lc, lr * 0.78, Color::WHITE);
            let t = format!("{:.0}", units.speed_limit(v));
            let px = if t.len() > 2 { 9.5 } else { 12.0 } * s;
            ui.text(&mut self.atlas, &self.fonts, &t, px, Weight::Black, Vec2::new(lc.x, lc.y + self.fonts.cap_height(px, Weight::Black) * 0.5), Align::Center, Color::rgba(15, 15, 15, 1.0));
        }
    }

    /// The next stops with their planned and expected times, under the map.
    fn draw_schedule(&mut self, ui: &mut Painter, f: &NavFrame, r: Rect, s: f32, pal: &Palette) {
        ui.rect(r, pal.card.alpha(1.0));
        let pad = 12.0 * s;
        let mut y = r.y + 6.0 * s;
        let late = f.delay.unwrap_or(0.0);
        for (k, st) in f.stops.iter().take(5).enumerate() {
            let row = Rect::new(r.x + pad, y, r.w - 2.0 * pad, 22.0 * s);
            // a line of dots down the left, as a route's stops
            let dot = Vec2::new(row.x + 5.0 * s, row.center().y);
            if k + 1 < f.stops.len().min(5) {
                ui.rect(Rect::new(dot.x - 1.0 * s, dot.y, 2.0 * s, 22.0 * s), style::TRAFFIC[0].alpha(0.35));
            }
            ui.circle(dot, 4.5 * s, style::TRAFFIC[0]);
            ui.circle(dot, 2.3 * s, pal.card);
            let planned = format!("{:02}:{:02}", (st.arrival / 3600.0) as i32 % 24, ((st.arrival % 3600.0) / 60.0) as i32);
            ui.text_in(&mut self.atlas, &self.fonts, &planned, 12.0 * s, Weight::Bold, Rect::new(row.x + 16.0 * s, row.y, 40.0 * s, row.h), Align::Left, pal.card_dim);
            let name = self.fonts.fit(st.name.trim(), 13.0 * s, Weight::Medium, row.w - 110.0 * s);
            let name_r = Rect::new(row.x + 58.0 * s, row.y, row.w - 110.0 * s, row.h);
            let w = ui.text_in(&mut self.atlas, &self.fonts, &name, 13.0 * s, Weight::Medium, name_r, Align::Left, if st.closed.is_some() { pal.card_dim } else { pal.card_text });
            // (closed today: struck through, and the bus does not stop there)
            if st.closed.is_some() {
                strike(ui, name_r.x, w, name_r, s, pal.card_dim);
            }
            let exp = st.arrival + late;
            let e = format!("{:02}:{:02}", (exp / 3600.0).rem_euclid(24.0) as i32, ((exp.rem_euclid(3600.0)) / 60.0) as i32);
            ui.text_in(&mut self.atlas, &self.fonts, &e, 12.0 * s, Weight::Medium, row, Align::Right, if late > 59.0 { style::TRAFFIC[2] } else if late < -59.0 { style::TRAFFIC[0] } else { pal.card_dim });
            y += 22.0 * s;
        }
    }

    /// A key that changes with everything the ground's look depends on.
    pub(super) fn look_key(&self) -> u64 {
        (self.light as u64) | (self.show_3d as u64) << 1 | (self.show_surface as u64) << 2 | self.surface_version << 3 | (self.map_version << 20) | (self.roads_version << 36)
    }
}

/// A line through a stop's name `w` wide from `x`, at the middle of `r` (a stop closed today).
fn strike(ui: &mut Painter, x: f32, w: f32, r: Rect, s: f32, c: Color) {
    ui.rect(Rect::new(x, (r.center().y - 0.75 * s).round(), w.max(0.0), (1.5 * s).max(1.0)), c);
}

/// A stop on the map: a white pin with the bus sign, blue for the next, red with a flag for the
/// last.
pub(super) fn stop_pin(ui: &mut Painter, atlas: &mut Atlas, p: Vec2, next: bool, last: bool, s: f32) {
    let r = if next { 9.0 } else { 7.0 } * s;
    let fill = if next { style::LIGHT.route } else if last { Color::hex(0xe5483d) } else { Color::hex(0x5d6b80) };
    ui.circle(p + Vec2::new(0.0, 1.2 * s), r + 1.5 * s, Color::rgba(0, 0, 0, 0.18));
    ui.circle(p, r + 1.5 * s, Color::WHITE);
    ui.circle(p, r, fill);
    ui.icon(atlas, if last { "sports_score" } else { "directions_bus" }, p, r * 1.45, Color::WHITE);
}

/// The bus or a player: a white disc with an arrow in it the way it faces (`angle`: radians,
/// clockwise from up on the screen), a soft shadow under it.
pub(super) fn puck(ui: &mut Painter, at: Vec2, angle: f32, r: f32, fill: Color) {
    ui.circle(at + Vec2::new(0.0, r * 0.12), r * 1.28, Color::rgba(0, 0, 0, 0.2));
    ui.circle(at, r * 1.18, Color::WHITE);
    ui.circle(at, r, fill);
    let rot = |v: Vec2| Vec2::new(v.x * angle.cos() - v.y * angle.sin(), v.x * angle.sin() + v.y * angle.cos());
    let k = r * 0.68;
    let tip = at + rot(Vec2::new(0.0, -1.0) * k);
    let l = at + rot(Vec2::new(-0.72, 0.75) * k);
    let m = at + rot(Vec2::new(0.0, 0.38) * k);
    let rr = at + rot(Vec2::new(0.72, 0.75) * k);
    ui.tri(tip, l, m, Color::WHITE, Color::WHITE, Color::WHITE);
    ui.tri(tip, m, rr, Color::WHITE, Color::WHITE, Color::WHITE);
}

/// A car on the map: a small dot with a pale ring (as the other drivers were always shown).
pub(super) fn car_dot(p: &mut Painter, at: Vec3, light: bool) {
    let (edge, fill) = if light { (Color::rgba(255, 255, 255, 0.95), Color::hex(0x5b6b84)) } else { (Color::rgba(14, 16, 20, 0.9), Color::hex(0x9fb0c8)) };
    p.world_disc(at, 1.9, 4.2, edge);
    p.world_disc(at, 1.35, 3.0, fill);
}

