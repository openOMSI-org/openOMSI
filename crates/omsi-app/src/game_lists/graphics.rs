//! Unsaved graphics edits, confirmed as a batch.
use super::*;

/// Values before the first edit in a graphics batch. Cancel restores graphics only,
/// not keyboard or gameplay settings edited in the meantime.
pub(crate) struct GraphicsBefore {
    runtime: crate::settings::Settings,
    deadline: Option<std::time::Instant>,
}

pub(crate) fn begin_graphics_change(app: &mut App) {
    if app.menus.graphics_before.is_none() {
        // Finish earlier pending writes before capturing the undo snapshot.
        if let Some(saved) = saved_settings() {
            *GRAPHICS_DRAFT.lock().unwrap_or_else(|e| e.into_inner()) = Some(saved.clone());
            app.menus.graphics_before = Some(GraphicsBefore { runtime: app.settings.clone(), deadline: None });
        }
    }
}

pub(crate) fn is_graphics_setting(key: &str) -> bool {
    omsi_launcher_lib::GRAPHICS_PROFILE_KEYS.contains(&key)
        || matches!(key, "mirror_shadows" | "mirror_particles")
}

/// Rebuild shader pipelines/targets once after confirmation, not for every slider step.
/// Never replace the renderer mode or device-dependent texture formats during a drive.
fn session_graphics_options(
    session: &crate::startup::SessionSettings,
    settings: &crate::settings::Settings,
    active: omsi_render::RenderOptions,
) -> omsi_render::RenderOptions {
    let mut options = session.render_options(settings);
    options.no_enhanced = active.no_enhanced;
    options.gpu_texture_compression = active.gpu_texture_compression;
    options.compress_textures = active.compress_textures;
    options.xr_scale = active.xr_scale;
    options.max_scene = active.max_scene;
    // Never switch the active Vanilla/Vanilla+/Enhanced renderer during the drive.
    if !session.enhanced { options.ray_tracing = false; }
    options
}

fn rebuild_session_graphics(app: &mut App) -> bool {
    let options = app.renderer.as_ref().map(|r| {
        session_graphics_options(&app.args.session_settings, &app.settings, r.options)
    }).unwrap_or_else(|| app.args.session_settings.render_options(&app.settings));
    let changed = if let (Some(renderer), Some(scene)) = (app.renderer.as_mut(), app.scene.as_mut()) {
        renderer.rebuild_with(scene, options)
    } else { false };
    sync_live(app);
    changed
}

pub(crate) fn apply_graphics_changes(app: &mut App) {
    if app.menus.graphics_before.take().is_none() { return; }
    let draft = GRAPHICS_DRAFT.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(draft) = draft {
        store_with(app, |v| overlay(v, &draft));
    }
    let changed = rebuild_session_graphics(app);
    app.menus.list_dirty = true;
    let _ = changed;
    app.service_msg = Some((omsi_ui::tr("Graphics settings applied. Restart to change renderer mode or device formats.").into_owned(), 6.0));
}

/// Restore graphic keys but preserve unrelated settings saved while the preview was open.
pub(super) fn overlay(current: &mut serde_json::Value, before: &serde_json::Value) {
    for key in omsi_launcher_lib::GRAPHICS_PROFILE_KEYS {
        if let Some(value) = before.get(key) { current[key] = value.clone(); }
    }
    for key in ["mirror_shadows", "mirror_particles"] {
        if let Some(value) = before.get(key) { current[key] = value.clone(); }
    }
}

pub(crate) fn cancel_graphics_changes(app: &mut App) {
    let Some(before) = app.menus.graphics_before.take() else { return };
    *GRAPHICS_DRAFT.lock().unwrap_or_else(|e| e.into_inner()) = None;
    // Nothing was rebuilt before confirmation: preserve optimiser runtime pipelines.
    reload_settings(app);
    restore_runtime_graphics(&mut app.settings, &before.runtime);
    sync_live(app);
    app.menus.list_dirty = true;
    app.service_msg = Some((omsi_ui::tr("Graphics changes cancelled.").into_owned(), 4.0));
}

// Restore only graphical runtime values; optimiser changes made before editing
// need not have been persisted, and unrelated gameplay changes must survive.
fn restore_runtime_graphics(target: &mut crate::settings::Settings, source: &crate::settings::Settings) {
    target.graphics = source.graphics.clone();
    target.enhanced = source.enhanced.clone();
    target.msaa = source.msaa.clone();
    target.render_scale = source.render_scale.clone();
    target.upscaler = source.upscaler.clone();
    target.fsr_quality = source.fsr_quality.clone();
    target.fsr_sharpness = source.fsr_sharpness.clone();
    target.anisotropy = source.anisotropy.clone();
    target.shadow_size = source.shadow_size.clone();
    target.ssao = source.ssao.clone();
    target.shadows = source.shadows.clone();
    target.shadow_casters = source.shadow_casters.clone();
    target.detail_textures = source.detail_textures.clone();
    target.night_brightness = source.night_brightness.clone();
    target.led_glow = source.led_glow.clone();
    target.led_mips = source.led_mips.clone();
    target.shadow_blobs = source.shadow_blobs.clone();
    target.lamp_shadows = source.lamp_shadows.clone();
    target.reflections = source.reflections.clone();
    target.road_glare = source.road_glare.clone();
    target.traffic_headlights = source.traffic_headlights.clone();
    target.clouds = source.clouds.clone();
    target.cloud_quality = source.cloud_quality.clone();
    target.street_light = source.street_light.clone();
    target.rain_quality = source.rain_quality.clone();
    target.vehicle_smoke = source.vehicle_smoke.clone();
    target.puddles = source.puddles.clone();
    target.windy_trees = source.windy_trees.clone();
    target.road_snow = source.road_snow.clone();
    target.ray_tracing_on = source.ray_tracing_on.clone();
    target.light_shadows = source.light_shadows.clone();
    target.cloud_shadows = source.cloud_shadows.clone();
    target.light_glow = source.light_glow.clone();
    target.vsync = source.vsync.clone();
    target.max_fps = source.max_fps.clone();
    target.max_obj_dist = source.max_obj_dist.clone();
    target.min_obj_size = source.min_obj_size.clone();
    target.mirror_size = source.mirror_size.clone();
    target.mirror_refresh = source.mirror_refresh.clone();
    target.mirror_quality = source.mirror_quality.clone();
    target.mirror_hud_every_frame = source.mirror_hud_every_frame.clone();
    target.video_memory = source.video_memory.clone();
    target.texture_compression = source.texture_compression.clone();
    target.gpu_texture_compression = source.gpu_texture_compression.clone();
    target.mirror_shadows = source.mirror_shadows.clone();
    target.mirror_particles = source.mirror_particles.clone();
}

/// A bounded confirmation window, measured in wall time even while paused.
pub(crate) fn arm_graphics_confirmation(app: &mut App) {
    if let Some(before) = app.menus.graphics_before.as_mut() {
        before.deadline.get_or_insert_with(|| std::time::Instant::now() + std::time::Duration::from_secs(20));
    }
}

pub(crate) fn tick_graphics_confirmation(app: &mut App) {
    if app.menus.graphics_before.as_ref().and_then(|b| b.deadline).is_some_and(|t| std::time::Instant::now() >= t) {
        cancel_graphics_changes(app);
        if let Some(ListKind::GraphicsConfirm(tab)) = app.menus.list_kind.clone() {
            app.open_list(ListKind::Options(tab));
        }
    }
}

#[cfg(test)]
mod graphics_confirm_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cancellation_restores_graphics_without_undoing_gameplay_settings() {
        let old = json!({"graphics":"enhanced", "msaa":2, "road_glare":0.35, "language":"PTB", "passenger_hailing":false});
        let mut edited = json!({"graphics":"vanilla", "msaa":8, "road_glare":1.6, "language":"ENG", "passenger_hailing":true});
        overlay(&mut edited, &old);
        assert_eq!(edited["graphics"], "enhanced");
        assert_eq!(edited["msaa"], 2);
        assert_eq!(edited["road_glare"], 0.35);
        assert_eq!(edited["language"], "ENG");
        assert_eq!(edited["passenger_hailing"], true);
    }

    #[test]
    fn rebuild_preserves_active_renderer_mode_and_device_formats() {
        let mut settings = crate::settings::Settings::from_text("graphics=enhanced\nray_tracing=1\nmsaa=8\n");
        let active = omsi_render::RenderOptions {
            no_enhanced: true,
            gpu_texture_compression: false,
            compress_textures: false,
            max_scene: Some((1280, 720)),
            ..Default::default()
        };
        let session = crate::startup::SessionSettings::default();
        let result = session_graphics_options(&session, &settings, active);
        assert!(result.no_enhanced);
        assert!(!result.ray_tracing, "Vanilla+ cannot acquire Enhanced+ while playing");
        assert!(!result.gpu_texture_compression);
        assert!(!result.compress_textures);
        assert_eq!(result.max_scene, Some((1280, 720)));

        settings.msaa = 2;
        let enhanced = crate::startup::SessionSettings { enhanced: true, ..session };
        let result = session_graphics_options(&enhanced, &settings, active);
        assert_eq!(result.msaa, 2, "MSAA is an option that can be rebuilt in-game");
    }
}
