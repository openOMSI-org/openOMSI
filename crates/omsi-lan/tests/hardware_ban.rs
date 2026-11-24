//! The machine ban from the admin's command to the door, with real sockets on this machine:
//! a player is banned and sent away, comes back with a new game and is turned away, and is
//! let in again once the ban is lifted; the roles decide who may ban for which list.

use omsi_admin::{Actor, Origin, Role, Status};
use omsi_lan::admin::{Action, Exec, ServerAdmin, ServerRules, exec};
use omsi_lan::bans::{BanStore, Scope, derive_hardware_id};
use omsi_net::{LanSession, Pose, WorldInfo};
use std::time::{Duration, Instant};

fn pump(
    host: &mut LanSession,
    clients: &mut [&mut LanSession],
    until: impl Fn(&LanSession, &[&mut LanSession]) -> bool,
) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(5) {
        host.tick(0.05, &Pose::default());
        omsi_lan::bans::sync_host(host);
        for c in clients.iter_mut() {
            c.tick(0.05, &Pose::default());
        }
        if until(host, clients) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn client(port: u16, name: &str, machine: &str) -> LanSession {
    let mut c = LanSession::join(
        &format!("127.0.0.1:{port}"),
        name,
        WorldInfo::default(),
        Duration::from_millis(10),
    )
    .unwrap();
    c.hardware_id = Some(derive_hardware_id(machine));
    c
}

fn actor(role: Role) -> Actor {
    Actor {
        label: "test".into(),
        role,
        origin: Origin::Local,
    }
}

fn run(host: &mut LanSession, adm: &ServerAdmin, role: Role, text: &str) -> omsi_admin::Outcome {
    let rules = ServerRules {
        real_time: false,
        traffic_max: 120,
        notice_ok: &|_| true,
        metar: false,
    };
    let action = Action::parse(text).expect("an action");
    match exec(host, adm, &actor(role), &action, None, &|_| None, &rules) {
        Exec::Done(o) => o,
        Exec::World => panic!("a session action"),
    }
}

#[test]
fn a_banned_machine_is_sent_away_stays_out_and_comes_back_when_lifted() {
    unsafe { std::env::set_var("OMSI_NO_BRIDGE", "1") };
    let dir = std::env::temp_dir().join(format!("omsi-hwban-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    omsi_lan::bans::install(BanStore::open(
        Some(dir.join("bans.json")),
        Some(dir.join("owner.json")),
    ));
    let mut host = LanSession::host(0, "host", WorldInfo::default(), true).unwrap();
    let port = host.local_addr().unwrap().port();
    let adm = ServerAdmin::default();

    let mut a = client(port, "Griefer", "machine-a");
    let mut b = client(port, "Friend", "machine-b");
    assert!(pump(&mut host, &mut [&mut a, &mut b], |h, _| h
        .peers()
        .count()
        == 2
        && h.peers().all(|p| h.peer_hardware_id(p.pose.id).is_some())));
    let ida = host
        .peers()
        .find(|p| p.pose.name == "Griefer")
        .unwrap()
        .pose
        .id;

    // a moderator bans for the session, not for all the owner's sessions
    let o = run(
        &mut host,
        &adm,
        Role::Moderator,
        &format!("hwban {ida} all griefing"),
    );
    assert_eq!(o.status, Status::Rejected, "{}", o.message);
    assert!(
        host.peers().any(|p| p.pose.id == ida),
        "refused: nobody touched"
    );
    let o = run(
        &mut host,
        &adm,
        Role::Moderator,
        &format!("hwban {ida} session 2d griefing"),
    );
    assert_eq!(o.status, Status::Applied, "{}", o.message);
    assert!(!host.peers().any(|p| p.pose.id == ida), "sent away");
    assert!(
        host.peers().any(|p| p.pose.name == "Friend"),
        "the other machine is not touched"
    );
    let id = {
        let s = omsi_lan::bans::global().unwrap();
        let bans = s.active(omsi_admin::unix_now());
        assert_eq!(bans.len(), 1);
        assert_eq!(
            (
                bans[0].scope,
                bans[0].reason.as_str(),
                bans[0].name.as_str()
            ),
            (Scope::Session, "griefing", "Griefer")
        );
        assert!(bans[0].until.is_some());
        bans[0].id.clone()
    };
    assert!(dir.join("bans.json").is_file(), "kept for the next run");

    // the same machine, a new game, another name: turned away at the door with the reason
    let mut a2 = client(port, "Somebody Else", "machine-a");
    assert!(pump(&mut host, &mut [&mut a2, &mut b], |_, c| c[0]
        .turned_away
        .is_some()));
    let why = a2.turned_away.clone().unwrap();
    assert!(why.contains("banned") && why.contains("griefing"), "{why}");
    assert!(!host.peers().any(|p| p.pose.name == "Somebody Else"));

    // only an admin lifts it; then the machine joins again
    assert_eq!(
        run(&mut host, &adm, Role::Moderator, &format!("unban {id}")).status,
        Status::Rejected
    );
    assert_eq!(
        run(&mut host, &adm, Role::Admin, &format!("unban {id}")).status,
        Status::Applied
    );
    assert_eq!(
        run(&mut host, &adm, Role::Admin, &format!("unban {id}")).status,
        Status::Rejected,
        "no such ban any more"
    );
    let mut a3 = client(port, "Griefer", "machine-a");
    assert!(pump(&mut host, &mut [&mut a3, &mut b], |h, _| h
        .peers()
        .any(|p| p.pose.name == "Griefer")));

    // the owner's list: an admin's ban for all sessions is in the file the other sessions read
    let idb = host
        .peers()
        .find(|p| p.pose.name == "Friend")
        .unwrap()
        .pose
        .id;
    assert_eq!(
        run(&mut host, &adm, Role::Admin, &format!("hwban {idb} all")).status,
        Status::Applied
    );
    assert!(
        std::fs::read_to_string(dir.join("owner.json"))
            .unwrap()
            .contains("\"scope\": \"owner\"")
    );
    let _ = std::fs::remove_dir_all(&dir);
}
