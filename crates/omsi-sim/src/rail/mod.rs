//! Rail vehicles: what kind they are, what kind of track they run on, and how they move.
//!
//! OMSI 2 has one rail kind (`[path]` type 2) and one vehicle flag for everything that
//! runs on it. Trams, metro trains and railway trains differ in how they are driven and
//! how they move - a tram brakes at 1.3 m/s² in the street, a train at 0.9 and needs a
//! kilometre to stop; a metro takes its current from a third rail, a tram from a wire -
//! so the game has to know which is which. [`RailClass`] says it for a vehicle (from what
//! its file declares, or an explicit `[rail_class]`), [`TrackClass`] for a piece of track,
//! and [`dynamics`] moves a consist along its track by the forces on it.

pub mod brake;
pub mod chain;
pub mod body;
pub mod class;
pub mod dynamics;
pub mod track;

/// Whether the rail physics beyond the original's are on: the pantograph against the wire, the
/// body on its springs and the power curve of AI trams.
/// They change how every existing vehicle drives, so they belong to the setting "Realistic
/// road physics" (or `OMSI_REALISTIC_ROAD=1`), as the road model's tyres do.
pub fn realistic() -> bool {
    crate::rigid::realistic_default()
}

pub use body::{Body, BodyOut, BodyParams};
pub use class::{RailClass, TrackClass, TrackFacts};
pub use dynamics::{step, Conditions, Consist, Forces, Outcome};
pub use track::{classify, RailMapConfig, TrackInfo, TrackRule};
