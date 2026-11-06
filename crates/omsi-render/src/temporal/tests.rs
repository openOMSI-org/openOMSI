use super::frame::{halton, jitter_phases, opaque, sl_matrix, temporal_mip_bias};
use super::*;
use glam::{Mat4, Vec3};

#[test]
fn the_settings_words_read_back() {
    for u in [TemporalUpscaler::Dlss, TemporalUpscaler::Fsr3] {
        assert_eq!(TemporalUpscaler::parse(u.as_str()), Some(u));
    }
    assert_eq!(TemporalUpscaler::parse(" DLSS "), Some(TemporalUpscaler::Dlss));
    for word in ["default", "fsr1", ""] {
        assert_eq!(TemporalUpscaler::parse(word), None);
    }
    for q in [TemporalQuality::Native, TemporalQuality::Quality, TemporalQuality::Balanced, TemporalQuality::Performance, TemporalQuality::UltraPerformance] {
        assert_eq!(TemporalQuality::parse(q.as_str()), q);
    }
    assert_eq!(TemporalQuality::parse("anything_else"), TemporalQuality::Quality);
}

#[test]
fn the_picture_size_follows_the_published_ratios() {
    assert_eq!(fallback_render_size(TemporalQuality::Native, (2560, 1440)), (2560, 1440));
    assert_eq!(fallback_render_size(TemporalQuality::Performance, (3840, 2160)), (1920, 1080));
    assert_eq!(fallback_render_size(TemporalQuality::UltraPerformance, (1, 1)), (1, 1));
}

/// The jitter walks the Halton (2, 3) sequence with more steps the more is scaled up, and the
/// matrices reach Streamline with the translation in the last row (glam's columns are its rows).
#[test]
fn jitter_and_matrix_layout() {
    assert_eq!([halton(1, 2), halton(2, 2), halton(3, 2)], [0.5, 0.25, 0.75]);
    assert!((halton(1, 3) - 1.0 / 3.0).abs() < 1e-6 && (halton(2, 3) - 2.0 / 3.0).abs() < 1e-6);
    assert_eq!(jitter_phases(2560, 2560), 8);
    assert_eq!(jitter_phases(1280, 2560), 32);
    assert_eq!(jitter_phases(640, 3840), 64);
    let m = sl_matrix(Mat4::from_translation(Vec3::new(4.0, 5.0, 6.0)));
    assert_eq!(m[3], [4.0, 5.0, 6.0, 1.0]);
}

#[test]
fn a_screenshot_keeps_the_colours_and_is_opaque() {
    assert_eq!(opaque(vec![1, 2, 3, 0, 4, 5, 6, 128]), [1, 2, 3, 255, 4, 5, 6, 255]);
}

#[test]
fn the_mip_bias_is_the_log_of_the_scale() {
    assert_eq!(temporal_mip_bias(2560, 2560), 0.0);
    assert_eq!(temporal_mip_bias(1280, 2560), -1.0);
    assert_eq!(temporal_mip_bias(1, 3840), -3.0);
}

/// FSR 3 has its own, weaker mask on the panes and none on the screens: at DLSS's strength the
/// digits of a bus's displays came out with doubled edges and the view through the glass soft.
#[test]
fn fsr3_masks_the_glass_less_and_leaves_the_screens_out() {
    let motion = include_str!("motion.wgsl");
    assert!(motion.contains("fn fs_glass_mask_fsr("));
    assert!(motion.contains("const FSR_GLASS_SHARE: f32 = 0.4;"));
    for upscaler in [TemporalUpscaler::Dlss, TemporalUpscaler::Fsr3] {
        assert!(motion.contains(&format!("fn {}(", upscaler.glass_mask_entry())));
    }
    assert_eq!(TemporalUpscaler::Fsr3.glass_mask_entry(), "fs_glass_mask_fsr");
    assert_ne!(TemporalUpscaler::Dlss.glass_mask_entry(), TemporalUpscaler::Fsr3.glass_mask_entry());
    assert!(TemporalUpscaler::Dlss.masks_screens());
    assert!(!TemporalUpscaler::Fsr3.masks_screens());
}

/// Streamline's Vulkan `getFormat` has no VK_FORMAT_R8_UNORM: a mask of that format logged four
/// errors a frame, with `game.log` growing by megabytes a minute.
#[test]
fn the_mask_has_a_format_streamline_can_read_back_on_vulkan() {
    assert_ne!(super::frame::TEMPORAL_MASK_FORMAT, wgpu::TextureFormat::R8Unorm);
}

/// A minimised window is 1x1, and NGX refuses that for good: it must never reach the upscaler.
#[test]
fn a_minimised_window_is_not_upscaled() {
    assert!(!super::frame::window_fits((1, 1)));
    assert!(!super::frame::window_fits((3264, 1)));
    assert!(super::frame::window_fits((640, 360)));
}
