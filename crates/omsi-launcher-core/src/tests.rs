include!("../../../tools/test-support/original_root.rs");
#[test]
fn vehicle_type_label_falls_back_to_the_file_name() {
    let path = std::path::Path::new("Vehicles/Pack/NL_202.bus");
    for empty in ["", "   "] {
        assert_eq!(super::vehicle_type_label(empty, path), "NL 202");
    }
    assert_eq!(super::vehicle_type_label("  MAN_NL202  ", path), "MAN NL202");
}

#[test]
fn a_part_found_from_the_vehicle_folder_is_no_missing_pack() {
    let root = std::env::temp_dir().join(format!("openomsi-packs-{}", std::process::id()));
    let obj = root.join("Sceneryobjects/X");
    let cfgs = root.join("Vehicles/B/model/Configuration Files");
    std::fs::create_dir_all(&obj).unwrap();
    std::fs::create_dir_all(&cfgs).unwrap();
    std::fs::write(obj.join("a.o3d"), b"x").unwrap();
    omsi_cfg::add_content_root(root.clone());
    let model = cfgs.join("m.cfg");
    std::fs::write(&model, "[mesh]\r\n..\\..\\..\\Sceneryobjects\\X\\a.o3d\r\n..\\..\\..\\Other\\b.o3d\r\n").unwrap();
    let packs = super::missing_packs_of(&model);
    omsi_cfg::remove_content_root(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(packs, vec!["Other".to_string()]);
}

include!("passenger_animation_tests.rs");

#[test]
fn the_games_options_survive_a_save() {
    // what the pause menu's Options change, read back as they were set
    let mut v = settings_from_text(None);
    for (k, x) in [("steer_look", json!(true)), ("discord_status", json!(false)), ("voice_chat", json!(false)), ("launcher_rest", json!(false)), ("camera_collision", json!(false)), ("brake_hold", json!(false)), ("auto_clutch", json!(false)), ("momentary_gears", json!(true)), ("ff_enabled", json!(false)), ("head_tracking", json!(true)), ("collision_objects", json!(false)), ("led_mips", json!(2.5)), ("led_glow", json!(11)), ("look_sens", json!(0.5)), ("look_smoothing_ms", json!(120.0)), ("blinker_cancel", json!(false)), ("pedal_brake", json!(1.5)), ("mouse_pedal_strength", json!(1.75)), ("head_idle", json!(0.35)), ("head_idle_pace", json!(1.5)), ("seat_y", json!(-0.1)), ("seat_pitch_deg", json!(8.0))] {
        v[k] = x;
    }
    let back = settings_from_text(Some(&settings_to_text(&v, None)));
    assert_eq!(settings_from_text(Some("seat_pitch_deg=NaN\n"))["seat_pitch_deg"], json!(0.0));
    assert_eq!(settings_from_text(Some("seat_pitch_deg=90\n"))["seat_pitch_deg"], json!(45.0));
    let mut r = settings_from_text(None);
    assert_eq!(r["resolution"], json!("auto"));
    r["resolution"] = json!("1280x800");
    assert_eq!(settings_from_text(Some(&settings_to_text(&r, None)))["resolution"], json!("1280x800"));
    assert_eq!(resolution_text("1920 x 1080"), "1920x1080");
    assert_eq!(resolution_text("huge"), "auto");
    for k in ["steer_look", "discord_status", "voice_chat", "launcher_rest", "camera_collision", "brake_hold", "auto_clutch", "momentary_gears", "ff_enabled", "head_tracking", "collision_objects", "led_mips", "led_glow", "look_sens", "look_smoothing_ms", "blinker_cancel", "pedal_brake", "mouse_pedal_strength", "head_idle", "head_idle_pace", "seat_y", "seat_pitch_deg"] {
        assert_eq!(back[k], v[k], "{k}");
    }
    assert!(settings_from_text(None)["discord_status"].as_bool().unwrap());
    assert!(settings_from_text(None)["launcher_rest"].as_bool().unwrap());
    assert!(settings_from_text(None)["voice_chat"].as_bool().unwrap());
    let prior = settings_from_text(Some("discord_status=1\ndiscord_status=0\n"));
    assert!(!prior["discord_status"].as_bool().unwrap());
    let mut enabled = prior;
    enabled["discord_status"] = json!(true);
    let saved = settings_to_text(&enabled, Some("discord_status=0\ndiscord_status=0\n"));
    assert_eq!(saved.lines().filter(|line| line.starts_with("discord_status=")).count(), 1);
    assert!(settings_from_text(Some(&saved))["discord_status"].as_bool().unwrap());
    let custom_id = "discord_app_id=123456\n";
    let values = settings_from_text(Some(custom_id));
    assert_eq!(values["discord_app_id"], json!("123456"));
    let saved = settings_to_text(&values, Some(custom_id));
    assert_eq!(settings_from_text(Some(&saved))["discord_app_id"], json!("123456"));
}

#[test]
fn dsc_files_give_name_and_description() {
    let d = super::parse_dsc("\r\n[friendlyname]\r\nMAN\r\nNL202 - EN92\r\nBeige\r\n\r\n[description]\r\nAlthough the BVG did not purchase\r\n\r\n-Technical specifications-\r\n[end]\r\n");
    assert_eq!(d.name, vec!["MAN", "NL202 - EN92", "Beige"]);
    assert_eq!(d.description, "Although the BVG did not purchase\n\n-Technical specifications-");
    let w = super::parse_dsc("[name]\r\nGround Fog\r\n\r\n[description]\r\nHeavy ground fog limits the maximum visibility dangerously!\r\n[end]\r\n");
    assert_eq!(w.name, vec!["Ground Fog"]);
    assert_eq!(w.description, "Heavy ground fog limits the maximum visibility dangerously!");
    let p = std::path::Path::new("/x/maps/Spandau/global.cfg");
    assert_eq!(super::dsc_candidates(p, "ENG"), vec![std::path::PathBuf::from("/x/maps/Spandau/global_ENG.dsc")]);
    assert_eq!(super::dsc_candidates(p, "FRA").len(), 2);
    assert_eq!(super::dsc_candidates(std::path::Path::new("/v/MAN_EN92_main.bus"), "DEU"), vec![std::path::PathBuf::from("/v/MAN_EN92_main_DEU.dsc")]);
}

use super::*;

/// A duty file written before the number plate field (or one that leaves it out) loads
/// with no plate, and a plate the player typed is kept as it stands.
#[test]
fn a_picked_trip_starts_the_rest_of_the_tour() {
    let d = Duty { map: "maps/x/global.cfg".into(), bus: "Vehicles/x.bus".into(), time: "09:43".into(), line: Some("14".into()), tour: Some("1".into()), trip: Some("5".into()), whole_tour: true, ..Default::default() };
    let a = duty_args_for_root(&d, Path::new("test-omsi")).unwrap();
    let k = a.iter().position(|x| x == "--trip").unwrap();
    assert_eq!((a[k + 1].as_str(), a[k + 2].as_str()), ("5", "--whole-tour"));
    let alone = duty_args_for_root(&Duty { whole_tour: false, ..d }, Path::new("test-omsi")).unwrap();
    assert!(!alone.iter().any(|x| x == "--whole-tour"));
}

include!("lan_join_tests.rs");
#[test]
fn a_duty_keeps_its_plate_and_older_files_load_without_one() {
    let old: Duty = serde_json::from_str(r#"{"map":"maps/x/global.cfg","bus":"Vehicles/x.bus","time":"09:00"}"#).unwrap();
    assert_eq!(old.plate, None);
    let typed: Duty = serde_json::from_str(r#"{"map":"maps/x/global.cfg","bus":"Vehicles/x.bus","time":"09:00","plate":"B-AB 1234"}"#).unwrap();
    assert_eq!(typed.plate.as_deref(), Some("B-AB 1234"));
}

/// The fleet number picked in the launcher reaches the game.
#[test]
fn a_duty_passes_its_fleet_number() {
    let d: Duty = serde_json::from_str(r#"{"map":"maps/x/global.cfg","bus":"Vehicles/x.bus","time":"09:00","number":"4711"}"#).unwrap();
    let a = duty_args_for_root(&d, Path::new("test-omsi")).unwrap();
    assert!(a.windows(2).any(|w| w[0] == "--number" && w[1] == "4711"), "{a:?}");
}

#[test]
fn portuguese_variants_are_distinct() {
    assert_eq!(language_code("pt-BR"), "PTB");
    assert_eq!(language_iso("PTB"), "pt");
    assert_eq!(language_code("pt-PT"), "PTP");
    assert_eq!(language_iso("PTP"), "pt-pt");
}

#[test]
fn catalan_survives_settings_round_trip() {
    for alias in ["CAT", "ca", "ca-ES", "ca-AD", "Català", "catala", "Catalan"] {
        assert_eq!(language_code(alias), "CAT");
        assert_eq!(language_iso(alias), "ca");
        let settings = settings_from_text(Some(&format!("language={alias}\n")));
        assert_eq!(settings["language"], "CAT");
        let saved = settings_to_text(&settings, None);
        assert_eq!(settings_from_text(Some(&saved))["language"], "CAT");
    }
}

#[test]
fn mirror_rendering_can_be_disabled() {
    let v = settings_from_text(Some("mirror_size=0\n"));
    assert_eq!(v["mirror_size"], 0);
    let text = settings_to_text(&v, None);
    assert!(text.lines().any(|l| l == "mirror_size=0"), "{text}");
}

#[test]
fn mirror_refresh_survives_the_launcher() {
    assert_eq!(settings_from_text(None)["mirror_refresh"], "full");
    for mode in ["off", "eco", "full", "every"] {
        let v = settings_from_text(Some(&format!("mirror_refresh={mode}\n")));
        assert_eq!(v["mirror_refresh"], mode);
        let text = settings_to_text(&v, None);
        assert!(text.lines().any(|l| l == format!("mirror_refresh={mode}")), "{text}");
    }
}

#[test]
fn mirror_hud_every_frame_survives_the_launcher() {
    assert_eq!(settings_from_text(None)["mirror_hud_every_frame"], false);
    for (word, on) in [("1", true), ("0", false)] {
        let v = settings_from_text(Some(&format!("mirror_hud_every_frame={word}\n")));
        assert_eq!(v["mirror_hud_every_frame"], on, "{word}");
        let text = settings_to_text(&v, None);
        assert!(text.lines().any(|l| l == format!("mirror_hud_every_frame={}", on as u8)), "{text}");
    }
}

#[test]
fn sixteen_x_anisotropy_survives_the_launcher() {
    let v = settings_from_text(Some("anisotropy=16\n"));
    assert_eq!(v["anisotropy"], 16);
    let text = settings_to_text(&v, None);
    assert!(text.lines().any(|l| l == "anisotropy=16"), "{text}");
    assert_eq!(settings_from_text(Some("anisotropy=32\n"))["anisotropy"], 16);
}

#[test]
fn update_settings_round_trip() {
    // no file: look for updates, ask before installing
    let d = settings_from_text(None);
    assert_eq!((d["update_check"].clone(), d["update_auto"].clone()), (json!(true), json!(false)));
    let v = settings_from_text(Some("update_check=0\nupdate_auto=1\n"));
    assert_eq!((v["update_check"].clone(), v["update_auto"].clone()), (json!(false), json!(true)));
    let text = settings_to_text(&v, None);
    assert!(text.lines().any(|l| l == "update_check=0") && text.lines().any(|l| l == "update_auto=1"), "{text}");
    assert_eq!(d["update_channel"], json!("stable"));
    let beta = settings_from_text(Some("update_channel=Beta\n"));
    assert_eq!(beta["update_channel"], json!("beta"));
    assert!(settings_to_text(&beta, None).lines().any(|l| l == "update_channel=beta"));
    assert_eq!(settings_from_text(Some("update_channel=nightly\n"))["update_channel"], json!("stable"));
}

#[test]
fn vr_settings_round_trip() {
    let mut settings = settings_from_text(None);
    settings["vr"] = json!(true);
    settings["vr_scale"] = json!(0.8);
    settings["vr_head_smoothing_ms"] = json!(10);
    settings["vr_mirror_rate"] = json!(0);
    settings["vr_desktop_mirror"] = json!(false);
    let saved = settings_to_text(&settings, None);
    let loaded = settings_from_text(Some(&saved));
    for key in ["vr", "vr_scale", "vr_head_smoothing_ms", "vr_mirror_rate", "vr_desktop_mirror"] {
        assert_eq!(loaded[key], settings[key], "{key} was not saved");
    }
}

#[test]
fn triple_screen_settings_survive_launcher_save() {
    let input = "triple_screen=1\ntriple_span=0\ntriple_width_mm=620\ntriple_distance_mm=700\ntriple_bezel_mm=18\ntriple_left_angle_deg=50\ntriple_right_angle_deg=40\ntriple_eye_height_mm=60\n";
    let values = settings_from_text(Some(input));
    let loaded = settings_from_text(Some(&settings_to_text(&values, Some(input))));
    for key in [
        "triple_screen",
        "triple_span",
        "triple_width_mm",
        "triple_distance_mm",
        "triple_bezel_mm",
        "triple_left_angle_deg",
        "triple_right_angle_deg",
        "triple_eye_height_mm",
    ] {
        assert_eq!(loaded[key], values[key], "{key}");
    }
    let invalid = settings_from_text(Some(
        "triple_width_mm=NaN\ntriple_distance_mm=inf\ntriple_bezel_mm=-20\n",
    ));
    assert_eq!(invalid["triple_width_mm"], 600.0);
    assert_eq!(invalid["triple_distance_mm"], 650.0);
    assert_eq!(invalid["triple_bezel_mm"], 0.0);
    assert_eq!(values["triple_hud_center"], true);
    let values = settings_from_text(Some(
        "triple_screen=1\ntriple_hud_center=0\ntriple_fov_deg=75\nfov=50\n",
    ));
    let loaded = settings_from_text(Some(&settings_to_text(&values, None)));
    assert_eq!(loaded["triple_hud_center"], false);
    assert_eq!(loaded["triple_fov_deg"], 75.0);
    assert_eq!(loaded["fov"], 50.0);
}

#[test]
fn steering_view_settings_survive_the_launcher() {
    let values = settings_from_text(Some("steer_look=1\nsteer_look_angle=45\nsteer_look_response=0.5\n"));
    let saved = settings_to_text(&values, Some("steer_look_angle=10\nsteer_look_response=0.1\n"));
    let loaded = settings_from_text(Some(&saved));
    assert_eq!(loaded["steer_look"], json!(true));
    assert_eq!(loaded["steer_look_angle"], json!(45.0));
    assert_eq!(loaded["steer_look_response"], json!(0.5));
    assert_eq!(saved.lines().filter(|l| l.starts_with("steer_look_angle=")).count(), 1);
    let invalid = settings_from_text(Some("steer_look_angle=NaN\nsteer_look_response=NaN\n"));
    assert_eq!(invalid["steer_look_angle"], json!(30.0));
    assert_eq!(invalid["steer_look_response"], json!(0.25));
}

#[test]
fn high_vr_mirror_rates_survive_launcher_settings() {
    for rate in [-1, 0, 16, 60, 120, 240, 360] {
        let mut settings = settings_from_text(None);
        // Select controls store their values as strings.
        settings["vr_mirror_rate"] = json!(rate.to_string());
        let saved = settings_to_text(&settings, None);
        let loaded = settings_from_text(Some(&saved));
        assert_eq!(loaded["vr_mirror_rate"], json!(rate));
    }
    assert_eq!(settings_from_text(Some("vr_mirror_rate=NaN\n"))["vr_mirror_rate"], json!(16));
    assert_eq!(settings_from_text(Some("vr_mirror_rate=999\n"))["vr_mirror_rate"], json!(360));
}

/// The interface size: 100% without a file, kept as set, and a hand-written value out
/// of range brought back into it.
#[test]
fn interface_size_round_trip() {
    assert_eq!(settings_from_text(None)["ui_scale"], json!(1.0));
    let mut v = settings_from_text(None);
    v["ui_scale"] = json!(1.5);
    let text = settings_to_text(&v, None);
    assert!(text.lines().any(|l| l == "ui_scale=1.5"), "{text}");
    assert_eq!(settings_from_text(Some(&text))["ui_scale"], json!(1.5));
    v["chat_size"] = json!(2.2);
    let text = settings_to_text(&v, None);
    assert_eq!(text.lines().filter(|l| l.starts_with("chat_size=")).collect::<Vec<_>>(), ["chat_size=2.2"], "{text}");
    assert_eq!(settings_from_text(Some(&text))["chat_size"], json!(2.2));
    assert_eq!(settings_from_text(Some("ui_scale=9\n"))["ui_scale"], json!(2.0));
    assert_eq!(settings_from_text(Some("ui_scale=0.1\n"))["ui_scale"], json!(0.5));
    assert_eq!(settings_from_text(Some("ui_scale=big\n"))["ui_scale"], json!(1.0));
    // (growing with the window: on unless switched off, and kept)
    assert_eq!(settings_from_text(None)["ui_scale_window"], json!(true));
    let off = settings_from_text(Some("ui_scale_window=0\n"));
    assert_eq!(off["ui_scale_window"], json!(false));
    assert!(settings_to_text(&off, None).lines().any(|l| l == "ui_scale_window=0"));
    // (the notes in the corner: on unless switched off, and kept)
    assert_eq!(settings_from_text(None)["notes"], json!(true));
    assert!(settings_to_text(&settings_from_text(Some("notes=0\n")), None).lines().any(|l| l == "notes=0"));
    // (the game's information bar: off unless the game left it on, and kept, #1164)
    assert_eq!(settings_from_text(None)["info_bar"], json!(false));
    let on = settings_from_text(Some("info_bar=1\n"));
    assert_eq!(on["info_bar"], json!(true));
    assert!(settings_to_text(&on, None).lines().any(|l| l == "info_bar=1"));
    assert_eq!(settings_from_text(None)["perf_overlay"], json!("off"));
    assert!(settings_to_text(&settings_from_text(Some("perf_overlay=detailed\n")), None).lines().any(|l| l == "perf_overlay=detailed"));
}

/// The opacity is the whole interface's now: a file of an older build, where it was the
/// navigator's, keeps its value under the new name, written once.
#[test]
fn the_navigators_opacity_becomes_the_interfaces() {
    let old = "navigator_opacity=0.5\n";
    let v = settings_from_text(Some(old));
    assert_eq!(v["ui_opacity"], json!(0.5));
    let text = settings_to_text(&v, Some(old));
    assert!(text.lines().any(|l| l == "ui_opacity=0.5"), "{text}");
    assert!(!text.contains("navigator_opacity"), "{text}");
}

#[test]
fn the_wheels_tremble_settings_round_trip() {
    let mut settings = settings_from_text(None);
    assert_eq!((settings["ff_road_vib"].clone(), settings["ff_engine_vib"].clone(), settings["ff_fade"].clone()), (json!(1.0), json!(1.0), json!(0.28)));
    settings["ff_road_vib"] = json!(1.5);
    settings["ff_engine_vib"] = json!(0.0);
    settings["ff_fade"] = json!(0.5);
    let saved = settings_to_text(&settings, None);
    let loaded = settings_from_text(Some(&saved));
    for key in ["ff_road_vib", "ff_engine_vib", "ff_fade"] {
        assert_eq!(loaded[key], settings[key], "{key} was not saved");
    }
    // the launcher's own line, and a value past what a wheel can be set to
    assert_eq!(settings_from_text(Some("ff_road_vib=1.5\n"))["ff_road_vib"], 1.5);
    assert_eq!(settings_from_text(Some("ff_road_vib=9\n"))["ff_road_vib"], 4.0);
    assert_eq!(settings_from_text(Some("ff_fade=12\n"))["ff_fade"], 1.5);
}

#[test]
fn settings_keep_what_the_page_does_not_manage() {
    // the user's file: a key of a newer game, a hand-written one, and the game's other
    // spellings of keys the page writes
    let old = "# mine\nversion=2\nmsaa=1\nview_distance=1500\nlang=de\ntexmemlimit=401.0\nfuture_switch=7\nOMSI_Thing = on\nfractal=0\n";
    let v = settings_from_text(Some(old));
    assert_eq!(v["msaa"], 1);
    assert_eq!(v["view_distance"], "1500");
    assert_eq!(v["language"], "DEU");
    // (OMSI's texture budget is not the video memory: Automatic)
    assert_eq!(v["video_memory"], 0);
    assert!(v.get("texture_memory").is_none());
    assert_eq!(v["texture_compression"], true);
    assert_eq!(v["detail_textures"], false);
    // the page changes a few things (its selects give strings) and saves
    let mut page = v.clone();
    page["view_distance"] = json!("2000");
    page["language"] = json!("FRA");
    page["video_memory"] = json!("3000");
    page["texture_compression"] = json!(false);
    let text = settings_to_text(&page, Some(old));
    for line in ["view_distance=2000", "language=FRA", "video_memory=3000", "texture_compression=0", "detail_textures=0", "future_switch=7", "OMSI_Thing = on"] {
        assert!(text.lines().any(|l| l == line), "{line} missing in\n{text}");
    }
    // the old spellings would override what was just written: gone, and nothing twice
    for gone in ["lang=", "texmemlimit=", "texture_memory=", "fractal=", "view_distance=1500"] {
        assert!(!text.contains(gone), "{gone} kept in\n{text}");
    }
    let keys: Vec<&str> = text.lines().filter_map(|l| l.split_once('=')).map(|(k, _)| k.trim()).collect();
    let mut unique = keys.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(keys.len(), unique.len(), "a key written twice:\n{text}");
    // and it reads back as saved
    let back = settings_from_text(Some(&text));
    assert_eq!(back["view_distance"], "2000");
    assert_eq!(back["language"], "FRA");
    assert_eq!(back["video_memory"], 3000);
    assert_eq!(back["texture_compression"], false);
}

/// The lines follow the date as the game's do: Spandau's 1991 timetable change takes
/// "5 & 5N" off and brings "130 & N30"; without a date it is the game's default day.
#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
fn lines_follow_the_chrono_date() {
    let root = original_root();
    let map = root.join("maps/Berlin-Spandau");
    require_content(&[&map.join("TTData")]);
    let names = |date: &str| lines_on(&map, date).unwrap().into_iter().map(|l| l.name).collect::<Vec<_>>();
    let (then, now) = (names(""), names("2026-09-17"));
    assert_eq!(then, names(DEFAULT_DATE));
    assert!(then.iter().any(|l| l == "5 & 5N") && !then.iter().any(|l| l == "130 & N30"), "{then:?}");
    assert!(!now.iter().any(|l| l == "5 & 5N") && now.iter().any(|l| l == "130 & N30"), "{now:?}");
    assert!(names("1991-06-01").iter().any(|l| l == "5 & 5N") && !names("1991-06-02").iter().any(|l| l == "5 & 5N"));
    assert!(lines_on(&map, "someday").is_err());
}

#[test]
fn settings_defaults_and_automatic_values() {
    let v = settings_from_text(None);
    assert_eq!(v["view_distance"], "auto");
    assert_eq!(v["language"], "ENG");
    assert_eq!(v["video_memory"], 0);
    let text = settings_to_text(&v, None);
    for line in ["view_distance=auto", "language=ENG", "video_memory=0", "texture_compression=1", "render_scale=auto"] {
        assert!(text.lines().any(|l| l == line), "{line} missing in\n{text}");
    }
    // nonsense from a hand-edited file falls back to the defaults
    let v = settings_from_text(Some("view_distance=far\nview_distance=-5\nlanguage=Klingon\nvideo_memory=lots\n"));
    assert_eq!((v["view_distance"].as_str(), v["language"].as_str(), v["video_memory"].as_i64()), (Some("auto"), Some("ENG"), Some(0)));
    // a number from a script instead of the select's string
    let text = settings_to_text(&json!({ "view_distance": 900, "video_memory": 1500.0 }), None);
    assert!(text.contains("\nview_distance=900\n") && text.contains("\nvideo_memory=1500\n"), "{text}");
}

/// Dates fitted to a map's year span: moved to the nearest allowed year keeping the day,
/// 29 February to the 28th in a year without it; inside the span, with no span, or not a
/// date: unchanged. And the spans `[years]` gives.
#[test]
fn dates_are_fitted_to_the_maps_year_span() {
    let nineties = Some((1991, 1999));
    for (date, fitted) in [("1996-07-04", "1996-07-04"), ("2031-03-15", "1999-03-15"), ("1960-12-01", "1991-12-01"), ("1999-12-31", "1999-12-31")] {
        assert_eq!(fit_date_to_years(date, nineties), fitted);
    }
    assert_eq!(fit_date_to_years("2028-02-29", Some((2001, 2003))), "2003-02-28");
    assert_eq!(fit_date_to_years("2028-02-29", Some((1979, 1980))), "1980-02-29");
    assert_eq!(fit_date_to_years("1950-06-30", Some((1972, i32::MAX))), "1972-06-30");
    assert_eq!(fit_date_to_years("2031-03-15", None), "2031-03-15");
    assert_eq!(fit_date_to_years("next tuesday", nineties), "next tuesday");
    let mut g = omsi_map::GlobalCfg::default();
    let mut span = |years| {
        g.years = years;
        year_span_of(&g)
    };
    assert_eq!(span((0, 0)), None);
    assert_eq!(span((0, 1988)), Some((i32::MIN, 1988)));
    assert_eq!(span((2010, 0)), Some((2010, i32::MAX)));
    assert_eq!(span((2010, 2004)), Some((2010, i32::MAX)), "a last year before the first");
    assert_eq!(span((1972, 1989)), Some((1972, 1989)));
}

#[test]
fn a_depot_bus_is_handed_to_the_game() {
    let d = Duty { map: "maps/x/global.cfg".into(), bus: "Vehicles/x.bus".into(), time: "09:00".into(), depot: Some("6703c1a0".into()), ..Default::default() };
    let a = duty_args_for_root(&d, Path::new("test-omsi")).unwrap();
    let k = a.iter().position(|x| x == "--depot").unwrap();
    assert_eq!(a[k + 1], "6703c1a0");
    let none = duty_args_for_root(&Duty { depot: None, ..d }, Path::new("test-omsi")).unwrap();
    assert!(!none.iter().any(|x| x == "--depot"));
}
