//! TGA decoder as lenient as D3DX: true-colour and grey images, colour-mapped ones, RLE or
//! not, any bit depth 8/15/16/24/32, ignoring inconsistent descriptor alpha bits.

use crate::Image;

pub fn decode(b: &[u8]) -> Result<Image, String> {
    if b.len() < 18 {
        return Err("too short".into());
    }
    let id_len = b[0] as usize;
    let cmap_type = b[1];
    let img_type = b[2];
    let cmap_start = u16::from_le_bytes([b[3], b[4]]) as usize;
    let cmap_len = u16::from_le_bytes([b[5], b[6]]) as usize;
    let cmap_bpp = b[7] as usize;
    let width = u16::from_le_bytes([b[12], b[13]]) as usize;
    let height = u16::from_le_bytes([b[14], b[15]]) as usize;
    let bpp = b[16] as usize;
    let desc = b[17];
    let top_down = desc & 0x20 != 0;
    let right_left = desc & 0x10 != 0;
    if width == 0 || height == 0 {
        return Err("empty image".into());
    }
    let mut pos = 18 + id_len;
    let cmap_bytes = (cmap_bpp + 7) / 8;
    let cmap: &[u8] = if cmap_type != 0 {
        let n = cmap_len * cmap_bytes;
        if b.len() < pos + n {
            return Err("truncated colour map".into());
        }
        let c = &b[pos..pos + n];
        pos += n;
        c
    } else {
        &[]
    };
    let base = img_type & 7;
    let rle = img_type & 8 != 0;
    if !(1..=3).contains(&base) {
        return Err(format!("unsupported image type {img_type}"));
    }
    let px_bytes = (bpp + 7) / 8;
    // (a pixel or palette entry of no bytes has no colour to read; a picture wider than
    // any texture is a damaged header, not something to allocate gigabytes for)
    if px_bytes == 0 || px_bytes > 4 || (base == 1 && !(1..=4).contains(&cmap_bytes)) {
        return Err(format!(
            "unsupported pixel depth {bpp} (palette {cmap_bpp})"
        ));
    }
    if width > crate::MAX_DIMENSION || height > crate::MAX_DIMENSION {
        return Err(format!("{width}x{height} is larger than any texture"));
    }
    let n = width * height;
    // read raw pixel values (px_bytes each)
    let mut raw = Vec::with_capacity(n * px_bytes);
    let src = &b[pos.min(b.len())..];
    if rle {
        let mut i = 0;
        while raw.len() < n * px_bytes && i < src.len() {
            let h = src[i];
            i += 1;
            let count = (h & 0x7f) as usize + 1;
            if h & 0x80 != 0 {
                if i + px_bytes > src.len() {
                    break;
                }
                for _ in 0..count {
                    raw.extend_from_slice(&src[i..i + px_bytes]);
                }
                i += px_bytes;
            } else {
                let take = (count * px_bytes).min(src.len() - i);
                raw.extend_from_slice(&src[i..i + take]);
                i += take;
            }
        }
    } else {
        raw.extend_from_slice(&src[..(n * px_bytes).min(src.len())]);
    }
    raw.resize(n * px_bytes, 0);
    let to_rgba = |p: &[u8], bits: usize| -> [u8; 4] {
        match bits {
            32 => [p[2], p[1], p[0], p[3]],
            24 => [p[2], p[1], p[0], 255],
            15 | 16 => {
                let v = u16::from_le_bytes([p[0], p[1]]);
                let r = ((v >> 10) & 31) as u32;
                let g = ((v >> 5) & 31) as u32;
                let bl = (v & 31) as u32;
                // D3DX picks its format by the depth alone: 16 bits are A1R5G5B5 (the top
                // bit is the alpha, whatever the descriptor says), 15 bits X1R5G5B5
                let a = if bits == 16 && v & 0x8000 == 0 {
                    0
                } else {
                    255
                };
                [
                    ((r * 255) / 31) as u8,
                    ((g * 255) / 31) as u8,
                    ((bl * 255) / 31) as u8,
                    a,
                ]
            }
            _ => [p[0], p[0], p[0], 255],
        }
    };
    let mut rgba = vec![0u8; n * 4];
    let has_alpha = (base == 2 && matches!(bpp, 16 | 32))
        || (base == 1 && matches!(cmap_bpp, 16 | 32))
        || (base == 3 && px_bytes == 2);
    for y in 0..height {
        for x in 0..width {
            let i = y * width + x;
            let p = &raw[i * px_bytes..][..px_bytes];
            let c = match base {
                1 => {
                    let mut idx = 0usize;
                    for k in 0..px_bytes {
                        idx |= (p[k] as usize) << (8 * k);
                    }
                    let idx = idx.saturating_sub(cmap_start);
                    match (idx < cmap_len)
                        .then(|| cmap.get(idx * cmap_bytes..(idx + 1) * cmap_bytes))
                        .flatten()
                    {
                        Some(entry) => to_rgba(entry, cmap_bpp),
                        None => [0, 0, 0, 255],
                    }
                }
                2 => to_rgba(p, bpp),
                _ => [p[0], p[0], p[0], if px_bytes == 2 { p[1] } else { 255 }],
            };
            let dx = if right_left { width - 1 - x } else { x };
            let dy = if top_down { y } else { height - 1 - y };
            rgba[(dy * width + dx) * 4..][..4].copy_from_slice(&c);
        }
    }
    Ok(Image {
        width: width as u32,
        height: height as u32,
        rgba,
        has_alpha,
    })
}

#[cfg(test)]
mod damaged_tests {
    #[test]
    fn a_palette_of_no_depth_is_an_error() {
        // colour-mapped, 1 entry of 0 bits, 1x1 picture of 8 bits
        let mut b = vec![0u8, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 8, 0];
        b.push(0);
        assert!(super::decode(&b).is_err());
    }

    #[test]
    fn a_colour_mapped_picture_without_its_colour_map_is_black() {
        let b = [0u8, 0, 1, 0, 0, 1, 0, 8, 0, 0, 0, 0, 1, 0, 1, 0, 8, 0, 0];
        let img = super::decode(&b).unwrap();
        assert_eq!(img.rgba, [0, 0, 0, 255]);
    }
}
