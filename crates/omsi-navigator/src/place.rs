//! The places a map's package names on the navigator and the city map: its icons, the names of districts and
//! towns, the regions as a watermark, and where the bus is (the municipality and the street, with the street's
//! speed limits).

use super::*;
use omsi_map::package::{Label, Marker, Package};
use omsi_ui::Vertex;

// (the place row's and the chips' own colours: they lie on dark bars in both looks)
const BAR: Color = Color::rgba(18, 18, 18, 0.55);
const TEXT: Color = Color::rgba(237, 237, 237, 1.0);
const TEXT_DIM: Color = Color::rgba(184, 184, 184, 1.0);

/// The marker pictures packed into one: its size and pixels (straight alpha) and, per picture, where it lies
/// (texture coordinates) and its height over its width.
struct Sheet {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
    rects: Vec<([f32; 4], f32)>,
}

/// Shelf packing of picture sizes into one sheet of at most 2048 x 2048 (safe on downlevel GPUs), one transparent
/// pixel round each picture. Returns the sheet size and each picture's corner, or `None` for one that does not fit.
fn pack_icons(sizes: &[(u32, u32)]) -> (u32, u32, Vec<Option<(u32, u32)>>) {
    const SHEET_MAX: u32 = 2048;
    let mut pos = Vec::with_capacity(sizes.len());
    let (mut x0, mut y0, mut row_h, mut w, mut h) = (1u32, 1u32, 0u32, 0u32, 0u32);
    for &(iw, ih) in sizes {
        if x0 + iw + 1 > SHEET_MAX {
            x0 = 1;
            y0 += row_h + 1;
            row_h = 0;
        }
        if x0 + iw + 1 > SHEET_MAX || y0 + ih + 1 > SHEET_MAX {
            log::warn!("map package: the icon sheet is full, an icon is left out");
            pos.push(None);
            continue;
        }
        pos.push(Some((x0, y0)));
        w = w.max(x0 + iw + 1);
        h = h.max(y0 + ih + 1);
        row_h = row_h.max(ih);
        x0 += iw + 1;
    }
    (w, h, pos)
}

/// What the package names, ready to draw, and where the bus is.
#[derive(Default)]
pub(super) struct PlaceView {
    city: Option<String>,
    street: Option<String>,
    /// The street's limit and the heavy vehicles' (km/h, 0 = not said).
    limit: (u32, u32),
    /// The markers and the picture of the sheet each uses.
    markers: Vec<(Marker, usize)>,
    labels: Vec<Label>,
    sheet: Option<Sheet>,
    tex: Option<usize>,
    /// The package says where one is (municipalities or named streets).
    has_location: bool,
}

impl PlaceView {
    /// The icons (decoded and packed into one sheet) and the names of a package.
    pub(super) fn of(pkg: &Package) -> PlaceView {
        let mut files: Vec<String> = Vec::new();
        let mut images: Vec<omsi_texture::Image> = Vec::new();
        let mut used: Vec<(Marker, usize)> = Vec::new();
        for m in pkg.markers() {
            let k = match files.iter().position(|f| f.eq_ignore_ascii_case(&m.icon)) {
                Some(k) => k,
                None => {
                    let img = pkg
                        .picture(&m.icon)
                        .and_then(|b| {
                            omsi_texture::decode_bytes(&b, std::path::Path::new(&m.icon)).ok()
                        })
                        .filter(|i| {
                            i.width > 0 && i.height > 0 && i.width <= 1024 && i.height <= 1024
                        });
                    match img {
                        Some(i) => {
                            files.push(m.icon.clone());
                            images.push(i);
                            images.len() - 1
                        }
                        None => {
                            log::warn!("map package: icon {} could not be read", m.icon);
                            continue;
                        }
                    }
                }
            };
            used.push((m, k));
        }
        let mut labels = pkg.labels();
        labels.sort_by(|a, b| b.rank.cmp(&a.rank));
        let mut view = PlaceView {
            labels,
            has_location: pkg.has_location(),
            ..Default::default()
        };
        if images.is_empty() {
            return view;
        }
        let sizes: Vec<(u32, u32)> = images.iter().map(|i| (i.width, i.height)).collect();
        let (w, h, pos) = pack_icons(&sizes);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        let mut rects = Vec::with_capacity(images.len());
        for (i, at) in images.iter().zip(&pos) {
            let (x0, y0) = at.unwrap_or((0, 0));
            if at.is_some() {
                for row in 0..i.height {
                    let from = (row * i.width * 4) as usize;
                    let to = (((y0 + row) * w + x0) * 4) as usize;
                    rgba[to..to + (i.width * 4) as usize]
                        .copy_from_slice(&i.rgba[from..from + (i.width * 4) as usize]);
                }
            }
            rects.push((
                [
                    x0 as f32 / w as f32,
                    y0 as f32 / h as f32,
                    (x0 + i.width) as f32 / w as f32,
                    (y0 + i.height) as f32 / h as f32,
                ],
                i.height as f32 / i.width as f32,
            ));
        }
        view.markers = used
            .into_iter()
            .filter(|(_, k)| pos.get(*k).is_some_and(Option::is_some))
            .collect();
        view.sheet = Some(Sheet { w, h, rgba, rects });
        log::info!("map package: {} icons on the map", view.markers.len());
        view
    }

    /// A marker's picture centred at `center` (pixels), `scale` times its width: six vertices for a drawing with
    /// the sheet as texture.
    fn push_icon(&self, marker: usize, center: Vec2, scale: f32, out: &mut Vec<Vertex>) {
        let Some((m, k)) = self.markers.get(marker) else {
            return;
        };
        let Some((uv, aspect)) = self.sheet.as_ref().and_then(|s| s.rects.get(*k)) else {
            return;
        };
        let (w, h) = (m.width_px * scale, m.width_px * scale * aspect);
        let (x0, y0, x1, y1) = (
            center.x - w * 0.5,
            center.y - h * 0.5,
            center.x + w * 0.5,
            center.y + h * 0.5,
        );
        let v = |x: f32, y: f32, u: f32, t: f32| Vertex {
            pos: [x, y, 0.0],
            uv: [u, t],
            color: [1.0; 4],
            mode: [0.0, 1.0],
            ..Default::default()
        };
        out.extend([
            v(x0, y0, uv[0], uv[1]),
            v(x1, y0, uv[2], uv[1]),
            v(x1, y1, uv[2], uv[3]),
            v(x0, y0, uv[0], uv[1]),
            v(x1, y1, uv[2], uv[3]),
            v(x0, y1, uv[0], uv[3]),
        ]);
    }

    /// The sheet as a texture of the navigator's own GPU state (once), and what to draw with it.
    pub(super) fn upload(
        &mut self,
        gpu: &mut Gpu,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        buffer: usize,
        verts: &[Vertex],
    ) -> (u32, usize) {
        if self.tex.is_none() {
            if let Some(sheet) = self.sheet.as_ref() {
                self.tex = Some(gpu.add_image(device, queue, sheet.w, sheet.h, &sheet.rgba));
            }
        }
        gpu.upload(device, queue, buffer, verts);
        (verts.len() as u32, self.tex.unwrap_or(0))
    }
}

impl Navigator {
    /// The package, when it is on and says where one is (municipalities or named streets).
    fn locating(&self) -> Option<&Package> {
        self.surface
            .as_deref()
            .filter(|_| self.show_surface && self.place.has_location)
    }

    /// Where the bus is: the municipality and the street (with its limits) from the package.
    pub(super) fn update_place(&mut self, bus: DVec2) {
        let at = (bus.x, bus.y);
        let found = self.locating().map(|sf| {
            (
                sf.locate(at),
                sf.street_info(at, 30.0)
                    .map_or((0, 0), |s| (s.speed, s.heavy)),
            )
        });
        let ((city, street), limit) = found.unwrap_or(((None, None), (0, 0)));
        (self.place.city, self.place.street, self.place.limit) = (city, street, limit);
    }

    /// The row under the next stop of the small map: the municipality and the street, when the package says them.
    pub(super) fn place_row_height(&self, s: f32) -> f32 {
        if self.locating().is_some() {
            22.0 * s
        } else {
            0.0
        }
    }

    pub(super) fn draw_place_row(&mut self, ui: &mut Painter, row: Rect, pad: f32, s: f32) {
        if self.locating().is_none() {
            return;
        }
        ui.rect(row, BAR);
        let text = match (&self.place.city, &self.place.street) {
            (Some(c), Some(st)) => format!("{c}  |  {st}"),
            (Some(c), None) => c.clone(),
            (None, Some(st)) => st.clone(),
            _ => String::new(),
        };
        if !text.is_empty() {
            ui.icon(
                &mut self.atlas,
                "location_on",
                Vec2::new(pad + 7.0 * s, row.center().y),
                15.0 * s,
                TEXT_DIM,
            );
            let text = self.fonts.fit(
                &text,
                12.5 * s,
                Weight::Medium,
                row.w - 2.0 * pad - 24.0 * s,
            );
            ui.text_in(
                &mut self.atlas,
                &self.fonts,
                &text,
                12.5 * s,
                Weight::Medium,
                Rect::new(pad + 22.0 * s, row.y, row.w - 2.0 * pad - 22.0 * s, row.h),
                Align::Left,
                TEXT,
            );
        }
    }

    /// The icons within reach of the small map's view, as vertices for the sheet's texture.
    pub(super) fn minimap_icons(
        &self,
        vpm: Mat4,
        vp: [f32; 4],
        anchor: DVec2,
        bus: DVec2,
        map: Rect,
        s: f32,
    ) -> Vec<Vertex> {
        let mut out = Vec::new();
        if !self.show_surface {
            return out;
        }
        let reach = self.zoom * 3.5 + 300.0;
        for (i, (m, _)) in self.place.markers.iter().enumerate() {
            if (DVec2::new(m.at.0, m.at.1) - bus).length() > reach {
                continue;
            }
            let rel = Vec3::new((m.at.0 - anchor.x) as f32, (m.at.1 - anchor.y) as f32, 0.0);
            if let Some(p) = project(vpm, vp, rel).filter(|p| map.contains(*p)) {
                self.place.push_icon(i, p, s, &mut out);
            }
        }
        out
    }

    /// The icons in the window of the city map.
    pub(super) fn city_icons(
        &self,
        to_screen: &dyn Fn(DVec3) -> Vec2,
        win: Rect,
        s: f32,
    ) -> Vec<Vertex> {
        let mut out = Vec::new();
        if !self.show_surface {
            return out;
        }
        for (i, (m, _)) in self.place.markers.iter().enumerate() {
            let p = to_screen(DVec3::new(m.at.0, m.at.1, 0.0));
            if win.pad(30.0 * s, 30.0 * s).contains(p) {
                self.place.push_icon(i, p, s, &mut out);
            }
        }
        out
    }
}

impl PlaceView {
    /// The names on the city map: the municipalities as a big faint watermark inside their outlines (out of sight
    /// when zoomed in, where the streets' names take over) and the districts, terminals and towns, the more
    /// important ones from further away.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_names(
        &self,
        ui: &mut Painter,
        atlas: &mut Atlas,
        fonts: &Fonts,
        pkg: &Package,
        light: bool,
        mpp: f64,
        taken: &mut Vec<Rect>,
        win: Rect,
        s: f32,
        to_screen: &dyn Fn(DVec3) -> Vec2,
    ) {
        let (w, h) = (win.w, win.h);
        let col = if light {
            Color::rgba(70, 58, 44, 0.26)
        } else {
            Color::WHITE.alpha(0.15)
        };
        for r in pkg.regions().iter().filter(|r| r.box_w > 0.0) {
            let p = to_screen(DVec3::new(r.label.0, r.label.1, 0.0));
            let (bw, bh) = ((r.box_w / mpp) as f32, (r.box_h / mpp) as f32);
            if bh > 110.0 * s
                || p.x + bw * 0.5 < 0.0
                || p.x - bw * 0.5 > w
                || p.y + bh < 0.0
                || p.y - bh > h
            {
                continue;
            }
            let name = r.name.to_uppercase();
            let mut px = bh;
            let tw = fonts.width(&name, px, Weight::Bold);
            if tw > bw {
                px *= bw / tw;
            }
            // (in steps: every size is a set of glyphs in the atlas)
            px = (px / 6.0).round() * 6.0;
            if px < 12.0 * s {
                continue;
            }
            ui.text(
                atlas,
                fonts,
                &name,
                px,
                Weight::Bold,
                p + Vec2::new(0.0, px * 0.36),
                Align::Center,
                col,
            );
        }
        let min_rank = if mpp > 60.0 {
            200
        } else if mpp > 25.0 {
            140
        } else if mpp > 10.0 {
            100
        } else {
            0
        };
        let px = 13.0 * s;
        for l in &self.labels {
            if l.rank < min_rank {
                continue;
            }
            let p = to_screen(DVec3::new(l.at.0, l.at.1, 0.0));
            if !win.pad(40.0 * s, 30.0 * s).contains(p) {
                continue;
            }
            let weight = if l.kind == 0 {
                Weight::Medium
            } else {
                Weight::Bold
            };
            let name = fonts.fit(&l.name, px, weight, 220.0 * s);
            let tw = fonts.width(&name, px, weight);
            let bb = Rect::new(p.x - tw * 0.5, p.y - px * 0.7, tw, px * 1.3);
            if taken.iter().any(|t| rects_overlap(t, &bb)) {
                continue;
            }
            taken.push(bb);
            let (halo, ink_dim, ink) = if light {
                (
                    Color::hex(0xf4f1ea).alpha(0.9),
                    Color::hex(0x5b564c),
                    Color::hex(0x2d2a25),
                )
            } else {
                (Color::rgba(15, 15, 15, 0.9), TEXT_DIM, TEXT)
            };
            let at = p + Vec2::new(0.0, px * 0.35);
            for o in [
                Vec2::new(1.0, 0.0),
                Vec2::new(-1.0, 0.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(0.0, -1.0),
            ] {
                ui.text(
                    atlas,
                    fonts,
                    &name,
                    px,
                    weight,
                    at + o * s,
                    Align::Center,
                    halo,
                );
            }
            ui.text(
                atlas,
                fonts,
                &name,
                px,
                weight,
                at,
                Align::Center,
                if l.kind == 0 { ink_dim } else { ink },
            );
        }
    }

    /// Where the bus is, under the header of the city map: the municipality, the street, and the street's limit as
    /// a road sign (and the heavy vehicles', smaller, when it is another).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_chips(
        &self,
        ui: &mut Painter,
        atlas: &mut Atlas,
        fonts: &Fonts,
        head: Rect,
        w: f32,
        pad: f32,
        s: f32,
    ) {
        // the words are in the catalog (crates/omsi-app/locales/app.yml): looked up before they are put into a sentence
        let (you_are_on, heavy) = (omsi_ui::tr("You are on"), omsi_ui::tr("heavy"));
        let mut y = head.bottom() + 10.0 * s;
        if let Some(c) = self.city.as_ref().map(|c| c.to_uppercase()) {
            let tw = fonts.width(&c, 12.5 * s, Weight::Bold) + 20.0 * s;
            let r = Rect::new(pad, y, tw, 24.0 * s);
            ui.rounded(r, 3.0 * s, Color::rgba(10, 12, 16, 0.88));
            ui.text_in(
                atlas,
                fonts,
                &c,
                12.5 * s,
                Weight::Bold,
                r,
                Align::Center,
                TEXT,
            );
            y += 28.0 * s;
        }
        let Some(st) = self.street.as_ref() else {
            return;
        };
        let t = fonts.fit(
            &format!("{}: {}", you_are_on, st),
            12.0 * s,
            Weight::Medium,
            w * 0.45,
        );
        let tw = fonts.width(&t, 12.0 * s, Weight::Medium) + 20.0 * s;
        let r = Rect::new(pad, y, tw, 22.0 * s);
        ui.rounded(r, 3.0 * s, Color::rgba(10, 12, 16, 0.82));
        ui.text_in(
            atlas,
            fonts,
            &t,
            12.0 * s,
            Weight::Medium,
            r,
            Align::Center,
            TEXT,
        );
        let mut sx = pad + tw + 8.0 * s;
        for (v, rad) in [(self.limit.0, 17.0 * s), (self.limit.1, 12.5 * s)] {
            if v == 0 {
                continue;
            }
            let c = Vec2::new(sx + rad, r.center().y);
            ui.rounded(
                Rect::new(c.x - rad, c.y - rad, 2.0 * rad, 2.0 * rad),
                rad,
                Color::rgba(205, 30, 36, 1.0),
            );
            ui.rounded(
                Rect::new(c.x - rad * 0.74, c.y - rad * 0.74, 1.48 * rad, 1.48 * rad),
                rad * 0.74,
                Color::WHITE,
            );
            ui.text_in(
                atlas,
                fonts,
                &v.to_string(),
                rad * 0.95,
                Weight::Bold,
                Rect::new(c.x - rad, c.y - rad, 2.0 * rad, 2.0 * rad),
                Align::Center,
                Color::rgba(15, 15, 15, 1.0),
            );
            sx += 2.0 * rad + 6.0 * s;
            if rad < 14.0 * s {
                ui.text(
                    atlas,
                    fonts,
                    &heavy,
                    10.5 * s,
                    Weight::Medium,
                    Vec2::new(sx, c.y + 4.0 * s),
                    Align::Left,
                    TEXT_DIM,
                );
            }
        }
    }
}

impl PlaceView {
    /// The package says where one is (municipalities or named streets).
    pub(super) fn has_location(&self) -> bool {
        self.has_location
    }
}
