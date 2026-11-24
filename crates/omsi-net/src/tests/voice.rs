use super::*;

fn voice_session() -> (LanSession, LanSession, LanSession, [Pose; 3]) {
    let mut host = LanSession::host(0, "host", world("m"), true).unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
    let mut a = LanSession::join_addr(vec![addr], None, "a", world("m")).unwrap();
    let mut b = LanSession::join_addr(vec![addr], None, "b", world("m")).unwrap();
    // host at 0, a at 20 m, b at 500 m
    let poses = [pose(0.0), pose(20.0), pose(500.0)];
    pump(&mut [&mut host, &mut a, &mut b], &poses, 200, |s| {
        s[1].connected && s[2].connected && s[0].peers().filter(|p| p.has_pose).count() == 2
    });
    assert!(a.connected && b.connected);
    // (both have voice chat on: they asked the host for its rules)
    host.set_voice_listener(a.my_id);
    host.set_voice_listener(b.my_id);
    (host, a, b, poses)
}

fn voice_round(host: &mut LanSession, a: &mut LanSession, b: &mut LanSession, poses: &[Pose; 3]) {
    for _ in 0..8 {
        host.tick(0.01, &poses[0]);
        a.tick(0.01, &poses[1]);
        b.tick(0.01, &poses[2]);
        std::thread::sleep(Duration::from_millis(3));
    }
}

#[test]
fn voice_is_relayed_by_range_and_the_radio_reaches_everybody() {
    let (mut host, mut a, mut b, poses) = voice_session();
    // nearby: a speaks, the host (20 m) hears, b (480 m) does not
    for seq in 0..5u16 {
        a.voice_send(voice_net::NEARBY, false, seq, &[seq as u8; 40]);
    }
    voice_round(&mut host, &mut a, &mut b, &poses);
    let at_host = host.take_voice();
    assert_eq!(
        at_host
            .iter()
            .map(|v| (v.from, v.channel, v.seq))
            .collect::<Vec<_>>(),
        (0..5).map(|s| (a.my_id, 0, s)).collect::<Vec<_>>()
    );
    assert!(at_host.iter().all(|v| v.payload == vec![v.seq as u8; 40]));
    assert!(b.take_voice().is_empty() && a.take_voice().is_empty());
    // the radio: b speaks, everybody else hears, b does not hear itself
    b.voice_send(voice_net::RADIO, false, 9, &[1, 2, 3]);
    b.voice_send(voice_net::RADIO, true, 10, &[]);
    voice_round(&mut host, &mut a, &mut b, &poses);
    for (who, s) in [("host", &mut host), ("a", &mut a)] {
        let got = s.take_voice();
        assert_eq!(
            got.iter()
                .map(|v| (v.from, v.channel, v.seq, v.end))
                .collect::<Vec<_>>(),
            vec![(b.my_id, 1, 9, false), (b.my_id, 1, 10, true)],
            "{who}"
        );
    }
    assert!(b.take_voice().is_empty());
    // the host speaks nearby: a hears
    host.voice_send(voice_net::NEARBY, false, 1, &[7; 10]);
    voice_round(&mut host, &mut a, &mut b, &poses);
    assert_eq!(a.take_voice().len(), 1);
    assert!(b.take_voice().is_empty());
}

#[test]
fn voice_policy_mutes_and_limits_are_enforced_by_the_host() {
    let (mut host, mut a, mut b, poses) = voice_session();
    // an administrator mutes a
    host.set_voice_muted(a.my_id, true);
    a.voice_send(voice_net::RADIO, false, 1, &[1]);
    voice_round(&mut host, &mut a, &mut b, &poses);
    assert!(host.take_voice().is_empty() && b.take_voice().is_empty());
    host.set_voice_muted(a.my_id, false);
    // the session without a radio
    host.set_voice_policy(VoicePolicy {
        radio: false,
        ..VoicePolicy::default()
    });
    a.voice_send(voice_net::RADIO, false, 2, &[1]);
    voice_round(&mut host, &mut a, &mut b, &poses);
    assert!(host.take_voice().is_empty() && b.take_voice().is_empty());
    host.set_voice_policy(VoicePolicy::default());
    // another client cannot speak with a's name
    let spoof = voice_net::encode(PROTOCOL as u8, voice_net::RADIO, false, a.my_id, 3, &[9]);
    b.send(
        &spoof,
        SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port())),
    );
    // an oversized frame is not sent at all
    a.voice_send(
        voice_net::RADIO,
        false,
        4,
        &vec![0; voice_net::MAX_PAYLOAD + 1],
    );
    voice_round(&mut host, &mut a, &mut b, &poses);
    assert!(host.take_voice().is_empty() && b.take_voice().is_empty() && a.take_voice().is_empty());
    // a flood is cut at the rate: far fewer than 1000 frames get through
    for seq in 0..1000u16 {
        a.voice_send(voice_net::RADIO, false, seq, &[0; 30]);
    }
    for _ in 0..40 {
        voice_round(&mut host, &mut a, &mut b, &poses);
    }
    let got = host.take_voice().len() + b.take_voice().len();
    assert!(got > 0 && got <= 2 * 200, "{got} frames got through");
}

#[test]
fn a_player_with_voice_chat_off_is_not_passed_the_frames() {
    let (mut host, mut a, mut b, poses) = voice_session();
    // (b never asked: its game has voice chat off)
    host.voice_listeners.remove(&b.my_id);
    a.voice_send(voice_net::RADIO, false, 1, &[1, 2, 3]);
    voice_round(&mut host, &mut a, &mut b, &poses);
    assert!(b.take_voice().is_empty());
    assert_eq!(host.take_voice().len(), 1);
}
