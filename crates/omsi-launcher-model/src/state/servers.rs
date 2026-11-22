use omsi_launcher_lib as core;
use serde::{Deserialize, Serialize};

/// What a server is shown as in the Multiplayer page: their addresses, and a bus fleet of
/// their own (empty: the server's).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ServerEntry {
    pub name: String,
    pub address: String,
    pub protocol: JoinProto,
}

/// How the Multiplayer page joins a server.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JoinProto {
    /// Straight to the game port over UDP (gateway status still required).
    Udp,
    /// Through the server's WebSocket gateway (`ws://` or `wss://`), or an official relay
    /// through it.
    WebSocket,
    /// Auto joins WebSocket when the address is a URL or name, and UDP when the address is an
    /// IP:port.
    Auto,
}

/// Select the gameplay address only after a gateway has answered. A bare explicit
/// port in Auto names the game port unless that exact HTTP endpoint answered status.
pub(crate) fn server_join_address(
    address: &str,
    proto: JoinProto,
    info: Option<&omsi_net::ws::ServerInfo>,
) -> Result<(String, JoinProto), &'static str> {
    let address = address.trim();
    let is_web = omsi_net::ws::ws_url(address).is_some() || omsi_net::official::is_alias(address);
    let explicit_port = address.rsplit_once(':').is_some_and(|(host, port)| {
        !host.is_empty()
            && !host.contains(char::is_whitespace)
            && !port.is_empty()
            && port.bytes().all(|b| b.is_ascii_digit())
            && port.parse::<u16>().is_ok()
    });
    let proto = match proto {
        JoinProto::Auto
            if !is_web
                && explicit_port
                && !info.is_some_and(|i| i.reached_at == format!("http://{address}")) =>
        {
            JoinProto::Udp
        }
        JoinProto::Auto => JoinProto::WebSocket,
        p => p,
    };
    if proto == JoinProto::Udp && is_web {
        return Err("UDP needs the game's address (host or host:port), not a web link");
    }
    let info =
        info.ok_or("The server has not answered yet (is its address right? is it running?)")?;
    let target = if proto == JoinProto::WebSocket && !is_web && !info.reached_at.is_empty() {
        info.reached_at.clone()
    } else {
        address.to_string()
    };
    Ok((target, proto))
}

/// A code host's first responsive status endpoint, including its tunnel.
pub(crate) fn host_status(code: &str) -> Result<omsi_net::ws::ServerInfo, String> {
    omsi_net::ws::query_session(code)
}

/// The list as saved, with the official server first when it is not in it.
pub(crate) fn with_official(mut list: Vec<ServerEntry>) -> Vec<ServerEntry> {
    if !list
        .iter()
        .any(|s| omsi_net::official::is_alias(&s.address))
    {
        list.insert(
            0,
            ServerEntry {
                name: omsi_net::official::NAME.into(),
                address: omsi_net::official::ALIAS.into(),
                protocol: JoinProto::Auto,
            },
        );
    }
    list
}

pub(crate) fn servers_path() -> std::path::PathBuf {
    core::data_dir().join("servers.json")
}
