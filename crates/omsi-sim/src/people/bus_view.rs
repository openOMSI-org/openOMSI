//! What the people's step reads of the player's bus, as owned values made once a frame
//! before the step (`PeopleSim::player_bus_view`). The step does not hold the vehicle: it can
//! then run on another thread while the bus stays where the picture and the player's
//! controls are (see `crate::sim_thread`). Every value is read from the vehicle exactly as the
//! step read it, so a step on the view is the step on the vehicle.

use super::*;

/// The player's bus as the people see it this frame.
#[derive(Debug, Clone)]
pub struct PlayerBusView {
    pub position: DVec3,
    pub heading: f64,
    /// `VehicleInstance::body_rotation`.
    pub rot: Mat4,
    /// `physics.speed` (m/s) and `physics.velocity_kmh()`.
    pub speed: f32,
    pub speed_kmh: f32,
    /// `physics.a_trans`: the floor's acceleration the riders feel.
    pub a_trans: Vec3,
    /// The vehicle type's `[boundingbox]`.
    pub bounding_box: Option<[f32; 6]>,
    /// The saloon light (`interior_light`) and the cabin air (`CabinAir::of`).
    pub interior: f32,
    pub air: CabinAir,
    /// `GivenTicket`: the ticket the driver handed over (none: -1 or no variable).
    pub given_ticket: Option<f32>,
    /// The terminus its destination display shows: `None` when its scripts keep no
    /// destination index (the duty's terminus is taken then), `Some(None)` for none.
    pub terminus: Option<Option<String>>,
    /// Its timetable's current stop (`tt_busstop_index`: the stop's object and name) and
    /// `act_busstop`, the stop its scripts name.
    pub tt_stop: Option<(i64, Option<String>)>,
    pub act_busstop: String,
    /// The IBIS route it is set to (`IBIS_RouteIndex`, `IBIS_busstop`).
    pub route: Option<SetRoute>,
    /// What depends on the passenger cabin (none without one: nobody rides it).
    pub cabin: Option<CabinView>,
}

/// The parts of [`PlayerBusView`] that go with the bus's passenger cabin.
#[derive(Debug, Clone)]
pub struct CabinView {
    /// Its entries' and exits' doors open (`PeopleSim::doors_open`).
    pub entry_open: Vec<bool>,
    pub exit_open: Vec<bool>,
    /// Its rear sections (`part_frames`) and the places switched off (`places_off`).
    pub trailers: Vec<PartFrame>,
    pub places_off: Vec<bool>,
}

impl PeopleSim {
    /// The player's bus `v` as this frame's step reads it (with the cabin the people know for
    /// it, `set_cabin`).
    pub fn player_bus_view(&self, v: &VehicleInstance) -> PlayerBusView {
        let cabin = self.player_cabin.as_ref().map(|cabin| {
            let (entry_open, exit_open) = Self::doors_open(v, cabin.entries.len(), cabin.exits.len());
            CabinView { entry_open, exit_open, trailers: part_frames(v, cabin), places_off: places_off(v, cabin) }
        });
        // the destination shown: the row the display's scripts chose (target_index_int, as
        // Omsi.exe's people read it), else the depot file's row the IBIS was set to (the
        // game sets IBIS_TerminusIndex and IBIS_TerminusCode for a pick from the list and a
        // code typed) - for a script that keeps no row, or a code typed past the list's
        // length, where the bus stood "not in service" at a full stop and nobody got on.
        // See `shown_terminus_row`.
        let terminus = match (v.host.hof.as_ref(), v.var("target_index_int")) {
            (Some(hof), script) => {
                let reports_display = script.is_some();
                let code = v.var("IBIS_TerminusCode").filter(|c| c.is_finite() && *c >= 0.0).map(|c| c.round() as i32);
                let index = v.var("IBIS_TerminusIndex").filter(|i| i.is_finite() && *i >= 0.0).map(|i| i.round() as usize);
                let script = script.filter(|i| i.is_finite() && *i >= 0.0).map(|i| i.round() as usize);
                match shown_terminus_row(&hof.termini, code, index, script) {
                    Some(i) => Some(
                        hof.termini
                            .get(i)
                            .filter(|t| !t.all_exit && !t.texture_id.trim().is_empty())
                            .map(|t| t.texture_id.trim().to_string()),
                    ),
                    // Unresolved IBIS values alone do not establish an empty display.
                    None if !reports_display => None,
                    None => Some(None),
                }
            }
            (None, None) => None,
            (None, Some(_)) => Some(None),
        };
        let tt_stop = usize::try_from(v.host.tt_busstop_index).ok().and_then(|index| {
            v.host.tt_stop_ids.get(index).map(|id| (*id, v.host.tt_stops.get(index).map(|stop| stop.0.clone())))
        });
        // the IBIS route it is set to, where everybody gets off at the last stop
        let route = v.host.hof.as_ref().zip(v.var("IBIS_RouteIndex").filter(|r| r.is_finite() && *r >= 0.0)).and_then(|(hof, r)| {
            let at = v.var("IBIS_busstop").filter(|k| k.is_finite() && *k >= 0.0).map(|k| k.round() as usize);
            SetRoute::from_depot(hof, r.round() as usize, at)
        });
        PlayerBusView {
            position: v.position,
            heading: v.heading,
            rot: v.body_rotation(),
            speed: v.physics.speed,
            speed_kmh: v.physics.velocity_kmh(),
            a_trans: v.physics.a_trans,
            bounding_box: v.ty.def.bounding_box,
            interior: v.interior_light(),
            air: CabinAir::of(v),
            given_ticket: v.var("GivenTicket"),
            terminus,
            tt_stop,
            act_busstop: v.str_var("act_busstop").to_string(),
            route,
            cabin,
        }
    }
}

/// The depot file's terminus row a bus shows: the row the display's script keeps (`script`,
/// when it is a row of the list), else the row the IBIS was set to (`index`, when it is a
/// row of the code the IBIS holds), else the first row of that code.
pub(crate) fn shown_terminus_row(termini: &[omsi_vehicle::hof::Terminus], code: Option<i32>, index: Option<usize>, script: Option<usize>) -> Option<usize> {
    // Omsi.exe ties `target_index_int` to the vehicle's own terminus (0x7f64e8 -> its field
    // +0x7b4) and its people go by that: the display script's row first, where it is a row
    // of the depot file. Taking the IBIS's row before it (0.2.47) left the people of every
    // bus whose IBIS values were stale or at their defaults - the display set another way -
    // reading another destination than the one shown, and nobody got on (#2601, #2473).
    // The IBIS's row is for a script that keeps none, or one past the list.
    script
        .filter(|&i| i < termini.len())
        .or_else(|| index.filter(|&i| termini.get(i).is_some_and(|t| code.is_none_or(|c| t.code == c))))
        .or_else(|| code.and_then(|c| termini.iter().position(|t| t.code == c)))
        .or(script)
}
