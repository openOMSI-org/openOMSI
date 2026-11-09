use super::{ai_context, apply_ai_context};
use omsi_lan::mirror::relative_to_roots;
    use std::path::{Path, PathBuf};

    #[test]
    fn client_ai_has_schedule_and_all_system_values_before_init_and_frame() {
        let mut script = String::from("{init}\n");
        let mut vars = String::from("schedule_active\ninit_active\ninit_count\ninit_index\ninit_delay\ninit_term\ninit_arr\ninit_dep\nframe_active\nframe_index\n");
        for (i, sys) in omsi_script::SysVar::ALL.into_iter().enumerate() {
            script.push_str(&format!("(L.S.{}) (S.L.sys_{i})\n", sys.name()));
            vars.push_str(&format!("sys_{i}\n"));
        }
        script.push_str("(L.L.schedule_active) (S.L.init_active)\n(M.V.GetTTBusstopCount) (S.L.init_count)\n(M.V.GetTTBusstopIndex) (S.L.init_index)\n(M.V.GetTTDelay) (S.L.init_delay)\n(M.V.GetTTTerminusIndex) (S.L.init_term)\n0 (M.V.GetTTBusstopArr) (S.L.init_arr)\n1 (M.V.GetTTBusstopDep) (S.L.init_dep)\n(M.V.GetTTLineString) (S.$.init_line)\n1 (M.V.GetTTBusstopName) (S.$.init_stop)\n{end}\n{frame_ai}\n(L.L.schedule_active) (S.L.frame_active)\n(M.V.GetTTBusstopIndex) (S.L.frame_index)\n{end}\n");
        let template = crate::schedule::tests::script_test_vehicle(&script, &vars, "init_line\ninit_stop\n");
        let mut owner = omsi_sim::VehicleHost::new(Default::default());
        owner.clock.set_date(2024, 3, 1);
        owner.clock.time = 23.0 * 3600.0;
        owner.clock.run_time = 1234.0;
        owner.temperature = -8.0;
        owner.abs_humidity = 7.25;
        owner.precip_type = 2.0;
        owner.precip_rate = 0.8;
        owner.sun_alt = -10.0;
        owner.coll_pos = [1.0, 2.0, 3.0];
        owner.coll_energy = 4.0;
        owner.mouse = (0.2, 0.3);
        owner.schedule_active = 1.0;
        owner.tt_line = "042".into();
        owner.tt_stops = vec![("First".into(), 100.0, 120.0), ("Repeated".into(), 200.0, 220.0)];
        owner.tt_stop_ids = vec![7, 7];
        owner.tt_busstop_index = 1;
        owner.tt_terminus_index = 3;
        owner.tt_delay = -12.5;
        let snapshot = ai_context(&owner);
        let mut receiver = omsi_net::ai_context::Receiver::default();
        let (_, snapshot) = receiver.receive(&snapshot.packet(omsi_net::PROTOCOL as u8, 7, 1, 0).unwrap(), omsi_net::PROTOCOL as u8).unwrap();
        let mut host = omsi_sim::VehicleHost::new(Default::default());
        apply_ai_context(&mut host, &snapshot);
        let mut v = omsi_sim::VehicleInstance::new(template.ty.clone(), host);
        assert_eq!(v.var("init_active"), Some(1.0));
        assert_eq!(v.var("init_count"), Some(2.0));
        assert_eq!(v.var("init_index"), Some(1.0));
        assert_eq!(host.tt_ibis_stops, host.tt_stops);
        assert_eq!(host.tt_ibis_busstop_index, 1);
        assert_eq!(v.var("init_delay"), Some(-12.5));
        assert_eq!(v.var("init_term"), Some(3.0));
        assert_eq!(v.var("init_arr"), Some(100.0));
        assert_eq!(v.var("init_dep"), Some(220.0));
        assert_eq!(v.str_var("init_line"), "042");
        assert_eq!(v.str_var("init_stop"), "Repeated");
        for (i, value) in snapshot.system.iter().enumerate() { assert_eq!(v.var(&format!("sys_{i}")), Some(*value), "{}", omsi_script::SysVar::ALL[i].name()); }
        v.update_ai(0.025, &Default::default());
        assert_eq!(v.var("frame_active"), Some(1.0));
        assert_eq!(v.var("frame_index"), Some(1.0));
        assert_eq!(v.host.clock.timegap, 0.025);
        assert!(v.host.clock.run_time > 1234.0);
        apply_ai_context(&mut v.host, &omsi_net::ai_context::Context::default());
        v.update_ai(0.025, &Default::default());
        assert_eq!(v.var("frame_active"), Some(0.0));
        assert!(v.host.tt_stops.is_empty());
    }

    #[test]
    fn a_file_is_named_relative_to_the_deepest_root_holding_it() {
        // the content folder first, the OMSI 2 folder inside it after (a server's layout)
        let roots = [PathBuf::from("/srv/openOMSI"), PathBuf::from("/srv/openOMSI/OMSI 2")];
        let golf = Path::new("/srv/openOMSI/OMSI 2/Vehicles/VW_Golf_2/ai_vw_golf_2.bus");
        assert_eq!(relative_to_roots(golf, &roots).as_deref(), Some("Vehicles/VW_Golf_2/ai_vw_golf_2.bus"));
        // a mod in the content folder itself stays relative to that
        let m = Path::new("/srv/openOMSI/Vehicles/Mod/mod.bus");
        assert_eq!(relative_to_roots(m, &roots).as_deref(), Some("Vehicles/Mod/mod.bus"));
        assert_eq!(relative_to_roots(Path::new("/elsewhere/x.bus"), &roots), None);
    }

    /// The host's bus to U Ruhleben (row 2) shows U Ruhleben on ours too, not Machandelweg
    /// (row 1), whose sign has RUHLEBEN on its second line.
    #[test]
    fn a_hosts_timetable_bus_shows_the_row_it_was_given() {
        let t = |code: i32, id: &str, s: &[&str]| omsi_vehicle::hof::Terminus { code, texture_id: id.into(), strings: s.iter().map(|x| x.to_string()).collect(), ..Default::default() };
        let hof = omsi_vehicle::Hof { termini: vec![t(0, "Empty", &[""]), t(194, "Machandelweg", &["MACHANDELWEG", "RUHLEBEN"]), t(282, "U Ruhleben", &["RUHLEBEN", "U-BAHNHOF"])], ..Default::default() };
        let mut v = crate::schedule::tests::script_test_vehicle("{frame}\n{end}\n", "AI_target_index\nIBIS_TerminusIndex\nIBIS_TerminusCode\n", "SetLineTo\n");
        super::show_car_destination(&mut v, Some(&hof), "5", "#2");
        assert_eq!((v.var("AI_target_index"), v.var("IBIS_TerminusCode")), (Some(2.0), Some(282.0)));
        assert_eq!(v.str_var("SetLineTo"), "5");
        // no such row, or no depot file: nothing changes
        super::show_car_destination(&mut v, Some(&hof), "5", "#7");
        super::show_car_destination(&mut v, None, "5", "#1");
        assert_eq!(v.var("AI_target_index"), Some(2.0));
    }

    /// What the host tells of a timetable bus holds the fleet number and plate it drew and its
    /// depot file by name: a player's copy showed no number and read the destination's row in
    /// a depot file of its own choosing.
    #[test]
    fn a_hosts_bus_tells_its_number_plate_and_depot_file() {
        let mut v = crate::schedule::tests::script_test_vehicle("{frame}\n{end}\n", "AI_target_index\n", "SetLineTo\nnumber\nident\n");
        for (name, text) in [("SetLineTo", "8022-10"), ("number", "80.849"), ("ident", "EMT-1234")] {
            let i = v.ty.program.str_var(name).unwrap();
            v.state.str_vars[i as usize] = text.into();
        }
        assert!(v.set_var("AI_target_index", 4.0));
        v.host.hof = Some(std::sync::Arc::new(omsi_vehicle::Hof { path: PathBuf::from("Vehicles/Torino/Brigadeiro Sampaio.hof"), ..Default::default() }));
        let m = super::car_meta(&v);
        assert_eq!((m.line.as_str(), m.destination.as_str()), ("8022-10", "#4"));
        assert_eq!((m.number.as_str(), m.ident.as_str(), m.hof.as_str()), ("80.849", "EMT-1234", "Brigadeiro Sampaio"));
        v.host.hof = None;
        assert_eq!(super::car_meta(&v).hof, "", "no depot file: none named");
    }
