//! The interface's text: Inter rendered once per label into a small texture with a dark
//! outline, kept while it is shown, and the plates and shapes drawn with it.

use super::*;

/// Inter (SIL Open Font Licence 1.1), the interface font.
pub(super) const INTER: &[u8] = include_bytes!("../../../../assets/fonts/InterVariable.ttf");

/// A rendered text: its texture and size in pixels.
#[derive(Clone, Copy)]
pub(super) struct Label {
    pub(super) tex: TextureId,
    pub(super) w: u32,
    pub(super) h: u32,
    pub(super) used: u64,
}

/// Or'ed into a label's pixel size: the text in the bold weight.
pub(super) const BOLD: u32 = 1 << 31;

/// Texts rendered into textures, kept while they are used.
pub struct TextCache {
    pub(super) font: FontVec,
    /// The same Inter at 700: the menu's titles and its primary button, as the launcher
    /// draws them (`Weight::Bold`); asked for with `BOLD` in the size.
    pub(super) bold: FontVec,
    pub(super) labels: hashbrown::HashMap<(String, u32, [u8; 4]), Label>,
    pub(super) frame: u64,
    /// How strongly the background plates are drawn this frame (`backdrop`).
    pub(super) backdrop: f32,
    /// No outline round the texts that ask for none (the game menu's: flat text on its card).
    pub(super) flat: bool,
}

impl TextCache {
    pub fn new() -> Option<TextCache> {
        let mut font = FontVec::try_from_vec(INTER.to_vec()).ok()?;
        // a little heavier than the regular 400: light text on a dark panel is thin and
        // greyish at menu sizes otherwise
        let _ = font.set_variation(b"wght", 500.0);
        let mut bold = FontVec::try_from_vec(INTER.to_vec()).ok()?;
        let _ = bold.set_variation(b"wght", 700.0);
        Some(TextCache { font, bold, labels: hashbrown::HashMap::new(), frame: 0, backdrop: 1.0, flat: false })
    }

    /// The texture of `text` at `px` pixels in `color` (alpha = opacity of the outline), and
    /// its size.
    pub(super) fn label(&mut self, r: &Renderer, scene: &mut Scene, text: &str, px: u32, color: [u8; 4]) -> Label {
        let color = [color[0], color[1], color[2], outline_for(color, if self.flat { 1.0 } else { self.backdrop })];
        // (in the interface's language: the menu, the notes, the windows; a letter and its
        // combining mark as one, as macOS gives file names - the weather "Eiseska\u{308}lte"
        // showed a box after its "a" in the menu's list)
        let text = omsi_ui::tr(text);
        let text = &*omsi_ui::text::composed(&text);
        let key = (text.to_string(), px, color);
        if let Some(l) = self.labels.get_mut(&key) {
            l.used = self.frame;
            return *l;
        }
        let font = if px & BOLD != 0 { &self.bold } else { &self.font };
        let img = render_text(font, text, (px & !BOLD) as f32, color);
        let tex = r.add_texture(scene, &img, false);
        let l = Label { tex, w: img.width, h: img.height, used: self.frame };
        self.labels.insert(key, l);
        l
    }

    /// `text` as it is (not looked up for a translation: the caller has the words in the
    /// player's language already) at `px` pixels in `color`, without an outline, `opacity`
    /// of the way faded in - in sixteenths, so that a fade is a few textures, not one a frame.
    /// The calm look of the notifications and the loading screen: flat text on its card.
    pub(super) fn label_op(&mut self, r: &Renderer, scene: &mut Scene, text: &str, px: u32, color: [u8; 4], opacity: f32) -> Label {
        let k = (opacity.clamp(0.0, 1.0) * 16.0).round() as u32;
        let color = [color[0], color[1], color[2], 0];
        let text = &*omsi_ui::text::composed(text);
        // (the step of the fade in the key's size, above the pixel size and under `BOLD`)
        let key = (text.to_string(), px | ((k + 1) << 24), color);
        if let Some(l) = self.labels.get_mut(&key) {
            l.used = self.frame;
            return *l;
        }
        let font = if px & BOLD != 0 { &self.bold } else { &self.font };
        let mut img = render_text(font, text, (px & !BOLD) as f32, color);
        if k < 16 {
            for a in img.rgba.iter_mut().skip(3).step_by(4) {
                *a = (*a as u32 * k / 16) as u8;
            }
        }
        let tex = r.add_texture(scene, &img, false);
        let l = Label { tex, w: img.width, h: img.height, used: self.frame };
        self.labels.insert(key, l);
        l
    }

    /// Text width in pixels, without rendering it.
    pub fn width(&self, text: &str, px: f32) -> f32 {
        let text = &*omsi_ui::tr(text);
        self.width_raw(text, px)
    }

    /// `width` of a text drawn in the bold weight (`BOLD`).
    pub fn width_bold(&self, text: &str, px: f32) -> f32 {
        let text = &*omsi_ui::tr(text);
        self.width_in(&self.bold, text, px)
    }

    pub(super) fn width_raw(&self, text: &str, px: f32) -> f32 {
        self.width_in(&self.font, text, px)
    }

    pub(super) fn width_in(&self, base: &FontVec, text: &str, px: f32) -> f32 {
        let text = omsi_ui::text::composed(text);
        let mut w = 0.0;
        let mut prev: Option<(ab_glyph::GlyphId, *const FontVec)> = None;
        for c in text.chars() {
            let font = font_for(base, c);
            let f = font.as_scaled(PxScale::from(px));
            let id = f.glyph_id(c);
            if let Some((p, pf)) = prev {
                if std::ptr::eq(pf, font) {
                    w += f.kern(p, id);
                }
            }
            w += f.h_advance(id);
            prev = Some((id, font as *const FontVec));
        }
        w + outline_px(px) * 2.0 + 2.0
    }

    /// End of a frame: labels not used for a few seconds are released.
    pub fn end_frame(&mut self, r: &Renderer, scene: &mut Scene) {
        self.frame += 1;
        if self.frame % 120 == 0 {
            let old: Vec<_> = self.labels.iter().filter(|(_, l)| self.frame.saturating_sub(l.used) > 240).map(|(k, _)| k.clone()).collect();
            for k in old {
                if let Some(l) = self.labels.remove(&k) {
                    r.free_texture(scene, l.tex);
                }
            }
        }
    }
}

/// How opaque a label's dark outline is (the alpha of its `color`): as asked, or - for a
/// panel's text, asked without one - more as the opacity setting thins the panels
/// (`backdrop` below 1): on a see-through panel over a bright sky the dim lines could not be
/// read. Light texts only: round a dark one (the highlighted menu line's, on the solid
/// accent) the outline is as dark as the glyphs and smeared them, like a shadow.
pub(super) fn outline_for(color: [u8; 4], backdrop: f32) -> u8 {
    let light = 0.299 * color[0] as f32 + 0.587 * color[1] as f32 + 0.114 * color[2] as f32 > 100.0;
    match color[3] {
        0 if backdrop < 1.0 && light => (((1.0 - backdrop) * 1.6).min(0.9) * 255.0) as u8,
        a => a,
    }
}

/// The outline (UIStroke) around the glyphs, in pixels.
pub(super) fn outline_px(px: f32) -> f32 {
    (px / 9.0).clamp(1.0, 3.0)
}

/// The font that draws `c`: Inter, else the system's font for the script (Chinese,
/// Japanese, Korean, Thai, Hindi - the menu was a column of boxes in those languages).
pub(super) fn font_for(inter: &FontVec, c: char) -> &FontVec {
    if omsi_ui::text::needs_fallback(inter, c) {
        if let Some(f) = omsi_ui::text::fallback_font(c) {
            return f;
        }
    }
    inter
}

/// `text` as straight-alpha RGBA: the glyphs in `color` over a dark outline.
pub(super) fn render_text(font: &FontVec, text: &str, px: f32, color: [u8; 4]) -> omsi_texture::Image {
    let f = font.as_scaled(PxScale::from(px));
    let stroke = outline_px(px);
    let pad = stroke.ceil() as i32 + 1;
    let asc = f.ascent();
    let h = (asc - f.descent()).ceil() as i32 + pad * 2;
    // the baseline and every glyph on whole pixels: glyphs drawn between pixels came out
    // different from letter to letter and soft (the pen itself still runs on fractions, so
    // the text keeps its width)
    let base = (pad as f32 + asc).round();
    // lay the glyphs out
    let mut glyphs: Vec<(&FontVec, ab_glyph::Glyph)> = Vec::new();
    let mut x = pad as f32;
    let mut prev: Option<(ab_glyph::GlyphId, *const FontVec)> = None;
    for c in text.chars() {
        let gf = font_for(font, c);
        let sf = gf.as_scaled(PxScale::from(px));
        let id = sf.glyph_id(c);
        if let Some((p, pf)) = prev {
            if std::ptr::eq(pf, gf) {
                x += sf.kern(p, id);
            }
        }
        glyphs.push((gf, id.with_scale_and_position(PxScale::from(px), ab_glyph::point(x.round(), base))));
        x += sf.h_advance(id);
        prev = Some((id, gf as *const FontVec));
    }
    let w = (x.ceil() as i32 + pad).max(1);
    let (wu, hu) = (w as usize, h.max(1) as usize);
    let mut cov = vec![0f32; wu * hu];
    for (gf, g) in glyphs {
        if let Some(o) = gf.outline_glyph(g) {
            let b = o.px_bounds();
            o.draw(|gx, gy, c| {
                let xx = b.min.x as i32 + gx as i32;
                let yy = b.min.y as i32 + gy as i32;
                if xx >= 0 && yy >= 0 && (xx as usize) < wu && (yy as usize) < hu {
                    let i = yy as usize * wu + xx as usize;
                    cov[i] = (cov[i] + c).min(1.0);
                }
            });
        }
    }
    // the texture is sRGB and blended in linear light: light text on a dark panel comes out
    // too fat and blurry, dark text on a light one too thin - bend the coverage to match
    let light = 0.299 * color[0] as f32 + 0.587 * color[1] as f32 + 0.114 * color[2] as f32 > 100.0;
    let gamma = if light { 1.45 } else { 0.8 };
    for c in cov.iter_mut() {
        if *c > 0.0 && *c < 1.0 {
            *c = c.powf(gamma);
        }
    }
    // the outline: the coverage grown by the stroke radius
    let r = stroke;
    let ri = r.ceil() as i32;
    let mut edge = vec![0f32; wu * hu];
    for y in 0..hu as i32 {
        for x in 0..wu as i32 {
            let mut m = 0f32;
            for dy in -ri..=ri {
                for dx in -ri..=ri {
                    let d = ((dx * dx + dy * dy) as f32).sqrt();
                    if d > r + 0.5 {
                        continue;
                    }
                    let (sx, sy) = (x + dx, y + dy);
                    if sx < 0 || sy < 0 || sx >= wu as i32 || sy >= hu as i32 {
                        continue;
                    }
                    let k = (r + 0.5 - d).clamp(0.0, 1.0);
                    m = m.max(cov[sy as usize * wu + sx as usize] * k);
                }
            }
            edge[y as usize * wu + x as usize] = m;
        }
    }
    let oa = color[3] as f32 / 255.0;
    let mut rgba = vec![0u8; wu * hu * 4];
    for i in 0..wu * hu {
        let a_text = cov[i];
        let a_edge = edge[i] * oa;
        let a = a_text + a_edge * (1.0 - a_text);
        if a <= 0.0 {
            continue;
        }
        for c in 0..3 {
            let v = (color[c] as f32 * a_text + 12.0 * a_edge * (1.0 - a_text)) / a;
            rgba[i * 4 + c] = v.round().clamp(0.0, 255.0) as u8;
        }
        rgba[i * 4 + 3] = (a * 255.0).round() as u8;
    }
    omsi_texture::Image { width: wu as u32, height: hu as u32, rgba, has_alpha: true }
}

impl TextCache {
    /// The mouse steering's cross: thin white arms with a dark edge, as a crosshair cursor,
    /// centred on the point that steers and receives a click.
    pub(super) fn steer_cross(&mut self, r: &Renderer, scene: &mut Scene) -> TextureId {
        let key = ("\u{0}steer_cross".to_string(), 0, [0, 0, 0, 0]);
        if let Some(label) = self.labels.get_mut(&key) {
            label.used = self.frame;
            return label.tex;
        }
        const N: usize = 34;
        let rgba = steer_cross_pixels(N);
        let image = omsi_texture::Image { width: N as u32, height: N as u32, rgba, has_alpha: true };
        let tex = r.add_texture(scene, &image, false);
        self.labels.insert(key, Label { tex, w: N as u32, h: N as u32, used: self.frame });
        tex
    }

    /// A small white circle, centred on the point that receives the click.
    pub(super) fn vr_pointer(&mut self, r: &Renderer, scene: &mut Scene) -> TextureId {
        let key = ("\u{0}vr_pointer_dot".to_string(), 0, [0, 0, 0, 0]);
        if let Some(label) = self.labels.get_mut(&key) {
            label.used = self.frame;
            return label.tex;
        }
        const W: usize = 32;
        const H: usize = 32;
        let mut rgba = vec![0u8; W * H * 4];
        for y in 0..H {
            for x in 0..W {
                let dx = x as f32 + 0.5 - W as f32 * 0.5;
                let dy = y as f32 + 0.5 - H as f32 * 0.5;
                let radius = (dx * dx + dy * dy).sqrt();
                let alpha = (16.0 - radius).clamp(0.0, 1.0);
                let white = radius < 13.0;
                let color = if white {
                    [255, 255, 255, (alpha * 255.0) as u8]
                } else {
                    [0, 0, 0, (alpha * 220.0) as u8]
                };
                rgba[(y * W + x) * 4..(y * W + x + 1) * 4].copy_from_slice(&color);
            }
        }
        let image = omsi_texture::Image { width: W as u32, height: H as u32, rgba, has_alpha: true };
        let tex = r.add_texture(scene, &image, false);
        self.labels.insert(key, Label { tex, w: W as u32, h: H as u32, used: self.frame });
        tex
    }

    /// A plate of one colour: 0 the chat's dark translucent input box, 1 the loading
    /// screen's bar track, 2 its fill, 3 .. 7 as said below.
    pub(super) fn plate(&mut self, r: &Renderer, scene: &mut Scene, kind: u8) -> TextureId {
        let mut rgba = match kind {
            1 => vec![255, 255, 255, 38],
            2 => vec![235, 238, 242, 255],
            // an opaque panel as designed (translucent dark panels show the sky through
            // them), unless the opacity setting asks for less
            3 => vec![22, 22, 22, 255],
            // the chosen line: the interface's accent
            4 => accent().to_vec(),
            // a line the mouse is over, the next stop
            5 => vec![255, 255, 255, 28],
            // a hairline between groups (the card's border colour)
            9 => vec![255, 255, 255, 15],
            // the picture dimmed behind the menu
            6 => vec![0, 0, 0, 120],
            // behind a note or the tooltip (the old HUD's 40 %)
            7 => vec![0, 0, 0, 102],
            _ => vec![10, 12, 16, 150],
        };
        // the backgrounds - the panels, the chat's box, the plates - as the opacity setting
        // has them (`backdrop`); the accent, the lines and the dimming stay as they are
        if matches!(kind, 0 | 3 | 7) {
            rgba[3] = (rgba[3] as f32 * self.backdrop).round().clamp(0.0, 255.0) as u8;
        }
        let key = ("\u{0}plate".to_string(), kind as u32, [rgba[0], rgba[1], rgba[2], rgba[3]]);
        if let Some(l) = self.labels.get_mut(&key) {
            l.used = self.frame;
            return l.tex;
        }
        let img = omsi_texture::Image { width: 1, height: 1, rgba, has_alpha: true };
        let tex = r.add_texture(scene, &img, false);
        self.labels.insert(key, Label { tex, w: 1, h: 1, used: self.frame });
        tex
    }

    /// A plate of one RGBA colour, any size (stretched).
    pub(super) fn solid(&mut self, r: &Renderer, scene: &mut Scene, rgba: [u8; 4]) -> TextureId {
        let key = ("\u{0}solid".to_string(), 0, rgba);
        if let Some(l) = self.labels.get_mut(&key) {
            l.used = self.frame;
            return l.tex;
        }
        let img = omsi_texture::Image { width: 1, height: 1, rgba: rgba.to_vec(), has_alpha: true };
        let tex = r.add_texture(scene, &img, false);
        self.labels.insert(key, Label { tex, w: 1, h: 1, used: u64::MAX / 2 });
        tex
    }

    /// One anti-aliased quarter disc of `rad` pixels: the rounded corner `idx` (0 top left,
    /// 1 top right, 2 bottom left, 3 bottom right) of a rounded plate.
    pub(super) fn corner(&mut self, r: &Renderer, scene: &mut Scene, rad: u32, idx: u32, rgba: [u8; 4]) -> TextureId {
        let key = ("\u{0}corner".to_string(), (rad << 8) | idx, rgba);
        if let Some(l) = self.labels.get_mut(&key) {
            l.used = self.frame;
            return l.tex;
        }
        let n = rad as usize;
        let (cx, cy) = (if idx & 1 == 0 { rad as f32 } else { 0.0 }, if idx & 2 == 0 { rad as f32 } else { 0.0 });
        let mut data = vec![0u8; n * n * 4];
        for py in 0..n {
            for px in 0..n {
                let (dx, dy) = (px as f32 + 0.5 - cx, py as f32 + 0.5 - cy);
                let cover = (rad as f32 - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
                let o = (py * n + px) * 4;
                data[o..o + 3].copy_from_slice(&rgba[..3]);
                data[o + 3] = (rgba[3] as f32 * cover).round() as u8;
            }
        }
        let img = omsi_texture::Image { width: rad, height: rad, rgba: data, has_alpha: true };
        let tex = r.add_texture(scene, &img, false);
        self.labels.insert(key, Label { tex, w: rad, h: rad, used: u64::MAX / 2 });
        tex
    }

    /// A texture made once for `key` (kept while it is used, like a text).
    pub(super) fn cached(&mut self, r: &Renderer, scene: &mut Scene, key: (String, u32, [u8; 4]), size: (u32, u32), build: impl FnOnce() -> Vec<u8>) -> TextureId {
        if let Some(l) = self.labels.get_mut(&key) {
            l.used = self.frame;
            return l.tex;
        }
        let img = omsi_texture::Image { width: size.0, height: size.1, rgba: build(), has_alpha: true };
        let tex = r.add_texture(scene, &img, false);
        self.labels.insert(key, Label { tex, w: size.0, h: size.1, used: self.frame });
        tex
    }

    /// A Material icon (`omsi_ui::icons`, as the launcher draws them) of `px` pixels in
    /// `rgba`; `None` for a name there is no icon of.
    pub(super) fn icon(&mut self, r: &Renderer, scene: &mut Scene, name: &str, px: u32, rgba: [u8; 4]) -> Option<TextureId> {
        omsi_ui::icons::svg(name)?;
        let px = px.max(8);
        let key = (format!("\u{0}icon:{name}"), px, rgba);
        Some(self.cached(r, scene, key, (px, px), || {
            let mask = omsi_ui::icons::rasterize(name, px).unwrap_or_else(|| vec![0; (px * px) as usize]);
            mask.iter().flat_map(|&a| [rgba[0], rgba[1], rgba[2], (a as u32 * rgba[3] as u32 / 255) as u8]).collect()
        }))
    }

    /// A whole rounded plate of `w` x `h` pixels in one texture, anti-aliased.
    pub(super) fn rrect(&mut self, r: &Renderer, scene: &mut Scene, w: u32, h: u32, rad: f32, rgba: [u8; 4]) -> TextureId {
        let key = (format!("\u{0}rr{w}x{h}r{}", rad as u32), 0, rgba);
        self.cached(r, scene, key, (w, h), || {
            let mut data = vec![0u8; (w * h * 4) as usize];
            let ri = rad as u32;
            for py in 0..h {
                let band_y = py >= ri && py + ri < h;
                for px in 0..w {
                    let cover = if band_y || (px >= ri && px + ri < w) {
                        1.0
                    } else {
                        let d = rr_dist(px as f32 + 0.5, py as f32 + 0.5, 0.0, 0.0, w as f32, h as f32, rad);
                        (0.5 - d).clamp(0.0, 1.0)
                    };
                    let o = ((py * w + px) * 4) as usize;
                    data[o..o + 3].copy_from_slice(&rgba[..3]);
                    data[o + 3] = (rgba[3] as f32 * cover).round() as u8;
                }
            }
            data
        })
    }

    /// A soft shadow under the rounded rectangle `rect`: it fades out over `spread` pixels
    /// and lies `dy` lower.
    pub(super) fn shadow(&mut self, r: &Renderer, scene: &mut Scene, rect: [f32; 4], radius: f32, spread: f32, dy: f32, alpha: u8) {
        let [x0, y0, x1, y1] = [rect[0].round(), rect[1].round(), rect[2].round(), rect[3].round()];
        let (w, h) = ((x1 - x0).max(0.0) as u32, (y1 - y0).max(0.0) as u32);
        let m = spread.round().max(1.0) as u32;
        let (tw, th) = (w + 2 * m, h + 2 * m);
        if w < 2 || h < 2 || tw as u64 * th as u64 > 4_000_000 {
            return;
        }
        let rad = radius.round().min((w as f32 * 0.5).floor()).min((h as f32 * 0.5).floor()).max(0.0);
        let key = (format!("\u{0}shadow{w}x{h}r{}m{m}", rad as u32), 0, [0, 0, 0, alpha]);
        let tex = self.cached(r, scene, key, (tw, th), || {
            let mut data = vec![0u8; (tw * th * 4) as usize];
            for py in 0..th {
                for px in 0..tw {
                    let d = rr_dist(px as f32 + 0.5, py as f32 + 0.5, m as f32, m as f32, w as f32, h as f32, rad);
                    let k = (1.0 - d / m as f32).clamp(0.0, 1.0);
                    data[((py * tw + px) * 4 + 3) as usize] = (alpha as f32 * k * k).round() as u8;
                }
            }
            data
        });
        scene.overlays.push((tex, [x0 - m as f32, y0 - m as f32 + dy, x1 + m as f32, y1 + m as f32 + dy]));
    }

    /// A rectangle with rounded corners of `radius` pixels in `rgba`, on whole pixels so
    /// that translucent colours show no seams: one texture for a plate of some size, else
    /// (a thin bar, a thumb that grows and shrinks) three bands and four corner discs.
    pub(super) fn rounded(&mut self, r: &Renderer, scene: &mut Scene, rect: [f32; 4], radius: f32, rgba: [u8; 4]) {
        let [x0, y0, x1, y1] = [rect[0].round(), rect[1].round(), rect[2].round(), rect[3].round()];
        let (w, h) = (x1 - x0, y1 - y0);
        if w < 1.0 || h < 1.0 {
            return;
        }
        let rad = radius.round().min((w * 0.5).floor()).min((h * 0.5).floor()).max(0.0);
        if rad < 1.0 {
            let solid = self.solid(r, scene, rgba);
            scene.overlays.push((solid, [x0, y0, x1, y1]));
            return;
        }
        if w.min(h) >= 12.0 && w * h <= 4_000_000.0 {
            let tex = self.rrect(r, scene, w as u32, h as u32, rad, rgba);
            scene.overlays.push((tex, [x0, y0, x1, y1]));
            return;
        }
        let solid = self.solid(r, scene, rgba);
        let ri = rad as u32;
        // the middle band over the whole height, the side bands between the corners
        if x1 - rad > x0 + rad {
            scene.overlays.push((solid, [x0 + rad, y0, x1 - rad, y1]));
        }
        if h > rad * 2.0 {
            scene.overlays.push((solid, [x0, y0 + rad, x0 + rad, y1 - rad]));
            scene.overlays.push((solid, [x1 - rad, y0 + rad, x1, y1 - rad]));
        }
        let tl = self.corner(r, scene, ri, 0, rgba);
        let tr = self.corner(r, scene, ri, 1, rgba);
        let bl = self.corner(r, scene, ri, 2, rgba);
        let br = self.corner(r, scene, ri, 3, rgba);
        scene.overlays.push((tl, [x0, y0, x0 + rad, y0 + rad]));
        scene.overlays.push((tr, [x1 - rad, y0, x1, y0 + rad]));
        scene.overlays.push((bl, [x0, y1 - rad, x0 + rad, y1]));
        scene.overlays.push((br, [x1 - rad, y1 - rad, x1, y1]));
    }
}
