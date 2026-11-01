use super::*;
use crate::game_lists::ListKind;
use omsi_sim::traffic::{LaneBuilder, LaneKind, Network};

fn fixture() -> (crate::App, u64) {
    let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
    let mut app = crate::assemble_app(args, Default::default());
    app.camera = Some(omsi_render::Camera {
        position: DVec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 60.0,
        near: 0.1,
        far: 2000.0,
    });
    let lane = LaneBuilder::arc(DVec3::ZERO, 0.0, 1500.0, 0.0, 0.0, LaneKind::Street, 3.0);
    let mut net = Network {
        lanes: vec![lane],
        ..Default::default()
    };
    net.build_grid();
    let random = RandomTypes {
        types: vec![],
        groups: vec![],
        group_curves: false,
        group_uvg: vec![],
        uvg_defaults: vec![],
    };
    let mut sim = TrafficSim::assemble(
        Path::new(""),
        net,
        random,
        vec![],
        HashMap::new(),
        (vec![], vec![]),
        vec![],
        (1.0, 0),
        0,
    );
    let mut bus = crate::schedule::tests::script_test_vehicle("", "", "");
    bus.position = DVec3::new(0.0, 100.0, 0.0);
    let other = omsi_sim::VehicleInstance::new(
        bus.ty.clone(),
        omsi_sim::VehicleHost::new(Default::default()),
    );
    let id = sim.adopt_vehicle(bus, 0, 100.0, None);
    sim.release(0);
    sim.adopt_vehicle(other, 0, 50.0, None);
    app.session.traffic = Some(Traffic::with_sim(sim, Path::new("")));
    app.session
        .handed_over
        .record(id, "Unscheduled test bus".into());
    (app, id)
}

#[test]
fn unscheduled_released_bus_can_be_located_without_taking_ownership() {
    let (mut app, id) = fixture();
    app.start_on_foot(DVec3::new(10.0, 20.0, 0.0), 0.0);
    let foot = app.session.on_foot.as_ref().unwrap().pos;
    let car = &app.session.traffic.as_ref().unwrap().cars[0];
    assert!(car.gone);
    assert!(car.bus.is_none());
    crate::game_panels::quick_menu::click(&mut app, "find_ai");
    assert_eq!(app.menus.list_kind, Some(ListKind::HandedOverBuses));
    assert!(app.find_handed_over_bus(id));
    assert_eq!(app.view, "free");
    assert!(!app.cam.ego);
    assert!((app.camera.unwrap().position - DVec3::new(0.0, 75.0, 30.0)).length() < 1e-6);
    assert_eq!(app.session.on_foot.as_ref().unwrap().pos, foot);
    assert!(app.player.is_none());
    assert_eq!(app.session.traffic.as_ref().unwrap().cars.len(), 2);
    assert!(app.session.handed_over.contains(id));
}

#[test]
fn locator_rechecks_stable_id_after_traffic_reorders() {
    let (mut app, id) = fixture();
    app.session.traffic.as_mut().unwrap().cars.swap(0, 1);
    assert!(app.find_handed_over_bus(id));
    assert!((app.camera.unwrap().position - DVec3::new(0.0, 75.0, 30.0)).length() < 1e-6);
}

#[test]
fn despawn_between_list_and_click_keeps_a_status_row_without_a_phantom_camera() {
    let (mut app, id) = fixture();
    let rows = app.handed_over_items();
    app.session
        .traffic
        .as_mut()
        .unwrap()
        .cars
        .retain(|car| car.id != id);
    let before = app.camera.unwrap().position;
    let action = &rows[0].1;
    assert_eq!(
        crate::game_lists::run(&mut app, &ListKind::HandedOverBuses, action),
        Some(ListKind::HandedOverBuses)
    );
    assert_eq!(app.camera.unwrap().position, before);
    assert!(app.service_msg.is_some());
    assert_eq!(app.handed_over_items().len(), 1);
}
