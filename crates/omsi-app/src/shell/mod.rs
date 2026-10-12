//! The game's own screens over the picture - the pause menu, its settings windows and lists,
//! and the photo mode - drawn with the launcher's toolkit (`launcher::ui`), so that they are
//! the launcher's widgets, type and colours rather than a look of their own.
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

use crate::launcher::ui::Ui;

pub(crate) mod pause;

/// What a click on one of the screens asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Action {
    /// Line `k` of the game menu or of the open list, as Enter on it.
    Choose(usize),
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
    pub(crate) ui: Ui,
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

impl Shell {
    pub(crate) fn new() -> Shell {
        // (the desktop launcher's calm look: graphite surfaces, no edges)
        let mut ui = Ui::new();
        ui.flat = true;
        Shell { ui, gpu: None, shot_gpu: None, drawn: None, actions: Vec::new(), picture: None, picture_tex: None, logo_tex: None, shot_logo: None, scale: 1.0, over_ui: false, last_sel: None, dd_anchor: None, opaque: false }
    }

    /// A frame of the screens begins: the window's size in physical pixels and how many of
    /// them make a point.
    pub(crate) fn begin(&mut self, width: f32, height: f32, scale: f32, dt: f32) {
        self.scale = scale.max(0.25);
        self.ui.input.touch = crate::platform::touch_controls();
        self.ui.begin(Vec2::new(width / self.scale, height / self.scale), self.scale, dt);
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

    /// The screens over the window's picture (`view`, `w` x `h` pixels).
    pub(crate) fn render(&mut self, r: &Renderer, view: &wgpu::TextureView, w: u32, h: u32) {
        let Some(d) = self.drawn.as_ref() else { return };
        let format = r.format();
        if self.gpu.as_ref().map(|g| g.1 != format).unwrap_or(true) {
            self.gpu = Some((Gpu::new(&r.device, format, 1, self.ui.atlas.size), format));
            self.ui.atlas.mark_all_dirty();
            self.picture_tex = None;
            self.logo_tex = None;
        }
        let Some((gpu, _)) = self.gpu.as_mut() else { return };
        if self.logo_tex.is_none() {
            self.logo_tex = logo_pixels().map(|px| gpu.add_image(&r.device, &r.queue, LOGO_PX, LOGO_PX, &px));
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
