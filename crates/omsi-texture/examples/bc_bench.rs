//! Encoder speed and quality on real pictures: `bc_bench <dir-or-zip> [filter] [limit]`
//! decodes the pictures first, then times `bc::encode` alone (all levels as the loader
//! makes them) and prints the colour PSNR spread.

use omsi_texture::bc::{self, Bc};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let src = PathBuf::from(args.get(1).expect("a zip"));
    let filter = args
        .get(2)
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let limit: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(300);
    let root = omsi_cfg::vfs::mount_zip(&src).expect("zip");
    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(d) = stack.pop() {
        for (name, is_dir) in omsi_cfg::vfs::list_dir(&d).unwrap_or_default() {
            let p = d.join(&name);
            if is_dir {
                stack.push(p);
            } else {
                let s = p.to_string_lossy().to_ascii_lowercase();
                if (s.ends_with(".bmp") || s.ends_with(".tga") || s.ends_with(".jpg"))
                    && s.contains(&filter)
                {
                    files.push(p);
                }
            }
        }
    }
    files.sort();
    files.truncate(limit);
    let imgs: Vec<omsi_texture::Image> = files
        .iter()
        .filter_map(|p| omsi_texture::decode_file(p).ok())
        .filter(|i| i.width % 4 == 0 && i.height % 4 == 0 && i.width >= 64)
        .collect();
    let texels: u64 = imgs.iter().map(|i| i.width as u64 * i.height as u64).sum();
    let t = std::time::Instant::now();
    let mut psnrs = Vec::new();
    for img in &imgs {
        let opaque = img.rgba.chunks_exact(4).all(|p| p[3] == 255);
        let f = if opaque {
            Bc::Bc1 { punch: false }
        } else {
            Bc::Bc3
        };
        let (_, ec, _) = bc::encode(&img.rgba, img.width, img.height, f);
        let n = img.width as f64 * img.height as f64;
        psnrs.push(10.0 * (65025.0 / (ec / n).max(1e-9)).log10());
        let (mut rgba, mut w, mut h) = (img.rgba.clone(), img.width, img.height);
        while w > 1 || h > 1 {
            let (next, nw, nh) = bc::downsample(&rgba, w, h);
            let _ = bc::encode(&next, nw, nh, f);
            rgba = next;
            w = nw;
            h = nh;
        }
    }
    let secs = t.elapsed().as_secs_f64();
    psnrs.sort_by(|a, b| a.total_cmp(b));
    let pct = |q: f64| psnrs[((psnrs.len() as f64 - 1.0) * q) as usize];
    println!(
        "{} pictures, {:.1} Mpx: {:.2} s wall ({:.1} ns a texel), PSNR p5 {:.2} p50 {:.2} mean {:.3}",
        imgs.len(),
        texels as f64 / 1e6,
        secs,
        secs * 1e9 / texels as f64,
        pct(0.05),
        pct(0.5),
        psnrs.iter().sum::<f64>() / psnrs.len() as f64
    );
}
