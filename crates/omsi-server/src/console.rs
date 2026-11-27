//! Interactive dedicated-server console (local stdin only).
//!
//! The reader never touches LAN or world state. The server's simulation thread
//! drains a bounded queue and executes commands on its own tick, through the
//! existing administrative command path. No extra web endpoint is opened.

use std::io::{self, BufRead};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, mpsc};

const QUEUE_LIMIT: usize = 128;
const LINE_LIMIT: usize = 512;
const PER_TICK: usize = 16;

static INPUT: OnceLock<Mutex<mpsc::Receiver<String>>> = OnceLock::new();
static STOP: AtomicBool = AtomicBool::new(false);
static RELOAD: AtomicBool = AtomicBool::new(false);
static RESTART: AtomicBool = AtomicBool::new(false);

/// Start reading lines in a separate thread. In non-interactive containers with
/// closed stdin this thread simply exits; the host continues to serve players.
pub fn start() {
    let (tx, rx) = mpsc::sync_channel(QUEUE_LIMIT);
    if INPUT.set(Mutex::new(rx)).is_err() {
        return;
    }
    let result = std::thread::Builder::new()
        .name("server console".into())
        .spawn(move || {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(text) => {
                        let Some(command) = normalize(&text) else {
                            continue;
                        };
                        if tx.try_send(command.to_owned()).is_err() {
                            log::warn!("server console: command queue full or closed");
                        }
                    }
                    Err(error) => {
                        log::warn!("server console: stdin read failed: {error}");
                        break;
                    }
                }
            }
        });
    if let Err(error) = result {
        log::warn!("server console: unable to read stdin: {error}");
    } else {
        log::info!("server console: type help for commands (stdin, no remote console access)");
    }
}

/// Leading slash is optional; never accept multiple-line/control input.
fn normalize(text: &str) -> Option<&str> {
    let text = text.trim();
    let text = text.strip_prefix('/').unwrap_or(text).trim();
    (!text.is_empty() && text.len() <= LINE_LIMIT && !text.chars().any(char::is_control))
        .then_some(text)
}

pub fn take() -> Vec<String> {
    let Some(queue) = INPUT.get() else {
        return Vec::new();
    };
    let Ok(rx) = queue.lock() else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for _ in 0..PER_TICK {
        match rx.try_recv() {
            Ok(line) => result.push(line),
            Err(_) => break,
        }
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Builtin {
    Help,
    List,
    Status,
    Tps,
    Stop,
    Reload,
    Restart,
    UpdateCheck,
    UpdateDownload,
    UpdateStatus,
    TimingStart,
    TimingStop,
    TimingReset,
    TimingReport,
}

fn builtin(command: &str) -> Option<Builtin> {
    match command {
        "help" | "?" => Some(Builtin::Help),
        "list" | "players" => Some(Builtin::List),
        "status" => Some(Builtin::Status),
        "tps" => Some(Builtin::Tps),
        "stop" => Some(Builtin::Stop),
        "reload" => Some(Builtin::Reload),
        "restart" => Some(Builtin::Restart),
        "update" | "update check" => Some(Builtin::UpdateCheck),
        "update download" => Some(Builtin::UpdateDownload),
        "update status" => Some(Builtin::UpdateStatus),
        "timings on" | "timings start" | "spark start" | "profile start" => {
            Some(Builtin::TimingStart)
        }
        "timings off" | "timings stop" | "spark stop" | "profile stop" => Some(Builtin::TimingStop),
        "timings reset" | "spark reset" | "profile reset" => Some(Builtin::TimingReset),
        "timings" | "timings report" | "spark" | "spark report" | "profile" | "profile report" => {
            Some(Builtin::TimingReport)
        }
        _ => None,
    }
}

/// Handle commands that do not alter game state. All other accepted admin
/// actions are validated and executed by the LAN admin command implementation.
pub fn run_builtin(
    command: &str,
    lan: &omsi_net::LanSession,
    metrics: &omsi_admin::metrics::TickMeter,
    profiler: &mut omsi_admin::profiler::Profiler,
) -> bool {
    match builtin(command) {
        Some(Builtin::Help) => {
            log::info!(
                "server console commands: help, list, status, tps, reload, restart, update, stop"
            );
            log::info!("timings on|off|report|reset (also: spark start|stop|report|reset)");
            log::info!(
                "update [check|status|download]: official headless ZIP, SHA-256 verified; install only after stopping the process"
            );
            log::info!(
                "administration: say <message>, kick <id> [reason], ban <id> [reason], unban <ban-id>, hwban <id> <session|all> [duration] [reason]"
            );
            log::info!(
                "world: clock <seconds>, time <offset-seconds>, speed <factor>, weather next|set <file>, traffic <count|clear>, passengers <density>"
            );
            log::info!(
                "players: tell <id> <message>, vmute <id>, vunmute <id>, unstick <id>, service <repair|refuel|wash> <id|all>"
            );
        }
        Some(Builtin::List) => {
            let players: Vec<_> = lan
                .peers()
                .filter(|p| p.has_info)
                .map(|p| {
                    format!(
                        "{} (#{}): {}",
                        omsi_admin::clean(&p.pose.name, 64),
                        p.pose.id,
                        omsi_admin::clean(&p.pose.bus, 100)
                    )
                })
                .collect();
            log::info!(
                "server console: {} player(s): {}",
                players.len(),
                if players.is_empty() {
                    "none".into()
                } else {
                    players.join(", ")
                }
            );
        }
        Some(Builtin::Status) => {
            let players = lan.peers().filter(|p| p.has_info).count();
            let tps = metrics
                .tps()
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "warming up".into());
            log::info!(
                "server console: online, {players} players, TPS {tps}/{:.0}",
                metrics.target()
            );
        }
        Some(Builtin::Tps) => match metrics.tps() {
            Some(rate) => log::info!(
                "server console: TPS {rate:.1}/{:.0} (completed ticks per wall-clock second)",
                metrics.target()
            ),
            None => {
                log::info!("server console: TPS warming up (first one-second sample not finished)")
            }
        },
        Some(Builtin::TimingStart) => {
            profiler.start();
            log::info!("server console: timings enabled (recording a new bounded sample)");
        }
        Some(Builtin::TimingStop) => {
            profiler.stop();
            log::info!("server console: timings disabled (report retained)");
        }
        Some(Builtin::TimingReset) => {
            profiler.reset();
            log::info!("server console: timings reset");
        }
        Some(Builtin::TimingReport) => {
            let report = profiler.report();
            log::info!(
                "server console: timings {}, {} ticks in {:.1} s, tick avg {:.2} ms, p95 {:.2} ms, p99 {:.2} ms, max {:.2} ms",
                if report.enabled { "running" } else { "stopped" },
                report.ticks,
                report.seconds,
                report.avg_tick_ms,
                report.p95_tick_ms,
                report.p99_tick_ms,
                report.max_tick_ms
            );
            for entry in report.stages {
                log::info!(
                    "server console: {:<18} mean {:.3} ms, max {:.3} ms, total {:.1} ms ({:.1}%), calls {}",
                    entry.name,
                    entry.avg_ms,
                    entry.max_ms,
                    entry.total_ms,
                    entry.share_pct,
                    entry.calls
                );
            }
        }
        Some(Builtin::Reload) => {
            RELOAD.store(true, Ordering::SeqCst);
            log::info!("server console: config reload requested (on next simulation tick)");
        }
        Some(Builtin::Restart) => {
            RESTART.store(true, Ordering::SeqCst);
            log::info!(
                "server console: in-session simulation restart requested (LAN identity retained)"
            );
        }
        Some(Builtin::UpdateCheck) => crate::updates::start(false),
        Some(Builtin::UpdateDownload) => crate::updates::start(true),
        Some(Builtin::UpdateStatus) => crate::updates::report(),
        Some(Builtin::Stop) => {
            STOP.store(true, Ordering::SeqCst);
            log::info!("server console: graceful shutdown requested");
        }
        None => return false,
    }
    true
}

/// One-shot local-console request. The simulation thread alone validates
/// and applies the config; no input thread mutates host/network state.
pub fn take_reload_requested() -> bool {
    RELOAD.swap(false, Ordering::SeqCst)
}

/// Restart only the CPU map/simulation while retaining the owning process,
/// session key/code and WebSocket gateway. Consumed only by the tick thread.
pub fn take_restart_requested() -> bool {
    RESTART.swap(false, Ordering::SeqCst)
}

pub fn stop_requested() -> bool {
    STOP.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_exact_builtins_and_a_single_safe_line() {
        assert_eq!(normalize(" /tps "), Some("tps"));
        assert_eq!(normalize(" say hello there "), Some("say hello there"));
        assert_eq!(normalize("stop\nstatus"), None);
        assert_eq!(normalize(" ".repeat(LINE_LIMIT + 1).as_str()), None);
        assert_eq!(builtin("stop"), Some(Builtin::Stop));
        assert_eq!(builtin("reload"), Some(Builtin::Reload));
        assert_eq!(builtin("restart"), Some(Builtin::Restart));
        assert_eq!(builtin("update"), Some(Builtin::UpdateCheck));
        assert_eq!(builtin("update check"), Some(Builtin::UpdateCheck));
        assert_eq!(builtin("update download"), Some(Builtin::UpdateDownload));
        assert_eq!(builtin("update status"), Some(Builtin::UpdateStatus));
        assert_eq!(builtin("update apply"), None);
        assert_eq!(builtin("restart now"), None);
        assert_eq!(normalize(" /reload "), Some("reload"));
        assert_eq!(builtin("reload now"), None);
        assert_eq!(builtin("stop now"), None);
        assert_eq!(builtin("tps"), Some(Builtin::Tps));
        assert_eq!(builtin("timings on"), Some(Builtin::TimingStart));
        assert_eq!(builtin("spark stop"), Some(Builtin::TimingStop));
        assert_eq!(builtin("admin password"), None);
    }
}
