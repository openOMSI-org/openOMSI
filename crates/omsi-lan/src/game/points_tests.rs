//! `points` between a host's game and a player's.

use super::*;
use omsi_net::{Pose, WorldInfo};
use omsi_sim::scenery::{SceneryChange, SceneryInbox, SceneryInstance, SceneryObject};
use std::sync::Arc;
use std::time::Duration;

/// A game whose map has points `7` beside the track at the origin.
fn map_with_points() -> (SceneryInbox, SceneryState) {
    let inbox = SceneryInbox::default();
    let mut s = SceneryState::default();
    place_points(&inbox);
    s.take_inbox(&inbox);
    (inbox, s)
}

/// Points `7` as the map has them, placed (again) by the tile loader.
fn place_points(inbox: &SceneryInbox) {
    let mut p = omsi_script::Program::default();
    p.declare_var("Switch");
    let inst = SceneryInstance::new(Arc::new(p), &[], omsi_sim::SimClock::default(), &[]);
    let mut o = SceneryObject::new(inbox.next_key(), 7, DVec3::ZERO, (0, 0), inst, 0);
    o.switch_dirs = vec![None, Some(1)];
    inbox.push(SceneryChange::Add(o));
}

/// The player's tram throws the points: they go up to the host, and the host's older
/// setting that was already on its way does not throw them back.
#[test]
fn points_a_player_throws_reach_the_host_and_stay() {
    let (_, mut host) = map_with_points();
    let (_, mut player) = map_with_points();
    let mut sync = PointsSync::default();
    assert!(sync.own_changes(&player, DVec3::ZERO, 0.25).is_empty());

    player.set_switches(&[(7, 1)]);
    let up = sync.own_changes(&player, DVec3::ZERO, 0.25);
    assert_eq!(up, vec![(7, 1.0)]);
    for (id, value) in up {
        assert!(host.set_switch_value(id, value));
    }
    assert_eq!(host.switch_set_to(7, 1), Some(true));

    sync.came_down(&[(7, 0.0)]);
    sync.take_host(&mut player);
    assert_eq!(
        player.switch_set_to(7, 1),
        Some(true),
        "held against the old setting"
    );
    sync.came_down(&near(&host, DVec3::ZERO));
    sync.take_host(&mut player);
    assert!(sync.own.is_empty(), "the host has them");
    assert!(sync.own_changes(&player, DVec3::ZERO, 0.25).is_empty());

    // the host's AI tram sets them back: the player's game follows, and sends nothing up
    host.set_switches(&[(7, 0)]);
    sync.came_down(&near(&host, DVec3::ZERO));
    sync.take_host(&mut player);
    assert_eq!(player.switch_set_to(7, 0), Some(true));
    assert!(sync.own_changes(&player, DVec3::ZERO, 0.25).is_empty());
}

/// The host takes a throw from a player whose vehicle is near the points, and not from one
/// the other side of the map, or one it does not know.
#[test]
fn only_points_near_the_player_who_threw_them_are_set() {
    let (_, mut host) = map_with_points();
    let mut sync = PointsSync::default();
    let far = DVec3::new(RADIUS * 3.0, 0.0, 0.0);
    sync.came_up(2, &[(7, 1.0)]);
    sync.came_up(3, &[(7, 1.0)]);
    sync.set_thrown(&mut host, |id| (id == 2).then_some(far));
    assert_eq!(host.switch_set_to(7, 0), Some(true));
    sync.came_up(2, &[(7, 1.0)]);
    sync.set_thrown(&mut host, |_| Some(DVec3::new(100.0, 0.0, 0.0)));
    assert_eq!(host.switch_set_to(7, 1), Some(true));
}

/// Points that come again with their tile stand as the map has them: that is not a throw
/// of the player's, and the host's setting is not overwritten with it.
#[test]
fn points_loaded_again_are_not_thrown_by_the_player() {
    let (inbox, mut player) = map_with_points();
    let mut sync = PointsSync::default();
    player.set_switch_value(7, 1.0);
    assert!(sync.own_changes(&player, DVec3::ZERO, 0.25).is_empty());
    player.drop_tile((0, 0), |_, _| {});
    place_points(&inbox);
    player.take_inbox(&inbox);
    assert_eq!(player.switch_set_to(7, 0), Some(true));
    assert!(sync.own_changes(&player, DVec3::ZERO, 0.25).is_empty());
}

/// A player's thrown points travel up in a world frame between two real UDP sessions.
#[test]
fn thrown_points_go_up_to_the_host_over_the_network() {
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
    let mut came = Vec::new();
    for _ in 0..200 {
        if player.connected && host.peers().count() == 1 {
            player.send_world_up(&WorldFrame {
                points: vec![(7, 1.0)],
                ..Default::default()
            });
        }
        host.tick(0.05, &pose(0.0));
        player.tick(0.05, &pose(100.0));
        for (_, _, f) in host.take_world_up() {
            came.extend(f.points);
        }
        if !came.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(came.first(), Some(&(7, 1.0)));
}
