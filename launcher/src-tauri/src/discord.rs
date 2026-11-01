//! The launcher's Discord status ("In launcher - Preparing a drive"), as the game's own launcher
//! window kept it: on while the setting `discord_status` is (off by default: it tells friends what is driven), off while a game
//! starts or runs (the game then shows its own, see the game's `integrations`). A thread looks
//! every two seconds; Discord not running is tried again every five.

use omsi_platform::discord::{Discord, Presence};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Start the thread that keeps the status (not in the self-test).
pub fn start() {
    let _ = std::thread::Builder::new().name("discord".into()).spawn(run);
}

fn run() {
    let mut discord: Option<Discord> = None;
    let mut next_try = Instant::now();
    loop {
        keep(&mut discord, &mut next_try);
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn keep(discord: &mut Option<Discord>, next_try: &mut Instant) {
    let settings = omsi_launcher_lib::get_settings().unwrap_or_default();
    let enabled = settings.get("discord_status").and_then(|v| v.as_bool()).unwrap_or(false);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let games = omsi_launcher_lib::instances::list();
    let game_running = games.iter().any(|g| g.running);
    // (a game just started that has not told the registry it runs yet)
    let launching = games.iter().any(|g| !g.running && g.stopping.is_none() && now.saturating_sub(g.started) < 15);
    let presence = Presence::for_launcher(enabled, launching, game_running);
    if presence.is_none() {
        if let Some(d) = discord.as_ref() {
            d.stop();
            if d.is_finished() {
                discord.take();
            }
        }
        return;
    }
    if discord.as_ref().is_some_and(|d| d.is_stopping()) {
        if !discord.as_ref().is_some_and(|d| d.is_finished()) {
            return;
        }
        discord.take();
    }
    if discord.is_none() {
        if Instant::now() < *next_try {
            return;
        }
        *next_try = Instant::now() + Duration::from_secs(5);
        let app_id = settings.get("discord_app_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        *discord = Discord::start(&app_id);
    }
    if let Some(d) = discord.as_ref() {
        d.set(presence);
    }
}
