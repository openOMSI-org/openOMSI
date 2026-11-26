//! Tests of `car_run`: the cars of a train running their own scripts.

use super::*;

/// Lamp variables of the test car, in the order of its lamps: front white, front red,
/// rear white, rear red.
const LAMPS: [&str; 4] = [
    "lamp_front_white",
    "lamp_front_red",
    "lamp_rear_white",
    "lamp_rear_red",
];

/// A multiple-unit car: each free end shows white where the car leads and red where it
/// trails, from its own place in the train.
fn unit_car() -> Arc<VehicleType> {
    static MADE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("omsi-car-run-test-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("cab_ends.osc");
    let list = dir.join("cab_ends_varlist.txt");
    std::fs::write(
        &script,
        "{frame}\n\
         (L.L.train_me_reverse) (S.L.turned)\n\
         0 (S.L.lamp_front_white) (S.L.lamp_front_red) (S.L.lamp_rear_white) (S.L.lamp_rear_red)\n\
         (L.L.train_frontcoupling) 0 = {if}\n\
           (L.L.turned) {if} 1 (S.L.lamp_front_red) {else} 1 (S.L.lamp_front_white) {endif}\n\
         {endif}\n\
         (L.L.train_backcoupling) 0 = {if}\n\
           (L.L.turned) {if} 1 (S.L.lamp_rear_white) {else} 1 (S.L.lamp_rear_red) {endif}\n\
         {endif}\n\
         {end}\n",
    )
    .unwrap();
    let mut names = PLACE_IN_TRAIN.join("\n");
    for extra in ["turned"].iter().chain(&LAMPS) {
        names.push('\n');
        names.push_str(extra);
    }
    std::fs::write(&list, names).unwrap();
    let program = omsi_script::compile(&CompileInput {
        varlists: vec![list],
        scripts: vec![script],
        ..Default::default()
    });
    std::fs::remove_dir_all(&dir).ok();
    assert!(program.errors.is_empty(), "{:?}", program.errors);
    let mut ty = super::super::tests::coupling_test_type(Some(-4.0));
    let t = Arc::get_mut(&mut ty).unwrap();
    t.program = Arc::new(program);
    let lamp_at = |i: usize| omsi_model::LightEnh2 {
        pos: [0.4, if i < 2 { 7.5 } else { -7.5 }, 1.2],
        omni: true,
        color: if i % 2 == 0 {
            [250.0, 250.0, 240.0]
        } else {
            [250.0, 20.0, 10.0]
        },
        size: 0.15,
        factor: 1.0,
        variable: LAMPS[i].to_string(),
        ..Default::default()
    };
    t.model.meshes = vec![omsi_model::MeshDef {
        light_enh_2: (0..4).map(lamp_at).collect(),
        ..Default::default()
    }];
    ty
}

fn run_frames(v: &mut VehicleInstance, n: usize) {
    for _ in 0..n {
        v.update_dirt(0.0);
        v.update_scripts_only(0.0);
        v.update_visuals(0.04);
    }
}

/// Which of a car's four lamps are lit, as `LAMPS` orders them.
fn lit(fades: &[f32]) -> [bool; 4] {
    std::array::from_fn(|k| fades.get(k).is_some_and(|&f| f > 0.5))
}

/// Four cars, the second and the fourth turned round: white only at the head of the
/// train, red only at its tail, every car between dark. Two cars uncoupled, the
/// second car (turned round) is the tail and shows red at its own front.
#[test]
fn turned_cars_light_the_end_they_stand_at() {
    let car = unit_car();
    let mut v = VehicleInstance::new(
        car.clone(),
        crate::host::VehicleHost::new(Default::default()),
    );
    for turned in [true, false, true] {
        v.attach_trailer_ex(car.clone(), turned);
    }
    run_frames(&mut v, 4);
    assert_eq!(lit(&v.light_fade), [true, false, false, false], "head car");
    for (k, part) in v.trailers.iter().enumerate() {
        assert!(part.run.is_some(), "car {k} runs its own scripts");
    }
    assert_eq!(lit(&v.trailers[0].light_fade), [false; 4]);
    assert_eq!(lit(&v.trailers[1].light_fade), [false; 4]);
    assert_eq!(
        lit(&v.trailers[2].light_fade),
        [false, true, false, false],
        "tail car, turned round"
    );
    assert_eq!(v.trailers[2].run_var("train_backcoupling"), Some(1.0));
    assert_eq!(
        v.var("lamp_front_red"),
        Some(0.0),
        "the head car's values stay its own"
    );
    v.detach_last_trailer();
    v.detach_last_trailer();
    run_frames(&mut v, 4);
    assert_eq!(
        lit(&v.trailers[0].light_fade),
        [false, true, false, false],
        "second car is the tail now"
    );
    assert_eq!(lit(&v.light_fade), [true, false, false, false]);
}

/// A car of the test train with one control cable (`C 7`, read `SLR_x`, write `SLW_x`,
/// link `Link`) at both plugs and the frame script `body`.
fn cable_car(body: &str) -> Arc<VehicleType> {
    cable_car_with(body, "")
}

/// [`cable_car`] with more script after its frame (`more`: triggers).
fn cable_car_with(body: &str, more: &str) -> Arc<VehicleType> {
    static MADE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("omsi-car-cable-test-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("cab.osc");
    let list = dir.join("cab_varlist.txt");
    std::fs::write(&script, format!("{{frame}}\n{body}\n{{end}}\n{more}\n")).unwrap();
    std::fs::write(
        &list,
        "SLR_x\nSLW_x\nLink\nSLR_y\nSLW_y\nLink2\nThrottle\ntrain_me_reverse",
    )
    .unwrap();
    let program = omsi_script::compile(&CompileInput {
        varlists: vec![list],
        scripts: vec![script],
        ..Default::default()
    });
    std::fs::remove_dir_all(&dir).ok();
    assert!(program.errors.is_empty(), "{:?}", program.errors);
    let mut ty = super::super::tests::coupling_test_type(Some(-4.0));
    let t = Arc::get_mut(&mut ty).unwrap();
    t.program = Arc::new(program);
    let cable = omsi_vehicle::vehicle::ControlCable {
        lines: ["C", "7", "SLR_x", "SLW_x", "Link"]
            .map(String::from)
            .to_vec(),
    };
    t.def.control_cable_front.push(cable.clone());
    t.def.control_cable_back.push(cable);
    ty
}

/// A wire number far beyond any plug's is left out, not made room for.
#[test]
fn a_cable_number_too_large_is_left_out() {
    let cable = |n: &str| omsi_vehicle::vehicle::ControlCable {
        lines: ["L", n, "a", "b", "c"].map(String::from).to_vec(),
    };
    let def = omsi_vehicle::Vehicle {
        control_cable_front: vec![
            cable("2"),
            cable("18446744073709551615"),
            cable("4000000000"),
        ],
        ..Default::default()
    };
    let plugs = plugs_of(&def, |_| Some(1));
    assert_eq!(plugs[0][0].len(), 3, "only the wire numbered 2");
    assert!(plugs[0][0][2].read.is_some());
}

/// The second cab at the tail writes a value on a cable; the head car reads it on its own
/// `read` variable.
#[test]
fn what_the_tail_car_writes_reaches_the_head_car() {
    let head = cable_car("1 (S.L.Link)");
    let mid = cable_car("1 (S.L.Link)");
    let tail = cable_car("1 (S.L.Link) 1 (S.L.SLW_x)");
    let mut v = VehicleInstance::new(head, crate::host::VehicleHost::new(Default::default()));
    v.attach_trailer_ex(mid, false);
    v.attach_trailer_ex(tail, true);
    run_frames(&mut v, 8);
    assert_eq!(
        v.trailers[1].run_var("SLW_x"),
        Some(1.0),
        "the tail car writes it"
    );
    assert_eq!(
        v.var("SLW_x"),
        Some(0.0),
        "the head car's own write stays its own"
    );
    assert_eq!(
        v.var("SLR_x"),
        Some(1.0),
        "and the head car reads what the tail wrote"
    );
    assert_eq!(
        v.trailers[0].run_var("SLR_x"),
        Some(1.0),
        "a car between hears the tail too (the NF6D's middle part's doors)"
    );
}

/// Every car reads the largest value any car puts on the wire, its own among them, as
/// Omsi.exe's cables do (a brake line every car's script writes: the driving car's
/// step must reach its own brakes as well as the others').
#[test]
fn a_wire_holds_the_largest_value_on_it() {
    let head = cable_car("1 (S.L.Link) 2 (S.L.SLW_x)");
    let mid = cable_car("1 (S.L.Link)");
    let tail = cable_car("1 (S.L.Link) 1 (S.L.SLW_x)");
    let mut v = VehicleInstance::new(head, crate::host::VehicleHost::new(Default::default()));
    v.attach_trailer_ex(mid, false);
    v.attach_trailer_ex(tail, true);
    run_frames(&mut v, 8);
    assert_eq!(v.var("SLR_x"), Some(2.0));
    assert_eq!(v.trailers[0].run_var("SLR_x"), Some(2.0));
    assert_eq!(v.trailers[1].run_var("SLR_x"), Some(2.0));
}

/// A wire reads from 0: a negative value on it is never read.
#[test]
fn a_wire_reads_no_negative_value() {
    let mut v = VehicleInstance::new(
        cable_car("1 (S.L.Link) -1 (S.L.SLW_x)"),
        crate::host::VehicleHost::new(Default::default()),
    );
    v.attach_trailer_ex(cable_car("1 (S.L.Link)"), false);
    run_frames(&mut v, 4);
    assert_eq!(v.var("SLW_x"), Some(-1.0));
    assert_eq!(v.var("SLR_x"), Some(0.0));
    assert_eq!(v.trailers[0].run_var("SLR_x"), Some(0.0));
}

/// Two coupled wires connect only while the link variable is on at both ends; a car's
/// own write still reaches its own `read` variable.
#[test]
fn a_link_off_at_either_end_cuts_the_wire() {
    let head = cable_car("1 (S.L.Link) 3 (S.L.SLW_x)");
    let tail = cable_car("0 (S.L.Link)");
    let mut v = VehicleInstance::new(head, crate::host::VehicleHost::new(Default::default()));
    v.attach_trailer_ex(tail, false);
    run_frames(&mut v, 4);
    assert_eq!(v.var("SLR_x"), Some(3.0));
    assert_eq!(v.trailers[0].run_var("SLR_x"), Some(0.0));
}

/// Four cabs in a row and a wire each can use: whichever cab writes, all four read it,
/// and the wire falls back when that cab stops. Nothing here counts the cabs.
#[test]
fn any_cab_of_a_long_train_reaches_all_the_others() {
    let cab = |k: usize| {
        cable_car_with(
            "1 (S.L.Link)",
            &format!(
                "{{trigger:send{k}}}\n7 (S.L.SLW_x)\n{{end}}\n{{trigger:stop{k}}}\n0 (S.L.SLW_x)\n{{end}}"
            ),
        )
    };
    let mut v = VehicleInstance::new(cab(0), crate::host::VehicleHost::new(Default::default()));
    for (k, turned) in [(1, true), (2, false), (3, true)] {
        v.attach_trailer_ex(cable_car("1 (S.L.Link)"), false);
        v.attach_trailer_ex(cab(k), turned);
    }
    run_frames(&mut v, 2);
    for (k, part) in [(0, None), (1, Some(1)), (2, Some(3)), (3, Some(5))] {
        v.active_cab = part;
        assert!(v.trigger(&format!("send{k}")));
        run_frames(&mut v, 2);
        assert_eq!(v.var("SLR_x"), Some(7.0), "cab {k} reaches the head");
        for (i, t) in v.trailers.iter().enumerate() {
            assert_eq!(t.run_var("SLR_x"), Some(7.0), "cab {k} reaches part {i}");
        }
        assert!(v.trigger(&format!("stop{k}")));
        run_frames(&mut v, 2);
        assert_eq!(v.trailers[5].run_var("SLR_x"), Some(0.0));
    }
}

/// `L` of a plug meets `R` of the plug coupled to it. Two cars back to back, each
/// writing `SLW_x` and reading `SLR_y`, tell each other their state on two wires.
#[test]
fn left_and_right_wires_cross_at_a_coupling() {
    let car = |body: &str| {
        let mut ty = cable_car(body);
        let t = Arc::get_mut(&mut ty).unwrap();
        t.def.control_cable_front.clear();
        t.def.control_cable_back.clear();
        for (kind, read, write) in [("L", "SLR_x", "SLW_x"), ("R", "SLR_y", "Link2")] {
            t.def
                .control_cable_back
                .push(omsi_vehicle::vehicle::ControlCable {
                    lines: [kind, "0", read, write, "Link"].map(String::from).to_vec(),
                });
        }
        ty
    };
    let mut v = VehicleInstance::new(
        car("1 (S.L.Link) 1 (S.L.SLW_x)"),
        crate::host::VehicleHost::new(Default::default()),
    );
    v.attach_trailer_ex(car("1 (S.L.Link)"), true);
    run_frames(&mut v, 4);
    assert_eq!(
        v.trailers[0].run_var("SLR_y"),
        Some(1.0),
        "the rear car reads the head car"
    );
    assert_eq!(
        v.var("SLR_y"),
        Some(0.0),
        "and the head car the rear car, which sends 0"
    );
    assert_eq!(
        v.trailers[0].run_var("SLR_x"),
        Some(0.0),
        "its own wire has nothing on it"
    );
}

/// What a car's scripts hold is that car's: another car does not see the leading
/// vehicle's value of it (a cockpit switch of the head cab, set by a key). What the
/// engine gives, it does.
#[test]
fn a_cars_script_variables_are_its_own() {
    let head = cable_car_with(
        "(L.L.Throttle) (S.L.Link)",
        "{trigger:flip}\n1 (S.L.SLW_y)\n{end}",
    );
    let tail = cable_car("(L.L.Throttle) (S.L.Link)");
    let mut v = VehicleInstance::new(head, crate::host::VehicleHost::new(Default::default()));
    v.attach_trailer_ex(tail, false);
    run_frames(&mut v, 2);
    assert!(v.trigger("flip"));
    v.set_var("Throttle", 1.0);
    run_frames(&mut v, 3);
    assert_eq!(v.var("SLW_y"), Some(1.0));
    assert_eq!(
        v.trailers[0].run_var("SLW_y"),
        Some(0.0),
        "the head cab's switch stays the head cab's"
    );
    assert_eq!(
        v.trailers[0].run_var("Throttle"),
        Some(1.0),
        "the controls reach every car"
    );
}

/// A `[scriptshare]` part that runs only to read its place in the train still has the
/// leading vehicle's variables, those its scripts hold among them.
#[test]
fn a_scriptshare_part_keeps_the_leading_vehicles_variables() {
    let head = cable_car("5 (S.L.SLW_y)");
    let mut rear = cable_car("(L.L.train_me_reverse) (S.L.Link2)");
    Arc::get_mut(&mut rear).unwrap().def.script_share = true;
    let mut v = VehicleInstance::new(head, crate::host::VehicleHost::new(Default::default()));
    v.attach_trailer_ex(rear, false);
    run_frames(&mut v, 3);
    assert!(
        v.trailers[0].run.is_some(),
        "it reads its place, so it runs"
    );
    assert_eq!(v.trailers[0].run_var("SLW_y"), Some(5.0));
}

/// A button in the head cab sends a pulse its {frame} clears at once, and the frame run for
/// the click clears it (`update_scripts_only`): the other cars still see it, in either
/// direction, and only once.
#[test]
fn a_pulse_from_a_button_reaches_every_car() {
    let frame = "1 (S.L.Link) (L.L.SLW_x) 1 = {if} 0 (S.L.SLW_x) {endif} (L.L.SLR_x) 1 = {if} (L.L.SLW_y) 1 + (S.L.SLW_y) {endif}";
    let cab = || cable_car_with(frame, "{trigger:press}\n1 (S.L.SLW_x)\n{end}");
    let mut v = VehicleInstance::new(cab(), crate::host::VehicleHost::new(Default::default()));
    v.attach_trailer_ex(cable_car("1 (S.L.Link)"), false);
    v.attach_trailer_ex(cab(), true);
    run_frames(&mut v, 3);
    assert!(v.trigger("press"));
    v.update_scripts_only(0.0);
    run_frames(&mut v, 3);
    assert_eq!(v.var("SLW_x"), Some(0.0), "the pulse is over");
    assert_eq!(
        v.trailers[1].run_var("SLW_y"),
        Some(1.0),
        "the far cab saw it, once"
    );
    v.active_cab = Some(1);
    assert!(v.trigger("press"));
    v.update_scripts_only(0.0);
    run_frames(&mut v, 3);
    assert_eq!(
        v.var("SLW_y").map(|n| n >= 1.0),
        Some(true),
        "and the head cab the far cab's"
    );
    assert_eq!(v.trailers[1].run_var("SLW_x"), Some(0.0));
}

/// The variable log sees what is named like the texts asked for, number or string,
/// and stops following a variable that changes all the time.
#[test]
fn the_variable_log_follows_the_names_asked_for() {
    let ty = cable_car("1 (S.L.Link)");
    let mut state = State::new(&ty.program);
    let slot = ty.program.var("SLW_x").unwrap() as usize;
    let mut seen = HashMap::new();
    let want = vec!["slw_x".to_string()];
    for k in 0..WATCHED_CHANGES + 20 {
        state.vars[slot] = k as f32 + 1.0;
        log_car_vars(&mut seen, &want, 3, &ty, &ty.program, &state);
    }
    assert_eq!(seen.len(), 1, "only the name asked for");
    let (last, changes) = &seen[&(3, false, slot)];
    assert_eq!(*changes, WATCHED_CHANGES + 1);
    assert_eq!(last, &(WATCHED_CHANGES + 1).to_string());
}

/// `train_me_reverse` reads 0 in the car driven from, as when Omsi.exe has turned the
/// train round: what scripts tell their own end by.
#[test]
fn train_me_reverse_follows_the_cab_driven_from() {
    let car = unit_car();
    let mut v = VehicleInstance::new(
        car.clone(),
        crate::host::VehicleHost::new(Default::default()),
    );
    v.attach_trailer_ex(car, true);
    run_frames(&mut v, 3);
    assert_eq!(v.var("train_me_reverse"), Some(0.0));
    assert_eq!(v.trailers[0].run_var("train_me_reverse"), Some(1.0));
    v.active_cab = Some(0);
    run_frames(&mut v, 3);
    assert_eq!(v.var("train_me_reverse"), Some(1.0));
    assert_eq!(v.trailers[0].run_var("train_me_reverse"), Some(0.0));
}

/// Sitting in the tail's cab, a key fires the tail's own trigger: what it stores stays
/// the tail's (not refreshed over from the head car) and goes up the cable to the head.
#[test]
fn keys_in_the_rear_cab_work_its_own_scripts() {
    let head = cable_car("1 (S.L.Link)");
    let mid = cable_car("1 (S.L.Link)");
    let tail = cable_car_with("1 (S.L.Link)", "{trigger:press}\n5 (S.L.SLW_x)\n{end}");
    let mut v = VehicleInstance::new(head, crate::host::VehicleHost::new(Default::default()));
    v.attach_trailer_ex(mid, false);
    v.attach_trailer_ex(tail, true);
    run_frames(&mut v, 2);
    assert!(
        !v.trigger("press"),
        "in the head car's cab the head car has no such key"
    );
    v.active_cab = Some(1);
    assert!(v.trigger("press"), "in the tail's cab its own trigger runs");
    run_frames(&mut v, 8);
    assert_eq!(
        v.trailers[1].run_var("SLW_x"),
        Some(5.0),
        "the tail keeps what its key stored"
    );
    assert_eq!(v.var("SLW_x"), Some(0.0));
    assert_eq!(v.var("SLR_x"), Some(5.0), "and the head car hears it");
}

/// A cab that sends on a line now and then (its IBIS, typed into) hears what it sent
/// itself, though the head cab once wrote the same line too: the NF6D's rear cab's
/// matrix shows the line typed there.
#[test]
fn a_cab_that_typed_last_hears_itself() {
    let head = cable_car_with("1 (S.L.Link)", "{trigger:zero}\n0 (S.L.SLW_x)\n{end}");
    let mid = cable_car("1 (S.L.Link)");
    let tail = cable_car_with("1 (S.L.Link)", "{trigger:type}\n37 (S.L.SLW_x)\n{end}");
    let mut v = VehicleInstance::new(head, crate::host::VehicleHost::new(Default::default()));
    v.attach_trailer_ex(mid, false);
    v.attach_trailer_ex(tail, true);
    v.trigger("zero");
    run_frames(&mut v, 4);
    v.active_cab = Some(1);
    assert!(v.trigger("type"));
    run_frames(&mut v, 3);
    assert_eq!(
        v.trailers[1].run_var("SLR_x"),
        Some(37.0),
        "the rear cab hears what it typed"
    );
    assert_eq!(
        v.trailers[0].run_var("SLR_x"),
        Some(37.0),
        "and so does the car between"
    );
    assert_eq!(v.var("SLR_x"), Some(37.0), "and the head cab");
}

#[test]
fn cars_without_place_variables_need_no_run() {
    let plain = super::super::tests::coupling_test_type(None);
    assert!(!uses_place_in_train(&plain));
    assert!(uses_place_in_train(&unit_car()));
}

/// A car with scripts of its own but no control cables and no place variables runs them:
/// its own motor and brakes are not left standing because nothing links it to the others.
#[test]
fn a_car_with_its_own_scripts_and_no_cables_runs_them() {
    let mut car = cable_car("1 (S.L.SLR_y)");
    let t = Arc::get_mut(&mut car).unwrap();
    t.def.control_cable_front.clear();
    t.def.control_cable_back.clear();
    assert!(uses_place_in_train(&car), "its own frame needs a run");
    let mut shared = cable_car("1 (S.L.SLR_y)");
    Arc::get_mut(&mut shared).unwrap().def.script_share = true;
    assert!(
        !uses_place_in_train(&shared),
        "a car sharing the scripts in front has none"
    );
    let lead = super::super::tests::coupling_test_type(None);
    let mut v = VehicleInstance::new(lead, VehicleHost::new(Default::default()));
    v.attach_trailer_ex(car, false);
    run_frames(&mut v, 3);
    assert_eq!(v.trailers[0].run_var("SLR_y"), Some(1.0), "its frame ran");
}

#[test]
fn a_car_brakes_with_its_own_brakeforce_once_and_never_with_the_leads() {
    let mut lead = VehicleInstance::new(
        super::super::tests::coupling_test_type(None),
        VehicleHost::new(Default::default()),
    );
    let car = |osc: &str| {
        crate::timetable_run::tests::script_test_vehicle(osc, "Brakeforce\n", "")
            .ty
            .clone()
    };
    let braking = car("{frame}\n1000 (S.L.Brakeforce)\n{end}\n");
    let silent = car("{frame}\n{end}\n");
    let mut a = CarRun::start(&braking, &mut lead, [1.0, 0.0, 0.0], &mut Vec::new());
    let mut b = CarRun::start(&silent, &mut lead, [2.0, 0.0, 0.0], &mut Vec::new());
    lead.set_var("Brakeforce", 5000.0);
    a.frame(&mut lead, [1.0, 0.0, 0.0], None, &mut Vec::new());
    b.frame(&mut lead, [2.0, 0.0, 0.0], None, &mut Vec::new());
    assert_eq!(a.take_own_brakeforce(), 1000.0);
    assert_eq!(a.take_own_brakeforce(), 0.0);
    assert_eq!(b.take_own_brakeforce(), 0.0);
}

#[test]
fn ai_train_couplings_hide_inner_tail_lights_and_light_the_last_car() {
    let car = unit_car();
    let mut train = VehicleInstance::new(car.clone(), VehicleHost::new(Default::default()));
    train.attach_trailer_ex(car.clone(), false);
    train.attach_trailer_ex(car.clone(), true);
    let frame = AiFrame {
        lights: true,
        ..Default::default()
    };
    for _ in 0..5 {
        train.update_ai(0.05, &frame);
    }
    assert_eq!(train.var("train_frontcoupling"), Some(0.0));
    assert_eq!(train.var("train_backcoupling"), Some(1.0));
    assert_eq!(
        lit(&train.light_fade),
        [true, false, false, false],
        "lead AI locomotive must not light rear tail lamps"
    );
    assert_eq!(
        lit(&train.trailers[0].light_fade),
        [false; 4],
        "middle car stays dark at both ends"
    );
    assert_eq!(
        lit(&train.trailers[1].light_fade),
        [false, true, false, false],
        "last car lights tail lamps"
    );

    let mut lone = VehicleInstance::new(car, VehicleHost::new(Default::default()));
    for _ in 0..5 {
        lone.update_ai(0.05, &frame);
    }
    assert_eq!(lone.var("train_backcoupling"), Some(0.0));
    assert_eq!(
        lit(&lone.light_fade),
        [true, false, false, true],
        "uncoupled railcar lights front white and rear tail lamps"
    );
}

// the rear cab's BKK FUTAR says its stops with (T.F.x), and looks them up by $.yard; a
// blank file is passed on too, as it frees the entry (0x74f2e8 -> 0x75030c)
#[test]
fn a_car_keeps_its_file_sounds_and_takes_the_new_yard() {
    let mut lead = VehicleInstance::new(
        super::super::tests::coupling_test_type(None),
        VehicleHost::new(Default::default()),
    );
    let ty = crate::timetable_run::tests::script_test_vehicle(
        "{frame}\n\"a.wav\" (T.F.ev_mp3)\n\" \" (T.F.ev_mp3)\n{end}\n",
        "",
        "yard\n",
    )
    .ty
    .clone();
    let mut run = CarRun::start(&ty, &mut lead, [1.0, 0.0, 0.0], &mut Vec::new());
    run.frame(&mut lead, [1.0, 0.0, 0.0], None, &mut Vec::new());
    assert_eq!(
        run.take_fired_files(),
        vec![
            ("ev_mp3".to_string(), "a.wav".to_string()),
            ("ev_mp3".to_string(), " ".to_string()),
        ]
    );
    assert!(run.take_fired_files().is_empty());
    run.set_str_var("yard", "Budafok 141-250");
    let slot = run.prog.str_var("yard").unwrap() as usize;
    assert_eq!(run.state.str_vars[slot], "Budafok 141-250");
}

/// A cable without a link variable (four fields, or an empty fifth as the reader leaves it)
/// is always connected, as on `main`: older vehicles' cables carry their signals.
#[test]
fn a_cable_without_a_link_variable_is_always_connected() {
    for link in [None, Some("")] {
        let car = |body: &str| {
            let mut ty = cable_car(body);
            let t = Arc::get_mut(&mut ty).unwrap();
            for c in t
                .def
                .control_cable_front
                .iter_mut()
                .chain(&mut t.def.control_cable_back)
            {
                c.lines.truncate(4);
                c.lines.extend(link.map(String::from));
            }
            ty
        };
        let mut v =
            VehicleInstance::new(car(""), crate::host::VehicleHost::new(Default::default()));
        v.attach_trailer_ex(car(""), false);
        v.attach_trailer_ex(car("2 (S.L.SLW_x)"), true);
        run_frames(&mut v, 8);
        assert_eq!(
            v.var("SLR_x"),
            Some(2.0),
            "the head car reads the tail's write ({link:?})"
        );
        assert_eq!(
            v.trailers[0].run_var("SLR_x"),
            Some(2.0),
            "and so does the car between ({link:?})"
        );
    }
}

/// The cables run every frame, so they keep their working storage: after the first frame a
/// wired train makes no new read maps or result lists, and its cars still read each other.
/// A coupled train without a cable builds none at all.
#[test]
fn the_cables_keep_their_working_storage_from_frame_to_frame() {
    let mut v = VehicleInstance::new(
        cable_car("1 (S.L.Link)"),
        crate::host::VehicleHost::new(Default::default()),
    );
    v.attach_trailer_ex(cable_car("1 (S.L.Link)"), false);
    v.attach_trailer_ex(cable_car("1 (S.L.Link) 2 (S.L.SLW_x)"), true);
    run_frames(&mut v, 2);
    let storage = |v: &VehicleInstance| {
        let c = &v.cables;
        let lines: Vec<_> = v
            .trailers
            .iter()
            .filter_map(|t| t.run.as_deref())
            .map(|r| r.lines.as_ptr())
            .collect();
        (
            c.reads.len(),
            c.reads.iter().map(|r| r.capacity()).collect::<Vec<_>>(),
            c.wired.as_ptr(),
            lines,
        )
    };
    let before = storage(&v);
    assert_eq!(before.0, 3, "a read map for each car");
    run_frames(&mut v, 6);
    assert_eq!(
        storage(&v),
        before,
        "the same maps and lists, nothing made again"
    );
    assert_eq!(
        v.var("SLR_x"),
        Some(2.0),
        "the head car reads the tail's write"
    );
    assert_eq!(
        v.trailers[0].run_var("SLR_x"),
        Some(2.0),
        "and so does the car between"
    );

    let plain = || super::super::tests::coupling_test_type(Some(-4.0));
    let mut v = VehicleInstance::new(plain(), crate::host::VehicleHost::new(Default::default()));
    v.attach_trailer_ex(plain(), false);
    run_frames(&mut v, 3);
    assert!(
        v.cables.reads.is_empty() && v.cables.values.is_empty(),
        "no cable, nothing built"
    );
}

/// What a key in the rear cab stored is kept in a situation and taken back by a train made
/// again, before its cars' scripts have started and after: saving and loading a tram does not
/// reset its rear cab. The head car's values stay its own.
#[test]
fn a_rear_cabs_switch_is_saved_and_taken_back() {
    let train = || {
        let head = cable_car("1 (S.L.Link)");
        let tail = cable_car_with("1 (S.L.Link)", "{trigger:press}\n5 (S.L.SLW_x)\n{end}");
        let mut v = VehicleInstance::new(head, crate::host::VehicleHost::new(Default::default()));
        v.attach_trailer_ex(tail, true);
        v
    };
    let mut v = train();
    run_frames(&mut v, 2);
    v.active_cab = Some(0);
    assert!(v.trigger("press"));
    run_frames(&mut v, 3);
    let saved = v.car_states();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].car, 0);
    assert!(
        saved[0]
            .vars
            .iter()
            .any(|(n, x)| n.eq_ignore_ascii_case("SLW_x") && *x == 5.0)
    );
    assert!(
        !saved[0]
            .vars
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case("SLR_x")),
        "what the leading vehicle gives it is not its own"
    );
    for started in [false, true] {
        let mut again = train();
        if started {
            run_frames(&mut again, 2);
            assert_eq!(again.trailers[0].run_var("SLW_x"), Some(0.0));
            assert!(again.restore_car_states(&saved) > 0);
        } else {
            assert_eq!(
                again.restore_car_states(&saved),
                0,
                "taken when its scripts start"
            );
        }
        run_frames(&mut again, 3);
        assert_eq!(
            again.trailers[0].run_var("SLW_x"),
            Some(5.0),
            "started first: {started}"
        );
        assert_eq!(
            again.var("SLW_x"),
            Some(0.0),
            "the head car's stays its own"
        );
    }
}
