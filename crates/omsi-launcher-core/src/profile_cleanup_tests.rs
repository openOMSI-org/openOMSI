use super::*;

/// A quality preset sets the graphics mode with the rest of what it holds and leaves what is not
/// graphics alone; Medium is Vanilla+ as it starts, Ultra is Enhanced with ray tracing.
#[test]
fn quality_presets_set_the_mode_and_keep_the_rest() {
    for start in ["vanilla", "vanilla_plus", "enhanced", "enhanced_plus"] {
        for (_, preset) in graphics_presets() {
            let mut v = settings_from_text(Some(&format!("graphics={start}\ngraphics_api=gl\nlanguage=PTB\nai_unsched_factor=25\n")));
            apply_graphics_profile(&preset, &mut v);
            let saved = settings_from_text(Some(&settings_to_text(&v, None)));
            assert_eq!(saved["graphics_api"], "gl");
            assert_eq!(saved["language"], "PTB");
            assert_eq!(saved["ai_unsched_factor"], 25);
            for (key, x) in preset.as_object().unwrap() {
                assert_eq!(saved[key.as_str()].to_string().trim_matches('"'), x.to_string().trim_matches('"'), "{start}: {key}");
            }
        }
    }
    let modes: Vec<&str> = graphics_presets().iter().map(|(_, p)| effective_mode(p)).collect();
    assert_eq!(modes, ["vanilla", "vanilla_plus", "enhanced", "enhanced", "enhanced", "enhanced_plus"]);
    let names: Vec<&str> = graphics_presets().iter().map(|(n, _)| *n).collect();
    assert_eq!(names, ["Low", "Medium", "High", "Extra", "Max", "Ultra"]);
    // Medium is what the game starts with
    let defaults = settings_from_text(None);
    let medium = &graphics_presets()[1].1;
    for (k, x) in medium.as_object().unwrap() {
        assert_eq!(defaults[k.as_str()].to_string().trim_matches('"'), x.to_string().trim_matches('"'), "{k}");
    }
}

/// Each step up is at least as much as the one below it: the lights' shadows, the street light and
/// the ray tracing only ever switch on as the level rises, and Ultra has them all.
#[test]
fn the_presets_rise_in_steps() {
    let rank = |s: &str, order: &[&str]| order.iter().position(|o| *o == s).unwrap();
    let p = graphics_presets();
    for w in p.windows(2) {
        let (a, b) = (&w[0].1, &w[1].1);
        assert!(rank(b["light_shadows"].as_str().unwrap(), &["off", "street", "all"]) >= rank(a["light_shadows"].as_str().unwrap(), &["off", "street", "all"]));
        assert!(b["shadow_size"].as_i64() >= a["shadow_size"].as_i64());
        assert!(b["anisotropy"].as_i64() >= a["anisotropy"].as_i64());
        assert!(b["ray_tracing"].as_bool() >= a["ray_tracing"].as_bool());
    }
    let ultra = &p[5].1;
    assert_eq!((&ultra["ray_tracing"], &ultra["light_shadows"], &ultra["cloud_shadows"], &ultra["light_glow"], &ultra["puddles"], &ultra["street_light"]), (&json!(true), &json!("all"), &json!(true), &json!(true), &json!("high"), &json!("on")));
    for (name, preset) in &p {
        assert!(preset.get("video_memory").is_none(), "{name}");
        assert!(preset.get("cloud_quality").is_none(), "{name}");
        assert_eq!(preset["clouds"], true, "{name}");
        assert_eq!(preset["reflections"], 1.0, "{name}");
    }
}

/// Low is Vanilla, which draws no ambient occlusion, sun shadows or detail grain: it leaves them as
/// they are, so that a player who picks Low and goes back to Vanilla+ finds them on, and what is not
/// graphics (the video memory) stays as it was.
#[test]
fn low_then_back_to_vanilla_plus_keeps_the_shadows() {
    let low = &graphics_presets()[0].1;
    for k in ["ssao", "shadows", "detail_textures"] {
        assert!(low.get(k).is_none(), "{k}");
    }
    let mut v = settings_from_text(Some("graphics=vanilla_plus\nvideo_memory=3000\n"));
    apply_graphics_profile(low, &mut v);
    assert_eq!(effective_mode(&v), "vanilla");
    let saved = settings_from_text(Some(&settings_to_text(&v, None)));
    assert_eq!((&saved["shadows"], &saved["ssao"], &saved["detail_textures"], &saved["video_memory"]), (&json!(true), &json!(true), &json!(true), &json!(3000)));
    let mut back = saved.clone();
    apply_graphics_profile(&graphics_presets()[1].1, &mut back);
    assert_eq!(effective_mode(&back), "vanilla_plus");
    assert_eq!((&back["shadows"], &back["ssao"]), (&json!(true), &json!(true)));
    // the options that cost most are off in the defaults and in every preset below Extra
    let defaults = settings_from_text(None);
    assert_eq!((&defaults["light_glow"], &defaults["cloud_shadows"], &defaults["light_shadows"]), (&json!(false), &json!(false), &json!("street")));
    for (name, p) in &graphics_presets()[..3] {
        assert_eq!((&p["light_glow"], &p["cloud_shadows"]), (&json!(false), &json!(false)), "{name}");
    }
}

/// A file of the days of Enhanced+ (`graphics=enhanced_plus`) reads as Enhanced with ray tracing
/// on, and is written that way.
#[test]
fn enhanced_plus_is_enhanced_with_ray_tracing() {
    let v = settings_from_text(Some("graphics=enhanced_plus\n"));
    assert_eq!((&v["graphics"], &v["ray_tracing"], &v["enhanced"]), (&json!("enhanced"), &json!(true), &json!(true)));
    assert_eq!(effective_mode(&v), "enhanced_plus");
    let text = settings_to_text(&v, None);
    assert!(text.contains("graphics=enhanced\n") && text.contains("ray_tracing=1\n"), "{text}");
    let v = settings_from_text(Some("graphics=enhanced\nray_tracing=0\n"));
    assert_eq!(effective_mode(&v), "enhanced");
    // ray tracing without Enhanced draws nothing traced
    let v = settings_from_text(Some("graphics=vanilla_plus\nray_tracing=1\n"));
    assert_eq!(effective_mode(&v), "vanilla_plus");
}

#[test]
fn a_phone_starts_from_the_games_phone_defaults() {
    let phone = settings_from_text_on(None, true);
    let saved = settings_from_text_on(Some(&settings_to_text(&phone, None)), false);
    for (k, x) in [("msaa", json!(2)), ("anisotropy", json!(4)), ("ssao", json!(false)), ("shadow_size", json!(1024)), ("mirror_size", json!(128)), ("max_fps", json!(60)), ("max_obj_dist", json!("900")), ("pax_density", json!(0.7)), ("navigator_corner", json!("top-center"))] {
        assert_eq!(phone[k], x, "{k}");
        assert_eq!(saved[k], x, "{k}");
    }
    let desktop = settings_from_text_on(None, false);
    assert_eq!((&desktop["msaa"], &desktop["max_obj_dist"], &desktop["navigator_corner"]), (&json!(4), &json!("auto"), &json!("bottom-left")));
    let file = settings_from_text_on(Some("msaa=8\nmax_obj_dist=1500\n"), true);
    assert_eq!((&file["msaa"], &file["max_obj_dist"], &file["anisotropy"]), (&json!(8), &json!("1500"), &json!(4)));
    for (k, d) in desktop.as_object().unwrap() {
        if !["msaa", "anisotropy", "ssao", "shadow_size", "mirror_size", "max_fps", "max_obj_dist", "pax_density", "navigator_corner"].contains(&k.as_str()) {
            assert_eq!(&phone[k.as_str()], d, "{k}");
        }
    }
}

#[test]
fn a_graphics_profile_holds_the_whole_graphics_tab() {
    for k in ["shadow_blobs", "night_brightness", "gpu_texture_compression"] {
        assert!(GRAPHICS_PROFILE_KEYS.contains(&k), "{k}");
    }
    let mut mine = settings_from_text(None);
    for (k, x) in [("shadow_blobs", json!(false)), ("night_brightness", json!(1.5)), ("gpu_texture_compression", json!(false)), ("graphics", json!("enhanced")), ("language", json!("PTB"))] {
        mine[k] = x;
    }
    let mut profile = serde_json::Map::new();
    for k in GRAPHICS_PROFILE_KEYS {
        profile.insert(k.to_string(), mine[k].clone());
    }
    let mut other = settings_from_text(None);
    apply_graphics_profile(&Value::Object(profile), &mut other);
    let saved = settings_from_text(Some(&settings_to_text(&other, None)));
    assert_eq!((&saved["shadow_blobs"], &saved["night_brightness"], &saved["gpu_texture_compression"], &saved["graphics"]), (&json!(false), &json!(1.5), &json!(false), &json!("enhanced")));
    assert_eq!(saved["language"], "ENG");
}

/// The wet roads' reflections are saved by the launcher, kept in graphics profiles, and
/// high in a file that has none (as they were).
#[test]
fn puddles_are_saved_and_kept_in_graphics_profiles() {
    assert_eq!(settings_from_text(None)["puddles"], "high");
    for q in ["off", "low", "high"] {
        let v = settings_from_text(Some(&format!("puddles={q}\n")));
        assert_eq!(settings_from_text(Some(&settings_to_text(&v, None)))["puddles"], q);
    }
    assert_eq!(settings_from_text(Some("puddles=ultra"))["puddles"], "high");
    assert!(GRAPHICS_PROFILE_KEYS.contains(&"puddles"));
}

/// The enhanced clouds' quality is saved by the launcher and kept in graphics profiles.
#[test]
fn cloud_quality_is_saved_and_kept_in_graphics_profiles() {
    assert_eq!(settings_from_text(None)["cloud_quality"], "high");
    for q in ["high", "low"] {
        let v = settings_from_text(Some(&format!("graphics=enhanced\ncloud_quality={q}\n")));
        assert_eq!(settings_from_text(Some(&settings_to_text(&v, None)))["cloud_quality"], q);
    }
    assert_eq!(settings_from_text(Some("cloud_quality=auto"))["cloud_quality"], "high");
    assert!(GRAPHICS_PROFILE_KEYS.contains(&"cloud_quality"));
}

#[test]
fn the_day_before_crosses_months_and_years() {
    assert_eq!(day_before(20261007), 20261006);
    assert_eq!(day_before(20261001), 20260930);
    assert_eq!(day_before(20240301), 20240229);
    assert_eq!(day_before(20260101), 20251231);
}

#[test]
fn deleting_one_driver_keeps_other_and_unowned_runs() {
    let dir = std::env::temp_dir().join(format!("omsi_profile_cleanup_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let write = |file: &str, driver: &str| {
        let session = Session {
            driver: driver.into(),
            ..Default::default()
        };
        std::fs::write(dir.join(file), serde_json::to_vec(&session).unwrap()).unwrap();
    };
    write("mine.json", "Isaac");
    write("mine-case.json", "ISAAC");
    write("other.json", "Isaac S.");
    std::fs::write(dir.join("unknown.json"), "{broken").unwrap();
    std::fs::create_dir_all(dir.join("folder.json")).unwrap();
    delete_profile_sessions(&dir, "isaac").unwrap();
    assert!(!dir.join("mine.json").exists());
    assert!(!dir.join("mine-case.json").exists());
    assert!(dir.join("other.json").exists());
    assert!(dir.join("unknown.json").exists());
    assert!(dir.join("folder.json").is_dir());
    // Repeating deletion, and a profile with no history directory, are harmless.
    delete_profile_sessions(&dir, "Isaac").unwrap();
    delete_profile_sessions(&dir.join("absent"), "Isaac").unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_delete_request_needs_a_plain_profile_name() {
    for name in ["", " ", "../other", "a\\b", "C:other"] {
        assert!(delete_profile(name).is_err());
    }
}

#[test]
fn the_reflections_are_a_strength_and_older_files_read_as_on_or_off() {
    for (text, x) in [("reflections=1\n", 1.0), ("reflections=0\n", 0.0), ("reflections=true\n", 1.0), ("reflections=0.35\n", 0.35), ("reflections=7\n", 1.0)] {
        let v = settings_from_text(Some(text));
        assert_eq!(v["reflections"], x, "{text}");
        assert_eq!(settings_from_text(Some(&settings_to_text(&v, None)))["reflections"], x, "{text}");
    }
    let mut v = settings_from_text(None);
    v["reflections"] = json!(false);
    assert_eq!(settings_from_text(Some(&settings_to_text(&v, None)))["reflections"], 0.0);
}
