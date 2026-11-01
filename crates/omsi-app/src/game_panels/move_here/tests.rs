use super::*;
use crate::App;
use glam::DVec3;

fn app(with_bus: bool) -> App {
    let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
    let mut app = crate::assemble_app(args, Default::default());
    app.camera = Some(omsi_render::Camera {
        position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0,
        fov_deg: 60.0, near: 0.1, far: 2000.0,
    });
    if with_bus {
        let vehicle = crate::schedule::tests::script_test_vehicle("", "", "");
        let ty = vehicle.ty.clone();
        let render = crate::scene::VehicleRender {
            window_wipers: None, instances: vec![], far: vec![], own_materials: vec![],
            set: None, variants: vec![], text_textures: vec![], script_textures: vec![],
            shared_script: false, displays_far: false, display_tick: 0, skinned: vec![],
            damaged: vec![], glass_cracks: vec![], hidden: false,
            interior_lamps: Default::default(), interior_blocks: Default::default(),
        };
        app.player = Some(crate::spawn::new_player(vehicle, render, vec![], vec![], false, &ty));
    }
    app
}

#[test]
fn no_owned_bus_cannot_open_the_move_question() {
    let mut app = app(false);
    ask(&mut app, DVec3::ONE);
    assert!(app.menus.game_panels.move_to.is_none());
}

#[test]
fn losing_the_bus_while_confirming_cannot_report_a_teleport() {
    let mut app = app(true);
    ask(&mut app, DVec3::ONE);
    assert!(app.menus.game_panels.move_to.is_some());
    app.player.take();
    click(&mut app, "yes");
    assert!(app.menus.game_panels.move_to.is_none());
    assert!(app.service_msg.is_none());
}

#[test]
fn an_owned_bus_can_still_be_moved_and_cancelled() {
    let mut app = app(true);
    let at = DVec3::new(12.0, 34.0, 5.0);
    ask(&mut app, at);
    click(&mut app, "no");
    assert_ne!(app.player.as_ref().unwrap().vehicle.position, at);
    assert!(app.service_msg.is_none());
    ask(&mut app, at);
    click(&mut app, "yes");
    assert_eq!(app.player.as_ref().unwrap().vehicle.position, at);
    assert!(app.service_msg.is_some());
}

#[test]
fn lan_clients_cannot_open_the_move_question() {
    let mut app = app(true);
    app.net.lan = Some(omsi_net::LanSession::join_addr(vec![], None, "test", Default::default()).unwrap());
    ask(&mut app, DVec3::ONE);
    assert!(app.menus.game_panels.move_to.is_none());
}

#[test]
fn joining_as_client_while_confirming_cannot_move_the_bus() {
    let mut app = app(true);
    let before = app.player.as_ref().unwrap().vehicle.position;
    ask(&mut app, DVec3::ONE);
    app.net.lan = Some(omsi_net::LanSession::join_addr(vec![], None, "test", Default::default()).unwrap());
    click(&mut app, "yes");
    assert_eq!(app.player.as_ref().unwrap().vehicle.position, before);
    assert!(app.menus.game_panels.move_to.is_none());
}

#[test]
fn no_owned_bus_cannot_start_a_city_map_move() {
    let mut app = app(false);
    app.place_bus_at(glam::DVec2::ZERO);
    assert!(app.service_msg.is_none());
}

#[test]
fn lan_host_can_move_its_owned_bus() {
    let mut app = app(true);
    app.net.lan = Some(omsi_net::LanSession::host(0, "test", Default::default(), false).unwrap());
    ask(&mut app, DVec3::ONE);
    assert!(app.menus.game_panels.move_to.is_some());
    click(&mut app, "yes");
    assert_eq!(app.player.as_ref().unwrap().vehicle.position, DVec3::ONE);
}

#[test]
fn walking_with_an_owned_bus_still_allows_a_map_move() {
    let mut app = app(true);
    app.start_on_foot(DVec3::ZERO, 0.0);
    ask(&mut app, DVec3::ONE);
    assert!(app.menus.game_panels.move_to.is_some());
    click(&mut app, "yes");
    assert_eq!(app.player.as_ref().unwrap().vehicle.position, DVec3::ONE);
    assert!(app.session.on_foot.is_none());
}
