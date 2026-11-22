//! The NF6D tram (Steam add-on "Strassenbahn NF6D Essen/Gelsenkirchen"): twelve parts, a
//! driver's cab at each end, the two talking through control cables.

use super::*;

/// The tram as the game couples it: `NF6D_A` and what its `[couple_back]` chain names
/// behind it, the rear cab turned round.
fn nf6d() -> VehicleInstance {
    let root = original_root();
    let dir = root.join("Vehicles/Tram_NF6D");
    require_content(&[
        &dir.join("NF6D_A.bus"),
        &dir.join("NF6D_B_User.bus"),
        &dir.join("Kbergbahn.hof"),
    ]);
    let load = |f: &str| {
        Arc::new(VehicleType::load(&root, &dir.join(f)).unwrap_or_else(|e| panic!("{f}: {e:#}")))
    };
    let mut host = VehicleHost::new(crate::SimClock::default());
    host.hof = omsi_vehicle::Hof::load(&dir.join("Kbergbahn.hof"))
        .ok()
        .map(Arc::new);
    let mut v = VehicleInstance::new(load("NF6D_A.bus"), host);
    for part in [
        "NF6D_Gelenk_ACV.bus",
        "NF6D_Gelenk_ACH.bus",
        "NF6D_C.bus",
        "NF6D_Gelenk_BCV.bus",
        "NF6D_Gelenk_BCH.bus",
        "NF6D_B.bus",
        "NF6D_DrehgestellV.bus",
        "NF6D_EEF_V.bus",
        "NF6D_EEF_H.bus",
        "NF6D_DrehgestellH.bus",
    ] {
        v.attach_trailer_ex(load(part), false);
    }
    v.attach_trailer_ex(load("NF6D_B_User.bus"), true);
    v
}

fn frames(v: &mut VehicleInstance, n: usize) {
    for _ in 0..n {
        v.update(0.02);
    }
}

/// The part with the rear cab (`NF6D_B_User`).
fn rear_cab(v: &VehicleInstance) -> &TrailerPart {
    v.trailers.last().unwrap()
}

fn own_str(t: &TrailerPart, name: &str) -> String {
    t.run
        .as_deref()
        .and_then(|r| r.get_str(name))
        .unwrap_or_default()
        .to_string()
}

/// Switched on (E, M) and driven from the cab given (`rear`: the rear cab's, D there).
fn switched_on(rear: bool) -> VehicleInstance {
    let mut v = nf6d();
    frames(&mut v, 10);
    if rear {
        v.active_cab = Some(v.trailers.len() - 1);
    }
    v.trigger("cp_batterietrennschalter_toggle");
    frames(&mut v, 50);
    v.trigger("kw_M_enginestart");
    frames(&mut v, 400);
    v.trigger("automatic_D");
    frames(&mut v, 20);
    v
}

/// A destination picked from openOMSI's list (as `game_lists` does it) reaches both cabs'
/// matrices: the head cab's own (its rear cab used to zero the cable it reads in its
/// {init}) and the rear cab's, line number included.
#[test]
#[ignore = "needs an OMSI 2 install with the NF6D add-on (OMSI_ROOT)"]
fn a_destination_picked_by_hand_reaches_both_cabs() {
    let mut v = switched_on(false);
    assert_eq!(v.var("A_Kopf_Drive"), Some(1.0), "the head cab drives");
    let hof = v.host.hof.clone().unwrap();
    crate::timetable_run::set_player_destination_at(&mut v, &hof, "50", 1, &[]);
    frames(&mut v, 200);
    let want = hof.termini[1].strings[0].trim().to_string();
    assert!(!want.is_empty());
    assert_eq!(v.var("SLR_IBIS_TerminusIndex_A"), Some(1.0));
    assert_eq!(
        v.str_var("Matrix_TerminusL1").trim(),
        want,
        "the head cab's matrix"
    );
    let b = rear_cab(&v);
    assert_eq!(b.run_var("SLR_IBIS_TerminusIndex_A"), Some(1.0));
    assert_eq!(
        own_str(b, "Matrix_TerminusL1").trim(),
        want,
        "the rear cab's matrix"
    );
    assert_eq!(
        b.run_var("SLR_IBIS_Linie_Complex_A"),
        v.var("IBIS_Linie_Complex"),
        "the line goes down the cables"
    );
    assert!(
        own_str(b, "Matrix_Nr").contains("50"),
        "the rear cab's line number: {:?}",
        own_str(b, "Matrix_Nr")
    );
    assert!(
        !b.script_textures.is_empty(),
        "the rear cab has a matrix of its own"
    );
    assert!(
        b.script_textures
            .iter()
            .any(|t| t.rgba.iter().any(|&x| x != 0)),
        "and its scripts drew into it"
    );
}

/// The doors opened from the head cab open in the middle part and the rear section too, for
/// the people and the player on foot: each part's own `PAX_Entry<n>_Open`, not the head
/// car's by the train's count.
#[test]
#[ignore = "needs an OMSI 2 install with the NF6D add-on (OMSI_ROOT)"]
fn the_doors_of_every_section_open_for_the_people() {
    let mut v = switched_on(false);
    let cabin = crate::people::cabin::Cabin::load_train(&crate::people::cabin::train_parts(&v))
        .expect("the tram's cabin");
    assert!(
        cabin.parts.len() >= 4,
        "the sections joined: {}",
        cabin.parts.len()
    );
    let sections: Vec<usize> = cabin.entries.iter().map(|d| d.part.0).collect();
    assert!(
        sections.iter().any(|&p| p > 0),
        "doors behind the head car: {sections:?}"
    );
    let (shut, _) = crate::people::PeopleSim::doors_open_cabin(&v, &cabin);
    assert!(shut.iter().all(|o| !o), "all shut at first: {shut:?}");
    // the release, then the right-hand doors open (the cab's own keys)
    v.trigger("bus_dooraft");
    frames(&mut v, 5);
    v.trigger("bus_doorfront0");
    frames(&mut v, 300);
    let (open, _) = crate::people::PeopleSim::doors_open_cabin(&v, &cabin);
    let open_in = |part: usize| {
        cabin
            .entries
            .iter()
            .zip(&open)
            .any(|(d, o)| d.part.0 == part && *o)
    };
    assert!(open_in(0), "a head car door: {open:?} {sections:?}");
    let behind: Vec<usize> = sections.iter().copied().filter(|&p| p > 0).collect();
    assert!(
        behind.iter().any(|&p| open_in(p)),
        "a door behind the head car: {open:?} {sections:?}"
    );
}

/// Every door of the tram the passengers use - entries and exits, all sections - opens for
/// them on its own side: the right-hand doors with the right preselected, the left-hand
/// ones with the left (`kw_s_2`).
#[test]
#[ignore = "needs an OMSI 2 install with the NF6D add-on (OMSI_ROOT)"]
fn every_door_opens_for_the_people_on_its_side() {
    for rear in [false, true] {
        doors_open_on_their_side(rear);
    }
}

/// [`every_door_opens_for_the_people_on_its_side`] driven from the head cab or the rear one
/// (`rear`: its own right hand is the tram's left).
fn doors_open_on_their_side(rear: bool) {
    let open_with = |left: bool| {
        let mut v = switched_on(rear);
        if left {
            v.trigger("kw_s_2");
            frames(&mut v, 5);
        }
        v.trigger("bus_dooraft");
        frames(&mut v, 5);
        v.trigger("bus_doorfront0");
        frames(&mut v, 300);
        let cabin = crate::people::cabin::Cabin::load_train(&crate::people::cabin::train_parts(&v))
            .unwrap();
        let (e, x) = crate::people::PeopleSim::doors_open_cabin(&v, &cabin);
        (cabin, e, x)
    };
    let (cabin, right_e, right_x) = open_with(false);
    let (_, left_e, left_x) = open_with(true);
    let mut shut = Vec::new();
    for (kind, doors, r, l) in [
        ("entry", &cabin.entries, &right_e, &left_e),
        ("exit", &cabin.exits, &right_x, &left_x),
    ] {
        for (i, d) in doors.iter().enumerate() {
            let (right, left) = (r[i], l[i]);
            eprintln!(
                "rear cab {rear}: {kind} {i}: section {} door {} side {:+} -> driver's right {right} left {left}",
                d.part.0, d.part.1, d.side
            );
            // (the driver's right hand is the tram's from the head cab, its left from the rear)
            let own_side = if (d.side > 0.0) != rear { right } else { left };
            if !own_side {
                shut.push(format!(
                    "{kind} {i} (section {}, door {}, side {:+})",
                    d.part.0, d.part.1, d.side
                ));
            }
        }
    }
    assert!(
        shut.is_empty(),
        "doors that do not open on their own side: {shut:?}"
    );
}

/// Driven from the rear cab, a destination picked from the list shows on the head cab's
/// matrix as well (it reads the rear cab's destination cable then).
#[test]
#[ignore = "needs an OMSI 2 install with the NF6D add-on (OMSI_ROOT)"]
fn a_destination_picked_in_the_rear_cab_shows_at_both_ends() {
    let mut v = switched_on(true);
    let hof = v.host.hof.clone().unwrap();
    crate::timetable_run::set_player_destination_at(&mut v, &hof, "50", 1, &[]);
    frames(&mut v, 200);
    let want = hof.termini[1].strings[0].trim().to_string();
    let b = rear_cab(&v);
    assert_eq!(
        own_str(b, "Matrix_TerminusL1").trim(),
        want,
        "the rear cab's matrix"
    );
    assert_eq!(
        v.str_var("Matrix_TerminusL1").trim(),
        want,
        "the head cab's matrix"
    );
}

/// A line typed on the rear cab's own IBIS keys goes out on its line cable and reaches the
/// matrices of every section: the head cab's, the middle section's and the rear cab's own
/// (it used to reach the head cab only).
#[test]
#[ignore = "needs an OMSI 2 install with the NF6D add-on (OMSI_ROOT)"]
fn a_line_typed_on_the_rear_cabs_ibis_shows_on_every_section() {
    let mut v = switched_on(true);
    for key in [
        "IBIS_setmode_linie_kurs",
        "IBIS_5",
        "IBIS_0",
        "IBIS_0",
        "IBIS_1",
        "IBIS_Eingabe",
    ] {
        assert!(v.trigger(key), "{key}");
        frames(&mut v, 5);
    }
    frames(&mut v, 200);
    let b = rear_cab(&v);
    let c = &v.trailers[2];
    let typed = b.run_var("SLW_IBIS_Linie_Complex_B");
    eprintln!(
        "typed {typed:?}; heard: A {:?}, C {:?}, B {:?}; matrix numbers: A {:?}, C {:?}, B {:?}",
        v.var("SLR_IBIS_Linie_Complex_B"),
        c.run_var("SLR_IBIS_Linie_Complex_B"),
        b.run_var("SLR_IBIS_Linie_Complex_B"),
        v.var("Matrix_Nr_Last"),
        c.run_var("Matrix_Nr_Last"),
        b.run_var("Matrix_Nr_Last"),
    );
    assert_eq!(typed, Some(5001.0), "the rear cab's IBIS took the line");
    assert_eq!(
        v.var("SLR_IBIS_Linie_Complex_B"),
        typed,
        "the head cab hears it"
    );
    assert_eq!(
        c.run_var("SLR_IBIS_Linie_Complex_B"),
        typed,
        "the middle section hears it"
    );
    assert_eq!(
        b.run_var("SLR_IBIS_Linie_Complex_B"),
        typed,
        "the rear cab's own matrix hears it"
    );
}

/// The train that begins with `first`, by its files' `[couple_back]` chain as the game
/// follows it.
fn train(first: &str) -> (VehicleInstance, Vec<String>) {
    train_in("Tram_NF6D", first, Some("Kbergbahn.hof"))
}

fn train_in(folder: &str, first: &str, hof: Option<&str>) -> (VehicleInstance, Vec<String>) {
    let root = original_root();
    let dir = root.join("Vehicles").join(folder);
    require_content(&[&dir.join(first)]);
    let mut host = VehicleHost::new(crate::SimClock::default());
    host.hof = hof
        .and_then(|h| omsi_vehicle::Hof::load(&dir.join(h)).ok())
        .map(Arc::new);
    let lead = Arc::new(VehicleType::load(&root, &dir.join(first)).unwrap());
    let mut names = vec![first.to_string()];
    let mut seen = vec![lead.def.path.clone()];
    let (mut last, mut rev) = (lead.clone(), false);
    let mut v = VehicleInstance::new(lead, host);
    for _ in 0..omsi_vehicle::vehicle::MAX_COUPLED_PARTS {
        let Some((path, reversed)) = omsi_vehicle::vehicle::next_coupled(&last.def, rev, true)
        else {
            break;
        };
        if omsi_vehicle::vehicle::chain_has_part(&seen, &path) {
            break;
        }
        let part = Arc::new(VehicleType::load(&root, &path).unwrap());
        v.attach_trailer_ex(part.clone(), reversed);
        names.push(path.file_name().unwrap().to_string_lossy().to_string());
        seen.push(path);
        (last, rev) = (part, reversed);
    }
    (v, names)
}

/// The cabs of a train, the parts with a driver's direction switch (None: the head car), and
/// the cab's letter in the NF6D's variable names.
fn cabs(v: &VehicleInstance) -> Vec<(Option<usize>, char)> {
    let mut out = vec![(None, 'A')];
    for (k, t) in v.trailers.iter().enumerate() {
        if t.run
            .as_deref()
            .is_some_and(|r| r.has_trigger("automatic_D"))
        {
            out.push((
                Some(k),
                if t.run_var("B_Kopf_Drive").is_some() && t.ty.program.var("B_Kopf_Drive").is_some()
                {
                    'B'
                } else {
                    'A'
                },
            ));
        }
    }
    out
}

fn cab_var(v: &VehicleInstance, cab: Option<usize>, name: &str) -> Option<f32> {
    match cab {
        Some(k) => v.trailers[k].run_var(name),
        None => v.var(name),
    }
}

/// As the game plays it: the head cab switched on and given a line, the driver walks to the
/// rear cab, takes the head cab out of service and that one into it, and types a new line.
#[test]
#[ignore = "needs an OMSI 2 install with the NF6D add-on (OMSI_ROOT)"]
fn changing_cabs_and_typing_a_line_shows_it_at_both_ends() {
    let mut v = switched_on(false);
    let line = |v: &mut VehicleInstance, digits: &str| {
        for key in ["IBIS_setmode_linie_kurs".to_string()]
            .into_iter()
            .chain(digits.chars().map(|c| format!("IBIS_{c}")))
            .chain(["IBIS_Eingabe".to_string()])
        {
            assert!(v.trigger(&key), "{key}");
            frames(v, 5);
        }
        frames(v, 200);
    };
    line(&mut v, "5001");
    assert_eq!(v.str_var("Matrix_Nr").trim(), "E50");
    assert_eq!(own_str(rear_cab(&v), "Matrix_Nr").trim(), "E50");
    v.trigger("kw_s_R");
    frames(&mut v, 50);
    v.active_cab = Some(v.trailers.len() - 1);
    v.trigger("kw_s_N");
    frames(&mut v, 20);
    v.trigger("automatic_D");
    frames(&mut v, 50);
    assert_eq!(
        rear_cab(&v).run_var("B_Kopf_Drive"),
        Some(1.0),
        "the rear cab drives"
    );
    line(&mut v, "3200");
    assert_eq!(
        rear_cab(&v).run_var("SLR_IBIS_Linie_Complex_B"),
        Some(3200.0)
    );
    assert_eq!(
        v.str_var("Matrix_Nr").trim(),
        own_str(rear_cab(&v), "Matrix_Nr").trim(),
        "both ends show one line"
    );
    assert!(
        own_str(rear_cab(&v), "Matrix_Nr").contains("32"),
        "{:?}",
        own_str(rear_cab(&v), "Matrix_Nr")
    );
    let picture = |t: &crate::scripttex::ScriptTexture| {
        (0..t.height as i32)
            .flat_map(|y| (0..t.width as i32).map(move |x| (x, y)))
            .filter(|&(x, y)| t.get(x, y)[0] > 40)
            .count()
    };
    let (head, rear) = (
        picture(&v.host.script_textures[1]),
        picture(&rear_cab(&v).script_textures[1]),
    );
    assert!(
        head > 0 && head == rear,
        "the same picture at both ends: {head} / {rear}"
    );
}

/// Two NF6Ds in one train, four cabs: it starts and drives from each cab at either end of
/// each unit that has one.
#[test]
#[ignore = "needs an OMSI 2 install with the NF6D add-on (OMSI_ROOT)"]
fn the_coupled_nf6d_drives_from_its_cabs() {
    let (mut probe, names) = train("NF6D_A_DT.bus");
    eprintln!("{} parts: {names:?}", names.len());
    frames(&mut probe, 2);
    let all = cabs(&probe);
    eprintln!("cabs {all:?}");
    drop(probe);
    for (cab, end) in all {
        let (mut v, _) = train("NF6D_A_DT.bus");
        frames(&mut v, 10);
        v.active_cab = cab;
        v.trigger("cp_batterietrennschalter_toggle");
        frames(&mut v, 50);
        v.trigger("kw_M_enginestart");
        frames(&mut v, 400);
        v.trigger("automatic_D");
        frames(&mut v, 30);
        v.set_rail_env(Some(crate::vehicle::RailEnv {
            grade: 0.0,
            radius: f32::INFINITY,
            in_tunnel: false,
            wire: None,
            third_rail: None,
        }));
        v.physics.controls.throttle = 1.0;
        frames(&mut v, 600);
        let drive = cab_var(&v, cab, &format!("{end}_Kopf_Drive"));
        let top = v.physics.velocity_kmh().abs();
        eprintln!(
            "cab {cab:?} ({end}): drive {drive:?} bat {:?} A_in {:?} B_in {:?} inaktiv-me {:?} gas {:?} -> {top:.1} km/h",
            cab_var(&v, cab, "SLAR_Elec_Bat"),
            cab_var(&v, cab, "SLR_A_Kopf_Inaktiv"),
            cab_var(&v, cab, "SLR_B_Kopf_Inaktiv"),
            cab_var(&v, cab, &format!("SLW_{end}_Kopf_Inaktiv")),
            cab_var(&v, cab, &format!("SLR_{end}_Kopf_Gas")),
        );
        assert_eq!(drive, Some(1.0), "cab {cab:?} takes D");
        assert!(top > 5.0, "cab {cab:?}: {top:.1} km/h");
    }
}
/// Full throttle on straight, level track for 20 s, then full service brake: as fast from
/// either cab (the rear cab stepped its throttle up by the head car's speed, negative when
/// driven from it), with both cab cars' motors and brakes (only the head car's counted).
#[test]
#[ignore = "needs an OMSI 2 install with the NF6D add-on (OMSI_ROOT)"]
fn both_cabs_pull_and_brake_alike() {
    let run = |rear: bool| {
        let mut v = switched_on(rear);
        v.set_rail_env(Some(crate::vehicle::RailEnv {
            grade: 0.0,
            radius: f32::INFINITY,
            in_tunnel: false,
            wire: None,
            third_rail: None,
        }));
        frames(&mut v, 600);
        v.physics.controls.throttle = 1.0;
        frames(&mut v, 1000);
        let top = v.physics.velocity_kmh().abs();
        let gas = if rear {
            rear_cab(&v).run_var("SLR_B_Kopf_Gas")
        } else {
            v.var("SLR_A_Kopf_Gas")
        };
        v.physics.controls.throttle = 0.0;
        v.physics.controls.brake = 1.0;
        let mut t = 0;
        while v.physics.velocity_kmh().abs() > 0.5 && t < 3000 {
            frames(&mut v, 1);
            t += 1;
        }
        eprintln!(
            "rear {rear}: {top:.1} km/h after 20 s (gas {gas:?}), stopped in {:.1} s",
            t as f32 * 0.02
        );
        (top, t as f32 * 0.02, gas)
    };
    let (a_top, a_stop, _) = run(false);
    let (b_top, b_stop, b_gas) = run(true);
    assert_eq!(
        b_gas,
        Some(1.0),
        "the rear cab's throttle opens all the way"
    );
    assert!(
        (a_top - b_top).abs() < a_top * 0.15,
        "as fast from either cab: {a_top:.1} / {b_top:.1} km/h"
    );
    assert!(a_top > 30.0, "{a_top:.1} km/h after 20 s at full throttle");
    assert!(
        a_stop < 12.0 && b_stop < 12.0,
        "stopped in {a_stop:.1} / {b_stop:.1} s"
    );
}

/// In the rear cab a key bound to a head-cab control works the rear cab's own one where the
/// tram names it with its cab's letter (`cp_venta` / `cp_ventb`), not the head cab's; one
/// the rear cab has no such twin of (the battery switch, the tram's own) stays the head's.
#[test]
#[ignore = "needs an OMSI 2 install with the NF6D add-on (OMSI_ROOT)"]
fn the_rear_cab_works_its_own_twin_of_a_head_cab_control() {
    let mut v = nf6d();
    frames(&mut v, 5);
    let b = v.trailers.len() - 1;
    assert_eq!(
        v.cab_twin_trigger(b, "cp_venta").as_deref(),
        Some("cp_ventb")
    );
    assert_eq!(
        v.cab_twin_trigger(b, "tuerhw_a").as_deref(),
        Some("tuerhw_b")
    );
    assert_eq!(
        v.cab_twin_trigger(b, "spiegel_al_init").as_deref(),
        Some("spiegel_bl_init")
    );
    assert_eq!(
        v.cab_twin_trigger(b, "cp_batterietrennschalter_toggle"),
        None
    );
    v.active_cab = Some(b);
    assert!(v.trigger("cp_venta"), "the rear cab's fan switch");
}

/// Driven from the rear cab: it is the active one, the head cab is not.
#[test]
#[ignore = "needs an OMSI 2 install with the NF6D add-on (OMSI_ROOT)"]
fn the_rear_cab_drives_with_its_own_keys() {
    let v = switched_on(true);
    let b = rear_cab(&v);
    assert_eq!(b.run_var("B_Kopf_Drive"), Some(1.0), "the rear cab drives");
    assert_eq!(v.var("A_Kopf_Drive"), Some(0.0), "the head cab does not");
}
