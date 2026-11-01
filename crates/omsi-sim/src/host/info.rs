//! The callbacks that answer from what the game knows: the depot's routes, termini and bus
//! stops, the timetable, the ground, the passengers, the tickets, the arrivals and the radio.

use super::*;

impl VehicleHost {
    /// Any other provided callback (see [`VehicleHost::callback`]).
    pub(super) fn info_callback(&mut self, cb: Option<Cb>, stacks: &mut Stacks, state: &mut State) {
        match cb {
            // --- depot (HOF) callbacks of the IBIS: arguments come off the float stack, the
            // last pushed first
            Some(Cb::GetRouteIndex) => {
                let code = stacks.pop();
                let code_i = arg_i32(code);
                let idx = self
                    .hof
                    .as_ref()
                    .and_then(|h| {
                        h.info_trips.iter().position(|t| {
                            let c = t.code.trim();
                            omsi_cfg::parse_i32(c) == code_i
                                || (code - omsi_cfg::parse_f32(c)).abs() < 1e-3
                        })
                    })
                    .map(|i| i as f32)
                    .unwrap_or(-1.0);
                stacks.push(idx);
            }
            // An index of -1 (what the lookups answer for an unknown code) names no entry:
            // the strings are empty and the codes -1. Reading entry 0 instead put the first
            // terminus of the depot ("LEERFELD") where a mod's display looks for a custom
            // font or an optional sign.
            Some(Cb::GetRouteTerminusIndex) => {
                let route = arg_i32(stacks.pop());
                let idx = self.hof.as_ref().and_then(|h| {
                    let trip = h.info_trips.get(usize::try_from(route).ok()?)?;
                    let code = omsi_cfg::parse_i32(&trip.route);
                    h.termini.iter().position(|t| t.code == code)
                });
                stacks.push(idx.map(|i| i as f32).unwrap_or(-1.0));
            }
            Some(Cb::GetTerminusCode) => {
                let idx = arg_i32(stacks.pop());
                let code = self.terminus(idx).map(|t| t.code as f32).unwrap_or(-1.0);
                stacks.push(code);
            }
            Some(Cb::GetTerminusIndex) => {
                let code = arg_i32(stacks.pop());
                let idx = self.hof.as_ref().and_then(|h| h.termini.iter().position(|t| t.code == code)).map(|i| i as f32).unwrap_or(-1.0);
                stacks.push(idx);
            }
            Some(Cb::GetTerminusString) => {
                let n = arg_i32(stacks.pop());
                let idx = arg_i32(stacks.pop());
                let s = self.terminus(idx).and_then(|t| t.strings.get(usize::try_from(n).ok()?)).cloned().unwrap_or_default();
                stacks.push_str(s);
            }
            Some(Cb::GetBusstopCount) => {
                let route = arg_i32(stacks.pop());
                let n = self.hof.as_ref().and_then(|h| h.info_busstop_lists.get(usize::try_from(route).ok()?)).map(|l| l.len() as f32).unwrap_or(0.0);
                stacks.push(n);
            }
            Some(Cb::GetRouteBusstopIdent) => {
                let stop = arg_i32(stacks.pop());
                let route = arg_i32(stacks.pop());
                let s = self.hof.as_ref().and_then(|h| h.info_busstop_lists.get(usize::try_from(route).ok()?)).and_then(|l| l.get(usize::try_from(stop).ok()?)).cloned().unwrap_or_default();
                stacks.push_str(s);
            }
            Some(Cb::GetBusstopIndex) => {
                let ident = stacks.pop_str();
                let idx = self.hof.as_ref().and_then(|h| h.bus_stops.iter().position(|b| b.ident.trim().eq_ignore_ascii_case(ident.trim()))).map(|i| i as f32).unwrap_or(-1.0);
                stacks.push(idx);
            }
            Some(Cb::GetBusstopString) => {
                let n = arg_i32(stacks.pop());
                let idx = arg_i32(stacks.pop());
                let s = self.bus_stop(idx).and_then(|b| b.strings.get(usize::try_from(n).ok()?)).cloned().unwrap_or_default();
                stacks.push_str(s);
            }
            Some(Cb::GetDepotStringGlobal) => {
                let n = arg_i32(stacks.pop());
                let s = self.hof.as_ref().and_then(|h| h.global_strings.get(usize::try_from(n).ok()?)).cloned().unwrap_or_default();
                stacks.push_str(s);
            }
            // --- the bus radio (openOMSI): the station and its song as strings
            Some(Cb::GetRadioName) => stacks.push_str(self.radio_name.clone()),
            Some(Cb::GetRadioSong) => stacks.push_str(self.radio_song.clone()),
            // --- timetable callbacks
            Some(Cb::GetTTLineString) => stacks.push_str(self.tt_line.clone()),
            Some(Cb::GetTTDelay) => stacks.push(self.tt_delay),
            Some(Cb::GetTTBusstopCount) => stacks.push(self.tt_stops.len() as f32),
            // Without a timetable there is no current stop. Returning the first
            // stop (0) makes the Atron repeatedly detect an arrival and clear its
            // sales text after the holding brake has been on for three seconds.
            Some(Cb::GetTTBusstopIndex) => stacks.push(if self.tt_stops.is_empty() {
                -1.0
            } else {
                self.tt_busstop_index as f32
            }),
            Some(Cb::GetTTerminusIndex) | Some(Cb::GetTTTerminusIndex) => stacks.push(if self.tt_stops.is_empty() {
                -1.0
            } else {
                self.tt_terminus_index as f32
            }),
            // how high a point of the vehicle stands over the ground (the NL/NG ramp
            // measures the kerb this way before extending)
            Some(Cb::GetHeightAbovePoint) => {
                let z = stacks.pop();
                let y = stacks.pop();
                let x = stacks.pop();
                let h = self.ground_probe.as_ref().map(|f| f(x, y, z)).unwrap_or(0.0);
                stacks.push(h);
            }
            // passengers standing on one link of the cabin path network
            Some(Cb::GetHumanCountOnPathLink) => {
                let i = arg_i32(stacks.pop());
                stacks.push(self.humans_on_path_link.get(usize::try_from(i).unwrap_or(usize::MAX)).copied().unwrap_or(0) as f32);
            }
            Some(Cb::GetTTBusstopName) => {
                let i = arg_i32(stacks.pop());
                stacks.push_str(self.tt_stops.get(usize::try_from(i).unwrap_or(usize::MAX)).map(|s| s.0.clone()).unwrap_or_default());
            }
            Some(Cb::GetTTBusstopDep) => {
                let i = arg_i32(stacks.pop());
                stacks.push(self.tt_stops.get(usize::try_from(i).unwrap_or(usize::MAX)).map(|s| s.2).unwrap_or(0.0));
            }
            Some(Cb::GetTTBusstopArr) => {
                let i = arg_i32(stacks.pop());
                stacks.push(self.tt_stops.get(usize::try_from(i).unwrap_or(usize::MAX)).map(|s| s.1).unwrap_or(0.0));
            }
            Some(Cb::GiveChangeCoin) => {
                let i = arg_i32(stacks.pop());
                if i >= 0 {
                    self.change_coins.push((i as usize, 0.0));
                }
            }
            // OMSI's number-specific random value: the same for a fleet
            // number and characteristic every time (broken matrix pixels, wear effects)
            Some(Cb::NrSpecRandom) => {
                let n = arg_i32(stacks.pop());
                let number = self.number_var.and_then(|i| state.str_vars.get(i as usize)).cloned().unwrap_or_default();
                stacks.push(omsi_vehicle::sound::spec_random(&number, n));
            }
            // GetTicketName is what the ticket printers print: the ticket's display string,
            // as in OMSI, not the name its menu lists it by (#2483)
            Some(Cb::GetTicketName) => {
                let i = arg_i32(stacks.pop());
                stacks.push_str(self.tickets.as_ref().and_then(|t| t.tickets.get(usize::try_from(i).unwrap_or(usize::MAX))).map(|t| t.display_string.clone()).unwrap_or_default());
            }
            Some(Cb::GetTTicketName) => {
                let i = arg_i32(stacks.pop());
                stacks.push_str(self.tickets.as_ref().and_then(|t| t.tickets.get(usize::try_from(i).unwrap_or(usize::MAX))).map(|t| t.name.clone()).unwrap_or_default());
            }
            Some(Cb::GetTicketValue) => {
                let i = arg_i32(stacks.pop());
                stacks.push(self.tickets.as_ref().and_then(|t| t.tickets.get(usize::try_from(i).unwrap_or(usize::MAX))).map(|t| t.value).unwrap_or(0.0));
            }
            Some(Cb::GetHumanCountOnSeat) => {
                let i = arg_i32(stacks.pop());
                stacks.push(self.humans_on_seat.get(usize::try_from(i).unwrap_or(usize::MAX)).copied().unwrap_or(0) as f32);
            }
            // --- scenery objects (`program/callbacklist_scenobj.txt`): the n-th bus due at
            // the object's stop. Past the last one the line and terminus are empty (the stock
            // display leaves its line dark on an empty terminus) and the time is 0.
            Some(Cb::GetArrBusLine) => {
                let i = arg_i32(stacks.pop());
                stacks.push_str(self.arrival(i).map(|a| a.line.clone()).unwrap_or_default());
            }
            Some(Cb::GetArrBusTerminus) => {
                let i = arg_i32(stacks.pop());
                stacks.push_str(self.arrival(i).map(|a| a.terminus.clone()).unwrap_or_default());
            }
            Some(Cb::GetArrBusTimeDiff) => {
                let i = arg_i32(stacks.pop());
                stacks.push(self.arrival(i).map(|a| a.due).unwrap_or(0.0));
            }
            _ => unreachable!("not an information callback: {cb:?}"),
        }
    }
}
