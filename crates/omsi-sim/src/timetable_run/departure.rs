//! When the game may press the IBIS on after a stop is left (#1289): the departure
//! announcement of a script that has not stepped its own counter on.
//!
//! A stock script steps the counter on in the frame the bus leaves the stop, and plays the next
//! stop's recording itself. One that does not - a map whose script waits for the driver's key,
//! the electrics off, another IBIS mode - would leave the announcement unheard for the rest of
//! the duty, so the game presses the key once (see `omsi_app::auto_announce`).
//!
//! When it presses is what this module decides: as the bus pulls away rather than on a timer,
//! so the announcement comes with the bus leaving, as a modern transit system does it - a
//! distance past the stop **and** a speed, or the old grace where the bus creeps and neither is
//! ever reached (a stop-and-go queue would otherwise keep it silent, and a bus that never moves
//! again never announces at all).
//!
//! The script keeps its chance: nothing is pressed in the first [`FIRST_WAIT`] seconds after the
//! stop is left, which is where a script that steps in the same frame has already done it.

/// The least time after a stop is left before the metre trigger may fire (s): the frame or two
/// a script that steps its own counter needs.
pub const FIRST_WAIT: f64 = 0.6;

/// How long a script is given to step its own counter before the game presses for it anyway
/// (s): a bus that creeps out of the stop, or stands still past it, still announces.
pub const GRACE: f64 = 4.0;

/// How far past the stop the bus has to be for the departure announcement (m).
pub const PAST_M: f32 = 8.0;

/// How fast the bus has to be moving for it (km/h): a bus being pushed or rolling a metre does
/// not count.
pub const KMH: f32 = 8.0;

/// Whether the game is to press the IBIS on now: `waited` seconds after the stop was left, with
/// the bus `past_m` metres beyond it (None while its place is unknown) at `kmh`.
///
/// The metres and the speed are what makes it "as the bus pulls out"; [`GRACE`] is the fallback
/// for a bus that never reaches them.
pub fn due(waited: f64, past_m: Option<f32>, kmh: f32) -> bool {
    if !waited.is_finite() || waited < 0.0 {
        return false;
    }
    if waited >= GRACE {
        return true;
    }
    if waited < FIRST_WAIT {
        return false;
    }
    past_m.is_some_and(|m| m.is_finite() && m >= PAST_M) && kmh.is_finite() && kmh >= KMH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bus_pulling_out_is_announced_as_it_goes() {
        // (8 m out at 20 km/h: a second or so after leaving, not four)
        assert!(due(1.0, Some(8.0), 20.0));
        assert!(due(2.5, Some(30.0), 45.0));
        // (not yet: still at the stop, or crawling)
        assert!(!due(1.0, Some(7.9), 20.0), "not far enough");
        assert!(!due(1.0, Some(20.0), 7.9), "not fast enough");
        // (the script's own frame: nothing is pressed that early)
        assert!(!due(0.5, Some(20.0), 30.0));
        assert!(!due(0.0, Some(20.0), 30.0));
        // (its place unknown - an ibox, a bus without a body: the grace decides)
        assert!(!due(1.0, None, 30.0));
    }

    #[test]
    fn a_bus_that_creeps_or_stands_still_still_announces() {
        // (a queue out of the stop: never 8 m nor 8 km/h within the grace)
        assert!(!due(3.9, Some(2.0), 3.0));
        assert!(due(GRACE, Some(2.0), 3.0));
        // (a bus left standing past its stop with the electrics off: it announces once the
        // press is taken)
        assert!(due(30.0, None, 0.0));
    }

    #[test]
    fn nonsense_times_are_no_press() {
        assert!(!due(f64::NAN, Some(20.0), 30.0));
        assert!(!due(-1.0, Some(20.0), 30.0));
        // (a speed or a distance out of a broken physics step)
        assert!(!due(1.0, Some(f32::NAN), 30.0));
        assert!(!due(1.0, Some(20.0), f32::NAN));
    }
}
