//! Textures on the GPU: uploads (prepared off the main thread, or with their mips made on
//! the GPU), render targets, replacing, shrinking and rebinding them.

use crate::*;
use crate::pipelines::mip::MipAverage;

/// An RGBA picture borrowed for an upload.
pub(crate) struct RgbaRef<'a> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: &'a [u8],
}

pub struct GpuTexture {
    #[allow(dead_code)]
    pub(crate) texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    pub(crate) size: (u32, u32),
    /// Bytes the texture takes on the GPU, all levels (0 for a freed slot's placeholder).
    pub(crate) bytes: u64,
    /// Which texture this is (a slot is reused and a texture replaced): part of the
    /// key of a material bind group made this frame (see `Scene::bind_groups`).
    pub(crate) generation: u64,
}

pub(crate) static TEXTURE_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub(crate) fn next_gen() -> u64 {
    TEXTURE_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl GpuTexture {
    /// A slot showing `view` (a picture the renderer draws: the one behind the rain on the
    /// glass), with `texture` a small stand-in the slot's bookkeeping asks about.
    pub(crate) fn showing(texture: wgpu::Texture, view: wgpu::TextureView, size: (u32, u32)) -> GpuTexture {
        GpuTexture { texture, view, size, bytes: 0, generation: next_gen() }
    }

    pub(crate) fn new(texture: wgpu::Texture, size: (u32, u32), bytes: u64) -> GpuTexture {
        let view = texture.create_view(&Default::default());
        GpuTexture {
            texture,
            view,
            size,
            bytes,
            generation: next_gen(),
        }
    }
}

/// Set where a texture could not be made for want of graphics memory (see
/// [`try_create_texture`]); read with the device's own out-of-memory flag by
/// `Renderer::take_out_of_memory`.
pub(crate) static TEXTURE_OUT_OF_MEMORY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

static MALFORMED_MIP_UPLOADS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static FAILED_TEXTURE_CREATIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Cumulative texture upload failures in this process, for the game log.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextureUploadFaults {
    pub malformed_mips: u64,
    pub failed_creations: u64,
}

pub fn texture_upload_faults() -> TextureUploadFaults {
    use std::sync::atomic::Ordering::Relaxed;
    TextureUploadFaults {
        malformed_mips: MALFORMED_MIP_UPLOADS.load(Relaxed),
        failed_creations: FAILED_TEXTURE_CREATIONS.load(Relaxed),
    }
}

/// Count a rejected upload, including the level and the byte count needed to reproduce it.
fn report_short_mip(w: u32, h: u32, format: omsi_texture::PixelFormat, level: usize, have: usize, need: usize) {
    let n = MALFORMED_MIP_UPLOADS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    if n <= 5 || n % 100 == 0 {
        log::warn!(
            "texture upload rejected (#{n}): {w}x{h} {format:?}, mip {level} has {have} bytes, needs {need}; drawn plain instead"
        );
    }
}

/// Validate every provided level before touching the GPU. A partially uploaded mip chain
/// must never be used in a material: its missing levels can display undefined colours.
fn first_short_mip(data: &omsi_texture::TextureData) -> Option<(usize, usize, usize)> {
    let (mut w, mut h) = (data.width.max(1), data.height.max(1));
    for (level, bytes) in data.levels.iter().enumerate() {
        let need = data.format.level_bytes(w, h);
        if bytes.len() < need {
            return Some((level, bytes.len(), need));
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    None
}

/// `device.create_texture` where its failure can be seen: None when the card is out of
/// memory (or refuses the texture). Made unchecked, such a texture was invalid, and so was
/// every view, bind group and render bundle made with it afterwards - the whole scene's
/// main pass among them: a black picture for the rest of the session while the sounds and
/// the menus went on (#2472). A texture that cannot be made is drawn as a plain stand-in
/// instead, and when memory ran short the game keeps fewer textures from then on.
pub(crate) fn try_create_texture(device: &wgpu::Device, desc: &wgpu::TextureDescriptor) -> Option<wgpu::Texture> {
    if desc.size.width == 0 || desc.size.height == 0 {
        log::warn!("texture {}x{}: drawn plain instead", desc.size.width, desc.size.height);
        return None;
    }
    let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let texture = device.create_texture(desc);
    let invalid = pollster::block_on(validation.pop());
    let out_of_memory = pollster::block_on(memory.pop());
    if out_of_memory.is_some() {
        TEXTURE_OUT_OF_MEMORY.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let Some(e) = out_of_memory.or(invalid) else {
        return Some(texture);
    };
    let n = FAILED_TEXTURE_CREATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if n < 5 || n % 1000 == 0 {
        log::warn!(
            "texture {}x{} ({:?}, {} levels) could not be made, drawn plain instead (#{}): {e}",
            desc.size.width,
            desc.size.height,
            desc.format,
            desc.mip_level_count,
            n + 1
        );
    }
    None
}

/// A texture that could not be made, drawn as one plain grey texel (its slot counts no
/// bytes; the next upload of that picture tries again).
pub(crate) fn stand_in_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> GpuTexture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("stand-in"),
        size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        &[128, 128, 128, 255],
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
        wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    GpuTexture::new(texture, (1, 1), 0)
}

/// Bytes of a texture of `format` with `levels` mip levels.
pub(crate) fn texture_bytes(format: wgpu::TextureFormat, w: u32, h: u32, levels: u32) -> u64 {
    let (bw, bh) = format.block_dimensions();
    let block = format.block_copy_size(None).unwrap_or(4) as u64;
    (0..levels)
        .map(|l| ((w >> l).max(1).div_ceil(bw) * (h >> l).max(1).div_ceil(bh)) as u64 * block)
        .sum()
}

/// A texture on the GPU, made on a worker thread; [`Renderer::add_prepared_texture`] puts it
/// into a scene.
pub struct PreparedTexture(GpuTexture);

impl PreparedTexture {
    pub fn bytes(&self) -> u64 {
        self.0.bytes
    }
}

/// Make a texture that carries its levels (blocks, or RGBA that wants no GPU-made chain) on
/// any thread; None for anything else (an RGBA picture whose chain the GPU makes, a block
/// format the device does not take).
pub fn prepare_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    data: &omsi_texture::TextureData,
) -> Option<PreparedTexture> {
    if let Some(small) = fit_texture(data, device.limits().max_texture_dimension_2d) {
        return prepare_texture(device, queue, &small);
    }
    let _turn = gl_worker_turn();
    use omsi_texture::PixelFormat;
    let format = match data.format {
        PixelFormat::Rgba8 => wgpu::TextureFormat::Rgba8UnormSrgb,
        PixelFormat::Bc1 => wgpu::TextureFormat::Bc1RgbaUnormSrgb,
        PixelFormat::Bc2 => wgpu::TextureFormat::Bc2RgbaUnormSrgb,
        PixelFormat::Bc3 => wgpu::TextureFormat::Bc3RgbaUnormSrgb,
        PixelFormat::Bc7 => wgpu::TextureFormat::Bc7RgbaUnormSrgb,
    };
    let (w, h) = (data.width.max(1), data.height.max(1));
    if data.levels.is_empty()
        || (data.format == PixelFormat::Rgba8
            && data.levels.len() == 1
            && data.gpu_mips
            && w > 1
            && h > 1)
    {
        return None;
    }
    if data.format.is_compressed()
        && (!device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
            || w % 4 != 0
            || h % 4 != 0)
    {
        return None;
    }
    if let Some((level, have, need)) = first_short_mip(data) {
        report_short_mip(w, h, data.format, level, have, need);
        return Some(PreparedTexture(stand_in_texture(device, queue)));
    }
    let levels = data.levels.len() as u32;
    let size = wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    };
    let Some(texture) = try_create_texture(device, &wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    }) else {
        return Some(PreparedTexture(stand_in_texture(device, queue)));
    };
    let (bw, bh) = format.block_dimensions();
    let block = format.block_copy_size(None).unwrap_or(4);
    for (l, bytes) in data.levels.iter().enumerate() {
        let (lw, lh) = ((w >> l).max(1), (h >> l).max(1));
        // compressed levels are written whole blocks at a time, also below 4x4
        let phys = wgpu::Extent3d {
            width: lw.div_ceil(bw) * bw,
            height: lh.div_ceil(bh) * bh,
            depth_or_array_layers: 1,
        };
        let need = ((phys.width / bw) * (phys.height / bh) * block) as usize;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: l as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes[..need],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some((phys.width / bw) * block),
                rows_per_image: Some(phys.height / bh),
            },
            phys,
        );
    }
    Some(PreparedTexture(GpuTexture::new(
        texture,
        (w, h),
        texture_bytes(format, w, h, levels),
    )))
}

pub(crate) fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    img: &omsi_texture::Image,
    mipmaps: bool,
) -> GpuTexture {
    upload_texture_format(device, queue, img, mipmaps, wgpu::TextureFormat::Rgba8UnormSrgb)
}

pub(crate) fn upload_texture_format(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    img: &omsi_texture::Image,
    mipmaps: bool,
    format: wgpu::TextureFormat,
) -> GpuTexture {
    let mip_count = if mipmaps {
        (32 - img.width.max(img.height).leading_zeros()).max(1)
    } else {
        1
    };
    let size = wgpu::Extent3d {
        width: img.width,
        height: img.height,
        depth_or_array_layers: 1,
    };
    let Some(texture) = try_create_texture(device, &wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    }) else {
        return stand_in_texture(device, queue);
    };
    // CPU box-filter mip chain
    let mut level: Vec<u8> = img.rgba.clone();
    let (mut w, mut h) = (img.width, img.height);
    for mip in 0..mip_count {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: mip,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &level,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        if mip + 1 == mip_count {
            break;
        }
        if format.is_srgb() {
            // Match GPU colour mips, including coverage weighting at clear edges.
            let (next, nw, nh) = omsi_texture::bc::downsample_alpha_weighted(&level, w, h);
            (level, w, h) = (next, nw, nh);
            continue;
        }
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut sum = 0u32;
                    let mut n = 0;
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let sx = (x * 2 + dx).min(w - 1);
                            let sy = (y * 2 + dy).min(h - 1);
                            sum += level[((sy * w + sx) * 4 + c) as usize] as u32;
                            n += 1;
                        }
                    }
                    next[((y * nw + x) * 4 + c) as usize] = (sum / n) as u8;
                }
            }
        }
        level = next;
        w = nw;
        h = nh;
    }
    let view = texture.create_view(&Default::default());
    GpuTexture {
        texture,
        view,
        size: (img.width, img.height),
        bytes: texture_bytes(
            format,
            img.width,
            img.height,
            mip_count,
        ),
        generation: next_gen(),
    }
}

/// A texture bigger than the graphics chip takes (`max` texels a side - 16384 on most, 2048
/// or 4096 on older ones) made to fit: its smaller levels when it has them, else the picture
/// halved until it fits. None when it fits as it is. A too big texture used to be a device
/// error, and the material that used it one too.
pub fn fit_texture(data: &omsi_texture::TextureData, max: u32) -> Option<omsi_texture::TextureData> {
    use omsi_texture::PixelFormat;
    let (w, h) = (data.width.max(1), data.height.max(1));
    if w <= max && h <= max {
        return None;
    }
    let mut k = 0u32;
    while (w >> k).max(1) > max || (h >> k).max(1) > max {
        k += 1;
    }
    if (k as usize) < data.levels.len() {
        return Some(omsi_texture::TextureData { width: (w >> k).max(1), height: (h >> k).max(1), levels: data.levels[k as usize..].to_vec(), ..data.clone() });
    }
    // one level only: decode it and halve it
    let mut rgba = match data.levels.first() {
        Some(l) => data.format.decode_level(l, w, h, true),
        None => return None,
    };
    let (mut cw, mut ch) = (w, h);
    for _ in 0..k {
        let (next, nw, nh) = omsi_texture::bc::downsample_alpha_weighted(&rgba, cw, ch);
        rgba = next;
        (cw, ch) = (nw, nh);
    }
    Some(omsi_texture::TextureData { width: cw, height: ch, format: PixelFormat::Rgba8, levels: vec![rgba], has_alpha: data.has_alpha, gpu_mips: true })
}

/// A size halved (both sides, as `fit_texture` halves a picture) until neither passes `max`.
pub(crate) fn fit_size(w: u32, h: u32, max: u32) -> (u32, u32) {
    let (mut w, mut h) = (w, h);
    while w > max || h > max {
        (w, h) = ((w / 2).max(1), (h / 2).max(1));
    }
    (w, h)
}

/// An RGBA picture of the game's own (a script's or a sign's text, an HTML display) larger
/// than the graphics chip takes, halved until it fits (None when it fits as it is). Made at
/// its full size, the texture was a device error, and so was every frame's write into it.
pub(crate) fn fit_image(img: &omsi_texture::Image, max: u32) -> Option<omsi_texture::Image> {
    if img.width <= max && img.height <= max {
        return None;
    }
    let data = omsi_texture::TextureData {
        width: img.width,
        height: img.height,
        format: omsi_texture::PixelFormat::Rgba8,
        levels: vec![img.rgba.clone()],
        has_alpha: img.has_alpha,
        gpu_mips: true,
    };
    let small = fit_texture(&data, max)?;
    Some(omsi_texture::Image { width: small.width, height: small.height, rgba: small.levels.into_iter().next().unwrap_or_default(), has_alpha: img.has_alpha })
}

#[cfg(test)]
pub(crate) mod fit_tests {
    #[test]
    fn a_big_picture_is_halved_until_it_fits() {
        let data = omsi_texture::TextureData { width: 8, height: 4, format: omsi_texture::PixelFormat::Rgba8, levels: vec![vec![200; 8 * 4 * 4]], has_alpha: false, gpu_mips: true };
        let small = super::fit_texture(&data, 2).unwrap();
        assert_eq!((small.width, small.height), (2, 1));
        assert_eq!(small.levels[0].len(), 2 * 4);
        assert!(super::fit_texture(&data, 8).is_none());
        // a picture of the game's own, and a size, the same way
        let img = omsi_texture::Image { width: 5000, height: 300, rgba: vec![9; 5000 * 300 * 4], has_alpha: true };
        let small = super::fit_image(&img, 2048).unwrap();
        assert_eq!((small.width, small.height), (1250, 75));
        assert_eq!(small.rgba.len(), 1250 * 75 * 4);
        assert_eq!(super::fit_size(5000, 300, 2048), (1250, 75));
        assert_eq!(super::fit_size(2048, 16, 2048), (2048, 16));
        assert!(super::fit_image(&small, 2048).is_none());
    }
}

impl Renderer {
    /// Put a texture made on another thread ([`prepare_texture`]) into the scene.
    pub fn add_prepared_texture(&self, scene: &mut Scene, texture: PreparedTexture) -> TextureId {
        scene.textures.push(texture.0);
        scene.textures.len() - 1
    }

    pub fn add_texture(
        &self,
        scene: &mut Scene,
        img: &omsi_texture::Image,
        mipmaps: bool,
    ) -> TextureId {
        let fitted = fit_image(img, self.device.limits().max_texture_dimension_2d);
        let img = fitted.as_ref().unwrap_or(img);
        let t = if mipmaps && img.width > 1 && img.height > 1 {
            self.upload_texture_gpu_mips(img)
        } else {
            upload_texture(&self.device, &self.queue, img, false)
        };
        scene.textures.push(t);
        scene.textures.len() - 1
    }

    /// Unconverted RGBA channels for dynamic masks and other data rather than colour.
    pub fn add_data_texture(&self, scene: &mut Scene, img: &omsi_texture::Image) -> TextureId {
        let texture = upload_texture_format(&self.device, &self.queue, img, false, wgpu::TextureFormat::Rgba8Unorm);
        scene.textures.push(texture);
        scene.textures.len() - 1
    }

    /// A transparent texture for a vehicle's displays; `mipmaps`: with the levels its text is drawn into.
    pub fn add_blank_texture(&self, scene: &mut Scene, width: u32, height: u32, mipmaps: bool) -> TextureId {
        let (width, height) = fit_size(width.max(1), height.max(1), self.device.limits().max_texture_dimension_2d);
        let mipmaps = mipmaps && width > 1 && height > 1;
        // A newly allocated GPU texture has undefined contents. Script displays may be
        // sampled before their first `STUnlock`, so initialise them as transparent rather
        // than briefly showing arbitrary solid pixels on new or AI vehicles.
        // (cleared on the GPU where it can: uploading the zeros was 10-35 ms per articulated AI bus)
        let texture = if self.device.features().contains(wgpu::Features::CLEAR_TEXTURE) {
            self.cleared_texture(width, height, mipmaps)
        } else {
            let image = omsi_texture::Image { width, height, rgba: vec![0; (width * height * 4) as usize], has_alpha: true };
            if mipmaps { self.upload_texture_gpu_mips(&image) } else { upload_texture(&self.device, &self.queue, &image, false) }
        };
        scene.textures.push(texture);
        scene.textures.len() - 1
    }

    /// An sRGB texture cleared to transparent on the GPU, made as `upload_texture` (`upload_texture_gpu_mips`) makes it.
    fn cleared_texture(&self, width: u32, height: u32, mipmaps: bool) -> GpuTexture {
        let mip_count = if mipmaps { (32 - width.max(height).leading_zeros()).max(1) } else { 1 };
        let usage = if mipmaps {
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT
        } else {
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST
        };
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let Some(texture) = try_create_texture(&self.device, &wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        }) else {
            return stand_in_texture(&self.device, &self.queue);
        };
        // (submitted now: a script's first picture, written after, then comes after the clear)
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("blank texture") });
        encoder.clear_texture(&texture, &wgpu::ImageSubresourceRange::default());
        self.queue.submit([encoder.finish()]);
        let view = texture.create_view(&Default::default());
        GpuTexture { texture, view, size: (width, height), bytes: texture_bytes(format, width, height, mip_count), generation: next_gen() }
    }

    /// The PBR set found beside diffuse texture `diffuse` (`omsi_texture::pbr`): its maps up
    /// (as data, not colours) and known to the materials made with that texture from now on.
    pub fn add_pbr_maps(&self, scene: &mut Scene, diffuse: TextureId, set: &omsi_texture::pbr::PbrImages) {
        // stored so that the sRGB textures give back the bytes as they are: a normal map's
        // 128 read as 0.22 through the sRGB curve bent every normal
        let lut: Vec<u8> = (0..256)
            .map(|v| {
                let l = v as f32 / 255.0;
                let s = if l <= 0.003_130_8 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
                (s * 255.0 + 0.5).clamp(0.0, 255.0) as u8
            })
            .collect();
        let mut up = |img: &omsi_texture::Image| {
            let data = omsi_texture::Image { width: img.width, height: img.height, rgba: img.rgba.iter().map(|b| lut[*b as usize]).collect(), has_alpha: false };
            self.add_texture(scene, &data, true)
        };
        let normal = set.normal.as_ref().map(&mut up);
        let orm = set.orm.as_ref().map(&mut up);
        scene.pbr_maps.insert(diffuse, PbrMaps { normal, orm, flags: set.flags });
    }

    /// The device takes BC1-3 (DXT) textures.
    pub fn supports_bc(&self) -> bool {
        self.device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
    }

    /// Upload a texture prepared by `omsi_texture::gpu` (blocks with their levels, or RGBA
    /// whose chain is made here).
    pub fn add_texture_data(
        &self,
        scene: &mut Scene,
        data: &omsi_texture::TextureData,
    ) -> TextureId {
        let t = self.upload_texture_data(data);
        scene.textures.push(t);
        scene.textures.len() - 1
    }

    pub(crate) fn upload_texture_data(&self, data: &omsi_texture::TextureData) -> GpuTexture {
        if let Some(small) = fit_texture(data, self.device.limits().max_texture_dimension_2d) {
            return self.upload_texture_data(&small);
        }
        // Also validate the RGBA single-level path, which builds its mip chain on the GPU
        // and therefore returns None from prepare_texture.
        if let Some((level, have, need)) = first_short_mip(data) {
            report_short_mip(data.width.max(1), data.height.max(1), data.format, level, have, need);
            return stand_in_texture(&self.device, &self.queue);
        }
        if let Some(t) = prepare_texture(&self.device, &self.queue, data) {
            return t.0;
        }
        let (w, h) = (data.width.max(1), data.height.max(1));
        let blocks_ok =
            !data.format.is_compressed() || (self.supports_bc() && w % 4 == 0 && h % 4 == 0);
        if !blocks_ok || data.levels.is_empty() {
            // a device without block formats (the loader was told otherwise): decode
            let rgba = match data.levels.first() {
                Some(l) => data.format.decode_level(l, w, h, true),
                None => vec![255; (w * h * 4) as usize],
            };
            return self.upload_texture_gpu_mips(&omsi_texture::Image {
                width: w,
                height: h,
                rgba,
                has_alpha: data.has_alpha,
            });
        }
        // RGBA gets its chain on the GPU (a picture with levels of its own was made by
        // `prepare_texture` above); the picture is borrowed, not copied
        self.upload_rgba_gpu_mips(w, h, &data.levels[0])
    }

    /// Bytes of all textures of the scene on the GPU (render targets included).
    /// Bytes the meshes' vertex and index buffers take on the GPU (the freed ones' shared
    /// placeholder not counted).
    pub fn mesh_bytes(&self, scene: &Scene) -> u64 {
        scene.mesh_page_bytes()
    }

    pub fn texture_bytes(&self, scene: &Scene) -> u64 {
        scene.textures.iter().map(|t| t.bytes).sum()
    }

    /// Bytes of one texture (0 for a freed slot).
    pub fn texture_size_bytes(&self, scene: &Scene, id: TextureId) -> u64 {
        scene.textures.get(id).map(|t| t.bytes).unwrap_or(0)
    }

    /// Upload a texture and build its mip chain on the GPU: level 0 is written, every
    /// further level is the one above drawn at half size.
    pub(crate) fn upload_texture_gpu_mips(&self, img: &omsi_texture::Image) -> GpuTexture {
        self.upload_rgba_gpu_mips(img.width, img.height, &img.rgba)
    }

    pub(crate) fn upload_rgba_gpu_mips(&self, width: u32, height: u32, rgba: &[u8]) -> GpuTexture {
        self.upload_rgba_mips_with(width, height, rgba, MipAverage::ByCoverage)
    }

    fn upload_rgba_mips_with(&self, width: u32, height: u32, rgba: &[u8], average: MipAverage) -> GpuTexture {
        let need = (width as usize).saturating_mul(height as usize).saturating_mul(4);
        if rgba.len() < need {
            report_short_mip(width, height, omsi_texture::PixelFormat::Rgba8, 0, rgba.len(), need);
            return stand_in_texture(&self.device, &self.queue);
        }
        let img = RgbaRef {
            width,
            height,
            rgba,
        };
        let mip_count = (32 - img.width.max(img.height).leading_zeros()).max(1);
        let size = wgpu::Extent3d {
            width: img.width,
            height: img.height,
            depth_or_array_layers: 1,
        };
        let Some(texture) = try_create_texture(&self.device, &wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        }) else {
            return stand_in_texture(&self.device, &self.queue);
        };
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            img.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(img.width * 4),
                rows_per_image: Some(img.height),
            },
            size,
        );
        self.generate_mip_chain(&texture, mip_count, average);
        let view = texture.create_view(&Default::default());
        GpuTexture {
            texture,
            view,
            size: (img.width, img.height),
            bytes: texture_bytes(
                wgpu::TextureFormat::Rgba8UnormSrgb,
                img.width,
                img.height,
                mip_count,
            ),
            generation: next_gen(),
        }
    }

    /// Build all lower levels of `texture` by rendering each level from the last. The
    /// texture has already received its level-zero pixels and was created with render-attach
    /// usage by [`upload_rgba_gpu_mips`].
    pub(crate) fn generate_mip_chain(&self, texture: &wgpu::Texture, mip_count: u32, average: MipAverage) {
        let pipeline = if average == MipAverage::Independent { &self.mip_text_pipeline } else { &self.mip_pipeline };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mips"),
            });
        for level in 1..mip_count {
            let src = texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level - 1,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let dst = texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mip"),
                layout: &self.mip_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.mip_sampler),
                    },
                ],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mip"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
    }

    /// The view of a texture of the scene (to draw into it with another pipeline).
    pub fn texture_view(&self, scene: &Scene, id: TextureId) -> Option<wgpu::TextureView> {
        scene.textures.get(id).map(|t| t.view.clone())
    }

    pub fn upload_speed_mb_s(&self) -> f64 {
        let n = 1024u32;
        let size = wgpu::Extent3d { width: n, height: n, depth_or_array_layers: 1 };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("upload check"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let data = vec![0u8; (n * n * 4) as usize];
        let rounds = 4;
        let t = std::time::Instant::now();
        for _ in 0..rounds {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                &data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(n * 4), rows_per_image: Some(n) },
                size,
            );
        }
        let secs = t.elapsed().as_secs_f64().max(1e-6);
        self.queue.submit([]);
        texture.destroy();
        (data.len() * rounds) as f64 / 1e6 / secs
    }

    /// A texture the scene can be rendered into (`render_to_texture`), e.g. a rear-view mirror.
    pub fn add_render_texture(&self, scene: &mut Scene, width: u32, height: u32) -> TextureId {
        let (width, height) = fit_size(width.max(1), height.max(1), self.device.limits().max_texture_dimension_2d);
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render target"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        // (its depth buffer is the pass's own, see `msaa_targets`)
        let bytes = texture_bytes(self.format, width, height, 1);
        scene.textures.push(GpuTexture {
            texture,
            view,
            size: (width, height),
            bytes,
            generation: next_gen(),
        });
        scene.textures.len() - 1
    }

    /// Render the scene from `camera` into a texture made by `add_render_texture`.
    /// `aspect`: the projection's width over height (OMSI draws its mirrors 1.6 wide into
    /// square textures, and their meshes show the middle of that).
    pub fn render_to_texture(
        &mut self,
        scene: &mut Scene,
        id: TextureId,
        camera: &Camera,
        lighting: &Lighting,
        aspect: f32,
    ) {
        let Some(t) = scene.textures.get(id) else {
            return;
        };
        let view = t.view.clone();
        let (w, h) = t.size;
        self.texture_aspect = Some(aspect);
        self.render_inner(scene, &view, w, h, camera, lighting, false, Some(id), None, false);
        self.texture_aspect = None;
    }

    /// Replace the pixels of a texture (same size as when created, no mipmaps regenerated).
    pub fn update_texture(&self, scene: &Scene, id: TextureId, img: &omsi_texture::Image) {
        let Some(gt) = scene.textures.get(id) else { return };
        // (a picture larger than the chip takes went up halved, see `fit_image`; one larger
        // than its texture is not written - a device error each frame)
        let fitted = fit_image(img, self.device.limits().max_texture_dimension_2d);
        let img = fitted.as_ref().unwrap_or(img);
        if img.width > gt.size.0 || img.height > gt.size.1 || img.rgba.len() < (img.width * img.height * 4) as usize {
            return;
        }
        let t = &gt.texture;
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: t,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &img.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(img.width * 4),
                rows_per_image: Some(img.height),
            },
            wgpu::Extent3d {
                width: img.width,
                height: img.height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn update_texture_rect(&self, scene: &Scene, id: TextureId, img: &omsi_texture::Image, [x, y, w, h]: [u32; 4]) {
        let Some(gt) = scene.textures.get(id) else { return };
        if (img.width, img.height) != gt.size || x + w > img.width || y + h > img.height || img.rgba.len() < (img.width * img.height * 4) as usize {
            return;
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &gt.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &img.rgba[((y * img.width + x) * 4) as usize..],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(img.width * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Replace the top pixels of a dynamic texture and rebuild its mip chain. Returns true
    /// when the texture had to be recreated (and its materials therefore need rebinding).
    /// This is used for OMSI script textures after `STFilter` and for text textures; their
    /// levels average colour and alpha each on its own, as OMSI's do (`MipAverage::Independent`).
    pub fn update_texture_mips(
        &self,
        scene: &mut Scene,
        id: TextureId,
        img: &omsi_texture::Image,
    ) -> bool {
        let Some(old) = scene.textures.get(id) else {
            return false;
        };
        let fitted = fit_image(img, self.device.limits().max_texture_dimension_2d);
        let img = fitted.as_ref().unwrap_or(img);
        let levels = (32 - img.width.max(img.height).leading_zeros()).max(1);
        if old.size != (img.width, img.height) || old.texture.mip_level_count() != levels {
            scene.textures[id] = self.upload_rgba_mips_with(img.width, img.height, &img.rgba, MipAverage::Independent);
            return true;
        }
        self.update_texture(scene, id, img);
        if levels > 1 {
            self.generate_mip_chain(&scene.textures[id].texture, levels, MipAverage::Independent);
        }
        false
    }

    /// Put `data` into texture slot `id` in place of what is there (a picture uploaded as
    /// RGBA to spare a frame, compressed since), and point the materials using it at it.
    pub fn replace_texture(
        &self,
        scene: &mut Scene,
        id: TextureId,
        data: &omsi_texture::TextureData,
    ) {
        if id >= scene.textures.len() {
            return;
        }
        scene.textures[id] = self.upload_texture_data(data);
    }

    /// Refresh a changed file in its existing slot. Reuse the GPU allocation and all
    /// material bind groups when format, dimensions and mip count have not changed.
    /// Returns true only when callers must rebind materials to a replacement texture.
    /// File decoding, compression and fitting to the device limit happen on a worker.
    pub fn refresh_texture(
        &self,
        scene: &mut Scene,
        id: TextureId,
        data: &omsi_texture::TextureData,
    ) -> bool {
        let Some(old) = scene.textures.get(id) else {
            return false;
        };
        use omsi_texture::PixelFormat;
        let format = match data.format {
            PixelFormat::Rgba8 => wgpu::TextureFormat::Rgba8UnormSrgb,
            PixelFormat::Bc1 => wgpu::TextureFormat::Bc1RgbaUnormSrgb,
            PixelFormat::Bc2 => wgpu::TextureFormat::Bc2RgbaUnormSrgb,
            PixelFormat::Bc3 => wgpu::TextureFormat::Bc3RgbaUnormSrgb,
            PixelFormat::Bc7 => wgpu::TextureFormat::Bc7RgbaUnormSrgb,
        };
        let generate = data.format == PixelFormat::Rgba8
            && data.levels.len() == 1
            && data.gpu_mips
            && data.width > 1
            && data.height > 1;
        let levels = if generate {
            (32 - data.width.max(data.height).leading_zeros()).max(1)
        } else {
            data.levels.len() as u32
        };
        if old.bytes == 0
            || old.size != (data.width, data.height)
            || old.texture.format() != format
            || old.texture.mip_level_count() != levels
        {
            self.replace_texture(scene, id, data);
            return true;
        }
        let (bw, bh) = format.block_dimensions();
        let block = format.block_copy_size(None).unwrap_or(4);
        for (level, bytes) in data.levels.iter().enumerate() {
            let width = (data.width >> level).max(1).div_ceil(bw) * bw;
            let height = (data.height >> level).max(1).div_ceil(bh) * bh;
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &old.texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width / bw * block),
                    rows_per_image: Some(height / bh),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        if generate && levels > 1 {
            self.generate_mip_chain(&old.texture, levels, MipAverage::ByCoverage);
        }
        false
    }

    /// Mip levels and size of a texture: (width, height, levels).
    pub fn texture_levels(&self, scene: &Scene, id: TextureId) -> Option<(u32, u32, u32)> {
        let t = scene.textures.get(id)?;
        (t.bytes > 0).then(|| (t.size.0, t.size.1, t.texture.mip_level_count()))
    }

    /// Let go of the `n` finest mip levels of texture `id` (a texture far away while the
    /// textures are over their memory budget): the coarser levels are copied into a smaller
    /// texture on the GPU, which takes the slot. False when the texture cannot shrink so far
    /// (a block format needs sides that are multiples of four). The materials using it
    /// need [`Renderer::rebind_textures`] afterwards. Several at once: [`LevelDrops`].
    pub fn drop_top_levels(&self, scene: &mut Scene, id: TextureId, n: u32) -> bool {
        let mut drops = self.begin_level_drops();
        let done = self.drop_top_levels_in(&mut drops, scene, id, n);
        self.finish_level_drops(drops);
        done
    }

    /// Start shrinking several textures with one command buffer and one submit (see
    /// [`Renderer::drop_top_levels_in`], [`Renderer::finish_level_drops`]): a submit of
    /// its own for each of the budget pass's up to 96 textures cost more than the copies.
    pub fn begin_level_drops(&self) -> LevelDrops {
        LevelDrops { encoder: None, count: 0 }
    }

    /// [`Renderer::drop_top_levels`], its copies recorded into `drops` (the slot takes the
    /// smaller texture at once; the copies run when the drops are finished, before anything
    /// drawn after them).
    pub fn drop_top_levels_in(&self, drops: &mut LevelDrops, scene: &mut Scene, id: TextureId, n: u32) -> bool {
        let Some(t) = scene.textures.get(id) else {
            return false;
        };
        let (w, h) = t.size;
        let levels = t.texture.mip_level_count();
        let format = t.texture.format();
        if n == 0 || n >= levels || !t.texture.usage().contains(wgpu::TextureUsages::COPY_SRC) {
            return false;
        }
        let (nw, nh) = ((w >> n).max(1), (h >> n).max(1));
        let (bw, bh) = format.block_dimensions();
        if nw % bw != 0 || nh % bh != 0 {
            return false;
        }
        let new_levels = levels - n;
        // (no memory for the smaller copy: the texture stays as it is)
        let Some(texture) = try_create_texture(&self.device, &wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: nw,
                height: nh,
                depth_or_array_layers: 1,
            },
            mip_level_count: new_levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        }) else {
            return false;
        };
        let encoder = drops.encoder.get_or_insert_with(|| {
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("drop levels"),
                })
        });
        for l in 0..new_levels {
            let (lw, lh) = ((nw >> l).max(1), (nh >> l).max(1));
            let size = wgpu::Extent3d {
                width: lw.div_ceil(bw) * bw,
                height: lh.div_ceil(bh) * bh,
                depth_or_array_layers: 1,
            };
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &t.texture,
                    mip_level: l + n,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: l,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                size,
            );
        }
        // (the old texture stays alive in the recorded copies until they ran)
        scene.textures[id] =
            GpuTexture::new(texture, (nw, nh), texture_bytes(format, nw, nh, new_levels));
        drops.count += 1;
        true
    }

    /// Submit the copies of [`Renderer::drop_top_levels_in`] (one submit for all of them).
    /// Returns how many textures were shrunk.
    pub fn finish_level_drops(&self, drops: LevelDrops) -> usize {
        if let Some(e) = drops.encoder {
            self.queue.submit([e.finish()]);
        }
        drops.count
    }

    /// Rebuild the bind groups of the materials that sample any of `ids` (after
    /// [`Renderer::replace_texture`]). Returns how many were rebuilt.
    pub fn rebind_textures(&self, scene: &mut Scene, ids: &[TextureId]) -> usize {
        if ids.is_empty() {
            return 0;
        }
        let textures = &scene.textures;
        let pbr_maps = &scene.pbr_maps;
        // (a flag per texture slot: the scan below looks up seven textures of every
        // material of the map, which a hash set made the larger part of the call)
        let mut set = vec![false; textures.len()];
        for &id in ids {
            if let Some(f) = set.get_mut(id) {
                *f = true;
            }
        }
        let mut n = 0;
        for m in scene.materials.iter_mut() {
            let uses = [
                m.texture,
                m.nightmap,
                m.lightmap,
                m.envmap.map(|e| e.0),
                m.transmap.map(|t| t.0),
                m.env_mask,
                m.bump.map(|b| b.0),
            ]
            .iter()
            .flatten()
            .any(|&t| set.get(t).copied().unwrap_or(false));
            if !uses {
                continue;
            }
            m.bind_group = self.material_bind_group(
                textures,
                MaterialMaps {
                    texture: m.texture,
                    transmap: m.transmap,
                    nightmap: m.nightmap,
                    lightmap: m.lightmap,
                    envmap: m.envmap,
                    env_mask: m.env_mask,
                    bump: m.bump,
                    pbr: m.texture.and_then(|t| pbr_maps.get(&t)).copied(),
                },
                m.address,
                &m.buf,
            );
            n += 1;
        }
        n
    }
}

/// Textures being shrunk together (see [`Renderer::begin_level_drops`]).
pub struct LevelDrops {
    encoder: Option<wgpu::CommandEncoder>,
    count: usize,
}

#[cfg(test)]
mod incomplete_mipmap_tests {
    use super::*;

    #[test]
    fn rejects_short_rgba_mips_before_upload() {
        let mut data = omsi_texture::TextureData {
            width: 4,
            height: 4,
            format: omsi_texture::PixelFormat::Rgba8,
            levels: vec![vec![128; 64], vec![64; 15], vec![32; 4]],
            has_alpha: true,
            gpu_mips: false,
        };
        assert_eq!(first_short_mip(&data), Some((1, 15, 16)));
        data.levels[1].push(64);
        assert_eq!(first_short_mip(&data), None);
        data.levels[0].truncate(63);
        assert_eq!(first_short_mip(&data), Some((0, 63, 64)));
    }

    #[test]
    fn compressed_levels_need_complete_blocks_even_below_four_texels() {
        let mut data = omsi_texture::TextureData {
            width: 8,
            height: 8,
            format: omsi_texture::PixelFormat::Bc1,
            levels: vec![vec![0; 32], vec![0; 8], vec![0; 7]],
            has_alpha: false,
            gpu_mips: false,
        };
        assert_eq!(first_short_mip(&data), Some((2, 7, 8)));
        data.levels[2].push(0);
        assert_eq!(first_short_mip(&data), None);
    }

    #[test]
    fn mip_size_stays_at_one_pixel_for_long_chains() {
        let data = omsi_texture::TextureData {
            width: 1,
            height: 1,
            format: omsi_texture::PixelFormat::Rgba8,
            levels: vec![vec![0; 4]; 33],
            has_alpha: false,
            gpu_mips: false,
        };
        assert_eq!(first_short_mip(&data), None);
    }
}
