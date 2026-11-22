//! `car_sync` between two real UDP sessions: a tram's rear cab as another player sees it.

use super::*;
use omsi_net::{Pose, WorldInfo};
use std::time::Duration;

/// A tram car whose rear cab keeps a switch of its own: `{trigger:press}` stores 5 into
/// `Cab_Switch` (and a destination), which its `{frame}` leaves alone.
fn cab_car() -> Arc<omsi_sim::VehicleType> {
    car_with("{trigger:press}\n5 (S.L.Cab_Switch) \"Depot\" (S.$.Cab_Target)\n{end}\n")
}

/// The head car: it knows `Cab_Switch` but its scripts never store it, so a car behind it
/// that does not hold the switch itself takes the head car's value at every frame.
fn head_car() -> Arc<omsi_sim::VehicleType> {
    car_with("")
}

/// A car with the frame script every one has, then `triggers`.
fn car_with(triggers: &str) -> Arc<omsi_sim::VehicleType> {
    static MADE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("omsi-lan-car-sync-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("cab.osc");
    std::fs::write(
        &script,
        format!("{{frame}}\n(L.L.train_me_reverse) (S.L.Turned)\n{{end}}\n{triggers}"),
    )
    .unwrap();
    let list = dir.join("cab_varlist.txt");
    std::fs::write(
        &list,
        "train_me_reverse\ntrain_frontcoupling\ntrain_backcoupling\nTurned\nCab_Switch\n",
    )
    .unwrap();
    let strings = dir.join("cab_stringvarlist.txt");
    std::fs::write(&strings, "Cab_Target\n").unwrap();
    let program = omsi_script::compile(&omsi_script::CompileInput {
        varlists: vec![list],
        stringvarlists: vec![strings],
        scripts: vec![script],
        ..Default::default()
    });
    std::fs::remove_dir_all(&dir).ok();
    assert!(program.errors.is_empty(), "{:?}", program.errors);
    let coupling = |y: f32| Some(omsi_vehicle::vehicle::Coupling { pos: [0.0, y, 0.3] });
    Arc::new(omsi_sim::VehicleType {
        def: omsi_vehicle::Vehicle {
            coupling_front: coupling(5.0),
            coupling_back: coupling(-5.0),
            axles: [-3.0, 3.0]
                .map(|long| omsi_vehicle::Axle {
                    long,
                    ..Default::default()
                })
                .to_vec(),
            ..Default::default()
        },
        model: Default::default(),
        model_dir: Default::default(),
        program: Arc::new(program),
        meshes: Vec::new(),
        lod_meshes: Vec::new(),
        paint_schemes: Vec::new(),
        texchanges: Vec::new(),
        wheel_meshes: Vec::new(),
        suspension_axles: Vec::new(),
        missing_packs: Vec::new(),
        mesh_bounds: Vec::new(),
        mesh_boxes: Vec::new(),
    })
}

/// A two-car tram, the second car (`car`) turned round: its cab at the train's rear.
fn tram(car: &Arc<omsi_sim::VehicleType>) -> VehicleInstance {
    let mut v = VehicleInstance::new(
        head_car(),
        omsi_sim::host::VehicleHost::new(Default::default()),
    );
    v.attach_trailer_ex(car.clone(), true);
    for _ in 0..3 {
        v.update(1.0 / 30.0);
    }
    v
}

/// The player at the rear cab of their tram turns its switch: the other player's copy of the
/// tram shows it in its rear car, not its head car, though its own scripts never pressed it.
/// With no cars of their own on a train, nothing is sent; another train's cars take nothing.
#[test]
fn a_rear_cab_switch_reaches_the_other_players_copy_of_the_tram() {
    let car = cab_car();
    let mut owner = tram(&car);
    let mut copy = tram(&car);
    assert!(
        owner.trailers[0].own_run().is_some(),
        "the rear car runs its own scripts"
    );
    owner.active_cab = Some(0);
    assert!(owner.trigger("press"));
    owner.update(1.0 / 30.0);
    assert_eq!(owner.trailers[0].run_var("Cab_Switch"), Some(5.0));

    let mut host = LanSession::host(0, "host", WorldInfo::default(), true).unwrap();
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
    let mut player =
        LanSession::join_addr(vec![addr], None, "driver", WorldInfo::default()).unwrap();
    let pose = |x: f64| Pose {
        name: "p".into(),
        bus: "Vehicles/x.bus".into(),
        x,
        ..Default::default()
    };
    let mut game = LanGame::default();
    let (mut tables, mut floats, mut strings) =
        (CarTables::default(), Synced::default(), Synced::default());
    for _ in 0..200 {
        if player.connected && host.peers().count() == 1 {
            send(&mut player, &mut game, &owner, 0.1);
        }
        host.tick(0.05, &pose(0.0));
        player.tick(0.05, &pose(100.0));
        for v in host.take_car_vars() {
            assert_eq!(v.id, player.my_id);
            take(&mut tables, &copy, (&mut floats, &mut strings), v);
        }
        if floats.values().any(|&x| x == 5.0) && !strings.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!floats.is_empty(), "the rear car's values came");
    assert_eq!(copy.trailers[0].run_var("Cab_Switch"), Some(0.0));
    apply(&mut copy, &floats, &strings);
    copy.update(1.0 / 30.0);
    assert_eq!(
        copy.trailers[0].run_var("Cab_Switch"),
        Some(5.0),
        "the copy's rear cab, through its own frame"
    );
    assert_eq!(
        copy.trailers[0].own_run().unwrap().get_str("Cab_Target"),
        Some("Depot")
    );
    assert_eq!(
        copy.var("Cab_Switch"),
        Some(0.0),
        "the head cab's stays its own"
    );

    // a lone car sends nothing; a train of other cars takes nothing
    let mut lone = VehicleInstance::new(car, omsi_sim::host::VehicleHost::new(Default::default()));
    lone.update(1.0 / 30.0);
    let mut alone = LanGame::default();
    send(&mut player, &mut alone, &lone, 0.1);
    assert!(alone.car_floats.is_empty());
    let other = tram(&cab_car_with_more());
    let (mut t2, mut f2, mut s2) = (CarTables::default(), Synced::default(), Synced::default());
    let came = omsi_net::vars::VarsIn {
        id: 1,
        table: game.my_cars.hash,
        floats: vec![(0, 5.0)],
        ..Default::default()
    };
    take(&mut t2, &other, (&mut f2, &mut s2), came);
    assert!(f2.is_empty(), "another table, nothing taken");
}

/// [`cab_car`] with one more variable: another table.
fn cab_car_with_more() -> Arc<omsi_sim::VehicleType> {
    let mut ty = cab_car();
    let t = Arc::get_mut(&mut ty).unwrap();
    let mut p = (*t.program).clone();
    p.var_names.push("Another".into());
    t.program = Arc::new(p);
    ty
}
