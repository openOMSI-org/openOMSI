//! The game's Administration menu of a server administered from afar, over a real session: the
//! menu asks at its own pace, shows the players and the state the server told it, and its
//! lines - picked as a player picks them - send the player picked away and nobody else.

mod common;
use common::*;
use omsi_lan::admin::panel::{AdminMenu, Panel};
use omsi_lan::commands::Command;
use std::time::{Duration, Instant};

fn tr(s: &str) -> String {
    s.to_string()
}

/// The admin's game: its menu's questions sent, the server's answers read into the menu.
fn menu_step(p: &mut Player, panel: &mut Panel) {
    for a in panel.due(Instant::now()) {
        p.lan.command(1, a);
    }
    p.step();
    for h in std::mem::take(&mut p.heard) {
        if let Some(c) = Command::parse(1, &h) {
            panel.take(&c);
        }
    }
}

/// A line's action, as the game carries it out: a command to the server, or another page.
fn pick(p: &mut Player, panel: &mut Panel, action: &str) -> AdminMenu {
    let m = AdminMenu::parse(action);
    if let AdminMenu::Send(line) = &m {
        p.lan.command(1, &format!("admin {line}"));
        panel.soon();
    }
    m
}

#[test]
fn the_remote_menu_sends_away_the_player_picked() {
    let port = 28800;
    let mut s = Server::new(port);
    let mut admin = Player::join(&mut s, port, "Chefe");
    let mut pichau = Player::join(&mut s, port, "Pichau");
    let mut fan = Player::join(&mut s, port, "pichau_fan");
    assert_eq!(login(&mut s, &mut admin, &mut [&mut pichau, &mut fan], "admin secret"), "admin-ok admin");
    let mut panel = Panel::default();
    panel.take(&Command::AdminOk(Some(omsi_admin::Role::Admin)));
    // the menu fills from the server's answers
    let t0 = Instant::now();
    while panel.players.len() < 3 && t0.elapsed() < Duration::from_secs(5) {
        menu_step(&mut admin, &mut panel);
        pichau.step();
        fan.step();
        s.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    let lines = panel.player_lines(&tr);
    let pichau_line = lines.iter().find(|l| l.0.starts_with("Pichau (#")).expect("Pichau listed").clone();
    assert!(lines.iter().any(|l| l.0.starts_with("pichau_fan (#")));
    let main = panel.main_lines(Instant::now(), &tr);
    assert!(main.iter().any(|l| l.1 == "send passengers 0"), "{main:?}");
    // pick Pichau, ask to send them away, say yes
    let AdminMenu::Player(t) = pick(&mut admin, &mut panel, &pichau_line.1) else { panic!("a player's page") };
    let ask = panel.one_player_lines(t, &tr)[1].1.clone();
    let AdminMenu::Confirm(verb, t2) = pick(&mut admin, &mut panel, &ask) else { panic!("the question") };
    assert_eq!(t2, t);
    let confirm = panel.confirm_lines(&verb, t, &tr);
    assert!(confirm[0].0.contains("Pichau (#"), "{confirm:?}");
    assert!(matches!(pick(&mut admin, &mut panel, &confirm[1].1), AdminMenu::Send(_)));
    let t0 = Instant::now();
    while (panel.results.is_empty() || pichau.lan.turned_away.is_none()) && t0.elapsed() < Duration::from_secs(5) {
        menu_step(&mut admin, &mut panel);
        pichau.step();
        fan.step();
        s.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(panel.results[0].1, omsi_admin::Status::Applied, "{:?}", panel.results);
    assert!(pichau.lan.turned_away.is_some());
    assert!(fan.lan.connected && fan.lan.turned_away.is_none());
    // the menu's next answer has no Pichau, and the old page says to pick again
    let t0 = Instant::now();
    while panel.player(t).is_some() && t0.elapsed() < Duration::from_secs(8) {
        menu_step(&mut admin, &mut panel);
        fan.step();
        s.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(panel.player(t).is_none());
    assert_eq!(AdminMenu::parse(&panel.confirm_lines("kick", t, &tr)[0].1), AdminMenu::Players);
    // a world operation from the menu: the server applies it and says so
    let line = panel.main_lines(Instant::now(), &tr).into_iter().find(|l| l.1 == "send passengers 0").unwrap();
    pick(&mut admin, &mut panel, &line.1);
    let t0 = Instant::now();
    while s.adm.passenger_density != Some(0.0) && t0.elapsed() < Duration::from_secs(5) {
        menu_step(&mut admin, &mut panel);
        fan.step();
        s.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(s.adm.passenger_density, Some(0.0));
}

/// A server that does not answer the menu (older, or gone): the menu says so after a while
/// instead of waiting for ever, and asks again.
#[test]
fn a_silent_server_is_shown_as_such() {
    let mut panel = Panel::default();
    let t0 = Instant::now();
    assert_eq!(panel.due(t0).len(), 2);
    assert!(!panel.silent(t0 + Duration::from_secs(1)));
    let lines = panel.main_lines(t0 + Duration::from_secs(7), &tr);
    assert!(lines[0].0.starts_with("The server does not answer"), "{lines:?}");
    // the older menu's full list is still offered
    assert!(lines.iter().any(|l| l.1 == "classic"));
}
