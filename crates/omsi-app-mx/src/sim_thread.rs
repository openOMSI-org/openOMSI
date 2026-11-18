//! The window's side of the sim thread (`omsi_sim::sim_thread`, `OMSI_SIM_THREAD`): the
//! traffic stage owes its step of each tick (`Pipeline::owe_traffic`, in `road::traffic`), the
//! humans stage the people's (`Pipeline::owe_people`, in `people::humans`) and the scripted
//! stage the scenery scripts' once a frame (`Pipeline::owe_scenery`, in `scripted`), the
//! render stage starts it right before the picture and it is back right after the picture is
//! presented (`frame_render`), before the frame is counted or a session finished.

pub(crate) use omsi_sim::sim_thread::Pipeline;

impl crate::App {
    /// What the stages left to the sim thread, started there (right before the picture).
    pub(crate) fn start_sim_thread(&mut self) {
        // (the people go out whole while a step of theirs is owed: nothing of the picture
        // looks at them; they are back in place at the join)
        let people = if self.pipeline.owes_people() { self.session.humans.take().map(|h| h.sim) } else { None };
        let left = self.pipeline.start(self.session.traffic.as_mut().map(|t| &mut t.sim), Some(&mut self.session.scenery), people);
        if let Some(sim) = left {
            self.session.humans = Some(crate::humans::Humans { sim });
        }
    }

    /// Everything out on the sim thread back (right after the picture is presented; again at
    /// the end of the render stage, where it is nothing, for the frames drawing nothing).
    pub(crate) fn join_sim_thread(&mut self) {
        if self.pipeline.busy() {
            use crate::profile_keys::ProfKey;
            let (j, people) = self.pipeline.join(self.session.traffic.as_mut().map(|t| &mut t.sim), Some(&mut self.session.scenery));
            if let Some(sim) = people {
                if self.session.humans.is_some() {
                    log::warn!("sim thread: other people came while the people were stepped; those stepped are kept");
                }
                self.session.humans = Some(crate::humans::Humans { sim });
            }
            // (the steps' own time, off the frame's thread, and what the frame waited for them)
            self.perf.profile.add(ProfKey::SimThreadStep, j.traffic + j.people + j.scenery);
            self.perf.profile.add(ProfKey::SimThreadWait, j.waited);
        }
    }
}

#[cfg(test)]
mod tests {
    fn send<T: Send>() {}

    /// The game's wrappers of the simulation can go to the sim thread as they are (the
    /// compiler says so here, before a step needs it).
    #[test]
    fn sim_wrappers_are_send() {
        send::<crate::traffic::Traffic>();
        send::<crate::humans::Humans>();
        send::<crate::schedule::Schedule>();
        send::<crate::player::Player>();
    }
}
