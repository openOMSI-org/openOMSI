//! Draw a few shapes with omsi-ui into a texture and write it as PNG (a check of the
//! pipeline without the game): `cargo run -p omsi-ui-mx --example headless_mx out.png`
extern crate omsi_ui_mx as omsi_ui;
use glam::{Mat4, Vec2, Vec3};
use omsi_ui::{Atlas, Color, Draw, Fonts, Gpu, Layer, Painter, Rect, Weight, paint::Align};

fn main() {
    let out = std::env::args().nth(1).unwrap_or("headless.png".into());
    let instance = mx::Instance::default();
    let adapter = instance.default_adapter().expect("adapter");
    let (device, queue) = adapter
        .request_device(&mx::DeviceDescriptor::default())
        .expect("device");
    let (w, h) = (460u32, 368u32);
    let format = mx::TextureFormat::Rgba8UnormSrgb;
    let target = device
        .create_texture(&mx::TextureDescriptor {
            label: None,
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
    let view = target
        .create_view(&Default::default())
        .expect("mX: a texture view");
    let mut gpu = Gpu::new(&device, format, 4, 512);
    let fonts = Fonts::new();
    let mut atlas = Atlas::new(512);
    atlas.begin_frame();
    // buffer 0: a grid of roads; buffer 1: a route
    let mut roads = Painter::new();
    for k in -5..=5 {
        let x = k as f32 * 60.0;
        roads.ribbon(
            &[Vec3::new(x, -400.0, 0.0), Vec3::new(x, 400.0, 0.0)],
            6.0,
            4.0,
            Color::hex(0x62686f),
            true,
        );
        roads.ribbon(
            &[Vec3::new(-400.0, x, 0.0), Vec3::new(400.0, x, 0.0)],
            6.0,
            4.0,
            Color::hex(0x62686f),
            true,
        );
    }
    // a turning loop at the end of a road, sampled every couple of metres as a map's is
    let mut lp = vec![Vec3::new(-120.0, -60.0, 0.0), Vec3::new(-120.0, 80.0, 0.0)];
    for k in 0..=40 {
        let a = std::f32::consts::PI * (1.5 - 2.0 * k as f32 / 40.0 * 0.9);
        lp.push(Vec3::new(
            -120.0 + 14.0 * a.cos() + 0.0,
            94.0 + 14.0 * a.sin(),
            0.0,
        ));
    }
    lp.push(Vec3::new(-121.0, 80.0, 0.0));
    roads.ribbon(&lp, 8.0, 5.0, Color::hex(0x1e1e1e), true);
    roads.ribbon(&lp, 6.0, 3.0, Color::hex(0x62686f), true);
    let mut route = Painter::new();
    route.ribbon(
        &[
            Vec3::new(0.0, -100.0, 0.0),
            Vec3::new(0.0, 60.0, 0.0),
            Vec3::new(60.0, 60.0, 0.0),
            Vec3::new(60.0, 300.0, 0.0),
        ],
        5.0,
        7.0,
        Color::hex(0xe22622),
        true,
    );
    let mut ui = Painter::new();
    ui.rounded(
        Rect::new(0.0, 0.0, w as f32, h as f32),
        12.0,
        Color::rgba(20, 22, 26, 1.0),
    );
    let n_bg = ui.len();
    ui.text(
        &mut atlas,
        &fonts,
        "Bauernhof 08:59",
        18.0,
        Weight::Bold,
        Vec2::new(12.0, 26.0),
        Align::Left,
        Color::WHITE,
    );
    ui.icon(
        &mut atlas,
        "directions_bus",
        Vec2::new(420.0, 20.0),
        24.0,
        Color::WHITE,
    );
    gpu.upload(&device, &queue, 0, &roads.verts);
    gpu.upload(&device, &queue, 1, &route.verts);
    gpu.upload(&device, &queue, 2, &ui.verts);
    gpu.upload_atlas(&queue, &mut atlas);
    let viewv = Mat4::look_at_rh(
        Vec3::new(0.0, -120.0, 160.0),
        Vec3::new(0.0, 40.0, 0.0),
        Vec3::Z,
    );
    let map = [0.0, 40.0, w as f32, h as f32 - 40.0];
    let layers = [
        Layer::flat([0.0, 0.0, w as f32, h as f32], 12.0, 1.0),
        Layer::world(viewv, 0.73, map, [0.0, 40.0, w as f32, h as f32], 12.0, 1.0),
    ];
    let draws = [
        Draw {
            buffer: 2,
            range: 0..n_bg,
            layer: 0,
            texture: 0,
        },
        Draw {
            buffer: 0,
            range: 0..roads.len(),
            layer: 1,
            texture: 0,
        },
        Draw {
            buffer: 1,
            range: 0..route.len(),
            layer: 1,
            texture: 0,
        },
        Draw {
            buffer: 2,
            range: n_bg..ui.len(),
            layer: 0,
            texture: 0,
        },
    ];
    let mut enc = device.create_command_encoder(None);
    gpu.render(
        &device,
        &queue,
        &mut enc,
        &view,
        (w, h),
        Some(mx::Color::TRANSPARENT),
        &layers,
        &draws,
    );
    let stride = (w * 4).div_ceil(256) * 256;
    let buf = device
        .create_buffer(&mx::BufferDescriptor {
            label: None,
            size: (stride * h) as u64,
            usage: mx::BufferUsages::COPY_DST | mx::BufferUsages::MAP_READ,
        })
        .expect("mX: a buffer");
    enc.copy_texture_to_buffer(
        target.as_image_copy(),
        mx::TexelCopyBufferInfo {
            buffer: &buf,
            layout: mx::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: None,
            },
        },
        mx::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    let copied = queue.submit([enc.finish()]);
    device.wait(copied).expect("the picture drawn");
    let img = buf
        .map_read_now(0..buf.size(), |data| {
            let mut img = image::RgbaImage::new(w, h);
            for y in 0..h {
                for x in 0..w {
                    let i = (y * stride + x * 4) as usize;
                    img.put_pixel(
                        x,
                        y,
                        image::Rgba([data[i], data[i + 1], data[i + 2], data[i + 3]]),
                    );
                }
            }
            img
        })
        .expect("the picture read");
    img.save(&out).unwrap();
    println!("wrote {out}");
}
