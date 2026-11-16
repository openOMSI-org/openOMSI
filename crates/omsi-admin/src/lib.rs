//! A dedicated server's administration, the part that needs no game: who may do what
//! (`perm`), the admins' sessions and the door's lock (`auth`), the operations they ask for
//! and their checks (`op`), the players as the admins see and pick them (`players`), the
//! audit log (`audit`), and the hub that ties them together for the server's threads (`hub`).
//!
//! Every way of administering a server - the game's Administration menu of a player who gave
//! the password, a tool on the server's machine (`POST /admin`), the web panel - ends in the
//! same place: an operation, checked against the role of whoever asks, carried out by the
//! server's own loop on its own tick (never by a web request's thread), and recorded with what
//! became of it (`Status`). A request that was taken is not one that was done: the record
//! says `queued` until the loop has applied or refused it.

pub mod audit;
pub mod auth;
pub mod http;
pub mod hub;
pub mod op;
pub mod perm;
pub mod players;

pub use audit::{Audit, Entry};
pub use auth::{Auth, AuthCfg, LoginError, Passwords, SessionInfo};
pub use http::{ProxyHeader, WebCfg, WebMode};
pub use hub::{Actor, BanView, Hub, Job, OpRecord, Origin, Outcome, ServerState, Snapshot, Status};
pub use op::{
    BanScope, MAX_BAN_HOURS, Op, PASSENGER_DENSITY_MAX, Target, TrafficOp, WeatherOp, clean,
    passenger_density, weather_file_ok,
};
pub use perm::{Perm, Role};
pub use players::{Player, PlayerState, TargetError, resolve, search};

/// Seconds since 1970 (0 on a clock before it).
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
