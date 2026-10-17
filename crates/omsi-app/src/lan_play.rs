//! LAN play as the game runs it: the session itself is omsi-lan's (`omsi_lan::game`, used
//! here as `lan`); what stays with the game is how it is shown (the other players' name
//! tags, the HUD's line, the buses offered on the status page) and what the session asks
//! of the game (`AppLanHost`, see `app_impl::lan_tick`).

pub(crate) use crate::app_impl::lan_tick::AppLanHost;
pub(crate) use omsi_lan::game::*;
use glam::DVec3;
use std::path::PathBuf;

/// The HUD's LAN lines (see `omsi_lan::game::hud_lines`), in the game's language.
pub(crate) fn hud_lines(lan: &omsi_net::LanSession) -> Vec<String> {
    omsi_lan::game::hud_lines(lan, &|s| omsi_ui::tr(s).into_owned())
}

/// [`omsi_lan::game::settle_spawn`] for our player's bus, with its driver figure, the
/// command line's choices and the world just loaded.
#[allow(clippy::too_many_arguments)]
pub(crate) fn settle_spawn(
    lan: &mut omsi_net::LanSession,
    game: &mut LanGame,
    p: &mut crate::Player,
    args: &crate::Args,
    world: &crate::scene::World,
    net: Option<&omsi_sim::traffic::Network>,
    wait: std::time::Duration,
) {
    let figure = p.driver.as_ref().map(|d| d.human_type().def.path.clone());
    let host = AppLanHost { args, world: Some(world) };
    omsi_lan::game::settle_spawn(lan, game, &mut p.vehicle, figure.as_deref(), &args.lan_config(), &host, net, wait);
}

/// The buses a joining player may choose, on the gateway's status page (see
/// `omsi_lan::gateway::publish_vehicles`): `only` when a server's list says, else every bus
/// installed under `root`.
pub(crate) fn publish_vehicles(root: PathBuf, only: Vec<String>) {
    omsi_lan::gateway::publish_vehicles(only, move || crate::menu::Menu::new(&root, "").vehicles.into_iter().map(|v| v.1).collect());
}

/// The other players' name tags, as ETS2 has them: the name over the bus's roof and a
/// small line under it (`omsi_lan::game::tag_text`), fading out beyond 300 m.
/// Screen positions in physical pixels of a `width` x `height` picture (on a triple-screen
/// rig, of the three panels side by side). `speaks` tells who talks in the voice chat now (by
/// name and id): "speaking" under their name. `on_radio` who keys the bus radio: "radio"
/// instead, and the tag stays visible across the map.
pub(crate) fn name_tags(
    game: &LanGame,
    cam: &omsi_render::Camera,
    width: f32,
    height: f32,
    rig: Option<&omsi_render::TripleScreen>,
    speaks: &dyn Fn(&str, u32) -> bool,
    on_radio: &dyn Fn(&str, u32) -> bool,
) -> Vec<((f32, f32), String, String, f32)> {
    let views: Vec<_> = if let Some(rig) = rig {
        rig.views(cam, width as u32, height as u32)
            .iter()
            .map(|v| {
                let vp = v.projection
                    * glam::Mat4::look_to_rh(glam::Vec3::ZERO, v.camera.forward(), v.camera.up());
                (vp, v.viewport[0] as f32, v.viewport[2] as f32)
            })
            .collect()
    } else {
        vec![(
            cam.view_proj(width / height.max(1.0), cam.position),
            0.0,
            width,
        )]
    };
    let mut tags = Vec::new();
    for r in game.remotes.values() {
        let v = r.vehicle();
        // the roof: the bounding box's top when the bus says, else a bus's height
        let top = v.ty.def.bounding_box.map(|b| (b[5] + b[2] * 0.5) as f64).filter(|t| *t > 1.0).unwrap_or(3.4);
        // (a player out of the seat: the name over their head, none over the bus)
        let p = match r.last.walker {
            Some(w) => DVec3::new(w.x, w.y, w.z + 2.15),
            None => v.position + DVec3::new(0.0, 0.0, top + 0.6),
        };
        let d = (p - cam.position).length();
        let radio = on_radio(&r.name, r.last.id) || r.last.radio_keyed;
        // (a radio key across the map: the tag still shows who is transmitting)
        if d > 450.0 && !radio {
            continue;
        }
        let Some((screen_x, y)) = views.iter().find_map(|(vp, offset, panel_width)| {
            let c = *vp * (p - cam.position).as_vec3().extend(1.0);
            if c.w <= 0.1 {
                return None;
            }
            let (x, y) = (c.x / c.w, c.y / c.w);
            let limit = if rig.is_some() { 1.0 } else { 1.2 };
            (x.abs() <= limit && y.abs() <= 1.2)
                .then_some((offset + (x + 1.0) * 0.5 * panel_width, y))
        }) else {
            continue;
        };
        let speaking = !radio && speaks(&r.name, r.last.id);
        let (name, sub) = omsi_lan::game::tag_text(r, d, radio, speaking, &|s| omsi_ui::tr(s).into_owned(), &|m| crate::units::Units::now().distance(m));
        let alpha = if radio { 1.0 } else { (1.0 - ((d as f32 - 300.0) / 150.0)).clamp(0.0, 1.0) };
        tags.push(((screen_x, (1.0 - y) * 0.5 * height), name, sub, alpha));
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The natural weather and the cycle need no file: a client takes them from any host.
    #[test]
    fn a_hosts_natural_weather_or_cycle_is_taken_without_a_file() {
        use clap::Parser;
        let full = crate::cli::Args::parse_from(["openomsi"]);
        let args = full.lan_config();
        let host = AppLanHost { args: &full, world: None };
        for w in ["natural", "Natural", "cycle"] {
            assert_eq!(host_weather(&args, &host, w), Ok(Some(w.to_string())), "{w}");
        }
        assert_eq!(host_weather(&args, &host, ""), Ok(None));
        assert!(host_weather(&args, &host, "weather/none_such.owt").is_err());
    }
}
