//! How the textures of a folder or a mounted archive fare on their way to the GPU:
//! `cargo run --release -p omsi-texture --example bc_check -- <dir-or-zip> [filter] [limit]`
//! prints, per source kind, how many were kept as blocks, compressed, or left as RGBA
//! for quality, the PSNR spread, the bytes before and after and the time it took.

use omsi_texture::gpu::{GpuOptions, PixelFormat, load_gpu_bytes};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let src = PathBuf::from(args.get(1).expect("a folder or a zip"));
    let filter = args
        .get(2)
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let limit: usize = args
        .get(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    let mut files: Vec<PathBuf> = Vec::new();
    if src
        .extension()
        .map(|e| e.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
    {
        let root = omsi_cfg::vfs::mount_zip(&src).expect("zip");
        let mut stack = vec![root];
        while let Some(d) = stack.pop() {
            for (name, is_dir) in omsi_cfg::vfs::list_dir(&d).unwrap_or_default() {
                let p = d.join(&name);
                if is_dir {
                    stack.push(p);
                } else {
                    files.push(p);
                }
            }
        }
    } else {
        let mut stack = vec![src.clone()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    files.push(p);
                }
            }
        }
    }
    files.retain(|p| {
        let s = p.to_string_lossy().to_ascii_lowercase();
        ["dds", "bmp", "tga", "jpg", "png"]
            .iter()
            .any(|e| s.ends_with(e))
            && s.contains(&filter)
            && !s.contains("/texture/map/")
    });
    files.sort();
    files.truncate(limit);
    let o = GpuOptions {
        bc: true,
        compress: std::env::var_os("BC_CHECK_NO_COMPRESS").is_none(),
    };
    #[derive(Default)]
    struct Row {
        n: usize,
        kept: usize,
        encoded: usize,
        rejected: usize,
        before: u64,
        after: u64,
        secs: f64,
        psnr: Vec<f64>,
    }
    let mut rows: BTreeMap<String, Row> = BTreeMap::new();
    let mut worst: Vec<(f64, String)> = Vec::new();
    let t_all = std::time::Instant::now();
    for p in &files {
        let Ok(bytes) = omsi_cfg::vfs::read(p) else {
            continue;
        };
        let kind = if bytes.starts_with(b"DDS ") {
            format!(
                "dds-{}",
                String::from_utf8_lossy(&bytes[84..88]).trim_matches(char::from(0))
            )
        } else {
            p.extension()
                .unwrap()
                .to_string_lossy()
                .to_ascii_lowercase()
        };
        let t = std::time::Instant::now();
        let Ok((data, info)) = load_gpu_bytes(&bytes, Path::new(p), o) else {
            continue;
        };
        let secs = t.elapsed().as_secs_f64();
        let r = rows.entry(kind).or_default();
        r.n += 1;
        r.secs += secs;
        let full = omsi_texture::gpu::mip_count(data.width, data.height);
        let mut rgba = 0u64;
        for l in 0..full {
            rgba += PixelFormat::Rgba8
                .level_bytes((data.width >> l).max(1), (data.height >> l).max(1))
                as u64;
        }
        r.before += rgba;
        r.after += data.gpu_bytes();
        if info.encoded {
            r.encoded += 1;
            r.psnr.push(info.psnr.0.min(info.psnr.1));
            worst.push((
                info.psnr.0.min(info.psnr.1),
                format!(
                    "{} {}x{} {:?}",
                    p.display(),
                    data.width,
                    data.height,
                    data.format
                ),
            ));
        } else if info.rejected {
            r.rejected += 1;
            worst.push((
                info.psnr.0.min(info.psnr.1),
                format!(
                    "REJECTED {} {}x{} psnr {:.1}/{:.1}",
                    p.display(),
                    data.width,
                    data.height,
                    info.psnr.0,
                    info.psnr.1
                ),
            ));
        } else if data.format.is_compressed() {
            r.kept += 1;
        } else if std::env::var_os("BC_CHECK_VERBOSE").is_some() {
            println!(
                "  rgba {} {}x{} {} MB",
                p.display(),
                data.width,
                data.height,
                rgba as f64 / 1e6
            );
        }
    }
    println!(
        "{} files in {:.1} s",
        files.len(),
        t_all.elapsed().as_secs_f64()
    );
    println!(
        "{:10} {:>6} {:>6} {:>7} {:>8} {:>10} {:>10} {:>8} {:>8} {:>8}",
        "kind",
        "files",
        "dxt",
        "encoded",
        "rejected",
        "rgba MB",
        "gpu MB",
        "secs",
        "psnr p5",
        "psnr p50"
    );
    for (k, mut r) in rows {
        r.psnr.sort_by(|a, b| a.total_cmp(b));
        let pct = |q: f64| {
            r.psnr
                .get(((r.psnr.len() as f64 - 1.0) * q) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        println!(
            "{:10} {:>6} {:>6} {:>7} {:>8} {:>10.1} {:>10.1} {:>8.2} {:>8.1} {:>8.1}",
            k,
            r.n,
            r.kept,
            r.encoded,
            r.rejected,
            r.before as f64 / 1e6,
            r.after as f64 / 1e6,
            r.secs,
            pct(0.05),
            pct(0.5)
        );
    }
    worst.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (p, s) in worst.iter().take(12) {
        println!("  {p:5.1} {s}");
    }
}
