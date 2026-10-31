//! The automatic stop announcements: at each stop of a depot file that keeps two recordings
//! per stop, the game presses the IBIS's "next stop" key once, as the driver does. That is
//! what plays the "… arrived" recording: a stock IBIS steps its stop counter on by itself
//! only when a stop is left, so without the press only the "next stop is …" half of each
//! stop is ever heard. The game plays nothing itself - the key runs the vehicle's own script,
//! with its volume, its 3D sound and its terminus suffix.
//!
//! What the scripts do is left alone: a press is skipped where the counter already stands
//! where it should, a press counts as played once the script stepped the counter on or played
//! a recording for it (some play without stepping it on), one it did not take - the electrics
//! off, another IBIS mode, the script busy - is tried again, and a departure the scripts did
//! not step on by themselves is made up for once after a grace period. That last is the watchdog for the
//! maps whose depot files the scripts cannot follow, and it is armed only where there is a
//! second recording to miss.
//!
//! Standing at the trip's first stop, the game presses the key once, as a driver does before
//! pulling out (see [`AutoAnnounce::park`]): that plays the line's own start recording -
//! `welcome…` on the southern maps, `1041` on Beijing's - and leaves the counter on the entry
//! before the first "next stop is …" one, so that the scripts' own step on leaving plays the
//! first stop's recording. A start recording the scripts start late is not waited for: the
//! departure recording takes over, as it would in service.

use omsi_sim::timetable_run::announce::{self, TwoRecordings};
use omsi_sim::timetable_run::PlayerDuty;
use omsi_sim::VehicleInstance;

/// The least time between two presses of a key (s): one press a frame at most, and the
/// scripts a moment to run their own step in between.
const MIN_GAP: f64 = 0.2;
/// The least time before the same entry is pressed for again (s): a press the script did not
/// take (electrics off, another IBIS mode) is tried again, but once a second rather than five
/// times - the key may be one the script uses for something else, and twenty tries are enough
/// to cover the stop.
const RETRY_GAP: f64 = 1.0;
/// How long the scripts have to step the IBIS on by themselves when a stop is left before the
/// game does it (s): a stock IBIS does it in the frame the stop index changes.
const SCRIPT_GRACE: f64 = 4.0;
/// How long an arrival recording is worth pressing for (s): the bus may stand at the stop with
/// its electrics off, or another IBIS mode up, for a few seconds, and the press is tried again
/// every [`RETRY_GAP`] until then.
const ARRIVAL_WAIT: f64 = 20.0;
/// The most entries one press passes silently.
const MAX_SKIP: usize = 3;

/// The entry the IBIS is still to be pressed on, and until when.
#[derive(Clone, Copy)]
struct Want {
    entry: usize,
    until: f64,
    /// The bus has to still stand at its stop: an arrival recording is dropped when it drives
    /// on, since a stop the bus passes is not announced.
    at_stop: bool,
    /// The line's start recording (see [`AutoAnnounce::park`]): once played, its route is noted.
    start: bool,
    /// The recordings the vehicle had played when the key was last pressed for this entry
    /// without a sign of being heard: one played since (a script that plays a frame later, or
    /// on the key's release) means it was, and it is not pressed again.
    recordings: Option<u64>,
}

/// The announcements of one bus, a frame at a time.
#[derive(Default)]
pub(crate) struct AutoAnnounce {
    /// The trip of the last frame (0-based) and whether the bus stood at its stop. The first
    /// frame sets them; a bus spawned standing at a stop announces it as it would any other,
    /// since that is where it stands (and nothing is pressed where the counter is too far
    /// behind, see [`MAX_SKIP`]).
    trip: Option<usize>,
    at_stop: Option<bool>,
    /// The stop of the trip whose arrival recording was pressed for last.
    announced: Option<usize>,
    /// The route whose start recording was played at the trip's first stop: once a route, as
    /// the driver presses it once (another route typed in at the stop gets its own).
    parked: Option<usize>,
    /// The press still to come (one at a time: the IBIS is a counter, and only one step can
    /// be taken at a time).
    want: Option<Want>,
    /// A stop was left: when, and the entry the IBIS stood at then - after [`SCRIPT_GRACE`]
    /// the scripts must have stepped it on.
    watch: Option<(f64, f32)>,
    /// When a key was pressed last, and the entry the IBIS stood at then: a press that leaves
    /// the counter there is one the script did not take, and is given a second.
    pressed: Option<(f64, f32)>,
    /// The route whose depot file was last looked at, and its layout (None: one recording per
    /// stop): the same for every stop of a trip, and reading it compares stop names.
    layout: Option<(usize, Option<TwoRecordings>)>,
    /// The vehicle has no `IBIS_vor` at all (an ibox or an Atron unit announces on its own).
    off: bool,
    /// The vehicle has no `IBIS_vor_stumm`: the counter can only be pressed on, not stepped
    /// over silently, so a press is only made where it lands on the recording wanted.
    no_skip: bool,
}

impl AutoAnnounce {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// One frame of the player's duty, after it was updated: `now` is the seconds of play
    /// (`SimClock::run_time`, which never wraps - a night line runs past midnight).
    pub(crate) fn step(&mut self, d: &PlayerDuty, bus: &mut VehicleInstance, now: f64) {
        if self.off {
            return;
        }
        if self.trip != Some(d.trip_index) {
            // (a trip of its own: its first stop is due, and the counter stands where the
            // previous trip left it)
            self.trip = Some(d.trip_index);
            self.at_stop = Some(d.at_stop());
            self.announced = None;
            self.parked = None;
            self.want = None;
            self.watch = None;
        }
        let at = d.at_stop();
        let left = self.at_stop.replace(at) == Some(true) && !at;
        let layout = self.layout_of(bus);
        if at {
            // (both are dropped again by `press_step` while the other one is under way: the
            // counter takes one step a frame)
            self.park(bus, d.next_stop, layout, now);
            self.arrive(bus, d, layout, now);
        } else if left {
            self.left(bus, layout, now);
        }
        self.press_step(bus, now);
        self.watch_step(bus, now);
    }

    /// The depot file's layout of the route the IBIS shows, read once per route.
    fn layout_of(&mut self, bus: &VehicleInstance) -> Option<TwoRecordings> {
        let route = route(bus)?;
        if let Some((known, layout)) = self.layout {
            if known == route {
                return layout;
            }
        }
        let layout = bus.host.hof.as_ref().and_then(|hof| announce::layout(hof, route));
        self.layout = Some((route, layout));
        if let Some(l) = layout {
            log::info!(
                "auto-announce: route {route} keeps {} pairs of recordings, its first next stop is entry {}",
                l.pairs,
                l.first
            );
        }
        layout
    }

    /// While the bus stands at the trip's first stop, the key is pressed once, as a driver
    /// presses it before pulling out: that is the entry the line's start recording stands on
    /// (`welcome…`, `1041`), and the counter is left on the entry before the first
    /// "next stop is …" one. Where the scripts played it themselves the counter already
    /// stands there, and nothing is pressed - a start recording is never played twice. The
    /// scripts' own step on leaving - or the watchdog's press, where they do not step - then
    /// plays the first stop's recording, so a start recording the scripts never played is
    /// heard here rather than at the departure, where it would be cut off.
    fn park(&mut self, bus: &impl Ibis, next_stop: usize, layout: Option<TwoRecordings>, now: f64) {
        if next_stop != 0 {
            // (a duty sent back to its first stop plays it again)
            self.parked = None;
            return;
        }
        let Some(layout) = layout.filter(|l| l.pairs > 0) else { return };
        let route = route(bus);
        if route.is_some() && self.parked == route {
            return;
        }
        let Some(entry) = park_target(layout, bus.var("IBIS_busstop")) else { return };
        // (one silent step a frame, so that a script's own press in between is seen; once a
        // route, see `press_step`)
        if self.want.is_none() {
            self.want = Some(Want { entry, until: now + ARRIVAL_WAIT, at_stop: true, start: true, recordings: None });
        }
    }

    /// The bus reached the stop it stands at: the stop's arrival recording.
    fn arrive(&mut self, bus: &VehicleInstance, d: &PlayerDuty, layout: Option<TwoRecordings>, now: f64) {
        // (the first stop is the one the bus pulls out of, and its listener has just boarded:
        // what is due there is the line's start recording, which `park` sees to)
        if d.next_stop == 0 || self.announced == Some(d.next_stop) {
            return;
        }
        // (a depot file of one recording per stop has no arrival half to add)
        if !layout.is_some_and(|l| l.pairs > 0) {
            return;
        }
        let Some(stop) = d.trips.get(d.trip_index).and_then(|t| t.stops.get(d.next_stop)) else { return };
        let (Some(hof), Some(route)) = (bus.host.hof.clone(), route(bus)) else { return };
        let Some(entry) = announce::arrival_entry(&hof, route, &stop.name, d.next_stop) else {
            // (the depot file keeps no pair of recordings that this stop could be: said
            // once, so that a stop that stays silent can be looked up)
            self.announced = Some(d.next_stop);
            log::info!(
                "auto-announce: no arrival recording for '{}', stop {} of route {route}",
                stop.name.trim(),
                d.next_stop
            );
            return;
        };
        self.announced = Some(d.next_stop);
        self.want = Some(Want { entry, until: now + ARRIVAL_WAIT, at_stop: true, start: false, recordings: None });
        log::info!("auto-announce: arrival at '{}', entry {entry} of route {route}", stop.name.trim());
    }

    /// The bus left the stop: a stock script steps the counter on by itself here, and plays
    /// the next stop's "next stop is …" recording. Where it does not, the watchdog presses
    /// the key instead. A depot file of one recording per stop is left alone: the scripts
    /// have always announced there.
    fn left(&mut self, bus: &VehicleInstance, layout: Option<TwoRecordings>, now: f64) {
        if !layout.is_some_and(|l| l.pairs > 0) {
            return;
        }
        self.watch = bus.var("IBIS_busstop").map(|entry| (now, entry));
    }

    /// One step a frame towards the entry to play: silently while entries stand in between,
    /// and by the key that plays on the last one. Nothing is pressed twice at the same entry
    /// within [`MIN_GAP`], and a press the script did not take (electrics off, another IBIS
    /// mode) is tried again every [`RETRY_GAP`] until [`ARRIVAL_WAIT`] or until the bus drives
    /// on.
    fn press_step(&mut self, bus: &mut impl Ibis, now: f64) {
        let Some(mut want) = self.want else { return };
        if now > want.until || (want.at_stop && self.at_stop != Some(true)) {
            self.want = None;
            return;
        }
        if want.recordings.is_some_and(|before| bus.recordings() > before) {
            // (the last press was heard after all: its recording came after it)
            self.played(bus, want);
            return;
        }
        // (a press the script did not take leaves the counter where it stood: the next one is
        // a retry, and waits a second rather than a fifth; a press that stepped it on was
        // heard by the script, and the entries in between follow a fifth of a second apart)
        let retry = self.pressed.is_some_and(|(_, before)| bus.var("IBIS_busstop") == Some(before));
        let gap = if retry { RETRY_GAP } else { MIN_GAP };
        if self.pressed.is_some_and(|(last, _)| now - last < gap) {
            return;
        }
        let Some(at) = bus.var("IBIS_busstop") else {
            self.want = None;
            return;
        };
        let target = want.entry as f32;
        if target - at < 0.5 {
            // (the scripts played it themselves, or the counter is past it)
            self.want = None;
            return;
        }
        if target - at > MAX_SKIP as f32 + 1.0 {
            // (far behind - a duty taken over in the middle of its trip: stepping it on to
            // this stop would play the recordings of the stops that were skipped)
            log::info!("auto-announce: the IBIS stands at entry {at}, {target} expected here, left alone");
            self.want = None;
            return;
        }
        if target - at < 1.5 {
            // (the press itself steps on by one, and plays: heard where the counter moved or a
            // recording was played, whichever the script does)
            let before = bus.recordings();
            if self.press(bus, "IBIS_vor", now) || bus.recordings() > before {
                self.played(bus, want);
            } else {
                // (not heard: tried again after a second, unless its recording comes first)
                want.recordings = Some(before);
                self.want = Some(want);
            }
            return;
        } else if self.no_skip {
            // (no `IBIS_vor_stumm`: pressing on would play the entries in between)
            log::info!("auto-announce: the IBIS cannot be stepped on silently, entry {target} left alone");
            if want.start {
                // (given up for this route, not asked for again every frame)
                self.parked = route(bus);
            }
            self.want = None;
        } else {
            self.press(bus, "IBIS_vor_stumm", now);
        }
        // (an entry the press did reach needs no second one; one it did not is tried again)
        if bus.var("IBIS_busstop").is_some_and(|v| (v - want.entry as f32).abs() < 0.5) {
            self.want = None;
        }
    }

    /// A stock IBIS steps its counter on when a stop is left. One that does not - the stop
    /// index the scripts read did not change, the electrics are off, another IBIS mode - would
    /// stay silent for the rest of the duty, so the game presses the key once.
    fn watch_step(&mut self, bus: &mut impl Ibis, now: f64) {
        let Some((since, entry)) = self.watch else { return };
        if bus.var("IBIS_busstop") != Some(entry) {
            self.watch = None;
            return;
        }
        if now - since < SCRIPT_GRACE {
            return;
        }
        self.watch = None;
        if self.want.is_some() {
            return;
        }
        self.want = Some(Want { entry: entry as usize + 1, until: now + SCRIPT_GRACE, at_stop: false, start: false, recordings: None });
        log::warn!("auto-announce: the scripts left the IBIS at entry {entry}, pressed it on");
    }

    /// The press for `want` was heard: not again.
    fn played(&mut self, bus: &impl Ibis, want: Want) {
        if want.start {
            self.parked = route(bus);
        }
        self.want = None;
    }

    /// Press `name` and let it go again, as the game does for every key of the cab (a script
    /// that guards a repeat on the release stays usable), and tell whether the IBIS went on.
    fn press(&mut self, bus: &mut impl Ibis, name: &str, now: f64) -> bool {
        let before = bus.var("IBIS_busstop");
        if !bus.trigger(name) {
            log::info!("auto-announce: this bus has no `{name}`");
            if name.eq_ignore_ascii_case("IBIS_vor") {
                // (an ibox or an Atron unit: it announces on its own, and has no counter the
                // game could press on)
                log::info!("auto-announce: its announcements stay its own");
                self.off = true;
            } else {
                self.no_skip = true;
            }
            return false;
        }
        bus.trigger(&format!("{name}_off"));
        if let Some(before) = before {
            self.pressed = Some((now, before));
        }
        bus.var("IBIS_busstop") != before
    }
}

/// What the presses read and press of a bus: its variables and its keys.
trait Ibis {
    fn var(&self, name: &str) -> Option<f32>;
    fn trigger(&mut self, name: &str) -> bool;
    /// The recordings (`(T.F.…)` triggers) the scripts played so far.
    fn recordings(&self) -> u64;
}

impl Ibis for VehicleInstance {
    fn var(&self, name: &str) -> Option<f32> {
        VehicleInstance::var(self, name)
    }

    fn trigger(&mut self, name: &str) -> bool {
        VehicleInstance::trigger(self, name)
    }

    fn recordings(&self) -> u64 {
        self.host.file_triggers_fired
    }
}

/// The entry the counter is to be parked at, or None where it stands there or past it: the
/// one before the depot file's first "next stop is …" entry.
fn park_target(layout: TwoRecordings, at: Option<f32>) -> Option<usize> {
    let at = at?;
    let entry = layout.first.checked_sub(1)?;
    (entry as f32 - at > 0.5).then_some(entry)
}

/// The route whose stop list the IBIS shows: its `IBIS_RouteIndex`, or the ibox's own.
fn route(bus: &impl Ibis) -> Option<usize> {
    bus.var("IBIS_RouteIndex")
        .or_else(|| bus.var("ibox_routenindex"))
        .filter(|r| *r >= 0.0)
        .map(|r| r.round() as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_counter_is_parked_before_the_first_next_stop_recording() {
        // (the terminus' own recording at 0, the line's start recording at 1, the first
        // "next stop is ..." one at 2: the counter is parked at 1)
        let beijing = TwoRecordings { first: 2, pairs: 6 };
        assert_eq!(park_target(beijing, Some(0.0)), Some(1));
        // (a map that repeats the terminus' arrival before its pairs: parked at 2)
        let zhengzhou = TwoRecordings { first: 3, pairs: 5 };
        assert_eq!(park_target(zhengzhou, Some(0.0)), Some(2));
        // (already there, or past it: nothing to do)
        assert_eq!(park_target(beijing, Some(1.0)), None);
        assert_eq!(park_target(beijing, Some(2.0)), None);
        // (no counter to read: nothing to do)
        assert_eq!(park_target(beijing, None), None);
    }

    /// An IBIS whose `IBIS_vor` plays (counted) and steps the counter on, or not, with the
    /// electrics on; with them off, or `deaf` (another IBIS mode, the script busy), a press
    /// does nothing. A `late` one plays at the next frame.
    struct FakeIbis {
        busstop: f32,
        steps: bool,
        plays: usize,
        power: bool,
        deaf: bool,
        late: bool,
        silent_key: bool,
        pending: usize,
        route: f32,
    }

    impl FakeIbis {
        fn new(steps: bool) -> Self {
            FakeIbis { busstop: 0.0, steps, plays: 0, power: true, deaf: false, late: false, silent_key: true, pending: 0, route: 57.0 }
        }

        /// A frame of the scripts: what a `late` IBIS was pressed for plays now.
        fn frame(&mut self) {
            self.plays += std::mem::take(&mut self.pending);
        }
    }

    impl Ibis for FakeIbis {
        fn var(&self, name: &str) -> Option<f32> {
            match name {
                "IBIS_busstop" => Some(self.busstop),
                "IBIS_RouteIndex" => Some(self.route),
                "elec_busbar_main" => Some(if self.power { 1.0 } else { 0.0 }),
                _ => None,
            }
        }

        fn trigger(&mut self, name: &str) -> bool {
            match name {
                "IBIS_vor" => {
                    if self.power && !self.deaf {
                        if self.late {
                            self.pending += 1;
                        } else {
                            self.plays += 1;
                        }
                        if self.steps {
                            self.busstop += 1.0;
                        }
                    }
                    true
                }
                "IBIS_vor_stumm" if self.silent_key => {
                    if self.power {
                        self.busstop += 1.0;
                    }
                    true
                }
                _ => name.ends_with("_off"),
            }
        }

        fn recordings(&self) -> u64 {
            self.plays as u64
        }
    }

    const LAYOUT: Option<TwoRecordings> = Some(TwoRecordings { first: 2, pairs: 18 });

    /// `secs` at a stop at 60 frames a second, as `step` runs it there, from `t0`; `bus`
    /// may be changed at each frame.
    fn stand(a: &mut AutoAnnounce, bus: &mut FakeIbis, next_stop: usize, t0: f64, secs: f64, at: &mut dyn FnMut(f64, &mut FakeIbis)) {
        a.at_stop = Some(true);
        for f in 0..(secs * 60.0) as usize {
            let now = t0 + f as f64 / 60.0;
            at(now, bus);
            bus.frame();
            a.park(bus, next_stop, LAYOUT, now);
            a.press_step(bus, now);
        }
    }

    #[test]
    fn the_start_recording_plays_once_even_where_the_counter_stays() {
        // (the O560 of #3282: its script plays `welcome…` and leaves the counter at 0 - it
        // was pressed again every second, 63 times in a minute)
        let mut keeps = FakeIbis::new(false);
        stand(&mut AutoAnnounce::new(), &mut keeps, 0, 0.0, 60.0, &mut |_, _| {});
        assert_eq!(keeps.plays, 1);
        // (a stock script steps on to the start recording's entry: once, and parked there)
        let mut steps = FakeIbis::new(true);
        stand(&mut AutoAnnounce::new(), &mut steps, 0, 0.0, 60.0, &mut |_, _| {});
        assert_eq!((steps.plays, steps.busstop), (1, 1.0));
    }

    #[test]
    fn the_start_recording_waits_for_the_electrics() {
        // (the driver switches on 30 s after the duty starts: the start recording is played
        // then, once - not lost because the first presses went unheard)
        let mut bus = FakeIbis::new(false);
        bus.power = false;
        stand(&mut AutoAnnounce::new(), &mut bus, 0, 0.0, 60.0, &mut |now, b| b.power = now >= 30.0);
        assert_eq!(bus.plays, 1);
    }

    #[test]
    fn another_route_or_the_first_stop_again_plays_the_start_recording_again() {
        let mut a = AutoAnnounce::new();
        let mut bus = FakeIbis::new(false);
        stand(&mut a, &mut bus, 0, 0.0, 10.0, &mut |_, _| {});
        // (another route typed into the IBIS at the first stop)
        stand(&mut a, &mut bus, 0, 10.0, 10.0, &mut |_, b| b.route = 58.0);
        assert_eq!(bus.plays, 2);
        // (the duty sent on and back to its first stop on the same trip)
        stand(&mut a, &mut bus, 1, 20.0, 1.0, &mut |_, _| {});
        stand(&mut a, &mut bus, 0, 21.0, 10.0, &mut |_, _| {});
        assert_eq!(bus.plays, 3);
    }

    /// An arrival press at entry 5 with the IBIS at 4, for `secs`: the electrics come on at
    /// `power_at`, the IBIS takes presses from `heard_at`.
    fn arrival(bus: &mut FakeIbis, secs: f64, power_at: f64, heard_at: f64) {
        let mut a = AutoAnnounce::new();
        a.at_stop = Some(true);
        a.want = Some(Want { entry: 5, until: ARRIVAL_WAIT, at_stop: true, start: false, recordings: None });
        bus.busstop = 4.0;
        for f in 0..(secs * 60.0) as usize {
            let now = f as f64 / 60.0;
            bus.power = now >= power_at;
            bus.deaf = now < heard_at;
            bus.frame();
            a.press_step(bus, now);
        }
    }

    #[test]
    fn an_arrival_is_played_once_and_waits_for_the_electrics() {
        // (a script that plays without stepping on: once, not every second for 20 s)
        let mut keeps = FakeIbis::new(false);
        arrival(&mut keeps, 10.0, 0.0, 0.0);
        assert_eq!(keeps.plays, 1);
        // (electrics off for the first 3 s: tried again until they are on, then played once)
        let mut late = FakeIbis::new(true);
        arrival(&mut late, 10.0, 3.0, 0.0);
        assert_eq!((late.plays, late.busstop), (1, 5.0));
    }

    #[test]
    fn an_arrival_the_ibis_did_not_take_is_pressed_again() {
        // (electrics on, but the IBIS in another mode for the first 3 s: the press it did not
        // take is not counted as played, and the stop is announced once it takes one)
        let mut steps = FakeIbis::new(true);
        arrival(&mut steps, 10.0, 0.0, 3.0);
        assert_eq!((steps.plays, steps.busstop), (1, 5.0));
        let mut keeps = FakeIbis::new(false);
        arrival(&mut keeps, 10.0, 0.0, 3.0);
        assert_eq!(keeps.plays, 1);
    }

    #[test]
    fn a_start_recording_out_of_reach_is_given_up_once() {
        // (no `IBIS_vor_stumm` and the start recording three entries on, the KPP2 IBIS of the
        // Urbino III: left alone, and not asked for again every frame)
        let mut a = AutoAnnounce::new();
        let mut bus = FakeIbis::new(false);
        bus.silent_key = false;
        a.at_stop = Some(true);
        let far = Some(TwoRecordings { first: 4, pairs: 3 });
        for f in 0..600 {
            let now = f as f64 / 60.0;
            a.park(&bus, 0, far, now);
            a.press_step(&mut bus, now);
        }
        assert_eq!((bus.plays, bus.busstop, a.parked, a.want.is_none()), (0, 0.0, Some(57), true));
    }

    #[test]
    fn a_recording_played_a_frame_late_is_not_pressed_for_again() {
        // (a script that plays at the next frame and keeps its counter: once, not again a
        // second later)
        let mut bus = FakeIbis::new(false);
        bus.late = true;
        arrival(&mut bus, 10.0, 0.0, 0.0);
        assert_eq!(bus.plays, 1);
        let mut bus = FakeIbis::new(false);
        bus.late = true;
        stand(&mut AutoAnnounce::new(), &mut bus, 0, 0.0, 60.0, &mut |_, _| {});
        assert_eq!(bus.plays, 1);
    }
}
