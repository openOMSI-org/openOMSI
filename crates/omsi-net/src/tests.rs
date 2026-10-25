//! Tests of the session (they open UDP sockets on the loopback).

use super::*;
mod bus_info;
mod freetex;

fn pose(x: f64) -> Pose {
    Pose {
        name: "p".into(),
        bus: "Vehicles/x.bus".into(),
        paint: "BVG 1990".into(),
        x,
        y: 2.0,
        z: 3.0,
        heading: 90.0,
        speed_kmh: 10.0,
        flags: FLAG_VEHICLE | FLAG_ENGINE | FLAG_BRAKE,
        head: 2,
        doors: vec![0.0, 0.5, 0.0, 0.0],
        blinker: 2,
        line: "37".into(),
        destination: "Hahneberg".into(),
        length: 11.5,
        width: 2.5,
        box_offset: -0.75,
        passengers: 12,
        ..Default::default()
    }
}

fn world(map: &str) -> WorldInfo {
    WorldInfo {
        map: map.into(),
        date: "1989-05-30".into(),
        time: 9.0 * 3600.0,
        weather: String::new(),
        season: String::new(),
    }
}

#[test]
fn an_info_with_everything_at_its_longest_fits_one_datagram() {
    let mut p = pose(1.5);
    p.id = 31;
    p.name = "Ж".repeat(40);
    p.bus = format!("Vehicles/{}/{}.bus", "Ü".repeat(60), "b".repeat(120));
    p.paint = "Ö".repeat(70);
    p.line = "Щ".repeat(20);
    p.destination = "Weiden (Oberpfalz) Bahnhof/ZOB – über Stockerhut ".repeat(3);
    p.tour = "ä".repeat(70);
    p.figure = format!("Humans/{}/{}.hum", "é".repeat(60), "f".repeat(120));
    p.texts = (0..MAX_TEXTS).map(|k| format!("{k}ß{}", "ñ".repeat(40))).collect();
    let text = p.encode_info();
    assert!(text.len() <= MAX_DATAGRAM, "{} bytes", text.len());
    let parts: Vec<&str> = text.split('|').collect();
    let q = Pose::decode_info(&parts).unwrap();
    assert_eq!(q.bus, p.bus);
    assert_eq!(q.figure, p.figure);
    assert!(q.texts.len() < MAX_TEXTS);
    // and an ordinary one keeps all its texts
    let mut o = pose(1.5);
    o.texts = (0..MAX_TEXTS).map(|k| format!("Text {k}")).collect();
    let parts_o = o.encode_info();
    let q = Pose::decode_info(&parts_o.split('|').collect::<Vec<_>>()).unwrap();
    assert_eq!(q.texts, o.texts);
}

#[test]
fn info_carries_the_freetex_pictures_and_an_older_info_has_none() {
    let mut p = pose(1.5);
    p.texts = vec!["17".into()];
    p.freetex = vec![
        r"..\..\Anzeigen\Rollband_FC\Paris\17.tga".into(),
        String::new(),
        r"..\..\Anzeigen\Rollband_FC\Paris\217.tga".into(),
    ];
    let text = p.encode_info();
    let q = Pose::decode_info(&text.split('|').collect::<Vec<_>>()).unwrap();
    assert_eq!(q.freetex, p.freetex);
    assert_eq!(q.texts, p.texts);
    // an older game's INFO ends with the figure: no pictures, everything else as before
    let older: Vec<&str> = text.split('|').take(14).collect();
    let q = Pose::decode_info(&older).unwrap();
    assert!(q.freetex.is_empty());
    assert_eq!(q.texts, p.texts);
    // and with everything else at its longest the INFO still fits one datagram
    p.freetex = (0..MAX_FREETEX).map(|k| format!("{k}{}", "é".repeat(200))).collect();
    p.bus = format!("Vehicles/{}/{}.bus", "Ü".repeat(60), "b".repeat(120));
    p.texts = (0..MAX_TEXTS).map(|k| format!("{k}ß{}", "ñ".repeat(40))).collect();
    assert!(p.encode_info().len() <= MAX_DATAGRAM);
}

#[test]
fn info_drops_freetex_pictures_that_leave_the_vehicle() {
    let mut p = pose(1.5);
    p.freetex = vec![r"\\attacker.example\s\x.bmp".into(), r"..\..\Anzeigen\17.tga".into(), "C:/x.bmp".into()];
    let text = p.encode_info();
    let q = Pose::decode_info(&text.split('|').collect::<Vec<_>>()).unwrap();
    assert_eq!(q.freetex, vec![String::new(), r"..\..\Anzeigen\17.tga".to_string(), String::new()]);
}

#[test]
fn info_round_trip_and_cleaning() {
    let mut p = pose(1.5);
    p.id = 7;
    p.table = 0xDEAD_BEEF;
    p.tour = "37/5".into();
    let text = p.encode_info();
    let parts: Vec<&str> = text.split('|').collect();
    let q = Pose::decode_info(&parts).unwrap();
    assert_eq!(
        (
            q.id,
            q.name.as_str(),
            q.bus.as_str(),
            q.paint.as_str(),
            q.line.as_str(),
            q.destination.as_str(),
            q.table
        ),
        (
            7,
            "p",
            "Vehicles/x.bus",
            "BVG 1990",
            "37",
            "Hahneberg",
            0xDEAD_BEEF
        )
    );
    assert_eq!((q.length, q.width, q.box_offset), (11.5, 2.5, -0.75));
    assert_eq!(q.tour, "37/5");
    // what a hostile or broken game might send
    let long = "x".repeat(500);
    let evil = format!("INFO|7|a\u{7}b{long}|../../etc/passwd.bus|p|l|d|11|2.5|0|0");
    let parts: Vec<&str> = evil.split('|').collect();
    let q = Pose::decode_info(&parts).unwrap();
    assert!(
        q.name.chars().count() <= MAX_NAME && !q.name.contains('\u{7}'),
        "{:?}",
        q.name
    );
    assert_eq!(q.bus, "", "a path out of the content folder is dropped");
    assert_eq!(q.tour, "", "an older game's INFO names no tour");
    for bad in [
        "INFO|7|a|Vehicles/x.bus|p|l|d|NaN|2.5|0|0",
        "INFO|7|a|Vehicles/x.bus|p|l|d|11|99|0|0",
        "INFO|7|a|Vehicles/x.bus|p|l|d|11|2.5|0",
        "INFO|x|a|Vehicles/x.bus|p|l|d|11|2.5|0|0",
        "INFO|7|a|Vehicles/x.bus|p|l|d|inf|2.5|0|0",
    ] {
        let parts: Vec<&str> = bad.split('|').collect();
        assert!(Pose::decode_info(&parts).is_none(), "{bad}");
    }
    // the box of a bus heading east whose centre lies 0.75 m behind its origin
    let f = p.footprint();
    assert!(
        (f.x - (1.5 - 0.75)).abs() < 1e-6 && (f.y - 2.0).abs() < 1e-6,
        "{f:?}"
    );
    assert_eq!(clean_text("  a|b\nc  ", 10), "a b c");
    assert_eq!(clean_text("ÄÖÜäöüß-long", 4), "ÄÖÜä");
}

#[test]
fn worlds_and_dates_from_the_network() {
    let text = "WELCOME|3|5|0000000000AB|host|maps/Grundorf/global.cfg|1989-12-24|64800.50|Weather/Schnee.owt|winter|3";
    let parts: Vec<&str> = text.split('|').collect();
    let w = WorldInfo::from_fields(&parts, 5);
    assert_eq!(
        w,
        WorldInfo {
            map: "maps/Grundorf/global.cfg".into(),
            date: "1989-12-24".into(),
            time: 64800.5,
            weather: "Weather/Schnee.owt".into(),
            season: "winter".into()
        }
    );
    for bad in [
        "1989-13-01",
        "1989-1-",
        "89-05-30",
        "1989/05/30",
        "x",
        "1989-05-30-1",
    ] {
        assert_eq!(clean_date(bad), "", "{bad}");
    }
    let parts = ["CLOCK", "m", "1989-05-30", "NaN", "", ""];
    assert_eq!(WorldInfo::from_fields(&parts, 1).time, 0.0);
    let parts = ["CLOCK", "m", "1989-05-30", "90000", "", ""];
    assert_eq!(WorldInfo::from_fields(&parts, 1).time, 3600.0);
}

#[test]
fn vehicle_paths_from_the_network() {
    assert_eq!(
        vehicle_path("Vehicles/MAN_NL_NG/MAN_EN92_main.bus").as_deref(),
        Some("Vehicles/MAN_NL_NG/MAN_EN92_main.bus")
    );
    assert_eq!(
        vehicle_path("Vehicles\\Golf\\golf.OVH").as_deref(),
        Some("Vehicles/Golf/golf.OVH")
    );
    for bad in [
        "",
        "../../../../../../../../../../dev/zero",
        "Vehicles/../../../etc/passwd.bus",
        "Vehicles/..\\..\\x.bus",
        "/dev/zero",
        "/Users/x/a.bus",
        "C:/Windows/x.bus",
        "C:x.bus",
        "\\\\server\\share\\x.bus",
        "Vehicles/./x.bus",
        "Vehicles//x.bus",
        "Vehicles/.../x.bus",
        "Vehicles/x.bus/",
        "Vehicles/x.cfg",
        "Vehicles/x",
        "Vehicles/a.bus/b",
        "Vehicles/x\u{0}.bus",
        "Vehicles/x|y.bus",
    ] {
        assert_eq!(vehicle_path(bad), None, "{bad:?}");
    }
    assert_eq!(
        vehicle_path(&format!("Vehicles/{}.bus", "a".repeat(300))),
        None
    );
}

#[test]
fn addresses_are_told_apart() {
    use addrs::{classify, AddrKind};
    let ip = |s: &str| s.parse::<Ipv4Addr>().unwrap();
    // the interfaces of the Mac this was written on: Hamachi, Tailscale, wifi
    assert_eq!(classify(ip("25.34.223.28"), "en9"), AddrKind::Hamachi);
    assert_eq!(classify(ip("100.74.245.63"), "utun1"), AddrKind::Tailscale);
    assert_eq!(classify(ip("192.168.1.174"), "en1"), AddrKind::Lan);
    assert_eq!(classify(ip("26.10.0.4"), "Radmin VPN"), AddrKind::Radmin);
    assert_eq!(classify(ip("10.147.17.3"), "ztks57ab3"), AddrKind::ZeroTier);
    assert_eq!(classify(ip("172.17.0.1"), "docker0"), AddrKind::Virtual);
    assert_eq!(classify(ip("192.168.64.1"), "bridge100"), AddrKind::Virtual);
    assert_eq!(classify(ip("169.254.12.1"), "en5"), AddrKind::LinkLocal);
    assert_eq!(classify(ip("10.8.0.2"), "utun3"), AddrKind::Vpn);
    assert_eq!(classify(ip("81.2.3.4"), "eth0"), AddrKind::Public);
    // Windows, in Russian (the adapter names come through as they can)
    let text = [
        "",
        "Настройка протокола IP для Windows",
        "",
        "Адаптер Ethernet Hamachi:",
        "",
        "   DNS-суффикс подключения . . . . . :",
        "   IPv4-адрес. . . . . . . . . . . . : 25.61.2.10",
        "   Маска подсети . . . . . . . . . . : 255.0.0.0",
        "   Основной шлюз. . . . . . . . . : 25.0.0.1",
        "",
        "Адаптер беспроводной локальной сети Беспроводная сеть:",
        "",
        "   IPv4-адрес. . . . . . . . . . . . : 192.168.0.105(Основной)",
        "   Маска подсети . . . . . . . . . . : 255.255.255.0",
        "",
        "Адаптер Ethernet vEthernet (WSL):",
        "",
        "   IPv4-адрес. . . . . . . . . . . . : 172.29.160.1",
        "   Маска подсети . . . . . . . . . . : 255.255.240.0",
    ]
    .join("\r\n");
    let list = addrs::parse_ipconfig(&text);
    let got: Vec<(Ipv4Addr, AddrKind, Option<Ipv4Addr>)> =
        list.iter().map(|a| (a.ip, a.kind, a.broadcast)).collect();
    assert_eq!(
        got,
        vec![
            (ip("25.61.2.10"), AddrKind::Hamachi, Some(ip("25.255.255.255"))),
            (ip("192.168.0.105"), AddrKind::Lan, Some(ip("192.168.0.255"))),
            (ip("172.29.160.1"), AddrKind::Virtual, Some(ip("172.29.175.255"))),
        ]
    );
}

#[test]
fn only_nearby_games_hear_where_the_host_is() {
    let at = |s: &str| s.parse::<SocketAddr>().unwrap();
    assert!(addrs::is_local_peer(at("127.0.0.1:5000")));
    assert!(addrs::is_local_peer(at("192.168.1.20:5000")));
    assert!(addrs::is_local_peer(at("100.74.245.63:5000")));
    assert!(!addrs::is_local_peer(at("[2001:db8::1]:5000")));
    let far = at("198.51.100.77:5000");
    let near = addrs::local_addresses().iter().any(|l| l.ip.octets()[..3] == [198, 51, 100]);
    assert_eq!(addrs::is_local_peer(far), near);
}

#[test]
fn this_machines_addresses() {
    // whatever this machine has: no loopback, no link-local, a VPN before the LAN
    let list = addrs::local_addresses();
    for a in &list {
        eprintln!("{:<16} {:<12} {}", a.ip, a.label(), a.interface);
        assert!(!a.ip.is_loopback());
    }
    let ranks: Vec<usize> = code_ipv4s()
        .iter()
        .filter_map(|ip| list.iter().position(|a| a.ip == *ip))
        .collect();
    assert!(ranks.windows(2).all(|w| w[0] < w[1]), "{ranks:?}");
    assert!(code_ipv4s().len() <= MAX_CODE_ADDRS);
}

#[test]
fn session_codes() {
    let c = SessionCode::single(
        PROTOCOL as u8,
        Ipv4Addr::new(192, 168, 178, 23),
        27016,
        0xA1B2_C3D4_E5F6,
    );
    let text = c.encode();
    assert!(text.starts_with("OMSI-"), "{text}");
    assert_eq!(text.len(), 5 + CODE_CHARS + 5, "{text}");
    assert!(!text[5..].contains(['0', '1', 'O', 'I']), "{text}");
    assert_eq!(SessionCode::decode(&text).unwrap(), c);
    // typed sloppily: lower case, no prefix, spaces
    let sloppy = text[5..].to_lowercase().replace('-', " ");
    assert_eq!(SessionCode::decode(&sloppy).unwrap(), c);
    // one character changed: the checksum catches it
    let mut bad: Vec<char> = text.chars().collect();
    let i = 11;
    bad[i] = if bad[i] == 'A' { 'B' } else { 'A' };
    let bad: String = bad.into_iter().collect();
    assert!(SessionCode::decode(&bad).unwrap_err().contains("typo"));
    assert!(SessionCode::decode("OMSI-ABCD").is_err());
    assert!(SessionCode::decode(&text.replace('A', "0")).is_err() || !text.contains('A'));
    // two sessions of one computer: different from the first group on (the address
    // used to come first, so 14 characters were the same)
    let other = SessionCode {
        session: 0xA1B2_C3D4_E5F7,
        ..c.clone()
    };
    let (a, b) = (c.encode(), other.encode());
    assert_ne!(a[5..9], b[5..9], "{a} {b}");
    let same = a.chars().zip(b.chars()).filter(|(x, y)| x == y).count();
    assert!(same < 12, "{a} {b}");
    for id in [1u64, 42, 0xFFFF_FFFF_FFFF, random_session_id()] {
        let k = SessionCode {
            session: id,
            ..c.clone()
        };
        assert_eq!(SessionCode::decode(&k.encode()).unwrap(), k);
    }
    // a code of the first layout is still read
    assert_eq!(SessionCode::decode(&c.encode_first_layout()).unwrap(), c);
    // a code of the previous protocol is refused with a clear message
    let old = SessionCode {
        protocol: 2,
        ..c.clone()
    };
    assert!(describe_join(&old.encode())
        .unwrap_err()
        .contains("protocol 2"));
    assert!(
        LanSession::join(&old.encode(), "x", world("m"), Duration::from_millis(10))
            .err()
            .unwrap()
            .contains("same version")
    );
    // several addresses: Hamachi, Tailscale and the LAN, as on the Mac this was written on
    let multi = SessionCode {
        protocol: PROTOCOL as u8,
        ips: vec![
            Ipv4Addr::new(25, 34, 223, 28),
            Ipv4Addr::new(100, 74, 245, 63),
            Ipv4Addr::new(192, 168, 1, 174),
        ],
        port: 27015,
        session: 0x49A5_0EA5_8A1F,
    };
    let text = multi.encode();
    // (37 characters, the last group filled up to four: 40)
    assert_eq!(text.replace('-', "").len(), 4 + 40, "{text}");
    assert!(text.split('-').all(|g| g.len() == 4), "{text}");
    assert!(!text[5..].contains(['0', '1', 'O', 'I']), "{text}");
    assert_eq!(SessionCode::decode(&text).unwrap(), multi);
    assert_eq!(
        SessionCode::decode(&text[5..].to_lowercase().replace('-', " ")).unwrap(),
        multi
    );
    let two = SessionCode {
        ips: multi.ips[..2].to_vec(),
        ..multi.clone()
    };
    assert_eq!(two.encode().replace('-', "").len(), 4 + 32);
    // the code without the filling (as an older game wrote it) is read the same
    let short: String = two.encode().replace('-', "")[4..35].to_string();
    assert_eq!(SessionCode::decode(&short).unwrap(), two);
    assert!(looks_like_code(&two.encode()) && looks_like_code(&short));
    assert_eq!(SessionCode::decode(&two.encode()).unwrap(), two);
    // every character matters in the long codes too
    let chars: Vec<char> = text.chars().collect();
    for i in (5..chars.len()).filter(|i| chars[*i] != '-') {
        let mut bad = chars.clone();
        // (its top bit, which is always data: the lowest bits of the last character may be
        // the zeros that fill up the last byte, and changing them changes nothing)
        let k = ALPHABET.iter().position(|c| *c as char == bad[i]).unwrap();
        bad[i] = ALPHABET[k ^ 16] as char;
        let bad: String = bad.into_iter().collect();
        assert_ne!(SessionCode::decode(&bad).ok(), Some(multi.clone()), "{bad}");
    }
    // cut short while copying
    let short = &text[..text.len() - 5];
    assert!(looks_like_code(short));
    assert!(SessionCode::decode(short).unwrap_err().contains("copy the whole code"));
    let d = describe_join(&text).unwrap();
    assert!(d.contains("25.34.223.28:27015 (Hamachi)") && d.contains("192.168.1.174:27015 (LAN)"), "{d}");
    assert_eq!(
        parse_join(&text).unwrap(),
        JoinTarget::Direct {
            addrs: vec![
                "25.34.223.28:27015".parse().unwrap(),
                "100.74.245.63:27015".parse().unwrap(),
                "192.168.1.174:27015".parse().unwrap()
            ],
            session: Some(0x49A5_0EA5_8A1F),
            protocol: Some(PROTOCOL as u8)
        }
    );
}

#[test]
fn join_targets() {
    assert_eq!(parse_join("").unwrap(), JoinTarget::Discover);
    assert_eq!(parse_join(" auto ").unwrap(), JoinTarget::Discover);
    assert_eq!(
        parse_join("27015").unwrap(),
        JoinTarget::Direct {
            addrs: vec!["127.0.0.1:27015".parse().unwrap()],
            session: None,
            protocol: None
        }
    );
    assert_eq!(
        parse_join("192.168.1.20").unwrap(),
        JoinTarget::Direct {
            addrs: vec!["192.168.1.20:27015".parse().unwrap()],
            session: None,
            protocol: None
        }
    );
    assert_eq!(
        parse_join("192.168.1.20:27020").unwrap(),
        JoinTarget::Direct {
            addrs: vec!["192.168.1.20:27020".parse().unwrap()],
            session: None,
            protocol: None
        }
    );
    assert_eq!(
        parse_join("localhost:27017").unwrap(),
        JoinTarget::Direct {
            addrs: vec!["127.0.0.1:27017".parse().unwrap()],
            session: None,
            protocol: None
        }
    );
    assert!(parse_join("99999").is_err());
    let c = SessionCode::single(PROTOCOL as u8, Ipv4Addr::new(10, 0, 0, 7), 27015, 42);
    assert_eq!(
        parse_join(&c.encode()).unwrap(),
        JoinTarget::Direct {
            addrs: vec!["10.0.0.7:27015".parse().unwrap()],
            session: Some(42),
            protocol: Some(PROTOCOL as u8)
        }
    );
    assert!(parse_join("OMSI-XXXX-YYYY").is_err());
    // what the user typed once: a port twice
    assert!(parse_join("27015:27015").is_err());
    assert!(describe_join("27015:27015").is_err());
    assert!(describe_join("").unwrap().contains("search"));
    assert!(describe_join("27016")
        .unwrap()
        .contains("this computer, port 27016"));
    assert!(describe_join(&c.encode())
        .unwrap()
        .contains("10.0.0.7:27015"));
    assert!(describe_join("OMSI-ABCD-EFGH").is_err());
    assert!(describe_join("bus-pc.local:27020")
        .unwrap()
        .contains("bus-pc.local, port 27020"));
    assert!(describe_join("hello world").is_err());
}

fn pump(
    sessions: &mut [&mut LanSession],
    poses: &[Pose],
    rounds: usize,
    until: impl Fn(&[&mut LanSession]) -> bool,
) {
    for _ in 0..rounds {
        for (s, p) in sessions.iter_mut().zip(poses) {
            s.tick(0.06, p);
        }
        if until(sessions) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn find(s: &LanSession, x: f64) -> Option<&Peer> {
    s.peers()
        .find(|p| p.has_pose && p.has_info && (p.pose.x - x).abs() < 0.01)
}

#[test]
fn animation_snapshots_keep_owner_timestamps_when_relayed_between_clients() {
    let mut host = LanSession::host(0, "host", world("m"), true).unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
    let mut a = LanSession::join_addr(vec![addr], None, "a", world("m")).unwrap();
    let mut b = LanSession::join_addr(vec![addr], None, "b", world("m")).unwrap();
    let poses = [pose(100.0), pose(200.0), pose(300.0)];
    pump(&mut [&mut host, &mut a, &mut b], &poses, 150, |s| {
        find(s[2], 200.0).is_some() && find(s[1], 100.0).is_some()
    });
    assert!(a.connected && b.connected);
    a.frame_ms = Some(1234.0);
    host.frame_ms = Some(5678.0);
    a.send_animation_vars(99, &[(5, 0.25)], 0.1);
    host.send_animation_vars(99, &[(5, 0.75)], 0.1);
    let mut received = [false; 3];
    for _ in 0..30 {
        host.tick(0.01, &poses[0]);
        a.tick(0.01, &poses[1]);
        b.tick(0.01, &poses[2]);
        let client_id = a.my_id;
        let host_id = host.my_id;
        for (slot, session, owner, ms, value) in [
            (0, &mut host, client_id, 1234, 0.25),
            (1, &mut a, host_id, 5678, 0.75),
            (2, &mut b, client_id, 1234, 0.25),
        ] {
            for state in session.take_vars() {
                if state.id == owner && state.sent_ms == ms {
                    assert_eq!(state.floats, vec![(5, value)]);
                    received[slot] = true;
                }
            }
        }
        if received.iter().all(|got| *got) { break; }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(received, [true; 3], "host, direct client and relayed client must get the owner's frame clock");
}

#[test]
fn animation_snapshots_have_their_own_budget_and_validate_the_sender() {
    let mut host = LanSession::host(0, "host", world("m"), true).unwrap();
    let from: SocketAddr = "127.0.0.1:29001".parse().unwrap();
    let mut peer = Peer::new(Some(from), pose(1.0));
    peer.states.tokens = 0.0;
    peer.variables.tokens = 0.0;
    host.peers.insert(7, peer);
    let mut sender = vars::AnimationSender::default();
    let packets = sender.tick(PROTOCOL as u8, 7, 99, 1234, &[(0, 0.5)], 0.1);
    host.on_vars(&packets[0], "127.0.0.1:29002".parse().unwrap());
    assert!(host.take_vars().is_empty());
    host.on_vars(&packets[0], from);
    let received = host.take_vars();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].sent_ms, 1234);
    assert_eq!(received[0].floats, vec![(0, 0.5)]);
    host.peers.get_mut(&7).unwrap().animations.tokens = 0.0;
    host.on_vars(&packets[0], from);
    assert!(host.take_vars().is_empty());
}

#[test]
fn variable_recovery_has_its_own_rate_budget_and_checks_the_sender() {
    let mut host = LanSession::host(0, "host", world("m"), true).unwrap();
    let from: SocketAddr = "127.0.0.1:29001".parse().unwrap();
    let mut peer = Peer::new(Some(from), pose(1.0));
    peer.states.tokens = 0.0;
    host.peers.insert(7, peer);
    let mut sender = vars::VarSender::default();
    let messages = sender.tick(PROTOCOL as u8, 7, 99, &[0], &[42.0], &[], &[], 0.1);
    host.on_vars(&messages[0], "127.0.0.1:29002".parse().unwrap());
    assert!(host.take_vars().is_empty(), "a different address cannot send for this peer");
    host.on_vars(&messages[0], from);
    let received = host.take_vars();
    assert_eq!(received.len(), 1, "pose traffic must not starve variables");
    assert_eq!(received[0].floats, vec![(0, 42.0)]);
    host.peers.get_mut(&7).unwrap().variables.tokens = 0.0;
    host.on_vars(&messages[0], from);
    assert!(host.take_vars().is_empty(), "variable traffic is still rate limited");
}

#[test]
fn host_and_clients_exchange_complete_variable_tables() {
    let mut host = LanSession::host(0, "host", world("m"), true).unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
    let mut a = LanSession::join_addr(vec![addr], None, "a", world("m")).unwrap();
    let mut b = LanSession::join_addr(vec![addr], None, "b", world("m")).unwrap();
    let poses = [pose(100.0), pose(200.0), pose(300.0)];
    pump(&mut [&mut host, &mut a, &mut b], &poses, 150, |s| {
        find(s[2], 200.0).is_some() && find(s[1], 100.0).is_some()
    });
    assert!(a.connected && b.connected);
    let ids: Vec<u16> = (0..600).map(|k| k * 2).collect();
    let values: Vec<f32> = (0..600).map(|k| k as f32 + 0.5).collect();
    let string_ids: Vec<u16> = (0..20).collect();
    let strings: Vec<String> = (0..20).map(|k| format!("{k}:{}", "árvíz".repeat(if k == 19 { 600 } else { 60 }))).collect();
    let schema = var_schema::Schema { floats: (0..300).map(|i| format!("variable_{i:04}")).collect(), ..Default::default() };
    let schema_text = schema.encode().unwrap();
    let mut received_schemas = [false; 3];
    let mut received = [vars::VarsIn::default(), vars::VarsIn::default(), vars::VarsIn::default()];
    for _ in 0..30 {
        host.send_var_schema(schema.table_hash(), &schema_text, 0.1);
        a.send_var_schema(schema.table_hash(), &schema_text, 0.1);
        host.send_vars(99, &ids, &values, &string_ids, &strings, 0.1);
        a.send_vars(99, &ids, &values, &string_ids, &strings, 0.1);
        host.tick(0.1, &poses[0]);
        a.tick(0.1, &poses[1]);
        b.tick(0.1, &poses[2]);
        let a_id = a.my_id;
        let host_id = host.my_id;
        for (slot, session, owner) in [(0, &mut host, a_id), (1, &mut a, host_id), (2, &mut b, a_id)] {
            for manifest in session.take_var_schemas() {
                if manifest.id != owner { continue; }
                for (_, text) in manifest.strings {
                    assert_eq!(var_schema::Schema::decode(&text), Some(schema.clone()));
                    received_schemas[slot] = true;
                }
            }
        }
        for v in host.take_vars().into_iter().filter(|v| v.id == a.my_id) {
            assert_eq!(v.table, 99);
            received[0].floats.extend(v.floats);
            received[0].strings.extend(v.strings);
        }
        for v in a.take_vars().into_iter().filter(|v| v.id == host.my_id) {
            received[1].floats.extend(v.floats);
            received[1].strings.extend(v.strings);
        }
        for v in b.take_vars().into_iter().filter(|v| v.id == a.my_id) {
            received[2].floats.extend(v.floats);
            received[2].strings.extend(v.strings);
        }
    }
    assert_eq!(received_schemas, [true; 3]);
    for got in received {
        let floats: std::collections::HashMap<_, _> = got.floats.into_iter().collect();
        let texts: std::collections::HashMap<_, _> = got.strings.into_iter().collect();
        for (&id, &value) in ids.iter().zip(&values) {
            assert_eq!(floats.get(&id), Some(&value));
        }
        for (&id, value) in string_ids.iter().zip(&strings) {
            assert_eq!(texts.get(&id), Some(value));
        }
    }
}

#[test]
fn clients_receive_ai_script_context_only_from_the_host() {
    let mut host = LanSession::host(0, "host", world("m"), true).unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
    let mut a = LanSession::join_addr(vec![addr], None, "a", world("m")).unwrap();
    let mut b = LanSession::join_addr(vec![addr], None, "b", world("m")).unwrap();
    let poses = [pose(100.0), pose(200.0), pose(300.0)];
    pump(&mut [&mut host, &mut a, &mut b], &poses, 150, |s| s[1].connected && s[2].connected && s[0].peers.len() == 2);
    let context = ai_context::Context {
        active: 1.0, line: "042".into(), terminus: 3,
        stops: (0..100).map(|i| (i, format!("árvíztűrő bus stop {i}"), i as f32 * 60.0, i as f32 * 60.0 + 10.0)).collect(),
        index: 5, delay: -12.5, system: [42.0; 22],
    };
    let mut receiver = ai_context::Receiver::default();
    let mut received = None;
    for page in 0..20 {
        host.send_ai_context(a.my_id, &context.packet(PROTOCOL as u8, 7, page as u32, page).unwrap());
        a.tick(0.1, &poses[1]);
        for packet in a.take_ai_contexts() {
            if let Some((id, value)) = receiver.receive(&packet, PROTOCOL as u8) { assert_eq!(id, 7); received = Some(value); }
        }
    }
    assert_eq!(received, Some(context.clone()));
    let packet = context.packet(PROTOCOL as u8, 8, 1, 0).unwrap();
    // A peer cannot publish shared AI state to another peer, or to the host.
    b.send(&packet, to_client(&a));
    b.send(&packet, addr);
    a.tick(0.1, &poses[1]);
    host.tick(0.1, &poses[0]);
    assert!(a.take_ai_contexts().is_empty());
    assert!(host.take_ai_contexts().is_empty());
}

#[test]
fn script_textures_initialize_and_update_through_the_host_in_both_directions() {
    use scripttex::Image;
    use std::sync::Arc;
    let mut host = LanSession::host(0, "host", world("m"), true).unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
    let mut a = LanSession::join_addr(vec![addr], None, "a", world("m")).unwrap();
    let mut b = LanSession::join_addr(vec![addr], None, "b", world("m")).unwrap();
    let poses = [pose(100.0), pose(200.0), pose(300.0)];
    pump(&mut [&mut host, &mut a, &mut b], &poses, 150, |s| {
        find(s[2], 200.0).is_some() && find(s[1], 100.0).is_some()
    });
    assert!(a.connected && b.connected);
    let host_image = Image { width: 2, height: 1, mipmaps: false, rgba: Arc::new(vec![1, 2, 3, 4, 5, 6, 7, 8]) };
    let mut client_image = Image { width: 64, height: 64, mipmaps: true, rgba: Arc::new((0..64 * 64 * 4).map(|k| ((k * 107 + k / 13) % 256) as u8).collect()) };
    host.set_script_textures(7, vec![Some(host_image.clone())]);
    a.set_script_textures(8, vec![Some(client_image.clone())]);
    host.subscribe_script_textures(a.my_id, 8);
    a.subscribe_script_textures(host.my_id, 7);
    b.subscribe_script_textures(host.my_id, 7);
    b.subscribe_script_textures(a.my_id, 8);
    pump(&mut [&mut host, &mut a, &mut b], &poses, 60, |_| false);
    assert_eq!(host.take_script_textures()[0].image, client_image);
    assert_eq!(a.take_script_textures()[0].image, host_image);
    let received = b.take_script_textures();
    assert_eq!(received.len(), 2);
    assert_eq!(received.iter().find(|i| i.owner == a.my_id).unwrap().image, client_image);
    assert_eq!(received.iter().find(|i| i.owner == host.my_id).unwrap().image, host_image);
    Arc::make_mut(&mut client_image.rgba)[3] = 123;
    a.set_script_textures(8, vec![Some(client_image.clone())]);
    pump(&mut [&mut host, &mut a, &mut b], &poses, 30, |_| false);
    assert_eq!(host.take_script_textures()[0].image, client_image);
    assert_eq!(b.take_script_textures()[0].image, client_image);
    assert!(a.take_script_textures().is_empty(), "unchanged host textures are not resent");
}

#[test]
fn host_and_two_clients_exchange_states() {
    let mut host =
        LanSession::host(28100, "host", world("maps/Grundorf/global.cfg"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    let code = host.code().unwrap().encode();
    let mut a = LanSession::join(
        &code,
        "a",
        world("maps/Grundorf/global.cfg"),
        Duration::from_secs(1),
    )
    .unwrap();
    let mut b = LanSession::join(
        &port.to_string(),
        "b",
        world("maps/Grundorf/global.cfg"),
        Duration::from_secs(1),
    )
    .unwrap();
    let mut pa = pose(200.0);
    pa.rear = vec![PartPose {
        x: 192.0,
        y: 2.5,
        z: 3.0,
        heading: 88.0,
    }];
    pa.lamps = vec![1.0, 0.0, 1.0 / 3.0];
    pa.values = vec![1850.0, 0.25];
    let poses = [pose(100.0), pa, pose(300.0)];
    pump(&mut [&mut host, &mut a, &mut b], &poses, 150, |s| {
        find(s[2], 200.0).is_some() && find(s[1], 100.0).is_some() && find(s[1], 300.0).is_some()
    });
    assert!(a.connected && b.connected, "clients connected");
    assert_ne!(a.my_id, b.my_id);
    let h = find(&a, 100.0).expect("client sees the host's bus");
    assert_eq!(
        (
            h.pose.name.as_str(),
            h.pose.destination.as_str(),
            h.pose.line.as_str(),
            h.pose.passengers,
            h.pose.blinker
        ),
        ("host", "Hahneberg", "37", 12, 2)
    );
    assert!(
        h.pose.flags & FLAG_BRAKE != 0 && (h.pose.doors[1] - 0.5).abs() < 0.04,
        "{:?}",
        h.pose
    );
    let other = find(&b, 200.0).expect("client sees the other client's bus through the host");
    assert_eq!(other.pose.name, "a");
    assert_eq!(other.pose.rear.len(), 1);
    assert!((other.pose.rear[0].x - 192.0).abs() < 0.01);
    assert_eq!(other.pose.lamps, vec![1.0, 0.0, 1.0 / 3.0]);
    assert_eq!(other.pose.values, vec![1850.0, 0.25]);
    assert_eq!(host.peer_count(), 2);
    assert!(a.warnings.is_empty(), "{:?}", a.warnings);
    assert_eq!(a.session, host.session);
    // the others were told who joined, and the newcomer who is here
    let notes: Vec<String> = b
        .take_events()
        .into_iter()
        .filter_map(|e| {
            if let LanEvent::Notice(n) = e {
                Some(n)
            } else {
                None
            }
        })
        .collect();
    assert!(
        notes
            .iter()
            .any(|n| n.contains("In this session") && n.contains("host")),
        "{notes:?}"
    );
    let host_notes = host.take_events();
    assert!(
        host_notes
            .iter()
            .any(|e| matches!(e, LanEvent::Notice(n) if n == "a joined with x")),
        "{host_notes:?}"
    );
    // a state is small: the moving bus with its rear section, lamps and values
    let data = wire::encode_state(&poses[1], PROTOCOL as u8, 1);
    assert!(data.len() < 64, "{} bytes", data.len());
}

#[test]
fn a_client_takes_the_hosts_world_and_clock() {
    let mut hw = world("maps/Grundorf/global.cfg");
    hw.weather = "Weather/Schmuddelwetter.owt".into();
    hw.season = "winter".into();
    hw.date = "1990-01-15".into();
    let mut host = LanSession::host(28110, "host", hw.clone(), true).unwrap();
    host.set_clock("1990-01-15", 17.5 * 3600.0);
    let port = host.local_addr().unwrap().port();
    let mut c = LanSession::join(
        &port.to_string(),
        "c",
        world("maps/Berlin-Spandau/global.cfg"),
        Duration::from_secs(1),
    )
    .unwrap();
    for s in [&mut host, &mut c] {
        s.heartbeat = 0.05;
    }
    host.clock_acc = -1.0e9;
    pump(&mut [&mut host, &mut c], &[pose(1.0), pose(2.0)], 80, |s| {
        s[1].welcome.is_some()
    });
    let w = c.welcome.clone().expect("welcome");
    assert_eq!(w.world.date, "1990-01-15");
    assert_eq!(w.world.time, 17.5 * 3600.0);
    assert_eq!(
        (w.world.weather.as_str(), w.world.season.as_str()),
        ("Weather/Schmuddelwetter.owt", "winter")
    );
    assert_eq!(c.welcomes, 1);
    let clock = c.take_host_clock().expect("the welcome sets the clock");
    assert!(clock.time_now() >= 17.5 * 3600.0);
    assert!(
        c.warnings.iter().any(|w| w.contains("Grundorf")),
        "map mismatch warned: {:?}",
        c.warnings
    );
    // the game takes the world (but stays on its map): only the map is still warned about
    let mut mine = w.world.clone();
    mine.map = "maps/Berlin-Spandau/global.cfg".into();
    c.set_world(mine);
    assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
    assert!(c.warnings[0].contains("will not meet"));
    // the host's clock goes on; the next CLOCK says so
    host.set_clock("1990-01-15", 18.0 * 3600.0);
    host.clock_acc = CLOCK_EVERY;
    pump(&mut [&mut host, &mut c], &[pose(1.0), pose(2.0)], 40, |s| {
        s[1].host_clock.is_some()
    });
    let clock = c.take_host_clock().expect("clock message");
    assert_eq!(clock.world.time, 18.0 * 3600.0);
    assert_eq!(clock.world.date, "1990-01-15");
}

#[test]
fn host_weather_changes_are_sent_immediately_in_clock() {
    let mut host = LanSession::host(27921, "host", world("maps/Grundorf/global.cfg"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    let mut client = LanSession::join(
        &port.to_string(),
        "client",
        world("maps/Grundorf/global.cfg"),
        Duration::from_secs(1),
    )
    .unwrap();
    for s in [&mut host, &mut client] {
        s.heartbeat = 0.05;
    }
    pump(&mut [&mut host, &mut client], &[pose(1.0), pose(2.0)], 80, |s| s[1].connected);
    let _ = client.take_host_clock();

    let custom = "custom:vis=5000;br=0.80;wd=90;ws=4.0;t=22.0;rh=70;p=1013;c=2;cb=120;pt=1;pi=80;wet=0.50;snow=0;snowroad=0";
    host.clock_acc = 0.0;
    host.set_weather(custom);
    assert!(host.clock_acc >= CLOCK_EVERY, "weather change must force the next CLOCK");
    pump(&mut [&mut host, &mut client], &[pose(1.0), pose(2.0)], 20, |s| s[1].host_clock.is_some());
    assert_eq!(client.take_host_clock().expect("custom weather clock").world.weather, custom);

    // the same weather again sends nothing new
    host.clock_acc = 0.0;
    host.set_weather(custom);
    assert_eq!(host.clock_acc, 0.0);

    host.set_weather("");
    assert!(host.clock_acc >= CLOCK_EVERY, "clearing weather must force the next CLOCK");
    pump(&mut [&mut host, &mut client], &[pose(1.0), pose(2.0)], 20, |s| s[1].host_clock.is_some());
    assert_eq!(client.take_host_clock().expect("default weather clock").world.weather, "");
}

#[test]
fn the_host_lists_the_vehicles_at_a_clients_bus() {
    let mut host =
        LanSession::host(28120, "host", world("maps/Grundorf/global.cfg"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    host.set_local_footprints(vec![
        Footprint {
            x: 100.0,
            y: 2.0,
            z: 3.0,
            heading: 90.0,
            length: 12.0,
            width: 2.5,
        },
        Footprint {
            x: 5000.0,
            y: 0.0,
            z: 0.0,
            heading: 0.0,
            length: 4.0,
            width: 2.0,
        },
    ]);
    let mut c = LanSession::join(
        &format!("127.0.0.1:{port}"),
        "c",
        world("maps/Grundorf/global.cfg"),
        Duration::from_secs(1),
    )
    .unwrap();
    let poses = [pose(100.0), pose(101.0)];
    pump(&mut [&mut host, &mut c], &poses, 80, |s| s[1].connected);
    c.request_near(Footprint {
        x: 101.0,
        y: 2.0,
        z: 3.0,
        heading: 90.0,
        length: 12.0,
        width: 2.5,
    });
    pump(&mut [&mut host, &mut c], &poses, 80, |s| {
        s[1].near.is_some()
    });
    let near = c.near.as_ref().expect("near");
    assert_eq!(near.len(), 1, "only the vehicle near the spawn: {near:?}");
    assert_eq!(near[0].x, 100.0);
}

#[test]
fn the_list_has_the_other_players_boxes() {
    let mut host = LanSession::host(28130, "host", world("m"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    host.set_local_footprints(vec![]);
    // A drives an articulated bus: an 18 m box whose centre is 3.5 m behind its origin
    let mut a =
        LanSession::join(&port.to_string(), "a", world("m"), Duration::from_secs(1)).unwrap();
    let mut pose_a = pose(200.0);
    pose_a.length = 18.0;
    pose_a.box_offset = -3.5;
    let far = pose(900.0);
    pump(
        &mut [&mut host, &mut a],
        &[far.clone(), pose_a.clone()],
        120,
        |s| s[0].peers().any(|p| p.has_pose && p.has_info),
    );
    // B's bus stands where A stands
    let mut b =
        LanSession::join(&port.to_string(), "b", world("m"), Duration::from_secs(1)).unwrap();
    pump(
        &mut [&mut host, &mut a, &mut b],
        &[far.clone(), pose_a.clone(), pose(201.0)],
        120,
        |s| s[2].connected,
    );
    b.request_near(Footprint {
        x: 201.0,
        y: 2.0,
        z: 3.0,
        heading: 90.0,
        length: 12.0,
        width: 2.5,
    });
    pump(
        &mut [&mut host, &mut a, &mut b],
        &[far, pose_a, pose(201.0)],
        120,
        |s| s[2].near.is_some(),
    );
    let near = b.near.as_ref().expect("near");
    assert_eq!(near.len(), 1, "{near:?}");
    // heading 90 (east): the centre lies 3.5 m west of A's origin
    assert!(
        (near[0].x - 196.5).abs() < 0.02 && (near[0].length - 18.0).abs() < 0.01,
        "{:?}",
        near[0]
    );
}

#[test]
fn a_private_line_reaches_one_player_only() {
    let mut host = LanSession::host(28140, "Server", world("m"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    let mut a = LanSession::join(
        &port.to_string(),
        "Anton",
        world("m"),
        Duration::from_secs(1),
    )
    .unwrap();
    let mut b = LanSession::join(
        &port.to_string(),
        "Berta",
        world("m"),
        Duration::from_secs(1),
    )
    .unwrap();
    let poses = [pose(1.0), pose(2.0), pose(3.0)];
    pump(&mut [&mut host, &mut a, &mut b], &poses, 80, |s| {
        s[1].connected && s[2].connected
    });
    for s in [&mut host, &mut a, &mut b] {
        s.take_events();
    }
    let anton = a.my_id;
    assert!(host
        .say_to(anton, "Admin (private)", "take tour 13/1 at 04:47")
        .is_ok());
    assert!(host.say_to(9999, "Admin (private)", "nobody").is_err());
    assert!(a.say_to(host.my_id, "x", "a client cannot").is_err());
    pump(&mut [&mut host, &mut a, &mut b], &poses, 40, |s| {
        !s[1].events.is_empty()
    });
    let chat = |s: &mut LanSession| -> Vec<(String, String)> {
        s.take_events()
            .into_iter()
            .filter_map(|e| {
                if let LanEvent::Chat { name, text, .. } = e {
                    Some((name, text))
                } else {
                    None
                }
            })
            .collect()
    };
    assert_eq!(
        chat(&mut a),
        [("Admin (private)".to_string(), "take tour 13/1 at 04:47".to_string())]
    );
    // (a little longer for Berta: nothing comes)
    pump(&mut [&mut host, &mut a, &mut b], &poses, 20, |_| false);
    assert!(chat(&mut b).is_empty());
}

#[test]
fn chat_reaches_everybody_and_floods_do_not() {
    let mut host = LanSession::host(28150, "Hanna", world("m"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    let mut a = LanSession::join(
        &port.to_string(),
        "Anton",
        world("m"),
        Duration::from_secs(1),
    )
    .unwrap();
    let mut b = LanSession::join(
        &port.to_string(),
        "Berta",
        world("m"),
        Duration::from_secs(1),
    )
    .unwrap();
    let poses = [pose(1.0), pose(2.0), pose(3.0)];
    pump(&mut [&mut host, &mut a, &mut b], &poses, 80, |s| {
        s[1].connected && s[2].connected
    });
    for s in [&mut host, &mut a, &mut b] {
        s.take_events();
    }
    assert!(a.say("  hello | there\n ").is_ok());
    assert!(host.say("welcome").is_ok());
    pump(&mut [&mut host, &mut a, &mut b], &poses, 40, |s| {
        s[2].events.len() >= 2
    });
    let lines = |s: &mut LanSession| -> Vec<(String, String, bool)> {
        s.take_events()
            .into_iter()
            .filter_map(|e| {
                if let LanEvent::Chat {
                    name, text, mine, ..
                } = e
                {
                    Some((name, text, mine))
                } else {
                    None
                }
            })
            .collect()
    };
    let seen_b = lines(&mut b);
    assert!(
        seen_b.contains(&("Anton".into(), "hello   there".into(), false)),
        "{seen_b:?}"
    );
    assert!(
        seen_b.contains(&("Hanna".into(), "welcome".into(), false)),
        "{seen_b:?}"
    );
    let seen_a = lines(&mut a);
    assert!(
        seen_a.contains(&("Anton".into(), "hello   there".into(), true))
            && seen_a.contains(&("Hanna".into(), "welcome".into(), false)),
        "{seen_a:?}"
    );
    assert!(lines(&mut host).contains(&("Anton".into(), "hello   there".into(), false)));
    // our own game lets one line a second through (after a burst of three)
    let ok = (0..10)
        .filter(|i| a.say(&format!("spam {i}")).is_ok())
        .count();
    assert!(ok <= 3, "{ok}");
    // a modified game that floods the host gets no further
    let at = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    for i in 0..50 {
        a.send(
            format!("CHAT|{}|flood {i} {}", a.my_id, "x".repeat(400)).as_bytes(),
            at,
        );
    }
    pump(&mut [&mut host, &mut a, &mut b], &poses, 20, |_| false);
    let flood: Vec<_> = lines(&mut b)
        .into_iter()
        .filter(|l| l.1.starts_with("flood"))
        .collect();
    assert!(flood.len() <= 3, "{} flood lines relayed", flood.len());
    assert!(flood.iter().all(|l| l.1.chars().count() <= MAX_CHAT));
    // a chat line under somebody else's id is not relayed
    let stranger = raw();
    stranger
        .send_to(format!("CHAT|{}|spoofed", b.my_id).as_bytes(), at)
        .unwrap();
    pump(&mut [&mut host, &mut a, &mut b], &poses, 10, |_| false);
    assert!(!lines(&mut a).iter().any(|l| l.1 == "spoofed"));
    // leaving is announced
    drop(b);
    pump(&mut [&mut host, &mut a], &poses[..2], 20, |_| false);
    let notes: Vec<LanEvent> = a.take_events();
    assert!(
        notes.contains(&LanEvent::Notice("Berta left".into())),
        "{notes:?}"
    );
}

#[test]
fn old_protocol_is_turned_away() {
    let mut host = LanSession::host(28160, "host", world("m"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    let old = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    old.set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let mut buf = [0u8; 512];
    // a protocol 2 game: its hello, and its poses sent blindly
    let told = format!("protocol {PROTOCOL}, your game protocol 2");
    for (msg, want) in [
        (
            "HELLO|2|-|someone|Vehicles/x.bus|m|1989-05-30|32400||0|0|0|0|12|2.5",
            told.as_str(),
        ),
        (
            "POSE|2|someone|Vehicles/x.bus||1|2|3|0|0|0|0|0000|0|0|||12|2.5|-|0",
            "older one",
        ),
    ] {
        let mut answers = Vec::new();
        for _ in 0..40 {
            let _ = old.send_to(msg.as_bytes(), ("127.0.0.1", port));
            host.tick(0.05, &pose(0.0));
            while let Ok((n, _)) = old.recv_from(&mut buf) {
                answers.push(String::from_utf8_lossy(&buf[..n]).to_string());
            }
            if answers.iter().any(|a| a.contains(want)) {
                break;
            }
        }
        assert!(
            answers.iter().all(|a| a.starts_with(&format!("REJECT|{PROTOCOL}|")))
                && answers.iter().any(|a| a.contains(want)),
            "{answers:?}"
        );
    }
    assert_eq!(host.peer_count(), 0);
}

/// A socket that plays the host (or a stranger) by hand.
fn raw() -> UdpSocket {
    let s = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    s.set_read_timeout(Some(Duration::from_millis(20))).unwrap();
    s
}

fn to_client(c: &LanSession) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, c.local_addr().unwrap().port()))
}

fn welcome_msg(id: u32) -> String {
    format!(
        "WELCOME|{PROTOCOL}|{id}|{}|host|m|1989-05-30|32400|||1",
        session_hex(7)
    )
}

fn state_of(id: u32, x: f64) -> Vec<u8> {
    let mut p = pose(x);
    p.id = id;
    wire::encode_state(&p, PROTOCOL as u8, 1)
}

#[test]
fn a_client_listens_to_its_host_only() {
    let host = raw();
    let mut c = LanSession::join_addr(vec![host.local_addr().unwrap()], None, "c", world("m")).unwrap();
    let mine = pose(1.0);
    host.send_to(welcome_msg(5).as_bytes(), to_client(&c))
        .unwrap();
    host.send_to(&state_of(1, 50.0), to_client(&c)).unwrap();
    for _ in 0..20 {
        c.tick(0.01, &mine);
        if c.connected && c.peer_count() == 1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        c.connected && c.my_id == 5 && c.peer_count() == 1,
        "the host is heard"
    );
    // somebody else on the network who found the client's port
    let stranger = raw();
    let at = to_client(&c);
    stranger
        .send_to(format!("REJECT|{PROTOCOL}|go away").as_bytes(), at)
        .unwrap();
    stranger.send_to(welcome_msg(9).as_bytes(), at).unwrap();
    stranger.send_to(b"BYE|1", at).unwrap();
    stranger.send_to(b"SAY|1|host|buy this", at).unwrap();
    stranger.send_to(b"CLOCK|m|1989-05-30|0|x|", at).unwrap();
    for id in 100..400 {
        stranger.send_to(&state_of(id, 3.0), at).unwrap();
    }
    for _ in 0..20 {
        c.tick(0.01, &mine);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(c.rejected.is_none(), "{:?}", c.rejected);
    assert!(c.connected);
    assert_eq!(c.my_id, 5);
    assert_eq!(c.peer_count(), 1, "no stranger's states");
    assert!(c.peers().all(|p| p.pose.id == 1));
    assert!(!c
        .take_events()
        .iter()
        .any(|e| matches!(e, LanEvent::Chat { .. })));
    assert!(c
        .take_host_clock()
        .map(|h| h.world.weather != "x")
        .unwrap_or(true));
}

#[test]
fn a_client_takes_a_limited_number_of_players() {
    let host = raw();
    let mut c = LanSession::join_addr(vec![host.local_addr().unwrap()], None, "c", world("m")).unwrap();
    host.send_to(welcome_msg(2).as_bytes(), to_client(&c))
        .unwrap();
    for id in 3..(3 + 3 * MAX_PEERS as u32) {
        host.send_to(&state_of(id, id as f64), to_client(&c))
            .unwrap();
    }
    for _ in 0..20 {
        c.tick(0.01, &pose(0.0));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(c.connected);
    assert_eq!(c.peer_count(), MAX_PEERS);
}

#[test]
fn old_states_do_not_overtake_new_ones() {
    let host = raw();
    let mut c = LanSession::join_addr(vec![host.local_addr().unwrap()], None, "c", world("m")).unwrap();
    host.send_to(welcome_msg(2).as_bytes(), to_client(&c))
        .unwrap();
    let mut p = pose(10.0);
    p.id = 1;
    host.send_to(&wire::encode_state(&p, PROTOCOL as u8, 500), to_client(&c))
        .unwrap();
    p.x = 5.0;
    host.send_to(&wire::encode_state(&p, PROTOCOL as u8, 499), to_client(&c))
        .unwrap();
    for _ in 0..10 {
        c.tick(0.01, &pose(0.0));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(c.peers().next().map(|p| p.pose.x), Some(10.0));
}

#[test]
fn a_full_session_turns_the_next_player_away() {
    let mut host = LanSession::host(28170, "host", world("m"), true).unwrap();
    let at = SocketAddr::from((Ipv4Addr::LOCALHOST, host.local_addr().unwrap().port()));
    let hello = format!("HELLO|{PROTOCOL}|-|p|Vehicles/x.bus|m|1989-05-30|32400||");
    let players: Vec<UdpSocket> = (0..MAX_PEERS).map(|_| raw()).collect();
    let mut buf = [0u8; 512];
    for (i, p) in players.iter().enumerate() {
        p.send_to(hello.as_bytes(), at).unwrap();
        host.tick(0.01, &pose(0.0));
        // everybody gets in only once the ones before them have loaded (a state arrived)
        let mut id = None;
        for _ in 0..20 {
            match p.recv_from(&mut buf) {
                Ok((n, _)) => {
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    if let Some(rest) = text.strip_prefix(&format!("WELCOME|{PROTOCOL}|")) {
                        id = rest.split('|').next().and_then(|s| s.parse::<u32>().ok());
                        break;
                    }
                }
                Err(_) => {
                    host.tick(0.01, &pose(0.0));
                }
            }
        }
        let id = id.unwrap_or_else(|| panic!("player {i} welcomed"));
        p.send_to(&state_of(id, i as f64), at).unwrap();
        host.tick(0.01, &pose(0.0));
    }
    assert_eq!(host.peer_count(), MAX_PEERS);
    let late = raw();
    let mut answer = String::new();
    for _ in 0..40 {
        late.send_to(hello.as_bytes(), at).unwrap();
        host.tick(0.01, &pose(0.0));
        if let Ok((n, _)) = late.recv_from(&mut buf) {
            answer = String::from_utf8_lossy(&buf[..n]).to_string();
            if answer.starts_with("REJECT") {
                break;
            }
        }
    }
    assert!(
        answer.starts_with("REJECT|") && answer.contains("full"),
        "{answer}"
    );
    assert_eq!(host.peer_count(), MAX_PEERS);
}

#[test]
fn too_many_loading_players_wait() {
    let mut host = LanSession::host(28180, "host", world("m"), true).unwrap();
    let at = SocketAddr::from((Ipv4Addr::LOCALHOST, host.local_addr().unwrap().port()));
    let hello = format!("HELLO|{PROTOCOL}|-|p|Vehicles/x.bus|m|1989-05-30|32400||");
    let players: Vec<UdpSocket> = (0..MAX_JOINING + 1).map(|_| raw()).collect();
    for p in &players {
        p.send_to(hello.as_bytes(), at).unwrap();
    }
    for _ in 0..10 {
        host.tick(0.01, &pose(0.0));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(host.peer_count(), MAX_JOINING);
    let mut buf = [0u8; 512];
    let mut answers = Vec::new();
    while let Ok((n, _)) = players[MAX_JOINING].recv_from(&mut buf) {
        answers.push(String::from_utf8_lossy(&buf[..n]).to_string());
    }
    assert!(
        answers
            .iter()
            .any(|a| a.starts_with("REJECT") && a.contains("try again")),
        "{answers:?}"
    );
}

#[test]
fn the_host_checks_and_limits_what_it_relays() {
    let mut host = LanSession::host(27890, "host", world("m"), true).unwrap();
    let port = host.local_addr().unwrap().port();
    let mut a =
        LanSession::join(&port.to_string(), "a", world("m"), Duration::from_secs(1)).unwrap();
    let mut b =
        LanSession::join(&port.to_string(), "b", world("m"), Duration::from_secs(1)).unwrap();
    let poses = [pose(1.0), pose(2.0), pose(3.0)];
    pump(&mut [&mut host, &mut a, &mut b], &poses, 80, |s| {
        s[1].connected && s[2].connected && s[2].peer_count() == 2
    });
    let at = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    // somebody who never joined, or who pretends to be A, is not relayed
    let stranger = raw();
    stranger.send_to(&state_of(a.my_id, 777.0), at).unwrap();
    stranger.send_to(&state_of(99, 778.0), at).unwrap();
    stranger
        .send_to(
            format!("INFO|{}|mallory|Vehicles/evil.bus|||||11|2.5|0|0", a.my_id).as_bytes(),
            at,
        )
        .unwrap();
    // B sends a state under A's id
    b.send(&state_of(a.my_id, 779.0), at);
    // A floods: a thousand states in no time (each newer than the one before)
    let before = b.received;
    for i in 0..1000u16 {
        let mut p = pose(1000.0 + i as f64);
        p.id = a.my_id;
        a.send(
            &wire::encode_state(&p, PROTOCOL as u8, a.seq.wrapping_add(1 + i)),
            at,
        );
    }
    for _ in 0..5 {
        host.tick(0.001, &poses[0]);
    }
    std::thread::sleep(Duration::from_millis(30));
    for _ in 0..3 {
        b.tick(0.001, &poses[2]);
    }
    let relayed = (b.received - before) / state_of(2, 1.0).len() as u64;
    assert!(relayed <= 60, "{relayed} of a thousand states relayed");
    assert!(
        b.peers().all(|p| (p.pose.x - 777.0).abs() > 0.5
            && (p.pose.x - 778.0).abs() > 0.5
            && (p.pose.x - 779.0).abs() > 0.5),
        "no spoofed state"
    );
    assert!(b.peers().all(|p| p.pose.name != "mallory"));
    assert!(host.peers().any(|p| p.dropped > 0));
}

#[test]
fn idle_buses_send_less() {
    let host = raw();
    let mut c = LanSession::join_addr(vec![host.local_addr().unwrap()], None, "c", world("m")).unwrap();
    host.send_to(welcome_msg(4).as_bytes(), to_client(&c))
        .unwrap();
    // (the simulated seconds pass quickly: the silent host must not time out meanwhile)
    host.set_nonblocking(true).unwrap();
    let still = pose(5.0);
    let mut buf = [0u8; 2048];
    let mut count = |c: &mut LanSession, frames: usize, moving: bool| -> usize {
        let mut states = 0;
        let mut p = still.clone();
        for i in 0..frames {
            if moving {
                p.x += 0.1 * i as f64;
            }
            c.tick(0.01, &p);
            while let Ok((n, _)) = host.recv_from(&mut buf) {
                if buf[0] == wire::STATE_MAGIC && n > 0 {
                    states += 1;
                }
            }
        }
        states
    };
    // two simulated seconds standing still: the first second at full rate, then five a second
    let standing = count(&mut c, 200, false);
    assert!(c.connected);
    // two simulated seconds driving
    let driving = count(&mut c, 200, true);
    assert!(
        (34..=46).contains(&driving),
        "{driving} states while driving"
    );
    assert!(standing < 30, "{standing} states while standing");
}

#[test]
fn a_player_without_a_bus_sends_a_heartbeat() {
    let host = raw();
    let mut c =
        LanSession::join_addr(vec![host.local_addr().unwrap()], None, "observer", world("m")).unwrap();
    host.send_to(welcome_msg(4).as_bytes(), to_client(&c))
        .unwrap();
    let none = Pose::default();
    let mut buf = [0u8; 1024];
    let mut beats = 0;
    // four seconds of frames without a bus
    for _ in 0..40 {
        c.tick(0.1, &none);
        while let Ok((n, _)) = host.recv_from(&mut buf) {
            if let Some((4, _, p)) = wire::decode_state(&buf[..n], PROTOCOL as u8) {
                assert_eq!(p.flags & FLAG_VEHICLE, 0);
                beats += 1;
            }
        }
    }
    assert!(c.connected);
    assert!(
        (3..=5).contains(&beats),
        "about one empty state a second, got {beats}"
    );
}

mod connections;

mod lifecycle;
mod traffic;
mod voice;
