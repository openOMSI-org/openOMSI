//! Every picture variant D3DX 9 reads for Omsi.exe (`D3DXCreateTextureFromFileExA` with
//! `D3DFMT_UNKNOWN`, no colour key, 0x7f8630), written here as small files and decoded with
//! [`crate::decode_bytes`]. The pixel each test looks at is the one D3DX hands over: the
//! colour in R, G, B order and the alpha of the format it picks (255 where that format has
//! no alpha: R8G8B8, X8R8G8B8, X1R5G5B5, R5G6B5, L8, JPEG).

use crate::{Image, decode_bytes};
use std::path::Path;

fn px(img: &Image, x: u32, y: u32) -> [u8; 4] {
    let o = ((y * img.width + x) * 4) as usize;
    img.rgba[o..o + 4].try_into().unwrap()
}

fn dec(bytes: &[u8], name: &str) -> Image {
    decode_bytes(bytes, Path::new(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

// ---------------------------------------------------------------- BMP

/// A bitmap with a header of `hdr` bytes (40, 52, 56, 108, 124), the given masks (written
/// after a 40-byte header, inside a longer one), palette and pixel rows (as stored).
fn bmp(
    hdr: u32,
    w: i32,
    h: i32,
    bpp: u16,
    comp: u32,
    masks: &[u32],
    palette: &[[u8; 4]],
    data: &[u8],
) -> Vec<u8> {
    let extra = if hdr == 40 && comp == 3 {
        masks.len() as u32 * 4
    } else {
        0
    };
    let off = 14 + hdr + extra + palette.len() as u32 * 4;
    let mut b = Vec::new();
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&(off + data.len() as u32).to_le_bytes());
    b.extend_from_slice(&[0; 4]);
    b.extend_from_slice(&off.to_le_bytes());
    let mut ih = vec![0u8; hdr as usize];
    ih[0..4].copy_from_slice(&hdr.to_le_bytes());
    ih[4..8].copy_from_slice(&w.to_le_bytes());
    ih[8..12].copy_from_slice(&h.to_le_bytes());
    ih[12..14].copy_from_slice(&1u16.to_le_bytes());
    ih[14..16].copy_from_slice(&bpp.to_le_bytes());
    ih[16..20].copy_from_slice(&comp.to_le_bytes());
    ih[20..24].copy_from_slice(&(data.len() as u32).to_le_bytes());
    ih[32..36].copy_from_slice(&(palette.len() as u32).to_le_bytes());
    if hdr > 40 {
        for (i, m) in masks.iter().enumerate() {
            ih[40 + i * 4..44 + i * 4].copy_from_slice(&m.to_le_bytes());
        }
    }
    b.extend_from_slice(&ih);
    if extra > 0 {
        for m in masks {
            b.extend_from_slice(&m.to_le_bytes());
        }
    }
    for p in palette {
        b.extend_from_slice(p);
    }
    b.extend_from_slice(data);
    b
}

#[test]
fn bmp_24_bit_is_opaque_and_bottom_up() {
    // 1x2: bottom row blue-ish first, rows padded to 4 bytes
    let b = bmp(40, 1, 2, 24, 0, &[], &[], &[10, 20, 30, 0, 40, 50, 60, 0]);
    let img = dec(&b, "a.bmp");
    assert_eq!(px(&img, 0, 0), [60, 50, 40, 255]);
    assert_eq!(px(&img, 0, 1), [30, 20, 10, 255]);
    assert!(!img.has_alpha);
}

#[test]
fn bmp_top_down() {
    let b = bmp(40, 1, -2, 24, 0, &[], &[], &[10, 20, 30, 0, 40, 50, 60, 0]);
    let img = dec(&b, "a.bmp");
    assert_eq!(px(&img, 0, 0), [30, 20, 10, 255]);
}

#[test]
fn bmp_32_bit_alpha() {
    // the fourth byte is alpha as soon as one is not zero
    let b = bmp(40, 2, 1, 32, 0, &[], &[], &[1, 2, 3, 0, 4, 5, 6, 128]);
    let img = dec(&b, "a.bmp");
    assert_eq!(px(&img, 0, 0), [3, 2, 1, 0]);
    assert_eq!(px(&img, 1, 0), [6, 5, 4, 128]);
    assert!(img.has_alpha);
    // all zero: X8R8G8B8, opaque
    let b = bmp(40, 2, 1, 32, 0, &[], &[], &[1, 2, 3, 0, 4, 5, 6, 0]);
    let img = dec(&b, "a.bmp");
    assert_eq!(px(&img, 1, 0), [6, 5, 4, 255]);
    assert!(!img.has_alpha);
}

#[test]
fn bmp_32_bit_v5_alpha_mask() {
    let m = [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000];
    let b = bmp(124, 2, 1, 32, 3, &m, &[], &[1, 2, 3, 7, 4, 5, 6, 200]);
    let img = dec(&b, "a.bmp");
    assert_eq!(px(&img, 0, 0), [3, 2, 1, 7]);
    assert_eq!(px(&img, 1, 0), [6, 5, 4, 200]);
    assert!(img.has_alpha);
}

#[test]
fn bmp_32_bit_bitfields_without_alpha_mask() {
    let m = [0x00FF_0000, 0x0000_FF00, 0x0000_00FF];
    let b = bmp(40, 1, 1, 32, 3, &m, &[], &[1, 2, 3, 0]);
    let img = dec(&b, "a.bmp");
    assert_eq!(px(&img, 0, 0), [3, 2, 1, 255]);
}

#[test]
fn bmp_16_bit_555() {
    // X1R5G5B5: r=31 g=0 b=16
    let v: u16 = (31 << 10) | 16;
    let mut d = v.to_le_bytes().to_vec();
    d.extend_from_slice(&[0, 0]);
    let img = dec(&bmp(40, 1, 1, 16, 0, &[], &[], &d), "a.bmp");
    let p = px(&img, 0, 0);
    assert_eq!((p[0], p[1], p[3]), (255, 0, 255));
    assert!((p[2] as i32 - 132).abs() <= 1, "{p:?}");
}

#[test]
fn bmp_16_bit_565_bitfields() {
    let v: u16 = (31 << 11) | (63 << 5);
    let mut d = v.to_le_bytes().to_vec();
    d.extend_from_slice(&[0, 0]);
    let img = dec(
        &bmp(40, 1, 1, 16, 3, &[0xF800, 0x07E0, 0x001F], &[], &d),
        "a.bmp",
    );
    assert_eq!(px(&img, 0, 0), [255, 255, 0, 255]);
}

#[test]
fn bmp_16_bit_4444_and_1555() {
    // A4R4G4B4 in a V4 header: a=8 r=15 g=0 b=0
    let v: u16 = (8 << 12) | (15 << 8);
    let mut d = v.to_le_bytes().to_vec();
    d.extend_from_slice(&[0, 0]);
    let img = dec(
        &bmp(108, 1, 1, 16, 3, &[0x0F00, 0x00F0, 0x000F, 0xF000], &[], &d),
        "a.bmp",
    );
    let p = px(&img, 0, 0);
    assert_eq!((p[0], p[1], p[2]), (255, 0, 0));
    assert!((p[3] as i32 - 136).abs() <= 1, "{p:?}");
    assert!(img.has_alpha);
    // A1R5G5B5 with the alpha bit clear
    let v: u16 = 31;
    let mut d = v.to_le_bytes().to_vec();
    d.extend_from_slice(&[0, 0]);
    let img = dec(
        &bmp(108, 1, 1, 16, 3, &[0x7C00, 0x03E0, 0x001F, 0x8000], &[], &d),
        "a.bmp",
    );
    assert_eq!(px(&img, 0, 0), [0, 0, 255, 0]);
}

#[test]
fn bmp_palettes() {
    let pal = [[0, 0, 255, 0], [0, 255, 0, 0], [255, 0, 0, 0], [9, 9, 9, 0]];
    // 8-bit, 2x1
    let img = dec(&bmp(40, 2, 1, 8, 0, &[], &pal, &[2, 0, 0, 0]), "a.bmp");
    assert_eq!(px(&img, 0, 0), [0, 0, 255, 255]);
    assert_eq!(px(&img, 1, 0), [255, 0, 0, 255]);
    assert!(!img.has_alpha);
    // 4-bit: indices 1, 2
    let img = dec(&bmp(40, 2, 1, 4, 0, &[], &pal, &[0x12, 0, 0, 0]), "a.bmp");
    assert_eq!(px(&img, 0, 0), [0, 255, 0, 255]);
    assert_eq!(px(&img, 1, 0), [0, 0, 255, 255]);
    // 1-bit: 1, 0
    let img = dec(
        &bmp(40, 2, 1, 1, 0, &[], &pal[..2], &[0x80, 0, 0, 0]),
        "a.bmp",
    );
    assert_eq!(px(&img, 0, 0), [0, 255, 0, 255]);
    assert_eq!(px(&img, 1, 0), [255, 0, 0, 255]);
}

#[test]
fn bmp_rle() {
    let pal = [[0, 0, 255, 0], [0, 255, 0, 0], [255, 0, 0, 0]];
    // RLE8 4x1: run of 3 x index 1, literal-ish run of 1 x index 2, end of line, end
    let img = dec(
        &bmp(40, 4, 1, 8, 1, &[], &pal, &[3, 1, 1, 2, 0, 0, 0, 1]),
        "a.bmp",
    );
    assert_eq!(px(&img, 0, 0), [0, 255, 0, 255]);
    assert_eq!(px(&img, 2, 0), [0, 255, 0, 255]);
    assert_eq!(px(&img, 3, 0), [0, 0, 255, 255]);
    // RLE4 4x1: 4 pixels alternating 1, 2
    let img = dec(
        &bmp(40, 4, 1, 4, 2, &[], &pal, &[4, 0x12, 0, 0, 0, 1]),
        "a.bmp",
    );
    assert_eq!(px(&img, 0, 0), [0, 255, 0, 255]);
    assert_eq!(px(&img, 1, 0), [0, 0, 255, 255]);
    assert_eq!(px(&img, 3, 0), [0, 0, 255, 255]);
}

// ---------------------------------------------------------------- TGA

fn tga(
    img_type: u8,
    bpp: u8,
    desc: u8,
    w: u16,
    h: u16,
    cmap: Option<(u8, &[u8])>,
    data: &[u8],
) -> Vec<u8> {
    let mut b = vec![0u8; 18];
    b[2] = img_type;
    if let Some((cbpp, entries)) = cmap {
        b[1] = 1;
        let n = entries.len() / ((cbpp as usize + 7) / 8);
        b[5..7].copy_from_slice(&(n as u16).to_le_bytes());
        b[7] = cbpp;
    }
    b[12..14].copy_from_slice(&w.to_le_bytes());
    b[14..16].copy_from_slice(&h.to_le_bytes());
    b[16] = bpp;
    b[17] = desc;
    if let Some((_, e)) = cmap {
        b.extend_from_slice(e);
    }
    b.extend_from_slice(data);
    b
}

#[test]
fn tga_true_colour() {
    // 24-bit, bottom-up 1x2
    let img = dec(
        &tga(2, 24, 0, 1, 2, None, &[10, 20, 30, 40, 50, 60]),
        "a.tga",
    );
    assert_eq!(px(&img, 0, 0), [60, 50, 40, 255]);
    assert!(!img.has_alpha);
    // 32-bit, top-down
    let img = dec(
        &tga(2, 32, 0x28, 1, 2, None, &[10, 20, 30, 40, 50, 60, 70, 80]),
        "a.tga",
    );
    assert_eq!(px(&img, 0, 0), [30, 20, 10, 40]);
    assert!(img.has_alpha);
    // RLE 32-bit: a run of 2
    let img = dec(&tga(10, 32, 0x28, 2, 1, None, &[0x81, 1, 2, 3, 4]), "a.tga");
    assert_eq!(px(&img, 1, 0), [3, 2, 1, 4]);
    // right-to-left
    let img = dec(&tga(2, 24, 0x30, 2, 1, None, &[1, 2, 3, 4, 5, 6]), "a.tga");
    assert_eq!(px(&img, 0, 0), [6, 5, 4, 255]);
}

#[test]
fn tga_16_bit() {
    // A1R5G5B5 with one bit of alpha in the descriptor: the bit is the alpha
    let clear: u16 = 31 << 10;
    let set: u16 = 0x8000 | 31;
    let mut d = clear.to_le_bytes().to_vec();
    d.extend_from_slice(&set.to_le_bytes());
    let img = dec(&tga(2, 16, 0x21, 2, 1, None, &d), "a.tga");
    assert_eq!(px(&img, 0, 0), [255, 0, 0, 0]);
    assert_eq!(px(&img, 1, 0), [0, 0, 255, 255]);
    assert!(img.has_alpha);
    // D3DX goes by the depth alone: 16 bits are A1R5G5B5 even without alpha bits in the
    // descriptor, 15 bits X1R5G5B5
    let img = dec(&tga(2, 16, 0x20, 2, 1, None, &d), "a.tga");
    assert_eq!(px(&img, 0, 0), [255, 0, 0, 0]);
    let img = dec(&tga(2, 15, 0x20, 2, 1, None, &d), "a.tga");
    assert_eq!(px(&img, 0, 0), [255, 0, 0, 255]);
    assert!(!img.has_alpha);
}

#[test]
fn tga_colour_mapped_and_grey() {
    let pal = [0u8, 0, 255, 0, 255, 0];
    let img = dec(&tga(1, 8, 0x20, 2, 1, Some((24, &pal)), &[1, 0]), "a.tga");
    assert_eq!(px(&img, 0, 0), [0, 255, 0, 255]);
    assert_eq!(px(&img, 1, 0), [255, 0, 0, 255]);
    // RLE colour-mapped
    let img = dec(
        &tga(9, 8, 0x20, 2, 1, Some((24, &pal)), &[0x81, 1]),
        "a.tga",
    );
    assert_eq!(px(&img, 1, 0), [0, 255, 0, 255]);
    // 32-bit palette carries alpha
    let pal32 = [0u8, 0, 255, 77];
    let img = dec(&tga(1, 8, 0x28, 1, 1, Some((32, &pal32)), &[0]), "a.tga");
    assert_eq!(px(&img, 0, 0), [255, 0, 0, 77]);
    // grey 8 and grey + alpha 16
    let img = dec(&tga(3, 8, 0x20, 1, 1, None, &[99]), "a.tga");
    assert_eq!(px(&img, 0, 0), [99, 99, 99, 255]);
    assert!(!img.has_alpha);
    let img = dec(&tga(11, 16, 0x28, 2, 1, None, &[0x81, 50, 60]), "a.tga");
    assert_eq!(px(&img, 1, 0), [50, 50, 50, 60]);
}

// ---------------------------------------------------------------- PNG

fn crc32(d: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in d {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

/// A PNG of one IDAT, stored (not deflated); `rows` are the raw scanlines without filter bytes.
fn png(
    w: u32,
    h: u32,
    depth: u8,
    colour: u8,
    extra: &[(&[u8; 4], Vec<u8>)],
    rows: &[Vec<u8>],
) -> Vec<u8> {
    let chunk = |out: &mut Vec<u8>, t: &[u8; 4], d: &[u8]| {
        out.extend_from_slice(&(d.len() as u32).to_be_bytes());
        let mut c = t.to_vec();
        c.extend_from_slice(d);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    };
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = w.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[depth, colour, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    for (t, d) in extra {
        chunk(&mut out, t, d);
    }
    let raw: Vec<u8> = rows
        .iter()
        .flat_map(|r| std::iter::once(0u8).chain(r.iter().copied()))
        .collect();
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    let mut z = vec![0x78, 0x01, 1];
    z.extend_from_slice(&(raw.len() as u16).to_le_bytes());
    z.extend_from_slice(&(!(raw.len() as u16)).to_le_bytes());
    z.extend_from_slice(&raw);
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[test]
fn png_colour_types() {
    let img = dec(&png(1, 1, 8, 2, &[], &[vec![1, 2, 3]]), "a.png");
    assert_eq!(px(&img, 0, 0), [1, 2, 3, 255]);
    assert!(!img.has_alpha);
    let img = dec(&png(1, 1, 8, 6, &[], &[vec![1, 2, 3, 4]]), "a.png");
    assert_eq!(px(&img, 0, 0), [1, 2, 3, 4]);
    assert!(img.has_alpha);
    let img = dec(&png(1, 1, 8, 0, &[], &[vec![77]]), "a.png");
    assert_eq!(px(&img, 0, 0), [77, 77, 77, 255]);
    let img = dec(&png(1, 1, 8, 4, &[], &[vec![77, 9]]), "a.png");
    assert_eq!(px(&img, 0, 0), [77, 77, 77, 9]);
    assert!(img.has_alpha);
    // 16 bits a channel
    let img = dec(
        &png(
            1,
            1,
            16,
            6,
            &[],
            &[vec![0xFF, 0xFF, 0x80, 0x00, 0, 0, 0x40, 0x00]],
        ),
        "a.png",
    );
    assert_eq!(px(&img, 0, 0), [255, 128, 0, 64]);
    let img = dec(&png(1, 1, 16, 0, &[], &[vec![0x80, 0x00]]), "a.png");
    assert_eq!(px(&img, 0, 0)[..3], [128, 128, 128]);
    // grey of 1, 2 and 4 bits
    let img = dec(&png(2, 1, 1, 0, &[], &[vec![0x80]]), "a.png");
    assert_eq!((px(&img, 0, 0)[0], px(&img, 1, 0)[0]), (255, 0));
    let img = dec(&png(2, 1, 4, 0, &[], &[vec![0xF0]]), "a.png");
    assert_eq!((px(&img, 0, 0)[0], px(&img, 1, 0)[0]), (255, 0));
}

#[test]
fn png_palette_and_trns() {
    let plte = vec![255, 0, 0, 0, 0, 255];
    let img = dec(
        &png(2, 1, 8, 3, &[(b"PLTE", plte.clone())], &[vec![0, 1]]),
        "a.png",
    );
    assert_eq!(px(&img, 0, 0), [255, 0, 0, 255]);
    assert_eq!(px(&img, 1, 0), [0, 0, 255, 255]);
    let img = dec(
        &png(
            2,
            1,
            8,
            3,
            &[(b"PLTE", plte.clone()), (b"tRNS", vec![0])],
            &[vec![0, 1]],
        ),
        "a.png",
    );
    assert_eq!(px(&img, 0, 0), [255, 0, 0, 0]);
    assert_eq!(px(&img, 1, 0), [0, 0, 255, 255]);
    assert!(img.has_alpha);
    // 4-bit palette
    let img = dec(&png(2, 1, 4, 3, &[(b"PLTE", plte)], &[vec![0x10]]), "a.png");
    assert_eq!(px(&img, 0, 0), [0, 0, 255, 255]);
    // tRNS colour key on RGB
    let img = dec(
        &png(
            2,
            1,
            8,
            2,
            &[(b"tRNS", vec![0, 1, 0, 2, 0, 3])],
            &[vec![1, 2, 3, 4, 5, 6]],
        ),
        "a.png",
    );
    assert_eq!(px(&img, 0, 0)[3], 0);
    assert_eq!(px(&img, 1, 0), [4, 5, 6, 255]);
}

// ---------------------------------------------------------------- JPEG

#[test]
fn jpeg_is_opaque() {
    let mut out = Vec::new();
    let rgb = image::RgbImage::from_pixel(8, 8, image::Rgb([200, 100, 50]));
    image::DynamicImage::ImageRgb8(rgb)
        .write_to(
            &mut std::io::Cursor::new(&mut out),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
    let img = dec(&out, "a.jpg");
    let p = px(&img, 4, 4);
    assert!(
        (p[0] as i32 - 200).abs() <= 3
            && (p[1] as i32 - 100).abs() <= 3
            && (p[2] as i32 - 50).abs() <= 3,
        "{p:?}"
    );
    assert_eq!(p[3], 255);
    assert!(!img.has_alpha);
    // grey JPEG
    let mut out = Vec::new();
    image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(8, 8, image::Luma([90])))
        .write_to(
            &mut std::io::Cursor::new(&mut out),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
    let p = px(&dec(&out, "a.jpg"), 0, 0);
    assert!((p[0] as i32 - 90).abs() <= 2 && p[3] == 255, "{p:?}");
}

// ---------------------------------------------------------------- DDS

fn dds_plain(
    w: u32,
    h: u32,
    pf_flags: u32,
    bpp: u32,
    masks: [u32; 4],
    mips: u32,
    data: &[u8],
) -> Vec<u8> {
    let mut b = vec![0u8; 128];
    b[..4].copy_from_slice(b"DDS ");
    b[4..8].copy_from_slice(&124u32.to_le_bytes());
    b[8..12].copy_from_slice(&(0x100Fu32 | if mips > 1 { 0x20000 } else { 0 }).to_le_bytes());
    b[12..16].copy_from_slice(&h.to_le_bytes());
    b[16..20].copy_from_slice(&w.to_le_bytes());
    b[28..32].copy_from_slice(&mips.to_le_bytes());
    b[76..80].copy_from_slice(&32u32.to_le_bytes());
    b[80..84].copy_from_slice(&pf_flags.to_le_bytes());
    b[88..92].copy_from_slice(&bpp.to_le_bytes());
    for (i, m) in masks.iter().enumerate() {
        b[92 + i * 4..96 + i * 4].copy_from_slice(&m.to_le_bytes());
    }
    b.extend_from_slice(data);
    b
}

#[test]
fn dds_uncompressed_formats() {
    const RGB: u32 = 0x40;
    const ALPHA: u32 = 0x1;
    // A8R8G8B8 with a mip level after it
    let img = dec(
        &dds_plain(
            1,
            1,
            RGB | ALPHA,
            32,
            [0xFF0000, 0xFF00, 0xFF, 0xFF00_0000],
            1,
            &[1, 2, 3, 4],
        ),
        "a.dds",
    );
    assert_eq!(px(&img, 0, 0), [3, 2, 1, 4]);
    assert!(img.has_alpha);
    let img = dec(
        &dds_plain(
            2,
            2,
            RGB | ALPHA,
            32,
            [0xFF0000, 0xFF00, 0xFF, 0xFF00_0000],
            2,
            &[1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 9, 9, 9, 9],
        ),
        "a.dds",
    );
    assert_eq!((img.width, px(&img, 1, 1)), (2, [3, 2, 1, 4]));
    // X8R8G8B8: the fourth byte is not alpha
    let img = dec(
        &dds_plain(1, 1, RGB, 32, [0xFF0000, 0xFF00, 0xFF, 0], 1, &[1, 2, 3, 0]),
        "a.dds",
    );
    assert_eq!(px(&img, 0, 0), [3, 2, 1, 255]);
    assert!(!img.has_alpha);
    // A8B8G8R8
    let img = dec(
        &dds_plain(
            1,
            1,
            RGB | ALPHA,
            32,
            [0xFF, 0xFF00, 0xFF0000, 0xFF00_0000],
            1,
            &[1, 2, 3, 4],
        ),
        "a.dds",
    );
    assert_eq!(px(&img, 0, 0), [1, 2, 3, 4]);
    // R8G8B8
    let img = dec(
        &dds_plain(1, 1, RGB, 24, [0xFF0000, 0xFF00, 0xFF, 0], 1, &[1, 2, 3, 0]),
        "a.dds",
    );
    assert_eq!(px(&img, 0, 0), [3, 2, 1, 255]);
    // R5G6B5
    let img = dec(
        &dds_plain(
            1,
            1,
            RGB,
            16,
            [0xF800, 0x07E0, 0x001F, 0],
            1,
            &((31u16 << 11) | 31).to_le_bytes(),
        ),
        "a.dds",
    );
    assert_eq!(px(&img, 0, 0), [255, 0, 255, 255]);
    // A1R5G5B5 (alpha bit clear) and X1R5G5B5 (the bit ignored)
    let img = dec(
        &dds_plain(
            1,
            1,
            RGB | ALPHA,
            16,
            [0x7C00, 0x03E0, 0x001F, 0x8000],
            1,
            &(31u16 << 5).to_le_bytes(),
        ),
        "a.dds",
    );
    assert_eq!(px(&img, 0, 0), [0, 255, 0, 0]);
    let img = dec(
        &dds_plain(
            1,
            1,
            RGB,
            16,
            [0x7C00, 0x03E0, 0x001F, 0],
            1,
            &(31u16 << 5).to_le_bytes(),
        ),
        "a.dds",
    );
    assert_eq!(px(&img, 0, 0), [0, 255, 0, 255]);
    // A4R4G4B4
    let img = dec(
        &dds_plain(
            1,
            1,
            RGB | ALPHA,
            16,
            [0x0F00, 0x00F0, 0x000F, 0xF000],
            1,
            &0xF00Fu16.to_le_bytes(),
        ),
        "a.dds",
    );
    assert_eq!(px(&img, 0, 0), [0, 0, 255, 255]);
    // L8, A8L8, A8
    let img = dec(
        &dds_plain(1, 1, 0x20000, 8, [0xFF, 0, 0, 0], 1, &[70]),
        "a.dds",
    );
    assert_eq!(px(&img, 0, 0), [70, 70, 70, 255]);
    assert!(!img.has_alpha);
    let img = dec(
        &dds_plain(
            1,
            1,
            0x20000 | ALPHA,
            16,
            [0xFF, 0, 0, 0xFF00],
            1,
            &[70, 30],
        ),
        "a.dds",
    );
    assert_eq!(px(&img, 0, 0), [70, 70, 70, 30]);
    assert!(img.has_alpha);
    let img = dec(&dds_plain(1, 1, 0x2, 8, [0, 0, 0, 0xFF], 1, &[30]), "a.dds");
    assert_eq!(px(&img, 0, 0)[3], 30);
}

#[test]
fn dds_block_formats() {
    let fourcc = |cc: &[u8; 4], blocks: &[u8]| {
        let mut b = dds_plain(4, 4, 0x4, 0, [0; 4], 1, blocks);
        b[84..88].copy_from_slice(cc);
        b
    };
    // DXT1: c0 red > c1 blue, all index 0
    let img = dec(
        &fourcc(b"DXT1", &[0x00, 0xF8, 0x1F, 0x00, 0, 0, 0, 0]),
        "a.dds",
    );
    assert_eq!(px(&img, 3, 3), [255, 0, 0, 255]);
    assert!(!img.has_alpha);
    // DXT1 with a transparent index (c0 <= c1, index 3)
    let img = dec(
        &fourcc(b"DXT1", &[0x1F, 0x00, 0x00, 0xF8, 0xFF, 0xFF, 0xFF, 0xFF]),
        "a.dds",
    );
    assert_eq!(px(&img, 0, 0)[3], 0);
    assert!(img.has_alpha);
    // DXT3: alpha nibble 8 -> 136
    let mut b3 = vec![0x88u8; 8];
    b3.extend_from_slice(&[0x00, 0xF8, 0x1F, 0x00, 0, 0, 0, 0]);
    let img = dec(&fourcc(b"DXT3", &b3), "a.dds");
    assert_eq!(px(&img, 1, 1), [255, 0, 0, 136]);
    // DXT5: a0 = 200, every index 0
    let mut b5 = vec![200u8, 10, 0, 0, 0, 0, 0, 0];
    b5.extend_from_slice(&[0x00, 0xF8, 0x1F, 0x00, 0, 0, 0, 0]);
    let img = dec(&fourcc(b"DXT5", &b5), "a.dds");
    assert_eq!(px(&img, 2, 2), [255, 0, 0, 200]);
}

// ---------------------------------------------------------------- content sniffing

#[test]
fn files_are_read_by_content_not_by_name() {
    // a BMP named .dds (the repaint tool's liveries), a PNG named .bmp, a TGA named .png
    let b = bmp(40, 1, 1, 24, 0, &[], &[], &[10, 20, 30, 0]);
    assert_eq!(px(&dec(&b, "x.dds"), 0, 0), [30, 20, 10, 255]);
    let p = png(1, 1, 8, 6, &[], &[vec![1, 2, 3, 4]]);
    assert_eq!(px(&dec(&p, "x.bmp"), 0, 0), [1, 2, 3, 4]);
    assert_eq!(px(&dec(&p, "x.tga"), 0, 0), [1, 2, 3, 4]);
    let t = tga(2, 32, 0x28, 1, 1, None, &[1, 2, 3, 4]);
    assert_eq!(px(&dec(&t, "x.png"), 0, 0), [3, 2, 1, 4]);
    assert_eq!(px(&dec(&t, "x.dds"), 0, 0), [3, 2, 1, 4]);
}

/// The picture formats beyond OMSI's: each read by content, even named as something else.
#[test]
fn modern_picture_formats_are_read_by_content() {
    use image::{ImageFormat as F, RgbaImage};
    let src = RgbaImage::from_fn(3, 2, |x, y| {
        image::Rgba([x as u8 * 80, y as u8 * 100, 50, 255])
    });
    for f in [
        F::WebP,
        F::Gif,
        F::Tiff,
        F::Qoi,
        F::Pnm,
        F::OpenExr,
        F::Ico,
        F::Png,
    ] {
        let mut bytes = std::io::Cursor::new(Vec::new());
        let dynimg = if matches!(f, F::Pnm) {
            image::DynamicImage::ImageRgb8(image::DynamicImage::ImageRgba8(src.clone()).to_rgb8())
        } else if matches!(f, F::OpenExr) {
            image::DynamicImage::ImageRgba32F(
                image::DynamicImage::ImageRgba8(src.clone()).to_rgba32f(),
            )
        } else {
            image::DynamicImage::ImageRgba8(src.clone())
        };
        dynimg
            .write_to(&mut bytes, f)
            .unwrap_or_else(|e| panic!("{f:?}: {e}"));
        let img = crate::decode_bytes(bytes.get_ref(), std::path::Path::new("misnamed.bmp"))
            .unwrap_or_else(|e| panic!("{f:?}: {e}"));
        assert_eq!((img.width, img.height), (3, 2), "{f:?}");
        // (GIF has a palette, EXR is linear floats: near enough)
        let p = &img.rgba[(1 * 3 + 2) * 4..][..3];
        assert!(
            (p[0] as i32 - 160).abs() <= 3 && (p[1] as i32 - 100).abs() <= 3,
            "{f:?}: {p:?}"
        );
    }
}

/// A 16-bit PNG (photos and height maps of today's tools) comes in at 8 bits per channel.
#[test]
fn a_16_bit_png_is_read() {
    let src = image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::from_fn(2, 2, |x, _| {
        image::Rgba([x as u16 * 65535, 32768, 0, 65535])
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba16(src)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let img = crate::decode_bytes(bytes.get_ref(), std::path::Path::new("h.png")).unwrap();
    assert_eq!(&img.rgba[4..8], &[255, 128, 0, 255]);
}

/// The DX10 header's BC7 and 10-bit formats.
#[test]
fn dx10_bc7_and_r10g10b10a2() {
    let dds = |dxgi: u32, w: u32, h: u32, data: &[u8]| {
        let mut b = vec![0u8; 148];
        b[..4].copy_from_slice(b"DDS ");
        b[12..16].copy_from_slice(&h.to_le_bytes());
        b[16..20].copy_from_slice(&w.to_le_bytes());
        b[80..84].copy_from_slice(&4u32.to_le_bytes());
        b[84..88].copy_from_slice(b"DX10");
        b[128..132].copy_from_slice(&dxgi.to_le_bytes());
        b.extend_from_slice(data);
        b
    };
    let img = crate::dds::decode(&dds(98, 4, 4, &[0u8; 16])).unwrap();
    assert_eq!(img.width, 4);
    // R10G10B10A2: r = 1023, g = 0, b = 512, a = 3
    let v: u32 = 1023 | (512 << 20) | (3 << 30);
    let img = crate::dds::decode(&dds(24, 1, 1, &v.to_le_bytes())).unwrap();
    assert_eq!(img.rgba, [255, 0, 127, 255]);
    assert!(
        crate::dds::decode(&dds(83, 4, 4, &[0u8; 16])).is_ok(),
        "BC5"
    );
}
