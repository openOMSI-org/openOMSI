//! Textures as they go to the GPU: DXT data kept as blocks (with the file's mip chain, or
//! levels filtered and compressed again like D3DX does), uncompressed pictures compressed
//! where the device takes block formats and the result is close enough, RGBA otherwise.

use crate::bc::{self, Bc};
use crate::disk_cache::{DiskCache, Key};
use crate::{Image, TextureError};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Pixel layout of a [`TextureData`] level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// 8-bit sRGB RGBA, 4 bytes a texel.
    Rgba8,
    /// DXT1: 8 bytes per 4x4 block (1-bit alpha when a block says so).
    Bc1,
    /// DXT3: 16 bytes per block, explicit 4-bit alpha.
    Bc2,
    /// DXT5: 16 bytes per block, interpolated alpha.
    Bc3,
    Bc7,
}

impl PixelFormat {
    pub fn is_compressed(self) -> bool {
        self != PixelFormat::Rgba8
    }

    /// Bytes of one level of `w` x `h` texels (blocks rounded up).
    pub fn level_bytes(self, w: u32, h: u32) -> usize {
        let (w, h) = (w.max(1) as usize, h.max(1) as usize);
        match self {
            PixelFormat::Rgba8 => w * h * 4,
            PixelFormat::Bc1 => w.div_ceil(4) * h.div_ceil(4) * 8,
            PixelFormat::Bc2 | PixelFormat::Bc3 | PixelFormat::Bc7 => w.div_ceil(4) * h.div_ceil(4) * 16,
        }
    }

    pub fn decode_level(self, data: &[u8], w: u32, h: u32, punch: bool) -> Vec<u8> {
        match self {
            PixelFormat::Rgba8 => data.to_vec(),
            PixelFormat::Bc7 => crate::blocks::decode(crate::blocks::Block::Bc7, data, w.max(1) as usize, h.max(1) as usize)
                .map(|i| i.rgba)
                .unwrap_or_else(|_| vec![255; w.max(1) as usize * h.max(1) as usize * 4]),
            f => bc::decode(data, w, h, f.bc(punch)),
        }
    }

    fn bc(self, punch: bool) -> Bc {
        match self {
            PixelFormat::Bc1 => Bc::Bc1 { punch },
            PixelFormat::Bc2 => Bc::Bc2,
            _ => Bc::Bc3,
        }
    }
}

/// A texture ready for the GPU.
#[derive(Debug, Clone)]
pub struct TextureData {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    /// Level 0 first. An `Rgba8` texture with a single level has its mip chain made on the
    /// GPU; a compressed one carries all of its levels.
    pub levels: Vec<Vec<u8>>,
    /// The source had an alpha channel (see [`Image::has_alpha`]).
    pub has_alpha: bool,
    /// A single RGBA level gets its mip chain made on the GPU (else it is drawn as it is).
    pub gpu_mips: bool,
}

impl TextureData {
    /// Size of all levels as uploaded (a single RGBA level counts its GPU-made chain too).
    pub fn gpu_bytes(&self) -> u64 {
        let mut total = 0u64;
        let full = mip_count(self.width, self.height);
        let n = if self.format == PixelFormat::Rgba8 && self.levels.len() == 1 && self.gpu_mips { full } else { self.levels.len() as u32 };
        for l in 0..n {
            total += self.format.level_bytes((self.width >> l).max(1), (self.height >> l).max(1)) as u64;
        }
        total
    }

    /// Bytes held on the CPU.
    pub fn cpu_bytes(&self) -> usize {
        self.levels.iter().map(|l| l.len()).sum()
    }

    pub fn from_image(img: Image) -> TextureData {
        TextureData { width: img.width, height: img.height, format: PixelFormat::Rgba8, levels: vec![img.rgba], has_alpha: img.has_alpha, gpu_mips: true }
    }

    /// Drop the `n` largest levels (a texture kept at a lower resolution). Only for data
    /// that carries its levels.
    pub fn without_top_levels(mut self, n: usize) -> TextureData {
        let n = n.min(self.levels.len().saturating_sub(1));
        if n > 0 {
            self.levels.drain(..n);
            self.width = (self.width >> n).max(1);
            self.height = (self.height >> n).max(1);
        }
        self
    }
}

/// Number of levels of a full mip chain.
pub fn mip_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

/// How textures are prepared for the GPU, set once the device is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuOptions {
    /// The device takes BC1-3 textures: DXT files go up as they are.
    pub bc: bool,
    /// Uncompressed pictures are compressed as well (where the result is close enough).
    pub compress: bool,
}

static OPTIONS: AtomicU8 = AtomicU8::new(0);

pub fn set_gpu_options(o: GpuOptions) {
    OPTIONS.store(o.bc as u8 | ((o.compress as u8) << 1), Ordering::Relaxed);
}

pub fn gpu_options() -> GpuOptions {
    let v = OPTIONS.load(Ordering::Relaxed);
    GpuOptions { bc: v & 1 != 0, compress: v & 2 != 0 }
}

/// Without BC every picture is RGBA, 4-8x the compressed one; at half size it takes what BC3 does.
static HALVE_PLAIN: AtomicBool = AtomicBool::new(false);

pub fn set_halve_plain(on: bool) {
    HALVE_PLAIN.store(on, Ordering::Relaxed);
}

/// What the scene uploads in `t`'s place; loaders give it whole, as online play hashes and shares it.
pub fn halved_plain(t: &TextureData) -> Option<TextureData> {
    if !HALVE_PLAIN.load(Ordering::Relaxed) || !halves(t) {
        return None;
    }
    let (rgba, width, height) = bc::downsample_alpha_weighted(&t.levels[0], t.width, t.height);
    Some(TextureData { width, height, format: t.format, levels: vec![rgba], has_alpha: t.has_alpha, gpu_mips: t.gpu_mips })
}

fn halves(t: &TextureData) -> bool {
    t.format == PixelFormat::Rgba8 && t.levels.len() == 1 && t.width >= 256 && t.height >= 256
}

/// Smallest picture (texels) worth compressing: below this the saving is a few kilobytes.
const MIN_COMPRESS_TEXELS: u64 = 64 * 64;
/// Colour PSNR (luma-weighted, dB) a compressed picture must reach to be kept compressed.
pub const MIN_PSNR_COLOUR: f64 = 33.0;
/// The same for the alpha channel.
pub const MIN_PSNR_ALPHA: f64 = 30.0;

/// Largest picture (texels) whose colour swatches are checked one by one: an atlas of small
/// parts is at most this size, and a bigger picture (a livery) whose swatch shifts is a
/// detail of a few pixels on the screen, where keeping it uncompressed costs the most.
const MAX_SWATCH_CHECK_TEXELS: u64 = 1024 * 1024;
/// Largest colour shift (luma-weighted RMS, levels of 255) a flat swatch may get from
/// compression (see [`bc::worst_patch_shift`]); more is a different colour (#2292).
pub const MAX_SWATCH_SHIFT: f64 = 40.0;

/// A compressed picture changes the colour of one of its small swatches.
fn swatch_shifts(src: &[u8], blocks: &[u8], w: u32, h: u32, format: PixelFormat) -> bool {
    if w as u64 * h as u64 > MAX_SWATCH_CHECK_TEXELS {
        return false;
    }
    bc::worst_block_patch_shift(src, blocks, w, h, format.bc(false)) > MAX_SWATCH_SHIFT * MAX_SWATCH_SHIFT
}

fn psnr(sum_sq: f64, n: u64) -> f64 {
    if sum_sq <= 0.0 || n == 0 {
        return 99.0;
    }
    10.0 * (65025.0 / (sum_sq / n as f64)).log10()
}

/// What happened to a picture on its way to the GPU (statistics, `OMSI_DEBUG_TEXTURES`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Prepared {
    /// Colour and alpha PSNR of a compressed picture (99 = exact).
    pub psnr: (f64, f64),
    /// It was compressed here (not a DXT file).
    pub encoded: bool,
    /// Compression was tried and given up for quality.
    pub rejected: bool,
}

/// Mip levels 1.. of `rgba` as blocks of `format`.
fn encode_levels(mut rgba: Vec<u8>, mut w: u32, mut h: u32, format: PixelFormat, punch: bool, out: &mut Vec<Vec<u8>>) {
    let full = mip_count(w, h);
    for _ in 1..full {
        let (next, nw, nh) = bc::downsample_alpha_weighted(&rgba, w, h);
        let blocks = if format == PixelFormat::Bc7 { crate::bc7::encode(&next, nw, nh) } else { bc::encode(&next, nw, nh, format.bc(punch)).0 };
        out.push(blocks);
        rgba = next;
        w = nw;
        h = nh;
    }
}

/// An RGBA picture for the GPU under the current options.
pub fn prepare_image(img: Image) -> (TextureData, Prepared) {
    prepare_image_with(img, gpu_options())
}

/// The size a picture is compressed at: its own, or (for a big picture whose sides are
/// not multiples of four, which block formats need) the nearest multiple of four - a
/// stretch of a texel or two (a 2550² repaint is 2552² on the GPU).
pub fn block_size(w: u32, h: u32) -> Option<(u32, u32)> {
    if w % 4 == 0 && h % 4 == 0 {
        return Some((w, h));
    }
    if w.min(h) < MIN_RESIZE_SIDE {
        return None;
    }
    let near = |v: u32| ((v + 2) / 4 * 4).max(4);
    Some((near(w), near(h)))
}

/// Smallest side of a picture that is resized to compress it (smaller ones stay RGBA: a
/// texel is a larger share of them).
const MIN_RESIZE_SIDE: u32 = 128;

fn resized(img: Image, nw: u32, nh: u32) -> Image {
    if (nw, nh) == (img.width, img.height) {
        return img;
    }
    let rgba = bc::resize(&img.rgba, img.width, img.height, nw, nh);
    Image { rgba, width: nw, height: nh, has_alpha: img.has_alpha }
}

pub(crate) fn at_block_size(img: Image, w: u32, h: u32) -> Option<Image> {
    (block_size(img.width, img.height) == Some((w, h))).then(|| resized(img, w, h))
}

fn bc7_texture(img: &Image) -> Option<(TextureData, (f64, f64))> {
    let (w, h) = (img.width, img.height);
    let blocks = crate::bc7::encode(&img.rgba, w, h);
    let dec = PixelFormat::Bc7.decode_level(&blocks, w, h, false);
    if bc::worst_patch_shift(&img.rgba, &dec, w, h) > MAX_SWATCH_SHIFT * MAX_SWATCH_SHIFT {
        return None;
    }
    let (mut ec, mut ea) = (0.0f64, 0.0f64);
    for (s, d) in img.rgba.chunks_exact(4).zip(dec.chunks_exact(4)) {
        ec += (0..3).map(|c| (s[c] as f64 - d[c] as f64).powi(2) * [0.299, 0.587, 0.114][c]).sum::<f64>();
        ea += (s[3] as f64 - d[3] as f64).powi(2);
    }
    let texels = w as u64 * h as u64;
    let quality = (psnr(ec, texels), psnr(ea, texels));
    if quality.0 < MIN_PSNR_COLOUR || quality.1 < MIN_PSNR_ALPHA {
        return None;
    }
    let mut levels = vec![blocks];
    encode_levels(img.rgba.clone(), w, h, PixelFormat::Bc7, false, &mut levels);
    Some((TextureData { width: w, height: h, format: PixelFormat::Bc7, levels, has_alpha: img.has_alpha, gpu_mips: false }, quality))
}

pub fn prepare_image_with(img: Image, o: GpuOptions) -> (TextureData, Prepared) {
    let mut info = Prepared::default();
    let texels = img.width as u64 * img.height as u64;
    let target = block_size(img.width, img.height);
    if !(o.bc && o.compress) || target.is_none() || texels < MIN_COMPRESS_TEXELS {
        return (TextureData::from_image(img), info);
    }
    let (nw, nh) = target.unwrap();
    // (the picture as read goes as soon as its resized copy is there)
    let img = resized(img, nw, nh);
    let texels = nw as u64 * nh as u64;
    let opaque = !img.has_alpha || img.rgba.chunks_exact(4).all(|p| p[3] == 255);
    let format = if opaque { PixelFormat::Bc1 } else { PixelFormat::Bc3 };
    let (mut blocks, mut ec, ea) = bc::encode(&img.rgba, img.width, img.height, format.bc(false));
    info.psnr = (psnr(ec, texels), if opaque { 99.0 } else { psnr(ea, texels) });
    let mut shifts = info.psnr.0 >= MIN_PSNR_COLOUR && info.psnr.1 >= MIN_PSNR_ALPHA && swatch_shifts(&img.rgba, &blocks, img.width, img.height, format);
    let mut repaired = false;
    if shifts {
        let mut fixed = blocks.clone();
        if let Some(d) = bc::repair_swatches(&img.rgba, &mut fixed, img.width, img.height, format.bc(false), MAX_SWATCH_SHIFT * MAX_SWATCH_SHIFT) {
            blocks = fixed;
            ec += d;
            info.psnr.0 = psnr(ec, texels);
            shifts = swatch_shifts(&img.rgba, &blocks, img.width, img.height, format);
            repaired = true;
        }
    }
    if shifts || (repaired && info.psnr.0 < MIN_PSNR_COLOUR) {
        if let Some((t, psnr)) = bc7_texture(&img) {
            info.psnr = psnr;
            info.encoded = true;
            return (t, info);
        }
    }
    if info.psnr.0 < MIN_PSNR_COLOUR || info.psnr.1 < MIN_PSNR_ALPHA || shifts {
        info.rejected = true;
        return (TextureData::from_image(img), info);
    }
    info.encoded = true;
    let mut levels = vec![blocks];
    let (w, h, has_alpha) = (img.width, img.height, img.has_alpha);
    encode_levels(img.rgba, w, h, format, false, &mut levels);
    (TextureData { width: w, height: h, format, levels, has_alpha, gpu_mips: false }, info)
}

/// Least alpha PSNR (dB) a compressed `[matl_bumpmap]` height map must keep: the renderer
/// reads the slope between neighbouring texels, which block noise disturbs far more than
/// the heights themselves (30 dB, an error of eight levels, is a slope of 6 %).
pub const MIN_PSNR_BUMP: f64 = 40.0;

/// A `[matl_bumpmap]` file for the GPU: its height in the alpha channel (see
/// [`Image::bump_height_map`]; the colour is white), as BC3 where the heights stay within
/// [`MIN_PSNR_BUMP`] (`compress`), else RGBA with its mip chain made here - so that either
/// can be made on a worker ([`TextureData::gpu_mips`] is off).
pub fn prepare_bump(img: &Image, compress: bool) -> TextureData {
    prepare_bump_with(img, if compress { gpu_options() } else { GpuOptions { bc: false, compress: false } })
}

/// [`prepare_bump`] under the given options.
pub fn prepare_bump_with(img: &Image, o: GpuOptions) -> TextureData {
    let h = img.bump_height_map();
    if o.bc && o.compress {
        let (data, info) = prepare_image_with(h.clone(), o);
        if data.format.is_compressed() && info.psnr.1 >= MIN_PSNR_BUMP {
            return data;
        }
    }
    let (w, hh) = (h.width.max(1), h.height.max(1));
    let mut levels = Vec::with_capacity(mip_count(w, hh) as usize);
    let (mut cur, mut cw, mut ch) = (h.rgba, w, hh);
    for _ in 1..mip_count(w, hh) {
        let (next, nw, nh) = bc::downsample(&cur, cw, ch);
        levels.push(std::mem::replace(&mut cur, next));
        cw = nw;
        ch = nh;
    }
    levels.push(cur);
    TextureData { width: w, height: hh, format: PixelFormat::Rgba8, levels, has_alpha: true, gpu_mips: false }
}

/// A picture drawn without a mip chain (a tile's light map and ground masks) for the GPU:
/// one level, compressed where [`prepare_image`] would compress it. `alpha_only`: only the
/// alpha channel is ever read, so a mask of nothing but 0 and 255 goes up as 1-bit BC1
/// (exact, and half the size of BC3).
pub fn prepare_single_level(img: Image, alpha_only: bool) -> (TextureData, Prepared) {
    let o = gpu_options();
    let mut info = Prepared::default();
    let texels = img.width as u64 * img.height as u64;
    let plain = |img: Image| TextureData { gpu_mips: false, ..TextureData::from_image(img) };
    if !(o.bc && o.compress) || img.width % 4 != 0 || img.height % 4 != 0 || texels < MIN_COMPRESS_TEXELS {
        return (plain(img), info);
    }
    let binary = alpha_only && img.rgba.chunks_exact(4).all(|p| p[3] == 0 || p[3] == 255);
    let opaque = !img.has_alpha || img.rgba.chunks_exact(4).all(|p| p[3] == 255);
    let (format, bcf) = if binary || opaque { (PixelFormat::Bc1, Bc::Bc1 { punch: binary }) } else { (PixelFormat::Bc3, Bc::Bc3) };
    let (blocks, ec, ea) = bc::encode(&img.rgba, img.width, img.height, bcf);
    info.psnr = (if alpha_only { 99.0 } else { psnr(ec, texels) }, if opaque || binary { 99.0 } else { psnr(ea, texels) });
    if info.psnr.0 < MIN_PSNR_COLOUR || info.psnr.1 < MIN_PSNR_ALPHA {
        info.rejected = true;
        return (plain(img), info);
    }
    info.encoded = true;
    (TextureData { width: img.width, height: img.height, format, levels: vec![blocks], has_alpha: img.has_alpha, gpu_mips: false }, info)
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// A DDS file with DXT blocks as it is: (format, width, height, levels found in the file,
/// offset of the data). None for anything else.
fn dds_blocks(bytes: &[u8]) -> Option<(PixelFormat, u32, u32, u32, usize)> {
    if bytes.len() < 128 || &bytes[..4] != b"DDS " {
        return None;
    }
    let flags = u32_at(bytes, 8);
    let height = u32_at(bytes, 12);
    let width = u32_at(bytes, 16);
    let mips = u32_at(bytes, 28);
    let pf_flags = u32_at(bytes, 80);
    let fourcc = &bytes[84..88];
    // (a cube map is made into a sphere map by the decoder, never passed on as blocks)
    if pf_flags & 0x4 == 0 || u32_at(bytes, 112) & 0x200 != 0 {
        return None;
    }
    let (format, offset) = match fourcc {
        b"DXT1" => (PixelFormat::Bc1, 128),
        b"DXT2" | b"DXT3" => (PixelFormat::Bc2, 128),
        b"DXT4" | b"DXT5" => (PixelFormat::Bc3, 128),
        b"DX10" if bytes.len() >= 148 => match u32_at(bytes, 128) {
            70..=72 => (PixelFormat::Bc1, 148),
            73..=75 => (PixelFormat::Bc2, 148),
            76..=78 => (PixelFormat::Bc3, 148),
            _ => return None,
        },
        _ => return None,
    };
    if width == 0 || height == 0 || width as usize > crate::MAX_DIMENSION || height as usize > crate::MAX_DIMENSION {
        return None;
    }
    // writers disagree on the flag; a count above one is taken as it is
    let mut count = if flags & 0x20000 != 0 || mips > 1 { mips.max(1) } else { 1 };
    count = count.min(mip_count(width, height));
    Some((format, width, height, count, offset))
}

/// Load a texture file for the GPU under the current options.
pub fn load_gpu(path: &Path) -> Result<(TextureData, Prepared), TextureError> {
    let bytes = omsi_cfg::vfs::read(path).map_err(|e| TextureError::Decode(path.to_path_buf(), e.to_string()))?;
    load_gpu_cached(&bytes, path, gpu_options(), DiskCache::global())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Work {
    None,
    Chain,
    Picture,
}

fn work_of(bytes: &[u8], o: GpuOptions) -> Work {
    if !o.bc {
        return Work::None;
    }
    if let Some((_, w, h, count, _)) = dds_blocks(bytes) {
        if w % 4 == 0 && h % 4 == 0 {
            return if count == 1 && mip_count(w, h) > 1 { Work::Chain } else { Work::None };
        }
    }
    if o.compress { Work::Picture } else { Work::None }
}

pub fn load_gpu_cached(bytes: &[u8], path: &Path, o: GpuOptions, cache: Option<&DiskCache>) -> Result<(TextureData, Prepared), TextureError> {
    let work = work_of(bytes, o);
    let Some((cache, key)) = cache.filter(|_| work != Work::None).and_then(|c| Some((c, Key::new(bytes, path, o)?))) else {
        return load_gpu_bytes(bytes, path, o);
    };
    if let Some(hit) = cache.load(&key).and_then(|e| e.restore(bytes, path)) {
        return Ok(hit);
    }
    let (t, info) = load_gpu_bytes(bytes, path, o)?;
    if info.encoded || info.rejected || (work == Work::Chain && t.format.is_compressed()) {
        cache.store(&key, &t, info);
    }
    Ok((t, info))
}

/// A texture for the GPU as quickly as it can be had, for a picture wanted while the game
/// runs: a DXT file that needs no work as blocks, anything else as RGBA whose mip chain the
/// GPU makes (what the renderer did before). The flag says [`load_gpu`] would give a
/// compressed texture, worth doing on a worker and swapping in.
pub fn load_gpu_fast(path: &Path) -> Result<(TextureData, bool), TextureError> {
    let bytes = omsi_cfg::vfs::read(path).map_err(|e| TextureError::Decode(path.to_path_buf(), e.to_string()))?;
    let o = gpu_options();
    if o.bc {
        if let Some((_, w, h, count, _)) = dds_blocks(&bytes) {
            // a chain the file has is taken as it is; a missing one is made in RGBA first
            if w % 4 == 0 && h % 4 == 0 && (count > 1 || mip_count(w, h) == 1) {
                return load_gpu_bytes(&bytes, path, o).map(|(t, _)| (t, false));
            }
        }
    }
    let img = crate::decode_bytes(&bytes, path)?;
    let texels = img.width as u64 * img.height as u64;
    let dxt = bytes.starts_with(b"DDS ") && dds_blocks(&bytes).is_some();
    let worth = o.bc && (dxt || o.compress) && block_size(img.width, img.height).is_some() && texels >= MIN_COMPRESS_TEXELS;
    Ok((TextureData::from_image(img), worth))
}

/// An RGBA texture at half its size (a quarter of the memory), for the while before its
/// compressed whole is swapped in; anything else as it is.
pub fn halved_for_now(t: TextureData) -> TextureData {
    if !halves(&t) {
        return t;
    }
    let (rgba, w, h) = bc::downsample_alpha_weighted(&t.levels[0], t.width, t.height);
    TextureData { width: w, height: h, levels: vec![rgba], ..t }
}

pub fn load_gpu_bytes(bytes: &[u8], path: &Path, o: GpuOptions) -> Result<(TextureData, Prepared), TextureError> {
    if o.bc {
        if let Some((format, w, h, count, offset)) = dds_blocks(bytes) {
            if w % 4 == 0 && h % 4 == 0 {
                let data = &bytes[offset..];
                let mut levels: Vec<Vec<u8>> = Vec::new();
                let mut at = 0usize;
                for l in 0..count {
                    let n = format.level_bytes((w >> l).max(1), (h >> l).max(1));
                    if at + n > data.len() {
                        break;
                    }
                    levels.push(data[at..at + n].to_vec());
                    at += n;
                }
                if !levels.is_empty() {
                    let has_alpha = format != PixelFormat::Bc1 || levels[0].chunks_exact(8).any(bc::bc1_block_has_alpha);
                    // a single level gets its chain like D3DX makes it: filtered in RGBA and
                    // compressed again (the top level stays the file's own)
                    if levels.len() == 1 && mip_count(w, h) > 1 {
                        let rgba = bc::decode(&levels[0], w, h, format.bc(has_alpha));
                        encode_levels(rgba, w, h, format, has_alpha, &mut levels);
                    }
                    return Ok((TextureData { width: w, height: h, format, levels, has_alpha, gpu_mips: false }, Prepared::default()));
                }
            }
        }
    }
    let img = crate::decode_bytes(bytes, path)?;
    Ok(prepare_image_with(img, o))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dds(fourcc: &[u8; 4], w: u32, h: u32, mips: u32, blocks: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[..4].copy_from_slice(b"DDS ");
        b[4..8].copy_from_slice(&124u32.to_le_bytes());
        b[8..12].copy_from_slice(&(0x1007u32 | if mips > 1 { 0x20000 } else { 0 }).to_le_bytes());
        b[12..16].copy_from_slice(&h.to_le_bytes());
        b[16..20].copy_from_slice(&w.to_le_bytes());
        b[28..32].copy_from_slice(&mips.to_le_bytes());
        b[76..80].copy_from_slice(&32u32.to_le_bytes());
        b[80..84].copy_from_slice(&4u32.to_le_bytes());
        b[84..88].copy_from_slice(fourcc);
        b.extend_from_slice(blocks);
        b
    }

    #[test]
    fn without_bc_a_picture_goes_up_halved_and_is_loaded_whole() {
        let pic = |s: u32| TextureData::from_image(Image { width: s, height: s, rgba: vec![200; (s * s * 4) as usize], has_alpha: false });
        let opaque: Vec<u8> = (0..64 * 64).flat_map(|_| [0xFF, 0xF0, 0x10, 0x00, 0x1B, 0x6C, 0xC6, 0x00]).collect();
        let file = dds(b"DXT1", 256, 256, 1, &opaque);
        assert!(halved_plain(&pic(256)).is_none());
        set_halve_plain(true);
        let half = halved_plain(&pic(256));
        let small = halved_plain(&pic(128));
        let loaded = load_gpu_bytes(&file, Path::new("x.dds"), GpuOptions { bc: false, compress: true });
        set_halve_plain(false);
        let half = half.unwrap();
        assert_eq!((half.width, half.height, half.levels[0].len()), (128, 128, 128 * 128 * 4));
        assert!(small.is_none());
        let (loaded, _) = loaded.unwrap();
        assert_eq!((loaded.format, loaded.width, loaded.height), (PixelFormat::Rgba8, 256, 256));
    }

    #[test]
    fn dxt_files_keep_their_blocks_and_chain() {
        let on = GpuOptions { bc: true, compress: true };
        // 8x8 DXT5 with its full chain (8x8, 4x4, 2x2, 1x1 = 4 + 1 + 1 + 1 blocks)
        let blocks: Vec<u8> = (0..7 * 16).map(|i| i as u8).collect();
        let f = dds(b"DXT5", 8, 8, 4, &blocks);
        let (t, _) = load_gpu_bytes(&f, Path::new("x.dds"), on).unwrap();
        assert_eq!(t.format, PixelFormat::Bc3);
        assert_eq!(t.levels.len(), 4);
        assert_eq!(t.levels[0], blocks[..64]);
        assert_eq!(t.levels[3], blocks[96..112]);
        assert!(t.has_alpha);
        // a DXT1 without a chain gets one made; its top level stays the file's
        let opaque: Vec<u8> = (0..16).flat_map(|i| [0xFF, 0xF0 - i as u8, 0x10, 0x00, 0x1B, 0x6C, 0xC6, 0x00]).collect();
        let f = dds(b"DXT1", 16, 16, 1, &opaque);
        let (t, _) = load_gpu_bytes(&f, Path::new("x.dds"), on).unwrap();
        assert_eq!((t.format, t.levels.len()), (PixelFormat::Bc1, 5));
        assert_eq!(t.levels[0], opaque);
        assert!(!t.has_alpha);
        assert_eq!(t.levels.iter().map(|l| l.len()).collect::<Vec<_>>(), vec![128, 32, 8, 8, 8]);
        // without block support the same file is decoded
        let (t, _) = load_gpu_bytes(&f, Path::new("x.dds"), GpuOptions { bc: false, compress: false }).unwrap();
        assert_eq!((t.format, t.levels.len(), t.levels[0].len()), (PixelFormat::Rgba8, 1, 16 * 16 * 4));
        // a truncated chain keeps the levels that are there
        let f = dds(b"DXT5", 8, 8, 4, &blocks[..80]);
        let (t, _) = load_gpu_bytes(&f, Path::new("x.dds"), on).unwrap();
        assert_eq!(t.levels.len(), 2);
    }

    #[test]
    fn bump_maps_keep_their_heights() {
        let on = GpuOptions { bc: true, compress: true };
        let off = GpuOptions { bc: true, compress: false };
        // a smooth slope compresses within the bound
        let (w, h) = (128u32, 128u32);
        let grey: Vec<u8> = (0..w * h).flat_map(|i| {
            let v = (i % w) as u8;
            [v, v, v, 255]
        }).collect();
        let img = Image { width: w, height: h, rgba: grey, has_alpha: false };
        let t = prepare_bump_with(&img, on);
        assert_eq!((t.format, t.width, t.height, t.gpu_mips), (PixelFormat::Bc3, w, h, false));
        let back = bc::decode(&t.levels[0], w, h, Bc::Bc3);
        assert!(back.chunks_exact(4).zip(img.rgba.chunks_exact(4)).all(|(b, s)| (b[3] as i32 - s[0] as i32).abs() <= 4 && b[0] == 255));
        // uncompressed: white with the height in alpha and a full chain of its own
        let t = prepare_bump_with(&img, off);
        assert_eq!((t.format, t.levels.len(), t.gpu_mips), (PixelFormat::Rgba8, 8, false));
        assert_eq!(&t.levels[0][..8], &[255, 255, 255, 0, 255, 255, 255, 1]);
        assert_eq!(t.levels[7].len(), 4);
        // a 3x1 map keeps its odd size
        let t = prepare_bump_with(&Image { width: 3, height: 1, rgba: vec![9; 12], has_alpha: false }, on);
        assert_eq!((t.width, t.height, t.levels.len()), (3, 1, 2));
    }

    #[test]
    fn transparent_dxt1_blocks_mean_alpha() {
        // c0 <= c1 and index 3 in use
        let blk = [0x00u8, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let blocks: Vec<u8> = blk.iter().cycle().take(8 * 4).copied().collect();
        let f = dds(b"DXT1", 8, 8, 1, &blocks);
        let (t, _) = load_gpu_bytes(&f, Path::new("x.dds"), GpuOptions { bc: true, compress: false }).unwrap();
        assert!(t.has_alpha);
        // its made levels keep the holes
        let back = bc::decode(&t.levels[1], 4, 4, Bc::Bc1 { punch: true });
        assert!(back.chunks_exact(4).all(|p| p[3] == 0));
    }

    #[test]
    fn pictures_are_compressed_when_close_enough() {
        let (w, h) = (128u32, 128u32);
        let smooth: Vec<u8> = (0..w * h).flat_map(|i| [(i % w) as u8, (i / w) as u8, 100, 255]).collect();
        let img = Image { width: w, height: h, rgba: smooth, has_alpha: false };
        let (t, info) = prepare_image_with(img.clone(), GpuOptions { bc: true, compress: true });
        assert_eq!(t.format, PixelFormat::Bc1);
        assert_eq!(t.levels.len(), 8);
        assert!(info.encoded && info.psnr.0 > 40.0, "{info:?}");
        // off: stays RGBA
        let (t, _) = prepare_image_with(img, GpuOptions { bc: true, compress: false });
        assert_eq!(t.format, PixelFormat::Rgba8);
        // alpha → BC3
        let soft: Vec<u8> = (0..w * h).flat_map(|i| [50, 60, 70, (i % w) as u8 * 2]).collect();
        let (t, _) = prepare_image_with(Image { width: w, height: h, rgba: soft, has_alpha: true }, GpuOptions { bc: true, compress: true });
        assert_eq!(t.format, PixelFormat::Bc3);
        // small odd sizes and tiny pictures stay RGBA, big odd ones are resized a little
        let img = Image { width: 130, height: 126, rgba: vec![0; 130 * 126 * 4], has_alpha: false };
        assert_eq!(prepare_image_with(img, GpuOptions { bc: true, compress: true }).0.format, PixelFormat::Rgba8);
        let img = Image { width: 130, height: 131, rgba: vec![7; 130 * 131 * 4], has_alpha: false };
        let (t, _) = prepare_image_with(img, GpuOptions { bc: true, compress: true });
        assert_eq!((t.format, t.width, t.height), (PixelFormat::Bc1, 132, 132));
        assert_eq!(block_size(2550, 2550), Some((2552, 2552)));
        assert_eq!(block_size(2000, 447), Some((2000, 448)));
        assert_eq!(block_size(1273, 1024), Some((1272, 1024)));
        assert_eq!(block_size(90, 256), None);
        // noise does not survive the quality check
        let mut s = 7u32;
        let noise: Vec<u8> = (0..w * h * 4)
            .map(|i| {
                s = s.wrapping_mul(1103515245).wrapping_add(12345);
                if i % 4 == 3 { 255 } else { (s >> 16) as u8 }
            })
            .collect();
        let (t, info) = prepare_image_with(Image { width: w, height: h, rgba: noise, has_alpha: false }, GpuOptions { bc: true, compress: true });
        assert_eq!(t.format, PixelFormat::Rgba8);
        assert!(info.rejected);
    }

    #[test]
    fn an_atlas_whose_swatch_changes_colour_in_bc1_goes_bc7() {
        // a smooth picture with one block of four one-texel swatches (red, green, blue,
        // yellow): the picture measures well, but no block can hold those four colours
        let (w, h) = (128u32, 128u32);
        let mut rgba: Vec<u8> = (0..w * h).flat_map(|i| [(i % w) as u8, (i / w) as u8, 100, 255]).collect();
        let swatches = [[230u8, 14, 23], [40, 200, 60], [20, 30, 220], [240, 220, 30]];
        for y in 0..4 {
            for (x, c) in swatches.iter().enumerate() {
                rgba[((y * w as usize) + 8 + x) * 4..][..3].copy_from_slice(c);
            }
        }
        let on = GpuOptions { bc: true, compress: true };
        let (t, info) = prepare_image_with(Image { width: w, height: h, rgba: rgba.clone(), has_alpha: false }, on);
        assert!(info.psnr.0 > MIN_PSNR_COLOUR, "{info:?}");
        assert_eq!(t.format, PixelFormat::Bc7);
        assert!(!info.rejected);
        assert_eq!(t.levels.len() as u32, mip_count(w, h));
        let back = t.format.decode_level(&t.levels[0], w, h, false);
        assert!(bc::worst_patch_shift(&rgba, &back, w, h) <= MAX_SWATCH_SHIFT * MAX_SWATCH_SHIFT);
        assert!(back.chunks_exact(4).all(|p| p[3] == 255));
        for (l, level) in t.levels.iter().enumerate() {
            assert_eq!(level.len(), PixelFormat::Bc7.level_bytes((w >> l).max(1), (h >> l).max(1)));
        }
        // two swatches on a line are exact: compressed as before
        for y in 0..4 {
            for x in 0..4 {
                let c = if x < 2 { swatches[0] } else { swatches[1] };
                rgba[((y * w as usize) + 8 + x) * 4..][..3].copy_from_slice(&c);
            }
        }
        let (t, _) = prepare_image_with(Image { width: w, height: h, rgba, has_alpha: false }, on);
        assert_eq!(t.format, PixelFormat::Bc1);
    }

    #[test]
    fn tile_masks_stay_single_level() {
        let _g = crate::gpu::tests::OPTIONS_LOCK.lock();
        set_gpu_options(GpuOptions { bc: true, compress: true });
        let (w, h) = (128u32, 128u32);
        // a cut mask: white, alpha 0 or 255 → 1-bit BC1, exact
        let cut: Vec<u8> = (0..w * h).flat_map(|i| [255, 255, 255, if (i % w) < 40 { 0 } else { 255 }]).collect();
        let (t, info) = prepare_single_level(Image { width: w, height: h, rgba: cut.clone(), has_alpha: true }, true);
        assert_eq!((t.format, t.levels.len(), t.gpu_mips), (PixelFormat::Bc1, 1, false));
        assert!(info.encoded);
        let back = bc::decode(&t.levels[0], w, h, Bc::Bc1 { punch: true });
        assert!(cut.chunks_exact(4).zip(back.chunks_exact(4)).all(|(a, b)| a[3] == b[3]));
        // a soft paint mask → BC3
        let soft: Vec<u8> = (0..w * h).flat_map(|i| [255, 255, 255, (i % w) as u8]).collect();
        let (t, _) = prepare_single_level(Image { width: w, height: h, rgba: soft, has_alpha: true }, true);
        assert_eq!((t.format, t.levels.len()), (PixelFormat::Bc3, 1));
        // off: RGBA, still one level
        set_gpu_options(GpuOptions { bc: false, compress: false });
        let (t, _) = prepare_single_level(Image { width: w, height: h, rgba: cut, has_alpha: true }, true);
        assert_eq!((t.format, t.gpu_mips, t.gpu_bytes()), (PixelFormat::Rgba8, false, (w * h * 4) as u64));
    }

    pub(crate) static OPTIONS_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    #[test]
    fn sizes() {
        let t = TextureData { width: 256, height: 128, format: PixelFormat::Bc1, levels: vec![vec![0; 64 * 32 * 8]], has_alpha: false, gpu_mips: false };
        assert_eq!(t.gpu_bytes(), (64 * 32 * 8) as u64);
        let t = TextureData { width: 4, height: 4, format: PixelFormat::Rgba8, levels: vec![vec![0; 64]], has_alpha: false, gpu_mips: true };
        assert_eq!(t.gpu_bytes(), 64 + 16 + 4);
        assert_eq!(TextureData { gpu_mips: false, ..t }.gpu_bytes(), 64);
        assert_eq!(mip_count(256, 128), 9);
        assert_eq!(PixelFormat::Bc3.level_bytes(1, 1), 16);
    }
}
