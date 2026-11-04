//! A support package for a GitHub issue (Setup → Export diagnostics, or
//! `openomsi --export-diagnostics <zip>`): a ZIP the player saves on this computer, looks
//! into and attaches by hand. Nothing is sent anywhere.
//!
//! It is built from an allowlist, never from configuration dumps: the program, the system,
//! the graphics device and the graphics settings, controllers by name and USB id, the
//! session's map and bus (relative content names only) and content roots and archives as
//! aliases. Free-text logs can hold chat, LAN codes, addresses, names and paths, so of the
//! logs only fixed event names and the stutter lines' frame/time numbers are copied.

use serde_json::{json, Value};

pub(crate) use omsi_platform::support_bundle::{clean_json, content_name, export, redact_label, session_dir};

/// The session a snapshot is of: map, bus, line and tour.
pub(crate) type Session<'a> = (&'a str, Option<&'a str>, Option<&'a str>, Option<&'a str>);

pub(crate) fn snapshot(
    renderer: Option<&omsi_render::Renderer>,
    settings: &crate::settings::Settings,
    devices: Option<Vec<crate::controllers::Connected>>,
    session: Option<Session>,
    source: &str,
) -> Value {
    let controllers: Option<Vec<Value>> = devices.map(|d| d.into_iter().map(|d| {
        json!({"name": redact_label(&d.name), "vid_pid": d.hardware_id,
            "axes": d.axes.len(), "buttons": d.buttons, "gamepad": d.gamepad,
            "ffb_capable": d.ff_capable, "ffb_effect_available": d.ff})
    }).collect());
    let roots: Vec<Value> = omsi_cfg::content_roots().iter().enumerate()
        .map(|(i, _)| json!({"alias": format!("root-{}", i + 1), "priority": i})).collect();
    let archives: Vec<Value> = omsi_cfg::vfs::mounts().iter().enumerate().map(|(i, a)| {
        json!({"alias": format!("archive-{}", i + 1), "files": a.file_count(), "bytes": a.total_size()})
    }).collect();
    let mut value = machine(renderer, settings, session, source);
    value["content_roots"] = json!(roots);
    value["mounted_archives"] = json!(archives);
    value["controllers"] = json!(controllers);
    clean_json(&mut value);
    value
}

fn machine(renderer: Option<&omsi_render::Renderer>, settings: &crate::settings::Settings, session: Option<Session>, source: &str) -> Value {
    let info = omsi_render::ADAPTER_INFO.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let graphics = renderer.map(|r| {
        let limits = r.device.limits();
        json!({
            "backend": info.as_ref().map(|i| format!("{:?}", i.backend)),
            "adapter": info.as_ref().map(|i| redact_label(&i.name)),
            "device_type": info.as_ref().map(|i| format!("{:?}", i.device_type)),
            "vendor_id": info.as_ref().map(|i| i.vendor), "device_id": info.as_ref().map(|i| i.device),
            "driver": info.as_ref().map(|i| redact_label(&i.driver)),
            "driver_info": info.as_ref().map(|i| redact_label(&i.driver_info)),
            "device_features": r.device.features().iter_names().map(|(n, _)| n).collect::<Vec<_>>(),
            "device_limits": {"max_texture_dimension_2d": limits.max_texture_dimension_2d,
                "max_buffer_size": limits.max_buffer_size,
                "max_storage_buffer_binding_size": limits.max_storage_buffer_binding_size,
                "max_bind_groups": limits.max_bind_groups,
                "max_sampled_textures_per_shader_stage": limits.max_sampled_textures_per_shader_stage,
                "max_storage_buffers_per_shader_stage": limits.max_storage_buffers_per_shader_stage},
            "device_lost": r.device_lost().is_some(),
        })
    });
    // (what the game's renderer ended up with after its fallbacks; the launcher's own
    // renderer draws with other settings, so it says nothing of the game's)
    let effective = renderer.filter(|_| source == "game").map(|r| json!({
        "msaa": r.options.msaa, "ssao": r.options.ssao,
        "render_scale": r.options.render_scale, "shadow_size": r.options.shadow_size,
        "basic_pipelines": omsi_render::basic_pipelines(),
    }));
    json!({
        "schema_version": 1,
        "recorded_unix_seconds": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|t| t.as_secs()).unwrap_or(0),
        "source": source, "version": crate::startup::VERSION, "build": crate::startup::BUILD,
        "os": std::env::consts::OS, "architecture": std::env::consts::ARCH,
        "os_version": crate::applog::os_version(),
        "graphics": graphics,
        "graphics_effective": effective,
        "graphics_settings": {"preset": settings.graphics, "backend_requested": settings.graphics_api,
            "render_scale": settings.render_scale, "msaa": settings.msaa, "ssao": settings.ssao,
            "shadows": settings.shadows, "shadow_size": settings.shadow_size,
            "mirrors": settings.mirror_refresh, "mirror_size": settings.mirror_size,
            "reflections": settings.reflections, "vsync": settings.vsync, "max_fps": settings.max_fps},
        "session": session.map(|(map, bus, line, tour)| json!({"map": content_name(map),
            "bus": bus.map(content_name), "line": line.map(redact_label), "tour": tour.map(redact_label)})),
        "physical_memory_bytes": crate::memory::physical_memory(),
        "adapter_vram_mb": omsi_render::ADAPTER_VRAM_MB.load(std::sync::atomic::Ordering::Relaxed),
        "adapter_texture_mb": omsi_render::ADAPTER_TEXTURE_MB.load(std::sync::atomic::Ordering::Relaxed),
    })
}

pub(crate) fn diagnostics(app: &crate::App) -> Value {
    let mut value = machine(app.renderer.as_ref(), &app.settings,
        Some((&app.args.map, app.args.bus.as_deref(), app.args.line.as_deref(), app.args.tour.as_deref())), "game");
    value["frame_times"] = app.perf.log_state.frame_times.to_json();
    value
}

/// A game started from the launcher keeps its snapshot (at its first frame and when the
/// graphics device is lost), so that an export after it ended - or crashed - still says
/// what it ran with. Snapshots of games the launcher no longer lists are removed.
pub(crate) fn record(app: &crate::App) {
    let Some(id) = omsi_cfg::flags::OMSI_INSTANCE.var().filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')) else { return };
    let mut value = snapshot(app.renderer.as_ref(), &app.settings, app.input.controllers.as_ref().map(|c| c.connected()),
        Some((&app.args.map, app.args.bus.as_deref(), app.args.line.as_deref(), app.args.tour.as_deref())), "game");
    if let (Some(r), Some(scene)) = (app.renderer.as_ref(), app.scene.as_ref()) {
        value["gpu_memory_bytes"] = json!({"textures": r.texture_bytes(scene), "meshes": r.mesh_bytes(scene)});
    }
    let dir = session_dir();
    let _ = std::fs::create_dir_all(&dir);
    let listed: Vec<String> = omsi_launcher_lib::list_instances().into_iter().map(|i| i.id).collect();
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let p = e.path();
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        if stem != id && !listed.contains(&stem) {
            let _ = std::fs::remove_file(p);
        }
    }
    let path = dir.join(format!("{id}.json"));
    let temp = path.with_extension("tmp");
    if let Ok(bytes) = serde_json::to_vec_pretty(&value) {
        if std::fs::write(&temp, bytes).is_ok() {
            // Windows cannot rename over an existing file.
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::rename(temp, path);
        }
    }
}

