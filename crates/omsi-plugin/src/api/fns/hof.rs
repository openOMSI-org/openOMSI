//! Read-only snapshots of the player's loaded depot data; no file reads or script calls.
use super::{def, p};
use crate::api::ApiFn;

const NEW: &str = "0.3.40";

pub static FNS: &[ApiFn] = &[
    def!(
        "hof.get",
        "hof",
        [],
        "table or nil",
        "A snapshot of every parsed field of the player's loaded HOF: `path`, `name`, `service_trip`, `global_strings`, `string_count_terminus`, `string_count_busstop`, `termini`, `bus_stops`, `info_trips`, `info_busstop_lists`, `info_busstops`. Records include their zero-based `index`; Lua arrays (including strings) start at 1. Returns nil without a vehicle or HOF. No filesystem access or scripts are involved; retain the snapshot until the vehicle or HOF changes.",
        NEW,
        None,
        false,
        |c, a| c.io().hof_data()
    ),
    def!(
        "hof.terminus",
        "hof",
        [p("identifier", "string")],
        "table or nil",
        "The first terminus whose internal `identifier` exactly matches the supplied timetable terminus, using the same case-sensitive comparison as `GetTTTerminusIndex`. Returns `{index, code, identifier, terminus_stop, all_exit, strings}`; `index` is zero-based for `GetTerminusString`, `strings[1]` is getter field 0, and `terminus_stop` is nil when absent. Returns nil without a vehicle, HOF or match; does not change the duty or destination.",
        NEW,
        None,
        false,
        |c, a| c.io().hof_terminus(&a.str(0)?)
    ),
    def!(
        "hof.busstop",
        "hof",
        [p("identifier", "string")],
        "table or nil",
        "The first bus stop whose `identifier` exactly matches the supplied HOF stop identifier. Returns `{index, identifier, strings}`; `index` is zero-based for `GetBusstopString` and `strings[1]` is getter field 0. Returns nil without a vehicle, HOF or match.",
        NEW,
        None,
        false,
        |c, a| c.io().hof_busstop(&a.str(0)?)
    ),
];
