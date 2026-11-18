//! Session snapshots and action routing for the independent multiplayer panels.
use super::*;
#[cfg(test)]
use omsi_admin::{Player, PlayerState, Role};
pub(crate) use omsi_multiplayer_ui::View;

pub(crate) fn from_app(app: &crate::App) -> View {
    if app.net.lan.is_none() || app.shell.pause_category != navigation::Category::Online {
        return View::default();
    }
    let weather = crate::weather_cycle::catalog(&app.menus.online_weather);
    let weather_at = weather
        .iter()
        .position(|(file, _)| {
            app.args
                .weather
                .as_deref()
                .is_some_and(|now| now.replace('\\', "/").eq_ignore_ascii_case(file))
        })
        .unwrap_or(0);
    let remote = crate::admin_panel::remote(app);
    let panel = &app.net.admin_panel;
    let time = if remote {
        panel
            .state_value("time")
            .split_once(':')
            .and_then(|(h, m)| {
                Some(h.parse::<f64>().ok()? * 3600.0 + m.parse::<f64>().ok()? * 60.0)
            })
            .unwrap_or(app.clock.time)
    } else {
        app.clock.time
    };
    let traffic = if remote {
        panel
            .state_value("traffic")
            .split('/')
            .next()
            .and_then(|n| n.parse().ok())
    } else {
        app.session.traffic.as_ref().map(|t| t.target)
    };
    let limit = panel
        .state_value("traffic")
        .split('/')
        .nth(1)
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(crate::game_lists::TRAFFIC_MAX);
    let players = crate::player_list::players(app);
    let management = app
        .shell
        .pause_online
        .management
        .as_ref()
        .and_then(|(target, stage)| {
            let target = *target;
            let kind = stage
                .clone()
                .map_or(crate::game_lists::ListKind::PlayerPage(target), |verb| {
                    crate::game_lists::ListKind::PlayerDo(verb, target)
                });
            players
                .iter()
                .find(|player| player.id == target.id && player.conn == target.conn)
                .cloned()
                .map(|player| {
                    let actions = crate::player_list::items(app, &kind)
                        .into_iter()
                        .filter(|(_, action)| action != "noop" && action != "list")
                        .collect();
                    (player, actions)
                })
        });
    View {
        players,
        bans: if crate::player_list::rights(app).is_some() {
            crate::player_list::bans(app)
        } else {
            Vec::new()
        },
        my_id: app.net.lan.as_ref().map_or(0, |lan| lan.my_id),
        rights: crate::player_list::rights(app),
        voice: !app.settings.voice_all_off,
        time,
        speed: if remote {
            panel.state_value("speed").parse().unwrap_or(1.0)
        } else {
            app.net.lan.as_ref().map_or(1.0, |lan| lan.clock_speed)
        },
        speeds: crate::admin::SPEEDS.to_vec(),
        cycling: app.session.weather_cycle.is_some(),
        weather: weather.to_vec(),
        weather_at,
        weather_name: if remote {
            omsi_lan::admin::unesc(panel.state_value("weather"))
        } else {
            weather
                .get(weather_at)
                .map(|(_, name)| name.clone())
                .unwrap_or_else(|| omsi_ui::tr("Unknown weather").into_owned())
        },
        traffic,
        remote,
        clock_locked: app.real_time_locked() || (remote && panel.state_value("rt") == "1"),
        weather_locked: if remote {
            !panel.state_value("metar").is_empty() && panel.state_value("metar") != "-"
        } else {
            app.settings.metar_sync
        },
        traffic_next: if remote {
            format!(
                "traffic {}",
                traffic.unwrap_or(0).saturating_add(10).min(limit)
            )
        } else {
            "traffic next".into()
        },
        management,
    }
}

fn with_panel(sh: &mut Shell, draw: impl FnOnce(&mut omsi_multiplayer_ui::Panel<'_>)) {
    let mut actions = Vec::new();
    draw(&mut omsi_multiplayer_ui::Panel {
        ui: &mut sh.ui,
        state: &mut sh.pause_online,
        actions: &mut actions,
    });
    sh.actions
        .extend(actions.into_iter().map(|action| match action {
            omsi_multiplayer_ui::Action::Choose(index) => Action::Choose(index),
            omsi_multiplayer_ui::Action::OnlineAction(tab, command) => {
                Action::OnlineAction(tab, command)
            }
        }));
}

pub(super) fn page(sh: &mut Shell, v: &PauseView, area: Rect) {
    let room_actions: Vec<_> = v
        .rail
        .iter()
        .enumerate()
        .filter(|(_, line)| !v.disabled.contains(&line.0))
        .map(|(index, line)| (line.0, index))
        .collect();
    let view = omsi_multiplayer_ui::PageView {
        room_actions: &room_actions,
        online: &v.online,
    };
    with_panel(sh, |panel| omsi_multiplayer_ui::page(panel, &view, area));
}

pub(super) fn management_modal(sh: &mut Shell, v: &View) {
    with_panel(sh, |panel| omsi_multiplayer_ui::management_modal(panel, v));
}
pub(super) fn confirmation(sh: &mut Shell) {
    with_panel(sh, omsi_multiplayer_ui::confirmation);
}
#[cfg(test)]
fn players(sh: &mut Shell, v: &View, area: Rect) {
    with_panel(sh, |panel| omsi_multiplayer_ui::players(panel, v, area));
}
#[cfg(test)]
fn world(sh: &mut Shell, v: &View, area: Rect) {
    with_panel(sh, |panel| omsi_multiplayer_ui::world(panel, v, area));
}
#[cfg(test)]
fn bans(sh: &mut Shell, v: &View, area: Rect) {
    with_panel(sh, |panel| omsi_multiplayer_ui::bans(panel, v, area));
}

#[cfg(test)]
#[path = "online_ui_tests.rs"]
mod tests;

#[cfg(test)]
mod weather_cache_tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn snapshots_reuse_the_weather_catalog_including_empty_catalogs() {
        for weather in [
            Vec::new(),
            vec![("Weather/test.owt".into(), "Cached weather".into())],
        ] {
            let mut app = crate::assemble_app(
                crate::Args::parse_from(["openomsi"]),
                Default::default(),
            );
            app.net.lan = Some(omsi_net::LanSession::host(
                0,
                "host",
                Default::default(),
                false,
            )
            .unwrap());
            app.shell.pause_category = navigation::Category::Online;
            app.menus.online_weather.set(weather.clone()).unwrap();
            for _ in 0..2 {
                assert_eq!(from_app(&app).weather, weather);
                let rows = crate::admin::items(&app);
                let presets: Vec<_> = rows
                    .iter()
                    .filter(|(_, action)| action.starts_with("weather set "))
                    .collect();
                assert_eq!(presets.len(), weather.len());
                for ((label, action), (file, name)) in presets.into_iter().zip(&weather) {
                    assert!(label.contains(name));
                    assert_eq!(action, &format!("weather set {file}"));
                }
            }
        }
    }

    #[test]
    fn hidden_online_page_does_not_load_weather() {
        let app = crate::assemble_app(
            crate::Args::parse_from(["openomsi"]),
            Default::default(),
        );
        from_app(&app);
        assert!(app.menus.online_weather.get().is_none());
    }
}
