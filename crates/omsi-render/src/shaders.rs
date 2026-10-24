//! The WGSL sources that are too long for one file, kept as parts in `shaders/<module>/` and
//! put back together at compile time, byte for byte: `SCENE` is what was `shader.wgsl`, and
//! `ENHANCED` what was `enhanced.wgsl` (comments elsewhere still call them so).
//!
//! The parts are plain cuts between top-level items, joined with nothing in between, so the
//! GPU receives exactly the text it did from the single files. That matters: Apple's Metal
//! compiler (M4) is at its limit with the scene module, and any change to the text it gets,
//! however harmless it looks, has to be tried there. The tests below hold the assembled text
//! to the old files' size and hash; a deliberate edit of a part updates them.

mod sources;

pub(crate) use sources::*;

/// The scene module's own part: the vanilla path, the shadow passes, the rain on the glass
/// (`shader.wgsl`). Joined with the shared parts in `scene_shader_text`.
pub(crate) const SCENE: &str = concat!(
    include_str!("shaders/scene/01_camera.wgsl"),
    include_str!("shaders/scene/02_bindings.wgsl"),
    include_str!("shaders/scene/03_vertex.wgsl"),
    include_str!("shaders/scene/04_shadow_passes.wgsl"),
    include_str!("shaders/scene/05_sun_shadow.wgsl"),
    include_str!("shaders/scene/06_lights.wgsl"),
    include_str!("shaders/scene/07_patterns_weather.wgsl"),
    include_str!("shaders/scene/08_rain_glass.wgsl"),
    include_str!("shaders/scene/09_vanilla.wgsl"),
);

/// The enhanced fragment shader (`enhanced.wgsl`): its lighting model, the lamps, the
/// surface's shading and the puddles' reflections.
pub(crate) const ENHANCED: &str = concat!(
    include_str!("shaders/enhanced/01_brdf_shadow.wgsl"),
    include_str!("shaders/enhanced/02_lamps.wgsl"),
    include_str!("shaders/enhanced/03_surface.wgsl"),
    include_str!("shaders/enhanced/04_shade.wgsl"),
    include_str!("shaders/enhanced/05_puddles.wgsl"),
);

/// FSR 1's passes for a picture of `format`: on an sRGB one they take its texels to gamma
/// and back (`SRGB_TARGET` in `fsr1.wgsl`), on another they read its values as they are.
/// With `raw_input` (`fsr1_raw_input`) they read an sRGB scene's stored values as they are.
pub(crate) fn fsr1_source_for(format: wgpu::TextureFormat, raw_input: bool) -> String {
    let source = include_str!("fsr1.wgsl");
    let source = if format.is_srgb() {
        source.to_string()
    } else {
        source.replace("const SRGB_TARGET: bool = true;", "const SRGB_TARGET: bool = false;")
    };
    if raw_input && format.is_srgb() {
        source.replace("const RAW_INPUT: bool = false;", "const RAW_INPUT: bool = true;")
    } else {
        source
    }
}

/// OMSI_FSR_RAW_INPUT on a device that can read a texture through a view of another format
/// (not OpenGL): FSR 1 reads the scene's sRGB values without decoding them.
pub(crate) fn fsr1_raw_input(format: wgpu::TextureFormat) -> bool {
    format.is_srgb() && !crate::gl_backend() && omsi_cfg::flags::OMSI_FSR_RAW_INPUT.is_set()
}

#[cfg(test)]
mod tests {
    #[test]
    fn fsr1_shader_parses_and_validates() {
        // No GPU or OMSI installation required. Run even though FSR is off by default.
        // Both variants: an sRGB window (gamma 2.0 in and out) and one that is not.
        for format in [wgpu::TextureFormat::Bgra8UnormSrgb, wgpu::TextureFormat::Rgba8Unorm] {
            let source = super::fsr1_source_for(format, false);
            assert_eq!(source.contains("const SRGB_TARGET: bool = true;"), format.is_srgb(), "{format:?}");
            let module = naga::front::wgsl::parse_str(&source).expect("FSR1 WGSL must parse");
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .expect("FSR1 WGSL must validate");
        }
    }

    /// FNV-1a, 64 bits: enough to tell one text from another, and no dependency for it.
    fn fnv1a(text: &str) -> u64 {
        text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
    }

    /// The assembled modules are byte for byte the single files they were cut from (sizes
    /// and hashes taken of `shader.wgsl` and `enhanced.wgsl` before the cut, updated for
    /// 0.2.30's capped surface pull and the Vanilla/transparency fixes in both modules,
    /// 0.2.42's night sphere map and lacquer, people lit as bodies, not leaves, the far
    /// map's shadow where no traced one is found, 0.2.51's clear coat that stays sharp on
    /// lacquer, the draw order's depth step of a surface lying on another of its object, and a
    /// livery's tint above the alpha test, the painted lines' pull and the wet paint's water film, and the player's vehicle
    /// standing between other vehicles' lamps and what lies behind it, and clear vehicle
    /// lamps taking coverage from their active night map, and #3019's larger rain caps, and the tracked panes' snow laid out in the film mesh's own
    /// metres, and the lamps' shadow maps by the tile their record names and the saloon's visibility term
    /// with the ground no longer lit by the tile light map, and the street-scale
    /// occlusion, the bounced light and the lightning's light, and the sun's share through the clouds' shadow field, and the cube maps of lamps that shine every way, and the wet roads' and the water's traced reflection), and the textures read with FSR 1's mip bias, `camera.lod`.
    /// (with Unix line ends: a Windows checkout may turn the parts' into CRLF, as it did the
    /// single files', and the 0.2.31 release's Windows test failed on that alone)
    #[test]
    fn assembled_shaders_are_the_old_files() {
        let (scene, enhanced) = (super::SCENE.replace("\r\n", "\n"), super::ENHANCED.replace("\r\n", "\n"));
        assert_eq!((scene.len(), fnv1a(&scene)), (130_362, 0x4f94b7116cd84139));
        assert_eq!((enhanced.len(), fnv1a(&enhanced)), (92_987, 0xe1a0398ea5e3172b));
    }
}
