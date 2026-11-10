//! Read the same game-owned data used by M.V. getters, without executing a script or
//! touching its stacks, callback dispatch, registers or variables.
use omsi_plugin::data::{DataQuery as Q, QueryValue as V};
use omsi_sim::VehicleInstance;

pub(crate) fn query(v: &mut VehicleInstance, q: Q, args: &[V]) -> Result<Vec<V>, String> {
    let number = v.str_var("number");
    read(&mut v.host, &number, q, args)
}

pub(crate) fn read(
    h: &mut omsi_sim::VehicleHost,
    fleet_number: &str,
    q: Q,
    args: &[V],
) -> Result<Vec<V>, String> {
    q.validate(args)?;
    let num = |i: usize| match &args[i] {
        V::Number(n) => *n,
        _ => unreachable!(),
    };
    let idx = |i: usize| usize::try_from(num(i).round_ties_even() as i32).unwrap_or(usize::MAX);
    let text = |i: usize| match &args[i] {
        V::String(s) => s.as_str(),
        _ => unreachable!(),
    };
    let hof = h.hof.as_deref();
    let number = |n: f32| Ok(vec![V::Number(n)]);
    let string = |s: String| Ok(vec![V::String(s)]);
    let index = |i: Option<usize>| number(i.map_or(-1.0, |i| i as f32));
    match q {
        Q::TextLength => number(
            h.fonts
                .entries
                .get(idx(1))
                .and_then(|f| f.1.as_ref())
                .map_or(0.0, |a| a.font.text_width(text(0)) as f32),
        ),
        Q::StReadPixel => Ok(h
            .script_textures
            .get(idx(0))
            .map_or([0; 4], |t| {
                t.get(
                    num(1).round_ties_even() as i32,
                    num(2).round_ties_even() as i32,
                )
            })
            .into_iter()
            .map(|n| V::Number(n as f32))
            .collect()),
        Q::StGetR | Q::StGetG | Q::StGetB | Q::StGetA => {
            let ch = match q {
                Q::StGetR => 0,
                Q::StGetG => 1,
                Q::StGetB => 2,
                _ => 3,
            };
            number(
                h.script_textures
                    .get(idx(0))
                    .map_or(0.0, |t| t.color[ch] as f32),
            )
        }
        Q::GetRouteIndex => index(hof.and_then(|h| {
            h.info_trips.iter().position(|t| {
                omsi_cfg::parse_i32(t.code.trim()) == num(0).round_ties_even() as i32
                    || (num(0) - omsi_cfg::parse_f32(t.code.trim())).abs() < 1e-3
            })
        })),
        Q::GetRouteTerminusIndex => index(hof.and_then(|h| {
            h.info_trips.get(idx(0)).and_then(|t| {
                h.termini
                    .iter()
                    .position(|x| x.code == omsi_cfg::parse_i32(&t.route))
            })
        })),
        Q::GetTerminusCode => number(
            hof.and_then(|h| h.termini.get(idx(0)))
                .map_or(-1.0, |t| t.code as f32),
        ),
        Q::GetTerminusIndex => index(hof.and_then(|h| {
            h.termini
                .iter()
                .position(|t| t.code == num(0).round_ties_even() as i32)
        })),
        Q::GetTerminusString => string(
            hof.and_then(|h| h.termini.get(idx(0)))
                .and_then(|t| t.strings.get(idx(1)))
                .cloned()
                .unwrap_or_default(),
        ),
        Q::GetBusstopCount => number(
            hof.and_then(|h| h.info_busstop_lists.get(idx(0)))
                .map_or(0.0, |s| s.len() as f32),
        ),
        Q::GetRouteBusstopIdent => string(
            hof.and_then(|h| h.info_busstop_lists.get(idx(0)))
                .and_then(|s| s.get(idx(1)))
                .cloned()
                .unwrap_or_default(),
        ),
        Q::GetBusstopIndex => index(hof.and_then(|h| {
            h.bus_stops
                .iter()
                .position(|s| s.ident.trim().eq_ignore_ascii_case(text(0).trim()))
        })),
        Q::GetBusstopString => string(
            hof.and_then(|h| h.bus_stops.get(idx(0)))
                .and_then(|s| s.strings.get(idx(1)))
                .cloned()
                .unwrap_or_default(),
        ),
        Q::GetDepotStringGlobal => string(
            hof.and_then(|h| h.global_strings.get(idx(0)))
                .cloned()
                .unwrap_or_default(),
        ),
        Q::GetTTLineString => string(h.tt_line.clone()),
        Q::GetTTDelay => number(h.tt_delay),
        Q::GetTTBusstopCount => number(h.tt_stops.len() as f32),
        Q::GetTTBusstopIndex => number(if h.tt_stops.is_empty() {
            -1.0
        } else {
            h.tt_busstop_index as f32
        }),
        Q::GetTTerminusIndex | Q::GetTTTerminusIndex => number(if h.tt_stops.is_empty() {
            -1.0
        } else {
            h.tt_terminus_index as f32
        }),
        Q::GetTTBusstopName => string(
            h.tt_stops
                .get(idx(0))
                .map(|s| s.0.clone())
                .unwrap_or_default(),
        ),
        Q::GetTTBusstopArr => number(h.tt_stops.get(idx(0)).map_or(0.0, |s| s.1)),
        Q::GetTTBusstopDep => number(h.tt_stops.get(idx(0)).map_or(0.0, |s| s.2)),
        Q::GetHeightAbovePoint => number(
            h.ground_probe
                .as_ref()
                .map_or(0.0, |f| f(num(0), num(1), num(2))),
        ),
        Q::GetHumanCountOnPathLink => {
            number(h.humans_on_path_link.get(idx(0)).copied().unwrap_or(0) as f32)
        }
        Q::GetHumanCountOnSeat => number(h.humans_on_seat.get(idx(0)).copied().unwrap_or(0) as f32),
        Q::NrSpecRandom => number(omsi_vehicle::sound::spec_random(
            fleet_number,
            num(0).round_ties_even() as i32,
        )),
        Q::GetTicketName => string(
            h.tickets
                .as_ref()
                .and_then(|t| t.tickets.get(idx(0)))
                .map(|t| t.display_string.clone())
                .unwrap_or_default(),
        ),
        Q::GetTTicketName => string(
            h.tickets
                .as_ref()
                .and_then(|t| t.tickets.get(idx(0)))
                .map(|t| t.name.clone())
                .unwrap_or_default(),
        ),
        Q::GetTicketValue => number(
            h.tickets
                .as_ref()
                .and_then(|t| t.tickets.get(idx(0)))
                .map_or(0.0, |t| t.value),
        ),
        Q::GetArrBusLine => string(
            h.arrivals
                .get(idx(0))
                .map(|a| a.line.clone())
                .unwrap_or_default(),
        ),
        Q::GetArrBusTerminus => string(
            h.arrivals
                .get(idx(0))
                .map(|a| a.terminus.clone())
                .unwrap_or_default(),
        ),
        Q::GetArrBusTimeDiff => number(h.arrivals.get(idx(0)).map_or(0.0, |a| a.due)),
        Q::GetRadioName => string(h.radio_name.clone()),
        Q::GetRadioSong => string(h.radio_song.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_plugin::data::{DataQuery, QueryType};

    #[test]
    fn all_information_callbacks_have_direct_plugin_reads() {
        for name in omsi_sim::host::PROVIDED_CALLBACKS {
            assert!(
                DataQuery::from_name(name).is_some()
                    || [
                        "stnewtex",
                        "stlock",
                        "stunlock",
                        "stfilter",
                        "stsetcolor",
                        "stdrawpixel",
                        "stdrawrect",
                        "sttextout",
                        "stcopycolor",
                        "stloadtex",
                        "givechangecoin",
                        "getfontindex"
                    ]
                    .contains(name),
                "missing getter {name}"
            );
        }
    }

    #[test]
    fn direct_reads_match_script_results_without_executing_the_bus() {
        use omsi_script::{Host, NameId, Stacks};
        let mut v = crate::schedule::tests::script_test_vehicle(
            "{frame}\n99 (S.L.sentinel)\n{end}\n",
            "sentinel\n",
            "number\n",
        );
        v.state.str_vars[v.host.number_var.unwrap() as usize] = "1234".into();
        v.host.tt_stops = vec![("Night stop".into(), 86460.0, 86490.0)];
        v.host.tt_line = "200E".into();
        v.host.hof = Some(std::sync::Arc::new(omsi_vehicle::Hof {
            termini: vec![omsi_vehicle::hof::Terminus {
                code: 0,
                strings: vec!["Airport".into()],
                ..Default::default()
            }],
            bus_stops: vec![omsi_vehicle::hof::BusStop {
                ident: String::new(),
                strings: vec!["Stop display".into()],
            }],
            info_trips: vec![omsi_vehicle::hof::InfoTrip {
                code: "0".into(),
                route: "0".into(),
                ..Default::default()
            }],
            info_busstop_lists: vec![vec!["Stop ident".into()]],
            global_strings: vec!["Depot".into()],
            ..Default::default()
        }));
        v.host.radio_name = "Station".into();
        v.host.radio_song = "Song".into();
        v.host.humans_on_seat = vec![2];
        v.host.humans_on_path_link = vec![3];
        v.host.ground_probe = Some(std::sync::Arc::new(|x, y, z| x + y + z + 4.0));
        for &q in DataQuery::ALL {
            if q == Q::StReadPixel {
                continue;
            }
            let args: Vec<_> = q
                .args()
                .iter()
                .map(|(_, ty)| match ty {
                    QueryType::Number => V::Number(0.0),
                    QueryType::String => V::String(String::new()),
                    _ => unreachable!(),
                })
                .collect();
            let actual = query(&mut v, q, &args).unwrap();
            let mut stack = Stacks::default();
            for arg in &args {
                match arg {
                    V::Number(n) => stack.push(*n),
                    V::String(s) => stack.push_str(s.clone()),
                }
            }
            v.host
                .callback(q.name(), NameId::MAX, &mut stack, &mut v.state);
            let expected = match q.result() {
                QueryType::String => V::String(stack.pop_str()),
                _ => V::Number(stack.pop()),
            };
            assert_eq!(actual, vec![expected], "{}", q.name());
        }
        assert_eq!(v.var("sentinel"), Some(0.0));
        assert_eq!(
            query(&mut v, Q::GetTTBusstopArr, &[V::Number(0.0)]).unwrap(),
            vec![V::Number(86460.0)]
        );
        assert_eq!(
            query(&mut v, Q::GetTTBusstopArr, &[V::Number(-1.0)]).unwrap(),
            vec![V::Number(0.0)]
        );
    }

    #[test]
    fn scenery_arrivals_are_read_without_a_vehicle_or_script() {
        let mut host = omsi_sim::VehicleHost::default();
        host.arrivals.push(omsi_sim::host::Arrival {
            line: "N1".into(),
            terminus: "Airport".into(),
            due: 75.0,
        });
        assert_eq!(
            read(&mut host, "", Q::GetArrBusLine, &[V::Number(0.0)]).unwrap(),
            vec![V::String("N1".into())]
        );
        assert_eq!(
            read(&mut host, "", Q::GetArrBusTimeDiff, &[V::Number(0.0)]).unwrap(),
            vec![V::Number(75.0)]
        );
        assert_eq!(
            read(&mut host, "", Q::GetArrBusLine, &[V::Number(-1.0)]).unwrap(),
            vec![V::String(String::new())]
        );
        assert!(host.unknown_callbacks().is_empty());
    }

    #[test]
    fn pixel_read_keeps_draw_colour_and_variables() {
        let mut v = crate::schedule::tests::script_test_vehicle("{frame}\n{end}\n", "", "");
        let mut t = omsi_sim::scripttex::ScriptTexture::new(2, 2);
        t.put(1, 0, [10, 20, 30, 40]);
        t.color = [1, 2, 3, 4];
        v.host.script_textures.push(t);
        assert_eq!(
            query(
                &mut v,
                Q::StReadPixel,
                &[V::Number(0.0), V::Number(1.0), V::Number(0.0)]
            )
            .unwrap(),
            [10, 20, 30, 40].map(|n| V::Number(n as f32))
        );
        assert_eq!(v.host.script_textures[0].color, [1, 2, 3, 4]);
        assert!(query(&mut v, Q::GetTTDelay, &[V::Number(0.0)]).is_err());
    }
}
