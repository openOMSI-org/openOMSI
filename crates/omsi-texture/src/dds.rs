//! DDS decoder covering what D3DX accepts in the original - uncompressed formats with
//! arbitrary channel masks (A8R8G8B8, X8R8G8B8, R5G6B5, A1R5G5B5, A4R4G4B4, R8G8B8, A8, L8,
//! A8L8) and the BC1/BC2/BC3 (DXT1/3/5) block formats - and what newer tools write: the
//! DX10 header's DXGI formats (BC4-BC7, 10- and 16-bit and float channels, packed 16-bit
//! colours), the ATI1/ATI2 and BC4U/BC5U fourccs, RXGB normal maps, Direct3D's float
//! formats. Only the top mip level is used.

use crate::Image;
use crate::blocks::{self, Block};

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Decode a DDS file. Returns None when the header is not a DDS one.
pub fn decode(bytes: &[u8]) -> Result<Image, String> {
    if bytes.len() < 128 || &bytes[..4] != b"DDS " {
        return Err("not a DDS file".into());
    }
    let height = u32_at(bytes, 12) as usize;
    let width = u32_at(bytes, 16) as usize;
    let pf_flags = u32_at(bytes, 80);
    let fourcc = &bytes[84..88];
    let bpp = u32_at(bytes, 88) as usize;
    let masks = [
        u32_at(bytes, 92),
        u32_at(bytes, 96),
        u32_at(bytes, 100),
        u32_at(bytes, 104),
    ];
    let mut data_start = 128;
    if fourcc == b"DX10" {
        data_start += 20;
    }
    let data = &bytes[data_start.min(bytes.len())..];
    if width == 0 || height == 0 {
        return Err("empty image".into());
    }
    // a cube map (six faces of the same size, one after the other): OMSI's reflections are
    // sphere maps, so the faces are turned into one (the first face alone had been taken)
    let caps2 = u32_at(bytes, 112);
    // (a DX10 header says it with its misc flag, TEXTURECUBE, and six array slices)
    let dx10_cube = fourcc == b"DX10"
        && bytes.len() >= 148
        && u32_at(bytes, 136) & 4 != 0
        && u32_at(bytes, 140) <= 1;
    if (caps2 & 0x200 != 0 && caps2 & 0xFC00 == 0xFC00 || dx10_cube) && data.len() >= 6 {
        let face_len = data.len() / 6;
        let mut faces = Vec::with_capacity(6);
        for k in 0..6 {
            let mut one = bytes[..data_start].to_vec();
            one[112..116].copy_from_slice(&0u32.to_le_bytes());
            if one.len() >= 148 && &one[84..88] == b"DX10" {
                one[136..140].copy_from_slice(&0u32.to_le_bytes());
            }
            one.extend_from_slice(&data[k * face_len..(k + 1) * face_len]);
            faces.push(decode(&one)?);
        }
        return Ok(cube_to_sphere(&faces, width.clamp(64, 512)));
    }
    // (checked before the picture is allocated: a damaged header asked for gigabytes)
    if width > crate::MAX_DIMENSION || height > crate::MAX_DIMENSION {
        return Err(format!("{width}x{height} is larger than any texture"));
    }
    let mut rgba = vec![0u8; width * height * 4];
    if pf_flags & 0x4 != 0 {
        // D3DFORMAT numbers in the fourcc: the wide RGBA formats D3DX writes for float or
        // 16-bit pictures (a Citaro repaint's `lublins.dds` is A32B32G32R32F)
        if let Some(img) = wide_rgba(u32_at(bytes, 84), data, width, height) {
            return img;
        }
        // the DX10 header names a DXGI format: the block formats as their DXT names, the
        // plain ones read as they lie (D3DX 9 itself knows none of them; newer tools write
        // them for mods)
        let dxgi = (fourcc == b"DX10" && bytes.len() >= 148).then(|| u32_at(bytes, 128));
        if let Some(img) = dxgi.and_then(|f| dxgi_plain(f, data, width, height)) {
            return img;
        }
        // the block formats of newer tools
        let modern = match (fourcc, dxgi) {
            (b"ATI1" | b"BC4U" | b"BC4S", _) | (_, Some(79..=81)) => Some(Block::Bc4),
            (b"ATI2" | b"BC5U" | b"BC5S", _) | (_, Some(82..=84)) => Some(Block::Bc5),
            (_, Some(94 | 95)) => Some(Block::Bc6Unsigned),
            (_, Some(96)) => Some(Block::Bc6Signed),
            (_, Some(97..=99)) => Some(Block::Bc7),
            (b"ETC1", _) => Some(Block::Etc1),
            _ => None,
        };
        if let Some(k) = modern {
            return blocks::decode(k, data, width, height);
        }
        // compressed
        let (block_bytes, kind) = match (fourcc, dxgi) {
            (b"DXT1", _) | (_, Some(70..=72)) => (8, 1),
            (b"DXT2" | b"DXT3", _) | (_, Some(73..=75)) => (16, 3),
            (b"DXT4" | b"DXT5" | b"RXGB", _) | (_, Some(76..=78)) => (16, 5),
            (_, Some(f)) => return Err(format!("unsupported DXGI format {f}")),
            (other, None) => {
                return Err(format!(
                    "unsupported fourcc {:?}",
                    String::from_utf8_lossy(other)
                ));
            }
        };
        // DXT2 and DXT4 hold colour premultiplied by alpha
        let premultiplied = matches!(fourcc, b"DXT2" | b"DXT4");
        let bw = (width + 3) / 4;
        let bh = (height + 3) / 4;
        if data.len() < bw * bh * block_bytes {
            return Err("truncated DXT data".into());
        }
        for by in 0..bh {
            for bx in 0..bw {
                let block = &data[(by * bw + bx) * block_bytes..][..block_bytes];
                let px = decode_block(block, kind);
                for y in 0..4 {
                    for x in 0..4 {
                        let (ix, iy) = (bx * 4 + x, by * 4 + y);
                        if ix < width && iy < height {
                            let o = (iy * width + ix) * 4;
                            rgba[o..o + 4].copy_from_slice(&px[y * 4 + x]);
                        }
                    }
                }
            }
        }
        // RXGB (Doom 3's normal maps): DXT5 with the red channel in the alpha
        if fourcc == b"RXGB" {
            for p in rgba.chunks_exact_mut(4) {
                p[0] = p[3];
                p[3] = 255;
            }
        }
        if premultiplied {
            for p in rgba.chunks_exact_mut(4) {
                if p[3] > 0 && p[3] < 255 {
                    for c in 0..3 {
                        p[c] = ((p[c] as u32 * 255 + p[3] as u32 / 2) / p[3] as u32).min(255) as u8;
                    }
                }
            }
        }
        let has_alpha =
            fourcc != b"RXGB" && (kind != 1 || rgba.chunks_exact(4).any(|p| p[3] < 255));
        return Ok(Image {
            width: width as u32,
            height: height as u32,
            rgba,
            has_alpha,
        });
    }
    // uncompressed: masks describe the channels (luminance: r mask only, flag 0x20000)
    let luminance = pf_flags & 0x20000 != 0;
    let alpha_only = pf_flags & 0x2 != 0 && pf_flags & 0x40 == 0 && !luminance;
    let bytes_pp = (bpp.max(8) + 7) / 8;
    let row = width * bytes_pp;
    if data.len() < row * height {
        return Err("truncated pixel data".into());
    }
    let extract = |v: u32, mask: u32| -> u8 {
        if mask == 0 {
            return 0;
        }
        let shift = mask.trailing_zeros();
        let bits = (mask >> shift).count_ones();
        let x = (v & mask) >> shift;
        // scale to 8 bits
        if bits >= 8 {
            (x >> (bits - 8)) as u8
        } else {
            ((x * 255) / ((1u32 << bits) - 1)) as u8
        }
    };
    let has_alpha = masks[3] != 0 || alpha_only;
    for y in 0..height {
        for x in 0..width {
            let o = y * row + x * bytes_pp;
            let mut v = 0u32;
            for k in 0..bytes_pp.min(4) {
                v |= (data[o + k] as u32) << (8 * k);
            }
            let p = &mut rgba[(y * width + x) * 4..][..4];
            if alpha_only {
                p.copy_from_slice(&[255, 255, 255, extract(v, masks[3])]);
            } else if luminance {
                let l = extract(v, masks[0]);
                let a = if masks[3] != 0 {
                    extract(v, masks[3])
                } else {
                    255
                };
                p.copy_from_slice(&[l, l, l, a]);
            } else {
                let a = if masks[3] != 0 {
                    extract(v, masks[3])
                } else {
                    255
                };
                p.copy_from_slice(&[
                    extract(v, masks[0]),
                    extract(v, masks[1]),
                    extract(v, masks[2]),
                    a,
                ]);
            }
        }
    }
    Ok(Image {
        width: width as u32,
        height: height as u32,
        rgba,
        has_alpha,
    })
}

/// The uncompressed DXGI formats of a DX10 header: R8G8B8A8 (28, 29 sRGB), B8G8R8A8 (87,
/// 91), B8G8R8X8 (88, 93), R16G16B16A16 float (10) and unorm (11), R32G32B32A32 float (2),
/// R32G32B32 float (6), R10G10B10A2 (24), R11G11B10 float (26), R16G16 float (34) and
/// unorm (35), R32 float (41), R8G8 (49), R16 float (54) and unorm (56), R8 (61), A8 (65),
/// R9G9B9E5 (67), B5G6R5 (85), B5G5R5A1 (86), B4G4R4A4 (115).
fn dxgi_plain(
    format: u32,
    data: &[u8],
    width: usize,
    height: usize,
) -> Option<Result<Image, String>> {
    let wide = match format {
        10 => Some(113),
        11 => Some(36),
        2 => Some(116),
        6 => Some(1006),
        34 => Some(112),
        35 => Some(34),
        41 => Some(114),
        54 => Some(111),
        56 => Some(1056),
        _ => None,
    };
    if let Some(d3d) = wide {
        return wide_rgba(d3d, data, width, height);
    }
    let bytes_pp = match format {
        28 | 29 | 87 | 88 | 91 | 93 | 24 | 26 | 67 => 4,
        49 | 85 | 86 | 115 => 2,
        61 | 65 => 1,
        _ => return None,
    };
    if data.len() < width * height * bytes_pp {
        return Some(Err("truncated pixel data".into()));
    }
    let unit = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let bits =
        |v: u32, shift: u32, n: u32| (((v >> shift) & ((1 << n) - 1)) * 255 / ((1 << n) - 1)) as u8;
    let mut rgba = vec![0u8; width * height * 4];
    for (i, px) in data.chunks_exact(bytes_pp).take(width * height).enumerate() {
        let p = &mut rgba[i * 4..i * 4 + 4];
        let v32 = || u32::from_le_bytes([px[0], px[1], px[2], px[3]]);
        let v16 = || u16::from_le_bytes([px[0], px[1]]) as u32;
        match format {
            28 | 29 => p.copy_from_slice(px),
            87 | 91 => p.copy_from_slice(&[px[2], px[1], px[0], px[3]]),
            88 | 93 => p.copy_from_slice(&[px[2], px[1], px[0], 255]),
            24 => {
                let v = v32();
                p.copy_from_slice(&[
                    bits(v, 0, 10),
                    bits(v, 10, 10),
                    bits(v, 20, 10),
                    bits(v, 30, 2),
                ]);
            }
            26 => {
                let v = v32();
                // 6-bit mantissa, 5-bit exponent (r, g); 5 and 5 (b): small floats
                let f = |m: u32, mb: u32, e: u32| -> f32 {
                    if e == 0 {
                        m as f32 / (1 << mb) as f32 * 2f32.powi(-14)
                    } else {
                        (1.0 + m as f32 / (1 << mb) as f32) * 2f32.powi(e as i32 - 15)
                    }
                };
                let r = f(v & 63, 6, (v >> 6) & 31);
                let g = f((v >> 11) & 63, 6, (v >> 17) & 31);
                let b = f((v >> 22) & 31, 5, (v >> 27) & 31);
                p.copy_from_slice(&[unit(r), unit(g), unit(b), 255]);
            }
            67 => {
                let v = v32();
                let scale = 2f32.powi(((v >> 27) & 31) as i32 - 15 - 9);
                let c = |s: u32| unit(((v >> s) & 511) as f32 * scale);
                p.copy_from_slice(&[c(0), c(9), c(18), 255]);
            }
            49 => p.copy_from_slice(&[px[0], px[1], 0, 255]),
            85 => {
                let v = v16();
                p.copy_from_slice(&[bits(v, 11, 5), bits(v, 5, 6), bits(v, 0, 5), 255]);
            }
            86 => {
                let v = v16();
                p.copy_from_slice(&[bits(v, 10, 5), bits(v, 5, 5), bits(v, 0, 5), bits(v, 15, 1)]);
            }
            115 => {
                let v = v16();
                p.copy_from_slice(&[bits(v, 8, 4), bits(v, 4, 4), bits(v, 0, 4), bits(v, 12, 4)]);
            }
            61 => p.copy_from_slice(&[px[0], px[0], px[0], 255]),
            _ => p.copy_from_slice(&[255, 255, 255, px[0]]),
        }
    }
    let has_alpha = rgba.chunks_exact(4).any(|p| p[3] < 255);
    Some(Ok(Image {
        width: width as u32,
        height: height as u32,
        rgba,
        has_alpha,
    }))
}

/// Direct3D's wide formats, in R, G, B, A order and clamped to 0..1: `D3DFMT_A16B16G16R16`
/// (36), `A16B16G16R16F` (113), `A32B32G32R32F` (116), `G16R16` (34), `R16F` (111),
/// `G16R16F` (112), `R32F` (114), `G32R32F` (115); and two of DXGI's that Direct3D 9 lacks
/// (here 1006, R32G32B32 float, and 1056, R16 unorm). A single channel shows grey.
fn wide_rgba(
    format: u32,
    data: &[u8],
    width: usize,
    height: usize,
) -> Option<Result<Image, String>> {
    let unorm16: fn(&[u8]) -> f32 = |b| u16::from_le_bytes([b[0], b[1]]) as f32 / 65535.0;
    let half: fn(&[u8]) -> f32 = |b| half_to_f32(u16::from_le_bytes([b[0], b[1]]));
    let float: fn(&[u8]) -> f32 = |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let (channels, step, read) = match format {
        36 => (4, 2, unorm16),
        113 => (4, 2, half),
        116 => (4, 4, float),
        34 => (2, 2, unorm16),
        111 => (1, 2, half),
        112 => (2, 2, half),
        114 => (1, 4, float),
        115 => (2, 4, float),
        1006 => (3, 4, float),
        1056 => (1, 2, unorm16),
        _ => return None,
    };
    let bytes_pp = channels * step;
    if data.len() < width * height * bytes_pp {
        return Some(Err("truncated pixel data".into()));
    }
    let mut rgba = vec![0u8; width * height * 4];
    for (i, px) in data.chunks_exact(bytes_pp).take(width * height).enumerate() {
        let c = |k: usize| (read(&px[k * step..]).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        let out = match channels {
            1 => [c(0), c(0), c(0), 255],
            2 => [c(0), c(1), 0, 255],
            3 => [c(0), c(1), c(2), 255],
            _ => [c(0), c(1), c(2), c(3)],
        };
        rgba[i * 4..i * 4 + 4].copy_from_slice(&out);
    }
    let has_alpha = rgba.chunks_exact(4).any(|p| p[3] < 255);
    Some(Ok(Image {
        width: width as u32,
        height: height as u32,
        rgba,
        has_alpha,
    }))
}

fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1F) as i32;
    let frac = (h & 0x3FF) as f32;
    sign * match exp {
        0 => frac * 2f32.powi(-24),
        31 => f32::INFINITY,
        e => (1.0 + frac / 1024.0) * 2f32.powi(e - 15),
    }
}

fn rgb565(v: u16) -> [u8; 4] {
    let r = ((v >> 11) & 31) as u32;
    let g = ((v >> 5) & 63) as u32;
    let b = (v & 31) as u32;
    [
        ((r * 255) / 31) as u8,
        ((g * 255) / 63) as u8,
        ((b * 255) / 31) as u8,
        255,
    ]
}

pub(crate) fn decode_block(block: &[u8], kind: u8) -> [[u8; 4]; 16] {
    let mut out = [[0u8; 4]; 16];
    let color = if kind == 1 { block } else { &block[8..] };
    let c0 = u16::from_le_bytes([color[0], color[1]]);
    let c1 = u16::from_le_bytes([color[2], color[3]]);
    let p0 = rgb565(c0);
    let p1 = rgb565(c1);
    let mix = |a: [u8; 4], b: [u8; 4], wa: u32, wb: u32| -> [u8; 4] {
        let f = |i: usize| ((a[i] as u32 * wa + b[i] as u32 * wb) / (wa + wb)) as u8;
        [f(0), f(1), f(2), 255]
    };
    let palette = if kind == 1 && c0 <= c1 {
        [p0, p1, mix(p0, p1, 1, 1), [0, 0, 0, 0]]
    } else {
        [p0, p1, mix(p0, p1, 2, 1), mix(p0, p1, 1, 2)]
    };
    let bits = u32::from_le_bytes([color[4], color[5], color[6], color[7]]);
    for i in 0..16 {
        out[i] = palette[((bits >> (2 * i)) & 3) as usize];
    }
    match kind {
        3 => {
            let a = u64::from_le_bytes(block[..8].try_into().unwrap());
            for i in 0..16 {
                let v = ((a >> (4 * i)) & 15) as u8;
                out[i][3] = v * 17;
            }
        }
        5 => {
            let a0 = block[0] as u32;
            let a1 = block[1] as u32;
            let mut table = [0u8; 8];
            table[0] = a0 as u8;
            table[1] = a1 as u8;
            if a0 > a1 {
                for i in 1..7 {
                    table[i + 1] = (((7 - i as u32) * a0 + i as u32 * a1) / 7) as u8;
                }
            } else {
                for i in 1..5 {
                    table[i + 1] = (((5 - i as u32) * a0 + i as u32 * a1) / 5) as u8;
                }
                table[6] = 0;
                table[7] = 255;
            }
            let mut bits = 0u64;
            for i in 0..6 {
                bits |= (block[2 + i] as u64) << (8 * i);
            }
            for i in 0..16 {
                out[i][3] = table[((bits >> (3 * i)) & 7) as usize];
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod dx10_tests {
    #[test]
    fn a_dx10_bgra_picture_is_read() {
        let mut b = vec![0u8; 148];
        b[..4].copy_from_slice(b"DDS ");
        b[12..16].copy_from_slice(&1u32.to_le_bytes()); // height
        b[16..20].copy_from_slice(&2u32.to_le_bytes()); // width
        b[80..84].copy_from_slice(&4u32.to_le_bytes()); // fourcc flag
        b[84..88].copy_from_slice(b"DX10");
        b[128..132].copy_from_slice(&87u32.to_le_bytes()); // B8G8R8A8_UNORM
        b.extend_from_slice(&[10, 20, 30, 40, 1, 2, 3, 255]);
        let img = super::decode(&b).unwrap();
        assert_eq!(&img.rgba[..8], &[30, 20, 10, 40, 3, 2, 1, 255]);
        assert!(img.has_alpha);
    }
}

/// A sphere map (as Direct3D's `D3DTSS_TCI_SPHEREMAP` reads it) of `size` pixels from the six
/// faces of a cube map (+X, −X, +Y, −Y, +Z, −Z, as DDS stores them): each pixel is the
/// direction a mirror sphere reflects towards the viewer there.
fn cube_to_sphere(faces: &[Image], size: usize) -> Image {
    let mut rgba = vec![0u8; size * size * 4];
    let mut has_alpha = false;
    for y in 0..size {
        for x in 0..size {
            let nx = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let ny = 1.0 - (y as f32 + 0.5) / size as f32 * 2.0;
            let r2 = nx * nx + ny * ny;
            if r2 > 1.0 {
                continue;
            }
            // the sphere's normal, and the view along −z reflected about it
            let nz = (1.0 - r2).sqrt();
            let d = [2.0 * nz * nx, 2.0 * nz * ny, 2.0 * nz * nz - 1.0];
            let (ax, ay, az) = (d[0].abs(), d[1].abs(), d[2].abs());
            let (face, u, v) = if ax >= ay && ax >= az {
                if d[0] > 0.0 {
                    (0, -d[2] / ax, -d[1] / ax)
                } else {
                    (1, d[2] / ax, -d[1] / ax)
                }
            } else if ay >= az {
                if d[1] > 0.0 {
                    (2, d[0] / ay, d[2] / ay)
                } else {
                    (3, d[0] / ay, -d[2] / ay)
                }
            } else if d[2] > 0.0 {
                (4, d[0] / az, -d[1] / az)
            } else {
                (5, -d[0] / az, -d[1] / az)
            };
            let f = &faces[face];
            let fx = (((u + 1.0) * 0.5 * f.width as f32) as u32).min(f.width.saturating_sub(1));
            let fy = (((v + 1.0) * 0.5 * f.height as f32) as u32).min(f.height.saturating_sub(1));
            let si = ((fy * f.width + fx) * 4) as usize;
            let di = (y * size + x) * 4;
            if let Some(px) = f.rgba.get(si..si + 4) {
                rgba[di..di + 4].copy_from_slice(px);
                has_alpha |= px[3] < 255;
            }
        }
    }
    Image {
        width: size as u32,
        height: size as u32,
        rgba,
        has_alpha,
    }
}

#[cfg(test)]
mod cube_tests {
    #[test]
    fn a_cube_map_becomes_a_sphere_map() {
        // six 4x4 uncompressed faces, each one colour
        let mut b = vec![0u8; 128];
        b[..4].copy_from_slice(b"DDS ");
        b[12..16].copy_from_slice(&4u32.to_le_bytes());
        b[16..20].copy_from_slice(&4u32.to_le_bytes());
        b[80..84].copy_from_slice(&0x41u32.to_le_bytes()); // RGB + alpha
        b[88..92].copy_from_slice(&32u32.to_le_bytes());
        b[92..96].copy_from_slice(&0x00FF_0000u32.to_le_bytes());
        b[96..100].copy_from_slice(&0x0000_FF00u32.to_le_bytes());
        b[100..104].copy_from_slice(&0x0000_00FFu32.to_le_bytes());
        b[104..108].copy_from_slice(&0xFF00_0000u32.to_le_bytes());
        b[112..116].copy_from_slice(&0xFE00u32.to_le_bytes());
        for k in 0..6u8 {
            for _ in 0..16 {
                b.extend_from_slice(&[k * 40, 0, 0, 255]); // B G R A
            }
        }
        let img = super::decode(&b).unwrap();
        assert_eq!(img.width, 64);
        // the middle looks straight back at the viewer: the +Z face (index 4)
        let c = ((32 * 64 + 32) * 4) as usize;
        assert_eq!(img.rgba[c + 2], 4 * 40);
    }
}
