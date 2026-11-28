use crate::{Image, PixelFormat, TextureData, bc, gpu};

fn picture(size: u32, clear: bool) -> Image {
    let rgba = (0..size * size)
        .flat_map(|i| {
            if i % size % 2 == 0 && i / size % 2 == 0 {
                [40, 80, 120, if clear { 0 } else { 255 }]
            } else {
                [255, 255, 255, 0]
            }
        })
        .collect();
    Image {
        width: size,
        height: size,
        rgba,
        has_alpha: true,
    }
}

#[test]
fn large_temporary_texture_keeps_visible_colour_when_halved() {
    for size in [256, 2048] {
        let small = gpu::halved_for_now(TextureData::from_image(picture(size, false)));
        assert_eq!((small.width, small.height), (size / 2, size / 2));
        for px in small.levels[0].chunks_exact(4) {
            assert!(
                px[..3]
                    .iter()
                    .zip([40u8, 80, 120])
                    .all(|(a, b)| a.abs_diff(b) <= 1),
                "{px:?}"
            );
            assert_eq!(px[3], 64);
        }
    }
}

#[test]
fn compressed_mips_keep_visible_colour_and_fractional_alpha() {
    let (texture, info) = gpu::prepare_image_with(
        picture(128, false),
        gpu::GpuOptions {
            bc: true,
            compress: true,
        },
    );
    assert!(info.encoded, "{info:?}");
    assert_eq!(texture.format, PixelFormat::Bc3);
    for (level, bytes) in texture.levels.iter().enumerate().skip(1) {
        let side = (texture.width >> level).max(1);
        let rgba = bc::decode(bytes, side, side, bc::Bc::Bc3);
        for px in rgba.chunks_exact(4) {
            assert!(
                px[..3]
                    .iter()
                    .zip([40u8, 80, 120])
                    .all(|(a, b)| a.abs_diff(b) <= 3),
                "mip {level}: {px:?}"
            );
            assert!(px[3].abs_diff(64) <= 1, "mip {level}: {px:?}");
        }
    }
}

#[test]
fn entirely_clear_colour_and_data_maps_keep_their_rgb() {
    // Some opaque materials use alpha only as a gloss mask; no covered texels must
    // not erase their colour. Height/data maps keep the existing unweighted filter.
    let image = picture(256, true);
    let small = gpu::halved_for_now(TextureData::from_image(image.clone()));
    let (plain, _, _) = bc::downsample(&image.rgba, image.width, image.height);
    assert_eq!(small.levels[0], plain);
    let opaque = [40, 80, 120, 255].repeat(256 * 256);
    let small = gpu::halved_for_now(TextureData::from_image(Image {
        width: 256,
        height: 256,
        rgba: opaque,
        has_alpha: false,
    }));
    assert!(
        small.levels[0]
            .chunks_exact(4)
            .all(|p| p == [40, 80, 120, 255])
    );
}
