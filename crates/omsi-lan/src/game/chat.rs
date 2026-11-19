//! LAN chat: opening and typing into the chat line, swallowing keys while open,
//! admin/reconnect commands, and commands passed to plugins (`chat_command`, #2536).

use super::{LanGame, filter_chat};
use omsi_net::LanSession;
use std::time::Instant;

/// A key while LAN play runs: the key bound to `chat_open` ('/' or '`', see
/// `KeyboardCfg::with_game_defaults`) opens the chat line, the one bound to `chat_toggle` (V) hides and shows the chat - `bound` is the `[game]` action of `Inputs/keyboard.cfg` the
/// key makes with the modifiers held. While the line is open every key is the chat's
/// (Enter sends, Escape drops the line, Backspace takes a character back).
/// Returns whether the key was taken. Text arrives through `chat_type`.
pub fn chat_key(
    lan: &mut LanSession,
    game: &mut LanGame,
    key: ChatKey,
    pressed: bool,
    repeat: bool,
    bound: Option<&str>,
) -> bool {
    let chat = &mut game.chat;
    if chat.disabled {
        return false;
    }
    if chat.typing.is_none() {
        let toggle = bound.is_some_and(|a| a.eq_ignore_ascii_case("chat_toggle"));
        let open = bound.is_some_and(|a| a.eq_ignore_ascii_case("chat_open"));
        if pressed && !repeat && (toggle || open) {
            if toggle {
                chat.hidden = !chat.hidden;
            } else {
                chat.open();
            }
            chat.swallow.insert(key.id);
            return true;
        }
        // the release of the key that opened or sent the line
        return !pressed && chat.swallow.remove(&key.id);
    }
    if !pressed {
        return chat.swallow.remove(&key.id);
    }
    chat.swallow.insert(key.id);
    match key.role {
        ChatRole::Send => {
            let text = chat.typing.take().unwrap_or_default();
            chat_send(lan, game, &text);
        }
        ChatRole::Cancel => chat.typing = None,
        ChatRole::Erase => {
            if let Some(t) = chat.typing.as_mut() {
                t.pop();
            }
        }
        ChatRole::Other => {}
    }
    true
}

/// A key as the chat line takes it: which physical key it is (its release belongs to the
/// chat too, `id`: any number the game gives that key alone) and what it does in an open line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatKey {
    pub id: u64,
    pub role: ChatRole,
}

/// What a key does in an open chat line: Enter sends it, Escape drops it, Backspace takes
/// the last character off; the others type (their text comes through `chat_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    Send,
    Cancel,
    Erase,
    Other,
}

/// The key that opened the chat: its release is the chat's too.
pub fn chat_swallow(game: &mut LanGame, key: ChatKey) {
    game.chat.swallow.insert(key.id);
}

/// Characters typed into an open chat line.
pub fn chat_type(game: &mut LanGame, text: &str) {
    if let Some(t) = game.chat.typing.as_mut() {
        for c in text.chars().filter(|c| !c.is_control()) {
            if t.chars().count() < omsi_net::MAX_CHAT {
                t.push(c);
            }
        }
    }
}

/// Is the chat line open (the keys are the chat's)?
pub fn chat_open(game: &LanGame) -> bool {
    game.chat.typing.is_some()
}

/// Recognise private admin commands without case-sensitive password leaks.
pub(crate) fn admin_chat_arg(text: &str) -> Option<&str> {
    let text = text.trim();
    if !text.get(..6)?.eq_ignore_ascii_case("/admin") {
        return None;
    }
    let rest = text.get(6..)?;
    if rest.chars().next().is_some_and(|c| !c.is_whitespace()) {
        return None;
    }
    Some(rest.trim())
}

/// What a line typed into the session's chat is: the session's own `/admin` (its password) and
/// `/reconnect`, a `/`-line for the plugins (`chat_command`, #2536), or something said to
/// everybody.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatLine {
    /// `/admin <password>`: the server's administration, never said to anybody.
    Admin(String),
    /// `/reconnect`: the host joined again.
    Reconnect,
    /// Any other `/`-line: a command, for whoever takes it (the plugins, when one hears
    /// `chat_command`), never public chat.
    Command(String),
    /// A line everybody hears.
    Say(String),
}

/// What `text`, typed into the chat, is (see [`ChatLine`]).
pub fn chat_line(text: &str) -> ChatLine {
    // Keep every case variant of /admin private: it must never reach lan.say().
    if let Some(pw) = admin_chat_arg(text) {
        return ChatLine::Admin(pw.to_string());
    }
    let t = text.trim();
    if t.eq_ignore_ascii_case("/reconnect") {
        return ChatLine::Reconnect;
    }
    if t.len() > 1 && t.starts_with('/') {
        return ChatLine::Command(t.to_string());
    }
    ChatLine::Say(t.to_string())
}

fn admin_request(lan: &mut LanSession, game: &mut LanGame, password: &str) {
    game.admin_password = Some(password.to_string());
    lan.command(1, "auth?");
}

/// Say `text` to everybody.
pub fn chat_send(lan: &mut LanSession, game: &mut LanGame, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    match chat_line(text) {
        ChatLine::Admin(pw) => {
            if pw.is_empty() {
                game.chat.push("* usage: /admin <password>".into());
            } else {
                admin_request(lan, game, &pw);
                game.chat
                    .push("* asked the server for its administration".into());
            }
        }
        ChatLine::Reconnect => {
            if lan.reconnect() {
                game.chat.push("* reconnecting ...".into());
            } else {
                game.chat.push("* only a joined game can reconnect".into());
            }
        }
        // (#2536) a `/`-line is a command: the plugins' while one of them hears it, and never
        // something the other players read
        ChatLine::Command(line) if game.take_commands => {
            if game.plugin_commands.len() < 32 {
                game.plugin_commands.push(line);
            }
            game.chat.push("* sent to the plugins".into());
        }
        // (no plugin hears `chat_command`: a `/`-line is chat, as it always was)
        ChatLine::Command(line) => chat_say(lan, game, &line),
        ChatLine::Say(said) => chat_say(lan, game, &said),
    }
}

/// `text` to everybody, as the chat line's Enter sends it.
fn chat_say(lan: &mut LanSession, game: &mut LanGame, text: &str) {
    let text = filter_chat(text.trim());
    match lan.say(&text) {
        Ok(()) => game.chat.error = None,
        Err(e) => {
            log::info!("LAN chat not sent: {e}");
            game.chat.error = Some((e, Instant::now()));
        }
    }
}
