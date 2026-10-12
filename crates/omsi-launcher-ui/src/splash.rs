//! The game's pictures drawn the way the launcher draws: its painter, its fonts, its ground and
//! openOMSI's mark, rendered into a texture the frame shows. The loading screen (Luc: the mark
//! on a plain grey ground, its line the bar of what is loaded) and the mark at the top of the
//! pause menu.

use std::time::Instant;

use glam::Vec2;
use omsi_render::{Renderer, Scene, TextureId};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Draw, Gpu, Rect, Weight};

use crate::intro::Loading;
use crate::ui::Ui;

/// The loading screen's ground: a plain dark grey.
const GROUND: Color = Color::rgba(46, 48, 53, 1.0);

/// A texture the painter draws into: where it is, its size.
type Target = (TextureId, u32, u32);

pub struct Splash {
    ui: Ui,
    gpu: Option<Gpu>,
    /// The loading screen's picture, and the pause menu's mark (with the accent and size it
    /// was drawn for).
    screen: Option<Target>,
    mark_tex: Option<(Target, u32)>,
    loading: Loading,
    /// The bar as shown (it eases towards what is loaded and never goes back), and the clock.
    shown: f32,
    last: Instant,
    /// The screen's own clock: the stops spring up and the light breathes in real time. (The
    /// painter's clock takes a tenth of a second a frame at most, and a frame of loading
    /// often takes longer: a stop the bar had passed stayed small, Luc.)
    started: Instant,
}

impl Splash {
    pub fn new() -> Splash {
        Splash { ui: Ui::new(), gpu: None, screen: None, mark_tex: None, loading: Loading::default(), shown: 0.0, last: Instant::now(), started: Instant::now() }
    }

    /// The loading screen over the whole window (`width` x `height` pixels, `scale` the
    /// interface's): a plain grey ground, openOMSI's mark in the middle with its
    /// ring and line drawn as far as `progress` (0..1) has loaded, the map's `title` under it
    /// and `caption` (what is being done) under that.
    #[allow(clippy::too_many_arguments)]
    pub fn loading(&mut self, r: &Renderer, scene: &mut Scene, width: f32, height: f32, scale: f32, title: &str, caption: &str, progress: f32) {
        let (pw, ph) = (width.max(1.0) as u32, height.max(1.0) as u32);
        let s = scale.max(0.5);
        let size = Vec2::new(pw as f32, ph as f32) / s;
        let now = Instant::now();
        // (in real time: a frame of loading may take a second, and the bar glides on as far
        // in it - the painter's own clock takes a tenth at most)
        let dt = now.duration_since(self.last).as_secs_f32().min(1.0);
        self.last = now;
        // (the bar eases to what is loaded: a big step glides, and it never goes back)
        let want = progress.clamp(0.0, 1.0).max(self.shown);
        self.shown += (want - self.shown) * (1.0 - (-dt * 6.0).exp());
        if want - self.shown < 0.002 {
            self.shown = want;
        }
        self.ui.begin(size, s, dt);
        let time = self.started.elapsed().as_secs_f32();
        paint_loading(&mut self.ui, &mut self.loading, self.shown, title, caption, time);
        if let Some(tex) = self.render(r, scene, Which::Screen, (pw, ph)) {
            scene.overlays.push((tex, [0.0, 0.0, width, height]));
        }
    }

    /// openOMSI's mark at rest for the pause menu, `w` pixels wide (its height as the mark
    /// has it): drawn again only when the size or the accent changed.
    pub fn mark(&mut self, r: &Renderer, scene: &mut Scene, w: f32, scale: f32) -> Option<(TextureId, f32, f32)> {
        let s = scale.max(0.5);
        // (the mark is about 3.3 times as wide as high)
        let (pw, ph) = (w.max(16.0) as u32, (w / 3.0).max(8.0) as u32);
        let accent = crate::accent::chosen();
        if let Some(((tex, tw, th), a)) = self.mark_tex {
            if (tw, th) == (pw, ph) && a == accent {
                return Some((tex, pw as f32, ph as f32));
            }
        }
        let size = Vec2::new(pw as f32, ph as f32) / s;
        let ui = &mut self.ui;
        ui.begin(size, s, 0.0);
        // (a window the mark's size: `fit` makes it fill it; drawn whole, nothing to wait for)
        let time = ui.time;
        Loading::at_rest().draw_in(ui, Rect::new(0.0, 0.0, size.x, size.y), 1.0, time);
        let tex = self.render(r, scene, Which::Mark, (pw, ph))?;
        self.mark_tex = Some(((tex, pw, ph), accent));
        Some((tex, pw as f32, ph as f32))
    }

    /// The loading screen is over: its picture is let go.
    pub fn release(&mut self, r: &Renderer, scene: &mut Scene) {
        if let Some((t, _, _)) = self.screen.take() {
            r.free_texture(scene, t);
            scene.premultiplied.remove(&t);
        }
        self.loading = Loading::default();
        self.shown = 0.0;
        self.started = Instant::now();
    }

    /// What was painted this frame into the texture `which` (made, or made anew at `size`).
    fn render(&mut self, r: &Renderer, scene: &mut Scene, which: Which, size: (u32, u32)) -> Option<TextureId> {
        let (layers, verts, ranges) = self.ui.finish();
        if self.gpu.is_none() {
            let samples = if r.format().guaranteed_format_features(wgpu::Features::empty()).flags.sample_count_supported(4) { 4 } else { 1 };
            self.gpu = Some(Gpu::new(&r.device, r.format(), samples, self.ui.atlas.size));
        }
        let mut fresh: Option<Target> = None;
        let slot = match which {
            Which::Screen => &mut self.screen,
            Which::Mark => {
                // (the mark's texture is made anew with every drawing: a size or an accent)
                if let Some(((t, _, _), _)) = self.mark_tex.take() {
                    r.free_texture(scene, t);
                    scene.premultiplied.remove(&t);
                }
                &mut fresh
            }
        };
        if slot.as_ref().is_some_and(|(_, w, h)| (*w, *h) != size) {
            let (t, _, _) = slot.take().unwrap();
            r.free_texture(scene, t);
            scene.premultiplied.remove(&t);
        }
        let (tex, _, _) = *slot.get_or_insert_with(|| {
            let t = r.add_render_texture(scene, size.0, size.1);
            scene.premultiplied.insert(t);
            (t, size.0, size.1)
        });
        let view = r.texture_view(scene, tex)?;
        let gpu = self.gpu.as_mut()?;
        gpu.upload(&r.device, &r.queue, 0, &verts);
        gpu.upload_atlas(&r.queue, &mut self.ui.atlas);
        let draws: Vec<Draw> = ranges.iter().enumerate().map(|(k, (range, t))| Draw { buffer: 0, range: range.clone(), layer: k, texture: *t }).collect();
        let mut enc = r.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("splash") });
        gpu.render(&r.device, &r.queue, &mut enc, &view, size, Some(wgpu::Color::TRANSPARENT), &layers, &draws);
        r.queue.submit([enc.finish()]);
        Some(tex)
    }
}

/// The loading screen painted over the window `ui` has: the ground, the mark with its pen at
/// `shown` (on the clock `time`, s), the title and the caption with how much is loaded. (The
/// launcher paints it too for pictures: `OMSI_LAUNCHER_SPLASH=<0..1>`.)
pub fn paint_loading(ui: &mut Ui, loading: &mut Loading, shown: f32, title: &str, caption: &str, time: f32) {
    let size = ui.size;
    let window = Rect::new(0.0, 0.0, size.x, size.y);
    // (a plain grey ground, Luc: calmer than the launcher's route behind the mark)
    ui.p().rect(window, GROUND);
    let centre = Vec2::new(size.x * 0.5, size.y * 0.46);
    let mark = loading.draw(ui, centre, shown, time);
    let mut y = mark.bottom() + 30.0;
    if !title.trim().is_empty() {
        ui.text_in(title.trim(), Rect::new(0.0, y, size.x, 34.0), 26.0, Weight::Bold, Color::rgba(236, 238, 242, 1.0), Align::Center);
        y += 40.0;
    }
    let pct = format!("{:.0} %", (shown * 100.0).floor());
    let line = if caption.trim().is_empty() { pct } else { format!("{}  ·  {pct}", caption.trim()) };
    ui.text_in(&line, Rect::new(0.0, y, size.x, 20.0), 13.0, Weight::Medium, Color::rgba(149, 157, 176, 1.0), Align::Center);
}

thread_local! {
    /// The launcher's picture of the loading screen (`OMSI_LAUNCHER_SPLASH`).
    static PREVIEW: std::cell::RefCell<Loading> = std::cell::RefCell::default();
}

/// `OMSI_LAUNCHER_SPLASH=<0..1>`: the launcher paints the game's loading screen at that point
/// over its page, for pictures of it (the game itself is not started for them).
pub fn preview(ui: &mut Ui) {
    let Some(p) = omsi_cfg::env::var("OMSI_LAUNCHER_SPLASH").ok().and_then(|v| v.trim().parse::<f32>().ok()) else { return };
    PREVIEW.with(|l| {
        let mut l = l.borrow_mut();
        l.settled = true;
        let time = ui.time;
        paint_loading(ui, &mut l, p.clamp(0.0, 1.0), "Berlin 186", "", time);
    });
    // (frame after frame, as the game draws it: the stops and the bus front spring up)
    ui.keep_moving();
}

#[derive(Clone, Copy)]
enum Which {
    Screen,
    Mark,
}
