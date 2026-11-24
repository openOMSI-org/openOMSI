//! Multiplayer panels rendered from session snapshots, without access to the game or transport.
use glam::Vec2;
use omsi_admin::{BanView, Player, PlayerState, Role, Target};
use omsi_ui::kit::{ButtonKind, Ui, id_of};
use omsi_ui::paint::Align;
use omsi_ui::theme::{corner, th};
use omsi_ui::{Color, Rect, Weight};

/// The interactions retained between frames; the app owns their lifetime.
#[derive(Default)]
pub struct State {
    pub tab: usize,
    pub search: String,
    pub confirm: Option<(u8, String)>,
    pub management: Option<(Target, Option<String>)>,
}

/// Requests only: the caller checks current permissions before executing them.
#[derive(Debug, PartialEq)]
pub enum Action {
    Choose(usize),
    OnlineAction(u8, String),
}

pub struct Panel<'a> {
    pub ui: &'a mut Ui,
    pub state: &'a mut State,
    pub actions: &'a mut Vec<Action>,
}

/// Room actions retain the indices of the caller's permission-filtered menu.
pub struct PageView<'a> {
    pub room_actions: &'a [(&'a str, usize)],
    pub online: &'a View,
}

#[derive(Default)]
pub struct View {
    pub players: Vec<Player>,
    pub bans: Vec<BanView>,
    pub my_id: u32,
    pub rights: Option<Role>,
    pub voice: bool,
    pub time: f64,
    pub speed: f64,
    pub speeds: Vec<f64>,
    pub cycling: bool,
    pub weather: Vec<(String, String)>,
    pub weather_at: usize,
    pub weather_name: String,
    pub traffic: Option<usize>,
    pub remote: bool,
    pub clock_locked: bool,
    pub weather_locked: bool,
    pub traffic_next: String,
    pub management: Option<(Player, Vec<(String, String)>)>,
}

/// The display name of a vehicle path, independent of path separators.
pub fn vehicle_name(file: &str) -> String {
    let file = file.replace('\\', "/");
    let name = file.rsplit('/').next().unwrap_or(&file);
    name.rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .to_string()
}

fn label(ui: &mut Ui, text: &str, r: Rect, size: f32, bold: bool) {
    ui.text_in(
        &omsi_ui::tr(text),
        r,
        size,
        if bold { Weight::Bold } else { Weight::Regular },
        th().ink,
        Align::Left,
    );
}

fn command(sh: &mut Panel<'_>, command: &str) {
    sh.actions.push(Action::OnlineAction(1, command.into()));
}

fn button(sh: &mut Panel<'_>, id: &str, r: Rect, text: &str, active: bool) -> bool {
    let (hovered, held, clicked) = sh.ui.interact(id_of(id), r);
    let t = sh.ui.anim(
        id_of(&format!("{id}-hover")),
        if hovered { 1.0 } else { 0.0 },
        0.08,
    );
    let confirm = id == "online-confirm-accept";
    let danger = id == "online-traffic-clear";
    let ink = if confirm {
        Color::WHITE
    } else if danger {
        th().danger_2
    } else if active {
        th().accent
    } else {
        th().ink
    };
    let fill = if confirm {
        th().danger.mix(th().danger_2, t * 0.2)
    } else if active {
        th().surface_2.mix(th().accent, 0.14)
    } else {
        th().surface_2.mix(th().surface_3, t)
    };
    let rr = if held { r.inset(0.5) } else { r };
    sh.ui.p().rounded(rr, corner(8.0), fill.alpha(1.0));
    sh.ui.p().rounded_border(
        rr,
        corner(8.0),
        1.0,
        if confirm {
            th().danger
        } else if active {
            th().accent.alpha(0.75)
        } else {
            th().ink_muted.alpha(0.12 + t * 0.12)
        },
    );
    sh.ui.text_in(
        &omsi_ui::tr(text),
        rr.inset(4.0),
        13.0,
        Weight::Medium,
        ink,
        Align::Center,
    );
    clicked
}

fn divider(ui: &mut Ui, r: Rect) {
    ui.p().rect(r, th().ink_muted.alpha(0.18));
}

fn section_title(ui: &mut Ui, text: &str, icon: &str, r: Rect) {
    let tile = Rect::new(r.x, r.y - 2.0, 30.0, 30.0);
    ui.p()
        .rounded(tile, corner(8.0), th().surface_2.mix(th().accent, 0.07));
    ui.icon(icon, tile.center(), 18.0, th().accent);
    label(
        ui,
        text,
        Rect::new(r.x + 40.0, r.y, (r.w - 40.0).max(0.0), 26.0),
        17.0,
        true,
    );
}

fn checkbox(ui: &mut Ui, id: &str, r: Rect, on: &mut bool, text: &str) -> bool {
    let (_, _, clicked) = ui.interact(id_of(id), r);
    if clicked {
        *on = !*on;
    }
    let box_rect = Rect::new(r.x + 4.0, r.center().y - 8.0, 16.0, 16.0);
    ui.p().rounded(
        box_rect,
        2.0,
        if *on { th().accent } else { th().surface_2 },
    );
    if *on {
        ui.icon("check", box_rect.center(), 15.0, th().accent_ink);
    } else {
        ui.p().rounded_border(box_rect, 2.0, 1.0, th().ink_muted);
    }
    ui.text_in(
        text,
        Rect::new(r.x + 32.0, r.y, (r.w - 32.0).max(0.0), r.h),
        13.0,
        Weight::Regular,
        th().ink_2,
        Align::Left,
    );
    clicked
}

fn room_action(sh: &mut Panel<'_>, v: &PageView<'_>, id: &str, text: &str, r: Rect) {
    let Some((_, index)) = v.room_actions.iter().find(|(key, _)| *key == id) else {
        return;
    };
    let name = format!("pause-online-room-{id}");
    let clicked = if r.w < 80.0 {
        sh.ui.tooltip(r, &omsi_ui::tr(text));
        sh.ui
            .button(&name, r, "", Some("content_copy"), ButtonKind::Normal)
    } else {
        button(sh, &name, r, text, false)
    };
    if clicked {
        sh.actions.push(Action::Choose(*index));
    }
}

pub fn page(sh: &mut Panel<'_>, v: &PageView<'_>, area: Rect) {
    let compact = area.w < 720.0;
    let room = Rect::new(area.x, area.y, area.w, 68.0);
    sh.ui
        .p()
        .rounded(room, corner(12.0), th().surface.alpha(1.0));
    let text_width = (room.w - if compact { 198.0 } else { 440.0 }).max(0.0);
    let text_x = room.x + if compact { 18.0 } else { 66.0 };
    if !compact {
        let tile = Rect::new(room.x + 18.0, room.y + 14.0, 36.0, 36.0);
        sh.ui
            .p()
            .rounded(tile, corner(10.0), th().surface_2.mix(th().accent, 0.08));
        sh.ui.icon("group", tile.center(), 22.0, th().accent);
    }
    label(
        &mut sh.ui,
        "Multiplayer session",
        Rect::new(text_x, room.y + 10.0, text_width, 24.0),
        16.0,
        true,
    );
    let summary = omsi_ui::tr("{count} players in this session")
        .replace("{count}", &v.online.players.len().to_string());
    label(
        &mut sh.ui,
        &summary,
        Rect::new(text_x, room.y + 36.0, text_width, 20.0),
        12.0,
        false,
    );
    let mut action_x = room.right() - if compact { 162.0 } else { 354.0 };
    for (id, text, width) in [
        (
            "copycode",
            "Copy server code",
            if compact { 40.0 } else { 164.0 },
        ),
        (
            "multiplayer",
            "Room settings",
            if compact { 96.0 } else { 164.0 },
        ),
    ] {
        room_action(
            sh,
            v,
            id,
            text,
            Rect::new(action_x, room.y + 16.0, width, 36.0),
        );
        action_x += width + 8.0;
    }
    let tabs_y = room.bottom() + 16.0;
    // The navigation stays separate from the content cards over the live scene.
    sh.ui.p().rounded(
        Rect::new(area.x, tabs_y, area.w, 56.0),
        corner(8.0),
        th().surface.mix(th().surface_2, 0.45).alpha(1.0),
    );
    sh.ui.p().rect(
        Rect::new(area.x + 12.0, tabs_y + 55.0, (area.w - 24.0).max(0.0), 1.0),
        th().ink_muted.alpha(0.4),
    );
    let mut x = area.x + 12.0;
    for (i, (id, text)) in [
        ("players", "Player list"),
        ("world", "World settings"),
        ("admin", "Ban management"),
    ]
    .iter()
    .enumerate()
    {
        let title = omsi_ui::tr(text);
        let badge = (i == 0).then(|| v.online.players.len().to_string());
        let badge_width = badge.as_ref().map_or(0.0, |count| {
            (sh.ui.width(count, 12.0, Weight::Medium) + 14.0).max(22.0)
        });
        let extra = if badge.is_some() {
            badge_width + 8.0
        } else {
            0.0
        };
        let share = if i == 0 { 0.4 } else { 0.3 };
        let w = (sh.ui.width(&title, 14.0, Weight::Medium) + extra + 20.0)
            .min(((area.w - 40.0) * share).max(0.0));
        let r = Rect::new(x, tabs_y + 12.0, w, 40.0);
        let (_, _, click) = sh.ui.interact(id_of(&format!("pause-online-tab-{id}")), r);
        if click {
            sh.state.tab = i;
            sh.ui.dismiss_popups();
            sh.ui.focus = None;
        }
        let on = i == sh.state.tab;
        sh.ui.text_in(
            &title,
            Rect::new(r.x + 10.0, r.y, (r.w - 20.0 - extra).max(0.0), r.h),
            14.0,
            Weight::Medium,
            if on {
                th().accent
            } else {
                th().ink_2.mix(th().ink, 0.25)
            },
            Align::Left,
        );
        if let Some(count) = badge {
            let pill = Rect::new(r.right() - 10.0 - badge_width, r.y + 9.0, badge_width, 22.0);
            sh.ui
                .p()
                .rounded(pill, corner(6.0), th().accent.alpha(0.16));
            sh.ui.text_in(
                &count,
                pill,
                12.0,
                Weight::Medium,
                th().accent,
                Align::Center,
            );
        }
        if on {
            sh.ui
                .p()
                .rect(Rect::new(r.x, r.bottom() - 2.0, r.w, 2.0), th().accent);
        }
        x += w + 8.0;
    }
    let body_y = tabs_y + 76.0;
    let content = Rect::new(area.x, body_y, area.w, (area.bottom() - body_y).max(0.0));
    let body = if sh.state.tab == 1 {
        content
    } else {
        section_card(&mut sh.ui, content);
        content.inset(18.0)
    };
    match sh.state.tab {
        1 => world(sh, &v.online, body),
        2 => bans(sh, &v.online, body),
        _ => players(sh, &v.online, body),
    }
}

pub fn players(sh: &mut Panel<'_>, v: &View, area: Rect) {
    let narrow = area.w < 550.0;
    let search = Rect::new(
        if narrow { area.x } else { area.right() - 300.0 },
        area.y,
        area.w.min(300.0),
        40.0,
    );
    if !narrow {
        label(
            &mut sh.ui,
            "Player list",
            Rect::new(area.x, area.y, area.w - 320.0, 40.0),
            16.0,
            true,
        );
    }
    sh.ui.text_input(
        "pause-online-search",
        search,
        &mut sh.state.search,
        &omsi_ui::tr("Search players by name or vehicle"),
        Some("search"),
    );
    let mut voice = v.voice;
    if checkbox(
        &mut sh.ui,
        "pause-online-voice-all",
        Rect::new(area.x, area.y + 48.0, area.w, 36.0),
        &mut voice,
        &omsi_ui::tr("Listen to other players"),
    ) {
        sh.actions.push(Action::OnlineAction(0, "vall".into()));
    }
    let query = sh.state.search.trim().to_lowercase();
    let found: Vec<_> = v
        .players
        .iter()
        .filter(|p| matches_query(p, &query))
        .collect();
    let mut picked = None;
    let body = Rect::new(area.x, area.y + 96.0, area.w, (area.h - 96.0).max(0.0));
    sh.ui
        .scroll_area("pause-online-player-list", body, &mut |ui, view| {
            for (row, p) in found.iter().enumerate() {
                let r = Rect::new(view.x, view.y + row as f32 * 84.0, view.w, 83.0);
                player_row(ui, p, p.id == v.my_id, r, &mut picked);
            }
            if found.is_empty() {
                label(
                    ui,
                    "No matching players",
                    Rect::new(view.x, view.y, view.w, 60.0),
                    14.0,
                    false,
                );
            }
            (found.len() as f32 * 84.0).max(60.0)
        });
    if let Some(target) = picked {
        sh.state.management = omsi_admin::Target::parse_tag(&target).map(|t| (t, None));
        sh.ui.focus = None;
        sh.ui.dismiss_popups();
        sh.actions
            .push(Action::OnlineAction(0, format!("pick {target}")));
    }
}

pub fn matches_query(p: &Player, query: &str) -> bool {
    p.name.to_lowercase().contains(query)
        || p.vehicle.to_lowercase().contains(query)
        || p.id.to_string() == query.trim_start_matches('#')
}

fn player_row(ui: &mut Ui, p: &Player, own: bool, r: Rect, picked: &mut Option<String>) {
    let narrow = r.w < 550.0;
    ui.p()
        .rect(Rect::new(r.x, r.bottom() - 1.0, r.w, 1.0), th().surface_2);
    let avatar = Rect::new(r.x, r.y + 17.0, 40.0, 40.0);
    ui.p().rounded(
        avatar,
        corner(12.0),
        if own {
            th().surface_2.mix(th().accent, 0.16)
        } else {
            th().surface_2
        },
    );
    ui.text_in(
        &p.name.chars().next().unwrap_or('?').to_string(),
        avatar,
        17.0,
        Weight::Medium,
        if own { th().accent } else { th().ink },
        Align::Center,
    );
    let width = (r.w - if narrow { 160.0 } else { 270.0 }).max(30.0);
    let name = if own {
        format!("{} ({})", p.name, omsi_ui::tr("you"))
    } else {
        p.name.clone()
    };
    label(
        ui,
        &name,
        Rect::new(r.x + 54.0, r.y + 10.0, width, 24.0),
        14.0,
        true,
    );
    let role = omsi_ui::tr(p.admin.map_or("Player", |role| role.name()));
    let vehicle = vehicle_name(&p.vehicle);
    let detail = format!(
        "#{} · {} · {}",
        p.id,
        if vehicle.is_empty() { "—" } else { &vehicle },
        role
    );
    label(
        ui,
        &detail,
        Rect::new(r.x + 54.0, r.y + 37.0, width, 21.0),
        12.0,
        false,
    );
    let state = match p.state {
        PlayerState::Driving => "driving",
        PlayerState::OnFoot => "on foot",
        PlayerState::Joining => "joining",
        PlayerState::Aboard => "aboard a bus",
    };
    let state_rect = if narrow {
        Rect::new(r.x + 54.0, r.y + 60.0, width, 18.0)
    } else {
        Rect::new(r.right() - 200.0, r.y + 27.0, 82.0, 26.0)
    };
    if !narrow {
        ui.p().rounded(state_rect, corner(13.0), th().surface_2);
    }
    ui.text_in(
        &omsi_ui::tr(state),
        state_rect,
        12.0,
        Weight::Regular,
        th().ink_2,
        if narrow { Align::Left } else { Align::Center },
    );
    let action = Rect::new(r.right() - 100.0, r.y + 20.0, 96.0, 40.0);
    if own {
        label(ui, "Yourself", action, 12.0, false);
    } else if ui.button(
        &format!("pause-online-manage-{}", p.target().tag()),
        action,
        &omsi_ui::tr("Manage"),
        Some("chevron_right"),
        ButtonKind::Primary,
    ) {
        *picked = Some(p.target().tag());
    }
}

pub fn management_modal(sh: &mut Panel<'_>, v: &View) {
    let Some((player, actions)) = v.management.as_ref() else {
        sh.state.management = None;
        return;
    };
    let size = sh.ui.size;
    sh.ui.p().rect(
        Rect::new(0.0, 0.0, size.x, size.y),
        Color::BLACK.alpha(0.68),
    );
    let width = (size.x - 32.0).clamp(0.0, 560.0);
    let height = (actions.len() as f32 * 48.0 + 106.0)
        .clamp(250.0, 660.0)
        .min((size.y - 32.0).max(0.0));
    let panel = Rect::new(
        (size.x - width) * 0.5,
        (size.y - height) * 0.5,
        width,
        height,
    );
    sh.ui.panel(panel);
    label(
        &mut sh.ui,
        match sh
            .state
            .management
            .as_ref()
            .and_then(|(_, stage)| stage.as_deref())
        {
            Some("ban") => "Ban this machine...",
            Some("kick") => "Send away...",
            _ => "Manage player",
        },
        Rect::new(panel.x + 22.0, panel.y + 18.0, panel.w - 84.0, 30.0),
        19.0,
        true,
    );
    if sh.ui.icon_button(
        "pause-online-manage-close",
        Vec2::new(panel.right() - 30.0, panel.y + 33.0),
        17.0,
        "close",
        "Close",
    ) {
        sh.state.management = None;
    }
    let vehicle = vehicle_name(&player.vehicle);
    let detail = format!(
        "#{} · {}",
        player.id,
        if vehicle.is_empty() { "—" } else { &vehicle }
    );
    label(
        &mut sh.ui,
        &format!("{}  {detail}", player.name),
        Rect::new(panel.x + 22.0, panel.y + 52.0, panel.w - 44.0, 24.0),
        13.0,
        false,
    );
    let body = Rect::new(
        panel.x + 16.0,
        panel.y + 90.0,
        panel.w - 32.0,
        (panel.h - 106.0).max(0.0),
    );
    let mut picked = None;
    let scroll_id = format!(
        "pause-online-manage-actions-{}-{}",
        player.target().tag(),
        sh.state
            .management
            .as_ref()
            .and_then(|(_, stage)| stage.as_deref())
            .unwrap_or("player")
    );
    sh.ui.scroll_area(&scroll_id, body, &mut |ui, view| {
        for (i, (title, action)) in actions.iter().enumerate() {
            let row = Rect::new(view.x, view.y + i as f32 * 48.0, view.w, 46.0);
            if ui.button(
                &format!("pause-online-manage-action-{i}"),
                row,
                &omsi_ui::tr(title),
                Some("chevron_right"),
                ButtonKind::Normal,
            ) {
                picked = Some(action.clone());
            }
        }
        actions.len() as f32 * 48.0
    });
    if let Some(action) = picked {
        match action.split_once(' ') {
            Some(("do", rest)) => {
                if let Some(verb) = rest.split_whitespace().next() {
                    sh.state.management = Some((player.target(), Some(verb.to_owned())));
                }
            }
            None if action == "reason" => {
                sh.actions.push(Action::OnlineAction(0, action));
            }
            Some(("vmute", _)) | Some(("vunmute", _)) => {
                sh.actions.push(Action::OnlineAction(0, action));
            }
            Some(("run", _)) => {
                sh.actions.push(Action::OnlineAction(0, action));
                if !sh
                    .state
                    .management
                    .as_ref()
                    .is_some_and(|(_, stage)| stage.is_some())
                {
                    sh.state.management = Some((player.target(), None));
                }
            }
            Some(("pick", _)) => sh.state.management = Some((player.target(), None)),
            _ => {}
        }
    }
}

fn section_card(ui: &mut Ui, r: Rect) {
    ui.p().rounded(
        Rect::new(r.x, r.y + 3.0, r.w, r.h),
        corner(12.0),
        Color::BLACK.alpha(0.12),
    );
    ui.p().rounded(r, corner(12.0), th().surface.alpha(1.0));
    ui.p()
        .rounded_border(r, corner(12.0), 1.0, th().ink_muted.alpha(0.20));
}

pub fn world(sh: &mut Panel<'_>, v: &View, area: Rect) {
    if v.rights != Some(Role::Admin) {
        label(
            &mut sh.ui,
            "Only the host or a server administrator can change shared world settings.",
            Rect::new(area.x, area.y, area.w, 60.0),
            14.0,
            false,
        );
        return;
    }
    let two = area.w >= 680.0;
    let cw = if two { (area.w - 16.0) / 2.0 } else { area.w };
    let mut panels = Vec::new();
    sh.ui
        .scroll_area("pause-online-world", area, &mut |ui, view| {
            let time = Rect::new(view.x, view.y, cw, 292.0);
            let weather = Rect::new(
                if two { view.x + cw + 16.0 } else { view.x },
                if two { view.y } else { view.y + 308.0 },
                cw,
                292.0,
            );
            let traffic = Rect::new(
                view.x,
                if two { view.y + 308.0 } else { view.y + 616.0 },
                view.w,
                160.0,
            );
            for r in [time, weather, traffic] {
                section_card(ui, r);
            }
            panels.extend([time, weather, traffic]);
            if two { 480.0 } else { 788.0 }
        });
    // Keep the controls in the same clipped viewport as their scrolling panels.
    sh.ui.push_clip(area, 0.0);
    time_controls(sh, v, panels[0].inset(18.0));
    weather_controls(sh, v, panels[1].inset(18.0));
    traffic_controls(sh, v, panels[2].inset(18.0));
    sh.ui.pop_clip();
}

fn time_controls(sh: &mut Panel<'_>, v: &View, r: Rect) {
    section_title(&mut sh.ui, "Time", "schedule", r);
    let secs = v.time.rem_euclid(86400.0) as u64;
    label(
        &mut sh.ui,
        &format!("{:02}:{:02}", secs / 3600, secs / 60 % 60),
        Rect::new(r.x, r.y + 38.0, 118.0, 44.0),
        32.0,
        false,
    );
    if v.clock_locked {
        label(
            &mut sh.ui,
            "The time cannot be changed while the real-time sync is on",
            Rect::new(r.x, r.y + 104.0, r.w, 72.0),
            13.0,
            false,
        );
        return;
    }
    for (i, (text, cmd)) in [("−1 hour", "time -3600"), ("+1 hour", "time 3600")]
        .iter()
        .enumerate()
    {
        let width = ((r.w - 126.0) / 2.0).max(45.0);
        if button(
            sh,
            &format!("online-time-shift-{i}"),
            Rect::new(
                r.x + 122.0 + i as f32 * width,
                r.y + 40.0,
                width - 4.0,
                40.0,
            ),
            text,
            false,
        ) {
            command(sh, cmd);
        }
    }
    for (i, hour) in [6, 12, 18, 23].iter().enumerate() {
        let w = r.w / 4.0;
        if button(
            sh,
            &format!("online-time-preset-{hour}"),
            Rect::new(r.x + i as f32 * w, r.y + 102.0, w - 6.0, 48.0),
            &format!("{hour:02}:00"),
            secs / 3600 == *hour,
        ) {
            command(sh, &format!("clock {}", hour * 3600));
        }
    }
    divider(&mut sh.ui, Rect::new(r.x, r.y + 162.0, r.w, 1.0));
    sh.ui.text_in(
        &format!("×{}", v.speed),
        Rect::new(r.right() - 70.0, r.y + 174.0, 70.0, 24.0),
        12.0,
        Weight::Medium,
        th().accent,
        Align::Right,
    );
    label(
        &mut sh.ui,
        "Time speed",
        Rect::new(r.x, r.y + 174.0, r.w, 24.0),
        13.0,
        false,
    );
    for (i, speed) in v.speeds.iter().enumerate() {
        let w = r.w / 6.0;
        if button(
            sh,
            &format!("online-speed-{speed}"),
            Rect::new(r.x + i as f32 * w, r.y + 210.0, w - 4.0, 36.0),
            &format!("×{speed}"),
            (*speed - v.speed).abs() < 0.001,
        ) {
            command(sh, &format!("speed {speed}"));
        }
    }
}

fn weather_controls(sh: &mut Panel<'_>, v: &View, r: Rect) {
    section_title(&mut sh.ui, "Weather", "partly_cloudy_day", r);
    label(
        &mut sh.ui,
        "Current weather",
        Rect::new(r.x, r.y + 42.0, r.w, 22.0),
        13.0,
        false,
    );
    let options: Vec<_> = v.weather.iter().map(|(_, name)| name.clone()).collect();
    let mut selected = v.weather_at;
    if v.remote || v.weather_locked {
        label(
            &mut sh.ui,
            &v.weather_name,
            Rect::new(r.x, r.y + 72.0, r.w, 42.0),
            14.0,
            false,
        );
    }
    if !v.remote
        && !v.weather_locked
        && sh.ui.select(
            "online-weather",
            Rect::new(r.x, r.y + 72.0, r.w, 42.0),
            &mut selected,
            &options,
        )
    {
        if let Some((file, _)) = v.weather.get(selected) {
            command(sh, &format!("weather set {file}"));
        }
    }
    if !v.weather_locked
        && button(
            sh,
            "online-next-weather",
            Rect::new(r.x, r.y + 126.0, r.w, 40.0),
            "Next weather",
            false,
        )
    {
        command(sh, "weather next");
    }
    divider(&mut sh.ui, Rect::new(r.x, r.y + 184.0, r.w, 1.0));
    let mut cycling = v.cycling;
    if v.weather_locked {
        label(
            &mut sh.ui,
            "Weather: follows the METAR report (it cannot be changed)",
            Rect::new(r.x, r.y + 180.0, r.w, 64.0),
            12.0,
            false,
        );
    }
    if !v.remote
        && !v.weather_locked
        && checkbox(
            &mut sh.ui,
            "online-weather-cycle",
            Rect::new(r.x, r.y + 202.0, r.w, 42.0),
            &mut cycling,
            &omsi_ui::tr("Weather cycle"),
        )
    {
        command(sh, "weather cycle");
    }
}

fn traffic_controls(sh: &mut Panel<'_>, v: &View, r: Rect) {
    section_title(&mut sh.ui, "Traffic", "directions_bus", r);
    if let Some(target) = v.traffic {
        label(
            &mut sh.ui,
            &format!("{target}"),
            Rect::new(r.x, r.y + 38.0, 90.0, 40.0),
            30.0,
            false,
        );
        label(
            &mut sh.ui,
            "Target traffic vehicles",
            Rect::new(r.x + 92.0, r.y + 49.0, r.w - 92.0, 22.0),
            12.0,
            false,
        );
        if button(
            sh,
            "online-traffic-next",
            Rect::new(r.x, r.y + 84.0, r.w * 0.48, 36.0),
            "Change traffic amount",
            false,
        ) {
            command(sh, &v.traffic_next);
        }
        if button(
            sh,
            "online-traffic-clear",
            Rect::new(r.x + r.w * 0.52, r.y + 84.0, r.w * 0.48, 36.0),
            "Clear the AI traffic (a jam)",
            false,
        ) {
            sh.state.confirm = Some((1, "traffic clear".into()));
        }
    } else {
        label(
            &mut sh.ui,
            "AI traffic is unavailable in this session.",
            Rect::new(r.x, r.y + 38.0, r.w, 40.0),
            13.0,
            false,
        );
    }
}

pub fn confirmation(sh: &mut Panel<'_>) {
    let Some((tab, action)) = sh.state.confirm.clone() else {
        return;
    };
    let size = sh.ui.size;
    sh.ui.p().rect(
        Rect::new(0.0, 0.0, size.x, size.y),
        Color::BLACK.alpha(0.65),
    );
    let width = (size.x - 32.0).clamp(0.0, 460.0);
    let r = Rect::new((size.x - width) * 0.5, (size.y - 180.0) * 0.5, width, 180.0);
    sh.ui.panel(r);
    label(
        &mut sh.ui,
        if tab == 2 {
            "Unban this device?"
        } else {
            "Clear all current AI traffic?"
        },
        Rect::new(r.x + 18.0, r.y + 20.0, r.w - 36.0, 64.0),
        17.0,
        true,
    );
    if button(
        sh,
        "online-confirm-cancel",
        Rect::new(r.x + 18.0, r.y + 112.0, (r.w - 48.0) * 0.5, 42.0),
        "Cancel",
        false,
    ) {
        sh.state.confirm = None;
    }
    if button(
        sh,
        "online-confirm-accept",
        Rect::new(r.x + r.w * 0.5 + 6.0, r.y + 112.0, (r.w - 48.0) * 0.5, 42.0),
        "Confirm",
        false,
    ) {
        sh.actions.push(Action::OnlineAction(tab, action));
        sh.state.confirm = None;
    }
}

pub fn bans(sh: &mut Panel<'_>, v: &View, area: Rect) {
    let title = format!("{} {}", omsi_ui::tr("Banned devices"), v.bans.len());
    label(
        &mut sh.ui,
        &title,
        Rect::new(area.x, area.y, area.w, 28.0),
        16.0,
        true,
    );
    let body = Rect::new(area.x, area.y + 42.0, area.w, (area.h - 80.0).max(0.0));
    if v.bans.is_empty() {
        sh.ui
            .panel(Rect::new(body.x, body.y, body.w, body.h.min(184.0)));
        let y = body.y + body.h.min(184.0) * 0.5 + 22.0;
        let tile = Rect::new(body.center().x - 22.0, y - 82.0, 44.0, 44.0);
        sh.ui.p().rounded(tile, corner(14.0), th().surface_2);
        sh.ui.icon("check_circle", tile.center(), 24.0, th().ink_2);
        let text = if v.rights.is_some() {
            "No banned devices"
        } else {
            "Only the host and server administrators can view device bans."
        };
        sh.ui.text_in(
            &omsi_ui::tr(text),
            Rect::new(body.x + 18.0, y - 26.0, body.w - 36.0, 28.0),
            15.0,
            Weight::Bold,
            th().ink,
            Align::Center,
        );
        if v.rights.is_some() {
            sh.ui.text_in(
                &omsi_ui::tr("A player's ban record appears here."),
                Rect::new(body.x + 18.0, y + 10.0, body.w - 36.0, 24.0),
                13.0,
                Weight::Regular,
                th().ink_2,
                Align::Center,
            );
        }
    } else {
        let mut action = None;
        sh.ui
            .scroll_area("pause-online-bans", body, &mut |ui, view| {
                for (i, ban) in v.bans.iter().enumerate() {
                    let r = Rect::new(view.x, view.y + i as f32 * 80.0, view.w, 78.0);
                    label(
                        ui,
                        &ban.name,
                        Rect::new(r.x, r.y + 8.0, r.w - 110.0, 24.0),
                        14.0,
                        true,
                    );
                    let scope = omsi_ui::tr(if ban.scope == "all" {
                        "all sessions"
                    } else {
                        "this session"
                    });
                    label(
                        ui,
                        &format!("{scope} · {}", ban.reason),
                        Rect::new(r.x, r.y + 38.0, r.w - 110.0, 24.0),
                        12.0,
                        false,
                    );
                    if v.rights == Some(Role::Admin)
                        && ui.button(
                            &format!("online-unban-{}", ban.id),
                            Rect::new(r.right() - 100.0, r.y + 14.0, 96.0, 40.0),
                            &omsi_ui::tr("Unban device"),
                            None,
                            ButtonKind::Normal,
                        )
                    {
                        action = Some(format!("unban {}", ban.id));
                    }
                }
                v.bans.len() as f32 * 80.0
            });
        if let Some(action) = action {
            sh.state.confirm = Some((2, action));
        }
    }
    label(
        &mut sh.ui,
        "Bans apply to devices; changing a name does not lift a ban.",
        Rect::new(area.x, area.bottom() - 25.0, area.w, 25.0),
        12.0,
        false,
    );
}

#[cfg(test)]
mod tests;
