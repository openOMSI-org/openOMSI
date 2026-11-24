//! A dedicated server in the public server list (`public_server = 1` in `server.cfg`): its
//! listing is built from the server's own status page and checked by the moderation of
//! `omsi-serverlist` before the first word to the service. See `docs/SERVER.md`.

use crate::server_cfg::ServerCfg;
pub use omsi_serverlist::publish::Handle;
use omsi_serverlist::publish::{self, Listing};
use std::sync::Arc;

/// Start listing the server; the reason it cannot be public otherwise (for the log).
/// `version` is the game's version as the status page shows it. The returned handle takes
/// the server off the list when dropped.
pub fn start(cfg: &ServerCfg, version: &str) -> Result<Handle, String> {
    let base = Listing {
        name: cfg.name.clone(),
        motd: cfg.motd.clone(),
        map: cfg.map.clone(),
        max_players: cfg.max_players,
        version: version.to_string(),
        protocol: omsi_net::PROTOCOL,
        web_port: cfg.web_port,
        ..Default::default()
    };
    let (name, motd, map, max, version, web_port) = (
        cfg.name.clone(),
        cfg.motd.clone(),
        cfg.map.clone(),
        cfg.max_players,
        version.to_string(),
        cfg.web_port,
    );
    let now = move || {
        let info = crate::gateway::server_info_now();
        Listing {
            name: name.clone(),
            motd: motd.clone(),
            map: map.clone(),
            players: info.as_ref().map(|i| i.players).unwrap_or(0),
            max_players: max,
            version: version.clone(),
            protocol: omsi_net::PROTOCOL,
            // (the port the gateway really got: the one asked for may have been taken)
            web_port: crate::gateway::web_port().unwrap_or(web_port),
            address: crate::gateway::tunnel_url(),
            icon: None,
        }
    };
    let icon = Some(cfg.icon.clone()).filter(|i| !i.is_empty());
    publish::start(
        &omsi_serverlist::service_url(&cfg.public_list_url),
        base,
        icon,
        Arc::new(omsi_serverlist::BuiltinModerator),
        Arc::new(now),
        Arc::new(crate::gateway::set_list_challenge),
    )
}
