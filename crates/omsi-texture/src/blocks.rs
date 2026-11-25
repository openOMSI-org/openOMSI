//! Block-compressed pictures beyond DXT1-5: BC4 / BC5 (one- and two-channel: height and
//! normal maps), BC6H (HDR), BC7 (what texconv, Substance and the engines of today write),
//! and the phone formats ETC1 / ETC2 / EAC / ASTC of KTX files - decoded to RGBA8 with
//! `texture2ddecoder`.

use crate::Image;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    Bc1,
    Bc2,
    Bc3,
    /// One channel, shown grey.
    Bc4,
    /// Two channels (a normal map's x and y): z is made from them.
    Bc5,
    Bc6Unsigned,
    Bc6Signed,
    Bc7,
    Etc1,
    Etc2Rgb,
    Etc2Rgba1,
    Etc2Rgba8,
    EacR,
    EacRg,
    Astc(u8, u8),
}

impl Block {
    /// Bytes of one block.
    pub fn block_bytes(self) -> usize {
        match self {
            Block::Bc1
            | Block::Bc4
            | Block::Etc1
            | Block::Etc2Rgb
            | Block::Etc2Rgba1
            | Block::EacR => 8,
            _ => 16,
        }
    }

    /// Width and height of one block in pixels.
    pub fn block_size(self) -> (usize, usize) {
        match self {
            Block::Astc(w, h) => (w as usize, h as usize),
            _ => (4, 4),
        }
    }

    /// Bytes of the top level of a `width` x `height` picture.
    pub fn level_bytes(self, width: usize, height: usize) -> usize {
        let (bw, bh) = self.block_size();
        width.div_ceil(bw) * height.div_ceil(bh) * self.block_bytes()
    }

    fn has_alpha(self) -> bool {
        matches!(
            self,
            Block::Bc2
                | Block::Bc3
                | Block::Bc7
                | Block::Etc2Rgba1
                | Block::Etc2Rgba8
                | Block::Astc(..)
        )
    }
}

/// Decode the top level of a block-compressed picture.
pub fn decode(kind: Block, data: &[u8], width: usize, height: usize) -> Result<Image, String> {
    if width == 0 || height == 0 || width > crate::MAX_DIMENSION || height > crate::MAX_DIMENSION {
        return Err(format!("{width}x{height} is no texture size"));
    }
    if data.len() < kind.level_bytes(width, height) {
        return Err(format!("truncated {kind:?} data"));
    }
    let mut px = vec![0u32; width * height];
    use texture2ddecoder as t;
    let r = match kind {
        Block::Bc1 => t::decode_bc1a(data, width, height, &mut px),
        Block::Bc2 => t::decode_bc2(data, width, height, &mut px),
        Block::Bc3 => t::decode_bc3(data, width, height, &mut px),
        Block::Bc4 => t::decode_bc4(data, width, height, &mut px),
        Block::Bc5 => t::decode_bc5(data, width, height, &mut px),
        Block::Bc6Unsigned => t::decode_bc6(data, width, height, &mut px, false),
        Block::Bc6Signed => t::decode_bc6(data, width, height, &mut px, true),
        Block::Bc7 => t::decode_bc7(data, width, height, &mut px),
        Block::Etc1 => t::decode_etc1(data, width, height, &mut px),
        Block::Etc2Rgb => t::decode_etc2_rgb(data, width, height, &mut px),
        Block::Etc2Rgba1 => t::decode_etc2_rgba1(data, width, height, &mut px),
        Block::Etc2Rgba8 => t::decode_etc2_rgba8(data, width, height, &mut px),
        Block::EacR => t::decode_eacr(data, width, height, &mut px),
        Block::EacRg => t::decode_eacrg(data, width, height, &mut px),
        Block::Astc(bw, bh) => {
            t::decode_astc(data, width, height, bw as usize, bh as usize, &mut px)
        }
    };
    r.map_err(|e| e.to_string())?;
    // (the decoder's pixels are B, G, R, A in memory)
    let mut rgba = Vec::with_capacity(width * height * 4);
    for p in &px {
        let [b, g, r, a] = p.to_le_bytes();
        match kind {
            Block::Bc4 | Block::EacR => rgba.extend_from_slice(&[r, r, r, 255]),
            Block::Bc5 | Block::EacRg => {
                // a normal map's z from its x and y
                let (x, y) = (r as f32 / 127.5 - 1.0, g as f32 / 127.5 - 1.0);
                let z = (1.0 - x * x - y * y).max(0.0).sqrt();
                rgba.extend_from_slice(&[r, g, ((z * 0.5 + 0.5) * 255.0).round() as u8, 255]);
            }
            _ => rgba.extend_from_slice(&[r, g, b, a]),
        }
    }
    let has_alpha = kind.has_alpha() && rgba.chunks_exact(4).any(|p| p[3] < 255)
        || kind == Block::Bc1 && rgba.chunks_exact(4).any(|p| p[3] == 0);
    Ok(Image {
        width: width as u32,
        height: height as u32,
        rgba,
        has_alpha,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bc7_block_of_one_colour() {
        // mode 6 (bit 6 set), both endpoints (r, g, b, a) = 0x7F,0x7F,0x7F,0x7F at 7 bits,
        // p bits 1: every pixel 255 grey - a block written by hand: 0x40 0xFF ...
        // (simpler: a block of zeros is mode-less and decodes to transparent black)
        let img = decode(Block::Bc7, &[0u8; 16], 4, 4).unwrap();
        assert_eq!(img.rgba.len(), 64);
        let img = decode(Block::Bc4, &[200, 200, 0, 0, 0, 0, 0, 0], 4, 4).unwrap();
        assert_eq!(&img.rgba[..4], &[200, 200, 200, 255]);
        assert!(!img.has_alpha);
        // BC5: x = y = 128 is the straight-up normal
        let img = decode(
            Block::Bc5,
            &[128, 128, 0, 0, 0, 0, 0, 0, 128, 128, 0, 0, 0, 0, 0, 0],
            4,
            4,
        )
        .unwrap();
        assert_eq!(img.rgba[2], 255);
        assert!(decode(Block::Bc7, &[0u8; 15], 4, 4).is_err());
    }
}
