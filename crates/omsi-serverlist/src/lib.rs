//! openOMSI's public server list. A server with `public_server = 1` registers with a list
//! service (`services/serverlist`, a Cloudflare Worker) and says it is alive every minute;
//! the launcher and the game ask the service for the list. Nothing here trusts the service
//! or the servers with what is shown: every name, message and icon passes the same
//! `Moderator` on the server before it registers and on the client before it is shown
//! (`moderation`).
//!
//! * `moderation` - the `Moderator` trait, the built-in text rules and the icon check;
//! * `client` - fetching the list and making it ready to show;
//! * `publish` - a server's registration and heartbeat.

pub mod client;
pub mod moderation;
pub mod publish;

pub use moderation::{BuiltinModerator, IconVerdict, ImageClassifier, Moderator, Verdict};

/// The list service (a Cloudflare Worker, see `services/serverlist/README.md`).
pub const DEFAULT_SERVICE: &str = "https://openomsi-serverlist.savvabestbrother.workers.dev";

/// The service a setting names (`public_list_url`), else the default; only `https://`.
pub fn service_url(setting: &str) -> String {
    let s = setting.trim().trim_end_matches('/');
    if s.starts_with("https://") && s.len() > 8 && !s.contains(char::is_whitespace) {
        s.to_string()
    } else {
        DEFAULT_SERVICE.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_https_service_is_taken() {
        assert_eq!(service_url(""), DEFAULT_SERVICE);
        assert_eq!(service_url("http://insecure.example"), DEFAULT_SERVICE);
        assert_eq!(service_url("https://list.example.org/"), "https://list.example.org");
        assert_eq!(service_url("https://a b"), DEFAULT_SERVICE);
    }
}
