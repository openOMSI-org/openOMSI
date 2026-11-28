//! Validate passenger door commands before dispatching them to a bus.

use omsi_sim::people::{BusId, DoorRequest};

pub fn parse_request(text: &str, host: bool) -> Option<DoorRequest> {
    let mut words = text.split_whitespace();
    let kind = words.next()?;
    let id = words.next()?.parse::<u64>().ok()?;
    let bus = match (kind, id) {
        ("player", 0) => BusId::Player,
        ("ai", id) if host && id > 0 && id <= omsi_net::world::MAX_ID as u64 => BusId::Ai(id),
        _ => return None,
    };
    let exit = match words.next()? {
        "entry" => false,
        "exit" => true,
        _ => return None,
    };
    let index = words.next()?.parse::<usize>().ok()?;
    if index > u16::MAX as usize || words.next().is_some() {
        return None;
    }
    Some(DoorRequest { bus, exit, index })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_only_target_an_owners_player_bus_or_the_hosts_ai() {
        assert_eq!(
            parse_request("player 0 entry 2", false),
            Some(DoorRequest {
                bus: BusId::Player,
                exit: false,
                index: 2
            })
        );
        assert_eq!(
            parse_request("ai 7 exit 1", true),
            Some(DoorRequest {
                bus: BusId::Ai(7),
                exit: true,
                index: 1
            })
        );
        assert!(parse_request("ai 7 exit 1", false).is_none());
        for invalid in [
            "player 3 exit 1",
            "ai 0 entry 0",
            "ai 1099511627776 entry 0",
            "player 0 entry 65536",
            "player 0 entry 0 extra",
            "player 0 open 0",
        ] {
            assert!(parse_request(invalid, true).is_none());
        }
    }
}
