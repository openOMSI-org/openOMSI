use super::*;
use crate::perf_overlay::{graph_scale, Part, PerfOverlay, Sample, HISTORY};

const GRAPH_H: u32 = 64;
const GRAPH_EVERY: f32 = 1.0 / 30.0;

#[derive(Default)]
pub(crate) struct PerfPanel {
    graph: Option<TextureId>,
    image: Option<omsi_texture::Image>,
    drawn: u64,
    since: f32,
    width: f32,
}

pub(crate) fn raster(frames: &std::collections::VecDeque<Sample>, rgba: &mut [u8]) {
    let (w, h) = (HISTORY, GRAPH_H as usize);
    let top = graph_scale(frames);
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&[0, 0, 0, 70]);
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
            for c in 0..3 {
                rgba[i + c] = ((rgba[i + c] as u16 + 255) / 2) as u8;
            }
            rgba[i + 3] = rgba[i + 3].max(150);
        }
    }
}

impl Ui {
    pub(super) fn draw_perf(
        &mut self,
        r: &Renderer,
        scene: &mut Scene,
        f: &Frame,
        s: f32,
        dt: f32,
    ) {
        let Some(o) = f.perf else {
            if let Some(tex) = self.perf.graph.take() {
                r.free_texture(scene, tex);
            }
            self.perf = PerfPanel::default();
            return;
        };
        let graph = self.perf_graph(r, scene, o, dt);
        let px = (12.0 * s).round() as u32;
        let lh = (px as f32 * 1.45).round();
        let pad = 10.0 * s;
        let text = [220, 220, 220, 0];
        let title = self.text.label_op(
            r,
            scene,
            &omsi_ui::tr("Performance"),
            px | BOLD,
            [240, 240, 240, 0],
            1.0,
        );
        let head = self.text.label_op(
            r,
            scene,
            &o.readout.head,
            px | BOLD,
            [240, 240, 240, 0],
            1.0,
        );
        let parts: Vec<(Part, Label, Label)> = o
            .readout
            .parts
            .iter()
            .map(|(p, v)| {
                (
                    *p,
                    self.text
                        .label_op(r, scene, &omsi_ui::tr(p.name()), px, text, 1.0),
                    self.text.label_op(r, scene, v, px, text, 1.0),
                )
            })
            .collect();
        let lines: Vec<Label> = o
            .readout
            .lines
            .iter()
            .map(|t| self.text.label_op(r, scene, &omsi_ui::tr(t), px, text, 1.0))
            .collect();
        let swatch = 8.0 * s;
        let col = parts
            .iter()
            .map(|(_, n, v)| swatch + 6.0 * s + n.w as f32 + 12.0 * s + v.w as f32)
            .fold(0.0f32, f32::max);
        let inner = [
            (title.w + head.w) as f32 + 16.0 * s,
            col * 2.0 + 12.0 * s,
            280.0 * s,
        ]
        .into_iter()
        .chain(lines.iter().map(|l| l.w as f32))
        .fold(0.0f32, f32::max);
        let step = 20.0 * s;
        self.perf.width = self
            .perf
            .width
            .max((inner / step).ceil() * step + pad * 2.0);
        let w = self.perf.width;
        let x0 = (f.width - w - 14.0 * s).round();
        let y0 = if f.fps.is_some() {
            (36.0 * s).round()
        } else {
            (10.0 * s).round()
        };
        let gh = (GRAPH_H as f32 * s).round();
        let rows = parts.len().div_ceil(2);
        let h = pad
            + lh
            + 4.0 * s
            + gh
            + 6.0 * s
            + lh * rows as f32
            + 4.0 * s
            + lh * lines.len() as f32
            + pad;
        self.text.rounded(
            r,
            scene,
            [x0, y0, x0 + w, y0 + h],
            8.0 * s,
            [28, 28, 28, (220.0 * self.text.backdrop.min(1.0)) as u8],
        );
        let mut put = |l: &Label, x: f32, y: f32| {
            let (x, y) = (x.round(), (y + (lh - l.h as f32) * 0.5).round());
            scene
                .overlays
                .push((l.tex, [x, y, x + l.w as f32, y + l.h as f32]));
        };
        let mut y = y0 + pad;
        put(&title, x0 + pad, y);
        put(&head, x0 + w - pad - head.w as f32, y);
        y += lh + 4.0 * s;
        let scale = self.text.label_op(
            r,
            scene,
            &format!("{} ms", (graph_scale(&o.frames) * 1000.0).round() as i32),
            (10.0 * s).round() as u32,
            [200, 200, 200, 0],
            1.0,
        );
        if let Some(tex) = graph {
            scene
                .overlays
                .push((tex, [x0 + pad, y, x0 + w - pad, y + gh]));
        }
        scene.overlays.push((
            scale.tex,
            [
                x0 + pad + 3.0 * s,
                y + 2.0 * s,
                x0 + pad + 3.0 * s + scale.w as f32,
                y + 2.0 * s + scale.h as f32,
            ],
        ));
        y += gh + 6.0 * s;
        let cw = (w - pad * 2.0 - 12.0 * s) * 0.5;
        for (k, (p, name, value)) in parts.iter().enumerate() {
            let x = x0 + pad + (k % 2) as f32 * (cw + 12.0 * s);
            let ry = y + (k / 2) as f32 * lh;
            let sw = self.text.solid(r, scene, p.color());
            let sy = (ry + (lh - swatch) * 0.5).round();
            scene.overlays.push((sw, [x, sy, x + swatch, sy + swatch]));
            let (nx, vx) = (x + swatch + 6.0 * s, x + cw - value.w as f32);
            let at = |l: &Label, x: f32| {
                let (x, y) = (x.round(), (ry + (lh - l.h as f32) * 0.5).round());
                (l.tex, [x, y, x + l.w as f32, y + l.h as f32])
            };
            scene.overlays.push(at(name, nx));
            scene.overlays.push(at(value, vx));
        }
        y += lh * rows as f32 + 4.0 * s;
        let mut put = |l: &Label, x: f32, y: f32| {
            let (x, y) = (x.round(), (y + (lh - l.h as f32) * 0.5).round());
            scene
                .overlays
                .push((l.tex, [x, y, x + l.w as f32, y + l.h as f32]));
        };
        for l in &lines {
            put(l, x0 + pad, y);
            y += lh;
        }
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
        let image = self.perf.image.get_or_insert_with(|| omsi_texture::Image {
            width: HISTORY as u32,
            height: GRAPH_H,
            rgba: vec![0; HISTORY * GRAPH_H as usize * 4],
            has_alpha: true,
        });
        raster(&o.frames, &mut image.rgba);
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
        raster(&frames, &mut rgba);
        let at = |x: usize, row: usize| {
            let i = ((GRAPH_H as usize - 1 - row) * HISTORY + x) * 4;
            [rgba[i], rgba[i + 1], rgba[i + 2]]
        };
        let x = HISTORY - 1;
        assert_eq!(at(x, 5), Part::Simulation.color()[..3]);
        assert_eq!(at(x, 40), Part::Gpu.color()[..3]);
        assert_eq!(at(0, 5), [0, 0, 0]);
    }
}
