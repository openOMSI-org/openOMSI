include!("../../../../tools/test-support/original_root.rs");
use super::*;

#[test]
fn multiplayer_hud_translates_client_states_and_preserves_session_details() {
    let mut lan = LanSession::join_addr(Vec::new(), None, "Driver", Default::default()).unwrap();
    let tr = |text: &str| match text {
        "Online: connecting ..." => "連線中...".into(),
        "Online: not connected: {reason} (chat /reconnect)" => {
            "未連線：{reason}（聊天 /reconnect）".into()
        }
        "Online: in {host}'s game, {others}" => "已加入 {host} 的遊戲，{others}".into(),
        "nobody else yet" => "尚無其他玩家".into(),
        "Online: {warning}" => "連線：{warning}".into(),
        "Map not installed" => "尚未安裝地圖".into(),
        _ => text.into(),
    };
    assert_eq!(hud_lines(&lan, &tr), ["連線中..."]);
    lan.rejected = Some("Map not installed".into());
    assert_eq!(
        hud_lines(&lan, &tr),
        ["未連線：尚未安裝地圖（聊天 /reconnect）"]
    );
    lan.rejected = None;
    lan.connected = true;
    lan.welcome = Some(omsi_net::Welcome {
        host_name: "Alice {others}".into(),
        session: 7,
        world: Default::default(),
        at: Instant::now(),
        players: 1,
    });
    lan.warnings.push("Map not installed".into());
    assert_eq!(
        hud_lines(&lan, &tr),
        [
            "已加入 Alice {others} 的遊戲，尚無其他玩家",
            "連線：尚未安裝地圖"
        ]
    );
}

#[test]
fn compatible_string_tables_check_picture_paths_at_the_local_index() {
    let table = crate::sync::VarTable {
        hash: 0,
        floats: Vec::new(),
        strings: vec![1, 0, u32::MAX],
    };
    let take = |values| mapped_remote_strings(&table, values, 2, &[1]).collect::<Vec<_>>();
    // A colon is valid in a display text but not in a picture path. The owner's
    // picture is wire slot 0; on this bus it is local slot 1.
    assert_eq!(
        take(vec![(0, "12:30".into()), (1, "12:30".into())]),
        vec![(0, "12:30".into())]
    );
    assert_eq!(
        take(vec![(0, r"..\Texture\blind.bmp".into())]),
        vec![(1, r"..\Texture\blind.bmp".into())]
    );
    for path in [r"\\host\share\x.bmp", "C:/x.bmp", "/dev/zero"] {
        assert!(take(vec![(0, path.into()), (1, path.into())]).is_empty());
    }
    assert!(take(vec![(2, "unmapped".into()), (3, "out of range".into())]).is_empty());
}

#[test]
fn runtime_weather_change_detects_custom_and_clearing_to_default() {
    let custom = "custom:vis=5000;br=0.80;wd=90;ws=4.0;t=22.0;rh=70;p=1013;c=2;cb=120;pt=1;pi=80;wet=0.50;snow=0;snowroad=0";
    assert!(runtime_weather_changed(
        custom,
        Some("Weather/#CAVOK.owt"),
        None
    ));
    assert!(!runtime_weather_changed(custom, Some(custom), None));
    assert!(!runtime_weather_changed(
        custom,
        Some(&custom.to_ascii_uppercase()),
        None
    ));
    assert!(!runtime_weather_changed(
        custom,
        Some("Weather/#CAVOK.owt"),
        Some(custom)
    ));
    // empty is a weather too: the host went back to the map's default
    assert!(runtime_weather_changed("", Some(custom), Some(custom)));
    assert!(!runtime_weather_changed("", None, Some(custom)));
}

/// The maps show another player with their bus, on foot where they walk, riding in a
/// third player's bus with that bus, and not at all riding in ours (#1011, #1080).
#[test]
fn the_maps_show_a_player_where_they_are() {
    let bus = (DVec3::new(100.0, 200.0, 5.0), 90.0);
    let third = |id: u32| (id == 7).then_some((DVec3::new(-50.0, 10.0, 0.0), 180.0));
    let mut pose = Pose {
        id: 3,
        ..Default::default()
    };
    let p = nav_player(&pose, " Anna ", bus, 2, third).unwrap();
    assert_eq!(
        (p.position, p.heading, p.name.as_str()),
        (bus.0, 90.0, "Anna")
    );
    pose.walker = Some(omsi_net::Walker {
        x: 1.0,
        y: 2.0,
        z: 3.0,
        heading: 45.0,
        ..Default::default()
    });
    let p = nav_player(&pose, "", bus, 2, third).unwrap();
    assert_eq!(
        (p.position, p.heading, p.name.as_str()),
        (DVec3::new(1.0, 2.0, 3.0), 45.0, "player 3")
    );
    pose.walker.as_mut().unwrap().aboard = Some(omsi_net::Aboard {
        owner: 7,
        ..Default::default()
    });
    assert_eq!(
        nav_player(&pose, "Anna", bus, 2, third).unwrap().position,
        DVec3::new(-50.0, 10.0, 0.0)
    );
    pose.walker.as_mut().unwrap().aboard = Some(omsi_net::Aboard {
        owner: 2,
        ..Default::default()
    });
    assert!(nav_player(&pose, "Anna", bus, 2, third).is_none());
}

/// What is seen comes before what is heard in the capped values list: the AA-FR Agora
/// L's sound variables filled it in name order before its roller blind's scroll.
#[test]
#[ignore = "needs OMSI_ROOT with AA-FR_BusBundle (not part of the stock install)"]
fn the_roller_blind_scroll_is_in_the_sync_table() {
    let root = original_root();
    let bus = root.join("Vehicles/AA-FR_BusBundle/2002_Agora_L_4d_main.bus");
    require_content(&[&bus]);
    let ty = omsi_sim::VehicleType::load(&root, &bus).expect("Agora L");
    let t = SyncTable::new(&ty, &[]);
    assert!(t.values.len() <= omsi_net::wire::MAX_VALUES);
    for want in ["Rollband_Linie_Trans", "Rollband_Linie_Trans_2"] {
        assert!(
            t.values.iter().any(|v| v.0.eq_ignore_ascii_case(want)),
            "no {want}: {}",
            t.describe()
        );
    }
}

/// An articulated bus's rear section is in its sync table: its lamps, displays' switches
/// and outside sounds (the AA-FR Agora L's rear section stood dark and silent in the
/// other players' games).
#[test]
#[ignore = "needs OMSI_ROOT with AA-FR_BusBundle (not part of the stock install)"]
fn the_rear_section_is_in_the_sync_table() {
    let root = original_root();
    let bus = root.join("Vehicles/AA-FR_BusBundle/2002_Agora_L_3d_main.bus");
    let trail = root.join("Vehicles/AA-FR_BusBundle/2002_Agora_L_3d_trail.bus");
    require_content(&[&bus, &trail]);
    let ty = omsi_sim::VehicleType::load(&root, &bus).expect("Agora L");
    let part = Arc::new(omsi_sim::VehicleType::load(&root, &trail).expect("Agora L trail"));
    let alone = SyncTable::new(&ty, &[]);
    let whole = SyncTable::new(&ty, &[part]);
    let count = |t: &SyncTable| t.lamps.len() + t.switches.len();
    assert!(
        count(&whole) > count(&alone),
        "alone {}, whole {}",
        alone.describe(),
        whole.describe()
    );
    assert!(
        !whole.part_sounds.is_empty(),
        "no sounds for the rear section: {}",
        whole.describe()
    );
    assert_ne!(whole.hash, alone.hash);
}

#[test]
fn admin_password_command_is_private_for_any_case() {
    assert_eq!(admin_chat_arg("/admin hunter2"), Some("hunter2"));
    assert_eq!(admin_chat_arg("/Admin hunter2"), Some("hunter2"));
    assert_eq!(admin_chat_arg(" /ADMIN    hunter2 "), Some("hunter2"));
    assert_eq!(admin_chat_arg("/aDmIn\thunter2"), Some("hunter2"));
    assert_eq!(admin_chat_arg("/admin"), Some(""));
    assert_eq!(admin_chat_arg("/administrator public"), None);
    assert_eq!(admin_chat_arg("/reconnect"), None);
}

/// (#2536) A line typed into the chat: the session's own commands, a `/`-line for the plugins,
/// or something to say - and a `/`-line is never mistaken for one of the others.
#[test]
fn a_slash_line_is_a_command_and_not_chat() {
    assert_eq!(
        chat_line("/login driver hunter2"),
        ChatLine::Command("/login driver hunter2".into())
    );
    assert_eq!(
        chat_line("  /duty 12 A  "),
        ChatLine::Command("/duty 12 A".into()),
        "trimmed"
    );
    assert_eq!(
        chat_line("hello there"),
        ChatLine::Say("hello there".into())
    );
    assert_eq!(
        chat_line("/"),
        ChatLine::Say("/".into()),
        "a lone slash is just a slash, as it always was"
    );
    // the session's own lines keep their meaning, whatever the case they are typed in
    assert_eq!(chat_line("/admin pw"), ChatLine::Admin("pw".into()));
    assert_eq!(chat_line("/ADMIN pw"), ChatLine::Admin("pw".into()));
    assert_eq!(
        chat_line("/admin"),
        ChatLine::Admin(String::new()),
        "no password: the usage line"
    );
    assert_eq!(chat_line("/Reconnect"), ChatLine::Reconnect);
    assert_eq!(
        chat_line("/administrator"),
        ChatLine::Command("/administrator".into()),
        "not /admin"
    );
}

#[test]
fn slash_commands_stay_private_when_no_listener_is_available() {
    let mut lan = LanSession::join_addr(Vec::new(), None, "Driver", Default::default()).unwrap();
    let mut game = LanGame::default();
    game.take_commands = false;
    chat_send(&mut lan, &mut game, "/login driver hunter2");
    assert_eq!(game.plugin_commands.len(), 0);
    assert_eq!(
        game.chat.lines.last(),
        Some(&"* no plugin is listening for commands".to_string())
    );

    game.take_commands = true;
    chat_send(&mut lan, &mut game, "/login driver hunter2");
    assert_eq!(game.plugin_commands, vec!["/login driver hunter2"]);
    assert_eq!(
        game.chat.lines.last(),
        Some(&"* sent to the plugins".to_string())
    );

    // Unloading the last listener resets take_commands:
    game.take_commands = false;
    chat_send(&mut lan, &mut game, "/login driver hunter2");
    assert_eq!(game.plugin_commands.len(), 1);
    assert_eq!(
        game.chat.lines.last(),
        Some(&"* no plugin is listening for commands".to_string())
    );
}

#[test]
fn slash_command_queue_limit_reports_error_and_drops_excess() {
    let mut lan = LanSession::join_addr(Vec::new(), None, "Driver", Default::default()).unwrap();
    let mut game = LanGame::default();
    game.take_commands = true;

    for i in 0..32 {
        chat_send(&mut lan, &mut game, &format!("/cmd {i}"));
        assert_eq!(
            game.chat.lines.last(),
            Some(&"* sent to the plugins".to_string())
        );
    }
    assert_eq!(game.plugin_commands.len(), 32);

    // 33rd command exceeds the queue limit of 32:
    chat_send(&mut lan, &mut game, "/cmd 32");
    assert_eq!(game.plugin_commands.len(), 32);
    assert_eq!(
        game.chat.lines.last(),
        Some(&"* plugin command queue is full".to_string())
    );
}
