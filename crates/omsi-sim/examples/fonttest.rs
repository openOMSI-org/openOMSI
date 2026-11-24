fn main() {
    omsi_sim::image_decode::install(decode_picture);
    let root = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let name = std::env::args().nth(2).unwrap();
    let text = std::env::args().nth(3).unwrap_or("EIN".into());
    let mut lib = omsi_sim::texttex::FontLibrary::new(&root);
    let atlas = lib.load(&name).expect("font");
    println!(
        "font {} from {} bitmap {} {}x{} height {} gap {}",
        atlas.font.name,
        atlas.font.path.display(),
        atlas.font.alpha,
        atlas.width,
        atlas.height,
        atlas.font.height,
        atlas.font.gap
    );
    for c in text.chars() {
        if let Some(g) = atlas.font.glyph(c) {
            println!("  '{c}' x0 {} x1 {} y {}", g.x0, g.x1, g.y);
        }
    }
    let mut st = omsi_sim::scripttex::ScriptTexture::new(128, 32);
    st.color = [255, 0, 0, 0];
    // like the VMatrix script: the number first with the big font, then the terminus
    if let Some(big) = lib.load("Krueger 16x9") {
        st.text_out(&big, 0, 0, 1, 2, "76");
    }
    st.text_out(&atlas, 32, 0, 0, 2, &text);
    for y in 0..8 {
        let row: String = (30..128)
            .map(|x| if st.get(x, y)[0] > 0 { '#' } else { '.' })
            .collect();
        println!("{row}");
    }

    // the [texttexture] path (a whole picture at once, '@' breaking it into lines), as the
    // IBIS screen and the number plates use it
    let (w, h) = (
        std::env::args()
            .nth(4)
            .and_then(|v| v.parse().ok())
            .unwrap_or(256u32),
        std::env::args()
            .nth(5)
            .and_then(|v| v.parse().ok())
            .unwrap_or(64u32),
    );
    let img = atlas.render(&text, w, h, false, [255, 255, 255]);
    println!("render {w}x{h} of {text:?}:");
    for y in 0..h {
        let row: String = (0..w)
            .step_by((w / 100).max(1) as usize)
            .map(|x| {
                if img[((y * w + x) * 4 + 3) as usize] > 0 {
                    '#'
                } else {
                    '.'
                }
            })
            .collect();
        println!("{y:3} {row}");
    }
}

/// The game's picture decoder for the simulation (as omsi-app registers it at start).
fn decode_picture(path: &std::path::Path) -> Result<omsi_sim::image_decode::DecodedImage, String> {
    omsi_texture::decode_file(path)
        .map(|i| omsi_sim::image_decode::DecodedImage {
            width: i.width,
            height: i.height,
            rgba: i.rgba,
            has_alpha: i.has_alpha,
        })
        .map_err(|e| e.to_string())
}
