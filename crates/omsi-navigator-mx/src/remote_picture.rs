use super::panel::PanelLook;
use super::*;
use std::sync::mpsc::{Receiver, Sender, channel};

const EVERY: f32 = 0.1;
const MAX_SIDE: u32 = 1280;
const MAX_PIXELS: u32 = 1280 * 800;
const MIN_SIDE: u32 = 64;

#[derive(Default)]
pub(crate) struct RemotePicture {
    want: Option<(u32, u32)>,
    t: f32,
    target: Option<(mx::Texture, mx::TextureView, u32, u32)>,
    targets: omsi_ui::Attachments,
    pending: Option<Readback>,
    copying: bool,
    copier: Option<(Sender<Readback>, Receiver<(mx::Buffer, RemoteFrame)>)>,
    spare: Vec<mx::Buffer>,
    done: Option<RemoteFrame>,
    warned: bool,
    pub(crate) drawing: bool,
    pub(crate) commands: Option<mx::CommandBuffer>,
    pub(crate) reach: f64,
}

struct Readback {
    buffer: mx::Buffer,
    row: u32,
    w: u32,
    h: u32,
    bgra: bool,
    /// The submission that copies the picture into `buffer`.
    submission: mx::SubmissionIndex,
}

pub struct RemoteFrame {
    pub width: u32,
    pub height: u32,
    pub row: u32,
    pub bgra: bool,
    pub data: Vec<u8>,
}

pub(crate) fn fit(w: u32, h: u32) -> (u32, u32) {
    let (w, h) = (w.max(1) as f64, h.max(1) as f64);
    let k = (MAX_SIDE as f64 / w)
        .min(MAX_SIDE as f64 / h)
        .min((MAX_PIXELS as f64 / (w * h)).sqrt())
        .min(1.0);
    (
        ((w * k).round() as u32).max(MIN_SIDE),
        ((h * k).round() as u32).max(MIN_SIDE),
    )
}

fn copier() -> Option<(Sender<Readback>, Receiver<(mx::Buffer, RemoteFrame)>)> {
    let (jobs, inbox) = channel::<Readback>();
    let (outbox, back) = channel();
    std::thread::Builder::new()
        .name("navigator remote picture".into())
        .spawn(move || {
            for r in inbox {
                let Ok(data) = r.buffer.map_read_now(0..r.buffer.size(), |b| b.to_vec()) else {
                    continue;
                };
                if outbox
                    .send((
                        r.buffer,
                        RemoteFrame {
                            width: r.w,
                            height: r.h,
                            row: r.row,
                            bgra: r.bgra,
                            data,
                        },
                    ))
                    .is_err()
                {
                    break;
                }
            }
        })
        .ok()?;
    Some((jobs, back))
}

impl Navigator {
    pub fn want_remote_picture(&mut self, size: Option<(u32, u32)>) {
        let size = size.map(|(w, h)| fit(w, h));
        if size.is_none() {
            self.remote_pic.target = None;
            self.remote_pic.targets = Default::default();
            self.remote_pic.reach = 0.0;
            self.remote_pic.spare.clear();
        }
        self.remote_pic.want = size;
    }

    pub fn take_remote_picture(&mut self) -> Option<RemoteFrame> {
        self.remote_pic.done.take()
    }

    fn poll_remote_picture(&mut self, device: &mx::Device) {
        if let Some((_, back)) = self.remote_pic.copier.as_ref() {
            if let Ok((buffer, frame)) = back.try_recv() {
                self.remote_pic.copying = false;
                if self.remote_pic.want.is_some() && self.remote_pic.spare.len() < 2 {
                    self.remote_pic.spare.push(buffer);
                }
                self.remote_pic.done = Some(frame);
            }
        }
        let Some(r) = self.remote_pic.pending.as_ref() else {
            return;
        };
        if !device.is_done(r.submission) {
            if device.poll().is_err() {
                self.remote_pic.pending = None;
                return;
            }
            if !device.is_done(r.submission) {
                return;
            }
        }
        let r = self.remote_pic.pending.take().unwrap();
        if self.remote_pic.copier.is_none() {
            self.remote_pic.copier = copier();
        }
        self.remote_pic.copying = self
            .remote_pic
            .copier
            .as_ref()
            .is_some_and(|(jobs, _)| jobs.send(r).is_ok());
    }

    pub(super) fn remote_picture_frame(&mut self, renderer: &Renderer, f: &NavFrame) {
        self.poll_remote_picture(&renderer.device);
        let Some((w, h)) = self.remote_pic.want else {
            return;
        };
        self.remote_pic.t -= f.dt;
        if self.remote_pic.t > 0.0 || self.remote_pic.pending.is_some() || self.remote_pic.copying {
            return;
        }
        self.remote_pic.t = EVERY;
        let format = renderer.format();
        let bgra = match format {
            mx::TextureFormat::Bgra8Unorm | mx::TextureFormat::Bgra8UnormSrgb => true,
            mx::TextureFormat::Rgba8Unorm | mx::TextureFormat::Rgba8UnormSrgb => false,
            other => {
                if !self.remote_pic.warned {
                    self.remote_pic.warned = true;
                    log::warn!(
                        "navigator: no picture for the remote cockpit from a {other:?} window"
                    );
                }
                return;
            }
        };
        let device = &renderer.device;
        if self.gpu.is_none() {
            self.gpu = Some(Gpu::with_depth(
                device,
                format,
                map_samples(renderer, format),
                self.atlas.size,
            ));
        }
        if self
            .remote_pic
            .target
            .as_ref()
            .is_none_or(|t| (t.2, t.3) != (w, h))
        {
            let texture = device
                .create_texture(&mx::TextureDescriptor {
                    label: Some("navigator remote picture"),
                    size: mx::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: mx::TextureDimension::D2,
                    format,
                    usage: mx::TextureUsages::RENDER_ATTACHMENT | mx::TextureUsages::COPY_SRC,
                    view_formats: &[],
                })
                .expect("mX: a texture");
            let view = texture
                .create_view(&Default::default())
                .expect("mX: a texture view");
            self.remote_pic.target = Some((texture, view, w, h));
        }
        renderer.queue.submit(renderer.take_deferred_commands());
        self.remote_pic.drawing = true;
        let mut targets = std::mem::take(&mut self.remote_pic.targets);
        let view = self
            .remote_pic
            .target
            .as_ref()
            .map(|t| t.1.clone())
            .unwrap();
        let look = PanelLook::remote((w, h));
        self.draw(
            renderer,
            &view,
            (w, h),
            h as f32,
            &look,
            Some(&mut targets),
            f,
        );
        self.remote_pic.targets = targets;
        self.remote_pic.drawing = false;
        let Some(drawn) = self.remote_pic.commands.take() else {
            return;
        };
        let row =
            (w * 4).div_ceil(mx::COPY_BYTES_PER_ROW_ALIGNMENT) * mx::COPY_BYTES_PER_ROW_ALIGNMENT;
        let size = (row * h) as u64;
        self.remote_pic.spare.retain(|b| b.size() == size);
        let buffer = self.remote_pic.spare.pop().unwrap_or_else(|| {
            device
                .create_buffer(&mx::BufferDescriptor {
                    label: Some("navigator remote picture"),
                    size,
                    usage: mx::BufferUsages::COPY_DST | mx::BufferUsages::MAP_READ,
                })
                .expect("mX: a buffer")
        });
        let mut enc = device.create_command_encoder(Some("navigator remote picture"));
        let texture = &self.remote_pic.target.as_ref().unwrap().0;
        enc.copy_texture_to_buffer(
            texture.as_image_copy(),
            mx::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: mx::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            mx::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let submission = renderer.queue.submit([drawn, enc.finish()]);
        self.remote_pic.pending = Some(Readback {
            buffer,
            row,
            w,
            h,
            bgra,
            submission,
        });
    }
}
