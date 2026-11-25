//! Decode every texture under the given folders or zip archives and print one line of
//! statistics per file (for comparing the decoder with a reference one):
//! `path \t w \t h \t has_alpha \t mean r g b a \t mean luma of the top / bottom half`, or
//! `path \t ERR \t message`.
//! usage: audit [--filter <substring>] <dir or .zip>...
use std::path::{Path, PathBuf};

fn walk(p: &Path, filter: &str, out: &mut Vec<PathBuf>) {
    let Some(entries) = omsi_cfg::vfs::list_dir(p) else {
        return;
    };
    for (name, is_dir) in entries {
        let c = p.join(&name);
        if is_dir {
            walk(&c, filter, out);
            continue;
        }
        let ext = c
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(ext.as_str(), "bmp" | "tga" | "dds" | "png" | "jpg" | "jpeg") && {
            let l = c.to_string_lossy().to_ascii_lowercase();
            filter.split('|').any(|f| l.contains(f))
        } {
            out.push(c);
        }
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut filter = String::new();
    // --gpu: compare the texture as it goes to the GPU (blocks decoded back) with the picture
    let gpu = args.first().map(|a| a == "--gpu").unwrap_or(false);
    if gpu {
        args.remove(0);
    }
    if args.first().map(|a| a == "--filter").unwrap_or(false) {
        filter = args[1].to_ascii_lowercase();
        args.drain(..2);
    }
    let mut files = Vec::new();
    for a in &args {
        let p = PathBuf::from(a);
        let root = if a.to_ascii_lowercase().ends_with(".zip") {
            omsi_cfg::vfs::mount_zip(&p).expect("zip")
        } else {
            p
        };
        walk(&root, &filter, &mut files);
    }
    files.sort();
    if gpu {
        use omsi_texture::gpu::*;
        for f in files {
            let Ok(bytes) = omsi_cfg::vfs::read(&f) else {
                continue;
            };
            let Ok(img) = omsi_texture::decode_bytes(&bytes, &f) else {
                continue;
            };
            let Ok((t, _)) = load_gpu_bytes(
                &bytes,
                &f,
                GpuOptions {
                    bc: true,
                    compress: true,
                },
            ) else {
                println!("{}\tGPU-ERR", f.display());
                continue;
            };
            let back = t
                .format
                .decode_level(&t.levels[0], t.width, t.height, t.has_alpha);
            let mean = |v: &[u8]| {
                let mut s = [0f64; 4];
                for p in v.chunks_exact(4) {
                    for c in 0..4 {
                        s[c] += p[c] as f64;
                    }
                }
                let n = (v.len() / 4).max(1) as f64;
                s.map(|x| x / n)
            };
            let (a, b) = (mean(&img.rgba), mean(&back));
            let d = (0..4).map(|c| (a[c] - b[c]).abs()).fold(0f64, f64::max);
            let flag = if d > 3.0 || t.has_alpha != img.has_alpha {
                "BAD"
            } else {
                "ok"
            };
            println!(
                "{}\t{flag}\t{:?}\t{}x{}\talpha {}/{}\tmean {:.1} {:.1} {:.1} {:.1} -> {:.1} {:.1} {:.1} {:.1}",
                f.display(),
                t.format,
                t.width,
                t.height,
                img.has_alpha as u8,
                t.has_alpha as u8,
                a[0],
                a[1],
                a[2],
                a[3],
                b[0],
                b[1],
                b[2],
                b[3]
            );
        }
        return;
    }
    for f in files {
        match omsi_texture::decode_file(&f) {
            Ok(img) => {
                let n = (img.width * img.height) as f64;
                let mut s = [0f64; 4];
                let (mut top, mut bot) = (0f64, 0f64);
                let half = (img.height / 2) as usize * img.width as usize;
                for (i, p) in img.rgba.chunks_exact(4).enumerate() {
                    for c in 0..4 {
                        s[c] += p[c] as f64;
                    }
                    let l = p[0] as f64 * 0.299 + p[1] as f64 * 0.587 + p[2] as f64 * 0.114;
                    if i < half {
                        top += l;
                    } else {
                        bot += l;
                    }
                }
                let rest = n - half as f64;
                println!(
                    "{}\t{}\t{}\t{}\t{:.2} {:.2} {:.2} {:.2}\t{:.2} {:.2}",
                    f.display(),
                    img.width,
                    img.height,
                    img.has_alpha as u8,
                    s[0] / n,
                    s[1] / n,
                    s[2] / n,
                    s[3] / n,
                    top / (half.max(1) as f64),
                    bot / rest.max(1.0)
                );
            }
            Err(e) => println!(
                "{}\tERR\t{}",
                f.display(),
                e.to_string().replace(['\t', '\n'], " ")
            ),
        }
    }
}
