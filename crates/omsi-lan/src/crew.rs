//! The conductor of a bus (#2859): the other player who sells the tickets and gives the change
//! at the cash desk while the driver drives.
//!
//! It is the driver's call, and the driver's bus is the one authority: the driver's game takes
//! `Crew::assign`, and it alone decides whether a switch another player asks for is one its
//! conductor may work (`may_serve`). Nothing of this changes how a ticket is sold - the desk's
//! switches are the bus's own (`ev_ticketsale_giveticket`, `ev_ticketsale_takemoney`, ...), so a
//! conductor's click goes through the same simulation a driver's does, and a transaction is
//! booked once because the switch is worked once.
//!
//! The rest of the session only needs to be told who serves whose desk, which is one line of a
//! command (`announce`, read back by `commands::Command::Crew`).

/// The players who serve a bus's cash desk: nobody, or one of the others.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Crew {
    conductor: Option<u32>,
}

impl Crew {
    /// Nobody serves our desk, as at the start of a session.
    pub fn new() -> Crew {
        Crew::default()
    }

    /// Who serves our bus's desk now.
    pub fn conductor(&self) -> Option<u32> {
        self.conductor
    }

    pub fn is_conductor(&self, id: u32) -> bool {
        self.conductor == Some(id)
    }

    /// The driver (`driver`: our own number in the session) takes `peer` as the conductor of
    /// our bus, or none for nobody. False: not ours to give - a player cannot make themselves
    /// or player 0 the conductor of their own bus.
    pub fn assign(&mut self, driver: u32, peer: Option<u32>) -> bool {
        if let Some(peer) = peer {
            if peer == 0 || peer == driver {
                return false;
            }
        }
        self.conductor = peer;
        true
    }

    /// The driver takes the role back. The player it was, if any.
    pub fn clear(&mut self) -> Option<u32> {
        self.conductor.take()
    }

    /// May player `from` work the cash desk's switch `ev` of our bus? Only the conductor we
    /// named, and only from inside the bus (`aboard_inside`: they ride in it, not beside it).
    pub fn may_serve_switch(&self, from: u32, aboard_inside: bool, ev: &str) -> bool {
        aboard_inside && self.is_conductor(from) && sale_trigger(ev)
    }

    /// May player `from` serve our cash desk at all, from where they are?
    pub fn may_serve(&self, from: u32, aboard_inside: bool) -> bool {
        aboard_inside && self.is_conductor(from)
    }
}

/// Is `name` one of the cash desk's switches - the ones a conductor works (the driver's own
/// controls are not in this list, nor are the passengers' request buttons).
pub fn sale_trigger(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    let n = n.strip_suffix("_off").unwrap_or(&n);
    n.starts_with("ev_ticketsale_") || n.starts_with("ticketprinter")
}

/// The line that tells the session who serves our bus's desk (`commands::Command::Crew`).
pub fn announce(conductor: Option<u32>) -> String {
    match conductor {
        Some(id) => format!("crew {id}"),
        None => "crew none".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_driver_takes_a_conductor_and_nobody_else_does() {
        let mut crew = Crew::new();
        assert_eq!(
            crew.conductor(),
            None,
            "nobody serves the desk at the start"
        );
        assert!(crew.assign(2, Some(5)));
        assert_eq!(crew.conductor(), Some(5));
        assert!(crew.is_conductor(5));
        assert!(!crew.is_conductor(2));
        // a bus of our own: we cannot be our own conductor, and player 0 is nobody
        assert!(!crew.assign(2, Some(2)));
        assert!(!crew.assign(2, Some(0)));
        assert_eq!(
            crew.conductor(),
            Some(5),
            "a refused assignment changes nothing"
        );
    }

    #[test]
    fn the_driver_can_take_the_role_back() {
        let mut crew = Crew::new();
        crew.assign(2, Some(5));
        assert_eq!(crew.clear(), Some(5));
        assert_eq!(crew.conductor(), None);
        assert_eq!(crew.clear(), None, "nothing to take back");
        assert!(
            crew.assign(2, None),
            "clearing through assign is allowed too"
        );
        assert_eq!(crew.conductor(), None);
    }

    #[test]
    fn only_the_named_conductor_serves_and_only_from_inside_the_bus() {
        let mut crew = Crew::new();
        crew.assign(2, Some(5));
        assert!(crew.may_serve(5, true));
        assert!(
            !crew.may_serve(5, false),
            "standing at the door is not serving the desk"
        );
        assert!(
            !crew.may_serve(6, true),
            "the driver's desk is not everybody's"
        );
        crew.clear();
        assert!(!crew.may_serve(5, true), "the role taken back ends it");
    }

    #[test]
    fn a_conductor_works_the_cash_desk_and_nothing_else() {
        let mut crew = Crew::new();
        crew.assign(2, Some(5));
        for ev in [
            "ev_ticketsale_giveticket",
            "ev_ticketsale_givemoney",
            "ev_ticketsale_takemoney",
            "ev_ticketsale_taketicket",
            "ticketprinter_ticket_selection",
            "TicketPrinter_Druckt",
        ] {
            assert!(sale_trigger(ev), "{ev} is the desk's");
            assert!(crew.may_serve_switch(5, true, ev));
        }
        for ev in [
            "bus_doorfront0",
            "handbrake",
            "engine_start",
            "int_haltewunsch",
            "lights_beam",
        ] {
            assert!(!sale_trigger(ev), "{ev} is not the conductor's");
            assert!(
                !crew.may_serve_switch(5, true, ev),
                "the conductor does not drive"
            );
        }
        // and not from outside the bus
        assert!(!crew.may_serve_switch(5, false, "ev_ticketsale_giveticket"));
        // nor after the driver took the role back
        crew.clear();
        assert!(!crew.may_serve_switch(5, true, "ev_ticketsale_giveticket"));
    }

    #[test]
    fn every_bus_driver_announces_its_own_crew() {
        use crate::commands::Command;
        // the driver's own command, not the host's: each player drives a bus of their own
        assert_eq!(Command::parse(3, "crew 2"), Some(Command::Crew(Some(2))));
        assert_eq!(
            Command::parse(3, "crew 3"),
            None,
            "a driver is not the conductor of their own bus"
        );
        assert_eq!(Command::parse(3, "crew 0"), None, "player 0 is nobody");
        assert_eq!(Command::parse(3, "crew x"), None);
        assert_eq!(
            Command::parse(3, "crew"),
            None,
            "there is always a number, or `none`"
        );
    }

    #[test]
    fn the_session_is_told_who_serves_the_desk() {
        assert_eq!(announce(Some(5)), "crew 5");
        assert_eq!(announce(None), "crew none");
        assert_eq!(
            crate::commands::Command::parse(2, &announce(Some(5))),
            Some(crate::commands::Command::Crew(Some(5)))
        );
        assert_eq!(
            crate::commands::Command::parse(2, &announce(None)),
            Some(crate::commands::Command::Crew(None))
        );
    }
}
