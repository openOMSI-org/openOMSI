//! `openomsi --launcher-service`: what the desktop launcher (`launcher/`, a webview) asks of
//! the game program, with no window of its own - the bus preview drawn by the game's
//! renderer exactly as the launcher window draws it (`showroom`), and the city map with a
//! trip's route read the way the navigator reads it (`mapview::export`).
//!
//! One request a line on stdin, JSON with an `id` and a `cmd`. Each answer is a JSON line
//! (`id`, what it says, `len`) followed by `len` bytes: a JPEG for a picture, nothing else
//! otherwise. Pictures are drawn on the service's thread, maps are read on workers, so a map
//! that takes a second to read does not stop the bus from turning.
//!
//! A phone runs the launcher's page inside the game program itself: there the same service
//! runs on a thread of its own (`in_process`), asked through a channel instead of a pipe.

use super::showroom::{Look, PreviewView, Quality, Showroom};
use omsi_render::Renderer;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

type Out = Arc<Mutex<Box<dyn Write + Send>>>;

/// Where an answer goes: the request's `id`, what it says, the bytes after it.
pub(super) type Reply = Arc<dyn Fn(&Value, Value, &[u8]) + Send + Sync>;

/// The answers' channel: stdout as the program got it, while anything else printing to
/// stdout from here on (a stray `println!` deep in the game) goes to stderr instead and
/// cannot cut into a picture.
fn answers() -> Box<dyn Write + Send> {
    #[cfg(unix)]
    {
        use std::os::fd::FromRawFd;
        // SAFETY: plain descriptor calls on this process's own stdout and stderr
        unsafe {
            let real = libc::dup(1);
            if real >= 0 && libc::dup2(2, 1) >= 0 {
                return Box::new(std::fs::File::from_raw_fd(real));
            }
        }
    }
    Box::new(std::io::stdout())
}

fn write(out: &Out, id: &Value, mut head: Value, body: &[u8]) {
    head["id"] = id.clone();
    head["len"] = json!(body.len());
    let mut o = out.lock().unwrap_or_else(|e| e.into_inner());
    let _ = writeln!(o, "{head}");
    let _ = o.write_all(body);
    let _ = o.flush();
}

/// `openomsi --launcher-service`: requests on stdin, answers on stdout.
pub fn run() -> anyhow::Result<()> {
    super::install();
    let out: Out = Arc::new(Mutex::new(answers()));
    let reply: Reply = Arc::new(move |id, head, body| write(&out, id, head, body));
    let (tx, rx) = std::sync::mpsc::channel::<Value>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            match serde_json::from_str::<Value>(&line) {
                Ok(v) => {
                    if tx.send(v).is_err() {
                        break;
                    }
                }
                Err(e) => log::warn!("launcher service: {e}: {line}"),
            }
        }
    });
    serve(rx, reply);
    Ok(())
}

/// The service in this process (the phone's launcher page): a thread of its own, asked with
/// `ask`.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub struct InProcess {
    tx: Mutex<std::sync::mpsc::Sender<Value>>,
    waiting: Arc<Mutex<std::collections::HashMap<u64, std::sync::mpsc::Sender<(Value, Vec<u8>)>>>>,
    next: std::sync::atomic::AtomicU64,
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
impl InProcess {
    pub fn start() -> InProcess {
        let (tx, rx) = std::sync::mpsc::channel::<Value>();
        let waiting: Arc<Mutex<std::collections::HashMap<u64, std::sync::mpsc::Sender<(Value, Vec<u8>)>>>> = Arc::default();
        let w = waiting.clone();
        let reply: Reply = Arc::new(move |id, head, body| {
            if let Some(tx) = id.as_u64().and_then(|i| w.lock().unwrap_or_else(|e| e.into_inner()).remove(&i)) {
                let _ = tx.send((head, body.to_vec()));
            }
        });
        std::thread::Builder::new().name("launcher service".into()).spawn(move || serve(rx, reply)).ok();
        InProcess { tx: Mutex::new(tx), waiting, next: std::sync::atomic::AtomicU64::new(1) }
    }

    /// Ask the service; its answer and the bytes after it.
    pub fn ask(&self, mut req: Value, wait: std::time::Duration) -> Result<(Value, Vec<u8>), String> {
        let id = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        req["id"] = json!(id);
        let (tx, rx) = std::sync::mpsc::channel();
        self.waiting.lock().unwrap_or_else(|e| e.into_inner()).insert(id, tx);
        self.tx.lock().unwrap_or_else(|e| e.into_inner()).send(req).map_err(|_| "the preview service ended".to_string())?;
        let (head, body) = rx.recv_timeout(wait).map_err(|_| "the preview service did not answer".to_string())?;
        if head.get("ok").and_then(Value::as_bool) == Some(false) {
            return Err(head.get("error").and_then(Value::as_str).unwrap_or("the preview service failed").to_string());
        }
        Ok((head, body))
    }
}

/// Answer requests until they stop coming.
fn serve(rx: std::sync::mpsc::Receiver<Value>, reply: Reply) {
    let send = |id: &Value, head: Value, body: &[u8]| reply(id, head, body);
    let mut renderer: Option<Renderer> = None;
    let mut quality = Quality::now();
    let mut showroom = Showroom::new();
    let pads = super::pads::Pads::start(super::pads::paths());
    let encoder = {
        let (tx, rx) = std::sync::mpsc::channel::<Encode>();
        let reply = reply.clone();
        std::thread::Builder::new().name("preview jpeg".into()).spawn(move || {
            for mut e in rx {
                let t = Instant::now();
                let jpeg = encode_jpeg(&e.pic, e.quality).unwrap_or_default();
                e.ms["jpeg"] = json!(t.elapsed().as_secs_f64() * 1e3);
                e.head["ms"] = e.ms;
                reply(&e.id, e.head, &jpeg);
            }
        }).ok();
        tx
    };
    let mut last = Instant::now();
    while let Ok(req) = rx.recv() {
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let s = |k: &str| req.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let f = |k: &str, d: f64| req.get(k).and_then(Value::as_f64).unwrap_or(d);
        match req.get("cmd").and_then(Value::as_str).unwrap_or("") {
            "look" => {
                if showroom.follow(&mut quality, true) {
                    renderer = None;
                }
                let _ = omsi_launcher_lib::content_dir();
                let root = omsi_launcher_lib::root_dir().unwrap_or_default();
                showroom.want(Look { root, map: s("map"), bus: s("bus"), paint: s("paint"), weather: s("weather"), time: f("time", 540.0) as i32, date: s("date") });
                send(&id, json!({ "ok": true }), &[]);
            }
            "orbit" => {
                showroom.orbit(f("dx", 0.0) as f32, f("dy", 0.0) as f32);
                send(&id, json!({ "ok": true }), &[]);
            }
            "zoom" => {
                showroom.zoom_by(f("k", 1.0) as f32);
                send(&id, json!({ "ok": true }), &[]);
            }
            "turn" => {
                showroom.auto_turn = req.get("on").and_then(Value::as_bool).unwrap_or(false);
                send(&id, json!({ "ok": true }), &[]);
            }
            // (the launcher starts: the renderer's pipelines are built now, not on the first picture)
            "warm" => {
                if renderer.is_none() {
                    renderer = open_renderer(quality.options).ok();
                }
                send(&id, json!({ "ok": renderer.is_some() }), &[]);
            }
            "frame" => {
                // (a drag and the wheel come with the picture they ask for: one round trip)
                let (dx, dy, zoom) = (f("dx", 0.0) as f32, f("dy", 0.0) as f32, f("zoom", 1.0) as f32);
                if dx != 0.0 || dy != 0.0 {
                    showroom.orbit(dx, dy);
                }
                if zoom != 1.0 {
                    showroom.zoom_by(zoom);
                }
                if let Some(view) = req.get("view").and_then(Value::as_str).and_then(PreviewView::from_name) {
                    showroom.set_view(view);
                }
                if showroom.follow(&mut quality, false) {
                    renderer = None;
                }
                if renderer.is_none() {
                    match open_renderer(quality.options) {
                        Ok(r) => renderer = Some(r),
                        Err(e) => {
                            send(&id, json!({ "ok": false, "error": format!("{e:#}") }), &[]);
                            continue;
                        }
                    }
                }
                let r = renderer.as_mut().unwrap();
                let dt = last.elapsed().as_secs_f32().min(0.1);
                last = Instant::now();
                showroom.update(r, dt);
                let before = showroom.drawn;
                let (w, h) = (f("w", 960.0) as u32, f("h", 540.0) as u32);
                let t_draw = Instant::now();
                let _ = showroom.preview(r, w, h);
                let drew = t_draw.elapsed();
                let head = json!({ "ok": true, "busy": showroom.busy, "failed": showroom.failed(), "error": showroom.error.as_deref().map(omsi_ui::tr), "moving": showroom.moving(), "picture": showroom.has_picture(), "view": showroom.view.name(), "views": showroom.views() });
                let new = showroom.drawn != before || req.get("force").and_then(Value::as_bool).unwrap_or(false);
                match (new, showroom.picture()) {
                    (true, Some(tex)) => {
                        // (lighter while it turns: the still picture after it is the sharp one)
                        let quality = f("quality", 90.0).clamp(40.0, 95.0) as u8;
                        // The card's pixels are read here; the JPEG is made on its own thread
                        // while the next picture is already being drawn (made on this one, the
                        // two took turns and a phone's frame was their sum).
                        let t_read = Instant::now();
                        match read_pixels(r, tex) {
                            Some(pic) => {
                                let ms = json!({ "draw": drew.as_secs_f64() * 1e3, "read": t_read.elapsed().as_secs_f64() * 1e3 });
                                let _ = encoder.send(Encode { id, head, pic, quality, ms });
                            }
                            None => send(&id, head, &[]),
                        }
                    }
                    _ => send(&id, head, &[]),
                }
            }
            "map" => {
                let out = reply.clone();
                let (map, date, trip) = (s("map"), s("date"), s("trip"));
                let surface = req.get("surface").and_then(Value::as_bool).unwrap_or(true);
                let flag = |k: &str| req.get(k).and_then(Value::as_bool).unwrap_or(false);
                let detail = if flag("route_only") {
                    omsi_launcher_model::mapview::Detail::Route
                } else if flag("editor") {
                    omsi_launcher_model::mapview::Detail::Editor
                } else {
                    omsi_launcher_model::mapview::Detail::Map
                };
                std::thread::spawn(move || {
                    let global = omsi_cfg::find_in_roots(&map).map(|(_, p)| p).or_else(|| omsi_launcher_lib::content_path(&map).ok()).unwrap_or_else(|| PathBuf::from(&map));
                    let v = omsi_launcher_model::mapview::export_with(&map, &global, &date, &trip, 0.6, surface, detail);
                    out(&id, json!({ "ok": true }), v.to_string().as_bytes());
                });
            }
            "map_ground" => {
                let out = reply.clone();
                let (map, date) = (s("map"), s("date"));
                let (x, y, reach, mpp) = (f("x", 0.0), f("y", 0.0), f("reach", 1500.0), f("mpp", 2.0));
                let light = req.get("light").and_then(Value::as_bool).unwrap_or(false);
                std::thread::spawn(move || {
                    let global = omsi_cfg::find_in_roots(&map).map(|(_, p)| p).or_else(|| omsi_launcher_lib::content_path(&map).ok()).unwrap_or_else(|| PathBuf::from(&map));
                    let body = map_ground(&map, &global, &date, glam::DVec2::new(x, y), reach, mpp, light);
                    out(&id, json!({ "ok": true }), &body);
                });
            }
            "pads" => pads.ask(req, reply.clone()),
            "key_actions" => {
                let out = reply.clone();
                std::thread::spawn(move || out(&id, json!({ "ok": true }), super::key_actions::key_actions(&req).to_string().as_bytes()));
            }
            "quit" => break,
            other => send(&id, json!({ "ok": false, "error": format!("unknown command {other}") }), &[]),
        }
    }
    super::showroom::clear_placing_mark();
}

/// The ground of the launcher's map around `at` (metres from the page's origin, as `map`
/// answers it) as the game's city map builds it at `mpp` (`omsi_navigator::MapGround`): a
/// little-endian `u32` length and a JSON head (where the vertices are measured from, the step
/// of their positions, the runs of flat and widened triangles in drawing order, the palette as
/// CSS), then the flat triangles' vertices, 8 bytes each (position as `i16` steps, the colour
/// as sRGB bytes, straight alpha), then the widened ones', 16 bytes each (position, extrusion
/// in 1/4096, width in metres and in pixels in 1/256, all `i16`, then the colour). The page
/// widens each by its extrusion times the larger of the two widths, as the game's shader does.
/// A flat triangle smaller than half a pixel is left out: the picture is the same.
fn map_ground(map: &str, global: &std::path::Path, date: &str, at: glam::DVec2, reach: f64, mpp: f64, light: bool) -> Vec<u8> {
    use std::sync::Arc;
    type Read = (String, glam::DVec2, Arc<omsi_navigator::MapGround>);
    static LAST: Mutex<Option<Read>> = Mutex::new(None);
    let key = format!("{map}|{date}");
    let read = {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        match last.as_ref().filter(|r| r.0 == key) {
            Some(r) => (r.1, r.2.clone()),
            None => {
                let (origin, net, layers) = omsi_launcher_model::mapview::map_network(map, global, date);
                let package = global.parent().and_then(omsi_map::package::Package::open);
                let g = Arc::new(omsi_navigator::MapGround::new(net, layers, package));
                *last = Some((key, origin, g.clone()));
                (origin, g)
            }
        }
    };
    let (origin, ground) = read;
    let b = ground.build(origin + at, reach, mpp, light);
    let [ground_v, _, solid, flat] = b.vertices();
    let pal = omsi_mapdraw::style::palette(light);
    let css = |c: omsi_ui::Color| {
        let c = c.0;
        format!("rgba({},{},{},{:.3})", (c[0] * 255.0).round(), (c[1] * 255.0).round(), (c[2] * 255.0).round(), c[3])
    };
    // (the houses' standing walls and roofs, then the flat roofs; their shadows only fall
    // where the map is tilted, which this one never is)
    let verts: Vec<&omsi_ui::Vertex> = ground_v.iter().chain(solid).chain(flat).collect();
    let tris: Vec<(bool, &[&omsi_ui::Vertex])> = verts
        .chunks_exact(3)
        .filter_map(|t| {
            let wide = t.iter().any(|v| v.ext != [0.0, 0.0]);
            let span = |i: usize| t.iter().map(|v| v.pos[i]).fold(f32::MIN, f32::max) - t.iter().map(|v| v.pos[i]).fold(f32::MAX, f32::min);
            (wide || span(0).max(span(1)) as f64 >= mpp * 0.5).then_some((wide, t))
        })
        .collect();
    let far = tris.iter().flat_map(|(_, t)| t.iter()).fold(1.0f32, |m, v| m.max(v.pos[0].abs()).max(v.pos[1].abs()));
    let step = far / 32000.0;
    let mut runs: Vec<(bool, usize)> = Vec::new();
    let (mut flats, mut wides) = (Vec::new(), Vec::new());
    let i16s = |out: &mut Vec<u8>, xs: [f32; 2]| xs.into_iter().for_each(|x| out.extend_from_slice(&(x.round().clamp(-32767.0, 32767.0) as i16).to_le_bytes()));
    for (wide, t) in &tris {
        match runs.last_mut() {
            Some((w, n)) if w == wide => *n += 3,
            _ => runs.push((*wide, 3)),
        }
        for v in t.iter() {
            let out = if *wide { &mut wides } else { &mut flats };
            i16s(out, [v.pos[0] / step, v.pos[1] / step]);
            if *wide {
                i16s(out, [v.ext[0] * 4096.0, v.ext[1] * 4096.0]);
                i16s(out, [v.width[0] * 256.0, v.width[1] * 256.0]);
            }
            out.extend(v.color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    let head = json!({
        "anchor": [b.anchor.x - origin.x, b.anchor.y - origin.y],
        "step": step,
        "runs": runs.iter().map(|(w, n)| json!([*w as u8, n])).collect::<Vec<_>>(),
        "flat": flats.len(),
        "palette": {
            "land": css(pal.land), "route": css(pal.route), "route_edge": css(pal.route_edge), "driven": css(pal.driven),
            "card": css(pal.card), "card_text": css(pal.card_text), "card_dim": css(pal.card_dim),
            "label": css(pal.label), "label_halo": css(pal.label_halo),
        },
    })
    .to_string();
    let mut out = Vec::with_capacity(4 + head.len() + flats.len() + wides.len());
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(head.as_bytes());
    out.extend_from_slice(&flats);
    out.extend_from_slice(&wides);
    out
}

impl Showroom {
    fn failed(&self) -> bool {
        self.error.is_some() && !self.has_picture()
    }
}

/// The preview's renderer: no window, the launcher window's own options.
fn open_renderer(options: omsi_render::RenderOptions) -> anyhow::Result<Renderer> {
    let instance = crate::graphics_instance();
    let r = pollster::block_on(Renderer::new_with(&instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb), options))?;
    log::info!("launcher service: renderer on {}", r.adapter_name);
    Ok(r)
}

/// A picture read back from the graphics card, ready to be made a JPEG.
struct Picture {
    w: u32,
    h: u32,
    bgra: bool,
    pixels: Vec<u8>,
}

/// A picture on its way to the JPEG thread, with the answer it goes out under.
struct Encode {
    id: Value,
    head: Value,
    pic: Picture,
    quality: u8,
    ms: Value,
}

/// The picture's pixels read back from the graphics card (rows without the copy's padding).
fn read_pixels(r: &Renderer, tex: &wgpu::Texture) -> Option<Picture> {
    let (w, h) = (tex.width(), tex.height());
    let row = (w * 4).div_ceil(256) * 256;
    let buf = r.device.create_buffer(&wgpu::BufferDescriptor { label: Some("preview readback"), size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut enc = r.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("preview readback") });
    enc.copy_texture_to_buffer(
        tex.as_image_copy(),
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    r.queue.submit([enc.finish()]);
    let slice = buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    omsi_render::wait_gpu(&r.device, None).ok()?;
    let data = slice.get_mapped_range();
    let bgra = matches!(tex.format(), wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        pixels.extend_from_slice(&data[y * row as usize..y * row as usize + w as usize * 4]);
    }
    drop(data);
    buf.unmap();
    Some(Picture { w, h, bgra, pixels })
}

/// The picture as a JPEG.
fn encode_jpeg(pic: &Picture, quality: u8) -> anyhow::Result<Vec<u8>> {
    let mut jpeg = Vec::new();
    let kind = if pic.bgra { jpeg_encoder::ColorType::Bgra } else { jpeg_encoder::ColorType::Rgba };
    jpeg_encoder::Encoder::new(&mut jpeg, quality).encode(&pic.pixels, pic.w as u16, pic.h as u16, kind)?;
    Ok(jpeg)
}
