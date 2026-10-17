//! Who may do what. A role comes from the password that was given (`admin_password` or
//! `moderator_password` of `server.cfg`), decided by the server alone: nothing a client sends
//! names or raises it.

use serde::Serialize;

/// What an operation needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Perm {
    /// See the server: its state, its players (with what they drive), the events.
    View,
    /// The players: send one away, a word to everybody or one, a duty, a service.
    Moderate,
    /// The shared world: the clock and its speed, the weather, the traffic, the passengers.
    World,
}

/// An admin's role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Sees the server and moderates its players; leaves the world alone.
    Moderator,
    /// Everything.
    Admin,
}

impl Role {
    pub fn allows(self, p: Perm) -> bool {
        match self {
            Role::Admin => true,
            Role::Moderator => matches!(p, Perm::View | Perm::Moderate),
        }
    }

    /// What the role may do, for the panels (which hide what is not allowed - the server
    /// checks every operation all the same).
    pub fn perms(self) -> Vec<Perm> {
        [Perm::View, Perm::Moderate, Perm::World].into_iter().filter(|p| self.allows(*p)).collect()
    }

    pub fn name(self) -> &'static str {
        match self {
            Role::Moderator => "moderator",
            Role::Admin => "admin",
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        match s.trim() {
            "moderator" => Some(Role::Moderator),
            "admin" => Some(Role::Admin),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moderator_leaves_the_world_alone() {
        assert!(Role::Admin.allows(Perm::World) && Role::Admin.allows(Perm::Moderate) && Role::Admin.allows(Perm::View));
        assert!(Role::Moderator.allows(Perm::View) && Role::Moderator.allows(Perm::Moderate));
        assert!(!Role::Moderator.allows(Perm::World));
        assert_eq!(Role::Moderator.perms(), vec![Perm::View, Perm::Moderate]);
        assert_eq!(Role::parse(Role::Admin.name()), Some(Role::Admin));
        assert_eq!(Role::parse("root"), None);
    }
}
