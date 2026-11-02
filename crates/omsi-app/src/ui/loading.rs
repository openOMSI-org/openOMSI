//! The loading screen: openOMSI's wordmark and striped bar, or an interface theme's colours and
//! logo - which may itself be the bar, drawn along its path, with its marks popping in (`themes`,
//! `omsi_ui::icons`). The time it follows is `Loading`'s (see the parent module).

use super::*;
use omsi_ui::icons::Stage;

/// The openOMSI wordmark (white letters, the orange "OMSI"), for the loading screen.
const WORDMARK: &str = include_str!("../../../../assets/logos/openomsi-wordmark-light.svg");

/// The pixels of a stretch of the loading bar `w` x `h` with its stripes: 45 degree bands of
/// the accent `dark` and a lighter one `light`, `period` pixels apart along the bar, moved
/// `phase` pixels on.
fn stripe_pixels(w: u32, h: u32, period: u32, phase: u32, alpha: u8, dark: [u8; 3], light: [u8; 3]) -> Vec<u8> {
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let lit = (x + period * 4 + phase - y % period) % period < period / 2;
            let c = if lit { light } else { dark };
            px.extend_from_slice(&[c[0], c[1], c[2], alpha]);
        }
    }
    px
}

/// The loading screen's colours: the background, the texts (the map's name, the tips), how
/// far (the per cent), the accent (the bar, "TIPS"), the bar's track, and the bar's stripes -
/// openOMSI's, or an interface theme's (`themes`).
struct LoadingLook {
    bg: [u8; 3],
    text: [u8; 3],
    ink: [u8; 3],
    accent: [u8; 3],
    track: [u8; 3],
    stripes: ([u8; 3], [u8; 3]),
}

impl LoadingLook {
    fn of(theme: Option<&crate::themes::Theme>) -> LoadingLook {
        let rgb = |c: omsi_ui::Color| {
            let [r, g, b, _] = omsi_ui::theme::rgba8(c);
            [r, g, b]
        };
        match theme {
            None => LoadingLook { bg: [18, 18, 18], text: [184, 184, 184], ink: [237, 237, 237], accent: [232, 160, 48], track: [42, 42, 42], stripes: ([232, 160, 48], [244, 182, 82]) },
            Some(t) => {
                // (the palette in force: the theme's over the light or dark look)
                let p = omsi_ui::theme::th();
                LoadingLook {
                    bg: rgb(t.loading.background.unwrap_or(p.bg)),
                    text: rgb(t.loading.text.unwrap_or(p.ink_2)),
                    ink: rgb(p.ink),
                    accent: rgb(p.accent),
                    track: rgb(p.surface_2),
                    stripes: (rgb(p.accent), rgb(p.accent.lighten(0.18))),
                }
            }
        }
    }
}

impl Ui {
    /// The loading screen: the openOMSI wordmark large in the middle and the map's name under
    /// it; along the bottom a square amber bar whose stripes slide on (it is working), over
    /// it a rotating tip on the left and how far on the right. All of it `opacity` (fading
    /// over the first pictures of the session); `clock` moves the stripes and the tips.
    /// An interface theme (`themes`) gives its colours and its logo, which may itself be the
    /// bar: drawn along its path, or filled from the left, as far as the loading is.
    #[allow(clippy::too_many_arguments)]
    pub fn loading_screen(&mut self, r: &Renderer, scene: &mut Scene, width: f32, height: f32, scale: f32, title: &str, progress: f32, opacity: f32, clock: f32, tip: (&str, f32)) {
        let s = scale.max(0.5);
        let k = (opacity.clamp(0.0, 1.0) * 16.0).round() / 16.0;
        if k <= 0.0 {
            return;
        }
        let theme = crate::themes::current();
        let look = LoadingLook::of(theme.as_deref());
        let with = |c: [u8; 3], a: u8| [c[0], c[1], c[2], a];
        let a = |v: f32| (v * k).round().clamp(0.0, 255.0) as u8;
        let p = progress.clamp(0.0, 1.0);
        let bg = self.text.solid(r, scene, with(look.bg, a(255.0)));
        scene.overlays.push((bg, [0.0, 0.0, width, height]));
        let ka = a(255.0);
        let fade = move |mut px: Vec<u8>| {
            for q in px.chunks_mut(4) {
                q[3] = (q[3] as u32 * ka as u32 / 255) as u8;
            }
            px
        };
        // the logo, large, a little above the middle: openOMSI's wordmark, or the theme's
        let theme_logo = theme.as_ref().and_then(|t| t.loading.logo.as_ref().map(|l| (t.id.as_str(), l, t.loading.progress)));
        let (logo, lw, lh, logo_is_bar) = match theme_logo {
            Some((id, l, mode)) => {
                let lw = (560.0 * s).min(width * 0.6).min(height * 0.4 * l.aspect()).round().max(1.0) as u32;
                let lh = (lw as f32 / l.aspect()).round().max(1.0) as u32;
                // (the progress in hundredths: a new picture of the logo now and then, not
                // every frame)
                let bar = mode != crate::themes::Progress::Bar;
                let q = if bar { (p * 100.0).round() / 100.0 } else { 1.0 };
                // its marks pop in as the loading reaches them (in twentieths of their size: a
                // new picture of the logo a few times a pop)
                let marks = l.marks();
                self.logo_marks.retain(|m, _| marks.iter().any(|k| k.id == *m && p >= k.at));
                for m in marks.iter().filter(|m| p >= m.at) {
                    self.logo_marks.entry(m.id.clone()).or_insert(clock);
                }
                let stages: Vec<Stage> = marks
                    .iter()
                    .map(|m| match Stage::of(m, self.logo_marks.get(&m.id).map(|t| clock - t)) {
                        Stage::Scale(x) => Stage::Scale((x * 20.0).round() / 20.0),
                        s => s,
                    })
                    .collect();
                let staged: String = stages.iter().map(|s| match s { Stage::Hidden => "h".to_string(), Stage::Shown => "s".to_string(), Stage::Scale(x) => format!("{x}") }).collect::<Vec<_>>().join(",");
                let key = (format!("\u{0}theme-logo {id} {mode:?} {q} {staged}"), lw, [255, 255, 255, ka]);
                let stage = |m: &omsi_ui::icons::Mark| marks.iter().position(|k| k.id == m.id).map_or(Stage::Shown, |i| stages[i]);
                let tex = self.text.cached(r, scene, key, (lw, lh), || fade(l.pixels(lw, lh, bar.then_some((mode, q)), &stage).unwrap_or_else(|| vec![0; (lw * lh * 4) as usize])));
                (tex, lw, lh, bar)
            }
            None => {
                let lw = (560.0 * s).min(width * 0.6).round() as u32;
                let lh = (lw as f32 * 303.0 / 1142.0).round() as u32;
                let tex = self.text.cached(r, scene, ("\u{0}wordmark".to_string(), lw, [255, 255, 255, ka]), (lw, lh), || {
                    fade(omsi_ui::icons::rasterize_rgba(WORDMARK, lw, lh).unwrap_or_else(|| vec![0; (lw * lh * 4) as usize]))
                });
                (tex, lw, lh, false)
            }
        };
        let lx = ((width - lw as f32) * 0.5).round();
        let ly = (height * 0.42 - lh as f32 * 0.5).round();
        scene.overlays.push((logo, [lx, ly, lx + lw as f32, ly + lh as f32]));
        // the map's name under it
        let tpx = (24.0 * s).round() as u32;
        let title = clip_to(&self.text, title, tpx as f32, width - 64.0 * s);
        let t = self.text.label_op(r, scene, &title, tpx, with(look.text, 0), k);
        let tx = ((width - t.w as f32) * 0.5).round();
        let ty = (ly + lh as f32 + 18.0 * s).round();
        scene.overlays.push((t.tex, [tx, ty, tx + t.w as f32, ty + t.h as f32]));
        // the bar along the bottom: square, the track and the striped fill (none where the
        // logo is the bar)
        let margin = (48.0 * s).round();
        let bw = (width - margin * 2.0).min(1100.0 * s).round();
        let bx = ((width - bw) * 0.5).round();
        let bh = (14.0 * s).round().max(6.0);
        let by = (height - margin - bh).round();
        if !logo_is_bar {
            let track = self.text.solid(r, scene, with(look.track, a(255.0)));
            scene.overlays.push((track, [bx, by, bx + bw, by + bh]));
            let fill_w = (bw * p).round() as u32;
            if fill_w > 0 {
                // (tiles of one period with the stripes moved on, a cut one at the end: the
                // overlays cannot move a texture's pixels, so the moving is in which tile is drawn)
                let period = (bh as u32 * 2).max(8);
                let phase = ((clock * 40.0 * s) as u32) % period;
                let (h, fa) = (bh as u32, a(255.0));
                let (dark, light) = look.stripes;
                let mut x = 0u32;
                while x < fill_w {
                    let w = period.min(fill_w - x);
                    let key = (format!("\u{0}stripes{period}x{h}p{phase}{light:?}"), w, with(dark, fa));
                    let tex = self.text.cached(r, scene, key, (w, h), || stripe_pixels(w, h, period, phase, fa, dark, light));
                    scene.overlays.push((tex, [bx + x as f32, by, bx + (x + w) as f32, by + bh]));
                    x += w;
                }
            }
        }
        // over the bar: a tip on the left, how far on the right
        let small = (11.0 * s).round() as u32 | BOLD;
        let tip_px = (14.0 * s).round() as u32;
        let pct = self.text.label_op(r, scene, &format!("{:.0} %", p * 100.0), (15.0 * s).round() as u32 | BOLD, with(look.ink, 0), k);
        let row_y = by - 12.0 * s;
        scene.overlays.push((pct.tex, [bx + bw - pct.w as f32, row_y - pct.h as f32, bx + bw, row_y]));
        let (tip, tk) = tip;
        let tip = clip_to(&self.text, &omsi_ui::tr(tip), tip_px as f32, bw - pct.w as f32 - 40.0 * s);
        let tl = self.text.label_op(r, scene, &tip, tip_px, with(look.text, 0), k * tk);
        scene.overlays.push((tl.tex, [bx, row_y - tl.h as f32, bx + tl.w as f32, row_y]));
        let head = self.text.label_op(r, scene, &omsi_ui::tr("Tips").to_uppercase(), small, with(look.accent, 0), k);
        let hy = row_y - tl.h as f32 - head.h as f32 + 2.0 * s;
        scene.overlays.push((head.tex, [bx, hy, bx + head.w as f32, hy + head.h as f32]));
    }

    /// The loading screen alone, before the session's first picture.
    #[allow(clippy::too_many_arguments)]
    pub fn loading(&mut self, r: &Renderer, scene: &mut Scene, width: f32, height: f32, scale: f32, title: &str, progress: f32, clock: f32, tip: (&str, f32)) {
        self.loading_screen(r, scene, width, height, scale, title, progress, 1.0, clock, tip);
        self.text.end_frame(r, scene);
    }
}
