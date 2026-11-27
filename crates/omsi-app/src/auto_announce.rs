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
//!
//! Which of the three a map's city makes is the options' business (#4033): the arrival
//! recording at the stop, the "next stop is …" one as a script that failed to step is made
//! up for, and the same recording played once more on the way in, a distance before the stop
//! - the last of them for the cities that announce on approach instead of on leaving. A
//! distance two stops closer together than twice would reach announces halfway between them
//! (`announce::approach_metres`).

use omsi_sim::VehicleInstance;
use omsi_sim::timetable_run::PlayerDuty;
use omsi_sim::timetable_run::announce::{self, LayoutStatus, Policy, TwoRecordings};

/// The least time between two presses of a key (s): one press a frame at most, and the
/// scripts a moment to run their own step in between.
const MIN_GAP: f64 = 0.2;
/// The least time before the same entry is pressed for again (s): a press the script did not
/// take (electrics off, another IBIS mode) is tried again, but once a second rather than five
/// times - the key may be one the script uses for something else, and twenty tries are enough
/// to cover the stop.
const RETRY_GAP: f64 = 1.0;
/// How long the scripts have to step the IBIS on by themselves when a stop is left before the
/// game does it (s): a stock IBIS does it in the frame the stop index changes. The press comes
/// earlier - as the bus pulls out - where the bus reaches `departure`'s metres and speed first.
const SCRIPT_GRACE: f64 = omsi_sim::timetable_run::departure::GRACE;
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
    /// The stop of the trip whose approach announcement was played on the way in.
    approached: Option<usize>,
    /// The route whose start recording was played at the trip's first stop: once a route, as
    /// the driver presses it once (another route typed in at the stop gets its own).
    parked: Option<usize>,
    /// The press still to come (one at a time: the IBIS is a counter, and only one step can
    /// be taken at a time).
    want: Option<Want>,
    /// A stop was left: when, the entry the IBIS stood at then and where the bus left it. The
    /// scripts must step the counter on by themselves - as the bus pulls out of the stop
    /// (`departure::due`), or within [`SCRIPT_GRACE`] where it does not.
    watch: Option<(f64, f32, glam::DVec3)>,
    /// When a key was pressed last, and the entry the IBIS stood at then: a press that leaves
    /// the counter there is one the script did not take, and is given a second.
    pressed: Option<(f64, f32)>,
    /// The route whose depot file was last looked at, and its layout.
    layout: Option<(usize, Option<LayoutStatus>)>,
    /// The vehicle has no `IBIS_vor` at all (an ibox or an Atron unit announces on its own).
    /// Looked at again at every new trip, not once a session (#3281): a bus swapped in under a
    /// running game, or a script that mounts late, gets its chance.
    off: bool,
    /// The pairs of recordings of the route last looked at (0: one recording per stop), for the
    /// line the player is told once a trip (#3281).
    pairs: usize,
    /// The line the player is owed about this trip (#3281: the game stays quiet without saying
    /// why), taken by the caller once.
    note: Option<announce::QuietReason>,
    /// This trip's silence was already accounted for.
    noted: bool,
    /// The last line the player was given: the same reason again on the next trip (a map
    /// whose every depot file keeps one recording per stop) is not shown again.
    told: Option<announce::QuietReason>,
    /// The vehicle has no `IBIS_vor_stumm`: the counter can only be pressed on, not stepped
    /// over silently, so a press is only made where it lands on the recording wanted.
    no_skip: bool,
}

#[rustfmt::skip]
impl AutoAnnounce {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Advance into a new trip: look again for depot files and keys.
    pub(crate) fn new_trip(&mut self, trip_index: usize, at_stop: bool) {
        self.off = false;
        self.noted = false;
        self.trip = Some(trip_index);
        self.at_stop = Some(at_stop);
        self.announced = None;
        self.approached = None;
        self.parked = None;
        self.want = None;
        self.watch = None;
        self.layout = None;
    }

    /// One frame of the player's duty, after it was updated: `now` is the seconds of play
    /// (`SimClock::run_time`, which never wraps - a night line runs past midnight).
    pub(crate) fn step(
        &mut self,
        d: &PlayerDuty,
        bus: &mut VehicleInstance,
        now: f64,
        policy: Policy,
    ) {
        if self.off || !policy.any() {
            return;
        }
        if self.trip != Some(d.trip_index) {
            self.new_trip(d.trip_index, d.at_stop());
        }
        let at = d.at_stop();
        let left = self.at_stop.replace(at) == Some(true) && !at;
        let status = self.layout_status(bus);
        let layout = status.and_then(LayoutStatus::two);
        self.pairs = layout.map_or(0, |l| l.pairs);
        if !self.noted && status == Some(LayoutStatus::Single) && route(bus).is_some() {
            self.noted = true;
            let keys = announce::keys(
                bus.has_trigger("IBIS_vor"),
                bus.has_trigger("IBIS_vor_stumm"),
            );
            self.note = announce::quiet(keys, 0);
        }
        if at {
            self.park(bus, d.next_stop, layout, now);
            if policy.arrival {
                self.arrive(bus, d, layout, now);
            }
        } else if left {
            // (off: a script that does not step its counter on is left to its silence, as in
            // the cities that announce nothing on leaving)
            if policy.departure {
                self.left(bus, layout, now);
            } else {
                self.watch = None;
            }
        }
        if policy.approach && !at {
            self.approach(bus, d, policy, now);
        }
        self.press_step(bus, now);
        self.watch_step(bus, now);
    }

    fn layout_status(&mut self, bus: &VehicleInstance) -> Option<LayoutStatus> {
        let route = route(bus)?;
        self.layout_lookup(route, bus.host.hof.as_deref())
    }

    fn layout_lookup(
        &mut self,
        route: usize,
        hof: Option<&omsi_vehicle::Hof>,
    ) -> Option<LayoutStatus> {
        if let Some((known, layout)) = self.layout {
            if known == route {
                return layout;
            }
        }
        let layout = hof.and_then(|h| announce::layout_status(h, route));
        self.layout = Some((route, layout));
        if let Some(LayoutStatus::Two(l)) = layout {
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
        let Some(layout) = layout.filter(|l| l.pairs > 0) else {
            return;
        };
        let route = route(bus);
        if route.is_some() && self.parked == route {
            return;
        }
        if bus.var("IBIS_busstop").is_none() {
            self.off = true;
            self.note = Some(announce::QuietReason::NoBusstopCounter);
            return;
        }
        let Some(entry) = park_target(layout, bus.var("IBIS_busstop")) else {
            return;
        };
        if self.want.is_none() {
            self.want = Some(Want {
                entry,
                until: now + ARRIVAL_WAIT,
                at_stop: true,
                start: true,
                recordings: None,
            });
        }
    }

    /// The bus reached the stop it stands at: the stop's arrival recording.
    fn arrive(
        &mut self,
        bus: &VehicleInstance,
        d: &PlayerDuty,
        layout: Option<TwoRecordings>,
        now: f64,
    ) {
        // (the first stop is the one the bus pulls out of, and its listener has just boarded:
        // what is due there is the line's start recording, which `park` sees to)
        if d.next_stop == 0 || self.announced == Some(d.next_stop) {
            return;
        }
        // (a depot file of one recording per stop has no arrival half to add)
        if !layout.is_some_and(|l| l.pairs > 0) {
            return;
        }
        let Some(stop) = d
            .trips
            .get(d.trip_index)
            .and_then(|t| t.stops.get(d.next_stop))
        else {
            return;
        };
        if !stop.stops {
            self.announced = Some(d.next_stop);
            return;
        }
        let (Some(hof), Some(route)) = (bus.host.hof.clone(), route(bus)) else {
            return;
        };
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
        self.want = Some(Want {
            entry,
            until: now + ARRIVAL_WAIT,
            at_stop: true,
            start: false,
            recordings: None,
        });
        log::info!(
            "auto-announce: arrival at '{}', entry {entry} of route {route}",
            stop.name.trim()
        );
    }

    /// The bus is on its way to the stop it is due at and near enough for the city's approach
    /// announcement (#4033): that stop's own "next stop is …" recording, pressed as the
    /// arrival one is at the stop. The distance is the city's option, or half the way from the
    /// stop before where the two stand closer together than twice it.
    fn approach(&mut self, bus: &VehicleInstance, d: &PlayerDuty, policy: Policy, now: f64) {
        if d.next_stop == 0 || self.approached == Some(d.next_stop) {
            return;
        }
        let Some((left, gap)) = d.leg_distances() else { return };
        if left > announce::approach_metres(policy.approach_m, gap) {
            return;
        }
        let Some(stop) = d.trips.get(d.trip_index).and_then(|t| t.stops.get(d.next_stop)) else { return };
        if !stop.stops {
            self.approached = Some(d.next_stop);
            return;
        }
        let (Some(hof), Some(route)) = (bus.host.hof.clone(), route(bus)) else { return };
        let Some(entry) = announce::preview_entry(&hof, route, &stop.name, d.next_stop) else {
            // (a depot file that keeps one recording per stop has no "next stop is …" half to
            // play on the way in: the scripts announce there as they always did)
            self.approached = Some(d.next_stop);
            return;
        };
        self.approached = Some(d.next_stop);
        // (the bus is driving, so the press is not tied to standing at the stop)
        self.want = Some(Want { entry, until: now + ARRIVAL_WAIT, at_stop: false, start: false, recordings: None });
        log::info!("auto-announce: approaching '{}', entry {entry} of route {route}", stop.name.trim());
    }

    /// The bus left the stop: a stock script steps the counter on by itself here, and plays
    /// the next stop's "next stop is …" recording. Where it does not, the watchdog presses
    /// the key instead. A depot file of one recording per stop is left alone: the scripts
    /// have always announced there.
    fn left(&mut self, bus: &VehicleInstance, layout: Option<TwoRecordings>, now: f64) {
        if !layout.is_some_and(|l| l.pairs > 0) {
            return;
        }
        // (where the bus left it: how far it has pulled out is what the press waits for, #1289)
        self.watch = bus.var("IBIS_busstop").map(|entry| (now, entry, bus.position));
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
        if want
            .recordings
            .is_some_and(|before| bus.recordings() > before)
        {
            // (the last press was heard after all: its recording came after it)
            self.played(bus, want);
            return;
        }
        // (a press the script did not take leaves the counter where it stood: the next one is
        // a retry, and waits a second rather than a fifth; a press that stepped it on was
        // heard by the script, and the entries in between follow a fifth of a second apart)
        let retry = self
            .pressed
            .is_some_and(|(_, before)| bus.var("IBIS_busstop") == Some(before));
        let gap = if retry { RETRY_GAP } else { MIN_GAP };
        if self.pressed.is_some_and(|(last, _)| now - last < gap) {
            return;
        }
        let Some(at) = bus.var("IBIS_busstop") else {
            self.off = true;
            self.note = Some(announce::QuietReason::NoBusstopCounter);
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
            log::info!(
                "auto-announce: the IBIS stands at entry {at}, {target} expected here, left alone"
            );
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
            log::info!(
                "auto-announce: the IBIS cannot be stepped on silently, entry {target} left alone"
            );
            if want.start {
                // (given up for this route, not asked for again every frame)
                self.parked = route(bus);
            }
            self.want = None;
        } else {
            self.press(bus, "IBIS_vor_stumm", now);
        }
        // (an entry the press did reach needs no second one; one it did not is tried again)
        if bus
            .var("IBIS_busstop")
            .is_some_and(|v| (v - want.entry as f32).abs() < 0.5)
        {
            self.want = None;
        }
    }

    /// A stock IBIS steps its counter on when a stop is left. One that does not - the stop
    /// index the scripts read did not change, the electrics are off, another IBIS mode - would
    /// stay silent for the rest of the duty, so the game presses the key once.
    fn watch_step(&mut self, bus: &mut impl Ibis, now: f64) {
        let Some((since, entry, left_at)) = self.watch else {
            return;
        };
        if bus.var("IBIS_busstop") != Some(entry) {
            self.watch = None;
            return;
        }
        // (the bus pulling out of the stop announces then, not four seconds later; a bus that
        // creeps waits for the grace, #1289)
        let (past_m, kmh) = bus.past_stop(left_at);
        if !omsi_sim::timetable_run::departure::due(now - since, past_m, kmh) {
            return;
        }
        self.watch = None;
        if self.want.is_some() {
            return;
        }
        self.want = Some(Want {
            entry: entry as usize + 1,
            until: now + SCRIPT_GRACE,
            at_stop: false,
            start: false,
            recordings: None,
        });
        log::warn!("auto-announce: the scripts left the IBIS at entry {entry}, pressed it on");
    }

    /// The press for `want` was heard: not again.
    /// The one line the player is owed about this trip (#3281), once: why the game is not
    /// announcing, where it cannot (a bus with no IBIS key, a depot file of one recording per
    /// stop). None where it is announcing as asked.
    pub(crate) fn take_note(&mut self) -> Option<announce::QuietReason> {
        let note = self.note.take()?;
        if self.told == Some(note) {
            return None;
        }
        self.told = Some(note);
        Some(note)
    }

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
                self.note = announce::quiet(announce::Keys::Neither, self.pairs.max(1));
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
    /// How far the bus has moved from `from` (m) and how fast it goes (km/h): what the
    /// departure announcement is timed by (#1289). None where the bus has no body to measure
    /// (an ibox, a bus without a model): the grace decides then.
    fn past_stop(&self, _from: glam::DVec3) -> (Option<f32>, f32) {
        (None, 0.0)
    }
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

    fn past_stop(&self, from: glam::DVec3) -> (Option<f32>, f32) {
        let past = (self.position - from).truncate().length();
        (
            past.is_finite().then_some(past as f32),
            self.physics.velocity_kmh().abs(),
        )
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
#[path = "auto_announce_tests.rs"]
mod tests;
