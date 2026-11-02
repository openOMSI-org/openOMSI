//! The first start: a graphics preset that fits the computer, picked once from its best
//! graphics chip before the renderer opens, while there is no settings file yet.

pub(crate) fn choose(settings: &mut crate::settings::Settings, instance: &wgpu::Instance) -> Option<&'static str> {
    let path = crate::settings::Settings::path()?;
    if cfg!(any(target_os = "android", target_os = "ios")) || path.exists() || omsi_cfg::flags::OMSI_GRAPHICS.is_set() {
        return None;
    }
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
    let best = adapters.iter().min_by_key(|a| rank(a.get_info().device_type))?;
    let info = best.get_info();
    let preset = pick(info.device_type, omsi_render::card_vram_mb(best), &info.name);
    let (_, values) = omsi_launcher_lib::graphics_presets().into_iter().find(|(n, _)| *n == preset)?;
    let mut v = omsi_launcher_lib::get_settings().ok()?;
    if let (Some(dst), Some(src)) = (v.as_object_mut(), values.as_object()) {
        for (k, x) in src {
            dst.insert(k.clone(), x.clone());
        }
    }
    omsi_launcher_lib::save_settings(&v).ok()?;
    *settings = crate::settings::Settings::load();
    log::info!("first start: graphics preset {preset} for {} ({:?})", info.name, info.device_type);
    Some(preset)
}

fn rank(t: wgpu::DeviceType) -> u8 {
    match t {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu | wgpu::DeviceType::Other => 2,
        wgpu::DeviceType::Cpu => 3,
    }
}

fn pick(kind: wgpu::DeviceType, vram_mb: Option<u64>, name: &str) -> &'static str {
    let name = name.to_ascii_lowercase();
    match kind {
        wgpu::DeviceType::DiscreteGpu => match vram_mb {
            Some(m) if m >= 10_000 => "Extra",
            Some(m) if m >= 6_000 => "High",
            Some(m) if m < 3_000 => "Low",
            _ => "Medium",
        },
        wgpu::DeviceType::IntegratedGpu if name.contains("apple") => "High",
        wgpu::DeviceType::IntegratedGpu if name.contains("intel") && (name.contains("hd graphics") || name.contains("uhd")) => "Low",
        wgpu::DeviceType::IntegratedGpu => "Medium",
        _ => "Low",
    }
}

#[cfg(test)]
mod tests {
    use super::pick;
    use wgpu::DeviceType as D;

    #[test]
    fn the_preset_follows_the_graphics_chip() {
        assert_eq!(pick(D::DiscreteGpu, Some(12_288), "NVIDIA GeForce RTX 4070"), "Extra");
        assert_eq!(pick(D::DiscreteGpu, Some(8_192), "AMD Radeon RX 6600"), "High");
        assert_eq!(pick(D::DiscreteGpu, Some(4_096), "NVIDIA GeForce GTX 1650"), "Medium");
        assert_eq!(pick(D::DiscreteGpu, Some(2_048), "NVIDIA GeForce GTX 750"), "Low");
        assert_eq!(pick(D::DiscreteGpu, None, "Unknown card"), "Medium");
        assert_eq!(pick(D::IntegratedGpu, None, "Apple M4 Pro"), "High");
        assert_eq!(pick(D::IntegratedGpu, None, "Intel(R) UHD Graphics 620"), "Low");
        assert_eq!(pick(D::IntegratedGpu, None, "AMD Radeon 680M"), "Medium");
        assert_eq!(pick(D::Cpu, None, "llvmpipe"), "Low");
    }
}
