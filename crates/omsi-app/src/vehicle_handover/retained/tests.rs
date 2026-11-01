use super::*;

fn player() -> crate::player::Player {
    let mut vehicle = crate::schedule::tests::script_test_vehicle("", "", "");
    let ty = std::sync::Arc::make_mut(&mut vehicle.ty);
    ty.def.path = "retained.bus".into();
    ty.def.cameras_driver.push(omsi_vehicle::Camera {
        pos: [0.0, 0.0, 1.8],
        dist: 0.0,
        fov: 60.0,
        yaw: 0.0,
        pitch: 0.0,
        extra: None,
        fixed: false,
    });
    let ty = vehicle.ty.clone();
    let render = crate::scene::VehicleRender {
        window_wipers: None,
        instances: vec![],
        far: vec![],
        own_materials: vec![],
        set: None,
        variants: vec![],
        text_textures: vec![],
        script_textures: vec![],
        shared_script: false,
        displays_far: false,
        display_tick: 0,
        skinned: vec![],
        damaged: vec![],
        glass_cracks: vec![],
        hidden: false,
        interior_lamps: Default::default(),
        interior_blocks: Default::default(),
    };
    crate::spawn::new_player(vehicle, render, vec![], vec![], false, &ty)
}

fn app() -> App {
    let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
    let mut app = crate::assemble_app(args, Default::default());
    app.view = "free".into();
    app
}

fn duty(line: &str) -> crate::schedule::PlayerDuty {
    crate::schedule::PlayerDuty::new(line.into(), "tour".into(), vec![], 0, 0, true)
}

#[test]
fn original_and_taken_bus_can_be_swapped_repeatedly_without_losing_their_duties() {
    let mut app = app();
    let mut original = player();
    let original_id = original.uid;
    original.parked_duty = Some(SavedDuty {
        duty: Some(duty("original")),
        ..Default::default()
    });
    let taken = player();
    let taken_id = taken.uid;
    app.session.placed.push(original);
    app.player = Some(taken);
    app.session.duty = Some(duty("taken"));
    for (uid, line) in [
        (original_id, "original"),
        (taken_id, "taken"),
        (original_id, "original"),
    ] {
        app.view = "free".into();
        app.take_retained_bus(uid).unwrap();
        assert_eq!(app.player.as_ref().unwrap().uid, uid);
        assert_eq!(app.session.placed.len(), 1);
        assert_ne!(app.session.placed[0].uid, uid);
        assert_eq!(app.session.duty.as_ref().unwrap().line, line);
        assert_eq!(app.args.line.as_deref(), Some(line));
        assert_eq!(app.view, "driver");
    }
}

#[test]
fn confirmation_resolves_uid_after_reordering_and_cancels_a_removed_bus() {
    let mut app = app();
    let original = player();
    let uid = original.uid;
    app.session.placed.extend([original, player()]);
    app.session.placed.swap(0, 1);
    app.take_retained_bus(uid).unwrap();
    assert_eq!(app.player.as_ref().unwrap().uid, uid);
    app.view = "free".into();
    assert!(app.take_retained_bus(uid).is_err());
    assert_eq!(app.player.as_ref().unwrap().uid, uid);
    assert_eq!(app.session.placed.len(), 1);
}

#[test]
fn a_bus_without_its_own_duty_does_not_inherit_the_other_bus_timetable() {
    let mut app = app();
    let parked = player();
    let uid = parked.uid;
    app.session.placed.push(parked);
    app.player = Some(player());
    app.session.duty = Some(duty("taken"));
    app.take_retained_bus(uid).unwrap();
    assert!(app.session.duty.is_none());
    assert!(app.args.line.is_none());
    assert_eq!(
        app.session.placed[0]
            .parked_duty
            .as_ref()
            .unwrap()
            .duty
            .as_ref()
            .unwrap()
            .line,
        "taken"
    );
}

#[test]
fn confirmation_rechecks_free_view_before_changing_ownership() {
    let mut app = app();
    let parked = player();
    let uid = parked.uid;
    app.session.placed.push(parked);
    for view in ["driver", "outside", "pax", "foot"] {
        app.view = view.into();
        assert!(app.take_retained_bus(uid).is_err());
        assert!(app.player.is_none());
        assert_eq!(app.session.placed.len(), 1);
    }
    app.view = "free".into();
    app.cam.ego = true;
    assert!(app.take_retained_bus(uid).is_err());
}

fn two_buses() -> (App, u64, u64) {
    let mut app = app();
    let active = player();
    let parked = player();
    let ids = (active.uid, parked.uid);
    let mut a = duty("A");
    a.next_stop = 3;
    let mut b = duty("B");
    b.next_stop = 7;
    app.player = Some(active);
    app.session.duty = Some(a);
    let mut parked = parked;
    parked.parked_duty = Some(SavedDuty {
        duty: Some(b),
        ..Default::default()
    });
    app.session.placed.push(parked);
    (app, ids.0, ids.1)
}

fn assert_driving(app: &App, uid: u64, line: &str, next_stop: usize) {
    assert_eq!(app.player.as_ref().unwrap().uid, uid);
    let duty = app
        .session
        .duty
        .as_ref()
        .expect("the selected bus keeps its duty");
    assert_eq!(duty.line, line);
    assert_eq!(duty.next_stop, next_stop);
    assert_eq!(app.args.line.as_deref(), Some(line));
    assert_eq!(app.args.tour.as_deref(), Some("tour"));
}

#[test]
fn f4_alt_and_pedestrian_entries_share_each_bus_duty_and_stop_progress() {
    let (mut app, a, b) = two_buses();
    app.take_retained_bus(b).unwrap();
    assert_driving(&app, b, "B", 7);
    app.switch_vehicle();
    assert_driving(&app, a, "A", 3);
    app.take_placed(0);
    assert_driving(&app, b, "B", 7);
    app.view = "free".into();
    app.take_retained_bus(a).unwrap();
    assert_driving(&app, a, "A", 3);
}

#[test]
fn legacy_entries_do_not_assign_another_bus_duty_to_an_unscheduled_bus() {
    for pedestrian in [false, true] {
        let (mut app, a, b) = two_buses();
        app.session.placed[0].parked_duty = None;
        if pedestrian {
            app.take_placed(0);
        } else {
            app.switch_vehicle();
        }
        assert_eq!(app.player.as_ref().unwrap().uid, b);
        assert!(app.session.duty.is_none());
        assert!(app.args.line.is_none());
        assert!(app.args.tour.is_none());
        app.view = "free".into();
        app.take_retained_bus(a).unwrap();
        assert_driving(&app, a, "A", 3);
    }
}

#[test]
fn legacy_entries_restore_the_placed_bus_duty_when_no_bus_is_owned() {
    for pedestrian in [false, true] {
        let (mut app, _, b) = two_buses();
        app.player = None;
        app.session.duty = None;
        if pedestrian {
            app.take_placed(0);
        } else {
            app.switch_vehicle();
        }
        assert_driving(&app, b, "B", 7);
        assert!(app.session.placed.is_empty());
    }
}

#[test]
fn invalid_switch_to_index_is_a_noop_instead_of_switching_the_first_bus() {
    let (mut app, a, _) = two_buses();
    app.args.line = Some("A".into());
    app.args.tour = Some("tour".into());
    app.switch_vehicle_to(usize::MAX);
    assert_driving(&app, a, "A", 3);
    assert_eq!(app.session.placed.len(), 1);
    app.take_placed(usize::MAX);
    assert_driving(&app, a, "A", 3);
}

#[test]
fn legacy_switch_clears_bus_specific_session_transients() {
    let (mut app, _, b) = two_buses();
    app.session.duty_places = true;
    app.session.safe_pose = Some((glam::DVec3::ZERO, 0.0));
    app.session.safe_age = 12.0;
    app.switch_vehicle();
    assert_driving(&app, b, "B", 7);
    assert!(!app.session.duty_places);
    assert!(app.session.safe_pose.is_none());
    assert_eq!(app.session.safe_age, 0.0);
}

#[test]
fn walking_arrival_uses_the_same_duty_exchange_as_f4_and_alt() {
    let (mut app, a, b) = two_buses();
    app.start_on_foot(glam::DVec3::ZERO, 0.0);
    app.session.on_foot.as_mut().unwrap().arrive = Some(crate::on_foot::Then::Placed(b));
    app.frame_on_foot(0.0);
    assert_eq!(app.player.as_ref().unwrap().uid, b);
    let duty = app.session.duty.as_ref().unwrap();
    assert_eq!((&*duty.line, duty.next_stop), ("B", 7));
    assert_driving(&app, b, "B", 7);
    assert!(app.session.on_foot.is_none());
    app.view = "free".into();
    app.take_retained_bus(a).unwrap();
    assert_driving(&app, a, "A", 3);
}

#[test]
fn alt_switch_from_pedestrian_mode_leaves_the_avatar_and_enters_the_target_cab() {
    let (mut app, _, b) = two_buses();
    app.start_on_foot(glam::DVec3::ZERO, 0.0);
    app.switch_vehicle();
    assert_driving(&app, b, "B", 7);
    assert!(app.session.on_foot.is_none());
    assert_eq!(app.view, "driver");
}

#[test]
fn indexed_alt_switch_restores_the_selected_bus_after_reordering() {
    let (mut app, a, b) = two_buses();
    app.session.placed.insert(0, player());
    app.switch_vehicle_to(1);
    assert_driving(&app, b, "B", 7);
    assert_eq!(app.session.placed.len(), 2);
    app.view = "free".into();
    app.take_retained_bus(a).unwrap();
    assert_driving(&app, a, "A", 3);
}

#[test]
fn walking_confirmation_uses_stable_identity_after_the_placed_list_changes() {
    let (mut app, _, b) = two_buses();
    app.start_on_foot(glam::DVec3::ZERO, 0.0);
    app.session.on_foot.as_mut().unwrap().arrive = Some(crate::on_foot::Then::Placed(b));
    app.session.placed.insert(0, player());
    app.frame_on_foot(0.0);
    assert_driving(&app, b, "B", 7);
}

#[test]
fn a_disappeared_walking_target_does_not_transfer_ownership_or_duty() {
    let (mut app, a, b) = two_buses();
    app.args.line = Some("A".into());
    app.args.tour = Some("tour".into());
    app.start_on_foot(glam::DVec3::ZERO, 0.0);
    app.session.on_foot.as_mut().unwrap().arrive = Some(crate::on_foot::Then::Placed(b));
    app.session.placed.clear();
    app.frame_on_foot(0.0);
    assert_driving(&app, a, "A", 3);
    assert!(app.session.on_foot.is_some());
}

mod reservations;

#[path = "passenger_tests.rs"]
mod passenger_tests;

#[test]
fn lan_vehicle_entries_keep_the_network_players_active_duty() {
    for host in [false, true] {
        for entry in 0..3 {
            let (mut app, _, b) = two_buses();
            app.session.placed[0].parked_duty = None;
            app.sync_driven_duty_args();
            app.net.lan = Some(if host {
                omsi_net::LanSession::host(0, "test", Default::default(), false).unwrap()
            } else {
                omsi_net::LanSession::join_addr(vec![], None, "test", Default::default()).unwrap()
            });
            match entry {
                0 => app.switch_vehicle(),
                1 => app.take_placed(0),
                _ => {
                    app.start_on_foot(glam::DVec3::ZERO, 0.0);
                    app.session.on_foot.as_mut().unwrap().arrive = Some(crate::on_foot::Then::Placed(b));
                    app.frame_on_foot(0.0);
                }
            }
            assert_driving(&app, b, "A", 3);
            assert!(app.session.placed[0].parked_duty.is_none());
        }
    }
}

#[test]
fn lan_roles_reject_ai_ownership_changes_before_mutation() {
    for host in [false, true] {
        let (mut app, a, b) = two_buses();
        app.sync_driven_duty_args();
        app.net.lan = Some(if host {
            omsi_net::LanSession::host(0, "test", Default::default(), false).unwrap()
        } else {
            omsi_net::LanSession::join_addr(vec![], None, "test", Default::default()).unwrap()
        });
        let reason = "Vehicle handover is only available in single-player";
        assert_eq!(app.give_bus_to_ai(), Err(reason.into()));
        assert_eq!(app.take_ai_bus(1), Err(reason.into()));
        assert_eq!(app.take_retained_bus(b), Err(reason.into()));
        assert_driving(&app, a, "A", 3);
        assert_eq!(app.session.placed[0].uid, b);
    }
}
