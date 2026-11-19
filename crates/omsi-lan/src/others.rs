//! How much of another player's bus this game bothers with (#2538): the whole of it while it is
//! close by and the picture keeps up, less of it when it is far away or the frame rate is down.
//!
//! What can be left out of a bus another player drives is not its meshes: a bus's model keeps
//! its full detail here whatever the distance, since its lower levels of detail carry no
//! destination display, no lit window and no switched texture (`omsi_scene`'s `lods_misfit`
//! says why). What is left out instead is the work that is done for it every frame and that
//! nobody reads from far away: its driver figure, and its destination displays - the freetex
//! textures the session renders for it and sends about (see the app's `view_sync::lan`).
//!
//! The rule is the player's (a setting, off by default: the game then draws other players' buses
//! as it always did), and the frame rate has the last word: under `LOW_FPS` everything is drawn
//! less finely until the picture is back over `RECOVERED_FPS` - the same way the render scale
//! governor keeps itself from flapping between two sizes.

/// A bus farther than this (m) from the camera is drawn less finely, however fast the game runs.
pub const REDUCE_BEYOND: f64 = 120.0;

/// A frame rate (a second's frames) at or under which the game is struggling...
pub const LOW_FPS: f32 = 20.0;

/// ... and the one it has to be back over before the buses are drawn in full again.
pub const RECOVERED_FPS: f32 = 26.0;

/// How much of another player's bus is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    /// All of it: its driver at the wheel, its destination displays.
    Full,
    /// What is left of it from far away: the bus itself, without the figures and displays
    /// nobody reads from there.
    Reduced,
}

/// The frame rate as the buses' detail follows it: struggling under `LOW_FPS`, and staying so
/// until it is back over `RECOVERED_FPS` (a frame rate that sits between the two would
/// otherwise take the drivers and displays away and back, frame after frame).
#[derive(Debug, Default, Clone, Copy)]
pub struct Budget {
    struggling: bool,
}

impl Budget {
    pub fn new() -> Budget {
        Budget::default()
    }

    /// The detail this frame's frame rate calls for.
    pub fn step(&mut self, fps: f32) -> bool {
        if fps <= LOW_FPS {
            self.struggling = true;
        } else if fps > RECOVERED_FPS {
            self.struggling = false;
        }
        self.struggling
    }

    /// Whether the last `step` found the game struggling.
    pub fn struggling(&self) -> bool {
        self.struggling
    }
}

/// How finely to draw another player's bus `distance` metres away (the camera's), with the
/// player's setting `enabled`, `inside` it (we ride in it: then it is never thinned), and the
/// frame rate struggling (`Budget::step`).
pub fn detail(enabled: bool, distance: f64, inside: bool, struggling: bool) -> Detail {
    if !enabled || inside {
        return Detail::Full;
    }
    if struggling || distance > REDUCE_BEYOND {
        return Detail::Reduced;
    }
    Detail::Full
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_the_setting_leaves_other_players_buses_as_they_were() {
        for distance in [0.0, 100.0, 400.0, 5000.0] {
            for struggling in [false, true] {
                assert_eq!(detail(false, distance, false, struggling), Detail::Full);
            }
        }
    }

    #[test]
    fn a_bus_far_away_is_drawn_less_finely_and_one_close_by_in_full() {
        assert_eq!(detail(true, 0.0, false, false), Detail::Full);
        assert_eq!(detail(true, REDUCE_BEYOND, false, false), Detail::Full);
        assert_eq!(
            detail(true, REDUCE_BEYOND + 1.0, false, false),
            Detail::Reduced
        );
        assert_eq!(detail(true, 350.0, false, false), Detail::Reduced);
    }

    #[test]
    fn a_slow_game_draws_every_bus_less_finely_wherever_it_stands() {
        assert_eq!(detail(true, 10.0, false, true), Detail::Reduced);
        assert_eq!(detail(true, 0.0, false, true), Detail::Reduced);
    }

    #[test]
    fn the_bus_we_ride_in_is_never_thinned() {
        assert_eq!(detail(true, 0.0, true, true), Detail::Full);
        assert_eq!(detail(true, 900.0, true, true), Detail::Full);
    }

    #[test]
    fn the_frame_rate_hangs_on_under_its_threshold_and_recovers_over_it() {
        let mut b = Budget::new();
        assert!(
            !b.struggling(),
            "a game at its first frame is not struggling"
        );
        assert!(b.step(LOW_FPS), "at 20 fps it is");
        assert!(b.struggling());
        // (between the two thresholds nothing changes: no flapping, frame after frame)
        assert!(b.step(RECOVERED_FPS));
        assert!(b.step(21.0));
        assert!(b.step(25.9));
        assert!(!b.step(RECOVERED_FPS + 0.1), "over 26 fps it has recovered");
        assert!(!b.struggling());
        assert!(!b.step(30.0));
        assert!(b.step(12.0), "and it is struggling again under 20");
    }
}
