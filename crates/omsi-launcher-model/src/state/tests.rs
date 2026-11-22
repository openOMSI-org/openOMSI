use super::*;

mod choice_tests {
    #[test]
    fn server_join_protocol_matrix() {
        use super::{JoinProto, server_join_address};
        let info = omsi_net::ws::ServerInfo {
            reached_at: "https://gateway.example.org".into(),
            ..Default::default()
        };
        assert_eq!(
            server_join_address("192.168.1.5:27015", JoinProto::Auto, Some(&info)),
            Ok(("192.168.1.5:27015".into(), JoinProto::Udp))
        );
        assert_eq!(
            server_join_address("play.example.org", JoinProto::Auto, Some(&info)),
            Ok(("https://gateway.example.org".into(), JoinProto::WebSocket))
        );
        assert_eq!(
            server_join_address("https://play.example.org", JoinProto::Auto, Some(&info)),
            Ok(("https://play.example.org".into(), JoinProto::WebSocket))
        );
        assert_eq!(
            server_join_address("https://play.example.org", JoinProto::Udp, Some(&info)),
            Err("UDP needs the game's address (host or host:port), not a web link")
        );
        assert_eq!(
            server_join_address("192.168.1.5:27015", JoinProto::WebSocket, Some(&info)),
            Ok(("https://gateway.example.org".into(), JoinProto::WebSocket))
        );
    }

    #[test]
    fn auto_uses_the_typed_port_when_it_answered_status() {
        use super::{JoinProto, server_join_address};
        for address in [
            "play.example.org:27025",
            "192.168.1.4:47025",
            "[::1]:27025",
            "  play.example.org:27025  ",
        ] {
            let info = omsi_net::ws::ServerInfo {
                reached_at: format!("http://{}", address.trim()),
                ..Default::default()
            };
            assert_eq!(
                server_join_address(address, JoinProto::Auto, Some(&info)),
                Ok((format!("http://{}", address.trim()), JoinProto::WebSocket))
            );
        }
    }

    #[test]
    fn unknown_saved_protocol_preserves_the_server_list() {
        let servers: Vec<super::ServerEntry> = serde_json::from_str(
            r#"[
            {"address":"old.example.org"},
            {"address":"udp.example.org","protocol":"udp"},
            {"address":"ws.example.org","protocol":"websocket"},
            {"address":"future.example.org","protocol":"future"}
        ]"#,
        )
        .unwrap();
        use super::JoinProto::*;
        assert_eq!(
            servers.iter().map(|s| s.protocol).collect::<Vec<_>>(),
            [Auto, Auto, Udp, WebSocket, Auto]
        );
        assert_eq!(servers[4].address, "future.example.org");
    }

    #[test]
    fn every_server_protocol_requires_gateway_status() {
        for address in [
            "play.example.org",
            "play.example.org:27015",
            "https://play.example.org",
        ] {
            for protocol in [
                super::JoinProto::Auto,
                super::JoinProto::Udp,
                super::JoinProto::WebSocket,
            ] {
                assert!(super::server_join_address(address, protocol, None).is_err());
            }
        }
    }

    #[test]
    fn server_protocol_defaults_to_auto_and_survives_saving() {
        let old: super::ServerEntry =
            serde_json::from_str(r#"{"name":"Example","address":"play.example.org:47015"}"#)
                .unwrap();
        assert_eq!(old.protocol, super::JoinProto::Auto);
        for protocol in [
            super::JoinProto::Auto,
            super::JoinProto::Udp,
            super::JoinProto::WebSocket,
        ] {
            let entry = super::ServerEntry {
                protocol,
                ..old.clone()
            };
            let back: super::ServerEntry =
                serde_json::from_str(&serde_json::to_string(&entry).unwrap()).unwrap();
            assert_eq!(entry, back);
        }
    }

    /// `launcher-duty.json` from before the number plate field: the missing key falls back to
    /// the default (no plate), and a typed plate survives a round trip.
    #[test]
    fn a_remembered_join_is_not_resumed_at_launch() {
        let saved: super::Choice =
            serde_json::from_str(r#"{"lan_mode":"join","lan_addr":"main.example.org"}"#).unwrap();
        assert_eq!(saved.lan_mode, "join");
        assert_eq!(super::Choice::fresh(saved).lan_mode, "off");
        let host: super::Choice = serde_json::from_str(r#"{"lan_mode":"host"}"#).unwrap();
        assert_eq!(super::Choice::fresh(host).lan_mode, "host");
    }

    #[test]
    fn an_old_duty_file_loads_and_a_typed_plate_is_kept() {
        let old: super::Choice =
            serde_json::from_str(r#"{"bus":"Vehicles/x.bus","map":"maps/x/global.cfg"}"#).unwrap();
        assert_eq!(old.plate, "");
        assert!(!old.comfortable_cabin_on_start);
        let mut c = super::Choice::default();
        c.plate = "B-AB 1234".into();
        c.comfortable_cabin_on_start = true;
        let back: super::Choice =
            serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back.plate, "B-AB 1234");
        assert!(back.comfortable_cabin_on_start);
    }
}

mod launch_tests {
    use omsi_launcher_lib::Instance;

    fn game(pid: u32, running: bool) -> Instance {
        Instance {
            pid,
            running,
            ..Default::default()
        }
    }

    /// The wait after Start ends with the first list of games that has the one started,
    /// running or ended already - a game that died at once left the launcher blank for the
    /// whole 15 seconds.
    #[test]
    fn the_wait_after_start_ends_once_the_game_is_listed() {
        // (a list asked for before the game was started)
        assert!(!super::game_listed(Some(7), &[game(3, true)]));
        assert!(!super::game_listed(Some(7), &[game(3, false)]));
        assert!(super::game_listed(Some(7), &[game(7, true)]));
        assert!(super::game_listed(Some(7), &[game(7, false)]));
    }
}
