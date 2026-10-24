use super::Settings;

#[test]
fn fsr1_menu_settings_round_trip_into_renderer() {
    let mut ui = omsi_launcher_lib::settings_from_text(None);
    assert_eq!(ui["upscaler"], "default");
    ui["upscaler"] = serde_json::json!("fsr1");
    ui["fsr_quality"] = serde_json::json!("balanced");
    ui["fsr_sharpness"] = serde_json::json!(0.45);
    let saved = omsi_launcher_lib::settings_to_text(&ui, None);
    let game = Settings::from_text(&saved);
    assert_eq!(game.upscaler, "fsr1");
    assert_eq!(game.fsr_quality, "balanced");
    assert!((game.fsr_sharpness - 0.45).abs() < 0.0001);
    let graphics = game.render_options();
    assert!(graphics.fsr1);
    assert!((graphics.render_scale - 0.59).abs() < 0.0001);
    assert!((graphics.fsr_sharpness - 0.45).abs() < 0.0001);
    assert_eq!(Settings::from_text(&game.to_text()), game);
    assert_eq!(omsi_launcher_lib::settings_from_text(Some(&saved))["fsr_quality"], "balanced");

    // A corrupt or future setting must not enable experimental shaders.
    assert_eq!(Settings::from_text("upscaler=anything_else\\n").upscaler, "default");
    assert_eq!(Settings::from_text("fsr_sharpness=nan\\n").fsr_sharpness, 0.8);
}

#[test]
fn fsr1_presets_keep_the_render_scale_caps() {
    let mut s = Settings::from_text("upscaler=fsr1\nfsr_quality=ultra\n");
    assert!((s.scene_render_scale() - 0.77).abs() < 0.0001);
    // (the optimiser's step down, or the safer graphics after a second lost device)
    s.apply_safe_gpu(2);
    assert!((s.scene_render_scale() - 0.75).abs() < 0.0001);
    s.render_scale = 0.5;
    assert!((s.render_options().render_scale - 0.5).abs() < 0.0001);
    // (a cap above the preset leaves it as it is; auto stays with the frame-rate governor)
    s.render_scale = 1.0;
    assert!((s.scene_render_scale() - 0.77).abs() < 0.0001);
    s.fsr_quality = "auto".into();
    assert_eq!(s.scene_render_scale(), 0.0);
}

#[test]
fn mobile_door_mode_defaults_and_round_trips() {
    assert_eq!(Settings::from_text("").touch_door_mode, "standard");
    assert_eq!(Settings::from_text("touch_door_mode=Direct\n").touch_door_mode, "direct");
    assert_eq!(Settings::from_text("touch_door_mode=unknown\n").touch_door_mode, "standard");
    let selected = Settings { touch_door_mode: "direct".into(), ..Default::default() };
    assert_eq!(Settings::from_text(&selected.to_text()).touch_door_mode, "direct");
    let mut launcher = omsi_launcher_lib::settings_from_text(None);
    assert_eq!(launcher["touch_door_mode"], "standard");
    launcher["touch_door_mode"] = serde_json::json!("direct");
    let saved = omsi_launcher_lib::settings_to_text(&launcher, None);
    assert_eq!(Settings::from_text(&saved).touch_door_mode, "direct");
    assert_eq!(omsi_launcher_lib::settings_from_text(Some(&saved))["touch_door_mode"], "direct");
}

/// DXT/BC on the GPU: on unless the file says off; the launcher and the game start
/// from the same defaults for the graphics settings.
#[test]
fn gpu_texture_compression_and_the_launchers_defaults() {
    assert!(Settings::default().gpu_texture_compression);
    let off = Settings { gpu_texture_compression: false, ..Default::default() };
    assert_eq!(Settings::from_text(&off.to_text()), off);
    assert!(!Settings::from_text("gpu_texture_compression=disabled\n").gpu_texture_compression);
    assert!(Settings::from_text("gpu_texture_compression=auto\n").gpu_texture_compression);
    assert!(!off.render_options().gpu_texture_compression);
    let launcher = omsi_launcher_lib::settings_from_text(None);
    let game = Settings::from_text(&omsi_launcher_lib::settings_to_text(&launcher, None));
    let d = Settings::default();
    assert_eq!((game.msaa, game.anisotropy, game.ssao, game.shadow_size, game.video_memory), (d.msaa, d.anisotropy, d.ssao, d.shadow_size, d.video_memory));
    assert_eq!((&game.mirror_refresh, game.gpu_texture_compression, &game.cloud_quality), (&d.mirror_refresh, d.gpu_texture_compression, &d.cloud_quality));
}

/// The gamepad's settings are kept and held in their ranges (a type it does not know
/// is `auto`).
#[test]
fn gamepad_settings_round_trip_and_are_clamped() {
    let d = Settings::default();
    assert_eq!((d.pad_steer_speed, d.pad_deadzone, d.pad_type.as_str(), d.pad_buttons), (2.0, 0.08, "auto", true));
    let s = Settings::from_text("pad_steer_speed=3.5\npad_deadzone=0.15\npad_type=PS5\npad_buttons=0\n");
    let back = Settings::from_text(&s.to_text());
    assert_eq!((back.pad_steer_speed, back.pad_deadzone, back.pad_type.as_str(), back.pad_buttons), (3.5, 0.15, "ps5", false));
    let wild = Settings::from_text("pad_steer_speed=0\npad_deadzone=9\npad_type=switch\n");
    assert_eq!((wild.pad_steer_speed, wild.pad_deadzone, wild.pad_type.as_str()), (0.8, 0.4, "auto"));
    assert_eq!(Settings::from_text("pad_steer_speed=99\n").pad_steer_speed, 5.0);
}

/// The enhanced clouds' quality is kept, `high` unless the file says `low`.
#[test]
fn street_light_round_trips_and_reaches_the_renderer() {
    assert_eq!(Settings::default().street_light, "off");
    assert_eq!(Settings::default().render_options().street_light, Some(false));
    for phone in [false, true] {
        let launcher = omsi_launcher_lib::settings_from_text_on(None, phone);
        assert_eq!(launcher["street_light"], "off");
        let game = Settings::from_text(&omsi_launcher_lib::settings_to_text(&launcher, None));
        assert_eq!(game.render_options().street_light, Some(false));
    }
    for (q, want) in [("auto", None), ("on", Some(true)), ("off", Some(false))] {
        let s = Settings::from_text(&format!("graphics=enhanced\nstreet_light={q}\n"));
        assert_eq!(Settings::from_text(&s.to_text()).street_light, q);
        assert_eq!(s.render_options().street_light, want);
    }
    assert_eq!(Settings::from_text("street_light=OFF\n").street_light, "off");
    assert_eq!(Settings::from_text("street_light=broken\n").street_light, "auto");
}

#[test]
fn cloud_quality_round_trips() {
    assert_eq!(Settings::default().cloud_quality, "high");
    for q in ["high", "low"] {
        let s = Settings::from_text(&format!("graphics=enhanced\ncloud_quality={q}\n"));
        assert_eq!(Settings::from_text(&s.to_text()).cloud_quality, q);
    }
    assert_eq!(Settings::from_text("cloud_quality=auto\n").cloud_quality, "high");
    assert_eq!(Settings::from_text("cloud_quality=LOW\n").cloud_quality, "low");
    assert_eq!(Settings::from_text("cloud_quality=broken\n").cloud_quality, "high");
    assert_eq!(Settings::default().rain_quality, "high");
    for q in ["high", "medium", "low"] {
        let s = Settings::from_text(&format!("rain_quality={q}\n"));
        assert_eq!(Settings::from_text(&s.to_text()).rain_quality, q);
    }
    assert_eq!(Settings::from_text("rain_quality=ultra\n").rain_quality, "high");
}

/// The information bar is as the last session left it (#1164); off at first.
#[test]
fn the_information_bar_is_kept_between_sessions() {
    assert!(!super::Settings::default().info_bar);
    let s = super::Settings::from_text("ambient=0\nvol_ambient=0.35\n");
    assert!(!s.ambient && (s.vol_ambient - 0.35).abs() < 1e-6);
    assert!(super::Settings::default().ambient);
    let again = super::Settings::from_text(&s.to_text());
    assert!(!again.ambient && (again.vol_ambient - 0.35).abs() < 1e-6, "kept through a save");
    assert!(super::Settings::from_text("info_bar=1\n").info_bar);
    assert!(!super::Settings::from_text("info_bar=0\n").info_bar);
    assert_eq!(super::Settings::default().perf_overlay, "off");
    assert_eq!(super::Settings::from_text(&super::Settings { perf_overlay: "detailed".into(), ..Default::default() }.to_text()).perf_overlay, "detailed");
    assert_eq!(super::Settings::from_text("perf_overlay=compact\n").perf_overlay, "compact");
}

/// The radio's effects are off until asked for, and kept through a save.
#[test]
fn the_radio_effects_are_off_at_first_and_kept() {
    assert!(super::Settings::default().radio_fx.is_off());
    let s = super::Settings::from_text("radio_speaker=0.6\nradio_hiss=2\nradio_dropouts=x\n");
    assert_eq!((s.radio_fx.speaker, s.radio_fx.hiss, s.radio_fx.dropouts), (0.6, 1.0, 0.0));
    let again = super::Settings::from_text(&s.to_text());
    assert_eq!(again.radio_fx, s.radio_fx);
}

#[test]
fn mouse_pedal_strength_is_saved_and_clamped() {
    let settings = super::Settings::from_text("mouse_pedal_strength=1.5\n");
    assert_eq!(settings.mouse_pedal_strength, 1.5);
    assert_eq!(
        super::Settings::from_text(&settings.to_text()).mouse_pedal_strength,
        1.5
    );
    assert_eq!(
        super::Settings::from_text("mouse_pedal_strength=NaN\n").mouse_pedal_strength,
        1.0
    );
    assert_eq!(
        super::Settings::from_text("mouse_pedal_strength=9\n").mouse_pedal_strength,
        4.0
    );
}

#[test]
fn triple_hud_defaults_to_the_centre_panel_and_can_span() {
    let mut s = super::Settings::from_text("triple_screen=1\n");
    assert_eq!(s.hud_viewport((5760, 1080)), [1920.0, 0.0, 1920.0, 1080.0]);
    assert_eq!(s.hud_viewport((5761, 1080)), [1921.0, 0.0, 1921.0, 1080.0]);
    s.triple_hud_center = false;
    assert_eq!(s.hud_viewport((5761, 1080)), [0.0, 0.0, 5761.0, 1080.0]);
    let s = super::Settings::from_text("triple_screen=0\n");
    assert_eq!(s.hud_viewport((1920, 1080)), [0.0, 0.0, 1920.0, 1080.0]);
}

#[test]
fn triple_fov_and_hud_preferences_round_trip() {
    let s = super::Settings::from_text(
        "triple_screen=1\ntriple_hud_center=0\ntriple_fov_deg=75\nfov=50\n",
    );
    assert_eq!(super::Settings::from_text(&s.to_text()), s);
    assert_eq!(s.triple.fov_deg, 75.0);
    assert_eq!(s.fov, 50.0);
    assert_eq!(
        super::Settings::from_text("triple_fov_deg=NaN\n")
            .triple
            .fov_deg,
        0.0
    );
}
#[test]
fn triple_settings_round_trip_and_reject_nonfinite_geometry() {
    let s = super::Settings::from_text("triple_screen=1\ntriple_span=0\ntriple_width_mm=620\ntriple_distance_mm=700\ntriple_bezel_mm=18\ntriple_left_angle_deg=50\ntriple_right_angle_deg=40\ntriple_eye_height_mm=60\n");
    assert_eq!(super::Settings::from_text(&s.to_text()), s);
    assert!(s.triple.enabled);
    assert!(!s.triple_span);
    let invalid = super::Settings::from_text("triple_width_mm=NaN\ntriple_distance_mm=inf\ntriple_left_angle_deg=-30\ntriple_bezel_mm=-20\n");
    assert_eq!(invalid.triple.width_mm, 600.0);
    assert_eq!(invalid.triple.distance_mm, 650.0);
    assert_eq!(invalid.triple.left_angle_deg, 0.0);
    assert_eq!(invalid.triple.bezel_mm, 0.0);
}
use super::*;

#[test]
fn old_files_board_automatically() {
    // the launcher's old default, written without a version
    assert_eq!(Settings::from_text("msaa=1\nboarding=pay\n").boarding, "auto");
    // chosen again in a current file, it stays
    assert_eq!(Settings::from_text("version=2\nboarding=pay\n").boarding, "pay");
    assert_eq!(Settings::from_text("boarding=walk\n").boarding, "walk");
    // what we write reads back the same
    let s = Settings { boarding: "pay".into(), ..Default::default() };
    assert_eq!(Settings::from_text(&s.to_text()), s);
    let s = Settings { texture_compression: false, video_memory: 1500, ..Default::default() };
    assert_eq!(Settings::from_text(&s.to_text()), s);
    // (texture budgets of before: not taken for the whole card's)
    assert_eq!(Settings::from_text("texmemlimit=401.0\ntexture_memory=1000\n").video_memory, 0);
}

#[test]
fn standing_chance_is_a_percentage() {
    assert_eq!(Settings::from_text("standing_chance=25\n").standing_chance, 0.25);
    assert_eq!(Settings::from_text("standing_chance=250\n").standing_chance, 1.0);
    let s = Settings { standing_chance: 0.1, ..Default::default() };
    assert_eq!(Settings::from_text(&s.to_text()).standing_chance, s.standing_chance);
}

#[test]
fn photo_mode_setting_round_trip() {
    assert_eq!(Settings::default().photo_mode, "simple");
    let s = Settings::from_text("photo_mode=advanced\n");
    assert_eq!(s.photo_mode, "advanced");
    assert_eq!(Settings::from_text(&s.to_text()).photo_mode, "advanced");
    assert_eq!(Settings::from_text("photo_mode=pro\n").photo_mode, "simple");
}

#[test]
fn character_models_setting_round_trip() {
    assert_eq!(Settings::default().character_models, "original");
    let s = Settings::from_text("character_models=improved\n");
    assert_eq!(s.character_models, "improved");
    assert!(s.to_text().contains("character_models=improved"));
    assert_eq!(Settings::from_text("character_models=whatever\n").character_models, "original");
}

#[test]
fn passenger_animation_setting_round_trip() {
    assert_eq!(Settings::default().passenger_animation, "original");
    assert_eq!(Settings::from_text("version=2\npassenger_animation=original\n").passenger_animation, "original");
    assert_eq!(Settings::from_text("version=2\npassenger_animation=enhanced\n").passenger_animation, "enhanced");
    assert_eq!(Settings::from_text("version=3\npassenger_animation=enhanced\n").passenger_animation, "original");
    assert_eq!(Settings::from_text("version=4\npassenger_animation=enhanced\n").passenger_animation, "enhanced");
    let s = Settings::from_text("version=4\npassenger_animation=enhanced\n");
    assert_eq!(Settings::from_text(&s.to_text()), s);
    assert_eq!(Settings::from_text("version=4\npassenger_animation=invalid\n").passenger_animation, "original");
}

#[test]
fn graphics_modes() {
    // an old file: its vanilla renderer is Vanilla+ now, enhanced stays enhanced
    assert_eq!(Settings::from_text("enhanced=0\n").graphics, "vanilla_plus");
    assert_eq!(Settings::from_text("enhanced=1\n").graphics, "enhanced");
    let v = Settings::from_text("graphics=vanilla\nshadows=1\nssao=1\n");
    assert!(v.classic() && !v.shadows && !v.ssao && !v.detail_textures && !v.enhanced);
    assert!(Settings::from_text("graphics=enhanced\nenhanced=0\n").enhanced);
    assert_eq!(graphics_mode("Vanilla+"), "vanilla_plus");
    assert_eq!(graphics_mode("OMSI 2"), "vanilla");
    assert_eq!(graphics_mode("Enhanced+"), "enhanced_plus");
    let rt = Settings::from_text("graphics=enhanced_plus\nshadows=0\nssao=0\nreflections=0\n");
    assert!(rt.enhanced && rt.ray_tracing() && rt.shadows && rt.ssao && rt.reflections == 1.0);
    assert_eq!(["1", "0", "true", "off", "0.35", "7", "x"].map(|v| Settings::from_text(&format!("reflections={v}\n")).reflections), [1.0, 0.0, 1.0, 0.0, 0.35, 1.0, 0.0]);
    assert!(rt.render_options().ray_tracing && !rt.render_options().no_enhanced);
    let switched = crate::startup::SessionSettings { enhanced: true, ..Default::default() };
    assert!(Settings::default().render_options().no_enhanced && !switched.render_options(&Settings::default()).no_enhanced);
    let s = Settings { graphics: "enhanced".into(), enhanced: true, ..Default::default() };
    assert_eq!(Settings::from_text(&s.to_text()), s);
}

/// Ray tracing is a switch of Enhanced, not a mode of its own; the lights' shadows, the clouds' and
/// the glow are Enhanced's options, kept and read back.
#[test]
fn enhanced_options_and_ray_tracing() {
    let on = Settings::from_text("graphics=enhanced\nray_tracing=1\nlight_shadows=all\ncloud_shadows=0\nlight_glow=0\n");
    assert!(on.ray_tracing() && on.enhanced && on.graphics == "enhanced" && on.mode() == "enhanced_plus");
    let o = on.render_options();
    assert_eq!((o.ray_tracing, o.light_shadows, o.cloud_shadows, o.light_glow), (true, 2, false, false));
    let text = on.to_text();
    assert!(text.contains("graphics=enhanced\n") && text.contains("ray_tracing=1\n") && text.contains("light_shadows=all\n"), "{text}");
    assert_eq!(Settings::from_text(&text), on);
    // without Enhanced it traces nothing, and the defaults keep the street lamps' shadows
    let plain = Settings::from_text("graphics=vanilla_plus\nray_tracing=1\n");
    assert!(!plain.ray_tracing() && !plain.render_options().ray_tracing && plain.mode() == "vanilla_plus");
    let d = Settings::default().render_options();
    assert_eq!((d.light_shadows, d.cloud_shadows, d.light_glow, d.ray_tracing), (1, false, false, false));
    assert_eq!(["off", "street", "all", "every", "x", "0"].map(|v| Settings::from_text(&format!("light_shadows={v}\n")).light_shadows), ["off", "street", "all", "all", "street", "off"]);
    // a file of the days of Enhanced+ is written back as Enhanced with ray tracing
    let old = Settings::from_text("graphics=enhanced_plus\n");
    assert!(old.to_text().contains("ray_tracing=1\n") && !old.to_text().contains("enhanced_plus"));
}

#[test]
fn mirror_size_zero_disables_mirror_rendering() {
    assert_eq!(Settings::from_text("mirror_size=0\n").mirror_size, 0);
    assert_eq!(Settings::from_text("mirror_size=32\n").mirror_size, 64);
    let off = Settings { mirror_size: 0, ..Default::default() };
    assert_eq!(Settings::from_text(&off.to_text()).mirror_size, 0);
}

#[test]
fn mirror_refresh_reads_omsi_realreflexions_and_round_trips() {
    assert_eq!(Settings::default().mirror_refresh, "full");
    assert_eq!(Settings::from_text("performance_realreflexions=none\n").mirror_refresh, "off");
    assert_eq!(Settings::from_text("performance_realreflexions=economy\n").mirror_refresh, "eco");
    assert_eq!(Settings::from_text("performance_realreflexions=full\n").mirror_refresh, "full");
    assert_eq!(Settings::from_text("mirror_refresh=nonsense\n").mirror_refresh, "full");
    for mode in ["off", "eco", "full", "every"] {
        let s = Settings { mirror_refresh: mode.into(), ..Default::default() };
        assert_eq!(Settings::from_text(&s.to_text()).mirror_refresh, mode);
    }
}

#[test]
fn mirror_hud_every_frame_is_off_by_default_and_round_trips() {
    assert!(!Settings::default().mirror_hud_every_frame);
    assert!(Settings::from_text("mirror_hud_every_frame=1\n").mirror_hud_every_frame);
    for on in [false, true] {
        let s = Settings { mirror_hud_every_frame: on, ..Default::default() };
        assert_eq!(Settings::from_text(&s.to_text()).mirror_hud_every_frame, on);
    }
}

#[test]
fn steering_view_settings_round_trip_and_reject_invalid_values() {
    let settings = Settings { steer_look: true, steer_look_angle: 45.0, steer_look_response: 0.5, ..Default::default() };
    assert_eq!(Settings::from_text(&settings.to_text()), settings);
    let invalid = Settings::from_text("steer_look_angle=NaN\nsteer_look_response=NaN\n");
    assert_eq!(invalid.steer_look_angle, 30.0);
    assert_eq!(invalid.steer_look_response, 0.25);
    let clamped = Settings::from_text("steer_look_angle=999\nsteer_look_response=-1\n");
    assert_eq!(clamped.steer_look_angle, 60.0);
    assert_eq!(clamped.steer_look_response, 0.05);
}

#[test]
fn seat_pitch_round_trips_and_is_clamped() {
    let settings = Settings { seat_pitch_deg: -12.0, ..Default::default() };
    assert_eq!(Settings::from_text(&settings.to_text()), settings);
    assert_eq!(Settings::from_text("seat_pitch_deg=NaN\n").seat_pitch_deg, 0.0);
    assert_eq!(Settings::from_text("seat_pitch_deg=90\n").seat_pitch_deg, 45.0);
}

#[test]
fn vr_mirror_rates_round_trip_including_every_frame() {
    for rate in [-1.0, 0.0, 16.0, 60.0, 120.0, 240.0, 360.0] {
        let s = Settings { vr_mirror_rate: rate, ..Default::default() };
        assert_eq!(Settings::from_text(&s.to_text()).vr_mirror_rate, rate);
    }
    assert_eq!(Settings::from_text("vr_mirror_rate=NaN\n").vr_mirror_rate, 16.0);
    assert_eq!(Settings::from_text("vr_mirror_rate=999\n").vr_mirror_rate, 360.0);
}

#[test]
fn interface_size_is_read_within_its_range() {
    assert_eq!(Settings::default().ui_scale, 1.0);
    assert_eq!(Settings::from_text("ui_scale=1.25\n").ui_scale, 1.25);
    assert_eq!(Settings::from_text("ui_scale=5\n").ui_scale, 2.0);
    assert_eq!(Settings::from_text("ui_scale=0\n").ui_scale, 0.5);
    assert_eq!(Settings::from_text("ui_scale=NaN\n").ui_scale, 1.0);
    let s = Settings { ui_scale: 1.5, ui_scale_window: false, ui_opacity: 0.4, notes: false, ..Default::default() };
    assert_eq!(Settings::from_text(&s.to_text()), s);
    // (the opacity under its old name, the navigator's)
    assert_eq!(Settings::from_text("navigator_opacity=0.5\n").ui_opacity, 0.5);
    assert_eq!(Settings::from_text("ui_opacity=7\n").ui_opacity, 1.0);
}

#[test]
fn discord_settings_round_trip_and_default_enabled() {
    assert!(Settings::default().discord_status);
    let settings = Settings { discord_status: false, discord_app_id: "123456".into(), ..Default::default() };
    assert_eq!(Settings::from_text(&settings.to_text()), settings);
}

#[test]
fn voice_chat_is_off_by_default_and_round_trips() {
    let d = Settings::default();
    assert!(!d.voice_enabled && !d.voice_all_off && d.voice_mode == "ptt" && d.voice_range == 75.0);
    // an old file with the earlier switch does not turn it on
    assert!(!Settings::from_text("voice_chat=1\n").voice_enabled);
    let s = Settings {
        voice_enabled: true,
        voice_mode: "open".into(),
        voice_mic_gain: 1.5,
        voice_threshold_db: -50.0,
        voice_input_device: "USB Mic".into(),
        voice_volume: 0.8,
        voice_volume_nearby: 1.2,
        voice_volume_radio: 0.5,
        voice_range: 120.0,
        voice_talker_list: false,
        voice_all_off: true,
        ..Default::default()
    };
    assert_eq!(Settings::from_text(&s.to_text()), s);
    // wrong values are cut to what the voice chat takes
    let w = Settings::from_text("voice_mic_gain=99\nvoice_range=1\nvoice_mode=shout\nvoice_volume=nan\nvoice_threshold_db=0\n");
    assert_eq!((w.voice_mic_gain, w.voice_range, w.voice_mode.as_str(), w.voice_volume, w.voice_threshold_db), (4.0, 5.0, "ptt", 1.0, -10.0));
}

#[test]
fn post_aa_is_read_and_written() {
    assert_eq!(Settings::from_text("enhanced=1\n").post_aa, "fxaa");
    let off = Settings::from_text("enhanced=1\npost_aa=off\n");
    assert_eq!(off.post_aa, "off");
    assert!(!off.render_options().fxaa);
    assert!(Settings::from_text("post_aa=FXAA").render_options().fxaa);
    assert_eq!(Settings::from_text(&off.to_text()), off);
}

#[test]
fn the_wheels_tremble_and_its_fade_out_survive_a_save() {
    let d = Settings::default();
    assert_eq!((d.ff_road_vib, d.ff_engine_vib, d.ff_fade), (1.0, 1.0, 0.28));
    let s = Settings { ff_road_vib: 1.5, ff_engine_vib: 0.0, ff_fade: 0.5, ..Default::default() };
    assert_eq!(Settings::from_text(&s.to_text()), s);
    // what the file may hold is held between none and twice as much, and a fade of
    // no more than a second - a number outside is brought back in rather than lost
    let wild = Settings::from_text("ff_road_vib=9\nff_engine_vib=-3\nff_fade=12\n");
    assert_eq!((wild.ff_road_vib, wild.ff_engine_vib, wild.ff_fade), (4.0, 0.0, 1.5));
    // a file that says nothing keeps the wheel feeling as it did out of the box
    let old = Settings::from_text("msaa=1\n");
    assert_eq!((old.ff_road_vib, old.ff_engine_vib, old.ff_fade), (1.0, 1.0, 0.28));
    // and one that says nonsense is left as it was
    let bad = Settings::from_text("ff_road_vib=loud\nff_fade=\n");
    assert_eq!((bad.ff_road_vib, bad.ff_fade), (1.0, 0.28));
}

/// The wet roads' reflections: high in a file that has none (as they were), the three
/// qualities kept through a save, a word it does not know read as high.
#[test]
fn puddles_round_trip() {
    assert_eq!(Settings::default().puddles, "high");
    assert_eq!(Settings::from_text("version=4\n").puddles, "high");
    for q in ["off", "low", "high"] {
        let s = Settings::from_text(&format!("puddles={q}\n"));
        assert_eq!(Settings::from_text(&s.to_text()).puddles, q);
    }
    assert_eq!(Settings::from_text("puddles=ultra\n").puddles, "high");
}

#[test]
fn vehicle_smoke_setting_round_trips_through_game_and_launcher() {
    assert_eq!(Settings::default().vehicle_smoke, "all");
    for mode in ["all", "player", "off"] {
        let s = Settings::from_text(&format!("vehicle_smoke={mode}\n"));
        assert_eq!(s.vehicle_smoke, mode);
        assert_eq!(Settings::from_text(&s.to_text()).vehicle_smoke, mode);
        let mut launcher = omsi_launcher_lib::settings_from_text(None);
        launcher["vehicle_smoke"] = serde_json::json!(mode);
        let saved = omsi_launcher_lib::settings_to_text(&launcher, None);
        assert_eq!(Settings::from_text(&saved).vehicle_smoke, mode);
        assert_eq!(omsi_launcher_lib::settings_from_text(Some(&saved))["vehicle_smoke"], mode);
    }
    assert_eq!(Settings::from_text("vehicle_smoke=unknown\n").vehicle_smoke, "all");
    assert_eq!(Settings::from_text("vehicle_smoke=PLAYER_ONLY\n").vehicle_smoke, "player");
}


/// The arrival announcements of the maps that keep two recordings per stop: off out of the
/// box, and kept through the launcher's writer (the game's Gameplay page turns it on).
#[test]
fn arrival_announcements_are_off_at_first_and_kept() {
    assert!(!Settings::default().auto_announce);
    assert!(!Settings::from_text("version=4\n").auto_announce);
    assert!(Settings::from_text("auto_announce=1\n").auto_announce);
    let mut launcher = omsi_launcher_lib::settings_from_text(None);
    assert_eq!(launcher["auto_announce"], false);
    launcher["auto_announce"] = serde_json::json!(true);
    let saved = omsi_launcher_lib::settings_to_text(&launcher, None);
    assert!(Settings::from_text(&saved).auto_announce, "{saved}");
    let on = Settings { auto_announce: true, ..Default::default() };
    assert_eq!(Settings::from_text(&on.to_text()), on);
}
