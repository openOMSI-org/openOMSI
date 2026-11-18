use super::*;
use crate::perf_overlay::{graph_scale, Part, PerfOverlay, Sample, HISTORY};

const GRAPH_H: u32 = 64;
const GRAPH_EVERY: f32 = 1.0 / 30.0;
const SLOT_STEP: u32 = 64;

struct Slot {
    tex: TextureId,
    text: String,
    px: u32,
    color: [u8; 3],
    w: u32,
    h: u32,
    cap: u32,
}

#[derive(Clone, Copy)]
struct Placed {
    tex: TextureId,
    w: f32,
    h: f32,
    cap: f32,
}

#[derive(Default)]
pub(crate) struct PerfPanel {
    graph: Option<TextureId>,
    image: Option<omsi_texture::Image>,
    drawn: u64,
    since: f32,
    scale_ms: i32,
    width: f32,
    detailed: bool,
    size: f32,
    slots: Vec<Slot>,
}

impl PerfPanel {
    fn free(&mut self, r: &Renderer, scene: &mut Scene) {
        for tex in self
            .graph
            .take()
            .into_iter()
            .chain(self.slots.drain(..).map(|s| s.tex))
        {
            r.free_texture(scene, tex);
        }
        *self = PerfPanel::default();
    }
}

pub(crate) fn raster(frames: &std::collections::VecDeque<Sample>, top: f32, rgba: &mut [u8]) {
    let (w, h) = (HISTORY, GRAPH_H as usize);
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&[0, 0, 0, 0]);
    }
    let start = w - frames.len().min(w);
    for (k, s) in frames.iter().rev().take(w).rev().enumerate() {
        let x = start + k;
        let mut y = 0.0f32;
        for p in Part::ALL {
            let from = y;
            y += s.parts[p as usize] / top * h as f32;
            let (a, b) = (from.round() as usize, (y.round() as usize).min(h));
            for row in a..b {
                let i = ((h - 1 - row) * w + x) * 4;
                rgba[i..i + 4].copy_from_slice(&p.color());
            }
        }
    }
    for line in [1.0 / 60.0, 1.0 / 30.0] {
        let row = (line / top * h as f32).round() as usize;
        if row == 0 || row >= h {
            continue;
        }
        let y = h - 1 - row;
        for x in 0..w {
            let i = (y * w + x) * 4;
            if rgba[i + 3] == 0 {
                rgba[i..i + 4].copy_from_slice(&[128, 128, 136, 120]);
                continue;
            }
            for c in 0..3 {
                rgba[i + c] = ((rgba[i + c] as u16 + 255) / 2) as u8;
            }
        }
    }
}

pub(crate) fn padded(img: &omsi_texture::Image, cap: u32) -> omsi_texture::Image {
    let mut rgba = vec![0; (cap * img.height * 4) as usize];
    let (row, w) = ((cap * 4) as usize, (img.width.min(cap) * 4) as usize);
    for (y, src) in img.rgba.chunks_exact((img.width * 4) as usize).enumerate() {
        rgba[y * row..y * row + w].copy_from_slice(&src[..w]);
    }
    omsi_texture::Image {
        width: cap,
        height: img.height,
        rgba,
        has_alpha: true,
    }
}

impl Ui {
    fn perf_text(
        &mut self,
        r: &Renderer,
        scene: &mut Scene,
        k: usize,
        text: &str,
        px: u32,
        color: [u8; 3],
    ) -> Placed {
        if let Some(s) = self
            .perf
            .slots
            .get(k)
            .filter(|s| s.px == px && s.color == color && s.text == text)
        {
            return Placed {
                tex: s.tex,
                w: s.w as f32,
                h: s.h as f32,
                cap: s.cap as f32,
            };
        }
        let font = if px & BOLD != 0 {
            &self.text.bold
        } else {
            &self.text.font
        };
        let img = render_text(
            font,
            &omsi_ui::text::composed(text),
            (px & !BOLD) as f32,
            [color[0], color[1], color[2], 0],
        );
        let fits = self
            .perf
            .slots
            .get(k)
            .filter(|s| s.h == img.height && s.cap >= img.width)
            .map(|s| (s.tex, s.cap));
        let cap = fits.map_or(img.width.div_ceil(SLOT_STEP) * SLOT_STEP, |f| f.1);
        let full = padded(&img, cap);
        let tex = match fits {
            Some((tex, _)) => {
                r.update_texture(scene, tex, &full);
                tex
            }
            None => {
                if let Some(old) = self.perf.slots.get(k) {
                    r.free_texture(scene, old.tex);
                }
                r.add_texture(scene, &full, false)
            }
        };
        let slot = Slot {
            tex,
            text: text.to_string(),
            px,
            color,
            w: img.width,
            h: img.height,
            cap,
        };
        match self.perf.slots.get_mut(k) {
            Some(s) => *s = slot,
            None => self.perf.slots.push(slot),
        }
        Placed {
            tex,
            w: img.width as f32,
            h: img.height as f32,
            cap: cap as f32,
        }
    }

    pub(super) fn draw_perf(
        &mut self,
        r: &Renderer,
        scene: &mut Scene,
        f: &Frame,
        s: f32,
        dt: f32,
    ) {
        let Some(o) = f.perf else {
            if self.perf.graph.is_some() || !self.perf.slots.is_empty() {
                self.perf.free(r, scene);
            }
            return;
        };
        if o.frames.is_empty() {
            return;
        }
        let graph = self.perf_graph(r, scene, o, dt);
        let s0 = s;
        let size = self.shown_size(2, f.sizes[2]);
        let s = s * size;
        let detailed = f.perf_detailed;
        if self.perf.detailed != detailed || self.perf.size != size {
            self.perf.detailed = detailed;
            self.perf.size = size;
            self.perf.width = 0.0;
        }
        let px = ((if detailed { 12.0 } else { 11.0 }) * s).round() as u32;
        let lh = (px as f32 * 1.45).round();
        let pad = if f.perf_detailed { 14.0 } else { 11.0 } * s;
        let rgb = |c: [u8; 4]| [c[0], c[1], c[2]];
        let (bright, text, dim) = (rgb(self.hud.ink), rgb(self.hud.dim), rgb(self.hud.muted));
        let ro = &o.readout;
        let title = self.perf_text(r, scene, 0, &ro.title, px | BOLD, bright);
        let head = self.perf_text(r, scene, 1, &ro.head, px | BOLD, bright);
        let scale = self.perf_text(
            r,
            scene,
            2,
            &format!("{} ms", self.perf.scale_ms),
            (10.0 * s).round() as u32,
            dim,
        );
        let mut k = 3;
        let mut parts: [(Part, Option<(Placed, Placed)>); 8] = [(Part::Other, None); 8];
        let legend = if detailed { &ro.parts[..] } else { &[] };
        for (i, (p, name, value)) in legend.iter().enumerate().take(8) {
            let n = self.perf_text(r, scene, k, name, px, text);
            let v = self.perf_text(r, scene, k + 1, value, px, bright);
            parts[i] = (*p, Some((n, v)));
            k += 2;
        }
        let shown = legend.len().min(8);
        let mut lines: [Option<Placed>; 12] = [None; 12];
        let src = if detailed { &ro.lines } else { &ro.brief };
        for (i, t) in src.iter().enumerate().take(12) {
            lines[i] = Some(self.perf_text(r, scene, k + i, t, px, text));
        }
        let swatch = 8.0 * s;
        let col = parts
            .iter()
            .filter_map(|p| p.1)
            .map(|(n, v)| swatch + 6.0 * s + n.w + 12.0 * s + v.w)
            .fold(0.0f32, f32::max);
        let least = if detailed { 280.0 } else { 190.0 } * s;
        let inner = [title.w + head.w + 16.0 * s, col * 2.0 + 12.0 * s, least]
            .into_iter()
            .chain(lines.iter().flatten().map(|l| l.w))
            .fold(0.0f32, f32::max);
        let step = 20.0 * s;
        self.perf.width = self
            .perf
            .width
            .max((inner / step).ceil() * step + pad * 2.0);
        let w = self.perf.width;
        let (x0, y0) = if let Some(top) = f.perf_touch {
            let y = top.map_or(80.0 * f.scale.max(0.5) * f.ui_scale.max(1.0), |b| {
                b + 8.0 * s0
            });
            ((16.0 * s0).round(), y.round())
        } else if f.fps.is_some() {
            ((f.width - w - 14.0 * s0).round(), (36.0 * s0).round())
        } else {
            ((f.width - w - 14.0 * s0).round(), (10.0 * s0).round())
        };
        let y0 = self.below_info(x0, x0 + w, y0, s).round();
        let gh = ((if detailed { GRAPH_H as f32 } else { 30.0 }) * s).round();
        let rows = shown.div_ceil(2);
        let line_count = lines.iter().flatten().count();
        let legend_h = if rows > 0 {
            lh * rows as f32 + 4.0 * s
        } else {
            0.0
        };
        let h = pad + lh + 6.0 * s + gh + 10.0 * s + legend_h + lh * line_count as f32 + pad;
        self.hud_card(r, scene, [x0, y0, x0 + w, y0 + h], hud::RADIUS * s, self.text.backdrop, s);
        let put = |scene: &mut Scene, l: &Placed, x: f32, y: f32| {
            let (x, y) = (x.round(), (y + (lh - l.h) * 0.5).round());
            scene.overlays.push((l.tex, [x, y, x + l.cap, y + l.h]));
        };
        let mut y = y0 + pad;
        put(scene, &title, x0 + pad, y);
        put(scene, &head, x0 + w - pad - head.w, y);
        y += lh + 6.0 * s;
        let well = self.hud.shade(self.hud.ink, 0.06, self.text.backdrop);
        self.text.rounded(r, scene, [x0 + pad - 4.0 * s, y - 4.0 * s, x0 + w - pad + 4.0 * s, y + gh + 4.0 * s], 8.0 * s, well);
        if let Some(tex) = graph {
            scene
                .overlays
                .push((tex, [x0 + pad, y, x0 + w - pad, y + gh]));
        }
        let (sx, sy) = ((x0 + pad + 3.0 * s).round(), (y + 2.0 * s).round());
        scene
            .overlays
            .push((scale.tex, [sx, sy, sx + scale.cap, sy + scale.h]));
        y += gh + 10.0 * s;
        let cw = (w - pad * 2.0 - 12.0 * s) * 0.5;
        for (i, (p, placed)) in parts.iter().enumerate() {
            let Some((name, value)) = placed else {
                continue;
            };
            let x = x0 + pad + (i % 2) as f32 * (cw + 12.0 * s);
            let ry = y + (i / 2) as f32 * lh;
            let sy = (ry + (lh - swatch) * 0.5).round();
            self.text.rounded(r, scene, [x, sy, x + swatch, sy + swatch], swatch * 0.5, p.color());
            put(scene, name, x + swatch + 6.0 * s, ry);
            put(scene, value, x + cw - value.w, ry);
        }
        y += legend_h;
        for l in lines.iter().flatten() {
            put(scene, l, x0 + pad, y);
            y += lh;
        }
        self.hud_grip(r, scene, 2, [x0, y0, x0 + w, y0 + h], size, f, s);
    }

    fn perf_graph(
        &mut self,
        r: &Renderer,
        scene: &mut Scene,
        o: &PerfOverlay,
        dt: f32,
    ) -> Option<TextureId> {
        self.perf.since += dt;
        let fresh = self.perf.drawn != o.version && self.perf.since >= GRAPH_EVERY;
        if self.perf.graph.is_some() && !fresh {
            return self.perf.graph;
        }
        self.perf.since = 0.0;
        self.perf.drawn = o.version;
        let top = graph_scale(&o.frames);
        self.perf.scale_ms = (top * 1000.0).round() as i32;
        let image = self.perf.image.get_or_insert_with(|| omsi_texture::Image {
            width: HISTORY as u32,
            height: GRAPH_H,
            rgba: vec![0; HISTORY * GRAPH_H as usize * 4],
            has_alpha: true,
        });
        raster(&o.frames, top, &mut image.rgba);
        match self.perf.graph {
            Some(tex) => r.update_texture(scene, tex, image),
            None => self.perf.graph = Some(r.add_texture(scene, image, false)),
        }
        self.perf.graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_is_a_column_stacked_by_its_parts() {
        let mut frames = std::collections::VecDeque::new();
        let mut parts = [0.0; 8];
        parts[Part::Simulation as usize] = 1.0 / 60.0;
        parts[Part::Gpu as usize] = 1.0 / 60.0;
        frames.push_back(Sample {
            total: 1.0 / 30.0,
            parts,
        });
        let mut rgba = vec![0; HISTORY * GRAPH_H as usize * 4];
        raster(&frames, graph_scale(&frames), &mut rgba);
        let at = |x: usize, row: usize| {
            let i = ((GRAPH_H as usize - 1 - row) * HISTORY + x) * 4;
            [rgba[i], rgba[i + 1], rgba[i + 2]]
        };
        let x = HISTORY - 1;
        assert_eq!(at(x, 5), Part::Simulation.color()[..3]);
        assert_eq!(at(x, 40), Part::Gpu.color()[..3]);
        assert_eq!(at(0, 5), [0, 0, 0]);
    }

    #[test]
    fn a_text_keeps_its_rows_in_a_wider_texture() {
        let img = omsi_texture::Image {
            width: 2,
            height: 2,
            rgba: (1..=16).collect(),
            has_alpha: true,
        };
        let p = padded(&img, 4);
        assert_eq!((p.width, p.height), (4, 2));
        assert_eq!(&p.rgba[..8], &img.rgba[..8]);
        assert_eq!(&p.rgba[8..16], &[0; 8]);
        assert_eq!(&p.rgba[16..24], &img.rgba[8..16]);
    }
}
