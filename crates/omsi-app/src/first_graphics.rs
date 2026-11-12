use std::path::Path;

const IMPORTED_FROM_OMSI: [&str; 5] = ["min_obj_size", "max_obj_dist", "anisotropy", "mirror_size", "mirror_refresh"];

pub(crate) fn choose(app: &mut crate::App) -> Option<&'static str> {
    let path = crate::settings::Settings::path()?;
    if cfg!(any(target_os = "android", target_os = "ios")) || !first_start(path.parent()?) || omsi_cfg::flags::OMSI_GRAPHICS.is_set() {
        return None;
    }
    let adapters = pollster::block_on(app.gfx.instance.enumerate_adapters(wgpu::Backends::all()));
    let best = adapters.iter().min_by_key(|a| rank(a.get_info().device_type))?;
    let info = best.get_info();
    let preset = pick(info.device_type, omsi_render::card_vram_mb(best), &info.name);
    let (_, values) = omsi_launcher_lib::graphics_presets().into_iter().find(|(n, _)| *n == preset)?;
    let mut v = omsi_launcher_lib::get_settings().ok()?;
    merge(&mut v, &values);
    omsi_launcher_lib::save_settings(&v).ok()?;
    take(app, crate::settings::Settings::load());
    log::info!("first start: graphics preset {preset} for {} ({:?})", info.name, info.device_type);
    Some(preset)
}

fn first_start(dir: &Path) -> bool {
    ["settings.cfg", "sessions", "cache/load-times.json"].iter().all(|f| !dir.join(f).exists())
}

fn merge(v: &mut serde_json::Value, preset: &serde_json::Value) {
    let imported = v.get("imported_from_omsi").and_then(|x| x.as_bool()).unwrap_or(false);
    if let (Some(dst), Some(src)) = (v.as_object_mut(), preset.as_object()) {
        for (k, x) in src {
            if !(imported && IMPORTED_FROM_OMSI.contains(&k.as_str())) {
                dst.insert(k.clone(), x.clone());
            }
        }
    }
}

fn take(app: &mut crate::App, settings: crate::settings::Settings) {
    app.settings = settings;
    app.args.session_settings = crate::startup::SessionSettings::for_session(&app.settings, &app.args);
    app.session.sky.look = app.args.session_settings;
    app.session.spray.graphics = app.args.session_settings.graphics();
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
        wgpu::DeviceType::Cpu | wgpu::DeviceType::VirtualGpu => "Low",
        _ if name.contains("apple") => "High",
        _ if name.contains("intel") && (name.contains("hd graphics") || name.contains("uhd")) => "Low",
        wgpu::DeviceType::IntegratedGpu => "Medium",
        _ => match vram_mb {
            Some(m) if m >= 10_000 => "Extra",
            Some(m) if m >= 6_000 => "High",
            Some(m) if m < 3_000 => "Low",
            _ => "Medium",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{first_start, merge, pick, take};
    use clap::Parser;
    use serde_json::json;
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
        assert_eq!(pick(D::Other, Some(8_192), "NVIDIA GeForce RTX 3070/PCIe/SSE2"), "High");
        assert_eq!(pick(D::Other, None, "Mesa Intel(R) HD Graphics 520"), "Low");
        assert_eq!(pick(D::Other, None, "AMD Radeon RX 580"), "Medium");
    }

    #[test]
    fn a_player_who_played_before_is_no_first_start() {
        let dir = std::env::temp_dir().join(format!("openomsi-first-start-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("cache")).unwrap();
        assert!(first_start(&dir));
        std::fs::create_dir_all(dir.join("sessions")).unwrap();
        assert!(!first_start(&dir));
        std::fs::remove_dir_all(dir.join("sessions")).unwrap();
        std::fs::write(dir.join("cache").join("load-times.json"), "{}").unwrap();
        assert!(!first_start(&dir));
        std::fs::remove_file(dir.join("cache").join("load-times.json")).unwrap();
        std::fs::write(dir.join("settings.cfg"), "").unwrap();
        assert!(!first_start(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_options_brought_from_omsi_2_stay() {
        let preset = json!({"graphics": "vanilla", "anisotropy": 2, "mirror_size": 128});
        let mut imported = json!({"imported_from_omsi": true, "anisotropy": 16, "mirror_size": 512});
        merge(&mut imported, &preset);
        assert_eq!((&imported["graphics"], &imported["anisotropy"], &imported["mirror_size"]), (&json!("vanilla"), &json!(16), &json!(512)));
        let mut fresh = json!({"anisotropy": 8});
        merge(&mut fresh, &preset);
        assert_eq!(fresh["anisotropy"], json!(2));
    }

    #[test]
    fn the_first_picture_is_drawn_with_the_preset() {
        let mut app = crate::assemble_app(crate::cli::Args::parse_from(["openomsi"]), crate::settings::Settings::from_text("graphics=vanilla_plus\nmirror_size=256\n"));
        take(&mut app, crate::settings::Settings::from_text("graphics=vanilla\nmirror_size=128\nrain_quality=low\n"));
        assert!(app.args.session_settings.classic);
        assert_eq!(app.args.session_settings.mirror_size, 128);
        assert_eq!(app.session.spray.graphics, crate::scene::host::GraphicsKind::Classic);
        let mut app = crate::assemble_app(crate::cli::Args::parse_from(["openomsi"]), crate::settings::Settings::from_text("graphics=vanilla_plus\n"));
        take(&mut app, crate::settings::Settings::from_text("graphics=enhanced\n"));
        assert!(app.args.session_settings.enhanced);
        assert!(app.session.sky.look.enhanced);
        assert_eq!(app.session.spray.graphics, crate::scene::host::GraphicsKind::Enhanced);
    }
}
