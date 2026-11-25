use super::*;
use crate::traffic_table::{Kind, Look, Meta, Row, TrafficTable};

#[test]
fn a_client_gets_the_hosts_traffic_over_real_sockets_and_the_host_hears_its_answers() {
    let mut host = LanSession::host(0, "host", world("m"), true).unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
    let mut c = LanSession::join_addr(vec![addr], None, "c", world("m")).unwrap();
    let poses = [pose(100.0), pose(100.0)];
    pump(&mut [&mut host, &mut c], &poses, 150, |s| s[1].connected);
    assert!(c.connected);
    let me = c.my_id;
    let at = [892_000.0, 4_196_000.0, 30.0];
    let mut table = TrafficTable::default();
    let put = |table: &mut TrafficTable, t: f64, cars: usize| {
        table.begin(t);
        for i in 0..cars {
            let row = Row::from_state(
                [at[0] + i as f64 * 7.0 + t * 10.0, at[1] + 5.0, at[2]],
                90.0,
                0.0,
                0.0,
                10.0,
                0.0,
                0,
                false,
                false,
                0,
            );
            let look = || Look {
                file: "Vehicles/Golf/golf.bus".into(),
                scheme: None,
                paint: String::new(),
            };
            table
                .upsert(i as u64, Kind::Car, look, Meta::default, row)
                .unwrap();
        }
        table.finish();
    };
    // (the session counts real seconds: the loop runs at the pace of its time steps)
    let mut t = 0.0;
    for step in 0..70 {
        t += 0.05;
        put(&mut table, t, if step < 45 { 12 } else { 8 });
        host.traffic_publish(&table, &[(me, at)], 0.05, 40_000.0);
        host.tick(0.05, &poses[0]);
        c.tick(0.05, &poses[1]);
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(c.traffic_client().len(), 8, "four cars were over");
    assert!(
        host.traffic_stats().acks_heard > 10,
        "the client answers: {:?}",
        host.traffic_stats()
    );
    let st = host.traffic_stats();
    assert!(st.delta_rows > 0, "changes of acknowledged rows: {st:?}");
    let id = table.id_of(3).unwrap();
    let p = c.traffic_client().newest(id).unwrap();
    assert!((p.pos[1] - (at[1] + 5.0)).abs() < 0.02 && (p.speed - 10.0).abs() < 0.1);
    assert_eq!(
        c.traffic_client()
            .look(c.traffic_client().get(id).unwrap())
            .unwrap()
            .file,
        "Vehicles/Golf/golf.bus"
    );
}
