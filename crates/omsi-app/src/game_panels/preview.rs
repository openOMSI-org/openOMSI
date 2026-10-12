//! Pictures of the game's panels as the game draws them (with `plugin_ui`'s cards), for a
//! look by eye: they need a graphics adapter and are left out unless asked for -
//! `cargo test -p omsi-app --lib game_panels::preview -- --ignored`, written beside
//! `OMSI_UI_PREVIEW` (its folder) or into `target/`.

use super::*;
use crate::plugin_ui::{draw_card, layout_typing, place};
use omsi_ui::{Atlas, Fonts, Gpu};

/// Draw `panels` (with the hit to show under the mouse) into a picture over a stand-in for
/// the road, and write it to `name`.
fn picture(name: &str, panels: &[(Panel, Option<&str>)]) {
    let (w, h, k) = (1600u32, 900u32, 1.0f32);
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("device");
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut gpu = Gpu::new(&device, format, 1, 1024);
    let fonts = Fonts::new();
    let mut atlas = Atlas::new(1024);
    atlas.begin_frame();
    let screen = glam::Vec2::new(w as f32 / k, h as f32 / k);
    let mut images = Default::default();
    for (i, (p, hot)) in panels.iter().enumerate() {
        let laid = layout_typing(p, &fonts, 1.0, None);
        let at = place(p, laid.w, laid.h, screen);
        let hot = hot.and_then(|id| laid.hits.iter().position(|h| h.element.as_deref() == Some(id)));
        draw_card(&mut gpu, &device, &queue, &mut atlas, &fonts, &view, (w, h), k, &laid, at, hot, i == 0, &mut images);
    }
    let dir = omsi_cfg::flags::OMSI_UI_PREVIEW
        .live_var()
        .ok()
        .and_then(|p| std::path::PathBuf::from(p).parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    let out = dir.join(name);
    // read back, laid over sky, houses and the street
    let stride = (w * 4).div_ceil(256) * 256;
    let buf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (stride * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: None } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([enc.finish()]);
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).ok();
    let data = buf.slice(..).get_mapped_range();
    let mut img = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let t = y as f32 / h as f32;
            let bg: [f32; 3] = if t < 0.45 { [120.0 + 80.0 * t, 165.0 + 60.0 * t, 225.0] } else if t < 0.7 { [160.0, 140.0, 128.0] } else { [70.0, 72.0, 76.0] };
            let i = (y * stride + x * 4) as usize;
            let a = data[i + 3] as f32 / 255.0;
            let px = |c: usize| (data[i + c] as f32 + bg[c] * (1.0 - a)).min(255.0) as u8;
            img.put_pixel(x, y, image::Rgba([px(0), px(1), px(2), 255]));
        }
    }
    img.save(&out).unwrap();
    println!("wrote {}", out.display());
}

fn summary() -> crate::journey::TripSummary {
    let row = |s: &str, p: &str, a: &str, d: &str, o: &str| [s, p, a, d, o].map(String::from);
    crate::journey::TripSummary {
        title: "Trip 3  |  137 to S+U Rathaus Spandau  |  08:20:00".into(),
        head: "Driver Test  |  Berlin-Spandau  |  MAN NL202  |  1989-05-30\nLine 137, tour 2".into(),
        rows: vec![
            row("Hahneberg", "08:20:00", "08:20:41", "+00:00:41", "75556.6 km"),
            row("Heerstr./Wilhelmstr.", "08:23:00", "08:24:10", "+00:01:10", "75557.9 km"),
            row("Melanchthonplatz", "08:25:00", "08:26:02", "+00:01:02", "75558.4 km"),
            row("Am Juliusturm", "08:28:00", "", "", ""),
            row("S+U Rathaus Spandau", "08:32:00", "08:31:30", "-00:00:30", "75560.2 km"),
        ],
        km: Some(3.6),
        served: (4, 5),
        on_time: 3,
        driven: Some(690.0),
        rating: Some(crate::journey::TripRating { driving: 82.0, comfort: 95.0, tickets: 60.0, passengers: 23, tickets_asked: 9 }),
        stem: "preview".into(),
        dir: std::env::temp_dir(),
    }
}

#[test]
#[ignore]
fn preview() {
    picture("game-panels-quick.png", &[(quick_menu::QuickMenu.panel_of(true), Some("repair"))]);
    picture("game-panels-trip.png", &[(trip_summary::TripPanel::new(summary()).panel(), Some("save"))]);
    let form = dest_form::DestForm { route: "137".into(), lines: ["Sonderfahrt".into(), "Betriebshof".into()], ..Default::default() };
    picture("game-panels-dest.png", &[(form.panel(), Some("select"))]);
    let values = view_editor::ViewValues { seat: [0.05, -0.12, 0.03], pitch: -4.0, yaw: 6.5, fov: Some(62.0), mirrors: vec![[2.5, -1.0], [-3.0, 0.5]] };
    picture("game-panels-view.png", &[(view_editor::ViewEditor::default().panel_of(&values), None)]);
    picture("game-panels-move.png", &[(move_here::panel(glam::DVec3::new(-1948.9, -174.7, 8.8)), Some("yes"))]);
}
