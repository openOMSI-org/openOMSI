//! The players as the admins see them, and how one is found and picked: a search by name
//! (whatever the case, the text taken as text - no pattern, no command), and an operation's
//! player checked against who is here now (`resolve`), so that it never reaches somebody who
//! has the number since - or a player who left and came back.

use crate::op::Target;
use crate::perm::Role;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerState {
    /// Joined, no state of theirs yet.
    Joining,
    Driving,
    OnFoot,
    /// Sitting in another player's bus (`Player::aboard`).
    Aboard,
}

impl PlayerState {
    pub fn name(self) -> &'static str {
        match self {
            PlayerState::Joining => "joining",
            PlayerState::Driving => "driving",
            PlayerState::OnFoot => "on_foot",
            PlayerState::Aboard => "aboard",
        }
    }

    pub fn parse(s: &str) -> Option<PlayerState> {
        [PlayerState::Joining, PlayerState::Driving, PlayerState::OnFoot, PlayerState::Aboard].into_iter().find(|p| p.name() == s)
    }
}

/// A player as the server knows them - what it really knows, nothing more: no address, no
/// position (the admins' list needs neither; `share_positions` is another matter).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Player {
    pub id: u32,
    /// The connection (see `Target`).
    pub conn: u64,
    /// As the player's game says it (cleaned): no proof of who it is.
    pub name: String,
    pub state: PlayerState,
    /// The vehicle file driven (empty: none, or on foot).
    pub vehicle: String,
    pub line: String,
    pub tour: String,
    /// Whose bus a player `Aboard` sits in.
    pub aboard: Option<u32>,
    /// Seconds since they joined.
    pub online_secs: u64,
    /// Administers the server from the game, with this role.
    pub admin: Option<Role>,
}

impl Player {
    pub fn target(&self) -> Target {
        Target { id: self.id, conn: self.conn }
    }

    /// `Name (#3)`, for the records and the confirmations.
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            format!("player {}", self.id)
        } else {
            format!("{} (#{})", self.name, self.id)
        }
    }
}

/// The players a search finds: the whole list for an empty one; else those whose name holds
/// the text (whatever the case), and the player of that number for `#3` or `3`. The exact
/// names first, then those that begin with it, then the rest, each by name and number - and
/// never just the first: two players called alike are both found, and the admin picks.
pub fn search<'a>(players: &'a [Player], query: &str) -> Vec<&'a Player> {
    let q = query.trim().to_lowercase();
    let by_id = q.strip_prefix('#').unwrap_or(&q).parse::<u32>().ok();
    let rank = |p: &Player| -> Option<u8> {
        let n = p.name.to_lowercase();
        if q.is_empty() {
            Some(3)
        } else if by_id == Some(p.id) {
            Some(0)
        } else if n == q {
            Some(1)
        } else if n.starts_with(&q) {
            Some(2)
        } else if n.contains(&q) {
            Some(3)
        } else {
            None
        }
    };
    let mut found: Vec<(u8, &Player)> = players.iter().filter_map(|p| rank(p).map(|r| (r, p))).collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase())).then(a.1.id.cmp(&b.1.id)));
    found.into_iter().map(|(_, p)| p).collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum TargetError {
    /// Nobody of that number is here: they left.
    Gone,
    /// Somebody of that number is here, on another connection: they left and came back, or
    /// it is another player. The name of who it is now.
    Replaced(String),
}

impl TargetError {
    pub fn message(&self) -> String {
        match self {
            TargetError::Gone => "that player has left".into(),
            TargetError::Replaced(now) => format!("that player is no longer here: number taken by another connection ({now}); refresh the list"),
        }
    }
}

/// The player an operation is for, if they are still the one picked.
pub fn resolve(players: &[Player], target: Target) -> Result<&Player, TargetError> {
    match players.iter().find(|p| p.id == target.id) {
        None => Err(TargetError::Gone),
        Some(p) if p.conn != target.conn => Err(TargetError::Replaced(p.label())),
        Some(p) => Ok(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: u32, conn: u64, name: &str) -> Player {
        Player { id, conn, name: name.into(), state: PlayerState::Driving, vehicle: String::new(), line: String::new(), tour: String::new(), aboard: None, online_secs: 0, admin: None }
    }

    fn ids(v: Vec<&Player>) -> Vec<u32> {
        v.into_iter().map(|p| p.id).collect()
    }

    /// The case the order names: `pichau` finds every player whose name holds it, the exact
    /// one first, and somebody called alike is never picked for them.
    #[test]
    fn a_search_finds_every_player_called_so() {
        let list = vec![p(2, 10, "Pichau"), p(3, 11, "pichau_fan"), p(4, 12, "Joao"), p(5, 13, "PICHAU"), p(6, 14, "xPichaux"), p(7, 15, "Pichu")];
        assert_eq!(ids(search(&list, "pichau")), vec![2, 5, 3, 6]);
        assert_eq!(ids(search(&list, "  PiChAu ")), vec![2, 5, 3, 6]);
        assert_eq!(ids(search(&list, "pichau_")), vec![3]);
        assert_eq!(ids(search(&list, "joao")), vec![4]);
        assert_eq!(ids(search(&list, "")).len(), 6);
        assert_eq!(ids(search(&list, "#7")), vec![7]);
        assert_eq!(ids(search(&list, "nobody")), Vec::<u32>::new());
    }

    /// The text is text: no pattern, no command, markup as letters.
    #[test]
    fn a_search_is_only_text() {
        let list = vec![p(2, 1, "a.b"), p(3, 1, "axb"), p(4, 1, "<b>bold</b>"), p(5, 1, "x; kick 2"), p(6, 1, "Ünïcødé")];
        assert_eq!(ids(search(&list, "a.b")), vec![2]);
        assert_eq!(ids(search(&list, ".*")), Vec::<u32>::new());
        assert_eq!(ids(search(&list, "<b>")), vec![4]);
        assert_eq!(ids(search(&list, "; kick")), vec![5]);
        assert_eq!(ids(search(&list, "ünï")), vec![6]);
    }

    /// An operation's player is the one picked, or it is refused: they left, or their number
    /// is somebody else's connection now (who left and came back is a new connection too).
    #[test]
    fn the_player_picked_or_nobody() {
        let list = vec![p(2, 10, "Pichau"), p(3, 11, "Pichau")];
        assert_eq!(resolve(&list, Target { id: 2, conn: 10 }).map(|p| p.conn), Ok(10));
        assert_eq!(resolve(&list, Target { id: 4, conn: 12 }), Err(TargetError::Gone));
        assert!(matches!(resolve(&list, Target { id: 3, conn: 9 }), Err(TargetError::Replaced(n)) if n == "Pichau (#3)"));
    }
}
