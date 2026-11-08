use super::*;

struct Harness {
    ui: Ui,
    state: State,
    actions: Vec<Action>,
}

impl Harness {
    fn new() -> Self {
        Self {
            ui: Ui::new(),
            state: State::default(),
            actions: Vec::new(),
        }
    }

    fn begin(&mut self, w: f32, h: f32, scale: f32, dt: f32) {
        self.ui.begin(Vec2::new(w, h), scale, dt);
    }

    fn panel(&mut self) -> Panel<'_> {
        Panel {
            ui: &mut self.ui,
            state: &mut self.state,
            actions: &mut self.actions,
        }
    }

    fn finish(&mut self) {
        self.ui.finish();
    }
}

fn session() -> View {
    View {
        rights: Some(Role::Admin),
        voice: true,
        time: 21600.0,
        speed: 1.0,
        speeds: vec![1.0, 2.0, 4.0],
        traffic: Some(120),
        traffic_next: "traffic next".into(),
        ..Default::default()
    }
}

fn render(sh: &mut Harness, tab: usize, v: &View) {
    sh.begin(1440.0, 900.0, 1.0, 0.016);
    let mut panel = sh.panel();
    let area = Rect::new(280.0, 300.0, 1000.0, 550.0);
    match tab {
        0 => players(&mut panel, v, area),
        1 => world(&mut panel, v, area),
        _ => bans(&mut panel, v, area),
    }
}

fn click(sh: &mut Harness, tab: usize, v: &View, id: &str) {
    render(sh, tab, v);
    let r = *sh.ui.drawn.get(&id_of(id)).expect("visible control");
    sh.finish();
    sh.begin(1440.0, 900.0, 1.0, 0.016);
    sh.ui.input.mouse = r.center();
    sh.ui.input.pressed = true;
    sh.ui.input.released = true;
    let mut panel = sh.panel();
    let area = Rect::new(280.0, 300.0, 1000.0, 550.0);
    match tab {
        0 => players(&mut panel, v, area),
        1 => world(&mut panel, v, area),
        _ => bans(&mut panel, v, area),
    }
}

#[test]
fn search_matches_unicode_names_vehicles_and_ids() {
    let p = Player {
        id: 2,
        conn: 102,
        name: "Alex 名稱".into(),
        state: PlayerState::Driving,
        vehicle: "Vehicles/MAN/MAN_D87.bus".into(),
        line: String::new(),
        tour: String::new(),
        aboard: None,
        online_secs: 60,
        admin: None,
    };
    for query in ["alex", "名稱", "man_d87", "#2"] {
        assert!(matches_query(&p, query), "{query}");
    }
    assert!(!matches_query(&p, "missing"));
}

#[test]
fn online_voice_and_time_controls_dispatch_existing_actions() {
    let v = session();
    for (tab, id, action) in [
        (0, "pause-online-voice-all", "vall"),
        (1, "online-time-preset-12", "clock 43200"),
        (1, "online-speed-4", "speed 4"),
        (1, "online-next-weather", "weather next"),
        (1, "online-weather-cycle", "weather cycle"),
        (1, "online-traffic-next", "traffic next"),
    ] {
        let mut sh = Harness::new();
        click(&mut sh, tab, &v, id);
        assert_eq!(
            sh.actions,
            [Action::OnlineAction(tab as u8, action.into())],
            "{id}"
        );
    }
}

#[test]
fn online_world_controls_respect_roles_and_time_locks() {
    let mut v = session();
    let mut sh = Harness::new();
    v.rights = Some(Role::Moderator);
    render(&mut sh, 1, &v);
    assert!(!sh.ui.drawn.contains_key(&id_of("online-speed-4")));
    v.rights = Some(Role::Admin);
    v.clock_locked = true;
    render(&mut sh, 1, &v);
    assert!(!sh.ui.drawn.contains_key(&id_of("online-time-preset-12")));
}

#[test]
fn online_unban_uses_device_identity_and_admin_permission() {
    let mut v = session();
    v.bans.push(BanView {
        id: "device-record".into(),
        name: "Alex".into(),
        reason: "".into(),
        by: "host".into(),
        at: 0,
        until: None,
        scope: "session".into(),
    });
    let mut sh = Harness::new();
    click(&mut sh, 2, &v, "online-unban-device-record");
    assert!(sh.actions.is_empty());
    assert_eq!(sh.state.confirm, Some((2, "unban device-record".into())));
    v.rights = Some(Role::Moderator);
    sh.state.confirm = None;
    render(&mut sh, 2, &v);
    assert!(
        !sh.ui
            .drawn
            .contains_key(&id_of("online-unban-device-record"))
    );
}

#[test]
fn online_confirmation_cancel_and_accept_keep_their_existing_actions() {
    for (tab, action) in [(1, "traffic clear"), (2, "unban device-record")] {
        for accept in [false, true] {
            let mut sh = Harness::new();
            sh.state.confirm = Some((tab, action.into()));
            sh.begin(1440.0, 900.0, 1.0, 0.016);
            confirmation(&mut sh.panel());
            let id = if accept {
                "online-confirm-accept"
            } else {
                "online-confirm-cancel"
            };
            let r = *sh.ui.drawn.get(&id_of(id)).unwrap();
            sh.finish();
            sh.begin(1440.0, 900.0, 1.0, 0.016);
            sh.ui.input.mouse = r.center();
            sh.ui.input.pressed = true;
            sh.ui.input.released = true;
            confirmation(&mut sh.panel());
            assert!(sh.state.confirm.is_none());
            if accept {
                assert_eq!(sh.actions, [Action::OnlineAction(tab, action.into())]);
            } else {
                assert!(sh.actions.is_empty());
            }
        }
    }
}
