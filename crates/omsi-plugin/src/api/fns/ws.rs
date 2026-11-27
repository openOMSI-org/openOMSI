//! `omsi.ws`: the read-only WebSocket feeds of a plugin (#3098). Only `wss://` addresses on
//! the hosts the player allowed are opened, nothing is sent, and a plugin holds three.

use super::{def, p};
use crate::api::{ApiError, ApiFn, Value};

const NEW: &str = crate::api::VERSION;

pub(crate) static FNS: &[ApiFn] = &[
    def!(
        "ws.open",
        "network",
        [p("url", "string")],
        "the feed's slot (0-2), or an error",
        "Opens a read-only WebSocket feed to `url` in the first free slot. Only `wss://` addresses are opened, with no port of its own, no user name or password and no literal IP address, and only to a host the player allowed in `websocket-hosts.txt` in the plugins folder; the plugin never sends anything, it only drains what the server pushes. A plugin holds 3 feeds at the same time.",
        NEW,
        NetworkRemote,
        false,
        |c, a| {
            let url = a.str(0)?;
            let hosts = crate::ws::hosts();
            c.state().feeds.open_with(&url, &hosts).map_err(ApiError)
        }
    ),
    def!(
        "ws.poll",
        "network",
        [p("slot", "integer")],
        "a list of the text messages that arrived",
        "Takes what arrived in the feed of `slot` since the last call (oldest first, at most a few dozen are kept: a slow plugin loses the oldest). Each message is the text of one WebSocket message, at most 64 KB.",
        NEW,
        NetworkRemote,
        false,
        |c, a| {
            let slot = a.int(0)?;
            let slot = slot
                .try_into()
                .map_err(|_| ApiError("omsi.ws.poll: argument 1 (slot) must be 0-2".into()))?;
            let messages = c
                .state()
                .feeds
                .poll(slot)
                .map_err(|e| ApiError(format!("omsi.ws.poll: {e}")))?;
            Ok::<_, ApiError>(Value::List(messages.into_iter().map(Value::from).collect()))
        }
    ),
    def!(
        "ws.state",
        "network",
        [p("slot", "integer")],
        "`connecting`, `open`, `reconnecting`, `failed` or `closed`",
        "What the feed of `slot` is doing: it connects on its own thread, so the game never waits, and a connection that breaks is opened again after 5 seconds.",
        NEW,
        NetworkRemote,
        false,
        |c, a| {
            let slot = a.int(0)?;
            let slot = slot
                .try_into()
                .map_err(|_| ApiError("omsi.ws.state: argument 1 (slot) must be 0-2".into()))?;
            c.state()
                .feeds
                .state(slot)
                .map_err(|e| ApiError(format!("omsi.ws.state: {e}")))
        }
    ),
    def!(
        "ws.close",
        "network",
        [p("slot", "integer")],
        "true",
        "Closes the feed of `slot` and frees it for another one.",
        NEW,
        NetworkRemote,
        false,
        |c, a| {
            let slot = a.int(0)?;
            let slot = slot
                .try_into()
                .map_err(|_| ApiError("omsi.ws.close: argument 1 (slot) must be 0-2".into()))?;
            c.state()
                .feeds
                .close(slot)
                .map_err(|e| ApiError(format!("omsi.ws.close: {e}")))?;
            Ok::<_, ApiError>(true)
        }
    ),
];
