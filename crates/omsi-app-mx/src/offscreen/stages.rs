//! The offscreen run's step as a list of stages (`OFFSCREEN_STEP`, a tick of 1/60 s each),
//! taken by the same runner as the window's frame (see `frame_stages`).

use super::*;
use crate::frame_stages::{self, Flow, Group, Phase, Profile, Role, StageInfo};
use crate::profile_keys::{ProfKey, ProfileTimes};

/// The offscreen run's stages, in the order of [`OFFSCREEN_STEP`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OffscreenStage {
    Server,
    Clock,
    Traffic,
    Startup,
    Duty,
    Drive,
    GroundGap,
    Humans,
    Lan,
    Surroundings,
    Snapshot,
    Record,
}

/// One step of the offscreen run: the server's work, the traffic, the player's bus, the
/// people, the LAN, the spray and the snapshots due.
pub(crate) const OFFSCREEN_STEP: &[OffscreenStage] = {
    use OffscreenStage::*;
    &[Server, Clock, Traffic, Startup, Duty, Drive, GroundGap, Humans, Lan, Surroundings, Snapshot, Record]
};

const fn stage(key: ProfKey, phase: Phase, role: Role, reads: &'static [Group], writes: &'static [Group]) -> StageInfo {
    StageInfo { name: key.name(), phase, role, reads, writes, takes: &[], gives: &[], profile: Profile::Key(key) }
}

impl frame_stages::Stage for OffscreenStage {
    fn info(self) -> StageInfo {
        use Group as G;
        use OffscreenStage::*;
        use Phase as P;
        use ProfKey as K;
        match self {
            Server => stage(K::Server, P::Housekeeping, Role::Own, &[G::Net, G::Clock, G::Traffic, G::People], &[G::Net, G::Clock, G::Traffic, G::Gfx]),
            Clock => stage(K::Clock, P::Sim, Role::Clock, &[G::Clock, G::Weather, G::World], &[G::Clock, G::Player]),
            Traffic => stage(K::Traffic, P::Sim, Role::Traffic, &[G::Traffic, G::Player, G::People, G::World, G::View, G::Clock, G::Weather, G::Net], &[G::Traffic, G::Player, G::Gfx, G::Perf]),
            Startup => stage(K::Startup, P::Sim, Role::Own, &[G::Player], &[G::Player]),
            Duty => stage(K::Duty, P::Sim, Role::Duty, &[G::Duty, G::Player, G::People, G::Clock, G::World], &[G::Duty, G::Player, G::Perf]),
            Drive => stage(K::Drive, P::Sim, Role::Player, &[G::Player, G::Input, G::World, G::Traffic], &[G::Player, G::Traffic, G::Gfx, G::Perf]),
            GroundGap => stage(K::GroundGap, P::Sim, Role::Own, &[G::World, G::Player, G::Traffic, G::Clock], &[G::Perf]),
            Humans => stage(K::Humans, P::Sim, Role::People, &[G::People, G::Player, G::Traffic, G::Duty, G::Clock, G::World, G::View, G::Net], &[G::People, G::Player, G::Traffic, G::Duty, G::Gfx, G::Perf]),
            Lan => stage(K::Lan, P::Sim, Role::Lan, &[G::Net, G::Player, G::Clock, G::Duty, G::View], &[G::Net, G::Traffic, G::People, G::Gfx, G::Sound]),
            Surroundings => stage(K::Surroundings, P::Present, Role::Surroundings, &[G::Weather, G::Player, G::Traffic, G::Net, G::View, G::People], &[G::Weather]),
            Snapshot => stage(K::Snapshot, P::Output, Role::Picture, &[G::Gfx, G::View, G::Traffic, G::Weather], &[G::Gfx, G::Perf]),
            Record => stage(K::Record, P::Output, Role::Own, &[G::Gfx, G::View, G::Sound], &[G::Gfx, G::Perf]),
        }
    }
}

/// The offscreen run's `i`-th step as the runner takes it.
pub(super) struct Step<'r, 'a> {
    pub(super) run: &'r mut Offscreen<'a>,
    pub(super) i: usize,
}

impl frame_stages::Host<OffscreenStage> for Step<'_, '_> {
    fn run_stage(&mut self, stage: OffscreenStage) -> Result<Flow> {
        use OffscreenStage::*;
        let (run, i) = (&mut *self.run, self.i);
        let t_s = i as f32 * run.dt;
        match stage {
            Server => {
                if run.server && !run.server_step(i) {
                    return Ok(Flow::Stop);
                }
            }
            Clock => run.clock_step(t_s),
            Traffic => run.traffic_step(i, t_s)?,
            Startup => run.startup_step(),
            Duty => run.duty_step(),
            Drive => run.drive_stage(i),
            GroundGap => run.ground_gap_step(t_s),
            Humans => run.humans_step(i),
            Lan => run.lan_step(),
            Surroundings => run.surroundings_step(),
            Snapshot => run.snapshot_step(t_s)?,
            Record => run.record_step(i, t_s)?,
        }
        Ok(Flow::Go)
    }

    fn profile(&mut self) -> &mut ProfileTimes {
        &mut self.run.profile
    }
}
