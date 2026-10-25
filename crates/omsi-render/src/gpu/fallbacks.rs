//! Building with less on devices that fail a pipeline: the optional pipelines, and the file
//! that remembers per adapter what had to be left out.

use crate::*;

/// The renderer is built without the pipelines that are not needed to draw a picture (the
/// snowfall, the lamps in the fog, the street lamps' shadow maps): a driver whose shader
/// compiler fails on one of them (it answers "out of memory" or an unknown error - phones'
/// drivers do) takes the whole device with it, and every frame after that came out black
/// (see `Renderer::new_on`). Once set, it stays for the rest of the run. OMSI_BASIC_PIPELINES=1
/// asks for it.
pub(crate) static BASIC_PIPELINES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn basic_pipelines() -> bool {
    BASIC_PIPELINES.load(std::sync::atomic::Ordering::Relaxed) || omsi_cfg::flags::OMSI_BASIC_PIPELINES.is_set()
}
pub(crate) static RAINFALL_LEFT_OUT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(crate) fn rainfall_pipeline() -> bool {
    !basic_pipelines() || !(cfg!(any(target_os = "android", target_os = "ios")) || RAINFALL_LEFT_OUT.load(std::sync::atomic::Ordering::Relaxed) || omsi_cfg::flags::OMSI_SAFE_MODE.is_set())
}

/// One of the pipelines a picture can do without (the snowfall, the lamps in the fog, the
/// street lamps' shadow maps), made in an error scope of its own: where the driver fails on
/// it, that one is left out and the rest of the renderer stands. Caught by the whole
/// build's scope instead, the renderer was made again without all three - one failing on
/// DirectX 12 took the snowfall with it, and the adapter was remembered so for good
/// (`fallback_load`). A device lost on it is not caught here (see `Renderer::new_on`).
pub(crate) fn optional_pipeline<T>(device: &wgpu::Device, what: &str, make: impl FnOnce() -> T) -> Option<T> {
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let made = make();
    let errors = pollster::block_on(async { [validation.pop().await, memory.pop().await, internal.pop().await] });
    match errors.iter().flatten().next() {
        None => Some(made),
        Some(e) => {
            log::warn!("renderer: {what} left out - the driver failed on it: {}", gpu_error_text(e));
            None
        }
    }
}
/// The file that remembers, per graphics adapter, the reduced renderer that worked there
/// (`~/.openomsi/gpu-fallback.cfg`, lines `adapter|msaa|basic@<version>`: the game's version
/// that found the full set failing, see `build_with_fallbacks`; older lines have no version).
pub(crate) fn fallback_path() -> Option<std::path::PathBuf> {
    Some(omsi_cfg::user_data::base_dir()?.join("gpu-fallback.cfg"))
}

pub(crate) fn parse_fallbacks(text: &str) -> Vec<(String, u32, bool, String, bool)> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.rsplitn(3, '|');
            let last = f.next()?.trim();
            let (basic, version) = last.split_once('@').map_or((last == "1", ""), |(b, v)| (b == "1", v));
            let (version, no_rain) = version.strip_suffix("+norain").map_or((version, false), |v| (v, true));
            let msaa = f.next()?.trim().parse().ok()?;
            Some((f.next()?.to_string(), msaa, basic, version.to_string(), no_rain))
        })
        .collect()
}

/// The game's version, for the remembered fallbacks.
pub(crate) fn game_version() -> &'static str {
    option_env!("OPENOMSI_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// The reduced renderer (MSAA, basic pipelines) that worked on adapter `name` before, and
/// whether it was found by this version of the game.
pub(crate) fn fallback_load(name: &str) -> Option<(u32, bool, bool)> {
    let text = std::fs::read_to_string(fallback_path()?).ok()?;
    let e = parse_fallbacks(&text).into_iter().find(|e| e.0 == name)?;
    if e.4 && e.3 == game_version() {
        RAINFALL_LEFT_OUT.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Some((e.1, e.2, e.3 == game_version()))
}

/// Remember (`Some`) or forget (`None`) the reduced renderer for adapter `name`.
pub(crate) fn fallback_store(name: &str, what: Option<(u32, bool)>) {
    let Some(path) = fallback_path() else { return };
    let mut all = std::fs::read_to_string(&path).map(|t| parse_fallbacks(&t)).unwrap_or_default();
    all.retain(|e| e.0 != name);
    if let Some((m, b)) = what {
        let no_rain = b && (RAINFALL_LEFT_OUT.load(std::sync::atomic::Ordering::Relaxed) || omsi_cfg::flags::OMSI_SAFE_MODE.is_set());
        all.push((name.to_string(), m, b, game_version().to_string(), no_rain));
    }
    let text: String = all
        .iter()
        .map(|(n, m, b, v, r)| if v.is_empty() { format!("{n}|{m}|{}\n", *b as u8) } else { format!("{n}|{m}|{}@{v}{}\n", *b as u8, if *r { "+norain" } else { "" }) })
        .collect();
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(path, text);
}

#[cfg(test)]
pub(crate) mod fallback_tests {
    #[test]
    fn a_remembered_fallback_is_read_back() {
        let e = super::parse_fallbacks("Adreno (TM) 830 (Gl)|1|1\nNVIDIA | odd (Vulkan)|4|0\nRadeon (Dx12)|4|1@0.2.27\nApple M1 (Metal)|1|1@0.3.35+norain\nbroken\n");
        assert_eq!(e, vec![
            ("Adreno (TM) 830 (Gl)".to_string(), 1, true, String::new(), false),
            ("NVIDIA | odd (Vulkan)".to_string(), 4, false, String::new(), false),
            ("Radeon (Dx12)".to_string(), 4, true, "0.2.27".to_string(), false),
            ("Apple M1 (Metal)".to_string(), 1, true, "0.3.35".to_string(), true),
        ]);
        // (a multisampling fallback carries the version that found it too)
        assert_eq!(super::parse_fallbacks("NVIDIA GeForce RTX 3060 (Vulkan)|1|0@0.3.1\n"), vec![("NVIDIA GeForce RTX 3060 (Vulkan)".to_string(), 1, false, "0.3.1".to_string(), false)]);
    }
}
