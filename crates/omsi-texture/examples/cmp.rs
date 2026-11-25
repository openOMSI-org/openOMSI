//! Compare the built-in DDS/TGA decoders with the `image` crate on the given files.
fn main() {
    for p in std::env::args().skip(1) {
        let bytes = std::fs::read(&p).unwrap();
        let ext = std::path::Path::new(&p)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let ours = match ext.as_str() {
            "dds" => omsi_texture::dds::decode(&bytes),
            _ => omsi_texture::tga::decode(&bytes),
        };
        let fmt = if ext == "dds" {
            image::ImageFormat::Dds
        } else {
            image::ImageFormat::Tga
        };
        let theirs = image::load_from_memory_with_format(&bytes, fmt);
        match (ours, theirs) {
            (Ok(a), Ok(b)) => {
                let b = b.into_rgba8();
                if (a.width, a.height) != (b.width(), b.height()) {
                    println!(
                        "{p}: size differs {}x{} vs {}x{}",
                        a.width,
                        a.height,
                        b.width(),
                        b.height()
                    );
                    continue;
                }
                let diff: usize = a
                    .rgba
                    .iter()
                    .zip(b.as_raw())
                    .filter(|(x, y)| (**x as i32 - **y as i32).abs() > 8)
                    .count();
                println!(
                    "{p}: {}x{} differing channels {} of {}",
                    a.width,
                    a.height,
                    diff,
                    a.rgba.len()
                );
            }
            (Ok(a), Err(e)) => println!(
                "{p}: ours ok {}x{}, image crate failed: {e}",
                a.width, a.height
            ),
            (Err(e), _) => println!("{p}: ours failed: {e}"),
        }
    }
}
