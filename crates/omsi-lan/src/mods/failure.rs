//! Why a request to another machine's mods did not get its answer: a kind the players' buses
//! decide by (ask again soon, give up, fall back to an older host), and the same words as
//! ever for the log and the player.
//!
//! The connection code the map download shares (`open_waiting`, `read_reply`) says what went
//! wrong in words. They are read here, once (`From<String>`: an anti-corruption layer between
//! that code and the players' buses): `connect` and `reply` hand our code the kind, and
//! nothing else looks into an error's text.

use std::fmt;
use std::io::BufRead;
use std::net::SocketAddr;
use std::time::Duration;

/// The words a host answers `ERR` with, which both sides write and read.
pub(crate) mod says {
    /// A bus not kept on the host (it may be on its way).
    pub(crate) const NO_SUCH_BUS: &str = "no such bus";
    /// A file the host does not keep (or one named wrongly).
    pub(crate) const NO_SUCH_FILE: &str = "no such file";
    /// Too many connections or too much held at once: come back.
    pub(crate) const BUSY: &str = "busy";
    /// A request the host does not know: a host of a version before it.
    pub(crate) const WHAT: &str = "what";
    /// A host that takes no buses from its players.
    pub(crate) const TAKES_NO_UPLOADS: &str = "this server takes no uploads";
}

/// What a request to the host came to, when it did not get its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    /// No way to the host is known yet (its mods were not fetched).
    NoHost,
    /// The host could not be reached (the whole line, with the address and the system's words).
    Unreachable(String),
    /// The connection broke off or was closed (what happened).
    ConnectionLost(String),
    /// The host answered `ERR` with these words (see `says`).
    HostSays(String),
    /// Nobody here drives the bus any more.
    NotDriven,
    /// A server waited for a bus that did not come.
    NotHereYet,
    /// Anything else: a bad answer, a check that failed here.
    Other(String),
}

const HOST_SAYS: &str = "the host says: ";
const NO_HOST: &str = "no host to ask yet";
const NOT_DRIVEN: &str = "no longer driven here";
const NOT_HERE_YET: &str = "no such bus here yet";
const UNREACHABLE: &str = "cannot reach ";

impl Failure {
    /// Did the host answer with these words?
    pub(crate) fn host_says(&self, words: &str) -> bool {
        matches!(self, Failure::HostSays(w) if w == words)
    }

    /// A host of a version before the request (it answered `ERR what`).
    pub(crate) fn older_host(&self) -> bool {
        self.host_says(says::WHAT)
    }

    /// Asking failed before the host could answer: no way to it yet, out of reach, the
    /// connection, or a host too busy for one more.
    pub(crate) fn unreached(&self) -> bool {
        matches!(self, Failure::NoHost | Failure::Unreachable(_) | Failure::ConnectionLost(_)) || self.host_says(says::BUSY)
    }

    /// A bus that is still to come: asked for again, never given up.
    pub(crate) fn coming(&self) -> bool {
        matches!(self, Failure::NotDriven | Failure::NotHereYet) || self.host_says(says::NO_SUCH_BUS) || self.unreached()
    }

    /// How long to wait before asking again: soon for a bus on its way, a little longer for a
    /// host out of reach, `otherwise` for anything else.
    pub(crate) fn retry_after(&self, otherwise: Duration) -> Duration {
        if matches!(self, Failure::NoHost | Failure::NotDriven | Failure::NotHereYet) || self.host_says(says::NO_SUCH_BUS) {
            Duration::from_secs(5)
        } else if self.unreached() {
            Duration::from_secs(15)
        } else {
            otherwise
        }
    }

    /// The same failure, about `what` (a file): the context for the log, the kind kept.
    pub(crate) fn about(self, what: &str) -> Failure {
        match self {
            Failure::ConnectionLost(s) => Failure::ConnectionLost(format!("{what}: {s}")),
            Failure::Other(s) => Failure::Other(format!("{what}: {s}")),
            f => f,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::NoHost => f.write_str(NO_HOST),
            Failure::Unreachable(line) => f.write_str(line),
            Failure::ConnectionLost(what) => write!(f, "{}: {what}", super::CONNECTION_LOST),
            Failure::HostSays(words) => write!(f, "{HOST_SAYS}{words}"),
            Failure::NotDriven => f.write_str(NOT_DRIVEN),
            Failure::NotHereYet => f.write_str(NOT_HERE_YET),
            Failure::Other(s) => f.write_str(s),
        }
    }
}

/// The words of the shared connection code, read as a kind (the one place that does).
impl From<String> for Failure {
    fn from(s: String) -> Failure {
        let lost = format!("{}: ", super::CONNECTION_LOST);
        if let Some(what) = s.strip_prefix(&lost) {
            Failure::ConnectionLost(what.to_string())
        } else if let Some(words) = s.strip_prefix(HOST_SAYS) {
            Failure::HostSays(words.to_string())
        } else if s.starts_with(UNREACHABLE) {
            Failure::Unreachable(s)
        } else if s == NO_HOST || s == "no host to ask" {
            Failure::NoHost
        } else {
            Failure::Other(s)
        }
    }
}

impl From<&str> for Failure {
    fn from(s: &str) -> Failure {
        Failure::from(s.to_string())
    }
}

/// A connection to the host's mods, greeted (`open_waiting`), its failure as a kind.
pub(super) fn connect(host: SocketAddr, session: u64, wait: Duration) -> Result<super::Connection, Failure> {
    super::open_waiting(host, session, wait).map_err(Failure::from)
}

/// A connection to the host's mods with the usual wait (`open`), its failure as a kind.
pub(super) fn open(host: SocketAddr, session: u64) -> Result<super::Connection, Failure> {
    super::open(host, session).map_err(Failure::from)
}

/// The host's answer to a request (`read_reply`: `OK [n]`), its failure as a kind.
pub(super) fn reply(input: &mut impl BufRead) -> Result<Option<u64>, Failure> {
    super::read_reply(input).map_err(Failure::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_of_the_connection_code_become_kinds_and_read_the_same() {
        let lost = format!("{}: closed by the host", super::super::CONNECTION_LOST);
        let cases = [
            (lost.as_str(), Failure::ConnectionLost("closed by the host".into())),
            ("the host says: no such bus", Failure::HostSays(says::NO_SUCH_BUS.into())),
            ("the host says: busy", Failure::HostSays(says::BUSY.into())),
            ("cannot reach the host's mods on TCP 127.0.0.1:1: refused", Failure::Unreachable("cannot reach the host's mods on TCP 127.0.0.1:1: refused".into())),
            ("no host to ask yet", Failure::NoHost),
            ("bad list: expected value", Failure::Other("bad list: expected value".into())),
        ];
        for (words, kind) in cases {
            let f = Failure::from(words);
            assert_eq!(f, kind, "{words}");
            // (the log and the player read what they read before)
            assert_eq!(f.to_string(), words);
        }
    }

    #[test]
    fn what_is_still_to_come_is_asked_for_again_and_how_soon() {
        let minute = Duration::from_secs(60);
        assert!(Failure::HostSays(says::NO_SUCH_BUS.into()).coming());
        assert!(Failure::NotDriven.coming() && Failure::NotHereYet.coming() && Failure::NoHost.coming());
        assert!(Failure::HostSays(says::BUSY.into()).unreached());
        assert!(!Failure::Other("the host's list is another bus".into()).coming());
        assert!(Failure::HostSays(says::WHAT.into()).older_host());
        assert_eq!(Failure::HostSays(says::NO_SUCH_BUS.into()).retry_after(minute), Duration::from_secs(5));
        assert_eq!(Failure::ConnectionLost("reset".into()).retry_after(minute), Duration::from_secs(15));
        assert_eq!(Failure::Other("bad".into()).retry_after(minute), minute);
        // (a file's name for the log, the kind kept)
        assert!(Failure::ConnectionLost("reset".into()).about("Vehicles/X/a.dds").unreached());
        assert!(Failure::HostSays(says::WHAT.into()).about("Vehicles/X/a.dds").older_host());
    }
}
