//! The simulation's own thread, pipelined with the picture (the game's `sim_thread` setting,
//! on by default; `OMSI_SIM_THREAD=0` / `=1` over it - see `set_enabled`).
//!
//! The window's frame runs its stages one after the other on the main thread (`WINDOW_FRAME`;
//! the simulation's once per tick of the fixed step, see `tick`), so a frame costs the
//! simulation plus the picture. The main thread has to keep the window,
//! the input and the surface (macOS and iOS want them there); the simulation does not. With
//! the switch on, a step of the simulation runs on the thread here while the main thread draws
//! and presents the picture of the frame, and it is back before anything else of the frame
//! looks at it.
//!
//! What goes there so far is the AI traffic's fixed step (`TrafficSim::advance`): the traffic
//! stage does everything else of the traffic as before (its inputs, the population, the
//! timetable, the cars taken off, the signals, the sound, the view sync, all on the traffic as
//! the last step left it) and leaves the step itself, with the frame's time and the player's
//! box as the stage saw them, to `Pipeline::start` right before the picture, one owed step per
//! tick of the frame; they come back at `Pipeline::join`, right after the picture is presented
//! (before the frame is counted, a session finished or saved). The cars are one frame of the
//! traffic later than the rest of the frame, the picture shows them as they were synced, and
//! the simulated time is the same (the same ticks, each its own step, in their order). The player's own vehicle and its
//! input stay on the main thread in their stage: driving has no frame more of latency.
//!
//! The people's step goes along tick by tick: each tick's traffic step, then that tick's
//! people's step (they read the traffic as it left it, as the humans stage read it after the
//! traffic stage), from a view of the player's bus as that tick left it (`PlayerBusView`).
//!
//! The scenery objects' scripts (`SceneryState::step`, once a frame) can go along in the same
//! job, after the ticks (they read its lights, as the scripted stage read them after the traffic
//! stage): the scripted stage then applies the commands of the step made during the last
//! picture and owes the next one, its inputs owned (`Pipeline::owe_scenery`). Their
//! commands are one frame later; the scripts' results are the same.
//!
//! The offscreen run never reads the switch: its step is the same code, inline. The window's
//! side (where the stages owe the step, where it starts and joins) is `omsi-app`'s
//! `sim_thread`.

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::mpsc;
use std::time::Instant;

use crate::ai_traffic::model::PlayerBox;
use crate::ai_traffic::TrafficSim;
use crate::people::{PeopleSim, PlayerBusView};
use crate::scenery::SceneryState;
use crate::world_data::WorldData;
use std::sync::Arc;

type Job = Box<dyn FnOnce() + Send>;

/// One persistent thread ("sim") the simulation's steps run on, one at a time.
pub struct SimThread {
    jobs: Option<mpsc::SyncSender<Job>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

/// What a step's job sends back: the state, how long the step took, and its panic, if it
/// panicked (raised again on the main thread at `join`, as it would have been inline).
type Outcome<T> = (T, f64, Option<Box<dyn std::any::Any + Send>>);

/// A step under way (or made inline): `join` gives back its state.
pub enum InFlight<T> {
    Done(Outcome<T>),
    Thread(mpsc::Receiver<Outcome<T>>),
}

fn run_step<T>(mut state: T, step: impl FnOnce(&mut T)) -> Outcome<T> {
    let t = Instant::now();
    let panic = catch_unwind(AssertUnwindSafe(|| step(&mut state))).err();
    (state, t.elapsed().as_secs_f64(), panic)
}

impl SimThread {
    pub fn spawn() -> std::io::Result<SimThread> {
        // (one job at a time: the slot holds the next while one runs, never more)
        let (tx, rx) = mpsc::sync_channel::<Job>(1);
        let handle = std::thread::Builder::new().name("sim".into()).spawn(move || {
            for job in rx {
                job();
            }
        })?;
        Ok(SimThread { jobs: Some(tx), handle: Some(handle) })
    }

    /// `step` on `state`, on the sim thread; `InFlight::join` gives the state back.
    pub fn start<T: Send + 'static>(&self, state: T, step: impl FnOnce(&mut T) + Send + 'static) -> InFlight<T> {
        let (tx, rx) = mpsc::sync_channel(1);
        let job: Job = Box::new(move || {
            let _ = tx.send(run_step(state, step));
        });
        match self.jobs.as_ref().map(|j| j.send(job)) {
            Some(Ok(())) => InFlight::Thread(rx),
            // (the thread is gone: the step runs here, as without the switch)
            Some(Err(mpsc::SendError(job))) => {
                job();
                InFlight::Thread(rx)
            }
            None => unreachable!("the sim thread's queue lives as long as it"),
        }
    }

    /// `step` on `state` right here, on the caller's thread: the same code without the thread.
    pub fn inline<T>(state: T, step: impl FnOnce(&mut T)) -> InFlight<T> {
        InFlight::Done(run_step(state, step))
    }
}

impl Drop for SimThread {
    fn drop(&mut self) {
        // the queue closed, the thread ends after its job (if any)
        self.jobs = None;
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl<T> InFlight<T> {
    /// The state back, once its step is made, with the seconds the step took and the
    /// seconds this call waited for it. A step that panicked panics here.
    pub fn join(self) -> (T, f64, f64) {
        let t = Instant::now();
        let (state, secs, panic) = match self {
            InFlight::Done(o) => o,
            InFlight::Thread(rx) => rx.recv().expect("the sim thread sends every step's state back"),
        };
        if let Some(p) = panic {
            resume_unwind(p);
        }
        (state, secs, t.elapsed().as_secs_f64())
    }
}

/// A tick's traffic step the traffic stage left to be made with the picture.
#[derive(Clone, Copy, Debug)]
struct OwedStep {
    dt: f32,
    player: Option<PlayerBox>,
}

/// The light programs' state as the scenery scripts read it: (`TrafficLightPhase`,
/// `TrafficLightApproach`) of a controller's light.
pub type PhaseOf<'a> = &'a dyn Fn(usize, usize) -> (f32, f32);

/// The scenery's step the scripted stage left to be made with the picture: the step with its
/// inputs (owned, moved in), given the lights' state to read.
type ScenerySteps = Box<dyn FnOnce(&mut SceneryState, PhaseOf) + Send>;

/// A tick's people's step the humans stage left to be made with the picture: its time, the
/// map as the step reads it and the player's bus as it stood (`PeopleSim::player_bus_view`).
struct OwedPeople {
    dt: f32,
    world: Arc<WorldData>,
    bus: Option<PlayerBusView>,
}

/// The people back from their ticks on the sim thread: whether a passenger took the printed
/// ticket in one of them (what `PeopleSim::tick_inner` returns), the ticks' time summed and
/// how long they took.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeopleStepped {
    pub took: bool,
    pub dt: f32,
    pub secs: f64,
}

/// What one job takes to the sim thread and brings back.
#[derive(Default)]
struct SimJob {
    traffic: Option<(Box<TrafficSim>, Vec<OwedStep>)>,
    people: Option<(PeopleSim, Vec<OwedPeople>, bool)>,
    scenery: Option<(SceneryState, ScenerySteps)>,
    /// Seconds of the traffic's step and of the scenery's (each part's own).
    traffic_secs: f64,
    people_secs: f64,
    scenery_secs: f64,
}

impl SimJob {
    /// The parts in the frame's order: tick by tick the traffic first, then the people (who
    /// read the traffic as its step left it, as the humans stage did after the traffic
    /// stage); after the ticks the scenery scripts, which read the lights so too. Without the
    /// traffic here there is none: the people see no traffic, its lights read unlinked.
    fn run(&mut self) {
        let ticks = self.traffic.as_ref().map_or(0, |(_, o)| o.len()).max(self.people.as_ref().map_or(0, |(_, o, _)| o.len()));
        for k in 0..ticks {
            let t = Instant::now();
            if let Some(owed) = self.traffic.as_mut().and_then(|(sim, owed)| owed.get(k).map(|o| (sim, o))) {
                owed.0.advance(owed.1.dt, owed.1.player);
            }
            let after_traffic = Instant::now();
            self.traffic_secs += (after_traffic - t).as_secs_f64();
            if let Some((people, owed, took)) = self.people.as_mut() {
                if let Some(owed) = owed.get(k) {
                    people.tick_stages.clear();
                    let traffic = self.traffic.as_ref().map(|(sim, _)| &**sim);
                    *took |= people.tick_inner(owed.dt, &*owed.world, owed.bus.as_ref(), traffic);
                }
            }
            self.people_secs += after_traffic.elapsed().as_secs_f64();
        }
        let mid = Instant::now();
        if let Some((state, step)) = self.scenery.take() {
            let mut state = state;
            let traffic = self.traffic.as_ref().map(|(sim, _)| &**sim);
            let phase = |c: usize, li: usize| traffic.map(|t| t.light_vars(c, li)).unwrap_or((crate::traffic::UNLINKED_PHASE as f32, 0.0));
            step(&mut state, &phase);
            self.scenery = Some((state, Box::new(|_, _| {})));
        }
        self.scenery_secs = mid.elapsed().as_secs_f64();
    }
}

/// What came back at a join: the seconds of each part's step there and of the wait here.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Joined {
    pub traffic: f64,
    pub people: f64,
    pub scenery: f64,
    pub waited: f64,
}

/// The window's pipeline: the sim thread (made the first time a step goes there) and what is
/// under way on it.
#[derive(Default)]
pub struct Pipeline {
    thread: Option<SimThread>,
    /// The traffic stage's steps (one a tick), while they wait for the picture.
    owed: Vec<OwedStep>,
    /// The scripted stage's step, while it waits for the picture.
    owed_scenery: Option<ScenerySteps>,
    /// The humans stage's steps (one a tick), while they wait for the picture.
    owed_people: Vec<OwedPeople>,
    /// The people came back from a step whose results the humans stage has not taken yet.
    people_stepped: Option<PeopleStepped>,
    /// The job out on the sim thread.
    out: Option<InFlight<SimJob>>,
    /// What stands in `Traffic::sim` while the traffic is out: an empty traffic, made once.
    stand_in: Option<Box<TrafficSim>>,
    /// The scenery came back from a step whose commands the view has not applied yet.
    scenery_stepped: bool,
    lag: DrawnLag,
    started: Option<Instant>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DrawnLag {
    now: f32,
    last: f32,
    span: f32,
    lag: f32,
}

impl DrawnLag {
    const SPAN: f32 = 0.25;
    const EASE: f32 = 0.1;

    pub(crate) fn note(&mut self, owed: f32, frame_secs: f32) {
        let frame_secs = frame_secs.max(0.0);
        self.now = self.now.max(owed);
        self.span += frame_secs;
        if self.span >= Self::SPAN {
            self.last = self.now;
            self.now = 0.0;
            self.span = 0.0;
        }
        let most = self.now.max(self.last);
        self.lag = if most >= self.lag { most } else { (self.lag - Self::EASE * frame_secs).max(most) };
    }

    pub(crate) fn behind(&self, behind: f32, owed: f32) -> f32 {
        behind + self.lag.max(owed) - owed
    }
}

static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the window's simulation steps go to the sim thread: decided once, as the game
/// starts (`set_enabled`), never in the middle of a session; off until then (the offscreen
/// run and the tests step inline).
pub fn enabled() -> bool {
    ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// The game's choice for the session to come: `OMSI_SIM_THREAD` when it is set (`0`,
/// `false`, `off` or `no`: inline; anything else: the sim thread), else `setting`.
pub fn set_enabled(setting: bool) -> bool {
    let on = choice(omsi_cfg::flags::OMSI_SIM_THREAD.var(), setting);
    ENABLED.store(on, std::sync::atomic::Ordering::Relaxed);
    on
}

/// `OMSI_SIM_THREAD`'s value over the setting (see `set_enabled`).
fn choice(var: Option<&str>, setting: bool) -> bool {
    match var {
        Some(v) => !matches!(v.trim().to_ascii_lowercase().as_str(), "0" | "false" | "off" | "no"),
        None => setting,
    }
}

/// An empty traffic: nothing on the road, no lanes. It stands in `Traffic::sim` while the real
/// one is stepped on the sim thread (nothing of the frame looks at the traffic then).
fn empty_traffic() -> TrafficSim {
    TrafficSim::assemble(
        std::path::Path::new(""),
        Default::default(),
        crate::ai_traffic::setup::RandomTypes { types: Vec::new(), groups: Vec::new(), group_curves: false, group_uvg: Vec::new(), uvg_defaults: Vec::new() },
        Vec::new(),
        Default::default(),
        (Vec::new(), Vec::new()),
        Vec::new(),
        (1.0, 0),
        0,
    )
}

impl Pipeline {
    /// Whether the traffic stage leaves the step of `sim` to the sim thread this tick: the
    /// switch is on and the traffic drives itself (a LAN client's only follows the host's,
    /// frame by frame, inline).
    pub fn takes_traffic(&self, sim: &TrafficSim) -> bool {
        enabled() && !sim.mirror
    }

    /// The traffic stage's step of a tick, left to be made with the picture (`dt` of the
    /// tick, the player's box as the stage saw it). The frame's ticks are made in their order.
    pub fn owe_traffic(&mut self, dt: f32, player: Option<PlayerBox>) {
        self.owed.push(OwedStep { dt, player });
    }

    pub fn traffic_behind(&self, behind: f32) -> f32 {
        self.lag.behind(behind, self.owed_secs())
    }

    fn owed_secs(&self) -> f32 {
        self.owed.iter().map(|o| o.dt).sum()
    }

    /// Whether the scripted stage leaves the scenery's step to the sim thread this frame: the
    /// scripts read the lights after the traffic's step, so only with the traffic's step
    /// going there too, or without a traffic (whose lights read unlinked anyway).
    pub fn takes_scenery(&self, traffic: Option<&TrafficSim>) -> bool {
        enabled() && traffic.is_none_or(|_| !self.owed.is_empty())
    }

    /// Whether the humans stage leaves the people's step to the sim thread this frame: as
    /// the scenery (after the traffic's step, or without a traffic), and not for a LAN
    /// client's people, who follow the host's.
    pub fn takes_people(&self, traffic: Option<&TrafficSim>, people: &PeopleSim) -> bool {
        enabled() && !people.mirror && traffic.is_none_or(|_| !self.owed.is_empty())
    }

    /// The humans stage's step of a tick: `dt`, the map as the step reads it, the player's
    /// bus as it stands now. Made after the same tick's traffic step (`owe_traffic`).
    pub fn owe_people(&mut self, dt: f32, world: Arc<WorldData>, bus: Option<PlayerBusView>) {
        self.owed_people.push(OwedPeople { dt, world, bus });
    }

    /// Rebind deferred passenger steps after a synchronous vehicle ownership change.
    /// Keep every step's elapsed time and world; only its player-slot snapshot changed.
    pub fn rebind_people_bus(&mut self, bus: Option<PlayerBusView>) {
        for step in &mut self.owed_people {
            step.bus = bus.clone();
        }
    }

    /// The people came back from a step on the sim thread whose results (the people who
    /// came and went, the exchange with the bus and the traffic) the humans stage applies
    /// now, before it owes the next one.
    pub fn take_people_stepped(&mut self) -> Option<PeopleStepped> {
        self.people_stepped.take()
    }

    /// A people's step is owed (the render stage hands the people over with it).
    pub fn owes_people(&self) -> bool {
        !self.owed_people.is_empty()
    }

    /// The scripted stage's step (its inputs moved into `step`), left to be made with the
    /// picture after the traffic's.
    pub fn owe_scenery(&mut self, step: impl FnOnce(&mut SceneryState, PhaseOf) + Send + 'static) {
        self.owed_scenery = Some(Box::new(step));
    }

    /// The scenery came back from the sim thread with commands not applied yet: the
    /// scripted stage applies them (`SceneryState::outputs`) before it owes the next step.
    pub fn take_scenery_stepped(&mut self) -> bool {
        std::mem::take(&mut self.scenery_stepped)
    }

    /// The steps owed, started on the sim thread with the traffic's simulation `sim` (an
    /// empty one stands in for it until `join`), the people (taken out of the frame by the
    /// caller, given back by `join`; with no people's step owed they are given back at
    /// once) and the scenery (an empty list meanwhile). Nothing owed, nothing starts.
    pub fn start(&mut self, sim: Option<&mut TrafficSim>, scenery: Option<&mut SceneryState>, people: Option<PeopleSim>) -> Option<PeopleSim> {
        if self.out.is_some() {
            return people;
        }
        let now = Instant::now();
        let frame_secs = self.started.replace(now).map_or(0.0, |t| (now - t).as_secs_f32());
        self.lag.note(self.owed_secs(), frame_secs);
        let mut job = SimJob::default();
        let owed = std::mem::take(&mut self.owed);
        if let (false, Some(sim)) = (owed.is_empty(), sim) {
            let mut out = self.stand_in.take().unwrap_or_else(|| Box::new(empty_traffic()));
            std::mem::swap(sim, &mut *out);
            job.traffic = Some((out, owed));
        }
        if let (Some(step), Some(scenery)) = (self.owed_scenery.take(), scenery) {
            job.scenery = Some((std::mem::take(scenery), step));
        }
        let mut people = people;
        if !self.owed_people.is_empty() {
            if let Some(p) = people.take() {
                job.people = Some((p, std::mem::take(&mut self.owed_people), false));
            }
        }
        if job.traffic.is_none() && job.scenery.is_none() && job.people.is_none() {
            return people;
        }
        if self.thread.is_none() {
            match SimThread::spawn() {
                Ok(th) => self.thread = Some(th),
                Err(e) => log::warn!("sim thread: not started ({e}); its steps run in the frame"),
            }
        }
        let run = |job: &mut SimJob| job.run();
        self.out = Some(match self.thread.as_ref() {
            Some(th) => th.start(job, run),
            None => SimThread::inline(job, run),
        });
        people
    }

    /// Everything out on the sim thread back in place (waits for it), the people given back
    /// to the caller. Nothing out: nothing.
    /// `sim`, `scenery`: where the traffic's simulation and the scenery stand in the frame.
    pub fn join(&mut self, sim: Option<&mut TrafficSim>, scenery: Option<&mut SceneryState>) -> (Joined, Option<PeopleSim>) {
        let Some(f) = self.out.take() else { return (Joined::default(), None) };
        let (job, _, waited) = f.join();
        if let Some((mut back, _)) = job.traffic {
            match sim {
                Some(sim) => {
                    std::mem::swap(sim, &mut *back);
                    self.stand_in = Some(back);
                }
                // (the traffic was let go meanwhile - nothing of the frame does that while it
                // is out, but should it, the stepped one is dropped with it)
                None => log::warn!("sim thread: the traffic went away while it was stepped"),
            }
        }
        if let Some((back, _)) = job.scenery {
            match scenery {
                Some(s) => {
                    *s = back;
                    self.scenery_stepped = true;
                }
                None => log::warn!("sim thread: the scenery went away while it was stepped"),
            }
        }
        let people = job.people.map(|(p, owed, took)| {
            let dt = owed.iter().map(|o| o.dt).sum();
            self.people_stepped = Some(PeopleStepped { took, dt, secs: job.people_secs });
            p
        });
        (Joined { traffic: job.traffic_secs, people: job.people_secs, scenery: job.scenery_secs, waited }, people)
    }

    /// Something is out on the sim thread.
    pub fn busy(&self) -> bool {
        self.out.is_some()
    }
}

#[cfg(test)]
mod tests;
