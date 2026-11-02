//! The game's own screens over the picture - the pause menu, its settings windows and lists,
//! and the photo mode - drawn with the widget kit (`omsi_ui::kit`), so that they have the
//! launcher's widgets, type and colours rather than a look of their own.
//!
//! The toolkit is immediate-mode: every frame the screen is drawn anew from the game's state
//! and the widgets answer what was clicked. What the game is to do about it is collected as
//! [`Action`]s and carried out at the start of the next frame (`App::apply_shell_actions`),
//! where the game has its event loop at hand. The keyboard keeps its own way through the
//! menus (`App::menu_key`); the mouse, the wheel and the fingers come here.

use std::ops::Range;

use glam::Vec2;
use omsi_render::Renderer;
use omsi_ui::{Draw, Gpu, Layer, Vertex};

use omsi_ui::kit::Ui;

pub(crate) mod pause;

/// What a click on one of the screens asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Action {
    ProcessPermission(u64, omsi_plugin::api::process::ProcessDecision),
    /// Line `k` of the game menu or of the open list, as Enter on it.
    Choose(usize),
    /// Switch the presentation category without changing the underlying actions.
    PauseCategory(pause::navigation::Category),
    /// Edit presentation-only shortcuts without executing the underlying action.
    QuickActions(pause::quick_actions::Edit),
    /// A line of the pause menu's rail chosen while a list is open over it: the list closes
    /// and the line is done.
    Rail(usize),
    /// The control of line `k` of a settings window: a slider `fx` of the way along, a
    /// stepper's left (`fx` < 0.5) or right half.
    Control(usize, f32),
    /// Page `i` of the open settings window, or (one past the last) the way back.
    Side(usize),
    /// A click in the timetable beside a line's tours (see `App::tour_pane_click`).
    Pane(usize),
    /// Entry `i` of the open drop-down.
    Dropdown(usize),
    /// The open drop-down closes without a choice.
    CloseDropdown,
    /// A request of the photo mode's panel.
    Photo(crate::photo::Request),
}

/// The screens' toolkit, their graphics and what was drawn last.
pub(crate) struct Shell {
    pub(crate) process_prompt: Option<u64>,
    pub(crate) process_dialog: crate::process_permissions::DialogState,
    pub(crate) ui: Ui,
    pub(crate) pause_category: pause::navigation::Category,
    pub(crate) quick_edit: bool,
    /// No category chosen: the pause menu shows only the rail over the dimmed paused game.
    pub(crate) pause_hidden: bool,
    pub(crate) quick_focus: usize,
    gpu: Option<(Gpu, wgpu::TextureFormat)>,
    shot_gpu: Option<Gpu>,
    drawn: Option<Drawn>,
    /// What the clicks of the last frame ask of the game.
    pub(crate) actions: Vec<Action>,
    /// A picture to show under the screens (the photo mode's developed photo), to be sent
    /// to the graphics card with the next frame: its size and pixels.
    picture: Option<(u32, u32, Vec<u8>)>,
    /// The picture's texture once it is on the card.
    pub(crate) picture_tex: Option<usize>,
    /// The openOMSI logo (the launcher's sidebar has it) on the card, and in the graphics of
    /// the `shot` pictures.
    pub(crate) logo_tex: Option<usize>,
    shot_logo: Option<usize>,
    /// The interface theme whose logo `logo_tex` is (none: openOMSI's), and the name it shows
    /// beside it (none: "openOMSI").
    logo_of: Option<String>,
    pub(crate) logo_title: Option<String>,
    /// A theme's logo that is a wordmark (wider than `WIDE` times its height): its width over
    /// its height. The rail then shows it across its top in place of the icon and the name.
    pub(crate) logo_wide: Option<f32>,
    /// A glass theme's picture behind the screens (`capture_backdrop`).
    glass: Option<GlassPicture>,
    /// When the glass's picture was last taken (none: it is to be taken; see `glass_due`).
    glass_taken: Option<std::time::Instant>,
    /// Physical pixels per point of the last frame.
    pub(crate) scale: f32,
    /// The mouse was over a panel of the screens in the last frame.
    pub(crate) over_ui: bool,
    /// The line the keyboard chose last frame (to scroll it into view when it changes).
    pub(crate) last_sel: Option<(u64, usize)>,
    /// Where the open drop-down's field was drawn last (its list lies under it).
    pub(crate) dd_anchor: Option<omsi_ui::Rect>,
    /// The screens cover the whole window (the photo mode's photo): it is cleared first.
    pub(crate) opaque: bool,
}

struct Drawn {
    layers: Vec<Layer>,
    verts: Vec<Vertex>,
    ranges: Vec<(Range<u32>, usize)>,
}

/// A glass theme's picture behind the screens: the window's picture copied on the graphics card
/// and blurred there in three small passes, never read back (`Shell::capture_backdrop`).
struct GlassPicture {
    /// The copy, and its number among the screens' textures.
    copy: (wgpu::Texture, usize),
    /// Each pass's picture and its number: a quarter of the size, `factor`th, twice that.
    passes: Vec<(wgpu::Texture, usize)>,
    factor: u32,
}

/// The vertex buffer of the glass's passes (0 is the screens').
const GLASS_BUFFER: usize = 1;
/// How often a second the glass's picture is taken while the game goes on behind the menu.
const GLASS_RATE: f32 = 30.0;

impl Shell {
    pub(crate) fn new() -> Shell {
        // (the desktop launcher's calm look: graphite surfaces, no edges)
        let mut ui = Ui::new();
        ui.flat = true;
        Shell { process_prompt: None, process_dialog: Default::default(), ui, pause_category: Default::default(), quick_edit: false, pause_hidden: true, quick_focus: 0, gpu: None, shot_gpu: None, drawn: None, actions: Vec::new(), picture: None, picture_tex: None, logo_tex: None, shot_logo: None, logo_of: None, logo_title: None, logo_wide: None, glass: None, glass_taken: None, scale: 1.0, over_ui: false, last_sel: None, dd_anchor: None, opaque: false }
    }

    /// A frame of the screens begins: the window's size in physical pixels and how many of
    /// them make a point.
    pub(crate) fn begin(&mut self, width: f32, height: f32, scale: f32, dt: f32) {
        // Keep pointer capture in the same coordinate space until its release is handled.
        let captured = self.ui.active.is_some() && (self.ui.input.down || self.ui.input.released);
        let next_scale = if captured { self.scale } else { scale.max(0.25) };
        if next_scale != self.scale {
            self.ui.input.mouse = self.ui.input.mouse * self.scale / next_scale;
        }
        self.scale = next_scale;
        self.ui.input.touch = crate::platform::touch_controls();
        self.ui.begin(Vec2::new(width / self.scale, height / self.scale), self.scale, dt);
        self.ui.backdrop = self.glass.as_ref().and_then(|g| g.passes.last()).map(|p| p.1);
        let theme = crate::themes::current();
        self.logo_title = theme.as_ref().and_then(|t| t.title.clone());
        self.logo_wide = theme.as_ref().and_then(|t| t.logo.as_ref()).map(|l| l.aspect()).filter(|a| *a >= WIDE);
    }

    /// The frame of the screens is drawn: its layers kept for `render`.
    pub(crate) fn finish(&mut self) {
        self.over_ui = self.ui.over_ui;
        let (layers, verts, ranges) = self.ui.finish();
        self.drawn = Some(Drawn { layers, verts, ranges });
    }

    /// Nothing of the screens shows this frame.
    pub(crate) fn clear(&mut self) {
        self.drawn = None;
        self.over_ui = false;
        self.ui.discard_input();
        self.ui.input.down = false;
        self.ui.active = None;
    }

    pub(crate) fn showing(&self) -> bool {
        self.drawn.is_some()
    }

    /// The photo to show under the screens (RGBA, sRGB), sent with the next frame.
    pub(crate) fn set_picture(&mut self, w: u32, h: u32, rgba: Vec<u8>) {
        self.picture = Some((w, h, rgba));
    }

    /// The photo is no longer shown.
    pub(crate) fn drop_picture(&mut self) {
        self.picture = None;
        self.picture_tex = None;
    }

    // --- the mouse, in physical pixels of the window --------------------------------------

    pub(crate) fn pointer(&mut self, x: f32, y: f32) {
        self.ui.input.mouse = Vec2::new(x, y) / self.scale;
    }

    pub(crate) fn onto_slider(&self, start: Vec2, p: Vec2) -> Option<Vec2> {
        self.ui.onto_slider(start / self.scale, p / self.scale).filter(|_| self.showing() && self.dd_anchor.is_none()).map(|q| q * self.scale)
    }

    pub(crate) fn button(&mut self, pressed: bool) {
        log::debug!("screens: button {pressed} at {:?}", self.ui.input.mouse);
        if pressed {
            self.ui.input.pressed = true;
            self.ui.input.down = true;
        } else {
            self.ui.input.released = true;
            self.ui.input.down = false;
        }
    }

    /// The wheel, in lines (up positive).
    pub(crate) fn wheel(&mut self, lines: f32) {
        self.ui.input.wheel.y += lines;
    }

    // --- drawing ---------------------------------------------------------------------------

    fn upload(gpu: &mut Gpu, r: &Renderer, ui: &mut Ui, d: &Drawn) {
        gpu.upload(&r.device, &r.queue, 0, &d.verts);
        gpu.upload_atlas(&r.queue, &mut ui.atlas);
    }

    /// The screens' graphics, made for the renderer's format (anew when it changed: the
    /// textures on the old ones are made again).
    fn ensure_gpu(&mut self, r: &Renderer) {
        let format = r.format();
        if self.gpu.as_ref().map(|g| g.1 != format).unwrap_or(true) {
            self.gpu = Some((Gpu::new(&r.device, format, 1, self.ui.atlas.size), format));
            self.ui.atlas.mark_all_dirty();
            self.picture_tex = None;
            self.logo_tex = None;
            self.glass = None;
        }
    }

    /// A glass theme's picture behind the screens, from the window's picture `src` as the
    /// scene left it (before the screens are drawn over it): copied on the graphics card and
    /// blurred there by about `blur` pixels - down to a quarter, then to a `factor`th, then up
    /// to twice that, each pass four taps a texel of the picture before apart, averaged (a box
    /// of four at each step). Nothing is read back and nothing waits for the graphics card: a
    /// copy and three passes into pictures of a sixteenth of the window and less.
    /// Whether the glass's picture is to be taken this frame: when the menu has just opened,
    /// and while the game goes on behind it (not `paused`) at most `GLASS_RATE` times a second
    /// - a paused picture does not change, and the blur hides a frame's movement anyway.
    pub(crate) fn glass_due(&self, paused: bool) -> bool {
        match self.glass_taken {
            None => true,
            Some(t) => !paused && t.elapsed().as_secs_f32() >= 1.0 / GLASS_RATE,
        }
    }

    /// The menu with the glass is closed: its picture is taken anew when it opens again.
    pub(crate) fn glass_closed(&mut self) {
        self.glass_taken = None;
    }

    pub(crate) fn capture_backdrop(&mut self, r: &Renderer, src: &wgpu::Texture, blur: f32) {
        self.glass_taken = Some(std::time::Instant::now());
        self.ensure_gpu(r);
        let Some((gpu, format)) = self.gpu.as_mut() else { return };
        let format = *format;
        let size = wgpu::Extent3d { depth_or_array_layers: 1, ..src.size() };
        let (w, h) = (size.width.max(1), size.height.max(1));
        let factor = ((blur * 0.5).round().max(1.0) as u32).next_power_of_two().clamp(8, 32);
        let stale = self.glass.as_ref().is_none_or(|g| g.copy.0.size() != size || g.copy.0.format() != src.format() || g.factor != factor);
        if stale {
            if let Some(old) = self.glass.take() {
                gpu.free(old.copy.1);
                for p in old.passes {
                    gpu.free(p.1);
                }
            }
            let make = |label: &str, (tw, th): (u32, u32), format: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
                r.device.create_texture(&wgpu::TextureDescriptor { label: Some(label), size: wgpu::Extent3d { width: tw.max(1), height: th.max(1), depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[] })
            };
            let copy = make("glass: the picture", (w, h), src.format(), wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING);
            let copy_id = gpu.add_view(&r.device, &copy.create_view(&Default::default()), (w, h));
            let passes = [4, factor, factor / 2]
                .into_iter()
                .map(|k| {
                    let s = ((w / k).max(1), (h / k).max(1));
                    let t = make("glass: a pass", s, format, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING);
                    let id = gpu.add_view(&r.device, &t.create_view(&Default::default()), s);
                    (t, id)
                })
                .collect();
            self.glass = Some(GlassPicture { copy: (copy, copy_id), passes, factor });
        }
        let Some(g) = self.glass.as_ref() else { return };
        let mut enc = r.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("glass: copy") });
        enc.copy_texture_to_texture(src.as_image_copy(), g.copy.0.as_image_copy(), size);
        r.queue.submit([enc.finish()]);
        let mut from = (g.copy.1, w, h);
        // (a texel apart going down, half a texel coming up)
        for ((t, id), apart) in g.passes.iter().zip([1.0f32, 1.0, 0.5]) {
            let (pw, ph) = (t.width(), t.height());
            let mut p = omsi_ui::Painter::new();
            let (du, dv) = (apart / from.1 as f32, apart / from.2 as f32);
            // (each tap over the ones before at 1/n of its weight: their mean)
            for (n, (ox, oy)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)].into_iter().enumerate() {
                let s = omsi_ui::Sprite { uv: [ox * du, oy * dv, 1.0 + ox * du, 1.0 + oy * dv], w: pw as f32, h: ph as f32, ascent: 0.0 };
                p.sprite(s, Vec2::ZERO, Vec2::new(pw as f32, ph as f32), omsi_ui::Color([1.0, 1.0, 1.0, 1.0 / (n as f32 + 1.0)]));
            }
            // (one pass a submission: the layers' uniforms and the vertices are written for it)
            gpu.upload(&r.device, &r.queue, GLASS_BUFFER, &p.verts);
            let layer = Layer::flat([0.0, 0.0, pw as f32, ph as f32], 0.0, 1.0);
            let view = t.create_view(&Default::default());
            let mut enc = r.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("glass: a pass") });
            gpu.render(&r.device, &r.queue, &mut enc, &view, (pw, ph), Some(wgpu::Color::TRANSPARENT), &[layer], &[Draw { buffer: GLASS_BUFFER, range: 0..p.len(), layer: 0, texture: from.0 }]);
            r.queue.submit([enc.finish()]);
            from = (*id, pw, ph);
        }
    }

    /// The screens over the window's picture (`view`, `w` x `h` pixels).
    pub(crate) fn render(&mut self, r: &Renderer, view: &wgpu::TextureView, w: u32, h: u32) {
        if self.drawn.is_none() {
            return;
        }
        self.ensure_gpu(r);
        let Some(d) = self.drawn.as_ref() else { return };
        let Some((gpu, _)) = self.gpu.as_mut() else { return };
        // the logo: the interface theme's, when it has one
        let theme = crate::themes::current().filter(|t| t.logo.is_some());
        let logo_of = theme.as_ref().map(|t| t.id.clone());
        if logo_of != self.logo_of {
            if let Some(id) = self.logo_tex.take() {
                gpu.free(id);
            }
            self.logo_of = logo_of;
        }
        if self.logo_tex.is_none() {
            // (a wordmark at its own width, for the rail's top; an icon square)
            let (px, w, h) = match theme.as_ref().and_then(|t| t.logo.as_ref()) {
                Some(logo) if logo.aspect() >= WIDE => {
                    let h = (LOGO_WIDE_PX as f32 / logo.aspect()).round().max(1.0) as u32;
                    (logo.pixels(LOGO_WIDE_PX, h, None, &|_| omsi_ui::icons::Stage::Shown), LOGO_WIDE_PX, h)
                }
                Some(logo) => (logo.square_pixels(LOGO_PX), LOGO_PX, LOGO_PX),
                None => (logo_pixels(), LOGO_PX, LOGO_PX),
            };
            self.logo_tex = px.map(|px| gpu.add_image(&r.device, &r.queue, w, h, &px));
        }
        if let Some((pw, ph, px)) = self.picture.take() {
            match self.picture_tex {
                Some(id) => gpu.update_image(&r.device, &r.queue, id, pw, ph, &px),
                None => self.picture_tex = Some(gpu.add_image(&r.device, &r.queue, pw, ph, &px)),
            }
        }
        Self::upload(gpu, r, &mut self.ui, d);
        let draws: Vec<Draw> = d.ranges.iter().enumerate().map(|(k, (range, tex))| Draw { buffer: 0, range: range.clone(), layer: k, texture: *tex }).collect();
        let mut enc = r.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("game screens") });
        gpu.render(&r.device, &r.queue, &mut enc, view, (w, h), self.opaque.then_some(wgpu::Color::BLACK), &d.layers, &draws);
        r.queue.submit([enc.finish()]);
    }

    /// The screens as an RGBA picture (premultiplied) of `w` x `h`, for the `shot` pictures
    /// of an input script. The photo under them is left out (the shot has the scene).
    pub(crate) fn picture_for_shot(&mut self, r: &Renderer, w: u32, h: u32) -> Option<Vec<u8>> {
        let d = self.drawn.as_ref()?;
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let size = self.ui.atlas.size;
        if self.shot_gpu.is_none() {
            let mut g = Gpu::new(&r.device, format, 1, size);
            self.shot_logo = logo_pixels().map(|px| g.add_image(&r.device, &r.queue, LOGO_PX, LOGO_PX, &px));
            self.shot_gpu = Some(g);
        }
        let gpu = self.shot_gpu.as_mut().unwrap();
        self.ui.atlas.mark_all_dirty();
        Self::upload(gpu, r, &mut self.ui, d);
        self.ui.atlas.mark_all_dirty();
        // (the logo as well, from the shot's own copy of it)
        let (logo, shot_logo) = (self.logo_tex, self.shot_logo);
        let draws: Vec<Draw> = d.ranges.iter().enumerate().filter_map(|(k, (range, tex))| {
            let texture = if *tex == 0 { 0 } else if Some(*tex) == logo { shot_logo? } else { return None };
            Some(Draw { buffer: 0, range: range.clone(), layer: k, texture })
        }).collect();
        let tex = r.device.create_texture(&wgpu::TextureDescriptor { label: Some("screens shot"), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
        let view = tex.create_view(&Default::default());
        let mut enc = r.device.create_command_encoder(&Default::default());
        gpu.render(&r.device, &r.queue, &mut enc, &view, (w, h), Some(wgpu::Color::TRANSPARENT), &d.layers, &draws);
        Some(read_back(r, enc, &tex, w, h))
    }
}

/// The launcher's logo (`launcher/ui/logo.svg`), as the pixels of a texture of `LOGO_PX`.
const LOGO: &str = include_str!("../../../../launcher/ui/logo.svg");
// (drawn 32 interface pixels wide, twice that on a Retina screen: a texture of about that
// size, since a much larger one squeezed without mipmaps came out blurred and jagged)
const LOGO_PX: u32 = 64;
/// A theme's wordmark: as wide as this in the texture (twice the rail's width it is drawn at,
/// for a Retina screen), and how much wider than high a logo is to be one.
const LOGO_WIDE_PX: u32 = 480;
const WIDE: f32 = 1.6;

fn logo_pixels() -> Option<Vec<u8>> {
    omsi_ui::icons::rasterize_rgba(LOGO, LOGO_PX, LOGO_PX)
}

/// `tex` (RGBA8, `w` x `h`) copied out after the commands of `enc`.
pub(crate) fn read_back(r: &Renderer, mut enc: wgpu::CommandEncoder, tex: &wgpu::Texture, w: u32, h: u32) -> Vec<u8> {
    let stride = (w * 4).div_ceil(256) * 256;
    let buf = r.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (stride * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    enc.copy_texture_to_buffer(tex.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: None } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
    r.queue.submit([enc.finish()]);
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    omsi_render::wait_gpu(&r.device, None).ok();
    let data = buf.slice(..).get_mapped_range();
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h as usize {
        let row = &data[y * stride as usize..y * stride as usize + w as usize * 4];
        out[y * w as usize * 4..(y + 1) * w as usize * 4].copy_from_slice(row);
    }
    out
}
