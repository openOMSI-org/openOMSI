//! What the timetable's tests need of a vehicle, and the tests of what the launcher's line
//! editor makes of the timetable.

/// A vehicle of the script `osc` that declares the variables `varlist` and the string
/// variables `stringvarlist` (one a line).
pub(crate) fn script_test_vehicle(osc: &str, varlist: &str, stringvarlist: &str) -> omsi_sim::VehicleInstance {
    // (a folder of its own: tests run side by side)
    static MADE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("omsi_ibis_dest_{}_{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("ibis.osc");
    let vars = dir.join("vars.txt");
    let strings = dir.join("strings.txt");
    std::fs::write(&vars, varlist).unwrap();
    std::fs::write(&strings, stringvarlist).unwrap();
    std::fs::write(&script, osc).unwrap();
    let program = omsi_script::compile(&omsi_script::CompileInput {
        scripts: vec![script],
        varlists: vec![vars],
        stringvarlists: vec![strings],
        ..Default::default()
    });
    assert!(program.errors.is_empty(), "{:?}", program.errors);
    let ty = std::sync::Arc::new(omsi_sim::VehicleType {
        def: Default::default(),
        model: Default::default(),
        model_dir: dir.clone(),
        program: std::sync::Arc::new(program),
        meshes: Vec::new(),
        paint_schemes: Vec::new(),
        texchanges: Vec::new(),
        wheel_meshes: Vec::new(),
        suspension_axles: Vec::new(),
        missing_packs: Vec::new(),
        mesh_bounds: Vec::new(),
        mesh_boxes: Vec::new(),
    });
    std::fs::remove_dir_all(dir).unwrap();
    omsi_sim::VehicleInstance::new(ty, omsi_sim::VehicleHost::new(Default::default()))
}

/// A line of the launcher's line editor, one way to a destination its depot file lacks and
/// back to one it has: the depot file as the editor writes it (`linehof`) gives the AI
/// bus's matrix its terminus and line, the player's IBIS the route of each trip, and a
/// typed route code its route, its terminus and its stops.
#[test]
fn a_players_line_is_known_to_the_depot_file() {
    use omsi_launcher_lib::{linehof, lines};
    use omsi_sim::timetable_run::{find_terminus, ibis_codes, ibis_target, routes_to};
    use std::path::Path;
    const HOF: &str = "[name]\r\nDorf\r\n\r\nstringcount_terminus\r\n3\r\n\r\nstringcount_busstop\r\n2\r\n\r\n\
        [addterminus]\r\n105\r\nKrankenhaus\r\nKRANKENHAUS\r\n  KRANKENHAUS\r\nKrankenhaus\r\n\r\n\
        [addbusstop]\r\nBauernhof\r\nBAUERNHOF\r\nBauernhof\r\n\r\n\
        [infosystem_trip]\r\n7601\r\nBAUERNHOF-KRANKENHAUS\r\n105\r\n76\r\n\r\n\
        [infosystem_busstop_list]\r\n2\r\nBauernhof\r\nKrankenhaus\r\n";
    let mut reg = lines::Registry { map: "Dorf".into(), ..Default::default() };
    let l = reg.add_line("Busses");
    l.number = "42".into();
    let stop = |id: i64, name: &str| lines::StopRef { id, name: name.into(), ..Default::default() };
    let leg = || lines::Leg { length: 300.0, ok: true, steps: vec![Default::default()], ..Default::default() };
    l.directions = vec![
        lines::Direction { terminus: "Marktplatz".into(), stops: vec![stop(1, "Bauernhof"), stop(2, "Kirche"), stop(3, "Marktplatz")], legs: vec![leg(), leg()], ..Default::default() },
        lines::Direction { stops: vec![stop(4, "Marktplatz"), stop(5, "Kirche"), stop(6, "Krankenhaus")], legs: vec![leg(), leg()], ..Default::default() },
    ];
    for d in &mut l.directions {
        d.refresh_times();
    }
    let groups = [("busses".to_string(), "Dorf".to_string())].into_iter().collect();
    let depot = linehof::Depot::from_text(Path::new("Dorf.hof"), HOF, "Dorf");
    let mut taken = linehof::Taken::default();
    taken.add(&depot.hof);
    linehof::assign(&mut reg, &groups, "Dorf", &depot, &taken);
    let all: Vec<&lines::LineDesign> = reg.lines.iter().collect();
    let text = linehof::put_block(HOF, "Dorf", &linehof::block(&all, &depot));
    let hof = omsi_vehicle::Hof::parse(&omsi_cfg::CfgFile::from_str("Dorf.hof", &text));
    // the trips as the line editor writes them into the timetable
    let e = lines::export(&reg, &[(0, 0)], &Default::default(), &Default::default()).unwrap();
    for (k, (file, code, route)) in [("oo_42_a.ttp", 901, 4201u32), ("oo_42_b.ttp", 105, 4202)].into_iter().enumerate() {
        let t = omsi_timetable::Trip::parse(&omsi_cfg::CfgFile::from_str(file, &e.files.iter().find(|f| f.0 == file).unwrap().1));
        let names: Vec<String> = reg.lines[0].directions[k].stops.iter().map(|s| s.name.clone()).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        // the AI bus's matrix: the trip's terminus is in the depot file
        let ti = find_terminus(&hof, &t.terminus).expect(file);
        assert_eq!(hof.termini[ti].code, code, "{file}");
        assert_eq!(routes_to(&hof, &t.line, &[code]).len(), 1, "{file}");
        // the player's IBIS: line 42, the route of the trip, at its stop "Kirche"
        let (codes, _) = ibis_codes(&hof, &t.line, &t.terminus, &names).expect(file);
        assert_eq!((codes.line, codes.route), (Some(4200), Some(route % 100)), "{file}");
        let target = ibis_target(&hof, &t.line, &t.terminus, &names, Some((1, "Kirche"))).expect(file);
        let ri = target.route_index.expect(file) as usize;
        assert_eq!(hof.info_trips[ri].code, route.to_string());
        assert_eq!(target.terminus_index as usize, ti);
        assert_eq!(target.stop, 1);
        // a typed route code (the IBIS script's GetRouteIndex): its route, its terminus
        // (GetRouteTerminusIndex) and its stops by their names (GetBusstopIndex)
        let typed = hof.info_trips.iter().position(|x| omsi_cfg::parse_i32(&x.code) == route as i32).unwrap();
        assert_eq!(hof.termini.iter().position(|x| x.code == omsi_cfg::parse_i32(&hof.info_trips[typed].route)), Some(ti));
        for ident in &hof.info_busstop_lists[typed] {
            let b = hof.bus_stops.iter().find(|b| b.ident.trim().eq_ignore_ascii_case(ident.trim())).expect(ident);
            assert!(!b.strings[0].trim().is_empty(), "{ident} has an IBIS name");
        }
    }
    // the new destination's texts are made as the file writes its own: the IBIS's, and the
    // front's (its second row empty for a name of one)
    let t = &hof.termini[find_terminus(&hof, "Marktplatz").unwrap()];
    assert_eq!(t.strings, vec!["MARKTPLATZ", "   MARKTPLATZ", ""], "centred in 16 as the file centres its front, and once");
}
