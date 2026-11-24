use super::*;

#[test]
fn a_library_beside_its_opl_in_a_subfolder_is_found() {
    let dir = std::env::temp_dir().join(format!("omsi-opl-lib-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Pablo")).unwrap();
    for f in ["Pablo/MBASNE.dll", "top.dll"] {
        std::fs::write(dir.join(f), b"x").unwrap();
    }
    let opl = dir.join("Pablo/MBAS.opl");
    assert_eq!(
        library_path(&opl, &dir, "MBASNE.dll"),
        Some(dir.join("Pablo/MBASNE.dll"))
    );
    assert_eq!(
        library_path(&opl, &dir, "top.dll"),
        Some(dir.join("top.dll"))
    );
    assert_eq!(library_path(&opl, &dir, "none.dll"), None);
    let other = std::env::temp_dir().join(format!("omsi-opl-lib-b-{}", std::process::id()));
    std::fs::create_dir_all(&other).unwrap();
    for (f, dll) in [
        (&opl, "MBASNE.dll"),
        (&other.join("mbas.opl"), "x\\MBASNE.dll"),
        (&dir.join("Pablo/Config.opl"), "a.dll"),
        (&other.join("Config.opl"), "b.dll"),
    ] {
        std::fs::write(f, format!("[dll]\r\n{dll}\r\n")).unwrap();
    }
    let mut found: Vec<PathBuf> = plugin_opls(&[other.clone(), dir.clone()])
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    found.sort();
    assert_eq!(
        found,
        vec![
            dir.join("Pablo/Config.opl"),
            other.join("Config.opl"),
            other.join("mbas.opl")
        ],
        "a later folder's copy of the same plugin is not loaded again"
    );
    let _ = (
        std::fs::remove_dir_all(&dir),
        std::fs::remove_dir_all(&other),
    );
}

#[test]
fn opl_lists() {
    let o = parse_opl(
        "[dll]\r\nsub\\my.dll\r\n\r\n[varlist]\r\n2\r\nVelocity\r\nthrottle\r\n[systemvarlist]\r\n1\r\nTime\r\n[stringvarlist]\r\n1\r\nIBIS_terminus_name\r\n[triggers]\r\n1\r\nbus_doorfront0\r\n[scripttexturelist]\r\n2\r\n0\r\n3\r\n",
    );
    assert_eq!(o.dll, "sub\\my.dll");
    assert_eq!(o.vars, ["Velocity", "throttle"]);
    assert_eq!(o.system_vars, ["Time"]);
    assert_eq!(o.string_vars, ["IBIS_terminus_name"]);
    assert_eq!(o.triggers, ["bus_doorfront0"]);
    assert_eq!(o.script_textures, ["0", "3"]);
}

#[test]
fn wire_round_trip() {
    let f = Frame {
        system: vec![(0, 1.5)],
        vars: vec![(1, 2.0), (3, -1.0)],
        strings: vec![(0, "Zoo €".into())],
        triggers: vec![0, 2],
        ..Default::default()
    };
    let mut buf = Vec::new();
    wire::put_frame(&mut buf, &f, false, false).unwrap();
    assert_eq!(
        wire::get_frame(&mut buf.as_slice(), false, false).unwrap(),
        f
    );
    let r = Reply {
        system: vec![None],
        vars: vec![Some(3.0), None],
        strings: vec![Some("ab".into())],
        triggers_active: vec![true, false],
        textures: vec![],
        tickets: vec![],
    };
    let mut buf = Vec::new();
    wire::put_reply(&mut buf, &r, false).unwrap();
    assert_eq!(wire::get_reply(&mut buf.as_slice(), &f, false).unwrap(), r);
}

#[test]
fn wire_round_trip_textures() {
    let f = Frame {
        vars: vec![(0, 1.0)],
        textures: vec![(0, 2, 1), (1, 4, 4)],
        pointer: vec![(1, POINTER_DOWN, 1.5, 2.25), (1, POINTER_UP, 3.0, 0.0)],
        ..Default::default()
    };
    let mut buf = Vec::new();
    wire::put_frame(&mut buf, &f, true, true).unwrap();
    assert_eq!(wire::get_frame(&mut buf.as_slice(), true, true).unwrap(), f);
    // without the pointer procedure the events stay off the wire
    let mut buf = Vec::new();
    wire::put_frame(&mut buf, &f, true, false).unwrap();
    assert_eq!(
        wire::get_frame(&mut buf.as_slice(), true, false).unwrap(),
        Frame {
            pointer: vec![],
            ..f.clone()
        }
    );
    let r = Reply {
        vars: vec![None],
        textures: vec![Some(vec![1, 2, 3, 4, 5, 6, 7, 8]), None],
        ..Default::default()
    };
    let mut buf = Vec::new();
    wire::put_reply(&mut buf, &r, true).unwrap();
    assert_eq!(wire::get_reply(&mut buf.as_slice(), &f, true).unwrap(), r);
    // a picture of another size than asked is refused, not believed
    let bad = Reply {
        vars: vec![None],
        textures: vec![Some(vec![0; 4]), None],
        ..Default::default()
    };
    let mut buf = Vec::new();
    wire::put_reply(&mut buf, &bad, true).unwrap();
    assert!(wire::get_reply(&mut buf.as_slice(), &f, true).is_err());
}

#[test]
fn pointer_events_for_listened_textures_only() {
    let mut tp = TexturePointer {
        pages: vec![2],
        ..Default::default()
    };
    assert!(
        !tp.push(0, POINTER_DOWN, 0.1, 0.1),
        "no plugin listens to texture 0"
    );
    assert!(tp.push(2, POINTER_DOWN, 0.1, 0.2));
    assert!(tp.push(2, POINTER_MOVE, 0.3, 0.3));
    assert!(tp.push(2, POINTER_MOVE, 0.4, 0.4));
    assert!(tp.push(2, POINTER_UP, 0.5, 0.5));
    // the second move replaced the first one
    assert_eq!(
        tp.pending,
        [
            (2, POINTER_DOWN, 0.1, 0.2),
            (2, POINTER_MOVE, 0.4, 0.4),
            (2, POINTER_UP, 0.5, 0.5)
        ]
    );
}

#[test]
fn wire_round_trip_tickets() {
    let calls = vec![
        TicketCall {
            person: 7,
            age: 34.0,
            stop: "Střížkov".into(),
            destination: "Nový Bor, aut.st.".into(),
            stop_number: 1,
            destination_number: 12,
            kind: 2,
            ticket: 1,
            price: 30.0,
            name: "Plnocenná".into(),
        },
        TicketCall {
            person: 9,
            age: 10.0,
            ..Default::default()
        },
    ];
    let mut buf = Vec::new();
    wire::put_tickets(&mut buf, &calls).unwrap();
    assert_eq!(wire::get_tickets(&mut buf.as_slice()).unwrap(), calls);
    let answers = vec![
        Some(TicketAnswer {
            kind: 2,
            ticket: 1,
            price: 64.0,
            name: "IDOL 4001".into(),
            voice: "Zone_4001".into(),
        }),
        None,
    ];
    let mut buf = Vec::new();
    wire::put_ticket_answers(&mut buf, &answers).unwrap();
    assert_eq!(
        wire::get_ticket_answers(&mut buf.as_slice(), 2).unwrap(),
        answers
    );
}

#[test]
fn ticket_answer_changes_only_what_differs() {
    let call = TicketCall {
        person: 7,
        kind: 2,
        ticket: 1,
        price: 30.0,
        name: "Plnocenná".into(),
        ..Default::default()
    };
    let same = TicketAnswer {
        kind: 2,
        ticket: 1,
        price: 30.0,
        name: "Plnocenná".into(),
        voice: String::new(),
    };
    assert_eq!(call.change(&same), TicketChange::default());
    let other = TicketAnswer {
        kind: 0,
        ticket: 3,
        price: 64.0,
        name: "IDOL".into(),
        voice: "Zone_4001".into(),
    };
    let c = call.change(&other);
    assert_eq!(c.kind, Some(TicketKind::None));
    assert_eq!(c.ticket, Some(3));
    assert_eq!(c.price, Some(64.0));
    assert_eq!(c.name.as_deref(), Some("IDOL"));
    assert_eq!(c.voice.as_deref(), Some("Zone_4001"));
    // (a kind or ticket number out of range is left as it was)
    let bad = TicketAnswer {
        kind: 7,
        ticket: -1,
        ..same
    };
    assert_eq!(call.change(&bad), TicketChange::default());
}

#[test]
fn omsi_exe_patchers_left_out() {
    assert!(patches_omsi_exe(Path::new(
        "plugins/OMSIPresentationTools.dll"
    )));
    assert!(!patches_omsi_exe(Path::new("plugins/Demo/demo.dll")));
}

#[test]
fn procs_flags() {
    for flags in 0..128u8 {
        assert_eq!(Procs::from_flags(flags).flags(), flags);
    }
}
