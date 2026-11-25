//! Khronos texture containers, KTX 1 and KTX 2 (what glTF tools, Basis-free engines and phone
//! pipelines write): the top level of a plain RGBA / RGB / R / RG 8-bit picture or of a
//! block-compressed one (BC1-BC7, ETC1/ETC2/EAC, ASTC). A KTX 2 file must not be
//! supercompressed (Basis Universal, zstd).

use crate::Image;
use crate::blocks::{self, Block};

const KTX1: &[u8; 12] = b"\xABKTX 11\xBB\r\n\x1A\n";
const KTX2: &[u8; 12] = b"\xABKTX 20\xBB\r\n\x1A\n";

pub fn is_ktx(b: &[u8]) -> bool {
    b.starts_with(KTX1) || b.starts_with(KTX2)
}

fn u32_at(b: &[u8], o: usize) -> Result<u32, String> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| "truncated header".to_string())
}

fn u64_at(b: &[u8], o: usize) -> Result<u64, String> {
    Ok(u32_at(b, o)? as u64 | (u32_at(b, o + 4)? as u64) << 32)
}

/// What the pixels of a level are.
enum Kind {
    Plain(usize, fn(&[u8]) -> [u8; 4]),
    Block(Block),
}

pub fn decode(b: &[u8]) -> Result<Image, String> {
    let (kind, w, h, data) = if b.starts_with(KTX2) {
        let vk = u32_at(b, 12)?;
        let (w, h) = (u32_at(b, 20)? as usize, (u32_at(b, 24)? as usize).max(1));
        let scheme = u32_at(b, 44)?;
        if scheme != 0 {
            return Err(format!(
                "supercompressed KTX 2 (scheme {scheme}) is not read"
            ));
        }
        let (off, len) = (u64_at(b, 80)? as usize, u64_at(b, 88)? as usize);
        let data = b
            .get(off..off.saturating_add(len))
            .ok_or("level 0 lies outside the file")?;
        (
            vk_kind(vk).ok_or_else(|| format!("KTX 2 format {vk} is not read"))?,
            w,
            h,
            data,
        )
    } else if b.starts_with(KTX1) {
        if u32_at(b, 12)? != 0x0403_0201 {
            return Err("big-endian KTX".into());
        }
        let (gl_type, gl_format, internal) = (u32_at(b, 16)?, u32_at(b, 24)?, u32_at(b, 28)?);
        let (w, h) = (u32_at(b, 36)? as usize, (u32_at(b, 40)? as usize).max(1));
        let kvd = u32_at(b, 60)? as usize;
        let at = 64 + kvd;
        let size = u32_at(b, at)? as usize;
        let data = b
            .get(at + 4..at + 4 + size)
            .ok_or("level 0 lies outside the file")?;
        (
            gl_kind(gl_type, gl_format, internal)
                .ok_or_else(|| format!("KTX format 0x{internal:x} is not read"))?,
            w,
            h,
            data,
        )
    } else {
        return Err("not a KTX file".into());
    };
    if w == 0 || w > crate::MAX_DIMENSION || h > crate::MAX_DIMENSION {
        return Err(format!("{w}x{h} is no texture size"));
    }
    match kind {
        Kind::Block(k) => blocks::decode(k, data, w, h),
        Kind::Plain(bpp, read) => {
            if data.len() < w * h * bpp {
                return Err("truncated pixel data".into());
            }
            // (KTX rows are padded to 4 bytes in KTX 1 only for the odd formats; plain
            // 8-bit rows of these sizes are packed)
            let mut rgba = Vec::with_capacity(w * h * 4);
            for px in data.chunks_exact(bpp).take(w * h) {
                rgba.extend_from_slice(&read(px));
            }
            let has_alpha = rgba.chunks_exact(4).any(|p| p[3] < 255);
            Ok(Image {
                width: w as u32,
                height: h as u32,
                rgba,
                has_alpha,
            })
        }
    }
}

fn rgba(p: &[u8]) -> [u8; 4] {
    [p[0], p[1], p[2], p[3]]
}
fn bgra(p: &[u8]) -> [u8; 4] {
    [p[2], p[1], p[0], p[3]]
}
fn rgb(p: &[u8]) -> [u8; 4] {
    [p[0], p[1], p[2], 255]
}
fn r(p: &[u8]) -> [u8; 4] {
    [p[0], p[0], p[0], 255]
}
fn rg(p: &[u8]) -> [u8; 4] {
    [p[0], p[1], 0, 255]
}

/// The Vulkan formats of a KTX 2 file.
fn vk_kind(vk: u32) -> Option<Kind> {
    const ASTC: [(u8, u8); 14] = [
        (4, 4),
        (5, 4),
        (5, 5),
        (6, 5),
        (6, 6),
        (8, 5),
        (8, 6),
        (8, 8),
        (10, 5),
        (10, 6),
        (10, 8),
        (10, 10),
        (12, 10),
        (12, 12),
    ];
    Some(match vk {
        37 | 43 => Kind::Plain(4, rgba),
        44 | 50 => Kind::Plain(4, bgra),
        23 | 29 => Kind::Plain(3, rgb),
        9 | 15 => Kind::Plain(1, r),
        16 | 22 => Kind::Plain(2, rg),
        131..=134 => Kind::Block(Block::Bc1),
        135 | 136 => Kind::Block(Block::Bc2),
        137 | 138 => Kind::Block(Block::Bc3),
        139 | 140 => Kind::Block(Block::Bc4),
        141 | 142 => Kind::Block(Block::Bc5),
        143 => Kind::Block(Block::Bc6Unsigned),
        144 => Kind::Block(Block::Bc6Signed),
        145 | 146 => Kind::Block(Block::Bc7),
        147 | 148 => Kind::Block(Block::Etc2Rgb),
        149 | 150 => Kind::Block(Block::Etc2Rgba1),
        151 | 152 => Kind::Block(Block::Etc2Rgba8),
        153 | 154 => Kind::Block(Block::EacR),
        155 | 156 => Kind::Block(Block::EacRg),
        157..=184 => {
            let (w, h) = ASTC[((vk - 157) / 2) as usize];
            Kind::Block(Block::Astc(w, h))
        }
        _ => return None,
    })
}

/// The OpenGL formats of a KTX 1 file.
fn gl_kind(gl_type: u32, gl_format: u32, internal: u32) -> Option<Kind> {
    const UNSIGNED_BYTE: u32 = 0x1401;
    Some(match internal {
        0x8058 | 0x8C43 => Kind::Plain(4, rgba),
        0x8051 | 0x8C41 => Kind::Plain(3, rgb),
        0x1908 if gl_type == UNSIGNED_BYTE => Kind::Plain(4, rgba),
        0x1907 if gl_type == UNSIGNED_BYTE => Kind::Plain(3, rgb),
        0x80E1 if gl_type == UNSIGNED_BYTE => Kind::Plain(4, bgra),
        0x8229 | 0x1903 if gl_type == UNSIGNED_BYTE => Kind::Plain(1, r),
        0x822B | 0x8227 if gl_type == UNSIGNED_BYTE => Kind::Plain(2, rg),
        0x83F0 | 0x83F1 | 0x8C4C | 0x8C4D => Kind::Block(Block::Bc1),
        0x83F2 | 0x8C4E => Kind::Block(Block::Bc2),
        0x83F3 | 0x8C4F => Kind::Block(Block::Bc3),
        0x8DBB | 0x8DBC => Kind::Block(Block::Bc4),
        0x8DBD | 0x8DBE => Kind::Block(Block::Bc5),
        0x8E8F => Kind::Block(Block::Bc6Unsigned),
        0x8E8E => Kind::Block(Block::Bc6Signed),
        0x8E8C | 0x8E8D => Kind::Block(Block::Bc7),
        0x8D64 => Kind::Block(Block::Etc1),
        0x9274 | 0x9275 => Kind::Block(Block::Etc2Rgb),
        0x9276 | 0x9277 => Kind::Block(Block::Etc2Rgba1),
        0x9278 | 0x9279 => Kind::Block(Block::Etc2Rgba8),
        0x9270 => Kind::Block(Block::EacR),
        0x9272 => Kind::Block(Block::EacRg),
        0x93B0..=0x93BD | 0x93D0..=0x93DD => {
            const ASTC: [(u8, u8); 14] = [
                (4, 4),
                (5, 4),
                (5, 5),
                (6, 5),
                (6, 6),
                (8, 5),
                (8, 6),
                (8, 8),
                (10, 5),
                (10, 6),
                (10, 8),
                (10, 10),
                (12, 10),
                (12, 12),
            ];
            let (w, h) = ASTC[(internal & 0xF) as usize];
            Kind::Block(Block::Astc(w, h))
        }
        _ => {
            let _ = gl_format;
            return None;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ktx2_rgba_picture() {
        let mut b = vec![0u8; 104];
        b[..12].copy_from_slice(KTX2);
        b[12..16].copy_from_slice(&37u32.to_le_bytes());
        b[20..24].copy_from_slice(&2u32.to_le_bytes());
        b[24..28].copy_from_slice(&1u32.to_le_bytes());
        b[40..44].copy_from_slice(&1u32.to_le_bytes());
        b[80..88].copy_from_slice(&104u64.to_le_bytes());
        b[88..96].copy_from_slice(&8u64.to_le_bytes());
        b.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 255]);
        let img = decode(&b).unwrap();
        assert_eq!((img.width, img.height), (2, 1));
        assert_eq!(img.rgba, [1, 2, 3, 4, 5, 6, 7, 255]);
        assert!(img.has_alpha);
    }

    #[test]
    fn a_ktx1_bc1_picture() {
        let mut b = vec![0u8; 64];
        b[..12].copy_from_slice(KTX1);
        b[12..16].copy_from_slice(&0x0403_0201u32.to_le_bytes());
        b[28..32].copy_from_slice(&0x83F0u32.to_le_bytes());
        b[36..40].copy_from_slice(&4u32.to_le_bytes());
        b[40..44].copy_from_slice(&4u32.to_le_bytes());
        b.extend_from_slice(&8u32.to_le_bytes());
        // a red block: both endpoints 0xF800
        b.extend_from_slice(&[0x00, 0xF8, 0x00, 0xF8, 0, 0, 0, 0]);
        let img = decode(&b).unwrap();
        assert_eq!(&img.rgba[..4], &[255, 0, 0, 255]);
    }
}
