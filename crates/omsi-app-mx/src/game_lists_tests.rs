#[test]
fn a_typed_route_with_a_letter_goes_onto_the_roller_blind() {
    use crate::schedule::BlindPick;
    let pending = Some(BlindPick {
        row: 3,
        line: "41".into(),
    });
    assert_eq!(
        super::blind_with_line(pending, Some(7), "92A"),
        Some(BlindPick {
            row: 3,
            line: "92A".into()
        })
    );
    assert_eq!(
        super::blind_with_line(None, Some(7), "4X"),
        Some(BlindPick {
            row: 7,
            line: "4X".into()
        })
    );
    assert_eq!(super::blind_with_line(None, None, "1A"), None);
    assert_eq!(
        super::kept_blind_line("92A", true),
        Some("92A".into()),
        "a destination picked: the letter stays"
    );
    assert_eq!(
        super::kept_blind_line("92A", false),
        None,
        "92A then 41 typed: the new number goes up"
    );
    assert_eq!(super::kept_blind_line("41", true), None);
    assert!(
        super::route_keeps_blind("") && super::route_keeps_blind("92A"),
        "a route the IBIS cannot take leaves the blind's text"
    );
    assert!(
        !super::route_keeps_blind("41") && !super::route_keeps_blind("92E"),
        "a number the IBIS takes goes up"
    );
}

/// "Random livery" draws among all liveries (#2410: it always gave the same one).
#[test]
fn random_livery_reaches_every_livery() {
    let names = ["A", "B", "C", "D"];
    let mut seen = std::collections::HashSet::new();
    for seed in 0..64u64 {
        seen.insert(*super::pick_random(&names, seed).unwrap());
    }
    assert_eq!(seen.len(), 4);
    assert!(super::pick_random::<&str>(&[], 7).is_none());
    assert_ne!(super::fresh_seed(), super::fresh_seed());
}

/// The start points as OMSI lists them: a name once (Gerolstein's ten depot places of
/// one name are one line), its places in global.cfg's order, the names in OMSI's order.
#[test]
fn start_points_are_grouped_by_name_in_omsis_order() {
    let ep = |name: &str| omsi_map::global::EntryPoint {
        name: name.into(),
        ..Default::default()
    };
    let points = [
        ep("beta"),
        ep("Nordspitze Bauernhof"),
        ep("Äpfel"),
        ep(""),
        ep("nordspitze bauernhof"),
        ep("ab"),
        ep("Nordspitze Bauernhof"),
    ];
    let groups = super::spot_groups(&points);
    let names: Vec<Option<&str>> = groups.iter().map(|g| g.0.as_deref()).collect();
    assert_eq!(
        names,
        [
            Some("ab"),
            Some("Äpfel"),
            Some("beta"),
            Some("Nordspitze Bauernhof"),
            None
        ]
    );
    assert_eq!(groups[3].1, [1, 4, 6]);
    assert_eq!(groups[4].1, [3]);
}

#[test]
fn searches_model_and_hof_names_and_paths_without_losing_their_actions() {
    assert!(super::search_matches(
        "Volvo BRT",
        "bus Vehicles/Pack/biarticulado.bus",
        "VOLVO biarticulado"
    ));
    assert!(super::search_matches(
        "Curitiba",
        "hof Vehicles/Bus/curitiba.hof",
        "curitiba .hof"
    ));
    assert!(!super::search_matches(
        "Curitiba",
        "hof curitiba.hof",
        "recife"
    ));
    assert!(super::search_matches("Anything", "bus x.bus", "  "));
}

/// The options page names the preset a file holds after the launcher saved it, whatever the
/// graphics mode the file was in before (a preset sets the mode), and "Custom" once one of its
/// values is changed.
#[test]
fn the_preset_is_recognised_from_any_mode() {
    for mode in ["vanilla", "vanilla_plus", "enhanced", "enhanced_plus"] {
        for (i, (_, preset)) in omsi_launcher_lib::graphics_presets()
            .into_iter()
            .enumerate()
        {
            let mut file =
                omsi_launcher_lib::settings_from_text(Some(&format!("graphics={mode}\n")));
            omsi_launcher_lib::apply_graphics_profile(&preset, &mut file);
            let mut file = omsi_launcher_lib::settings_from_text(Some(
                &omsi_launcher_lib::settings_to_text(&file, None),
            ));
            assert_eq!(super::preset_now(&file), Some(i), "{mode}: {i}");
            file["mirror_size"] = serde_json::json!(1024);
            assert_eq!(super::preset_now(&file), None);
        }
    }
}

/// #2369: the IBIS on 6 with letter code 12 (C6), the display showing "6a" typed by hand.
/// A destination picked keeps the IBIS's line code: given as the bare 6 it became 600,
/// and the stock matrix, drawing its number again on any change of
/// `IBIS_Linie_Complex`, put a plain 6 over the 6a about a second later.
#[test]
fn a_destination_picked_keeps_the_ibis_letter_under_a_display_only_route_number() {
    let mut v = crate::schedule::tests::script_test_vehicle(
        "{frame}\n{end}\n",
        "IBIS_LinieKurs\nIBIS_Linie_Suffix\nIBIS_Linie_Complex\nIBIS_TerminusCode\n",
        "Matrix_Nr\nSetLineTo\n",
    );
    v.set_var("IBIS_LinieKurs", 6.0);
    v.set_var("IBIS_Linie_Suffix", 12.0);
    v.set_var("IBIS_Linie_Complex", 612.0);
    let i = v.ty.program.str_var("Matrix_Nr").unwrap();
    v.state.str_vars[i as usize] = "6a".into();
    let hof = omsi_vehicle::Hof {
        termini: vec![omsi_vehicle::hof::Terminus {
            code: 7,
            texture_id: "Mei Foo".into(),
            strings: vec!["MEI FOO".into()],
            ..Default::default()
        }],
        ..Default::default()
    };
    let line = super::destination_line(&v);
    assert_eq!(line, "C6");
    crate::schedule::set_player_destination_with_display(&mut v, &hof, &line, Some("6a"), 0, &[]);
    assert_eq!(
        (
            v.var("IBIS_LinieKurs"),
            v.var("IBIS_Linie_Suffix"),
            v.var("IBIS_Linie_Complex")
        ),
        (Some(6.0), Some(12.0), Some(612.0))
    );
    assert_eq!(v.str_var("Matrix_Nr"), "6a");
    // the IBIS's 6E (suffix 10) is kept the same way
    v.set_var("IBIS_Linie_Suffix", 10.0);
    v.state.str_vars[i as usize] = String::new();
    assert_eq!(super::destination_line(&v), "6E");
}

/// A destination picked from the list keeps the route number the bus shows, its letter
/// too (92E, IBIS 92 and suffix 10).
#[test]
fn a_destination_picked_from_the_list_keeps_the_lines_letter() {
    let mut v = crate::schedule::tests::script_test_vehicle(
        "{frame}\n{end}\n",
        "IBIS_LinieKurs\nIBIS_Linie_Suffix\nIBIS_Linie_Complex\nIBIS_TerminusCode\n",
        "Matrix_Nr\nSetLineTo\n",
    );
    let set_str = |v: &mut omsi_sim::VehicleInstance, name: &str, s: &str| {
        let i = v.ty.program.str_var(name).unwrap();
        v.state.str_vars[i as usize] = s.to_string();
    };
    v.set_var("IBIS_LinieKurs", 92.0);
    v.set_var("IBIS_Linie_Suffix", 10.0);
    v.set_var("IBIS_Linie_Complex", 9210.0);
    set_str(&mut v, "Matrix_Nr", "92E");
    assert_eq!(super::destination_line(&v), "92E");
    let hof = omsi_vehicle::Hof {
        termini: vec![omsi_vehicle::hof::Terminus {
            code: 211,
            texture_id: "U Rathaus Spandau".into(),
            strings: vec!["RATHAUS SPANDAU".into()],
            ..Default::default()
        }],
        ..Default::default()
    };
    let line = super::destination_line(&v);
    crate::schedule::set_player_destination_at(&mut v, &hof, &line, 0, &[]);
    assert_eq!(
        (
            v.var("IBIS_LinieKurs"),
            v.var("IBIS_Linie_Suffix"),
            v.var("IBIS_Linie_Complex"),
            v.var("IBIS_TerminusCode")
        ),
        (Some(92.0), Some(10.0), Some(9210.0), Some(211.0))
    );
    // a route number the IBIS cannot take is left to the display: the IBIS keeps its own
    set_str(&mut v, "Matrix_Nr", "-10");
    assert_eq!(super::destination_line(&v), "92E");
    for alpha in ["R56", "CE19", "SB15"] {
        set_str(&mut v, "Matrix_Nr", alpha);
        assert_eq!(super::destination_line(&v), "92E");
        assert_eq!(super::form_route(&v, None), alpha);
    }
    assert!(super::display_route_line("R56") && !super::display_route_line("-10"));
    // nothing shown: the IBIS's number
    set_str(&mut v, "Matrix_Nr", "   ");
    set_str(&mut v, "SetLineTo", "");
    assert_eq!(super::destination_line(&v), "92E");
    // no matrix line, the line of an earlier pick left in SetLineTo and another typed on
    // the IBIS since: the IBIS's (only a roller blind shows SetLineTo)
    set_str(&mut v, "Matrix_Nr", "");
    set_str(&mut v, "SetLineTo", "5");
    v.set_var("IBIS_LinieKurs", 145.0);
    assert_eq!(super::destination_line(&v), "145E");
    let mut blind = crate::schedule::tests::script_test_vehicle(
        "{trigger:rollband_sync}\n{end}\n",
        "IBIS_LinieKurs\n",
        "SetLineTo\n",
    );
    set_str(&mut blind, "SetLineTo", "  5");
    blind.set_var("IBIS_LinieKurs", 145.0);
    assert_eq!(super::destination_line(&blind), "5");
    // a blind on 92A over an IBIS on 41: the form offers 92A, Select keeps it (#2799)
    set_str(&mut blind, "SetLineTo", "92A");
    blind.set_var("IBIS_LinieKurs", 41.0);
    let route = super::form_route(&blind, None);
    assert_eq!(route, "92A");
    assert!(super::route_keeps_blind(&route));
    assert_eq!(
        super::kept_blind_line(
            &blind.str_var("SetLineTo"),
            super::route_keeps_blind(&route)
        ),
        Some("92A".into())
    );
    set_str(&mut v, "Matrix_Nr", "92A");
    assert_eq!(
        super::form_route(&v, None),
        "92A",
        "a matrix showing 92A: the form offers it, not the IBIS's 145"
    );
    set_str(&mut v, "Matrix_Nr", "");
    assert_eq!(
        super::form_route(&v, None),
        super::destination_line(&v),
        "nothing lettered shown: the form offers the IBIS's line"
    );
    set_str(&mut blind, "SetLineTo", " 41");
    let pending = crate::schedule::BlindPick {
        row: 2,
        line: "92A".into(),
    };
    assert_eq!(
        super::form_route(&blind, Some(&pending)),
        "92A",
        "a bus still off: the pick waiting for it, not what the IBIS wrote"
    );
}

#[test]
fn escape_fov_updates_the_active_projection_and_can_restore_geometry() {
    let mut settings = crate::settings::Settings::default();
    assert_eq!(super::set_camera_fov(&mut settings, 50.0).0, "fov");
    settings.triple.enabled = true;
    assert_eq!(
        super::set_camera_fov(&mut settings, 75.0).0,
        "triple_fov_deg"
    );
    assert_eq!(settings.fov, 50.0);
    assert_eq!(settings.triple.fov_deg, 75.0);
    super::set_camera_fov(&mut settings, 0.0);
    assert_eq!(settings.triple.fov_deg, 0.0);
}
/// The game menu offers the 16x anisotropic filtering the launcher does (#669): set
/// there, it showed as a bare "16" here and could not be chosen again.
#[test]
fn sixteen_x_anisotropy_can_be_chosen_in_the_game_menu() {
    let file = serde_json::json!({ "anisotropy": 16 });
    let (options, at, _) = super::select_state(&file, "anisotropy");
    assert_eq!(at.map(|i| options[i]), Some(("16", "16x")));
}

#[test]
fn steps_wrap_round() {
    assert_eq!(super::next_step(&super::SPEEDS, 1.0), 2.0);
    assert_eq!(super::next_step(&super::SPEEDS, 15.0), 1.0);
    assert_eq!(super::next_step(&super::TRAFFIC, 35), 50);
}

#[test]
fn mouse_pedal_strength_steps_include_percentage_range_and_omsi_default() {
    let steps = super::steps_of("mouse_pedal_strength").unwrap();
    assert_eq!(steps.first(), Some(&0.5));
    assert_eq!(steps.last(), Some(&2.0));
    assert!(steps.contains(&1.0));
}

#[test]
fn visibility_goes_down_to_50_m_like_omsi() {
    let steps = super::steps_of("visibility").unwrap();
    assert_eq!(steps.first(), Some(&50.0));
    assert_eq!(steps.last(), Some(&50000.0));
    assert!(steps.windows(2).all(|w| w[0] < w[1]) && steps.contains(&100.0));
}

#[test]
fn lines_sort_as_numbers() {
    let mut v = vec!["13N", "5", "137", "N30", "92"];
    v.sort_by(|a, b| super::natural(a, b));
    assert_eq!(v, vec!["5", "13N", "92", "137", "N30"]);
}

#[test]
fn symbols_are_not_forced_through_numeric_ibis_lines() {
    assert!(super::numeric_ibis_line("10"));
    assert!(super::numeric_ibis_line("10E"));
    assert!(super::numeric_ibis_line("X10"));
    assert!(!super::numeric_ibis_line("-10"));
    assert!(!super::numeric_ibis_line("10-"));
    assert!(!super::numeric_ibis_line("EXP"));
}
